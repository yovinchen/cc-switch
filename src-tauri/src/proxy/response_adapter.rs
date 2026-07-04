use super::{
    engine::context::RequestContext,
    engine::response_pipeline::{
        claude_transformed_json_response_from_context,
        claude_transformed_json_response_to_axum_response,
        claude_transformed_sse_response_to_axum_response,
        claude_transformed_sse_stream_from_context,
        codex_auto_transformed_json_response_from_context,
        codex_auto_transformed_sse_stream_from_context,
        codex_chat_upstream_error_response_to_axum_response, codex_proxy_error_to_axum_response,
        codex_transformed_json_response_to_axum_response,
        codex_transformed_sse_response_to_axum_response, process_response,
        read_decoded_proxy_response_body, record_forward_core_error_usage,
        ClaudeTransformedJsonResponseContext, ClaudeTransformedSseStreamContext,
        CodexAutoTransformedJsonResponseContext, CodexAutoTransformedSseStreamContext,
    },
    error::ProxyError,
    error_mapper::{
        claude_response_transform_error_to_proxy_error,
        codex_chat_to_responses_transform_error_to_proxy_error,
        parse_claude_transform_upstream_json_or_unlabeled_sse,
        parse_codex_chat_upstream_json_or_unlabeled_sse,
    },
    transport::{
        http::request_body::{
            collect_json_or_null_proxy_request, collect_json_proxy_request, endpoint_from_uri,
        },
        upstream::hyper_client::ProxyResponse,
        upstream::proxy_core_response_to_proxy_response,
    },
};
use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::engine::forward_pipeline::ActiveConnectionGuard;
use crate::proxy::host::cc_switch::provider_projection::{
    provider_claude_transform_streaming_decision,
    provider_codex_responses_to_chat_conversion_required, provider_needs_claude_transform,
};
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy_core::api::transforms::{
    codex_chat_transform_streaming_decision, ClaudeTransformStreamingDecision,
    CodexChatTransformStreamingDecision, CodexToolContext,
};
use crate::proxy_core::api::transport::{ProxyRequest, ProxyResult, UpstreamSseAggregationKind};
use crate::proxy_core::api::usage::{
    CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG, OPENAI_PARSER_CONFIG,
};
use http::{HeaderMap, Uri};
use serde_json::Value;

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

pub(crate) async fn dispatch_gemini_request_to_axum_response(
    state: &ProxyState,
    uri: Uri,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    let parsed_request = collect_json_or_null_proxy_request(request).await?;
    let is_stream = parsed_request.is_stream;

    let mut ctx = parsed_request.gemini_request_context(state, &uri).await?;
    let endpoint = endpoint_from_uri(&uri);
    let proxy_request = parsed_request.into_gemini_proxy_request(endpoint);

    let response =
        dispatch_proxy_request_to_proxy_response(state, &mut ctx, proxy_request, is_stream).await?;

    gemini_passthrough_response_to_axum_response(response, &ctx, state).await
}

pub(crate) async fn dispatch_claude_request_to_axum_response(
    state: &ProxyState,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    dispatch_claude_messages_request_to_axum_response(
        state,
        request,
        AppType::Claude,
        "Claude",
        "claude",
        None,
    )
    .await
}

pub(crate) async fn dispatch_claude_desktop_messages_request_to_axum_response(
    state: &ProxyState,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    dispatch_claude_messages_request_to_axum_response(
        state,
        request,
        AppType::ClaudeDesktop,
        "Claude Desktop",
        "claude-desktop",
        Some("/claude-desktop"),
    )
    .await
}

