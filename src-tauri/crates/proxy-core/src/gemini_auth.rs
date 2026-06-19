//! Host-neutral Gemini authentication helpers.

use serde_json::Value;

pub const GEMINI_API_KEY_ENV: &str = "GEMINI_API_KEY";
pub const GOOGLE_GEMINI_BASE_URL_ENV: &str = "GOOGLE_GEMINI_BASE_URL";

/// Parsed Gemini OAuth credential material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeminiOAuthCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

impl GeminiOAuthCredentials {
    /// Checks whether a refresh is needed because only refresh material exists.
    pub fn needs_refresh(&self) -> bool {
        self.refresh_token.is_some() && self.access_token.is_empty()
    }

    /// Checks whether enough client metadata exists to refresh the token.
    pub fn can_refresh(&self) -> bool {
        self.refresh_token.is_some() && self.client_id.is_some() && self.client_secret.is_some()
    }
}

/// Parse Gemini CLI OAuth credentials from a pasted access token or JSON blob.
pub fn parse_gemini_oauth_credentials(key: &str) -> Option<GeminiOAuthCredentials> {
    let key = key.trim();

    if key.starts_with("ya29.") {
        return Some(GeminiOAuthCredentials {
            access_token: key.to_string(),
            refresh_token: None,
            client_id: None,
            client_secret: None,
        });
    }

    if !key.starts_with('{') {
        return None;
    }

    let json = serde_json::from_str::<serde_json::Value>(key).ok()?;
    let access_token = json
        .get("access_token")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .unwrap_or_default();
    let refresh_token = json
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let client_id = json
        .get("client_id")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let client_secret = json
        .get("client_secret")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    if access_token.is_empty() && refresh_token.is_none() {
        return None;
    }

    Some(GeminiOAuthCredentials {
        access_token,
        refresh_token,
        client_id,
        client_secret,
    })
}

pub fn extract_gemini_api_key_from_settings(settings: &Value) -> Option<String> {
    if let Some(key) = settings
        .get("env")
        .and_then(|env| env.get(GEMINI_API_KEY_ENV))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        return Some(key.to_string());
    }

    settings
        .get("apiKey")
        .or_else(|| settings.get("api_key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(ToString::to_string)
}

pub fn extract_gemini_base_url_from_settings(settings: &Value) -> Option<String> {
    if let Some(url) = settings
        .get("env")
        .and_then(|env| env.get(GOOGLE_GEMINI_BASE_URL_ENV))
        .and_then(Value::as_str)
    {
        return Some(url.trim_end_matches('/').to_string());
    }

    settings
        .get("base_url")
        .or_else(|| settings.get("baseURL"))
        .and_then(Value::as_str)
        .map(|url| url.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_direct_access_token() {
        let credentials = parse_gemini_oauth_credentials("ya29.test-access-token").unwrap();

        assert_eq!(credentials.access_token, "ya29.test-access-token");
        assert_eq!(credentials.refresh_token, None);
        assert!(!credentials.needs_refresh());
        assert!(!credentials.can_refresh());
    }

    #[test]
    fn parses_json_credentials() {
        let credentials = parse_gemini_oauth_credentials(
            r#"{
                "access_token": "ya29.test",
                "refresh_token": "1//refresh",
                "client_id": "client",
                "client_secret": "secret"
            }"#,
        )
        .unwrap();

        assert_eq!(credentials.access_token, "ya29.test");
        assert_eq!(credentials.refresh_token.as_deref(), Some("1//refresh"));
        assert_eq!(credentials.client_id.as_deref(), Some("client"));
        assert_eq!(credentials.client_secret.as_deref(), Some("secret"));
        assert!(credentials.can_refresh());
    }

    #[test]
    fn treats_refresh_only_json_as_refreshable_material() {
        let credentials = parse_gemini_oauth_credentials(
            r#"{
                "refresh_token": "1//refresh",
                "client_id": "client",
                "client_secret": "secret"
            }"#,
        )
        .unwrap();

        assert!(credentials.access_token.is_empty());
        assert!(credentials.needs_refresh());
        assert!(credentials.can_refresh());
    }

    #[test]
    fn rejects_api_keys_and_invalid_json() {
        assert!(parse_gemini_oauth_credentials("AIza-api-key").is_none());
        assert!(parse_gemini_oauth_credentials("invalid-json{").is_none());
        assert!(parse_gemini_oauth_credentials(r#"{"client_id":"client"}"#).is_none());
    }

    #[test]
    fn extracts_gemini_api_key_from_env_before_direct_fields() {
        let settings = json!({
            "env": {
                "GEMINI_API_KEY": " env-key "
            },
            "apiKey": "direct-key"
        });

        assert_eq!(
            extract_gemini_api_key_from_settings(&settings).as_deref(),
            Some("env-key")
        );
    }

    #[test]
    fn extracts_gemini_api_key_from_direct_aliases() {
        assert_eq!(
            extract_gemini_api_key_from_settings(&json!({ "apiKey": " direct-key " })).as_deref(),
            Some("direct-key")
        );
        assert_eq!(
            extract_gemini_api_key_from_settings(&json!({ "api_key": " snake-key " })).as_deref(),
            Some("snake-key")
        );
        assert_eq!(
            extract_gemini_api_key_from_settings(&json!({ "apiKey": "   " })),
            None
        );
    }

    #[test]
    fn extracts_gemini_base_url_from_env_before_direct_fields() {
        let settings = json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://env.example.com/v1beta/"
            },
            "base_url": "https://direct.example.com/"
        });

        assert_eq!(
            extract_gemini_base_url_from_settings(&settings).as_deref(),
            Some("https://env.example.com/v1beta")
        );
    }

    #[test]
    fn extracts_gemini_base_url_from_direct_aliases() {
        assert_eq!(
            extract_gemini_base_url_from_settings(
                &json!({ "base_url": "https://snake.example.com/" })
            )
            .as_deref(),
            Some("https://snake.example.com")
        );
        assert_eq!(
            extract_gemini_base_url_from_settings(
                &json!({ "baseURL": "https://camel.example.com/" })
            )
            .as_deref(),
            Some("https://camel.example.com")
        );
    }
}
