use cc_switch_proxy_core::api::prelude::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct DemoRelayHost {
    usage: Mutex<Vec<UsageRecord>>,
    events: Mutex<Vec<ProxyCoreEvent>>,
}

fn main() -> ProxyCoreResult<()> {
    futures::executor::block_on(run())
}

async fn run() -> ProxyCoreResult<()> {
    let services = Arc::new(DemoRelayHost::default());
    let engine = ProxyEngine::new(services.clone());

    let status = engine
        .proxy_status_response(ProxyStatusRequest::new())
        .await?;
    let route = engine
        .resolve_route_response(RouteResolveManagementRequest::from_body(
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("sonnet".to_string()),
                interface_kind: Some(InterfaceKind::AnthropicMessages.as_str().to_string()),
                route_group: Some("premium".to_string()),
            },
        )?)
        .await?;
    let channels = engine
        .channel_list_response(ChannelListRequest::from_query(
            serde_json::from_value(json!({ "appType": "claude" }))
                .map_err(|error| ProxyCoreError::InvalidRequest(error.to_string()))?,
        )?)
        .await?;

    let mut request = ProxyRequest::new(
        AppKind::Claude,
        Method::POST,
        "/v1/messages",
        InterfaceKind::AnthropicMessages,
        ProxyBody::Json(json!({ "model": "sonnet", "messages": [] })),
    );
    request.requested_model = Some("sonnet".to_string());
    request.route_group = Some("premium".to_string());

    let forwarded = engine.handle(request).await?;
    let usage_count = services
        .usage
        .lock()
        .expect("usage mutex")
        .len();
    let event_count = services
        .events
        .lock()
        .expect("events mutex")
        .len();
    let selected = route
        .candidates
        .first()
        .ok_or_else(|| ProxyCoreError::Unavailable("missing route candidate".to_string()))?;
    let listed = channels
        .channels
        .first()
        .ok_or_else(|| ProxyCoreError::Unavailable("missing listed channel".to_string()))?;

    println!(
        "relay host ready: running={} route={} channel_base={} upstream={} usage_records={} events={}",
        status.status.running,
        selected.channel_id,
        listed.base_url,
        forwarded.outbound_model.as_deref().unwrap_or("unknown"),
        usage_count,
        event_count
    );

    Ok(())
}

fn provider_specs(app: &AppKind) -> Vec<ProviderSpec> {
    match app {
        AppKind::Claude => vec![ProviderSpec {
            id: "relay-east".to_string(),
            name: "Relay East".to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: Default::default(),
        }],
        AppKind::Codex => vec![ProviderSpec {
            id: "relay-west".to_string(),
            name: "Relay West".to_string(),
            kind: ProviderKind::Codex,
            account_ref: None,
            metadata: Default::default(),
        }],
        _ => Vec::new(),
    }
}

fn channel_specs() -> Vec<ChannelSpec> {
    vec![
        channel_spec_from_input(ChannelSpecInput {
            id: "claude-premium".to_string(),
            provider_id: "relay-east".to_string(),
            app_type: AppKind::Claude.as_str().to_string(),
            name: "Claude Premium".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay-east.example/v1".to_string(),
            interface_kind: InterfaceKind::AnthropicMessages.as_str().to_string(),
            models: vec![ModelRouteInput {
                public_model: "sonnet".to_string(),
                upstream_model: "anthropic/claude-sonnet-4-6".to_string(),
                pricing_model: Some("anthropic-standard".to_string()),
                ..ModelRouteInput::default()
            }],
            groups: vec![DEFAULT_ROUTE_GROUP.to_string(), "premium".to_string()],
            priority: 100,
            weight: 2,
            metadata: json!({ "tenant": "external-relay-demo" }),
            ..ChannelSpecInput::default()
        }),
        channel_spec_from_input(ChannelSpecInput {
            id: "codex-responses".to_string(),
            provider_id: "relay-west".to_string(),
            app_type: AppKind::Codex.as_str().to_string(),
            name: "Codex Responses".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay-west.example/responses".to_string(),
            interface_kind: InterfaceKind::OpenAiResponses.as_str().to_string(),
            models: vec![ModelRouteInput {
                public_model: "gpt-5.4".to_string(),
                upstream_model: "openai/gpt-5.4".to_string(),
                pricing_model: Some("responses-standard".to_string()),
                ..ModelRouteInput::default()
            }],
            groups: vec![DEFAULT_ROUTE_GROUP.to_string()],
            priority: 90,
            weight: 1,
            metadata: json!({ "tenant": "external-relay-demo" }),
            ..ChannelSpecInput::default()
        }),
    ]
}

