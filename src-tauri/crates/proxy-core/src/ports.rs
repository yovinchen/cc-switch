use super::domain::{
    AppKind, AuthProfileRef, ChannelAttemptResult, ChannelQuery, ChannelSpec, ProviderSpec,
    ProxyRequest, ProxyResult, RoutePlan, RoutePolicy, RouteRequest, UsageRecord,
    DEFAULT_ROUTE_GROUP,
};
use super::error::ProxyCoreResult;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
        AppChannelListResponse, AppChannelResponse, AppChannelRouteResponse, AppListResponse,
        AppSummary, ChannelDeleteResponse, ChannelListResponse, ChannelModelsResponse,
        ChannelMigrationMaterializeResponse, ChannelMigrationPreviewResponse,
        CurrentRouteProviderSummary, CurrentRouteResponse, ProviderListResponse, ProviderSummary,
        RouteGroupChannelInput, RouteGroupListResponse, RouteGroupSourceInput,
    };
    use serde_json::json;

    #[test]
    fn app_list_response_serializes_management_envelope() {
        let response = AppListResponse::new(vec![AppSummary::new("claude", true, false, 2, 3)]);

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
        let response = ProviderListResponse::new(
            "claude",
            vec![ProviderSummary {
                id: "provider-a".to_string(),
                name: "Provider A".to_string(),
                category: Some("aggregator".to_string()),
                sort_index: Some(1),
                icon: None,
                icon_color: Some("#00A67E".to_string()),
                provider_type: Some("openai_compatible".to_string()),
                current: true,
                in_failover_queue: false,
                route_candidate: true,
            }],
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
    fn app_channel_response_serializes_unfiltered_list_envelope() {
        let response: AppChannelResponse<_, serde_json::Value, serde_json::Value> =
            AppChannelResponse::List(AppChannelListResponse::new(
                "claude",
                "materialized_channels",
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
            AppChannelResponse::Route(AppChannelRouteResponse::new(
                "claude",
                "legacy_projection",
                Some("sonnet".to_string()),
                Some("anthropic_messages".to_string()),
                "default",
                vec![json!({
                    "channelId": "channel-a",
                    "upstreamModel": "claude-sonnet"
                })],
                vec![json!({
                    "channelId": "channel-b",
                    "reasons": ["model_unavailable:sonnet"]
                })],
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
        let response = CurrentRouteResponse::new(
            "claude",
            Some(json!({
                "providerId": "provider-a",
                "channelId": "channel-a"
            })),
            Some(CurrentRouteProviderSummary::new(
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
            CurrentRouteResponse::new("claude", None, None);

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["active"], false);
        assert!(value["target"].is_null());
        assert!(value["configuredProvider"].is_null());
    }

    #[test]
    fn channel_migration_preview_response_serializes_management_envelope() {
        let response = ChannelMigrationPreviewResponse::new(
            "claude",
            vec![json!({
                "id": "channel-a",
                "needsReview": false
            })],
            1,
            0,
        );

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["channels"][0]["id"], "channel-a");
        assert_eq!(value["duplicateCount"], 1);
        assert_eq!(value["needsReviewCount"], 0);
    }

    #[test]
    fn channel_migration_materialize_response_serializes_management_envelope() {
        let response =
            ChannelMigrationMaterializeResponse::new("claude", 2, 1, 3, 1, 1, 0);

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
                RouteGroupSourceInput::new(
                    "claude",
                    "materialized_channels",
                    vec![
                        RouteGroupChannelInput::new(vec![]),
                        RouteGroupChannelInput::new(vec!["beta".to_string()]),
                    ],
                ),
                RouteGroupSourceInput::new(
                    "codex",
                    "legacy_projection",
                    vec![RouteGroupChannelInput::new(vec![
                        "default".to_string(),
                        "paid".to_string(),
                    ])],
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
