//! Codex Responses ↔ OpenAI Chat Completions conversion.
//!
//! This module is used when the Codex client talks to CC Switch through the
//! Responses API, while the selected upstream provider only exposes an
//! OpenAI-compatible Chat Completions endpoint.

use crate::provider::CodexChatReasoningConfig;
use crate::proxy::error::ProxyError;
pub(crate) use crate::proxy_core::{
    append_responses_input_as_chat_messages, apply_codex_chat_reasoning_options,
    build_codex_tool_context_from_request, chat_message_to_response_output_item,
    chat_reasoning_text, chat_reasoning_to_response_output_item,
    chat_tool_calls_to_response_output_items, chat_usage_to_responses_usage,
    collapse_system_messages_to_head, custom_tool_input_from_chat_arguments,
    response_id_from_chat_id, response_status_from_finish_reason,
    response_tool_call_item_from_chat_name, response_tool_call_item_id_from_chat_name,
    responses_instruction_text, responses_tool_choice_to_chat_tool_choice,
    CodexChatReasoningOptions, CodexToolContext,
};
use serde_json::{json, Value};

const EXTRA_CHAT_PASSTHROUGH_FIELDS: &[&str] = &[
    "frequency_penalty",
    "logit_bias",
    "logprobs",
    "metadata",
    "n",
    "parallel_tool_calls",
    "presence_penalty",
    "response_format",
    "seed",
    "service_tier",
    "stop",
    "stream_options",
    "top_logprobs",
    "user",
];

/// Convert an OpenAI Responses request into an OpenAI Chat Completions request.
#[allow(dead_code)]
pub fn responses_to_chat_completions(body: Value) -> Result<Value, ProxyError> {
    responses_to_chat_completions_with_reasoning(body, None)
}

/// Convert an OpenAI Responses request into an OpenAI Chat Completions request,
/// using provider-declared Codex Chat reasoning capabilities when available.
pub fn responses_to_chat_completions_with_reasoning(
    body: Value,
    reasoning_config: Option<&CodexChatReasoningConfig>,
) -> Result<Value, ProxyError> {
    let mut result = json!({});
    let tool_context = build_codex_tool_context_from_request(&body);

    if let Some(model) = body.get("model") {
        result["model"] = model.clone();
    }

    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        let instructions = responses_instruction_text(instructions);
        if !instructions.is_empty() {
            messages.push(json!({
                "role": "system",
                "content": instructions
            }));
        }
    }

    if let Some(input) = body.get("input") {
        append_responses_input_as_chat_messages(input, &mut messages, &tool_context);
    }
    let messages = collapse_system_messages_to_head(messages);
    result["messages"] = json!(messages);

    let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(max_tokens) = body.get("max_output_tokens") {
        if super::transform::is_openai_o_series(model) {
            result["max_completion_tokens"] = max_tokens.clone();
        } else {
            result["max_tokens"] = max_tokens.clone();
        }
    }
    if let Some(max_tokens) = body.get("max_tokens") {
        result["max_tokens"] = max_tokens.clone();
    }
    if let Some(max_tokens) = body.get("max_completion_tokens") {
        result["max_completion_tokens"] = max_tokens.clone();
    }

    for key in ["temperature", "top_p", "stream"] {
        if let Some(value) = body.get(key) {
            result[key] = value.clone();
        }
    }

    let reasoning_options = reasoning_config.map(codex_chat_reasoning_options_from_provider);
    apply_codex_chat_reasoning_options(
        &mut result,
        &body,
        reasoning_options.as_ref(),
        super::transform::supports_reasoning_effort(model),
    );

    let tools = tool_context.chat_tools();
    if !tools.is_empty() {
        result["tools"] = json!(tools);
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] =
            responses_tool_choice_to_chat_tool_choice(tool_choice, &tool_context);
    }

    for key in EXTRA_CHAT_PASSTHROUGH_FIELDS {
        if let Some(value) = body.get(*key) {
            result[*key] = value.clone();
        }
    }

    // Strict OpenAI-compatible upstreams (vLLM, enterprise gateways) reject
    // requests that carry tool_choice or parallel_tool_calls without a non-empty
    // tools array. Drop both fields when tools ended up absent or empty after
    // conversion to avoid 503/400 from such providers.
    let has_tools = result
        .get("tools")
        .is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty()));
    if !has_tools {
        if let Some(obj) = result.as_object_mut() {
            obj.remove("tool_choice");
            obj.remove("parallel_tool_calls");
        }
    }
    // OpenAI 兼容上游在流式下默认不在 SSE 里返回 usage，必须显式声明
    // include_usage 才会在末尾吐 usage chunk。Codex CLI 用 Responses 协议、
    // 自身不带 stream_options，缺这一注入会导致 kimi/MiniMax 等第三方流式请求的
    // token/成本/缓存命中率全部漏记（input/output/cache 全为 0）。
    // 与 Claude→openai_chat 路径共用同一 helper，保证两个客户端方向一致。
    crate::proxy_core::inject_openai_stream_include_usage(&mut result);

    Ok(result)
}

