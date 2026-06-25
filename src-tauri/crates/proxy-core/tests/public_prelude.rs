use cc_switch_proxy_core::api::prelude::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct ExternalRelayServices {
    events: Mutex<Vec<ProxyCoreEvent>>,
    forwarded: Mutex<Vec<String>>,
    management_auth: Mutex<ManagementAuthRuntimeConfig>,
    usage: Mutex<Vec<UsageRecord>>,
    clock: Mutex<i64>,
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

fn channel_record(groups: Vec<String>) -> ChannelRecord {
    ChannelRecord {
        id: "channel-a".to_string(),
        provider_id: "relay-a".to_string(),
        app_type: "claude".to_string(),
        name: "Channel A".to_string(),
        status: "enabled".to_string(),
        base_url: "https://relay.example/v1".to_string(),
        interface_kind: "anthropic".to_string(),
        auth_profile_ref: None,
        groups,
        priority: 100,
        weight: 1,
        retry_policy: json!({}),
        health_policy: json!({}),
        header_overrides: json!({}),
        param_overrides: json!({}),
        status_code_mapping: json!({}),
        tags: Vec::new(),
        metadata: json!({ "source": "public-prelude-smoke" }),
        source_kind: "manual".to_string(),
        source_endpoint_url: None,
        models: Vec::new(),
        needs_review: false,
        review_reasons: Vec::new(),
    }
}

fn channel_key_record(
    key_ref: impl Into<String>,
    status: impl Into<String>,
    priority: i64,
    weight: u32,
) -> ChannelKeyRecord {
    channel_key_record_from_input(ChannelKeyRecordInput {
        channel_id: "channel-a".to_string(),
        key_ref: key_ref.into(),
        status: status.into(),
        priority,
        weight,
        last_failure_at: None,
    })
}

fn channel_model_record(
    public_model: impl Into<String>,
    upstream_model: impl Into<String>,
) -> ChannelModelRecord {
    channel_model_record_from_input(ChannelModelRecordInput {
        channel_id: "channel-a".to_string(),
        public_model: public_model.into(),
        upstream_model: upstream_model.into(),
        capabilities: json!({ "streaming": true }),
        pricing_model: Some("relay-standard".to_string()),
        request_overrides: json!({}),
        response_overrides: json!({}),
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

    fn current_provider_id<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<String>>> {
        Box::pin(async { Ok(Some("relay-a".to_string())) })
    }

    fn active_route_target<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<CurrentRouteTarget>>> {
        Box::pin(async {
            Ok(Some(CurrentRouteTarget {
                app_type: "claude".to_string(),
                provider_name: "Relay A".to_string(),
                provider_id: "relay-a".to_string(),
                channel_id: Some("channel-a".to_string()),
                channel_name: Some("Channel A".to_string()),
                interface_kind: Some("anthropic".to_string()),
                public_model: Some("sonnet".to_string()),
                upstream_model: Some("relay-sonnet".to_string()),
            }))
        })
    }

    fn route_candidate_provider_ids<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<String>>> {
        Box::pin(async { Ok(vec!["relay-a".to_string()]) })
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

    fn list_materialized_channel_records<'a>(
        &'a self,
        _app: Option<&'a AppKind>,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelRecord>>> {
        Box::pin(async { Ok(vec![channel_record(vec![DEFAULT_ROUTE_GROUP.to_string()])]) })
    }

    fn list_channel_records<'a>(
        &'a self,
        _app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<(ChannelRouteSource, Vec<ChannelRecord>)>> {
        Box::pin(async {
            Ok((
                ChannelRouteSource::MaterializedChannels,
                vec![channel_record(vec![
                    DEFAULT_ROUTE_GROUP.to_string(),
                    "premium".to_string(),
                ])],
            ))
        })
    }

    fn preview_legacy_channel_migration<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelMigrationPreviewInput<ChannelRecord>>> {
        let app_type = app.as_str().to_string();
        Box::pin(async move {
            Ok(ChannelMigrationPreviewInput::new(
                app_type,
                vec![channel_record(vec![DEFAULT_ROUTE_GROUP.to_string()])],
                2,
                1,
            ))
        })
    }

    fn materialize_legacy_channel_migration<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelMigrationMaterializeInput>> {
        let app_type = app.as_str().to_string();
        Box::pin(async move {
            Ok(ChannelMigrationMaterializeInput::new(
                app_type, 3, 2, 4, 2, 1, 1,
            ))
        })
    }

    fn create_channel_record<'a>(
        &'a self,
        request: ProxyChannelWriteRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelRecord>> {
        Box::pin(async move {
            let mut channel = channel_record(request.groups);
            channel.id = request.id.unwrap_or_else(|| "created-channel".to_string());
            channel.provider_id = request.provider_id;
            channel.app_type = request.app_type;
            channel.name = request.name;
            channel.status = request.status;
            channel.base_url = request.base_url;
            channel.interface_kind = request.interface_kind;
            channel.auth_profile_ref = request.auth_profile_ref;
            channel.priority = request.priority;
            channel.weight = request.weight;
            channel.retry_policy = request.retry_policy;
            channel.health_policy = request.health_policy;
            channel.header_overrides = request.header_overrides;
            channel.param_overrides = request.param_overrides;
            channel.status_code_mapping = request.status_code_mapping;
            channel.tags = request.tags;
            channel.metadata = request.metadata;
            Ok(channel)
        })
    }

    fn get_channel_record<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelRecord>>> {
        let channel = (channel_id == "channel-a")
            .then(|| channel_record(vec![DEFAULT_ROUTE_GROUP.to_string(), "premium".to_string()]));
        Box::pin(async move { Ok(channel) })
    }

    fn update_channel_record<'a>(
        &'a self,
        channel_id: &'a str,
        patch: ProxyChannelPatchRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelRecord>>> {
        let mut channel = channel_record(vec![DEFAULT_ROUTE_GROUP.to_string()]);
        channel.id = channel_id.to_string();
        if let Some(name) = patch.name {
            channel.name = name;
        }
        if let Some(status) = patch.status {
            channel.status = status;
        }
        if let Some(base_url) = patch.base_url {
            channel.base_url = base_url;
        }
        if let Some(interface_kind) = patch.interface_kind {
            channel.interface_kind = interface_kind;
        }
        if let Some(groups) = patch.groups {
            channel.groups = groups;
        }
        if let Some(priority) = patch.priority {
            channel.priority = priority;
        }
        if let Some(weight) = patch.weight {
            channel.weight = weight;
        }
        Box::pin(async move { Ok(Some(channel)) })
    }

    fn delete_channel_record<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<bool>> {
        let deleted = channel_id == "channel-a";
        Box::pin(async move { Ok(deleted) })
    }

    fn list_channel_key_records<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<Vec<ChannelKeyRecord>>>> {
        let keys = (channel_id == "channel-a")
            .then(|| vec![channel_key_record("primary", "enabled", 10, 1)]);
        Box::pin(async move { Ok(keys) })
    }

    fn upsert_channel_key_record<'a>(
        &'a self,
        channel_id: &'a str,
        key_ref: &'a str,
        request: ProxyChannelKeyWriteRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelKeyRecord>>> {
        let channel_id = channel_id.to_string();
        let key_ref = key_ref.to_string();
        Box::pin(async move {
            if channel_id != "channel-a" {
                return Ok(None);
            }

            Ok(Some(channel_key_record(
                key_ref,
                request.status,
                request.priority,
                request.weight,
            )))
        })
    }

    fn update_channel_key_record<'a>(
        &'a self,
        channel_id: &'a str,
        key_ref: &'a str,
        patch: ProxyChannelKeyPatchRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelKeyRecord>>> {
        let channel_id = channel_id.to_string();
        let key_ref = key_ref.to_string();
        Box::pin(async move {
            if channel_id != "channel-a" {
                return Ok(None);
            }

            Ok(Some(channel_key_record(
                key_ref,
                patch.status.unwrap_or_else(|| "enabled".to_string()),
                patch.priority.unwrap_or(10),
                patch.weight.unwrap_or(1),
            )))
        })
    }

    fn delete_channel_key_record<'a>(
        &'a self,
        channel_id: &'a str,
        key_ref: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<bool>> {
        let deleted = channel_id == "channel-a" && key_ref == "primary";
        Box::pin(async move { Ok(deleted) })
    }

    fn list_channel_model_records<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<Vec<ChannelModelRecord>>>> {
        let models = (channel_id == "channel-a")
            .then(|| vec![channel_model_record("sonnet", "relay-sonnet")]);
        Box::pin(async move { Ok(models) })
    }

    fn replace_channel_model_records<'a>(
        &'a self,
        channel_id: &'a str,
        request: ProxyChannelModelsReplaceRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<Vec<ChannelModelRecord>>>> {
        let channel_id = channel_id.to_string();
        Box::pin(async move {
            if channel_id != "channel-a" {
                return Ok(None);
            }

            let models = request
                .models
                .into_iter()
                .map(|model| {
                    channel_model_record_from_input(ChannelModelRecordInput {
                        channel_id: channel_id.clone(),
                        public_model: model.public_model,
                        upstream_model: model.upstream_model,
                        capabilities: model.capabilities,
                        pricing_model: model.pricing_model,
                        request_overrides: model.request_overrides,
                        response_overrides: model.response_overrides,
                    })
                })
                .collect();
            Ok(Some(models))
        })
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

    fn channel_breaker_stats<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelBreakerStats>> {
        let channel_id = channel_id.to_string();
        Box::pin(async move {
            Ok(ChannelBreakerStats {
                channel_id,
                app: AppKind::Claude,
                stats: None,
            })
        })
    }
}

impl ChannelReachabilityProbe for ExternalRelayServices {
    fn probe_channel<'a>(
        &'a self,
        request: ChannelTestProbeRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelReachabilityResult>> {
        Box::pin(async move {
            Ok(ChannelReachabilityResult::from_input(
                ChannelReachabilityInput {
                    success: true,
                    status: ChannelReachabilityStatus::Degraded,
                    message: format!("reachable: {}", request.base_url),
                    latency_ms: Some(6100),
                    http_status: Some(200),
                    tested_at: 1_771_000_123,
                    retry_count: 1,
                },
            ))
        })
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
        Box::pin(async {
            Ok(ProxyRuntimeStatus {
                running: true,
                address: "127.0.0.1".to_string(),
                port: 4100,
                active_connections: 2,
                total_requests: 8,
                success_requests: 6,
                failed_requests: 2,
                success_rate: 75.0,
                uptime_seconds: 42,
                current_provider: Some("Relay A".to_string()),
                current_provider_id: Some("relay-a".to_string()),
                active_targets: vec![CurrentRouteTarget {
                    app_type: "claude".to_string(),
                    provider_name: "Relay A".to_string(),
                    provider_id: "relay-a".to_string(),
                    channel_id: Some("channel-a".to_string()),
                    channel_name: Some("Channel A".to_string()),
                    interface_kind: Some("anthropic".to_string()),
                    public_model: Some("sonnet".to_string()),
                    upstream_model: Some("relay-sonnet".to_string()),
                }],
                ..ProxyRuntimeStatus::default()
            })
        })
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
    fn unix_timestamp(&self) -> i64 {
        *self.clock.lock().expect("clock mutex")
    }

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
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer gateway-token"),
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
        .expect("management auth mutex") =
        ManagementAuthRuntimeConfig::new("0.0.0.0", Some("management-token".to_string()), None);
    let engine = ProxyEngine::new(services);
    let mut headers = HeaderMap::new();

    let rejected =
        futures::executor::block_on(engine.validate_management_auth(&headers)).unwrap_err();
    assert!(
        matches!(rejected, ProxyCoreError::Auth(message) if message == "Missing management bearer token")
    );

    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer management-token"),
    );
    futures::executor::block_on(engine.validate_management_auth(&headers))
        .expect("management auth");
}

#[test]
fn external_host_can_use_custom_app_namespace_contracts_from_prelude() {
    let app = AppKind::from("opencode");
    let channel = channel_spec_from_input(ChannelSpecInput {
        id: "opencode-tools".to_string(),
        provider_id: "relay-tools".to_string(),
        app_type: app.as_str().to_string(),
        name: "OpenCode Tools".to_string(),
        status: "enabled".to_string(),
        base_url: "https://relay-tools.example/openai".to_string(),
        interface_kind: InterfaceKind::OpenAiChatCompletions.as_str().to_string(),
        models: vec![ModelRouteInput {
            public_model: "toolsmith".to_string(),
            upstream_model: "openrouter/toolsmith".to_string(),
            ..ModelRouteInput::default()
        }],
        groups: vec!["tools".to_string()],
        priority: 80,
        weight: 1,
        ..ChannelSpecInput::default()
    });
    let app_path = ManagementAppPathRequest::from_path(" opencode ").expect("app path");
    let model_request = AppModelCatalogRequest::from_parts(
        "opencode",
        AppModelListQuery::new(
            Some(InterfaceKind::OpenAiChatCompletions.as_str().to_string()),
            Some("tools".to_string()),
        ),
    )
    .expect("custom app model request");
    let channel_request =
        AppChannelManagementRequest::from_parts("opencode", AppChannelListQuery::list())
            .expect("custom app channel list request");
    let route_request = RouteResolveManagementRequest::from_body(RouteResolveRequest {
        app_type: "opencode".to_string(),
        requested_model: Some("toolsmith".to_string()),
        interface_kind: Some(InterfaceKind::OpenAiChatCompletions.as_str().to_string()),
        route_group: Some("tools".to_string()),
    })
    .expect("custom app route request");

    assert_eq!(app, AppKind::Custom("opencode".to_string()));
    assert_eq!(app.as_str(), "opencode");
    assert_eq!(channel.app, app);
    assert_eq!(
        channel.endpoint.base_url,
        "https://relay-tools.example/openai"
    );
    assert_eq!(channel.interface, InterfaceKind::OpenAiChatCompletions);
    assert_eq!(channel.models[0].public_model, "toolsmith");
    assert_eq!(app_path.app_type, "opencode");
    assert_eq!(model_request.app, AppKind::Custom("opencode".to_string()));
    assert_eq!(model_request.route_group.as_deref(), Some("tools"));
    assert!(matches!(
        channel_request.plan(),
        AppChannelManagementPlan::List { app_type } if app_type == "opencode"
    ));
    assert_eq!(route_request.request.app_type, "opencode");
    assert_eq!(
        route_request.request.interface_kind.as_deref(),
        Some("openai_chat_completions")
    );
}

#[test]
fn external_host_can_use_health_check_contracts_from_prelude() {
    let request = HealthCheckRequest::new();
    let response: HealthCheckResponse =
        request.response_from_source(HealthCheckSource::new("2026-06-26T00:00:00Z"));

    assert_eq!(response.status, "healthy");
    assert_eq!(response.timestamp, "2026-06-26T00:00:00Z");
}

#[test]
fn external_host_can_use_runtime_status_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let request = ProxyStatusRequest::new();

    let response: ProxyStatusResponse<ProxyRuntimeStatus> =
        futures::executor::block_on(engine.proxy_status_response(request))
            .expect("proxy status response");
    let status_source = ProxyStatusSource::new(response.status.clone());
    let passthrough: ProxyStatusResponse<ProxyRuntimeStatus> =
        ProxyStatusRequest::new().response_from_source(status_source);

    assert!(response.status.running);
    assert_eq!(response.status.address, "127.0.0.1");
    assert_eq!(response.status.port, 4100);
    assert_eq!(response.status.total_requests, 8);
    assert_eq!(response.status.success_rate, 75.0);
    assert_eq!(
        response.status.current_provider_id.as_deref(),
        Some("relay-a")
    );
    assert_eq!(response.status.active_targets.len(), 1);
    assert_eq!(
        response.status.active_targets[0].channel_id.as_deref(),
        Some("channel-a")
    );
    assert_eq!(passthrough.status, response.status);
}

#[test]
fn external_host_can_use_event_stream_contracts_from_prelude() {
    let payload = ProxyCoreEvent {
        event_type: ProxyCoreEventType::RouteSelected,
        request_id: Some("req-1".to_string()),
        channel_id: Some("channel-a".to_string()),
        payload: json!({ "appType": "claude" }),
    }
    .into_event_payload();
    let connected = ProxyEventEnvelope::new(
        1,
        PROXY_EVENTS_CONNECTED_EVENT,
        "2026-06-26T00:00:00Z",
        build_proxy_events_connected_payload(16),
    );
    let lagged = ProxyEventEnvelope::new(
        2,
        PROXY_EVENTS_LAGGED_EVENT,
        "2026-06-26T00:00:01Z",
        build_proxy_events_lagged_payload(3),
    );
    let connected_spec: ProxyEventSseSpec = connected.to_sse_spec();
    let lagged_spec: ProxyEventSseSpec = lagged.to_sse_spec();
    let connected_data: Value = from_str(&connected_spec.data).expect("connected data");
    let lagged_data: Value = from_str(&lagged_spec.data).expect("lagged data");

    assert_eq!(
        ProxyCoreEventType::RouteSelected.event_name(),
        "route_selected"
    );
    assert_eq!(
        ProxyCoreEventType::Custom("relay.updated".to_string()).event_name(),
        "relay.updated"
    );
    assert_eq!(payload["requestId"], "req-1");
    assert_eq!(payload["channelId"], "channel-a");
    assert_eq!(payload["appType"], "claude");

    assert_eq!(connected_spec.id, "1");
    assert_eq!(connected_spec.event, PROXY_EVENTS_CONNECTED_EVENT);
    assert_eq!(connected_data["id"], 1);
    assert_eq!(connected_data["event"], PROXY_EVENTS_CONNECTED_EVENT);
    assert_eq!(connected_data["payload"]["bufferSize"], 16);
    assert_eq!(lagged_spec.id, "2");
    assert_eq!(lagged_spec.event, PROXY_EVENTS_LAGGED_EVENT);
    assert_eq!(lagged_data["payload"]["skipped"], 3);
}

#[test]
fn external_host_can_use_app_list_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);

    let response: AppListResponse =
        futures::executor::block_on(engine.app_list_response(AppListRequest::new()))
            .expect("app list response");
    let apps: &[AppSummary] = response.apps.as_slice();
    let from_source: AppListResponse = AppListRequest::new().response_from_source(
        AppListSource::new(vec![AppSummaryInput::new("codex", true, true, 2, 3)]),
    );

    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].app_type, "claude");
    assert!(apps[0].enabled);
    assert!(!apps[0].auto_failover_enabled);
    assert_eq!(apps[0].provider_count, 1);
    assert_eq!(apps[0].channel_count, 1);
    assert_eq!(from_source.apps.len(), 1);
    assert_eq!(from_source.apps[0].app_type, "codex");
    assert!(from_source.apps[0].auto_failover_enabled);
    assert_eq!(from_source.apps[0].provider_count, 2);
    assert_eq!(from_source.apps[0].channel_count, 3);
}

