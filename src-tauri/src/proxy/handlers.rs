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
    providers::{codex_chat_history::record_responses_sse_stream, get_adapter},
    response_adapter::{
        proxy_core_response_to_axum_response, proxy_core_response_to_proxy_response,
        proxy_event_envelope_to_axum_sse_event,
    },
    response_processor::{create_logged_passthrough_stream, process_response, read_decoded_body},
    server::ProxyState,
    usage_sink_bridge::{
        record_forward_error_usage, record_transformed_response_usage,
        transformed_streaming_usage_collector,
    },
};
use crate::app_config::AppType;
use crate::proxy_core_adapter::{
    append_query_to_endpoint_path, AppChannelListSource, AppChannelManagementPlan,
    chat_completion_to_response_with_context as build_chat_completion_response_with_context,
    claude_stream_usage_event_filter, claude_transform_unlabeled_sse_aggregation,
    codex_stream_usage_event_filter,
    create_codex_chat_to_responses_sse_stream_with_context as create_responses_sse_stream_from_chat_with_context,
    create_gemini_to_anthropic_sse_stream_with_callbacks as create_anthropic_sse_stream_from_gemini,
    create_openai_chat_to_anthropic_sse_stream as create_anthropic_sse_stream,
    create_openai_responses_to_anthropic_sse_stream as create_anthropic_sse_stream_from_responses,
    extract_anthropic_tool_schema_hints, extract_gemini_model_from_path,
    gemini_response_to_anthropic_message_with_shadow, openai_chat_to_anthropic_message,
    openai_responses_to_anthropic_message, parse_upstream_json_or_unlabeled_sse, plan_channel_test,
    rebuilt_json_proxy_response, resolve_management_auth_decision,
    should_aggregate_codex_oauth_responses_sse, should_use_claude_transform_streaming,
    strip_endpoint_prefix, transformed_sse_proxy_response, validate_management_bearer_header,
    AppChannelListQuery, AppChannelManagementRequest, AppChannelResponse, AppKind, AppListRequest,
    AppListResponse, AppListSource, AppModelCatalogRequest, AppModelListQuery, ChannelCreateRequest,
    ChannelCreateSource, ChannelDeleteResponse, ChannelDeleteSource, ChannelHealthResetResponse,
    ChannelHealthResetSource, ChannelListPlan, ChannelListQuery, ChannelListRequest,
    ChannelListResponse, ChannelListSource,
    ChannelMigrationMaterializeResponse, ChannelMigrationPreviewResponse,
    ChannelMigrationMaterializeSource, ChannelMigrationPreviewSource, ChannelModelRecord,
    ChannelModelsResponse, ChannelModelsSource, ChannelPathRequest, ChannelRecord,
    ChannelRecordResponse, ChannelRecordSource, ChannelRouteCandidate, ChannelRouteRejected,
    ChannelTestPlan, ChannelTestResponse, ClaudeDesktopModelListResponse,
    ClientModelCatalogResponse, CurrentRouteResponse, CurrentRouteSource, CurrentRouteTarget,
    CodexToolContext, GroupListChannelSource, GroupListQuery, GroupListRequest, HealthCheckRequest,
    HealthCheckResponse, HealthCheckSource, InterfaceKind, ManagementAppPathRequest,
    ManagementAuthDecision, ProviderListResponse, ProviderListSource, ProxyBody,
    ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest, ProxyChannelTestRequest,
    ProxyChannelWriteRequest, ProxyRequest, ProxyRuntimeStatus, ProxyStatusRequest,
    ProxyStatusResponse, ProxyStatusSource, RoutableModelList, RouteGroupListResponse,
    RouteResolveManagementRequest, RouteResolveRequest, RouteResolveResponse,
    TransformedResponseUsageFormat,
    UnlabeledSseFallbackLogContext, UnlabeledSseFallbackLogLevel, UpstreamSseAggregationKind,
    CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG, OPENAI_PARSER_CONFIG,
};
use crate::proxy_core_adapter::{
    proxy_app_summary_input, proxy_channel_model_records_to_core, proxy_channel_record_to_core,
    proxy_channel_record_to_core_spec, proxy_channel_records_to_core, proxy_channel_specs_to_core,
    proxy_current_route_provider_summary_input, proxy_providers_to_core_specs,
    stream_check_result_to_channel_reachability, synthesize_gemini_tool_call_id_with_uuid,
};
use crate::services::stream_check::StreamCheckService;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde_json::Value;
use std::convert::Infallible;
use std::str::FromStr;
use std::time::Duration;

