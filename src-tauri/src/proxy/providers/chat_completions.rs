//! ChatCompletions 适配器
//!
//! 专门用于格式转换的适配器，支持多协议互转：
//! - Anthropic ↔ OpenAI ↔ Gemini ↔ Cohere ↔ DeepSeek 等
//!
//! ## 使用场景
//!
//! 当 Provider 配置了 `chat_completions_mode: true` 时，会使用此适配器
//! 根据 `source_format` 和 `target_format` 配置进行协议转换
//!
//! ## 配置示例
//!
//! ### 基础配置（Anthropic → OpenAI）
//! ```json
//! {
//!     "settings_config": {
//!         "env": {
//!             "ANTHROPIC_BASE_URL": "https://api.openai.com",
//!             "ANTHROPIC_AUTH_TOKEN": "sk-xxx"
//!         },
//!         "chat_completions_mode": true
//!     }
//! }
//! ```
//!
//! ### 自定义协议转换（Anthropic → Gemini）
//! ```json
//! {
//!     "settings_config": {
//!         "env": {
//!             "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
//!             "GEMINI_API_KEY": "AIza-xxx"
//!         },
//!         "chat_completions_mode": true,
//!         "source_format": "anthropic",
//!         "target_format": "gemini"
//!     }
//! }
//! ```
//!
//! ## 支持的协议格式
//!
//! - `anthropic` / `claude`: Anthropic Claude API
//! - `openai` / `gpt`: OpenAI Chat Completions API
//! - `gemini` / `google`: Google Gemini API
//! - `cohere`: Cohere API
//! - `deepseek`: DeepSeek API (OpenAI 兼容)
//! - `mistral`: Mistral API (OpenAI 兼容)
//! - `groq`: Groq API (OpenAI 兼容)
//! - `xai` / `grok`: xAI Grok API (OpenAI 兼容)
//! - `ollama`: Ollama 本地模型 (OpenAI 兼容)
//! - `openrouter`: OpenRouter API

use super::{
    adapter::ProviderAdapter,
    auth::{AuthInfo, AuthStrategy},
};
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy::unified::{
    converter::ProtocolConverter,
    protocol::{ProtocolConfig, ProtocolFormat},
};
use reqwest::RequestBuilder;
use serde_json::Value;

/// ChatCompletions 适配器
///
/// 此适配器专门用于格式转换场景，支持多协议互转
/// 使用统一消息格式作为中间层实现跨协议转换
pub struct ChatCompletionsAdapter;

impl ChatCompletionsAdapter {
    pub fn new() -> Self {
        Self
    }

    /// 从 Provider 配置中解析协议配置
    ///
    /// 支持的配置字段：
    /// - `source_format`: 源协议格式（默认 "anthropic"）
    /// - `target_format`: 目标协议格式（默认 "openai"）
    /// - `preserve_reasoning`: 是否保留思维链（默认 true）
    /// - `transform_tools`: 是否转换工具调用（默认 true）
    pub fn get_protocol_config(&self, provider: &Provider) -> ProtocolConfig {
        let settings = &provider.settings_config;

        // 解析源格式
        let source_format = settings
            .get("source_format")
            .and_then(|v| v.as_str())
            .and_then(ProtocolFormat::from_str)
            .unwrap_or(ProtocolFormat::Anthropic);

        // 解析目标格式
        let target_format = settings
            .get("target_format")
            .and_then(|v| v.as_str())
            .and_then(ProtocolFormat::from_str)
            .unwrap_or(ProtocolFormat::OpenAIChat);

        // 解析其他选项
        let preserve_reasoning = settings
            .get("preserve_reasoning")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let transform_tools = settings
            .get("transform_tools")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        ProtocolConfig {
            source_format,
            target_format,
            transform_enabled: source_format != target_format,
            model_mapping: std::collections::HashMap::new(),
            preserve_reasoning,
            transform_tools,
        }
    }

    /// 根据目标格式获取认证策略
    fn get_auth_strategy(&self, target_format: ProtocolFormat) -> AuthStrategy {
        match target_format {
            ProtocolFormat::Anthropic => AuthStrategy::Anthropic,
            ProtocolFormat::Gemini => AuthStrategy::Google,
            _ => AuthStrategy::Bearer,
        }
    }

    /// 根据目标格式获取默认端点
    fn get_default_endpoint(&self, target_format: ProtocolFormat) -> &'static str {
        target_format.default_endpoint()
    }
}

