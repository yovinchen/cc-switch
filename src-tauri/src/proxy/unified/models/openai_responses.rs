//! OpenAI Responses API 消息格式
//!
//! 实现 OpenAI Responses API (/v1/responses) 的消息结构和与统一格式的双向转换
//!
//! ## 双向转换
//!
//! - `Message → ResponsesInput`: 统一格式转 Responses API（用于发送请求）
//! - `ResponsesOutput → Message`: Responses API 转统一格式（用于解析响应）
//!
//! ## 与 Chat Completions API 的主要差异
//!
//! | 特性 | Chat Completions | Responses API |
//! |------|------------------|---------------|
//! | 消息容器 | `messages[]` | `input[]` |
//! | 系统提示 | `messages[0].role="system"` | `instructions` |
//! | 用户内容 | `content` | `input_text`, `input_image` |
//! | 工具结果 | `role="tool"` | `type="function_call_output"` |
//! | Reasoning | 不支持 | 支持 (`reasoning` 配置) |
//! | 消息 ID | 无 | 必需 (`id` 字段) |

use crate::proxy::error::ProxyError;
use crate::proxy::unified::message::*;
use base64::Engine;
use serde::{Deserialize, Serialize};

// ================================================================
// Responses API 请求类型
// ================================================================

/// Responses API 输入项
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ResponsesInput {
    /// 用户消息
    Message(ResponsesMessage),
    /// 函数调用输出
    FunctionCallOutput(ResponsesFunctionCallOutput),
}

/// Responses API 消息
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponsesMessage {
    pub role: ResponsesRole,
    #[serde(rename = "type")]
    pub msg_type: String, // "message"
    pub content: Vec<ResponsesContentItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Responses API 角色
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ResponsesRole {
    User,
    Assistant,
}

/// Responses API 内容项
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesContentItem {
    /// 输入文本
    InputText { text: String },
    /// 输入图片
    InputImage { image_url: String },
    /// 输出文本
    OutputText { text: String },
}

/// 函数调用输出
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponsesFunctionCallOutput {
    #[serde(rename = "type")]
    pub output_type: String, // "function_call_output"
    pub call_id: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

// ================================================================
// Responses API 响应类型
// ================================================================

/// Responses API 响应
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponsesApiResponse {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub status: String,
    pub model: String,
    pub output: Vec<ResponsesOutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponsesUsage>,
}

/// Responses API 输出项
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesOutputItem {
    /// 消息输出
    Message {
        id: String,
        role: String,
        status: String,
        content: Vec<ResponsesContentItem>,
    },
    /// 函数调用
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: serde_json::Value,
        status: String,
    },
    /// 推理/思考
    Reasoning {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<Vec<ReasoningSummary>>,
    },
}

/// 推理摘要
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReasoningSummary {
    #[serde(rename = "type")]
    pub summary_type: String,
    pub text: String,
}

/// Responses API 用量
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponsesUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<TokenDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<TokenDetails>,
}

/// Token 详情
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<i64>,
}

// ================================================================
// Message → Responses API (统一格式 → Responses API)
// ================================================================

impl TryFrom<&Message> for Vec<ResponsesInput> {
    type Error = ProxyError;

