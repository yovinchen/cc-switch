use crate::app_config::AppType;
use crate::database::Database;
use crate::error::AppError;
use crate::proxy::provider_router::ProviderRouter;
use crate::proxy_core::{
    AppKind, AuthInfo, AuthProfileRef, ChannelAttemptPlan, ChannelAttemptResult, ChannelQuery,
    ChannelSource, ChannelSpec, ChannelStatus, CopilotOptimizerConfigSpec, ModelCatalog,
    OptimizerConfigSpec, ProviderSource, ProviderSpec, ProxyAppConfig, ProxyConfigSource,
    ProxyCoreError, ProxyCoreEvent, ProxyCoreResult, ProxyEventSink, ProxyGlobalConfig,
    ProxyRequest, ProxyRuntimeConfig, ProxyServices, RectifierConfigSpec, RoutePlan, RoutePolicy,
    RoutePolicySource, RouteRequest, RouteResolver, UsageHint, UsageSink,
};
use crate::proxy_core_adapter::{ToProxyCoreChannelSpec, ToProxyCoreProviderSpec};
use futures::future::BoxFuture;
use serde_json::{json, Value};
use std::str::FromStr;
use std::sync::Arc;

const DEFAULT_ROUTE_GROUP: &str = "default";
const DEFAULT_CHANNEL_FAILURE_THRESHOLD: u32 = 4;

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct CcSwitchProxyServices {
    config: CcSwitchConfigSource,
    providers: CcSwitchProviderSource,
    channels: CcSwitchChannelSource,
    route_policies: CcSwitchRoutePolicySource,
    route_resolver: CcSwitchRouteResolver,
    health_store: CcSwitchHealthStore,
    auth_provider: CcSwitchAuthProvider,
    model_catalog: CcSwitchModelCatalogProvider,
    usage_sink: CcSwitchUsageSink,
    event_sink: CcSwitchEventSink,
}

#[allow(dead_code)]
impl CcSwitchProxyServices {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        let router = Arc::new(ProviderRouter::new(db.clone()));
        Self {
            config: CcSwitchConfigSource { db: db.clone() },
            providers: CcSwitchProviderSource { db: db.clone() },
            channels: CcSwitchChannelSource {
                db: db.clone(),
                router: router.clone(),
            },
            route_policies: CcSwitchRoutePolicySource { db: db.clone() },
            route_resolver: CcSwitchRouteResolver,
            health_store: CcSwitchHealthStore { db: db.clone() },
            auth_provider: CcSwitchAuthProvider,
            model_catalog: CcSwitchModelCatalogProvider { db: db.clone() },
            usage_sink: CcSwitchUsageSink,
            event_sink: CcSwitchEventSink,
        }
    }
}

impl ProxyServices for CcSwitchProxyServices {
    fn config(&self) -> &(dyn ProxyConfigSource + Send + Sync) {
        &self.config
    }

    fn providers(&self) -> &(dyn ProviderSource + Send + Sync) {
        &self.providers
    }

    fn channels(&self) -> &(dyn ChannelSource + Send + Sync) {
        &self.channels
    }

    fn route_policies(&self) -> &(dyn RoutePolicySource + Send + Sync) {
        &self.route_policies
    }

    fn route_resolver(&self) -> &(dyn RouteResolver + Send + Sync) {
        &self.route_resolver
    }

    fn health_store(&self) -> &(dyn crate::proxy_core::ChannelHealthStore + Send + Sync) {
        &self.health_store
    }

    fn auth_provider(&self) -> &(dyn crate::proxy_core::AuthProvider + Send + Sync) {
        &self.auth_provider
    }

    fn model_catalog(&self) -> &(dyn crate::proxy_core::ModelCatalogProvider + Send + Sync) {
        &self.model_catalog
    }

    fn usage_sink(&self) -> &(dyn UsageSink + Send + Sync) {
        &self.usage_sink
    }

