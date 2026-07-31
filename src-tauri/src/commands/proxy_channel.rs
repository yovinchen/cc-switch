//! 中转站（channel）管理相关的 Tauri 命令
//!
//! 这些命令把 `ProxyEngine` 的 channel 管理能力桥接给前端，等价于 HTTP `/proxy/v1/*`
//! 管理 API，但走 Tauri IPC，无需前端处理端口/鉴权/服务未启动等问题。
//!
//! 请求体直接复用 `proxy-core` 的 camelCase 请求 DTO；返回值统一为 `serde_json::Value`
//! （即 proxy-core 的 camelCase 响应 DTO 序列化结果），前端类型与 HTTP API 契约一致。

use crate::proxy_core::api::management::{
    ProxyChannelKeyPatchRequest, ProxyChannelKeyWriteRequest, ProxyChannelModelsReplaceRequest,
    ProxyChannelPatchRequest, ProxyChannelTestRequest, ProxyChannelWriteRequest,
    RouteResolveRequest,
};
use crate::store::AppState;
use serde_json::Value;

// ==================== Channel CRUD ====================

#[tauri::command]
pub async fn list_proxy_channels(
    state: tauri::State<'_, AppState>,
    app_type: Option<String>,
) -> Result<Value, String> {
    state.proxy_service.list_all_channels(app_type).await
}

#[tauri::command]
pub async fn create_proxy_channel(
    state: tauri::State<'_, AppState>,
    request: ProxyChannelWriteRequest,
) -> Result<Value, String> {
    state.proxy_service.create_channel(request).await
}

#[tauri::command]
pub async fn get_proxy_channel(
    state: tauri::State<'_, AppState>,
    channel_id: String,
) -> Result<Value, String> {
    state.proxy_service.get_channel(channel_id).await
}

#[tauri::command]
pub async fn update_proxy_channel(
    state: tauri::State<'_, AppState>,
    channel_id: String,
    request: ProxyChannelPatchRequest,
) -> Result<Value, String> {
    state
        .proxy_service
        .update_channel(channel_id, request)
        .await
}

#[tauri::command]
pub async fn delete_proxy_channel(
    state: tauri::State<'_, AppState>,
    channel_id: String,
) -> Result<Value, String> {
    state.proxy_service.delete_channel(channel_id).await
}

// ==================== Channel models ====================

#[tauri::command]
pub async fn list_proxy_channel_models(
    state: tauri::State<'_, AppState>,
    channel_id: String,
) -> Result<Value, String> {
    state.proxy_service.list_channel_models(channel_id).await
}

#[tauri::command]
pub async fn replace_proxy_channel_models(
    state: tauri::State<'_, AppState>,
    channel_id: String,
    request: ProxyChannelModelsReplaceRequest,
) -> Result<Value, String> {
    state
        .proxy_service
        .replace_channel_models(channel_id, request)
        .await
}

// ==================== Channel keys ====================

#[tauri::command]
pub async fn list_proxy_channel_keys(
    state: tauri::State<'_, AppState>,
    channel_id: String,
) -> Result<Value, String> {
    state.proxy_service.list_channel_keys(channel_id).await
}

#[tauri::command]
pub async fn upsert_proxy_channel_key(
    state: tauri::State<'_, AppState>,
    channel_id: String,
    key_ref: String,
    request: ProxyChannelKeyWriteRequest,
) -> Result<Value, String> {
    state
        .proxy_service
        .upsert_channel_key(channel_id, key_ref, request)
        .await
}

#[tauri::command]
pub async fn update_proxy_channel_key(
    state: tauri::State<'_, AppState>,
    channel_id: String,
    key_ref: String,
    request: ProxyChannelKeyPatchRequest,
) -> Result<Value, String> {
    state
        .proxy_service
        .update_channel_key(channel_id, key_ref, request)
        .await
}

#[tauri::command]
pub async fn delete_proxy_channel_key(
    state: tauri::State<'_, AppState>,
    channel_id: String,
    key_ref: String,
) -> Result<Value, String> {
    state
        .proxy_service
        .delete_channel_key(channel_id, key_ref)
        .await
}

// ==================== Channel test ====================

#[tauri::command]
pub async fn test_proxy_channel(
    state: tauri::State<'_, AppState>,
    channel_id: String,
    request: ProxyChannelTestRequest,
) -> Result<Value, String> {
    state.proxy_service.test_channel(channel_id, request).await
}

// ==================== App-scoped views ====================

#[tauri::command]
pub async fn list_proxy_app_channels(
    state: tauri::State<'_, AppState>,
    app_type: String,
    requested_model: Option<String>,
    interface_kind: Option<String>,
    route_group: Option<String>,
) -> Result<Value, String> {
    state
        .proxy_service
        .list_app_channels(app_type, requested_model, interface_kind, route_group)
        .await
}

#[tauri::command]
pub async fn get_current_proxy_route(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<Value, String> {
    state.proxy_service.current_route(app_type).await
}

#[tauri::command]
pub async fn list_proxy_route_groups(
    state: tauri::State<'_, AppState>,
    app_type: Option<String>,
) -> Result<Value, String> {
    state.proxy_service.list_groups(app_type).await
}

// ==================== Legacy migration ====================

#[tauri::command]
pub async fn preview_proxy_channel_migration(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<Value, String> {
    state.proxy_service.migration_preview(app_type).await
}

#[tauri::command]
pub async fn materialize_proxy_channel_migration(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<Value, String> {
    state.proxy_service.migration_materialize(app_type).await
}

// ==================== Route dry-run & breakers ====================

#[tauri::command]
pub async fn resolve_proxy_route(
    state: tauri::State<'_, AppState>,
    request: RouteResolveRequest,
) -> Result<Value, String> {
    state.proxy_service.resolve_route(request).await
}

#[tauri::command]
pub async fn get_proxy_channel_breaker_stats(
    state: tauri::State<'_, AppState>,
    channel_id: String,
) -> Result<Value, String> {
    state.proxy_service.channel_breaker_stats(channel_id).await
}

#[tauri::command]
pub async fn reset_proxy_channel_breaker(
    state: tauri::State<'_, AppState>,
    channel_id: String,
) -> Result<Value, String> {
    state.proxy_service.reset_channel_breaker(channel_id).await
}
