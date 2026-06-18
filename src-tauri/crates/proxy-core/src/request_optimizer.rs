pub const BEDROCK_OPTIMIZER_ENV_FLAG: &str = "CLAUDE_CODE_USE_BEDROCK";

pub fn provider_declares_bedrock(use_bedrock_env: Option<&str>) -> bool {
    matches!(use_bedrock_env, Some("1"))
}

pub fn should_apply_bedrock_pre_send_optimizer(
    optimizer_enabled: bool,
    use_bedrock_env: Option<&str>,
) -> bool {
    optimizer_enabled && provider_declares_bedrock(use_bedrock_env)
}

#[cfg(test)]
mod tests {
    use super::{provider_declares_bedrock, should_apply_bedrock_pre_send_optimizer};

    #[test]
    fn bedrock_provider_detection_matches_existing_env_flag_contract() {
        assert!(provider_declares_bedrock(Some("1")));
        assert!(!provider_declares_bedrock(Some("0")));
        assert!(!provider_declares_bedrock(Some("true")));
        assert!(!provider_declares_bedrock(Some("")));
        assert!(!provider_declares_bedrock(None));
    }

    #[test]
    fn bedrock_pre_send_optimizer_requires_feature_and_provider_flags() {
        assert!(should_apply_bedrock_pre_send_optimizer(true, Some("1")));
        assert!(!should_apply_bedrock_pre_send_optimizer(false, Some("1")));
        assert!(!should_apply_bedrock_pre_send_optimizer(true, Some("0")));
        assert!(!should_apply_bedrock_pre_send_optimizer(true, None));
    }
}
