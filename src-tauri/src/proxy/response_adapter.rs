use super::{
    engine::context::RequestContext,
    engine::response_pipeline::{
        claude_passthrough_response_to_axum_response, claude_proxy_result_to_proxy_response,
        claude_response_needs_transform, claude_transformed_response_to_axum_response,
        codex_chat_to_responses_transformed_response_to_axum_response,
        codex_passthrough_response_to_axum_response, codex_proxy_error_to_axum_response,
        codex_response_needs_chat_transform, gemini_passthrough_response_to_axum_response,
        openai_chat_passthrough_response_to_axum_response, proxy_result_to_proxy_response,
        record_forward_core_error_usage,
    },
    error::ProxyError,
    transport::{
        http::request_body::{
            collect_json_or_null_proxy_request, collect_json_proxy_request, endpoint_from_uri,
        },
        upstream::hyper_client::ProxyResponse,
    },
};
use crate::app_config::AppType;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy_core::api::transforms::CodexToolContext;
use crate::proxy_core::api::transport::{ProxyRequest, ProxyResult};
use http::Uri;

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
