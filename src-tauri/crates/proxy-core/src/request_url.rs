use crate::{
    domain::{AppKind, ProviderKind, CODEX_OAUTH_CLAUDE_BASE_URL},
    gemini_url::{normalize_gemini_model_id, resolve_gemini_native_url},
};
use serde_json::Value;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointRewrite {
    pub endpoint: String,
    pub passthrough_query: Option<String>,
}

impl EndpointRewrite {
    pub fn into_parts(self) -> (String, Option<String>) {
        (self.endpoint, self.passthrough_query)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaudeTransformEndpointRewriteInput<'a> {
    pub endpoint: &'a str,
    pub api_format: &'a str,
    pub is_copilot: bool,
    pub gemini_model: Option<&'a str>,
    pub gemini_stream: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ForwardUpstreamUrlPlanInput<'a> {
    pub base_url: &'a str,
    pub endpoint: &'a str,
    pub is_full_url: bool,
    pub codex_responses_to_chat: bool,
    pub use_claude_transform: bool,
    pub is_copilot: bool,
    pub claude_api_format: Option<&'a str>,
    pub body: &'a Value,
    pub channel_param_overrides: Option<&'a Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardUpstreamUrlPlan {
    pub effective_endpoint: String,
    pub passthrough_query: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwarderProviderUrlFacts {
    pub base_url: String,
    pub is_full_url: bool,
    pub is_copilot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwarderProviderUrlFactsInput<'a> {
    pub base_url: String,
    pub is_full_url: bool,
    pub provider_type: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CodexProviderChatCompletionsFacts<'a> {
    pub api_format: Option<&'a str>,
    pub wire_api: Option<&'a str>,
    pub base_url: Option<&'a str>,
    pub config_base_url: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodexResponsesToChatConversionFacts<'a> {
    pub provider: CodexProviderChatCompletionsFacts<'a>,
    pub endpoint: &'a str,
}

pub fn claude_transform_endpoint_rewrite_input_from_body<'a>(
    endpoint: &'a str,
    api_format: &'a str,
    is_copilot: bool,
    body: &'a Value,
) -> ClaudeTransformEndpointRewriteInput<'a> {
    let (gemini_model, gemini_stream) = if api_format == "gemini_native" {
        let model = body
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let model = normalize_gemini_model_id(model);
        let is_stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
        (Some(model), is_stream)
    } else {
        (None, false)
    };

    ClaudeTransformEndpointRewriteInput {
        endpoint,
        api_format,
        is_copilot,
        gemini_model,
        gemini_stream,
    }
}

pub fn split_endpoint_and_query(endpoint: &str) -> (&str, Option<&str>) {
    endpoint
        .split_once('?')
        .map_or((endpoint, None), |(path, query)| (path, Some(query)))
}

pub fn invalid_upstream_url_error_message(url: &str, error: impl std::fmt::Display) -> String {
    format!("Invalid URL '{url}': {error}")
}

pub fn strip_beta_query(query: Option<&str>) -> Option<String> {
    let filtered = query.map(|query| {
        query
            .split('&')
            .filter(|pair| !pair.is_empty() && !pair.starts_with("beta="))
            .collect::<Vec<_>>()
            .join("&")
    });

    match filtered.as_deref() {
        Some("") | None => None,
        Some(_) => filtered,
    }
}

pub fn merge_query_params(base_query: Option<&str>, extra_param: Option<&str>) -> Option<String> {
    let mut params: Vec<String> = base_query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter(|pair| !pair.is_empty())
        .filter(|pair| !pair.starts_with("alt="))
        .map(ToString::to_string)
        .collect();

    if let Some(extra_param) = extra_param {
        params.push(extra_param.to_string());
    }

    if params.is_empty() {
        None
    } else {
        Some(params.join("&"))
    }
}

pub fn append_query_to_full_url(base_url: &str, query: Option<&str>) -> String {
    match query {
        Some(query) if !query.is_empty() => {
            if base_url.contains('?') {
                format!("{base_url}&{query}")
            } else {
                format!("{base_url}?{query}")
            }
        }
        _ => base_url.to_string(),
    }
}

pub fn apply_channel_param_overrides_to_url(url: &str, param_overrides: &Value) -> String {
    let override_pairs = channel_param_override_pairs(param_overrides);
    if override_pairs.is_empty() {
        return url.to_string();
    }

    let (base, existing_query) = split_endpoint_and_query(url);
    let override_keys = override_pairs
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    let mut params = existing_query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let existing_key = pair.split_once('=').map_or(*pair, |(key, _)| key);
            !override_keys.contains(&existing_key)
        })
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    params.extend(
        override_pairs
            .into_iter()
            .map(|(key, value)| format!("{key}={value}")),
    );

    if params.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", params.join("&"))
    }
}

