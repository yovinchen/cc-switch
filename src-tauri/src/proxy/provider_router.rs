//! 供应商路由器模块
//!
//! 负责选择和管理代理目标供应商，实现智能故障转移

use crate::app_config::AppType;
use crate::database::{Database, ProxyChannelMigrationPreview, ProxyChannelRecord};
use crate::error::AppError;
use crate::provider::Provider;
use crate::proxy::circuit_breaker::{AllowResult, CircuitBreaker, CircuitBreakerStats};
use crate::proxy_core::{
    ChannelRouteSource, CircuitBreakerConfig, ProviderSelectionCandidate, ProviderSelectionFailure,
    ProviderSelectionInput, ProxyCoreError, RouteResolveRequest, RouteResolveResponse,
    app_type_from_circuit_key, channel_circuit_key, channel_circuit_key_prefix,
    circuit_breaker_config_from_app_config, circuit_failure_threshold_from_app_config,
    provider_circuit_key, provider_circuit_key_prefix, reject_unavailable_channel_ids,
    resolve_channel_route as resolve_core_channel_route, select_provider_ids,
};
use crate::proxy_core_adapter::proxy_channel_route_inputs_to_core;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 供应商路由器
pub struct ProviderRouter {
    /// 数据库连接
    db: Arc<Database>,
    /// 熔断器管理器 - provider key: "app_type:provider_id", channel key: "channel:app_type:channel_id"
    circuit_breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,
}

impl ProviderRouter {
    /// 创建新的供应商路由器
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 选择可用的供应商（支持故障转移）
    ///
    /// 返回按优先级排序的可用供应商列表：
    /// - 故障转移关闭时：仅返回当前供应商
    /// - 故障转移开启时：仅使用故障转移队列，按队列顺序依次尝试（P1 → P2 → ...）
    pub async fn select_providers(&self, app_type: &str) -> Result<Vec<Provider>, AppError> {
        // 检查该应用的自动故障转移开关是否开启（从 proxy_config 表读取）
        let auto_failover_enabled = match self.db.get_proxy_config_for_app(app_type).await {
            Ok(config) => config.auto_failover_enabled,
            Err(e) => {
                log::error!("[{app_type}] 读取 proxy_config 失败: {e}，默认禁用故障转移");
                false
            }
        };

        let result = if auto_failover_enabled {
            self.select_failover_providers(app_type).await
        } else {
            self.select_current_provider(app_type)
        }?;

        Ok(result)
    }

    async fn select_failover_providers(&self, app_type: &str) -> Result<Vec<Provider>, AppError> {
        // 故障转移开启：仅按队列顺序依次尝试（P1 → P2 → ...）
        let all_providers = self.db.get_all_providers(app_type)?;

        // 使用 DAO 返回的排序结果，确保和前端展示一致
        let ordered_ids: Vec<String> = self
            .db
            .get_failover_queue(app_type)?
            .into_iter()
            .map(|item| item.provider_id)
            .collect();

        let mut candidates = Vec::with_capacity(ordered_ids.len());
        for provider_id in &ordered_ids {
            let Some(provider) = all_providers.get(provider_id) else {
                candidates.push(ProviderSelectionCandidate::new(provider_id, false, true));
                continue;
            };

            let circuit_key = provider_circuit_key(app_type, &provider.id);
            let breaker = self.get_or_create_circuit_breaker(&circuit_key).await;
            candidates.push(ProviderSelectionCandidate::new(
                provider_id,
                true,
                breaker.is_available().await,
            ));
        }

        let selected_ids = select_provider_ids(ProviderSelectionInput::failover(candidates))
            .map_err(|error| provider_selection_failure_to_app_error(app_type, error))?;

        Ok(selected_ids
            .into_iter()
            .filter_map(|provider_id| all_providers.get(&provider_id).cloned())
            .collect())
    }

