use crate::app_config::AppType;
use crate::claude_desktop_config::ResolvedModelRoute;
use crate::database::{ProxyChannelModelRecord, ProxyChannelRecord};
use crate::provider::{Provider, ProviderMeta};
use crate::proxy::providers::provider_kind_from_app_type_and_config;
use crate::proxy::usage::RequestLog;
use crate::proxy_core::{
    AppKind, AppSummaryInput, AuthProfileRef, ChannelHealthPolicy, ChannelModelRecord,
    ChannelRouteCandidate,
    ChannelOverrides, ChannelReachabilityInput, ChannelReachabilityResult,
    ChannelReachabilityStatus, ChannelRecord, ChannelSpec, ChannelStatus,
    ClaudeDesktopModelListResponse, ClaudeDesktopModelRouteInput, CodexChatErrorNormalization,
    CodexToolContext,
    CostCalculator, CurrentRouteProviderSummaryInput, InterfaceKind, ModelCapabilities,
    ModelCatalog, ModelPricing, ModelRoute, ProviderMetadata, ProviderSpec, RetryPolicy,
    ResolvedChannelAttempt, RoutePlan, RouteResolveChannelInput, RouteResolveModelInput,
    RouteSelection, SessionIdResult, UpstreamEndpoint, UpstreamRequestHeadersInput,
    UpstreamRequestTransportPolicy, UpstreamSendPolicy, UpstreamSendPolicyInput,
    UsageRecord, DEFAULT_ROUTE_GROUP,
};
use crate::services::usage_stats::is_placeholder_pricing_model;
use crate::services::stream_check::{HealthStatus, StreamCheckResult};
use http::HeaderMap;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) fn synthesize_gemini_tool_call_id_with_uuid() -> String {
    crate::proxy_core::synthesize_gemini_tool_call_id(Uuid::new_v4().simple().to_string())
}

pub(crate) type ClaudeDesktopGatewayAuthError =
    crate::proxy_core::ClaudeDesktopGatewayAuthError;

pub(crate) type ProxyErrorStatusKind = crate::proxy_core::ProxyErrorStatusKind;

pub(crate) fn validate_claude_desktop_gateway_bearer_header(
    headers: &HeaderMap,
    expected_token: &str,
) -> Result<(), ClaudeDesktopGatewayAuthError> {
    crate::proxy_core::validate_claude_desktop_gateway_bearer_header(headers, expected_token)
}

pub(crate) fn proxy_error_http_status_code(kind: ProxyErrorStatusKind) -> u16 {
    crate::proxy_core::proxy_error_http_status_code(kind)
}

pub(crate) const SYSTEM_PROXY_ENV_KEYS: [&str; 6] =
    crate::proxy_core::SYSTEM_PROXY_ENV_KEYS;

pub(crate) fn mask_url_for_log(url: &str) -> String {
    crate::proxy_core::mask_url_for_log(url)
}

#[cfg(test)]
pub(crate) fn proxy_url_points_to_loopback_port(value: &str, loopback_port: u16) -> bool {
    crate::proxy_core::proxy_url_points_to_loopback_port(value, loopback_port)
}

pub(crate) fn proxy_values_point_to_loopback_port<I, V>(
    values: I,
    loopback_port: u16,
) -> bool
where
    I: IntoIterator<Item = V>,
    V: AsRef<str>,
{
    crate::proxy_core::proxy_values_point_to_loopback_port(values, loopback_port)
}

pub(crate) const COPILOT_PUBLIC_GITHUB_DOMAIN: &str =
    crate::proxy_core::COPILOT_PUBLIC_GITHUB_DOMAIN;

pub(crate) fn default_copilot_github_domain() -> String {
    crate::proxy_core::default_copilot_github_domain()
}

pub(crate) fn normalize_github_domain(raw: &str) -> Result<String, String> {
    crate::proxy_core::normalize_github_domain(raw)
}

