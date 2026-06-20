use crate::app_config::AppType;
use crate::claude_desktop_config::ResolvedModelRoute;
use crate::database::{ProxyChannelKeyRecord, ProxyChannelModelRecord, ProxyChannelRecord};
use crate::provider::{Provider, ProviderMeta};
use crate::proxy::providers::provider_kind_from_app_type_and_config;
use crate::proxy::usage::RequestLog;
use crate::proxy_core::api::auth::ClaudeDesktopModelRouteInput;
use crate::proxy_core::api::domain::{
    ChannelSpecInput, ModelRoute, ModelRouteInput, ProviderMetadata, ProviderMetadataInput,
};
#[cfg(test)]
use crate::proxy_core::api::domain::{ChannelHealthPolicy, ChannelOverrides, UpstreamEndpoint};
use crate::proxy_core::api::management::{
    AppSummaryInput, ChannelReachabilityInput, ChannelReachabilityResult,
    CurrentRouteProviderSummaryInput,
};
use crate::proxy_core::api::routing::{RouteResolveChannelInput, RouteResolveModelInput};
use crate::proxy_core::api::session::SessionIdResult;
use crate::proxy_core::api::transforms::CodexChatErrorNormalization;
use crate::proxy_core::api::transport::{UpstreamRequestTransportPolicy, UpstreamSendPolicy};
use crate::services::usage_stats::is_placeholder_pricing_model;
use crate::services::stream_check::{HealthStatus, StreamCheckResult};
use bytes::Bytes;
use futures::Stream;
use http::{HeaderMap, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) fn synthesize_gemini_tool_call_id_with_uuid() -> String {
    crate::proxy_core::api::transforms::synthesize_gemini_tool_call_id(
        Uuid::new_v4().simple().to_string(),
    )
}

pub(crate) type ClaudeDesktopGatewayAuthError =
    crate::proxy_core::api::auth::ClaudeDesktopGatewayAuthError;

pub(crate) type ProxyErrorStatusKind =
    crate::proxy_core::api::errors::ProxyErrorStatusKind;

pub(crate) fn validate_claude_desktop_gateway_bearer_header(
    headers: &HeaderMap,
    expected_token: &str,
) -> Result<(), ClaudeDesktopGatewayAuthError> {
    crate::proxy_core::api::auth::validate_claude_desktop_gateway_bearer_header(
        headers,
        expected_token,
    )
}

pub(crate) fn proxy_error_http_status_code(kind: ProxyErrorStatusKind) -> u16 {
    crate::proxy_core::api::errors::proxy_error_http_status_code(kind)
}

pub(crate) fn proxy_core_error_from_status_kind(
    kind: ProxyErrorStatusKind,
    message: impl Into<String>,
) -> ProxyCoreError {
    crate::proxy_core::api::errors::proxy_core_error_from_status_kind(kind, message)
}

pub(crate) const SYSTEM_PROXY_ENV_KEYS: [&str; 6] =
    crate::proxy_core::api::transport::SYSTEM_PROXY_ENV_KEYS;

pub(crate) fn mask_url_for_log(url: &str) -> String {
    crate::proxy_core::api::security::mask_url_for_log(url)
}

#[cfg(test)]
pub(crate) fn proxy_url_points_to_loopback_port(value: &str, loopback_port: u16) -> bool {
    crate::proxy_core::api::transport::proxy_url_points_to_loopback_port(
        value,
        loopback_port,
    )
}

pub(crate) fn proxy_values_point_to_loopback_port<I, V>(
    values: I,
    loopback_port: u16,
) -> bool
where
    I: IntoIterator<Item = V>,
    V: AsRef<str>,
{
    crate::proxy_core::api::transport::proxy_values_point_to_loopback_port(
        values,
        loopback_port,
    )
}

pub(crate) const COPILOT_PUBLIC_GITHUB_DOMAIN: &str =
    crate::proxy_core::api::model_catalog::COPILOT_PUBLIC_GITHUB_DOMAIN;

pub(crate) fn default_copilot_github_domain() -> String {
    crate::proxy_core::api::model_catalog::default_copilot_github_domain()
}

pub(crate) fn normalize_github_domain(raw: &str) -> Result<String, String> {
    crate::proxy_core::api::model_catalog::normalize_github_domain(raw)
}

pub(crate) fn is_copilot_ghes_domain(domain: &str) -> bool {
    crate::proxy_core::api::model_catalog::is_copilot_ghes_domain(domain)
}

pub(crate) fn copilot_composite_account_id(domain: &str, user_id: u64) -> String {
    crate::proxy_core::api::model_catalog::copilot_composite_account_id(domain, user_id)
}

pub(crate) type CopilotModel = crate::proxy_core::api::model_catalog::CopilotModel;

pub(crate) fn parse_copilot_models_response_bytes(
    body: &[u8],
) -> Result<Vec<CopilotModel>, String> {
    crate::proxy_core::api::model_catalog::parse_copilot_models_response_bytes(body)
}

pub(crate) fn copilot_github_client_id(domain: &str) -> &'static str {
    crate::proxy_core::api::model_catalog::copilot_github_client_id(domain)
}

pub(crate) fn copilot_github_device_code_url(domain: &str) -> String {
    crate::proxy_core::api::model_catalog::copilot_github_device_code_url(domain)
}

pub(crate) fn copilot_github_oauth_token_url(domain: &str) -> String {
    crate::proxy_core::api::model_catalog::copilot_github_oauth_token_url(domain)
}

pub(crate) fn copilot_github_user_url(domain: &str) -> String {
    crate::proxy_core::api::model_catalog::copilot_github_user_url(domain)
}

pub(crate) fn copilot_token_url(domain: &str) -> String {
    crate::proxy_core::api::model_catalog::copilot_token_url(domain)
}

pub(crate) fn copilot_usage_url(domain: &str) -> String {
    crate::proxy_core::api::model_catalog::copilot_usage_url(domain)
}

pub(crate) fn copilot_api_base(domain: &str) -> String {
    crate::proxy_core::api::model_catalog::copilot_api_base(domain)
}

pub(crate) type FetchedModel = crate::proxy_core::api::model_catalog::FetchedModel;
pub(crate) type CodexOAuthModelsRequest<'a> =
    crate::proxy_core::api::model_catalog::CodexOAuthModelsRequest<'a>;
pub(crate) type OpenAiCompatibleModelsRequest<'a> =
    crate::proxy_core::api::model_catalog::OpenAiCompatibleModelsRequest<'a>;
pub(crate) type ModelFetchHttpResponse =
    crate::proxy_core::api::model_catalog::ModelFetchHttpResponse;
pub(crate) use crate::proxy_core::api::model_catalog::{
    CodexOAuthModelsTransport, OpenAiCompatibleModelsTransport,
};

pub(crate) async fn fetch_openai_compatible_models_with_transport<T>(
    base_url: &str,
    api_key: &str,
    is_full_url: bool,
    models_url_override: Option<&str>,
    user_agent: Option<&http::HeaderValue>,
    transport: &T,
) -> Result<Vec<FetchedModel>, String>
where
    T: OpenAiCompatibleModelsTransport + ?Sized,
{
    crate::proxy_core::api::model_catalog::fetch_openai_compatible_models_with_transport(
        base_url,
        api_key,
        is_full_url,
        models_url_override,
        user_agent,
        transport,
    )
    .await
}

pub(crate) async fn fetch_codex_oauth_models_with_transport<T>(
    token: &str,
    account_id: &str,
    client_version: &str,
    transport: &T,
) -> Result<Vec<FetchedModel>, String>
where
    T: CodexOAuthModelsTransport + ?Sized,
{
    crate::proxy_core::api::model_catalog::fetch_codex_oauth_models_with_transport(
        token,
        account_id,
        client_version,
        transport,
    )
    .await
}

pub(crate) type RectifierConfig = crate::proxy_core::api::ports::RectifierConfig;
pub(crate) type OptimizerConfig = crate::proxy_core::api::ports::OptimizerConfig;
pub(crate) type CopilotOptimizerConfig =
    crate::proxy_core::api::ports::CopilotOptimizerConfig;
pub(crate) type RectifierConfigSpec =
    crate::proxy_core::api::ports::RectifierConfigSpec;
pub(crate) type OptimizerConfigSpec =
    crate::proxy_core::api::ports::OptimizerConfigSpec;
pub(crate) type CopilotOptimizerConfigSpec =
    crate::proxy_core::api::ports::CopilotOptimizerConfigSpec;

pub(crate) type ProxyConfig = crate::proxy_core::api::ports::ProxyConfig;
pub(crate) type ProxyRuntimeStatus =
    crate::proxy_core::api::ports::ProxyRuntimeStatus;
pub(crate) type ProxyRuntimeConfig =
    crate::proxy_core::api::config::ProxyRuntimeConfig;
pub(crate) type ProxyGlobalConfig = crate::proxy_core::api::config::ProxyGlobalConfig;
pub(crate) type ProxyAppConfig = crate::proxy_core::api::config::ProxyAppConfig;
pub(crate) type ProxyServerInfo = crate::proxy_core::api::ports::ProxyServerInfo;
pub(crate) type ProxyTakeoverStatus =
    crate::proxy_core::api::ports::ProxyTakeoverStatus;
pub(crate) type ClaudeDesktopModelListResponse =
    crate::proxy_core::api::auth::ClaudeDesktopModelListResponse;
pub(crate) type ProxyCoreResponse =
    crate::proxy_core::api::transport::ProxyCoreResponse;
pub(crate) type ProxyCoreResult<T> = crate::proxy_core::api::errors::ProxyCoreResult<T>;
pub(crate) type ProxyEngine<S> = crate::proxy_core::api::engine::ProxyEngine<S>;
pub(crate) type ProxyResult = crate::proxy_core::api::transport::ProxyResult;
pub(crate) type ProxyCoreEvent = crate::proxy_core::api::events::ProxyCoreEvent;
pub(crate) type ProxyEventEnvelope =
    crate::proxy_core::api::events::ProxyEventEnvelope;
pub(crate) type ProxyEventSseSpec =
    crate::proxy_core::api::events::ProxyEventSseSpec;
pub(crate) type CodexChatHistorySseInspection =
    crate::proxy_core::api::transforms::CodexChatHistorySseInspection;
pub(crate) type CodexChatHistorySseRecord =
    crate::proxy_core::api::transforms::CodexChatHistorySseRecord;
pub(crate) type CodexChatHistoryState =
    crate::proxy_core::api::transforms::CodexChatHistoryState;
pub(crate) type CodexChatReasoningOptions =
    crate::proxy_core::api::transforms::CodexChatReasoningOptions;
pub(crate) type CodexChatReasoningProfile =
    crate::proxy_core::api::transforms::CodexChatReasoningProfile;
pub(crate) type CodexToolContext =
    crate::proxy_core::api::transforms::CodexToolContext;
pub(crate) type ProxyResponseBody =
    crate::proxy_core::api::transport::ProxyResponseBody;
pub(crate) type ProxyTransportResponse =
    crate::proxy_core::api::transport::ProxyTransportResponse;
pub(crate) type ProxyTransportResponseBody =
    crate::proxy_core::api::transport::ProxyTransportResponseBody;
pub(crate) type ResponseBodyDecode =
    crate::proxy_core::api::transport::ResponseBodyDecode;
pub(crate) type ResponseBodyDecodeLogLevel =
    crate::proxy_core::api::transport::ResponseBodyDecodeLogLevel;
pub(crate) type CostBreakdown = crate::proxy_core::api::usage::CostBreakdown;
pub(crate) type CostCalculator = crate::proxy_core::api::usage::CostCalculator;
pub(crate) type ModelPricing = crate::proxy_core::api::usage::ModelPricing;
pub(crate) type TokenUsage = crate::proxy_core::api::usage::TokenUsage;
pub(crate) type UsageRecord = crate::proxy_core::api::usage::UsageRecord;
pub(crate) type UsageRouteContext = crate::proxy_core::api::usage::UsageRouteContext;
#[cfg(test)]
pub(crate) type UsageTokens = crate::proxy_core::api::usage::UsageTokens;
pub(crate) type NonStreamingResponseUsageRecord =
    crate::proxy_core::api::usage::NonStreamingResponseUsageRecord;
pub(crate) type StreamingResponseUsageRecord =
    crate::proxy_core::api::usage::StreamingResponseUsageRecord;
pub(crate) type UsageParserConfig =
    crate::proxy_core::api::usage::UsageParserConfig;
pub(crate) type StreamUsageEventFilter =
    crate::proxy_core::api::usage::StreamUsageEventFilter;
pub(crate) type TransformedResponseUsageFormat =
    crate::proxy_core::api::usage::TransformedResponseUsageFormat;
pub(crate) type CurrentRouteTarget =
    crate::proxy_core::api::ports::CurrentRouteTarget;
pub(crate) type GeminiShadowStore =
    crate::proxy_core::api::transforms::GeminiShadowStore;
pub(crate) type GeminiToAnthropicMessageOutput =
    crate::proxy_core::api::transforms::GeminiToAnthropicMessageOutput;
pub(crate) type AnthropicToolSchemaHints =
    crate::proxy_core::api::transforms::AnthropicToolSchemaHints;
pub(crate) type AuthProfileRef = crate::proxy_core::api::domain::AuthProfileRef;
pub(crate) type GeminiOAuthCredentials =
    crate::proxy_core::api::auth::GeminiOAuthCredentials;
pub(crate) type ClaudeAuthHeaderKind =
    crate::proxy_core::api::transport::ClaudeAuthHeaderKind;
pub(crate) type ClaudeAuthKey = crate::proxy_core::api::auth::ClaudeAuthKey;
pub(crate) type ClaudeAuthKeySource =
    crate::proxy_core::api::auth::ClaudeAuthKeySource;
pub(crate) type ClaudePromptCacheKeyResolution =
    crate::proxy_core::api::transforms::ClaudePromptCacheKeyResolution;
pub(crate) type CopilotAuthHeadersInput<'a> =
    crate::proxy_core::api::transport::CopilotAuthHeadersInput<'a>;
pub(crate) type CopilotAuthHeaderOverrides<'a> =
    crate::proxy_core::api::transport::CopilotAuthHeaderOverrides<'a>;
pub(crate) type ResponseRuntimePolicy =
    crate::proxy_core::api::config::ResponseRuntimePolicy;
pub(crate) type ResponseTimeoutConfig =
    crate::proxy_core::api::config::ResponseTimeoutConfig;
pub(crate) type StreamingTimeoutConfig =
    crate::proxy_core::api::config::StreamingTimeoutConfig;
pub(crate) type StreamingTimeoutPhase =
    crate::proxy_core::api::transport::StreamingTimeoutPhase;
pub(crate) type SseEventScanner =
    crate::proxy_core::api::transforms::SseEventScanner;
pub(crate) type SsePassthroughEventKind =
    crate::proxy_core::api::transforms::SsePassthroughEventKind;
pub(crate) type SseUsageAccumulator =
    crate::proxy_core::api::transforms::SseUsageAccumulator;
pub(crate) type GlobalProxyConfig = crate::proxy_core::api::ports::GlobalProxyConfig;
pub(crate) type AppProxyConfig = crate::proxy_core::api::config::AppProxyConfig;
pub(crate) type ProviderHealth = crate::proxy_core::api::ports::ProviderHealth;
pub(crate) type ProviderKind = crate::proxy_core::api::domain::ProviderKind;
pub(crate) type ProviderAuthInfo =
    crate::proxy_core::api::auth::ProviderAuthInfo;
pub(crate) type ProviderAuthStrategy =
    crate::proxy_core::api::auth::ProviderAuthStrategy;
pub(crate) type AuthInfo = crate::proxy_core::api::ports::AuthInfo;
pub(crate) type AttemptEventChannel<'a> =
    crate::proxy_core::api::events::AttemptEventChannel<'a>;
pub(crate) type AttemptEventPayloadInput<'a> =
    crate::proxy_core::api::events::AttemptEventPayloadInput<'a>;
pub(crate) type AttemptEventPhase =
    crate::proxy_core::api::events::AttemptEventPhase;
pub(crate) type ChannelAttemptResult =
    crate::proxy_core::api::ports::ChannelAttemptResult;
pub(crate) type ChannelQuery<'a> = crate::proxy_core::api::routing::ChannelQuery<'a>;
pub(crate) type ForwardFailureCategory =
    crate::proxy_core::api::transport::ForwardFailureCategory;
pub(crate) type MediaRetryInput<'a> =
    crate::proxy_core::api::transport::MediaRetryInput<'a>;
pub(crate) type PromptCacheTraceLogInput<'a> =
    crate::proxy_core::api::transport::PromptCacheTraceLogInput<'a>;
