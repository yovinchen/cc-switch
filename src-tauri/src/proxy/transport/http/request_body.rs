use crate::proxy::error::ProxyError;
use crate::proxy_core::api::transport::{
    endpoint_from_path_and_query, parse_json_proxy_request_body,
    parse_json_proxy_request_body_or_null, request_body_read_error_message,
};
use bytes::Bytes;
use http::{HeaderMap, Method, Uri};
use http_body_util::BodyExt;
use serde_json::Value;

pub(crate) struct ParsedHttpJsonProxyRequest {
    pub(crate) method: Method,
    pub(crate) uri: Uri,
    pub(crate) headers: HeaderMap,
    pub(crate) extensions: http::Extensions,
    pub(crate) body: Value,
    pub(crate) is_stream: bool,
}

pub(crate) fn endpoint_from_uri(uri: &Uri) -> String {
    endpoint_from_path_and_query(uri.path(), uri.query())
}

async fn collect_axum_request_body(body: axum::body::Body) -> Result<Bytes, ProxyError> {
    body.collect()
        .await
        .map_err(|error| ProxyError::Internal(request_body_read_error_message(error)))
        .map(|collected| collected.to_bytes())
}

pub(crate) async fn collect_json_proxy_request(
    request: axum::extract::Request,
) -> Result<ParsedHttpJsonProxyRequest, ProxyError> {
    let (parts, body) = request.into_parts();
    let body_bytes = collect_axum_request_body(body).await?;
    let parsed = parse_json_proxy_request_body(body_bytes.as_ref())
        .map_err(|error| ProxyError::Internal(error.to_string()))?;

    Ok(ParsedHttpJsonProxyRequest {
        method: parts.method,
        uri: parts.uri,
        headers: parts.headers,
        extensions: parts.extensions,
        body: parsed.body,
        is_stream: parsed.is_stream,
    })
}

pub(crate) async fn collect_json_or_null_proxy_request(
    request: axum::extract::Request,
) -> Result<ParsedHttpJsonProxyRequest, ProxyError> {
    let (parts, body) = request.into_parts();
    let body_bytes = collect_axum_request_body(body).await?;
    let parsed = parse_json_proxy_request_body_or_null(body_bytes.as_ref())
        .map_err(|error| ProxyError::Internal(error.to_string()))?;

    Ok(ParsedHttpJsonProxyRequest {
        method: parts.method,
        uri: parts.uri,
        headers: parts.headers,
        extensions: parts.extensions,
        body: parsed.body,
        is_stream: parsed.is_stream,
    })
}
