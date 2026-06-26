//! CC Switch ProviderRouter channel source.

use crate::error::AppError;
use crate::proxy::engine::routing::ProviderRouterChannelSource;
use crate::proxy::host::cc_switch::database_channel_source::CcSwitchChannelSource;
use crate::proxy_core_adapter::{
    app_error_from_proxy_core_error, router_channel_route_inputs_from_channel_source,
    ChannelRouteSource, RouteResolveChannelInput,
};
use futures::future::BoxFuture;

pub(crate) struct CcSwitchProviderRouterChannelSource {
    source: CcSwitchChannelSource,
}

impl CcSwitchProviderRouterChannelSource {
    pub(crate) fn new(source: CcSwitchChannelSource) -> Self {
        Self { source }
    }
}

impl ProviderRouterChannelSource for CcSwitchProviderRouterChannelSource {
    fn channel_route_inputs<'a>(
        &'a self,
        app_type: &'a str,
    ) -> BoxFuture<'a, Result<(Vec<RouteResolveChannelInput>, ChannelRouteSource), AppError>> {
        Box::pin(async move {
            router_channel_route_inputs_from_channel_source(&self.source, app_type)
                .await
                .map_err(app_error_from_proxy_core_error)
        })
    }
}
