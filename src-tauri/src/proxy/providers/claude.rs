//! Claude (Anthropic) Provider Adapter
//!
//! 支持透传模式和 OpenAI 格式转换模式
//!
//! ## API 格式
//! - **anthropic** (默认): Anthropic Messages API 格式，直接透传
//! - **openai_chat**: OpenAI Chat Completions 格式，需要 Anthropic ↔ OpenAI 转换
//! - **openai_responses**: OpenAI Responses API 格式，需要 Anthropic ↔ Responses 转换
//! - **gemini_native**: Google Gemini Native generateContent 格式，需要 Anthropic ↔ Gemini 转换
//!
//! ## 认证模式
//! - **Claude**: Anthropic 官方 API (x-api-key + anthropic-version)
//! - **ClaudeAuth**: 中转服务 (仅 Bearer 认证，无 x-api-key)
//! - **OpenRouter**: 已支持 Claude Code 兼容接口，默认透传
//! - **GitHubCopilot**: GitHub Copilot (OAuth + Copilot Token)

use super::ProviderAdapter;
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy_core_adapter::{
    normalize_anthropic_tool_thinking_history, provider_claude_api_format,
    provider_claude_auth_headers, provider_claude_auth_info,
    provider_claude_transform_request_for_api_format, provider_claude_transform_response,
    provider_claude_upstream_url, required_claude_provider_base_url,
    provider_normalize_deepseek_thinking_disabled_strip_effort,
    provider_needs_claude_transform, provider_should_normalize_anthropic_tool_thinking_history,
    GeminiShadowStore, ProviderAuthInfo,
};
#[cfg(test)]
use crate::proxy_core_adapter::{
    provider_claude_kind, should_normalize_anthropic_tool_thinking_history, ProviderAuthStrategy,
    ProviderKind,
};
use serde_json::Value;

/// 获取 Claude 供应商的 API 格式
///
/// 供 handler/forwarder 外部使用的公开函数。
/// 优先级：meta.apiFormat > settings_config.api_format > openrouter_compat_mode > 默认 "anthropic"
pub fn get_claude_api_format(provider: &Provider) -> &'static str {
    provider_claude_api_format(provider)
}

pub fn normalize_anthropic_messages_for_provider(
    body: &mut Value,
    provider: &Provider,
    api_format: &str,
) -> bool {
    if api_format.trim() != "anthropic" {
        return false;
    }

    let mut changed = if provider_should_normalize_anthropic_tool_thinking_history(
        provider,
        body,
        api_format,
    ) {
        normalize_anthropic_tool_thinking_history(body)
    } else {
        false
    };
    changed |= provider_normalize_deepseek_thinking_disabled_strip_effort(provider, body);
    changed
}

pub fn transform_claude_request_for_api_format(
    body: serde_json::Value,
    provider: &Provider,
    api_format: &str,
    session_id: Option<&str>,
    shadow_store: Option<&GeminiShadowStore>,
) -> Result<serde_json::Value, ProxyError> {
    provider_claude_transform_request_for_api_format(
        body,
        provider,
        api_format,
        session_id,
        shadow_store,
    )
    .map_err(ProxyError::TransformError)
}

