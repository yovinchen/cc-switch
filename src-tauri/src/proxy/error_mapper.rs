//! 错误类型到 HTTP 状态码的映射
//!
//! 将 ProxyError 映射到合适的 HTTP 状态码，用于日志记录和手动构建错误响应

use super::ProxyError;
use crate::proxy_core::{CodexProxyErrorContext, ProxyCoreError};
use serde_json::Value;

/// 将 ProxyError 映射到 HTTP 状态码
///
/// 映射规则：
/// - 上游错误：直接使用上游返回的状态码
/// - 超时：504 Gateway Timeout
/// - 连接失败：502 Bad Gateway
/// - 无可用 Provider：503 Service Unavailable
/// - 重试耗尽：503 Service Unavailable
/// - 认证错误：401 Unauthorized
/// - 配置/请求错误：400 Bad Request
/// - 转换错误：422 Unprocessable Entity
/// - 其他错误：500 Internal Server Error
pub fn map_proxy_error_to_status(error: &ProxyError) -> u16 {
    match error {
        // 服务状态错误：与 IntoResponse 保持一致
        ProxyError::AlreadyRunning => 409,
        ProxyError::NotRunning => 503,

        // 上游错误：使用实际状态码
        ProxyError::UpstreamError { status, .. } => *status,

        // 超时错误：504 Gateway Timeout
        ProxyError::Timeout(_) | ProxyError::StreamIdleTimeout(_) => 504,

        // 转发失败/连接失败：502 Bad Gateway
        ProxyError::ForwardFailed(_) => 502,

        // 无可用 Provider：503 Service Unavailable
        ProxyError::NoAvailableProvider => 503,

        // 所有供应商已熔断：503 Service Unavailable
        ProxyError::AllProvidersCircuitOpen => 503,

        // 未配置供应商：503 Service Unavailable
        ProxyError::NoProvidersConfigured => 503,

        // 重试耗尽：503 Service Unavailable
        ProxyError::MaxRetriesExceeded => 503,

        // Provider 不健康：503 Service Unavailable
        ProxyError::ProviderUnhealthy(_) => 503,

        // 配置错误/无效请求：400 Bad Request
        ProxyError::ConfigError(_) | ProxyError::InvalidRequest(_) => 400,

        // 认证错误：401 Unauthorized
        ProxyError::AuthError(_) => 401,

        // 数据库错误：500 Internal Server Error
        ProxyError::DatabaseError(_) => 500,

        // 转换错误：422 Unprocessable Entity
        ProxyError::TransformError(_) => 422,

        // 其他未知错误：500 Internal Server Error
        _ => 500,
    }
}

/// 将 ProxyError 转换为用户友好的错误消息
pub fn get_error_message(error: &ProxyError) -> String {
    match error {
        ProxyError::UpstreamError { status, body } => {
            if let Some(body) = body {
                format!("上游错误 ({status}): {body}")
            } else {
                format!("上游错误 ({status})")
            }
        }
        ProxyError::Timeout(msg) => format!("请求超时: {msg}"),
        ProxyError::ForwardFailed(msg) => format!("转发失败: {msg}"),
        ProxyError::NoAvailableProvider => "无可用 Provider".to_string(),
        ProxyError::AllProvidersCircuitOpen => "所有供应商已熔断，无可用渠道".to_string(),
        ProxyError::NoProvidersConfigured => "未配置供应商".to_string(),
        ProxyError::MaxRetriesExceeded => "所有 Provider 都失败，重试耗尽".to_string(),
        ProxyError::ProviderUnhealthy(msg) => format!("Provider 不健康: {msg}"),
        ProxyError::DatabaseError(msg) => format!("数据库错误: {msg}"),
        ProxyError::TransformError(msg) => format!("请求/响应转换错误: {msg}"),
        _ => error.to_string(),
    }
}