    fn select_current_provider(&self, app_type: &str) -> Result<Vec<Provider>, AppError> {
        // 故障转移关闭：仅使用当前供应商，跳过熔断器检查
        let current_id = AppType::from_str(app_type)
            .ok()
            .and_then(|app_enum| {
                crate::settings::get_effective_current_provider(&self.db, &app_enum)
                    .ok()
                    .flatten()
            })
            .or_else(|| self.db.get_current_provider(app_type).ok().flatten());

        let current = current_id
            .and_then(|current_id| {
                self.db
                    .get_provider_by_id(&current_id, app_type)
                    .transpose()
            })
            .transpose()?;

        let selected_ids = select_provider_ids(ProviderSelectionInput::current(
            current.as_ref().map(|provider| provider.id.clone()),
        ))
        .map_err(|error| provider_selection_failure_to_app_error(app_type, error))?;

        Ok(selected_ids
            .into_iter()
            .filter_map(|provider_id| {
                current
                    .as_ref()
                    .filter(|provider| provider.id == provider_id)
                    .cloned()
            })
            .collect())
    }

    /// List routable channels for an app without changing the forwarding path.
    ///
    /// Materialized proxy_channels win. If the migration table is still empty,
    /// fall back to a live projection from legacy providers/provider_endpoints.
    pub async fn list_channels_for_app(
        &self,
        app_type: &str,
    ) -> Result<(Vec<ProxyChannelRecord>, ChannelRouteSource), AppError> {
        let channels = self.db.list_proxy_channels_for_app(app_type)?;
        if !channels.is_empty() {
            return Ok((channels, ChannelRouteSource::MaterializedChannels));
        }

        let preview: ProxyChannelMigrationPreview =
            self.db.preview_legacy_proxy_channel_migration(app_type)?;
        Ok((preview.channels, ChannelRouteSource::LegacyProjection))
    }

    /// Resolve a dry-run channel route for management API/debugging.
    ///
    /// This does not allocate circuit-breaker permits and does not mutate
    /// current provider state.
    pub async fn resolve_channel_route_dry_run(
        &self,
        request: RouteResolveRequest,
    ) -> Result<RouteResolveResponse, AppError> {
        let (channels, source) = self.list_channels_for_app(&request.app_type).await?;
        let mut response = resolve_core_channel_route(
            request,
            proxy_channel_route_inputs_to_core(channels),
            source,
        )
        .map_err(proxy_core_error_to_app_error)?;
        let unavailable_channel_ids = self.unavailable_route_candidate_ids(&response).await;
        reject_unavailable_channel_ids(&mut response, unavailable_channel_ids);
        Ok(response)
    }

    async fn unavailable_route_candidate_ids(
        &self,
        response: &RouteResolveResponse,
    ) -> Vec<String> {
        let mut unavailable_channel_ids = Vec::new();

        for candidate in &response.candidates {
            let circuit_key = channel_circuit_key(&response.app_type, &candidate.channel_id);
            let is_available = match self.get_existing_circuit_breaker(&circuit_key).await {
                Some(breaker) => breaker.is_available().await,
                None => true,
            };

            if !is_available {
                unavailable_channel_ids.push(candidate.channel_id.clone());
            }
        }

        unavailable_channel_ids
    }

    /// 请求执行前获取熔断器“放行许可”
    ///
    /// - Closed：直接放行
    /// - Open：超时到达后切到 HalfOpen 并放行一次探测
    /// - HalfOpen：按限流规则放行探测
    ///
    /// 注意：调用方必须在请求结束后通过 `record_result()` 释放 HalfOpen 名额，
    /// 否则会导致该 Provider 长时间无法进入探测状态。
    pub async fn allow_provider_request(&self, provider_id: &str, app_type: &str) -> AllowResult {
        let circuit_key = provider_circuit_key(app_type, provider_id);
        let breaker = self.get_or_create_circuit_breaker(&circuit_key).await;
        breaker.allow_request().await
    }

