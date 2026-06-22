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

pub use adapter::ProviderAdapter;
pub use claude::ClaudeAdapter;
pub use codex::CodexAdapter;
pub use gemini::GeminiAdapter;

/// 根据 AppType 获取对应的适配器
pub fn get_adapter(app_type: &AppType) -> Box<dyn ProviderAdapter> {
    match app_type {
        AppType::Claude | AppType::ClaudeDesktop => Box::new(ClaudeAdapter::new()),
        AppType::Codex => Box::new(CodexAdapter::new()),
        AppType::Gemini => Box::new(GeminiAdapter::new()),
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => {
            // These apps don't support proxy, fallback to Codex adapter
            Box::new(CodexAdapter::new())
        }
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
