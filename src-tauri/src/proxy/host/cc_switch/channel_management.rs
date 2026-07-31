//! Channel 管理业务逻辑（宿主侧）。
//!
//! 中转站（channel）能力此前只暴露在 HTTP `/proxy/v1/*` 管理 API 上。本模块把同一套
//! `ProxyEngine` 调用包装成宿主方法，供 Tauri 命令层复用，让 CC Switch 前端无需直连
//! 本地 HTTP 服务即可管理 channel、模型映射、key、迁移、路由 dry-run 和熔断。
//!
//! 所有方法都直接消费 `proxy-core` 的请求/响应 DTO（camelCase，外部契约与 HTTP API
//! 完全一致），错误统一映射为 `String` 以符合 Tauri IPC 约定。

use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy_core::api::management::{
    AppChannelListQuery, AppChannelManagementRequest, ChannelCreateRequest, ChannelKeyPathRequest,
    ChannelListQuery, ChannelListRequest, ChannelPathRequest, GroupListQuery, GroupListRequest,
    ManagementAppPathRequest, ProxyChannelKeyPatchRequest, ProxyChannelKeyWriteRequest,
    ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest, ProxyChannelTestRequest,
    ProxyChannelWriteRequest, RouteResolveManagementRequest, RouteResolveRequest,
};

/// 把 `proxy-core` 错误映射为 Tauri IPC 友好的字符串。
fn err_to_string<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

/// Channel 管理入口：借用一个 `ProxyState`（运行中的共享态或临时构造），对外提供与
/// HTTP 管理 API 等价的 channel 操作。
pub(crate) struct ChannelManagement<'a> {
    state: &'a ProxyState,
}

impl<'a> ChannelManagement<'a> {
    pub(crate) fn new(state: &'a ProxyState) -> Self {
        Self { state }
    }

    // ---- Channel CRUD ---------------------------------------------------

