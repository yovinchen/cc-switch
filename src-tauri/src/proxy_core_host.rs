use crate::app_config::AppType;
use crate::database::Database;
use crate::error::AppError;
use crate::proxy::error_mapper::forward_error_to_core_error;
use crate::proxy::events::ProxyEventBus;
use crate::proxy::failover_switch::FailoverSwitchManager;
use crate::proxy::hyper_client::ProxyResponse;
use crate::proxy::provider_router::ProviderRouter;
use crate::proxy::providers::codex_chat_history::CodexChatHistoryStore;
use crate::proxy::route_attempt::forward_attempts_from_route_plan;
use crate::proxy::usage::{RequestLog, UsageLogger};
use crate::proxy::RequestForwarder;
use crate::proxy_core::{
    app_proxy_config_raw, channel_matches_query, interfaces_compatible,
    resolve_usage_record_pricing_models, route_group_matches, route_plan_provider_ids,
    select_route_for_forward_result, token_usage_from_usage_record,
    usage_record_request_id_with_fallback, AppKind, AuthInfo, AuthProfileRef, ChannelAttemptPlan,
    ChannelAttemptResult, ChannelQuery, ChannelSource, ChannelSpec, ChannelStatus,
    CopilotOptimizerConfigSpec, CostCalculator, CurrentRouteTarget, ForwardPipeline,
    GeminiShadowStore, ModelCatalog, OptimizerConfigSpec, ProviderSource, ProviderSpec,
    ProxyAppConfig, ProxyConfigSource, ProxyCoreError, ProxyCoreEvent, ProxyCoreResponse,
    ProxyCoreResult, ProxyEventSink, ProxyGlobalConfig, ProxyRequest, ProxyResponseBody,
    ProxyResult, ProxyRuntimeConfig, ProxyRuntimeStatus, ProxyServices, RectifierConfigSpec,
    RoutePlan, RoutePolicy, RoutePolicySource, RouteRequest, RouteResolver, RouteSelection,
    UsageRecord, UsageSink, CLAUDE_API_FORMAT_METADATA_KEY, DEFAULT_ROUTE_GROUP,
};
use crate::proxy_core_adapter::extract_proxy_session_id;
use crate::proxy_core_adapter::{ToProxyCoreChannelSpec, ToProxyCoreProviderSpec};
use crate::services::usage_stats::is_placeholder_pricing_model;
use bytes::Bytes;
use futures::{future::BoxFuture, Stream, StreamExt};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

const DEFAULT_CHANNEL_FAILURE_THRESHOLD: u32 = 4;

#[derive(Clone)]
pub(crate) struct CcSwitchProxyRuntime {
    pub(crate) db: Arc<Database>,
    pub(crate) provider_router: Arc<ProviderRouter>,
    pub(crate) status: Arc<RwLock<ProxyRuntimeStatus>>,
    pub(crate) current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    pub(crate) events: Arc<ProxyEventBus>,
    pub(crate) gemini_shadow: Arc<GeminiShadowStore>,
    pub(crate) codex_chat_history: Arc<CodexChatHistoryStore>,
    pub(crate) failover_manager: Arc<FailoverSwitchManager>,
    pub(crate) app_handle: Option<tauri::AppHandle>,
}

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
    forward_pipeline: CcSwitchForwardPipeline,
}

