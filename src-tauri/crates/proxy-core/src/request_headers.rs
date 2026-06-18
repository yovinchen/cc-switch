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

const COPILOT_FINGERPRINT_REQUEST_HEADERS: &[&str] = &[
    "user-agent",
    "editor-version",
    "editor-plugin-version",
    "copilot-integration-id",
    "x-github-api-version",
    "openai-intent",
    "x-initiator",
    "x-interaction-type",
    "x-interaction-id",
    "x-vscode-user-agent-library-version",
    "x-request-id",
    "x-agent-task-id",
];

pub const CLAUDE_CODE_BETA: &str = "claude-code-20250219";
pub const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct UpstreamRequestHeadersInput<'a> {
    pub inbound_headers: &'a http::HeaderMap,
    pub upstream_host: Option<&'a str>,
    pub auth_headers: &'a [(http::HeaderName, http::HeaderValue)],
    pub force_identity_encoding: bool,
    pub custom_user_agent: Option<&'a http::HeaderValue>,
    pub is_copilot: bool,
    pub should_send_anthropic_headers: bool,
    pub anthropic_beta_value: Option<&'a str>,
    pub codex_oauth_session_headers: &'a [(http::HeaderName, http::HeaderValue)],
    pub ensure_json_content_type: bool,
}

pub struct UpstreamAuthHeadersInput<'a> {
    pub base_auth_headers: &'a [(http::HeaderName, http::HeaderValue)],
    pub codex_oauth_account_id: Option<&'a str>,
    pub copilot_overrides: Option<CopilotAuthHeaderOverrides<'a>>,
}

#[derive(Clone, Copy)]
pub struct CopilotAuthHeaderOverrides<'a> {
    pub initiator: Option<&'a str>,
    pub is_subagent: bool,
    pub deterministic_request_id: Option<&'a str>,
    pub interaction_id: Option<&'a str>,
}

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

pub fn build_upstream_auth_headers(
    input: UpstreamAuthHeadersInput<'_>,
) -> Vec<(http::HeaderName, http::HeaderValue)> {
    let mut headers = Vec::with_capacity(input.base_auth_headers.len() + 2);

    for (name, value) in input.base_auth_headers {
        let mut value = value.clone();
        if let Some(overrides) = input.copilot_overrides {
            value = apply_copilot_auth_header_override(name, value, overrides);
        }
        headers.push((name.clone(), value));
    }

    if let Some(account_id) = input.codex_oauth_account_id {
        if let Ok(value) = http::HeaderValue::from_str(account_id) {
            headers.push((http::HeaderName::from_static("chatgpt-account-id"), value));
        }
    }

    if let Some(overrides) = input.copilot_overrides {
        if let Some(interaction_id) = overrides.interaction_id {
            if let Ok(value) = http::HeaderValue::from_str(interaction_id) {
                headers.push((http::HeaderName::from_static("x-interaction-id"), value));
            }
        }
    }

    headers
}

pub fn should_strip_forwarded_request_header(name: &str) -> bool {
    REQUEST_HEADERS_STRIPPED_BEFORE_UPSTREAM
        .iter()
        .any(|header| name.eq_ignore_ascii_case(header))
}

