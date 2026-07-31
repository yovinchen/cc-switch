//! Passthrough response processing and usage wiring.

use super::context::RequestContext;
use super::response_assembly::{
    is_sse_response, log_non_streaming_proxy_response_body, log_streaming_proxy_response_received,
    proxy_core_response_to_axum_response, read_decoded_proxy_response_body,
};
use super::response_stream::create_passthrough_logged_stream;
use super::response_usage::{
    record_non_streaming_response_usage_from_context, usage_logging_enabled_from_state,
    NonStreamingUsageRecordContext,
};
use crate::proxy::engine::forward_pipeline::ActiveConnectionGuard;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy::{error::ProxyError, transport::upstream::hyper_client::ProxyResponse};
use crate::proxy_core::api::transport::{
    passthrough_bytes_proxy_response, passthrough_stream_proxy_response, ProxyCoreResponse,
    ProxyResponseBuildErrorContext as AxumResponseBuildErrorContext,
};
use crate::proxy_core::api::usage::UsageParserConfig;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures::Stream;
use http::HeaderMap;

/// 处理流式响应
pub async fn handle_streaming(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    parser_config: &UsageParserConfig,
    connection_guard: Option<ActiveConnectionGuard>,
) -> Response {
    let status = response.status();
    let response_headers = response.headers().clone();
    let stream = response.bytes_stream();

    let response = passthrough_stream_proxy_response_from_context(
        status,
        response_headers,
        stream,
        state,
        ctx,
        parser_config,
        connection_guard,
    );
    match proxy_core_response_to_axum_response(
        response,
        AxumResponseBuildErrorContext::TaggedStreaming { tag: ctx.tag },
    ) {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}

/// 处理非流式响应
pub async fn handle_non_streaming(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    parser_config: &UsageParserConfig,
    // guard 在函数 scope 内持有，整包响应读取完成后随函数返回一并 drop
    _connection_guard: Option<ActiveConnectionGuard>,
) -> Result<Response, ProxyError> {
    let decoded =
        read_decoded_proxy_response_body(response, ctx.tag, ctx.body_timeout_duration()).await?;
    let response_headers = decoded.headers;
    let status = decoded.status;
    let body_bytes = decoded.body;

    let response = passthrough_non_stream_proxy_response_from_context(
        status,
        response_headers,
        body_bytes,
        state,
        ctx,
        parser_config,
    )
    .map_err(ProxyError::ConfigError)?;
    proxy_core_response_to_axum_response(
        response,
        AxumResponseBuildErrorContext::TaggedResponse { tag: ctx.tag },
    )
}

/// 通用响应处理入口
///
/// 根据响应类型自动选择流式或非流式处理
pub async fn process_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    parser_config: &UsageParserConfig,
    connection_guard: Option<ActiveConnectionGuard>,
) -> Result<Response, ProxyError> {
    if is_sse_response(&response) {
        Ok(handle_streaming(response, ctx, state, parser_config, connection_guard).await)
    } else {
        handle_non_streaming(response, ctx, state, parser_config, connection_guard).await
    }
}

pub(crate) fn passthrough_stream_proxy_response_from_context<G>(
    status: http::StatusCode,
    headers: HeaderMap,
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    state: &ProxyState,
    ctx: &RequestContext,
    parser_config: &UsageParserConfig,
    connection_guard: Option<G>,
) -> ProxyCoreResponse
where
    G: Send + 'static,
{
    log_streaming_proxy_response_received(&headers, status, ctx.tag);
    let logged_stream = create_passthrough_logged_stream(
        stream,
        state,
        ctx,
        status.as_u16(),
        parser_config,
        connection_guard,
    );
    passthrough_stream_proxy_response(status, headers, logged_stream)
}

pub(crate) fn record_non_streaming_response_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    body: &[u8],
    parser_config: &UsageParserConfig,
    status_code: u16,
) -> Result<(), String> {
    record_non_streaming_response_usage_from_context(NonStreamingUsageRecordContext {
        usage_logging_enabled: usage_logging_enabled_from_state(state),
        services: state.proxy_core_services.clone(),
        body,
        parser_config,
        provider: ctx.provider_for_usage(),
        app_type: ctx.app_type_str,
        tag: ctx.tag,
        request_model: &ctx.request_model,
        outbound_model: ctx.outbound_model.as_deref(),
        route_context: ctx.usage_route_context.as_ref(),
        latency_ms: ctx.latency_ms(),
        status_code,
        session_id: &ctx.session_id,
    })
}

pub(crate) fn passthrough_non_stream_proxy_response_from_context(
    status: http::StatusCode,
    headers: HeaderMap,
    body: Bytes,
    state: &ProxyState,
    ctx: &RequestContext,
    parser_config: &UsageParserConfig,
) -> Result<ProxyCoreResponse, String> {
    log_non_streaming_proxy_response_body(&body, ctx.tag);
    record_non_streaming_response_usage(state, ctx, &body, parser_config, status.as_u16())?;
    Ok(passthrough_bytes_proxy_response(status, headers, body))
}