/// Claude 适配器
pub struct ClaudeAdapter;

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self
    }

    /// 获取供应商类型
    ///
    /// 根据 base_url 和 auth_mode 检测具体的供应商类型：
    /// - GitHubCopilot: meta.provider_type 为 github_copilot 或 base_url 包含 githubcopilot.com
    /// - CodexOAuth: meta.provider_type 为 codex_oauth
    /// - OpenRouter: base_url 包含 openrouter.ai
    /// - ClaudeAuth: auth_mode 为 bearer_only
    /// - Claude: 默认 Anthropic 官方
    #[cfg(test)]
    pub fn provider_type(&self, provider: &Provider) -> ProviderKind {
        provider_claude_kind(provider)
    }

    /// 获取 API 格式
    ///
    /// 从 provider.meta.api_format 读取格式设置：
    /// - "anthropic" (默认): Anthropic Messages API 格式，直接透传
    /// - "openai_chat": OpenAI Chat Completions 格式，需要格式转换
    /// - "openai_responses": OpenAI Responses API 格式，需要格式转换
    fn get_api_format(&self, provider: &Provider) -> &'static str {
        get_claude_api_format(provider)
    }
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAdapter for ClaudeAdapter {
    fn name(&self) -> &'static str {
        "Claude"
    }

    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError> {
        required_claude_provider_base_url(provider).map_err(ProxyError::ConfigError)
    }

    fn extract_auth(&self, provider: &Provider) -> Option<ProviderAuthInfo> {
        provider_claude_auth_info(provider)
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        provider_claude_upstream_url(base_url, endpoint)
    }

    fn get_auth_headers(
        &self,
        auth: &ProviderAuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        provider_claude_auth_headers(auth).map_err(ProxyError::AuthError)
    }

    fn needs_transform(&self, provider: &Provider) -> bool {
        provider_needs_claude_transform(provider)
    }

    fn transform_request(
        &self,
        body: serde_json::Value,
        provider: &Provider,
    ) -> Result<serde_json::Value, ProxyError> {
        transform_claude_request_for_api_format(
            body,
            provider,
            self.get_api_format(provider),
            None,
            None,
        )
    }

    fn transform_response(&self, body: serde_json::Value) -> Result<serde_json::Value, ProxyError> {
        provider_claude_transform_response(body).map_err(ProxyError::TransformError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderMeta;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Claude".to_string(),
            settings_config: config,
            website_url: None,
            category: Some("claude".to_string()),
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    fn create_provider_with_meta(config: serde_json::Value, meta: ProviderMeta) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Claude".to_string(),
            settings_config: config,
            website_url: None,
            category: Some("claude".to_string()),
            created_at: None,
            sort_index: None,
            notes: None,
            meta: Some(meta),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn test_extract_base_url_from_env() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }));

        let url = adapter.extract_base_url(&provider).unwrap();
        assert_eq!(url, "https://api.anthropic.com");
    }

    #[test]
    fn test_extract_auth_anthropic_auth_token_uses_claude_auth_strategy() {
        // ANTHROPIC_AUTH_TOKEN 在 Anthropic SDK 里语义就是 Authorization: Bearer，
        // 因此走 ClaudeAuth strategy 而不是 Anthropic（x-api-key）。
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-ant-test-key");
        assert_eq!(auth.strategy, ProviderAuthStrategy::ClaudeAuth);
    }

    #[test]
    fn test_extract_auth_anthropic_api_key() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_API_KEY": "sk-ant-test-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-ant-test-key");
        assert_eq!(auth.strategy, ProviderAuthStrategy::Anthropic);
    }

    #[test]
    fn test_extract_auth_both_env_vars_prefer_auth_token() {
        // 两个变量都填时，extract_key 选 AUTH_TOKEN，strategy 推断也必须保持一致。
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-from-auth-token",
                "ANTHROPIC_API_KEY": "sk-from-api-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-from-auth-token");
        assert_eq!(auth.strategy, ProviderAuthStrategy::ClaudeAuth);
    }

    #[test]
    fn test_extract_auth_apikey_field_fallback_uses_anthropic_strategy() {
        // 当用户没填任一 ANTHROPIC_* env，而是直接使用 apiKey 字段时，
        // 视为没有显式语义偏好，默认走 Anthropic 官方协议（x-api-key）。
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "apiKey": "sk-direct",
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-direct");
        assert_eq!(auth.strategy, ProviderAuthStrategy::Anthropic);
    }

    #[test]
    fn test_get_auth_headers_anthropic_emits_x_api_key() {
        let adapter = ClaudeAdapter::new();
        let auth =
            ProviderAuthInfo::new("sk-ant-test".to_string(), ProviderAuthStrategy::Anthropic);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "x-api-key");
        assert_eq!(headers[0].1.to_str().unwrap(), "sk-ant-test");
    }

    #[test]
    fn test_get_auth_headers_claude_auth_emits_authorization_bearer() {
        let adapter = ClaudeAdapter::new();
        let auth = ProviderAuthInfo::new(
            "sk-relay-test".to_string(),
            ProviderAuthStrategy::ClaudeAuth,
        );

        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1.to_str().unwrap(), "Bearer sk-relay-test");
    }

    #[test]
    fn test_get_auth_headers_bearer_emits_authorization_bearer() {
        let adapter = ClaudeAdapter::new();
        let auth = ProviderAuthInfo::new("sk-or-test".to_string(), ProviderAuthStrategy::Bearer);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1.to_str().unwrap(), "Bearer sk-or-test");
    }

    #[test]
    fn test_get_auth_headers_google_oauth_emits_bearer_and_client_marker() {
        let adapter = ClaudeAdapter::new();
        let auth = ProviderAuthInfo::with_access_token(
            "refresh-token".to_string(),
            "ya29.access-token".to_string(),
        );

        let headers = adapter.get_auth_headers(&auth).unwrap();

        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1.to_str().unwrap(), "Bearer ya29.access-token");
        assert_eq!(headers[1].0.as_str(), "x-goog-api-client");
        assert_eq!(headers[1].1.to_str().unwrap(), "GeminiCLI/1.0");
    }

    #[test]
    fn test_get_auth_headers_codex_oauth_emits_originator() {
        let adapter = ClaudeAdapter::new();
        let auth = ProviderAuthInfo::new(
            "chatgpt-token".to_string(),
            ProviderAuthStrategy::CodexOAuth,
        );

        let headers = adapter.get_auth_headers(&auth).unwrap();

        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1.to_str().unwrap(), "Bearer chatgpt-token");
        assert_eq!(headers[1].0.as_str(), "originator");
        assert_eq!(headers[1].1.to_str().unwrap(), "cc-switch");
    }

    #[test]
    fn test_get_auth_headers_github_copilot_emits_fingerprint_headers() {
        let adapter = ClaudeAdapter::new();
        let auth = ProviderAuthInfo::new(
            "copilot-token".to_string(),
            ProviderAuthStrategy::GitHubCopilot,
        );

        let headers = adapter.get_auth_headers(&auth).unwrap();

        let pairs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.to_str().unwrap()))
            .collect();
        assert_eq!(pairs[0], ("authorization", "Bearer copilot-token"));
        assert_eq!(
            pairs[1],
            (
                "editor-version",
                crate::proxy::providers::copilot_auth::COPILOT_EDITOR_VERSION,
            )
        );
        assert_eq!(
            pairs[2],
            (
                "editor-plugin-version",
                crate::proxy::providers::copilot_auth::COPILOT_PLUGIN_VERSION,
            )
        );
        assert_eq!(
            pairs[3],
            (
                "copilot-integration-id",
                crate::proxy::providers::copilot_auth::COPILOT_INTEGRATION_ID,
            )
        );
        assert_eq!(
            pairs[4],
            (
                "user-agent",
                crate::proxy::providers::copilot_auth::COPILOT_USER_AGENT,
            )
        );
        assert_eq!(
            pairs[5],
            (
                "x-github-api-version",
                crate::proxy::providers::copilot_auth::COPILOT_API_VERSION,
            )
        );
        assert_eq!(pairs[6], ("openai-intent", "conversation-agent"));
        assert_eq!(pairs[7], ("x-initiator", "user"));
        assert_eq!(pairs[8], ("x-interaction-type", "conversation-agent"));
        assert_eq!(
            pairs[9],
            ("x-vscode-user-agent-library-version", "electron-fetch")
        );
        assert_eq!(pairs[10].0, "x-request-id");
        assert_eq!(pairs[11].0, "x-agent-task-id");
        assert_eq!(pairs[10].1, pairs[11].1);
        uuid::Uuid::parse_str(pairs[10].1).expect("request id should be a UUID");
    }

    #[test]
    fn test_get_auth_headers_rejects_illegal_header_chars() {
        // 用户粘贴含 \r\n 的"脏"key 不能让进程 panic
        let adapter = ClaudeAdapter::new();
        let auth = ProviderAuthInfo::new(
            "sk-ant-bad\r\nX-Inject: 1".to_string(),
            ProviderAuthStrategy::Anthropic,
        );

        let result = adapter.get_auth_headers(&auth);
        assert!(result.is_err(), "expected AuthError, got Ok");
        assert!(matches!(result, Err(ProxyError::AuthError(_))));
    }

    #[test]
    fn test_extract_auth_openrouter() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-or-test-key");
        assert_eq!(auth.strategy, ProviderAuthStrategy::Bearer);
    }

    #[test]
    fn test_extract_auth_gemini_api_key() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com/v1beta",
                    "GEMINI_API_KEY": "gemini-test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "gemini-test-key");
        assert_eq!(auth.strategy, ProviderAuthStrategy::Google);
    }

    #[test]
    fn test_extract_auth_claude_auth_mode() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-proxy-key"
            },
            "auth_mode": "bearer_only"
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-proxy-key");
        assert_eq!(auth.strategy, ProviderAuthStrategy::ClaudeAuth);
    }

    #[test]
    fn test_extract_auth_claude_auth_env_mode() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-proxy-key",
                "AUTH_MODE": "bearer_only"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-proxy-key");
        assert_eq!(auth.strategy, ProviderAuthStrategy::ClaudeAuth);
    }

    /// Regression: a Gemini OAuth credential JSON that carries only a
    /// refresh_token (no active access_token) must not be surfaced as an
    /// `ProviderAuthInfo` whose bearer would be empty. Without the guard, downstream
    /// header injection produces `Authorization: Bearer ` and a deterministic
    /// 401 from upstream.
    #[test]
    fn test_extract_auth_gemini_cli_refresh_only_json_does_not_expose_empty_bearer() {
        let adapter = ClaudeAdapter::new();
        let refresh_only_json =
            r#"{"refresh_token":"rt-abc","client_id":"cid","client_secret":"cs"}"#;
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": refresh_only_json
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        let auth = adapter.extract_auth(&provider).unwrap();
        // access_token must not be surfaced as `Some("")` — the OAuth header
        // builder uses `access_token.as_ref().unwrap_or(&api_key)`, so a
        // `Some("")` would win over the raw key and emit `Bearer `.
        assert!(
            auth.access_token.as_deref().is_none_or(|t| !t.is_empty()),
            "empty access_token leaked into ProviderAuthInfo"
        );
        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
    }

    /// Companion case: a JSON credential with an empty-string `access_token`
    /// field (the shape an expired credential can take after partial writes)
    /// must degrade the same way.
    #[test]
    fn test_extract_auth_gemini_cli_empty_access_token_degrades_to_raw_key() {
        let adapter = ClaudeAdapter::new();
        let expired_json = r#"{"access_token":"","refresh_token":"rt-abc","client_id":"cid","client_secret":"cs"}"#;
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": expired_json
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        let auth = adapter.extract_auth(&provider).unwrap();
        assert!(
            auth.access_token.as_deref().is_none_or(|t| !t.is_empty()),
            "empty access_token leaked into ProviderAuthInfo"
        );
        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
    }

    /// Counter-case: a well-formed JSON credential with a non-empty
    /// access_token must still flow through the OAuth path unchanged.
    #[test]
    fn test_extract_auth_gemini_cli_valid_json_keeps_access_token() {
        let adapter = ClaudeAdapter::new();
        let valid_json = r#"{"access_token":"ya29.valid","refresh_token":"rt"}"#;
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": valid_json
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.access_token.as_deref(), Some("ya29.valid"));
        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
    }

    /// 回归:从 oauth_creds.json 复制时常带前导换行/空格。未 trim 时
    /// `starts_with('{')` 会落空,导致误分类为 `ProviderKind::Gemini`,再
    /// 以 raw JSON 当 `x-goog-api-key` 发出去触发 401。trim 应在 provider
    /// 类型判定和 OAuth 解析前统一生效。
    #[test]
    fn test_extract_auth_gemini_cli_json_with_leading_whitespace_classifies_correctly() {
        let adapter = ClaudeAdapter::new();
        let valid_json = r#"{"access_token":"ya29.valid","refresh_token":"rt"}"#;
        let key_with_whitespace = format!("\n  {valid_json}\n");
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": key_with_whitespace
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        assert_eq!(adapter.provider_type(&provider), ProviderKind::GeminiCli);

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.access_token.as_deref(), Some("ya29.valid"));
        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
    }

    /// 回归:裸 `ya29.` access_token 若带前导换行,也应被 trim 后识别为
    /// Gemini CLI OAuth,避免前导空白把 `starts_with("ya29.")` 检查顶穿。
    #[test]
    fn test_extract_auth_gemini_cli_access_token_with_leading_newline_classifies_correctly() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": "\nya29.raw-token-value\n"
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );

        assert_eq!(adapter.provider_type(&provider), ProviderKind::GeminiCli);

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.access_token.as_deref(), Some("ya29.raw-token-value"));
        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
    }

    #[test]
    fn test_provider_type_detection() {
        let adapter = ClaudeAdapter::new();

        // Anthropic 官方
        let anthropic = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
            }
        }));
        assert_eq!(adapter.provider_type(&anthropic), ProviderKind::Claude);

        // OpenRouter
        let openrouter = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test"
            }
        }));
        assert_eq!(adapter.provider_type(&openrouter), ProviderKind::OpenRouter);

        // ClaudeAuth
        let claude_auth = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-test"
            },
            "auth_mode": "bearer_only"
        }));
        assert_eq!(
            adapter.provider_type(&claude_auth),
            ProviderKind::ClaudeAuth
        );
    }

    #[test]
    fn test_build_url_anthropic() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://api.anthropic.com", "/v1/messages");
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_build_url_openrouter() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://openrouter.ai/api", "/v1/messages");
        assert_eq!(url, "https://openrouter.ai/api/v1/messages");
    }

    #[test]
    fn test_build_url_no_beta_for_other_endpoints() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://api.anthropic.com", "/v1/complete");
        assert_eq!(url, "https://api.anthropic.com/v1/complete");
    }

    #[test]
    fn test_build_url_preserve_existing_query() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://api.anthropic.com", "/v1/messages?foo=bar");
        assert_eq!(url, "https://api.anthropic.com/v1/messages?foo=bar");
    }

    #[test]
    fn test_build_url_no_beta_for_github_copilot() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://api.githubcopilot.com", "/v1/messages");
        assert_eq!(url, "https://api.githubcopilot.com/v1/messages");
    }

    #[test]
    fn test_build_url_no_beta_for_openai_chat_completions() {
        let adapter = ClaudeAdapter::new();
        let url = adapter.build_url("https://integrate.api.nvidia.com", "/v1/chat/completions");
        assert_eq!(url, "https://integrate.api.nvidia.com/v1/chat/completions");
    }

    #[test]
    fn test_needs_transform() {
        let adapter = ClaudeAdapter::new();

        // Default: no transform (anthropic format) - no meta
        let anthropic_provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }));
        assert!(!adapter.needs_transform(&anthropic_provider));

        // Explicit anthropic format in meta: no transform
        let explicit_anthropic = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("anthropic".to_string()),
                ..Default::default()
            },
        );
        assert!(!adapter.needs_transform(&explicit_anthropic));

        // Legacy settings_config.api_format: openai_chat should enable transform
        let legacy_settings_api_format = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "api_format": "openai_chat"
        }));
        assert!(adapter.needs_transform(&legacy_settings_api_format));

        // Legacy openrouter_compat_mode: bool/number/string should enable transform
        let legacy_openrouter_bool = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "openrouter_compat_mode": true
        }));
        assert!(adapter.needs_transform(&legacy_openrouter_bool));

        let legacy_openrouter_num = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "openrouter_compat_mode": 1
        }));
        assert!(adapter.needs_transform(&legacy_openrouter_num));

        let legacy_openrouter_str = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "openrouter_compat_mode": "true"
        }));
        assert!(adapter.needs_transform(&legacy_openrouter_str));

        // OpenAI Chat format in meta: needs transform
        let openai_chat_provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        assert!(adapter.needs_transform(&openai_chat_provider));

        // OpenAI Responses format in meta: needs transform
        let openai_responses_provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                ..Default::default()
            },
        );
        assert!(adapter.needs_transform(&openai_responses_provider));

        let gemini_native_provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );
        assert!(adapter.needs_transform(&gemini_native_provider));
        assert_eq!(
            adapter.provider_type(&gemini_native_provider),
            ProviderKind::Gemini
        );

        // meta takes precedence over legacy settings_config fields
        let meta_precedence_over_settings = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                },
                "api_format": "openai_chat",
                "openrouter_compat_mode": true
            }),
            ProviderMeta {
                api_format: Some("anthropic".to_string()),
                ..Default::default()
            },
        );
        assert!(!adapter.needs_transform(&meta_precedence_over_settings));

        // Unknown format in meta: default to anthropic (no transform)
        let unknown_format = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("unknown".to_string()),
                ..Default::default()
            },
        );
        assert!(!adapter.needs_transform(&unknown_format));
    }

    #[test]
    fn test_github_copilot_detection_by_url() {
        let adapter = ClaudeAdapter::new();

        // GitHub Copilot by base_url
        let copilot = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
            }
        }));
        assert_eq!(adapter.provider_type(&copilot), ProviderKind::GitHubCopilot);
    }

    #[test]
    fn test_github_copilot_detection_by_meta() {
        let adapter = ClaudeAdapter::new();

        // GitHub Copilot by meta.provider_type
        let copilot_meta = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            ProviderMeta {
                provider_type: Some("github_copilot".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(
            adapter.provider_type(&copilot_meta),
            ProviderKind::GitHubCopilot
        );
    }

    #[test]
    fn test_github_copilot_auth() {
        let adapter = ClaudeAdapter::new();

        let copilot = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
            }
        }));

        let auth = adapter.extract_auth(&copilot).unwrap();
        assert_eq!(auth.strategy, ProviderAuthStrategy::GitHubCopilot);
    }

    #[test]
    fn test_github_copilot_needs_transform() {
        let adapter = ClaudeAdapter::new();

        let copilot = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
            }
        }));

        // GitHub Copilot always needs transform
        assert!(adapter.needs_transform(&copilot));
    }

    #[test]
    fn test_transform_response_delegates_to_adapter() {
        let adapter = ClaudeAdapter::new();
        let transformed = adapter
            .transform_response(json!({
                "id": "chatcmpl_1",
                "model": "chat-model",
                "choices": [{
                    "message": {"role": "assistant", "content": "Hi"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 2}
            }))
            .unwrap();

        assert_eq!(transformed["content"][0]["text"], "Hi");
    }

    #[test]
    fn test_transform_claude_request_for_api_format_responses() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
            }
        }));
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            None,
            None,
        )
        .unwrap();

        assert_eq!(transformed["model"], "gpt-5.4");
        assert!(transformed.get("input").is_some());
        assert!(transformed.get("max_output_tokens").is_some());
    }

    #[test]
    fn test_transform_claude_request_openai_chat_streaming_injects_include_usage() {
        let provider = create_provider(json!({
            "env": { "ANTHROPIC_BASE_URL": "https://openrouter.ai/api/v1" }
        }));
        // 流式请求必须注入 stream_options.include_usage，否则 OpenAI 兼容上游不在
        // SSE 末尾吐 usage → 转换出的 Anthropic message_delta 全 0 → 整笔 usage 漏记。
        let body = json!({
            "model": "moonshotai/kimi-k2",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128,
            "stream": true
        });
        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();
        assert_eq!(transformed["stream"], true);
        assert_eq!(transformed["stream_options"]["include_usage"], true);
    }

    #[test]
    fn test_transform_claude_request_openai_chat_non_streaming_omits_stream_options() {
        let provider = create_provider(json!({
            "env": { "ANTHROPIC_BASE_URL": "https://openrouter.ai/api/v1" }
        }));
        // 非流式请求不应注入 stream_options（usage 在非流式响应体里恒有）。
        let body = json!({
            "model": "moonshotai/kimi-k2",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });
        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();
        assert!(transformed.get("stream_options").is_none());
    }

    #[test]
    fn test_transform_claude_request_for_codex_oauth_uses_session_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                provider_type: Some("codex_oauth".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            Some("session-123"),
            None,
        )
        .unwrap();

        assert_eq!(transformed["prompt_cache_key"], "session-123");
    }

    #[test]
    fn test_transform_claude_request_for_codex_oauth_without_session_omits_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                provider_type: Some("codex_oauth".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            None,
            None,
        )
        .unwrap();

        assert!(transformed.get("prompt_cache_key").is_none());
    }

    #[test]
    fn test_transform_claude_request_for_responses_uses_session_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.openai.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            Some("claude-session-123"),
            None,
        )
        .unwrap();

        assert_eq!(transformed["prompt_cache_key"], "claude-session-123");
    }

    #[test]
    fn test_transform_claude_request_for_responses_without_session_omits_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.openai.example.com"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            None,
            None,
        )
        .unwrap();

        assert!(transformed.get("prompt_cache_key").is_none());
    }

    #[test]
    fn test_transform_claude_request_for_codex_oauth_keeps_explicit_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_responses".to_string()),
                provider_type: Some("codex_oauth".to_string()),
                prompt_cache_key: Some("explicit-cache-key".to_string()),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            Some("session-123"),
            None,
        )
        .unwrap();

        assert_eq!(transformed["prompt_cache_key"], "explicit-cache-key");
    }

    #[test]
    fn test_transform_claude_request_for_api_format_codex_oauth_fast_mode_off() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            ProviderMeta {
                provider_type: Some("codex_oauth".to_string()),
                codex_fast_mode: Some(false),
                ..ProviderMeta::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 128
        });

        let transformed = transform_claude_request_for_api_format(
            body,
            &provider,
            "openai_responses",
            None,
            None,
        )
        .unwrap();

        assert_eq!(transformed["store"], json!(false));
        assert!(transformed.get("service_tier").is_none());
        assert_eq!(
            transformed["include"],
            json!(["reasoning.encrypted_content"])
        );
    }

    #[test]
    fn test_transform_claude_request_for_api_format_gemini_native() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("gemini_native".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "gemini-2.5-pro",
            "system": "You are helpful.",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 64
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "gemini_native", None, None)
                .unwrap();

        assert!(transformed.get("contents").is_some());
        assert_eq!(
            transformed["systemInstruction"]["parts"][0]["text"],
            "You are helpful."
        );
        assert_eq!(transformed["generationConfig"]["maxOutputTokens"], 64);
    }

    #[test]
    fn test_transform_claude_request_for_api_format_openai_chat_skips_prompt_cache_key_by_default()
    {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 64
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        assert!(transformed.get("prompt_cache_key").is_none());
    }

    #[test]
    fn test_transform_claude_request_for_api_format_openai_chat_keeps_explicit_prompt_cache_key() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                prompt_cache_key: Some("claude-cache-route".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 64
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        assert_eq!(transformed["prompt_cache_key"], "claude-cache-route");
    }

    #[test]
    fn test_transform_openai_chat_skips_reasoning_content_for_generic_provider() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "gpt-5.4",
            "max_tokens": 64,
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "I should call the tool."},
                    {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {"location": "Tokyo"}}
                ]
            }]
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        let msg = &transformed["messages"][0];
        assert!(msg.get("tool_calls").is_some());
        assert!(msg.get("reasoning_content").is_none());
    }

    #[test]
    fn test_transform_openai_chat_preserves_reasoning_content_for_kimi_provider() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.moonshot.cn/v1",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "kimi-k2.6",
            "max_tokens": 64,
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "I should call the tool."},
                    {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {"location": "Tokyo"}}
                ]
            }]
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        let msg = &transformed["messages"][0];
        assert_eq!(msg["reasoning_content"], "I should call the tool.");
        assert!(msg.get("tool_calls").is_some());
    }

    #[test]
    fn test_transform_openai_chat_preserves_reasoning_content_for_deepseek_provider() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.deepseek.com/v1",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "deepseek-v4-flash",
            "max_tokens": 64,
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "I should call the tool."},
                    {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {"location": "Tokyo"}}
                ]
            }]
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        let msg = &transformed["messages"][0];
        assert_eq!(msg["reasoning_content"], "I should call the tool.");
        assert!(msg.get("tool_calls").is_some());
    }

    #[test]
    fn test_transform_openai_chat_preserves_reasoning_content_for_mimo_provider() {
        let provider = create_provider_with_meta(
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.xiaomimimo.com/v1",
                    "ANTHROPIC_API_KEY": "test-key"
                }
            }),
            ProviderMeta {
                api_format: Some("openai_chat".to_string()),
                ..Default::default()
            },
        );
        let body = json!({
            "model": "mimo-v2.5-pro",
            "max_tokens": 64,
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "I should call the tool."},
                    {"type": "tool_use", "id": "call_123", "name": "get_weather", "input": {"location": "Tokyo"}}
                ]
            }]
        });

        let transformed =
            transform_claude_request_for_api_format(body, &provider, "openai_chat", None, None)
                .unwrap();

        let msg = &transformed["messages"][0];
        assert_eq!(msg["reasoning_content"], "I should call the tool.");
        assert!(msg.get("tool_calls").is_some());
    }

    fn normalize_anthropic_tool_thinking_history_for_provider(
        body: &mut Value,
        provider: &Provider,
        api_format: &str,
    ) -> bool {
        if !should_normalize_anthropic_tool_thinking_history(
            &provider.settings_config,
            body,
            api_format,
        ) {
            return false;
        }

        normalize_anthropic_tool_thinking_history(body)
    }

    #[test]
    fn test_deepseek_anthropic_tool_history_injects_missing_thinking() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "I will inspect the repo."},
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(changed);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(
            content[0]["thinking"],
            crate::proxy_core_adapter::anthropic_tool_thinking_placeholder()
        );
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[2]["type"], "tool_use");
    }

    #[test]
    fn test_anthropic_messages_no_longer_hoists_system_role_messages() {
        // After reverting #3775, role=system messages are left in `messages[]`
        // (DeepSeek's endpoint accepts them natively) and the top-level `system`
        // field is untouched, preserving the request prefix.
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "system": "Existing top-level system.",
            "model": "deepseek-v4-pro",
            "messages": [
                { "role": "system", "content": "Message system one." },
                { "role": "user", "content": "hello" },
                {
                    "role": "system",
                    "content": [{ "type": "text", "text": "Message system two." }]
                }
            ]
        });

        let changed = normalize_anthropic_messages_for_provider(&mut body, &provider, "anthropic");

        assert!(!changed);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "system");
        assert_eq!(body["system"], "Existing top-level system.");
    }

    #[test]
    fn test_anthropic_system_role_messages_skip_non_anthropic_format() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/v1",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [
                { "role": "system", "content": "Keep in messages." },
                { "role": "user", "content": "hello" }
            ]
        });

        let changed =
            normalize_anthropic_messages_for_provider(&mut body, &provider, "openai_chat");

        assert!(!changed);
        assert!(body.get("system").is_none());
        assert_eq!(body["messages"][0]["role"], "system");
    }

    #[test]
    fn test_kimi_anthropic_tool_history_injects_missing_thinking() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "kimi-for-coding",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(changed);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(
            content[0]["thinking"],
            crate::proxy_core_adapter::anthropic_tool_thinking_placeholder()
        );
        assert_eq!(content[1]["type"], "tool_use");
    }

    #[test]
    fn test_deepseek_anthropic_tool_history_rewrites_redacted_thinking() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "redacted_thinking", "data": "opaque"},
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(changed);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(
            content[0]["thinking"],
            crate::proxy_core_adapter::anthropic_redacted_thinking_placeholder()
        );
        assert!(content[0].get("data").is_none());
    }

    #[test]
    fn test_deepseek_anthropic_tool_history_keeps_thinking_text_but_drops_signature() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Need to inspect the file.", "signature": "anthropic-signature"},
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(changed);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "Need to inspect the file.");
        assert!(content[0].get("signature").is_none());
    }

    #[test]
    fn test_generic_anthropic_tool_history_is_not_modified() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "claude-sonnet-4.6",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });
        let original = body.clone();

        let changed = normalize_anthropic_tool_thinking_history_for_provider(
            &mut body,
            &provider,
            "anthropic",
        );

        assert!(!changed);
        assert_eq!(body, original);
    }

    // ==================== normalize_deepseek_thinking_disabled_strip_effort 测试 ====================

    fn deepseek_official_provider() -> Provider {
        create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }))
    }

    fn normalize_deepseek_thinking_disabled_strip_effort(
        body: &mut Value,
        provider: &Provider,
    ) -> bool {
        crate::proxy_core_adapter::normalize_deepseek_thinking_disabled_strip_effort(
            body,
            &provider.settings_config,
        )
    }

    #[test]
    fn test_deepseek_official_strips_output_config_effort() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(changed);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("output_config").is_none());
    }

    #[test]
    fn test_deepseek_official_strips_reasoning_effort() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "reasoning_effort": "high",
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(changed);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_deepseek_official_strips_both_effort_fields() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "reasoning_effort": "high",
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(changed);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("output_config").is_none());
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_deepseek_official_no_effort_no_change() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "max_tokens": 100000
        });
        let original = body.clone();

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(!changed);
        assert_eq!(body, original);
    }

    #[test]
    fn test_deepseek_official_preserves_output_config_other_fields() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max", "temperature": 0.5 },
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider(),
        );

        assert!(changed);
        assert_eq!(body["output_config"]["temperature"], 0.5);
        assert!(body["output_config"].get("effort").is_none());
    }

    #[test]
    fn test_deepseek_official_non_disabled_not_modified() {
        let cases = vec![
            (
                "enabled",
                json!({ "type": "enabled", "budget_tokens": 16000 }),
            ),
            ("adaptive", json!({ "type": "adaptive" })),
        ];

        for (label, thinking_value) in cases {
            let mut body = json!({
                "model": "deepseek-v4-pro",
                "thinking": thinking_value,
                "output_config": { "effort": "max" },
                "max_tokens": 100000
            });
            let original = body.clone();

            let changed = normalize_deepseek_thinking_disabled_strip_effort(
                &mut body,
                &deepseek_official_provider(),
            );

            assert!(!changed, "should not modify thinking.type={label}");
            assert_eq!(body, original);
        }

        // missing thinking field entirely
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "output_config": { "effort": "max" },
            "max_tokens": 100000
        });
        let original = body.clone();
        assert!(!normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &deepseek_official_provider()
        ));
        assert_eq!(body, original);
    }

    #[test]
    fn test_deepseek_official_url_with_trailing_slash() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic/",
                "ANTHROPIC_API_KEY": "test-key"
            }
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(&mut body, &provider);

        assert!(changed);
        assert!(body.get("output_config").is_none());
    }

    #[test]
    fn test_deepseek_official_detected_via_base_url_fallback() {
        let provider = create_provider(json!({
            "base_url": "https://api.deepseek.com/anthropic",
            "ANTHROPIC_API_KEY": "test-key"
        }));
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "reasoning_effort": "high",
            "max_tokens": 100000
        });

        let changed = normalize_deepseek_thinking_disabled_strip_effort(&mut body, &provider);

        assert!(changed);
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_non_deepseek_endpoint_not_modified() {
        let providers = vec![
            create_provider(json!({
                "env": { "ANTHROPIC_BASE_URL": "https://other-api.com/anthropic", "ANTHROPIC_API_KEY": "test-key" }
            })),
            create_provider(json!({
                "env": { "ANTHROPIC_BASE_URL": "https://api.anthropic.com", "ANTHROPIC_API_KEY": "test-key" }
            })),
        ];

        for provider in providers {
            let mut body = json!({
                "model": "deepseek-v4-pro",
                "thinking": { "type": "disabled" },
                "output_config": { "effort": "max" },
                "max_tokens": 100000
            });
            let original = body.clone();

            let changed = normalize_deepseek_thinking_disabled_strip_effort(&mut body, &provider);

            assert!(
                !changed,
                "should not modify for {}",
                provider.settings_config["env"]["ANTHROPIC_BASE_URL"]
            );
            assert_eq!(body, original);
        }
    }

    #[test]
    fn test_normalize_messages_pipeline_strips_effort_for_deepseek() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "max_tokens": 100000,
            "messages": [{ "role": "user", "content": "hello" }]
        });

        let changed = normalize_anthropic_messages_for_provider(
            &mut body,
            &deepseek_official_provider(),
            "anthropic",
        );

        assert!(changed);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("output_config").is_none());
    }
}
