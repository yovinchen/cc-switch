//! Gemini 消息格式
//!
//! 实现 Google Gemini API 的消息结构和与统一格式的双向转换
//!
//! ## 双向转换
//!
//! - `Message → GeminiContent`: 统一格式转 Gemini（用于发送请求）
//! - `GeminiContent → Message`: Gemini 转统一格式（用于解析响应）
//!
//! ## 注意事项
//!
//! - Gemini 使用 `thought` 字段标记思考内容，而非独立类型
//! - Gemini 的工具调用使用 `functionCall`，工具结果使用 `functionResponse`
//! - Gemini 不支持 URL 图片，需要转换为 base64

use crate::proxy::error::ProxyError;
use crate::proxy::unified::message::*;
use serde::{Deserialize, Serialize};

/// Gemini 内容
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<GeminiRole>,
    pub parts: Vec<GeminiPart>,
}

/// Gemini 角色
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GeminiRole {
    User,
    Model,
}

/// Gemini 部分
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(flatten)]
    pub content: GeminiPartContent,
}

/// Gemini 部分内容
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GeminiPartContent {
    Text {
        text: String,
    },
    InlineData {
        inline_data: GeminiBlob,
    },
    FunctionCall {
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        function_response: GeminiFunctionResponse,
    },
}

/// Gemini Blob 数据
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiBlob {
    pub mime_type: String,
    pub data: String,
}

/// Gemini 函数调用
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeminiFunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

/// Gemini 函数响应
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeminiFunctionResponse {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
}

// ================================================================
// Message → GeminiContent (统一格式 → Gemini)
// ================================================================

impl TryFrom<&Message> for GeminiContent {
    type Error = ProxyError;

    fn try_from(msg: &Message) -> Result<Self, Self::Error> {
        match msg {
            Message::User { content } => {
                let parts: Vec<GeminiPart> = content
                    .iter()
                    .map(|c| c.try_into())
                    .collect::<Result<_, _>>()?;
                Ok(GeminiContent {
                    role: Some(GeminiRole::User),
                    parts,
                })
            }
            Message::Assistant { content, .. } => {
                let parts: Vec<GeminiPart> = content
                    .iter()
                    .map(|c| c.try_into())
                    .collect::<Result<_, _>>()?;
                Ok(GeminiContent {
                    role: Some(GeminiRole::Model),
                    parts,
                })
            }
            Message::System { .. } => Err(ProxyError::TransformError(
                "System messages should be handled separately in Gemini format".to_string(),
            )),
            Message::Tool {
                tool_call_id,
                content,
            } => {
                // OpenAI Tool 消息转换为 Gemini 的 user + functionResponse
                Ok(GeminiContent {
                    role: Some(GeminiRole::User),
                    parts: vec![GeminiPart {
                        thought: Some(false),
                        thought_signature: None,
                        content: GeminiPartContent::FunctionResponse {
                            function_response: GeminiFunctionResponse {
                                name: tool_call_id.clone(),
                                response: Some(serde_json::json!({ "result": content })),
                            },
                        },
                    }],
                })
            }
        }
    }
}

impl TryFrom<&UserContent> for GeminiPart {
    type Error = ProxyError;

    fn try_from(content: &UserContent) -> Result<Self, Self::Error> {
        let part_content = match content {
            UserContent::Text { text, .. } => GeminiPartContent::Text { text: text.clone() },
            UserContent::Image { data } => {
                let (mime_type, data_str) = match &data.source {
                    DataSource::Base64 { data: base64_data } => (
                        data.media_type
                            .clone()
                            .unwrap_or_else(|| "image/png".to_string()),
                        base64_data.clone(),
                    ),
                    DataSource::Url { .. } => {
                        return Err(ProxyError::TransformError(
                            "Gemini requires base64 encoded images".to_string(),
                        ));
                    }
                    DataSource::Raw { .. } => {
                        return Err(ProxyError::TransformError(
                            "Raw data source not supported for Gemini, use Base64".to_string(),
                        ));
                    }
                    DataSource::Text { .. } => {
                        return Err(ProxyError::TransformError(
                            "Text data source not valid for images".to_string(),
                        ));
                    }
                    DataSource::Unknown => {
                        return Err(ProxyError::TransformError(
                            "Unknown data source cannot be converted".to_string(),
                        ));
                    }
                };
                GeminiPartContent::InlineData {
                    inline_data: GeminiBlob {
                        mime_type,
                        data: data_str,
                    },
                }
            }
            UserContent::Audio { .. } => {
                return Err(ProxyError::TransformError(
                    "Audio content is not supported by Gemini API".to_string(),
                ));
            }
            UserContent::Video { .. } => {
                return Err(ProxyError::TransformError(
                    "Video content is not supported by Gemini API".to_string(),
                ));
            }
            UserContent::ToolResult { id, content, .. } => {
                let result = content
                    .iter()
                    .map(|c| match c {
                        ToolResultContent::Text { text } => serde_json::json!({ "result": text }),
                        ToolResultContent::Image { .. } => {
                            serde_json::json!({ "result": "[Image]" })
                        }
                    })
                    .collect::<Vec<_>>();
                GeminiPartContent::FunctionResponse {
                    function_response: GeminiFunctionResponse {
                        name: id.clone(),
                        response: Some(serde_json::Value::Array(result)),
                    },
                }
            }
            UserContent::Document { .. } => {
                return Err(ProxyError::TransformError(
                    "Document content not yet supported for Gemini".to_string(),
                ))
            }
        };

        Ok(GeminiPart {
            thought: Some(false),
            thought_signature: None,
            content: part_content,
        })
    }
}

