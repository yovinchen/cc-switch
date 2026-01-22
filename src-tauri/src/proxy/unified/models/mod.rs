//! 提供商特定的消息模型
//!
//! 包含各提供商的消息格式定义和转换实现

pub mod anthropic;
pub mod gemini;
pub mod openai;
pub mod openai_responses;

// 这些类型目前主要在 transform_v2.rs 中使用
// 保留公开导出以便未来扩展
#[allow(unused_imports)]
pub use anthropic::{AnthropicContent, AnthropicMessage, AnthropicRole};
#[allow(unused_imports)]
pub use gemini::{GeminiContent, GeminiPart, GeminiRole};
#[allow(unused_imports)]
pub use openai::{OpenAIMessage, OpenAIUserContent};
#[allow(unused_imports)]
pub use openai_responses::{
    ResponsesApiResponse, ResponsesContentItem, ResponsesInput, ResponsesMessage,
    ResponsesOutputItem, ResponsesRole, ResponsesUsage,
};