    fn event_sink(&self) -> &(dyn ProxyEventSink + Send + Sync) {
        &self.event_sink
    }
}

#[derive(Clone)]
struct CcSwitchConfigSource {
    db: Arc<Database>,
}

impl ProxyConfigSource for CcSwitchConfigSource {
    fn load_global<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyGlobalConfig>> {
        Box::pin(async move {
            let config = self
                .db
                .get_global_proxy_config()
                .await
                .map_err(|error| app_error("load global proxy config", error))?;
            Ok(ProxyGlobalConfig {
                bind_host: Some(config.listen_address.clone()),
                bind_port: Some(config.listen_port),
                request_timeout_ms: None,
                raw: serde_json::to_value(config).unwrap_or_else(|_| json!({})),
            })
        })
    }

    fn load_app<'a>(&'a self, app: &'a AppKind) -> BoxFuture<'a, ProxyCoreResult<ProxyAppConfig>> {
        Box::pin(async move {
            let app_type = AppType::from_str(app.as_str()).ok();
            let config = self
                .db
                .get_proxy_config_for_app(app.as_str())
                .await
                .map_err(|error| app_error("load app proxy config", error))?;
            let rectifier = self.db.get_rectifier_config().unwrap_or_default();
            let optimizer = self.db.get_optimizer_config().unwrap_or_default();
            let copilot_optimizer = self.db.get_copilot_optimizer_config().unwrap_or_default();
            let current_provider_id = app_type
                .as_ref()
                .and_then(crate::settings::get_current_provider);
            let raw = app_config_raw(config.clone(), current_provider_id);

            Ok(ProxyAppConfig {
                app: Some(app.clone()),
                enabled: config.enabled,
                default_group: Some(DEFAULT_ROUTE_GROUP.to_string()),
                rectifier: RectifierConfigSpec {
                    enabled: rectifier.enabled,
                    raw: serde_json::to_value(rectifier).unwrap_or_else(|_| json!({})),
                },
                optimizer: OptimizerConfigSpec {
                    enabled: optimizer.enabled,
                    raw: serde_json::to_value(optimizer).unwrap_or_else(|_| json!({})),
                },
                copilot_optimizer: CopilotOptimizerConfigSpec {
                    enabled: copilot_optimizer.enabled,
                    raw: serde_json::to_value(copilot_optimizer).unwrap_or_else(|_| json!({})),
                },
                raw,
            })
        })
    }

    fn load_runtime<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeConfig>> {
        Box::pin(async move {
            let config = self
                .db
                .get_proxy_config()
                .await
                .map_err(|error| app_error("load runtime proxy config", error))?;
            Ok(ProxyRuntimeConfig {
                privacy_filter_enabled: false,
                route_events_enabled: config.enable_logging,
                raw: serde_json::to_value(config).unwrap_or_else(|_| json!({})),
            })
        })
    }
}

#[derive(Clone)]
struct CcSwitchProviderSource {
    db: Arc<Database>,
}

impl ProviderSource for CcSwitchProviderSource {
    fn list_providers<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ProviderSpec>>> {
        Box::pin(async move {
            let app_type = parse_app_type(app)?;
            let providers = self
                .db
                .get_all_providers(app.as_str())
                .map_err(|error| app_error("list providers", error))?;
            Ok(providers
                .values()
                .map(|provider| provider.to_proxy_core_provider_spec(&app_type))
                .collect())
        })
    }

    fn get_provider<'a>(
        &'a self,
        app: &'a AppKind,
        provider_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ProviderSpec>>> {
        Box::pin(async move {
            let app_type = parse_app_type(app)?;
            let provider = self
                .db
                .get_provider_by_id(provider_id, app.as_str())
                .map_err(|error| app_error("get provider", error))?;
            Ok(provider.map(|provider| provider.to_proxy_core_provider_spec(&app_type)))
        })
    }
}

