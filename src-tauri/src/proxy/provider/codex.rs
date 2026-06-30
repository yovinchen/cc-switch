//! Codex (OpenAI) Provider Adapter
//!
//! 仅透传模式，支持直连 OpenAI API
//!
//! ## 客户端检测
//! 支持检测官方 Codex 客户端 (codex_vscode, codex_cli_rs)

use super::ProviderAdapter;
use crate::provider::{CodexChatReasoningConfig, Provider};
use crate::proxy::error::ProxyError;
use crate::proxy_core::api::auth::codex_auth_info_from_api_key;
use crate::proxy_core::api::auth::ProviderAuthInfo;
use crate::proxy_core::api::ports::codex_base_url_from_settings;
use crate::proxy_core::api::ports::codex_config_text_from_settings;
use crate::proxy_core::api::ports::codex_model_from_config_toml;
use crate::proxy_core::api::ports::codex_wire_api_from_config_toml;
use crate::proxy_core::api::ports::required_provider_base_url;
use crate::proxy_core::api::transforms::infer_codex_chat_reasoning_profile;
use crate::proxy_core::api::transforms::normalize_codex_chat_reasoning_profile;
use crate::proxy_core::api::transforms::CodexChatReasoningOptions;
use crate::proxy_core::api::transforms::CodexChatReasoningProfile;
use crate::proxy_core::api::transport::apply_codex_chat_upstream_model_policy;
use crate::proxy_core::api::transport::build_codex_provider_auth_headers;
use crate::proxy_core::api::transport::build_codex_upstream_url;
use crate::proxy_core::api::transport::codex_provider_catalog_model_ids_from_settings;
use crate::proxy_core::api::transport::codex_provider_uses_chat_completions as core_codex_provider_uses_chat_completions;
use crate::proxy_core::api::transport::codex_responses_to_chat_conversion_required as core_codex_responses_to_chat_conversion_required;
use crate::proxy_core::api::transport::resolve_codex_provider_upstream_model;
use crate::proxy_core::api::transport::CodexProviderChatCompletionsFacts;
use crate::proxy_core::api::transport::CodexResponsesToChatConversionFacts;
use serde_json::Value;
use std::collections::HashSet;

/// Codex 适配器
pub struct CodexAdapter;

