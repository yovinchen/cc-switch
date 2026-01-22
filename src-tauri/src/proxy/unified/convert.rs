//! 转换 Trait 定义
//!
//! 提供统一格式与各提供商格式之间的转换接口
//!
//! ## Trait 层次
//!
//! ```text
//! ToUnified        - 提供商格式 → 统一格式
//! FromUnified      - 统一格式 → 提供商格式
//! ConvertRequest   - 请求体转换（带 Provider 上下文）
//! ConvertResponse  - 响应体转换
//! ```
//!
//! ## 使用示例
//!
//! ```ignore
//! // Anthropic → 统一格式
//! let unified: Message = anthropic_msg.to_unified()?;
//!
//! // 统一格式 → OpenAI
//! let openai: OpenAIMessage = OpenAIMessage::from_unified(&unified)?;
//! ```

use super::message::Message;
use crate::proxy::error::ProxyError;

/// 转换为统一格式
///
/// 这些 trait 为未来扩展预留，目前转换通过 TryFrom 实现
#[allow(dead_code)]
pub trait ToUnified {
    /// 将提供商特定格式转换为统一格式
    fn to_unified(&self) -> Result<Message, ProxyError>;
}

/// 从统一格式转换
#[allow(dead_code)]
pub trait FromUnified {
    /// 从统一格式转换为提供商特定格式
    fn from_unified(msg: &Message) -> Result<Self, ProxyError>
    where
        Self: Sized;
}

/// 请求转换
#[allow(dead_code)]
pub trait ConvertRequest {
    type Target;

    /// 转换请求体
    fn convert(&self, provider: &crate::provider::Provider) -> Result<Self::Target, ProxyError>;
}

/// 响应转换
#[allow(dead_code)]
pub trait ConvertResponse {
    type Target;

    /// 转换响应体
    fn convert(&self) -> Result<Self::Target, ProxyError>;
}
