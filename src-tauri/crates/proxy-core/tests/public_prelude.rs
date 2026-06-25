use cc_switch_proxy_core::api::prelude::*;
use http::{Method, StatusCode};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct ExternalRelayServices {
    events: Mutex<Vec<ProxyCoreEvent>>,
    forwarded: Mutex<Vec<String>>,
    management_auth: Mutex<ManagementAuthRuntimeConfig>,
    usage: Mutex<Vec<UsageRecord>>,
}

fn unavailable<'a, T: Send + 'a>() -> BoxFuture<'a, ProxyCoreResult<T>> {
    Box::pin(async {
        Err(ProxyCoreError::Unavailable(
            "external smoke service is not configured".to_string(),
        ))
    })
}

fn provider_spec() -> ProviderSpec {
    ProviderSpec {
        id: "relay-a".to_string(),
        name: "Relay A".to_string(),
        kind: ProviderKind::Claude,
        account_ref: None,
        metadata: Default::default(),
    }
}

fn channel_spec() -> ChannelSpec {
    channel_spec_from_input(ChannelSpecInput {
        id: "channel-a".to_string(),
        provider_id: "relay-a".to_string(),
        app_type: AppKind::Claude.as_str().to_string(),
        name: "Channel A".to_string(),
        status: "enabled".to_string(),
        base_url: "https://relay.example/v1".to_string(),
        interface_kind: InterfaceKind::AnthropicMessages.as_str().to_string(),
        models: vec![ModelRouteInput {
            public_model: "sonnet".to_string(),
            upstream_model: "relay-sonnet".to_string(),
            ..ModelRouteInput::default()
        }],
        groups: vec![DEFAULT_ROUTE_GROUP.to_string()],
        priority: 100,
        weight: 1,
        ..ChannelSpecInput::default()
    })
}

impl ProxyConfigSource for ExternalRelayServices {
    fn list_apps<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<Vec<AppKind>>> {
        Box::pin(async { Ok(vec![AppKind::Claude]) })
    }

    fn load_global<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyGlobalConfig>> {
        Box::pin(async { Ok(ProxyGlobalConfig::default()) })
    }

    fn load_app<'a>(&'a self, app: &'a AppKind) -> BoxFuture<'a, ProxyCoreResult<ProxyAppConfig>> {
        Box::pin(async move {
            Ok(ProxyAppConfig {
                app: Some(app.clone()),
                enabled: true,
                ..ProxyAppConfig::default()
            })
        })
    }

    fn load_runtime<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeConfig>> {
        Box::pin(async { Ok(ProxyRuntimeConfig::default()) })
    }
}

impl ProviderSource for ExternalRelayServices {
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
        let provider = (provider_id == "relay-a").then(provider_spec);
        Box::pin(async move { Ok(provider) })
    }
}

impl ChannelSource for ExternalRelayServices {
    fn list_channels<'a>(
        &'a self,
        _query: ChannelQuery<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelSpec>>> {
        Box::pin(async { Ok(vec![channel_spec()]) })
    }

    fn get_channel<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelSpec>>> {
        let channel = (channel_id == "channel-a").then(channel_spec);
        Box::pin(async move { Ok(channel) })
    }
}

impl RoutePolicySource for ExternalRelayServices {
    fn load_policy<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<RoutePolicy>>> {
        Box::pin(async { Ok(None) })
    }
}

impl RouteResolver for ExternalRelayServices {
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
                attempts: vec![ChannelAttemptPlan {
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
        let app_type = request.app_type;
        let requested_model = request.requested_model;
        let interface_kind = request.interface_kind;
        let route_group = request
            .route_group
            .unwrap_or_else(|| DEFAULT_ROUTE_GROUP.to_string());

        Box::pin(async move {
            Ok(RouteResolveResponse {
                app_type,
                requested_model: requested_model.clone(),
                interface_kind: interface_kind.clone(),
                route_group: route_group.clone(),
                source: ChannelRouteSource::MaterializedChannels,
                candidates: vec![ChannelRouteCandidate {
                    channel_id: "channel-a".to_string(),
                    provider_id: "relay-a".to_string(),
                    channel_name: "Channel A".to_string(),
                    base_url: "https://relay.example/v1".to_string(),
                    interface_kind: interface_kind
                        .clone()
                        .unwrap_or_else(|| InterfaceKind::AnthropicMessages.as_str().to_string()),
                    public_model: requested_model.clone(),
                    upstream_model: requested_model.as_ref().map(|_| "relay-sonnet".to_string()),
                    route_group: route_group.clone(),
                    priority: 100,
                    weight: 1,
                    source_kind: "manual".to_string(),
                }],
                rejected: vec![ChannelRouteRejected {
                    channel_id: "channel-b".to_string(),
                    provider_id: "relay-b".to_string(),
                    channel_name: "Channel B".to_string(),
                    reasons: vec!["interface_mismatch".to_string()],
                }],
            })
        })
    }
}

impl ChannelHealthStore for ExternalRelayServices {
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
        let channel_id = channel_id.to_string();
        Box::pin(async move {
            Ok(ChannelHealthReset {
                channel_id,
                app: AppKind::Claude,
            })
        })
    }
}

