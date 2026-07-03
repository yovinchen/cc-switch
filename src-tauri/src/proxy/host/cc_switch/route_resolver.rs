//! CC Switch route resolver source.

use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::errors::{ProxyCoreError, ProxyCoreResult};
use crate::proxy_core::api::management::{RouteResolveRequest, RouteResolveResponse};
use crate::proxy_core::api::ports::RouteResolver;
use crate::proxy_core::api::routing::{
    apply_route_candidate_circuit_availability, build_route_plan_with_weighted_roll,
    resolve_channel_route, route_candidate_channel_circuit_keys, RoutePlan, RouteRequest,
};
use futures::future::BoxFuture;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct CcSwitchRouteResolver {
    router: Arc<ProviderRouter>,
}

impl CcSwitchRouteResolver {
    pub(crate) fn new(router: Arc<ProviderRouter>) -> Self {
        Self { router }
    }
}

impl RouteResolver for CcSwitchRouteResolver {
    fn resolve<'a>(
        &'a self,
        request: RouteRequest<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<RoutePlan>> {
        Box::pin(
            async move { build_route_plan_with_weighted_roll(request, route_plan_weighted_roll()) },
        )
    }

    fn resolve_management_route<'a>(
        &'a self,
        request: RouteResolveRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<RouteResolveResponse>> {
        Box::pin(async move {
            management_route_response_from_router_source(&self.router, request).await
        })
    }
}

pub(crate) async fn management_route_response_from_router_source(
    router: &ProviderRouter,
    request: RouteResolveRequest,
) -> ProxyCoreResult<RouteResolveResponse> {
    let (channels, source) = router
        .list_route_channel_inputs_for_app(&request.app_type)
        .await
        .map_err(|error| ProxyCoreError::Config(format!("list channel route inputs: {error}")))?;
    let mut response = resolve_channel_route(request, channels, source)?;
    let availability = router
        .route_candidate_circuit_availability(route_candidate_channel_circuit_keys(&response))
        .await;
    apply_route_candidate_circuit_availability(&mut response, availability);
    Ok(response)
}

fn route_plan_weighted_roll() -> u64 {
    let now = chrono::Utc::now();
    now.timestamp_nanos_opt()
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_else(|| u64::try_from(now.timestamp_millis()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
    use crate::proxy_core::api::domain::{
        AppKind, ChannelOverrides, ModelCapabilities, ModelRoute, ProviderKind, ProviderSpec,
        ProxyBody, ProxyRequest, RetryPolicy, UpstreamEndpoint,
    };
    use crate::proxy_core::api::routing::{
        ChannelSpec, ChannelStatus, InterfaceKind, DEFAULT_ROUTE_GROUP,
    };
    use http::Method;
    use serde_json::json;

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
    async fn route_resolver_selects_highest_priority_matching_channel() {
        let resolver = CcSwitchRouteResolver::new(Arc::new(provider_router_from_database(
            Arc::new(Database::memory().expect("memory db")),
        )));
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

        let plan = resolver
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
}
