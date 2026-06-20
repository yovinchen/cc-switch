use crate::proxy_core_adapter::{proxy_error_http_status_code, ProxyErrorStatusKind};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("服务器已在运行")]
    AlreadyRunning,

    #[error("服务器未运行")]
    NotRunning,

    #[error("地址绑定失败: {0}")]
    BindFailed(String),

    #[error("停止超时")]
    StopTimeout,

    #[error("停止失败: {0}")]
    StopFailed(String),

    #[error("请求转发失败: {0}")]
    ForwardFailed(String),

    #[error("无可用的Provider")]
    NoAvailableProvider,

    #[error("所有供应商已熔断，无可用渠道")]
    AllProvidersCircuitOpen,

    #[error("未配置供应商")]
    NoProvidersConfigured,

    #[allow(dead_code)]
    #[error("Provider不健康: {0}")]
    ProviderUnhealthy(String),

    #[error("上游错误 (状态码 {status}): {body:?}")]
    UpstreamError { status: u16, body: Option<String> },

    #[error("超过最大重试次数")]
    MaxRetriesExceeded,

    #[error("数据库错误: {0}")]
    DatabaseError(String),

    #[error("配置错误: {0}")]
    ConfigError(String),

    #[allow(dead_code)]
    #[error("格式转换错误: {0}")]
    TransformError(String),

    #[allow(dead_code)]
    #[error("无效的请求: {0}")]
    InvalidRequest(String),

    #[error("超时: {0}")]
    Timeout(String),

    /// 流式响应空闲超时
    #[allow(dead_code)]
    #[error("流式响应空闲超时: {0}秒无数据")]
    StreamIdleTimeout(u64),

    /// 认证错误
    #[error("认证失败: {0}")]
    AuthError(String),

    #[allow(dead_code)]
    #[error("内部错误: {0}")]
    Internal(String),
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            ProxyError::UpstreamError {
                status: upstream_status,
                body: upstream_body,
            } => {
                let http_status = status_code_from_proxy_error(&self, StatusCode::BAD_GATEWAY);

                // 尝试解析上游响应体为 JSON，如果失败则包装为字符串
                let error_body = if let Some(body_str) = upstream_body {
                    if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(body_str) {
                        // 上游返回的是 JSON，直接透传
                        json_body
                    } else {
                        // 上游返回的不是 JSON，包装为错误消息
                        json!({
                            "error": {
                                "message": body_str,
                                "type": "upstream_error",
                            }
                        })
                    }
                } else {
                    json!({
                        "error": {
                            "message": format!("Upstream error (status {})", upstream_status),
                            "type": "upstream_error",
                        }
                    })
                };

                (http_status, error_body)
            }
            _ => {
                let http_status =
                    status_code_from_proxy_error(&self, StatusCode::INTERNAL_SERVER_ERROR);
                let message = self.to_string();

                let error_body = json!({
                    "error": {
                        "message": message,
                        "type": "proxy_error",
                    }
                });

                (http_status, error_body)
            }
        };

        (status, Json(body)).into_response()
    }
}

pub(crate) fn proxy_error_status_kind(error: &ProxyError) -> ProxyErrorStatusKind {
    match error {
        ProxyError::AlreadyRunning => ProxyErrorStatusKind::AlreadyRunning,
        ProxyError::NotRunning => ProxyErrorStatusKind::NotRunning,
        ProxyError::BindFailed(_) => ProxyErrorStatusKind::BindFailed,
        ProxyError::StopTimeout => ProxyErrorStatusKind::StopTimeout,
        ProxyError::StopFailed(_) => ProxyErrorStatusKind::StopFailed,
        ProxyError::ForwardFailed(_) => ProxyErrorStatusKind::ForwardFailed,
        ProxyError::NoAvailableProvider => ProxyErrorStatusKind::NoAvailableProvider,
        ProxyError::AllProvidersCircuitOpen => ProxyErrorStatusKind::AllProvidersCircuitOpen,
        ProxyError::NoProvidersConfigured => ProxyErrorStatusKind::NoProvidersConfigured,
        ProxyError::ProviderUnhealthy(_) => ProxyErrorStatusKind::ProviderUnhealthy,
        ProxyError::UpstreamError { status, .. } => ProxyErrorStatusKind::UpstreamError(*status),
        ProxyError::MaxRetriesExceeded => ProxyErrorStatusKind::MaxRetriesExceeded,
        ProxyError::DatabaseError(_) => ProxyErrorStatusKind::DatabaseError,
        ProxyError::ConfigError(_) => ProxyErrorStatusKind::ConfigError,
        ProxyError::TransformError(_) => ProxyErrorStatusKind::TransformError,
        ProxyError::InvalidRequest(_) => ProxyErrorStatusKind::InvalidRequest,
        ProxyError::Timeout(_) => ProxyErrorStatusKind::Timeout,
        ProxyError::StreamIdleTimeout(_) => ProxyErrorStatusKind::StreamIdleTimeout,
        ProxyError::AuthError(_) => ProxyErrorStatusKind::AuthError,
        ProxyError::Internal(_) => ProxyErrorStatusKind::Internal,
    }
}

fn status_code_from_proxy_error(error: &ProxyError, fallback: StatusCode) -> StatusCode {
    StatusCode::from_u16(proxy_error_http_status_code(proxy_error_status_kind(error)))
        .unwrap_or(fallback)
}

/// 错误分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// 可重试错误（网络问题、5xx）
    Retryable, // 网络超时、5xx 错误
    /// 不可重试错误（4xx、认证失败）
    NonRetryable, // 认证失败、参数错误、4xx 错误
    #[allow(dead_code)]
    ClientAbort, // 客户端主动中断
}

/// 判断错误是否可重试
#[allow(dead_code)]
pub fn categorize_error(error: &reqwest::Error) -> ErrorCategory {
    if error.is_timeout() || error.is_connect() {
        return ErrorCategory::Retryable;
    }

    if let Some(status) = error.status() {
        if status.is_server_error() {
            ErrorCategory::Retryable
        } else if status.is_client_error() {
            ErrorCategory::NonRetryable
        } else {
            ErrorCategory::Retryable
        }
    } else {
        ErrorCategory::Retryable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_error_into_response_uses_shared_status_contract() {
        let response = ProxyError::ForwardFailed("dns lookup failed".to_string()).into_response();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn invalid_upstream_status_falls_back_to_bad_gateway_response() {
        let response = (ProxyError::UpstreamError {
            status: 42,
            body: None,
        })
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }
}
