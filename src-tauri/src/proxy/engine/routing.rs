//! 供应商路由器模块
//!
//! 负责选择和管理代理目标供应商，实现智能故障转移

use crate::error::AppError;
use crate::proxy::circuit_breaker::CircuitBreaker;
use crate::proxy_core_adapter::{
    app_type_from_circuit_key, channel_circuit_key, channel_circuit_key_prefix,
    channel_health_reset_from_parts, effective_channel_health_failure_threshold,
    provider_circuit_key, provider_circuit_key_prefix,
    select_failover_provider_ids_from_router_lookup_availability, AllowResult,
    ChannelAttemptResult, ChannelHealthReset, ChannelRouteSource, CircuitBreakerConfig,
    CircuitBreakerStats, ProviderFailoverCircuitLookup, RouteCandidateCircuitKey,
    RouteResolveChannelInput,
};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub(crate) struct ProviderFailoverRouterSources {
    pub(crate) provider_ids: Vec<String>,
    pub(crate) lookups: Vec<ProviderFailoverCircuitLookup>,
}

pub(crate) trait ProviderRouterConfigSource: Send + Sync {
    fn load_failover_enabled<'a>(&'a self, app_type: &'a str) -> BoxFuture<'a, bool>;

    fn circuit_breaker_config<'a>(
        &'a self,
        app_type: &'a str,
    ) -> BoxFuture<'a, CircuitBreakerConfig>;

    fn failure_threshold<'a>(&'a self, app_type: &'a str, fallback: u32) -> BoxFuture<'a, u32>;
}

pub(crate) trait ProviderRouterProviderSource: Send + Sync {
    fn failover_sources<'a>(
        &'a self,
        app_type: &'a str,
    ) -> BoxFuture<'a, Result<ProviderFailoverRouterSources, AppError>>;

    fn current_provider_ids<'a>(
        &'a self,
        app_type: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>, AppError>>;
}

pub(crate) trait ProviderRouterChannelSource: Send + Sync {
    fn channel_route_inputs<'a>(
        &'a self,
        app_type: &'a str,
    ) -> BoxFuture<'a, Result<(Vec<RouteResolveChannelInput>, ChannelRouteSource), AppError>>;
}

pub(crate) trait ProviderRouterHealthStore: Send + Sync {
    fn record_provider_health<'a>(
        &'a self,
        provider_id: &'a str,
        app_type: &'a str,
        success: bool,
        error_msg: Option<String>,
        failure_threshold: u32,
    ) -> BoxFuture<'a, Result<(), AppError>>;

    fn record_channel_health<'a>(
        &'a self,
        result: ChannelAttemptResult,
    ) -> BoxFuture<'a, Result<(), AppError>>;

    fn reset_channel_health(&self, reset: ChannelHealthReset) -> Result<(), AppError>;
}

pub(crate) struct ProviderRouterSources {
    config: Arc<dyn ProviderRouterConfigSource>,
    providers: Arc<dyn ProviderRouterProviderSource>,
    channels: Arc<dyn ProviderRouterChannelSource>,
    health: Arc<dyn ProviderRouterHealthStore>,
}

impl ProviderRouterSources {
    pub(crate) fn new(
        config: Arc<dyn ProviderRouterConfigSource>,
        providers: Arc<dyn ProviderRouterProviderSource>,
        channels: Arc<dyn ProviderRouterChannelSource>,
        health: Arc<dyn ProviderRouterHealthStore>,
    ) -> Self {
        Self {
            config,
            providers,
            channels,
            health,
        }
    }
}

/// 供应商路由器
pub struct ProviderRouter {
    /// Host-provided provider/channel/config/health source.
    sources: ProviderRouterSources,
    /// Runtime-owned live circuit state.
    circuit_runtime: ProviderRoutingCircuitRuntime,
}

impl ProviderRouter {
    pub(crate) fn with_sources(sources: ProviderRouterSources) -> Self {
        let circuit_runtime = ProviderRoutingCircuitRuntime::new(sources.config.clone());
        Self {
            sources,
            circuit_runtime,
        }
    }