#[test]
fn external_host_can_use_group_list_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let query = GroupListQuery::for_app("claude");
    let request = GroupListRequest::from_query(query).expect("group list request");

    let response: RouteGroupListResponse =
        futures::executor::block_on(engine.group_list_response(request))
            .expect("group list response");
    let groups: &[RouteGroupSummary] = response.groups.as_slice();
    let helper_request =
        GroupListRequest::from_query(GroupListQuery::all()).expect("helper request");
    let source_input: RouteGroupSourceInput = helper_request.source_input(
        "codex",
        &ChannelRouteSource::LegacyProjection,
        vec![Vec::new(), vec!["research".to_string()]],
    );
    let from_source: RouteGroupListResponse = helper_request.response(vec![source_input]);
    let from_channel_source: RouteGroupListResponse =
        GroupListRequest::from_query(GroupListQuery::all())
            .expect("source request")
            .response_from_channel_sources(vec![GroupListChannelSource::from_record_inputs(
                "gemini",
                ChannelRouteSource::MaterializedChannels,
                vec![GroupListChannelRecordInput::new(vec!["vision".to_string()])],
            )]);

    assert_eq!(response.app_type.as_deref(), Some("claude"));
    assert_eq!(response.sources.as_slice(), ["materialized_channels"]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].name, DEFAULT_ROUTE_GROUP);
    assert_eq!(groups[0].app_types.as_slice(), ["claude"]);
    assert_eq!(groups[0].channel_count, 1);
    assert_eq!(groups[1].name, "premium");
    assert_eq!(from_source.sources.as_slice(), ["legacy_projection"]);
    assert_eq!(from_source.groups[0].name, DEFAULT_ROUTE_GROUP);
    assert_eq!(from_source.groups[1].name, "research");
    assert_eq!(
        from_channel_source.groups[0].app_types.as_slice(),
        ["gemini"]
    );
    assert_eq!(from_channel_source.groups[0].name, "vision");
}

