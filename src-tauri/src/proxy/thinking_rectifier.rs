//! Thinking Signature 整流器
//!
//! 核心整流规则位于 proxy-core；本模块保留旧 host API，并负责把
//! `RectifierConfig` 投影成 core 的中立配置。

use super::types::RectifierConfig;

fn signature_rectifier_config(
    config: &RectifierConfig,
) -> crate::proxy_core::ThinkingSignatureRectifierConfig {
    crate::proxy_core::ThinkingSignatureRectifierConfig {
        enabled: config.enabled,
        request_thinking_signature: config.request_thinking_signature,
    }
}

pub fn should_rectify_thinking_signature(
    error_message: Option<&str>,
    config: &RectifierConfig,
) -> bool {
    crate::proxy_core::should_rectify_thinking_signature(
        error_message,
        &signature_rectifier_config(config),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(enabled: bool, request_thinking_signature: bool) -> RectifierConfig {
        RectifierConfig {
            enabled,
            request_thinking_signature,
            request_thinking_budget: true,
            request_media_fallback: true,
            request_media_heuristic: true,
        }
    }

    #[test]
    fn projects_host_config_to_core_detection() {
        let message = Some("messages.1.content.0: Invalid `signature` in `thinking` block");

        assert!(should_rectify_thinking_signature(
            message,
            &config(true, true)
        ));
        assert!(!should_rectify_thinking_signature(
            message,
            &config(false, true)
        ));
        assert!(!should_rectify_thinking_signature(
            message,
            &config(true, false)
        ));
    }
}
