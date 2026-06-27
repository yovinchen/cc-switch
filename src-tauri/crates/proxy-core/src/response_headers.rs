use http::{header, HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

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

pub fn apply_channel_response_header_overrides(
    headers: &mut HeaderMap,
    response_overrides: &Value,
) -> Option<Vec<String>> {
    let overrides = response_overrides
        .get("headers")
        .or_else(|| response_overrides.get("headerOverrides"))
        .or_else(|| response_overrides.get("header_overrides"))
        .and_then(Value::as_object)?;
    if overrides.is_empty() {
        return None;
    }

    let mut applied_headers = Vec::new();
    for (name, value) in overrides {
        let Some(value) = value.as_str() else {
            continue;
        };
        let Ok(name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(value) = HeaderValue::from_str(value) else {
            continue;
        };
        headers.insert(name.clone(), value);
        applied_headers.push(name.to_string());
    }
    applied_headers.sort();

    (!applied_headers.is_empty()).then_some(applied_headers)
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

    #[test]
    fn applies_channel_response_header_overrides_from_model_route_policy() {
        let mut headers = HeaderMap::new();
        headers.insert("x-relay-tier", HeaderValue::from_static("old"));

        let applied = apply_channel_response_header_overrides(
            &mut headers,
            &serde_json::json!({
                "headers": {
                    "x-relay-tier": "paid",
                    "x-relay-model": "sonnet",
                    "x-bad": ["not", "string"]
                }
            }),
        )
        .expect("response header overrides");

        assert_eq!(
            headers.get("x-relay-tier"),
            Some(&HeaderValue::from_static("paid"))
        );
        assert_eq!(
            headers.get("x-relay-model"),
            Some(&HeaderValue::from_static("sonnet"))
        );
        assert!(!headers.contains_key("x-bad"));
        assert_eq!(applied, vec!["x-relay-model", "x-relay-tier"]);
    }

    #[test]
    fn skips_invalid_or_missing_channel_response_header_overrides() {
        let mut headers = HeaderMap::new();
        headers.insert("x-relay-tier", HeaderValue::from_static("old"));

        assert!(apply_channel_response_header_overrides(
            &mut headers,
            &serde_json::json!({"headers": {"bad name": "paid", "x-bad": "bad\r\nvalue"}})
        )
        .is_none());
        assert_eq!(
            headers.get("x-relay-tier"),
            Some(&HeaderValue::from_static("old"))
        );
        assert!(apply_channel_response_header_overrides(
            &mut headers,
            &serde_json::json!({"headers": {}})
        )
        .is_none());
        assert!(apply_channel_response_header_overrides(
            &mut headers,
            &serde_json::json!({"body": {"strip": ["metadata"]}})
        )
        .is_none());
    }
}
