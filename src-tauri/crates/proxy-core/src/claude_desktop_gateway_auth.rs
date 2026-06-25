use http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ProxyCoreError;

pub const CLAUDE_DESKTOP_MODEL_CREATED_AT: &str = "2024-01-01T00:00:00Z";
const CLAUDE_ROUTE_PREFIX: &str = "claude-";
const ANTHROPIC_CLAUDE_ROUTE_PREFIX: &str = "anthropic/claude-";
const ONE_M_CONTEXT_MARKER: &str = "[1m]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeDesktopGatewayAuthError {
    MissingAuthorizationHeader,
    InvalidAuthorizationHeader,
    InvalidToken,
}

impl ClaudeDesktopGatewayAuthError {
    pub fn message(self) -> &'static str {
        match self {
            Self::MissingAuthorizationHeader => "Claude Desktop gateway 缺少 Authorization 头",
            Self::InvalidAuthorizationHeader => "Authorization 头格式无效",
            Self::InvalidToken => "Claude Desktop gateway token 无效",
        }
    }
}

pub fn validate_claude_desktop_gateway_bearer_value(
    authorization: Option<&str>,
    expected_token: &str,
) -> Result<(), ClaudeDesktopGatewayAuthError> {
    let value = authorization.ok_or(ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader)?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .unwrap_or("")
        .trim();

    if token != expected_token {
        return Err(ClaudeDesktopGatewayAuthError::InvalidToken);
    }

    Ok(())
}

pub fn validate_claude_desktop_gateway_bearer_header(
    headers: &HeaderMap,
    expected_token: &str,
) -> Result<(), ClaudeDesktopGatewayAuthError> {
    let value = headers
        .get(http::header::AUTHORIZATION)
        .map(|value| {
            value
                .to_str()
                .map_err(|_| ClaudeDesktopGatewayAuthError::InvalidAuthorizationHeader)
        })
        .transpose()?;

    validate_claude_desktop_gateway_bearer_value(value, expected_token)
}

#[derive(Debug, Clone, Copy)]
pub struct ClaudeDesktopProviderValidationInput<'a> {
    pub settings_config: &'a Value,
    pub api_format: Option<&'a str>,
    pub claude_desktop_mode_is_proxy: bool,
    pub provider_type: Option<&'a str>,
    pub is_full_url: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeDesktopDirectProviderValidationIssue {
    SettingsNotObject,
    ApiFormatUnsupported,
    ProxyModeUnsupported,
    ManagedProviderTypeUnsupported,
    FullUrlUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeDesktopProxyProviderConfigValidationIssue {
    SettingsNotObject,
    ApiFormatUnsupported(String),
}

pub fn claude_desktop_routes_support_1m_by_default(provider_type: Option<&str>) -> bool {
    !is_managed_oauth_provider_type(provider_type)
}

pub fn claude_desktop_model_id_is_profile_safe(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.contains(ONE_M_CONTEXT_MARKER) {
        return false;
    }

    let Some(route_tail) = normalized
        .strip_prefix(ANTHROPIC_CLAUDE_ROUTE_PREFIX)
        .or_else(|| normalized.strip_prefix(CLAUDE_ROUTE_PREFIX))
    else {
        return false;
    };

    ["sonnet-", "opus-", "haiku-", "fable-"]
        .iter()
        .any(|prefix| {
            route_tail
                .strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty())
        })
}

pub fn claude_desktop_provider_models_are_profile_safe(settings_config: &Value) -> bool {
    let Some(env) = settings_config.get("env").and_then(Value::as_object) else {
        return true;
    };

    [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
    ]
    .into_iter()
    .filter_map(|key| env.get(key).and_then(Value::as_str))
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .all(claude_desktop_model_id_is_profile_safe)
}

