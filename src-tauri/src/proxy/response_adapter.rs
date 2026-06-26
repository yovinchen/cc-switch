use super::{
    engine::context::RequestContext,
    engine::response_pipeline::{
        process_response, read_decoded_proxy_response_body, record_forward_core_error_usage,
    },
    error::ProxyError,
    error_mapper::{
        claude_response_transform_error_to_proxy_error,
        codex_chat_to_responses_transform_error_to_proxy_error,
        codex_proxy_error_body_build_error_to_proxy_error, codex_proxy_error_response,
        codex_responses_error_body_build_error_to_proxy_error, management_api_error_to_proxy_error,
        parse_claude_transform_upstream_json_or_unlabeled_sse,
        parse_codex_chat_upstream_json_or_unlabeled_sse, proxy_core_error_to_proxy_error,
        response_build_error_to_proxy_error,
    },
    transport::upstream::hyper_client::ProxyResponse,
};
use crate::app_config::AppType;
use crate::provider::Provider;
pub(crate) use crate::proxy_core::api::auth::ClaudeDesktopModelListResponse;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::events::ProxyEventEnvelope;
pub(crate) use crate::proxy_core::api::management::{
    AppChannelListQuery, AppChannelManagementRequest, AppChannelResponse, AppListRequest,
    AppListResponse, AppModelCatalogRequest, AppModelListQuery, ChannelBreakerStatsResponse,
    ChannelCreateRequest, ChannelDeleteResponse, ChannelHealthResetResponse,
    ChannelKeyDeleteResponse, ChannelKeyPathRequest, ChannelKeyRecord, ChannelKeyRecordResponse,
    ChannelKeysResponse, ChannelListQuery, ChannelListRequest, ChannelListResponse,
    ChannelMigrationMaterializeResponse, ChannelMigrationPreviewResponse, ChannelModelRecord,
    ChannelModelsResponse, ChannelPathRequest, ChannelRecord, ChannelRecordResponse,
    ChannelRouteCandidate, ChannelRouteRejected, ChannelTestResponse, CurrentRouteResponse,
    GroupListQuery, GroupListRequest, HealthCheckRequest, HealthCheckResponse,
    ManagementAppPathRequest, ProviderListResponse, ProxyChannelKeyPatchRequest,
    ProxyChannelKeyWriteRequest, ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest,
    ProxyChannelTestRequest, ProxyChannelWriteRequest, ProxyStatusRequest, ProxyStatusResponse,
    RouteGroupListResponse, RouteResolveManagementRequest, RouteResolveRequest,
    RouteResolveResponse,
};
pub(crate) use crate::proxy_core::api::model_catalog::{
    ClientModelCatalogResponse, RoutableModelList,
};
pub(crate) use crate::proxy_core::api::ports::{CurrentRouteTarget, ProxyRuntimeStatus};
use crate::proxy_core::api::routing::InterfaceKind;
use crate::proxy_core::api::transforms::{
    build_codex_tool_context_from_request, codex_chat_transform_streaming_decision,
    normalize_codex_chat_error_body, ClaudeTransformStreamingDecision,
    CodexChatTransformStreamingDecision, CodexToolContext,
};
use crate::proxy_core::api::transport::{
    append_query_to_endpoint_path, extract_gemini_model_from_path, parse_json_request_body,
    parse_json_request_body_or_null, rebuilt_json_proxy_response, request_body_read_error_message,
    request_body_stream_flag, strip_endpoint_prefix, transformed_sse_proxy_response, ProxyBody,
    ProxyCoreResponse, ProxyRequest,
    ProxyResponseBuildErrorContext as AxumResponseBuildErrorContext,
    ProxyResponseBuildFailureContext as CoreResponseBuildFailureContext, ProxyResult,
    ProxyTransportResponse, ProxyTransportResponseBody, UpstreamSseAggregationKind,
};
use crate::proxy_core::api::usage::{
    CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG, OPENAI_PARSER_CONFIG,
};
use crate::proxy_core_adapter::{
    claude_transformed_json_response_from_context, claude_transformed_sse_stream_from_context,
    codex_auto_transformed_json_response_from_context,
    codex_auto_transformed_sse_stream_from_context, provider_claude_transform_streaming_decision,
    provider_needs_claude_transform, provider_should_convert_codex_responses_to_chat,
    ActiveConnectionGuard, ClaudeTransformedJsonResponseContext, ClaudeTransformedSseStreamContext,
    CodexAutoTransformedJsonResponseContext, CodexAutoTransformedSseStreamContext, ProxyState,
};
use axum::{
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use bytes::Bytes;
use futures::Stream;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use serde_json::Value;
use std::convert::Infallible;
use std::time::Duration;

pub(crate) struct ParsedAxumJsonProxyRequest {
    pub(crate) method: Method,
    pub(crate) uri: Uri,
    pub(crate) headers: HeaderMap,
    pub(crate) extensions: http::Extensions,
    pub(crate) body: Value,
    pub(crate) is_stream: bool,
}

pub(crate) struct CodexResponsesProxyRequest {
    pub(crate) request: ProxyRequest,
    pub(crate) tool_context: CodexToolContext,
}

impl ParsedAxumJsonProxyRequest {
    pub(crate) async fn request_context(
        &self,
        state: &ProxyState,
        app_type: AppType,
        tag: &'static str,
        app_type_str: &'static str,
    ) -> Result<RequestContext, ProxyError> {
        RequestContext::new(
            state,
            &self.body,
            &self.headers,
            app_type,
            tag,
            app_type_str,
        )
        .await
    }

    pub(crate) async fn codex_request_context(
        &self,
        state: &ProxyState,
    ) -> Result<RequestContext, ProxyError> {
        self.request_context(state, AppType::Codex, "Codex", "codex")
            .await
    }

    pub(crate) async fn gemini_request_context(
        &self,
        state: &ProxyState,
        uri: &Uri,
    ) -> Result<RequestContext, ProxyError> {
        self.request_context(state, AppType::Gemini, "Gemini", "gemini")
            .await
            .map(|ctx| ctx.with_model_from_uri(uri))
    }

    pub(crate) fn endpoint_from_request_uri(&self) -> String {
        endpoint_from_uri(&self.uri)
    }

    pub(crate) fn endpoint_from_request_uri_stripping_prefix(
        &self,
        strip_prefix: Option<&str>,
    ) -> String {
        strip_endpoint_prefix(&self.endpoint_from_request_uri(), strip_prefix).to_string()
    }

    pub(crate) fn endpoint_for_path(&self, path: &str) -> String {
        append_query_to_endpoint_path(path, self.uri.query())
    }

    fn into_json_proxy_request(
        self,
        app_type: AppType,
        endpoint: String,
        inbound_interface: InterfaceKind,
        requested_model: Option<String>,
    ) -> ProxyRequest {
        ProxyRequest::new(
            AppKind::from(&app_type),
            self.method,
            endpoint,
            inbound_interface,
            ProxyBody::Json(self.body),
        )
        .with_observed_request_context(requested_model, self.headers, self.extensions)
    }

    pub(crate) fn into_anthropic_messages_proxy_request(
        self,
        app_type: AppType,
        endpoint: String,
        requested_model: Option<String>,
    ) -> ProxyRequest {
        self.into_json_proxy_request(
            app_type,
            endpoint,
            InterfaceKind::AnthropicMessages,
            requested_model,
        )
    }

    pub(crate) fn into_codex_chat_proxy_request(
        self,
        endpoint: String,
        requested_model: Option<String>,
    ) -> ProxyRequest {
        self.into_json_proxy_request(
            AppType::Codex,
            endpoint,
            InterfaceKind::OpenAiChatCompletions,
            requested_model,
        )
    }

    pub(crate) fn into_codex_responses_proxy_request(
        self,
        endpoint: String,
        requested_model: Option<String>,
    ) -> CodexResponsesProxyRequest {
        let tool_context = build_codex_tool_context_from_request(&self.body);
        let request = self.into_json_proxy_request(
            AppType::Codex,
            endpoint,
            InterfaceKind::OpenAiResponses,
            requested_model,
        );
        CodexResponsesProxyRequest {
            request,
            tool_context,
        }
    }

    pub(crate) fn into_gemini_proxy_request(self, endpoint: String) -> ProxyRequest {
        let requested_model = extract_gemini_model_from_path(&endpoint);
        self.into_json_proxy_request(
            AppType::Gemini,
            endpoint,
            InterfaceKind::GeminiNative,
            requested_model,
        )
    }
}

pub(crate) fn endpoint_from_uri(uri: &Uri) -> String {
    append_query_to_endpoint_path(uri.path(), uri.query())
}

async fn collect_axum_request_body(body: axum::body::Body) -> Result<Bytes, ProxyError> {
    body.collect()
        .await
        .map_err(|error| ProxyError::Internal(request_body_read_error_message(error)))
        .map(|collected| collected.to_bytes())
}

pub(crate) async fn collect_json_proxy_request(
    request: axum::extract::Request,
) -> Result<ParsedAxumJsonProxyRequest, ProxyError> {
    let (parts, body) = request.into_parts();
    let body_bytes = collect_axum_request_body(body).await?;
    let body = parse_json_request_body(body_bytes.as_ref())
        .map_err(|error| ProxyError::Internal(error.to_string()))?;
    let is_stream = request_body_stream_flag(&body);

    Ok(ParsedAxumJsonProxyRequest {
        method: parts.method,
        uri: parts.uri,
        headers: parts.headers,
        extensions: parts.extensions,
        body,
        is_stream,
    })
}

pub(crate) async fn collect_json_or_null_proxy_request(
    request: axum::extract::Request,
) -> Result<ParsedAxumJsonProxyRequest, ProxyError> {
    let (parts, body) = request.into_parts();
    let body_bytes = collect_axum_request_body(body).await?;
    let body = parse_json_request_body_or_null(body_bytes.as_ref())
        .map_err(|error| ProxyError::Internal(error.to_string()))?;
    let is_stream = request_body_stream_flag(&body);

    Ok(ParsedAxumJsonProxyRequest {
        method: parts.method,
        uri: parts.uri,
        headers: parts.headers,
        extensions: parts.extensions,
        body,
        is_stream,
    })
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

pub(crate) fn proxy_health_check_to_axum_json_response() -> (StatusCode, Json<HealthCheckResponse>)
{
    let request = HealthCheckRequest::new();
    (
        StatusCode::OK,
        Json(request.response(chrono::Utc::now().to_rfc3339())),
    )
}

pub(crate) async fn dispatch_proxy_status_request_to_axum_json_response(
    state: &ProxyState,
) -> Result<Json<ProxyStatusResponse<ProxyRuntimeStatus>>, ProxyError> {
    let request = ProxyStatusRequest::new();
    let response = state
        .proxy_engine()
        .proxy_status_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

pub(crate) async fn dispatch_claude_desktop_models_request_to_axum_json_response(
    state: &ProxyState,
) -> Result<Json<ClaudeDesktopModelListResponse>, ProxyError> {
    let response = state
        .proxy_engine()
        .claude_desktop_model_list_response()
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

pub(crate) async fn dispatch_proxy_apps_request_to_axum_json_response(
    state: &ProxyState,
) -> Result<Json<AppListResponse>, ProxyError> {
    let request = AppListRequest::new();
    let response = state
        .proxy_engine()
        .app_list_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

pub(crate) async fn dispatch_proxy_providers_request_to_axum_json_response(
    state: &ProxyState,
    app_type: String,
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

pub(crate) async fn dispatch_proxy_app_models_request_to_axum_json_response(
    state: &ProxyState,
    app_type: String,
    query: AppModelListQuery,
) -> Result<Json<RoutableModelList>, ProxyError> {
    let request = AppModelCatalogRequest::from_parts(app_type, query)
        .map_err(management_api_error_to_proxy_error)?;
    let catalog = state
        .proxy_engine()
        .list_model_catalog_for_request(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(catalog))
}

pub(crate) async fn dispatch_codex_client_model_catalog_request_to_axum_json_response(
    state: &ProxyState,
) -> Result<Json<ClientModelCatalogResponse>, ProxyError> {
    let response = state
        .proxy_engine()
        .client_model_catalog_response(&AppKind::Codex)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

pub(crate) async fn dispatch_proxy_channels_request_to_axum_json_response(
    state: &ProxyState,
    query: ChannelListQuery,
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

pub(crate) async fn dispatch_proxy_app_channels_request_to_axum_json_response(
    state: &ProxyState,
    app_type: String,
    query: AppChannelListQuery,
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

pub(crate) async fn dispatch_proxy_groups_request_to_axum_json_response(
    state: &ProxyState,
    query: GroupListQuery,
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

pub(crate) async fn dispatch_current_proxy_route_request_to_axum_json_response(
    state: &ProxyState,
    app_type: String,
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

pub(crate) async fn dispatch_proxy_route_resolve_request_to_axum_json_response(
    state: &ProxyState,
    request: RouteResolveRequest,
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

pub(crate) async fn dispatch_create_proxy_channel_request_to_axum_json_response(
    state: &ProxyState,
    request: ProxyChannelWriteRequest,
) -> Result<Json<ChannelRecordResponse<ChannelRecord>>, ProxyError> {
    let request = ChannelCreateRequest::from_body(request);
    let response = state
        .proxy_engine()
        .create_channel_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

pub(crate) async fn dispatch_get_proxy_channel_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
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

pub(crate) async fn dispatch_update_proxy_channel_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
    request: ProxyChannelPatchRequest,
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

pub(crate) async fn dispatch_delete_proxy_channel_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
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

pub(crate) async fn dispatch_proxy_channel_keys_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
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

pub(crate) async fn dispatch_upsert_proxy_channel_key_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
    key_ref: String,
    request: ProxyChannelKeyWriteRequest,
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

pub(crate) async fn dispatch_update_proxy_channel_key_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
    key_ref: String,
    request: ProxyChannelKeyPatchRequest,
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

pub(crate) async fn dispatch_delete_proxy_channel_key_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
    key_ref: String,
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

pub(crate) async fn dispatch_proxy_channel_models_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
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

pub(crate) async fn dispatch_replace_proxy_channel_models_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
    request: ProxyChannelModelsReplaceRequest,
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

pub(crate) async fn dispatch_proxy_channel_test_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
    request: ProxyChannelTestRequest,
) -> Result<Json<ChannelTestResponse>, ProxyError> {
    let path_request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .channel_test_response(path_request, request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

pub(crate) async fn dispatch_preview_proxy_channel_migration_request_to_axum_json_response(
    state: &ProxyState,
    app_type: String,
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

pub(crate) async fn dispatch_materialize_proxy_channel_migration_request_to_axum_json_response(
    state: &ProxyState,
    app_type: String,
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

pub(crate) async fn dispatch_proxy_channel_breaker_stats_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
) -> Result<Json<ChannelBreakerStatsResponse>, ProxyError> {
    let request =
        ChannelPathRequest::from_path(channel_id).map_err(management_api_error_to_proxy_error)?;
    let response = state
        .proxy_engine()
        .channel_breaker_stats_response(request)
        .await
        .map_err(proxy_core_error_to_proxy_error)?;

    Ok(Json(response))
}

pub(crate) async fn dispatch_reset_proxy_channel_breaker_request_to_axum_json_response(
    state: &ProxyState,
    channel_id: String,
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

fn proxy_result_to_proxy_response(
    result: ProxyResult,
    ctx: &mut RequestContext,
    state: &ProxyState,
) -> Result<ProxyResponse, ProxyError> {
    ctx.apply_proxy_result(state, &result)?;
    proxy_core_response_to_proxy_response(result.response)
}

fn claude_proxy_result_to_proxy_response(
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
    let normalized = normalize_codex_chat_error_body(body_bytes);
    if let Some(message) = normalized.non_json_body_log_message() {
        log::warn!("{message}");
    }
    let response = rebuilt_json_proxy_response(status, response_headers, normalized.response_error)
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

pub(crate) fn proxy_events_request_to_axum_sse_response(
    state: &ProxyState,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::api::events::ProxyEventEnvelope;
    use crate::proxy_core::api::transport::ProxyResponseBody;
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
