//! 请求上下文模块
//!
//! 提供请求生命周期的上下文管理，封装通用初始化逻辑

use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy_core::api::config::{
    ResponseRuntimePolicy, ResponseTimeoutConfig, StreamingTimeoutConfig,
};
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::{
    selected_provider_display_name_for_error, selected_provider_missing_from_source_message,
    selected_provider_not_applied_message, unselected_provider_fallback_id,
};
use crate::proxy_core::api::ports::ProxyServices;
use crate::proxy_core::api::session::extract_session_id_with_generator;
use crate::proxy_core::api::transforms::claude_api_format_from_metadata;
use crate::proxy_core::api::transport::{
    request_model_for_forward, resolve_response_runtime_policy, ProxyResult,
};
use crate::proxy_core::api::usage::{usage_route_context_from_selection, UsageRouteContext};
use crate::proxy_core_adapter::{
    app_proxy_config_from_proxy_app_config, provider_claude_api_format,
};
use axum::http::HeaderMap;
use std::time::Instant;

#[derive(Debug, Clone)]
struct RequestContextRouteUpdate {
    outbound_model: Option<String>,
    usage_route_context: UsageRouteContext,
    provider: Provider,
}

#[derive(Debug)]
enum RequestContextRouteUpdateError<E> {
    ProviderLoad(E),
    ProviderMissing(String),
}

fn request_context_route_update_from_proxy_result(
    app_type: &AppType,
    provider: &Provider,
    result: &ProxyResult,
) -> RequestContextRouteUpdate {
    RequestContextRouteUpdate {
        outbound_model: result.outbound_model.clone(),
        usage_route_context: usage_route_context_from_selection(&result.selected_route),
        provider: ForwardAttempt::from_core_selection(app_type, provider, &result.selected_route)
            .provider()
            .clone(),
    }
}

fn request_context_route_update_from_proxy_result_source<E>(
    app_type: &AppType,
    app_type_str: &str,
    result: &ProxyResult,
    source_name: &str,
    load_provider: impl FnOnce(&str, &str) -> Result<Option<Provider>, E>,
) -> Result<RequestContextRouteUpdate, RequestContextRouteUpdateError<E>> {
    let provider_id = result.selected_route.provider.id.as_str();
    let provider = load_provider(provider_id, app_type_str)
        .map_err(RequestContextRouteUpdateError::ProviderLoad)?;
    let Some(provider) = provider else {
        return Err(RequestContextRouteUpdateError::ProviderMissing(
            selected_provider_missing_from_source_message(provider_id, source_name),
        ));
    };

    Ok(request_context_route_update_from_proxy_result(
        app_type, &provider, result,
    ))
}

/// 请求上下文
///
/// 贯穿整个请求生命周期，包含：
/// - 计时信息
/// - 响应处理运行时策略（per-app）
/// - 选中的 Provider（用于错误和转换兼容语义）
/// - 请求模型名称
/// - 日志标签
/// - Session ID（用于日志关联）
pub struct RequestContext {
    /// 请求开始时间
    pub start_time: Instant,
    /// 响应处理运行时策略（per-app，包含重试次数和超时配置）
    pub response_runtime_policy: ResponseRuntimePolicy,
    /// ProxyEngine 成功选路后的 Provider。
    provider: Option<Provider>,
    /// 请求中的模型名称
    pub request_model: String,
    /// 实际发往上游的模型名（路由接管/模型映射后的真值，forward 成功后回填）。
    ///
    /// usage 归因的兜底顺序：上游响应回显 → outbound_model → request_model。
    /// 不能直接用 request_model 兜底：接管场景下它是映射前的客户端别名。
    pub outbound_model: Option<String>,
    /// 选中的代理 channel，用于 usage 明细归因。
    pub usage_route_context: Option<UsageRouteContext>,
    /// 日志标签（如 "Claude"、"Codex"、"Gemini"）
    pub tag: &'static str,
    /// 应用类型字符串（如 "claude"、"codex"、"gemini"）
    pub app_type_str: &'static str,
    /// 应用类型，用于 ProxyEngine 路由结果回填时按 app 加载 Provider。
    pub app_type: AppType,
    /// Session ID（从客户端请求提取或新生成）
    pub session_id: String,
}

