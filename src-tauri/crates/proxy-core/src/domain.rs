use bytes::Bytes;
use futures::stream::Stream;
use http::{HeaderMap, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::pin::Pin;

use super::error::{ProxyCoreError, ProxyCoreResult};

pub const DEFAULT_ROUTE_GROUP: &str = "default";
pub const CODEX_OAUTH_CLAUDE_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppKind {
    Claude,
    ClaudeDesktop,
    Codex,
    Gemini,
    Custom(String),
}

impl AppKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Claude => "claude",
            Self::ClaudeDesktop => "claude-desktop",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Custom(value) => value.as_str(),
        }
    }
}

impl From<&str> for AppKind {
    fn from(value: &str) -> Self {
        match normalize_token(value).as_str() {
            "claude" => Self::Claude,
            "claude-desktop" | "claude_desktop" | "claudedesktop" => Self::ClaudeDesktop,
            "codex" => Self::Codex,
            "gemini" => Self::Gemini,
            _ => Self::Custom(value.trim().to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Claude,
    ClaudeAuth,
    Codex,
    Gemini,
    GeminiCli,
    OpenRouter,
    GitHubCopilot,
    CodexOAuth,
    Custom(String),
}

impl ProviderKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Claude => "claude",
            Self::ClaudeAuth => "claude_auth",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::GeminiCli => "gemini_cli",
            Self::OpenRouter => "openrouter",
            Self::GitHubCopilot => "github_copilot",
            Self::CodexOAuth => "codex_oauth",
            Self::Custom(value) => value.as_str(),
        }
    }

    pub fn needs_transform(&self) -> bool {
        matches!(self, Self::GitHubCopilot | Self::CodexOAuth)
    }

    pub fn default_endpoint(&self) -> Option<&'static str> {
        match self {
            Self::Claude | Self::ClaudeAuth => Some("https://api.anthropic.com"),
            Self::Codex => Some("https://api.openai.com"),
            Self::Gemini | Self::GeminiCli => Some("https://generativelanguage.googleapis.com"),
            Self::OpenRouter => Some("https://openrouter.ai/api"),
            Self::GitHubCopilot => Some("https://api.githubcopilot.com"),
            Self::CodexOAuth => Some("https://chatgpt.com/backend-api/codex"),
            Self::Custom(_) => None,
        }
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for ProviderKind {
    fn from(value: &str) -> Self {
        match normalize_token(value).as_str() {
            "claude" => Self::Claude,
            "claude_auth" | "claude-auth" => Self::ClaudeAuth,
            "codex" => Self::Codex,
            "gemini" => Self::Gemini,
            "gemini_cli" | "gemini-cli" => Self::GeminiCli,
            "openrouter" => Self::OpenRouter,
            "github_copilot" | "github-copilot" | "githubcopilot" => Self::GitHubCopilot,
            "codex_oauth" | "codex-oauth" | "codexoauth" => Self::CodexOAuth,
            _ => Self::Custom(value.trim().to_string()),
        }
    }
}

impl std::str::FromStr for ProviderKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match ProviderKind::from(value) {
            Self::Custom(_) => Err(format!("Invalid provider kind: {value}")),
            known => Ok(known),
        }
    }
}

pub fn infer_claude_provider_kind(
    api_format: &str,
    uses_google_oauth: bool,
    meta_provider_type: Option<&str>,
    base_url: Option<&str>,
    settings_config: &Value,
) -> ProviderKind {
    if api_format == "gemini_native" {
        return if uses_google_oauth {
            ProviderKind::GeminiCli
        } else {
            ProviderKind::Gemini
        };
    }

    match meta_provider_type {
        Some("github_copilot") => return ProviderKind::GitHubCopilot,
        Some("codex_oauth") => return ProviderKind::CodexOAuth,
        _ => {}
    }

    if let Some(base_url) = base_url {
        if base_url.contains("githubcopilot.com") {
            return ProviderKind::GitHubCopilot;
        }
        if base_url.contains("openrouter.ai") {
            return ProviderKind::OpenRouter;
        }
    }

    if settings_config.get("auth_mode").and_then(Value::as_str) == Some("bearer_only")
        || settings_config
            .get("env")
            .and_then(|env| env.get("AUTH_MODE"))
            .and_then(Value::as_str)
            == Some("bearer_only")
    {
        return ProviderKind::ClaudeAuth;
    }

    ProviderKind::Claude
}

pub fn extract_claude_base_url_from_settings(
    is_codex_oauth: bool,
    settings_config: &Value,
) -> Option<String> {
    if is_codex_oauth {
        return Some(CODEX_OAUTH_CLAUDE_BASE_URL.to_string());
    }

    [
        settings_config
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
            .and_then(Value::as_str),
        settings_config.get("base_url").and_then(Value::as_str),
        settings_config.get("baseURL").and_then(Value::as_str),
        settings_config.get("apiEndpoint").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .next()
    .map(|url| url.trim_end_matches('/').to_string())
}

pub fn extract_openclaw_stream_check_base_url(settings_config: &Value) -> Option<String> {
    trimmed_non_empty_setting(settings_config.get("baseUrl"))
}

pub fn extract_hermes_stream_check_base_url(settings_config: &Value) -> Option<String> {
    trimmed_non_empty_setting(settings_config.get("base_url"))
}

pub fn extract_opencode_stream_check_npm(settings_config: &Value) -> Option<String> {
    trimmed_non_empty_setting(settings_config.get("npm"))
}

pub fn resolve_opencode_stream_check_base_url(
    settings_config: &Value,
    npm: Option<&str>,
) -> Option<String> {
    settings_config
        .get("options")
        .and_then(|options| trimmed_non_empty_setting(options.get("baseURL")))
        .or_else(|| opencode_default_base_url_for_npm(npm).map(ToString::to_string))
}

pub fn opencode_default_base_url_for_npm(npm: Option<&str>) -> Option<&'static str> {
    match npm {
        Some("@ai-sdk/openai") => Some("https://api.openai.com/v1"),
        Some("@ai-sdk/anthropic") => Some("https://api.anthropic.com"),
        Some("@ai-sdk/google") => Some("https://generativelanguage.googleapis.com"),
        _ => None,
    }
}