// ============================================================================
// 健康检查和状态查询（简单端点）
// ============================================================================

/// 健康检查
pub async fn health_check() -> (StatusCode, Json<HealthCheckResponse>) {
    let request = HealthCheckRequest::new();
    (
        StatusCode::OK,
        Json(request.response_from_source(HealthCheckSource::new(
            chrono::Utc::now().to_rfc3339(),
        ))),
    )
}

/// 获取服务状态
pub async fn get_status(
    State(state): State<ProxyState>,
) -> Result<Json<ProxyStatusResponse<ProxyRuntimeStatus>>, ProxyError> {
    let request = ProxyStatusRequest::new();
    let status = state.status.read().await.clone();
    Ok(Json(
        request.response_from_source(ProxyStatusSource::new(status)),
    ))
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
        let fallback_token = std::env::var("CC_SWITCH_PROXY_MANAGEMENT_TOKEN").ok();
        resolve_management_auth_decision(
            &config.listen_address,
            config.management_auth_token.as_deref(),
            fallback_token.as_deref(),
        )
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
    let mut apps = Vec::new();

    for app in AppType::all() {
        let app_type = app.as_str();
        let config = state
            .db
            .get_proxy_config_for_app(app_type)
            .await
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
        let providers = state
            .db
            .get_all_providers(app_type)
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
        let channels = state
            .db
            .list_proxy_channels_for_app(app_type)
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;

        apps.push(proxy_app_summary_input(
            &app,
            config.enabled,
            config.auto_failover_enabled,
            providers.into_values(),
            channels,
        ));
    }

    Ok(Json(request.response_from_source(AppListSource::new(apps))))
}

/// GET /proxy/v1/apps/{app}/providers
pub async fn list_proxy_providers(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<ProviderListResponse>, ProxyError> {
    let request = ManagementAppPathRequest::from_path(app_type)
        .map_err(management_api_error_to_proxy_error)?;
    let app = request
        .app_type
        .parse::<AppType>()
        .map_err(|e| ProxyError::InvalidRequest(e.to_string()))?;

    let providers = state
        .db
        .get_all_providers(&request.app_type)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
    let current_provider = state
        .db
        .get_current_provider(&request.app_type)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
    let failover_queue = state
        .db
        .get_failover_queue(&request.app_type)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
    let failover_ids: Vec<String> = failover_queue
        .into_iter()
        .map(|item| item.provider_id)
        .collect();

    let route_candidate_ids: Vec<String> = match state
        .provider_router
        .select_providers(&request.app_type)
        .await
    {
        Ok(selected) => selected.into_iter().map(|provider| provider.id).collect(),
        Err(crate::error::AppError::NoProvidersConfigured)
        | Err(crate::error::AppError::AllProvidersCircuitOpen) => Vec::new(),
        Err(e) => return Err(ProxyError::DatabaseError(e.to_string())),
    };

    let provider_specs = proxy_providers_to_core_specs(&app, providers.into_values());

    Ok(Json(request.provider_list_response_from_source(
        ProviderListSource::new(
            provider_specs,
            current_provider,
            failover_ids,
            route_candidate_ids,
        ),
    )))
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
    let channels = match request.plan() {
        ChannelListPlan::App { app_type } => state
            .db
            .list_proxy_channels_for_app(&app_type)
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?,
        ChannelListPlan::All => state
            .db
            .list_all_proxy_channels()
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?,
    };

    Ok(Json(request.response_from_source(ChannelListSource::new(
        proxy_channel_records_to_core(channels),
    ))))
}

/// POST /proxy/v1/channels
pub async fn create_proxy_channel(
    State(state): State<ProxyState>,
    Json(request): Json<ProxyChannelWriteRequest>,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    let request = ChannelCreateRequest::from_body(request);
    let channel = state
        .db
        .create_proxy_channel(request.clone().into_body())
        .map_err(|e| ProxyError::InvalidRequest(e.to_string()))?;
    Ok(Json(
        request.record_response_from_source(ChannelCreateSource::new(proxy_channel_record_to_core(
            channel,
        ))),
    ))
}

/// GET /proxy/v1/channels/{channel_id}
pub async fn get_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    let request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let channel = state
        .db
        .get_proxy_channel(&request.channel_id)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?
        .map(proxy_channel_record_to_core);
    Ok(Json(
        request
            .record_response_from_source(ChannelRecordSource::new(channel))
            .map_err(management_api_error_to_proxy_error)?,
    ))
}

