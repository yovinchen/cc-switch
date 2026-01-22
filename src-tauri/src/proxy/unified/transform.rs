//! 流式响应转换器
//!
//! 提供 SSE 流式响应的双向转换能力，支持状态追踪和工具调用累积。
//!
//! ## 核心设计（借鉴 claude-code-hub）
//!
//! - `TransformState`: 跨 SSE 事件的状态管理
//! - `StreamTransformer`: 流式响应转换接口
//! - 支持 Anthropic ↔ OpenAI 双向流式转换

use crate::proxy::error::ProxyError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

// ================================================================
// Transform State
// ================================================================

/// 流式转换状态
///
/// 跟踪跨多个 SSE 事件的转换状态，用于正确累积和转换流式响应。
#[derive(Debug, Clone, Default)]
pub struct TransformState {
    /// 当前是否有工具调用
    pub has_tool_call: bool,
    /// 当前内容块索引
    pub current_index: usize,
    /// 当前块类型
    pub current_block_type: Option<BlockType>,
    /// 累积的工具调用（按索引）
    pub tool_calls: HashMap<usize, ToolCallState>,
    /// 累积的文本内容
    pub text_content: String,
    /// 累积的思考内容
    pub thinking_content: String,
    /// 完成原因
    pub finish_reason: Option<String>,
    /// 消息 ID
    pub message_id: Option<String>,
    /// 模型名称
    pub model: Option<String>,
    /// 使用统计
    pub usage: Option<UsageState>,
}

/// 内容块类型
#[derive(Debug, Clone, PartialEq)]
pub enum BlockType {
    Text,
    Thinking,
    ToolUse,
}

/// 工具调用状态
#[derive(Debug, Clone, Default)]
pub struct ToolCallState {
    /// 工具调用 ID
    pub id: String,
    /// 工具名称
    pub name: String,
    /// 累积的参数 JSON 字符串
    pub arguments: String,
    /// 是否完成
    pub completed: bool,
}

/// 使用统计状态
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageState {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
}

impl TransformState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 重置状态（用于新消息）
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// 开始新的内容块
    pub fn start_block(&mut self, block_type: BlockType, index: usize) {
        self.current_block_type = Some(block_type);
        self.current_index = index;
    }

    /// 结束当前内容块
    pub fn end_block(&mut self) {
        if let Some(BlockType::ToolUse) = &self.current_block_type {
            if let Some(tool_call) = self.tool_calls.get_mut(&self.current_index) {
                tool_call.completed = true;
            }
        }
        self.current_block_type = None;
    }

    /// 追加文本内容
    pub fn append_text(&mut self, text: &str) {
        match &self.current_block_type {
            Some(BlockType::Text) => self.text_content.push_str(text),
            Some(BlockType::Thinking) => self.thinking_content.push_str(text),
            _ => {}
        }
    }

    /// 初始化工具调用
    pub fn init_tool_call(&mut self, index: usize, id: &str, name: &str) {
        self.has_tool_call = true;
        self.tool_calls.insert(
            index,
            ToolCallState {
                id: id.to_string(),
                name: name.to_string(),
                arguments: String::new(),
                completed: false,
            },
        );
    }

    /// 追加工具调用参数
    pub fn append_tool_arguments(&mut self, index: usize, args: &str) {
        if let Some(tool_call) = self.tool_calls.get_mut(&index) {
            tool_call.arguments.push_str(args);
        }
    }
}

// ================================================================
// Anthropic → OpenAI Stream Transformer
// ================================================================

