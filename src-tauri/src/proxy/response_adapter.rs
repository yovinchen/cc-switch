use super::{
    engine::response_pipeline::{
        claude_passthrough_response_to_axum_response, claude_response_needs_transform,
        claude_transformed_response_to_axum_response, codex_chat_proxy_request_to_axum_response,
        codex_responses_proxy_request_to_axum_response,
        dispatch_claude_proxy_request_to_proxy_response, dispatch_proxy_request_to_proxy_response,
        gemini_passthrough_response_to_axum_response,
    },
    error::ProxyError,
    transport::http::request_body::{
        collect_json_or_null_proxy_request, collect_json_proxy_request, endpoint_from_uri,
    },
};
use crate::app_config::AppType;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use http::Uri;

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