#[test]
fn external_host_can_use_app_channel_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let list_query = AppChannelListQuery::list();
    let list_request =
        AppChannelManagementRequest::from_parts("claude", list_query).expect("list request");
    match list_request.plan() {
        AppChannelManagementPlan::List { app_type } => assert_eq!(app_type, "claude"),
        AppChannelManagementPlan::Route(_) => panic!("expected list plan"),
    }

    let list_response: AppChannelResponse<
        ChannelRecord,
        ChannelRouteCandidate,
        ChannelRouteRejected,
    > = futures::executor::block_on(engine.app_channel_response(list_request))
        .expect("app channel list response");
    let helper_response: AppChannelResponse<
        ChannelRecord,
        ChannelRouteCandidate,
        ChannelRouteRejected,
    > = AppChannelManagementRequest::from_parts("codex", AppChannelListQuery::list())
        .expect("helper list request")
        .response_from_list_source(AppChannelListSource::new(
            ChannelRouteSource::LegacyProjection,
            vec![channel_record(vec!["research".to_string()])],
        ));

    match list_response {
        AppChannelResponse::List(list) => {
            let list: AppChannelListResponse<ChannelRecord> = list;
            assert_eq!(list.app_type, "claude");
            assert_eq!(list.source, "materialized_channels");
            assert_eq!(list.channels.len(), 1);
            assert_eq!(list.channels[0].groups.as_slice(), ["default", "premium"]);
        }
        AppChannelResponse::Route(_) => panic!("expected list response"),
    }
    match helper_response {
        AppChannelResponse::List(list) => {
            assert_eq!(list.app_type, "codex");
            assert_eq!(list.source, "legacy_projection");
            assert_eq!(list.channels[0].groups.as_slice(), ["research"]);
        }
        AppChannelResponse::Route(_) => panic!("expected helper list response"),
    }

    let route_query = AppChannelListQuery::route("sonnet", "anthropic", "premium");
    let route_request =
        AppChannelManagementRequest::from_parts("claude", route_query).expect("route request");
    match route_request.plan() {
        AppChannelManagementPlan::Route(route) => {
            assert_eq!(route.requested_model.as_deref(), Some("sonnet"));
            assert_eq!(route.interface_kind.as_deref(), Some("anthropic"));
            assert_eq!(route.route_group.as_deref(), Some("premium"));
        }
        AppChannelManagementPlan::List { .. } => panic!("expected route plan"),
    }

    let route_response: AppChannelResponse<
        ChannelRecord,
        ChannelRouteCandidate,
        ChannelRouteRejected,
    > = futures::executor::block_on(engine.app_channel_response(route_request))
        .expect("app channel route response");
    match route_response {
        AppChannelResponse::Route(route) => {
            let route: AppChannelRouteResponse<ChannelRouteCandidate, ChannelRouteRejected> = route;
            assert_eq!(route.app_type, "claude");
            assert_eq!(route.source, "materialized_channels");
            assert_eq!(route.route_group, "premium");
            assert_eq!(route.channels.len(), 1);
            assert_eq!(route.channels[0].channel_id, "channel-a");
            assert_eq!(route.rejected.len(), 1);
            assert_eq!(route.rejected[0].channel_id, "channel-b");
        }
        AppChannelResponse::List(_) => panic!("expected route response"),
    }
}

