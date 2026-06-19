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

use super::{AuthInfo, AuthStrategy, ProviderAdapter, ProviderType};
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy_core::{
    anthropic_to_openai_chat_request, anthropic_to_openai_responses_request,
    build_claude_auth_headers, build_claude_upstream_url, build_copilot_auth_headers,
    claude_api_format_needs_transform, extract_claude_auth_key_from_settings,
    extract_claude_base_url_from_settings, infer_claude_provider_kind,
    is_copilot_prompt_cache_provider, normalize_anthropic_tool_thinking_history,
    openai_chat_to_anthropic_message, openai_responses_to_anthropic_message,
    resolve_claude_api_format_from_settings, resolve_claude_responses_prompt_cache_key,
    should_normalize_anthropic_tool_thinking_history,
    should_preserve_reasoning_content_for_openai_chat, ClaudeAuthHeaderKind, ClaudeAuthKey,
    ClaudeAuthKeySource, CopilotAuthHeadersInput,
};
use serde_json::Value;

/// 获取 Claude 供应商的 API 格式
///
/// 供 handler/forwarder 外部使用的公开函数。
/// 优先级：meta.apiFormat > settings_config.api_format > openrouter_compat_mode > 默认 "anthropic"
pub fn get_claude_api_format(provider: &Provider) -> &'static str {
    let meta = provider.meta.as_ref();
    resolve_claude_api_format_from_settings(
        meta.and_then(|meta| meta.provider_type.as_deref()),
        meta.and_then(|meta| meta.api_format.as_deref()),
        &provider.settings_config,
    )
}

pub fn normalize_anthropic_messages_for_provider(
    body: &mut Value,
    provider: &Provider,
    api_format: &str,
) -> bool {
    if api_format.trim() != "anthropic" {
        return false;
    }

    let mut changed = if should_normalize_anthropic_tool_thinking_history(
        &provider.settings_config,
        body,
        api_format,
    ) {
        normalize_anthropic_tool_thinking_history(body)
    } else {
        false
    };
    changed |= crate::proxy_core::normalize_deepseek_thinking_disabled_strip_effort(
        body,
        &provider.settings_config,
    );
    changed
}