pub fn claude_desktop_proxy_has_base_url_and_key(
    input: ClaudeDesktopProviderValidationInput<'_>,
) -> bool {
    let settings = input.settings_config;
    let env = settings.get("env");
    let has_base_url = env
        .and_then(|value| value.get("ANTHROPIC_BASE_URL"))
        .or_else(|| settings.get("base_url"))
        .or_else(|| settings.get("baseURL"))
        .or_else(|| settings.get("apiEndpoint"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    if is_managed_oauth_provider_type(input.provider_type) {
        return has_base_url;
    }

    let has_key = env
        .and_then(|value| {
            [
                "ANTHROPIC_AUTH_TOKEN",
                "ANTHROPIC_API_KEY",
                "OPENROUTER_API_KEY",
                "OPENAI_API_KEY",
                "GEMINI_API_KEY",
            ]
            .into_iter()
            .find_map(|key| value.get(key))
        })
        .or_else(|| settings.get("apiKey"))
        .or_else(|| settings.get("api_key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    has_base_url && has_key
}

pub fn claude_desktop_direct_provider_validation_issue(
    input: ClaudeDesktopProviderValidationInput<'_>,
) -> Option<ClaudeDesktopDirectProviderValidationIssue> {
    if !input.settings_config.is_object() {
        return Some(ClaudeDesktopDirectProviderValidationIssue::SettingsNotObject);
    }

    if let Some(api_format) = input.api_format {
        if !api_format.trim().is_empty() && api_format != "anthropic" {
            return Some(ClaudeDesktopDirectProviderValidationIssue::ApiFormatUnsupported);
        }
    }

    if input.claude_desktop_mode_is_proxy {
        return Some(ClaudeDesktopDirectProviderValidationIssue::ProxyModeUnsupported);
    }

    if is_managed_oauth_provider_type(input.provider_type) {
        return Some(ClaudeDesktopDirectProviderValidationIssue::ManagedProviderTypeUnsupported);
    }

    if input.is_full_url {
        return Some(ClaudeDesktopDirectProviderValidationIssue::FullUrlUnsupported);
    }

    None
}

pub fn claude_desktop_proxy_provider_config_validation_issue(
    input: ClaudeDesktopProviderValidationInput<'_>,
) -> Option<ClaudeDesktopProxyProviderConfigValidationIssue> {
    if !input.settings_config.is_object() {
        return Some(ClaudeDesktopProxyProviderConfigValidationIssue::SettingsNotObject);
    }

    if let Some(api_format) = input.api_format {
        if !matches!(
            api_format,
            "" | "anthropic" | "openai_chat" | "openai_responses" | "gemini_native"
        ) {
            return Some(
                ClaudeDesktopProxyProviderConfigValidationIssue::ApiFormatUnsupported(
                    api_format.to_string(),
                ),
            );
        }
    }

    None
}

pub fn claude_desktop_provider_selection_error(error: impl std::fmt::Display) -> ProxyCoreError {
    ProxyCoreError::Internal(format!("select claude desktop provider: {error}"))
}

pub fn claude_desktop_gateway_token_error(error: impl std::fmt::Display) -> ProxyCoreError {
    ProxyCoreError::Auth(error.to_string())
}

pub fn claude_desktop_provider_unavailable_error_message() -> &'static str {
    "no available claude desktop provider"
}

pub fn claude_desktop_provider_unavailable_error() -> ProxyCoreError {
    ProxyCoreError::Unavailable(claude_desktop_provider_unavailable_error_message().to_string())
}

fn is_managed_oauth_provider_type(provider_type: Option<&str>) -> bool {
    matches!(provider_type, Some("github_copilot") | Some("codex_oauth"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeDesktopModelRouteInput {
    pub route_id: String,
    pub supports_1m: bool,
}

impl ClaudeDesktopModelRouteInput {
    pub fn new(route_id: impl Into<String>, supports_1m: bool) -> Self {
        Self {
            route_id: route_id.into(),
            supports_1m,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeDesktopModelListItem {
    #[serde(rename = "type")]
    pub object_type: String,
    pub id: String,
    pub created_at: String,
    #[serde(default, rename = "supports1m", skip_serializing_if = "is_false")]
    pub supports_1m: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeDesktopModelListResponse {
    pub data: Vec<ClaudeDesktopModelListItem>,
    pub has_more: bool,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
}

impl ClaudeDesktopModelListResponse {
    pub fn from_routes(routes: impl IntoIterator<Item = ClaudeDesktopModelRouteInput>) -> Self {
        let data: Vec<_> = routes
            .into_iter()
            .map(|route| ClaudeDesktopModelListItem {
                object_type: "model".to_string(),
                id: route.route_id,
                created_at: CLAUDE_DESKTOP_MODEL_CREATED_AT.to_string(),
                supports_1m: route.supports_1m,
            })
            .collect();
        let first_id = data.first().map(|item| item.id.clone());
        let last_id = data.last().map(|item| item.id.clone());

        Self {
            data,
            has_more: false,
            first_id,
            last_id,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::{
        claude_desktop_direct_provider_validation_issue, claude_desktop_gateway_token_error,
        claude_desktop_model_id_is_profile_safe, claude_desktop_provider_models_are_profile_safe,
        claude_desktop_provider_selection_error, claude_desktop_provider_unavailable_error,
        claude_desktop_provider_unavailable_error_message,
        claude_desktop_proxy_has_base_url_and_key,
        claude_desktop_proxy_provider_config_validation_issue,
        claude_desktop_routes_support_1m_by_default, validate_claude_desktop_gateway_bearer_header,
        validate_claude_desktop_gateway_bearer_value, ClaudeDesktopDirectProviderValidationIssue,
        ClaudeDesktopGatewayAuthError, ClaudeDesktopModelListResponse,
        ClaudeDesktopModelRouteInput, ClaudeDesktopProviderValidationInput,
        ClaudeDesktopProxyProviderConfigValidationIssue,
    };
    use crate::error::ProxyCoreError;
    use http::{HeaderMap, HeaderValue};
    use serde_json::json;

    #[test]
    fn gateway_bearer_value_accepts_existing_scheme_variants() {
        validate_claude_desktop_gateway_bearer_value(Some("Bearer gateway-token"), "gateway-token")
            .expect("valid bearer");
        validate_claude_desktop_gateway_bearer_value(
            Some("bearer gateway-token "),
            "gateway-token",
        )
        .expect("valid bearer");
    }

    #[test]
    fn gateway_bearer_value_preserves_existing_error_messages() {
        assert_eq!(
            validate_claude_desktop_gateway_bearer_value(None, "gateway-token").unwrap_err(),
            ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader
        );
        assert_eq!(
            validate_claude_desktop_gateway_bearer_value(
                Some("Basic gateway-token"),
                "gateway-token"
            )
            .unwrap_err(),
            ClaudeDesktopGatewayAuthError::InvalidToken
        );
        assert_eq!(
            ClaudeDesktopGatewayAuthError::InvalidToken.message(),
            "Claude Desktop gateway token 无效"
        );
    }

    #[test]
    fn gateway_bearer_header_reads_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer gateway-token"),
        );

        validate_claude_desktop_gateway_bearer_header(&headers, "gateway-token")
            .expect("valid bearer");

        assert_eq!(
            validate_claude_desktop_gateway_bearer_header(&HeaderMap::new(), "gateway-token")
                .unwrap_err(),
            ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader
        );
    }

    #[test]
    fn provider_routes_disable_default_1m_for_managed_oauth_kinds() {
        assert!(claude_desktop_routes_support_1m_by_default(None));
        assert!(claude_desktop_routes_support_1m_by_default(Some("claude")));
        assert!(!claude_desktop_routes_support_1m_by_default(Some(
            "github_copilot"
        )));
        assert!(!claude_desktop_routes_support_1m_by_default(Some(
            "codex_oauth"
        )));
    }

    #[test]
    fn profile_safe_model_id_rejects_unsafe_claude_desktop_routes() {
        assert!(!claude_desktop_model_id_is_profile_safe(
            "claude-sonnet-4-6 [1m]"
        ));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "  claude-sonnet-4-6  [1M]  "
        ));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-old"));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "claude-3-5-sonnet-20241022"
        ));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "claude-deepseek-v4-pro"
        ));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-gpt-5-4"));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-"));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "anthropic/claude-"
        ));
        assert!(!claude_desktop_model_id_is_profile_safe("sonnet"));
        assert!(!claude_desktop_model_id_is_profile_safe("sonnet-"));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-sonnet-"));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-opus-"));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "anthropic/claude-haiku-"
        ));
        assert!(claude_desktop_model_id_is_profile_safe(
            "  claude-sonnet-4-6  "
        ));
        assert!(claude_desktop_model_id_is_profile_safe(
            "anthropic/claude-opus-4-8"
        ));
        assert!(claude_desktop_model_id_is_profile_safe("claude-fable-4-8"));
    }

    #[test]
    fn provider_models_profile_safe_reads_claude_env_model_fields() {
        assert!(claude_desktop_provider_models_are_profile_safe(&json!({})));
        assert!(claude_desktop_provider_models_are_profile_safe(&json!({
            "env": {
                "ANTHROPIC_MODEL": " claude-sonnet-4-6 ",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "anthropic/claude-opus-4-8"
            }
        })));
        assert!(!claude_desktop_provider_models_are_profile_safe(&json!({
            "env": {
                "ANTHROPIC_MODEL": "claude-sonnet-4-6",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "gpt-5[1m]"
            }
        })));
    }

    #[test]
    fn proxy_credentials_allow_typed_oauth_without_static_key() {
        let proxy_settings = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-provider"
            }
        });
        assert!(claude_desktop_proxy_has_base_url_and_key(
            ClaudeDesktopProviderValidationInput {
                settings_config: &proxy_settings,
                api_format: None,
                claude_desktop_mode_is_proxy: false,
                provider_type: None,
                is_full_url: false,
            }
        ));

        let missing_key_settings = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.com"
            }
        });
        assert!(!claude_desktop_proxy_has_base_url_and_key(
            ClaudeDesktopProviderValidationInput {
                settings_config: &missing_key_settings,
                api_format: None,
                claude_desktop_mode_is_proxy: false,
                provider_type: None,
                is_full_url: false,
            }
        ));
        assert!(claude_desktop_proxy_has_base_url_and_key(
            ClaudeDesktopProviderValidationInput {
                settings_config: &missing_key_settings,
                api_format: None,
                claude_desktop_mode_is_proxy: false,
                provider_type: Some("codex_oauth"),
                is_full_url: false,
            }
        ));
    }

    #[test]
    fn provider_validation_reports_existing_issue_order() {
        let settings = json!({});
        let non_object = json!(null);

        assert_eq!(
            claude_desktop_direct_provider_validation_issue(ClaudeDesktopProviderValidationInput {
                settings_config: &non_object,
                api_format: None,
                claude_desktop_mode_is_proxy: false,
                provider_type: None,
                is_full_url: false,
            }),
            Some(ClaudeDesktopDirectProviderValidationIssue::SettingsNotObject)
        );
        assert_eq!(
            claude_desktop_proxy_provider_config_validation_issue(
                ClaudeDesktopProviderValidationInput {
                    settings_config: &non_object,
                    api_format: None,
                    claude_desktop_mode_is_proxy: false,
                    provider_type: None,
                    is_full_url: false,
                }
            ),
            Some(ClaudeDesktopProxyProviderConfigValidationIssue::SettingsNotObject)
        );

        assert_eq!(
            claude_desktop_direct_provider_validation_issue(ClaudeDesktopProviderValidationInput {
                settings_config: &settings,
                api_format: Some("openai_chat"),
                claude_desktop_mode_is_proxy: false,
                provider_type: None,
                is_full_url: false,
            }),
            Some(ClaudeDesktopDirectProviderValidationIssue::ApiFormatUnsupported)
        );
        assert_eq!(
            claude_desktop_direct_provider_validation_issue(ClaudeDesktopProviderValidationInput {
                settings_config: &settings,
                api_format: Some("anthropic"),
                claude_desktop_mode_is_proxy: true,
                provider_type: None,
                is_full_url: false,
            }),
            Some(ClaudeDesktopDirectProviderValidationIssue::ProxyModeUnsupported)
        );
        assert_eq!(
            claude_desktop_direct_provider_validation_issue(ClaudeDesktopProviderValidationInput {
                settings_config: &settings,
                api_format: Some("anthropic"),
                claude_desktop_mode_is_proxy: false,
                provider_type: Some("github_copilot"),
                is_full_url: false,
            }),
            Some(ClaudeDesktopDirectProviderValidationIssue::ManagedProviderTypeUnsupported)
        );
        assert_eq!(
            claude_desktop_direct_provider_validation_issue(ClaudeDesktopProviderValidationInput {
                settings_config: &settings,
                api_format: Some("anthropic"),
                claude_desktop_mode_is_proxy: false,
                provider_type: None,
                is_full_url: true,
            }),
            Some(ClaudeDesktopDirectProviderValidationIssue::FullUrlUnsupported)
        );
        assert_eq!(
            claude_desktop_proxy_provider_config_validation_issue(
                ClaudeDesktopProviderValidationInput {
                    settings_config: &settings,
                    api_format: Some("unsupported_wire"),
                    claude_desktop_mode_is_proxy: false,
                    provider_type: None,
                    is_full_url: false,
                }
            ),
            Some(
                ClaudeDesktopProxyProviderConfigValidationIssue::ApiFormatUnsupported(
                    "unsupported_wire".to_string()
                )
            )
        );
    }

    #[test]
    fn claude_desktop_provider_selection_errors_preserve_contracts() {
        assert!(matches!(
            claude_desktop_provider_selection_error("router failed"),
            ProxyCoreError::Internal(message)
                if message == "select claude desktop provider: router failed"
        ));
        assert_eq!(
            claude_desktop_provider_unavailable_error_message(),
            "no available claude desktop provider"
        );
        assert!(matches!(
            claude_desktop_provider_unavailable_error(),
            ProxyCoreError::Unavailable(message)
                if message == "no available claude desktop provider"
        ));
        assert!(matches!(
            claude_desktop_gateway_token_error("token store failed"),
            ProxyCoreError::Auth(message) if message == "token store failed"
        ));
    }

    #[test]
    fn model_list_response_serializes_claude_desktop_contract() {
        let response = ClaudeDesktopModelListResponse::from_routes([
            ClaudeDesktopModelRouteInput::new("claude-sonnet-4-6", true),
            ClaudeDesktopModelRouteInput::new("claude-haiku-4-5", false),
        ]);

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(
            value,
            json!({
                "data": [
                    {
                        "type": "model",
                        "id": "claude-sonnet-4-6",
                        "created_at": "2024-01-01T00:00:00Z",
                        "supports1m": true
                    },
                    {
                        "type": "model",
                        "id": "claude-haiku-4-5",
                        "created_at": "2024-01-01T00:00:00Z"
                    }
                ],
                "has_more": false,
                "first_id": "claude-sonnet-4-6",
                "last_id": "claude-haiku-4-5"
            })
        );
    }
}
