use super::domain::{ChannelQuery, ProxyRequest, ProxyResult, RoutePlan, RouteRequest};
use super::error::ProxyCoreResult;
use super::ports::{ProxyCoreEvent, ProxyCoreEventType, ProxyServices};
use serde_json::{json, to_value};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AppKind, AuthProfileRef, ChannelAttemptResult, ChannelHealthPolicy, ChannelOverrides,
        ChannelSpec, ChannelStatus, InterfaceKind, ModelCapabilities, ModelRoute, ProviderKind,
        ProviderMetadata, ProviderSpec, ProxyBody, ProxyCoreResponse, RetryPolicy,
        RouteSelection, UpstreamEndpoint, UsageRecord, UsageTokens,
    };
    use crate::error::ProxyCoreError;
    use crate::ports::{
        AuthInfo, AuthProvider, ChannelHealthStore, ChannelSource, ForwardPipeline, ModelCatalog,
        ModelCatalogProvider, ProviderSource, ProxyAppConfig, ProxyConfigSource, ProxyCoreEvent,
        ProxyEventSink, ProxyGlobalConfig, ProxyRuntimeConfig, RoutePolicySource, RouteResolver,
        UsageSink,
    };
    use futures::future::BoxFuture;
    use http::{Method, StatusCode};
    use std::sync::Mutex;

    #[derive(Default)]
    struct TestServices {
        events: Mutex<Vec<ProxyCoreEvent>>,
        forwarded: Mutex<Vec<String>>,
        usage: Mutex<Vec<UsageRecord>>,
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
            Box::pin(async { Ok(vec![provider_spec()]) })
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
    }

    impl ChannelSource for TestServices {
        fn list_channels<'a>(
            &'a self,
            _query: crate::domain::ChannelQuery<'a>,
        ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelSpec>>> {
            Box::pin(async { Ok(vec![channel_spec()]) })
        }

        fn get_channel<'a>(
            &'a self,
            channel_id: &'a str,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelSpec>>> {
            Box::pin(async move { Ok((channel_id == "channel-a").then(channel_spec)) })
        }
    }

    impl RoutePolicySource for TestServices {
        fn load_policy<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<crate::domain::RoutePolicy>>> {
            Box::pin(async { Ok(None) })
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
    }

    impl ChannelHealthStore for TestServices {
        fn record_attempt<'a>(
            &'a self,
            _result: ChannelAttemptResult,
        ) -> BoxFuture<'a, ProxyCoreResult<()>> {
            Box::pin(async { Ok(()) })
        }

        fn reset_channel<'a>(&'a self, _channel_id: &'a str) -> BoxFuture<'a, ProxyCoreResult<()>> {
            Box::pin(async { Ok(()) })
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
}
