use super::ports::AppProxyConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub timeout_seconds: u64,
    pub error_rate_threshold: f64,
    pub min_requests: u32,
}

impl From<&AppProxyConfig> for CircuitBreakerConfig {
    fn from(config: &AppProxyConfig) -> Self {
        Self {
            failure_threshold: config.circuit_failure_threshold,
            success_threshold: config.circuit_success_threshold,
            timeout_seconds: config.circuit_timeout_seconds as u64,
            error_rate_threshold: config.circuit_error_rate_threshold,
            min_requests: config.circuit_min_requests,
        }
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 4,
            success_threshold: 2,
            timeout_seconds: 60,
            error_rate_threshold: 0.6,
            min_requests: 10,
        }
    }
}

pub fn circuit_breaker_config_from_app_config(
    config: Option<&AppProxyConfig>,
) -> CircuitBreakerConfig {
    config.map(CircuitBreakerConfig::from).unwrap_or_default()
}

pub fn circuit_failure_threshold_from_app_config(
    config: Option<&AppProxyConfig>,
    fallback: u32,
) -> u32 {
    config
        .map(|config| CircuitBreakerConfig::from(config).failure_threshold)
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::{
        circuit_breaker_config_from_app_config, circuit_failure_threshold_from_app_config,
        CircuitBreakerConfig,
    };
    use crate::ports::AppProxyConfig;

    #[test]
    fn default_matches_existing_proxy_config_defaults() {
        assert_eq!(
            CircuitBreakerConfig::default(),
            CircuitBreakerConfig {
                failure_threshold: 4,
                success_threshold: 2,
                timeout_seconds: 60,
                error_rate_threshold: 0.6,
                min_requests: 10,
            }
        );
    }

    #[test]
    fn maps_app_proxy_config_fields() {
        let app_config = AppProxyConfig {
            app_type: "claude".to_string(),
            enabled: true,
            auto_failover_enabled: true,
            max_retries: 3,
            streaming_first_byte_timeout: 30,
            streaming_idle_timeout: 600,
            non_streaming_timeout: 600,
            circuit_failure_threshold: 7,
            circuit_success_threshold: 3,
            circuit_timeout_seconds: 120,
            circuit_error_rate_threshold: 0.75,
            circuit_min_requests: 25,
        };

        assert_eq!(
            circuit_breaker_config_from_app_config(Some(&app_config)),
            CircuitBreakerConfig {
                failure_threshold: 7,
                success_threshold: 3,
                timeout_seconds: 120,
                error_rate_threshold: 0.75,
                min_requests: 25,
            }
        );
    }

    #[test]
    fn missing_app_config_uses_default_or_supplied_fallback() {
        assert_eq!(
            circuit_breaker_config_from_app_config(None),
            CircuitBreakerConfig::default()
        );
        assert_eq!(circuit_failure_threshold_from_app_config(None, 9), 9);
    }
}