    /// 请求执行前获取 Channel 熔断器“放行许可”
    pub async fn allow_channel_request(&self, channel_id: &str, app_type: &str) -> AllowResult {
        let circuit_key = channel_circuit_key(app_type, channel_id);
        let breaker = self.get_or_create_circuit_breaker(&circuit_key).await;
        breaker.allow_request().await
    }

    /// 记录供应商请求结果
    pub async fn record_result(
        &self,
        provider_id: &str,
        app_type: &str,
        used_half_open_permit: bool,
        success: bool,
        error_msg: Option<String>,
    ) -> Result<(), AppError> {
        // 1. 按应用独立获取熔断器配置
        let failure_threshold = self.failure_threshold_for_app(app_type, 5).await;

        // 2. 更新熔断器状态
        let circuit_key = provider_circuit_key(app_type, provider_id);
        let breaker = self.get_or_create_circuit_breaker(&circuit_key).await;

        if success {
            breaker.record_success(used_half_open_permit).await;
        } else {
            breaker.record_failure(used_half_open_permit).await;
        }

        // 3. 更新数据库健康状态（使用配置的阈值）
        self.db
            .update_provider_health_with_threshold(
                provider_id,
                app_type,
                success,
                error_msg.clone(),
                failure_threshold,
            )
            .await?;

        Ok(())
    }

    /// 记录 Channel 请求结果
    pub async fn record_channel_result(
        &self,
        channel_id: &str,
        app_type: &str,
        used_half_open_permit: bool,
        success: bool,
        error_msg: Option<String>,
        response_time_ms: Option<i64>,
    ) -> Result<(), AppError> {
        let failure_threshold = self
            .failure_threshold_for_app(app_type, CircuitBreakerConfig::default().failure_threshold)
            .await;
        let circuit_key = channel_circuit_key(app_type, channel_id);
        let breaker = self.get_or_create_circuit_breaker(&circuit_key).await;

        if success {
            breaker.record_success(used_half_open_permit).await;
        } else {
            breaker.record_failure(used_half_open_permit).await;
        }

        self.db.update_proxy_channel_health_with_threshold(
            channel_id,
            success,
            error_msg,
            failure_threshold,
            response_time_ms,
        )?;

        Ok(())
    }

    /// 重置熔断器（手动恢复）
    pub async fn reset_circuit_breaker(&self, circuit_key: &str) {
        let breakers = self.circuit_breakers.read().await;
        if let Some(breaker) = breakers.get(circuit_key) {
            breaker.reset().await;
        }
    }

    /// 重置指定供应商的熔断器
    pub async fn reset_provider_breaker(&self, provider_id: &str, app_type: &str) {
        let circuit_key = provider_circuit_key(app_type, provider_id);
        self.reset_circuit_breaker(&circuit_key).await;
    }

    /// 重置指定 Channel 的熔断器和健康状态
    pub async fn reset_channel_breaker(
        &self,
        channel_id: &str,
        app_type: &str,
    ) -> Result<(), AppError> {
        let circuit_key = channel_circuit_key(app_type, channel_id);
        self.reset_circuit_breaker(&circuit_key).await;
        self.db.reset_proxy_channel_health(channel_id)
    }

    /// 仅释放 HalfOpen permit，不影响健康统计（neutral 接口）
    ///
    /// 用于整流器等场景：请求结果不应计入 Provider 健康度，
    /// 但仍需释放占用的探测名额，避免 HalfOpen 状态卡死
    pub async fn release_permit_neutral(
        &self,
        provider_id: &str,
        app_type: &str,
        used_half_open_permit: bool,
    ) {
        if !used_half_open_permit {
            return;
        }
        let circuit_key = provider_circuit_key(app_type, provider_id);
        let breaker = self.get_or_create_circuit_breaker(&circuit_key).await;
        breaker.release_half_open_permit();
    }

