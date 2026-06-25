use http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ProxyCoreError;

pub const CLAUDE_DESKTOP_MODEL_CREATED_AT: &str = "2024-01-01T00:00:00Z";
const CLAUDE_ROUTE_PREFIX: &str = "claude-";
const ANTHROPIC_CLAUDE_ROUTE_PREFIX: &str = "anthropic/claude-";
const ONE_M_CONTEXT_MARKER: &str = "[1m]";
const CURRENT_OPUS_ROUTE_ID: &str = "claude-opus-4-8";
const LEGACY_OPUS_ROUTE_ID: &str = "claude-opus-4-7";
const DEFAULT_PROXY_ROUTE_IDS: &[&str] = &[
    "claude-sonnet-4-6",
    CURRENT_OPUS_ROUTE_ID,
    "claude-haiku-4-5",
    "claude-fable-5",
];

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

#[derive(Debug, Clone, Copy)]
pub struct ClaudeDesktopProxyRouteInput<'a> {
    pub route_id: &'a str,
    pub upstream_model: &'a str,
    pub label_override: Option<&'a str>,
    pub supports_1m: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeDesktopResolvedProxyRoute {
    pub route_id: String,
    pub upstream_model: String,
    pub label_override: Option<String>,
    pub supports_1m: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeDesktopDirectInferenceModelSpec {
    pub name: String,
    pub label_override: Option<String>,
    pub supports_1m: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeDesktopDirectModelRouteIssue {
    InvalidRouteId {
        route_id: String,
    },
    MappingUnsupported {
        route_id: String,
        upstream_model: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeDesktopDirectGatewayCredentials {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeDesktopDirectGatewayCredentialIssue {
    EnvMissing,
    BaseUrlMissing,
    AuthTokenMissing,
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

pub fn claude_desktop_profile_has_unsafe_model_ids(profile: &Value) -> bool {
    profile
        .get("inferenceModels")
        .and_then(Value::as_array)
        .is_some_and(|models| {
            models.iter().any(|item| {
                item.as_str()
                    .or_else(|| item.get("name").and_then(Value::as_str))
                    .is_some_and(|model| !claude_desktop_model_id_is_profile_safe(model))
            })
        })
}

pub fn claude_desktop_proxy_model_routes<'a>(
    routes: impl IntoIterator<Item = ClaudeDesktopProxyRouteInput<'a>>,
) -> Vec<ClaudeDesktopResolvedProxyRoute> {
    let mut entries = routes.into_iter().collect::<Vec<_>>();
    let reserved_route_ids = entries
        .iter()
        .map(|entry| entry.route_id.trim())
        .filter(|route_id| claude_desktop_model_id_is_profile_safe(route_id))
        .map(str::to_string)
        .collect::<std::collections::HashSet<_>>();

    let mut result = Vec::new();
    entries.sort_by_key(|entry| entry.route_id);
    for entry in entries {
        let route_id = entry.route_id.trim();
        let upstream_model = entry.upstream_model.trim();
        if route_id.is_empty() || upstream_model.is_empty() {
            continue;
        }

        let is_profile_safe = claude_desktop_model_id_is_profile_safe(route_id);
        let repaired_route_id = if is_profile_safe {
            route_id.to_string()
        } else {
            next_catalog_safe_route_id(&result, &reserved_route_ids)
        };

        result.push(ClaudeDesktopResolvedProxyRoute {
            route_id: repaired_route_id,
            upstream_model: upstream_model.to_string(),
            label_override: entry
                .label_override
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| (!is_profile_safe).then(|| upstream_model.to_string())),
            supports_1m: entry.supports_1m,
        });
    }

    result.sort_by(|a, b| a.route_id.cmp(&b.route_id));
    result.dedup_by(|a, b| a.route_id == b.route_id);
    result
}

pub fn claude_desktop_direct_inference_model_specs<'a>(
    routes: impl IntoIterator<Item = ClaudeDesktopProxyRouteInput<'a>>,
) -> Result<Vec<ClaudeDesktopDirectInferenceModelSpec>, ClaudeDesktopDirectModelRouteIssue> {
    let mut result = Vec::new();
    for route in routes {
        let route_id = route.route_id.trim();
        if route_id.is_empty() {
            continue;
        }
        if !claude_desktop_model_id_is_profile_safe(route_id) {
            return Err(ClaudeDesktopDirectModelRouteIssue::InvalidRouteId {
                route_id: route_id.to_string(),
            });
        }

        let upstream_model = route.upstream_model.trim();
        if !upstream_model.is_empty() && upstream_model != route_id {
            return Err(ClaudeDesktopDirectModelRouteIssue::MappingUnsupported {
                route_id: route_id.to_string(),
                upstream_model: upstream_model.to_string(),
            });
        }

        result.push(ClaudeDesktopDirectInferenceModelSpec {
            name: route_id.to_string(),
            label_override: route
                .label_override
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            supports_1m: route.supports_1m,
        });
    }

    result.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| b.supports_1m.cmp(&a.supports_1m))
    });
    result.dedup_by(|a, b| a.name == b.name);
    Ok(result)
}