pub fn should_skip_copilot_fingerprint_request_header(is_copilot: bool, name: &str) -> bool {
    is_copilot
        && COPILOT_FINGERPRINT_REQUEST_HEADERS
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

pub fn build_upstream_request_headers(input: UpstreamRequestHeadersInput<'_>) -> http::HeaderMap {
    let mut ordered_headers = http::HeaderMap::new();
    let mut saw_auth = false;
    let mut saw_accept_encoding = false;
    let mut saw_user_agent = false;
    let mut saw_anthropic_beta = false;
    let mut saw_anthropic_version = false;

    for (key, value) in input.inbound_headers {
        let key_str = key.as_str();

        if key_str.eq_ignore_ascii_case("host") {
            if let Some(host) = input.upstream_host {
                if let Ok(value) = http::HeaderValue::from_str(host) {
                    ordered_headers.append(key.clone(), value);
                }
            }
            continue;
        }

        if should_strip_forwarded_request_header(key_str) {
            continue;
        }

        if is_upstream_auth_header(key_str) {
            if !saw_auth {
                saw_auth = true;
                append_header_pairs(&mut ordered_headers, input.auth_headers);
            }
            continue;
        }

        if key_str.eq_ignore_ascii_case("accept-encoding") {
            if !saw_accept_encoding {
                saw_accept_encoding = true;
                if input.force_identity_encoding {
                    ordered_headers.append(
                        http::header::ACCEPT_ENCODING,
                        http::HeaderValue::from_static("identity"),
                    );
                } else {
                    ordered_headers.append(key.clone(), value.clone());
                }
            }
            continue;
        }

        if !input.is_copilot && key_str.eq_ignore_ascii_case("user-agent") {
            if !saw_user_agent {
                saw_user_agent = true;
                if let Some(user_agent) = input.custom_user_agent {
                    ordered_headers.append(http::header::USER_AGENT, user_agent.clone());
                } else {
                    ordered_headers.append(key.clone(), value.clone());
                }
            }
            continue;
        }

        if key_str.eq_ignore_ascii_case("anthropic-beta") {
            if !saw_anthropic_beta {
                saw_anthropic_beta = true;
                append_header_from_str(
                    &mut ordered_headers,
                    http::HeaderName::from_static("anthropic-beta"),
                    input.anthropic_beta_value,
                );
            }
            continue;
        }

        if key_str.eq_ignore_ascii_case("anthropic-version") {
            if input.should_send_anthropic_headers {
                saw_anthropic_version = true;
                ordered_headers.append(key.clone(), value.clone());
            }
            continue;
        }

        if should_skip_copilot_fingerprint_request_header(input.is_copilot, key_str) {
            continue;
        }

        ordered_headers.append(key.clone(), value.clone());
    }

    if !saw_auth && !input.auth_headers.is_empty() {
        append_header_pairs(&mut ordered_headers, input.auth_headers);
    }

    if !saw_accept_encoding && input.force_identity_encoding {
        ordered_headers.append(
            http::header::ACCEPT_ENCODING,
            http::HeaderValue::from_static("identity"),
        );
    }

    if !input.is_copilot && !saw_user_agent {
        if let Some(user_agent) = input.custom_user_agent {
            ordered_headers.append(http::header::USER_AGENT, user_agent.clone());
        }
    }

    if !saw_anthropic_beta {
        append_header_from_str(
            &mut ordered_headers,
            http::HeaderName::from_static("anthropic-beta"),
            input.anthropic_beta_value,
        );
    }

    if input.should_send_anthropic_headers && !saw_anthropic_version {
        ordered_headers.append(
            "anthropic-version",
            http::HeaderValue::from_static(DEFAULT_ANTHROPIC_VERSION),
        );
    }

    for (name, value) in input.codex_oauth_session_headers {
        ordered_headers.insert(name.clone(), value.clone());
    }

    if input.ensure_json_content_type && !ordered_headers.contains_key(http::header::CONTENT_TYPE) {
        ordered_headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
    }

    ordered_headers
}

fn is_upstream_auth_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("authorization")
        || name.eq_ignore_ascii_case("x-api-key")
        || name.eq_ignore_ascii_case("x-goog-api-key")
}

fn apply_copilot_auth_header_override(
    name: &http::HeaderName,
    current: http::HeaderValue,
    overrides: CopilotAuthHeaderOverrides<'_>,
) -> http::HeaderValue {
    let name = name.as_str();
    if name.eq_ignore_ascii_case("x-initiator") {
        return header_value_from_optional_str(overrides.initiator).unwrap_or(current);
    }

    if name.eq_ignore_ascii_case("x-interaction-type") && overrides.is_subagent {
        return http::HeaderValue::from_static("conversation-subagent");
    }

    if name.eq_ignore_ascii_case("x-request-id") || name.eq_ignore_ascii_case("x-agent-task-id") {
        return header_value_from_optional_str(overrides.deterministic_request_id)
            .unwrap_or(current);
    }

    current
}

fn append_header_pairs(
    headers: &mut http::HeaderMap,
    pairs: &[(http::HeaderName, http::HeaderValue)],
) {
    for (name, value) in pairs {
        headers.append(name.clone(), value.clone());
    }
}

fn header_value_from_optional_str(value: Option<&str>) -> Option<http::HeaderValue> {
    value.and_then(|value| http::HeaderValue::from_str(value).ok())
}