impl ChannelReachabilityProbe for ExternalRelayServices {
    fn probe_channel<'a>(
        &'a self,
        _request: ChannelTestProbeRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelReachabilityResult>> {
        unavailable()
    }
}

impl AuthProvider for ExternalRelayServices {
    fn resolve_auth<'a>(
        &'a self,
        _app: &'a AppKind,
        _provider: &'a ProviderSpec,
        _channel: &'a ChannelSpec,
        _request: &'a ProxyRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>> {
        Box::pin(async { Ok(AuthInfo::default()) })
    }
}

impl ChannelKeyRuntimeSource for ExternalRelayServices {
    fn load_channel_key_value(
        &self,
        _channel_id: &str,
        _key_ref: &str,
    ) -> ProxyCoreResult<Option<String>> {
        Ok(None)
    }
}

impl ModelCatalogProvider for ExternalRelayServices {
    fn load_catalog<'a>(
        &'a self,
        _app: &'a AppKind,
        provider_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
        let provider_id = provider_id.to_string();
        Box::pin(async move {
            Ok(ModelCatalog {
                provider_id,
                models: vec!["sonnet".to_string()],
                raw: json!({ "data": [{ "id": "sonnet" }] }),
            })
        })
    }

    fn load_client_catalog<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
        self.load_catalog(app, "client")
    }

    fn load_claude_desktop_model_routes<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ClaudeDesktopModelRouteInput>>> {
        Box::pin(async {
            Ok(vec![
                ClaudeDesktopModelRouteInput::new("claude-sonnet-4-6", true),
                ClaudeDesktopModelRouteInput::new("claude-haiku-4-5", false),
            ])
        })
    }
}

impl RuntimeStatusSource for ExternalRelayServices {
    fn load_status<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeStatus>> {
        Box::pin(async { Ok(ProxyRuntimeStatus::default()) })
    }
}

impl ClaudeDesktopGatewayAuthSource for ExternalRelayServices {
    fn load_gateway_token<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<String>> {
        Box::pin(async { Ok("gateway-token".to_string()) })
    }
}

impl ManagementAuthSource for ExternalRelayServices {
    fn load_management_auth_config<'a>(
        &'a self,
    ) -> BoxFuture<'a, ProxyCoreResult<ManagementAuthRuntimeConfig>> {
        Box::pin(async {
            Ok(self
                .management_auth
                .lock()
                .expect("management auth mutex")
                .clone())
        })
    }
}

impl UsageSink for ExternalRelayServices {
    fn record_usage<'a>(&'a self, record: UsageRecord) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move {
            self.usage.lock().expect("usage mutex").push(record);
            Ok(())
        })
    }
}

impl ProxyEventSink for ExternalRelayServices {
    fn emit_event<'a>(&'a self, event: ProxyCoreEvent) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move {
            self.events.lock().expect("events mutex").push(event);
            Ok(())
        })
    }
}

impl ForwardPipeline for ExternalRelayServices {
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

            let selected_route = plan.selection;
            let outbound_model = selected_route
                .model_route
                .as_ref()
                .map(|route| route.upstream_model.clone());
            let response_model = outbound_model.clone();

            Ok(ProxyResult {
                response: ProxyCoreResponse::empty(StatusCode::OK),
                selected_route,
                outbound_model: outbound_model.clone(),
                usage_record: Some(UsageRecord {
                    request_id: request.client_request_id.clone(),
                    message_id: Some("msg-1".to_string()),
                    app: request.app.clone(),
                    provider_id: "relay-a".to_string(),
                    provider_kind: Some(ProviderKind::Claude),
                    channel_id: Some("channel-a".to_string()),
                    channel_name: Some("Channel A".to_string()),
                    route_group: Some(
                        request
                            .route_group
                            .clone()
                            .unwrap_or_else(|| DEFAULT_ROUTE_GROUP.to_string()),
                    ),
                    request_model: request.requested_model.clone().unwrap_or_default(),
                    outbound_model: outbound_model.unwrap_or_default(),
                    response_model,
                    pricing_model: None,
                    tokens: UsageTokens {
                        input_tokens: 3,
                        output_tokens: 5,
                        cache_read_tokens: 0,
                        cache_creation_tokens: 0,
                    },
                    latency_ms: 7,
                    first_token_ms: Some(2),
                    status_code: 200,
                    error_message: None,
                    session_id: None,
                    is_streaming: false,
                    metadata: json!({ "source": "public-prelude-smoke" }),
                }),
                metadata: json!({ "source": "public-prelude-smoke" }),
            })
        })
    }
}

impl ProxyServices for ExternalRelayServices {
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

    fn reachability_probe(&self) -> &(dyn ChannelReachabilityProbe + Send + Sync) {
        self
    }

    fn auth_provider(&self) -> &(dyn AuthProvider + Send + Sync) {
        self
    }

    fn channel_key_runtime_source(&self) -> &(dyn ChannelKeyRuntimeSource + Send + Sync) {
        self
    }

    fn model_catalog(&self) -> &(dyn ModelCatalogProvider + Send + Sync) {
        self
    }

