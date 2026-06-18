use bytes::Bytes;
use futures::stream::Stream;
use http::{HeaderMap, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;

pub const DEFAULT_ROUTE_GROUP: &str = "default";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthProfileRef(pub String);

impl AuthProfileRef {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
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

#[derive(Debug)]
pub struct RouteRequest<'a> {
    pub request: &'a ProxyRequest,
    pub providers: &'a [ProviderSpec],
    pub channels: &'a [ChannelSpec],
    pub policy: Option<&'a RoutePolicy>,
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

pub type ProxyByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;

pub enum ProxyResponseBody {
    Empty,
    Json(Value),
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

    pub fn stream(stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static) -> Self {
        Self::Stream(Box::pin(stream))
    }

    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream(_))
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
}
