//! 请求处理器
//!
//! 处理各种API端点的HTTP请求
//!
//! 重构后的结构：
//! - 协议请求编排由 `response_adapter` 承接
//! - HTTP handler 只保留 Axum 提取、鉴权和管理 API 转发

use super::{
    auth_adapter::{validate_claude_desktop_gateway_auth, validate_proxy_management_auth},
    error::ProxyError,
    response_adapter::{
        dispatch_claude_desktop_messages_request_to_axum_response,
        dispatch_claude_desktop_models_request_to_axum_json_response,
        dispatch_claude_request_to_axum_response, dispatch_codex_chat_request_to_axum_response,
        dispatch_codex_client_model_catalog_request_to_axum_json_response,
        dispatch_codex_responses_compact_request_to_axum_response,
        dispatch_codex_responses_request_to_axum_response,
        dispatch_create_proxy_channel_request_to_axum_json_response,
        dispatch_current_proxy_route_request_to_axum_json_response,
        dispatch_delete_proxy_channel_key_request_to_axum_json_response,
        dispatch_delete_proxy_channel_request_to_axum_json_response,
        dispatch_gemini_request_to_axum_response,
        dispatch_get_proxy_channel_request_to_axum_json_response,
        dispatch_materialize_proxy_channel_migration_request_to_axum_json_response,
        dispatch_preview_proxy_channel_migration_request_to_axum_json_response,
        dispatch_proxy_app_channels_request_to_axum_json_response,
        dispatch_proxy_app_models_request_to_axum_json_response,
        dispatch_proxy_apps_request_to_axum_json_response,
        dispatch_proxy_channel_breaker_stats_request_to_axum_json_response,
        dispatch_proxy_channel_keys_request_to_axum_json_response,
        dispatch_proxy_channel_models_request_to_axum_json_response,
        dispatch_proxy_channel_test_request_to_axum_json_response,
        dispatch_proxy_channels_request_to_axum_json_response,
        dispatch_proxy_groups_request_to_axum_json_response,
        dispatch_proxy_providers_request_to_axum_json_response,
        dispatch_proxy_route_resolve_request_to_axum_json_response,
        dispatch_proxy_status_request_to_axum_json_response,
        dispatch_replace_proxy_channel_models_request_to_axum_json_response,
        dispatch_reset_proxy_channel_breaker_request_to_axum_json_response,
        dispatch_update_proxy_channel_key_request_to_axum_json_response,
        dispatch_update_proxy_channel_request_to_axum_json_response,
        dispatch_upsert_proxy_channel_key_request_to_axum_json_response,
        proxy_events_request_to_axum_sse_response, proxy_health_check_to_axum_json_response,
    },
};
use crate::proxy_core_adapter::{
    AppChannelListQuery, AppChannelResponse, AppListResponse, AppModelListQuery,
    ChannelBreakerStatsResponse, ChannelDeleteResponse, ChannelHealthResetResponse,
    ChannelKeyDeleteResponse, ChannelKeyRecord, ChannelKeyRecordResponse, ChannelKeysResponse,
    ChannelListQuery, ChannelListResponse, ChannelMigrationMaterializeResponse,
    ChannelMigrationPreviewResponse, ChannelModelRecord, ChannelModelsResponse, ChannelRecord,
    ChannelRecordResponse, ChannelRouteCandidate, ChannelRouteRejected, ChannelTestResponse,
    ClaudeDesktopModelListResponse, ClientModelCatalogResponse, CurrentRouteResponse,
    CurrentRouteTarget, GroupListQuery, HealthCheckResponse, ProviderListResponse,
    ProxyChannelKeyPatchRequest, ProxyChannelKeyWriteRequest, ProxyChannelModelsReplaceRequest,
    ProxyChannelPatchRequest, ProxyChannelTestRequest, ProxyChannelWriteRequest,
    ProxyRuntimeStatus, ProxyState, ProxyStatusResponse, RoutableModelList, RouteGroupListResponse,
    RouteResolveRequest, RouteResolveResponse,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

// ============================================================================
// 健康检查和状态查询（简单端点）
// ============================================================================

/// 健康检查
pub async fn health_check() -> (StatusCode, Json<HealthCheckResponse>) {
    proxy_health_check_to_axum_json_response()
}

/// 获取服务状态
pub async fn get_status(
    State(state): State<ProxyState>,
) -> Result<Json<ProxyStatusResponse<ProxyRuntimeStatus>>, ProxyError> {
    dispatch_proxy_status_request_to_axum_json_response(&state).await
}

/// GET /proxy/v1/events
pub async fn stream_proxy_events(
    State(state): State<ProxyState>,
) -> impl axum::response::IntoResponse {
    proxy_events_request_to_axum_sse_response(&state)
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
    validate_proxy_management_auth(&state, request.headers()).await?;

    Ok(next.run(request).await)
}

/// GET /proxy/v1/apps
pub async fn list_proxy_apps(
    State(state): State<ProxyState>,
) -> Result<Json<AppListResponse>, ProxyError> {
    dispatch_proxy_apps_request_to_axum_json_response(&state).await
}

/// GET /proxy/v1/apps/{app}/providers
pub async fn list_proxy_providers(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<ProviderListResponse>, ProxyError> {
    dispatch_proxy_providers_request_to_axum_json_response(&state, app_type).await
}

/// GET /proxy/v1/apps/{app}/models
pub async fn list_proxy_app_models(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
    Query(query): Query<AppModelListQuery>,
) -> Result<Json<RoutableModelList>, ProxyError> {
    dispatch_proxy_app_models_request_to_axum_json_response(&state, app_type, query).await
}

/// GET /proxy/v1/channels
pub async fn list_all_proxy_channels(
    State(state): State<ProxyState>,
    Query(query): Query<ChannelListQuery>,
) -> Result<Json<ChannelListResponse<ChannelRecord>>, ProxyError> {
    dispatch_proxy_channels_request_to_axum_json_response(&state, query).await
}

/// POST /proxy/v1/channels
pub async fn create_proxy_channel(
    State(state): State<ProxyState>,
    Json(request): Json<ProxyChannelWriteRequest>,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    dispatch_create_proxy_channel_request_to_axum_json_response(&state, request).await
}

/// GET /proxy/v1/channels/{channel_id}
pub async fn get_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    dispatch_get_proxy_channel_request_to_axum_json_response(&state, channel_id).await
}

/// PATCH /proxy/v1/channels/{channel_id}
pub async fn update_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
    Json(request): Json<ProxyChannelPatchRequest>,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    dispatch_update_proxy_channel_request_to_axum_json_response(&state, channel_id, request).await
}

