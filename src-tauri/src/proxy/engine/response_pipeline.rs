//! 响应处理器模块
//!
//! 统一处理流式和非流式 API 响应

use super::context::RequestContext;
use super::response_assembly::{
    claude_transformed_json_response_to_axum_response,
    claude_transformed_sse_response_to_axum_response,
    codex_chat_upstream_error_response_to_axum_response, codex_proxy_error_to_axum_response,
    codex_transformed_json_response_to_axum_response,
    codex_transformed_sse_response_to_axum_response, is_sse_response,
    log_non_streaming_proxy_response_body, log_streaming_proxy_response_received,
    proxy_core_response_to_axum_response, read_decoded_proxy_response_body,
};
#[cfg(test)]
use super::response_assembly::{
    codex_chat_error_response_to_axum_response, rebuilt_json_proxy_response_to_axum_response,
    transformed_sse_proxy_response_to_axum_response,
};
use super::response_stream::{
    create_claude_transformed_logged_stream, create_codex_auto_transformed_logged_stream,
    create_passthrough_logged_stream,
};
#[cfg(test)]
use super::response_usage::{
    forward_error_usage_record_from_response_context,
    non_streaming_response_usage_record_from_response_context, record_success_usage_for_test,
    response_usage_provider_facts, transformed_response_usage_record_from_response_context,
    ForwardErrorUsageContext, NonStreamingResponseUsageContext, TransformedResponseUsageContext,
};
use super::response_usage::{
    record_forward_core_error_usage, record_non_streaming_response_usage_from_context,
    record_transformed_response_usage_from_context, usage_logging_enabled_from_state,
    NonStreamingUsageRecordContext, TransformedResponseUsageRecordContext,
};
use crate::provider::Provider;
use crate::proxy::codex_chat_history::{
    transform_codex_chat_response_with_history, transform_codex_chat_sse_with_history,
};
use crate::proxy::engine::forward_pipeline::ActiveConnectionGuard;
use crate::proxy::host::cc_switch::provider_projection::{
    provider_claude_transform_streaming_decision,
    provider_codex_responses_to_chat_conversion_required, provider_needs_claude_transform,
};
#[cfg(test)]
use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy::provider::{
    transform_claude_response_for_api_format, transform_claude_sse_for_api_format,
};
use crate::proxy::{
    error::ProxyError,
    error_mapper::{
        claude_response_transform_error_to_proxy_error,
        codex_chat_to_responses_transform_error_to_proxy_error,
        parse_claude_transform_upstream_json_or_unlabeled_sse,
        parse_codex_chat_upstream_json_or_unlabeled_sse,
    },
    transport::upstream::{hyper_client::ProxyResponse, proxy_core_response_to_proxy_response},
};
#[cfg(test)]
use crate::proxy_core::api::domain::{AppKind, ProviderKind};
#[cfg(test)]
use crate::proxy_core::api::errors::selected_provider_not_applied_message;
use crate::proxy_core::api::transforms::{
    codex_chat_transform_streaming_decision, extract_anthropic_tool_schema_hints,
    AnthropicToolSchemaHints, ClaudeTransformStreamingDecision,
    CodexChatTransformStreamingDecision, CodexToolContext,
};
#[cfg(test)]
use crate::proxy_core::api::transport::ProxyResponseBuildFailureContext as CoreResponseBuildFailureContext;
use crate::proxy_core::api::transport::{
    passthrough_bytes_proxy_response, passthrough_stream_proxy_response, ProxyCoreResponse,
    ProxyRequest, ProxyResponseBuildErrorContext as AxumResponseBuildErrorContext, ProxyResult,
    UpstreamSseAggregationKind,
};
#[cfg(test)]
use crate::proxy_core::api::usage::usage_selected_provider_missing_log_message;
#[cfg(test)]
use crate::proxy_core::api::usage::TokenUsage;
use crate::proxy_core::api::usage::{
    TransformedResponseUsageFormat, UsageParserConfig, CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG,
    GEMINI_PARSER_CONFIG, OPENAI_PARSER_CONFIG,
};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures::Stream;
use http::HeaderMap;
use serde_json::Value;

// ============================================================================
// 公共接口
// ============================================================================

pub(crate) fn proxy_result_to_proxy_response(
    result: ProxyResult,
    ctx: &mut RequestContext,
    state: &ProxyState,
) -> Result<ProxyResponse, ProxyError> {
    ctx.apply_proxy_result(state.request_context_provider_source.as_ref(), &result)?;
    proxy_core_response_to_proxy_response(result.response)
}

pub(crate) fn claude_proxy_result_to_proxy_response(
    result: ProxyResult,
    ctx: &mut RequestContext,
    state: &ProxyState,
) -> Result<(ProxyResponse, String), ProxyError> {
    ctx.apply_proxy_result(state.request_context_provider_source.as_ref(), &result)?;
    let api_format = ctx.claude_api_format_for_proxy_result(&result)?;
    let response = proxy_core_response_to_proxy_response(result.response)?;
    Ok((response, api_format))
}

async fn dispatch_proxy_request(
    state: &ProxyState,
    ctx: &RequestContext,
    proxy_request: ProxyRequest,
    is_stream: bool,
) -> Result<ProxyResult, ProxyError> {
    state
        .proxy_engine()
        .handle(proxy_request)
        .await
        .map_err(|error| record_forward_core_error_usage(state, ctx, is_stream, error))
}