    /// GET /proxy/v1/channels
    pub(crate) async fn list_all_channels(
        &self,
        app_type: Option<String>,
    ) -> Result<serde_json::Value, String> {
        let query = match app_type {
            Some(app_type) => ChannelListQuery::for_app(app_type),
            None => ChannelListQuery::all(),
        };
        let request = ChannelListRequest::from_query(query).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .channel_list_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// POST /proxy/v1/channels
    pub(crate) async fn create_channel(
        &self,
        request: ProxyChannelWriteRequest,
    ) -> Result<serde_json::Value, String> {
        let request = ChannelCreateRequest::from_body(request);
        let response = self
            .state
            .proxy_engine()
            .create_channel_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// GET /proxy/v1/channels/{channel_id}
    pub(crate) async fn get_channel(
        &self,
        channel_id: String,
    ) -> Result<serde_json::Value, String> {
        let request = ChannelPathRequest::from_path(channel_id).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .channel_record_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// PATCH /proxy/v1/channels/{channel_id}
    pub(crate) async fn update_channel(
        &self,
        channel_id: String,
        request: ProxyChannelPatchRequest,
    ) -> Result<serde_json::Value, String> {
        let path_request = ChannelPathRequest::from_path(channel_id).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .update_channel_response(path_request, request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// DELETE /proxy/v1/channels/{channel_id}
    pub(crate) async fn delete_channel(
        &self,
        channel_id: String,
    ) -> Result<serde_json::Value, String> {
        let request = ChannelPathRequest::from_path(channel_id).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .delete_channel_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    // ---- Channel models -------------------------------------------------

    /// GET /proxy/v1/channels/{channel_id}/models
    pub(crate) async fn list_channel_models(
        &self,
        channel_id: String,
    ) -> Result<serde_json::Value, String> {
        let request = ChannelPathRequest::from_path(channel_id).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .channel_models_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// PUT /proxy/v1/channels/{channel_id}/models
    pub(crate) async fn replace_channel_models(
        &self,
        channel_id: String,
        request: ProxyChannelModelsReplaceRequest,
    ) -> Result<serde_json::Value, String> {
        let path_request = ChannelPathRequest::from_path(channel_id).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .replace_channel_models_response(path_request, request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    // ---- Channel keys ---------------------------------------------------

    /// GET /proxy/v1/channels/{channel_id}/keys
    pub(crate) async fn list_channel_keys(
        &self,
        channel_id: String,
    ) -> Result<serde_json::Value, String> {
        let request = ChannelPathRequest::from_path(channel_id).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .channel_keys_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// PUT /proxy/v1/channels/{channel_id}/keys/{key_ref}
    pub(crate) async fn upsert_channel_key(
        &self,
        channel_id: String,
        key_ref: String,
        request: ProxyChannelKeyWriteRequest,
    ) -> Result<serde_json::Value, String> {
        let path_request =
            ChannelKeyPathRequest::from_path(channel_id, key_ref).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .upsert_channel_key_response(path_request, request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// PATCH /proxy/v1/channels/{channel_id}/keys/{key_ref}
    pub(crate) async fn update_channel_key(
        &self,
        channel_id: String,
        key_ref: String,
        request: ProxyChannelKeyPatchRequest,
    ) -> Result<serde_json::Value, String> {
        let path_request =
            ChannelKeyPathRequest::from_path(channel_id, key_ref).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .update_channel_key_response(path_request, request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// DELETE /proxy/v1/channels/{channel_id}/keys/{key_ref}
    pub(crate) async fn delete_channel_key(
        &self,
        channel_id: String,
        key_ref: String,
    ) -> Result<serde_json::Value, String> {
        let path_request =
            ChannelKeyPathRequest::from_path(channel_id, key_ref).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .delete_channel_key_response(path_request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    // ---- Channel test ---------------------------------------------------

    /// POST /proxy/v1/channels/{channel_id}/test
    pub(crate) async fn test_channel(
        &self,
        channel_id: String,
        request: ProxyChannelTestRequest,
    ) -> Result<serde_json::Value, String> {
        let path_request = ChannelPathRequest::from_path(channel_id).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .channel_test_response(path_request, request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    // ---- App-scoped views ----------------------------------------------

    /// GET /proxy/v1/apps/{app}/channels（list 或 route dry-run）
    pub(crate) async fn list_app_channels(
        &self,
        app_type: String,
        requested_model: Option<String>,
        interface_kind: Option<String>,
        route_group: Option<String>,
    ) -> Result<serde_json::Value, String> {
        let query = match (requested_model, interface_kind, route_group) {
            (Some(model), Some(interface), group) => {
                AppChannelListQuery::route(model, interface, group.unwrap_or_default())
            }
            _ => AppChannelListQuery::list(),
        };
        let request =
            AppChannelManagementRequest::from_parts(app_type, query).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .app_channel_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// GET /proxy/v1/apps/{app}/routes/current
    pub(crate) async fn current_route(
        &self,
        app_type: String,
    ) -> Result<serde_json::Value, String> {
        let request = ManagementAppPathRequest::from_path(app_type).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .current_route_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// GET /proxy/v1/groups
    pub(crate) async fn list_groups(
        &self,
        app_type: Option<String>,
    ) -> Result<serde_json::Value, String> {
        let query = match app_type {
            Some(app_type) => GroupListQuery::for_app(app_type),
            None => GroupListQuery::all(),
        };
        let request = GroupListRequest::from_query(query).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .group_list_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    // ---- Legacy migration ----------------------------------------------

    /// GET /proxy/v1/apps/{app}/channels/migration/preview
    pub(crate) async fn migration_preview(
        &self,
        app_type: String,
    ) -> Result<serde_json::Value, String> {
        let request = ManagementAppPathRequest::from_path(app_type).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .channel_migration_preview_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// POST /proxy/v1/apps/{app}/channels/migration/materialize
    pub(crate) async fn migration_materialize(
        &self,
        app_type: String,
    ) -> Result<serde_json::Value, String> {
        let request = ManagementAppPathRequest::from_path(app_type).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .channel_migration_materialize_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    // ---- Route dry-run & breakers --------------------------------------

    /// POST /proxy/v1/route/resolve
    pub(crate) async fn resolve_route(
        &self,
        request: RouteResolveRequest,
    ) -> Result<serde_json::Value, String> {
        let request = RouteResolveManagementRequest::from_body(request).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .resolve_route_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// GET /proxy/v1/channels/{channel_id}/breakers/stats
    pub(crate) async fn channel_breaker_stats(
        &self,
        channel_id: String,
    ) -> Result<serde_json::Value, String> {
        let request = ChannelPathRequest::from_path(channel_id).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .channel_breaker_stats_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }

    /// POST /proxy/v1/channels/{channel_id}/breakers/reset
    pub(crate) async fn reset_channel_breaker(
        &self,
        channel_id: String,
    ) -> Result<serde_json::Value, String> {
        let request = ChannelPathRequest::from_path(channel_id).map_err(err_to_string)?;
        let response = self
            .state
            .proxy_engine()
            .reset_channel_health_response(request)
            .await
            .map_err(err_to_string)?;
        serde_json::to_value(response).map_err(err_to_string)
    }
}
