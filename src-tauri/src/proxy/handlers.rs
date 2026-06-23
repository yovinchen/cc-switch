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
        claude_response_transform_error_to_proxy_error,
        codex_chat_to_responses_transform_error_to_proxy_error,
        parse_claude_transform_upstream_json_or_unlabeled_sse,
        parse_codex_chat_upstream_json_or_unlabeled_sse, management_api_error_to_proxy_error,
        management_auth_error_to_proxy_error, proxy_core_error_to_proxy_error,
    },
    forwarder::ActiveConnectionGuard,
    handler_context::RequestContext,
    response_adapter::{
        claude_transformed_json_response_to_axum_response,
        claude_transformed_sse_response_to_axum_response, collect_axum_request_body,
        codex_chat_error_response_to_axum_response, codex_proxy_error_to_axum_response,
        codex_transformed_json_response_to_axum_response,
        codex_transformed_sse_response_to_axum_response, proxy_core_response_to_proxy_response,
        proxy_event_envelope_to_axum_sse_event,
    },
    response_processor::{process_response, read_decoded_body},
};
use crate::app_config::AppType;
use crate::proxy_core_adapter::{
    append_query_to_endpoint_path,
    claude_transformed_streaming_usage_collector, codex_auto_transformed_streaming_usage_collector,
    create_logged_passthrough_stream,
    codex_chat_transform_streaming_decision, extract_anthropic_tool_schema_hints,
    extract_gemini_model_from_path, json_proxy_request_from_input, JsonProxyRequestInput,
    parse_json_proxy_request_body,
    parse_json_proxy_request_body_or_null, ProxyState,
    management_auth_decision_from_proxy_config, record_forward_core_error_usage,
    record_claude_transformed_response_usage, record_codex_auto_transformed_response_usage,
    transform_codex_chat_response_with_history, transform_codex_chat_sse_with_history,
    provider_claude_transform_response_for_api_format,
    provider_claude_transform_sse_for_api_format,
    provider_claude_transform_streaming_decision, provider_needs_claude_transform,
    provider_should_convert_codex_responses_to_chat,
    strip_endpoint_prefix,
    validate_management_bearer_header,
    AppChannelListQuery, AppChannelManagementRequest, AppChannelResponse, AppKind, AppListRequest,
    AppListResponse, AppModelCatalogRequest, AppModelListQuery, ChannelCreateRequest,
    ChannelDeleteResponse, ChannelHealthResetResponse,
    ChannelKeyDeleteResponse, ChannelKeyPathRequest, ChannelKeyRecord, ChannelKeyRecordResponse,
    ChannelKeysResponse, ChannelListQuery, ChannelListRequest, ChannelListResponse,
    ChannelMigrationMaterializeResponse,
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
    CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG, OPENAI_PARSER_CONFIG,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
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
            let error = record_forward_core_error_usage(&state, &ctx, is_stream, error);
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
    let streaming_decision = provider_claude_transform_streaming_decision(
        provider,
        is_stream,
        response.headers(),
        api_format,
    );
    let tool_schema_hints = extract_anthropic_tool_schema_hints(original_body);
    let tool_schema_hints = (!tool_schema_hints.is_empty()).then_some(tool_schema_hints);

    if streaming_decision.use_streaming {
        let stream = response.bytes_stream();
        let sse_stream = provider_claude_transform_sse_for_api_format(
            stream,
            api_format,
            Some(state.gemini_shadow.clone()),
            Some(provider.id.clone()),
            Some(ctx.session_id.clone()),
            tool_schema_hints.clone(),
        );

        // 创建使用量收集器；关闭 usage logging 时不要再解析转换后的 SSE。
        let usage_collector =
            claude_transformed_streaming_usage_collector(state, ctx, status.as_u16());

        // 获取流式超时配置
        let timeout_config = ctx.streaming_timeout_config();

        let logged_stream = create_logged_passthrough_stream(
            sse_stream,
            "Claude/OpenRouter",
            usage_collector,
            timeout_config,
            connection_guard,
        );

        return claude_transformed_sse_response_to_axum_response(logged_stream);
    }

    // 非流式响应转换 (OpenAI/Responses → Anthropic)
    let (response_headers, _status, body_bytes) =
        read_decoded_body(response, ctx.tag, ctx.body_timeout_duration()).await?;

    let upstream_response = parse_claude_transform_upstream_json_or_unlabeled_sse(
        body_bytes.as_ref(),
        &response_headers,
        streaming_decision.response_sse_aggregation,
        api_format,
        streaming_decision.aggregate_codex_oauth_responses_sse,
    )?;

    let anthropic_response = provider_claude_transform_response_for_api_format(
        &upstream_response,
        api_format,
        Some(state.gemini_shadow.as_ref()),
        Some(&provider.id),
        Some(&ctx.session_id),
        tool_schema_hints.as_ref(),
    )
    .map_err(claude_response_transform_error_to_proxy_error)?;

    record_claude_transformed_response_usage(state, ctx, &anthropic_response, status.as_u16());

    claude_transformed_json_response_to_axum_response(status, response_headers, anthropic_response)
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
            let error = record_forward_core_error_usage(&state, &ctx, is_stream, error);
            return codex_proxy_error_to_axum_response(
                ctx.provider_name_for_error(),
                &ctx.request_model,
                &endpoint,
                &error,
            );
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
            let error = record_forward_core_error_usage(&state, &ctx, is_stream, error);
            return codex_proxy_error_to_axum_response(
                ctx.provider_name_for_error(),
                &ctx.request_model,
                &endpoint,
                &error,
            );
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
            let error = record_forward_core_error_usage(&state, &ctx, is_stream, error);
            return codex_proxy_error_to_axum_response(
                ctx.provider_name_for_error(),
                &ctx.request_model,
                &endpoint,
                &error,
            );
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

    let streaming_decision =
        codex_chat_transform_streaming_decision(is_stream, response.headers());

    if streaming_decision.use_streaming {
        let stream = response.bytes_stream();
        let sse_stream = transform_codex_chat_sse_with_history(
            stream,
            tool_context,
            state.codex_chat_history.clone(),
        );

        let usage_collector =
            codex_auto_transformed_streaming_usage_collector(state, ctx, status.as_u16());

        let logged_stream = create_logged_passthrough_stream(
            sse_stream,
            ctx.tag,
            usage_collector,
            ctx.streaming_timeout_config(),
            connection_guard,
        );

        return codex_transformed_sse_response_to_axum_response(logged_stream);
    }

    let _connection_guard = connection_guard;
    let (response_headers, status, body_bytes) =
        read_decoded_body(response, ctx.tag, ctx.body_timeout_duration()).await?;
    // 与 Claude 侧 handle_claude_transform 对称的兜底嗅探（#2234）：
    // 上游对 stream:false 返回未标记 Content-Type 的 SSE 体时按 Chat SSE 聚合。
    let chat_response = parse_codex_chat_upstream_json_or_unlabeled_sse(
        body_bytes.as_ref(),
        &response_headers,
        streaming_decision.response_sse_aggregation,
    )?;
    let responses_response = transform_codex_chat_response_with_history(
        &chat_response,
        &tool_context,
        &state.codex_chat_history,
    )
    .await
    .map_err(codex_chat_to_responses_transform_error_to_proxy_error)?;

    record_codex_auto_transformed_response_usage(state, ctx, &responses_response, status.as_u16());

    codex_transformed_json_response_to_axum_response(status, response_headers, responses_response)
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

    codex_chat_error_response_to_axum_response(status, response_headers, &body_bytes)
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
            let error = record_forward_core_error_usage(&state, &ctx, is_stream, error);
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