pub(crate) async fn dispatch_proxy_request_to_proxy_response(
    state: &ProxyState,
    ctx: &mut RequestContext,
    proxy_request: ProxyRequest,
    is_stream: bool,
) -> Result<ProxyResponse, ProxyError> {
    let result = dispatch_proxy_request(state, ctx, proxy_request, is_stream).await?;
    proxy_result_to_proxy_response(result, ctx, state)
}

pub(crate) async fn dispatch_claude_proxy_request_to_proxy_response(
    state: &ProxyState,
    ctx: &mut RequestContext,
    proxy_request: ProxyRequest,
    is_stream: bool,
) -> Result<(ProxyResponse, String), ProxyError> {
    let result = dispatch_proxy_request(state, ctx, proxy_request, is_stream).await?;
    claude_proxy_result_to_proxy_response(result, ctx, state)
}

pub(crate) enum CodexProxyDispatchResponse {
    ProxyResponse(ProxyResponse),
    ErrorResponse(axum::response::Response),
}

pub(crate) async fn dispatch_codex_proxy_request_to_proxy_response(
    state: &ProxyState,
    ctx: &mut RequestContext,
    proxy_request: ProxyRequest,
    endpoint: &str,
    is_stream: bool,
) -> Result<CodexProxyDispatchResponse, ProxyError> {
    let result = match dispatch_proxy_request(state, ctx, proxy_request, is_stream).await {
        Ok(result) => result,
        Err(error) => {
            let response = codex_proxy_error_to_axum_response(
                ctx.provider_name_for_error(),
                &ctx.request_model,
                endpoint,
                &error,
            )?;
            return Ok(CodexProxyDispatchResponse::ErrorResponse(response));
        }
    };
    proxy_result_to_proxy_response(result, ctx, state)
        .map(CodexProxyDispatchResponse::ProxyResponse)
}

pub(crate) async fn codex_chat_proxy_request_to_axum_response(
    state: &ProxyState,
    ctx: &mut RequestContext,
    proxy_request: ProxyRequest,
    endpoint: &str,
    is_stream: bool,
) -> Result<axum::response::Response, ProxyError> {
    let response = match dispatch_codex_proxy_request_to_proxy_response(
        state,
        ctx,
        proxy_request,
        endpoint,
        is_stream,
    )
    .await?
    {
        CodexProxyDispatchResponse::ProxyResponse(response) => response,
        CodexProxyDispatchResponse::ErrorResponse(response) => return Ok(response),
    };

    openai_chat_passthrough_response_to_axum_response(response, ctx, state).await
}

pub(crate) async fn codex_responses_proxy_request_to_axum_response(
    state: &ProxyState,
    ctx: &mut RequestContext,
    proxy_request: ProxyRequest,
    endpoint: &str,
    is_stream: bool,
    codex_tool_context: CodexToolContext,
) -> Result<axum::response::Response, ProxyError> {
    let response = match dispatch_codex_proxy_request_to_proxy_response(
        state,
        ctx,
        proxy_request,
        endpoint,
        is_stream,
    )
    .await?
    {
        CodexProxyDispatchResponse::ProxyResponse(response) => response,
        CodexProxyDispatchResponse::ErrorResponse(response) => return Ok(response),
    };

    if codex_response_needs_chat_transform(ctx, endpoint)? {
        return codex_chat_to_responses_transformed_response_to_axum_response(
            response,
            ctx,
            state,
            is_stream,
            None,
            codex_tool_context,
        )
        .await;
    }

    codex_passthrough_response_to_axum_response(response, ctx, state).await
}

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

pub(crate) async fn claude_passthrough_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
) -> Result<Response, ProxyError> {
    process_response(response, ctx, state, &CLAUDE_PARSER_CONFIG, None).await
}

pub(crate) async fn openai_chat_passthrough_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
) -> Result<Response, ProxyError> {
    process_response(response, ctx, state, &OPENAI_PARSER_CONFIG, None).await
}

pub(crate) async fn codex_passthrough_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
) -> Result<Response, ProxyError> {
    process_response(response, ctx, state, &CODEX_PARSER_CONFIG, None).await
}

pub(crate) async fn gemini_passthrough_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
) -> Result<Response, ProxyError> {
    process_response(response, ctx, state, &GEMINI_PARSER_CONFIG, None).await
}

pub(crate) fn claude_response_needs_transform(ctx: &RequestContext) -> Result<bool, ProxyError> {
    Ok(provider_needs_claude_transform(ctx.provider()?))
}

pub(crate) fn codex_response_needs_chat_transform(
    ctx: &RequestContext,
    endpoint: &str,
) -> Result<bool, ProxyError> {
    Ok(provider_codex_responses_to_chat_conversion_required(
        ctx.provider()?,
        endpoint,
    ))
}

pub(crate) fn claude_transform_streaming_decision_for_response(
    provider: &Provider,
    requested_streaming: bool,
    response_headers: &HeaderMap,
    api_format: &str,
) -> ClaudeTransformStreamingDecision {
    provider_claude_transform_streaming_decision(
        provider,
        requested_streaming,
        response_headers,
        api_format,
    )
}

