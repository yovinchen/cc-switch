use crate::app_config::AppType;
use crate::claude_desktop_config::ResolvedModelRoute;
use crate::database::{
    Database, FailoverQueueItem, ProxyChannelKeyRecord, ProxyChannelModelRecord, ProxyChannelRecord,
    ProxyChannelMigrationPreview, ProxyChannelMaterializeResult, ProxyChannelSourceKind,
};
use crate::error::AppError;
use crate::openclaw_config::OpenClawProviderConfig;
use crate::provider::{
    OpenCodeProviderConfig, Provider, ProviderMeta, ProviderTestConfig, UsageScript,
};
use crate::proxy::error_mapper::forward_error_to_core_error;
use crate::proxy::events::ProxyEventBus;
use crate::proxy::failover_switch::FailoverSwitchManager;
use crate::proxy::hyper_client::ProxyResponse;
use crate::proxy::provider_router::ProviderRouter;
use crate::proxy::codex_chat_history::CodexChatHistoryStore;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy::usage::{RequestLog, UsageLogger};
use crate::proxy::RequestForwarder;
use crate::services::stream_check::StreamCheckService;
use crate::proxy_core::api::domain::{
    ChannelSpecInput, ModelRoute, ModelRouteInput, ProviderMetadata, ProviderMetadataInput,
};
#[cfg(test)]
use crate::proxy_core::api::domain::{ChannelHealthPolicy, ChannelOverrides, UpstreamEndpoint};
pub(crate) use crate::proxy_core::api::management::ChannelReachabilityResult;
use crate::proxy_core::api::routing::{
    route_resolve_channel_input_from_record, RouteResolveChannelInput,
    RouteResolveChannelRecordInput, RouteResolveModelRecordInput,
};
#[cfg(test)]
use crate::proxy_core::api::routing::RouteResolveModelInput;
use crate::proxy_core::api::session::SessionIdResult;
use bytes::Bytes;
use futures::{future::BoxFuture, Stream, StreamExt};
use http::{HeaderMap, Method};
use indexmap::IndexMap;
use regex::Regex;
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub(crate) const COPILOT_EDITOR_VERSION: &str = "vscode/1.110.1";
pub(crate) const COPILOT_PLUGIN_VERSION: &str = "copilot-chat/0.38.2";
pub(crate) const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.38.2";
pub(crate) const COPILOT_API_VERSION: &str = "2025-10-01";
pub(crate) const COPILOT_INTEGRATION_ID: &str = "vscode-chat";

pub(crate) fn synthesize_gemini_tool_call_id_with_uuid() -> String {
    crate::proxy_core::api::transforms::synthesize_gemini_tool_call_id(
        Uuid::new_v4().simple().to_string(),
    )
}

pub(crate) type ClaudeDesktopGatewayAuthError =
    crate::proxy_core::api::auth::ClaudeDesktopGatewayAuthError;

pub(crate) type ProxyErrorStatusKind =
    crate::proxy_core::api::errors::ProxyErrorStatusKind;

pub(crate) use crate::proxy_core::api::errors::{
    proxy_core_error_from_status_kind, proxy_error_http_status_code, proxy_error_response_body,
    upstream_proxy_error_response_body,
};

pub(crate) fn error_message_with_context(
    context: &str,
    error: impl std::fmt::Display,
) -> String {
    crate::proxy_core::api::errors::error_message_with_context(context, &error.to_string())
}

pub(crate) use crate::proxy_core::api::errors::{
    selected_provider_display_name_for_error, selected_provider_missing_from_source_message,
    selected_provider_not_applied_message, unselected_provider_fallback_id,
};

pub(crate) fn app_error(context: &str, error: AppError) -> ProxyCoreError {
    ProxyCoreError::Config(error_message_with_context(context, error))
}

pub(crate) fn app_write_error(context: &str, error: AppError) -> ProxyCoreError {
    match error {
        AppError::InvalidInput(message) => {
            ProxyCoreError::InvalidRequest(AppError::InvalidInput(message).to_string())
        }
        other => app_error(context, other),
    }
}

pub(crate) fn usage_error(context: &str, error: AppError) -> ProxyCoreError {
    ProxyCoreError::Internal(error_message_with_context(context, error))
}

pub(crate) fn app_error_from_proxy_core_error(error: ProxyCoreError) -> AppError {
    match error {
        ProxyCoreError::Config(message) => AppError::Config(message),
        ProxyCoreError::InvalidRequest(message) => AppError::InvalidInput(message),
        other => AppError::Message(other.to_string()),
    }
}

pub(crate) fn app_error_from_provider_selection_failure(
    app_type: &str,
    error: ProviderSelectionFailure,
) -> AppError {
    match error {
        ProviderSelectionFailure::AllProvidersCircuitOpen => {
            log::warn!("[{app_type}] [FO-004] 所有供应商均已熔断");
            AppError::AllProvidersCircuitOpen
        }
        ProviderSelectionFailure::NoProvidersConfigured => {
            log::warn!("[{app_type}] [FO-005] 未配置供应商");
            AppError::NoProvidersConfigured
        }
    }
}

pub(crate) fn provider_selection_failure_from_app_error(
    error: &AppError,
) -> Option<ProviderSelectionFailure> {
    match error {
        AppError::AllProvidersCircuitOpen => Some(ProviderSelectionFailure::AllProvidersCircuitOpen),
        AppError::NoProvidersConfigured => Some(ProviderSelectionFailure::NoProvidersConfigured),
        _ => None,
    }
}

pub(crate) const SYSTEM_PROXY_ENV_KEYS: [&str; 6] =
    crate::proxy_core::api::transport::SYSTEM_PROXY_ENV_KEYS;

pub(crate) use crate::proxy_core::api::security::mask_url_for_log;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::proxy_url_points_to_loopback_port;

pub(crate) use crate::proxy_core::api::transport::proxy_values_point_to_loopback_port;

pub(crate) const COPILOT_PUBLIC_GITHUB_DOMAIN: &str =
    crate::proxy_core::api::model_catalog::COPILOT_PUBLIC_GITHUB_DOMAIN;

pub(crate) use crate::proxy_core::api::model_catalog::{
    copilot_composite_account_id, default_copilot_github_domain, is_copilot_ghes_domain,
    normalize_github_domain, parse_copilot_models_response_bytes,
};

pub(crate) type CopilotModel = crate::proxy_core::api::model_catalog::CopilotModel;

pub(crate) use crate::proxy_core::api::model_catalog::{
    copilot_api_base, copilot_github_client_id, copilot_github_device_code_url,
    copilot_github_oauth_token_url, copilot_github_user_url, copilot_token_url,
    copilot_usage_url,
};

pub(crate) type RectifierConfig = crate::proxy_core::api::ports::RectifierConfig;
pub(crate) type OptimizerConfig = crate::proxy_core::api::ports::OptimizerConfig;
pub(crate) type CopilotOptimizerConfig =
    crate::proxy_core::api::ports::CopilotOptimizerConfig;
pub(crate) type ProxyConfig = crate::proxy_core::api::ports::ProxyConfig;
pub(crate) type ProxyRuntimeStatus =
    crate::proxy_core::api::ports::ProxyRuntimeStatus;

pub(crate) use crate::proxy_core::api::ports::proxy_runtime_status_stopped;

const PROXY_MANAGEMENT_AUTH_TOKEN_ENV: &str = "CC_SWITCH_PROXY_MANAGEMENT_TOKEN";

pub(crate) fn management_auth_decision_from_proxy_config(
    config: &ProxyConfig,
) -> Result<ManagementAuthDecision, ManagementAuthError> {
    let fallback_token = std::env::var(PROXY_MANAGEMENT_AUTH_TOKEN_ENV).ok();
    management_auth_decision_from_proxy_config_sources(config, fallback_token.as_deref())
}

pub(crate) fn management_auth_decision_from_proxy_config_sources(
    config: &ProxyConfig,
    fallback_token: Option<&str>,
) -> Result<ManagementAuthDecision, ManagementAuthError> {
    resolve_management_auth_decision(
        &config.listen_address,
        config.management_auth_token.as_deref(),
        fallback_token,
    )
}

pub(crate) fn record_forward_success_status(
    status: &mut ProxyRuntimeStatus,
    current_provider_id_at_start: &str,
    provider_id: &str,
) -> bool {
    crate::proxy_core::api::ports::record_forward_success_status(
        status,
        crate::proxy_core::api::ports::ForwardSuccessStatusInput {
            current_provider_id_at_start,
            provider_id,
        },
    )
    .should_switch_current_provider
}

pub(crate) fn record_forward_failure_status(
    status: &mut ProxyRuntimeStatus,
    error_message: &str,
) {
    crate::proxy_core::api::ports::record_forward_failure_status(
        status,
        crate::proxy_core::api::ports::ForwardFailureStatusInput { error_message },
    );
}

pub(crate) fn record_forward_request_started_status(
    status: &mut ProxyRuntimeStatus,
    timestamp: &str,
) {
    crate::proxy_core::api::ports::record_forward_request_started_status(
        status,
        crate::proxy_core::api::ports::ForwardRequestStartedStatusInput { timestamp },
    );
}

pub(crate) use crate::proxy_core::api::ports::{
    record_active_connection_acquired_status, record_active_connection_released_status,
};

pub(crate) async fn record_forward_active_connection_acquired_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
) {
    let mut status = status.write().await;
    record_active_connection_acquired_status(&mut status);
}

pub(crate) async fn record_forward_active_connection_released_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
) {
    let mut status = status.write().await;
    record_active_connection_released_status(&mut status);
}

pub(crate) async fn record_forward_request_started_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    timestamp: &str,
) {
    let mut status = status.write().await;
    record_forward_request_started_status(&mut status, timestamp);
}

pub(crate) async fn record_forward_current_provider_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    provider_id: &str,
    provider_name: &str,
) {
    let mut status = status.write().await;
    crate::proxy_core::api::ports::record_forward_current_provider_status(
        &mut status,
        crate::proxy_core::api::ports::ForwardCurrentProviderStatusInput {
            provider_id,
            provider_name,
        },
    );
}

pub(crate) async fn record_forward_success_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    current_provider_id_at_start: &str,
    provider_id: &str,
) -> bool {
    let mut status = status.write().await;
    record_forward_success_status(&mut status, current_provider_id_at_start, provider_id)
}

pub(crate) async fn record_forward_failure_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    error_message: &str,
) {
    let mut status = status.write().await;
    record_forward_failure_status(&mut status, error_message);
}

pub(crate) async fn record_forward_provider_failure_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    provider_name: &str,
    error_message: &str,
) {
    let mut status = status.write().await;
    crate::proxy_core::api::ports::record_forward_provider_failure_status(
        &mut status,
        crate::proxy_core::api::ports::ForwardProviderFailureStatusInput {
            provider_name,
            error_message,
        },
    );
}

pub(crate) async fn record_forward_provider_rectifier_retry_failure_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    provider_name: &str,
    rectifier_label: &str,
    error_message: &str,
) {
    let mut status = status.write().await;
    crate::proxy_core::api::ports::record_forward_provider_rectifier_retry_failure_status(
        &mut status,
        crate::proxy_core::api::ports::ForwardProviderRectifierRetryFailureStatusInput {
            provider_name,
            rectifier_label,
            error_message,
        },
    );
}

pub(crate) fn record_proxy_server_started_status(
    status: &mut ProxyRuntimeStatus,
    address: &str,
    port: u16,
) {
    crate::proxy_core::api::ports::record_proxy_server_started_status(
        status,
        crate::proxy_core::api::ports::ProxyServerStartedStatusInput { address, port },
    );
}

pub(crate) use crate::proxy_core::api::ports::{
    apply_proxy_runtime_uptime, record_proxy_server_stopped_status,
};

pub(crate) type ProxyRuntimeConfig =
    crate::proxy_core::api::config::ProxyRuntimeConfig;
pub(crate) type ProxyGlobalConfig = crate::proxy_core::api::config::ProxyGlobalConfig;
pub(crate) type ProxyAppConfig = crate::proxy_core::api::config::ProxyAppConfig;
pub(crate) type ProxyServerInfo = crate::proxy_core::api::ports::ProxyServerInfo;

pub(crate) use crate::proxy_core::api::ports::proxy_server_info_from_parts;

pub(crate) fn proxy_live_urls_from_listen_parts(
    listen_address: &str,
    listen_port: u16,
) -> Option<(String, String)> {
    if listen_port == 0 {
        return None;
    }

    // listen_address 可能是 0.0.0.0（用于监听所有网卡），但客户端无法用
    // 0.0.0.0 连接；因此写回到各应用配置时，优先使用本机回环地址。
    let connect_host = match listen_address {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "::1".to_string(),
        _ => listen_address.to_string(),
    };
    let connect_host_for_url = if connect_host.contains(':') && !connect_host.starts_with('[') {
        format!("[{connect_host}]")
    } else {
        connect_host
    };

    let proxy_origin = format!("http://{connect_host_for_url}:{listen_port}");
    let proxy_codex_base_url = format!("{}/v1", proxy_origin.trim_end_matches('/'));
    Some((proxy_origin, proxy_codex_base_url))
}

pub(crate) fn record_proxy_server_listen_port_runtime_source(port: u16) {
    crate::proxy::http_client::set_proxy_port(port);
}

pub(crate) type ProxyTakeoverStatus =
    crate::proxy_core::api::ports::ProxyTakeoverStatus;

pub(crate) use crate::proxy_core::api::ports::proxy_takeover_status_from_parts;

pub(crate) type ClaudeDesktopModelListResponse =
    crate::proxy_core::api::auth::ClaudeDesktopModelListResponse;
pub(crate) type ClaudeDesktopModelRouteInput =
    crate::proxy_core::api::auth::ClaudeDesktopModelRouteInput;
pub(crate) type ProxyCoreResponse =
    crate::proxy_core::api::transport::ProxyCoreResponse;
pub(crate) type RequestBodyJsonParseError =
    crate::proxy_core::api::transport::RequestBodyJsonParseError;
pub(crate) type ProxyCoreResult<T> = crate::proxy_core::api::errors::ProxyCoreResult<T>;
pub(crate) type ProxyEngine<S> = crate::proxy_core::api::engine::ProxyEngine<S>;
pub(crate) type ProxyResult = crate::proxy_core::api::transport::ProxyResult;
pub(crate) type ProxyCoreEvent = crate::proxy_core::api::events::ProxyCoreEvent;
pub(crate) type ProxyEventEnvelope =
    crate::proxy_core::api::events::ProxyEventEnvelope;
pub(crate) type CodexChatHistorySseRecord =
    crate::proxy_core::api::transforms::CodexChatHistorySseRecord;
pub(crate) type CodexChatHistoryState =
    crate::proxy_core::api::transforms::CodexChatHistoryState;
pub(crate) type CodexChatReasoningOptions =
    crate::proxy_core::api::transforms::CodexChatReasoningOptions;
pub(crate) type CodexChatReasoningProfile =
    crate::proxy_core::api::transforms::CodexChatReasoningProfile;
pub(crate) type CodexToolContext =
    crate::proxy_core::api::transforms::CodexToolContext;
pub(crate) type ProxyResponseBody =
    crate::proxy_core::api::transport::ProxyResponseBody;
pub(crate) type ProxyTransportResponse =
    crate::proxy_core::api::transport::ProxyTransportResponse;
pub(crate) type ProxyTransportResponseBody =
    crate::proxy_core::api::transport::ProxyTransportResponseBody;
pub(crate) type ResponseBodyDecodeLogLevel =
    crate::proxy_core::api::transport::ResponseBodyDecodeLogLevel;
pub(crate) type CostBreakdown = crate::proxy_core::api::usage::CostBreakdown;
pub(crate) type CostCalculator = crate::proxy_core::api::usage::CostCalculator;
pub(crate) type ModelPricing = crate::proxy_core::api::usage::ModelPricing;
pub(crate) type TokenUsage = crate::proxy_core::api::usage::TokenUsage;
pub(crate) type UsageRecord = crate::proxy_core::api::usage::UsageRecord;
pub(crate) type UsageRouteContext = crate::proxy_core::api::usage::UsageRouteContext;
#[cfg(test)]
pub(crate) type UsageTokens = crate::proxy_core::api::usage::UsageTokens;
pub(crate) type NonStreamingResponseUsageRecord =
    crate::proxy_core::api::usage::NonStreamingResponseUsageRecord;
pub(crate) type StreamingResponseUsageRecord =
    crate::proxy_core::api::usage::StreamingResponseUsageRecord;
pub(crate) type UsageParserConfig =
    crate::proxy_core::api::usage::UsageParserConfig;
pub(crate) type StreamUsageEventFilter =
    crate::proxy_core::api::usage::StreamUsageEventFilter;
pub(crate) type TransformedResponseUsageFormat =
    crate::proxy_core::api::usage::TransformedResponseUsageFormat;
pub(crate) type UsageSelectedProviderMissingPhase =
    crate::proxy_core::api::usage::UsageSelectedProviderMissingPhase;
pub(crate) type UsageRecordFailureLogContext =
    crate::proxy_core::api::usage::UsageRecordFailureLogContext;
pub(crate) type CurrentRouteTarget =
    crate::proxy_core::api::ports::CurrentRouteTarget;

pub(crate) fn current_route_target_from_forward_attempt(
    app_type: &str,
    attempt: &ForwardAttempt,
) -> CurrentRouteTarget {
    let provider = attempt.provider();
    let channel = attempt.channel();
    crate::proxy_core::api::ports::current_route_target_from_input(
        crate::proxy_core::api::ports::CurrentRouteTargetInput {
            app_type,
            provider_id: provider.id.as_str(),
            provider_name: provider.name.as_str(),
            channel: channel.map(|channel| {
                crate::proxy_core::api::ports::CurrentRouteChannelTargetInput {
                    channel_id: channel.channel_id.as_str(),
                    channel_name: channel.channel_name.as_str(),
                    interface_kind: channel.interface_kind.as_str(),
                    public_model: channel.public_model.as_deref(),
                    upstream_model: channel.upstream_model.as_deref(),
                }
            }),
        },
    )
}

pub(crate) fn current_route_target_from_provider(
    app_type: &str,
    provider_id: &str,
    provider_name: &str,
) -> CurrentRouteTarget {
    crate::proxy_core::api::ports::current_route_target_from_input(
        crate::proxy_core::api::ports::CurrentRouteTargetInput {
            app_type,
            provider_id,
            provider_name,
            channel: None,
        },
    )
}

pub(crate) fn apply_proxy_runtime_active_targets(
    status: &mut ProxyRuntimeStatus,
    active_targets: impl IntoIterator<Item = CurrentRouteTarget>,
) {
    crate::proxy_core::api::ports::apply_proxy_runtime_active_targets(status, active_targets);
}

pub(crate) async fn record_proxy_server_started_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    start_time: &RwLock<Option<std::time::Instant>>,
    address: &str,
    port: u16,
) {
    {
        let mut status = status.write().await;
        record_proxy_server_started_status(&mut status, address, port);
    }
    *start_time.write().await = Some(std::time::Instant::now());
}

pub(crate) async fn record_proxy_server_stopped_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    start_time: &RwLock<Option<std::time::Instant>>,
) {
    {
        let mut status = status.write().await;
        record_proxy_server_stopped_status(&mut status);
    }
    *start_time.write().await = None;
}

pub(crate) async fn proxy_runtime_status_from_runtime_sources(
    status: &RwLock<ProxyRuntimeStatus>,
    start_time: &RwLock<Option<std::time::Instant>>,
    current_providers: &RwLock<HashMap<String, CurrentRouteTarget>>,
) -> ProxyRuntimeStatus {
    let mut status = status.read().await.clone();

    if let Some(start) = start_time.read().await.as_ref().copied() {
        apply_proxy_runtime_uptime(&mut status, start.elapsed().as_secs());
    }

    let current_providers = current_providers.read().await;
    apply_proxy_runtime_active_targets(&mut status, current_providers.values().cloned());

    status
}

pub(crate) async fn set_active_route_target_runtime_source(
    current_providers: &RwLock<HashMap<String, CurrentRouteTarget>>,
    app_type: &str,
    provider_id: &str,
    provider_name: &str,
) {
    let mut current_providers = current_providers.write().await;
    current_providers.insert(
        app_type.to_string(),
        current_route_target_from_provider(app_type, provider_id, provider_name),
    );
}

pub(crate) fn emit_request_started_event_source(
    events: &ProxyEventBus,
    request_id: &str,
    app_type: &str,
) {
    let message = request_started_event_message(request_id, app_type);
    events.emit(message.event_name, message.payload);
}

pub(crate) fn emit_attempt_event_source(
    events: &ProxyEventBus,
    request_id: &str,
    app_type: &str,
    attempt: &ForwardAttempt,
    phase: AttemptEventPhase,
    error: Option<&str>,
) {
    let message =
        attempt_event_message_from_forward_attempt(request_id, app_type, attempt, phase, error);
    events.emit(message.event_name, message.payload);
}

pub(crate) async fn record_forward_active_route_target_runtime_source(
    current_providers: &RwLock<HashMap<String, CurrentRouteTarget>>,
    events: &ProxyEventBus,
    request_id: &str,
    app_type: &str,
    attempt: &ForwardAttempt,
) {
    {
        let mut current_providers = current_providers.write().await;
        current_providers.insert(
            app_type.to_string(),
            current_route_target_from_forward_attempt(app_type, attempt),
        );
    }

    let message = route_selected_event_message_from_forward_attempt(request_id, app_type, attempt);
    events.emit(message.event_name, message.payload);
}

pub(crate) async fn allow_forward_attempt_runtime_source(
    router: &ProviderRouter,
    attempt: &ForwardAttempt,
    app_type: &str,
    bypass_circuit_breaker: bool,
) -> AllowResult {
    if bypass_circuit_breaker {
        return AllowResult {
            allowed: true,
            used_half_open_permit: false,
        };
    }

    if let Some(channel) = attempt.channel() {
        router
            .allow_channel_request(&channel.channel_id, app_type)
            .await
    } else {
        router
            .allow_provider_request(&attempt.provider().id, app_type)
            .await
    }
}

pub(crate) async fn record_forward_attempt_success_runtime_source(
    router: &Arc<ProviderRouter>,
    attempt: &ForwardAttempt,
    app_type: &str,
    used_half_open_permit: bool,
) {
    if let Some(channel) = attempt.channel() {
        if used_half_open_permit {
            if let Err(error) = router
                .record_channel_result(&channel.channel_id, app_type, true, true, None, None)
                .await
            {
                log::warn!(
                    "[{app_type}] 记录 Channel 成功结果失败: channel_id={}, error={error}",
                    channel.channel_id
                );
            }
            return;
        }

        let router = router.clone();
        let channel_id = channel.channel_id.clone();
        let app_type = app_type.to_string();
        tokio::spawn(async move {
            if let Err(error) = router
                .record_channel_result(&channel_id, &app_type, false, true, None, None)
                .await
            {
                log::warn!(
                    "[{app_type}] 异步记录 Channel 成功结果失败: channel_id={channel_id}, error={error}"
                );
            }
        });
        return;
    }

    let provider_id = attempt.provider().id.clone();
    if used_half_open_permit {
        if let Err(error) = router
            .record_result(&provider_id, app_type, true, true, None)
            .await
        {
            log::warn!(
                "[{app_type}] 记录 Provider 成功结果失败: provider_id={provider_id}, error={error}"
            );
        }
        return;
    }

    let router = router.clone();
    let app_type = app_type.to_string();
    tokio::spawn(async move {
        if let Err(error) = router
            .record_result(&provider_id, &app_type, false, true, None)
            .await
        {
            log::warn!(
                "[{app_type}] 异步记录 Provider 成功结果失败: provider_id={provider_id}, error={error}"
            );
        }
    });
}

pub(crate) async fn record_forward_attempt_failure_runtime_source(
    router: &ProviderRouter,
    attempt: &ForwardAttempt,
    app_type: &str,
    used_half_open_permit: bool,
    error_message: &str,
) {
    if let Some(channel) = attempt.channel() {
        let _ = router
            .record_channel_result(
                &channel.channel_id,
                app_type,
                used_half_open_permit,
                false,
                Some(error_message.to_string()),
                None,
            )
            .await;
        return;
    }

    let _ = router
        .record_result(
            &attempt.provider().id,
            app_type,
            used_half_open_permit,
            false,
            Some(error_message.to_string()),
        )
        .await;
}

pub(crate) async fn release_forward_attempt_permit_neutral_runtime_source(
    router: &ProviderRouter,
    attempt: &ForwardAttempt,
    app_type: &str,
    used_half_open_permit: bool,
) {
    if let Some(channel) = attempt.channel() {
        router
            .release_channel_permit_neutral(
                &channel.channel_id,
                app_type,
                used_half_open_permit,
            )
            .await;
        return;
    }

    router
        .release_permit_neutral(&attempt.provider().id, app_type, used_half_open_permit)
        .await;
}

pub(crate) type GeminiShadowStore =
    crate::proxy_core::api::transforms::GeminiShadowStore;
pub(crate) type AuthProfileRef = crate::proxy_core::api::domain::AuthProfileRef;
pub(crate) type ChannelAuthProfileResolution =
    crate::proxy_core::api::domain::ChannelAuthProfileResolution;
pub(crate) type ClaudeAuthHeaderKind =
    crate::proxy_core::api::transport::ClaudeAuthHeaderKind;
pub(crate) type ClaudeAuthKey = crate::proxy_core::api::auth::ClaudeAuthKey;
pub(crate) type ClaudeAuthKeySource =
    crate::proxy_core::api::auth::ClaudeAuthKeySource;
pub(crate) type ClaudePromptCacheKeyResolution =
    crate::proxy_core::api::transforms::ClaudePromptCacheKeyResolution;
pub(crate) type CopilotAuthHeadersInput<'a> =
    crate::proxy_core::api::transport::CopilotAuthHeadersInput<'a>;
pub(crate) type CopilotAuthHeaderOverrides<'a> =
    crate::proxy_core::api::transport::CopilotAuthHeaderOverrides<'a>;
pub(crate) type ResponseRuntimePolicy =
    crate::proxy_core::api::config::ResponseRuntimePolicy;
pub(crate) type ResponseTimeoutConfig =
    crate::proxy_core::api::config::ResponseTimeoutConfig;
pub(crate) type StreamingTimeoutConfig =
    crate::proxy_core::api::config::StreamingTimeoutConfig;
pub(crate) type StreamingTimeoutPhase =
    crate::proxy_core::api::transport::StreamingTimeoutPhase;
pub(crate) type SseEventScanner =
    crate::proxy_core::api::transforms::SseEventScanner;
pub(crate) type SsePassthroughEventKind =
    crate::proxy_core::api::transforms::SsePassthroughEventKind;
pub(crate) type SseUsageAccumulator =
    crate::proxy_core::api::transforms::SseUsageAccumulator;
pub(crate) type GlobalProxyConfig = crate::proxy_core::api::ports::GlobalProxyConfig;
pub(crate) type AppProxyConfig = crate::proxy_core::api::config::AppProxyConfig;

pub(crate) fn proxy_global_config_from_config(config: GlobalProxyConfig) -> ProxyGlobalConfig {
    crate::proxy_core::api::config::proxy_global_config_from_global_config(config)
}

pub(crate) fn proxy_app_config_from_config_parts(
    app: AppKind,
    config: AppProxyConfig,
    current_provider_id: Option<String>,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ProxyAppConfig {
    crate::proxy_core::api::config::proxy_app_config_from_parts(
        app,
        config,
        current_provider_id,
        rectifier,
        optimizer,
        copilot_optimizer,
    )
}

pub(crate) fn current_provider_id_from_settings_for_app(app: &AppKind) -> Option<String> {
    app_type_option_from_proxy_core_app(app)
        .as_ref()
        .and_then(crate::settings::get_current_provider)
}

pub(crate) fn current_provider_id_from_settings_for_app_type(
    app_type: &AppType,
) -> Option<String> {
    crate::settings::get_current_provider(app_type)
}

pub(crate) fn proxy_app_config_from_config_source(
    app: AppKind,
    config: AppProxyConfig,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ProxyAppConfig {
    let settings_current_provider_id = current_provider_id_from_settings_for_app(&app);
    proxy_app_config_from_config_source_parts(
        app,
        config,
        settings_current_provider_id.as_deref(),
        rectifier,
        optimizer,
        copilot_optimizer,
    )
}

pub(crate) async fn proxy_global_config_from_db_source(
    db: &Database,
) -> ProxyCoreResult<ProxyGlobalConfig> {
    let config = db
        .get_global_proxy_config()
        .await
        .map_err(|error| app_error("load global proxy config", error))?;
    Ok(proxy_global_config_from_config(config))
}

pub(crate) async fn proxy_app_config_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<ProxyAppConfig> {
    let config = db
        .get_proxy_config_for_app(app.as_str())
        .await
        .map_err(|error| app_error("load app proxy config", error))?;
    Ok(proxy_app_config_from_config_source(
        app.clone(),
        config,
        db.get_rectifier_config().unwrap_or_default(),
        db.get_optimizer_config().unwrap_or_default(),
        db.get_copilot_optimizer_config().unwrap_or_default(),
    ))
}

pub(crate) fn app_summary_config_from_config_source(config: AppProxyConfig) -> AppSummaryConfig {
    AppSummaryConfig::new(config.enabled, config.auto_failover_enabled)
}

pub(crate) async fn app_summary_config_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<AppSummaryConfig> {
    let config = db
        .get_proxy_config_for_app(app.as_str())
        .await
        .map_err(|error| app_error("load app summary config", error))?;
    Ok(app_summary_config_from_config_source(config))
}

pub(crate) fn forward_current_provider_id_from_source(
    settings_current_provider_id: Option<&str>,
    load_db_current_provider_id: impl FnOnce() -> Option<String>,
) -> String {
    let db_current_provider_id =
        if current_provider_db_fallback_required(settings_current_provider_id) {
            load_db_current_provider_id()
        } else {
            None
        };
    current_provider_id_from_sources(
        settings_current_provider_id,
        db_current_provider_id.as_deref(),
    )
}

pub(crate) fn forward_current_provider_id_from_db_sources(
    db: &Database,
    app_type: &AppType,
) -> String {
    let settings_current_provider_id = current_provider_id_from_settings_for_app_type(app_type);
    forward_current_provider_id_from_source(settings_current_provider_id.as_deref(), || {
        db.get_current_provider(app_type.as_str()).ok().flatten()
    })
}

pub(crate) fn proxy_app_config_from_config_source_parts(
    app: AppKind,
    config: AppProxyConfig,
    settings_current_provider_id: Option<&str>,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ProxyAppConfig {
    let current_provider_id =
        current_provider_id_option_from_sources(settings_current_provider_id, None);
    proxy_app_config_from_config_parts(
        app,
        config,
        current_provider_id,
        rectifier,
        optimizer,
        copilot_optimizer,
    )
}

pub(crate) fn app_proxy_config_from_proxy_app_config(
    config: &ProxyAppConfig,
) -> Result<AppProxyConfig, String> {
    serde_json::from_value(config.raw.clone())
        .map_err(|error| format!("invalid app proxy config: {error}"))
}

pub(crate) fn proxy_runtime_config_from_config(
    config: ProxyConfig,
    privacy_filter_enabled: bool,
) -> ProxyRuntimeConfig {
    crate::proxy_core::api::config::proxy_runtime_config_from_proxy_config(
        config,
        privacy_filter_enabled,
    )
}

pub(crate) fn proxy_runtime_config_from_config_source(config: ProxyConfig) -> ProxyRuntimeConfig {
    proxy_runtime_config_from_config(config, false)
}

pub(crate) async fn proxy_runtime_config_from_db_source(
    db: &Database,
) -> ProxyCoreResult<ProxyRuntimeConfig> {
    let config = db
        .get_proxy_config()
        .await
        .map_err(|error| app_error("load runtime proxy config", error))?;
    Ok(proxy_runtime_config_from_config_source(config))
}

pub(crate) type ProviderHealth = crate::proxy_core::api::ports::ProviderHealth;
pub(crate) type ProviderKind = crate::proxy_core::api::domain::ProviderKind;
pub(crate) type ProviderAuthInfo =
    crate::proxy_core::api::auth::ProviderAuthInfo;
pub(crate) type ProviderAuthStrategy =
    crate::proxy_core::api::auth::ProviderAuthStrategy;
pub(crate) type AuthInfo = crate::proxy_core::api::ports::AuthInfo;

pub(crate) fn auth_info_from_profile_ref(
    auth_profile: Option<&AuthProfileRef>,
    source: &str,
) -> AuthInfo {
    crate::proxy_core::api::ports::auth_info_from_profile_ref(auth_profile, source)
}

pub(crate) fn auth_info_from_cc_switch_provider_config(
    auth_profile: Option<&AuthProfileRef>,
) -> AuthInfo {
    auth_info_from_profile_ref(auth_profile, "cc_switch_provider_config")
}

pub(crate) type AttemptEventChannel<'a> =
    crate::proxy_core::api::events::AttemptEventChannel<'a>;
pub(crate) type AttemptEventPayloadInput<'a> =
    crate::proxy_core::api::events::AttemptEventPayloadInput<'a>;
pub(crate) type AttemptEventPhase =
    crate::proxy_core::api::events::AttemptEventPhase;

pub(crate) fn attempt_event_payload_from_forward_attempt(
    request_id: &str,
    app_type: &str,
    attempt: &ForwardAttempt,
    error: Option<&str>,
) -> Value {
    let provider = attempt.provider();
    let channel = attempt.channel().map(|channel| AttemptEventChannel {
        channel_id: channel.channel_id.as_str(),
        channel_name: channel.channel_name.as_str(),
        interface_kind: channel.interface_kind.as_str(),
        public_model: channel.public_model.as_deref(),
        upstream_model: channel.upstream_model.as_deref(),
    });

    build_attempt_event_payload(AttemptEventPayloadInput {
        request_id,
        app_type,
        provider_id: provider.id.as_str(),
        provider_name: provider.name.as_str(),
        channel,
        error,
    })
}

pub(crate) fn attempt_event_message_from_forward_attempt(
    request_id: &str,
    app_type: &str,
    attempt: &ForwardAttempt,
    phase: AttemptEventPhase,
    error: Option<&str>,
) -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: attempt_event_name(attempt.is_channel(), phase).to_string(),
        payload: attempt_event_payload_from_forward_attempt(request_id, app_type, attempt, error),
    }
}

pub(crate) fn route_selected_event_message_from_forward_attempt(
    request_id: &str,
    app_type: &str,
    attempt: &ForwardAttempt,
) -> ProxyEventBusMessage {
    proxy_core_event_to_bus_message(ProxyCoreEvent {
        event_type: crate::proxy_core::api::events::ProxyCoreEventType::RouteSelected,
        request_id: Some(request_id.to_string()),
        channel_id: attempt
            .channel()
            .map(|channel| channel.channel_id.clone()),
        payload: attempt_event_payload_from_forward_attempt(request_id, app_type, attempt, None),
    })
}

pub(crate) type ChannelAttemptResult =
    crate::proxy_core::api::ports::ChannelAttemptResult;
pub(crate) type ChannelQuery<'a> = crate::proxy_core::api::routing::ChannelQuery<'a>;
pub(crate) type ForwardFailureCategory =
    crate::proxy_core::api::transport::ForwardFailureCategory;

pub(crate) const DEFAULT_PROXY_LISTEN_PORT: u16 =
    crate::proxy_core::api::ports::DEFAULT_PROXY_LISTEN_PORT;
pub(crate) const DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD: u32 =
    crate::proxy_core::api::ports::DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD;
pub(crate) type MediaRetryInput<'a> =
    crate::proxy_core::api::transport::MediaRetryInput<'a>;
pub(crate) type PromptCacheTraceLogInput<'a> =
    crate::proxy_core::api::transport::PromptCacheTraceLogInput<'a>;
pub(crate) type AllowResult = crate::proxy_core::api::config::AllowResult;
pub(crate) type CircuitBreakerConfig =
    crate::proxy_core::api::config::CircuitBreakerConfig;
pub(crate) type CircuitBreakerStats =
    crate::proxy_core::api::config::CircuitBreakerStats;
pub(crate) type CircuitState = crate::proxy_core::api::config::CircuitState;

pub(crate) mod circuit_breaker_log_codes {
    pub(crate) const OPEN_TO_HALF_OPEN: &str =
        crate::proxy_core::api::logging::cb::OPEN_TO_HALF_OPEN;
    pub(crate) const HALF_OPEN_TO_CLOSED: &str =
        crate::proxy_core::api::logging::cb::HALF_OPEN_TO_CLOSED;
    pub(crate) const HALF_OPEN_PROBE_FAILED: &str =
        crate::proxy_core::api::logging::cb::HALF_OPEN_PROBE_FAILED;
    pub(crate) const TRIGGERED_FAILURES: &str =
        crate::proxy_core::api::logging::cb::TRIGGERED_FAILURES;
    pub(crate) const TRIGGERED_ERROR_RATE: &str =
        crate::proxy_core::api::logging::cb::TRIGGERED_ERROR_RATE;
    pub(crate) const MANUAL_RESET: &str = crate::proxy_core::api::logging::cb::MANUAL_RESET;
}

pub(crate) mod server_log_codes {
    pub(crate) const STARTED: &str = crate::proxy_core::api::logging::srv::STARTED;
    pub(crate) const STOPPED: &str = crate::proxy_core::api::logging::srv::STOPPED;
    pub(crate) const STOP_TIMEOUT: &str = crate::proxy_core::api::logging::srv::STOP_TIMEOUT;
    pub(crate) const TASK_ERROR: &str = crate::proxy_core::api::logging::srv::TASK_ERROR;
    pub(crate) const ACCEPT_ERR: &str = crate::proxy_core::api::logging::srv::ACCEPT_ERR;
    pub(crate) const CONN_ERR: &str = crate::proxy_core::api::logging::srv::CONN_ERR;
}

pub(crate) type ProxyCoreAppKind = crate::proxy_core::api::domain::AppKind;
#[cfg(test)]
pub(crate) type ProxyCoreChannelOverrides =
    crate::proxy_core::api::domain::ChannelOverrides;
pub(crate) type ChannelSpec = crate::proxy_core::api::routing::ChannelSpec;
#[cfg(test)]
pub(crate) type ProxyCoreChannelSpec =
    crate::proxy_core::api::routing::ChannelSpec;
#[cfg(test)]
pub(crate) type ChannelStatus = crate::proxy_core::api::routing::ChannelStatus;
#[cfg(test)]
pub(crate) type ProxyCoreChannelStatus =
    crate::proxy_core::api::routing::ChannelStatus;
#[cfg(test)]
pub(crate) type ProxyCoreInterfaceKind =
    crate::proxy_core::api::routing::InterfaceKind;
#[cfg(test)]
pub(crate) type ProxyCoreModelCapabilities =
    crate::proxy_core::api::domain::ModelCapabilities;
#[cfg(test)]
pub(crate) type ProxyCoreModelRoute = crate::proxy_core::api::domain::ModelRoute;
pub(crate) type ModelCatalog = crate::proxy_core::api::model_catalog::ModelCatalog;
#[cfg(test)]
pub(crate) type ProxyCoreProviderMetadata =
    crate::proxy_core::api::domain::ProviderMetadata;
pub(crate) type ProviderSpec = crate::proxy_core::api::domain::ProviderSpec;
#[cfg(test)]
pub(crate) type ProxyCoreProviderSpec =
    crate::proxy_core::api::domain::ProviderSpec;

pub(crate) use crate::proxy_core::api::domain::{
    provider_account_ref, provider_metadata_from_input,
};

pub(crate) use crate::proxy_core::api::domain::extract_openclaw_stream_check_base_url;

pub(crate) fn provider_openclaw_stream_check_base_url(provider: &Provider) -> Option<String> {
    extract_openclaw_stream_check_base_url(&provider.settings_config)
}

pub(crate) fn provider_openclaw_has_live_provider_fields(provider: &Provider) -> bool {
    crate::proxy_core::api::domain::openclaw_settings_have_live_provider_fields(
        &provider.settings_config,
    )
}

#[derive(Debug, Clone)]
pub(crate) enum OpenClawLiveWriteConfig {
    Typed(OpenClawProviderConfig),
    Raw {
        config: Value,
        parse_error: String,
    },
    Invalid {
        parse_error: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct OpenClawLiveWritePlan {
    pub(crate) config: OpenClawLiveWriteConfig,
}

#[derive(Debug, Clone)]
pub(crate) enum OpenClawLiveWriteAction {
    Typed(OpenClawProviderConfig),
    Raw {
        config: Value,
        parse_error: String,
    },
    Reject {
        parse_error: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct OpenClawLiveWriteProjection {
    pub(crate) action: OpenClawLiveWriteAction,
}

pub(crate) fn provider_openclaw_live_write_plan(provider: &Provider) -> OpenClawLiveWritePlan {
    let config_to_write = provider.settings_config.clone();

    let config = match serde_json::from_value::<OpenClawProviderConfig>(config_to_write.clone()) {
        Ok(config) => OpenClawLiveWriteConfig::Typed(config),
        Err(error) if provider_openclaw_has_live_provider_fields(provider) => {
            OpenClawLiveWriteConfig::Raw {
                config: config_to_write,
                parse_error: error.to_string(),
            }
        }
        Err(error) => OpenClawLiveWriteConfig::Invalid {
            parse_error: error.to_string(),
        },
    };

    OpenClawLiveWritePlan { config }
}

pub(crate) fn provider_openclaw_live_write_projection(
    provider: &Provider,
) -> OpenClawLiveWriteProjection {
    let plan = provider_openclaw_live_write_plan(provider);
    let action = match plan.config {
        OpenClawLiveWriteConfig::Typed(config) => OpenClawLiveWriteAction::Typed(config),
        OpenClawLiveWriteConfig::Raw {
            config,
            parse_error,
        } => OpenClawLiveWriteAction::Raw {
            config,
            parse_error,
        },
        OpenClawLiveWriteConfig::Invalid { parse_error } => OpenClawLiveWriteAction::Reject {
            parse_error,
            message: format!(
                "OpenClaw provider '{}' has invalid config structure for live config (must contain 'baseUrl', 'api', or 'models')",
                provider.id
            ),
        },
    };

    OpenClawLiveWriteProjection { action }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenClawLiveImportIssue {
    EmptyId,
    NoModels,
    Serialization(String),
}

pub(crate) fn provider_from_openclaw_live_config(
    id: &str,
    config: &OpenClawProviderConfig,
) -> Result<Provider, OpenClawLiveImportIssue> {
    if id.trim().is_empty() {
        return Err(OpenClawLiveImportIssue::EmptyId);
    }
    if config.models.is_empty() {
        return Err(OpenClawLiveImportIssue::NoModels);
    }

    let settings_config = serde_json::to_value(config)
        .map_err(|error| OpenClawLiveImportIssue::Serialization(error.to_string()))?;
    let display_name = config
        .models
        .first()
        .and_then(|model| model.name.clone())
        .unwrap_or_else(|| id.to_string());
    let mut provider = Provider::with_id(id.to_string(), display_name, settings_config, None);
    provider.meta = Some(ProviderMeta {
        live_config_managed: Some(true),
        ..Default::default()
    });

    Ok(provider)
}

pub(crate) struct OpenClawCredentialParts<'a> {
    pub(crate) api_key: Option<&'a str>,
    pub(crate) base_url: Option<&'a str>,
}

pub(crate) fn provider_openclaw_credential_parts(
    provider: &Provider,
) -> OpenClawCredentialParts<'_> {
    OpenClawCredentialParts {
        api_key: provider
            .settings_config
            .get("apiKey")
            .and_then(Value::as_str),
        base_url: provider
            .settings_config
            .get("baseUrl")
            .and_then(Value::as_str),
    }
}

pub(crate) fn openclaw_common_config_value_from_settings(settings: &Value) -> Value {
    let mut config = settings.clone();

    if let Some(obj) = config.as_object_mut() {
        obj.remove("apiKey");
        obj.remove("baseUrl");
    }

    config
}

pub(crate) use crate::proxy_core::api::domain::extract_hermes_stream_check_base_url;

pub(crate) fn provider_hermes_stream_check_base_url(provider: &Provider) -> Option<String> {
    extract_hermes_stream_check_base_url(&provider.settings_config)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HermesLiveImportIssue {
    EmptyName,
}

pub(crate) fn provider_from_hermes_live_config(
    name: &str,
    config: Value,
) -> Result<Provider, HermesLiveImportIssue> {
    if name.trim().is_empty() {
        return Err(HermesLiveImportIssue::EmptyName);
    }

    let mut provider = Provider::with_id(name.to_string(), name.to_string(), config, None);
    provider.meta = Some(ProviderMeta {
        live_config_managed: Some(true),
        ..Default::default()
    });

    Ok(provider)
}

pub(crate) use crate::proxy_core::api::domain::extract_opencode_stream_check_npm;

pub(crate) fn provider_opencode_stream_check_npm(provider: &Provider) -> Option<String> {
    extract_opencode_stream_check_npm(&provider.settings_config)
}

pub(crate) use crate::proxy_core::api::domain::resolve_opencode_stream_check_base_url;

pub(crate) fn provider_opencode_stream_check_base_url(
    provider: &Provider,
    npm: Option<&str>,
) -> Option<String> {
    resolve_opencode_stream_check_base_url(&provider.settings_config, npm)
}

pub(crate) struct OpenCodeCredentialParts<'a> {
    pub(crate) api_key: Option<&'a str>,
    pub(crate) base_url: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenCodeCredentialIssue {
    MissingOptions,
}

pub(crate) fn provider_opencode_credential_parts(
    provider: &Provider,
) -> Result<OpenCodeCredentialParts<'_>, OpenCodeCredentialIssue> {
    let options = provider
        .settings_config
        .get("options")
        .and_then(Value::as_object)
        .ok_or(OpenCodeCredentialIssue::MissingOptions)?;

    Ok(OpenCodeCredentialParts {
        api_key: options.get("apiKey").and_then(Value::as_str),
        base_url: options.get("baseURL").and_then(Value::as_str),
    })
}

pub(crate) fn opencode_common_config_value_from_settings(settings: &Value) -> Value {
    let mut config = settings.clone();

    if let Some(obj) = config.as_object_mut() {
        if let Some(options) = obj.get_mut("options").and_then(Value::as_object_mut) {
            options.remove("apiKey");
            options.remove("baseURL");
        }
    }

    config
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommonConfigSnippetIssue {
    Serialization(String),
    TomlParse(String),
}

pub(crate) fn common_config_snippet_issue_message(issue: CommonConfigSnippetIssue) -> String {
    match issue {
        CommonConfigSnippetIssue::Serialization(error) => {
            format!("Serialization failed: {error}")
        }
        CommonConfigSnippetIssue::TomlParse(error) => format!("TOML parse error: {error}"),
    }
}

pub(crate) fn common_config_snippet_from_settings(
    app_type: &AppType,
    settings: &Value,
) -> Result<String, CommonConfigSnippetIssue> {
    match app_type {
        AppType::Claude => claude_common_config_snippet_from_settings(settings),
        AppType::ClaudeDesktop => Ok(String::new()),
        AppType::Codex => codex_common_config_snippet_from_settings(settings),
        AppType::Gemini => gemini_common_config_snippet_from_settings(settings),
        AppType::OpenCode => opencode_common_config_snippet_from_settings(settings),
        AppType::OpenClaw => openclaw_common_config_snippet_from_settings(settings),
        AppType::Hermes => Ok(String::new()),
    }
}

pub(crate) fn claude_common_config_snippet_from_settings(
    settings: &Value,
) -> Result<String, CommonConfigSnippetIssue> {
    let mut config = settings.clone();

    const ENV_EXCLUDES: &[&str] = &[
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_REASONING_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
        "ANTHROPIC_BASE_URL",
    ];
    const TOP_LEVEL_EXCLUDES: &[&str] = &["apiBaseUrl", "primaryModel", "smallFastModel"];

    if let Some(env) = config.get_mut("env").and_then(Value::as_object_mut) {
        for key in ENV_EXCLUDES {
            env.remove(*key);
        }
        if env.is_empty() {
            if let Some(obj) = config.as_object_mut() {
                obj.remove("env");
            }
        }
    }

    if let Some(obj) = config.as_object_mut() {
        for key in TOP_LEVEL_EXCLUDES {
            obj.remove(*key);
        }
    }

    if config.as_object().is_none_or(|obj| obj.is_empty()) {
        return Ok("{}".to_string());
    }

    serde_json::to_string_pretty(&config)
        .map_err(|e| CommonConfigSnippetIssue::Serialization(e.to_string()))
}

pub(crate) fn codex_common_config_snippet_from_settings(
    settings: &Value,
) -> Result<String, CommonConfigSnippetIssue> {
    let config_toml = codex_config_text_from_settings(settings).unwrap_or("");

    if config_toml.is_empty() {
        return Ok(String::new());
    }

    let mut doc = config_toml
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| CommonConfigSnippetIssue::TomlParse(e.to_string()))?;

    let root = doc.as_table_mut();
    root.remove("model");
    root.remove("model_provider");
    root.remove("base_url");
    root.remove("model_providers");

    let mut cleaned = String::new();
    let mut blank_run = 0usize;
    for line in doc.to_string().lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                cleaned.push('\n');
            }
            continue;
        }
        blank_run = 0;
        cleaned.push_str(line);
        cleaned.push('\n');
    }

    Ok(cleaned.trim().to_string())
}

pub(crate) fn gemini_common_config_snippet_from_settings(
    settings: &Value,
) -> Result<String, CommonConfigSnippetIssue> {
    let env = gemini_env_map_from_settings(settings);

    let mut snippet = Map::new();
    if let Some(env) = env {
        for (key, value) in env {
            if key == "GOOGLE_GEMINI_BASE_URL" || key == "GEMINI_API_KEY" {
                continue;
            }
            let Value::String(v) = value else {
                continue;
            };
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                snippet.insert(key.to_string(), Value::String(trimmed.to_string()));
            }
        }
    }

    if snippet.is_empty() {
        return Ok("{}".to_string());
    }

    serde_json::to_string_pretty(&Value::Object(snippet))
        .map_err(|e| CommonConfigSnippetIssue::Serialization(e.to_string()))
}

pub(crate) fn opencode_common_config_snippet_from_settings(
    settings: &Value,
) -> Result<String, CommonConfigSnippetIssue> {
    json_object_or_null_common_config_snippet(opencode_common_config_value_from_settings(settings))
}

pub(crate) fn openclaw_common_config_snippet_from_settings(
    settings: &Value,
) -> Result<String, CommonConfigSnippetIssue> {
    json_object_or_null_common_config_snippet(openclaw_common_config_value_from_settings(settings))
}

pub(crate) fn provider_default_live_import_settings(
    app_type: &AppType,
    mut settings: Value,
) -> Value {
    let _ = normalize_provider_settings_for_storage(app_type, &mut settings);
    settings
}

pub(crate) fn normalize_provider_settings_for_storage(
    app_type: &AppType,
    settings: &mut Value,
) -> bool {
    if matches!(app_type, AppType::Claude) {
        return normalize_claude_models_in_value(settings);
    }

    false
}

pub(crate) fn should_skip_manual_default_live_import(
    app_type: &AppType,
    has_non_official_seed_provider: bool,
) -> bool {
    app_type.is_additive_mode() || has_non_official_seed_provider
}

pub(crate) fn should_skip_startup_default_live_import(
    app_type: &AppType,
    has_any_provider: bool,
) -> bool {
    app_type.is_additive_mode() || has_any_provider
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderLiveSyncScope {
    AllProviders,
    CurrentProvider,
}

pub(crate) fn provider_live_sync_scope(app_type: &AppType) -> ProviderLiveSyncScope {
    if app_type.is_additive_mode() {
        ProviderLiveSyncScope::AllProviders
    } else {
        ProviderLiveSyncScope::CurrentProvider
    }
}

pub(crate) fn provider_app_has_current_provider(app_type: &AppType) -> bool {
    !app_type.is_additive_mode()
}

pub(crate) fn provider_should_sync_to_live(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.live_config_managed)
        != Some(false)
}

pub(crate) fn provider_initial_live_config_managed_marker(
    app_type: &AppType,
    add_to_live: bool,
) -> Option<bool> {
    if app_type.is_additive_mode() {
        Some(add_to_live)
    } else {
        None
    }
}

pub(crate) fn provider_supports_legacy_common_config_migration(app_type: &AppType) -> bool {
    !app_type.is_additive_mode()
}

pub(crate) fn should_skip_provider_legacy_common_config_migration(
    app_type: &AppType,
    legacy_snippet: &str,
) -> bool {
    !provider_supports_legacy_common_config_migration(app_type) || legacy_snippet.trim().is_empty()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderLiveConfigPresenceErrorPolicy {
    Strict,
    TreatErrorAsMissing,
}

pub(crate) fn provider_live_config_presence_error_policy(
    live_config_managed: Option<bool>,
) -> ProviderLiveConfigPresenceErrorPolicy {
    if live_config_managed == Some(false) {
        ProviderLiveConfigPresenceErrorPolicy::TreatErrorAsMissing
    } else {
        ProviderLiveConfigPresenceErrorPolicy::Strict
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderKeyChangePolicyIssue {
    UnsupportedAppMode,
    ExclusiveCurrentStateProvider,
}

pub(crate) fn provider_key_change_policy_issue(
    app_type: &AppType,
    existing_provider: Option<&Provider>,
) -> Option<ProviderKeyChangePolicyIssue> {
    if !app_type.is_additive_mode() {
        return Some(ProviderKeyChangePolicyIssue::UnsupportedAppMode);
    }

    if matches!(app_type, AppType::OpenCode)
        && matches!(
            existing_provider.and_then(|provider| provider.category.as_deref()),
            Some("omo") | Some("omo-slim")
        )
    {
        return Some(ProviderKeyChangePolicyIssue::ExclusiveCurrentStateProvider);
    }

    None
}

pub(crate) fn provider_key_change_policy_issue_message(
    issue: ProviderKeyChangePolicyIssue,
) -> &'static str {
    match issue {
        ProviderKeyChangePolicyIssue::UnsupportedAppMode => {
            "Only additive-mode providers support changing provider key"
        }
        ProviderKeyChangePolicyIssue::ExclusiveCurrentStateProvider => {
            "Provider key cannot be changed for OMO/OMO Slim providers"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderAdditiveLiveWriteAction {
    SkipExclusiveCurrentStateProvider,
    SkipNotRequested,
    Write,
}

pub(crate) fn provider_additive_live_write_action(
    app_type: &AppType,
    provider: &Provider,
    add_to_live: bool,
) -> ProviderAdditiveLiveWriteAction {
    if matches!(app_type, AppType::OpenCode)
        && matches!(provider.category.as_deref(), Some("omo") | Some("omo-slim"))
    {
        return ProviderAdditiveLiveWriteAction::SkipExclusiveCurrentStateProvider;
    }

    if !add_to_live {
        return ProviderAdditiveLiveWriteAction::SkipNotRequested;
    }

    ProviderAdditiveLiveWriteAction::Write
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderOmoVariant {
    Standard,
    Slim,
}

impl ProviderOmoVariant {
    pub(crate) fn category(self) -> &'static str {
        match self {
            ProviderOmoVariant::Standard => "omo",
            ProviderOmoVariant::Slim => "omo-slim",
        }
    }

    fn opposite(self) -> Self {
        match self {
            ProviderOmoVariant::Standard => ProviderOmoVariant::Slim,
            ProviderOmoVariant::Slim => ProviderOmoVariant::Standard,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderOmoSwitchPair {
    pub(crate) enable: ProviderOmoVariant,
    pub(crate) disable: ProviderOmoVariant,
}

pub(crate) fn provider_omo_variant_for_category(
    app_type: &AppType,
    category: Option<&str>,
) -> Option<ProviderOmoVariant> {
    if !matches!(app_type, AppType::OpenCode) {
        return None;
    }

    match category {
        Some("omo") => Some(ProviderOmoVariant::Standard),
        Some("omo-slim") => Some(ProviderOmoVariant::Slim),
        _ => None,
    }
}

pub(crate) fn provider_omo_switch_pair(
    app_type: &AppType,
    provider: &Provider,
) -> Option<ProviderOmoSwitchPair> {
    let enable = provider_omo_variant_for_category(app_type, provider.category.as_deref())?;

    Some(ProviderOmoSwitchPair {
        enable,
        disable: enable.opposite(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderAdditiveUpdateRoute {
    OmoVariant(ProviderOmoVariant),
    LiveConfigPresence,
}

pub(crate) fn provider_additive_update_route(
    app_type: &AppType,
    category: Option<&str>,
) -> Option<ProviderAdditiveUpdateRoute> {
    if !app_type.is_additive_mode() {
        return None;
    }

    if let Some(omo_variant) = provider_omo_variant_for_category(app_type, category) {
        return Some(ProviderAdditiveUpdateRoute::OmoVariant(omo_variant));
    }

    Some(ProviderAdditiveUpdateRoute::LiveConfigPresence)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderSwitchDispatch {
    Normal,
    TakeoverAware,
}

pub(crate) fn provider_switch_dispatch(
    app_type: &AppType,
    provider: &Provider,
) -> ProviderSwitchDispatch {
    if matches!(app_type, AppType::OpenCode)
        && matches!(provider.category.as_deref(), Some("omo") | Some("omo-slim"))
    {
        return ProviderSwitchDispatch::Normal;
    }

    if matches!(app_type, AppType::ClaudeDesktop) {
        return ProviderSwitchDispatch::Normal;
    }

    ProviderSwitchDispatch::TakeoverAware
}

pub(crate) fn provider_switch_requires_takeover_lock(app_type: &AppType) -> bool {
    matches!(app_type, AppType::Claude | AppType::Codex | AppType::Gemini)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderTakeoverLiveSyncTarget {
    LiveConfig,
    LiveBackup,
}

pub(crate) fn provider_takeover_live_sync_target(
    app_type: &AppType,
) -> ProviderTakeoverLiveSyncTarget {
    if matches!(app_type, AppType::ClaudeDesktop) {
        ProviderTakeoverLiveSyncTarget::LiveConfig
    } else {
        ProviderTakeoverLiveSyncTarget::LiveBackup
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderLiveRemovalTarget {
    OpenCode,
    OpenClaw,
    Hermes,
}

pub(crate) fn provider_live_removal_target(
    app_type: &AppType,
) -> Option<ProviderLiveRemovalTarget> {
    match app_type {
        AppType::OpenCode => Some(ProviderLiveRemovalTarget::OpenCode),
        AppType::OpenClaw => Some(ProviderLiveRemovalTarget::OpenClaw),
        AppType::Hermes => Some(ProviderLiveRemovalTarget::Hermes),
        _ => None,
    }
}

pub(crate) fn provider_delete_is_current_provider(
    provider_id: &str,
    local_current: Option<&str>,
    db_current: Option<&str>,
) -> bool {
    local_current == Some(provider_id) || db_current == Some(provider_id)
}

pub(crate) fn provider_switch_backfill_source_id<'a>(
    app_type: &AppType,
    current_id: Option<&'a str>,
    target_id: &str,
) -> Option<&'a str> {
    if !provider_app_has_current_provider(app_type) {
        return None;
    }

    match current_id {
        Some(current_id) if current_id != target_id => Some(current_id),
        _ => None,
    }
}

pub(crate) fn provider_switch_should_mark_live_config_managed(
    app_type: &AppType,
    live_config_managed: Option<bool>,
) -> bool {
    app_type.is_additive_mode() && live_config_managed != Some(true)
}

/// Reads old Claude model keys, writes DEFAULT_* keys, and deletes legacy SMALL_FAST.
pub(crate) fn normalize_claude_models_in_value(settings: &mut Value) -> bool {
    let mut changed = false;
    let env = match settings.get_mut("env").and_then(Value::as_object_mut) {
        Some(obj) => obj,
        None => return changed,
    };

    let model = env
        .get("ANTHROPIC_MODEL")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let small_fast = env
        .get("ANTHROPIC_SMALL_FAST_MODEL")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let current_haiku = env
        .get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let current_sonnet = env
        .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let current_opus = env
        .get("ANTHROPIC_DEFAULT_OPUS_MODEL")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let target_haiku = current_haiku
        .or_else(|| small_fast.clone())
        .or_else(|| model.clone());
    let target_sonnet = current_sonnet
        .or_else(|| model.clone())
        .or_else(|| small_fast.clone());
    let target_opus = current_opus
        .or_else(|| model.clone())
        .or_else(|| small_fast.clone());

    if env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").is_none() {
        if let Some(v) = target_haiku {
            env.insert(
                "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                Value::String(v),
            );
            changed = true;
        }
    }
    if env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").is_none() {
        if let Some(v) = target_sonnet {
            env.insert(
                "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                Value::String(v),
            );
            changed = true;
        }
    }
    if env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none() {
        if let Some(v) = target_opus {
            env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), Value::String(v));
            changed = true;
        }
    }

    if env.remove("ANTHROPIC_SMALL_FAST_MODEL").is_some() {
        changed = true;
    }

    changed
}

fn json_object_or_null_common_config_snippet(
    config: Value,
) -> Result<String, CommonConfigSnippetIssue> {
    if config.is_null() || (config.is_object() && config.as_object().unwrap().is_empty()) {
        return Ok("{}".to_string());
    }

    serde_json::to_string_pretty(&config)
        .map_err(|e| CommonConfigSnippetIssue::Serialization(e.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderCredentialValues {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderCredentialIssue {
    ClaudeEnvMissing,
    ClaudeApiKeyMissing,
    ClaudeBaseUrlMissing,
    ClaudeDesktopRequiresGateway,
    CodexAuthMissing,
    CodexApiKeyMissing,
    CodexBaseUrlMissing,
    CodexBaseUrlInvalid,
    GeminiApiKeyMissing,
    OpenCodeOptionsMissing,
    OpenCodeApiKeyMissing,
    OpenClawApiKeyMissing,
}

pub(crate) fn provider_credential_issue_spec(issue: ProviderCredentialIssue) -> LocalizedErrorSpec {
    match issue {
        ProviderCredentialIssue::ClaudeEnvMissing => LocalizedErrorSpec::new(
            "provider.claude.env.missing",
            "配置格式错误: 缺少 env",
            "Invalid configuration: missing env section",
        ),
        ProviderCredentialIssue::ClaudeApiKeyMissing => LocalizedErrorSpec::new(
            "provider.claude.api_key.missing",
            "缺少 API Key",
            "API key is missing",
        ),
        ProviderCredentialIssue::ClaudeBaseUrlMissing => LocalizedErrorSpec::new(
            "provider.claude.base_url.missing",
            "缺少 ANTHROPIC_BASE_URL 配置",
            "Missing ANTHROPIC_BASE_URL configuration",
        ),
        ProviderCredentialIssue::ClaudeDesktopRequiresGateway => LocalizedErrorSpec::new(
            "provider.claude_desktop.gateway.required",
            "Claude Desktop 凭据必须通过 gateway 配置解析",
            "Claude Desktop credentials must be resolved through gateway configuration",
        ),
        ProviderCredentialIssue::CodexAuthMissing => LocalizedErrorSpec::new(
            "provider.codex.auth.missing",
            "配置格式错误: 缺少 auth",
            "Invalid configuration: missing auth section",
        ),
        ProviderCredentialIssue::CodexApiKeyMissing => LocalizedErrorSpec::new(
            "provider.codex.api_key.missing",
            "缺少 API Key",
            "API key is missing",
        ),
        ProviderCredentialIssue::CodexBaseUrlMissing => LocalizedErrorSpec::new(
            "provider.codex.base_url.missing",
            "config.toml 中缺少 base_url 配置",
            "base_url is missing from config.toml",
        ),
        ProviderCredentialIssue::CodexBaseUrlInvalid => LocalizedErrorSpec::new(
            "provider.codex.base_url.invalid",
            "config.toml 中 base_url 格式错误",
            "base_url in config.toml has invalid format",
        ),
        ProviderCredentialIssue::GeminiApiKeyMissing => LocalizedErrorSpec::new(
            "gemini.missing_api_key",
            "缺少 GEMINI_API_KEY",
            "Missing GEMINI_API_KEY",
        ),
        ProviderCredentialIssue::OpenCodeOptionsMissing => LocalizedErrorSpec::new(
            "provider.opencode.options.missing",
            "配置格式错误: 缺少 options",
            "Invalid configuration: missing options section",
        ),
        ProviderCredentialIssue::OpenCodeApiKeyMissing => LocalizedErrorSpec::new(
            "provider.opencode.api_key.missing",
            "缺少 API Key",
            "API key is missing",
        ),
        ProviderCredentialIssue::OpenClawApiKeyMissing => LocalizedErrorSpec::new(
            "provider.openclaw.api_key.missing",
            "缺少 API Key",
            "API key is missing",
        ),
    }
}

pub(crate) fn provider_credential_values(
    provider: &Provider,
    app_type: &AppType,
) -> Result<ProviderCredentialValues, ProviderCredentialIssue> {
    match app_type {
        AppType::Claude => {
            let credentials = claude_env_credentials_from_settings(&provider.settings_config)
                .ok_or(ProviderCredentialIssue::ClaudeEnvMissing)?;
            let api_key = credentials
                .api_key
                .ok_or(ProviderCredentialIssue::ClaudeApiKeyMissing)?
                .to_string();
            let base_url = credentials
                .base_url
                .ok_or(ProviderCredentialIssue::ClaudeBaseUrlMissing)?
                .to_string();

            Ok(ProviderCredentialValues { api_key, base_url })
        }
        AppType::ClaudeDesktop => Err(ProviderCredentialIssue::ClaudeDesktopRequiresGateway),
        AppType::Codex => {
            let auth = codex_auth_object_value_from_settings(&provider.settings_config)
                .ok_or(ProviderCredentialIssue::CodexAuthMissing)?;
            let config_toml =
                codex_config_text_from_settings(&provider.settings_config).unwrap_or("");
            let api_key = codex_api_key_from_auth_and_config(Some(auth), Some(config_toml))
                .ok_or(ProviderCredentialIssue::CodexApiKeyMissing)?;
            let base_url = if config_toml.contains("base_url") {
                let re = Regex::new(r#"base_url\s*=\s*["']([^"']+)["']"#)
                    .expect("static Codex base_url regex must compile");
                re.captures(config_toml)
                    .and_then(|caps| caps.get(1))
                    .map(|m| m.as_str().to_string())
                    .ok_or(ProviderCredentialIssue::CodexBaseUrlInvalid)?
            } else {
                return Err(ProviderCredentialIssue::CodexBaseUrlMissing);
            };

            Ok(ProviderCredentialValues { api_key, base_url })
        }
        AppType::Gemini => {
            let env_map = gemini_env_map_from_settings(&provider.settings_config);
            let api_key = env_map
                .and_then(|env| env.get("GEMINI_API_KEY"))
                .and_then(Value::as_str)
                .ok_or(ProviderCredentialIssue::GeminiApiKeyMissing)?
                .to_string();
            let base_url = env_map
                .and_then(|env| env.get("GOOGLE_GEMINI_BASE_URL"))
                .and_then(Value::as_str)
                .unwrap_or("https://generativelanguage.googleapis.com")
                .to_string();

            Ok(ProviderCredentialValues { api_key, base_url })
        }
        AppType::OpenCode => {
            let parts =
                provider_opencode_credential_parts(provider).map_err(|issue| match issue {
                    OpenCodeCredentialIssue::MissingOptions => {
                        ProviderCredentialIssue::OpenCodeOptionsMissing
                    }
                })?;
            let api_key = parts
                .api_key
                .ok_or(ProviderCredentialIssue::OpenCodeApiKeyMissing)?
                .to_string();
            let base_url = parts.base_url.unwrap_or("").to_string();

            Ok(ProviderCredentialValues { api_key, base_url })
        }
        AppType::OpenClaw | AppType::Hermes => {
            let parts = provider_openclaw_credential_parts(provider);
            let api_key = parts
                .api_key
                .ok_or(ProviderCredentialIssue::OpenClawApiKeyMissing)?
                .to_string();
            let base_url = parts.base_url.unwrap_or("").to_string();

            Ok(ProviderCredentialValues { api_key, base_url })
        }
    }
}

pub(crate) struct OpenCodeLiveProviderFragment {
    pub(crate) config: Value,
    pub(crate) from_full_config: bool,
}

pub(crate) fn provider_opencode_live_provider_fragment(
    provider: &Provider,
) -> OpenCodeLiveProviderFragment {
    let Some(obj) = provider.settings_config.as_object() else {
        return OpenCodeLiveProviderFragment {
            config: provider.settings_config.clone(),
            from_full_config: false,
        };
    };

    let from_full_config = obj.contains_key("$schema") || obj.contains_key("provider");
    if from_full_config {
        let config = obj
            .get("provider")
            .and_then(|providers| providers.get(&provider.id))
            .cloned()
            .unwrap_or_else(|| provider.settings_config.clone());
        return OpenCodeLiveProviderFragment {
            config,
            from_full_config,
        };
    }

    OpenCodeLiveProviderFragment {
        config: provider.settings_config.clone(),
        from_full_config,
    }
}

pub(crate) use crate::proxy_core::api::domain::{
    opencode_settings_have_live_provider_fields as opencode_live_provider_fragment_has_provider_fields,
};

#[derive(Debug, Clone)]
pub(crate) enum OpenCodeLiveWriteConfig {
    Typed(OpenCodeProviderConfig),
    Raw {
        config: Value,
        parse_error: String,
    },
    Invalid {
        parse_error: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct OpenCodeLiveWritePlan {
    pub(crate) config: OpenCodeLiveWriteConfig,
    pub(crate) from_full_config: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum OpenCodeLiveWriteAction {
    Typed(OpenCodeProviderConfig),
    Raw {
        config: Value,
        parse_error: String,
    },
    Reject {
        parse_error: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct OpenCodeLiveWriteProjection {
    pub(crate) action: OpenCodeLiveWriteAction,
    pub(crate) from_full_config: bool,
}

pub(crate) fn provider_opencode_live_write_plan(
    provider: &Provider,
) -> OpenCodeLiveWritePlan {
    let fragment = provider_opencode_live_provider_fragment(provider);
    let config_to_write = fragment.config;

    let config = match serde_json::from_value::<OpenCodeProviderConfig>(config_to_write.clone()) {
        Ok(config) => OpenCodeLiveWriteConfig::Typed(config),
        Err(error) if opencode_live_provider_fragment_has_provider_fields(&config_to_write) => {
            OpenCodeLiveWriteConfig::Raw {
                config: config_to_write,
                parse_error: error.to_string(),
            }
        }
        Err(error) => OpenCodeLiveWriteConfig::Invalid {
            parse_error: error.to_string(),
        },
    };

    OpenCodeLiveWritePlan {
        config,
        from_full_config: fragment.from_full_config,
    }
}

pub(crate) fn provider_opencode_live_write_projection(
    provider: &Provider,
) -> OpenCodeLiveWriteProjection {
    let plan = provider_opencode_live_write_plan(provider);
    let action = match plan.config {
        OpenCodeLiveWriteConfig::Typed(config) => OpenCodeLiveWriteAction::Typed(config),
        OpenCodeLiveWriteConfig::Raw {
            config,
            parse_error,
        } => OpenCodeLiveWriteAction::Raw {
            config,
            parse_error,
        },
        OpenCodeLiveWriteConfig::Invalid { parse_error } => OpenCodeLiveWriteAction::Reject {
            parse_error,
            message: format!(
                "OpenCode provider '{}' has invalid config structure for live config (must contain 'npm' or 'options')",
                provider.id
            ),
        },
    };

    OpenCodeLiveWriteProjection {
        action,
        from_full_config: plan.from_full_config,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenCodeLiveImportIssue {
    Serialization(String),
}

pub(crate) fn provider_from_opencode_live_config(
    id: &str,
    config: &OpenCodeProviderConfig,
) -> Result<Provider, OpenCodeLiveImportIssue> {
    let settings_config = serde_json::to_value(config)
        .map_err(|error| OpenCodeLiveImportIssue::Serialization(error.to_string()))?;
    let mut provider = Provider::with_id(
        id.to_string(),
        config.name.clone().unwrap_or_else(|| id.to_string()),
        settings_config,
        None,
    );
    provider.meta = Some(ProviderMeta {
        live_config_managed: Some(true),
        ..Default::default()
    });

    Ok(provider)
}

pub(crate) fn channel_auth_profile_resolution(
    auth_profile_ref: Option<&str>,
    app_type: &str,
) -> ChannelAuthProfileResolution {
    let auth_profile_ref = auth_profile_ref.map(AuthProfileRef::new);
    crate::proxy_core::api::domain::channel_auth_profile_resolution(
        auth_profile_ref.as_ref(),
        app_type,
    )
}

pub(crate) fn channel_auth_profile_missing_provider_warning(
    app_type: &str,
    auth_profile_ref: Option<&str>,
) -> String {
    crate::proxy_core::api::domain::channel_auth_profile_missing_provider_warning(
        app_type,
        auth_profile_ref,
    )
}

pub(crate) enum ChannelAuthProfileAction {
    Provider {
        provider_id: String,
        missing_provider_warning: String,
    },
    ChannelKey {
        channel_id: String,
        key_ref: String,
    },
    Ignore,
}

pub(crate) fn channel_auth_profile_action(
    app_type: &str,
    auth_profile_ref: Option<&str>,
    channel_id: Option<&str>,
) -> ChannelAuthProfileAction {
    match channel_auth_profile_resolution(auth_profile_ref, app_type) {
        ChannelAuthProfileResolution::Provider { provider_id } => {
            ChannelAuthProfileAction::Provider {
                provider_id,
                missing_provider_warning: channel_auth_profile_missing_provider_warning(
                    app_type,
                    auth_profile_ref,
                ),
            }
        }
        ChannelAuthProfileResolution::ChannelKey { key_ref } => {
            let Some(channel_id) = channel_id else {
                return ChannelAuthProfileAction::Ignore;
            };
            ChannelAuthProfileAction::ChannelKey {
                channel_id: channel_id.to_string(),
                key_ref,
            }
        }
        ChannelAuthProfileResolution::Ignore => ChannelAuthProfileAction::Ignore,
    }
}

pub(crate) use crate::proxy_core::api::domain::{channel_spec_from_input, model_route_from_input};

#[cfg(test)]
pub(crate) type ProxyCoreUpstreamEndpoint =
    crate::proxy_core::api::domain::UpstreamEndpoint;
pub(crate) type UpstreamAuthHeadersInput<'a> =
    crate::proxy_core::api::transport::UpstreamAuthHeadersInput<'a>;
pub(crate) type UpstreamRequestHeadersInput<'a> =
    crate::proxy_core::api::transport::UpstreamRequestHeadersInput<'a>;
pub(crate) type UpstreamSendPolicyInput =
    crate::proxy_core::api::transport::UpstreamSendPolicyInput;
pub(crate) type UpstreamTransportKind =
    crate::proxy_core::api::transport::UpstreamTransportKind;
pub(crate) type ChannelRequestValidationError =
    crate::proxy_core::api::routing::ChannelRequestValidationError;
pub(crate) type ChannelRouteSource =
    crate::proxy_core::api::management::ChannelRouteSource;
pub(crate) type ChannelRecord = crate::proxy_core::api::management::ChannelRecord;
pub type ChannelReachabilityStatus =
    crate::proxy_core::api::management::ChannelReachabilityStatus;
pub type StreamCheckConfig = crate::proxy_core::api::management::StreamCheckConfig;
pub type StreamCheckResult = crate::proxy_core::api::management::StreamCheckResult;
pub(crate) type ChannelKeyRecord =
    crate::proxy_core::api::management::ChannelKeyRecord;
pub(crate) type ChannelKeyRecordInput =
    crate::proxy_core::api::management::ChannelKeyRecordInput;
pub(crate) type ChannelModelRecord =
    crate::proxy_core::api::management::ChannelModelRecord;
pub(crate) type ChannelModelRecordInput =
    crate::proxy_core::api::management::ChannelModelRecordInput;
pub(crate) type ChannelRecordInput =
    crate::proxy_core::api::management::ChannelRecordInput;

pub(crate) use crate::proxy_core::api::management::{
    channel_key_record_from_input, channel_model_record_from_input, channel_record_from_input,
};

pub(crate) type InterfaceKind = crate::proxy_core::api::routing::InterfaceKind;
pub(crate) type LegacyChannelModelProjection =
    crate::proxy_core::api::routing::LegacyChannelModelProjection;
pub(crate) type LegacyChannelProjection =
    crate::proxy_core::api::routing::LegacyChannelProjection;
#[cfg(test)]
pub(crate) type LegacyChannelProjectionInput =
    crate::proxy_core::api::routing::LegacyChannelProjectionInput;
pub(crate) type LegacyChannelMigrationPlanInput =
    crate::proxy_core::api::routing::LegacyChannelMigrationPlanInput;
pub(crate) type LegacyEndpointInput =
    crate::proxy_core::api::routing::LegacyEndpointInput;
pub(crate) type LegacyModelRouteInput =
    crate::proxy_core::api::routing::LegacyModelRouteInput;
pub(crate) type LegacyProviderChannelMigrationInput =
    crate::proxy_core::api::routing::LegacyProviderChannelMigrationInput;
pub(crate) type LegacyProviderProjectionInput =
    crate::proxy_core::api::routing::LegacyProviderProjectionInput;
pub(crate) type ProxyChannelModelWriteRequest =
    crate::proxy_core::api::management::ProxyChannelModelWriteRequest;
pub(crate) type ProxyChannelKeyPatchRequest =
    crate::proxy_core::api::management::ProxyChannelKeyPatchRequest;
pub(crate) type ProxyChannelKeyWriteRequest =
    crate::proxy_core::api::management::ProxyChannelKeyWriteRequest;
pub(crate) type ProxyChannelPatchRequest =
    crate::proxy_core::api::management::ProxyChannelPatchRequest;
pub(crate) type ProxyChannelWriteRequest =
    crate::proxy_core::api::management::ProxyChannelWriteRequest;
#[cfg(test)]
pub(crate) type ProviderSelectionCandidate =
    crate::proxy_core::api::routing::ProviderSelectionCandidate;
pub(crate) type ProviderFailoverCircuitLookup =
    crate::proxy_core::api::routing::ProviderFailoverCircuitLookup;
pub(crate) type ProviderSelectionFailure =
    crate::proxy_core::api::routing::ProviderSelectionFailure;
pub(crate) type ProviderSelectionInput =
    crate::proxy_core::api::routing::ProviderSelectionInput;
pub(crate) type AutoFailoverToggleInput =
    crate::proxy_core::api::routing::AutoFailoverToggleInput;
pub(crate) type FailoverQueuePosition =
    crate::proxy_core::api::routing::FailoverQueuePosition;
pub(crate) type ProxyCoreError = crate::proxy_core::api::errors::ProxyCoreError;
#[cfg(test)]
pub(crate) type ProxyCoreEventType = crate::proxy_core::api::events::ProxyCoreEventType;
pub(crate) type AppKind = crate::proxy_core::api::domain::AppKind;
#[cfg(test)]
pub(crate) type RetryPolicy = crate::proxy_core::api::domain::RetryPolicy;
pub(crate) type RouteResolveRequest =
    crate::proxy_core::api::management::RouteResolveRequest;
pub(crate) type RouteResolveResponse =
    crate::proxy_core::api::management::RouteResolveResponse;
pub(crate) type RouteCandidateCircuitKey =
    crate::proxy_core::api::routing::RouteCandidateCircuitKey;
pub(crate) type ChannelRouteCandidate =
    crate::proxy_core::api::routing::ChannelRouteCandidate;
pub(crate) type ResolvedChannelAttempt =
    crate::proxy_core::api::routing::ResolvedChannelAttempt;
pub(crate) type RoutePlan = crate::proxy_core::api::routing::RoutePlan;
pub(crate) type RouteSelection = crate::proxy_core::api::routing::RouteSelection;
pub(crate) type CodexProxyErrorContext<'a> =
    crate::proxy_core::api::transforms::CodexProxyErrorContext<'a>;
pub(crate) type CodexProxyErrorKind =
    crate::proxy_core::api::transforms::CodexProxyErrorKind;
pub(crate) type ForwardFailureKind =
    crate::proxy_core::api::transport::ForwardFailureKind;
pub(crate) type ManagementAuthError =
    crate::proxy_core::api::auth::ManagementAuthError;
pub(crate) type CircuitBreakerFailureDecision =
    crate::proxy_core::api::config::CircuitBreakerFailureDecision;
pub(crate) use crate::proxy_core::api::management::channel_not_found_error;
pub(crate) use crate::proxy_core::api::ports::{
    AppSummaryConfig, AuthProvider, ChannelHealthReset, ChannelHealthStore,
    ChannelReachabilityProbe, ChannelSource, ForwardPipeline, ModelCatalogProvider,
    ProviderSource, ProxyConfigSource, ProxyEventSink, ProxyServices, RoutePolicySource,
    RouteResolver, UsageSink, channel_health_reset_from_parts,
};
pub(crate) use crate::proxy_core::api::domain::channel_matches_query;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::routing::DEFAULT_ROUTE_GROUP;
pub(crate) use crate::proxy_core::api::routing::{RoutePolicy, RouteRequest};
pub(crate) use crate::proxy_core::api::transforms::{
    claude_api_format_from_metadata, CLAUDE_API_FORMAT_METADATA_KEY,
};
pub(crate) use crate::proxy_core::api::auth::{
    channel_auth_profile_missing_key_error_message, extract_claude_auth_key_from_settings,
    is_gemini_oauth_key_shape, parse_gemini_oauth_credentials,
    resolve_management_auth_decision, validate_claude_desktop_gateway_bearer_header,
    validate_management_bearer_header, settings_config_with_channel_auth_key,
    ManagementAuthDecision,
};
pub(crate) use crate::proxy_core::api::management::{
    channel_health_update_from_input,
    channel_reachability_result_from_stream_check_result as stream_check_result_to_channel_reachability,
    channel_reachability_status_from_latency, provider_health_update_from_input,
    should_retry_channel_reachability_failure,
    AppChannelListQuery,
    AppChannelManagementRequest, AppChannelResponse, AppListRequest, AppListResponse,
    AppModelCatalogRequest,
    AppModelListQuery, ChannelCreateRequest,
    CHANNEL_HEALTH_UNKNOWN_STATUS,
    ChannelDeleteResponse, ChannelHealthResetResponse,
    ChannelHealthUpdateInput, ChannelKeyDeleteResponse,
    ChannelKeyPathRequest, ChannelKeyRecordResponse,
    ChannelKeysResponse, ChannelListQuery,
    ChannelListRequest, ChannelListResponse,
    ChannelMigrationMaterializeInput, ChannelMigrationMaterializeResponse,
    ChannelMigrationPreviewInput, ChannelMigrationPreviewResponse, ChannelModelsResponse,
    ChannelPathRequest, ChannelRecordResponse,
    ChannelRouteRejected,
    ChannelTestProbeRequest, ChannelTestResponse, CurrentRouteResponse, GroupListQuery,
    GroupListRequest, HealthCheckRequest, HealthCheckResponse, ManagementAppPathRequest,
    ProviderHealthUpdateInput, ProviderListResponse,
    ProxyChannelModelsReplaceRequest, ProxyChannelTestRequest, ProxyStatusRequest,
    ProxyStatusResponse, RouteGroupListResponse,
    RouteResolveManagementRequest,
};
pub(crate) use crate::proxy_core::api::model_catalog::{
    client_model_catalog_source_for_app, ClientModelCatalogResponse, ClientModelCatalogSource,
    RoutableModelList,
};
pub(crate) use crate::proxy_core::api::transforms::{
    anthropic_request_to_gemini_request_with_shadow, anthropic_to_openai_chat_request,
    anthropic_to_openai_responses_request, append_utf8_safe,
    chat_completion_to_response_with_context, claude_stream_usage_event_filter,
    claude_transform_unlabeled_sse_aggregation, codex_stream_usage_event_filter,
    create_codex_chat_to_responses_sse_stream_with_context,
    create_gemini_to_anthropic_sse_stream_with_callbacks,
    create_openai_chat_to_anthropic_sse_stream,
    create_openai_responses_to_anthropic_sse_stream, extract_anthropic_tool_schema_hints,
    build_gemini_upstream_url, gemini_response_to_anthropic_message,
    gemini_response_to_anthropic_message_with_shadow, inspect_codex_chat_history_sse_block,
    openai_chat_to_anthropic_message, openai_responses_to_anthropic_message,
    should_aggregate_codex_oauth_responses_sse,
    should_preserve_reasoning_content_for_openai_chat, should_use_claude_transform_streaming,
    take_sse_block,
};
pub(crate) use crate::proxy_core::api::transport::{
    append_query_to_endpoint_path, parse_upstream_json_or_unlabeled_sse,
    rebuilt_json_proxy_response, strip_endpoint_prefix, transformed_sse_proxy_response, ProxyBody,
    ProxyRequest, UnlabeledSseFallbackLogContext, UnlabeledSseFallbackLogLevel,
    UpstreamSseAggregationKind,
};
pub(crate) use crate::proxy_core::api::usage::{
    CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG, OPENAI_PARSER_CONFIG,
    error_usage_record_with_request_id_fallback,
    transformed_response_usage_record_with_request_id_fallback,
    transformed_streaming_response_usage_record_with_request_id_fallback,
    usage_logging_enabled_from_config_flag, usage_record_debug_log_message,
    usage_record_failure_warning_message, usage_selected_provider_missing_log_message,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::usage::success_usage_record_with_request_id_fallback;
pub(crate) use crate::proxy_core::api::auth::validate_managed_account_upstream_auth;
pub(crate) use crate::proxy_core::api::config::{
    app_proxy_config_defaults_for_app, app_type_from_circuit_key, cache_injection_log_message,
    channel_circuit_key, channel_circuit_key_prefix, circuit_breaker_config_from_app_config,
    circuit_failure_threshold_from_app_config, normalize_thinking_type, provider_circuit_key,
    provider_circuit_key_prefix, rectify_anthropic_request, rectify_thinking_budget,
    should_rectify_thinking_budget, should_rectify_thinking_signature,
    thinking_optimization_log_message,
};
use crate::proxy_core::api::events::{
    attempt_event_name, build_attempt_event_payload,
    build_proxy_official_warning_event_payload, build_provider_switched_event_payload,
    build_request_started_event_payload,
};
pub(crate) use crate::proxy_core::api::model_catalog::{
    apply_copilot_model_normalization, resolve_copilot_model_against_ids,
    strip_one_m_suffix_for_upstream, strip_one_m_suffix_for_upstream_from_body,
};
pub(crate) use crate::proxy_core::api::transforms::{
    resolve_claude_forward_api_format, responses_to_chat_completions_with_options,
};
pub(crate) use crate::proxy_core::api::transport::{
    append_query_to_full_url, apply_bedrock_pre_send_optimizers,
    apply_copilot_warmup_model_override, bedrock_env_flag_from_provider_settings,
    build_claude_auth_headers, build_claude_upstream_url, build_codex_bearer_auth_headers,
    build_codex_oauth_session_headers, build_copilot_auth_headers, build_gemini_auth_headers,
    build_retryable_forward_failure_log, build_terminal_forward_failure_log,
    build_upstream_auth_headers, categorize_forward_failure, classify_copilot_request,
    claude_transform_endpoint_rewrite_input_from_body,
    contains_image_blocks, is_codex_chat_full_endpoint_base,
    invalid_upstream_url_error_message, is_openai_o_series, is_unsupported_image_error,
    merge_copilot_tool_results,
    parse_json_request_body, parse_json_request_body_or_null,
    prepare_upstream_request_body_with_report, prompt_cache_trace_log_message,
    replace_image_blocks_with_marker, replace_images_for_text_only_model,
    request_body_filter_log_message, request_body_read_error_message,
    request_body_serialize_error_message,
    resolve_copilot_deterministic_interaction_id,
    resolve_codex_provider_uses_chat_completions, resolve_copilot_optimizer_session_id,
    resolve_copilot_request_id_with_fallback, resolve_media_prevention_policy,
    resolved_copilot_dynamic_base_url, sanitize_copilot_orphan_tool_results,
    should_apply_bedrock_pre_send_optimizer, should_check_media_retry,
    should_convert_codex_responses_endpoint_to_chat,
    should_failover_after_rectifier_retry_failure,
    should_preserve_exact_request_header_case, should_resolve_copilot_dynamic_endpoint,
    should_send_anthropic_request_headers, should_trigger_media_retry, split_endpoint_and_query,
    strip_copilot_thinking_blocks, supports_reasoning_effort,
    UNSUPPORTED_IMAGE_MARKER,
    build_codex_upstream_url, rewrite_claude_transform_endpoint,
};
pub(crate) use crate::proxy_core::api::transport::{
    extract_gemini_model_from_path, request_model_for_forward,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::interface_kind_for_forward;
pub(crate) use crate::proxy_core::api::usage::{
    normalize_pricing_source, validate_cost_multiplier_value, CostMultiplierValidationError,
    PricingSourceValidationError, PRICING_SOURCE_REQUEST, PRICING_SOURCE_RESPONSE,
};
pub(crate) use crate::proxy_core::api::auth::{
    managed_account_auth_plan, ManagedAccountAuthPlan, ManagedAccountAuthRuntime,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::auth::ManagedAccountAuthError;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::{canonical_json_string, short_value_hash};

pub(crate) const SESSION_REQUEST_ID_PREFIX: &str =
    crate::proxy_core::api::usage::SESSION_REQUEST_ID_PREFIX;
const PROXY_EVENTS_CONNECTED_EVENT: &str =
    crate::proxy_core::api::events::PROXY_EVENTS_CONNECTED_EVENT;
const PROXY_EVENTS_LAGGED_EVENT: &str =
    crate::proxy_core::api::events::PROXY_EVENTS_LAGGED_EVENT;
const PROXY_OFFICIAL_WARNING_EVENT: &str =
    crate::proxy_core::api::events::PROXY_OFFICIAL_WARNING_EVENT;
const PROVIDER_SWITCHED_EVENT: &str =
    crate::proxy_core::api::events::PROVIDER_SWITCHED_EVENT;
const PROVIDER_SWITCHED_SOURCE_FAILOVER: &str =
    crate::proxy_core::api::events::PROVIDER_SWITCHED_SOURCE_FAILOVER;
const PROVIDER_SWITCHED_SOURCE_FAILOVER_ENABLED: &str =
    crate::proxy_core::api::events::PROVIDER_SWITCHED_SOURCE_FAILOVER_ENABLED;
const REQUEST_STARTED_EVENT: &str =
    crate::proxy_core::api::events::REQUEST_STARTED_EVENT;
const SERVER_STARTED_EVENT: &str =
    crate::proxy_core::api::events::SERVER_STARTED_EVENT;
const SERVER_STOPPED_EVENT: &str =
    crate::proxy_core::api::events::SERVER_STOPPED_EVENT;
pub(crate) const AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE: &str =
    crate::proxy_core::api::routing::AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE;
pub(crate) const AUTO_FAILOVER_EMPTY_QUEUE_WITHOUT_CURRENT_PROVIDER_MESSAGE: &str =
    crate::proxy_core::api::routing::AUTO_FAILOVER_EMPTY_QUEUE_WITHOUT_CURRENT_PROVIDER_MESSAGE;

fn build_proxy_events_connected_payload(buffer_size: usize) -> Value {
    crate::proxy_core::api::events::build_proxy_events_connected_payload(buffer_size)
}

fn build_proxy_events_lagged_payload(skipped: u64) -> Value {
    crate::proxy_core::api::events::build_proxy_events_lagged_payload(skipped)
}

pub(crate) fn proxy_events_connected_message(buffer_size: usize) -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: PROXY_EVENTS_CONNECTED_EVENT.to_string(),
        payload: build_proxy_events_connected_payload(buffer_size),
    }
}

pub(crate) fn proxy_events_lagged_message(skipped: u64) -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: PROXY_EVENTS_LAGGED_EVENT.to_string(),
        payload: build_proxy_events_lagged_payload(skipped),
    }
}

fn build_server_started_event_payload(address: &str, port: u16) -> Value {
    crate::proxy_core::api::events::build_server_started_event_payload(address, port)
}

fn build_server_stopped_event_payload() -> Value {
    crate::proxy_core::api::events::build_server_stopped_event_payload()
}

pub(crate) fn server_started_event_message(address: &str, port: u16) -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: SERVER_STARTED_EVENT.to_string(),
        payload: build_server_started_event_payload(address, port),
    }
}

pub(crate) fn server_stopped_event_message() -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: SERVER_STOPPED_EVENT.to_string(),
        payload: build_server_stopped_event_payload(),
    }
}

pub(crate) fn emit_proxy_server_started_event_source(
    events: &ProxyEventBus,
    address: &str,
    port: u16,
) {
    let message = server_started_event_message(address, port);
    events.emit(message.event_name, message.payload);
}

pub(crate) fn emit_proxy_server_stopped_event_source(events: &ProxyEventBus) {
    let message = server_stopped_event_message();
    events.emit(message.event_name, message.payload);
}

pub(crate) fn provider_switched_event_message(
    app_type: &str,
    provider_id: &str,
    source: &str,
) -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: PROVIDER_SWITCHED_EVENT.to_string(),
        payload: build_provider_switched_event_payload(app_type, provider_id, source),
    }
}

pub(crate) fn provider_switched_failover_event_message(
    app_type: &str,
    provider_id: &str,
) -> ProxyEventBusMessage {
    provider_switched_event_message(app_type, provider_id, PROVIDER_SWITCHED_SOURCE_FAILOVER)
}

pub(crate) fn provider_switched_failover_enabled_event_message(
    app_type: &str,
    provider_id: &str,
) -> ProxyEventBusMessage {
    provider_switched_event_message(
        app_type,
        provider_id,
        PROVIDER_SWITCHED_SOURCE_FAILOVER_ENABLED,
    )
}

pub(crate) fn proxy_official_warning_event_message(
    app_type: &str,
    provider_name: &str,
) -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: PROXY_OFFICIAL_WARNING_EVENT.to_string(),
        payload: build_proxy_official_warning_event_payload(app_type, provider_name),
    }
}

pub(crate) struct ProxyEventBusMessage {
    pub(crate) event_name: String,
    pub(crate) payload: Value,
}

pub(crate) fn request_started_event_message(
    request_id: &str,
    app_type: &str,
) -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: REQUEST_STARTED_EVENT.to_string(),
        payload: build_request_started_event_payload(request_id, app_type),
    }
}

pub(crate) fn proxy_core_event_to_bus_message(event: ProxyCoreEvent) -> ProxyEventBusMessage {
    ProxyEventBusMessage {
        event_name: event.event_type.event_name(),
        payload: event.into_event_payload(),
    }
}

pub(crate) fn emit_proxy_core_event(
    event: ProxyCoreEvent,
    mut emit: impl FnMut(String, Value),
) {
    let message = proxy_core_event_to_bus_message(event);
    emit(message.event_name, message.payload);
}

pub(crate) fn emit_proxy_core_event_bus_source(
    events: &ProxyEventBus,
    event: ProxyCoreEvent,
) {
    emit_proxy_core_event(event, |event_name, payload| {
        events.emit(event_name, payload);
    });
}

pub(crate) fn proxy_engine_from_services<S>(services: Arc<S>) -> ProxyEngine<S>
where
    S: ProxyServices + ?Sized,
{
    ProxyEngine::new(services)
}

pub(crate) fn provider_codex_auth_headers(
    auth: &ProviderAuthInfo,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, String> {
    build_codex_bearer_auth_headers(&auth.api_key).map_err(|error| error.to_string())
}

pub(crate) use crate::proxy_core::api::transport::resolve_codex_provider_upstream_model;

pub(crate) fn provider_codex_api_key(provider: &Provider) -> Option<String> {
    if let Some(env) = provider.settings_config.get("env") {
        if let Some(key) = env
            .get("OPENAI_API_KEY")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            return Some(key.to_string());
        }
    }

    if let Some(auth) = provider.settings_config.get("auth") {
        if let Some(key) = crate::codex_config::extract_codex_auth_api_key(auth) {
            return Some(key.to_string());
        }
    }

    if let Some(key) = provider
        .settings_config
        .get("apiKey")
        .or_else(|| provider.settings_config.get("api_key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        return Some(key.to_string());
    }

    if let Some(config) = provider.settings_config.get("config") {
        if let Some(key) = config
            .get("api_key")
            .or_else(|| config.get("apiKey"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            return Some(key.to_string());
        }

        if let Some(config_str) = config.as_str() {
            if let Some(key) =
                crate::codex_config::extract_codex_experimental_bearer_token(config_str)
            {
                return Some(key);
            }
        }
    }

    None
}

pub(crate) fn provider_codex_auth_info(provider: &Provider) -> Option<ProviderAuthInfo> {
    provider_codex_api_key(provider)
        .map(|key| ProviderAuthInfo::new(key, ProviderAuthStrategy::Bearer))
}

pub(crate) fn codex_auth_object_value_from_settings(settings: &Value) -> Option<&Value> {
    let auth = settings.get("auth")?;
    auth.as_object()?;
    Some(auth)
}

pub(crate) fn codex_api_key_from_auth_and_config(
    auth: Option<&Value>,
    config_text: Option<&str>,
) -> Option<String> {
    crate::codex_config::extract_codex_api_key(auth, config_text)
}

pub(crate) fn provider_codex_base_url(provider: &Provider) -> Option<String> {
    if let Some(url) = provider
        .settings_config
        .get("base_url")
        .and_then(Value::as_str)
    {
        return Some(url.trim_end_matches('/').to_string());
    }

    if let Some(url) = provider
        .settings_config
        .get("baseURL")
        .and_then(Value::as_str)
    {
        return Some(url.trim_end_matches('/').to_string());
    }

    if let Some(config) = provider.settings_config.get("config") {
        if let Some(url) = config.get("base_url").and_then(Value::as_str) {
            return Some(url.trim_end_matches('/').to_string());
        }

        if let Some(config_str) = config.as_str() {
            if let Some(start) = config_str.find("base_url = \"") {
                let rest = &config_str[start + 12..];
                if let Some(end) = rest.find('"') {
                    return Some(rest[..end].trim_end_matches('/').to_string());
                }
            }
            if let Some(start) = config_str.find("base_url = '") {
                let rest = &config_str[start + 12..];
                if let Some(end) = rest.find('\'') {
                    return Some(rest[..end].trim_end_matches('/').to_string());
                }
            }
        }
    }

    None
}

fn missing_provider_base_url_message(provider_name: &str) -> String {
    format!("{provider_name} Provider 缺少 base_url 配置")
}

pub(crate) fn required_codex_provider_base_url(provider: &Provider) -> Result<String, String> {
    provider_codex_base_url(provider).ok_or_else(|| missing_provider_base_url_message("Codex"))
}

pub(crate) fn required_gemini_provider_base_url(provider: &Provider) -> Result<String, String> {
    provider_gemini_base_url(provider).ok_or_else(|| missing_provider_base_url_message("Gemini"))
}

pub(crate) fn required_claude_provider_base_url(provider: &Provider) -> Result<String, String> {
    provider_claude_base_url(provider).ok_or_else(|| missing_provider_base_url_message("Claude"))
}

pub(crate) fn codex_config_text_from_settings(settings: &Value) -> Option<&str> {
    settings.get("config").and_then(Value::as_str)
}

fn provider_codex_config_text(provider: &Provider) -> Option<&str> {
    codex_config_text_from_settings(&provider.settings_config)
}

pub(crate) fn provider_codex_imported_live_category(provider: &Provider) -> &'static str {
    let config_text = provider_codex_config_text(provider);
    let auth = provider.settings_config.get("auth");
    let has_provider_key = crate::codex_config::extract_codex_api_key(auth, config_text).is_some();
    let has_login_material = auth.is_some_and(crate::codex_config::codex_auth_has_login_material);

    if has_login_material && !has_provider_key {
        "official"
    } else {
        "custom"
    }
}

pub(crate) fn provider_from_default_live_settings(
    app_type: &AppType,
    settings_config: Value,
) -> Provider {
    let mut provider = Provider::with_id(
        "default".to_string(),
        "default".to_string(),
        settings_config,
        None,
    );
    provider.category = Some(
        if matches!(app_type, AppType::Codex) {
            provider_codex_imported_live_category(&provider)
        } else {
            "custom"
        }
        .to_string(),
    );

    provider
}

pub(crate) struct CodexLiveSettingsParts<'a> {
    pub(crate) category: Option<&'a str>,
    pub(crate) auth: &'a Value,
    pub(crate) config_text: Option<&'a str>,
}

pub(crate) struct CodexProviderLiveWriteParts<'a> {
    pub(crate) category: Option<&'a str>,
    pub(crate) auth: &'a Value,
    pub(crate) config_text: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexProviderLiveWriteIssue {
    MissingAuth,
}

pub(crate) fn codex_provider_live_write_parts<'a>(
    settings: &'a Value,
    provider: &'a Provider,
) -> Result<CodexProviderLiveWriteParts<'a>, CodexProviderLiveWriteIssue> {
    let auth = settings
        .get("auth")
        .ok_or(CodexProviderLiveWriteIssue::MissingAuth)?;

    Ok(CodexProviderLiveWriteParts {
        category: provider.category.as_deref(),
        auth,
        config_text: codex_config_text_from_settings(settings),
    })
}

pub(crate) struct CodexRestoredLiveSettingsParts<'a> {
    pub(crate) auth: Option<&'a Value>,
    pub(crate) config: Option<&'a Value>,
}

pub(crate) struct CodexProviderBackfillParts<'a> {
    pub(crate) template_settings: &'a Value,
    pub(crate) restore_provider_token: bool,
    pub(crate) strip_unified_session_bucket: bool,
}

pub(crate) fn provider_codex_backfill_parts(provider: &Provider) -> CodexProviderBackfillParts<'_> {
    CodexProviderBackfillParts {
        template_settings: &provider.settings_config,
        restore_provider_token: crate::codex_config::should_restore_codex_provider_token_for_backfill(
            provider.category.as_deref(),
            &provider.settings_config,
        ),
        strip_unified_session_bucket: provider.category.as_deref() == Some("official"),
    }
}

pub(crate) fn restore_codex_settings_for_provider_backfill(
    provider: &Provider,
    settings: &mut Value,
) -> Result<(), AppError> {
    let backfill_parts = provider_codex_backfill_parts(provider);
    crate::codex_config::restore_codex_settings_for_backfill(
        settings,
        backfill_parts.template_settings,
        backfill_parts.restore_provider_token,
    )
}

pub(crate) fn strip_codex_unified_session_bucket_for_provider_backfill(
    provider: &Provider,
    settings: &mut Value,
) -> Result<(), AppError> {
    let backfill_parts = provider_codex_backfill_parts(provider);
    if backfill_parts.strip_unified_session_bucket {
        crate::codex_config::strip_codex_unified_session_bucket_from_settings(settings)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderBackfillSettingsWarning {
    CommonConfigStrip(CommonConfigSettingsMutationIssue),
    CodexSettingsRestore(String),
    CodexUnifiedSessionBucketStrip(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProviderBackfillSettingsResult {
    pub(crate) settings: Value,
    pub(crate) warnings: Vec<ProviderBackfillSettingsWarning>,
}

pub(crate) fn restore_live_settings_for_provider_backfill(
    app_type: &AppType,
    provider: &Provider,
    live_settings: Value,
) -> ProviderBackfillSettingsResult {
    if !matches!(app_type, AppType::Codex) {
        return ProviderBackfillSettingsResult {
            settings: live_settings,
            warnings: Vec::new(),
        };
    }

    let mut settings = live_settings;
    let mut warnings = Vec::new();
    if let Err(err) = restore_codex_settings_for_provider_backfill(provider, &mut settings) {
        warnings.push(ProviderBackfillSettingsWarning::CodexSettingsRestore(
            err.to_string(),
        ));
    }

    if let Err(err) =
        strip_codex_unified_session_bucket_for_provider_backfill(provider, &mut settings)
    {
        warnings.push(ProviderBackfillSettingsWarning::CodexUnifiedSessionBucketStrip(
            err.to_string(),
        ));
    }

    // `modelCatalog` is a cc-switch-private field whose SSOT is the DB. Live's
    // `config.toml` only carries a lossy projection (`model_catalog_json` to a
    // generated catalog file) that proxy takeover/restore cycles and Codex.app
    // config rewrites can drop. Prefer the DB provider's stored catalog so a
    // switch-away backfill never erases it.
    settings = codex_live_settings_with_model_catalog(
        settings,
        provider_model_catalog_raw_value(provider).cloned(),
    );

    ProviderBackfillSettingsResult { settings, warnings }
}

pub(crate) fn apply_codex_unified_session_bucket_for_provider(
    provider: &Provider,
    settings: &mut Value,
) -> Result<(), AppError> {
    crate::codex_config::apply_codex_unified_session_bucket_to_settings(
        provider.category.as_deref(),
        settings,
    )
}

pub(crate) fn codex_restored_live_settings_parts(
    settings: &Value,
) -> CodexRestoredLiveSettingsParts<'_> {
    CodexRestoredLiveSettingsParts {
        auth: settings.get("auth"),
        config: settings.get("config"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexLiveSettingsIssue {
    NotObject,
    MissingAuth,
    AuthNotObject,
}

pub(crate) fn provider_codex_live_settings_parts(
    provider: &Provider,
) -> Result<CodexLiveSettingsParts<'_>, CodexLiveSettingsIssue> {
    let settings = provider
        .settings_config
        .as_object()
        .ok_or(CodexLiveSettingsIssue::NotObject)?;
    let auth = settings
        .get("auth")
        .ok_or(CodexLiveSettingsIssue::MissingAuth)?;

    if !auth.is_object() {
        return Err(CodexLiveSettingsIssue::AuthNotObject);
    }

    Ok(CodexLiveSettingsParts {
        category: provider.category.as_deref(),
        auth,
        config_text: codex_config_text_from_settings(&provider.settings_config),
    })
}

pub(crate) struct CodexLiveSnapshotParts<'a> {
    pub(crate) category: Option<&'a str>,
    pub(crate) auth: &'a Value,
    pub(crate) config_text: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexLiveSnapshotIssue {
    NotObject,
    MissingAuth,
}

pub(crate) fn provider_codex_live_snapshot_parts(
    provider: &Provider,
) -> Result<CodexLiveSnapshotParts<'_>, CodexLiveSnapshotIssue> {
    let settings = provider
        .settings_config
        .as_object()
        .ok_or(CodexLiveSnapshotIssue::NotObject)?;
    let auth = settings
        .get("auth")
        .ok_or(CodexLiveSnapshotIssue::MissingAuth)?;

    Ok(CodexLiveSnapshotParts {
        category: provider.category.as_deref(),
        auth,
        config_text: codex_config_text_from_settings(&provider.settings_config),
    })
}

pub(crate) struct CodexProviderValidationParts<'a> {
    pub(crate) config_text: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexProviderValidationIssue {
    NotObject,
    MissingAuth,
    AuthNotObject,
    ConfigInvalidType,
}

pub(crate) fn provider_codex_validation_parts(
    provider: &Provider,
) -> Result<CodexProviderValidationParts<'_>, CodexProviderValidationIssue> {
    let settings = provider
        .settings_config
        .as_object()
        .ok_or(CodexProviderValidationIssue::NotObject)?;
    let auth = settings
        .get("auth")
        .ok_or(CodexProviderValidationIssue::MissingAuth)?;

    if !auth.is_object() {
        return Err(CodexProviderValidationIssue::AuthNotObject);
    }

    let config_text = match settings.get("config") {
        Some(config_value) if !(config_value.is_string() || config_value.is_null()) => {
            return Err(CodexProviderValidationIssue::ConfigInvalidType);
        }
        Some(_) => codex_config_text_from_settings(&provider.settings_config),
        None => None,
    };

    Ok(CodexProviderValidationParts { config_text })
}

#[derive(Default)]
pub(crate) struct ProviderSettingsValidationParts<'a> {
    pub(crate) codex_config_text: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderSettingsValidationIssue {
    ClaudeSettingsNotObject,
    Codex(CodexProviderValidationIssue),
    OpenCodeSettingsNotObject,
    OpenClawSettingsNotObject,
    HermesSettingsNotObject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalizedErrorSpec {
    pub(crate) key: &'static str,
    pub(crate) zh: String,
    pub(crate) en: String,
}

impl LocalizedErrorSpec {
    fn new(key: &'static str, zh: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            key,
            zh: zh.into(),
            en: en.into(),
        }
    }
}

pub(crate) fn provider_settings_validation_issue_spec(
    issue: ProviderSettingsValidationIssue,
    provider_id: &str,
) -> LocalizedErrorSpec {
    match issue {
        ProviderSettingsValidationIssue::ClaudeSettingsNotObject => LocalizedErrorSpec::new(
            "provider.claude.settings.not_object",
            "Claude 配置必须是 JSON 对象",
            "Claude configuration must be a JSON object",
        ),
        ProviderSettingsValidationIssue::Codex(issue) => match issue {
            CodexProviderValidationIssue::NotObject => LocalizedErrorSpec::new(
                "provider.codex.settings.not_object",
                "Codex 配置必须是 JSON 对象",
                "Codex configuration must be a JSON object",
            ),
            CodexProviderValidationIssue::MissingAuth => LocalizedErrorSpec::new(
                "provider.codex.auth.missing",
                format!("供应商 {provider_id} 缺少 auth 配置"),
                format!("Provider {provider_id} is missing auth configuration"),
            ),
            CodexProviderValidationIssue::AuthNotObject => LocalizedErrorSpec::new(
                "provider.codex.auth.not_object",
                format!("供应商 {provider_id} 的 auth 配置必须是 JSON 对象"),
                format!("Provider {provider_id} auth configuration must be a JSON object"),
            ),
            CodexProviderValidationIssue::ConfigInvalidType => LocalizedErrorSpec::new(
                "provider.codex.config.invalid_type",
                "Codex config 字段必须是字符串",
                "Codex config field must be a string",
            ),
        },
        ProviderSettingsValidationIssue::OpenCodeSettingsNotObject => LocalizedErrorSpec::new(
            "provider.opencode.settings.not_object",
            "OpenCode 配置必须是 JSON 对象",
            "OpenCode configuration must be a JSON object",
        ),
        ProviderSettingsValidationIssue::OpenClawSettingsNotObject => LocalizedErrorSpec::new(
            "provider.openclaw.settings.not_object",
            "OpenClaw 配置必须是 JSON 对象",
            "OpenClaw configuration must be a JSON object",
        ),
        ProviderSettingsValidationIssue::HermesSettingsNotObject => LocalizedErrorSpec::new(
            "provider.hermes.settings.not_object",
            "Hermes 配置必须是 JSON 对象",
            "Hermes configuration must be a JSON object",
        ),
    }
}

pub(crate) fn provider_settings_validation_parts<'a>(
    app_type: &AppType,
    provider: &'a Provider,
) -> Result<ProviderSettingsValidationParts<'a>, ProviderSettingsValidationIssue> {
    match app_type {
        AppType::Claude => {
            if !provider_settings_config_is_object(provider) {
                return Err(ProviderSettingsValidationIssue::ClaudeSettingsNotObject);
            }
        }
        AppType::Codex => {
            let parts = provider_codex_validation_parts(provider)
                .map_err(ProviderSettingsValidationIssue::Codex)?;
            return Ok(ProviderSettingsValidationParts {
                codex_config_text: parts.config_text,
            });
        }
        AppType::OpenCode => {
            if !provider_settings_config_is_object(provider) {
                return Err(ProviderSettingsValidationIssue::OpenCodeSettingsNotObject);
            }
        }
        AppType::OpenClaw => {
            if !provider_settings_config_is_object(provider) {
                return Err(ProviderSettingsValidationIssue::OpenClawSettingsNotObject);
            }
        }
        AppType::Hermes => {
            if !provider_settings_config_is_object(provider) {
                return Err(ProviderSettingsValidationIssue::HermesSettingsNotObject);
            }
        }
        AppType::ClaudeDesktop | AppType::Gemini => {}
    }

    Ok(ProviderSettingsValidationParts::default())
}

fn codex_wire_api_from_toml(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<toml::Value>().ok()?;

    if let Some(active_provider) = doc.get("model_provider").and_then(|value| value.as_str()) {
        if let Some(wire_api) = doc
            .get("model_providers")
            .and_then(|providers| providers.get(active_provider))
            .and_then(|provider| provider.get("wire_api"))
            .and_then(|value| value.as_str())
        {
            return Some(wire_api.to_string());
        }
    }

    doc.get("wire_api")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn codex_model_from_toml(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<toml::Value>().ok()?;

    doc.get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodexBackupProjectionIssue {
    InvalidTargetSettings,
    ParseTargetConfig(String),
    ParseExistingConfig(String),
    PrepareLiveConfig(String),
}

pub(crate) fn codex_backup_projection_error_message(issue: CodexBackupProjectionIssue) -> String {
    match issue {
        CodexBackupProjectionIssue::InvalidTargetSettings => {
            "Codex 备份必须是 JSON 对象".to_string()
        }
        CodexBackupProjectionIssue::ParseTargetConfig(message) => {
            format!("解析新的 Codex config.toml 失败: {message}")
        }
        CodexBackupProjectionIssue::ParseExistingConfig(message) => {
            format!("解析现有 Codex 备份失败: {message}")
        }
        CodexBackupProjectionIssue::PrepareLiveConfig(message) => {
            format!("更新 Codex 备份配置失败: {message}")
        }
    }
}

pub(crate) fn preserve_codex_mcp_servers_from_existing_config(
    target_settings: &mut Value,
    existing_config: &Value,
) -> Result<(), CodexBackupProjectionIssue> {
    let target_obj = target_settings
        .as_object_mut()
        .ok_or(CodexBackupProjectionIssue::InvalidTargetSettings)?;

    let target_config = target_obj
        .get("config")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut target_doc = if target_config.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        target_config
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| CodexBackupProjectionIssue::ParseTargetConfig(e.to_string()))?
    };

    let existing_config = existing_config
        .get("config")
        .and_then(Value::as_str)
        .unwrap_or("");
    if existing_config.trim().is_empty() {
        target_obj.insert("config".to_string(), json!(target_doc.to_string()));
        return Ok(());
    }

    let existing_doc = existing_config
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| CodexBackupProjectionIssue::ParseExistingConfig(e.to_string()))?;

    if let Some(existing_mcp_servers) = existing_doc.get("mcp_servers") {
        match target_doc.get_mut("mcp_servers") {
            Some(target_mcp_servers) => {
                if let (Some(target_table), Some(existing_table)) = (
                    target_mcp_servers.as_table_like_mut(),
                    existing_mcp_servers.as_table_like(),
                ) {
                    for (server_id, server_item) in existing_table.iter() {
                        if target_table.get(server_id).is_none() {
                            target_table.insert(server_id, server_item.clone());
                        }
                    }
                } else {
                    log::warn!(
                        "Codex config contains a non-table mcp_servers section; skipping MCP merge"
                    );
                }
            }
            None => {
                target_doc["mcp_servers"] = existing_mcp_servers.clone();
            }
        }
    }

    target_obj.insert("config".to_string(), json!(target_doc.to_string()));
    Ok(())
}

pub(crate) fn preserve_codex_oauth_auth_in_backup_if_present(
    target_settings: &mut Value,
    existing_backup: &Value,
) -> Result<(), CodexBackupProjectionIssue> {
    let Some(existing_auth) = existing_backup
        .get("auth")
        .filter(|auth| crate::codex_config::codex_auth_has_oauth_login_material(auth))
        .cloned()
    else {
        return Ok(());
    };

    let Some(target_obj) = target_settings.as_object_mut() else {
        return Ok(());
    };

    let provider_auth = target_obj.get("auth").cloned().unwrap_or_else(|| json!({}));
    if let Some(config_text) = target_obj.get("config").and_then(Value::as_str) {
        let live_config =
            crate::codex_config::prepare_codex_provider_live_config(&provider_auth, config_text)
                .map_err(|e| CodexBackupProjectionIssue::PrepareLiveConfig(e.to_string()))?;
        target_obj.insert("config".to_string(), json!(live_config));
    }
    target_obj.insert("auth".to_string(), existing_auth);

    Ok(())
}

pub(crate) fn preserve_codex_oauth_auth_in_backup_for_configured_policy(
    target_settings: &mut Value,
    existing_backup: &Value,
) -> Result<(), CodexBackupProjectionIssue> {
    if !crate::settings::preserve_codex_official_auth_on_switch() {
        return Ok(());
    }

    preserve_codex_oauth_auth_in_backup_if_present(target_settings, existing_backup)
}

pub(crate) fn provider_codex_uses_chat_completions(provider: &Provider) -> bool {
    let config_text = provider_codex_config_text(provider);
    resolve_codex_provider_uses_chat_completions(
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.api_format.as_deref())
            .or_else(|| {
                provider
                    .settings_config
                    .get("api_format")
                    .and_then(Value::as_str)
            })
            .or_else(|| {
                provider
                    .settings_config
                    .get("apiFormat")
                    .and_then(Value::as_str)
            }),
        config_text.and_then(codex_wire_api_from_toml).as_deref(),
        provider
            .settings_config
            .get("base_url")
            .or_else(|| provider.settings_config.get("baseURL"))
            .and_then(Value::as_str),
        config_text
            .and_then(crate::codex_config::extract_codex_base_url)
            .as_deref(),
    )
}

pub(crate) fn provider_should_convert_codex_responses_to_chat(
    provider: &Provider,
    endpoint: &str,
) -> bool {
    should_convert_codex_responses_endpoint_to_chat(
        provider_codex_uses_chat_completions(provider),
        endpoint,
    )
}

pub(crate) fn provider_codex_upstream_model(provider: &Provider) -> Option<String> {
    let settings_model = provider
        .settings_config
        .get("model")
        .and_then(Value::as_str);
    let config_model = provider_codex_config_text(provider).and_then(codex_model_from_toml);
    resolve_codex_provider_upstream_model(settings_model, config_model.as_deref())
}

pub(crate) fn codex_takeover_toml_config_for_provider(
    toml_str: &str,
    proxy_url: &str,
    provider: Option<&Provider>,
) -> String {
    let updated = crate::codex_config::update_codex_toml_field(toml_str, "base_url", proxy_url)
        .unwrap_or_else(|_| toml_str.to_string());
    let mut updated =
        crate::codex_config::update_codex_toml_field(&updated, "wire_api", "responses")
            .unwrap_or(updated);

    if let Some(upstream_model) = provider.and_then(provider_codex_upstream_model) {
        updated = crate::codex_config::update_codex_toml_field(&updated, "model", &upstream_model)
            .unwrap_or(updated);
    }

    updated
}

pub(crate) fn provider_codex_catalog_model_ids(
    provider: &Provider,
) -> std::collections::HashSet<String> {
    codex_provider_catalog_model_ids_from_settings(&provider.settings_config)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexTakeoverAuthPolicy {
    ExistingAuthOnly,
    EnsureAuth,
}

pub(crate) fn apply_codex_takeover_fields_for_provider(
    config: &mut Value,
    proxy_url: &str,
    placeholder: &str,
    provider: Option<&Provider>,
    auth_policy: CodexTakeoverAuthPolicy,
) {
    match auth_policy {
        CodexTakeoverAuthPolicy::ExistingAuthOnly => {
            apply_codex_takeover_auth_placeholder_if_present(config, placeholder);
        }
        CodexTakeoverAuthPolicy::EnsureAuth => {
            ensure_codex_takeover_auth_placeholder(config, placeholder);
        }
    }

    let config_str = config.get("config").and_then(Value::as_str).unwrap_or("");
    let updated_config = codex_takeover_toml_config_for_provider(config_str, proxy_url, provider);
    config["config"] = json!(updated_config);
    attach_codex_model_catalog_from_provider(config, provider);
}

fn codex_chat_reasoning_profile_from_config(
    config: crate::provider::CodexChatReasoningConfig,
) -> CodexChatReasoningProfile {
    CodexChatReasoningProfile {
        supports_thinking: config.supports_thinking,
        supports_effort: config.supports_effort,
        thinking_param: config.thinking_param,
        effort_param: config.effort_param,
        effort_value_mode: config.effort_value_mode,
        output_format: config.output_format,
    }
}

pub(crate) fn provider_codex_chat_reasoning_profile(
    provider: &Provider,
    request_model: Option<&str>,
) -> Option<CodexChatReasoningProfile> {
    if let Some(config) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.codex_chat_reasoning.clone())
    {
        return Some(normalize_codex_chat_reasoning_profile(
            codex_chat_reasoning_profile_from_config(config),
        ));
    }

    let model = request_model
        .map(ToString::to_string)
        .or_else(|| provider_codex_upstream_model(provider))
        .unwrap_or_default();
    let base_url = provider
        .settings_config
        .get("base_url")
        .or_else(|| provider.settings_config.get("baseURL"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            provider_codex_config_text(provider)
                .and_then(crate::codex_config::extract_codex_base_url)
        })
        .unwrap_or_default();

    infer_codex_chat_reasoning_profile(&provider.name, &base_url, &model)
}

pub(crate) use crate::proxy_core::api::transport::{
    apply_codex_chat_upstream_model_policy, codex_provider_catalog_model_ids_from_settings,
};

pub(crate) fn provider_apply_codex_chat_upstream_model(
    provider: &Provider,
    body: &mut Value,
) -> Option<String> {
    if !provider_codex_uses_chat_completions(provider) {
        return None;
    }

    let catalog_model_ids = provider_codex_catalog_model_ids(provider);
    let upstream_model = provider_codex_upstream_model(provider);
    apply_codex_chat_upstream_model_policy(
        body,
        true,
        upstream_model.as_deref(),
        &catalog_model_ids,
    )
}

pub(crate) fn provider_codex_chat_reasoning_options(
    provider: &Provider,
    body: &Value,
) -> Option<CodexChatReasoningOptions> {
    provider_codex_chat_reasoning_profile(
        provider,
        body.get("model").and_then(|value| value.as_str()),
    )
    .map(|profile| CodexChatReasoningOptions::from_profile(&profile))
}

pub(crate) use crate::proxy_core::api::transforms::{
    infer_codex_chat_reasoning_profile, normalize_codex_chat_reasoning_profile,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwarderRuntimeOptions {
    pub(crate) non_streaming_timeout: u64,
    pub(crate) streaming_first_byte_timeout: u64,
    pub(crate) streaming_idle_timeout: u64,
    pub(crate) max_retries: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwarderRuntimeConfig {
    pub(crate) options: ForwarderRuntimeOptions,
    pub(crate) rectifier: RectifierConfig,
    pub(crate) optimizer: OptimizerConfig,
    pub(crate) copilot_optimizer: CopilotOptimizerConfig,
}

pub(crate) use crate::proxy_core::api::transport::resolve_response_runtime_policy;

pub(crate) fn response_runtime_policy_from_app_proxy_config(
    config: &AppProxyConfig,
) -> ResponseRuntimePolicy {
    resolve_response_runtime_policy(
        config.auto_failover_enabled,
        config.max_retries,
        config.non_streaming_timeout as u64,
        config.streaming_first_byte_timeout as u64,
        config.streaming_idle_timeout as u64,
    )
}

pub(crate) fn forwarder_runtime_options_from_app_proxy_config(
    config: &AppProxyConfig,
) -> ForwarderRuntimeOptions {
    let policy = response_runtime_policy_from_app_proxy_config(config);
    ForwarderRuntimeOptions {
        non_streaming_timeout: policy.timeout.non_streaming_timeout,
        streaming_first_byte_timeout: policy.timeout.streaming.first_byte_timeout,
        streaming_idle_timeout: policy.timeout.streaming.idle_timeout,
        max_retries: policy.max_retries,
    }
}

pub(crate) fn forwarder_runtime_config_from_sources(
    app_config: &AppProxyConfig,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ForwarderRuntimeConfig {
    ForwarderRuntimeConfig {
        options: forwarder_runtime_options_from_app_proxy_config(app_config),
        rectifier,
        optimizer,
        copilot_optimizer,
    }
}

pub(crate) async fn forwarder_runtime_config_from_db_sources(
    db: &Database,
    app_type: &AppType,
) -> ProxyCoreResult<ForwarderRuntimeConfig> {
    let app_config = db
        .get_proxy_config_for_app(app_type.as_str())
        .await
        .map_err(|error| app_error("load app proxy config", error))?;

    Ok(forwarder_runtime_config_from_sources(
        &app_config,
        db.get_rectifier_config().unwrap_or_default(),
        db.get_optimizer_config().unwrap_or_default(),
        db.get_copilot_optimizer_config().unwrap_or_default(),
    ))
}

pub(crate) use crate::proxy_core::api::auth::extract_gemini_api_key_from_settings;

pub(crate) fn provider_gemini_api_key(provider: &Provider) -> Option<String> {
    extract_gemini_api_key_from_settings(&provider.settings_config)
}

pub(crate) use crate::proxy_core::api::auth::extract_gemini_base_url_from_settings;

pub(crate) fn provider_gemini_base_url(provider: &Provider) -> Option<String> {
    extract_gemini_base_url_from_settings(&provider.settings_config)
}

pub(crate) fn gemini_env_map_from_settings(settings: &Value) -> Option<&Map<String, Value>> {
    settings.get("env").and_then(Value::as_object)
}

pub(crate) fn gemini_env_value_from_env_json(env_json: &Value) -> Value {
    env_json.get("env").cloned().unwrap_or_else(|| json!({}))
}

pub(crate) fn gemini_live_settings_from_env_json_and_config(
    env_json: &Value,
    config: Value,
) -> Value {
    json!({
        "env": gemini_env_value_from_env_json(env_json),
        "config": config
    })
}

pub(crate) fn gemini_live_backup_from_effective_settings(settings: &Value) -> Value {
    json!({
        "env": settings.get("env").cloned().unwrap_or_else(|| json!({}))
    })
}

pub(crate) fn provider_gemini_env_map(
    provider: &Provider,
) -> Result<HashMap<String, String>, AppError> {
    crate::gemini_config::json_to_env(&provider.settings_config)
}

pub(crate) fn validate_provider_gemini_settings_strict(
    provider: &Provider,
) -> Result<(), AppError> {
    crate::gemini_config::validate_gemini_settings_strict(&provider.settings_config)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeminiLiveConfigIssue {
    InvalidType,
}

pub(crate) fn provider_gemini_live_config_object(
    provider: &Provider,
) -> Result<Option<&Value>, GeminiLiveConfigIssue> {
    match provider.settings_config.get("config") {
        Some(config) if config.is_object() => Ok(Some(config)),
        Some(config) if config.is_null() => Ok(None),
        Some(_) => Err(GeminiLiveConfigIssue::InvalidType),
        None => Ok(None),
    }
}

pub(crate) fn gemini_live_settings_to_write(
    existing_settings: Option<Value>,
    provider_config: Option<&Value>,
) -> Option<Value> {
    match provider_config {
        Some(config_value) => {
            let mut merged = existing_settings.unwrap_or_else(|| json!({}));
            if let (Some(merged_obj), Some(config_obj)) =
                (merged.as_object_mut(), config_value.as_object())
            {
                for (key, value) in config_obj {
                    merged_obj.insert(key.clone(), value.clone());
                }
            }
            Some(merged)
        }
        None => existing_settings,
    }
}

pub(crate) fn provider_gemini_kind(provider: &Provider) -> ProviderKind {
    if provider_gemini_api_key(provider)
        .as_deref()
        .map(is_gemini_oauth_key_shape)
        .unwrap_or(false)
    {
        ProviderKind::GeminiCli
    } else {
        ProviderKind::Gemini
    }
}

pub(crate) fn provider_gemini_auth_strategy(provider: &Provider) -> ProviderAuthStrategy {
    match provider_gemini_kind(provider) {
        ProviderKind::GeminiCli => ProviderAuthStrategy::GoogleOAuth,
        _ => ProviderAuthStrategy::Google,
    }
}

pub(crate) fn provider_gemini_auth_info(provider: &Provider) -> Option<ProviderAuthInfo> {
    let key = provider_gemini_api_key(provider)?;

    match provider_gemini_auth_strategy(provider) {
        ProviderAuthStrategy::GoogleOAuth => {
            if let Some(credentials) = parse_gemini_oauth_credentials(&key) {
                Some(ProviderAuthInfo::with_access_token(
                    key,
                    credentials.access_token,
                ))
            } else {
                Some(ProviderAuthInfo::new(key, ProviderAuthStrategy::Google))
            }
        }
        _ => Some(ProviderAuthInfo::new(key, ProviderAuthStrategy::Google)),
    }
}

pub(crate) fn provider_gemini_auth_headers(
    auth: &ProviderAuthInfo,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, String> {
    build_gemini_auth_headers(
        &auth.api_key,
        auth.access_token.as_deref(),
        matches!(auth.strategy, ProviderAuthStrategy::GoogleOAuth),
    )
    .map_err(|error| error.to_string())
}

pub(crate) use crate::proxy_core::api::transforms::resolve_claude_api_format_from_settings;

pub(crate) fn provider_claude_api_format(provider: &Provider) -> &'static str {
    let meta = provider.meta.as_ref();
    resolve_claude_api_format_from_settings(
        meta.and_then(|meta| meta.provider_type.as_deref()),
        meta.and_then(|meta| meta.api_format.as_deref()),
        &provider.settings_config,
    )
}

pub(crate) fn provider_needs_claude_transform(provider: &Provider) -> bool {
    if matches!(
        provider_claude_kind(provider),
        ProviderKind::GitHubCopilot | ProviderKind::CodexOAuth
    ) {
        return true;
    }

    claude_api_format_needs_transform(provider_claude_api_format(provider))
}

pub(crate) use crate::proxy_core::api::domain::infer_claude_provider_kind;

pub(crate) fn provider_claude_kind(provider: &Provider) -> ProviderKind {
    let api_format = provider_claude_api_format(provider);
    let uses_google_oauth = provider_claude_auth_key(provider)
        .map(|auth_key| is_gemini_oauth_key_shape(&auth_key.key))
        .unwrap_or(false);
    let meta_provider_type = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref());
    let base_url = provider_claude_base_url(provider);

    infer_claude_provider_kind(
        api_format,
        uses_google_oauth,
        meta_provider_type,
        base_url.as_deref(),
        &provider.settings_config,
    )
}

pub(crate) fn provider_kind_from_app_type_and_config(
    app_type: &AppType,
    provider: &Provider,
) -> ProviderKind {
    match app_type {
        AppType::Claude | AppType::ClaudeDesktop => provider_claude_kind(provider),
        AppType::Codex => ProviderKind::Codex,
        AppType::Gemini => provider_gemini_kind(provider),
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => ProviderKind::Codex,
    }
}

pub(crate) use crate::proxy_core::api::transforms::is_copilot_prompt_cache_provider;

pub(crate) fn provider_is_copilot_prompt_cache_provider(provider: &Provider) -> bool {
    is_copilot_prompt_cache_provider(
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.provider_type.as_deref()),
        &provider.settings_config,
    )
}

pub(crate) fn provider_claude_prompt_cache_key(provider: &Provider) -> Option<&str> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.prompt_cache_key.as_deref())
}

pub(crate) use crate::proxy_core::api::transforms::resolve_claude_responses_prompt_cache_key;

pub(crate) fn provider_claude_responses_prompt_cache_key(
    provider: &Provider,
    body: &Value,
    session_id: Option<&str>,
) -> ClaudePromptCacheKeyResolution {
    resolve_claude_responses_prompt_cache_key(
        body,
        provider_claude_prompt_cache_key(provider),
        session_id,
        provider_is_copilot_prompt_cache_provider(provider),
    )
}

pub(crate) fn provider_codex_fast_mode_enabled(provider: &Provider) -> bool {
    provider.codex_fast_mode_enabled()
}

pub(crate) fn provider_settings_config_is_object(provider: &Provider) -> bool {
    provider.settings_config.is_object()
}

pub(crate) struct ClaudeEnvCredentials<'a> {
    pub(crate) api_key: Option<&'a str>,
    pub(crate) base_url: Option<&'a str>,
}

pub(crate) fn claude_env_credentials_from_settings(
    settings_config: &Value,
) -> Option<ClaudeEnvCredentials<'_>> {
    let env = settings_config.get("env").and_then(Value::as_object)?;
    Some(ClaudeEnvCredentials {
        api_key: env
            .get("ANTHROPIC_AUTH_TOKEN")
            .or_else(|| env.get("ANTHROPIC_API_KEY"))
            .and_then(Value::as_str),
        base_url: env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
    })
}

pub(crate) fn provider_claude_auth_key(provider: &Provider) -> Option<ClaudeAuthKey> {
    extract_claude_auth_key_from_settings(&provider.settings_config)
}

fn log_claude_auth_key_source(auth_key: Option<&ClaudeAuthKey>) {
    match auth_key.map(|auth_key| auth_key.source) {
        Some(ClaudeAuthKeySource::AnthropicAuthToken) => {
            log::debug!("[Claude] 使用 ANTHROPIC_AUTH_TOKEN");
        }
        Some(ClaudeAuthKeySource::AnthropicApiKey) => {
            log::debug!("[Claude] 使用 ANTHROPIC_API_KEY");
        }
        Some(ClaudeAuthKeySource::OpenRouterApiKey) => {
            log::debug!("[Claude] 使用 OPENROUTER_API_KEY");
        }
        Some(ClaudeAuthKeySource::OpenAiApiKey) => {
            log::debug!("[Claude] 使用 OPENAI_API_KEY");
        }
        Some(ClaudeAuthKeySource::GeminiApiKey) => {
            log::debug!("[Claude] 使用 GEMINI_API_KEY");
        }
        Some(ClaudeAuthKeySource::DirectApiKey) => {
            log::debug!("[Claude] 使用 apiKey/api_key");
        }
        None => {
            log::warn!("[Claude] 未找到有效的 API Key");
        }
    }
}

fn claude_anthropic_auth_strategy(source: ClaudeAuthKeySource) -> Option<ProviderAuthStrategy> {
    match source {
        ClaudeAuthKeySource::AnthropicAuthToken => Some(ProviderAuthStrategy::ClaudeAuth),
        ClaudeAuthKeySource::AnthropicApiKey => Some(ProviderAuthStrategy::Anthropic),
        _ => None,
    }
}

fn claude_gemini_cli_auth_info(provider: &Provider, key: String) -> ProviderAuthInfo {
    match parse_gemini_oauth_credentials(&key) {
        Some(credentials) if !credentials.access_token.is_empty() => {
            ProviderAuthInfo::with_access_token(key, credentials.access_token)
        }
        Some(_) => {
            log::warn!(
                "[Gemini OAuth] access_token missing or empty for provider `{}`; \
                 bearer auth will likely fail with 401. Refresh \
                 ~/.gemini/oauth_creds.json via the gemini CLI to obtain a new token.",
                provider.id
            );
            ProviderAuthInfo::new(key, ProviderAuthStrategy::GoogleOAuth)
        }
        None => ProviderAuthInfo::new(key, ProviderAuthStrategy::GoogleOAuth),
    }
}

pub(crate) fn provider_claude_auth_info(provider: &Provider) -> Option<ProviderAuthInfo> {
    let provider_type = provider_claude_kind(provider);

    if provider_type == ProviderKind::GitHubCopilot {
        return Some(ProviderAuthInfo::new(
            "copilot_placeholder".to_string(),
            ProviderAuthStrategy::GitHubCopilot,
        ));
    }

    if provider_type == ProviderKind::CodexOAuth {
        return Some(ProviderAuthInfo::new(
            "codex_oauth_placeholder".to_string(),
            ProviderAuthStrategy::CodexOAuth,
        ));
    }

    let auth_key = provider_claude_auth_key(provider);
    log_claude_auth_key_source(auth_key.as_ref());
    let auth_key = auth_key?;
    let key = auth_key.key;

    match provider_type {
        ProviderKind::GeminiCli => Some(claude_gemini_cli_auth_info(provider, key)),
        ProviderKind::Gemini => Some(ProviderAuthInfo::new(key, ProviderAuthStrategy::Google)),
        ProviderKind::OpenRouter => Some(ProviderAuthInfo::new(key, ProviderAuthStrategy::Bearer)),
        ProviderKind::ClaudeAuth => {
            Some(ProviderAuthInfo::new(key, ProviderAuthStrategy::ClaudeAuth))
        }
        _ => {
            let strategy = claude_anthropic_auth_strategy(auth_key.source)
                .unwrap_or(ProviderAuthStrategy::Anthropic);
            Some(ProviderAuthInfo::new(key, strategy))
        }
    }
}

pub(crate) fn channel_key_auth_error(channel_id: &str, key_ref: &str) -> ProxyCoreError {
    ProxyCoreError::Auth(channel_auth_profile_missing_key_error_message(
        channel_id, key_ref,
    ))
}

pub(crate) fn channel_key_value_from_record(key: Option<ProxyChannelKeyRecord>) -> Option<String> {
    key.map(|key| key.key_value)
}

pub(crate) fn provider_with_channel_auth_key(
    app_type: &AppType,
    provider: &Provider,
    key_value: &str,
) -> Provider {
    let mut auth_provider = provider.clone();
    auth_provider.settings_config = settings_config_with_channel_auth_key(
        app_type.as_str(),
        &provider.settings_config,
        key_value,
    );
    auth_provider
}

pub(crate) use crate::proxy_core::api::domain::extract_claude_base_url_from_settings;

pub(crate) fn provider_claude_base_url(provider: &Provider) -> Option<String> {
    extract_claude_base_url_from_settings(
        provider_is_codex_oauth(provider),
        &provider.settings_config,
    )
}

fn claude_auth_header_kind(strategy: ProviderAuthStrategy) -> Option<ClaudeAuthHeaderKind> {
    match strategy {
        ProviderAuthStrategy::Anthropic => Some(ClaudeAuthHeaderKind::AnthropicApiKey),
        ProviderAuthStrategy::ClaudeAuth | ProviderAuthStrategy::Bearer => {
            Some(ClaudeAuthHeaderKind::Bearer)
        }
        ProviderAuthStrategy::Google => Some(ClaudeAuthHeaderKind::GoogleApiKey),
        ProviderAuthStrategy::GoogleOAuth => Some(ClaudeAuthHeaderKind::GoogleOAuth),
        ProviderAuthStrategy::CodexOAuth => Some(ClaudeAuthHeaderKind::CodexOAuth),
        ProviderAuthStrategy::GitHubCopilot => None,
    }
}

pub(crate) fn provider_claude_auth_headers(
    auth: &ProviderAuthInfo,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, String> {
    if let Some(kind) = claude_auth_header_kind(auth.strategy) {
        return build_claude_auth_headers(kind, &auth.api_key, auth.access_token.as_deref())
            .map_err(|error| error.to_string());
    }

    match auth.strategy {
        ProviderAuthStrategy::GitHubCopilot => {
            let request_id = Uuid::new_v4().to_string();
            build_copilot_auth_headers(CopilotAuthHeadersInput {
                api_key: &auth.api_key,
                request_id: &request_id,
                editor_version: COPILOT_EDITOR_VERSION,
                editor_plugin_version: COPILOT_PLUGIN_VERSION,
                integration_id: COPILOT_INTEGRATION_ID,
                user_agent: COPILOT_USER_AGENT,
                github_api_version: COPILOT_API_VERSION,
            })
            .map_err(|error| error.to_string())
        }
        _ => unreachable!("static auth strategies are delegated to proxy-core"),
    }
}

pub(crate) fn provider_claude_transform_request_for_api_format(
    body: Value,
    provider: &Provider,
    api_format: &str,
    session_id: Option<&str>,
    shadow_store: Option<&GeminiShadowStore>,
) -> Result<Value, String> {
    let is_codex_oauth = provider_is_codex_oauth(provider);
    let cache_key_resolution =
        provider_claude_responses_prompt_cache_key(provider, &body, session_id);

    match api_format {
        "openai_responses" => {
            log::debug!(
                "[Cache] OpenAI Responses prompt_cache_key source={cache_key_source}, provider={}, codex_oauth={is_codex_oauth}, has_key={}",
                provider.id,
                cache_key_resolution.key.is_some(),
                cache_key_source = cache_key_resolution.source.as_str()
            );
            Ok(anthropic_to_openai_responses_request(
                &body,
                cache_key_resolution.key.as_deref(),
                is_codex_oauth,
                provider_codex_fast_mode_enabled(provider),
            ))
        }
        "openai_chat" => {
            let preserve_reasoning_content =
                provider_should_preserve_reasoning_content_for_openai_chat(provider, &body);
            let mut result = anthropic_to_openai_chat_request(&body, preserve_reasoning_content);
            if let Some(key) = provider_claude_prompt_cache_key(provider) {
                result["prompt_cache_key"] = serde_json::json!(key);
            }
            inject_openai_stream_include_usage(&mut result);
            Ok(result)
        }
        "gemini_native" => anthropic_request_to_gemini_request_with_shadow(
            &body,
            shadow_store,
            Some(&provider.id),
            session_id,
        ),
        _ => Ok(body),
    }
}

pub(crate) fn provider_claude_transform_response(body: Value) -> Result<Value, String> {
    // ProviderAdapter::transform_response does not receive provider config, so detect
    // structurally disjoint upstream response formats by their top-level fields.
    if body.get("candidates").is_some() || body.get("promptFeedback").is_some() {
        let output = gemini_response_to_anthropic_message(
            &body,
            None,
            synthesize_gemini_tool_call_id_with_uuid,
        )?;
        for name in &output.rectified_tool_names {
            log::info!("[Claude/Gemini] Rectified tool args for `{name}`");
        }
        Ok(output.response)
    } else if body.get("output").is_some() {
        openai_responses_to_anthropic_message(&body)
    } else {
        openai_chat_to_anthropic_message(&body)
    }
}

pub(crate) fn provider_should_preserve_reasoning_content_for_openai_chat(
    provider: &Provider,
    body: &Value,
) -> bool {
    should_preserve_reasoning_content_for_openai_chat(&provider.settings_config, body)
}

pub(crate) fn circuit_breaker_config_from_router_config_result(
    result: Result<AppProxyConfig, AppError>,
) -> CircuitBreakerConfig {
    let config = result.ok();
    circuit_breaker_config_from_app_config(config.as_ref())
}

pub(crate) fn circuit_failure_threshold_from_router_config_result(
    result: Result<AppProxyConfig, AppError>,
    fallback: u32,
) -> u32 {
    let config = result.ok();
    circuit_failure_threshold_from_app_config(config.as_ref(), fallback)
}

pub(crate) fn auto_failover_enabled_from_router_config_result(
    app_type: &str,
    result: Result<AppProxyConfig, AppError>,
) -> bool {
    match result {
        Ok(config) => config.auto_failover_enabled,
        Err(error) => {
            log::error!("[{app_type}] 读取 proxy_config 失败: {error}，默认禁用故障转移");
            false
        }
    }
}

pub(crate) fn select_current_provider_from_router_source(
    app_type: &str,
    current: Option<Provider>,
) -> Result<Vec<Provider>, AppError> {
    let selected_ids = select_provider_ids(ProviderSelectionInput::current(
        current.as_ref().map(|provider| provider.id.clone()),
    ))
    .map_err(|error| app_error_from_provider_selection_failure(app_type, error))?;

    Ok(selected_ids
        .into_iter()
        .filter_map(|provider_id| {
            current
                .as_ref()
                .filter(|provider| provider.id == provider_id)
                .cloned()
        })
        .collect())
}

pub(crate) fn select_failover_providers_from_router_lookup_availability<I>(
    app_type: &str,
    providers: &IndexMap<String, Provider>,
    lookup_availability: I,
) -> Result<Vec<Provider>, AppError>
where
    I: IntoIterator<Item = (ProviderFailoverCircuitLookup, bool)>,
{
    let candidates = lookup_availability
        .into_iter()
        .map(|(lookup, available)| {
            provider_selection_candidate_from_failover_lookup(lookup, available)
        })
        .collect();
    let selected_ids = select_provider_ids(ProviderSelectionInput::failover(candidates))
        .map_err(|error| app_error_from_provider_selection_failure(app_type, error))?;

    Ok(selected_ids
        .into_iter()
        .filter_map(|provider_id| providers.get(&provider_id).cloned())
        .collect())
}

pub(crate) fn provider_failover_circuit_lookups_from_router_sources(
    app_type: &str,
    queue: impl IntoIterator<Item = FailoverQueueItem>,
    providers: &IndexMap<String, Provider>,
) -> Vec<ProviderFailoverCircuitLookup> {
    provider_failover_circuit_lookups(
        app_type,
        queue.into_iter().map(|item| item.provider_id).collect::<Vec<_>>(),
        providers.keys().cloned().collect::<Vec<_>>(),
    )
}

pub(crate) fn current_provider_id_from_router_sources(
    app_type: &str,
    load_settings_current_provider_id: impl FnOnce(&AppType) -> Option<String>,
    load_db_current_provider_id: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let settings_current_provider_id = app_type
        .parse::<AppType>()
        .ok()
        .and_then(|app| load_settings_current_provider_id(&app));
    let db_current_provider_id =
        if current_provider_db_fallback_required(settings_current_provider_id.as_deref()) {
            load_db_current_provider_id()
        } else {
            None
        };
    current_provider_id_option_from_sources(
        settings_current_provider_id.as_deref(),
        db_current_provider_id.as_deref(),
    )
}

pub(crate) fn should_block_proxy_switch_to_provider(
    proxy_takeover_active: bool,
    provider: &Provider,
) -> bool {
    should_block_proxy_switch_to_provider_category(proxy_takeover_active, provider.category.as_deref())
}

pub(crate) fn proxy_live_config_owned_by_takeover(
    has_live_backup: bool,
    live_taken_over: bool,
) -> bool {
    has_live_backup || live_taken_over
}

pub(crate) fn proxy_switch_should_hot_switch(
    proxy_config_takeover_enabled: bool,
    live_taken_over: bool,
) -> bool {
    proxy_config_takeover_enabled || live_taken_over
}

pub(crate) fn proxy_hot_switch_should_refresh_codex_live_from_backup(
    app_type: &AppType,
    has_live_backup: bool,
    live_taken_over: bool,
) -> bool {
    matches!(app_type, AppType::Codex) && has_live_backup && !live_taken_over
}

pub(crate) fn proxy_hot_switch_should_sync_codex_live_while_proxy_active(
    app_type: &AppType,
    live_taken_over: bool,
) -> bool {
    matches!(app_type, AppType::Codex) && live_taken_over
}

pub(crate) fn proxy_hot_switch_should_sync_claude_live_while_proxy_active(
    app_type: &AppType,
    proxy_live_owned_by_takeover: bool,
) -> bool {
    matches!(app_type, AppType::Claude) && proxy_live_owned_by_takeover
}

pub(crate) fn sanitize_claude_settings_for_live(settings: &Value) -> Value {
    let mut sanitized = settings.clone();
    if let Some(obj) = sanitized.as_object_mut() {
        obj.remove("api_format");
        obj.remove("apiFormat");
        obj.remove("openrouter_compat_mode");
        obj.remove("openrouterCompatMode");
    }
    sanitized
}

pub(crate) fn json_value_is_subset(target: &Value, source: &Value) -> bool {
    match source {
        Value::Object(source_map) => {
            let Some(target_map) = target.as_object() else {
                return false;
            };
            source_map.iter().all(|(key, source_value)| {
                target_map
                    .get(key)
                    .is_some_and(|target_value| {
                        json_value_is_subset(target_value, source_value)
                    })
            })
        }
        Value::Array(source_arr) => {
            let Some(target_arr) = target.as_array() else {
                return false;
            };
            json_array_contains_subset(target_arr, source_arr)
        }
        _ => target == source,
    }
}

pub(crate) fn json_array_contains_subset(target_arr: &[Value], source_arr: &[Value]) -> bool {
    let mut matched = vec![false; target_arr.len()];

    source_arr.iter().all(|source_item| {
        if let Some((index, _)) = target_arr.iter().enumerate().find(|(index, target_item)| {
            !matched[*index] && json_value_is_subset(target_item, source_item)
        }) {
            matched[index] = true;
            true
        } else {
            false
        }
    })
}

pub(crate) fn json_remove_array_items(target_arr: &mut Vec<Value>, source_arr: &[Value]) {
    for source_item in source_arr {
        if let Some(index) = target_arr
            .iter()
            .position(|target_item| json_value_is_subset(target_item, source_item))
        {
            target_arr.remove(index);
        }
    }
}

pub(crate) fn json_deep_merge(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target_map), Value::Object(source_map)) => {
            for (key, source_value) in source_map {
                match target_map.get_mut(key) {
                    Some(target_value) => json_deep_merge(target_value, source_value),
                    None => {
                        target_map.insert(key.clone(), source_value.clone());
                    }
                }
            }
        }
        (target_value, source_value) => {
            *target_value = source_value.clone();
        }
    }
}

pub(crate) fn json_deep_remove(target: &mut Value, source: &Value) {
    let (Some(target_map), Some(source_map)) =
        (target.as_object_mut(), source.as_object())
    else {
        return;
    };

    for (key, source_value) in source_map {
        let mut remove_key = false;

        if let Some(target_value) = target_map.get_mut(key) {
            if source_value.is_object() && target_value.is_object() {
                json_deep_remove(target_value, source_value);
                remove_key = target_value.as_object().is_some_and(|obj| obj.is_empty());
            } else if let (Some(target_arr), Some(source_arr)) =
                (target_value.as_array_mut(), source_value.as_array())
            {
                json_remove_array_items(target_arr, source_arr);
                remove_key = target_arr.is_empty();
            } else if json_value_is_subset(target_value, source_value) {
                remove_key = true;
            }
        }

        if remove_key {
            target_map.remove(key);
        }
    }
}

pub(crate) fn toml_value_is_subset(
    target: &toml_edit::Value,
    source: &toml_edit::Value,
) -> bool {
    match (target, source) {
        (toml_edit::Value::String(target), toml_edit::Value::String(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Integer(target), toml_edit::Value::Integer(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Float(target), toml_edit::Value::Float(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Boolean(target), toml_edit::Value::Boolean(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Datetime(target), toml_edit::Value::Datetime(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Array(target), toml_edit::Value::Array(source)) => {
            toml_array_contains_subset(target, source)
        }
        (toml_edit::Value::InlineTable(target), toml_edit::Value::InlineTable(source)) => {
            source.iter().all(|(key, source_item)| {
                target
                    .get(key)
                    .is_some_and(|target_item| toml_value_is_subset(target_item, source_item))
            })
        }
        _ => false,
    }
}

pub(crate) fn toml_array_contains_subset(
    target: &toml_edit::Array,
    source: &toml_edit::Array,
) -> bool {
    let mut matched = vec![false; target.len()];
    let target_items: Vec<&toml_edit::Value> = target.iter().collect();

    source.iter().all(|source_item| {
        if let Some((index, _)) = target_items
            .iter()
            .enumerate()
            .find(|(index, target_item)| {
                !matched[*index] && toml_value_is_subset(target_item, source_item)
            })
        {
            matched[index] = true;
            true
        } else {
            false
        }
    })
}

pub(crate) fn toml_remove_array_items(
    target: &mut toml_edit::Array,
    source: &toml_edit::Array,
) {
    for source_item in source.iter() {
        let index = {
            let target_items: Vec<&toml_edit::Value> = target.iter().collect();
            target_items
                .iter()
                .enumerate()
                .find(|(_, target_item)| toml_value_is_subset(target_item, source_item))
                .map(|(index, _)| index)
        };

        if let Some(index) = index {
            target.remove(index);
        }
    }
}

pub(crate) fn toml_item_is_subset(target: &toml_edit::Item, source: &toml_edit::Item) -> bool {
    if let Some(source_table) = source.as_table_like() {
        let Some(target_table) = target.as_table_like() else {
            return false;
        };
        return source_table.iter().all(|(key, source_item)| {
            target_table
                .get(key)
                .is_some_and(|target_item| toml_item_is_subset(target_item, source_item))
        });
    }

    match (target.as_value(), source.as_value()) {
        (Some(target_value), Some(source_value)) => {
            toml_value_is_subset(target_value, source_value)
        }
        _ => false,
    }
}

fn merge_toml_item(target: &mut toml_edit::Item, source: &toml_edit::Item) {
    if let Some(source_table) = source.as_table_like() {
        if let Some(target_table) = target.as_table_like_mut() {
            merge_toml_table_like(target_table, source_table);
            return;
        }
    }

    *target = source.clone();
}

pub(crate) fn merge_toml_table_like(
    target: &mut dyn toml_edit::TableLike,
    source: &dyn toml_edit::TableLike,
) {
    for (key, source_item) in source.iter() {
        match target.get_mut(key) {
            Some(target_item) => merge_toml_item(target_item, source_item),
            None => {
                target.insert(key, source_item.clone());
            }
        }
    }
}

fn remove_toml_item(target: &mut toml_edit::Item, source: &toml_edit::Item) {
    if let Some(source_table) = source.as_table_like() {
        if let Some(target_table) = target.as_table_like_mut() {
            remove_toml_table_like(target_table, source_table);
            if target_table.is_empty() {
                *target = toml_edit::Item::None;
            }
            return;
        }
    }

    if let Some(source_value) = source.as_value() {
        let mut remove_item = false;

        if let Some(target_value) = target.as_value_mut() {
            match (target_value, source_value) {
                (toml_edit::Value::Array(target_arr), toml_edit::Value::Array(source_arr)) => {
                    toml_remove_array_items(target_arr, source_arr);
                    remove_item = target_arr.is_empty();
                }
                (target_value, source_value) if toml_value_is_subset(target_value, source_value) => {
                    remove_item = true;
                }
                _ => {}
            }
        }

        if remove_item {
            *target = toml_edit::Item::None;
        }
    }
}

pub(crate) fn remove_toml_table_like(
    target: &mut dyn toml_edit::TableLike,
    source: &dyn toml_edit::TableLike,
) {
    let keys: Vec<String> = source.iter().map(|(key, _)| key.to_string()).collect();

    for key in keys {
        let mut remove_key = false;
        if let (Some(target_item), Some(source_item)) = (target.get_mut(&key), source.get(&key)) {
            remove_toml_item(target_item, source_item);
            remove_key = target_item.is_none()
                || target_item
                    .as_table_like()
                    .is_some_and(|table_like| table_like.is_empty());
        }

        if remove_key {
            target.remove(&key);
        }
    }
}

pub(crate) fn contains_common_config_snippet(
    app_type: &AppType,
    settings: &Value,
    snippet: &str,
) -> bool {
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        return false;
    }

    match app_type {
        AppType::Claude => match serde_json::from_str::<Value>(trimmed) {
            Ok(source) if source.is_object() => json_value_is_subset(settings, &source),
            _ => false,
        },
        AppType::Codex => {
            let config_toml = codex_config_text_from_settings(settings).unwrap_or("");
            if config_toml.trim().is_empty() {
                return false;
            }

            let target_doc = match config_toml.parse::<toml_edit::DocumentMut>() {
                Ok(doc) => doc,
                Err(_) => return false,
            };
            let source_doc = match trimmed.parse::<toml_edit::DocumentMut>() {
                Ok(doc) => doc,
                Err(_) => return false,
            };

            toml_item_is_subset(target_doc.as_item(), source_doc.as_item())
        }
        AppType::Gemini => match serde_json::from_str::<Value>(trimmed) {
            Ok(Value::Object(source_map)) => {
                let Some(target_map) = gemini_env_map_from_settings(settings) else {
                    return false;
                };
                source_map.iter().all(|(key, source_value)| {
                    target_map
                        .get(key)
                        .is_some_and(|target_value| {
                            json_value_is_subset(target_value, source_value)
                        })
                })
            }
            _ => false,
        },
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes | AppType::ClaudeDesktop => false,
    }
}

pub(crate) fn provider_uses_common_config(
    app_type: &AppType,
    provider: &Provider,
    snippet: Option<&str>,
) -> bool {
    match provider
        .meta
        .as_ref()
        .and_then(|meta| meta.common_config_enabled)
    {
        Some(explicit) => explicit && snippet.is_some_and(|value| !value.trim().is_empty()),
        None => snippet.is_some_and(|value| {
            contains_common_config_snippet(app_type, &provider.settings_config, value)
        }),
    }
}

pub(crate) fn provider_common_config_storage_normalization_requires_snippet(
    provider: &Provider,
) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.common_config_enabled)
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommonConfigSettingsMutationIssue {
    ClaudeCommonConfigJson(String),
    CodexApplyTargetToml(String),
    CodexRemoveTargetToml(String),
    CodexCommonConfigSnippetToml(String),
    GeminiCommonConfigJson(String),
}

pub(crate) fn common_config_settings_mutation_issue_message(
    issue: CommonConfigSettingsMutationIssue,
) -> String {
    match issue {
        CommonConfigSettingsMutationIssue::ClaudeCommonConfigJson(error) => {
            format!("Invalid Claude common config: {error}")
        }
        CommonConfigSettingsMutationIssue::CodexApplyTargetToml(error) => {
            format!("Invalid Codex config.toml while applying common config: {error}")
        }
        CommonConfigSettingsMutationIssue::CodexRemoveTargetToml(error) => {
            format!("Invalid Codex config.toml while removing common config: {error}")
        }
        CommonConfigSettingsMutationIssue::CodexCommonConfigSnippetToml(error) => {
            format!("Invalid Codex common config snippet: {error}")
        }
        CommonConfigSettingsMutationIssue::GeminiCommonConfigJson(error) => {
            format!("Invalid Gemini common config: {error}")
        }
    }
}

pub(crate) fn apply_common_config_to_settings(
    app_type: &AppType,
    settings: &Value,
    snippet: &str,
) -> Result<Value, CommonConfigSettingsMutationIssue> {
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        return Ok(settings.clone());
    }

    match app_type {
        AppType::Claude => {
            let source = serde_json::from_str::<Value>(trimmed).map_err(|e| {
                CommonConfigSettingsMutationIssue::ClaudeCommonConfigJson(e.to_string())
            })?;
            let mut result = settings.clone();
            json_deep_merge(&mut result, &source);
            Ok(result)
        }
        AppType::Codex => {
            let mut result = settings.clone();
            let config_toml = codex_config_text_from_settings(settings).unwrap_or("");
            let mut target_doc = if config_toml.trim().is_empty() {
                toml_edit::DocumentMut::new()
            } else {
                config_toml
                    .parse::<toml_edit::DocumentMut>()
                    .map_err(|e| {
                        CommonConfigSettingsMutationIssue::CodexApplyTargetToml(
                            e.to_string(),
                        )
                    })?
            };
            let source_doc = trimmed
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| {
                    CommonConfigSettingsMutationIssue::CodexCommonConfigSnippetToml(
                        e.to_string(),
                    )
                })?;

            merge_toml_table_like(target_doc.as_table_mut(), source_doc.as_table());
            if let Some(obj) = result.as_object_mut() {
                obj.insert("config".to_string(), Value::String(target_doc.to_string()));
            }
            Ok(result)
        }
        AppType::Gemini => {
            let source = serde_json::from_str::<Value>(trimmed).map_err(|e| {
                CommonConfigSettingsMutationIssue::GeminiCommonConfigJson(e.to_string())
            })?;
            let mut result = settings.clone();
            if let Some(env) = result.get_mut("env") {
                json_deep_merge(env, &source);
            } else if let Some(obj) = result.as_object_mut() {
                obj.insert("env".to_string(), source);
            }
            Ok(result)
        }
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes | AppType::ClaudeDesktop => {
            Ok(settings.clone())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderEffectiveSettingsWarning {
    CommonConfigApply(CommonConfigSettingsMutationIssue),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProviderEffectiveSettingsResult {
    pub(crate) settings: Value,
    pub(crate) warnings: Vec<ProviderEffectiveSettingsWarning>,
}

pub(crate) fn build_effective_settings_with_common_config(
    app_type: &AppType,
    provider: &Provider,
    snippet: Option<&str>,
) -> ProviderEffectiveSettingsResult {
    let mut settings = provider.settings_config.clone();
    let mut warnings = Vec::new();

    if provider_uses_common_config(app_type, provider, snippet) {
        if let Some(snippet_text) = snippet {
            match apply_common_config_to_settings(app_type, &settings, snippet_text) {
                Ok(applied_settings) => settings = applied_settings,
                Err(issue) => warnings.push(ProviderEffectiveSettingsWarning::CommonConfigApply(
                    issue,
                )),
            }
        }
    }

    ProviderEffectiveSettingsResult { settings, warnings }
}

pub(crate) fn remove_common_config_from_settings(
    app_type: &AppType,
    settings: &Value,
    snippet: &str,
) -> Result<Value, CommonConfigSettingsMutationIssue> {
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        return Ok(settings.clone());
    }

    match app_type {
        AppType::Claude => {
            let source = serde_json::from_str::<Value>(trimmed).map_err(|e| {
                CommonConfigSettingsMutationIssue::ClaudeCommonConfigJson(e.to_string())
            })?;
            let mut result = settings.clone();
            json_deep_remove(&mut result, &source);
            Ok(result)
        }
        AppType::Codex => {
            let mut result = settings.clone();
            let config_toml = codex_config_text_from_settings(settings).unwrap_or("");
            let mut target_doc = if config_toml.trim().is_empty() {
                toml_edit::DocumentMut::new()
            } else {
                config_toml
                    .parse::<toml_edit::DocumentMut>()
                    .map_err(|e| {
                        CommonConfigSettingsMutationIssue::CodexRemoveTargetToml(
                            e.to_string(),
                        )
                    })?
            };
            let source_doc = trimmed
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| {
                    CommonConfigSettingsMutationIssue::CodexCommonConfigSnippetToml(
                        e.to_string(),
                    )
                })?;

            remove_toml_table_like(target_doc.as_table_mut(), source_doc.as_table());
            if let Some(obj) = result.as_object_mut() {
                obj.insert("config".to_string(), Value::String(target_doc.to_string()));
            }
            Ok(result)
        }
        AppType::Gemini => {
            let source = serde_json::from_str::<Value>(trimmed).map_err(|e| {
                CommonConfigSettingsMutationIssue::GeminiCommonConfigJson(e.to_string())
            })?;
            let mut result = settings.clone();
            if let Some(env) = result.get_mut("env") {
                json_deep_remove(env, &source);
            }
            Ok(result)
        }
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes | AppType::ClaudeDesktop => {
            Ok(settings.clone())
        }
    }
}

pub(crate) fn strip_common_config_from_live_settings_for_backfill(
    app_type: &AppType,
    provider: &Provider,
    live_settings: Value,
    snippet: Option<&str>,
) -> ProviderBackfillSettingsResult {
    let mut warnings = Vec::new();
    let backfill_settings = if provider_uses_common_config(app_type, provider, snippet) {
        match snippet {
            Some(snippet_text) => {
                match remove_common_config_from_settings(app_type, &live_settings, snippet_text) {
                    Ok(settings) => settings,
                    Err(issue) => {
                        warnings.push(ProviderBackfillSettingsWarning::CommonConfigStrip(issue));
                        live_settings
                    }
                }
            }
            None => live_settings,
        }
    } else {
        live_settings
    };

    let result =
        restore_live_settings_for_provider_backfill(app_type, provider, backfill_settings);
    warnings.extend(result.warnings);

    ProviderBackfillSettingsResult {
        settings: result.settings,
        warnings,
    }
}

pub(crate) fn normalize_provider_common_config_for_storage(
    app_type: &AppType,
    provider: &Provider,
    snippet: Option<&str>,
) -> Result<Option<Value>, CommonConfigSettingsMutationIssue> {
    if !provider_common_config_storage_normalization_requires_snippet(provider) {
        return Ok(None);
    }

    let Some(snippet) = snippet.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };

    remove_common_config_from_settings(app_type, &provider.settings_config, snippet).map(Some)
}

pub(crate) fn proxy_takeover_marked_state_is_reusable(
    has_live_backup: bool,
    live_matches_current_proxy: bool,
) -> bool {
    has_live_backup && live_matches_current_proxy
}

pub(crate) fn proxy_takeover_should_restore_existing_backup_before_retakeover(
    has_live_backup: bool,
    live_matches_current_proxy: bool,
) -> bool {
    has_live_backup && !live_matches_current_proxy
}

pub(crate) fn provider_is_official_category(provider: &Provider) -> bool {
    provider.category.as_deref() == Some("official")
}

pub(crate) fn should_emit_proxy_official_warning_for_provider(provider: &Provider) -> bool {
    provider_is_official_category(provider)
}

pub(crate) fn should_reapply_codex_official_live_for_provider(provider: &Provider) -> bool {
    provider_is_official_category(provider)
}

pub(crate) use crate::proxy_core::api::routing::{
    current_provider_db_fallback_required, current_provider_id_from_sources,
    current_provider_id_option_from_sources, failover_switch_pending_key,
    legacy_provider_codex_catalog_models_from_settings, legacy_provider_config_text_from_settings,
    legacy_provider_env_from_settings,
    normalize_channel_base_url, normalize_proxy_channel_key_patch_request_fields,
    normalize_proxy_channel_key_write_request_fields,
    normalize_proxy_channel_model_write_request_fields,
    normalize_proxy_channel_models_replace_request_fields,
    normalize_proxy_channel_patch_request_fields, normalize_proxy_channel_write_request_fields,
    normalize_required_channel_string,
    plan_auto_failover_toggle, provider_failover_circuit_lookups,
    provider_selection_candidate_from_failover_lookup, restored_provider_switchback_decision,
    select_provider_ids, should_block_proxy_switch_to_provider_category,
    apply_route_candidate_circuit_availability, resolve_channel_route,
    route_candidate_channel_circuit_keys, stable_channel_id,
};

#[cfg(test)]
pub(crate) use crate::proxy_core::api::routing::{
    build_legacy_channel_projection, infer_legacy_channel_interface, legacy_channel_priority,
};

pub(crate) fn legacy_provider_projection_input(
    provider: &Provider,
) -> LegacyProviderProjectionInput {
    let config_text = legacy_provider_config_text_from_settings(&provider.settings_config);
    let env = legacy_provider_env_from_settings(&provider.settings_config);
    let codex_catalog_models =
        legacy_provider_codex_catalog_models_from_settings(&provider.settings_config);
    let (api_format, claude_desktop_model_routes) = provider
        .meta
        .as_ref()
        .map(|meta| {
            let routes = meta
                .claude_desktop_model_routes
                .iter()
                .map(|(public_model, route)| LegacyModelRouteInput {
                    public_model: public_model.clone(),
                    upstream_model: route.model.clone(),
                })
                .collect::<Vec<_>>();
            (meta.api_format.clone(), routes)
        })
        .unwrap_or_default();

    LegacyProviderProjectionInput {
        api_format,
        codex_wire_api: config_text.and_then(extract_codex_wire_api),
        codex_model: config_text.and_then(extract_codex_model),
        codex_catalog_models,
        env,
        claude_desktop_model_routes,
    }
}

pub(crate) fn legacy_channel_migration_preview_from_providers<'a>(
    app_type: &str,
    app: Option<&AppType>,
    current_provider_id: Option<&str>,
    providers: impl IntoIterator<Item = &'a Provider>,
) -> ProxyChannelMigrationPreview {
    let provider_inputs = providers
        .into_iter()
        .map(|provider| {
            let primary_base_url = app
                .map(|app| provider.resolve_usage_credentials(app).0)
                .unwrap_or_default();
            let endpoints = provider
                .meta
                .as_ref()
                .map(|meta| {
                    meta.custom_endpoints
                        .values()
                        .map(|endpoint| LegacyEndpointInput {
                            url: endpoint.url.clone(),
                            added_at: endpoint.added_at,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            LegacyProviderChannelMigrationInput {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                provider_sort_index: provider.sort_index,
                provider_in_failover_queue: provider.in_failover_queue,
                primary_base_url,
                endpoints,
                provider_projection: legacy_provider_projection_input(provider),
            }
        })
        .collect::<Vec<_>>();

    let plan = crate::proxy_core::api::routing::build_legacy_channel_migration_plan(
        LegacyChannelMigrationPlanInput {
            app_type: app_type.to_string(),
            app: app.map(|app| ProxyCoreAppKind::from(app.as_str())),
            current_provider_id: current_provider_id.map(ToString::to_string),
            providers: provider_inputs,
        },
    );
    let channels = plan
        .channels
        .into_iter()
        .map(|projection| {
            let source_kind = proxy_channel_source_kind_from_legacy(&projection.source_kind);
            proxy_channel_record_from_legacy_projection(projection, source_kind)
        })
        .collect();

    ProxyChannelMigrationPreview {
        app_type: plan.app_type,
        channels,
        duplicate_count: plan.duplicate_count,
        needs_review_count: plan.needs_review_count,
    }
}

fn proxy_channel_source_kind_from_legacy(source_kind: &str) -> ProxyChannelSourceKind {
    match source_kind {
        crate::proxy_core::api::routing::LEGACY_PRIMARY_SOURCE => {
            ProxyChannelSourceKind::LegacyPrimary
        }
        crate::proxy_core::api::routing::LEGACY_ENDPOINT_SOURCE => {
            ProxyChannelSourceKind::LegacyEndpoint
        }
        _ => ProxyChannelSourceKind::Manual,
    }
}

pub(crate) fn proxy_channel_record_from_legacy_projection(
    projection: LegacyChannelProjection,
    source_kind: ProxyChannelSourceKind,
) -> ProxyChannelRecord {
    ProxyChannelRecord {
        id: projection.id,
        provider_id: projection.provider_id,
        app_type: projection.app_type,
        name: projection.name,
        status: projection.status,
        base_url: projection.base_url,
        interface_kind: projection.interface_kind,
        auth_profile_ref: projection.auth_profile_ref,
        groups: projection.groups,
        priority: projection.priority,
        weight: projection.weight,
        retry_policy: projection.retry_policy,
        health_policy: projection.health_policy,
        header_overrides: projection.header_overrides,
        param_overrides: projection.param_overrides,
        status_code_mapping: projection.status_code_mapping,
        tags: projection.tags,
        metadata: projection.metadata,
        source_kind,
        source_endpoint_url: projection.source_endpoint_url,
        models: projection
            .models
            .into_iter()
            .map(proxy_channel_model_record_from_legacy)
            .collect(),
        needs_review: projection.needs_review,
        review_reasons: projection.review_reasons,
    }
}

fn proxy_channel_model_record_from_legacy(
    route: LegacyChannelModelProjection,
) -> ProxyChannelModelRecord {
    ProxyChannelModelRecord {
        channel_id: route.channel_id,
        public_model: route.public_model,
        upstream_model: route.upstream_model,
        capabilities: route.capabilities,
        pricing_model: route.pricing_model,
        request_overrides: route.request_overrides,
        response_overrides: route.response_overrides,
    }
}

fn extract_codex_wire_api(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<toml::Value>().ok()?;
    if let Some(active_provider) = doc.get("model_provider").and_then(|value| value.as_str()) {
        if let Some(wire_api) = doc
            .get("model_providers")
            .and_then(|providers| providers.get(active_provider))
            .and_then(|provider| provider.get("wire_api"))
            .and_then(|value| value.as_str())
        {
            return Some(wire_api.to_string());
        }
    }
    doc.get("wire_api")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn extract_codex_model(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<toml::Value>().ok()?;
    doc.get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

impl From<&AppType> for AppKind {
    fn from(value: &AppType) -> Self {
        Self::from(value.as_str())
    }
}

pub(crate) fn proxy_core_app_kind_from_app_type(app_type: &AppType) -> ProxyCoreAppKind {
    AppKind::from(app_type)
}

pub(crate) fn cc_switch_app_kinds() -> Vec<AppKind> {
    AppType::all().map(|app| AppKind::from(&app)).collect()
}

pub(crate) fn app_type_option_from_proxy_core_app(app: &AppKind) -> Option<AppType> {
    app.as_str().parse::<AppType>().ok()
}

pub(crate) fn app_type_from_proxy_core_app(app: &AppKind) -> ProxyCoreResult<AppType> {
    app.as_str()
        .parse::<AppType>()
        .map_err(|error| ProxyCoreError::Config(unsupported_app_kind_error_message(error)))
}

pub(crate) struct JsonProxyRequestInput {
    pub(crate) app_type: AppType,
    pub(crate) method: Method,
    pub(crate) endpoint: String,
    pub(crate) inbound_interface: InterfaceKind,
    pub(crate) body: Value,
    pub(crate) requested_model: Option<String>,
    pub(crate) headers: HeaderMap,
    pub(crate) extensions: http::Extensions,
}

pub(crate) struct ParsedJsonProxyBody {
    pub(crate) body: Value,
    pub(crate) is_stream: bool,
}

pub(crate) fn parse_json_proxy_request_body(
    body_bytes: &Bytes,
) -> Result<ParsedJsonProxyBody, RequestBodyJsonParseError> {
    let body = parse_json_request_body(body_bytes.as_ref())?;
    Ok(parsed_json_proxy_body_from_value(body))
}

pub(crate) fn parse_json_proxy_request_body_or_null(
    body_bytes: &Bytes,
) -> Result<ParsedJsonProxyBody, RequestBodyJsonParseError> {
    let body = parse_json_request_body_or_null(body_bytes.as_ref())?;
    Ok(parsed_json_proxy_body_from_value(body))
}

fn parsed_json_proxy_body_from_value(body: Value) -> ParsedJsonProxyBody {
    let is_stream = request_body_stream_flag(&body);
    ParsedJsonProxyBody { body, is_stream }
}

pub(crate) fn json_proxy_request_from_input(input: JsonProxyRequestInput) -> ProxyRequest {
    ProxyRequest::new(
        AppKind::from(&input.app_type),
        input.method,
        input.endpoint,
        input.inbound_interface,
        ProxyBody::Json(input.body),
    )
    .with_observed_request_context(input.requested_model, input.headers, input.extensions)
}

pub(crate) struct ForwardRuntimeRequest {
    pub(crate) app_type: AppType,
    pub(crate) method: Method,
    pub(crate) endpoint: String,
    pub(crate) headers: HeaderMap,
    pub(crate) extensions: http::Extensions,
    pub(crate) body: Value,
    pub(crate) session_result: SessionIdResult,
}

pub(crate) fn forward_runtime_request_from_proxy_request(
    request: ProxyRequest,
) -> ProxyCoreResult<ForwardRuntimeRequest> {
    let ProxyRequest {
        app,
        method,
        endpoint,
        headers,
        extensions,
        body,
        ..
    } = request;
    let app_type = app_type_from_proxy_core_app(&app)?;
    let body = body.into_json()?;
    let session_result = extract_proxy_session_id(&headers, &body, app_type.as_str());
    Ok(ForwardRuntimeRequest {
        app_type,
        method,
        endpoint,
        headers,
        extensions,
        body,
        session_result,
    })
}

pub(crate) fn unsupported_app_kind_error_message(error: impl std::fmt::Display) -> String {
    crate::proxy_core::api::domain::unsupported_app_kind_error_message(&error.to_string())
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreProviderSpec {
    fn to_proxy_core_provider_spec(&self, app_type: &AppType) -> ProviderSpec;
}

impl ToProxyCoreProviderSpec for Provider {
    fn to_proxy_core_provider_spec(&self, app_type: &AppType) -> ProviderSpec {
        let kind = provider_kind_from_app_type_and_config(app_type, self);
        let metadata = provider_metadata_without_secrets(self);

        ProviderSpec {
            id: self.id.clone(),
            name: self.name.clone(),
            kind,
            account_ref: account_ref(self),
            metadata,
        }
    }
}

pub(crate) fn proxy_provider_to_core_spec(provider: &Provider, app_type: &AppType) -> ProviderSpec {
    provider.to_proxy_core_provider_spec(app_type)
}

pub(crate) fn proxy_providers_to_core_specs(
    providers: impl IntoIterator<Item = Provider>,
    app_type: &AppType,
) -> Vec<ProviderSpec> {
    providers
        .into_iter()
        .map(|provider| proxy_provider_to_core_spec(&provider, app_type))
        .collect()
}

pub(crate) fn provider_specs_from_source(
    app: &AppKind,
    providers: impl IntoIterator<Item = Provider>,
) -> ProxyCoreResult<Vec<ProviderSpec>> {
    let app_type = app_type_from_proxy_core_app(app)?;
    Ok(proxy_providers_to_core_specs(providers, &app_type))
}

pub(crate) fn provider_specs_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<Vec<ProviderSpec>> {
    let providers = db
        .get_all_providers(app.as_str())
        .map_err(|error| app_error("list providers", error))?;
    provider_specs_from_source(app, providers.into_values())
}

pub(crate) fn provider_spec_from_source(
    app: &AppKind,
    provider: Option<Provider>,
) -> ProxyCoreResult<Option<ProviderSpec>> {
    let app_type = app_type_from_proxy_core_app(app)?;
    Ok(provider.map(|provider| proxy_provider_to_core_spec(&provider, &app_type)))
}

pub(crate) fn provider_spec_from_db_source(
    db: &Database,
    app: &AppKind,
    provider_id: &str,
) -> ProxyCoreResult<Option<ProviderSpec>> {
    let provider = db
        .get_provider_by_id(provider_id, app.as_str())
        .map_err(|error| app_error("get provider", error))?;
    provider_spec_from_source(app, provider)
}

pub(crate) fn current_provider_id_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<Option<String>> {
    db.get_current_provider(app.as_str())
        .map_err(|error| app_error("get current provider", error))
}

pub(crate) async fn active_route_target_from_runtime_source(
    current_providers: &RwLock<HashMap<String, CurrentRouteTarget>>,
    app: &AppKind,
) -> ProxyCoreResult<Option<CurrentRouteTarget>> {
    let current_providers = current_providers.read().await;
    Ok(current_providers.get(app.as_str()).cloned())
}

pub(crate) fn route_candidate_provider_ids_from_selection_result(
    result: Result<Vec<Provider>, AppError>,
) -> ProxyCoreResult<Vec<String>> {
    let selection_result = match result {
        Ok(providers) => Ok(providers.into_iter().map(|provider| provider.id).collect()),
        Err(error) => match provider_selection_failure_from_app_error(&error) {
            Some(failure) => Err(failure),
            None => return Err(app_error("select route candidate providers", error)),
        },
    };
    Ok(
        crate::proxy_core::api::routing::route_candidate_provider_ids_from_selection_result(
            selection_result,
        ),
    )
}

pub(crate) async fn route_candidate_provider_ids_from_router_source(
    router: &ProviderRouter,
    app: &AppKind,
) -> ProxyCoreResult<Vec<String>> {
    route_candidate_provider_ids_from_selection_result(router.select_providers(app.as_str()).await)
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelSpec {
    fn to_proxy_core_channel_spec(&self) -> ChannelSpec;
}

impl ToProxyCoreChannelSpec for ProxyChannelRecord {
    fn to_proxy_core_channel_spec(&self) -> ChannelSpec {
        channel_spec_from_input(ChannelSpecInput {
            id: self.id.clone(),
            provider_id: self.provider_id.clone(),
            app_type: self.app_type.clone(),
            name: self.name.clone(),
            status: self.status.clone(),
            base_url: self.base_url.clone(),
            interface_kind: self.interface_kind.clone(),
            auth_profile_ref: self.auth_profile_ref.clone(),
            models: self
                .models
                .iter()
                .map(ProxyChannelModelRecord::to_proxy_core_model_route_input)
                .collect(),
            groups: self.groups.clone(),
            priority: self.priority,
            weight: self.weight,
            retry_policy: self.retry_policy.clone(),
            health_policy: self.health_policy.clone(),
            header_overrides: self.header_overrides.clone(),
            param_overrides: self.param_overrides.clone(),
            status_code_mapping: self.status_code_mapping.clone(),
            tags: self.tags.clone(),
            metadata: self.metadata.clone(),
            source_ref: self.source_endpoint_url.clone(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        })
    }
}

pub(crate) fn proxy_channel_record_to_core_spec(channel: &ProxyChannelRecord) -> ChannelSpec {
    channel.to_proxy_core_channel_spec()
}

pub(crate) fn proxy_channel_records_to_core_specs_for_query(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
    query: &ChannelQuery<'_>,
) -> Vec<ChannelSpec> {
    channels
        .into_iter()
        .map(|channel| proxy_channel_record_to_core_spec(&channel))
        .filter(|channel| channel_matches_query(channel, query))
        .collect()
}

pub(crate) fn channel_specs_from_source(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
    query: &ChannelQuery<'_>,
) -> Vec<ChannelSpec> {
    proxy_channel_records_to_core_specs_for_query(channels, query)
}

pub(crate) async fn channel_specs_from_source_lookup(
    db: &Database,
    router: &ProviderRouter,
    query: ChannelQuery<'_>,
) -> ProxyCoreResult<Vec<ChannelSpec>> {
    let channels = if query.allow_legacy_projection {
        router
            .list_channels_for_app(query.app.as_str())
            .await
            .map_err(|error| app_error("list channels", error))?
            .0
    } else {
        db.list_proxy_channels_for_app(query.app.as_str())
            .map_err(|error| app_error("list materialized channels", error))?
    };
    Ok(channel_specs_from_source(channels, &query))
}

pub(crate) fn channel_spec_from_source(channel: Option<ProxyChannelRecord>) -> Option<ChannelSpec> {
    channel.map(|channel| proxy_channel_record_to_core_spec(&channel))
}

pub(crate) fn channel_spec_from_source_lookup(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<Option<ChannelSpec>> {
    let channel = db
        .get_proxy_channel(channel_id)
        .map_err(|error| app_error("get channel", error))?;
    Ok(channel_spec_from_source(channel))
}

pub(crate) fn channel_route_source_for_materialized_records(
    channels: &[ProxyChannelRecord],
) -> ChannelRouteSource {
    crate::proxy_core::api::management::channel_route_source_for_materialized_count(channels.len())
}

pub(crate) fn channel_route_should_load_legacy_projection(source: &ChannelRouteSource) -> bool {
    source == &ChannelRouteSource::LegacyProjection
}

pub(crate) fn channel_route_records_from_sources(
    materialized_channels: Vec<ProxyChannelRecord>,
    load_legacy_projection: impl FnOnce() -> Result<ProxyChannelMigrationPreview, AppError>,
) -> Result<(Vec<ProxyChannelRecord>, ChannelRouteSource), AppError> {
    let source = channel_route_source_for_materialized_records(&materialized_channels);
    if !channel_route_should_load_legacy_projection(&source) {
        return Ok((materialized_channels, source));
    }

    let preview = load_legacy_projection()?;
    Ok((preview.channels, source))
}

pub(crate) fn proxy_channel_route_inputs_to_core(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
) -> Vec<RouteResolveChannelInput> {
    channels
        .into_iter()
        .map(|channel| {
            let ProxyChannelRecord {
                id,
                provider_id,
                name,
                status,
                base_url,
                interface_kind,
                groups,
                models,
                priority,
                weight,
                source_kind,
                ..
            } = channel;

            route_resolve_channel_input_from_record(RouteResolveChannelRecordInput {
                channel_id: id,
                provider_id,
                channel_name: name,
                status,
                base_url,
                interface_kind,
                groups,
                models: models
                    .into_iter()
                    .map(|model| RouteResolveModelRecordInput {
                        public_model: model.public_model,
                        upstream_model: model.upstream_model,
                    })
                    .collect(),
                priority,
                weight,
                source_kind: source_kind.as_str().to_string(),
            })
        })
        .collect()
}

pub(crate) fn claude_desktop_model_routes_to_core_inputs(
    routes: impl IntoIterator<Item = ResolvedModelRoute>,
) -> Vec<ClaudeDesktopModelRouteInput> {
    routes
        .into_iter()
        .map(|route| ClaudeDesktopModelRouteInput::new(route.route_id, route.supports_1m))
        .collect()
}

pub(crate) fn codex_default_model_context_window() -> u64 {
    crate::proxy_core::api::model_catalog::DEFAULT_CODEX_MODEL_CONTEXT_WINDOW
}

pub(crate) use crate::proxy_core::api::model_catalog::{
    build_codex_model_catalog_from_settings as codex_model_catalog_from_settings,
    client_model_catalog_raw_from_text, empty_client_model_catalog_raw,
    has_codex_model_catalog_specs as codex_settings_have_model_catalog_specs,
    provider_model_catalog_from_settings, simplify_codex_model_catalog,
};

pub(crate) fn provider_model_catalog_from_provider(
    provider_id: &str,
    provider: Option<&Provider>,
) -> ModelCatalog {
    provider_model_catalog_from_settings(
        provider_id,
        provider.map(|provider| &provider.settings_config),
    )
}

pub(crate) fn provider_model_catalog_from_db_source(
    db: &Database,
    app: &AppKind,
    provider_id: &str,
) -> ProxyCoreResult<ModelCatalog> {
    let provider = db
        .get_provider_by_id(provider_id, app.as_str())
        .map_err(|error| app_error("load model catalog", error))?;
    Ok(provider_model_catalog_from_provider(
        provider_id,
        provider.as_ref(),
    ))
}

pub(crate) fn claude_desktop_provider_from_selection_result(
    result: Result<Vec<Provider>, AppError>,
) -> ProxyCoreResult<Provider> {
    let providers = result.map_err(|error| {
        ProxyCoreError::Internal(format!("select claude desktop provider: {error}"))
    })?;
    providers.into_iter().next().ok_or_else(|| {
        ProxyCoreError::Unavailable("no available claude desktop provider".to_string())
    })
}

pub(crate) async fn claude_desktop_model_routes_from_router_source(
    router: &ProviderRouter,
    app: &AppKind,
) -> ProxyCoreResult<Vec<ClaudeDesktopModelRouteInput>> {
    let providers = router.select_providers(app.as_str()).await;
    let provider = claude_desktop_provider_from_selection_result(providers)?;
    let routes = crate::claude_desktop_config::proxy_model_routes(&provider)
        .map_err(|error| app_error("load claude desktop model routes", error))?;
    Ok(claude_desktop_model_routes_to_core_inputs(routes))
}

pub(crate) fn provider_model_catalog_raw_value(provider: &Provider) -> Option<&Value> {
    provider.settings_config.get("modelCatalog")
}

pub(crate) fn codex_live_settings_with_model_catalog(
    mut live_settings: Value,
    model_catalog: Option<Value>,
) -> Value {
    if let (Some(root), Some(model_catalog)) = (live_settings.as_object_mut(), model_catalog) {
        root.insert("modelCatalog".to_string(), model_catalog);
    }

    live_settings
}

pub(crate) fn attach_codex_model_catalog_from_provider(
    live_config: &mut Value,
    provider: Option<&Provider>,
) {
    let Some(root) = live_config.as_object_mut() else {
        return;
    };

    let Some(provider) = provider else {
        return;
    };

    let model_catalog = provider_model_catalog_raw_value(provider)
        .cloned()
        .unwrap_or_else(|| json!({ "models": [] }));
    root.insert("modelCatalog".to_string(), model_catalog);
}

pub(crate) fn client_model_catalog_from_optional_raw(
    app: &AppKind,
    raw: Option<Value>,
) -> ModelCatalog {
    crate::proxy_core::api::model_catalog::client_model_catalog_from_optional_raw(
        app.as_str(),
        raw,
    )
}

pub(crate) fn client_model_catalog_raw_from_source(
    source: ClientModelCatalogSource,
) -> Option<Value> {
    match source {
        ClientModelCatalogSource::CodexActiveConfig => {
            Some(codex_client_model_catalog_raw_from_active_config())
        }
        ClientModelCatalogSource::Empty => None,
    }
}

pub(crate) fn client_model_catalog_from_app_source(
    app: &AppKind,
) -> ProxyCoreResult<ModelCatalog> {
    let source = client_model_catalog_source_for_app(app.as_str());
    let raw = client_model_catalog_raw_from_source(source);
    Ok(client_model_catalog_from_optional_raw(app, raw))
}

pub(crate) fn codex_client_model_catalog_raw_from_active_config() -> Value {
    let generated_path = crate::codex_config::get_codex_model_catalog_path();
    let active_catalog_path = match crate::codex_config::read_codex_config_text() {
        Ok(config_text) => {
            crate::codex_config::resolve_cc_switch_catalog_path(&config_text, &generated_path)
        }
        Err(_) => None,
    };

    if let Some(catalog_path) = active_catalog_path.as_ref().filter(|path| path.exists()) {
        let text = std::fs::read_to_string(catalog_path).unwrap_or_default();
        client_model_catalog_raw_from_text(&text)
    } else {
        if active_catalog_path.is_none() {
            log::debug!(
                "[models] stale guard: catalog not served (model_catalog_json not set to cc-switch catalog)"
            );
        }
        empty_client_model_catalog_raw()
    }
}

#[cfg(test)]
pub(crate) use crate::proxy_core::api::routing::route_plan_provider_ids;
pub(crate) use crate::proxy_core::api::routing::route_plan_provider_match;

pub(crate) fn host_providers_for_plan(
    providers: &IndexMap<String, Provider>,
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<Provider>> {
    let provider_match = route_plan_provider_match(plan, providers.keys().map(String::as_str));
    let has_matches = provider_match.has_matches();
    let matching: Vec<_> = provider_match
        .matched_provider_ids
        .iter()
        .filter_map(|provider_id| providers.get(provider_id.as_str()).cloned())
        .collect();
    if !has_matches {
        return Err(ProxyCoreError::Unavailable(
            route_plan_providers_unconfigured_error_message().to_string(),
        ));
    }
    Ok(matching)
}

pub(crate) fn forward_attempts_from_plan(
    app_type: &AppType,
    providers: &[Provider],
    plan: &RoutePlan,
) -> Vec<ForwardAttempt> {
    crate::proxy::route_attempt::forward_attempts_from_route_plan(app_type, providers, plan)
}

pub(crate) fn required_forward_attempts_from_plan(
    app_type: &AppType,
    providers: &[Provider],
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<ForwardAttempt>> {
    let attempts = forward_attempts_from_plan(app_type, providers, plan);
    if attempts.is_empty() {
        return Err(route_plan_no_matching_host_providers_error());
    }
    Ok(attempts)
}

pub(crate) fn apply_channel_auth_profile_providers_from_source(
    app_type: &AppType,
    providers: &IndexMap<String, Provider>,
    attempts: &mut [ForwardAttempt],
    mut load_channel_key_value: impl FnMut(&str, &str) -> ProxyCoreResult<Option<String>>,
) -> ProxyCoreResult<()> {
    for attempt in attempts {
        let auth_profile_ref = attempt
            .channel()
            .and_then(|channel| channel.auth_profile_ref.as_ref())
            .map(String::as_str);
        let channel_id = attempt.channel().map(|channel| channel.channel_id.as_str());
        match channel_auth_profile_action(app_type.as_str(), auth_profile_ref, channel_id) {
            ChannelAuthProfileAction::Provider {
                provider_id,
                missing_provider_warning,
            } => {
                let Some(provider) = providers.get(&provider_id).cloned() else {
                    log::warn!("{missing_provider_warning}");
                    continue;
                };
                attempt.set_auth_provider(provider);
            }
            ChannelAuthProfileAction::ChannelKey {
                channel_id,
                key_ref,
            } => {
                let Some(key_value) = load_channel_key_value(&channel_id, &key_ref)? else {
                    return Err(channel_key_auth_error(&channel_id, &key_ref));
                };
                attempt.set_auth_provider(provider_with_channel_auth_key(
                    app_type,
                    attempt.provider(),
                    &key_value,
                ));
            }
            ChannelAuthProfileAction::Ignore => {
                continue;
            }
        }
    }
    Ok(())
}

pub(crate) fn apply_channel_auth_profile_providers_from_db(
    db: &Database,
    app_type: &AppType,
    providers: &IndexMap<String, Provider>,
    attempts: &mut [ForwardAttempt],
) -> ProxyCoreResult<()> {
    apply_channel_auth_profile_providers_from_source(
        app_type,
        providers,
        attempts,
        |channel_id, key_ref| {
            let key = db
                .get_enabled_proxy_channel_key(channel_id, key_ref)
                .map_err(|error| app_error("load channel auth key", error))?;
            Ok(channel_key_value_from_record(key))
        },
    )
}

pub(crate) fn required_forward_attempts_from_db_sources(
    db: &Database,
    app_type: &AppType,
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<ForwardAttempt>> {
    let all_providers = db
        .get_all_providers(app_type.as_str())
        .map_err(|error| app_error("load host providers", error))?;
    let providers = host_providers_for_plan(&all_providers, plan)?;
    let mut attempts = required_forward_attempts_from_plan(app_type, &providers, plan)?;
    apply_channel_auth_profile_providers_from_db(db, app_type, &all_providers, &mut attempts)?;
    Ok(attempts)
}

#[derive(Clone)]
pub(crate) struct ForwarderRuntimeHostResources {
    pub(crate) provider_router: Arc<ProviderRouter>,
    pub(crate) status: Arc<RwLock<ProxyRuntimeStatus>>,
    pub(crate) current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    pub(crate) events: Arc<ProxyEventBus>,
    pub(crate) gemini_shadow: Arc<GeminiShadowStore>,
    pub(crate) codex_chat_history: Arc<CodexChatHistoryStore>,
    pub(crate) failover_manager: Arc<FailoverSwitchManager>,
    pub(crate) app_handle: Option<tauri::AppHandle>,
}

pub(crate) async fn forward_with_preplanned_host_runtime(
    resources: ForwarderRuntimeHostResources,
    request: ForwardRuntimeRequest,
    plan: RoutePlan,
    forwarder_config: ForwarderRuntimeConfig,
    current_provider_id: String,
    attempts: Vec<ForwardAttempt>,
) -> ProxyCoreResult<ProxyResult> {
    let ForwarderRuntimeHostResources {
        provider_router,
        status,
        current_providers,
        events,
        gemini_shadow,
        codex_chat_history,
        failover_manager,
        app_handle,
    } = resources;
    let ForwardRuntimeRequest {
        app_type,
        method,
        endpoint,
        headers,
        extensions,
        body,
        session_result,
    } = request;
    let forwarder_options = forwarder_config.options;
    let forwarder = RequestForwarder::new_preplanned(
        provider_router,
        forwarder_options.non_streaming_timeout,
        status,
        current_providers,
        events,
        gemini_shadow,
        codex_chat_history,
        failover_manager,
        app_handle,
        current_provider_id,
        session_result.session_id,
        session_result.client_provided,
        forwarder_options.streaming_first_byte_timeout,
        forwarder_options.streaming_idle_timeout,
        forwarder_config.rectifier,
        forwarder_config.optimizer,
        forwarder_config.copilot_optimizer,
        forwarder_options.max_retries,
    );

    let result = forwarder
        .forward_with_preplanned_attempts(
            &app_type, method, &endpoint, body, headers, extensions, attempts,
        )
        .await
        .map_err(forward_error_to_core_error)?;
    Ok(forward_result_to_proxy_result(result, plan))
}

pub(crate) async fn forward_proxy_request_with_host_runtime(
    db: &Database,
    resources: ForwarderRuntimeHostResources,
    request: ProxyRequest,
    plan: RoutePlan,
) -> ProxyCoreResult<ProxyResult> {
    let forward_request = forward_runtime_request_from_proxy_request(request)?;
    let app_type = forward_request.app_type.clone();
    let forwarder_config = forwarder_runtime_config_from_db_sources(db, &app_type).await?;
    let current_provider_id = forward_current_provider_id_from_db_sources(db, &app_type);
    let attempts = required_forward_attempts_from_db_sources(db, &app_type, &plan)?;

    forward_with_preplanned_host_runtime(
        resources,
        forward_request,
        plan,
        forwarder_config,
        current_provider_id,
        attempts,
    )
    .await
}

pub(crate) trait HostForwardRuntime {
    fn forward_host<'a>(
        &'a self,
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>>;
}

pub(crate) fn forward_with_optional_host_runtime<'a, R>(
    runtime: Option<&'a R>,
    request: ProxyRequest,
    plan: RoutePlan,
) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>>
where
    R: HostForwardRuntime + Sync + 'a,
{
    Box::pin(async move {
        let runtime = runtime.ok_or_else(forwarding_runtime_unavailable_error)?;
        runtime.forward_host(request, plan).await
    })
}

pub(crate) use crate::proxy_core::api::routing::forwarding_requires_runtime_error_message;

pub(crate) fn forwarding_runtime_unavailable_error() -> ProxyCoreError {
    ProxyCoreError::Unsupported(forwarding_requires_runtime_error_message().to_string())
}

pub(crate) use crate::proxy_core::api::routing::route_plan_no_matching_host_providers_error_message;

pub(crate) fn route_plan_no_matching_host_providers_error() -> ProxyCoreError {
    ProxyCoreError::Unavailable(route_plan_no_matching_host_providers_error_message().to_string())
}

pub(crate) use crate::proxy_core::api::routing::route_plan_providers_unconfigured_error_message;

pub(crate) use crate::proxy_core::api::routing::{
    route_plan_selections, select_route_for_forward_result as route_selection_for_forward_result,
};

pub(crate) fn route_policy_from_failover_queue(
    app: AppKind,
    queue: impl IntoIterator<Item = FailoverQueueItem>,
) -> RoutePolicy {
    crate::proxy_core::api::routing::route_policy_from_failover_provider_ids(
        app,
        queue.into_iter().map(|item| item.provider_id),
    )
}

pub(crate) fn route_policy_from_source(
    app: AppKind,
    queue: impl IntoIterator<Item = FailoverQueueItem>,
) -> Option<RoutePolicy> {
    Some(route_policy_from_failover_queue(app, queue))
}

pub(crate) fn route_policy_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<Option<RoutePolicy>> {
    let queue = db
        .get_failover_queue(app.as_str())
        .map_err(|error| app_error("load route policy", error))?;
    Ok(route_policy_from_source(app.clone(), queue))
}

#[derive(Debug)]
pub(crate) struct ChannelHealthResetPlan {
    pub(crate) channel_id: String,
    pub(crate) app_type: String,
}

pub(crate) fn channel_health_reset_plan_from_lookup(
    channel_id: &str,
    app_type: Option<String>,
) -> ProxyCoreResult<ChannelHealthResetPlan> {
    let app_type = app_type.ok_or_else(|| channel_not_found_error(channel_id))?;
    Ok(ChannelHealthResetPlan {
        channel_id: channel_id.to_string(),
        app_type,
    })
}

pub(crate) fn channel_health_reset_from_plan(
    plan: ChannelHealthResetPlan,
) -> ChannelHealthReset {
    channel_health_reset_from_parts(plan.channel_id, plan.app_type.as_str())
}

pub(crate) struct ChannelHealthAttemptDbUpdate {
    pub(crate) channel_id: String,
    pub(crate) success: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) failure_threshold: u32,
    pub(crate) response_time_ms: Option<i64>,
}

pub(crate) fn channel_health_attempt_db_update(
    result: ChannelAttemptResult,
) -> ChannelHealthAttemptDbUpdate {
    ChannelHealthAttemptDbUpdate {
        channel_id: result.channel_id,
        success: result.success,
        error_code: result.error_code,
        failure_threshold: DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD,
        response_time_ms: result.latency_ms.map(|latency| latency as i64),
    }
}

pub(crate) fn record_channel_attempt_in_db_source(
    db: &Database,
    result: ChannelAttemptResult,
) -> ProxyCoreResult<()> {
    let update = channel_health_attempt_db_update(result);
    db.update_proxy_channel_health_with_threshold(
        &update.channel_id,
        update.success,
        update.error_code,
        update.failure_threshold,
        update.response_time_ms,
    )
    .map_err(|error| app_error("record channel attempt", error))
}

pub(crate) async fn reset_channel_health_with_router_source(
    db: &Database,
    router: &ProviderRouter,
    channel_id: &str,
) -> ProxyCoreResult<ChannelHealthReset> {
    let app_type = db
        .get_proxy_channel_app_type(channel_id)
        .map_err(|error| app_error("lookup channel app", error))?;
    let reset_plan = channel_health_reset_plan_from_lookup(channel_id, app_type)?;
    router
        .reset_channel_breaker(&reset_plan.channel_id, &reset_plan.app_type)
        .await
        .map_err(|error| app_error("reset channel health", error))?;
    Ok(channel_health_reset_from_plan(reset_plan))
}

pub(crate) fn proxy_response_to_core_response<G>(
    response: ProxyResponse,
    connection_guard: Option<G>,
) -> ProxyCoreResponse
where
    G: Send + 'static,
{
    match response {
        ProxyResponse::Buffered {
            status,
            headers,
            body,
        } => ProxyCoreResponse::with_body(status, headers, ProxyResponseBody::bytes(body)),
        ProxyResponse::Streamed {
            status,
            headers,
            stream,
        } => ProxyCoreResponse::with_body(
            status,
            headers,
            ProxyResponseBody::stream(stream_with_connection_guard(stream, connection_guard)),
        ),
        other => {
            let status = other.status();
            let headers = other.headers().clone();
            ProxyCoreResponse::with_body(
                status,
                headers,
                ProxyResponseBody::stream(stream_with_connection_guard(
                    other.bytes_stream(),
                    connection_guard,
                )),
            )
        }
    }
}

pub(crate) fn forward_result_to_proxy_result(
    result: crate::proxy::ForwardResult,
    plan: RoutePlan,
) -> ProxyResult {
    let crate::proxy::ForwardResult {
        response,
        provider,
        claude_api_format,
        outbound_model,
        selected_channel,
        connection_guard,
    } = result;
    let selected_channel_id = selected_channel
        .as_ref()
        .map(|channel| channel.channel_id.as_str());
    let response = proxy_response_to_core_response(response, connection_guard);

    proxy_result_from_forward_parts(
        response,
        plan,
        &provider,
        claude_api_format,
        outbound_model,
        selected_channel_id,
    )
}

fn stream_with_connection_guard<S, G>(
    stream: S,
    connection_guard: Option<G>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    G: Send + 'static,
{
    async_stream::stream! {
        let _connection_guard = connection_guard;
        tokio::pin!(stream);
        while let Some(chunk) = stream.next().await {
            yield chunk;
        }
    }
}

pub(crate) fn proxy_result_from_forward_parts(
    response: ProxyCoreResponse,
    plan: RoutePlan,
    provider: &Provider,
    claude_api_format: Option<String>,
    outbound_model: Option<String>,
    selected_channel_id: Option<&str>,
) -> ProxyResult {
    let selected_route =
        route_selection_for_forward_result(&plan, selected_channel_id, &provider.id);
    let mut metadata = Map::new();
    metadata.insert("hostProviderId".to_string(), json!(provider.id.clone()));
    metadata.insert("hostProviderName".to_string(), json!(provider.name.clone()));
    metadata.insert(
        CLAUDE_API_FORMAT_METADATA_KEY.to_string(),
        json!(claude_api_format),
    );
    metadata.insert("selectedChannelId".to_string(), json!(selected_channel_id));

    ProxyResult {
        response,
        selected_route,
        outbound_model,
        usage_record: None,
        metadata: Value::Object(metadata),
    }
}

pub(crate) use crate::proxy_core::api::routing::build_route_plan as route_plan_from_request;

pub(crate) async fn management_route_response_from_router_source(
    router: &ProviderRouter,
    request: RouteResolveRequest,
) -> ProxyCoreResult<RouteResolveResponse> {
    router
        .resolve_channel_route_dry_run(request)
        .await
        .map_err(|error| app_error("resolve channel route dry run", error))
}

pub(crate) use crate::proxy_core::api::config::{
    circuit_breaker_failure_decision, half_open_probe_allow_result,
    should_close_half_open_after_success, should_transition_open_to_half_open,
};

pub(crate) async fn update_all_circuit_breaker_configs_source(
    router: &ProviderRouter,
    config: CircuitBreakerConfig,
) {
    router.update_all_configs(config).await;
}

pub(crate) async fn update_app_circuit_breaker_config_source(
    router: &ProviderRouter,
    app_type: &str,
    config: CircuitBreakerConfig,
) {
    router.update_app_configs(app_type, config).await;
}

pub(crate) async fn reset_provider_circuit_breaker_source(
    router: &ProviderRouter,
    provider_id: &str,
    app_type: &str,
) {
    router.reset_provider_breaker(provider_id, app_type).await;
}

pub(crate) use crate::proxy_core::api::transport::forward_failure_kind_from_proxy_status;

pub(crate) use crate::proxy_core::api::routing::default_route_candidate_from_selection as channel_route_candidate_from_selection;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::routing::resolved_channel_attempt_from_candidate;
pub(crate) use crate::proxy_core::api::routing::resolved_channel_attempt_from_selection;

pub(crate) use crate::proxy_core::api::transport::{
    apply_channel_param_overrides_to_url, resolve_channel_response_status_mapping,
};

pub(crate) use crate::proxy_core::api::transforms::codex_proxy_error_code;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CodexProxyHostErrorFacts<'a> {
    pub(crate) status: ProxyErrorStatusKind,
    pub(crate) message: &'a str,
    pub(crate) kind: CodexProxyErrorKind,
    pub(crate) upstream_status: Option<u16>,
    pub(crate) upstream_body: Option<&'a str>,
}

#[cfg(test)]
pub(crate) fn codex_proxy_error_json_from_host_facts(
    provider_name: &str,
    request_model: &str,
    endpoint: &str,
    facts: CodexProxyHostErrorFacts<'_>,
) -> Value {
    codex_proxy_error_json(codex_proxy_error_context_from_host_facts(
        provider_name,
        request_model,
        endpoint,
        facts,
    ))
}

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::codex_proxy_error_json;

pub(crate) fn codex_proxy_error_response_from_host_facts(
    provider_name: &str,
    request_model: &str,
    endpoint: &str,
    facts: CodexProxyHostErrorFacts<'_>,
) -> ProxyCoreResult<ProxyCoreResponse> {
    codex_proxy_error_response(
        facts.status,
        codex_proxy_error_context_from_host_facts(provider_name, request_model, endpoint, facts),
    )
}

pub(crate) use crate::proxy_core::api::transforms::codex_proxy_error_response;

fn codex_proxy_error_context_from_host_facts<'a>(
    provider_name: &'a str,
    request_model: &'a str,
    endpoint: &'a str,
    facts: CodexProxyHostErrorFacts<'a>,
) -> CodexProxyErrorContext<'a> {
    CodexProxyErrorContext {
        provider_name,
        request_model,
        endpoint,
        fallback_message: facts.message,
        fallback_code: codex_proxy_error_code(facts.kind),
        upstream_status: facts.upstream_status,
        upstream_body: facts.upstream_body,
    }
}

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::apply_channel_route_model_override;

pub(crate) use crate::proxy_core::api::transport::apply_resolved_channel_model_override;

pub(crate) fn apply_channel_provider_overrides(
    app_type: &AppType,
    provider: &mut Provider,
    candidate: &ChannelRouteCandidate,
) {
    let plan = crate::proxy_core::api::routing::channel_provider_override_plan(
        &AppKind::from(app_type),
        candidate,
    );
    crate::proxy_core::api::routing::apply_channel_provider_settings_overrides(
        &mut provider.settings_config,
        &plan,
    );

    if let Some(api_format) = plan.api_format {
        provider
            .meta
            .get_or_insert_with(ProviderMeta::default)
            .api_format = Some(api_format);
    }
}

pub(crate) use crate::proxy_core::api::transforms::{
    build_codex_tool_context_from_request as codex_tool_context_from_request,
    normalize_anthropic_tool_thinking_history, normalize_codex_chat_error_body,
    normalize_deepseek_thinking_disabled_strip_effort,
    should_normalize_anthropic_tool_thinking_history,
};

pub(crate) fn provider_should_normalize_anthropic_tool_thinking_history(
    provider: &Provider,
    body: &Value,
    api_format: &str,
) -> bool {
    should_normalize_anthropic_tool_thinking_history(&provider.settings_config, body, api_format)
}

pub(crate) fn provider_normalize_deepseek_thinking_disabled_strip_effort(
    provider: &Provider,
    body: &mut Value,
) -> bool {
    normalize_deepseek_thinking_disabled_strip_effort(body, &provider.settings_config)
}

pub(crate) fn provider_claude_normalize_anthropic_messages(
    body: &mut Value,
    provider: &Provider,
    api_format: &str,
) -> bool {
    if api_format.trim() != "anthropic" {
        return false;
    }

    let mut changed =
        if provider_should_normalize_anthropic_tool_thinking_history(provider, body, api_format) {
            normalize_anthropic_tool_thinking_history(body)
        } else {
            false
        };
    changed |= provider_normalize_deepseek_thinking_disabled_strip_effort(provider, body);
    changed
}

pub(crate) use crate::proxy_core::api::transport::inject_openai_stream_include_usage;

#[cfg(test)]
pub(crate) fn anthropic_tool_thinking_placeholder() -> &'static str {
    crate::proxy_core::api::transforms::ANTHROPIC_TOOL_THINKING_PLACEHOLDER
}

#[cfg(test)]
pub(crate) fn anthropic_redacted_thinking_placeholder() -> &'static str {
    crate::proxy_core::api::transforms::ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
}

pub(crate) type ModelMappingProjection =
    crate::proxy_core::api::model_catalog::ModelMappingProjection;

pub(crate) use crate::proxy_core::api::model_catalog::apply_provider_model_mapping;

pub(crate) fn apply_provider_model_mapping_from_provider(
    body: Value,
    provider: &Provider,
) -> ModelMappingProjection {
    apply_provider_model_mapping(body, &provider.settings_config)
}

pub(crate) fn replace_images_for_text_only_provider_model(
    body: &mut Value,
    provider: &Provider,
    allow_heuristic: bool,
) -> usize {
    replace_images_for_text_only_model(body, &provider.settings_config, allow_heuristic)
}

pub(crate) fn rewrite_codex_responses_endpoint_to_chat(
    endpoint: &str,
) -> (String, Option<String>) {
    crate::proxy_core::api::transport::rewrite_codex_responses_endpoint_to_chat(endpoint)
        .into_parts()
}

pub(crate) struct ForwardUpstreamUrlPlanInput<'a> {
    pub(crate) base_url: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) is_full_url: bool,
    pub(crate) codex_responses_to_chat: bool,
    pub(crate) use_claude_transform: bool,
    pub(crate) is_copilot: bool,
    pub(crate) claude_api_format: Option<&'a str>,
    pub(crate) body: &'a Value,
    pub(crate) channel_param_overrides: Option<&'a Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwardUpstreamUrlPlan {
    pub(crate) effective_endpoint: String,
    pub(crate) passthrough_query: Option<String>,
    pub(crate) url: String,
}

pub(crate) fn forward_upstream_url_plan(
    input: ForwardUpstreamUrlPlanInput<'_>,
    build_adapter_url: impl FnOnce(&str, &str) -> String,
) -> ForwardUpstreamUrlPlan {
    let (effective_endpoint, passthrough_query) = if input.codex_responses_to_chat {
        rewrite_codex_responses_endpoint_to_chat(input.endpoint)
    } else if input.use_claude_transform {
        let api_format = input.claude_api_format.unwrap_or("anthropic");
        rewrite_claude_transform_endpoint(claude_transform_endpoint_rewrite_input_from_body(
            input.endpoint,
            api_format,
            input.is_copilot,
            input.body,
        ))
        .into_parts()
    } else {
        (
            input.endpoint.to_string(),
            split_endpoint_and_query(input.endpoint)
                .1
                .map(ToString::to_string),
        )
    };

    let codex_chat_base_is_full_endpoint =
        is_codex_chat_full_endpoint_base(input.codex_responses_to_chat, input.base_url);
    let mut url = if matches!(input.claude_api_format, Some("gemini_native")) {
        resolve_gemini_native_url(input.base_url, &effective_endpoint, input.is_full_url)
    } else if input.is_full_url || codex_chat_base_is_full_endpoint {
        append_query_to_full_url(input.base_url, passthrough_query.as_deref())
    } else {
        build_adapter_url(input.base_url, &effective_endpoint)
    };

    if let Some(param_overrides) = input.channel_param_overrides {
        url = apply_channel_param_overrides_to_url(&url, param_overrides);
    }

    ForwardUpstreamUrlPlan {
        effective_endpoint,
        passthrough_query,
        url,
    }
}

pub(crate) use crate::proxy_core::api::transforms::{
    claude_api_format_needs_transform, resolve_gemini_native_url,
};

pub(crate) use crate::proxy_core::api::transport::{
    anthropic_beta_header_value, build_upstream_request_headers,
};

pub(crate) use crate::proxy_core::api::transport::upstream_host_header_from_url;

pub(crate) use crate::proxy_core::api::transport::{
    resolve_upstream_request_transport_policy, serialize_upstream_request_body,
};

pub(crate) use crate::proxy_core::api::transport::request_body_stream_flag;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::is_streaming_upstream_request;

pub(crate) use crate::proxy_core::api::transport::is_socks_proxy_url;

pub(crate) use crate::proxy_core::api::transport::resolve_upstream_send_policy;

pub(crate) use crate::proxy_core::api::transport::{
    get_content_encoding, response_headers_indicate_sse, response_headers_log_summary,
};

pub(crate) use crate::proxy_core::api::transport::decode_response_body;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::decompress_body;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::strip_sse_field;

pub(crate) use crate::proxy_core::api::transport::{
    non_streaming_body_timeout_message, streaming_body_ended_before_first_chunk_message,
    streaming_body_first_chunk_read_error_message, streaming_body_first_chunk_timeout_message,
    streaming_header_timeout_message,
};

pub(crate) use crate::proxy_core::api::transport::{
    passthrough_bytes_proxy_response, passthrough_stream_proxy_response,
};

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::is_official_codex_client_user_agent;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::build_gemini_native_url;

pub(crate) struct UsageRequestLogProjection {
    pub(crate) log: RequestLog,
    pub(crate) missing_pricing_warning_message: Option<String>,
}

pub(crate) struct UsagePricingConfigLookup {
    pub(crate) provider_id: String,
    pub(crate) app_type: String,
}

pub(crate) async fn record_usage_with_proxy_services(
    services: &(dyn ProxyServices + Send + Sync),
    record: UsageRecord,
) {
    record_usage_with_proxy_services_context(
        services,
        record,
        UsageRecordFailureLogContext::UsageRecord,
    )
    .await
}

pub(crate) async fn record_usage_with_proxy_services_context(
    services: &(dyn ProxyServices + Send + Sync),
    record: UsageRecord,
    failure_context: UsageRecordFailureLogContext,
) {
    log::debug!("{}", usage_record_debug_log_message(&record));

    if let Err(error) = services.usage_sink().record_usage(record).await {
        log::warn!(
            "{}",
            usage_record_failure_warning_message(failure_context, error)
        );
    }
}

pub(crate) fn provider_kind_from_provider(provider: &Provider) -> Option<ProviderKind> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        .map(ProviderKind::from)
}

pub(crate) fn provider_is_codex_oauth(provider: &Provider) -> bool {
    provider_kind_from_provider(provider) == Some(ProviderKind::CodexOAuth)
}

pub(crate) fn provider_is_github_copilot(provider: &Provider) -> bool {
    provider_kind_from_provider(provider) == Some(ProviderKind::GitHubCopilot)
        || provider
            .settings_config
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(Value::as_str)
            .map(|base_url| base_url.contains("githubcopilot.com"))
            .unwrap_or(false)
}

pub(crate) fn provider_uses_managed_account_auth(provider: &Provider) -> bool {
    provider_is_github_copilot(provider)
        || provider_is_codex_oauth(provider)
        || provider
            .settings_config
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(Value::as_str)
            .map(|base_url| base_url.contains("chatgpt.com/backend-api/codex"))
            .unwrap_or(false)
}

pub(crate) fn provider_uses_anthropic_rectifiers(app_type: &AppType, provider: &Provider) -> bool {
    matches!(
        provider_kind_from_app_type_and_config(app_type, provider),
        ProviderKind::Claude | ProviderKind::ClaudeAuth
    )
}

pub(crate) fn provider_is_github_copilot_upstream(provider: &Provider, base_url: &str) -> bool {
    crate::proxy_core::api::transport::is_github_copilot_upstream(
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.provider_type.as_deref()),
        base_url,
    )
}

pub(crate) fn provider_is_github_copilot_stream_check_target(provider: &Provider) -> bool {
    let base_url = provider
        .settings_config
        .pointer("/env/ANTHROPIC_BASE_URL")
        .and_then(Value::as_str)
        .unwrap_or("");
    provider_is_github_copilot_upstream(provider, base_url)
}

pub(crate) fn provider_github_copilot_managed_account_id(provider: &Provider) -> Option<String> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.managed_account_id_for("github_copilot"))
}

pub(crate) fn provider_codex_oauth_managed_account_id(provider: &Provider) -> Option<String> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.managed_account_id_for("codex_oauth"))
}

pub(crate) fn provider_usage_script(provider: Option<&Provider>) -> Option<&UsageScript> {
    provider
        .and_then(|provider| provider.meta.as_ref())
        .and_then(|meta| meta.usage_script.as_ref())
}

pub(crate) fn provider_claude_env_settings(
    provider: &Provider,
) -> Option<&serde_json::Map<String, Value>> {
    provider
        .settings_config
        .get("env")
        .and_then(Value::as_object)
}

pub(crate) fn provider_launch_env_vars_for_app(
    provider: &Provider,
    app_type: &AppType,
) -> Vec<(String, String)> {
    launch_env_vars_from_provider_settings(&provider.settings_config, app_type)
}

pub(crate) fn provider_settings_have_proxy_placeholder_for_app(
    provider: &Provider,
    app_type: &AppType,
    placeholder: &str,
) -> bool {
    live_config_has_proxy_placeholder_for_app(app_type, &provider.settings_config, placeholder)
}

pub(crate) fn live_config_has_proxy_placeholder_for_app(
    app_type: &AppType,
    config: &Value,
    placeholder: &str,
) -> bool {
    match app_type {
        AppType::Claude => claude_live_config_has_proxy_placeholder(config, placeholder),
        AppType::Codex => codex_live_config_has_proxy_placeholder(config, placeholder),
        AppType::Gemini => gemini_live_config_has_proxy_placeholder(config, placeholder),
        _ => false,
    }
}

pub(crate) fn live_backup_snapshot_from_live_config(
    app_type: &AppType,
    config: &Value,
    placeholder: &str,
) -> Option<Value> {
    if live_config_has_proxy_placeholder_for_app(app_type, config, placeholder) {
        None
    } else {
        Some(config.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveTokenProviderSettingsIssue {
    InvalidProviderSettings,
}

pub(crate) fn provider_settings_with_live_token_sync(
    app_type: &AppType,
    live_config: &Value,
    provider_settings: &Value,
    placeholder: &str,
) -> Result<Option<Value>, LiveTokenProviderSettingsIssue> {
    match app_type {
        AppType::Claude => {
            let Some((token_key, token)) = claude_live_token_pair(live_config, placeholder) else {
                return Ok(None);
            };
            sync_claude_live_token_to_provider_settings(provider_settings, token_key, token)
                .map(Some)
        }
        AppType::Codex => {
            let Some(token) = codex_live_openai_api_key(live_config, placeholder) else {
                return Ok(None);
            };
            sync_section_token_to_provider_settings(
                provider_settings,
                "auth",
                "OPENAI_API_KEY",
                token,
            )
            .map(Some)
        }
        AppType::Gemini => {
            let Some(token) = gemini_live_api_key(live_config, placeholder) else {
                return Ok(None);
            };
            sync_section_token_to_provider_settings(
                provider_settings,
                "env",
                "GEMINI_API_KEY",
                token,
            )
            .map(Some)
        }
        _ => Ok(None),
    }
}

pub(crate) fn sync_provider_settings_with_live_token(
    app_type: &AppType,
    live_config: &Value,
    provider: &mut Provider,
    placeholder: &str,
) -> Result<bool, LiveTokenProviderSettingsIssue> {
    match provider_settings_with_live_token_sync(
        app_type,
        live_config,
        &provider.settings_config,
        placeholder,
    )? {
        Some(settings_config) => {
            provider.settings_config = settings_config;
            Ok(true)
        }
        None => Ok(false),
    }
}

const CLAUDE_TAKEOVER_TOKEN_ENV_KEYS: [&str; 4] = [
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "OPENROUTER_API_KEY",
    "OPENAI_API_KEY",
];

pub(crate) fn claude_live_config_has_proxy_placeholder(
    config: &Value,
    placeholder: &str,
) -> bool {
    let Some(env) = config.get("env").and_then(Value::as_object) else {
        return false;
    };

    CLAUDE_TAKEOVER_TOKEN_ENV_KEYS
        .into_iter()
        .any(|key| env.get(key).and_then(Value::as_str) == Some(placeholder))
}

pub(crate) fn is_local_proxy_url(url: &str) -> bool {
    let url = url.trim();
    if !url.starts_with("http://") {
        return false;
    }
    let rest = &url["http://".len()..];
    rest.starts_with("127.0.0.1")
        || rest.starts_with("localhost")
        || rest.starts_with("0.0.0.0")
        || rest.starts_with("[::1]")
        || rest.starts_with("[::]")
        || rest.starts_with("::1")
        || rest.starts_with("::")
}

pub(crate) fn remove_claude_takeover_env_fields_if_present<F>(
    config: &mut Value,
    placeholder: &str,
    is_local_proxy_url: F,
) -> Option<bool>
where
    F: Fn(&str) -> bool,
{
    let env = config.get_mut("env").and_then(Value::as_object_mut)?;
    let mut changed = false;

    for key in CLAUDE_TAKEOVER_TOKEN_ENV_KEYS {
        if env.get(key).and_then(Value::as_str) == Some(placeholder) {
            env.remove(key);
            changed = true;
        }
    }

    if env
        .get("ANTHROPIC_BASE_URL")
        .and_then(Value::as_str)
        .map(is_local_proxy_url)
        .unwrap_or(false)
    {
        env.remove("ANTHROPIC_BASE_URL");
        changed = true;
    }

    Some(changed)
}

pub(crate) fn codex_live_config_has_proxy_placeholder(
    config: &Value,
    placeholder: &str,
) -> bool {
    if config
        .get("auth")
        .and_then(Value::as_object)
        .and_then(|auth| auth.get("OPENAI_API_KEY"))
        .and_then(Value::as_str)
        == Some(placeholder)
    {
        return true;
    }

    config
        .get("config")
        .and_then(Value::as_str)
        .and_then(crate::codex_config::extract_codex_experimental_bearer_token)
        .as_deref()
        == Some(placeholder)
}

pub(crate) fn apply_codex_takeover_auth_placeholder_if_present(
    config: &mut Value,
    placeholder: &str,
) -> bool {
    let Some(auth) = config.get_mut("auth").and_then(Value::as_object_mut) else {
        return false;
    };

    auth.insert("OPENAI_API_KEY".to_string(), json!(placeholder));
    true
}

pub(crate) fn ensure_codex_takeover_auth_placeholder(
    config: &mut Value,
    placeholder: &str,
) -> bool {
    if apply_codex_takeover_auth_placeholder_if_present(config, placeholder) {
        return true;
    }

    let Some(root) = config.as_object_mut() else {
        return false;
    };

    root.insert(
        "auth".to_string(),
        json!({ "OPENAI_API_KEY": placeholder }),
    );
    true
}

pub(crate) fn remove_codex_takeover_auth_placeholder_if_present(
    config: &mut Value,
    placeholder: &str,
) -> bool {
    let Some(auth) = config.get_mut("auth").and_then(Value::as_object_mut) else {
        return false;
    };

    if auth.get("OPENAI_API_KEY").and_then(Value::as_str) != Some(placeholder) {
        return false;
    }

    auth.remove("OPENAI_API_KEY");
    true
}

pub(crate) fn remove_codex_takeover_config_placeholders_if_present<F>(
    config: &mut Value,
    placeholder: &str,
    is_local_proxy_url: F,
) -> Result<(), String>
where
    F: Fn(&str) -> bool,
{
    let Some(config_text) = config.get("config").and_then(Value::as_str) else {
        return Ok(());
    };

    let updated =
        crate::codex_config::remove_codex_toml_base_url_if(config_text, is_local_proxy_url);
    let updated =
        crate::codex_config::remove_codex_experimental_bearer_token_if(&updated, |token| {
            token == placeholder
        })
        .map_err(|e| e.to_string())?;
    config["config"] = json!(updated);

    Ok(())
}

pub(crate) fn codex_preserved_auth_live_config_text_if_proxy_placeholder(
    config: &Value,
    placeholder: &str,
    include_optional_catalog: bool,
) -> Result<Option<String>, String> {
    let Some(auth) = config
        .get("auth")
        .filter(|auth| codex_auth_value_has_proxy_placeholder(auth, placeholder))
    else {
        return Ok(None);
    };
    let Some(config_str) = config.get("config").and_then(Value::as_str) else {
        return Ok(None);
    };

    let prepared_config = if include_optional_catalog {
        crate::codex_config::prepare_codex_live_config_text_with_optional_catalog(
            config, config_str,
        )
        .map_err(|e| e.to_string())?
    } else {
        config_str.to_string()
    };

    crate::codex_config::prepare_codex_provider_live_config(auth, &prepared_config)
        .map(Some)
        .map_err(|e| e.to_string())
}

pub(crate) fn codex_preserved_auth_live_config_text_for_policy(
    config: &Value,
    placeholder: &str,
    preserve_auth: bool,
    include_optional_catalog: bool,
) -> Result<Option<String>, String> {
    if !preserve_auth {
        return Ok(None);
    }

    codex_preserved_auth_live_config_text_if_proxy_placeholder(
        config,
        placeholder,
        include_optional_catalog,
    )
}

pub(crate) fn codex_preserved_auth_live_config_text_for_configured_policy(
    config: &Value,
    placeholder: &str,
    include_optional_catalog: bool,
) -> Result<Option<String>, String> {
    codex_preserved_auth_live_config_text_for_policy(
        config,
        placeholder,
        crate::settings::preserve_codex_official_auth_on_switch(),
        include_optional_catalog,
    )
}

fn codex_auth_value_has_proxy_placeholder(auth: &Value, placeholder: &str) -> bool {
    auth.get("OPENAI_API_KEY").and_then(Value::as_str) == Some(placeholder)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodexLiveWriteProjection {
    WriteAuthAndConfig { auth: Value, config_text: String },
    DeleteAuthAndWriteConfig { config_text: String },
    WriteAuthOnly { auth: Value },
    WriteConfigOnly { config_text: String },
    Noop,
}

/// Classifies restored Codex live settings into host filesystem actions.
/// Snapshot backups may already carry a model catalog pointer, while
/// provider-derived backups may need inline `modelCatalog` projected first.
pub(crate) fn codex_live_write_projection(
    config: &Value,
) -> Result<CodexLiveWriteProjection, String> {
    let auth = config.get("auth").cloned();
    let config_text = config
        .get("config")
        .and_then(Value::as_str)
        .map(|config_text| {
            crate::codex_config::prepare_codex_live_config_text_with_optional_catalog(
                config,
                config_text,
            )
        })
        .transpose()
        .map_err(|e| e.to_string())?;

    Ok(match (auth, config_text) {
        (Some(auth), Some(config_text)) => {
            if auth.as_object().is_some_and(|obj| obj.is_empty()) {
                CodexLiveWriteProjection::DeleteAuthAndWriteConfig { config_text }
            } else {
                CodexLiveWriteProjection::WriteAuthAndConfig { auth, config_text }
            }
        }
        (Some(auth), None) => CodexLiveWriteProjection::WriteAuthOnly { auth },
        (None, Some(config_text)) => CodexLiveWriteProjection::WriteConfigOnly { config_text },
        (None, None) => CodexLiveWriteProjection::Noop,
    })
}

pub(crate) fn gemini_live_config_has_proxy_placeholder(
    config: &Value,
    placeholder: &str,
) -> bool {
    config
        .get("env")
        .and_then(Value::as_object)
        .and_then(|env| env.get("GEMINI_API_KEY"))
        .and_then(Value::as_str)
        == Some(placeholder)
}

pub(crate) fn apply_gemini_takeover_env_fields(
    config: &mut Value,
    proxy_url: &str,
    placeholder: &str,
) {
    if let Some(env) = config.get_mut("env").and_then(Value::as_object_mut) {
        env.insert("GOOGLE_GEMINI_BASE_URL".to_string(), json!(proxy_url));
        env.insert("GEMINI_API_KEY".to_string(), json!(placeholder));
        return;
    }

    config["env"] = json!({
        "GOOGLE_GEMINI_BASE_URL": proxy_url,
        "GEMINI_API_KEY": placeholder
    });
}

pub(crate) fn remove_gemini_takeover_env_fields_if_present<F>(
    config: &mut Value,
    placeholder: &str,
    is_local_proxy_url: F,
) -> Option<bool>
where
    F: Fn(&str) -> bool,
{
    let env = config.get_mut("env").and_then(Value::as_object_mut)?;
    let mut changed = false;

    if env.get("GEMINI_API_KEY").and_then(Value::as_str) == Some(placeholder) {
        env.remove("GEMINI_API_KEY");
        changed = true;
    }

    if env
        .get("GOOGLE_GEMINI_BASE_URL")
        .and_then(Value::as_str)
        .map(is_local_proxy_url)
        .unwrap_or(false)
    {
        env.remove("GOOGLE_GEMINI_BASE_URL");
        changed = true;
    }

    Some(changed)
}

pub(crate) fn live_takeover_config_matches_proxy_for_app(
    app_type: &AppType,
    config: &Value,
    proxy_url: &str,
    codex_proxy_base_url: &str,
    placeholder: &str,
) -> bool {
    match app_type {
        AppType::Claude => {
            claude_live_config_has_proxy_placeholder(config, placeholder)
                && live_env_base_url_matches(config, "ANTHROPIC_BASE_URL", proxy_url)
        }
        AppType::Codex => {
            codex_live_config_has_proxy_placeholder(config, placeholder)
                && config
                    .get("config")
                    .and_then(Value::as_str)
                    .is_some_and(|config_text| {
                        codex_config_has_base_url_matching(config_text, |url| {
                            proxy_urls_match(url, codex_proxy_base_url)
                        })
                    })
        }
        AppType::Gemini => {
            gemini_live_config_has_proxy_placeholder(config, placeholder)
                && live_env_base_url_matches(config, "GOOGLE_GEMINI_BASE_URL", proxy_url)
        }
        _ => false,
    }
}

fn live_env_base_url_matches(config: &Value, key: &str, expected: &str) -> bool {
    config
        .get("env")
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .is_some_and(|url| proxy_urls_match(url, expected))
}

fn proxy_urls_match(actual: &str, expected: &str) -> bool {
    actual.trim().trim_end_matches('/') == expected.trim().trim_end_matches('/')
}

fn codex_config_has_base_url_matching(
    config_text: &str,
    predicate: impl Fn(&str) -> bool,
) -> bool {
    let Ok(doc) = toml::from_str::<toml::Value>(config_text) else {
        return false;
    };

    let active_provider = doc
        .get("model_provider")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty());

    if let Some(provider_id) = active_provider {
        if doc
            .get("model_providers")
            .and_then(|value| value.get(provider_id))
            .and_then(|value| value.get("base_url"))
            .and_then(|value| value.as_str())
            .is_some_and(&predicate)
        {
            return true;
        }
    }

    doc.get("base_url")
        .and_then(|value| value.as_str())
        .is_some_and(predicate)
}

fn claude_live_token_pair<'a>(
    live_config: &'a Value,
    placeholder: &str,
) -> Option<(&'static str, &'a str)> {
    let env = live_config.get("env").and_then(Value::as_object)?;

    CLAUDE_TAKEOVER_TOKEN_ENV_KEYS
        .into_iter()
        .find_map(|key| {
            non_placeholder_trimmed_string(env.get(key), placeholder).map(|token| (key, token))
        })
}

fn codex_live_openai_api_key<'a>(live_config: &'a Value, placeholder: &str) -> Option<&'a str> {
    non_placeholder_trimmed_string(
        live_config
            .get("auth")
            .and_then(Value::as_object)
            .and_then(|auth| auth.get("OPENAI_API_KEY")),
        placeholder,
    )
}

fn gemini_live_api_key<'a>(live_config: &'a Value, placeholder: &str) -> Option<&'a str> {
    non_placeholder_trimmed_string(
        live_config
            .get("env")
            .and_then(Value::as_object)
            .and_then(|env| env.get("GEMINI_API_KEY")),
        placeholder,
    )
}

fn non_placeholder_trimmed_string<'a>(
    value: Option<&'a Value>,
    placeholder: &str,
) -> Option<&'a str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty() && *token != placeholder)
}

fn sync_claude_live_token_to_provider_settings(
    provider_settings: &Value,
    token_key: &'static str,
    token: &str,
) -> Result<Value, LiveTokenProviderSettingsIssue> {
    let mut config = provider_settings.clone();

    if let Some(env_obj) = config.get_mut("env").and_then(Value::as_object_mut) {
        if token_key == "ANTHROPIC_AUTH_TOKEN" || token_key == "ANTHROPIC_API_KEY" {
            let mut updated = false;
            if env_obj.contains_key("ANTHROPIC_AUTH_TOKEN") {
                env_obj.insert("ANTHROPIC_AUTH_TOKEN".to_string(), json!(token));
                updated = true;
            }
            if env_obj.contains_key("ANTHROPIC_API_KEY") {
                env_obj.insert("ANTHROPIC_API_KEY".to_string(), json!(token));
                updated = true;
            }
            if !updated {
                env_obj.insert(token_key.to_string(), json!(token));
            }
        } else {
            env_obj.insert(token_key.to_string(), json!(token));
        }

        return Ok(config);
    }

    sync_section_token_to_provider_settings(provider_settings, "env", token_key, token)
}

fn sync_section_token_to_provider_settings(
    provider_settings: &Value,
    section_key: &str,
    token_key: &str,
    token: &str,
) -> Result<Value, LiveTokenProviderSettingsIssue> {
    let mut config = provider_settings.clone();

    if let Some(section_obj) = config.get_mut(section_key).and_then(Value::as_object_mut) {
        section_obj.insert(token_key.to_string(), json!(token));
        return Ok(config);
    }

    if config.is_null() {
        config = json!({});
    }

    let Some(root) = config.as_object_mut() else {
        return Err(LiveTokenProviderSettingsIssue::InvalidProviderSettings);
    };

    let mut section = Map::new();
    section.insert(token_key.to_string(), json!(token));
    root.insert(section_key.to_string(), Value::Object(section));

    Ok(config)
}

fn launch_env_vars_from_provider_settings(
    config: &Value,
    app_type: &AppType,
) -> Vec<(String, String)> {
    let mut env_vars = Vec::new();

    let Some(obj) = config.as_object() else {
        return env_vars;
    };

    if let Some(env) = obj.get("env").and_then(Value::as_object) {
        for (key, value) in env {
            if let Some(str_val) = value.as_str() {
                env_vars.push((key.clone(), str_val.to_string()));
            }
        }

        let base_url_key = match app_type {
            AppType::Claude | AppType::ClaudeDesktop => Some("ANTHROPIC_BASE_URL"),
            AppType::Gemini => Some("GOOGLE_GEMINI_BASE_URL"),
            _ => None,
        };

        if let Some(key) = base_url_key {
            if let Some(url_str) = env.get(key).and_then(Value::as_str) {
                env_vars.push((key.to_string(), url_str.to_string()));
            }
        }
    }

    if *app_type == AppType::Codex {
        if let Some(auth) = obj.get("auth").and_then(Value::as_str) {
            env_vars.push(("OPENAI_API_KEY".to_string(), auth.to_string()));
        }
    }

    if *app_type == AppType::Gemini {
        if let Some(api_key) = obj.get("api_key").and_then(Value::as_str) {
            env_vars.push(("GEMINI_API_KEY".to_string(), api_key.to_string()));
        }
    }

    env_vars
}

pub(crate) fn provider_claude_models_are_claude_safe(provider: &Provider) -> bool {
    let Some(env) = provider_claude_env_settings(provider) else {
        return true;
    };

    [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
    ]
    .into_iter()
    .filter_map(|key| env.get(key).and_then(Value::as_str))
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .all(crate::claude_desktop_config::is_claude_safe_model_id)
}

pub(crate) fn provider_claude_desktop_routes_support_1m_by_default(
    provider: &Provider,
) -> bool {
    !matches!(
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.provider_type.as_deref()),
        Some("github_copilot") | Some("codex_oauth")
    )
}

pub(crate) fn provider_claude_desktop_proxy_has_base_url_and_key(provider: &Provider) -> bool {
    let settings = &provider.settings_config;
    let env = settings.get("env");
    let has_base_url = env
        .and_then(|value| value.get("ANTHROPIC_BASE_URL"))
        .or_else(|| settings.get("base_url"))
        .or_else(|| settings.get("baseURL"))
        .or_else(|| settings.get("apiEndpoint"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    if provider_is_typed_managed_oauth_proxy(provider) {
        return has_base_url;
    }

    let has_key = env
        .and_then(|value| {
            [
                "ANTHROPIC_AUTH_TOKEN",
                "ANTHROPIC_API_KEY",
                "OPENROUTER_API_KEY",
                "OPENAI_API_KEY",
                "GEMINI_API_KEY",
            ]
            .into_iter()
            .find_map(|key| value.get(key))
        })
        .or_else(|| settings.get("apiKey"))
        .or_else(|| settings.get("api_key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    has_base_url && has_key
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopDirectProviderValidationIssue {
    SettingsNotObject,
    ApiFormatUnsupported,
    ProxyModeUnsupported,
    ManagedProviderTypeUnsupported,
    FullUrlUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProxyProviderConfigValidationIssue {
    SettingsNotObject,
    ApiFormatUnsupported(String),
}

pub(crate) fn provider_claude_desktop_direct_validation_issue(
    provider: &Provider,
) -> Option<ClaudeDesktopDirectProviderValidationIssue> {
    if !provider.settings_config.is_object() {
        return Some(ClaudeDesktopDirectProviderValidationIssue::SettingsNotObject);
    }

    let meta = provider.meta.as_ref()?;
    if let Some(api_format) = meta.api_format.as_deref() {
        if !api_format.trim().is_empty() && api_format != "anthropic" {
            return Some(ClaudeDesktopDirectProviderValidationIssue::ApiFormatUnsupported);
        }
    }

    if matches!(
        meta.claude_desktop_mode.as_ref(),
        Some(crate::provider::ClaudeDesktopMode::Proxy)
    ) {
        return Some(ClaudeDesktopDirectProviderValidationIssue::ProxyModeUnsupported);
    }

    if matches!(
        meta.provider_type.as_deref(),
        Some("github_copilot") | Some("codex_oauth")
    ) {
        return Some(ClaudeDesktopDirectProviderValidationIssue::ManagedProviderTypeUnsupported);
    }

    if meta.is_full_url == Some(true) {
        return Some(ClaudeDesktopDirectProviderValidationIssue::FullUrlUnsupported);
    }

    None
}

pub(crate) fn provider_claude_desktop_proxy_config_validation_issue(
    provider: &Provider,
) -> Option<ClaudeDesktopProxyProviderConfigValidationIssue> {
    if !provider.settings_config.is_object() {
        return Some(ClaudeDesktopProxyProviderConfigValidationIssue::SettingsNotObject);
    }

    let meta = provider.meta.as_ref()?;
    if let Some(api_format) = meta.api_format.as_deref() {
        if !matches!(
            api_format,
            "" | "anthropic" | "openai_chat" | "openai_responses" | "gemini_native"
        ) {
            return Some(
                ClaudeDesktopProxyProviderConfigValidationIssue::ApiFormatUnsupported(
                    api_format.to_string(),
                ),
            );
        }
    }

    None
}

pub(crate) fn provider_should_normalize_mimo_anthropic_thinking_history(
    provider: &Provider,
    upstream_model: &str,
) -> bool {
    if !provider_uses_anthropic_messages_format(provider) {
        return false;
    }

    is_mimo_identifier(upstream_model) || provider_has_mimo_endpoint(provider)
}

fn provider_uses_anthropic_messages_format(provider: &Provider) -> bool {
    let api_format = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.api_format.as_deref())
        .or_else(|| provider.settings_config.get("api_format").and_then(Value::as_str))
        .map(str::trim)
        .unwrap_or("anthropic");

    api_format.is_empty() || api_format == "anthropic"
}

fn provider_has_mimo_endpoint(provider: &Provider) -> bool {
    let settings = &provider.settings_config;
    [
        settings
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
            .and_then(Value::as_str),
        settings.get("base_url").and_then(Value::as_str),
        settings.get("baseURL").and_then(Value::as_str),
        settings.get("apiEndpoint").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .any(is_mimo_identifier)
}

fn is_mimo_identifier(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("mimo") || value.contains("xiaomimimo")
}

fn provider_is_typed_managed_oauth_proxy(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        .is_some_and(|provider_type| matches!(provider_type, "github_copilot" | "codex_oauth"))
}

pub(crate) fn provider_stream_check_test_config(
    provider: &Provider,
) -> Option<&ProviderTestConfig> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.test_config.as_ref())
        .filter(|config| config.enabled)
}

pub(crate) fn provider_is_full_url(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.is_full_url)
        .unwrap_or(false)
}

pub(crate) fn provider_custom_user_agent_header(
    provider: &Provider,
    is_copilot: bool,
) -> Option<http::HeaderValue> {
    if is_copilot {
        return None;
    }

    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.custom_user_agent_header().ok().flatten())
}

pub(crate) fn provider_bedrock_env_flag(provider: &Provider) -> Option<&str> {
    bedrock_env_flag_from_provider_settings(&provider.settings_config)
}

pub(crate) use crate::proxy_core::api::usage::{
    is_placeholder_pricing_model,
    non_streaming_response_usage_record_from_body_with_request_id_fallback,
    streaming_response_usage_record_with_optional_outbound_model,
    usage_record_with_route_context, usage_route_context_from_selection,
};

#[derive(Debug, Clone)]
pub(crate) struct ResponseUsageProviderFacts {
    provider_id: String,
    provider_kind: Option<ProviderKind>,
    app: AppKind,
}

pub(crate) fn response_usage_provider_facts(
    provider: &Provider,
    app_type: &str,
) -> ResponseUsageProviderFacts {
    ResponseUsageProviderFacts {
        provider_id: provider.id.clone(),
        provider_kind: provider_kind_from_provider(provider),
        app: AppKind::from(app_type),
    }
}

pub(crate) fn fallback_response_usage_provider_facts(
    provider_id: String,
    app_type: &str,
) -> ResponseUsageProviderFacts {
    ResponseUsageProviderFacts {
        provider_id,
        provider_kind: None,
        app: AppKind::from(app_type),
    }
}

pub(crate) fn response_usage_provider_facts_from_optional(
    provider: Option<&Provider>,
    app_type: &str,
    tag: &str,
    phase: UsageSelectedProviderMissingPhase,
) -> Result<ResponseUsageProviderFacts, String> {
    provider
        .map(|provider| response_usage_provider_facts(provider, app_type))
        .ok_or_else(|| usage_selected_provider_missing_log_message(tag, phase))
}

pub(crate) struct StreamingResponseUsageContext<'a> {
    pub(crate) events: &'a [Value],
    pub(crate) stream_parser: fn(&[Value]) -> Option<TokenUsage>,
    pub(crate) model_extractor: fn(&[Value], &str) -> String,
    pub(crate) provider_facts: &'a ResponseUsageProviderFacts,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) first_token_ms: Option<u64>,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

pub(crate) struct NonStreamingResponseUsageContext<'a> {
    pub(crate) body: &'a [u8],
    pub(crate) response_parser: fn(&Value) -> Option<TokenUsage>,
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) app_type: &'a str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

pub(crate) struct ForwardErrorUsageContext<'a> {
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) fallback_provider_id: &'a str,
    pub(crate) app_type: &'a str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) status_code: u16,
    pub(crate) error_message: String,
    pub(crate) latency_ms: u64,
    pub(crate) is_streaming: bool,
    pub(crate) session_id: &'a str,
}

pub(crate) struct TransformedResponseUsageContext<'a> {
    pub(crate) body: &'a Value,
    pub(crate) format: TransformedResponseUsageFormat,
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) tag: &'a str,
    pub(crate) app_type: &'a str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

pub(crate) struct TransformedStreamingResponseUsageContext<'a> {
    pub(crate) events: &'a [Value],
    pub(crate) format: TransformedResponseUsageFormat,
    pub(crate) provider_facts: &'a ResponseUsageProviderFacts,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) first_token_ms: Option<u64>,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn success_usage_record_from_app_type_with_request_id_fallback(
    provider_id: &str,
    provider_kind: Option<ProviderKind>,
    app_type: &str,
    model: &str,
    request_model: &str,
    outbound_model: &str,
    usage: TokenUsage,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    is_streaming: bool,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> UsageRecord {
    success_usage_record_with_request_id_fallback(
        provider_id,
        provider_kind,
        AppKind::from(app_type),
        model,
        request_model,
        outbound_model,
        usage,
        latency_ms,
        first_token_ms,
        is_streaming,
        status_code,
        session_id,
        request_id_fallback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn error_usage_record_from_provider_facts_with_request_id_fallback(
    provider_facts: &ResponseUsageProviderFacts,
    request_model: &str,
    outbound_model: Option<&str>,
    status_code: u16,
    error_message: String,
    latency_ms: u64,
    is_streaming: bool,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> UsageRecord {
    error_usage_record_with_request_id_fallback(
        &provider_facts.provider_id,
        provider_facts.provider_kind.clone(),
        provider_facts.app.clone(),
        request_model,
        outbound_model,
        status_code,
        error_message,
        latency_ms,
        is_streaming,
        session_id,
        request_id_fallback,
    )
}

pub(crate) fn forward_error_usage_record_from_response_context(
    context: ForwardErrorUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> UsageRecord {
    let provider_facts = context
        .provider
        .map(|provider| response_usage_provider_facts(provider, context.app_type))
        .unwrap_or_else(|| {
            fallback_response_usage_provider_facts(
                context.fallback_provider_id.to_string(),
                context.app_type,
            )
        });
    let record = error_usage_record_from_provider_facts_with_request_id_fallback(
        &provider_facts,
        context.request_model,
        context.outbound_model,
        context.status_code,
        context.error_message,
        context.latency_ms,
        context.is_streaming,
        Some(context.session_id.to_string()),
        request_id_fallback,
    );
    usage_record_with_route_context(record, context.route_context)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn streaming_response_usage_record_from_provider_facts(
    events: &[Value],
    stream_parser: fn(&[Value]) -> Option<TokenUsage>,
    model_extractor: fn(&[Value], &str) -> String,
    provider_facts: &ResponseUsageProviderFacts,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> StreamingResponseUsageRecord {
    streaming_response_usage_record_with_optional_outbound_model(
        events,
        stream_parser,
        model_extractor,
        &provider_facts.provider_id,
        provider_facts.provider_kind.clone(),
        provider_facts.app.clone(),
        request_model,
        outbound_model,
        latency_ms,
        first_token_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

pub(crate) fn streaming_response_usage_record_from_response_context(
    context: StreamingResponseUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> StreamingResponseUsageRecord {
    let mut output = streaming_response_usage_record_from_provider_facts(
        context.events,
        context.stream_parser,
        context.model_extractor,
        context.provider_facts,
        context.request_model,
        context.outbound_model,
        context.latency_ms,
        context.first_token_ms,
        context.status_code,
        Some(context.session_id.to_string()),
        request_id_fallback,
    );
    output.record = usage_record_with_route_context(output.record, context.route_context);
    output
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn transformed_response_usage_record_from_provider_facts_with_request_id_fallback(
    body: &Value,
    format: TransformedResponseUsageFormat,
    provider_facts: &ResponseUsageProviderFacts,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> Option<UsageRecord> {
    transformed_response_usage_record_with_request_id_fallback(
        body,
        format,
        &provider_facts.provider_id,
        provider_facts.provider_kind.clone(),
        provider_facts.app.clone(),
        request_model,
        outbound_model,
        latency_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn transformed_streaming_response_usage_record_from_provider_facts_with_request_id_fallback(
    events: &[Value],
    format: TransformedResponseUsageFormat,
    provider_facts: &ResponseUsageProviderFacts,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> Option<UsageRecord> {
    transformed_streaming_response_usage_record_with_request_id_fallback(
        events,
        format,
        &provider_facts.provider_id,
        provider_facts.provider_kind.clone(),
        provider_facts.app.clone(),
        request_model,
        outbound_model,
        latency_ms,
        first_token_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

pub(crate) fn transformed_response_usage_record_from_response_context(
    context: TransformedResponseUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> Result<Option<UsageRecord>, String> {
    let provider_facts = response_usage_provider_facts_from_optional(
        context.provider,
        context.app_type,
        context.tag,
        UsageSelectedProviderMissingPhase::TransformedResponse,
    )?;
    Ok(
        transformed_response_usage_record_from_provider_facts_with_request_id_fallback(
            context.body,
            context.format,
            &provider_facts,
            context.request_model,
            context.outbound_model,
            context.latency_ms,
            context.status_code,
            Some(context.session_id.to_string()),
            request_id_fallback,
        )
        .map(|record| usage_record_with_route_context(record, context.route_context)),
    )
}

pub(crate) fn transformed_streaming_response_usage_record_from_response_context(
    context: TransformedStreamingResponseUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> Option<UsageRecord> {
    transformed_streaming_response_usage_record_from_provider_facts_with_request_id_fallback(
        context.events,
        context.format,
        context.provider_facts,
        context.request_model,
        context.outbound_model,
        context.latency_ms,
        context.first_token_ms,
        context.status_code,
        Some(context.session_id.to_string()),
        request_id_fallback,
    )
    .map(|record| usage_record_with_route_context(record, context.route_context))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn non_streaming_response_usage_record_from_provider_body_with_request_id_fallback(
    body: &[u8],
    response_parser: fn(&Value) -> Option<TokenUsage>,
    provider: &Provider,
    app_type: &str,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> NonStreamingResponseUsageRecord {
    let provider_facts = response_usage_provider_facts(provider, app_type);
    non_streaming_response_usage_record_from_body_with_request_id_fallback(
        body,
        response_parser,
        &provider_facts.provider_id,
        provider_facts.provider_kind,
        provider_facts.app,
        request_model,
        outbound_model,
        latency_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

pub(crate) fn non_streaming_response_usage_record_from_response_context(
    context: NonStreamingResponseUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> Result<NonStreamingResponseUsageRecord, String> {
    let provider = context
        .provider
        .ok_or_else(|| selected_provider_not_applied_message(context.app_type))?;
    let mut output = non_streaming_response_usage_record_from_provider_body_with_request_id_fallback(
        context.body,
        context.response_parser,
        provider,
        context.app_type,
        context.request_model,
        context.outbound_model,
        context.latency_ms,
        context.status_code,
        Some(context.session_id.to_string()),
        request_id_fallback,
    );
    output.record = usage_record_with_route_context(output.record, context.route_context);
    Ok(output)
}

pub(crate) fn usage_record_pricing_model(
    record: &UsageRecord,
    pricing_model_source: &str,
) -> String {
    crate::proxy_core::api::usage::resolve_usage_record_pricing_models(record, pricing_model_source)
        .pricing_model
}

pub(crate) fn usage_pricing_config_lookup_from_record(
    record: &UsageRecord,
) -> UsagePricingConfigLookup {
    UsagePricingConfigLookup {
        provider_id: record.provider_id.clone(),
        app_type: record.app.as_str().to_string(),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RequestContextRouteUpdate {
    pub(crate) outbound_model: Option<String>,
    pub(crate) usage_route_context: UsageRouteContext,
    pub(crate) provider: Provider,
}

#[derive(Debug)]
pub(crate) enum RequestContextRouteUpdateError<E> {
    ProviderLoad(E),
    ProviderMissing(String),
}

pub(crate) fn request_context_route_update_from_proxy_result(
    app_type: &AppType,
    provider: &Provider,
    result: &ProxyResult,
) -> RequestContextRouteUpdate {
    RequestContextRouteUpdate {
        outbound_model: result.outbound_model.clone(),
        usage_route_context: usage_route_context_from_selection(&result.selected_route),
        provider: ForwardAttempt::from_core_selection(app_type, provider, &result.selected_route)
            .provider()
            .clone(),
    }
}

pub(crate) fn request_context_route_update_from_proxy_result_source<E>(
    app_type: &AppType,
    app_type_str: &str,
    result: &ProxyResult,
    source_name: &str,
    load_provider: impl FnOnce(&str, &str) -> Result<Option<Provider>, E>,
) -> Result<RequestContextRouteUpdate, RequestContextRouteUpdateError<E>> {
    let provider_id = result.selected_route.provider.id.as_str();
    let provider = load_provider(provider_id, app_type_str)
        .map_err(RequestContextRouteUpdateError::ProviderLoad)?;
    let Some(provider) = provider else {
        return Err(RequestContextRouteUpdateError::ProviderMissing(
            selected_provider_missing_from_source_message(provider_id, source_name),
        ));
    };

    Ok(request_context_route_update_from_proxy_result(
        app_type, &provider, result,
    ))
}

pub(crate) fn usage_record_to_request_log(
    record: &UsageRecord,
    pricing_model_source: &str,
    pricing: Option<&ModelPricing>,
    multiplier: Decimal,
    fallback_request_id: impl FnOnce() -> String,
) -> UsageRequestLogProjection {
    let projection = crate::proxy_core::api::usage::usage_request_log_projection(
        record,
        pricing_model_source,
        pricing,
        multiplier,
        fallback_request_id,
    );
    let fields = projection.fields;
    UsageRequestLogProjection {
        log: RequestLog {
            request_id: fields.request_id,
            provider_id: fields.provider_id,
            app_type: fields.app_type,
            model: fields.model,
            request_model: fields.request_model,
            pricing_model: fields.pricing_model,
            usage: fields.usage,
            cost: fields.cost,
            latency_ms: fields.latency_ms,
            first_token_ms: fields.first_token_ms,
            status_code: fields.status_code,
            error_message: fields.error_message,
            session_id: fields.session_id,
            provider_type: fields.provider_type,
            channel_id: fields.channel_id,
            channel_name: fields.channel_name,
            route_group: fields.route_group,
            is_streaming: fields.is_streaming,
            cost_multiplier: fields.cost_multiplier,
        },
        missing_pricing_warning_message: projection.missing_pricing_warning_message,
    }
}

pub(crate) fn log_usage_request_projection_warnings(projection: &UsageRequestLogProjection) {
    if let Some(message) = projection.missing_pricing_warning_message.as_ref() {
        log::warn!("{message}");
    }
}

pub(crate) async fn record_usage_in_db_source(
    db: &Database,
    record: UsageRecord,
) -> ProxyCoreResult<()> {
    let logger = UsageLogger::new(db);
    let lookup = usage_pricing_config_lookup_from_record(&record);
    let (multiplier, pricing_model_source) = logger
        .resolve_pricing_config(&lookup.provider_id, &lookup.app_type)
        .await;
    let pricing_model = usage_record_pricing_model(&record, &pricing_model_source);
    let pricing = logger
        .get_model_pricing(&pricing_model)
        .map_err(|error| usage_error("load model pricing", error))?;
    let projection = usage_record_to_request_log(
        &record,
        &pricing_model_source,
        pricing.as_ref(),
        multiplier,
        || uuid::Uuid::new_v4().to_string(),
    );

    log_usage_request_projection_warnings(&projection);

    logger
        .log_request(&projection.log)
        .map_err(|error| usage_error("record usage", error))
}

pub(crate) use crate::proxy_core::api::model_catalog::{
    claude_takeover_client_model_for_upstream, claude_takeover_default_display_name,
};

/// 代理接管模式下需要从 Claude Live 配置中移除的"模型覆盖"字段。
///
/// 原因：接管模式下 `*_MODEL` 必须由 CC Switch 写成稳定的 Claude 角色别名，
/// 再由本地代理映射到当前供应商真实模型；`*_MODEL_NAME` 也需要同步接管，
/// 否则 Claude Code 模型菜单会残留上一个供应商的显示名称。
const CLAUDE_MODEL_OVERRIDE_ENV_KEYS: [&str; 9] = [
    "ANTHROPIC_MODEL",
    "ANTHROPIC_REASONING_MODEL", // legacy: 已废弃，但旧配置可能残留
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
    // Legacy key (已废弃)：历史版本使用该字段区分 small/fast 模型
    "ANTHROPIC_SMALL_FAST_MODEL",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaudeTakeoverAuthPolicy {
    PreserveExistingOrAuthToken,
    ManagedAccount { keep_auth_token: bool },
}

const CLAUDE_TAKEOVER_HAIKU_MODEL: &str = "claude-haiku-4-5";
const CLAUDE_TAKEOVER_SONNET_MODEL: &str = "claude-sonnet-4-6";
const CLAUDE_TAKEOVER_OPUS_MODEL: &str = "claude-opus-4-8";

pub(crate) fn provider_claude_takeover_model_fields(
    provider: &Provider,
) -> Vec<(&'static str, String)> {
    claude_takeover_model_fields_from_settings(&provider.settings_config)
}

pub(crate) fn claude_takeover_model_fields_from_settings(
    config: &Value,
) -> Vec<(&'static str, String)> {
    let Some(env) = config.get("env").and_then(Value::as_object) else {
        return Vec::new();
    };

    let default_model = claude_takeover_env_string(env, "ANTHROPIC_MODEL");
    let small_fast_model = claude_takeover_env_string(env, "ANTHROPIC_SMALL_FAST_MODEL");
    let haiku_model = claude_takeover_env_string(env, "ANTHROPIC_DEFAULT_HAIKU_MODEL")
        .or(small_fast_model)
        .or(default_model);
    let sonnet_model = claude_takeover_env_string(env, "ANTHROPIC_DEFAULT_SONNET_MODEL")
        .or(default_model)
        .or(small_fast_model);
    let opus_model = claude_takeover_env_string(env, "ANTHROPIC_DEFAULT_OPUS_MODEL")
        .or(default_model)
        .or(small_fast_model);

    let mut fields = Vec::with_capacity(6);
    push_claude_takeover_role_fields(
        &mut fields,
        env,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
        CLAUDE_TAKEOVER_HAIKU_MODEL,
        false,
        haiku_model,
    );
    push_claude_takeover_role_fields(
        &mut fields,
        env,
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
        CLAUDE_TAKEOVER_SONNET_MODEL,
        true,
        sonnet_model,
    );
    push_claude_takeover_role_fields(
        &mut fields,
        env,
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
        CLAUDE_TAKEOVER_OPUS_MODEL,
        true,
        opus_model,
    );
    fields
}

pub(crate) fn apply_claude_takeover_fields_for_provider(
    config: &mut Value,
    proxy_url: &str,
    placeholder: &str,
    provider: &Provider,
) {
    let uses_managed_account = provider_uses_managed_account_auth(provider);
    let auth_policy = if uses_managed_account {
        // Codex 系（含仅凭 base_url 识别、无 provider_type meta 的）必须保留
        // ANTHROPIC_AUTH_TOKEN 占位符：Claude Code 缺该键会弹登录提示（#3784）。
        // Copilot 维持仅 API_KEY 占位，避免与 /login 管理的 key 冲突（#1049）。
        ClaudeTakeoverAuthPolicy::ManagedAccount {
            keep_auth_token: !provider_is_github_copilot(provider),
        }
    } else {
        ClaudeTakeoverAuthPolicy::PreserveExistingOrAuthToken
    };
    // Copilot/Codex 接管时 live config 可能还是旧供应商；显示模型必须跟随目标 provider。
    let takeover_model_fields = if uses_managed_account {
        provider_claude_takeover_model_fields(provider)
    } else {
        claude_takeover_model_fields_from_settings(config)
    };

    apply_claude_takeover_fields_with_policy_and_models(
        config,
        proxy_url,
        placeholder,
        auth_policy,
        takeover_model_fields,
    );
}

pub(crate) fn apply_claude_takeover_fields_with_policy(
    config: &mut Value,
    proxy_url: &str,
    placeholder: &str,
    auth_policy: ClaudeTakeoverAuthPolicy,
) {
    // 必须在 remove/insert 前 snapshot：避免读到自己刚写入的接管别名。
    let takeover_model_fields = claude_takeover_model_fields_from_settings(config);

    apply_claude_takeover_fields_with_policy_and_models(
        config,
        proxy_url,
        placeholder,
        auth_policy,
        takeover_model_fields,
    );
}

pub(crate) fn apply_claude_takeover_fields_with_policy_and_models(
    config: &mut Value,
    proxy_url: &str,
    placeholder: &str,
    auth_policy: ClaudeTakeoverAuthPolicy,
    takeover_model_fields: Vec<(&'static str, String)>,
) {
    if !config.is_object() {
        *config = json!({});
    }

    let root = config
        .as_object_mut()
        .expect("Claude config should be normalized to an object");
    let env = root.entry("env".to_string()).or_insert_with(|| json!({}));
    if !env.is_object() {
        *env = json!({});
    }

    let env = env
        .as_object_mut()
        .expect("Claude env should be normalized to an object");
    env.insert("ANTHROPIC_BASE_URL".to_string(), json!(proxy_url));

    for key in CLAUDE_MODEL_OVERRIDE_ENV_KEYS {
        env.remove(key);
    }

    for (key, value) in takeover_model_fields {
        env.insert(key.to_string(), Value::String(value));
    }

    let token_keys = [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "OPENROUTER_API_KEY",
        "OPENAI_API_KEY",
    ];

    match auth_policy {
        ClaudeTakeoverAuthPolicy::PreserveExistingOrAuthToken => {
            let mut replaced_any = false;
            for key in token_keys {
                if env.contains_key(key) {
                    env.insert(key.to_string(), json!(placeholder));
                    replaced_any = true;
                }
            }

            if !replaced_any {
                env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), json!(placeholder));
            }
        }
        ClaudeTakeoverAuthPolicy::ManagedAccount { keep_auth_token } => {
            for key in token_keys {
                env.remove(key);
            }
            env.insert("ANTHROPIC_API_KEY".to_string(), json!(placeholder));
            if keep_auth_token {
                // 无条件注入而非"已存在才保留"：热切换路径传入的是 provider
                // settings（预设不含该键），且旧版接管已把存量用户 live 中的键删光。
                env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), json!(placeholder));
            }
        }
    }
}

fn push_claude_takeover_role_fields(
    fields: &mut Vec<(&'static str, String)>,
    env: &Map<String, Value>,
    model_key: &'static str,
    name_key: &'static str,
    takeover_model: &'static str,
    supports_one_m: bool,
    upstream_model: Option<&str>,
) {
    let Some(upstream_model) = upstream_model else {
        return;
    };

    fields.push((
        model_key,
        claude_takeover_client_model_for_upstream(
            takeover_model,
            supports_one_m,
            upstream_model,
        ),
    ));

    let display_name = claude_takeover_env_string(env, name_key)
        .map(str::to_string)
        .unwrap_or_else(|| claude_takeover_default_display_name(upstream_model));
    if !display_name.is_empty() {
        fields.push((name_key, display_name));
    }
}

fn claude_takeover_env_string<'a>(
    env: &'a Map<String, Value>,
    key: &str,
) -> Option<&'a str> {
    env.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreModelRoute {
    fn to_proxy_core_model_route(&self) -> ModelRoute;
}

impl ToProxyCoreModelRoute for ProxyChannelModelRecord {
    fn to_proxy_core_model_route(&self) -> ModelRoute {
        model_route_from_input(self.to_proxy_core_model_route_input())
    }
}

impl ProxyChannelModelRecord {
    fn to_proxy_core_model_route_input(&self) -> ModelRouteInput {
        ModelRouteInput {
            public_model: self.public_model.clone(),
            upstream_model: self.upstream_model.clone(),
            capabilities: self.capabilities.clone(),
            pricing_model: self.pricing_model.clone(),
            request_overrides: self.request_overrides.clone(),
            response_overrides: self.response_overrides.clone(),
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord;
}

impl ToProxyCoreChannelModelRecord for ProxyChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord {
        channel_model_record_from_input(self.to_proxy_core_channel_model_record_input())
    }
}

impl ProxyChannelModelRecord {
    fn to_proxy_core_channel_model_record_input(&self) -> ChannelModelRecordInput {
        ChannelModelRecordInput {
            channel_id: self.channel_id.clone(),
            public_model: self.public_model.clone(),
            upstream_model: self.upstream_model.clone(),
            capabilities: self.capabilities.clone(),
            pricing_model: self.pricing_model.clone(),
            request_overrides: self.request_overrides.clone(),
            response_overrides: self.response_overrides.clone(),
        }
    }
}

pub(crate) fn proxy_channel_model_records_to_core(
    models: Vec<ProxyChannelModelRecord>,
) -> Vec<ChannelModelRecord> {
    models
        .iter()
        .map(ProxyChannelModelRecord::to_proxy_core_channel_model_record)
        .collect()
}

pub(crate) fn channel_model_records_from_db_source(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<Option<Vec<ChannelModelRecord>>> {
    let channel_exists = db
        .get_proxy_channel(channel_id)
        .map_err(|error| app_error("get channel for model records", error))?
        .is_some();

    if !channel_exists {
        return Ok(None);
    }

    let models = db
        .list_proxy_channel_models(channel_id)
        .map_err(|error| app_error("list channel model records", error))?;
    Ok(Some(proxy_channel_model_records_to_core(models)))
}

pub(crate) fn replace_channel_model_records_from_db_source(
    db: &Database,
    channel_id: &str,
    request: ProxyChannelModelsReplaceRequest,
) -> ProxyCoreResult<Option<Vec<ChannelModelRecord>>> {
    let models = db
        .replace_proxy_channel_models(channel_id, request)
        .map_err(|error| app_write_error("replace channel model records", error))?;
    Ok(models.map(proxy_channel_model_records_to_core))
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelRecord {
    fn to_proxy_core_channel_record(&self) -> ChannelRecord;
}

impl ToProxyCoreChannelRecord for ProxyChannelRecord {
    fn to_proxy_core_channel_record(&self) -> ChannelRecord {
        channel_record_from_input(ChannelRecordInput {
            id: self.id.clone(),
            provider_id: self.provider_id.clone(),
            app_type: self.app_type.clone(),
            name: self.name.clone(),
            status: self.status.clone(),
            base_url: self.base_url.clone(),
            interface_kind: self.interface_kind.clone(),
            auth_profile_ref: self.auth_profile_ref.clone(),
            groups: self.groups.clone(),
            priority: self.priority,
            weight: self.weight,
            retry_policy: self.retry_policy.clone(),
            health_policy: self.health_policy.clone(),
            header_overrides: self.header_overrides.clone(),
            param_overrides: self.param_overrides.clone(),
            status_code_mapping: self.status_code_mapping.clone(),
            tags: self.tags.clone(),
            metadata: self.metadata.clone(),
            source_kind: self.source_kind.as_str().to_string(),
            source_endpoint_url: self.source_endpoint_url.clone(),
            models: self
                .models
                .iter()
                .map(ProxyChannelModelRecord::to_proxy_core_channel_model_record_input)
                .collect(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        })
    }
}

pub(crate) fn proxy_channel_record_to_core(channel: ProxyChannelRecord) -> ChannelRecord {
    channel.to_proxy_core_channel_record()
}

pub(crate) fn create_channel_record_from_db_source(
    db: &Database,
    request: ProxyChannelWriteRequest,
) -> ProxyCoreResult<ChannelRecord> {
    let channel = db
        .create_proxy_channel(request)
        .map_err(|error| app_write_error("create channel record", error))?;
    Ok(proxy_channel_record_to_core(channel))
}

pub(crate) fn channel_record_from_db_source(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<Option<ChannelRecord>> {
    let channel = db
        .get_proxy_channel(channel_id)
        .map_err(|error| app_error("get channel record", error))?;
    Ok(channel.map(proxy_channel_record_to_core))
}

pub(crate) fn update_channel_record_from_db_source(
    db: &Database,
    channel_id: &str,
    patch: ProxyChannelPatchRequest,
) -> ProxyCoreResult<Option<ChannelRecord>> {
    let channel = db
        .update_proxy_channel(channel_id, patch)
        .map_err(|error| app_write_error("update channel record", error))?;
    Ok(channel.map(proxy_channel_record_to_core))
}

pub(crate) fn delete_channel_record_from_db_source(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<bool> {
    db.delete_proxy_channel(channel_id)
        .map_err(|error| app_error("delete channel record", error))
}

pub(crate) fn proxy_channel_records_to_core(
    channels: Vec<ProxyChannelRecord>,
) -> Vec<ChannelRecord> {
    channels
        .into_iter()
        .map(proxy_channel_record_to_core)
        .collect()
}

pub(crate) async fn channel_records_from_router_source(
    router: &ProviderRouter,
    app: &AppKind,
) -> ProxyCoreResult<(ChannelRouteSource, Vec<ChannelRecord>)> {
    let (channels, source) = router
        .list_channels_for_app(app.as_str())
        .await
        .map_err(|error| app_error("list channel records", error))?;
    Ok((source, proxy_channel_records_to_core(channels)))
}

pub(crate) fn materialized_channel_records_from_db_source(
    db: &Database,
    app: Option<&AppKind>,
) -> ProxyCoreResult<Vec<ChannelRecord>> {
    let channels = match app {
        Some(app) => db
            .list_proxy_channels_for_app(app.as_str())
            .map_err(|error| app_error("list materialized channel records", error))?,
        None => db
            .list_all_proxy_channels()
            .map_err(|error| app_error("list materialized channel records", error))?,
    };
    Ok(proxy_channel_records_to_core(channels))
}

pub(crate) fn proxy_channel_key_record_to_core(key: ProxyChannelKeyRecord) -> ChannelKeyRecord {
    channel_key_record_from_input(ChannelKeyRecordInput {
        channel_id: key.channel_id,
        key_ref: key.key_ref,
        status: key.status,
        priority: key.priority,
        weight: key.weight,
        last_failure_at: key.last_failure_at,
    })
}

pub(crate) fn proxy_channel_key_records_to_core(
    keys: Vec<ProxyChannelKeyRecord>,
) -> Vec<ChannelKeyRecord> {
    keys.into_iter()
        .map(proxy_channel_key_record_to_core)
        .collect()
}

pub(crate) fn channel_key_records_from_db_source(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<Option<Vec<ChannelKeyRecord>>> {
    let keys = db
        .list_proxy_channel_keys(channel_id)
        .map_err(|error| app_error("list channel key records", error))?;
    Ok(keys.map(proxy_channel_key_records_to_core))
}

pub(crate) fn upsert_channel_key_record_from_db_source(
    db: &Database,
    channel_id: &str,
    key_ref: &str,
    request: ProxyChannelKeyWriteRequest,
) -> ProxyCoreResult<Option<ChannelKeyRecord>> {
    let key = db
        .upsert_proxy_channel_key(channel_id, key_ref, request)
        .map_err(|error| app_write_error("upsert channel key record", error))?;
    Ok(Some(proxy_channel_key_record_to_core(key)))
}

pub(crate) fn update_channel_key_record_from_db_source(
    db: &Database,
    channel_id: &str,
    key_ref: &str,
    patch: ProxyChannelKeyPatchRequest,
) -> ProxyCoreResult<Option<ChannelKeyRecord>> {
    let key = db
        .update_proxy_channel_key(channel_id, key_ref, patch)
        .map_err(|error| app_write_error("update channel key record", error))?;
    Ok(key.map(proxy_channel_key_record_to_core))
}

pub(crate) fn delete_channel_key_record_from_db_source(
    db: &Database,
    channel_id: &str,
    key_ref: &str,
) -> ProxyCoreResult<bool> {
    db.delete_proxy_channel_key(channel_id, key_ref)
        .map_err(|error| app_error("delete channel key record", error))
}

pub(crate) fn channel_migration_preview_input_from_result(
    preview: ProxyChannelMigrationPreview,
) -> ChannelMigrationPreviewInput<ChannelRecord> {
    ChannelMigrationPreviewInput::new(
        preview.app_type,
        proxy_channel_records_to_core(preview.channels),
        preview.duplicate_count,
        preview.needs_review_count,
    )
}

pub(crate) fn channel_migration_preview_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<ChannelMigrationPreviewInput<ChannelRecord>> {
    let preview = db
        .preview_legacy_proxy_channel_migration(app.as_str())
        .map_err(|error| app_error("preview legacy channel migration", error))?;
    Ok(channel_migration_preview_input_from_result(preview))
}

pub(crate) fn channel_migration_materialize_input_from_result(
    result: ProxyChannelMaterializeResult,
) -> ChannelMigrationMaterializeInput {
    ChannelMigrationMaterializeInput::new(
        result.app_type,
        result.previewed_channels,
        result.inserted_channels,
        result.inserted_models,
        result.inserted_health_rows,
        result.duplicate_count,
        result.needs_review_count,
    )
}

pub(crate) fn channel_migration_materialize_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<ChannelMigrationMaterializeInput> {
    let result = db
        .materialize_legacy_proxy_channels(app.as_str())
        .map_err(|error| app_error("materialize legacy channel migration", error))?;
    Ok(channel_migration_materialize_input_from_result(result))
}

pub(crate) fn extract_proxy_session_id(
    headers: &HeaderMap,
    body: &Value,
    client_format: &str,
) -> SessionIdResult {
    crate::proxy_core::api::session::extract_session_id_with_generator(
        headers,
        body,
        client_format,
        || Uuid::new_v4().to_string(),
    )
}

pub(crate) fn channel_test_app_type_from_probe_request(
    request: &ChannelTestProbeRequest,
) -> ProxyCoreResult<AppType> {
    request
        .app_type
        .parse::<AppType>()
        .map_err(|error| ProxyCoreError::InvalidRequest(error.to_string()))
}

pub(crate) fn channel_test_provider_from_probe_source(
    request: &ChannelTestProbeRequest,
    provider: Option<Provider>,
) -> ProxyCoreResult<Provider> {
    provider.ok_or_else(|| ProxyCoreError::Config(request.provider_not_found_message()))
}

pub(crate) fn channel_reachability_probe_error(
    error: impl std::fmt::Display,
) -> ProxyCoreError {
    ProxyCoreError::Internal(error.to_string())
}

pub(crate) async fn probe_channel_reachability_from_db_source(
    db: &Database,
    request: ChannelTestProbeRequest,
) -> ProxyCoreResult<ChannelReachabilityResult> {
    let app_type = channel_test_app_type_from_probe_request(&request)?;
    let provider = db
        .get_provider_by_id(&request.provider_id, &request.app_type)
        .map_err(|error| app_error("get channel test provider", error))?;
    let provider = channel_test_provider_from_probe_source(&request, provider)?;
    let config = db
        .get_stream_check_config()
        .map_err(|error| app_error("get stream check config", error))?;
    let result =
        StreamCheckService::check_with_retry(&app_type, &provider, &config, Some(request.base_url))
            .await
            .map_err(channel_reachability_probe_error)?;

    Ok(stream_check_result_to_channel_reachability(result))
}

fn provider_metadata_without_secrets(provider: &Provider) -> ProviderMetadata {
    let meta = provider.meta.as_ref();
    provider_metadata_from_input(ProviderMetadataInput {
        website_url: provider.website_url.clone(),
        category: provider.category.clone(),
        sort_index: provider.sort_index,
        notes: provider.notes.clone(),
        icon: provider.icon.clone(),
        icon_color: provider.icon_color.clone(),
        in_failover_queue: provider.in_failover_queue,
        provider_type: meta.and_then(|meta| meta.provider_type.clone()),
        api_format: meta.and_then(|meta| meta.api_format.clone()),
        auth_binding: meta
            .and_then(|meta| meta.auth_binding.as_ref())
            .map(|binding| json!(binding)),
        endpoint_auto_select: meta.and_then(|meta| meta.endpoint_auto_select),
        custom_endpoint_count: meta
            .map(|meta| meta.custom_endpoints.len())
            .unwrap_or(0),
    })
}

fn account_ref(provider: &Provider) -> Option<String> {
    provider.meta.as_ref().and_then(|meta| {
        let provider_type = meta.provider_type.as_deref();
        let account_id =
            provider_type.and_then(|provider_type| meta.managed_account_id_for(provider_type));
        provider_account_ref(provider_type, account_id.as_deref())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::ProxyChannelSourceKind;
    use crate::provider::{
        AuthBinding, AuthBindingSource, ClaudeDesktopModelRoute, ProviderMeta,
    };
    use crate::proxy_core::api::errors::ProxyCoreError;
    use crate::proxy_core::api::session::SessionIdSource;
    use crate::proxy_core::api::transforms::GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX;
    use crate::proxy_core::api::transport::UpstreamTransportKind;

    #[test]
    fn app_type_conversion_preserves_known_and_custom_names() {
        assert_eq!(AppKind::from(&AppType::Claude), AppKind::Claude);
        assert_eq!(
            proxy_core_app_kind_from_app_type(&AppType::Claude),
            AppKind::Claude
        );
        assert_eq!(
            AppKind::from(&AppType::ClaudeDesktop),
            AppKind::ClaudeDesktop
        );
        assert_eq!(AppKind::from(&AppType::Codex), AppKind::Codex);
        assert_eq!(
            AppKind::from(&AppType::OpenClaw),
            AppKind::Custom("openclaw".to_string())
        );
        let catalog = cc_switch_app_kinds();
        let expected_catalog = AppType::all()
            .map(|app| AppKind::from(&app))
            .collect::<Vec<_>>();
        assert_eq!(catalog, expected_catalog);
        assert!(catalog.contains(&AppKind::Claude));
        assert!(catalog.contains(&AppKind::Custom("opencode".to_string())));
        assert_eq!(
            app_type_from_proxy_core_app(&AppKind::Claude).expect("claude app"),
            AppType::Claude
        );
        assert_eq!(
            app_type_option_from_proxy_core_app(&AppKind::Custom("openclaw".to_string())),
            Some(AppType::OpenClaw)
        );
        assert!(matches!(
            app_type_from_proxy_core_app(&AppKind::Custom("unknown-app".to_string())),
            Err(ProxyCoreError::Config(message))
                if message.starts_with("unsupported app kind:")
                    && message.contains("unknown-app")
        ));
        assert_eq!(
            unsupported_app_kind_error_message("invalid app: openclaw"),
            "unsupported app kind: invalid app: openclaw"
        );

        let parsed_body = parse_json_proxy_request_body(&Bytes::from_static(
            br#"{"model":"gpt-5","stream":true}"#,
        ))
        .expect("parse streamed body");
        assert_eq!(parsed_body.body["model"], "gpt-5");
        assert!(parsed_body.is_stream);

        let parsed_null_body =
            parse_json_proxy_request_body_or_null(&Bytes::new()).expect("parse empty body");
        assert!(parsed_null_body.body.is_null());
        assert!(!parsed_null_body.is_stream);

        let mut headers = HeaderMap::new();
        headers.insert("x-test", "1".parse().expect("header value"));
        let mut extensions = http::Extensions::new();
        extensions.insert("extension-value".to_string());
        let bridged_request = json_proxy_request_from_input(JsonProxyRequestInput {
            app_type: AppType::Codex,
            method: Method::POST,
            endpoint: "/v1/responses".to_string(),
            inbound_interface: InterfaceKind::OpenAiResponses,
            body: json!({"model": "gpt-5"}),
            requested_model: Some("gpt-5".to_string()),
            headers,
            extensions,
        });
        assert_eq!(bridged_request.app, AppKind::Codex);
        assert_eq!(bridged_request.endpoint, "/v1/responses");
        assert_eq!(
            bridged_request.inbound_interface,
            InterfaceKind::OpenAiResponses
        );
        assert_eq!(bridged_request.requested_model.as_deref(), Some("gpt-5"));
        assert_eq!(
            bridged_request.headers.get("x-test").and_then(|value| value.to_str().ok()),
            Some("1")
        );
        assert_eq!(
            bridged_request
                .extensions
                .get::<String>()
                .map(String::as_str),
            Some("extension-value")
        );
        assert_eq!(bridged_request.body, ProxyBody::Json(json!({"model": "gpt-5"})));

        let forward_request = forward_runtime_request_from_proxy_request(ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Bytes(Bytes::from_static(br#"{"ok":true}"#)),
        ))
        .expect("forward request");
        assert_eq!(forward_request.app_type, AppType::Claude);
        assert_eq!(forward_request.method, Method::POST);
        assert_eq!(forward_request.endpoint, "/v1/messages");
        assert_eq!(forward_request.body, json!({"ok": true}));
        assert_eq!(forward_request.session_result.source, SessionIdSource::Generated);
        assert!(!forward_request.session_result.client_provided);
        Uuid::parse_str(&forward_request.session_result.session_id)
            .expect("generated forward session id should be a UUID");

        let invalid_request = match forward_runtime_request_from_proxy_request(ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Bytes(Bytes::from_static(b"{bad-json")),
        )) {
            Ok(_) => panic!("invalid JSON body should fail"),
            Err(error) => error,
        };
        assert!(matches!(
            invalid_request,
            ProxyCoreError::InvalidRequest(message) if message.contains("invalid JSON body")
        ));
    }

    #[test]
    fn response_usage_helpers_project_provider_and_app_facts() {
        let mut provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..ProviderMeta::default()
        });

        let facts = response_usage_provider_facts(&provider, AppType::ClaudeDesktop.as_str());
        assert_eq!(facts.provider_id, "provider-a");
        assert_eq!(facts.provider_kind, Some(ProviderKind::GitHubCopilot));
        assert_eq!(facts.app, AppKind::ClaudeDesktop);

        let optional_facts = response_usage_provider_facts_from_optional(
            Some(&provider),
            AppType::ClaudeDesktop.as_str(),
            "Claude Desktop",
            UsageSelectedProviderMissingPhase::StreamingPassthrough,
        )
        .expect("provider facts");
        assert_eq!(optional_facts.provider_id, "provider-a");

        let missing_provider = response_usage_provider_facts_from_optional(
            None,
            AppType::ClaudeDesktop.as_str(),
            "Claude Desktop",
            UsageSelectedProviderMissingPhase::StreamingPassthrough,
        )
        .unwrap_err();
        assert_eq!(
            missing_provider,
            usage_selected_provider_missing_log_message(
                "Claude Desktop",
                UsageSelectedProviderMissingPhase::StreamingPassthrough
            )
        );

        fn parsed_stream_usage(_events: &[Value]) -> Option<TokenUsage> {
            Some(TokenUsage {
                input_tokens: 4,
                output_tokens: 6,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                model: None,
                message_id: None,
            })
        }

        fn extracted_stream_model(events: &[Value], fallback: &str) -> String {
            events
                .iter()
                .find_map(|event| event.get("model").and_then(Value::as_str))
                .unwrap_or(fallback)
                .to_string()
        }

        let route_context = UsageRouteContext {
            channel_id: "channel-1".to_string(),
            channel_name: "Channel One".to_string(),
            route_group: "beta".to_string(),
        };
        let error_record = forward_error_usage_record_from_response_context(
            ForwardErrorUsageContext {
                provider: Some(&provider),
                fallback_provider_id: "fallback-provider",
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                status_code: 502,
                error_message: "upstream failed".to_string(),
                latency_ms: 321,
                is_streaming: false,
                session_id: "session-error",
            },
            || "request-error".to_string(),
        );
        assert_eq!(error_record.provider_id, "provider-a");
        assert_eq!(
            error_record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(error_record.app, AppKind::ClaudeDesktop);
        assert_eq!(error_record.request_model, "request-model");
        assert_eq!(error_record.outbound_model, "outbound-model");
        assert_eq!(error_record.status_code, 502);
        assert_eq!(
            error_record.error_message.as_deref(),
            Some("upstream failed")
        );
        assert_eq!(error_record.tokens.input_tokens, 0);
        assert_eq!(error_record.channel_id.as_deref(), Some("channel-1"));

        let transformed_body = json!({
            "id": "msg_1",
            "model": "claude-response-model",
            "usage": {
                "input_tokens": 3,
                "output_tokens": 5
            }
        });
        let transformed_record = transformed_response_usage_record_from_response_context(
            TransformedResponseUsageContext {
                body: &transformed_body,
                format: TransformedResponseUsageFormat::Claude,
                provider: Some(&provider),
                tag: "Claude Desktop",
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                latency_ms: 123,
                status_code: 200,
                session_id: "session-transformed",
            },
            || "request-transformed".to_string(),
        )
        .expect("transformed provider facts")
        .expect("transformed usage record");
        assert_eq!(transformed_record.provider_id, "provider-a");
        assert_eq!(
            transformed_record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(transformed_record.app, AppKind::ClaudeDesktop);
        assert_eq!(
            transformed_record.response_model.as_deref(),
            Some("claude-response-model")
        );
        assert_eq!(transformed_record.tokens.input_tokens, 3);
        assert!(!transformed_record.is_streaming);
        assert_eq!(
            transformed_record.channel_name.as_deref(),
            Some("Channel One")
        );

        let missing_transformed = transformed_response_usage_record_from_response_context(
            TransformedResponseUsageContext {
                body: &transformed_body,
                format: TransformedResponseUsageFormat::Claude,
                provider: None,
                tag: "Claude Desktop",
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: None,
                route_context: None,
                latency_ms: 123,
                status_code: 200,
                session_id: "session-transformed",
            },
            || "request-missing".to_string(),
        )
        .unwrap_err();
        assert_eq!(
            missing_transformed,
            usage_selected_provider_missing_log_message(
                "Claude Desktop",
                UsageSelectedProviderMissingPhase::TransformedResponse
            )
        );

        let stream_events = vec![json!({"model": "stream-response-model"})];
        let stream_output = streaming_response_usage_record_from_response_context(
            StreamingResponseUsageContext {
                events: &stream_events,
                stream_parser: parsed_stream_usage,
                model_extractor: extracted_stream_model,
                provider_facts: &optional_facts,
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                latency_ms: 456,
                first_token_ms: Some(12),
                status_code: 200,
                session_id: "session-stream",
            },
            || "request-stream".to_string(),
        );
        assert_eq!(stream_output.record.provider_id, "provider-a");
        assert_eq!(
            stream_output.record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(stream_output.record.app, AppKind::ClaudeDesktop);
        assert_eq!(
            stream_output.record.response_model.as_deref(),
            Some("stream-response-model")
        );
        assert_eq!(stream_output.record.outbound_model, "outbound-model");
        assert_eq!(stream_output.record.tokens.input_tokens, 4);
        assert_eq!(stream_output.record.tokens.output_tokens, 6);
        assert!(stream_output.record.is_streaming);
        assert_eq!(stream_output.record.first_token_ms, Some(12));
        assert_eq!(
            stream_output.record.channel_id.as_deref(),
            Some("channel-1")
        );
        assert_eq!(
            stream_output.record.channel_name.as_deref(),
            Some("Channel One")
        );
        assert_eq!(stream_output.record.route_group.as_deref(), Some("beta"));

        let transformed_stream_events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "id": "msg_stream_1",
                    "model": "claude-stream-model",
                    "usage": {
                        "input_tokens": 7
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "output_tokens": 11
                }
            }),
        ];
        let transformed_stream_record =
            transformed_streaming_response_usage_record_from_response_context(
                TransformedStreamingResponseUsageContext {
                    events: &transformed_stream_events,
                    format: TransformedResponseUsageFormat::Claude,
                    provider_facts: &optional_facts,
                    request_model: "request-model",
                    outbound_model: Some("outbound-model"),
                    route_context: Some(&route_context),
                    latency_ms: 654,
                    first_token_ms: Some(34),
                    status_code: 200,
                    session_id: "session-transformed-stream",
                },
                || "request-transformed-stream".to_string(),
            )
            .expect("transformed streaming usage record");
        assert_eq!(transformed_stream_record.provider_id, "provider-a");
        assert_eq!(
            transformed_stream_record.response_model.as_deref(),
            Some("claude-stream-model")
        );
        assert_eq!(transformed_stream_record.tokens.input_tokens, 7);
        assert_eq!(transformed_stream_record.tokens.output_tokens, 11);
        assert_eq!(transformed_stream_record.first_token_ms, Some(34));
        assert!(transformed_stream_record.is_streaming);
        assert_eq!(
            transformed_stream_record.route_group.as_deref(),
            Some("beta")
        );

        let response_body =
            br#"{"model":"response-model","usage":{"prompt_tokens":2,"completion_tokens":3}}"#;
        let output = non_streaming_response_usage_record_from_response_context(
            NonStreamingResponseUsageContext {
                body: response_body,
                response_parser: TokenUsage::from_openai_response,
                provider: Some(&provider),
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                latency_ms: 123,
                status_code: 200,
                session_id: "session-1",
            },
            || "request-1".to_string(),
        )
        .expect("non-streaming usage record");

        assert!(output.usage_found);
        assert_eq!(output.record.provider_id, "provider-a");
        assert_eq!(
            output.record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(output.record.app, AppKind::ClaudeDesktop);
        assert_eq!(output.record.response_model.as_deref(), Some("response-model"));
        assert_eq!(output.record.outbound_model, "outbound-model");
        assert_eq!(output.record.tokens.input_tokens, 2);
        assert_eq!(output.record.tokens.output_tokens, 3);
        assert_eq!(output.record.channel_id.as_deref(), Some("channel-1"));
        assert_eq!(output.record.channel_name.as_deref(), Some("Channel One"));
        assert_eq!(output.record.route_group.as_deref(), Some("beta"));

        let missing = non_streaming_response_usage_record_from_response_context(
            NonStreamingResponseUsageContext {
                body: b"{}",
                response_parser: TokenUsage::from_openai_response,
                provider: None,
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: None,
                route_context: None,
                latency_ms: 123,
                status_code: 200,
                session_id: "session-1",
            },
            || "request-2".to_string(),
        )
        .unwrap_err();
        assert_eq!(
            missing,
            selected_provider_not_applied_message(AppType::ClaudeDesktop.as_str())
        );
    }

    #[test]
    fn channel_auth_profile_warning_adapter_projects_optional_ref() {
        assert_eq!(
            channel_auth_profile_missing_provider_warning(
                "claude",
                Some("provider:claude:missing"),
            ),
            "[claude] channel auth profile references missing provider: provider:claude:missing"
        );
        assert_eq!(
            channel_auth_profile_missing_provider_warning("claude", None),
            "[claude] channel auth profile references missing provider: "
        );
        assert!(matches!(
            channel_auth_profile_action(
                "claude",
                Some("provider:claude:provider-a"),
                Some("channel-a")
            ),
            ChannelAuthProfileAction::Provider {
                provider_id,
                missing_provider_warning,
            } if provider_id == "provider-a"
                && missing_provider_warning.contains("provider:claude:provider-a")
        ));
        assert!(matches!(
            channel_auth_profile_action("claude", Some("channel-key:primary"), Some("channel-a")),
            ChannelAuthProfileAction::ChannelKey { channel_id, key_ref }
                if channel_id == "channel-a" && key_ref == "primary"
        ));
        assert!(matches!(
            channel_auth_profile_action("claude", Some("channel-key:primary"), None),
            ChannelAuthProfileAction::Ignore
        ));
        assert!(matches!(
            channel_key_auth_error("channel-a", "primary"),
            ProxyCoreError::Auth(message)
                if message.contains("channel_id=channel-a")
                    && message.contains("key_ref=primary")
        ));

        let provider = Provider::with_id(
            "route-provider".to_string(),
            "Route Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "route-key" } }),
            None,
        );
        let auth_provider =
            provider_with_channel_auth_key(&AppType::Claude, &provider, "channel-key");
        assert_eq!(
            provider
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("route-key")
        );
        assert_eq!(
            auth_provider
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("channel-key")
        );

        let route_provider = provider.clone();
        fn attempt_with_auth_ref(
            route_provider: &Provider,
            channel_id: &str,
            auth_profile_ref: &str,
        ) -> ForwardAttempt {
            let provider = ProviderSpec {
                id: route_provider.id.clone(),
                name: route_provider.name.clone(),
                kind: ProviderKind::Claude,
                account_ref: None,
                metadata: ProviderMetadata::default(),
            };
            let channel = ChannelSpec {
                id: channel_id.to_string(),
                provider_id: route_provider.id.clone(),
                app: AppKind::Claude,
                name: channel_id.to_string(),
                status: ChannelStatus::Enabled,
                endpoint: UpstreamEndpoint {
                    base_url: format!("https://{channel_id}.example.com/v1"),
                    path_template: None,
                    api_version: None,
                    timeout_profile: None,
                },
                interface: InterfaceKind::AnthropicMessages,
                auth_profile: Some(AuthProfileRef::new(auth_profile_ref)),
                models: Vec::new(),
                groups: vec!["default".to_string()],
                priority: 0,
                weight: 100,
                retry_policy: RetryPolicy {
                    raw: Value::Object(Default::default()),
                },
                health_policy: ChannelHealthPolicy {
                    raw: Value::Object(Default::default()),
                },
                overrides: ChannelOverrides {
                    headers: Value::Object(Default::default()),
                    params: Value::Object(Default::default()),
                    status_code_mapping: Value::Array(Vec::new()),
                    model_mapping: Value::Object(Default::default()),
                },
                tags: Vec::new(),
                metadata: Value::Object(Default::default()),
                source_ref: None,
                needs_review: false,
                review_reasons: Vec::new(),
            };
            let selection = crate::proxy_core::api::routing::route_selection_from_parts(
                provider,
                channel,
                None,
                InterfaceKind::AnthropicMessages,
            );
            ForwardAttempt::from_core_selection(&AppType::Claude, route_provider, &selection)
        }

        let provider_auth = Provider::with_id(
            "provider-auth".to_string(),
            "Provider Auth".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-auth-key" } }),
            None,
        );
        let mut providers = IndexMap::new();
        providers.insert(route_provider.id.clone(), route_provider.clone());
        providers.insert(provider_auth.id.clone(), provider_auth);
        let mut provider_attempt = attempt_with_auth_ref(
            &route_provider,
            "channel-a",
            "provider:claude:provider-auth",
        );
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut provider_attempt),
            |_, _| unreachable!("provider auth should not load channel keys"),
        )
        .expect("apply provider auth profile");
        assert_eq!(provider_attempt.auth_provider().id, "provider-auth");

        let mut channel_key_attempt = attempt_with_auth_ref(
            &route_provider,
            "channel-key",
            "channel-key:primary",
        );
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut channel_key_attempt),
            |channel_id, key_ref| {
                assert_eq!(channel_id, "channel-key");
                assert_eq!(key_ref, "primary");
                Ok(Some("loaded-channel-key".to_string()))
            },
        )
        .expect("apply channel key auth profile");
        assert_eq!(
            channel_key_attempt
                .auth_provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("loaded-channel-key")
        );
    }

    #[test]
    fn claude_desktop_gateway_auth_adapter_projects_bearer_validation() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer gateway-token"),
        );

        validate_claude_desktop_gateway_bearer_header(&headers, "gateway-token")
            .expect("valid bearer");

        assert_eq!(
            validate_claude_desktop_gateway_bearer_header(&HeaderMap::new(), "gateway-token")
                .unwrap_err(),
            ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader
        );
        assert_eq!(
            validate_claude_desktop_gateway_bearer_header(&headers, "wrong-token").unwrap_err(),
            ClaudeDesktopGatewayAuthError::InvalidToken
        );
    }

    #[test]
    fn proxy_error_status_adapter_projects_http_contract() {
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::ForwardFailed),
            502
        );
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::AuthError),
            401
        );
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::UpstreamError(42)),
            502
        );
        assert_eq!(
            error_message_with_context("load config", "disk failed"),
            "load config: disk failed"
        );
        assert_eq!(
            proxy_error_response_body("bad")["error"]["type"],
            "proxy_error"
        );
        assert_eq!(
            upstream_proxy_error_response_body(502, Some("bad gateway"))["error"]["message"],
            "bad gateway"
        );
    }

    #[test]
    fn global_proxy_adapter_projects_masking_and_loopback_policy() {
        assert_eq!(
            SYSTEM_PROXY_ENV_KEYS,
            [
                "HTTP_PROXY",
                "http_proxy",
                "HTTPS_PROXY",
                "https_proxy",
                "ALL_PROXY",
                "all_proxy"
            ]
        );
        assert_eq!(
            mask_url_for_log("http://user:pass@127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert!(proxy_url_points_to_loopback_port(
            "socks5://localhost:15721",
            15721
        ));
        assert!(!proxy_url_points_to_loopback_port(
            "http://127.0.0.1:7890",
            15721
        ));
        assert!(proxy_values_point_to_loopback_port(
            ["", " http://127.0.0.1:15721 "],
            15721
        ));
    }

    #[test]
    fn settings_config_adapter_preserves_frontend_contracts() {
        assert_eq!(
            serde_json::to_value(RectifierConfig::default()).expect("rectifier"),
            json!({
                "enabled": true,
                "requestThinkingSignature": true,
                "requestThinkingBudget": true,
                "requestMediaFallback": true,
                "requestMediaHeuristic": true
            })
        );
        assert_eq!(
            serde_json::to_value(OptimizerConfig::default()).expect("optimizer"),
            json!({
                "enabled": false,
                "thinkingOptimizer": true,
                "cacheInjection": true,
                "cacheTtl": "1h"
            })
        );
        assert_eq!(
            CopilotOptimizerConfig::default().warmup_model,
            "gpt-5-mini"
        );
    }

    #[test]
    fn proxy_config_adapter_preserves_management_contracts() {
        let proxy_config = serde_json::to_value(ProxyConfig::default()).expect("proxy config");
        assert_eq!(
            proxy_config.get("listen_address").and_then(Value::as_str),
            Some("127.0.0.1")
        );
        assert_eq!(
            proxy_config
                .get("streaming_first_byte_timeout")
                .and_then(Value::as_u64),
            Some(60)
        );

        let app_config = AppProxyConfig {
            app_type: "claude".to_string(),
            enabled: true,
            auto_failover_enabled: true,
            max_retries: 3,
            streaming_first_byte_timeout: 60,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
            circuit_failure_threshold: 4,
            circuit_success_threshold: 2,
            circuit_timeout_seconds: 60,
            circuit_error_rate_threshold: 0.6,
            circuit_min_requests: 10,
        };
        assert!(auto_failover_enabled_from_router_config_result(
            "claude",
            Ok(app_config.clone())
        ));
        let mut no_failover_config = app_config.clone();
        no_failover_config.auto_failover_enabled = false;
        assert!(!auto_failover_enabled_from_router_config_result(
            "claude",
            Ok(no_failover_config)
        ));
        assert!(!auto_failover_enabled_from_router_config_result(
            "claude",
            Err(AppError::Config("missing proxy_config".to_string()))
        ));

        let loopback_auth =
            management_auth_decision_from_proxy_config_sources(&ProxyConfig::default(), None)
                .expect("loopback auth decision");
        assert_eq!(loopback_auth, ManagementAuthDecision::AllowWithoutToken);
        let mut public_proxy_config = ProxyConfig {
            listen_address: "0.0.0.0".to_string(),
            ..ProxyConfig::default()
        };
        assert_eq!(
            management_auth_decision_from_proxy_config_sources(&public_proxy_config, None)
                .unwrap_err(),
            ManagementAuthError::RequiredTokenMissing
        );
        assert_eq!(
            management_auth_decision_from_proxy_config_sources(
                &public_proxy_config,
                Some("env-token")
            )
            .expect("env fallback token"),
            ManagementAuthDecision::RequireToken("env-token".to_string())
        );
        public_proxy_config.management_auth_token = Some(" config-token ".to_string());
        assert_eq!(
            management_auth_decision_from_proxy_config_sources(
                &public_proxy_config,
                Some("env-token")
            )
            .expect("configured token"),
            ManagementAuthDecision::RequireToken("config-token".to_string())
        );

        assert_eq!(CircuitBreakerConfig::from(&app_config), CircuitBreakerConfig::default());
        let mut custom_breaker_app_config = app_config.clone();
        custom_breaker_app_config.circuit_failure_threshold = 7;
        custom_breaker_app_config.circuit_timeout_seconds = 45;
        let projected_breaker_config = circuit_breaker_config_from_router_config_result(Ok(
            custom_breaker_app_config.clone(),
        ));
        assert_eq!(projected_breaker_config.failure_threshold, 7);
        assert_eq!(projected_breaker_config.timeout_seconds, 45);
        assert_eq!(
            circuit_breaker_config_from_router_config_result(Err(AppError::Config(
                "missing proxy_config".to_string(),
            ))),
            CircuitBreakerConfig::default()
        );
        assert_eq!(
            circuit_failure_threshold_from_router_config_result(
                Ok(custom_breaker_app_config),
                9,
            ),
            7
        );
        assert_eq!(
            circuit_failure_threshold_from_router_config_result(
                Err(AppError::Config("missing proxy_config".to_string())),
                9,
            ),
            9
        );
        let enabled_policy = response_runtime_policy_from_app_proxy_config(&app_config);
        assert_eq!(enabled_policy.max_retries, 3);
        assert_eq!(enabled_policy.timeout.non_streaming_timeout, 600);
        assert_eq!(enabled_policy.timeout.streaming.first_byte_timeout, 60);
        assert_eq!(enabled_policy.timeout.streaming.idle_timeout, 120);
        assert_eq!(
            forwarder_runtime_options_from_app_proxy_config(&app_config),
            ForwarderRuntimeOptions {
                non_streaming_timeout: 600,
                streaming_first_byte_timeout: 60,
                streaming_idle_timeout: 120,
                max_retries: 3,
            }
        );
        let forwarder_config = forwarder_runtime_config_from_sources(
            &app_config,
            RectifierConfig {
                request_media_fallback: false,
                ..RectifierConfig::default()
            },
            OptimizerConfig {
                enabled: true,
                cache_ttl: "2h".to_string(),
                ..OptimizerConfig::default()
            },
            CopilotOptimizerConfig {
                warmup_model: "gpt-5".to_string(),
                ..CopilotOptimizerConfig::default()
            },
        );
        assert_eq!(
            forwarder_config.options,
            ForwarderRuntimeOptions {
                non_streaming_timeout: 600,
                streaming_first_byte_timeout: 60,
                streaming_idle_timeout: 120,
                max_retries: 3,
            }
        );
        assert!(!forwarder_config.rectifier.request_media_fallback);
        assert!(forwarder_config.optimizer.enabled);
        assert_eq!(forwarder_config.optimizer.cache_ttl, "2h");
        assert_eq!(forwarder_config.copilot_optimizer.warmup_model, "gpt-5");
        let projected_app = proxy_app_config_from_config_source_parts(
            AppKind::Claude,
            app_config.clone(),
            Some("anthropic-main"),
            RectifierConfig::default(),
            OptimizerConfig::default(),
            CopilotOptimizerConfig::default(),
        );
        assert_eq!(projected_app.app, Some(AppKind::Claude));
        assert_eq!(
            projected_app.raw["currentProviderId"],
            json!("anthropic-main")
        );
        let app_summary = app_summary_config_from_config_source(app_config.clone());
        assert!(app_summary.enabled);
        assert!(app_summary.auto_failover_enabled);
        assert_eq!(
            app_proxy_config_from_proxy_app_config(&projected_app)
                .expect("project host app config from core raw"),
            app_config
        );
        let invalid_projected_app = ProxyAppConfig {
            raw: json!({ "enabled": true }),
            ..projected_app.clone()
        };
        let error = app_proxy_config_from_proxy_app_config(&invalid_projected_app)
            .expect_err("invalid raw app config");
        assert!(error.starts_with("invalid app proxy config:"));

        let mut disabled_app_config = app_config.clone();
        disabled_app_config.auto_failover_enabled = false;
        let disabled_policy = response_runtime_policy_from_app_proxy_config(&disabled_app_config);
        assert_eq!(disabled_policy.max_retries, 0);
        assert_eq!(disabled_policy.timeout, ResponseTimeoutConfig::default());
        assert_eq!(
            forwarder_runtime_options_from_app_proxy_config(&disabled_app_config),
            ForwarderRuntimeOptions {
                non_streaming_timeout: ResponseTimeoutConfig::default().non_streaming_timeout,
                streaming_first_byte_timeout: ResponseTimeoutConfig::default()
                    .streaming
                    .first_byte_timeout,
                streaming_idle_timeout: ResponseTimeoutConfig::default()
                    .streaming
                    .idle_timeout,
                max_retries: 0,
            }
        );
        assert_eq!(
            serde_json::to_value(GlobalProxyConfig {
                proxy_enabled: true,
                listen_address: "127.0.0.1".to_string(),
                listen_port: DEFAULT_PROXY_LISTEN_PORT,
                enable_logging: true,
            })
            .expect("global proxy config")
            .get("proxyEnabled")
            .and_then(Value::as_bool),
            Some(true)
        );
        assert!(!proxy_runtime_config_from_config_source(ProxyConfig::default())
            .privacy_filter_enabled);
    }

    #[test]
    fn auth_adapter_projects_cc_switch_provider_config_source() {
        let auth_profile = AuthProfileRef::new("provider:claude:anthropic-main");
        let auth = auth_info_from_cc_switch_provider_config(Some(&auth_profile));
        assert!(auth.headers.is_empty());
        assert_eq!(
            auth.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(auth.metadata["source"], json!("cc_switch_provider_config"));

        let fallback = auth_info_from_cc_switch_provider_config(None);
        assert!(fallback.account_ref.is_none());
        assert_eq!(fallback.metadata["source"], json!("cc_switch_provider_config"));
    }

    #[test]
    fn circuit_and_route_adapter_projects_provider_router_contracts() {
        assert_eq!(circuit_breaker_log_codes::OPEN_TO_HALF_OPEN, "CB-001");
        assert_eq!(
            circuit_breaker_log_codes::HALF_OPEN_TO_CLOSED,
            "CB-002"
        );
        assert_eq!(provider_circuit_key("claude", "provider-a"), "claude:provider-a");
        assert_eq!(
            channel_circuit_key("claude", "channel-a"),
            "channel:claude:channel-a"
        );
        assert_eq!(
            app_type_from_circuit_key("channel:claude:channel-a"),
            "claude"
        );
        assert_eq!(CircuitState::HalfOpen.to_string(), "half_open");

        let allow_result = AllowResult {
            allowed: true,
            used_half_open_permit: true,
        };
        assert!(allow_result.allowed);
        assert!(allow_result.used_half_open_permit);

        let stats = CircuitBreakerStats {
            state: CircuitState::Open,
            consecutive_failures: 4,
            consecutive_successes: 0,
            total_requests: 10,
            failed_requests: 6,
        };
        assert_eq!(
            serde_json::to_value(stats).expect("serialize circuit stats"),
            json!({
                "state": "open",
                "consecutiveFailures": 4,
                "consecutiveSuccesses": 0,
                "totalRequests": 10,
                "failedRequests": 6
            })
        );

        let selected = select_provider_ids(ProviderSelectionInput::failover(vec![
            ProviderSelectionCandidate::new("missing", false, true),
            ProviderSelectionCandidate::new("provider-b", true, true),
            ProviderSelectionCandidate::new("provider-a", true, false),
        ]))
        .expect("selected provider ids");
        assert_eq!(selected, vec!["provider-b"]);
        let mut failover_providers = IndexMap::new();
        failover_providers.insert(
            "provider-a".to_string(),
            Provider::with_id(
                "provider-a".to_string(),
                "Provider A".to_string(),
                json!({}),
                None,
            ),
        );
        failover_providers.insert(
            "provider-b".to_string(),
            Provider::with_id(
                "provider-b".to_string(),
                "Provider B".to_string(),
                json!({}),
                None,
            ),
        );
        let failover_lookups = provider_failover_circuit_lookups_from_router_sources(
            "claude",
            vec![
                FailoverQueueItem {
                    provider_id: "missing".to_string(),
                    provider_name: "Missing".to_string(),
                    sort_index: Some(0),
                    provider_notes: None,
                },
                FailoverQueueItem {
                    provider_id: "provider-b".to_string(),
                    provider_name: "Provider B".to_string(),
                    sort_index: Some(1),
                    provider_notes: None,
                },
                FailoverQueueItem {
                    provider_id: "provider-a".to_string(),
                    provider_name: "Provider A".to_string(),
                    sort_index: Some(2),
                    provider_notes: None,
                },
            ],
            &failover_providers,
        );
        assert_eq!(failover_lookups[0].provider_id, "missing");
        assert!(!failover_lookups[0].configured);
        assert_eq!(failover_lookups[1].provider_id, "provider-b");
        assert!(failover_lookups[1].configured);
        assert_eq!(
            failover_lookups[1].circuit_key.as_deref(),
            Some("claude:provider-b")
        );
        let selected_failover = select_failover_providers_from_router_lookup_availability(
            "claude",
            &failover_providers,
            failover_lookups
                .into_iter()
                .map(|lookup| {
                    let available = lookup.provider_id == "provider-b";
                    (lookup, available)
                }),
        )
        .expect("selected failover providers");
        assert_eq!(selected_failover.len(), 1);
        assert_eq!(selected_failover[0].id, "provider-b");
        assert!(!channel_route_should_load_legacy_projection(
            &ChannelRouteSource::MaterializedChannels
        ));
        assert!(channel_route_should_load_legacy_projection(
            &ChannelRouteSource::LegacyProjection
        ));
        assert!(matches!(
            app_error_from_provider_selection_failure(
                "claude",
                ProviderSelectionFailure::AllProvidersCircuitOpen,
            ),
            AppError::AllProvidersCircuitOpen
        ));
        assert!(matches!(
            app_error_from_provider_selection_failure(
                "claude",
                ProviderSelectionFailure::NoProvidersConfigured,
            ),
            AppError::NoProvidersConfigured
        ));
        assert!(matches!(
            app_error_from_proxy_core_error(ProxyCoreError::Config("bad config".to_string())),
            AppError::Config(message) if message == "bad config"
        ));
        assert!(matches!(
            app_error_from_proxy_core_error(ProxyCoreError::InvalidRequest(
                "bad request".to_string()
            )),
            AppError::InvalidInput(message) if message == "bad request"
        ));
        assert!(matches!(
            app_error_from_proxy_core_error(ProxyCoreError::Unavailable(
                "not available".to_string()
            )),
            AppError::Message(message) if message.contains("not available")
        ));
        assert_eq!(
            current_provider_id_from_sources(Some("settings-provider"), Some("db-provider")),
            "settings-provider"
        );
        assert_eq!(
            current_provider_id_option_from_sources(Some("settings-provider"), Some("db-provider")),
            Some("settings-provider".to_string())
        );
        assert!(!current_provider_db_fallback_required(Some("settings-provider")));
        assert!(!current_provider_db_fallback_required(Some("")));
        assert!(current_provider_db_fallback_required(None));
        assert_eq!(current_provider_id_option_from_sources(None, None), None);
        let mut db_lookup_called_for_settings = false;
        assert_eq!(
            current_provider_id_from_router_sources(
                "claude",
                |app| {
                    assert_eq!(app, &AppType::Claude);
                    Some("settings-provider".to_string())
                },
                || {
                    db_lookup_called_for_settings = true;
                    Some("db-provider".to_string())
                },
            ),
            Some("settings-provider".to_string())
        );
        assert!(!db_lookup_called_for_settings);
        let mut settings_lookup_called_for_unknown = false;
        assert_eq!(
            current_provider_id_from_router_sources(
                "unknown-app",
                |_| {
                    settings_lookup_called_for_unknown = true;
                    Some("settings-provider".to_string())
                },
                || Some("db-provider".to_string()),
            ),
            Some("db-provider".to_string())
        );
        assert!(!settings_lookup_called_for_unknown);
        let mut db_lookup_called_for_empty = false;
        assert_eq!(
            current_provider_id_from_router_sources(
                "claude",
                |_| Some(String::new()),
                || {
                    db_lookup_called_for_empty = true;
                    Some("db-provider".to_string())
                },
            ),
            Some(String::new())
        );
        assert!(!db_lookup_called_for_empty);
        let mut db_lookup_called = false;
        assert_eq!(
            forward_current_provider_id_from_source(Some("settings-provider"), || {
                db_lookup_called = true;
                Some("db-provider".to_string())
            }),
            "settings-provider"
        );
        assert!(!db_lookup_called);
        assert_eq!(
            forward_current_provider_id_from_source(None, || Some("db-provider".to_string())),
            "db-provider"
        );
        assert_eq!(forward_current_provider_id_from_source(None, || None), "");
        let current_provider =
            Provider::with_id("provider-a".to_string(), "Provider A".to_string(), json!({}), None);
        let selected_current =
            select_current_provider_from_router_source("claude", Some(current_provider))
                .expect("selected current provider");
        assert_eq!(selected_current.len(), 1);
        assert_eq!(selected_current[0].id, "provider-a");
        assert!(matches!(
            select_current_provider_from_router_source("claude", None),
            Err(AppError::NoProvidersConfigured)
        ));

        let mut response = resolve_channel_route(
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
            vec![RouteResolveChannelInput {
                channel_id: "channel-a".to_string(),
                provider_id: "provider-a".to_string(),
                channel_name: "Provider A".to_string(),
                status: "enabled".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                groups: vec!["default".to_string()],
                models: vec![RouteResolveModelInput {
                    public_model: "claude-sonnet-4".to_string(),
                    upstream_model: "upstream-sonnet".to_string(),
                }],
                priority: 100,
                weight: 1,
                source_kind: "legacy_provider".to_string(),
            }],
            ChannelRouteSource::MaterializedChannels,
        )
        .expect("route response");
        assert_eq!(response.candidates.len(), 1);

        let circuit_lookups = route_candidate_channel_circuit_keys(&response);
        assert_eq!(circuit_lookups.len(), 1);
        assert_eq!(circuit_lookups[0].channel_id, "channel-a");
        assert_eq!(circuit_lookups[0].circuit_key, "channel:claude:channel-a");
        apply_route_candidate_circuit_availability(
            &mut response,
            [(circuit_lookups[0].clone(), false)],
        );
        assert!(response.candidates.is_empty());
        assert_eq!(response.rejected.len(), 1);
        assert_eq!(response.rejected[0].reasons, vec!["circuit_open"]);
    }

    #[test]
    fn proxy_event_adapter_projects_event_stream_contracts() {
        assert_eq!(PROXY_EVENTS_CONNECTED_EVENT, "proxy_events_connected");
        assert_eq!(PROXY_EVENTS_LAGGED_EVENT, "proxy_events_lagged");
        assert_eq!(PROXY_OFFICIAL_WARNING_EVENT, "proxy-official-warning");
        assert_eq!(PROVIDER_SWITCHED_EVENT, "provider-switched");
        assert_eq!(REQUEST_STARTED_EVENT, "request_started");
        assert_eq!(SERVER_STARTED_EVENT, "server_started");
        assert_eq!(SERVER_STOPPED_EVENT, "server_stopped");
        assert_eq!(build_proxy_events_connected_payload(256)["bufferSize"], 256);
        assert_eq!(build_proxy_events_lagged_payload(3)["skipped"], 3);
        let connected = proxy_events_connected_message(256);
        assert_eq!(connected.event_name, "proxy_events_connected");
        assert_eq!(connected.payload["bufferSize"], 256);
        let lagged = proxy_events_lagged_message(3);
        assert_eq!(lagged.event_name, "proxy_events_lagged");
        assert_eq!(lagged.payload["skipped"], 3);
        assert_eq!(
            build_proxy_official_warning_event_payload("claude", "Official Claude"),
            json!({
                "appType": "claude",
                "providerName": "Official Claude",
            })
        );
        let official_warning =
            proxy_official_warning_event_message("claude", "Official Claude");
        assert_eq!(official_warning.event_name, "proxy-official-warning");
        assert_eq!(
            official_warning.payload,
            json!({
                "appType": "claude",
                "providerName": "Official Claude",
            })
        );
        let mut provider = Provider::with_id(
            "official-codex".to_string(),
            "Official Codex".to_string(),
            json!({}),
            None,
        );
        provider.category = Some("official".to_string());
        assert!(provider_is_official_category(&provider));
        assert!(should_emit_proxy_official_warning_for_provider(&provider));
        assert!(should_reapply_codex_official_live_for_provider(&provider));
        provider.category = Some("custom".to_string());
        assert!(!provider_is_official_category(&provider));
        assert!(!should_emit_proxy_official_warning_for_provider(&provider));
        assert!(!should_reapply_codex_official_live_for_provider(&provider));
        provider.category = None;
        assert!(!provider_is_official_category(&provider));
        assert!(!should_emit_proxy_official_warning_for_provider(&provider));
        assert!(!should_reapply_codex_official_live_for_provider(&provider));
        assert_eq!(
            build_provider_switched_event_payload("claude", "provider-1", "failover"),
            json!({
                "appType": "claude",
                "providerId": "provider-1",
                "source": "failover",
            })
        );
        let provider_switched =
            provider_switched_failover_event_message("claude", "provider-1");
        assert_eq!(provider_switched.event_name, "provider-switched");
        assert_eq!(provider_switched.payload["source"], "failover");
        let provider_switched_enabled =
            provider_switched_failover_enabled_event_message("claude", "provider-1");
        assert_eq!(provider_switched_enabled.event_name, "provider-switched");
        assert_eq!(provider_switched_enabled.payload["source"], "failoverEnabled");
        assert_eq!(
            build_server_started_event_payload("127.0.0.1", 15721),
            json!({"address": "127.0.0.1", "port": 15721})
        );
        assert!(build_server_stopped_event_payload()
            .as_object()
            .is_some_and(|object| object.is_empty()));
        let server_started = server_started_event_message("127.0.0.1", 15721);
        assert_eq!(server_started.event_name, "server_started");
        assert_eq!(server_started.payload, json!({"address": "127.0.0.1", "port": 15721}));
        let server_stopped = server_stopped_event_message();
        assert_eq!(server_stopped.event_name, "server_stopped");
        assert!(server_stopped
            .payload
            .as_object()
            .is_some_and(|object| object.is_empty()));

        let envelope = ProxyEventEnvelope::new(
            42,
            "request_started",
            "2026-06-20T00:00:00Z",
            json!({"provider": "relay-a"}),
        );
        let spec = envelope.to_sse_spec();

        assert_eq!(spec.id, "42");
        assert_eq!(spec.event, "request_started");
        assert!(spec.data.contains("\"provider\":\"relay-a\""));

        let request_started = request_started_event_message("req-start", "claude");
        assert_eq!(request_started.event_name, "request_started");
        assert_eq!(request_started.payload["requestId"], "req-start");
        assert_eq!(request_started.payload["appType"], "claude");

        let message = proxy_core_event_to_bus_message(ProxyCoreEvent {
            event_type: ProxyCoreEventType::RouteSelected,
            request_id: Some("req-1".to_string()),
            channel_id: Some("channel-a".to_string()),
            payload: json!({"attemptCount": 2}),
        });
        assert_eq!(message.event_name, "route_selected");
        assert_eq!(message.payload["requestId"], "req-1");
        assert_eq!(message.payload["channelId"], "channel-a");
        assert_eq!(message.payload["attemptCount"], 2);

        let route_provider = Provider::with_id(
            "provider-1".to_string(),
            "Relay Provider".to_string(),
            json!({}),
            None,
        );
        let route_attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &route_provider,
            ChannelRouteCandidate {
                channel_id: "channel-a".to_string(),
                provider_id: route_provider.id.clone(),
                channel_name: "Relay A".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "openai_responses".to_string(),
                public_model: Some("public-sonnet".to_string()),
                upstream_model: Some("upstream-sonnet".to_string()),
                route_group: "default".to_string(),
                priority: 100,
                weight: 50,
                source_kind: "manual".to_string(),
            },
        );
        let route_message = route_selected_event_message_from_forward_attempt(
            "req-route",
            "claude",
            &route_attempt,
        );
        assert_eq!(route_message.event_name, "route_selected");
        assert_eq!(route_message.payload["requestId"], "req-route");
        assert_eq!(route_message.payload["providerId"], "provider-1");
        assert_eq!(route_message.payload["channelId"], "channel-a");
        assert_eq!(route_message.payload["interfaceKind"], "openai_responses");
        assert_eq!(route_message.payload["upstreamModel"], "upstream-sonnet");

        let failed_attempt_message = attempt_event_message_from_forward_attempt(
            "req-failed",
            "claude",
            &route_attempt,
            AttemptEventPhase::Failed,
            Some("upstream failed"),
        );
        assert_eq!(failed_attempt_message.event_name, "channel_failed");
        assert_eq!(failed_attempt_message.payload["requestId"], "req-failed");
        assert_eq!(failed_attempt_message.payload["channelId"], "channel-a");
        assert_eq!(failed_attempt_message.payload["error"], "upstream failed");

        let mut emitted = None;
        emit_proxy_core_event(
            ProxyCoreEvent {
                event_type: ProxyCoreEventType::RouteSelected,
                request_id: Some("req-2".to_string()),
                channel_id: Some("channel-b".to_string()),
                payload: json!({"attemptCount": 1}),
            },
            |event_name, payload| emitted = Some((event_name, payload)),
        );
        let (event_name, payload) = emitted.expect("event emitted");
        assert_eq!(event_name, "route_selected");
        assert_eq!(payload["requestId"], "req-2");
        assert_eq!(payload["channelId"], "channel-b");
        assert_eq!(payload["attemptCount"], 1);
    }

    #[test]
    fn codex_chat_history_adapter_projects_sse_and_state_helpers() {
        let mut buffer = String::new();
        let mut remainder = Vec::new();
        append_utf8_safe(
            &mut buffer,
            &mut remainder,
            br#"data: {"type":"response.output_item.done","response":{"id":"resp_1"},"item":{"type":"function_call","call_id":"call_1","name":"read_file","arguments":"{}"}}"#,
        );
        append_utf8_safe(&mut buffer, &mut remainder, b"\n\n");

        let block = take_sse_block(&mut buffer).expect("sse block");
        let inspection = inspect_codex_chat_history_sse_block(&block).expect("inspection");
        assert_eq!(inspection.response_id.as_deref(), Some("resp_1"));
        match inspection.record {
            Some(CodexChatHistorySseRecord::OutputItemDone { item }) => {
                assert_eq!(item["call_id"], "call_1");
            }
            other => panic!("unexpected inspection record: {other:?}"),
        }

        let mut state = CodexChatHistoryState::default();
        assert_eq!(
            state.record_response(&json!({
                "id": "resp_1",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{}",
                    "reasoning_content": "Need context."
                }]
            })),
            1
        );
        let mut request = json!({
            "previous_response_id": "resp_1",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "ok"
            }]
        });

        assert_eq!(state.enrich_request(&mut request), 1);
        assert_eq!(request["input"][0]["type"], "function_call");
        assert_eq!(request["input"][0]["reasoning_content"], "Need context.");
    }

    #[test]
    fn provider_backfill_live_settings_adapter_preserves_codex_catalog() {
        let mut provider = Provider::with_id(
            "deepseek".to_string(),
            "DeepSeek".to_string(),
            json!({
                "auth": { "OPENAI_API_KEY": "sk-deepseek" },
                "config": "model_provider = \"custom\"\nmodel = \"deepseek-v4-pro\"\n",
                "modelCatalog": {
                    "models": [
                        { "model": "deepseek-v4-pro", "contextWindow": 1_000_000 }
                    ]
                }
            }),
            None,
        );
        provider.category = Some("cn_official".to_string());

        let live_settings = json!({
            "auth": { "OPENAI_API_KEY": "sk-deepseek" },
            "config": "model_provider = \"custom\"\nmodel = \"deepseek-v4-pro\"\n"
        });
        let result =
            restore_live_settings_for_provider_backfill(&AppType::Codex, &provider, live_settings);
        assert!(result.warnings.is_empty());
        assert_eq!(
            result.settings.get("modelCatalog"),
            provider.settings_config.get("modelCatalog")
        );

        let non_codex_settings = json!({"env": {"ANTHROPIC_API_KEY": "sk-test"}});
        let result = restore_live_settings_for_provider_backfill(
            &AppType::Claude,
            &provider,
            non_codex_settings.clone(),
        );
        assert!(result.warnings.is_empty());
        assert_eq!(result.settings, non_codex_settings);
    }

    #[test]
    fn codex_provider_adapter_projects_chat_policy_headers_and_reasoning() {
        assert!(resolve_codex_provider_uses_chat_completions(
            Some("openai_chat"),
            None,
            None,
            None
        ));
        assert!(should_convert_codex_responses_endpoint_to_chat(
            true,
            "/v1/responses"
        ));
        assert_eq!(
            build_codex_upstream_url("https://api.openai.com", "/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );

        let headers = build_codex_bearer_auth_headers("sk-test").expect("bearer header");
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(
            headers[0].1,
            http::HeaderValue::from_static("Bearer sk-test")
        );
        let auth_headers =
            provider_codex_auth_headers(&ProviderAuthInfo::new(
                "sk-provider".to_string(),
                ProviderAuthStrategy::Bearer,
            ))
            .expect("provider codex auth headers");
        assert_eq!(auth_headers[0].0.as_str(), "authorization");
        assert_eq!(
            auth_headers[0].1,
            http::HeaderValue::from_static("Bearer sk-provider")
        );
        let provider = Provider::with_id(
            "codex".to_string(),
            "Codex".to_string(),
            json!({
                "env": {
                    "OPENAI_API_KEY": " sk-provider "
                },
                "baseURL": "https://api.openai.com/v1/"
            }),
            None,
        );
        assert_eq!(
            provider_codex_api_key(&provider).as_deref(),
            Some("sk-provider")
        );
        let provider_auth = provider_codex_auth_info(&provider).expect("codex auth info");
        assert_eq!(provider_auth.api_key, "sk-provider");
        assert_eq!(provider_auth.access_token, None);
        assert_eq!(provider_auth.strategy, ProviderAuthStrategy::Bearer);
        let missing_codex_auth = Provider::with_id(
            "codex-missing-auth".to_string(),
            "Codex Missing Auth".to_string(),
            json!({}),
            None,
        );
        assert!(provider_codex_auth_info(&missing_codex_auth).is_none());
        assert_eq!(
            provider_codex_base_url(&provider).as_deref(),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(
            required_codex_provider_base_url(&provider).as_deref(),
            Ok("https://api.openai.com/v1")
        );
        let missing_codex_base_url = Provider::with_id(
            "codex-missing-base-url".to_string(),
            "Codex Missing Base URL".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            required_codex_provider_base_url(&missing_codex_base_url).unwrap_err(),
            "Codex Provider 缺少 base_url 配置"
        );
        assert_eq!(
            codex_config_text_from_settings(&json!({"config": "model = \"gpt-5\""})),
            Some("model = \"gpt-5\"")
        );
        assert_eq!(codex_config_text_from_settings(&json!({"config": 42})), None);
        assert_eq!(codex_config_text_from_settings(&json!({})), None);
        let codex_auth_settings = json!({"auth": {"OPENAI_API_KEY": "sk-auth"}});
        let auth = codex_auth_object_value_from_settings(&codex_auth_settings)
            .expect("codex auth object");
        assert_eq!(
            codex_api_key_from_auth_and_config(Some(auth), Some("")).as_deref(),
            Some("sk-auth")
        );
        assert!(codex_auth_object_value_from_settings(&json!({"auth": "sk-auth"})).is_none());
        let restored_settings = json!({
            "auth": {"OPENAI_API_KEY": "sk-restored"},
            "config": "model = \"gpt-5\""
        });
        let restored_parts = codex_restored_live_settings_parts(&restored_settings);
        let expected_auth = json!({"OPENAI_API_KEY": "sk-restored"});
        let expected_config = json!("model = \"gpt-5\"");
        assert_eq!(restored_parts.auth, Some(&expected_auth));
        assert_eq!(restored_parts.config, Some(&expected_config));
        let official_live_provider = Provider::with_id(
            "codex-live-official".to_string(),
            "Codex Live Official".to_string(),
            json!({
                "auth": {
                    "tokens": {"id_token": "id-token"},
                    "auth_mode": "chatgpt"
                },
                "config": ""
            }),
            None,
        );
        assert_eq!(
            provider_codex_imported_live_category(&official_live_provider),
            "official"
        );
        let imported_official_provider = provider_from_default_live_settings(
            &AppType::Codex,
            official_live_provider.settings_config.clone(),
        );
        assert_eq!(imported_official_provider.id, "default");
        assert_eq!(imported_official_provider.name, "default");
        assert_eq!(
            imported_official_provider.category.as_deref(),
            Some("official")
        );
        let api_key_live_provider = Provider::with_id(
            "codex-live-api-key".to_string(),
            "Codex Live API Key".to_string(),
            json!({
                "auth": {"OPENAI_API_KEY": "sk-test"},
                "config": ""
            }),
            None,
        );
        assert_eq!(
            provider_codex_imported_live_category(&api_key_live_provider),
            "custom"
        );
        let imported_custom_provider = provider_from_default_live_settings(
            &AppType::Codex,
            api_key_live_provider.settings_config.clone(),
        );
        assert_eq!(
            imported_custom_provider.category.as_deref(),
            Some("custom")
        );
        let imported_claude_provider = provider_from_default_live_settings(
            &AppType::Claude,
            json!({"env": {"ANTHROPIC_API_KEY": "sk-test"}}),
        );
        assert_eq!(
            imported_claude_provider.category.as_deref(),
            Some("custom")
        );
        let bearer_live_provider = Provider::with_id(
            "codex-live-bearer".to_string(),
            "Codex Live Bearer".to_string(),
            json!({
                "auth": {"tokens": {"id_token": "id-token"}},
                "config": r#"model_provider = "custom"

[model_providers.custom]
experimental_bearer_token = "bearer-token"
"#
            }),
            None,
        );
        assert_eq!(
            provider_codex_imported_live_category(&bearer_live_provider),
            "custom"
        );
        let parts =
            provider_codex_live_settings_parts(&official_live_provider).expect("codex live parts");
        assert_eq!(parts.category, None);
        assert!(parts.auth.is_object());
        assert_eq!(parts.config_text, Some(""));
        let mut custom_category_provider = api_key_live_provider.clone();
        custom_category_provider.category = Some("custom".to_string());
        let custom_backfill_parts = provider_codex_backfill_parts(&custom_category_provider);
        assert!(custom_backfill_parts.restore_provider_token);
        assert!(!custom_backfill_parts.strip_unified_session_bucket);
        let mut official_category_provider = api_key_live_provider.clone();
        official_category_provider.category = Some("official".to_string());
        let official_backfill_parts = provider_codex_backfill_parts(&official_category_provider);
        assert!(!official_backfill_parts.restore_provider_token);
        assert!(official_backfill_parts.strip_unified_session_bucket);
        let mut live_backfill_settings = json!({
            "auth": {},
            "config": r#"model_provider = "custom"

[model_providers.custom]
experimental_bearer_token = "live-token"
"#
        });
        restore_codex_settings_for_provider_backfill(
            &custom_category_provider,
            &mut live_backfill_settings,
        )
        .expect("restore codex provider backfill");
        assert_eq!(
            live_backfill_settings
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some("live-token")
        );
        assert!(
            !live_backfill_settings
                .get("config")
                .and_then(Value::as_str)
                .expect("restored config")
                .contains("experimental_bearer_token")
        );
        let injected_unified_config =
            crate::codex_config::inject_codex_unified_session_bucket("").expect("inject");
        let mut official_unified_backfill = json!({"config": injected_unified_config});
        strip_codex_unified_session_bucket_for_provider_backfill(
            &official_category_provider,
            &mut official_unified_backfill,
        )
        .expect("strip official unified session bucket");
        assert!(
            !official_unified_backfill
                .get("config")
                .and_then(Value::as_str)
                .expect("official stripped config")
                .contains("model_provider")
        );
        let mut custom_unified_backfill = json!({
            "config": crate::codex_config::inject_codex_unified_session_bucket("").expect("inject")
        });
        strip_codex_unified_session_bucket_for_provider_backfill(
            &custom_category_provider,
            &mut custom_unified_backfill,
        )
        .expect("custom backfill no-op");
        assert!(
            custom_unified_backfill
                .get("config")
                .and_then(Value::as_str)
                .expect("custom retained config")
                .contains("model_provider")
        );
        let write_settings = json!({
            "auth": {"OPENAI_API_KEY": "sk-write"},
            "config": "model = \"gpt-5\""
        });
        let write_parts = codex_provider_live_write_parts(
            &write_settings,
            &custom_category_provider,
        )
        .expect("codex provider live write parts");
        assert_eq!(write_parts.category, Some("custom"));
        assert_eq!(
            write_parts.auth.get("OPENAI_API_KEY").and_then(Value::as_str),
            Some("sk-write")
        );
        assert_eq!(write_parts.config_text, Some("model = \"gpt-5\""));
        assert!(matches!(
            codex_provider_live_write_parts(
                &json!({"config": "model = \"gpt-5\""}),
                &custom_category_provider,
            ),
            Err(CodexProviderLiveWriteIssue::MissingAuth)
        ));
        let invalid_shape = Provider::with_id(
            "codex-live-invalid".to_string(),
            "Codex Live Invalid".to_string(),
            json!("not-object"),
            None,
        );
        assert!(matches!(
            provider_codex_live_settings_parts(&invalid_shape),
            Err(CodexLiveSettingsIssue::NotObject)
        ));
        let missing_auth = Provider::with_id(
            "codex-live-missing-auth".to_string(),
            "Codex Live Missing Auth".to_string(),
            json!({"config": ""}),
            None,
        );
        assert!(matches!(
            provider_codex_live_settings_parts(&missing_auth),
            Err(CodexLiveSettingsIssue::MissingAuth)
        ));
        let mut auth_not_object = Provider::with_id(
            "codex-live-auth-string".to_string(),
            "Codex Live Auth String".to_string(),
            json!({"auth": "sk-test"}),
            None,
        );
        auth_not_object.category = Some("custom".to_string());
        assert!(matches!(
            provider_codex_live_settings_parts(&auth_not_object),
            Err(CodexLiveSettingsIssue::AuthNotObject)
        ));
        let snapshot_parts = provider_codex_live_snapshot_parts(&auth_not_object)
            .expect("snapshot keeps legacy auth shape tolerance");
        assert_eq!(snapshot_parts.category, Some("custom"));
        assert_eq!(snapshot_parts.auth, &json!("sk-test"));
        assert_eq!(snapshot_parts.config_text, None);
        assert!(matches!(
            provider_codex_live_snapshot_parts(&invalid_shape),
            Err(CodexLiveSnapshotIssue::NotObject)
        ));
        assert!(matches!(
            provider_codex_live_snapshot_parts(&missing_auth),
            Err(CodexLiveSnapshotIssue::MissingAuth)
        ));
        let validation_parts = provider_codex_validation_parts(&official_live_provider)
            .expect("codex validation parts");
        assert_eq!(validation_parts.config_text, Some(""));
        let provider_validation_parts =
            provider_settings_validation_parts(&AppType::Codex, &official_live_provider)
                .expect("provider validation parts");
        assert_eq!(provider_validation_parts.codex_config_text, Some(""));
        assert!(matches!(
            provider_codex_validation_parts(&invalid_shape),
            Err(CodexProviderValidationIssue::NotObject)
        ));
        assert!(matches!(
            provider_settings_validation_parts(&AppType::Claude, &invalid_shape),
            Err(ProviderSettingsValidationIssue::ClaudeSettingsNotObject)
        ));
        assert!(matches!(
            provider_settings_validation_parts(&AppType::OpenCode, &invalid_shape),
            Err(ProviderSettingsValidationIssue::OpenCodeSettingsNotObject)
        ));
        assert!(matches!(
            provider_codex_validation_parts(&missing_auth),
            Err(CodexProviderValidationIssue::MissingAuth)
        ));
        assert!(matches!(
            provider_settings_validation_parts(&AppType::Codex, &missing_auth),
            Err(ProviderSettingsValidationIssue::Codex(
                CodexProviderValidationIssue::MissingAuth
            ))
        ));
        assert!(matches!(
            provider_codex_validation_parts(&auth_not_object),
            Err(CodexProviderValidationIssue::AuthNotObject)
        ));
        let invalid_config = Provider::with_id(
            "codex-live-invalid-config".to_string(),
            "Codex Live Invalid Config".to_string(),
            json!({"auth": {}, "config": 42}),
            None,
        );
        assert!(matches!(
            provider_codex_validation_parts(&invalid_config),
            Err(CodexProviderValidationIssue::ConfigInvalidType)
        ));
        let validation_spec = provider_settings_validation_issue_spec(
            ProviderSettingsValidationIssue::Codex(CodexProviderValidationIssue::MissingAuth),
            "codex-live-missing-auth",
        );
        assert_eq!(validation_spec.key, "provider.codex.auth.missing");
        assert_eq!(
            validation_spec.zh,
            "供应商 codex-live-missing-auth 缺少 auth 配置"
        );
        assert_eq!(
            validation_spec.en,
            "Provider codex-live-missing-auth is missing auth configuration"
        );
        assert_eq!(
            provider_settings_validation_issue_spec(
                ProviderSettingsValidationIssue::OpenClawSettingsNotObject,
                "openclaw-invalid",
            )
            .key,
            "provider.openclaw.settings.not_object"
        );
        let chat_provider = Provider::with_id(
            "codex-chat".to_string(),
            "Codex Chat".to_string(),
            json!({
                "config": r#"model_provider = "openai"
model = " upstream-model "

[model_providers.openai]
wire_api = "chat"
base_url = "https://api.openai.com/v1"
"#,
                "modelCatalog": {
                    "models": [{"model": "catalog-model"}]
                }
            }),
            None,
        );
        assert!(provider_codex_uses_chat_completions(&chat_provider));
        assert!(provider_should_convert_codex_responses_to_chat(
            &chat_provider,
            "/responses"
        ));
        assert!(!provider_should_convert_codex_responses_to_chat(
            &chat_provider,
            "/chat/completions"
        ));
        assert_eq!(
            provider_codex_upstream_model(&chat_provider).as_deref(),
            Some("upstream-model")
        );
        assert!(provider_codex_catalog_model_ids(&chat_provider).contains("catalog-model"));
        let reasoning_provider = Provider::with_id(
            "codex-reasoning".to_string(),
            "DeepSeek Relay".to_string(),
            json!({
                "config": r#"model_provider = "deepseek"
model = "deepseek-v4-pro"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat"
"#
            }),
            None,
        );
        let inferred_profile =
            provider_codex_chat_reasoning_profile(&reasoning_provider, Some("deepseek-v4-pro"))
                .expect("deepseek reasoning profile");
        assert_eq!(inferred_profile.supports_effort, Some(true));
        assert_eq!(inferred_profile.effort_value_mode.as_deref(), Some("deepseek"));
        let mut explicit_reasoning_provider = Provider::with_id(
            "codex-explicit-reasoning".to_string(),
            "Explicit Reasoning".to_string(),
            json!({}),
            None,
        );
        explicit_reasoning_provider.meta = Some(ProviderMeta {
            codex_chat_reasoning: Some(crate::provider::CodexChatReasoningConfig {
                supports_thinking: Some(false),
                supports_effort: Some(false),
                thinking_param: Some("none".to_string()),
                effort_param: Some("none".to_string()),
                effort_value_mode: None,
                output_format: Some("auto".to_string()),
            }),
            ..Default::default()
        });
        let explicit_profile = provider_codex_chat_reasoning_profile(
            &explicit_reasoning_provider,
            Some("deepseek-v4-pro"),
        )
        .expect("explicit reasoning profile");
        assert_eq!(explicit_profile.supports_thinking, Some(false));
        assert_eq!(explicit_profile.effort_param.as_deref(), Some("none"));

        assert_eq!(
            resolve_codex_provider_upstream_model(Some(" upstream-model "), None).as_deref(),
            Some("upstream-model")
        );
        let catalog_model_ids = codex_provider_catalog_model_ids_from_settings(&json!({
            "modelCatalog": {
                "models": [{"model": "catalog-model"}]
            }
        }));
        assert!(catalog_model_ids.contains("catalog-model"));
        let mut body = json!({"model": "client-model"});
        assert_eq!(
            apply_codex_chat_upstream_model_policy(
                &mut body,
                true,
                Some("upstream-model"),
                &catalog_model_ids,
            )
            .as_deref(),
            Some("upstream-model")
        );
        assert_eq!(body["model"], "upstream-model");

        let profile = normalize_codex_chat_reasoning_profile(CodexChatReasoningProfile {
            supports_effort: Some(true),
            effort_param: Some("reasoning_effort".to_string()),
            ..CodexChatReasoningProfile::default()
        });
        assert_eq!(profile.supports_thinking, Some(true));
        let options = CodexChatReasoningOptions::from_profile(&profile);
        assert_eq!(options.supports_effort, Some(true));
        let takeover_config = codex_takeover_toml_config_for_provider(
            r#"model_provider = "openai"
model = "client-model"

[model_providers.openai]
base_url = "https://api.openai.com/v1"
wire_api = "chat"
"#,
            "http://127.0.0.1:15721/v1",
            Some(&chat_provider),
        );
        let parsed_takeover: toml::Value =
            toml::from_str(&takeover_config).expect("takeover config should be valid TOML");
        assert_eq!(
            parsed_takeover
                .get("model_providers")
                .and_then(|providers| providers.get("openai"))
                .and_then(|provider| provider.get("base_url"))
                .and_then(toml::Value::as_str),
            Some("http://127.0.0.1:15721/v1")
        );
        assert_eq!(
            parsed_takeover
                .get("model_providers")
                .and_then(|providers| providers.get("openai"))
                .and_then(|provider| provider.get("wire_api"))
                .and_then(toml::Value::as_str),
            Some("responses")
        );
        assert_eq!(
            parsed_takeover
                .get("model")
                .and_then(toml::Value::as_str),
            Some("upstream-model")
        );
        let mut existing_auth_only_live = json!({
            "config": r#"model_provider = "openai"
model = "client-model"

[model_providers.openai]
base_url = "https://api.openai.com/v1"
"#
        });
        apply_codex_takeover_fields_for_provider(
            &mut existing_auth_only_live,
            "http://127.0.0.1:15721/v1",
            "PROXY_MANAGED",
            Some(&chat_provider),
            CodexTakeoverAuthPolicy::ExistingAuthOnly,
        );
        assert!(existing_auth_only_live.get("auth").is_none());
        assert_eq!(
            crate::codex_config::extract_codex_base_url(
                existing_auth_only_live
                    .get("config")
                    .and_then(Value::as_str)
                    .expect("takeover config")
            )
            .as_deref(),
            Some("http://127.0.0.1:15721/v1")
        );
        assert!(existing_auth_only_live.get("modelCatalog").is_some());

        let mut ensure_auth_live = json!({"config": ""});
        apply_codex_takeover_fields_for_provider(
            &mut ensure_auth_live,
            "http://127.0.0.1:15721/v1",
            "PROXY_MANAGED",
            Some(&chat_provider),
            CodexTakeoverAuthPolicy::EnsureAuth,
        );
        assert_eq!(
            ensure_auth_live
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some("PROXY_MANAGED")
        );
        assert_eq!(
            infer_codex_chat_reasoning_profile(
                "DeepSeek Relay",
                "https://api.deepseek.com",
                "deepseek-chat",
            )
            .expect("deepseek reasoning profile")
            .supports_effort,
            Some(true)
        );
    }

    #[test]
    fn proxy_response_adapter_projects_transport_body_contracts() {
        let response = ProxyCoreResponse::with_body(
            http::StatusCode::CREATED,
            HeaderMap::new(),
            ProxyResponseBody::json(json!({"ok": true})),
        );
        let transport = response
            .into_transport_response()
            .expect("transport response");

        assert_eq!(transport.status, http::StatusCode::CREATED);
        match transport.body {
            ProxyTransportResponseBody::Bytes(body) => {
                assert_eq!(body.as_ref(), br#"{"ok":true}"#);
            }
            _ => panic!("expected buffered bytes transport body"),
        }
    }

    #[test]
    fn handler_context_adapter_projects_runtime_and_model_helpers() {
        let disabled_policy = resolve_response_runtime_policy(false, 3, 600, 60, 120);
        assert_eq!(disabled_policy.max_retries, 0);
        assert_eq!(disabled_policy.timeout, ResponseTimeoutConfig::default());

        let enabled_policy = resolve_response_runtime_policy(true, 3, 600, 60, 120);
        assert_eq!(enabled_policy.max_retries, 3);
        assert_eq!(enabled_policy.timeout.non_streaming_timeout, 600);
        assert_eq!(enabled_policy.timeout.streaming.first_byte_timeout, 60);
        assert_eq!(enabled_policy.timeout.streaming.idle_timeout, 120);

        assert_eq!(
            extract_gemini_model_from_path("/v1beta/models/gemini-pro:generateContent")
                .as_deref(),
            Some("gemini-pro")
        );
        assert_eq!(
            request_model_for_forward(&AppKind::Codex, "", &json!({"model": " gpt-5 "}))
                .as_deref(),
            Some("gpt-5")
        );
        assert_eq!(
            request_model_for_forward(&AppKind::Claude, "", &json!({"model": "  "})),
            None
        );
        assert_eq!(
            request_model_for_forward(
                &AppKind::Gemini,
                "/v1beta/models/gemini-pro:generateContent",
                &Value::Null,
            )
            .as_deref(),
            Some("gemini-pro")
        );
        assert_eq!(
            claude_api_format_from_metadata(
                &json!({"claudeApiFormat": "openai_chat"}),
                "anthropic"
            ),
            "openai_chat"
        );
        assert_eq!(
            claude_api_format_from_metadata(&json!({"apiFormat": " "}), "anthropic"),
            "anthropic"
        );
    }

    #[test]
    fn gemini_provider_adapter_projects_auth_settings_and_url_helpers() {
        let settings = json!({
            "env": {
                "GEMINI_API_KEY": " ya29.access-token ",
                "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com/v1beta/"
            }
        });
        assert_eq!(
            extract_gemini_api_key_from_settings(&settings).as_deref(),
            Some("ya29.access-token")
        );
        assert_eq!(
            extract_gemini_base_url_from_settings(&settings).as_deref(),
            Some("https://generativelanguage.googleapis.com/v1beta")
        );
        let env = gemini_env_map_from_settings(&settings).expect("gemini env map");
        assert_eq!(
            env.get("GEMINI_API_KEY").and_then(Value::as_str),
            Some(" ya29.access-token ")
        );
        assert!(gemini_env_map_from_settings(&json!({"env": "invalid"})).is_none());
        assert_eq!(
            gemini_env_value_from_env_json(&json!({"env": {"A": "B"}})),
            json!({"A": "B"})
        );
        assert_eq!(gemini_env_value_from_env_json(&json!({})), json!({}));
        assert_eq!(
            gemini_live_settings_from_env_json_and_config(
                &json!({"env": {"GEMINI_API_KEY": "sk-test"}}),
                json!({"mcpServers": {"server": {}}})
            ),
            json!({
                "env": {"GEMINI_API_KEY": "sk-test"},
                "config": {"mcpServers": {"server": {}}}
            })
        );
        assert_eq!(
            gemini_live_settings_from_env_json_and_config(&json!({}), json!({})),
            json!({"env": {}, "config": {}})
        );
        assert_eq!(
            gemini_live_backup_from_effective_settings(&json!({
                "env": {"GEMINI_API_KEY": "key"},
                "config": {"mcpServers": {"kept-out-of-env-backup": {}}}
            })),
            json!({"env": {"GEMINI_API_KEY": "key"}})
        );
        assert_eq!(
            gemini_live_backup_from_effective_settings(&json!({
                "config": {"mcpServers": {}}
            })),
            json!({"env": {}})
        );
        let provider = Provider::with_id(
            "gemini".to_string(),
            "Gemini".to_string(),
            settings.clone(),
            None,
        );
        assert_eq!(
            provider_gemini_api_key(&provider).as_deref(),
            Some("ya29.access-token")
        );
        assert_eq!(
            provider_gemini_base_url(&provider).as_deref(),
            Some("https://generativelanguage.googleapis.com/v1beta")
        );
        assert_eq!(
            required_gemini_provider_base_url(&provider).as_deref(),
            Ok("https://generativelanguage.googleapis.com/v1beta")
        );
        let missing_gemini_base_url = Provider::with_id(
            "gemini-missing-base-url".to_string(),
            "Gemini Missing Base URL".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            required_gemini_provider_base_url(&missing_gemini_base_url).unwrap_err(),
            "Gemini Provider 缺少 base_url 配置"
        );
        let provider_auth = provider_gemini_auth_info(&provider).expect("gemini oauth auth info");
        assert_eq!(provider_gemini_auth_strategy(&provider), ProviderAuthStrategy::GoogleOAuth);
        assert_eq!(provider_auth.api_key, "ya29.access-token");
        assert_eq!(provider_auth.access_token.as_deref(), Some("ya29.access-token"));
        assert_eq!(provider_auth.strategy, ProviderAuthStrategy::GoogleOAuth);
        let api_key_provider = Provider::with_id(
            "gemini-api-key".to_string(),
            "Gemini API Key".to_string(),
            json!({"env": {"GEMINI_API_KEY": "AIza-api-key"}}),
            None,
        );
        let api_key_auth =
            provider_gemini_auth_info(&api_key_provider).expect("gemini api key auth info");
        assert_eq!(
            provider_gemini_auth_strategy(&api_key_provider),
            ProviderAuthStrategy::Google
        );
        assert_eq!(api_key_auth.api_key, "AIza-api-key");
        assert_eq!(api_key_auth.access_token, None);
        assert_eq!(api_key_auth.strategy, ProviderAuthStrategy::Google);
        let missing_auth = Provider::with_id(
            "gemini-missing-auth".to_string(),
            "Gemini Missing Auth".to_string(),
            json!({}),
            None,
        );
        assert!(provider_gemini_auth_info(&missing_auth).is_none());
        let live_env = provider_gemini_env_map(&provider).expect("gemini env map");
        assert_eq!(
            live_env.get("GEMINI_API_KEY").map(String::as_str),
            Some(" ya29.access-token ")
        );
        validate_provider_gemini_settings_strict(&provider)
            .expect("provider Gemini settings should be valid for API key mode");
        assert_eq!(
            provider_gemini_live_config_object(&Provider::with_id(
                "gemini-config".to_string(),
                "Gemini Config".to_string(),
                json!({"config": {"mcpServers": {}}}),
                None,
            ))
            .expect("config object")
            .and_then(Value::as_object)
            .map(|obj| obj.contains_key("mcpServers")),
            Some(true)
        );
        assert!(
            provider_gemini_live_config_object(&Provider::with_id(
                "gemini-null-config".to_string(),
                "Gemini Null Config".to_string(),
                json!({"config": Value::Null}),
                None,
            ))
            .expect("null config should preserve live file")
            .is_none()
        );
        assert!(matches!(
            provider_gemini_live_config_object(&Provider::with_id(
                "gemini-invalid-config".to_string(),
                "Gemini Invalid Config".to_string(),
                json!({"config": "not-object"}),
                None,
            )),
            Err(GeminiLiveConfigIssue::InvalidType)
        ));
        assert_eq!(
            gemini_live_settings_to_write(
                Some(json!({
                    "mcpServers": {"existing": {}},
                    "security": {"auth": {"selectedType": "oauth-personal"}}
                })),
                Some(&json!({
                    "security": {"auth": {"selectedType": "api-key"}},
                    "ui": {"theme": "dark"}
                })),
            ),
            Some(json!({
                "mcpServers": {"existing": {}},
                "security": {"auth": {"selectedType": "api-key"}},
                "ui": {"theme": "dark"}
            }))
        );
        assert_eq!(
            gemini_live_settings_to_write(Some(json!({"mcpServers": {}})), None),
            Some(json!({"mcpServers": {}}))
        );
        assert_eq!(
            gemini_live_settings_to_write(None, Some(&json!({"ui": {"theme": "dark"}}))),
            Some(json!({"ui": {"theme": "dark"}}))
        );

        let creds = parse_gemini_oauth_credentials("ya29.access-token")
            .expect("direct oauth token should parse");
        assert_eq!(creds.access_token, "ya29.access-token");
        assert!(!creds.needs_refresh());
        assert_eq!(
            build_gemini_upstream_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "/v1beta/models/gemini-pro:generateContent",
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent"
        );

        let oauth_headers = build_gemini_auth_headers(
            "refresh-token",
            Some("ya29.access-token"),
            true,
        )
        .expect("oauth headers");
        assert_eq!(oauth_headers[0].0.as_str(), "authorization");
        assert_eq!(
            oauth_headers[0].1,
            http::HeaderValue::from_static("Bearer ya29.access-token")
        );
        let provider_oauth_headers =
            provider_gemini_auth_headers(&ProviderAuthInfo::with_access_token(
                "refresh-token".to_string(),
                "ya29.provider-access-token".to_string(),
            ))
            .expect("provider gemini oauth headers");
        assert_eq!(provider_oauth_headers[0].0.as_str(), "authorization");
        assert_eq!(
            provider_oauth_headers[0].1,
            http::HeaderValue::from_static("Bearer ya29.provider-access-token")
        );
        let api_key_headers =
            build_gemini_auth_headers("AIza-api-key", None, false).expect("api key headers");
        assert_eq!(api_key_headers[0].0.as_str(), "x-goog-api-key");
        assert_eq!(
            api_key_headers[0].1,
            http::HeaderValue::from_static("AIza-api-key")
        );
        let provider_api_key_headers =
            provider_gemini_auth_headers(&ProviderAuthInfo::new(
                "AIza-provider-key".to_string(),
                ProviderAuthStrategy::Google,
            ))
            .expect("provider gemini api key headers");
        assert_eq!(provider_api_key_headers[0].0.as_str(), "x-goog-api-key");
        assert_eq!(
            provider_api_key_headers[0].1,
            http::HeaderValue::from_static("AIza-provider-key")
        );
    }

    #[test]
    fn claude_provider_adapter_projects_config_auth_url_and_cache_helpers() {
        let settings = json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": " claude-token ",
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com/v1/"
            }
        });
        assert_eq!(
            resolve_claude_api_format_from_settings(None, Some("openai_chat"), &settings),
            "openai_chat"
        );
        let auth_key = extract_claude_auth_key_from_settings(&settings)
            .expect("anthropic auth token");
        assert_eq!(auth_key.key, "claude-token");
        assert_eq!(auth_key.source, ClaudeAuthKeySource::AnthropicAuthToken);
        assert_eq!(
            extract_claude_base_url_from_settings(false, &settings).as_deref(),
            Some("https://api.anthropic.com/v1")
        );
        let env_credentials =
            claude_env_credentials_from_settings(&settings).expect("claude env credentials");
        assert_eq!(env_credentials.api_key, Some(" claude-token "));
        assert_eq!(
            env_credentials.base_url,
            Some("https://api.anthropic.com/v1/")
        );
        assert!(claude_env_credentials_from_settings(&json!({"env": "invalid"})).is_none());
        let mut provider = Provider::with_id(
            "claude".to_string(),
            "Claude".to_string(),
            settings.clone(),
            None,
        );
        assert!(provider_settings_config_is_object(&provider));
        assert!(!provider_settings_config_is_object(&Provider::with_id(
            "invalid".to_string(),
            "Invalid".to_string(),
            json!("not-object"),
            None,
        )));
        provider.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        assert_eq!(provider_claude_api_format(&provider), "openai_chat");
        assert!(provider_needs_claude_transform(&provider));
        let no_transform_provider = Provider::with_id(
            "claude-no-transform".to_string(),
            "Claude No Transform".to_string(),
            json!({"env": {"ANTHROPIC_BASE_URL": "https://api.anthropic.com/v1/"}}),
            None,
        );
        assert!(!provider_needs_claude_transform(&no_transform_provider));
        let provider_auth_key = provider_claude_auth_key(&provider).expect("provider auth token");
        assert_eq!(provider_auth_key.key, "claude-token");
        let provider_auth = provider_claude_auth_info(&provider).expect("provider auth info");
        assert_eq!(provider_auth.api_key, "claude-token");
        assert_eq!(provider_auth.access_token, None);
        assert_eq!(provider_auth.strategy, ProviderAuthStrategy::ClaudeAuth);
        assert_eq!(
            provider_claude_base_url(&provider).as_deref(),
            Some("https://api.anthropic.com/v1")
        );
        assert_eq!(
            required_claude_provider_base_url(&provider).as_deref(),
            Ok("https://api.anthropic.com/v1")
        );
        let missing_claude_base_url = Provider::with_id(
            "claude-missing-base-url".to_string(),
            "Claude Missing Base URL".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            required_claude_provider_base_url(&missing_claude_base_url).unwrap_err(),
            "Claude Provider 缺少 base_url 配置"
        );
        let direct_key_provider = Provider::with_id(
            "claude-direct-key".to_string(),
            "Claude Direct Key".to_string(),
            json!({"apiKey": "sk-direct"}),
            None,
        );
        let direct_key_auth =
            provider_claude_auth_info(&direct_key_provider).expect("direct auth info");
        assert_eq!(direct_key_auth.api_key, "sk-direct");
        assert_eq!(direct_key_auth.strategy, ProviderAuthStrategy::Anthropic);
        let mut gemini_cli_provider = Provider::with_id(
            "claude-gemini-cli".to_string(),
            "Claude Gemini CLI".to_string(),
            json!({"env": {
                "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                "ANTHROPIC_API_KEY": "{\"access_token\":\"ya29.valid\",\"refresh_token\":\"rt\"}"
            }}),
            None,
        );
        gemini_cli_provider.meta = Some(ProviderMeta {
            api_format: Some("gemini_native".to_string()),
            ..Default::default()
        });
        let gemini_cli_auth =
            provider_claude_auth_info(&gemini_cli_provider).expect("gemini cli auth info");
        assert_eq!(gemini_cli_auth.access_token.as_deref(), Some("ya29.valid"));
        assert_eq!(gemini_cli_auth.strategy, ProviderAuthStrategy::GoogleOAuth);
        let missing_claude_auth = Provider::with_id(
            "claude-missing-auth".to_string(),
            "Claude Missing Auth".to_string(),
            json!({}),
            None,
        );
        assert!(provider_claude_auth_info(&missing_claude_auth).is_none());
        assert_eq!(
            build_claude_upstream_url("https://api.anthropic.com/v1", "/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );

        let bearer_headers =
            build_claude_auth_headers(ClaudeAuthHeaderKind::Bearer, "claude-token", None)
                .expect("bearer headers");
        assert_eq!(bearer_headers[0].0.as_str(), "authorization");
        assert_eq!(
            bearer_headers[0].1,
            http::HeaderValue::from_static("Bearer claude-token")
        );
        let provider_bearer_headers = provider_claude_auth_headers(&ProviderAuthInfo::new(
            "claude-provider-token".to_string(),
            ProviderAuthStrategy::ClaudeAuth,
        ))
        .expect("provider claude bearer headers");
        assert_eq!(provider_bearer_headers[0].0.as_str(), "authorization");
        assert_eq!(
            provider_bearer_headers[0].1,
            http::HeaderValue::from_static("Bearer claude-provider-token")
        );
        let copilot_headers = build_copilot_auth_headers(CopilotAuthHeadersInput {
            api_key: "copilot-token",
            request_id: "request-1",
            editor_version: "vscode/1",
            editor_plugin_version: "plugin/1",
            integration_id: "integration-1",
            user_agent: "copilot-test",
            github_api_version: "2022-11-28",
        })
        .expect("copilot headers");
        assert!(copilot_headers
            .iter()
            .any(|(name, value)| name.as_str() == "x-request-id" && value == "request-1"));
        let provider_copilot_headers = provider_claude_auth_headers(&ProviderAuthInfo::new(
            "copilot-provider-token".to_string(),
            ProviderAuthStrategy::GitHubCopilot,
        ))
        .expect("provider claude copilot headers");
        assert!(provider_copilot_headers.iter().any(|(name, value)| {
            name.as_str() == "authorization" && value == "Bearer copilot-provider-token"
        }));
        assert!(provider_copilot_headers
            .iter()
            .any(|(name, _)| name.as_str() == "x-request-id"));

        assert!(is_copilot_prompt_cache_provider(
            Some("github_copilot"),
            &json!({})
        ));
        let cache_key = resolve_claude_responses_prompt_cache_key(
            &json!({"metadata": {"session_id": "session-1"}}),
            None,
            Some("fallback-session"),
            true,
        );
        assert_eq!(cache_key.key.as_deref(), Some("session-1"));
        assert_eq!(cache_key.source.as_str(), "session");
        let mut copilot_cache_provider = Provider::with_id(
            "copilot-cache".to_string(),
            "Copilot Cache".to_string(),
            json!({}),
            None,
        );
        copilot_cache_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..Default::default()
        });
        let provider_cache_key = provider_claude_responses_prompt_cache_key(
            &copilot_cache_provider,
            &json!({"metadata": {"session_id": "session-2"}}),
            Some("fallback-session"),
        );
        assert_eq!(provider_cache_key.key.as_deref(), Some("session-2"));
        let mut explicit_cache_provider = Provider::with_id(
            "explicit-cache".to_string(),
            "Explicit Cache".to_string(),
            json!({}),
            None,
        );
        explicit_cache_provider.meta = Some(ProviderMeta {
            prompt_cache_key: Some("cache-explicit".to_string()),
            codex_fast_mode: Some(true),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_prompt_cache_key(&explicit_cache_provider),
            Some("cache-explicit")
        );
        assert!(provider_codex_fast_mode_enabled(&explicit_cache_provider));
    }

    #[test]
    fn claude_transform_adapter_projects_request_and_response_facades() {
        let anthropic_body = json!({
            "model": "claude-sonnet",
            "max_tokens": 128,
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let chat_request = anthropic_to_openai_chat_request(&anthropic_body, false);
        assert_eq!(chat_request["model"], "claude-sonnet");
        assert_eq!(chat_request["messages"][0]["role"], "user");
        let mut request_provider = Provider::with_id(
            "claude-request".to_string(),
            "Claude Request".to_string(),
            json!({}),
            None,
        );
        request_provider.meta = Some(ProviderMeta {
            prompt_cache_key: Some("cache-request".to_string()),
            ..Default::default()
        });
        let mut streaming_anthropic_body = anthropic_body.clone();
        streaming_anthropic_body["stream"] = json!(true);
        let delegated_chat_request = provider_claude_transform_request_for_api_format(
            streaming_anthropic_body,
            &request_provider,
            "openai_chat",
            None,
            None,
        )
        .expect("delegated chat request");
        assert_eq!(delegated_chat_request["model"], "claude-sonnet");
        assert_eq!(delegated_chat_request["prompt_cache_key"], "cache-request");
        assert_eq!(
            delegated_chat_request["stream_options"]["include_usage"],
            true
        );

        let responses_request =
            anthropic_to_openai_responses_request(&anthropic_body, Some("cache-1"), false, false);
        assert_eq!(responses_request["model"], "claude-sonnet");
        assert_eq!(responses_request["prompt_cache_key"], "cache-1");
        let delegated_responses_request = provider_claude_transform_request_for_api_format(
            anthropic_body.clone(),
            &request_provider,
            "openai_responses",
            Some("session-request"),
            None,
        )
        .expect("delegated responses request");
        assert_eq!(delegated_responses_request["model"], "claude-sonnet");
        assert_eq!(
            delegated_responses_request["prompt_cache_key"],
            "cache-request"
        );

        let gemini_request = anthropic_request_to_gemini_request_with_shadow(
            &anthropic_body,
            None,
            Some("provider-a"),
            Some("session-a"),
        )
        .expect("gemini request");
        assert_eq!(gemini_request["contents"][0]["role"], "user");
        let delegated_gemini_request = provider_claude_transform_request_for_api_format(
            anthropic_body.clone(),
            &request_provider,
            "gemini_native",
            Some("session-request"),
            None,
        )
        .expect("delegated gemini request");
        assert_eq!(delegated_gemini_request["contents"][0]["role"], "user");
        let delegated_passthrough_request = provider_claude_transform_request_for_api_format(
            anthropic_body.clone(),
            &request_provider,
            "anthropic",
            None,
            None,
        )
        .expect("delegated passthrough request");
        assert_eq!(delegated_passthrough_request, anthropic_body);

        let chat_response = openai_chat_to_anthropic_message(&json!({
            "id": "chatcmpl_1",
            "model": "chat-model",
            "choices": [{
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
        }))
        .expect("chat response");
        assert_eq!(chat_response["content"][0]["text"], "Hi");
        let delegated_chat_response = provider_claude_transform_response(json!({
            "id": "chatcmpl_1",
            "model": "chat-model",
            "choices": [{
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
        }))
        .expect("delegated chat response");
        assert_eq!(delegated_chat_response["content"][0]["text"], "Hi");

        let responses_response = openai_responses_to_anthropic_message(&json!({
            "id": "resp_1",
            "model": "responses-model",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "Done"}]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 2}
        }))
        .expect("responses response");
        assert_eq!(responses_response["content"][0]["text"], "Done");
        let delegated_responses_response = provider_claude_transform_response(json!({
            "id": "resp_1",
            "model": "responses-model",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "Done"}]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 2}
        }))
        .expect("delegated responses response");
        assert_eq!(delegated_responses_response["content"][0]["text"], "Done");

        let gemini_output = gemini_response_to_anthropic_message(
            &json!({
                "responseId": "gemini_1",
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{"text": "Gemini hi"}]
                    },
                    "finishReason": "STOP"
                }],
                "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 2}
            }),
            None,
            || "toolu_test".to_string(),
        )
        .expect("gemini response");
        assert_eq!(gemini_output.response["content"][0]["text"], "Gemini hi");
        let delegated_gemini_response = provider_claude_transform_response(json!({
            "responseId": "gemini_1",
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Gemini hi"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 2}
        }))
        .expect("delegated gemini response");
        assert_eq!(delegated_gemini_response["content"][0]["text"], "Gemini hi");

        assert!(should_preserve_reasoning_content_for_openai_chat(
            &json!({}),
            &json!({"model": "deepseek-v4-pro"})
        ));
        let reasoning_provider = Provider::with_id(
            "reasoning".to_string(),
            "Reasoning".to_string(),
            json!({}),
            None,
        );
        assert!(provider_should_preserve_reasoning_content_for_openai_chat(
            &reasoning_provider,
            &json!({"model": "deepseek-v4-pro"})
        ));

        let mut normalize_provider = Provider::with_id(
            "claude-normalize".to_string(),
            "Claude Normalize".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic"
                }
            }),
            None,
        );
        normalize_provider.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });
        let mut normalize_body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "messages": [{ "role": "user", "content": "hello" }]
        });
        assert!(provider_claude_normalize_anthropic_messages(
            &mut normalize_body,
            &normalize_provider,
            "anthropic"
        ));
        assert!(normalize_body.get("output_config").is_none());
        let mut non_anthropic_body = normalize_body.clone();
        assert!(!provider_claude_normalize_anthropic_messages(
            &mut non_anthropic_body,
            &normalize_provider,
            "openai_chat"
        ));
    }

    #[test]
    fn proxy_server_adapter_projects_runtime_contracts() {
        assert_eq!(server_log_codes::STARTED, "SRV-001");
        assert_eq!(server_log_codes::STOPPED, "SRV-002");
        assert_eq!(server_log_codes::ACCEPT_ERR, "SRV-005");
        let _shadow_store = GeminiShadowStore::default();
        let stopped = proxy_runtime_status_stopped();
        assert!(!stopped.running);
        assert_eq!(stopped.port, 0);
        assert!(stopped.active_targets.is_empty());
        let info = proxy_server_info_from_parts(
            "127.0.0.1",
            15721,
            "2026-06-21T00:00:00Z",
        );
        assert_eq!(info.address, "127.0.0.1");
        assert_eq!(info.port, 15721);
        assert_eq!(info.started_at, "2026-06-21T00:00:00Z");
        let takeover = proxy_takeover_status_from_parts(true, false, true, false, false);
        assert!(takeover.claude);
        assert!(!takeover.codex);
        assert!(takeover.gemini);
        assert!(!takeover.opencode);
        assert!(!takeover.openclaw);

        assert_eq!(
            proxy_live_urls_from_listen_parts("127.0.0.1", 15721),
            Some((
                "http://127.0.0.1:15721".to_string(),
                "http://127.0.0.1:15721/v1".to_string()
            ))
        );
        assert_eq!(
            proxy_live_urls_from_listen_parts("0.0.0.0", 15721),
            Some((
                "http://127.0.0.1:15721".to_string(),
                "http://127.0.0.1:15721/v1".to_string()
            ))
        );
        assert_eq!(
            proxy_live_urls_from_listen_parts("::", 15721),
            Some((
                "http://[::1]:15721".to_string(),
                "http://[::1]:15721/v1".to_string()
            ))
        );
        assert_eq!(
            proxy_live_urls_from_listen_parts("fd00::1", 15721),
            Some((
                "http://[fd00::1]:15721".to_string(),
                "http://[fd00::1]:15721/v1".to_string()
            ))
        );
        assert_eq!(proxy_live_urls_from_listen_parts("127.0.0.1", 0), None);

        let target = CurrentRouteTarget {
            app_type: "claude".to_string(),
            provider_id: "provider-a".to_string(),
            provider_name: "Provider A".to_string(),
            channel_id: Some("channel-a".to_string()),
            channel_name: Some("Channel A".to_string()),
            interface_kind: Some("anthropic_messages".to_string()),
            public_model: Some("sonnet-public".to_string()),
            upstream_model: Some("upstream-sonnet".to_string()),
        };
        assert_eq!(
            serde_json::to_value(target).expect("serialize target"),
            json!({
                "appType": "claude",
                "providerId": "provider-a",
                "providerName": "Provider A",
                "channelId": "channel-a",
                "channelName": "Channel A",
                "interfaceKind": "anthropic_messages",
                "publicModel": "sonnet-public",
                "upstreamModel": "upstream-sonnet"
            })
        );

        let provider_only =
            current_route_target_from_provider("codex", "provider-b", "Provider B");
        assert_eq!(provider_only.app_type, "codex");
        assert_eq!(provider_only.provider_id, "provider-b");
        assert_eq!(provider_only.provider_name, "Provider B");
        assert!(provider_only.channel_id.is_none());
        assert!(provider_only.interface_kind.is_none());
    }

    #[test]
    fn proxy_switch_policy_adapter_blocks_official_only_during_takeover() {
        assert!(should_block_proxy_switch_to_provider_category(
            true,
            Some("official")
        ));
        assert!(!should_block_proxy_switch_to_provider_category(
            false,
            Some("official")
        ));
        assert!(!should_block_proxy_switch_to_provider_category(
            true,
            Some("custom")
        ));
        assert!(!should_block_proxy_switch_to_provider_category(true, None));

        let mut provider = Provider::with_id(
            "official-codex".to_string(),
            "Official Codex".to_string(),
            json!({}),
            None,
        );
        provider.category = Some("official".to_string());
        assert!(should_block_proxy_switch_to_provider(true, &provider));
        assert!(!should_block_proxy_switch_to_provider(false, &provider));
        provider.category = Some("custom".to_string());
        assert!(!should_block_proxy_switch_to_provider(true, &provider));

        assert!(!proxy_live_config_owned_by_takeover(false, false));
        assert!(proxy_live_config_owned_by_takeover(true, false));
        assert!(proxy_live_config_owned_by_takeover(false, true));
        assert!(!proxy_switch_should_hot_switch(false, false));
        assert!(proxy_switch_should_hot_switch(true, false));
        assert!(proxy_switch_should_hot_switch(false, true));

        assert!(!proxy_hot_switch_should_refresh_codex_live_from_backup(
            &AppType::Codex,
            false,
            false
        ));
        assert!(proxy_hot_switch_should_refresh_codex_live_from_backup(
            &AppType::Codex,
            true,
            false
        ));
        assert!(!proxy_hot_switch_should_refresh_codex_live_from_backup(
            &AppType::Codex,
            true,
            true
        ));
        assert!(!proxy_hot_switch_should_refresh_codex_live_from_backup(
            &AppType::Claude,
            true,
            false
        ));
        assert!(!proxy_hot_switch_should_sync_codex_live_while_proxy_active(
            &AppType::Codex,
            false
        ));
        assert!(proxy_hot_switch_should_sync_codex_live_while_proxy_active(
            &AppType::Codex,
            true
        ));
        assert!(!proxy_hot_switch_should_sync_codex_live_while_proxy_active(
            &AppType::Claude,
            true
        ));
        assert!(!proxy_hot_switch_should_sync_claude_live_while_proxy_active(
            &AppType::Claude,
            false
        ));
        assert!(proxy_hot_switch_should_sync_claude_live_while_proxy_active(
            &AppType::Claude,
            true
        ));
        assert!(!proxy_hot_switch_should_sync_claude_live_while_proxy_active(
            &AppType::Codex,
            true
        ));

        assert!(!proxy_takeover_marked_state_is_reusable(false, false));
        assert!(!proxy_takeover_marked_state_is_reusable(true, false));
        assert!(!proxy_takeover_marked_state_is_reusable(false, true));
        assert!(proxy_takeover_marked_state_is_reusable(true, true));
        assert!(!proxy_takeover_should_restore_existing_backup_before_retakeover(
            false, false
        ));
        assert!(proxy_takeover_should_restore_existing_backup_before_retakeover(true, false));
        assert!(!proxy_takeover_should_restore_existing_backup_before_retakeover(
            false, true
        ));
        assert!(!proxy_takeover_should_restore_existing_backup_before_retakeover(
            true, true
        ));
    }

    #[test]
    fn sanitize_claude_settings_for_live_strips_host_only_fields() {
        let sanitized = sanitize_claude_settings_for_live(&json!({
            "api_format": "anthropic",
            "apiFormat": "openai",
            "openrouter_compat_mode": true,
            "openrouterCompatMode": true,
            "env": {
                "ANTHROPIC_API_KEY": "sk-test"
            },
            "includeCoAuthoredBy": false
        }));

        assert_eq!(
            sanitized,
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                },
                "includeCoAuthoredBy": false
            })
        );
    }

    #[test]
    fn json_subset_helpers_match_and_remove_array_items_once() {
        let target = json!({
            "allowedTools": [
                { "name": "tool-a", "scope": "global" },
                { "name": "tool-b", "scope": "local" },
                { "name": "tool-a", "scope": "project" }
            ],
            "env": {
                "A": "1",
                "B": "2"
            }
        });
        let source = json!({
            "allowedTools": [
                { "name": "tool-a" },
                { "name": "tool-b", "scope": "local" }
            ],
            "env": {
                "A": "1"
            }
        });
        assert!(json_value_is_subset(&target, &source));

        let mut target_arr = target["allowedTools"].as_array().cloned().unwrap();
        let source_arr = source["allowedTools"].as_array().unwrap();
        json_remove_array_items(&mut target_arr, source_arr);
        assert_eq!(
            target_arr,
            vec![json!({ "name": "tool-a", "scope": "project" })]
        );
    }

    #[test]
    fn json_deep_merge_and_remove_preserve_unrelated_fields() {
        let mut target = json!({
            "env": {
                "ANTHROPIC_API_KEY": "sk-test"
            },
            "allowedTools": ["tool-a", "tool-b"],
            "includeCoAuthoredBy": true
        });
        let source = json!({
            "env": {
                "CLAUDE_CODE_USE_BEDROCK": "1"
            },
            "allowedTools": ["tool-a"],
            "includeCoAuthoredBy": false
        });

        json_deep_merge(&mut target, &source);
        assert_eq!(target["env"]["ANTHROPIC_API_KEY"], json!("sk-test"));
        assert_eq!(target["env"]["CLAUDE_CODE_USE_BEDROCK"], json!("1"));
        assert_eq!(target["allowedTools"], json!(["tool-a"]));
        assert_eq!(target["includeCoAuthoredBy"], json!(false));

        json_deep_remove(&mut target, &source);
        assert_eq!(
            target,
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            })
        );
    }

    #[test]
    fn toml_subset_helpers_match_and_remove_array_items_once() {
        let mut target_doc = r#"
allowed_tools = ["tool1", "tool2", "tool1"]

[shared]
reasoning = "medium"
extra = "keep"
"#
        .parse::<toml_edit::DocumentMut>()
        .expect("target TOML should parse");
        let source_doc = r#"
allowed_tools = ["tool1", "tool2"]

[shared]
reasoning = "medium"
"#
        .parse::<toml_edit::DocumentMut>()
        .expect("source TOML should parse");

        assert!(toml_item_is_subset(
            target_doc.as_item(),
            source_doc.as_item()
        ));

        remove_toml_table_like(target_doc.as_table_mut(), source_doc.as_table());

        let allowed_tools = target_doc["allowed_tools"]
            .as_array()
            .expect("allowed_tools should remain an array");
        let values: Vec<&str> = allowed_tools
            .iter()
            .map(|value| value.as_str().expect("tool id should be string"))
            .collect();
        assert_eq!(values, vec!["tool1"]);
        assert_eq!(target_doc["shared"]["extra"].as_str(), Some("keep"));
        assert!(target_doc["shared"]
            .as_table()
            .and_then(|table| table.get("reasoning"))
            .is_none());
    }

    #[test]
    fn common_config_settings_mutation_adapter_applies_and_removes_by_app() {
        let claude_settings = json!({"allowedTools": ["tool-a", "tool-b"]});
        let claude_snippet = r#"{"allowedTools": ["tool-a"]}"#;
        let stripped =
            remove_common_config_from_settings(&AppType::Claude, &claude_settings, claude_snippet)
                .expect("claude remove");
        assert_eq!(stripped, json!({"allowedTools": ["tool-b"]}));

        let codex_settings = json!({
            "auth": {},
            "config": "model_provider = \"openai\"\n"
        });
        let codex_snippet = "[shared]\nreasoning = \"medium\"\n";
        let applied =
            apply_common_config_to_settings(&AppType::Codex, &codex_settings, codex_snippet)
                .expect("codex apply");
        let applied_config = applied["config"].as_str().expect("codex config");
        assert!(applied_config.contains("[shared]"));
        assert!(applied_config.contains("reasoning = \"medium\""));
        let stripped =
            remove_common_config_from_settings(&AppType::Codex, &applied, codex_snippet)
                .expect("codex remove");
        assert_eq!(stripped, codex_settings);

        let gemini_settings = json!({});
        let gemini_snippet = r#"{"SHARED_REGION": "us-central1"}"#;
        let applied =
            apply_common_config_to_settings(&AppType::Gemini, &gemini_settings, gemini_snippet)
                .expect("gemini apply");
        assert_eq!(
            applied,
            json!({"env": {"SHARED_REGION": "us-central1"}})
        );

        assert_eq!(
            common_config_settings_mutation_issue_message(
                CommonConfigSettingsMutationIssue::ClaudeCommonConfigJson("bad json".to_string())
            ),
            "Invalid Claude common config: bad json"
        );
        assert_eq!(
            common_config_settings_mutation_issue_message(
                CommonConfigSettingsMutationIssue::CodexApplyTargetToml("bad target".to_string())
            ),
            "Invalid Codex config.toml while applying common config: bad target"
        );
        assert_eq!(
            common_config_settings_mutation_issue_message(
                CommonConfigSettingsMutationIssue::CodexRemoveTargetToml("bad target".to_string())
            ),
            "Invalid Codex config.toml while removing common config: bad target"
        );
        assert_eq!(
            common_config_settings_mutation_issue_message(
                CommonConfigSettingsMutationIssue::CodexCommonConfigSnippetToml(
                    "bad snippet".to_string()
                )
            ),
            "Invalid Codex common config snippet: bad snippet"
        );
        assert_eq!(
            common_config_settings_mutation_issue_message(
                CommonConfigSettingsMutationIssue::GeminiCommonConfigJson("bad json".to_string())
            ),
            "Invalid Gemini common config: bad json"
        );
        assert_eq!(
            common_config_snippet_issue_message(CommonConfigSnippetIssue::Serialization(
                "bad json".to_string()
            )),
            "Serialization failed: bad json"
        );
        assert_eq!(
            common_config_snippet_issue_message(CommonConfigSnippetIssue::TomlParse(
                "bad toml".to_string()
            )),
            "TOML parse error: bad toml"
        );
    }

    #[test]
    fn provider_effective_settings_apply_common_config_returns_warnings() {
        let mut provider = Provider::with_id(
            "claude-test".to_string(),
            "Claude Test".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            common_config_enabled: Some(true),
            ..Default::default()
        });

        let result = build_effective_settings_with_common_config(
            &AppType::Claude,
            &provider,
            Some(r#"{ "includeCoAuthoredBy": false }"#),
        );
        assert!(result.warnings.is_empty());
        assert_eq!(
            result.settings,
            json!({
                "includeCoAuthoredBy": false,
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            })
        );

        let result =
            build_effective_settings_with_common_config(&AppType::Claude, &provider, Some("{"));
        assert!(matches!(
            result.warnings.as_slice(),
            [ProviderEffectiveSettingsWarning::CommonConfigApply(_)]
        ));
        assert_eq!(result.settings, provider.settings_config);
    }

    #[test]
    fn provider_common_config_storage_normalization_requires_explicit_enablement() {
        let mut provider = Provider::with_id(
            "claude-test".to_string(),
            "Claude Test".to_string(),
            json!({
                "includeCoAuthoredBy": false,
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            }),
            None,
        );
        let snippet = r#"{ "includeCoAuthoredBy": false }"#;

        assert!(!provider_common_config_storage_normalization_requires_snippet(
            &provider
        ));
        assert_eq!(
            normalize_provider_common_config_for_storage(
                &AppType::Claude,
                &provider,
                Some(snippet)
            )
            .expect("disabled storage normalization"),
            None
        );

        provider.meta = Some(ProviderMeta {
            common_config_enabled: Some(true),
            ..Default::default()
        });

        assert!(provider_common_config_storage_normalization_requires_snippet(
            &provider
        ));
        assert_eq!(
            normalize_provider_common_config_for_storage(
                &AppType::Claude,
                &provider,
                Some("   ")
            )
            .expect("empty snippet"),
            None
        );
        assert_eq!(
            normalize_provider_common_config_for_storage(
                &AppType::Claude,
                &provider,
                Some(snippet)
            )
            .expect("enabled storage normalization"),
            Some(json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            }))
        );
    }

    #[test]
    fn provider_backfill_common_config_strip_returns_warnings() {
        let mut provider = Provider::with_id(
            "claude-test".to_string(),
            "Claude Test".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            common_config_enabled: Some(true),
            ..Default::default()
        });

        let live_settings = json!({
            "includeCoAuthoredBy": false,
            "env": {
                "ANTHROPIC_API_KEY": "sk-test"
            }
        });
        let result = strip_common_config_from_live_settings_for_backfill(
            &AppType::Claude,
            &provider,
            live_settings.clone(),
            Some(r#"{ "includeCoAuthoredBy": false }"#),
        );
        assert!(result.warnings.is_empty());
        assert_eq!(
            result.settings,
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            })
        );

        let result = strip_common_config_from_live_settings_for_backfill(
            &AppType::Claude,
            &provider,
            live_settings.clone(),
            Some("{"),
        );
        assert!(matches!(
            result.warnings.as_slice(),
            [ProviderBackfillSettingsWarning::CommonConfigStrip(_)]
        ));
        assert_eq!(result.settings, live_settings);
    }

    #[test]
    fn provider_common_config_detection_respects_meta_and_legacy_snippet() {
        let mut provider = Provider::with_id(
            "claude-test".to_string(),
            "Claude Test".to_string(),
            json!({
                "includeCoAuthoredBy": false,
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            }),
            None,
        );
        let snippet = r#"{ "includeCoAuthoredBy": false }"#;

        assert!(contains_common_config_snippet(
            &AppType::Claude,
            &provider.settings_config,
            snippet
        ));
        assert!(provider_uses_common_config(
            &AppType::Claude,
            &provider,
            Some(snippet)
        ));

        provider.meta = Some(ProviderMeta {
            common_config_enabled: Some(false),
            ..Default::default()
        });
        assert!(!provider_uses_common_config(
            &AppType::Claude,
            &provider,
            Some(snippet)
        ));

        provider.meta = Some(ProviderMeta {
            common_config_enabled: Some(true),
            ..Default::default()
        });
        assert!(!provider_uses_common_config(
            &AppType::Claude,
            &provider,
            Some("   ")
        ));
    }

    #[test]
    fn provider_auth_adapter_projects_strategy_contracts() {
        let bearer =
            ProviderAuthInfo::new("provider-token".to_string(), ProviderAuthStrategy::Bearer);
        assert_eq!(bearer.strategy, ProviderAuthStrategy::Bearer);
        assert_eq!(bearer.masked_key(), "prov...oken");
        assert!(bearer.access_token.is_none());

        let oauth = ProviderAuthInfo::with_access_token(
            "refresh-token".to_string(),
            "ya29.access-token-12345".to_string(),
        );
        assert_eq!(oauth.strategy, ProviderAuthStrategy::GoogleOAuth);
        assert_eq!(
            oauth.masked_access_token(),
            Some("ya29...2345".to_string())
        );
    }

    #[test]
    fn provider_kind_adapter_projects_inference_helpers() {
        assert_eq!(
            infer_claude_provider_kind("gemini_native", true, None, None, &json!({})),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            infer_claude_provider_kind(
                "anthropic",
                false,
                Some("github_copilot"),
                Some("https://example.com"),
                &json!({})
            ),
            ProviderKind::GitHubCopilot
        );
        assert!(is_gemini_oauth_key_shape(" ya29.access-token "));
        assert!(is_gemini_oauth_key_shape(
            r#"{"access_token":"ya29.access-token"}"#
        ));
        assert!(!is_gemini_oauth_key_shape("AIza-api-key"));
        let mut gemini_cli_provider = Provider::with_id(
            "gemini-cli".to_string(),
            "Gemini CLI".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": r#"{"access_token":"ya29.access-token"}"#,
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com"
                }
            }),
            None,
        );
        gemini_cli_provider.meta = Some(ProviderMeta {
            api_format: Some("gemini_native".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_kind(&gemini_cli_provider),
            ProviderKind::GeminiCli
        );
        let mut gemini_cli_raw_provider = Provider::with_id(
            "gemini-cli-raw".to_string(),
            "Gemini CLI Raw".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "\nya29.raw-token-value\n",
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com"
                }
            }),
            None,
        );
        gemini_cli_raw_provider.meta = Some(ProviderMeta {
            api_format: Some("gemini_native".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_kind(&gemini_cli_raw_provider),
            ProviderKind::GeminiCli
        );
        let anthropic_provider = Provider::with_id(
            "anthropic".to_string(),
            "Anthropic".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
                }
            }),
            None,
        );
        assert_eq!(provider_claude_kind(&anthropic_provider), ProviderKind::Claude);
        let openrouter_provider = Provider::with_id(
            "openrouter".to_string(),
            "OpenRouter".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                    "OPENROUTER_API_KEY": "sk-or-test"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&openrouter_provider),
            ProviderKind::OpenRouter
        );
        let claude_auth_provider = Provider::with_id(
            "claude-auth".to_string(),
            "Claude Auth".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-test"
                },
                "auth_mode": "bearer_only"
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&claude_auth_provider),
            ProviderKind::ClaudeAuth
        );
        let mut copilot_provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "copilot-token",
                    "ANTHROPIC_BASE_URL": "https://example.com"
                }
            }),
            None,
        );
        copilot_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_kind(&copilot_provider),
            ProviderKind::GitHubCopilot
        );
        assert_eq!(
            provider_kind_from_app_type_and_config(&AppType::Claude, &copilot_provider),
            ProviderKind::GitHubCopilot
        );
        let copilot_url_provider = Provider::with_id(
            "copilot-url".to_string(),
            "Copilot URL".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&copilot_url_provider),
            ProviderKind::GitHubCopilot
        );
        let gemini_provider = Provider::with_id(
            "gemini-cli".to_string(),
            "Gemini CLI".to_string(),
            json!({
                "env": {
                    "GEMINI_API_KEY": r#"{"access_token":"ya29.access-token"}"#
                }
            }),
            None,
        );
        assert_eq!(provider_gemini_kind(&gemini_provider), ProviderKind::GeminiCli);
        assert_eq!(
            provider_kind_from_app_type_and_config(&AppType::Gemini, &gemini_provider),
            ProviderKind::GeminiCli
        );
        let gemini_api_key_provider = Provider::with_id(
            "gemini-api-key".to_string(),
            "Gemini API Key".to_string(),
            json!({
                "env": {
                    "GEMINI_API_KEY": "AIza-api-key"
                }
            }),
            None,
        );
        assert_eq!(
            provider_gemini_kind(&gemini_api_key_provider),
            ProviderKind::Gemini
        );
    }

    #[test]
    fn provider_launch_env_adapter_projects_app_specific_settings() {
        let provider = Provider::with_id(
            "mixed-provider".to_string(),
            "Mixed Provider".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://anthropic.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "anthropic-token",
                    "GOOGLE_GEMINI_BASE_URL": "https://gemini.example.com",
                    "IGNORED_NUMERIC": 1
                },
                "auth": "codex-token",
                "api_key": "gemini-key"
            }),
            None,
        );

        let claude_env = provider_launch_env_vars_for_app(&provider, &AppType::Claude);
        assert!(claude_env.contains(&(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            "anthropic-token".to_string()
        )));
        assert!(claude_env.contains(&(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://anthropic.example.com".to_string()
        )));
        assert!(!claude_env.iter().any(|(key, _)| key == "IGNORED_NUMERIC"));

        let codex_env = provider_launch_env_vars_for_app(&provider, &AppType::Codex);
        assert!(codex_env.contains(&(
            "OPENAI_API_KEY".to_string(),
            "codex-token".to_string()
        )));

        let gemini_env = provider_launch_env_vars_for_app(&provider, &AppType::Gemini);
        assert!(gemini_env.contains(&(
            "GOOGLE_GEMINI_BASE_URL".to_string(),
            "https://gemini.example.com".to_string()
        )));
        assert!(gemini_env.contains(&(
            "GEMINI_API_KEY".to_string(),
            "gemini-key".to_string()
        )));
    }

    #[test]
    fn proxy_adapter_classifies_local_proxy_urls_for_takeover_cleanup() {
        for url in [
            " http://127.0.0.1:15721 ",
            "http://localhost:15721",
            "http://0.0.0.0:15721",
            "http://[::1]:15721",
            "http://[::]:15721",
            "http://::1:15721",
            "http://:::15721",
        ] {
            assert!(is_local_proxy_url(url), "{url} should be local");
        }

        for url in [
            "https://127.0.0.1:15721",
            "socks5://localhost:15721",
            "http://relay.example/v1",
            "",
        ] {
            assert!(!is_local_proxy_url(url), "{url} should not be local");
        }
    }

    #[test]
    fn proxy_placeholder_adapter_projects_app_specific_live_detection() {
        let placeholder = "PROXY_MANAGED";

        assert!(live_config_has_proxy_placeholder_for_app(
            &AppType::Claude,
            &json!({ "env": { "ANTHROPIC_API_KEY": placeholder } }),
            placeholder
        ));
        assert!(live_config_has_proxy_placeholder_for_app(
            &AppType::Codex,
            &json!({ "config": "experimental_bearer_token = \"PROXY_MANAGED\"" }),
            placeholder
        ));
        assert!(live_config_has_proxy_placeholder_for_app(
            &AppType::Gemini,
            &json!({ "env": { "GEMINI_API_KEY": placeholder } }),
            placeholder
        ));
        assert!(!live_config_has_proxy_placeholder_for_app(
            &AppType::OpenClaw,
            &json!({ "env": { "ANTHROPIC_API_KEY": placeholder } }),
            placeholder
        ));
        assert_eq!(
            live_backup_snapshot_from_live_config(
                &AppType::Claude,
                &json!({ "env": { "ANTHROPIC_AUTH_TOKEN": "real-token" } }),
                placeholder
            ),
            Some(json!({ "env": { "ANTHROPIC_AUTH_TOKEN": "real-token" } }))
        );
        assert_eq!(
            live_backup_snapshot_from_live_config(
                &AppType::Claude,
                &json!({ "env": { "ANTHROPIC_AUTH_TOKEN": placeholder } }),
                placeholder
            ),
            None
        );
        assert_eq!(
            live_backup_snapshot_from_live_config(
                &AppType::Codex,
                &json!({ "config": "experimental_bearer_token = \"PROXY_MANAGED\"" }),
                placeholder
            ),
            None
        );

        let provider = Provider::with_id(
            "codex-live-residue".to_string(),
            "Codex Live Residue".to_string(),
            json!({ "auth": placeholder }),
            None,
        );
        assert!(!provider_settings_have_proxy_placeholder_for_app(
            &provider,
            &AppType::Claude,
            placeholder
        ));
        assert!(provider_settings_have_proxy_placeholder_for_app(
            &Provider::with_id(
                "claude-live-residue".to_string(),
                "Claude Live Residue".to_string(),
                json!({ "env": { "ANTHROPIC_AUTH_TOKEN": placeholder } }),
                None,
            ),
            &AppType::Claude,
            placeholder
        ));

        let mut claude_live = json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": placeholder,
                "ANTHROPIC_API_KEY": "real-key",
                "ANTHROPIC_BASE_URL": "http://localhost:15721",
                "OTHER": "kept"
            }
        });
        assert_eq!(
            remove_claude_takeover_env_fields_if_present(
                &mut claude_live,
                placeholder,
                |url| url.starts_with("http://localhost")
            ),
            Some(true)
        );
        let claude_env = claude_live
            .get("env")
            .and_then(Value::as_object)
            .expect("claude env");
        assert!(claude_env.get("ANTHROPIC_AUTH_TOKEN").is_none());
        assert!(claude_env.get("ANTHROPIC_BASE_URL").is_none());
        assert_eq!(
            claude_env.get("ANTHROPIC_API_KEY").and_then(Value::as_str),
            Some("real-key")
        );
        assert_eq!(claude_env.get("OTHER").and_then(Value::as_str), Some("kept"));

        let mut claude_real_config = json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "real-token",
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        });
        assert_eq!(
            remove_claude_takeover_env_fields_if_present(
                &mut claude_real_config,
                placeholder,
                |url| url.starts_with("http://localhost")
            ),
            Some(false)
        );
        let claude_real_env = claude_real_config
            .get("env")
            .and_then(Value::as_object)
            .expect("claude env");
        assert_eq!(
            claude_real_env
                .get("ANTHROPIC_AUTH_TOKEN")
                .and_then(Value::as_str),
            Some("real-token")
        );
        assert_eq!(
            claude_real_env
                .get("ANTHROPIC_BASE_URL")
                .and_then(Value::as_str),
            Some("https://api.anthropic.com")
        );

        let mut claude_missing_env = json!({});
        assert_eq!(
            remove_claude_takeover_env_fields_if_present(
                &mut claude_missing_env,
                placeholder,
                |url| url.starts_with("http://localhost")
            ),
            None
        );

        let mut codex_live = json!({"auth": {"OPENAI_API_KEY": "real-key"}});
        assert!(apply_codex_takeover_auth_placeholder_if_present(
            &mut codex_live,
            placeholder
        ));
        assert_eq!(
            codex_live
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some(placeholder)
        );
        assert!(remove_codex_takeover_auth_placeholder_if_present(
            &mut codex_live,
            placeholder
        ));
        assert!(codex_live
            .get("auth")
            .and_then(|auth| auth.get("OPENAI_API_KEY"))
            .is_none());

        let mut codex_live_with_real_auth = json!({"auth": {"OPENAI_API_KEY": "real-key"}});
        assert!(!remove_codex_takeover_auth_placeholder_if_present(
            &mut codex_live_with_real_auth,
            placeholder
        ));
        assert_eq!(
            codex_live_with_real_auth
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some("real-key")
        );

        let mut codex_takeover_config = json!({
            "config": r#"model_provider = "custom"

[model_providers.custom]
base_url = "http://127.0.0.1:15721/v1"
experimental_bearer_token = "PROXY_MANAGED"
wire_api = "responses"
"#
        });
        remove_codex_takeover_config_placeholders_if_present(
            &mut codex_takeover_config,
            placeholder,
            |url| url.starts_with("http://127.0.0.1"),
        )
        .expect("cleanup codex config placeholders");
        let cleaned_config = codex_takeover_config
            .get("config")
            .and_then(Value::as_str)
            .expect("cleaned config");
        assert!(!cleaned_config.contains("base_url"));
        assert!(!cleaned_config.contains("experimental_bearer_token"));
        assert!(cleaned_config.contains("wire_api"));

        let codex_config_only = json!({
            "auth": {"OPENAI_API_KEY": placeholder},
            "config": r#"model_provider = "custom"

[model_providers.custom]
base_url = "https://relay.example/v1"
"#
        });
        let config_only_live = codex_preserved_auth_live_config_text_if_proxy_placeholder(
            &codex_config_only,
            placeholder,
            true,
        )
        .expect("config-only projection")
        .expect("placeholder auth should project config-only live text");
        assert_eq!(
            crate::codex_config::extract_codex_experimental_bearer_token(&config_only_live)
                .as_deref(),
            Some(placeholder)
        );
        assert_eq!(
            codex_preserved_auth_live_config_text_for_policy(
                &codex_config_only,
                placeholder,
                false,
                true,
            )
            .expect("disabled preservation should be a valid no-op"),
            None
        );
        assert!(
            codex_preserved_auth_live_config_text_for_policy(
                &codex_config_only,
                placeholder,
                true,
                true,
            )
            .expect("enabled preservation should be valid")
            .is_some()
        );
        assert_eq!(
            codex_preserved_auth_live_config_text_if_proxy_placeholder(
                &codex_live_with_real_auth,
                placeholder,
                true,
            )
            .expect("real auth projection should be valid"),
            None
        );

        let mut codex_live_without_auth = json!({"config": ""});
        assert!(!apply_codex_takeover_auth_placeholder_if_present(
            &mut codex_live_without_auth,
            placeholder
        ));
        assert!(codex_live_without_auth.get("auth").is_none());
        assert!(ensure_codex_takeover_auth_placeholder(
            &mut codex_live_without_auth,
            placeholder
        ));
        assert_eq!(
            codex_live_without_auth
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some(placeholder)
        );

        let mut gemini_config = json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://gemini.example",
                "GEMINI_API_KEY": "real-key",
                "OTHER": "kept"
            }
        });
        apply_gemini_takeover_env_fields(
            &mut gemini_config,
            "http://127.0.0.1:15721",
            placeholder,
        );
        let gemini_env = gemini_config
            .get("env")
            .and_then(Value::as_object)
            .expect("gemini env");
        assert_eq!(
            gemini_env
                .get("GOOGLE_GEMINI_BASE_URL")
                .and_then(Value::as_str),
            Some("http://127.0.0.1:15721")
        );
        assert_eq!(
            gemini_env.get("GEMINI_API_KEY").and_then(Value::as_str),
            Some(placeholder)
        );
        assert_eq!(gemini_env.get("OTHER").and_then(Value::as_str), Some("kept"));
        assert_eq!(
            remove_gemini_takeover_env_fields_if_present(
                &mut gemini_config,
                placeholder,
                |url| url.starts_with("http://127.0.0.1")
            ),
            Some(true)
        );
        let gemini_env = gemini_config
            .get("env")
            .and_then(Value::as_object)
            .expect("gemini env");
        assert!(gemini_env.get("GOOGLE_GEMINI_BASE_URL").is_none());
        assert!(gemini_env.get("GEMINI_API_KEY").is_none());
        assert_eq!(gemini_env.get("OTHER").and_then(Value::as_str), Some("kept"));

        let mut gemini_real_config = json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://gemini.example",
                "GEMINI_API_KEY": "real-key"
            }
        });
        assert_eq!(
            remove_gemini_takeover_env_fields_if_present(
                &mut gemini_real_config,
                placeholder,
                |url| url.starts_with("http://127.0.0.1")
            ),
            Some(false)
        );
        let gemini_real_env = gemini_real_config
            .get("env")
            .and_then(Value::as_object)
            .expect("gemini env");
        assert_eq!(
            gemini_real_env
                .get("GOOGLE_GEMINI_BASE_URL")
                .and_then(Value::as_str),
            Some("https://gemini.example")
        );
        assert_eq!(
            gemini_real_env.get("GEMINI_API_KEY").and_then(Value::as_str),
            Some("real-key")
        );

        let mut missing_env = json!({});
        assert_eq!(
            remove_gemini_takeover_env_fields_if_present(
                &mut missing_env,
                placeholder,
                |url| url.starts_with("http://127.0.0.1")
            ),
            None
        );
        apply_gemini_takeover_env_fields(
            &mut missing_env,
            "http://127.0.0.1:15721",
            placeholder,
        );
        assert_eq!(
            missing_env
                .get("env")
                .and_then(|env| env.get("GEMINI_API_KEY"))
                .and_then(Value::as_str),
            Some(placeholder)
        );
    }

    #[test]
    fn codex_live_write_projection_adapter_projects_auth_config_branches() {
        let auth = json!({"OPENAI_API_KEY": "key"});
        let write_both = codex_live_write_projection(&json!({
            "auth": auth.clone(),
            "config": "model_catalog_json = \"cc-switch-model-catalog.json\"\n"
        }))
        .expect("write auth and config");
        match write_both {
            CodexLiveWriteProjection::WriteAuthAndConfig {
                auth: projected_auth,
                config_text,
            } => {
                assert_eq!(projected_auth, auth);
                assert!(config_text.contains(
                    "model_catalog_json = \"cc-switch-model-catalog.json\""
                ));
            }
            other => panic!("unexpected projection: {other:?}"),
        }

        assert_eq!(
            codex_live_write_projection(&json!({
                "auth": {},
                "config": "model = \"gpt-5\"\n"
            }))
            .expect("delete auth and write config"),
            CodexLiveWriteProjection::DeleteAuthAndWriteConfig {
                config_text: "model = \"gpt-5\"\n".to_string(),
            }
        );
        assert_eq!(
            codex_live_write_projection(&json!({
                "auth": {"OPENAI_API_KEY": "key"}
            }))
            .expect("write auth only"),
            CodexLiveWriteProjection::WriteAuthOnly {
                auth: json!({"OPENAI_API_KEY": "key"}),
            }
        );
        assert_eq!(
            codex_live_write_projection(&json!({
                "config": "model = \"gpt-5\"\n"
            }))
            .expect("write config only"),
            CodexLiveWriteProjection::WriteConfigOnly {
                config_text: "model = \"gpt-5\"\n".to_string(),
            }
        );
        assert_eq!(
            codex_live_write_projection(&json!({})).expect("noop"),
            CodexLiveWriteProjection::Noop
        );
    }

    #[test]
    fn live_takeover_match_adapter_projects_app_specific_proxy_urls() {
        let placeholder = "PROXY_MANAGED";
        let proxy_url = "http://127.0.0.1:15721";
        let codex_proxy_url = "http://127.0.0.1:15721/v1";

        assert!(live_takeover_config_matches_proxy_for_app(
            &AppType::Claude,
            &json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": placeholder,
                    "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721/"
                }
            }),
            proxy_url,
            codex_proxy_url,
            placeholder
        ));
        assert!(!live_takeover_config_matches_proxy_for_app(
            &AppType::Claude,
            &json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": placeholder,
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
                }
            }),
            proxy_url,
            codex_proxy_url,
            placeholder
        ));

        assert!(live_takeover_config_matches_proxy_for_app(
            &AppType::Codex,
            &json!({
                "auth": {"OPENAI_API_KEY": placeholder},
                "config": r#"model_provider = "cc-switch"

[model_providers.cc-switch]
base_url = "http://127.0.0.1:15721/v1/"
"#
            }),
            proxy_url,
            codex_proxy_url,
            placeholder
        ));
        assert!(!live_takeover_config_matches_proxy_for_app(
            &AppType::Codex,
            &json!({
                "auth": {"OPENAI_API_KEY": placeholder},
                "config": r#"model_provider = "cc-switch"

[model_providers.cc-switch]
base_url = "https://relay.example/v1"
"#
            }),
            proxy_url,
            codex_proxy_url,
            placeholder
        ));

        assert!(live_takeover_config_matches_proxy_for_app(
            &AppType::Gemini,
            &json!({
                "env": {
                    "GEMINI_API_KEY": placeholder,
                    "GOOGLE_GEMINI_BASE_URL": "http://127.0.0.1:15721/"
                }
            }),
            proxy_url,
            codex_proxy_url,
            placeholder
        ));
        assert!(!live_takeover_config_matches_proxy_for_app(
            &AppType::Gemini,
            &json!({
                "env": {
                    "GEMINI_API_KEY": "real-key",
                    "GOOGLE_GEMINI_BASE_URL": "http://127.0.0.1:15721"
                }
            }),
            proxy_url,
            codex_proxy_url,
            placeholder
        ));
    }

    #[test]
    fn codex_backup_projection_adapter_preserves_mcp_and_oauth_auth() {
        assert_eq!(
            codex_backup_projection_error_message(
                CodexBackupProjectionIssue::InvalidTargetSettings
            ),
            "Codex 备份必须是 JSON 对象"
        );
        assert_eq!(
            codex_backup_projection_error_message(CodexBackupProjectionIssue::ParseTargetConfig(
                "bad target".to_string()
            )),
            "解析新的 Codex config.toml 失败: bad target"
        );
        assert_eq!(
            codex_backup_projection_error_message(CodexBackupProjectionIssue::ParseExistingConfig(
                "bad existing".to_string()
            )),
            "解析现有 Codex 备份失败: bad existing"
        );
        assert_eq!(
            codex_backup_projection_error_message(CodexBackupProjectionIssue::PrepareLiveConfig(
                "bad live".to_string()
            )),
            "更新 Codex 备份配置失败: bad live"
        );

        let oauth_auth = json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "access_token": "oauth-access"
            }
        });
        let existing_backup = json!({
            "auth": oauth_auth,
            "config": r#"[mcp_servers.shared]
command = "old-command"

[mcp_servers.legacy]
command = "legacy-command"
"#
        });
        let mut target_settings = json!({
            "auth": {
                "OPENAI_API_KEY": "provider-key"
            },
            "config": r#"model_provider = "custom"
model = "gpt-5"

[model_providers.custom]
base_url = "https://new.example/v1"
wire_api = "responses"

[mcp_servers.shared]
command = "new-command"

[mcp_servers.latest]
command = "latest-command"
"#
        });

        preserve_codex_mcp_servers_from_existing_config(&mut target_settings, &existing_backup)
            .expect("mcp merge");
        preserve_codex_oauth_auth_in_backup_if_present(&mut target_settings, &existing_backup)
            .expect("oauth auth preserve");

        assert_eq!(target_settings.get("auth"), Some(&oauth_auth));

        let config = target_settings
            .get("config")
            .and_then(Value::as_str)
            .expect("config text");
        assert_eq!(
            crate::codex_config::extract_codex_experimental_bearer_token(config).as_deref(),
            Some("provider-key")
        );

        let parsed: toml::Value = toml::from_str(config).expect("parse projected config");
        let mcp_servers = parsed.get("mcp_servers").expect("mcp_servers");
        assert_eq!(
            mcp_servers
                .get("shared")
                .and_then(|server| server.get("command"))
                .and_then(toml::Value::as_str),
            Some("new-command")
        );
        assert_eq!(
            mcp_servers
                .get("legacy")
                .and_then(|server| server.get("command"))
                .and_then(toml::Value::as_str),
            Some("legacy-command")
        );
        assert_eq!(
            mcp_servers
                .get("latest")
                .and_then(|server| server.get("command"))
                .and_then(toml::Value::as_str),
            Some("latest-command")
        );
    }

    #[test]
    fn live_token_sync_adapter_projects_app_specific_settings_updates() {
        let placeholder = "PROXY_MANAGED";

        let claude_settings = provider_settings_with_live_token_sync(
            &AppType::Claude,
            &json!({ "env": { "ANTHROPIC_AUTH_TOKEN": " fresh-claude " } }),
            &json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                    "ANTHROPIC_API_KEY": "stale"
                }
            }),
            placeholder,
        )
        .expect("claude token sync should be valid")
        .expect("claude token should sync");
        assert_eq!(
            claude_settings
                .get("env")
                .and_then(|env| env.get("ANTHROPIC_API_KEY"))
                .and_then(Value::as_str),
            Some("fresh-claude")
        );
        assert!(
            claude_settings
                .get("env")
                .and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN"))
                .is_none(),
            "Claude auth token should update existing API key field instead of adding a new one"
        );

        let codex_settings = provider_settings_with_live_token_sync(
            &AppType::Codex,
            &json!({ "auth": { "OPENAI_API_KEY": " fresh-codex " } }),
            &Value::Null,
            placeholder,
        )
        .expect("codex token sync should be valid")
        .expect("codex token should sync");
        assert_eq!(
            codex_settings
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some("fresh-codex")
        );

        let mut gemini_provider = Provider::with_id(
            "gemini-provider".to_string(),
            "Gemini Provider".to_string(),
            json!({ "env": { "GEMINI_API_KEY": "stale-gemini" } }),
            None,
        );
        assert!(
            sync_provider_settings_with_live_token(
                &AppType::Gemini,
                &json!({ "env": { "GEMINI_API_KEY": "fresh-gemini" } }),
                &mut gemini_provider,
                placeholder,
            )
            .expect("provider token sync should be valid")
        );
        assert_eq!(
            gemini_provider
                .settings_config
                .get("env")
                .and_then(|env| env.get("GEMINI_API_KEY"))
                .and_then(Value::as_str),
            Some("fresh-gemini")
        );

        assert_eq!(
            provider_settings_with_live_token_sync(
                &AppType::Gemini,
                &json!({ "env": { "GEMINI_API_KEY": "fresh-gemini" } }),
                &json!("invalid-settings"),
                placeholder,
            ),
            Err(LiveTokenProviderSettingsIssue::InvalidProviderSettings)
        );
        assert_eq!(
            provider_settings_with_live_token_sync(
                &AppType::Gemini,
                &json!({ "env": { "GEMINI_API_KEY": placeholder } }),
                &Value::Null,
                placeholder,
            )
            .expect("placeholder should be a valid no-op"),
            None
        );
    }

    #[test]
    fn claude_desktop_proxy_credentials_adapter_preserves_oauth_key_policy() {
        let proxy_provider = Provider::with_id(
            "proxy".to_string(),
            "Proxy".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-provider"
                }
            }),
            None,
        );
        assert!(provider_claude_desktop_proxy_has_base_url_and_key(
            &proxy_provider
        ));

        let missing_key = Provider::with_id(
            "missing-key".to_string(),
            "Missing Key".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.example.com"
                }
            }),
            None,
        );
        assert!(!provider_claude_desktop_proxy_has_base_url_and_key(
            &missing_key
        ));

        let mut typed_oauth = missing_key.clone();
        typed_oauth.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            ..Default::default()
        });
        assert!(provider_claude_desktop_proxy_has_base_url_and_key(
            &typed_oauth
        ));

        let url_heuristic_only = Provider::with_id(
            "chatgpt-url".to_string(),
            "ChatGPT URL".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            None,
        );
        assert!(!provider_claude_desktop_proxy_has_base_url_and_key(
            &url_heuristic_only
        ));
    }

    #[test]
    fn claude_desktop_mimo_gate_adapter_requires_anthropic_format() {
        let anthropic_provider = Provider::with_id(
            "anthropic-mimo".to_string(),
            "Anthropic MiMo".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.example.com"
                }
            }),
            None,
        );
        assert!(provider_should_normalize_mimo_anthropic_thinking_history(
            &anthropic_provider,
            "mimo-v2.5-pro"
        ));

        let endpoint_provider = Provider::with_id(
            "mimo-endpoint".to_string(),
            "MiMo Endpoint".to_string(),
            json!({
                "baseURL": "https://api.xiaomimimo.com/anthropic"
            }),
            None,
        );
        assert!(provider_should_normalize_mimo_anthropic_thinking_history(
            &endpoint_provider,
            "claude-sonnet-4-6"
        ));

        let mut openai_provider = endpoint_provider.clone();
        openai_provider.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        assert!(!provider_should_normalize_mimo_anthropic_thinking_history(
            &openai_provider,
            "mimo-v2.5-pro"
        ));
    }

    #[test]
    fn claude_desktop_validation_adapter_projects_config_issues() {
        let non_object = Provider::with_id(
            "bad-settings".to_string(),
            "Bad Settings".to_string(),
            Value::Null,
            None,
        );
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&non_object),
            Some(ClaudeDesktopDirectProviderValidationIssue::SettingsNotObject)
        );
        assert_eq!(
            provider_claude_desktop_proxy_config_validation_issue(&non_object),
            Some(ClaudeDesktopProxyProviderConfigValidationIssue::SettingsNotObject)
        );

        let mut direct_openai = Provider::with_id(
            "direct-openai".to_string(),
            "Direct OpenAI".to_string(),
            json!({}),
            None,
        );
        direct_openai.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&direct_openai),
            Some(ClaudeDesktopDirectProviderValidationIssue::ApiFormatUnsupported)
        );
        assert_eq!(
            provider_claude_desktop_proxy_config_validation_issue(&direct_openai),
            None
        );

        let mut direct_proxy_mode = direct_openai.clone();
        direct_proxy_mode.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            claude_desktop_mode: Some(crate::provider::ClaudeDesktopMode::Proxy),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&direct_proxy_mode),
            Some(ClaudeDesktopDirectProviderValidationIssue::ProxyModeUnsupported)
        );

        let mut direct_managed = direct_openai.clone();
        direct_managed.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            provider_type: Some("github_copilot".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&direct_managed),
            Some(ClaudeDesktopDirectProviderValidationIssue::ManagedProviderTypeUnsupported)
        );

        let mut direct_full_url = direct_openai.clone();
        direct_full_url.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            is_full_url: Some(true),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&direct_full_url),
            Some(ClaudeDesktopDirectProviderValidationIssue::FullUrlUnsupported)
        );

        let mut proxy_bad_format = direct_openai.clone();
        proxy_bad_format.meta = Some(ProviderMeta {
            api_format: Some("unsupported_wire".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_proxy_config_validation_issue(&proxy_bad_format),
            Some(ClaudeDesktopProxyProviderConfigValidationIssue::ApiFormatUnsupported(
                "unsupported_wire".to_string()
            ))
        );
    }

    #[test]
    fn proxy_channel_dao_adapter_projects_validation_and_legacy_projection() {
        assert_eq!(
            normalize_channel_base_url(" https://api.example.com/v1/ "),
            "https://api.example.com/v1"
        );
        assert_eq!(
            stable_channel_id("Claude", "Provider A", "legacy_primary", "https://api.example.com"),
            "legacy-claude-provider-a-legacy-primary-fcc8db014bbc"
        );

        let request = ProxyChannelWriteRequest {
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Provider A primary".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            models: vec![ProxyChannelModelWriteRequest {
                public_model: "claude-sonnet-4-6".to_string(),
                upstream_model: "anthropic/claude-sonnet-4-6".to_string(),
                ..ProxyChannelModelWriteRequest::default()
            }],
            ..ProxyChannelWriteRequest::default()
        };
        let normalized =
            normalize_proxy_channel_write_request_fields(request).expect("valid channel request");
        assert_eq!(normalized.base_url, "https://api.example.com/v1");

        let provider_projection = LegacyProviderProjectionInput {
            env: std::collections::BTreeMap::from([(
                "ANTHROPIC_MODEL".to_string(),
                "claude-sonnet-4-6".to_string(),
            )]),
            ..LegacyProviderProjectionInput::default()
        };
        let interface =
            infer_legacy_channel_interface(Some(&ProxyCoreAppKind::Claude), &provider_projection);
        assert_eq!(interface, ProxyCoreInterfaceKind::AnthropicMessages);

        let projection = build_legacy_channel_projection(LegacyChannelProjectionInput {
            app_type: "claude".to_string(),
            app: Some(ProxyCoreAppKind::Claude),
            provider_id: "provider-a".to_string(),
            provider_name: "Provider A".to_string(),
            provider_sort_index: Some(1),
            provider_in_failover_queue: false,
            base_url: "https://api.example.com/v1".to_string(),
            interface_kind: interface,
            priority: legacy_channel_priority("provider-a", false, Some("provider-a")),
            source_kind: "legacy_primary".to_string(),
            source_endpoint_url: None,
            provider_projection,
        });

        assert_eq!(projection.priority, 100);
        assert_eq!(projection.interface_kind, "anthropic_messages");
        assert_eq!(projection.models.len(), 1);
        assert!(!projection.needs_review);

        let record = proxy_channel_record_from_legacy_projection(
            projection,
            ProxyChannelSourceKind::LegacyPrimary,
        );
        assert_eq!(record.provider_id, "provider-a");
        assert_eq!(record.source_kind, ProxyChannelSourceKind::LegacyPrimary);
        assert_eq!(record.models.len(), 1);
        assert_eq!(record.models[0].channel_id, record.id);
        assert_eq!(record.models[0].public_model, "claude-sonnet-4-6");
    }

    #[test]
    fn legacy_provider_projection_input_projects_provider_settings() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "sonnet-safe".to_string(),
            ClaudeDesktopModelRoute {
                model: "claude-sonnet-4".to_string(),
                label_override: Some("Sonnet".to_string()),
                supports_1m: None,
            },
        );
        let mut provider = Provider::with_id(
            "codex-relay".to_string(),
            "Codex Relay".to_string(),
            json!({
                "config": "model_provider = \"custom\"\nmodel = \"gpt-5.4\"\n\n[model_providers.custom]\nwire_api = \"chat\"\n",
                "env": {
                    "ANTHROPIC_MODEL": "claude-sonnet-4",
                    "IGNORED_NON_STRING": 123
                },
                "modelCatalog": {
                    "models": [
                        { "model": "gpt-5.4" },
                        { "model": "gpt-5.4-mini" },
                        { "notModel": "skip" }
                    ]
                }
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            api_format: Some("openai_responses".to_string()),
            claude_desktop_model_routes: routes,
            ..ProviderMeta::default()
        });

        assert!(
            legacy_provider_config_text_from_settings(&provider.settings_config)
                .is_some_and(|config| config.contains("wire_api = \"chat\""))
        );
        assert_eq!(
            legacy_provider_env_from_settings(&provider.settings_config)
                .get("ANTHROPIC_MODEL")
                .map(String::as_str),
            Some("claude-sonnet-4")
        );
        assert_eq!(
            legacy_provider_codex_catalog_models_from_settings(&provider.settings_config),
            vec!["gpt-5.4".to_string(), "gpt-5.4-mini".to_string()]
        );

        let projection = legacy_provider_projection_input(&provider);

        assert_eq!(projection.api_format.as_deref(), Some("openai_responses"));
        assert_eq!(projection.codex_wire_api.as_deref(), Some("chat"));
        assert_eq!(projection.codex_model.as_deref(), Some("gpt-5.4"));
        assert_eq!(
            projection.codex_catalog_models,
            vec!["gpt-5.4".to_string(), "gpt-5.4-mini".to_string()]
        );
        assert_eq!(
            projection.env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("claude-sonnet-4")
        );
        assert!(!projection.env.contains_key("IGNORED_NON_STRING"));
        assert_eq!(projection.claude_desktop_model_routes.len(), 1);
        assert_eq!(
            projection.claude_desktop_model_routes[0].public_model,
            "sonnet-safe"
        );
        assert_eq!(
            projection.claude_desktop_model_routes[0].upstream_model,
            "claude-sonnet-4"
        );
    }

    #[test]
    fn copilot_account_adapter_projects_domain_and_composite_id_rules() {
        assert_eq!(COPILOT_PUBLIC_GITHUB_DOMAIN, "github.com");
        assert_eq!(default_copilot_github_domain(), "github.com");
        assert_eq!(
            normalize_github_domain("https://Company.GHE.Com/api/v3?foo=bar").unwrap(),
            "company.ghe.com"
        );
        assert!(!is_copilot_ghes_domain("github.com"));
        assert!(is_copilot_ghes_domain("company.ghe.com"));
        assert_eq!(copilot_composite_account_id("github.com", 12345), "12345");
        assert_eq!(
            copilot_composite_account_id("company.ghe.com", 12345),
            "company.ghe.com:12345"
        );
    }

    #[test]
    fn copilot_transport_adapter_projects_urls_and_model_parsing() {
        assert_eq!(
            copilot_github_client_id("github.com"),
            "Iv1.b507a08c87ecfe98"
        );
        assert_eq!(
            copilot_github_client_id("company.ghe.com"),
            "Ov23li8tweQw6odWQebz"
        );
        assert_eq!(
            copilot_github_device_code_url("company.ghe.com"),
            "https://company.ghe.com/login/device/code"
        );
        assert_eq!(
            copilot_github_oauth_token_url("company.ghe.com"),
            "https://company.ghe.com/login/oauth/access_token"
        );
        assert_eq!(
            copilot_github_user_url("github.com"),
            "https://api.github.com/user"
        );
        assert_eq!(
            copilot_token_url("company.ghe.com"),
            "https://company.ghe.com/api/v3/copilot_internal/v2/token"
        );
        assert_eq!(
            copilot_usage_url("company.ghe.com"),
            "https://company.ghe.com/api/v3/copilot_internal/user"
        );
        assert_eq!(
            copilot_api_base("company.ghe.com"),
            "https://copilot-api.company.ghe.com"
        );

        let models = parse_copilot_models_response_bytes(
            br#"{
            "data": [
                {
                    "id": "gpt-5.4",
                    "name": "GPT-5.4",
                    "vendor": "OpenAI",
                    "model_picker_enabled": true
                },
                {
                    "id": "hidden",
                    "name": "Hidden",
                    "vendor": "GitHub",
                    "model_picker_enabled": false
                }
            ]
        }"#,
        )
        .unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-5.4");
        assert_eq!(models[0].vendor, "OpenAI");
    }

    #[test]
    fn claude_desktop_model_routes_to_core_inputs_preserve_route_contract() {
        let inputs = claude_desktop_model_routes_to_core_inputs([ResolvedModelRoute {
            route_id: "claude-sonnet-4-6".to_string(),
            upstream_model: "anthropic/claude-sonnet-4-6".to_string(),
            label_override: None,
            supports_1m: true,
        }]);
        let response = ClaudeDesktopModelListResponse::from_routes(inputs.clone());

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].route_id, "claude-sonnet-4-6");
        assert!(inputs[0].supports_1m);
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].id, "claude-sonnet-4-6");
        assert!(response.data[0].supports_1m);
        assert_eq!(response.first_id.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(response.last_id.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn codex_catalog_adapter_builds_and_simplifies_model_catalog() {
        let settings = json!({
            "modelCatalog": {
                "models": [
                    {
                        "model": "kimi-k2",
                        "displayName": "Kimi K2",
                        "contextWindow": "64000"
                    }
                ]
            }
        });
        let template = json!({
            "slug": "gpt-5.5",
            "display_name": "GPT-5.5",
            "context_window": 272000,
            "model_messages": {"base": "template"}
        });

        assert!(codex_settings_have_model_catalog_specs(&settings));
        assert_eq!(codex_default_model_context_window(), 128_000);

        let catalog = codex_model_catalog_from_settings(&settings, 128_000, &template)
            .expect("catalog");
        let models = catalog
            .get("models")
            .and_then(Value::as_array)
            .expect("models");
        assert_eq!(models[0].get("slug").and_then(Value::as_str), Some("kimi-k2"));
        assert_eq!(
            models[0].get("context_window").and_then(Value::as_u64),
            Some(64_000)
        );

        let simplified = simplify_codex_model_catalog(&catalog.to_string(), 128_000)
            .expect("simplified catalog");
        assert_eq!(
            simplified["models"][0].get("model").and_then(Value::as_str),
            Some("kimi-k2")
        );
        assert_eq!(
            simplified["models"][0]
                .get("displayName")
                .and_then(Value::as_str),
            Some("Kimi K2")
        );
    }

    #[test]
    fn model_catalog_adapter_projects_provider_settings_and_client_raw() {
        let settings = json!({
            "model": " claude-sonnet-4 ",
            "env": {
                "ANTHROPIC_MODEL": "claude-opus-4"
            },
            "modelCatalog": {
                "models": [
                    {"model": "deepseek-v4"},
                    {"id": "kimi-k2"}
                ]
            }
        });

        let provider_catalog =
            provider_model_catalog_from_settings("provider-a", Some(&settings));
        assert_eq!(provider_catalog.provider_id, "provider-a");
        assert_eq!(
            provider_catalog.models,
            vec![
                "claude-opus-4".to_string(),
                "claude-sonnet-4".to_string(),
                "deepseek-v4".to_string(),
                "kimi-k2".to_string()
            ]
        );
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            settings.clone(),
            None,
        );
        assert_eq!(
            provider_model_catalog_from_provider("provider-a", Some(&provider)).models,
            provider_catalog.models
        );
        assert_eq!(
            claude_desktop_provider_from_selection_result(Ok(vec![provider.clone()]))
                .expect("selected provider")
                .id,
            "provider-a"
        );
        assert!(matches!(
            claude_desktop_provider_from_selection_result(Ok(Vec::new())),
            Err(ProxyCoreError::Unavailable(message))
                if message == "no available claude desktop provider"
        ));
        assert!(matches!(
            claude_desktop_provider_from_selection_result(Err(AppError::Message(
                "router failed".to_string()
            ))),
            Err(ProxyCoreError::Internal(message))
                if message == "select claude desktop provider: router failed"
        ));
        assert_eq!(
            provider_model_catalog_raw_value(&provider),
            settings.get("modelCatalog")
        );
        let mut live_config = json!({"auth": {}, "config": ""});
        attach_codex_model_catalog_from_provider(&mut live_config, Some(&provider));
        assert_eq!(live_config.get("modelCatalog"), settings.get("modelCatalog"));
        assert_eq!(
            codex_live_settings_with_model_catalog(
                json!({"auth": {}, "config": ""}),
                settings.get("modelCatalog").cloned()
            )
            .get("modelCatalog"),
            settings.get("modelCatalog")
        );
        assert_eq!(
            codex_live_settings_with_model_catalog(json!({"auth": {}}), None),
            json!({"auth": {}})
        );
        assert_eq!(
            codex_live_settings_with_model_catalog(
                json!("not-object"),
                Some(json!({"models": []}))
            ),
            json!("not-object")
        );

        let provider_without_catalog = Provider::with_id(
            "provider-b".to_string(),
            "Provider B".to_string(),
            json!({"config": ""}),
            None,
        );
        attach_codex_model_catalog_from_provider(
            &mut live_config,
            Some(&provider_without_catalog),
        );
        assert_eq!(
            live_config.get("modelCatalog"),
            Some(&json!({ "models": [] }))
        );

        let client_catalog = client_model_catalog_from_optional_raw(
            &AppKind::Codex,
            Some(json!({
                "models": [
                    {"id": " gpt-5 "},
                    {"model": "o4-mini"},
                    {"id": "gpt-5"}
                ]
            })),
        );
        assert_eq!(client_catalog.provider_id, "codex");
        assert_eq!(
            client_catalog.models,
            vec!["gpt-5".to_string(), "o4-mini".to_string()]
        );
        let empty_client_catalog =
            client_model_catalog_from_app_source(&AppKind::Gemini).expect("client catalog");
        assert_eq!(empty_client_catalog.provider_id, "gemini");
        assert_eq!(empty_client_catalog.models, Vec::<String>::new());
        assert_eq!(empty_client_catalog.raw, json!({"models": []}));
        assert_eq!(
            client_model_catalog_raw_from_text(r#"{"models":[{"id":"gpt-5"}]}"#),
            json!({"models":[{"id":"gpt-5"}]})
        );
        assert_eq!(client_model_catalog_raw_from_text("not json"), json!({"models": []}));
        assert_eq!(empty_client_model_catalog_raw(), json!({"models": []}));
    }

    #[test]
    fn route_plan_adapter_projects_provider_ids_and_forward_selection() {
        fn selection(channel_id: &str, provider_id: &str) -> RouteSelection {
            let provider = ProviderSpec {
                id: provider_id.to_string(),
                name: provider_id.to_string(),
                kind: ProviderKind::Claude,
                account_ref: None,
                metadata: ProviderMetadata::default(),
            };
            let channel = ChannelSpec {
                id: channel_id.to_string(),
                provider_id: provider_id.to_string(),
                app: AppKind::Claude,
                name: channel_id.to_string(),
                status: ChannelStatus::Enabled,
                endpoint: UpstreamEndpoint {
                    base_url: "https://api.example.com".to_string(),
                    path_template: None,
                    api_version: None,
                    timeout_profile: None,
                },
                interface: InterfaceKind::OpenAiChatCompletions,
                auth_profile: None,
                models: Vec::new(),
                groups: vec!["default".to_string()],
                priority: 0,
                weight: 100,
                retry_policy: RetryPolicy {
                    raw: Value::Object(Default::default()),
                },
                health_policy: ChannelHealthPolicy {
                    raw: Value::Object(Default::default()),
                },
                overrides: ChannelOverrides {
                    headers: Value::Object(Default::default()),
                    params: Value::Object(Default::default()),
                    status_code_mapping: Value::Array(Vec::new()),
                    model_mapping: Value::Object(Default::default()),
                },
                tags: Vec::new(),
                metadata: Value::Object(Default::default()),
                source_ref: None,
                needs_review: false,
                review_reasons: Vec::new(),
            };

            crate::proxy_core::api::routing::route_selection_from_parts(
                provider,
                channel,
                None,
                InterfaceKind::OpenAiChatCompletions,
            )
        }

        let primary = selection("ch-a", "provider-a");
        let plan = RoutePlan {
            selection: primary.clone(),
            selections: vec![
                primary,
                selection("ch-b", "provider-b"),
                selection("ch-c", "provider-a"),
            ],
            attempts: Vec::new(),
        };

        assert_eq!(
            route_plan_provider_ids(&plan),
            vec!["provider-a".to_string(), "provider-b".to_string()]
        );
        assert_eq!(
            route_candidate_provider_ids_from_selection_result(Ok(vec![
                Provider::with_id(
                    "provider-a".to_string(),
                    "Provider A".to_string(),
                    json!({}),
                    None,
                ),
                Provider::with_id(
                    "provider-b".to_string(),
                    "Provider B".to_string(),
                    json!({}),
                    None,
                ),
            ]))
            .expect("candidate ids"),
            vec!["provider-a".to_string(), "provider-b".to_string()]
        );
        assert_eq!(
            route_candidate_provider_ids_from_selection_result(Err(AppError::NoProvidersConfigured))
                .expect("empty no providers"),
            Vec::<String>::new()
        );
        assert_eq!(
            route_candidate_provider_ids_from_selection_result(Err(
                AppError::AllProvidersCircuitOpen
            ))
            .expect("empty circuit open"),
            Vec::<String>::new()
        );
        assert!(matches!(
            route_candidate_provider_ids_from_selection_result(Err(AppError::Message(
                "router failed".to_string()
            ))),
            Err(ProxyCoreError::Config(message))
                if message == "select route candidate providers: router failed"
        ));
        assert_eq!(
            route_selection_for_forward_result(&plan, Some("ch-b"), "provider-a").channel.id,
            "ch-b"
        );
        assert_eq!(
            route_selection_for_forward_result(&plan, Some("ch-b"), "provider-a")
                .outbound_interface,
            InterfaceKind::OpenAiChatCompletions
        );
        let selected = route_selection_for_forward_result(&plan, Some("ch-b"), "provider-a");
        let candidate = channel_route_candidate_from_selection(&selected);
        assert_eq!(candidate.channel_id, "ch-b");
        assert_eq!(candidate.route_group, "default");
        assert_eq!(candidate.source_kind, "proxy_core");
        let resolved = resolved_channel_attempt_from_candidate(candidate);
        assert_eq!(resolved.channel_id, "ch-b");
        let result = ProxyResult {
            response: ProxyCoreResponse::empty(http::StatusCode::OK),
            selected_route: selected,
            outbound_model: Some("upstream-sonnet".to_string()),
            usage_record: None,
            metadata: json!({}),
        };
        let selected_host_provider = Provider::with_id(
            "provider-b".to_string(),
            "Provider B".to_string(),
            json!({}),
            None,
        );
        let update = request_context_route_update_from_proxy_result(
            &AppType::Claude,
            &selected_host_provider,
            &result,
        );
        assert_eq!(update.outbound_model.as_deref(), Some("upstream-sonnet"));
        assert_eq!(update.usage_route_context.channel_id, "ch-b");
        assert_eq!(update.provider.id, "provider-b");
        let sourced_update = request_context_route_update_from_proxy_result_source(
            &AppType::Claude,
            AppType::Claude.as_str(),
            &result,
            "host database",
            |provider_id, app_type| {
                assert_eq!(provider_id, "provider-b");
                assert_eq!(app_type, AppType::Claude.as_str());
                Ok::<_, String>(Some(selected_host_provider.clone()))
            },
        )
        .expect("sourced route update");
        assert_eq!(sourced_update.outbound_model.as_deref(), Some("upstream-sonnet"));
        assert_eq!(sourced_update.usage_route_context.channel_id, "ch-b");
        assert_eq!(sourced_update.provider.id, "provider-b");
        let missing_sourced_update = request_context_route_update_from_proxy_result_source(
            &AppType::Claude,
            AppType::Claude.as_str(),
            &result,
            "host database",
            |_provider_id, _app_type| Ok::<_, String>(None),
        )
        .expect_err("missing provider");
        assert!(matches!(
            missing_sourced_update,
            RequestContextRouteUpdateError::ProviderMissing(message)
                if message == "selected provider is missing from host database: provider-b"
        ));
        let load_error = request_context_route_update_from_proxy_result_source(
            &AppType::Claude,
            AppType::Claude.as_str(),
            &result,
            "host database",
            |_provider_id, _app_type| Err::<Option<Provider>, _>("db failed".to_string()),
        )
        .expect_err("load error");
        assert!(matches!(
            load_error,
            RequestContextRouteUpdateError::ProviderLoad(message) if message == "db failed"
        ));
        let host_provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        let attempts = forward_attempts_from_plan(&AppType::Claude, &[host_provider], &plan);
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].provider().id, "provider-a");
        assert_eq!(attempts[1].provider().id, "provider-a");
        let missing_attempts =
            required_forward_attempts_from_plan(&AppType::Claude, &[], &plan).unwrap_err();
        assert!(matches!(
            missing_attempts,
            ProxyCoreError::Unavailable(message)
                if message == "route plan has no matching host providers"
        ));
        assert_eq!(
            forwarding_requires_runtime_error_message(),
            "cc-switch forwarding requires a proxy server runtime"
        );
        assert!(matches!(
            forwarding_runtime_unavailable_error(),
            ProxyCoreError::Unsupported(message)
                if message == "cc-switch forwarding requires a proxy server runtime"
        ));
        assert_eq!(
            route_plan_no_matching_host_providers_error_message(),
            "route plan has no matching host providers"
        );
        assert!(matches!(
            route_plan_no_matching_host_providers_error(),
            ProxyCoreError::Unavailable(message)
                if message == "route plan has no matching host providers"
        ));
        assert_eq!(
            route_plan_providers_unconfigured_error_message(),
            "route plan providers are not configured in host database"
        );
        let policy = route_policy_from_source(
            AppKind::Claude,
            vec![FailoverQueueItem {
                provider_id: "provider-b".to_string(),
                provider_name: "Provider B".to_string(),
                sort_index: Some(1),
                provider_notes: None,
            }],
        )
        .expect("route policy");
        assert_eq!(policy.app, AppKind::Claude);
        assert_eq!(policy.raw["failoverProviderIds"], json!(["provider-b"]));

        let mut body = json!({"model": "sonnet-public"});
        assert_eq!(
            apply_channel_route_model_override(
                &mut body,
                Some("sonnet-public"),
                Some("upstream-sonnet")
            )
            .as_deref(),
            Some("upstream-sonnet")
        );
        assert_eq!(body.get("model").and_then(Value::as_str), Some("upstream-sonnet"));
    }

    #[test]
    fn error_mapper_adapter_projects_error_contracts() {
        assert!(matches!(
            app_error("load config", AppError::Message("disk failed".to_string())),
            ProxyCoreError::Config(message)
                if message == "load config: disk failed"
        ));
        assert!(matches!(
            usage_error("record usage", AppError::Message("db failed".to_string())),
            ProxyCoreError::Internal(message)
                if message == "record usage: db failed"
        ));
        assert_eq!(
            selected_provider_missing_from_source_message("provider-a", "host database"),
            "selected provider is missing from host database: provider-a"
        );
        assert_eq!(
            selected_provider_not_applied_message("codex"),
            "selected provider is not available before route result is applied: codex"
        );
        assert_eq!(
            selected_provider_display_name_for_error(Some("Provider A"), "Codex"),
            "Provider A"
        );
        assert_eq!(
            selected_provider_display_name_for_error(None, "Codex"),
            "Codex"
        );
        assert_eq!(unselected_provider_fallback_id("codex"), "unselected:codex");
        assert_eq!(
            codex_proxy_error_code(CodexProxyErrorKind::ForwardFailed),
            "cc_switch_forward_failed"
        );
        let json_body = codex_proxy_error_json(CodexProxyErrorContext {
            provider_name: "Relay",
            request_model: "model-a",
            endpoint: "/responses",
            fallback_message: "failed",
            fallback_code: "cc_switch_forward_failed",
            upstream_status: None,
            upstream_body: None,
        });
        assert_eq!(json_body["error"]["provider"], "Relay");
        let facts_body = codex_proxy_error_json_from_host_facts(
            "Relay",
            "model-a",
            "/responses",
            CodexProxyHostErrorFacts {
                status: ProxyErrorStatusKind::ForwardFailed,
                message: "failed",
                kind: CodexProxyErrorKind::ForwardFailed,
                upstream_status: None,
                upstream_body: None,
            },
        );
        assert_eq!(facts_body["error"]["code"], "cc_switch_forward_failed");
        assert_eq!(facts_body["error"]["provider"], "Relay");

        let response = codex_proxy_error_response(
            ProxyErrorStatusKind::AuthError,
            CodexProxyErrorContext {
                provider_name: "Relay",
                request_model: "model-a",
                endpoint: "/responses",
                fallback_message: "bad token",
                fallback_code: "cc_switch_auth_error",
                upstream_status: None,
                upstream_body: None,
            },
        )
        .expect("codex error response");
        assert_eq!(response.status.as_u16(), 401);
        let facts_response = codex_proxy_error_response_from_host_facts(
            "Relay",
            "model-a",
            "/responses",
            CodexProxyHostErrorFacts {
                status: ProxyErrorStatusKind::AuthError,
                message: "bad token",
                kind: CodexProxyErrorKind::AuthError,
                upstream_status: None,
                upstream_body: None,
            },
        )
        .expect("codex facts error response");
        assert_eq!(facts_response.status.as_u16(), 401);

        let failure = ForwardFailureKind::Timeout("slow".to_string());
        assert!(matches!(failure, ForwardFailureKind::Timeout(message) if message == "slow"));
        assert_eq!(
            ManagementAuthError::MissingBearerToken.message(),
            "Missing management bearer token"
        );
    }

    #[test]
    fn codex_handler_adapter_projects_tool_context_and_chat_error() {
        let context = codex_tool_context_from_request(&json!({
            "tools": [
                {
                    "type": "custom",
                    "name": "apply_patch"
                }
            ]
        }));

        assert_eq!(context.chat_tools().len(), 1);
        assert!(context.is_custom_tool_chat_name("apply_patch"));

        let normalized = normalize_codex_chat_error_body(b"Unauthorized");
        assert!(normalized.non_json_body_log_message().is_some());
        assert!(normalized.response_error.get("error").is_some());
    }

    #[test]
    fn claude_body_normalization_adapter_projects_stream_and_thinking_rules() {
        let mut stream_body = json!({"stream": true});
        inject_openai_stream_include_usage(&mut stream_body);
        assert_eq!(stream_body["stream_options"]["include_usage"], true);

        let settings = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic"
            }
        });
        let mut disabled_body = json!({
            "model": "deepseek-v4-pro",
            "thinking": {"type": "disabled"},
            "output_config": {"effort": "high", "temperature": 0.2},
            "reasoning_effort": "high"
        });

        assert!(normalize_deepseek_thinking_disabled_strip_effort(
            &mut disabled_body,
            &settings
        ));
        assert!(disabled_body.get("reasoning_effort").is_none());
        assert!(disabled_body["output_config"].get("effort").is_none());
        assert_eq!(disabled_body["output_config"]["temperature"], json!(0.2));

        let mut tool_body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {}}
                ]
            }]
        });

        assert!(should_normalize_anthropic_tool_thinking_history(
            &settings,
            &tool_body,
            "anthropic"
        ));
        assert!(normalize_anthropic_tool_thinking_history(&mut tool_body));
        assert_eq!(
            tool_body["messages"][0]["content"][0]["thinking"],
            anthropic_tool_thinking_placeholder()
        );
    }

    #[test]
    fn model_mapping_adapter_projects_body_and_log_message() {
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-mapped"
                }
            }),
            None,
        );

        let projection = apply_provider_model_mapping_from_provider(
            json!({"model": "claude-sonnet", "messages": []}),
            &provider,
        );

        assert_eq!(
            projection.body.get("model").and_then(Value::as_str),
            Some("sonnet-mapped")
        );
        assert_eq!(
            projection.log_message.as_deref(),
            Some("[ModelMapper] 模型映射: claude-sonnet \u{2192} sonnet-mapped")
        );

        let unchanged = apply_provider_model_mapping_from_provider(
            json!({"model": "unknown"}),
            &Provider::with_id(
                "provider-b".to_string(),
                "Provider B".to_string(),
                json!({}),
                None,
            ),
        );
        assert_eq!(
            unchanged.body.get("model").and_then(Value::as_str),
            Some("unknown")
        );
        assert!(unchanged.log_message.is_none());

        let mut image_body = json!({
            "model": "text-model",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        let text_only_provider = Provider::with_id(
            "provider-c".to_string(),
            "Provider C".to_string(),
            json!({
                "models": [ { "id": "text-model", "input": ["text"] } ]
            }),
            None,
        );

        assert_eq!(
            replace_images_for_text_only_provider_model(&mut image_body, &text_only_provider, false),
            1
        );
        assert_eq!(image_body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn upstream_url_adapter_projects_codex_and_gemini_url_rules() {
        let (endpoint, passthrough_query) =
            rewrite_codex_responses_endpoint_to_chat("/v1/responses?foo=bar");
        assert_eq!(endpoint, "/chat/completions?foo=bar");
        assert_eq!(passthrough_query.as_deref(), Some("foo=bar"));

        let codex_plan = forward_upstream_url_plan(
            ForwardUpstreamUrlPlanInput {
                base_url: "https://api.openai.com/v1/chat/completions",
                endpoint: "/v1/responses?foo=bar&api-version=old",
                is_full_url: false,
                codex_responses_to_chat: true,
                use_claude_transform: false,
                is_copilot: false,
                claude_api_format: None,
                body: &json!({}),
                channel_param_overrides: Some(&json!({"api-version": "2026-06-21"})),
            },
            |base_url, effective_endpoint| format!("{base_url}{effective_endpoint}"),
        );
        assert_eq!(codex_plan.effective_endpoint, "/chat/completions?foo=bar&api-version=old");
        assert_eq!(codex_plan.passthrough_query.as_deref(), Some("foo=bar&api-version=old"));
        assert_eq!(
            codex_plan.url,
            "https://api.openai.com/v1/chat/completions?foo=bar&api-version=2026-06-21"
        );

        assert_eq!(
            build_gemini_native_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "/v1beta/models/gemini-2.5-pro:generateContent",
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
        );
        assert_eq!(
            resolve_gemini_native_url(
                "https://relay.example/custom/generate-content",
                "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
                true,
            ),
            "https://relay.example/custom/generate-content?alt=sse"
        );

        let gemini_plan = forward_upstream_url_plan(
            ForwardUpstreamUrlPlanInput {
                base_url: "https://relay.example/custom/generate-content",
                endpoint: "/v1/messages?beta=true",
                is_full_url: true,
                codex_responses_to_chat: false,
                use_claude_transform: true,
                is_copilot: false,
                claude_api_format: Some("gemini_native"),
                body: &json!({"model": "gemini-2.5-flash", "stream": true}),
                channel_param_overrides: None,
            },
            |base_url, effective_endpoint| format!("{base_url}{effective_endpoint}"),
        );
        assert_eq!(
            gemini_plan.effective_endpoint,
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(gemini_plan.passthrough_query.as_deref(), Some("alt=sse"));
        assert_eq!(
            gemini_plan.url,
            "https://relay.example/custom/generate-content?alt=sse"
        );
    }

    #[test]
    fn claude_api_format_adapter_projects_transform_gate() {
        assert!(!claude_api_format_needs_transform("anthropic"));
        assert!(claude_api_format_needs_transform("openai_chat"));
        assert!(claude_api_format_needs_transform("openai_responses"));
        assert!(claude_api_format_needs_transform("gemini_native"));
        assert!(!claude_api_format_needs_transform("unknown"));
    }

    #[test]
    fn upstream_request_adapter_projects_headers_and_body_serialization() {
        let mut inbound_headers = HeaderMap::new();
        inbound_headers.insert(http::header::HOST, http::HeaderValue::from_static("local"));
        inbound_headers.insert(
            http::header::ACCEPT_ENCODING,
            http::HeaderValue::from_static("gzip"),
        );

        let auth_headers = [(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer token"),
        )];
        let anthropic_beta = anthropic_beta_header_value(Some("other-beta"));
        let headers = build_upstream_request_headers(UpstreamRequestHeadersInput {
            inbound_headers: &inbound_headers,
            upstream_host: Some("upstream.example"),
            auth_headers: &auth_headers,
            channel_header_overrides: None,
            force_identity_encoding: true,
            custom_user_agent: None,
            is_copilot: false,
            should_send_anthropic_headers: true,
            anthropic_beta_value: Some(&anthropic_beta),
            codex_oauth_session_headers: &[],
            ensure_json_content_type: true,
        });

        assert_eq!(
            headers.get(http::header::HOST).and_then(|value| value.to_str().ok()),
            Some("upstream.example")
        );
        assert_eq!(
            headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer token")
        );
        assert_eq!(
            headers
                .get(http::header::ACCEPT_ENCODING)
                .and_then(|value| value.to_str().ok()),
            Some("identity")
        );
        assert_eq!(
            headers
                .get("anthropic-beta")
                .and_then(|value| value.to_str().ok()),
            Some("claude-code-20250219,other-beta")
        );
        assert_eq!(
            headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        assert!(serialize_upstream_request_body(&http::Method::GET, &json!({"model": "x"}))
            .unwrap()
            .is_empty());
        assert_eq!(
            serialize_upstream_request_body(&http::Method::POST, &json!({"model": "x"}))
                .unwrap(),
            br#"{"model":"x"}"#
        );
    }

    #[test]
    fn upstream_transport_adapter_projects_request_and_send_policy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::ACCEPT,
            http::HeaderValue::from_static("text/event-stream"),
        );

        let request_policy = resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({"model": "gpt-5"}),
            &headers,
        );
        assert!(request_policy.is_streaming_request);
        assert!(request_policy.force_identity_encoding);
        assert!(is_streaming_upstream_request(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
            &json!({"model": "gemini-2.5-pro"}),
            &HeaderMap::new()
        ));
        assert!(is_socks_proxy_url(Some("socks5://127.0.0.1:1080")));

        let send_policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: true,
            preserve_exact_header_case: true,
            request_is_streaming: true,
            non_streaming_timeout: std::time::Duration::from_secs(5),
            streaming_first_byte_timeout: std::time::Duration::from_secs(1),
        });

        assert_eq!(send_policy.transport, UpstreamTransportKind::PooledReqwest);
        assert_eq!(
            send_policy.streaming_header_timeout,
            Some(std::time::Duration::from_secs(1))
        );
    }

    #[test]
    fn codex_user_agent_adapter_projects_official_client_policy() {
        assert!(is_official_codex_client_user_agent("codex_vscode/1.0.0"));
        assert!(is_official_codex_client_user_agent("codex_cli_rs/0.5.2"));
        assert!(!is_official_codex_client_user_agent("Mozilla/5.0"));
        assert!(!is_official_codex_client_user_agent(
            "prefix_codex_cli_rs/1.0.0"
        ));
    }

    #[test]
    fn usage_record_adapter_builds_request_log_and_missing_pricing_signal() {
        let record = UsageRecord {
            request_id: Some("req-usage-1".to_string()),
            message_id: Some("msg-usage-1".to_string()),
            app: AppKind::Claude,
            provider_id: "provider-a".to_string(),
            provider_kind: Some(ProviderKind::Claude),
            channel_id: Some("channel-a".to_string()),
            channel_name: Some("Channel A".to_string()),
            route_group: Some("default".to_string()),
            request_model: "public-sonnet".to_string(),
            outbound_model: "upstream-sonnet".to_string(),
            response_model: Some("upstream-sonnet".to_string()),
            pricing_model: None,
            tokens: crate::proxy_core::api::usage::UsageTokens {
                input_tokens: 1_000,
                output_tokens: 500,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
            latency_ms: 42,
            first_token_ms: Some(7),
            status_code: 200,
            error_message: None,
            session_id: Some("session-a".to_string()),
            is_streaming: true,
            metadata: json!({}),
        };
        let pricing =
            ModelPricing::from_strings("3.0", "15.0", "0.3", "3.75").expect("pricing");
        let lookup = usage_pricing_config_lookup_from_record(&record);
        assert_eq!(lookup.provider_id, "provider-a");
        assert_eq!(lookup.app_type, "claude");
        assert_eq!(
            usage_record_pricing_model(&record, "response"),
            "upstream-sonnet"
        );

        let projection = usage_record_to_request_log(
            &record,
            "response",
            Some(&pricing),
            Decimal::new(2, 0),
            || "fallback".to_string(),
        );

        assert_eq!(projection.log.request_id, "req-usage-1");
        assert_eq!(projection.log.provider_id, "provider-a");
        assert_eq!(projection.log.app_type, "claude");
        assert_eq!(projection.log.model, "upstream-sonnet");
        assert_eq!(projection.log.request_model, "public-sonnet");
        assert_eq!(projection.log.pricing_model, "upstream-sonnet");
        assert_eq!(projection.log.usage.input_tokens, 1_000);
        assert!(projection.log.cost.is_some());
        assert_eq!(projection.log.provider_type.as_deref(), Some("claude"));
        assert_eq!(projection.log.channel_id.as_deref(), Some("channel-a"));
        assert_eq!(projection.log.channel_name.as_deref(), Some("Channel A"));
        assert_eq!(projection.log.route_group.as_deref(), Some("default"));
        assert!(projection.log.is_streaming);
        assert_eq!(projection.log.cost_multiplier, "2");

        let missing_pricing = usage_record_to_request_log(
            &record,
            "response",
            None,
            Decimal::new(1, 0),
            || "fallback".to_string(),
        );
        assert_eq!(
            missing_pricing.missing_pricing_warning_message.as_deref(),
            Some("[USG-002] 模型定价未找到，成本将记录为 0: upstream-sonnet")
        );
        assert!(missing_pricing.log.cost.is_none());
        assert_eq!(
            usage_selected_provider_missing_log_message(
                "Claude",
                UsageSelectedProviderMissingPhase::StreamingPassthrough,
            ),
            "[Claude] 跳过流式 usage 收集：ProxyEngine 尚未回填 selected provider"
        );
        assert_eq!(
            usage_selected_provider_missing_log_message(
                "Claude",
                UsageSelectedProviderMissingPhase::TransformedResponse,
            ),
            "[Claude] 跳过转换响应 usage 记录：ProxyEngine 尚未回填 selected provider"
        );
        assert_eq!(
            usage_selected_provider_missing_log_message(
                "Codex",
                UsageSelectedProviderMissingPhase::TransformedStreaming,
            ),
            "[Codex] 跳过转换流式 usage 收集：ProxyEngine 尚未回填 selected provider"
        );
        assert_eq!(
            usage_record_failure_warning_message(
                UsageRecordFailureLogContext::ForwardError,
                "db failed"
            ),
            "记录失败请求日志失败: db failed"
        );
        assert_eq!(
            usage_record_failure_warning_message(
                UsageRecordFailureLogContext::UsageRecord,
                "db failed"
            ),
            "[USG-001] 记录使用量失败: db failed"
        );
        assert_eq!(
            usage_record_debug_log_message(&record),
            "[claude] 记录请求日志: provider=provider-a, model=upstream-sonnet, streaming=true, status=200, latency_ms=42, first_token_ms=Some(7), session=session-a, input=1000, output=500, cache_read=0, cache_creation=0"
        );
        assert!(usage_logging_enabled_from_config_flag(Some(true)));
        assert!(!usage_logging_enabled_from_config_flag(Some(false)));
        assert!(usage_logging_enabled_from_config_flag(None));
    }

    #[test]
    fn claude_takeover_adapter_projects_one_m_marker_and_display_name() {
        assert_eq!(
            claude_takeover_client_model_for_upstream(
                "claude-sonnet-4-6",
                true,
                "deepseek-v4-pro[1M]"
            ),
            "claude-sonnet-4-6[1M]"
        );
        assert_eq!(
            claude_takeover_client_model_for_upstream(
                "claude-haiku-4-5",
                false,
                "deepseek-v4-flash[1M]"
            ),
            "claude-haiku-4-5"
        );
        assert_eq!(
            claude_takeover_default_display_name("deepseek-v4-ultra [1m]  "),
            "deepseek-v4-ultra"
        );

        let provider = Provider::with_id(
            "takeover-model-provider".to_string(),
            "Takeover Model Provider".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "deepseek-v4-flash",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "deepseek-v4-pro[1M]",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "DeepSeek V4 Pro",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "deepseek-v4-ultra [1m]"
                }
            }),
            None,
        );
        let fields = provider_claude_takeover_model_fields(&provider);

        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "claude-haiku-4-5".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
            "deepseek-v4-flash".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "claude-sonnet-4-6[1M]".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
            "DeepSeek V4 Pro".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "claude-opus-4-8[1M]".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "deepseek-v4-ultra".to_string()
        )));

        let mut live_config = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://old.example",
                "ANTHROPIC_AUTH_TOKEN": "old-token",
                "ANTHROPIC_MODEL": "stale-model",
                "OPENAI_API_KEY": "old-openai",
                "OTHER": "kept"
            }
        });
        apply_claude_takeover_fields_with_policy_and_models(
            &mut live_config,
            "http://127.0.0.1:15721",
            "PROXY_MANAGED",
            ClaudeTakeoverAuthPolicy::ManagedAccount {
                keep_auth_token: true,
            },
            vec![(
                "ANTHROPIC_DEFAULT_SONNET_MODEL",
                "claude-sonnet-4-6".to_string(),
            )],
        );
        let env = live_config
            .get("env")
            .and_then(Value::as_object)
            .expect("env");
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
            Some("http://127.0.0.1:15721")
        );
        assert!(env.get("ANTHROPIC_MODEL").is_none());
        assert!(env.get("OPENAI_API_KEY").is_none());
        assert_eq!(
            env.get("ANTHROPIC_API_KEY").and_then(Value::as_str),
            Some("PROXY_MANAGED")
        );
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str),
            Some("PROXY_MANAGED")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                .and_then(Value::as_str),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(env.get("OTHER").and_then(Value::as_str), Some("kept"));
    }

    #[test]
    fn host_session_adapter_generates_uuid_when_core_needs_new_session_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_proxy_session_id(&headers, &body, "claude");

        uuid::Uuid::parse_str(&result.session_id).expect("generated session id should be a UUID");
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }

    #[test]
    fn gemini_tool_call_id_adapter_uses_core_prefix_contract() {
        let id = synthesize_gemini_tool_call_id_with_uuid();

        assert!(id.starts_with(GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX));
        assert!(id.len() > GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX.len());
    }

    #[test]
    fn stream_check_adapter_preserves_reachability_fields() {
        let result = StreamCheckResult {
            status: ChannelReachabilityStatus::Degraded,
            success: true,
            message: "slow but reachable".to_string(),
            response_time_ms: Some(6100),
            http_status: Some(200),
            model_used: String::new(),
            tested_at: 1_797_000_000,
            retry_count: 1,
            error_category: None,
        };

        let reachability = stream_check_result_to_channel_reachability(result);

        assert!(reachability.success);
        assert_eq!(
            reachability.status,
            ChannelReachabilityStatus::Degraded.as_str()
        );
        assert_eq!(reachability.message, "slow but reachable");
        assert_eq!(reachability.latency_ms, Some(6100));
        assert_eq!(reachability.http_status, Some(200));
        assert_eq!(reachability.tested_at, 1_797_000_000);
        assert_eq!(reachability.retry_count, 1);

        let probe = ChannelTestProbeRequest {
            channel_id: "channel-a".to_string(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
        };
        assert_eq!(
            channel_test_app_type_from_probe_request(&probe).expect("app type"),
            AppType::Claude
        );
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            channel_test_provider_from_probe_source(&probe, Some(provider))
                .expect("provider")
                .id,
            "provider-a"
        );
        let missing_provider = channel_test_provider_from_probe_source(&probe, None)
            .expect_err("missing provider");
        assert!(matches!(
            missing_provider,
            ProxyCoreError::Config(message)
                if message == "provider not found for channel channel-a: provider-a"
        ));
        let invalid_probe = ChannelTestProbeRequest {
            app_type: "unknown-app".to_string(),
            ..probe.clone()
        };
        assert!(matches!(
            channel_test_app_type_from_probe_request(&invalid_probe),
            Err(ProxyCoreError::InvalidRequest(message))
                if message.contains("unknown-app")
        ));
        assert!(matches!(
            channel_reachability_probe_error("probe failed"),
            ProxyCoreError::Internal(message) if message == "probe failed"
        ));

        for (health_status, reachability_status) in [
            (
                ChannelReachabilityStatus::Operational,
                ChannelReachabilityStatus::Operational,
            ),
            (
                ChannelReachabilityStatus::Failed,
                ChannelReachabilityStatus::Failed,
            ),
        ] {
            let result = StreamCheckResult {
                status: health_status,
                success: false,
                message: String::new(),
                response_time_ms: None,
                http_status: None,
                model_used: String::new(),
                tested_at: 0,
                retry_count: 0,
                error_category: None,
            };

            assert_eq!(
                stream_check_result_to_channel_reachability(result).status,
                reachability_status.as_str()
            );
        }
    }

    #[test]
    fn hermes_live_import_adapter_projects_provider_settings() {
        let settings = json!({
            "apiKey": "sk-hermes",
            "baseUrl": "https://hermes.example",
            "models": {
                "fast": "claude-sonnet"
            }
        });
        let imported_provider =
            provider_from_hermes_live_config("hermes-provider", settings.clone())
                .expect("import provider");
        assert_eq!(imported_provider.id, "hermes-provider");
        assert_eq!(imported_provider.name, "hermes-provider");
        assert_eq!(imported_provider.settings_config, settings);
        assert_eq!(
            imported_provider
                .meta
                .as_ref()
                .and_then(|meta| meta.live_config_managed),
            Some(true)
        );
        assert!(matches!(
            provider_from_hermes_live_config("   ", json!({})),
            Err(HermesLiveImportIssue::EmptyName)
        ));
    }

    #[test]
    fn openclaw_live_provider_shape_adapter_projects_provider_settings() {
        let credential_provider = Provider::with_id(
            "openclaw-credentials".to_string(),
            "OpenClaw Credentials".to_string(),
            json!({
                "apiKey": "sk-openclaw",
                "baseUrl": "https://openclaw.example"
            }),
            None,
        );
        let credentials = provider_openclaw_credential_parts(&credential_provider);
        assert_eq!(credentials.api_key, Some("sk-openclaw"));
        assert_eq!(credentials.base_url, Some("https://openclaw.example"));
        let common_config = openclaw_common_config_value_from_settings(&json!({
            "apiKey": "sk-openclaw",
            "baseUrl": "https://openclaw.example",
            "api": {"chat": "/v1/chat/completions"},
            "models": {"fast": "claude-sonnet"}
        }));
        assert_eq!(
            common_config,
            json!({
                "api": {"chat": "/v1/chat/completions"},
                "models": {"fast": "claude-sonnet"}
            })
        );

        for settings in [
            json!({"baseUrl": Value::Null}),
            json!({"api": {"key": "sk-test"}}),
            json!({"models": []}),
        ] {
            let provider = Provider::with_id(
                "openclaw-provider".to_string(),
                "OpenClaw Provider".to_string(),
                settings,
                None,
            );
            assert!(provider_openclaw_has_live_provider_fields(&provider));
        }

        let typed_plan = provider_openclaw_live_write_plan(&credential_provider);
        assert!(matches!(typed_plan.config, OpenClawLiveWriteConfig::Typed(_)));

        let typed_config = serde_json::from_value::<OpenClawProviderConfig>(json!({
            "baseUrl": "https://openclaw.example",
            "apiKey": "sk-openclaw",
            "models": [
                {
                    "id": "claude-sonnet-4",
                    "name": "Claude Sonnet 4"
                }
            ]
        }))
        .expect("typed openclaw provider config");
        let imported_provider = provider_from_openclaw_live_config("anthropic", &typed_config)
            .expect("import provider");
        assert_eq!(imported_provider.id, "anthropic");
        assert_eq!(imported_provider.name, "Claude Sonnet 4");
        assert_eq!(
            imported_provider.settings_config,
            serde_json::to_value(&typed_config).expect("serialized typed config")
        );
        assert_eq!(
            imported_provider
                .meta
                .as_ref()
                .and_then(|meta| meta.live_config_managed),
            Some(true)
        );
        let unnamed_config = serde_json::from_value::<OpenClawProviderConfig>(json!({
            "models": [
                {
                    "id": "claude-sonnet-4"
                }
            ]
        }))
        .expect("typed openclaw provider config");
        let imported_unnamed_provider =
            provider_from_openclaw_live_config("anthropic", &unnamed_config)
                .expect("import provider");
        assert_eq!(imported_unnamed_provider.name, "anthropic");
        assert!(matches!(
            provider_from_openclaw_live_config("   ", &typed_config),
            Err(OpenClawLiveImportIssue::EmptyId)
        ));
        assert!(matches!(
            provider_from_openclaw_live_config("empty", &OpenClawProviderConfig {
                models: Vec::new(),
                ..typed_config.clone()
            }),
            Err(OpenClawLiveImportIssue::NoModels)
        ));

        let raw_provider = Provider::with_id(
            "raw-openclaw".to_string(),
            "Raw OpenClaw".to_string(),
            json!({
                "models": {}
            }),
            None,
        );
        let plan = provider_openclaw_live_write_plan(&raw_provider);
        match plan.config {
            OpenClawLiveWriteConfig::Raw {
                config,
                parse_error,
            } => {
                assert_eq!(config, json!({"models": {}}));
                assert!(parse_error.contains("invalid type"));
            }
            other => panic!("expected raw OpenClaw write plan, got {other:?}"),
        }
        assert!(matches!(
            provider_openclaw_live_write_projection(&raw_provider).action,
            OpenClawLiveWriteAction::Raw { .. }
        ));

        let provider = Provider::with_id(
            "invalid-provider".to_string(),
            "Invalid Provider".to_string(),
            json!({"name": "Provider"}),
            None,
        );

        assert!(!provider_openclaw_has_live_provider_fields(&provider));

        let plan = provider_openclaw_live_write_plan(&provider);
        assert!(matches!(plan.config, OpenClawLiveWriteConfig::Typed(_)));

        let provider = Provider::with_id(
            "invalid-provider".to_string(),
            "Invalid Provider".to_string(),
            json!("invalid"),
            None,
        );
        let plan = provider_openclaw_live_write_plan(&provider);
        assert!(matches!(
            plan.config,
            OpenClawLiveWriteConfig::Invalid { .. }
        ));
        match provider_openclaw_live_write_projection(&provider).action {
            OpenClawLiveWriteAction::Reject { message, .. } => {
                assert!(message.contains("OpenClaw provider 'invalid-provider'"));
                assert!(message.contains("baseUrl"));
            }
            other => panic!("expected reject OpenClaw write action, got {other:?}"),
        }
    }

    #[test]
    fn opencode_live_provider_fragment_adapter_projects_provider_settings() {
        let provider = Provider::with_id(
            "openai".to_string(),
            "OpenAI".to_string(),
            json!({
                "npm": "@ai-sdk/openai",
                "options": {"apiKey": "sk-test"}
            }),
            None,
        );
        let credentials =
            provider_opencode_credential_parts(&provider).expect("opencode credentials");
        assert_eq!(credentials.api_key, Some("sk-test"));
        assert_eq!(credentials.base_url, None);
        let common_config = opencode_common_config_value_from_settings(&json!({
            "npm": "@ai-sdk/openai",
            "options": {
                "apiKey": "sk-test",
                "baseURL": "https://opencode.example",
                "timeout": 30
            },
            "models": {"fast": "gpt-4o-mini"}
        }));
        assert_eq!(
            common_config,
            json!({
                "npm": "@ai-sdk/openai",
                "options": {"timeout": 30},
                "models": {"fast": "gpt-4o-mini"}
            })
        );
        let fragment = provider_opencode_live_provider_fragment(&provider);
        assert_eq!(fragment.config, provider.settings_config);
        assert!(!fragment.from_full_config);
        assert!(opencode_live_provider_fragment_has_provider_fields(
            &fragment.config
        ));
        let typed_config =
            serde_json::from_value::<OpenCodeProviderConfig>(provider.settings_config.clone())
                .expect("typed opencode provider config");
        let imported_provider =
            provider_from_opencode_live_config("openai", &typed_config).expect("import provider");
        assert_eq!(imported_provider.id, "openai");
        assert_eq!(imported_provider.name, "openai");
        assert_eq!(
            imported_provider.settings_config,
            serde_json::to_value(&typed_config).expect("serialized typed config")
        );
        assert_eq!(
            imported_provider
                .meta
                .as_ref()
                .and_then(|meta| meta.live_config_managed),
            Some(true)
        );
        let named_config = OpenCodeProviderConfig {
            name: Some("OpenAI".to_string()),
            ..typed_config.clone()
        };
        let imported_named_provider =
            provider_from_opencode_live_config("openai", &named_config).expect("import provider");
        assert_eq!(imported_named_provider.name, "OpenAI");
        let plan = provider_opencode_live_write_plan(&provider);
        assert!(!plan.from_full_config);
        assert!(matches!(plan.config, OpenCodeLiveWriteConfig::Typed(_)));
        assert!(opencode_live_provider_fragment_has_provider_fields(&json!({
            "npm": Value::Null
        })));
        let raw_provider = Provider::with_id(
            "raw".to_string(),
            "Raw".to_string(),
            json!({
                "npm": Value::Null
            }),
            None,
        );
        let plan = provider_opencode_live_write_plan(&raw_provider);
        match plan.config {
            OpenCodeLiveWriteConfig::Raw {
                config,
                parse_error,
            } => {
                assert_eq!(config, json!({"npm": Value::Null}));
                assert!(parse_error.contains("invalid type"));
            }
            other => panic!("expected raw OpenCode write plan, got {other:?}"),
        }
        assert!(matches!(
            provider_opencode_live_write_projection(&raw_provider).action,
            OpenCodeLiveWriteAction::Raw { .. }
        ));
        assert!(opencode_live_provider_fragment_has_provider_fields(&json!({
            "options": {}
        })));
        assert!(!opencode_live_provider_fragment_has_provider_fields(&json!({
            "name": "Provider"
        })));
        let invalid_provider = Provider::with_id(
            "invalid".to_string(),
            "Invalid".to_string(),
            json!({
                "name": "Provider"
            }),
            None,
        );
        let plan = provider_opencode_live_write_plan(&invalid_provider);
        assert!(matches!(
            plan.config,
            OpenCodeLiveWriteConfig::Invalid { .. }
        ));
        match provider_opencode_live_write_projection(&invalid_provider).action {
            OpenCodeLiveWriteAction::Reject { message, .. } => {
                assert!(message.contains("OpenCode provider 'invalid'"));
                assert!(message.contains("npm"));
            }
            other => panic!("expected reject OpenCode write action, got {other:?}"),
        }
        assert!(matches!(
            provider_opencode_credential_parts(&Provider::with_id(
                "missing-options".to_string(),
                "Missing Options".to_string(),
                json!({}),
                None,
            )),
            Err(OpenCodeCredentialIssue::MissingOptions)
        ));

        let provider = Provider::with_id(
            "openai".to_string(),
            "OpenAI".to_string(),
            json!({
                "$schema": "https://opencode.ai/config.json",
                "provider": {
                    "openai": {
                        "npm": "@ai-sdk/openai",
                        "options": {"apiKey": "sk-nested"}
                    }
                }
            }),
            None,
        );
        let fragment = provider_opencode_live_provider_fragment(&provider);
        assert_eq!(
            fragment.config,
            json!({
                "npm": "@ai-sdk/openai",
                "options": {"apiKey": "sk-nested"}
            })
        );
        assert!(fragment.from_full_config);
        let plan = provider_opencode_live_write_plan(&provider);
        assert!(plan.from_full_config);
        assert!(matches!(plan.config, OpenCodeLiveWriteConfig::Typed(_)));

        let provider = Provider::with_id(
            "missing".to_string(),
            "Missing".to_string(),
            json!({
                "$schema": "https://opencode.ai/config.json",
                "provider": {}
            }),
            None,
        );
        let fragment = provider_opencode_live_provider_fragment(&provider);
        assert_eq!(fragment.config, provider.settings_config);
        assert!(fragment.from_full_config);
    }

    #[test]
    fn provider_credentials_adapter_extracts_app_specific_values() {
        let claude = Provider::with_id(
            "claude".to_string(),
            "Claude".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token",
                    "ANTHROPIC_BASE_URL": "https://claude.example"
                }
            }),
            None,
        );
        let claude_credentials = provider_credential_values(&claude, &AppType::Claude)
            .expect("claude credentials");
        assert_eq!(claude_credentials.api_key, "token");
        assert_eq!(claude_credentials.base_url, "https://claude.example");

        let codex = Provider::with_id(
            "codex".to_string(),
            "Codex".to_string(),
            json!({
                "auth": {"OPENAI_API_KEY": "sk-test"},
                "config": "base_url = \"https://codex.example/v1\"\n"
            }),
            None,
        );
        let codex_credentials = provider_credential_values(&codex, &AppType::Codex)
            .expect("codex credentials");
        assert_eq!(codex_credentials.api_key, "sk-test");
        assert_eq!(codex_credentials.base_url, "https://codex.example/v1");

        let gemini = Provider::with_id(
            "gemini".to_string(),
            "Gemini".to_string(),
            json!({"env": {"GEMINI_API_KEY": "AIza-test"}}),
            None,
        );
        let gemini_credentials = provider_credential_values(&gemini, &AppType::Gemini)
            .expect("gemini credentials");
        assert_eq!(gemini_credentials.api_key, "AIza-test");
        assert_eq!(
            gemini_credentials.base_url,
            "https://generativelanguage.googleapis.com"
        );

        let missing_codex_base_url = Provider::with_id(
            "codex-missing-base-url".to_string(),
            "Codex Missing Base URL".to_string(),
            json!({"auth": {"OPENAI_API_KEY": "sk-test"}, "config": ""}),
            None,
        );
        assert_eq!(
            provider_credential_values(&missing_codex_base_url, &AppType::Codex),
            Err(ProviderCredentialIssue::CodexBaseUrlMissing)
        );
        let missing_base_url_spec =
            provider_credential_issue_spec(ProviderCredentialIssue::CodexBaseUrlMissing);
        assert_eq!(missing_base_url_spec.key, "provider.codex.base_url.missing");
        assert_eq!(
            missing_base_url_spec.zh,
            "config.toml 中缺少 base_url 配置"
        );
        assert_eq!(
            missing_base_url_spec.en,
            "base_url is missing from config.toml"
        );
        let missing_options_spec =
            provider_credential_issue_spec(ProviderCredentialIssue::OpenCodeOptionsMissing);
        assert_eq!(
            missing_options_spec.key,
            "provider.opencode.options.missing"
        );
        assert_eq!(
            missing_options_spec.en,
            "Invalid configuration: missing options section"
        );
    }

    #[test]
    fn claude_model_normalization_adapter_backfills_default_model_keys() {
        let mut settings = json!({
            "env": {
                "ANTHROPIC_MODEL": "claude-sonnet",
                "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-opus"
            }
        });

        assert!(normalize_claude_models_in_value(&mut settings));

        let env = settings
            .get("env")
            .and_then(Value::as_object)
            .expect("env object");
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
                .and_then(Value::as_str),
            Some("claude-haiku")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                .and_then(Value::as_str),
            Some("claude-sonnet")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_OPUS_MODEL")
                .and_then(Value::as_str),
            Some("claude-opus")
        );
        assert!(env.get("ANTHROPIC_SMALL_FAST_MODEL").is_none());

        assert!(!normalize_claude_models_in_value(&mut settings));

        let imported = provider_default_live_import_settings(
            &AppType::Claude,
            json!({
                "env": {
                    "ANTHROPIC_MODEL": "claude-sonnet",
                    "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku"
                }
            }),
        );
        assert_eq!(
            imported["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"].as_str(),
            Some("claude-haiku")
        );
        assert_eq!(
            imported["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"].as_str(),
            Some("claude-sonnet")
        );
        assert!(imported["env"].get("ANTHROPIC_SMALL_FAST_MODEL").is_none());

        let mut saved = json!({
            "env": {
                "ANTHROPIC_MODEL": "claude-sonnet",
                "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku"
            }
        });
        assert!(normalize_provider_settings_for_storage(
            &AppType::Claude,
            &mut saved
        ));
        assert_eq!(
            saved["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"].as_str(),
            Some("claude-haiku")
        );
        assert!(saved["env"].get("ANTHROPIC_SMALL_FAST_MODEL").is_none());

        let codex_settings = json!({"config": "model = \"gpt-5\""});
        assert_eq!(
            provider_default_live_import_settings(&AppType::Codex, codex_settings.clone()),
            codex_settings
        );
        let mut codex_saved = codex_settings.clone();
        assert!(!normalize_provider_settings_for_storage(
            &AppType::Codex,
            &mut codex_saved
        ));
        assert_eq!(codex_saved, codex_settings);
    }

    #[test]
    fn default_live_import_skip_policy_distinguishes_manual_and_startup() {
        assert!(should_skip_manual_default_live_import(
            &AppType::OpenCode,
            false
        ));
        assert!(should_skip_startup_default_live_import(
            &AppType::OpenCode,
            false
        ));

        assert!(!should_skip_manual_default_live_import(
            &AppType::Claude,
            false
        ));
        assert!(should_skip_manual_default_live_import(
            &AppType::Claude,
            true
        ));

        assert!(!should_skip_startup_default_live_import(
            &AppType::Claude,
            false
        ));
        assert!(should_skip_startup_default_live_import(
            &AppType::Claude,
            true
        ));
    }

    #[test]
    fn provider_live_sync_scope_uses_all_only_for_additive_apps() {
        assert_eq!(
            provider_live_sync_scope(&AppType::OpenCode),
            ProviderLiveSyncScope::AllProviders
        );
        assert_eq!(
            provider_live_sync_scope(&AppType::OpenClaw),
            ProviderLiveSyncScope::AllProviders
        );
        assert_eq!(
            provider_live_sync_scope(&AppType::Claude),
            ProviderLiveSyncScope::CurrentProvider
        );
        assert_eq!(
            provider_live_sync_scope(&AppType::ClaudeDesktop),
            ProviderLiveSyncScope::CurrentProvider
        );
    }

    #[test]
    fn provider_current_provider_scope_excludes_additive_apps() {
        assert!(!provider_app_has_current_provider(&AppType::OpenCode));
        assert!(!provider_app_has_current_provider(&AppType::OpenClaw));
        assert!(provider_app_has_current_provider(&AppType::Claude));
        assert!(provider_app_has_current_provider(&AppType::Codex));
    }

    #[test]
    fn provider_live_sync_includes_unknown_and_managed_providers() {
        let mut provider = Provider::with_id(
            "sync-provider".to_string(),
            "Sync Provider".to_string(),
            json!({}),
            None,
        );
        assert!(provider_should_sync_to_live(&provider));

        provider.meta = Some(ProviderMeta {
            live_config_managed: Some(true),
            ..Default::default()
        });
        assert!(provider_should_sync_to_live(&provider));

        provider.meta = Some(ProviderMeta {
            live_config_managed: Some(false),
            ..Default::default()
        });
        assert!(!provider_should_sync_to_live(&provider));
    }

    #[test]
    fn provider_initial_live_config_managed_marker_only_applies_to_additive_apps() {
        assert_eq!(
            provider_initial_live_config_managed_marker(&AppType::OpenCode, true),
            Some(true)
        );
        assert_eq!(
            provider_initial_live_config_managed_marker(&AppType::OpenClaw, false),
            Some(false)
        );
        assert_eq!(
            provider_initial_live_config_managed_marker(&AppType::Claude, true),
            None
        );
    }

    #[test]
    fn provider_legacy_common_config_migration_skips_additive_and_empty_snippets() {
        assert!(provider_supports_legacy_common_config_migration(
            &AppType::Claude
        ));
        assert!(!provider_supports_legacy_common_config_migration(
            &AppType::OpenCode
        ));
        assert!(!should_skip_provider_legacy_common_config_migration(
            &AppType::Claude,
            "legacy = true"
        ));
        assert!(should_skip_provider_legacy_common_config_migration(
            &AppType::Claude,
            "  \n  "
        ));
        assert!(should_skip_provider_legacy_common_config_migration(
            &AppType::OpenClaw,
            "legacy = true"
        ));
    }

    #[test]
    fn provider_live_config_presence_error_policy_tolerates_db_only_providers() {
        assert_eq!(
            provider_live_config_presence_error_policy(None),
            ProviderLiveConfigPresenceErrorPolicy::Strict
        );
        assert_eq!(
            provider_live_config_presence_error_policy(Some(true)),
            ProviderLiveConfigPresenceErrorPolicy::Strict
        );
        assert_eq!(
            provider_live_config_presence_error_policy(Some(false)),
            ProviderLiveConfigPresenceErrorPolicy::TreatErrorAsMissing
        );
    }

    #[test]
    fn provider_key_change_policy_blocks_non_additive_and_omo_providers() {
        assert_eq!(
            provider_key_change_policy_issue(&AppType::Claude, None),
            Some(ProviderKeyChangePolicyIssue::UnsupportedAppMode)
        );

        let mut omo_provider = Provider::with_id(
            "omo-provider".to_string(),
            "OMO Provider".to_string(),
            json!({}),
            None,
        );
        omo_provider.category = Some("omo".to_string());
        assert_eq!(
            provider_key_change_policy_issue(&AppType::OpenCode, Some(&omo_provider)),
            Some(ProviderKeyChangePolicyIssue::ExclusiveCurrentStateProvider)
        );

        let mut custom_provider = Provider::with_id(
            "custom-provider".to_string(),
            "Custom Provider".to_string(),
            json!({}),
            None,
        );
        custom_provider.category = Some("custom".to_string());
        assert_eq!(
            provider_key_change_policy_issue(&AppType::OpenCode, Some(&custom_provider)),
            None
        );
        assert_eq!(
            provider_key_change_policy_issue(&AppType::OpenClaw, None),
            None
        );
        assert_eq!(
            provider_key_change_policy_issue_message(
                ProviderKeyChangePolicyIssue::UnsupportedAppMode
            ),
            "Only additive-mode providers support changing provider key"
        );
    }

    #[test]
    fn provider_additive_live_write_action_skips_omo_and_unrequested_writes() {
        let mut omo_provider = Provider::with_id(
            "omo-provider".to_string(),
            "OMO Provider".to_string(),
            json!({}),
            None,
        );
        omo_provider.category = Some("omo-slim".to_string());
        assert_eq!(
            provider_additive_live_write_action(&AppType::OpenCode, &omo_provider, true),
            ProviderAdditiveLiveWriteAction::SkipExclusiveCurrentStateProvider
        );

        let custom_provider = Provider::with_id(
            "custom-provider".to_string(),
            "Custom Provider".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            provider_additive_live_write_action(&AppType::OpenCode, &custom_provider, false),
            ProviderAdditiveLiveWriteAction::SkipNotRequested
        );
        assert_eq!(
            provider_additive_live_write_action(&AppType::OpenClaw, &custom_provider, true),
            ProviderAdditiveLiveWriteAction::Write
        );
    }

    #[test]
    fn provider_omo_switch_pair_maps_enable_and_disable_variants() {
        let mut standard_provider = Provider::with_id(
            "omo-provider".to_string(),
            "OMO Provider".to_string(),
            json!({}),
            None,
        );
        standard_provider.category = Some("omo".to_string());
        assert_eq!(
            provider_omo_variant_for_category(&AppType::OpenCode, Some("omo")),
            Some(ProviderOmoVariant::Standard)
        );
        assert_eq!(
            provider_omo_switch_pair(&AppType::OpenCode, &standard_provider),
            Some(ProviderOmoSwitchPair {
                enable: ProviderOmoVariant::Standard,
                disable: ProviderOmoVariant::Slim,
            })
        );

        let mut slim_provider = standard_provider.clone();
        slim_provider.category = Some("omo-slim".to_string());
        assert_eq!(
            provider_omo_variant_for_category(&AppType::OpenCode, Some("omo-slim")),
            Some(ProviderOmoVariant::Slim)
        );
        assert_eq!(
            provider_omo_switch_pair(&AppType::OpenCode, &slim_provider),
            Some(ProviderOmoSwitchPair {
                enable: ProviderOmoVariant::Slim,
                disable: ProviderOmoVariant::Standard,
            })
        );

        let custom_provider = Provider::with_id(
            "custom-provider".to_string(),
            "Custom Provider".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            provider_omo_switch_pair(&AppType::OpenCode, &custom_provider),
            None
        );
        assert_eq!(
            provider_omo_variant_for_category(&AppType::OpenCode, Some("custom")),
            None
        );
        assert_eq!(
            provider_omo_switch_pair(&AppType::Claude, &standard_provider),
            None
        );
        assert_eq!(
            provider_omo_variant_for_category(&AppType::Claude, Some("omo")),
            None
        );
    }

    #[test]
    fn provider_switch_dispatch_routes_exclusive_and_desktop_to_normal_flow() {
        let mut omo_provider = Provider::with_id(
            "omo-provider".to_string(),
            "OMO Provider".to_string(),
            json!({}),
            None,
        );
        omo_provider.category = Some("omo".to_string());
        assert_eq!(
            provider_switch_dispatch(&AppType::OpenCode, &omo_provider),
            ProviderSwitchDispatch::Normal
        );

        let normal_provider = Provider::with_id(
            "normal-provider".to_string(),
            "Normal Provider".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            provider_switch_dispatch(&AppType::ClaudeDesktop, &normal_provider),
            ProviderSwitchDispatch::Normal
        );
        assert_eq!(
            provider_switch_dispatch(&AppType::OpenCode, &normal_provider),
            ProviderSwitchDispatch::TakeoverAware
        );
        assert_eq!(
            provider_switch_dispatch(&AppType::Claude, &normal_provider),
            ProviderSwitchDispatch::TakeoverAware
        );

        assert!(provider_switch_requires_takeover_lock(&AppType::Claude));
        assert!(provider_switch_requires_takeover_lock(&AppType::Codex));
        assert!(provider_switch_requires_takeover_lock(&AppType::Gemini));
        assert!(!provider_switch_requires_takeover_lock(
            &AppType::ClaudeDesktop
        ));
        assert!(!provider_switch_requires_takeover_lock(&AppType::OpenCode));
        assert!(!provider_switch_requires_takeover_lock(&AppType::OpenClaw));
        assert!(!provider_switch_requires_takeover_lock(&AppType::Hermes));
    }

    #[test]
    fn provider_takeover_live_sync_target_keeps_desktop_on_live_config() {
        assert_eq!(
            provider_takeover_live_sync_target(&AppType::ClaudeDesktop),
            ProviderTakeoverLiveSyncTarget::LiveConfig
        );
        assert_eq!(
            provider_takeover_live_sync_target(&AppType::Claude),
            ProviderTakeoverLiveSyncTarget::LiveBackup
        );
        assert_eq!(
            provider_takeover_live_sync_target(&AppType::Codex),
            ProviderTakeoverLiveSyncTarget::LiveBackup
        );
        assert_eq!(
            provider_takeover_live_sync_target(&AppType::Gemini),
            ProviderTakeoverLiveSyncTarget::LiveBackup
        );
        assert_eq!(
            provider_takeover_live_sync_target(&AppType::OpenCode),
            ProviderTakeoverLiveSyncTarget::LiveBackup
        );
    }

    #[test]
    fn provider_live_removal_target_only_covers_additive_live_configs() {
        assert_eq!(
            provider_live_removal_target(&AppType::OpenCode),
            Some(ProviderLiveRemovalTarget::OpenCode)
        );
        assert_eq!(
            provider_live_removal_target(&AppType::OpenClaw),
            Some(ProviderLiveRemovalTarget::OpenClaw)
        );
        assert_eq!(
            provider_live_removal_target(&AppType::Hermes),
            Some(ProviderLiveRemovalTarget::Hermes)
        );
        assert_eq!(provider_live_removal_target(&AppType::Claude), None);
        assert_eq!(provider_live_removal_target(&AppType::ClaudeDesktop), None);
        assert_eq!(provider_live_removal_target(&AppType::Codex), None);
        assert_eq!(provider_live_removal_target(&AppType::Gemini), None);
    }

    #[test]
    fn provider_delete_is_current_provider_checks_local_and_db_sources() {
        assert!(provider_delete_is_current_provider(
            "provider-a",
            Some("provider-a"),
            None
        ));
        assert!(provider_delete_is_current_provider(
            "provider-a",
            None,
            Some("provider-a")
        ));
        assert!(provider_delete_is_current_provider(
            "provider-a",
            Some("provider-a"),
            Some("provider-a")
        ));
        assert!(!provider_delete_is_current_provider(
            "provider-a",
            Some("provider-b"),
            Some("provider-c")
        ));
        assert!(!provider_delete_is_current_provider("provider-a", None, None));
    }

    #[test]
    fn provider_additive_update_route_keeps_omo_separate_from_live_presence() {
        assert_eq!(
            provider_additive_update_route(&AppType::OpenCode, Some("omo")),
            Some(ProviderAdditiveUpdateRoute::OmoVariant(
                ProviderOmoVariant::Standard
            ))
        );
        assert_eq!(
            provider_additive_update_route(&AppType::OpenCode, Some("omo-slim")),
            Some(ProviderAdditiveUpdateRoute::OmoVariant(
                ProviderOmoVariant::Slim
            ))
        );
        assert_eq!(
            provider_additive_update_route(&AppType::OpenCode, Some("custom")),
            Some(ProviderAdditiveUpdateRoute::LiveConfigPresence)
        );
        assert_eq!(
            provider_additive_update_route(&AppType::OpenClaw, None),
            Some(ProviderAdditiveUpdateRoute::LiveConfigPresence)
        );
        assert_eq!(
            provider_additive_update_route(&AppType::Hermes, None),
            Some(ProviderAdditiveUpdateRoute::LiveConfigPresence)
        );
        assert_eq!(
            provider_additive_update_route(&AppType::Claude, Some("omo")),
            None
        );
    }

    #[test]
    fn provider_switch_backfill_source_id_requires_exclusive_different_current() {
        assert_eq!(
            provider_switch_backfill_source_id(&AppType::Claude, Some("current"), "target"),
            Some("current")
        );
        assert_eq!(
            provider_switch_backfill_source_id(&AppType::Claude, Some("target"), "target"),
            None
        );
        assert_eq!(
            provider_switch_backfill_source_id(&AppType::Claude, None, "target"),
            None
        );
        assert_eq!(
            provider_switch_backfill_source_id(&AppType::OpenCode, Some("current"), "target"),
            None
        );
    }

    #[test]
    fn provider_switch_should_mark_live_config_managed_only_for_unmanaged_additive() {
        assert!(provider_switch_should_mark_live_config_managed(
            &AppType::OpenCode,
            None
        ));
        assert!(provider_switch_should_mark_live_config_managed(
            &AppType::OpenCode,
            Some(false)
        ));
        assert!(!provider_switch_should_mark_live_config_managed(
            &AppType::OpenCode,
            Some(true)
        ));
        assert!(!provider_switch_should_mark_live_config_managed(
            &AppType::Claude,
            None
        ));
    }

    #[test]
    fn channel_health_adapter_projects_attempt_db_update() {
        let reset_plan =
            channel_health_reset_plan_from_lookup("channel-a", Some("claude".to_string()))
                .expect("reset plan");
        assert_eq!(reset_plan.channel_id, "channel-a");
        assert_eq!(reset_plan.app_type, "claude");
        let reset = channel_health_reset_from_plan(reset_plan);
        assert_eq!(reset.channel_id, "channel-a");
        assert_eq!(reset.app, AppKind::Claude);
        let missing = channel_health_reset_plan_from_lookup("missing-channel", None)
            .expect_err("missing channel should fail");
        assert!(missing.to_string().contains("missing-channel"));

        let update = channel_health_attempt_db_update(ChannelAttemptResult {
            channel_id: "channel-a".to_string(),
            success: false,
            status_code: Some(429),
            latency_ms: Some(123),
            error_code: Some("rate_limited".to_string()),
        });

        assert_eq!(update.channel_id, "channel-a");
        assert!(!update.success);
        assert_eq!(update.error_code.as_deref(), Some("rate_limited"));
        assert_eq!(
            update.failure_threshold,
            DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD
        );
        assert_eq!(update.response_time_ms, Some(123));
    }

    #[test]
    fn provider_conversion_uses_inferred_provider_kind_without_settings_leak() {
        let mut provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "secret-token",
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com",
                    "CLAUDE_CODE_USE_BEDROCK": "1"
                }
            }),
            Some("https://github.com/features/copilot".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            is_full_url: Some(true),
            custom_user_agent: Some("cc-switch-test/1.0".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("github_copilot".to_string()),
                account_id: Some("acct-1".to_string()),
            }),
            usage_script: Some(UsageScript {
                enabled: true,
                language: "javascript".to_string(),
                code: String::new(),
                timeout: None,
                api_key: None,
                base_url: None,
                access_token: None,
                user_id: None,
                template_type: Some("github_copilot".to_string()),
                auto_query_interval: None,
                coding_plan_provider: None,
            }),
            test_config: Some(ProviderTestConfig {
                enabled: true,
                timeout_secs: Some(20),
                degraded_threshold_ms: Some(3000),
                max_retries: None,
            }),
            ..ProviderMeta::default()
        });

        let usage_provider_kind = provider_kind_from_provider(&provider);
        let usage_provider_is_codex_oauth = provider_is_codex_oauth(&provider);
        let usage_provider_is_github_copilot = provider_is_github_copilot(&provider);
        let usage_provider_uses_managed_account = provider_uses_managed_account_auth(&provider);
        let usage_provider_is_copilot =
            provider_is_github_copilot_upstream(&provider, "https://example.com");
        let stream_check_provider_is_copilot =
            provider_is_github_copilot_stream_check_target(&provider);
        let copilot_account_id = provider_github_copilot_managed_account_id(&provider);
        let has_claude_env = provider_claude_env_settings(&provider).is_some();
        let models_are_claude_safe = provider_claude_models_are_claude_safe(&provider);
        let supports_1m_by_default =
            provider_claude_desktop_routes_support_1m_by_default(&provider);
        let stream_check_timeout_secs =
            provider_stream_check_test_config(&provider).and_then(|config| config.timeout_secs);
        assert_eq!(
            provider_usage_script(Some(&provider)).and_then(|script| script.template_type.as_deref()),
            Some("github_copilot")
        );
        assert!(provider_usage_script(None).is_none());
        let usage_provider_is_full_url = provider_is_full_url(&provider);
        let provider_user_agent =
            provider_custom_user_agent_header(&provider, false).expect("custom user agent");
        let copilot_provider_user_agent = provider_custom_user_agent_header(&provider, true);
        assert_eq!(provider_bedrock_env_flag(&provider), Some("1"));
        let mut codex_provider = Provider::with_id(
            "codex-oauth".to_string(),
            "Codex OAuth".to_string(),
            json!({}),
            None,
        );
        codex_provider.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("codex_oauth".to_string()),
                account_id: Some("codex-acct-1".to_string()),
            }),
            ..ProviderMeta::default()
        });
        let mut claude_auth_provider = Provider::with_id(
            "claude-auth".to_string(),
            "Claude Auth".to_string(),
            json!({}),
            None,
        );
        claude_auth_provider.meta = Some(ProviderMeta {
            provider_type: Some("claude_auth".to_string()),
            ..ProviderMeta::default()
        });

        let spec = provider.to_proxy_core_provider_spec(&AppType::Claude);
        let source_spec = provider_spec_from_source(&AppKind::Claude, Some(provider.clone()))
            .expect("provider spec")
            .expect("provider");
        let source_specs =
            provider_specs_from_source(&AppKind::Claude, vec![provider]).expect("provider specs");

        assert_eq!(spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(source_spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(source_specs[0].kind, ProviderKind::GitHubCopilot);
        assert_eq!(usage_provider_kind, Some(ProviderKind::GitHubCopilot));
        assert!(!usage_provider_is_codex_oauth);
        assert!(usage_provider_is_github_copilot);
        assert!(usage_provider_uses_managed_account);
        assert!(usage_provider_is_copilot);
        assert!(stream_check_provider_is_copilot);
        assert_eq!(copilot_account_id.as_deref(), Some("acct-1"));
        assert!(has_claude_env);
        assert!(models_are_claude_safe);
        assert!(!supports_1m_by_default);
        assert_eq!(stream_check_timeout_secs, Some(20));
        assert!(usage_provider_is_full_url);
        assert_eq!(provider_user_agent, http::HeaderValue::from_static("cc-switch-test/1.0"));
        assert!(copilot_provider_user_agent.is_none());
        assert!(provider_is_github_copilot_upstream(
            &Provider::with_id(
                "plain".to_string(),
                "Plain".to_string(),
                json!({}),
                None,
            ),
            "https://api.githubcopilot.com"
        ));
        assert!(provider_is_codex_oauth(&codex_provider));
        assert_eq!(
            provider_codex_oauth_managed_account_id(&codex_provider).as_deref(),
            Some("codex-acct-1")
        );
        assert!(provider_uses_anthropic_rectifiers(
            &AppType::Claude,
            &claude_auth_provider
        ));
        assert!(!provider_uses_anthropic_rectifiers(
            &AppType::Codex,
            &claude_auth_provider
        ));
        assert_eq!(spec.account_ref.as_deref(), Some("github_copilot:acct-1"));
        let serialized = serde_json::to_string(&spec).expect("serialize spec");
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("ANTHROPIC_AUTH_TOKEN"));
        assert!(!serialized.contains("settingsConfig"));
    }

    #[test]
    fn channel_conversion_preserves_endpoint_interface_models_and_groups() {
        let channel = ProxyChannelRecord {
            id: "ch-1".to_string(),
            provider_id: "provider-1".to_string(),
            app_type: "claude".to_string(),
            name: "Relay A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_chat_completions".to_string(),
            auth_profile_ref: Some("provider:claude:provider-1".to_string()),
            groups: vec!["default".to_string(), "paid".to_string()],
            priority: 50,
            weight: 80,
            retry_policy: json!({"maxAttempts": 2}),
            health_policy: json!({"breaker": "standard"}),
            header_overrides: json!({"x-test": "1"}),
            param_overrides: json!({"stream": true}),
            status_code_mapping: json!([{"from": 429, "to": 503}]),
            tags: vec!["manual".to_string()],
            metadata: json!({"owner": "ops"}),
            source_kind: ProxyChannelSourceKind::Manual,
            source_endpoint_url: Some("https://relay.example.com/v1".to_string()),
            models: vec![ProxyChannelModelRecord {
                channel_id: "ch-1".to_string(),
                public_model: "sonnet".to_string(),
                upstream_model: "anthropic/sonnet".to_string(),
                capabilities: json!({"tools": true}),
                pricing_model: Some("standard".to_string()),
                request_overrides: json!({"temperature": 0.2}),
                response_overrides: json!({}),
            }],
            needs_review: false,
            review_reasons: Vec::new(),
        };

        let spec = channel.to_proxy_core_channel_spec();
        let source_spec = channel_spec_from_source(Some(channel.clone())).expect("channel spec");
        let (materialized_channels, materialized_source) =
            channel_route_records_from_sources(vec![channel.clone()], || {
                panic!("materialized channels must not load legacy projection")
            })
            .expect("materialized route records");
        let (legacy_channels, legacy_source) =
            channel_route_records_from_sources(Vec::new(), || {
                Ok(ProxyChannelMigrationPreview {
                    app_type: "claude".to_string(),
                    channels: vec![channel.clone()],
                    duplicate_count: 0,
                    needs_review_count: 0,
                })
            })
            .expect("legacy route records");

        assert_eq!(source_spec.id, "ch-1");
        assert_eq!(materialized_source, ChannelRouteSource::MaterializedChannels);
        assert_eq!(materialized_channels[0].id, "ch-1");
        assert_eq!(legacy_source, ChannelRouteSource::LegacyProjection);
        assert_eq!(legacy_channels[0].id, "ch-1");
        assert_eq!(spec.app, AppKind::Claude);
        assert_eq!(spec.endpoint.base_url, "https://relay.example.com/v1");
        assert_eq!(spec.interface, InterfaceKind::OpenAiChatCompletions);
        assert_eq!(spec.groups, vec!["default".to_string(), "paid".to_string()]);
        assert_eq!(spec.models.len(), 1);
        assert_eq!(spec.models[0].public_model, "sonnet");
        assert_eq!(spec.models[0].upstream_model, "anthropic/sonnet");
        assert_eq!(
            spec.auth_profile.as_ref().map(|value| value.0.as_str()),
            Some("provider:claude:provider-1")
        );
    }
}