pub fn claude_desktop_direct_gateway_credentials(
    settings_config: &Value,
) -> Result<ClaudeDesktopDirectGatewayCredentials, ClaudeDesktopDirectGatewayCredentialIssue> {
    let env = settings_config
        .get("env")
        .and_then(Value::as_object)
        .ok_or(ClaudeDesktopDirectGatewayCredentialIssue::EnvMissing)?;

    let base_url = env
        .get("ANTHROPIC_BASE_URL")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(ClaudeDesktopDirectGatewayCredentialIssue::BaseUrlMissing)?
        .to_string();

    let api_key = env
        .get("ANTHROPIC_AUTH_TOKEN")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(ClaudeDesktopDirectGatewayCredentialIssue::AuthTokenMissing)?
        .to_string();

    Ok(ClaudeDesktopDirectGatewayCredentials { base_url, api_key })
}

pub fn claude_desktop_proxy_request_upstream_model<'a>(
    requested_model: &str,
    routes: &[ClaudeDesktopResolvedProxyRoute],
    raw_routes: impl IntoIterator<Item = ClaudeDesktopProxyRouteInput<'a>>,
) -> Option<String> {
    let requested = strip_one_m_suffix_for_route_lookup(requested_model);
    routes
        .iter()
        .find(|route| route.route_id == requested)
        .or_else(|| {
            routes
                .iter()
                .find(|route| is_compatible_opus_route_alias(&route.route_id, requested))
        })
        .map(|route| route.upstream_model.clone())
        .or_else(|| legacy_raw_route_upstream_model(raw_routes, requested))
        .or_else(|| proxy_route_role_fallback_upstream_model(routes, requested))
}

fn next_catalog_safe_route_id(
    existing: &[ClaudeDesktopResolvedProxyRoute],
    reserved: &std::collections::HashSet<String>,
) -> String {
    if let Some(default_route) = DEFAULT_PROXY_ROUTE_IDS.iter().find(|route_id| {
        !reserved.contains(**route_id) && !existing.iter().any(|route| route.route_id == **route_id)
    }) {
        return (*default_route).to_string();
    }

    let mut index = 2usize;
    loop {
        let route_id = format!("{}-r{index}", DEFAULT_PROXY_ROUTE_IDS[0]);
        if !reserved.contains(&route_id) && !existing.iter().any(|route| route.route_id == route_id)
        {
            return route_id;
        }
        index += 1;
    }
}

fn strip_one_m_suffix_for_route_lookup(model: &str) -> &str {
    let trimmed = model.trim();
    let marker = ONE_M_CONTEXT_MARKER.as_bytes();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= marker.len()
        && bytes[bytes.len() - marker.len()..].eq_ignore_ascii_case(marker)
    {
        return trimmed[..trimmed.len() - marker.len()].trim_end();
    }
    trimmed
}

