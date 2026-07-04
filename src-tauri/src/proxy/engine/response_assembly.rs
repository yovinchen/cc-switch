//! Response body decoding and Axum response assembly.

use super::context::RequestContext;
use crate::proxy::{
    error::ProxyError,
    error_mapper::{
        codex_proxy_error_body_build_error_to_proxy_error, codex_proxy_error_response,
        codex_responses_error_body_build_error_to_proxy_error, response_build_error_to_proxy_error,
    },
    transport::upstream::hyper_client::ProxyResponse,
};
use crate::proxy_core::api::transforms::codex_chat_error_proxy_response;
use crate::proxy_core::api::transport::{
    decode_response_body, non_streaming_body_timeout_message,
    non_streaming_response_body_log_event, non_streaming_response_received_log_event,
    rebuilt_json_proxy_response, response_headers_indicate_sse,
    streaming_response_received_log_events, transformed_sse_proxy_response, ProxyCoreResponse,
    ProxyResponseBuildErrorContext as AxumResponseBuildErrorContext,
    ProxyResponseBuildFailureContext as CoreResponseBuildFailureContext, ProxyTransportResponse,
    ProxyTransportResponseBody, ResponseBodyDecodeLogLevel, ResponseLogEvent, ResponseLogLevel,
};
use bytes::Bytes;
use futures::Stream;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

pub(crate) struct DecodedProxyResponseBody {
    pub(crate) headers: HeaderMap,
    pub(crate) status: http::StatusCode,
    pub(crate) body: Bytes,
}

pub(crate) fn emit_response_log_event(event: ResponseLogEvent) {
    match event.level {
        ResponseLogLevel::Debug => log::debug!("{}", event.message),
        ResponseLogLevel::Warn => log::warn!("{}", event.message),
    }
}

pub(crate) fn decode_raw_proxy_response_body(
    mut headers: HeaderMap,
    status: http::StatusCode,
    raw_bytes: Bytes,
    tag: &str,
) -> DecodedProxyResponseBody {
    emit_response_log_event(non_streaming_response_received_log_event(
        tag,
        status,
        raw_bytes.len(),
        &headers,
    ));

    let decoded = decode_response_body(&mut headers, &raw_bytes);
    if let Some(event) = decoded.status.log_event() {
        match event.level() {
            ResponseBodyDecodeLogLevel::Debug => log::debug!("{}", event.message(tag)),
            ResponseBodyDecodeLogLevel::Warn => log::warn!("{}", event.message(tag)),
        }
    }

    DecodedProxyResponseBody {
        headers,
        status,
        body: Bytes::from(decoded.body),
    }
}

/// Read a non-streaming response body and decompress it when needed so headers
/// and returned body stay aligned.
pub(crate) async fn read_decoded_proxy_response_body(
    response: ProxyResponse,
    tag: &str,
    body_timeout: std::time::Duration,
) -> Result<DecodedProxyResponseBody, ProxyError> {
    let headers = response.headers().clone();
    let status = response.status();
    let raw_bytes = if body_timeout.is_zero() {
        response.bytes().await?
    } else {
        tokio::time::timeout(body_timeout, response.bytes())
            .await
            .map_err(|_| ProxyError::Timeout(non_streaming_body_timeout_message(body_timeout)))??
    };

    Ok(decode_raw_proxy_response_body(
        headers, status, raw_bytes, tag,
    ))
}

pub(crate) fn log_streaming_proxy_response_received(
    headers: &HeaderMap,
    status: http::StatusCode,
    tag: &str,
) {
    for event in streaming_response_received_log_events(tag, status, headers) {
        emit_response_log_event(event);
    }
}

pub(crate) fn log_non_streaming_proxy_response_body(body: &[u8], tag: &str) {
    emit_response_log_event(non_streaming_response_body_log_event(tag, body));
}

/// 检测响应是否为 SSE 流式响应
#[inline]
pub fn is_sse_response(response: &ProxyResponse) -> bool {
    response_headers_indicate_sse(response.headers())
}

