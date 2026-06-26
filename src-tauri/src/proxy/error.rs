use crate::proxy::error_mapper::proxy_error_status_kind;
use crate::proxy_core_adapter::{
    proxy_error_http_status_code, proxy_error_response_body, upstream_proxy_error_response_body,
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
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

    #[allow(dead_code)]
    #[error("所有供应商已熔断，无可用渠道")]
    AllProvidersCircuitOpen,

    #[allow(dead_code)]
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

    #[error("格式转换错误: {0}")]
    TransformError(String),

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
                let error_body =
                    upstream_proxy_error_response_body(*upstream_status, upstream_body.as_deref());

                (http_status, error_body)
            }
            _ => {
                let http_status =
                    status_code_from_proxy_error(&self, StatusCode::INTERNAL_SERVER_ERROR);
                let message = self.to_string();

                let error_body = proxy_error_response_body(message);

                (http_status, error_body)
            }
        };

        (status, Json(body)).into_response()
    }
}

fn status_code_from_proxy_error(error: &ProxyError, fallback: StatusCode) -> StatusCode {
    StatusCode::from_u16(proxy_error_http_status_code(proxy_error_status_kind(error)))
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn proxy_error_into_response_uses_shared_status_and_body_contract() {
        let response = ProxyError::ForwardFailed("dns lookup failed".to_string()).into_response();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("proxy error json body");
        assert_eq!(json["error"]["message"], "请求转发失败: dns lookup failed");
        assert_eq!(json["error"]["type"], "proxy_error");
    }

    #[tokio::test]
    async fn invalid_upstream_status_falls_back_to_bad_gateway_response_and_body() {
        let response = (ProxyError::UpstreamError {
            status: 42,
            body: None,
        })
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let json: serde_json::Value =
            serde_json::from_slice(&body).expect("upstream error json body");
        assert_eq!(json["error"]["message"], "Upstream error (status 42)");
        assert_eq!(json["error"]["type"], "upstream_error");
    }
}