#[test]
fn external_host_can_use_channel_crud_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let list_request = ChannelListRequest::from_query(ChannelListQuery::for_app("claude"))
        .expect("channel list request");
    match list_request.plan() {
        ChannelListPlan::App { app_type } => assert_eq!(app_type, "claude"),
        ChannelListPlan::All => panic!("expected app-scoped list"),
    }

    let list_response: ChannelListResponse<ChannelRecord> =
        futures::executor::block_on(engine.channel_list_response(list_request))
            .expect("channel list response");
    let helper_list: ChannelListResponse<ChannelRecord> =
        ChannelListRequest::from_query(ChannelListQuery::all())
            .expect("all request")
            .response_from_source(ChannelListSource::new(vec![channel_record(vec![
                "research".to_string(),
            ])]));

    assert_eq!(list_response.channels.len(), 1);
    assert_eq!(list_response.channels[0].id, "channel-a");
    assert_eq!(helper_list.channels[0].groups.as_slice(), ["research"]);

    let create_body = ProxyChannelWriteRequest {
        id: Some("channel-new".to_string()),
        provider_id: "relay-a".to_string(),
        app_type: "claude".to_string(),
        name: "New Channel".to_string(),
        base_url: "https://new.example/v1".to_string(),
        interface_kind: "anthropic".to_string(),
        groups: vec!["premium".to_string()],
        priority: 10,
        weight: 3,
        ..ProxyChannelWriteRequest::default()
    };
    let create_response: ChannelRecordResponse<ChannelRecord> = futures::executor::block_on(
        engine.create_channel_response(ChannelCreateRequest::from_body(create_body)),
    )
    .expect("create channel response");
    let helper_create: ChannelRecordResponse<ChannelRecord> =
        ChannelCreateRequest::from_body(ProxyChannelWriteRequest::default())
            .record_response_from_source(ChannelCreateSource::new(channel_record(vec![
                "helper".to_string()
            ])));

    assert_eq!(create_response.channel.id, "channel-new");
    assert_eq!(create_response.channel.name, "New Channel");
    assert_eq!(create_response.channel.groups.as_slice(), ["premium"]);
    assert_eq!(helper_create.channel.groups.as_slice(), ["helper"]);

    let path = ChannelPathRequest::from_path(" channel-a ").expect("channel path");
    let record_response: ChannelRecordResponse<ChannelRecord> =
        futures::executor::block_on(engine.channel_record_response(path.clone()))
            .expect("channel record response");
    let helper_record: ChannelRecordResponse<ChannelRecord> = path
        .record_response_from_source(ChannelRecordSource::new(Some(channel_record(vec![
            "source".to_string(),
        ]))))
        .expect("helper record response");

    assert_eq!(record_response.channel.id, "channel-a");
    assert_eq!(
        record_response.channel.groups.as_slice(),
        ["default", "premium"]
    );
    assert_eq!(helper_record.channel.groups.as_slice(), ["source"]);

    let patch = ProxyChannelPatchRequest {
        name: Some("Renamed Channel".to_string()),
        groups: Some(vec!["gold".to_string()]),
        priority: Some(42),
        weight: Some(5),
        ..ProxyChannelPatchRequest::default()
    };
    let update_response: ChannelRecordResponse<ChannelRecord> =
        futures::executor::block_on(engine.update_channel_response(path.clone(), patch))
            .expect("update channel response");

    assert_eq!(update_response.channel.name, "Renamed Channel");
    assert_eq!(update_response.channel.groups.as_slice(), ["gold"]);
    assert_eq!(update_response.channel.priority, 42);
    assert_eq!(update_response.channel.weight, 5);

    let delete_response: ChannelDeleteResponse =
        futures::executor::block_on(engine.delete_channel_response(path.clone()))
            .expect("delete channel response");
    let helper_delete: ChannelDeleteResponse =
        path.delete_response_from_source(ChannelDeleteSource::new(false));

    assert_eq!(delete_response.channel_id, "channel-a");
    assert!(delete_response.deleted);
    assert_eq!(helper_delete.channel_id, "channel-a");
    assert!(!helper_delete.deleted);
}