pub(crate) fn proxy_core_response_to_axum_response(
    response: ProxyCoreResponse,
    build_error_context: AxumResponseBuildErrorContext<'_>,
) -> Result<axum::response::Response, ProxyError> {
    let response = response
        .into_transport_response()
        .map_err(ProxyError::Internal)?;
    let ProxyTransportResponse {
        status,
        headers,
        body,
    } = response;
    let body = match body {
        ProxyTransportResponseBody::Empty => axum::body::Body::from(Bytes::new()),
        ProxyTransportResponseBody::Bytes(body) => axum::body::Body::from(body),
        ProxyTransportResponseBody::Stream(stream) => axum::body::Body::from_stream(stream),
    };

    let mut builder = axum::response::Response::builder().status(status);
    for (key, value) in headers.iter() {
        builder = builder.header(key, value);
    }

    let build_error_message = build_error_context.internal_error_prefix();
    let build_error_context_message = build_error_context.message();
    builder.body(body).map_err(|error| {
        log::error!("{build_error_context_message}: {error}");
        ProxyError::Internal(format!("{build_error_message}: {error}"))
    })
}

pub(crate) fn rebuilt_json_proxy_response_to_axum_response(
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
    response_build_error_context: CoreResponseBuildFailureContext,
    axum_build_error_context: AxumResponseBuildErrorContext<'_>,
) -> Result<axum::response::Response, ProxyError> {
    let response = rebuilt_json_proxy_response(status, headers, body).map_err(|error| {
        response_build_error_to_proxy_error(response_build_error_context, error)
    })?;
    proxy_core_response_to_axum_response(response, axum_build_error_context)
}

pub(crate) fn transformed_sse_proxy_response_to_axum_response(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    build_error_context: AxumResponseBuildErrorContext<'_>,
) -> Result<axum::response::Response, ProxyError> {
    proxy_core_response_to_axum_response(
        transformed_sse_proxy_response(stream),
        build_error_context,
    )
}

pub(crate) fn claude_transformed_sse_response_to_axum_response(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
) -> Result<axum::response::Response, ProxyError> {
    transformed_sse_proxy_response_to_axum_response(
        stream,
        AxumResponseBuildErrorContext::ClaudeSse,
    )
}

pub(crate) fn codex_transformed_sse_response_to_axum_response(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
) -> Result<axum::response::Response, ProxyError> {
    transformed_sse_proxy_response_to_axum_response(stream, AxumResponseBuildErrorContext::CodexSse)
}

pub(crate) fn claude_transformed_json_response_to_axum_response(
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
) -> Result<axum::response::Response, ProxyError> {
    rebuilt_json_proxy_response_to_axum_response(
        status,
        headers,
        body,
        CoreResponseBuildFailureContext::ClaudeJson,
        AxumResponseBuildErrorContext::ClaudeResponse,
    )
}

pub(crate) fn codex_transformed_json_response_to_axum_response(
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
) -> Result<axum::response::Response, ProxyError> {
    rebuilt_json_proxy_response_to_axum_response(
        status,
        headers,
        body,
        CoreResponseBuildFailureContext::CodexResponses,
        AxumResponseBuildErrorContext::CodexResponses,
    )
}

pub(crate) fn codex_chat_error_response_to_axum_response(
    status: StatusCode,
    response_headers: HeaderMap,
    body_bytes: &[u8],
) -> Result<axum::response::Response, ProxyError> {
    let response = codex_chat_error_proxy_response(status, response_headers, body_bytes)
        .map_err(codex_responses_error_body_build_error_to_proxy_error)?;
    if let Some(message) = response.normalization.non_json_body_log_message() {
        log::warn!("{message}");
    }
    proxy_core_response_to_axum_response(
        response.response,
        AxumResponseBuildErrorContext::CodexResponsesError,
    )
}

pub(crate) async fn codex_chat_upstream_error_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
) -> Result<axum::response::Response, ProxyError> {
    let decoded =
        read_decoded_proxy_response_body(response, ctx.tag, ctx.body_timeout_duration()).await?;
    codex_chat_error_response_to_axum_response(decoded.status, decoded.headers, &decoded.body)
}

pub(crate) fn codex_proxy_error_to_axum_response(
    provider_name: &str,
    request_model: &str,
    endpoint: &str,
    error: &ProxyError,
) -> Result<axum::response::Response, ProxyError> {
    let response = codex_proxy_error_response(provider_name, request_model, endpoint, error)
        .map_err(codex_proxy_error_body_build_error_to_proxy_error)?;
    proxy_core_response_to_axum_response(response, AxumResponseBuildErrorContext::CodexProxyError)
}