    /// 选择可用的供应商（支持故障转移）
    ///
    /// 返回按优先级排序的可用供应商列表：
    /// - 故障转移关闭时：仅返回当前供应商
    /// - 故障转移开启时：仅使用故障转移队列，按队列顺序依次尝试（P1 → P2 → ...）
    pub async fn select_provider_ids(&self, app_type: &str) -> Result<Vec<String>, AppError> {
        // 检查该应用的自动故障转移开关是否开启（从 proxy_config 表读取）
        let auto_failover_enabled = self.sources.config.load_failover_enabled(app_type).await;

        let result = if auto_failover_enabled {
            self.select_failover_provider_ids(app_type).await
        } else {
            self.select_current_provider_ids(app_type).await
        }?;

        Ok(result)
    }

    async fn select_failover_provider_ids(&self, app_type: &str) -> Result<Vec<String>, AppError> {
        // 故障转移开启：仅按队列顺序依次尝试（P1 → P2 → ...）
        let sources = self.sources.providers.failover_sources(app_type).await?;
        let mut lookup_availability = Vec::with_capacity(sources.lookups.len());
        for lookup in sources.lookups {
            let available = match lookup.circuit_key.as_ref() {
                Some(circuit_key) => self.circuit_runtime.is_available(circuit_key).await,
                None => true,
            };
            lookup_availability.push((lookup, available));
        }

        select_failover_provider_ids_from_router_lookup_availability(
            app_type,
            &sources.provider_ids,
            lookup_availability,
        )
    }

    async fn select_current_provider_ids(&self, app_type: &str) -> Result<Vec<String>, AppError> {
        // 故障转移关闭：仅使用当前供应商，跳过熔断器检查
        self.sources.providers.current_provider_ids(app_type).await
    }

    /// List routable channels for an app without changing the forwarding path.
    ///
    /// Materialized proxy_channels win. If the migration table is still empty,
    /// fall back to a live projection from legacy providers/provider_endpoints.
    pub async fn list_route_channel_inputs_for_app(
        &self,
        app_type: &str,
    ) -> Result<(Vec<RouteResolveChannelInput>, ChannelRouteSource), AppError> {
        self.sources.channels.channel_route_inputs(app_type).await
    }

    /// Query live Channel circuit breaker availability for management dry-run candidates.
    pub(crate) async fn route_candidate_circuit_availability(
        &self,
        lookups: impl IntoIterator<Item = RouteCandidateCircuitKey>,
    ) -> Vec<(RouteCandidateCircuitKey, bool)> {
        let mut availability = Vec::new();

        for lookup in lookups {
            let is_available = self
                .circuit_runtime
                .existing_is_available_or_default(&lookup.circuit_key)
                .await;
            availability.push((lookup, is_available));
        }

        availability
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
        self.circuit_runtime.allow_request(&circuit_key).await
    }

    /// 请求执行前获取 Channel 熔断器“放行许可”
    pub async fn allow_channel_request(&self, channel_id: &str, app_type: &str) -> AllowResult {
        let circuit_key = channel_circuit_key(app_type, channel_id);
        let config = self
            .channel_circuit_breaker_config_for_app(channel_id, app_type)
            .await;
        self.circuit_runtime
            .allow_request_with_config(&circuit_key, config)
            .await
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
        self.circuit_runtime
            .record_result(&circuit_key, used_half_open_permit, success)
            .await;

        // 3. 更新数据库健康状态（使用配置的阈值）
        self.sources
            .health
            .record_provider_health(provider_id, app_type, success, error_msg, failure_threshold)
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
        let circuit_config = self
            .channel_circuit_breaker_config_for_app(channel_id, app_type)
            .await;
        let failure_threshold = circuit_config.failure_threshold;
        let circuit_key = channel_circuit_key(app_type, channel_id);
        self.circuit_runtime
            .record_result_with_config(&circuit_key, circuit_config, used_half_open_permit, success)
            .await;

        self.sources
            .health
            .record_channel_health(ChannelAttemptResult {
                channel_id: channel_id.to_string(),
                success,
                status_code: None,
                latency_ms: response_time_ms.and_then(|latency| u64::try_from(latency).ok()),
                failure_threshold: Some(failure_threshold),
                error_code: error_msg,
            })
            .await?;

        Ok(())
    }