fn legacy_raw_route_upstream_model<'a>(
    raw_routes: impl IntoIterator<Item = ClaudeDesktopProxyRouteInput<'a>>,
    requested: &str,
) -> Option<String> {
    raw_routes
        .into_iter()
        .find(|route| route.route_id.trim() == requested)
        .and_then(|route| {
            let upstream_model = route.upstream_model.trim();
            (!upstream_model.is_empty()).then(|| upstream_model.to_string())
        })
}

fn is_compatible_opus_route_alias(route_id: &str, requested: &str) -> bool {
    matches!(
        (route_id, requested),
        (CURRENT_OPUS_ROUTE_ID, LEGACY_OPUS_ROUTE_ID)
            | (LEGACY_OPUS_ROUTE_ID, CURRENT_OPUS_ROUTE_ID)
    )
}

fn proxy_route_role_fallback_upstream_model(
    routes: &[ClaudeDesktopResolvedProxyRoute],
    requested: &str,
) -> Option<String> {
    if !claude_desktop_model_id_is_profile_safe(requested) {
        return None;
    }

    let role = claude_role_keyword(requested)?;
    routes
        .iter()
        .find(|route| claude_role_keyword(&route.route_id) == Some(role))
        .or_else(|| {
            (role == "fable")
                .then(|| {
                    routes
                        .iter()
                        .find(|route| claude_role_keyword(&route.route_id) == Some("opus"))
                })
                .flatten()
        })
        .map(|route| route.upstream_model.clone())
}

