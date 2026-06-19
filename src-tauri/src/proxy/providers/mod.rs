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
pub mod codex_chat_history;
pub mod codex_oauth_auth;
pub mod copilot_auth;
mod gemini;

use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy_core::{
    infer_claude_provider_kind, is_gemini_oauth_key_shape, ProviderAuthStrategy as AuthStrategy,
    ProviderKind,
};

pub use adapter::ProviderAdapter;
pub use claude::{
    get_claude_api_format, normalize_anthropic_messages_for_provider,
    transform_claude_request_for_api_format, ClaudeAdapter,
};
pub use codex::CodexAdapter;
pub use codex::{
    apply_codex_chat_upstream_model, codex_provider_upstream_model,
    resolve_codex_chat_reasoning_options, should_convert_codex_responses_to_chat,
};
pub use gemini::GeminiAdapter;

/// 从 AppType 和 Provider 配置推断供应商类型。
///
/// 供应商类型契约由 `proxy-core::ProviderKind` 维护；host 这里只保留读取
/// desktop Provider 配置并投影到 core kind 的适配逻辑。
#[allow(dead_code)]
pub fn provider_kind_from_app_type_and_config(
    app_type: &AppType,
    provider: &Provider,
) -> ProviderKind {
    match app_type {
        AppType::Claude | AppType::ClaudeDesktop => {
            let adapter = ClaudeAdapter::new();
            let api_format = get_claude_api_format(provider);
            let uses_google_oauth = if api_format == "gemini_native" {
                adapter
                    .extract_auth(provider)
                    .map(|auth| matches!(auth.strategy, AuthStrategy::GoogleOAuth))
                    .unwrap_or(false)
            } else {
                false
            };
            let base_url = adapter.extract_base_url(provider).ok();
            let meta_provider_type = provider
                .meta
                .as_ref()
                .and_then(|meta| meta.provider_type.as_deref());

            infer_claude_provider_kind(
                api_format,
                uses_google_oauth,
                meta_provider_type,
                base_url.as_deref(),
                &provider.settings_config,
            )
        }
        AppType::Codex => ProviderKind::Codex,
        AppType::Gemini => {
            // 检测是否为 CLI 模式（OAuth）
            let adapter = GeminiAdapter::new();
            if let Some(auth) = adapter.extract_auth(provider) {
                let key = &auth.api_key;
                if is_gemini_oauth_key_shape(key) {
                    return ProviderKind::GeminiCli;
                }
            }
            ProviderKind::Gemini
        }
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => {
            // These apps don't support proxy, fallback to Codex-like type
            ProviderKind::Codex
        }
    }
}

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

