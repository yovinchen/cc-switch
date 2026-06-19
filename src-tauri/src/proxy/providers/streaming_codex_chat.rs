//! OpenAI Chat Completions SSE → OpenAI Responses SSE conversion.

#[cfg(test)]
use crate::proxy_core::build_codex_tool_context_from_request;
use crate::proxy_core::{
    append_utf8_safe, extract_chat_sse_error, strip_sse_field, take_sse_block,
    CodexChatToResponsesState, CodexToolContext,
};
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;

/// Create a stream that converts Chat Completions SSE chunks into Responses SSE events.
#[allow(dead_code)]
pub fn create_responses_sse_stream_from_chat<E: std::error::Error + Send + 'static>(
    stream: impl Stream<Item = Result<Bytes, E>> + Send + 'static,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    create_responses_sse_stream_from_chat_with_context(stream, CodexToolContext::default())
}

/// Create a stream that converts Chat Completions SSE chunks into Responses SSE
/// events while restoring Codex tool namespace/custom/tool_search metadata.
pub fn create_responses_sse_stream_from_chat_with_context<E: std::error::Error + Send + 'static>(
    stream: impl Stream<Item = Result<Bytes, E>> + Send + 'static,
    tool_context: CodexToolContext,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    async_stream::stream! {
        let mut buffer = String::new();
        let mut utf8_remainder: Vec<u8> = Vec::new();
        let mut state = CodexChatToResponsesState::with_tool_context(tool_context);
        let mut stream_failed = false;

        tokio::pin!(stream);

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    append_utf8_safe(&mut buffer, &mut utf8_remainder, &bytes);

                    while let Some(block) = take_sse_block(&mut buffer) {
                        if block.trim().is_empty() {
                            continue;
                        }

                        let mut event_name: Option<String> = None;
                        let mut data_parts: Vec<String> = Vec::new();
                        for line in block.lines() {
                            if let Some(event) = strip_sse_field(line, "event") {
                                event_name = Some(event.trim().to_string());
                            }
                            if let Some(data) = strip_sse_field(line, "data") {
                                data_parts.push(data.to_string());
                            }
                        }

                        if data_parts.is_empty() {
                            continue;
                        }

                        let data = data_parts.join("\n");
                        if data.trim() == "[DONE]" {
                            for event in state.finalize() {
                                yield Ok(event);
                            }
                            continue;
                        }

                        let chunk: Value = match serde_json::from_str(&data) {
                            Ok(value) => value,
                            Err(_) => continue,
                        };

                        if event_name.as_deref() == Some("error") || chunk.get("error").is_some() {
                            let (message, error_type) = extract_chat_sse_error(&chunk);
                            yield Ok(state.failed_event(message, error_type));
                            stream_failed = true;
                            break;
                        }

                        for event in state.handle_chat_chunk(&chunk) {
                            yield Ok(event);
                        }
                    }

                    if stream_failed {
                        break;
                    }
                }
                Err(e) => {
                    yield Ok(state.failed_event(
                        format!("Stream error: {e}"),
                        Some("stream_error".to_string()),
                    ));
                    stream_failed = true;
                    break;
                }
            }
        }

        if !stream_failed {
            if state.is_completed() || state.has_finish_reason() {
                for event in state.finalize() {
                    yield Ok(event);
                }
            } else if state.has_substantive_output() {
                state.set_finish_reason("length");
                for event in state.finalize() {
                    yield Ok(event);
                }
            } else {
                yield Ok(state.failed_event(
                    "Upstream Chat Completions stream ended before sending finish_reason".to_string(),
                    Some("stream_truncated".to_string()),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{stream, StreamExt};

    async fn collect(chunks: Vec<&str>) -> String {
        collect_with_context(chunks, CodexToolContext::default()).await
    }

    async fn collect_with_context(chunks: Vec<&str>, tool_context: CodexToolContext) -> String {
        let chunks: Vec<Result<Bytes, std::io::Error>> = chunks
            .into_iter()
            .map(|chunk| Ok(Bytes::copy_from_slice(chunk.as_bytes())))
            .collect();
        let upstream = stream::iter(chunks);
        let converted = create_responses_sse_stream_from_chat_with_context(upstream, tool_context);
        let bytes: Vec<Bytes> = converted.map(|item| item.unwrap()).collect().await;
        String::from_utf8(bytes.concat()).unwrap()
    }

    #[tokio::test]
    async fn converts_text_chat_sse_to_responses_sse() {
        let output = collect(vec![
            "data: {\"id\":\"chatcmpl_1\",\"created\":123,\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"created\":123,\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2,\"total_tokens\":6}}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(output.contains("event: response.created"));
        assert!(output.contains("event: response.output_text.delta"));
        assert!(output.contains("\"text\":\"Hello\""));
        assert!(output.contains("event: response.completed"));
        assert!(output.contains("\"input_tokens\":4"));
    }

    #[tokio::test]
    async fn converts_reasoning_content_chat_sse_to_responses_reasoning_events() {
        let output = collect(vec![
            "data: {\"id\":\"chatcmpl_reason\",\"created\":123,\"model\":\"deepseek-reasoner\",\"choices\":[{\"delta\":{\"reasoning_content\":\"Need context. \"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_reason\",\"created\":123,\"model\":\"deepseek-reasoner\",\"choices\":[{\"delta\":{\"reasoning\":\"Now answer. \"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_reason\",\"created\":123,\"model\":\"deepseek-reasoner\",\"choices\":[{\"delta\":{\"content\":\"Done\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":6,\"total_tokens\":10,\"completion_tokens_details\":{\"reasoning_tokens\":3}}}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(output.contains("event: response.reasoning_summary_part.added"));
        assert!(output.contains("event: response.reasoning_summary_text.delta"));
        assert!(output.contains("event: response.reasoning_summary_text.done"));
        assert!(output.contains("Need context. Now answer. "));
        assert!(output.contains("\"type\":\"reasoning\""));
        assert!(output.contains("\"text\":\"Done\""));
        assert!(output.contains("\"reasoning_tokens\":3"));

        let reasoning_pos = output.find("\"type\":\"reasoning\"").unwrap();
        let message_pos = output.find("\"type\":\"message\"").unwrap();
        assert!(reasoning_pos < message_pos);
    }

    #[tokio::test]
    async fn converts_inline_think_chat_sse_to_reasoning_without_leaking_tags() {
        let output = collect(vec![
            "data: {\"id\":\"chatcmpl_minimax\",\"created\":123,\"model\":\"MiniMax-M2.7\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"<think>\\nNeed\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_minimax\",\"created\":123,\"model\":\"MiniMax-M2.7\",\"choices\":[{\"delta\":{\"content\":\" context.</think>\\n\\npong\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"id\":\"chatcmpl_minimax\",\"created\":123,\"model\":\"MiniMax-M2.7\",\"choices\":[],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":6,\"total_tokens\":10,\"completion_tokens_details\":{\"reasoning_tokens\":3}}}\n\n",
        ])
        .await;

        assert!(output.contains("event: response.reasoning_summary_text.delta"));
        assert!(output.contains("Need context."));
        assert!(output.contains("\"text\":\"pong\""));
        assert!(output.contains("\"reasoning_tokens\":3"));
        assert!(!output.contains("<think>"));
        assert!(!output.contains("</think>"));
        assert!(output.contains("event: response.completed"));
    }

    #[tokio::test]
    async fn converts_tool_call_chat_sse_to_responses_sse() {
        let output = collect(vec![
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\\\"Tokyo\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(output.contains("event: response.function_call_arguments.delta"));
        assert!(output.contains("event: response.function_call_arguments.done"));
        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("\"call_id\":\"call_1\""));
    }

    #[tokio::test]
    async fn restores_custom_tool_input_stream_events() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{ "type": "custom", "name": "exec" }]
        });
        let context = build_codex_tool_context_from_request(&request);
        let output = collect_with_context(
            vec![
                "data: {\"id\":\"chatcmpl_custom\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_custom\",\"type\":\"function\",\"function\":{\"name\":\"exec\"}}]}}]}\n\n",
                "data: {\"id\":\"chatcmpl_custom\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"input\\\":\"}}]}}]}\n\n",
                "data: {\"id\":\"chatcmpl_custom\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"ls -la\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n",
            ],
            context,
        )
        .await;

        assert!(output.contains("event: response.custom_tool_call_input.delta"));
        assert!(output.contains("event: response.custom_tool_call_input.done"));
        assert!(!output.contains("event: response.function_call_arguments.delta"));
        assert!(!output.contains("event: response.function_call_arguments.done"));
        assert!(output.contains("\"id\":\"ctc_call_custom\""));
        assert!(output.contains("\"type\":\"custom_tool_call\""));
        assert!(output.contains("\"name\":\"exec\""));
        assert!(output.contains("\"input\":\"ls -la\""));
    }

    #[tokio::test]
    async fn canonicalizes_streamed_tool_call_arguments_on_done_events() {
        let output = collect(vec![
            "data: {\"id\":\"chatcmpl_args\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_args\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{ \\\"b\\\": 2,\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_args\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\" \\\"a\\\": 1 }\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(output.contains(r#""arguments":"{\"a\":1,\"b\":2}""#));
    }

    #[tokio::test]
    async fn preserves_reasoning_content_on_streamed_tool_call_items() {
        let output = collect(vec![
            "data: {\"id\":\"chatcmpl_tool_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"reasoning_content\":\"Need file.\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\\\"README.md\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(output.contains("event: response.output_item.done"));
        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("\"reasoning_content\":\"Need file.\""));
    }

    #[tokio::test]
    async fn preserves_late_reasoning_content_on_streamed_tool_call_items() {
        let output = collect(vec![
            "data: {\"id\":\"chatcmpl_tool_late_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_late_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\\\"README.md\\\"}\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_late_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"reasoning_content\":\"Need file.\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_late_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(output.contains("event: response.output_item.done"));
        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("\"reasoning_content\":\"Need file.\""));
    }

    #[tokio::test]
    async fn restores_namespace_on_streamed_tool_call_items() {
        let request = json!({
            "model": "gpt-5.4",
            "input": [{
                "type": "tool_search_output",
                "call_id": "call_tool_search_1",
                "tools": [{
                    "type": "namespace",
                    "name": "mcp__codex_apps__gmail",
                    "tools": [{
                        "type": "function",
                        "name": "_search_emails",
                        "description": "Search Gmail.",
                        "parameters": {"type": "object"}
                    }]
                }]
            }]
        });
        let context = build_codex_tool_context_from_request(&request);
        let output = collect_with_context(
            vec![
                "data: {\"id\":\"chatcmpl_gmail\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_gmail\",\"type\":\"function\",\"function\":{\"name\":\"mcp__codex_apps__gmail___search_emails\"}}]}}]}\n\n",
                "data: {\"id\":\"chatcmpl_gmail\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"query\\\":\\\"in:inbox\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n",
            ],
            context,
        )
        .await;

        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("\"namespace\":\"mcp__codex_apps__gmail\""));
        assert!(output.contains("\"name\":\"_search_emails\""));
        assert!(output.contains(r#""arguments":"{\"query\":\"in:inbox\"}""#));
    }

    #[tokio::test]
    async fn restores_tool_search_on_streamed_tool_call_items() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{"type": "tool_search"}],
            "input": "Search for Gmail tools."
        });
        let context = build_codex_tool_context_from_request(&request);
        let output = collect_with_context(
            vec![
                "data: {\"id\":\"chatcmpl_tool_search\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_tool_search_1\",\"type\":\"function\",\"function\":{\"name\":\"tool_search\"}}]}}]}\n\n",
                "data: {\"id\":\"chatcmpl_tool_search\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"query\\\":\\\"Gmail search emails\\\",\\\"limit\\\":10}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n",
            ],
            context,
        )
        .await;

        assert!(output.contains("\"type\":\"tool_search_call\""));
        assert!(output.contains("\"execution\":\"client\""));
        assert!(output.contains("\"call_id\":\"call_tool_search_1\""));
        assert!(output.contains("\"query\":\"Gmail search emails\""));
    }

    #[tokio::test]
    async fn stream_error_emits_failed_without_completed() {
        let upstream = stream::iter(vec![Err::<Bytes, std::io::Error>(std::io::Error::other(
            "boom",
        ))]);
        let converted = create_responses_sse_stream_from_chat(upstream);
        let bytes: Vec<Bytes> = converted.map(|item| item.unwrap()).collect().await;
        let output = String::from_utf8(bytes.concat()).unwrap();

        assert!(output.contains("event: response.failed"));
        assert!(!output.contains("event: response.completed"));
    }

    #[tokio::test]
    async fn stream_end_with_output_without_finish_reason_emits_incomplete_without_failed() {
        let output = collect(vec![
            "data: {\"id\":\"chatcmpl_truncated\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
        ])
        .await;

        assert!(output.contains("event: response.completed"));
        assert!(output.contains("\"status\":\"incomplete\""));
        assert!(output.contains("\"incomplete_details\":{\"reason\":\"max_output_tokens\"}"));
        assert!(!output.contains("event: response.failed"));
    }

    #[tokio::test]
    async fn stream_end_without_output_or_finish_reason_emits_failed_without_completed() {
        let output = collect(vec![
            "data: {\"id\":\"chatcmpl_truncated\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{}}]}\n\n",
        ])
        .await;

        assert!(output.contains("event: response.failed"));
        assert!(output.contains("stream_truncated"));
        assert!(!output.contains("event: response.completed"));
    }

    #[tokio::test]
    async fn chat_sse_error_event_emits_failed_without_completed() {
        let output = collect(vec![
            "event: error\ndata: {\"error\":{\"message\":\"bad request\",\"type\":\"invalid_request_error\"}}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(output.contains("event: response.failed"));
        assert!(output.contains("bad request"));
        assert!(output.contains("invalid_request_error"));
        assert!(!output.contains("event: response.completed"));
    }

    #[tokio::test]
    async fn chat_sse_data_only_error_emits_failed_without_completed() {
        let output = collect(vec![
            "data: {\"error\":{\"message\":\"quota exceeded\",\"code\":\"rate_limit_exceeded\"}}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(output.contains("event: response.failed"));
        assert!(output.contains("quota exceeded"));
        assert!(output.contains("rate_limit_exceeded"));
        assert!(!output.contains("event: response.completed"));
    }
}