fn channel_param_override_pairs(param_overrides: &Value) -> Vec<(String, String)> {
    param_overrides
        .as_object()
        .into_iter()
        .flat_map(|params| params.iter())
        .filter_map(|(key, value)| {
            let key = key.trim();
            if key.is_empty() {
                return None;
            }

            scalar_query_value(value).map(|value| {
                (
                    percent_encode_query_component(key),
                    percent_encode_query_component(&value),
                )
            })
        })
        .collect()
}

fn scalar_query_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn percent_encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char);
            }
            byte => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

pub fn append_query_to_endpoint_path(endpoint: &str, query: Option<&str>) -> String {
    match query {
        Some(query) if endpoint.contains('?') => format!("{endpoint}&{query}"),
        Some(query) => format!("{endpoint}?{query}"),
        None => endpoint.to_string(),
    }
}

pub fn strip_endpoint_prefix<'a>(endpoint: &'a str, prefix: Option<&str>) -> &'a str {
    prefix
        .and_then(|prefix| endpoint.strip_prefix(prefix))
        .unwrap_or(endpoint)
}

pub fn build_claude_upstream_url(base_url: &str, endpoint: &str) -> String {
    if base_url == CODEX_OAUTH_CLAUDE_BASE_URL {
        return format!("{CODEX_OAUTH_CLAUDE_BASE_URL}/responses");
    }

    let mut url = format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    );

    while url.contains("/v1/v1") {
        url = url.replace("/v1/v1", "/v1");
    }

    url
}

pub fn build_codex_upstream_url(base_url: &str, endpoint: &str) -> String {
    let base_trimmed = base_url.trim_end_matches('/');
    let endpoint_trimmed = endpoint.trim_start_matches('/');

    let mut url = if base_trimmed.ends_with("/v1") {
        format!("{base_trimmed}/{endpoint_trimmed}")
    } else if is_origin_only_url(base_trimmed) {
        format!("{base_trimmed}/v1/{endpoint_trimmed}")
    } else {
        format!("{base_trimmed}/{endpoint_trimmed}")
    };

    while url.contains("/v1/v1") {
        url = url.replace("/v1/v1", "/v1");
    }

    url
}

pub fn is_github_copilot_upstream(provider_type: Option<&str>, base_url: &str) -> bool {
    provider_type.map(ProviderKind::from) == Some(ProviderKind::GitHubCopilot)
        || base_url.contains("githubcopilot.com")
}

pub fn forwarder_provider_url_facts(
    input: ForwarderProviderUrlFactsInput<'_>,
) -> ForwarderProviderUrlFacts {
    let is_copilot = is_github_copilot_upstream(input.provider_type, &input.base_url);
    ForwarderProviderUrlFacts {
        base_url: input.base_url,
        is_full_url: input.is_full_url,
        is_copilot,
    }
}

pub fn should_resolve_copilot_dynamic_endpoint(is_copilot: bool, is_full_url: bool) -> bool {
    is_copilot && !is_full_url
}

pub fn resolved_copilot_dynamic_base_url(
    current_base_url: &str,
    dynamic_endpoint: &str,
    is_copilot: bool,
    is_full_url: bool,
) -> Option<String> {
    if should_resolve_copilot_dynamic_endpoint(is_copilot, is_full_url)
        && dynamic_endpoint != current_base_url
    {
        Some(dynamic_endpoint.to_string())
    } else {
        None
    }
}

pub fn is_claude_messages_path(path: &str) -> bool {
    matches!(path, "/v1/messages" | "/claude/v1/messages")
}

pub fn rewrite_codex_responses_endpoint_to_chat(endpoint: &str) -> EndpointRewrite {
    let (_path, query) = split_endpoint_and_query(endpoint);
    let passthrough_query = query.map(ToString::to_string);
    let target_path = "/chat/completions";
    let endpoint = match passthrough_query.as_deref() {
        Some(query) if !query.is_empty() => format!("{target_path}?{query}"),
        _ => target_path.to_string(),
    };

    EndpointRewrite {
        endpoint,
        passthrough_query,
    }
}