fn codex_chat_reasoning_options_from_provider(
    config: &CodexChatReasoningConfig,
) -> CodexChatReasoningOptions {
    CodexChatReasoningOptions {
        supports_thinking: config.supports_thinking,
        supports_effort: config.supports_effort,
        thinking_param: config.thinking_param.clone(),
        effort_param: config.effort_param.clone(),
        effort_value_mode: config.effort_value_mode.clone(),
    }
}

/// Convert a non-streaming Chat Completions response into a Responses response.
#[allow(dead_code)]
pub fn chat_completion_to_response(body: Value) -> Result<Value, ProxyError> {
    chat_completion_to_response_with_context(body, &CodexToolContext::default())
}

/// Convert a non-streaming Chat Completions response into a Responses response,
/// restoring Codex-specific tool names using the original Responses request.
pub(crate) fn chat_completion_to_response_with_context(
    body: Value,
    tool_context: &CodexToolContext,
) -> Result<Value, ProxyError> {
    let choices = body
        .get("choices")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ProxyError::TransformError("No choices in chat response".to_string()))?;
    let choice = choices
        .first()
        .ok_or_else(|| ProxyError::TransformError("Empty choices in chat response".to_string()))?;
    let message = choice
        .get("message")
        .ok_or_else(|| ProxyError::TransformError("No message in chat choice".to_string()))?;

    let response_id = response_id_from_chat_id(body.get("id").and_then(|v| v.as_str()));
    let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("");
    let created_at = body.get("created").and_then(|v| v.as_u64()).unwrap_or(0);
    let finish_reason = choice.get("finish_reason").and_then(|v| v.as_str());

    let reasoning = chat_reasoning_text(message);
    let mut output = Vec::new();
    if let Some(reasoning_item) =
        chat_reasoning_to_response_output_item(reasoning.as_deref(), &response_id)
    {
        output.push(reasoning_item);
    }
    if let Some(message_item) = chat_message_to_response_output_item(message, &response_id) {
        output.push(message_item);
    }
    output.extend(chat_tool_calls_to_response_output_items(
        message,
        reasoning.as_deref(),
        tool_context,
    ));

    let mut response = json!({
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": response_status_from_finish_reason(finish_reason),
        "model": model,
        "output": output,
        "usage": chat_usage_to_responses_usage(body.get("usage"))
    });

    if finish_reason == Some("length") {
        response["incomplete_details"] = json!({ "reason": "max_output_tokens" });
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_request_with_stream_injects_include_usage() {
        let input = json!({
            "model": "kimi-k2.6",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}],
            "stream": true
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert_eq!(result["stream"], true);
        assert_eq!(result["stream_options"]["include_usage"], true);
    }

    #[test]
    fn responses_request_without_stream_omits_stream_options() {
        let input = json!({
            "model": "kimi-k2.6",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}]
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(result.get("stream_options").is_none());
    }

    #[test]
    fn responses_request_merges_include_usage_into_existing_stream_options() {
        let input = json!({
            "model": "kimi-k2.6",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}],
            "stream": true,
            "stream_options": {"continuous_usage_stats": true}
        });

        let result = responses_to_chat_completions(input).unwrap();

        // 既补上 include_usage，又保留客户端原有的 stream_options 字段。
        assert_eq!(result["stream_options"]["include_usage"], true);
        assert_eq!(result["stream_options"]["continuous_usage_stats"], true);
    }

    #[test]
    fn responses_request_maps_input_file_content_parts() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [{
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "Summarize this."},
                    {
                        "type": "input_file",
                        "file_id": "file_123",
                        "file_url": "https://example.com/spec.pdf",
                        "filename": "spec.pdf"
                    },
                    {
                        "type": "input_audio",
                        "input_audio": {
                            "data": "UklGRg==",
                            "format": "wav"
                        }
                    }
                ]
            }]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let content = result["messages"][0]["content"].as_array().unwrap();

        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "file");
        assert_eq!(content[1]["file"]["file_id"], "file_123");
        assert!(content[1]["file"].get("file_url").is_none());
        assert_eq!(content[1]["file"]["filename"], "spec.pdf");
        assert_eq!(content[2]["type"], "input_audio");
        assert_eq!(content[2]["input_audio"]["format"], "wav");
    }

    #[test]
    fn responses_request_does_not_emit_chat_file_for_url_only_input_file() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [{
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "Summarize this URL file."},
                    {
                        "type": "input_file",
                        "file_url": "https://example.com/spec.pdf"
                    }
                ]
            }]
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert_eq!(result["messages"][0]["content"], "Summarize this URL file.");
    }

    #[test]
    fn responses_request_maps_top_level_input_file_item() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "input_file",
                    "file_id": "file_top",
                    "filename": "top.pdf"
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let content = result["messages"][0]["content"].as_array().unwrap();

        assert_eq!(result["messages"][0]["role"], "user");
        assert_eq!(content[0]["type"], "file");
        assert_eq!(content[0]["file"]["file_id"], "file_top");
        assert_eq!(content[0]["file"]["filename"], "top.pdf");
    }

    #[test]
    fn top_level_user_content_part_clears_pending_reasoning() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "reasoning",
                    "summary": [{"text": "stale reasoning"}]
                },
                {
                    "type": "input_text",
                    "text": "Please run the tool."
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "lookup",
                    "arguments": "{}"
                }
            ],
            "tools": [{
                "type": "function",
                "name": "lookup",
                "parameters": {"type": "object"}
            }]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "Please run the tool.");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["reasoning_content"], "tool call");
    }

    #[test]
    fn responses_request_to_chat_maps_messages_tools_and_limits() {
        let input = json!({
            "model": "gpt-5.4",
            "instructions": "You are concise.",
            "input": [
                {
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": "Weather?"},
                        {"type": "input_image", "image_url": "data:image/png;base64,abc"},
                        {"type": "input_text", "text": "Use Celsius."}
                    ]
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "get_weather",
                    "arguments": "{\"city\":\"Tokyo\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "Sunny"
                }
            ],
            "tools": [{
                "type": "function",
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {"type": "object"},
                "strict": true
            }],
            "tool_choice": {"type": "function", "name": "get_weather"},
            "max_output_tokens": 100,
            "reasoning": {"effort": "high"},
            "stream": true
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert_eq!(result["model"], "gpt-5.4");
        assert_eq!(result["messages"][0]["role"], "system");
        assert_eq!(result["messages"][1]["role"], "user");
        assert_eq!(result["messages"][1]["content"][0]["type"], "text");
        assert_eq!(result["messages"][1]["content"][1]["type"], "image_url");
        assert_eq!(result["messages"][1]["content"][2]["type"], "text");
        assert_eq!(result["messages"][1]["content"][2]["text"], "Use Celsius.");
        assert_eq!(result["messages"][2]["tool_calls"][0]["id"], "call_1");
        assert_eq!(result["messages"][3]["role"], "tool");
        assert_eq!(result["tools"][0]["function"]["name"], "get_weather");
        assert_eq!(result["tools"][0]["function"]["strict"], true);
        assert_eq!(result["tool_choice"]["function"]["name"], "get_weather");
        assert_eq!(result["max_tokens"], 100);
        assert_eq!(result["reasoning_effort"], "high");
    }

    #[test]
    fn responses_request_to_chat_exposes_tool_search_and_loaded_namespace_tools() {
        let input = json!({
            "model": "gpt-5.4",
            "tools": [{"type": "tool_search"}],
            "input": [
                {
                    "type": "tool_search_call",
                    "call_id": "call_tool_search_1",
                    "status": "completed",
                    "execution": "client",
                    "arguments": {"query": "Gmail search emails", "limit": 5}
                },
                {
                    "type": "tool_search_output",
                    "call_id": "call_tool_search_1",
                    "status": "completed",
                    "execution": "client",
                    "tools": [{
                        "type": "namespace",
                        "name": "mcp__codex_apps__gmail",
                        "description": "Find and reference emails from your inbox.",
                        "tools": [{
                            "type": "function",
                            "name": "_search_emails",
                            "description": "Search Gmail for emails matching a query.",
                            "strict": false,
                            "parameters": {
                                "type": "object",
                                "properties": {
                                    "query": {"type": "string"},
                                    "max_results": {"type": "integer"}
                                },
                                "required": ["query"]
                            }
                        }]
                    }]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": "Search unread inbox mail."
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let tools = result["tools"].as_array().unwrap();
        let tool_names = tools
            .iter()
            .filter_map(|tool| tool.pointer("/function/name").and_then(|v| v.as_str()))
            .collect::<Vec<_>>();

        assert!(tool_names.contains(&"tool_search"));
        assert!(tool_names.contains(&"mcp__codex_apps__gmail___search_emails"));
        assert_eq!(
            result["messages"][0]["tool_calls"][0]["function"]["name"],
            "tool_search"
        );
        assert_eq!(result["messages"][1]["role"], "tool");
        assert_eq!(result["messages"][1]["tool_call_id"], "call_tool_search_1");
        assert!(result["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("mcp__codex_apps__gmail"));
    }

    #[test]
    fn responses_request_to_chat_maps_custom_tool_and_choice() {
        let input = json!({
            "model": "gpt-5.4",
            "tools": [{
                "type": "custom",
                "name": "apply_patch",
                "description": "Apply a patch to files."
            }],
            "tool_choice": {"type": "custom", "name": "apply_patch"},
            "input": [{
                "type": "custom_tool_call",
                "id": "ctc_1",
                "call_id": "call_patch",
                "name": "apply_patch",
                "input": "*** Begin Patch\n*** End Patch"
            }]
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert_eq!(result["tools"][0]["function"]["name"], "apply_patch");
        assert_eq!(
            result["tools"][0]["function"]["parameters"]["required"][0],
            "input"
        );
        assert_eq!(result["tool_choice"]["function"]["name"], "apply_patch");
        assert_eq!(
            result["messages"][0]["tool_calls"][0]["function"]["arguments"],
            r#"{"input":"*** Begin Patch\n*** End Patch"}"#
        );
    }

    #[test]
    fn responses_request_to_chat_preserves_custom_tool_metadata_in_description() {
        let input = json!({
            "model": "gpt-5.4",
            "tools": [{
                "type": "custom",
                "name": "apply_patch",
                "description": "Use the `apply_patch` tool to edit files. This is a FREEFORM tool, so do not wrap the patch in JSON.",
                "format": {
                    "type": "grammar",
                    "syntax": "lark",
                    "definition": "start: begin_patch hunk+ end_patch"
                }
            }]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let description = result["tools"][0]["function"]["description"]
            .as_str()
            .unwrap();

        assert!(description.starts_with("Original tool definition:"));
        assert!(!description.contains("Original Codex tool definition"));
        assert!(description.contains("\"type\":\"custom\""));
        assert!(description.contains("\"format\":"));
        assert!(description.contains("\"syntax\":\"lark\""));
    }

    #[test]
    fn responses_request_to_chat_uses_provider_reasoning_effort_for_deepseek_model() {
        let input = json!({
            "model": "deepseek-v4-pro",
            "input": "hello",
            "reasoning": {"effort": "xhigh"}
        });
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("deepseek".to_string()),
            output_format: Some("reasoning_content".to_string()),
        };

        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["thinking"]["type"], "enabled");
        assert_eq!(result["reasoning_effort"], "max");
    }

    #[test]
    fn responses_request_to_chat_maps_openrouter_to_native_reasoning_object() {
        // OpenRouter 平台形态：原生 reasoning:{effort} 对象 + "openrouter" 值映射
        // （与 infer_aggregator_platform_config 推断出的配置保持一致）。
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning.effort".to_string()),
            effort_value_mode: Some("openrouter".to_string()),
            output_format: Some("auto".to_string()),
        };

        // max 不在 OpenRouter 枚举内（见 openclaw#77350），必须钳成 xhigh，
        // 且写进原生 reasoning 对象，而非顶层 reasoning_effort 别名。
        let input = json!({
            "model": "deepseek/deepseek-chat-v3.1",
            "input": "hello",
            "reasoning": {"effort": "max"}
        });
        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["reasoning"]["effort"], "xhigh");
        assert!(result.get("reasoning_effort").is_none());
        // thinking_param=none：即使 supports_effort 把 supports_thinking 带成 true，
        // 也不写任何 thinking 字段（OpenRouter 不认 thinking:{type}）。
        assert!(result.get("thinking").is_none());

        // 合法档位原样透传。
        let input_high = json!({
            "model": "deepseek/deepseek-chat-v3.1",
            "input": "hello",
            "reasoning": {"effort": "high"}
        });
        let result_high =
            responses_to_chat_completions_with_reasoning(input_high, Some(&config)).unwrap();
        assert_eq!(result_high["reasoning"]["effort"], "high");
        assert!(result_high.get("reasoning_effort").is_none());
    }

    #[test]
    fn responses_request_to_chat_passes_explicit_none_through_for_openrouter() {
        // OpenRouter 原生 reasoning 对象支持显式关闭：effort=none 应忠实转发为
        // {"reasoning":{"effort":"none"}}，而非被吞掉——否则默认开思考的模型无法关闭，
        // 带来行为与成本偏差。
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning.effort".to_string()),
            effort_value_mode: Some("openrouter".to_string()),
            output_format: Some("auto".to_string()),
        };

        let input = json!({
            "model": "openai/gpt-5",
            "input": "hello",
            "reasoning": {"effort": "none"}
        });
        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["reasoning"]["effort"], "none");
        // none 不是 OpenAI 顶层 reasoning_effort 的合法枚举，不写顶层别名；也不写 thinking。
        assert!(result.get("reasoning_effort").is_none());
        assert!(result.get("thinking").is_none());
    }

    #[test]
    fn responses_request_to_chat_drops_explicit_none_for_top_level_effort_provider() {
        // 对照：顶层 reasoning_effort 平台（DeepSeek/OpenAI 风格）的 effort 枚举不含 none，
        // 显式 none 不应透传成 reasoning_effort:"none"（会被上游拒），仅走 thinking 关闭路径。
        // 锁定「none 透传仅限 reasoning.effort 形态」的边界，防止回归。
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("deepseek".to_string()),
            output_format: Some("reasoning_content".to_string()),
        };

        let input = json!({
            "model": "deepseek-v4-pro",
            "input": "hello",
            "reasoning": {"effort": "none"}
        });
        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        // thinking 关闭信号照发；但不写 reasoning_effort，也不写原生 reasoning 对象。
        assert_eq!(result["thinking"]["type"], "disabled");
        assert!(result.get("reasoning_effort").is_none());
        assert!(result.get("reasoning").is_none());
    }

    #[test]
    fn responses_request_to_chat_maps_thinking_only_provider_without_effort() {
        let input = json!({
            "model": "kimi-k2.6",
            "input": "hello",
            "reasoning": {"effort": "high"}
        });
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
        };

        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["thinking"]["type"], "enabled");
        assert!(result.get("reasoning_effort").is_none());
    }

    #[test]
    fn responses_request_to_chat_maps_enable_thinking_provider() {
        let input = json!({
            "model": "qwen3-max",
            "input": "hello",
            "reasoning": {"effort": "medium"}
        });
        let config = CodexChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("enable_thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
        };

        let result = responses_to_chat_completions_with_reasoning(input, Some(&config)).unwrap();

        assert_eq!(result["enable_thinking"], true);
        assert!(result.get("reasoning_effort").is_none());
    }

    #[test]
    fn chat_response_to_responses_extracts_reasoning_details() {
        let input = json!({
            "id": "chatcmpl_minimax",
            "object": "chat.completion",
            "created": 123,
            "model": "MiniMax-M2.7",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_details": [
                        {"type": "reasoning_text", "text": "Need to inspect the code."}
                    ],
                    "content": "Done"
                },
                "finish_reason": "stop"
            }]
        });

        let result = chat_completion_to_response(input).unwrap();

        assert_eq!(result["output"][0]["type"], "reasoning");
        assert_eq!(
            result["output"][0]["summary"][0]["text"],
            "Need to inspect the code."
        );
        assert_eq!(result["output"][1]["content"][0]["text"], "Done");
    }

    #[test]
    fn responses_request_to_chat_normalizes_codex_internal_roles() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "message",
                    "role": "developer",
                    "content": [
                        {"type": "input_text", "text": "Follow project instructions."}
                    ]
                },
                {
                    "type": "message",
                    "role": "latest_reminder",
                    "content": "Keep the reply brief."
                },
                {
                    "type": "message",
                    "role": "unknown_codex_role",
                    "content": "Fallback content."
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "Follow project instructions.");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Keep the reply brief.");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"], "Fallback content.");
    }

    #[test]
    fn responses_request_to_chat_merges_mid_stream_system_into_head() {
        let input = json!({
            "model": "MiniMax-M2.7",
            "instructions": "You are Codex.",
            "input": [
                {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Permissions block"}]},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "AGENTS.md"}]},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "你好"}]},
                {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Collaboration Mode: Default"}]},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "你好"}]},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "你好"}]}
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        for (idx, msg) in messages.iter().enumerate() {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap();
            if idx == 0 {
                assert_eq!(role, "system", "first message must be system");
            } else {
                assert_ne!(
                    role, "system",
                    "no system role allowed past index 0 (got at {idx})"
                );
            }
        }

        let head_content = messages[0]["content"].as_str().unwrap();
        assert!(head_content.contains("You are Codex."));
        assert!(head_content.contains("Permissions block"));
        assert!(head_content.contains("Collaboration Mode: Default"));
    }

    #[test]
    fn collapse_system_messages_preserves_non_system_order() {
        let input = vec![
            json!({"role": "system", "content": "S1"}),
            json!({"role": "user", "content": "U1"}),
            json!({"role": "assistant", "content": "A1"}),
            json!({"role": "system", "content": "S2"}),
            json!({"role": "user", "content": "U2"}),
        ];
        let out = collapse_system_messages_to_head(input);

        assert_eq!(out.len(), 4);
        assert_eq!(out[0]["role"], "system");
        assert_eq!(out[0]["content"], "S1\n\nS2");
        assert_eq!(out[1]["content"], "U1");
        assert_eq!(out[2]["content"], "A1");
        assert_eq!(out[3]["content"], "U2");
    }

    #[test]
    fn responses_request_to_chat_passes_reasoning_content_back_to_assistant_message() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "reasoning",
                    "summary": [
                        {"type": "summary_text", "text": "Need to inspect the repo."}
                    ]
                },
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "I will check the files."}
                    ]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": "Continue"
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"], "I will check the files.");
        assert_eq!(
            messages[0]["reasoning_content"],
            "Need to inspect the repo."
        );
        assert_eq!(messages[1]["role"], "user");
        assert!(messages[1].get("reasoning_content").is_none());
    }

    #[test]
    fn responses_request_to_chat_attaches_trailing_reasoning_to_previous_assistant() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "message",
                    "role": "assistant",
                    "content": "I checked the files."
                },
                {
                    "type": "reasoning",
                    "summary": [
                        {"type": "summary_text", "text": "The answer came from README."}
                    ]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": "Continue"
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"], "I checked the files.");
        assert_eq!(
            messages[0]["reasoning_content"],
            "The answer came from README."
        );
        assert_eq!(messages[1]["role"], "user");
        assert!(messages[1].get("reasoning_content").is_none());
    }

    #[test]
    fn responses_request_to_chat_keeps_embedded_assistant_reasoning() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "message",
                    "role": "assistant",
                    "reasoning_content": "I need to preserve thinking history.",
                    "content": "Done."
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"], "Done.");
        assert_eq!(
            messages[0]["reasoning_content"],
            "I need to preserve thinking history."
        );
    }

    #[test]
    fn responses_request_to_chat_attaches_reasoning_to_tool_call_message() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "reasoning",
                    "summary": "Need to read a file."
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"README.md\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "Readme content"
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["reasoning_content"], "Need to read a file.");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[1]["role"], "tool");
    }

    #[test]
    fn responses_request_to_chat_recovers_reasoning_from_function_call_item() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"README.md\"}",
                    "reasoning_content": "Need to read a file."
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "Readme content"
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[0]["reasoning_content"], "Need to read a file.");
        assert_eq!(messages[1]["role"], "tool");
    }

    #[test]
    fn responses_request_to_chat_injects_placeholder_reasoning_for_bare_tool_call() {
        // 历史恢复 miss 时，带 tool_calls 的 assistant 消息没有任何可用 reasoning，
        // 必须补占位，否则 kimi/Moonshot thinking 模型会拒绝整个请求。
        let input = json!({
            "model": "kimi-k2-thinking",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"README.md\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "Readme content"
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[0]["reasoning_content"], "tool call");
        assert_eq!(messages[1]["role"], "tool");
    }

    #[test]
    fn responses_request_to_chat_attaches_trailing_reasoning_to_tool_call_message() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"README.md\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "Readme content"
                },
                {
                    "type": "reasoning",
                    "summary": "Need to read a file."
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[0]["reasoning_content"], "Need to read a file.");
        assert_eq!(messages[1]["role"], "tool");
    }

    #[test]
    fn responses_request_to_chat_keeps_multiple_tool_calls_adjacent_to_outputs() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"README.md\"}"
                },
                {
                    "type": "function_call",
                    "call_id": "call_2",
                    "name": "list_files",
                    "arguments": "{\"path\":\"src\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "Readme content"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_2",
                    "output": ["main.rs", "lib.rs"]
                },
                {
                    "role": "user",
                    "content": "Continue"
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[0]["tool_calls"][1]["id"], "call_2");
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["tool_call_id"], "call_1");
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call_2");
        assert_eq!(messages[2]["content"], "[\"main.rs\",\"lib.rs\"]");
        assert_eq!(messages[3]["role"], "user");
    }

    #[test]
    fn responses_request_to_chat_canonicalizes_json_string_tool_payloads() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "lookup",
                    "arguments": "{ \"b\": 2, \"a\": 1 }"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "{ \"z\": true, \"a\": [2, 1] }"
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(
            messages[0]["tool_calls"][0]["function"]["arguments"],
            r#"{"a":1,"b":2}"#
        );
        assert_eq!(messages[1]["content"], r#"{"a":[2,1],"z":true}"#);
    }

    #[test]
    fn responses_request_to_chat_preserves_plain_text_tool_output() {
        let input = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "not json"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "plain text result"
                }
            ]
        });

        let result = responses_to_chat_completions(input).unwrap();
        let messages = result["messages"].as_array().unwrap();

        assert_eq!(
            messages[0]["tool_calls"][0]["function"]["arguments"],
            "not json"
        );
        assert_eq!(messages[1]["content"], "plain text result");
    }

    #[test]
    fn chat_response_to_responses_maps_text_tool_calls_and_usage() {
        let input = json!({
            "id": "chatcmpl_1",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-5.4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "I should check the weather before answering.",
                    "content": "Let me check.",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"city\":\"Tokyo\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15,
                "prompt_tokens_details": {"cached_tokens": 3}
            }
        });

        let result = chat_completion_to_response(input).unwrap();

        assert_eq!(result["id"], "resp_chatcmpl_1");
        assert_eq!(result["status"], "completed");
        assert_eq!(result["output"][0]["type"], "reasoning");
        assert_eq!(
            result["output"][0]["summary"][0]["text"],
            "I should check the weather before answering."
        );
        assert_eq!(result["output"][1]["type"], "message");
        assert_eq!(result["output"][1]["content"][0]["text"], "Let me check.");
        assert_eq!(result["output"][2]["type"], "function_call");
        assert_eq!(result["output"][2]["call_id"], "call_1");
        assert_eq!(
            result["output"][2]["reasoning_content"],
            "I should check the weather before answering."
        );
        assert_eq!(result["usage"]["input_tokens"], 10);
        assert_eq!(result["usage"]["output_tokens"], 5);
        assert_eq!(result["usage"]["input_tokens_details"]["cached_tokens"], 3);
    }

    #[test]
    fn chat_response_to_responses_restores_loaded_namespace_tool_call() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{"type": "tool_search"}],
            "input": [{
                "type": "tool_search_output",
                "call_id": "call_tool_search_1",
                "status": "completed",
                "execution": "client",
                "tools": [{
                    "type": "namespace",
                    "name": "mcp__codex_apps__gmail",
                    "description": "Find and reference emails from your inbox.",
                    "tools": [{
                        "type": "function",
                        "name": "_search_emails",
                        "description": "Search Gmail for emails matching a query.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "query": {"type": "string"},
                                "label_ids": {"type": "array", "items": {"type": "string"}},
                                "max_results": {"type": "integer"}
                            }
                        }
                    }]
                }]
            }]
        });
        let context = build_codex_tool_context_from_request(&request);
        let chat = json!({
            "id": "chatcmpl_gmail",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-5.4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_gmail",
                        "type": "function",
                        "function": {
                            "name": "mcp__codex_apps__gmail___search_emails",
                            "arguments": "{\"query\":\"-in:spam -in:trash\",\"label_ids\":[\"UNREAD\"],\"max_results\":5}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let result = chat_completion_to_response_with_context(chat, &context).unwrap();

        assert_eq!(result["output"][0]["type"], "function_call");
        assert_eq!(result["output"][0]["call_id"], "call_gmail");
        assert_eq!(result["output"][0]["namespace"], "mcp__codex_apps__gmail");
        assert_eq!(result["output"][0]["name"], "_search_emails");
        assert_eq!(
            result["output"][0]["arguments"],
            r#"{"label_ids":["UNREAD"],"max_results":5,"query":"-in:spam -in:trash"}"#
        );
    }

    #[test]
    fn chat_response_to_responses_restores_tool_search_call() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{"type": "tool_search"}],
            "input": "Find tools."
        });
        let context = build_codex_tool_context_from_request(&request);
        let chat = json!({
            "id": "chatcmpl_tool_search",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-5.4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_tool_search_1",
                        "type": "function",
                        "function": {
                            "name": "tool_search",
                            "arguments": "{\"query\":\"Gmail search emails\",\"limit\":10}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let result = chat_completion_to_response_with_context(chat, &context).unwrap();

        assert_eq!(result["output"][0]["type"], "tool_search_call");
        assert_eq!(result["output"][0]["call_id"], "call_tool_search_1");
        assert_eq!(result["output"][0]["execution"], "client");
        assert_eq!(
            result["output"][0]["arguments"]["query"],
            "Gmail search emails"
        );
        assert_eq!(result["output"][0]["arguments"]["limit"], 10);
    }

    #[test]
    fn chat_response_to_responses_restores_custom_tool_call() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{"type": "custom", "name": "apply_patch"}],
            "input": "Patch it."
        });
        let context = build_codex_tool_context_from_request(&request);
        let chat = json!({
            "id": "chatcmpl_custom",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-5.4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_patch",
                        "type": "function",
                        "function": {
                            "name": "apply_patch",
                            "arguments": "{\"input\":\"*** Begin Patch\\n*** End Patch\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let result = chat_completion_to_response_with_context(chat, &context).unwrap();

        assert_eq!(result["output"][0]["type"], "custom_tool_call");
        assert_eq!(result["output"][0]["id"], "ctc_call_patch");
        assert_eq!(result["output"][0]["call_id"], "call_patch");
        assert_eq!(result["output"][0]["name"], "apply_patch");
        assert_eq!(
            result["output"][0]["input"],
            "*** Begin Patch\n*** End Patch"
        );
    }

    #[test]
    fn chat_response_to_responses_canonicalizes_json_string_tool_arguments() {
        let input = json!({
            "id": "chatcmpl_args",
            "object": "chat.completion",
            "created": 123,
            "model": "gpt-5.4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "lookup",
                            "arguments": "{ \"b\": 2, \"a\": 1 }"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let result = chat_completion_to_response(input).unwrap();

        assert_eq!(result["output"][0]["type"], "function_call");
        assert_eq!(result["output"][0]["arguments"], r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn chat_response_to_responses_splits_inline_think_content() {
        let input = json!({
            "id": "chatcmpl_think",
            "object": "chat.completion",
            "created": 123,
            "model": "MiniMax-M2.7",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "<think>\nI should answer with pong.\n</think>\n\npong"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30,
                "completion_tokens_details": {"reasoning_tokens": 18}
            }
        });

        let result = chat_completion_to_response(input).unwrap();

        assert_eq!(result["output"][0]["type"], "reasoning");
        assert_eq!(
            result["output"][0]["summary"][0]["text"],
            "I should answer with pong."
        );
        assert_eq!(result["output"][1]["type"], "message");
        assert_eq!(result["output"][1]["content"][0]["text"], "pong");
        assert_eq!(
            result["usage"]["output_tokens_details"]["reasoning_tokens"],
            18
        );
    }

    #[test]
    fn chat_response_length_maps_to_incomplete_response() {
        let input = json!({
            "id": "chatcmpl_2",
            "model": "gpt-5.4",
            "choices": [{
                "message": {"role": "assistant", "content": "partial"},
                "finish_reason": "length"
            }]
        });

        let result = chat_completion_to_response(input).unwrap();

        assert_eq!(result["status"], "incomplete");
        assert_eq!(result["incomplete_details"]["reason"], "max_output_tokens");
    }

    // Regression tests for tool_choice without tools guard
    // https://github.com/farion1231/cc-switch/issues/3557

    #[test]
    fn responses_request_to_chat_drops_tool_choice_when_no_tools() {
        // When tools is absent from the request, tool_choice must be dropped
        // to avoid 503/400 from strict OpenAI-compatible upstreams.
        let input = json!({
            "model": "qwen3-7-max",
            "tool_choice": "auto",
            "input": "hi"
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(
            result.get("tool_choice").is_none(),
            "tool_choice should be dropped when tools is absent"
        );
        assert!(result.get("tools").is_none(), "tools should be absent");
        assert_eq!(result["model"], "qwen3-7-max");
    }

    #[test]
    fn responses_request_to_chat_drops_tool_choice_when_tools_empty_array() {
        // When tools is an empty array, tool_choice must be dropped.
        let input = json!({
            "model": "gpt-5.4",
            "tools": [],
            "tool_choice": "auto",
            "input": "hi"
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(
            result.get("tool_choice").is_none(),
            "tool_choice should be dropped when tools is empty"
        );
        assert!(
            result.get("tools").is_none(),
            "tools should be absent when input tools was empty"
        );
    }

    #[test]
    fn responses_request_to_chat_drops_parallel_tool_calls_when_no_tools() {
        // parallel_tool_calls must also be dropped when tools is absent,
        // as it is part of EXTRA_CHAT_PASSTHROUGH_FIELDS.
        let input = json!({
            "model": "gpt-5.4",
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "input": "hi"
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(
            result.get("tool_choice").is_none(),
            "tool_choice should be dropped"
        );
        assert!(
            result.get("parallel_tool_calls").is_none(),
            "parallel_tool_calls should be dropped"
        );
        assert!(result.get("tools").is_none(), "tools should be absent");
    }

    #[test]
    fn responses_request_to_chat_drops_tool_choice_when_all_tools_filtered() {
        // When all tools are filtered out (e.g., missing name), tool_choice must be dropped.
        let input = json!({
            "model": "gpt-5.4",
            "tools": [
                {"type": "function"}
            ],
            "tool_choice": "auto",
            "input": "hi"
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(
            result.get("tool_choice").is_none(),
            "tool_choice should be dropped when all tools filtered"
        );
        assert!(
            result.get("tools").is_none(),
            "tools should be absent when all filtered"
        );
    }

    #[test]
    fn responses_request_to_chat_keeps_tool_choice_when_tools_present() {
        // When tools is present and non-empty, tool_choice must be preserved.
        let input = json!({
            "model": "gpt-5.4",
            "tools": [{
                "type": "function",
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {"type": "object"}
            }],
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "input": "hi"
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(
            result.get("tool_choice").is_some(),
            "tool_choice should be kept when tools present"
        );
        assert_eq!(result["tool_choice"], "auto");
        assert!(
            result.get("parallel_tool_calls").is_some(),
            "parallel_tool_calls should be kept"
        );
        assert_eq!(result["parallel_tool_calls"], true);
        assert!(
            result
                .get("tools")
                .is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty())),
            "tools should be present"
        );
        assert_eq!(result["tools"][0]["function"]["name"], "get_weather");
    }

    #[test]
    fn responses_request_to_chat_keeps_tool_choice_function_when_tools_present() {
        // When tools is present, function-type tool_choice must be preserved and mapped.
        let input = json!({
            "model": "gpt-5.4",
            "tools": [{
                "type": "function",
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {"type": "object"}
            }],
            "tool_choice": {"type": "function", "name": "get_weather"},
            "input": "hi"
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(
            result.get("tool_choice").is_some(),
            "tool_choice should be kept"
        );
        assert_eq!(result["tool_choice"]["type"], "function");
        assert_eq!(result["tool_choice"]["function"]["name"], "get_weather");
    }

    #[test]
    fn responses_request_to_chat_no_tool_choice_no_tools_stays_clean() {
        // When neither tool_choice nor tools are present, the output should be clean.
        let input = json!({
            "model": "gpt-5.4",
            "input": "hi"
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(
            result.get("tool_choice").is_none(),
            "tool_choice should be absent"
        );
        assert!(result.get("tools").is_none(), "tools should be absent");
        assert!(
            result.get("parallel_tool_calls").is_none(),
            "parallel_tool_calls should be absent"
        );
    }

    #[test]
    fn responses_request_to_chat_tool_choice_none_dropped_when_no_tools() {
        // Even tool_choice: "none" should be dropped when tools is absent,
        // because strict upstreams reject the combination regardless of value.
        let input = json!({
            "model": "gpt-5.4",
            "tool_choice": "none",
            "input": "hi"
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(
            result.get("tool_choice").is_none(),
            "tool_choice 'none' should be dropped when no tools"
        );
    }

    #[test]
    fn responses_request_to_chat_tool_search_output_provides_tools_keeps_tool_choice() {
        // When tool_search_output in input provides tools, tool_choice should be kept.
        let input = json!({
            "model": "gpt-5.4",
            "tool_choice": "auto",
            "input": [{
                "type": "tool_search_output",
                "call_id": "call_ts_1",
                "status": "completed",
                "execution": "client",
                "tools": [{
                    "type": "function",
                    "name": "search_docs",
                    "description": "Search documentation.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"}
                        }
                    }
                }]
            }]
        });

        let result = responses_to_chat_completions(input).unwrap();

        assert!(
            result.get("tool_choice").is_some(),
            "tool_choice should be kept when tool_search_output provides tools"
        );
        assert_eq!(result["tool_choice"], "auto");
        assert!(
            result
                .get("tools")
                .is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty())),
            "tools should be present from tool_search_output"
        );
        assert_eq!(result["tools"][0]["function"]["name"], "search_docs");
    }
}
