//! 统一消息类型定义
//!
//! 借鉴 Rig 项目的类型安全设计，提供跨提供商的统一消息格式。
//! 所有 Provider 格式转换都通过这个中间层进行，保证类型安全和双向转换一致性。
//!
//! ## 设计理念
//!
//! 统一消息格式作为中间层，实现不同 API 格式之间的双向转换：
//!
//! ```text
//! Anthropic ◄──┐
//!              │
//! OpenAI ◄─────┼──► 统一格式 (Message) ◄──► 目标格式
//!              │
//! Gemini ◄─────┘
//! ```
//!
//! ## 设计原则（借鉴 Rig）
//!
//! 1. **类型安全**: 使用 Rust 枚举和结构体替代动态 JSON
//! 2. **双向转换**: 实现 `TryFrom`/`Into` traits 支持双向转换
//! 3. **保留元数据**: 支持 signature、cache_control 等 Provider 特定字段
//! 4. **可扩展性**: 预留 additional_params 字段支持未知字段透传
//!
//! ## 支持的内容类型
//!
//! - 文本 (Text) - 支持 cache_control
//! - 图片 (Image) - Base64 或 URL
//! - 工具调用 (ToolCall) - 支持 signature
//! - 工具结果 (ToolResult)
//! - 推理/思维链 (Reasoning) - Claude/Gemini 支持，带 signature
//! - 文档 (Document) - PDF 等

use serde::{Deserialize, Serialize};
use serde_json::Value;

// 需要 Engine trait 来使用 encode/decode 方法
use base64::Engine;

// ================================================================
// Core Message Types
// ================================================================

/// 统一消息类型
///
/// 作为所有 Provider 格式转换的中间层。
/// 借鉴 Rig 的设计，使用枚举区分不同角色的消息。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    /// 用户消息
    User {
        #[serde(default)]
        content: Vec<UserContent>,
    },
    /// 助手消息
    Assistant {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        content: Vec<AssistantContent>,
    },
    /// 系统消息
    System { content: String },
    /// 工具结果消息 (OpenAI 格式单独处理)
    Tool {
        tool_call_id: String,
        content: String,
    },
}

// ================================================================
// User Content Types
// ================================================================

/// 用户内容类型
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserContent {
    /// 文本内容
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// 图片内容
    Image { data: ImageData },
    /// 音频内容 (OpenAI 支持)
    Audio { data: AudioData },
    /// 视频内容 (预留，未来支持)
    Video { data: VideoData },
    /// 工具结果
    ToolResult {
        id: String,
        #[serde(default)]
        content: Vec<ToolResultContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    /// 文档内容（PDF 等）
    Document {
        data: DocumentData,
        media_type: String,
    },
}

// ================================================================
// Assistant Content Types
// ================================================================

/// 助手内容类型
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantContent {
    /// 文本内容
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// 工具调用
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
        /// 签名 (Gemini 特有)
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// 推理/思维链（Claude/Gemini 支持）
    Reasoning {
        thinking: String,
        /// 签名 (Anthropic/Gemini 特有)
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

// ================================================================
// Content Structures
// ================================================================

/// 缓存控制 (Anthropic 特有)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub cache_type: String,
}

impl CacheControl {
    pub fn ephemeral() -> Self {
        Self {
            cache_type: "ephemeral".to_string(),
        }
    }
}

/// 工具结果内容
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultContent {
    Text { text: String },
    Image { data: ImageData },
}

/// 图片数据
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ImageData {
    pub source: DataSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// 详细程度 (OpenAI 特有)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<ImageDetail>,
}

/// 音频数据
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AudioData {
    pub source: DataSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<AudioMediaType>,
    /// 音频时长（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
}

/// 视频数据
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct VideoData {
    pub source: DataSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<VideoMediaType>,
    /// 视频时长（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
}

/// 图片详细程度 (OpenAI 特有)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail {
    Low,
    High,
    Auto,
}

impl Default for ImageDetail {
    fn default() -> Self {
        Self::Auto
    }
}

/// 图片媒体类型
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Copy)]
pub enum ImageMediaType {
    #[serde(rename = "image/jpeg")]
    JPEG,
    #[serde(rename = "image/png")]
    PNG,
    #[serde(rename = "image/gif")]
    GIF,
    #[serde(rename = "image/webp")]
    WEBP,
    #[serde(rename = "image/svg+xml")]
    SVG,
}

