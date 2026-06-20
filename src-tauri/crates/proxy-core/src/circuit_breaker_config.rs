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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitBreakerFailureDecision {
    KeepCurrent,
    OpenFromHalfOpenProbeFailure,
    OpenFromFailureThreshold { failures: u32 },
    OpenFromErrorRate { error_rate: f64 },
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

pub fn should_transition_open_to_half_open(
    open_elapsed_seconds: Option<u64>,
    timeout_seconds: u64,
) -> bool {
    open_elapsed_seconds.is_some_and(|elapsed| elapsed >= timeout_seconds)
}

pub fn should_close_half_open_after_success(
    consecutive_successes: u32,
    success_threshold: u32,
) -> bool {
    consecutive_successes >= success_threshold
}

pub fn half_open_probe_allow_result(current_requests: u32, max_requests: u32) -> AllowResult {
    AllowResult {
        allowed: current_requests < max_requests,
        used_half_open_permit: current_requests < max_requests,
    }
}

pub fn circuit_breaker_failure_decision(
    state: CircuitState,
    consecutive_failures: u32,
    total_requests: u32,
    failed_requests: u32,
    config: &CircuitBreakerConfig,
) -> CircuitBreakerFailureDecision {
    match state {
        CircuitState::HalfOpen => CircuitBreakerFailureDecision::OpenFromHalfOpenProbeFailure,
        CircuitState::Closed => {
            if consecutive_failures >= config.failure_threshold {
                CircuitBreakerFailureDecision::OpenFromFailureThreshold {
                    failures: consecutive_failures,
                }
            } else if total_requests >= config.min_requests {
                let error_rate = failed_requests as f64 / total_requests as f64;
                if error_rate >= config.error_rate_threshold {
                    CircuitBreakerFailureDecision::OpenFromErrorRate { error_rate }
                } else {
                    CircuitBreakerFailureDecision::KeepCurrent
                }
            } else {
                CircuitBreakerFailureDecision::KeepCurrent
            }
        }
        CircuitState::Open => CircuitBreakerFailureDecision::KeepCurrent,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        circuit_breaker_config_from_app_config, circuit_breaker_failure_decision,
        circuit_failure_threshold_from_app_config, half_open_probe_allow_result,
        should_close_half_open_after_success, should_transition_open_to_half_open, AllowResult,
        CircuitBreakerConfig, CircuitBreakerFailureDecision, CircuitBreakerStats, CircuitState,
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

    #[test]
    fn open_timeout_transition_requires_elapsed_timeout() {
        assert!(!should_transition_open_to_half_open(None, 60));
        assert!(!should_transition_open_to_half_open(Some(59), 60));
        assert!(should_transition_open_to_half_open(Some(60), 60));
        assert!(should_transition_open_to_half_open(Some(61), 60));
    }

    #[test]
    fn half_open_success_and_probe_rules_preserve_runtime_policy() {
        assert!(!should_close_half_open_after_success(1, 2));
        assert!(should_close_half_open_after_success(2, 2));
        assert!(should_close_half_open_after_success(3, 2));

        assert_eq!(
            half_open_probe_allow_result(0, 1),
            AllowResult {
                allowed: true,
                used_half_open_permit: true,
            }
        );
        assert_eq!(
            half_open_probe_allow_result(1, 1),
            AllowResult {
                allowed: false,
                used_half_open_permit: false,
            }
        );
    }

    #[test]
    fn failure_decision_preserves_circuit_breaker_open_rules() {
        let config = CircuitBreakerConfig {
            failure_threshold: 4,
            success_threshold: 2,
            timeout_seconds: 60,
            error_rate_threshold: 0.6,
            min_requests: 10,
        };

        assert_eq!(
            circuit_breaker_failure_decision(CircuitState::HalfOpen, 1, 1, 1, &config),
            CircuitBreakerFailureDecision::OpenFromHalfOpenProbeFailure
        );
        assert_eq!(
            circuit_breaker_failure_decision(CircuitState::Closed, 4, 4, 4, &config),
            CircuitBreakerFailureDecision::OpenFromFailureThreshold { failures: 4 }
        );
        assert_eq!(
            circuit_breaker_failure_decision(CircuitState::Closed, 2, 10, 6, &config),
            CircuitBreakerFailureDecision::OpenFromErrorRate { error_rate: 0.6 }
        );
        assert_eq!(
            circuit_breaker_failure_decision(CircuitState::Closed, 2, 9, 9, &config),
            CircuitBreakerFailureDecision::KeepCurrent
        );
        assert_eq!(
            circuit_breaker_failure_decision(CircuitState::Open, 10, 10, 10, &config),
            CircuitBreakerFailureDecision::KeepCurrent
        );
    }
}