pub(crate) fn codex_chat_transform_streaming_decision_for_response(
    requested_streaming: bool,
    response_headers: &HeaderMap,
) -> CodexChatTransformStreamingDecision {
    codex_chat_transform_streaming_decision(requested_streaming, response_headers)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn claude_transformed_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    original_body: &Value,
    is_stream: bool,
    api_format: &str,
    connection_guard: Option<ActiveConnectionGuard>,
) -> Result<axum::response::Response, ProxyError> {
    let status = response.status();
    let provider = ctx.provider()?;
    let streaming_decision = claude_transform_streaming_decision_for_response(
        provider,
        is_stream,
        response.headers(),
        api_format,
    );
    if streaming_decision.use_streaming {
        let stream = response.bytes_stream();
        let logged_stream = claude_transformed_sse_stream_from_context(
            stream,
            ClaudeTransformedSseStreamContext {
                state,
                ctx,
                provider,
                api_format,
                original_body,
                status_code: status.as_u16(),
                connection_guard,
            },
        );

        return claude_transformed_sse_response_to_axum_response(logged_stream);
    }

    claude_transformed_upstream_json_response_to_axum_response(
        response,
        ctx,
        state,
        provider,
        api_format,
        original_body,
        streaming_decision.response_sse_aggregation,
        streaming_decision.aggregate_codex_oauth_responses_sse,
    )
    .await
}

pub(crate) async fn codex_chat_to_responses_transformed_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    is_stream: bool,
    connection_guard: Option<ActiveConnectionGuard>,
    tool_context: CodexToolContext,
) -> Result<axum::response::Response, ProxyError> {
    let status = response.status();

    if !status.is_success() {
        return codex_chat_upstream_error_response_to_axum_response(response, ctx).await;
    }

    let streaming_decision =
        codex_chat_transform_streaming_decision_for_response(is_stream, response.headers());

    if streaming_decision.use_streaming {
        let stream = response.bytes_stream();
        let logged_stream = codex_auto_transformed_sse_stream_from_context(
            stream,
            CodexAutoTransformedSseStreamContext {
                state,
                ctx,
                tool_context,
                status_code: status.as_u16(),
                connection_guard,
            },
        );

        return codex_transformed_sse_response_to_axum_response(logged_stream);
    }

    let _connection_guard = connection_guard;
    codex_transformed_upstream_json_response_to_axum_response(
        response,
        ctx,
        state,
        &tool_context,
        streaming_decision.response_sse_aggregation,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn claude_transformed_upstream_json_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    provider: &Provider,
    api_format: &str,
    original_body: &Value,
    response_sse_aggregation: Option<UpstreamSseAggregationKind>,
    aggregate_codex_oauth_responses_sse: bool,
) -> Result<axum::response::Response, ProxyError> {
    let decoded =
        read_decoded_proxy_response_body(response, ctx.tag, ctx.body_timeout_duration()).await?;
    let response_headers = decoded.headers;
    let status = decoded.status;
    let body_bytes = decoded.body;

    let upstream_response = parse_claude_transform_upstream_json_or_unlabeled_sse(
        body_bytes.as_ref(),
        &response_headers,
        response_sse_aggregation,
        api_format,
        aggregate_codex_oauth_responses_sse,
    )?;

    let anthropic_response = claude_transformed_json_response_from_context(
        &upstream_response,
        ClaudeTransformedJsonResponseContext {
            state,
            ctx,
            provider,
            api_format,
            original_body,
            status_code: status.as_u16(),
        },
    )
    .map_err(claude_response_transform_error_to_proxy_error)?;

    claude_transformed_json_response_to_axum_response(status, response_headers, anthropic_response)
}

pub(crate) async fn codex_transformed_upstream_json_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    tool_context: &CodexToolContext,
    response_sse_aggregation: Option<UpstreamSseAggregationKind>,
) -> Result<axum::response::Response, ProxyError> {
    let decoded =
        read_decoded_proxy_response_body(response, ctx.tag, ctx.body_timeout_duration()).await?;
    let response_headers = decoded.headers;
    let status = decoded.status;
    let body_bytes = decoded.body;

    let chat_response = parse_codex_chat_upstream_json_or_unlabeled_sse(
        body_bytes.as_ref(),
        &response_headers,
        response_sse_aggregation,
    )?;
    let responses_response = codex_auto_transformed_json_response_from_context(
        &chat_response,
        CodexAutoTransformedJsonResponseContext {
            state,
            ctx,
            tool_context,
            status_code: status.as_u16(),
        },
    )
    .await
    .map_err(codex_chat_to_responses_transform_error_to_proxy_error)?;

    codex_transformed_json_response_to_axum_response(status, response_headers, responses_response)
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

pub(crate) fn claude_transform_tool_schema_hints(
    original_body: &Value,
) -> Option<AnthropicToolSchemaHints> {
    let tool_schema_hints = extract_anthropic_tool_schema_hints(original_body);
    (!tool_schema_hints.is_empty()).then_some(tool_schema_hints)
}

pub(crate) struct ClaudeTransformedSseStreamContext<'a, G> {
    pub(crate) state: &'a ProxyState,
    pub(crate) ctx: &'a RequestContext,
    pub(crate) provider: &'a Provider,
    pub(crate) api_format: &'a str,
    pub(crate) original_body: &'a Value,
    pub(crate) status_code: u16,
    pub(crate) connection_guard: Option<G>,
}