    /// 重置熔断器（手动恢复）
    pub async fn reset_circuit_breaker(&self, circuit_key: &str) {
        self.circuit_runtime.reset(circuit_key).await;
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
        self.sources
            .health
            .reset_channel_health(channel_health_reset_from_parts(channel_id, app_type))
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
        self.circuit_runtime
            .release_half_open_permit(&circuit_key)
            .await;
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
        self.circuit_runtime
            .release_half_open_permit(&circuit_key)
            .await;
    }

    /// 更新所有熔断器的配置（热更新）
    pub async fn update_all_configs(&self, config: CircuitBreakerConfig) {
        self.circuit_runtime.update_all_configs(config).await;
    }

    /// 更新指定应用已创建熔断器的配置（热更新）
    pub async fn update_app_configs(&self, app_type: &str, config: CircuitBreakerConfig) {
        self.circuit_runtime
            .update_app_configs(app_type, config)
            .await;
    }

    /// 获取熔断器状态
    pub async fn get_circuit_breaker_stats(
        &self,
        provider_id: &str,
        app_type: &str,
    ) -> Option<CircuitBreakerStats> {
        let circuit_key = provider_circuit_key(app_type, provider_id);
        self.circuit_runtime.stats(&circuit_key).await
    }

    /// 获取 Channel 熔断器状态
    pub async fn get_channel_circuit_breaker_stats(
        &self,
        channel_id: &str,
        app_type: &str,
    ) -> Option<CircuitBreakerStats> {
        let circuit_key = channel_circuit_key(app_type, channel_id);
        self.circuit_runtime.stats(&circuit_key).await
    }

    async fn failure_threshold_for_app(&self, app_type: &str, fallback: u32) -> u32 {
        self.sources
            .config
            .failure_threshold(app_type, fallback)
            .await
    }

    async fn channel_circuit_breaker_config_for_app(
        &self,
        channel_id: &str,
        app_type: &str,
    ) -> CircuitBreakerConfig {
        let mut config = self.sources.config.circuit_breaker_config(app_type).await;
        let Ok((channels, _source)) = self.sources.channels.channel_route_inputs(app_type).await
        else {
            return config;
        };

        if let Some(channel) = channels
            .iter()
            .find(|channel| channel.channel_id == channel_id)
        {
            config.failure_threshold = effective_channel_health_failure_threshold(
                config.failure_threshold,
                &channel.health_policy,
            );
        }

        config
    }
}

struct ProviderRoutingCircuitRuntime {
    config: Arc<dyn ProviderRouterConfigSource>,
    breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,
}