async fn dispatch_claude_messages_request_to_axum_response(
    state: &ProxyState,
    request: axum::extract::Request,
    app_type: AppType,
    tag: &'static str,
    app_type_str: &'static str,
    strip_prefix: Option<&'static str>,
) -> Result<axum::response::Response, ProxyError> {
    let parsed_request = collect_json_proxy_request(request).await?;
    let is_stream = parsed_request.is_stream;

    let mut ctx = parsed_request
        .request_context(state, app_type.clone(), tag, app_type_str)
        .await?;

    let endpoint = parsed_request.endpoint_from_request_uri_stripping_prefix(strip_prefix);
    let original_body = parsed_request.body.clone();

    let proxy_request = parsed_request.into_anthropic_messages_proxy_request(
        app_type,
        endpoint.to_string(),
        Some(ctx.request_model.clone()),
    );

    let (response, api_format) =
        dispatch_claude_proxy_request_to_proxy_response(state, &mut ctx, proxy_request, is_stream)
            .await?;

    if claude_response_needs_transform(&ctx)? {
        return claude_transformed_response_to_axum_response(
            response,
            &ctx,
            state,
            &original_body,
            is_stream,
            &api_format,
            None,
        )
        .await;
    }

    claude_passthrough_response_to_axum_response(response, &ctx, state).await
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

enum CodexProxyDispatchResponse {
    ProxyResponse(ProxyResponse),
    ErrorResponse(axum::response::Response),
}

async fn dispatch_codex_proxy_request_to_proxy_response(
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

pub(crate) async fn dispatch_codex_chat_request_to_axum_response(
    state: &ProxyState,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    let parsed_request = collect_json_proxy_request(request).await?;
    let is_stream = parsed_request.is_stream;

    let mut ctx = parsed_request.codex_request_context(state).await?;
    let endpoint = parsed_request.endpoint_for_path("/chat/completions");
    let proxy_request = parsed_request
        .into_codex_chat_proxy_request(endpoint.clone(), Some(ctx.request_model.clone()));

    codex_chat_proxy_request_to_axum_response(state, &mut ctx, proxy_request, &endpoint, is_stream)
        .await
}

pub(crate) async fn dispatch_codex_responses_request_to_axum_response(
    state: &ProxyState,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    dispatch_codex_responses_path_request_to_axum_response(state, request, "/responses").await
}

pub(crate) async fn dispatch_codex_responses_compact_request_to_axum_response(
    state: &ProxyState,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    dispatch_codex_responses_path_request_to_axum_response(state, request, "/responses/compact")
        .await
}

async fn dispatch_codex_responses_path_request_to_axum_response(
    state: &ProxyState,
    request: axum::extract::Request,
    endpoint_path: &'static str,
) -> Result<axum::response::Response, ProxyError> {
    let parsed_request = collect_json_proxy_request(request).await?;
    let is_stream = parsed_request.is_stream;

    let mut ctx = parsed_request.codex_request_context(state).await?;
    let endpoint = parsed_request.endpoint_for_path(endpoint_path);
    let codex_proxy_request = parsed_request
        .into_codex_responses_proxy_request(endpoint.clone(), Some(ctx.request_model.clone()));
    let proxy_request = codex_proxy_request.request;
    let codex_tool_context = codex_proxy_request.tool_context;

    codex_responses_proxy_request_to_axum_response(
        state,
        &mut ctx,
        proxy_request,
        &endpoint,
        is_stream,
        codex_tool_context,
    )
    .await
}

async fn codex_chat_proxy_request_to_axum_response(
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

async fn codex_responses_proxy_request_to_axum_response(
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

fn proxy_result_to_proxy_response(
    result: ProxyResult,
    ctx: &mut RequestContext,
    state: &ProxyState,
) -> Result<ProxyResponse, ProxyError> {
    ctx.apply_proxy_result(state.request_context_provider_source.as_ref(), &result)?;
    proxy_core_response_to_proxy_response(result.response)
}

fn claude_proxy_result_to_proxy_response(
    result: ProxyResult,
    ctx: &mut RequestContext,
    state: &ProxyState,
) -> Result<(ProxyResponse, String), ProxyError> {
    ctx.apply_proxy_result(state.request_context_provider_source.as_ref(), &result)?;
    let api_format = ctx.claude_api_format_for_proxy_result(&result)?;
    let response = proxy_core_response_to_proxy_response(result.response)?;
    Ok((response, api_format))
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

pub(crate) async fn claude_passthrough_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
) -> Result<axum::response::Response, ProxyError> {
    process_response(response, ctx, state, &CLAUDE_PARSER_CONFIG, None).await
}

pub(crate) async fn openai_chat_passthrough_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
) -> Result<axum::response::Response, ProxyError> {
    process_response(response, ctx, state, &OPENAI_PARSER_CONFIG, None).await
}

pub(crate) async fn codex_passthrough_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
) -> Result<axum::response::Response, ProxyError> {
    process_response(response, ctx, state, &CODEX_PARSER_CONFIG, None).await
}

pub(crate) async fn gemini_passthrough_response_to_axum_response(
    response: ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
) -> Result<axum::response::Response, ProxyError> {
    process_response(response, ctx, state, &GEMINI_PARSER_CONFIG, None).await
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