pub fn rewrite_claude_transform_endpoint(
    input: ClaudeTransformEndpointRewriteInput<'_>,
) -> EndpointRewrite {
    let (path, query) = split_endpoint_and_query(input.endpoint);
    let passthrough_query = if is_claude_messages_path(path) {
        strip_beta_query(query)
    } else {
        query.map(ToString::to_string)
    };

    if !is_claude_messages_path(path) {
        return EndpointRewrite {
            endpoint: input.endpoint.to_string(),
            passthrough_query,
        };
    }

    if input.api_format == "gemini_native" {
        let model = input.gemini_model.unwrap_or("unknown");
        let target_path = if input.gemini_stream {
            format!("/v1beta/models/{model}:streamGenerateContent")
        } else {
            format!("/v1beta/models/{model}:generateContent")
        };
        let rewritten_query = merge_query_params(
            passthrough_query.as_deref(),
            if input.gemini_stream {
                Some("alt=sse")
            } else {
                None
            },
        );
        let endpoint = match rewritten_query.as_deref() {
            Some(query) if !query.is_empty() => format!("{target_path}?{query}"),
            _ => target_path,
        };

        return EndpointRewrite {
            endpoint,
            passthrough_query: rewritten_query,
        };
    }

    let target_path = if input.is_copilot && input.api_format == "openai_responses" {
        "/v1/responses"
    } else if input.is_copilot {
        "/chat/completions"
    } else if input.api_format == "openai_responses" {
        "/v1/responses"
    } else {
        "/v1/chat/completions"
    };
    let endpoint = match passthrough_query.as_deref() {
        Some(query) if !query.is_empty() => format!("{target_path}?{query}"),
        _ => target_path.to_string(),
    };

    EndpointRewrite {
        endpoint,
        passthrough_query,
    }
}

pub fn forward_upstream_url_plan(
    input: ForwardUpstreamUrlPlanInput<'_>,
    build_adapter_url: impl FnOnce(&str, &str) -> String,
) -> ForwardUpstreamUrlPlan {
    let (effective_endpoint, passthrough_query) = if input.codex_responses_to_chat {
        rewrite_codex_responses_endpoint_to_chat(input.endpoint).into_parts()
    } else if input.use_claude_transform {
        let api_format = input.claude_api_format.unwrap_or("anthropic");
        rewrite_claude_transform_endpoint(claude_transform_endpoint_rewrite_input_from_body(
            input.endpoint,
            api_format,
            input.is_copilot,
            input.body,
        ))
        .into_parts()
    } else {
        (
            input.endpoint.to_string(),
            split_endpoint_and_query(input.endpoint)
                .1
                .map(ToString::to_string),
        )
    };

    let codex_chat_base_is_full_endpoint =
        is_codex_chat_full_endpoint_base(input.codex_responses_to_chat, input.base_url);
    let mut url = if matches!(input.claude_api_format, Some("gemini_native")) {
        resolve_gemini_native_url(input.base_url, &effective_endpoint, input.is_full_url)
    } else if input.is_full_url || codex_chat_base_is_full_endpoint {
        append_query_to_full_url(input.base_url, passthrough_query.as_deref())
    } else {
        build_adapter_url(input.base_url, &effective_endpoint)
    };

    if let Some(param_overrides) = input.channel_param_overrides {
        url = apply_channel_param_overrides_to_url(&url, param_overrides);
    }

    ForwardUpstreamUrlPlan {
        effective_endpoint,
        passthrough_query,
        url,
    }
}

pub fn extract_gemini_model_from_path(endpoint: &str) -> Option<String> {
    let segments: Vec<&str> = endpoint.split('/').collect();
    segments
        .iter()
        .position(|segment| *segment == "models")
        .and_then(|index| segments.get(index + 1).copied())
        .map(|segment| segment.split('?').next().unwrap_or(segment))
        .map(|segment| segment.split(':').next().unwrap_or(segment))
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
}

pub fn is_codex_chat_wire_api(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "chat"
            | "chat_completions"
            | "chat-completions"
            | "openai_chat"
            | "openai-chat"
            | "openai_chat_completions"
    )
}

pub fn is_codex_chat_completions_url(value: &str) -> bool {
    value
        .trim_end_matches('/')
        .to_ascii_lowercase()
        .ends_with("/chat/completions")
}

pub fn is_codex_chat_full_endpoint_base(codex_responses_to_chat: bool, base_url: &str) -> bool {
    codex_responses_to_chat && is_codex_chat_completions_url(base_url)
}

/// `scheme://host` 之后没有路径段的纯 origin 形式。
pub fn is_origin_only_url(value: &str) -> bool {
    let trimmed = value.trim_end_matches('/');
    match trimmed.split_once("://") {
        Some((_scheme, rest)) => !rest.contains('/'),
        None => !trimmed.contains('/'),
    }
}

pub fn is_codex_responses_endpoint(endpoint: &str) -> bool {
    let (path, _query) = split_endpoint_and_query(endpoint);
    matches!(
        path,
        "/responses" | "/v1/responses" | "/responses/compact" | "/v1/responses/compact"
    )
}

pub fn should_convert_codex_responses_endpoint_to_chat(
    provider_uses_chat_completions: bool,
    endpoint: &str,
) -> bool {
    provider_uses_chat_completions && is_codex_responses_endpoint(endpoint)
}

