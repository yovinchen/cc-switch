use crate::error::{ProxyCoreError, ProxyCoreResult};
use crate::ports::AuthInfo;
use crate::provider_auth::{ProviderAuthInfo, ProviderAuthStrategy};
use serde_json::Value;

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
    pub channel_header_overrides: Option<&'a Value>,
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

pub struct CopilotAuthHeadersInput<'a> {
    pub api_key: &'a str,
    pub request_id: &'a str,
    pub editor_version: &'a str,
    pub editor_plugin_version: &'a str,
    pub integration_id: &'a str,
    pub user_agent: &'a str,
    pub github_api_version: &'a str,
}

pub struct ClaudeProviderAuthHeadersInput<'a> {
    pub auth: &'a ProviderAuthInfo,
    pub copilot_request_id: &'a str,
    pub copilot_editor_version: &'a str,
    pub copilot_editor_plugin_version: &'a str,
    pub copilot_integration_id: &'a str,
    pub copilot_user_agent: &'a str,
    pub copilot_github_api_version: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeAuthHeaderKind {
    AnthropicApiKey,
    Bearer,
    GoogleApiKey,
    GoogleOAuth,
    CodexOAuth,
}

pub fn claude_auth_header_kind_for_provider_strategy(
    strategy: ProviderAuthStrategy,
) -> Option<ClaudeAuthHeaderKind> {
    match strategy {
        ProviderAuthStrategy::Anthropic => Some(ClaudeAuthHeaderKind::AnthropicApiKey),
        ProviderAuthStrategy::ClaudeAuth | ProviderAuthStrategy::Bearer => {
            Some(ClaudeAuthHeaderKind::Bearer)
        }
        ProviderAuthStrategy::Google => Some(ClaudeAuthHeaderKind::GoogleApiKey),
        ProviderAuthStrategy::GoogleOAuth => Some(ClaudeAuthHeaderKind::GoogleOAuth),
        ProviderAuthStrategy::CodexOAuth => Some(ClaudeAuthHeaderKind::CodexOAuth),
        ProviderAuthStrategy::GitHubCopilot => None,
    }
}

pub fn should_send_anthropic_request_headers(
    adapter_name: &str,
    resolved_claude_api_format: Option<&str>,
) -> bool {
    adapter_name == "Claude" && matches!(resolved_claude_api_format, Some("anthropic"))
}

pub fn is_official_codex_client_user_agent(user_agent: &str) -> bool {
    let version = user_agent
        .strip_prefix("codex_vscode/")
        .or_else(|| user_agent.strip_prefix("codex_cli_rs/"));

    version
        .and_then(|value| value.as_bytes().first())
        .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
}

/// Build an HTTP `HeaderValue` from user-provided credential material.
///
/// Invalid bytes (control characters, CR/LF, non-ASCII where disallowed by
/// `http`) become an auth error instead of letting host adapters unwrap/panic.
pub fn auth_header_value(value: &str) -> ProxyCoreResult<http::HeaderValue> {
    http::HeaderValue::from_str(value)
        .map_err(|error| ProxyCoreError::Auth(format!("invalid auth header value: {error}")))
}

pub fn build_codex_bearer_auth_headers(
    api_key: &str,
) -> ProxyCoreResult<Vec<(http::HeaderName, http::HeaderValue)>> {
    let bearer = format!("Bearer {api_key}");
    Ok(vec![(
        http::HeaderName::from_static("authorization"),
        auth_header_value(&bearer)?,
    )])
}

pub fn build_auth_provider_headers(
    auth: &AuthInfo,
) -> ProxyCoreResult<Option<Vec<(http::HeaderName, http::HeaderValue)>>> {
    if auth.headers.is_empty() {
        return Ok(None);
    }

    let mut headers = Vec::with_capacity(auth.headers.len());
    for (name, value) in &auth.headers {
        let name = http::HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
            ProxyCoreError::InvalidRequest(format!("invalid AuthProvider header name: {error}"))
        })?;
        let value = http::HeaderValue::from_str(value).map_err(|error| {
            ProxyCoreError::InvalidRequest(format!("invalid AuthProvider header value: {error}"))
        })?;
        headers.push((name, value));
    }
    Ok(Some(headers))
}

