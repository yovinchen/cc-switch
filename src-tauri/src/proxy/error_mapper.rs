//! 错误类型到 HTTP 状态码的映射
//!
//! 将 ProxyError 映射到合适的 HTTP 状态码，用于日志记录和手动构建错误响应

use super::{error::ProxyError, ForwardError};
use crate::proxy::error::proxy_error_status_kind;
use crate::proxy_core::{
    codex_proxy_error_code, proxy_error_http_status_code, ClaudeDesktopGatewayAuthError,
    CodexProxyErrorContext, CodexProxyErrorKind, ForwardFailureKind, ManagementAuthError,
    ProxyCoreError,
};
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
    proxy_error_http_status_code(proxy_error_status_kind(error))
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

pub(crate) fn proxy_error_to_core_error(error: ProxyError) -> ProxyCoreError {
    let message = error.to_string();
    match error {
        ProxyError::NoAvailableProvider
        | ProxyError::AllProvidersCircuitOpen
        | ProxyError::NoProvidersConfigured
        | ProxyError::ProviderUnhealthy(_)
        | ProxyError::MaxRetriesExceeded => ProxyCoreError::Unavailable(message),
        ProxyError::ConfigError(_) => ProxyCoreError::Config(message),
        ProxyError::AuthError(_) => ProxyCoreError::Auth(message),
        ProxyError::InvalidRequest(_) => ProxyCoreError::InvalidRequest(message),
        ProxyError::ForwardFailed(_)
        | ProxyError::UpstreamError { .. }
        | ProxyError::Timeout(_)
        | ProxyError::StreamIdleTimeout(_) => ProxyCoreError::Upstream(message),
        ProxyError::AlreadyRunning
        | ProxyError::NotRunning
        | ProxyError::BindFailed(_)
        | ProxyError::StopTimeout
        | ProxyError::StopFailed(_)
        | ProxyError::DatabaseError(_)
        | ProxyError::TransformError(_)
        | ProxyError::Internal(_) => ProxyCoreError::Internal(message),
    }
}

pub(crate) fn forward_error_to_core_error(error: ForwardError) -> ProxyCoreError {
    proxy_error_to_core_error(error.error)
}

pub(crate) fn forward_failure_kind_from_proxy_error(error: &ProxyError) -> ForwardFailureKind {
    match error {
        ProxyError::UpstreamError { status, body } => ForwardFailureKind::Upstream {
            status: *status,
            body: body.clone(),
        },
        ProxyError::Timeout(message) => ForwardFailureKind::Timeout(message.clone()),
        ProxyError::ForwardFailed(message) => ForwardFailureKind::ForwardFailed(message.clone()),
        ProxyError::TransformError(message) => ForwardFailureKind::TransformError(message.clone()),
        ProxyError::ConfigError(message) => ForwardFailureKind::ConfigError(message.clone()),
        ProxyError::AuthError(message) => ForwardFailureKind::AuthError(message.clone()),
        ProxyError::ProviderUnhealthy(_) | ProxyError::StreamIdleTimeout(_) => {
            ForwardFailureKind::RetryableOther(error.to_string())
        }
        _ => ForwardFailureKind::Other(error.to_string()),
    }
}

pub(crate) fn reqwest_send_error_to_proxy_error(error: reqwest::Error) -> ProxyError {
    if error.is_timeout() {
        ProxyError::Timeout(format!("请求超时: {error}"))
    } else if error.is_connect() {
        ProxyError::ForwardFailed(format!("连接失败: {error}"))
    } else {
        ProxyError::ForwardFailed(error.to_string())
    }
}

pub(crate) fn management_api_error_to_proxy_error(error: ProxyCoreError) -> ProxyError {
    match error {
        ProxyCoreError::InvalidRequest(message) => ProxyError::InvalidRequest(message),
        other => proxy_core_error_to_proxy_error(other),
    }
}

pub(crate) fn management_auth_error_to_proxy_error(error: ManagementAuthError) -> ProxyError {
    ProxyError::AuthError(error.message().to_string())
}

pub(crate) fn claude_desktop_gateway_auth_error_to_proxy_error(
    error: ClaudeDesktopGatewayAuthError,
) -> ProxyError {
    ProxyError::AuthError(error.message().to_string())
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
        fallback_code: codex_proxy_error_code(codex_proxy_error_kind(error)),
        upstream_status,
        upstream_body,
    })
}

