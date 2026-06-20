use serde_json::Value;

use crate::error::ProxyErrorStatusKind;

pub const PROVIDER_FAILED_RETRY: &str = "FWD-001";
pub const ALL_PROVIDERS_FAILED: &str = "FWD-002";
pub const SINGLE_PROVIDER_FAILED: &str = "FWD-003";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardFailureLog {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardFailureKind {
    Upstream { status: u16, body: Option<String> },
    Timeout(String),
    ForwardFailed(String),
    TransformError(String),
    ConfigError(String),
    AuthError(String),
    RetryableOther(String),
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardFailureCategory {
    Retryable,
    NonRetryable,
}

pub fn forward_failure_kind_from_proxy_status(
    kind: ProxyErrorStatusKind,
    message: impl Into<String>,
    upstream_body: Option<String>,
) -> ForwardFailureKind {
    let message = message.into();
    match kind {
        ProxyErrorStatusKind::UpstreamError(status) => ForwardFailureKind::Upstream {
            status,
            body: upstream_body,
        },
        ProxyErrorStatusKind::Timeout => ForwardFailureKind::Timeout(message),
        ProxyErrorStatusKind::ForwardFailed => ForwardFailureKind::ForwardFailed(message),
        ProxyErrorStatusKind::TransformError => ForwardFailureKind::TransformError(message),
        ProxyErrorStatusKind::ConfigError => ForwardFailureKind::ConfigError(message),
        ProxyErrorStatusKind::AuthError => ForwardFailureKind::AuthError(message),
        ProxyErrorStatusKind::ProviderUnhealthy | ProxyErrorStatusKind::StreamIdleTimeout => {
            ForwardFailureKind::RetryableOther(message)
        }
        _ => ForwardFailureKind::Other(message),
    }
}

pub fn categorize_forward_failure(failure: &ForwardFailureKind) -> ForwardFailureCategory {
    match failure {
        ForwardFailureKind::Timeout(_)
        | ForwardFailureKind::ForwardFailed(_)
        | ForwardFailureKind::TransformError(_)
        | ForwardFailureKind::ConfigError(_)
        | ForwardFailureKind::AuthError(_)
        | ForwardFailureKind::RetryableOther(_) => ForwardFailureCategory::Retryable,
        ForwardFailureKind::Upstream { status, .. } => match *status {
            400 | 405 | 406 | 413 | 414 | 415 | 422 | 501 => ForwardFailureCategory::NonRetryable,
            _ => ForwardFailureCategory::Retryable,
        },
        ForwardFailureKind::Other(_) => ForwardFailureCategory::NonRetryable,
    }
}

pub fn should_failover_after_rectifier_retry_failure(failure: &ForwardFailureKind) -> bool {
    match failure {
        ForwardFailureKind::Timeout(_) | ForwardFailureKind::ForwardFailed(_) => true,
        ForwardFailureKind::Upstream { status, .. } => *status >= 500,
        _ => false,
    }
}

pub fn build_retryable_forward_failure_log(
    provider_name: &str,
    attempted_providers: usize,
    total_providers: usize,
    failure: &ForwardFailureKind,
) -> ForwardFailureLog {
    let error_summary = summarize_forward_failure(failure);

    if total_providers <= 1 {
        ForwardFailureLog {
            code: SINGLE_PROVIDER_FAILED,
            message: format!("Provider {provider_name} 请求失败: {error_summary}"),
        }
    } else {
        ForwardFailureLog {
            code: PROVIDER_FAILED_RETRY,
            message: format!(
                "Provider {provider_name} 失败，继续尝试下一个 ({attempted_providers}/{total_providers}): {error_summary}"
            ),
        }
    }
}

pub fn build_terminal_forward_failure_log(
    attempted_providers: usize,
    total_providers: usize,
    last_failure: Option<&ForwardFailureKind>,
) -> Option<ForwardFailureLog> {
    if total_providers <= 1 {
        return None;
    }

    let error_summary = last_failure
        .map(summarize_forward_failure)
        .unwrap_or_else(|| "未知错误".to_string());

    Some(ForwardFailureLog {
        code: ALL_PROVIDERS_FAILED,
        message: format!(
            "已尝试 {attempted_providers}/{total_providers} 个 Provider，均失败。最后错误: {error_summary}"
        ),
    })
}

pub fn summarize_forward_failure(failure: &ForwardFailureKind) -> String {
    match failure {
        ForwardFailureKind::Upstream { status, body } => {
            let body_summary = body
                .as_deref()
                .map(summarize_upstream_body_for_log)
                .filter(|summary| !summary.is_empty());

            match body_summary {
                Some(summary) => format!("上游 HTTP {status}: {summary}"),
                None => format!("上游 HTTP {status}"),
            }
        }
        ForwardFailureKind::Timeout(message) => {
            format!("请求超时: {}", summarize_text_for_log(message, 180))
        }
        ForwardFailureKind::ForwardFailed(message) => {
            format!("请求转发失败: {}", summarize_text_for_log(message, 180))
        }
        ForwardFailureKind::TransformError(message) => {
            format!("响应转换失败: {}", summarize_text_for_log(message, 180))
        }
        ForwardFailureKind::ConfigError(message) => {
            format!("配置错误: {}", summarize_text_for_log(message, 180))
        }
        ForwardFailureKind::AuthError(message) => {
            format!("认证失败: {}", summarize_text_for_log(message, 180))
        }
        ForwardFailureKind::RetryableOther(message) | ForwardFailureKind::Other(message) => {
            summarize_text_for_log(message, 180)
        }
    }
}

pub fn summarize_upstream_body_for_log(body: &str) -> String {
    if let Ok(json_body) = serde_json::from_str::<Value>(body) {
        if let Some(message) = extract_json_error_message(&json_body) {
            return summarize_text_for_log(&message, 180);
        }

        if let Ok(compact_json) = serde_json::to_string(&json_body) {
            return summarize_text_for_log(&compact_json, 180);
        }
    }

    summarize_text_for_log(body, 180)
}

pub fn summarize_text_for_log(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized.trim();

    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let truncated: String = trimmed.chars().take(max_chars).collect();
    let truncated = truncated.trim_end();
    format!("{truncated}...")
}

fn extract_json_error_message(body: &Value) -> Option<String> {
    let candidates = [
        body.pointer("/error/message"),
        body.pointer("/message"),
        body.pointer("/detail"),
        body.pointer("/error"),
    ];

    candidates
        .into_iter()
        .flatten()
        .find_map(|value| value.as_str().map(ToString::to_string))
}

#[cfg(test)]
mod tests {
    use super::{
        build_retryable_forward_failure_log, build_terminal_forward_failure_log,
        categorize_forward_failure, forward_failure_kind_from_proxy_status,
        should_failover_after_rectifier_retry_failure, summarize_text_for_log,
        summarize_upstream_body_for_log, ForwardFailureCategory, ForwardFailureKind,
        ALL_PROVIDERS_FAILED, PROVIDER_FAILED_RETRY, SINGLE_PROVIDER_FAILED,
    };
    use crate::error::ProxyErrorStatusKind;
    use serde_json::json;

    #[test]
    fn single_provider_retryable_log_uses_single_provider_code() {
        let failure = ForwardFailureKind::Upstream {
            status: 429,
            body: Some(r#"{"error":{"message":"rate limit exceeded"}}"#.to_string()),
        };

        let log = build_retryable_forward_failure_log("PackyCode-response", 1, 1, &failure);

        assert_eq!(log.code, SINGLE_PROVIDER_FAILED);
        assert!(log.message.contains("Provider PackyCode-response 请求失败"));
        assert!(log.message.contains("上游 HTTP 429"));
        assert!(log.message.contains("rate limit exceeded"));
        assert!(!log.message.contains("切换下一个"));
    }

    #[test]
    fn multi_provider_retryable_log_keeps_failover_wording() {
        let failure = ForwardFailureKind::Timeout("upstream timed out after 30s".to_string());

        let log = build_retryable_forward_failure_log("primary", 1, 3, &failure);

        assert_eq!(log.code, PROVIDER_FAILED_RETRY);
        assert!(log.message.contains("继续尝试下一个 (1/3)"));
        assert!(log.message.contains("请求超时"));
    }

    #[test]
    fn single_provider_has_no_terminal_all_failed_log() {
        assert!(build_terminal_forward_failure_log(1, 1, None).is_none());
    }

    #[test]
    fn multi_provider_terminal_log_contains_last_error_summary() {
        let failure = ForwardFailureKind::ForwardFailed("connection reset by peer".to_string());

        let log = build_terminal_forward_failure_log(2, 2, Some(&failure))
            .expect("expected terminal log");

        assert_eq!(log.code, ALL_PROVIDERS_FAILED);
        assert!(log.message.contains("已尝试 2/2 个 Provider，均失败"));
        assert!(log.message.contains("connection reset by peer"));
    }

    #[test]
    fn summarize_upstream_body_prefers_json_message() {
        let body = json!({
            "error": {
                "message": "invalid_request_error: unsupported field"
            },
            "request_id": "req_123"
        });

        let summary = summarize_upstream_body_for_log(&body.to_string());

        assert_eq!(summary, "invalid_request_error: unsupported field");
    }

    #[test]
    fn summarize_text_for_log_collapses_whitespace_and_truncates() {
        let summary = summarize_text_for_log("line1\n\n line2   line3", 12);

        assert_eq!(summary, "line1 line2...");
    }

    #[test]
    fn categorizes_client_request_upstream_statuses_as_non_retryable() {
        for status in [400, 405, 406, 413, 414, 415, 422, 501] {
            let failure = ForwardFailureKind::Upstream { status, body: None };

            assert_eq!(
                categorize_forward_failure(&failure),
                ForwardFailureCategory::NonRetryable,
                "status {status} should not fail over"
            );
        }
    }

    #[test]
    fn categorizes_quota_auth_network_and_server_errors_as_retryable() {
        let failures = [
            ForwardFailureKind::Upstream {
                status: 401,
                body: None,
            },
            ForwardFailureKind::Upstream {
                status: 429,
                body: None,
            },
            ForwardFailureKind::Upstream {
                status: 500,
                body: None,
            },
            ForwardFailureKind::Timeout("timeout".to_string()),
            ForwardFailureKind::ForwardFailed("connection reset".to_string()),
            ForwardFailureKind::ConfigError("bad provider config".to_string()),
            ForwardFailureKind::TransformError("transform failed".to_string()),
            ForwardFailureKind::AuthError("token expired".to_string()),
            ForwardFailureKind::RetryableOther("provider unhealthy".to_string()),
        ];

        for failure in failures {
            assert_eq!(
                categorize_forward_failure(&failure),
                ForwardFailureCategory::Retryable
            );
        }
    }

    #[test]
    fn categorizes_unknown_host_errors_as_non_retryable() {
        let failure = ForwardFailureKind::Other("database error".to_string());

        assert_eq!(
            categorize_forward_failure(&failure),
            ForwardFailureCategory::NonRetryable
        );
    }

    #[test]
    fn proxy_status_kind_maps_to_forward_failure_kind() {
        assert!(matches!(
            forward_failure_kind_from_proxy_status(
                ProxyErrorStatusKind::Timeout,
                "slow",
                None
            ),
            ForwardFailureKind::Timeout(message) if message == "slow"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_status(
                ProxyErrorStatusKind::ForwardFailed,
                "connection reset",
                None
            ),
            ForwardFailureKind::ForwardFailed(message) if message == "connection reset"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_status(
                ProxyErrorStatusKind::AuthError,
                "bad token",
                None
            ),
            ForwardFailureKind::AuthError(message) if message == "bad token"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_status(
                ProxyErrorStatusKind::ProviderUnhealthy,
                "half-open",
                None
            ),
            ForwardFailureKind::RetryableOther(message) if message == "half-open"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_status(
                ProxyErrorStatusKind::DatabaseError,
                "write failed",
                None
            ),
            ForwardFailureKind::Other(message) if message == "write failed"
        ));

        let failure = forward_failure_kind_from_proxy_status(
            ProxyErrorStatusKind::UpstreamError(429),
            "ignored",
            Some(r#"{"error":{"message":"rate limit"}}"#.to_string()),
        );

        match failure {
            ForwardFailureKind::Upstream { status, body } => {
                assert_eq!(status, 429);
                assert_eq!(
                    body.as_deref(),
                    Some(r#"{"error":{"message":"rate limit"}}"#)
                );
            }
            other => panic!("expected upstream failure, got {other:?}"),
        }
    }

    #[test]
    fn rectifier_retry_failover_only_for_network_and_server_side_errors() {
        for failure in [
            ForwardFailureKind::Timeout("timeout".to_string()),
            ForwardFailureKind::ForwardFailed("connection reset".to_string()),
            ForwardFailureKind::Upstream {
                status: 500,
                body: None,
            },
            ForwardFailureKind::Upstream {
                status: 503,
                body: None,
            },
        ] {
            assert!(
                should_failover_after_rectifier_retry_failure(&failure),
                "{failure:?} should keep provider failover alive"
            );
        }

        for failure in [
            ForwardFailureKind::Upstream {
                status: 400,
                body: None,
            },
            ForwardFailureKind::Upstream {
                status: 429,
                body: None,
            },
            ForwardFailureKind::TransformError("still invalid".to_string()),
            ForwardFailureKind::AuthError("bad token".to_string()),
            ForwardFailureKind::RetryableOther("provider unhealthy".to_string()),
            ForwardFailureKind::Other("database error".to_string()),
        ] {
            assert!(
                !should_failover_after_rectifier_retry_failure(&failure),
                "{failure:?} should end rectifier retry as a client/non-provider failure"
            );
        }
    }
}