pub fn build_codex_provider_auth_headers(
    auth: &ProviderAuthInfo,
) -> ProxyCoreResult<Vec<(http::HeaderName, http::HeaderValue)>> {
    build_codex_bearer_auth_headers(&auth.api_key)
}

pub fn build_gemini_auth_headers(
    api_key: &str,
    access_token: Option<&str>,
    use_oauth: bool,
) -> ProxyCoreResult<Vec<(http::HeaderName, http::HeaderValue)>> {
    if use_oauth {
        let token = access_token.unwrap_or(api_key);
        return Ok(vec![
            (
                http::HeaderName::from_static("authorization"),
                auth_header_value(&format!("Bearer {token}"))?,
            ),
            (
                http::HeaderName::from_static("x-goog-api-client"),
                http::HeaderValue::from_static("GeminiCLI/1.0"),
            ),
        ]);
    }

    Ok(vec![(
        http::HeaderName::from_static("x-goog-api-key"),
        auth_header_value(api_key)?,
    )])
}

pub fn build_gemini_provider_auth_headers(
    auth: &ProviderAuthInfo,
) -> ProxyCoreResult<Vec<(http::HeaderName, http::HeaderValue)>> {
    build_gemini_auth_headers(
        &auth.api_key,
        auth.access_token.as_deref(),
        matches!(auth.strategy, ProviderAuthStrategy::GoogleOAuth),
    )
}

pub fn build_claude_auth_headers(
    kind: ClaudeAuthHeaderKind,
    api_key: &str,
    access_token: Option<&str>,
) -> ProxyCoreResult<Vec<(http::HeaderName, http::HeaderValue)>> {
    match kind {
        ClaudeAuthHeaderKind::AnthropicApiKey => Ok(vec![(
            http::HeaderName::from_static("x-api-key"),
            auth_header_value(api_key)?,
        )]),
        ClaudeAuthHeaderKind::Bearer => build_codex_bearer_auth_headers(api_key),
        ClaudeAuthHeaderKind::GoogleApiKey => build_gemini_auth_headers(api_key, None, false),
        ClaudeAuthHeaderKind::GoogleOAuth => build_gemini_auth_headers(api_key, access_token, true),
        ClaudeAuthHeaderKind::CodexOAuth => {
            let mut headers = build_codex_bearer_auth_headers(api_key)?;
            headers.push((
                http::HeaderName::from_static("originator"),
                http::HeaderValue::from_static("cc-switch"),
            ));
            Ok(headers)
        }
    }
}

pub fn build_claude_provider_auth_headers(
    input: ClaudeProviderAuthHeadersInput<'_>,
) -> ProxyCoreResult<Vec<(http::HeaderName, http::HeaderValue)>> {
    if let Some(kind) = claude_auth_header_kind_for_provider_strategy(input.auth.strategy) {
        return build_claude_auth_headers(
            kind,
            &input.auth.api_key,
            input.auth.access_token.as_deref(),
        );
    }

    match input.auth.strategy {
        ProviderAuthStrategy::GitHubCopilot => {
            build_copilot_auth_headers(CopilotAuthHeadersInput {
                api_key: &input.auth.api_key,
                request_id: input.copilot_request_id,
                editor_version: input.copilot_editor_version,
                editor_plugin_version: input.copilot_editor_plugin_version,
                integration_id: input.copilot_integration_id,
                user_agent: input.copilot_user_agent,
                github_api_version: input.copilot_github_api_version,
            })
        }
        unsupported => Err(ProxyCoreError::Auth(format!(
            "unsupported Claude provider auth strategy: {unsupported:?}"
        ))),
    }
}

