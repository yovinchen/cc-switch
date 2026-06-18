use http::HeaderMap;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamRequestTransportPolicy {
    pub is_streaming_request: bool,
    pub force_identity_encoding: bool,
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

#[cfg(test)]
mod tests {
    use super::{is_streaming_upstream_request, resolve_upstream_request_transport_policy};
    use http::{header::ACCEPT, HeaderMap, HeaderValue};
    use serde_json::json;

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
}
