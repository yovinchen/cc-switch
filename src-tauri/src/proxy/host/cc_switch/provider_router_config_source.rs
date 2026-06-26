//! CC Switch ProviderRouter config source.

use crate::proxy::engine::routing::ProviderRouterConfigSource;
use crate::proxy_core_adapter::{
    auto_failover_enabled_from_router_config_source,
    circuit_breaker_config_from_router_config_source,
    circuit_failure_threshold_from_router_config_source, CcSwitchConfigSource,
    CircuitBreakerConfig,
};
use futures::future::BoxFuture;

pub(crate) struct CcSwitchProviderRouterConfigSource {
    source: CcSwitchConfigSource,
}

impl CcSwitchProviderRouterConfigSource {
    pub(crate) fn new(source: CcSwitchConfigSource) -> Self {
        Self { source }
    }
}

impl ProviderRouterConfigSource for CcSwitchProviderRouterConfigSource {
    fn load_failover_enabled<'a>(&'a self, app_type: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(async move {
            auto_failover_enabled_from_router_config_source(&self.source, app_type).await
        })
    }

    fn circuit_breaker_config<'a>(
        &'a self,
        app_type: &'a str,
    ) -> BoxFuture<'a, CircuitBreakerConfig> {
        Box::pin(async move {
            circuit_breaker_config_from_router_config_source(&self.source, app_type).await
        })
    }

    fn failure_threshold<'a>(&'a self, app_type: &'a str, fallback: u32) -> BoxFuture<'a, u32> {
        Box::pin(async move {
            circuit_failure_threshold_from_router_config_source(&self.source, app_type, fallback)
                .await
        })
    }
}