/// Anthropic SSE 事件转换为 OpenAI SSE 事件
pub fn transform_anthropic_sse_to_openai(
    event_type: &str,
    data: &Value,
    state: &mut TransformState,
) -> Result<Option<Value>, ProxyError> {
    match event_type {
        "message_start" => {
            // 提取消息 ID 和模型
            if let Some(message) = data.get("message") {
                state.message_id = message.get("id").and_then(|v| v.as_str()).map(String::from);
                state.model = message.get("model").and_then(|v| v.as_str()).map(String::from);

                // 提取 usage
                if let Some(usage) = message.get("usage") {
                    state.usage = Some(UsageState {
                        input_tokens: usage
                            .get("input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        output_tokens: 0,
                        cache_read_tokens: usage
                            .get("cache_read_input_tokens")
                            .and_then(|v| v.as_i64()),
                        cache_creation_tokens: usage
                            .get("cache_creation_input_tokens")
                            .and_then(|v| v.as_i64()),
                    });
                }
            }

            // 返回 OpenAI 格式的初始 chunk
            Ok(Some(json!({
                "id": state.message_id.clone().unwrap_or_default(),
                "object": "chat.completion.chunk",
                "created": chrono::Utc::now().timestamp(),
                "model": state.model.clone().unwrap_or_default(),
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "content": ""
                    },
                    "finish_reason": null
                }]
            })))
        }

        "content_block_start" => {
            let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let content_block = data.get("content_block");

            if let Some(block) = content_block {
                let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match block_type {
                    "text" => {
                        state.start_block(BlockType::Text, index);
                        Ok(None) // 文本块开始不需要发送事件
                    }
                    "thinking" => {
                        state.start_block(BlockType::Thinking, index);
                        Ok(None)
                    }
                    "tool_use" => {
                        state.start_block(BlockType::ToolUse, index);
                        let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");

                        state.init_tool_call(index, id, name);

                        // 返回工具调用开始的 OpenAI 格式
                        Ok(Some(json!({
                            "id": state.message_id.clone().unwrap_or_default(),
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "model": state.model.clone().unwrap_or_default(),
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "tool_calls": [{
                                        "index": index,
                                        "id": id,
                                        "type": "function",
                                        "function": {
                                            "name": name,
                                            "arguments": ""
                                        }
                                    }]
                                },
                                "finish_reason": null
                            }]
                        })))
                    }
                    _ => Ok(None),
                }
            } else {
                Ok(None)
            }
        }

        "content_block_delta" => {
            let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let delta = data.get("delta");

            if let Some(d) = delta {
                let delta_type = d.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match delta_type {
                    "text_delta" => {
                        let text = d.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        state.append_text(text);

                        Ok(Some(json!({
                            "id": state.message_id.clone().unwrap_or_default(),
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "model": state.model.clone().unwrap_or_default(),
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "content": text
                                },
                                "finish_reason": null
                            }]
                        })))
                    }
                    "thinking_delta" => {
                        let thinking = d.get("thinking").and_then(|v| v.as_str()).unwrap_or("");
                        state.append_text(thinking);
                        // OpenAI 不支持 thinking，可以作为特殊内容发送或忽略
                        Ok(None)
                    }
                    "input_json_delta" => {
                        let partial_json =
                            d.get("partial_json").and_then(|v| v.as_str()).unwrap_or("");
                        state.append_tool_arguments(index, partial_json);

                        Ok(Some(json!({
                            "id": state.message_id.clone().unwrap_or_default(),
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "model": state.model.clone().unwrap_or_default(),
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "tool_calls": [{
                                        "index": index,
                                        "function": {
                                            "arguments": partial_json
                                        }
                                    }]
                                },
                                "finish_reason": null
                            }]
                        })))
                    }
                    _ => Ok(None),
                }
            } else {
                Ok(None)
            }
        }

        "content_block_stop" => {
            state.end_block();
            Ok(None)
        }

        "message_delta" => {
            // 更新完成原因和 usage
            if let Some(delta) = data.get("delta") {
                if let Some(stop_reason) = delta.get("stop_reason").and_then(|v| v.as_str()) {
                    state.finish_reason = Some(map_stop_reason_to_finish_reason(stop_reason));
                }
            }

            if let Some(usage) = data.get("usage") {
                if let Some(ref mut u) = state.usage {
                    u.output_tokens = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                }
            }

            Ok(None)
        }

        "message_stop" => {
            // 发送最终的 finish_reason
            let finish_reason = state.finish_reason.clone().unwrap_or_else(|| "stop".to_string());

            Ok(Some(json!({
                "id": state.message_id.clone().unwrap_or_default(),
                "object": "chat.completion.chunk",
                "created": chrono::Utc::now().timestamp(),
                "model": state.model.clone().unwrap_or_default(),
                "choices": [{
                    "index": 0,
                    "delta": {},
                    "finish_reason": finish_reason
                }]
            })))
        }

        "ping" | "error" => Ok(None),

        _ => Ok(None),
    }
}

// ================================================================
// OpenAI → Anthropic Stream Transformer
// ================================================================

