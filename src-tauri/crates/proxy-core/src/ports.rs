use super::domain::{
    AppKind, AuthProfileRef, ChannelAttemptResult, ChannelQuery, ChannelSpec, InterfaceKind,
    ProviderSpec, ProxyRequest, ProxyResult, RoutePlan, RoutePolicy, RouteRequest, UsageRecord,
    DEFAULT_ROUTE_GROUP,
};
use super::error::ProxyCoreResult;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub provider_id: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelHealthReset {
    pub channel_id: String,
    pub app: AppKind,
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

#[cfg(test)]
mod tests {
    use super::{
        AppChannelListQuery, AppChannelListResponse, AppChannelResponse, AppChannelRouteResponse,
        AppListResponse, AppModelListQuery, AppSummaryInput, ChannelDeleteResponse,
        ChannelListQuery, ChannelListResponse, ChannelModelsResponse,
        ChannelMigrationMaterializeInput, ChannelMigrationMaterializeResponse,
        ChannelMigrationPreviewInput, ChannelMigrationPreviewResponse,
        ChannelRouteCandidate, ChannelRouteRejected, ChannelRouteSource,
        CurrentRouteProviderSummaryInput, CurrentRouteResponse, GroupListQuery,
        HealthCheckResponse, ProviderListResponse, ProviderSpec, ProviderSummaryInput,
        ProxyChannelModelWriteRequest, ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest,
        ProxyChannelWriteRequest, RouteGroupListResponse, RouteGroupSourceInput,
        RouteResolveResponse,
    };
    use crate::{ProviderKind, ProviderMetadata};
    use serde_json::json;

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
            Some(json!({
                "providerId": "provider-a",
                "channelId": "channel-a"
            })),
            Some(CurrentRouteProviderSummaryInput::new(
                "provider-a",
                "Provider A",
                Some("aggregator".to_string()),
            )),
        );

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["active"], true);
        assert_eq!(value["target"]["providerId"], "provider-a");
        assert_eq!(value["configuredProvider"]["id"], "provider-a");
        assert_eq!(value["configuredProvider"]["category"], "aggregator");
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
    fn channel_models_response_serializes_management_envelope() {
        let response = ChannelModelsResponse::new(
            "channel-a",
            vec![json!({
                "publicModel": "sonnet",
                "upstreamModel": "upstream-sonnet"
            })],
        );

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["channelId"], "channel-a");
        assert_eq!(value["models"][0]["publicModel"], "sonnet");
        assert_eq!(value["models"][0]["upstreamModel"], "upstream-sonnet");
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
}