pub(crate) fn proxy_core_error_to_proxy_error(error: ProxyCoreError) -> ProxyError {
    let message = error.to_string();
    match error {
        ProxyCoreError::InvalidRequest(_) => ProxyError::InvalidRequest(message),
        ProxyCoreError::Config(_) => ProxyError::ConfigError(message),
        ProxyCoreError::Auth(_) => ProxyError::AuthError(message),
        ProxyCoreError::Unavailable(_) => ProxyError::NoAvailableProvider,
        ProxyCoreError::Upstream(_) => ProxyError::ForwardFailed(message),
        ProxyCoreError::Unsupported(_) | ProxyCoreError::Internal(_) => {
            ProxyError::Internal(message)
        }
    }
}

pub(crate) fn response_body_parse_error_to_proxy_error(error: ProxyCoreError) -> ProxyError {
    match error {
        ProxyCoreError::Upstream(message) => ProxyError::TransformError(message),
        other => proxy_core_error_to_proxy_error(other),
    }
}

pub(crate) fn codex_proxy_error_json(
    provider_name: &str,
    request_model: &str,
    endpoint: &str,
    error: &ProxyError,
) -> Value {
    let (upstream_status, upstream_body) = match error {
        ProxyError::UpstreamError { status, body } => (Some(*status), body.as_deref()),
        _ => (None, None),
    };
    crate::proxy_core::codex_proxy_error_json(CodexProxyErrorContext {
        provider_name,
        request_model,
        endpoint,
        fallback_message: &get_error_message(error),
        fallback_code: codex_proxy_error_code(error),
        upstream_status,
        upstream_body,
    })
}

