use super::{error::ProxyError, error_mapper::proxy_core_error_to_proxy_error};
use crate::proxy_core_adapter::ProxyState;
use axum::http::HeaderMap;

pub(crate) async fn validate_claude_desktop_gateway_auth(
    state: &ProxyState,
    headers: &HeaderMap,
) -> Result<(), ProxyError> {
    state
        .proxy_engine()
        .validate_claude_desktop_gateway_auth(headers)
        .await
        .map_err(proxy_core_error_to_proxy_error)
}

pub(crate) async fn validate_proxy_management_auth(
    state: &ProxyState,
    headers: &HeaderMap,
) -> Result<(), ProxyError> {
    state
        .proxy_engine()
        .validate_management_auth(headers)
        .await
        .map_err(proxy_core_error_to_proxy_error)
}