pub fn build_copilot_auth_headers(
    input: CopilotAuthHeadersInput<'_>,
) -> ProxyCoreResult<Vec<(http::HeaderName, http::HeaderValue)>> {
    let bearer = format!("Bearer {}", input.api_key);
    Ok(vec![
        (
            http::HeaderName::from_static("authorization"),
            auth_header_value(&bearer)?,
        ),
        (
            http::HeaderName::from_static("editor-version"),
            auth_header_value(input.editor_version)?,
        ),
        (
            http::HeaderName::from_static("editor-plugin-version"),
            auth_header_value(input.editor_plugin_version)?,
        ),
        (
            http::HeaderName::from_static("copilot-integration-id"),
            auth_header_value(input.integration_id)?,
        ),
        (
            http::HeaderName::from_static("user-agent"),
            auth_header_value(input.user_agent)?,
        ),
        (
            http::HeaderName::from_static("x-github-api-version"),
            auth_header_value(input.github_api_version)?,
        ),
        (
            http::HeaderName::from_static("openai-intent"),
            http::HeaderValue::from_static("conversation-agent"),
        ),
        (
            http::HeaderName::from_static("x-initiator"),
            http::HeaderValue::from_static("user"),
        ),
        (
            http::HeaderName::from_static("x-interaction-type"),
            http::HeaderValue::from_static("conversation-agent"),
        ),
        (
            http::HeaderName::from_static("x-vscode-user-agent-library-version"),
            http::HeaderValue::from_static("electron-fetch"),
        ),
        (
            http::HeaderName::from_static("x-request-id"),
            auth_header_value(input.request_id)?,
        ),
        (
            http::HeaderName::from_static("x-agent-task-id"),
            auth_header_value(input.request_id)?,
        ),
    ])
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

pub fn build_codex_oauth_session_headers_for_forwarder(
    should_send_session_headers: bool,
    session_client_provided: bool,
    session_id: &str,
) -> Vec<(http::HeaderName, http::HeaderValue)> {
    if should_send_session_headers && session_client_provided {
        build_codex_oauth_session_headers(session_id)
    } else {
        Vec::new()
    }
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

pub fn upstream_host_header_from_url(url: &str) -> Option<String> {
    url.parse::<http::Uri>()
        .ok()
        .and_then(|uri| uri.authority().map(|authority| authority.to_string()))
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

    apply_channel_header_overrides(&mut ordered_headers, input.channel_header_overrides);

    if input.ensure_json_content_type && !ordered_headers.contains_key(http::header::CONTENT_TYPE) {
        ordered_headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
    }

    ordered_headers
}

pub fn apply_channel_header_overrides(
    headers: &mut http::HeaderMap,
    header_overrides: Option<&Value>,
) {
    let Some(overrides) = header_overrides.and_then(Value::as_object) else {
        return;
    };

    for (name, value) in overrides {
        let name = name.trim();
        if !is_channel_header_override_allowed(name) {
            continue;
        }
        let Ok(name) = http::HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Some(value) = scalar_header_value(value) else {
            continue;
        };
        let Ok(value) = http::HeaderValue::from_str(&value) else {
            continue;
        };

        headers.insert(name, value);
    }
}

fn is_channel_header_override_allowed(name: &str) -> bool {
    !name.is_empty()
        && !name.eq_ignore_ascii_case("host")
        && !is_upstream_auth_header(name)
        && !should_strip_forwarded_request_header(name)
}

fn scalar_header_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
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
        anthropic_beta_header_value, auth_header_value, build_auth_provider_headers,
        build_claude_auth_headers, build_claude_provider_auth_headers,
        build_codex_bearer_auth_headers, build_codex_oauth_session_headers,
        build_codex_oauth_session_headers_for_forwarder, build_codex_provider_auth_headers,
        build_copilot_auth_headers, build_gemini_auth_headers, build_gemini_provider_auth_headers,
        build_upstream_auth_headers, build_upstream_request_headers,
        claude_auth_header_kind_for_provider_strategy, is_official_codex_client_user_agent,
        should_preserve_exact_request_header_case, should_send_anthropic_request_headers,
        should_skip_copilot_fingerprint_request_header, should_strip_forwarded_request_header,
        upstream_host_header_from_url, ClaudeAuthHeaderKind, ClaudeProviderAuthHeadersInput,
        CopilotAuthHeaderOverrides, CopilotAuthHeadersInput, UpstreamAuthHeadersInput,
        UpstreamRequestHeadersInput, CLAUDE_CODE_BETA, DEFAULT_ANTHROPIC_VERSION,
    };
    use crate::error::ProxyCoreError;
    use crate::ports::AuthInfo;
    use crate::provider_auth::{ProviderAuthInfo, ProviderAuthStrategy};
    use http::{header, HeaderMap, HeaderName, HeaderValue};
    use serde_json::json;

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
    fn detects_official_codex_client_user_agent_prefixes() {
        assert!(is_official_codex_client_user_agent("codex_vscode/1.0.0"));
        assert!(is_official_codex_client_user_agent("codex_cli_rs/0.5.2"));
        assert!(is_official_codex_client_user_agent(
            "codex_vscode/1.0.0 extra"
        ));
        assert!(!is_official_codex_client_user_agent("Mozilla/5.0"));
        assert!(!is_official_codex_client_user_agent(
            "some codex_vscode/1.0.0"
        ));
        assert!(!is_official_codex_client_user_agent("codex_other/1.0.0"));
        assert!(!is_official_codex_client_user_agent("codex_vscode/"));
        assert!(!is_official_codex_client_user_agent("codex_cli_rs/x.y.z"));
    }

    #[test]
    fn builds_codex_bearer_auth_headers() {
        let headers = build_codex_bearer_auth_headers("sk-codex-test").unwrap();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(
            headers[0].1,
            HeaderValue::from_static("Bearer sk-codex-test")
        );

        let error =
            build_codex_bearer_auth_headers("bad\r\nx-evil: 1").expect_err("invalid header");
        assert!(matches!(error, ProxyCoreError::Auth(_)));
    }

    #[test]
    fn builds_auth_provider_headers_from_core_auth_info() {
        let empty = AuthInfo {
            headers: Vec::new(),
            account_ref: None,
            metadata: Default::default(),
        };
        assert_eq!(build_auth_provider_headers(&empty).unwrap(), None);

        let auth = AuthInfo {
            headers: vec![(
                "authorization".to_string(),
                "Bearer channel-token".to_string(),
            )],
            account_ref: Some("channel-key:primary".to_string()),
            metadata: Default::default(),
        };
        let headers = build_auth_provider_headers(&auth).unwrap().unwrap();
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(
            headers[0].1,
            HeaderValue::from_static("Bearer channel-token")
        );
    }

    #[test]
    fn rejects_invalid_auth_provider_headers() {
        let invalid_name = AuthInfo {
            headers: vec![("bad header".to_string(), "value".to_string())],
            account_ref: None,
            metadata: Default::default(),
        };
        assert_eq!(
            build_auth_provider_headers(&invalid_name)
                .unwrap_err()
                .to_string(),
            "invalid proxy request: invalid AuthProvider header name: invalid HTTP header name"
        );

        let invalid_value = AuthInfo {
            headers: vec![("authorization".to_string(), "bad\r\nvalue".to_string())],
            account_ref: None,
            metadata: Default::default(),
        };
        assert!(build_auth_provider_headers(&invalid_value)
            .unwrap_err()
            .to_string()
            .starts_with("invalid proxy request: invalid AuthProvider header value:"));
    }

    #[test]
    fn builds_codex_provider_auth_headers_from_auth_info() {
        let auth = ProviderAuthInfo::new("sk-codex".to_string(), ProviderAuthStrategy::Bearer);
        let headers = build_codex_provider_auth_headers(&auth).unwrap();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1, HeaderValue::from_static("Bearer sk-codex"));
    }

    #[test]
    fn builds_gemini_auth_headers_for_api_key_and_oauth() {
        let api_key_headers = build_gemini_auth_headers("gemini-key", None, false).unwrap();
        assert_eq!(api_key_headers.len(), 1);
        assert_eq!(api_key_headers[0].0.as_str(), "x-goog-api-key");
        assert_eq!(api_key_headers[0].1, HeaderValue::from_static("gemini-key"));

        let oauth_headers =
            build_gemini_auth_headers("refresh-token", Some("ya29.access-token"), true).unwrap();
        assert_eq!(oauth_headers.len(), 2);
        assert_eq!(oauth_headers[0].0.as_str(), "authorization");
        assert_eq!(
            oauth_headers[0].1,
            HeaderValue::from_static("Bearer ya29.access-token")
        );
        assert_eq!(oauth_headers[1].0.as_str(), "x-goog-api-client");
        assert_eq!(
            oauth_headers[1].1,
            HeaderValue::from_static("GeminiCLI/1.0")
        );

        let fallback_headers = build_gemini_auth_headers("ya29.raw-token", None, true).unwrap();
        assert_eq!(
            fallback_headers[0].1,
            HeaderValue::from_static("Bearer ya29.raw-token")
        );
    }

    #[test]
    fn builds_gemini_provider_auth_headers_from_auth_strategy() {
        let oauth = ProviderAuthInfo::with_access_token(
            "refresh-token".to_string(),
            "ya29.access-token".to_string(),
        );
        let oauth_headers = build_gemini_provider_auth_headers(&oauth).unwrap();
        assert_eq!(oauth_headers[0].0.as_str(), "authorization");
        assert_eq!(
            oauth_headers[0].1,
            HeaderValue::from_static("Bearer ya29.access-token")
        );

        let api_key = ProviderAuthInfo::new("gemini-key".to_string(), ProviderAuthStrategy::Google);
        let api_key_headers = build_gemini_provider_auth_headers(&api_key).unwrap();
        assert_eq!(api_key_headers[0].0.as_str(), "x-goog-api-key");
        assert_eq!(api_key_headers[0].1, HeaderValue::from_static("gemini-key"));
    }

    #[test]
    fn rejects_invalid_gemini_auth_header_values() {
        let api_key_error =
            build_gemini_auth_headers("bad\r\nx-evil: 1", None, false).expect_err("invalid key");
        assert!(matches!(api_key_error, ProxyCoreError::Auth(_)));

        let oauth_error = build_gemini_auth_headers("refresh", Some("bad\r\nx-evil: 1"), true)
            .expect_err("invalid token");
        assert!(matches!(oauth_error, ProxyCoreError::Auth(_)));
    }

    #[test]
    fn builds_claude_static_auth_headers() {
        let anthropic =
            build_claude_auth_headers(ClaudeAuthHeaderKind::AnthropicApiKey, "sk-ant", None)
                .unwrap();
        assert_eq!(anthropic[0].0.as_str(), "x-api-key");
        assert_eq!(anthropic[0].1, HeaderValue::from_static("sk-ant"));

        let bearer =
            build_claude_auth_headers(ClaudeAuthHeaderKind::Bearer, "sk-relay", None).unwrap();
        assert_eq!(bearer[0].0.as_str(), "authorization");
        assert_eq!(bearer[0].1, HeaderValue::from_static("Bearer sk-relay"));

        let google =
            build_claude_auth_headers(ClaudeAuthHeaderKind::GoogleApiKey, "gemini-key", None)
                .unwrap();
        assert_eq!(google[0].0.as_str(), "x-goog-api-key");
        assert_eq!(google[0].1, HeaderValue::from_static("gemini-key"));

        let google_oauth = build_claude_auth_headers(
            ClaudeAuthHeaderKind::GoogleOAuth,
            "refresh-token",
            Some("ya29.access-token"),
        )
        .unwrap();
        assert_eq!(google_oauth[0].0.as_str(), "authorization");
        assert_eq!(
            google_oauth[0].1,
            HeaderValue::from_static("Bearer ya29.access-token")
        );
        assert_eq!(google_oauth[1].0.as_str(), "x-goog-api-client");

        let codex =
            build_claude_auth_headers(ClaudeAuthHeaderKind::CodexOAuth, "chatgpt-token", None)
                .unwrap();
        assert_eq!(codex[0].0.as_str(), "authorization");
        assert_eq!(codex[0].1, HeaderValue::from_static("Bearer chatgpt-token"));
        assert_eq!(codex[1].0.as_str(), "originator");
        assert_eq!(codex[1].1, HeaderValue::from_static("cc-switch"));
    }

    #[test]
    fn builds_claude_provider_auth_headers_for_static_and_copilot() {
        let static_auth =
            ProviderAuthInfo::new("claude-token".to_string(), ProviderAuthStrategy::ClaudeAuth);
        let static_headers = build_claude_provider_auth_headers(ClaudeProviderAuthHeadersInput {
            auth: &static_auth,
            copilot_request_id: "request-static",
            copilot_editor_version: "vscode/1",
            copilot_editor_plugin_version: "plugin/1",
            copilot_integration_id: "integration-1",
            copilot_user_agent: "copilot-test",
            copilot_github_api_version: "2022-11-28",
        })
        .unwrap();
        assert_eq!(static_headers[0].0.as_str(), "authorization");
        assert_eq!(
            static_headers[0].1,
            HeaderValue::from_static("Bearer claude-token")
        );

        let copilot_auth = ProviderAuthInfo::new(
            "copilot-token".to_string(),
            ProviderAuthStrategy::GitHubCopilot,
        );
        let copilot_headers = build_claude_provider_auth_headers(ClaudeProviderAuthHeadersInput {
            auth: &copilot_auth,
            copilot_request_id: "request-copilot",
            copilot_editor_version: "vscode/1",
            copilot_editor_plugin_version: "plugin/1",
            copilot_integration_id: "integration-1",
            copilot_user_agent: "copilot-test",
            copilot_github_api_version: "2022-11-28",
        })
        .unwrap();
        assert!(copilot_headers.iter().any(|(name, value)| {
            name.as_str() == "authorization" && value == "Bearer copilot-token"
        }));
        assert!(copilot_headers
            .iter()
            .any(|(name, value)| name.as_str() == "x-request-id" && value == "request-copilot"));
    }

    #[test]
    fn maps_provider_auth_strategy_to_claude_static_header_kind() {
        assert_eq!(
            claude_auth_header_kind_for_provider_strategy(ProviderAuthStrategy::Anthropic),
            Some(ClaudeAuthHeaderKind::AnthropicApiKey)
        );
        assert_eq!(
            claude_auth_header_kind_for_provider_strategy(ProviderAuthStrategy::ClaudeAuth),
            Some(ClaudeAuthHeaderKind::Bearer)
        );
        assert_eq!(
            claude_auth_header_kind_for_provider_strategy(ProviderAuthStrategy::Bearer),
            Some(ClaudeAuthHeaderKind::Bearer)
        );
        assert_eq!(
            claude_auth_header_kind_for_provider_strategy(ProviderAuthStrategy::Google),
            Some(ClaudeAuthHeaderKind::GoogleApiKey)
        );
        assert_eq!(
            claude_auth_header_kind_for_provider_strategy(ProviderAuthStrategy::GoogleOAuth),
            Some(ClaudeAuthHeaderKind::GoogleOAuth)
        );
        assert_eq!(
            claude_auth_header_kind_for_provider_strategy(ProviderAuthStrategy::CodexOAuth),
            Some(ClaudeAuthHeaderKind::CodexOAuth)
        );
        assert_eq!(
            claude_auth_header_kind_for_provider_strategy(ProviderAuthStrategy::GitHubCopilot),
            None
        );
    }

    #[test]
    fn rejects_invalid_claude_static_auth_header_values() {
        let anthropic_error = build_claude_auth_headers(
            ClaudeAuthHeaderKind::AnthropicApiKey,
            "bad\r\nx-evil: 1",
            None,
        )
        .expect_err("invalid anthropic key");
        assert!(matches!(anthropic_error, ProxyCoreError::Auth(_)));

        let oauth_error = build_claude_auth_headers(
            ClaudeAuthHeaderKind::GoogleOAuth,
            "refresh",
            Some("bad\r\nx-evil: 1"),
        )
        .expect_err("invalid oauth token");
        assert!(matches!(oauth_error, ProxyCoreError::Auth(_)));
    }

    #[test]
    fn builds_copilot_auth_headers_with_request_ids() {
        let headers = build_copilot_auth_headers(CopilotAuthHeadersInput {
            api_key: "copilot-token",
            request_id: "request-123",
            editor_version: "vscode/1.110.1",
            editor_plugin_version: "copilot-chat/0.38.2",
            integration_id: "vscode-chat",
            user_agent: "GitHubCopilotChat/0.38.2",
            github_api_version: "2025-10-01",
        })
        .unwrap();

        let pairs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.to_str().unwrap()))
            .collect();

        assert_eq!(
            pairs,
            vec![
                ("authorization", "Bearer copilot-token"),
                ("editor-version", "vscode/1.110.1"),
                ("editor-plugin-version", "copilot-chat/0.38.2"),
                ("copilot-integration-id", "vscode-chat"),
                ("user-agent", "GitHubCopilotChat/0.38.2"),
                ("x-github-api-version", "2025-10-01"),
                ("openai-intent", "conversation-agent"),
                ("x-initiator", "user"),
                ("x-interaction-type", "conversation-agent"),
                ("x-vscode-user-agent-library-version", "electron-fetch"),
                ("x-request-id", "request-123"),
                ("x-agent-task-id", "request-123"),
            ]
        );
    }

    #[test]
    fn rejects_invalid_copilot_auth_header_values() {
        let error = build_copilot_auth_headers(CopilotAuthHeadersInput {
            api_key: "copilot-token",
            request_id: "bad\r\nx-evil: 1",
            editor_version: "vscode/1.110.1",
            editor_plugin_version: "copilot-chat/0.38.2",
            integration_id: "vscode-chat",
            user_agent: "GitHubCopilotChat/0.38.2",
            github_api_version: "2025-10-01",
        })
        .expect_err("invalid request id");

        assert!(matches!(error, ProxyCoreError::Auth(_)));
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
    fn codex_oauth_session_headers_for_forwarder_require_runtime_and_client_session() {
        assert!(
            build_codex_oauth_session_headers_for_forwarder(false, true, "session-123").is_empty()
        );
        assert!(
            build_codex_oauth_session_headers_for_forwarder(true, false, "session-123").is_empty()
        );

        let headers = build_codex_oauth_session_headers_for_forwarder(true, true, "session-123");
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
            channel_header_overrides: None,
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
        assert_eq!(
            headers.get("x-keep"),
            Some(&HeaderValue::from_static("keep"))
        );
        assert!(headers.get(header::CONTENT_LENGTH).is_none());
        assert!(headers.get("x-request-id").is_none());
        assert_eq!(
            headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
    }

    #[test]
    fn channel_header_overrides_apply_without_overriding_auth_or_host() {
        let mut inbound = HeaderMap::new();
        inbound.insert("host", HeaderValue::from_static("localhost:3456"));
        inbound.insert("authorization", HeaderValue::from_static("Bearer inbound"));
        inbound.insert("x-keep", HeaderValue::from_static("keep"));
        let auth_headers = vec![(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer upstream"),
        )];
        let overrides = json!({
            "host": "evil.example.com",
            "authorization": "Bearer evil",
            "x-api-key": "evil-key",
            "x-request-id": "trace-override",
            "x-relay-profile": "manual",
            "content-type": "application/x-ndjson"
        });

        let headers = build_upstream_request_headers(UpstreamRequestHeadersInput {
            inbound_headers: &inbound,
            upstream_host: Some("api.example.com"),
            auth_headers: &auth_headers,
            channel_header_overrides: Some(&overrides),
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
        assert!(headers.get("x-api-key").is_none());
        assert!(headers.get("x-request-id").is_none());
        assert_eq!(
            headers.get("x-relay-profile"),
            Some(&HeaderValue::from_static("manual"))
        );
        assert_eq!(
            headers.get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/x-ndjson"))
        );
    }

    #[test]
    fn extracts_upstream_host_header_from_absolute_url() {
        assert_eq!(
            upstream_host_header_from_url("https://api.example.com/v1/messages").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(
            upstream_host_header_from_url("https://api.example.com:8443/v1/messages").as_deref(),
            Some("api.example.com:8443")
        );
        assert_eq!(upstream_host_header_from_url("/v1/messages"), None);
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
            channel_header_overrides: None,
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
            channel_header_overrides: None,
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
        assert_eq!(
            headers.get("x-safe"),
            Some(&HeaderValue::from_static("safe"))
        );
        assert_eq!(
            headers.get("session_id"),
            Some(&HeaderValue::from_static("session-123"))
        );
        assert_eq!(
            headers.get("x-codex-window-id"),
            Some(&HeaderValue::from_static("session-123:0"))
        );
    }

    #[test]
    fn auth_header_value_rejects_invalid_credential_bytes() {
        let value = auth_header_value("Bearer sk-valid").expect("valid auth header");
        assert_eq!(value.to_str().unwrap(), "Bearer sk-valid");

        let error = auth_header_value("Bearer bad\r\nx-evil: 1").expect_err("invalid header");
        assert!(matches!(error, ProxyCoreError::Auth(_)));
        assert!(error.to_string().contains("invalid auth header value"));
    }
}
