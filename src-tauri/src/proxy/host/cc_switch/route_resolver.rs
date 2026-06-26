//! CC Switch route resolver source.

use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::management::{RouteResolveRequest, RouteResolveResponse};
use crate::proxy_core::api::ports::RouteResolver;
use crate::proxy_core::api::routing::{build_route_plan, RoutePlan, RouteRequest};
use crate::proxy_core_adapter::management_route_response_from_router_source;
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
        Box::pin(async move { build_route_plan(request) })
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