pub(crate) type AllowResult = crate::proxy_core::api::config::AllowResult;
pub(crate) type CircuitBreakerConfig =
    crate::proxy_core::api::config::CircuitBreakerConfig;
pub(crate) type CircuitBreakerStats =
    crate::proxy_core::api::config::CircuitBreakerStats;
pub(crate) type CircuitState = crate::proxy_core::api::config::CircuitState;

pub(crate) mod circuit_breaker_log_codes {
    pub(crate) const OPEN_TO_HALF_OPEN: &str =
        crate::proxy_core::api::logging::cb::OPEN_TO_HALF_OPEN;
    pub(crate) const HALF_OPEN_TO_CLOSED: &str =
        crate::proxy_core::api::logging::cb::HALF_OPEN_TO_CLOSED;
    pub(crate) const HALF_OPEN_PROBE_FAILED: &str =
        crate::proxy_core::api::logging::cb::HALF_OPEN_PROBE_FAILED;
    pub(crate) const TRIGGERED_FAILURES: &str =
        crate::proxy_core::api::logging::cb::TRIGGERED_FAILURES;
    pub(crate) const TRIGGERED_ERROR_RATE: &str =
        crate::proxy_core::api::logging::cb::TRIGGERED_ERROR_RATE;
    pub(crate) const MANUAL_RESET: &str = crate::proxy_core::api::logging::cb::MANUAL_RESET;
}

pub(crate) mod server_log_codes {
    pub(crate) const STARTED: &str = crate::proxy_core::api::logging::srv::STARTED;
    pub(crate) const STOPPED: &str = crate::proxy_core::api::logging::srv::STOPPED;
    pub(crate) const STOP_TIMEOUT: &str = crate::proxy_core::api::logging::srv::STOP_TIMEOUT;
    pub(crate) const TASK_ERROR: &str = crate::proxy_core::api::logging::srv::TASK_ERROR;
    pub(crate) const ACCEPT_ERR: &str = crate::proxy_core::api::logging::srv::ACCEPT_ERR;
    pub(crate) const CONN_ERR: &str = crate::proxy_core::api::logging::srv::CONN_ERR;
}

pub(crate) type ProxyCoreAppKind = crate::proxy_core::api::domain::AppKind;
#[cfg(test)]
pub(crate) type ProxyCoreChannelOverrides =
    crate::proxy_core::api::domain::ChannelOverrides;
pub(crate) type ChannelSpec = crate::proxy_core::api::routing::ChannelSpec;
#[cfg(test)]
pub(crate) type ProxyCoreChannelSpec =
    crate::proxy_core::api::routing::ChannelSpec;
#[cfg(test)]
pub(crate) type ChannelStatus = crate::proxy_core::api::routing::ChannelStatus;
#[cfg(test)]
pub(crate) type ProxyCoreChannelStatus =
    crate::proxy_core::api::routing::ChannelStatus;
pub(crate) type ProxyCoreInterfaceKind =
    crate::proxy_core::api::routing::InterfaceKind;
#[cfg(test)]
pub(crate) type ProxyCoreModelCapabilities =
    crate::proxy_core::api::domain::ModelCapabilities;
#[cfg(test)]
pub(crate) type ProxyCoreModelRoute = crate::proxy_core::api::domain::ModelRoute;
pub(crate) type ModelCatalog = crate::proxy_core::api::model_catalog::ModelCatalog;
#[cfg(test)]
pub(crate) type ProxyCoreProviderMetadata =
    crate::proxy_core::api::domain::ProviderMetadata;
pub(crate) type ProviderSpec = crate::proxy_core::api::domain::ProviderSpec;
#[cfg(test)]
pub(crate) type ProxyCoreProviderSpec =
    crate::proxy_core::api::domain::ProviderSpec;

pub(crate) fn provider_metadata_from_input(input: ProviderMetadataInput) -> ProviderMetadata {
    crate::proxy_core::api::domain::provider_metadata_from_input(input)
}

pub(crate) fn provider_account_ref(
    provider_type: Option<&str>,
    managed_account_id: Option<&str>,
) -> Option<String> {
    crate::proxy_core::api::domain::provider_account_ref(provider_type, managed_account_id)
}

pub(crate) fn model_route_from_input(input: ModelRouteInput) -> ModelRoute {
    crate::proxy_core::api::domain::model_route_from_input(input)
}

pub(crate) fn channel_spec_from_input(input: ChannelSpecInput) -> ChannelSpec {
    crate::proxy_core::api::domain::channel_spec_from_input(input)
}

#[cfg(test)]
pub(crate) type ProxyCoreUpstreamEndpoint =
    crate::proxy_core::api::domain::UpstreamEndpoint;
pub(crate) type UpstreamAuthHeadersInput<'a> =
    crate::proxy_core::api::transport::UpstreamAuthHeadersInput<'a>;
pub(crate) type UpstreamRequestHeadersInput<'a> =
    crate::proxy_core::api::transport::UpstreamRequestHeadersInput<'a>;
pub(crate) type UpstreamSendPolicyInput =
    crate::proxy_core::api::transport::UpstreamSendPolicyInput;
pub(crate) type UpstreamTransportKind =
    crate::proxy_core::api::transport::UpstreamTransportKind;
pub(crate) type ChannelRequestValidationError =
    crate::proxy_core::api::routing::ChannelRequestValidationError;
pub(crate) type ChannelRouteSource =
    crate::proxy_core::api::management::ChannelRouteSource;
pub(crate) type ChannelRecord = crate::proxy_core::api::management::ChannelRecord;
pub(crate) type ChannelReachabilityStatus =
    crate::proxy_core::api::management::ChannelReachabilityStatus;
pub(crate) type ChannelKeyRecord =
    crate::proxy_core::api::management::ChannelKeyRecord;
pub(crate) type ChannelKeyRecordInput =
    crate::proxy_core::api::management::ChannelKeyRecordInput;
pub(crate) type ChannelModelRecord =
    crate::proxy_core::api::management::ChannelModelRecord;
pub(crate) type ChannelModelRecordInput =
    crate::proxy_core::api::management::ChannelModelRecordInput;
pub(crate) type ChannelRecordInput =
    crate::proxy_core::api::management::ChannelRecordInput;

pub(crate) fn channel_model_record_from_input(
    input: ChannelModelRecordInput,
) -> ChannelModelRecord {
    crate::proxy_core::api::management::channel_model_record_from_input(input)
}

pub(crate) fn channel_key_record_from_input(input: ChannelKeyRecordInput) -> ChannelKeyRecord {
    crate::proxy_core::api::management::channel_key_record_from_input(input)
}

pub(crate) fn channel_record_from_input(input: ChannelRecordInput) -> ChannelRecord {
    crate::proxy_core::api::management::channel_record_from_input(input)
}

pub(crate) type InterfaceKind = crate::proxy_core::api::routing::InterfaceKind;
pub(crate) type LegacyChannelModelProjection =
    crate::proxy_core::api::routing::LegacyChannelModelProjection;
pub(crate) type LegacyChannelProjection =
    crate::proxy_core::api::routing::LegacyChannelProjection;
pub(crate) type LegacyChannelProjectionInput =
    crate::proxy_core::api::routing::LegacyChannelProjectionInput;
pub(crate) type LegacyModelRouteInput =
    crate::proxy_core::api::routing::LegacyModelRouteInput;
pub(crate) type LegacyProviderProjectionInput =
    crate::proxy_core::api::routing::LegacyProviderProjectionInput;
pub(crate) type ProxyChannelModelWriteRequest =
    crate::proxy_core::api::management::ProxyChannelModelWriteRequest;
pub(crate) type ProxyChannelKeyPatchRequest =
    crate::proxy_core::api::management::ProxyChannelKeyPatchRequest;
pub(crate) type ProxyChannelKeyWriteRequest =
    crate::proxy_core::api::management::ProxyChannelKeyWriteRequest;
pub(crate) type ProxyChannelPatchRequest =
    crate::proxy_core::api::management::ProxyChannelPatchRequest;
pub(crate) type ProxyChannelWriteRequest =
    crate::proxy_core::api::management::ProxyChannelWriteRequest;
pub(crate) type ProviderSelectionCandidate =
    crate::proxy_core::api::routing::ProviderSelectionCandidate;
pub(crate) type ProviderSelectionFailure =
    crate::proxy_core::api::routing::ProviderSelectionFailure;
pub(crate) type ProviderSelectionInput =
    crate::proxy_core::api::routing::ProviderSelectionInput;
pub(crate) type ProxyCoreError = crate::proxy_core::api::errors::ProxyCoreError;
#[cfg(test)]
pub(crate) type ProxyCoreEventType = crate::proxy_core::api::events::ProxyCoreEventType;
pub(crate) type AppKind = crate::proxy_core::api::domain::AppKind;
#[cfg(test)]
pub(crate) type RetryPolicy = crate::proxy_core::api::domain::RetryPolicy;
pub(crate) type RouteResolveRequest =
    crate::proxy_core::api::management::RouteResolveRequest;
pub(crate) type RouteResolveResponse =
    crate::proxy_core::api::management::RouteResolveResponse;
pub(crate) type ChannelRouteCandidate =
    crate::proxy_core::api::routing::ChannelRouteCandidate;
pub(crate) type ResolvedChannelAttempt =
    crate::proxy_core::api::routing::ResolvedChannelAttempt;
pub(crate) type RoutePlan = crate::proxy_core::api::routing::RoutePlan;
pub(crate) type RouteSelection = crate::proxy_core::api::routing::RouteSelection;
pub(crate) type CodexProxyErrorContext<'a> =
    crate::proxy_core::api::transforms::CodexProxyErrorContext<'a>;
pub(crate) type CodexProxyErrorKind =
    crate::proxy_core::api::transforms::CodexProxyErrorKind;
pub(crate) type ForwardFailureKind =
    crate::proxy_core::api::transport::ForwardFailureKind;
pub(crate) type ManagementAuthError =
    crate::proxy_core::api::auth::ManagementAuthError;
pub(crate) type CircuitBreakerFailureDecision =
    crate::proxy_core::api::config::CircuitBreakerFailureDecision;