fn codex_proxy_error_code(error: &ProxyError) -> &'static str {
    match error {
        ProxyError::ForwardFailed(_) => "cc_switch_forward_failed",
        ProxyError::Timeout(_) | ProxyError::StreamIdleTimeout(_) => "cc_switch_timeout",
        ProxyError::NoAvailableProvider => "cc_switch_no_available_provider",
        ProxyError::AllProvidersCircuitOpen => "cc_switch_all_providers_circuit_open",
        ProxyError::NoProvidersConfigured => "cc_switch_no_providers_configured",
        ProxyError::MaxRetriesExceeded => "cc_switch_max_retries_exceeded",
        ProxyError::ProviderUnhealthy(_) => "cc_switch_provider_unhealthy",
        ProxyError::ConfigError(_) => "cc_switch_config_error",
        ProxyError::TransformError(_) => "cc_switch_transform_error",
        ProxyError::InvalidRequest(_) => "cc_switch_invalid_request",
        ProxyError::AuthError(_) => "cc_switch_auth_error",
        ProxyError::UpstreamError { .. } => "cc_switch_upstream_error",
        ProxyError::DatabaseError(_) => "cc_switch_database_error",
        ProxyError::Internal(_) => "cc_switch_internal_error",
        ProxyError::AlreadyRunning
        | ProxyError::NotRunning
        | ProxyError::BindFailed(_)
        | ProxyError::StopTimeout
        | ProxyError::StopFailed(_) => "cc_switch_proxy_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_upstream_error() {
        let error = ProxyError::UpstreamError {
            status: 401,
            body: Some("Unauthorized".to_string()),
        };
        assert_eq!(map_proxy_error_to_status(&error), 401);
    }

    #[test]
    fn test_map_timeout_error() {
        let error = ProxyError::Timeout("Request timeout".to_string());
        assert_eq!(map_proxy_error_to_status(&error), 504);
    }

    #[test]
    fn test_map_connection_error() {
        let error = ProxyError::ForwardFailed("Connection refused".to_string());
        assert_eq!(map_proxy_error_to_status(&error), 502);
    }

    #[test]
    fn test_map_no_provider_error() {
        let error = ProxyError::NoAvailableProvider;
        assert_eq!(map_proxy_error_to_status(&error), 503);
    }

    #[test]
    fn test_map_status_matches_proxy_error_response_semantics() {
        assert_eq!(
            map_proxy_error_to_status(&ProxyError::AuthError("bad token".to_string())),
            401
        );
        assert_eq!(
            map_proxy_error_to_status(&ProxyError::ConfigError("bad config".to_string())),
            400
        );
        assert_eq!(
            map_proxy_error_to_status(&ProxyError::InvalidRequest("bad request".to_string())),
            400
        );
        assert_eq!(
            map_proxy_error_to_status(&ProxyError::TransformError("bad transform".to_string())),
            422
        );
        assert_eq!(
            map_proxy_error_to_status(&ProxyError::StreamIdleTimeout(30)),
            504
        );
    }

    #[test]
    fn test_get_error_message() {
        let error = ProxyError::UpstreamError {
            status: 500,
            body: Some("Internal Server Error".to_string()),
        };
        let msg = get_error_message(&error);
        assert!(msg.contains("上游错误"));
        assert!(msg.contains("500"));
        assert!(msg.contains("Internal Server Error"));
    }

    #[test]
    fn test_proxy_core_error_bridge_maps_unavailable_to_proxy_error() {
        let error = proxy_core_error_to_proxy_error(ProxyCoreError::Unavailable(
            "no routable channel".to_string(),
        ));

        assert!(matches!(error, ProxyError::NoAvailableProvider));
    }

    #[test]
    fn test_proxy_core_error_bridge_maps_categories() {
        assert!(matches!(
            proxy_core_error_to_proxy_error(ProxyCoreError::InvalidRequest("bad".to_string())),
            ProxyError::InvalidRequest(_)
        ));
        assert!(matches!(
            proxy_core_error_to_proxy_error(ProxyCoreError::Config("bad".to_string())),
            ProxyError::ConfigError(_)
        ));
        assert!(matches!(
            proxy_core_error_to_proxy_error(ProxyCoreError::Auth("bad".to_string())),
            ProxyError::AuthError(_)
        ));
        assert!(matches!(
            proxy_core_error_to_proxy_error(ProxyCoreError::Upstream("bad".to_string())),
            ProxyError::ForwardFailed(_)
        ));
        assert!(matches!(
            proxy_core_error_to_proxy_error(ProxyCoreError::Internal("bad".to_string())),
            ProxyError::Internal(_)
        ));
    }

    #[test]
    fn test_response_body_parse_error_maps_upstream_to_transform_error() {
        let error = response_body_parse_error_to_proxy_error(ProxyCoreError::Upstream(
            "bad upstream body".to_string(),
        ));

        match error {
            ProxyError::TransformError(message) => assert!(message.contains("bad upstream body")),
            other => panic!("expected TransformError, got {other:?}"),
        }
    }

    #[test]
    fn test_codex_proxy_error_json_maps_host_error_code() {
        let body = codex_proxy_error_json(
            "DeepSeek",
            "deepseek-chat",
            "/responses",
            &ProxyError::ForwardFailed("dns lookup failed".to_string()),
        );

        assert_eq!(body["error"]["code"], "cc_switch_forward_failed");
        assert_eq!(body["error"]["provider"], "DeepSeek");
        assert_eq!(body["error"]["model"], "deepseek-chat");
        assert_eq!(body["error"]["endpoint"], "/responses");
    }

    #[test]
    fn test_codex_proxy_error_json_preserves_upstream_status() {
        let body = codex_proxy_error_json(
            "MiniMax",
            "abab6.5s",
            "/responses",
            &ProxyError::UpstreamError {
                status: 413,
                body: Some(r#"{"error":{"message":"too large"}}"#.to_string()),
            },
        );

        assert_eq!(body["error"]["code"], "cc_switch_upstream_error");
        assert_eq!(body["error"]["upstream_status"], 413);
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("413"));
        assert!(message.to_lowercase().contains("upstream"));
        assert_eq!(body["error"]["endpoint"], "/responses");
    }
}
