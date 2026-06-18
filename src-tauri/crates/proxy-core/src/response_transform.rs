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
}