impl ImageMediaType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "image/jpeg" | "jpeg" | "jpg" => Some(Self::JPEG),
            "image/png" | "png" => Some(Self::PNG),
            "image/gif" | "gif" => Some(Self::GIF),
            "image/webp" | "webp" => Some(Self::WEBP),
            "image/svg+xml" | "svg" => Some(Self::SVG),
            _ => None,
        }
    }

    pub fn to_mime_type(&self) -> &'static str {
        match self {
            Self::JPEG => "image/jpeg",
            Self::PNG => "image/png",
            Self::GIF => "image/gif",
            Self::WEBP => "image/webp",
            Self::SVG => "image/svg+xml",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::JPEG => "jpg",
            Self::PNG => "png",
            Self::GIF => "gif",
            Self::WEBP => "webp",
            Self::SVG => "svg",
        }
    }
}

/// 音频媒体类型
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Copy)]
pub enum AudioMediaType {
    #[serde(rename = "audio/mp3", alias = "mp3")]
    MP3,
    #[serde(rename = "audio/wav", alias = "wav")]
    WAV,
    #[serde(rename = "audio/ogg", alias = "ogg")]
    OGG,
    #[serde(rename = "audio/flac", alias = "flac")]
    FLAC,
    #[serde(rename = "audio/webm", alias = "webm")]
    WEBM,
}

impl AudioMediaType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "audio/mp3" | "mp3" | "audio/mpeg" => Some(Self::MP3),
            "audio/wav" | "wav" => Some(Self::WAV),
            "audio/ogg" | "ogg" => Some(Self::OGG),
            "audio/flac" | "flac" => Some(Self::FLAC),
            "audio/webm" | "webm" => Some(Self::WEBM),
            _ => None,
        }
    }

    pub fn to_mime_type(&self) -> &'static str {
        match self {
            Self::MP3 => "audio/mp3",
            Self::WAV => "audio/wav",
            Self::OGG => "audio/ogg",
            Self::FLAC => "audio/flac",
            Self::WEBM => "audio/webm",
        }
    }
}

/// 视频媒体类型
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Copy)]
pub enum VideoMediaType {
    #[serde(rename = "video/mp4", alias = "mp4")]
    MP4,
    #[serde(rename = "video/webm", alias = "webm")]
    WEBM,
    #[serde(rename = "video/quicktime", alias = "mov")]
    MOV,
}

impl VideoMediaType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "video/mp4" | "mp4" => Some(Self::MP4),
            "video/webm" | "webm" => Some(Self::WEBM),
            "video/quicktime" | "mov" => Some(Self::MOV),
            _ => None,
        }
    }

    pub fn to_mime_type(&self) -> &'static str {
        match self {
            Self::MP4 => "video/mp4",
            Self::WEBM => "video/webm",
            Self::MOV => "video/quicktime",
        }
    }
}

/// 文档媒体类型
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Copy)]
pub enum DocumentMediaType {
    #[serde(rename = "application/pdf", alias = "pdf")]
    PDF,
    #[serde(rename = "text/plain", alias = "txt")]
    TXT,
    #[serde(rename = "text/markdown", alias = "md")]
    Markdown,
    #[serde(rename = "application/json", alias = "json")]
    JSON,
}

impl DocumentMediaType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "application/pdf" | "pdf" => Some(Self::PDF),
            "text/plain" | "txt" => Some(Self::TXT),
            "text/markdown" | "md" | "markdown" => Some(Self::Markdown),
            "application/json" | "json" => Some(Self::JSON),
            _ => None,
        }
    }

    pub fn to_mime_type(&self) -> &'static str {
        match self {
            Self::PDF => "application/pdf",
            Self::TXT => "text/plain",
            Self::Markdown => "text/markdown",
            Self::JSON => "application/json",
        }
    }
}

/// 文档数据
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct DocumentData {
    pub source: DataSource,
}

