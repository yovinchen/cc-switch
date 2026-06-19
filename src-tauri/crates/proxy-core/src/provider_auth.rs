use crate::secret::mask_secret;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
