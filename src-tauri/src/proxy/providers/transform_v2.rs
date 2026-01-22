//! 格式转换模块 V2
//!
//! 使用统一消息格式实现提供商之间的转换
//! 这是基于 Rig 设计理念的新实现，与现有 transform.rs 并存
//!
//! ## 设计原则
//!
//! 1. **透传优先**：默认不启用转换，保持零开销
//! 2. **统一中间格式**：通过 `Message` 类型实现跨格式转换
//! 3. **类型安全**：使用 Rust 类型系统保证转换正确性
//! 4. **向后兼容**：与 transform.rs 并存，可选使用
//! 5. **职责单一**：只负责格式转换，模型映射由 model_mapper.rs 统一处理
//!
//! ## 重要说明
//!
//! **模型映射已移至 `model_mapper.rs`**
//!
//! 为了避免重复映射和逻辑冲突，本模块不再执行模型映射。
//! 模型映射在 `forwarder.rs` 中统一调用 `model_mapper::apply_model_mapping()`。
//! 本模块接收到的请求体中的模型名称已经是映射后的结果。
//!
//! ## 支持的转换路径
//!
//! - Anthropic → OpenAI (请求)
//! - OpenAI → Anthropic (响应)
//! - Anthropic → Gemini (请求)
//! - Gemini → OpenAI (请求) [计划中]

use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy::unified::{
    message::*,
    models::{anthropic::*, gemini::*, openai::*},
};
use serde_json::{json, Value};

/// 统一格式转换器 V2
pub struct UnifiedConverter;

impl UnifiedConverter {
    /// Anthropic 请求 → OpenAI 请求（使用统一格式）
    ///
    /// 注意：模型映射已在 forwarder.rs 中通过 model_mapper.rs 完成，
    /// 此处接收的 body 中的 model 字段已经是映射后的结果。
    pub fn anthropic_to_openai(body: Value, _provider: &Provider) -> Result<Value, ProxyError> {
        // 1. 解析 Anthropic 消息
        let messages = Self::parse_anthropic_messages(&body)?;

        // 2. 转换为统一格式
        let unified: Vec<Message> = messages
            .iter()
            .map(|m| Self::anthropic_to_unified(m))
            .collect::<Result<_, _>>()?;

        // 3. 处理 system prompt
        let mut all_messages = Vec::new();
        if let Some(system) = body.get("system") {
            if let Some(text) = system.as_str() {
                all_messages.push(Message::System {
                    content: text.to_string(),
                });
            } else if let Some(arr) = system.as_array() {
                for msg in arr {
                    if let Some(text) = msg.get("text").and_then(|t| t.as_str()) {
                        all_messages.push(Message::System {
                            content: text.to_string(),
                        });
                    }
                }
            }
        }
        all_messages.extend(unified);

        // 4. 转换为 OpenAI 格式
        let openai_messages: Vec<OpenAIMessage> = all_messages
            .iter()
            .flat_map(|m| {
                let msgs: Vec<OpenAIMessage> = m.try_into().unwrap_or_default();
                msgs
            })
            .collect();

        // 5. 构建 OpenAI 请求（克隆 body 避免借用冲突）
        Self::build_openai_request(body.clone(), openai_messages)
    }

    /// OpenAI 响应 → Anthropic 响应（使用统一格式）
    pub fn openai_to_anthropic(body: Value) -> Result<Value, ProxyError> {
        // 1. 解析 OpenAI 响应
        let choice = body
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .ok_or_else(|| ProxyError::TransformError("No choices in response".to_string()))?;

        let message = choice
            .get("message")
            .ok_or_else(|| ProxyError::TransformError("No message in choice".to_string()))?;

        // 2. 转换为统一格式
        let unified = Self::openai_to_unified(message)?;

        // 3. 转换为 Anthropic 格式
        let anthropic: AnthropicMessage = (&unified).try_into()?;

        // 4. 构建响应（克隆 body 和 choice 避免借用冲突）
        Self::build_anthropic_response(body.clone(), anthropic, choice.clone())
    }

    /// Anthropic 请求 → Gemini 请求
    #[allow(dead_code)] // 为未来扩展预留
    pub fn anthropic_to_gemini(body: &Value, _provider: &Provider) -> Result<Value, ProxyError> {
        // 1. 解析 Anthropic 消息
        let messages = Self::parse_anthropic_messages(body)?;

        // 2. 转换为统一格式
        let unified: Vec<Message> = messages
            .iter()
            .map(|m| Self::anthropic_to_unified(m))
            .collect::<Result<_, _>>()?;

        // 3. 转换为 Gemini 格式
        let gemini_contents: Vec<GeminiContent> = unified
            .iter()
            .filter_map(|m| {
                if matches!(m, Message::System { .. }) {
                    None // Gemini 的 system 指令单独处理
                } else {
                    Some(m.try_into().ok())
                }
            })
            .flatten()
            .collect();

        // 4. 构建 Gemini 请求
        let mut result = json!({
            "contents": gemini_contents,
        });

        // 处理 system instruction
        if let Some(system) = body.get("system") {
            if let Some(text) = system.as_str() {
                result["systemInstruction"] = json!({
                    "parts": [{"text": text}]
                });
            }
        }

        // 复制其他参数
        if let Some(v) = body.get("max_tokens") {
            result["generationConfig"] = json!({
                "maxOutputTokens": v
            });
        }

        Ok(result)
    }

