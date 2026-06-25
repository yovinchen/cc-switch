use crate::{
    domain::{ProxyCoreResponse, ProxyResponseBody},
    error::{ProxyCoreError, ProxyCoreResult},
    response_headers::{
        prepare_rebuilt_json_response_headers, strip_hop_by_hop_response_headers,
        transformed_sse_response_headers,
    },
};
use bytes::Bytes;
use futures::Stream;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

pub enum ProxyResponseBuildErrorContext<'a> {
    TaggedStreaming { tag: &'a str },
    TaggedResponse { tag: &'a str },
    ClaudeSse,
    ClaudeResponse,
    CodexSse,
    CodexResponses,
    CodexResponsesError,
    CodexProxyError,
}

impl ProxyResponseBuildErrorContext<'_> {
    pub fn message(&self) -> String {
        match self {
            Self::TaggedStreaming { tag } => format!("[{tag}] 构建流式响应失败"),
            Self::TaggedResponse { tag } => format!("[{tag}] 构建响应失败"),
            Self::ClaudeSse => "[Claude] 构建 SSE 响应失败".to_string(),
            Self::ClaudeResponse => "[Claude] 构建响应失败".to_string(),
            Self::CodexSse => "[Codex] 构建 SSE 响应失败".to_string(),
            Self::CodexResponses => "[Codex] 构建 Responses 响应失败".to_string(),
            Self::CodexResponsesError => "[Codex] 构建 Responses 错误响应失败".to_string(),
            Self::CodexProxyError => "[Codex] 构建代理错误响应失败".to_string(),
        }
    }

    pub fn internal_error_prefix(&self) -> &'static str {
        match self {
            Self::TaggedStreaming { .. } => "Failed to build streaming response",
            _ => "Failed to build response",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyResponseBuildFailureContext {
    ClaudeJson,
    CodexResponses,
    CodexResponsesError,
    CodexProxyError,
}

impl ProxyResponseBuildFailureContext {
    pub fn log_prefix(&self) -> &'static str {
        match self {
            Self::ClaudeJson => "[Claude] 构造 JSON 响应失败",
            Self::CodexResponses => "[Codex] 构造 Responses 响应失败",
            Self::CodexResponsesError => "[Codex] 构造 Responses 错误体失败",
            Self::CodexProxyError => "[Codex] 构造代理错误响应失败",
        }
    }
}

/// Build a host-neutral response for a JSON body reconstructed by a transform.
pub fn rebuilt_json_proxy_response(
    status: StatusCode,
    mut headers: HeaderMap,
    body: Value,
) -> ProxyCoreResult<ProxyCoreResponse> {
    prepare_rebuilt_json_response_headers(&mut headers);
    json_proxy_response_with_headers(status, headers, body)
}

/// Build a host-neutral JSON response with fresh JSON headers.
pub fn json_proxy_response(status: StatusCode, body: Value) -> ProxyCoreResult<ProxyCoreResponse> {
    let mut headers = HeaderMap::new();
    prepare_rebuilt_json_response_headers(&mut headers);
    json_proxy_response_with_headers(status, headers, body)
}

/// Build a host-neutral response for an upstream byte body that should be
/// passed through after hop-by-hop response headers are removed.
pub fn passthrough_bytes_proxy_response(
    status: StatusCode,
    mut headers: HeaderMap,
    body: impl Into<Bytes>,
) -> ProxyCoreResponse {
    strip_hop_by_hop_response_headers(&mut headers);
    ProxyCoreResponse::with_body(status, headers, ProxyResponseBody::bytes(body))
}

/// Build a host-neutral response for an upstream byte stream that should be
/// passed through after hop-by-hop response headers are removed.
pub fn passthrough_stream_proxy_response(
    status: StatusCode,
    mut headers: HeaderMap,
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
) -> ProxyCoreResponse {
    strip_hop_by_hop_response_headers(&mut headers);
    ProxyCoreResponse::with_body(status, headers, ProxyResponseBody::stream(stream))
}

fn json_proxy_response_with_headers(
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
) -> ProxyCoreResult<ProxyCoreResponse> {
    let body = serde_json::to_vec(&body).map_err(|error| {
        ProxyCoreError::Internal(format!("failed to serialize JSON response: {error}"))
    })?;
    Ok(ProxyCoreResponse::with_body(
        status,
        headers,
        ProxyResponseBody::bytes(Bytes::from(body)),
    ))
}

/// Build a host-neutral response for a transformed SSE byte stream.
pub fn transformed_sse_proxy_response(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
) -> ProxyCoreResponse {
    ProxyCoreResponse::with_body(
        StatusCode::OK,
        transformed_sse_response_headers(),
        ProxyResponseBody::stream(stream),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt as _;
    use http::{header, HeaderName, HeaderValue};
    use serde_json::json;

    #[test]
    fn response_build_error_context_messages_preserve_host_contracts() {
        let cases = [
            (
                ProxyResponseBuildErrorContext::TaggedStreaming { tag: "Usage" },
                "[Usage] 构建流式响应失败",
            ),
            (
                ProxyResponseBuildErrorContext::TaggedResponse { tag: "JSON" },
                "[JSON] 构建响应失败",
            ),
            (
                ProxyResponseBuildErrorContext::ClaudeSse,
                "[Claude] 构建 SSE 响应失败",
            ),
            (
                ProxyResponseBuildErrorContext::ClaudeResponse,
                "[Claude] 构建响应失败",
            ),
            (
                ProxyResponseBuildErrorContext::CodexSse,
                "[Codex] 构建 SSE 响应失败",
            ),
            (
                ProxyResponseBuildErrorContext::CodexResponses,
                "[Codex] 构建 Responses 响应失败",
            ),
            (
                ProxyResponseBuildErrorContext::CodexResponsesError,
                "[Codex] 构建 Responses 错误响应失败",
            ),
            (
                ProxyResponseBuildErrorContext::CodexProxyError,
                "[Codex] 构建代理错误响应失败",
            ),
        ];

        for (context, expected) in cases {
            assert_eq!(context.message(), expected);
        }
    }

    #[test]
    fn response_build_error_context_internal_error_prefixes_preserve_host_contracts() {
        let cases = [
            (
                ProxyResponseBuildErrorContext::TaggedStreaming { tag: "Usage" },
                "Failed to build streaming response",
            ),
            (
                ProxyResponseBuildErrorContext::TaggedResponse { tag: "JSON" },
                "Failed to build response",
            ),
            (
                ProxyResponseBuildErrorContext::ClaudeSse,
                "Failed to build response",
            ),
            (
                ProxyResponseBuildErrorContext::CodexProxyError,
                "Failed to build response",
            ),
        ];

        for (context, expected) in cases {
            assert_eq!(context.internal_error_prefix(), expected);
        }
    }

    #[test]
    fn response_build_failure_context_log_prefixes_preserve_host_contracts() {
        let cases = [
            (
                ProxyResponseBuildFailureContext::ClaudeJson,
                "[Claude] 构造 JSON 响应失败",
            ),
            (
                ProxyResponseBuildFailureContext::CodexResponses,
                "[Codex] 构造 Responses 响应失败",
            ),
            (
                ProxyResponseBuildFailureContext::CodexResponsesError,
                "[Codex] 构造 Responses 错误体失败",
            ),
            (
                ProxyResponseBuildFailureContext::CodexProxyError,
                "[Codex] 构造代理错误响应失败",
            ),
        ];

        for (context, expected) in cases {
            assert_eq!(context.log_prefix(), expected);
        }
    }

    #[test]
    fn rebuilt_json_response_serializes_body_and_rebuilds_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("999"));
        headers.insert(header::CONNECTION, HeaderValue::from_static("x-debug"));
        headers.insert("x-debug", HeaderValue::from_static("1"));

        let response =
            rebuilt_json_proxy_response(StatusCode::CREATED, headers, json!({"ok": true})).unwrap();

        assert_eq!(response.status, StatusCode::CREATED);
        assert_eq!(
            response.headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
        assert!(!response.headers.contains_key(header::CONTENT_ENCODING));
        assert!(!response.headers.contains_key(header::CONTENT_LENGTH));
        assert!(!response.headers.contains_key(header::CONNECTION));
        assert!(!response.headers.contains_key("x-debug"));

        match response.body {
            ProxyResponseBody::Bytes(body) => {
                assert_eq!(body, Bytes::from_static(br#"{"ok":true}"#))
            }
            other => panic!("expected bytes body, got {other:?}"),
        }
    }

    #[test]
    fn json_response_serializes_body_with_json_headers() {
        let response =
            json_proxy_response(StatusCode::BAD_GATEWAY, json!({"error": "upstream"})).unwrap();

        assert_eq!(response.status, StatusCode::BAD_GATEWAY);
        assert_eq!(
            response.headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );

        match response.body {
            ProxyResponseBody::Bytes(body) => {
                assert_eq!(body, Bytes::from_static(br#"{"error":"upstream"}"#))
            }
            other => panic!("expected bytes body, got {other:?}"),
        }
    }

    #[test]
    fn passthrough_bytes_response_strips_hop_by_hop_headers_and_preserves_body() {
        let mut headers = HeaderMap::new();
        let keep_alive = HeaderName::from_static("keep-alive");
        headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
        headers.insert(keep_alive.clone(), HeaderValue::from_static("timeout=5"));
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("2"));
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));

        let response = passthrough_bytes_proxy_response(
            StatusCode::ACCEPTED,
            headers,
            Bytes::from_static(b"ok"),
        );

        assert_eq!(response.status, StatusCode::ACCEPTED);
        assert!(!response.headers.contains_key(header::CONNECTION));
        assert!(!response.headers.contains_key(keep_alive));
        assert_eq!(
            response.headers.get(header::CONTENT_LENGTH),
            Some(&HeaderValue::from_static("2"))
        );
        assert_eq!(
            response.headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/plain"))
        );

        match response.body {
            ProxyResponseBody::Bytes(body) => assert_eq!(body, Bytes::from_static(b"ok")),
            other => panic!("expected bytes body, got {other:?}"),
        }
    }

    #[test]
    fn passthrough_stream_response_strips_connection_listed_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONNECTION, HeaderValue::from_static("x-debug"));
        headers.insert("x-debug", HeaderValue::from_static("1"));
        headers.insert("x-keep", HeaderValue::from_static("yes"));
        let stream =
            futures::stream::once(async { Ok::<_, std::io::Error>(Bytes::from_static(b"chunk")) });

        let response = passthrough_stream_proxy_response(StatusCode::OK, headers, stream);

        assert!(!response.headers.contains_key(header::CONNECTION));
        assert!(!response.headers.contains_key("x-debug"));
        assert_eq!(
            response.headers.get("x-keep"),
            Some(&HeaderValue::from_static("yes"))
        );

        match response.body {
            ProxyResponseBody::Stream(mut stream) => {
                let chunk = futures::executor::block_on(stream.next())
                    .expect("stream item")
                    .expect("stream chunk");
                assert_eq!(chunk, Bytes::from_static(b"chunk"));
            }
            other => panic!("expected stream body, got {other:?}"),
        }
    }

    #[test]
    fn transformed_sse_response_wraps_stream_with_fixed_headers() {
        let stream = futures::stream::once(async {
            Ok::<_, std::io::Error>(Bytes::from_static(b"data: ok\n\n"))
        });

        let response = transformed_sse_proxy_response(stream);

        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(
            response.headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/event-stream"))
        );
        assert_eq!(
            response.headers.get(header::CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-cache"))
        );

        match response.body {
            ProxyResponseBody::Stream(mut stream) => {
                let chunk = futures::executor::block_on(stream.next())
                    .expect("stream item")
                    .expect("stream chunk");
                assert_eq!(chunk, Bytes::from_static(b"data: ok\n\n"));
            }
            other => panic!("expected stream body, got {other:?}"),
        }
    }
}
