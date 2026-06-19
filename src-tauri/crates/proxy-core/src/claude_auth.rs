use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeAuthKeySource {
    AnthropicAuthToken,
    AnthropicApiKey,
    OpenRouterApiKey,
    OpenAiApiKey,
    GeminiApiKey,
    DirectApiKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeAuthKey {
    pub key: String,
    pub source: ClaudeAuthKeySource,
}

pub fn extract_claude_auth_key_from_settings(settings_config: &Value) -> Option<ClaudeAuthKey> {
    let env = settings_config.get("env");
    [
        (
            env.and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN")),
            ClaudeAuthKeySource::AnthropicAuthToken,
        ),
        (
            env.and_then(|env| env.get("ANTHROPIC_API_KEY")),
            ClaudeAuthKeySource::AnthropicApiKey,
        ),
        (
            env.and_then(|env| env.get("OPENROUTER_API_KEY")),
            ClaudeAuthKeySource::OpenRouterApiKey,
        ),
        (
            env.and_then(|env| env.get("OPENAI_API_KEY")),
            ClaudeAuthKeySource::OpenAiApiKey,
        ),
        (
            env.and_then(|env| env.get("GEMINI_API_KEY")),
            ClaudeAuthKeySource::GeminiApiKey,
        ),
        (
            settings_config
                .get("apiKey")
                .or_else(|| settings_config.get("api_key")),
            ClaudeAuthKeySource::DirectApiKey,
        ),
    ]
    .into_iter()
    .find_map(|(value, source)| {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(|key| ClaudeAuthKey {
                key: key.to_string(),
                source,
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_claude_auth_key_from_settings_by_legacy_priority() {
        let key = extract_claude_auth_key_from_settings(&json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": " token-key ",
                "ANTHROPIC_API_KEY": "api-key",
                "OPENROUTER_API_KEY": "openrouter-key",
                "OPENAI_API_KEY": "openai-key",
                "GEMINI_API_KEY": "gemini-key",
            },
            "apiKey": "direct-key",
        }))
        .expect("auth token");

        assert_eq!(key.key, "token-key");
        assert_eq!(key.source, ClaudeAuthKeySource::AnthropicAuthToken);
    }

    #[test]
    fn extracts_claude_auth_key_from_fallback_sources() {
        let openai = extract_claude_auth_key_from_settings(&json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": " ",
                "OPENAI_API_KEY": " openai-key ",
            },
        }))
        .expect("openai fallback");
        assert_eq!(openai.key, "openai-key");
        assert_eq!(openai.source, ClaudeAuthKeySource::OpenAiApiKey);

        let direct = extract_claude_auth_key_from_settings(&json!({
            "api_key": " direct-key ",
        }))
        .expect("direct fallback");
        assert_eq!(direct.key, "direct-key");
        assert_eq!(direct.source, ClaudeAuthKeySource::DirectApiKey);

        assert!(extract_claude_auth_key_from_settings(&json!({
            "env": { "ANTHROPIC_AUTH_TOKEN": " " },
            "apiKey": "",
        }))
        .is_none());
    }
}
