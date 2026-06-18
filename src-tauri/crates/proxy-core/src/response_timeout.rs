use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamingTimeoutConfig {
    /// First-byte timeout in seconds. `0` disables the timeout.
    pub first_byte_timeout: u64,
    /// Idle timeout in seconds. `0` disables the timeout.
    pub idle_timeout: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResponseTimeoutConfig {
    /// Non-streaming body timeout in seconds. `0` disables the timeout.
    pub non_streaming_timeout: u64,
    pub streaming: StreamingTimeoutConfig,
}

impl ResponseTimeoutConfig {
    pub fn body_timeout_duration(self) -> Duration {
        if self.non_streaming_timeout > 0 {
            Duration::from_secs(self.non_streaming_timeout)
        } else {
            Duration::ZERO
        }
    }
}

pub fn resolve_response_timeout_config(
    auto_failover_enabled: bool,
    non_streaming_timeout: u64,
    streaming_first_byte_timeout: u64,
    streaming_idle_timeout: u64,
) -> ResponseTimeoutConfig {
    if auto_failover_enabled {
        ResponseTimeoutConfig {
            non_streaming_timeout,
            streaming: StreamingTimeoutConfig {
                first_byte_timeout: streaming_first_byte_timeout,
                idle_timeout: streaming_idle_timeout,
            },
        }
    } else {
        ResponseTimeoutConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_response_timeout_config_uses_values_when_failover_enabled() {
        let config = resolve_response_timeout_config(true, 600, 60, 120);

        assert_eq!(config.non_streaming_timeout, 600);
        assert_eq!(config.streaming.first_byte_timeout, 60);
        assert_eq!(config.streaming.idle_timeout, 120);
        assert_eq!(config.body_timeout_duration(), Duration::from_secs(600));
    }

    #[test]
    fn resolve_response_timeout_config_disables_timeouts_when_failover_disabled() {
        let config = resolve_response_timeout_config(false, 600, 60, 120);

        assert_eq!(config, ResponseTimeoutConfig::default());
        assert_eq!(config.body_timeout_duration(), Duration::ZERO);
    }

    #[test]
    fn zero_timeout_values_remain_disabled_when_failover_enabled() {
        let config = resolve_response_timeout_config(true, 0, 0, 0);

        assert_eq!(config.non_streaming_timeout, 0);
        assert_eq!(config.streaming.first_byte_timeout, 0);
        assert_eq!(config.streaming.idle_timeout, 0);
        assert_eq!(config.body_timeout_duration(), Duration::ZERO);
    }
}