#[allow(dead_code)]
impl CcSwitchProxyServices {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self::with_optional_event_bus(db, None)
    }

    pub(crate) fn with_event_bus(db: Arc<Database>, events: Arc<ProxyEventBus>) -> Self {
        Self::with_optional_event_bus(db, Some(events))
    }

    pub(crate) fn with_runtime(runtime: CcSwitchProxyRuntime) -> Self {
        let db = runtime.db.clone();
        Self {
            config: CcSwitchConfigSource { db: db.clone() },
            providers: CcSwitchProviderSource { db: db.clone() },
            channels: CcSwitchChannelSource {
                db: db.clone(),
                router: runtime.provider_router.clone(),
            },
            route_policies: CcSwitchRoutePolicySource { db: db.clone() },
            route_resolver: CcSwitchRouteResolver,
            health_store: CcSwitchHealthStore {
                db: db.clone(),
                router: runtime.provider_router.clone(),
            },
            auth_provider: CcSwitchAuthProvider,
            model_catalog: CcSwitchModelCatalogProvider { db: db.clone() },
            usage_sink: CcSwitchUsageSink { db },
            event_sink: CcSwitchEventSink {
                events: Some(runtime.events.clone()),
            },
            forward_pipeline: CcSwitchForwardPipeline::with_runtime(runtime),
        }
    }

    fn with_optional_event_bus(db: Arc<Database>, events: Option<Arc<ProxyEventBus>>) -> Self {
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
            health_store: CcSwitchHealthStore {
                db: db.clone(),
                router: router.clone(),
            },
            auth_provider: CcSwitchAuthProvider,
            model_catalog: CcSwitchModelCatalogProvider { db: db.clone() },
            usage_sink: CcSwitchUsageSink { db: db.clone() },
            event_sink: CcSwitchEventSink { events },
            forward_pipeline: CcSwitchForwardPipeline::default(),
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

    fn forward_pipeline(&self) -> &(dyn ForwardPipeline + Send + Sync) {
        &self.forward_pipeline
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
            let raw = app_proxy_config_raw(config.clone(), current_provider_id);

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
            let channels = if query.allow_legacy_projection {
                self.router
                    .list_channels_for_app(query.app.as_str())
                    .await
                    .map_err(|error| app_error("list channels", error))?
                    .0
            } else {
                self.db
                    .list_proxy_channels_for_app(query.app.as_str())
                    .map_err(|error| app_error("list materialized channels", error))?
            };
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
                selections,
                attempts,
            })
        })
    }
}

#[derive(Clone)]
struct CcSwitchHealthStore {
    db: Arc<Database>,
    router: Arc<ProviderRouter>,
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

    fn reset_channel<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<crate::proxy_core::ChannelHealthReset>> {
        Box::pin(async move {
            let app_type = self
                .db
                .get_proxy_channel_app_type(channel_id)
                .map_err(|error| app_error("lookup channel app", error))?
                .ok_or_else(|| crate::proxy_core::channel_not_found_error(channel_id))?;
            self.router
                .reset_channel_breaker(channel_id, &app_type)
                .await
                .map_err(|error| app_error("reset channel health", error))?;
            Ok(crate::proxy_core::ChannelHealthReset {
                channel_id: channel_id.to_string(),
                app: AppKind::from(app_type.as_str()),
            })
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
            Ok(crate::proxy_core::provider_model_catalog_from_settings(
                provider_id,
                provider.as_ref().map(|provider| &provider.settings_config),
            ))
        })
    }

    fn load_client_catalog<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
        Box::pin(async move {
            let raw = match app {
                AppKind::Codex => load_codex_client_model_catalog_raw(),
                _ => json!({"models": []}),
            };
            Ok(crate::proxy_core::client_model_catalog_from_raw(
                app.as_str(),
                raw,
            ))
        })
    }
}

#[derive(Clone)]
struct CcSwitchUsageSink {
    db: Arc<Database>,
}

impl UsageSink for CcSwitchUsageSink {
    fn record_usage<'a>(&'a self, record: UsageRecord) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move {
            let logger = UsageLogger::new(&self.db);
            let app_type = record.app.as_str().to_string();
            let (multiplier, pricing_model_source) = logger
                .resolve_pricing_config(&record.provider_id, &app_type)
                .await;
            let model_selection =
                resolve_usage_record_pricing_models(&record, &pricing_model_source);
            let usage = token_usage_from_usage_record(&record);
            let pricing = logger
                .get_model_pricing(&model_selection.pricing_model)
                .map_err(|error| usage_error("load model pricing", error))?;

            if pricing.is_none()
                && record.tokens.has_billable_tokens()
                && !is_placeholder_pricing_model(&model_selection.pricing_model)
            {
                log::warn!(
                    "[USG-002] 模型定价未找到，成本将记录为 0: {}",
                    model_selection.pricing_model
                );
            }

            let cost = CostCalculator::try_calculate_for_app(
                &app_type,
                &usage,
                pricing.as_ref(),
                multiplier,
            );
            let log = RequestLog {
                request_id: usage_record_request_id_with_fallback(&record, || {
                    uuid::Uuid::new_v4().to_string()
                }),
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
            };

            logger
                .log_request(&log)
                .map_err(|error| usage_error("record usage", error))
        })
    }
}

