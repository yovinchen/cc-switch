//! Anthropic (Claude) 消息格式
//!
//! 实现 Anthropic API 的消息结构和与统一格式的双向转换
//!
//! ## 双向转换
//!
//! - `Message → AnthropicMessage`: 统一格式转 Anthropic（用于发送请求）
//! - `AnthropicMessage → Message`: Anthropic 转统一格式（用于解析响应）

use crate::proxy::error::ProxyError;
use crate::proxy::unified::message::*;
use serde::{Deserialize, Serialize};

/// Anthropic 消息
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnthropicMessage {
    pub role: AnthropicRole,
    pub content: Vec<AnthropicContent>,
}

/// Anthropic 角色
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AnthropicRole {
    User,
    Assistant,
}

/// Anthropic 内容块
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicContent {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Image {
        source: AnthropicImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

/// Anthropic 图片源
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnthropicImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

// ================================================================
// Message → AnthropicMessage (统一格式 → Anthropic)
// ================================================================

impl TryFrom<&Message> for AnthropicMessage {
    type Error = ProxyError;

    fn try_from(msg: &Message) -> Result<Self, Self::Error> {
        match msg {
            Message::User { content } => Ok(AnthropicMessage {
                role: AnthropicRole::User,
                content: content
                    .iter()
                    .map(|c| c.try_into())
                    .collect::<Result<_, _>>()?,
            }),
            Message::Assistant { content, .. } => Ok(AnthropicMessage {
                role: AnthropicRole::Assistant,
                content: content
                    .iter()
                    .map(|c| c.try_into())
                    .collect::<Result<_, _>>()?,
            }),
            Message::System { .. } => Err(ProxyError::TransformError(
                "System messages should be handled separately in Anthropic format".to_string(),
            )),
            Message::Tool { tool_call_id, content } => {
                // OpenAI Tool 消息转换为 Anthropic 的 user + tool_result
                Ok(AnthropicMessage {
                    role: AnthropicRole::User,
                    content: vec![AnthropicContent::ToolResult {
                        tool_use_id: tool_call_id.clone(),
                        content: serde_json::json!(content),
                        is_error: None,
                    }],
                })
            }
        }
    }
}

impl TryFrom<&UserContent> for AnthropicContent {
    type Error = ProxyError;

    fn try_from(content: &UserContent) -> Result<Self, Self::Error> {
        match content {
            UserContent::Text { text, cache_control } => Ok(AnthropicContent::Text {
                text: text.clone(),
                cache_control: cache_control.clone(),
            }),
            UserContent::Image { data } => {
                let source = match &data.source {
                    DataSource::Base64 { data: base64_data } => AnthropicImageSource {
                        source_type: "base64".to_string(),
                        media_type: data
                            .media_type
                            .clone()
                            .unwrap_or_else(|| "image/png".to_string()),
                        data: base64_data.clone(),
                    },
                    DataSource::Url { url } => AnthropicImageSource {
                        source_type: "url".to_string(),
                        media_type: data
                            .media_type
                            .clone()
                            .unwrap_or_else(|| "image/png".to_string()),
                        data: url.clone(),
                    },
                    _ => {
                        return Err(ProxyError::TransformError(
                            "Unsupported image source type for Anthropic".to_string(),
                        ))
                    }
                };
                Ok(AnthropicContent::Image { source })
            }
            UserContent::Audio { .. } => Err(ProxyError::TransformError(
                "Audio content is not supported by Anthropic API".to_string(),
            )),
            UserContent::Video { .. } => Err(ProxyError::TransformError(
                "Video content is not supported by Anthropic API".to_string(),
            )),
            UserContent::ToolResult { id, content, is_error } => {
                let content_value = content
                    .iter()
                    .map(|c| match c {
                        ToolResultContent::Text { text } => {
                            serde_json::json!({"type": "text", "text": text})
                        }
                        ToolResultContent::Image { data } => {
                            let media_type = data.media_type.as_deref().unwrap_or("image/png");
                            match &data.source {
                                DataSource::Base64 { data: base64 } => serde_json::json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": media_type,
                                        "data": base64
                                    }
                                }),
                                DataSource::Url { url } => serde_json::json!({
                                    "type": "image",
                                    "source": {
                                        "type": "url",
                                        "url": url
                                    }
                                }),
                                _ => serde_json::json!({
                                    "type": "text",
                                    "text": "[Unsupported image source]"
                                }),
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                Ok(AnthropicContent::ToolResult {
                    tool_use_id: id.clone(),
                    content: serde_json::Value::Array(content_value),
                    is_error: *is_error,
                })
            }
            UserContent::Document { .. } => Err(ProxyError::TransformError(
                "Document content not yet supported for Anthropic".to_string(),
            )),
        }
    }
}

impl TryFrom<&AssistantContent> for AnthropicContent {
    type Error = ProxyError;

    fn try_from(content: &AssistantContent) -> Result<Self, Self::Error> {
        match content {
            AssistantContent::Text { text, cache_control } => Ok(AnthropicContent::Text {
                text: text.clone(),
                cache_control: cache_control.clone(),
            }),
            AssistantContent::ToolCall {
                id,
                name,
                arguments,
                ..
            } => Ok(AnthropicContent::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: arguments.clone(),
            }),
            AssistantContent::Reasoning { thinking, signature } => {
                Ok(AnthropicContent::Thinking {
                    thinking: thinking.clone(),
                    signature: signature.clone(),
                })
            }
        }
    }
}

// ================================================================
// AnthropicMessage → Message (Anthropic → 统一格式)
// ================================================================

impl TryFrom<&AnthropicMessage> for Message {
    type Error = ProxyError;

    fn try_from(msg: &AnthropicMessage) -> Result<Self, Self::Error> {
        match msg.role {
            AnthropicRole::User => {
                let content: Vec<UserContent> = msg
                    .content
                    .iter()
                    .filter_map(|c| anthropic_content_to_user(c).ok())
                    .collect();
                Ok(Message::User { content })
            }
            AnthropicRole::Assistant => {
                let content: Vec<AssistantContent> = msg
                    .content
                    .iter()
                    .filter_map(|c| anthropic_content_to_assistant(c).ok())
                    .collect();
                Ok(Message::Assistant { id: None, content })
            }
        }
    }
}

fn anthropic_content_to_user(content: &AnthropicContent) -> Result<UserContent, ProxyError> {
    match content {
        AnthropicContent::Text { text, cache_control } => Ok(UserContent::Text {
            text: text.clone(),
            cache_control: cache_control.clone(),
        }),
        AnthropicContent::Image { source } => Ok(UserContent::Image {
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
        }),
        AnthropicContent::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => {
            // 解析 content 数组为 ToolResultContent
            let result_content = if let Some(arr) = content.as_array() {
                arr.iter()
                    .filter_map(|item| {
                        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                            item.get("text")
                                .and_then(|t| t.as_str())
                                .map(|t| ToolResultContent::Text { text: t.to_string() })
                        } else {
                            None
                        }
                    })
                    .collect()
            } else if let Some(text) = content.as_str() {
                vec![ToolResultContent::Text { text: text.to_string() }]
            } else {
                vec![ToolResultContent::Text {
                    text: content.to_string(),
                }]
            };
            Ok(UserContent::ToolResult {
                id: tool_use_id.clone(),
                content: result_content,
                is_error: *is_error,
            })
        }
        _ => Err(ProxyError::TransformError(
            "Unexpected content type in user message".to_string(),
        )),
    }
}

fn anthropic_content_to_assistant(
    content: &AnthropicContent,
) -> Result<AssistantContent, ProxyError> {
    match content {
        AnthropicContent::Text { text, cache_control } => Ok(AssistantContent::Text {
            text: text.clone(),
            cache_control: cache_control.clone(),
        }),
        AnthropicContent::ToolUse { id, name, input } => Ok(AssistantContent::ToolCall {
            id: id.clone(),
            name: name.clone(),
            arguments: input.clone(),
            signature: None,
        }),
        AnthropicContent::Thinking { thinking, signature } => Ok(AssistantContent::Reasoning {
            thinking: thinking.clone(),
            signature: signature.clone(),
        }),
        _ => Err(ProxyError::TransformError(
            "Unexpected content type in assistant message".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_user_text_conversion() {
        let unified = Message::user_text("Hello");
        let anthropic: AnthropicMessage = (&unified).try_into().unwrap();
        assert_eq!(anthropic.role, AnthropicRole::User);
        assert_eq!(anthropic.content.len(), 1);
    }

    #[test]
    fn test_assistant_tool_call_conversion() {
        let unified = Message::Assistant {
            id: None,
            content: vec![AssistantContent::tool_call(
                "call_123",
                "get_weather",
                json!({"location": "Tokyo"}),
            )],
        };
        let anthropic: AnthropicMessage = (&unified).try_into().unwrap();
        assert_eq!(anthropic.role, AnthropicRole::Assistant);
        if let AnthropicContent::ToolUse { id, name, .. } = &anthropic.content[0] {
            assert_eq!(id, "call_123");
            assert_eq!(name, "get_weather");
        } else {
            panic!("Expected ToolUse content");
        }
    }

    #[test]
    fn test_reasoning_conversion() {
        let unified = Message::Assistant {
            id: None,
            content: vec![AssistantContent::reasoning_with_signature(
                "Let me think...",
                "sig123",
            )],
        };
        let anthropic: AnthropicMessage = (&unified).try_into().unwrap();
        if let AnthropicContent::Thinking { thinking, signature } = &anthropic.content[0] {
            assert_eq!(thinking, "Let me think...");
            assert_eq!(signature.as_ref().unwrap(), "sig123");
        } else {
            panic!("Expected Thinking content");
        }
    }

    // ========== 反向转换测试 ==========

    #[test]
    fn test_anthropic_to_unified_user_text() {
        let anthropic = AnthropicMessage {
            role: AnthropicRole::User,
            content: vec![AnthropicContent::Text {
                text: "Hello".to_string(),
                cache_control: None,
            }],
        };
        let unified: Message = (&anthropic).try_into().unwrap();
        if let Message::User { content } = unified {
            assert_eq!(content.len(), 1);
            if let UserContent::Text { text, .. } = &content[0] {
                assert_eq!(text, "Hello");
            } else {
                panic!("Expected Text content");
            }
        } else {
            panic!("Expected User message");
        }
    }

    #[test]
    fn test_anthropic_to_unified_assistant_tool_use() {
        let anthropic = AnthropicMessage {
            role: AnthropicRole::Assistant,
            content: vec![AnthropicContent::ToolUse {
                id: "call_123".to_string(),
                name: "get_weather".to_string(),
                input: json!({"location": "Tokyo"}),
            }],
        };
        let unified: Message = (&anthropic).try_into().unwrap();
        if let Message::Assistant { content, .. } = unified {
            if let AssistantContent::ToolCall { id, name, .. } = &content[0] {
                assert_eq!(id, "call_123");
                assert_eq!(name, "get_weather");
            } else {
                panic!("Expected ToolCall content");
            }
        } else {
            panic!("Expected Assistant message");
        }
    }

    #[test]
    fn test_anthropic_to_unified_thinking() {
        let anthropic = AnthropicMessage {
            role: AnthropicRole::Assistant,
            content: vec![AnthropicContent::Thinking {
                thinking: "Analyzing...".to_string(),
                signature: Some("sig_abc".to_string()),
            }],
        };
        let unified: Message = (&anthropic).try_into().unwrap();
        if let Message::Assistant { content, .. } = unified {
            if let AssistantContent::Reasoning { thinking, signature } = &content[0] {
                assert_eq!(thinking, "Analyzing...");
                assert_eq!(signature.as_ref().unwrap(), "sig_abc");
            } else {
                panic!("Expected Reasoning content");
            }
        } else {
            panic!("Expected Assistant message");
        }
    }

    #[test]
    fn test_roundtrip_conversion() {
        // 测试双向转换的一致性
        let original = Message::Assistant {
            id: None,
            content: vec![
                AssistantContent::text("Let me help"),
                AssistantContent::tool_call("tc_1", "search", json!({"q": "rust"})),
            ],
        };

        // Unified → Anthropic → Unified
        let anthropic: AnthropicMessage = (&original).try_into().unwrap();
        let roundtrip: Message = (&anthropic).try_into().unwrap();

        if let (Message::Assistant { content: orig, .. }, Message::Assistant { content: rt, .. }) =
            (&original, &roundtrip)
        {
            assert_eq!(orig.len(), rt.len());
        } else {
            panic!("Roundtrip failed");
        }
    }
}