impl RequestContext {
    /// 创建请求上下文
    ///
    /// # Arguments
    /// * `state` - 代理服务器状态
    /// * `body` - 请求体 JSON
    /// * `headers` - 请求头（用于提取 Session ID）
    /// * `app_type` - 应用类型
    /// * `tag` - 日志标签
    /// * `app_type_str` - 应用类型字符串
    ///
    /// # Errors
    /// 返回 `ProxyError` 如果 Provider 选择失败
    pub async fn new(
        state: &ProxyState,
        body: &serde_json::Value,
        headers: &HeaderMap,
        app_type: AppType,
        tag: &'static str,
        app_type_str: &'static str,
    ) -> Result<Self, ProxyError> {
        let start_time = Instant::now();

        let app_kind = AppKind::from(app_type.as_str());
        let core_app_config = state
            .proxy_core_services
            .config()
            .load_app(&app_kind)
            .await
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
        let app_config = app_proxy_config_from_proxy_app_config(&core_app_config)
            .map_err(ProxyError::ConfigError)?;
        let response_runtime_policy = resolve_response_runtime_policy(
            app_config.auto_failover_enabled,
            app_config.max_retries,
            app_config.non_streaming_timeout as u64,
            app_config.streaming_first_byte_timeout as u64,
            app_config.streaming_idle_timeout as u64,
        );

        let request_model =
            request_model_for_forward(&app_kind, "", body).unwrap_or_else(|| "unknown".to_string());

        // 提取 Session ID
        let session_result = extract_session_id_with_generator(headers, body, app_type_str, || {
            uuid::Uuid::new_v4().to_string()
        });
        let session_id = session_result.session_id.clone();

        log::debug!(
            "[{}] Session ID: {} (from {:?}, client_provided: {})",
            tag,
            session_id,
            session_result.source,
            session_result.client_provided
        );

        log::debug!(
            "[{}] Request model: {}, session: {}",
            tag,
            request_model,
            session_id
        );

        Ok(Self {
            start_time,
            response_runtime_policy,
            provider: None,
            request_model,
            outbound_model: None,
            usage_route_context: None,
            tag,
            app_type_str,
            app_type,
            session_id,
        })
    }

    /// 从 URI 提取模型名称（Gemini 专用）
    ///
    /// Gemini API 的模型名称在 URI 中，格式如：
    /// `/v1beta/models/gemini-pro:generateContent`
    pub fn with_model_from_uri(mut self, uri: &axum::http::Uri) -> Self {
        // 用 path() 而不是 path_and_query()：模型名必须从路径段中解析，
        // 否则 GET /v1beta/models/<id>?key=... 会把 query 拼到 request_model 上。
        let endpoint = uri.path();

        self.request_model =
            request_model_for_forward(&AppKind::Gemini, endpoint, &serde_json::Value::Null)
                .unwrap_or_else(|| "unknown".to_string());

        self
    }

    pub fn apply_proxy_result(
        &mut self,
        state: &ProxyState,
        result: &ProxyResult,
    ) -> Result<(), ProxyError> {
        let update = request_context_route_update_from_proxy_result_source(
            &self.app_type,
            self.app_type_str,
            result,
            "host database",
            |provider_id, app_type| state.db.get_provider_by_id(provider_id, app_type),
        )
        .map_err(|error| match error {
            RequestContextRouteUpdateError::ProviderLoad(error) => {
                ProxyError::DatabaseError(error.to_string())
            }
            RequestContextRouteUpdateError::ProviderMissing(message) => {
                ProxyError::ConfigError(message)
            }
        })?;
        self.outbound_model = update.outbound_model;
        self.usage_route_context = Some(update.usage_route_context);
        self.provider = Some(update.provider);
        Ok(())
    }

    pub fn provider(&self) -> Result<&Provider, ProxyError> {
        self.provider.as_ref().ok_or_else(|| {
            ProxyError::ConfigError(selected_provider_not_applied_message(self.app_type_str))
        })
    }

    pub fn provider_for_usage(&self) -> Option<&Provider> {
        self.provider.as_ref()
    }

    pub fn provider_name_for_error(&self) -> &str {
        selected_provider_display_name_for_error(
            self.provider
                .as_ref()
                .map(|provider| provider.name.as_str()),
            self.tag,
        )
    }

    pub fn fallback_provider_id(&self) -> String {
        unselected_provider_fallback_id(self.app_type_str)
    }

    pub fn claude_api_format_for_proxy_result(
        &self,
        result: &ProxyResult,
    ) -> Result<String, ProxyError> {
        Ok(claude_api_format_from_metadata(
            &result.metadata,
            provider_claude_api_format(self.provider()?),
        ))
    }

    /// 计算请求延迟（毫秒）
    #[inline]
    pub fn latency_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// 获取流式超时配置
    ///
    /// 配置生效规则：
    /// - 故障转移开启：返回配置的值（0 表示禁用超时检查）
    /// - 故障转移关闭：返回 0（禁用超时检查）
    #[inline]
    pub fn streaming_timeout_config(&self) -> StreamingTimeoutConfig {
        self.response_timeout_config().streaming
    }

    #[inline]
    pub fn response_timeout_config(&self) -> ResponseTimeoutConfig {
        self.response_runtime_policy().timeout
    }

    #[inline]
    pub fn response_runtime_policy(&self) -> ResponseRuntimePolicy {
        self.response_runtime_policy
    }

