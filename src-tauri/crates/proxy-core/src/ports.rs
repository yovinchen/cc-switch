use super::cache_injector::CacheInjectionConfig;
use super::domain::{
    AppKind, AuthProfileRef, ChannelAttemptResult, ChannelQuery, ChannelSpec, InterfaceKind,
    ModelRoute, ProviderSpec, ProxyRequest, ProxyResult, RoutePlan, RoutePolicy, RouteRequest,
    UsageRecord, DEFAULT_ROUTE_GROUP,
};
use super::error::{ProxyCoreError, ProxyCoreResult};
use super::thinking_budget_rectifier::ThinkingBudgetRectifierConfig;
use super::thinking_optimizer::ThinkingOptimizerConfig;
use super::thinking_rectifier::ThinkingSignatureRectifierConfig;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const DEFAULT_PROXY_LISTEN_ADDRESS: &str = "127.0.0.1";
pub const DEFAULT_PROXY_LISTEN_PORT: u16 = 15721;

pub trait ProxyServices: Send + Sync {
    fn config(&self) -> &(dyn ProxyConfigSource + Send + Sync);
    fn providers(&self) -> &(dyn ProviderSource + Send + Sync);
    fn channels(&self) -> &(dyn ChannelSource + Send + Sync);
    fn route_policies(&self) -> &(dyn RoutePolicySource + Send + Sync);
    fn route_resolver(&self) -> &(dyn RouteResolver + Send + Sync);
    fn health_store(&self) -> &(dyn ChannelHealthStore + Send + Sync);
    fn auth_provider(&self) -> &(dyn AuthProvider + Send + Sync);
    fn model_catalog(&self) -> &(dyn ModelCatalogProvider + Send + Sync);
    fn usage_sink(&self) -> &(dyn UsageSink + Send + Sync);
    fn event_sink(&self) -> &(dyn ProxyEventSink + Send + Sync);
    fn forward_pipeline(&self) -> &(dyn ForwardPipeline + Send + Sync);
}

pub trait ProxyConfigSource: Send + Sync {
    fn load_global<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyGlobalConfig>>;

    fn load_app<'a>(&'a self, app: &'a AppKind) -> BoxFuture<'a, ProxyCoreResult<ProxyAppConfig>>;

    fn load_runtime<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeConfig>>;
}

pub trait ProviderSource: Send + Sync {
    fn list_providers<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ProviderSpec>>>;

    fn get_provider<'a>(
        &'a self,
        app: &'a AppKind,
        provider_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ProviderSpec>>>;

    fn current_provider_id<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<String>>> {
        Box::pin(async { Ok(None) })
    }

    fn route_candidate_provider_ids<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<String>>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

pub trait ChannelSource: Send + Sync {
    fn list_channels<'a>(
        &'a self,
        query: ChannelQuery<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelSpec>>>;

    fn get_channel<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelSpec>>>;

    fn list_channel_records<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<(ChannelRouteSource, Vec<ChannelRecord>)>> {
        Box::pin(async {
            Err(ProxyCoreError::Unavailable(
                "management channel list source is not configured".to_string(),
            ))
        })
    }

    fn list_materialized_channel_records<'a>(
        &'a self,
        _app: Option<&'a AppKind>,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelRecord>>> {
        Box::pin(async {
            Err(ProxyCoreError::Unavailable(
                "management materialized channel list source is not configured".to_string(),
            ))
        })
    }
}

pub trait RoutePolicySource: Send + Sync {
    fn load_policy<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<RoutePolicy>>>;
}

pub trait RouteResolver: Send + Sync {
    fn resolve<'a>(
        &'a self,
        request: RouteRequest<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<RoutePlan>>;

    fn resolve_management_route<'a>(
        &'a self,
        _request: RouteResolveRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<RouteResolveResponse>> {
        Box::pin(async {
            Err(ProxyCoreError::Unavailable(
                "management route resolver is not configured".to_string(),
            ))
        })
    }
}

pub trait ChannelHealthStore: Send + Sync {
    fn record_attempt<'a>(
        &'a self,
        result: ChannelAttemptResult,
    ) -> BoxFuture<'a, ProxyCoreResult<()>>;

    fn reset_channel<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelHealthReset>>;
}

pub trait AuthProvider: Send + Sync {
    fn resolve_auth<'a>(
        &'a self,
        auth_profile: Option<&'a AuthProfileRef>,
        request: &'a ProxyRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>>;
}

pub trait ModelCatalogProvider: Send + Sync {
    fn load_catalog<'a>(
        &'a self,
        app: &'a AppKind,
        provider_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>>;

    fn load_client_catalog<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>>;
}

pub trait UsageSink: Send + Sync {
    fn record_usage<'a>(&'a self, record: UsageRecord) -> BoxFuture<'a, ProxyCoreResult<()>>;
}

pub trait ProxyEventSink: Send + Sync {
    fn emit_event<'a>(&'a self, event: ProxyCoreEvent) -> BoxFuture<'a, ProxyCoreResult<()>>;
}

