//! 请求处理器
//!
//! 处理各种API端点的HTTP请求
//!
//! 重构后的结构：
//! - 通用逻辑提取到 `handler_context` 和 `response_processor` 模块
//! - 各 handler 只保留独特的业务逻辑
//! - Claude 的格式转换逻辑保留在此文件（用于 OpenRouter 旧接口回退）

use super::{
    auth_adapter::validate_claude_desktop_gateway_auth,
    error::ProxyError,
    error_mapper::{
        codex_proxy_error_response, management_api_error_to_proxy_error,
        management_auth_error_to_proxy_error, proxy_core_error_to_proxy_error,
        response_body_parse_error_to_proxy_error,
    },
    forwarder::ActiveConnectionGuard,
    handler_context::RequestContext,
    codex_chat_history::record_responses_sse_stream,
    response_adapter::{
        collect_axum_request_body, proxy_core_response_to_axum_response,
        proxy_core_response_to_proxy_response, proxy_event_envelope_to_axum_sse_event,
    },
    response_processor::{create_logged_passthrough_stream, process_response, read_decoded_body},
    server::ProxyState,
    usage_sink_bridge::{
        record_forward_error_usage, record_transformed_response_usage,
        transformed_streaming_usage_collector,
    },
};
use crate::app_config::AppType;
use crate::proxy_core_adapter::synthesize_gemini_tool_call_id_with_uuid;
use crate::proxy_core_adapter::{
    append_query_to_endpoint_path,
    chat_completion_to_response_with_context as build_chat_completion_response_with_context,
    claude_stream_usage_event_filter, claude_transform_unlabeled_sse_aggregation,
    codex_stream_usage_event_filter,
    create_codex_chat_to_responses_sse_stream_with_context as create_responses_sse_stream_from_chat_with_context,
    create_gemini_to_anthropic_sse_stream_with_callbacks as create_anthropic_sse_stream_from_gemini,
    create_openai_chat_to_anthropic_sse_stream as create_anthropic_sse_stream,
    create_openai_responses_to_anthropic_sse_stream as create_anthropic_sse_stream_from_responses,
    extract_anthropic_tool_schema_hints, extract_gemini_model_from_path,
    gemini_response_to_anthropic_message_with_shadow, openai_chat_to_anthropic_message,
    json_proxy_request_from_input, JsonProxyRequestInput,
    openai_responses_to_anthropic_message, parse_json_proxy_request_body,
    parse_json_proxy_request_body_or_null, parse_upstream_json_or_unlabeled_sse,
    management_auth_decision_from_proxy_config,
    provider_is_codex_oauth, provider_needs_claude_transform,
    provider_should_convert_codex_responses_to_chat, rebuilt_json_proxy_response,
    response_headers_indicate_sse, should_aggregate_codex_oauth_responses_sse,
    should_use_claude_transform_streaming,
    strip_endpoint_prefix, transformed_sse_proxy_response, validate_management_bearer_header,
    AppChannelListQuery, AppChannelManagementRequest, AppChannelResponse, AppKind, AppListRequest,
    AppListResponse, AppModelCatalogRequest, AppModelListQuery, ChannelCreateRequest,
    ChannelDeleteResponse, ChannelHealthResetResponse, ChannelKeyDeleteResponse,
    ChannelKeyPathRequest, ChannelKeyRecord, ChannelKeyRecordResponse, ChannelKeysResponse,
    ChannelListQuery, ChannelListRequest, ChannelListResponse, ChannelMigrationMaterializeResponse,
    ChannelMigrationPreviewResponse, ChannelModelRecord, ChannelModelsResponse, ChannelPathRequest,
    ChannelRecord, ChannelRecordResponse, ChannelRouteCandidate, ChannelRouteRejected,
    ChannelTestResponse, ClaudeDesktopModelListResponse, ClientModelCatalogResponse,
    CodexToolContext, CurrentRouteResponse, CurrentRouteTarget, GroupListQuery, GroupListRequest,
    HealthCheckRequest, HealthCheckResponse, InterfaceKind, ManagementAppPathRequest,
    ManagementAuthDecision, ProviderListResponse, ProxyChannelKeyPatchRequest,
    ProxyChannelKeyWriteRequest, ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest,
    ProxyChannelTestRequest, ProxyChannelWriteRequest, ProxyRuntimeStatus,
    ProxyStatusRequest, ProxyStatusResponse, RoutableModelList, RouteGroupListResponse,
    RouteResolveManagementRequest, RouteResolveRequest, RouteResolveResponse,
    TransformedResponseUsageFormat, UnlabeledSseFallbackLogContext, UnlabeledSseFallbackLogLevel,
    UpstreamSseAggregationKind, CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG,
    OPENAI_PARSER_CONFIG,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use bytes::Bytes;
use serde_json::Value;
use std::convert::Infallible;
use std::time::Duration;

// ============================================================================
// 健康检查和状态查询（简单端点）
// ============================================================================

/// 健康检查
pub async fn health_check() -> (StatusCode, Json<HealthCheckResponse>) {
    let request = HealthCheckRequest::new();
    (
        StatusCode::OK,
        Json(request.response(chrono::Utc::now().to_rfc3339())),
    )
}

/// 获取服务状态
pub async fn get_status(
    State(state): State<ProxyState>,
) -> Result<Json<ProxyStatusResponse<ProxyRuntimeStatus>>, ProxyError> {
    let request = ProxyStatusRequest::new();
    let status = state.status.read().await.clone();
    Ok(Json(request.response(status)))
}

/// GET /proxy/v1/events
pub async fn stream_proxy_events(
    State(state): State<ProxyState>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let mut receiver = state.events.subscribe();
    let events = state.events.clone();

    let stream = async_stream::stream! {
        yield Ok(proxy_event_envelope_to_axum_sse_event(events.connected_event()));

        loop {
            match receiver.recv().await {
                Ok(event) => yield Ok(proxy_event_envelope_to_axum_sse_event(event)),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    yield Ok(proxy_event_envelope_to_axum_sse_event(events.lagged_event(skipped)));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Management API auth middleware.
///
/// Loopback listeners stay compatible and allow unauthenticated local access.
/// Non-loopback listeners require a bearer token from `ProxyConfig` or
/// `CC_SWITCH_PROXY_MANAGEMENT_TOKEN`.
pub async fn require_proxy_management_auth(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, ProxyError> {
    let auth_decision = {
        let config = state.config.read().await;
        management_auth_decision_from_proxy_config(&config)
            .map_err(management_auth_error_to_proxy_error)?
    };

    if let ManagementAuthDecision::RequireToken(expected_token) = auth_decision {
        validate_management_bearer_header(request.headers(), &expected_token)
            .map_err(management_auth_error_to_proxy_error)?;
    }

    Ok(next.run(request).await)
}

/// GET /proxy/v1/apps
pub async fn list_proxy_apps(
    State(state): State<ProxyState>,
) -> Result<Json<AppListResponse>, ProxyError> {
    let request = AppListRequest::new();
    let response = state
        .proxy_engine()
        .app_list_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /proxy/v1/apps/{app}/providers
pub async fn list_proxy_providers(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<ProviderListResponse>, ProxyError> {
    let request = ManagementAppPathRequest::from_path(app_type)
        .map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .provider_list_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /proxy/v1/apps/{app}/models
pub async fn list_proxy_app_models(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
    Query(query): Query<AppModelListQuery>,
) -> Result<Json<RoutableModelList>, ProxyError> {
    let request = AppModelCatalogRequest::from_parts(app_type, query)
        .map_err(management_api_error_to_proxy_error)?;
    let engine = state.proxy_engine();
    let catalog = engine
        .list_model_catalog_for_request(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(catalog))
}

/// GET /proxy/v1/channels
pub async fn list_all_proxy_channels(
    State(state): State<ProxyState>,
    Query(query): Query<ChannelListQuery>,
) -> Result<Json<ChannelListResponse<ChannelRecord>>, ProxyError> {
    let request =
        ChannelListRequest::from_query(query).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .channel_list_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// POST /proxy/v1/channels
pub async fn create_proxy_channel(
    State(state): State<ProxyState>,
    Json(request): Json<ProxyChannelWriteRequest>,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    let request = ChannelCreateRequest::from_body(request);
    let response = state
        .proxy_engine()
        .create_channel_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /proxy/v1/channels/{channel_id}
pub async fn get_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    let request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .channel_record_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// PATCH /proxy/v1/channels/{channel_id}
pub async fn update_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
    Json(request): Json<ProxyChannelPatchRequest>,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    let path_request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .update_channel_response(path_request, request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// DELETE /proxy/v1/channels/{channel_id}
pub async fn delete_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelDeleteResponse>, ProxyError> {
    let request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .delete_channel_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /proxy/v1/channels/{channel_id}/keys
pub async fn list_proxy_channel_keys(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelKeysResponse<ChannelKeyRecord>>, ProxyError> {
    let request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .channel_keys_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// PUT /proxy/v1/channels/{channel_id}/keys/{key_ref}
pub async fn upsert_proxy_channel_key(
    State(state): State<ProxyState>,
    Path((channel_id, key_ref)): Path<(String, String)>,
    Json(request): Json<ProxyChannelKeyWriteRequest>,
) -> Result<Json<ChannelKeyRecordResponse<ChannelKeyRecord>>, ProxyError> {
    let path_request = ChannelKeyPathRequest::from_path(channel_id, key_ref)
        .map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .upsert_channel_key_response(path_request, request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// PATCH /proxy/v1/channels/{channel_id}/keys/{key_ref}
pub async fn update_proxy_channel_key(
    State(state): State<ProxyState>,
    Path((channel_id, key_ref)): Path<(String, String)>,
    Json(request): Json<ProxyChannelKeyPatchRequest>,
) -> Result<Json<ChannelKeyRecordResponse<ChannelKeyRecord>>, ProxyError> {
    let path_request = ChannelKeyPathRequest::from_path(channel_id, key_ref)
        .map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .update_channel_key_response(path_request, request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// DELETE /proxy/v1/channels/{channel_id}/keys/{key_ref}
pub async fn delete_proxy_channel_key(
    State(state): State<ProxyState>,
    Path((channel_id, key_ref)): Path<(String, String)>,
) -> Result<Json<ChannelKeyDeleteResponse>, ProxyError> {
    let path_request = ChannelKeyPathRequest::from_path(channel_id, key_ref)
        .map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .delete_channel_key_response(path_request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /proxy/v1/channels/{channel_id}/models
pub async fn list_proxy_channel_models(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelModelsResponse<ChannelModelRecord>>, ProxyError> {
    let request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .channel_models_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// PUT /proxy/v1/channels/{channel_id}/models
pub async fn replace_proxy_channel_models(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
    Json(request): Json<ProxyChannelModelsReplaceRequest>,
) -> Result<Json<ChannelModelsResponse<ChannelModelRecord>>, ProxyError> {
    let path_request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .replace_channel_models_response(path_request, request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// POST /proxy/v1/channels/{channel_id}/test
pub async fn test_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
    Json(request): Json<ProxyChannelTestRequest>,
) -> Result<Json<ChannelTestResponse>, ProxyError> {
    let path_request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .channel_test_response(path_request, request, chrono::Utc::now().timestamp())
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /proxy/v1/apps/{app}/channels
pub async fn list_proxy_channels(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
    Query(query): Query<AppChannelListQuery>,
) -> Result<
    Json<AppChannelResponse<ChannelRecord, ChannelRouteCandidate, ChannelRouteRejected>>,
    ProxyError,
> {
    let request = AppChannelManagementRequest::from_parts(app_type, query)
        .map_err(management_api_error_to_proxy_error)?;

    let response = state
        .proxy_engine()
        .app_channel_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /proxy/v1/groups
pub async fn list_proxy_groups(
    State(state): State<ProxyState>,
    Query(query): Query<GroupListQuery>,
) -> Result<Json<RouteGroupListResponse>, ProxyError> {
    let request =
        GroupListRequest::from_query(query).map_err(management_api_error_to_proxy_error)?;

    let response = state
        .proxy_engine()
        .group_list_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /proxy/v1/apps/{app}/routes/current
pub async fn get_current_proxy_route(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<CurrentRouteResponse<CurrentRouteTarget>>, ProxyError> {
    let request = ManagementAppPathRequest::from_path(app_type)
        .map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .current_route_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /proxy/v1/apps/{app}/channels/migration/preview
pub async fn preview_proxy_channel_migration(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<ChannelMigrationPreviewResponse<ChannelRecord>>, ProxyError> {
    let request = ManagementAppPathRequest::from_path(app_type)
        .map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .channel_migration_preview_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// POST /proxy/v1/apps/{app}/channels/migration/materialize
pub async fn materialize_proxy_channel_migration(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<ChannelMigrationMaterializeResponse>, ProxyError> {
    let request = ManagementAppPathRequest::from_path(app_type)
        .map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .channel_migration_materialize_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// POST /proxy/v1/channels/{channel_id}/breakers/reset
pub async fn reset_proxy_channel_breaker(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelHealthResetResponse>, ProxyError> {
    let request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .reset_channel_health_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// POST /proxy/v1/route/resolve
pub async fn resolve_proxy_route(
    State(state): State<ProxyState>,
    Json(request): Json<RouteResolveRequest>,
) -> Result<Json<RouteResolveResponse>, ProxyError> {
    let request = RouteResolveManagementRequest::from_body(request)
        .map_err(management_api_error_to_proxy_error)?;

    let response = state
        .proxy_engine()
        .resolve_route_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

/// GET /v1/models — Codex model list (reachability check)
///
/// Codex CLI probes this endpoint at startup and deserializes the response as a
/// catalog with a top-level `models` field.  Return the cc-switch–managed model
/// catalog file directly so the format always matches what the current version
/// of Codex expects.
///
/// Only serves the catalog when the live config.toml still references the
/// cc-switch–owned `model_catalog_json`, using the same path ownership rules as
/// Codex live-setting import.
pub async fn handle_models(
    State(state): State<ProxyState>,
) -> Result<Json<ClientModelCatalogResponse>, ProxyError> {
    let response = state
        .proxy_engine()
        .client_model_catalog_response(&AppKind::Codex)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;
    Ok(Json(response))
}

// ============================================================================
// Claude API 处理器（包含格式转换逻辑）
// ============================================================================

/// 处理 /v1/messages 请求（Claude API）
///
/// Claude 处理器包含独特的格式转换逻辑：
/// - 过去用于 OpenRouter 的 OpenAI Chat Completions 兼容接口（Anthropic ↔ OpenAI 转换）
/// - 现在 OpenRouter 已推出 Claude Code 兼容接口，默认不再启用该转换（逻辑保留以备回退）
pub async fn handle_messages(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    handle_messages_for_app(state, request, AppType::Claude, "Claude", "claude", None).await
}

pub async fn handle_claude_desktop_messages(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    validate_claude_desktop_gateway_auth(&state, request.headers())?;
    handle_messages_for_app(
        state,
        request,
        AppType::ClaudeDesktop,
        "Claude Desktop",
        "claude-desktop",
        Some("/claude-desktop"),
    )
    .await
}

pub async fn handle_claude_desktop_models(
    State(state): State<ProxyState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ClaudeDesktopModelListResponse>, ProxyError> {
    validate_claude_desktop_gateway_auth(&state, &headers)?;
    let response = state
        .proxy_engine()
        .claude_desktop_model_list_response()
        .await
        .map_err(proxy_core_error_to_proxy_error)?;
    Ok(Json(response))
}

async fn handle_messages_for_app(
    state: ProxyState,
    request: axum::extract::Request,
    app_type: AppType,
    tag: &'static str,
    app_type_str: &'static str,
    strip_prefix: Option<&'static str>,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, body) = request.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri;
    let headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = collect_axum_request_body(body).await?;
    let parsed_body = parse_json_proxy_request_body(&body_bytes)
        .map_err(|e| ProxyError::Internal(e.to_string()))?;
    let body = parsed_body.body;
    let is_stream = parsed_body.is_stream;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, app_type.clone(), tag, app_type_str).await?;

    let raw_endpoint = append_query_to_endpoint_path(uri.path(), uri.query());
    let endpoint = strip_endpoint_prefix(&raw_endpoint, strip_prefix);

    let proxy_request = json_proxy_request_from_input(JsonProxyRequestInput {
        app_type: app_type.clone(),
        method,
        endpoint: endpoint.to_string(),
        inbound_interface: InterfaceKind::AnthropicMessages,
        body: body.clone(),
        requested_model: Some(ctx.request_model.clone()),
        headers,
        extensions,
    });

    let engine = state.proxy_engine();
    let result = match engine.handle(proxy_request).await {
        Ok(result) => result,
        Err(error) => {
            let error = proxy_core_error_to_proxy_error(error);
            record_forward_error_usage(&state, &ctx, is_stream, &error);
            return Err(error);
        }
    };

    ctx.apply_proxy_result(&state, &result)?;
    let api_format = ctx.claude_api_format_for_proxy_result(&result)?;
    let response = proxy_core_response_to_proxy_response(result.response)?;

    // 检查是否需要格式转换（OpenRouter 等中转服务）
    let needs_transform = provider_needs_claude_transform(ctx.provider()?);

    // Claude 特有：格式转换处理
    if needs_transform {
        return handle_claude_transform(
            response,
            &ctx,
            &state,
            &body,
            is_stream,
            &api_format,
            None,
        )
        .await;
    }

    // 通用响应处理（透传模式）
    process_response(response, &ctx, &state, &CLAUDE_PARSER_CONFIG, None).await
}

/// Claude 格式转换处理（独有逻辑）
///
/// 支持 OpenAI Chat Completions 和 Responses API 两种格式的转换
async fn handle_claude_transform(
    response: super::hyper_client::ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    original_body: &Value,
    is_stream: bool,
    api_format: &str,
    connection_guard: Option<ActiveConnectionGuard>,
) -> Result<axum::response::Response, ProxyError> {
    let status = response.status();
    let provider = ctx.provider()?;
    let is_codex_oauth = provider_is_codex_oauth(provider);
    // Codex OAuth 会把 openai_responses 响应强制升级为 SSE，即使客户端发的是 stream:false。
    // should_use_claude_transform_streaming 默认会把这个组合路由到流式转换器——虽然能避免
    // JSON parse 报 422，但会让非流客户端收到 text/event-stream，违反 Anthropic 非流语义。
    // 这里为这个特定组合打开 override：把上游 SSE 聚合成 Anthropic JSON 回给客户端，其它
    // 场景（任意上游 is_sse、非 Codex OAuth 等）仍沿用原有流式兜底。
    let aggregate_codex_oauth_responses_sse =
        should_aggregate_codex_oauth_responses_sse(is_stream, api_format, is_codex_oauth);
    let use_streaming = if aggregate_codex_oauth_responses_sse {
        false
    } else {
        should_use_claude_transform_streaming(
            is_stream,
            response_headers_indicate_sse(response.headers()),
            api_format,
            is_codex_oauth,
        )
    };
    let tool_schema_hints = extract_anthropic_tool_schema_hints(original_body);
    let tool_schema_hints = (!tool_schema_hints.is_empty()).then_some(tool_schema_hints);

    if use_streaming {
        // 根据 api_format 选择流式转换器
        let stream = response.bytes_stream();
        let sse_stream: Box<
            dyn futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + Unpin,
        > = if api_format == "openai_responses" {
            Box::new(Box::pin(create_anthropic_sse_stream_from_responses(stream)))
        } else if api_format == "gemini_native" {
            Box::new(Box::pin(create_anthropic_sse_stream_from_gemini(
                stream,
                Some(state.gemini_shadow.clone()),
                Some(provider.id.clone()),
                Some(ctx.session_id.clone()),
                tool_schema_hints.clone(),
                synthesize_gemini_tool_call_id_with_uuid,
                |name| log::info!("[Claude/Gemini] Rectified tool args for `{name}`"),
            )))
        } else {
            Box::new(Box::pin(create_anthropic_sse_stream(stream)))
        };

        // 创建使用量收集器；关闭 usage logging 时不要再解析转换后的 SSE。
        let usage_collector = transformed_streaming_usage_collector(
            state,
            ctx,
            status.as_u16(),
            TransformedResponseUsageFormat::Claude,
            claude_stream_usage_event_filter,
        );

        // 获取流式超时配置
        let timeout_config = ctx.streaming_timeout_config();

        let logged_stream = create_logged_passthrough_stream(
            sse_stream,
            "Claude/OpenRouter",
            usage_collector,
            timeout_config,
            connection_guard,
        );

        let response = transformed_sse_proxy_response(logged_stream);
        return proxy_core_response_to_axum_response(response, "[Claude] 构建 SSE 响应失败");
    }

    // 非流式响应转换 (OpenAI/Responses → Anthropic)
    let (response_headers, _status, body_bytes) =
        read_decoded_body(response, ctx.tag, ctx.body_timeout_duration()).await?;

    let body_str = String::from_utf8_lossy(&body_bytes);

    // 兜底嗅探（#2234）：部分网关对 stream:false 强制返回 SSE 体，却把
    // Content-Type 标成 application/json 等，is_sse() 的 header 检查失效。
    // 此时按 SSE 聚合成单个 JSON 再走既有非流转换器，客户端仍收到
    // Anthropic JSON，非流语义不变。gemini_native 暂无聚合器，落诊断错误。
    let response_sse_aggregation = if aggregate_codex_oauth_responses_sse {
        Some(UpstreamSseAggregationKind::Responses)
    } else {
        claude_transform_unlabeled_sse_aggregation(api_format)
    };
    let parsed = parse_upstream_json_or_unlabeled_sse(
        &body_bytes,
        &response_headers,
        "Failed to parse upstream response",
        response_sse_aggregation,
        || uuid::Uuid::new_v4().to_string(),
    )
    .map_err(|error| {
        log::error!("[Claude] 解析/聚合上游响应失败: {error}, body: {body_str}");
        response_body_parse_error_to_proxy_error(error)
    })?;

    if let Some(event) =
        parsed
            .source
            .unlabeled_sse_fallback_log_event(UnlabeledSseFallbackLogContext::Claude {
                api_format,
                codex_oauth_responses_aggregation: aggregate_codex_oauth_responses_sse,
            })
    {
        match event.level {
            UnlabeledSseFallbackLogLevel::Debug => log::debug!("{}", event.message),
            UnlabeledSseFallbackLogLevel::Warn => log::warn!("{}", event.message),
        }
    }
    let upstream_response: Value = parsed.value;

    // 根据 api_format 选择非流式转换器
    let anthropic_response = if api_format == "openai_responses" {
        openai_responses_to_anthropic_message(&upstream_response)
            .map_err(ProxyError::TransformError)
    } else if api_format == "gemini_native" {
        gemini_response_to_anthropic_message_with_shadow(
            &upstream_response,
            Some(state.gemini_shadow.as_ref()),
            Some(&provider.id),
            Some(&ctx.session_id),
            tool_schema_hints.as_ref(),
            synthesize_gemini_tool_call_id_with_uuid,
        )
        .map(|output| {
            for name in &output.rectified_tool_names {
                log::info!("[Claude/Gemini] Rectified tool args for `{name}`");
            }
            output.response
        })
        .map_err(ProxyError::TransformError)
    } else {
        openai_chat_to_anthropic_message(&upstream_response).map_err(ProxyError::TransformError)
    }
    .map_err(|e| {
        log::error!("[Claude] 转换响应失败: {e}");
        e
    })?;

    record_transformed_response_usage(
        state,
        ctx,
        &anthropic_response,
        TransformedResponseUsageFormat::Claude,
        status.as_u16(),
    );

    let response = rebuilt_json_proxy_response(status, response_headers, anthropic_response)
        .map_err(|error| {
            log::error!("[Claude] 构造 JSON 响应失败: {error}");
            proxy_core_error_to_proxy_error(error)
        })?;

    proxy_core_response_to_axum_response(response, "[Claude] 构建响应失败")
}

// ============================================================================
// Codex API 处理器
// ============================================================================

/// 处理 /v1/chat/completions 请求（OpenAI Chat Completions API - Codex CLI）
pub async fn handle_chat_completions(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri;
    let headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = collect_axum_request_body(req_body).await?;
    let parsed_body = parse_json_proxy_request_body(&body_bytes)
        .map_err(|e| ProxyError::Internal(e.to_string()))?;
    let body = parsed_body.body;
    let is_stream = parsed_body.is_stream;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex").await?;
    let endpoint = append_query_to_endpoint_path("/chat/completions", uri.query());

    let proxy_request = json_proxy_request_from_input(JsonProxyRequestInput {
        app_type: AppType::Codex,
        method,
        endpoint: endpoint.clone(),
        inbound_interface: InterfaceKind::OpenAiChatCompletions,
        body,
        requested_model: Some(ctx.request_model.clone()),
        headers,
        extensions,
    });

    let engine = state.proxy_engine();
    let result = match engine.handle(proxy_request).await {
        Ok(result) => result,
        Err(error) => {
            let error = proxy_core_error_to_proxy_error(error);
            record_forward_error_usage(&state, &ctx, is_stream, &error);
            return build_codex_proxy_error_response(&ctx, &endpoint, &error);
        }
    };

    ctx.apply_proxy_result(&state, &result)?;
    let response = proxy_core_response_to_proxy_response(result.response)?;

    process_response(response, &ctx, &state, &OPENAI_PARSER_CONFIG, None).await
}

/// 处理 /v1/responses 请求（OpenAI Responses API - Codex CLI 透传）
pub async fn handle_responses(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri;
    let headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = collect_axum_request_body(req_body).await?;
    let parsed_body = parse_json_proxy_request_body(&body_bytes)
        .map_err(|e| ProxyError::Internal(e.to_string()))?;
    let body = parsed_body.body;
    let is_stream = parsed_body.is_stream;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex").await?;
    let endpoint = append_query_to_endpoint_path("/responses", uri.query());

    let codex_tool_context = crate::proxy_core_adapter::codex_tool_context_from_request(&body);

    let proxy_request = json_proxy_request_from_input(JsonProxyRequestInput {
        app_type: AppType::Codex,
        method,
        endpoint: endpoint.clone(),
        inbound_interface: InterfaceKind::OpenAiResponses,
        body,
        requested_model: Some(ctx.request_model.clone()),
        headers,
        extensions,
    });

    let engine = state.proxy_engine();
    let result = match engine.handle(proxy_request).await {
        Ok(result) => result,
        Err(error) => {
            let error = proxy_core_error_to_proxy_error(error);
            record_forward_error_usage(&state, &ctx, is_stream, &error);
            return build_codex_proxy_error_response(&ctx, &endpoint, &error);
        }
    };

    ctx.apply_proxy_result(&state, &result)?;
    let response = proxy_core_response_to_proxy_response(result.response)?;

    if provider_should_convert_codex_responses_to_chat(ctx.provider()?, &endpoint) {
        return handle_codex_chat_to_responses_transform(
            response,
            &ctx,
            &state,
            is_stream,
            None,
            codex_tool_context,
        )
        .await;
    }

    process_response(response, &ctx, &state, &CODEX_PARSER_CONFIG, None).await
}

/// 处理 /v1/responses/compact 请求（OpenAI Responses Compact API - Codex CLI 透传）
pub async fn handle_responses_compact(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri;
    let headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = collect_axum_request_body(req_body).await?;
    let parsed_body = parse_json_proxy_request_body(&body_bytes)
        .map_err(|e| ProxyError::Internal(e.to_string()))?;
    let body = parsed_body.body;
    let is_stream = parsed_body.is_stream;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex").await?;
    let endpoint = append_query_to_endpoint_path("/responses/compact", uri.query());

    let codex_tool_context = crate::proxy_core_adapter::codex_tool_context_from_request(&body);

    let proxy_request = json_proxy_request_from_input(JsonProxyRequestInput {
        app_type: AppType::Codex,
        method,
        endpoint: endpoint.clone(),
        inbound_interface: InterfaceKind::OpenAiResponses,
        body,
        requested_model: Some(ctx.request_model.clone()),
        headers,
        extensions,
    });

    let engine = state.proxy_engine();
    let result = match engine.handle(proxy_request).await {
        Ok(result) => result,
        Err(error) => {
            let error = proxy_core_error_to_proxy_error(error);
            record_forward_error_usage(&state, &ctx, is_stream, &error);
            return build_codex_proxy_error_response(&ctx, &endpoint, &error);
        }
    };

    ctx.apply_proxy_result(&state, &result)?;
    let response = proxy_core_response_to_proxy_response(result.response)?;

    if provider_should_convert_codex_responses_to_chat(ctx.provider()?, &endpoint) {
        return handle_codex_chat_to_responses_transform(
            response,
            &ctx,
            &state,
            is_stream,
            None,
            codex_tool_context,
        )
        .await;
    }

    process_response(response, &ctx, &state, &CODEX_PARSER_CONFIG, None).await
}

async fn handle_codex_chat_to_responses_transform(
    response: super::hyper_client::ProxyResponse,
    ctx: &RequestContext,
    state: &ProxyState,
    is_stream: bool,
    connection_guard: Option<ActiveConnectionGuard>,
    tool_context: CodexToolContext,
) -> Result<axum::response::Response, ProxyError> {
    let status = response.status();

    if !status.is_success() {
        // 上游 Chat 错误体形状与 Responses 不一致（如 MiniMax 的 base_resp、自定义 detail 字段）；
        // 直接透传会让 Codex 客户端无法识别错误码。这里统一转换为 Responses 风格
        // `{"error": {message, type, code, param}}`，保留原始 HTTP 状态码。
        return handle_codex_chat_error_response(response, ctx, status).await;
    }

    if is_stream || response_headers_indicate_sse(response.headers()) {
        let stream = response.bytes_stream();
        let sse_stream = create_responses_sse_stream_from_chat_with_context(stream, tool_context);
        let sse_stream = record_responses_sse_stream(sse_stream, state.codex_chat_history.clone());

        let usage_collector = transformed_streaming_usage_collector(
            state,
            ctx,
            status.as_u16(),
            TransformedResponseUsageFormat::CodexAuto,
            codex_stream_usage_event_filter,
        );

        let logged_stream = create_logged_passthrough_stream(
            sse_stream,
            ctx.tag,
            usage_collector,
            ctx.streaming_timeout_config(),
            connection_guard,
        );

        let response = transformed_sse_proxy_response(logged_stream);
        return proxy_core_response_to_axum_response(response, "[Codex] 构建 SSE 响应失败");
    }

    let _connection_guard = connection_guard;
    let (response_headers, status, body_bytes) =
        read_decoded_body(response, ctx.tag, ctx.body_timeout_duration()).await?;
    let body_str = String::from_utf8_lossy(&body_bytes);
    // 与 Claude 侧 handle_claude_transform 对称的兜底嗅探（#2234）：
    // 上游对 stream:false 返回未标记 Content-Type 的 SSE 体时按 Chat SSE 聚合。
    let parsed_chat_response = parse_upstream_json_or_unlabeled_sse(
        &body_bytes,
        &response_headers,
        "Failed to parse upstream chat response",
        Some(UpstreamSseAggregationKind::ChatCompletions),
        || uuid::Uuid::new_v4().to_string(),
    )
    .map_err(|error| {
        log::error!("[Codex] 解析/聚合 Chat 上游响应失败: {error}, body: {body_str}");
        response_body_parse_error_to_proxy_error(error)
    })?;

    if let Some(event) = parsed_chat_response
        .source
        .unlabeled_sse_fallback_log_event(UnlabeledSseFallbackLogContext::CodexChat)
    {
        match event.level {
            UnlabeledSseFallbackLogLevel::Debug => log::debug!("{}", event.message),
            UnlabeledSseFallbackLogLevel::Warn => log::warn!("{}", event.message),
        }
    }

    let chat_response = parsed_chat_response.value;
    let responses_response =
        build_chat_completion_response_with_context(&chat_response, &tool_context)
            .map_err(ProxyError::TransformError)
            .map_err(|e| {
                log::error!("[Codex] Chat → Responses 响应转换失败: {e}");
                e
            })?;
    state
        .codex_chat_history
        .record_response(&responses_response)
        .await;

    record_transformed_response_usage(
        state,
        ctx,
        &responses_response,
        TransformedResponseUsageFormat::CodexAuto,
        status.as_u16(),
    );

    let response = rebuilt_json_proxy_response(status, response_headers, responses_response)
        .map_err(|error| {
            log::error!("[Codex] 构造 Responses 响应失败: {error}");
            proxy_core_error_to_proxy_error(error)
        })?;

    proxy_core_response_to_axum_response(response, "[Codex] 构建 Responses 响应失败")
}

/// 把上游 Chat Completions 的错误响应转换为 Responses API 错误形状。
///
/// 与正常响应分支配套：正常响应已经被改写成 Responses 形式，错误响应若仍保留
/// Chat 错误体（如 MiniMax 的 `{"base_resp": {"status_code": 2013}}`），Codex
/// 客户端的错误处理就无法对齐字段。这里读取上游 body、规整成
/// `{"error": {message, type, code, param}}` 并保留原始 HTTP 状态码。
async fn handle_codex_chat_error_response(
    response: super::hyper_client::ProxyResponse,
    ctx: &RequestContext,
    status: axum::http::StatusCode,
) -> Result<axum::response::Response, ProxyError> {
    let (response_headers, _status, body_bytes) =
        read_decoded_body(response, ctx.tag, ctx.body_timeout_duration()).await?;

    let normalized_error = crate::proxy_core_adapter::normalize_codex_chat_error_body(&body_bytes);
    if let Some(message) = normalized_error.non_json_body_log_message() {
        log::warn!("{message}");
    }
    let responses_error = normalized_error.response_error;

    let response = rebuilt_json_proxy_response(status, response_headers, responses_error).map_err(
        |error| {
            log::error!("[Codex] 构造 Responses 错误体失败: {error}");
            proxy_core_error_to_proxy_error(error)
        },
    )?;

    proxy_core_response_to_axum_response(response, "[Codex] 构建 Responses 错误响应失败")
}

/// 把转发层（非上游响应）的失败构造成富化的 Codex 错误响应。
///
/// 与 `handle_codex_chat_error_response`（处理上游真实错误响应、复制上游头）不同，
/// 这里没有上游响应可参照，只产出一个 `application/json` 错误体。状态码走
/// host error adapter 仍负责把 `ProxyError` 映射成 core status kind/context。
///
/// 注意：`endpoint` 经 core endpoint query helper 可能携带 query（如 `?beta=true`）并被
/// 原样写入错误体。当前 Codex 端点不在 query 里放凭证，故安全；若将来复用到
/// query 携带密钥的端点（如 Gemini 的 `?key=`），需先脱敏再回显。
fn build_codex_proxy_error_response(
    ctx: &RequestContext,
    endpoint: &str,
    error: &ProxyError,
) -> Result<axum::response::Response, ProxyError> {
    let response = codex_proxy_error_response(
        ctx.provider_name_for_error(),
        &ctx.request_model,
        endpoint,
        error,
    )
    .map_err(|error| {
        log::error!("[Codex] 构造代理错误响应失败: {error}");
        proxy_core_error_to_proxy_error(error)
    })?;

    proxy_core_response_to_axum_response(response, "[Codex] 构建代理错误响应失败")
}

// ============================================================================
// Gemini API 处理器
// ============================================================================

/// 处理 Gemini API 请求（透传，包括查询参数）
pub async fn handle_gemini(
    State(state): State<ProxyState>,
    uri: axum::http::Uri,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    let method = parts.method.clone();
    let headers = parts.headers;
    let extensions = parts.extensions;
    let body_bytes = collect_axum_request_body(req_body).await?;
    let parsed_body = parse_json_proxy_request_body_or_null(&body_bytes)
        .map_err(|e| ProxyError::Internal(e.to_string()))?;
    let body = parsed_body.body;
    let is_stream = parsed_body.is_stream;

    // Gemini 的模型名称在 URI 中
    let mut ctx = RequestContext::new(&state, &body, &headers, AppType::Gemini, "Gemini", "gemini")
        .await?
        .with_model_from_uri(&uri);

    // 提取完整的路径和查询参数
    let endpoint = append_query_to_endpoint_path(uri.path(), uri.query());

    let proxy_request = json_proxy_request_from_input(JsonProxyRequestInput {
        app_type: AppType::Gemini,
        method,
        endpoint: endpoint.clone(),
        inbound_interface: InterfaceKind::GeminiNative,
        body,
        requested_model: extract_gemini_model_from_path(&endpoint),
        headers,
        extensions,
    });

    let engine = state.proxy_engine();
    let result = match engine.handle(proxy_request).await {
        Ok(result) => result,
        Err(error) => {
            let error = proxy_core_error_to_proxy_error(error);
            record_forward_error_usage(&state, &ctx, is_stream, &error);
            return Err(error);
        }
    };

    ctx.apply_proxy_result(&state, &result)?;
    let response = proxy_core_response_to_proxy_response(result.response)?;

    process_response(response, &ctx, &state, &GEMINI_PARSER_CONFIG, None).await
}

#[cfg(test)]
mod tests {
    use crate::proxy::error::ProxyError;
    use crate::proxy::error_mapper::codex_proxy_error_json;

    #[test]
    fn codex_proxy_forward_error_includes_context_and_cause() {
        let error = ProxyError::ForwardFailed("连接失败: dns lookup failed".to_string());
        let body = codex_proxy_error_json("DeepSeek", "deepseek-chat", "/responses", &error);

        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("CC Switch local proxy failed"));
        assert!(message.contains("DeepSeek"));
        assert!(message.contains("deepseek-chat"));
        assert!(message.contains("/responses"));
        assert!(message.contains("dns lookup failed"));
        assert_eq!(body["error"]["code"], "cc_switch_forward_failed");
        assert_eq!(body["error"]["provider"], "DeepSeek");
        assert_eq!(body["error"]["model"], "deepseek-chat");
    }

    #[test]
    fn codex_proxy_upstream_error_normalizes_nonstandard_body() {
        let error = ProxyError::UpstreamError {
            status: 502,
            body: Some(
                r#"{"base_resp":{"status_code":2013,"status_msg":"upstream gateway failed"}}"#
                    .to_string(),
            ),
        };
        let body = codex_proxy_error_json("MiniMax", "abab6.5s", "/responses", &error);

        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("upstream_status: HTTP 502"));
        assert!(message.contains("upstream gateway failed"));
        assert_eq!(body["error"]["code"], 2013);
        assert_eq!(body["error"]["upstream_status"], 502);
    }

    #[test]
    fn codex_proxy_413_points_to_upstream_not_local_proxy() {
        let error = ProxyError::UpstreamError {
            status: 413,
            body: Some(
                "<html>\r\n<head><title>413 Request Entity Too Large</title></head>\r\n\
                 <body>\r\n<center><h1>413 Request Entity Too Large</h1></center>\r\n\
                 <hr><center>nginx/1.29.6</center>\r\n</body>\r\n</html>"
                    .to_string(),
            ),
        };
        let body = codex_proxy_error_json("HCAI", "gpt-5.5", "/responses", &error);

        let message = body["error"]["message"].as_str().unwrap();
        assert!(!message.contains("CC Switch local proxy failed"));
        assert!(message.contains("413"));
        assert!(message.to_lowercase().contains("upstream"));
        assert!(message.contains("/compact"));
        assert!(!message.contains("<html>"));
        assert!(!message.contains("nginx/1.29.6"));
        assert_eq!(body["error"]["upstream_status"], 413);
        assert_eq!(body["error"]["provider"], "HCAI");
        assert_eq!(body["error"]["model"], "gpt-5.5");
        assert_eq!(body["error"]["endpoint"], "/responses");
    }
}