impl TryFrom<&AssistantContent> for GeminiPart {
    type Error = ProxyError;

    fn try_from(content: &AssistantContent) -> Result<Self, Self::Error> {
        match content {
            AssistantContent::Text { text, .. } => Ok(GeminiPart {
                thought: Some(false),
                thought_signature: None,
                content: GeminiPartContent::Text { text: text.clone() },
            }),
            AssistantContent::ToolCall {
                name,
                arguments,
                signature,
                ..
            } => Ok(GeminiPart {
                thought: Some(false),
                thought_signature: signature.clone(),
                content: GeminiPartContent::FunctionCall {
                    function_call: GeminiFunctionCall {
                        name: name.clone(),
                        args: arguments.clone(),
                    },
                },
            }),
            AssistantContent::Reasoning { thinking, signature } => Ok(GeminiPart {
                thought: Some(true),
                thought_signature: signature.clone(),
                content: GeminiPartContent::Text {
                    text: thinking.clone(),
                },
            }),
        }
    }
}

// ================================================================
// GeminiContent → Message (Gemini → 统一格式)
// ================================================================

impl TryFrom<&GeminiContent> for Message {
    type Error = ProxyError;

    fn try_from(content: &GeminiContent) -> Result<Self, Self::Error> {
        match content.role {
            Some(GeminiRole::User) => {
                let user_content: Vec<UserContent> = content
                    .parts
                    .iter()
                    .filter_map(|p| gemini_part_to_user(p).ok())
                    .collect();
                Ok(Message::User {
                    content: user_content,
                })
            }
            Some(GeminiRole::Model) | None => {
                let assistant_content: Vec<AssistantContent> = content
                    .parts
                    .iter()
                    .filter_map(|p| gemini_part_to_assistant(p).ok())
                    .collect();
                Ok(Message::Assistant {
                    id: None,
                    content: assistant_content,
                })
            }
        }
    }
}

fn gemini_part_to_user(part: &GeminiPart) -> Result<UserContent, ProxyError> {
    match &part.content {
        GeminiPartContent::Text { text } => Ok(UserContent::Text {
            text: text.clone(),
            cache_control: None,
        }),
        GeminiPartContent::InlineData { inline_data } => Ok(UserContent::Image {
            data: ImageData {
                source: DataSource::Base64 {
                    data: inline_data.data.clone(),
                },
                media_type: Some(inline_data.mime_type.clone()),
                detail: None,
            },
        }),
        GeminiPartContent::FunctionResponse {
            function_response, ..
        } => {
            // 将 function response 转换为 tool result
            let text = function_response
                .response
                .as_ref()
                .map(|r| r.to_string())
                .unwrap_or_default();
            Ok(UserContent::ToolResult {
                id: function_response.name.clone(),
                content: vec![ToolResultContent::Text { text }],
                is_error: None,
            })
        }
        GeminiPartContent::FunctionCall { .. } => Err(ProxyError::TransformError(
            "FunctionCall not expected in user message".to_string(),
        )),
    }
}

