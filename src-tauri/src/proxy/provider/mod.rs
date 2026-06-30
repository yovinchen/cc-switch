//! Provider Adapters Module
//!
//! 供应商适配器模块，提供统一的接口抽象不同上游供应商的处理逻辑。
//!
//! ## 模块结构
//! - `adapter`: 定义 `ProviderAdapter` trait
//! - `claude`: Claude (Anthropic) 适配器
//! - `codex`: Codex (OpenAI) 适配器
//! - `gemini`: Gemini (Google) 适配器

mod adapter;
mod claude;
mod codex;
mod gemini;

use crate::app_config::AppType;
use crate::proxy_core::api::domain::{
    provider_adapter_kind_for_app, AppKind, AppProviderAdapterKind,
};

pub use adapter::ProviderAdapter;
pub(crate) use claude::claude_provider_api_format;
pub use claude::ClaudeAdapter;
pub use codex::CodexAdapter;
pub(crate) use codex::{
    codex_provider_apply_chat_upstream_model, codex_provider_chat_reasoning_options,
    codex_provider_should_convert_responses_to_chat, codex_provider_upstream_model,
};
#[cfg(test)]
pub(crate) use codex::{
    codex_provider_chat_reasoning_profile, codex_provider_uses_chat_completions,
};
pub use gemini::GeminiAdapter;

/// 根据 AppType 获取对应的适配器
pub fn get_adapter(app_type: &AppType) -> Box<dyn ProviderAdapter> {
    match provider_adapter_kind_for_app(&AppKind::from(app_type.as_str())) {
        AppProviderAdapterKind::Claude => Box::new(ClaudeAdapter::new()),
        AppProviderAdapterKind::Codex => Box::new(CodexAdapter::new()),
        AppProviderAdapterKind::Gemini => Box::new(GeminiAdapter::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_adapter_selects_host_adapter_by_app_type() {
        assert_eq!(get_adapter(&AppType::Claude).name(), "Claude");
        assert_eq!(get_adapter(&AppType::ClaudeDesktop).name(), "Claude");
        assert_eq!(get_adapter(&AppType::Codex).name(), "Codex");
        assert_eq!(get_adapter(&AppType::Gemini).name(), "Gemini");
        assert_eq!(get_adapter(&AppType::OpenCode).name(), "Codex");
        assert_eq!(get_adapter(&AppType::OpenClaw).name(), "Codex");
        assert_eq!(get_adapter(&AppType::Hermes).name(), "Codex");
    }
}
