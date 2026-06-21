use http::StatusCode;
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxyCoreError {
    #[error("invalid proxy request: {0}")]
    InvalidRequest(String),
    #[error("proxy configuration error: {0}")]
    Config(String),
    #[error("proxy authentication error: {0}")]
    Auth(String),
    #[error("upstream proxy error: {0}")]
    Upstream(String),
    #[error("proxy route unavailable: {0}")]
    Unavailable(String),
    #[error("proxy core feature is not supported yet: {0}")]
    Unsupported(String),
    #[error("proxy core internal error: {0}")]
    Internal(String),
}

pub type ProxyCoreResult<T> = Result<T, ProxyCoreError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyErrorStatusKind {
    AlreadyRunning,
    NotRunning,
    BindFailed,
    StopTimeout,
    StopFailed,
    ForwardFailed,
    NoAvailableProvider,
    AllProvidersCircuitOpen,
    NoProvidersConfigured,
    ProviderUnhealthy,
    UpstreamError(u16),
    MaxRetriesExceeded,
    DatabaseError,
    ConfigError,
    TransformError,
    InvalidRequest,
    Timeout,
    StreamIdleTimeout,
    AuthError,
    Internal,
}

pub fn proxy_error_http_status_code(kind: ProxyErrorStatusKind) -> u16 {
    match kind {
        ProxyErrorStatusKind::AlreadyRunning => StatusCode::CONFLICT.as_u16(),
        ProxyErrorStatusKind::NotRunning
        | ProxyErrorStatusKind::NoAvailableProvider
        | ProxyErrorStatusKind::AllProvidersCircuitOpen
        | ProxyErrorStatusKind::NoProvidersConfigured
        | ProxyErrorStatusKind::ProviderUnhealthy
        | ProxyErrorStatusKind::MaxRetriesExceeded => StatusCode::SERVICE_UNAVAILABLE.as_u16(),
        ProxyErrorStatusKind::UpstreamError(status) => {
            StatusCode::from_u16(status).map_or(StatusCode::BAD_GATEWAY.as_u16(), |_| status)
        }
        ProxyErrorStatusKind::Timeout | ProxyErrorStatusKind::StreamIdleTimeout => {
            StatusCode::GATEWAY_TIMEOUT.as_u16()
        }
        ProxyErrorStatusKind::ForwardFailed => StatusCode::BAD_GATEWAY.as_u16(),
        ProxyErrorStatusKind::ConfigError | ProxyErrorStatusKind::InvalidRequest => {
            StatusCode::BAD_REQUEST.as_u16()
        }
        ProxyErrorStatusKind::AuthError => StatusCode::UNAUTHORIZED.as_u16(),
        ProxyErrorStatusKind::TransformError => StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
        ProxyErrorStatusKind::BindFailed
        | ProxyErrorStatusKind::StopTimeout
        | ProxyErrorStatusKind::StopFailed
        | ProxyErrorStatusKind::DatabaseError
        | ProxyErrorStatusKind::Internal => StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
    }
}

pub fn proxy_core_error_from_status_kind(
    kind: ProxyErrorStatusKind,
    message: impl Into<String>,
) -> ProxyCoreError {
    let message = message.into();
    match kind {
        ProxyErrorStatusKind::NoAvailableProvider
        | ProxyErrorStatusKind::AllProvidersCircuitOpen
        | ProxyErrorStatusKind::NoProvidersConfigured
        | ProxyErrorStatusKind::ProviderUnhealthy
        | ProxyErrorStatusKind::MaxRetriesExceeded => ProxyCoreError::Unavailable(message),
        ProxyErrorStatusKind::ConfigError => ProxyCoreError::Config(message),
        ProxyErrorStatusKind::AuthError => ProxyCoreError::Auth(message),
        ProxyErrorStatusKind::InvalidRequest => ProxyCoreError::InvalidRequest(message),
        ProxyErrorStatusKind::ForwardFailed
        | ProxyErrorStatusKind::UpstreamError(_)
        | ProxyErrorStatusKind::Timeout
        | ProxyErrorStatusKind::StreamIdleTimeout => ProxyCoreError::Upstream(message),
        ProxyErrorStatusKind::AlreadyRunning
        | ProxyErrorStatusKind::NotRunning
        | ProxyErrorStatusKind::BindFailed
        | ProxyErrorStatusKind::StopTimeout
        | ProxyErrorStatusKind::StopFailed
        | ProxyErrorStatusKind::DatabaseError
        | ProxyErrorStatusKind::TransformError
        | ProxyErrorStatusKind::Internal => ProxyCoreError::Internal(message),
    }
}

pub fn proxy_error_response_body(message: impl Into<String>) -> Value {
    json!({
        "error": {
            "message": message.into(),
            "type": "proxy_error",
        }
    })
}

