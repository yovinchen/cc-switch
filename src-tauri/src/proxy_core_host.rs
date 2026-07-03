#[cfg(test)]
use crate::database::Database;
#[cfg(test)]
use crate::error::AppError;
#[cfg(test)]
use crate::proxy::codex_chat_history::CodexChatHistoryStore;
#[cfg(test)]
use crate::proxy::events::ProxyEventBus;
#[cfg(test)]
use crate::proxy::host::cc_switch::auth_provider::CcSwitchAuthProvider;
#[cfg(test)]
use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
#[cfg(test)]
use crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime;
#[cfg(test)]
use crate::proxy::host::cc_switch::route_resolver::management_route_response_from_router_source;
#[cfg(test)]
use crate::proxy_core::api::domain::{
    AppKind, ChannelOverrides, ModelCapabilities, ModelRoute, ProviderKind, ProviderSpec,
    RetryPolicy, UpstreamEndpoint,
};
#[cfg(test)]
use crate::proxy_core::api::engine::ProxyEngine;
#[cfg(test)]
use crate::proxy_core::api::errors::ProxyCoreError;
#[cfg(test)]
use crate::proxy_core::api::ports::{AuthProvider, ProxyServices};
#[cfg(test)]
use crate::proxy_core::api::ports::{ProxyConfig, ProxyRuntimeStatus};
#[cfg(test)]
use crate::proxy_core::api::routing::{
    ChannelQuery, ChannelSpec, ChannelStatus, InterfaceKind, RoutePlan, RouteRequest,
    RouteSelection, DEFAULT_ROUTE_GROUP,
};
#[cfg(test)]
use crate::proxy_core::api::transforms::GeminiShadowStore;
#[cfg(test)]
use crate::proxy_core::api::transport::{ProxyBody, ProxyRequest};
#[cfg(test)]
use crate::proxy_core::api::usage::UsageRecord;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::RwLock;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::proxy_core::api::management::{
        ProxyChannelModelWriteRequest, ProxyChannelWriteRequest, RouteResolveRequest,
    };
    use http::Method;
    use serde_json::json;

    type CcSwitchProxyServices =
        crate::proxy::host::cc_switch::proxy_services::CcSwitchProxyServices<
            crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime,
        >;

    fn auth_profile_ref<T: serde::de::DeserializeOwned>(value: &str) -> T {
        serde_json::from_value(json!(value)).expect("auth profile ref")
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

    fn create_materialized_channel(
        db: &Database,
        id: &str,
        priority: i64,
        weight: u32,
        upstream_model: &str,
    ) {
        db.create_proxy_channel(ProxyChannelWriteRequest {
            id: Some(id.to_string()),
            provider_id: "anthropic-main".to_string(),
            app_type: "claude".to_string(),
            name: id.to_string(),
            base_url: format!("https://{id}.example.com/v1"),
            interface_kind: "openai_responses".to_string(),
            priority,
            weight,
            models: vec![ProxyChannelModelWriteRequest {
                public_model: "sonnet-public".to_string(),
                upstream_model: upstream_model.to_string(),
                ..ProxyChannelModelWriteRequest::default()
            }],
            ..ProxyChannelWriteRequest::default()
        })
        .expect("create materialized channel");
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
        let status = Arc::new(RwLock::new(ProxyRuntimeStatus::default()));
        let current_providers = Arc::new(RwLock::new(std::collections::HashMap::new()));
        let gemini_shadow = Arc::new(GeminiShadowStore::default());
        let codex_chat_history = Arc::new(CodexChatHistoryStore::default());
        let provider_router = Arc::new(provider_router_from_database(db.clone()));
        CcSwitchProxyRuntime {
            db: db.clone(),
            config: Arc::new(RwLock::new(ProxyConfig::default())),
            provider_router: provider_router.clone(),
            status: status.clone(),
            start_time: Arc::new(RwLock::new(None)),
            events: events.clone(),
            current_providers: current_providers.clone(),
            attempt_runtime_source:
                crate::proxy::host::cc_switch::forwarder_attempt_runtime_source::forwarder_attempt_runtime_source_from_runtime_sources(
                    provider_router,
                    db.clone(),
                ),
            protocol_state_source:
                crate::proxy::host::cc_switch::forwarder_protocol_state_source::forwarder_protocol_state_source_from_runtime_parts(
                    gemini_shadow,
                    codex_chat_history,
                ),
            runtime_state_source:
                crate::proxy::host::cc_switch::forwarder_runtime_state_source::forwarder_runtime_state_source_from_runtime_parts(
                    status,
                    current_providers,
                    events,
                ),
            auth_source:
                crate::proxy::host::cc_switch::forwarder_auth_source::default_forwarder_auth_source(
                ),
            request_source:
                crate::proxy::host::cc_switch::forwarder_request_source::default_forwarder_request_source(
                ),
            transport_source:
                crate::proxy::host::cc_switch::forwarder_transport_source::default_forwarder_transport_source(
                ),
            response_source:
                crate::proxy::host::cc_switch::forwarder_response_source::default_forwarder_response_source(
                ),
            failover_switch_scheduler:
                crate::proxy::host::cc_switch::failover_switch::noop_failover_switch_scheduler(),
        }
    }

    #[tokio::test]
    async fn auth_provider_projects_profile_ref_through_core() {
        let provider = CcSwitchAuthProvider;
        let request = proxy_request();
        let mut plan = route_plan("anthropic-main", "channel-auth");
        plan.selection.channel.auth_profile =
            Some(auth_profile_ref("provider:claude:anthropic-main"));

        let auth = provider
            .resolve_auth(
                &request.app,
                &plan.selection.provider,
                &plan.selection.channel,
                &request,
            )
            .await
            .expect("resolve auth");

        assert!(auth.headers.is_empty());
        assert_eq!(
            auth.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(auth.metadata["source"], json!("cc_switch_provider_config"));
        assert_eq!(auth.metadata["app"], json!("claude"));
        assert_eq!(auth.metadata["providerId"], json!("anthropic-main"));
        assert_eq!(auth.metadata["channelId"], json!("channel-auth"));
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
                tokens: serde_json::from_value(json!({
                    "inputTokens": 1_000,
                    "outputTokens": 500,
                    "cacheReadTokens": 0,
                    "cacheCreationTokens": 0,
                }))
                .expect("usage tokens"),
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

        type UsageRow = (
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
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            String,
        );

        let conn = crate::database::lock_conn!(db.conn);
        let row: UsageRow = conn
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
                    channel_id,
                    channel_name,
                    route_group,
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
                        row.get(14)?,
                        row.get(15)?,
                        row.get(16)?,
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
        assert_eq!(row.12.as_deref(), Some("channel-a"));
        assert_eq!(row.13.as_deref(), Some("Channel A"));
        assert_eq!(row.14.as_deref(), Some(DEFAULT_ROUTE_GROUP));
        assert_eq!(row.15, 1);
        assert_ne!(row.16, "0");
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

    #[tokio::test]
    async fn route_dry_run_matches_proxy_engine_materialized_plan_order() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        create_materialized_channel(&db, "channel-low", 10, 100, "upstream-low");
        create_materialized_channel(&db, "channel-high", 100, 20, "upstream-high");

        let router = provider_router_from_database(db.clone());
        let dry_run = management_route_response_from_router_source(
            &router,
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("sonnet-public".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
        )
        .await
        .expect("dry-run route");

        let services = Arc::new(CcSwitchProxyServices::new(db));
        let engine = ProxyEngine::new(services);
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({ "model": "sonnet-public", "messages": [] })),
        );
        request.requested_model = Some("sonnet-public".to_string());

        let plan = engine
            .plan_materialized_route(&request)
            .await
            .expect("materialized plan");
        let plan_channel_ids = plan
            .selections
            .iter()
            .map(|selection| selection.channel.id.as_str())
            .collect::<Vec<_>>();
        let dry_run_channel_ids = dry_run
            .candidates
            .iter()
            .map(|candidate| candidate.channel_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(dry_run_channel_ids, vec!["channel-high", "channel-low"]);
        assert_eq!(plan_channel_ids, dry_run_channel_ids);
        assert_eq!(plan.selection.channel.id, "channel-high");
        assert_eq!(
            plan.selection
                .model_route
                .as_ref()
                .map(|route| route.upstream_model.as_str()),
            Some("upstream-high")
        );
    }
}