impl Default for ChatCompletionsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAdapter for ChatCompletionsAdapter {
    fn name(&self) -> &'static str {
        "ChatCompletions"
    }

    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError> {
        // 从 env 中提取 base_url
        let env = provider
            .settings_config
            .get("env")
            .ok_or_else(|| ProxyError::ConfigError("Missing env config".to_string()))?;

        // 尝试多个可能的 key
        let base_url = env
            .get("ANTHROPIC_BASE_URL")
            .or_else(|| env.get("OPENAI_BASE_URL"))
            .or_else(|| env.get("GEMINI_BASE_URL"))
            .or_else(|| env.get("BASE_URL"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProxyError::ConfigError("Missing base_url in env".to_string()))?;

        Ok(base_url.trim_end_matches('/').to_string())
    }

    fn extract_auth(&self, provider: &Provider) -> Option<AuthInfo> {
        let env = provider.settings_config.get("env")?;
        let config = self.get_protocol_config(provider);

        // 尝试多个可能的认证 key
        let api_key = env
            .get("ANTHROPIC_AUTH_TOKEN")
            .or_else(|| env.get("ANTHROPIC_API_KEY"))
            .or_else(|| env.get("OPENAI_API_KEY"))
            .or_else(|| env.get("GEMINI_API_KEY"))
            .or_else(|| env.get("OPENROUTER_API_KEY"))
            .or_else(|| env.get("COHERE_API_KEY"))
            .or_else(|| env.get("DEEPSEEK_API_KEY"))
            .or_else(|| env.get("MISTRAL_API_KEY"))
            .or_else(|| env.get("GROQ_API_KEY"))
            .or_else(|| env.get("XAI_API_KEY"))
            .or_else(|| env.get("API_KEY"))
            .and_then(|v| v.as_str())?;

        Some(AuthInfo {
            api_key: api_key.to_string(),
            strategy: self.get_auth_strategy(config.target_format),
            access_token: None,
        })
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        format!("{}{}", base_url.trim_end_matches('/'), endpoint)
    }

    fn add_auth_headers(&self, request: RequestBuilder, auth: &AuthInfo) -> RequestBuilder {
        match auth.strategy {
            AuthStrategy::Bearer | AuthStrategy::ClaudeAuth => {
                request.header("authorization", format!("Bearer {}", auth.api_key))
            }
            AuthStrategy::Anthropic => request
                .header("x-api-key", &auth.api_key)
                .header("anthropic-version", "2023-06-01"),
            AuthStrategy::Google => request.header("x-goog-api-key", &auth.api_key),
            AuthStrategy::GoogleOAuth => {
                if let Some(access_token) = &auth.access_token {
                    request.header("authorization", format!("Bearer {}", access_token))
                } else {
                    request.header("authorization", format!("Bearer {}", auth.api_key))
                }
            }
        }
    }

    fn needs_transform(&self, provider: &Provider) -> bool {
        let config = self.get_protocol_config(provider);
        config.needs_transform()
    }

    fn transform_request(&self, body: Value, provider: &Provider) -> Result<Value, ProxyError> {
        let config = self.get_protocol_config(provider);

        log::debug!(
            "[ChatCompletions] 转换请求: {} -> {}",
            config.source_format,
            config.target_format
        );

        // 使用 ProtocolConverter 进行多协议转换
        ProtocolConverter::convert_request(body, &config, provider)
    }

    fn transform_response(&self, body: Value) -> Result<Value, ProxyError> {
        // 注意：响应转换需要知道原始配置
        // 这里使用默认配置（OpenAI → Anthropic）
        // 实际使用时应该从上下文获取配置
        let config = ProtocolConfig::transform(ProtocolFormat::OpenAIChat, ProtocolFormat::Anthropic);

        log::debug!(
            "[ChatCompletions] 转换响应: {} -> {}",
            config.source_format,
            config.target_format
        );

        ProtocolConverter::convert_response(body, &config)
    }

    fn transform_response_with_provider(
        &self,
        body: Value,
        provider: &Provider,
    ) -> Result<Value, ProxyError> {
        let config = self.get_protocol_config(provider);

        // 响应转换是请求转换的逆过程
        let response_config = ProtocolConfig::transform(config.target_format, config.source_format);

        log::debug!(
            "[ChatCompletions] 转换响应: {} -> {}",
            response_config.source_format,
            response_config.target_format
        );

        ProtocolConverter::convert_response(body, &response_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_provider() -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Provider".to_string(),
            settings_config: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-test-123"
                }
            }),
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

    fn create_provider_with_protocol(source: &str, target: &str) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Provider".to_string(),
            settings_config: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-test-123"
                },
                "source_format": source,
                "target_format": target
            }),
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
    fn test_adapter_name() {
        let adapter = ChatCompletionsAdapter::new();
        assert_eq!(adapter.name(), "ChatCompletions");
    }

    #[test]
    fn test_needs_transform_default() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_test_provider();
        // 默认 anthropic -> openai，需要转换
        assert!(adapter.needs_transform(&provider));
    }

    #[test]
    fn test_needs_transform_same_format() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_provider_with_protocol("anthropic", "anthropic");
        // 相同格式，不需要转换
        assert!(!adapter.needs_transform(&provider));
    }

    #[test]
    fn test_extract_base_url() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_test_provider();
        let base_url = adapter.extract_base_url(&provider).unwrap();
        assert_eq!(base_url, "https://api.example.com");
    }

    #[test]
    fn test_extract_auth() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_test_provider();
        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-test-123");
        assert!(matches!(auth.strategy, AuthStrategy::Bearer));
    }

    #[test]
    fn test_extract_auth_gemini_target() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_provider_with_protocol("anthropic", "gemini");
        let auth = adapter.extract_auth(&provider).unwrap();
        assert!(matches!(auth.strategy, AuthStrategy::Google));
    }

    #[test]
    fn test_extract_auth_anthropic_target() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_provider_with_protocol("openai", "anthropic");
        let auth = adapter.extract_auth(&provider).unwrap();
        assert!(matches!(auth.strategy, AuthStrategy::Anthropic));
    }

    #[test]
    fn test_build_url() {
        let adapter = ChatCompletionsAdapter::new();
        let url = adapter.build_url("https://api.example.com", "/v1/messages");
        assert_eq!(url, "https://api.example.com/v1/messages");
    }

    #[test]
    fn test_protocol_config_default() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_test_provider();
        let config = adapter.get_protocol_config(&provider);

        assert_eq!(config.source_format, ProtocolFormat::Anthropic);
        assert_eq!(config.target_format, ProtocolFormat::OpenAIChat);
        assert!(config.transform_enabled);
        assert!(config.preserve_reasoning);
        assert!(config.transform_tools);
    }

    #[test]
    fn test_protocol_config_custom() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_provider_with_protocol("anthropic", "gemini");
        let config = adapter.get_protocol_config(&provider);

        assert_eq!(config.source_format, ProtocolFormat::Anthropic);
        assert_eq!(config.target_format, ProtocolFormat::Gemini);
        assert!(config.transform_enabled);
    }

    #[test]
    fn test_protocol_config_openai_to_anthropic() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_provider_with_protocol("openai", "anthropic");
        let config = adapter.get_protocol_config(&provider);

        assert_eq!(config.source_format, ProtocolFormat::OpenAIChat);
        assert_eq!(config.target_format, ProtocolFormat::Anthropic);
        assert!(config.transform_enabled);
    }

    #[test]
    fn test_protocol_config_gemini_to_openai() {
        let adapter = ChatCompletionsAdapter::new();
        let provider = create_provider_with_protocol("gemini", "openai");
        let config = adapter.get_protocol_config(&provider);

        assert_eq!(config.source_format, ProtocolFormat::Gemini);
        assert_eq!(config.target_format, ProtocolFormat::OpenAIChat);
        assert!(config.transform_enabled);
    }

    #[test]
    fn test_protocol_config_aliases() {
        let adapter = ChatCompletionsAdapter::new();

        // claude -> anthropic
        let provider = create_provider_with_protocol("claude", "gpt");
        let config = adapter.get_protocol_config(&provider);
        assert_eq!(config.source_format, ProtocolFormat::Anthropic);
        assert_eq!(config.target_format, ProtocolFormat::OpenAIChat);

        // google -> gemini
        let provider = create_provider_with_protocol("google", "claude");
        let config = adapter.get_protocol_config(&provider);
        assert_eq!(config.source_format, ProtocolFormat::Gemini);
        assert_eq!(config.target_format, ProtocolFormat::Anthropic);
    }

    #[test]
    fn test_get_default_endpoint() {
        let adapter = ChatCompletionsAdapter::new();

        assert_eq!(
            adapter.get_default_endpoint(ProtocolFormat::Anthropic),
            "/v1/messages"
        );
        assert_eq!(
            adapter.get_default_endpoint(ProtocolFormat::OpenAIChat),
            "/v1/chat/completions"
        );
        assert_eq!(
            adapter.get_default_endpoint(ProtocolFormat::Gemini),
            "/v1beta/models"
        );
    }
}