#[derive(Clone)]
struct CcSwitchChannelSource {
    db: Arc<Database>,
    router: Arc<ProviderRouter>,
}

impl ChannelSource for CcSwitchChannelSource {
    fn list_channels<'a>(
        &'a self,
        query: ChannelQuery<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelSpec>>> {
        Box::pin(async move {
            let (channels, _) = self
                .router
                .list_channels_for_app(query.app.as_str())
                .await
                .map_err(|error| app_error("list channels", error))?;
            let channels = channels
                .into_iter()
                .map(|channel| channel.to_proxy_core_channel_spec())
                .filter(|channel| channel_matches_query(channel, &query))
                .collect();
            Ok(channels)
        })
    }

    fn get_channel<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelSpec>>> {
        Box::pin(async move {
            let channel = self
                .db
                .get_proxy_channel(channel_id)
                .map_err(|error| app_error("get channel", error))?;
            Ok(channel.map(|channel| channel.to_proxy_core_channel_spec()))
        })
    }
}

#[derive(Clone)]
struct CcSwitchRoutePolicySource {
    db: Arc<Database>,
}

impl RoutePolicySource for CcSwitchRoutePolicySource {
    fn load_policy<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<RoutePolicy>>> {
        Box::pin(async move {
            let queue = self
                .db
                .get_failover_queue(app.as_str())
                .map_err(|error| app_error("load route policy", error))?;
            Ok(Some(RoutePolicy {
                app: app.clone(),
                groups: Vec::new(),
                raw: json!({
                    "defaultGroup": DEFAULT_ROUTE_GROUP,
                    "failoverProviderIds": queue.into_iter().map(|item| item.provider_id).collect::<Vec<_>>(),
                }),
            }))
        })
    }
}

#[derive(Clone, Default)]
struct CcSwitchRouteResolver;

impl RouteResolver for CcSwitchRouteResolver {
    fn resolve<'a>(
        &'a self,
        request: RouteRequest<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<RoutePlan>> {
        Box::pin(async move {
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
                if !channel_groups_match(&channel.groups, requested_group) {
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

                selections.push(crate::proxy_core::RouteSelection {
                    provider,
                    channel: channel.clone(),
                    model_route,
                    inbound_interface: request.request.inbound_interface.clone(),
                    outbound_interface: channel.interface.clone(),
                });
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
                attempts,
            })
        })
    }
}

#[derive(Clone)]
struct CcSwitchHealthStore {
    db: Arc<Database>,
}

impl crate::proxy_core::ChannelHealthStore for CcSwitchHealthStore {
    fn record_attempt<'a>(
        &'a self,
        result: ChannelAttemptResult,
    ) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move {
            self.db
                .update_proxy_channel_health_with_threshold(
                    &result.channel_id,
                    result.success,
                    result.error_code,
                    DEFAULT_CHANNEL_FAILURE_THRESHOLD,
                    result.latency_ms.map(|latency| latency as i64),
                )
                .map_err(|error| app_error("record channel attempt", error))
        })
    }

    fn reset_channel<'a>(&'a self, channel_id: &'a str) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move {
            self.db
                .reset_proxy_channel_health(channel_id)
                .map_err(|error| app_error("reset channel health", error))
        })
    }
}

#[derive(Clone, Default)]
struct CcSwitchAuthProvider;

impl crate::proxy_core::AuthProvider for CcSwitchAuthProvider {
    fn resolve_auth<'a>(
        &'a self,
        auth_profile: Option<&'a AuthProfileRef>,
        _request: &'a ProxyRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>> {
        Box::pin(async move {
            Ok(AuthInfo {
                headers: Vec::new(),
                account_ref: auth_profile.map(|value| value.0.clone()),
                metadata: json!({"source": "cc_switch_provider_config"}),
            })
        })
    }
}

#[derive(Clone)]
struct CcSwitchModelCatalogProvider {
    db: Arc<Database>,
}

