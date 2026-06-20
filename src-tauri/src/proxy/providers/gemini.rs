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
use crate::proxy_core::{
    build_gemini_auth_headers, build_gemini_upstream_url, extract_gemini_api_key_from_settings,
    extract_gemini_base_url_from_settings, parse_gemini_oauth_credentials, GeminiOAuthCredentials,
    ProviderKind,
};
use crate::proxy_core_adapter::{ProviderAuthInfo, ProviderAuthStrategy};

/// Gemini 适配器
pub struct GeminiAdapter;

impl GeminiAdapter {
    pub fn new() -> Self {
        Self
    }

    /// 获取供应商类型
    ///
    /// 根据 API Key 格式检测：
    /// - GeminiCli: access_token (ya29. 开头) 或 JSON 格式凭证
    /// - Gemini: 普通 API Key
    pub fn provider_type(&self, provider: &Provider) -> ProviderKind {
        if let Some(key) = self.extract_key_raw(provider) {
            if parse_gemini_oauth_credentials(&key).is_some() {
                return ProviderKind::GeminiCli;
            }
        }
        ProviderKind::Gemini
    }

    /// 检测认证类型
    pub fn detect_auth_type(&self, provider: &Provider) -> ProviderAuthStrategy {
        match self.provider_type(provider) {
            ProviderKind::GeminiCli => ProviderAuthStrategy::GoogleOAuth,
            _ => ProviderAuthStrategy::Google,
        }
    }

    /// 解析 OAuth 凭证
    pub fn parse_oauth_credentials(&self, key: &str) -> Option<GeminiOAuthCredentials> {
        parse_gemini_oauth_credentials(key)
    }

    /// 从 Provider 配置中提取原始 API Key
    fn extract_key_raw(&self, provider: &Provider) -> Option<String> {
        extract_gemini_api_key_from_settings(&provider.settings_config)
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
        extract_gemini_base_url_from_settings(&provider.settings_config).ok_or_else(|| {
            ProxyError::ConfigError("Gemini Provider 缺少 base_url 配置".to_string())
        })
    }

    fn extract_auth(&self, provider: &Provider) -> Option<ProviderAuthInfo> {
        let key = self.extract_key_raw(provider)?;
        let strategy = self.detect_auth_type(provider);

        match strategy {
            ProviderAuthStrategy::GoogleOAuth => {
                // 解析 OAuth 凭证
                if let Some(creds) = self.parse_oauth_credentials(&key) {
                    Some(ProviderAuthInfo::with_access_token(key, creds.access_token))
                } else {
                    // 回退到普通 API Key
                    Some(ProviderAuthInfo::new(key, ProviderAuthStrategy::Google))
                }
            }
            _ => Some(ProviderAuthInfo::new(key, ProviderAuthStrategy::Google)),
        }
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        build_gemini_upstream_url(base_url, endpoint)
    }

    fn get_auth_headers(
        &self,
        auth: &ProviderAuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        build_gemini_auth_headers(
            &auth.api_key,
            auth.access_token.as_deref(),
            matches!(auth.strategy, ProviderAuthStrategy::GoogleOAuth),
        )
        .map_err(|error| ProxyError::AuthError(error.to_string()))
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
    fn test_provider_type_detection() {
        let adapter = GeminiAdapter::new();

        // API Key
        let api_key_provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "AIza-test-key"
            }
        }));
        assert_eq!(
            adapter.provider_type(&api_key_provider),
            ProviderKind::Gemini
        );

        // OAuth access_token
        let oauth_provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "ya29.test-token"
            }
        }));
        assert_eq!(
            adapter.provider_type(&oauth_provider),
            ProviderKind::GeminiCli
        );

        // OAuth JSON
        let oauth_json_provider = create_provider(json!({
            "env": {
                "GEMINI_API_KEY": "{\"access_token\":\"ya29.test\"}"
            }
        }));
        assert_eq!(
            adapter.provider_type(&oauth_json_provider),
            ProviderKind::GeminiCli
        );
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
    fn test_parse_oauth_credentials_direct_token() {
        let adapter = GeminiAdapter::new();
        let creds = adapter
            .parse_oauth_credentials("ya29.test-access-token")
            .unwrap();
        assert_eq!(creds.access_token, "ya29.test-access-token");
        assert!(creds.refresh_token.is_none());
    }

    #[test]
    fn test_parse_oauth_credentials_json() {
        let adapter = GeminiAdapter::new();
        let creds = adapter
            .parse_oauth_credentials(
                "{\"access_token\":\"ya29.test\",\"refresh_token\":\"1//refresh\"}",
            )
            .unwrap();
        assert_eq!(creds.access_token, "ya29.test");
        assert_eq!(creds.refresh_token, Some("1//refresh".to_string()));
    }

    #[test]
    fn test_parse_oauth_credentials_invalid() {
        let adapter = GeminiAdapter::new();
        assert!(adapter.parse_oauth_credentials("AIza-api-key").is_none());
        assert!(adapter.parse_oauth_credentials("invalid-json{").is_none());
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