/// PATCH /proxy/v1/channels/{channel_id}
pub async fn update_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
    Json(request): Json<ProxyChannelPatchRequest>,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    let path_request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let channel = state
        .db
        .update_proxy_channel(&path_request.channel_id, request)
        .map_err(|e| ProxyError::InvalidRequest(e.to_string()))?
        .map(proxy_channel_record_to_core);
    Ok(Json(
        path_request
            .record_response_from_source(ChannelRecordSource::new(channel))
            .map_err(management_api_error_to_proxy_error)?,
    ))
}

/// DELETE /proxy/v1/channels/{channel_id}
pub async fn delete_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelDeleteResponse>, ProxyError> {
    let request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let deleted = state
        .db
        .delete_proxy_channel(&request.channel_id)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
    Ok(Json(
        request.delete_response_from_source(ChannelDeleteSource::new(deleted)),
    ))
}

/// GET /proxy/v1/channels/{channel_id}/models
pub async fn list_proxy_channel_models(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelModelsResponse<ChannelModelRecord>>, ProxyError> {
    let request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let channel_exists = state
        .db
        .get_proxy_channel(&request.channel_id)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?
        .is_some();

    let models = if channel_exists {
        Some(
            state
                .db
                .list_proxy_channel_models(&request.channel_id)
                .map_err(|e| ProxyError::DatabaseError(e.to_string()))?,
        )
    } else {
        None
    };
    Ok(Json(
        request
            .models_response_from_source(ChannelModelsSource::new(
                models.map(proxy_channel_model_records_to_core),
            ))
            .map_err(management_api_error_to_proxy_error)?,
    ))
}

/// PUT /proxy/v1/channels/{channel_id}/models
pub async fn replace_proxy_channel_models(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
    Json(request): Json<ProxyChannelModelsReplaceRequest>,
) -> Result<Json<ChannelModelsResponse<ChannelModelRecord>>, ProxyError> {
    let path_request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let models = state
        .db
        .replace_proxy_channel_models(&path_request.channel_id, request.models)
        .map_err(|e| ProxyError::InvalidRequest(e.to_string()))?
        .map(proxy_channel_model_records_to_core);

    Ok(Json(
        path_request
            .models_response_from_source(ChannelModelsSource::new(models))
            .map_err(management_api_error_to_proxy_error)?,
    ))
}

/// POST /proxy/v1/channels/{channel_id}/test
pub async fn test_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
    Json(request): Json<ProxyChannelTestRequest>,
) -> Result<Json<ChannelTestResponse>, ProxyError> {
    let path_request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let channel = state
        .db
        .get_proxy_channel(&path_request.channel_id)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?
        .ok_or_else(|| proxy_core_error_to_proxy_error(path_request.channel_not_found_error()))?;

    let channel_test_context = match plan_channel_test(
        &proxy_channel_record_to_core_spec(&channel),
        &request,
        chrono::Utc::now().timestamp(),
    ) {
        ChannelTestPlan::Probe(context) => context,
        ChannelTestPlan::Failure(response) => return Ok(Json(response)),
    };

    let probe_request = channel_test_context.probe_request();

    let app_type = AppType::from_str(&probe_request.app_type)
        .map_err(|error| ProxyError::InvalidRequest(error.to_string()))?;
    let provider = state
        .db
        .get_provider_by_id(&probe_request.provider_id, &probe_request.app_type)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?
        .ok_or_else(|| ProxyError::ConfigError(probe_request.provider_not_found_message()))?;
    let config = state
        .db
        .get_stream_check_config()
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;

    let result = StreamCheckService::check_with_retry(
        &app_type,
        &provider,
        &config,
        Some(probe_request.base_url.clone()),
    )
    .await
    .map_err(|e| ProxyError::Internal(e.to_string()))?;

    Ok(Json(channel_test_context.reachability_response(
        stream_check_result_to_channel_reachability(result),
    )))
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

    match request.plan() {
        AppChannelManagementPlan::Route(route_request) => {
            let response = state
                .provider_router
                .resolve_channel_route_dry_run(route_request)
                .await
                .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;

            Ok(Json(request.response_from_route_resolution(response)))
        }
        AppChannelManagementPlan::List { app_type } => {
            let (channels, source) = state
                .provider_router
                .list_channels_for_app(&app_type)
                .await
                .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;

            Ok(Json(
                request.response_from_list_source(AppChannelListSource::new(
                    source,
                    proxy_channel_records_to_core(channels),
                )),
            ))
        }
    }
}