fn codex_proxy_error_kind(error: &ProxyError) -> CodexProxyErrorKind {
    match error {
        ProxyError::ForwardFailed(_) => CodexProxyErrorKind::ForwardFailed,
        ProxyError::Timeout(_) | ProxyError::StreamIdleTimeout(_) => CodexProxyErrorKind::Timeout,
        ProxyError::NoAvailableProvider => CodexProxyErrorKind::NoAvailableProvider,
        ProxyError::AllProvidersCircuitOpen => CodexProxyErrorKind::AllProvidersCircuitOpen,
        ProxyError::NoProvidersConfigured => CodexProxyErrorKind::NoProvidersConfigured,
        ProxyError::MaxRetriesExceeded => CodexProxyErrorKind::MaxRetriesExceeded,
        ProxyError::ProviderUnhealthy(_) => CodexProxyErrorKind::ProviderUnhealthy,
        ProxyError::ConfigError(_) => CodexProxyErrorKind::ConfigError,
        ProxyError::TransformError(_) => CodexProxyErrorKind::TransformError,
        ProxyError::InvalidRequest(_) => CodexProxyErrorKind::InvalidRequest,
        ProxyError::AuthError(_) => CodexProxyErrorKind::AuthError,
        ProxyError::UpstreamError { .. } => CodexProxyErrorKind::UpstreamError,
        ProxyError::DatabaseError(_) => CodexProxyErrorKind::DatabaseError,
        ProxyError::Internal(_) => CodexProxyErrorKind::InternalError,
        ProxyError::AlreadyRunning
        | ProxyError::NotRunning
        | ProxyError::BindFailed(_)
        | ProxyError::StopTimeout
        | ProxyError::StopFailed(_) => CodexProxyErrorKind::ProxyError,
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
        assert_eq!(
            map_proxy_error_to_status(&ProxyError::UpstreamError {
                status: 42,
                body: None
            }),
            502
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
    fn test_proxy_error_bridge_maps_categories_to_core() {
        assert!(matches!(
            proxy_error_to_core_error(ProxyError::NoProvidersConfigured),
            ProxyCoreError::Unavailable(_)
        ));
        assert!(matches!(
            proxy_error_to_core_error(ProxyError::ConfigError("bad".to_string())),
            ProxyCoreError::Config(_)
        ));
        assert!(matches!(
            proxy_error_to_core_error(ProxyError::AuthError("bad".to_string())),
            ProxyCoreError::Auth(_)
        ));
        assert!(matches!(
            proxy_error_to_core_error(ProxyError::InvalidRequest("bad".to_string())),
            ProxyCoreError::InvalidRequest(_)
        ));
        assert!(matches!(
            proxy_error_to_core_error(ProxyError::Timeout("slow".to_string())),
            ProxyCoreError::Upstream(_)
        ));
        assert!(matches!(
            proxy_error_to_core_error(ProxyError::TransformError("bad body".to_string())),
            ProxyCoreError::Internal(_)
        ));
    }

    #[test]
    fn test_forward_error_bridge_uses_proxy_error_category() {
        let error = forward_error_to_core_error(ForwardError {
            error: ProxyError::ForwardFailed("connection refused".to_string()),
            provider: None,
        });

        assert!(matches!(error, ProxyCoreError::Upstream(_)));
    }

    #[test]
    fn test_proxy_error_bridge_maps_forward_failure_kind() {
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::Timeout("slow".to_string())),
            ForwardFailureKind::Timeout(message) if message == "slow"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::ForwardFailed(
                "connection reset".to_string()
            )),
            ForwardFailureKind::ForwardFailed(message) if message == "connection reset"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::AuthError("bad token".to_string())),
            ForwardFailureKind::AuthError(message) if message == "bad token"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::ProviderUnhealthy(
                "half-open".to_string()
            )),
            ForwardFailureKind::RetryableOther(_)
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::DatabaseError(
                "write failed".to_string()
            )),
            ForwardFailureKind::Other(_)
        ));
    }

    #[test]
    fn test_proxy_error_bridge_preserves_upstream_failure_details() {
        let failure = forward_failure_kind_from_proxy_error(&ProxyError::UpstreamError {
            status: 429,
            body: Some(r#"{"error":{"message":"rate limit"}}"#.to_string()),
        });

        match failure {
            ForwardFailureKind::Upstream { status, body } => {
                assert_eq!(status, 429);
                assert_eq!(
                    body.as_deref(),
                    Some(r#"{"error":{"message":"rate limit"}}"#)
                );
            }
            other => panic!("expected upstream failure, got {other:?}"),
        }
    }

    #[test]
    fn test_reqwest_send_error_bridge_maps_non_network_errors_to_forward_failed() {
        let error = reqwest::Client::new()
            .get("https://example.com")
            .header("x-bad-header", "\n")
            .build()
            .expect_err("invalid header value should fail request build");

        let error = reqwest_send_error_to_proxy_error(error);

        assert!(matches!(error, ProxyError::ForwardFailed(message) if !message.is_empty()));
    }

    #[test]
    fn test_management_api_error_bridge_preserves_invalid_request_message() {
        let error =
            management_api_error_to_proxy_error(ProxyCoreError::InvalidRequest("bad".to_string()));

        assert!(matches!(error, ProxyError::InvalidRequest(message) if message == "bad"));
    }

    #[test]
    fn test_management_auth_error_bridge_maps_to_auth_error() {
        let error = management_auth_error_to_proxy_error(ManagementAuthError::MissingBearerToken);

        assert!(
            matches!(error, ProxyError::AuthError(message) if message == "Missing management bearer token")
        );
    }

    #[test]
    fn test_claude_desktop_gateway_auth_error_bridge_maps_to_auth_error() {
        let error = claude_desktop_gateway_auth_error_to_proxy_error(
            ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader,
        );

        assert!(
            matches!(error, ProxyError::AuthError(message) if message == "Claude Desktop gateway 缺少 Authorization 头")
        );
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
