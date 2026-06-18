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

#[cfg(test)]
mod tests {
    use super::*;

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
}