    #[inline]
    pub fn body_timeout_duration(&self) -> std::time::Duration {
        self.response_timeout_config().body_timeout_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::api::domain::{
        ChannelHealthPolicy, ChannelOverrides, ChannelSpec, ProviderKind, ProviderMetadata,
        ProviderSpec, RetryPolicy, UpstreamEndpoint,
    };
    use crate::proxy_core::api::routing::{
        route_selection_from_parts, ChannelStatus, InterfaceKind,
    };
    use crate::proxy_core::api::transport::{ProxyCoreResponse, ProxyResult};
    use serde_json::{json, Value};

    fn context_with_provider(provider: Option<Provider>) -> RequestContext {
        RequestContext {
            start_time: Instant::now(),
            response_runtime_policy: ResponseRuntimePolicy::default(),
            provider,
            request_model: "client-model".to_string(),
            outbound_model: None,
            usage_route_context: None,
            tag: "Codex",
            app_type_str: "codex",
            app_type: AppType::Codex,
            session_id: "session-1".to_string(),
        }
    }

    #[test]
    fn provider_accessors_use_fallback_before_route_result() {
        let ctx = context_with_provider(None);

        assert!(matches!(ctx.provider(), Err(ProxyError::ConfigError(_))));
        assert!(ctx.provider_for_usage().is_none());
        assert_eq!(ctx.provider_name_for_error(), "Codex");
        assert_eq!(ctx.fallback_provider_id(), "unselected:codex");
    }

    #[test]
    fn provider_accessors_use_selected_provider_after_route_result() {
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            serde_json::json!({}),
            None,
        );
        let ctx = context_with_provider(Some(provider));

        assert_eq!(ctx.provider().expect("selected provider").id, "provider-a");
        assert_eq!(
            ctx.provider_for_usage()
                .map(|provider| provider.id.as_str()),
            Some("provider-a")
        );
        assert_eq!(ctx.provider_name_for_error(), "Provider A");
        assert_eq!(ctx.fallback_provider_id(), "unselected:codex");
    }

    fn selection(
        channel_id: &str,
        provider_id: &str,
    ) -> crate::proxy_core::api::routing::RouteSelection {
        let provider = ProviderSpec {
            id: provider_id.to_string(),
            name: provider_id.to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        };
        let channel = ChannelSpec {
            id: channel_id.to_string(),
            provider_id: provider_id.to_string(),
            app: AppKind::Claude,
            name: channel_id.to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: "https://api.example.com".to_string(),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::OpenAiChatCompletions,
            auth_profile: None,
            models: Vec::new(),
            groups: vec!["default".to_string()],
            priority: 0,
            weight: 100,
            retry_policy: RetryPolicy {
                raw: Value::Object(Default::default()),
            },
            health_policy: ChannelHealthPolicy {
                raw: Value::Object(Default::default()),
            },
            overrides: ChannelOverrides {
                headers: Value::Object(Default::default()),
                params: Value::Object(Default::default()),
                status_code_mapping: Value::Array(Vec::new()),
                model_mapping: Value::Object(Default::default()),
            },
            tags: Vec::new(),
            metadata: Value::Object(Default::default()),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        };

        route_selection_from_parts(
            provider,
            channel,
            None,
            InterfaceKind::OpenAiChatCompletions,
        )
    }

    #[test]
    fn route_update_uses_proxy_result_selection_after_forwarding() {
        let selected = selection("ch-b", "provider-b");
        let result = ProxyResult {
            response: ProxyCoreResponse::empty(http::StatusCode::OK),
            selected_route: selected,
            outbound_model: Some("upstream-sonnet".to_string()),
            usage_record: None,
            metadata: json!({}),
        };
        let selected_host_provider = Provider::with_id(
            "provider-b".to_string(),
            "Provider B".to_string(),
            json!({}),
            None,
        );

        let update = request_context_route_update_from_proxy_result(
            &AppType::Claude,
            &selected_host_provider,
            &result,
        );
        assert_eq!(update.outbound_model.as_deref(), Some("upstream-sonnet"));
        assert_eq!(update.usage_route_context.channel_id, "ch-b");
        assert_eq!(update.provider.id, "provider-b");

        let sourced_update = request_context_route_update_from_proxy_result_source(
            &AppType::Claude,
            AppType::Claude.as_str(),
            &result,
            "host database",
            |provider_id, app_type| {
                assert_eq!(provider_id, "provider-b");
                assert_eq!(app_type, AppType::Claude.as_str());
                Ok::<_, String>(Some(selected_host_provider.clone()))
            },
        )
        .expect("sourced route update");
        assert_eq!(
            sourced_update.outbound_model.as_deref(),
            Some("upstream-sonnet")
        );
        assert_eq!(sourced_update.usage_route_context.channel_id, "ch-b");
        assert_eq!(sourced_update.provider.id, "provider-b");

        let missing_sourced_update = request_context_route_update_from_proxy_result_source(
            &AppType::Claude,
            AppType::Claude.as_str(),
            &result,
            "host database",
            |_provider_id, _app_type| Ok::<_, String>(None),
        )
        .expect_err("missing provider");
        assert!(matches!(
            missing_sourced_update,
            RequestContextRouteUpdateError::ProviderMissing(message)
                if message == "selected provider is missing from host database: provider-b"
        ));

        let load_error = request_context_route_update_from_proxy_result_source(
            &AppType::Claude,
            AppType::Claude.as_str(),
            &result,
            "host database",
            |_provider_id, _app_type| Err::<Option<Provider>, _>("db failed".to_string()),
        )
        .expect_err("load error");
        assert!(matches!(
            load_error,
            RequestContextRouteUpdateError::ProviderLoad(message) if message == "db failed"
        ));
    }
}
