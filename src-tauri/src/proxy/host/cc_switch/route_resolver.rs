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