fn channel_record_from_spec(channel: ChannelSpec) -> ChannelRecord {
    let models = channel
        .models
        .into_iter()
        .map(|model| ChannelModelRecordInput {
            channel_id: channel.id.clone(),
            public_model: model.public_model,
            upstream_model: model.upstream_model,
            capabilities: json!(model.capabilities),
            pricing_model: model.pricing_model,
            request_overrides: model.request_overrides,
            response_overrides: model.response_overrides,
        })
        .collect();

    channel_record_from_input(ChannelRecordInput {
        id: channel.id,
        provider_id: channel.provider_id,
        app_type: channel.app.as_str().to_string(),
        name: channel.name,
        status: "enabled".to_string(),
        base_url: channel.endpoint.base_url,
        interface_kind: channel.interface.as_str().to_string(),
        groups: channel.groups,
        priority: channel.priority,
        weight: channel.weight,
        metadata: channel.metadata,
        source_kind: "external_host".to_string(),
        models,
        ..ChannelRecordInput::default()
    })
}

fn management_candidate(
    request: &RouteResolveRequest,
    channel: &ChannelSpec,
) -> ChannelRouteCandidate {
    let model = request
        .requested_model
        .as_ref()
        .and_then(|requested| {
            channel
                .models
                .iter()
                .find(|model| {
                    model.public_model == *requested || model.upstream_model == *requested
                })
        })
        .or_else(|| channel.models.first());

    ChannelRouteCandidate {
        channel_id: channel.id.clone(),
        provider_id: channel.provider_id.clone(),
        channel_name: channel.name.clone(),
        base_url: channel.endpoint.base_url.clone(),
        interface_kind: channel.interface.as_str().to_string(),
        public_model: model.map(|model| model.public_model.clone()),
        upstream_model: model.map(|model| model.upstream_model.clone()),
        route_group: request
            .route_group
            .clone()
            .unwrap_or_else(|| DEFAULT_ROUTE_GROUP.to_string()),
        priority: channel.priority,
        weight: channel.weight,
        source_kind: "external_host".to_string(),
    }
}

impl ProxyConfigSource for DemoRelayHost {
    fn list_apps<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<Vec<AppKind>>> {
        Box::pin(async { Ok(vec![AppKind::Claude, AppKind::Codex]) })
    }

    fn load_global<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyGlobalConfig>> {
        Box::pin(async {
            Ok(ProxyGlobalConfig {
                bind_host: Some("127.0.0.1".to_string()),
                bind_port: Some(15721),
                ..ProxyGlobalConfig::default()
            })
        })
    }

    fn load_app<'a>(&'a self, app: &'a AppKind) -> BoxFuture<'a, ProxyCoreResult<ProxyAppConfig>> {
        Box::pin(async move {
            Ok(ProxyAppConfig {
                app: Some(app.clone()),
                enabled: true,
                default_group: Some(DEFAULT_ROUTE_GROUP.to_string()),
                ..ProxyAppConfig::default()
            })
        })
    }

    fn load_runtime<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeConfig>> {
        Box::pin(async { Ok(ProxyRuntimeConfig::default()) })
    }
}

impl ProviderSource for DemoRelayHost {
    fn list_providers<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ProviderSpec>>> {
        Box::pin(async move { Ok(provider_specs(app)) })
    }

    fn get_provider<'a>(
        &'a self,
        app: &'a AppKind,
        provider_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ProviderSpec>>> {
        Box::pin(async move {
            Ok(provider_specs(app)
                .into_iter()
                .find(|provider| provider.id == provider_id))
        })
    }

    fn current_provider_id<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<String>>> {
        Box::pin(async move { Ok(provider_specs(app).first().map(|provider| provider.id.clone())) })
    }
}

