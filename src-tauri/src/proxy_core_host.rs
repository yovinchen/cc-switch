#[cfg(test)]
use crate::database::Database;
#[cfg(test)]
use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
#[cfg(test)]
use crate::proxy::host::cc_switch::route_resolver::management_route_response_from_router_source;
#[cfg(test)]
use crate::proxy_core::api::domain::AppKind;
#[cfg(test)]
use crate::proxy_core::api::engine::ProxyEngine;
#[cfg(test)]
use crate::proxy_core::api::errors::ProxyCoreError;
#[cfg(test)]
use crate::proxy_core::api::routing::InterfaceKind;
#[cfg(test)]
use crate::proxy_core::api::transport::{ProxyBody, ProxyRequest};
#[cfg(test)]
use std::sync::Arc;

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