pub(crate) use crate::proxy_core::api::management::channel_not_found_error;
pub(crate) use crate::proxy_core::api::ports::{
    app_proxy_config_raw, AuthProvider, ChannelHealthReset, ChannelHealthStore, ChannelSource,
    ForwardPipeline, ModelCatalogProvider, ProviderSource, ProxyConfigSource, ProxyEventSink,
    ProxyServices, RoutePolicySource, RouteResolver, UsageSink,
};
pub(crate) use crate::proxy_core::api::domain::channel_matches_query;
pub(crate) use crate::proxy_core::api::routing::{
    RoutePolicy, RouteRequest, DEFAULT_ROUTE_GROUP,
};
pub(crate) use crate::proxy_core::api::transforms::CLAUDE_API_FORMAT_METADATA_KEY;
pub(crate) use crate::proxy_core::api::auth::{
    resolve_management_auth_decision, validate_management_bearer_header, ManagementAuthDecision,
};
pub(crate) use crate::proxy_core::api::management::{
    plan_channel_test, AppChannelListQuery, AppChannelListSource, AppChannelManagementPlan,
    AppChannelManagementRequest, AppChannelResponse, AppListRequest, AppListResponse,
    AppListSource, AppModelCatalogRequest, AppModelListQuery, ChannelCreateRequest,
    ChannelCreateSource, ChannelDeleteResponse, ChannelDeleteSource, ChannelHealthResetResponse,
    ChannelHealthResetSource, ChannelKeyDeleteResponse, ChannelKeyDeleteSource,
    ChannelKeyPathRequest, ChannelKeyRecordResponse, ChannelKeyRecordSource, ChannelKeysResponse,
    ChannelKeysSource, ChannelListPlan, ChannelListQuery, ChannelListRequest, ChannelListResponse,
    ChannelListSource, ChannelMigrationMaterializeResponse, ChannelMigrationMaterializeSource,
    ChannelMigrationPreviewResponse, ChannelMigrationPreviewSource, ChannelModelsResponse,
    ChannelModelsSource, ChannelPathRequest, ChannelRecordResponse, ChannelRecordSource,
    ChannelRouteRejected,
    ChannelTestPlan, ChannelTestResponse, CurrentRouteResponse, CurrentRouteSource,
    GroupListChannelSource, GroupListQuery, GroupListRequest, HealthCheckRequest,
    HealthCheckResponse, HealthCheckSource, ManagementAppPathRequest, ProviderListResponse,
    ProviderListSource, ProxyChannelModelsReplaceRequest, ProxyChannelTestRequest,
    ProxyStatusRequest, ProxyStatusResponse, ProxyStatusSource, RouteGroupListResponse,
    RouteResolveManagementRequest,
};
pub(crate) use crate::proxy_core::api::model_catalog::{
    ClientModelCatalogResponse, RoutableModelList,
};
pub(crate) use crate::proxy_core::api::transforms::{
    chat_completion_to_response_with_context, claude_stream_usage_event_filter,
    claude_transform_unlabeled_sse_aggregation, codex_stream_usage_event_filter,
    create_codex_chat_to_responses_sse_stream_with_context,
    create_gemini_to_anthropic_sse_stream_with_callbacks,
    create_openai_chat_to_anthropic_sse_stream,
    create_openai_responses_to_anthropic_sse_stream, extract_anthropic_tool_schema_hints,
    gemini_response_to_anthropic_message_with_shadow,
    should_aggregate_codex_oauth_responses_sse, should_use_claude_transform_streaming,
};
pub(crate) use crate::proxy_core::api::transport::{
    append_query_to_endpoint_path, parse_upstream_json_or_unlabeled_sse,
    rebuilt_json_proxy_response, strip_endpoint_prefix, transformed_sse_proxy_response, ProxyBody,
    ProxyRequest, UnlabeledSseFallbackLogContext, UnlabeledSseFallbackLogLevel,
    UpstreamSseAggregationKind,
};
pub(crate) use crate::proxy_core::api::usage::{
    CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG, OPENAI_PARSER_CONFIG,
};
pub(crate) use crate::proxy_core::api::auth::validate_managed_account_upstream_auth;
pub(crate) use crate::proxy_core::api::config::{
    cache_injection_log_message, normalize_thinking_type, rectify_anthropic_request,
    rectify_thinking_budget, should_rectify_thinking_budget, should_rectify_thinking_signature,
    thinking_optimization_log_message,
};
pub(crate) use crate::proxy_core::api::events::{
    attempt_event_name, build_attempt_event_payload, build_request_started_event_payload,
};
pub(crate) use crate::proxy_core::api::model_catalog::{
    apply_copilot_model_normalization, resolve_copilot_model_against_ids,
    strip_one_m_suffix_for_upstream, strip_one_m_suffix_for_upstream_from_body,
};
pub(crate) use crate::proxy_core::api::transforms::{
    resolve_claude_forward_api_format, responses_to_chat_completions_with_options,
};
pub(crate) use crate::proxy_core::api::transport::{
    append_query_to_full_url, apply_bedrock_pre_send_optimizers,
    apply_copilot_warmup_model_override, bedrock_env_flag_from_provider_settings,
    build_codex_oauth_session_headers, build_retryable_forward_failure_log,
    build_terminal_forward_failure_log, build_upstream_auth_headers, categorize_forward_failure,
    classify_copilot_request, claude_transform_endpoint_rewrite_input_from_body,
    contains_image_blocks, interface_kind_for_forward, is_codex_chat_full_endpoint_base,
    is_github_copilot_upstream, is_openai_o_series, is_unsupported_image_error,
    merge_copilot_tool_results,
    prepare_upstream_request_body_with_report, prompt_cache_trace_log_message,
    replace_image_blocks_with_marker, replace_images_for_text_only_model,
    request_body_filter_log_message, request_model_for_forward,
    resolve_copilot_deterministic_interaction_id,
    resolve_copilot_optimizer_session_id, resolve_copilot_request_id_with_fallback,
    resolve_media_prevention_policy, resolved_copilot_dynamic_base_url,
    sanitize_copilot_orphan_tool_results, should_apply_bedrock_pre_send_optimizer,
    should_check_media_retry, should_failover_after_rectifier_retry_failure,
    should_preserve_exact_request_header_case, should_resolve_copilot_dynamic_endpoint,
    should_send_anthropic_request_headers, should_trigger_media_retry, split_endpoint_and_query,
    strip_copilot_thinking_blocks, supports_reasoning_effort,
    UNSUPPORTED_IMAGE_MARKER,
    rewrite_claude_transform_endpoint,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::auth::ManagedAccountAuthError;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::{canonical_json_string, short_value_hash};

pub(crate) const SESSION_REQUEST_ID_PREFIX: &str =
    crate::proxy_core::api::usage::SESSION_REQUEST_ID_PREFIX;
pub(crate) const PROXY_EVENTS_CONNECTED_EVENT: &str =
    crate::proxy_core::api::events::PROXY_EVENTS_CONNECTED_EVENT;
pub(crate) const PROXY_EVENTS_LAGGED_EVENT: &str =
    crate::proxy_core::api::events::PROXY_EVENTS_LAGGED_EVENT;

pub(crate) fn build_proxy_events_connected_payload(buffer_size: usize) -> Value {
    crate::proxy_core::api::events::build_proxy_events_connected_payload(buffer_size)
}

pub(crate) fn build_proxy_events_lagged_payload(skipped: u64) -> Value {
    crate::proxy_core::api::events::build_proxy_events_lagged_payload(skipped)
}

pub(crate) fn proxy_event_envelope_to_sse_spec(
    event: &ProxyEventEnvelope,
) -> ProxyEventSseSpec {
    event.to_sse_spec()
}

pub(crate) fn append_utf8_safe(
    buffer: &mut String,
    remainder: &mut Vec<u8>,
    new_bytes: &[u8],
) {
    crate::proxy_core::api::transforms::append_utf8_safe(buffer, remainder, new_bytes);
}

pub(crate) fn take_sse_block(buffer: &mut String) -> Option<String> {
    crate::proxy_core::api::transforms::take_sse_block(buffer)
}

pub(crate) fn inspect_codex_chat_history_sse_block(
    block: &str,
) -> Option<CodexChatHistorySseInspection> {
    crate::proxy_core::api::transforms::inspect_codex_chat_history_sse_block(block)
}

pub(crate) fn build_codex_upstream_url(base_url: &str, endpoint: &str) -> String {
    crate::proxy_core::api::transport::build_codex_upstream_url(base_url, endpoint)
}

pub(crate) fn should_convert_codex_responses_endpoint_to_chat(
    provider_uses_chat_completions: bool,
    endpoint: &str,
) -> bool {
    crate::proxy_core::api::transport::should_convert_codex_responses_endpoint_to_chat(
        provider_uses_chat_completions,
        endpoint,
    )
}

pub(crate) fn resolve_codex_provider_uses_chat_completions(
    api_format: Option<&str>,
    wire_api: Option<&str>,
    base_url: Option<&str>,
    config_base_url: Option<&str>,
) -> bool {
    crate::proxy_core::api::transport::resolve_codex_provider_uses_chat_completions(
        api_format,
        wire_api,
        base_url,
        config_base_url,
    )
}

pub(crate) fn build_codex_bearer_auth_headers(
    api_key: &str,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyCoreError> {
    crate::proxy_core::api::transport::build_codex_bearer_auth_headers(api_key)
}

pub(crate) fn resolve_codex_provider_upstream_model(
    settings_model: Option<&str>,
    config_model: Option<&str>,
) -> Option<String> {
    crate::proxy_core::api::transport::resolve_codex_provider_upstream_model(
        settings_model,
        config_model,
    )
}

pub(crate) fn codex_provider_catalog_model_ids_from_settings(
    settings_config: &Value,
) -> std::collections::HashSet<String> {
    crate::proxy_core::api::transport::codex_provider_catalog_model_ids_from_settings(
        settings_config,
    )
}

pub(crate) fn apply_codex_chat_upstream_model_policy(
    body: &mut Value,
    uses_chat_completions: bool,
    upstream_model: Option<&str>,
    catalog_model_ids: &std::collections::HashSet<String>,
) -> Option<String> {
    crate::proxy_core::api::transport::apply_codex_chat_upstream_model_policy(
        body,
        uses_chat_completions,
        upstream_model,
        catalog_model_ids,
    )
}

pub(crate) fn infer_codex_chat_reasoning_profile(
    provider_name: &str,
    base_url: &str,
    model: &str,
) -> Option<CodexChatReasoningProfile> {
    crate::proxy_core::api::transforms::infer_codex_chat_reasoning_profile(
        provider_name,
        base_url,
        model,
    )
}

pub(crate) fn normalize_codex_chat_reasoning_profile(
    profile: CodexChatReasoningProfile,
) -> CodexChatReasoningProfile {
    crate::proxy_core::api::transforms::normalize_codex_chat_reasoning_profile(profile)
}

pub(crate) fn resolve_response_runtime_policy(
    auto_failover_enabled: bool,
    max_retries: u32,
    non_streaming_timeout: u64,
    streaming_first_byte_timeout: u64,
    streaming_idle_timeout: u64,
) -> ResponseRuntimePolicy {
    crate::proxy_core::api::transport::resolve_response_runtime_policy(
        auto_failover_enabled,
        max_retries,
        non_streaming_timeout,
        streaming_first_byte_timeout,
        streaming_idle_timeout,
    )
}

pub(crate) fn extract_gemini_model_from_path(endpoint: &str) -> Option<String> {
    crate::proxy_core::api::transport::extract_gemini_model_from_path(endpoint)
}

pub(crate) fn extract_gemini_api_key_from_settings(settings: &Value) -> Option<String> {
    crate::proxy_core::api::auth::extract_gemini_api_key_from_settings(settings)
}

pub(crate) fn extract_gemini_base_url_from_settings(settings: &Value) -> Option<String> {
    crate::proxy_core::api::auth::extract_gemini_base_url_from_settings(settings)
}

pub(crate) fn parse_gemini_oauth_credentials(
    key: &str,
) -> Option<GeminiOAuthCredentials> {
    crate::proxy_core::api::auth::parse_gemini_oauth_credentials(key)
}

pub(crate) fn build_gemini_upstream_url(base_url: &str, endpoint: &str) -> String {
    crate::proxy_core::api::transforms::build_gemini_upstream_url(base_url, endpoint)
}

pub(crate) fn build_gemini_auth_headers(
    api_key: &str,
    access_token: Option<&str>,
    use_oauth: bool,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyCoreError> {
    crate::proxy_core::api::transport::build_gemini_auth_headers(api_key, access_token, use_oauth)
}

pub(crate) fn claude_api_format_from_metadata(metadata: &Value, fallback: &str) -> String {
    crate::proxy_core::api::transforms::claude_api_format_from_metadata(metadata, fallback)
}

pub(crate) fn resolve_claude_api_format_from_settings(
    provider_type: Option<&str>,
    meta_api_format: Option<&str>,
    settings_config: &Value,
) -> &'static str {
    crate::proxy_core::api::transforms::resolve_claude_api_format_from_settings(
        provider_type,
        meta_api_format,
        settings_config,
    )
}

pub(crate) fn infer_claude_provider_kind(
    api_format: &str,
    uses_google_oauth: bool,
    meta_provider_type: Option<&str>,
    base_url: Option<&str>,
    settings_config: &Value,
) -> ProviderKind {
    crate::proxy_core::api::domain::infer_claude_provider_kind(
        api_format,
        uses_google_oauth,
        meta_provider_type,
        base_url,
        settings_config,
    )
}

pub(crate) fn is_gemini_oauth_key_shape(key: &str) -> bool {
    crate::proxy_core::api::auth::is_gemini_oauth_key_shape(key)
}

pub(crate) fn is_copilot_prompt_cache_provider(
    meta_provider_type: Option<&str>,
    settings_config: &Value,
) -> bool {
    crate::proxy_core::api::transforms::is_copilot_prompt_cache_provider(
        meta_provider_type,
        settings_config,
    )
}

pub(crate) fn resolve_claude_responses_prompt_cache_key(
    body: &Value,
    explicit_cache_key: Option<&str>,
    session_id: Option<&str>,
    is_copilot: bool,
) -> ClaudePromptCacheKeyResolution {
    crate::proxy_core::api::transforms::resolve_claude_responses_prompt_cache_key(
        body,
        explicit_cache_key,
        session_id,
        is_copilot,
    )
}

pub(crate) fn extract_claude_auth_key_from_settings(
    settings_config: &Value,
) -> Option<ClaudeAuthKey> {
    crate::proxy_core::api::auth::extract_claude_auth_key_from_settings(settings_config)
}

pub(crate) fn extract_claude_base_url_from_settings(
    is_codex_oauth: bool,
    settings_config: &Value,
) -> Option<String> {
    crate::proxy_core::api::domain::extract_claude_base_url_from_settings(
        is_codex_oauth,
        settings_config,
    )
}

pub(crate) fn build_claude_upstream_url(base_url: &str, endpoint: &str) -> String {
    crate::proxy_core::api::transport::build_claude_upstream_url(base_url, endpoint)
}

pub(crate) fn build_claude_auth_headers(
    kind: ClaudeAuthHeaderKind,
    api_key: &str,
    access_token: Option<&str>,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyCoreError> {
    crate::proxy_core::api::transport::build_claude_auth_headers(kind, api_key, access_token)
}

pub(crate) fn build_copilot_auth_headers(
    input: CopilotAuthHeadersInput<'_>,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyCoreError> {
    crate::proxy_core::api::transport::build_copilot_auth_headers(input)
}

pub(crate) fn anthropic_to_openai_responses_request(
    body: &Value,
    cache_key: Option<&str>,
    is_codex_oauth: bool,
    codex_fast_mode: bool,
) -> Value {
    crate::proxy_core::api::transforms::anthropic_to_openai_responses_request(
        body,
        cache_key,
        is_codex_oauth,
        codex_fast_mode,
    )
}

pub(crate) fn anthropic_to_openai_chat_request(
    body: &Value,
    preserve_reasoning_content: bool,
) -> Value {
    crate::proxy_core::api::transforms::anthropic_to_openai_chat_request(
        body,
        preserve_reasoning_content,
    )
}

pub(crate) fn anthropic_request_to_gemini_request_with_shadow(
    body: &Value,
    shadow_store: Option<&GeminiShadowStore>,
    provider_id: Option<&str>,
    session_id: Option<&str>,
) -> Result<Value, String> {
    crate::proxy_core::api::transforms::anthropic_request_to_gemini_request_with_shadow(
        body,
        shadow_store,
        provider_id,
        session_id,
    )
}

pub(crate) fn openai_responses_to_anthropic_message(body: &Value) -> Result<Value, String> {
    crate::proxy_core::api::transforms::openai_responses_to_anthropic_message(body)
}

pub(crate) fn openai_chat_to_anthropic_message(body: &Value) -> Result<Value, String> {
    crate::proxy_core::api::transforms::openai_chat_to_anthropic_message(body)
}

pub(crate) fn gemini_response_to_anthropic_message<F>(
    body: &Value,
    tool_schema_hints: Option<&AnthropicToolSchemaHints>,
    synthesize_tool_call_id: F,
) -> Result<GeminiToAnthropicMessageOutput, String>
where
    F: FnMut() -> String,
{
    crate::proxy_core::api::transforms::gemini_response_to_anthropic_message(
        body,
        tool_schema_hints,
        synthesize_tool_call_id,
    )
}

pub(crate) fn should_preserve_reasoning_content_for_openai_chat(
    settings_config: &Value,
    body: &Value,
) -> bool {
    crate::proxy_core::api::transforms::should_preserve_reasoning_content_for_openai_chat(
        settings_config,
        body,
    )
}

pub(crate) fn circuit_breaker_config_from_app_config(
    config: Option<&AppProxyConfig>,
) -> CircuitBreakerConfig {
    crate::proxy_core::api::config::circuit_breaker_config_from_app_config(config)
}

pub(crate) fn circuit_failure_threshold_from_app_config(
    config: Option<&AppProxyConfig>,
    fallback: u32,
) -> u32 {
    crate::proxy_core::api::config::circuit_failure_threshold_from_app_config(config, fallback)
}

pub(crate) fn provider_circuit_key(app_type: &str, provider_id: &str) -> String {
    crate::proxy_core::api::config::provider_circuit_key(app_type, provider_id)
}

pub(crate) fn channel_circuit_key(app_type: &str, channel_id: &str) -> String {
    crate::proxy_core::api::config::channel_circuit_key(app_type, channel_id)
}

pub(crate) fn provider_circuit_key_prefix(app_type: &str) -> String {
    crate::proxy_core::api::config::provider_circuit_key_prefix(app_type)
}

pub(crate) fn channel_circuit_key_prefix(app_type: &str) -> String {
    crate::proxy_core::api::config::channel_circuit_key_prefix(app_type)
}

pub(crate) fn app_type_from_circuit_key(key: &str) -> &str {
    crate::proxy_core::api::config::app_type_from_circuit_key(key)
}

pub(crate) fn select_provider_ids(
    input: ProviderSelectionInput,
) -> Result<Vec<String>, ProviderSelectionFailure> {
    crate::proxy_core::api::routing::select_provider_ids(input)
}

pub(crate) fn resolve_channel_route(
    request: RouteResolveRequest,
    channels: Vec<RouteResolveChannelInput>,
    source: ChannelRouteSource,
) -> Result<RouteResolveResponse, ProxyCoreError> {
    crate::proxy_core::api::routing::resolve_channel_route(request, channels, source)
}

pub(crate) fn reject_unavailable_channel_ids<I, S>(
    response: &mut RouteResolveResponse,
    unavailable_channel_ids: I,
) where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    crate::proxy_core::api::routing::reject_unavailable_channel_ids(
        response,
        unavailable_channel_ids,
    );
}

pub(crate) fn stable_channel_id(
    app_type: &str,
    provider_id: &str,
    source_kind: &str,
    base_url: &str,
) -> String {
    crate::proxy_core::api::routing::stable_channel_id(
        app_type,
        provider_id,
        source_kind,
        base_url,
    )
}

pub(crate) fn legacy_channel_priority(
    provider_id: &str,
    in_failover_queue: bool,
    current_provider_id: Option<&str>,
) -> i64 {
    crate::proxy_core::api::routing::legacy_channel_priority(
        provider_id,
        in_failover_queue,
        current_provider_id,
    )
}

pub(crate) fn infer_legacy_channel_interface(
    app: Option<&ProxyCoreAppKind>,
    provider: &LegacyProviderProjectionInput,
) -> ProxyCoreInterfaceKind {
    crate::proxy_core::api::routing::infer_legacy_channel_interface(app, provider)
}

pub(crate) fn build_legacy_channel_projection(
    input: LegacyChannelProjectionInput,
) -> LegacyChannelProjection {
    crate::proxy_core::api::routing::build_legacy_channel_projection(input)
}

pub(crate) fn normalize_required_channel_string(
    value: &str,
    field: &str,
) -> Result<String, ChannelRequestValidationError> {
    crate::proxy_core::api::routing::normalize_required_channel_string(value, field)
}

pub(crate) fn normalize_optional_channel_string(value: String) -> Option<String> {
    crate::proxy_core::api::routing::normalize_optional_channel_string(value)
}

pub(crate) fn normalize_channel_base_url(value: &str) -> String {
    crate::proxy_core::api::routing::normalize_channel_base_url(value)
}

pub(crate) fn normalize_channel_groups(groups: Vec<String>) -> Vec<String> {
    crate::proxy_core::api::routing::normalize_channel_groups(groups)
}

pub(crate) fn channel_object_or_default(value: Value) -> Value {
    crate::proxy_core::api::routing::channel_object_or_default(value)
}

pub(crate) fn channel_array_or_default(value: Value) -> Value {
    crate::proxy_core::api::routing::channel_array_or_default(value)
}

pub(crate) fn validate_proxy_channel_write_request_fields(
    request: &ProxyChannelWriteRequest,
) -> Result<(), ChannelRequestValidationError> {
    crate::proxy_core::api::routing::validate_proxy_channel_write_request_fields(request)
}

pub(crate) fn validate_proxy_channel_model_write_request_fields(
    model: &ProxyChannelModelWriteRequest,
) -> Result<(), ChannelRequestValidationError> {
    crate::proxy_core::api::routing::validate_proxy_channel_model_write_request_fields(model)
}

pub(crate) fn validate_optional_channel_auth_profile_ref(
    auth_profile_ref: Option<&str>,
) -> Result<(), ChannelRequestValidationError> {
    crate::proxy_core::api::routing::validate_optional_channel_auth_profile_ref(auth_profile_ref)
}

pub(crate) fn validate_proxy_channel_key_patch_request_fields(
    request: &ProxyChannelKeyPatchRequest,
) -> Result<(), ChannelRequestValidationError> {
    crate::proxy_core::api::routing::validate_proxy_channel_key_patch_request_fields(request)
}

impl From<&AppType> for AppKind {
    fn from(value: &AppType) -> Self {
        match value {
            AppType::Claude => Self::Claude,
            AppType::ClaudeDesktop => Self::ClaudeDesktop,
            AppType::Codex => Self::Codex,
            AppType::Gemini => Self::Gemini,
            AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => {
                Self::Custom(value.as_str().to_string())
            }
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreProviderSpec {
    fn to_proxy_core_provider_spec(&self, app_type: &AppType) -> ProviderSpec;
}

impl ToProxyCoreProviderSpec for Provider {
    fn to_proxy_core_provider_spec(&self, app_type: &AppType) -> ProviderSpec {
        let kind = provider_kind_from_app_type_and_config(app_type, self);
        let metadata = provider_metadata_without_secrets(self);

        ProviderSpec {
            id: self.id.clone(),
            name: self.name.clone(),
            kind,
            account_ref: account_ref(self),
            metadata,
        }
    }
}

pub(crate) fn proxy_providers_to_core_specs(
    app_type: &AppType,
    providers: impl IntoIterator<Item = Provider>,
) -> Vec<ProviderSpec> {
    providers
        .into_iter()
        .map(|provider| provider.to_proxy_core_provider_spec(app_type))
        .collect()
}

pub(crate) fn proxy_current_route_provider_summary_input(
    provider: Provider,
    app_type: &AppType,
) -> CurrentRouteProviderSummaryInput {
    CurrentRouteProviderSummaryInput::from_provider_spec(
        provider.to_proxy_core_provider_spec(app_type),
    )
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelSpec {
    fn to_proxy_core_channel_spec(&self) -> ChannelSpec;
}

impl ToProxyCoreChannelSpec for ProxyChannelRecord {
    fn to_proxy_core_channel_spec(&self) -> ChannelSpec {
        channel_spec_from_input(ChannelSpecInput {
            id: self.id.clone(),
            provider_id: self.provider_id.clone(),
            app_type: self.app_type.clone(),
            name: self.name.clone(),
            status: self.status.clone(),
            base_url: self.base_url.clone(),
            interface_kind: self.interface_kind.clone(),
            auth_profile_ref: self.auth_profile_ref.clone(),
            models: self
                .models
                .iter()
                .map(ProxyChannelModelRecord::to_proxy_core_model_route_input)
                .collect(),
            groups: self.groups.clone(),
            priority: self.priority,
            weight: self.weight,
            retry_policy: self.retry_policy.clone(),
            health_policy: self.health_policy.clone(),
            header_overrides: self.header_overrides.clone(),
            param_overrides: self.param_overrides.clone(),
            status_code_mapping: self.status_code_mapping.clone(),
            tags: self.tags.clone(),
            metadata: self.metadata.clone(),
            source_ref: self.source_endpoint_url.clone(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        })
    }
}

pub(crate) fn proxy_channel_record_to_core_spec(channel: &ProxyChannelRecord) -> ChannelSpec {
    channel.to_proxy_core_channel_spec()
}

pub(crate) fn proxy_channel_specs_to_core(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
) -> Vec<ChannelSpec> {
    channels
        .into_iter()
        .map(|channel| channel.to_proxy_core_channel_spec())
        .collect()
}

pub(crate) fn proxy_channel_route_inputs_to_core(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
) -> Vec<RouteResolveChannelInput> {
    channels
        .into_iter()
        .map(|channel| {
            let ProxyChannelRecord {
                id,
                provider_id,
                name,
                status,
                base_url,
                interface_kind,
                groups,
                models,
                priority,
                weight,
                source_kind,
                ..
            } = channel;

            RouteResolveChannelInput {
                channel_id: id,
                provider_id,
                channel_name: name,
                status,
                base_url,
                interface_kind,
                groups,
                models: models
                    .into_iter()
                    .map(|model| RouteResolveModelInput {
                        public_model: model.public_model,
                        upstream_model: model.upstream_model,
                    })
                    .collect(),
                priority,
                weight,
                source_kind: source_kind.as_str().to_string(),
            }
        })
        .collect()
}

pub(crate) fn proxy_app_summary_input(
    app_type: &AppType,
    enabled: bool,
    auto_failover_enabled: bool,
    providers: impl IntoIterator<Item = Provider>,
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
) -> AppSummaryInput {
    AppSummaryInput::from_specs(
        app_type.as_str(),
        enabled,
        auto_failover_enabled,
        proxy_providers_to_core_specs(app_type, providers),
        proxy_channel_specs_to_core(channels),
    )
}

pub(crate) fn claude_desktop_model_routes_to_core_response(
    routes: impl IntoIterator<Item = ResolvedModelRoute>,
) -> ClaudeDesktopModelListResponse {
    ClaudeDesktopModelListResponse::from_routes(
        routes
            .into_iter()
            .map(|route| ClaudeDesktopModelRouteInput::new(route.route_id, route.supports_1m)),
    )
}

pub(crate) fn codex_default_model_context_window() -> u64 {
    crate::proxy_core::api::model_catalog::DEFAULT_CODEX_MODEL_CONTEXT_WINDOW
}

pub(crate) fn codex_settings_have_model_catalog_specs(settings: &Value) -> bool {
    crate::proxy_core::api::model_catalog::has_codex_model_catalog_specs(settings)
}

pub(crate) fn codex_model_catalog_from_settings(
    settings: &Value,
    default_context_window: u64,
    template: &Value,
) -> Option<Value> {
    crate::proxy_core::api::model_catalog::build_codex_model_catalog_from_settings(
        settings,
        default_context_window,
        template,
    )
}

pub(crate) fn simplify_codex_model_catalog(
    catalog_text: &str,
    default_context_window: u64,
) -> Option<Value> {
    crate::proxy_core::api::model_catalog::simplify_codex_model_catalog(
        catalog_text,
        default_context_window,
    )
}

pub(crate) fn provider_model_catalog_from_settings(
    provider_id: &str,
    settings: Option<&Value>,
) -> ModelCatalog {
    crate::proxy_core::api::model_catalog::provider_model_catalog_from_settings(
        provider_id,
        settings,
    )
}

pub(crate) fn client_model_catalog_from_raw(app: &AppKind, raw: Value) -> ModelCatalog {
    crate::proxy_core::api::model_catalog::client_model_catalog_from_raw(app.as_str(), raw)
}

pub(crate) fn route_plan_provider_ids(plan: &RoutePlan) -> Vec<String> {
    crate::proxy_core::api::routing::route_plan_provider_ids(plan)
}

pub(crate) fn route_plan_selections(plan: &RoutePlan) -> &[RouteSelection] {
    crate::proxy_core::api::routing::route_plan_selections(plan)
}

pub(crate) fn route_selection_for_forward_result(
    plan: &RoutePlan,
    selected_channel_id: Option<&str>,
    provider_id: &str,
) -> RouteSelection {
    crate::proxy_core::api::routing::select_route_for_forward_result(
        plan,
        selected_channel_id,
        provider_id,
    )
}

pub(crate) fn route_plan_from_request(
    request: RouteRequest<'_>,
) -> ProxyCoreResult<RoutePlan> {
    crate::proxy_core::api::routing::build_route_plan(request)
}

pub(crate) fn should_transition_open_to_half_open(
    open_elapsed_seconds: Option<u64>,
    timeout_seconds: u64,
) -> bool {
    crate::proxy_core::api::config::should_transition_open_to_half_open(
        open_elapsed_seconds,
        timeout_seconds,
    )
}

pub(crate) fn should_close_half_open_after_success(
    consecutive_successes: u32,
    success_threshold: u32,
) -> bool {
    crate::proxy_core::api::config::should_close_half_open_after_success(
        consecutive_successes,
        success_threshold,
    )
}

pub(crate) fn half_open_probe_allow_result(current_requests: u32, max_requests: u32) -> AllowResult {
    crate::proxy_core::api::config::half_open_probe_allow_result(current_requests, max_requests)
}

pub(crate) fn circuit_breaker_failure_decision(
    state: CircuitState,
    consecutive_failures: u32,
    total_requests: u32,
    failed_requests: u32,
    config: &CircuitBreakerConfig,
) -> CircuitBreakerFailureDecision {
    crate::proxy_core::api::config::circuit_breaker_failure_decision(
        state,
        consecutive_failures,
        total_requests,
        failed_requests,
        config,
    )
}

pub(crate) fn forward_failure_kind_from_proxy_status(
    kind: ProxyErrorStatusKind,
    message: impl Into<String>,
    upstream_body: Option<String>,
) -> ForwardFailureKind {
    crate::proxy_core::api::transport::forward_failure_kind_from_proxy_status(
        kind,
        message,
        upstream_body,
    )
}

pub(crate) fn channel_route_candidate_from_selection(
    selection: &RouteSelection,
) -> ChannelRouteCandidate {
    crate::proxy_core::api::routing::route_candidate_from_selection(
        selection,
        DEFAULT_ROUTE_GROUP,
        "proxy_core",
    )
}

#[cfg(test)]
pub(crate) fn resolved_channel_attempt_from_candidate(
    candidate: ChannelRouteCandidate,
) -> ResolvedChannelAttempt {
    crate::proxy_core::api::routing::resolved_channel_attempt_from_candidate(candidate)
}

pub(crate) fn resolved_channel_attempt_from_selection(
    selection: &RouteSelection,
) -> ResolvedChannelAttempt {
    crate::proxy_core::api::routing::resolved_channel_attempt_from_selection(selection)
}

pub(crate) fn apply_channel_param_overrides_to_url(
    url: &str,
    param_overrides: &Value,
) -> String {
    crate::proxy_core::api::transport::apply_channel_param_overrides_to_url(
        url,
        param_overrides,
    )
}

pub(crate) fn proxy_core_error_is_unavailable(error: &ProxyCoreError) -> bool {
    matches!(error, ProxyCoreError::Unavailable(_))
}

pub(crate) fn codex_proxy_error_code(kind: CodexProxyErrorKind) -> &'static str {
    crate::proxy_core::api::transforms::codex_proxy_error_code(kind)
}

#[cfg(test)]
pub(crate) fn codex_proxy_error_json(ctx: CodexProxyErrorContext<'_>) -> Value {
    crate::proxy_core::api::transforms::codex_proxy_error_json(ctx)
}

pub(crate) fn codex_proxy_error_response(
    status: ProxyErrorStatusKind,
    ctx: CodexProxyErrorContext<'_>,
) -> ProxyCoreResult<ProxyCoreResponse> {
    crate::proxy_core::api::transforms::codex_proxy_error_response(status, ctx)
}

pub(crate) fn apply_channel_route_model_override(
    body: &mut Value,
    public_model: Option<&str>,
    upstream_model: Option<&str>,
) -> Option<String> {
    crate::proxy_core::api::transport::apply_channel_route_model_override(
        body,
        public_model,
        upstream_model,
    )
}

pub(crate) fn apply_channel_provider_overrides(
    app_type: &AppType,
    provider: &mut Provider,
    candidate: &ChannelRouteCandidate,
) {
    let plan = crate::proxy_core::api::routing::channel_provider_override_plan(
        &AppKind::from(app_type),
        candidate,
    );
    crate::proxy_core::api::routing::apply_channel_provider_settings_overrides(
        &mut provider.settings_config,
        &plan,
    );

    if let Some(api_format) = plan.api_format {
        provider
            .meta
            .get_or_insert_with(ProviderMeta::default)
            .api_format = Some(api_format);
    }
}

pub(crate) fn codex_tool_context_from_request(body: &Value) -> CodexToolContext {
    crate::proxy_core::api::transforms::build_codex_tool_context_from_request(body)
}

pub(crate) fn normalize_codex_chat_error_body(body: &[u8]) -> CodexChatErrorNormalization {
    crate::proxy_core::api::transforms::normalize_codex_chat_error_body(body)
}

pub(crate) fn should_normalize_anthropic_tool_thinking_history(
    settings_config: &Value,
    body: &Value,
    api_format: &str,
) -> bool {
    crate::proxy_core::api::transforms::should_normalize_anthropic_tool_thinking_history(
        settings_config,
        body,
        api_format,
    )
}

pub(crate) fn normalize_anthropic_tool_thinking_history(body: &mut Value) -> bool {
    crate::proxy_core::api::transforms::normalize_anthropic_tool_thinking_history(body)
}

pub(crate) fn normalize_deepseek_thinking_disabled_strip_effort(
    body: &mut Value,
    settings_config: &Value,
) -> bool {
    crate::proxy_core::api::transforms::normalize_deepseek_thinking_disabled_strip_effort(
        body,
        settings_config,
    )
}

pub(crate) fn inject_openai_stream_include_usage(body: &mut Value) {
    crate::proxy_core::api::transport::inject_openai_stream_include_usage(body);
}

#[cfg(test)]
pub(crate) fn anthropic_tool_thinking_placeholder() -> &'static str {
    crate::proxy_core::api::transforms::ANTHROPIC_TOOL_THINKING_PLACEHOLDER
}

#[cfg(test)]
pub(crate) fn anthropic_redacted_thinking_placeholder() -> &'static str {
    crate::proxy_core::api::transforms::ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
}

pub(crate) type ModelMappingProjection =
    crate::proxy_core::api::model_catalog::ModelMappingProjection;

pub(crate) fn apply_provider_model_mapping(
    body: Value,
    provider_settings: &Value,
) -> ModelMappingProjection {
    crate::proxy_core::api::model_catalog::apply_provider_model_mapping(body, provider_settings)
}

pub(crate) fn rewrite_codex_responses_endpoint_to_chat(
    endpoint: &str,
) -> (String, Option<String>) {
    crate::proxy_core::api::transport::rewrite_codex_responses_endpoint_to_chat(endpoint)
        .into_parts()
}

pub(crate) fn resolve_gemini_native_url(
    base_url: &str,
    endpoint: &str,
    is_full_url: bool,
) -> String {
    crate::proxy_core::api::transforms::resolve_gemini_native_url(
        base_url,
        endpoint,
        is_full_url,
    )
}

pub(crate) fn claude_api_format_needs_transform(api_format: &str) -> bool {
    crate::proxy_core::api::transforms::claude_api_format_needs_transform(api_format)
}

pub(crate) fn anthropic_beta_header_value(existing_beta: Option<&str>) -> String {
    crate::proxy_core::api::transport::anthropic_beta_header_value(existing_beta)
}

pub(crate) fn build_upstream_request_headers(
    input: UpstreamRequestHeadersInput<'_>,
) -> HeaderMap {
    crate::proxy_core::api::transport::build_upstream_request_headers(input)
}

pub(crate) fn upstream_host_header_from_url(url: &str) -> Option<String> {
    crate::proxy_core::api::transport::upstream_host_header_from_url(url)
}

pub(crate) fn serialize_upstream_request_body(
    method: &http::Method,
    body: &Value,
) -> serde_json::Result<Vec<u8>> {
    crate::proxy_core::api::transport::serialize_upstream_request_body(method, body)
}

pub(crate) fn resolve_upstream_request_transport_policy(
    needs_transform: bool,
    codex_responses_to_chat: bool,
    endpoint: &str,
    body: &Value,
    headers: &HeaderMap,
) -> UpstreamRequestTransportPolicy {
    crate::proxy_core::api::transport::resolve_upstream_request_transport_policy(
        needs_transform,
        codex_responses_to_chat,
        endpoint,
        body,
        headers,
    )
}

#[cfg(test)]
pub(crate) fn is_streaming_upstream_request(
    endpoint: &str,
    body: &Value,
    headers: &HeaderMap,
) -> bool {
    crate::proxy_core::api::transport::is_streaming_upstream_request(endpoint, body, headers)
}

pub(crate) fn is_socks_proxy_url(upstream_proxy_url: Option<&str>) -> bool {
    crate::proxy_core::api::transport::is_socks_proxy_url(upstream_proxy_url)
}

pub(crate) fn resolve_upstream_send_policy(
    input: UpstreamSendPolicyInput,
) -> UpstreamSendPolicy {
    crate::proxy_core::api::transport::resolve_upstream_send_policy(input)
}

pub(crate) fn response_headers_log_summary(headers: &HeaderMap) -> String {
    crate::proxy_core::api::transport::response_headers_log_summary(headers)
}

pub(crate) fn get_content_encoding(headers: &HeaderMap) -> Option<String> {
    crate::proxy_core::api::transport::get_content_encoding(headers)
}

pub(crate) fn decode_response_body(
    headers: &mut HeaderMap,
    raw_body: &[u8],
) -> ResponseBodyDecode {
    crate::proxy_core::api::transport::decode_response_body(headers, raw_body)
}

#[cfg(test)]
pub(crate) fn decompress_body(
    content_encoding: &str,
    body: &[u8],
) -> Result<Option<Vec<u8>>, std::io::Error> {
    crate::proxy_core::api::transport::decompress_body(content_encoding, body)
}

#[cfg(test)]
pub(crate) fn strip_sse_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    crate::proxy_core::api::transforms::strip_sse_field(line, field)
}

pub(crate) fn non_streaming_body_timeout_message(timeout: std::time::Duration) -> String {
    crate::proxy_core::api::transport::non_streaming_body_timeout_message(timeout)
}

pub(crate) fn passthrough_bytes_proxy_response(
    status: StatusCode,
    headers: HeaderMap,
    body: impl Into<Bytes>,
) -> ProxyCoreResponse {
    crate::proxy_core::api::transport::passthrough_bytes_proxy_response(status, headers, body)
}

pub(crate) fn passthrough_stream_proxy_response(
    status: StatusCode,
    headers: HeaderMap,
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
) -> ProxyCoreResponse {
    crate::proxy_core::api::transport::passthrough_stream_proxy_response(status, headers, stream)
}

#[cfg(test)]
pub(crate) fn is_official_codex_client_user_agent(user_agent: &str) -> bool {
    crate::proxy_core::api::transport::is_official_codex_client_user_agent(user_agent)
}

#[cfg(test)]
pub(crate) fn build_gemini_native_url(base_url: &str, endpoint: &str) -> String {
    crate::proxy_core::api::transforms::build_gemini_native_url(base_url, endpoint)
}

pub(crate) struct UsageRequestLogProjection {
    pub(crate) log: RequestLog,
    pub(crate) missing_pricing_model: Option<String>,
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn success_usage_record_with_request_id_fallback(
    provider_id: &str,
    provider_kind: Option<ProviderKind>,
    app: AppKind,
    response_model: &str,
    request_model: &str,
    outbound_model: &str,
    usage: TokenUsage,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    is_streaming: bool,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> UsageRecord {
    crate::proxy_core::api::usage::success_usage_record_with_request_id_fallback(
        provider_id,
        provider_kind,
        app,
        response_model,
        request_model,
        outbound_model,
        usage,
        latency_ms,
        first_token_ms,
        is_streaming,
        status_code,
        session_id,
        request_id_fallback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn error_usage_record_with_request_id_fallback(
    provider_id: &str,
    provider_kind: Option<ProviderKind>,
    app: AppKind,
    request_model: &str,
    outbound_model: Option<&str>,
    status_code: u16,
    error_message: String,
    latency_ms: u64,
    is_streaming: bool,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> UsageRecord {
    crate::proxy_core::api::usage::error_usage_record_with_request_id_fallback(
        provider_id,
        provider_kind,
        app,
        request_model,
        outbound_model,
        status_code,
        error_message,
        latency_ms,
        is_streaming,
        session_id,
        request_id_fallback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn transformed_response_usage_record_with_request_id_fallback(
    body: &Value,
    format: TransformedResponseUsageFormat,
    provider_id: &str,
    provider_kind: Option<ProviderKind>,
    app: AppKind,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> Option<UsageRecord> {
    crate::proxy_core::api::usage::transformed_response_usage_record_with_request_id_fallback(
        body,
        format,
        provider_id,
        provider_kind,
        app,
        request_model,
        outbound_model,
        latency_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn transformed_streaming_response_usage_record_with_request_id_fallback(
    events: &[Value],
    format: TransformedResponseUsageFormat,
    provider_id: &str,
    provider_kind: Option<ProviderKind>,
    app: AppKind,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> Option<UsageRecord> {
    crate::proxy_core::api::usage::transformed_streaming_response_usage_record_with_request_id_fallback(
        events,
        format,
        provider_id,
        provider_kind,
        app,
        request_model,
        outbound_model,
        latency_ms,
        first_token_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn streaming_response_usage_record_with_optional_outbound_model(
    events: &[Value],
    stream_parser: fn(&[Value]) -> Option<TokenUsage>,
    model_extractor: fn(&[Value], &str) -> String,
    provider_id: &str,
    provider_kind: Option<ProviderKind>,
    app: AppKind,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> StreamingResponseUsageRecord {
    crate::proxy_core::api::usage::streaming_response_usage_record_with_optional_outbound_model(
        events,
        stream_parser,
        model_extractor,
        provider_id,
        provider_kind,
        app,
        request_model,
        outbound_model,
        latency_ms,
        first_token_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn non_streaming_response_usage_record_from_body_with_request_id_fallback(
    body: &[u8],
    response_parser: fn(&Value) -> Option<TokenUsage>,
    provider_id: &str,
    provider_kind: Option<ProviderKind>,
    app: AppKind,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> NonStreamingResponseUsageRecord {
    crate::proxy_core::api::usage::non_streaming_response_usage_record_from_body_with_request_id_fallback(
        body,
        response_parser,
        provider_id,
        provider_kind,
        app,
        request_model,
        outbound_model,
        latency_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

pub(crate) fn usage_record_pricing_model(
    record: &UsageRecord,
    pricing_model_source: &str,
) -> String {
    crate::proxy_core::api::usage::resolve_usage_record_pricing_models(record, pricing_model_source)
        .pricing_model
}

pub(crate) fn usage_route_context_from_selection(selection: &RouteSelection) -> UsageRouteContext {
    crate::proxy_core::api::usage::usage_route_context_from_selection(selection)
}

pub(crate) fn usage_record_with_route_context(
    record: UsageRecord,
    route: Option<&UsageRouteContext>,
) -> UsageRecord {
    crate::proxy_core::api::usage::usage_record_with_route_context(record, route)
}

pub(crate) fn usage_record_to_request_log(
    record: &UsageRecord,
    pricing_model_source: &str,
    pricing: Option<&ModelPricing>,
    multiplier: Decimal,
    fallback_request_id: impl FnOnce() -> String,
) -> UsageRequestLogProjection {
    let app_type = record.app.as_str().to_string();
    let model_selection =
        crate::proxy_core::api::usage::resolve_usage_record_pricing_models(
            record,
            pricing_model_source,
        );
    let usage = crate::proxy_core::api::usage::token_usage_from_usage_record(record);
    let missing_pricing_model = (pricing.is_none()
        && record.tokens.has_billable_tokens()
        && !is_placeholder_pricing_model(&model_selection.pricing_model))
    .then(|| model_selection.pricing_model.clone());
    let cost = CostCalculator::try_calculate_for_app(&app_type, &usage, pricing, multiplier);

    UsageRequestLogProjection {
        log: RequestLog {
            request_id: crate::proxy_core::api::usage::usage_record_request_id_with_fallback(
                record,
                fallback_request_id,
            ),
            provider_id: record.provider_id.clone(),
            app_type,
            model: model_selection.response_model,
            request_model: record.request_model.clone(),
            pricing_model: model_selection.pricing_model,
            usage,
            cost,
            latency_ms: record.latency_ms,
            first_token_ms: record.first_token_ms,
            status_code: record.status_code,
            error_message: record.error_message.clone(),
            session_id: record.session_id.clone(),
            provider_type: record
                .provider_kind
                .as_ref()
                .map(|provider_kind| provider_kind.as_str().to_string()),
            channel_id: record.channel_id.clone(),
            channel_name: record.channel_name.clone(),
            route_group: record.route_group.clone(),
            is_streaming: record.is_streaming,
            cost_multiplier: multiplier.to_string(),
        },
        missing_pricing_model,
    }
}

pub(crate) fn claude_takeover_client_model_for_upstream(
    takeover_model: &str,
    supports_one_m: bool,
    upstream_model: &str,
) -> String {
    crate::proxy_core::api::model_catalog::claude_takeover_client_model_for_upstream(
        takeover_model,
        supports_one_m,
        upstream_model,
    )
}

pub(crate) fn claude_takeover_default_display_name(upstream_model: &str) -> String {
    crate::proxy_core::api::model_catalog::claude_takeover_default_display_name(upstream_model)
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreModelRoute {
    fn to_proxy_core_model_route(&self) -> ModelRoute;
}

impl ToProxyCoreModelRoute for ProxyChannelModelRecord {
    fn to_proxy_core_model_route(&self) -> ModelRoute {
        model_route_from_input(self.to_proxy_core_model_route_input())
    }
}

impl ProxyChannelModelRecord {
    fn to_proxy_core_model_route_input(&self) -> ModelRouteInput {
        ModelRouteInput {
            public_model: self.public_model.clone(),
            upstream_model: self.upstream_model.clone(),
            capabilities: self.capabilities.clone(),
            pricing_model: self.pricing_model.clone(),
            request_overrides: self.request_overrides.clone(),
            response_overrides: self.response_overrides.clone(),
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord;
}

impl ToProxyCoreChannelModelRecord for ProxyChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord {
        channel_model_record_from_input(self.to_proxy_core_channel_model_record_input())
    }
}

impl ProxyChannelModelRecord {
    fn to_proxy_core_channel_model_record_input(&self) -> ChannelModelRecordInput {
        ChannelModelRecordInput {
            channel_id: self.channel_id.clone(),
            public_model: self.public_model.clone(),
            upstream_model: self.upstream_model.clone(),
            capabilities: self.capabilities.clone(),
            pricing_model: self.pricing_model.clone(),
            request_overrides: self.request_overrides.clone(),
            response_overrides: self.response_overrides.clone(),
        }
    }
}

pub(crate) fn proxy_channel_model_records_to_core(
    models: Vec<ProxyChannelModelRecord>,
) -> Vec<ChannelModelRecord> {
    models
        .iter()
        .map(ProxyChannelModelRecord::to_proxy_core_channel_model_record)
        .collect()
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelRecord {
    fn to_proxy_core_channel_record(&self) -> ChannelRecord;
}

impl ToProxyCoreChannelRecord for ProxyChannelRecord {
    fn to_proxy_core_channel_record(&self) -> ChannelRecord {
        channel_record_from_input(ChannelRecordInput {
            id: self.id.clone(),
            provider_id: self.provider_id.clone(),
            app_type: self.app_type.clone(),
            name: self.name.clone(),
            status: self.status.clone(),
            base_url: self.base_url.clone(),
            interface_kind: self.interface_kind.clone(),
            auth_profile_ref: self.auth_profile_ref.clone(),
            groups: self.groups.clone(),
            priority: self.priority,
            weight: self.weight,
            retry_policy: self.retry_policy.clone(),
            health_policy: self.health_policy.clone(),
            header_overrides: self.header_overrides.clone(),
            param_overrides: self.param_overrides.clone(),
            status_code_mapping: self.status_code_mapping.clone(),
            tags: self.tags.clone(),
            metadata: self.metadata.clone(),
            source_kind: self.source_kind.as_str().to_string(),
            source_endpoint_url: self.source_endpoint_url.clone(),
            models: self
                .models
                .iter()
                .map(ProxyChannelModelRecord::to_proxy_core_channel_model_record_input)
                .collect(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        })
    }
}

pub(crate) fn proxy_channel_record_to_core(channel: ProxyChannelRecord) -> ChannelRecord {
    channel.to_proxy_core_channel_record()
}

pub(crate) fn proxy_channel_records_to_core(
    channels: Vec<ProxyChannelRecord>,
) -> Vec<ChannelRecord> {
    channels
        .into_iter()
        .map(proxy_channel_record_to_core)
        .collect()
}

pub(crate) fn proxy_channel_key_record_to_core(key: ProxyChannelKeyRecord) -> ChannelKeyRecord {
    channel_key_record_from_input(ChannelKeyRecordInput {
        channel_id: key.channel_id,
        key_ref: key.key_ref,
        status: key.status,
        priority: key.priority,
        weight: key.weight,
        last_failure_at: key.last_failure_at,
    })
}

pub(crate) fn proxy_channel_key_records_to_core(
    keys: Vec<ProxyChannelKeyRecord>,
) -> Vec<ChannelKeyRecord> {
    keys.into_iter()
        .map(proxy_channel_key_record_to_core)
        .collect()
}

pub(crate) fn extract_proxy_session_id(
    headers: &HeaderMap,
    body: &Value,
    client_format: &str,
) -> SessionIdResult {
    crate::proxy_core::api::session::extract_session_id_with_generator(
        headers,
        body,
        client_format,
        || Uuid::new_v4().to_string(),
    )
}

pub(crate) fn stream_check_result_to_channel_reachability(
    result: StreamCheckResult,
) -> ChannelReachabilityResult {
    ChannelReachabilityResult::from_input(ChannelReachabilityInput {
        success: result.success,
        status: stream_check_health_status_to_channel_reachability(&result.status),
        message: result.message,
        latency_ms: result.response_time_ms,
        http_status: result.http_status,
        tested_at: result.tested_at,
        retry_count: result.retry_count,
    })
}

pub(crate) fn channel_reachability_status_from_latency(
    latency_ms: u64,
    degraded_threshold_ms: u64,
) -> ChannelReachabilityStatus {
    crate::proxy_core::api::management::channel_reachability_status_from_latency(
        latency_ms,
        degraded_threshold_ms,
    )
}

pub(crate) fn should_retry_channel_reachability_failure(message: &str) -> bool {
    crate::proxy_core::api::management::should_retry_channel_reachability_failure(message)
}

fn stream_check_health_status_to_channel_reachability(
    status: &HealthStatus,
) -> ChannelReachabilityStatus {
    match status {
        HealthStatus::Operational => ChannelReachabilityStatus::Operational,
        HealthStatus::Degraded => ChannelReachabilityStatus::Degraded,
        HealthStatus::Failed => ChannelReachabilityStatus::Failed,
    }
}

fn provider_metadata_without_secrets(provider: &Provider) -> ProviderMetadata {
    let meta = provider.meta.as_ref();
    provider_metadata_from_input(ProviderMetadataInput {
        website_url: provider.website_url.clone(),
        category: provider.category.clone(),
        sort_index: provider.sort_index,
        notes: provider.notes.clone(),
        icon: provider.icon.clone(),
        icon_color: provider.icon_color.clone(),
        in_failover_queue: provider.in_failover_queue,
        provider_type: meta.and_then(|meta| meta.provider_type.clone()),
        api_format: meta.and_then(|meta| meta.api_format.clone()),
        auth_binding: meta
            .and_then(|meta| meta.auth_binding.as_ref())
            .map(|binding| json!(binding)),
        endpoint_auto_select: meta.and_then(|meta| meta.endpoint_auto_select),
        custom_endpoint_count: meta
            .map(|meta| meta.custom_endpoints.len())
            .unwrap_or(0),
    })
}

fn account_ref(provider: &Provider) -> Option<String> {
    provider.meta.as_ref().and_then(|meta| {
        let provider_type = meta.provider_type.as_deref();
        let account_id =
            provider_type.and_then(|provider_type| meta.managed_account_id_for(provider_type));
        provider_account_ref(provider_type, account_id.as_deref())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::ProxyChannelSourceKind;
    use crate::provider::{AuthBinding, AuthBindingSource, ProviderMeta};
    use crate::proxy_core::api::errors::ProxyCoreError;
    use crate::proxy_core::api::session::SessionIdSource;
    use crate::proxy_core::api::transforms::GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX;
    use crate::proxy_core::api::transport::UpstreamTransportKind;

    #[test]
    fn app_type_conversion_preserves_known_and_custom_names() {
        assert_eq!(AppKind::from(&AppType::Claude), AppKind::Claude);
        assert_eq!(
            AppKind::from(&AppType::ClaudeDesktop),
            AppKind::ClaudeDesktop
        );
        assert_eq!(AppKind::from(&AppType::Codex), AppKind::Codex);
        assert_eq!(
            AppKind::from(&AppType::OpenClaw),
            AppKind::Custom("openclaw".to_string())
        );
    }

    #[test]
    fn claude_desktop_gateway_auth_adapter_projects_bearer_validation() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer gateway-token"),
        );

        validate_claude_desktop_gateway_bearer_header(&headers, "gateway-token")
            .expect("valid bearer");

        assert_eq!(
            validate_claude_desktop_gateway_bearer_header(&HeaderMap::new(), "gateway-token")
                .unwrap_err(),
            ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader
        );
        assert_eq!(
            validate_claude_desktop_gateway_bearer_header(&headers, "wrong-token").unwrap_err(),
            ClaudeDesktopGatewayAuthError::InvalidToken
        );
    }

    #[test]
    fn proxy_error_status_adapter_projects_http_contract() {
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::ForwardFailed),
            502
        );
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::AuthError),
            401
        );
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::UpstreamError(42)),
            502
        );
    }

    #[test]
    fn global_proxy_adapter_projects_masking_and_loopback_policy() {
        assert_eq!(
            SYSTEM_PROXY_ENV_KEYS,
            [
                "HTTP_PROXY",
                "http_proxy",
                "HTTPS_PROXY",
                "https_proxy",
                "ALL_PROXY",
                "all_proxy"
            ]
        );
        assert_eq!(
            mask_url_for_log("http://user:pass@127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert!(proxy_url_points_to_loopback_port(
            "socks5://localhost:15721",
            15721
        ));
        assert!(!proxy_url_points_to_loopback_port(
            "http://127.0.0.1:7890",
            15721
        ));
        assert!(proxy_values_point_to_loopback_port(
            ["", " http://127.0.0.1:15721 "],
            15721
        ));
    }

    #[test]
    fn fetched_model_adapter_preserves_frontend_contract() {
        let value = serde_json::to_value(FetchedModel {
            id: "gpt-5.4".to_string(),
            owned_by: Some("openai".to_string()),
        })
        .expect("serialize fetched model");

        assert_eq!(
            value,
            json!({
                "id": "gpt-5.4",
                "ownedBy": "openai"
            })
        );
    }

    #[test]
    fn model_fetch_transport_adapter_projects_core_planning_and_parsing() {
        struct StaticTransport;

        impl OpenAiCompatibleModelsTransport for StaticTransport {
            fn send_openai_compatible_models_request<'a>(
                &'a self,
                request: OpenAiCompatibleModelsRequest<'a>,
            ) -> futures::future::BoxFuture<'a, Result<ModelFetchHttpResponse, String>> {
                Box::pin(async move {
                    assert_eq!(request.url, "https://api.example.com/v1/models");
                    assert_eq!(
                        request.authorization_header.1,
                        "Bearer provider-token"
                    );
                    Ok(ModelFetchHttpResponse {
                        status: http::StatusCode::OK,
                        body: br#"{"data":[{"id":"model-a","owned_by":"vendor-a"}]}"#.to_vec(),
                    })
                })
            }
        }

        impl CodexOAuthModelsTransport for StaticTransport {
            fn send_codex_oauth_models_request<'a>(
                &'a self,
                request: CodexOAuthModelsRequest<'a>,
            ) -> futures::future::BoxFuture<'a, Result<ModelFetchHttpResponse, String>> {
                Box::pin(async move {
                    assert_eq!(request.account_id_header.1, "account-a");
                    assert_eq!(request.authorization_header.1, "Bearer oauth-token");
                    Ok(ModelFetchHttpResponse {
                        status: http::StatusCode::OK,
                        body: br#"{"data":[{"model":"codex-mini","display_name":"Codex Mini"}]}"#
                            .to_vec(),
                    })
                })
            }
        }

        let transport = StaticTransport;
        let openai_models = futures::executor::block_on(
            fetch_openai_compatible_models_with_transport(
                "https://api.example.com",
                "provider-token",
                false,
                None,
                None,
                &transport,
            ),
        )
        .expect("openai-compatible models");
        assert_eq!(openai_models[0].id, "model-a");
        assert_eq!(openai_models[0].owned_by.as_deref(), Some("vendor-a"));

        let codex_models = futures::executor::block_on(
            fetch_codex_oauth_models_with_transport(
                "oauth-token",
                "account-a",
                "3.16.3",
                &transport,
            ),
        )
        .expect("codex oauth models");
        assert_eq!(codex_models[0].id, "codex-mini");
        assert_eq!(codex_models[0].owned_by.as_deref(), Some("Codex"));
    }

    #[test]
    fn settings_config_adapter_preserves_frontend_contracts() {
        assert_eq!(
            serde_json::to_value(RectifierConfig::default()).expect("rectifier"),
            json!({
                "enabled": true,
                "requestThinkingSignature": true,
                "requestThinkingBudget": true,
                "requestMediaFallback": true,
                "requestMediaHeuristic": true
            })
        );
        assert_eq!(
            serde_json::to_value(OptimizerConfig::default()).expect("optimizer"),
            json!({
                "enabled": false,
                "thinkingOptimizer": true,
                "cacheInjection": true,
                "cacheTtl": "1h"
            })
        );
        assert_eq!(
            CopilotOptimizerConfig::default().warmup_model,
            "gpt-5-mini"
        );
    }

    #[test]
    fn proxy_config_adapter_preserves_management_contracts() {
        let proxy_config = serde_json::to_value(ProxyConfig::default()).expect("proxy config");
        assert_eq!(
            proxy_config.get("listen_address").and_then(Value::as_str),
            Some("127.0.0.1")
        );
        assert_eq!(
            proxy_config
                .get("streaming_first_byte_timeout")
                .and_then(Value::as_u64),
            Some(60)
        );

        let app_config = AppProxyConfig {
            app_type: "claude".to_string(),
            enabled: true,
            auto_failover_enabled: true,
            max_retries: 3,
            streaming_first_byte_timeout: 60,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
            circuit_failure_threshold: 4,
            circuit_success_threshold: 2,
            circuit_timeout_seconds: 60,
            circuit_error_rate_threshold: 0.6,
            circuit_min_requests: 10,
        };

        assert_eq!(CircuitBreakerConfig::from(&app_config), CircuitBreakerConfig::default());
        assert_eq!(
            serde_json::to_value(GlobalProxyConfig {
                proxy_enabled: true,
                listen_address: "127.0.0.1".to_string(),
                listen_port: 15721,
                enable_logging: true,
            })
            .expect("global proxy config")
            .get("proxyEnabled")
            .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn circuit_and_route_adapter_projects_provider_router_contracts() {
        assert_eq!(circuit_breaker_log_codes::OPEN_TO_HALF_OPEN, "CB-001");
        assert_eq!(
            circuit_breaker_log_codes::HALF_OPEN_TO_CLOSED,
            "CB-002"
        );
        assert_eq!(provider_circuit_key("claude", "provider-a"), "claude:provider-a");
        assert_eq!(
            channel_circuit_key("claude", "channel-a"),
            "channel:claude:channel-a"
        );
        assert_eq!(
            app_type_from_circuit_key("channel:claude:channel-a"),
            "claude"
        );
        assert_eq!(CircuitState::HalfOpen.to_string(), "half_open");

        let allow_result = AllowResult {
            allowed: true,
            used_half_open_permit: true,
        };
        assert!(allow_result.allowed);
        assert!(allow_result.used_half_open_permit);

        let stats = CircuitBreakerStats {
            state: CircuitState::Open,
            consecutive_failures: 4,
            consecutive_successes: 0,
            total_requests: 10,
            failed_requests: 6,
        };
        assert_eq!(
            serde_json::to_value(stats).expect("serialize circuit stats"),
            json!({
                "state": "open",
                "consecutiveFailures": 4,
                "consecutiveSuccesses": 0,
                "totalRequests": 10,
                "failedRequests": 6
            })
        );

        let selected = select_provider_ids(ProviderSelectionInput::failover(vec![
            ProviderSelectionCandidate::new("missing", false, true),
            ProviderSelectionCandidate::new("provider-b", true, true),
            ProviderSelectionCandidate::new("provider-a", true, false),
        ]))
        .expect("selected provider ids");
        assert_eq!(selected, vec!["provider-b"]);

        let mut response = resolve_channel_route(
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
            vec![RouteResolveChannelInput {
                channel_id: "channel-a".to_string(),
                provider_id: "provider-a".to_string(),
                channel_name: "Provider A".to_string(),
                status: "enabled".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                groups: vec!["default".to_string()],
                models: vec![RouteResolveModelInput {
                    public_model: "claude-sonnet-4".to_string(),
                    upstream_model: "upstream-sonnet".to_string(),
                }],
                priority: 100,
                weight: 1,
                source_kind: "legacy_provider".to_string(),
            }],
            ChannelRouteSource::MaterializedChannels,
        )
        .expect("route response");
        assert_eq!(response.candidates.len(), 1);

        reject_unavailable_channel_ids(&mut response, ["channel-a"]);
        assert!(response.candidates.is_empty());
        assert_eq!(response.rejected.len(), 1);
        assert_eq!(response.rejected[0].reasons, vec!["circuit_open"]);
    }

    #[test]
    fn proxy_event_adapter_projects_event_stream_contracts() {
        assert_eq!(PROXY_EVENTS_CONNECTED_EVENT, "proxy_events_connected");
        assert_eq!(PROXY_EVENTS_LAGGED_EVENT, "proxy_events_lagged");
        assert_eq!(build_proxy_events_connected_payload(256)["bufferSize"], 256);
        assert_eq!(build_proxy_events_lagged_payload(3)["skipped"], 3);

        let envelope = ProxyEventEnvelope::new(
            42,
            "request_started",
            "2026-06-20T00:00:00Z",
            json!({"provider": "relay-a"}),
        );
        let spec = proxy_event_envelope_to_sse_spec(&envelope);

        assert_eq!(spec.id, "42");
        assert_eq!(spec.event, "request_started");
        assert!(spec.data.contains("\"provider\":\"relay-a\""));
    }

    #[test]
    fn codex_chat_history_adapter_projects_sse_and_state_helpers() {
        let mut buffer = String::new();
        let mut remainder = Vec::new();
        append_utf8_safe(
            &mut buffer,
            &mut remainder,
            br#"data: {"type":"response.output_item.done","response":{"id":"resp_1"},"item":{"type":"function_call","call_id":"call_1","name":"read_file","arguments":"{}"}}"#,
        );
        append_utf8_safe(&mut buffer, &mut remainder, b"\n\n");

        let block = take_sse_block(&mut buffer).expect("sse block");
        let inspection = inspect_codex_chat_history_sse_block(&block).expect("inspection");
        assert_eq!(inspection.response_id.as_deref(), Some("resp_1"));
        match inspection.record {
            Some(CodexChatHistorySseRecord::OutputItemDone { item }) => {
                assert_eq!(item["call_id"], "call_1");
            }
            other => panic!("unexpected inspection record: {other:?}"),
        }

        let mut state = CodexChatHistoryState::default();
        assert_eq!(
            state.record_response(&json!({
                "id": "resp_1",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{}",
                    "reasoning_content": "Need context."
                }]
            })),
            1
        );
        let mut request = json!({
            "previous_response_id": "resp_1",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "ok"
            }]
        });

        assert_eq!(state.enrich_request(&mut request), 1);
        assert_eq!(request["input"][0]["type"], "function_call");
        assert_eq!(request["input"][0]["reasoning_content"], "Need context.");
    }

    #[test]
    fn codex_provider_adapter_projects_chat_policy_headers_and_reasoning() {
        assert!(resolve_codex_provider_uses_chat_completions(
            Some("openai_chat"),
            None,
            None,
            None
        ));
        assert!(should_convert_codex_responses_endpoint_to_chat(
            true,
            "/v1/responses"
        ));
        assert_eq!(
            build_codex_upstream_url("https://api.openai.com", "/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );

        let headers = build_codex_bearer_auth_headers("sk-test").expect("bearer header");
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(
            headers[0].1,
            http::HeaderValue::from_static("Bearer sk-test")
        );

        assert_eq!(
            resolve_codex_provider_upstream_model(Some(" upstream-model "), None).as_deref(),
            Some("upstream-model")
        );
        let catalog_model_ids = codex_provider_catalog_model_ids_from_settings(&json!({
            "modelCatalog": {
                "models": [{"model": "catalog-model"}]
            }
        }));
        assert!(catalog_model_ids.contains("catalog-model"));
        let mut body = json!({"model": "client-model"});
        assert_eq!(
            apply_codex_chat_upstream_model_policy(
                &mut body,
                true,
                Some("upstream-model"),
                &catalog_model_ids,
            )
            .as_deref(),
            Some("upstream-model")
        );
        assert_eq!(body["model"], "upstream-model");

        let profile = normalize_codex_chat_reasoning_profile(CodexChatReasoningProfile {
            supports_effort: Some(true),
            effort_param: Some("reasoning_effort".to_string()),
            ..CodexChatReasoningProfile::default()
        });
        assert_eq!(profile.supports_thinking, Some(true));
        let options = CodexChatReasoningOptions::from_profile(&profile);
        assert_eq!(options.supports_effort, Some(true));
        assert_eq!(
            infer_codex_chat_reasoning_profile(
                "DeepSeek Relay",
                "https://api.deepseek.com",
                "deepseek-chat",
            )
            .expect("deepseek reasoning profile")
            .supports_effort,
            Some(true)
        );
    }

    #[test]
    fn proxy_response_adapter_projects_transport_body_contracts() {
        let response = ProxyCoreResponse::with_body(
            http::StatusCode::CREATED,
            HeaderMap::new(),
            ProxyResponseBody::json(json!({"ok": true})),
        );
        let transport = response
            .into_transport_response()
            .expect("transport response");

        assert_eq!(transport.status, http::StatusCode::CREATED);
        match transport.body {
            ProxyTransportResponseBody::Bytes(body) => {
                assert_eq!(body.as_ref(), br#"{"ok":true}"#);
            }
            _ => panic!("expected buffered bytes transport body"),
        }
    }

    #[test]
    fn handler_context_adapter_projects_runtime_and_model_helpers() {
        let disabled_policy = resolve_response_runtime_policy(false, 3, 600, 60, 120);
        assert_eq!(disabled_policy.max_retries, 0);
        assert_eq!(disabled_policy.timeout, ResponseTimeoutConfig::default());

        let enabled_policy = resolve_response_runtime_policy(true, 3, 600, 60, 120);
        assert_eq!(enabled_policy.max_retries, 3);
        assert_eq!(enabled_policy.timeout.non_streaming_timeout, 600);
        assert_eq!(enabled_policy.timeout.streaming.first_byte_timeout, 60);
        assert_eq!(enabled_policy.timeout.streaming.idle_timeout, 120);

        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro:generateContent")
                .as_deref(),
            Some("gemini-pro")
        );
        assert_eq!(
            claude_api_format_from_metadata(
                &json!({"claudeApiFormat": "openai_chat"}),
                "anthropic"
            ),
            "openai_chat"
        );
        assert_eq!(
            claude_api_format_from_metadata(&json!({"apiFormat": " "}), "anthropic"),
            "anthropic"
        );
    }

    #[test]
    fn gemini_provider_adapter_projects_auth_settings_and_url_helpers() {
        let settings = json!({
            "env": {
                "GEMINI_API_KEY": " ya29.access-token ",
                "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com/v1beta/"
            }
        });
        assert_eq!(
            extract_gemini_api_key_from_settings(&settings).as_deref(),
            Some("ya29.access-token")
        );
        assert_eq!(
            extract_gemini_base_url_from_settings(&settings).as_deref(),
            Some("https://generativelanguage.googleapis.com/v1beta")
        );

        let creds = parse_gemini_oauth_credentials("ya29.access-token")
            .expect("direct oauth token should parse");
        assert_eq!(creds.access_token, "ya29.access-token");
        assert!(!creds.needs_refresh());
        assert_eq!(
            build_gemini_upstream_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "/v1beta/models/gemini-pro:generateContent",
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent"
        );

        let oauth_headers = build_gemini_auth_headers(
            "refresh-token",
            Some("ya29.access-token"),
            true,
        )
        .expect("oauth headers");
        assert_eq!(oauth_headers[0].0.as_str(), "authorization");
        assert_eq!(
            oauth_headers[0].1,
            http::HeaderValue::from_static("Bearer ya29.access-token")
        );
        let api_key_headers =
            build_gemini_auth_headers("AIza-api-key", None, false).expect("api key headers");
        assert_eq!(api_key_headers[0].0.as_str(), "x-goog-api-key");
        assert_eq!(
            api_key_headers[0].1,
            http::HeaderValue::from_static("AIza-api-key")
        );
    }

    #[test]
    fn claude_provider_adapter_projects_config_auth_url_and_cache_helpers() {
        let settings = json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": " claude-token ",
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com/v1/"
            }
        });
        assert_eq!(
            resolve_claude_api_format_from_settings(None, Some("openai_chat"), &settings),
            "openai_chat"
        );
        let auth_key = extract_claude_auth_key_from_settings(&settings)
            .expect("anthropic auth token");
        assert_eq!(auth_key.key, "claude-token");
        assert_eq!(auth_key.source, ClaudeAuthKeySource::AnthropicAuthToken);
        assert_eq!(
            extract_claude_base_url_from_settings(false, &settings).as_deref(),
            Some("https://api.anthropic.com/v1")
        );
        assert_eq!(
            build_claude_upstream_url("https://api.anthropic.com/v1", "/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );

        let bearer_headers =
            build_claude_auth_headers(ClaudeAuthHeaderKind::Bearer, "claude-token", None)
                .expect("bearer headers");
        assert_eq!(bearer_headers[0].0.as_str(), "authorization");
        assert_eq!(
            bearer_headers[0].1,
            http::HeaderValue::from_static("Bearer claude-token")
        );
        let copilot_headers = build_copilot_auth_headers(CopilotAuthHeadersInput {
            api_key: "copilot-token",
            request_id: "request-1",
            editor_version: "vscode/1",
            editor_plugin_version: "plugin/1",
            integration_id: "integration-1",
            user_agent: "copilot-test",
            github_api_version: "2022-11-28",
        })
        .expect("copilot headers");
        assert!(copilot_headers
            .iter()
            .any(|(name, value)| name.as_str() == "x-request-id" && value == "request-1"));

        assert!(is_copilot_prompt_cache_provider(
            Some("github_copilot"),
            &json!({})
        ));
        let cache_key = resolve_claude_responses_prompt_cache_key(
            &json!({"metadata": {"session_id": "session-1"}}),
            None,
            Some("fallback-session"),
            true,
        );
        assert_eq!(cache_key.key.as_deref(), Some("session-1"));
        assert_eq!(cache_key.source.as_str(), "session");
    }

    #[test]
    fn claude_transform_adapter_projects_request_and_response_facades() {
        let anthropic_body = json!({
            "model": "claude-sonnet",
            "max_tokens": 128,
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let chat_request = anthropic_to_openai_chat_request(&anthropic_body, false);
        assert_eq!(chat_request["model"], "claude-sonnet");
        assert_eq!(chat_request["messages"][0]["role"], "user");

        let responses_request =
            anthropic_to_openai_responses_request(&anthropic_body, Some("cache-1"), false, false);
        assert_eq!(responses_request["model"], "claude-sonnet");
        assert_eq!(responses_request["prompt_cache_key"], "cache-1");

        let gemini_request = anthropic_request_to_gemini_request_with_shadow(
            &anthropic_body,
            None,
            Some("provider-a"),
            Some("session-a"),
        )
        .expect("gemini request");
        assert_eq!(gemini_request["contents"][0]["role"], "user");

        let chat_response = openai_chat_to_anthropic_message(&json!({
            "id": "chatcmpl_1",
            "model": "chat-model",
            "choices": [{
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
        }))
        .expect("chat response");
        assert_eq!(chat_response["content"][0]["text"], "Hi");

        let responses_response = openai_responses_to_anthropic_message(&json!({
            "id": "resp_1",
            "model": "responses-model",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "Done"}]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 2}
        }))
        .expect("responses response");
        assert_eq!(responses_response["content"][0]["text"], "Done");

        let gemini_output = gemini_response_to_anthropic_message(
            &json!({
                "responseId": "gemini_1",
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{"text": "Gemini hi"}]
                    },
                    "finishReason": "STOP"
                }],
                "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 2}
            }),
            None,
            || "toolu_test".to_string(),
        )
        .expect("gemini response");
        assert_eq!(gemini_output.response["content"][0]["text"], "Gemini hi");

        assert!(should_preserve_reasoning_content_for_openai_chat(
            &json!({}),
            &json!({"model": "deepseek-v4-pro"})
        ));
    }

    #[test]
    fn proxy_server_adapter_projects_runtime_contracts() {
        assert_eq!(server_log_codes::STARTED, "SRV-001");
        assert_eq!(server_log_codes::STOPPED, "SRV-002");
        assert_eq!(server_log_codes::ACCEPT_ERR, "SRV-005");
        let _shadow_store = GeminiShadowStore::default();

        let target = CurrentRouteTarget {
            app_type: "claude".to_string(),
            provider_id: "provider-a".to_string(),
            provider_name: "Provider A".to_string(),
            channel_id: Some("channel-a".to_string()),
            channel_name: Some("Channel A".to_string()),
            interface_kind: Some("anthropic_messages".to_string()),
            public_model: Some("sonnet-public".to_string()),
            upstream_model: Some("upstream-sonnet".to_string()),
        };
        assert_eq!(
            serde_json::to_value(target).expect("serialize target"),
            json!({
                "appType": "claude",
                "providerId": "provider-a",
                "providerName": "Provider A",
                "channelId": "channel-a",
                "channelName": "Channel A",
                "interfaceKind": "anthropic_messages",
                "publicModel": "sonnet-public",
                "upstreamModel": "upstream-sonnet"
            })
        );
    }

    #[test]
    fn provider_auth_adapter_projects_strategy_contracts() {
        let bearer =
            ProviderAuthInfo::new("provider-token".to_string(), ProviderAuthStrategy::Bearer);
        assert_eq!(bearer.strategy, ProviderAuthStrategy::Bearer);
        assert_eq!(bearer.masked_key(), "prov...oken");
        assert!(bearer.access_token.is_none());

        let oauth = ProviderAuthInfo::with_access_token(
            "refresh-token".to_string(),
            "ya29.access-token-12345".to_string(),
        );
        assert_eq!(oauth.strategy, ProviderAuthStrategy::GoogleOAuth);
        assert_eq!(
            oauth.masked_access_token(),
            Some("ya29...2345".to_string())
        );
    }

    #[test]
    fn provider_kind_adapter_projects_inference_helpers() {
        assert_eq!(
            infer_claude_provider_kind("gemini_native", true, None, None, &json!({})),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            infer_claude_provider_kind(
                "anthropic",
                false,
                Some("github_copilot"),
                Some("https://example.com"),
                &json!({})
            ),
            ProviderKind::GitHubCopilot
        );
        assert!(is_gemini_oauth_key_shape(" ya29.access-token "));
        assert!(is_gemini_oauth_key_shape(
            r#"{"access_token":"ya29.access-token"}"#
        ));
        assert!(!is_gemini_oauth_key_shape("AIza-api-key"));
    }

    #[test]
    fn proxy_channel_dao_adapter_projects_validation_and_legacy_projection() {
        assert_eq!(
            normalize_channel_base_url(" https://api.example.com/v1/ "),
            "https://api.example.com/v1"
        );
        assert_eq!(
            normalize_channel_groups(vec![
                "default".to_string(),
                " ".to_string(),
                "beta".to_string(),
                "default".to_string()
            ]),
            vec!["beta".to_string(), "default".to_string()]
        );
        assert_eq!(
            stable_channel_id("Claude", "Provider A", "legacy_primary", "https://api.example.com"),
            "legacy-claude-provider-a-legacy-primary-fcc8db014bbc"
        );

        let request = ProxyChannelWriteRequest {
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Provider A primary".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            models: vec![ProxyChannelModelWriteRequest {
                public_model: "claude-sonnet-4-6".to_string(),
                upstream_model: "anthropic/claude-sonnet-4-6".to_string(),
                ..ProxyChannelModelWriteRequest::default()
            }],
            ..ProxyChannelWriteRequest::default()
        };
        validate_proxy_channel_write_request_fields(&request).expect("valid channel request");

        let provider_projection = LegacyProviderProjectionInput {
            env: std::collections::BTreeMap::from([(
                "ANTHROPIC_MODEL".to_string(),
                "claude-sonnet-4-6".to_string(),
            )]),
            ..LegacyProviderProjectionInput::default()
        };
        let interface =
            infer_legacy_channel_interface(Some(&ProxyCoreAppKind::Claude), &provider_projection);
        assert_eq!(interface, ProxyCoreInterfaceKind::AnthropicMessages);

        let projection = build_legacy_channel_projection(LegacyChannelProjectionInput {
            app_type: "claude".to_string(),
            app: Some(ProxyCoreAppKind::Claude),
            provider_id: "provider-a".to_string(),
            provider_name: "Provider A".to_string(),
            provider_sort_index: Some(1),
            provider_in_failover_queue: false,
            base_url: "https://api.example.com/v1".to_string(),
            interface_kind: interface,
            priority: legacy_channel_priority("provider-a", false, Some("provider-a")),
            source_kind: "legacy_primary".to_string(),
            source_endpoint_url: None,
            provider_projection,
        });

        assert_eq!(projection.priority, 100);
        assert_eq!(projection.interface_kind, "anthropic_messages");
        assert_eq!(projection.models.len(), 1);
        assert!(!projection.needs_review);
    }

    #[test]
    fn copilot_account_adapter_projects_domain_and_composite_id_rules() {
        assert_eq!(COPILOT_PUBLIC_GITHUB_DOMAIN, "github.com");
        assert_eq!(default_copilot_github_domain(), "github.com");
        assert_eq!(
            normalize_github_domain("https://Company.GHE.Com/api/v3?foo=bar").unwrap(),
            "company.ghe.com"
        );
        assert!(!is_copilot_ghes_domain("github.com"));
        assert!(is_copilot_ghes_domain("company.ghe.com"));
        assert_eq!(copilot_composite_account_id("github.com", 12345), "12345");
        assert_eq!(
            copilot_composite_account_id("company.ghe.com", 12345),
            "company.ghe.com:12345"
        );
    }

    #[test]
    fn copilot_transport_adapter_projects_urls_and_model_parsing() {
        assert_eq!(
            copilot_github_client_id("github.com"),
            "Iv1.b507a08c87ecfe98"
        );
        assert_eq!(
            copilot_github_client_id("company.ghe.com"),
            "Ov23li8tweQw6odWQebz"
        );
        assert_eq!(
            copilot_github_device_code_url("company.ghe.com"),
            "https://company.ghe.com/login/device/code"
        );
        assert_eq!(
            copilot_github_oauth_token_url("company.ghe.com"),
            "https://company.ghe.com/login/oauth/access_token"
        );
        assert_eq!(
            copilot_github_user_url("github.com"),
            "https://api.github.com/user"
        );
        assert_eq!(
            copilot_token_url("company.ghe.com"),
            "https://company.ghe.com/api/v3/copilot_internal/v2/token"
        );
        assert_eq!(
            copilot_usage_url("company.ghe.com"),
            "https://company.ghe.com/api/v3/copilot_internal/user"
        );
        assert_eq!(
            copilot_api_base("company.ghe.com"),
            "https://copilot-api.company.ghe.com"
        );

        let models = parse_copilot_models_response_bytes(
            br#"{
            "data": [
                {
                    "id": "gpt-5.4",
                    "name": "GPT-5.4",
                    "vendor": "OpenAI",
                    "model_picker_enabled": true
                },
                {
                    "id": "hidden",
                    "name": "Hidden",
                    "vendor": "GitHub",
                    "model_picker_enabled": false
                }
            ]
        }"#,
        )
        .unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-5.4");
        assert_eq!(models[0].vendor, "OpenAI");
    }

    #[test]
    fn claude_desktop_model_routes_to_core_response_preserves_route_contract() {
        let response =
            claude_desktop_model_routes_to_core_response([ResolvedModelRoute {
                route_id: "claude-sonnet-4-6".to_string(),
                upstream_model: "anthropic/claude-sonnet-4-6".to_string(),
                label_override: None,
                supports_1m: true,
            }]);

        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].id, "claude-sonnet-4-6");
        assert!(response.data[0].supports_1m);
        assert_eq!(response.first_id.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(response.last_id.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn codex_catalog_adapter_builds_and_simplifies_model_catalog() {
        let settings = json!({
            "modelCatalog": {
                "models": [
                    {
                        "model": "kimi-k2",
                        "displayName": "Kimi K2",
                        "contextWindow": "64000"
                    }
                ]
            }
        });
        let template = json!({
            "slug": "gpt-5.5",
            "display_name": "GPT-5.5",
            "context_window": 272000,
            "model_messages": {"base": "template"}
        });

        assert!(codex_settings_have_model_catalog_specs(&settings));
        assert_eq!(codex_default_model_context_window(), 128_000);

        let catalog = codex_model_catalog_from_settings(&settings, 128_000, &template)
            .expect("catalog");
        let models = catalog
            .get("models")
            .and_then(Value::as_array)
            .expect("models");
        assert_eq!(models[0].get("slug").and_then(Value::as_str), Some("kimi-k2"));
        assert_eq!(
            models[0].get("context_window").and_then(Value::as_u64),
            Some(64_000)
        );

        let simplified = simplify_codex_model_catalog(&catalog.to_string(), 128_000)
            .expect("simplified catalog");
        assert_eq!(
            simplified["models"][0].get("model").and_then(Value::as_str),
            Some("kimi-k2")
        );
        assert_eq!(
            simplified["models"][0]
                .get("displayName")
                .and_then(Value::as_str),
            Some("Kimi K2")
        );
    }

    #[test]
    fn model_catalog_adapter_projects_provider_settings_and_client_raw() {
        let settings = json!({
            "model": " claude-sonnet-4 ",
            "env": {
                "ANTHROPIC_MODEL": "claude-opus-4"
            },
            "modelCatalog": {
                "models": [
                    {"model": "deepseek-v4"},
                    {"id": "kimi-k2"}
                ]
            }
        });

        let provider_catalog =
            provider_model_catalog_from_settings("provider-a", Some(&settings));
        assert_eq!(provider_catalog.provider_id, "provider-a");
        assert_eq!(
            provider_catalog.models,
            vec![
                "claude-opus-4".to_string(),
                "claude-sonnet-4".to_string(),
                "deepseek-v4".to_string(),
                "kimi-k2".to_string()
            ]
        );

        let client_catalog = client_model_catalog_from_raw(
            &AppKind::Codex,
            json!({
                "models": [
                    {"id": " gpt-5 "},
                    {"model": "o4-mini"},
                    {"id": "gpt-5"}
                ]
            }),
        );
        assert_eq!(client_catalog.provider_id, "codex");
        assert_eq!(
            client_catalog.models,
            vec!["gpt-5".to_string(), "o4-mini".to_string()]
        );
    }

    #[test]
    fn route_plan_adapter_projects_provider_ids_and_forward_selection() {
        fn selection(channel_id: &str, provider_id: &str) -> RouteSelection {
            let provider = ProviderSpec {
                id: provider_id.to_string(),
                name: provider_id.to_string(),
                kind: ProviderKind::Claude,
                account_ref: None,
                metadata: ProviderMetadata::default(),
            };
            let channel = ChannelSpec {
                id: channel_id.to_string(),
                provider_id: provider_id.to_string(),
                app: AppKind::Claude,
                name: channel_id.to_string(),
                status: ChannelStatus::Enabled,
                endpoint: UpstreamEndpoint {
                    base_url: "https://api.example.com".to_string(),
                    path_template: None,
                    api_version: None,
                    timeout_profile: None,
                },
                interface: InterfaceKind::OpenAiChatCompletions,
                auth_profile: None,
                models: Vec::new(),
                groups: vec!["default".to_string()],
                priority: 0,
                weight: 100,
                retry_policy: RetryPolicy {
                    raw: Value::Object(Default::default()),
                },
                health_policy: ChannelHealthPolicy {
                    raw: Value::Object(Default::default()),
                },
                overrides: ChannelOverrides {
                    headers: Value::Object(Default::default()),
                    params: Value::Object(Default::default()),
                    status_code_mapping: Value::Array(Vec::new()),
                    model_mapping: Value::Object(Default::default()),
                },
                tags: Vec::new(),
                metadata: Value::Object(Default::default()),
                source_ref: None,
                needs_review: false,
                review_reasons: Vec::new(),
            };

            crate::proxy_core::api::routing::route_selection_from_parts(
                provider,
                channel,
                None,
                InterfaceKind::OpenAiChatCompletions,
            )
        }

        let primary = selection("ch-a", "provider-a");
        let plan = RoutePlan {
            selection: primary.clone(),
            selections: vec![
                primary,
                selection("ch-b", "provider-b"),
                selection("ch-c", "provider-a"),
            ],
            attempts: Vec::new(),
        };

        assert_eq!(
            route_plan_provider_ids(&plan),
            vec!["provider-a".to_string(), "provider-b".to_string()]
        );
        assert_eq!(
            route_selection_for_forward_result(&plan, Some("ch-b"), "provider-a").channel.id,
            "ch-b"
        );
        assert_eq!(
            route_selection_for_forward_result(&plan, Some("ch-b"), "provider-a")
                .outbound_interface,
            InterfaceKind::OpenAiChatCompletions
        );
        let selected = route_selection_for_forward_result(&plan, Some("ch-b"), "provider-a");
        let candidate = channel_route_candidate_from_selection(&selected);
        assert_eq!(candidate.channel_id, "ch-b");
        assert_eq!(candidate.route_group, "default");
        assert_eq!(candidate.source_kind, "proxy_core");
        let resolved = resolved_channel_attempt_from_candidate(candidate);
        assert_eq!(resolved.channel_id, "ch-b");
        assert!(proxy_core_error_is_unavailable(
            &ProxyCoreError::Unavailable("missing provider".to_string())
        ));
        assert!(!proxy_core_error_is_unavailable(&ProxyCoreError::Config(
            "invalid route".to_string()
        )));

        let mut body = json!({"model": "sonnet-public"});
        assert_eq!(
            apply_channel_route_model_override(
                &mut body,
                Some("sonnet-public"),
                Some("upstream-sonnet")
            )
            .as_deref(),
            Some("upstream-sonnet")
        );
        assert_eq!(body.get("model").and_then(Value::as_str), Some("upstream-sonnet"));
    }

    #[test]
    fn error_mapper_adapter_projects_error_contracts() {
        assert_eq!(
            codex_proxy_error_code(CodexProxyErrorKind::ForwardFailed),
            "cc_switch_forward_failed"
        );
        let json_body = codex_proxy_error_json(CodexProxyErrorContext {
            provider_name: "Relay",
            request_model: "model-a",
            endpoint: "/responses",
            fallback_message: "failed",
            fallback_code: "cc_switch_forward_failed",
            upstream_status: None,
            upstream_body: None,
        });
        assert_eq!(json_body["error"]["provider"], "Relay");

        let response = codex_proxy_error_response(
            ProxyErrorStatusKind::AuthError,
            CodexProxyErrorContext {
                provider_name: "Relay",
                request_model: "model-a",
                endpoint: "/responses",
                fallback_message: "bad token",
                fallback_code: "cc_switch_auth_error",
                upstream_status: None,
                upstream_body: None,
            },
        )
        .expect("codex error response");
        assert_eq!(response.status.as_u16(), 401);

        let failure = ForwardFailureKind::Timeout("slow".to_string());
        assert!(matches!(failure, ForwardFailureKind::Timeout(message) if message == "slow"));
        assert_eq!(
            ManagementAuthError::MissingBearerToken.message(),
            "Missing management bearer token"
        );
    }

    #[test]
    fn codex_handler_adapter_projects_tool_context_and_chat_error() {
        let context = codex_tool_context_from_request(&json!({
            "tools": [
                {
                    "type": "custom",
                    "name": "apply_patch"
                }
            ]
        }));

        assert_eq!(context.chat_tools().len(), 1);
        assert!(context.is_custom_tool_chat_name("apply_patch"));

        let normalized = normalize_codex_chat_error_body(b"Unauthorized");
        assert!(normalized.non_json_body_log_message().is_some());
        assert!(normalized.response_error.get("error").is_some());
    }

    #[test]
    fn claude_body_normalization_adapter_projects_stream_and_thinking_rules() {
        let mut stream_body = json!({"stream": true});
        inject_openai_stream_include_usage(&mut stream_body);
        assert_eq!(stream_body["stream_options"]["include_usage"], true);

        let settings = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic"
            }
        });
        let mut disabled_body = json!({
            "model": "deepseek-v4-pro",
            "thinking": {"type": "disabled"},
            "output_config": {"effort": "high", "temperature": 0.2},
            "reasoning_effort": "high"
        });

        assert!(normalize_deepseek_thinking_disabled_strip_effort(
            &mut disabled_body,
            &settings
        ));
        assert!(disabled_body.get("reasoning_effort").is_none());
        assert!(disabled_body["output_config"].get("effort").is_none());
        assert_eq!(disabled_body["output_config"]["temperature"], json!(0.2));

        let mut tool_body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {}}
                ]
            }]
        });

        assert!(should_normalize_anthropic_tool_thinking_history(
            &settings,
            &tool_body,
            "anthropic"
        ));
        assert!(normalize_anthropic_tool_thinking_history(&mut tool_body));
        assert_eq!(
            tool_body["messages"][0]["content"][0]["thinking"],
            anthropic_tool_thinking_placeholder()
        );
    }

    #[test]
    fn model_mapping_adapter_projects_body_and_log_message() {
        let projection = apply_provider_model_mapping(
            json!({"model": "claude-sonnet", "messages": []}),
            &json!({
                "env": {
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-mapped"
                }
            }),
        );

        assert_eq!(
            projection.body.get("model").and_then(Value::as_str),
            Some("sonnet-mapped")
        );
        assert_eq!(
            projection.log_message.as_deref(),
            Some("[ModelMapper] 模型映射: claude-sonnet \u{2192} sonnet-mapped")
        );

        let unchanged = apply_provider_model_mapping(json!({"model": "unknown"}), &json!({}));
        assert_eq!(
            unchanged.body.get("model").and_then(Value::as_str),
            Some("unknown")
        );
        assert!(unchanged.log_message.is_none());
    }

    #[test]
    fn upstream_url_adapter_projects_codex_and_gemini_url_rules() {
        let (endpoint, passthrough_query) =
            rewrite_codex_responses_endpoint_to_chat("/v1/responses?foo=bar");
        assert_eq!(endpoint, "/chat/completions?foo=bar");
        assert_eq!(passthrough_query.as_deref(), Some("foo=bar"));

        assert_eq!(
            build_gemini_native_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "/v1beta/models/gemini-2.5-pro:generateContent",
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
        );
        assert_eq!(
            resolve_gemini_native_url(
                "https://relay.example/custom/generate-content",
                "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
                true,
            ),
            "https://relay.example/custom/generate-content?alt=sse"
        );
    }

    #[test]
    fn claude_api_format_adapter_projects_transform_gate() {
        assert!(!claude_api_format_needs_transform("anthropic"));
        assert!(claude_api_format_needs_transform("openai_chat"));
        assert!(claude_api_format_needs_transform("openai_responses"));
        assert!(claude_api_format_needs_transform("gemini_native"));
        assert!(!claude_api_format_needs_transform("unknown"));
    }

    #[test]
    fn upstream_request_adapter_projects_headers_and_body_serialization() {
        let mut inbound_headers = HeaderMap::new();
        inbound_headers.insert(http::header::HOST, http::HeaderValue::from_static("local"));
        inbound_headers.insert(
            http::header::ACCEPT_ENCODING,
            http::HeaderValue::from_static("gzip"),
        );

        let auth_headers = [(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer token"),
        )];
        let anthropic_beta = anthropic_beta_header_value(Some("other-beta"));
        let headers = build_upstream_request_headers(UpstreamRequestHeadersInput {
            inbound_headers: &inbound_headers,
            upstream_host: Some("upstream.example"),
            auth_headers: &auth_headers,
            channel_header_overrides: None,
            force_identity_encoding: true,
            custom_user_agent: None,
            is_copilot: false,
            should_send_anthropic_headers: true,
            anthropic_beta_value: Some(&anthropic_beta),
            codex_oauth_session_headers: &[],
            ensure_json_content_type: true,
        });

        assert_eq!(
            headers.get(http::header::HOST).and_then(|value| value.to_str().ok()),
            Some("upstream.example")
        );
        assert_eq!(
            headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer token")
        );
        assert_eq!(
            headers
                .get(http::header::ACCEPT_ENCODING)
                .and_then(|value| value.to_str().ok()),
            Some("identity")
        );
        assert_eq!(
            headers
                .get("anthropic-beta")
                .and_then(|value| value.to_str().ok()),
            Some("claude-code-20250219,other-beta")
        );
        assert_eq!(
            headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        assert!(serialize_upstream_request_body(&http::Method::GET, &json!({"model": "x"}))
            .unwrap()
            .is_empty());
        assert_eq!(
            serialize_upstream_request_body(&http::Method::POST, &json!({"model": "x"}))
                .unwrap(),
            br#"{"model":"x"}"#
        );
    }

    #[test]
    fn upstream_transport_adapter_projects_request_and_send_policy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::ACCEPT,
            http::HeaderValue::from_static("text/event-stream"),
        );

        let request_policy = resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({"model": "gpt-5"}),
            &headers,
        );
        assert!(request_policy.is_streaming_request);
        assert!(request_policy.force_identity_encoding);
        assert!(is_streaming_upstream_request(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
            &json!({"model": "gemini-2.5-pro"}),
            &HeaderMap::new()
        ));
        assert!(is_socks_proxy_url(Some("socks5://127.0.0.1:1080")));

        let send_policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: true,
            preserve_exact_header_case: true,
            request_is_streaming: true,
            non_streaming_timeout: std::time::Duration::from_secs(5),
            streaming_first_byte_timeout: std::time::Duration::from_secs(1),
        });

        assert_eq!(send_policy.transport, UpstreamTransportKind::PooledReqwest);
        assert_eq!(
            send_policy.streaming_header_timeout,
            Some(std::time::Duration::from_secs(1))
        );
    }

    #[test]
    fn codex_user_agent_adapter_projects_official_client_policy() {
        assert!(is_official_codex_client_user_agent("codex_vscode/1.0.0"));
        assert!(is_official_codex_client_user_agent("codex_cli_rs/0.5.2"));
        assert!(!is_official_codex_client_user_agent("Mozilla/5.0"));
        assert!(!is_official_codex_client_user_agent(
            "prefix_codex_cli_rs/1.0.0"
        ));
    }

    #[test]
    fn usage_record_adapter_builds_request_log_and_missing_pricing_signal() {
        let record = UsageRecord {
            request_id: Some("req-usage-1".to_string()),
            message_id: Some("msg-usage-1".to_string()),
            app: AppKind::Claude,
            provider_id: "provider-a".to_string(),
            provider_kind: Some(ProviderKind::Claude),
            channel_id: Some("channel-a".to_string()),
            channel_name: Some("Channel A".to_string()),
            route_group: Some("default".to_string()),
            request_model: "public-sonnet".to_string(),
            outbound_model: "upstream-sonnet".to_string(),
            response_model: Some("upstream-sonnet".to_string()),
            pricing_model: None,
            tokens: crate::proxy_core::api::usage::UsageTokens {
                input_tokens: 1_000,
                output_tokens: 500,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
            latency_ms: 42,
            first_token_ms: Some(7),
            status_code: 200,
            error_message: None,
            session_id: Some("session-a".to_string()),
            is_streaming: true,
            metadata: json!({}),
        };
        let pricing =
            ModelPricing::from_strings("3.0", "15.0", "0.3", "3.75").expect("pricing");
        assert_eq!(
            usage_record_pricing_model(&record, "response"),
            "upstream-sonnet"
        );

        let projection = usage_record_to_request_log(
            &record,
            "response",
            Some(&pricing),
            Decimal::new(2, 0),
            || "fallback".to_string(),
        );

        assert_eq!(projection.log.request_id, "req-usage-1");
        assert_eq!(projection.log.provider_id, "provider-a");
        assert_eq!(projection.log.app_type, "claude");
        assert_eq!(projection.log.model, "upstream-sonnet");
        assert_eq!(projection.log.request_model, "public-sonnet");
        assert_eq!(projection.log.pricing_model, "upstream-sonnet");
        assert_eq!(projection.log.usage.input_tokens, 1_000);
        assert!(projection.log.cost.is_some());
        assert_eq!(projection.log.provider_type.as_deref(), Some("claude"));
        assert_eq!(projection.log.channel_id.as_deref(), Some("channel-a"));
        assert_eq!(projection.log.channel_name.as_deref(), Some("Channel A"));
        assert_eq!(projection.log.route_group.as_deref(), Some("default"));
        assert!(projection.log.is_streaming);
        assert_eq!(projection.log.cost_multiplier, "2");
        assert!(projection.missing_pricing_model.is_none());

        let missing_pricing = usage_record_to_request_log(
            &record,
            "response",
            None,
            Decimal::new(1, 0),
            || "fallback".to_string(),
        );
        assert_eq!(
            missing_pricing.missing_pricing_model.as_deref(),
            Some("upstream-sonnet")
        );
        assert!(missing_pricing.log.cost.is_none());
    }

    #[test]
    fn claude_takeover_adapter_projects_one_m_marker_and_display_name() {
        assert_eq!(
            claude_takeover_client_model_for_upstream(
                "claude-sonnet-4-6",
                true,
                "deepseek-v4-pro[1M]"
            ),
            "claude-sonnet-4-6[1M]"
        );
        assert_eq!(
            claude_takeover_client_model_for_upstream(
                "claude-haiku-4-5",
                false,
                "deepseek-v4-flash[1M]"
            ),
            "claude-haiku-4-5"
        );
        assert_eq!(
            claude_takeover_default_display_name("deepseek-v4-ultra [1m]  "),
            "deepseek-v4-ultra"
        );
    }

    #[test]
    fn host_session_adapter_generates_uuid_when_core_needs_new_session_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_proxy_session_id(&headers, &body, "claude");

        uuid::Uuid::parse_str(&result.session_id).expect("generated session id should be a UUID");
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }

    #[test]
    fn gemini_tool_call_id_adapter_uses_core_prefix_contract() {
        let id = synthesize_gemini_tool_call_id_with_uuid();

        assert!(id.starts_with(GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX));
        assert!(id.len() > GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX.len());
    }

    #[test]
    fn stream_check_adapter_preserves_reachability_fields() {
        let result = StreamCheckResult {
            status: HealthStatus::Degraded,
            success: true,
            message: "slow but reachable".to_string(),
            response_time_ms: Some(6100),
            http_status: Some(200),
            model_used: String::new(),
            tested_at: 1_797_000_000,
            retry_count: 1,
            error_category: None,
        };

        let reachability = stream_check_result_to_channel_reachability(result);

        assert!(reachability.success);
        assert_eq!(
            reachability.status,
            ChannelReachabilityStatus::Degraded.as_str()
        );
        assert_eq!(reachability.message, "slow but reachable");
        assert_eq!(reachability.latency_ms, Some(6100));
        assert_eq!(reachability.http_status, Some(200));
        assert_eq!(reachability.tested_at, 1_797_000_000);
        assert_eq!(reachability.retry_count, 1);

        for (health_status, reachability_status) in [
            (
                HealthStatus::Operational,
                ChannelReachabilityStatus::Operational,
            ),
            (HealthStatus::Failed, ChannelReachabilityStatus::Failed),
        ] {
            let result = StreamCheckResult {
                status: health_status,
                success: false,
                message: String::new(),
                response_time_ms: None,
                http_status: None,
                model_used: String::new(),
                tested_at: 0,
                retry_count: 0,
                error_category: None,
            };

            assert_eq!(
                stream_check_result_to_channel_reachability(result).status,
                reachability_status.as_str()
            );
        }
    }

    #[test]
    fn provider_conversion_uses_inferred_provider_kind_without_settings_leak() {
        let mut provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "secret-token",
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
                }
            }),
            Some("https://github.com/features/copilot".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("github_copilot".to_string()),
                account_id: Some("acct-1".to_string()),
            }),
            ..ProviderMeta::default()
        });

        let spec = provider.to_proxy_core_provider_spec(&AppType::Claude);

        assert_eq!(spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(spec.account_ref.as_deref(), Some("github_copilot:acct-1"));
        let serialized = serde_json::to_string(&spec).expect("serialize spec");
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("ANTHROPIC_AUTH_TOKEN"));
        assert!(!serialized.contains("settingsConfig"));
    }

    #[test]
    fn channel_conversion_preserves_endpoint_interface_models_and_groups() {
        let channel = ProxyChannelRecord {
            id: "ch-1".to_string(),
            provider_id: "provider-1".to_string(),
            app_type: "claude".to_string(),
            name: "Relay A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_chat_completions".to_string(),
            auth_profile_ref: Some("provider:claude:provider-1".to_string()),
            groups: vec!["default".to_string(), "paid".to_string()],
            priority: 50,
            weight: 80,
            retry_policy: json!({"maxAttempts": 2}),
            health_policy: json!({"breaker": "standard"}),
            header_overrides: json!({"x-test": "1"}),
            param_overrides: json!({"stream": true}),
            status_code_mapping: json!([{"from": 429, "to": 503}]),
            tags: vec!["manual".to_string()],
            metadata: json!({"owner": "ops"}),
            source_kind: ProxyChannelSourceKind::Manual,
            source_endpoint_url: Some("https://relay.example.com/v1".to_string()),
            models: vec![ProxyChannelModelRecord {
                channel_id: "ch-1".to_string(),
                public_model: "sonnet".to_string(),
                upstream_model: "anthropic/sonnet".to_string(),
                capabilities: json!({"tools": true}),
                pricing_model: Some("standard".to_string()),
                request_overrides: json!({"temperature": 0.2}),
                response_overrides: json!({}),
            }],
            needs_review: false,
            review_reasons: Vec::new(),
        };

        let spec = channel.to_proxy_core_channel_spec();

        assert_eq!(spec.app, AppKind::Claude);
        assert_eq!(spec.endpoint.base_url, "https://relay.example.com/v1");
        assert_eq!(spec.interface, InterfaceKind::OpenAiChatCompletions);
        assert_eq!(spec.groups, vec!["default".to_string(), "paid".to_string()]);
        assert_eq!(spec.models.len(), 1);
        assert_eq!(spec.models[0].public_model, "sonnet");
        assert_eq!(spec.models[0].upstream_model, "anthropic/sonnet");
        assert_eq!(
            spec.auth_profile.as_ref().map(|value| value.0.as_str()),
            Some("provider:claude:provider-1")
        );
    }
}
