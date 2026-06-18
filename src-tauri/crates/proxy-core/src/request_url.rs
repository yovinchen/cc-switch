use crate::AppKind;
use serde_json::Value;

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

pub fn split_endpoint_and_query(endpoint: &str) -> (&str, Option<&str>) {
    endpoint
        .split_once('?')
        .map_or((endpoint, None), |(path, query)| (path, Some(query)))
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

pub fn is_github_copilot_upstream(provider_type: Option<&str>, base_url: &str) -> bool {
    matches!(provider_type, Some("github_copilot")) || base_url.contains("githubcopilot.com")
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

pub fn resolve_codex_provider_uses_chat_completions(
    api_format: Option<&str>,
    wire_api: Option<&str>,
    base_url: Option<&str>,
    config_base_url: Option<&str>,
) -> bool {
    if let Some(api_format) = api_format {
        return is_codex_chat_wire_api(api_format);
    }

    if let Some(wire_api) = wire_api {
        return is_codex_chat_wire_api(wire_api);
    }

    if let Some(base_url) = base_url {
        return is_codex_chat_completions_url(base_url);
    }

    config_base_url
        .map(is_codex_chat_completions_url)
        .unwrap_or(false)
}

pub fn request_model_for_forward(
    app: &AppKind,
    endpoint: &str,
    body: &Value,
) -> Option<String> {
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
        append_query_to_full_url, extract_gemini_model_from_path, interface_kind_for_forward,
        is_codex_chat_completions_url, is_codex_chat_wire_api, is_codex_responses_endpoint,
        is_github_copilot_upstream, is_origin_only_url, merge_query_params, request_model_for_forward,
        resolved_copilot_dynamic_base_url, rewrite_claude_transform_endpoint,
        rewrite_codex_responses_endpoint_to_chat, should_convert_codex_responses_endpoint_to_chat,
        should_resolve_copilot_dynamic_endpoint, split_endpoint_and_query, strip_beta_query,
        AppKind, ClaudeTransformEndpointRewriteInput, resolve_codex_provider_uses_chat_completions,
    };
    use serde_json::json;

    #[test]
    fn split_endpoint_and_query_separates_first_query_marker() {
        assert_eq!(
            split_endpoint_and_query("/v1/messages?beta=true&x-id=1"),
            ("/v1/messages", Some("beta=true&x-id=1"))
        );
        assert_eq!(split_endpoint_and_query("/v1/messages"), ("/v1/messages", None));
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
        assert_eq!(merge_query_params(None, Some("alt=sse")), Some("alt=sse".to_string()));
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
    fn detects_github_copilot_upstream_from_provider_type_or_base_url() {
        assert!(is_github_copilot_upstream(
            Some("github_copilot"),
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
    fn extracts_gemini_model_from_path_variants() {
        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro:generateContent").as_deref(),
            Some("gemini-pro"),
        );
        assert_eq!(
            extract_gemini_model_from_path(
                "/v1beta/models/gemini-1.5-flash:streamGenerateContent"
            )
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