#[test]
fn external_host_can_use_channel_key_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let channel_path = ChannelPathRequest::from_path("channel-a").expect("channel path");
    let key_path = ChannelKeyPathRequest::from_path("channel-a", "primary").expect("key path");

    let keys_response: ChannelKeysResponse<ChannelKeyRecord> =
        futures::executor::block_on(engine.channel_keys_response(channel_path.clone()))
            .expect("channel keys response");
    let helper_keys: ChannelKeysResponse<ChannelKeyRecord> = channel_path
        .keys_response_from_source(ChannelKeysSource::new(Some(vec![channel_key_record(
            "helper", "enabled", 3, 2,
        )])))
        .expect("helper keys response");

    let upsert_response: ChannelKeyRecordResponse<ChannelKeyRecord> =
        futures::executor::block_on(engine.upsert_channel_key_response(
            key_path.clone(),
            ProxyChannelKeyWriteRequest {
                key_value: "sk-relay".to_string(),
                status: "enabled".to_string(),
                priority: 20,
                weight: 4,
            },
        ))
        .expect("upsert key response");
    let update_response: ChannelKeyRecordResponse<ChannelKeyRecord> =
        futures::executor::block_on(engine.update_channel_key_response(
            key_path.clone(),
            ProxyChannelKeyPatchRequest {
                key_value: Some("sk-new".to_string()),
                status: Some("disabled".to_string()),
                priority: Some(30),
                weight: Some(5),
            },
        ))
        .expect("update key response");
    let helper_record: ChannelKeyRecordResponse<ChannelKeyRecord> = key_path
        .record_response_from_source(ChannelKeyRecordSource::new(Some(channel_key_record(
            "source", "enabled", 7, 1,
        ))))
        .expect("helper key record response");

    let delete_response: ChannelKeyDeleteResponse =
        futures::executor::block_on(engine.delete_channel_key_response(key_path.clone()))
            .expect("delete key response");
    let helper_delete: ChannelKeyDeleteResponse =
        key_path.delete_response_from_source(ChannelKeyDeleteSource::new(false));

    assert_eq!(keys_response.channel_id, "channel-a");
    assert_eq!(keys_response.keys.len(), 1);
    assert_eq!(keys_response.keys[0].key_ref, "primary");
    assert_eq!(keys_response.keys[0].status, "enabled");
    assert_eq!(helper_keys.keys[0].key_ref, "helper");
    assert_eq!(helper_keys.keys[0].weight, 2);

    assert_eq!(upsert_response.key.key_ref, "primary");
    assert_eq!(upsert_response.key.priority, 20);
    assert_eq!(upsert_response.key.weight, 4);
    assert_eq!(update_response.key.status, "disabled");
    assert_eq!(update_response.key.priority, 30);
    assert_eq!(update_response.key.weight, 5);
    assert_eq!(helper_record.key.key_ref, "source");
    assert_eq!(helper_record.key.priority, 7);

    assert_eq!(delete_response.channel_id, "channel-a");
    assert_eq!(delete_response.key_ref, "primary");
    assert!(delete_response.deleted);
    assert_eq!(helper_delete.key_ref, "primary");
    assert!(!helper_delete.deleted);
}

