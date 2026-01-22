//! 统一消息格式模块
//!
//! 借鉴 Rig 项目的类型安全设计，提供跨提供商的统一消息抽象。
//!
//! ## 设计原则
//!
//! 1. **类型安全**: 使用 Rust 枚举和结构体替代动态 JSON
//! 2. **双向转换**: 实现 `TryFrom`/`Into` traits 支持双向转换
//! 3. **保留元数据**: 支持 signature、cache_control 等 Provider 特定字段
//! 4. **流式支持**: 支持 SSE 流式响应的双向转换（借鉴 claude-code-hub）
//!
//! ## 模块结构
//!
//! - `message`: 统一消息类型定义 (Message, UserContent, AssistantContent)
//! - `convert`: 转换 trait 定义 (ToUnified, FromUnified)
//! - `converter`: 多协议转换器 (ProtocolConverter)
//! - `protocol`: 协议配置和功能矩阵
//! - `transform`: 流式响应转换器 (TransformState, SSE 转换)
//! - `tool_mapper`: 工具名称映射器 (缩短/恢复长工具名)
//! - `models`: 各提供商的消息格式和双向转换实现
//!   - `anthropic`: Anthropic (Claude) 格式 - 完整支持 Thinking
//!   - `openai`: OpenAI 格式 - Reasoning 会被忽略
//!   - `openai_responses`: OpenAI Responses API 格式
//!   - `gemini`: Google Gemini 格式 - 支持 thought 字段
//!
//! ## 支持的协议
//!
//! - Anthropic (Claude)
//! - OpenAI (GPT) - Chat Completions API
//! - OpenAI Responses API
//! - Google Gemini
//! - Cohere
//! - DeepSeek
//! - Mistral
//! - Groq
//! - OpenRouter
//! - xAI (Grok)
//! - Ollama
//!
//! ## 双向转换示例
//!
//! ```ignore
//! use crate::proxy::unified::message::*;
//! use crate::proxy::unified::models::anthropic::AnthropicMessage;
//!
//! // 统一格式 → Anthropic
//! let unified = Message::user_text("Hello");
//! let anthropic: AnthropicMessage = (&unified).try_into()?;
//!
//! // Anthropic → 统一格式
//! let back: Message = (&anthropic).try_into()?;
//! ```
//!
//! ## 流式转换示例
//!
//! ```ignore
//! use crate::proxy::unified::transform::*;
//!
//! // Anthropic SSE → OpenAI SSE
//! let mut state = TransformState::new();
//! let openai_chunk = transform_anthropic_sse_to_openai("content_block_delta", &data, &mut state)?;
//!
//! // OpenAI SSE → Anthropic SSE
//! let anthropic_events = transform_openai_sse_to_anthropic(&data, &mut state)?;
//! ```
//!
//! ## 使用方式
//!
//! 此模块作为可选的转换层存在，不影响现有的透传功能。
//! 只有当 Provider 配置了转换模式时才会启用。

pub mod convert;
pub mod converter;
pub mod message;
pub mod models;
pub mod one_or_many;
pub mod protocol;
pub mod tool_mapper;
pub mod transform;

// 公开导出核心类型
pub use converter::ProtocolConverter;
pub use message::{
    AssistantContent, AudioData, AudioMediaType, CacheControl, DataSource, DocumentData,
    DocumentMediaType, ImageData, ImageDetail, ImageMediaType, Message, StopReason, ThinkingConfig,
    ToolDefinition, ToolResultContent, UnifiedRequest, UnifiedResponse, UsageStats, UserContent,
    VideoData, VideoMediaType,
};
pub use one_or_many::{string_or_one_or_many, EmptyListError, OneOrMany};
pub use protocol::{FeatureSupport, ProtocolConfig, ProtocolFormat, TransformMatrix};

// 公开导出转换 traits
pub use convert::{ConvertRequest, ConvertResponse, FromUnified, ToUnified};

// 公开导出流式转换类型
pub use tool_mapper::ToolNameMapper;
pub use transform::{
    transform_anthropic_sse_to_openai, transform_openai_sse_to_anthropic, BlockType,
    ToolCallState, TransformState, UsageState,
};

// 公开导出各 Provider 的消息类型
pub use models::anthropic::{AnthropicContent, AnthropicMessage, AnthropicRole};
pub use models::gemini::{GeminiContent, GeminiPart, GeminiPartContent, GeminiRole};
pub use models::openai::{OpenAIContent, OpenAIMessage, OpenAIToolCall};
