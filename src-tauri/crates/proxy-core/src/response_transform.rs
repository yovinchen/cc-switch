use crate::UpstreamSseAggregationKind;
use serde_json::Value;

pub const CLAUDE_API_FORMAT_METADATA_KEY: &str = "claudeApiFormat";

pub fn claude_api_format_from_metadata(metadata: &Value, fallback: &str) -> String {
    metadata
        .get(CLAUDE_API_FORMAT_METADATA_KEY)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

pub fn claude_api_format_needs_transform(api_format: &str) -> bool {
    matches!(
        api_format,
        "openai_chat" | "openai_responses" | "gemini_native"
    )
}

pub fn resolve_claude_api_format(
    provider_type: Option<&str>,
    meta_api_format: Option<&str>,
    settings_api_format: Option<&str>,
    openrouter_compat_mode: Option<&Value>,
) -> &'static str {
    if provider_type == Some("codex_oauth") {
        return "openai_responses";
    }

    if let Some(api_format) = meta_api_format {
        return normalize_configured_claude_api_format(api_format);
    }

    if let Some(api_format) = settings_api_format {
        return normalize_configured_claude_api_format(api_format);
    }

    if openrouter_compat_mode_enabled(openrouter_compat_mode) {
        "openai_chat"
    } else {
        "anthropic"
    }
}

fn normalize_configured_claude_api_format(api_format: &str) -> &'static str {
    match api_format {
        "openai_chat" => "openai_chat",
        "openai_responses" => "openai_responses",
        "gemini_native" => "gemini_native",
        _ => "anthropic",
    }
}

fn openrouter_compat_mode_enabled(raw: Option<&Value>) -> bool {
    match raw {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(number)) => number.as_i64().unwrap_or(0) != 0,
        Some(Value::String(value)) => {
            let normalized = value.trim().to_lowercase();
            normalized == "true" || normalized == "1"
        }
        _ => false,
    }
}

pub fn should_aggregate_codex_oauth_responses_sse(
    requested_streaming: bool,
    api_format: &str,
    is_codex_oauth: bool,
) -> bool {
    !requested_streaming && is_codex_oauth && api_format == "openai_responses"
}

pub fn should_use_claude_transform_streaming(
    requested_streaming: bool,
    upstream_is_sse: bool,
    api_format: &str,
    is_codex_oauth: bool,
) -> bool {
    requested_streaming || upstream_is_sse || (is_codex_oauth && api_format == "openai_responses")
}

pub fn claude_transform_unlabeled_sse_aggregation(
    api_format: &str,
) -> Option<UpstreamSseAggregationKind> {
    match api_format {
        "gemini_native" => None,
        "openai_responses" => Some(UpstreamSseAggregationKind::Responses),
        _ => Some(UpstreamSseAggregationKind::ChatCompletions),
    }
}

pub fn map_openai_responses_stop_reason_to_anthropic(
    status: Option<&str>,
    has_tool_use: bool,
    incomplete_reason: Option<&str>,
) -> Option<&'static str> {
    status.map(|value| match value {
        "completed" if has_tool_use => "tool_use",
        "incomplete"
            if matches!(
                incomplete_reason,
                Some("max_output_tokens") | Some("max_tokens")
            ) || incomplete_reason.is_none() =>
        {
            "max_tokens"
        }
        "incomplete" => "end_turn",
        _ => "end_turn",
    })
}

pub fn sanitize_anthropic_tool_use_input(name: &str, input: Value) -> Value {
    if name != "Read" {
        return input;
    }

    match input {
        Value::Object(mut object) => {
            if matches!(object.get("pages"), Some(Value::String(value)) if value.is_empty()) {
                object.remove("pages");
            }
            Value::Object(object)
        }
        other => other,
    }
}

