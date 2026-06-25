//! 错误类型到 HTTP 状态码的映射
//!
//! 将 ProxyError 映射到合适的 HTTP 状态码，用于日志记录和手动构建错误响应

use super::{error::ProxyError, ForwardError};
#[cfg(test)]
use crate::proxy_core_adapter::{
    codex_proxy_error_json_from_proxy_error as core_codex_proxy_error_json, ProxyResponseBody,
};
use crate::proxy_core_adapter::{
    codex_proxy_error_response_from_proxy_error as core_codex_proxy_error_response,
    log_unlabeled_sse_fallback_event, parse_upstream_json_or_unlabeled_sse,
    proxy_core_error_from_status_kind, proxy_error_status_kind,
    upstream_response_parse_failure_log_message, upstream_send_error_projection,
    CoreResponseBuildFailureContext, CoreResponseTransformFailureContext, ProxyCoreError,
    ProxyCoreResponse, ProxyCoreResult, ProxyErrorStatusKind, UnlabeledSseFallbackLogContext,
    UpstreamResponseParseFailureLogContext, UpstreamSendErrorInput, UpstreamSseAggregationKind,
};
use http::HeaderMap;
use serde_json::Value;

pub(crate) fn proxy_core_error_to_proxy_error(error: ProxyCoreError) -> ProxyError {
    match error {
        ProxyCoreError::InvalidRequest(message) => ProxyError::InvalidRequest(message),
        ProxyCoreError::Config(message) => ProxyError::ConfigError(message),
        ProxyCoreError::Auth(message) => ProxyError::AuthError(message),
        ProxyCoreError::Unavailable(_) => ProxyError::NoAvailableProvider,
        ProxyCoreError::Upstream(message) => ProxyError::ForwardFailed(message),
        ProxyCoreError::Unsupported(message) | ProxyCoreError::Internal(message) => {
            ProxyError::Internal(message)
        }
    }
}

pub(crate) fn proxy_error_to_core_error(error: ProxyError) -> ProxyCoreError {
    let kind = proxy_error_status_kind(&error);
    let message = error.to_string();
    proxy_core_error_from_status_kind(kind, message)
}

pub(crate) fn forward_error_to_core_error(error: ForwardError) -> ProxyCoreError {
    proxy_error_to_core_error(error.error)
}

pub(crate) fn reqwest_send_error_to_proxy_error(error: reqwest::Error) -> ProxyError {
    let projection = upstream_send_error_projection(UpstreamSendErrorInput {
        is_timeout: error.is_timeout(),
        is_connect: error.is_connect(),
        message: error.to_string(),
    });
    match projection.status_kind {
        ProxyErrorStatusKind::Timeout => ProxyError::Timeout(projection.message),
        _ => ProxyError::ForwardFailed(projection.message),
    }
}

pub(crate) fn management_api_error_to_proxy_error(error: ProxyCoreError) -> ProxyError {
    match error {
        ProxyCoreError::InvalidRequest(message) => ProxyError::InvalidRequest(message),
        other => proxy_core_error_to_proxy_error(other),
    }
}

pub(crate) fn response_body_parse_error_to_proxy_error(error: ProxyCoreError) -> ProxyError {
    match error {
        ProxyCoreError::Upstream(message) => ProxyError::TransformError(message),
        other => proxy_core_error_to_proxy_error(other),
    }
}

pub(crate) fn parse_logged_upstream_json_or_unlabeled_sse(
    body: &[u8],
    headers: &HeaderMap,
    failure_message: &'static str,
    aggregation: Option<UpstreamSseAggregationKind>,
    parse_failure_context: UpstreamResponseParseFailureLogContext,
    fallback_log_context: UnlabeledSseFallbackLogContext<'_>,
) -> Result<Value, ProxyError> {
    let parsed =
        parse_upstream_json_or_unlabeled_sse(body, headers, failure_message, aggregation, || {
            uuid::Uuid::new_v4().to_string()
        })
        .map_err(|error| {
            log::error!(
                "{}",
                upstream_response_parse_failure_log_message(parse_failure_context, &error, body)
            );
            response_body_parse_error_to_proxy_error(error)
        })?;

    log_unlabeled_sse_fallback_event(parsed.source, fallback_log_context);
    Ok(parsed.value)
}

pub(crate) fn parse_claude_transform_upstream_json_or_unlabeled_sse(
    body: &[u8],
    headers: &HeaderMap,
    aggregation: Option<UpstreamSseAggregationKind>,
    api_format: &str,
    aggregate_codex_oauth_responses_sse: bool,
) -> Result<Value, ProxyError> {
    parse_logged_upstream_json_or_unlabeled_sse(
        body,
        headers,
        "Failed to parse upstream response",
        aggregation,
        UpstreamResponseParseFailureLogContext::ClaudeTransform,
        UnlabeledSseFallbackLogContext::Claude {
            api_format,
            codex_oauth_responses_aggregation: aggregate_codex_oauth_responses_sse,
        },
    )
}