/// 数据源 (扩展版本，借鉴 Rig 的 DocumentSourceKind)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataSource {
    /// Base64 编码数据
    Base64 { data: String },
    /// URL 引用
    Url { url: String },
    /// 原始字节数据
    Raw {
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
    /// 纯文本字符串（用于文档）
    Text { text: String },
    /// 未知/空数据源
    #[default]
    Unknown,
}

/// Base64 字节序列化模块
mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD
            .decode(&s)
            .map_err(|e| serde::de::Error::custom(format!("Invalid base64: {}", e)))
    }
}

impl DataSource {
    /// 创建 URL 数据源
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }

    /// 创建 Base64 数据源
    pub fn base64(data: impl Into<String>) -> Self {
        Self::Base64 { data: data.into() }
    }

    /// 创建原始字节数据源
    pub fn raw(bytes: impl Into<Vec<u8>>) -> Self {
        Self::Raw {
            bytes: bytes.into(),
        }
    }

    /// 创建文本数据源
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// 检查是否为空/未知
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// 转换为 data URL 格式
    pub fn to_data_url(&self, media_type: &str) -> String {
        match self {
            Self::Base64 { data } => format!("data:{};base64,{}", media_type, data),
            Self::Url { url } => url.clone(),
            Self::Raw { bytes } => {
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                format!("data:{};base64,{}", media_type, encoded)
            }
            Self::Text { text } => text.clone(),
            Self::Unknown => String::new(),
        }
    }

    /// 从 data URL 解析
    pub fn from_data_url(url: &str) -> Option<(Self, String)> {
        if url.starts_with("data:") {
            let rest = &url[5..];
            let parts: Vec<&str> = rest.splitn(2, ',').collect();
            if parts.len() == 2 {
                let meta = parts[0];
                let data = parts[1];
                let media_type = meta.split(';').next().unwrap_or("image/png");
                return Some((
                    Self::Base64 {
                        data: data.to_string(),
                    },
                    media_type.to_string(),
                ));
            }
        }
        None
    }

    /// 尝试获取内部字符串值
    pub fn try_into_inner(self) -> Option<String> {
        match self {
            Self::Url { url } => Some(url),
            Self::Base64 { data } => Some(data),
            Self::Text { text } => Some(text),
            Self::Raw { bytes } => String::from_utf8(bytes).ok(),
            Self::Unknown => None,
        }
    }

    /// 获取内部字符串值的引用
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Url { url } => Some(url.as_str()),
            Self::Base64 { data } => Some(data.as_str()),
            Self::Text { text } => Some(text.as_str()),
            _ => None,
        }
    }
}

// ================================================================
// Request/Response Structures
// ================================================================

/// 统一请求格式
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UnifiedRequest {
    /// 模型名称
    pub model: String,
    /// 消息列表
    pub messages: Vec<Message>,
    /// 系统提示 (可选，某些 Provider 支持独立 system 字段)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// 最大输出 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// 温度参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-P 参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 是否流式输出
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// 停止序列
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// 工具定义
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    /// 工具选择策略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    /// 思考模式配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
}

/// 工具定义
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// 思考模式配置
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub thinking_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

impl ThinkingConfig {
    pub fn enabled() -> Self {
        Self {
            thinking_type: "enabled".to_string(),
            budget_tokens: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.thinking_type == "enabled"
    }
}

/// 统一响应格式
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UnifiedResponse {
    /// 响应 ID
    pub id: String,
    /// 响应类型
    #[serde(rename = "type")]
    pub response_type: String,
    /// 角色
    pub role: String,
    /// 内容
    pub content: Vec<AssistantContent>,
    /// 模型名称
    pub model: String,
    /// 停止原因
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    /// 使用统计
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageStats>,
}

/// 停止原因
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    StopSequence,
    #[serde(other)]
    Other,
}

impl StopReason {
    /// 从 OpenAI finish_reason 转换
    pub fn from_openai(reason: &str) -> Self {
        match reason {
            "stop" => Self::EndTurn,
            "length" => Self::MaxTokens,
            "tool_calls" => Self::ToolUse,
            _ => Self::Other,
        }
    }

