use crate::claude_auth::{extract_claude_auth_key_from_settings, ClaudeAuthKeySource};
use crate::domain::ProviderKind;
use crate::gemini_auth::GeminiOAuthCredentials;
use crate::secret::mask_secret;
use serde_json::{Map, Value};

/// Provider credential selected by a host adapter.
///
/// This is distinct from the port-layer `AuthInfo`, which represents already
/// materialized request headers for the independent proxy engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAuthInfo {
    pub api_key: String,
    pub strategy: ProviderAuthStrategy,
    pub access_token: Option<String>,
}

impl ProviderAuthInfo {
    pub fn new(api_key: String, strategy: ProviderAuthStrategy) -> Self {
        Self {
            api_key,
            strategy,
            access_token: None,
        }
    }

    pub fn with_access_token(api_key: String, access_token: String) -> Self {
        Self {
            api_key,
            strategy: ProviderAuthStrategy::GoogleOAuth,
            access_token: Some(access_token),
        }
    }

    pub fn masked_key(&self) -> String {
        mask_secret(&self.api_key)
    }

    pub fn masked_access_token(&self) -> Option<String> {
        self.access_token.as_ref().map(|token| mask_secret(token))
    }
}

/// Provider authentication mode selected before request-header construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthStrategy {
    Anthropic,
    ClaudeAuth,
    Bearer,
    Google,
    GoogleOAuth,
    GitHubCopilot,
    CodexOAuth,
}

pub fn gemini_auth_strategy_for_provider_kind(
    provider_kind: &ProviderKind,
) -> ProviderAuthStrategy {
    match provider_kind {
        ProviderKind::GeminiCli => ProviderAuthStrategy::GoogleOAuth,
        _ => ProviderAuthStrategy::Google,
    }
}

pub fn claude_anthropic_auth_strategy_for_key_source(
    source: ClaudeAuthKeySource,
) -> Option<ProviderAuthStrategy> {
    match source {
        ClaudeAuthKeySource::AnthropicAuthToken => Some(ProviderAuthStrategy::ClaudeAuth),
        ClaudeAuthKeySource::AnthropicApiKey => Some(ProviderAuthStrategy::Anthropic),
        _ => None,
    }
}

pub fn claude_static_auth_strategy_for_provider_kind(
    provider_kind: &ProviderKind,
) -> Option<ProviderAuthStrategy> {
    match provider_kind {
        ProviderKind::Gemini => Some(ProviderAuthStrategy::Google),
        ProviderKind::OpenRouter => Some(ProviderAuthStrategy::Bearer),
        ProviderKind::ClaudeAuth => Some(ProviderAuthStrategy::ClaudeAuth),
        _ => None,
    }
}

pub fn codex_auth_info_from_api_key(api_key: String) -> ProviderAuthInfo {
    ProviderAuthInfo::new(api_key, ProviderAuthStrategy::Bearer)
}

pub fn gemini_auth_info_from_api_key(
    api_key: String,
    strategy: ProviderAuthStrategy,
    oauth_credentials: Option<&GeminiOAuthCredentials>,
) -> ProviderAuthInfo {
    match (strategy, oauth_credentials) {
        (ProviderAuthStrategy::GoogleOAuth, Some(credentials)) => {
            ProviderAuthInfo::with_access_token(api_key, credentials.access_token.clone())
        }
        _ => ProviderAuthInfo::new(api_key, ProviderAuthStrategy::Google),
    }
}

pub fn settings_config_with_channel_auth_key(
    app_type: &str,
    settings_config: &Value,
    key_value: &str,
) -> Value {
    let mut settings = settings_config.clone();
    match app_type {
        "claude" | "claude-desktop" => {
            match extract_claude_auth_key_from_settings(settings_config)
                .map(|auth_key| auth_key.source)
                .unwrap_or(ClaudeAuthKeySource::AnthropicApiKey)
            {
                ClaudeAuthKeySource::AnthropicAuthToken => {
                    set_env_auth_key(&mut settings, "ANTHROPIC_AUTH_TOKEN", key_value)
                }
                ClaudeAuthKeySource::AnthropicApiKey => {
                    set_env_auth_key(&mut settings, "ANTHROPIC_API_KEY", key_value)
                }
                ClaudeAuthKeySource::OpenRouterApiKey => {
                    set_env_auth_key(&mut settings, "OPENROUTER_API_KEY", key_value)
                }
                ClaudeAuthKeySource::OpenAiApiKey => {
                    set_env_auth_key(&mut settings, "OPENAI_API_KEY", key_value)
                }
                ClaudeAuthKeySource::GeminiApiKey => {
                    set_env_auth_key(&mut settings, "GEMINI_API_KEY", key_value)
                }
                ClaudeAuthKeySource::DirectApiKey => set_direct_auth_key(&mut settings, key_value),
            }
        }
        "gemini" => set_env_auth_key(&mut settings, "GEMINI_API_KEY", key_value),
        "codex" | "opencode" | "openclaw" | "hermes" => {
            set_env_auth_key(&mut settings, "OPENAI_API_KEY", key_value)
        }
        _ => set_env_auth_key(&mut settings, "OPENAI_API_KEY", key_value),
    }
    settings
}

