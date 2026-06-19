//! Thinking 优化器
//!
//! 核心优化规则位于 proxy-core；本模块保留旧 host API 和日志格式。

use super::types::OptimizerConfig;
use serde_json::Value;

/// 根据模型类型自动优化 thinking 配置
pub fn optimize(body: &mut Value, config: &OptimizerConfig) {
    let report =
        crate::proxy_core::optimize_thinking(body, &config.thinking_optimizer_core_config());

    if let Some(message) = crate::proxy_core::thinking_optimization_log_message(&report) {
        log::info!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn enabled_config() -> OptimizerConfig {
        OptimizerConfig {
            enabled: true,
            thinking_optimizer: true,
            cache_injection: true,
            cache_ttl: "1h".to_string(),
        }
    }

    fn disabled_config() -> OptimizerConfig {
        OptimizerConfig {
            enabled: true,
            thinking_optimizer: false,
            cache_injection: true,
            cache_ttl: "1h".to_string(),
        }
    }

    #[test]
    fn wrapper_applies_adaptive_thinking() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "thinking": {"type": "enabled", "budget_tokens": 8000},
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        assert_eq!(body["thinking"]["type"], "adaptive");
        assert!(body["thinking"].get("budget_tokens").is_none());
        assert_eq!(body["output_config"]["effort"], "max");
    }

    #[test]
    fn wrapper_applies_legacy_thinking() {
        let mut body = json!({
            "model": "anthropic.claude-sonnet-4-5-20250514-v1:0",
            "max_tokens": 8192,
            "thinking": {"type": "disabled"},
            "messages": [{"role": "user", "content": "hello"}]
        });

        optimize(&mut body, &enabled_config());

        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 8191);
    }

    #[test]
    fn wrapper_respects_disabled_optimizer() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let original = body.clone();

        optimize(&mut body, &disabled_config());

        assert_eq!(body, original);
    }
}