    fn runtime_status_source(&self) -> &(dyn RuntimeStatusSource + Send + Sync) {
        self
    }

    fn claude_desktop_gateway_auth_source(
        &self,
    ) -> &(dyn ClaudeDesktopGatewayAuthSource + Send + Sync) {
        self
    }

    fn management_auth_source(&self) -> &(dyn ManagementAuthSource + Send + Sync) {
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

#[test]
fn external_host_can_construct_engine_and_handle_request_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services.clone());
    let mut request = ProxyRequest::new(
        AppKind::Claude,
        Method::POST,
        "/v1/messages",
        InterfaceKind::AnthropicMessages,
        ProxyBody::Json(json!({ "model": "sonnet", "messages": [] })),
    );
    request.client_request_id = Some("req-1".to_string());
    request.requested_model = Some("sonnet".to_string());

    let result = futures::executor::block_on(engine.handle(request)).expect("handle request");

    assert_eq!(engine.status().accepted_requests, 1);
    assert_eq!(result.response.status, StatusCode::OK);
    assert_eq!(result.selected_route.provider.id, "relay-a");
    assert_eq!(result.selected_route.channel.id, "channel-a");
    assert_eq!(result.outbound_model.as_deref(), Some("relay-sonnet"));
    assert_eq!(
        services
            .forwarded
            .lock()
            .expect("forwarded mutex")
            .as_slice(),
        ["POST /v1/messages"]
    );

    let events = services.events.lock().expect("events mutex");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, ProxyCoreEventType::RouteSelected);
    assert_eq!(events[0].request_id.as_deref(), Some("req-1"));
    assert_eq!(events[0].channel_id.as_deref(), Some("channel-a"));

    let usage = services.usage.lock().expect("usage mutex");
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].provider_id, "relay-a");
    assert_eq!(usage[0].request_model, "sonnet");
    assert_eq!(usage[0].outbound_model, "relay-sonnet");
    assert_eq!(usage[0].tokens.output_tokens, 5);
}

#[test]
fn external_host_can_use_gateway_model_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        http::HeaderValue::from_static("Bearer gateway-token"),
    );

    futures::executor::block_on(engine.validate_claude_desktop_gateway_auth(&headers))
        .expect("gateway auth");
    let response: ClaudeDesktopModelListResponse =
        futures::executor::block_on(engine.claude_desktop_model_list_response())
            .expect("desktop model list");

    assert_eq!(response.data.len(), 2);
    assert_eq!(response.first_id.as_deref(), Some("claude-sonnet-4-6"));
    assert_eq!(response.last_id.as_deref(), Some("claude-haiku-4-5"));
    assert_eq!(response.data[0].id, "claude-sonnet-4-6");
    assert!(response.data[0].supports_1m);
    assert_eq!(response.data[1].id, "claude-haiku-4-5");
    assert!(!response.data[1].supports_1m);
}

#[test]
fn external_host_can_use_management_auth_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    *services
        .management_auth
        .lock()
        .expect("management auth mutex") = ManagementAuthRuntimeConfig::new(
        "0.0.0.0",
        Some("management-token".to_string()),
        None,
    );
    let engine = ProxyEngine::new(services);
    let mut headers = http::HeaderMap::new();

    let rejected =
        futures::executor::block_on(engine.validate_management_auth(&headers)).unwrap_err();
    assert!(
        matches!(rejected, ProxyCoreError::Auth(message) if message == "Missing management bearer token")
    );

    headers.insert(
        http::header::AUTHORIZATION,
        http::HeaderValue::from_static("Bearer management-token"),
    );
    futures::executor::block_on(engine.validate_management_auth(&headers))
        .expect("management auth");
}

#[test]
fn external_host_can_use_route_resolve_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let request = RouteResolveManagementRequest::from_body(RouteResolveRequest {
        app_type: "claude".to_string(),
        requested_model: Some("sonnet".to_string()),
        interface_kind: Some("anthropic".to_string()),
        route_group: Some("premium".to_string()),
    })
    .expect("route resolve request");

    let response: RouteResolveResponse =
        futures::executor::block_on(engine.resolve_route_response(request))
            .expect("route resolve response");
    let candidates: &[ChannelRouteCandidate] = response.candidates.as_slice();
    let rejected: &[ChannelRouteRejected] = response.rejected.as_slice();
    let source: ChannelRouteSource = response.source.clone();

    assert_eq!(response.app_type, "claude");
    assert_eq!(response.requested_model.as_deref(), Some("sonnet"));
    assert_eq!(response.interface_kind.as_deref(), Some("anthropic"));
    assert_eq!(response.route_group, "premium");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].channel_id, "channel-a");
    assert_eq!(candidates[0].public_model.as_deref(), Some("sonnet"));
    assert_eq!(candidates[0].upstream_model.as_deref(), Some("relay-sonnet"));
    assert_eq!(candidates[0].route_group, "premium");
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].channel_id, "channel-b");
    assert_eq!(rejected[0].reasons.as_slice(), ["interface_mismatch"]);
    assert_eq!(source, ChannelRouteSource::MaterializedChannels);
    assert_eq!(source.as_str(), "materialized_channels");
}