fn claude_role_keyword(model: &str) -> Option<&'static str> {
    let normalized = model.to_ascii_lowercase();
    if normalized.contains("opus") {
        Some("opus")
    } else if normalized.contains("haiku") {
        Some("haiku")
    } else if normalized.contains("fable") {
        Some("fable")
    } else if normalized.contains("sonnet") {
        Some("sonnet")
    } else {
        None
    }
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
        claude_desktop_direct_gateway_credentials, claude_desktop_direct_inference_model_specs,
        claude_desktop_direct_provider_validation_issue, claude_desktop_gateway_token_error,
        claude_desktop_model_id_is_profile_safe, claude_desktop_profile_has_unsafe_model_ids,
        claude_desktop_provider_models_are_profile_safe, claude_desktop_provider_selection_error,
        claude_desktop_provider_unavailable_error,
        claude_desktop_provider_unavailable_error_message,
        claude_desktop_proxy_has_base_url_and_key, claude_desktop_proxy_model_routes,
        claude_desktop_proxy_provider_config_validation_issue,
        claude_desktop_proxy_request_upstream_model, claude_desktop_routes_support_1m_by_default,
        validate_claude_desktop_gateway_bearer_header,
        validate_claude_desktop_gateway_bearer_value, ClaudeDesktopDirectGatewayCredentialIssue,
        ClaudeDesktopDirectModelRouteIssue, ClaudeDesktopDirectProviderValidationIssue,
        ClaudeDesktopGatewayAuthError, ClaudeDesktopModelListResponse,
        ClaudeDesktopModelRouteInput, ClaudeDesktopProviderValidationInput,
        ClaudeDesktopProxyProviderConfigValidationIssue, ClaudeDesktopProxyRouteInput,
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
    fn proxy_model_routes_repair_unsafe_routes_without_colliding() {
        let routes = claude_desktop_proxy_model_routes([
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-deepseek-v4-pro",
                upstream_model: "deepseek-v4-pro",
                label_override: None,
                supports_1m: true,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-old",
                upstream_model: "legacy-upstream",
                label_override: Some("   "),
                supports_1m: false,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-sonnet-4-6",
                upstream_model: "claude-sonnet-4-6",
                label_override: None,
                supports_1m: false,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: " ",
                upstream_model: "ignored-upstream",
                label_override: Some("ignored"),
                supports_1m: true,
            },
        ]);

        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].route_id, "claude-haiku-4-5");
        assert_eq!(routes[0].upstream_model, "legacy-upstream");
        assert_eq!(routes[0].label_override.as_deref(), Some("legacy-upstream"));
        assert!(!routes[0].supports_1m);

        assert_eq!(routes[1].route_id, "claude-opus-4-8");
        assert_eq!(routes[1].upstream_model, "deepseek-v4-pro");
        assert_eq!(routes[1].label_override.as_deref(), Some("deepseek-v4-pro"));
        assert!(routes[1].supports_1m);

        assert_eq!(routes[2].route_id, "claude-sonnet-4-6");
        assert_eq!(routes[2].upstream_model, "claude-sonnet-4-6");
        assert_eq!(routes[2].label_override, None);
    }

    #[test]
    fn proxy_model_routes_deduplicate_by_repaired_route_id() {
        let routes = claude_desktop_proxy_model_routes([
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-sonnet-4-6",
                upstream_model: "upstream-a",
                label_override: Some("First"),
                supports_1m: false,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-sonnet-4-6",
                upstream_model: "upstream-b",
                label_override: Some("Second"),
                supports_1m: true,
            },
        ]);

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].route_id, "claude-sonnet-4-6");
        assert_eq!(routes[0].upstream_model, "upstream-a");
        assert_eq!(routes[0].label_override.as_deref(), Some("First"));
        assert!(!routes[0].supports_1m);
    }

    #[test]
    fn direct_inference_model_specs_trim_sort_and_deduplicate_routes() {
        let specs = claude_desktop_direct_inference_model_specs([
            ClaudeDesktopProxyRouteInput {
                route_id: " claude-haiku-4-5 ",
                upstream_model: "",
                label_override: Some("  Haiku  "),
                supports_1m: false,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-sonnet-4-6",
                upstream_model: "claude-sonnet-4-6",
                label_override: Some("   "),
                supports_1m: false,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-sonnet-4-6",
                upstream_model: "claude-sonnet-4-6",
                label_override: Some("Sonnet 1M"),
                supports_1m: true,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: " ",
                upstream_model: "ignored",
                label_override: Some("ignored"),
                supports_1m: true,
            },
        ])
        .expect("direct specs");

        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].name, "claude-haiku-4-5");
        assert_eq!(specs[0].label_override.as_deref(), Some("Haiku"));
        assert!(!specs[0].supports_1m);
        assert_eq!(specs[1].name, "claude-sonnet-4-6");
        assert_eq!(specs[1].label_override.as_deref(), Some("Sonnet 1M"));
        assert!(specs[1].supports_1m);
    }

    #[test]
    fn direct_inference_model_specs_reject_invalid_route_and_mapping() {
        let invalid_route =
            claude_desktop_direct_inference_model_specs([ClaudeDesktopProxyRouteInput {
                route_id: "claude-old",
                upstream_model: "claude-old",
                label_override: None,
                supports_1m: false,
            }])
            .expect_err("invalid route");
        assert_eq!(
            invalid_route,
            ClaudeDesktopDirectModelRouteIssue::InvalidRouteId {
                route_id: "claude-old".to_string()
            }
        );

        let mapped_route =
            claude_desktop_direct_inference_model_specs([ClaudeDesktopProxyRouteInput {
                route_id: "claude-sonnet-4-6",
                upstream_model: "mimo-v2.5-pro",
                label_override: None,
                supports_1m: false,
            }])
            .expect_err("direct mapping");
        assert_eq!(
            mapped_route,
            ClaudeDesktopDirectModelRouteIssue::MappingUnsupported {
                route_id: "claude-sonnet-4-6".to_string(),
                upstream_model: "mimo-v2.5-pro".to_string()
            }
        );
    }

    #[test]
    fn direct_gateway_credentials_trim_env_values() {
        let credentials = claude_desktop_direct_gateway_credentials(&json!({
            "env": {
                "ANTHROPIC_BASE_URL": " https://gateway.example.com ",
                "ANTHROPIC_AUTH_TOKEN": " test-token "
            }
        }))
        .expect("credentials");

        assert_eq!(credentials.base_url, "https://gateway.example.com");
        assert_eq!(credentials.api_key, "test-token");
    }

    #[test]
    fn direct_gateway_credentials_report_missing_parts() {
        assert_eq!(
            claude_desktop_direct_gateway_credentials(&json!({})).expect_err("missing env"),
            ClaudeDesktopDirectGatewayCredentialIssue::EnvMissing
        );
        assert_eq!(
            claude_desktop_direct_gateway_credentials(&json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "test-token"
                }
            }))
            .expect_err("missing base url"),
            ClaudeDesktopDirectGatewayCredentialIssue::BaseUrlMissing
        );
        assert_eq!(
            claude_desktop_direct_gateway_credentials(&json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://gateway.example.com"
                }
            }))
            .expect_err("missing auth token"),
            ClaudeDesktopDirectGatewayCredentialIssue::AuthTokenMissing
        );
    }

    #[test]
    fn profile_unsafe_model_detection_checks_string_and_object_models() {
        assert!(!claude_desktop_profile_has_unsafe_model_ids(&json!({})));
        assert!(!claude_desktop_profile_has_unsafe_model_ids(&json!({
            "inferenceModels": [
                "claude-sonnet-4-6",
                { "name": "anthropic/claude-opus-4-8" },
                { "labelOverride": "missing name" }
            ]
        })));
        assert!(claude_desktop_profile_has_unsafe_model_ids(&json!({
            "inferenceModels": [
                { "name": "claude-sonnet-4-6 [1m]" }
            ]
        })));
        assert!(claude_desktop_profile_has_unsafe_model_ids(&json!({
            "inferenceModels": [
                "kimi-k2"
            ]
        })));
    }

    #[test]
    fn proxy_request_upstream_model_maps_exact_one_m_legacy_and_opus_aliases() {
        let raw_routes = [
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-sonnet-4-6",
                upstream_model: "upstream-sonnet",
                label_override: None,
                supports_1m: true,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-opus-4-8",
                upstream_model: "upstream-opus",
                label_override: None,
                supports_1m: true,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-old",
                upstream_model: "legacy-upstream",
                label_override: None,
                supports_1m: false,
            },
        ];
        let routes = claude_desktop_proxy_model_routes(raw_routes);

        assert_eq!(
            claude_desktop_proxy_request_upstream_model(
                " claude-sonnet-4-6 [1M] ",
                &routes,
                raw_routes
            )
            .as_deref(),
            Some("upstream-sonnet")
        );
        assert_eq!(
            claude_desktop_proxy_request_upstream_model("claude-old", &routes, raw_routes)
                .as_deref(),
            Some("legacy-upstream")
        );
        assert_eq!(
            claude_desktop_proxy_request_upstream_model("claude-opus-4-7", &routes, raw_routes)
                .as_deref(),
            Some("upstream-opus")
        );
    }

    #[test]
    fn proxy_request_upstream_model_maps_role_aliases_without_mapping_unknown_models() {
        let raw_routes = [
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-sonnet-4-6",
                upstream_model: "upstream-sonnet",
                label_override: None,
                supports_1m: true,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-opus-4-8",
                upstream_model: "upstream-opus",
                label_override: None,
                supports_1m: true,
            },
            ClaudeDesktopProxyRouteInput {
                route_id: "claude-haiku-4-5",
                upstream_model: "upstream-haiku",
                label_override: None,
                supports_1m: true,
            },
        ];
        let routes = claude_desktop_proxy_model_routes(raw_routes);

        assert_eq!(
            claude_desktop_proxy_request_upstream_model(
                "claude-haiku-4-5-20251001",
                &routes,
                raw_routes
            )
            .as_deref(),
            Some("upstream-haiku")
        );
        assert_eq!(
            claude_desktop_proxy_request_upstream_model("claude-fable-5[1m]", &routes, raw_routes)
                .as_deref(),
            Some("upstream-opus")
        );
        assert_eq!(
            claude_desktop_proxy_request_upstream_model("gpt-5[1m]", &routes, raw_routes),
            None
        );
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
