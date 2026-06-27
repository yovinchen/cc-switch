//! CC Switch route resolver source.

use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::management::{RouteResolveRequest, RouteResolveResponse};
use crate::proxy_core::api::ports::RouteResolver;
use crate::proxy_core::api::routing::{build_route_plan_with_weighted_roll, RoutePlan};
use crate::proxy_core_adapter::management_route_response_from_router_source;
use futures::future::BoxFuture;
use std::sync::Arc;

#[cfg(not(test))]
use crate::proxy_core::api::routing::RouteRequest;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::routing::RouteRequest;

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

fn route_plan_weighted_roll() -> u64 {
    let now = chrono::Utc::now();
    now.timestamp_nanos_opt()
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_else(|| u64::try_from(now.timestamp_millis()).unwrap_or_default())
}
