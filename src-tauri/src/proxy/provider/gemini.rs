//! Gemini (Google) Provider Adapter
//!
//! 支持 API Key 和 OAuth 两种认证方式
//!
//! ## 认证模式
//! - **Gemini**: API Key 认证 (x-goog-api-key)
//! - **GeminiCli**: OAuth Bearer 认证 (用于 Gemini CLI)

use super::ProviderAdapter;
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy_core::api::auth::ProviderAuthInfo;
#[cfg(test)]
use crate::proxy_core::api::auth::ProviderAuthStrategy;
use crate::proxy_core_adapter::{
    build_gemini_upstream_url, provider_gemini_auth_headers, provider_gemini_auth_info,
    required_gemini_provider_base_url,
};

/// Gemini 适配器
pub struct GeminiAdapter;

impl GeminiAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GeminiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAdapter for GeminiAdapter {
    fn name(&self) -> &'static str {
        "Gemini"
    }

    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError> {
        required_gemini_provider_base_url(provider).map_err(ProxyError::ConfigError)
    }

    fn extract_auth(&self, provider: &Provider) -> Option<ProviderAuthInfo> {
        provider_gemini_auth_info(provider)
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        build_gemini_upstream_url(base_url, endpoint)
    }

    fn get_auth_headers(
        &self,
        auth: &ProviderAuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        provider_gemini_auth_headers(auth).map_err(ProxyError::AuthError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Gemini".to_string(),
            settings_config: config,
            website_url: None,
            category: Some("gemini".to_string()),
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
    fn test_extract_base_url_from_env() {
        let adapter = GeminiAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com/v1beta"
            }
        }));

        let url = adapter.extract_base_url(&provider).unwrap();
        assert_eq!(url, "https://generativelanguage.googleapis.com/v1beta");
    }

    #[test]
    fn test_extract_auth_api_key() {
        let adapter = GeminiAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "AIza-test-key-12345678"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "AIza-test-key-12345678");
        assert_eq!(auth.strategy, ProviderAuthStrategy::Google);
        assert!(auth.access_token.is_none());
    }

    #[test]
    fn test_extract_auth_oauth_access_token() {
        let adapter = GeminiAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "ya29.test-access-token-12345"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
        assert_eq!(
            auth.access_token,
            Some("ya29.test-access-token-12345".to_string())
        );
    }

    #[test]
    fn test_extract_auth_oauth_json() {
        let adapter = GeminiAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "{\"access_token\":\"ya29.test-token\",\"refresh_token\":\"1//refresh\"}"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.strategy, ProviderAuthStrategy::GoogleOAuth);
        assert_eq!(auth.access_token, Some("ya29.test-token".to_string()));
    }

    #[test]
    fn test_extract_auth_fallback() {
        let adapter = GeminiAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "AIza-fallback-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "AIza-fallback-key");
    }

    #[test]
    fn test_build_url_dedup() {
        let adapter = GeminiAdapter::new();
        // 模拟 base_url 已包含 /v1beta，endpoint 也包含 /v1beta
        let url = adapter.build_url(
            "https://generativelanguage.googleapis.com/v1beta",
            "/v1beta/models/gemini-pro:generateContent",
        );
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent"
        );
    }

    #[test]
    fn test_build_url_normal() {
        let adapter = GeminiAdapter::new();
        let url = adapter.build_url(
            "https://generativelanguage.googleapis.com/v1beta",
            "/models/gemini-pro:generateContent",
        );
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent"
        );
    }

    #[test]
    fn test_get_auth_headers_api_key() {
        let adapter = GeminiAdapter::new();
        let auth = ProviderAuthInfo::new("gemini-key".to_string(), ProviderAuthStrategy::Google);

        let headers = adapter.get_auth_headers(&auth).unwrap();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0.as_str(), "x-goog-api-key");
        assert_eq!(headers[0].1, http::HeaderValue::from_static("gemini-key"));
    }

    #[test]
    fn test_get_auth_headers_oauth() {
        let adapter = GeminiAdapter::new();
        let auth = ProviderAuthInfo::with_access_token(
            "refresh-token".to_string(),
            "ya29.access-token".to_string(),
        );

        let headers = adapter.get_auth_headers(&auth).unwrap();

        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(
            headers[0].1,
            http::HeaderValue::from_static("Bearer ya29.access-token")
        );
        assert_eq!(headers[1].0.as_str(), "x-goog-api-client");
        assert_eq!(
            headers[1].1,
            http::HeaderValue::from_static("GeminiCLI/1.0")
        );
    }

    #[test]
    fn test_get_auth_headers_rejects_illegal_header_chars() {
        let adapter = GeminiAdapter::new();
        let auth =
            ProviderAuthInfo::new("bad\r\nx-evil: 1".to_string(), ProviderAuthStrategy::Google);

        let result = adapter.get_auth_headers(&auth);

        assert!(matches!(result, Err(ProxyError::AuthError(_))));
    }
}
