//! CC Switch ProviderRouter config source.

use crate::error::AppError;
use crate::proxy::engine::routing::ProviderRouterConfigSource;
use crate::proxy::host::cc_switch::config_source::CcSwitchConfigSource;
use crate::proxy_core::api::config::{
    circuit_breaker_config_from_app_config, circuit_failure_threshold_from_app_config,
    AppProxyConfig, CircuitBreakerConfig,
};
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::ports::ProxyConfigSource;
use crate::proxy_core::api::routing::provider_router_auto_failover_enabled_decision;
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

async fn router_app_proxy_config_from_config_source(
    source: &(dyn ProxyConfigSource + Send + Sync),
    app_type: &str,
) -> Result<AppProxyConfig, AppError> {
    let app = AppKind::from(app_type);
    let config = source
        .load_app(&app)
        .await
        .map_err(|error| AppError::Message(error.to_string()))?;
    serde_json::from_value(config.raw.clone())
        .map_err(|error| AppError::Config(format!("invalid app proxy config: {error}")))
}

async fn circuit_breaker_config_from_router_config_source(
    source: &(dyn ProxyConfigSource + Send + Sync),
    app_type: &str,
) -> CircuitBreakerConfig {
    let config = router_app_proxy_config_from_config_source(source, app_type)
        .await
        .ok();
    circuit_breaker_config_from_app_config(config.as_ref())
}

async fn circuit_failure_threshold_from_router_config_source(
    source: &(dyn ProxyConfigSource + Send + Sync),
    app_type: &str,
    fallback: u32,
) -> u32 {
    let config = router_app_proxy_config_from_config_source(source, app_type)
        .await
        .ok();
    circuit_failure_threshold_from_app_config(config.as_ref(), fallback)
}

async fn auto_failover_enabled_from_router_config_source(
    source: &(dyn ProxyConfigSource + Send + Sync),
    app_type: &str,
) -> bool {
    let decision = provider_router_auto_failover_enabled_decision(
        app_type,
        router_app_proxy_config_from_config_source(source, app_type)
            .await
            .map(|config| config.auto_failover_enabled),
    );
    if let Some(log_line) = decision.error_log_line {
        log::error!("{log_line}");
    }
    decision.enabled
}
