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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "closed"),
            CircuitState::Open => write!(f, "open"),
            CircuitState::HalfOpen => write!(f, "half_open"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AllowResult {
    pub allowed: bool,
    pub used_half_open_permit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub total_requests: u32,
    pub failed_requests: u32,
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
        AllowResult, CircuitBreakerConfig, CircuitBreakerStats, CircuitState,
        circuit_breaker_config_from_app_config, circuit_failure_threshold_from_app_config,
    };
    use crate::ports::AppProxyConfig;
    use serde_json::json;

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

    #[test]
    fn circuit_state_display_and_serde_use_external_labels() {
        assert_eq!(CircuitState::Closed.to_string(), "closed");
        assert_eq!(CircuitState::Open.to_string(), "open");
        assert_eq!(CircuitState::HalfOpen.to_string(), "half_open");
        assert_eq!(
            serde_json::to_value(CircuitState::HalfOpen).expect("serialize circuit state"),
            json!("half_open")
        );
    }

    #[test]
    fn circuit_breaker_stats_serialize_management_shape() {
        let stats = CircuitBreakerStats {
            state: CircuitState::Open,
            consecutive_failures: 4,
            consecutive_successes: 0,
            total_requests: 10,
            failed_requests: 6,
        };

        assert_eq!(
            serde_json::to_value(stats).expect("serialize stats"),
            json!({
                "state": "open",
                "consecutiveFailures": 4,
                "consecutiveSuccesses": 0,
                "totalRequests": 10,
                "failedRequests": 6
            })
        );
    }

    #[test]
    fn allow_result_preserves_half_open_permit_flag() {
        let result = AllowResult {
            allowed: true,
            used_half_open_permit: true,
        };

        assert!(result.allowed);
        assert!(result.used_half_open_permit);
    }
}