pub(crate) fn is_copilot_ghes_domain(domain: &str) -> bool {
    crate::proxy_core::is_copilot_ghes_domain(domain)
}

pub(crate) fn copilot_composite_account_id(domain: &str, user_id: u64) -> String {
    crate::proxy_core::copilot_composite_account_id(domain, user_id)
}

pub(crate) type CopilotModel = crate::proxy_core::CopilotModel;

pub(crate) fn parse_copilot_models_response_bytes(
    body: &[u8],
) -> Result<Vec<CopilotModel>, String> {
    crate::proxy_core::parse_copilot_models_response_bytes(body)
}

pub(crate) fn copilot_github_client_id(domain: &str) -> &'static str {
    crate::proxy_core::copilot_github_client_id(domain)
}

pub(crate) fn copilot_github_device_code_url(domain: &str) -> String {
    crate::proxy_core::copilot_github_device_code_url(domain)
}

pub(crate) fn copilot_github_oauth_token_url(domain: &str) -> String {
    crate::proxy_core::copilot_github_oauth_token_url(domain)
}

pub(crate) fn copilot_github_user_url(domain: &str) -> String {
    crate::proxy_core::copilot_github_user_url(domain)
}

pub(crate) fn copilot_token_url(domain: &str) -> String {
    crate::proxy_core::copilot_token_url(domain)
}

pub(crate) fn copilot_usage_url(domain: &str) -> String {
    crate::proxy_core::copilot_usage_url(domain)
}

pub(crate) fn copilot_api_base(domain: &str) -> String {
    crate::proxy_core::copilot_api_base(domain)
}

pub(crate) type FetchedModel = crate::proxy_core::FetchedModel;
pub(crate) type CodexOAuthModelsRequest<'a> =
    crate::proxy_core::CodexOAuthModelsRequest<'a>;
pub(crate) type OpenAiCompatibleModelsRequest<'a> =
    crate::proxy_core::OpenAiCompatibleModelsRequest<'a>;
