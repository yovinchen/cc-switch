use super::domain::{
    interfaces_compatible, route_group_matches, ChannelQuery, ChannelStatus, InterfaceKind,
    AppKind, ProxyRequest, ProxyResult, RoutableModel, RoutableModelList, RoutePlan, RoutePolicy,
    RouteRequest,
    DEFAULT_ROUTE_GROUP,
};
use super::error::ProxyCoreResult;
use super::management_api::{
    AppChannelListSource, AppChannelManagementPlan, AppChannelManagementRequest,
    AppModelCatalogRequest, ChannelListPlan, ChannelListRequest, ChannelListSource,
    GroupListChannelRecordInput, GroupListChannelSource, GroupListRequest,
    ManagementAppPathRequest, ProviderListSource, RouteResolveManagementRequest,
};
use super::ports::{
    AppChannelResponse, ChannelHealthReset, ChannelHealthResetResponse, ChannelRecord,
    ChannelListResponse, ChannelRouteCandidate, ChannelRouteRejected,
    ClientModelCatalogResponse, ModelCatalog, ProviderListResponse, ProxyCoreEvent,
    ProxyCoreEventType, ProxyServices, RouteGroupListResponse, RouteResolveResponse,
};
use serde_json::{json, to_value, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct ProxyRuntimeState {
    accepted_requests: AtomicU64,
}

impl ProxyRuntimeState {
    pub fn accepted_requests(&self) -> u64 {
        self.accepted_requests.load(Ordering::Relaxed)
    }

    fn mark_accepted(&self) {
        self.accepted_requests.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyCoreStatus {
    pub accepted_requests: u64,
    pub forwarding_enabled: bool,
}

pub struct ProxyEngine<S: ?Sized> {
    services: Arc<S>,
    state: Arc<ProxyRuntimeState>,
}

impl<S> ProxyEngine<S>
where
    S: ProxyServices + ?Sized,
{
    pub fn new(services: Arc<S>) -> Self {
        Self {
            services,
            state: Arc::new(ProxyRuntimeState::default()),
        }
    }

    pub fn services(&self) -> &S {
        self.services.as_ref()
    }

    pub fn state(&self) -> &ProxyRuntimeState {
        self.state.as_ref()
    }

    pub fn status(&self) -> ProxyCoreStatus {
        ProxyCoreStatus {
            accepted_requests: self.state.accepted_requests(),
            forwarding_enabled: true,
        }
    }

    pub async fn plan_route(&self, request: &ProxyRequest) -> ProxyCoreResult<RoutePlan> {
        self.plan_route_with_legacy_projection(request, true).await
    }

    pub async fn plan_materialized_route(
        &self,
        request: &ProxyRequest,
    ) -> ProxyCoreResult<RoutePlan> {
        self.plan_route_with_legacy_projection(request, false).await
    }

    pub async fn list_models(
        &self,
        app: &super::domain::AppKind,
        group: Option<&str>,
        inbound_interface: Option<&InterfaceKind>,
    ) -> ProxyCoreResult<Vec<RoutableModel>> {
        let providers = self.services.providers().list_providers(app).await?;
        let channels = self
            .services
            .channels()
            .list_channels(ChannelQuery {
                app,
                provider_id: None,
                model: None,
                group,
                include_disabled: false,
                allow_legacy_projection: true,
            })
            .await?;
        let requested_group = group.unwrap_or(DEFAULT_ROUTE_GROUP);
        let mut models = BTreeMap::new();

        for channel in channels {
            if channel.status != ChannelStatus::Enabled {
                continue;
            }
            if !route_group_matches(&channel.groups, requested_group) {
                continue;
            }
            if let Some(inbound_interface) = inbound_interface {
                if !interfaces_compatible(inbound_interface, &channel.interface) {
                    continue;
                }
            }
            let Some(provider) = providers
                .iter()
                .find(|provider| provider.id == channel.provider_id)
            else {
                continue;
            };

            for model in &channel.models {
                if model.public_model.trim().is_empty() {
                    continue;
                }
                let item = RoutableModel {
                    public_model: model.public_model.clone(),
                    upstream_model: model.upstream_model.clone(),
                    pricing_model: model.pricing_model.clone(),
                    app: channel.app.clone(),
                    provider_id: provider.id.clone(),
                    provider_name: provider.name.clone(),
                    channel_id: channel.id.clone(),
                    channel_name: channel.name.clone(),
                    interface: channel.interface.clone(),
                    groups: channel.groups.clone(),
                    priority: channel.priority,
                    weight: channel.weight,
                    capabilities: model.capabilities.clone(),
                };
                models.insert(
                    (
                        item.public_model.clone(),
                        item.channel_id.clone(),
                        item.upstream_model.clone(),
                    ),
                    item,
                );
            }
        }

        let mut models: Vec<_> = models.into_values().collect();
        models.sort_by(|left, right| {
            left.public_model
                .cmp(&right.public_model)
                .then_with(|| right.priority.cmp(&left.priority))
                .then_with(|| right.weight.cmp(&left.weight))
                .then_with(|| left.channel_name.cmp(&right.channel_name))
                .then_with(|| left.channel_id.cmp(&right.channel_id))
        });
        Ok(models)
    }

    pub async fn list_model_catalog(
        &self,
        app: &super::domain::AppKind,
        app_type: impl Into<String>,
        group: Option<&str>,
        inbound_interface: Option<&InterfaceKind>,
    ) -> ProxyCoreResult<RoutableModelList> {
        let models = self.list_models(app, group, inbound_interface).await?;
        Ok(RoutableModelList::new(
            app_type,
            group.map(ToString::to_string),
            inbound_interface.map(|interface| interface.as_str().to_string()),
            models,
        ))
    }

    pub async fn list_model_catalog_for_request(
        &self,
        request: AppModelCatalogRequest,
    ) -> ProxyCoreResult<RoutableModelList> {
        let AppModelCatalogRequest {
            app,
            app_type,
            route_group,
            interface_kind,
        } = request;

        self.list_model_catalog(
            &app,
            app_type,
            route_group.as_deref(),
            interface_kind.as_ref(),
        )
        .await
    }

    pub async fn provider_list_source(
        &self,
        app: &AppKind,
    ) -> ProxyCoreResult<ProviderListSource> {
        let providers = self.services.providers().list_providers(app).await?;
        let current_provider = self.services.providers().current_provider_id(app).await?;
        let route_policy = self.services.route_policies().load_policy(app).await?;
        let failover_provider_ids = failover_provider_ids_from_policy(route_policy.as_ref());
        let route_candidate_ids = self
            .services
            .providers()
            .route_candidate_provider_ids(app)
            .await?;

        Ok(ProviderListSource::from_provider_specs(
            providers,
            current_provider,
            failover_provider_ids,
            route_candidate_ids,
        ))
    }

    pub async fn provider_list_response(
        &self,
        request: ManagementAppPathRequest,
    ) -> ProxyCoreResult<ProviderListResponse> {
        let app = AppKind::from(request.app_type.as_str());
        let source = self.provider_list_source(&app).await?;
        Ok(request.provider_list_response_from_source(source))
    }

    pub async fn resolve_route_response(
        &self,
        request: RouteResolveManagementRequest,
    ) -> ProxyCoreResult<RouteResolveResponse> {
        let response = self
            .services
            .route_resolver()
            .resolve_management_route(request.request.clone())
            .await?;
        Ok(request.response_from_resolution(response))
    }

    pub async fn app_channel_response(
        &self,
        request: AppChannelManagementRequest,
    ) -> ProxyCoreResult<AppChannelResponse<ChannelRecord, ChannelRouteCandidate, ChannelRouteRejected>>
    {
        match request.plan() {
            AppChannelManagementPlan::Route(route_request) => {
                let response = self
                    .resolve_route_response(RouteResolveManagementRequest::from_body(route_request)?)
                    .await?;
                Ok(request.response_from_route_resolution(response))
            }
            AppChannelManagementPlan::List { app_type } => {
                let app = AppKind::from(app_type.as_str());
                let (source, channels) = self.services.channels().list_channel_records(&app).await?;
                Ok(request.response_from_list_source(AppChannelListSource::new(
                    source, channels,
                )))
            }
        }
    }

    pub async fn channel_list_response(
        &self,
        request: ChannelListRequest,
    ) -> ProxyCoreResult<ChannelListResponse<ChannelRecord>> {
        let app = match request.plan() {
            ChannelListPlan::App { app_type } => Some(AppKind::from(app_type.as_str())),
            ChannelListPlan::All => None,
        };
        let channels = self
            .services
            .channels()
            .list_materialized_channel_records(app.as_ref())
            .await?;
        Ok(request.response_from_source(ChannelListSource::new(channels)))
    }

    pub async fn group_list_response<I, T>(
        &self,
        request: GroupListRequest,
        all_app_types: I,
    ) -> ProxyCoreResult<RouteGroupListResponse>
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let app_types = request.app_scope(all_app_types);
        let mut sources = Vec::new();

        for app_type in app_types {
            let app = AppKind::from(app_type.as_str());
            let (source, channels) = self.services.channels().list_channel_records(&app).await?;
            sources.push(GroupListChannelSource::from_record_inputs(
                app_type,
                source,
                channels
                    .into_iter()
                    .map(|channel| GroupListChannelRecordInput::new(channel.groups)),
            ));
        }

        Ok(request.response_from_channel_sources(sources))
    }

    pub async fn client_model_catalog(
        &self,
        app: &super::domain::AppKind,
    ) -> ProxyCoreResult<ModelCatalog> {
        self.services.model_catalog().load_client_catalog(app).await
    }

    pub async fn client_model_catalog_response(
        &self,
        app: &super::domain::AppKind,
    ) -> ProxyCoreResult<ClientModelCatalogResponse> {
        self.client_model_catalog(app)
            .await
            .map(ClientModelCatalogResponse::from_catalog)
    }

    pub async fn reset_channel_health(
        &self,
        channel_id: &str,
    ) -> ProxyCoreResult<ChannelHealthReset> {
        self.services.health_store().reset_channel(channel_id).await
    }

    pub async fn reset_channel_health_response(
        &self,
        channel_id: &str,
    ) -> ProxyCoreResult<ChannelHealthResetResponse> {
        self.reset_channel_health(channel_id)
            .await
            .map(ChannelHealthResetResponse::from_reset)
    }

    async fn plan_route_with_legacy_projection(
        &self,
        request: &ProxyRequest,
        allow_legacy_projection: bool,
    ) -> ProxyCoreResult<RoutePlan> {
        let providers = self
            .services
            .providers()
            .list_providers(&request.app)
            .await?;
        let channels = self
            .services
            .channels()
            .list_channels(ChannelQuery {
                app: &request.app,
                provider_id: None,
                model: request.requested_model.as_deref(),
                group: request.route_group.as_deref(),
                include_disabled: false,
                allow_legacy_projection,
            })
            .await?;
        let policy = self
            .services
            .route_policies()
            .load_policy(&request.app)
            .await?;

        self.services
            .route_resolver()
            .resolve(RouteRequest {
                request,
                providers: &providers,
                channels: &channels,
                policy: policy.as_ref(),
            })
            .await
    }

    pub async fn handle(&self, request: ProxyRequest) -> ProxyCoreResult<ProxyResult> {
        self.state.mark_accepted();
        let route_plan = self.plan_route(&request).await?;
        self.services
            .event_sink()
            .emit_event(ProxyCoreEvent {
                event_type: ProxyCoreEventType::RouteSelected,
                request_id: request.client_request_id.clone(),
                channel_id: Some(route_plan.selection.channel.id.clone()),
                payload: json!({
                    "selection": to_value(&route_plan.selection).unwrap_or_else(|_| json!({})),
                    "attemptCount": route_plan.attempts.len(),
                }),
            })
            .await?;

        let result = self
            .services
            .forward_pipeline()
            .forward(request, route_plan)
            .await?;

        if let Some(usage_record) = result.usage_record.clone() {
            self.services().usage_sink().record_usage(usage_record).await?;
        }

        Ok(result)
    }
}

fn failover_provider_ids_from_policy(policy: Option<&RoutePolicy>) -> Vec<String> {
    policy
        .and_then(|policy| policy.raw.get("failoverProviderIds"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(ToString::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        route_policy_from_failover_provider_ids, AppKind, AuthProfileRef, ChannelAttemptResult,
        ChannelHealthPolicy, ChannelOverrides, ChannelSpec, ChannelStatus, InterfaceKind,
        ModelCapabilities, ModelRoute, ProviderKind, ProviderMetadata, ProviderSpec, ProxyBody,
        ProxyCoreResponse, RetryPolicy, RoutePolicy, RouteSelection, UpstreamEndpoint,
        UsageRecord, UsageTokens, DEFAULT_ROUTE_GROUP,
    };
    use crate::error::ProxyCoreError;
    use crate::ports::{
        channel_record_from_input, AppChannelListResponse, ChannelRecordInput,
        ChannelRouteSource,
        AuthInfo, AuthProvider, ChannelHealthStore, ChannelSource, ForwardPipeline, ModelCatalog,
        ModelCatalogProvider, ProviderSource, ProxyAppConfig, ProxyConfigSource, ProxyCoreEvent,
        ProxyEventSink, ProxyGlobalConfig, ProxyRuntimeConfig, RoutePolicySource, RouteResolver,
        RouteResolveRequest, UsageSink,
    };
    use futures::future::BoxFuture;
    use http::{Method, StatusCode};
    use std::sync::Mutex;

    #[derive(Default)]
    struct TestServices {
        events: Mutex<Vec<ProxyCoreEvent>>,
        forwarded: Mutex<Vec<String>>,
        usage: Mutex<Vec<UsageRecord>>,
        channels: Mutex<Vec<ChannelSpec>>,
        providers: Mutex<Vec<ProviderSpec>>,
        current_provider: Mutex<Option<String>>,
        route_candidate_provider_ids: Mutex<Vec<String>>,
        route_policy: Mutex<Option<RoutePolicy>>,
        route_resolution: Mutex<Option<RouteResolveResponse>>,
        channel_records: Mutex<Vec<ChannelRecord>>,
        channel_route_source: Mutex<Option<ChannelRouteSource>>,
        queried_channel_apps: Mutex<Vec<String>>,
        queried_materialized_channel_apps: Mutex<Vec<Option<String>>>,
    }

    impl ProxyServices for TestServices {
        fn config(&self) -> &(dyn ProxyConfigSource + Send + Sync) {
            self
        }

        fn providers(&self) -> &(dyn ProviderSource + Send + Sync) {
            self
        }

        fn channels(&self) -> &(dyn ChannelSource + Send + Sync) {
            self
        }

        fn route_policies(&self) -> &(dyn RoutePolicySource + Send + Sync) {
            self
        }

        fn route_resolver(&self) -> &(dyn RouteResolver + Send + Sync) {
            self
        }

        fn health_store(&self) -> &(dyn ChannelHealthStore + Send + Sync) {
            self
        }

        fn auth_provider(&self) -> &(dyn AuthProvider + Send + Sync) {
            self
        }

        fn model_catalog(&self) -> &(dyn ModelCatalogProvider + Send + Sync) {
            self
        }

        fn usage_sink(&self) -> &(dyn UsageSink + Send + Sync) {
            self
        }

        fn event_sink(&self) -> &(dyn ProxyEventSink + Send + Sync) {
            self
        }

        fn forward_pipeline(&self) -> &(dyn ForwardPipeline + Send + Sync) {
            self
        }
    }

    impl ProxyConfigSource for TestServices {
        fn load_global<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyGlobalConfig>> {
            Box::pin(async { Ok(ProxyGlobalConfig::default()) })
        }

        fn load_app<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<ProxyAppConfig>> {
            Box::pin(async { Ok(ProxyAppConfig::default()) })
        }

        fn load_runtime<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeConfig>> {
            Box::pin(async { Ok(ProxyRuntimeConfig::default()) })
        }
    }

    impl ProviderSource for TestServices {
        fn list_providers<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Vec<ProviderSpec>>> {
            let providers = self.providers.lock().expect("providers mutex").clone();
            Box::pin(async move {
                if providers.is_empty() {
                    Ok(vec![provider_spec()])
                } else {
                    Ok(providers)
                }
            })
        }

        fn get_provider<'a>(
            &'a self,
            _app: &'a AppKind,
            provider_id: &'a str,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<ProviderSpec>>> {
            Box::pin(async move {
                Ok((provider_id == "provider-a").then(provider_spec))
            })
        }

        fn current_provider_id<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<String>>> {
            let current_provider = self
                .current_provider
                .lock()
                .expect("current provider mutex")
                .clone();
            Box::pin(async move { Ok(current_provider) })
        }

        fn route_candidate_provider_ids<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Vec<String>>> {
            let provider_ids = self
                .route_candidate_provider_ids
                .lock()
                .expect("route candidates mutex")
                .clone();
            Box::pin(async move { Ok(provider_ids) })
        }
    }

    impl ChannelSource for TestServices {
        fn list_channels<'a>(
            &'a self,
            _query: crate::domain::ChannelQuery<'a>,
        ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelSpec>>> {
            Box::pin(async move {
                let channels = self.channels.lock().expect("channels mutex").clone();
                if channels.is_empty() {
                    Ok(vec![channel_spec()])
                } else {
                    Ok(channels)
                }
            })
        }

        fn get_channel<'a>(
            &'a self,
            channel_id: &'a str,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelSpec>>> {
            Box::pin(async move { Ok((channel_id == "channel-a").then(channel_spec)) })
        }

        fn list_channel_records<'a>(
            &'a self,
            app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<(ChannelRouteSource, Vec<ChannelRecord>)>> {
            self.queried_channel_apps
                .lock()
                .expect("queried channel apps mutex")
                .push(app.as_str().to_string());
            let source = self
                .channel_route_source
                .lock()
                .expect("channel route source mutex")
                .clone()
                .unwrap_or(ChannelRouteSource::MaterializedChannels);
            let records = self
                .channel_records
                .lock()
                .expect("channel records mutex")
                .clone();
            Box::pin(async move {
                if records.is_empty() {
                    Ok((source, vec![channel_record()]))
                } else {
                    Ok((source, records))
                }
            })
        }

        fn list_materialized_channel_records<'a>(
            &'a self,
            app: Option<&'a AppKind>,
        ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelRecord>>> {
            let app_type = app.map(|app| app.as_str().to_string());
            self.queried_materialized_channel_apps
                .lock()
                .expect("queried materialized channel apps mutex")
                .push(app_type.clone());
            let records = self
                .channel_records
                .lock()
                .expect("channel records mutex")
                .clone();
            Box::pin(async move {
                let records = if records.is_empty() {
                    vec![channel_record()]
                } else {
                    records
                };
                Ok(match app_type {
                    Some(app_type) => records
                        .into_iter()
                        .filter(|record| record.app_type == app_type)
                        .collect(),
                    None => records,
                })
            })
        }
    }

    impl RoutePolicySource for TestServices {
        fn load_policy<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<crate::domain::RoutePolicy>>> {
            let route_policy = self
                .route_policy
                .lock()
                .expect("route policy mutex")
                .clone();
            Box::pin(async move { Ok(route_policy) })
        }
    }

    impl RouteResolver for TestServices {
        fn resolve<'a>(
            &'a self,
            request: RouteRequest<'a>,
        ) -> BoxFuture<'a, ProxyCoreResult<RoutePlan>> {
            Box::pin(async move {
                let provider = request
                    .providers
                    .first()
                    .cloned()
                    .ok_or_else(|| ProxyCoreError::Unavailable("missing provider".to_string()))?;
                let channel = request
                    .channels
                    .first()
                    .cloned()
                    .ok_or_else(|| ProxyCoreError::Unavailable("missing channel".to_string()))?;
                let model_route = channel.models.first().cloned();
                let selection = RouteSelection {
                    provider,
                    channel: channel.clone(),
                    model_route,
                    inbound_interface: request.request.inbound_interface.clone(),
                    outbound_interface: channel.interface.clone(),
                };

                Ok(RoutePlan {
                    selection: selection.clone(),
                    selections: vec![selection],
                    attempts: vec![crate::domain::ChannelAttemptPlan {
                        channel_id: channel.id,
                        provider_id: channel.provider_id,
                        priority: channel.priority,
                        weight: channel.weight,
                    }],
                })
            })
        }

        fn resolve_management_route<'a>(
            &'a self,
            request: RouteResolveRequest,
        ) -> BoxFuture<'a, ProxyCoreResult<RouteResolveResponse>> {
            let response = self
                .route_resolution
                .lock()
                .expect("route resolution mutex")
                .clone()
                .unwrap_or(RouteResolveResponse {
                    app_type: request.app_type,
                    requested_model: request.requested_model,
                    interface_kind: request.interface_kind,
                    route_group: request
                        .route_group
                        .unwrap_or_else(|| DEFAULT_ROUTE_GROUP.to_string()),
                    source: crate::ports::ChannelRouteSource::MaterializedChannels,
                    candidates: Vec::new(),
                    rejected: Vec::new(),
                });
            Box::pin(async move { Ok(response) })
        }
    }

    impl ChannelHealthStore for TestServices {
        fn record_attempt<'a>(
            &'a self,
            _result: ChannelAttemptResult,
        ) -> BoxFuture<'a, ProxyCoreResult<()>> {
            Box::pin(async { Ok(()) })
        }

        fn reset_channel<'a>(
            &'a self,
            channel_id: &'a str,
        ) -> BoxFuture<'a, ProxyCoreResult<ChannelHealthReset>> {
            Box::pin(async move {
                Ok(ChannelHealthReset {
                    channel_id: channel_id.to_string(),
                    app: AppKind::Claude,
                })
            })
        }
    }

    impl AuthProvider for TestServices {
        fn resolve_auth<'a>(
            &'a self,
            _auth_profile: Option<&'a AuthProfileRef>,
            _request: &'a ProxyRequest,
        ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>> {
            Box::pin(async { Ok(AuthInfo::default()) })
        }
    }

    impl ModelCatalogProvider for TestServices {
        fn load_catalog<'a>(
            &'a self,
            _app: &'a AppKind,
            provider_id: &'a str,
        ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
            Box::pin(async move {
                Ok(ModelCatalog {
                    provider_id: provider_id.to_string(),
                    models: vec!["sonnet".to_string()],
                    raw: json!({}),
                })
            })
        }

        fn load_client_catalog<'a>(
            &'a self,
            app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
            Box::pin(async move {
                Ok(ModelCatalog {
                    provider_id: app.as_str().to_string(),
                    models: vec!["gpt-5".to_string()],
                    raw: json!({"models": [{"id": "gpt-5"}]}),
                })
            })
        }
    }

    impl UsageSink for TestServices {
        fn record_usage<'a>(&'a self, record: UsageRecord) -> BoxFuture<'a, ProxyCoreResult<()>> {
            Box::pin(async move {
                self.usage.lock().expect("usage mutex").push(record);
                Ok(())
            })
        }
    }

    impl ProxyEventSink for TestServices {
        fn emit_event<'a>(&'a self, event: ProxyCoreEvent) -> BoxFuture<'a, ProxyCoreResult<()>> {
            Box::pin(async move {
                self.events.lock().expect("events mutex").push(event);
                Ok(())
            })
        }
    }

    impl ForwardPipeline for TestServices {
        fn forward<'a>(
            &'a self,
            request: ProxyRequest,
            plan: RoutePlan,
        ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>> {
            Box::pin(async move {
                self.forwarded
                    .lock()
                    .expect("forwarded mutex")
                    .push(format!("{} {}", request.method, request.endpoint));
                let outbound_model = plan
                    .selection
                    .model_route
                    .as_ref()
                    .map(|route| route.upstream_model.clone());

                Ok(ProxyResult {
                    response: ProxyCoreResponse::empty(StatusCode::OK),
                    selected_route: plan.selection,
                    outbound_model: outbound_model.clone(),
                    metadata: json!({}),
                    usage_record: Some(UsageRecord {
                        request_id: request.client_request_id,
                        message_id: Some("msg-1".to_string()),
                        app: request.app,
                        provider_id: "provider-a".to_string(),
                        provider_kind: Some(ProviderKind::Claude),
                        channel_id: Some("channel-a".to_string()),
                        channel_name: Some("Channel A".to_string()),
                        route_group: Some("default".to_string()),
                        request_model: "sonnet".to_string(),
                        outbound_model: outbound_model.clone().unwrap_or_default(),
                        response_model: outbound_model,
                        pricing_model: None,
                        tokens: UsageTokens {
                            input_tokens: 5,
                            output_tokens: 8,
                            cache_read_tokens: 0,
                            cache_creation_tokens: 0,
                        },
                        latency_ms: 10,
                        first_token_ms: Some(3),
                        status_code: StatusCode::OK.as_u16(),
                        error_message: None,
                        session_id: Some("session-1".to_string()),
                        is_streaming: false,
                        metadata: json!({}),
                    }),
                })
            })
        }
    }

    #[test]
    fn handle_plans_route_and_delegates_to_forward_pipeline() {
        let services = Arc::new(TestServices::default());
        let engine = ProxyEngine::new(services.clone());
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({"model": "sonnet"})),
        );
        request.client_request_id = Some("req-1".to_string());
        request.requested_model = Some("sonnet".to_string());

        let result = futures::executor::block_on(engine.handle(request)).expect("handle request");

        assert_eq!(engine.state().accepted_requests(), 1);
        assert_eq!(result.response.status, StatusCode::OK);
        assert_eq!(result.selected_route.channel.id, "channel-a");
        assert_eq!(result.outbound_model.as_deref(), Some("upstream-sonnet"));
        assert_eq!(
            services.forwarded.lock().expect("forwarded mutex").as_slice(),
            ["POST /v1/messages"]
        );
        let usage = services.usage.lock().expect("usage mutex");
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].provider_id, "provider-a");
        assert_eq!(usage[0].request_model, "sonnet");
        assert_eq!(usage[0].tokens.input_tokens, 5);
        let events = services.events.lock().expect("events mutex");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, ProxyCoreEventType::RouteSelected);
        assert_eq!(events[0].request_id.as_deref(), Some("req-1"));
        assert_eq!(events[0].channel_id.as_deref(), Some("channel-a"));
    }

    #[test]
    fn list_models_returns_route_visible_channel_models() {
        let services = Arc::new(TestServices::default());
        let engine = ProxyEngine::new(services.clone());
        let mut default_channel = channel_spec();
        default_channel.models.push(ModelRoute {
            public_model: "sonnet".to_string(),
            upstream_model: "upstream-sonnet".to_string(),
            capabilities: ModelCapabilities::default(),
            pricing_model: None,
            request_overrides: json!({}),
            response_overrides: json!({}),
        });
        let mut beta_channel = channel_spec();
        beta_channel.id = "channel-beta".to_string();
        beta_channel.name = "Channel Beta".to_string();
        beta_channel.groups = vec!["beta".to_string()];
        beta_channel.models = vec![ModelRoute {
            public_model: "haiku".to_string(),
            upstream_model: "upstream-haiku".to_string(),
            capabilities: ModelCapabilities::default(),
            pricing_model: None,
            request_overrides: json!({}),
            response_overrides: json!({}),
        }];
        let mut disabled_channel = channel_spec();
        disabled_channel.id = "channel-disabled".to_string();
        disabled_channel.status = ChannelStatus::ManuallyDisabled;
        disabled_channel.models = vec![ModelRoute {
            public_model: "opus".to_string(),
            upstream_model: "upstream-opus".to_string(),
            capabilities: ModelCapabilities::default(),
            pricing_model: None,
            request_overrides: json!({}),
            response_overrides: json!({}),
        }];
        let mut incompatible_channel = channel_spec();
        incompatible_channel.id = "channel-embeddings".to_string();
        incompatible_channel.interface = InterfaceKind::Embeddings;
        incompatible_channel.models = vec![ModelRoute {
            public_model: "embedding-3".to_string(),
            upstream_model: "embedding-3".to_string(),
            capabilities: ModelCapabilities::default(),
            pricing_model: None,
            request_overrides: json!({}),
            response_overrides: json!({}),
        }];

        *services.channels.lock().expect("channels mutex") = vec![
            default_channel,
            beta_channel,
            disabled_channel,
            incompatible_channel,
        ];

        let models = futures::executor::block_on(engine.list_models(
            &AppKind::Claude,
            Some(DEFAULT_ROUTE_GROUP),
            Some(&InterfaceKind::AnthropicMessages),
        ))
        .expect("list models");

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].public_model, "sonnet");
        assert_eq!(models[0].channel_id, "channel-a");
    }

    #[test]
    fn list_model_catalog_wraps_route_visible_models() {
        let services = Arc::new(TestServices::default());
        let engine = ProxyEngine::new(services);

        let catalog = futures::executor::block_on(engine.list_model_catalog(
            &AppKind::Claude,
            "claude",
            Some(DEFAULT_ROUTE_GROUP),
            Some(&InterfaceKind::AnthropicMessages),
        ))
        .expect("list model catalog");

        assert_eq!(catalog.app_type, "claude");
        assert_eq!(catalog.route_group.as_deref(), Some(DEFAULT_ROUTE_GROUP));
        assert_eq!(
            catalog.interface_kind.as_deref(),
            Some("anthropic_messages")
        );
        assert_eq!(catalog.models.len(), 1);
        assert_eq!(catalog.models[0].public_model, "sonnet");

        let value = serde_json::to_value(&catalog).expect("serialize catalog");
        assert_eq!(value["appType"], "claude");
        assert_eq!(value["routeGroup"], DEFAULT_ROUTE_GROUP);
        assert_eq!(value["interfaceKind"], "anthropic_messages");
        assert_eq!(value["models"][0]["publicModel"], "sonnet");
    }

    #[test]
    fn list_model_catalog_accepts_management_request() {
        let services = Arc::new(TestServices::default());
        let engine = ProxyEngine::new(services);
        let request = AppModelCatalogRequest::from_parts(
            " claude ",
            serde_json::from_value(json!({
                "group": " default ",
                "interface": "anthropic_messages"
            }))
            .expect("query"),
        )
        .expect("catalog request");

        let catalog =
            futures::executor::block_on(engine.list_model_catalog_for_request(request))
                .expect("catalog");

        assert_eq!(catalog.app_type, "claude");
        assert_eq!(catalog.route_group.as_deref(), Some(DEFAULT_ROUTE_GROUP));
        assert_eq!(
            catalog.interface_kind.as_deref(),
            Some("anthropic_messages")
        );
        assert_eq!(catalog.models.len(), 1);
    }

    #[test]
    fn provider_list_response_combines_provider_runtime_sources() {
        let services = Arc::new(TestServices::default());
        let mut provider_a = provider_spec();
        provider_a.id = "provider-a".to_string();
        provider_a.name = "Provider A".to_string();
        let mut provider_b = provider_spec();
        provider_b.id = "provider-b".to_string();
        provider_b.name = "Provider B".to_string();

        *services.providers.lock().expect("providers mutex") = vec![provider_a, provider_b];
        *services
            .current_provider
            .lock()
            .expect("current provider mutex") = Some("provider-a".to_string());
        *services
            .route_candidate_provider_ids
            .lock()
            .expect("route candidates mutex") = vec!["provider-b".to_string()];
        *services.route_policy.lock().expect("route policy mutex") =
            Some(route_policy_from_failover_provider_ids(
                AppKind::Claude,
                vec!["provider-a".to_string()],
            ));

        let engine = ProxyEngine::new(services);
        let request = ManagementAppPathRequest::from_path("claude").expect("provider request");

        let response = futures::executor::block_on(engine.provider_list_response(request))
            .expect("provider list response");

        assert_eq!(response.app_type, "claude");
        assert_eq!(response.providers.len(), 2);
        assert!(response.providers[0].current);
        assert!(response.providers[0].in_failover_queue);
        assert!(!response.providers[0].route_candidate);
        assert!(!response.providers[1].current);
        assert!(!response.providers[1].in_failover_queue);
        assert!(response.providers[1].route_candidate);
    }

    #[test]
    fn resolve_route_response_delegates_management_dry_run_to_route_resolver() {
        let services = Arc::new(TestServices::default());
        let route_response = RouteResolveResponse {
            app_type: "claude".to_string(),
            requested_model: Some("sonnet".to_string()),
            interface_kind: Some("anthropic_messages".to_string()),
            route_group: "default".to_string(),
            source: crate::ports::ChannelRouteSource::LegacyProjection,
            candidates: Vec::new(),
            rejected: Vec::new(),
        };
        *services
            .route_resolution
            .lock()
            .expect("route resolution mutex") = Some(route_response);
        let engine = ProxyEngine::new(services);
        let request = RouteResolveManagementRequest::from_body(RouteResolveRequest {
            app_type: "claude".to_string(),
            requested_model: Some("sonnet".to_string()),
            interface_kind: Some("anthropic_messages".to_string()),
            route_group: None,
        })
        .expect("route resolve request");

        let response = futures::executor::block_on(engine.resolve_route_response(request))
            .expect("route resolve response");

        assert_eq!(response.app_type, "claude");
        assert_eq!(response.requested_model.as_deref(), Some("sonnet"));
        assert_eq!(
            response.interface_kind.as_deref(),
            Some("anthropic_messages")
        );
        assert_eq!(response.source, crate::ports::ChannelRouteSource::LegacyProjection);
    }

    #[test]
    fn app_channel_response_delegates_list_sources_to_channel_source() {
        let services = Arc::new(TestServices::default());
        *services
            .channel_route_source
            .lock()
            .expect("channel route source mutex") = Some(ChannelRouteSource::LegacyProjection);
        *services
            .channel_records
            .lock()
            .expect("channel records mutex") = vec![channel_record()];
        let engine = ProxyEngine::new(services);
        let request = AppChannelManagementRequest::from_parts(
            "claude",
            serde_json::from_value(json!({})).expect("query"),
        )
        .expect("channel management request");

        let response = futures::executor::block_on(engine.app_channel_response(request))
            .expect("channel response");

        let AppChannelResponse::List(AppChannelListResponse {
            source, channels, ..
        }) = response
        else {
            panic!("expected channel list response");
        };
        assert_eq!(source, "legacy_projection");
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id, "channel-a");
        assert_eq!(channels[0].source_kind, "legacy_projection");
    }

    #[test]
    fn channel_list_response_delegates_materialized_scope_to_channel_source() {
        let services = Arc::new(TestServices::default());
        *services
            .channel_records
            .lock()
            .expect("channel records mutex") = vec![
            channel_record_with_app_and_groups("claude", vec![DEFAULT_ROUTE_GROUP.to_string()]),
            channel_record_with_app_and_groups("codex", vec!["tools".to_string()]),
        ];
        let engine = ProxyEngine::new(services.clone());

        let all_request =
            ChannelListRequest::from_query(serde_json::from_value(json!({})).expect("query"))
                .expect("all channel list request");
        let all_response = futures::executor::block_on(engine.channel_list_response(all_request))
            .expect("all channel list response");

        let app_request = ChannelListRequest::from_query(
            serde_json::from_value(json!({ "appType": "claude" })).expect("query"),
        )
        .expect("app channel list request");
        let app_response = futures::executor::block_on(engine.channel_list_response(app_request))
            .expect("app channel list response");

        assert_eq!(all_response.channels.len(), 2);
        assert_eq!(app_response.channels.len(), 1);
        assert_eq!(app_response.channels[0].app_type, "claude");
        assert_eq!(
            *services
                .queried_materialized_channel_apps
                .lock()
                .expect("queried materialized channel apps mutex"),
            vec![None, Some("claude".to_string())]
        );
    }

    #[test]
    fn group_list_response_delegates_scoped_channel_sources_to_channel_source() {
        let services = Arc::new(TestServices::default());
        *services
            .channel_records
            .lock()
            .expect("channel records mutex") =
            vec![channel_record_with_groups(vec!["shared".to_string()])];
        let engine = ProxyEngine::new(services.clone());
        let request = GroupListRequest::from_query(serde_json::from_value(json!({})).expect("query"))
            .expect("group request");

        let response = futures::executor::block_on(engine.group_list_response(
            request,
            ["claude".to_string(), "codex".to_string()],
        ))
        .expect("group response");

        assert_eq!(response.app_type, None);
        assert_eq!(response.sources, vec!["materialized_channels"]);
        assert_eq!(response.groups.len(), 1);
        assert_eq!(response.groups[0].name, "shared");
        assert_eq!(response.groups[0].app_types, vec!["claude", "codex"]);
        assert_eq!(response.groups[0].channel_count, 2);
        assert_eq!(
            *services
                .queried_channel_apps
                .lock()
                .expect("queried channel apps mutex"),
            vec!["claude".to_string(), "codex".to_string()]
        );
    }

    #[test]
    fn client_model_catalog_delegates_to_catalog_provider() {
        let services = Arc::new(TestServices::default());
        let engine = ProxyEngine::new(services);

        let catalog =
            futures::executor::block_on(engine.client_model_catalog(&AppKind::Codex))
                .expect("client model catalog");

        assert_eq!(catalog.provider_id, "codex");
        assert_eq!(catalog.models, ["gpt-5"]);
        assert_eq!(catalog.raw, json!({"models": [{"id": "gpt-5"}]}));
    }

    #[test]
    fn client_model_catalog_response_wraps_catalog_raw_payload() {
        let services = Arc::new(TestServices::default());
        let engine = ProxyEngine::new(services);

        let response =
            futures::executor::block_on(engine.client_model_catalog_response(&AppKind::Codex))
                .expect("client model catalog response");

        assert_eq!(response.raw, json!({"models": [{"id": "gpt-5"}]}));
    }

    #[test]
    fn reset_channel_health_delegates_to_health_store() {
        let services = Arc::new(TestServices::default());
        let engine = ProxyEngine::new(services);

        let reset =
            futures::executor::block_on(engine.reset_channel_health("channel-a"))
                .expect("reset channel health");

        assert_eq!(reset.channel_id, "channel-a");
        assert_eq!(reset.app, AppKind::Claude);
    }

    #[test]
    fn reset_channel_health_response_wraps_management_envelope() {
        let services = Arc::new(TestServices::default());
        let engine = ProxyEngine::new(services);

        let response = futures::executor::block_on(engine.reset_channel_health_response("channel-a"))
            .expect("reset response");

        assert_eq!(response.channel_id, "channel-a");
        assert_eq!(response.app_type, "claude");
        assert!(response.reset);

        let value = serde_json::to_value(&response).expect("serialize reset response");
        assert_eq!(value["channelId"], "channel-a");
        assert_eq!(value["appType"], "claude");
        assert_eq!(value["reset"], true);
    }

    fn provider_spec() -> ProviderSpec {
        ProviderSpec {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        }
    }

    fn channel_spec() -> ChannelSpec {
        ChannelSpec {
            id: "channel-a".to_string(),
            provider_id: "provider-a".to_string(),
            app: AppKind::Claude,
            name: "Channel A".to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: "https://upstream.example.com/v1".to_string(),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::AnthropicMessages,
            auth_profile: None,
            models: vec![ModelRoute {
                public_model: "sonnet".to_string(),
                upstream_model: "upstream-sonnet".to_string(),
                capabilities: ModelCapabilities::default(),
                pricing_model: None,
                request_overrides: json!({}),
                response_overrides: json!({}),
            }],
            groups: vec!["default".to_string()],
            priority: 100,
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

    fn channel_record() -> ChannelRecord {
        channel_record_with_app_and_groups("claude", vec![DEFAULT_ROUTE_GROUP.to_string()])
    }

    fn channel_record_with_groups(groups: Vec<String>) -> ChannelRecord {
        channel_record_with_app_and_groups("claude", groups)
    }

    fn channel_record_with_app_and_groups(app_type: &str, groups: Vec<String>) -> ChannelRecord {
        channel_record_from_input(ChannelRecordInput {
            id: "channel-a".to_string(),
            provider_id: "provider-a".to_string(),
            app_type: app_type.to_string(),
            name: "Channel A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://upstream.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            auth_profile_ref: None,
            groups,
            priority: 100,
            weight: 100,
            retry_policy: json!({}),
            health_policy: json!({}),
            header_overrides: json!({}),
            param_overrides: json!({}),
            status_code_mapping: json!({}),
            tags: Vec::new(),
            metadata: json!({}),
            source_kind: "legacy_projection".to_string(),
            source_endpoint_url: Some("https://upstream.example.com/v1".to_string()),
            models: Vec::new(),
            needs_review: false,
            review_reasons: Vec::new(),
        })
    }
}