/// DELETE /proxy/v1/channels/{channel_id}
pub async fn delete_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelDeleteResponse>, ProxyError> {
    dispatch_delete_proxy_channel_request_to_axum_json_response(&state, channel_id).await
}

/// GET /proxy/v1/channels/{channel_id}/keys
pub async fn list_proxy_channel_keys(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelKeysResponse<ChannelKeyRecord>>, ProxyError> {
    dispatch_proxy_channel_keys_request_to_axum_json_response(&state, channel_id).await
}

/// PUT /proxy/v1/channels/{channel_id}/keys/{key_ref}
pub async fn upsert_proxy_channel_key(
    State(state): State<ProxyState>,
    Path((channel_id, key_ref)): Path<(String, String)>,
    Json(request): Json<ProxyChannelKeyWriteRequest>,
) -> Result<Json<ChannelKeyRecordResponse<ChannelKeyRecord>>, ProxyError> {
    dispatch_upsert_proxy_channel_key_request_to_axum_json_response(
        &state, channel_id, key_ref, request,
    )
    .await
}

/// PATCH /proxy/v1/channels/{channel_id}/keys/{key_ref}
pub async fn update_proxy_channel_key(
    State(state): State<ProxyState>,
    Path((channel_id, key_ref)): Path<(String, String)>,
    Json(request): Json<ProxyChannelKeyPatchRequest>,
) -> Result<Json<ChannelKeyRecordResponse<ChannelKeyRecord>>, ProxyError> {
    dispatch_update_proxy_channel_key_request_to_axum_json_response(
        &state, channel_id, key_ref, request,
    )
    .await
}

/// DELETE /proxy/v1/channels/{channel_id}/keys/{key_ref}
pub async fn delete_proxy_channel_key(
    State(state): State<ProxyState>,
    Path((channel_id, key_ref)): Path<(String, String)>,
) -> Result<Json<ChannelKeyDeleteResponse>, ProxyError> {
    dispatch_delete_proxy_channel_key_request_to_axum_json_response(&state, channel_id, key_ref)
        .await
}

/// GET /proxy/v1/channels/{channel_id}/models
pub async fn list_proxy_channel_models(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelModelsResponse<ChannelModelRecord>>, ProxyError> {
    dispatch_proxy_channel_models_request_to_axum_json_response(&state, channel_id).await
}

/// PUT /proxy/v1/channels/{channel_id}/models
pub async fn replace_proxy_channel_models(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
    Json(request): Json<ProxyChannelModelsReplaceRequest>,
) -> Result<Json<ChannelModelsResponse<ChannelModelRecord>>, ProxyError> {
    dispatch_replace_proxy_channel_models_request_to_axum_json_response(&state, channel_id, request)
        .await
}

/// POST /proxy/v1/channels/{channel_id}/test
pub async fn test_proxy_channel(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
    Json(request): Json<ProxyChannelTestRequest>,
) -> Result<Json<ChannelTestResponse>, ProxyError> {
    dispatch_proxy_channel_test_request_to_axum_json_response(&state, channel_id, request).await
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
    dispatch_proxy_app_channels_request_to_axum_json_response(&state, app_type, query).await
}

/// GET /proxy/v1/groups
pub async fn list_proxy_groups(
    State(state): State<ProxyState>,
    Query(query): Query<GroupListQuery>,
) -> Result<Json<RouteGroupListResponse>, ProxyError> {
    dispatch_proxy_groups_request_to_axum_json_response(&state, query).await
}

/// GET /proxy/v1/apps/{app}/routes/current
pub async fn get_current_proxy_route(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<CurrentRouteResponse<CurrentRouteTarget>>, ProxyError> {
    dispatch_current_proxy_route_request_to_axum_json_response(&state, app_type).await
}

/// GET /proxy/v1/apps/{app}/channels/migration/preview
pub async fn preview_proxy_channel_migration(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<ChannelMigrationPreviewResponse<ChannelRecord>>, ProxyError> {
    dispatch_preview_proxy_channel_migration_request_to_axum_json_response(&state, app_type).await
}

/// POST /proxy/v1/apps/{app}/channels/migration/materialize
pub async fn materialize_proxy_channel_migration(
    State(state): State<ProxyState>,
    Path(app_type): Path<String>,
) -> Result<Json<ChannelMigrationMaterializeResponse>, ProxyError> {
    dispatch_materialize_proxy_channel_migration_request_to_axum_json_response(&state, app_type)
        .await
}

/// GET /proxy/v1/channels/{channel_id}/breakers/stats
pub async fn get_proxy_channel_breaker_stats(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelBreakerStatsResponse>, ProxyError> {
    dispatch_proxy_channel_breaker_stats_request_to_axum_json_response(&state, channel_id).await
}

/// POST /proxy/v1/channels/{channel_id}/breakers/reset
pub async fn reset_proxy_channel_breaker(
    State(state): State<ProxyState>,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelHealthResetResponse>, ProxyError> {
    dispatch_reset_proxy_channel_breaker_request_to_axum_json_response(&state, channel_id).await
}

/// POST /proxy/v1/route/resolve
pub async fn resolve_proxy_route(
    State(state): State<ProxyState>,
    Json(request): Json<RouteResolveRequest>,
) -> Result<Json<RouteResolveResponse>, ProxyError> {
    dispatch_proxy_route_resolve_request_to_axum_json_response(&state, request).await
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
    dispatch_codex_client_model_catalog_request_to_axum_json_response(&state).await
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
    dispatch_claude_request_to_axum_response(&state, request).await
}

pub async fn handle_claude_desktop_messages(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    validate_claude_desktop_gateway_auth(&state, request.headers()).await?;
    dispatch_claude_desktop_messages_request_to_axum_response(&state, request).await
}

pub async fn handle_claude_desktop_models(
    State(state): State<ProxyState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ClaudeDesktopModelListResponse>, ProxyError> {
    validate_claude_desktop_gateway_auth(&state, &headers).await?;
    dispatch_claude_desktop_models_request_to_axum_json_response(&state).await
}

// ============================================================================
// Codex API 处理器
// ============================================================================

/// 处理 /v1/chat/completions 请求（OpenAI Chat Completions API - Codex CLI）
pub async fn handle_chat_completions(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    dispatch_codex_chat_request_to_axum_response(&state, request).await
}

/// 处理 /v1/responses 请求（OpenAI Responses API - Codex CLI 透传）
pub async fn handle_responses(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    dispatch_codex_responses_request_to_axum_response(&state, request).await
}

/// 处理 /v1/responses/compact 请求（OpenAI Responses Compact API - Codex CLI 透传）
pub async fn handle_responses_compact(
    State(state): State<ProxyState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, ProxyError> {
    dispatch_codex_responses_compact_request_to_axum_response(&state, request).await
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
    dispatch_gemini_request_to_axum_response(&state, uri, request).await
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