pub fn codex_provider_uses_chat_completions(facts: CodexProviderChatCompletionsFacts<'_>) -> bool {
    if let Some(api_format) = facts.api_format {
        return is_codex_chat_wire_api(api_format);
    }

    if let Some(wire_api) = facts.wire_api {
        return is_codex_chat_wire_api(wire_api);
    }

    if let Some(base_url) = facts.base_url {
        return is_codex_chat_completions_url(base_url);
    }

    facts
        .config_base_url
        .map(is_codex_chat_completions_url)
        .unwrap_or(false)
}

pub fn codex_responses_to_chat_conversion_required(
    facts: CodexResponsesToChatConversionFacts<'_>,
) -> bool {
    should_convert_codex_responses_endpoint_to_chat(
        codex_provider_uses_chat_completions(facts.provider),
        facts.endpoint,
    )
}

pub fn resolve_codex_provider_uses_chat_completions(
    api_format: Option<&str>,
    wire_api: Option<&str>,
    base_url: Option<&str>,
    config_base_url: Option<&str>,
) -> bool {
    codex_provider_uses_chat_completions(CodexProviderChatCompletionsFacts {
        api_format,
        wire_api,
        base_url,
        config_base_url,
    })
}

pub fn request_model_for_forward(app: &AppKind, endpoint: &str, body: &Value) -> Option<String> {
    if matches!(app, AppKind::Gemini) {
        return extract_gemini_model_from_path(endpoint);
    }

    body.get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

pub fn interface_kind_for_forward(app: &AppKind, endpoint: &str) -> Option<&'static str> {
    match app {
        AppKind::Claude | AppKind::ClaudeDesktop => Some("anthropic_messages"),
        AppKind::Codex => openai_interface_kind_for_endpoint(endpoint),
        AppKind::Custom(value) if is_openai_compatible_custom_app(value) => {
            openai_interface_kind_for_endpoint(endpoint)
        }
        AppKind::Gemini => Some("gemini_native"),
        AppKind::Custom(_) => None,
    }
}

fn openai_interface_kind_for_endpoint(endpoint: &str) -> Option<&'static str> {
    let path = endpoint.split_once('?').map_or(endpoint, |(path, _)| path);
    if path.ends_with("/chat/completions") {
        Some("openai_chat_completions")
    } else if path.ends_with("/responses") || path.ends_with("/responses/compact") {
        Some("openai_responses")
    } else {
        None
    }
}