impl crate::proxy_core::ModelCatalogProvider for CcSwitchModelCatalogProvider {
    fn load_catalog<'a>(
        &'a self,
        app: &'a AppKind,
        provider_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
        Box::pin(async move {
            let provider = self
                .db
                .get_provider_by_id(provider_id, app.as_str())
                .map_err(|error| app_error("load model catalog", error))?;
            let mut models = Vec::new();
            if let Some(provider) = provider {
                collect_models_from_value(&provider.settings_config, &mut models);
            }
            models.sort();
            models.dedup();
            Ok(ModelCatalog {
                provider_id: provider_id.to_string(),
                models,
                raw: Value::Object(Default::default()),
            })
        })
    }
}

#[derive(Clone, Default)]
struct CcSwitchUsageSink;

impl UsageSink for CcSwitchUsageSink {
    fn record_usage<'a>(&'a self, _hint: UsageHint) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move { Ok(()) })
    }
}

#[derive(Clone, Default)]
struct CcSwitchEventSink;

impl ProxyEventSink for CcSwitchEventSink {
    fn emit_event<'a>(&'a self, _event: ProxyCoreEvent) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move { Ok(()) })
    }
}

fn parse_app_type(app: &AppKind) -> ProxyCoreResult<AppType> {
    AppType::from_str(app.as_str())
        .map_err(|error| ProxyCoreError::Config(format!("unsupported app kind: {error}")))
}

fn app_error(context: &str, error: AppError) -> ProxyCoreError {
    ProxyCoreError::Config(format!("{context}: {error}"))
}

