//! CC Switch route resolver source.

use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core_adapter::{
    management_route_response_from_router_source, route_plan_from_request, ProxyCoreResult,
    RoutePlan, RouteRequest, RouteResolveRequest, RouteResolveResponse, RouteResolver,
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
        Box::pin(async move { route_plan_from_request(request) })
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