/// OpenAI SSE 事件转换为 Anthropic SSE 事件
pub fn transform_openai_sse_to_anthropic(
    data: &Value,
    state: &mut TransformState,
) -> Result<Vec<(String, Value)>, ProxyError> {
    let mut events = Vec::new();

    // 首次事件，发送 message_start
    if state.message_id.is_none() {
        let id = data
            .get("id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| format!("msg_{}", uuid::Uuid::new_v4()));

        let model = data
            .get("model")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_default();

        state.message_id = Some(id.clone());
        state.model = Some(model.clone());

        events.push((
            "message_start".to_string(),
            json!({
                "type": "message_start",
                "message": {
                    "id": id,
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 0
                    }
                }
            }),
        ));
    }

    // 处理 choices
    if let Some(choices) = data.get("choices").and_then(|v| v.as_array()) {
        for choice in choices {
            let delta = choice.get("delta");
            let finish_reason = choice.get("finish_reason").and_then(|v| v.as_str());

            if let Some(d) = delta {
                // 处理内容
                if let Some(content) = d.get("content").and_then(|v| v.as_str()) {
                    if !content.is_empty() {
                        // 如果是新的文本块，先发送 content_block_start
                        if state.current_block_type.is_none()
                            || state.current_block_type != Some(BlockType::Text)
                        {
                            let index = state.current_index;
                            state.start_block(BlockType::Text, index);

                            events.push((
                                "content_block_start".to_string(),
                                json!({
                                    "type": "content_block_start",
                                    "index": index,
                                    "content_block": {
                                        "type": "text",
                                        "text": ""
                                    }
                                }),
                            ));
                        }

                        // 发送文本 delta
                        events.push((
                            "content_block_delta".to_string(),
                            json!({
                                "type": "content_block_delta",
                                "index": state.current_index,
                                "delta": {
                                    "type": "text_delta",
                                    "text": content
                                }
                            }),
                        ));

                        state.append_text(content);
                    }
                }

                // 处理工具调用
                if let Some(tool_calls) = d.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tool_calls {
                        let tc_index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let tc_id = tc.get("id").and_then(|v| v.as_str());
                        let function = tc.get("function");

                        // 新的工具调用
                        if let (Some(id), Some(func)) = (tc_id, function) {
                            let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("");

                            // 先结束当前文本块（如果有）
                            if state.current_block_type == Some(BlockType::Text) {
                                events.push((
                                    "content_block_stop".to_string(),
                                    json!({
                                        "type": "content_block_stop",
                                        "index": state.current_index
                                    }),
                                ));
                                state.end_block();
                                state.current_index += 1;
                            }

                            // 开始工具使用块
                            let block_index = state.current_index + tc_index;
                            state.start_block(BlockType::ToolUse, block_index);
                            state.init_tool_call(block_index, id, name);

                            events.push((
                                "content_block_start".to_string(),
                                json!({
                                    "type": "content_block_start",
                                    "index": block_index,
                                    "content_block": {
                                        "type": "tool_use",
                                        "id": id,
                                        "name": name,
                                        "input": {}
                                    }
                                }),
                            ));
                        }

                        // 工具调用参数增量
                        if let Some(func) = function {
                            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                                if !args.is_empty() {
                                    let block_index = state.current_index + tc_index;
                                    state.append_tool_arguments(block_index, args);

                                    events.push((
                                        "content_block_delta".to_string(),
                                        json!({
                                            "type": "content_block_delta",
                                            "index": block_index,
                                            "delta": {
                                                "type": "input_json_delta",
                                                "partial_json": args
                                            }
                                        }),
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            // 处理完成
            if let Some(reason) = finish_reason {
                // 结束当前块
                if state.current_block_type.is_some() {
                    events.push((
                        "content_block_stop".to_string(),
                        json!({
                            "type": "content_block_stop",
                            "index": state.current_index
                        }),
                    ));
                    state.end_block();
                }

                // 发送 message_delta
                let stop_reason = map_finish_reason_to_stop_reason(reason);
                events.push((
                    "message_delta".to_string(),
                    json!({
                        "type": "message_delta",
                        "delta": {
                            "stop_reason": stop_reason,
                            "stop_sequence": null
                        },
                        "usage": {
                            "output_tokens": 0
                        }
                    }),
                ));

                // 发送 message_stop
                events.push((
                    "message_stop".to_string(),
                    json!({
                        "type": "message_stop"
                    }),
                ));
            }
        }
    }

    Ok(events)
}

// ================================================================
// Helper Functions
// ================================================================

/// 映射 Anthropic stop_reason 到 OpenAI finish_reason
fn map_stop_reason_to_finish_reason(stop_reason: &str) -> String {
    match stop_reason {
        "end_turn" => "stop".to_string(),
        "max_tokens" => "length".to_string(),
        "tool_use" => "tool_calls".to_string(),
        "stop_sequence" => "stop".to_string(),
        other => other.to_string(),
    }
}

/// 映射 OpenAI finish_reason 到 Anthropic stop_reason
fn map_finish_reason_to_stop_reason(finish_reason: &str) -> String {
    match finish_reason {
        "stop" => "end_turn".to_string(),
        "length" => "max_tokens".to_string(),
        "tool_calls" => "tool_use".to_string(),
        "content_filter" => "end_turn".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_state_text() {
        let mut state = TransformState::new();
        state.start_block(BlockType::Text, 0);
        state.append_text("Hello ");
        state.append_text("World");
        state.end_block();

        assert_eq!(state.text_content, "Hello World");
        assert!(state.current_block_type.is_none());
    }

    #[test]
    fn test_transform_state_tool_call() {
        let mut state = TransformState::new();
        state.start_block(BlockType::ToolUse, 0);
        state.init_tool_call(0, "call_123", "get_weather");
        state.append_tool_arguments(0, r#"{"location":"#);
        state.append_tool_arguments(0, r#""Tokyo"}"#);
        state.end_block();

        assert!(state.has_tool_call);
        let tool = state.tool_calls.get(&0).unwrap();
        assert_eq!(tool.id, "call_123");
        assert_eq!(tool.name, "get_weather");
        assert_eq!(tool.arguments, r#"{"location":"Tokyo"}"#);
        assert!(tool.completed);
    }

    #[test]
    fn test_stop_reason_mapping() {
        assert_eq!(map_stop_reason_to_finish_reason("end_turn"), "stop");
        assert_eq!(map_stop_reason_to_finish_reason("max_tokens"), "length");
        assert_eq!(map_stop_reason_to_finish_reason("tool_use"), "tool_calls");

        assert_eq!(map_finish_reason_to_stop_reason("stop"), "end_turn");
        assert_eq!(map_finish_reason_to_stop_reason("length"), "max_tokens");
        assert_eq!(map_finish_reason_to_stop_reason("tool_calls"), "tool_use");
    }

    #[test]
    fn test_anthropic_message_start_transform() {
        let mut state = TransformState::new();
        let data = json!({
            "message": {
                "id": "msg_123",
                "model": "claude-3-5-sonnet",
                "usage": {
                    "input_tokens": 100
                }
            }
        });

        let result = transform_anthropic_sse_to_openai("message_start", &data, &mut state).unwrap();

        assert!(result.is_some());
        let chunk = result.unwrap();
        assert_eq!(chunk["id"], "msg_123");
        assert_eq!(chunk["choices"][0]["delta"]["role"], "assistant");
        assert_eq!(state.message_id, Some("msg_123".to_string()));
    }

    #[test]
    fn test_anthropic_text_delta_transform() {
        let mut state = TransformState::new();
        state.message_id = Some("msg_123".to_string());
        state.model = Some("claude-3-5-sonnet".to_string());
        state.start_block(BlockType::Text, 0);

        let data = json!({
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": "Hello"
            }
        });

        let result =
            transform_anthropic_sse_to_openai("content_block_delta", &data, &mut state).unwrap();

        assert!(result.is_some());
        let chunk = result.unwrap();
        assert_eq!(chunk["choices"][0]["delta"]["content"], "Hello");
        assert_eq!(state.text_content, "Hello");
    }

    #[test]
    fn test_anthropic_tool_use_transform() {
        let mut state = TransformState::new();
        state.message_id = Some("msg_123".to_string());
        state.model = Some("claude-3-5-sonnet".to_string());

        // Tool use block start
        let start_data = json!({
            "index": 0,
            "content_block": {
                "type": "tool_use",
                "id": "call_abc",
                "name": "get_weather"
            }
        });

        let result =
            transform_anthropic_sse_to_openai("content_block_start", &start_data, &mut state)
                .unwrap();

        assert!(result.is_some());
        let chunk = result.unwrap();
        assert_eq!(chunk["choices"][0]["delta"]["tool_calls"][0]["id"], "call_abc");
        assert_eq!(
            chunk["choices"][0]["delta"]["tool_calls"][0]["function"]["name"],
            "get_weather"
        );

        // Tool use argument delta
        let delta_data = json!({
            "index": 0,
            "delta": {
                "type": "input_json_delta",
                "partial_json": r#"{"city":"Tokyo"}"#
            }
        });

        let result =
            transform_anthropic_sse_to_openai("content_block_delta", &delta_data, &mut state)
                .unwrap();

        assert!(result.is_some());
        let chunk = result.unwrap();
        assert_eq!(
            chunk["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            r#"{"city":"Tokyo"}"#
        );
    }
}
