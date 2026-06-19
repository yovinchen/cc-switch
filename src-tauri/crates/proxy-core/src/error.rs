use http::StatusCode;
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
}