fn trimmed_non_empty_setting(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthProfileRef(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthProfileRefKind {
    Provider {
        app_type: String,
        provider_id: String,
    },
    ChannelKey {
        key_ref: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelAuthProfileResolution {
    Provider { provider_id: String },
    ChannelKey { key_ref: String },
    Ignore,
}

impl AuthProfileRef {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn kind(&self) -> Option<AuthProfileRefKind> {
        parse_auth_profile_ref(&self.0)
    }
}

pub fn parse_auth_profile_ref(value: &str) -> Option<AuthProfileRefKind> {
    if let Some(key_ref) = value.strip_prefix("channel-key:") {
        let key_ref = key_ref.trim();
        return (!key_ref.is_empty()).then(|| AuthProfileRefKind::ChannelKey {
            key_ref: key_ref.to_string(),
        });
    }

    if let Some(provider_ref) = value.strip_prefix("provider:") {
        let mut parts = provider_ref.splitn(2, ':');
        let app_type = parts.next().unwrap_or_default();
        let provider_id = parts.next().unwrap_or_default();
        if app_type.trim().is_empty() || provider_id.trim().is_empty() {
            return None;
        }

        return Some(AuthProfileRefKind::Provider {
            app_type: app_type.to_string(),
            provider_id: provider_id.to_string(),
        });
    }

    None
}

pub fn channel_auth_profile_resolution(
    auth_profile_ref: Option<&AuthProfileRef>,
    app_type: &str,
) -> ChannelAuthProfileResolution {
    let Some(auth_profile_ref) = auth_profile_ref else {
        return ChannelAuthProfileResolution::Ignore;
    };

    match auth_profile_ref.kind() {
        Some(AuthProfileRefKind::Provider {
            app_type: profile_app,
            provider_id,
        }) if profile_app == app_type => ChannelAuthProfileResolution::Provider { provider_id },
        Some(AuthProfileRefKind::ChannelKey { key_ref }) => {
            ChannelAuthProfileResolution::ChannelKey { key_ref }
        }
        Some(AuthProfileRefKind::Provider { .. }) | None => ChannelAuthProfileResolution::Ignore,
    }
}

pub fn channel_auth_profile_missing_provider_warning(
    app_type: &str,
    auth_profile_ref: &str,
) -> String {
    format!(
        "[{app_type}] channel auth profile references missing provider: {auth_profile_ref}"
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSpec {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_ref: Option<String>,
    #[serde(default)]
    pub metadata: ProviderMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMetadata {
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub raw: Value,
}

impl Default for ProviderMetadata {
    fn default() -> Self {
        Self {
            labels: Vec::new(),
            raw: Value::Object(Default::default()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMetadataInput {
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub sort_index: Option<usize>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub icon_color: Option<String>,
    #[serde(default)]
    pub in_failover_queue: bool,
    #[serde(default)]
    pub provider_type: Option<String>,
    #[serde(default)]
    pub api_format: Option<String>,
    #[serde(default)]
    pub auth_binding: Option<Value>,
    #[serde(default)]
    pub endpoint_auto_select: Option<bool>,
    #[serde(default)]
    pub custom_endpoint_count: usize,
}

pub fn provider_metadata_from_input(input: ProviderMetadataInput) -> ProviderMetadata {
    let mut labels = Vec::new();
    if input.in_failover_queue {
        labels.push("failover".to_string());
    }
    if let Some(category) = input.category.as_deref() {
        labels.push(category.to_string());
    }

    ProviderMetadata {
        labels,
        raw: json!({
            "websiteUrl": input.website_url,
            "category": input.category,
            "sortIndex": input.sort_index,
            "notes": input.notes,
            "icon": input.icon,
            "iconColor": input.icon_color,
            "inFailoverQueue": input.in_failover_queue,
            "providerType": input.provider_type,
            "apiFormat": input.api_format,
            "authBinding": input.auth_binding,
            "endpointAutoSelect": input.endpoint_auto_select,
            "customEndpointCount": input.custom_endpoint_count,
        }),
    }
}

pub fn provider_account_ref(
    provider_type: Option<&str>,
    managed_account_id: Option<&str>,
) -> Option<String> {
    provider_type
        .zip(managed_account_id)
        .map(|(provider_type, account_id)| format!("{provider_type}:{account_id}"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatus {
    Enabled,
    ManuallyDisabled,
    AutoDisabled,
    Draining,
    Unknown,
    Custom(String),
}

impl ChannelStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Enabled => "enabled",
            Self::ManuallyDisabled => "manually_disabled",
            Self::AutoDisabled => "auto_disabled",
            Self::Draining => "draining",
            Self::Unknown => "unknown",
            Self::Custom(value) => value.as_str(),
        }
    }

    pub fn from_storage(value: &str) -> Self {
        match normalize_token(value).as_str() {
            "enabled" | "active" => Self::Enabled,
            "disabled" | "manual_disabled" | "manually_disabled" | "manually-disabled" => {
                Self::ManuallyDisabled
            }
            "auto_disabled" | "autodisabled" | "auto-disabled" => Self::AutoDisabled,
            "draining" => Self::Draining,
            "unknown" | "" => Self::Unknown,
            _ => Self::Custom(value.trim().to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceKind {
    AnthropicMessages,
    OpenAiChatCompletions,
    OpenAiResponses,
    GeminiNative,
    GeminiOpenAiCompatible,
    AzureOpenAi,
    Embeddings,
    Rerank,
    Custom(String),
}

impl InterfaceKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::AnthropicMessages => "anthropic_messages",
            Self::OpenAiChatCompletions => "openai_chat_completions",
            Self::OpenAiResponses => "openai_responses",
            Self::GeminiNative => "gemini_native",
            Self::GeminiOpenAiCompatible => "gemini_openai_compatible",
            Self::AzureOpenAi => "azure_openai",
            Self::Embeddings => "embeddings",
            Self::Rerank => "rerank",
            Self::Custom(value) => value.as_str(),
        }
    }

    pub fn from_storage(value: &str) -> Self {
        match normalize_token(value).as_str() {
            "anthropic" | "anthropic_messages" | "anthropic-messages" => Self::AnthropicMessages,
            "openai_chat"
            | "openai-chat"
            | "openai_chat_completions"
            | "openai-chat-completions"
            | "chat_completions"
            | "chat-completions" => Self::OpenAiChatCompletions,
            "openai_responses" | "openai-responses" | "responses" => Self::OpenAiResponses,
            "gemini" | "gemini_native" | "gemini-native" => Self::GeminiNative,
            "gemini_openai_compatible" | "gemini-openai-compatible" | "gemini_openai" => {
                Self::GeminiOpenAiCompatible
            }
            "azure_openai" | "azure-openai" => Self::AzureOpenAi,
            "embeddings" | "embedding" => Self::Embeddings,
            "rerank" | "reranker" => Self::Rerank,
            _ => Self::Custom(value.trim().to_string()),
        }
    }

    pub fn claude_api_format(&self) -> Option<&'static str> {
        match self {
            Self::AnthropicMessages => Some("anthropic"),
            Self::OpenAiChatCompletions => Some("openai_chat"),
            Self::OpenAiResponses => Some("openai_responses"),
            Self::GeminiNative => Some("gemini_native"),
            _ => None,
        }
    }

    pub fn codex_api_format(&self) -> Option<&'static str> {
        match self {
            Self::OpenAiChatCompletions => Some("openai_chat"),
            Self::OpenAiResponses => Some("openai_responses"),
            _ => None,
        }
    }
}

pub fn claude_api_format_for_interface_kind(interface_kind: &str) -> Option<&'static str> {
    InterfaceKind::from_storage(interface_kind).claude_api_format()
}

pub fn codex_api_format_for_interface_kind(interface_kind: &str) -> Option<&'static str> {
    InterfaceKind::from_storage(interface_kind).codex_api_format()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamEndpoint {
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSpec {
    pub id: String,
    pub provider_id: String,
    pub app: AppKind,
    pub name: String,
    pub status: ChannelStatus,
    pub endpoint: UpstreamEndpoint,
    pub interface: InterfaceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_profile: Option<AuthProfileRef>,
    #[serde(default)]
    pub models: Vec<ModelRoute>,
    #[serde(default)]
    pub groups: Vec<String>,
    pub priority: i64,
    pub weight: u32,
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    #[serde(default)]
    pub health_policy: ChannelHealthPolicy,
    #[serde(default)]
    pub overrides: ChannelOverrides,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub needs_review: bool,
    #[serde(default)]
    pub review_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRoute {
    pub public_model: String,
    pub upstream_model: String,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_model: Option<String>,
    #[serde(default)]
    pub request_overrides: Value,
    #[serde(default)]
    pub response_overrides: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRouteInput {
    pub public_model: String,
    pub upstream_model: String,
    #[serde(default)]
    pub capabilities: Value,
    #[serde(default)]
    pub pricing_model: Option<String>,
    #[serde(default)]
    pub request_overrides: Value,
    #[serde(default)]
    pub response_overrides: Value,
}

pub fn model_route_from_input(input: ModelRouteInput) -> ModelRoute {
    ModelRoute {
        public_model: input.public_model,
        upstream_model: input.upstream_model,
        capabilities: ModelCapabilities {
            raw: crate::channel_request::channel_object_or_default(input.capabilities),
        },
        pricing_model: input.pricing_model,
        request_overrides: crate::channel_request::channel_object_or_default(
            input.request_overrides,
        ),
        response_overrides: crate::channel_request::channel_object_or_default(
            input.response_overrides,
        ),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSpecInput {
    pub id: String,
    pub provider_id: String,
    pub app_type: String,
    pub name: String,
    pub status: String,
    pub base_url: String,
    pub interface_kind: String,
    #[serde(default)]
    pub auth_profile_ref: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelRouteInput>,
    #[serde(default)]
    pub groups: Vec<String>,
    pub priority: i64,
    pub weight: u32,
    #[serde(default)]
    pub retry_policy: Value,
    #[serde(default)]
    pub health_policy: Value,
    #[serde(default)]
    pub header_overrides: Value,
    #[serde(default)]
    pub param_overrides: Value,
    #[serde(default)]
    pub status_code_mapping: Value,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub needs_review: bool,
    #[serde(default)]
    pub review_reasons: Vec<String>,
}

pub fn channel_spec_from_input(input: ChannelSpecInput) -> ChannelSpec {
    ChannelSpec {
        id: input.id,
        provider_id: input.provider_id,
        app: AppKind::from(input.app_type.as_str()),
        name: input.name,
        status: ChannelStatus::from_storage(&input.status),
        endpoint: UpstreamEndpoint {
            base_url: input.base_url,
            path_template: None,
            api_version: None,
            timeout_profile: None,
        },
        interface: InterfaceKind::from_storage(&input.interface_kind),
        auth_profile: input.auth_profile_ref.map(AuthProfileRef::new),
        models: input
            .models
            .into_iter()
            .map(model_route_from_input)
            .collect(),
        groups: input.groups,
        priority: input.priority,
        weight: input.weight,
        retry_policy: RetryPolicy {
            raw: crate::channel_request::channel_object_or_default(input.retry_policy),
        },
        health_policy: ChannelHealthPolicy {
            raw: crate::channel_request::channel_object_or_default(input.health_policy),
        },
        overrides: ChannelOverrides {
            headers: crate::channel_request::channel_object_or_default(input.header_overrides),
            params: crate::channel_request::channel_object_or_default(input.param_overrides),
            status_code_mapping: crate::channel_request::channel_array_or_default(
                input.status_code_mapping,
            ),
            model_mapping: Value::Object(Default::default()),
        },
        tags: input.tags,
        metadata: crate::channel_request::channel_object_or_default(input.metadata),
        source_ref: input.source_ref,
        needs_review: input.needs_review,
        review_reasons: input.review_reasons,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutableModel {
    pub public_model: String,
    pub upstream_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_model: Option<String>,
    pub app: AppKind,
    pub provider_id: String,
    pub provider_name: String,
    pub channel_id: String,
    pub channel_name: String,
    pub interface: InterfaceKind,
    #[serde(default)]
    pub groups: Vec<String>,
    pub priority: i64,
    pub weight: u32,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutableModelList {
    pub app_type: String,
    pub route_group: Option<String>,
    pub interface_kind: Option<String>,
    #[serde(default)]
    pub models: Vec<RoutableModel>,
}

impl RoutableModelList {
    pub fn new(
        app_type: impl Into<String>,
        route_group: Option<String>,
        interface_kind: Option<String>,
        models: Vec<RoutableModel>,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            route_group,
            interface_kind,
            models,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryPolicy {
    #[serde(default)]
    pub raw: Value,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            raw: Value::Object(Default::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelHealthPolicy {
    #[serde(default)]
    pub raw: Value,
}

impl Default for ChannelHealthPolicy {
    fn default() -> Self {
        Self {
            raw: Value::Object(Default::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelOverrides {
    #[serde(default)]
    pub headers: Value,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub status_code_mapping: Value,
    #[serde(default)]
    pub model_mapping: Value,
}

impl Default for ChannelOverrides {
    fn default() -> Self {
        Self {
            headers: Value::Object(Default::default()),
            params: Value::Object(Default::default()),
            status_code_mapping: Value::Array(Vec::new()),
            model_mapping: Value::Object(Default::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSelection {
    pub provider: ProviderSpec,
    pub channel: ChannelSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_route: Option<ModelRoute>,
    pub inbound_interface: InterfaceKind,
    pub outbound_interface: InterfaceKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePlan {
    pub selection: RouteSelection,
    #[serde(default)]
    pub selections: Vec<RouteSelection>,
    #[serde(default)]
    pub attempts: Vec<ChannelAttemptPlan>,
}

pub fn route_selection_from_parts(
    provider: ProviderSpec,
    channel: ChannelSpec,
    model_route: Option<ModelRoute>,
    inbound_interface: InterfaceKind,
) -> RouteSelection {
    let outbound_interface = channel.interface.clone();
    RouteSelection {
        provider,
        channel,
        model_route,
        inbound_interface,
        outbound_interface,
    }
}

pub fn route_plan_selections(plan: &RoutePlan) -> &[RouteSelection] {
    if plan.selections.is_empty() {
        std::slice::from_ref(&plan.selection)
    } else {
        plan.selections.as_slice()
    }
}

pub fn route_plan_provider_ids(plan: &RoutePlan) -> Vec<String> {
    let mut provider_ids = Vec::new();
    for selection in route_plan_selections(plan) {
        let provider_id = &selection.channel.provider_id;
        if !provider_ids.iter().any(|id| id == provider_id) {
            provider_ids.push(provider_id.clone());
        }
    }
    provider_ids
}

pub fn select_route_for_forward_result(
    plan: &RoutePlan,
    selected_channel_id: Option<&str>,
    provider_id: &str,
) -> RouteSelection {
    if let Some(channel_id) = selected_channel_id {
        if let Some(selection) = route_plan_selections(plan)
            .iter()
            .find(|selection| selection.channel.id == channel_id)
        {
            return selection.clone();
        }
    }

    route_plan_selections(plan)
        .iter()
        .find(|selection| {
            selection.provider.id == provider_id || selection.channel.provider_id == provider_id
        })
        .cloned()
        .unwrap_or_else(|| plan.selection.clone())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAttemptPlan {
    pub channel_id: String,
    pub provider_id: String,
    pub priority: i64,
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAttemptResult {
    pub channel_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedChannelAttempt {
    pub channel_id: String,
    pub channel_name: String,
    pub base_url: String,
    pub interface_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_profile_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_model: Option<String>,
    #[serde(default)]
    pub header_overrides: Value,
    #[serde(default)]
    pub param_overrides: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub channel_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePolicy {
    pub app: AppKind,
    #[serde(default)]
    pub groups: Vec<RouteGroup>,
    #[serde(default)]
    pub raw: Value,
}

pub fn route_policy_from_failover_provider_ids(
    app: AppKind,
    failover_provider_ids: impl IntoIterator<Item = String>,
) -> RoutePolicy {
    RoutePolicy {
        app,
        groups: Vec::new(),
        raw: json!({
            "defaultGroup": DEFAULT_ROUTE_GROUP,
            "failoverProviderIds": failover_provider_ids.into_iter().collect::<Vec<_>>(),
        }),
    }
}

#[derive(Debug, Clone)]
pub struct ChannelQuery<'a> {
    pub app: &'a AppKind,
    pub provider_id: Option<&'a str>,
    pub model: Option<&'a str>,
    pub group: Option<&'a str>,
    pub include_disabled: bool,
    pub allow_legacy_projection: bool,
}

pub fn route_group_matches(groups: &[String], requested_group: &str) -> bool {
    if groups.is_empty() {
        return requested_group == DEFAULT_ROUTE_GROUP;
    }
    groups.iter().any(|group| group == requested_group)
}

pub fn interfaces_compatible(requested: &InterfaceKind, channel: &InterfaceKind) -> bool {
    if requested == channel {
        return true;
    }

    matches!(
        (requested, channel),
        (
            InterfaceKind::AnthropicMessages,
            InterfaceKind::OpenAiChatCompletions
                | InterfaceKind::OpenAiResponses
                | InterfaceKind::GeminiNative
        ) | (
            InterfaceKind::OpenAiResponses,
            InterfaceKind::OpenAiChatCompletions | InterfaceKind::OpenAiResponses
        ) | (
            InterfaceKind::OpenAiChatCompletions,
            InterfaceKind::OpenAiChatCompletions | InterfaceKind::OpenAiResponses
        )
    )
}

pub fn channel_matches_query(channel: &ChannelSpec, query: &ChannelQuery<'_>) -> bool {
    if !query.include_disabled && channel.status != ChannelStatus::Enabled {
        return false;
    }
    if let Some(provider_id) = query.provider_id {
        if channel.provider_id != provider_id {
            return false;
        }
    }
    if let Some(group) = query.group {
        if !route_group_matches(&channel.groups, group) {
            return false;
        }
    }
    if let Some(model) = query.model {
        return channel
            .models
            .iter()
            .any(|route| route.public_model == model || route.upstream_model == model);
    }

    true
}

#[derive(Debug)]
pub struct RouteRequest<'a> {
    pub request: &'a ProxyRequest,
    pub providers: &'a [ProviderSpec],
    pub channels: &'a [ChannelSpec],
    pub policy: Option<&'a RoutePolicy>,
}

pub fn build_route_plan(request: RouteRequest<'_>) -> ProxyCoreResult<RoutePlan> {
    let mut selections = Vec::new();
    let requested_group = request
        .request
        .route_group
        .as_deref()
        .unwrap_or(DEFAULT_ROUTE_GROUP);
    let requested_model = request.request.requested_model.as_deref();

    for channel in request.channels {
        if channel.status != ChannelStatus::Enabled {
            continue;
        }
        if !route_group_matches(&channel.groups, requested_group) {
            continue;
        }
        if !interfaces_compatible(&request.request.inbound_interface, &channel.interface) {
            continue;
        }

        let model_route = match requested_model {
            Some(model) => channel
                .models
                .iter()
                .find(|route| route.public_model == model || route.upstream_model == model)
                .cloned(),
            None => channel.models.first().cloned(),
        };
        if requested_model.is_some() && model_route.is_none() {
            continue;
        }

        let Some(provider) = request
            .providers
            .iter()
            .find(|provider| provider.id == channel.provider_id)
            .cloned()
        else {
            continue;
        };

        selections.push(route_selection_from_parts(
            provider,
            channel.clone(),
            model_route,
            request.request.inbound_interface.clone(),
        ));
    }

    selections.sort_by(|left, right| {
        right
            .channel
            .priority
            .cmp(&left.channel.priority)
            .then_with(|| right.channel.weight.cmp(&left.channel.weight))
            .then_with(|| left.channel.name.cmp(&right.channel.name))
            .then_with(|| left.channel.id.cmp(&right.channel.id))
    });

    let selection = selections
        .first()
        .cloned()
        .ok_or_else(|| ProxyCoreError::Unavailable("no routable channel".to_string()))?;
    let attempts = selections
        .iter()
        .map(|selection| ChannelAttemptPlan {
            channel_id: selection.channel.id.clone(),
            provider_id: selection.channel.provider_id.clone(),
            priority: selection.channel.priority,
            weight: selection.channel.weight,
        })
        .collect();

    Ok(RoutePlan {
        selection,
        selections,
        attempts,
    })
}

#[derive(Debug)]
pub struct ProxyRequest {
    pub app: AppKind,
    pub method: Method,
    pub endpoint: String,
    pub inbound_interface: InterfaceKind,
    pub requested_model: Option<String>,
    pub route_group: Option<String>,
    pub headers: HeaderMap,
    pub extensions: http::Extensions,
    pub body: ProxyBody,
    pub client_request_id: Option<String>,
}

impl ProxyRequest {
    pub fn new(
        app: AppKind,
        method: Method,
        endpoint: impl Into<String>,
        inbound_interface: InterfaceKind,
        body: ProxyBody,
    ) -> Self {
        Self {
            app,
            method,
            endpoint: endpoint.into(),
            inbound_interface,
            requested_model: None,
            route_group: None,
            headers: HeaderMap::new(),
            extensions: http::Extensions::new(),
            body,
            client_request_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProxyBody {
    Empty,
    Json(Value),
    Bytes(Bytes),
}

impl Default for ProxyBody {
    fn default() -> Self {
        Self::Empty
    }
}

impl ProxyBody {
    pub fn into_json(self) -> ProxyCoreResult<Value> {
        match self {
            Self::Json(value) => Ok(value),
            Self::Empty => Ok(json!({})),
            Self::Bytes(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                ProxyCoreError::InvalidRequest(format!("invalid JSON body: {error}"))
            }),
        }
    }
}

pub type ProxyByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;

pub enum ProxyResponseBody {
    Empty,
    Json(Value),
    Bytes(Bytes),
    Stream(ProxyByteStream),
}

pub enum ProxyTransportResponseBody {
    Empty,
    Bytes(Bytes),
    Stream(ProxyByteStream),
}

impl ProxyResponseBody {
    pub fn bytes(body: impl Into<Bytes>) -> Self {
        Self::Bytes(body.into())
    }

    pub fn json(body: Value) -> Self {
        Self::Json(body)
    }

    pub fn stream(
        stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    ) -> Self {
        Self::Stream(Box::pin(stream))
    }

    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream(_))
    }

    pub fn into_transport_body(self) -> Result<ProxyTransportResponseBody, String> {
        match self {
            Self::Empty => Ok(ProxyTransportResponseBody::Empty),
            Self::Json(value) => serde_json::to_vec(&value)
                .map(Bytes::from)
                .map(ProxyTransportResponseBody::Bytes)
                .map_err(|error| format!("Failed to serialize proxy core response: {error}")),
            Self::Bytes(body) => Ok(ProxyTransportResponseBody::Bytes(body)),
            Self::Stream(stream) => Ok(ProxyTransportResponseBody::Stream(stream)),
        }
    }
}

impl Default for ProxyResponseBody {
    fn default() -> Self {
        Self::Empty
    }
}

impl std::fmt::Debug for ProxyResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("Empty"),
            Self::Json(value) => f.debug_tuple("Json").field(value).finish(),
            Self::Bytes(bytes) => f
                .debug_tuple("Bytes")
                .field(&format_args!("{} bytes", bytes.len()))
                .finish(),
            Self::Stream(_) => f.write_str("Stream(..)"),
        }
    }
}

#[derive(Debug)]
pub struct ProxyCoreResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ProxyResponseBody,
}

pub struct ProxyTransportResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ProxyTransportResponseBody,
}

impl ProxyCoreResponse {
    pub fn empty(status: StatusCode) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: ProxyResponseBody::Empty,
        }
    }

    pub fn with_body(status: StatusCode, headers: HeaderMap, body: ProxyResponseBody) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn into_transport_response(self) -> Result<ProxyTransportResponse, String> {
        Ok(ProxyTransportResponse {
            status: self.status,
            headers: self.headers,
            body: self.body.into_transport_body()?,
        })
    }
}

#[derive(Debug)]
pub struct ProxyResult {
    pub response: ProxyCoreResponse,
    pub selected_route: RouteSelection,
    pub outbound_model: Option<String>,
    pub usage_record: Option<UsageRecord>,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

impl UsageTokens {
    pub fn has_billable_tokens(&self) -> bool {
        self.input_tokens > 0
            || self.output_tokens > 0
            || self.cache_read_tokens > 0
            || self.cache_creation_tokens > 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub request_id: Option<String>,
    pub message_id: Option<String>,
    pub app: AppKind,
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_kind: Option<ProviderKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_group: Option<String>,
    pub request_model: String,
    pub outbound_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_model: Option<String>,
    pub tokens: UsageTokens,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_token_ms: Option<u64>,
    pub status_code: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub is_streaming: bool,
    #[serde(default)]
    pub metadata: Value,
}

fn normalize_token(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn test_channel(status: ChannelStatus) -> ChannelSpec {
        ChannelSpec {
            id: "ch_1".to_string(),
            provider_id: "p1".to_string(),
            app: AppKind::Claude,
            name: "Relay".to_string(),
            status,
            endpoint: UpstreamEndpoint {
                base_url: "https://relay.example.com/v1".to_string(),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::AnthropicMessages,
            auth_profile: None,
            models: vec![ModelRoute {
                public_model: "sonnet-public".to_string(),
                upstream_model: "upstream-sonnet".to_string(),
                capabilities: ModelCapabilities::default(),
                pricing_model: None,
                request_overrides: json!({}),
                response_overrides: json!({}),
            }],
            groups: vec!["default".to_string()],
            priority: 100,
            weight: 1,
            retry_policy: RetryPolicy::default(),
            health_policy: ChannelHealthPolicy::default(),
            overrides: ChannelOverrides::default(),
            tags: Vec::new(),
            metadata: json!({}),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        }
    }

    fn test_provider(id: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: format!("{id} Provider"),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        }
    }

    fn test_route_selection(provider_id: &str, channel_id: &str) -> RouteSelection {
        let mut channel = test_channel(ChannelStatus::Enabled);
        channel.id = channel_id.to_string();
        channel.provider_id = provider_id.to_string();
        RouteSelection {
            provider: test_provider(provider_id),
            channel,
            model_route: None,
            inbound_interface: InterfaceKind::AnthropicMessages,
            outbound_interface: InterfaceKind::AnthropicMessages,
        }
    }

    fn test_route_plan(selections: Vec<RouteSelection>) -> RoutePlan {
        RoutePlan {
            selection: selections
                .first()
                .cloned()
                .unwrap_or_else(|| test_route_selection("provider-a", "channel-a")),
            selections,
            attempts: Vec::new(),
        }
    }

    #[test]
    fn route_policy_from_failover_provider_ids_preserves_raw_contract() {
        let policy = route_policy_from_failover_provider_ids(
            AppKind::Claude,
            vec!["provider-a".to_string(), "provider-b".to_string()],
        );

        assert_eq!(policy.app, AppKind::Claude);
        assert!(policy.groups.is_empty());
        assert_eq!(policy.raw["defaultGroup"], json!(DEFAULT_ROUTE_GROUP));
        assert_eq!(
            policy.raw["failoverProviderIds"],
            json!(["provider-a", "provider-b"])
        );
    }

    #[test]
    fn provider_metadata_input_builds_sanitized_raw_and_labels() {
        let metadata = provider_metadata_from_input(ProviderMetadataInput {
            website_url: Some("https://provider.example".to_string()),
            category: Some("aggregator".to_string()),
            sort_index: Some(7),
            notes: Some("operator note".to_string()),
            icon: Some("provider-icon".to_string()),
            icon_color: Some("#123456".to_string()),
            in_failover_queue: true,
            provider_type: Some("github_copilot".to_string()),
            api_format: Some("openai_responses".to_string()),
            auth_binding: Some(json!({
                "source": "managed_account",
                "authProvider": "github_copilot",
                "accountId": "acct-1"
            })),
            endpoint_auto_select: Some(true),
            custom_endpoint_count: 2,
        });

        assert_eq!(
            metadata.labels,
            vec!["failover".to_string(), "aggregator".to_string()]
        );
        assert_eq!(
            metadata.raw["websiteUrl"],
            json!("https://provider.example")
        );
        assert_eq!(metadata.raw["category"], json!("aggregator"));
        assert_eq!(metadata.raw["sortIndex"], json!(7));
        assert_eq!(metadata.raw["iconColor"], json!("#123456"));
        assert_eq!(metadata.raw["providerType"], json!("github_copilot"));
        assert_eq!(metadata.raw["apiFormat"], json!("openai_responses"));
        assert_eq!(metadata.raw["endpointAutoSelect"], json!(true));
        assert_eq!(metadata.raw["customEndpointCount"], json!(2));
        assert_eq!(
            metadata.raw["authBinding"],
            json!({
                "source": "managed_account",
                "authProvider": "github_copilot",
                "accountId": "acct-1"
            })
        );
        assert!(metadata.raw.get("apiKey").is_none());
        assert!(metadata.raw.get("settingsConfig").is_none());
    }

    #[test]
    fn provider_account_ref_requires_provider_type_and_account() {
        assert_eq!(
            provider_account_ref(Some("github_copilot"), Some("acct-1")),
            Some("github_copilot:acct-1".to_string())
        );
        assert_eq!(provider_account_ref(None, Some("acct-1")), None);
        assert_eq!(provider_account_ref(Some("github_copilot"), None), None);
    }

    #[test]
    fn auth_profile_ref_parser_preserves_host_runtime_semantics() {
        assert_eq!(
            parse_auth_profile_ref("channel-key: primary "),
            Some(AuthProfileRefKind::ChannelKey {
                key_ref: "primary".to_string()
            })
        );
        assert_eq!(
            parse_auth_profile_ref("provider:claude:provider-a"),
            Some(AuthProfileRefKind::Provider {
                app_type: "claude".to_string(),
                provider_id: "provider-a".to_string()
            })
        );
        assert_eq!(
            parse_auth_profile_ref("provider:claude: provider-a"),
            Some(AuthProfileRefKind::Provider {
                app_type: "claude".to_string(),
                provider_id: " provider-a".to_string()
            })
        );
        assert_eq!(parse_auth_profile_ref(" channel-key:primary"), None);
        assert_eq!(parse_auth_profile_ref("channel-key: "), None);
        assert_eq!(parse_auth_profile_ref("provider:claude:"), None);
        assert_eq!(parse_auth_profile_ref("vault:primary"), None);
    }

    #[test]
    fn channel_auth_profile_resolution_preserves_host_runtime_semantics() {
        assert_eq!(
            channel_auth_profile_resolution(
                Some(&AuthProfileRef::new("provider:claude:auth-provider")),
                "claude",
            ),
            ChannelAuthProfileResolution::Provider {
                provider_id: "auth-provider".to_string()
            }
        );
        assert_eq!(
            channel_auth_profile_resolution(
                Some(&AuthProfileRef::new("provider:codex:auth-provider")),
                "claude",
            ),
            ChannelAuthProfileResolution::Ignore
        );
        assert_eq!(
            channel_auth_profile_resolution(
                Some(&AuthProfileRef::new("provider:claude: auth-provider")),
                "claude",
            ),
            ChannelAuthProfileResolution::Provider {
                provider_id: " auth-provider".to_string()
            }
        );
        assert_eq!(
            channel_auth_profile_resolution(
                Some(&AuthProfileRef::new("channel-key: primary ")),
                "claude",
            ),
            ChannelAuthProfileResolution::ChannelKey {
                key_ref: "primary".to_string()
            }
        );
        assert_eq!(
            channel_auth_profile_resolution(Some(&AuthProfileRef::new("vault:primary")), "claude"),
            ChannelAuthProfileResolution::Ignore
        );
        assert_eq!(
            channel_auth_profile_missing_provider_warning(
                "claude",
                "provider:claude:missing-provider",
            ),
            "[claude] channel auth profile references missing provider: provider:claude:missing-provider"
        );
    }

    #[test]
    fn model_route_input_builds_route_with_object_defaults() {
        let route = model_route_from_input(ModelRouteInput {
            public_model: "sonnet".to_string(),
            upstream_model: "anthropic/sonnet".to_string(),
            capabilities: json!(["not", "object"]),
            pricing_model: Some("standard".to_string()),
            request_overrides: json!({"temperature": 0.2}),
            response_overrides: json!("not-object"),
        });

        assert_eq!(route.public_model, "sonnet");
        assert_eq!(route.upstream_model, "anthropic/sonnet");
        assert_eq!(route.capabilities.raw, json!({}));
        assert_eq!(route.pricing_model.as_deref(), Some("standard"));
        assert_eq!(route.request_overrides, json!({"temperature": 0.2}));
        assert_eq!(route.response_overrides, json!({}));
    }

    #[test]
    fn channel_spec_input_builds_spec_with_core_defaults() {
        let spec = channel_spec_from_input(ChannelSpecInput {
            id: "ch-1".to_string(),
            provider_id: "provider-1".to_string(),
            app_type: "claude".to_string(),
            name: "Relay A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_chat_completions".to_string(),
            auth_profile_ref: Some("provider:claude:provider-1".to_string()),
            models: vec![ModelRouteInput {
                public_model: "sonnet".to_string(),
                upstream_model: "anthropic/sonnet".to_string(),
                capabilities: json!({"tools": true}),
                pricing_model: None,
                request_overrides: json!({}),
                response_overrides: json!({}),
            }],
            groups: vec!["default".to_string(), "paid".to_string()],
            priority: 50,
            weight: 80,
            retry_policy: json!({"maxAttempts": 2}),
            health_policy: json!("not-object"),
            header_overrides: json!({"x-test": "1"}),
            param_overrides: json!("not-object"),
            status_code_mapping: json!({"not": "array"}),
            tags: vec!["manual".to_string()],
            metadata: json!(["not", "object"]),
            source_ref: Some("https://relay.example.com/v1".to_string()),
            needs_review: true,
            review_reasons: vec!["missing-auth".to_string()],
        });

        assert_eq!(spec.id, "ch-1");
        assert_eq!(spec.provider_id, "provider-1");
        assert_eq!(spec.app, AppKind::Claude);
        assert_eq!(spec.status, ChannelStatus::Enabled);
        assert_eq!(spec.endpoint.base_url, "https://relay.example.com/v1");
        assert_eq!(spec.interface, InterfaceKind::OpenAiChatCompletions);
        assert_eq!(
            spec.auth_profile.as_ref().map(|value| value.0.as_str()),
            Some("provider:claude:provider-1")
        );
        assert_eq!(spec.models.len(), 1);
        assert_eq!(spec.models[0].public_model, "sonnet");
        assert_eq!(spec.groups, vec!["default".to_string(), "paid".to_string()]);
        assert_eq!(spec.retry_policy.raw, json!({"maxAttempts": 2}));
        assert_eq!(spec.health_policy.raw, json!({}));
        assert_eq!(spec.overrides.headers, json!({"x-test": "1"}));
        assert_eq!(spec.overrides.params, json!({}));
        assert_eq!(spec.overrides.status_code_mapping, json!([]));
        assert_eq!(spec.metadata, json!({}));
        assert_eq!(
            spec.source_ref.as_deref(),
            Some("https://relay.example.com/v1")
        );
        assert!(spec.needs_review);
        assert_eq!(spec.review_reasons, vec!["missing-auth".to_string()]);
    }

    #[test]
    fn build_route_plan_filters_channels_and_orders_attempts() {
        fn channel(
            id: &str,
            provider_id: &str,
            status: ChannelStatus,
            priority: i64,
            weight: u32,
        ) -> ChannelSpec {
            let mut channel = test_channel(status);
            channel.id = id.to_string();
            channel.name = id.to_string();
            channel.provider_id = provider_id.to_string();
            channel.priority = priority;
            channel.weight = weight;
            channel
        }

        let providers = vec![
            test_provider("provider-a"),
            test_provider("provider-b"),
            test_provider("provider-c"),
        ];
        let mut wrong_group = channel(
            "channel-wrong-group",
            "provider-a",
            ChannelStatus::Enabled,
            500,
            1,
        );
        wrong_group.groups = vec!["paid".to_string()];
        let mut wrong_interface = channel(
            "channel-wrong-interface",
            "provider-a",
            ChannelStatus::Enabled,
            500,
            1,
        );
        wrong_interface.interface = InterfaceKind::Embeddings;
        let mut wrong_model = channel(
            "channel-wrong-model",
            "provider-a",
            ChannelStatus::Enabled,
            500,
            1,
        );
        wrong_model.models[0].public_model = "other-public".to_string();
        wrong_model.models[0].upstream_model = "other-upstream".to_string();
        let channels = vec![
            channel("channel-low", "provider-a", ChannelStatus::Enabled, 10, 10),
            channel(
                "channel-high",
                "provider-b",
                ChannelStatus::Enabled,
                100,
                10,
            ),
            channel(
                "channel-heavier",
                "provider-c",
                ChannelStatus::Enabled,
                100,
                20,
            ),
            channel(
                "channel-disabled",
                "provider-a",
                ChannelStatus::ManuallyDisabled,
                500,
                1,
            ),
            channel(
                "channel-missing-provider",
                "provider-missing",
                ChannelStatus::Enabled,
                500,
                1,
            ),
            wrong_group,
            wrong_interface,
            wrong_model,
        ];
        let mut proxy_request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Empty,
        );
        proxy_request.requested_model = Some("sonnet-public".to_string());

        let plan = build_route_plan(RouteRequest {
            request: &proxy_request,
            providers: &providers,
            channels: &channels,
            policy: None,
        })
        .expect("route plan");

        assert_eq!(plan.selection.channel.id, "channel-heavier");
        assert_eq!(
            plan.selections
                .iter()
                .map(|selection| selection.channel.id.as_str())
                .collect::<Vec<_>>(),
            vec!["channel-heavier", "channel-high", "channel-low"]
        );
        assert_eq!(
            plan.attempts
                .iter()
                .map(|attempt| attempt.channel_id.as_str())
                .collect::<Vec<_>>(),
            vec!["channel-heavier", "channel-high", "channel-low"]
        );
    }

    #[test]
    fn proxy_core_response_serializes_json_for_transport() {
        let response = ProxyCoreResponse::with_body(
            StatusCode::CREATED,
            HeaderMap::new(),
            ProxyResponseBody::json(json!({"ok": true})),
        );

        let response = response
            .into_transport_response()
            .expect("transport response");

        assert_eq!(response.status, StatusCode::CREATED);
        match response.body {
            ProxyTransportResponseBody::Bytes(body) => {
                assert_eq!(body, Bytes::from_static(br#"{"ok":true}"#))
            }
            _ => panic!("expected bytes body"),
        }
    }

    #[test]
    fn proxy_response_body_keeps_stream_for_transport() {
        let stream =
            futures::stream::once(async { Ok::<_, std::io::Error>(Bytes::from_static(b"chunk")) });
        let body = ProxyResponseBody::stream(stream)
            .into_transport_body()
            .expect("transport body");

        match body {
            ProxyTransportResponseBody::Stream(mut stream) => {
                let chunk = futures::executor::block_on(stream.next())
                    .expect("stream item")
                    .expect("stream chunk");
                assert_eq!(chunk, Bytes::from_static(b"chunk"));
            }
            _ => panic!("expected stream body"),
        }
    }

    #[test]
    fn route_plan_provider_ids_uses_primary_selection_when_no_attempt_list_exists() {
        let plan = RoutePlan {
            selection: test_route_selection("provider-a", "channel-a"),
            selections: Vec::new(),
            attempts: Vec::new(),
        };

        assert_eq!(route_plan_selections(&plan).len(), 1);
        assert_eq!(route_plan_selections(&plan)[0].channel.id, "channel-a");
        assert_eq!(route_plan_provider_ids(&plan), vec!["provider-a"]);
    }

    #[test]
    fn route_plan_provider_ids_dedupes_in_selection_order() {
        let plan = test_route_plan(vec![
            test_route_selection("provider-a", "channel-a"),
            test_route_selection("provider-b", "channel-b"),
            test_route_selection("provider-a", "channel-c"),
        ]);

        assert_eq!(
            route_plan_provider_ids(&plan),
            vec!["provider-a", "provider-b"]
        );
    }

    #[test]
    fn select_route_for_forward_result_prefers_selected_channel_id() {
        let plan = test_route_plan(vec![
            test_route_selection("provider-a", "channel-a"),
            test_route_selection("provider-a", "channel-b"),
        ]);

        let selection = select_route_for_forward_result(&plan, Some("channel-b"), "provider-a");

        assert_eq!(selection.channel.id, "channel-b");
    }

    #[test]
    fn select_route_for_forward_result_falls_back_to_provider_id() {
        let plan = test_route_plan(vec![
            test_route_selection("provider-a", "channel-a"),
            test_route_selection("provider-b", "channel-b"),
        ]);

        let selection =
            select_route_for_forward_result(&plan, Some("missing-channel"), "provider-b");

        assert_eq!(selection.channel.id, "channel-b");
    }

    #[test]
    fn select_route_for_forward_result_falls_back_to_primary_selection() {
        let plan = test_route_plan(vec![
            test_route_selection("provider-a", "channel-a"),
            test_route_selection("provider-b", "channel-b"),
        ]);

        let selection =
            select_route_for_forward_result(&plan, Some("missing-channel"), "provider-c");

        assert_eq!(selection.channel.id, "channel-a");
    }

    #[test]
    fn response_body_can_hold_stream_without_host_transport_types() {
        let stream = futures::stream::iter(vec![Ok(Bytes::from_static(b"chunk"))]);
        let body = ProxyResponseBody::stream(stream);

        assert!(body.is_stream());
        match body {
            ProxyResponseBody::Stream(mut stream) => {
                let chunk = futures::executor::block_on(stream.next())
                    .expect("stream item")
                    .expect("stream chunk");
                assert_eq!(chunk, Bytes::from_static(b"chunk"));
            }
            _ => panic!("expected stream body"),
        }
    }

    #[test]
    fn channel_query_matches_status_provider_group_and_model() {
        let app = AppKind::Claude;
        let channel = test_channel(ChannelStatus::Enabled);

        assert!(channel_matches_query(
            &channel,
            &ChannelQuery {
                app: &app,
                provider_id: Some("p1"),
                model: Some("sonnet-public"),
                group: Some("default"),
                include_disabled: false,
                allow_legacy_projection: false,
            },
        ));
        assert!(channel_matches_query(
            &channel,
            &ChannelQuery {
                app: &app,
                provider_id: Some("p1"),
                model: Some("upstream-sonnet"),
                group: Some("default"),
                include_disabled: false,
                allow_legacy_projection: false,
            },
        ));
        assert!(!channel_matches_query(
            &channel,
            &ChannelQuery {
                app: &app,
                provider_id: Some("other"),
                model: Some("sonnet-public"),
                group: Some("default"),
                include_disabled: false,
                allow_legacy_projection: false,
            },
        ));
        assert!(!channel_matches_query(
            &test_channel(ChannelStatus::ManuallyDisabled),
            &ChannelQuery {
                app: &app,
                provider_id: None,
                model: None,
                group: None,
                include_disabled: false,
                allow_legacy_projection: false,
            },
        ));
    }

    #[test]
    fn proxy_body_into_json_preserves_core_body_contract() {
        assert_eq!(ProxyBody::Empty.into_json().unwrap(), json!({}));
        assert_eq!(
            ProxyBody::Json(json!({"ok": true})).into_json().unwrap(),
            json!({"ok": true})
        );
        assert_eq!(
            ProxyBody::Bytes(Bytes::from_static(br#"{"model":"x"}"#))
                .into_json()
                .unwrap(),
            json!({"model": "x"})
        );

        assert!(matches!(
            ProxyBody::Bytes(Bytes::from_static(b"{bad-json")).into_json(),
            Err(ProxyCoreError::InvalidRequest(message)) if message.contains("invalid JSON body")
        ));
    }

    #[test]
    fn interface_kind_maps_claude_channel_formats() {
        assert_eq!(
            claude_api_format_for_interface_kind("anthropic_messages"),
            Some("anthropic")
        );
        assert_eq!(
            claude_api_format_for_interface_kind("openai_chat_completions"),
            Some("openai_chat")
        );
        assert_eq!(
            claude_api_format_for_interface_kind("openai_responses"),
            Some("openai_responses")
        );
        assert_eq!(
            claude_api_format_for_interface_kind("gemini_native"),
            Some("gemini_native")
        );
        assert_eq!(claude_api_format_for_interface_kind("embeddings"), None);
    }

    #[test]
    fn interface_kind_maps_codex_channel_formats() {
        assert_eq!(
            codex_api_format_for_interface_kind("openai_chat_completions"),
            Some("openai_chat")
        );
        assert_eq!(
            codex_api_format_for_interface_kind("openai_responses"),
            Some("openai_responses")
        );
        assert_eq!(codex_api_format_for_interface_kind("gemini_native"), None);
    }

    #[test]
    fn provider_kind_reports_transform_requirements() {
        assert!(!ProviderKind::Claude.needs_transform());
        assert!(!ProviderKind::ClaudeAuth.needs_transform());
        assert!(!ProviderKind::Codex.needs_transform());
        assert!(!ProviderKind::Gemini.needs_transform());
        assert!(!ProviderKind::GeminiCli.needs_transform());
        assert!(!ProviderKind::OpenRouter.needs_transform());
        assert!(ProviderKind::GitHubCopilot.needs_transform());
        assert!(ProviderKind::CodexOAuth.needs_transform());
        assert!(!ProviderKind::Custom("custom".to_string()).needs_transform());
    }

    #[test]
    fn provider_kind_default_endpoint_matches_known_providers() {
        assert_eq!(
            ProviderKind::Claude.default_endpoint(),
            Some("https://api.anthropic.com")
        );
        assert_eq!(
            ProviderKind::ClaudeAuth.default_endpoint(),
            Some("https://api.anthropic.com")
        );
        assert_eq!(
            ProviderKind::Codex.default_endpoint(),
            Some("https://api.openai.com")
        );
        assert_eq!(
            ProviderKind::Gemini.default_endpoint(),
            Some("https://generativelanguage.googleapis.com")
        );
        assert_eq!(
            ProviderKind::GeminiCli.default_endpoint(),
            Some("https://generativelanguage.googleapis.com")
        );
        assert_eq!(
            ProviderKind::OpenRouter.default_endpoint(),
            Some("https://openrouter.ai/api")
        );
        assert_eq!(
            ProviderKind::GitHubCopilot.default_endpoint(),
            Some("https://api.githubcopilot.com")
        );
        assert_eq!(
            ProviderKind::CodexOAuth.default_endpoint(),
            Some("https://chatgpt.com/backend-api/codex")
        );
        assert_eq!(
            ProviderKind::Custom("x".to_string()).default_endpoint(),
            None
        );
    }

    #[test]
    fn infers_claude_provider_kind_from_host_neutral_facts() {
        assert_eq!(
            infer_claude_provider_kind(
                "gemini_native",
                false,
                Some("github_copilot"),
                Some("https://api.githubcopilot.com"),
                &Value::Null,
            ),
            ProviderKind::Gemini
        );
        assert_eq!(
            infer_claude_provider_kind("gemini_native", true, None, None, &Value::Null),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            infer_claude_provider_kind(
                "anthropic",
                false,
                Some("github_copilot"),
                Some("https://api.anthropic.com"),
                &Value::Null,
            ),
            ProviderKind::GitHubCopilot
        );
        assert_eq!(
            infer_claude_provider_kind("anthropic", false, Some("codex_oauth"), None, &Value::Null),
            ProviderKind::CodexOAuth
        );
        assert_eq!(
            infer_claude_provider_kind(
                "anthropic",
                false,
                None,
                Some("https://openrouter.ai/api"),
                &Value::Null,
            ),
            ProviderKind::OpenRouter
        );
        assert_eq!(
            infer_claude_provider_kind(
                "anthropic",
                false,
                None,
                Some("https://proxy.example.com"),
                &serde_json::json!({"env": {"AUTH_MODE": "bearer_only"}}),
            ),
            ProviderKind::ClaudeAuth
        );
        assert_eq!(
            infer_claude_provider_kind("anthropic", false, None, None, &Value::Null),
            ProviderKind::Claude
        );
    }

    #[test]
    fn extracts_claude_base_url_from_settings() {
        let settings = serde_json::json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://env.example.com/v1///",
            },
            "base_url": "https://direct.example.com/v1",
        });
        assert_eq!(
            extract_claude_base_url_from_settings(false, &settings).as_deref(),
            Some("https://env.example.com/v1")
        );

        assert_eq!(
            extract_claude_base_url_from_settings(
                false,
                &serde_json::json!({"baseURL": "https://camel.example.com/"})
            )
            .as_deref(),
            Some("https://camel.example.com")
        );
        assert_eq!(
            extract_claude_base_url_from_settings(
                false,
                &serde_json::json!({"apiEndpoint": "https://endpoint.example.com///"})
            )
            .as_deref(),
            Some("https://endpoint.example.com")
        );
        assert_eq!(
            extract_claude_base_url_from_settings(true, &Value::Null).as_deref(),
            Some(CODEX_OAUTH_CLAUDE_BASE_URL)
        );
        assert_eq!(
            extract_claude_base_url_from_settings(false, &Value::Null),
            None
        );
    }

    #[test]
    fn extracts_stream_check_base_urls_from_settings() {
        assert_eq!(
            extract_openclaw_stream_check_base_url(&serde_json::json!({
                "baseUrl": " https://openclaw.example.com/v1 "
            }))
            .as_deref(),
            Some("https://openclaw.example.com/v1")
        );
        assert_eq!(
            extract_openclaw_stream_check_base_url(&serde_json::json!({"baseUrl": "   "})),
            None
        );
        assert_eq!(
            extract_hermes_stream_check_base_url(&serde_json::json!({
                "base_url": " https://hermes.example.com "
            }))
            .as_deref(),
            Some("https://hermes.example.com")
        );
        assert_eq!(
            extract_opencode_stream_check_npm(&serde_json::json!({
                "npm": " @ai-sdk/openai "
            }))
            .as_deref(),
            Some("@ai-sdk/openai")
        );
        assert_eq!(
            resolve_opencode_stream_check_base_url(
                &serde_json::json!({
                    "npm": "@ai-sdk/openai",
                    "options": { "baseURL": " https://proxy.example.com/v1 " }
                }),
                Some("@ai-sdk/openai"),
            )
            .as_deref(),
            Some("https://proxy.example.com/v1")
        );
        assert_eq!(
            resolve_opencode_stream_check_base_url(
                &serde_json::json!({"npm": "@ai-sdk/anthropic", "options": {}}),
                Some("@ai-sdk/anthropic"),
            )
            .as_deref(),
            Some("https://api.anthropic.com")
        );
        assert_eq!(
            resolve_opencode_stream_check_base_url(
                &serde_json::json!({"npm": "@ai-sdk/openai-compatible", "options": {}}),
                Some("@ai-sdk/openai-compatible"),
            ),
            None
        );
    }

    #[test]
    fn provider_kind_parses_common_aliases() {
        assert_eq!(ProviderKind::from("claude"), ProviderKind::Claude);
        assert_eq!(ProviderKind::from("claude-auth"), ProviderKind::ClaudeAuth);
        assert_eq!(ProviderKind::from("gemini-cli"), ProviderKind::GeminiCli);
        assert_eq!(
            ProviderKind::from("githubcopilot"),
            ProviderKind::GitHubCopilot
        );
        assert_eq!(ProviderKind::from("codexoauth"), ProviderKind::CodexOAuth);
        assert_eq!(
            ProviderKind::from("vendor-x"),
            ProviderKind::Custom("vendor-x".to_string())
        );
    }

    #[test]
    fn provider_kind_from_str_accepts_known_aliases_and_rejects_custom() {
        assert_eq!(
            "claude".parse::<ProviderKind>().unwrap(),
            ProviderKind::Claude
        );
        assert_eq!(
            "github-copilot".parse::<ProviderKind>().unwrap(),
            ProviderKind::GitHubCopilot
        );
        assert!("vendor-x".parse::<ProviderKind>().is_err());
    }

    #[test]
    fn provider_kind_display_uses_external_label() {
        assert_eq!(ProviderKind::ClaudeAuth.to_string(), "claude_auth");
        assert_eq!(
            ProviderKind::Custom("vendor-x".to_string()).to_string(),
            "vendor-x"
        );
    }
}