impl ChannelSource for DemoRelayHost {
    fn list_channels<'a>(
        &'a self,
        query: ChannelQuery<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelSpec>>> {
        Box::pin(async move {
            let requested_group = query.group.unwrap_or(DEFAULT_ROUTE_GROUP);
            let channels = channel_specs()
                .into_iter()
                .filter(|channel| channel.app == *query.app)
                .filter(|channel| {
                    query
                        .provider_id
                        .map(|provider_id| channel.provider_id == provider_id)
                        .unwrap_or(true)
                })
                .filter(|channel| channel.groups.iter().any(|group| group == requested_group))
                .filter(|channel| {
                    query
                        .model
                        .map(|requested| {
                            channel.models.iter().any(|model| {
                                model.public_model == requested || model.upstream_model == requested
                            })
                        })
                        .unwrap_or(true)
                })
                .collect();

            Ok(channels)
        })
    }

    fn get_channel<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelSpec>>> {
        Box::pin(async move {
            Ok(channel_specs()
                .into_iter()
                .find(|channel| channel.id == channel_id))
        })
    }

    fn list_materialized_channel_records<'a>(
        &'a self,
        app: Option<&'a AppKind>,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelRecord>>> {
        Box::pin(async move {
            let channels = channel_specs()
                .into_iter()
                .filter(|channel| app.map(|app| channel.app == *app).unwrap_or(true))
                .map(channel_record_from_spec)
                .collect();

            Ok(channels)
        })
    }
}

impl RoutePolicySource for DemoRelayHost {
    fn load_policy<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<RoutePolicy>>> {
        Box::pin(async { Ok(None) })
    }
}

impl RouteResolver for DemoRelayHost {
    fn resolve<'a>(
        &'a self,
        request: RouteRequest<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<RoutePlan>> {
        Box::pin(async move { build_route_plan(request) })
    }

    fn resolve_management_route<'a>(
        &'a self,
        request: RouteResolveRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<RouteResolveResponse>> {
        Box::pin(async move {
            let app = AppKind::from(request.app_type.as_str());
            let requested_group = request
                .route_group
                .clone()
                .unwrap_or_else(|| DEFAULT_ROUTE_GROUP.to_string());
            let channel = channel_specs()
                .into_iter()
                .find(|channel| {
                    channel.app == app
                        && channel.groups.iter().any(|group| group == &requested_group)
                        && request
                            .interface_kind
                            .as_deref()
                            .map(|interface| channel.interface.as_str() == interface)
                            .unwrap_or(true)
                })
                .ok_or_else(|| ProxyCoreError::Unavailable("no management route".to_string()))?;

            Ok(RouteResolveResponse {
                app_type: request.app_type.clone(),
                requested_model: request.requested_model.clone(),
                interface_kind: request.interface_kind.clone(),
                route_group: requested_group,
                source: ChannelRouteSource::MaterializedChannels,
                candidates: vec![management_candidate(&request, &channel)],
                rejected: Vec::new(),
            })
        })
    }
}

impl ChannelHealthStore for DemoRelayHost {
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

impl ChannelReachabilityProbe for DemoRelayHost {
    fn probe_channel<'a>(
        &'a self,
        request: ChannelTestProbeRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelReachabilityResult>> {
        Box::pin(async move {
            Ok(ChannelReachabilityResult::from_input(
                ChannelReachabilityInput {
                    success: true,
                    status: ChannelReachabilityStatus::Operational,
                    message: format!("{} is reachable", request.base_url),
                    latency_ms: Some(80),
                    http_status: Some(200),
                    tested_at: 1_772_000_000,
                    retry_count: 0,
                },
            ))
        })
    }
}

impl AuthProvider for DemoRelayHost {
    fn resolve_auth<'a>(
        &'a self,
        _app: &'a AppKind,
        _provider: &'a ProviderSpec,
        _channel: &'a ChannelSpec,
        _request: &'a ProxyRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>> {
        Box::pin(async {
            Ok(AuthInfo {
                headers: vec![("authorization".to_string(), "Bearer sk-relay".to_string())],
                account_ref: Some("external-relay-account".to_string()),
                metadata: json!({ "source": "external-host" }),
            })
        })
    }
}

impl ChannelKeyRuntimeSource for DemoRelayHost {
    fn load_channel_key_value(
        &self,
        _channel_id: &str,
        _key_ref: &str,
    ) -> ProxyCoreResult<Option<String>> {
        Ok(Some("sk-relay".to_string()))
    }
}