#[test]
fn external_host_can_use_channel_model_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let path = ChannelPathRequest::from_path("channel-a").expect("channel path");

    let list_response: ChannelModelsResponse<ChannelModelRecord> =
        futures::executor::block_on(engine.channel_models_response(path.clone()))
            .expect("channel models response");
    let replace_response: ChannelModelsResponse<ChannelModelRecord> =
        futures::executor::block_on(engine.replace_channel_models_response(
            path.clone(),
            ProxyChannelModelsReplaceRequest {
                models: vec![ProxyChannelModelWriteRequest {
                    public_model: "haiku".to_string(),
                    upstream_model: "relay-haiku".to_string(),
                    capabilities: json!({ "contextWindow": 200000 }),
                    pricing_model: Some("relay-fast".to_string()),
                    request_overrides: json!({ "temperature": 0.2 }),
                    response_overrides: json!({ "stream": true }),
                }],
            },
        ))
        .expect("replace channel models response");
    let helper_response: ChannelModelsResponse<ChannelModelRecord> =
        path.models_response_from_source(ChannelModelsSource::new(Some(vec![
            channel_model_record("source-model", "relay-source-model"),
        ])))
        .expect("helper channel models response");

    assert_eq!(list_response.channel_id, "channel-a");
    assert_eq!(list_response.models.len(), 1);
    assert_eq!(list_response.models[0].public_model, "sonnet");
    assert_eq!(list_response.models[0].upstream_model, "relay-sonnet");

    assert_eq!(replace_response.channel_id, "channel-a");
    assert_eq!(replace_response.models.len(), 1);
    assert_eq!(replace_response.models[0].public_model, "haiku");
    assert_eq!(replace_response.models[0].upstream_model, "relay-haiku");
    assert_eq!(
        replace_response.models[0].pricing_model.as_deref(),
        Some("relay-fast")
    );
    assert_eq!(
        replace_response.models[0].capabilities,
        json!({ "contextWindow": 200000 })
    );

    assert_eq!(helper_response.channel_id, "channel-a");
    assert_eq!(helper_response.models[0].public_model, "source-model");
    assert_eq!(
        helper_response.models[0].upstream_model,
        "relay-source-model"
    );
}

#[test]
fn external_host_can_use_channel_health_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let path = ChannelPathRequest::from_path("channel-a").expect("channel path");

    let reset_response: ChannelHealthResetResponse =
        futures::executor::block_on(engine.reset_channel_health_response(path.clone()))
            .expect("channel health reset response");
    let stats_response: ChannelBreakerStatsResponse =
        futures::executor::block_on(engine.channel_breaker_stats_response(path.clone()))
            .expect("channel breaker stats response");
    let helper_reset = path.health_reset_response_from_source(ChannelHealthResetSource::new(
        ChannelHealthResetResponse::from_reset(ChannelHealthReset {
            channel_id: "channel-helper".to_string(),
            app: AppKind::Codex,
        }),
    ));
    let helper_stats = path.breaker_stats_response_from_source(ChannelBreakerStatsSource::new(
        ChannelBreakerStatsResponse::from_stats(ChannelBreakerStats {
            channel_id: "channel-helper".to_string(),
            app: AppKind::Codex,
            stats: None,
        }),
    ));

    assert_eq!(reset_response.channel_id, "channel-a");
    assert_eq!(reset_response.app_type, "claude");
    assert!(reset_response.reset);
    assert_eq!(stats_response.channel_id, "channel-a");
    assert_eq!(stats_response.app_type, "claude");
    assert!(stats_response.stats.is_none());

    assert_eq!(helper_reset.channel_id, "channel-helper");
    assert_eq!(helper_reset.app_type, "codex");
    assert!(helper_reset.reset);
    assert_eq!(helper_stats.channel_id, "channel-helper");
    assert_eq!(helper_stats.app_type, "codex");
    assert!(helper_stats.stats.is_none());
}

#[test]
fn external_host_can_use_channel_test_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    *services.clock.lock().expect("clock mutex") = 1_771_000_000;
    let engine = ProxyEngine::new(services);
    let path = ChannelPathRequest::from_path("channel-a").expect("channel path");

    let response: ChannelTestResponse = futures::executor::block_on(engine.channel_test_response(
        path.clone(),
        ProxyChannelTestRequest {
            model: Some("sonnet".to_string()),
            interface_kind: Some("anthropic_messages".to_string()),
        },
    ))
    .expect("channel test response");
    let missing_model: ChannelTestResponse =
        futures::executor::block_on(engine.channel_test_response(
            path.clone(),
            ProxyChannelTestRequest {
                model: Some("opus".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
            },
        ))
        .expect("missing model response");
    let helper_response = path.test_response(ChannelTestInput {
        channel_id: "channel-helper".to_string(),
        provider_id: "relay-helper".to_string(),
        app_type: "codex".to_string(),
        channel_name: "Helper Channel".to_string(),
        base_url: "https://helper.example/v1".to_string(),
        interface_kind: "openai_responses".to_string(),
        model: Some("gpt-5.4".to_string()),
        model_available: Some(true),
        success: false,
        status: ChannelReachabilityStatus::Failed.as_str().to_string(),
        message: "timeout".to_string(),
        latency_ms: None,
        http_status: None,
        tested_at: 1_771_000_999,
        retry_count: 2,
        failure_reason: Some("timeout".to_string()),
    });
    let plan: ChannelTestPlan = ChannelTestPlan::Failure(helper_response.clone());

    assert_eq!(response.channel_id, "channel-a");
    assert_eq!(response.provider_id, "relay-a");
    assert_eq!(response.app_type, "claude");
    assert_eq!(response.interface_kind, "anthropic_messages");
    assert_eq!(response.model.as_deref(), Some("sonnet"));
    assert_eq!(response.model_available, Some(true));
    assert!(response.success);
    assert_eq!(
        response.status,
        ChannelReachabilityStatus::Degraded.as_str()
    );
    assert_eq!(response.message, "reachable: https://relay.example/v1");
    assert_eq!(response.latency_ms, Some(6100));
    assert_eq!(response.http_status, Some(200));
    assert_eq!(response.retry_count, 1);

    assert_eq!(missing_model.model.as_deref(), Some("opus"));
    assert_eq!(missing_model.model_available, Some(false));
    assert!(!missing_model.success);
    assert_eq!(
        missing_model.status,
        ChannelReachabilityStatus::Failed.as_str()
    );
    assert_eq!(
        missing_model.failure_reason.as_deref(),
        Some("model not mapped on channel: opus")
    );

    match plan {
        ChannelTestPlan::Failure(failure) => {
            assert_eq!(failure.channel_id, "channel-helper");
            assert_eq!(failure.app_type, "codex");
            assert_eq!(failure.retry_count, 2);
            assert_eq!(failure.failure_reason.as_deref(), Some("timeout"));
        }
        ChannelTestPlan::Probe(_) => panic!("expected failure plan"),
    }
}