pub(crate) type ModelFetchHttpResponse = crate::proxy_core::ModelFetchHttpResponse;
pub(crate) use crate::proxy_core::{
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
    crate::proxy_core::fetch_openai_compatible_models_with_transport(
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
    crate::proxy_core::fetch_codex_oauth_models_with_transport(
        token,
        account_id,
        client_version,
        transport,
    )
    .await
}

pub(crate) type RectifierConfig = crate::proxy_core::RectifierConfig;
pub(crate) type OptimizerConfig = crate::proxy_core::OptimizerConfig;
pub(crate) type CopilotOptimizerConfig = crate::proxy_core::CopilotOptimizerConfig;

pub(crate) type ProxyConfig = crate::proxy_core::ProxyConfig;
pub(crate) type ProxyRuntimeStatus = crate::proxy_core::ProxyRuntimeStatus;
pub(crate) type ProxyServerInfo = crate::proxy_core::ProxyServerInfo;
pub(crate) type ProxyTakeoverStatus = crate::proxy_core::ProxyTakeoverStatus;
pub(crate) type ProxyCoreResponse = crate::proxy_core::ProxyCoreResponse;
pub(crate) type ProxyEventEnvelope = crate::proxy_core::ProxyEventEnvelope;
pub(crate) type ProxyEventSseSpec = crate::proxy_core::ProxyEventSseSpec;
#[cfg(test)]
pub(crate) type ProxyResponseBody = crate::proxy_core::ProxyResponseBody;
pub(crate) type ProxyTransportResponse = crate::proxy_core::ProxyTransportResponse;
pub(crate) type ProxyTransportResponseBody =
    crate::proxy_core::ProxyTransportResponseBody;
pub(crate) type GlobalProxyConfig = crate::proxy_core::GlobalProxyConfig;
pub(crate) type AppProxyConfig = crate::proxy_core::AppProxyConfig;
pub(crate) type ProviderHealth = crate::proxy_core::ProviderHealth;
pub(crate) type AllowResult = crate::proxy_core::AllowResult;
pub(crate) type CircuitBreakerConfig = crate::proxy_core::CircuitBreakerConfig;
pub(crate) type CircuitBreakerStats = crate::proxy_core::CircuitBreakerStats;
pub(crate) type CircuitState = crate::proxy_core::CircuitState;

pub(crate) mod circuit_breaker_log_codes {
    pub(crate) const OPEN_TO_HALF_OPEN: &str =
        crate::proxy_core::log_codes::cb::OPEN_TO_HALF_OPEN;
    pub(crate) const HALF_OPEN_TO_CLOSED: &str =
        crate::proxy_core::log_codes::cb::HALF_OPEN_TO_CLOSED;
    pub(crate) const HALF_OPEN_PROBE_FAILED: &str =
        crate::proxy_core::log_codes::cb::HALF_OPEN_PROBE_FAILED;
    pub(crate) const TRIGGERED_FAILURES: &str =
        crate::proxy_core::log_codes::cb::TRIGGERED_FAILURES;
    pub(crate) const TRIGGERED_ERROR_RATE: &str =
        crate::proxy_core::log_codes::cb::TRIGGERED_ERROR_RATE;
    pub(crate) const MANUAL_RESET: &str =
        crate::proxy_core::log_codes::cb::MANUAL_RESET;
}

pub(crate) type ProxyCoreAppKind = crate::proxy_core::AppKind;
pub(crate) type ProxyCoreInterfaceKind = crate::proxy_core::InterfaceKind;
pub(crate) type ChannelRequestValidationError =
    crate::proxy_core::ChannelRequestValidationError;
pub(crate) type ChannelRouteSource = crate::proxy_core::ChannelRouteSource;
pub(crate) type LegacyChannelModelProjection =
    crate::proxy_core::LegacyChannelModelProjection;
pub(crate) type LegacyChannelProjection = crate::proxy_core::LegacyChannelProjection;
pub(crate) type LegacyChannelProjectionInput =
    crate::proxy_core::LegacyChannelProjectionInput;
pub(crate) type LegacyModelRouteInput = crate::proxy_core::LegacyModelRouteInput;
pub(crate) type LegacyProviderProjectionInput =
    crate::proxy_core::LegacyProviderProjectionInput;
pub(crate) type ProxyChannelModelWriteRequest =
    crate::proxy_core::ProxyChannelModelWriteRequest;
pub(crate) type ProxyChannelPatchRequest = crate::proxy_core::ProxyChannelPatchRequest;
pub(crate) type ProxyChannelWriteRequest = crate::proxy_core::ProxyChannelWriteRequest;
pub(crate) type ProviderSelectionCandidate =
    crate::proxy_core::ProviderSelectionCandidate;
pub(crate) type ProviderSelectionFailure = crate::proxy_core::ProviderSelectionFailure;
pub(crate) type ProviderSelectionInput = crate::proxy_core::ProviderSelectionInput;
pub(crate) type ProxyCoreError = crate::proxy_core::ProxyCoreError;
pub(crate) type RouteResolveRequest = crate::proxy_core::RouteResolveRequest;
pub(crate) type RouteResolveResponse = crate::proxy_core::RouteResolveResponse;

pub(crate) const PROXY_EVENTS_CONNECTED_EVENT: &str =
    crate::proxy_core::PROXY_EVENTS_CONNECTED_EVENT;
pub(crate) const PROXY_EVENTS_LAGGED_EVENT: &str =
    crate::proxy_core::PROXY_EVENTS_LAGGED_EVENT;

pub(crate) fn build_proxy_events_connected_payload(buffer_size: usize) -> Value {
    crate::proxy_core::build_proxy_events_connected_payload(buffer_size)
}

pub(crate) fn build_proxy_events_lagged_payload(skipped: u64) -> Value {
    crate::proxy_core::build_proxy_events_lagged_payload(skipped)
}

pub(crate) fn proxy_event_envelope_to_sse_spec(
    event: &ProxyEventEnvelope,
) -> ProxyEventSseSpec {
    event.to_sse_spec()
}

pub(crate) fn circuit_breaker_config_from_app_config(
    config: Option<&AppProxyConfig>,
) -> CircuitBreakerConfig {
    crate::proxy_core::circuit_breaker_config_from_app_config(config)
}

pub(crate) fn circuit_failure_threshold_from_app_config(
    config: Option<&AppProxyConfig>,
    fallback: u32,
) -> u32 {
    crate::proxy_core::circuit_failure_threshold_from_app_config(config, fallback)
}

pub(crate) fn provider_circuit_key(app_type: &str, provider_id: &str) -> String {
    crate::proxy_core::provider_circuit_key(app_type, provider_id)
}

pub(crate) fn channel_circuit_key(app_type: &str, channel_id: &str) -> String {
    crate::proxy_core::channel_circuit_key(app_type, channel_id)
}

pub(crate) fn provider_circuit_key_prefix(app_type: &str) -> String {
    crate::proxy_core::provider_circuit_key_prefix(app_type)
}

pub(crate) fn channel_circuit_key_prefix(app_type: &str) -> String {
    crate::proxy_core::channel_circuit_key_prefix(app_type)
}

pub(crate) fn app_type_from_circuit_key(key: &str) -> &str {
    crate::proxy_core::app_type_from_circuit_key(key)
}

pub(crate) fn select_provider_ids(
    input: ProviderSelectionInput,
) -> Result<Vec<String>, ProviderSelectionFailure> {
    crate::proxy_core::select_provider_ids(input)
}

pub(crate) fn resolve_channel_route(
    request: RouteResolveRequest,
    channels: Vec<RouteResolveChannelInput>,
    source: ChannelRouteSource,
) -> Result<RouteResolveResponse, ProxyCoreError> {
    crate::proxy_core::resolve_channel_route(request, channels, source)
}

pub(crate) fn reject_unavailable_channel_ids<I, S>(
    response: &mut RouteResolveResponse,
    unavailable_channel_ids: I,
) where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    crate::proxy_core::reject_unavailable_channel_ids(response, unavailable_channel_ids);
}

