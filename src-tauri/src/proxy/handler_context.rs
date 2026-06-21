//! 请求上下文模块
//!
//! 提供请求生命周期的上下文管理，封装通用初始化逻辑

use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::{
    error::ProxyError, providers::get_claude_api_format, server::ProxyState,
};
use crate::proxy_core_adapter::{
    app_proxy_config_from_proxy_app_config, claude_api_format_from_metadata,
    extract_proxy_session_id, request_context_route_update_from_proxy_result,
    request_model_from_body_for_context, request_model_from_gemini_path_for_context,
    response_runtime_policy_from_app_proxy_config, ProxyCoreAppKind as AppKind, ProxyResult,
    ProxyServices, ResponseRuntimePolicy, ResponseTimeoutConfig, StreamingTimeoutConfig,
    UsageRouteContext,
};
use axum::http::HeaderMap;
use std::time::Instant;

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
    /// 应用类型（预留，目前通过 app_type_str 使用）
    #[allow(dead_code)]
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

        let app_kind = AppKind::from(&app_type);
        let core_app_config = state
            .proxy_core_services
            .config()
            .load_app(&app_kind)
            .await
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
        let app_config = app_proxy_config_from_proxy_app_config(&core_app_config)
            .map_err(ProxyError::ConfigError)?;
        let response_runtime_policy = response_runtime_policy_from_app_proxy_config(&app_config);

        let request_model = request_model_from_body_for_context(&app_type, body);

        // 提取 Session ID
        let session_result = extract_proxy_session_id(headers, body, app_type_str);
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

        self.request_model = request_model_from_gemini_path_for_context(endpoint);

        self
    }

    pub fn apply_proxy_result(
        &mut self,
        state: &ProxyState,
        result: &ProxyResult,
    ) -> Result<(), ProxyError> {
        let provider_id = result.selected_route.provider.id.as_str();
        let Some(provider) = state
            .db
            .get_provider_by_id(provider_id, self.app_type_str)
            .map_err(|error| ProxyError::DatabaseError(error.to_string()))?
        else {
            return Err(ProxyError::ConfigError(format!(
                "selected provider is missing from host database: {provider_id}"
            )));
        };

        let update =
            request_context_route_update_from_proxy_result(&self.app_type, &provider, result);
        self.outbound_model = update.outbound_model;
        self.usage_route_context = Some(update.usage_route_context);
        self.provider = Some(update.provider);
        Ok(())
    }

    pub fn provider(&self) -> Result<&Provider, ProxyError> {
        self.provider.as_ref().ok_or_else(|| {
            ProxyError::ConfigError(format!(
                "selected provider is not available before route result is applied: {}",
                self.app_type_str
            ))
        })
    }

    pub fn provider_for_usage(&self) -> Option<&Provider> {
        self.provider.as_ref()
    }

    pub fn provider_name_for_error(&self) -> &str {
        self.provider
            .as_ref()
            .map(|provider| provider.name.as_str())
            .unwrap_or(self.tag)
    }

    pub fn fallback_provider_id(&self) -> String {
        format!("unselected:{}", self.app_type_str)
    }

    pub fn claude_api_format_for_proxy_result(
        &self,
        result: &ProxyResult,
    ) -> Result<String, ProxyError> {
        Ok(claude_api_format_from_metadata(
            &result.metadata,
            get_claude_api_format(self.provider()?),
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
            ctx.provider_for_usage().map(|provider| provider.id.as_str()),
            Some("provider-a")
        );
        assert_eq!(ctx.provider_name_for_error(), "Provider A");
        assert_eq!(ctx.fallback_provider_id(), "unselected:codex");
    }
}