fn app_config_raw(
    config: crate::proxy::types::AppProxyConfig,
    current_provider_id: Option<String>,
) -> Value {
    let mut raw = serde_json::to_value(config).unwrap_or_else(|_| json!({}));
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

fn channel_matches_query(channel: &ChannelSpec, query: &ChannelQuery<'_>) -> bool {
    if !query.include_disabled && channel.status != ChannelStatus::Enabled {
        return false;
    }
    if let Some(provider_id) = query.provider_id {
        if channel.provider_id != provider_id {
            return false;
        }
    }
    if let Some(group) = query.group {
        if !channel_groups_match(&channel.groups, group) {
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

fn channel_groups_match(groups: &[String], requested_group: &str) -> bool {
    if groups.is_empty() {
        return requested_group == DEFAULT_ROUTE_GROUP;
    }
    groups.iter().any(|group| group == requested_group)
}

fn interfaces_compatible(
    requested: &crate::proxy_core::InterfaceKind,
    channel: &crate::proxy_core::InterfaceKind,
) -> bool {
    use crate::proxy_core::InterfaceKind;

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

fn collect_models_from_value(value: &Value, models: &mut Vec<String>) {
    if let Some(model) = value.get("model").and_then(Value::as_str) {
        push_model(models, model);
    }
    if let Some(env) = value.get("env").and_then(Value::as_object) {
        for key in [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_SMALL_FAST_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "GEMINI_MODEL",
        ] {
            if let Some(model) = env.get(key).and_then(Value::as_str) {
                push_model(models, model);
            }
        }
    }
    if let Some(catalog_models) = value
        .get("modelCatalog")
        .and_then(|catalog| catalog.get("models"))
        .and_then(Value::as_array)
    {
        for entry in catalog_models {
            if let Some(model) = entry
                .get("model")
                .or_else(|| entry.get("id"))
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)
            {
                push_model(models, model);
            }
        }
    }
}

fn push_model(models: &mut Vec<String>, model: &str) {
    let model = model.trim();
    if !model.is_empty() {
        models.push(model.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::proxy_core::{
        ChannelOverrides, InterfaceKind, ModelCapabilities, ModelRoute, ProviderKind, ProxyBody,
        RetryPolicy, UpstreamEndpoint,
    };
    use http::{Method, StatusCode};

    fn save_claude_provider(db: &Database) {
        let provider = Provider::with_id(
            "anthropic-main".to_string(),
            "Anthropic Main".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay-a.example.com/v1/",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider)
            .expect("save provider");
        db.set_current_provider("claude", "anthropic-main")
            .expect("set current provider");
    }

    fn provider_spec(id: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: id.to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: Default::default(),
        }
    }

    fn channel_spec(id: &str, priority: i64, model: &str) -> ChannelSpec {
        ChannelSpec {
            id: id.to_string(),
            provider_id: "provider-a".to_string(),
            app: AppKind::Claude,
            name: id.to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: format!("https://{id}.example.com/v1"),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::OpenAiResponses,
            auth_profile: None,
            models: vec![ModelRoute {
                public_model: model.to_string(),
                upstream_model: format!("upstream-{model}"),
                capabilities: ModelCapabilities::default(),
                pricing_model: None,
                request_overrides: json!({}),
                response_overrides: json!({}),
            }],
            groups: vec![DEFAULT_ROUTE_GROUP.to_string()],
            priority,
            weight: 100,
            retry_policy: RetryPolicy::default(),
            health_policy: Default::default(),
            overrides: ChannelOverrides::default(),
            tags: Vec::new(),
            metadata: json!({}),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        }
    }

    #[tokio::test]
    async fn channel_source_projects_legacy_provider_channels_through_ports() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        let services = CcSwitchProxyServices::new(db);

        let channels = services
            .channels()
            .list_channels(ChannelQuery {
                app: &AppKind::Claude,
                provider_id: Some("anthropic-main"),
                model: Some("claude-sonnet-4"),
                group: Some(DEFAULT_ROUTE_GROUP),
                include_disabled: false,
            })
            .await
            .expect("list channels");

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].provider_id, "anthropic-main");
        assert_eq!(
            channels[0].endpoint.base_url,
            "https://relay-a.example.com/v1"
        );
        assert_eq!(channels[0].models[0].public_model, "claude-sonnet-4");
    }

    #[tokio::test]
    async fn route_resolver_selects_highest_priority_matching_channel() {
        let services = CcSwitchProxyServices::new(Arc::new(Database::memory().expect("memory db")));
        let providers = vec![provider_spec("provider-a")];
        let channels = vec![
            channel_spec("low", 1, "sonnet"),
            channel_spec("high", 10, "sonnet"),
        ];
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({})),
        );
        request.requested_model = Some("sonnet".to_string());

        let plan = services
            .route_resolver()
            .resolve(RouteRequest {
                request: &request,
                providers: &providers,
                channels: &channels,
                policy: None,
            })
            .await
            .expect("resolve route");

        assert_eq!(plan.selection.channel.id, "high");
        assert_eq!(
            plan.selection
                .model_route
                .as_ref()
                .map(|route| route.upstream_model.as_str()),
            Some("upstream-sonnet")
        );
        assert_eq!(plan.attempts.len(), 2);
    }

    #[tokio::test]
    async fn health_store_records_channel_attempts_in_database() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        db.materialize_legacy_proxy_channels("claude")
            .expect("materialize channels");
        let channel_id = db
            .list_proxy_channels_for_app("claude")
            .expect("list channels")
            .first()
            .expect("channel")
            .id
            .clone();
        let services = CcSwitchProxyServices::new(db.clone());

        services
            .health_store()
            .record_attempt(ChannelAttemptResult {
                channel_id: channel_id.clone(),
                success: false,
                status_code: Some(StatusCode::TOO_MANY_REQUESTS.as_u16()),
                latency_ms: Some(123),
                error_code: Some("rate_limited".to_string()),
            })
            .await
            .expect("record attempt");

        let health = db
            .get_proxy_channel_health(&channel_id)
            .expect("read channel health");
        assert_eq!(health.status, "degraded");
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.response_time_ms, Some(123));
    }
}
