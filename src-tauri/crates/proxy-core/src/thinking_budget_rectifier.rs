//! Thinking budget request rectification.

use serde_json::Value;

const MAX_THINKING_BUDGET: u64 = 32000;
const MAX_TOKENS_VALUE: u64 = 64000;
const MIN_MAX_TOKENS_FOR_BUDGET: u64 = MAX_THINKING_BUDGET + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThinkingBudgetRectifierConfig {
    pub enabled: bool,
    pub request_thinking_budget: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BudgetRectifySnapshot {
    pub max_tokens: Option<u64>,
    pub thinking_type: Option<String>,
    pub thinking_budget_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BudgetRectifyResult {
    pub applied: bool,
    pub before: BudgetRectifySnapshot,
    pub after: BudgetRectifySnapshot,
}

pub fn should_rectify_thinking_budget(
    error_message: Option<&str>,
    config: &ThinkingBudgetRectifierConfig,
) -> bool {
    if !config.enabled || !config.request_thinking_budget {
        return false;
    }

    let Some(msg) = error_message else {
        return false;
    };
    let lower = msg.to_lowercase();

    let has_budget_tokens_reference =
        lower.contains("budget_tokens") || lower.contains("budget tokens");
    let has_thinking_reference = lower.contains("thinking");
    let has_1024_constraint = lower.contains("greater than or equal to 1024")
        || lower.contains(">= 1024")
        || (lower.contains("1024") && lower.contains("input should be"));

    has_budget_tokens_reference && has_thinking_reference && has_1024_constraint
}

pub fn rectify_thinking_budget(body: &mut Value) -> BudgetRectifyResult {
    let before = snapshot_budget(body);

    if before.thinking_type.as_deref() == Some("adaptive") {
        return BudgetRectifyResult {
            applied: false,
            before: before.clone(),
            after: before,
        };
    }

    if !body.get("thinking").is_some_and(Value::is_object) {
        body["thinking"] = Value::Object(serde_json::Map::new());
    }

    let Some(thinking) = body.get_mut("thinking").and_then(|t| t.as_object_mut()) else {
        return BudgetRectifyResult {
            applied: false,
            before: before.clone(),
            after: before,
        };
    };

    thinking.insert("type".to_string(), Value::String("enabled".to_string()));
    thinking.insert(
        "budget_tokens".to_string(),
        Value::Number(MAX_THINKING_BUDGET.into()),
    );

    if before.max_tokens.is_none() || before.max_tokens < Some(MIN_MAX_TOKENS_FOR_BUDGET) {
        body["max_tokens"] = Value::Number(MAX_TOKENS_VALUE.into());
    }

    let after = snapshot_budget(body);
    BudgetRectifyResult {
        applied: before != after,
        before,
        after,
    }
}

fn snapshot_budget(body: &Value) -> BudgetRectifySnapshot {
    let max_tokens = body.get("max_tokens").and_then(Value::as_u64);
    let thinking = body.get("thinking").and_then(Value::as_object);
    let thinking_type = thinking
        .and_then(|t| t.get("type"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let thinking_budget_tokens = thinking
        .and_then(|t| t.get("budget_tokens"))
        .and_then(Value::as_u64);
    BudgetRectifySnapshot {
        max_tokens,
        thinking_type,
        thinking_budget_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn enabled_config() -> ThinkingBudgetRectifierConfig {
        ThinkingBudgetRectifierConfig {
            enabled: true,
            request_thinking_budget: true,
        }
    }

    fn budget_disabled_config() -> ThinkingBudgetRectifierConfig {
        ThinkingBudgetRectifierConfig {
            enabled: true,
            request_thinking_budget: false,
        }
    }

    fn master_disabled_config() -> ThinkingBudgetRectifierConfig {
        ThinkingBudgetRectifierConfig {
            enabled: false,
            request_thinking_budget: true,
        }
    }

    #[test]
    fn detects_budget_tokens_thinking_error() {
        assert!(should_rectify_thinking_budget(
            Some("thinking.budget_tokens: Input should be greater than or equal to 1024"),
            &enabled_config()
        ));
    }

    #[test]
    fn ignores_budget_tokens_max_tokens_error() {
        assert!(!should_rectify_thinking_budget(
            Some("budget_tokens must be less than max_tokens"),
            &enabled_config()
        ));
    }

    #[test]
    fn ignores_budget_tokens_without_thinking_and_1024_context() {
        assert!(!should_rectify_thinking_budget(
            Some("budget_tokens: value must be at least 1024"),
            &enabled_config()
        ));
    }

    #[test]
    fn detects_budget_tokens_with_thinking_and_1024_error() {
        assert!(should_rectify_thinking_budget(
            Some("thinking budget_tokens must be >= 1024"),
            &enabled_config()
        ));
    }

    #[test]
    fn ignores_unrelated_or_missing_error() {
        assert!(!should_rectify_thinking_budget(
            Some("Request timeout"),
            &enabled_config()
        ));
        assert!(!should_rectify_thinking_budget(None, &enabled_config()));
    }

    #[test]
    fn respects_disabled_budget_config() {
        assert!(!should_rectify_thinking_budget(
            Some("thinking.budget_tokens: Input should be greater than or equal to 1024"),
            &budget_disabled_config()
        ));
    }

    #[test]
    fn respects_master_disabled_config() {
        assert!(!should_rectify_thinking_budget(
            Some("thinking.budget_tokens: Input should be greater than or equal to 1024"),
            &master_disabled_config()
        ));
    }

    #[test]
    fn rectifies_budget_basic() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "enabled", "budget_tokens": 512 },
            "max_tokens": 1024
        });

        let result = rectify_thinking_budget(&mut body);

        assert!(result.applied);
        assert_eq!(result.before.thinking_type.as_deref(), Some("enabled"));
        assert_eq!(result.after.thinking_type.as_deref(), Some("enabled"));
        assert_eq!(result.before.thinking_budget_tokens, Some(512));
        assert_eq!(result.after.thinking_budget_tokens, Some(32000));
        assert_eq!(result.before.max_tokens, Some(1024));
        assert_eq!(result.after.max_tokens, Some(64000));
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 32000);
        assert_eq!(body["max_tokens"], 64000);
    }

    #[test]
    fn skips_adaptive_thinking() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "adaptive", "budget_tokens": 512 },
            "max_tokens": 1024
        });

        let result = rectify_thinking_budget(&mut body);

        assert!(!result.applied);
        assert_eq!(result.before, result.after);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["thinking"]["budget_tokens"], 512);
        assert_eq!(body["max_tokens"], 1024);
    }

    #[test]
    fn preserves_large_max_tokens() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "enabled", "budget_tokens": 512 },
            "max_tokens": 100000
        });

        let result = rectify_thinking_budget(&mut body);

        assert!(result.applied);
        assert_eq!(result.before.max_tokens, Some(100000));
        assert_eq!(result.after.max_tokens, Some(100000));
        assert_eq!(body["max_tokens"], 100000);
    }

    #[test]
    fn creates_thinking_object_when_missing() {
        let mut body = json!({
            "model": "claude-test",
            "max_tokens": 1024
        });

        let result = rectify_thinking_budget(&mut body);

        assert!(result.applied);
        assert_eq!(result.before.thinking_type, None);
        assert_eq!(result.after.thinking_type.as_deref(), Some("enabled"));
        assert_eq!(result.after.thinking_budget_tokens, Some(32000));
        assert_eq!(result.after.max_tokens, Some(64000));
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 32000);
        assert_eq!(body["max_tokens"], 64000);
    }

    #[test]
    fn creates_max_tokens_when_missing() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "enabled", "budget_tokens": 512 }
        });

        let result = rectify_thinking_budget(&mut body);

        assert!(result.applied);
        assert_eq!(result.before.max_tokens, None);
        assert_eq!(result.after.max_tokens, Some(64000));
        assert_eq!(body["max_tokens"], 64000);
    }

    #[test]
    fn normalizes_non_enabled_type() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "disabled", "budget_tokens": 512 },
            "max_tokens": 1024
        });

        let result = rectify_thinking_budget(&mut body);

        assert!(result.applied);
        assert_eq!(result.before.thinking_type.as_deref(), Some("disabled"));
        assert_eq!(result.after.thinking_type.as_deref(), Some("enabled"));
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 32000);
        assert_eq!(body["max_tokens"], 64000);
    }

    #[test]
    fn no_change_when_already_valid() {
        let mut body = json!({
            "model": "claude-test",
            "thinking": { "type": "enabled", "budget_tokens": 32000 },
            "max_tokens": 64001
        });

        let result = rectify_thinking_budget(&mut body);

        assert!(!result.applied);
        assert_eq!(result.before, result.after);
        assert_eq!(body["thinking"]["budget_tokens"], 32000);
        assert_eq!(body["max_tokens"], 64001);
    }
}
