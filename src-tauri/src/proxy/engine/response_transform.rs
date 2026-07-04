//! Transformed response wrappers for Claude and Codex response conversion.

use super::context::RequestContext;
use super::response_stream::{
    create_claude_transformed_logged_stream, create_codex_auto_transformed_logged_stream,
};
use super::response_usage::{
    record_transformed_response_usage_from_context, usage_logging_enabled_from_state,
    TransformedResponseUsageRecordContext,
};
use crate::provider::Provider;
use crate::proxy::codex_chat_history::{
    transform_codex_chat_response_with_history, transform_codex_chat_sse_with_history,
};
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy::provider::{
    transform_claude_response_for_api_format, transform_claude_sse_for_api_format,
};
use crate::proxy_core::api::transforms::{
    extract_anthropic_tool_schema_hints, AnthropicToolSchemaHints, CodexToolContext,
};
use crate::proxy_core::api::usage::TransformedResponseUsageFormat;
use bytes::Bytes;
use futures::Stream;
use serde_json::Value;

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
