//! Thinking Budget 整流器
//!
//! 核心整流规则位于 proxy-core；本模块保留旧 host API，并负责把
//! `RectifierConfig` 投影成 core 的中立配置。

use super::types::RectifierConfig;
use serde_json::Value;

fn budget_rectifier_config(
    config: &RectifierConfig,
) -> crate::proxy_core::ThinkingBudgetRectifierConfig {
    crate::proxy_core::ThinkingBudgetRectifierConfig {
        enabled: config.enabled,
        request_thinking_budget: config.request_thinking_budget,
    }
}

pub fn should_rectify_thinking_budget(
    error_message: Option<&str>,
    config: &RectifierConfig,
) -> bool {
    crate::proxy_core::should_rectify_thinking_budget(
        error_message,
        &budget_rectifier_config(config),
    )
}

pub fn rectify_thinking_budget(body: &mut Value) -> crate::proxy_core::BudgetRectifyResult {
    crate::proxy_core::rectify_thinking_budget(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(enabled: bool, request_thinking_budget: bool) -> RectifierConfig {
        RectifierConfig {
            enabled,
            request_thinking_signature: true,
            request_thinking_budget,
            request_media_fallback: true,
            request_media_heuristic: true,
        }
    }

    #[test]
    fn projects_host_config_to_core_detection() {
        let message = Some("thinking.budget_tokens: Input should be greater than or equal to 1024");

        assert!(should_rectify_thinking_budget(message, &config(true, true)));
        assert!(!should_rectify_thinking_budget(
            message,
            &config(false, true)
        ));
        assert!(!should_rectify_thinking_budget(
            message,
            &config(true, false)
        ));
    }

    #[test]
    fn wrapper_rectifies_request_body() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "enabled", "budget_tokens": 512 },
            "max_tokens": 1024
        });

        let result = rectify_thinking_budget(&mut body);

        assert!(result.applied);
        assert_eq!(result.before.thinking_budget_tokens, Some(512));
        assert_eq!(result.after.thinking_budget_tokens, Some(32000));
        assert_eq!(result.after.max_tokens, Some(64000));
        assert_eq!(body["thinking"]["budget_tokens"], 32000);
        assert_eq!(body["max_tokens"], 64000);
    }
}
