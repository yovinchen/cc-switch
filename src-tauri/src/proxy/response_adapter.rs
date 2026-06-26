use super::{
    error::ProxyError,
    error_mapper::{
        claude_response_transform_error_to_proxy_error,
        codex_chat_to_responses_transform_error_to_proxy_error,
        codex_proxy_error_body_build_error_to_proxy_error, codex_proxy_error_response,
        codex_responses_error_body_build_error_to_proxy_error,
        parse_claude_transform_upstream_json_or_unlabeled_sse,
        parse_codex_chat_upstream_json_or_unlabeled_sse, response_build_error_to_proxy_error,
    },
    handler_context::RequestContext,
    hyper_client::ProxyResponse,
    response_processor::process_response,
};
use crate::provider::Provider;
use crate::proxy_core_adapter::{
    claude_transformed_json_response_from_context, claude_transformed_sse_stream_from_context,
    codex_auto_transformed_json_response_from_context,
    codex_auto_transformed_sse_stream_from_context, codex_chat_error_proxy_response,
    codex_chat_transform_streaming_decision, provider_claude_transform_streaming_decision,
    provider_needs_claude_transform, provider_should_convert_codex_responses_to_chat,
    read_decoded_proxy_response_body, rebuilt_json_proxy_response, record_forward_core_error_usage,
    request_body_read_error_message, transformed_sse_proxy_response, ActiveConnectionGuard,
    AxumResponseBuildErrorContext, ClaudeTransformStreamingDecision,
    ClaudeTransformedJsonResponseContext, ClaudeTransformedSseStreamContext,
    CodexAutoTransformedJsonResponseContext, CodexAutoTransformedSseStreamContext,
    CodexChatTransformStreamingDecision, CodexToolContext, CoreResponseBuildFailureContext,
    ProxyCoreResponse, ProxyEventEnvelope, ProxyRequest, ProxyResult, ProxyState,
    ProxyTransportResponse, ProxyTransportResponseBody, UpstreamSseAggregationKind,
    CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG, OPENAI_PARSER_CONFIG,
};
use axum::response::sse::Event;
use bytes::Bytes;
use futures::Stream;
use http::{HeaderMap, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;

pub(crate) async fn collect_axum_request_body(body: axum::body::Body) -> Result<Bytes, ProxyError> {
    body.collect()
        .await
        .map_err(|error| ProxyError::Internal(request_body_read_error_message(error)))
        .map(|collected| collected.to_bytes())
}

pub(crate) fn proxy_core_response_to_proxy_response(
    response: ProxyCoreResponse,
) -> Result<ProxyResponse, ProxyError> {
    let response = response
        .into_transport_response()
        .map_err(ProxyError::Internal)?;
    let ProxyTransportResponse {
        status,
        headers,
        body,
    } = response;

    let response = match body {
        ProxyTransportResponseBody::Empty => ProxyResponse::buffered(status, headers, Bytes::new()),
        ProxyTransportResponseBody::Bytes(body) => ProxyResponse::buffered(status, headers, body),
        ProxyTransportResponseBody::Stream(stream) => {
            ProxyResponse::streamed(status, headers, stream)
        }
    };

    Ok(response)
}

pub(crate) async fn dispatch_proxy_request(
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

pub(crate) fn proxy_result_to_proxy_response(
    result: ProxyResult,
    ctx: &mut RequestContext,
    state: &ProxyState,
) -> Result<ProxyResponse, ProxyError> {
    ctx.apply_proxy_result(state, &result)?;
    proxy_core_response_to_proxy_response(result.response)
}

pub(crate) fn claude_proxy_result_to_proxy_response(
    result: ProxyResult,
    ctx: &mut RequestContext,
    state: &ProxyState,
) -> Result<(ProxyResponse, String), ProxyError> {
    ctx.apply_proxy_result(state, &result)?;
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
    Ok(provider_should_convert_codex_responses_to_chat(
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

pub(crate) fn codex_chat_error_response_to_axum_response(
    status: StatusCode,
    response_headers: HeaderMap,
    body_bytes: &[u8],
) -> Result<axum::response::Response, ProxyError> {
    let response = codex_chat_error_proxy_response(status, response_headers, body_bytes)
        .map_err(codex_responses_error_body_build_error_to_proxy_error)?;
    proxy_core_response_to_axum_response(
        response,
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

pub(crate) fn proxy_event_envelope_to_axum_sse_event(event: ProxyEventEnvelope) -> Event {
    let spec = event.to_sse_spec();
    Event::default()
        .id(spec.id)
        .event(spec.event)
        .data(spec.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core_adapter::{ProxyEventEnvelope, ProxyResponseBody};
    use axum::response::{sse::Sse, IntoResponse};
    use http::StatusCode;
    use serde_json::json;
    use std::convert::Infallible;

    #[tokio::test]
    async fn proxy_core_response_bridge_preserves_stream_body() {
        let response = ProxyCoreResponse::with_body(
            StatusCode::OK,
            http::HeaderMap::new(),
            ProxyResponseBody::stream(futures::stream::once(async {
                Ok(Bytes::from_static(b"chunk"))
            })),
        );

        let proxy_response = proxy_core_response_to_proxy_response(response).expect("bridge");

        assert_eq!(proxy_response.status(), StatusCode::OK);
        let body = proxy_response.bytes().await.expect("body");
        assert_eq!(body, Bytes::from_static(b"chunk"));
    }

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

    #[tokio::test]
    async fn proxy_event_envelope_bridge_serializes_sse_fields() {
        let event = proxy_event_envelope_to_axum_sse_event(ProxyEventEnvelope::new(
            42,
            "request_started",
            "2026-06-20T00:00:00Z",
            json!({"provider": "relay-a"}),
        ));

        let response =
            Sse::new(futures::stream::once(async { Ok::<_, Infallible>(event) })).into_response();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).expect("sse body");

        assert!(text.contains("id: 42\n"), "{text}");
        assert!(text.contains("event: request_started\n"), "{text}");
        assert!(text.contains("\"provider\":\"relay-a\""), "{text}");
        assert!(text.ends_with("\n\n"), "{text}");
    }
}