fn append_header_from_str(
    headers: &mut http::HeaderMap,
    name: http::HeaderName,
    value: Option<&str>,
) {
    if let Some(value) = value {
        if let Ok(value) = http::HeaderValue::from_str(value) {
            headers.append(name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        anthropic_beta_header_value, build_codex_oauth_session_headers,
        build_upstream_auth_headers,
        build_upstream_request_headers,
        should_preserve_exact_request_header_case, should_send_anthropic_request_headers,
        should_skip_copilot_fingerprint_request_header, should_strip_forwarded_request_header,
        CopilotAuthHeaderOverrides, UpstreamAuthHeadersInput, UpstreamRequestHeadersInput,
        CLAUDE_CODE_BETA, DEFAULT_ANTHROPIC_VERSION,
    };
    use http::{header, HeaderMap, HeaderName, HeaderValue};

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
    fn skips_copilot_fingerprint_headers_only_for_copilot_requests() {
        for header in [
            "user-agent",
            "editor-version",
            "copilot-integration-id",
            "openai-intent",
            "x-initiator",
            "x-interaction-id",
            "x-agent-task-id",
        ] {
            assert!(
                should_skip_copilot_fingerprint_request_header(true, header),
                "expected {header} to be skipped for Copilot"
            );
        }

        assert!(should_skip_copilot_fingerprint_request_header(
            true,
            "X-GitHub-Api-Version"
        ));
        assert!(!should_skip_copilot_fingerprint_request_header(
            false,
            "user-agent"
        ));
        assert!(!should_skip_copilot_fingerprint_request_header(
            true,
            "authorization"
        ));
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

    #[test]
    fn builds_upstream_auth_headers_with_codex_account_id() {
        let base_auth_headers = vec![
            (
                HeaderName::from_static("authorization"),
                HeaderValue::from_static("Bearer token"),
            ),
            (
                HeaderName::from_static("originator"),
                HeaderValue::from_static("cc-switch"),
            ),
        ];

        let headers = build_upstream_auth_headers(UpstreamAuthHeadersInput {
            base_auth_headers: &base_auth_headers,
            codex_oauth_account_id: Some("account-123"),
            copilot_overrides: None,
        });

        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0], base_auth_headers[0]);
        assert_eq!(headers[1], base_auth_headers[1]);
        assert_eq!(
            headers[2],
            (
                HeaderName::from_static("chatgpt-account-id"),
                HeaderValue::from_static("account-123"),
            )
        );
    }

    #[test]
    fn skips_invalid_codex_account_id_auth_header() {
        let base_auth_headers = vec![(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer token"),
        )];

        let headers = build_upstream_auth_headers(UpstreamAuthHeadersInput {
            base_auth_headers: &base_auth_headers,
            codex_oauth_account_id: Some("bad\naccount"),
            copilot_overrides: None,
        });

        assert_eq!(headers, base_auth_headers);
    }

    #[test]
    fn builds_upstream_auth_headers_with_copilot_overrides() {
        let base_auth_headers = vec![
            (
                HeaderName::from_static("authorization"),
                HeaderValue::from_static("Bearer token"),
            ),
            (
                HeaderName::from_static("x-initiator"),
                HeaderValue::from_static("user"),
            ),
            (
                HeaderName::from_static("x-interaction-type"),
                HeaderValue::from_static("conversation"),
            ),
            (
                HeaderName::from_static("x-request-id"),
                HeaderValue::from_static("random-request"),
            ),
            (
                HeaderName::from_static("x-agent-task-id"),
                HeaderValue::from_static("random-task"),
            ),
        ];

        let headers = build_upstream_auth_headers(UpstreamAuthHeadersInput {
            base_auth_headers: &base_auth_headers,
            codex_oauth_account_id: None,
            copilot_overrides: Some(CopilotAuthHeaderOverrides {
                initiator: Some("agent"),
                is_subagent: true,
                deterministic_request_id: Some("deterministic-id"),
                interaction_id: Some("interaction-id"),
            }),
        });

        assert_eq!(
            headers[1],
            (
                HeaderName::from_static("x-initiator"),
                HeaderValue::from_static("agent"),
            )
        );
        assert_eq!(
            headers[2],
            (
                HeaderName::from_static("x-interaction-type"),
                HeaderValue::from_static("conversation-subagent"),
            )
        );
        assert_eq!(
            headers[3],
            (
                HeaderName::from_static("x-request-id"),
                HeaderValue::from_static("deterministic-id"),
            )
        );
        assert_eq!(
            headers[4],
            (
                HeaderName::from_static("x-agent-task-id"),
                HeaderValue::from_static("deterministic-id"),
            )
        );
        assert_eq!(
            headers[5],
            (
                HeaderName::from_static("x-interaction-id"),
                HeaderValue::from_static("interaction-id"),
            )
        );
    }

    #[test]
    fn copilot_auth_overrides_preserve_headers_without_enabled_values() {
        let base_auth_headers = vec![
            (
                HeaderName::from_static("x-initiator"),
                HeaderValue::from_static("user"),
            ),
            (
                HeaderName::from_static("x-interaction-type"),
                HeaderValue::from_static("conversation"),
            ),
            (
                HeaderName::from_static("x-request-id"),
                HeaderValue::from_static("random-request"),
            ),
        ];

        let headers = build_upstream_auth_headers(UpstreamAuthHeadersInput {
            base_auth_headers: &base_auth_headers,
            codex_oauth_account_id: None,
            copilot_overrides: Some(CopilotAuthHeaderOverrides {
                initiator: None,
                is_subagent: false,
                deterministic_request_id: None,
                interaction_id: None,
            }),
        });

        assert_eq!(headers, base_auth_headers);
    }

    #[test]
    fn builds_upstream_headers_with_auth_replacement_and_stripping() {
        let mut inbound = HeaderMap::new();
        inbound.insert("host", HeaderValue::from_static("localhost:3456"));
        inbound.insert("authorization", HeaderValue::from_static("Bearer inbound"));
        inbound.insert("content-length", HeaderValue::from_static("99"));
        inbound.insert("x-request-id", HeaderValue::from_static("trace"));
        inbound.insert("x-keep", HeaderValue::from_static("keep"));
        let auth_headers = vec![(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer upstream"),
        )];

        let headers = build_upstream_request_headers(UpstreamRequestHeadersInput {
            inbound_headers: &inbound,
            upstream_host: Some("api.example.com"),
            auth_headers: &auth_headers,
            force_identity_encoding: false,
            custom_user_agent: None,
            is_copilot: false,
            should_send_anthropic_headers: false,
            anthropic_beta_value: None,
            codex_oauth_session_headers: &[],
            ensure_json_content_type: true,
        });

        assert_eq!(
            headers.get(header::HOST),
            Some(&HeaderValue::from_static("api.example.com"))
        );
        assert_eq!(
            headers.get(header::AUTHORIZATION),
            Some(&HeaderValue::from_static("Bearer upstream"))
        );
        assert_eq!(headers.get("x-keep"), Some(&HeaderValue::from_static("keep")));
        assert!(headers.get(header::CONTENT_LENGTH).is_none());
        assert!(headers.get("x-request-id").is_none());
        assert_eq!(
            headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
    }

    #[test]
    fn builds_upstream_headers_with_identity_user_agent_and_anthropic_defaults() {
        let mut inbound = HeaderMap::new();
        inbound.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
        inbound.insert(header::USER_AGENT, HeaderValue::from_static("client"));
        inbound.insert("anthropic-beta", HeaderValue::from_static("other-beta"));
        let custom_user_agent = HeaderValue::from_static("cc-switch-test");
        let anthropic_beta = anthropic_beta_header_value(Some("other-beta"));

        let headers = build_upstream_request_headers(UpstreamRequestHeadersInput {
            inbound_headers: &inbound,
            upstream_host: None,
            auth_headers: &[],
            force_identity_encoding: true,
            custom_user_agent: Some(&custom_user_agent),
            is_copilot: false,
            should_send_anthropic_headers: true,
            anthropic_beta_value: Some(&anthropic_beta),
            codex_oauth_session_headers: &[],
            ensure_json_content_type: false,
        });

        assert_eq!(
            headers.get(header::ACCEPT_ENCODING),
            Some(&HeaderValue::from_static("identity"))
        );
        assert_eq!(
            headers.get(header::USER_AGENT),
            Some(&HeaderValue::from_static("cc-switch-test"))
        );
        assert_eq!(
            headers.get("anthropic-beta"),
            Some(&HeaderValue::from_static("claude-code-20250219,other-beta"))
        );
        assert_eq!(
            headers.get("anthropic-version"),
            Some(&HeaderValue::from_static(DEFAULT_ANTHROPIC_VERSION))
        );
    }

    #[test]
    fn builds_upstream_headers_with_copilot_and_codex_session_overrides() {
        let mut inbound = HeaderMap::new();
        inbound.insert(header::USER_AGENT, HeaderValue::from_static("client"));
        inbound.insert("x-agent-task-id", HeaderValue::from_static("old-task"));
        inbound.insert("x-safe", HeaderValue::from_static("safe"));
        let session_headers = build_codex_oauth_session_headers("session-123");

        let headers = build_upstream_request_headers(UpstreamRequestHeadersInput {
            inbound_headers: &inbound,
            upstream_host: None,
            auth_headers: &[],
            force_identity_encoding: false,
            custom_user_agent: Some(&HeaderValue::from_static("ignored-for-copilot")),
            is_copilot: true,
            should_send_anthropic_headers: false,
            anthropic_beta_value: None,
            codex_oauth_session_headers: &session_headers,
            ensure_json_content_type: false,
        });

        assert!(headers.get(header::USER_AGENT).is_none());
        assert!(headers.get("x-agent-task-id").is_none());
        assert_eq!(headers.get("x-safe"), Some(&HeaderValue::from_static("safe")));
        assert_eq!(
            headers.get("session_id"),
            Some(&HeaderValue::from_static("session-123"))
        );
        assert_eq!(
            headers.get("x-codex-window-id"),
            Some(&HeaderValue::from_static("session-123:0"))
        );
    }
}