impl ModelCatalogProvider for DemoRelayHost {
    fn load_catalog<'a>(
        &'a self,
        app: &'a AppKind,
        provider_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
        Box::pin(async move {
            let models = channel_specs()
                .into_iter()
                .filter(|channel| channel.app == *app && channel.provider_id == provider_id)
                .flat_map(|channel| channel.models.into_iter().map(|model| model.public_model))
                .collect::<Vec<_>>();

            Ok(ModelCatalog {
                provider_id: provider_id.to_string(),
                raw: json!({ "models": models }),
                models,
            })
        })
    }

    fn load_client_catalog<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
        Box::pin(async move {
            let provider_id = provider_specs(app)
                .first()
                .map(|provider| provider.id.clone())
                .unwrap_or_else(|| "none".to_string());
            self.load_catalog(app, &provider_id).await
        })
    }
}

impl RuntimeStatusSource for DemoRelayHost {
    fn load_status<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeStatus>> {
        Box::pin(async {
            Ok(ProxyRuntimeStatus {
                running: true,
                address: "127.0.0.1".to_string(),
                port: 15721,
                total_requests: 12,
                success_requests: 12,
                success_rate: 100.0,
                active_targets: vec![CurrentRouteTarget {
                    app_type: "claude".to_string(),
                    provider_name: "Relay East".to_string(),
                    provider_id: "relay-east".to_string(),
                    channel_id: Some("claude-premium".to_string()),
                    channel_name: Some("Claude Premium".to_string()),
                    interface_kind: Some(InterfaceKind::AnthropicMessages.as_str().to_string()),
                    public_model: Some("sonnet".to_string()),
                    upstream_model: Some("anthropic/claude-sonnet-4-6".to_string()),
                }],
                ..ProxyRuntimeStatus::default()
            })
        })
    }
}

impl ClaudeDesktopGatewayAuthSource for DemoRelayHost {
    fn load_gateway_token<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<String>> {
        Box::pin(async { Ok("gateway-token".to_string()) })
    }
}

impl ManagementAuthSource for DemoRelayHost {
    fn load_management_auth_config<'a>(
        &'a self,
    ) -> BoxFuture<'a, ProxyCoreResult<ManagementAuthRuntimeConfig>> {
        Box::pin(async { Ok(ManagementAuthRuntimeConfig::default()) })
    }
}

impl UsageSink for DemoRelayHost {
    fn record_usage<'a>(&'a self, record: UsageRecord) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move {
            self.usage.lock().expect("usage mutex").push(record);
            Ok(())
        })
    }
}

impl ProxyEventSink for DemoRelayHost {
    fn emit_event<'a>(&'a self, event: ProxyCoreEvent) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move {
            self.events.lock().expect("events mutex").push(event);
            Ok(())
        })
    }
}

impl ForwardPipeline for DemoRelayHost {
    fn forward<'a>(
        &'a self,
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>> {
        Box::pin(async move {
            let selected_route = plan.selection;
            let outbound_model = selected_route
                .model_route
                .as_ref()
                .map(|model| model.upstream_model.clone());

            Ok(ProxyResult {
                response: ProxyCoreResponse::with_body(
                    StatusCode::OK,
                    HeaderMap::new(),
                    ProxyResponseBody::json(json!({ "ok": true })),
                ),
                selected_route: selected_route.clone(),
                outbound_model: outbound_model.clone(),
                usage_record: Some(UsageRecord {
                    request_id: request.client_request_id.clone(),
                    message_id: Some("msg-external-relay".to_string()),
                    app: request.app,
                    provider_id: selected_route.provider.id.clone(),
                    provider_kind: Some(selected_route.provider.kind.clone()),
                    channel_id: Some(selected_route.channel.id.clone()),
                    channel_name: Some(selected_route.channel.name.clone()),
                    route_group: request.route_group.clone(),
                    request_model: request.requested_model.unwrap_or_default(),
                    outbound_model: outbound_model.clone().unwrap_or_default(),
                    response_model: outbound_model,
                    pricing_model: selected_route
                        .model_route
                        .as_ref()
                        .and_then(|model| model.pricing_model.clone()),
                    tokens: UsageTokens {
                        input_tokens: 32,
                        output_tokens: 12,
                        cache_read_tokens: 0,
                        cache_creation_tokens: 0,
                    },
                    latency_ms: 96,
                    first_token_ms: Some(34),
                    status_code: 200,
                    error_message: None,
                    session_id: None,
                    is_streaming: false,
                    metadata: json!({ "host": "external_relay_host" }),
                }),
                metadata: json!({ "host": "external_relay_host" }),
            })
        })
    }
}

impl ProxyServices for DemoRelayHost {
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