    /// 转换为 OpenAI finish_reason
    pub fn to_openai(&self) -> &'static str {
        match self {
            Self::EndTurn => "stop",
            Self::MaxTokens => "length",
            Self::ToolUse => "tool_calls",
            Self::StopSequence => "stop",
            Self::Other => "stop",
        }
    }

    /// 从 Anthropic stop_reason 转换
    pub fn from_anthropic(reason: &str) -> Self {
        match reason {
            "end_turn" => Self::EndTurn,
            "max_tokens" => Self::MaxTokens,
            "tool_use" => Self::ToolUse,
            "stop_sequence" => Self::StopSequence,
            _ => Self::Other,
        }
    }

    /// 转换为 Anthropic stop_reason
    pub fn to_anthropic(&self) -> &'static str {
        match self {
            Self::EndTurn => "end_turn",
            Self::MaxTokens => "max_tokens",
            Self::ToolUse => "tool_use",
            Self::StopSequence => "stop_sequence",
            Self::Other => "end_turn",
        }
    }

    /// 从 Gemini finishReason 转换
    pub fn from_gemini(reason: &str) -> Self {
        match reason {
            "STOP" => Self::EndTurn,
            "MAX_TOKENS" => Self::MaxTokens,
            "SAFETY" => Self::EndTurn,
            _ => Self::Other,
        }
    }
}

/// 使用统计
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageStats {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
}

// ================================================================
// Helper Implementations
// ================================================================

impl Message {
    /// 创建用户文本消息
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::User {
            content: vec![UserContent::Text {
                text: text.into(),
                cache_control: None,
            }],
        }
    }

    /// 创建助手文本消息
    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::Assistant {
            id: None,
            content: vec![AssistantContent::Text {
                text: text.into(),
                cache_control: None,
            }],
        }
    }

    /// 创建系统消息
    pub fn system(text: impl Into<String>) -> Self {
        Self::System {
            content: text.into(),
        }
    }

    /// 创建工具结果消息
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Tool {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
        }
    }
}

impl UserContent {
    /// 创建文本内容
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            cache_control: None,
        }
    }

    /// 创建带缓存控制的文本内容
    pub fn text_with_cache(text: impl Into<String>, cache: CacheControl) -> Self {
        Self::Text {
            text: text.into(),
            cache_control: Some(cache),
        }
    }

    /// 创建图片内容 (base64)
    pub fn image_base64(media_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self::Image {
            data: ImageData {
                source: DataSource::Base64 { data: data.into() },
                media_type: Some(media_type.into()),
                detail: None,
            },
        }
    }

    /// 创建图片内容 (URL)
    pub fn image_url(url: impl Into<String>) -> Self {
        Self::Image {
            data: ImageData {
                source: DataSource::Url { url: url.into() },
                media_type: None,
                detail: None,
            },
        }
    }

    /// 创建音频内容 (base64)
    pub fn audio_base64(data: impl Into<String>, media_type: AudioMediaType) -> Self {
        Self::Audio {
            data: AudioData {
                source: DataSource::Base64 { data: data.into() },
                media_type: Some(media_type),
                duration: None,
            },
        }
    }

    /// 创建音频内容 (URL)
    pub fn audio_url(url: impl Into<String>) -> Self {
        Self::Audio {
            data: AudioData {
                source: DataSource::Url { url: url.into() },
                media_type: None,
                duration: None,
            },
        }
    }

    /// 创建视频内容 (base64)
    pub fn video_base64(data: impl Into<String>, media_type: VideoMediaType) -> Self {
        Self::Video {
            data: VideoData {
                source: DataSource::Base64 { data: data.into() },
                media_type: Some(media_type),
                duration: None,
            },
        }
    }

    /// 创建视频内容 (URL)
    pub fn video_url(url: impl Into<String>) -> Self {
        Self::Video {
            data: VideoData {
                source: DataSource::Url { url: url.into() },
                media_type: None,
                duration: None,
            },
        }
    }

    /// 创建工具结果
    pub fn tool_result_text(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::ToolResult {
            id: id.into(),
            content: vec![ToolResultContent::Text { text: text.into() }],
            is_error: None,
        }
    }
}

