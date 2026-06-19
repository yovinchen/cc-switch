use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use serde_json::Value;

pub use crate::proxy_core::UNSUPPORTED_IMAGE_MARKER;

/// Replace image blocks before sending when the routed model is text-only.
///
/// Two paths, both reached only when the caller's media-fallback switch is on:
/// - explicit capability from the provider config (modelCatalog / modalities) is
///   always trusted — it is declaration-driven, never a guess;
/// - the curated `known_text_only_model` list is a heuristic *prediction* and only
///   runs when `allow_heuristic` is true, so a mislabeled multimodal model cannot
///   have its images silently stripped when the user opts out.
pub fn replace_images_for_text_only_model(
    body: &mut Value,
    provider: &Provider,
    allow_heuristic: bool,
) -> usize {
    crate::proxy_core::replace_images_for_text_only_model(
        body,
        &provider.settings_config,
        allow_heuristic,
    )
}

pub fn contains_image_blocks(body: &Value) -> bool {
    crate::proxy_core::contains_image_blocks(body)
}

pub fn replace_image_blocks_with_marker(body: &mut Value) -> usize {
    crate::proxy_core::replace_image_blocks_with_marker(body)
}

pub fn is_unsupported_image_error(error: &ProxyError) -> bool {
    let ProxyError::UpstreamError { status, body } = error else {
        return false;
    };

    crate::proxy_core::is_unsupported_image_error(*status, body.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use serde_json::json;

    fn provider(settings_config: Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test".to_string(),
            settings_config,
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn keeps_images_when_model_capability_is_unknown() {
        let provider = provider(json!({}));
        let mut body = json!({
            "model": "unknown-model",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "look" },
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 0);
        assert_eq!(body["messages"][0]["content"][1]["type"], "image");
    }

    #[test]
    fn known_text_only_models_replace_images_before_send() {
        let provider = provider(json!({}));
        let mut body = json!({
            "model": "deepseek/deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 1);
        assert_eq!(
            body["messages"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn known_text_only_models_replace_chat_image_url_before_send() {
        let provider = provider(json!({}));
        let mut body = json!({
            "model": "deepseek-v4-flash",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "look" },
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 1);
        assert_eq!(body["messages"][0]["content"][1]["type"], "text");
        assert_eq!(
            body["messages"][0]["content"][1]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn known_text_only_models_replace_codex_input_image_before_send() {
        let provider = provider(json!({}));
        let mut body = json!({
            "model": "deepseek-v4-flash",
            "input": [{
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "look" },
                    { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 1);
        assert_eq!(body["input"][0]["content"][1]["type"], "input_text");
        assert_eq!(
            body["input"][0]["content"][1]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn explicit_text_modalities_replace_images_before_send() {
        let provider = provider(json!({
            "models": [
                { "id": "deepseek-v4-pro", "input": ["text"] }
            ]
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "look" },
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 1);
        assert_eq!(body["messages"][0]["content"][0]["text"], "look");
        assert_eq!(body["messages"][0]["content"][1]["type"], "text");
        assert_eq!(
            body["messages"][0]["content"][1]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn preserves_images_without_explicit_capability_even_for_unknown_models() {
        let provider = provider(json!({}));
        let mut body = json!({
            "model": "unknown-model",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 0);
        assert_eq!(body["messages"][0]["content"][0]["type"], "image");
    }

    #[test]
    fn explicit_text_modalities_can_override_visual_model_ids() {
        let provider = provider(json!({
            "models": [
                { "id": "gpt-4o", "input": ["text"] }
            ]
        }));
        let mut body = json!({
            "model": "gpt-4o",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 1);
        assert_eq!(
            body["messages"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn explicit_image_modalities_preserve_model_images() {
        let provider = provider(json!({
            "modelCatalog": {
                "models": [
                    { "model": "deepseek-v4-pro", "modalities": { "input": ["text", "image"] } }
                ]
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 0);
        assert_eq!(body["messages"][0]["content"][0]["type"], "image");
    }

    #[test]
    fn known_mimo_pro_replaces_but_mimo_multimodal_preserves() {
        let provider = provider(json!({}));
        let mut pro_body = json!({
            "model": "xiaomi-mimo-token-plan/mimo-v2.5-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        let mut multimodal_body = json!({
            "model": "xiaomi-mimo-token-plan/mimo-v2.5",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let pro_count = replace_images_for_text_only_model(&mut pro_body, &provider, true);
        let multimodal_count =
            replace_images_for_text_only_model(&mut multimodal_body, &provider, true);

        assert_eq!(pro_count, 1);
        assert_eq!(multimodal_count, 0);
        assert_eq!(
            multimodal_body["messages"][0]["content"][0]["type"],
            "image"
        );
    }

    #[test]
    fn multimodal_kimi_model_is_not_on_text_only_list() {
        let provider = provider(json!({}));
        let mut body = json!({
            "model": "kimi/kimi-k2.6",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 0);
        assert_eq!(body["messages"][0]["content"][0]["type"], "image");
    }

    #[test]
    fn known_text_only_prefixes_replace_images_before_send() {
        let provider = provider(json!({}));
        let mut body = json!({
            "model": "therouter/qwen/qwen3-coder-480b",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, true);

        assert_eq!(count, 1);
        assert_eq!(
            body["messages"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn unconditional_marker_replacement_handles_retry_path() {
        let mut body = json!({
            "model": "xiaomi-mimo-token-plan/mimo-v2.5-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        assert!(contains_image_blocks(&body));
        let count = replace_image_blocks_with_marker(&mut body);

        assert_eq!(count, 1);
        assert_eq!(
            body["messages"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn replaces_nested_tool_result_image_blocks() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "toolu_1",
                    "content": [
                        { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                    ]
                }]
            }]
        });

        let count = replace_image_blocks_with_marker(&mut body);

        assert_eq!(count, 1);
        assert_eq!(
            body["messages"][0]["content"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn detects_unsupported_image_errors() {
        let error = ProxyError::UpstreamError {
            status: 400,
            body: Some(
                r#"{"error":{"message":"This model does not support image input"}}"#.to_string(),
            ),
        };

        assert!(is_unsupported_image_error(&error));
    }

    #[test]
    fn ignores_non_image_errors() {
        let error = ProxyError::UpstreamError {
            status: 400,
            body: Some(r#"{"error":{"message":"Invalid API key"}}"#.to_string()),
        };

        assert!(!is_unsupported_image_error(&error));
    }

    #[test]
    fn preserves_cache_control_when_replacing_image() {
        // image block 可能承载 prompt cache 断点；替换成标记时必须把
        // cache_control 迁移到新的 text block，否则会断掉缓存命中。
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "image",
                    "source": { "type": "base64", "media_type": "image/png", "data": "abc" },
                    "cache_control": { "type": "ephemeral" }
                }]
            }]
        });

        let count = replace_image_blocks_with_marker(&mut body);

        assert_eq!(count, 1);
        let block = &body["messages"][0]["content"][0];
        assert_eq!(block["type"], "text");
        assert_eq!(block["text"], UNSUPPORTED_IMAGE_MARKER);
        assert_eq!(block["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn detects_media_and_attachment_error_phrasings() {
        let media_error = ProxyError::UpstreamError {
            status: 400,
            body: Some(
                r#"{"error":{"message":"This model cannot process media inputs"}}"#.to_string(),
            ),
        };
        assert!(is_unsupported_image_error(&media_error));

        let attachment_error = ProxyError::UpstreamError {
            status: 422,
            body: Some(r#"{"message":"attachments are not supported by this model"}"#.to_string()),
        };
        assert!(is_unsupported_image_error(&attachment_error));
    }

    #[test]
    fn detects_chat_content_unknown_variant_image_url_errors() {
        let error = ProxyError::UpstreamError {
            status: 400,
            body: Some(
                r#"{"error":{"message":"Failed to deserialize the JSON body into the target type: messages[11]: unknown variant image_url, expected text"}}"#
                    .to_string(),
            ),
        };

        assert!(is_unsupported_image_error(&error));
    }

    #[test]
    fn heuristic_disabled_keeps_images_for_listed_text_only_models() {
        // allow_heuristic = false：内置列表不再预测性剥图，避免误判多模态模型时静默丢图。
        let provider = provider(json!({}));
        let mut body = json!({
            "model": "deepseek/deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, false);

        assert_eq!(count, 0);
        assert_eq!(body["messages"][0]["content"][0]["type"], "image");
    }

    #[test]
    fn explicit_text_capability_replaces_even_when_heuristic_disabled() {
        // 显式声明 text-only 是声明驱动、零误判，即使关掉启发式也应生效。
        let provider = provider(json!({
            "models": [
                { "id": "deepseek-v4-pro", "input": ["text"] }
            ]
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &provider, false);

        assert_eq!(count, 1);
        assert_eq!(
            body["messages"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }
}