    // === 辅助方法 ===

    fn parse_anthropic_messages(body: &Value) -> Result<Vec<AnthropicMessage>, ProxyError> {
        let messages = body
            .get("messages")
            .and_then(|m| m.as_array())
            .ok_or_else(|| ProxyError::TransformError("No messages in request".to_string()))?;

        messages
            .iter()
            .map(|msg| {
                serde_json::from_value(msg.clone()).map_err(|e| {
                    ProxyError::TransformError(format!("Failed to parse Anthropic message: {}", e))
                })
            })
            .collect()
    }

    fn anthropic_to_unified(msg: &AnthropicMessage) -> Result<Message, ProxyError> {
        let content: Vec<_> = msg
            .content
            .iter()
            .filter_map(|c| match c {
                AnthropicContent::Text { text, .. } => Some(Ok(match msg.role {
                    AnthropicRole::User => UserContent::Text {
                        text: text.clone(),
                        cache_control: None,
                    },
                    AnthropicRole::Assistant => {
                        return Some(Err(ProxyError::TransformError(
                            "Text content should be handled separately".to_string(),
                        )))
                    }
                })),
                AnthropicContent::Image { source } => Some(Ok(UserContent::Image {
                    data: ImageData {
                        source: if source.source_type == "base64" {
                            DataSource::Base64 {
                                data: source.data.clone(),
                            }
                        } else {
                            DataSource::Url {
                                url: source.data.clone(),
                            }
                        },
                        media_type: Some(source.media_type.clone()),
                        detail: None,
                    },
                })),
                AnthropicContent::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    let text = if let Some(s) = content.as_str() {
                        s.to_string()
                    } else {
                        serde_json::to_string(content).unwrap_or_default()
                    };
                    Some(Ok(UserContent::ToolResult {
                        id: tool_use_id.clone(),
                        content: vec![ToolResultContent::Text { text }],
                        is_error: None,
                    }))
                }
                _ => None,
            })
            .collect::<Result<Vec<_>, _>>()?;

