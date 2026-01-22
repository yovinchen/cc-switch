//! OpenAI 消息格式
//!
//! 实现 OpenAI API 的消息结构和与统一格式的双向转换
//!
//! ## 双向转换
//!
//! - `Message → Vec<OpenAIMessage>`: 统一格式转 OpenAI（用于发送请求）
//! - `OpenAIMessage → Message`: OpenAI 转统一格式（用于解析响应）
//!
//! ## 注意事项
//!
//! - OpenAI 不支持 Reasoning/Thinking 内容，转换时会被忽略
//! - 工具结果在 OpenAI 格式中是独立的 Tool 角色消息

use crate::proxy::error::ProxyError;
use crate::proxy::unified::message::*;
use base64::Engine;
use serde::{Deserialize, Serialize};

/// OpenAI 消息
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum OpenAIMessage {
    System {
        content: String,
    },
    User {
        content: OpenAIContent,
    },
    Assistant {
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<OpenAIToolCall>>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

/// OpenAI 内容（可以是字符串或数组）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum OpenAIContent {
    Text(String),
    Parts(Vec<OpenAIUserContent>),
}

/// OpenAI 用户内容部分
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAIUserContent {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

/// 图片 URL
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// OpenAI 工具调用
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: OpenAIFunction,
}

/// OpenAI 函数
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenAIFunction {
    pub name: String,
    pub arguments: String,
}

// ================================================================
// Message → Vec<OpenAIMessage> (统一格式 → OpenAI)
// ================================================================

impl TryFrom<&Message> for Vec<OpenAIMessage> {
    type Error = ProxyError;

    fn try_from(msg: &Message) -> Result<Self, Self::Error> {
        match msg {
            Message::System { content } => Ok(vec![OpenAIMessage::System {
                content: content.clone(),
            }]),
            Message::User { content } => {
                // 分离 tool_result 和其他内容
                let (tool_results, other): (Vec<_>, Vec<_>) = content
                    .iter()
                    .partition(|c| matches!(c, UserContent::ToolResult { .. }));

                let mut messages = Vec::new();

                // 添加 tool 消息
                for tr in tool_results {
                    if let UserContent::ToolResult { id, content, .. } = tr {
                        let text = content
                            .iter()
                            .map(|c| match c {
                                ToolResultContent::Text { text } => text.clone(),
                                ToolResultContent::Image { .. } => "[Image]".to_string(),
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        messages.push(OpenAIMessage::Tool {
                            tool_call_id: id.clone(),
                            content: text,
                        });
                    }
                }

                // 添加 user 消息
                if !other.is_empty() {
                    let content = convert_user_content(&other)?;
                    messages.push(OpenAIMessage::User { content });
                }

                Ok(messages)
            }
            Message::Assistant { content, .. } => {
                // 分离文本和工具调用，忽略 Reasoning
                let (texts, tool_calls): (Vec<_>, Vec<_>) = content.iter().partition(|c| {
                    matches!(c, AssistantContent::Text { .. } | AssistantContent::Reasoning { .. })
                });

                let text_content = texts
                    .iter()
                    .filter_map(|c| {
                        if let AssistantContent::Text { text, .. } = c {
                            Some(text.clone())
                        } else {
                            // Reasoning 内容被忽略（OpenAI 不支持）
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let tool_calls_vec: Vec<OpenAIToolCall> = tool_calls
                    .iter()
                    .filter_map(|c| {
                        if let AssistantContent::ToolCall {
                            id,
                            name,
                            arguments,
                            ..
                        } = c
                        {
                            Some(OpenAIToolCall {
                                id: id.clone(),
                                call_type: "function".to_string(),
                                function: OpenAIFunction {
                                    name: name.clone(),
                                    arguments: serde_json::to_string(arguments).unwrap_or_default(),
                                },
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                Ok(vec![OpenAIMessage::Assistant {
                    content: if text_content.is_empty() {
                        None
                    } else {
                        Some(text_content)
                    },
                    tool_calls: if tool_calls_vec.is_empty() {
                        None
                    } else {
                        Some(tool_calls_vec)
                    },
                }])
            }
            Message::Tool {
                tool_call_id,
                content,
            } => Ok(vec![OpenAIMessage::Tool {
                tool_call_id: tool_call_id.clone(),
                content: content.clone(),
            }]),
        }
    }
}

fn convert_user_content(content: &[&UserContent]) -> Result<OpenAIContent, ProxyError> {
    if content.len() == 1 {
        if let UserContent::Text { text, .. } = content[0] {
            return Ok(OpenAIContent::Text(text.clone()));
        }
    }

    let parts: Vec<OpenAIUserContent> = content
        .iter()
        .map(|c| match c {
            UserContent::Text { text, .. } => Ok(OpenAIUserContent::Text { text: text.clone() }),
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
                        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                        format!("data:{};base64,{}", media_type, encoded)
                    }
                    DataSource::Text { .. } | DataSource::Unknown => {
                        return Err(ProxyError::TransformError(
                            "Invalid data source for image".to_string(),
                        ))
                    }
                };
                let detail = data.detail.as_ref().map(|d| match d {
                    ImageDetail::Low => "low".to_string(),
                    ImageDetail::High => "high".to_string(),
                    ImageDetail::Auto => "auto".to_string(),
                });
                Ok(OpenAIUserContent::ImageUrl {
                    image_url: ImageUrl { url, detail },
                })
            }
            _ => Err(ProxyError::TransformError(
                "Unsupported content type for OpenAI user message".to_string(),
            )),
        })
        .collect::<Result<_, _>>()?;

    Ok(OpenAIContent::Parts(parts))
}

// ================================================================
// OpenAIMessage → Message (OpenAI → 统一格式)
// ================================================================

impl TryFrom<&OpenAIMessage> for Message {
    type Error = ProxyError;

    fn try_from(msg: &OpenAIMessage) -> Result<Self, Self::Error> {
        match msg {
            OpenAIMessage::System { content } => Ok(Message::System {
                content: content.clone(),
            }),
            OpenAIMessage::User { content } => {
                let user_content = openai_content_to_unified(content)?;
                Ok(Message::User {
                    content: user_content,
                })
            }
            OpenAIMessage::Assistant {
                content,
                tool_calls,
            } => {
                let mut assistant_content = Vec::new();

                // 添加文本内容
                if let Some(text) = content {
                    if !text.is_empty() {
                        assistant_content.push(AssistantContent::Text {
                            text: text.clone(),
                            cache_control: None,
                        });
                    }
                }

                // 添加工具调用
                if let Some(calls) = tool_calls {
                    for tc in calls {
                        let arguments: serde_json::Value =
                            serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                        assistant_content.push(AssistantContent::ToolCall {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments,
                            signature: None,
                        });
                    }
                }

                Ok(Message::Assistant {
                    id: None,
                    content: assistant_content,
                })
            }
            OpenAIMessage::Tool {
                tool_call_id,
                content,
            } => Ok(Message::Tool {
                tool_call_id: tool_call_id.clone(),
                content: content.clone(),
            }),
        }
    }
}

fn openai_content_to_unified(content: &OpenAIContent) -> Result<Vec<UserContent>, ProxyError> {
    match content {
        OpenAIContent::Text(text) => Ok(vec![UserContent::Text {
            text: text.clone(),
            cache_control: None,
        }]),
        OpenAIContent::Parts(parts) => {
            let mut result = Vec::new();
            for part in parts {
                match part {
                    OpenAIUserContent::Text { text } => {
                        result.push(UserContent::Text {
                            text: text.clone(),
                            cache_control: None,
                        });
                    }
                    OpenAIUserContent::ImageUrl { image_url } => {
                        // 解析 data URL 或普通 URL
                        let (source, media_type) =
                            if let Some((src, mt)) = DataSource::from_data_url(&image_url.url) {
                                (src, Some(mt))
                            } else {
                                (
                                    DataSource::Url {
                                        url: image_url.url.clone(),
                                    },
                                    None,
                                )
                            };
                        let detail = image_url.detail.as_ref().and_then(|d| match d.as_str() {
                            "low" => Some(ImageDetail::Low),
                            "high" => Some(ImageDetail::High),
                            "auto" => Some(ImageDetail::Auto),
                            _ => None,
                        });
                        result.push(UserContent::Image {
                            data: ImageData {
                                source,
                                media_type,
                                detail,
                            },
                        });
                    }
                }
            }
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_system_message_conversion() {
        let unified = Message::system("You are a helpful assistant.");
        let openai: Vec<OpenAIMessage> = (&unified).try_into().unwrap();
        assert_eq!(openai.len(), 1);
        if let OpenAIMessage::System { content } = &openai[0] {
            assert_eq!(content, "You are a helpful assistant.");
        } else {
            panic!("Expected System message");
        }
    }

    #[test]
    fn test_user_text_conversion() {
        let unified = Message::user_text("Hello");
        let openai: Vec<OpenAIMessage> = (&unified).try_into().unwrap();
        assert_eq!(openai.len(), 1);
        if let OpenAIMessage::User { content } = &openai[0] {
            if let OpenAIContent::Text(text) = content {
                assert_eq!(text, "Hello");
            } else {
                panic!("Expected text content");
            }
        } else {
            panic!("Expected User message");
        }
    }

    #[test]
    fn test_assistant_tool_call_conversion() {
        let unified = Message::Assistant {
            id: None,
            content: vec![
                AssistantContent::text("Let me check"),
                AssistantContent::tool_call("call_123", "get_weather", json!({"location": "Tokyo"})),
            ],
        };
        let openai: Vec<OpenAIMessage> = (&unified).try_into().unwrap();
        assert_eq!(openai.len(), 1);
        if let OpenAIMessage::Assistant {
            content,
            tool_calls,
        } = &openai[0]
        {
            assert_eq!(content.as_ref().unwrap(), "Let me check");
            assert_eq!(tool_calls.as_ref().unwrap().len(), 1);
            assert_eq!(tool_calls.as_ref().unwrap()[0].id, "call_123");
        } else {
            panic!("Expected Assistant message");
        }
    }

    #[test]
    fn test_tool_result_conversion() {
        let unified = Message::User {
            content: vec![UserContent::tool_result_text("call_123", "Sunny, 25°C")],
        };
        let openai: Vec<OpenAIMessage> = (&unified).try_into().unwrap();
        assert_eq!(openai.len(), 1);
        if let OpenAIMessage::Tool {
            tool_call_id,
            content,
        } = &openai[0]
        {
            assert_eq!(tool_call_id, "call_123");
            assert_eq!(content, "Sunny, 25°C");
        } else {
            panic!("Expected Tool message");
        }
    }

    #[test]
    fn test_reasoning_is_ignored() {
        // OpenAI 不支持 Reasoning，应该被忽略
        let unified = Message::Assistant {
            id: None,
            content: vec![
                AssistantContent::reasoning("Let me think..."),
                AssistantContent::text("The answer is 42"),
            ],
        };
        let openai: Vec<OpenAIMessage> = (&unified).try_into().unwrap();
        if let OpenAIMessage::Assistant { content, .. } = &openai[0] {
            // Reasoning 被忽略，只有文本内容
            assert_eq!(content.as_ref().unwrap(), "The answer is 42");
        } else {
            panic!("Expected Assistant message");
        }
    }

    // ========== 反向转换测试 ==========

    #[test]
    fn test_openai_to_unified_system() {
        let openai = OpenAIMessage::System {
            content: "You are helpful.".to_string(),
        };
        let unified: Message = (&openai).try_into().unwrap();
        if let Message::System { content } = unified {
            assert_eq!(content, "You are helpful.");
        } else {
            panic!("Expected System message");
        }
    }

    #[test]
    fn test_openai_to_unified_user() {
        let openai = OpenAIMessage::User {
            content: OpenAIContent::Text("Hello".to_string()),
        };
        let unified: Message = (&openai).try_into().unwrap();
        if let Message::User { content } = unified {
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
    fn test_openai_to_unified_assistant_with_tools() {
        let openai = OpenAIMessage::Assistant {
            content: Some("Let me search".to_string()),
            tool_calls: Some(vec![OpenAIToolCall {
                id: "call_abc".to_string(),
                call_type: "function".to_string(),
                function: OpenAIFunction {
                    name: "search".to_string(),
                    arguments: r#"{"query":"rust"}"#.to_string(),
                },
            }]),
        };
        let unified: Message = (&openai).try_into().unwrap();
        if let Message::Assistant { content, .. } = unified {
            assert_eq!(content.len(), 2);
            if let AssistantContent::Text { text, .. } = &content[0] {
                assert_eq!(text, "Let me search");
            }
            if let AssistantContent::ToolCall { id, name, .. } = &content[1] {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "search");
            }
        } else {
            panic!("Expected Assistant message");
        }
    }

    #[test]
    fn test_openai_to_unified_tool() {
        let openai = OpenAIMessage::Tool {
            tool_call_id: "call_xyz".to_string(),
            content: "Result data".to_string(),
        };
        let unified: Message = (&openai).try_into().unwrap();
        if let Message::Tool {
            tool_call_id,
            content,
        } = unified
        {
            assert_eq!(tool_call_id, "call_xyz");
            assert_eq!(content, "Result data");
        } else {
            panic!("Expected Tool message");
        }
    }

    #[test]
    fn test_roundtrip_user_image() {
        let original = Message::User {
            content: vec![UserContent::image_base64("image/png", "iVBORw0KGgo=")],
        };

        let openai: Vec<OpenAIMessage> = (&original).try_into().unwrap();
        let roundtrip: Message = (&openai[0]).try_into().unwrap();

        if let Message::User { content } = roundtrip {
            if let UserContent::Image { data } = &content[0] {
                assert_eq!(data.media_type.as_ref().unwrap(), "image/png");
            } else {
                panic!("Expected Image content");
            }
        } else {
            panic!("Expected User message");
        }
    }
}
