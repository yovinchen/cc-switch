const REQUEST_HEADERS_STRIPPED_BEFORE_UPSTREAM: &[&str] = &[
    "content-length",
    "transfer-encoding",
    "x-forwarded-host",
    "x-forwarded-port",
    "x-forwarded-proto",
    "forwarded",
    "cf-connecting-ip",
    "cf-ipcountry",
    "cf-ray",
    "cf-visitor",
    "true-client-ip",
    "fastly-client-ip",
    "x-azure-clientip",
    "x-azure-fdid",
    "x-azure-ref",
    "akamai-origin-hop",
    "x-akamai-config-log-detail",
    "x-request-id",
    "x-correlation-id",
    "x-trace-id",
    "x-amzn-trace-id",
    "x-b3-traceid",
    "x-b3-spanid",
    "x-b3-parentspanid",
    "x-b3-sampled",
    "traceparent",
    "tracestate",
];

pub const CLAUDE_CODE_BETA: &str = "claude-code-20250219";
pub const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

pub fn should_send_anthropic_request_headers(
    adapter_name: &str,
    resolved_claude_api_format: Option<&str>,
) -> bool {
    adapter_name == "Claude" && matches!(resolved_claude_api_format, Some("anthropic"))
}

pub fn anthropic_beta_header_value(existing_beta: Option<&str>) -> String {
    match existing_beta {
        Some(value) if value.contains(CLAUDE_CODE_BETA) => value.to_string(),
        Some(value) if !value.is_empty() => format!("{CLAUDE_CODE_BETA},{value}"),
        _ => CLAUDE_CODE_BETA.to_string(),
    }
}

pub fn build_codex_oauth_session_headers(
    session_id: &str,
) -> Vec<(http::HeaderName, http::HeaderValue)> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Vec::new();
    }

    let mut headers = Vec::new();
    if let Ok(value) = http::HeaderValue::from_str(session_id) {
        headers.push((http::HeaderName::from_static("session_id"), value.clone()));
        headers.push((http::HeaderName::from_static("x-client-request-id"), value));
    }

    let window_id = format!("{session_id}:0");
    if let Ok(value) = http::HeaderValue::from_str(&window_id) {
        headers.push((http::HeaderName::from_static("x-codex-window-id"), value));
    }

    headers
}

pub fn should_strip_forwarded_request_header(name: &str) -> bool {
    REQUEST_HEADERS_STRIPPED_BEFORE_UPSTREAM
        .iter()
        .any(|header| name.eq_ignore_ascii_case(header))
}

pub fn should_preserve_exact_request_header_case(
    adapter_name: &str,
    provider_is_codex_oauth: bool,
    is_copilot: bool,
    resolved_claude_api_format: Option<&str>,
) -> bool {
    if matches!(adapter_name, "Codex" | "Gemini") {
        return false;
    }

    if is_copilot || provider_is_codex_oauth {
        return false;
    }

    matches!(resolved_claude_api_format, None | Some("anthropic"))
}

#[cfg(test)]
mod tests {
    use super::{
        anthropic_beta_header_value, build_codex_oauth_session_headers,
        should_preserve_exact_request_header_case, should_send_anthropic_request_headers,
        should_strip_forwarded_request_header, CLAUDE_CODE_BETA, DEFAULT_ANTHROPIC_VERSION,
    };
    use http::{HeaderMap, HeaderValue};

    #[test]
    fn preserves_exact_header_case_for_native_claude_or_unknown_claude_format() {
        assert!(should_preserve_exact_request_header_case(
            "Claude",
            false,
            false,
            Some("anthropic"),
        ));
        assert!(should_preserve_exact_request_header_case(
            "Claude", false, false, None,
        ));
    }

    #[test]
    fn skips_exact_header_case_for_transformed_or_pooled_backends() {
        assert!(!should_preserve_exact_request_header_case(
            "Claude",
            false,
            false,
            Some("openai_responses"),
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Codex", false, false, None,
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Gemini", false, false, None,
        ));
    }

    #[test]
    fn skips_exact_header_case_for_managed_account_paths() {
        assert!(!should_preserve_exact_request_header_case(
            "Claude",
            true,
            false,
            Some("anthropic"),
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Claude",
            false,
            true,
            Some("anthropic"),
        ));
    }

    #[test]
    fn sends_anthropic_request_headers_only_for_native_claude() {
        assert!(should_send_anthropic_request_headers(
            "Claude",
            Some("anthropic"),
        ));
        assert!(!should_send_anthropic_request_headers(
            "Claude",
            Some("openai_responses"),
        ));
        assert!(!should_send_anthropic_request_headers(
            "Codex",
            Some("anthropic"),
        ));
        assert_eq!(DEFAULT_ANTHROPIC_VERSION, "2023-06-01");
    }

    #[test]
    fn anthropic_beta_header_value_preserves_or_prepends_claude_code_beta() {
        assert_eq!(
            anthropic_beta_header_value(None),
            CLAUDE_CODE_BETA.to_string()
        );
        assert_eq!(
            anthropic_beta_header_value(Some("other-beta")),
            format!("{CLAUDE_CODE_BETA},other-beta")
        );
        assert_eq!(
            anthropic_beta_header_value(Some("other-beta,claude-code-20250219")),
            "other-beta,claude-code-20250219"
        );
        assert_eq!(anthropic_beta_header_value(Some("")), CLAUDE_CODE_BETA);
    }

    #[test]
    fn strips_connection_tracing_and_cdn_request_headers_before_upstream() {
        for header in [
            "content-length",
            "transfer-encoding",
            "x-forwarded-host",
            "cf-connecting-ip",
            "x-request-id",
            "traceparent",
            "tracestate",
        ] {
            assert!(
                should_strip_forwarded_request_header(header),
                "expected {header} to be stripped"
            );
        }

        assert!(should_strip_forwarded_request_header("X-Forwarded-Proto"));
    }

    #[test]
    fn keeps_application_and_provider_headers_for_forwarder_policy() {
        for header in [
            "authorization",
            "accept-encoding",
            "anthropic-version",
            "anthropic-beta",
            "content-type",
            "user-agent",
        ] {
            assert!(
                !should_strip_forwarded_request_header(header),
                "expected {header} to stay available to forwarder policy"
            );
        }
    }

    #[test]
    fn codex_oauth_session_headers_match_codex_cache_identity() {
        let headers = build_codex_oauth_session_headers("session-123");
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.insert(name, value);
        }

        assert_eq!(
            map.get("session_id"),
            Some(&HeaderValue::from_static("session-123"))
        );
        assert_eq!(
            map.get("x-client-request-id"),
            Some(&HeaderValue::from_static("session-123"))
        );
        assert_eq!(
            map.get("x-codex-window-id"),
            Some(&HeaderValue::from_static("session-123:0"))
        );
    }

    #[test]
    fn codex_oauth_session_headers_trim_and_skip_empty_values() {
        assert!(build_codex_oauth_session_headers("  \n\t ").is_empty());

        let headers = build_codex_oauth_session_headers("  session-abc  ");
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.insert(name, value);
        }

        assert_eq!(
            map.get("session_id"),
            Some(&HeaderValue::from_static("session-abc"))
        );
        assert_eq!(
            map.get("x-codex-window-id"),
            Some(&HeaderValue::from_static("session-abc:0"))
        );
    }
}
