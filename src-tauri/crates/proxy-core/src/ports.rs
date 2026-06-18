use super::domain::{
    AppKind, AuthProfileRef, ChannelAttemptResult, ChannelQuery, ChannelSpec, ProviderSpec,
    ProxyRequest, ProxyResult, RoutePlan, RoutePolicy, RouteRequest, UsageRecord,
};
use super::error::ProxyCoreResult;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

    fn reset_channel<'a>(&'a self, channel_id: &'a str) -> BoxFuture<'a, ProxyCoreResult<()>>;
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