pub(crate) fn claude_transformed_sse_stream_from_context<G>(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    context: ClaudeTransformedSseStreamContext<'_, G>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    G: Send + 'static,
{
    let tool_schema_hints = claude_transform_tool_schema_hints(context.original_body);
    let sse_stream = transform_claude_sse_for_api_format(
        stream,
        context.api_format,
        Some(context.state.gemini_shadow.clone()),
        Some(context.provider.id.clone()),
        Some(context.ctx.session_id.clone()),
        tool_schema_hints,
    );

    create_claude_transformed_logged_stream(
        sse_stream,
        context.state,
        context.ctx,
        context.status_code,
        context.connection_guard,
    )
}

pub(crate) struct CodexAutoTransformedSseStreamContext<'a, G> {
    pub(crate) state: &'a ProxyState,
    pub(crate) ctx: &'a RequestContext,
    pub(crate) tool_context: CodexToolContext,
    pub(crate) status_code: u16,
    pub(crate) connection_guard: Option<G>,
}

pub(crate) fn codex_auto_transformed_sse_stream_from_context<G>(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    context: CodexAutoTransformedSseStreamContext<'_, G>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    G: Send + 'static,
{
    let sse_stream = transform_codex_chat_sse_with_history(
        stream,
        context.tool_context,
        context.state.codex_chat_history.clone(),
    );

    create_codex_auto_transformed_logged_stream(
        sse_stream,
        context.state,
        context.ctx,
        context.status_code,
        context.connection_guard,
    )
}

pub(crate) fn record_transformed_response_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    body: &Value,
    format: TransformedResponseUsageFormat,
    status_code: u16,
) {
    record_transformed_response_usage_from_context(TransformedResponseUsageRecordContext {
        usage_logging_enabled: usage_logging_enabled_from_state(state),
        services: state.proxy_core_services.clone(),
        body,
        format,
        provider: ctx.provider_for_usage(),
        tag: ctx.tag,
        app_type: ctx.app_type_str,
        request_model: &ctx.request_model,
        outbound_model: ctx.outbound_model.as_deref(),
        route_context: ctx.usage_route_context.as_ref(),
        latency_ms: ctx.latency_ms(),
        status_code,
        session_id: &ctx.session_id,
    });
}

pub(crate) fn record_claude_transformed_response_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    body: &Value,
    status_code: u16,
) {
    record_transformed_response_usage(
        state,
        ctx,
        body,
        TransformedResponseUsageFormat::Claude,
        status_code,
    );
}

pub(crate) fn record_codex_auto_transformed_response_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    body: &Value,
    status_code: u16,
) {
    record_transformed_response_usage(
        state,
        ctx,
        body,
        TransformedResponseUsageFormat::CodexAuto,
        status_code,
    );
}

pub(crate) struct ClaudeTransformedJsonResponseContext<'a> {
    pub(crate) state: &'a ProxyState,
    pub(crate) ctx: &'a RequestContext,
    pub(crate) provider: &'a Provider,
    pub(crate) api_format: &'a str,
    pub(crate) original_body: &'a Value,
    pub(crate) status_code: u16,
}

pub(crate) fn claude_transformed_json_response_from_context(
    upstream_response: &Value,
    context: ClaudeTransformedJsonResponseContext<'_>,
) -> Result<Value, String> {
    let tool_schema_hints = claude_transform_tool_schema_hints(context.original_body);
    let anthropic_response = transform_claude_response_for_api_format(
        upstream_response,
        context.api_format,
        Some(context.state.gemini_shadow.as_ref()),
        Some(&context.provider.id),
        Some(&context.ctx.session_id),
        tool_schema_hints.as_ref(),
    )?;

    record_claude_transformed_response_usage(
        context.state,
        context.ctx,
        &anthropic_response,
        context.status_code,
    );
    Ok(anthropic_response)
}

pub(crate) struct CodexAutoTransformedJsonResponseContext<'a> {
    pub(crate) state: &'a ProxyState,
    pub(crate) ctx: &'a RequestContext,
    pub(crate) tool_context: &'a CodexToolContext,
    pub(crate) status_code: u16,
}