pub fn transform_claude_request_for_api_format(
    body: serde_json::Value,
    provider: &Provider,
    api_format: &str,
    session_id: Option<&str>,
    shadow_store: Option<&crate::proxy_core::GeminiShadowStore>,
) -> Result<serde_json::Value, ProxyError> {
    let is_codex_oauth = provider.is_codex_oauth();

    // Copilot 场景：优先从 metadata.user_id 提取 session ID 作为 cache key
    // 格式: "uuid_sessionId" → 提取 "_" 后面的部分作为 session 标识
    // 同一会话的请求共享 cache key，提升 Copilot 缓存命中率
    let is_copilot = is_copilot_prompt_cache_provider(
        provider
            .meta
            .as_ref()
            .and_then(|m| m.provider_type.as_deref()),
        &provider.settings_config,
    );
    let explicit_cache_key = provider
        .meta
        .as_ref()
        .and_then(|m| m.prompt_cache_key.as_deref());
    let cache_key_resolution = resolve_claude_responses_prompt_cache_key(
        &body,
        explicit_cache_key,
        session_id,
        is_copilot,
    );
    match api_format {
        "openai_responses" => {
            log::debug!(
                "[Cache] OpenAI Responses prompt_cache_key source={cache_key_source}, provider={}, codex_oauth={is_codex_oauth}, has_key={}",
                provider.id,
                cache_key_resolution.key.is_some(),
                cache_key_source = cache_key_resolution.source.as_str()
            );
            // Codex OAuth (ChatGPT Plus/Pro 反代) 需要在请求体里强制 store: false
            // + include: ["reasoning.encrypted_content"]，由 transform 层统一处理。
            let codex_fast_mode = provider.codex_fast_mode_enabled();
            Ok(anthropic_to_openai_responses_request(
                &body,
                cache_key_resolution.key.as_deref(),
                is_codex_oauth,
                codex_fast_mode,
            ))
        }
        "openai_chat" => {
            let preserve_reasoning_content =
                should_preserve_reasoning_content_for_openai_chat(&provider.settings_config, &body);
            let mut result = anthropic_to_openai_chat_request(&body, preserve_reasoning_content);
            // Inject prompt_cache_key only if explicitly configured in meta
            if let Some(key) = provider
                .meta
                .as_ref()
                .and_then(|m| m.prompt_cache_key.as_deref())
            {
                result["prompt_cache_key"] = serde_json::json!(key);
            }
            // 流式请求必须注入 stream_options.include_usage，否则 OpenAI 兼容上游
            // 不在 SSE 末尾吐 usage → 转换出的 Anthropic message_delta 全 0 →
            // 整笔 input/output/cache 漏记（与 Codex Responses→Chat 路径同源）。
            crate::proxy_core::inject_openai_stream_include_usage(&mut result);
            Ok(result)
        }
        "gemini_native" => super::transform_gemini::anthropic_to_gemini_with_shadow(
            body,
            shadow_store,
            Some(&provider.id),
            session_id,
        ),
        _ => Ok(body),
    }
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
    pub fn provider_type(&self, provider: &Provider) -> ProviderType {
        let api_format = self.get_api_format(provider);
        let uses_google_oauth = self
            .extract_key(provider)
            .map(|key| key.starts_with("ya29.") || key.starts_with('{'))
            .unwrap_or(false);
        let meta_provider_type = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.provider_type.as_deref());
        let base_url = self.extract_base_url(provider).ok();

        ProviderType::from_provider_kind(infer_claude_provider_kind(
            api_format,
            uses_google_oauth,
            meta_provider_type,
            base_url.as_deref(),
            &provider.settings_config,
        ))
    }

    /// 检测是否为 Codex OAuth 供应商（ChatGPT Plus/Pro 反代）
    fn is_codex_oauth(&self, provider: &Provider) -> bool {
        if let Some(meta) = provider.meta.as_ref() {
            if meta.provider_type.as_deref() == Some("codex_oauth") {
                return true;
            }
        }
        false
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

    /// 从 Provider 配置中提取 API Key
    fn extract_key(&self, provider: &Provider) -> Option<String> {
        self.extract_auth_key(provider).map(|auth_key| auth_key.key)
    }

    fn extract_auth_key(&self, provider: &Provider) -> Option<ClaudeAuthKey> {
        let auth_key = extract_claude_auth_key_from_settings(&provider.settings_config);
        match auth_key.as_ref().map(|auth_key| auth_key.source) {
            Some(ClaudeAuthKeySource::AnthropicAuthToken) => {
                log::debug!("[Claude] 使用 ANTHROPIC_AUTH_TOKEN");
            }
            Some(ClaudeAuthKeySource::AnthropicApiKey) => {
                log::debug!("[Claude] 使用 ANTHROPIC_API_KEY");
            }
            Some(ClaudeAuthKeySource::OpenRouterApiKey) => {
                log::debug!("[Claude] 使用 OPENROUTER_API_KEY");
            }
            Some(ClaudeAuthKeySource::OpenAiApiKey) => {
                log::debug!("[Claude] 使用 OPENAI_API_KEY");
            }
            Some(ClaudeAuthKeySource::GeminiApiKey) => {
                log::debug!("[Claude] 使用 GEMINI_API_KEY");
            }
            Some(ClaudeAuthKeySource::DirectApiKey) => {
                log::debug!("[Claude] 使用 apiKey/api_key");
            }
            None => {
                log::warn!("[Claude] 未找到有效的 API Key");
            }
        }
        auth_key
    }

    /// 根据 env 中填写的变量名推断 Anthropic 默认走哪种鉴权策略。
    ///
    /// 与 Anthropic SDK 原生语义保持一致：
    /// - `ANTHROPIC_AUTH_TOKEN` → `ClaudeAuth`（发送 `Authorization: Bearer`）
    /// - `ANTHROPIC_API_KEY`    → `Anthropic` （发送 `x-api-key`）
    ///
    /// 优先级与 [`extract_key`] 一致；两者都缺时返回 `None` 由调用方决定 fallback。
    fn infer_anthropic_auth_strategy(source: ClaudeAuthKeySource) -> Option<AuthStrategy> {
        match source {
            ClaudeAuthKeySource::AnthropicAuthToken => Some(AuthStrategy::ClaudeAuth),
            ClaudeAuthKeySource::AnthropicApiKey => Some(AuthStrategy::Anthropic),
            _ => None,
        }
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
        extract_claude_base_url_from_settings(
            self.is_codex_oauth(provider),
            &provider.settings_config,
        )
        .ok_or_else(|| ProxyError::ConfigError("Claude Provider 缺少 base_url 配置".to_string()))
    }

    fn extract_auth(&self, provider: &Provider) -> Option<AuthInfo> {
        let provider_type = self.provider_type(provider);

        // GitHub Copilot 使用特殊的认证策略
        // 实际的 token 会在代理请求时动态获取
        if provider_type == ProviderType::GitHubCopilot {
            // 返回一个占位符，实际 token 由 CopilotAuthManager 动态提供
            return Some(AuthInfo::new(
                "copilot_placeholder".to_string(),
                AuthStrategy::GitHubCopilot,
            ));
        }

        // Codex OAuth (ChatGPT Plus/Pro) 同样使用占位符
        // 实际的 access_token 由 CodexOAuthManager 动态提供
        if provider_type == ProviderType::CodexOAuth {
            return Some(AuthInfo::new(
                "codex_oauth_placeholder".to_string(),
                AuthStrategy::CodexOAuth,
            ));
        }

        let auth_key = self.extract_auth_key(provider)?;
        let key = auth_key.key;

        match provider_type {
            ProviderType::GeminiCli => {
                // Parse stored OAuth JSON and only attach access_token when
                // it's actually usable. `parse_oauth_credentials` accepts
                // refresh-token-only JSON (which is legitimate before the
                // first refresh) and also surfaces `{"access_token": "", ...}`
                // for expired credentials. In both cases we would otherwise
                // send `Authorization: Bearer ` to upstream and get a 401.
                //
                // CC Switch does not currently exchange the refresh_token for
                // a fresh access_token. Until that path exists, degrade to
                // plain GoogleOAuth strategy (which still sends the raw key
                // as a fallback) and log loudly so users know to refresh
                // their `~/.gemini/oauth_creds.json`.
                match super::gemini::GeminiAdapter::new().parse_oauth_credentials(&key) {
                    Some(creds) if !creds.access_token.is_empty() => {
                        Some(AuthInfo::with_access_token(key, creds.access_token))
                    }
                    Some(_) => {
                        log::warn!(
                            "[Gemini OAuth] access_token missing or empty for provider `{}`; \
                             bearer auth will likely fail with 401. Refresh \
                             ~/.gemini/oauth_creds.json via the gemini CLI to obtain a new token.",
                            provider.id
                        );
                        Some(AuthInfo::new(key, AuthStrategy::GoogleOAuth))
                    }
                    None => Some(AuthInfo::new(key, AuthStrategy::GoogleOAuth)),
                }
            }
            ProviderType::Gemini => Some(AuthInfo::new(key, AuthStrategy::Google)),
            ProviderType::OpenRouter => Some(AuthInfo::new(key, AuthStrategy::Bearer)),
            ProviderType::ClaudeAuth => Some(AuthInfo::new(key, AuthStrategy::ClaudeAuth)),
            _ => {
                // 按 env 中的变量名推断鉴权策略，对齐 Anthropic SDK 语义：
                // ANTHROPIC_AUTH_TOKEN → Authorization: Bearer
                // ANTHROPIC_API_KEY    → x-api-key
                // 其他来源（apiKey 直填等）默认走 x-api-key（Anthropic 官方协议）。
                let strategy = Self::infer_anthropic_auth_strategy(auth_key.source)
                    .unwrap_or(AuthStrategy::Anthropic);
                Some(AuthInfo::new(key, strategy))
            }
        }
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        build_claude_upstream_url(base_url, endpoint)
    }

    fn get_auth_headers(
        &self,
        auth: &AuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        let static_kind = match auth.strategy {
            AuthStrategy::Anthropic => Some(ClaudeAuthHeaderKind::AnthropicApiKey),
            AuthStrategy::ClaudeAuth | AuthStrategy::Bearer => Some(ClaudeAuthHeaderKind::Bearer),
            AuthStrategy::Google => Some(ClaudeAuthHeaderKind::GoogleApiKey),
            AuthStrategy::GoogleOAuth => Some(ClaudeAuthHeaderKind::GoogleOAuth),
            AuthStrategy::CodexOAuth => Some(ClaudeAuthHeaderKind::CodexOAuth),
            AuthStrategy::GitHubCopilot => None,
        };
        if let Some(kind) = static_kind {
            return build_claude_auth_headers(kind, &auth.api_key, auth.access_token.as_deref())
                .map_err(|error| ProxyError::AuthError(error.to_string()));
        }

        Ok(match auth.strategy {
            AuthStrategy::GitHubCopilot => {
                let request_id = uuid::Uuid::new_v4().to_string();
                build_copilot_auth_headers(CopilotAuthHeadersInput {
                    api_key: &auth.api_key,
                    request_id: &request_id,
                    editor_version: super::copilot_auth::COPILOT_EDITOR_VERSION,
                    editor_plugin_version: super::copilot_auth::COPILOT_PLUGIN_VERSION,
                    integration_id: super::copilot_auth::COPILOT_INTEGRATION_ID,
                    user_agent: super::copilot_auth::COPILOT_USER_AGENT,
                    github_api_version: super::copilot_auth::COPILOT_API_VERSION,
                })
                .map_err(|error| ProxyError::AuthError(error.to_string()))?
            }
            _ => unreachable!("static auth strategies are delegated to proxy-core"),
        })
    }

    fn needs_transform(&self, provider: &Provider) -> bool {
        // GitHub Copilot / Codex OAuth 总是需要格式转换
        if matches!(
            self.provider_type(provider),
            ProviderType::GitHubCopilot | ProviderType::CodexOAuth
        ) {
            return true;
        }

        // 根据 api_format 配置决定是否需要格式转换
        // - "anthropic" (默认): 直接透传，无需转换
        // - "openai_chat": 需要 Anthropic ↔ OpenAI Chat Completions 格式转换
        // - "openai_responses": 需要 Anthropic ↔ OpenAI Responses API 格式转换
        claude_api_format_needs_transform(self.get_api_format(provider))
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
        // Heuristic: detect response format by presence of top-level fields.
        // The ProviderAdapter trait's transform_response doesn't receive the Provider
        // config, so we can't check api_format here. Instead we rely on the fact that
        // Responses API always returns "output" while Chat Completions returns "choices".
        // This is safe because the two formats are structurally disjoint.
        if body.get("candidates").is_some() || body.get("promptFeedback").is_some() {
            super::transform_gemini::gemini_to_anthropic(body)
        } else if body.get("output").is_some() {
            openai_responses_to_anthropic_message(&body).map_err(ProxyError::TransformError)
        } else {
            openai_chat_to_anthropic_message(&body).map_err(ProxyError::TransformError)
        }
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
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
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
        assert_eq!(auth.strategy, AuthStrategy::Anthropic);
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
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
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
        assert_eq!(auth.strategy, AuthStrategy::Anthropic);
    }

    #[test]
    fn test_get_auth_headers_anthropic_emits_x_api_key() {
        let adapter = ClaudeAdapter::new();
        let auth = AuthInfo::new("sk-ant-test".to_string(), AuthStrategy::Anthropic);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "x-api-key");
        assert_eq!(headers[0].1.to_str().unwrap(), "sk-ant-test");
    }

    #[test]
    fn test_get_auth_headers_claude_auth_emits_authorization_bearer() {
        let adapter = ClaudeAdapter::new();
        let auth = AuthInfo::new("sk-relay-test".to_string(), AuthStrategy::ClaudeAuth);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1.to_str().unwrap(), "Bearer sk-relay-test");
    }

    #[test]
    fn test_get_auth_headers_bearer_emits_authorization_bearer() {
        let adapter = ClaudeAdapter::new();
        let auth = AuthInfo::new("sk-or-test".to_string(), AuthStrategy::Bearer);

        let headers = adapter.get_auth_headers(&auth).unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(headers[0].1.to_str().unwrap(), "Bearer sk-or-test");
    }

    #[test]
    fn test_get_auth_headers_google_oauth_emits_bearer_and_client_marker() {
        let adapter = ClaudeAdapter::new();
        let auth = AuthInfo::with_access_token(
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
        let auth = AuthInfo::new("chatgpt-token".to_string(), AuthStrategy::CodexOAuth);

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
        let auth = AuthInfo::new("copilot-token".to_string(), AuthStrategy::GitHubCopilot);

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
        let auth = AuthInfo::new(
            "sk-ant-bad\r\nX-Inject: 1".to_string(),
            AuthStrategy::Anthropic,
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
        assert_eq!(auth.strategy, AuthStrategy::Bearer);
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
        assert_eq!(auth.strategy, AuthStrategy::Google);
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
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
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
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
    }

    /// Regression: a Gemini OAuth credential JSON that carries only a
    /// refresh_token (no active access_token) must not be surfaced as an
    /// `AuthInfo` whose bearer would be empty. Without the guard, downstream
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
            "empty access_token leaked into AuthInfo"
        );
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
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
            "empty access_token leaked into AuthInfo"
        );
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
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
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
    }

    /// 回归:从 oauth_creds.json 复制时常带前导换行/空格。未 trim 时
    /// `starts_with('{')` 会落空,导致误分类为 `ProviderType::Gemini`,再
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

        assert_eq!(adapter.provider_type(&provider), ProviderType::GeminiCli);

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.access_token.as_deref(), Some("ya29.valid"));
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
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

        assert_eq!(adapter.provider_type(&provider), ProviderType::GeminiCli);

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.access_token.as_deref(), Some("ya29.raw-token-value"));
        assert_eq!(auth.strategy, AuthStrategy::GoogleOAuth);
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
        assert_eq!(adapter.provider_type(&anthropic), ProviderType::Claude);

        // OpenRouter
        let openrouter = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test"
            }
        }));
        assert_eq!(adapter.provider_type(&openrouter), ProviderType::OpenRouter);

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
            ProviderType::ClaudeAuth
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
            ProviderType::Gemini
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
        assert_eq!(adapter.provider_type(&copilot), ProviderType::GitHubCopilot);
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
            ProviderType::GitHubCopilot
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
        assert_eq!(auth.strategy, AuthStrategy::GitHubCopilot);
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
            crate::proxy_core::ANTHROPIC_TOOL_THINKING_PLACEHOLDER
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
            crate::proxy_core::ANTHROPIC_TOOL_THINKING_PLACEHOLDER
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
            crate::proxy_core::ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
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
        crate::proxy_core::normalize_deepseek_thinking_disabled_strip_effort(
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
