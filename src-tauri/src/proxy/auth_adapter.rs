use super::{
    error::ProxyError,
    error_mapper::claude_desktop_gateway_auth_error_to_proxy_error,
};
use crate::proxy_core_adapter::{
    validate_claude_desktop_gateway_bearer_header, ProxyState,
};
use axum::http::HeaderMap;

pub(crate) fn validate_claude_desktop_gateway_auth(
    state: &ProxyState,
    headers: &HeaderMap,
) -> Result<(), ProxyError> {
    let expected = crate::claude_desktop_config::get_or_create_gateway_token(state.db.as_ref())
        .map_err(|error| ProxyError::AuthError(error.to_string()))?;
    validate_claude_desktop_gateway_bearer_header(headers, &expected)
        .map_err(claude_desktop_gateway_auth_error_to_proxy_error)
}
