use crate::app_config::AppType;
use crate::claude_desktop_config::ResolvedModelRoute;
use crate::database::{
    FailoverQueueItem, ProxyChannelKeyRecord, ProxyChannelModelRecord, ProxyChannelRecord,
    ProxyChannelMigrationPreview, ProxyChannelMaterializeResult, ProxyChannelSourceKind,
};
use crate::error::AppError;
use crate::provider::{Provider, ProviderMeta};
use crate::proxy::hyper_client::ProxyResponse;
use crate::proxy::providers::provider_kind_from_app_type_and_config;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy::usage::RequestLog;
use crate::proxy_core::api::auth::ClaudeDesktopModelRouteInput;
use crate::proxy_core::api::domain::{
    ChannelSpecInput, ModelRoute, ModelRouteInput, ProviderMetadata, ProviderMetadataInput,
};
#[cfg(test)]
use crate::proxy_core::api::domain::{ChannelHealthPolicy, ChannelOverrides, UpstreamEndpoint};
use crate::proxy_core::api::management::ChannelReachabilityResult;
use crate::proxy_core::api::routing::{
    route_resolve_channel_input_from_record, RouteResolveChannelInput,
    RouteResolveChannelRecordInput, RouteResolveModelRecordInput,
};
#[cfg(test)]
use crate::proxy_core::api::routing::RouteResolveModelInput;
use crate::proxy_core::api::session::SessionIdResult;
use crate::proxy_core::api::transforms::CodexChatErrorNormalization;
use crate::proxy_core::api::transport::{UpstreamRequestTransportPolicy, UpstreamSendPolicy};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use http::{HeaderMap, Method, StatusCode};
use indexmap::IndexMap;
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};
use std::sync::Arc;
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

pub(crate) fn error_message_with_context(
    context: &str,
    error: impl std::fmt::Display,
) -> String {
    crate::proxy_core::api::errors::error_message_with_context(context, &error.to_string())
}

pub(crate) fn app_error(context: &str, error: AppError) -> ProxyCoreError {
    ProxyCoreError::Config(error_message_with_context(context, error))
}