#[derive(Clone, Default)]
struct CcSwitchEventSink {
    events: Option<Arc<ProxyEventBus>>,
}

impl ProxyEventSink for CcSwitchEventSink {
    fn emit_event<'a>(&'a self, event: ProxyCoreEvent) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move {
            if let Some(events) = self.events.as_ref() {
                let event_name = event.event_type.event_name();
                events.emit(event_name, event.into_event_payload());
            }
            Ok(())
        })
    }
}

#[derive(Clone, Default)]
struct CcSwitchForwardPipeline {
    runtime: Option<CcSwitchProxyRuntime>,
}

impl CcSwitchForwardPipeline {
    fn with_runtime(runtime: CcSwitchProxyRuntime) -> Self {
        Self {
            runtime: Some(runtime),
        }
    }
}

impl ForwardPipeline for CcSwitchForwardPipeline {
    fn forward<'a>(
        &'a self,
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>> {
        Box::pin(async move {
            let runtime = self.runtime.as_ref().ok_or_else(|| {
                ProxyCoreError::Unsupported(
                    "cc-switch forwarding requires a proxy server runtime".to_string(),
                )
            })?;
            runtime.forward(request, plan).await
        })
    }
}

impl CcSwitchProxyRuntime {
    async fn forward(
        &self,
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> ProxyCoreResult<ProxyResult> {
        let ProxyRequest {
            app,
            method,
            endpoint,
            headers,
            extensions,
            body,
            ..
        } = request;
        let app_type = parse_app_type(&app)?;
        let body = body.into_json()?;
        let app_config = self
            .db
            .get_proxy_config_for_app(app_type.as_str())
            .await
            .map_err(|error| app_error("load app proxy config", error))?;
        let rectifier_config = self.db.get_rectifier_config().unwrap_or_default();
        let optimizer_config = self.db.get_optimizer_config().unwrap_or_default();
        let copilot_optimizer_config = self.db.get_copilot_optimizer_config().unwrap_or_default();
        let current_provider_id = crate::settings::get_current_provider(&app_type)
            .or_else(|| {
                self.db
                    .get_current_provider(app_type.as_str())
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
        let session_result = extract_proxy_session_id(&headers, &body, app_type.as_str());
        let providers = host_providers_for_plan(&self.db, &app_type, &plan)?;
        let attempts = forward_attempts_from_route_plan(&app_type, &providers, &plan);
        if attempts.is_empty() {
            return Err(ProxyCoreError::Unavailable(
                "route plan has no matching host providers".to_string(),
            ));
        }

        let (non_streaming_timeout, first_byte_timeout, idle_timeout) =
            if app_config.auto_failover_enabled {
                (
                    app_config.non_streaming_timeout as u64,
                    app_config.streaming_first_byte_timeout as u64,
                    app_config.streaming_idle_timeout as u64,
                )
            } else {
                (0, 0, 0)
            };
        let max_retries = if app_config.auto_failover_enabled {
            app_config.max_retries
        } else {
            0
        };

        let forwarder = RequestForwarder::new_preplanned(
            self.provider_router.clone(),
            non_streaming_timeout,
            self.status.clone(),
            self.current_providers.clone(),
            self.events.clone(),
            self.gemini_shadow.clone(),
            self.codex_chat_history.clone(),
            self.failover_manager.clone(),
            self.app_handle.clone(),
            current_provider_id,
            session_result.session_id,
            session_result.client_provided,
            first_byte_timeout,
            idle_timeout,
            rectifier_config,
            optimizer_config,
            copilot_optimizer_config,
            max_retries,
        );

        let result = forwarder
            .forward_with_preplanned_attempts(
                &app_type, method, &endpoint, body, headers, extensions, attempts,
            )
            .await
            .map_err(forward_error_to_core_error)?;
        Ok(forward_result_to_proxy_result(result, plan))
    }
}

fn host_providers_for_plan(
    db: &Database,
    app_type: &AppType,
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<crate::provider::Provider>> {
    let providers = db
        .get_all_providers(app_type.as_str())
        .map_err(|error| app_error("load host providers", error))?;
    let provider_ids = route_plan_provider_ids(plan);

    let matching: Vec<_> = provider_ids
        .into_iter()
        .filter_map(|provider_id| providers.get(&provider_id).cloned())
        .collect();
    if matching.is_empty() {
        return Err(ProxyCoreError::Unavailable(
            "route plan providers are not configured in host database".to_string(),
        ));
    }
    Ok(matching)
}

fn forward_result_to_proxy_result(
    mut result: crate::proxy::ForwardResult,
    plan: RoutePlan,
) -> ProxyResult {
    let selected_route = selected_route_for_forward_result(&result, &plan);
    let connection_guard = result.connection_guard.take();
    let mut metadata = Map::new();
    metadata.insert("hostProviderId".to_string(), json!(result.provider.id));
    metadata.insert("hostProviderName".to_string(), json!(result.provider.name));
    metadata.insert(
        CLAUDE_API_FORMAT_METADATA_KEY.to_string(),
        json!(result.claude_api_format),
    );
    metadata.insert(
        "selectedChannelId".to_string(),
        json!(result
            .selected_channel
            .as_ref()
            .map(|channel| channel.channel_id.clone())),
    );
    ProxyResult {
        response: proxy_response_to_core_response(result.response, connection_guard),
        selected_route,
        outbound_model: result.outbound_model,
        usage_record: None,
        metadata: Value::Object(metadata),
    }
}

fn selected_route_for_forward_result(
    result: &crate::proxy::ForwardResult,
    plan: &RoutePlan,
) -> RouteSelection {
    select_route_for_forward_result(
        plan,
        result
            .selected_channel
            .as_ref()
            .map(|channel| channel.channel_id.as_str()),
        &result.provider.id,
    )
}

fn proxy_response_to_core_response<G>(
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

fn parse_app_type(app: &AppKind) -> ProxyCoreResult<AppType> {
    AppType::from_str(app.as_str())
        .map_err(|error| ProxyCoreError::Config(format!("unsupported app kind: {error}")))
}

fn app_error(context: &str, error: AppError) -> ProxyCoreError {
    ProxyCoreError::Config(format!("{context}: {error}"))
}

fn usage_error(context: &str, error: AppError) -> ProxyCoreError {
    ProxyCoreError::Internal(format!("{context}: {error}"))
}

fn load_codex_client_model_catalog_raw() -> Value {
    let generated_path = crate::codex_config::get_codex_model_catalog_path();
    let active_catalog_path = match crate::codex_config::read_codex_config_text() {
        Ok(config_text) => {
            crate::codex_config::resolve_cc_switch_catalog_path(&config_text, &generated_path)
        }
        Err(_) => None,
    };

    if let Some(catalog_path) = active_catalog_path.as_ref().filter(|path| path.exists()) {
        let text = std::fs::read_to_string(catalog_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or_else(|_| json!({"models": []}))
    } else {
        if active_catalog_path.is_none() {
            log::debug!(
                "[models] stale guard: catalog not served (model_catalog_json not set to cc-switch catalog)"
            );
        }
        json!({"models": []})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::ProxyCoreEventType;
    use crate::provider::Provider;
    use crate::proxy_core::{
        ChannelOverrides, InterfaceKind, ModelCapabilities, ModelRoute, ProviderKind, ProxyBody,
        ProxyEngine, ProxyResponseBody, ProxyRuntimeStatus, RetryPolicy, RouteSelection,
        UpstreamEndpoint, UsageRecord, UsageTokens,
    };
    use bytes::Bytes;
    use futures::StreamExt;
    use http::{Method, StatusCode};
    use std::ffi::OsString;

    struct IsolatedTestHome {
        _dir: tempfile::TempDir,
        original_test_home: Option<OsString>,
    }

    impl IsolatedTestHome {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("temp home");
            let original_test_home = std::env::var_os("CC_SWITCH_TEST_HOME");
            std::env::set_var("CC_SWITCH_TEST_HOME", dir.path());
            Self {
                _dir: dir,
                original_test_home,
            }
        }
    }

    impl Drop for IsolatedTestHome {
        fn drop(&mut self) {
            match &self.original_test_home {
                Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

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

    fn route_plan(provider_id: &str, channel_id: &str) -> RoutePlan {
        let mut channel = channel_spec(channel_id, 100, "sonnet");
        channel.provider_id = provider_id.to_string();
        let model_route = channel.models.first().cloned();
        let selection = RouteSelection {
            provider: provider_spec(provider_id),
            channel,
            model_route,
            inbound_interface: InterfaceKind::AnthropicMessages,
            outbound_interface: InterfaceKind::OpenAiResponses,
        };
        RoutePlan {
            selection,
            selections: Vec::new(),
            attempts: Vec::new(),
        }
    }

    fn proxy_request() -> ProxyRequest {
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({ "model": "sonnet", "messages": [] })),
        );
        request.requested_model = Some("sonnet".to_string());
        request
    }

    fn runtime(db: Arc<Database>) -> CcSwitchProxyRuntime {
        let events = Arc::new(ProxyEventBus::default());
        CcSwitchProxyRuntime {
            db: db.clone(),
            provider_router: Arc::new(ProviderRouter::new(db.clone())),
            status: Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            current_providers: Arc::new(RwLock::new(std::collections::HashMap::new())),
            events,
            gemini_shadow: Arc::new(GeminiShadowStore::default()),
            codex_chat_history: Arc::new(CodexChatHistoryStore::default()),
            failover_manager: Arc::new(FailoverSwitchManager::new(db)),
            app_handle: None,
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
                allow_legacy_projection: true,
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
        assert_eq!(plan.selections.len(), 2);
    }

    #[test]
    fn model_catalog_from_raw_extracts_supported_client_model_ids() {
        let catalog = crate::proxy_core::client_model_catalog_from_raw(
            "codex",
            json!({
                "models": [
                    {"id": " gpt-5 "},
                    {"model": "o4-mini"},
                    {"name": "gemini-2.5-pro"},
                    "claude-sonnet-4",
                    {"id": "gpt-5"}
                ]
            }),
        );

        assert_eq!(
            catalog.models,
            vec![
                "claude-sonnet-4".to_string(),
                "gemini-2.5-pro".to_string(),
                "gpt-5".to_string(),
                "o4-mini".to_string(),
            ]
        );
        assert_eq!(catalog.provider_id, "codex");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn model_catalog_provider_loads_codex_client_catalog_file() {
        let _home = IsolatedTestHome::new();
        let codex_dir = crate::codex_config::get_codex_config_dir();
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            crate::codex_config::get_codex_config_path(),
            "model_catalog_json = \"cc-switch-model-catalog.json\"\n",
        )
        .expect("write codex config");
        std::fs::write(
            crate::codex_config::get_codex_model_catalog_path(),
            r#"{"models":[{"id":"gpt-5"},{"model":"o4-mini"}]}"#,
        )
        .expect("write codex model catalog");
        let services = CcSwitchProxyServices::new(Arc::new(Database::memory().expect("memory db")));

        let catalog = services
            .model_catalog()
            .load_client_catalog(&AppKind::Codex)
            .await
            .expect("load client catalog");

        assert_eq!(catalog.provider_id, "codex");
        assert_eq!(
            catalog.models,
            vec!["gpt-5".to_string(), "o4-mini".to_string()]
        );
        assert_eq!(
            catalog.raw,
            json!({"models": [{"id": "gpt-5"}, {"model": "o4-mini"}]})
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn model_catalog_provider_ignores_user_owned_codex_catalog_file() {
        let _home = IsolatedTestHome::new();
        let codex_dir = crate::codex_config::get_codex_config_dir();
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            crate::codex_config::get_codex_config_path(),
            "model_catalog_json = \"my-custom-catalog.json\"\n",
        )
        .expect("write codex config");
        std::fs::write(
            codex_dir.join("my-custom-catalog.json"),
            r#"{"models":[{"id":"user-model"}]}"#,
        )
        .expect("write user catalog");
        let services = CcSwitchProxyServices::new(Arc::new(Database::memory().expect("memory db")));

        let catalog = services
            .model_catalog()
            .load_client_catalog(&AppKind::Codex)
            .await
            .expect("load client catalog");

        assert_eq!(catalog.models, Vec::<String>::new());
        assert_eq!(catalog.raw, json!({"models": []}));
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

    #[tokio::test]
    async fn event_sink_bridges_core_events_to_proxy_event_bus() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let events = Arc::new(ProxyEventBus::default());
        let mut subscriber = events.subscribe();
        let services = CcSwitchProxyServices::with_event_bus(db, events);

        services
            .event_sink()
            .emit_event(ProxyCoreEvent {
                event_type: ProxyCoreEventType::RouteSelected,
                request_id: Some("req-1".to_string()),
                channel_id: Some("channel-a".to_string()),
                payload: json!({
                    "attemptCount": 2,
                }),
            })
            .await
            .expect("emit event");

        let event = subscriber.recv().await.expect("receive event");
        assert_eq!(event.event, "route_selected");
        assert_eq!(event.payload["requestId"], "req-1");
        assert_eq!(event.payload["channelId"], "channel-a");
        assert_eq!(event.payload["attemptCount"], 2);
    }

    #[tokio::test]
    async fn forward_pipeline_without_runtime_reports_unsupported() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let services = CcSwitchProxyServices::new(db);

        let err = services
            .forward_pipeline()
            .forward(proxy_request(), route_plan("provider-a", "channel-a"))
            .await
            .expect_err("plain services do not own server runtime");

        assert!(matches!(err, ProxyCoreError::Unsupported(_)));
    }

    #[tokio::test]
    async fn runtime_forward_pipeline_requires_matching_host_provider() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let services = CcSwitchProxyServices::with_runtime(runtime(db));

        let err = services
            .forward_pipeline()
            .forward(proxy_request(), route_plan("missing-provider", "channel-a"))
            .await
            .expect_err("missing provider should stop before forwarding");

        assert!(matches!(err, ProxyCoreError::Unavailable(_)));
        assert!(err
            .to_string()
            .contains("route plan providers are not configured"));
    }

    #[test]
    fn proxy_response_bridge_preserves_buffered_body() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        let response = ProxyResponse::buffered(
            StatusCode::CREATED,
            headers,
            Bytes::from_static(br#"{"ok":true}"#),
        );

        let core_response = proxy_response_to_core_response(response, Option::<()>::None);

        assert_eq!(core_response.status, StatusCode::CREATED);
        assert_eq!(
            core_response
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        match core_response.body {
            ProxyResponseBody::Bytes(body) => {
                assert_eq!(body, Bytes::from_static(br#"{"ok":true}"#))
            }
            other => panic!("expected bytes body, got {other:?}"),
        }
    }

    #[test]
    fn selected_route_prefers_successful_channel_from_forward_result() {
        let primary = route_plan("provider-a", "channel-a").selection;
        let fallback = route_plan("provider-a", "channel-b").selection;
        let plan = RoutePlan {
            selection: primary.clone(),
            selections: vec![primary, fallback],
            attempts: Vec::new(),
        };
        let result = crate::proxy::ForwardResult {
            response: ProxyResponse::buffered(
                StatusCode::OK,
                http::HeaderMap::new(),
                Bytes::from_static(b"{}"),
            ),
            provider: Provider::with_id(
                "provider-a".to_string(),
                "Provider A".to_string(),
                json!({}),
                None,
            ),
            claude_api_format: None,
            outbound_model: None,
            selected_channel: Some(crate::proxy_core::ResolvedChannelAttempt {
                channel_id: "channel-b".to_string(),
                channel_name: "Channel B".to_string(),
                base_url: "https://fallback.example.com/v1".to_string(),
                interface_kind: "openai_responses".to_string(),
                public_model: Some("sonnet".to_string()),
                upstream_model: Some("upstream-sonnet".to_string()),
            }),
            connection_guard: None,
        };

        let selected = selected_route_for_forward_result(&result, &plan);

        assert_eq!(selected.channel.id, "channel-b");
    }

    #[tokio::test]
    async fn proxy_response_bridge_wraps_streamed_body() {
        let response = ProxyResponse::streamed(
            StatusCode::OK,
            http::HeaderMap::new(),
            futures::stream::once(async { Ok(Bytes::from_static(b"chunk")) }),
        );

        let core_response = proxy_response_to_core_response(response, Option::<()>::None);

        match core_response.body {
            ProxyResponseBody::Stream(mut stream) => {
                let chunk = stream.next().await.expect("chunk").expect("stream item");
                assert_eq!(chunk, Bytes::from_static(b"chunk"));
                assert!(stream.next().await.is_none());
            }
            other => panic!("expected stream body, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn usage_sink_records_complete_usage_records() -> Result<(), AppError> {
        let db = Arc::new(Database::memory().expect("memory db"));
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO model_pricing (
                    model_id,
                    display_name,
                    input_cost_per_million,
                    output_cost_per_million,
                    cache_read_cost_per_million,
                    cache_creation_cost_per_million
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "upstream-sonnet",
                    "Upstream Sonnet",
                    "3.0",
                    "15.0",
                    "0.3",
                    "3.75"
                ],
            )
            .expect("insert pricing");
        }
        let services = CcSwitchProxyServices::new(db.clone());

        services
            .usage_sink()
            .record_usage(UsageRecord {
                request_id: Some("req-usage-1".to_string()),
                message_id: Some("msg-usage-1".to_string()),
                app: AppKind::Claude,
                provider_id: "provider-a".to_string(),
                provider_kind: Some(ProviderKind::Claude),
                channel_id: Some("channel-a".to_string()),
                channel_name: Some("Channel A".to_string()),
                route_group: Some(DEFAULT_ROUTE_GROUP.to_string()),
                request_model: "public-sonnet".to_string(),
                outbound_model: "upstream-sonnet".to_string(),
                response_model: Some("upstream-sonnet".to_string()),
                pricing_model: None,
                tokens: UsageTokens {
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
            })
            .await
            .expect("record usage");

        let conn = crate::database::lock_conn!(db.conn);
        let row: (
            String,
            String,
            String,
            String,
            String,
            i64,
            i64,
            i64,
            Option<i64>,
            i64,
            Option<String>,
            Option<String>,
            i64,
            String,
        ) = conn
            .query_row(
                "SELECT
                    provider_id,
                    app_type,
                    model,
                    request_model,
                    pricing_model,
                    input_tokens,
                    output_tokens,
                    latency_ms,
                    first_token_ms,
                    status_code,
                    session_id,
                    provider_type,
                    is_streaming,
                    total_cost_usd
                 FROM proxy_request_logs
                 WHERE request_id = 'req-usage-1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                    ))
                },
            )
            .expect("usage row");

        assert_eq!(row.0, "provider-a");
        assert_eq!(row.1, "claude");
        assert_eq!(row.2, "upstream-sonnet");
        assert_eq!(row.3, "public-sonnet");
        assert_eq!(row.4, "upstream-sonnet");
        assert_eq!(row.5, 1_000);
        assert_eq!(row.6, 500);
        assert_eq!(row.7, 42);
        assert_eq!(row.8, Some(7));
        assert_eq!(row.9, 200);
        assert_eq!(row.10.as_deref(), Some("session-a"));
        assert_eq!(row.11.as_deref(), Some("claude"));
        assert_eq!(row.12, 1);
        assert_ne!(row.13, "0");
        Ok(())
    }

    #[tokio::test]
    async fn proxy_engine_plans_routes_through_cc_switch_services() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        let services = Arc::new(CcSwitchProxyServices::new(db));
        let engine = ProxyEngine::new(services);
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({})),
        );
        request.requested_model = Some("claude-sonnet-4".to_string());

        let plan = engine.plan_route(&request).await.expect("plan route");

        assert_eq!(plan.selection.provider.id, "anthropic-main");
        assert_eq!(plan.selection.channel.provider_id, "anthropic-main");
        assert_eq!(
            plan.selection
                .model_route
                .as_ref()
                .map(|route| route.upstream_model.as_str()),
            Some("claude-sonnet-4")
        );
    }

    #[tokio::test]
    async fn proxy_engine_materialized_plan_does_not_fallback_to_legacy_projection() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        let services = Arc::new(CcSwitchProxyServices::new(db.clone()));
        let engine = ProxyEngine::new(services);
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({})),
        );
        request.requested_model = Some("claude-sonnet-4".to_string());

        let err = engine
            .plan_materialized_route(&request)
            .await
            .expect_err("empty channel table should not fallback to legacy projection");
        assert!(matches!(err, ProxyCoreError::Unavailable(_)));

        db.materialize_legacy_proxy_channels("claude")
            .expect("materialize channels");
        let plan = engine
            .plan_materialized_route(&request)
            .await
            .expect("materialized plan");
        assert_eq!(plan.selection.channel.provider_id, "anthropic-main");
    }
}