#[test]
fn external_host_can_use_channel_migration_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let request = ManagementAppPathRequest::from_path("claude").expect("migration request");

    let preview: ChannelMigrationPreviewResponse<ChannelRecord> =
        futures::executor::block_on(engine.channel_migration_preview_response(request.clone()))
            .expect("migration preview response");
    let materialize: ChannelMigrationMaterializeResponse =
        futures::executor::block_on(engine.channel_migration_materialize_response(request))
            .expect("migration materialize response");
    let helper_request =
        ManagementAppPathRequest::from_path("codex").expect("helper migration request");
    let helper_preview: ChannelMigrationPreviewResponse<ChannelRecord> = helper_request
        .migration_preview_response_from_source(ChannelMigrationPreviewSource::from_input(
            ChannelMigrationPreviewInput::new(
                "ignored-app",
                vec![channel_record(vec!["helper".to_string()])],
                4,
                2,
            ),
        ));
    let helper_materialize: ChannelMigrationMaterializeResponse = helper_request
        .migration_materialize_response_from_source(ChannelMigrationMaterializeSource::from_input(
            ChannelMigrationMaterializeInput::new("ignored-app", 5, 3, 7, 3, 2, 1),
        ));

    assert_eq!(preview.app_type, "claude");
    assert_eq!(preview.channels.len(), 1);
    assert_eq!(preview.channels[0].id, "channel-a");
    assert_eq!(preview.duplicate_count, 2);
    assert_eq!(preview.needs_review_count, 1);
    assert_eq!(materialize.app_type, "claude");
    assert_eq!(materialize.previewed_channels, 3);
    assert_eq!(materialize.inserted_channels, 2);
    assert_eq!(materialize.inserted_models, 4);
    assert_eq!(materialize.inserted_health_rows, 2);
    assert_eq!(materialize.duplicate_count, 1);
    assert_eq!(materialize.needs_review_count, 1);

    assert_eq!(helper_preview.app_type, "codex");
    assert_eq!(helper_preview.channels[0].groups.as_slice(), ["helper"]);
    assert_eq!(helper_preview.duplicate_count, 4);
    assert_eq!(helper_preview.needs_review_count, 2);
    assert_eq!(helper_materialize.app_type, "codex");
    assert_eq!(helper_materialize.previewed_channels, 5);
    assert_eq!(helper_materialize.inserted_channels, 3);
    assert_eq!(helper_materialize.inserted_models, 7);
    assert_eq!(helper_materialize.inserted_health_rows, 3);
    assert_eq!(helper_materialize.duplicate_count, 2);
    assert_eq!(helper_materialize.needs_review_count, 1);
}

#[test]
fn external_host_can_use_app_model_catalog_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);
    let query = AppModelListQuery::new(
        Some("anthropic".to_string()),
        Some(DEFAULT_ROUTE_GROUP.to_string()),
    );
    let request =
        AppModelCatalogRequest::from_parts("claude", query).expect("model catalog request");

    let catalog: RoutableModelList =
        futures::executor::block_on(engine.list_model_catalog_for_request(request))
            .expect("model catalog");
    let helper_request = AppModelCatalogRequest::from_parts(
        "codex",
        AppModelListQuery::new(None, Some("research".to_string())),
    )
    .expect("helper request");
    let from_source: RoutableModelList =
        helper_request.response_from_source(AppModelCatalogSource::new(vec![RoutableModel {
            public_model: "gpt-5.4".to_string(),
            upstream_model: "gpt-5.4".to_string(),
            pricing_model: None,
            app: AppKind::Codex,
            provider_id: "relay-b".to_string(),
            provider_name: "Relay B".to_string(),
            channel_id: "channel-b".to_string(),
            channel_name: "Channel B".to_string(),
            interface: InterfaceKind::OpenAiResponses,
            groups: vec!["research".to_string()],
            priority: 50,
            weight: 2,
            capabilities: ModelCapabilities::default(),
        }]));

    assert_eq!(catalog.app_type, "claude");
    assert_eq!(catalog.route_group.as_deref(), Some("default"));
    assert_eq!(
        catalog.interface_kind.as_deref(),
        Some("anthropic_messages")
    );
    assert_eq!(catalog.models.len(), 1);
    assert_eq!(catalog.models[0].public_model, "sonnet");
    assert_eq!(catalog.models[0].upstream_model, "relay-sonnet");
    assert_eq!(catalog.models[0].provider_id, "relay-a");
    assert_eq!(catalog.models[0].channel_id, "channel-a");
    assert_eq!(from_source.app_type, "codex");
    assert_eq!(from_source.route_group.as_deref(), Some("research"));
    assert_eq!(
        from_source.models[0].interface,
        InterfaceKind::OpenAiResponses
    );
    assert_eq!(from_source.models[0].groups.as_slice(), ["research"]);

    let client_catalog = client_model_catalog_from_routable_models(
        "codex-route",
        &from_source.models,
        256_000,
        &json!({
            "slug": "template",
            "display_name": "Template",
            "model_messages": { "instructions_template": "template" }
        }),
    );
    let client_raw_models = client_catalog
        .raw
        .get("models")
        .and_then(Value::as_array)
        .expect("client models");

    assert_eq!(client_catalog.provider_id, "codex-route");
    assert_eq!(client_catalog.models, vec!["gpt-5.4".to_string()]);
    assert_eq!(
        client_raw_models[0].get("slug").and_then(Value::as_str),
        Some("gpt-5.4")
    );
    assert_eq!(
        client_raw_models[0]
            .get("context_window")
            .and_then(Value::as_u64),
        Some(256_000)
    );
}