    fn try_from(msg: &Message) -> Result<Self, Self::Error> {
        match msg {
            Message::User { content } => {
                let mut items = Vec::new();

                for c in content {
                    match c {
                        UserContent::Text { text, .. } => {
                            items.push(ResponsesInput::Message(ResponsesMessage {
                                role: ResponsesRole::User,
                                msg_type: "message".to_string(),
                                content: vec![ResponsesContentItem::InputText {
                                    text: text.clone(),
                                }],
                                id: None,
                                status: None,
                            }));
                        }
                        UserContent::Image { data } => {
                            let url = match &data.source {
                                DataSource::Base64 { data: base64_data } => {
                                    let media_type = data
                                        .media_type
                                        .as_ref()
                                        .map(|s| s.as_str())
                                        .unwrap_or("image/png");
                                    format!("data:{};base64,{}", media_type, base64_data)
                                }
                                DataSource::Url { url } => url.clone(),
                                DataSource::Raw { bytes } => {
                                    let media_type = data
                                        .media_type
                                        .as_ref()
                                        .map(|s| s.as_str())
                                        .unwrap_or("image/png");
                                    let encoded =
                                        base64::engine::general_purpose::STANDARD.encode(bytes);
                                    format!("data:{};base64,{}", media_type, encoded)
                                }
                                DataSource::Text { .. } | DataSource::Unknown => {
                                    continue; // Skip invalid image sources
                                }
                            };
                            items.push(ResponsesInput::Message(ResponsesMessage {
                                role: ResponsesRole::User,
                                msg_type: "message".to_string(),
                                content: vec![ResponsesContentItem::InputImage { image_url: url }],
                                id: None,
                                status: None,
                            }));
                        }
                        UserContent::Audio { .. } => {
                            // Audio not supported in Responses API, skip
                        }
                        UserContent::Video { .. } => {
                            // Video not supported in Responses API, skip
                        }
                        UserContent::ToolResult { id, content, .. } => {
                            let output = content
                                .iter()
                                .map(|c| match c {
                                    ToolResultContent::Text { text } => text.clone(),
                                    ToolResultContent::Image { .. } => "[Image]".to_string(),
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            items.push(ResponsesInput::FunctionCallOutput(
                                ResponsesFunctionCallOutput {
                                    output_type: "function_call_output".to_string(),
                                    call_id: id.clone(),
                                    output,
                                    status: Some("completed".to_string()),
                                },
                            ));
                        }
                        UserContent::Document { .. } => {
                            // Documents 暂不支持
                        }
                    }
                }

                Ok(items)
            }
            Message::Assistant { id, content, .. } => {
                let mut items = Vec::new();

                // 分离不同类型的内容
                let mut text_contents = Vec::new();
                let mut tool_calls = Vec::new();
                let mut reasonings = Vec::new();

                for c in content {
                    match c {
                        AssistantContent::Text { text, .. } => {
                            text_contents.push(text.clone());
                        }
                        AssistantContent::ToolCall {
                            id: tool_id,
                            name,
                            arguments,
                            ..
                        } => {
                            tool_calls.push((tool_id.clone(), name.clone(), arguments.clone()));
                        }
                        AssistantContent::Reasoning { thinking, .. } => {
                            reasonings.push(thinking.clone());
                        }
                    }
                }

                // 添加文本消息
                if !text_contents.is_empty() {
                    items.push(ResponsesInput::Message(ResponsesMessage {
                        role: ResponsesRole::Assistant,
                        msg_type: "message".to_string(),
                        content: text_contents
                            .into_iter()
                            .map(|text| ResponsesContentItem::OutputText { text })
                            .collect(),
                        id: id.clone(),
                        status: Some("completed".to_string()),
                    }));
                }

                // 注意：工具调用和推理在 Responses API 中是独立的输出项，
                // 但在请求的 input 中不需要这些（它们只出现在响应中）

                Ok(items)
            }
            Message::System { .. } => {
                // System 消息在 Responses API 中使用 instructions 字段，不在 input 中
                Ok(vec![])
            }
            Message::Tool { tool_call_id, content } => {
                Ok(vec![ResponsesInput::FunctionCallOutput(
                    ResponsesFunctionCallOutput {
                        output_type: "function_call_output".to_string(),
                        call_id: tool_call_id.clone(),
                        output: content.clone(),
                        status: Some("completed".to_string()),
                    },
                )])
            }
        }
    }
}

// ================================================================
// Responses API → Message (Responses API → 统一格式)
// ================================================================

impl TryFrom<&ResponsesOutputItem> for AssistantContent {
    type Error = ProxyError;

    fn try_from(item: &ResponsesOutputItem) -> Result<Self, Self::Error> {
        match item {
            ResponsesOutputItem::Message { content, .. } => {
                // 提取所有文本内容
                let texts: Vec<String> = content
                    .iter()
                    .filter_map(|c| {
                        if let ResponsesContentItem::OutputText { text } = c {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                if texts.is_empty() {
                    Err(ProxyError::TransformError(
                        "Message output has no text content".to_string(),
                    ))
                } else {
                    Ok(AssistantContent::Text {
                        text: texts.join("\n"),
                        cache_control: None,
                    })
                }
            }
            ResponsesOutputItem::FunctionCall {
                id,
                call_id,
                name,
                arguments,
                ..
            } => Ok(AssistantContent::ToolCall {
                id: id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
                signature: Some(call_id.clone()),
            }),
            ResponsesOutputItem::Reasoning { id, summary } => {
                let text = summary
                    .as_ref()
                    .map(|summaries| {
                        summaries
                            .iter()
                            .map(|s| s.text.clone())
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                Ok(AssistantContent::Reasoning {
                    thinking: text,
                    signature: Some(id.clone()),
                })
            }
        }
    }
}

/// 从 Responses API 响应转换为统一格式的助手消息
pub fn responses_output_to_message(
    output: &[ResponsesOutputItem],
) -> Result<Message, ProxyError> {
    let mut content = Vec::new();

    for item in output {
        match item.try_into() {
            Ok(c) => content.push(c),
            Err(_) => continue, // 跳过无法转换的项
        }
    }

    Ok(Message::Assistant { id: None, content })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_user_text_to_responses() {
        let unified = Message::user_text("Hello");
        let responses: Vec<ResponsesInput> = (&unified).try_into().unwrap();
        assert_eq!(responses.len(), 1);
        if let ResponsesInput::Message(msg) = &responses[0] {
            assert_eq!(msg.role, ResponsesRole::User);
            assert_eq!(msg.content.len(), 1);
        } else {
            panic!("Expected Message input");
        }
    }

    #[test]
    fn test_tool_result_to_responses() {
        let unified = Message::User {
            content: vec![UserContent::tool_result_text("call_123", "Result data")],
        };
        let responses: Vec<ResponsesInput> = (&unified).try_into().unwrap();
        assert_eq!(responses.len(), 1);
        if let ResponsesInput::FunctionCallOutput(output) = &responses[0] {
            assert_eq!(output.call_id, "call_123");
            assert_eq!(output.output, "Result data");
        } else {
            panic!("Expected FunctionCallOutput input");
        }
    }

    #[test]
    fn test_responses_output_to_message() {
        let output = vec![
            ResponsesOutputItem::Message {
                id: "msg_123".to_string(),
                role: "assistant".to_string(),
                status: "completed".to_string(),
                content: vec![ResponsesContentItem::OutputText {
                    text: "Hello!".to_string(),
                }],
            },
            ResponsesOutputItem::FunctionCall {
                id: "fc_123".to_string(),
                call_id: "call_123".to_string(),
                name: "search".to_string(),
                arguments: json!({"query": "rust"}),
                status: "completed".to_string(),
            },
            ResponsesOutputItem::Reasoning {
                id: "rs_123".to_string(),
                summary: Some(vec![ReasoningSummary {
                    summary_type: "summary_text".to_string(),
                    text: "Let me think...".to_string(),
                }]),
            },
        ];

        let message = responses_output_to_message(&output).unwrap();
        if let Message::Assistant { content, .. } = message {
            assert_eq!(content.len(), 3);

            // 检查文本内容
            if let AssistantContent::Text { text, .. } = &content[0] {
                assert_eq!(text, "Hello!");
            } else {
                panic!("Expected Text content");
            }

            // 检查工具调用
            if let AssistantContent::ToolCall { name, .. } = &content[1] {
                assert_eq!(name, "search");
            } else {
                panic!("Expected ToolCall content");
            }

            // 检查推理
            if let AssistantContent::Reasoning { thinking, .. } = &content[2] {
                assert_eq!(thinking, "Let me think...");
            } else {
                panic!("Expected Reasoning content");
            }
        } else {
            panic!("Expected Assistant message");
        }
    }

    #[test]
    fn test_system_message_skipped() {
        let unified = Message::system("You are helpful.");
        let responses: Vec<ResponsesInput> = (&unified).try_into().unwrap();
        // System 消息应该返回空数组（在 Responses API 中使用 instructions）
        assert!(responses.is_empty());
    }
}
