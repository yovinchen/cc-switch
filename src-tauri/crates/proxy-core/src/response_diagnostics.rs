use crate::response_body::get_content_encoding;
use http::{HeaderMap, StatusCode};
use std::fmt;

/// Detect whether a body looks like Server-Sent Events text.
///
/// Call this only after JSON parsing has failed; valid JSON cannot start with
/// these prefixes.
pub fn body_looks_like_sse(body: &str) -> bool {
    let trimmed = body.trim_start_matches('\u{feff}').trim_start();
    ["data:", "event:", "id:", "retry:", ":"]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

/// Build a response-body diagnostics suffix with content headers and a snippet.
pub fn body_diagnostics_suffix(headers: &HeaderMap, body: &str) -> String {
    let header_str = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("<none>")
    };
    format!(
        "(content-type: {}; content-encoding: {}; body[..120]: '{}')",
        header_str("content-type"),
        header_str("content-encoding"),
        body_snippet(body, 120),
    )
}

pub fn response_headers_log_summary(headers: &HeaderMap) -> String {
    headers
        .iter()
        .map(|(key, value)| {
            let value_str = value.to_str().unwrap_or("<non-utf8>");
            format!("{key}={value_str}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseLogLevel {
    Debug,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseLogEvent {
    pub level: ResponseLogLevel,
    pub message: String,
}

pub fn non_streaming_response_received_log_event(
    tag: &str,
    status: StatusCode,
    body_len: usize,
    headers: &HeaderMap,
) -> ResponseLogEvent {
    ResponseLogEvent {
        level: ResponseLogLevel::Debug,
        message: format!(
            "[{tag}] 已接收上游响应体: status={}, bytes={}, headers={}",
            status.as_u16(),
            body_len,
            response_headers_log_summary(headers)
        ),
    }
}

pub fn streaming_response_received_log_events(
    tag: &str,
    status: StatusCode,
    headers: &HeaderMap,
) -> Vec<ResponseLogEvent> {
    let mut events = vec![ResponseLogEvent {
        level: ResponseLogLevel::Debug,
        message: format!(
            "[{tag}] 已接收上游流式响应: status={}, headers={}",
            status.as_u16(),
            response_headers_log_summary(headers)
        ),
    }];

    if let Some(encoding) = get_content_encoding(headers) {
        events.push(ResponseLogEvent {
            level: ResponseLogLevel::Warn,
            message: format!(
                "[{tag}] 流式响应含 content-encoding={encoding}，SSE 解析可能失败。\
                 上游在 accept-encoding 透传后压缩了 SSE 流。"
            ),
        });
    }

    events
}

pub fn non_streaming_response_body_log_event(tag: &str, body: &[u8]) -> ResponseLogEvent {
    ResponseLogEvent {
        level: ResponseLogLevel::Debug,
        message: format!("[{tag}] 上游响应体内容: {}", String::from_utf8_lossy(body)),
    }
}

/// Append body diagnostics to an upstream SSE aggregation fallback error message.
pub fn aggregate_fallback_diagnostics_message(
    base_message: &str,
    headers: &HeaderMap,
    body: &str,
) -> String {
    format!("{base_message} {}", body_diagnostics_suffix(headers, body))
}

/// Build an upstream JSON parse error message with response-body diagnostics.
pub fn upstream_body_parse_error_message(
    prefix: &str,
    error: impl fmt::Display,
    headers: &HeaderMap,
    body: &str,
) -> String {
    format!(
        "{prefix}: {error} {}",
        body_diagnostics_suffix(headers, body)
    )
}

/// Return a single-line snippet of the first `max_chars` chars.
///
/// Carriage returns are dropped, line feeds are rendered as literal `\n`, and
/// other control characters are replaced with U+FFFD.
pub fn body_snippet(body: &str, max_chars: usize) -> String {
    let mut snippet = String::new();
    for c in body.chars().take(max_chars) {
        match c {
            '\n' => snippet.push_str("\\n"),
            '\r' => {}
            c if c.is_control() => snippet.push('\u{FFFD}'),
            c => snippet.push(c),
        }
    }
    if body.chars().nth(max_chars).is_some() {
        snippet.push('…');
    }
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;

    #[test]
    fn detects_unlabeled_sse_prefixes() {
        assert!(body_looks_like_sse("data: {\"id\":\"1\"}\n\n"));
        assert!(body_looks_like_sse("event: message\ndata: {}\n\n"));
        assert!(body_looks_like_sse("id: 1\ndata: {}\n\n"));
        assert!(body_looks_like_sse("retry: 3000\ndata: {}\n\n"));
        assert!(body_looks_like_sse(
            ": OPENROUTER PROCESSING\n\ndata: {}\n\n"
        ));
        assert!(body_looks_like_sse("\u{feff}\n  data: {}\n\n"));

        assert!(!body_looks_like_sse("<html><body>blocked</body></html>"));
        assert!(!body_looks_like_sse("Bad Gateway"));
        assert!(!body_looks_like_sse(""));
    }

    #[test]
    fn snippet_sanitizes_controls_and_truncates() {
        assert_eq!(
            body_snippet("<html>\r\nblocked\u{0}</html>", 120),
            "<html>\\nblocked\u{FFFD}</html>"
        );
        let long = "a".repeat(200);
        let snippet = body_snippet(&long, 120);
        assert_eq!(snippet.chars().count(), 121);
        assert!(snippet.ends_with('…'));
    }

    #[test]
    fn diagnostics_suffix_includes_headers_and_snippet() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "text/html".parse().unwrap());
        headers.insert("content-encoding", "gzip".parse().unwrap());

        let suffix = body_diagnostics_suffix(&headers, "<html>\nblocked</html>");

        assert!(suffix.contains("content-type: text/html"), "{suffix}");
        assert!(suffix.contains("content-encoding: gzip"), "{suffix}");
        assert!(suffix.contains("<html>\\nblocked</html>"), "{suffix}");
    }

    #[test]
    fn diagnostics_suffix_marks_missing_headers() {
        let suffix = body_diagnostics_suffix(&HeaderMap::new(), "Bad Gateway");

        assert!(suffix.contains("content-type: <none>"), "{suffix}");
        assert!(suffix.contains("content-encoding: <none>"), "{suffix}");
        assert!(suffix.contains("Bad Gateway"), "{suffix}");
    }

    #[test]
    fn response_headers_log_summary_formats_header_pairs() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert(
            "x-binary",
            http::HeaderValue::from_bytes(&[0xff, b'a']).unwrap(),
        );

        let summary = response_headers_log_summary(&headers);

        assert!(
            summary.contains("content-type=application/json"),
            "{summary}"
        );
        assert!(summary.contains("x-binary=<non-utf8>"), "{summary}");
    }

    #[test]
    fn response_log_events_preserve_host_contracts() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "text/event-stream".parse().unwrap());
        headers.insert("content-encoding", "gzip".parse().unwrap());

        let received =
            non_streaming_response_received_log_event("REQ", StatusCode::CREATED, 12, &headers);
        assert_eq!(received.level, ResponseLogLevel::Debug);
        assert!(received.message.contains("[REQ] 已接收上游响应体"));
        assert!(received.message.contains("status=201"));
        assert!(received.message.contains("bytes=12"));
        assert!(
            received.message.contains("content-encoding=gzip"),
            "{}",
            received.message
        );

        let streaming = streaming_response_received_log_events("REQ", StatusCode::OK, &headers);
        assert_eq!(streaming.len(), 2);
        assert_eq!(streaming[0].level, ResponseLogLevel::Debug);
        assert_eq!(
            streaming[0].message,
            "[REQ] 已接收上游流式响应: status=200, headers=content-type=text/event-stream, content-encoding=gzip"
        );
        assert_eq!(streaming[1].level, ResponseLogLevel::Warn);
        assert_eq!(
            streaming[1].message,
            "[REQ] 流式响应含 content-encoding=gzip，SSE 解析可能失败。上游在 accept-encoding 透传后压缩了 SSE 流。"
        );

        let body = non_streaming_response_body_log_event("REQ", b"hello");
        assert_eq!(body.level, ResponseLogLevel::Debug);
        assert_eq!(body.message, "[REQ] 上游响应体内容: hello");
    }

    #[test]
    fn aggregate_fallback_message_appends_diagnostics() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());

        let message = aggregate_fallback_diagnostics_message(
            "No chat completion choices in upstream SSE",
            &headers,
            "data: {}\n\n",
        );

        assert!(
            message.starts_with("No chat completion choices in upstream SSE "),
            "{message}"
        );
        assert!(
            message.contains("content-type: application/json"),
            "{message}"
        );
        assert!(
            message.contains("body[..120]: 'data: {}\\n\\n'"),
            "{message}"
        );
    }

    #[test]
    fn upstream_parse_error_message_includes_error_and_diagnostics() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "text/html".parse().unwrap());
        headers.insert("content-encoding", "gzip".parse().unwrap());
        let parse_err = serde_json::from_str::<serde_json::Value>("<html>").unwrap_err();

        let message = upstream_body_parse_error_message(
            "Failed to parse upstream response",
            parse_err,
            &headers,
            "<html>\nblocked</html>",
        );

        assert!(
            message.contains("Failed to parse upstream response"),
            "{message}"
        );
        assert!(message.contains("content-type: text/html"), "{message}");
        assert!(message.contains("content-encoding: gzip"), "{message}");
        assert!(message.contains("<html>\\nblocked</html>"), "{message}");
    }
}