impl ProviderRoutingCircuitRuntime {
    fn new(config: Arc<dyn ProviderRouterConfigSource>) -> Self {
        Self {
            config,
            breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn is_available(&self, key: &str) -> bool {
        let breaker = self.get_or_create_circuit_breaker(key).await;
        breaker.is_available().await
    }

    async fn existing_is_available_or_default(&self, key: &str) -> bool {
        match self.get_existing_circuit_breaker(key).await {
            Some(breaker) => breaker.is_available().await,
            None => true,
        }
    }

    async fn allow_request(&self, key: &str) -> AllowResult {
        let breaker = self.get_or_create_circuit_breaker(key).await;
        breaker.allow_request().await
    }

    async fn allow_request_with_config(
        &self,
        key: &str,
        config: CircuitBreakerConfig,
    ) -> AllowResult {
        let breaker = self
            .get_or_create_circuit_breaker_with_config(key, config)
            .await;
        breaker.allow_request().await
    }

    async fn record_result(&self, key: &str, used_half_open_permit: bool, success: bool) {
        let breaker = self.get_or_create_circuit_breaker(key).await;

        if success {
            breaker.record_success(used_half_open_permit).await;
        } else {
            breaker.record_failure(used_half_open_permit).await;
        }
    }

    async fn record_result_with_config(
        &self,
        key: &str,
        config: CircuitBreakerConfig,
        used_half_open_permit: bool,
        success: bool,
    ) {
        let breaker = self
            .get_or_create_circuit_breaker_with_config(key, config)
            .await;

        if success {
            breaker.record_success(used_half_open_permit).await;
        } else {
            breaker.record_failure(used_half_open_permit).await;
        }
    }

    async fn reset(&self, key: &str) {
        if let Some(breaker) = self.get_existing_circuit_breaker(key).await {
            breaker.reset().await;
        }
    }

    async fn release_half_open_permit(&self, key: &str) {
        let breaker = self.get_or_create_circuit_breaker(key).await;
        breaker.release_half_open_permit();
    }

    async fn update_all_configs(&self, config: CircuitBreakerConfig) {
        let breakers = self.breakers.read().await;
        for breaker in breakers.values() {
            breaker.update_config(config.clone()).await;
        }
    }

    async fn update_app_configs(&self, app_type: &str, config: CircuitBreakerConfig) {
        let provider_prefix = provider_circuit_key_prefix(app_type);
        let channel_prefix = channel_circuit_key_prefix(app_type);
        let breakers = self.breakers.read().await;
        for (key, breaker) in breakers.iter() {
            if key.starts_with(&provider_prefix) || key.starts_with(&channel_prefix) {
                breaker.update_config(config.clone()).await;
            }
        }
    }

    async fn stats(&self, key: &str) -> Option<CircuitBreakerStats> {
        match self.get_existing_circuit_breaker(key).await {
            Some(breaker) => Some(breaker.get_stats().await),
            None => None,
        }
    }

    /// 获取或创建熔断器
    async fn get_or_create_circuit_breaker(&self, key: &str) -> Arc<CircuitBreaker> {
        // 先尝试读锁获取
        {
            let breakers = self.breakers.read().await;
            if let Some(breaker) = breakers.get(key) {
                return breaker.clone();
            }
        }

        let app_type = app_type_from_circuit_key(key);
        let config = self.config.circuit_breaker_config(app_type).await;

        // 如果不存在，获取写锁创建
        let mut breakers = self.breakers.write().await;

        // 双重检查，防止竞争条件
        if let Some(breaker) = breakers.get(key) {
            return breaker.clone();
        }

        let breaker = Arc::new(CircuitBreaker::new(config));
        breakers.insert(key.to_string(), breaker.clone());

        breaker
    }

    async fn get_or_create_circuit_breaker_with_config(
        &self,
        key: &str,
        config: CircuitBreakerConfig,
    ) -> Arc<CircuitBreaker> {
        if let Some(breaker) = self.get_existing_circuit_breaker(key).await {
            breaker.update_config(config).await;
            return breaker;
        }

        let mut breakers = self.breakers.write().await;

        if let Some(breaker) = breakers.get(key).cloned() {
            drop(breakers);
            breaker.update_config(config).await;
            return breaker;
        }

        let breaker = Arc::new(CircuitBreaker::new(config));
        breakers.insert(key.to_string(), breaker.clone());

        breaker
    }

    async fn get_existing_circuit_breaker(&self, key: &str) -> Option<Arc<CircuitBreaker>> {
        let breakers = self.breakers.read().await;
        breakers.get(key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::provider::Provider;
    use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
    use crate::proxy_core_adapter::{
        management_route_response_from_router_source, ChannelRouteSource, CircuitState,
        ProxyChannelWriteRequest, RouteResolveRequest,
    };
    use crate::settings::CustomEndpoint;
    use serde_json::json;
    use serial_test::serial;
    use std::collections::HashMap;
    use std::env;
    use tempfile::TempDir;

    struct TempHome {
        _dir: TempDir,
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
                _dir: dir,
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
        let router = provider_router_from_database(db);

        assert!(
            router
                .allow_provider_request("test", "claude")
                .await
                .allowed
        );
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

        let router = provider_router_from_database(db.clone());
        let provider_ids = router.select_provider_ids("claude").await.unwrap();

        assert_eq!(provider_ids, vec!["a"]);
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

        let router = provider_router_from_database(db);
        let response = management_route_response_from_router_source(
            &router,
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
        )
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

        let router = provider_router_from_database(db);
        let response = management_route_response_from_router_source(
            &router,
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
        )
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

        let router = provider_router_from_database(db.clone());
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

        let blocked = management_route_response_from_router_source(
            &router,
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
        )
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

        let recovered = management_route_response_from_router_source(
            &router,
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(recovered.candidates.len(), 1);
        assert!(recovered.rejected.is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn channel_health_policy_overrides_persisted_failure_threshold() {
        let _home = TempHome::new();
        let db = Arc::new(Database::memory().unwrap());
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-key" } }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();

        let mut config = db.get_proxy_config_for_app("claude").await.unwrap();
        config.circuit_failure_threshold = 1;
        db.update_proxy_config_for_app(config).await.unwrap();

        let channel_id = db
            .create_proxy_channel(ProxyChannelWriteRequest {
                id: Some("channel-health-policy".to_string()),
                provider_id: "provider-a".to_string(),
                app_type: "claude".to_string(),
                name: "Channel Health Policy".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                health_policy: json!({"failureThreshold": 2}),
                ..Default::default()
            })
            .unwrap()
            .id;

        let router = provider_router_from_database(db.clone());
        router
            .record_channel_result(
                &channel_id,
                "claude",
                false,
                false,
                Some("first failure".to_string()),
                None,
            )
            .await
            .unwrap();

        let degraded = db.get_proxy_channel_health(&channel_id).unwrap();
        assert_eq!(degraded.status, "degraded");
        assert_eq!(degraded.consecutive_failures, 1);
        let closed_stats = router
            .get_channel_circuit_breaker_stats(&channel_id, "claude")
            .await
            .expect("channel stats after first failure");
        assert_eq!(closed_stats.state, CircuitState::Closed);
        assert!(
            router
                .allow_channel_request(&channel_id, "claude")
                .await
                .allowed
        );

        router
            .record_channel_result(
                &channel_id,
                "claude",
                false,
                false,
                Some("second failure".to_string()),
                None,
            )
            .await
            .unwrap();

        let unhealthy = db.get_proxy_channel_health(&channel_id).unwrap();
        assert_eq!(unhealthy.status, "unhealthy");
        assert_eq!(unhealthy.consecutive_failures, 2);
        let open_stats = router
            .get_channel_circuit_breaker_stats(&channel_id, "claude")
            .await
            .expect("channel stats after second failure");
        assert_eq!(open_stats.state, CircuitState::Open);
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

        let router = provider_router_from_database(db.clone());
        let provider_ids = router.select_provider_ids("claude").await.unwrap();

        assert_eq!(provider_ids.len(), 2);
        // 故障转移开启时：仅按队列顺序选择（忽略当前供应商）
        assert_eq!(provider_ids, vec!["b", "a"]);
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

        let router = provider_router_from_database(db.clone());
        let provider_ids = router.select_provider_ids("claude").await.unwrap();

        assert_eq!(provider_ids, vec!["b"]);
    }

    #[tokio::test]
    #[serial]
    async fn test_select_provider_ids_does_not_consume_half_open_permit() {
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

        let router = provider_router_from_database(db.clone());

        router
            .record_result("b", "claude", false, false, Some("fail".to_string()))
            .await
            .unwrap();

        let stats = router
            .get_circuit_breaker_stats("b", "claude")
            .await
            .expect("provider stats");
        assert_eq!(stats.state, CircuitState::Open);
        assert_eq!(stats.failed_requests, 1);

        let provider_ids = router.select_provider_ids("claude").await.unwrap();
        assert_eq!(provider_ids.len(), 2);

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

        let router = provider_router_from_database(db.clone());

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