/// 根据 ProviderKind 获取对应的适配器
#[allow(dead_code)]
pub fn get_adapter_for_provider_type(provider_type: &ProviderKind) -> Box<dyn ProviderAdapter> {
    match provider_type {
        ProviderKind::Claude
        | ProviderKind::ClaudeAuth
        | ProviderKind::OpenRouter
        | ProviderKind::GitHubCopilot
        | ProviderKind::CodexOAuth => Box::new(ClaudeAdapter::new()),
        ProviderKind::Codex => Box::new(CodexAdapter::new()),
        ProviderKind::Gemini | ProviderKind::GeminiCli => Box::new(GeminiAdapter::new()),
        ProviderKind::Custom(_) => Box::new(ClaudeAdapter::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Provider".to_string(),
            settings_config: config,
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn test_provider_type_needs_transform() {
        assert!(!ProviderKind::Claude.needs_transform());
        assert!(!ProviderKind::ClaudeAuth.needs_transform());
        assert!(!ProviderKind::Codex.needs_transform());
        assert!(!ProviderKind::Gemini.needs_transform());
        assert!(!ProviderKind::GeminiCli.needs_transform());
        assert!(!ProviderKind::OpenRouter.needs_transform());
        assert!(ProviderKind::GitHubCopilot.needs_transform());
    }

    #[test]
    fn test_provider_type_default_endpoint() {
        assert_eq!(
            ProviderKind::Claude.default_endpoint(),
            Some("https://api.anthropic.com")
        );
        assert_eq!(
            ProviderKind::ClaudeAuth.default_endpoint(),
            Some("https://api.anthropic.com")
        );
        assert_eq!(
            ProviderKind::Codex.default_endpoint(),
            Some("https://api.openai.com")
        );
        assert_eq!(
            ProviderKind::Gemini.default_endpoint(),
            Some("https://generativelanguage.googleapis.com")
        );
        assert_eq!(
            ProviderKind::GeminiCli.default_endpoint(),
            Some("https://generativelanguage.googleapis.com")
        );
        assert_eq!(
            ProviderKind::OpenRouter.default_endpoint(),
            Some("https://openrouter.ai/api")
        );
        assert_eq!(
            ProviderKind::GitHubCopilot.default_endpoint(),
            Some("https://api.githubcopilot.com")
        );
    }

    #[test]
    fn test_provider_type_from_str() {
        assert_eq!(
            "claude".parse::<ProviderKind>().unwrap(),
            ProviderKind::Claude
        );
        assert_eq!(
            "claude_auth".parse::<ProviderKind>().unwrap(),
            ProviderKind::ClaudeAuth
        );
        assert_eq!(
            "claude-auth".parse::<ProviderKind>().unwrap(),
            ProviderKind::ClaudeAuth
        );
        assert_eq!(
            "codex".parse::<ProviderKind>().unwrap(),
            ProviderKind::Codex
        );
        assert_eq!(
            "gemini".parse::<ProviderKind>().unwrap(),
            ProviderKind::Gemini
        );
        assert_eq!(
            "gemini_cli".parse::<ProviderKind>().unwrap(),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            "gemini-cli".parse::<ProviderKind>().unwrap(),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            "openrouter".parse::<ProviderKind>().unwrap(),
            ProviderKind::OpenRouter
        );
        assert_eq!(
            "github_copilot".parse::<ProviderKind>().unwrap(),
            ProviderKind::GitHubCopilot
        );
        assert_eq!(
            "github-copilot".parse::<ProviderKind>().unwrap(),
            ProviderKind::GitHubCopilot
        );
        assert_eq!(
            "githubcopilot".parse::<ProviderKind>().unwrap(),
            ProviderKind::GitHubCopilot
        );
        assert!("invalid".parse::<ProviderKind>().is_err());
    }

    #[test]
    fn test_provider_type_as_str() {
        assert_eq!(ProviderKind::Claude.as_str(), "claude");
        assert_eq!(ProviderKind::ClaudeAuth.as_str(), "claude_auth");
        assert_eq!(ProviderKind::Codex.as_str(), "codex");
        assert_eq!(ProviderKind::Gemini.as_str(), "gemini");
        assert_eq!(ProviderKind::GeminiCli.as_str(), "gemini_cli");
        assert_eq!(ProviderKind::OpenRouter.as_str(), "openrouter");
        assert_eq!(ProviderKind::GitHubCopilot.as_str(), "github_copilot");
    }

    #[test]
    fn test_provider_type_serde() {
        // Test serialization
        let claude = ProviderKind::Claude;
        let serialized = serde_json::to_string(&claude).unwrap();
        assert_eq!(serialized, "\"claude\"");

        let claude_auth = ProviderKind::ClaudeAuth;
        let serialized = serde_json::to_string(&claude_auth).unwrap();
        assert_eq!(serialized, "\"claude_auth\"");

        // Test deserialization
        let deserialized: ProviderKind = serde_json::from_str("\"claude\"").unwrap();
        assert_eq!(deserialized, ProviderKind::Claude);

        let deserialized: ProviderKind = serde_json::from_str("\"gemini_cli\"").unwrap();
        assert_eq!(deserialized, ProviderKind::GeminiCli);
    }

    #[test]
    fn test_from_app_type_claude_direct() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
            }
        }));

        let provider_type = provider_kind_from_app_type_and_config(&AppType::Claude, &provider);
        assert_eq!(provider_type, ProviderKind::Claude);
    }

    #[test]
    fn test_from_app_type_claude_openrouter() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test"
            }
        }));

        let provider_type = provider_kind_from_app_type_and_config(&AppType::Claude, &provider);
        assert_eq!(provider_type, ProviderKind::OpenRouter);
    }

    #[test]
    fn test_from_app_type_claude_auth() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-test"
            },
            "auth_mode": "bearer_only"
        }));

        let provider_type = provider_kind_from_app_type_and_config(&AppType::Claude, &provider);
        assert_eq!(provider_type, ProviderKind::ClaudeAuth);
    }

    #[test]
    fn test_from_app_type_codex() {
        let provider = create_provider(json!({
            "env": {
                "OPENAI_API_KEY": "sk-test"
            }
        }));

        let provider_type = provider_kind_from_app_type_and_config(&AppType::Codex, &provider);
        assert_eq!(provider_type, ProviderKind::Codex);
    }

    #[test]
    fn test_from_app_type_gemini_api_key() {
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "AIza-test-key"
            }
        }));

        let provider_type = provider_kind_from_app_type_and_config(&AppType::Gemini, &provider);
        assert_eq!(provider_type, ProviderKind::Gemini);
    }

    #[test]
    fn test_from_app_type_gemini_cli_oauth() {
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "ya29.test-access-token"
            }
        }));

        let provider_type = provider_kind_from_app_type_and_config(&AppType::Gemini, &provider);
        assert_eq!(provider_type, ProviderKind::GeminiCli);
    }

    #[test]
    fn test_from_app_type_gemini_cli_json() {
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "{\"access_token\":\"ya29.test\",\"refresh_token\":\"1//test\"}"
            }
        }));

        let provider_type = provider_kind_from_app_type_and_config(&AppType::Gemini, &provider);
        assert_eq!(provider_type, ProviderKind::GeminiCli);
    }

    #[test]
    fn test_get_adapter_for_provider_type() {
        let adapter = get_adapter_for_provider_type(&ProviderKind::Claude);
        assert_eq!(adapter.name(), "Claude");

        let adapter = get_adapter_for_provider_type(&ProviderKind::ClaudeAuth);
        assert_eq!(adapter.name(), "Claude");

        let adapter = get_adapter_for_provider_type(&ProviderKind::OpenRouter);
        assert_eq!(adapter.name(), "Claude");

        let adapter = get_adapter_for_provider_type(&ProviderKind::GitHubCopilot);
        assert_eq!(adapter.name(), "Claude");

        let adapter = get_adapter_for_provider_type(&ProviderKind::Codex);
        assert_eq!(adapter.name(), "Codex");

        let adapter = get_adapter_for_provider_type(&ProviderKind::Gemini);
        assert_eq!(adapter.name(), "Gemini");

        let adapter = get_adapter_for_provider_type(&ProviderKind::GeminiCli);
        assert_eq!(adapter.name(), "Gemini");
    }
}