pub(crate) async fn codex_auto_transformed_json_response_from_context(
    chat_response: &Value,
    context: CodexAutoTransformedJsonResponseContext<'_>,
) -> Result<Value, String> {
    let responses_response = transform_codex_chat_response_with_history(
        chat_response,
        context.tool_context,
        &context.state.codex_chat_history,
    )
    .await?;

    record_codex_auto_transformed_response_usage(
        context.state,
        context.ctx,
        &responses_response,
        context.status_code,
    );
    Ok(responses_response)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppType;
    use crate::database::Database;
    use crate::error::AppError;
    use crate::provider::ProviderMeta;
    use crate::proxy::codex_chat_history::CodexChatHistoryStore;
    use crate::proxy::engine::response_usage::{
        response_usage_provider_facts_from_optional,
        streaming_response_usage_record_from_response_context,
        transformed_streaming_response_usage_record_from_response_context,
        StreamingResponseUsageContext, TransformedStreamingResponseUsageContext,
    };
    use crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime;
    use crate::proxy::host::cc_switch::proxy_services::CcSwitchProxyServices as GenericCcSwitchProxyServices;
    use crate::proxy::host::cc_switch::request_context_provider_source::CcSwitchRequestContextProviderSource;
    use crate::proxy_core::api::ports::{ProxyConfig, ProxyRuntimeStatus};
    use crate::proxy_core::api::transforms::strip_sse_field;
    use crate::proxy_core::api::transforms::GeminiShadowStore;
    use crate::proxy_core::api::transport::{decompress_body, ProxyResponseBody};
    use crate::proxy_core::api::usage::{UsageRouteContext, UsageSelectedProviderMissingPhase};
    use http::StatusCode;
    use http_body_util::BodyExt;
    use rust_decimal::Decimal;
    use serde_json::json;
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    type CcSwitchProxyServices = GenericCcSwitchProxyServices<CcSwitchProxyRuntime>;

    #[tokio::test]
    async fn proxy_core_response_to_axum_response_preserves_buffered_body_and_headers() {
        let mut headers = http::HeaderMap::new();
        headers.insert("x-test", http::HeaderValue::from_static("yes"));
        let response = ProxyCoreResponse::with_body(
            StatusCode::CREATED,
            headers,
            ProxyResponseBody::bytes(Bytes::from_static(b"ok")),
        );

        let response = proxy_core_response_to_axum_response(
            response,
            AxumResponseBuildErrorContext::TaggedResponse { tag: "test" },
        )
        .expect("bridge");

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.headers().get("x-test"),
            Some(&http::HeaderValue::from_static("yes"))
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, Bytes::from_static(b"ok"));
    }

    #[tokio::test]
    async fn rebuilt_json_proxy_response_to_axum_response_rebuilds_json_headers() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/plain"),
        );
        headers.insert(
            http::header::CONTENT_ENCODING,
            http::HeaderValue::from_static("gzip"),
        );

        let response = rebuilt_json_proxy_response_to_axum_response(
            StatusCode::OK,
            headers,
            json!({"ok": true}),
            CoreResponseBuildFailureContext::ClaudeJson,
            AxumResponseBuildErrorContext::ClaudeResponse,
        )
        .expect("json response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("application/json"))
        );
        assert!(!response
            .headers()
            .contains_key(http::header::CONTENT_ENCODING));
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, Bytes::from_static(br#"{"ok":true}"#));
    }

    #[tokio::test]
    async fn transformed_sse_proxy_response_to_axum_response_sets_sse_headers() {
        let response = transformed_sse_proxy_response_to_axum_response(
            futures::stream::once(async { Ok(Bytes::from_static(b"data: {}\n\n")) }),
            AxumResponseBuildErrorContext::CodexSse,
        )
        .expect("sse response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/event-stream"))
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, Bytes::from_static(b"data: {}\n\n"));
    }

    #[tokio::test]
    async fn transformed_protocol_response_helpers_preserve_json_and_sse_shapes() {
        let response = claude_transformed_json_response_to_axum_response(
            StatusCode::OK,
            http::HeaderMap::new(),
            json!({"type": "message"}),
        )
        .expect("claude json response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("application/json"))
        );

        let response =
            codex_transformed_sse_response_to_axum_response(futures::stream::once(async {
                Ok(Bytes::from_static(b"data: {}\n\n"))
            }))
            .expect("codex sse response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/event-stream"))
        );
    }

    #[tokio::test]
    async fn codex_chat_error_response_helper_normalizes_and_bridges_error_body() {
        let response = codex_chat_error_response_to_axum_response(
            StatusCode::BAD_GATEWAY,
            http::HeaderMap::new(),
            br#"{"base_resp":{"status_code":2013,"status_msg":"bad role"}}"#,
        )
        .expect("codex chat error response");

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("application/json"))
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(value["error"]["message"], "bad role");
        assert_eq!(value["error"]["code"], 2013);
    }

    #[tokio::test]
    async fn codex_proxy_error_response_helper_maps_host_error_and_bridges_body() {
        let response = codex_proxy_error_to_axum_response(
            "DeepSeek",
            "deepseek-chat",
            "/responses",
            &ProxyError::AuthError("bad token".to_string()),
        )
        .expect("codex proxy error response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("application/json"))
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(value["error"]["code"], "cc_switch_auth_error");
        assert_eq!(value["error"]["provider"], "DeepSeek");
        assert_eq!(value["error"]["model"], "deepseek-chat");
        assert_eq!(value["error"]["endpoint"], "/responses");
    }

    #[test]
    fn decompress_body_deflate_handles_zlib_wrapped_per_rfc9110() {
        // RFC 9110 规范的 deflate = zlib 包裹格式（合规上游发的就是这个）
        let payload = br#"{"ok":true}"#;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let decompressed = decompress_body("deflate", &compressed).unwrap().unwrap();
        assert_eq!(decompressed, payload);
    }

    #[test]
    fn decompress_body_deflate_falls_back_to_raw_stream() {
        // 部分上游违规发 raw deflate 流，保持兼容
        let payload = br#"{"ok":true}"#;
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let decompressed = decompress_body("deflate", &compressed).unwrap().unwrap();
        assert_eq!(decompressed, payload);
    }

    #[test]
    fn decompress_body_unknown_encoding_returns_none_to_keep_headers() {
        // 未知编码必须返回 None（而非伪装成"已解码"），否则 content-encoding
        // 头被剥掉，下游诊断会把压缩字节误报成明文
        let result = decompress_body("zstd", b"\x28\xb5\x2f\xfd").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_strip_sse_field_accepts_optional_space() {
        assert_eq!(
            strip_sse_field("data: {\"ok\":true}", "data"),
            Some("{\"ok\":true}")
        );
        assert_eq!(
            strip_sse_field("data:{\"ok\":true}", "data"),
            Some("{\"ok\":true}")
        );
        assert_eq!(
            strip_sse_field("event: message_start", "event"),
            Some("message_start")
        );
        assert_eq!(
            strip_sse_field("event:message_start", "event"),
            Some("message_start")
        );
        assert_eq!(strip_sse_field("id:1", "data"), None);
    }

    #[test]
    fn response_usage_helpers_project_provider_and_app_facts() {
        let mut provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..ProviderMeta::default()
        });

        let facts = response_usage_provider_facts(&provider, AppType::ClaudeDesktop.as_str());
        assert_eq!(facts.provider_id, "provider-a");
        assert_eq!(facts.provider_kind, Some(ProviderKind::GitHubCopilot));
        assert_eq!(facts.app, AppKind::ClaudeDesktop);

        let optional_facts = response_usage_provider_facts_from_optional(
            Some(&provider),
            AppType::ClaudeDesktop.as_str(),
            "Claude Desktop",
            UsageSelectedProviderMissingPhase::StreamingPassthrough,
        )
        .expect("provider facts");
        assert_eq!(optional_facts.provider_id, "provider-a");

        let missing_provider = response_usage_provider_facts_from_optional(
            None,
            AppType::ClaudeDesktop.as_str(),
            "Claude Desktop",
            UsageSelectedProviderMissingPhase::StreamingPassthrough,
        )
        .unwrap_err();
        assert_eq!(
            missing_provider,
            usage_selected_provider_missing_log_message(
                "Claude Desktop",
                UsageSelectedProviderMissingPhase::StreamingPassthrough
            )
        );

        fn parsed_stream_usage(_events: &[Value]) -> Option<TokenUsage> {
            Some(TokenUsage {
                input_tokens: 4,
                output_tokens: 6,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                model: None,
                message_id: None,
            })
        }

        fn extracted_stream_model(events: &[Value], fallback: &str) -> String {
            events
                .iter()
                .find_map(|event| event.get("model").and_then(Value::as_str))
                .unwrap_or(fallback)
                .to_string()
        }

        let route_context = UsageRouteContext {
            channel_id: "channel-1".to_string(),
            channel_name: "Channel One".to_string(),
            route_group: "beta".to_string(),
            pricing_model: Some("route-price-model".to_string()),
        };
        let error_record = forward_error_usage_record_from_response_context(
            ForwardErrorUsageContext {
                provider: Some(&provider),
                fallback_provider_id: "fallback-provider",
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                status_code: 502,
                error_message: "upstream failed".to_string(),
                latency_ms: 321,
                is_streaming: false,
                session_id: "session-error",
            },
            || "request-error".to_string(),
        );
        assert_eq!(error_record.provider_id, "provider-a");
        assert_eq!(
            error_record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(error_record.app, AppKind::ClaudeDesktop);
        assert_eq!(error_record.request_model, "request-model");
        assert_eq!(error_record.outbound_model, "outbound-model");
        assert_eq!(error_record.status_code, 502);
        assert_eq!(
            error_record.error_message.as_deref(),
            Some("upstream failed")
        );
        assert_eq!(error_record.tokens.input_tokens, 0);
        assert_eq!(error_record.channel_id.as_deref(), Some("channel-1"));

        let transformed_body = json!({
            "id": "msg_1",
            "model": "claude-response-model",
            "usage": {
                "input_tokens": 3,
                "output_tokens": 5
            }
        });
        let transformed_record = transformed_response_usage_record_from_response_context(
            TransformedResponseUsageContext {
                body: &transformed_body,
                format: TransformedResponseUsageFormat::Claude,
                provider: Some(&provider),
                tag: "Claude Desktop",
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                latency_ms: 123,
                status_code: 200,
                session_id: "session-transformed",
            },
            || "request-transformed".to_string(),
        )
        .expect("transformed provider facts")
        .expect("transformed usage record");
        assert_eq!(transformed_record.provider_id, "provider-a");
        assert_eq!(
            transformed_record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(transformed_record.app, AppKind::ClaudeDesktop);
        assert_eq!(
            transformed_record.response_model.as_deref(),
            Some("claude-response-model")
        );
        assert_eq!(transformed_record.tokens.input_tokens, 3);
        assert!(!transformed_record.is_streaming);
        assert_eq!(
            transformed_record.channel_name.as_deref(),
            Some("Channel One")
        );

        let missing_transformed = transformed_response_usage_record_from_response_context(
            TransformedResponseUsageContext {
                body: &transformed_body,
                format: TransformedResponseUsageFormat::Claude,
                provider: None,
                tag: "Claude Desktop",
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: None,
                route_context: None,
                latency_ms: 123,
                status_code: 200,
                session_id: "session-transformed",
            },
            || "request-missing".to_string(),
        )
        .unwrap_err();
        assert_eq!(
            missing_transformed,
            usage_selected_provider_missing_log_message(
                "Claude Desktop",
                UsageSelectedProviderMissingPhase::TransformedResponse
            )
        );

        let stream_events = vec![json!({"model": "stream-response-model"})];
        let stream_output = streaming_response_usage_record_from_response_context(
            StreamingResponseUsageContext {
                events: &stream_events,
                stream_parser: parsed_stream_usage,
                model_extractor: extracted_stream_model,
                provider_facts: &optional_facts,
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                latency_ms: 456,
                first_token_ms: Some(12),
                status_code: 200,
                session_id: "session-stream",
            },
            || "request-stream".to_string(),
        );
        assert_eq!(stream_output.record.provider_id, "provider-a");
        assert_eq!(
            stream_output.record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(stream_output.record.app, AppKind::ClaudeDesktop);
        assert_eq!(
            stream_output.record.response_model.as_deref(),
            Some("stream-response-model")
        );
        assert_eq!(stream_output.record.outbound_model, "outbound-model");
        assert_eq!(stream_output.record.tokens.input_tokens, 4);
        assert_eq!(stream_output.record.tokens.output_tokens, 6);
        assert!(stream_output.record.is_streaming);
        assert_eq!(stream_output.record.first_token_ms, Some(12));
        assert_eq!(
            stream_output.record.channel_id.as_deref(),
            Some("channel-1")
        );
        assert_eq!(
            stream_output.record.channel_name.as_deref(),
            Some("Channel One")
        );
        assert_eq!(stream_output.record.route_group.as_deref(), Some("beta"));

        let transformed_stream_events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "id": "msg_stream_1",
                    "model": "claude-stream-model",
                    "usage": {
                        "input_tokens": 7
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "output_tokens": 11
                }
            }),
        ];
        let transformed_stream_record =
            transformed_streaming_response_usage_record_from_response_context(
                TransformedStreamingResponseUsageContext {
                    events: &transformed_stream_events,
                    format: TransformedResponseUsageFormat::Claude,
                    provider_facts: &optional_facts,
                    request_model: "request-model",
                    outbound_model: Some("outbound-model"),
                    route_context: Some(&route_context),
                    latency_ms: 654,
                    first_token_ms: Some(34),
                    status_code: 200,
                    session_id: "session-transformed-stream",
                },
                || "request-transformed-stream".to_string(),
            )
            .expect("transformed streaming usage record");
        assert_eq!(transformed_stream_record.provider_id, "provider-a");
        assert_eq!(
            transformed_stream_record.response_model.as_deref(),
            Some("claude-stream-model")
        );
        assert_eq!(transformed_stream_record.tokens.input_tokens, 7);
        assert_eq!(transformed_stream_record.tokens.output_tokens, 11);
        assert_eq!(transformed_stream_record.first_token_ms, Some(34));
        assert!(transformed_stream_record.is_streaming);
        assert_eq!(
            transformed_stream_record.route_group.as_deref(),
            Some("beta")
        );

        let response_body =
            br#"{"model":"response-model","usage":{"prompt_tokens":2,"completion_tokens":3}}"#;
        let output = non_streaming_response_usage_record_from_response_context(
            NonStreamingResponseUsageContext {
                body: response_body,
                response_parser: TokenUsage::from_openai_response,
                provider: Some(&provider),
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                latency_ms: 123,
                status_code: 200,
                session_id: "session-1",
            },
            || "request-1".to_string(),
        )
        .expect("non-streaming usage record");

        assert!(output.usage_found);
        assert_eq!(output.record.provider_id, "provider-a");
        assert_eq!(
            output.record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(output.record.app, AppKind::ClaudeDesktop);
        assert_eq!(
            output.record.response_model.as_deref(),
            Some("response-model")
        );
        assert_eq!(output.record.outbound_model, "outbound-model");
        assert_eq!(output.record.tokens.input_tokens, 2);
        assert_eq!(output.record.tokens.output_tokens, 3);
        assert_eq!(output.record.channel_id.as_deref(), Some("channel-1"));
        assert_eq!(output.record.channel_name.as_deref(), Some("Channel One"));
        assert_eq!(output.record.route_group.as_deref(), Some("beta"));

        let missing = non_streaming_response_usage_record_from_response_context(
            NonStreamingResponseUsageContext {
                body: b"{}",
                response_parser: TokenUsage::from_openai_response,
                provider: None,
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: None,
                route_context: None,
                latency_ms: 123,
                status_code: 200,
                session_id: "session-1",
            },
            || "request-2".to_string(),
        )
        .unwrap_err();
        assert_eq!(
            missing,
            selected_provider_not_applied_message(AppType::ClaudeDesktop.as_str())
        );
    }

    fn build_state(db: Arc<Database>) -> ProxyState {
        ProxyState {
            config: Arc::new(RwLock::new(ProxyConfig::default())),
            status: Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            start_time: Arc::new(RwLock::new(None)),
            current_providers: Arc::new(RwLock::new(HashMap::new())),
            provider_router: Arc::new(provider_router_from_database(db.clone())),
            proxy_core_services: Arc::new(CcSwitchProxyServices::new(db.clone())),
            request_context_provider_source: Arc::new(CcSwitchRequestContextProviderSource::new(
                db.clone(),
            )),
            gemini_shadow: Arc::new(GeminiShadowStore::default()),
            codex_chat_history: Arc::new(CodexChatHistoryStore::default()),
            events: Arc::new(crate::proxy::events::ProxyEventBus::default()),
        }
    }

    fn seed_pricing(db: &Database) -> Result<(), AppError> {
        let conn = crate::database::lock_conn!(db.conn);
        conn.execute(
            "INSERT OR REPLACE INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["resp-model", "Resp Model", "1.0", "0"],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["req-model", "Req Model", "2.0", "0"],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    fn insert_provider(
        db: &Database,
        id: &str,
        app_type: &str,
        meta: ProviderMeta,
    ) -> Result<(), AppError> {
        let meta_json =
            serde_json::to_string(&meta).map_err(|e| AppError::Database(e.to_string()))?;
        let conn = crate::database::lock_conn!(db.conn);
        conn.execute(
            "INSERT INTO providers (id, app_type, name, settings_config, meta)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, app_type, "Test Provider", "{}", meta_json],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    #[tokio::test]
    async fn test_log_usage_uses_provider_override_config() -> Result<(), AppError> {
        let db = Arc::new(Database::memory()?);
        let app_type = "claude";

        db.set_default_cost_multiplier(app_type, "1.5").await?;
        db.set_pricing_model_source(app_type, "response").await?;
        seed_pricing(&db)?;

        let meta = ProviderMeta {
            cost_multiplier: Some("2".to_string()),
            pricing_model_source: Some("request".to_string()),
            ..ProviderMeta::default()
        };
        insert_provider(&db, "provider-1", app_type, meta)?;

        let state = build_state(db.clone());
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            model: None,
            message_id: None,
        };

        record_success_usage_for_test(
            state.proxy_core_services.as_ref(),
            "provider-1",
            None,
            app_type,
            "resp-model",
            "req-model",
            "req-model",
            usage,
            10,
            None,
            false,
            200,
            None,
        )
        .await;

        let conn = crate::database::lock_conn!(db.conn);
        let (model, request_model, total_cost, cost_multiplier): (String, String, String, String) =
            conn.query_row(
                "SELECT model, request_model, total_cost_usd, cost_multiplier
                 FROM proxy_request_logs WHERE provider_id = ?1",
                ["provider-1"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        assert_eq!(model, "resp-model");
        assert_eq!(request_model, "req-model");
        assert_eq!(
            Decimal::from_str(&cost_multiplier).unwrap(),
            Decimal::from_str("2").unwrap()
        );
        assert_eq!(
            Decimal::from_str(&total_cost).unwrap(),
            Decimal::from_str("4").unwrap()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_request_pricing_mode_anchors_to_outbound_model() -> Result<(), AppError> {
        let db = Arc::new(Database::memory()?);
        let app_type = "claude";

        db.set_pricing_model_source(app_type, "request").await?;
        seed_pricing(&db)?;
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute(
                "INSERT OR REPLACE INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million)
                 VALUES ('outbound-model', 'Outbound Model', '4.0', '0')",
                [],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        insert_provider(&db, "provider-3", app_type, ProviderMeta::default())?;

        let state = build_state(db.clone());
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            model: None,
            message_id: None,
        };

        // 路由接管场景：客户端请求 req-model（$2/M），代理实际发出 outbound-model
        // （$4/M），上游回显 resp-model。「按请求计价」必须锚定实际发出的模型。
        record_success_usage_for_test(
            state.proxy_core_services.as_ref(),
            "provider-3",
            None,
            app_type,
            "resp-model",
            "req-model",
            "outbound-model",
            usage,
            10,
            None,
            false,
            200,
            None,
        )
        .await;

        let conn = crate::database::lock_conn!(db.conn);
        let (model, request_model, total_cost): (String, String, String) = conn
            .query_row(
                "SELECT model, request_model, total_cost_usd
                 FROM proxy_request_logs WHERE provider_id = ?1",
                ["provider-3"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        // model / request_model 列不受计价锚点影响
        assert_eq!(model, "resp-model");
        assert_eq!(request_model, "req-model");
        // 按 outbound-model（$4/M）计价，而不是 req-model（$2/M）或 resp-model（$1/M）
        assert_eq!(
            Decimal::from_str(&total_cost).unwrap(),
            Decimal::from_str("4").unwrap()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_claude_desktop_inherits_claude_global_defaults() -> Result<(), AppError> {
        use crate::proxy::host::cc_switch::database_usage_sink::UsageLogger;

        let db = Arc::new(Database::memory()?);

        // 全局计费配置只有 claude/codex/gemini 三行；claude-desktop 的
        // 全局默认必须继承 claude，而不是静默落回工厂默认（1 / response）
        db.set_default_cost_multiplier("claude", "1.5").await?;
        db.set_pricing_model_source("claude", "request").await?;

        let logger = UsageLogger::new(&db);
        let (multiplier, source) = logger
            .resolve_pricing_config("nonexistent-provider", "claude-desktop")
            .await;

        assert_eq!(multiplier, Decimal::from_str("1.5").unwrap());
        assert_eq!(source, "request");
        Ok(())
    }

    #[tokio::test]
    async fn test_log_usage_falls_back_to_global_defaults() -> Result<(), AppError> {
        let db = Arc::new(Database::memory()?);
        let app_type = "claude";

        db.set_default_cost_multiplier(app_type, "1.5").await?;
        db.set_pricing_model_source(app_type, "response").await?;
        seed_pricing(&db)?;

        let meta = ProviderMeta::default();
        insert_provider(&db, "provider-2", app_type, meta)?;

        let state = build_state(db.clone());
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            model: None,
            message_id: None,
        };

        record_success_usage_for_test(
            state.proxy_core_services.as_ref(),
            "provider-2",
            None,
            app_type,
            "resp-model",
            "req-model",
            "req-model",
            usage,
            10,
            None,
            false,
            200,
            None,
        )
        .await;

        let conn = crate::database::lock_conn!(db.conn);
        let (total_cost, cost_multiplier): (String, String) = conn
            .query_row(
                "SELECT total_cost_usd, cost_multiplier
                 FROM proxy_request_logs WHERE provider_id = ?1",
                ["provider-2"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        assert_eq!(
            Decimal::from_str(&cost_multiplier).unwrap(),
            Decimal::from_str("1.5").unwrap()
        );
        assert_eq!(
            Decimal::from_str(&total_cost).unwrap(),
            Decimal::from_str("1.5").unwrap()
        );
        Ok(())
    }
}
