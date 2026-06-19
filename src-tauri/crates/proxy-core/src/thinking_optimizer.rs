//! Bedrock thinking request optimization.

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThinkingOptimizerConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingOptimizationPath {
    Disabled,
    MissingModel,
    SkipHaiku,
    Adaptive,
    Legacy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingOptimizationReport {
    pub path: ThinkingOptimizationPath,
    pub model: Option<String>,
}

pub fn optimize_thinking(
    body: &mut Value,
    config: &ThinkingOptimizerConfig,
) -> ThinkingOptimizationReport {
    if !config.enabled {
        return ThinkingOptimizationReport {
            path: ThinkingOptimizationPath::Disabled,
            model: None,
        };
    }

    let model = match body.get("model").and_then(Value::as_str) {
        Some(model) => model.to_lowercase(),
        None => {
            return ThinkingOptimizationReport {
                path: ThinkingOptimizationPath::MissingModel,
                model: None,
            };
        }
    };

    if model.contains("haiku") {
        return ThinkingOptimizationReport {
            path: ThinkingOptimizationPath::SkipHaiku,
            model: Some(model),
        };
    }

    if uses_adaptive_thinking(&model) {
        body["thinking"] = json!({"type": "adaptive"});
        body["output_config"] = json!({"effort": "max"});
        append_beta(body, "context-1m-2025-08-07");
        return ThinkingOptimizationReport {
            path: ThinkingOptimizationPath::Adaptive,
            model: Some(model),
        };
    }

    let max_tokens = body
        .get("max_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(16384);
    let budget_target = max_tokens.saturating_sub(1);

    let thinking_type = body
        .get("thinking")
        .and_then(|thinking| thinking.get("type"))
        .and_then(Value::as_str)
        .map(ToString::to_string);

    match thinking_type.as_deref() {
        None | Some("disabled") => {
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": budget_target
            });
            append_beta(body, "interleaved-thinking-2025-05-14");
        }
        Some("enabled") => {
            let current_budget = body
                .get("thinking")
                .and_then(|thinking| thinking.get("budget_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if current_budget < budget_target {
                body["thinking"]["budget_tokens"] = json!(budget_target);
            }
            append_beta(body, "interleaved-thinking-2025-05-14");
        }
        _ => {
            append_beta(body, "interleaved-thinking-2025-05-14");
        }
    }

    ThinkingOptimizationReport {
        path: ThinkingOptimizationPath::Legacy,
        model: Some(model),
    }
}

pub fn thinking_optimization_log_message(report: &ThinkingOptimizationReport) -> Option<String> {
    match report.path {
        ThinkingOptimizationPath::Disabled | ThinkingOptimizationPath::MissingModel => None,
        ThinkingOptimizationPath::SkipHaiku => Some("[OPT] thinking: skip(haiku)".to_string()),
        ThinkingOptimizationPath::Adaptive => report
            .model
            .as_deref()
            .map(|model| format!("[OPT] thinking: adaptive({model})")),
        ThinkingOptimizationPath::Legacy => report
            .model
            .as_deref()
            .map(|model| format!("[OPT] thinking: legacy({model})")),
    }
}

fn uses_adaptive_thinking(model: &str) -> bool {
    let normalized = model.replace('.', "-");
    ["opus-4-8", "opus-4-7", "opus-4-6", "sonnet-4-6"]
        .iter()
        .any(|needle| normalized.contains(needle))
}

fn append_beta(body: &mut Value, beta: &str) {
    match body.get_mut("anthropic_beta") {
        Some(Value::Array(arr)) => {
            if arr.iter().any(|value| value.as_str() == Some(beta)) {
                return;
            }
            arr.push(json!(beta));
        }
        Some(Value::Null) | None => {
            body["anthropic_beta"] = json!([beta]);
        }
        _ => {
            body["anthropic_beta"] = json!([beta]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn enabled_config() -> ThinkingOptimizerConfig {
        ThinkingOptimizerConfig { enabled: true }
    }

    fn disabled_config() -> ThinkingOptimizerConfig {
        ThinkingOptimizerConfig { enabled: false }
    }

    #[test]
    fn log_message_matches_optimization_path() {
        assert_eq!(
            thinking_optimization_log_message(&ThinkingOptimizationReport {
                path: ThinkingOptimizationPath::Adaptive,
                model: Some("anthropic.claude-opus-4-6".to_string()),
            })
            .as_deref(),
            Some("[OPT] thinking: adaptive(anthropic.claude-opus-4-6)")
        );
        assert_eq!(
            thinking_optimization_log_message(&ThinkingOptimizationReport {
                path: ThinkingOptimizationPath::Legacy,
                model: Some("anthropic.claude-sonnet-4-5".to_string()),
            })
            .as_deref(),
            Some("[OPT] thinking: legacy(anthropic.claude-sonnet-4-5)")
        );
        assert_eq!(
            thinking_optimization_log_message(&ThinkingOptimizationReport {
                path: ThinkingOptimizationPath::SkipHaiku,
                model: Some("anthropic.claude-haiku".to_string()),
            })
            .as_deref(),
            Some("[OPT] thinking: skip(haiku)")
        );
        assert!(thinking_optimization_log_message(&ThinkingOptimizationReport {
            path: ThinkingOptimizationPath::Disabled,
            model: None,
        })
        .is_none());
    }

    #[test]
    fn adaptive_opus_4_8() {
        let mut body = json!({
            "model": "anthropic/claude-opus-4.8",
            "max_tokens": 16384,
            "thinking": {"type": "enabled", "budget_tokens": 8000},
            "messages": [{"role": "user", "content": "hello"}]
        });

        let report = optimize_thinking(&mut body, &enabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::Adaptive);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert!(body["thinking"].get("budget_tokens").is_none());
        assert_eq!(body["output_config"]["effort"], "max");
        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas.iter().any(|value| value == "context-1m-2025-08-07"));
    }

    #[test]
    fn adaptive_opus_4_6() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "thinking": {"type": "enabled", "budget_tokens": 8000},
            "messages": [{"role": "user", "content": "hello"}]
        });

        let report = optimize_thinking(&mut body, &enabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::Adaptive);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert!(body["thinking"].get("budget_tokens").is_none());
        assert_eq!(body["output_config"]["effort"], "max");
        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas.iter().any(|value| value == "context-1m-2025-08-07"));
    }

    #[test]
    fn adaptive_sonnet_4_6() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let report = optimize_thinking(&mut body, &enabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::Adaptive);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert!(body["thinking"].get("budget_tokens").is_none());
        assert_eq!(body["output_config"]["effort"], "max");
        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas.iter().any(|value| value == "context-1m-2025-08-07"));
    }

    #[test]
    fn legacy_sonnet_4_5_thinking_null() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let report = optimize_thinking(&mut body, &enabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::Legacy);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16383);
        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas
            .iter()
            .any(|value| value == "interleaved-thinking-2025-05-14"));
    }

    #[test]
    fn legacy_budget_too_small_upgraded() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "max_tokens": 16384,
            "thinking": {"type": "enabled", "budget_tokens": 1024},
            "messages": [{"role": "user", "content": "hello"}]
        });

        let report = optimize_thinking(&mut body, &enabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::Legacy);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16383);
    }

    #[test]
    fn skip_haiku() {
        let mut body = json!({
            "model": "anthropic.claude-haiku-4-5-20250514-v1:0",
            "max_tokens": 8192,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let original = body.clone();

        let report = optimize_thinking(&mut body, &enabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::SkipHaiku);
        assert_eq!(body, original);
    }

    #[test]
    fn disabled_optimizer_leaves_body_unchanged() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let original = body.clone();

        let report = optimize_thinking(&mut body, &disabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::Disabled);
        assert_eq!(body, original);
    }

    #[test]
    fn adaptive_dedups_beta() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "anthropic_beta": ["context-1m-2025-08-07"],
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize_thinking(&mut body, &enabled_config());

        let betas = body["anthropic_beta"].as_array().unwrap();
        let count = betas
            .iter()
            .filter(|value| value == &&json!("context-1m-2025-08-07"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn legacy_disabled_thinking_injected() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "max_tokens": 8192,
            "thinking": {"type": "disabled"},
            "messages": [{"role": "user", "content": "hello"}]
        });

        let report = optimize_thinking(&mut body, &enabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::Legacy);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 8191);
    }

    #[test]
    fn legacy_default_max_tokens() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "messages": [{"role": "user", "content": "hello"}]
        });

        let report = optimize_thinking(&mut body, &enabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::Legacy);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16383);
    }

    #[test]
    fn append_beta_null_field() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "anthropic_beta": null,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let report = optimize_thinking(&mut body, &enabled_config());

        assert_eq!(report.path, ThinkingOptimizationPath::Adaptive);
        let betas = body["anthropic_beta"].as_array().unwrap();
        assert!(betas.iter().any(|value| value == "context-1m-2025-08-07"));
    }
}
