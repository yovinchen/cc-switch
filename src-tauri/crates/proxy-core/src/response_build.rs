use crate::{
    prepare_rebuilt_json_response_headers, transformed_sse_response_headers, ProxyCoreError,
    ProxyCoreResponse, ProxyCoreResult, ProxyResponseBody,
};
use bytes::Bytes;
use futures::Stream;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

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
    use http::{header, HeaderValue};
    use serde_json::json;

    #[test]
    fn rebuilt_json_response_serializes_body_and_rebuilds_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("999"));
        headers.insert(header::CONNECTION, HeaderValue::from_static("x-debug"));
        headers.insert("x-debug", HeaderValue::from_static("1"));

        let response =
            rebuilt_json_proxy_response(StatusCode::CREATED, headers, json!({"ok": true}))
                .unwrap();

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
            ProxyResponseBody::Bytes(body) => assert_eq!(body, Bytes::from_static(br#"{"ok":true}"#)),
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