pub fn upstream_proxy_error_response_body(
    upstream_status: u16,
    upstream_body: Option<&str>,
) -> Value {
    if let Some(body) = upstream_body {
        serde_json::from_str::<Value>(body).unwrap_or_else(|_| {
            json!({
                "error": {
                    "message": body,
                    "type": "upstream_error",
                }
            })
        })
    } else {
        json!({
            "error": {
                "message": format!("Upstream error (status {upstream_status})"),
                "type": "upstream_error",
            }
        })
    }
}

pub fn error_message_with_context(context: &str, error: &str) -> String {
    format!("{context}: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_error_http_status_codes_preserve_host_contract() {
        let cases = [
            (ProxyErrorStatusKind::AlreadyRunning, 409),
            (ProxyErrorStatusKind::NotRunning, 503),
            (ProxyErrorStatusKind::BindFailed, 500),
            (ProxyErrorStatusKind::StopTimeout, 500),
            (ProxyErrorStatusKind::StopFailed, 500),
            (ProxyErrorStatusKind::ForwardFailed, 502),
            (ProxyErrorStatusKind::NoAvailableProvider, 503),
            (ProxyErrorStatusKind::AllProvidersCircuitOpen, 503),
            (ProxyErrorStatusKind::NoProvidersConfigured, 503),
            (ProxyErrorStatusKind::ProviderUnhealthy, 503),
            (ProxyErrorStatusKind::UpstreamError(401), 401),
            (ProxyErrorStatusKind::MaxRetriesExceeded, 503),
            (ProxyErrorStatusKind::DatabaseError, 500),
            (ProxyErrorStatusKind::ConfigError, 400),
            (ProxyErrorStatusKind::TransformError, 422),
            (ProxyErrorStatusKind::InvalidRequest, 400),
            (ProxyErrorStatusKind::Timeout, 504),
            (ProxyErrorStatusKind::StreamIdleTimeout, 504),
            (ProxyErrorStatusKind::AuthError, 401),
            (ProxyErrorStatusKind::Internal, 500),
        ];

        for (kind, expected) in cases {
            assert_eq!(proxy_error_http_status_code(kind), expected);
        }
    }

    #[test]
    fn invalid_upstream_status_falls_back_to_bad_gateway() {
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::UpstreamError(42)),
            StatusCode::BAD_GATEWAY.as_u16()
        );
    }

    #[test]
    fn proxy_status_kind_maps_to_core_error_categories() {
        assert!(matches!(
            proxy_core_error_from_status_kind(ProxyErrorStatusKind::NoProvidersConfigured, "none"),
            ProxyCoreError::Unavailable(_)
        ));
        assert!(matches!(
            proxy_core_error_from_status_kind(ProxyErrorStatusKind::ConfigError, "bad"),
            ProxyCoreError::Config(_)
        ));
        assert!(matches!(
            proxy_core_error_from_status_kind(ProxyErrorStatusKind::AuthError, "bad"),
            ProxyCoreError::Auth(_)
        ));
        assert!(matches!(
            proxy_core_error_from_status_kind(ProxyErrorStatusKind::InvalidRequest, "bad"),
            ProxyCoreError::InvalidRequest(_)
        ));
        assert!(matches!(
            proxy_core_error_from_status_kind(ProxyErrorStatusKind::Timeout, "slow"),
            ProxyCoreError::Upstream(_)
        ));
        assert!(matches!(
            proxy_core_error_from_status_kind(ProxyErrorStatusKind::TransformError, "bad body"),
            ProxyCoreError::Internal(_)
        ));
    }

    #[test]
    fn proxy_error_response_body_preserves_host_envelope() {
        let body = proxy_error_response_body("dns lookup failed");

        assert_eq!(body["error"]["message"], "dns lookup failed");
        assert_eq!(body["error"]["type"], "proxy_error");
    }

    #[test]
    fn upstream_proxy_error_response_body_preserves_json_or_wraps_text() {
        let json_body = upstream_proxy_error_response_body(
            429,
            Some(r#"{"error":{"message":"rate limited","type":"quota"}}"#),
        );
        assert_eq!(json_body["error"]["message"], "rate limited");
        assert_eq!(json_body["error"]["type"], "quota");

        let text_body = upstream_proxy_error_response_body(502, Some("bad gateway"));
        assert_eq!(text_body["error"]["message"], "bad gateway");
        assert_eq!(text_body["error"]["type"], "upstream_error");

        let empty_body = upstream_proxy_error_response_body(503, None);
        assert_eq!(
            empty_body["error"]["message"],
            "Upstream error (status 503)"
        );
        assert_eq!(empty_body["error"]["type"], "upstream_error");
    }

    #[test]
    fn error_message_with_context_preserves_host_adapter_text() {
        assert_eq!(
            error_message_with_context("load app proxy config", "database unavailable"),
            "load app proxy config: database unavailable"
        );
    }
}
