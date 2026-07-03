#[cfg(test)]
use crate::database::Database;
#[cfg(test)]
use crate::proxy::codex_chat_history::CodexChatHistoryStore;
#[cfg(test)]
use crate::proxy::events::ProxyEventBus;
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
use crate::proxy_core::api::ports::ProxyServices;
#[cfg(test)]
use crate::proxy_core::api::ports::{ProxyConfig, ProxyRuntimeStatus};
#[cfg(test)]
use crate::proxy_core::api::routing::{
    ChannelSpec, ChannelStatus, InterfaceKind, RoutePlan, RouteSelection, DEFAULT_ROUTE_GROUP,
};
#[cfg(test)]
use crate::proxy_core::api::transforms::GeminiShadowStore;
#[cfg(test)]
use crate::proxy_core::api::transport::{ProxyBody, ProxyRequest};
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