fn is_openai_compatible_custom_app(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "opencode" | "openclaw" | "hermes"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        append_query_to_endpoint_path, append_query_to_full_url,
        apply_channel_param_overrides_to_url, build_claude_upstream_url, build_codex_upstream_url,
        claude_transform_endpoint_rewrite_input_from_body, codex_provider_uses_chat_completions,
        codex_responses_to_chat_conversion_required, extract_gemini_model_from_path,
        forward_upstream_url_plan, forwarder_provider_url_facts, interface_kind_for_forward,
        invalid_upstream_url_error_message, is_codex_chat_completions_url,
        is_codex_chat_full_endpoint_base, is_codex_chat_wire_api, is_codex_responses_endpoint,
        is_github_copilot_upstream, is_origin_only_url, merge_query_params,
        request_model_for_forward, resolve_codex_provider_uses_chat_completions,
        resolved_copilot_dynamic_base_url, rewrite_claude_transform_endpoint,
        rewrite_codex_responses_endpoint_to_chat, should_convert_codex_responses_endpoint_to_chat,
        should_resolve_copilot_dynamic_endpoint, split_endpoint_and_query, strip_beta_query,
        strip_endpoint_prefix, AppKind, ClaudeTransformEndpointRewriteInput,
        CodexProviderChatCompletionsFacts, CodexResponsesToChatConversionFacts,
        ForwardUpstreamUrlPlanInput, ForwarderProviderUrlFactsInput,
    };
    use crate::domain::CODEX_OAUTH_CLAUDE_BASE_URL;
    use serde_json::json;

    #[test]
    fn formats_invalid_upstream_url_errors() {
        assert_eq!(
            invalid_upstream_url_error_message("http://[bad", "invalid uri"),
            "Invalid URL 'http://[bad': invalid uri"
        );
    }

    #[test]
    fn split_endpoint_and_query_separates_first_query_marker() {
        assert_eq!(
            split_endpoint_and_query("/v1/messages?beta=true&x-id=1"),
            ("/v1/messages", Some("beta=true&x-id=1"))
        );
        assert_eq!(
            split_endpoint_and_query("/v1/messages"),
            ("/v1/messages", None)
        );
    }

    #[test]
    fn strip_beta_query_removes_beta_pairs_and_empty_segments() {
        assert_eq!(
            strip_beta_query(Some("beta=true&x-id=1&&foo=bar")),
            Some("x-id=1&foo=bar".to_string())
        );
        assert_eq!(strip_beta_query(Some("beta=true&&")), None);
        assert_eq!(strip_beta_query(None), None);
    }

    #[test]
    fn merge_query_params_replaces_alt_param_with_extra_param() {
        assert_eq!(
            merge_query_params(Some("x-id=1&alt=json&foo=bar"), Some("alt=sse")),
            Some("x-id=1&foo=bar&alt=sse".to_string())
        );
        assert_eq!(merge_query_params(Some("alt=json"), None), None);
        assert_eq!(
            merge_query_params(None, Some("alt=sse")),
            Some("alt=sse".to_string())
        );
    }

    #[test]
    fn append_query_to_full_url_preserves_existing_query_string() {
        assert_eq!(
            append_query_to_full_url("https://relay.example/api?foo=bar", Some("x-id=1")),
            "https://relay.example/api?foo=bar&x-id=1"
        );
        assert_eq!(
            append_query_to_full_url("https://relay.example/api", Some("x-id=1")),
            "https://relay.example/api?x-id=1"
        );
        assert_eq!(
            append_query_to_full_url("https://relay.example/api", None),
            "https://relay.example/api"
        );
    }

    #[test]
    fn channel_param_overrides_merge_into_upstream_url() {
        let url = apply_channel_param_overrides_to_url(
            "https://relay.example/api?api-version=old&keep=1",
            &json!({
                "api-version": "2026-06-20",
                "mode": "fast lane",
                "enabled": true,
                "skip": null,
                "nested": { "ignored": true }
            }),
        );

        assert_eq!(
            url,
            "https://relay.example/api?keep=1&api-version=2026-06-20&mode=fast%20lane&enabled=true"
        );
        assert_eq!(
            apply_channel_param_overrides_to_url("https://relay.example/api", &json!({})),
            "https://relay.example/api"
        );
    }

    #[test]
    fn append_query_to_endpoint_path_preserves_client_query() {
        assert_eq!(
            append_query_to_endpoint_path("/responses", Some("stream=false&x-id=1")),
            "/responses?stream=false&x-id=1"
        );
        assert_eq!(
            append_query_to_endpoint_path("/responses?existing=true", Some("x-id=1")),
            "/responses?existing=true&x-id=1"
        );
        assert_eq!(
            append_query_to_endpoint_path("/responses/compact", None),
            "/responses/compact"
        );
        assert_eq!(
            append_query_to_endpoint_path("/responses", Some("")),
            "/responses?"
        );
    }

    #[test]
    fn strip_endpoint_prefix_preserves_suffix_and_query() {
        assert_eq!(
            strip_endpoint_prefix(
                "/claude-desktop/v1/messages?x-id=1",
                Some("/claude-desktop")
            ),
            "/v1/messages?x-id=1"
        );
        assert_eq!(
            strip_endpoint_prefix("/v1/messages?x-id=1", Some("/claude-desktop")),
            "/v1/messages?x-id=1"
        );
        assert_eq!(strip_endpoint_prefix("/v1/messages", None), "/v1/messages");
    }

    #[test]
    fn builds_claude_upstream_url_with_legacy_join_semantics() {
        assert_eq!(
            build_claude_upstream_url("https://api.anthropic.com", "/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            build_claude_upstream_url("https://api.anthropic.com/", "v1/messages?foo=bar"),
            "https://api.anthropic.com/v1/messages?foo=bar"
        );
        assert_eq!(
            build_claude_upstream_url("https://relay.example.com/v1", "/v1/messages"),
            "https://relay.example.com/v1/messages"
        );
        assert_eq!(
            build_claude_upstream_url(CODEX_OAUTH_CLAUDE_BASE_URL, "/v1/messages"),
            "https://chatgpt.com/backend-api/codex/responses"
        );
    }

    #[test]
    fn builds_codex_upstream_url_with_origin_v1_policy() {
        assert_eq!(
            build_codex_upstream_url("https://api.openai.com/v1", "/responses"),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            build_codex_upstream_url("https://api.openai.com", "/responses"),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            build_codex_upstream_url("https://example.com/openai", "/responses"),
            "https://example.com/openai/responses"
        );
        assert_eq!(
            build_codex_upstream_url("https://www.packyapi.com/v1", "/v1/responses"),
            "https://www.packyapi.com/v1/responses"
        );
    }

    #[test]
    fn detects_github_copilot_upstream_from_provider_type_or_base_url() {
        assert!(is_github_copilot_upstream(
            Some("github_copilot"),
            "https://copilot-api.corp.example.com"
        ));
        assert!(is_github_copilot_upstream(
            Some("github-copilot"),
            "https://copilot-api.corp.example.com"
        ));
        assert!(is_github_copilot_upstream(
            None,
            "https://api.githubcopilot.com"
        ));
        assert!(!is_github_copilot_upstream(
            Some("anthropic"),
            "https://api.anthropic.com"
        ));
    }

    #[test]
    fn projects_forwarder_provider_url_facts() {
        let facts = forwarder_provider_url_facts(ForwarderProviderUrlFactsInput {
            base_url: "https://relay.example/v1/chat/completions".to_string(),
            is_full_url: true,
            provider_type: Some("github_copilot"),
        });

        assert_eq!(facts.base_url, "https://relay.example/v1/chat/completions");
        assert!(facts.is_full_url);
        assert!(facts.is_copilot);

        let base_url_copilot = forwarder_provider_url_facts(ForwarderProviderUrlFactsInput {
            base_url: "https://api.githubcopilot.com".to_string(),
            is_full_url: false,
            provider_type: None,
        });
        assert!(base_url_copilot.is_copilot);
    }

    #[test]
    fn detects_codex_chat_wire_api_aliases() {
        assert!(is_codex_chat_wire_api("chat"));
        assert!(is_codex_chat_wire_api("openai_chat"));
        assert!(is_codex_chat_wire_api("openai_chat_completions"));
        assert!(!is_codex_chat_wire_api("responses"));
    }

    #[test]
    fn detects_codex_chat_completions_urls() {
        assert!(is_codex_chat_completions_url(
            "https://relay.example.com/v1/chat/completions/"
        ));
        assert!(!is_codex_chat_completions_url(
            "https://relay.example.com/v1/responses"
        ));
        assert!(is_codex_chat_full_endpoint_base(
            true,
            "https://relay.example.com/v1/chat/completions/"
        ));
        assert!(!is_codex_chat_full_endpoint_base(
            false,
            "https://relay.example.com/v1/chat/completions/"
        ));
        assert!(!is_codex_chat_full_endpoint_base(
            true,
            "https://relay.example.com/v1/responses"
        ));
    }

    #[test]
    fn detects_origin_only_urls() {
        assert!(is_origin_only_url("https://api.openai.com/"));
        assert!(is_origin_only_url("localhost:8080"));
        assert!(!is_origin_only_url("https://api.openai.com/v1"));
        assert!(!is_origin_only_url("localhost:8080/v1"));
    }

    #[test]
    fn detects_codex_responses_endpoint_conversion_targets() {
        assert!(is_codex_responses_endpoint("/responses?stream=true"));
        assert!(is_codex_responses_endpoint("/v1/responses/compact"));
        assert!(!is_codex_responses_endpoint("/chat/completions"));
        assert!(should_convert_codex_responses_endpoint_to_chat(
            true,
            "/v1/responses"
        ));
        assert!(!should_convert_codex_responses_endpoint_to_chat(
            false,
            "/v1/responses"
        ));
    }

    #[test]
    fn resolves_codex_chat_completions_provider_priority() {
        assert!(resolve_codex_provider_uses_chat_completions(
            Some("openai_chat"),
            Some("responses"),
            Some("https://relay.example.com/v1/responses"),
            None,
        ));
        assert!(!resolve_codex_provider_uses_chat_completions(
            Some("responses"),
            Some("chat"),
            Some("https://relay.example.com/v1/chat/completions"),
            None,
        ));
        assert!(resolve_codex_provider_uses_chat_completions(
            None,
            Some("chat"),
            Some("https://relay.example.com/v1/responses"),
            None,
        ));
        assert!(resolve_codex_provider_uses_chat_completions(
            None,
            None,
            Some("https://relay.example.com/v1/chat/completions"),
            None,
        ));
        assert!(resolve_codex_provider_uses_chat_completions(
            None,
            None,
            None,
            Some("https://relay.example.com/v1/chat/completions"),
        ));
        assert!(!resolve_codex_provider_uses_chat_completions(
            None,
            None,
            None,
            Some("https://relay.example.com/v1"),
        ));
    }

    #[test]
    fn resolves_codex_responses_to_chat_conversion_from_provider_facts() {
        let chat_provider = CodexProviderChatCompletionsFacts {
            api_format: None,
            wire_api: Some("chat"),
            base_url: Some("https://relay.example.com/v1/responses"),
            config_base_url: None,
        };

        assert!(codex_provider_uses_chat_completions(chat_provider));
        assert!(codex_responses_to_chat_conversion_required(
            CodexResponsesToChatConversionFacts {
                provider: chat_provider,
                endpoint: "/v1/responses/compact?stream=true",
            }
        ));
        assert!(!codex_responses_to_chat_conversion_required(
            CodexResponsesToChatConversionFacts {
                provider: chat_provider,
                endpoint: "/chat/completions",
            }
        ));

        let explicit_responses_provider = CodexProviderChatCompletionsFacts {
            api_format: Some("openai_responses"),
            wire_api: Some("chat"),
            base_url: Some("https://relay.example.com/v1/chat/completions"),
            config_base_url: None,
        };
        assert!(!codex_responses_to_chat_conversion_required(
            CodexResponsesToChatConversionFacts {
                provider: explicit_responses_provider,
                endpoint: "/v1/responses",
            }
        ));
    }

    #[test]
    fn resolves_copilot_dynamic_base_url_only_for_non_full_url_copilot() {
        assert!(should_resolve_copilot_dynamic_endpoint(true, false));
        assert!(!should_resolve_copilot_dynamic_endpoint(true, true));
        assert!(!should_resolve_copilot_dynamic_endpoint(false, false));

        assert_eq!(
            resolved_copilot_dynamic_base_url(
                "https://api.githubcopilot.com",
                "https://copilot-api.enterprise.example.com",
                true,
                false,
            )
            .as_deref(),
            Some("https://copilot-api.enterprise.example.com")
        );
        assert_eq!(
            resolved_copilot_dynamic_base_url(
                "https://api.githubcopilot.com",
                "https://api.githubcopilot.com",
                true,
                false,
            ),
            None
        );
        assert_eq!(
            resolved_copilot_dynamic_base_url(
                "https://api.githubcopilot.com",
                "https://copilot-api.enterprise.example.com",
                true,
                true,
            ),
            None
        );
    }

    #[test]
    fn rewrites_codex_responses_endpoint_to_chat_and_preserves_query() {
        let rewrite = rewrite_codex_responses_endpoint_to_chat("/v1/responses?foo=bar");

        assert_eq!(rewrite.endpoint, "/chat/completions?foo=bar");
        assert_eq!(rewrite.passthrough_query.as_deref(), Some("foo=bar"));
    }

    #[test]
    fn plans_codex_chat_full_endpoint_url_with_channel_param_overrides() {
        let plan = forward_upstream_url_plan(
            ForwardUpstreamUrlPlanInput {
                base_url: "https://api.openai.com/v1/chat/completions",
                endpoint: "/v1/responses?foo=bar&api-version=old",
                is_full_url: false,
                codex_responses_to_chat: true,
                use_claude_transform: false,
                is_copilot: false,
                claude_api_format: None,
                body: &json!({}),
                channel_param_overrides: Some(&json!({"api-version": "2026-06-21"})),
            },
            |base_url, effective_endpoint| format!("{base_url}{effective_endpoint}"),
        );

        assert_eq!(
            plan.effective_endpoint,
            "/chat/completions?foo=bar&api-version=old"
        );
        assert_eq!(
            plan.passthrough_query.as_deref(),
            Some("foo=bar&api-version=old")
        );
        assert_eq!(
            plan.url,
            "https://api.openai.com/v1/chat/completions?foo=bar&api-version=2026-06-21"
        );
    }

    #[test]
    fn plans_gemini_native_full_url_with_stream_query() {
        let plan = forward_upstream_url_plan(
            ForwardUpstreamUrlPlanInput {
                base_url: "https://relay.example/custom/generate-content",
                endpoint: "/v1/messages?beta=true",
                is_full_url: true,
                codex_responses_to_chat: false,
                use_claude_transform: true,
                is_copilot: false,
                claude_api_format: Some("gemini_native"),
                body: &json!({"model": "gemini-2.5-flash", "stream": true}),
                channel_param_overrides: None,
            },
            |base_url, effective_endpoint| format!("{base_url}{effective_endpoint}"),
        );

        assert_eq!(
            plan.effective_endpoint,
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(plan.passthrough_query.as_deref(), Some("alt=sse"));
        assert_eq!(
            plan.url,
            "https://relay.example/custom/generate-content?alt=sse"
        );
    }

    #[test]
    fn rewrites_claude_messages_to_openai_chat_and_strips_beta_query() {
        let rewrite = rewrite_claude_transform_endpoint(ClaudeTransformEndpointRewriteInput {
            endpoint: "/v1/messages?beta=true&foo=bar",
            api_format: "openai_chat",
            is_copilot: false,
            gemini_model: None,
            gemini_stream: false,
        });

        assert_eq!(rewrite.endpoint, "/v1/chat/completions?foo=bar");
        assert_eq!(rewrite.passthrough_query.as_deref(), Some("foo=bar"));
    }

    #[test]
    fn rewrites_claude_messages_to_openai_responses_and_copilot_paths() {
        let responses = rewrite_claude_transform_endpoint(ClaudeTransformEndpointRewriteInput {
            endpoint: "/claude/v1/messages?beta=true&x-id=1",
            api_format: "openai_responses",
            is_copilot: false,
            gemini_model: None,
            gemini_stream: false,
        });
        let copilot_chat = rewrite_claude_transform_endpoint(ClaudeTransformEndpointRewriteInput {
            endpoint: "/v1/messages?beta=true&x-id=1",
            api_format: "anthropic",
            is_copilot: true,
            gemini_model: None,
            gemini_stream: false,
        });

        assert_eq!(responses.endpoint, "/v1/responses?x-id=1");
        assert_eq!(responses.passthrough_query.as_deref(), Some("x-id=1"));
        assert_eq!(copilot_chat.endpoint, "/chat/completions?x-id=1");
        assert_eq!(copilot_chat.passthrough_query.as_deref(), Some("x-id=1"));
    }

    #[test]
    fn rewrites_claude_messages_to_gemini_native_targets() {
        let non_stream = rewrite_claude_transform_endpoint(ClaudeTransformEndpointRewriteInput {
            endpoint: "/v1/messages?beta=true&x-id=1",
            api_format: "gemini_native",
            is_copilot: false,
            gemini_model: Some("gemini-2.5-pro"),
            gemini_stream: false,
        });
        let stream = rewrite_claude_transform_endpoint(ClaudeTransformEndpointRewriteInput {
            endpoint: "/v1/messages?beta=true",
            api_format: "gemini_native",
            is_copilot: false,
            gemini_model: Some("gemini-2.5-flash"),
            gemini_stream: true,
        });

        assert_eq!(
            non_stream.endpoint,
            "/v1beta/models/gemini-2.5-pro:generateContent?x-id=1"
        );
        assert_eq!(
            stream.endpoint,
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(stream.passthrough_query.as_deref(), Some("alt=sse"));
    }

    #[test]
    fn builds_claude_transform_rewrite_input_from_gemini_body() {
        let body = json!({ "model": "models/gemini-2.5-flash", "stream": true });
        let input = claude_transform_endpoint_rewrite_input_from_body(
            "/v1/messages?beta=true",
            "gemini_native",
            false,
            &body,
        );
        let rewrite = rewrite_claude_transform_endpoint(input);

        assert_eq!(
            rewrite.endpoint,
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(rewrite.passthrough_query.as_deref(), Some("alt=sse"));
    }

    #[test]
    fn extracts_gemini_model_from_path_variants() {
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro:generateContent").as_deref(),
            Some("gemini-pro"),
        );
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-1.5-flash:streamGenerateContent")
                .as_deref(),
            Some("gemini-1.5-flash"),
        );
        assert_eq!(
            extract_gemini_model_from_path("/gemini/v1beta/models/gemini-2.0-flash?key=abc")
                .as_deref(),
            Some("gemini-2.0-flash"),
        );
        assert_eq!(extract_gemini_model_from_path("/v1beta/models"), None);
        assert_eq!(extract_gemini_model_from_path("/v1beta/operations"), None);
    }

    #[test]
    fn infers_forward_request_model_from_body_or_gemini_path() {
        assert_eq!(
            request_model_for_forward(
                &AppKind::Codex,
                "/v1/responses",
                &json!({"model": " gpt-5 "})
            )
            .as_deref(),
            Some("gpt-5"),
        );
        assert_eq!(
            request_model_for_forward(
                &AppKind::Gemini,
                "/v1beta/models/gemini-2.0-flash:generateContent",
                &json!({"model": "ignored"})
            )
            .as_deref(),
            Some("gemini-2.0-flash"),
        );
        assert_eq!(
            request_model_for_forward(&AppKind::Codex, "/v1/responses", &json!({"model": "  "})),
            None
        );
    }

    #[test]
    fn infers_forward_interface_kind_from_app_and_endpoint() {
        assert_eq!(
            interface_kind_for_forward(&AppKind::Claude, "/v1/messages"),
            Some("anthropic_messages")
        );
        assert_eq!(
            interface_kind_for_forward(&AppKind::Codex, "/v1/responses?stream=1"),
            Some("openai_responses")
        );
        assert_eq!(
            interface_kind_for_forward(
                &AppKind::Custom("opencode".to_string()),
                "/v1/chat/completions"
            ),
            Some("openai_chat_completions")
        );
        assert_eq!(
            interface_kind_for_forward(&AppKind::Gemini, "/v1beta/models/gemini-pro"),
            Some("gemini_native")
        );
        assert_eq!(
            interface_kind_for_forward(&AppKind::Custom("other".to_string()), "/v1/responses"),
            None
        );
    }
}