#[test]
fn external_host_can_use_provider_list_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);

    let response: ProviderListResponse =
        futures::executor::block_on(engine.provider_list_response(
            ManagementAppPathRequest::from_path("claude").expect("provider list request"),
        ))
        .expect("provider list response");
    let providers: &[ProviderSummary] = response.providers.as_slice();
    let from_source = ManagementAppPathRequest::from_path("codex")
        .expect("source request")
        .provider_list_response_from_source(ProviderListSource::new(
            vec![ProviderSummaryInput::new(
                "relay-b",
                "Relay B",
                Some("custom".to_string()),
                Some(2),
                None,
                None,
                Some("openai-compatible".to_string()),
            )],
            Some("relay-b".to_string()),
            vec!["relay-b".to_string()],
            vec!["relay-b".to_string()],
        ));

    assert_eq!(response.app_type, "claude");
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].id, "relay-a");
    assert!(providers[0].current);
    assert!(!providers[0].in_failover_queue);
    assert!(providers[0].route_candidate);
    assert_eq!(from_source.app_type, "codex");
    assert_eq!(from_source.providers.len(), 1);
    assert_eq!(from_source.providers[0].category.as_deref(), Some("custom"));
    assert!(from_source.providers[0].current);
    assert!(from_source.providers[0].in_failover_queue);
    assert!(from_source.providers[0].route_candidate);
}

#[test]
fn external_host_can_use_current_route_contracts_from_prelude() {
    let services = Arc::new(ExternalRelayServices::default());
    let engine = ProxyEngine::new(services);

    let response: CurrentRouteResponse<CurrentRouteTarget> =
        futures::executor::block_on(engine.current_route_response(
            ManagementAppPathRequest::from_path("claude").expect("current route request"),
        ))
        .expect("current route response");
    let configured: &CurrentRouteProviderSummary = response
        .configured_provider
        .as_ref()
        .expect("configured provider");
    let target = response.target.as_ref().expect("active target");
    let from_source: CurrentRouteResponse<CurrentRouteTarget> =
        ManagementAppPathRequest::from_path("codex")
            .expect("source request")
            .current_route_response_from_source(CurrentRouteSource::new(
                None,
                Some(CurrentRouteProviderSummaryInput::new(
                    "relay-c",
                    "Relay C",
                    Some("official".to_string()),
                )),
            ));

    assert_eq!(response.app_type, "claude");
    assert!(response.active);
    assert_eq!(target.channel_id.as_deref(), Some("channel-a"));
    assert_eq!(target.upstream_model.as_deref(), Some("relay-sonnet"));
    assert_eq!(configured.id, "relay-a");
    assert_eq!(configured.name, "Relay A");
    assert_eq!(from_source.app_type, "codex");
    assert!(!from_source.active);
    assert!(from_source.target.is_none());
    assert_eq!(
        from_source.configured_provider.as_ref().map(|provider| {
            (
                provider.id.as_str(),
                provider.name.as_str(),
                provider.category.as_deref(),
            )
        }),
        Some(("relay-c", "Relay C", Some("official")))
    );
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
    assert_eq!(
        candidates[0].upstream_model.as_deref(),
        Some("relay-sonnet")
    );
    assert_eq!(candidates[0].route_group, "premium");
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].channel_id, "channel-b");
    assert_eq!(rejected[0].reasons.as_slice(), ["interface_mismatch"]);
    assert_eq!(source, ChannelRouteSource::MaterializedChannels);
    assert_eq!(source.as_str(), "materialized_channels");
}

#[test]
fn external_host_can_use_claude_transform_contracts_from_prelude() {
    let request = claude_request_transform_for_api_format(
        json!({
            "model": "claude-sonnet",
            "stream": true,
            "messages": [{"role": "user", "content": "hello"}]
        }),
        "openai_chat",
        ClaudeApiFormatRequestTransformContext {
            provider_id: "relay-a",
            responses_prompt_cache_key: None,
            responses_prompt_cache_key_source: ClaudePromptCacheKeySource::None,
            chat_prompt_cache_key: Some("cache-a"),
            is_codex_oauth: false,
            codex_fast_mode_enabled: false,
            preserve_reasoning_content: false,
            shadow_store: None,
            session_id: None,
        },
    )
    .expect("request transform");
    let request_output: ClaudeApiFormatRequestTransformOutput = request;
    assert_eq!(request_output.request["prompt_cache_key"], "cache-a");
    assert_eq!(
        request_output.request["stream_options"]["include_usage"],
        true
    );

    let response: ClaudeApiFormatResponseTransformOutput =
        claude_response_to_anthropic_message_for_api_format(
            &json!({
                "id": "chatcmpl_1",
                "model": "relay-chat",
                "choices": [{
                    "message": {"role": "assistant", "content": "hi"},
                    "finish_reason": "stop"
                }]
            }),
            "openai_chat",
            None,
            None,
            None,
            None,
            || "toolu_unused".to_string(),
        )
        .expect("response transform");
    assert_eq!(response.response["content"][0]["text"], "hi");

    let mut body = json!({
        "thinking": {"type": "disabled"},
        "output_config": {"effort": "max"},
        "messages": []
    });
    assert!(normalize_claude_anthropic_messages(
        &mut body,
        &json!({"base_url": "https://api.deepseek.com/anthropic"}),
        "anthropic"
    ));
    assert!(body.get("output_config").is_none());

    let _hints: Option<AnthropicToolSchemaHints> = None;
    let _shadow = GeminiShadowStore::default();
    let _stream_context = ClaudeApiFormatSseTransformContext {
        shadow_store: None,
        provider_id: None,
        session_id: None,
        tool_schema_hints: None,
        synthesize_gemini_tool_call_id: || "toolu_stream".to_string(),
        on_rectified_tool_name: |_name: &str| {},
    };
    let _chunk = Bytes::from_static(b"data: [DONE]\n\n");
}