fn gemini_part_to_assistant(part: &GeminiPart) -> Result<AssistantContent, ProxyError> {
    match &part.content {
        GeminiPartContent::Text { text } => {
            // 检查是否为思考内容
            if part.thought == Some(true) {
                Ok(AssistantContent::Reasoning {
                    thinking: text.clone(),
                    signature: part.thought_signature.clone(),
                })
            } else {
                Ok(AssistantContent::Text {
                    text: text.clone(),
                    cache_control: None,
                })
            }
        }
        GeminiPartContent::FunctionCall { function_call } => Ok(AssistantContent::ToolCall {
            id: uuid::Uuid::new_v4().to_string(), // Gemini 没有 ID，生成一个
            name: function_call.name.clone(),
            arguments: function_call.args.clone(),
            signature: part.thought_signature.clone(),
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
        let gemini: GeminiContent = (&unified).try_into().unwrap();
        assert_eq!(gemini.role, Some(GeminiRole::User));
        assert_eq!(gemini.parts.len(), 1);
    }

    #[test]
    fn test_assistant_function_call_conversion() {
        let unified = Message::Assistant {
            id: None,
            content: vec![AssistantContent::tool_call(
                "call_123",
                "get_weather",
                json!({"location": "Tokyo"}),
            )],
        };
        let gemini: GeminiContent = (&unified).try_into().unwrap();
        assert_eq!(gemini.role, Some(GeminiRole::Model));
        if let GeminiPartContent::FunctionCall { function_call } = &gemini.parts[0].content {
            assert_eq!(function_call.name, "get_weather");
        } else {
            panic!("Expected FunctionCall content");
        }
    }

    #[test]
    fn test_reasoning_conversion() {
        let unified = Message::Assistant {
            id: None,
            content: vec![AssistantContent::reasoning_with_signature(
                "Let me analyze...",
                "sig456",
            )],
        };
        let gemini: GeminiContent = (&unified).try_into().unwrap();
        assert_eq!(gemini.parts[0].thought, Some(true));
        assert_eq!(
            gemini.parts[0].thought_signature.as_ref().unwrap(),
            "sig456"
        );
    }

    // ========== 反向转换测试 ==========

    #[test]
    fn test_gemini_to_unified_user_text() {
        let gemini = GeminiContent {
            role: Some(GeminiRole::User),
            parts: vec![GeminiPart {
                thought: Some(false),
                thought_signature: None,
                content: GeminiPartContent::Text {
                    text: "Hello".to_string(),
                },
            }],
        };
        let unified: Message = (&gemini).try_into().unwrap();
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
    fn test_gemini_to_unified_model_function_call() {
        let gemini = GeminiContent {
            role: Some(GeminiRole::Model),
            parts: vec![GeminiPart {
                thought: Some(false),
                thought_signature: None,
                content: GeminiPartContent::FunctionCall {
                    function_call: GeminiFunctionCall {
                        name: "search".to_string(),
                        args: json!({"query": "rust"}),
                    },
                },
            }],
        };
        let unified: Message = (&gemini).try_into().unwrap();
        if let Message::Assistant { content, .. } = unified {
            if let AssistantContent::ToolCall { name, .. } = &content[0] {
                assert_eq!(name, "search");
            } else {
                panic!("Expected ToolCall content");
            }
        } else {
            panic!("Expected Assistant message");
        }
    }

    #[test]
    fn test_gemini_to_unified_thinking() {
        let gemini = GeminiContent {
            role: Some(GeminiRole::Model),
            parts: vec![GeminiPart {
                thought: Some(true),
                thought_signature: Some("sig_xyz".to_string()),
                content: GeminiPartContent::Text {
                    text: "Analyzing the problem...".to_string(),
                },
            }],
        };
        let unified: Message = (&gemini).try_into().unwrap();
        if let Message::Assistant { content, .. } = unified {
            if let AssistantContent::Reasoning { thinking, signature } = &content[0] {
                assert_eq!(thinking, "Analyzing the problem...");
                assert_eq!(signature.as_ref().unwrap(), "sig_xyz");
            } else {
                panic!("Expected Reasoning content");
            }
        } else {
            panic!("Expected Assistant message");
        }
    }

    #[test]
    fn test_gemini_to_unified_inline_data() {
        let gemini = GeminiContent {
            role: Some(GeminiRole::User),
            parts: vec![GeminiPart {
                thought: Some(false),
                thought_signature: None,
                content: GeminiPartContent::InlineData {
                    inline_data: GeminiBlob {
                        mime_type: "image/jpeg".to_string(),
                        data: "base64data".to_string(),
                    },
                },
            }],
        };
        let unified: Message = (&gemini).try_into().unwrap();
        if let Message::User { content } = unified {
            if let UserContent::Image { data } = &content[0] {
                assert_eq!(data.media_type.as_ref().unwrap(), "image/jpeg");
                if let DataSource::Base64 { data: b64 } = &data.source {
                    assert_eq!(b64, "base64data");
                } else {
                    panic!("Expected Base64 source");
                }
            } else {
                panic!("Expected Image content");
            }
        } else {
            panic!("Expected User message");
        }
    }

    #[test]
    fn test_roundtrip_conversion() {
        let original = Message::Assistant {
            id: None,
            content: vec![
                AssistantContent::text("Let me help"),
                AssistantContent::reasoning_with_signature("thinking...", "sig"),
            ],
        };

        let gemini: GeminiContent = (&original).try_into().unwrap();
        let roundtrip: Message = (&gemini).try_into().unwrap();

        if let (Message::Assistant { content: orig, .. }, Message::Assistant { content: rt, .. }) =
            (&original, &roundtrip)
        {
            assert_eq!(orig.len(), rt.len());
        } else {
            panic!("Roundtrip failed");
        }
    }
}