impl CodexAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn codex_provider_api_key(provider: &Provider) -> Option<String> {
    if let Some(env) = provider.settings_config.get("env") {
        if let Some(key) = env
            .get("OPENAI_API_KEY")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            return Some(key.to_string());
        }
    }

    if let Some(auth) = provider.settings_config.get("auth") {
        if let Some(key) = crate::codex_config::extract_codex_auth_api_key(auth) {
            return Some(key.to_string());
        }
    }

    if let Some(key) = provider
        .settings_config
        .get("apiKey")
        .or_else(|| provider.settings_config.get("api_key"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        return Some(key.to_string());
    }

    if let Some(config) = provider.settings_config.get("config") {
        if let Some(key) = config
            .get("api_key")
            .or_else(|| config.get("apiKey"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            return Some(key.to_string());
        }

        if let Some(config_str) = config.as_str() {
            if let Some(key) =
                crate::codex_config::extract_codex_experimental_bearer_token(config_str)
            {
                return Some(key);
            }
        }
    }

    None
}

fn codex_provider_auth_info(provider: &Provider) -> Option<ProviderAuthInfo> {
    codex_provider_api_key(provider).map(codex_auth_info_from_api_key)
}

fn required_codex_provider_base_url(provider: &Provider) -> Result<String, String> {
    required_provider_base_url(
        "Codex",
        codex_base_url_from_settings(&provider.settings_config),
    )
}

fn codex_provider_auth_headers(
    auth: &ProviderAuthInfo,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, String> {
    build_codex_provider_auth_headers(auth).map_err(|error| error.to_string())
}

fn codex_provider_config_text(provider: &Provider) -> Option<&str> {
    codex_config_text_from_settings(&provider.settings_config)
}

fn with_codex_provider_chat_completions_facts<T>(
    provider: &Provider,
    evaluate: impl FnOnce(CodexProviderChatCompletionsFacts<'_>) -> T,
) -> T {
    let config_text = codex_provider_config_text(provider);
    let wire_api = config_text.and_then(codex_wire_api_from_config_toml);
    let config_base_url = config_text.and_then(crate::codex_config::extract_codex_base_url);

    evaluate(CodexProviderChatCompletionsFacts {
        api_format: provider
            .meta
            .as_ref()
            .and_then(|meta| meta.api_format.as_deref())
            .or_else(|| {
                provider
                    .settings_config
                    .get("api_format")
                    .and_then(Value::as_str)
            })
            .or_else(|| {
                provider
                    .settings_config
                    .get("apiFormat")
                    .and_then(Value::as_str)
            }),
        wire_api: wire_api.as_deref(),
        base_url: provider
            .settings_config
            .get("base_url")
            .or_else(|| provider.settings_config.get("baseURL"))
            .and_then(Value::as_str),
        config_base_url: config_base_url.as_deref(),
    })
}

pub(crate) fn codex_provider_uses_chat_completions(provider: &Provider) -> bool {
    with_codex_provider_chat_completions_facts(provider, core_codex_provider_uses_chat_completions)
}

pub(crate) fn codex_provider_should_convert_responses_to_chat(
    provider: &Provider,
    endpoint: &str,
) -> bool {
    with_codex_provider_chat_completions_facts(provider, |provider_facts| {
        core_codex_responses_to_chat_conversion_required(CodexResponsesToChatConversionFacts {
            provider: provider_facts,
            endpoint,
        })
    })
}

pub(crate) fn codex_provider_upstream_model(provider: &Provider) -> Option<String> {
    let settings_model = provider
        .settings_config
        .get("model")
        .and_then(Value::as_str);
    let config_model = codex_provider_config_text(provider).and_then(codex_model_from_config_toml);
    resolve_codex_provider_upstream_model(settings_model, config_model.as_deref())
}

fn codex_provider_catalog_model_ids(provider: &Provider) -> HashSet<String> {
    codex_provider_catalog_model_ids_from_settings(&provider.settings_config)
}

fn codex_chat_reasoning_profile_from_config(
    config: CodexChatReasoningConfig,
) -> CodexChatReasoningProfile {
    CodexChatReasoningProfile {
        supports_thinking: config.supports_thinking,
        supports_effort: config.supports_effort,
        thinking_param: config.thinking_param,
        effort_param: config.effort_param,
        effort_value_mode: config.effort_value_mode,
        output_format: config.output_format,
    }
}

pub(crate) fn codex_provider_chat_reasoning_profile(
    provider: &Provider,
    request_model: Option<&str>,
) -> Option<CodexChatReasoningProfile> {
    if let Some(config) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.codex_chat_reasoning.clone())
    {
        return Some(normalize_codex_chat_reasoning_profile(
            codex_chat_reasoning_profile_from_config(config),
        ));
    }

    let model = request_model
        .map(ToString::to_string)
        .or_else(|| codex_provider_upstream_model(provider))
        .unwrap_or_default();
    let base_url = provider
        .settings_config
        .get("base_url")
        .or_else(|| provider.settings_config.get("baseURL"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            codex_provider_config_text(provider)
                .and_then(crate::codex_config::extract_codex_base_url)
        })
        .unwrap_or_default();

    infer_codex_chat_reasoning_profile(&provider.name, &base_url, &model)
}

pub(crate) fn codex_provider_apply_chat_upstream_model(
    provider: &Provider,
    body: &mut Value,
) -> Option<String> {
    if !codex_provider_uses_chat_completions(provider) {
        return None;
    }

    let catalog_model_ids = codex_provider_catalog_model_ids(provider);
    let upstream_model = codex_provider_upstream_model(provider);
    apply_codex_chat_upstream_model_policy(
        body,
        true,
        upstream_model.as_deref(),
        &catalog_model_ids,
    )
}

pub(crate) fn codex_provider_chat_reasoning_options(
    provider: &Provider,
    body: &Value,
) -> Option<CodexChatReasoningOptions> {
    codex_provider_chat_reasoning_profile(
        provider,
        body.get("model").and_then(|value| value.as_str()),
    )
    .map(|profile| CodexChatReasoningOptions::from_profile(&profile))
}

impl ProviderAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "Codex"
    }

    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError> {
        required_codex_provider_base_url(provider).map_err(ProxyError::ConfigError)
    }

    fn extract_auth(&self, provider: &Provider) -> Option<ProviderAuthInfo> {
        codex_provider_auth_info(provider)
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        build_codex_upstream_url(base_url, endpoint)
    }

    fn get_auth_headers(
        &self,
        auth: &ProviderAuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        codex_provider_auth_headers(auth).map_err(ProxyError::AuthError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::CodexChatReasoningConfig;
    use crate::proxy_core::api::auth::ProviderAuthStrategy;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Codex".to_string(),
            settings_config: config,
            website_url: None,
            category: Some("codex".to_string()),
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
    fn test_extract_base_url_direct() {
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "base_url": "https://api.openai.com/v1"
        }));

        let url = adapter.extract_base_url(&provider).unwrap();
        assert_eq!(url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_extract_auth_from_auth_field() {
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "auth": {
                "OPENAI_API_KEY": "sk-test-key-12345678"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-test-key-12345678");
        assert_eq!(auth.strategy, ProviderAuthStrategy::Bearer);
    }

    #[test]
    fn test_extract_auth_falls_back_to_config_bearer_when_auth_key_empty() {
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "auth": {
                "OPENAI_API_KEY": ""
            },
            "config": r#"model_provider = "custom"

[model_providers.custom]
experimental_bearer_token = "sk-config-key"
"#
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-config-key");
        assert_eq!(auth.strategy, ProviderAuthStrategy::Bearer);
    }

    #[test]
    fn test_extract_auth_from_env() {
        let adapter = CodexAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "OPENAI_API_KEY": "sk-env-key-12345678"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-env-key-12345678");
    }

    #[test]
    fn test_build_url() {
        let adapter = CodexAdapter::new();
        let url = adapter.build_url("https://api.openai.com/v1", "/responses");
        assert_eq!(url, "https://api.openai.com/v1/responses");
    }

    #[test]
    fn test_build_url_origin_adds_v1() {
        let adapter = CodexAdapter::new();
        let url = adapter.build_url("https://api.openai.com", "/responses");
        assert_eq!(url, "https://api.openai.com/v1/responses");
    }

    #[test]
    fn test_build_url_custom_prefix_no_v1() {
        let adapter = CodexAdapter::new();
        let url = adapter.build_url("https://example.com/openai", "/responses");
        assert_eq!(url, "https://example.com/openai/responses");
    }

    #[test]
    fn test_build_url_dedup_v1() {
        let adapter = CodexAdapter::new();
        // base_url 已包含 /v1，endpoint 也包含 /v1
        let url = adapter.build_url("https://www.packyapi.com/v1", "/v1/responses");
        assert_eq!(url, "https://www.packyapi.com/v1/responses");
    }

    #[test]
    fn test_get_auth_headers_emits_bearer_authorization() {
        let adapter = CodexAdapter::new();
        let auth = ProviderAuthInfo::new("sk-codex-test".to_string(), ProviderAuthStrategy::Bearer);

        let headers = adapter.get_auth_headers(&auth).unwrap();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(
            headers[0].1,
            http::HeaderValue::from_static("Bearer sk-codex-test")
        );
    }

    #[test]
    fn test_get_auth_headers_rejects_illegal_header_chars() {
        let adapter = CodexAdapter::new();
        let auth =
            ProviderAuthInfo::new("bad\r\nx-evil: 1".to_string(), ProviderAuthStrategy::Bearer);

        let result = adapter.get_auth_headers(&auth);

        assert!(matches!(result, Err(ProxyError::AuthError(_))));
    }

    #[test]
    fn test_codex_provider_uses_chat_completions_from_active_wire_api() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "chat_only"
model = "gpt-5"

[model_providers.chat_only]
name = "Chat Only"
base_url = "https://example.com/v1"
wire_api = "chat"
"#
        }));

        assert!(codex_provider_uses_chat_completions(&provider));
        assert!(codex_provider_should_convert_responses_to_chat(
            &provider,
            "/responses?stream=true"
        ));
        assert!(!codex_provider_should_convert_responses_to_chat(
            &provider,
            "/chat/completions"
        ));
    }

    #[test]
    fn test_codex_provider_uses_chat_completions_from_full_chat_url() {
        let provider = create_provider(json!({
            "base_url": "https://example.com/v1/chat/completions"
        }));

        assert!(codex_provider_uses_chat_completions(&provider));
        assert!(codex_provider_should_convert_responses_to_chat(
            &provider,
            "/v1/responses/compact"
        ));
    }

    #[test]
    fn test_codex_provider_uses_chat_completions_from_meta_api_format_for_compact() {
        let mut provider = create_provider(json!({
            "base_url": "https://example.com/v1"
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });

        assert!(codex_provider_uses_chat_completions(&provider));
        assert!(codex_provider_should_convert_responses_to_chat(
            &provider,
            "/responses/compact?stream=true"
        ));
    }

    #[test]
    fn test_codex_provider_uses_chat_completions_from_meta_api_format_for_responses() {
        let mut provider = create_provider(json!({
            "base_url": "https://api.deepseek.com/v1"
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });

        assert!(codex_provider_should_convert_responses_to_chat(
            &provider,
            "/v1/responses"
        ));
    }

    #[test]
    fn test_apply_codex_chat_upstream_model_uses_provider_config_model() {
        let mut provider = create_provider(json!({
            "config": r#"
model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        let mut body = json!({
            "model": "placeholder-client-model",
            "input": "ping"
        });

        let upstream_model = codex_provider_apply_chat_upstream_model(&provider, &mut body);

        assert_eq!(upstream_model.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(
            body.get("model").and_then(|v| v.as_str()),
            Some("deepseek-v4-flash")
        );
    }

    #[test]
    fn test_apply_codex_chat_upstream_model_preserves_catalog_model_selection() {
        let mut provider = create_provider(json!({
            "config": r#"
model_provider = "deepseek"
model = "deepseek-v4-flash"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
wire_api = "responses"
"#,
            "modelCatalog": {
                "models": [
                    { "model": "deepseek-v4-flash" },
                    { "model": "kimi-k2" }
                ]
            }
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        let mut body = json!({
            "model": "kimi-k2",
            "input": "ping"
        });

        let upstream_model = codex_provider_apply_chat_upstream_model(&provider, &mut body);

        assert_eq!(upstream_model.as_deref(), Some("kimi-k2"));
        assert_eq!(body.get("model").and_then(|v| v.as_str()), Some("kimi-k2"));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_infers_deepseek_effort_support() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "deepseek"
model = "deepseek-v4-pro"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat"
"#
        }));

        let config =
            codex_provider_chat_reasoning_profile(&provider, Some("deepseek-v4-pro")).unwrap();

        assert_eq!(config.supports_thinking, Some(true));
        assert_eq!(config.supports_effort, Some(true));
        assert_eq!(config.effort_value_mode.as_deref(), Some("deepseek"));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_explicit_meta_overrides_inference() {
        let mut provider = create_provider(json!({
            "config": r#"
model_provider = "deepseek"
model = "deepseek-v4-pro"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat"
"#
        }));
        provider.meta = Some(crate::provider::ProviderMeta {
            codex_chat_reasoning: Some(CodexChatReasoningConfig {
                supports_thinking: Some(false),
                supports_effort: Some(false),
                thinking_param: Some("none".to_string()),
                effort_param: Some("none".to_string()),
                effort_value_mode: None,
                output_format: Some("auto".to_string()),
            }),
            ..Default::default()
        });

        let config =
            codex_provider_chat_reasoning_profile(&provider, Some("deepseek-v4-pro")).unwrap();

        assert_eq!(config.supports_thinking, Some(false));
        assert_eq!(config.supports_effort, Some(false));
        assert_eq!(config.thinking_param.as_deref(), Some("none"));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_openrouter_platform_overrides_model() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "openrouter"
model = "deepseek/deepseek-chat-v3.1"

[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
wire_api = "chat"
"#
        }));

        // 模型名含 "deepseek"，但平台是 OpenRouter —— 平台规则必须覆盖模型规则。
        let config =
            codex_provider_chat_reasoning_profile(&provider, Some("deepseek/deepseek-chat-v3.1"))
                .unwrap();

        assert_eq!(config.thinking_param.as_deref(), Some("none"));
        assert_eq!(config.effort_param.as_deref(), Some("reasoning.effort"));
        assert_eq!(config.effort_value_mode.as_deref(), Some("openrouter"));
        assert_eq!(config.supports_effort, Some(true));
    }

    #[test]
    fn test_resolve_codex_chat_reasoning_siliconflow_platform_overrides_minimax() {
        let provider = create_provider(json!({
            "config": r#"
model_provider = "siliconflow"
model = "MiniMaxAI/MiniMax-M2.7"

[model_providers.siliconflow]
name = "SiliconFlow"
base_url = "https://api.siliconflow.cn/v1"
wire_api = "chat"
"#
        }));

        // 模型是 MiniMax（官方用 reasoning_split），但平台是 SiliconFlow —— 应走平台的 enable_thinking。
        let config =
            codex_provider_chat_reasoning_profile(&provider, Some("MiniMaxAI/MiniMax-M2.7"))
                .unwrap();

        assert_eq!(config.thinking_param.as_deref(), Some("enable_thinking"));
        assert_eq!(config.supports_effort, Some(false));
        assert_eq!(config.output_format.as_deref(), Some("reasoning_content"));
    }
}
