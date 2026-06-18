use http::HeaderMap;
use serde_json::Value;
use std::time::Duration;

pub const DEFAULT_UPSTREAM_SEND_TIMEOUT: Duration = Duration::from_secs(600);
pub const STREAMING_REQWEST_REQUEST_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamRequestTransportPolicy {
    pub is_streaming_request: bool,
    pub force_identity_encoding: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamTransportKind {
    PooledReqwest,
    RawHyper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamSendPolicy {
    pub transport: UpstreamTransportKind,
    pub base_timeout: Duration,
    pub reqwest_request_timeout: Option<Duration>,
    pub streaming_header_timeout: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamSendPolicyInput {
    pub is_socks_proxy: bool,
    pub preserve_exact_header_case: bool,
    pub request_is_streaming: bool,
    pub non_streaming_timeout: Duration,
    pub streaming_first_byte_timeout: Duration,
}

pub fn resolve_upstream_request_transport_policy(
    needs_transform: bool,
    codex_responses_to_chat: bool,
    endpoint: &str,
    body: &Value,
    headers: &HeaderMap,
) -> UpstreamRequestTransportPolicy {
    let is_streaming_request = is_streaming_upstream_request(endpoint, body, headers);

    UpstreamRequestTransportPolicy {
        is_streaming_request,
        force_identity_encoding: needs_transform
            || codex_responses_to_chat
            || is_streaming_request,
    }
}

pub fn is_streaming_upstream_request(endpoint: &str, body: &Value, headers: &HeaderMap) -> bool {
    if body
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return true;
    }

    if endpoint.contains("streamGenerateContent") || endpoint.contains("alt=sse") {
        return true;
    }

    headers
        .get(http::header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|accept| accept.contains("text/event-stream"))
        .unwrap_or(false)
}

pub fn is_socks_proxy_url(upstream_proxy_url: Option<&str>) -> bool {
    upstream_proxy_url
        .map(|url| url.starts_with("socks5"))
        .unwrap_or(false)
}

pub fn resolve_upstream_send_policy(input: UpstreamSendPolicyInput) -> UpstreamSendPolicy {
    let base_timeout = if input.non_streaming_timeout.is_zero() {
        DEFAULT_UPSTREAM_SEND_TIMEOUT
    } else {
        input.non_streaming_timeout
    };
    let transport = if input.is_socks_proxy || !input.preserve_exact_header_case {
        UpstreamTransportKind::PooledReqwest
    } else {
        UpstreamTransportKind::RawHyper
    };

    let (reqwest_request_timeout, streaming_header_timeout) =
        if matches!(transport, UpstreamTransportKind::PooledReqwest) {
            if input.request_is_streaming {
                let header_timeout = if input.streaming_first_byte_timeout.is_zero() {
                    base_timeout
                } else {
                    input.streaming_first_byte_timeout
                };
                (
                    Some(STREAMING_REQWEST_REQUEST_TIMEOUT),
                    Some(header_timeout),
                )
            } else if input.non_streaming_timeout.is_zero() {
                (None, None)
            } else {
                (Some(input.non_streaming_timeout), None)
            }
        } else {
            (None, None)
        };

    UpstreamSendPolicy {
        transport,
        base_timeout,
        reqwest_request_timeout,
        streaming_header_timeout,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_socks_proxy_url, is_streaming_upstream_request,
        resolve_upstream_request_transport_policy, resolve_upstream_send_policy,
        UpstreamSendPolicyInput, UpstreamTransportKind, DEFAULT_UPSTREAM_SEND_TIMEOUT,
        STREAMING_REQWEST_REQUEST_TIMEOUT,
    };
    use http::{header::ACCEPT, HeaderMap, HeaderValue};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn stream_flag_marks_request_as_streaming_and_forces_identity() {
        let headers = HeaderMap::new();

        let policy = resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({ "stream": true }),
            &headers,
        );

        assert!(policy.is_streaming_request);
        assert!(policy.force_identity_encoding);
    }

    #[test]
    fn gemini_sse_endpoint_marks_request_as_streaming() {
        let headers = HeaderMap::new();

        assert!(is_streaming_upstream_request(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
            &json!({ "model": "gemini-2.5-pro" }),
            &headers
        ));
    }

    #[test]
    fn sse_accept_header_marks_request_as_streaming() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));

        let policy = resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({ "model": "gpt-5" }),
            &headers,
        );

        assert!(policy.is_streaming_request);
        assert!(policy.force_identity_encoding);
    }

    #[test]
    fn transform_paths_force_identity_even_for_non_streaming_requests() {
        let headers = HeaderMap::new();

        let transform_policy = resolve_upstream_request_transport_policy(
            true,
            false,
            "/v1/messages",
            &json!({ "model": "claude-sonnet-4" }),
            &headers,
        );
        let codex_chat_policy = resolve_upstream_request_transport_policy(
            false,
            true,
            "/v1/chat/completions",
            &json!({ "model": "gpt-5" }),
            &headers,
        );

        assert!(!transform_policy.is_streaming_request);
        assert!(transform_policy.force_identity_encoding);
        assert!(!codex_chat_policy.is_streaming_request);
        assert!(codex_chat_policy.force_identity_encoding);
    }

    #[test]
    fn ordinary_requests_allow_automatic_compression() {
        let headers = HeaderMap::new();

        let policy = resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({ "model": "gpt-5" }),
            &headers,
        );

        assert!(!policy.is_streaming_request);
        assert!(!policy.force_identity_encoding);
    }

    #[test]
    fn socks_proxy_detection_requires_socks5_prefix() {
        assert!(is_socks_proxy_url(Some("socks5://127.0.0.1:1080")));
        assert!(!is_socks_proxy_url(Some("http://127.0.0.1:8080")));
        assert!(!is_socks_proxy_url(None));
    }

    #[test]
    fn send_policy_uses_reqwest_when_socks_proxy_is_active() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: true,
            preserve_exact_header_case: true,
            request_is_streaming: false,
            non_streaming_timeout: Duration::from_secs(12),
            streaming_first_byte_timeout: Duration::from_secs(3),
        });

        assert_eq!(policy.transport, UpstreamTransportKind::PooledReqwest);
        assert_eq!(policy.base_timeout, Duration::from_secs(12));
        assert_eq!(policy.reqwest_request_timeout, Some(Duration::from_secs(12)));
        assert_eq!(policy.streaming_header_timeout, None);
    }

    #[test]
    fn send_policy_uses_reqwest_when_exact_header_case_is_not_required() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: false,
            preserve_exact_header_case: false,
            request_is_streaming: false,
            non_streaming_timeout: Duration::from_secs(0),
            streaming_first_byte_timeout: Duration::from_secs(0),
        });

        assert_eq!(policy.transport, UpstreamTransportKind::PooledReqwest);
        assert_eq!(policy.base_timeout, DEFAULT_UPSTREAM_SEND_TIMEOUT);
        assert_eq!(policy.reqwest_request_timeout, None);
        assert_eq!(policy.streaming_header_timeout, None);
    }

    #[test]
    fn send_policy_keeps_raw_hyper_when_exact_header_case_is_required_without_socks() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: false,
            preserve_exact_header_case: true,
            request_is_streaming: false,
            non_streaming_timeout: Duration::from_secs(30),
            streaming_first_byte_timeout: Duration::from_secs(5),
        });

        assert_eq!(policy.transport, UpstreamTransportKind::RawHyper);
        assert_eq!(policy.base_timeout, Duration::from_secs(30));
        assert_eq!(policy.reqwest_request_timeout, None);
        assert_eq!(policy.streaming_header_timeout, None);
    }

    #[test]
    fn streaming_reqwest_policy_uses_long_request_timeout_and_header_timeout() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: false,
            preserve_exact_header_case: false,
            request_is_streaming: true,
            non_streaming_timeout: Duration::from_secs(20),
            streaming_first_byte_timeout: Duration::from_secs(4),
        });

        assert_eq!(policy.transport, UpstreamTransportKind::PooledReqwest);
        assert_eq!(
            policy.reqwest_request_timeout,
            Some(STREAMING_REQWEST_REQUEST_TIMEOUT)
        );
        assert_eq!(
            policy.streaming_header_timeout,
            Some(Duration::from_secs(4))
        );
    }

    #[test]
    fn streaming_reqwest_header_timeout_falls_back_to_base_timeout() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: false,
            preserve_exact_header_case: false,
            request_is_streaming: true,
            non_streaming_timeout: Duration::from_secs(0),
            streaming_first_byte_timeout: Duration::from_secs(0),
        });

        assert_eq!(
            policy.streaming_header_timeout,
            Some(DEFAULT_UPSTREAM_SEND_TIMEOUT)
        );
    }
}