pub(crate) fn parse_codex_chat_upstream_json_or_unlabeled_sse(
    body: &[u8],
    headers: &HeaderMap,
    aggregation: Option<UpstreamSseAggregationKind>,
) -> Result<Value, ProxyError> {
    parse_logged_upstream_json_or_unlabeled_sse(
        body,
        headers,
        "Failed to parse upstream chat response",
        aggregation,
        UpstreamResponseParseFailureLogContext::CodexChat,
        UnlabeledSseFallbackLogContext::CodexChat,
    )
}

pub(crate) fn response_build_error_to_proxy_error(
    context: CoreResponseBuildFailureContext,
    error: ProxyCoreError,
) -> ProxyError {
    log::error!("{}: {error}", context.log_prefix());
    proxy_core_error_to_proxy_error(error)
}

pub(crate) fn codex_responses_error_body_build_error_to_proxy_error(
    error: ProxyCoreError,
) -> ProxyError {
    response_build_error_to_proxy_error(CoreResponseBuildFailureContext::CodexResponsesError, error)
}

pub(crate) fn codex_proxy_error_body_build_error_to_proxy_error(
    error: ProxyCoreError,
) -> ProxyError {
    response_build_error_to_proxy_error(CoreResponseBuildFailureContext::CodexProxyError, error)
}

pub(crate) fn response_transform_error_to_proxy_error(
    context: CoreResponseTransformFailureContext,
    error: String,
) -> ProxyError {
    log::error!("{}: {error}", context.log_prefix());
    ProxyError::TransformError(error)
}

pub(crate) fn claude_response_transform_error_to_proxy_error(error: String) -> ProxyError {
    response_transform_error_to_proxy_error(
        CoreResponseTransformFailureContext::ClaudeResponse,
        error,
    )
}

pub(crate) fn codex_chat_to_responses_transform_error_to_proxy_error(error: String) -> ProxyError {
    response_transform_error_to_proxy_error(
        CoreResponseTransformFailureContext::CodexChatToResponses,
        error,
    )
}

#[cfg(test)]
pub(crate) fn codex_proxy_error_json(
    provider_name: &str,
    request_model: &str,
    endpoint: &str,
    error: &ProxyError,
) -> Value {
    core_codex_proxy_error_json(provider_name, request_model, endpoint, error)
}

pub(crate) fn codex_proxy_error_response(
    provider_name: &str,
    request_model: &str,
    endpoint: &str,
    error: &ProxyError,
) -> ProxyCoreResult<ProxyCoreResponse> {
    core_codex_proxy_error_response(provider_name, request_model, endpoint, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core_adapter::{proxy_error_display_message, proxy_error_status_code};

    #[test]
    fn adapter_status_contract_maps_upstream_error() {
        let error = ProxyError::UpstreamError {
            status: 401,
            body: Some("Unauthorized".to_string()),
        };
        assert_eq!(proxy_error_status_code(&error), 401);
    }

    #[test]
    fn adapter_status_contract_maps_timeout_error() {
        let error = ProxyError::Timeout("Request timeout".to_string());
        assert_eq!(proxy_error_status_code(&error), 504);
    }

    #[test]
    fn adapter_status_contract_maps_connection_error() {
        let error = ProxyError::ForwardFailed("Connection refused".to_string());
        assert_eq!(proxy_error_status_code(&error), 502);
    }

    #[test]
    fn adapter_status_contract_maps_no_provider_error() {
        let error = ProxyError::NoAvailableProvider;
        assert_eq!(proxy_error_status_code(&error), 503);
    }

    #[test]
    fn adapter_status_contract_matches_proxy_error_response_semantics() {
        assert_eq!(
            proxy_error_status_code(&ProxyError::AuthError("bad token".to_string())),
            401
        );
        assert_eq!(
            proxy_error_status_code(&ProxyError::ConfigError("bad config".to_string())),
            400
        );
        assert_eq!(
            proxy_error_status_code(&ProxyError::InvalidRequest("bad request".to_string())),
            400
        );
        assert_eq!(
            proxy_error_status_code(&ProxyError::TransformError("bad transform".to_string())),
            422
        );
        assert_eq!(
            proxy_error_status_code(&ProxyError::StreamIdleTimeout(30)),
            504
        );
        assert_eq!(
            proxy_error_status_code(&ProxyError::UpstreamError {
                status: 42,
                body: None
            }),
            502
        );
    }

    #[test]
    fn adapter_display_contract_maps_upstream_error_message() {
        let error = ProxyError::UpstreamError {
            status: 500,
            body: Some("Internal Server Error".to_string()),
        };
        let msg = proxy_error_display_message(&error);
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
        });

        assert!(matches!(error, ProxyCoreError::Upstream(_)));
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

    #[test]
    fn test_codex_proxy_error_response_maps_host_status_and_body() {
        let response = codex_proxy_error_response(
            "DeepSeek",
            "deepseek-chat",
            "/responses",
            &ProxyError::AuthError("bad token".to_string()),
        )
        .expect("response");

        assert_eq!(response.status.as_u16(), 401);

        let body = match response.body {
            ProxyResponseBody::Bytes(body) => body,
            other => panic!("expected bytes body, got {other:?}"),
        };
        let value: Value = serde_json::from_slice(&body).expect("json body");

        assert_eq!(value["error"]["code"], "cc_switch_auth_error");
        assert_eq!(value["error"]["provider"], "DeepSeek");
        assert_eq!(value["error"]["model"], "deepseek-chat");
    }
}