/// GET /proxy/v1/groups
pub async fn list_proxy_groups(
    State(state): State<ProxyState>,
    Query(query): Query<GroupListQuery>,
) -> Result<Json<RouteGroupListResponse>, ProxyError> {
    let request =
        GroupListRequest::from_query(query).map_err(management_api_error_to_proxy_error)?;
    let app_types = request.app_scope(AppType::all().map(|app| app.as_str().to_string()));

    let mut sources = Vec::new();

    for app_type in &app_types {
        let (channels, source) = state
            .provider_router
            .list_channels_for_app(app_type)
            .await
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
        sources.push(GroupListChannelSource::new(
            app_type.clone(),
            source,
            proxy_channel_specs_to_core(channels),
        ));
    }

    Ok(Json(request.response_from_channel_sources(sources)))
}

/// GET /proxy/v1/apps/{app}/routes/current
pub async fn get_current_proxy_route(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<CurrentRouteResponse<CurrentRouteTarget>>, ProxyError> {
    let request = ManagementAppPathRequest::from_path(app_type)
        .map_err(management_api_error_to_proxy_error)?;
    let app = request
        .app_type
        .parse::<AppType>()
        .map_err(|e| ProxyError::InvalidRequest(e.to_string()))?;

    let active_target = {
        let current_providers = state.current_providers.read().await;
        current_providers.get(&request.app_type).cloned()
    };

    let configured_provider = match state
        .db
        .get_current_provider(&request.app_type)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?
    {
        Some(provider_id) => state
            .db
            .get_provider_by_id(&provider_id, &request.app_type)
            .map_err(|e| ProxyError::DatabaseError(e.to_string()))?
            .map(|provider| proxy_current_route_provider_summary_input(provider, &app)),
        None => None,
    };

    Ok(Json(
        request.current_route_response_from_source(CurrentRouteSource::new(
            active_target,
            configured_provider,
        )),
    ))
}

/// GET /proxy/v1/apps/{app}/channels/migration/preview
pub async fn preview_proxy_channel_migration(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<ChannelMigrationPreviewResponse<ChannelRecord>>, ProxyError> {
    let request = ManagementAppPathRequest::from_path(app_type)
        .map_err(management_api_error_to_proxy_error)?;

    let preview = state
        .db
        .preview_legacy_proxy_channel_migration(&request.app_type)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;

    Ok(Json(request.migration_preview_response_from_source(
        ChannelMigrationPreviewSource::new(
            proxy_channel_records_to_core(preview.channels),
            preview.duplicate_count,
            preview.needs_review_count,
        ),
    )))
}

/// POST /proxy/v1/apps/{app}/channels/migration/materialize
pub async fn materialize_proxy_channel_migration(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<ChannelMigrationMaterializeResponse>, ProxyError> {
    let request = ManagementAppPathRequest::from_path(app_type)
        .map_err(management_api_error_to_proxy_error)?;

    let result = state
        .db
        .materialize_legacy_proxy_channels(&request.app_type)
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;

    Ok(Json(request.migration_materialize_response_from_source(
        ChannelMigrationMaterializeSource::new(
            result.previewed_channels,
            result.inserted_channels,
            result.inserted_models,
            result.inserted_health_rows,
            result.duplicate_count,
            result.needs_review_count,
        ),
    )))
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
        .reset_channel_health_response(&request.channel_id)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(
        request.health_reset_response_from_source(ChannelHealthResetSource::new(response)),
    ))
}

/// POST /proxy/v1/route/resolve
pub async fn resolve_proxy_route(
    State(state): State<ProxyState>,
    Json(request): Json<RouteResolveRequest>,
) -> Result<Json<RouteResolveResponse>, ProxyError> {
    let request = RouteResolveManagementRequest::from_body(request)
        .map_err(management_api_error_to_proxy_error)?;

    let response = state
        .provider_router
        .resolve_channel_route_dry_run(request.request.clone())
        .await
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;

    Ok(Json(request.response_from_resolution(response)))
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
    let providers = state
        .provider_router
        .select_providers("claude-desktop")
        .await
        .map_err(|e| ProxyError::DatabaseError(e.to_string()))?;
    let provider = providers.first().ok_or(ProxyError::NoAvailableProvider)?;
    let response = crate::claude_desktop_config::model_list_response(provider)
        .map_err(|e| ProxyError::ConfigError(e.to_string()))?;
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
    let body_bytes = body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, app_type.clone(), tag, app_type_str).await?;

    let raw_endpoint = append_query_to_endpoint_path(uri.path(), uri.query());
    let endpoint = strip_endpoint_prefix(&raw_endpoint, strip_prefix);

    let is_stream = body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    let mut proxy_request = ProxyRequest::new(
        AppKind::from(&app_type),
        method,
        endpoint,
        InterfaceKind::AnthropicMessages,
        ProxyBody::Json(body.clone()),
    );
    proxy_request.requested_model = Some(ctx.request_model.clone());
    proxy_request.headers = headers;
    proxy_request.extensions = extensions;

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
    let api_format = ctx.claude_api_format_for_proxy_result(&result);
    let response = proxy_core_response_to_proxy_response(result.response)?;

    // 检查是否需要格式转换（OpenRouter 等中转服务）
    let adapter = get_adapter(&app_type);
    let needs_transform = adapter.needs_transform(&ctx.provider);

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
    let is_codex_oauth = ctx
        .provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        == Some("codex_oauth");
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
            response.is_sse(),
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
                Some(ctx.provider.id.clone()),
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
            Some(&ctx.provider.id),
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
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex").await?;
    let endpoint = append_query_to_endpoint_path("/chat/completions", uri.query());

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut proxy_request = ProxyRequest::new(
        AppKind::from(&AppType::Codex),
        method,
        &endpoint,
        InterfaceKind::OpenAiChatCompletions,
        ProxyBody::Json(body),
    );
    proxy_request.requested_model = Some(ctx.request_model.clone());
    proxy_request.headers = headers;
    proxy_request.extensions = extensions;

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
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex").await?;
    let endpoint = append_query_to_endpoint_path("/responses", uri.query());

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let codex_tool_context = crate::proxy_core_adapter::codex_tool_context_from_request(&body);

    let mut proxy_request = ProxyRequest::new(
        AppKind::from(&AppType::Codex),
        method,
        &endpoint,
        InterfaceKind::OpenAiResponses,
        ProxyBody::Json(body),
    );
    proxy_request.requested_model = Some(ctx.request_model.clone());
    proxy_request.headers = headers;
    proxy_request.extensions = extensions;

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

    if super::providers::should_convert_codex_responses_to_chat(&ctx.provider, &endpoint) {
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
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?;

    let mut ctx =
        RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex").await?;
    let endpoint = append_query_to_endpoint_path("/responses/compact", uri.query());

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let codex_tool_context = crate::proxy_core_adapter::codex_tool_context_from_request(&body);

    let mut proxy_request = ProxyRequest::new(
        AppKind::from(&AppType::Codex),
        method,
        &endpoint,
        InterfaceKind::OpenAiResponses,
        ProxyBody::Json(body),
    );
    proxy_request.requested_model = Some(ctx.request_model.clone());
    proxy_request.headers = headers;
    proxy_request.extensions = extensions;

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

    if super::providers::should_convert_codex_responses_to_chat(&ctx.provider, &endpoint) {
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

    if is_stream || response.is_sse() {
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
    let response =
        codex_proxy_error_response(&ctx.provider.name, &ctx.request_model, endpoint, error)
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
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed to read request body: {e}")))?
        .to_bytes();
    // GET 类只读端点（/v1beta/models、/v1beta/models/<model> 等）没有请求体，
    // 不能强制 parse 为 JSON —— 否则空 body 会被拒绝。
    let body: Value = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes)
            .map_err(|e| ProxyError::Internal(format!("Failed to parse request body: {e}")))?
    };

    // Gemini 的模型名称在 URI 中
    let mut ctx = RequestContext::new(&state, &body, &headers, AppType::Gemini, "Gemini", "gemini")
        .await?
        .with_model_from_uri(&uri);

    // 提取完整的路径和查询参数
    let endpoint = append_query_to_endpoint_path(uri.path(), uri.query());

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut proxy_request = ProxyRequest::new(
        AppKind::from(&AppType::Gemini),
        method,
        &endpoint,
        InterfaceKind::GeminiNative,
        ProxyBody::Json(body),
    );
    proxy_request.requested_model = extract_gemini_model_from_path(&endpoint);
    proxy_request.headers = headers;
    proxy_request.extensions = extensions;

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
