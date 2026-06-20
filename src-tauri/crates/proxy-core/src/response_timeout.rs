use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamingTimeoutConfig {
    /// First-byte timeout in seconds. `0` disables the timeout.
    pub first_byte_timeout: u64,
    /// Idle timeout in seconds. `0` disables the timeout.
    pub idle_timeout: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingTimeoutPhase {
    FirstByte,
    Idle,
}

impl StreamingTimeoutConfig {
    pub fn duration_for_phase(self, phase: StreamingTimeoutPhase) -> Option<Duration> {
        let seconds = match phase {
            StreamingTimeoutPhase::FirstByte => self.first_byte_timeout,
            StreamingTimeoutPhase::Idle => self.idle_timeout,
        };
        (seconds > 0).then(|| Duration::from_secs(seconds))
    }
}

impl StreamingTimeoutPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::FirstByte => "首字节",
            Self::Idle => "静默期",
        }
    }

    pub fn timeout_message(self) -> String {
        format!("流式响应{}超时", self.label())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResponseTimeoutConfig {
    /// Non-streaming body timeout in seconds. `0` disables the timeout.
    pub non_streaming_timeout: u64,
    pub streaming: StreamingTimeoutConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResponseRuntimePolicy {
    pub timeout: ResponseTimeoutConfig,
    pub max_retries: u32,
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

pub fn non_streaming_body_timeout_message(timeout: Duration) -> String {
    format!(
        "响应体读取超时: {}s（上游发完响应头后 body 未到达）",
        timeout.as_secs()
    )
}

pub fn resolve_response_runtime_policy(
    auto_failover_enabled: bool,
    max_retries: u32,
    non_streaming_timeout: u64,
    streaming_first_byte_timeout: u64,
    streaming_idle_timeout: u64,
) -> ResponseRuntimePolicy {
    ResponseRuntimePolicy {
        timeout: resolve_response_timeout_config(
            auto_failover_enabled,
            non_streaming_timeout,
            streaming_first_byte_timeout,
            streaming_idle_timeout,
        ),
        max_retries: resolve_failover_max_retries(auto_failover_enabled, max_retries),
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

pub fn resolve_failover_max_retries(auto_failover_enabled: bool, max_retries: u32) -> u32 {
    if auto_failover_enabled {
        max_retries
    } else {
        0
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

    #[test]
    fn streaming_timeout_duration_selects_first_byte_and_idle_phases() {
        let config = StreamingTimeoutConfig {
            first_byte_timeout: 5,
            idle_timeout: 30,
        };

        assert_eq!(
            config.duration_for_phase(StreamingTimeoutPhase::FirstByte),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            config.duration_for_phase(StreamingTimeoutPhase::Idle),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn streaming_timeout_duration_disables_zero_values_by_phase() {
        let config = StreamingTimeoutConfig {
            first_byte_timeout: 0,
            idle_timeout: 30,
        };

        assert_eq!(
            config.duration_for_phase(StreamingTimeoutPhase::FirstByte),
            None
        );
        assert_eq!(
            config.duration_for_phase(StreamingTimeoutPhase::Idle),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn streaming_timeout_phase_labels_and_messages_match_host_contract() {
        assert_eq!(StreamingTimeoutPhase::FirstByte.label(), "首字节");
        assert_eq!(StreamingTimeoutPhase::Idle.label(), "静默期");
        assert_eq!(
            StreamingTimeoutPhase::FirstByte.timeout_message(),
            "流式响应首字节超时"
        );
        assert_eq!(
            StreamingTimeoutPhase::Idle.timeout_message(),
            "流式响应静默期超时"
        );
    }

    #[test]
    fn non_streaming_body_timeout_message_matches_host_contract() {
        assert_eq!(
            non_streaming_body_timeout_message(Duration::from_secs(45)),
            "响应体读取超时: 45s（上游发完响应头后 body 未到达）"
        );
    }

    #[test]
    fn resolve_response_runtime_policy_uses_failover_values_when_enabled() {
        let policy = resolve_response_runtime_policy(true, 3, 600, 60, 120);

        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.timeout.non_streaming_timeout, 600);
        assert_eq!(policy.timeout.streaming.first_byte_timeout, 60);
        assert_eq!(policy.timeout.streaming.idle_timeout, 120);
    }

    #[test]
    fn resolve_response_runtime_policy_disables_retry_and_timeout_when_failover_disabled() {
        let policy = resolve_response_runtime_policy(false, 3, 600, 60, 120);

        assert_eq!(policy.max_retries, 0);
        assert_eq!(policy.timeout, ResponseTimeoutConfig::default());
    }
}
