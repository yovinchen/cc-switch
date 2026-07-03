#[cfg(test)]
use crate::provider::Provider;

#[cfg(test)]
mod tests {
    use crate::proxy::provider::{
        transform_claude_response_for_api_format, transform_claude_sse_for_api_format,
    };
    use crate::proxy_core::api::transforms::{
        should_preserve_reasoning_content_for_openai_chat, GeminiShadowStore,
    };
    use serde_json::json;

    use super::*;
    use crate::provider::ProviderMeta;
    use crate::proxy_core::api::transforms::normalize_claude_anthropic_messages;
    use bytes::Bytes;
    use std::sync::Arc;

    #[test]
    fn claude_provider_projects_response_facades() {
        let chat_response =
            crate::proxy_core::api::transforms::openai_chat_to_anthropic_message(&json!({
            "id": "chatcmpl_1",
            "model": "chat-model",
            "choices": [{
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
            }))
            .expect("chat response");
        assert_eq!(chat_response["content"][0]["text"], "Hi");
        let delegated_chat_response = transform_claude_response_for_api_format(
            &json!({
            "id": "chatcmpl_1",
            "model": "chat-model",
            "choices": [{
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
            }),
            "openai_chat",
            None,
            None,
            None,
            None,
        )
        .expect("delegated chat response");
        assert_eq!(delegated_chat_response["content"][0]["text"], "Hi");

        let responses_response =
            crate::proxy_core::api::transforms::openai_responses_to_anthropic_message(&json!({
            "id": "resp_1",
            "model": "responses-model",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "Done"}]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 2}
            }))
            .expect("responses response");
        assert_eq!(responses_response["content"][0]["text"], "Done");
        let delegated_responses_response = transform_claude_response_for_api_format(
            &json!({
            "id": "resp_1",
            "model": "responses-model",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "Done"}]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 2}
            }),
            "openai_responses",
            None,
            None,
            None,
            None,
        )
        .expect("delegated responses response");
        assert_eq!(delegated_responses_response["content"][0]["text"], "Done");

        let explicit_chat_response = transform_claude_response_for_api_format(
            &json!({
                "id": "chatcmpl_2",
                "model": "chat-model",
                "choices": [{
                    "message": {"role": "assistant", "content": "Explicit chat"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 2}
            }),
            "openai_chat",
            None,
            None,
            None,
            None,
        )
        .expect("explicit chat response");
        assert_eq!(
            explicit_chat_response["content"][0]["text"],
            "Explicit chat"
        );

        let explicit_responses_response = transform_claude_response_for_api_format(
            &json!({
                "id": "resp_2",
                "model": "responses-model",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{"type": "output_text", "text": "Explicit responses"}]
                }],
                "usage": {"input_tokens": 1, "output_tokens": 2}
            }),
            "openai_responses",
            None,
            None,
            None,
            None,
        )
        .expect("explicit responses response");
        assert_eq!(
            explicit_responses_response["content"][0]["text"],
            "Explicit responses"
        );

        let gemini_output =
            crate::proxy_core::api::transforms::gemini_response_to_anthropic_message(
                &json!({
                    "responseId": "gemini_1",
                    "candidates": [{
                        "content": {
                            "role": "model",
                            "parts": [{"text": "Gemini hi"}]
                        },
                        "finishReason": "STOP"
                    }],
                    "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 2}
                }),
                None,
                || "toolu_test".to_string(),
            )
            .expect("gemini response");
        assert_eq!(gemini_output.response["content"][0]["text"], "Gemini hi");
        let delegated_gemini_response = transform_claude_response_for_api_format(
            &json!({
            "responseId": "gemini_1",
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Gemini hi"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 2}
            }),
            "gemini_native",
            None,
            None,
            None,
            None,
        )
        .expect("delegated gemini response");
        assert_eq!(delegated_gemini_response["content"][0]["text"], "Gemini hi");

        assert!(should_preserve_reasoning_content_for_openai_chat(
            &json!({}),
            &json!({"model": "deepseek-v4-pro"})
        ));
        let reasoning_provider = Provider::with_id(
            "reasoning".to_string(),
            "Reasoning".to_string(),
            json!({}),
            None,
        );
        assert!(should_preserve_reasoning_content_for_openai_chat(
            &reasoning_provider.settings_config,
            &json!({"model": "deepseek-v4-pro"})
        ));

        let mut normalize_provider = Provider::with_id(
            "claude-normalize".to_string(),
            "Claude Normalize".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic"
                }
            }),
            None,
        );
        normalize_provider.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });
        let mut normalize_body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "messages": [{ "role": "user", "content": "hello" }]
        });
        assert!(normalize_claude_anthropic_messages(
            &mut normalize_body,
            &normalize_provider.settings_config,
            "anthropic"
        ));
        assert!(normalize_body.get("output_config").is_none());
        let mut non_anthropic_body = normalize_body.clone();
        assert!(!normalize_claude_anthropic_messages(
            &mut non_anthropic_body,
            &normalize_provider.settings_config,
            "openai_chat"
        ));
    }

    #[tokio::test]
    async fn claude_stream_transform_provider_dispatches_api_formats() {
        use futures::StreamExt as _;

        let chat_stream = futures::stream::iter(vec![
            Ok::<_, std::io::Error>(Bytes::from_static(
                b"data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
            )),
            Ok(Bytes::from_static(
                b"data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
            )),
            Ok(Bytes::from_static(b"data: [DONE]\n\n")),
        ]);
        let chat_output =
            transform_claude_sse_for_api_format(chat_stream, "openai_chat", None, None, None, None)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .map(|item| {
                    String::from_utf8(item.expect("chat chunk").to_vec()).expect("chat utf8")
                })
                .collect::<String>();
        assert!(chat_output.contains("event: message_start"));
        assert!(chat_output.contains("Hi"));

        let responses_stream = futures::stream::iter(vec![Ok::<_, std::io::Error>(
            Bytes::from_static(
                b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-4o\",\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\nevent: response.content_part.added\ndata: {\"type\":\"response.content_part.added\",\"part\":{\"type\":\"output_text\",\"text\":\"\"},\"output_index\":0,\"content_index\":0}\n\nevent: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Done\",\"output_index\":0,\"content_index\":0}\n\nevent: response.content_part.done\ndata: {\"type\":\"response.content_part.done\",\"output_index\":0,\"content_index\":0}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
            ),
        )]);
        let responses_output = transform_claude_sse_for_api_format(
            responses_stream,
            "openai_responses",
            None,
            None,
            None,
            None,
        )
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|item| {
            String::from_utf8(item.expect("responses chunk").to_vec()).expect("responses utf8")
        })
        .collect::<String>();
        assert!(responses_output.contains("event: message_start"));
        assert!(responses_output.contains("Done"));

        let gemini_stream = futures::stream::iter(vec![Ok::<_, std::io::Error>(
            Bytes::from_static(
                b"data: {\"responseId\":\"gemini_1\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"text\":\"Gemini hi\"}]}}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n",
            ),
        )]);
        let gemini_output = transform_claude_sse_for_api_format(
            gemini_stream,
            "gemini_native",
            Some(Arc::new(GeminiShadowStore::default())),
            Some("provider-a".to_string()),
            Some("session-a".to_string()),
            None,
        )
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|item| String::from_utf8(item.expect("gemini chunk").to_vec()).expect("gemini utf8"))
        .collect::<String>();
        assert!(gemini_output.contains("event: message_start"));
        assert!(gemini_output.contains("Gemini hi"));
    }
}