pub(crate) fn stable_channel_id(
    app_type: &str,
    provider_id: &str,
    source_kind: &str,
    base_url: &str,
) -> String {
    crate::proxy_core::stable_channel_id(app_type, provider_id, source_kind, base_url)
}

pub(crate) fn legacy_channel_priority(
    provider_id: &str,
    in_failover_queue: bool,
    current_provider_id: Option<&str>,
) -> i64 {
    crate::proxy_core::legacy_channel_priority(
        provider_id,
        in_failover_queue,
        current_provider_id,
    )
}

pub(crate) fn infer_legacy_channel_interface(
    app: Option<&ProxyCoreAppKind>,
    provider: &LegacyProviderProjectionInput,
) -> ProxyCoreInterfaceKind {
    crate::proxy_core::infer_legacy_channel_interface(app, provider)
}

pub(crate) fn build_legacy_channel_projection(
    input: LegacyChannelProjectionInput,
) -> LegacyChannelProjection {
    crate::proxy_core::build_legacy_channel_projection(input)
}

pub(crate) fn normalize_required_channel_string(
    value: &str,
    field: &str,
) -> Result<String, ChannelRequestValidationError> {
    crate::proxy_core::normalize_required_channel_string(value, field)
}

pub(crate) fn normalize_optional_channel_string(value: String) -> Option<String> {
    crate::proxy_core::normalize_optional_channel_string(value)
}

pub(crate) fn normalize_channel_base_url(value: &str) -> String {
    crate::proxy_core::normalize_channel_base_url(value)
}

pub(crate) fn normalize_channel_groups(groups: Vec<String>) -> Vec<String> {
    crate::proxy_core::normalize_channel_groups(groups)
}

pub(crate) fn channel_object_or_default(value: Value) -> Value {
    crate::proxy_core::channel_object_or_default(value)
}