pub fn channel_auth_profile_missing_key_error_message(channel_id: &str, key_ref: &str) -> String {
    format!(
        "channel auth profile references missing or disabled key: channel_id={channel_id}, key_ref={key_ref}"
    )
}

fn set_env_auth_key(settings: &mut Value, key_name: &str, key_value: &str) {
    ensure_object(settings)
        .entry("env".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let env = ensure_object(settings)
        .get_mut("env")
        .expect("env was inserted");
    ensure_object(env).insert(
        key_name.to_string(),
        Value::String(key_value.trim().to_string()),
    );
}

fn set_direct_auth_key(settings: &mut Value, key_value: &str) {
    ensure_object(settings).insert(
        "apiKey".to_string(),
        Value::String(key_value.trim().to_string()),
    );
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("value is object")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_auth_info_masks_key_with_shared_secret_policy() {
        let auth = ProviderAuthInfo::new(
            "sk-1234567890abcdef".to_string(),
            ProviderAuthStrategy::Bearer,
        );

        assert_eq!(auth.masked_key(), "sk-1...cdef");
    }

    #[test]
    fn provider_auth_info_new_has_no_access_token() {
        let auth = ProviderAuthInfo::new("api-key".to_string(), ProviderAuthStrategy::Bearer);

        assert!(auth.access_token.is_none());
    }

    #[test]
    fn provider_auth_info_with_access_token_selects_google_oauth() {
        let auth = ProviderAuthInfo::with_access_token(
            "refresh-token".to_string(),
            "ya29.access-token-12345".to_string(),
        );

        assert_eq!(auth.api_key, "refresh-token");
        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
        assert_eq!(
            auth.access_token,
            Some("ya29.access-token-12345".to_string())
        );
    }

    #[test]
    fn provider_auth_info_masks_access_token_when_present() {
        let auth = ProviderAuthInfo::with_access_token(
            "refresh".to_string(),
            "ya29.1234567890abcdef".to_string(),
        );

        assert_eq!(auth.masked_access_token(), Some("ya29...cdef".to_string()));
    }

    #[test]
    fn provider_auth_info_has_no_masked_access_token_when_absent() {
        let auth = ProviderAuthInfo::new("api-key".to_string(), ProviderAuthStrategy::Bearer);

        assert!(auth.masked_access_token().is_none());
    }

    #[test]
    fn codex_auth_info_from_api_key_selects_bearer_strategy() {
        let auth = codex_auth_info_from_api_key("sk-codex".to_string());

        assert_eq!(auth.api_key, "sk-codex");
        assert_eq!(auth.strategy, ProviderAuthStrategy::Bearer);
        assert_eq!(auth.access_token, None);
    }

    #[test]
    fn gemini_auth_info_from_api_key_selects_oauth_when_credentials_parse() {
        let credentials = GeminiOAuthCredentials {
            access_token: "ya29.access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            client_id: None,
            client_secret: None,
        };

        let auth = gemini_auth_info_from_api_key(
            "refresh-token".to_string(),
            ProviderAuthStrategy::GoogleOAuth,
            Some(&credentials),
        );

        assert_eq!(auth.api_key, "refresh-token");
        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
        assert_eq!(auth.access_token.as_deref(), Some("ya29.access-token"));
    }

    #[test]
    fn gemini_auth_info_from_api_key_preserves_empty_oauth_token_contract() {
        let credentials = GeminiOAuthCredentials {
            access_token: String::new(),
            refresh_token: Some("refresh-token".to_string()),
            client_id: None,
            client_secret: None,
        };

        let auth = gemini_auth_info_from_api_key(
            "refresh-token".to_string(),
            ProviderAuthStrategy::GoogleOAuth,
            Some(&credentials),
        );

        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
        assert_eq!(auth.access_token.as_deref(), Some(""));
    }

    #[test]
    fn gemini_auth_info_from_api_key_falls_back_to_google_api_key() {
        let oauth_without_credentials = gemini_auth_info_from_api_key(
            "AIza-api-key".to_string(),
            ProviderAuthStrategy::GoogleOAuth,
            None,
        );
        assert_eq!(oauth_without_credentials.api_key, "AIza-api-key");
        assert_eq!(
            oauth_without_credentials.strategy,
            ProviderAuthStrategy::Google
        );
        assert_eq!(oauth_without_credentials.access_token, None);

        let api_key = gemini_auth_info_from_api_key(
            "AIza-api-key".to_string(),
            ProviderAuthStrategy::Google,
            None,
        );
        assert_eq!(api_key.strategy, ProviderAuthStrategy::Google);
        assert_eq!(api_key.access_token, None);
    }

    #[test]
    fn provider_auth_strategies_are_distinct() {
        let strategies = [
            ProviderAuthStrategy::Anthropic,
            ProviderAuthStrategy::ClaudeAuth,
            ProviderAuthStrategy::Bearer,
            ProviderAuthStrategy::Google,
            ProviderAuthStrategy::GoogleOAuth,
            ProviderAuthStrategy::GitHubCopilot,
            ProviderAuthStrategy::CodexOAuth,
        ];

        for (index, left) in strategies.iter().enumerate() {
            for (right_index, right) in strategies.iter().enumerate() {
                if index == right_index {
                    assert_eq!(left, right);
                } else {
                    assert_ne!(left, right);
                }
            }
        }
    }

    #[test]
    fn gemini_auth_strategy_follows_provider_kind() {
        assert_eq!(
            gemini_auth_strategy_for_provider_kind(&ProviderKind::GeminiCli),
            ProviderAuthStrategy::GoogleOAuth
        );
        assert_eq!(
            gemini_auth_strategy_for_provider_kind(&ProviderKind::Gemini),
            ProviderAuthStrategy::Google
        );
        assert_eq!(
            gemini_auth_strategy_for_provider_kind(&ProviderKind::Claude),
            ProviderAuthStrategy::Google
        );
    }

    #[test]
    fn claude_anthropic_auth_strategy_follows_key_source() {
        assert_eq!(
            claude_anthropic_auth_strategy_for_key_source(ClaudeAuthKeySource::AnthropicAuthToken),
            Some(ProviderAuthStrategy::ClaudeAuth)
        );
        assert_eq!(
            claude_anthropic_auth_strategy_for_key_source(ClaudeAuthKeySource::AnthropicApiKey),
            Some(ProviderAuthStrategy::Anthropic)
        );
        assert_eq!(
            claude_anthropic_auth_strategy_for_key_source(ClaudeAuthKeySource::OpenRouterApiKey),
            None
        );
        assert_eq!(
            claude_anthropic_auth_strategy_for_key_source(ClaudeAuthKeySource::DirectApiKey),
            None
        );
    }

    #[test]
    fn claude_static_auth_strategy_follows_provider_kind() {
        assert_eq!(
            claude_static_auth_strategy_for_provider_kind(&ProviderKind::Gemini),
            Some(ProviderAuthStrategy::Google)
        );
        assert_eq!(
            claude_static_auth_strategy_for_provider_kind(&ProviderKind::OpenRouter),
            Some(ProviderAuthStrategy::Bearer)
        );
        assert_eq!(
            claude_static_auth_strategy_for_provider_kind(&ProviderKind::ClaudeAuth),
            Some(ProviderAuthStrategy::ClaudeAuth)
        );
        assert_eq!(
            claude_static_auth_strategy_for_provider_kind(&ProviderKind::GeminiCli),
            None
        );
        assert_eq!(
            claude_static_auth_strategy_for_provider_kind(&ProviderKind::Claude),
            None
        );
    }

    #[test]
    fn channel_auth_key_settings_preserve_claude_source_shape() {
        let anthropic = settings_config_with_channel_auth_key(
            "claude",
            &json!({"env": {"ANTHROPIC_API_KEY": "old-key"}}),
            " new-key ",
        );
        assert_eq!(
            anthropic
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("new-key")
        );

        let openrouter = settings_config_with_channel_auth_key(
            "claude",
            &json!({"env": {"OPENROUTER_API_KEY": "old-key"}}),
            "router-key",
        );
        assert_eq!(
            openrouter
                .pointer("/env/OPENROUTER_API_KEY")
                .and_then(Value::as_str),
            Some("router-key")
        );

        let direct = settings_config_with_channel_auth_key(
            "claude",
            &json!({"api_key": "old-direct"}),
            "direct-key",
        );
        assert_eq!(
            direct.get("apiKey").and_then(Value::as_str),
            Some("direct-key")
        );
    }

    #[test]
    fn channel_auth_key_settings_use_app_specific_env_defaults() {
        let gemini = settings_config_with_channel_auth_key("gemini", &json!({}), "gemini-key");
        assert_eq!(
            gemini
                .pointer("/env/GEMINI_API_KEY")
                .and_then(Value::as_str),
            Some("gemini-key")
        );

        let codex = settings_config_with_channel_auth_key("codex", &json!({}), "openai-key");
        assert_eq!(
            codex.pointer("/env/OPENAI_API_KEY").and_then(Value::as_str),
            Some("openai-key")
        );
    }

    #[test]
    fn channel_auth_profile_missing_key_error_message_preserves_runtime_text() {
        assert_eq!(
            channel_auth_profile_missing_key_error_message("channel-a", "primary"),
            "channel auth profile references missing or disabled key: channel_id=channel-a, key_ref=primary"
        );
    }
}