pub fn sanitize_anthropic_tool_use_input_json(name: &str, raw: &str) -> String {
    if name != "Read" || raw.is_empty() {
        return raw.to_string();
    }

    let Ok(input) = serde_json::from_str::<Value>(raw) else {
        return raw.to_string();
    };

    serde_json::to_string(&sanitize_anthropic_tool_use_input(name, input))
        .unwrap_or_else(|_| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn metadata_with_claude_api_format(value: &str) -> Value {
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            CLAUDE_API_FORMAT_METADATA_KEY.to_string(),
            Value::String(value.to_string()),
        );
        Value::Object(metadata)
    }

    #[test]
    fn claude_api_format_from_metadata_uses_non_empty_metadata_value() {
        let metadata = metadata_with_claude_api_format("openai_responses");

        assert_eq!(
            claude_api_format_from_metadata(&metadata, "openai_chat"),
            "openai_responses"
        );
    }

    #[test]
    fn claude_api_format_from_metadata_falls_back_for_missing_or_blank_values() {
        assert_eq!(
            claude_api_format_from_metadata(&json!({}), "openai_chat"),
            "openai_chat"
        );
        assert_eq!(
            claude_api_format_from_metadata(
                &metadata_with_claude_api_format(" "),
                "openai_chat"
            ),
            "openai_chat"
        );
    }

    #[test]
    fn claude_api_format_needs_transform_only_for_translated_formats() {
        assert!(!claude_api_format_needs_transform("anthropic"));
        assert!(claude_api_format_needs_transform("openai_chat"));
        assert!(claude_api_format_needs_transform("openai_responses"));
        assert!(claude_api_format_needs_transform("gemini_native"));
        assert!(!claude_api_format_needs_transform("unknown"));
    }

    #[test]
    fn resolve_claude_api_format_uses_codex_oauth_first() {
        assert_eq!(
            resolve_claude_api_format(
                Some("codex_oauth"),
                Some("anthropic"),
                Some("openai_chat"),
                None,
            ),
            "openai_responses"
        );
    }

    #[test]
    fn resolve_claude_api_format_prefers_meta_then_settings_then_openrouter() {
        assert_eq!(
            resolve_claude_api_format(
                None,
                Some("gemini_native"),
                Some("openai_chat"),
                Some(&json!(true)),
            ),
            "gemini_native"
        );
        assert_eq!(
            resolve_claude_api_format(None, Some("unknown"), Some("openai_chat"), None),
            "anthropic"
        );
        assert_eq!(
            resolve_claude_api_format(None, None, Some("unknown"), Some(&json!("1"))),
            "anthropic"
        );
        assert_eq!(
            resolve_claude_api_format(None, None, None, Some(&json!("1"))),
            "openai_chat"
        );
        assert_eq!(resolve_claude_api_format(None, None, None, None), "anthropic");
    }

    #[test]
    fn claude_transform_streaming_uses_requested_streaming() {
        assert!(should_use_claude_transform_streaming(
            true,
            false,
            "openai_chat",
            false,
        ));
    }

    #[test]
    fn claude_transform_streaming_uses_upstream_sse() {
        assert!(should_use_claude_transform_streaming(
            false,
            true,
            "openai_chat",
            false,
        ));
    }

    #[test]
    fn codex_oauth_responses_force_streaming_by_default() {
        assert!(should_use_claude_transform_streaming(
            false,
            false,
            "openai_responses",
            true,
        ));
    }

    #[test]
    fn regular_openai_responses_can_stay_non_streaming() {
        assert!(!should_use_claude_transform_streaming(
            false,
            false,
            "openai_responses",
            false,
        ));
    }

    #[test]
    fn codex_oauth_responses_aggregates_only_for_non_streaming_requests() {
        assert!(should_aggregate_codex_oauth_responses_sse(
            false,
            "openai_responses",
            true,
        ));
        assert!(!should_aggregate_codex_oauth_responses_sse(
            true,
            "openai_responses",
            true,
        ));
        assert!(!should_aggregate_codex_oauth_responses_sse(
            false,
            "openai_chat",
            true,
        ));
        assert!(!should_aggregate_codex_oauth_responses_sse(
            false,
            "openai_responses",
            false,
        ));
    }

    #[test]
    fn claude_transform_unlabeled_sse_aggregation_matches_api_format() {
        assert_eq!(
            claude_transform_unlabeled_sse_aggregation("openai_responses"),
            Some(UpstreamSseAggregationKind::Responses)
        );
        assert_eq!(
            claude_transform_unlabeled_sse_aggregation("openai_chat"),
            Some(UpstreamSseAggregationKind::ChatCompletions)
        );
        assert_eq!(
            claude_transform_unlabeled_sse_aggregation("gemini_native"),
            None
        );
    }

    #[test]
    fn maps_openai_responses_status_to_anthropic_stop_reason() {
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(Some("completed"), true, None),
            Some("tool_use")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(
                Some("incomplete"),
                false,
                Some("max_output_tokens")
            ),
            Some("max_tokens")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(
                Some("incomplete"),
                false,
                Some("max_tokens")
            ),
            Some("max_tokens")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(Some("incomplete"), false, None),
            Some("max_tokens")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(
                Some("incomplete"),
                false,
                Some("content_filter")
            ),
            Some("end_turn")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(None, true, Some("max_tokens")),
            None
        );
    }

    #[test]
    fn sanitizes_read_tool_empty_pages_for_anthropic() {
        let input = sanitize_anthropic_tool_use_input(
            "Read",
            json!({
                "file_path": "/tmp/demo.py",
                "limit": 2000,
                "offset": 0,
                "pages": ""
            }),
        );

        assert_eq!(input["file_path"], "/tmp/demo.py");
        assert_eq!(input["limit"], 2000);
        assert_eq!(input["offset"], 0);
        assert!(input.get("pages").is_none());
    }

    #[test]
    fn read_tool_sanitizer_preserves_other_tools_and_non_empty_pages() {
        let other_tool = sanitize_anthropic_tool_use_input(
            "Search",
            json!({
                "query": "pages",
                "pages": ""
            }),
        );
        assert_eq!(other_tool["pages"], "");

        let non_empty_pages = sanitize_anthropic_tool_use_input(
            "Read",
            json!({
                "file_path": "/tmp/demo.py",
                "pages": "1-2"
            }),
        );
        assert_eq!(non_empty_pages["pages"], "1-2");
    }

    #[test]
    fn sanitizes_read_tool_json_without_rewriting_invalid_input() {
        assert_eq!(
            sanitize_anthropic_tool_use_input_json(
                "Read",
                r#"{"file_path":"/tmp/demo.py","pages":""}"#
            ),
            r#"{"file_path":"/tmp/demo.py"}"#
        );
        assert_eq!(
            sanitize_anthropic_tool_use_input_json("Read", "{not-json"),
            "{not-json"
        );
        assert_eq!(
            sanitize_anthropic_tool_use_input_json("Search", r#"{"pages":""}"#),
            r#"{"pages":""}"#
        );
    }
}