    /// 仅释放 Channel HalfOpen permit，不影响健康统计。
    pub async fn release_channel_permit_neutral(
        &self,
        channel_id: &str,
        app_type: &str,
        used_half_open_permit: bool,
    ) {
        if !used_half_open_permit {
            return;
        }
        let circuit_key = channel_circuit_key(app_type, channel_id);
        let breaker = self.get_or_create_circuit_breaker(&circuit_key).await;
        breaker.release_half_open_permit();
    }

    /// 更新所有熔断器的配置（热更新）
    pub async fn update_all_configs(&self, config: CircuitBreakerConfig) {
        let breakers = self.circuit_breakers.read().await;
        for breaker in breakers.values() {
            breaker.update_config(config.clone()).await;
        }
    }

    /// 更新指定应用已创建熔断器的配置（热更新）
    pub async fn update_app_configs(&self, app_type: &str, config: CircuitBreakerConfig) {
        let provider_prefix = provider_circuit_key_prefix(app_type);
        let channel_prefix = channel_circuit_key_prefix(app_type);
        let breakers = self.circuit_breakers.read().await;
        for (key, breaker) in breakers.iter() {
            if key.starts_with(&provider_prefix) || key.starts_with(&channel_prefix) {
                breaker.update_config(config.clone()).await;
            }
        }
    }

    /// 获取熔断器状态
    #[allow(dead_code)]
    pub async fn get_circuit_breaker_stats(
        &self,
        provider_id: &str,
        app_type: &str,
    ) -> Option<CircuitBreakerStats> {
        let circuit_key = provider_circuit_key(app_type, provider_id);
        let breakers = self.circuit_breakers.read().await;

        if let Some(breaker) = breakers.get(&circuit_key) {
            Some(breaker.get_stats().await)
        } else {
            None
        }
    }

    /// 获取 Channel 熔断器状态
    #[allow(dead_code)]
    pub async fn get_channel_circuit_breaker_stats(
        &self,
        channel_id: &str,
        app_type: &str,
    ) -> Option<CircuitBreakerStats> {
        let circuit_key = channel_circuit_key(app_type, channel_id);
        let breakers = self.circuit_breakers.read().await;

        if let Some(breaker) = breakers.get(&circuit_key) {
            Some(breaker.get_stats().await)
        } else {
            None
        }
    }

    /// 获取或创建熔断器
    async fn get_or_create_circuit_breaker(&self, key: &str) -> Arc<CircuitBreaker> {
        // 先尝试读锁获取
        {
            let breakers = self.circuit_breakers.read().await;
            if let Some(breaker) = breakers.get(key) {
                return breaker.clone();
            }
        }

        // 如果不存在，获取写锁创建
        let mut breakers = self.circuit_breakers.write().await;

        // 双重检查，防止竞争条件
        if let Some(breaker) = breakers.get(key) {
            return breaker.clone();
        }

        let app_type = app_type_from_circuit_key(key);

        // 按应用独立读取熔断器配置
        let app_config = self.db.get_proxy_config_for_app(app_type).await.ok();
        let config = circuit_breaker_config_from_app_config(app_config.as_ref());

        let breaker = Arc::new(CircuitBreaker::new(config));
        breakers.insert(key.to_string(), breaker.clone());

        breaker
    }

    async fn get_existing_circuit_breaker(&self, key: &str) -> Option<Arc<CircuitBreaker>> {
        let breakers = self.circuit_breakers.read().await;
        breakers.get(key).cloned()
    }

    async fn failure_threshold_for_app(&self, app_type: &str, fallback: u32) -> u32 {
        let app_config = self.db.get_proxy_config_for_app(app_type).await.ok();
        circuit_failure_threshold_from_app_config(app_config.as_ref(), fallback)
    }
}

fn proxy_core_error_to_app_error(error: ProxyCoreError) -> AppError {
    match error {
        ProxyCoreError::Config(message) => AppError::Config(message),
        ProxyCoreError::InvalidRequest(message) => AppError::InvalidInput(message),
        other => AppError::Message(other.to_string()),
    }
}

