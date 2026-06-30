//! CC Switch ProviderRouter circuit runtime helpers.

use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::config::{CircuitBreakerConfig, CircuitBreakerStats};

pub(crate) async fn update_all_circuit_breaker_configs_source(
    router: &ProviderRouter,
    config: CircuitBreakerConfig,
) {
    router.update_all_configs(config).await;
}

pub(crate) async fn update_app_circuit_breaker_config_source(
    router: &ProviderRouter,
    app_type: &str,
    config: CircuitBreakerConfig,
) {
    router.update_app_configs(app_type, config).await;
}

pub(crate) async fn reset_provider_circuit_breaker_source(
    router: &ProviderRouter,
    provider_id: &str,
    app_type: &str,
) {
    router.reset_provider_breaker(provider_id, app_type).await;
}

pub(crate) async fn provider_circuit_breaker_stats_source(
    router: &ProviderRouter,
    provider_id: &str,
    app_type: &str,
) -> Option<CircuitBreakerStats> {
    router
        .get_circuit_breaker_stats(provider_id, app_type)
        .await
}
