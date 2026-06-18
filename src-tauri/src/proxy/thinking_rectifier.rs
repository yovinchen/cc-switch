//! Thinking Signature 整流器
//!
//! 核心整流规则位于 proxy-core；本模块保留旧 host API，并负责把
//! `RectifierConfig` 投影成 core 的中立配置。

use super::types::RectifierConfig;
use serde_json::Value;

pub type RectifyResult = crate::proxy_core::RectifyResult;

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

pub fn rectify_anthropic_request(body: &mut Value) -> RectifyResult {
    crate::proxy_core::rectify_anthropic_request(body)
}

pub fn normalize_thinking_type(body: Value) -> Value {
    crate::proxy_core::normalize_thinking_type(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn wrapper_rectifies_request_body() {
        let mut body = json!({
            "model": "claude-test",
            "messages": [{
                "role": "assistant",
                "content": [
                    { "type": "thinking", "thinking": "t", "signature": "sig" },
                    { "type": "text", "text": "hello", "signature": "sig_text" }
                ]
            }]
        });

        let result = rectify_anthropic_request(&mut body);

        assert!(result.applied);
        assert_eq!(result.removed_thinking_blocks, 1);
        assert_eq!(result.removed_signature_fields, 1);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0].get("signature").is_none());
    }

    #[test]
    fn wrapper_preserves_normalize_thinking_type_behavior() {
        let body = json!({
            "model": "claude-test",
            "thinking": { "type": "adaptive", "budget_tokens": 5000 }
        });

        let result = normalize_thinking_type(body);

        assert_eq!(result["thinking"]["type"], "adaptive");
        assert_eq!(result["thinking"]["budget_tokens"], 5000);
    }
}
