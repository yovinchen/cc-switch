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
    use super::should_preserve_exact_request_header_case;

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
}