impl AssistantContent {
    /// 创建文本内容
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            cache_control: None,
        }
    }

    /// 创建工具调用
    pub fn tool_call(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self::ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
            signature: None,
        }
    }

    /// 创建带签名的工具调用
    pub fn tool_call_with_signature(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: Value,
        signature: impl Into<String>,
    ) -> Self {
        Self::ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
            signature: Some(signature.into()),
        }
    }

    /// 创建思考内容
    pub fn reasoning(thinking: impl Into<String>) -> Self {
        Self::Reasoning {
            thinking: thinking.into(),
            signature: None,
        }
    }

    /// 创建带签名的思考内容
    pub fn reasoning_with_signature(
        thinking: impl Into<String>,
        signature: impl Into<String>,
    ) -> Self {
        Self::Reasoning {
            thinking: thinking.into(),
            signature: Some(signature.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_message_user_text() {
        let msg = Message::user_text("Hello");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "Hello");
    }

    #[test]
    fn test_message_assistant_tool_call() {
        let msg = Message::Assistant {
            id: Some("msg_123".to_string()),
            content: vec![AssistantContent::tool_call(
                "call_123",
                "get_weather",
                json!({"location": "Tokyo"}),
            )],
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"][0]["type"], "tool_call");
        assert_eq!(json["content"][0]["id"], "call_123");
    }

    #[test]
    fn test_message_reasoning() {
        let msg = Message::Assistant {
            id: None,
            content: vec![AssistantContent::reasoning_with_signature(
                "Let me think...",
                "sig123",
            )],
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["content"][0]["type"], "reasoning");
        assert_eq!(json["content"][0]["thinking"], "Let me think...");
        assert_eq!(json["content"][0]["signature"], "sig123");
    }

    #[test]
    fn test_message_tool_result() {
        let msg = Message::Tool {
            tool_call_id: "call_123".to_string(),
            content: "Result content".to_string(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_123");
    }

    #[test]
    fn test_data_source_data_url() {
        let source = DataSource::Base64 {
            data: "iVBORw0KGgo=".to_string(),
        };
        let url = source.to_data_url("image/png");
        assert_eq!(url, "data:image/png;base64,iVBORw0KGgo=");

        let (parsed_source, media_type) = DataSource::from_data_url(&url).unwrap();
        assert_eq!(media_type, "image/png");
        if let DataSource::Base64 { data } = parsed_source {
            assert_eq!(data, "iVBORw0KGgo=");
        } else {
            panic!("Expected Base64 source");
        }
    }

    #[test]
    fn test_stop_reason_conversions() {
        // OpenAI
        assert_eq!(StopReason::from_openai("stop"), StopReason::EndTurn);
        assert_eq!(StopReason::from_openai("length"), StopReason::MaxTokens);
        assert_eq!(StopReason::from_openai("tool_calls"), StopReason::ToolUse);
        assert_eq!(StopReason::EndTurn.to_openai(), "stop");
        assert_eq!(StopReason::MaxTokens.to_openai(), "length");

        // Anthropic
        assert_eq!(StopReason::from_anthropic("end_turn"), StopReason::EndTurn);
        assert_eq!(StopReason::from_anthropic("max_tokens"), StopReason::MaxTokens);
        assert_eq!(StopReason::EndTurn.to_anthropic(), "end_turn");

        // Gemini
        assert_eq!(StopReason::from_gemini("STOP"), StopReason::EndTurn);
        assert_eq!(StopReason::from_gemini("MAX_TOKENS"), StopReason::MaxTokens);
    }

    #[test]
    fn test_thinking_config() {
        let config = ThinkingConfig::enabled();
        assert!(config.is_enabled());
        assert_eq!(config.thinking_type, "enabled");
    }

    #[test]
    fn test_cache_control() {
        let cache = CacheControl::ephemeral();
        let json = serde_json::to_value(&cache).unwrap();
        assert_eq!(json["type"], "ephemeral");
    }

    #[test]
    fn test_user_content_helpers() {
        let text = UserContent::text("Hello");
        if let UserContent::Text { text: t, cache_control } = text {
            assert_eq!(t, "Hello");
            assert!(cache_control.is_none());
        } else {
            panic!("Expected Text");
        }

        let cached = UserContent::text_with_cache("Hello", CacheControl::ephemeral());
        if let UserContent::Text { cache_control, .. } = cached {
            assert!(cache_control.is_some());
        } else {
            panic!("Expected Text with cache");
        }
    }
}