pub(crate) fn usage_error(context: &str, error: AppError) -> ProxyCoreError {
    ProxyCoreError::Internal(error_message_with_context(context, error))
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
pub(crate) type ChannelAuthProfileResolution =
    crate::proxy_core::api::domain::ChannelAuthProfileResolution;
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

pub(crate) fn proxy_global_config_from_config(config: GlobalProxyConfig) -> ProxyGlobalConfig {
    crate::proxy_core::api::config::proxy_global_config_from_global_config(config)
}

pub(crate) fn proxy_app_config_from_config_parts(
    app: AppKind,
    config: AppProxyConfig,
    current_provider_id: Option<String>,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ProxyAppConfig {
    crate::proxy_core::api::config::proxy_app_config_from_parts(
        app,
        config,
        current_provider_id,
        rectifier,
        optimizer,
        copilot_optimizer,
    )
}

pub(crate) fn current_provider_id_from_settings_for_app(app: &AppKind) -> Option<String> {
    app_type_option_from_proxy_core_app(app)
        .as_ref()
        .and_then(crate::settings::get_current_provider)
}

pub(crate) fn current_provider_id_from_settings_for_app_type(
    app_type: &AppType,
) -> Option<String> {
    crate::settings::get_current_provider(app_type)
}

pub(crate) fn proxy_app_config_from_config_source(
    app: AppKind,
    config: AppProxyConfig,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ProxyAppConfig {
    let settings_current_provider_id = current_provider_id_from_settings_for_app(&app);
    proxy_app_config_from_config_source_parts(
        app,
        config,
        settings_current_provider_id.as_deref(),
        rectifier,
        optimizer,
        copilot_optimizer,
    )
}

pub(crate) fn forward_current_provider_id_from_source(
    settings_current_provider_id: Option<&str>,
    load_db_current_provider_id: impl FnOnce() -> Option<String>,
) -> String {
    let db_current_provider_id =
        if current_provider_db_fallback_required(settings_current_provider_id) {
            load_db_current_provider_id()
        } else {
            None
        };
    current_provider_id_from_sources(
        settings_current_provider_id,
        db_current_provider_id.as_deref(),
    )
}

pub(crate) fn proxy_app_config_from_config_source_parts(
    app: AppKind,
    config: AppProxyConfig,
    settings_current_provider_id: Option<&str>,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ProxyAppConfig {
    let current_provider_id =
        current_provider_id_option_from_sources(settings_current_provider_id, None);
    proxy_app_config_from_config_parts(
        app,
        config,
        current_provider_id,
        rectifier,
        optimizer,
        copilot_optimizer,
    )
}

pub(crate) fn app_proxy_config_from_proxy_app_config(
    config: &ProxyAppConfig,
) -> Result<AppProxyConfig, String> {
    serde_json::from_value(config.raw.clone())
        .map_err(|error| format!("invalid app proxy config: {error}"))
}

pub(crate) fn proxy_runtime_config_from_config(
    config: ProxyConfig,
    privacy_filter_enabled: bool,
) -> ProxyRuntimeConfig {
    crate::proxy_core::api::config::proxy_runtime_config_from_proxy_config(
        config,
        privacy_filter_enabled,
    )
}

pub(crate) fn proxy_runtime_config_from_config_source(config: ProxyConfig) -> ProxyRuntimeConfig {
    proxy_runtime_config_from_config(config, false)
}

pub(crate) type ProviderHealth = crate::proxy_core::api::ports::ProviderHealth;
pub(crate) type ProviderKind = crate::proxy_core::api::domain::ProviderKind;
pub(crate) type ProviderAuthInfo =
    crate::proxy_core::api::auth::ProviderAuthInfo;
pub(crate) type ProviderAuthStrategy =
    crate::proxy_core::api::auth::ProviderAuthStrategy;
pub(crate) type AuthInfo = crate::proxy_core::api::ports::AuthInfo;

pub(crate) fn auth_info_from_profile_ref(
    auth_profile: Option<&AuthProfileRef>,
    source: &str,
) -> AuthInfo {
    crate::proxy_core::api::ports::auth_info_from_profile_ref(auth_profile, source)
}

pub(crate) fn auth_info_from_cc_switch_provider_config(
    auth_profile: Option<&AuthProfileRef>,
) -> AuthInfo {
    auth_info_from_profile_ref(auth_profile, "cc_switch_provider_config")
}

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

pub(crate) const DEFAULT_PROXY_LISTEN_PORT: u16 =
    crate::proxy_core::api::ports::DEFAULT_PROXY_LISTEN_PORT;
pub(crate) const DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD: u32 =
    crate::proxy_core::api::ports::DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD;
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

pub(crate) fn extract_openclaw_stream_check_base_url(settings_config: &Value) -> Option<String> {
    crate::proxy_core::api::domain::extract_openclaw_stream_check_base_url(settings_config)
}

pub(crate) fn extract_hermes_stream_check_base_url(settings_config: &Value) -> Option<String> {
    crate::proxy_core::api::domain::extract_hermes_stream_check_base_url(settings_config)
}

pub(crate) fn extract_opencode_stream_check_npm(settings_config: &Value) -> Option<String> {
    crate::proxy_core::api::domain::extract_opencode_stream_check_npm(settings_config)
}

pub(crate) fn resolve_opencode_stream_check_base_url(
    settings_config: &Value,
    npm: Option<&str>,
) -> Option<String> {
    crate::proxy_core::api::domain::resolve_opencode_stream_check_base_url(settings_config, npm)
}

pub(crate) fn channel_auth_profile_resolution(
    auth_profile_ref: Option<&str>,
    app_type: &str,
) -> ChannelAuthProfileResolution {
    let auth_profile_ref = auth_profile_ref.map(AuthProfileRef::new);
    crate::proxy_core::api::domain::channel_auth_profile_resolution(
        auth_profile_ref.as_ref(),
        app_type,
    )
}

pub(crate) fn channel_auth_profile_missing_provider_warning(
    app_type: &str,
    auth_profile_ref: Option<&str>,
) -> String {
    crate::proxy_core::api::domain::channel_auth_profile_missing_provider_warning(
        app_type,
        auth_profile_ref,
    )
}

pub(crate) enum ChannelAuthProfileAction {
    Provider {
        provider_id: String,
        missing_provider_warning: String,
    },
    ChannelKey {
        channel_id: String,
        key_ref: String,
    },
    Ignore,
}

pub(crate) fn channel_auth_profile_action(
    app_type: &str,
    auth_profile_ref: Option<&str>,
    channel_id: Option<&str>,
) -> ChannelAuthProfileAction {
    match channel_auth_profile_resolution(auth_profile_ref, app_type) {
        ChannelAuthProfileResolution::Provider { provider_id } => {
            ChannelAuthProfileAction::Provider {
                provider_id,
                missing_provider_warning: channel_auth_profile_missing_provider_warning(
                    app_type,
                    auth_profile_ref,
                ),
            }
        }
        ChannelAuthProfileResolution::ChannelKey { key_ref } => {
            let Some(channel_id) = channel_id else {
                return ChannelAuthProfileAction::Ignore;
            };
            ChannelAuthProfileAction::ChannelKey {
                channel_id: channel_id.to_string(),
                key_ref,
            }
        }
        ChannelAuthProfileResolution::Ignore => ChannelAuthProfileAction::Ignore,
    }
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
pub type ChannelReachabilityStatus =
    crate::proxy_core::api::management::ChannelReachabilityStatus;
pub type StreamCheckConfig = crate::proxy_core::api::management::StreamCheckConfig;
pub type StreamCheckResult = crate::proxy_core::api::management::StreamCheckResult;
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
pub(crate) type RoutePlanProviderMatch =
    crate::proxy_core::api::routing::RoutePlanProviderMatch;
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
    AppSummaryConfig, AuthProvider, ChannelHealthReset, ChannelHealthStore, ChannelSource, ForwardPipeline,
    ModelCatalogProvider, ProviderSource, ProxyConfigSource, ProxyEventSink, ProxyServices,
    RoutePolicySource, RouteResolver, UsageSink,
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
    channel_health_update_from_input, plan_channel_test, provider_health_update_from_input,
    AppChannelListQuery,
    AppChannelManagementRequest, AppChannelResponse, AppListRequest, AppListResponse,
    AppModelCatalogRequest,
    AppModelListQuery, ChannelCreateRequest,
    CHANNEL_HEALTH_UNKNOWN_STATUS,
    ChannelDeleteResponse, ChannelHealthResetResponse,
    ChannelHealthResetSource, ChannelHealthUpdateInput, ChannelKeyDeleteResponse,
    ChannelKeyPathRequest, ChannelKeyRecordResponse,
    ChannelKeysResponse, ChannelListQuery,
    ChannelListRequest, ChannelListResponse,
    ChannelMigrationMaterializeInput, ChannelMigrationMaterializeResponse,
    ChannelMigrationPreviewInput, ChannelMigrationPreviewResponse, ChannelModelsResponse,
    ChannelPathRequest, ChannelRecordResponse,
    ChannelRouteRejected,
    ChannelTestPlan, ChannelTestResponse, CurrentRouteResponse, GroupListQuery, GroupListRequest,
    HealthCheckRequest, HealthCheckResponse, HealthCheckSource, ManagementAppPathRequest,
    ProviderHealthUpdateInput, ProviderListResponse,
    ProxyChannelModelsReplaceRequest, ProxyChannelTestRequest, ProxyStatusRequest,
    ProxyStatusResponse, ProxyStatusSource, RouteGroupListResponse,
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
    app_proxy_config_defaults_for_app, cache_injection_log_message, normalize_thinking_type,
    rectify_anthropic_request, rectify_thinking_budget, should_rectify_thinking_budget,
    should_rectify_thinking_signature, thinking_optimization_log_message,
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
    contains_image_blocks, is_codex_chat_full_endpoint_base,
    is_github_copilot_upstream, is_openai_o_series, is_unsupported_image_error,
    merge_copilot_tool_results,
    prepare_upstream_request_body_with_report, prompt_cache_trace_log_message,
    replace_image_blocks_with_marker, replace_images_for_text_only_model,
    request_body_filter_log_message,
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
pub(crate) use crate::proxy_core::api::transport::{
    interface_kind_for_forward, request_model_for_forward,
};
pub(crate) use crate::proxy_core::api::usage::{
    normalize_pricing_source, validate_cost_multiplier_value, CostMultiplierValidationError,
    PricingSourceValidationError, PRICING_SOURCE_REQUEST, PRICING_SOURCE_RESPONSE,
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

pub(crate) struct ProxyEventBusMessage {
    pub(crate) event_name: String,
    pub(crate) payload: Value,
}

pub(crate) fn proxy_core_event_to_bus_message(event: ProxyCoreEvent) -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: event.event_type.event_name(),
        payload: event.into_event_payload(),
    }
}

pub(crate) fn emit_proxy_core_event(
    event: ProxyCoreEvent,
    mut emit: impl FnMut(String, Value),
) {
    let message = proxy_core_event_to_bus_message(event);
    emit(message.event_name, message.payload);
}

pub(crate) fn proxy_engine_from_services<S>(services: Arc<S>) -> ProxyEngine<S>
where
    S: ProxyServices + ?Sized,
{
    ProxyEngine::new(services)
}

pub(crate) fn health_check_source_from_timestamp(
    timestamp: impl Into<String>,
) -> HealthCheckSource {
    HealthCheckSource::new(timestamp)
}

pub(crate) fn proxy_status_source_from_status(
    status: ProxyRuntimeStatus,
) -> ProxyStatusSource<ProxyRuntimeStatus> {
    ProxyStatusSource::new(status)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwarderRuntimeOptions {
    pub(crate) non_streaming_timeout: u64,
    pub(crate) streaming_first_byte_timeout: u64,
    pub(crate) streaming_idle_timeout: u64,
    pub(crate) max_retries: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwarderRuntimeConfig {
    pub(crate) options: ForwarderRuntimeOptions,
    pub(crate) rectifier: RectifierConfig,
    pub(crate) optimizer: OptimizerConfig,
    pub(crate) copilot_optimizer: CopilotOptimizerConfig,
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

pub(crate) fn response_runtime_policy_from_app_proxy_config(
    config: &AppProxyConfig,
) -> ResponseRuntimePolicy {
    resolve_response_runtime_policy(
        config.auto_failover_enabled,
        config.max_retries,
        config.non_streaming_timeout as u64,
        config.streaming_first_byte_timeout as u64,
        config.streaming_idle_timeout as u64,
    )
}

pub(crate) fn forwarder_runtime_options_from_app_proxy_config(
    config: &AppProxyConfig,
) -> ForwarderRuntimeOptions {
    let policy = response_runtime_policy_from_app_proxy_config(config);
    ForwarderRuntimeOptions {
        non_streaming_timeout: policy.timeout.non_streaming_timeout,
        streaming_first_byte_timeout: policy.timeout.streaming.first_byte_timeout,
        streaming_idle_timeout: policy.timeout.streaming.idle_timeout,
        max_retries: policy.max_retries,
    }
}

pub(crate) fn forwarder_runtime_config_from_sources(
    app_config: &AppProxyConfig,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ForwarderRuntimeConfig {
    ForwarderRuntimeConfig {
        options: forwarder_runtime_options_from_app_proxy_config(app_config),
        rectifier,
        optimizer,
        copilot_optimizer,
    }
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

pub(crate) fn settings_config_with_channel_auth_key(
    app_type: &str,
    settings_config: &Value,
    key_value: &str,
) -> Value {
    crate::proxy_core::api::auth::settings_config_with_channel_auth_key(
        app_type,
        settings_config,
        key_value,
    )
}

pub(crate) fn channel_auth_profile_missing_key_error_message(
    channel_id: &str,
    key_ref: &str,
) -> String {
    crate::proxy_core::api::auth::channel_auth_profile_missing_key_error_message(
        channel_id,
        key_ref,
    )
}

pub(crate) fn channel_key_auth_error(channel_id: &str, key_ref: &str) -> ProxyCoreError {
    ProxyCoreError::Auth(channel_auth_profile_missing_key_error_message(
        channel_id, key_ref,
    ))
}

pub(crate) fn channel_key_value_from_record(key: Option<ProxyChannelKeyRecord>) -> Option<String> {
    key.map(|key| key.key_value)
}

pub(crate) fn provider_with_channel_auth_key(
    app_type: &AppType,
    provider: &Provider,
    key_value: &str,
) -> Provider {
    let mut auth_provider = provider.clone();
    auth_provider.settings_config = settings_config_with_channel_auth_key(
        app_type.as_str(),
        &provider.settings_config,
        key_value,
    );
    auth_provider
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

pub(crate) fn current_provider_id_from_sources(
    settings_current_provider_id: Option<&str>,
    db_current_provider_id: Option<&str>,
) -> String {
    crate::proxy_core::api::routing::current_provider_id_from_sources(
        settings_current_provider_id,
        db_current_provider_id,
    )
}

pub(crate) fn current_provider_id_option_from_sources(
    settings_current_provider_id: Option<&str>,
    db_current_provider_id: Option<&str>,
) -> Option<String> {
    crate::proxy_core::api::routing::current_provider_id_option_from_sources(
        settings_current_provider_id,
        db_current_provider_id,
    )
}

pub(crate) fn current_provider_db_fallback_required(
    settings_current_provider_id: Option<&str>,
) -> bool {
    crate::proxy_core::api::routing::current_provider_db_fallback_required(
        settings_current_provider_id,
    )
}

pub(crate) fn should_block_proxy_switch_to_provider_category(
    proxy_takeover_active: bool,
    provider_category: Option<&str>,
) -> bool {
    crate::proxy_core::api::routing::should_block_proxy_switch_to_provider_category(
        proxy_takeover_active,
        provider_category,
    )
}

pub(crate) fn should_attempt_restored_provider_switchback(
    proxy_takeover_active: bool,
    auto_failover_enabled: bool,
    proxy_service_running: bool,
    restored_sort_index: Option<usize>,
    current_sort_index: Option<usize>,
) -> bool {
    crate::proxy_core::api::routing::should_attempt_restored_provider_switchback(
        proxy_takeover_active,
        auto_failover_enabled,
        proxy_service_running,
        restored_sort_index,
        current_sort_index,
    )
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

pub(crate) fn legacy_provider_config_text_from_settings(settings_config: &Value) -> Option<&str> {
    crate::proxy_core::api::routing::legacy_provider_config_text_from_settings(settings_config)
}

pub(crate) fn legacy_provider_env_from_settings(
    settings_config: &Value,
) -> std::collections::BTreeMap<String, String> {
    crate::proxy_core::api::routing::legacy_provider_env_from_settings(settings_config)
}

pub(crate) fn legacy_provider_codex_catalog_models_from_settings(
    settings_config: &Value,
) -> Vec<String> {
    crate::proxy_core::api::routing::legacy_provider_codex_catalog_models_from_settings(
        settings_config,
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

pub(crate) fn legacy_provider_projection_input(
    provider: &Provider,
) -> LegacyProviderProjectionInput {
    let config_text = legacy_provider_config_text_from_settings(&provider.settings_config);
    let env = legacy_provider_env_from_settings(&provider.settings_config);
    let codex_catalog_models =
        legacy_provider_codex_catalog_models_from_settings(&provider.settings_config);
    let (api_format, claude_desktop_model_routes) = provider
        .meta
        .as_ref()
        .map(|meta| {
            let routes = meta
                .claude_desktop_model_routes
                .iter()
                .map(|(public_model, route)| LegacyModelRouteInput {
                    public_model: public_model.clone(),
                    upstream_model: route.model.clone(),
                })
                .collect::<Vec<_>>();
            (meta.api_format.clone(), routes)
        })
        .unwrap_or_default();

    LegacyProviderProjectionInput {
        api_format,
        codex_wire_api: config_text.and_then(extract_codex_wire_api),
        codex_model: config_text.and_then(extract_codex_model),
        codex_catalog_models,
        env,
        claude_desktop_model_routes,
    }
}

pub(crate) fn proxy_channel_record_from_legacy_projection(
    projection: LegacyChannelProjection,
    source_kind: ProxyChannelSourceKind,
) -> ProxyChannelRecord {
    ProxyChannelRecord {
        id: projection.id,
        provider_id: projection.provider_id,
        app_type: projection.app_type,
        name: projection.name,
        status: projection.status,
        base_url: projection.base_url,
        interface_kind: projection.interface_kind,
        auth_profile_ref: projection.auth_profile_ref,
        groups: projection.groups,
        priority: projection.priority,
        weight: projection.weight,
        retry_policy: projection.retry_policy,
        health_policy: projection.health_policy,
        header_overrides: projection.header_overrides,
        param_overrides: projection.param_overrides,
        status_code_mapping: projection.status_code_mapping,
        tags: projection.tags,
        metadata: projection.metadata,
        source_kind,
        source_endpoint_url: projection.source_endpoint_url,
        models: projection
            .models
            .into_iter()
            .map(proxy_channel_model_record_from_legacy)
            .collect(),
        needs_review: projection.needs_review,
        review_reasons: projection.review_reasons,
    }
}

fn proxy_channel_model_record_from_legacy(
    route: LegacyChannelModelProjection,
) -> ProxyChannelModelRecord {
    ProxyChannelModelRecord {
        channel_id: route.channel_id,
        public_model: route.public_model,
        upstream_model: route.upstream_model,
        capabilities: route.capabilities,
        pricing_model: route.pricing_model,
        request_overrides: route.request_overrides,
        response_overrides: route.response_overrides,
    }
}

fn extract_codex_wire_api(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<toml::Value>().ok()?;
    if let Some(active_provider) = doc.get("model_provider").and_then(|value| value.as_str()) {
        if let Some(wire_api) = doc
            .get("model_providers")
            .and_then(|providers| providers.get(active_provider))
            .and_then(|provider| provider.get("wire_api"))
            .and_then(|value| value.as_str())
        {
            return Some(wire_api.to_string());
        }
    }
    doc.get("wire_api")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn extract_codex_model(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<toml::Value>().ok()?;
    doc.get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn normalize_required_channel_string(
    value: &str,
    field: &str,
) -> Result<String, ChannelRequestValidationError> {
    crate::proxy_core::api::routing::normalize_required_channel_string(value, field)
}

pub(crate) fn normalize_channel_base_url(value: &str) -> String {
    crate::proxy_core::api::routing::normalize_channel_base_url(value)
}

pub(crate) fn normalize_proxy_channel_write_request_fields(
    request: ProxyChannelWriteRequest,
) -> Result<ProxyChannelWriteRequest, ChannelRequestValidationError> {
    crate::proxy_core::api::routing::normalize_proxy_channel_write_request_fields(request)
}

pub(crate) fn normalize_proxy_channel_patch_request_fields(
    request: ProxyChannelPatchRequest,
) -> Result<ProxyChannelPatchRequest, ChannelRequestValidationError> {
    crate::proxy_core::api::routing::normalize_proxy_channel_patch_request_fields(request)
}

pub(crate) fn normalize_proxy_channel_model_write_request_fields(
    model: ProxyChannelModelWriteRequest,
) -> Result<ProxyChannelModelWriteRequest, ChannelRequestValidationError> {
    crate::proxy_core::api::routing::normalize_proxy_channel_model_write_request_fields(model)
}

pub(crate) fn normalize_proxy_channel_models_replace_request_fields(
    request: ProxyChannelModelsReplaceRequest,
) -> Result<ProxyChannelModelsReplaceRequest, ChannelRequestValidationError> {
    crate::proxy_core::api::routing::normalize_proxy_channel_models_replace_request_fields(request)
}

pub(crate) fn normalize_proxy_channel_key_write_request_fields(
    request: ProxyChannelKeyWriteRequest,
) -> Result<ProxyChannelKeyWriteRequest, ChannelRequestValidationError> {
    crate::proxy_core::api::routing::normalize_proxy_channel_key_write_request_fields(request)
}

pub(crate) fn normalize_proxy_channel_key_patch_request_fields(
    request: ProxyChannelKeyPatchRequest,
) -> Result<ProxyChannelKeyPatchRequest, ChannelRequestValidationError> {
    crate::proxy_core::api::routing::normalize_proxy_channel_key_patch_request_fields(request)
}

impl From<&AppType> for AppKind {
    fn from(value: &AppType) -> Self {
        Self::from(value.as_str())
    }
}

pub(crate) fn app_type_option_from_proxy_core_app(app: &AppKind) -> Option<AppType> {
    app.as_str().parse::<AppType>().ok()
}

pub(crate) fn app_type_from_proxy_core_app(app: &AppKind) -> ProxyCoreResult<AppType> {
    app.as_str()
        .parse::<AppType>()
        .map_err(|error| ProxyCoreError::Config(unsupported_app_kind_error_message(error)))
}

pub(crate) struct ForwardRuntimeRequest {
    pub(crate) app_type: AppType,
    pub(crate) method: Method,
    pub(crate) endpoint: String,
    pub(crate) headers: HeaderMap,
    pub(crate) extensions: http::Extensions,
    pub(crate) body: Value,
    pub(crate) session_result: SessionIdResult,
}

pub(crate) fn forward_runtime_request_from_proxy_request(
    request: ProxyRequest,
) -> ProxyCoreResult<ForwardRuntimeRequest> {
    let ProxyRequest {
        app,
        method,
        endpoint,
        headers,
        extensions,
        body,
        ..
    } = request;
    let app_type = app_type_from_proxy_core_app(&app)?;
    let body = body.into_json()?;
    let session_result = extract_proxy_session_id(&headers, &body, app_type.as_str());
    Ok(ForwardRuntimeRequest {
        app_type,
        method,
        endpoint,
        headers,
        extensions,
        body,
        session_result,
    })
}

pub(crate) fn unsupported_app_kind_error_message(error: impl std::fmt::Display) -> String {
    crate::proxy_core::api::domain::unsupported_app_kind_error_message(&error.to_string())
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

pub(crate) fn proxy_provider_to_core_spec(provider: &Provider, app_type: &AppType) -> ProviderSpec {
    provider.to_proxy_core_provider_spec(app_type)
}

pub(crate) fn proxy_providers_to_core_specs(
    providers: impl IntoIterator<Item = Provider>,
    app_type: &AppType,
) -> Vec<ProviderSpec> {
    providers
        .into_iter()
        .map(|provider| proxy_provider_to_core_spec(&provider, app_type))
        .collect()
}

pub(crate) fn provider_specs_from_source(
    app: &AppKind,
    providers: impl IntoIterator<Item = Provider>,
) -> ProxyCoreResult<Vec<ProviderSpec>> {
    let app_type = app_type_from_proxy_core_app(app)?;
    Ok(proxy_providers_to_core_specs(providers, &app_type))
}

pub(crate) fn provider_spec_from_source(
    app: &AppKind,
    provider: Option<Provider>,
) -> ProxyCoreResult<Option<ProviderSpec>> {
    let app_type = app_type_from_proxy_core_app(app)?;
    Ok(provider.map(|provider| proxy_provider_to_core_spec(&provider, &app_type)))
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

pub(crate) fn proxy_channel_records_to_core_specs_for_query(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
    query: &ChannelQuery<'_>,
) -> Vec<ChannelSpec> {
    channels
        .into_iter()
        .map(|channel| proxy_channel_record_to_core_spec(&channel))
        .filter(|channel| channel_matches_query(channel, query))
        .collect()
}

pub(crate) fn channel_specs_from_source(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
    query: &ChannelQuery<'_>,
) -> Vec<ChannelSpec> {
    proxy_channel_records_to_core_specs_for_query(channels, query)
}

pub(crate) fn channel_spec_from_source(channel: Option<ProxyChannelRecord>) -> Option<ChannelSpec> {
    channel.map(|channel| proxy_channel_record_to_core_spec(&channel))
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

            route_resolve_channel_input_from_record(RouteResolveChannelRecordInput {
                channel_id: id,
                provider_id,
                channel_name: name,
                status,
                base_url,
                interface_kind,
                groups,
                models: models
                    .into_iter()
                    .map(|model| RouteResolveModelRecordInput {
                        public_model: model.public_model,
                        upstream_model: model.upstream_model,
                    })
                    .collect(),
                priority,
                weight,
                source_kind: source_kind.as_str().to_string(),
            })
        })
        .collect()
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

pub(crate) fn provider_model_catalog_from_provider(
    provider_id: &str,
    provider: Option<&Provider>,
) -> ModelCatalog {
    provider_model_catalog_from_settings(
        provider_id,
        provider.map(|provider| &provider.settings_config),
    )
}

pub(crate) fn client_model_catalog_from_optional_raw(
    app: &AppKind,
    raw: Option<Value>,
) -> ModelCatalog {
    crate::proxy_core::api::model_catalog::client_model_catalog_from_optional_raw(
        app.as_str(),
        raw,
    )
}

pub(crate) fn client_model_catalog_from_source(app: &AppKind) -> ModelCatalog {
    let raw = match app {
        AppKind::Codex => Some(codex_client_model_catalog_raw_from_active_config()),
        _ => None,
    };
    client_model_catalog_from_optional_raw(app, raw)
}

pub(crate) fn empty_client_model_catalog_raw() -> Value {
    crate::proxy_core::api::model_catalog::empty_client_model_catalog_raw()
}

pub(crate) fn client_model_catalog_raw_from_text(catalog_text: &str) -> Value {
    crate::proxy_core::api::model_catalog::client_model_catalog_raw_from_text(catalog_text)
}

pub(crate) fn codex_client_model_catalog_raw_from_active_config() -> Value {
    let generated_path = crate::codex_config::get_codex_model_catalog_path();
    let active_catalog_path = match crate::codex_config::read_codex_config_text() {
        Ok(config_text) => {
            crate::codex_config::resolve_cc_switch_catalog_path(&config_text, &generated_path)
        }
        Err(_) => None,
    };

    if let Some(catalog_path) = active_catalog_path.as_ref().filter(|path| path.exists()) {
        let text = std::fs::read_to_string(catalog_path).unwrap_or_default();
        client_model_catalog_raw_from_text(&text)
    } else {
        if active_catalog_path.is_none() {
            log::debug!(
                "[models] stale guard: catalog not served (model_catalog_json not set to cc-switch catalog)"
            );
        }
        empty_client_model_catalog_raw()
    }
}

#[cfg(test)]
pub(crate) fn route_plan_provider_ids(plan: &RoutePlan) -> Vec<String> {
    crate::proxy_core::api::routing::route_plan_provider_ids(plan)
}

pub(crate) fn route_plan_provider_match<I, S>(
    plan: &RoutePlan,
    configured_provider_ids: I,
) -> RoutePlanProviderMatch
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    crate::proxy_core::api::routing::route_plan_provider_match(plan, configured_provider_ids)
}

pub(crate) fn host_providers_for_plan(
    providers: &IndexMap<String, Provider>,
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<Provider>> {
    let provider_match = route_plan_provider_match(plan, providers.keys().map(String::as_str));
    let has_matches = provider_match.has_matches();
    let matching: Vec<_> = provider_match
        .matched_provider_ids
        .iter()
        .filter_map(|provider_id| providers.get(provider_id.as_str()).cloned())
        .collect();
    if !has_matches {
        return Err(ProxyCoreError::Unavailable(
            route_plan_providers_unconfigured_error_message().to_string(),
        ));
    }
    Ok(matching)
}

pub(crate) fn forward_attempts_from_plan(
    app_type: &AppType,
    providers: &[Provider],
    plan: &RoutePlan,
) -> Vec<ForwardAttempt> {
    crate::proxy::route_attempt::forward_attempts_from_route_plan(app_type, providers, plan)
}

pub(crate) fn required_forward_attempts_from_plan(
    app_type: &AppType,
    providers: &[Provider],
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<ForwardAttempt>> {
    let attempts = forward_attempts_from_plan(app_type, providers, plan);
    if attempts.is_empty() {
        return Err(route_plan_no_matching_host_providers_error());
    }
    Ok(attempts)
}

pub(crate) fn apply_channel_auth_profile_providers_from_source(
    app_type: &AppType,
    providers: &IndexMap<String, Provider>,
    attempts: &mut [ForwardAttempt],
    mut load_channel_key_value: impl FnMut(&str, &str) -> ProxyCoreResult<Option<String>>,
) -> ProxyCoreResult<()> {
    for attempt in attempts {
        let auth_profile_ref = attempt
            .channel()
            .and_then(|channel| channel.auth_profile_ref.as_ref())
            .map(String::as_str);
        let channel_id = attempt.channel().map(|channel| channel.channel_id.as_str());
        match channel_auth_profile_action(app_type.as_str(), auth_profile_ref, channel_id) {
            ChannelAuthProfileAction::Provider {
                provider_id,
                missing_provider_warning,
            } => {
                let Some(provider) = providers.get(&provider_id).cloned() else {
                    log::warn!("{missing_provider_warning}");
                    continue;
                };
                attempt.set_auth_provider(provider);
            }
            ChannelAuthProfileAction::ChannelKey {
                channel_id,
                key_ref,
            } => {
                let Some(key_value) = load_channel_key_value(&channel_id, &key_ref)? else {
                    return Err(channel_key_auth_error(&channel_id, &key_ref));
                };
                attempt.set_auth_provider(provider_with_channel_auth_key(
                    app_type,
                    attempt.provider(),
                    &key_value,
                ));
            }
            ChannelAuthProfileAction::Ignore => {
                continue;
            }
        }
    }
    Ok(())
}

pub(crate) fn forwarding_requires_runtime_error_message() -> &'static str {
    crate::proxy_core::api::routing::forwarding_requires_runtime_error_message()
}

pub(crate) fn forwarding_runtime_unavailable_error() -> ProxyCoreError {
    ProxyCoreError::Unsupported(forwarding_requires_runtime_error_message().to_string())
}

pub(crate) fn route_plan_no_matching_host_providers_error_message() -> &'static str {
    crate::proxy_core::api::routing::route_plan_no_matching_host_providers_error_message()
}

pub(crate) fn route_plan_no_matching_host_providers_error() -> ProxyCoreError {
    ProxyCoreError::Unavailable(route_plan_no_matching_host_providers_error_message().to_string())
}

pub(crate) fn route_plan_providers_unconfigured_error_message() -> &'static str {
    crate::proxy_core::api::routing::route_plan_providers_unconfigured_error_message()
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

pub(crate) fn route_policy_from_failover_queue(
    app: AppKind,
    queue: impl IntoIterator<Item = FailoverQueueItem>,
) -> RoutePolicy {
    crate::proxy_core::api::routing::route_policy_from_failover_provider_ids(
        app,
        queue.into_iter().map(|item| item.provider_id),
    )
}

pub(crate) fn route_policy_from_source(
    app: AppKind,
    queue: impl IntoIterator<Item = FailoverQueueItem>,
) -> Option<RoutePolicy> {
    Some(route_policy_from_failover_queue(app, queue))
}

pub(crate) fn channel_health_reset_from_parts(
    channel_id: impl Into<String>,
    app_type: &str,
) -> ChannelHealthReset {
    crate::proxy_core::api::ports::channel_health_reset_from_parts(channel_id, app_type)
}

#[derive(Debug)]
pub(crate) struct ChannelHealthResetPlan {
    pub(crate) channel_id: String,
    pub(crate) app_type: String,
}

pub(crate) fn channel_health_reset_plan_from_lookup(
    channel_id: &str,
    app_type: Option<String>,
) -> ProxyCoreResult<ChannelHealthResetPlan> {
    let app_type = app_type.ok_or_else(|| channel_not_found_error(channel_id))?;
    Ok(ChannelHealthResetPlan {
        channel_id: channel_id.to_string(),
        app_type,
    })
}

pub(crate) fn channel_health_reset_from_plan(
    plan: ChannelHealthResetPlan,
) -> ChannelHealthReset {
    channel_health_reset_from_parts(plan.channel_id, plan.app_type.as_str())
}

pub(crate) struct ChannelHealthAttemptDbUpdate {
    pub(crate) channel_id: String,
    pub(crate) success: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) failure_threshold: u32,
    pub(crate) response_time_ms: Option<i64>,
}

pub(crate) fn channel_health_attempt_db_update(
    result: ChannelAttemptResult,
) -> ChannelHealthAttemptDbUpdate {
    ChannelHealthAttemptDbUpdate {
        channel_id: result.channel_id,
        success: result.success,
        error_code: result.error_code,
        failure_threshold: DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD,
        response_time_ms: result.latency_ms.map(|latency| latency as i64),
    }
}

pub(crate) fn proxy_response_to_core_response<G>(
    response: ProxyResponse,
    connection_guard: Option<G>,
) -> ProxyCoreResponse
where
    G: Send + 'static,
{
    match response {
        ProxyResponse::Buffered {
            status,
            headers,
            body,
        } => ProxyCoreResponse::with_body(status, headers, ProxyResponseBody::bytes(body)),
        ProxyResponse::Streamed {
            status,
            headers,
            stream,
        } => ProxyCoreResponse::with_body(
            status,
            headers,
            ProxyResponseBody::stream(stream_with_connection_guard(stream, connection_guard)),
        ),
        other => {
            let status = other.status();
            let headers = other.headers().clone();
            ProxyCoreResponse::with_body(
                status,
                headers,
                ProxyResponseBody::stream(stream_with_connection_guard(
                    other.bytes_stream(),
                    connection_guard,
                )),
            )
        }
    }
}

pub(crate) fn forward_result_to_proxy_result(
    result: crate::proxy::ForwardResult,
    plan: RoutePlan,
) -> ProxyResult {
    let crate::proxy::ForwardResult {
        response,
        provider,
        claude_api_format,
        outbound_model,
        selected_channel,
        connection_guard,
    } = result;
    let selected_channel_id = selected_channel
        .as_ref()
        .map(|channel| channel.channel_id.as_str());
    let response = proxy_response_to_core_response(response, connection_guard);

    proxy_result_from_forward_parts(
        response,
        plan,
        &provider,
        claude_api_format,
        outbound_model,
        selected_channel_id,
    )
}

fn stream_with_connection_guard<S, G>(
    stream: S,
    connection_guard: Option<G>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    G: Send + 'static,
{
    async_stream::stream! {
        let _connection_guard = connection_guard;
        tokio::pin!(stream);
        while let Some(chunk) = stream.next().await {
            yield chunk;
        }
    }
}

pub(crate) fn proxy_result_from_forward_parts(
    response: ProxyCoreResponse,
    plan: RoutePlan,
    provider: &Provider,
    claude_api_format: Option<String>,
    outbound_model: Option<String>,
    selected_channel_id: Option<&str>,
) -> ProxyResult {
    let selected_route =
        route_selection_for_forward_result(&plan, selected_channel_id, &provider.id);
    let mut metadata = Map::new();
    metadata.insert("hostProviderId".to_string(), json!(provider.id.clone()));
    metadata.insert("hostProviderName".to_string(), json!(provider.name.clone()));
    metadata.insert(
        CLAUDE_API_FORMAT_METADATA_KEY.to_string(),
        json!(claude_api_format),
    );
    metadata.insert("selectedChannelId".to_string(), json!(selected_channel_id));

    ProxyResult {
        response,
        selected_route,
        outbound_model,
        usage_record: None,
        metadata: Value::Object(metadata),
    }
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

#[cfg(test)]
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

pub(crate) fn apply_resolved_channel_model_override(
    body: &mut Value,
    channel: &ResolvedChannelAttempt,
) -> Option<crate::proxy_core::api::transport::ChannelRouteModelOverride> {
    crate::proxy_core::api::transport::apply_resolved_channel_model_override(body, channel)
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

pub(crate) fn response_headers_indicate_sse(headers: &HeaderMap) -> bool {
    crate::proxy_core::api::transport::response_headers_indicate_sse(headers)
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
    pub(crate) missing_pricing_warning_message: Option<String>,
}

pub(crate) struct UsagePricingConfigLookup {
    pub(crate) provider_id: String,
    pub(crate) app_type: String,
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

pub(crate) fn usage_pricing_config_lookup_from_record(
    record: &UsageRecord,
) -> UsagePricingConfigLookup {
    UsagePricingConfigLookup {
        provider_id: record.provider_id.clone(),
        app_type: record.app.as_str().to_string(),
    }
}

pub(crate) fn usage_route_context_from_selection(selection: &RouteSelection) -> UsageRouteContext {
    crate::proxy_core::api::usage::usage_route_context_from_selection(selection)
}

#[derive(Debug, Clone)]
pub(crate) struct RequestContextRouteUpdate {
    pub(crate) outbound_model: Option<String>,
    pub(crate) usage_route_context: UsageRouteContext,
    pub(crate) provider: Provider,
}

pub(crate) fn request_context_route_update_from_proxy_result(
    app_type: &AppType,
    provider: &Provider,
    result: &ProxyResult,
) -> RequestContextRouteUpdate {
    RequestContextRouteUpdate {
        outbound_model: result.outbound_model.clone(),
        usage_route_context: usage_route_context_from_selection(&result.selected_route),
        provider: ForwardAttempt::from_core_selection(app_type, provider, &result.selected_route)
            .provider()
            .clone(),
    }
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
    let projection = crate::proxy_core::api::usage::usage_request_log_projection(
        record,
        pricing_model_source,
        pricing,
        multiplier,
        fallback_request_id,
    );
    let fields = projection.fields;
    UsageRequestLogProjection {
        log: RequestLog {
            request_id: fields.request_id,
            provider_id: fields.provider_id,
            app_type: fields.app_type,
            model: fields.model,
            request_model: fields.request_model,
            pricing_model: fields.pricing_model,
            usage: fields.usage,
            cost: fields.cost,
            latency_ms: fields.latency_ms,
            first_token_ms: fields.first_token_ms,
            status_code: fields.status_code,
            error_message: fields.error_message,
            session_id: fields.session_id,
            provider_type: fields.provider_type,
            channel_id: fields.channel_id,
            channel_name: fields.channel_name,
            route_group: fields.route_group,
            is_streaming: fields.is_streaming,
            cost_multiplier: fields.cost_multiplier,
        },
        missing_pricing_warning_message: projection.missing_pricing_warning_message,
    }
}

pub(crate) fn is_placeholder_pricing_model(model_id: &str) -> bool {
    crate::proxy_core::api::usage::is_placeholder_pricing_model(model_id)
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

pub(crate) fn channel_health_reset_source_from_response(
    response: ChannelHealthResetResponse,
) -> ChannelHealthResetSource {
    ChannelHealthResetSource::new(response)
}

pub(crate) fn channel_migration_preview_input_from_result(
    preview: ProxyChannelMigrationPreview,
) -> ChannelMigrationPreviewInput<ChannelRecord> {
    ChannelMigrationPreviewInput::new(
        preview.app_type,
        proxy_channel_records_to_core(preview.channels),
        preview.duplicate_count,
        preview.needs_review_count,
    )
}

pub(crate) fn channel_migration_materialize_input_from_result(
    result: ProxyChannelMaterializeResult,
) -> ChannelMigrationMaterializeInput {
    ChannelMigrationMaterializeInput::new(
        result.app_type,
        result.previewed_channels,
        result.inserted_channels,
        result.inserted_models,
        result.inserted_health_rows,
        result.duplicate_count,
        result.needs_review_count,
    )
}

pub(crate) fn channel_test_plan_from_record(
    channel: &ProxyChannelRecord,
    request: &ProxyChannelTestRequest,
    tested_at: i64,
) -> ChannelTestPlan {
    plan_channel_test(&proxy_channel_record_to_core_spec(channel), request, tested_at)
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
    crate::proxy_core::api::management::channel_reachability_result_from_stream_check_result(result)
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
    use crate::provider::{
        AuthBinding, AuthBindingSource, ClaudeDesktopModelRoute, ProviderMeta,
    };
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
        assert_eq!(
            app_type_from_proxy_core_app(&AppKind::Claude).expect("claude app"),
            AppType::Claude
        );
        assert_eq!(
            app_type_option_from_proxy_core_app(&AppKind::Custom("openclaw".to_string())),
            Some(AppType::OpenClaw)
        );
        assert!(matches!(
            app_type_from_proxy_core_app(&AppKind::Custom("unknown-app".to_string())),
            Err(ProxyCoreError::Config(message))
                if message.starts_with("unsupported app kind:")
                    && message.contains("unknown-app")
        ));
        assert_eq!(
            unsupported_app_kind_error_message("invalid app: openclaw"),
            "unsupported app kind: invalid app: openclaw"
        );

        let forward_request = forward_runtime_request_from_proxy_request(ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Bytes(Bytes::from_static(br#"{"ok":true}"#)),
        ))
        .expect("forward request");
        assert_eq!(forward_request.app_type, AppType::Claude);
        assert_eq!(forward_request.method, Method::POST);
        assert_eq!(forward_request.endpoint, "/v1/messages");
        assert_eq!(forward_request.body, json!({"ok": true}));
        assert_eq!(forward_request.session_result.source, SessionIdSource::Generated);
        assert!(!forward_request.session_result.client_provided);
        Uuid::parse_str(&forward_request.session_result.session_id)
            .expect("generated forward session id should be a UUID");

        let invalid_request = match forward_runtime_request_from_proxy_request(ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Bytes(Bytes::from_static(b"{bad-json")),
        )) {
            Ok(_) => panic!("invalid JSON body should fail"),
            Err(error) => error,
        };
        assert!(matches!(
            invalid_request,
            ProxyCoreError::InvalidRequest(message) if message.contains("invalid JSON body")
        ));
    }

    #[test]
    fn channel_auth_profile_warning_adapter_projects_optional_ref() {
        assert_eq!(
            channel_auth_profile_missing_provider_warning(
                "claude",
                Some("provider:claude:missing"),
            ),
            "[claude] channel auth profile references missing provider: provider:claude:missing"
        );
        assert_eq!(
            channel_auth_profile_missing_provider_warning("claude", None),
            "[claude] channel auth profile references missing provider: "
        );
        assert!(matches!(
            channel_auth_profile_action(
                "claude",
                Some("provider:claude:provider-a"),
                Some("channel-a")
            ),
            ChannelAuthProfileAction::Provider {
                provider_id,
                missing_provider_warning,
            } if provider_id == "provider-a"
                && missing_provider_warning.contains("provider:claude:provider-a")
        ));
        assert!(matches!(
            channel_auth_profile_action("claude", Some("channel-key:primary"), Some("channel-a")),
            ChannelAuthProfileAction::ChannelKey { channel_id, key_ref }
                if channel_id == "channel-a" && key_ref == "primary"
        ));
        assert!(matches!(
            channel_auth_profile_action("claude", Some("channel-key:primary"), None),
            ChannelAuthProfileAction::Ignore
        ));
        assert!(matches!(
            channel_key_auth_error("channel-a", "primary"),
            ProxyCoreError::Auth(message)
                if message.contains("channel_id=channel-a")
                    && message.contains("key_ref=primary")
        ));

        let provider = Provider::with_id(
            "route-provider".to_string(),
            "Route Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "route-key" } }),
            None,
        );
        let auth_provider =
            provider_with_channel_auth_key(&AppType::Claude, &provider, "channel-key");
        assert_eq!(
            provider
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("route-key")
        );
        assert_eq!(
            auth_provider
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("channel-key")
        );

        let route_provider = provider.clone();
        fn attempt_with_auth_ref(
            route_provider: &Provider,
            channel_id: &str,
            auth_profile_ref: &str,
        ) -> ForwardAttempt {
            let provider = ProviderSpec {
                id: route_provider.id.clone(),
                name: route_provider.name.clone(),
                kind: ProviderKind::Claude,
                account_ref: None,
                metadata: ProviderMetadata::default(),
            };
            let channel = ChannelSpec {
                id: channel_id.to_string(),
                provider_id: route_provider.id.clone(),
                app: AppKind::Claude,
                name: channel_id.to_string(),
                status: ChannelStatus::Enabled,
                endpoint: UpstreamEndpoint {
                    base_url: format!("https://{channel_id}.example.com/v1"),
                    path_template: None,
                    api_version: None,
                    timeout_profile: None,
                },
                interface: InterfaceKind::AnthropicMessages,
                auth_profile: Some(AuthProfileRef::new(auth_profile_ref)),
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
            let selection = crate::proxy_core::api::routing::route_selection_from_parts(
                provider,
                channel,
                None,
                InterfaceKind::AnthropicMessages,
            );
            ForwardAttempt::from_core_selection(&AppType::Claude, route_provider, &selection)
        }

        let provider_auth = Provider::with_id(
            "provider-auth".to_string(),
            "Provider Auth".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-auth-key" } }),
            None,
        );
        let mut providers = IndexMap::new();
        providers.insert(route_provider.id.clone(), route_provider.clone());
        providers.insert(provider_auth.id.clone(), provider_auth);
        let mut provider_attempt = attempt_with_auth_ref(
            &route_provider,
            "channel-a",
            "provider:claude:provider-auth",
        );
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut provider_attempt),
            |_, _| unreachable!("provider auth should not load channel keys"),
        )
        .expect("apply provider auth profile");
        assert_eq!(provider_attempt.auth_provider().id, "provider-auth");

        let mut channel_key_attempt = attempt_with_auth_ref(
            &route_provider,
            "channel-key",
            "channel-key:primary",
        );
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut channel_key_attempt),
            |channel_id, key_ref| {
                assert_eq!(channel_id, "channel-key");
                assert_eq!(key_ref, "primary");
                Ok(Some("loaded-channel-key".to_string()))
            },
        )
        .expect("apply channel key auth profile");
        assert_eq!(
            channel_key_attempt
                .auth_provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("loaded-channel-key")
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
        assert_eq!(
            error_message_with_context("load config", "disk failed"),
            "load config: disk failed"
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
        let enabled_policy = response_runtime_policy_from_app_proxy_config(&app_config);
        assert_eq!(enabled_policy.max_retries, 3);
        assert_eq!(enabled_policy.timeout.non_streaming_timeout, 600);
        assert_eq!(enabled_policy.timeout.streaming.first_byte_timeout, 60);
        assert_eq!(enabled_policy.timeout.streaming.idle_timeout, 120);
        assert_eq!(
            forwarder_runtime_options_from_app_proxy_config(&app_config),
            ForwarderRuntimeOptions {
                non_streaming_timeout: 600,
                streaming_first_byte_timeout: 60,
                streaming_idle_timeout: 120,
                max_retries: 3,
            }
        );
        let forwarder_config = forwarder_runtime_config_from_sources(
            &app_config,
            RectifierConfig {
                request_media_fallback: false,
                ..RectifierConfig::default()
            },
            OptimizerConfig {
                enabled: true,
                cache_ttl: "2h".to_string(),
                ..OptimizerConfig::default()
            },
            CopilotOptimizerConfig {
                warmup_model: "gpt-5".to_string(),
                ..CopilotOptimizerConfig::default()
            },
        );
        assert_eq!(
            forwarder_config.options,
            ForwarderRuntimeOptions {
                non_streaming_timeout: 600,
                streaming_first_byte_timeout: 60,
                streaming_idle_timeout: 120,
                max_retries: 3,
            }
        );
        assert!(!forwarder_config.rectifier.request_media_fallback);
        assert!(forwarder_config.optimizer.enabled);
        assert_eq!(forwarder_config.optimizer.cache_ttl, "2h");
        assert_eq!(forwarder_config.copilot_optimizer.warmup_model, "gpt-5");
        let projected_app = proxy_app_config_from_config_source_parts(
            AppKind::Claude,
            app_config.clone(),
            Some("anthropic-main"),
            RectifierConfig::default(),
            OptimizerConfig::default(),
            CopilotOptimizerConfig::default(),
        );
        assert_eq!(projected_app.app, Some(AppKind::Claude));
        assert_eq!(
            projected_app.raw["currentProviderId"],
            json!("anthropic-main")
        );
        assert_eq!(
            app_proxy_config_from_proxy_app_config(&projected_app)
                .expect("project host app config from core raw"),
            app_config
        );
        let invalid_projected_app = ProxyAppConfig {
            raw: json!({ "enabled": true }),
            ..projected_app.clone()
        };
        let error = app_proxy_config_from_proxy_app_config(&invalid_projected_app)
            .expect_err("invalid raw app config");
        assert!(error.starts_with("invalid app proxy config:"));

        let mut disabled_app_config = app_config.clone();
        disabled_app_config.auto_failover_enabled = false;
        let disabled_policy = response_runtime_policy_from_app_proxy_config(&disabled_app_config);
        assert_eq!(disabled_policy.max_retries, 0);
        assert_eq!(disabled_policy.timeout, ResponseTimeoutConfig::default());
        assert_eq!(
            forwarder_runtime_options_from_app_proxy_config(&disabled_app_config),
            ForwarderRuntimeOptions {
                non_streaming_timeout: ResponseTimeoutConfig::default().non_streaming_timeout,
                streaming_first_byte_timeout: ResponseTimeoutConfig::default()
                    .streaming
                    .first_byte_timeout,
                streaming_idle_timeout: ResponseTimeoutConfig::default()
                    .streaming
                    .idle_timeout,
                max_retries: 0,
            }
        );
        assert_eq!(
            serde_json::to_value(GlobalProxyConfig {
                proxy_enabled: true,
                listen_address: "127.0.0.1".to_string(),
                listen_port: DEFAULT_PROXY_LISTEN_PORT,
                enable_logging: true,
            })
            .expect("global proxy config")
            .get("proxyEnabled")
            .and_then(Value::as_bool),
            Some(true)
        );
        assert!(!proxy_runtime_config_from_config_source(ProxyConfig::default())
            .privacy_filter_enabled);
    }

    #[test]
    fn auth_adapter_projects_cc_switch_provider_config_source() {
        let auth_profile = AuthProfileRef::new("provider:claude:anthropic-main");
        let auth = auth_info_from_cc_switch_provider_config(Some(&auth_profile));
        assert!(auth.headers.is_empty());
        assert_eq!(
            auth.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(auth.metadata["source"], json!("cc_switch_provider_config"));

        let fallback = auth_info_from_cc_switch_provider_config(None);
        assert!(fallback.account_ref.is_none());
        assert_eq!(fallback.metadata["source"], json!("cc_switch_provider_config"));
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
        assert_eq!(
            current_provider_id_from_sources(Some("settings-provider"), Some("db-provider")),
            "settings-provider"
        );
        assert_eq!(
            current_provider_id_option_from_sources(Some("settings-provider"), Some("db-provider")),
            Some("settings-provider".to_string())
        );
        assert!(!current_provider_db_fallback_required(Some("settings-provider")));
        assert!(!current_provider_db_fallback_required(Some("")));
        assert!(current_provider_db_fallback_required(None));
        assert_eq!(current_provider_id_option_from_sources(None, None), None);
        let mut db_lookup_called = false;
        assert_eq!(
            forward_current_provider_id_from_source(Some("settings-provider"), || {
                db_lookup_called = true;
                Some("db-provider".to_string())
            }),
            "settings-provider"
        );
        assert!(!db_lookup_called);
        assert_eq!(
            forward_current_provider_id_from_source(None, || Some("db-provider".to_string())),
            "db-provider"
        );
        assert_eq!(forward_current_provider_id_from_source(None, || None), "");

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

        let message = proxy_core_event_to_bus_message(ProxyCoreEvent {
            event_type: ProxyCoreEventType::RouteSelected,
            request_id: Some("req-1".to_string()),
            channel_id: Some("channel-a".to_string()),
            payload: json!({"attemptCount": 2}),
        });
        assert_eq!(message.event_name, "route_selected");
        assert_eq!(message.payload["requestId"], "req-1");
        assert_eq!(message.payload["channelId"], "channel-a");
        assert_eq!(message.payload["attemptCount"], 2);

        let mut emitted = None;
        emit_proxy_core_event(
            ProxyCoreEvent {
                event_type: ProxyCoreEventType::RouteSelected,
                request_id: Some("req-2".to_string()),
                channel_id: Some("channel-b".to_string()),
                payload: json!({"attemptCount": 1}),
            },
            |event_name, payload| emitted = Some((event_name, payload)),
        );
        let (event_name, payload) = emitted.expect("event emitted");
        assert_eq!(event_name, "route_selected");
        assert_eq!(payload["requestId"], "req-2");
        assert_eq!(payload["channelId"], "channel-b");
        assert_eq!(payload["attemptCount"], 1);
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
        let normalized =
            normalize_proxy_channel_write_request_fields(request).expect("valid channel request");
        assert_eq!(normalized.base_url, "https://api.example.com/v1");

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

        let record = proxy_channel_record_from_legacy_projection(
            projection,
            ProxyChannelSourceKind::LegacyPrimary,
        );
        assert_eq!(record.provider_id, "provider-a");
        assert_eq!(record.source_kind, ProxyChannelSourceKind::LegacyPrimary);
        assert_eq!(record.models.len(), 1);
        assert_eq!(record.models[0].channel_id, record.id);
        assert_eq!(record.models[0].public_model, "claude-sonnet-4-6");
    }

    #[test]
    fn legacy_provider_projection_input_projects_provider_settings() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "sonnet-safe".to_string(),
            ClaudeDesktopModelRoute {
                model: "claude-sonnet-4".to_string(),
                label_override: Some("Sonnet".to_string()),
                supports_1m: None,
            },
        );
        let mut provider = Provider::with_id(
            "codex-relay".to_string(),
            "Codex Relay".to_string(),
            json!({
                "config": "model_provider = \"custom\"\nmodel = \"gpt-5.4\"\n\n[model_providers.custom]\nwire_api = \"chat\"\n",
                "env": {
                    "ANTHROPIC_MODEL": "claude-sonnet-4",
                    "IGNORED_NON_STRING": 123
                },
                "modelCatalog": {
                    "models": [
                        { "model": "gpt-5.4" },
                        { "model": "gpt-5.4-mini" },
                        { "notModel": "skip" }
                    ]
                }
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            api_format: Some("openai_responses".to_string()),
            claude_desktop_model_routes: routes,
            ..ProviderMeta::default()
        });

        assert!(
            legacy_provider_config_text_from_settings(&provider.settings_config)
                .is_some_and(|config| config.contains("wire_api = \"chat\""))
        );
        assert_eq!(
            legacy_provider_env_from_settings(&provider.settings_config)
                .get("ANTHROPIC_MODEL")
                .map(String::as_str),
            Some("claude-sonnet-4")
        );
        assert_eq!(
            legacy_provider_codex_catalog_models_from_settings(&provider.settings_config),
            vec!["gpt-5.4".to_string(), "gpt-5.4-mini".to_string()]
        );

        let projection = legacy_provider_projection_input(&provider);

        assert_eq!(projection.api_format.as_deref(), Some("openai_responses"));
        assert_eq!(projection.codex_wire_api.as_deref(), Some("chat"));
        assert_eq!(projection.codex_model.as_deref(), Some("gpt-5.4"));
        assert_eq!(
            projection.codex_catalog_models,
            vec!["gpt-5.4".to_string(), "gpt-5.4-mini".to_string()]
        );
        assert_eq!(
            projection.env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("claude-sonnet-4")
        );
        assert!(!projection.env.contains_key("IGNORED_NON_STRING"));
        assert_eq!(projection.claude_desktop_model_routes.len(), 1);
        assert_eq!(
            projection.claude_desktop_model_routes[0].public_model,
            "sonnet-safe"
        );
        assert_eq!(
            projection.claude_desktop_model_routes[0].upstream_model,
            "claude-sonnet-4"
        );
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
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            settings.clone(),
            None,
        );
        assert_eq!(
            provider_model_catalog_from_provider("provider-a", Some(&provider)).models,
            provider_catalog.models
        );

        let client_catalog = client_model_catalog_from_optional_raw(
            &AppKind::Codex,
            Some(json!({
                "models": [
                    {"id": " gpt-5 "},
                    {"model": "o4-mini"},
                    {"id": "gpt-5"}
                ]
            })),
        );
        assert_eq!(client_catalog.provider_id, "codex");
        assert_eq!(
            client_catalog.models,
            vec!["gpt-5".to_string(), "o4-mini".to_string()]
        );
        let empty_client_catalog = client_model_catalog_from_source(&AppKind::Gemini);
        assert_eq!(empty_client_catalog.provider_id, "gemini");
        assert_eq!(empty_client_catalog.models, Vec::<String>::new());
        assert_eq!(empty_client_catalog.raw, json!({"models": []}));
        assert_eq!(
            client_model_catalog_raw_from_text(r#"{"models":[{"id":"gpt-5"}]}"#),
            json!({"models":[{"id":"gpt-5"}]})
        );
        assert_eq!(client_model_catalog_raw_from_text("not json"), json!({"models": []}));
        assert_eq!(empty_client_model_catalog_raw(), json!({"models": []}));
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
        let result = ProxyResult {
            response: ProxyCoreResponse::empty(http::StatusCode::OK),
            selected_route: selected,
            outbound_model: Some("upstream-sonnet".to_string()),
            usage_record: None,
            metadata: json!({}),
        };
        let selected_host_provider = Provider::with_id(
            "provider-b".to_string(),
            "Provider B".to_string(),
            json!({}),
            None,
        );
        let update = request_context_route_update_from_proxy_result(
            &AppType::Claude,
            &selected_host_provider,
            &result,
        );
        assert_eq!(update.outbound_model.as_deref(), Some("upstream-sonnet"));
        assert_eq!(update.usage_route_context.channel_id, "ch-b");
        assert_eq!(update.provider.id, "provider-b");
        let host_provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        let attempts = forward_attempts_from_plan(&AppType::Claude, &[host_provider], &plan);
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].provider().id, "provider-a");
        assert_eq!(attempts[1].provider().id, "provider-a");
        let missing_attempts =
            required_forward_attempts_from_plan(&AppType::Claude, &[], &plan).unwrap_err();
        assert!(matches!(
            missing_attempts,
            ProxyCoreError::Unavailable(message)
                if message == "route plan has no matching host providers"
        ));
        assert_eq!(
            forwarding_requires_runtime_error_message(),
            "cc-switch forwarding requires a proxy server runtime"
        );
        assert!(matches!(
            forwarding_runtime_unavailable_error(),
            ProxyCoreError::Unsupported(message)
                if message == "cc-switch forwarding requires a proxy server runtime"
        ));
        assert_eq!(
            route_plan_no_matching_host_providers_error_message(),
            "route plan has no matching host providers"
        );
        assert!(matches!(
            route_plan_no_matching_host_providers_error(),
            ProxyCoreError::Unavailable(message)
                if message == "route plan has no matching host providers"
        ));
        assert_eq!(
            route_plan_providers_unconfigured_error_message(),
            "route plan providers are not configured in host database"
        );
        let policy = route_policy_from_source(
            AppKind::Claude,
            vec![FailoverQueueItem {
                provider_id: "provider-b".to_string(),
                provider_name: "Provider B".to_string(),
                sort_index: Some(1),
                provider_notes: None,
            }],
        )
        .expect("route policy");
        assert_eq!(policy.app, AppKind::Claude);
        assert_eq!(policy.raw["failoverProviderIds"], json!(["provider-b"]));

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
        assert!(matches!(
            app_error("load config", AppError::Message("disk failed".to_string())),
            ProxyCoreError::Config(message)
                if message == "load config: disk failed"
        ));
        assert!(matches!(
            usage_error("record usage", AppError::Message("db failed".to_string())),
            ProxyCoreError::Internal(message)
                if message == "record usage: db failed"
        ));
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
        let lookup = usage_pricing_config_lookup_from_record(&record);
        assert_eq!(lookup.provider_id, "provider-a");
        assert_eq!(lookup.app_type, "claude");
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

        let missing_pricing = usage_record_to_request_log(
            &record,
            "response",
            None,
            Decimal::new(1, 0),
            || "fallback".to_string(),
        );
        assert_eq!(
            missing_pricing.missing_pricing_warning_message.as_deref(),
            Some("[USG-002] 模型定价未找到，成本将记录为 0: upstream-sonnet")
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
            status: ChannelReachabilityStatus::Degraded,
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
                ChannelReachabilityStatus::Operational,
                ChannelReachabilityStatus::Operational,
            ),
            (
                ChannelReachabilityStatus::Failed,
                ChannelReachabilityStatus::Failed,
            ),
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
    fn channel_health_adapter_projects_attempt_db_update() {
        let reset_plan =
            channel_health_reset_plan_from_lookup("channel-a", Some("claude".to_string()))
                .expect("reset plan");
        assert_eq!(reset_plan.channel_id, "channel-a");
        assert_eq!(reset_plan.app_type, "claude");
        let reset = channel_health_reset_from_plan(reset_plan);
        assert_eq!(reset.channel_id, "channel-a");
        assert_eq!(reset.app, AppKind::Claude);
        let missing = channel_health_reset_plan_from_lookup("missing-channel", None)
            .expect_err("missing channel should fail");
        assert!(missing.to_string().contains("missing-channel"));

        let update = channel_health_attempt_db_update(ChannelAttemptResult {
            channel_id: "channel-a".to_string(),
            success: false,
            status_code: Some(429),
            latency_ms: Some(123),
            error_code: Some("rate_limited".to_string()),
        });

        assert_eq!(update.channel_id, "channel-a");
        assert!(!update.success);
        assert_eq!(update.error_code.as_deref(), Some("rate_limited"));
        assert_eq!(
            update.failure_threshold,
            DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD
        );
        assert_eq!(update.response_time_ms, Some(123));
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
        let source_spec = provider_spec_from_source(&AppKind::Claude, Some(provider.clone()))
            .expect("provider spec")
            .expect("provider");
        let source_specs =
            provider_specs_from_source(&AppKind::Claude, vec![provider]).expect("provider specs");

        assert_eq!(spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(source_spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(source_specs[0].kind, ProviderKind::GitHubCopilot);
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
        let source_spec = channel_spec_from_source(Some(channel.clone())).expect("channel spec");

        assert_eq!(source_spec.id, "ch-1");
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
