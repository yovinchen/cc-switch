use crate::{
    prepare_rebuilt_json_response_headers, ProxyCoreError, ProxyCoreResponse, ProxyCoreResult,
    ProxyResponseBody,
};
use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

/// Build a host-neutral response for a JSON body reconstructed by a transform.
pub fn rebuilt_json_proxy_response(
    status: StatusCode,
    mut headers: HeaderMap,
    body: Value,
) -> ProxyCoreResult<ProxyCoreResponse> {
    prepare_rebuilt_json_response_headers(&mut headers);
    let body = serde_json::to_vec(&body)
        .map_err(|error| ProxyCoreError::Internal(format!("failed to serialize JSON response: {error}")))?;

    Ok(ProxyCoreResponse::with_body(
        status,
        headers,
        ProxyResponseBody::bytes(Bytes::from(body)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