        // 处理 assistant 内容
        let assistant_content: Vec<AssistantContent> = msg
            .content
            .iter()
            .filter_map(|c| match c {
                AnthropicContent::Text { text, .. } => Some(AssistantContent::Text {
                    text: text.clone(),
                    cache_control: None,
                }),
                AnthropicContent::ToolUse { id, name, input } => {
                    Some(AssistantContent::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: input.clone(),
                        signature: None,
                    })
                }
                AnthropicContent::Thinking { thinking, signature } => {
                    Some(AssistantContent::Reasoning {
                        thinking: thinking.clone(),
                        signature: signature.clone(),
                    })
                }
                _ => None,
            })
            .collect();

        match msg.role {
            AnthropicRole::User => Ok(Message::User {
                content: content
                    .into_iter()
                    .filter_map(|c| {
                        if let UserContent::Text { .. }
                        | UserContent::Image { .. }
                        | UserContent::ToolResult { .. } = c
                        {
                            Some(c)
                        } else {
                            None
                        }
                    })
                    .collect(),
            }),
            AnthropicRole::Assistant => Ok(Message::Assistant {
                id: None,
                content: assistant_content,
            }),
        }
    }

    fn openai_to_unified(message: &Value) -> Result<Message, ProxyError> {
        let role = message
            .get("role")
            .and_then(|r| r.as_str())
            .ok_or_else(|| ProxyError::TransformError("No role in message".to_string()))?;

        match role {
            "assistant" => {
                let mut content = Vec::new();

                // 文本内容
                if let Some(text) = message.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        content.push(AssistantContent::Text {
                            text: text.to_string(),
                            cache_control: None,
                        });
                    }
                }

                // 工具调用
                if let Some(tool_calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tool_calls {
                        let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let func = tc.get("function");
                        let name = func
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        let args_str = func
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");
                        let arguments: Value =
                            serde_json::from_str(args_str).unwrap_or(json!({}));

                        content.push(AssistantContent::ToolCall {
                            id: id.to_string(),
                            name: name.to_string(),
                            arguments,
                            signature: None,
                        });
                    }
                }

                Ok(Message::Assistant { id: None, content })
            }
            _ => Err(ProxyError::TransformError(format!(
                "Unsupported role: {}",
                role
            ))),
        }
    }

    fn build_openai_request(
        body: Value,
        messages: Vec<OpenAIMessage>,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({
            "messages": messages,
        });

        // 模型名称直接使用（已在 model_mapper.rs 中完成映射）
        if let Some(model) = body.get("model") {
            result["model"] = model.clone();
        }

        // 复制参数
        for key in &[
            "max_tokens",
            "temperature",
            "top_p",
            "stream",
            "tool_choice",
        ] {
            if let Some(v) = body.get(key) {
                result[key] = v.clone();
            }
        }

        // 转换 stop_sequences → stop
        if let Some(v) = body.get("stop_sequences") {
            result["stop"] = v.clone();
        }

        // 转换 tools
        if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
            let openai_tools: Vec<Value> = tools
                .iter()
                .filter(|t| t.get("type").and_then(|v| v.as_str()) != Some("BatchTool"))
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.get("name"),
                            "description": t.get("description"),
                            "parameters": super::transform::clean_schema(
                                t.get("input_schema").cloned().unwrap_or(json!({}))
                            )
                        }
                    })
                })
                .collect();

            if !openai_tools.is_empty() {
                result["tools"] = json!(openai_tools);
            }
        }

        Ok(result)
    }

    fn build_anthropic_response(
        body: Value,
        anthropic: AnthropicMessage,
        choice: Value,
    ) -> Result<Value, ProxyError> {
        let stop_reason = choice
            .get("finish_reason")
            .and_then(|r| r.as_str())
            .map(|r| match r {
                "stop" => "end_turn",
                "length" => "max_tokens",
                "tool_calls" => "tool_use",
                other => other,
            });

        let usage = body.get("usage").cloned().unwrap_or(json!({}));
        let input_tokens = usage
            .get("prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let output_tokens = usage
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok(json!({
            "id": body.get("id").and_then(|i| i.as_str()).unwrap_or(""),
            "type": "message",
            "role": "assistant",
            "content": anthropic.content,
            "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(""),
            "stop_reason": stop_reason,
            "stop_sequence": null,
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": output_tokens
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_provider() -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Provider".to_string(),
            settings_config: json!({}),
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
    fn test_anthropic_to_openai_simple() {
        let provider = create_test_provider();
        // 注意：模型映射已在 model_mapper.rs 完成，这里传入的是映射后的模型
        let input = json!({
            "model": "gpt-4-turbo",  // 假设已经被映射
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Hello"}]}
            ]
        });

        let result = UnifiedConverter::anthropic_to_openai(input, &provider).unwrap();
        assert_eq!(result["model"], "gpt-4-turbo");
        assert_eq!(result["max_tokens"], 1024);
        assert!(result["messages"].is_array());
    }

    #[test]
    fn test_openai_to_anthropic_simple() {
        let input = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5
            }
        });

        let result = UnifiedConverter::openai_to_anthropic(input).unwrap();
        assert_eq!(result["type"], "message");
        assert_eq!(result["role"], "assistant");
        assert_eq!(result["stop_reason"], "end_turn");
    }

    #[test]
    fn test_anthropic_to_openai_with_system() {
        let provider = create_test_provider();
        let input = json!({
            "model": "claude-3-sonnet",
            "max_tokens": 1024,
            "system": "You are a helpful assistant.",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Hello"}]}
            ]
        });

        let result = UnifiedConverter::anthropic_to_openai(input, &provider).unwrap();
        assert!(result["messages"].is_array());
        // 第一条应该是 system 消息
        assert_eq!(result["messages"][0]["role"], "system");
        assert_eq!(result["messages"][0]["content"], "You are a helpful assistant.");
    }

    #[test]
    fn test_anthropic_to_openai_with_tools() {
        let provider = create_test_provider();
        let input = json!({
            "model": "claude-3-opus",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "What's the weather?"}]}
            ],
            "tools": [{
                "name": "get_weather",
                "description": "Get weather info",
                "input_schema": {"type": "object", "properties": {"location": {"type": "string"}}}
            }]
        });

        let result = UnifiedConverter::anthropic_to_openai(input, &provider).unwrap();
        assert!(result["tools"].is_array());
        assert_eq!(result["tools"][0]["type"], "function");
        assert_eq!(result["tools"][0]["function"]["name"], "get_weather");
    }

    #[test]
    fn test_openai_to_anthropic_with_tool_calls() {
        let input = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\": \"Tokyo\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5
            }
        });

        let result = UnifiedConverter::openai_to_anthropic(input).unwrap();
        assert_eq!(result["stop_reason"], "tool_use");
        assert!(result["content"].is_array());
    }

    #[test]
    fn test_anthropic_to_gemini_simple() {
        let provider = create_test_provider();
        let input = json!({
            "model": "claude-3-opus",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Hello"}]}
            ]
        });

        let result = UnifiedConverter::anthropic_to_gemini(&input, &provider).unwrap();
        assert!(result["contents"].is_array());
    }

    #[test]
    fn test_anthropic_to_gemini_with_system() {
        let provider = create_test_provider();
        let input = json!({
            "model": "claude-3-opus",
            "max_tokens": 1024,
            "system": "You are a helpful assistant.",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Hello"}]}
            ]
        });

        let result = UnifiedConverter::anthropic_to_gemini(&input, &provider).unwrap();
        assert!(result["systemInstruction"].is_object());
        assert_eq!(result["systemInstruction"]["parts"][0]["text"], "You are a helpful assistant.");
    }

    // 注意：模型映射测试已移至 model_mapper.rs
    // transform_v2.rs 不再负责模型映射，只负责格式转换
}