fn provider_selection_failure_to_app_error(
    app_type: &str,
    error: ProviderSelectionFailure,
) -> AppError {
    match error {
        ProviderSelectionFailure::AllProvidersCircuitOpen => {
            log::warn!("[{app_type}] [FO-004] 所有供应商均已熔断");
            AppError::AllProvidersCircuitOpen
        }
        ProviderSelectionFailure::NoProvidersConfigured => {
            log::warn!("[{app_type}] [FO-005] 未配置供应商");
            AppError::NoProvidersConfigured
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::proxy::circuit_breaker::CircuitState;
    use crate::proxy_core::{ChannelRouteSource, RouteResolveRequest};
    use crate::settings::CustomEndpoint;
    use serde_json::json;
    use serial_test::serial;
    use std::collections::HashMap;
    use std::env;
    use tempfile::TempDir;

    struct TempHome {
        #[allow(dead_code)]
        dir: TempDir,
        original_home: Option<String>,
        original_userprofile: Option<String>,
        original_test_home: Option<String>,
    }

    impl TempHome {
        fn new() -> Self {
            let dir = TempDir::new().expect("failed to create temp home");
            let original_home = env::var("HOME").ok();
            let original_userprofile = env::var("USERPROFILE").ok();
            let original_test_home = env::var("CC_SWITCH_TEST_HOME").ok();

            env::set_var("HOME", dir.path());
            env::set_var("USERPROFILE", dir.path());
            env::set_var("CC_SWITCH_TEST_HOME", dir.path());
            crate::settings::reload_settings().expect("reload settings");

            Self {
                dir,
                original_home,
                original_userprofile,
                original_test_home,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match &self.original_home {
                Some(value) => env::set_var("HOME", value),
                None => env::remove_var("HOME"),
            }

            match &self.original_userprofile {
                Some(value) => env::set_var("USERPROFILE", value),
                None => env::remove_var("USERPROFILE"),
            }

            match &self.original_test_home {
                Some(value) => env::set_var("CC_SWITCH_TEST_HOME", value),
                None => env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_provider_router_creation() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());
        let router = ProviderRouter::new(db);

        let breaker = router.get_or_create_circuit_breaker("claude:test").await;
        assert!(breaker.allow_request().await.allowed);
    }

    #[tokio::test]
    #[serial]
    async fn test_failover_disabled_uses_current_provider() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());

        let provider_a =
            Provider::with_id("a".to_string(), "Provider A".to_string(), json!({}), None);
        let provider_b =
            Provider::with_id("b".to_string(), "Provider B".to_string(), json!({}), None);

        db.save_provider("claude", &provider_a).unwrap();
        db.save_provider("claude", &provider_b).unwrap();
        db.set_current_provider("claude", "a").unwrap();
        db.add_to_failover_queue("claude", "b").unwrap();

        let router = ProviderRouter::new(db.clone());
        let providers = router.select_providers("claude").await.unwrap();

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "a");
    }

    #[tokio::test]
    #[serial]
    async fn route_dry_run_uses_legacy_projection_before_materialization() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());
        let mut provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        let mut endpoints = HashMap::new();
        endpoints.insert(
            "https://backup.example.com/v1".to_string(),
            CustomEndpoint {
                url: "https://backup.example.com/v1".to_string(),
                added_at: 1,
                last_used: None,
            },
        );
        provider.meta = Some(crate::provider::ProviderMeta {
            custom_endpoints: endpoints,
            ..crate::provider::ProviderMeta::default()
        });
        db.save_provider("claude", &provider).unwrap();
        db.set_current_provider("claude", "a").unwrap();

        let router = ProviderRouter::new(db);
        let response = router
            .resolve_channel_route_dry_run(RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            })
            .await
            .unwrap();

        assert_eq!(response.source, ChannelRouteSource::LegacyProjection);
        assert_eq!(response.candidates.len(), 2);
        assert!(response.rejected.is_empty());
        assert_eq!(response.candidates[0].provider_id, "a");
    }

    #[tokio::test]
    #[serial]
    async fn route_dry_run_prefers_materialized_channels() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();
        db.materialize_legacy_proxy_channels("claude").unwrap();

        let router = ProviderRouter::new(db);
        let response = router
            .resolve_channel_route_dry_run(RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            })
            .await
            .unwrap();

        assert_eq!(response.source, ChannelRouteSource::MaterializedChannels);
        assert_eq!(response.candidates.len(), 1);
        assert_eq!(
            response.candidates[0].base_url,
            "https://primary.example.com/v1"
        );
    }

    #[tokio::test]
    #[serial]
    async fn route_dry_run_filters_open_channel_breakers() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();
        db.materialize_legacy_proxy_channels("claude").unwrap();

        let mut config = db.get_proxy_config_for_app("claude").await.unwrap();
        config.circuit_failure_threshold = 1;
        config.circuit_timeout_seconds = 60;
        db.update_proxy_config_for_app(config).await.unwrap();

        let channels = db.list_proxy_channels_for_app("claude").unwrap();
        let channel_id = channels[0].id.clone();

        let router = ProviderRouter::new(db.clone());
        router
            .record_channel_result(
                &channel_id,
                "claude",
                false,
                false,
                Some("upstream failed".to_string()),
                Some(123),
            )
            .await
            .unwrap();

        let stats = router
            .get_channel_circuit_breaker_stats(&channel_id, "claude")
            .await
            .expect("channel stats");
        assert_eq!(stats.state, CircuitState::Open);

        let health = db.get_proxy_channel_health(&channel_id).unwrap();
        assert_eq!(health.status, "unhealthy");
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.response_time_ms, Some(123));

        let blocked = router
            .resolve_channel_route_dry_run(RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            })
            .await
            .unwrap();

        assert!(blocked.candidates.is_empty());
        assert!(blocked.rejected.iter().any(|rejected| {
            rejected.channel_id == channel_id
                && rejected
                    .reasons
                    .iter()
                    .any(|reason| reason == "circuit_open")
        }));
        assert!(
            !router
                .allow_channel_request(&channel_id, "claude")
                .await
                .allowed
        );

        router
            .reset_channel_breaker(&channel_id, "claude")
            .await
            .unwrap();
        let reset_health = db.get_proxy_channel_health(&channel_id).unwrap();
        assert_eq!(reset_health.status, "unknown");

        let recovered = router
            .resolve_channel_route_dry_run(RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            })
            .await
            .unwrap();
        assert_eq!(recovered.candidates.len(), 1);
        assert!(recovered.rejected.is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn test_failover_enabled_uses_queue_order_ignoring_current() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());

        // 设置 sort_index 来控制顺序：b=1, a=2
        let mut provider_a =
            Provider::with_id("a".to_string(), "Provider A".to_string(), json!({}), None);
        provider_a.sort_index = Some(2);
        let mut provider_b =
            Provider::with_id("b".to_string(), "Provider B".to_string(), json!({}), None);
        provider_b.sort_index = Some(1);

        db.save_provider("claude", &provider_a).unwrap();
        db.save_provider("claude", &provider_b).unwrap();
        db.set_current_provider("claude", "a").unwrap();

        db.add_to_failover_queue("claude", "b").unwrap();
        db.add_to_failover_queue("claude", "a").unwrap();

        // 启用自动故障转移（使用新的 proxy_config API）
        let mut config = db.get_proxy_config_for_app("claude").await.unwrap();
        config.auto_failover_enabled = true;
        db.update_proxy_config_for_app(config).await.unwrap();

        let router = ProviderRouter::new(db.clone());
        let providers = router.select_providers("claude").await.unwrap();

        assert_eq!(providers.len(), 2);
        // 故障转移开启时：仅按队列顺序选择（忽略当前供应商）
        assert_eq!(providers[0].id, "b");
        assert_eq!(providers[1].id, "a");
    }

    #[tokio::test]
    #[serial]
    async fn test_failover_enabled_uses_queue_only_even_if_current_not_in_queue() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());

        let provider_a =
            Provider::with_id("a".to_string(), "Provider A".to_string(), json!({}), None);
        let mut provider_b =
            Provider::with_id("b".to_string(), "Provider B".to_string(), json!({}), None);
        provider_b.sort_index = Some(1);

        db.save_provider("claude", &provider_a).unwrap();
        db.save_provider("claude", &provider_b).unwrap();
        db.set_current_provider("claude", "a").unwrap();

        // 只把 b 加入故障转移队列（模拟“当前供应商不在队列里”的常见配置）
        db.add_to_failover_queue("claude", "b").unwrap();

        let mut config = db.get_proxy_config_for_app("claude").await.unwrap();
        config.auto_failover_enabled = true;
        db.update_proxy_config_for_app(config).await.unwrap();

        let router = ProviderRouter::new(db.clone());
        let providers = router.select_providers("claude").await.unwrap();

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "b");
    }

    #[tokio::test]
    #[serial]
    async fn test_select_providers_does_not_consume_half_open_permit() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());

        db.update_circuit_breaker_config(&CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_seconds: 0,
            ..Default::default()
        })
        .await
        .unwrap();

        let provider_a =
            Provider::with_id("a".to_string(), "Provider A".to_string(), json!({}), None);
        let provider_b =
            Provider::with_id("b".to_string(), "Provider B".to_string(), json!({}), None);

        db.save_provider("claude", &provider_a).unwrap();
        db.save_provider("claude", &provider_b).unwrap();

        db.add_to_failover_queue("claude", "a").unwrap();
        db.add_to_failover_queue("claude", "b").unwrap();

        // 启用自动故障转移（使用新的 proxy_config API）
        let mut config = db.get_proxy_config_for_app("claude").await.unwrap();
        config.auto_failover_enabled = true;
        db.update_proxy_config_for_app(config).await.unwrap();

        let router = ProviderRouter::new(db.clone());

        router
            .record_result("b", "claude", false, false, Some("fail".to_string()))
            .await
            .unwrap();

        let providers = router.select_providers("claude").await.unwrap();
        assert_eq!(providers.len(), 2);

        assert!(router.allow_provider_request("b", "claude").await.allowed);
    }

    #[tokio::test]
    #[serial]
    async fn test_release_permit_neutral_frees_half_open_slot() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());

        // 配置熔断器：1 次失败即熔断，0 秒超时立即进入 HalfOpen
        db.update_circuit_breaker_config(&CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_seconds: 0,
            ..Default::default()
        })
        .await
        .unwrap();

        let provider_a =
            Provider::with_id("a".to_string(), "Provider A".to_string(), json!({}), None);
        db.save_provider("claude", &provider_a).unwrap();
        db.add_to_failover_queue("claude", "a").unwrap();

        // 启用自动故障转移
        let mut config = db.get_proxy_config_for_app("claude").await.unwrap();
        config.auto_failover_enabled = true;
        db.update_proxy_config_for_app(config).await.unwrap();

        let router = ProviderRouter::new(db.clone());

        // 触发熔断：1 次失败
        router
            .record_result("a", "claude", false, false, Some("fail".to_string()))
            .await
            .unwrap();

        // 第一次请求：获取 HalfOpen 探测名额
        let first = router.allow_provider_request("a", "claude").await;
        assert!(first.allowed);
        assert!(first.used_half_open_permit);

        // 第二次请求应被拒绝（名额已被占用）
        let second = router.allow_provider_request("a", "claude").await;
        assert!(!second.allowed);

        // 使用 release_permit_neutral 释放名额（不影响健康统计）
        router
            .release_permit_neutral("a", "claude", first.used_half_open_permit)
            .await;

        // 第三次请求应被允许（名额已释放）
        let third = router.allow_provider_request("a", "claude").await;
        assert!(third.allowed);
        assert!(third.used_half_open_permit);
    }
}
