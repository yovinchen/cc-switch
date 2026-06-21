use http::{header, HeaderMap, HeaderName, HeaderValue};

/// RFC 2616 / RFC 7230 hop-by-hop response headers that must not be forwarded.
const HOP_BY_HOP_RESPONSE_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "trailers",
    "transfer-encoding",
    "upgrade",
];

/// Remove response hop-by-hop headers and extension headers named by Connection.
pub fn strip_hop_by_hop_response_headers(headers: &mut HeaderMap) {
    let connection_listed_headers: Vec<HeaderName> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .filter_map(|name| HeaderName::from_bytes(name.as_bytes()).ok())
        .collect();

    for name in HOP_BY_HOP_RESPONSE_HEADERS {
        headers.remove(*name);
    }

    for name in connection_listed_headers {
        headers.remove(name);
    }
}

/// Remove entity headers that become invalid when a response body is rebuilt.
pub fn strip_entity_headers_for_rebuilt_body(headers: &mut HeaderMap) {
    headers.remove(header::CONTENT_ENCODING);
    headers.remove(header::CONTENT_LENGTH);
    headers.remove(header::TRANSFER_ENCODING);
}

/// Prepare upstream response headers for a rebuilt JSON response body.
pub fn prepare_rebuilt_json_response_headers(headers: &mut HeaderMap) {
    strip_entity_headers_for_rebuilt_body(headers);
    strip_hop_by_hop_response_headers(headers);
    headers.remove(header::CONTENT_TYPE);
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
}

/// Build headers for transformed SSE responses that no longer reuse upstream
/// response headers.
pub fn transformed_sse_response_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers
}

pub fn response_headers_indicate_sse(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|content_type| content_type.contains("text/event-stream"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_standard_hop_by_hop_response_headers() {
        let mut headers = HeaderMap::new();
        let keep_alive = HeaderName::from_static("keep-alive");
        headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
        headers.insert(keep_alive.clone(), HeaderValue::from_static("timeout=5"));
        headers.insert(header::TRANSFER_ENCODING, HeaderValue::from_static("chunked"));
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));

        strip_hop_by_hop_response_headers(&mut headers);

        assert!(!headers.contains_key(header::CONNECTION));
        assert!(!headers.contains_key(keep_alive));
        assert!(!headers.contains_key(header::TRANSFER_ENCODING));
        assert_eq!(
            headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
    }

    #[test]
    fn strips_connection_listed_extension_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONNECTION, HeaderValue::from_static("x-trace, x-debug"));
        headers.insert("x-trace", HeaderValue::from_static("trace-id"));
        headers.insert("x-debug", HeaderValue::from_static("1"));
        headers.insert("x-keep", HeaderValue::from_static("yes"));

        strip_hop_by_hop_response_headers(&mut headers);

        assert!(!headers.contains_key("x-trace"));
        assert!(!headers.contains_key("x-debug"));
        assert_eq!(headers.get("x-keep"), Some(&HeaderValue::from_static("yes")));
    }

    #[test]
    fn strips_entity_headers_for_rebuilt_body() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("24"));
        headers.insert(header::TRANSFER_ENCODING, HeaderValue::from_static("chunked"));
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));

        strip_entity_headers_for_rebuilt_body(&mut headers);

        assert!(!headers.contains_key(header::CONTENT_ENCODING));
        assert!(!headers.contains_key(header::CONTENT_LENGTH));
        assert!(!headers.contains_key(header::TRANSFER_ENCODING));
        assert_eq!(
            headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
    }

    #[test]
    fn prepares_rebuilt_json_response_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONNECTION, HeaderValue::from_static("x-debug"));
        headers.insert("x-debug", HeaderValue::from_static("1"));
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("24"));
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));

        prepare_rebuilt_json_response_headers(&mut headers);

        assert!(!headers.contains_key(header::CONNECTION));
        assert!(!headers.contains_key("x-debug"));
        assert!(!headers.contains_key(header::CONTENT_ENCODING));
        assert!(!headers.contains_key(header::CONTENT_LENGTH));
        assert_eq!(
            headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
    }

    #[test]
    fn transformed_sse_headers_are_stable() {
        let headers = transformed_sse_response_headers();

        assert_eq!(
            headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/event-stream"))
        );
        assert_eq!(
            headers.get(header::CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-cache"))
        );
    }

    #[test]
    fn response_headers_identify_sse_content_type() {
        let mut headers = HeaderMap::new();
        assert!(!response_headers_indicate_sse(&headers));

        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        assert!(!response_headers_indicate_sse(&headers));

        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        assert!(response_headers_indicate_sse(&headers));
    }
}