pub trait ForwardPipeline: Send + Sync {
    fn forward<'a>(
        &'a self,
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>>;
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyGlobalConfig {
    pub bind_host: Option<String>,
    pub bind_port: Option<u16>,
    pub request_timeout_ms: Option<u64>,
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyAppConfig {
    pub app: Option<AppKind>,
    pub enabled: bool,
    pub default_group: Option<String>,
    #[serde(default)]
    pub rectifier: RectifierConfigSpec,
    #[serde(default)]
    pub optimizer: OptimizerConfigSpec,
    #[serde(default)]
    pub copilot_optimizer: CopilotOptimizerConfigSpec,
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeConfig {
    pub privacy_filter_enabled: bool,
    pub route_events_enabled: bool,
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RectifierConfigSpec {
    pub enabled: bool,
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerConfigSpec {
    pub enabled: bool,
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotOptimizerConfigSpec {
    pub enabled: bool,
    #[serde(default)]
    pub raw: Value,
}

pub fn rectifier_config_spec_from_config(config: RectifierConfig) -> RectifierConfigSpec {
    RectifierConfigSpec {
        enabled: config.enabled,
        raw: serde_json::to_value(config).unwrap_or_else(|_| serde_json::json!({})),
    }
}

pub fn optimizer_config_spec_from_config(config: OptimizerConfig) -> OptimizerConfigSpec {
    OptimizerConfigSpec {
        enabled: config.enabled,
        raw: serde_json::to_value(config).unwrap_or_else(|_| serde_json::json!({})),
    }
}

pub fn copilot_optimizer_config_spec_from_config(
    config: CopilotOptimizerConfig,
) -> CopilotOptimizerConfigSpec {
    CopilotOptimizerConfigSpec {
        enabled: config.enabled,
        raw: serde_json::to_value(config).unwrap_or_else(|_| serde_json::json!({})),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RectifierConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub request_thinking_signature: bool,
    #[serde(default = "default_true")]
    pub request_thinking_budget: bool,
    #[serde(default = "default_true")]
    pub request_media_fallback: bool,
    #[serde(default = "default_true")]
    pub request_media_heuristic: bool,
}

impl Default for RectifierConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            request_thinking_signature: true,
            request_thinking_budget: true,
            request_media_fallback: true,
            request_media_heuristic: true,
        }
    }
}

impl RectifierConfig {
    pub fn thinking_signature_core_config(&self) -> ThinkingSignatureRectifierConfig {
        ThinkingSignatureRectifierConfig {
            enabled: self.enabled,
            request_thinking_signature: self.request_thinking_signature,
        }
    }

    pub fn thinking_budget_core_config(&self) -> ThinkingBudgetRectifierConfig {
        ThinkingBudgetRectifierConfig {
            enabled: self.enabled,
            request_thinking_budget: self.request_thinking_budget,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub thinking_optimizer: bool,
    #[serde(default = "default_true")]
    pub cache_injection: bool,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl: String,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            thinking_optimizer: true,
            cache_injection: true,
            cache_ttl: "1h".to_string(),
        }
    }
}

impl OptimizerConfig {
    pub fn thinking_optimizer_core_config(&self) -> ThinkingOptimizerConfig {
        ThinkingOptimizerConfig {
            enabled: self.thinking_optimizer,
        }
    }

    pub fn cache_injection_core_config(&self) -> CacheInjectionConfig {
        CacheInjectionConfig {
            enabled: self.cache_injection,
            ttl: self.cache_ttl.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotOptimizerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub request_classification: bool,
    #[serde(default = "default_true")]
    pub tool_result_merging: bool,
    #[serde(default = "default_true")]
    pub compact_detection: bool,
    #[serde(default = "default_true")]
    pub deterministic_request_id: bool,
    #[serde(default = "default_true")]
    pub subagent_detection: bool,
    #[serde(default = "default_true")]
    pub warmup_downgrade: bool,
    #[serde(default = "default_warmup_model")]
    pub warmup_model: String,
    #[serde(default = "default_true")]
    pub strip_thinking: bool,
}

impl Default for CopilotOptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            request_classification: true,
            tool_result_merging: true,
            compact_detection: true,
            deterministic_request_id: true,
            subagent_detection: true,
            warmup_downgrade: true,
            warmup_model: "gpt-5-mini".to_string(),
            strip_thinking: true,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_cache_ttl() -> String {
    "1h".to_string()
}

fn default_warmup_model() -> String {
    "gpt-5-mini".to_string()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthInfo {
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_ref: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

pub fn auth_info_from_profile_ref(
    auth_profile: Option<&AuthProfileRef>,
    source: impl Into<String>,
) -> AuthInfo {
    AuthInfo {
        headers: Vec::new(),
        account_ref: auth_profile.map(|value| value.0.clone()),
        metadata: serde_json::json!({ "source": source.into() }),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub provider_id: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientModelCatalogResponse {
    pub raw: Value,
}

impl ClientModelCatalogResponse {
    pub fn from_catalog(catalog: ModelCatalog) -> Self {
        Self { raw: catalog.raw }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProxyStatusResponse<T> {
    pub status: T,
}

impl<T> ProxyStatusResponse<T> {
    pub fn new(status: T) -> Self {
        Self { status }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProxyRuntimeStatus {
    pub running: bool,
    pub address: String,
    pub port: u16,
    pub active_connections: usize,
    pub total_requests: u64,
    pub success_requests: u64,
    pub failed_requests: u64,
    pub success_rate: f32,
    pub uptime_seconds: u64,
    pub current_provider: Option<String>,
    pub current_provider_id: Option<String>,
    pub last_request_at: Option<String>,
    pub last_error: Option<String>,
    pub failover_count: u64,
    #[serde(default)]
    pub active_targets: Vec<CurrentRouteTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub listen_address: String,
    pub listen_port: u16,
    pub max_retries: u8,
    pub request_timeout: u64,
    pub enable_logging: bool,
    #[serde(default)]
    pub live_takeover_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub management_auth_token: Option<String>,
    #[serde(default = "default_streaming_first_byte_timeout")]
    pub streaming_first_byte_timeout: u64,
    #[serde(default = "default_streaming_idle_timeout")]
    pub streaming_idle_timeout: u64,
    #[serde(default = "default_non_streaming_timeout")]
    pub non_streaming_timeout: u64,
}

fn default_streaming_first_byte_timeout() -> u64 {
    60
}

fn default_streaming_idle_timeout() -> u64 {
    120
}

fn default_non_streaming_timeout() -> u64 {
    600
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            listen_address: DEFAULT_PROXY_LISTEN_ADDRESS.to_string(),
            listen_port: DEFAULT_PROXY_LISTEN_PORT,
            max_retries: 3,
            request_timeout: 600,
            enable_logging: true,
            live_takeover_active: false,
            management_auth_token: None,
            streaming_first_byte_timeout: 60,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalProxyConfig {
    pub proxy_enabled: bool,
    pub listen_address: String,
    pub listen_port: u16,
    pub enable_logging: bool,
}

impl Default for GlobalProxyConfig {
    fn default() -> Self {
        Self {
            proxy_enabled: false,
            listen_address: DEFAULT_PROXY_LISTEN_ADDRESS.to_string(),
            listen_port: DEFAULT_PROXY_LISTEN_PORT,
            enable_logging: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppProxyConfig {
    pub app_type: String,
    pub enabled: bool,
    pub auto_failover_enabled: bool,
    pub max_retries: u32,
    pub streaming_first_byte_timeout: u32,
    pub streaming_idle_timeout: u32,
    pub non_streaming_timeout: u32,
    pub circuit_failure_threshold: u32,
    pub circuit_success_threshold: u32,
    pub circuit_timeout_seconds: u32,
    pub circuit_error_rate_threshold: f64,
    pub circuit_min_requests: u32,
}

pub fn app_proxy_config_defaults_for_app(app_type: &str) -> AppProxyConfig {
    let (
        max_retries,
        streaming_first_byte_timeout,
        streaming_idle_timeout,
        circuit_failure_threshold,
        circuit_success_threshold,
        circuit_timeout_seconds,
        circuit_error_rate_threshold,
        circuit_min_requests,
    ) = match app_type {
        "claude" => (6, 90, 180, 8, 3, 90, 0.7, 15),
        "codex" => (3, 60, 120, 4, 2, 60, 0.6, 10),
        "gemini" => (5, 60, 120, 4, 2, 60, 0.6, 10),
        _ => (3, 60, 120, 4, 2, 60, 0.6, 10),
    };

    AppProxyConfig {
        app_type: app_type.to_string(),
        enabled: false,
        auto_failover_enabled: false,
        max_retries,
        streaming_first_byte_timeout,
        streaming_idle_timeout,
        non_streaming_timeout: 600,
        circuit_failure_threshold,
        circuit_success_threshold,
        circuit_timeout_seconds,
        circuit_error_rate_threshold,
        circuit_min_requests,
    }
}

pub fn app_proxy_config_raw(
    config: AppProxyConfig,
    current_provider_id: Option<String>,
) -> Value {
    let mut raw = serde_json::to_value(config).unwrap_or_else(|_| serde_json::json!({}));
    if let Value::Object(object) = &mut raw {
        object.insert(
            "currentProviderId".to_string(),
            current_provider_id
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
    }
    raw
}

pub fn proxy_global_config_from_global_config(config: GlobalProxyConfig) -> ProxyGlobalConfig {
    ProxyGlobalConfig {
        bind_host: Some(config.listen_address.clone()),
        bind_port: Some(config.listen_port),
        request_timeout_ms: None,
        raw: serde_json::to_value(config).unwrap_or_else(|_| serde_json::json!({})),
    }
}

pub fn proxy_app_config_from_parts(
    app: AppKind,
    config: AppProxyConfig,
    current_provider_id: Option<String>,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ProxyAppConfig {
    let enabled = config.enabled;
    ProxyAppConfig {
        app: Some(app),
        enabled,
        default_group: Some(DEFAULT_ROUTE_GROUP.to_string()),
        rectifier: rectifier_config_spec_from_config(rectifier),
        optimizer: optimizer_config_spec_from_config(optimizer),
        copilot_optimizer: copilot_optimizer_config_spec_from_config(copilot_optimizer),
        raw: app_proxy_config_raw(config, current_provider_id),
    }
}

pub fn proxy_runtime_config_from_proxy_config(
    config: ProxyConfig,
    privacy_filter_enabled: bool,
) -> ProxyRuntimeConfig {
    let route_events_enabled = config.enable_logging;
    ProxyRuntimeConfig {
        privacy_filter_enabled,
        route_events_enabled,
        raw: serde_json::to_value(config).unwrap_or_else(|_| serde_json::json!({})),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyServerInfo {
    pub address: String,
    pub port: u16,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProxyTakeoverStatus {
    pub claude: bool,
    pub codex: bool,
    pub gemini: bool,
    pub opencode: bool,
    pub openclaw: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub app_type: String,
    pub is_healthy: bool,
    pub consecutive_failures: u32,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHealthUpdateInput {
    pub current_consecutive_failures: u32,
    pub success: bool,
    pub error_msg: Option<String>,
    pub failure_threshold: u32,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHealthUpdate {
    pub is_healthy: bool,
    pub consecutive_failures: u32,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub last_error: Option<String>,
}

pub fn provider_health_update_from_input(input: ProviderHealthUpdateInput) -> ProviderHealthUpdate {
    if input.success {
        return ProviderHealthUpdate {
            is_healthy: true,
            consecutive_failures: 0,
            last_success_at: Some(input.timestamp),
            last_failure_at: None,
            last_error: input.error_msg,
        };
    }

    let consecutive_failures = input.current_consecutive_failures.saturating_add(1);

    ProviderHealthUpdate {
        is_healthy: consecutive_failures < input.failure_threshold,
        consecutive_failures,
        last_success_at: None,
        last_failure_at: Some(input.timestamp),
        last_error: input.error_msg,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelHealthReset {
    pub channel_id: String,
    pub app: AppKind,
}

pub fn channel_health_reset_from_parts(
    channel_id: impl Into<String>,
    app_type: &str,
) -> ChannelHealthReset {
    ChannelHealthReset {
        channel_id: channel_id.into(),
        app: AppKind::from(app_type),
    }
}

pub const CHANNEL_HEALTH_UNKNOWN_STATUS: &str = "unknown";
pub const DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelHealthUpdateInput {
    pub current_consecutive_failures: u32,
    pub success: bool,
    pub error_msg: Option<String>,
    pub failure_threshold: u32,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelHealthUpdate {
    pub status: &'static str,
    pub consecutive_failures: u32,
    pub last_success_at: Option<i64>,
    pub last_failure_at: Option<i64>,
    pub disabled_reason: Option<String>,
}

pub fn channel_health_update_from_input(input: ChannelHealthUpdateInput) -> ChannelHealthUpdate {
    if input.success {
        return ChannelHealthUpdate {
            status: "healthy",
            consecutive_failures: 0,
            last_success_at: Some(input.timestamp_ms),
            last_failure_at: None,
            disabled_reason: None,
        };
    }

    let consecutive_failures = input.current_consecutive_failures.saturating_add(1);
    let status = if consecutive_failures >= input.failure_threshold {
        "unhealthy"
    } else {
        "degraded"
    };

    ChannelHealthUpdate {
        status,
        consecutive_failures,
        last_success_at: None,
        last_failure_at: Some(input.timestamp_ms),
        disabled_reason: input.error_msg,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckResponse {
    pub status: String,
    pub timestamp: String,
}

impl HealthCheckResponse {
    pub fn healthy(timestamp: impl Into<String>) -> Self {
        Self {
            status: "healthy".to_string(),
            timestamp: timestamp.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelHealthResetResponse {
    pub channel_id: String,
    pub app_type: String,
    pub reset: bool,
}

impl ChannelHealthResetResponse {
    pub fn from_reset(reset: ChannelHealthReset) -> Self {
        Self {
            channel_id: reset.channel_id,
            app_type: reset.app.as_str().to_string(),
            reset: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDeleteResponse {
    pub channel_id: String,
    pub deleted: bool,
}

impl ChannelDeleteResponse {
    pub fn new(channel_id: impl Into<String>, deleted: bool) -> Self {
        Self {
            channel_id: channel_id.into(),
            deleted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyChannelWriteRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub provider_id: String,
    pub app_type: String,
    pub name: String,
    #[serde(default = "default_channel_status")]
    pub status: String,
    pub base_url: String,
    pub interface_kind: String,
    #[serde(default)]
    pub auth_profile_ref: Option<String>,
    #[serde(default = "default_channel_groups")]
    pub groups: Vec<String>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default = "default_channel_weight")]
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
    pub models: Vec<ProxyChannelModelWriteRequest>,
}

impl Default for ProxyChannelWriteRequest {
    fn default() -> Self {
        Self {
            id: None,
            provider_id: String::new(),
            app_type: String::new(),
            name: String::new(),
            status: default_channel_status(),
            base_url: String::new(),
            interface_kind: String::new(),
            auth_profile_ref: None,
            groups: default_channel_groups(),
            priority: 0,
            weight: default_channel_weight(),
            retry_policy: empty_object_value(),
            health_policy: empty_object_value(),
            header_overrides: empty_object_value(),
            param_overrides: empty_object_value(),
            status_code_mapping: empty_array_value(),
            tags: Vec::new(),
            metadata: empty_object_value(),
            models: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyChannelPatchRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub interface_kind: Option<String>,
    #[serde(default)]
    pub auth_profile_ref: Option<String>,
    #[serde(default)]
    pub groups: Option<Vec<String>>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub weight: Option<u32>,
    #[serde(default)]
    pub retry_policy: Option<Value>,
    #[serde(default)]
    pub health_policy: Option<Value>,
    #[serde(default)]
    pub header_overrides: Option<Value>,
    #[serde(default)]
    pub param_overrides: Option<Value>,
    #[serde(default)]
    pub status_code_mapping: Option<Value>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyChannelKeyWriteRequest {
    pub key_value: String,
    #[serde(default = "default_channel_status")]
    pub status: String,
    #[serde(default)]
    pub priority: i64,
    #[serde(default = "default_channel_weight")]
    pub weight: u32,
}

impl Default for ProxyChannelKeyWriteRequest {
    fn default() -> Self {
        Self {
            key_value: String::new(),
            status: default_channel_status(),
            priority: 0,
            weight: default_channel_weight(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyChannelKeyPatchRequest {
    #[serde(default)]
    pub key_value: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub weight: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyChannelModelWriteRequest {
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyChannelModelsReplaceRequest {
    #[serde(default)]
    pub models: Vec<ProxyChannelModelWriteRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyChannelTestRequest {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub interface_kind: Option<String>,
}

impl ProxyChannelTestRequest {
    pub fn requested_model(&self) -> Option<&str> {
        self.model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn requested_interface(&self) -> Option<&str> {
        self.interface_kind
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelTestInput {
    pub channel_id: String,
    pub provider_id: String,
    pub app_type: String,
    pub channel_name: String,
    pub base_url: String,
    pub interface_kind: String,
    pub model: Option<String>,
    pub model_available: Option<bool>,
    pub success: bool,
    pub status: String,
    pub message: String,
    pub latency_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub tested_at: i64,
    pub retry_count: u32,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelTestResponse {
    pub channel_id: String,
    pub provider_id: String,
    pub app_type: String,
    pub channel_name: String,
    pub base_url: String,
    pub interface_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_available: Option<bool>,
    pub success: bool,
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    pub tested_at: i64,
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

impl ChannelTestResponse {
    pub fn from_input(input: ChannelTestInput) -> Self {
        Self {
            channel_id: input.channel_id,
            provider_id: input.provider_id,
            app_type: input.app_type,
            channel_name: input.channel_name,
            base_url: input.base_url,
            interface_kind: input.interface_kind,
            model: input.model,
            model_available: input.model_available,
            success: input.success,
            status: input.status,
            message: input.message,
            latency_ms: input.latency_ms,
            http_status: input.http_status,
            tested_at: input.tested_at,
            retry_count: input.retry_count,
            failure_reason: input.failure_reason,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelTestPlan {
    Probe(ChannelTestContext),
    Failure(ChannelTestResponse),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelTestContext {
    pub channel_id: String,
    pub provider_id: String,
    pub app_type: String,
    pub channel_name: String,
    pub base_url: String,
    pub interface_kind: String,
    pub model: Option<String>,
    pub model_available: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelTestProbeRequest {
    pub channel_id: String,
    pub provider_id: String,
    pub app_type: String,
    pub base_url: String,
}

impl ChannelTestProbeRequest {
    pub fn provider_not_found_message(&self) -> String {
        format!(
            "provider not found for channel {}: {}",
            self.channel_id, self.provider_id
        )
    }
}

impl ChannelTestContext {
    pub fn from_channel(channel: &ChannelSpec, request: &ProxyChannelTestRequest) -> Self {
        let model = request.requested_model().map(str::to_string);
        let model_available = model
            .as_deref()
            .map(|requested_model| channel_test_model_matches(channel, requested_model));

        Self {
            channel_id: channel.id.clone(),
            provider_id: channel.provider_id.clone(),
            app_type: channel.app.as_str().to_string(),
            channel_name: channel.name.clone(),
            base_url: channel.endpoint.base_url.clone(),
            interface_kind: channel.interface.as_str().to_string(),
            model,
            model_available,
        }
    }

    pub fn probe_request(&self) -> ChannelTestProbeRequest {
        ChannelTestProbeRequest {
            channel_id: self.channel_id.clone(),
            provider_id: self.provider_id.clone(),
            app_type: self.app_type.clone(),
            base_url: self.base_url.clone(),
        }
    }

    pub fn failure_response(
        &self,
        message: impl Into<String>,
        tested_at: i64,
    ) -> ChannelTestResponse {
        let message = message.into();
        ChannelTestResponse::from_input(ChannelTestInput {
            channel_id: self.channel_id.clone(),
            provider_id: self.provider_id.clone(),
            app_type: self.app_type.clone(),
            channel_name: self.channel_name.clone(),
            base_url: self.base_url.clone(),
            interface_kind: self.interface_kind.clone(),
            model: self.model.clone(),
            model_available: self.model_available,
            success: false,
            status: ChannelReachabilityStatus::Failed.as_str().to_string(),
            message: message.clone(),
            latency_ms: None,
            http_status: None,
            tested_at,
            retry_count: 0,
            failure_reason: Some(message),
        })
    }

    pub fn reachability_response(
        &self,
        result: ChannelReachabilityResult,
    ) -> ChannelTestResponse {
        let success = result.success && self.model_available != Some(false);
        let failure_reason = (!success).then(|| result.message.clone());
        ChannelTestResponse::from_input(ChannelTestInput {
            channel_id: self.channel_id.clone(),
            provider_id: self.provider_id.clone(),
            app_type: self.app_type.clone(),
            channel_name: self.channel_name.clone(),
            base_url: self.base_url.clone(),
            interface_kind: self.interface_kind.clone(),
            model: self.model.clone(),
            model_available: self.model_available,
            success,
            status: result.status,
            message: result.message,
            latency_ms: result.latency_ms,
            http_status: result.http_status,
            tested_at: result.tested_at,
            retry_count: result.retry_count,
            failure_reason,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelReachabilityResult {
    pub success: bool,
    pub status: String,
    pub message: String,
    pub latency_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub tested_at: i64,
    pub retry_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelReachabilityStatus {
    Operational,
    Degraded,
    Failed,
}

impl ChannelReachabilityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Operational => "operational",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
        }
    }
}

pub fn channel_reachability_status_from_latency(
    latency_ms: u64,
    degraded_threshold_ms: u64,
) -> ChannelReachabilityStatus {
    if latency_ms <= degraded_threshold_ms {
        ChannelReachabilityStatus::Operational
    } else {
        ChannelReachabilityStatus::Degraded
    }
}

pub fn should_retry_channel_reachability_failure(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("timeout") || lower.contains("abort") || lower.contains("timed out")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamCheckConfig {
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub degraded_threshold_ms: u64,
}

impl Default for StreamCheckConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 8,
            max_retries: 1,
            degraded_threshold_ms: 6000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamCheckResult {
    pub status: ChannelReachabilityStatus,
    pub success: bool,
    pub message: String,
    pub response_time_ms: Option<u64>,
    pub http_status: Option<u16>,
    /// Preserved for the historical stream_check_logs schema. Reachability
    /// probes do not exercise a model, so host probes usually set this to "".
    pub model_used: String,
    pub tested_at: i64,
    pub retry_count: u32,
    /// Fine-grained error category retained for response compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelReachabilityInput {
    pub success: bool,
    pub status: ChannelReachabilityStatus,
    pub message: String,
    pub latency_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub tested_at: i64,
    pub retry_count: u32,
}

impl ChannelReachabilityResult {
    pub fn from_input(input: ChannelReachabilityInput) -> Self {
        Self {
            success: input.success,
            status: input.status.as_str().to_string(),
            message: input.message,
            latency_ms: input.latency_ms,
            http_status: input.http_status,
            tested_at: input.tested_at,
            retry_count: input.retry_count,
        }
    }
}

pub fn channel_reachability_result_from_stream_check_result(
    result: StreamCheckResult,
) -> ChannelReachabilityResult {
    ChannelReachabilityResult::from_input(ChannelReachabilityInput {
        success: result.success,
        status: result.status,
        message: result.message,
        latency_ms: result.response_time_ms,
        http_status: result.http_status,
        tested_at: result.tested_at,
        retry_count: result.retry_count,
    })
}

pub fn plan_channel_test(
    channel: &ChannelSpec,
    request: &ProxyChannelTestRequest,
    tested_at: i64,
) -> ChannelTestPlan {
    let context = ChannelTestContext::from_channel(channel, request);

    if let Some(requested_interface) = request.requested_interface() {
        let requested = InterfaceKind::from_storage(requested_interface);
        let actual = context.interface_kind.as_str();
        if requested.as_str() != actual {
            return ChannelTestPlan::Failure(context.failure_response(
                format!(
                    "interface not available on channel: requested {}, actual {}",
                    requested.as_str(),
                    actual
                ),
                tested_at,
            ));
        }
    }

    if context.model_available == Some(false) {
        return ChannelTestPlan::Failure(context.failure_response(
            format!(
                "model not mapped on channel: {}",
                context.model.as_deref().unwrap_or_default()
            ),
            tested_at,
        ));
    }

    ChannelTestPlan::Probe(context)
}

fn channel_test_model_matches(channel: &ChannelSpec, requested_model: &str) -> bool {
    channel.models.iter().any(|model| {
        model.public_model == requested_model || model.upstream_model == requested_model
    })
}

fn default_channel_status() -> String {
    "enabled".to_string()
}

fn default_channel_groups() -> Vec<String> {
    vec![DEFAULT_ROUTE_GROUP.to_string()]
}

fn default_channel_weight() -> u32 {
    100
}

fn empty_object_value() -> Value {
    Value::Object(Map::new())
}

fn empty_array_value() -> Value {
    Value::Array(Vec::new())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSummaryInput {
    pub app_type: String,
    pub enabled: bool,
    pub auto_failover_enabled: bool,
    pub provider_count: usize,
    pub channel_count: usize,
}

impl AppSummaryInput {
    pub fn new(
        app_type: impl Into<String>,
        enabled: bool,
        auto_failover_enabled: bool,
        provider_count: usize,
        channel_count: usize,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            enabled,
            auto_failover_enabled,
            provider_count,
            channel_count,
        }
    }

    pub fn from_specs(
        app_type: impl Into<String>,
        enabled: bool,
        auto_failover_enabled: bool,
        providers: impl IntoIterator<Item = ProviderSpec>,
        channels: impl IntoIterator<Item = ChannelSpec>,
    ) -> Self {
        Self::new(
            app_type,
            enabled,
            auto_failover_enabled,
            providers.into_iter().count(),
            channels.into_iter().count(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSummary {
    pub app_type: String,
    pub enabled: bool,
    pub auto_failover_enabled: bool,
    pub provider_count: usize,
    pub channel_count: usize,
}

impl AppSummary {
    pub fn new(
        app_type: impl Into<String>,
        enabled: bool,
        auto_failover_enabled: bool,
        provider_count: usize,
        channel_count: usize,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            enabled,
            auto_failover_enabled,
            provider_count,
            channel_count,
        }
    }

    pub fn from_input(input: AppSummaryInput) -> Self {
        Self::new(
            input.app_type,
            input.enabled,
            input.auto_failover_enabled,
            input.provider_count,
            input.channel_count,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppListResponse {
    pub apps: Vec<AppSummary>,
}

impl AppListResponse {
    pub fn new(apps: Vec<AppSummary>) -> Self {
        Self { apps }
    }

    pub fn from_app_inputs(inputs: impl IntoIterator<Item = AppSummaryInput>) -> Self {
        Self::new(inputs.into_iter().map(AppSummary::from_input).collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub sort_index: Option<usize>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub provider_type: Option<String>,
    pub current: bool,
    pub in_failover_queue: bool,
    pub route_candidate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummaryInput {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub sort_index: Option<usize>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub provider_type: Option<String>,
}

impl ProviderSummaryInput {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        category: Option<String>,
        sort_index: Option<usize>,
        icon: Option<String>,
        icon_color: Option<String>,
        provider_type: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            category,
            sort_index,
            icon,
            icon_color,
            provider_type,
        }
    }

    pub fn from_provider_spec(spec: ProviderSpec) -> Self {
        let raw = spec.metadata.raw.as_object();
        Self::new(
            spec.id,
            spec.name,
            string_value(raw, "category"),
            usize_value(raw, "sortIndex"),
            string_value(raw, "icon"),
            string_value(raw, "iconColor"),
            string_value(raw, "providerType"),
        )
    }
}

impl ProviderSummary {
    pub fn from_input(
        input: ProviderSummaryInput,
        current_provider: Option<&str>,
        failover_provider_ids: &[String],
        route_candidate_ids: &[String],
    ) -> Self {
        let current = current_provider == Some(input.id.as_str());
        let in_failover_queue = contains_provider_id(failover_provider_ids, &input.id);
        let route_candidate = contains_provider_id(route_candidate_ids, &input.id);

        Self {
            id: input.id,
            name: input.name,
            category: input.category,
            sort_index: input.sort_index,
            icon: input.icon,
            icon_color: input.icon_color,
            provider_type: input.provider_type,
            current,
            in_failover_queue,
            route_candidate,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListResponse {
    pub app_type: String,
    pub providers: Vec<ProviderSummary>,
}

impl ProviderListResponse {
    pub fn new(app_type: impl Into<String>, providers: Vec<ProviderSummary>) -> Self {
        Self {
            app_type: app_type.into(),
            providers,
        }
    }

    pub fn from_provider_inputs(
        app_type: impl Into<String>,
        providers: Vec<ProviderSummaryInput>,
        current_provider: Option<&str>,
        failover_provider_ids: &[String],
        route_candidate_ids: &[String],
    ) -> Self {
        Self::new(
            app_type,
            providers
                .into_iter()
                .map(|provider| {
                    ProviderSummary::from_input(
                        provider,
                        current_provider,
                        failover_provider_ids,
                        route_candidate_ids,
                    )
                })
                .collect(),
        )
    }

    pub fn from_provider_specs(
        app_type: impl Into<String>,
        providers: impl IntoIterator<Item = ProviderSpec>,
        current_provider: Option<&str>,
        failover_provider_ids: &[String],
        route_candidate_ids: &[String],
    ) -> Self {
        Self::from_provider_inputs(
            app_type,
            providers
                .into_iter()
                .map(ProviderSummaryInput::from_provider_spec)
                .collect(),
            current_provider,
            failover_provider_ids,
            route_candidate_ids,
        )
    }
}

fn contains_provider_id(provider_ids: &[String], provider_id: &str) -> bool {
    provider_ids
        .iter()
        .any(|candidate| candidate.as_str() == provider_id)
}

fn string_value(raw: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    raw.and_then(|raw| raw.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn usize_value(raw: Option<&Map<String, Value>>, key: &str) -> Option<usize> {
    raw.and_then(|raw| raw.get(key))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AppChannelResponse<T, C, R> {
    List(AppChannelListResponse<T>),
    Route(AppChannelRouteResponse<C, R>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppChannelListResponse<T> {
    pub app_type: String,
    pub source: String,
    pub channels: Vec<T>,
}

impl<T> AppChannelListResponse<T> {
    pub fn new(app_type: impl Into<String>, source: impl Into<String>, channels: Vec<T>) -> Self {
        Self {
            app_type: app_type.into(),
            source: source.into(),
            channels,
        }
    }

    pub fn from_route_source(
        app_type: impl Into<String>,
        source: &ChannelRouteSource,
        channels: Vec<T>,
    ) -> Self {
        Self::new(app_type, source.as_str(), channels)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppChannelRouteResponse<C, R> {
    pub app_type: String,
    pub source: String,
    pub requested_model: Option<String>,
    pub interface_kind: Option<String>,
    pub route_group: String,
    pub channels: Vec<C>,
    pub rejected: Vec<R>,
}

impl<C, R> AppChannelRouteResponse<C, R> {
    pub fn new(
        app_type: impl Into<String>,
        source: impl Into<String>,
        requested_model: Option<String>,
        interface_kind: Option<String>,
        route_group: impl Into<String>,
        channels: Vec<C>,
        rejected: Vec<R>,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            source: source.into(),
            requested_model,
            interface_kind,
            route_group: route_group.into(),
            channels,
            rejected,
        }
    }
}

impl AppChannelRouteResponse<ChannelRouteCandidate, ChannelRouteRejected> {
    pub fn from_route_resolve(response: RouteResolveResponse) -> Self {
        Self::new(
            response.app_type,
            response.source.as_str(),
            response.requested_model,
            response.interface_kind,
            response.route_group,
            response.candidates,
            response.rejected,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentRouteProviderSummaryInput {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
}

impl CurrentRouteProviderSummaryInput {
    pub fn new(id: impl Into<String>, name: impl Into<String>, category: Option<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            category,
        }
    }

    pub fn from_provider_spec(spec: ProviderSpec) -> Self {
        let raw = spec.metadata.raw.as_object();
        Self::new(spec.id, spec.name, string_value(raw, "category"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentRouteProviderSummary {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
}

impl CurrentRouteProviderSummary {
    pub fn new(id: impl Into<String>, name: impl Into<String>, category: Option<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            category,
        }
    }

    pub fn from_input(input: CurrentRouteProviderSummaryInput) -> Self {
        Self::new(input.id, input.name, input.category)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentRouteTarget {
    pub app_type: String,
    pub provider_name: String,
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentRouteResponse<T> {
    pub app_type: String,
    pub active: bool,
    pub target: Option<T>,
    pub configured_provider: Option<CurrentRouteProviderSummary>,
}

impl<T> CurrentRouteResponse<T> {
    pub fn new(
        app_type: impl Into<String>,
        target: Option<T>,
        configured_provider: Option<CurrentRouteProviderSummary>,
    ) -> Self {
        let active = target.is_some();
        Self {
            app_type: app_type.into(),
            active,
            target,
            configured_provider,
        }
    }

    pub fn from_inputs(
        app_type: impl Into<String>,
        target: Option<T>,
        configured_provider: Option<CurrentRouteProviderSummaryInput>,
    ) -> Self {
        Self::new(
            app_type,
            target,
            configured_provider.map(CurrentRouteProviderSummary::from_input),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMigrationPreviewInput<T> {
    pub app_type: String,
    pub channels: Vec<T>,
    pub duplicate_count: usize,
    pub needs_review_count: usize,
}

impl<T> ChannelMigrationPreviewInput<T> {
    pub fn new(
        app_type: impl Into<String>,
        channels: Vec<T>,
        duplicate_count: usize,
        needs_review_count: usize,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            channels,
            duplicate_count,
            needs_review_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMigrationPreviewResponse<T> {
    pub app_type: String,
    pub channels: Vec<T>,
    pub duplicate_count: usize,
    pub needs_review_count: usize,
}

impl<T> ChannelMigrationPreviewResponse<T> {
    pub fn new(
        app_type: impl Into<String>,
        channels: Vec<T>,
        duplicate_count: usize,
        needs_review_count: usize,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            channels,
            duplicate_count,
            needs_review_count,
        }
    }

    pub fn from_input(input: ChannelMigrationPreviewInput<T>) -> Self {
        Self::new(
            input.app_type,
            input.channels,
            input.duplicate_count,
            input.needs_review_count,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMigrationMaterializeInput {
    pub app_type: String,
    pub previewed_channels: usize,
    pub inserted_channels: usize,
    pub inserted_models: usize,
    pub inserted_health_rows: usize,
    pub duplicate_count: usize,
    pub needs_review_count: usize,
}

impl ChannelMigrationMaterializeInput {
    pub fn new(
        app_type: impl Into<String>,
        previewed_channels: usize,
        inserted_channels: usize,
        inserted_models: usize,
        inserted_health_rows: usize,
        duplicate_count: usize,
        needs_review_count: usize,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            previewed_channels,
            inserted_channels,
            inserted_models,
            inserted_health_rows,
            duplicate_count,
            needs_review_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMigrationMaterializeResponse {
    pub app_type: String,
    pub previewed_channels: usize,
    pub inserted_channels: usize,
    pub inserted_models: usize,
    pub inserted_health_rows: usize,
    pub duplicate_count: usize,
    pub needs_review_count: usize,
}

impl ChannelMigrationMaterializeResponse {
    pub fn new(
        app_type: impl Into<String>,
        previewed_channels: usize,
        inserted_channels: usize,
        inserted_models: usize,
        inserted_health_rows: usize,
        duplicate_count: usize,
        needs_review_count: usize,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            previewed_channels,
            inserted_channels,
            inserted_models,
            inserted_health_rows,
            duplicate_count,
            needs_review_count,
        }
    }

    pub fn from_input(input: ChannelMigrationMaterializeInput) -> Self {
        Self::new(
            input.app_type,
            input.previewed_channels,
            input.inserted_channels,
            input.inserted_models,
            input.inserted_health_rows,
            input.duplicate_count,
            input.needs_review_count,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelRouteSource {
    MaterializedChannels,
    LegacyProjection,
}

impl ChannelRouteSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MaterializedChannels => "materialized_channels",
            Self::LegacyProjection => "legacy_projection",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RouteResolveRequest {
    pub app_type: String,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub interface_kind: Option<String>,
    #[serde(default)]
    pub route_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppChannelListQuery {
    #[serde(default)]
    requested_model: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    interface_kind: Option<String>,
    #[serde(default, rename = "interface")]
    interface_alias: Option<String>,
    #[serde(default)]
    route_group: Option<String>,
    #[serde(default)]
    group: Option<String>,
}

impl AppChannelListQuery {
    pub fn has_route_filters(&self) -> bool {
        self.requested_model.is_some()
            || self.model.is_some()
            || self.interface_kind.is_some()
            || self.interface_alias.is_some()
            || self.route_group.is_some()
            || self.group.is_some()
    }

    pub fn into_route_request(self, app_type: impl Into<String>) -> RouteResolveRequest {
        RouteResolveRequest {
            app_type: app_type.into(),
            requested_model: self.requested_model.or(self.model),
            interface_kind: self.interface_kind.or(self.interface_alias),
            route_group: self.route_group.or(self.group),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppModelListQuery {
    #[serde(default)]
    interface_kind: Option<String>,
    #[serde(default, rename = "interface")]
    interface_alias: Option<String>,
    #[serde(default)]
    route_group: Option<String>,
    #[serde(default)]
    group: Option<String>,
}

impl AppModelListQuery {
    pub fn route_group(&self) -> Option<String> {
        self.route_group
            .as_deref()
            .or(self.group.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    }

    pub fn interface_kind(&self) -> Option<InterfaceKind> {
        self.interface_kind
            .as_deref()
            .or(self.interface_alias.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(InterfaceKind::from_storage)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChannelListQuery {
    #[serde(default)]
    app_type: Option<String>,
}

impl ChannelListQuery {
    pub fn app_type(&self) -> Option<String> {
        self.app_type
            .as_deref()
            .map(str::trim)
            .map(ToString::to_string)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GroupListQuery {
    #[serde(default)]
    app_type: Option<String>,
}

impl GroupListQuery {
    pub fn app_type(&self) -> Option<String> {
        self.app_type
            .as_deref()
            .map(str::trim)
            .map(ToString::to_string)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRouteCandidate {
    pub channel_id: String,
    pub provider_id: String,
    pub channel_name: String,
    pub base_url: String,
    pub interface_kind: String,
    pub public_model: Option<String>,
    pub upstream_model: Option<String>,
    pub route_group: String,
    pub priority: i64,
    pub weight: u32,
    pub source_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRouteRejected {
    pub channel_id: String,
    pub provider_id: String,
    pub channel_name: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RouteResolveResponse {
    pub app_type: String,
    pub requested_model: Option<String>,
    pub interface_kind: Option<String>,
    pub route_group: String,
    pub source: ChannelRouteSource,
    pub candidates: Vec<ChannelRouteCandidate>,
    pub rejected: Vec<ChannelRouteRejected>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelListResponse<T> {
    pub channels: Vec<T>,
}

impl<T> ChannelListResponse<T> {
    pub fn new(channels: Vec<T>) -> Self {
        Self { channels }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRecord {
    pub id: String,
    pub provider_id: String,
    pub app_type: String,
    pub name: String,
    pub status: String,
    pub base_url: String,
    pub interface_kind: String,
    pub auth_profile_ref: Option<String>,
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
    pub source_kind: String,
    pub source_endpoint_url: Option<String>,
    #[serde(default)]
    pub models: Vec<ChannelModelRecord>,
    #[serde(default)]
    pub needs_review: bool,
    #[serde(default)]
    pub review_reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRecordInput {
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
    pub source_kind: String,
    #[serde(default)]
    pub source_endpoint_url: Option<String>,
    #[serde(default)]
    pub models: Vec<ChannelModelRecordInput>,
    #[serde(default)]
    pub needs_review: bool,
    #[serde(default)]
    pub review_reasons: Vec<String>,
}

pub fn channel_record_from_input(input: ChannelRecordInput) -> ChannelRecord {
    ChannelRecord {
        id: input.id,
        provider_id: input.provider_id,
        app_type: input.app_type,
        name: input.name,
        status: input.status,
        base_url: input.base_url,
        interface_kind: input.interface_kind,
        auth_profile_ref: input.auth_profile_ref,
        groups: input.groups,
        priority: input.priority,
        weight: input.weight,
        retry_policy: input.retry_policy,
        health_policy: input.health_policy,
        header_overrides: input.header_overrides,
        param_overrides: input.param_overrides,
        status_code_mapping: input.status_code_mapping,
        tags: input.tags,
        metadata: input.metadata,
        source_kind: input.source_kind,
        source_endpoint_url: input.source_endpoint_url,
        models: input
            .models
            .into_iter()
            .map(channel_model_record_from_input)
            .collect(),
        needs_review: input.needs_review,
        review_reasons: input.review_reasons,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChannelRecordResponse<T> {
    pub channel: T,
}

impl<T> ChannelRecordResponse<T> {
    pub fn new(channel: T) -> Self {
        Self { channel }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelKeyRecord {
    pub channel_id: String,
    pub key_ref: String,
    pub status: String,
    pub priority: i64,
    pub weight: u32,
    pub last_failure_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelKeyRecordInput {
    pub channel_id: String,
    pub key_ref: String,
    pub status: String,
    pub priority: i64,
    pub weight: u32,
    #[serde(default)]
    pub last_failure_at: Option<i64>,
}

pub fn channel_key_record_from_input(input: ChannelKeyRecordInput) -> ChannelKeyRecord {
    ChannelKeyRecord {
        channel_id: input.channel_id,
        key_ref: input.key_ref,
        status: input.status,
        priority: input.priority,
        weight: input.weight,
        last_failure_at: input.last_failure_at,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChannelKeyRecordResponse<T> {
    pub key: T,
}

impl<T> ChannelKeyRecordResponse<T> {
    pub fn new(key: T) -> Self {
        Self { key }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelKeyDeleteResponse {
    pub channel_id: String,
    pub key_ref: String,
    pub deleted: bool,
}

impl ChannelKeyDeleteResponse {
    pub fn new(channel_id: impl Into<String>, key_ref: impl Into<String>, deleted: bool) -> Self {
        Self {
            channel_id: channel_id.into(),
            key_ref: key_ref.into(),
            deleted,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelKeysResponse<T> {
    pub channel_id: String,
    pub keys: Vec<T>,
}

impl<T> ChannelKeysResponse<T> {
    pub fn new(channel_id: impl Into<String>, keys: Vec<T>) -> Self {
        Self {
            channel_id: channel_id.into(),
            keys,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelRecord {
    pub channel_id: String,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelRecordInput {
    pub channel_id: String,
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

pub fn channel_model_record_from_input(input: ChannelModelRecordInput) -> ChannelModelRecord {
    ChannelModelRecord {
        channel_id: input.channel_id,
        public_model: input.public_model,
        upstream_model: input.upstream_model,
        capabilities: input.capabilities,
        pricing_model: input.pricing_model,
        request_overrides: input.request_overrides,
        response_overrides: input.response_overrides,
    }
}

impl ChannelModelRecord {
    pub fn from_model_route(channel_id: impl Into<String>, route: ModelRoute) -> Self {
        Self {
            channel_id: channel_id.into(),
            public_model: route.public_model,
            upstream_model: route.upstream_model,
            capabilities: route.capabilities.raw,
            pricing_model: route.pricing_model,
            request_overrides: route.request_overrides,
            response_overrides: route.response_overrides,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelModelsResponse<T> {
    pub channel_id: String,
    pub models: Vec<T>,
}

impl<T> ChannelModelsResponse<T> {
    pub fn new(channel_id: impl Into<String>, models: Vec<T>) -> Self {
        Self {
            channel_id: channel_id.into(),
            models,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGroupChannelInput {
    #[serde(default)]
    pub groups: Vec<String>,
}

impl RouteGroupChannelInput {
    pub fn new(groups: Vec<String>) -> Self {
        Self { groups }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGroupSourceInput {
    pub app_type: String,
    pub source: String,
    #[serde(default)]
    pub channels: Vec<RouteGroupChannelInput>,
}

impl RouteGroupSourceInput {
    pub fn new(
        app_type: impl Into<String>,
        source: impl Into<String>,
        channels: Vec<RouteGroupChannelInput>,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            source: source.into(),
            channels,
        }
    }

    pub fn from_route_source(
        app_type: impl Into<String>,
        source: &ChannelRouteSource,
        channel_groups: impl IntoIterator<Item = Vec<String>>,
    ) -> Self {
        Self::new(
            app_type,
            source.as_str(),
            channel_groups
                .into_iter()
                .map(RouteGroupChannelInput::new)
                .collect(),
        )
    }

    pub fn from_channel_specs(
        app_type: impl Into<String>,
        source: &ChannelRouteSource,
        channels: impl IntoIterator<Item = ChannelSpec>,
    ) -> Self {
        Self::from_route_source(
            app_type,
            source,
            channels.into_iter().map(|channel| channel.groups),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGroupSummary {
    pub name: String,
    pub app_types: Vec<String>,
    pub channel_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGroupListResponse {
    pub app_type: Option<String>,
    pub sources: Vec<String>,
    pub groups: Vec<RouteGroupSummary>,
}

impl RouteGroupListResponse {
    pub fn from_sources(
        app_type: Option<String>,
        sources: impl IntoIterator<Item = RouteGroupSourceInput>,
    ) -> Self {
        let mut groups: BTreeMap<String, (BTreeSet<String>, usize)> = BTreeMap::new();
        let mut source_names = BTreeSet::new();

        for source in sources {
            source_names.insert(source.source);

            for channel in source.channels {
                let channel_groups = if channel.groups.is_empty() {
                    vec![DEFAULT_ROUTE_GROUP.to_string()]
                } else {
                    channel.groups
                };

                for group in channel_groups {
                    let entry = groups.entry(group).or_default();
                    entry.0.insert(source.app_type.clone());
                    entry.1 += 1;
                }
            }
        }

        Self {
            app_type,
            sources: source_names.into_iter().collect(),
            groups: groups
                .into_iter()
                .map(|(name, (app_types, channel_count))| RouteGroupSummary {
                    name,
                    app_types: app_types.into_iter().collect(),
                    channel_count,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyCoreEvent {
    pub event_type: ProxyCoreEventType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyCoreEventType {
    RouteSelected,
    AttemptStarted,
    AttemptSucceeded,
    AttemptFailed,
    BreakerOpened,
    BreakerClosed,
    UsageRecorded,
    Custom(String),
}

impl ProxyCoreEventType {
    pub fn event_name(&self) -> String {
        match self {
            Self::RouteSelected => "route_selected".to_string(),
            Self::AttemptStarted => "attempt_started".to_string(),
            Self::AttemptSucceeded => "attempt_succeeded".to_string(),
            Self::AttemptFailed => "attempt_failed".to_string(),
            Self::BreakerOpened => "breaker_opened".to_string(),
            Self::BreakerClosed => "breaker_closed".to_string(),
            Self::UsageRecorded => "usage_recorded".to_string(),
            Self::Custom(value) => value.clone(),
        }
    }
}

impl ProxyCoreEvent {
    pub fn into_event_payload(self) -> Value {
        let mut payload = if self.payload.is_object() {
            self.payload
        } else {
            serde_json::json!({ "data": self.payload })
        };

        if let Value::Object(object) = &mut payload {
            if let Some(request_id) = self.request_id {
                object.insert("requestId".to_string(), Value::String(request_id));
            }
            if let Some(channel_id) = self.channel_id {
                object.insert("channelId".to_string(), Value::String(channel_id));
            }
        }

        payload
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppChannelListQuery, AppChannelListResponse, AppChannelResponse, AppChannelRouteResponse,
        app_proxy_config_defaults_for_app, app_proxy_config_raw, auth_info_from_profile_ref,
        channel_health_reset_from_parts, channel_health_update_from_input,
        channel_model_record_from_input, channel_reachability_result_from_stream_check_result,
        proxy_app_config_from_parts, proxy_global_config_from_global_config,
        proxy_runtime_config_from_proxy_config,
        AppListResponse, AppModelListQuery, AppProxyConfig, AppSummaryInput,
        channel_key_record_from_input,
        channel_reachability_status_from_latency, channel_record_from_input, ChannelDeleteResponse,
        ChannelHealthUpdateInput, CHANNEL_HEALTH_UNKNOWN_STATUS,
        ChannelKeyRecordInput, ChannelListQuery, ChannelListResponse, ChannelReachabilityInput,
        ChannelMigrationMaterializeInput,
        ChannelMigrationMaterializeResponse, ChannelMigrationPreviewInput,
        ChannelMigrationPreviewResponse, ChannelModelRecord, ChannelModelRecordInput,
        ChannelModelsResponse, ChannelRecord, ChannelRecordInput, ChannelRecordResponse,
        ChannelRouteCandidate, ChannelReachabilityResult, ChannelReachabilityStatus,
        ChannelRouteRejected, ChannelRouteSource, ChannelTestInput, ChannelTestPlan,
        ChannelTestResponse, ClientModelCatalogResponse, should_retry_channel_reachability_failure,
        CopilotOptimizerConfig, CurrentRouteProviderSummaryInput, CurrentRouteResponse,
        CurrentRouteTarget, GlobalProxyConfig, GroupListQuery, HealthCheckResponse, ModelCatalog,
        OptimizerConfig, ProviderHealth, ProviderHealthUpdateInput, ProviderListResponse,
        ProviderSpec, ProviderSummaryInput, ProxyChannelModelWriteRequest,
        ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest, ProxyChannelTestRequest,
        ProxyChannelWriteRequest, ProxyConfig, ProxyCoreEvent, ProxyCoreEventType,
        ProxyRuntimeStatus, ProxyServerInfo, ProxyStatusResponse, ProxyTakeoverStatus,
        RectifierConfig, RouteGroupListResponse, RouteGroupSourceInput, RouteResolveResponse,
        StreamCheckConfig, StreamCheckResult, DEFAULT_PROXY_LISTEN_ADDRESS,
        DEFAULT_PROXY_LISTEN_PORT, DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD,
        plan_channel_test, provider_health_update_from_input,
    };
    use crate::domain::{
        AppKind, AuthProfileRef, ChannelHealthPolicy, ChannelOverrides, ChannelSpec,
        ChannelStatus, InterfaceKind, ModelCapabilities, ModelRoute, ProviderKind,
        ProviderMetadata, RetryPolicy, UpstreamEndpoint, DEFAULT_ROUTE_GROUP,
    };
    use serde_json::{json, Value};

    fn route_group_channel_spec(id: &str, app: AppKind, groups: Vec<String>) -> ChannelSpec {
        ChannelSpec {
            id: id.to_string(),
            provider_id: "provider-a".to_string(),
            app,
            name: id.to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: "https://api.example.com".to_string(),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::AnthropicMessages,
            auth_profile: None,
            models: Vec::new(),
            groups,
            priority: 0,
            weight: 100,
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

    fn channel_test_channel_spec() -> ChannelSpec {
        let mut channel = route_group_channel_spec(
            "channel-a",
            AppKind::Claude,
            vec!["default".to_string()],
        );
        channel.provider_id = "provider-a".to_string();
        channel.name = "Primary".to_string();
        channel.endpoint.base_url = "https://relay.example.com/v1".to_string();
        channel.interface = InterfaceKind::AnthropicMessages;
        channel.models = vec![ModelRoute {
            public_model: "sonnet".to_string(),
            upstream_model: "claude-sonnet".to_string(),
            capabilities: ModelCapabilities::default(),
            pricing_model: None,
            request_overrides: json!({}),
            response_overrides: json!({}),
        }];
        channel
    }

    fn app_summary_provider_spec(id: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: id.to_string(),
            kind: ProviderKind::OpenRouter,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        }
    }

    #[test]
    fn health_check_response_serializes_management_envelope() {
        let response = HealthCheckResponse::healthy("2026-06-18T00:00:00+00:00");

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(
            value,
            json!({
                "status": "healthy",
                "timestamp": "2026-06-18T00:00:00+00:00"
            })
        );
    }

    #[test]
    fn app_model_list_query_normalizes_aliases() {
        let query: AppModelListQuery = serde_json::from_value(json!({
            "interface": " openai_responses ",
            "group": " beta "
        }))
        .expect("deserialize query");

        assert_eq!(query.route_group().as_deref(), Some("beta"));
        assert_eq!(
            query.interface_kind().as_ref().map(|kind| kind.as_str()),
            Some("openai_responses")
        );
    }

    #[test]
    fn app_channel_list_query_builds_route_resolve_request() {
        let query: AppChannelListQuery = serde_json::from_value(json!({
            "model": "claude-sonnet-4",
            "interface": "anthropic_messages",
            "group": "default"
        }))
        .expect("deserialize query");

        assert!(query.has_route_filters());
        let request = query.into_route_request("claude");

        assert_eq!(request.app_type, "claude");
        assert_eq!(request.requested_model.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(request.interface_kind.as_deref(), Some("anthropic_messages"));
        assert_eq!(request.route_group.as_deref(), Some("default"));
    }

    #[test]
    fn app_channel_list_query_treats_present_blank_alias_as_filter() {
        let query: AppChannelListQuery = serde_json::from_value(json!({
            "group": " "
        }))
        .expect("deserialize query");

        assert!(query.has_route_filters());
        let request = query.into_route_request("claude");

        assert_eq!(request.route_group.as_deref(), Some(" "));
    }

    #[test]
    fn channel_list_query_trims_app_type_without_dropping_empty_value() {
        let query: ChannelListQuery = serde_json::from_value(json!({
            "appType": " claude "
        }))
        .expect("deserialize query");

        assert_eq!(query.app_type().as_deref(), Some("claude"));

        let query: ChannelListQuery = serde_json::from_value(json!({
            "appType": " "
        }))
        .expect("deserialize query");

        assert_eq!(query.app_type().as_deref(), Some(""));
    }

    #[test]
    fn group_list_query_trims_app_type_without_dropping_empty_value() {
        let query: GroupListQuery = serde_json::from_value(json!({
            "appType": " codex "
        }))
        .expect("deserialize query");

        assert_eq!(query.app_type().as_deref(), Some("codex"));

        let query: GroupListQuery = serde_json::from_value(json!({
            "appType": " "
        }))
        .expect("deserialize query");

        assert_eq!(query.app_type().as_deref(), Some(""));
    }

    #[test]
    fn proxy_channel_write_request_defaults_match_management_contract() {
        let request: ProxyChannelWriteRequest = serde_json::from_value(json!({
            "providerId": "provider-a",
            "appType": "claude",
            "name": "Primary",
            "baseUrl": "https://primary.example.com/v1",
            "interfaceKind": "anthropic_messages"
        }))
        .expect("deserialize request");

        assert_eq!(request.status, "enabled");
        assert_eq!(request.groups, vec!["default".to_string()]);
        assert_eq!(request.weight, 100);
        assert_eq!(request.retry_policy, serde_json::Value::Null);
        assert_eq!(request.status_code_mapping, serde_json::Value::Null);
        assert!(request.models.is_empty());
    }

    #[test]
    fn proxy_channel_patch_request_accepts_partial_updates() {
        let request: ProxyChannelPatchRequest = serde_json::from_value(json!({
            "baseUrl": "https://next.example.com/v1",
            "groups": ["beta"]
        }))
        .expect("deserialize request");

        assert_eq!(
            request.base_url.as_deref(),
            Some("https://next.example.com/v1")
        );
        assert_eq!(request.groups, Some(vec!["beta".to_string()]));
        assert!(request.name.is_none());
        assert!(request.metadata.is_none());
    }

    #[test]
    fn proxy_channel_models_replace_request_wraps_model_writes() {
        let request = ProxyChannelModelsReplaceRequest {
            models: vec![ProxyChannelModelWriteRequest {
                public_model: "sonnet".to_string(),
                upstream_model: "claude-sonnet".to_string(),
                capabilities: json!({"toolUse": true}),
                pricing_model: Some("claude-sonnet".to_string()),
                request_overrides: json!({}),
                response_overrides: json!({}),
            }],
        };

        let value = serde_json::to_value(request).expect("serialize request");

        assert_eq!(value["models"][0]["publicModel"], "sonnet");
        assert_eq!(value["models"][0]["upstreamModel"], "claude-sonnet");
        assert_eq!(value["models"][0]["capabilities"]["toolUse"], true);
    }

    #[test]
    fn proxy_channel_test_request_normalizes_optional_filters() {
        let request: ProxyChannelTestRequest = serde_json::from_value(json!({
            "model": " sonnet ",
            "interfaceKind": " openai_responses "
        }))
        .expect("deserialize request");

        assert_eq!(request.requested_model(), Some("sonnet"));
        assert_eq!(request.requested_interface(), Some("openai_responses"));

        let blank: ProxyChannelTestRequest = serde_json::from_value(json!({
            "model": " ",
            "interfaceKind": ""
        }))
        .expect("deserialize blank request");

        assert_eq!(blank.requested_model(), None);
        assert_eq!(blank.requested_interface(), None);
    }

    #[test]
    fn channel_test_response_serializes_management_contract() {
        let response = ChannelTestResponse::from_input(ChannelTestInput {
            channel_id: "ch_1".to_string(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            channel_name: "Primary".to_string(),
            base_url: "https://upstream.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            model: Some("sonnet".to_string()),
            model_available: Some(true),
            success: true,
            status: "operational".to_string(),
            message: "Reachable".to_string(),
            latency_ms: Some(123),
            http_status: Some(401),
            tested_at: 1_771_000_000,
            retry_count: 1,
            failure_reason: None,
        });

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["channelId"], "ch_1");
        assert_eq!(value["providerId"], "provider-a");
        assert_eq!(value["appType"], "claude");
        assert_eq!(value["channelName"], "Primary");
        assert_eq!(value["interfaceKind"], "anthropic_messages");
        assert_eq!(value["model"], "sonnet");
        assert_eq!(value["modelAvailable"], true);
        assert_eq!(value["success"], true);
        assert_eq!(value["status"], "operational");
        assert_eq!(value["latencyMs"], 123);
        assert_eq!(value["httpStatus"], 401);
    }

    #[test]
    fn channel_test_plan_rejects_unavailable_interface_before_probe() {
        let channel = channel_test_channel_spec();
        let request: ProxyChannelTestRequest = serde_json::from_value(json!({
            "interfaceKind": "openai_responses"
        }))
        .expect("deserialize request");

        let response = match plan_channel_test(&channel, &request, 1_771_000_000) {
            ChannelTestPlan::Failure(response) => response,
            ChannelTestPlan::Probe(_) => panic!("expected preflight failure"),
        };

        assert!(!response.success);
        assert_eq!(response.status, "failed");
        assert_eq!(response.interface_kind, "anthropic_messages");
        assert!(response
            .failure_reason
            .as_deref()
            .unwrap()
            .contains("interface not available"));
    }

    #[test]
    fn channel_test_plan_rejects_unmapped_model_before_probe() {
        let channel = channel_test_channel_spec();
        let request: ProxyChannelTestRequest = serde_json::from_value(json!({
            "model": "missing-model"
        }))
        .expect("deserialize request");

        let response = match plan_channel_test(&channel, &request, 1_771_000_000) {
            ChannelTestPlan::Failure(response) => response,
            ChannelTestPlan::Probe(_) => panic!("expected preflight failure"),
        };

        assert_eq!(response.model.as_deref(), Some("missing-model"));
        assert_eq!(response.model_available, Some(false));
        assert!(response
            .failure_reason
            .as_deref()
            .unwrap()
            .contains("model not mapped"));
    }

    #[test]
    fn channel_test_context_wraps_reachability_result() {
        let channel = channel_test_channel_spec();
        let request: ProxyChannelTestRequest = serde_json::from_value(json!({
            "model": "sonnet",
            "interfaceKind": "anthropic_messages"
        }))
        .expect("deserialize request");

        let context = match plan_channel_test(&channel, &request, 1_771_000_000) {
            ChannelTestPlan::Probe(context) => context,
            ChannelTestPlan::Failure(response) => {
                panic!("unexpected preflight failure: {:?}", response.failure_reason)
            }
        };
        let response = context.reachability_response(ChannelReachabilityResult {
            success: true,
            status: "operational".to_string(),
            message: "Reachable".to_string(),
            latency_ms: Some(23),
            http_status: Some(401),
            tested_at: 1_771_000_001,
            retry_count: 1,
        });

        assert!(response.success);
        assert_eq!(response.model_available, Some(true));
        assert_eq!(response.latency_ms, Some(23));
        assert_eq!(response.failure_reason, None);
    }

    #[test]
    fn channel_test_context_exposes_probe_request() {
        let channel = channel_test_channel_spec();
        let request: ProxyChannelTestRequest = serde_json::from_value(json!({
            "model": "sonnet",
            "interfaceKind": "anthropic_messages"
        }))
        .expect("deserialize request");

        let context = match plan_channel_test(&channel, &request, 1_771_000_000) {
            ChannelTestPlan::Probe(context) => context,
            ChannelTestPlan::Failure(response) => {
                panic!("unexpected preflight failure: {:?}", response.failure_reason)
            }
        };
        let probe = context.probe_request();

        assert_eq!(probe.channel_id, "channel-a");
        assert_eq!(probe.provider_id, "provider-a");
        assert_eq!(probe.app_type, "claude");
        assert_eq!(probe.base_url, "https://relay.example.com/v1");
        assert_eq!(
            probe.provider_not_found_message(),
            "provider not found for channel channel-a: provider-a"
        );
    }

    #[test]
    fn channel_reachability_result_status_contract_mapping() {
        assert_eq!(ChannelReachabilityStatus::Operational.as_str(), "operational");
        assert_eq!(ChannelReachabilityStatus::Degraded.as_str(), "degraded");
        assert_eq!(ChannelReachabilityStatus::Failed.as_str(), "failed");
        assert_eq!(
            serde_json::to_value(ChannelReachabilityStatus::Operational)
                .expect("serialize status"),
            json!("operational")
        );
        assert_eq!(
            channel_reachability_status_from_latency(1_500, 1_500),
            ChannelReachabilityStatus::Operational
        );
        assert_eq!(
            channel_reachability_status_from_latency(1_501, 1_500),
            ChannelReachabilityStatus::Degraded
        );
        assert!(should_retry_channel_reachability_failure("Request timeout"));
        assert!(should_retry_channel_reachability_failure("request timed out"));
        assert!(should_retry_channel_reachability_failure("connection abort"));
        assert!(!should_retry_channel_reachability_failure(
            "Connection failed: dns error"
        ));
        assert!(!should_retry_channel_reachability_failure("Reachable"));
        assert_eq!(
            serde_json::to_value(StreamCheckConfig::default()).expect("serialize config"),
            json!({
                "timeoutSecs": 8,
                "maxRetries": 1,
                "degradedThresholdMs": 6000
            })
        );
        let stream_check = StreamCheckResult {
            status: ChannelReachabilityStatus::Degraded,
            success: true,
            message: "Reachable".to_string(),
            response_time_ms: Some(6100),
            http_status: Some(403),
            model_used: String::new(),
            tested_at: 1_771_000_000,
            retry_count: 1,
            error_category: None,
        };
        assert_eq!(
            serde_json::to_value(stream_check.clone()).expect("serialize stream check result"),
            json!({
                "status": "degraded",
                "success": true,
                "message": "Reachable",
                "responseTimeMs": 6100,
                "httpStatus": 403,
                "modelUsed": "",
                "testedAt": 1771000000,
                "retryCount": 1
            })
        );
        let reachability =
            channel_reachability_result_from_stream_check_result(stream_check);
        assert_eq!(reachability.status, "degraded");
        assert_eq!(reachability.message, "Reachable");
        assert_eq!(reachability.latency_ms, Some(6100));
        assert_eq!(reachability.http_status, Some(403));
        assert_eq!(reachability.tested_at, 1_771_000_000);
        assert_eq!(reachability.retry_count, 1);

        let result = ChannelReachabilityResult::from_input(ChannelReachabilityInput {
            success: false,
            status: ChannelReachabilityStatus::Degraded,
            message: "Slow response".to_string(),
            latency_ms: Some(1_250),
            http_status: Some(429),
            tested_at: 1_771_000_002,
            retry_count: 2,
        });

        assert!(!result.success);
        assert_eq!(result.status, "degraded");
        assert_eq!(result.message, "Slow response");
        assert_eq!(result.latency_ms, Some(1_250));
        assert_eq!(result.http_status, Some(429));
        assert_eq!(result.tested_at, 1_771_000_002);
        assert_eq!(result.retry_count, 2);
    }

    #[test]
    fn channel_health_update_tracks_threshold_contract() {
        assert_eq!(CHANNEL_HEALTH_UNKNOWN_STATUS, "unknown");

        let degraded = channel_health_update_from_input(ChannelHealthUpdateInput {
            current_consecutive_failures: 0,
            success: false,
            error_msg: Some("first failure".to_string()),
            failure_threshold: 2,
            timestamp_ms: 1_771_000_000_000,
        });
        assert_eq!(degraded.status, "degraded");
        assert_eq!(degraded.consecutive_failures, 1);
        assert_eq!(degraded.last_success_at, None);
        assert_eq!(degraded.last_failure_at, Some(1_771_000_000_000));
        assert_eq!(degraded.disabled_reason.as_deref(), Some("first failure"));

        let unhealthy = channel_health_update_from_input(ChannelHealthUpdateInput {
            current_consecutive_failures: 1,
            success: false,
            error_msg: Some("second failure".to_string()),
            failure_threshold: 2,
            timestamp_ms: 1_771_000_000_100,
        });
        assert_eq!(unhealthy.status, "unhealthy");
        assert_eq!(unhealthy.consecutive_failures, 2);
        assert_eq!(unhealthy.last_failure_at, Some(1_771_000_000_100));
        assert_eq!(unhealthy.disabled_reason.as_deref(), Some("second failure"));

        let healthy = channel_health_update_from_input(ChannelHealthUpdateInput {
            current_consecutive_failures: 2,
            success: true,
            error_msg: Some("ignored".to_string()),
            failure_threshold: 2,
            timestamp_ms: 1_771_000_000_200,
        });
        assert_eq!(healthy.status, "healthy");
        assert_eq!(healthy.consecutive_failures, 0);
        assert_eq!(healthy.last_success_at, Some(1_771_000_000_200));
        assert_eq!(healthy.last_failure_at, None);
        assert_eq!(healthy.disabled_reason, None);
    }

    #[test]
    fn provider_health_update_tracks_threshold_contract() {
        let still_healthy = provider_health_update_from_input(ProviderHealthUpdateInput {
            current_consecutive_failures: 0,
            success: false,
            error_msg: Some("first failure".to_string()),
            failure_threshold: 2,
            timestamp: "2026-06-21T01:00:00Z".to_string(),
        });
        assert!(still_healthy.is_healthy);
        assert_eq!(still_healthy.consecutive_failures, 1);
        assert_eq!(still_healthy.last_success_at, None);
        assert_eq!(
            still_healthy.last_failure_at.as_deref(),
            Some("2026-06-21T01:00:00Z")
        );
        assert_eq!(still_healthy.last_error.as_deref(), Some("first failure"));

        let unhealthy = provider_health_update_from_input(ProviderHealthUpdateInput {
            current_consecutive_failures: 1,
            success: false,
            error_msg: Some("second failure".to_string()),
            failure_threshold: 2,
            timestamp: "2026-06-21T01:01:00Z".to_string(),
        });
        assert!(!unhealthy.is_healthy);
        assert_eq!(unhealthy.consecutive_failures, 2);
        assert_eq!(
            unhealthy.last_failure_at.as_deref(),
            Some("2026-06-21T01:01:00Z")
        );
        assert_eq!(unhealthy.last_error.as_deref(), Some("second failure"));

        let healthy = provider_health_update_from_input(ProviderHealthUpdateInput {
            current_consecutive_failures: 2,
            success: true,
            error_msg: None,
            failure_threshold: 2,
            timestamp: "2026-06-21T01:02:00Z".to_string(),
        });
        assert!(healthy.is_healthy);
        assert_eq!(healthy.consecutive_failures, 0);
        assert_eq!(
            healthy.last_success_at.as_deref(),
            Some("2026-06-21T01:02:00Z")
        );
        assert_eq!(healthy.last_failure_at, None);
        assert_eq!(healthy.last_error, None);

        let zero_threshold = provider_health_update_from_input(ProviderHealthUpdateInput {
            current_consecutive_failures: 0,
            success: false,
            error_msg: None,
            failure_threshold: 0,
            timestamp: "2026-06-21T01:03:00Z".to_string(),
        });
        assert!(!zero_threshold.is_healthy);
        assert_eq!(zero_threshold.consecutive_failures, 1);
    }

    #[test]
    fn app_list_response_serializes_management_envelope() {
        let response =
            AppListResponse::from_app_inputs(vec![AppSummaryInput::new("claude", true, false, 2, 3)]);

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(
            value,
            json!({
                "apps": [{
                    "appType": "claude",
                    "enabled": true,
                    "autoFailoverEnabled": false,
                    "providerCount": 2,
                    "channelCount": 3
                }]
            })
        );
    }

    #[test]
    fn app_summary_input_counts_provider_and_channel_specs() {
        let input = AppSummaryInput::from_specs(
            "claude",
            true,
            false,
            vec![
                app_summary_provider_spec("provider-a"),
                app_summary_provider_spec("provider-b"),
            ],
            vec![
                route_group_channel_spec("channel-a", AppKind::Claude, vec![]),
                route_group_channel_spec("channel-b", AppKind::Claude, vec![]),
                route_group_channel_spec("channel-c", AppKind::Claude, vec![]),
            ],
        );

        assert_eq!(input, AppSummaryInput::new("claude", true, false, 2, 3));
    }

    #[test]
    fn client_model_catalog_response_serializes_as_raw_catalog() {
        let response = ClientModelCatalogResponse::from_catalog(ModelCatalog {
            provider_id: "codex".to_string(),
            models: vec!["gpt-5".to_string()],
            raw: json!({
                "models": [{
                    "id": "gpt-5",
                    "object": "model"
                }]
            }),
        });

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(
            value,
            json!({
                "models": [{
                    "id": "gpt-5",
                    "object": "model"
                }]
            })
        );
    }

    #[test]
    fn proxy_status_response_serializes_as_raw_status() {
        let response = ProxyStatusResponse::new(ProxyRuntimeStatus {
            running: true,
            address: "127.0.0.1".to_string(),
            port: 15721,
            active_connections: 2,
            total_requests: 5,
            success_requests: 4,
            failed_requests: 1,
            success_rate: 80.0,
            uptime_seconds: 30,
            current_provider: Some("Provider A".to_string()),
            current_provider_id: Some("provider-a".to_string()),
            last_request_at: None,
            last_error: None,
            failover_count: 1,
            active_targets: vec![CurrentRouteTarget {
                app_type: "claude".to_string(),
                provider_name: "Provider A".to_string(),
                provider_id: "provider-a".to_string(),
                channel_id: Some("channel-a".to_string()),
                channel_name: None,
                interface_kind: None,
                public_model: None,
                upstream_model: None,
            }],
        });

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["running"], true);
        assert_eq!(value["active_connections"], 2);
        assert_eq!(value["total_requests"], 5);
        assert_eq!(value["current_provider"], "Provider A");
        assert!(value["last_request_at"].is_null());
        assert_eq!(value["active_targets"][0]["appType"], "claude");
        assert_eq!(value["active_targets"][0]["providerName"], "Provider A");
        assert_eq!(value["active_targets"][0]["channelId"], "channel-a");
    }

    #[test]
    fn proxy_config_default_preserves_legacy_values() {
        let config = ProxyConfig::default();

        assert_eq!(config.listen_address, DEFAULT_PROXY_LISTEN_ADDRESS);
        assert_eq!(config.listen_port, DEFAULT_PROXY_LISTEN_PORT);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.request_timeout, 600);
        assert!(config.enable_logging);
        assert!(!config.live_takeover_active);
        assert_eq!(config.management_auth_token, None);
        assert_eq!(config.streaming_first_byte_timeout, 60);
        assert_eq!(config.streaming_idle_timeout, 120);
        assert_eq!(config.non_streaming_timeout, 600);
    }

    #[test]
    fn proxy_config_serde_preserves_legacy_command_contract() {
        let value = serde_json::to_value(ProxyConfig::default()).expect("serialize proxy config");

        assert_eq!(value["listen_address"], DEFAULT_PROXY_LISTEN_ADDRESS);
        assert_eq!(value["listen_port"], DEFAULT_PROXY_LISTEN_PORT);
        assert_eq!(value["max_retries"], 3);
        assert_eq!(value["request_timeout"], 600);
        assert_eq!(value["enable_logging"], true);
        assert_eq!(value["live_takeover_active"], false);
        assert_eq!(value["streaming_first_byte_timeout"], 60);
        assert_eq!(value["streaming_idle_timeout"], 120);
        assert_eq!(value["non_streaming_timeout"], 600);
        assert!(value.get("management_auth_token").is_none());
    }

    #[test]
    fn proxy_config_deserializes_old_payload_with_timeout_defaults() {
        let config: ProxyConfig = serde_json::from_value(json!({
            "listen_address": "127.0.0.1",
            "listen_port": 15721,
            "max_retries": 2,
            "request_timeout": 600,
            "enable_logging": false
        }))
        .expect("deserialize legacy proxy config");

        assert!(!config.enable_logging);
        assert!(!config.live_takeover_active);
        assert_eq!(config.management_auth_token, None);
        assert_eq!(config.streaming_first_byte_timeout, 60);
        assert_eq!(config.streaming_idle_timeout, 120);
        assert_eq!(config.non_streaming_timeout, 600);
    }

    #[test]
    fn global_proxy_config_preserves_management_command_shape() {
        assert_eq!(
            GlobalProxyConfig::default(),
            GlobalProxyConfig {
                proxy_enabled: false,
                listen_address: DEFAULT_PROXY_LISTEN_ADDRESS.to_string(),
                listen_port: DEFAULT_PROXY_LISTEN_PORT,
                enable_logging: true,
            }
        );

        let config = GlobalProxyConfig {
            proxy_enabled: true,
            listen_address: "127.0.0.1".to_string(),
            listen_port: 15721,
            enable_logging: false,
        };

        let value = serde_json::to_value(config).expect("serialize global proxy config");

        assert_eq!(
            value,
            json!({
                "proxyEnabled": true,
                "listenAddress": "127.0.0.1",
                "listenPort": 15721,
                "enableLogging": false
            })
        );
    }

    #[test]
    fn app_proxy_config_defaults_preserve_seed_contract() {
        let claude = app_proxy_config_defaults_for_app("claude");
        assert_eq!(claude.app_type, "claude");
        assert!(!claude.enabled);
        assert!(!claude.auto_failover_enabled);
        assert_eq!(claude.max_retries, 6);
        assert_eq!(claude.streaming_first_byte_timeout, 90);
        assert_eq!(claude.streaming_idle_timeout, 180);
        assert_eq!(claude.non_streaming_timeout, 600);
        assert_eq!(claude.circuit_failure_threshold, 8);
        assert_eq!(claude.circuit_success_threshold, 3);
        assert_eq!(claude.circuit_timeout_seconds, 90);
        assert_eq!(claude.circuit_error_rate_threshold, 0.7);
        assert_eq!(claude.circuit_min_requests, 15);

        let codex = app_proxy_config_defaults_for_app("codex");
        assert_eq!(codex.max_retries, 3);
        assert_eq!(codex.streaming_first_byte_timeout, 60);
        assert_eq!(codex.streaming_idle_timeout, 120);
        assert_eq!(codex.circuit_failure_threshold, 4);

        let gemini = app_proxy_config_defaults_for_app("gemini");
        assert_eq!(gemini.max_retries, 5);
        assert_eq!(gemini.streaming_first_byte_timeout, 60);
        assert_eq!(gemini.circuit_failure_threshold, 4);

        let other = app_proxy_config_defaults_for_app("opencode");
        assert_eq!(other.app_type, "opencode");
        assert_eq!(other.max_retries, 3);
        assert_eq!(other.streaming_idle_timeout, 120);
        assert_eq!(other.circuit_min_requests, 10);
    }

    #[test]
    fn app_proxy_config_preserves_management_command_shape() {
        let config = AppProxyConfig {
            app_type: "codex".to_string(),
            enabled: true,
            auto_failover_enabled: true,
            max_retries: 4,
            streaming_first_byte_timeout: 30,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
            circuit_failure_threshold: 4,
            circuit_success_threshold: 2,
            circuit_timeout_seconds: 60,
            circuit_error_rate_threshold: 0.6,
            circuit_min_requests: 10,
        };

        let value = serde_json::to_value(config).expect("serialize app proxy config");

        assert_eq!(
            value,
            json!({
                "appType": "codex",
                "enabled": true,
                "autoFailoverEnabled": true,
                "maxRetries": 4,
                "streamingFirstByteTimeout": 30,
                "streamingIdleTimeout": 120,
                "nonStreamingTimeout": 600,
                "circuitFailureThreshold": 4,
                "circuitSuccessThreshold": 2,
                "circuitTimeoutSeconds": 60,
                "circuitErrorRateThreshold": 0.6,
                "circuitMinRequests": 10
            })
        );
    }

    #[test]
    fn app_proxy_config_raw_includes_current_provider_id() {
        let config = AppProxyConfig {
            app_type: "codex".to_string(),
            enabled: true,
            auto_failover_enabled: true,
            max_retries: 4,
            streaming_first_byte_timeout: 30,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
            circuit_failure_threshold: 4,
            circuit_success_threshold: 2,
            circuit_timeout_seconds: 60,
            circuit_error_rate_threshold: 0.6,
            circuit_min_requests: 10,
        };

        let raw = app_proxy_config_raw(config.clone(), Some("provider-1".to_string()));

        assert_eq!(
            raw.get("currentProviderId").and_then(Value::as_str),
            Some("provider-1")
        );
        assert_eq!(raw.get("appType").and_then(Value::as_str), Some("codex"));

        let raw_without_provider = app_proxy_config_raw(config, None);
        assert!(raw_without_provider
            .get("currentProviderId")
            .is_some_and(Value::is_null));
    }

    #[test]
    fn proxy_config_projection_helpers_preserve_raw_contracts() {
        let proxy_config = ProxyConfig {
            listen_address: "127.0.0.1".to_string(),
            listen_port: 18080,
            enable_logging: false,
            ..ProxyConfig::default()
        };

        let global = proxy_global_config_from_global_config(GlobalProxyConfig {
            proxy_enabled: true,
            listen_address: "127.0.0.1".to_string(),
            listen_port: 18080,
            enable_logging: false,
        });
        assert_eq!(global.bind_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(global.bind_port, Some(18080));
        assert_eq!(global.request_timeout_ms, None);
        assert_eq!(global.raw["proxyEnabled"], json!(true));
        assert_eq!(global.raw["listenAddress"], json!("127.0.0.1"));

        let runtime = proxy_runtime_config_from_proxy_config(proxy_config, true);
        assert!(runtime.privacy_filter_enabled);
        assert!(!runtime.route_events_enabled);
        assert_eq!(runtime.raw["enable_logging"], json!(false));

        let app = proxy_app_config_from_parts(
            AppKind::Codex,
            AppProxyConfig {
                app_type: "codex".to_string(),
                enabled: true,
                auto_failover_enabled: true,
                max_retries: 4,
                streaming_first_byte_timeout: 30,
                streaming_idle_timeout: 120,
                non_streaming_timeout: 600,
                circuit_failure_threshold: 4,
                circuit_success_threshold: 2,
                circuit_timeout_seconds: 60,
                circuit_error_rate_threshold: 0.6,
                circuit_min_requests: 10,
            },
            Some("provider-1".to_string()),
            RectifierConfig {
                enabled: false,
                ..RectifierConfig::default()
            },
            OptimizerConfig {
                enabled: true,
                cache_ttl: "5m".to_string(),
                ..OptimizerConfig::default()
            },
            CopilotOptimizerConfig {
                enabled: false,
                warmup_model: "gpt-5".to_string(),
                ..CopilotOptimizerConfig::default()
            },
        );

        assert_eq!(app.app, Some(AppKind::Codex));
        assert!(app.enabled);
        assert_eq!(app.default_group.as_deref(), Some(DEFAULT_ROUTE_GROUP));
        assert_eq!(app.raw["currentProviderId"], json!("provider-1"));
        assert_eq!(app.raw["autoFailoverEnabled"], json!(true));
        assert!(!app.rectifier.enabled);
        assert_eq!(app.rectifier.raw["enabled"], json!(false));
        assert!(app.optimizer.enabled);
        assert_eq!(app.optimizer.raw["cacheTtl"], json!("5m"));
        assert!(!app.copilot_optimizer.enabled);
        assert_eq!(app.copilot_optimizer.raw["warmupModel"], json!("gpt-5"));
    }

    #[test]
    fn auth_info_from_profile_ref_preserves_source_metadata() {
        let auth_info = auth_info_from_profile_ref(
            Some(&AuthProfileRef::new("provider:claude:anthropic-main")),
            "cc_switch_provider_config",
        );

        assert!(auth_info.headers.is_empty());
        assert_eq!(
            auth_info.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(
            auth_info.metadata["source"],
            json!("cc_switch_provider_config")
        );

        let anonymous = auth_info_from_profile_ref(None, "cc_switch_provider_config");
        assert_eq!(anonymous.account_ref, None);
        assert_eq!(anonymous.metadata["source"], json!("cc_switch_provider_config"));
    }

    #[test]
    fn channel_health_reset_from_parts_normalizes_app_kind() {
        let reset = channel_health_reset_from_parts("channel-a", "claude");

        assert_eq!(reset.channel_id, "channel-a");
        assert_eq!(reset.app, AppKind::Claude);

        let custom = channel_health_reset_from_parts("channel-b", "opencode");
        assert_eq!(custom.app, AppKind::Custom("opencode".to_string()));
    }

    #[test]
    fn default_channel_health_failure_threshold_matches_runtime_contract() {
        assert_eq!(DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD, 4);
    }

    #[test]
    fn proxy_core_event_builds_external_name_and_payload() {
        assert_eq!(ProxyCoreEventType::RouteSelected.event_name(), "route_selected");
        assert_eq!(
            ProxyCoreEventType::Custom("custom.event".to_string()).event_name(),
            "custom.event"
        );

        let payload = ProxyCoreEvent {
            event_type: ProxyCoreEventType::RouteSelected,
            request_id: Some("req-1".to_string()),
            channel_id: Some("ch-1".to_string()),
            payload: json!({"attemptCount": 2}),
        }
        .into_event_payload();

        assert_eq!(payload["requestId"], "req-1");
        assert_eq!(payload["channelId"], "ch-1");
        assert_eq!(payload["attemptCount"], 2);

        let payload = ProxyCoreEvent {
            event_type: ProxyCoreEventType::UsageRecorded,
            request_id: None,
            channel_id: None,
            payload: json!("done"),
        }
        .into_event_payload();

        assert_eq!(payload, json!({"data": "done"}));
    }

    #[test]
    fn rectifier_config_defaults_all_controls_enabled() {
        let config = RectifierConfig::default();

        assert!(config.enabled);
        assert!(config.request_thinking_signature);
        assert!(config.request_thinking_budget);
        assert!(config.request_media_fallback);
        assert!(config.request_media_heuristic);
    }

    #[test]
    fn rectifier_config_missing_fields_default_to_enabled() {
        let config: RectifierConfig =
            serde_json::from_value(json!({})).expect("deserialize default rectifier config");

        assert_eq!(config, RectifierConfig::default());
        assert!(config.enabled);
        assert!(config.request_thinking_signature);
        assert!(config.request_thinking_budget);
        assert!(config.request_media_fallback);
        assert!(config.request_media_heuristic);
    }

    #[test]
    fn rectifier_config_partial_fields_preserve_explicit_false() {
        let config: RectifierConfig =
            serde_json::from_value(json!({"enabled": true, "requestThinkingSignature": false}))
                .expect("deserialize partial rectifier config");

        assert!(config.enabled);
        assert!(!config.request_thinking_signature);
        assert!(config.request_thinking_budget);
        assert!(config.request_media_fallback);
        assert!(config.request_media_heuristic);
    }

    #[test]
    fn rectifier_config_media_fields_preserve_explicit_false() {
        let config: RectifierConfig = serde_json::from_value(json!({
            "requestMediaFallback": false,
            "requestMediaHeuristic": false
        }))
        .expect("deserialize media rectifier config");

        assert!(!config.request_media_fallback);
        assert!(!config.request_media_heuristic);
        assert!(config.enabled);
        assert!(config.request_thinking_signature);
        assert!(config.request_thinking_budget);
    }

    #[test]
    fn rectifier_config_projects_signature_detection_controls() {
        let message = Some("messages.1.content.0: Invalid `signature` in `thinking` block");

        let config = RectifierConfig::default();
        assert!(crate::thinking_rectifier::should_rectify_thinking_signature(
            message,
            &config.thinking_signature_core_config()
        ));

        let config = RectifierConfig {
            enabled: false,
            ..RectifierConfig::default()
        };
        assert!(!crate::thinking_rectifier::should_rectify_thinking_signature(
            message,
            &config.thinking_signature_core_config()
        ));

        let config = RectifierConfig {
            request_thinking_signature: false,
            ..RectifierConfig::default()
        };
        assert!(!crate::thinking_rectifier::should_rectify_thinking_signature(
            message,
            &config.thinking_signature_core_config()
        ));
    }

    #[test]
    fn rectifier_config_projects_budget_detection_controls() {
        let message = Some("thinking.budget_tokens: Input should be greater than or equal to 1024");

        let config = RectifierConfig::default();
        assert!(crate::thinking_budget_rectifier::should_rectify_thinking_budget(
            message,
            &config.thinking_budget_core_config()
        ));

        let config = RectifierConfig {
            enabled: false,
            ..RectifierConfig::default()
        };
        assert!(!crate::thinking_budget_rectifier::should_rectify_thinking_budget(
            message,
            &config.thinking_budget_core_config()
        ));

        let config = RectifierConfig {
            request_thinking_budget: false,
            ..RectifierConfig::default()
        };
        assert!(!crate::thinking_budget_rectifier::should_rectify_thinking_budget(
            message,
            &config.thinking_budget_core_config()
        ));
    }

    #[test]
    fn optimizer_config_defaults_and_projects_core_configs() {
        let config: OptimizerConfig = serde_json::from_value(json!({}))
            .expect("deserialize default optimizer config");

        assert_eq!(config, OptimizerConfig::default());
        assert!(!config.enabled);
        assert!(config.thinking_optimizer_core_config().enabled);
        assert!(config.cache_injection_core_config().enabled);
        assert_eq!(config.cache_injection_core_config().ttl, "1h");
    }

    #[test]
    fn copilot_optimizer_config_defaults_missing_fields_to_enabled() {
        let config: CopilotOptimizerConfig = serde_json::from_value(json!({}))
            .expect("deserialize default copilot optimizer config");

        assert_eq!(config, CopilotOptimizerConfig::default());
        assert!(config.enabled);
        assert!(config.request_classification);
        assert!(config.tool_result_merging);
        assert!(config.compact_detection);
        assert!(config.deterministic_request_id);
        assert!(config.subagent_detection);
        assert!(config.warmup_downgrade);
        assert_eq!(config.warmup_model, "gpt-5-mini");
        assert!(config.strip_thinking);
    }

    #[test]
    fn proxy_server_info_preserves_tauri_command_shape() {
        let info = ProxyServerInfo {
            address: "127.0.0.1".to_string(),
            port: 15721,
            started_at: "2026-06-19T00:00:00Z".to_string(),
        };

        let value = serde_json::to_value(info).expect("serialize proxy server info");

        assert_eq!(
            value,
            json!({
                "address": "127.0.0.1",
                "port": 15721,
                "started_at": "2026-06-19T00:00:00Z"
            })
        );
    }

    #[test]
    fn proxy_takeover_status_preserves_tauri_command_shape() {
        let status = ProxyTakeoverStatus {
            claude: true,
            codex: false,
            gemini: true,
            opencode: false,
            openclaw: false,
        };

        let value = serde_json::to_value(status).expect("serialize takeover status");

        assert_eq!(
            value,
            json!({
                "claude": true,
                "codex": false,
                "gemini": true,
                "opencode": false,
                "openclaw": false
            })
        );
    }

    #[test]
    fn provider_health_preserves_tauri_command_shape() {
        let health = ProviderHealth {
            provider_id: "provider-a".to_string(),
            app_type: "codex".to_string(),
            is_healthy: false,
            consecutive_failures: 3,
            last_success_at: Some("2026-06-18T23:00:00Z".to_string()),
            last_failure_at: Some("2026-06-19T00:00:00Z".to_string()),
            last_error: Some("timeout".to_string()),
            updated_at: "2026-06-19T00:00:01Z".to_string(),
        };

        let value = serde_json::to_value(health).expect("serialize provider health");

        assert_eq!(
            value,
            json!({
                "provider_id": "provider-a",
                "app_type": "codex",
                "is_healthy": false,
                "consecutive_failures": 3,
                "last_success_at": "2026-06-18T23:00:00Z",
                "last_failure_at": "2026-06-19T00:00:00Z",
                "last_error": "timeout",
                "updated_at": "2026-06-19T00:00:01Z"
            })
        );
    }

    #[test]
    fn provider_list_response_serializes_sanitized_management_envelope() {
        let response = ProviderListResponse::from_provider_inputs(
            "claude",
            vec![ProviderSummaryInput {
                id: "provider-a".to_string(),
                name: "Provider A".to_string(),
                category: Some("aggregator".to_string()),
                sort_index: Some(1),
                icon: None,
                icon_color: Some("#00A67E".to_string()),
                provider_type: Some("openai_compatible".to_string()),
            }],
            Some("provider-a"),
            &[],
            &["provider-a".to_string()],
        );

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(
            value,
            json!({
                "appType": "claude",
                "providers": [{
                    "id": "provider-a",
                    "name": "Provider A",
                    "category": "aggregator",
                    "sortIndex": 1,
                    "icon": null,
                    "iconColor": "#00A67E",
                    "providerType": "openai_compatible",
                    "current": true,
                    "inFailoverQueue": false,
                    "routeCandidate": true
                }]
            })
        );
    }

    #[test]
    fn provider_summary_input_projects_from_provider_spec_metadata() {
        let input = ProviderSummaryInput::from_provider_spec(ProviderSpec {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            kind: ProviderKind::OpenRouter,
            account_ref: None,
            metadata: ProviderMetadata {
                labels: vec!["aggregator".to_string()],
                raw: json!({
                    "category": "aggregator",
                    "sortIndex": 7,
                    "icon": "openrouter",
                    "iconColor": "#111111",
                    "providerType": "openai_compatible",
                    "apiKey": "must-not-leak"
                }),
            },
        });

        assert_eq!(
            input,
            ProviderSummaryInput::new(
                "provider-a",
                "Provider A",
                Some("aggregator".to_string()),
                Some(7),
                Some("openrouter".to_string()),
                Some("#111111".to_string()),
                Some("openai_compatible".to_string()),
            )
        );
    }

    #[test]
    fn provider_summary_input_preserves_missing_provider_type() {
        let input = ProviderSummaryInput::from_provider_spec(ProviderSpec {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            kind: ProviderKind::GitHubCopilot,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        });

        assert!(input.provider_type.is_none());
    }

    #[test]
    fn provider_list_response_projects_provider_specs() {
        let response = ProviderListResponse::from_provider_specs(
            "claude",
            vec![ProviderSpec {
                id: "provider-a".to_string(),
                name: "Provider A".to_string(),
                kind: ProviderKind::OpenRouter,
                account_ref: None,
                metadata: ProviderMetadata {
                    labels: vec![],
                    raw: json!({
                        "category": "aggregator",
                        "sortIndex": 3,
                        "icon": "openrouter",
                        "providerType": "openai_compatible",
                        "apiKey": "must-not-leak"
                    }),
                },
            }],
            Some("provider-a"),
            &["provider-a".to_string()],
            &["provider-a".to_string()],
        );

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["providers"][0]["id"], "provider-a");
        assert_eq!(value["providers"][0]["category"], "aggregator");
        assert_eq!(value["providers"][0]["sortIndex"], 3);
        assert_eq!(value["providers"][0]["current"], true);
        assert_eq!(value["providers"][0]["inFailoverQueue"], true);
        assert_eq!(value["providers"][0]["routeCandidate"], true);
        assert!(value["providers"][0].get("apiKey").is_none());
    }

    #[test]
    fn current_route_provider_summary_input_projects_from_provider_spec_metadata() {
        let input = CurrentRouteProviderSummaryInput::from_provider_spec(ProviderSpec {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            kind: ProviderKind::OpenRouter,
            account_ref: None,
            metadata: ProviderMetadata {
                labels: Vec::new(),
                raw: json!({
                    "category": "aggregator",
                    "sortIndex": 7,
                    "providerType": "openai_compatible"
                }),
            },
        });

        assert_eq!(
            input,
            CurrentRouteProviderSummaryInput::new(
                "provider-a",
                "Provider A",
                Some("aggregator".to_string()),
            )
        );
    }

    #[test]
    fn provider_list_response_marks_failover_and_route_candidates() {
        let response = ProviderListResponse::from_provider_inputs(
            "claude",
            vec![
                ProviderSummaryInput {
                    id: "provider-a".to_string(),
                    name: "Provider A".to_string(),
                    category: None,
                    sort_index: None,
                    icon: None,
                    icon_color: None,
                    provider_type: None,
                },
                ProviderSummaryInput {
                    id: "provider-b".to_string(),
                    name: "Provider B".to_string(),
                    category: None,
                    sort_index: None,
                    icon: None,
                    icon_color: None,
                    provider_type: None,
                },
            ],
            Some("provider-b"),
            &["provider-a".to_string()],
            &["provider-b".to_string()],
        );

        assert_eq!(response.providers.len(), 2);
        assert!(response.providers[0].in_failover_queue);
        assert!(!response.providers[0].current);
        assert!(!response.providers[0].route_candidate);
        assert!(response.providers[1].current);
        assert!(!response.providers[1].in_failover_queue);
        assert!(response.providers[1].route_candidate);
    }

    #[test]
    fn app_channel_response_serializes_unfiltered_list_envelope() {
        let response: AppChannelResponse<_, serde_json::Value, serde_json::Value> =
            AppChannelResponse::List(AppChannelListResponse::from_route_source(
                "claude",
                &ChannelRouteSource::MaterializedChannels,
                vec![json!({
                    "id": "channel-a",
                    "name": "Primary"
                })],
            ));

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["source"], "materialized_channels");
        assert_eq!(value["channels"][0]["id"], "channel-a");
        assert!(value.get("rejected").is_none());
    }

    #[test]
    fn app_channel_response_serializes_route_filter_envelope() {
        let response: AppChannelResponse<serde_json::Value, _, _> =
            AppChannelResponse::Route(AppChannelRouteResponse::from_route_resolve(
                RouteResolveResponse {
                    app_type: "claude".to_string(),
                    requested_model: Some("sonnet".to_string()),
                    interface_kind: Some("anthropic_messages".to_string()),
                    route_group: "default".to_string(),
                    source: ChannelRouteSource::LegacyProjection,
                    candidates: vec![ChannelRouteCandidate {
                        channel_id: "channel-a".to_string(),
                        provider_id: "provider-a".to_string(),
                        channel_name: "Primary".to_string(),
                        base_url: "https://primary.example.com/v1".to_string(),
                        interface_kind: "anthropic_messages".to_string(),
                        public_model: Some("sonnet".to_string()),
                        upstream_model: Some("claude-sonnet".to_string()),
                        route_group: "default".to_string(),
                        priority: 10,
                        weight: 100,
                        source_kind: "legacy_primary".to_string(),
                    }],
                    rejected: vec![ChannelRouteRejected {
                        channel_id: "channel-b".to_string(),
                        provider_id: "provider-b".to_string(),
                        channel_name: "Secondary".to_string(),
                        reasons: vec!["model_unavailable:sonnet".to_string()],
                    }],
                },
            ));

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["source"], "legacy_projection");
        assert_eq!(value["requestedModel"], "sonnet");
        assert_eq!(value["interfaceKind"], "anthropic_messages");
        assert_eq!(value["routeGroup"], "default");
        assert_eq!(value["channels"][0]["channelId"], "channel-a");
        assert_eq!(value["rejected"][0]["channelId"], "channel-b");
    }

    #[test]
    fn current_route_response_serializes_runtime_target_envelope() {
        let response = CurrentRouteResponse::from_inputs(
            "claude",
            Some(CurrentRouteTarget {
                app_type: "claude".to_string(),
                provider_name: "Provider A".to_string(),
                provider_id: "provider-a".to_string(),
                channel_id: Some("channel-a".to_string()),
                channel_name: Some("Relay A".to_string()),
                interface_kind: Some("openai_responses".to_string()),
                public_model: Some("public-sonnet".to_string()),
                upstream_model: Some("upstream-sonnet".to_string()),
            }),
            Some(CurrentRouteProviderSummaryInput::new(
                "provider-a",
                "Provider A",
                Some("aggregator".to_string()),
            )),
        );

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["active"], true);
        assert_eq!(value["target"]["appType"], "claude");
        assert_eq!(value["target"]["providerId"], "provider-a");
        assert_eq!(value["target"]["providerName"], "Provider A");
        assert_eq!(value["configuredProvider"]["id"], "provider-a");
        assert_eq!(value["configuredProvider"]["category"], "aggregator");
        assert_eq!(value["target"]["interfaceKind"], "openai_responses");
        assert_eq!(value["target"]["publicModel"], "public-sonnet");
        assert_eq!(value["target"]["upstreamModel"], "upstream-sonnet");
    }

    #[test]
    fn current_route_response_serializes_inactive_target_as_null() {
        let response: CurrentRouteResponse<serde_json::Value> =
            CurrentRouteResponse::from_inputs("claude", None, None);

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["active"], false);
        assert!(value["target"].is_null());
        assert!(value["configuredProvider"].is_null());
    }

    #[test]
    fn channel_migration_preview_response_serializes_management_envelope() {
        let response = ChannelMigrationPreviewResponse::from_input(
            ChannelMigrationPreviewInput::new(
            "claude",
            vec![json!({
                "id": "channel-a",
                "needsReview": false
            })],
            1,
            0,
        ));

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["channels"][0]["id"], "channel-a");
        assert_eq!(value["duplicateCount"], 1);
        assert_eq!(value["needsReviewCount"], 0);
    }

    #[test]
    fn channel_migration_materialize_response_serializes_management_envelope() {
        let response = ChannelMigrationMaterializeResponse::from_input(
            ChannelMigrationMaterializeInput::new("claude", 2, 1, 3, 1, 1, 0),
        );

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["previewedChannels"], 2);
        assert_eq!(value["insertedChannels"], 1);
        assert_eq!(value["insertedModels"], 3);
        assert_eq!(value["insertedHealthRows"], 1);
        assert_eq!(value["duplicateCount"], 1);
        assert_eq!(value["needsReviewCount"], 0);
    }

    #[test]
    fn channel_record_input_builds_management_record_contract() {
        let record = channel_record_from_input(ChannelRecordInput {
            id: "ch-1".to_string(),
            provider_id: "provider-1".to_string(),
            app_type: "claude".to_string(),
            name: "Relay A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_responses".to_string(),
            auth_profile_ref: Some("channel-key:primary".to_string()),
            groups: vec!["default".to_string(), "beta".to_string()],
            priority: 10,
            weight: 80,
            retry_policy: json!({"maxAttempts": 2}),
            health_policy: json!({"mode": "http"}),
            header_overrides: json!({"x-relay": "1"}),
            param_overrides: json!({"api-version": "2026-06-20"}),
            status_code_mapping: json!([{"from": 429, "to": "rate_limited"}]),
            tags: vec!["manual".to_string()],
            metadata: json!({"owner": "ops"}),
            source_kind: "manual".to_string(),
            source_endpoint_url: Some("https://relay.example.com/v1".to_string()),
            models: vec![ChannelModelRecordInput {
                channel_id: "ch-1".to_string(),
                public_model: "sonnet".to_string(),
                upstream_model: "anthropic/sonnet".to_string(),
                capabilities: json!({"tools": true}),
                pricing_model: Some("standard".to_string()),
                request_overrides: json!({"temperature": 0.2}),
                response_overrides: json!({}),
            }],
            needs_review: true,
            review_reasons: vec!["missing-auth".to_string()],
        });

        assert_eq!(record.id, "ch-1");
        assert_eq!(
            record.auth_profile_ref.as_deref(),
            Some("channel-key:primary")
        );
        assert_eq!(
            record.groups,
            vec!["default".to_string(), "beta".to_string()]
        );
        assert_eq!(record.retry_policy, json!({"maxAttempts": 2}));
        assert_eq!(record.status_code_mapping[0]["to"], "rate_limited");
        assert_eq!(record.models.len(), 1);
        assert_eq!(record.models[0].public_model, "sonnet");
        assert_eq!(record.models[0].capabilities, json!({"tools": true}));
        assert!(record.needs_review);
        assert_eq!(record.review_reasons, vec!["missing-auth".to_string()]);

        let model = channel_model_record_from_input(ChannelModelRecordInput {
            channel_id: "ch-2".to_string(),
            public_model: "haiku".to_string(),
            upstream_model: "anthropic/haiku".to_string(),
            capabilities: json!({}),
            pricing_model: None,
            request_overrides: json!({}),
            response_overrides: json!({}),
        });
        assert_eq!(model.channel_id, "ch-2");
        assert_eq!(model.public_model, "haiku");
    }

    #[test]
    fn channel_key_record_input_builds_management_key_contract() {
        let record = channel_key_record_from_input(ChannelKeyRecordInput {
            channel_id: "ch-1".to_string(),
            key_ref: "primary".to_string(),
            status: "enabled".to_string(),
            priority: 5,
            weight: 60,
            last_failure_at: Some(1_771_000_003),
        });

        assert_eq!(record.channel_id, "ch-1");
        assert_eq!(record.key_ref, "primary");
        assert_eq!(record.status, "enabled");
        assert_eq!(record.priority, 5);
        assert_eq!(record.weight, 60);
        assert_eq!(record.last_failure_at, Some(1_771_000_003));
    }

    #[test]
    fn route_resolve_response_serializes_management_contract() {
        let response = RouteResolveResponse {
            app_type: "claude".to_string(),
            requested_model: Some("sonnet".to_string()),
            interface_kind: Some("anthropic_messages".to_string()),
            route_group: "default".to_string(),
            source: ChannelRouteSource::MaterializedChannels,
            candidates: vec![ChannelRouteCandidate {
                channel_id: "channel-a".to_string(),
                provider_id: "provider-a".to_string(),
                channel_name: "Primary".to_string(),
                base_url: "https://primary.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                public_model: Some("sonnet".to_string()),
                upstream_model: Some("claude-sonnet".to_string()),
                route_group: "default".to_string(),
                priority: 10,
                weight: 100,
                source_kind: "manual".to_string(),
            }],
            rejected: vec![ChannelRouteRejected {
                channel_id: "channel-b".to_string(),
                provider_id: "provider-b".to_string(),
                channel_name: "Backup".to_string(),
                reasons: vec!["model_unavailable:sonnet".to_string()],
            }],
        };

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["requestedModel"], "sonnet");
        assert_eq!(value["interfaceKind"], "anthropic_messages");
        assert_eq!(value["source"], "materialized_channels");
        assert_eq!(value["candidates"][0]["channelId"], "channel-a");
        assert_eq!(value["rejected"][0]["reasons"][0], "model_unavailable:sonnet");
    }

    #[test]
    fn channel_delete_response_serializes_management_envelope() {
        let response = ChannelDeleteResponse::new("channel-a", true);

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(
            value,
            json!({
                "channelId": "channel-a",
                "deleted": true
            })
        );
    }

    #[test]
    fn channel_list_response_serializes_management_envelope() {
        let response = ChannelListResponse::new(vec![json!({
            "id": "channel-a",
            "name": "Primary"
        })]);

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["channels"][0]["id"], "channel-a");
        assert_eq!(value["channels"][0]["name"], "Primary");
    }

    #[test]
    fn channel_record_response_serializes_as_raw_record() {
        let response = ChannelRecordResponse::new(ChannelRecord {
            id: "channel-a".to_string(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Primary".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_responses".to_string(),
            auth_profile_ref: None,
            groups: vec!["default".to_string()],
            priority: 10,
            weight: 100,
            retry_policy: json!({}),
            health_policy: json!({}),
            header_overrides: json!({}),
            param_overrides: json!({}),
            status_code_mapping: json!([]),
            tags: vec!["paid".to_string()],
            metadata: json!({"region": "us"}),
            source_kind: "manual".to_string(),
            source_endpoint_url: None,
            models: vec![ChannelModelRecord::from_model_route(
                "channel-a",
                ModelRoute {
                    public_model: "sonnet".to_string(),
                    upstream_model: "upstream-sonnet".to_string(),
                    capabilities: ModelCapabilities::default(),
                    pricing_model: None,
                    request_overrides: json!({}),
                    response_overrides: json!({}),
                },
            )],
            needs_review: false,
            review_reasons: vec![],
        });

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["id"], "channel-a");
        assert_eq!(value["providerId"], "provider-a");
        assert_eq!(value["appType"], "claude");
        assert_eq!(value["authProfileRef"], serde_json::Value::Null);
        assert_eq!(value["baseUrl"], "https://relay.example.com/v1");
        assert_eq!(value["interfaceKind"], "openai_responses");
        assert_eq!(value["sourceKind"], "manual");
        assert_eq!(value["sourceEndpointUrl"], serde_json::Value::Null);
        assert_eq!(value["models"][0]["publicModel"], "sonnet");
    }

    #[test]
    fn channel_models_response_serializes_management_envelope() {
        let response = ChannelModelsResponse::new(
            "channel-a",
            vec![ChannelModelRecord::from_model_route(
                "channel-a",
                ModelRoute {
                    public_model: "sonnet".to_string(),
                    upstream_model: "upstream-sonnet".to_string(),
                    capabilities: ModelCapabilities {
                        raw: json!({"toolUse": true}),
                    },
                    pricing_model: None,
                    request_overrides: json!({"temperature": 0.2}),
                    response_overrides: json!({"strip": ["metadata"]}),
                },
            )],
        );

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["channelId"], "channel-a");
        assert_eq!(value["models"][0]["channelId"], "channel-a");
        assert_eq!(value["models"][0]["publicModel"], "sonnet");
        assert_eq!(value["models"][0]["upstreamModel"], "upstream-sonnet");
        assert_eq!(value["models"][0]["capabilities"]["toolUse"], true);
        assert!(value["models"][0]["pricingModel"].is_null());
        assert_eq!(value["models"][0]["requestOverrides"]["temperature"], 0.2);
        assert_eq!(value["models"][0]["responseOverrides"]["strip"][0], "metadata");
    }

    #[test]
    fn route_group_list_response_aggregates_groups_and_sources() {
        let response = RouteGroupListResponse::from_sources(
            Some("claude".to_string()),
            vec![
                RouteGroupSourceInput::from_route_source(
                    "claude",
                    &ChannelRouteSource::MaterializedChannels,
                    vec![vec![], vec!["beta".to_string()]],
                ),
                RouteGroupSourceInput::from_route_source(
                    "codex",
                    &ChannelRouteSource::LegacyProjection,
                    vec![vec!["default".to_string(), "paid".to_string()]],
                ),
            ],
        );

        assert_eq!(response.app_type.as_deref(), Some("claude"));
        assert_eq!(
            response.sources,
            vec![
                "legacy_projection".to_string(),
                "materialized_channels".to_string()
            ]
        );
        assert_eq!(response.groups.len(), 3);
        assert_eq!(response.groups[0].name, "beta");
        assert_eq!(response.groups[0].app_types, vec!["claude"]);
        assert_eq!(response.groups[0].channel_count, 1);
        assert_eq!(response.groups[1].name, "default");
        assert_eq!(response.groups[1].app_types, vec!["claude", "codex"]);
        assert_eq!(response.groups[1].channel_count, 2);
        assert_eq!(response.groups[2].name, "paid");
        assert_eq!(response.groups[2].app_types, vec!["codex"]);

        let value = serde_json::to_value(response).expect("serialize response");
        assert_eq!(value["appType"], "claude");
        assert_eq!(value["groups"][1]["channelCount"], 2);
    }

    #[test]
    fn route_group_source_input_projects_from_channel_specs() {
        let input = RouteGroupSourceInput::from_channel_specs(
            "claude",
            &ChannelRouteSource::MaterializedChannels,
            vec![
                route_group_channel_spec("channel-a", AppKind::Claude, vec![]),
                route_group_channel_spec(
                    "channel-b",
                    AppKind::Claude,
                    vec!["beta".to_string(), "paid".to_string()],
                ),
            ],
        );
        let response =
            RouteGroupListResponse::from_sources(Some("claude".to_string()), vec![input]);

        assert_eq!(
            response
                .groups
                .iter()
                .map(|group| (group.name.as_str(), group.channel_count))
                .collect::<Vec<_>>(),
            vec![("beta", 1), ("default", 1), ("paid", 1)]
        );
    }
}