pub(crate) fn channel_array_or_default(value: Value) -> Value {
    crate::proxy_core::channel_array_or_default(value)
}

pub(crate) fn validate_proxy_channel_write_request_fields(
    request: &ProxyChannelWriteRequest,
) -> Result<(), ChannelRequestValidationError> {
    crate::proxy_core::validate_proxy_channel_write_request_fields(request)
}

pub(crate) fn validate_proxy_channel_model_write_request_fields(
    model: &ProxyChannelModelWriteRequest,
) -> Result<(), ChannelRequestValidationError> {
    crate::proxy_core::validate_proxy_channel_model_write_request_fields(model)
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
        ChannelSpec {
            id: self.id.clone(),
            provider_id: self.provider_id.clone(),
            app: AppKind::from(self.app_type.as_str()),
            name: self.name.clone(),
            status: ChannelStatus::from_storage(&self.status),
            endpoint: UpstreamEndpoint {
                base_url: self.base_url.clone(),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::from_storage(&self.interface_kind),
            auth_profile: self.auth_profile_ref.clone().map(AuthProfileRef::new),
            models: self
                .models
                .iter()
                .map(ProxyChannelModelRecord::to_proxy_core_model_route)
                .collect(),
            groups: self.groups.clone(),
            priority: self.priority,
            weight: self.weight,
            retry_policy: RetryPolicy {
                raw: object_or_empty(self.retry_policy.clone()),
            },
            health_policy: ChannelHealthPolicy {
                raw: object_or_empty(self.health_policy.clone()),
            },
            overrides: ChannelOverrides {
                headers: object_or_empty(self.header_overrides.clone()),
                params: object_or_empty(self.param_overrides.clone()),
                status_code_mapping: array_or_empty(self.status_code_mapping.clone()),
                model_mapping: Value::Object(Default::default()),
            },
            tags: self.tags.clone(),
            metadata: object_or_empty(self.metadata.clone()),
            source_ref: self.source_endpoint_url.clone(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        }
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
    crate::proxy_core::DEFAULT_CODEX_MODEL_CONTEXT_WINDOW
}

pub(crate) fn codex_settings_have_model_catalog_specs(settings: &Value) -> bool {
    crate::proxy_core::has_codex_model_catalog_specs(settings)
}

pub(crate) fn codex_model_catalog_from_settings(
    settings: &Value,
    default_context_window: u64,
    template: &Value,
) -> Option<Value> {
    crate::proxy_core::build_codex_model_catalog_from_settings(
        settings,
        default_context_window,
        template,
    )
}

pub(crate) fn simplify_codex_model_catalog(
    catalog_text: &str,
    default_context_window: u64,
) -> Option<Value> {
    crate::proxy_core::simplify_codex_model_catalog(catalog_text, default_context_window)
}

pub(crate) fn provider_model_catalog_from_settings(
    provider_id: &str,
    settings: Option<&Value>,
) -> ModelCatalog {
    crate::proxy_core::provider_model_catalog_from_settings(provider_id, settings)
}

pub(crate) fn client_model_catalog_from_raw(app: &AppKind, raw: Value) -> ModelCatalog {
    crate::proxy_core::client_model_catalog_from_raw(app.as_str(), raw)
}

pub(crate) fn route_plan_provider_ids(plan: &RoutePlan) -> Vec<String> {
    crate::proxy_core::route_plan_provider_ids(plan)
}

pub(crate) fn route_selection_for_forward_result(
    plan: &RoutePlan,
    selected_channel_id: Option<&str>,
    provider_id: &str,
) -> RouteSelection {
    crate::proxy_core::select_route_for_forward_result(plan, selected_channel_id, provider_id)
}

pub(crate) fn route_group_matches(groups: &[String], requested_group: &str) -> bool {
    crate::proxy_core::route_group_matches(groups, requested_group)
}

pub(crate) fn route_interfaces_compatible(
    requested: &InterfaceKind,
    channel: &InterfaceKind,
) -> bool {
    crate::proxy_core::interfaces_compatible(requested, channel)
}

pub(crate) fn route_selection_from_parts(
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

pub(crate) fn channel_route_candidate_from_selection(
    selection: &RouteSelection,
) -> ChannelRouteCandidate {
    crate::proxy_core::route_candidate_from_selection(
        selection,
        DEFAULT_ROUTE_GROUP,
        "proxy_core",
    )
}

pub(crate) fn resolved_channel_attempt_from_candidate(
    candidate: ChannelRouteCandidate,
) -> ResolvedChannelAttempt {
    crate::proxy_core::resolved_channel_attempt_from_candidate(candidate)
}

pub(crate) fn proxy_core_error_is_unavailable(
    error: &crate::proxy_core::ProxyCoreError,
) -> bool {
    matches!(error, crate::proxy_core::ProxyCoreError::Unavailable(_))
}

pub(crate) fn apply_channel_route_model_override(
    body: &mut Value,
    public_model: Option<&str>,
    upstream_model: Option<&str>,
) -> Option<String> {
    crate::proxy_core::apply_channel_route_model_override(body, public_model, upstream_model)
}

pub(crate) fn apply_channel_provider_overrides(
    app_type: &AppType,
    provider: &mut Provider,
    candidate: &ChannelRouteCandidate,
) {
    let plan =
        crate::proxy_core::channel_provider_override_plan(&AppKind::from(app_type), candidate);
    crate::proxy_core::apply_channel_provider_settings_overrides(
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
    crate::proxy_core::build_codex_tool_context_from_request(body)
}

pub(crate) fn normalize_codex_chat_error_body(body: &[u8]) -> CodexChatErrorNormalization {
    crate::proxy_core::normalize_codex_chat_error_body(body)
}

pub(crate) fn should_normalize_anthropic_tool_thinking_history(
    settings_config: &Value,
    body: &Value,
    api_format: &str,
) -> bool {
    crate::proxy_core::should_normalize_anthropic_tool_thinking_history(
        settings_config,
        body,
        api_format,
    )
}

pub(crate) fn normalize_anthropic_tool_thinking_history(body: &mut Value) -> bool {
    crate::proxy_core::normalize_anthropic_tool_thinking_history(body)
}

pub(crate) fn normalize_deepseek_thinking_disabled_strip_effort(
    body: &mut Value,
    settings_config: &Value,
) -> bool {
    crate::proxy_core::normalize_deepseek_thinking_disabled_strip_effort(body, settings_config)
}

pub(crate) fn inject_openai_stream_include_usage(body: &mut Value) {
    crate::proxy_core::inject_openai_stream_include_usage(body);
}

#[cfg(test)]
pub(crate) fn anthropic_tool_thinking_placeholder() -> &'static str {
    crate::proxy_core::ANTHROPIC_TOOL_THINKING_PLACEHOLDER
}

#[cfg(test)]
pub(crate) fn anthropic_redacted_thinking_placeholder() -> &'static str {
    crate::proxy_core::ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
}

pub(crate) struct ModelMappingProjection {
    pub(crate) body: Value,
    pub(crate) log_message: Option<String>,
}

pub(crate) fn apply_provider_model_mapping(
    body: Value,
    provider_settings: &Value,
) -> ModelMappingProjection {
    let mapping = crate::proxy_core::ModelMapping::from_settings_config(provider_settings);
    let (body, original_model, mapped_model) =
        crate::proxy_core::apply_model_mapping_to_body(body, &mapping);
    let log_message = crate::proxy_core::model_mapping_log_message(
        original_model.as_deref(),
        mapped_model.as_deref(),
    );

    ModelMappingProjection { body, log_message }
}

pub(crate) fn rewrite_codex_responses_endpoint_to_chat(
    endpoint: &str,
) -> (String, Option<String>) {
    crate::proxy_core::rewrite_codex_responses_endpoint_to_chat(endpoint).into_parts()
}

pub(crate) fn resolve_gemini_native_url(
    base_url: &str,
    endpoint: &str,
    is_full_url: bool,
) -> String {
    crate::proxy_core::resolve_gemini_native_url(base_url, endpoint, is_full_url)
}

pub(crate) fn claude_api_format_needs_transform(api_format: &str) -> bool {
    crate::proxy_core::claude_api_format_needs_transform(api_format)
}

pub(crate) fn anthropic_beta_header_value(existing_beta: Option<&str>) -> String {
    crate::proxy_core::anthropic_beta_header_value(existing_beta)
}

pub(crate) fn build_upstream_request_headers(
    input: UpstreamRequestHeadersInput<'_>,
) -> HeaderMap {
    crate::proxy_core::build_upstream_request_headers(input)
}

pub(crate) fn serialize_upstream_request_body(
    method: &http::Method,
    body: &Value,
) -> serde_json::Result<Vec<u8>> {
    crate::proxy_core::serialize_upstream_request_body(method, body)
}

pub(crate) fn resolve_upstream_request_transport_policy(
    needs_transform: bool,
    codex_responses_to_chat: bool,
    endpoint: &str,
    body: &Value,
    headers: &HeaderMap,
) -> UpstreamRequestTransportPolicy {
    crate::proxy_core::resolve_upstream_request_transport_policy(
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
    crate::proxy_core::is_streaming_upstream_request(endpoint, body, headers)
}

pub(crate) fn is_socks_proxy_url(upstream_proxy_url: Option<&str>) -> bool {
    crate::proxy_core::is_socks_proxy_url(upstream_proxy_url)
}

pub(crate) fn resolve_upstream_send_policy(
    input: UpstreamSendPolicyInput,
) -> UpstreamSendPolicy {
    crate::proxy_core::resolve_upstream_send_policy(input)
}

#[cfg(test)]
pub(crate) fn is_official_codex_client_user_agent(user_agent: &str) -> bool {
    crate::proxy_core::is_official_codex_client_user_agent(user_agent)
}

#[cfg(test)]
pub(crate) fn build_gemini_native_url(base_url: &str, endpoint: &str) -> String {
    crate::proxy_core::build_gemini_native_url(base_url, endpoint)
}

pub(crate) struct UsageRequestLogProjection {
    pub(crate) log: RequestLog,
    pub(crate) missing_pricing_model: Option<String>,
}

pub(crate) fn usage_record_pricing_model(
    record: &UsageRecord,
    pricing_model_source: &str,
) -> String {
    crate::proxy_core::resolve_usage_record_pricing_models(record, pricing_model_source)
        .pricing_model
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
        crate::proxy_core::resolve_usage_record_pricing_models(record, pricing_model_source);
    let usage = crate::proxy_core::token_usage_from_usage_record(record);
    let missing_pricing_model = (pricing.is_none()
        && record.tokens.has_billable_tokens()
        && !is_placeholder_pricing_model(&model_selection.pricing_model))
    .then(|| model_selection.pricing_model.clone());
    let cost = CostCalculator::try_calculate_for_app(&app_type, &usage, pricing, multiplier);

    UsageRequestLogProjection {
        log: RequestLog {
            request_id: crate::proxy_core::usage_record_request_id_with_fallback(
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
            is_streaming: record.is_streaming,
            cost_multiplier: multiplier.to_string(),
        },
        missing_pricing_model,
    }
}

const CLAUDE_ONE_M_MARKER_FOR_CLIENT: &str = "[1M]";

pub(crate) fn claude_takeover_client_model_for_upstream(
    takeover_model: &str,
    supports_one_m: bool,
    upstream_model: &str,
) -> String {
    let mut client_model = takeover_model.to_string();
    if supports_one_m && crate::proxy_core::has_one_m_suffix_for_upstream(upstream_model) {
        client_model.push_str(CLAUDE_ONE_M_MARKER_FOR_CLIENT);
    }
    client_model
}

pub(crate) fn claude_takeover_default_display_name(upstream_model: &str) -> String {
    crate::proxy_core::strip_one_m_suffix_for_upstream(upstream_model)
        .trim()
        .to_string()
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreModelRoute {
    fn to_proxy_core_model_route(&self) -> ModelRoute;
}

impl ToProxyCoreModelRoute for ProxyChannelModelRecord {
    fn to_proxy_core_model_route(&self) -> ModelRoute {
        ModelRoute {
            public_model: self.public_model.clone(),
            upstream_model: self.upstream_model.clone(),
            capabilities: ModelCapabilities {
                raw: object_or_empty(self.capabilities.clone()),
            },
            pricing_model: self.pricing_model.clone(),
            request_overrides: object_or_empty(self.request_overrides.clone()),
            response_overrides: object_or_empty(self.response_overrides.clone()),
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord;
}

impl ToProxyCoreChannelModelRecord for ProxyChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord {
        ChannelModelRecord::from_model_route(
            self.channel_id.clone(),
            self.to_proxy_core_model_route(),
        )
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
        ChannelRecord {
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
                .map(ProxyChannelModelRecord::to_proxy_core_channel_model_record)
                .collect(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        }
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

pub(crate) fn extract_proxy_session_id(
    headers: &HeaderMap,
    body: &Value,
    client_format: &str,
) -> SessionIdResult {
    crate::proxy_core::extract_session_id_with_generator(headers, body, client_format, || {
        Uuid::new_v4().to_string()
    })
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
    let mut labels = Vec::new();
    if provider.in_failover_queue {
        labels.push("failover".to_string());
    }
    if let Some(category) = provider.category.as_deref() {
        labels.push(category.to_string());
    }

    let meta = provider.meta.as_ref();
    let raw = json!({
        "websiteUrl": provider.website_url,
        "category": provider.category,
        "sortIndex": provider.sort_index,
        "notes": provider.notes,
        "icon": provider.icon,
        "iconColor": provider.icon_color,
        "inFailoverQueue": provider.in_failover_queue,
        "providerType": meta.and_then(|meta| meta.provider_type.clone()),
        "apiFormat": meta.and_then(|meta| meta.api_format.clone()),
        "authBinding": meta.and_then(|meta| meta.auth_binding.as_ref()).map(|binding| json!(binding)),
        "endpointAutoSelect": meta.and_then(|meta| meta.endpoint_auto_select),
        "customEndpointCount": meta.map(|meta| meta.custom_endpoints.len()).unwrap_or(0),
    });

    ProviderMetadata { labels, raw }
}

fn account_ref(provider: &Provider) -> Option<String> {
    provider.meta.as_ref().and_then(|meta| {
        meta.provider_type.as_deref().and_then(|provider_type| {
            meta.managed_account_id_for(provider_type)
                .map(|account_id| format!("{provider_type}:{account_id}"))
        })
    })
}

fn object_or_empty(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        Value::Object(Default::default())
    }
}

fn array_or_empty(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        Value::Array(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::ProxyChannelSourceKind;
    use crate::provider::{AuthBinding, AuthBindingSource, ProviderMeta};
    use crate::proxy_core::{
        GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX, ProviderKind, ProxyCoreError, SessionIdSource,
        UpstreamTransportKind,
    };

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

            route_selection_from_parts(
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
    fn route_predicate_adapter_projects_group_and_interface_rules() {
        assert!(route_group_matches(&[], "default"));
        assert!(route_group_matches(&["paid".to_string()], "paid"));
        assert!(!route_group_matches(&["paid".to_string()], "default"));
        assert!(route_interfaces_compatible(
            &InterfaceKind::OpenAiResponses,
            &InterfaceKind::OpenAiResponses
        ));
        assert!(!route_interfaces_compatible(
            &InterfaceKind::GeminiNative,
            &InterfaceKind::OpenAiResponses
        ));
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
            tokens: crate::proxy_core::UsageTokens {
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
