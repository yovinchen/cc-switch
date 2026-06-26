use crate::app_config::AppType;
use crate::database::{
    Database, FailoverQueueItem, ProxyChannelMigrationPreview, ProxyChannelModelRecord,
    ProxyChannelRecord, ProxyChannelSourceKind,
};
use crate::error::AppError;
use crate::openclaw_config::OpenClawProviderConfig;
use crate::provider::{
    AuthBindingSource, OpenCodeProviderConfig, Provider, ProviderMeta, ProviderTestConfig,
    UsageScript,
};
use crate::proxy::codex_chat_history::{record_responses_sse_stream, CodexChatHistoryStore};
use crate::proxy::engine::routing::{ProviderFailoverRouterSources, ProviderRouter};
use crate::proxy::error::ProxyError;
use crate::proxy::error_mapper::forward_error_to_core_error;
pub(crate) use crate::proxy::error_mapper::proxy_core_error_to_proxy_error;
pub(crate) use crate::proxy::error_mapper::{
    proxy_error_display_message, proxy_error_status_code, proxy_error_status_kind,
};
use crate::proxy::events::ProxyEventBus;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::channel_health_store::CcSwitchChannelHealthStore;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::claude_desktop_gateway_auth_source::CcSwitchClaudeDesktopGatewayAuthSource;
pub(crate) use crate::proxy::host::cc_switch::config_source::CcSwitchConfigSource;
use crate::proxy::host::cc_switch::database_usage_sink::RequestLog;
use crate::proxy::host::cc_switch::failover_switch::FailoverSwitchManager;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::forward_pipeline::CcSwitchForwardPipeline;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::management_auth_source::CcSwitchManagementAuthSource;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::model_catalog_provider::CcSwitchModelCatalogProvider;
pub(crate) use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::provider_source::CcSwitchProviderSource;
pub(crate) use crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime;
pub(crate) use crate::proxy::host::cc_switch::proxy_services::CcSwitchProxyServices;
pub(crate) use crate::proxy::host::cc_switch::proxy_state::ProxyState;
pub(crate) use crate::proxy::host::cc_switch::route_policy_source::CcSwitchRoutePolicySource;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::route_resolver::CcSwitchRouteResolver;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::runtime_status_source::CcSwitchRuntimeStatusSource;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy::transport::http::server::ProxyServer;
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
use crate::proxy::RequestForwarder;
#[cfg(test)]
use crate::proxy_core::api::domain::{ChannelHealthPolicy, ChannelOverrides, UpstreamEndpoint};
use crate::proxy_core::api::domain::{ProviderMetadata, ProviderMetadataInput};
pub(crate) use crate::proxy_core::api::management::channel_route_source_for_materialized_count;
#[cfg(test)]
use crate::proxy_core::api::routing::RouteResolveModelInput;
use crate::proxy_core::api::routing::{
    route_resolve_channel_input_from_record, RouteResolveChannelRecordInput,
    RouteResolveModelRecordInput,
};
use crate::proxy_core::api::session::SessionIdResult;
use crate::settings::CustomEndpoint;
use bytes::Bytes;
use futures::{future::BoxFuture, Stream, StreamExt};
use http::{HeaderMap, Method};
#[cfg(test)]
use indexmap::IndexMap;
use rust_decimal::Decimal;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub(crate) const COPILOT_EDITOR_VERSION: &str = "vscode/1.110.1";
pub(crate) const COPILOT_PLUGIN_VERSION: &str = "copilot-chat/0.38.2";
pub(crate) const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.38.2";
pub(crate) const COPILOT_API_VERSION: &str = "2025-10-01";
pub(crate) const COPILOT_INTEGRATION_ID: &str = "vscode-chat";

pub(crate) type CcSwitchProxyRuntimeServices = CcSwitchProxyServices<CcSwitchProxyRuntime>;

impl ProxyState {
    pub(crate) fn proxy_engine(&self) -> ProxyEngine<CcSwitchProxyRuntimeServices> {
        ProxyEngine::new(self.proxy_core_services.clone())
    }
}

pub(crate) fn synthesize_gemini_tool_call_id_with_uuid() -> String {
    crate::proxy_core::api::transforms::synthesize_gemini_tool_call_id(
        Uuid::new_v4().simple().to_string(),
    )
}

fn log_rectified_gemini_tool_args(name: &str) {
    log::info!("[Claude/Gemini] Rectified tool args for `{name}`");
}

#[cfg(test)]
pub(crate) type ClaudeDesktopGatewayAuthError =
    crate::proxy_core::api::auth::ClaudeDesktopGatewayAuthError;

pub(crate) use crate::proxy_core::api::auth::{
    ClaudeDesktopDirectProviderValidationIssue, ClaudeDesktopProxyProviderConfigValidationIssue,
};

pub(crate) use crate::proxy_core::api::errors::{
    config_error_with_context as core_config_error_with_context,
    internal_error_with_context as core_internal_error_with_context,
    invalid_request_error as core_invalid_request_error,
    selected_provider_missing_from_source_message,
};
#[cfg(test)]
use crate::proxy_core::api::errors::{
    selected_provider_display_name_for_error, selected_provider_not_applied_message,
    unselected_provider_fallback_id,
};

pub(crate) fn app_error(context: &str, error: AppError) -> ProxyCoreError {
    core_config_error_with_context(context, error)
}

pub(crate) fn app_write_error(context: &str, error: AppError) -> ProxyCoreError {
    match error {
        AppError::InvalidInput(message) => {
            core_invalid_request_error(AppError::InvalidInput(message))
        }
        other => app_error(context, other),
    }
}

pub(crate) fn usage_error(context: &str, error: AppError) -> ProxyCoreError {
    core_internal_error_with_context(context, error)
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
            log::warn!(
                "{}",
                forwarder_all_providers_circuit_open_log_line(app_type)
            );
            AppError::AllProvidersCircuitOpen
        }
        ProviderSelectionFailure::NoProvidersConfigured => {
            log::warn!("{}", forwarder_no_providers_configured_log_line(app_type));
            AppError::NoProvidersConfigured
        }
    }
}

pub(crate) fn provider_selection_failure_from_app_error(
    error: &AppError,
) -> Option<ProviderSelectionFailure> {
    match error {
        AppError::AllProvidersCircuitOpen => {
            Some(ProviderSelectionFailure::AllProvidersCircuitOpen)
        }
        AppError::NoProvidersConfigured => Some(ProviderSelectionFailure::NoProvidersConfigured),
        _ => None,
    }
}

pub(crate) const SYSTEM_PROXY_ENV_KEYS: [&str; 6] =
    crate::proxy_core::api::transport::SYSTEM_PROXY_ENV_KEYS;

pub(crate) use crate::proxy_core::api::security::mask_url_for_log;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::proxy_url_points_to_loopback_port;

pub(crate) use crate::proxy_core::api::transport::{
    invalid_explicit_proxy_url_message, proxy_values_point_to_loopback_port,
    validate_explicit_proxy_url,
};

pub(crate) use crate::proxy_core::api::management::custom_endpoint_url_key;

pub(crate) fn provider_custom_endpoint_list(provider: Option<&Provider>) -> Vec<CustomEndpoint> {
    let Some(meta) = provider.and_then(|provider| provider.meta.as_ref()) else {
        return Vec::new();
    };
    let mut endpoints: Vec<_> = meta.custom_endpoints.values().cloned().collect();
    endpoints.sort_by_key(|endpoint| std::cmp::Reverse(endpoint.added_at));
    endpoints
}

pub(crate) fn normalize_custom_endpoint_url(url: &str) -> Result<String, AppError> {
    crate::proxy_core::api::management::normalize_custom_endpoint_url(url).map_err(|issue| {
        let spec = crate::proxy_core::api::management::custom_endpoint_url_issue_spec(issue);
        AppError::localized(spec.key, spec.zh, spec.en)
    })
}

pub(crate) fn mark_custom_endpoint_last_used(
    provider: &mut Provider,
    normalized_url: &str,
    last_used: i64,
) -> bool {
    if let Some(endpoint) = provider
        .meta
        .as_mut()
        .and_then(|meta| meta.custom_endpoints.get_mut(normalized_url))
    {
        endpoint.last_used = Some(last_used);
        true
    } else {
        false
    }
}

pub(crate) const COPILOT_PUBLIC_GITHUB_DOMAIN: &str =
    crate::proxy_core::api::model_catalog::COPILOT_PUBLIC_GITHUB_DOMAIN;

pub(crate) use crate::proxy_core::api::model_catalog::{
    copilot_composite_account_id, default_copilot_github_domain, is_copilot_ghes_domain,
    normalize_github_domain, parse_copilot_models_response_bytes,
    parse_copilot_usage_response_bytes,
};

pub(crate) use crate::proxy_core::api::model_catalog::{
    copilot_api_base, copilot_api_endpoint_from_usage_or_default, copilot_github_client_id,
    copilot_github_device_code_url, copilot_github_oauth_token_url, copilot_github_user_url,
    copilot_token_url, copilot_usage_response_endpoint, copilot_usage_url,
};

use crate::proxy_core::api::ports::{CopilotOptimizerConfig, OptimizerConfig, RectifierConfig};
pub(crate) type ProxyConfig = crate::proxy_core::api::ports::ProxyConfig;
pub(crate) type ProxyRuntimeStatus = crate::proxy_core::api::ports::ProxyRuntimeStatus;

pub(crate) use crate::proxy_core::api::ports::{
    app_proxy_config_with_enabled as proxy_app_config_with_enabled,
    apply_codex_takeover_auth_placeholder_if_present, apply_gemini_takeover_env_fields,
    codex_auth_has_oauth_login_material as core_codex_auth_has_oauth_login_material,
    codex_base_url_from_settings as core_codex_base_url_from_settings,
    codex_config_has_base_url_matching as core_codex_config_has_base_url_matching,
    codex_config_text_from_settings,
    codex_live_settings_parts_from_settings as core_codex_live_settings_parts_from_settings,
    codex_live_snapshot_parts_from_settings as core_codex_live_snapshot_parts_from_settings,
    codex_model_from_config_toml as core_codex_model_from_config_toml,
    codex_provider_backfill_parts_from_settings as core_codex_provider_backfill_parts_from_settings,
    codex_provider_live_write_parts_from_settings as core_codex_provider_live_write_parts_from_settings,
    codex_restored_live_settings_parts,
    codex_wire_api_from_config_toml as core_codex_wire_api_from_config_toml,
    detect_gemini_auth_type as core_detect_gemini_auth_type,
    ensure_codex_takeover_auth_placeholder, gemini_env_json_from_map,
    gemini_env_parse_issue_spec as core_gemini_env_parse_issue_spec,
    gemini_env_string_map_from_settings,
    gemini_live_config_object_from_settings as core_gemini_live_config_object_from_settings,
    gemini_settings_validation_issue_spec as core_gemini_settings_validation_issue_spec,
    is_local_proxy_url,
    launch_env_vars_from_provider_settings as core_launch_env_vars_from_provider_settings,
    live_backup_snapshot_from_live_config as core_live_backup_snapshot_from_live_config,
    live_config_has_proxy_placeholder_for_app as core_live_config_has_proxy_placeholder_for_app,
    live_takeover_app_kinds,
    live_takeover_config_matches_proxy_for_app as core_live_takeover_config_matches_proxy_for_app,
    live_token_sync_app_label as core_live_token_sync_app_label,
    normalize_provider_settings_for_storage as core_normalize_provider_settings_for_storage,
    parse_gemini_env_file,
    provider_additive_live_write_action_for_app as core_provider_additive_live_write_action,
    provider_additive_update_route_for_app as core_provider_additive_update_route,
    provider_app_has_current_provider as core_provider_app_has_current_provider,
    provider_default_live_import_category_from_parts as core_provider_default_live_import_category_from_parts,
    provider_default_live_import_settings as core_provider_default_live_import_settings,
    provider_delete_is_current_provider,
    provider_initial_live_config_managed_marker as core_provider_initial_live_config_managed_marker,
    provider_key_change_policy_issue_for_app as core_provider_key_change_policy_issue,
    provider_key_change_policy_issue_message, provider_live_config_presence_error_policy,
    provider_live_removal_target_for_app as core_provider_live_removal_target,
    provider_live_sync_scope_for_app as core_provider_live_sync_scope,
    provider_non_codex_common_config_snippet_from_settings as core_provider_non_codex_common_config_snippet_from_settings,
    provider_omo_switch_pair_for_app_category as core_provider_omo_switch_pair,
    provider_omo_variant_for_app_category as core_provider_omo_variant_for_category,
    provider_settings_validation_issue_spec,
    provider_settings_validation_parts_from_settings as core_provider_settings_validation_parts_from_settings,
    provider_settings_with_live_token_sync as core_provider_settings_with_live_token_sync,
    provider_should_sync_to_live as core_provider_should_sync_to_live,
    provider_switch_backfill_source_id as core_provider_switch_backfill_source_id,
    provider_switch_dispatch_for_app as core_provider_switch_dispatch,
    provider_switch_requires_takeover_lock as core_provider_switch_requires_takeover_lock,
    provider_switch_should_mark_live_config_managed as core_provider_switch_should_mark_live_config_managed,
    provider_takeover_live_sync_target_for_app as core_provider_takeover_live_sync_target,
    proxy_config_preserving_live_takeover_active, proxy_config_with_ephemeral_listen_port,
    proxy_config_with_live_takeover_active,
    proxy_hot_switch_should_refresh_codex_live_from_backup as core_proxy_hot_switch_should_refresh_codex_live_from_backup,
    proxy_hot_switch_should_sync_claude_live_while_proxy_active as core_proxy_hot_switch_should_sync_claude_live_while_proxy_active,
    proxy_hot_switch_should_sync_codex_live_while_proxy_active as core_proxy_hot_switch_should_sync_codex_live_while_proxy_active,
    proxy_live_config_owned_by_takeover, proxy_runtime_status_stopped,
    proxy_switch_should_hot_switch, proxy_urls_match as core_proxy_urls_match,
    remove_claude_takeover_env_fields_if_present,
    remove_codex_takeover_auth_placeholder_if_present,
    remove_gemini_takeover_env_fields_if_present,
    required_provider_base_url as core_required_provider_base_url,
    sanitize_claude_settings_for_live, serialize_gemini_env_file,
    should_skip_manual_default_live_import as core_should_skip_manual_default_live_import,
    should_skip_provider_legacy_common_config_migration as core_should_skip_provider_legacy_common_config_migration,
    should_skip_startup_default_live_import as core_should_skip_startup_default_live_import,
    validate_gemini_settings_basic as core_validate_gemini_settings_basic,
    validate_gemini_settings_strict as core_validate_gemini_settings_strict,
    CodexLiveSettingsIssue, CodexLiveSettingsParts, CodexLiveSnapshotIssue, CodexLiveSnapshotParts,
    CodexLiveTakeoverMatchFacts, CodexProviderBackfillParts, CodexProviderLiveWriteIssue,
    CodexProviderLiveWriteParts, GeminiAuthType, GeminiAuthTypeInput, GeminiEnvParseIssue,
    GeminiLiveConfigIssue, GeminiSettingsValidationIssue, LiveTokenProviderSettingsIssue,
    ProviderAdditiveLiveWriteAction, ProviderAdditiveUpdateRoute, ProviderKeyChangePolicyIssue,
    ProviderLiveConfigPresenceErrorPolicy, ProviderLiveRemovalTarget, ProviderLiveSyncScope,
    ProviderOmoSwitchPair, ProviderOmoVariant, ProviderSettingsValidationIssue,
    ProviderSettingsValidationParts, ProviderSwitchDispatch, ProviderTakeoverLiveSyncTarget,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::ports::{
    claude_env_credentials_from_settings, gemini_env_map_from_settings,
    openclaw_credential_parts_from_settings, opencode_credential_parts_from_settings,
    CodexProviderValidationIssue, OpenCodeCredentialIssue,
};
#[cfg(test)]
use crate::proxy_core::api::ports::{
    codex_auth_object_value_from_settings,
    provider_codex_credential_values_from_parts as core_provider_codex_credential_values_from_parts,
    provider_non_codex_credential_values_from_settings as core_provider_non_codex_credential_values_from_settings,
    CodexCredentialParts, ProviderCredentialValues as CoreProviderCredentialValues,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::ports::{
    provider_credential_issue_spec, ProviderCredentialIssue,
};

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

pub(crate) fn record_forward_failure_status(status: &mut ProxyRuntimeStatus, error_message: &str) {
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
    provider: &Provider,
    error: &ProxyError,
) {
    let mut status = status.write().await;
    let error_message = error.to_string();
    crate::proxy_core::api::ports::record_forward_provider_failure_status(
        &mut status,
        crate::proxy_core::api::ports::ForwardProviderFailureStatusInput {
            provider_name: provider.name.as_str(),
            error_message: &error_message,
        },
    );
}

pub(crate) async fn record_forward_provider_rectifier_retry_failure_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    provider: &Provider,
    kind: ForwarderRectifierRetryKind,
    error: &ProxyError,
) {
    let mut status = status.write().await;
    let error_message = error.to_string();
    crate::proxy_core::api::ports::record_forward_provider_rectifier_retry_failure_status(
        &mut status,
        crate::proxy_core::api::ports::ForwardProviderRectifierRetryFailureStatusInput {
            provider_name: provider.name.as_str(),
            rectifier_label: forwarder_rectifier_retry_failure_label(kind),
            error_message: &error_message,
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
    apply_proxy_runtime_active_targets, apply_proxy_runtime_uptime,
    record_proxy_server_stopped_status,
};

pub(crate) type ProxyRuntimeConfig = crate::proxy_core::api::config::ProxyRuntimeConfig;
pub(crate) type ProxyGlobalConfig = crate::proxy_core::api::config::ProxyGlobalConfig;
pub(crate) type ProxyAppConfig = crate::proxy_core::api::config::ProxyAppConfig;
pub(crate) type CcSwitchProxyServer = ProxyServer;
pub(crate) type ProxyServerInfo = crate::proxy_core::api::ports::ProxyServerInfo;

pub(crate) use crate::proxy_core::api::ports::proxy_server_info_from_parts;

pub(crate) fn proxy_state_from_runtime_sources(
    config: ProxyConfig,
    db: Arc<Database>,
    app_handle: Option<tauri::AppHandle>,
) -> ProxyState {
    let provider_router = Arc::new(provider_router_from_database(db.clone()));
    let events = Arc::new(ProxyEventBus::default());
    let failover_manager = Arc::new(FailoverSwitchManager::new(db.clone()));
    let config = Arc::new(RwLock::new(config));
    let status = Arc::new(RwLock::new(ProxyRuntimeStatus::default()));
    let start_time = Arc::new(RwLock::new(None));
    let current_providers = Arc::new(RwLock::new(HashMap::new()));
    let gemini_shadow = Arc::new(GeminiShadowStore::default());
    let codex_chat_history = Arc::new(CodexChatHistoryStore::default());
    let attempt_runtime_source =
        forwarder_attempt_runtime_source_from_router(provider_router.clone());
    let protocol_state_source = forwarder_protocol_state_source_from_runtime_parts(
        gemini_shadow.clone(),
        codex_chat_history.clone(),
    );
    let runtime_state_source = forwarder_runtime_state_source_from_runtime_parts(
        status.clone(),
        current_providers.clone(),
        events.clone(),
    );
    let failover_switch_scheduler = failover_switch_scheduler_from_runtime_sources(
        failover_manager.clone(),
        app_handle.clone(),
    );
    let managed_account_runtime_source =
        managed_account_runtime_source_from_app_handle(app_handle.clone());
    let auth_source = forwarder_auth_source_from_managed_account_runtime_source(
        managed_account_runtime_source.clone(),
    );
    let request_source = forwarder_request_source_from_managed_account_runtime_source(
        managed_account_runtime_source.clone(),
    );
    let transport_source = default_forwarder_transport_source();
    let response_source = default_forwarder_response_source();
    let proxy_core_services = Arc::new(CcSwitchProxyServices::with_runtime(CcSwitchProxyRuntime {
        db: db.clone(),
        config: config.clone(),
        provider_router: provider_router.clone(),
        status: status.clone(),
        start_time: start_time.clone(),
        events: events.clone(),
        current_providers: current_providers.clone(),
        attempt_runtime_source,
        protocol_state_source,
        runtime_state_source,
        auth_source,
        request_source,
        transport_source,
        response_source,
        failover_switch_scheduler,
    }));

    ProxyState {
        db,
        config,
        status,
        start_time,
        current_providers,
        provider_router,
        proxy_core_services,
        gemini_shadow,
        codex_chat_history,
        events,
    }
}

pub(crate) fn proxy_server_from_runtime_config(
    config: ProxyConfig,
    db: Arc<Database>,
    app_handle: Option<tauri::AppHandle>,
) -> CcSwitchProxyServer {
    let state = proxy_state_from_runtime_sources(config.clone(), db, app_handle);
    ProxyServer::from_runtime_state(config, state)
}

#[allow(unused_imports)]
pub(crate) use crate::proxy::transport::http::server::{
    await_proxy_http_accept_loop_stop, bind_proxy_http_listener, proxy_http_router_from_state,
    proxy_http_shutdown_channel, spawn_proxy_http_accept_loop, start_proxy_http_server,
    stop_proxy_http_server, ProxyHttpServerHandles,
};

pub(crate) use crate::proxy_core::api::ports::proxy_live_urls_from_listen_parts;

pub(crate) fn record_proxy_server_listen_port_runtime_source(port: u16) {
    crate::proxy::host::cc_switch::global_http_client::set_proxy_port(port);
}

pub(crate) type ProxyTakeoverStatus = crate::proxy_core::api::ports::ProxyTakeoverStatus;

pub(crate) use crate::proxy_core::api::ports::proxy_takeover_status_from_enabled_options;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::ports::proxy_takeover_status_from_parts;

pub(crate) type ClaudeDesktopModelListResponse =
    crate::proxy_core::api::auth::ClaudeDesktopModelListResponse;
pub(crate) type ClaudeDesktopModelRouteInput =
    crate::proxy_core::api::auth::ClaudeDesktopModelRouteInput;
pub(crate) use crate::proxy_core::api::auth::{
    claude_desktop_gateway_token_error, claude_desktop_provider_selection_error,
    claude_desktop_provider_unavailable_error,
};
pub(crate) type ProxyCoreResponse = crate::proxy_core::api::transport::ProxyCoreResponse;
pub(crate) type RequestBodyJsonParseError =
    crate::proxy_core::api::transport::RequestBodyJsonParseError;
pub(crate) type ProxyCoreResult<T> = crate::proxy_core::api::errors::ProxyCoreResult<T>;
pub(crate) type ProxyEngine<S> = crate::proxy_core::api::engine::ProxyEngine<S>;
pub(crate) type ProxyResult = crate::proxy_core::api::transport::ProxyResult;
pub(crate) type ProxyCoreEvent = crate::proxy_core::api::events::ProxyCoreEvent;
pub(crate) type CodexChatHistorySseRecord =
    crate::proxy_core::api::transforms::CodexChatHistorySseRecord;
pub(crate) type CodexChatHistoryState = crate::proxy_core::api::transforms::CodexChatHistoryState;
pub(crate) type CodexChatErrorNormalization =
    crate::proxy_core::api::transforms::CodexChatErrorNormalization;
pub(crate) type CodexChatReasoningOptions =
    crate::proxy_core::api::transforms::CodexChatReasoningOptions;
pub(crate) type CodexChatReasoningProfile =
    crate::proxy_core::api::transforms::CodexChatReasoningProfile;
pub(crate) type CodexToolContext = crate::proxy_core::api::transforms::CodexToolContext;
pub(crate) type ProxyResponseBody = crate::proxy_core::api::transport::ProxyResponseBody;
pub(crate) type CostBreakdown = crate::proxy_core::api::usage::CostBreakdown;
pub(crate) type ModelPricing = crate::proxy_core::api::usage::ModelPricing;
pub(crate) type TokenUsage = crate::proxy_core::api::usage::TokenUsage;
pub(crate) type UsageRecord = crate::proxy_core::api::usage::UsageRecord;
pub(crate) type UsageRouteContext = crate::proxy_core::api::usage::UsageRouteContext;
#[cfg(test)]
pub(crate) type UsageTokens = crate::proxy_core::api::usage::UsageTokens;
pub(crate) type CurrentRouteTarget = crate::proxy_core::api::ports::CurrentRouteTarget;

pub(crate) type AxumResponseBuildErrorContext<'a> =
    crate::proxy_core::api::transport::ProxyResponseBuildErrorContext<'a>;
pub(crate) type CoreResponseBuildFailureContext =
    crate::proxy_core::api::transport::ProxyResponseBuildFailureContext;

pub(crate) fn log_codex_chat_error_normalization(normalized: &CodexChatErrorNormalization) {
    if let Some(message) = normalized.non_json_body_log_message() {
        log::warn!("{message}");
    }
}

pub(crate) fn codex_chat_error_proxy_response(
    status: http::StatusCode,
    headers: HeaderMap,
    body: &[u8],
) -> ProxyCoreResult<ProxyCoreResponse> {
    let normalized = normalize_codex_chat_error_body(body);
    log_codex_chat_error_normalization(&normalized);
    rebuilt_json_proxy_response(status, headers, normalized.response_error)
}

pub(crate) async fn record_codex_chat_response_history(
    history: &CodexChatHistoryStore,
    response: &Value,
) -> usize {
    history.record_response(response).await
}

pub(crate) async fn transform_codex_chat_response_with_history(
    chat_response: &Value,
    tool_context: &CodexToolContext,
    history: &CodexChatHistoryStore,
) -> Result<Value, String> {
    let response = chat_completion_to_response_with_context(chat_response, tool_context)?;
    record_codex_chat_response_history(history, &response).await;
    Ok(response)
}

pub(crate) fn record_codex_chat_response_sse_history(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    history: Arc<CodexChatHistoryStore>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    record_responses_sse_stream(stream, history)
}

pub(crate) fn transform_codex_chat_sse_with_history(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    tool_context: CodexToolContext,
    history: Arc<CodexChatHistoryStore>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    let responses_stream =
        create_codex_chat_to_responses_sse_stream_with_context(stream, tool_context);
    record_codex_chat_response_sse_history(responses_stream, history)
}

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

pub(crate) fn record_proxy_server_bound_runtime_source(
    state: &ProxyState,
    bound_address: &str,
    port: u16,
) {
    emit_proxy_server_started_event_source(state.events.as_ref(), bound_address, port);
    record_proxy_server_listen_port_runtime_source(port);
}

pub(crate) async fn record_proxy_server_started_info_runtime_source(
    state: &ProxyState,
    listen_address: &str,
    port: u16,
) -> ProxyServerInfo {
    record_proxy_server_started_runtime_source(
        state.status.as_ref(),
        state.start_time.as_ref(),
        listen_address,
        port,
    )
    .await;

    proxy_server_info_from_parts(
        listen_address.to_string(),
        port,
        chrono::Utc::now().to_rfc3339(),
    )
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

pub(crate) async fn record_proxy_server_stopped_runtime_event_source(state: &ProxyState) {
    record_proxy_server_stopped_runtime_source(state.status.as_ref(), state.start_time.as_ref())
        .await;
    emit_proxy_server_stopped_event_source(state.events.as_ref());
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
    error: &ProxyError,
) {
    let error_message = error.to_string();
    if let Some(channel) = attempt.channel() {
        let _ = router
            .record_channel_result(
                &channel.channel_id,
                app_type,
                used_half_open_permit,
                false,
                Some(error_message),
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
            Some(error_message),
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
            .release_channel_permit_neutral(&channel.channel_id, app_type, used_half_open_permit)
            .await;
        return;
    }

    router
        .release_permit_neutral(&attempt.provider().id, app_type, used_half_open_permit)
        .await;
}

pub(crate) type GeminiShadowStore = crate::proxy_core::api::transforms::GeminiShadowStore;
#[cfg(test)]
pub(crate) type AuthProfileRef = crate::proxy_core::api::domain::AuthProfileRef;
#[cfg(test)]
pub(crate) type ClaudeAuthHeaderKind = crate::proxy_core::api::transport::ClaudeAuthHeaderKind;
pub(crate) type ClaudeAuthKey = crate::proxy_core::api::auth::ClaudeAuthKey;
pub(crate) type ClaudeAuthKeySource = crate::proxy_core::api::auth::ClaudeAuthKeySource;
pub(crate) type ClaudePromptCacheKeyResolution =
    crate::proxy_core::api::transforms::ClaudePromptCacheKeyResolution;
pub(crate) type ClaudeProviderAuthHeadersInput<'a> =
    crate::proxy_core::api::transport::ClaudeProviderAuthHeadersInput<'a>;
#[cfg(test)]
pub(crate) type CopilotAuthHeadersInput<'a> =
    crate::proxy_core::api::transport::CopilotAuthHeadersInput<'a>;
pub(crate) type CopilotClassification = crate::proxy_core::api::transport::CopilotClassification;
pub(crate) type ForwarderMaybeCopilotAuthOptimizationInput<'a> =
    crate::proxy_core::api::transport::OptionalCopilotAuthOptimizationPreparationInput<'a>;
pub(crate) type ForwarderAuthHeaders = crate::proxy_core::api::transport::ForwarderAuthHeaders;
pub(crate) type ForwarderPreparedCopilotAuthOptimization =
    crate::proxy_core::api::transport::PreparedCopilotAuthOptimization;
pub(crate) type ForwarderProtocolPreparation =
    crate::proxy_core::api::transport::ForwarderProtocolPreparation;
pub(crate) type ForwarderProtocolPreparationInput<'a> =
    crate::proxy_core::api::transport::ForwarderProtocolPreparationInput<'a>;
pub(crate) type ForwarderTransformPlan = crate::proxy_core::api::transport::ForwarderTransformPlan;
pub(crate) type ResponseRuntimePolicy = crate::proxy_core::api::config::ResponseRuntimePolicy;
#[cfg(test)]
pub(crate) type ResponseTimeoutConfig = crate::proxy_core::api::config::ResponseTimeoutConfig;
pub(crate) type SsePassthroughStreamState =
    crate::proxy_core::api::transforms::SsePassthroughStreamState;
pub(crate) type SseUsageAccumulator = crate::proxy_core::api::transforms::SseUsageAccumulator;
pub(crate) type GlobalProxyConfig = crate::proxy_core::api::ports::GlobalProxyConfig;
pub(crate) type AppProxyConfig = crate::proxy_core::api::config::AppProxyConfig;

pub(crate) use crate::proxy_core::api::config::{
    proxy_app_config_from_parts as proxy_app_config_from_config_parts,
    proxy_global_config_from_global_config as proxy_global_config_from_config,
};

pub(crate) fn current_provider_id_from_settings_for_app(app: &AppKind) -> Option<String> {
    app_type_option_from_proxy_core_app(app)
        .as_ref()
        .and_then(crate::settings::get_current_provider)
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

pub(crate) async fn enable_global_proxy_in_db(db: &Database) -> Result<(), String> {
    let mut config = db
        .get_global_proxy_config()
        .await
        .map_err(|e| format!("获取全局代理配置失败: {e}"))?;
    if !config.proxy_enabled {
        config.proxy_enabled = true;
        db.update_global_proxy_config(config)
            .await
            .map_err(|e| format!("更新代理总开关失败: {e}"))?;
    }
    Ok(())
}

pub(crate) async fn disable_global_proxy_best_effort_in_db(db: &Database) -> Result<(), String> {
    let mut config = db
        .get_global_proxy_config()
        .await
        .map_err(|e| format!("获取全局代理配置失败: {e}"))?;
    if config.proxy_enabled {
        config.proxy_enabled = false;
        if let Err(e) = db.update_global_proxy_config(config).await {
            log::warn!("更新代理总开关失败: {e}");
        }
    }
    Ok(())
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

pub(crate) async fn proxy_app_enabled_from_db(
    db: &Database,
    app_type: &str,
) -> Result<bool, String> {
    let config = db
        .get_proxy_config_for_app(app_type)
        .await
        .map_err(|e| format!("获取 {app_type} 配置失败: {e}"))?;
    Ok(config.enabled)
}

pub(crate) async fn set_proxy_app_enabled_in_db(
    db: &Database,
    app_type: &str,
    enabled: bool,
) -> Result<(), String> {
    let config = db
        .get_proxy_config_for_app(app_type)
        .await
        .map_err(|e| format!("获取 {app_type} 配置失败: {e}"))?;
    db.update_proxy_config_for_app(proxy_app_config_with_enabled(config, enabled))
        .await
        .map_err(|e| {
            if enabled {
                format!("设置 {app_type} enabled 状态失败: {e}")
            } else {
                format!("清除 {app_type} enabled 状态失败: {e}")
            }
        })
}

pub(crate) async fn proxy_config_from_db(db: &Database) -> Result<ProxyConfig, String> {
    db.get_proxy_config()
        .await
        .map_err(|e| format!("获取代理配置失败: {e}"))
}

pub(crate) async fn persist_ephemeral_listen_port_if_needed_in_db(
    db: &Database,
    config: &ProxyConfig,
    actual_port: u16,
) -> Result<(), String> {
    let Some(resolved_config) = proxy_config_with_ephemeral_listen_port(config, actual_port) else {
        return Ok(());
    };

    db.update_proxy_config(resolved_config)
        .await
        .map_err(|e| format!("保存动态代理端口失败: {e}"))
}

pub(crate) async fn update_proxy_config_preserving_live_takeover_active_in_db(
    db: &Database,
    config: &ProxyConfig,
) -> Result<(ProxyConfig, ProxyConfig), String> {
    let previous = proxy_config_from_db(db).await?;
    let new_config = proxy_config_preserving_live_takeover_active(&previous, config.clone());

    db.update_proxy_config(new_config.clone())
        .await
        .map_err(|e| format!("保存代理配置失败: {e}"))?;
    Ok((previous, new_config))
}

pub(crate) async fn clear_live_takeover_enabled_flags_in_db(db: &Database) {
    for app_type in live_takeover_app_types() {
        let app_type = app_type.as_str();
        if let Ok(config) = db.get_proxy_config_for_app(app_type).await {
            if config.enabled {
                let config = proxy_app_config_with_enabled(config, false);
                if let Err(e) = db.update_proxy_config_for_app(config).await {
                    log::warn!("清除 {app_type} enabled 状态失败: {e}");
                }
            }
        }
    }
}

pub(crate) async fn clear_legacy_live_takeover_active_flag_in_db(db: &Database) {
    if let Ok(config) = db.get_proxy_config().await {
        let config = proxy_config_with_live_takeover_active(config, false);
        let _ = db.update_proxy_config(config).await;
    }
}

pub(crate) async fn clear_legacy_live_takeover_active_flag_strict_in_db(
    db: &Database,
) -> Result<(), String> {
    db.set_live_takeover_active(false)
        .await
        .map_err(|e| format!("清除接管状态失败: {e}"))
}

pub(crate) async fn live_takeover_backup_exists_from_db(db: &Database, app_type: &str) -> bool {
    match db.get_live_backup(app_type).await {
        Ok(backup) => backup.is_some(),
        Err(e) => {
            log::warn!("读取 {app_type} 备份失败（将继续重建接管）: {e}");
            false
        }
    }
}

pub(crate) async fn delete_live_backup_best_effort_in_db(db: &Database, app_type: &str) {
    let _ = db.delete_live_backup(app_type).await;
}

pub(crate) async fn delete_live_backup_in_db(db: &Database, app_type: &str) -> Result<(), String> {
    db.delete_live_backup(app_type)
        .await
        .map_err(|e| format!("删除 {app_type} Live 备份失败: {e}"))
}

pub(crate) async fn set_legacy_live_takeover_active_best_effort_in_db(db: &Database, active: bool) {
    let _ = db.set_live_takeover_active(active).await;
}

pub(crate) async fn set_legacy_live_takeover_active_in_db(
    db: &Database,
    active: bool,
) -> Result<(), String> {
    db.set_live_takeover_active(active)
        .await
        .map_err(|e| format!("设置接管状态失败: {e}"))
}

pub(crate) async fn live_takeover_any_enabled_from_db(db: &Database) -> Result<bool, String> {
    db.is_live_takeover_active()
        .await
        .map_err(|e| format!("检查接管状态失败: {e}"))
}

pub(crate) async fn clear_provider_health_for_app_in_db(
    db: &Database,
    app_type: &str,
) -> Result<(), String> {
    db.clear_provider_health_for_app(app_type)
        .await
        .map_err(|e| format!("清除 {app_type} 健康状态失败: {e}"))
}

pub(crate) async fn cleanup_all_live_backups_best_effort_in_db(db: &Database) {
    if let Err(clean_err) = db.delete_all_live_backups().await {
        log::warn!("清理 Live 备份失败: {clean_err}");
    }
}

pub(crate) async fn delete_all_live_backups_best_effort_in_db(db: &Database) {
    let _ = db.delete_all_live_backups().await;
}

pub(crate) async fn delete_all_live_backups_in_db(db: &Database) -> Result<(), String> {
    db.delete_all_live_backups()
        .await
        .map_err(|e| format!("删除备份失败: {e}"))
}

pub(crate) async fn save_live_backup_value_in_db(
    db: &Database,
    app_type: &str,
    backup_value: &Value,
    error_label: &str,
) -> Result<(), String> {
    let json_str = serde_json::to_string(backup_value)
        .map_err(|e| format!("序列化 {error_label} 配置失败: {e}"))?;
    db.save_live_backup(app_type, &json_str)
        .await
        .map_err(|e| format!("备份 {error_label} 配置失败: {e}"))
}

pub(crate) async fn clear_all_provider_health_in_db(db: &Database) -> Result<(), String> {
    db.clear_all_provider_health()
        .await
        .map_err(|e| format!("重置健康状态失败: {e}"))
}

pub(crate) async fn live_backup_value_for_restore_from_db(
    db: &Database,
    app_type: &AppType,
) -> Result<Option<Value>, String> {
    let app_type_str = app_type.as_str();
    let backup = db
        .get_live_backup(app_type_str)
        .await
        .map_err(|e| format!("获取 {app_type_str} Live 备份失败: {e}"))?;

    let Some(backup) = backup else {
        return Ok(None);
    };

    serde_json::from_str::<Value>(&backup.original_config)
        .map(Some)
        .map_err(|e| format!("解析 {app_type_str} 备份失败: {e}"))
}

pub(crate) async fn existing_live_backup_value_for_update_from_db(
    db: &Database,
    app_type: &str,
) -> Result<Option<Value>, String> {
    let backup = db
        .get_live_backup(app_type)
        .await
        .map_err(|e| format!("读取 {app_type} 现有备份失败: {e}"))?;

    let Some(backup) = backup else {
        return Ok(None);
    };

    serde_json::from_str::<Value>(&backup.original_config)
        .map(Some)
        .map_err(|e| format!("解析 {app_type} 现有备份失败: {e}"))
}

pub(crate) async fn save_provider_live_backup_from_effective_settings_in_db(
    db: &Database,
    app_type: &AppType,
    effective_settings: &Value,
) -> Result<(), String> {
    let app_type_str = app_type.as_str();
    let backup_json = match app_type {
        AppType::Claude => serde_json::to_string(effective_settings)
            .map_err(|e| format!("序列化 Claude 配置失败: {e}"))?,
        AppType::Codex => serde_json::to_string(effective_settings)
            .map_err(|e| format!("序列化 Codex 配置失败: {e}"))?,
        AppType::Gemini => {
            let env_backup = gemini_live_backup_from_effective_settings(effective_settings);
            serde_json::to_string(&env_backup)
                .map_err(|e| format!("序列化 Gemini 配置失败: {e}"))?
        }
        _ => return Err(format!("未知的应用类型: {app_type_str}")),
    };

    db.save_live_backup(app_type_str, &backup_json)
        .await
        .map_err(|e| format!("更新 {app_type_str} 备份失败: {e}"))
}

pub(crate) async fn live_backup_config_for_simple_restore_from_db(
    db: &Database,
    app_type: &AppType,
) -> Result<Option<Value>, String> {
    let backup = match db.get_live_backup(app_type.as_str()).await {
        Ok(backup) => backup,
        Err(_) => return Ok(None),
    };
    let Some(backup) = backup else {
        return Ok(None);
    };
    let app_label = match app_type {
        AppType::Claude => "Claude",
        AppType::Codex => "Codex",
        AppType::Gemini => "Gemini",
        _ => app_type.as_str(),
    };
    serde_json::from_str(&backup.original_config)
        .map(Some)
        .map_err(|e| format!("解析 {app_label} 备份失败: {e}"))
}

pub(crate) async fn app_summary_config_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<AppSummaryConfig> {
    let config = db
        .get_proxy_config_for_app(app.as_str())
        .await
        .map_err(|error| app_error("load app summary config", error))?;
    Ok(AppSummaryConfig::new(
        config.enabled,
        config.auto_failover_enabled,
    ))
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
    let app = AppKind::from(app_type);
    let settings_current_provider_id = current_provider_id_from_settings_for_app(&app);
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

pub(crate) use crate::proxy_core::api::config::proxy_runtime_config_from_proxy_config as proxy_runtime_config_from_config;

pub(crate) async fn proxy_runtime_config_from_db_source(
    db: &Database,
) -> ProxyCoreResult<ProxyRuntimeConfig> {
    let config = db
        .get_proxy_config()
        .await
        .map_err(|error| app_error("load runtime proxy config", error))?;
    Ok(proxy_runtime_config_from_config(config, false))
}

pub(crate) type ProviderHealth = crate::proxy_core::api::ports::ProviderHealth;
pub(crate) type ProviderAttemptResult = crate::proxy_core::api::ports::ProviderAttemptResult;
pub(crate) type ProviderKind = crate::proxy_core::api::domain::ProviderKind;
pub(crate) type ProviderAuthInfo = crate::proxy_core::api::auth::ProviderAuthInfo;
pub(crate) type ProviderAuthStrategy = crate::proxy_core::api::auth::ProviderAuthStrategy;
#[cfg(test)]
pub(crate) type AuthInfo = crate::proxy_core::api::ports::AuthInfo;

#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::auth_provider::auth_info_from_cc_switch_provider_config;
#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::auth_provider::auth_info_from_cc_switch_route_context;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::auth_provider::{
    provider_with_channel_auth_key, CcSwitchAuthProvider,
};
pub(crate) use crate::proxy_core::api::auth::claude_gemini_cli_auth_info_from_api_key as core_claude_gemini_cli_auth_info_from_api_key;
pub(crate) use crate::proxy_core::api::auth::claude_static_auth_info_from_key as core_claude_static_auth_info_from_key;
pub(crate) use crate::proxy_core::api::auth::codex_auth_info_from_api_key as core_codex_auth_info_from_api_key;
pub(crate) use crate::proxy_core::api::auth::gemini_auth_info_from_api_key as core_gemini_auth_info_from_api_key;
pub(crate) use crate::proxy_core::api::auth::gemini_auth_strategy_for_provider_kind as core_gemini_auth_strategy_for_provider_kind;

pub(crate) const CLAUDE_DESKTOP_GATEWAY_TOKEN_SETTING_KEY: &str = "claude_desktop_gateway_token";

pub(crate) fn claude_desktop_gateway_token_configured_from_db_source(db: &Database) -> bool {
    db.get_setting(CLAUDE_DESKTOP_GATEWAY_TOKEN_SETTING_KEY)
        .ok()
        .flatten()
        .is_some_and(|token| !token.trim().is_empty())
}

pub(crate) fn get_or_create_claude_desktop_gateway_token_from_db_source(
    db: &Database,
) -> Result<String, AppError> {
    if let Some(token) = db.get_setting(CLAUDE_DESKTOP_GATEWAY_TOKEN_SETTING_KEY)? {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let token = format!("ccs-{}", uuid::Uuid::new_v4().simple());
    db.set_setting(CLAUDE_DESKTOP_GATEWAY_TOKEN_SETTING_KEY, &token)?;
    Ok(token)
}

pub(crate) type AttemptEventChannel<'a> = crate::proxy_core::api::events::AttemptEventChannel<'a>;
pub(crate) type AttemptEventPayloadInput<'a> =
    crate::proxy_core::api::events::AttemptEventPayloadInput<'a>;
pub(crate) type AttemptEventPhase = crate::proxy_core::api::events::AttemptEventPhase;

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
        channel_id: attempt.channel().map(|channel| channel.channel_id.clone()),
        payload: attempt_event_payload_from_forward_attempt(request_id, app_type, attempt, None),
    })
}

pub(crate) type ChannelAttemptResult = crate::proxy_core::api::ports::ChannelAttemptResult;
pub(crate) type ChannelQuery<'a> = crate::proxy_core::api::routing::ChannelQuery<'a>;
pub(crate) type ForwardFailureCategory = crate::proxy_core::api::transport::ForwardFailureCategory;

pub(crate) const DEFAULT_PROXY_LISTEN_PORT: u16 =
    crate::proxy_core::api::ports::DEFAULT_PROXY_LISTEN_PORT;
pub(crate) const DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD: u32 =
    crate::proxy_core::api::ports::DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD;
pub(crate) type ForwarderMediaPreventionFacts<'a> =
    crate::proxy_core::api::transport::ForwarderMediaPreventionFacts<'a>;
pub(crate) type AllowResult = crate::proxy_core::api::config::AllowResult;
pub(crate) type CircuitBreakerConfig = crate::proxy_core::api::config::CircuitBreakerConfig;
pub(crate) type CircuitBreakerStats = crate::proxy_core::api::config::CircuitBreakerStats;
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
pub(crate) type ProxyCoreChannelOverrides = crate::proxy_core::api::domain::ChannelOverrides;
pub(crate) type ChannelSpec = crate::proxy_core::api::routing::ChannelSpec;
#[cfg(test)]
pub(crate) type ProxyCoreChannelSpec = crate::proxy_core::api::routing::ChannelSpec;
#[cfg(test)]
pub(crate) type ChannelStatus = crate::proxy_core::api::routing::ChannelStatus;
#[cfg(test)]
pub(crate) type ProxyCoreChannelStatus = crate::proxy_core::api::routing::ChannelStatus;
#[cfg(test)]
pub(crate) type ProxyCoreInterfaceKind = crate::proxy_core::api::routing::InterfaceKind;
#[cfg(test)]
pub(crate) type ProxyCoreModelCapabilities = crate::proxy_core::api::domain::ModelCapabilities;
#[cfg(test)]
pub(crate) type ProxyCoreModelRoute = crate::proxy_core::api::domain::ModelRoute;
pub(crate) type ModelCatalog = crate::proxy_core::api::model_catalog::ModelCatalog;
#[cfg(test)]
pub(crate) type ProxyCoreProviderMetadata = crate::proxy_core::api::domain::ProviderMetadata;
pub(crate) type ProviderSpec = crate::proxy_core::api::domain::ProviderSpec;
#[cfg(test)]
pub(crate) type ProxyCoreProviderSpec = crate::proxy_core::api::domain::ProviderSpec;

pub(crate) use crate::proxy_core::api::domain::{
    provider_account_ref, provider_metadata_from_input, unsupported_app_kind_config_error,
};

use crate::proxy_core::api::domain::{
    additive_provider_stream_check_base_url_from_settings as core_additive_provider_stream_check_base_url_from_settings,
    additive_stream_check_base_url_missing_error_spec as core_additive_stream_check_base_url_missing_error_spec,
};
pub(crate) use crate::proxy_core::api::ports::common_config_settings_mutation_issue_message;
pub(crate) use crate::proxy_core::api::ports::common_config_snippet_issue_message;
#[cfg(test)]
use crate::proxy_core::api::ports::provider_category_is_official as core_provider_category_is_official;
pub(crate) use crate::proxy_core::api::ports::CommonConfigSettingsMutationIssue;
pub(crate) use crate::proxy_core::api::ports::CommonConfigSnippetIssue;
use crate::proxy_core::api::ports::{
    apply_claude_common_config_to_settings as core_apply_claude_common_config_to_settings,
    apply_gemini_common_config_to_settings as core_apply_gemini_common_config_to_settings,
    contains_claude_common_config_snippet as core_contains_claude_common_config_snippet,
    contains_gemini_common_config_snippet as core_contains_gemini_common_config_snippet,
    openclaw_live_write_action_decision as core_openclaw_live_write_action_decision,
    openclaw_live_write_config_decision as core_openclaw_live_write_config_decision,
    opencode_live_provider_fragment_decision as core_opencode_live_provider_fragment_decision,
    opencode_live_write_action_decision as core_opencode_live_write_action_decision,
    opencode_live_write_config_decision as core_opencode_live_write_config_decision,
    provider_common_config_storage_normalization_requires_snippet as core_provider_common_config_storage_normalization_requires_snippet,
    provider_uses_common_config_from_parts as core_provider_uses_common_config_from_parts,
    remove_claude_common_config_from_settings as core_remove_claude_common_config_from_settings,
    remove_gemini_common_config_from_settings as core_remove_gemini_common_config_from_settings,
    should_emit_proxy_official_warning_for_provider_category as core_should_emit_proxy_official_warning_for_provider_category,
    should_reapply_codex_official_live_for_provider_category as core_should_reapply_codex_official_live_for_provider_category,
    OpenClawLiveWriteActionDecision as CoreOpenClawLiveWriteActionDecision,
    OpenClawLiveWriteConfigDecision as CoreOpenClawLiveWriteConfigDecision,
    OpenCodeLiveWriteActionDecision as CoreOpenCodeLiveWriteActionDecision,
    OpenCodeLiveWriteConfigDecision as CoreOpenCodeLiveWriteConfigDecision,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::ports::{
    openclaw_common_config_value_from_settings, opencode_common_config_value_from_settings,
};
pub(crate) use crate::proxy_core::api::ports::{
    proxy_takeover_marked_state_is_reusable,
    proxy_takeover_should_restore_existing_backup_before_retakeover,
};

pub(crate) fn provider_openclaw_has_live_provider_fields(provider: &Provider) -> bool {
    crate::proxy_core::api::domain::openclaw_settings_have_live_provider_fields(
        &provider.settings_config,
    )
}

#[derive(Debug, Clone)]
pub(crate) enum OpenClawLiveWriteConfig {
    Typed(OpenClawProviderConfig),
    Raw { config: Value, parse_error: String },
    Invalid { parse_error: String },
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
    let typed_config = serde_json::from_value::<OpenClawProviderConfig>(config_to_write.clone());
    let decision = core_openclaw_live_write_config_decision(
        config_to_write,
        typed_config.as_ref().err().map(|error| error.to_string()),
        provider_openclaw_has_live_provider_fields(provider),
    );

    let config = match decision {
        CoreOpenClawLiveWriteConfigDecision::Typed => {
            OpenClawLiveWriteConfig::Typed(typed_config.expect("typed OpenClaw config"))
        }
        CoreOpenClawLiveWriteConfigDecision::Raw {
            config,
            parse_error,
        } => OpenClawLiveWriteConfig::Raw {
            config,
            parse_error,
        },
        CoreOpenClawLiveWriteConfigDecision::Invalid { parse_error } => {
            OpenClawLiveWriteConfig::Invalid { parse_error }
        }
    };

    OpenClawLiveWritePlan { config }
}

pub(crate) fn provider_openclaw_live_write_projection(
    provider: &Provider,
) -> OpenClawLiveWriteProjection {
    let plan = provider_openclaw_live_write_plan(provider);
    let action = match plan.config {
        OpenClawLiveWriteConfig::Typed(config) => {
            let decision = core_openclaw_live_write_action_decision(
                &provider.id,
                CoreOpenClawLiveWriteConfigDecision::Typed,
            );
            match decision {
                CoreOpenClawLiveWriteActionDecision::Typed => {
                    OpenClawLiveWriteAction::Typed(config)
                }
                other => unreachable!("typed OpenClaw plan produced non-typed action: {other:?}"),
            }
        }
        OpenClawLiveWriteConfig::Raw {
            config,
            parse_error,
        } => match core_openclaw_live_write_action_decision(
            &provider.id,
            CoreOpenClawLiveWriteConfigDecision::Raw {
                config,
                parse_error,
            },
        ) {
            CoreOpenClawLiveWriteActionDecision::Raw {
                config,
                parse_error,
            } => OpenClawLiveWriteAction::Raw {
                config,
                parse_error,
            },
            other => unreachable!("raw OpenClaw plan produced non-raw action: {other:?}"),
        },
        OpenClawLiveWriteConfig::Invalid { parse_error } => {
            match core_openclaw_live_write_action_decision(
                &provider.id,
                CoreOpenClawLiveWriteConfigDecision::Invalid { parse_error },
            ) {
                CoreOpenClawLiveWriteActionDecision::Reject {
                    parse_error,
                    message,
                } => OpenClawLiveWriteAction::Reject {
                    parse_error,
                    message,
                },
                other => {
                    unreachable!("invalid OpenClaw plan produced non-reject action: {other:?}")
                }
            }
        }
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

pub(crate) fn common_config_snippet_from_settings(
    app_type: &AppType,
    settings: &Value,
) -> Result<String, CommonConfigSnippetIssue> {
    match app_type {
        AppType::Codex => codex_common_config_snippet_from_settings(settings),
        AppType::Claude
        | AppType::ClaudeDesktop
        | AppType::Gemini
        | AppType::OpenCode
        | AppType::OpenClaw
        | AppType::Hermes => core_provider_non_codex_common_config_snippet_from_settings(
            &AppKind::from(app_type),
            settings,
        )
        .map(|snippet| snippet.expect("known non-Codex app should project common config snippet")),
    }
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

pub(crate) fn provider_default_live_import_settings(app_type: &AppType, settings: Value) -> Value {
    core_provider_default_live_import_settings(&AppKind::from(app_type), settings)
}

pub(crate) fn normalize_provider_settings_for_storage(
    app_type: &AppType,
    settings: &mut Value,
) -> bool {
    core_normalize_provider_settings_for_storage(&AppKind::from(app_type), settings)
}

pub(crate) fn should_skip_manual_default_live_import(
    app_type: &AppType,
    has_non_official_seed_provider: bool,
) -> bool {
    core_should_skip_manual_default_live_import(
        &AppKind::from(app_type),
        has_non_official_seed_provider,
    )
}

pub(crate) fn should_skip_startup_default_live_import(
    app_type: &AppType,
    has_any_provider: bool,
) -> bool {
    core_should_skip_startup_default_live_import(&AppKind::from(app_type), has_any_provider)
}

pub(crate) fn provider_live_sync_scope(app_type: &AppType) -> ProviderLiveSyncScope {
    core_provider_live_sync_scope(&AppKind::from(app_type))
}

pub(crate) fn provider_app_has_current_provider(app_type: &AppType) -> bool {
    core_provider_app_has_current_provider(&AppKind::from(app_type))
}

pub(crate) fn provider_should_sync_to_live(provider: &Provider) -> bool {
    let live_config_managed = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.live_config_managed);
    core_provider_should_sync_to_live(live_config_managed)
}

pub(crate) fn provider_initial_live_config_managed_marker(
    app_type: &AppType,
    add_to_live: bool,
) -> Option<bool> {
    core_provider_initial_live_config_managed_marker(&AppKind::from(app_type), add_to_live)
}

pub(crate) fn should_skip_provider_legacy_common_config_migration(
    app_type: &AppType,
    legacy_snippet: &str,
) -> bool {
    core_should_skip_provider_legacy_common_config_migration(
        &AppKind::from(app_type),
        legacy_snippet,
    )
}

pub(crate) fn provider_key_change_policy_issue(
    app_type: &AppType,
    existing_provider: Option<&Provider>,
) -> Option<ProviderKeyChangePolicyIssue> {
    core_provider_key_change_policy_issue(
        &AppKind::from(app_type),
        existing_provider.and_then(|provider| provider.category.as_deref()),
    )
}

pub(crate) fn provider_additive_live_write_action(
    app_type: &AppType,
    provider: &Provider,
    add_to_live: bool,
) -> ProviderAdditiveLiveWriteAction {
    core_provider_additive_live_write_action(
        &AppKind::from(app_type),
        provider.category.as_deref(),
        add_to_live,
    )
}

pub(crate) fn provider_omo_variant_for_category(
    app_type: &AppType,
    category: Option<&str>,
) -> Option<ProviderOmoVariant> {
    core_provider_omo_variant_for_category(&AppKind::from(app_type), category)
}

pub(crate) fn provider_omo_switch_pair(
    app_type: &AppType,
    provider: &Provider,
) -> Option<ProviderOmoSwitchPair> {
    core_provider_omo_switch_pair(&AppKind::from(app_type), provider.category.as_deref())
}

pub(crate) fn provider_additive_update_route(
    app_type: &AppType,
    category: Option<&str>,
) -> Option<ProviderAdditiveUpdateRoute> {
    core_provider_additive_update_route(&AppKind::from(app_type), category)
}

pub(crate) fn provider_switch_dispatch(
    app_type: &AppType,
    provider: &Provider,
) -> ProviderSwitchDispatch {
    core_provider_switch_dispatch(&AppKind::from(app_type), provider.category.as_deref())
}

pub(crate) fn provider_switch_requires_takeover_lock(app_type: &AppType) -> bool {
    core_provider_switch_requires_takeover_lock(&AppKind::from(app_type))
}

pub(crate) fn live_takeover_app_types() -> [AppType; 3] {
    live_takeover_app_kinds().map(|app| {
        app.as_str()
            .parse::<AppType>()
            .expect("proxy-core live takeover app kind must be supported by cc-switch")
    })
}

pub(crate) fn provider_takeover_live_sync_target(
    app_type: &AppType,
) -> ProviderTakeoverLiveSyncTarget {
    core_provider_takeover_live_sync_target(&AppKind::from(app_type))
}

pub(crate) fn provider_live_removal_target(
    app_type: &AppType,
) -> Option<ProviderLiveRemovalTarget> {
    core_provider_live_removal_target(&AppKind::from(app_type))
}

pub(crate) fn provider_switch_backfill_source_id<'a>(
    app_type: &AppType,
    current_id: Option<&'a str>,
    target_id: &str,
) -> Option<&'a str> {
    core_provider_switch_backfill_source_id(&AppKind::from(app_type), current_id, target_id)
}

pub(crate) fn provider_switch_should_mark_live_config_managed(
    app_type: &AppType,
    live_config_managed: Option<bool>,
) -> bool {
    core_provider_switch_should_mark_live_config_managed(
        &AppKind::from(app_type),
        live_config_managed,
    )
}

#[cfg(test)]
pub(crate) type ProviderCredentialValues = CoreProviderCredentialValues;

#[cfg(test)]
pub(crate) fn provider_credential_values(
    provider: &Provider,
    app_type: &AppType,
) -> Result<ProviderCredentialValues, ProviderCredentialIssue> {
    match app_type {
        AppType::Codex => {
            let auth = codex_auth_object_value_from_settings(&provider.settings_config)
                .ok_or(ProviderCredentialIssue::CodexAuthMissing)?;
            let config_toml =
                codex_config_text_from_settings(&provider.settings_config).unwrap_or("");
            core_provider_codex_credential_values_from_parts(CodexCredentialParts {
                api_key: codex_api_key_from_auth_and_config(Some(auth), Some(config_toml)),
                config_toml: Some(config_toml.to_string()),
            })
        }
        AppType::Claude
        | AppType::ClaudeDesktop
        | AppType::Gemini
        | AppType::OpenCode
        | AppType::OpenClaw
        | AppType::Hermes => core_provider_non_codex_credential_values_from_settings(
            &AppKind::from(app_type),
            &provider.settings_config,
        )
        .map(|values| values.expect("known non-Codex app should project credential values")),
    }
}

pub(crate) struct OpenCodeLiveProviderFragment {
    pub(crate) config: Value,
    pub(crate) from_full_config: bool,
}

pub(crate) fn provider_opencode_live_provider_fragment(
    provider: &Provider,
) -> OpenCodeLiveProviderFragment {
    let fragment =
        core_opencode_live_provider_fragment_decision(&provider.id, &provider.settings_config);
    OpenCodeLiveProviderFragment {
        config: fragment.config,
        from_full_config: fragment.from_full_config,
    }
}

pub(crate) use crate::proxy_core::api::domain::opencode_settings_have_live_provider_fields as opencode_live_provider_fragment_has_provider_fields;

#[derive(Debug, Clone)]
pub(crate) enum OpenCodeLiveWriteConfig {
    Typed(OpenCodeProviderConfig),
    Raw { config: Value, parse_error: String },
    Invalid { parse_error: String },
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

pub(crate) fn provider_opencode_live_write_plan(provider: &Provider) -> OpenCodeLiveWritePlan {
    let fragment = provider_opencode_live_provider_fragment(provider);
    let config_to_write = fragment.config;
    let has_live_provider_fields =
        opencode_live_provider_fragment_has_provider_fields(&config_to_write);
    let typed_config = serde_json::from_value::<OpenCodeProviderConfig>(config_to_write.clone());
    let decision = core_opencode_live_write_config_decision(
        config_to_write,
        typed_config.as_ref().err().map(|error| error.to_string()),
        has_live_provider_fields,
    );

    let config = match decision {
        CoreOpenCodeLiveWriteConfigDecision::Typed => {
            OpenCodeLiveWriteConfig::Typed(typed_config.expect("typed OpenCode config"))
        }
        CoreOpenCodeLiveWriteConfigDecision::Raw {
            config,
            parse_error,
        } => OpenCodeLiveWriteConfig::Raw {
            config,
            parse_error,
        },
        CoreOpenCodeLiveWriteConfigDecision::Invalid { parse_error } => {
            OpenCodeLiveWriteConfig::Invalid { parse_error }
        }
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
        OpenCodeLiveWriteConfig::Typed(config) => {
            match core_opencode_live_write_action_decision(
                &provider.id,
                CoreOpenCodeLiveWriteConfigDecision::Typed,
            ) {
                CoreOpenCodeLiveWriteActionDecision::Typed => {
                    OpenCodeLiveWriteAction::Typed(config)
                }
                other => unreachable!("typed OpenCode plan produced non-typed action: {other:?}"),
            }
        }
        OpenCodeLiveWriteConfig::Raw {
            config,
            parse_error,
        } => match core_opencode_live_write_action_decision(
            &provider.id,
            CoreOpenCodeLiveWriteConfigDecision::Raw {
                config,
                parse_error,
            },
        ) {
            CoreOpenCodeLiveWriteActionDecision::Raw {
                config,
                parse_error,
            } => OpenCodeLiveWriteAction::Raw {
                config,
                parse_error,
            },
            other => unreachable!("raw OpenCode plan produced non-raw action: {other:?}"),
        },
        OpenCodeLiveWriteConfig::Invalid { parse_error } => {
            match core_opencode_live_write_action_decision(
                &provider.id,
                CoreOpenCodeLiveWriteConfigDecision::Invalid { parse_error },
            ) {
                CoreOpenCodeLiveWriteActionDecision::Reject {
                    parse_error,
                    message,
                } => OpenCodeLiveWriteAction::Reject {
                    parse_error,
                    message,
                },
                other => {
                    unreachable!("invalid OpenCode plan produced non-reject action: {other:?}")
                }
            }
        }
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

pub(crate) use crate::proxy_core::api::domain::{
    channel_spec_from_input, ChannelSpecInput, ModelRouteInput,
};

#[cfg(test)]
pub(crate) type ProxyCoreUpstreamEndpoint = crate::proxy_core::api::domain::UpstreamEndpoint;
pub(crate) type ChannelRequestValidationError =
    crate::proxy_core::api::routing::ChannelRequestValidationError;
pub(crate) type ChannelRouteSource = crate::proxy_core::api::management::ChannelRouteSource;
pub(crate) type ChannelRecord = crate::proxy_core::api::management::ChannelRecord;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::management::ChannelReachabilityStatus;
#[cfg(test)]
use crate::proxy_core::api::management::StreamCheckResult;
pub(crate) type ChannelKeyRecord = crate::proxy_core::api::management::ChannelKeyRecord;
pub(crate) type ChannelKeyRecordInput = crate::proxy_core::api::management::ChannelKeyRecordInput;
#[cfg(test)]
pub(crate) type ChannelKeyRuntimeCandidate =
    crate::proxy_core::api::management::ChannelKeyRuntimeCandidate;
pub(crate) type ChannelModelRecord = crate::proxy_core::api::management::ChannelModelRecord;
pub(crate) type ChannelModelRecordInput =
    crate::proxy_core::api::management::ChannelModelRecordInput;
pub(crate) type ChannelRecordInput = crate::proxy_core::api::management::ChannelRecordInput;

pub(crate) use crate::proxy_core::api::management::{
    channel_key_record_from_input, channel_model_record_from_input, channel_record_from_input,
};

pub(crate) type InterfaceKind = crate::proxy_core::api::routing::InterfaceKind;
pub(crate) type LegacyChannelModelProjection =
    crate::proxy_core::api::routing::LegacyChannelModelProjection;
pub(crate) type LegacyChannelProjection = crate::proxy_core::api::routing::LegacyChannelProjection;
#[cfg(test)]
pub(crate) type LegacyChannelProjectionInput =
    crate::proxy_core::api::routing::LegacyChannelProjectionInput;
pub(crate) type LegacyChannelMigrationPlanInput =
    crate::proxy_core::api::routing::LegacyChannelMigrationPlanInput;
pub(crate) type LegacyEndpointInput = crate::proxy_core::api::routing::LegacyEndpointInput;
pub(crate) type LegacyModelRouteInput = crate::proxy_core::api::routing::LegacyModelRouteInput;
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
pub(crate) type ProviderSelectionInput = crate::proxy_core::api::routing::ProviderSelectionInput;
pub(crate) type AutoFailoverToggleInput = crate::proxy_core::api::routing::AutoFailoverToggleInput;
pub(crate) type AutoFailoverTogglePlan = crate::proxy_core::api::routing::AutoFailoverTogglePlan;
pub(crate) type FailoverQueuePosition = crate::proxy_core::api::routing::FailoverQueuePosition;
pub(crate) type ProxyCoreError = crate::proxy_core::api::errors::ProxyCoreError;
#[cfg(test)]
pub(crate) type ProxyCoreEventType = crate::proxy_core::api::events::ProxyCoreEventType;
pub(crate) type AppKind = crate::proxy_core::api::domain::AppKind;
pub(crate) type AppProviderAdapterKind = crate::proxy_core::api::domain::AppProviderAdapterKind;
#[cfg(test)]
pub(crate) type RetryPolicy = crate::proxy_core::api::domain::RetryPolicy;
pub(crate) type RouteResolveRequest = crate::proxy_core::api::management::RouteResolveRequest;
pub(crate) type RouteResolveResponse = crate::proxy_core::api::management::RouteResolveResponse;
pub(crate) type RouteResolveChannelInput =
    crate::proxy_core::api::routing::RouteResolveChannelInput;
pub(crate) type RouteCandidateCircuitKey =
    crate::proxy_core::api::routing::RouteCandidateCircuitKey;
pub(crate) type ChannelRouteCandidate = crate::proxy_core::api::routing::ChannelRouteCandidate;
pub(crate) type ResolvedChannelAttempt = crate::proxy_core::api::routing::ResolvedChannelAttempt;
pub(crate) type RoutePlan = crate::proxy_core::api::routing::RoutePlan;
pub(crate) type RouteSelection = crate::proxy_core::api::routing::RouteSelection;
#[cfg(test)]
pub(crate) type CodexProxyErrorContext<'a> =
    crate::proxy_core::api::transforms::CodexProxyErrorContext<'a>;
#[cfg(test)]
pub(crate) type CodexProxyErrorKind = crate::proxy_core::api::transforms::CodexProxyErrorKind;
pub(crate) type ForwardFailureKind = crate::proxy_core::api::transport::ForwardFailureKind;
pub(crate) enum ForwarderFailureDecision {
    Retryable,
    NonRetryable,
}
pub(crate) enum ForwarderRectifierRetryFailureDecision {
    ProviderFailure,
    ClientFailure,
}
pub(crate) type ForwarderRectifierRetryKind =
    crate::proxy_core::api::transport::ForwarderRectifierRetryKind;

pub(crate) fn terminal_forward_failure_log_line_for_error(
    app_type: &str,
    attempted_providers: usize,
    total_providers: usize,
    last_error: Option<&ProxyError>,
) -> Option<String> {
    let last_failure = last_error.map(forward_failure_kind_from_proxy_error);
    build_terminal_forward_failure_log(attempted_providers, total_providers, last_failure.as_ref())
        .map(|log| forwarder_failure_log_line(app_type, &log))
}

pub(crate) fn retryable_forward_failure_log_line(
    app_type: &str,
    error: &ProxyError,
    provider: &Provider,
    attempted_providers: usize,
    total_providers: usize,
) -> String {
    let failure = forward_failure_kind_from_proxy_error(error);
    let log = build_retryable_forward_failure_log(
        provider.name.as_str(),
        attempted_providers,
        total_providers,
        &failure,
    );
    forwarder_failure_log_line(app_type, &log)
}

pub(crate) fn forwarder_rectifier_retry_success_log_line(
    app_type: &str,
    kind: ForwarderRectifierRetryKind,
) -> String {
    format!(
        "[{app_type}] {}",
        core_forwarder_rectifier_retry_success_message(kind)
    )
}

pub(crate) fn forwarder_rectifier_retry_failure_log_line(
    app_type: &str,
    kind: ForwarderRectifierRetryKind,
    error: &ProxyError,
) -> String {
    format!(
        "[{app_type}] {}",
        core_forwarder_rectifier_retry_failure_message(kind, &error.to_string())
    )
}

#[cfg(test)]
pub(crate) type ManagementAuthError = crate::proxy_core::api::auth::ManagementAuthError;
pub(crate) type CircuitBreakerFailureDecision =
    crate::proxy_core::api::config::CircuitBreakerFailureDecision;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::auth::claude_desktop_model_id_is_profile_safe;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::auth::validate_claude_desktop_gateway_bearer_header;
pub(crate) use crate::proxy_core::api::auth::validate_managed_account_upstream_auth;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::auth::ManagedAccountAuthError;
pub(crate) use crate::proxy_core::api::auth::{
    classify_provider_managed_auth as core_classify_provider_managed_auth,
    codex_oauth_access_token_expires_at_ms, codex_oauth_authorization_code_form,
    codex_oauth_device_auth_token_request_body, codex_oauth_device_auth_token_url,
    codex_oauth_device_auth_usercode_url, codex_oauth_device_code_expires_at_ms,
    codex_oauth_device_code_expires_in_secs, codex_oauth_device_code_request_failure,
    codex_oauth_device_poll_failure, codex_oauth_device_poll_status_kind,
    codex_oauth_device_usercode_request_body, codex_oauth_device_verification_url,
    codex_oauth_identity_from_token_claims, codex_oauth_missing_account_id_message,
    codex_oauth_missing_pending_user_code_message, codex_oauth_missing_refresh_token_message,
    codex_oauth_pending_device_code_is_expired, codex_oauth_poll_interval_secs,
    codex_oauth_refresh_failure, codex_oauth_refresh_token_form, codex_oauth_status_from_parts,
    codex_oauth_token_exchange_failure, codex_oauth_token_is_expiring_soon, codex_oauth_token_url,
    compare_managed_auth_account_order, copilot_auth_status_from_parts,
    copilot_oauth_poll_error_kind, copilot_token_is_expiring_soon, ensure_managed_auth_provider,
    managed_account_id_for_auth_provider as core_managed_account_id_for_auth_provider,
    managed_auth_account_from_parts, managed_auth_device_code_response_from_parts,
    managed_auth_fallback_default_account_id, managed_auth_status_from_parts,
    managed_provider_auth_info_for_provider_kind as core_managed_provider_auth_info_for_provider_kind,
    CodexOAuthDevicePollStatusKind, CodexOAuthTokenClaims, CopilotOAuthPollErrorKind,
    ManagedAccountBindingInput, ManagedAccountBindingSource, ManagedAuthAccount,
    ManagedAuthAccountSortKey, ManagedAuthDefaultAccountCandidate, ManagedAuthDeviceCodeResponse,
    ManagedAuthStatus, ProviderManagedAuthClassification, ProviderManagedAuthFacts,
    CODEX_OAUTH_AUTH_PROVIDER, GITHUB_COPILOT_AUTH_PROVIDER,
};
pub(crate) use crate::proxy_core::api::auth::{
    claude_desktop_config_with_deployment_mode,
    claude_desktop_config_without_gateway_enterprise_config, claude_desktop_default_proxy_routes,
    claude_desktop_direct_gateway_credentials, claude_desktop_direct_inference_model_specs,
    claude_desktop_gateway_profile, claude_desktop_meta_applied_id,
    claude_desktop_meta_has_profile_entry, claude_desktop_meta_with_profile_entry,
    claude_desktop_profile_gateway_base_url, claude_desktop_profile_has_unsafe_model_ids,
    claude_desktop_proxy_gateway_base_url, claude_desktop_proxy_model_routes,
    claude_desktop_proxy_request_body_with_upstream_model,
    ClaudeDesktopDirectGatewayCredentialIssue, ClaudeDesktopDirectModelRouteIssue,
    ClaudeDesktopGatewayProfileModelSpec, ClaudeDesktopProxyRequestBodyIssue,
    ClaudeDesktopProxyRouteInput, ClaudeDesktopResolvedProxyRoute,
};
pub(crate) use crate::proxy_core::api::auth::{
    extract_claude_auth_key_from_settings, is_gemini_oauth_key_shape,
    parse_gemini_oauth_credentials,
};
pub(crate) use crate::proxy_core::api::config::{
    app_proxy_config_defaults_for_app, app_type_from_circuit_key, cache_injection_log_message,
    channel_circuit_key, channel_circuit_key_prefix, circuit_breaker_config_from_app_config,
    circuit_failure_threshold_from_app_config, normalize_thinking_type, provider_circuit_key,
    provider_circuit_key_prefix, rectify_anthropic_request, rectify_thinking_budget,
    should_rectify_thinking_budget, should_rectify_thinking_signature,
    thinking_optimization_log_message,
};
pub(crate) use crate::proxy_core::api::domain::channel_matches_query;
use crate::proxy_core::api::events::{
    attempt_event_name, build_attempt_event_payload, build_provider_switched_event_payload,
    build_proxy_official_warning_event_payload, build_request_started_event_payload,
    build_server_started_event_payload, build_server_stopped_event_payload,
};
pub(crate) use crate::proxy_core::api::management::channel_not_found_error;
pub(crate) use crate::proxy_core::api::management::{
    channel_health_update_from_input, channel_reachability_probe_error,
    channel_reachability_result_from_stream_check_result as stream_check_result_to_channel_reachability,
    channel_test_app_type_error, channel_test_provider_not_found_error, merge_stream_check_config,
    provider_health_update_from_input, should_retry_channel_reachability_failure,
    stream_check_failed_result, stream_check_failed_result_with_retry_count,
    stream_check_result_from_probe_result, AppChannelListQuery, AppChannelManagementRequest,
    AppChannelResponse, AppListRequest, AppListResponse, AppModelCatalogRequest, AppModelListQuery,
    ChannelBreakerStatsResponse, ChannelCreateRequest, ChannelDeleteResponse,
    ChannelHealthResetResponse, ChannelHealthUpdateInput, ChannelKeyDeleteResponse,
    ChannelKeyPathRequest, ChannelKeyRecordResponse, ChannelKeysResponse, ChannelListQuery,
    ChannelListRequest, ChannelListResponse, ChannelMigrationMaterializeInput,
    ChannelMigrationMaterializeResponse, ChannelMigrationPreviewInput,
    ChannelMigrationPreviewResponse, ChannelModelsResponse, ChannelPathRequest,
    ChannelReachabilityResult, ChannelRecordResponse, ChannelRouteRejected,
    ChannelTestProbeRequest, ChannelTestResponse, CurrentRouteResponse, GroupListQuery,
    GroupListRequest, HealthCheckRequest, HealthCheckResponse, ManagementAppPathRequest,
    ProviderHealthUpdateInput, ProviderListResponse, ProxyChannelModelsReplaceRequest,
    ProxyChannelTestRequest, ProxyStatusRequest, ProxyStatusResponse, RouteGroupListResponse,
    RouteResolveManagementRequest, StreamCheckConfigOverride, CHANNEL_HEALTH_UNKNOWN_STATUS,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::model_catalog::client_model_catalog_from_optional_raw;
pub(crate) use crate::proxy_core::api::model_catalog::{
    apply_copilot_model_normalization, strip_one_m_suffix_for_upstream,
    strip_one_m_suffix_for_upstream_from_body,
};
pub(crate) use crate::proxy_core::api::model_catalog::{
    client_model_catalog_source_for_app, ClientModelCatalogResponse, ClientModelCatalogSource,
    RoutableModelList,
};
pub(crate) use crate::proxy_core::api::ports::ChannelReachabilityProbe;
pub(crate) use crate::proxy_core::api::ports::{
    channel_breaker_stats_from_parts, channel_health_reset_from_parts, AppSummaryConfig,
    AuthProvider, ChannelBreakerStats, ChannelHealthReset, ChannelHealthStore,
    ChannelKeyRuntimeSource, ChannelSource, ClaudeDesktopGatewayAuthSource, ForwardPipeline,
    ManagementAuthRuntimeConfig, ManagementAuthSource, ModelCatalogProvider, ProviderHealthStore,
    ProviderSource, ProxyConfigSource, ProxyEventSink, ProxyServices, RoutePolicySource,
    RouteResolver, RuntimeStatusSource, UsageSink,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::routing::DEFAULT_ROUTE_GROUP;
pub(crate) use crate::proxy_core::api::routing::{
    failover_config_read_error_log_line, provider_router_auto_failover_enabled_decision,
    route_policy_failover_provider_ids, RoutePolicy, RouteRequest,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::claude_api_format_from_metadata;
pub(crate) use crate::proxy_core::api::transforms::resolve_claude_forward_api_format;
pub(crate) use crate::proxy_core::api::transforms::CLAUDE_API_FORMAT_METADATA_KEY;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::{
    anthropic_request_to_gemini_request_with_shadow, anthropic_to_openai_chat_request,
    anthropic_to_openai_responses_request, canonical_json_string,
    gemini_response_to_anthropic_message, openai_chat_to_anthropic_message,
    openai_responses_to_anthropic_message, short_value_hash,
};
pub(crate) use crate::proxy_core::api::transforms::{
    append_utf8_safe, build_gemini_upstream_url, chat_completion_to_response_with_context,
    claude_provider_transform_required as core_claude_provider_transform_required,
    claude_request_transform_for_api_format, claude_response_to_anthropic_message_for_api_format,
    claude_stream_usage_event_filter,
    claude_transform_streaming_decision as core_claude_transform_streaming_decision,
    codex_chat_transform_streaming_decision as core_codex_chat_transform_streaming_decision,
    codex_stream_usage_event_filter, create_claude_to_anthropic_sse_stream_for_api_format,
    create_codex_chat_to_responses_sse_stream_with_context, extract_anthropic_tool_schema_hints,
    inspect_codex_chat_history_sse_block, should_preserve_reasoning_content_for_openai_chat,
    take_sse_block, AnthropicToolSchemaHints, ClaudeApiFormatRequestTransformContext,
    ClaudeApiFormatSseTransformContext, ClaudeTransformStreamingDecision,
    CodexChatTransformStreamingDecision,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::build_claude_auth_headers;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::build_codex_bearer_auth_headers;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::build_codex_oauth_session_headers;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::build_copilot_auth_headers;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::build_gemini_auth_headers;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::extract_gemini_model_from_path;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::interface_kind_for_forward;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::prepare_upstream_request_body_with_report;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::request_model_for_forward;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::resolve_upstream_request_transport_policy;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::should_preserve_exact_request_header_case;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::{
    append_query_to_full_url, claude_transform_endpoint_rewrite_input_from_body,
    rewrite_claude_transform_endpoint,
};
pub(crate) use crate::proxy_core::api::transport::{
    apply_bedrock_pre_send_optimizers, apply_forwarder_media_prevention_from_facts,
    bedrock_env_flag_from_provider_settings, build_claude_provider_auth_headers,
    build_claude_upstream_url, build_codex_provider_auth_headers, build_codex_upstream_url,
    build_gemini_provider_auth_headers, build_retryable_forward_failure_log,
    build_terminal_forward_failure_log, categorize_forward_failure,
    forward_failure_message_from_proxy_status as core_forward_failure_message_from_proxy_status,
    forwarder_all_providers_circuit_open_log_line, forwarder_failure_log_line,
    forwarder_no_available_provider_status_message, forwarder_no_providers_configured_log_line,
    forwarder_rectifier_retry_failure_label,
    forwarder_rectifier_retry_failure_message as core_forwarder_rectifier_retry_failure_message,
    forwarder_rectifier_retry_success_message as core_forwarder_rectifier_retry_success_message,
    forwarder_terminal_failure_status_message, parse_json_request_body,
    parse_json_request_body_or_null, should_apply_bedrock_pre_send_optimizer,
    should_apply_forwarder_media_prevention_for_app, should_failover_after_rectifier_retry_failure,
    CodexProviderChatCompletionsFacts, CodexResponsesToChatConversionFacts, ForwardUpstreamUrlPlan,
    ForwarderProviderUrlFacts,
};
pub(crate) use crate::proxy_core::api::transport::{
    codex_provider_uses_chat_completions as core_codex_provider_uses_chat_completions,
    codex_responses_to_chat_conversion_required as core_codex_responses_to_chat_conversion_required,
};
pub(crate) use crate::proxy_core::api::transport::{
    parse_custom_user_agent,
    provider_custom_user_agent_header as core_provider_custom_user_agent_header,
};
pub(crate) use crate::proxy_core::api::transport::{
    rebuilt_json_proxy_response, ProxyBody, ProxyRequest,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::{
    resolve_codex_provider_uses_chat_completions, should_convert_codex_responses_endpoint_to_chat,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::usage::success_usage_record_with_request_id_fallback;
pub(crate) use crate::proxy_core::api::usage::usage_logging_enabled_from_config_flag;
pub(crate) use crate::proxy_core::api::usage::{
    normalize_pricing_source, validate_cost_multiplier_value, CostMultiplierValidationError,
    PricingSourceValidationError, PRICING_SOURCE_REQUEST, PRICING_SOURCE_RESPONSE,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::usage::{
    usage_record_debug_log_message, usage_record_failure_warning_message,
};

#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::managed_account_runtime_source::default_managed_account_runtime_source;
#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::managed_account_runtime_source::resolve_managed_account_auth_from_runtime_source;
#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::managed_account_runtime_source::ManagedAccountRuntimeSource;
#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::managed_account_runtime_source::{
    copilot_api_endpoint_from_app_handle, copilot_live_models_from_app_handle,
    copilot_model_vendor_from_app_handle,
};
pub(crate) use crate::proxy::host::cc_switch::managed_account_runtime_source::{
    managed_account_runtime_source_from_app_handle, ManagedAccountRuntimeSourceRef,
};

const PROXY_OFFICIAL_WARNING_EVENT: &str =
    crate::proxy_core::api::events::PROXY_OFFICIAL_WARNING_EVENT;
const PROVIDER_SWITCHED_EVENT: &str = crate::proxy_core::api::events::PROVIDER_SWITCHED_EVENT;
const PROVIDER_SWITCHED_SOURCE_FAILOVER: &str =
    crate::proxy_core::api::events::PROVIDER_SWITCHED_SOURCE_FAILOVER;
const PROVIDER_SWITCHED_SOURCE_FAILOVER_ENABLED: &str =
    crate::proxy_core::api::events::PROVIDER_SWITCHED_SOURCE_FAILOVER_ENABLED;
const REQUEST_STARTED_EVENT: &str = crate::proxy_core::api::events::REQUEST_STARTED_EVENT;
const SERVER_STARTED_EVENT: &str = crate::proxy_core::api::events::SERVER_STARTED_EVENT;
const SERVER_STOPPED_EVENT: &str = crate::proxy_core::api::events::SERVER_STOPPED_EVENT;
#[cfg(test)]
pub(crate) const AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE: &str =
    crate::proxy_core::api::routing::AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE;

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

pub(crate) fn proxy_official_warning_event_from_provider(
    app_type: &str,
    provider: Option<&Provider>,
) -> Option<ProxyEventBusMessage> {
    let provider = provider?;
    if !should_emit_proxy_official_warning_for_provider(provider) {
        return None;
    }

    Some(proxy_official_warning_event_message(
        app_type,
        &provider.name,
    ))
}

pub(crate) fn proxy_official_warning_event_from_current_provider_db(
    db: &Database,
    app_type: &AppType,
) -> Option<ProxyEventBusMessage> {
    let current_id = crate::settings::get_effective_current_provider(db, app_type)
        .ok()
        .flatten()?;
    let provider = db
        .get_provider_by_id(&current_id, app_type.as_str())
        .ok()
        .flatten()?;

    proxy_official_warning_event_from_provider(app_type.as_str(), Some(&provider))
}

pub(crate) fn current_provider_for_app_from_db(
    db: &Database,
    app_type: &AppType,
) -> Result<Option<Provider>, String> {
    let Some(current_id) = crate::settings::get_effective_current_provider(db, app_type)
        .map_err(|error| format!("获取 {app_type:?} 当前供应商失败: {error}"))?
    else {
        return Ok(None);
    };

    db.get_provider_by_id(&current_id, app_type.as_str())
        .map_err(|error| format!("读取 {app_type:?} 当前供应商失败: {error}"))
}

pub(crate) fn require_current_provider_for_app_from_db(
    db: &Database,
    app_type: &AppType,
) -> Result<Provider, String> {
    current_provider_for_app_from_db(db, app_type)?
        .ok_or_else(|| format!("{app_type:?} 当前供应商不存在，无法接管 Live 配置"))
}

#[derive(Debug, Clone)]
pub(crate) struct ProxyHotSwitchTargetState {
    pub(crate) provider: Provider,
    pub(crate) logical_target_changed: bool,
    pub(crate) has_live_backup: bool,
}

pub(crate) async fn proxy_hot_switch_target_state_from_db(
    db: &Database,
    app_type: &AppType,
    provider_id: &str,
) -> Result<ProxyHotSwitchTargetState, String> {
    let app_type_str = app_type.as_str();
    let provider = db
        .get_provider_by_id(provider_id, app_type_str)
        .map_err(|e| format!("读取供应商失败: {e}"))?
        .ok_or_else(|| format!("供应商不存在: {provider_id}"))?;

    if should_block_proxy_switch_to_provider(true, &provider) {
        return Err(
            "代理接管模式下不能切换到官方供应商 (Cannot switch to official provider during proxy takeover)"
                .to_string(),
        );
    }

    let logical_target_changed = crate::settings::get_effective_current_provider(db, app_type)
        .map_err(|e| format!("读取当前供应商失败: {e}"))?
        .as_deref()
        != Some(provider_id);

    let has_live_backup = db
        .get_live_backup(app_type_str)
        .await
        .map_err(|e| format!("读取 {app_type_str} 备份失败: {e}"))?
        .is_some();

    Ok(ProxyHotSwitchTargetState {
        provider,
        logical_target_changed,
        has_live_backup,
    })
}

pub(crate) fn persist_hot_switch_current_provider_sources(
    db: &Database,
    app_type: &AppType,
    provider_id: &str,
) -> Result<(), String> {
    db.set_current_provider(app_type.as_str(), provider_id)
        .map_err(|e| format!("更新当前供应商失败: {e}"))?;
    crate::settings::set_current_provider(app_type, Some(provider_id))
        .map_err(|e| format!("更新本地当前供应商失败: {e}"))
}

pub(crate) fn ssot_live_restore_provider_from_db(
    db: &Database,
    app_type: &AppType,
    proxy_token_placeholder: &str,
) -> Result<Option<Provider>, String> {
    let current_id = crate::settings::get_effective_current_provider(db, app_type)
        .map_err(|e| format!("获取 {app_type:?} 当前供应商失败: {e}"))?;

    let Some(current_id) = current_id else {
        return Ok(None);
    };

    let providers = db
        .get_all_providers(app_type.as_str())
        .map_err(|e| format!("读取 {app_type:?} 供应商列表失败: {e}"))?;

    let Some(provider) = providers.get(&current_id) else {
        return Ok(None);
    };

    if provider_settings_have_proxy_placeholder_for_app(provider, app_type, proxy_token_placeholder)
    {
        log::warn!(
            "{app_type:?} 当前供应商配置含代理接管占位符（疑似接管期间被导入的残留），跳过 SSOT 写回，改走占位符清理"
        );
        return Ok(None);
    }

    Ok(Some(provider.clone()))
}

pub(crate) fn write_ssot_live_restore_provider_with_common_config(
    db: &Database,
    app_type: &AppType,
    provider: &Provider,
) -> Result<(), String> {
    crate::services::provider::write_live_with_common_config(db, app_type, provider)
        .map_err(|e| format!("写入 {app_type:?} Live 配置失败: {e}"))
}

pub(crate) fn live_token_sync_app_label(app_type: &AppType) -> Option<&'static str> {
    core_live_token_sync_app_label(&AppKind::from(app_type))
}

pub(crate) fn live_token_sync_provider_from_db(
    db: &Database,
    app_type: &AppType,
) -> Result<Option<Provider>, String> {
    let Some(app_label) = live_token_sync_app_label(app_type) else {
        return Ok(None);
    };
    let Some(provider_id) = crate::settings::get_effective_current_provider(db, app_type)
        .map_err(|error| format!("获取 {app_label} 当前供应商失败: {error}"))?
    else {
        return Ok(None);
    };

    Ok(db
        .get_provider_by_id(&provider_id, app_type.as_str())
        .ok()
        .flatten())
}

pub(crate) fn update_live_token_sync_provider_settings_in_db(
    db: &Database,
    app_type: &AppType,
    app_label: &str,
    provider_id: &str,
    settings_config: &Value,
) {
    if let Err(e) =
        db.update_provider_settings_config(app_type.as_str(), provider_id, settings_config)
    {
        log::warn!("同步 {app_label} Token 到数据库失败: {e}");
    } else {
        log::info!("已同步 {app_label} Token 到数据库 (provider: {provider_id})");
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

pub(crate) fn emit_proxy_core_event(event: ProxyCoreEvent, mut emit: impl FnMut(String, Value)) {
    let message = proxy_core_event_to_bus_message(event);
    emit(message.event_name, message.payload);
}

pub(crate) fn emit_proxy_core_event_bus_source(events: &ProxyEventBus, event: ProxyCoreEvent) {
    emit_proxy_core_event(event, |event_name, payload| {
        events.emit(event_name, payload);
    });
}

#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::event_sink::CcSwitchEventSink;

pub(crate) fn provider_codex_auth_headers(
    auth: &ProviderAuthInfo,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, String> {
    build_codex_provider_auth_headers(auth).map_err(|error| error.to_string())
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
    provider_codex_api_key(provider).map(core_codex_auth_info_from_api_key)
}

#[cfg(test)]
pub(crate) fn codex_api_key_from_auth_and_config(
    auth: Option<&Value>,
    config_text: Option<&str>,
) -> Option<String> {
    crate::codex_config::extract_codex_api_key(auth, config_text)
}

pub(crate) fn provider_codex_base_url(provider: &Provider) -> Option<String> {
    core_codex_base_url_from_settings(&provider.settings_config)
}

pub(crate) fn required_codex_provider_base_url(provider: &Provider) -> Result<String, String> {
    core_required_provider_base_url("Codex", provider_codex_base_url(provider))
}

pub(crate) fn required_gemini_provider_base_url(provider: &Provider) -> Result<String, String> {
    core_required_provider_base_url(
        "Gemini",
        extract_gemini_base_url_from_settings(&provider.settings_config),
    )
}

pub(crate) fn required_claude_provider_base_url(provider: &Provider) -> Result<String, String> {
    core_required_provider_base_url("Claude", provider_claude_base_url(provider))
}

fn provider_codex_config_text(provider: &Provider) -> Option<&str> {
    codex_config_text_from_settings(&provider.settings_config)
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
    let codex_config_has_provider_key = if matches!(app_type, AppType::Codex) {
        provider_codex_config_text(&provider)
            .and_then(crate::codex_config::extract_codex_experimental_bearer_token)
            .is_some()
    } else {
        false
    };
    provider.category = Some(
        core_provider_default_live_import_category_from_parts(
            &AppKind::from(app_type),
            provider.settings_config.get("auth"),
            codex_config_has_provider_key,
        )
        .to_string(),
    );

    provider
}

pub(crate) fn codex_provider_live_write_parts<'a>(
    settings: &'a Value,
    provider: &'a Provider,
) -> Result<CodexProviderLiveWriteParts<'a>, CodexProviderLiveWriteIssue> {
    core_codex_provider_live_write_parts_from_settings(settings, provider.category.as_deref())
}

pub(crate) fn provider_codex_backfill_parts(provider: &Provider) -> CodexProviderBackfillParts<'_> {
    core_codex_provider_backfill_parts_from_settings(
        provider.category.as_deref(),
        &provider.settings_config,
    )
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
        warnings
            .push(ProviderBackfillSettingsWarning::CodexUnifiedSessionBucketStrip(err.to_string()));
    }

    // `modelCatalog` is a cc-switch-private field whose SSOT is the DB. Live's
    // `config.toml` only carries a lossy projection (`model_catalog_json` to a
    // generated catalog file) that proxy takeover/restore cycles and Codex.app
    // config rewrites can drop. Prefer the DB provider's stored catalog so a
    // switch-away backfill never erases it.
    settings = codex_live_settings_with_model_catalog(
        settings,
        provider.settings_config.get("modelCatalog").cloned(),
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

pub(crate) fn provider_codex_live_settings_parts(
    provider: &Provider,
) -> Result<CodexLiveSettingsParts<'_>, CodexLiveSettingsIssue> {
    core_codex_live_settings_parts_from_settings(
        &provider.settings_config,
        provider.category.as_deref(),
    )
}

pub(crate) fn provider_codex_live_snapshot_parts(
    provider: &Provider,
) -> Result<CodexLiveSnapshotParts<'_>, CodexLiveSnapshotIssue> {
    core_codex_live_snapshot_parts_from_settings(
        &provider.settings_config,
        provider.category.as_deref(),
    )
}

pub(crate) fn provider_settings_validation_parts<'a>(
    app_type: &AppType,
    provider: &'a Provider,
) -> Result<ProviderSettingsValidationParts<'a>, ProviderSettingsValidationIssue> {
    core_provider_settings_validation_parts_from_settings(
        &AppKind::from(app_type),
        &provider.settings_config,
    )
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
        .filter(|auth| core_codex_auth_has_oauth_login_material(auth))
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

fn with_provider_codex_chat_completions_facts<T>(
    provider: &Provider,
    evaluate: impl FnOnce(CodexProviderChatCompletionsFacts<'_>) -> T,
) -> T {
    let config_text = provider_codex_config_text(provider);
    let wire_api = config_text.and_then(core_codex_wire_api_from_config_toml);
    let config_base_url = config_text.and_then(crate::codex_config::extract_codex_base_url);

    evaluate(CodexProviderChatCompletionsFacts {
        api_format: provider
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
        wire_api: wire_api.as_deref(),
        base_url: provider
            .settings_config
            .get("base_url")
            .or_else(|| provider.settings_config.get("baseURL"))
            .and_then(Value::as_str),
        config_base_url: config_base_url.as_deref(),
    })
}

pub(crate) fn provider_codex_uses_chat_completions(provider: &Provider) -> bool {
    with_provider_codex_chat_completions_facts(provider, core_codex_provider_uses_chat_completions)
}

pub(crate) fn provider_should_convert_codex_responses_to_chat(
    provider: &Provider,
    endpoint: &str,
) -> bool {
    with_provider_codex_chat_completions_facts(provider, |provider_facts| {
        core_codex_responses_to_chat_conversion_required(CodexResponsesToChatConversionFacts {
            provider: provider_facts,
            endpoint,
        })
    })
}

pub(crate) fn provider_codex_upstream_model(provider: &Provider) -> Option<String> {
    let settings_model = provider
        .settings_config
        .get("model")
        .and_then(Value::as_str);
    let config_model =
        provider_codex_config_text(provider).and_then(core_codex_model_from_config_toml);
    resolve_codex_provider_upstream_model(settings_model, config_model.as_deref())
}

pub(crate) fn codex_takeover_toml_config_for_provider(
    toml_str: &str,
    proxy_url: &str,
    provider: Option<&Provider>,
) -> String {
    let upstream_model = provider.and_then(provider_codex_upstream_model);
    let patch = crate::proxy_core::api::ports::codex_takeover_toml_config_patch(
        proxy_url,
        upstream_model.as_deref(),
    );

    let updated =
        crate::codex_config::update_codex_toml_field(toml_str, "base_url", patch.base_url)
            .unwrap_or_else(|_| toml_str.to_string());
    let mut updated =
        crate::codex_config::update_codex_toml_field(&updated, "wire_api", patch.wire_api)
            .unwrap_or(updated);

    if let Some(upstream_model) = patch.model {
        updated = crate::codex_config::update_codex_toml_field(&updated, "model", upstream_model)
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
    if let Some(provider) = provider {
        let model_catalog = provider
            .settings_config
            .get("modelCatalog")
            .cloned()
            .unwrap_or_else(|| json!({ "models": [] }));
        if let Some(root) = config.as_object_mut() {
            root.insert("modelCatalog".to_string(), model_catalog);
        }
    }
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

pub(crate) use crate::proxy_core::api::auth::extract_gemini_base_url_from_settings;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::ports::gemini_env_value_from_env_json;
pub(crate) use crate::proxy_core::api::ports::{
    gemini_live_backup_from_effective_settings, gemini_live_settings_from_env_json_and_config,
};

pub(crate) fn provider_gemini_env_map(
    provider: &Provider,
) -> Result<HashMap<String, String>, AppError> {
    Ok(gemini_env_string_map_from_settings(
        &provider.settings_config,
    ))
}

pub(crate) fn parse_gemini_env_file_strict(
    content: &str,
) -> Result<HashMap<String, String>, AppError> {
    crate::proxy_core::api::ports::parse_gemini_env_file_strict(content)
        .map_err(gemini_env_parse_issue_to_app_error)
}

pub(crate) fn gemini_env_parse_issue_to_app_error(issue: GeminiEnvParseIssue) -> AppError {
    let spec = core_gemini_env_parse_issue_spec(&issue);
    AppError::localized(spec.key, spec.zh, spec.en)
}

pub(crate) fn detect_gemini_auth_type(provider: &Provider) -> GeminiAuthType {
    core_detect_gemini_auth_type(GeminiAuthTypeInput {
        name: &provider.name,
        website_url: provider.website_url.as_deref(),
        partner_promotion_key: provider
            .meta
            .as_ref()
            .and_then(|meta| meta.partner_promotion_key.as_deref()),
        settings_config: &provider.settings_config,
    })
}

pub(crate) fn gemini_settings_validation_issue_to_app_error(
    issue: GeminiSettingsValidationIssue,
) -> AppError {
    let spec = core_gemini_settings_validation_issue_spec(issue);
    AppError::localized(spec.key, spec.zh, spec.en)
}

pub(crate) fn validate_gemini_settings_basic(settings: &Value) -> Result<(), AppError> {
    core_validate_gemini_settings_basic(settings)
        .map_err(gemini_settings_validation_issue_to_app_error)
}

pub(crate) fn validate_gemini_settings_strict(settings: &Value) -> Result<(), AppError> {
    core_validate_gemini_settings_strict(settings)
        .map_err(gemini_settings_validation_issue_to_app_error)
}

pub(crate) fn validate_provider_gemini_settings(provider: &Provider) -> Result<(), AppError> {
    validate_gemini_settings_basic(&provider.settings_config)
}

pub(crate) fn validate_provider_gemini_settings_strict(
    provider: &Provider,
) -> Result<(), AppError> {
    validate_gemini_settings_strict(&provider.settings_config)
}

pub(crate) fn provider_gemini_live_config_object(
    provider: &Provider,
) -> Result<Option<&Value>, GeminiLiveConfigIssue> {
    core_gemini_live_config_object_from_settings(&provider.settings_config)
}

pub(crate) use crate::proxy_core::api::ports::gemini_live_settings_to_write;

pub(crate) fn provider_gemini_kind(provider: &Provider) -> ProviderKind {
    if extract_gemini_api_key_from_settings(&provider.settings_config)
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
    core_gemini_auth_strategy_for_provider_kind(&provider_gemini_kind(provider))
}

pub(crate) fn provider_gemini_auth_info(provider: &Provider) -> Option<ProviderAuthInfo> {
    let key = extract_gemini_api_key_from_settings(&provider.settings_config)?;
    let strategy = provider_gemini_auth_strategy(provider);
    let credentials = parse_gemini_oauth_credentials(&key);
    Some(core_gemini_auth_info_from_api_key(
        key,
        strategy,
        credentials.as_ref(),
    ))
}

pub(crate) fn provider_gemini_auth_headers(
    auth: &ProviderAuthInfo,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, String> {
    build_gemini_provider_auth_headers(auth).map_err(|error| error.to_string())
}

pub(crate) use crate::proxy::host::cc_switch::provider_adapter_context::{
    forwarder_provider_adapter_context_for_app, ForwarderAdapterContext,
};
pub(crate) use crate::proxy_core::api::transforms::resolve_claude_api_format_from_settings;

pub(crate) fn provider_claude_api_format(provider: &Provider) -> &'static str {
    let meta = provider.meta.as_ref();
    resolve_claude_api_format_from_settings(
        meta.and_then(|meta| meta.provider_type.as_deref()),
        meta.and_then(|meta| meta.api_format.as_deref()),
        &provider.settings_config,
    )
}

pub(crate) fn resolve_forwarder_claude_api_format(
    provider: &Provider,
    is_copilot: bool,
    copilot_model_vendor: Option<&str>,
) -> String {
    resolve_claude_forward_api_format(
        provider_claude_api_format(provider),
        is_copilot,
        copilot_model_vendor,
    )
}

pub(crate) fn stream_check_provider_base_url(
    app_type: &AppType,
    provider: &Provider,
) -> Result<String, AppError> {
    match app_type {
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => {
            core_additive_provider_stream_check_base_url_from_settings(
                &AppKind::from(app_type),
                &provider.settings_config,
            )
            .ok_or_else(|| missing_stream_check_base_url_error(app_type))
        }
        _ => forwarder_provider_adapter_context_for_app(app_type)
            .provider_url_facts(provider)
            .map(|facts| facts.base_url)
            .map_err(|e| AppError::Message(format!("Failed to extract base_url: {e}"))),
    }
}

fn missing_stream_check_base_url_error(app_type: &AppType) -> AppError {
    if let Some(spec) =
        core_additive_stream_check_base_url_missing_error_spec(&AppKind::from(app_type))
    {
        AppError::localized(spec.key, spec.zh, spec.en)
    } else {
        AppError::Message("base_url 为空".to_string())
    }
}

pub(crate) fn stream_check_proxy_target_ids_from_sources(
    proxy_targets_only: bool,
    current_provider_id: Option<String>,
    failover_provider_ids: impl IntoIterator<Item = String>,
) -> Option<HashSet<String>> {
    if !proxy_targets_only {
        return None;
    }

    let mut ids = HashSet::new();
    if let Some(current_provider_id) = current_provider_id {
        ids.insert(current_provider_id);
    }
    ids.extend(failover_provider_ids);
    Some(ids)
}

pub(crate) fn stream_check_proxy_target_ids_from_db(
    db: &Database,
    app_type: &str,
    proxy_targets_only: bool,
) -> Option<HashSet<String>> {
    if !proxy_targets_only {
        return None;
    }

    let current_provider_id = db.get_current_provider(app_type).ok().flatten();
    let failover_provider_ids = db
        .get_failover_queue(app_type)
        .ok()
        .into_iter()
        .flatten()
        .map(|item| item.provider_id);

    stream_check_proxy_target_ids_from_sources(
        proxy_targets_only,
        current_provider_id,
        failover_provider_ids,
    )
}

pub(crate) fn provider_needs_claude_transform(provider: &Provider) -> bool {
    core_claude_provider_transform_required(
        provider_claude_kind(provider).needs_transform(),
        provider_claude_api_format(provider),
    )
}

pub(crate) fn provider_claude_transform_streaming_decision(
    provider: &Provider,
    requested_streaming: bool,
    response_headers: &HeaderMap,
    api_format: &str,
) -> ClaudeTransformStreamingDecision {
    core_claude_transform_streaming_decision(
        requested_streaming,
        response_headers,
        api_format,
        provider_is_codex_oauth(provider),
    )
}

pub(crate) fn codex_chat_transform_streaming_decision(
    requested_streaming: bool,
    response_headers: &HeaderMap,
) -> CodexChatTransformStreamingDecision {
    core_codex_chat_transform_streaming_decision(requested_streaming, response_headers)
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

fn claude_gemini_cli_auth_info(provider: &Provider, key: String) -> ProviderAuthInfo {
    let credentials = parse_gemini_oauth_credentials(&key);
    let (auth, warning) = core_claude_gemini_cli_auth_info_from_api_key(key, credentials.as_ref());

    if warning.is_some() {
        log::warn!(
            "[Gemini OAuth] access_token missing or empty for provider `{}`; \
             bearer auth will likely fail with 401. Refresh \
             ~/.gemini/oauth_creds.json via the gemini CLI to obtain a new token.",
            provider.id
        );
    }

    auth
}

pub(crate) fn provider_claude_auth_info(provider: &Provider) -> Option<ProviderAuthInfo> {
    let provider_type = provider_claude_kind(provider);

    if let Some(auth) = core_managed_provider_auth_info_for_provider_kind(&provider_type) {
        return Some(auth);
    }

    let auth_key = provider_claude_auth_key(provider);
    log_claude_auth_key_source(auth_key.as_ref());
    let auth_key = auth_key?;
    let key = auth_key.key;

    match provider_type {
        ProviderKind::GeminiCli => Some(claude_gemini_cli_auth_info(provider, key)),
        _ => Some(core_claude_static_auth_info_from_key(
            key,
            &provider_type,
            auth_key.source,
        )),
    }
}

pub(crate) use crate::proxy_core::api::domain::extract_claude_base_url_from_settings;

pub(crate) fn provider_claude_base_url(provider: &Provider) -> Option<String> {
    extract_claude_base_url_from_settings(
        provider_is_codex_oauth(provider),
        &provider.settings_config,
    )
}

pub(crate) fn provider_claude_auth_headers(
    auth: &ProviderAuthInfo,
) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, String> {
    let request_id = Uuid::new_v4().to_string();
    build_claude_provider_auth_headers(ClaudeProviderAuthHeadersInput {
        auth,
        copilot_request_id: &request_id,
        copilot_editor_version: COPILOT_EDITOR_VERSION,
        copilot_editor_plugin_version: COPILOT_PLUGIN_VERSION,
        copilot_integration_id: COPILOT_INTEGRATION_ID,
        copilot_user_agent: COPILOT_USER_AGENT,
        copilot_github_api_version: COPILOT_API_VERSION,
    })
    .map_err(|error| error.to_string())
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
    let preserve_reasoning_content =
        provider_should_preserve_reasoning_content_for_openai_chat(provider, &body);
    let output = claude_request_transform_for_api_format(
        body,
        api_format,
        ClaudeApiFormatRequestTransformContext {
            provider_id: &provider.id,
            responses_prompt_cache_key: cache_key_resolution.key.as_deref(),
            responses_prompt_cache_key_source: cache_key_resolution.source,
            chat_prompt_cache_key: provider_claude_prompt_cache_key(provider),
            is_codex_oauth,
            codex_fast_mode_enabled: provider_codex_fast_mode_enabled(provider),
            preserve_reasoning_content,
            shadow_store,
            session_id,
        },
    )?;
    if let Some(cache_log) = output.responses_prompt_cache_log {
        log::debug!("{}", cache_log.message());
    }
    Ok(output.request)
}

#[cfg(test)]
pub(crate) fn provider_claude_transform_response(body: Value) -> Result<Value, String> {
    // This test helper does not receive provider config, so detect structurally
    // disjoint upstream response formats by their top-level fields.
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

pub(crate) fn provider_claude_transform_response_for_api_format(
    body: &Value,
    api_format: &str,
    shadow_store: Option<&GeminiShadowStore>,
    provider_id: Option<&str>,
    session_id: Option<&str>,
    tool_schema_hints: Option<&AnthropicToolSchemaHints>,
) -> Result<Value, String> {
    let output = claude_response_to_anthropic_message_for_api_format(
        body,
        api_format,
        shadow_store,
        provider_id,
        session_id,
        tool_schema_hints,
        synthesize_gemini_tool_call_id_with_uuid,
    )?;
    for name in &output.rectified_tool_names {
        log_rectified_gemini_tool_args(name);
    }
    Ok(output.response)
}

pub(crate) fn provider_claude_transform_sse_for_api_format(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    api_format: &str,
    shadow_store: Option<Arc<GeminiShadowStore>>,
    provider_id: Option<String>,
    session_id: Option<String>,
    tool_schema_hints: Option<AnthropicToolSchemaHints>,
) -> Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + Unpin> {
    create_claude_to_anthropic_sse_stream_for_api_format(
        stream,
        api_format,
        ClaudeApiFormatSseTransformContext {
            shadow_store,
            provider_id,
            session_id,
            tool_schema_hints,
            synthesize_gemini_tool_call_id: synthesize_gemini_tool_call_id_with_uuid,
            on_rectified_tool_name: log_rectified_gemini_tool_args,
        },
    )
}

pub(crate) fn provider_should_preserve_reasoning_content_for_openai_chat(
    provider: &Provider,
    body: &Value,
) -> bool {
    should_preserve_reasoning_content_for_openai_chat(&provider.settings_config, body)
}

pub(crate) async fn router_app_proxy_config_from_config_source(
    source: &(dyn ProxyConfigSource + Send + Sync),
    app_type: &str,
) -> Result<AppProxyConfig, AppError> {
    let app = AppKind::from(app_type);
    let config = source
        .load_app(&app)
        .await
        .map_err(|error| AppError::Message(error.to_string()))?;
    app_proxy_config_from_proxy_app_config(&config).map_err(AppError::Config)
}

pub(crate) async fn circuit_breaker_config_from_router_config_source(
    source: &(dyn ProxyConfigSource + Send + Sync),
    app_type: &str,
) -> CircuitBreakerConfig {
    let config = router_app_proxy_config_from_config_source(source, app_type)
        .await
        .ok();
    circuit_breaker_config_from_app_config(config.as_ref())
}

pub(crate) async fn circuit_failure_threshold_from_router_config_source(
    source: &(dyn ProxyConfigSource + Send + Sync),
    app_type: &str,
    fallback: u32,
) -> u32 {
    let config = router_app_proxy_config_from_config_source(source, app_type)
        .await
        .ok();
    circuit_failure_threshold_from_app_config(config.as_ref(), fallback)
}

pub(crate) struct ProviderHealthAttemptDbUpdate {
    pub(crate) provider_id: String,
    pub(crate) app_type: String,
    pub(crate) success: bool,
    pub(crate) error_msg: Option<String>,
    pub(crate) failure_threshold: u32,
}

pub(crate) fn provider_health_attempt_db_update(
    result: ProviderAttemptResult,
) -> ProviderHealthAttemptDbUpdate {
    ProviderHealthAttemptDbUpdate {
        provider_id: result.provider_id,
        app_type: result.app.as_str().to_string(),
        success: result.success,
        error_msg: result.error_message,
        failure_threshold: result.failure_threshold,
    }
}

pub(crate) async fn record_provider_attempt_in_db_source(
    db: &Database,
    result: ProviderAttemptResult,
) -> ProxyCoreResult<()> {
    let update = provider_health_attempt_db_update(result);
    db.update_provider_health_with_threshold(
        &update.provider_id,
        &update.app_type,
        update.success,
        update.error_msg,
        update.failure_threshold,
    )
    .await
    .map_err(|error| app_error("record provider attempt", error))
}

pub(crate) fn record_channel_health_attempt_from_router_db(
    db: &Database,
    result: ChannelAttemptResult,
) -> Result<(), AppError> {
    record_channel_attempt_in_db_source(db, result).map_err(app_error_from_proxy_core_error)
}

pub(crate) fn reset_channel_health_from_router_db(
    db: &Database,
    reset: ChannelHealthReset,
) -> Result<(), AppError> {
    db.reset_proxy_channel_health(&reset.channel_id)
}

pub(crate) fn auto_failover_enabled_from_router_config_result(
    app_type: &str,
    result: Result<AppProxyConfig, AppError>,
) -> bool {
    let decision = provider_router_auto_failover_enabled_decision(
        app_type,
        result.map(|config| config.auto_failover_enabled),
    );
    if let Some(log_line) = decision.error_log_line {
        log::error!("{log_line}");
    }
    decision.enabled
}

pub(crate) async fn auto_failover_enabled_from_router_config_source(
    source: &(dyn ProxyConfigSource + Send + Sync),
    app_type: &str,
) -> bool {
    auto_failover_enabled_from_router_config_result(
        app_type,
        router_app_proxy_config_from_config_source(source, app_type).await,
    )
}

pub(crate) async fn proxy_takeover_status_from_db(db: &Database) -> ProxyTakeoverStatus {
    let claude = db
        .get_proxy_config_for_app(AppType::Claude.as_str())
        .await
        .ok()
        .map(|config| config.enabled);
    let codex = db
        .get_proxy_config_for_app(AppType::Codex.as_str())
        .await
        .ok()
        .map(|config| config.enabled);
    let gemini = db
        .get_proxy_config_for_app(AppType::Gemini.as_str())
        .await
        .ok()
        .map(|config| config.enabled);
    proxy_takeover_status_from_enabled_options(claude, codex, gemini, None, None)
}

#[derive(Debug, Clone)]
pub(crate) struct AutoFailoverToggleDbPlan {
    pub(crate) config: AppProxyConfig,
    pub(crate) plan: AutoFailoverTogglePlan,
}

fn auto_failover_toggle_error_to_string(error: ProxyCoreError) -> String {
    match error {
        ProxyCoreError::InvalidRequest(message) => message,
        other => other.to_string(),
    }
}

pub(crate) fn auto_failover_toggle_plan_from_sources(
    config: AppProxyConfig,
    enabled: bool,
    queued_provider_ids: Vec<String>,
    current_provider_id: Option<String>,
) -> Result<AutoFailoverToggleDbPlan, String> {
    let plan = plan_auto_failover_toggle(AutoFailoverToggleInput::new(
        enabled,
        config.enabled,
        queued_provider_ids,
        current_provider_id,
    ))
    .map_err(auto_failover_toggle_error_to_string)?;

    Ok(AutoFailoverToggleDbPlan { config, plan })
}

pub(crate) async fn auto_failover_toggle_plan_from_db(
    db: &Database,
    app_type: &str,
    enabled: bool,
) -> Result<AutoFailoverToggleDbPlan, String> {
    let config = db
        .get_proxy_config_for_app(app_type)
        .await
        .map_err(|error| error.to_string())?;

    let mut current_provider_id = None;
    let queued_provider_ids = if enabled {
        let queue = db
            .get_failover_queue(app_type)
            .map_err(|error| error.to_string())?;

        if queue.is_empty() {
            let app_enum = app_type
                .parse::<AppType>()
                .map_err(|_| format!("无效的应用类型: {app_type}"))?;
            let current_id = crate::settings::get_effective_current_provider(db, &app_enum)
                .map_err(|error| error.to_string())?;
            current_provider_id = current_id;
        }

        queue.into_iter().map(|item| item.provider_id).collect()
    } else {
        Vec::new()
    };

    auto_failover_toggle_plan_from_sources(
        config,
        enabled,
        queued_provider_ids,
        current_provider_id,
    )
}

pub(crate) fn failover_switch_app_enabled_from_config_result(
    app_type: &str,
    result: Result<AppProxyConfig, AppError>,
) -> bool {
    match result {
        Ok(config) => config.enabled,
        Err(error) => {
            log::warn!("{}", failover_config_read_error_log_line(app_type, error));
            false
        }
    }
}

pub(crate) async fn failover_switch_app_enabled_from_db(db: &Database, app_type: &str) -> bool {
    failover_switch_app_enabled_from_config_result(
        app_type,
        db.get_proxy_config_for_app(app_type).await,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResetCircuitBreakerSwitchbackTarget {
    pub(crate) provider_id: String,
    pub(crate) provider_name: String,
    pub(crate) restored_sort_index: Option<usize>,
    pub(crate) current_sort_index: Option<usize>,
}

pub(crate) fn reset_circuit_breaker_switchback_target_from_sources(
    app_enabled: bool,
    auto_failover_enabled: bool,
    proxy_service_running: bool,
    restored_provider_id: &str,
    current_provider_id: Option<String>,
    queue: impl IntoIterator<Item = FailoverQueueItem>,
    provider_name: Option<String>,
) -> Option<ResetCircuitBreakerSwitchbackTarget> {
    let current_provider_id = current_provider_id?;
    let decision = restored_provider_switchback_decision(
        app_enabled,
        auto_failover_enabled,
        proxy_service_running,
        restored_provider_id,
        &current_provider_id,
        queue
            .into_iter()
            .map(|item| FailoverQueuePosition::new(item.provider_id, item.sort_index))
            .collect::<Vec<_>>(),
    );

    if !decision.should_switch {
        return None;
    }

    Some(ResetCircuitBreakerSwitchbackTarget {
        provider_id: restored_provider_id.to_string(),
        provider_name: provider_name.unwrap_or_else(|| restored_provider_id.to_string()),
        restored_sort_index: decision.restored_sort_index,
        current_sort_index: decision.current_sort_index,
    })
}

pub(crate) async fn reset_circuit_breaker_switchback_target_from_db(
    db: &Database,
    app_type: &str,
    restored_provider_id: &str,
    proxy_service_running: bool,
) -> Result<Option<ResetCircuitBreakerSwitchbackTarget>, AppError> {
    let (app_enabled, auto_failover_enabled) = match db.get_proxy_config_for_app(app_type).await {
        Ok(config) => (config.enabled, config.auto_failover_enabled),
        Err(error) => {
            log::error!(
                "[{app_type}] Failed to read proxy_config: {error}, defaulting to disabled"
            );
            return Ok(None);
        }
    };

    if !(app_enabled && auto_failover_enabled && proxy_service_running) {
        return Ok(None);
    }

    let current_provider_id = db.get_current_provider(app_type)?;
    let queue = db.get_failover_queue(app_type)?;
    let provider_name = db.get_all_providers(app_type).ok().and_then(|providers| {
        providers
            .get(restored_provider_id)
            .map(|provider| provider.name.clone())
    });

    Ok(reset_circuit_breaker_switchback_target_from_sources(
        app_enabled,
        auto_failover_enabled,
        proxy_service_running,
        restored_provider_id,
        current_provider_id,
        queue,
        provider_name,
    ))
}

#[cfg(test)]
pub(crate) fn select_current_provider_ids_from_router_source(
    app_type: &str,
    current: Option<Provider>,
) -> Result<Vec<String>, AppError> {
    select_current_provider_ids_from_router_provider_id_source(
        app_type,
        current.map(|provider| provider.id),
    )
}

pub(crate) fn select_current_provider_ids_from_router_provider_id_source(
    app_type: &str,
    current_provider_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    let selected_ids =
        select_provider_ids(ProviderSelectionInput::current(current_provider_id.clone()))
            .map_err(|error| app_error_from_provider_selection_failure(app_type, error))?;

    Ok(selected_ids
        .into_iter()
        .filter(|provider_id| current_provider_id.as_ref() == Some(provider_id))
        .collect())
}

pub(crate) async fn provider_ids_from_router_provider_source(
    source: &(dyn ProviderSource + Send + Sync),
    app_type: &str,
) -> Result<Vec<String>, AppError> {
    let app = AppKind::from(app_type);
    let providers = source
        .list_providers(&app)
        .await
        .map_err(app_error_from_proxy_core_error)?;
    Ok(providers.into_iter().map(|provider| provider.id).collect())
}

pub(crate) async fn select_current_provider_ids_from_router_provider_source(
    source: &(dyn ProviderSource + Send + Sync),
    app_type: &str,
) -> Result<Vec<String>, AppError> {
    let app = AppKind::from(app_type);
    let current_provider_id = source
        .current_provider_id(&app)
        .await
        .map_err(app_error_from_proxy_core_error)?;
    let current_provider_id = match current_provider_id {
        Some(current_provider_id) => source
            .get_provider(&app, &current_provider_id)
            .await
            .map_err(app_error_from_proxy_core_error)?
            .map(|provider| provider.id),
        None => None,
    };

    select_current_provider_ids_from_router_provider_id_source(app_type, current_provider_id)
}

pub(crate) async fn failover_provider_ids_from_route_policy_source(
    source: &(dyn RoutePolicySource + Send + Sync),
    app_type: &str,
) -> Result<Vec<String>, AppError> {
    let app = AppKind::from(app_type);
    let policy = source
        .load_policy(&app)
        .await
        .map_err(app_error_from_proxy_core_error)?;
    Ok(policy
        .as_ref()
        .map(route_policy_failover_provider_ids)
        .unwrap_or_default())
}

pub(crate) fn select_failover_provider_ids_from_router_lookup_availability<I>(
    app_type: &str,
    provider_ids: &[String],
    lookup_availability: I,
) -> Result<Vec<String>, AppError>
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
        .filter(|provider_id| {
            provider_ids
                .iter()
                .any(|configured_provider_id| configured_provider_id == provider_id)
        })
        .collect())
}

pub(crate) fn provider_failover_circuit_lookups_from_router_sources(
    app_type: &str,
    failover_provider_ids: impl IntoIterator<Item = String>,
    provider_ids: impl IntoIterator<Item = String>,
) -> Vec<ProviderFailoverCircuitLookup> {
    provider_failover_circuit_lookups(
        app_type,
        failover_provider_ids.into_iter().collect::<Vec<_>>(),
        provider_ids.into_iter().collect::<Vec<_>>(),
    )
}

pub(crate) async fn provider_failover_sources_from_router_provider_source(
    source: &(dyn ProviderSource + Send + Sync),
    app_type: &str,
    failover_provider_ids: impl IntoIterator<Item = String>,
) -> Result<ProviderFailoverRouterSources, AppError> {
    let provider_ids = provider_ids_from_router_provider_source(source, app_type).await?;
    let lookups = provider_failover_circuit_lookups_from_router_sources(
        app_type,
        failover_provider_ids,
        provider_ids.clone(),
    );
    Ok(ProviderFailoverRouterSources {
        provider_ids,
        lookups,
    })
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
    should_block_proxy_switch_to_provider_category(
        proxy_takeover_active,
        provider.category.as_deref(),
    )
}

pub(crate) fn proxy_hot_switch_should_refresh_codex_live_from_backup(
    app_type: &AppType,
    has_live_backup: bool,
    live_taken_over: bool,
) -> bool {
    core_proxy_hot_switch_should_refresh_codex_live_from_backup(
        &AppKind::from(app_type),
        has_live_backup,
        live_taken_over,
    )
}

pub(crate) fn proxy_hot_switch_should_sync_codex_live_while_proxy_active(
    app_type: &AppType,
    live_taken_over: bool,
) -> bool {
    core_proxy_hot_switch_should_sync_codex_live_while_proxy_active(
        &AppKind::from(app_type),
        live_taken_over,
    )
}

pub(crate) fn proxy_hot_switch_should_sync_claude_live_while_proxy_active(
    app_type: &AppType,
    proxy_live_owned_by_takeover: bool,
) -> bool {
    core_proxy_hot_switch_should_sync_claude_live_while_proxy_active(
        &AppKind::from(app_type),
        proxy_live_owned_by_takeover,
    )
}

pub(crate) fn toml_value_is_subset(target: &toml_edit::Value, source: &toml_edit::Value) -> bool {
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

pub(crate) fn toml_remove_array_items(target: &mut toml_edit::Array, source: &toml_edit::Array) {
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
                (target_value, source_value)
                    if toml_value_is_subset(target_value, source_value) =>
                {
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
        AppType::Claude => core_contains_claude_common_config_snippet(settings, trimmed),
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
        AppType::Gemini => core_contains_gemini_common_config_snippet(settings, trimmed),
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes | AppType::ClaudeDesktop => false,
    }
}

pub(crate) fn provider_uses_common_config(
    app_type: &AppType,
    provider: &Provider,
    snippet: Option<&str>,
) -> bool {
    let explicit_enabled = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.common_config_enabled);
    let settings_contains_snippet = explicit_enabled.is_none()
        && snippet.is_some_and(|value| {
            contains_common_config_snippet(app_type, &provider.settings_config, value)
        });

    core_provider_uses_common_config_from_parts(
        explicit_enabled,
        snippet,
        settings_contains_snippet,
    )
}

pub(crate) fn provider_common_config_storage_normalization_requires_snippet(
    provider: &Provider,
) -> bool {
    let explicit_enabled = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.common_config_enabled);

    core_provider_common_config_storage_normalization_requires_snippet(explicit_enabled)
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
        AppType::Claude => core_apply_claude_common_config_to_settings(settings, trimmed),
        AppType::Codex => {
            let mut result = settings.clone();
            let config_toml = codex_config_text_from_settings(settings).unwrap_or("");
            let mut target_doc = if config_toml.trim().is_empty() {
                toml_edit::DocumentMut::new()
            } else {
                config_toml.parse::<toml_edit::DocumentMut>().map_err(|e| {
                    CommonConfigSettingsMutationIssue::CodexApplyTargetToml(e.to_string())
                })?
            };
            let source_doc = trimmed.parse::<toml_edit::DocumentMut>().map_err(|e| {
                CommonConfigSettingsMutationIssue::CodexCommonConfigSnippetToml(e.to_string())
            })?;

            merge_toml_table_like(target_doc.as_table_mut(), source_doc.as_table());
            if let Some(obj) = result.as_object_mut() {
                obj.insert("config".to_string(), Value::String(target_doc.to_string()));
            }
            Ok(result)
        }
        AppType::Gemini => core_apply_gemini_common_config_to_settings(settings, trimmed),
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
                Err(issue) => {
                    warnings.push(ProviderEffectiveSettingsWarning::CommonConfigApply(issue))
                }
            }
        }
    }

    ProviderEffectiveSettingsResult { settings, warnings }
}

pub(crate) fn provider_effective_settings_with_common_config_from_db(
    db: &Database,
    app_type: &AppType,
    provider: &Provider,
) -> Result<Value, AppError> {
    let snippet = db.get_config_snippet(app_type.as_str())?;
    let result =
        build_effective_settings_with_common_config(app_type, provider, snippet.as_deref());
    log_provider_effective_settings_warnings(app_type, provider, result.warnings);

    Ok(result.settings)
}

fn log_provider_effective_settings_warnings(
    app_type: &AppType,
    provider: &Provider,
    warnings: Vec<ProviderEffectiveSettingsWarning>,
) {
    for warning in warnings {
        match warning {
            ProviderEffectiveSettingsWarning::CommonConfigApply(issue) => {
                let err = common_config_settings_mutation_issue_message(issue);
                log::warn!(
                    "Failed to apply common config for {} provider '{}': {err}",
                    app_type.as_str(),
                    provider.id
                );
            }
        }
    }
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
        AppType::Claude => core_remove_claude_common_config_from_settings(settings, trimmed),
        AppType::Codex => {
            let mut result = settings.clone();
            let config_toml = codex_config_text_from_settings(settings).unwrap_or("");
            let mut target_doc = if config_toml.trim().is_empty() {
                toml_edit::DocumentMut::new()
            } else {
                config_toml.parse::<toml_edit::DocumentMut>().map_err(|e| {
                    CommonConfigSettingsMutationIssue::CodexRemoveTargetToml(e.to_string())
                })?
            };
            let source_doc = trimmed.parse::<toml_edit::DocumentMut>().map_err(|e| {
                CommonConfigSettingsMutationIssue::CodexCommonConfigSnippetToml(e.to_string())
            })?;

            remove_toml_table_like(target_doc.as_table_mut(), source_doc.as_table());
            if let Some(obj) = result.as_object_mut() {
                obj.insert("config".to_string(), Value::String(target_doc.to_string()));
            }
            Ok(result)
        }
        AppType::Gemini => core_remove_gemini_common_config_from_settings(settings, trimmed),
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

    let result = restore_live_settings_for_provider_backfill(app_type, provider, backfill_settings);
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

#[cfg(test)]
pub(crate) fn provider_is_official_category(provider: &Provider) -> bool {
    core_provider_category_is_official(provider.category.as_deref())
}

pub(crate) fn should_emit_proxy_official_warning_for_provider(provider: &Provider) -> bool {
    core_should_emit_proxy_official_warning_for_provider_category(provider.category.as_deref())
}

pub(crate) fn should_reapply_codex_official_live_for_provider(provider: &Provider) -> bool {
    core_should_reapply_codex_official_live_for_provider_category(provider.category.as_deref())
}

pub(crate) use crate::proxy_core::api::routing::{
    apply_route_candidate_circuit_availability, current_provider_db_fallback_required,
    current_provider_id_from_sources, current_provider_id_option_from_sources,
    failover_switch_pending_key, legacy_provider_codex_catalog_models_from_settings,
    legacy_provider_config_text_from_settings, legacy_provider_env_from_settings,
    normalize_channel_base_url, normalize_proxy_channel_key_patch_request_fields,
    normalize_proxy_channel_key_write_request_fields,
    normalize_proxy_channel_model_write_request_fields,
    normalize_proxy_channel_models_replace_request_fields,
    normalize_proxy_channel_patch_request_fields, normalize_proxy_channel_write_request_fields,
    normalize_required_channel_string, plan_auto_failover_toggle,
    provider_failover_circuit_lookups, provider_selection_candidate_from_failover_lookup,
    resolve_channel_route, restored_provider_switchback_decision,
    route_candidate_channel_circuit_keys, select_provider_ids,
    should_block_proxy_switch_to_provider_category, stable_channel_id,
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
        codex_wire_api: config_text.and_then(core_codex_wire_api_from_config_toml),
        codex_model: config_text.and_then(core_codex_model_from_config_toml),
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

impl From<&AppType> for AppKind {
    fn from(value: &AppType) -> Self {
        Self::from(value.as_str())
    }
}

pub(crate) fn proxy_core_app_kind_from_app_type(app_type: &AppType) -> ProxyCoreAppKind {
    AppKind::from(app_type)
}

pub(crate) fn provider_adapter_kind_for_app_type(app_type: &AppType) -> AppProviderAdapterKind {
    crate::proxy_core::api::domain::provider_adapter_kind_for_app(&AppKind::from(app_type))
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
        .map_err(unsupported_app_kind_config_error)
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

pub(crate) struct CodexResponsesProxyRequest {
    pub(crate) request: ProxyRequest,
    pub(crate) tool_context: CodexToolContext,
}

pub(crate) fn codex_responses_proxy_request_from_input(
    input: JsonProxyRequestInput,
) -> CodexResponsesProxyRequest {
    let tool_context = codex_tool_context_from_request(&input.body);
    let request = json_proxy_request_from_input(input);
    CodexResponsesProxyRequest {
        request,
        tool_context,
    }
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

pub(crate) fn proxy_provider_to_core_spec(provider: &Provider, app_type: &AppType) -> ProviderSpec {
    let kind = provider_kind_from_app_type_and_config(app_type, provider);
    let metadata = provider_metadata_without_secrets(provider);

    ProviderSpec {
        id: provider.id.clone(),
        name: provider.name.clone(),
        kind,
        account_ref: account_ref(provider),
        metadata,
    }
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
    result: Result<Vec<String>, AppError>,
) -> ProxyCoreResult<Vec<String>> {
    let selection_result = match result {
        Ok(provider_ids) => Ok(provider_ids),
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
    route_candidate_provider_ids_from_selection_result(
        router.select_provider_ids(app.as_str()).await,
    )
}

#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::database_channel_source::{
    channel_route_records_from_sources, channel_spec_from_source, proxy_channel_record_to_core_spec,
};

#[cfg(test)]
pub(crate) fn proxy_channel_record_to_route_resolve_channel_input(
    channel: ProxyChannelRecord,
) -> RouteResolveChannelInput {
    channel_record_to_route_resolve_channel_input(proxy_channel_record_to_core(channel))
}

pub(crate) fn channel_record_to_route_resolve_channel_input(
    channel: ChannelRecord,
) -> RouteResolveChannelInput {
    route_resolve_channel_input_from_record(RouteResolveChannelRecordInput {
        channel_id: channel.id,
        provider_id: channel.provider_id,
        channel_name: channel.name,
        status: channel.status,
        base_url: channel.base_url,
        interface_kind: channel.interface_kind,
        groups: channel.groups,
        models: channel
            .models
            .into_iter()
            .map(|model| RouteResolveModelRecordInput {
                public_model: model.public_model,
                upstream_model: model.upstream_model,
            })
            .collect(),
        priority: channel.priority,
        weight: channel.weight,
        source_kind: channel.source_kind,
    })
}

pub(crate) fn channel_records_to_route_resolve_channel_inputs(
    channels: impl IntoIterator<Item = ChannelRecord>,
) -> Vec<RouteResolveChannelInput> {
    channels
        .into_iter()
        .map(channel_record_to_route_resolve_channel_input)
        .collect()
}

pub(crate) async fn router_channel_route_inputs_from_channel_source(
    source: &(dyn ChannelSource + Send + Sync),
    app_type: &str,
) -> ProxyCoreResult<(Vec<RouteResolveChannelInput>, ChannelRouteSource)> {
    let app = AppKind::from(app_type);
    let (route_source, channels) = source.list_channel_records(&app).await?;
    Ok((
        channel_records_to_route_resolve_channel_inputs(channels),
        route_source,
    ))
}

pub(crate) fn claude_desktop_model_routes_to_core_inputs(
    routes: impl IntoIterator<Item = ClaudeDesktopResolvedProxyRoute>,
) -> Vec<ClaudeDesktopModelRouteInput> {
    routes
        .into_iter()
        .map(|route| ClaudeDesktopModelRouteInput::new(route.route_id, route.supports_1m))
        .collect()
}

#[cfg(test)]
use crate::proxy_core::api::model_catalog::DEFAULT_CODEX_MODEL_CONTEXT_WINDOW;
pub(crate) use crate::proxy_core::api::model_catalog::{
    build_codex_model_catalog_from_settings as codex_model_catalog_from_settings,
    client_model_catalog_raw_from_text, empty_client_model_catalog_raw,
    has_codex_model_catalog_specs as codex_settings_have_model_catalog_specs,
    provider_model_catalog_from_settings, simplify_codex_model_catalog,
};

pub(crate) fn provider_model_catalog_from_db_source(
    db: &Database,
    app: &AppKind,
    provider_id: &str,
) -> ProxyCoreResult<ModelCatalog> {
    let provider = db
        .get_provider_by_id(provider_id, app.as_str())
        .map_err(|error| app_error("load model catalog", error))?;
    Ok(provider_model_catalog_from_settings(
        provider_id,
        provider.as_ref().map(|provider| &provider.settings_config),
    ))
}

pub(crate) fn claude_desktop_provider_from_selection_result(
    result: Result<Vec<String>, AppError>,
    load_provider: impl FnOnce(&str) -> Result<Option<Provider>, AppError>,
) -> ProxyCoreResult<Provider> {
    let provider_ids = result.map_err(claude_desktop_provider_selection_error)?;
    let provider_id = provider_ids
        .into_iter()
        .next()
        .ok_or_else(claude_desktop_provider_unavailable_error)?;
    load_provider(&provider_id)
        .map_err(|error| app_error("load claude desktop provider", error))?
        .ok_or_else(claude_desktop_provider_unavailable_error)
}

pub(crate) async fn claude_desktop_model_routes_from_router_source(
    db: &Database,
    router: &ProviderRouter,
    app: &AppKind,
) -> ProxyCoreResult<Vec<ClaudeDesktopModelRouteInput>> {
    let provider_ids = router.select_provider_ids(app.as_str()).await;
    let provider = claude_desktop_provider_from_selection_result(provider_ids, |provider_id| {
        db.get_provider_by_id(provider_id, app.as_str())
    })?;
    let routes = provider_claude_desktop_proxy_model_routes(&provider).map_err(|issue| {
        app_error(
            "load claude desktop model routes",
            AppError::Config(format!(
                "Claude Desktop proxy model routes unavailable: {issue:?}"
            )),
        )
    })?;
    Ok(claude_desktop_model_routes_to_core_inputs(routes))
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

pub(crate) fn client_model_catalog_from_app_source(app: &AppKind) -> ProxyCoreResult<ModelCatalog> {
    let source = client_model_catalog_source_for_app(app.as_str());
    let raw = match source {
        ClientModelCatalogSource::CodexActiveConfig => {
            Some(codex_client_model_catalog_raw_from_active_config())
        }
        ClientModelCatalogSource::Empty => None,
    };
    Ok(
        crate::proxy_core::api::model_catalog::client_model_catalog_from_optional_raw(
            app.as_str(),
            raw,
        ),
    )
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

pub(crate) use crate::proxy::host::cc_switch::channel_auth_profile_attempts::required_forward_attempts_from_sources;
#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::channel_auth_profile_attempts::{
    apply_channel_auth_profile_providers_from_source, forward_attempts_from_plan,
    host_providers_for_plan, required_forward_attempts_from_plan,
};
#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::channel_key_runtime_source::select_enabled_proxy_channel_key_runtime_candidate;
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::channel_key_runtime_source::{
    channel_key_runtime_source_from_database, CcSwitchChannelKeyRuntimeSource,
};
#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::channel_reachability_probe::CcSwitchChannelReachabilityProbe;

pub(crate) type FailoverSwitchSchedulerRef = Arc<dyn FailoverSwitchScheduler + Send + Sync>;

pub(crate) trait FailoverSwitchScheduler {
    fn schedule_switch(&self, app_type: &str, target: ForwarderFailoverSwitchTarget);
}

pub(crate) use crate::proxy::host::cc_switch::failover_switch::failover_switch_scheduler_from_runtime_sources;

#[cfg(test)]
struct NoopFailoverSwitchScheduler;

#[cfg(test)]
impl FailoverSwitchScheduler for NoopFailoverSwitchScheduler {
    fn schedule_switch(&self, _app_type: &str, _target: ForwarderFailoverSwitchTarget) {}
}

#[cfg(test)]
pub(crate) fn noop_failover_switch_scheduler() -> FailoverSwitchSchedulerRef {
    Arc::new(NoopFailoverSwitchScheduler)
}

pub(crate) type ForwarderRuntimeStateSourceRef = Arc<dyn ForwarderRuntimeStateSource + Send + Sync>;

/// 活跃连接 RAII guard
///
/// 构造时把 `ProxyRuntimeStatus.active_connections` +1；Drop 时在 tokio runtime 上调度
/// 一个异步任务执行 -1，从而支持把 guard move 进流式 body future（stream 自然结束
/// 时 guard 与 future 一起 drop）。
///
/// 设计动机：之前在请求 wrapper 出口处同步 -1，但流式响应的 body 实际
/// 在 `create_logged_passthrough_stream` 内还会继续 yield 字节流，导致 UI 的
/// `active_connections` 计数过早归零。RAII guard 让"减量"由 Rust 类型系统驱动，
/// 不需要每条出口路径都手动调用。
pub(crate) struct ActiveConnectionGuard {
    runtime_state_source: ForwarderRuntimeStateSourceRef,
}

impl ActiveConnectionGuard {
    pub(crate) async fn acquire(runtime_state_source: ForwarderRuntimeStateSourceRef) -> Self {
        runtime_state_source
            .record_active_connection_acquired()
            .await;
        Self {
            runtime_state_source,
        }
    }
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        // Drop 不能 await：把减量操作调度到 tokio runtime
        let runtime_state_source = self.runtime_state_source.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                runtime_state_source
                    .record_active_connection_released()
                    .await;
            });
        }
        // 没有 runtime 时静默丢失计数（仅 UI 展示用，可接受最终一致性）
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwarderFailoverSwitchTarget {
    pub(crate) provider_id: String,
    pub(crate) provider_name: String,
}

pub(crate) trait ForwarderRuntimeStateSource {
    fn next_request_id(&self) -> String;
    fn emit_request_started(&self, request_id: &str, app_type: &str);
    fn emit_attempt_started(&self, request_id: &str, app_type: &str, attempt: &ForwardAttempt);
    fn emit_attempt_succeeded(&self, request_id: &str, app_type: &str, attempt: &ForwardAttempt);
    fn emit_attempt_failed_for_error(
        &self,
        request_id: &str,
        app_type: &str,
        attempt: &ForwardAttempt,
        error: &ProxyError,
    );
    fn record_active_route_target<'a>(
        &'a self,
        request_id: &'a str,
        app_type: &'a str,
        attempt: &'a ForwardAttempt,
    ) -> BoxFuture<'a, ()>;
    fn record_success_status<'a>(
        &'a self,
        current_provider_id_at_start: &'a str,
        provider: &'a Provider,
    ) -> BoxFuture<'a, Option<ForwarderFailoverSwitchTarget>>;
    fn record_current_provider<'a>(&'a self, provider: &'a Provider) -> BoxFuture<'a, ()>;
    fn record_provider_failure<'a>(
        &'a self,
        provider: &'a Provider,
        error: &'a ProxyError,
    ) -> BoxFuture<'a, ()>;
    fn record_provider_rectifier_retry_failure<'a>(
        &'a self,
        provider: &'a Provider,
        kind: ForwarderRectifierRetryKind,
        error: &'a ProxyError,
    ) -> BoxFuture<'a, ()>;
    fn forward_failure_decision(&self, error: &ProxyError) -> ForwarderFailureDecision;
    fn log_retryable_forward_failure(
        &self,
        app_type: &str,
        error: &ProxyError,
        provider: &Provider,
        attempted_providers: usize,
        total_providers: usize,
    );
    fn log_terminal_forward_failure(
        &self,
        app_type: &str,
        attempted_providers: usize,
        total_providers: usize,
        last_error: Option<&ProxyError>,
    );
    fn rectifier_retry_failure_decision(
        &self,
        error: &ProxyError,
    ) -> ForwarderRectifierRetryFailureDecision;
    fn log_rectifier_retry_success(&self, app_type: &str, kind: ForwarderRectifierRetryKind);
    fn log_rectifier_retry_failure(
        &self,
        app_type: &str,
        kind: ForwarderRectifierRetryKind,
        error: &ProxyError,
    );
    fn record_forward_error_status<'a>(&'a self, error: &'a ProxyError) -> BoxFuture<'a, ()>;
    fn record_no_available_provider_status<'a>(&'a self) -> BoxFuture<'a, ()>;
    fn record_terminal_failure_status<'a>(&'a self) -> BoxFuture<'a, ()>;
    fn record_request_started_now<'a>(&'a self) -> BoxFuture<'a, ()>;
    fn record_active_connection_acquired<'a>(&'a self) -> BoxFuture<'a, ()>;
    fn record_active_connection_released<'a>(&'a self) -> BoxFuture<'a, ()>;
}

pub(crate) use crate::proxy::host::cc_switch::forwarder_runtime_state_source::forwarder_runtime_state_source_from_runtime_parts;

#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::forwarder_runtime_state_source::CcSwitchForwarderRuntimeStateSource;

pub(crate) type ForwarderProtocolStateSourceRef =
    Arc<dyn ForwarderProtocolStateSource + Send + Sync>;

pub(crate) struct ForwarderClaudeProtocolTransformInput<'a> {
    pub(crate) body: Value,
    pub(crate) provider: &'a Provider,
    pub(crate) api_format: Option<&'a str>,
    pub(crate) session_id: &'a str,
    pub(crate) session_client_provided: bool,
}

pub(crate) struct ForwarderCodexChatProtocolEnrichmentInput<'a> {
    pub(crate) body: &'a mut Value,
    pub(crate) enabled: bool,
}

pub(crate) trait ForwarderProtocolStateSource {
    fn enrich_codex_chat_request<'a>(
        &'a self,
        input: ForwarderCodexChatProtocolEnrichmentInput<'a>,
    ) -> BoxFuture<'a, ()>;
    fn transform_claude_request(
        &self,
        input: ForwarderClaudeProtocolTransformInput<'_>,
    ) -> Result<Value, String>;
}

pub(crate) use crate::proxy::host::cc_switch::forwarder_protocol_state_source::forwarder_protocol_state_source_from_runtime_parts;

#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::forwarder_protocol_state_source::CcSwitchForwarderProtocolStateSource;

pub(crate) type ForwarderAttemptRuntimeSourceRef =
    Arc<dyn ForwarderAttemptRuntimeSource + Send + Sync>;

pub(crate) struct ForwarderAttemptAllowInput<'a> {
    pub(crate) attempt: &'a ForwardAttempt,
    pub(crate) app_type: &'a str,
    pub(crate) attempts: &'a [ForwardAttempt],
    pub(crate) attempted_providers: usize,
    pub(crate) max_attempts: usize,
}

pub(crate) enum ForwarderAttemptAllowDecision {
    Stop,
    Skipped,
    Allowed { used_half_open_permit: bool },
}

pub(crate) trait ForwarderAttemptRuntimeSource {
    fn allow<'a>(
        &'a self,
        input: ForwarderAttemptAllowInput<'a>,
    ) -> BoxFuture<'a, ForwarderAttemptAllowDecision>;

    fn record_success<'a>(
        &'a self,
        attempt: &'a ForwardAttempt,
        app_type: &'a str,
        used_half_open_permit: bool,
    ) -> BoxFuture<'a, ()>;

    fn record_failure<'a>(
        &'a self,
        attempt: &'a ForwardAttempt,
        app_type: &'a str,
        used_half_open_permit: bool,
        error: &'a ProxyError,
    ) -> BoxFuture<'a, ()>;

    fn release_attempt_permit_neutral<'a>(
        &'a self,
        attempt: &'a ForwardAttempt,
        app_type: &'a str,
        used_half_open_permit: bool,
    ) -> BoxFuture<'a, ()>;
}

pub(crate) use crate::proxy::host::cc_switch::forwarder_attempt_runtime_source::forwarder_attempt_runtime_source_from_router;

pub(crate) type ForwarderAuthSourceRef = Arc<dyn ForwarderAuthSource + Send + Sync>;
pub(crate) type AuthProviderRef = Arc<dyn AuthProvider + Send + Sync>;

pub(crate) struct ForwarderAuthHeadersInput<'a> {
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) app_type: &'a AppType,
    pub(crate) method: &'a Method,
    pub(crate) endpoint: &'a str,
    pub(crate) request_body: &'a Value,
    pub(crate) request_headers: &'a HeaderMap,
    pub(crate) attempt: &'a ForwardAttempt,
    pub(crate) session_id: &'a str,
    pub(crate) session_client_provided: bool,
    pub(crate) copilot_optimization: Option<ForwarderPreparedCopilotAuthOptimization>,
}

pub(crate) trait ForwarderAuthSource {
    fn prepare_optional_copilot_auth_optimization(
        &self,
        input: ForwarderMaybeCopilotAuthOptimizationInput<'_>,
    ) -> Option<ForwarderPreparedCopilotAuthOptimization>;

    fn resolve_upstream_auth_headers<'a>(
        &'a self,
        input: ForwarderAuthHeadersInput<'a>,
    ) -> BoxFuture<'a, Result<ForwarderAuthHeaders, ProxyError>>;
}
pub(crate) use crate::proxy::host::cc_switch::forwarder_auth_source::forwarder_auth_source_from_managed_account_runtime_source;
#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::forwarder_auth_source::{
    default_forwarder_auth_source, forwarder_auth_source_from_sources,
};

pub(crate) type ForwarderRequestSourceRef = Arc<dyn ForwarderRequestSource + Send + Sync>;

pub(crate) struct ForwarderRequestPreparationInput<'a> {
    pub(crate) app: &'a str,
    pub(crate) provider_id: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) api_format: Option<&'a str>,
    pub(crate) body: Value,
    pub(crate) session_client_provided: bool,
    pub(crate) transform_plan: &'a ForwarderTransformPlan,
    pub(crate) initial_outbound_model: Option<String>,
    pub(crate) headers: &'a HeaderMap,
}

pub(crate) struct ForwarderPreparedRequest {
    pub(crate) body: Value,
    pub(crate) request_is_streaming: bool,
    pub(crate) force_identity_encoding: bool,
    pub(crate) body_model_label: String,
    pub(crate) outbound_model: Option<String>,
}

pub(crate) struct ForwarderUpstreamRequestLogInput<'a> {
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) url: &'a str,
    pub(crate) prepared_request: &'a ForwarderPreparedRequest,
}

pub(crate) struct ForwarderCopilotRequestOptimizationInput<'a> {
    pub(crate) body: Value,
    pub(crate) headers: &'a HeaderMap,
    pub(crate) config: &'a CopilotOptimizerConfig,
}

pub(crate) struct ForwarderCopilotRequestOptimizationGateInput<'a> {
    pub(crate) body: Value,
    pub(crate) headers: &'a HeaderMap,
    pub(crate) config: &'a CopilotOptimizerConfig,
    pub(crate) is_copilot: bool,
}

pub(crate) struct ForwarderCopilotLiveModelInput<'a> {
    pub(crate) provider: &'a Provider,
    pub(crate) body: &'a mut Value,
    pub(crate) is_copilot: bool,
}

pub(crate) struct ForwarderCopilotDynamicBaseUrlInput<'a> {
    pub(crate) provider: &'a Provider,
    pub(crate) base_url: &'a mut String,
    pub(crate) is_copilot: bool,
    pub(crate) is_full_url: bool,
}

pub(crate) struct ForwarderClaudeApiFormatInput<'a> {
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) provider: &'a Provider,
    pub(crate) body: &'a Value,
    pub(crate) is_copilot: bool,
}

pub(crate) struct ForwarderCopilotRequestOptimization {
    pub(crate) body: Value,
    pub(crate) classification: CopilotClassification,
}

pub(crate) struct ForwarderMaybeCopilotRequestOptimization {
    pub(crate) body: Value,
    pub(crate) classification: Option<CopilotClassification>,
}

pub(crate) struct ForwarderAttemptBodyInput<'a> {
    pub(crate) body: &'a Value,
    pub(crate) provider: &'a Provider,
    pub(crate) config: &'a OptimizerConfig,
}

pub(crate) struct ForwarderProviderRequestBodyInput<'a> {
    pub(crate) app_type: &'a AppType,
    pub(crate) body: Value,
    pub(crate) provider: &'a Provider,
    pub(crate) channel: Option<&'a ResolvedChannelAttempt>,
    pub(crate) is_copilot: bool,
}

pub(crate) struct ForwarderClaudeBodyPolicyInput<'a> {
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) body: &'a mut Value,
    pub(crate) provider: &'a Provider,
    pub(crate) api_format: Option<&'a str>,
    pub(crate) config: &'a RectifierConfig,
}

pub(crate) struct ForwarderCodexResponsesToChatInput<'a> {
    pub(crate) body: Value,
    pub(crate) provider: &'a Provider,
}

pub(crate) struct ForwarderProviderTransformInput<'a> {
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) body: Value,
    pub(crate) provider: &'a Provider,
}

pub(crate) struct ForwarderRequestBodyTransformInput<'a> {
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) body: Value,
    pub(crate) provider: &'a Provider,
    pub(crate) transform_plan: &'a ForwarderTransformPlan,
    pub(crate) claude_transformed_body: Option<Value>,
}

pub(crate) struct ForwarderRequestBodyTransform {
    pub(crate) body: Value,
    pub(crate) outbound_model: Option<String>,
}

pub(crate) struct ForwarderTransformPlanInput<'a> {
    pub(crate) app_type: &'a AppType,
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) endpoint: &'a str,
    pub(crate) provider: &'a Provider,
    pub(crate) resolved_claude_api_format: Option<&'a str>,
}

pub(crate) struct ForwarderUpstreamUrlInput<'a> {
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) base_url: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) is_full_url: bool,
    pub(crate) transform_plan: &'a ForwarderTransformPlan,
    pub(crate) is_copilot: bool,
    pub(crate) body: &'a Value,
    pub(crate) channel_param_overrides: Option<&'a Value>,
}

pub(crate) struct ForwarderMediaPreventionInput<'a> {
    pub(crate) body: &'a mut Value,
    pub(crate) provider: &'a Provider,
    pub(crate) config: &'a RectifierConfig,
}

pub(crate) struct ForwarderAppMediaPreventionInput<'a> {
    pub(crate) app_type: &'a AppType,
    pub(crate) body: &'a mut Value,
    pub(crate) provider: &'a Provider,
    pub(crate) config: &'a RectifierConfig,
}

pub(crate) struct ForwarderMediaRetryPlanInput<'a> {
    pub(crate) app: &'a str,
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) provider: &'a Provider,
    pub(crate) already_retried: bool,
    pub(crate) provider_body: &'a Value,
    pub(crate) error: &'a ProxyError,
    pub(crate) config: &'a RectifierConfig,
}

pub(crate) struct ForwarderMediaRetryPlan {
    pub(crate) body: Value,
}

pub(crate) struct ForwarderThinkingSignatureRectifierInput<'a> {
    pub(crate) app: &'a str,
    pub(crate) body: &'a mut Value,
    pub(crate) error: &'a ProxyError,
    pub(crate) already_retried: bool,
    pub(crate) config: &'a RectifierConfig,
}

pub(crate) struct ForwarderThinkingBudgetRectifierInput<'a> {
    pub(crate) app: &'a str,
    pub(crate) body: &'a mut Value,
    pub(crate) error: &'a ProxyError,
    pub(crate) already_retried: bool,
    pub(crate) config: &'a RectifierConfig,
}

pub(crate) struct ForwarderAnthropicRectifierGateInput<'a> {
    pub(crate) app_type: &'a AppType,
    pub(crate) provider: &'a Provider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForwarderRequestRectifierPlan {
    NotTriggered,
    AlreadyRetried,
    TriggeredUnchanged,
    Retry,
}

pub(crate) struct ForwarderRequestPartsInput<'a> {
    pub(crate) method: &'a Method,
    pub(crate) url: &'a str,
    pub(crate) inbound_headers: &'a HeaderMap,
    pub(crate) provider: &'a Provider,
    pub(crate) prepared_request: &'a ForwarderPreparedRequest,
    pub(crate) auth_headers: &'a [(http::HeaderName, http::HeaderValue)],
    pub(crate) channel_header_overrides: Option<&'a Value>,
    pub(crate) is_copilot: bool,
    pub(crate) adapter: &'a ForwarderAdapterContext,
    pub(crate) resolved_claude_api_format: Option<&'a str>,
    pub(crate) codex_oauth_session_headers: &'a [(http::HeaderName, http::HeaderValue)],
}

pub(crate) struct ForwarderUpstreamRequestParts {
    pub(crate) ordered_headers: HeaderMap,
    pub(crate) body: Vec<u8>,
    pub(crate) preserve_exact_header_case: bool,
}

pub(crate) trait ForwarderRequestSource {
    fn adapter_context_for_app(&self, app_type: &AppType) -> ForwarderAdapterContext;

    fn prepare_attempt_body(&self, input: ForwarderAttemptBodyInput<'_>) -> Value;

    fn prepare_provider_request_body(
        &self,
        input: ForwarderProviderRequestBodyInput<'_>,
    ) -> Result<Value, ProxyError>;

    fn apply_claude_body_policies(&self, input: ForwarderClaudeBodyPolicyInput<'_>);

    fn transform_request_body(
        &self,
        input: ForwarderRequestBodyTransformInput<'_>,
    ) -> Result<ForwarderRequestBodyTransform, ProxyError>;

    fn transform_plan(&self, input: ForwarderTransformPlanInput<'_>) -> ForwarderTransformPlan;

    fn protocol_preparation(
        &self,
        input: ForwarderProtocolPreparationInput<'_>,
    ) -> ForwarderProtocolPreparation;

    fn plan_upstream_url(&self, input: ForwarderUpstreamUrlInput<'_>) -> ForwardUpstreamUrlPlan;

    fn prepare_copilot_request_optimization(
        &self,
        input: ForwarderCopilotRequestOptimizationGateInput<'_>,
    ) -> ForwarderMaybeCopilotRequestOptimization;

    fn apply_copilot_live_model_for_adapter<'a>(
        &'a self,
        input: ForwarderCopilotLiveModelInput<'a>,
    ) -> BoxFuture<'a, ()>;

    fn apply_copilot_dynamic_base_url_for_provider<'a>(
        &'a self,
        input: ForwarderCopilotDynamicBaseUrlInput<'a>,
    ) -> BoxFuture<'a, ()>;

    fn resolve_claude_api_format_for_adapter<'a>(
        &'a self,
        input: ForwarderClaudeApiFormatInput<'a>,
    ) -> BoxFuture<'a, Option<String>>;

    fn apply_app_media_prevention(&self, input: ForwarderAppMediaPreventionInput<'_>) -> usize;

    fn media_retry_plan(
        &self,
        input: ForwarderMediaRetryPlanInput<'_>,
    ) -> Option<ForwarderMediaRetryPlan>;

    fn anthropic_rectifiers_enabled(&self, input: ForwarderAnthropicRectifierGateInput<'_>)
        -> bool;

    fn thinking_signature_rectifier_plan(
        &self,
        input: ForwarderThinkingSignatureRectifierInput<'_>,
    ) -> ForwarderRequestRectifierPlan;

    fn thinking_budget_rectifier_plan(
        &self,
        input: ForwarderThinkingBudgetRectifierInput<'_>,
    ) -> ForwarderRequestRectifierPlan;

    fn prepare_upstream_body(
        &self,
        input: ForwarderRequestPreparationInput<'_>,
    ) -> ForwarderPreparedRequest;

    fn log_upstream_request(&self, input: ForwarderUpstreamRequestLogInput<'_>);

    fn build_upstream_request_parts(
        &self,
        input: ForwarderRequestPartsInput<'_>,
    ) -> Result<ForwarderUpstreamRequestParts, ProxyError>;
}

pub(crate) use crate::proxy::host::cc_switch::forwarder_request_source::forwarder_request_source_from_managed_account_runtime_source;
#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::forwarder_request_source::{
    default_forwarder_request_source, forwarder_rectifier_error_message,
    CcSwitchForwarderRequestSource,
};

pub(crate) type ForwarderTransportSourceRef = Arc<dyn ForwarderTransportSource + Send + Sync>;

pub(crate) struct ForwarderUpstreamTransportRequest {
    pub(crate) method: Method,
    pub(crate) url: String,
    pub(crate) request_parts: ForwarderUpstreamRequestParts,
    pub(crate) extensions: http::Extensions,
    pub(crate) request_is_streaming: bool,
    pub(crate) non_streaming_timeout: std::time::Duration,
    pub(crate) streaming_first_byte_timeout: std::time::Duration,
}

pub(crate) trait ForwarderTransportSource {
    fn send_upstream_request<'a>(
        &'a self,
        request: ForwarderUpstreamTransportRequest,
    ) -> BoxFuture<'a, Result<ProxyResponse, ProxyError>>;
}

pub(crate) use crate::proxy::host::cc_switch::forwarder_transport_source::default_forwarder_transport_source;

pub(crate) type ForwarderResponseSourceRef = Arc<dyn ForwarderResponseSource + Send + Sync>;

pub(crate) struct ForwarderChannelResponseStatusInput<'a> {
    pub(crate) response: ProxyResponse,
    pub(crate) channel: Option<&'a ResolvedChannelAttempt>,
}

pub(crate) struct ForwarderResponseFinalizationInput {
    pub(crate) response: ProxyResponse,
    pub(crate) request_is_streaming: bool,
    pub(crate) non_streaming_timeout: std::time::Duration,
    pub(crate) streaming_first_byte_timeout: std::time::Duration,
}

pub(crate) trait ForwarderResponseSource {
    fn apply_channel_response_status_mapping(
        &self,
        input: ForwarderChannelResponseStatusInput<'_>,
    ) -> Result<ProxyResponse, ProxyError>;

    fn finalize_upstream_response<'a>(
        &'a self,
        input: ForwarderResponseFinalizationInput,
    ) -> BoxFuture<'a, Result<ProxyResponse, ProxyError>>;
}

pub(crate) use crate::proxy::host::cc_switch::forwarder_response_source::default_forwarder_response_source;

#[cfg(test)]
pub(crate) use crate::proxy::host::cc_switch::forwarder_response_source::CcSwitchForwarderResponseSource;

#[derive(Clone)]
pub(crate) struct ForwarderRuntimeHostResources {
    pub(crate) attempt_runtime_source: ForwarderAttemptRuntimeSourceRef,
    pub(crate) protocol_state_source: ForwarderProtocolStateSourceRef,
    pub(crate) runtime_state_source: ForwarderRuntimeStateSourceRef,
    pub(crate) auth_source: ForwarderAuthSourceRef,
    pub(crate) request_source: ForwarderRequestSourceRef,
    pub(crate) transport_source: ForwarderTransportSourceRef,
    pub(crate) response_source: ForwarderResponseSourceRef,
    pub(crate) failover_switch_scheduler: FailoverSwitchSchedulerRef,
}

pub(crate) fn forwarder_runtime_host_resources_from_runtime(
    runtime: &CcSwitchProxyRuntime,
) -> ForwarderRuntimeHostResources {
    ForwarderRuntimeHostResources {
        attempt_runtime_source: runtime.attempt_runtime_source.clone(),
        protocol_state_source: runtime.protocol_state_source.clone(),
        runtime_state_source: runtime.runtime_state_source.clone(),
        auth_source: runtime.auth_source.clone(),
        request_source: runtime.request_source.clone(),
        transport_source: runtime.transport_source.clone(),
        response_source: runtime.response_source.clone(),
        failover_switch_scheduler: runtime.failover_switch_scheduler.clone(),
    }
}

pub(crate) async fn forward_proxy_request_with_cc_switch_runtime(
    runtime: &CcSwitchProxyRuntime,
    channel_key_runtime_source: &(dyn ChannelKeyRuntimeSource + Send + Sync),
    request: ProxyRequest,
    plan: RoutePlan,
) -> ProxyCoreResult<ProxyResult> {
    forward_proxy_request_with_host_runtime(
        &runtime.db,
        forwarder_runtime_host_resources_from_runtime(runtime),
        channel_key_runtime_source,
        request,
        plan,
    )
    .await
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
        attempt_runtime_source,
        protocol_state_source,
        runtime_state_source,
        auth_source,
        request_source,
        transport_source,
        response_source,
        failover_switch_scheduler,
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
    let forwarder = RequestForwarder::new_preplanned(
        attempt_runtime_source,
        protocol_state_source,
        runtime_state_source,
        auth_source,
        request_source,
        transport_source,
        response_source,
        failover_switch_scheduler,
        forwarder_config,
        current_provider_id,
        session_result.session_id,
        session_result.client_provided,
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
    channel_key_runtime_source: &(dyn ChannelKeyRuntimeSource + Send + Sync),
    request: ProxyRequest,
    plan: RoutePlan,
) -> ProxyCoreResult<ProxyResult> {
    let forward_request = forward_runtime_request_from_proxy_request(request)?;
    let app_type = forward_request.app_type.clone();
    let forwarder_config = forwarder_runtime_config_from_db_sources(db, &app_type).await?;
    let current_provider_id = forward_current_provider_id_from_db_sources(db, &app_type);
    let all_providers = db
        .get_all_providers(app_type.as_str())
        .map_err(|error| app_error("load host providers", error))?;
    let attempts = required_forward_attempts_from_sources(
        &app_type,
        &all_providers,
        &plan,
        channel_key_runtime_source,
    )?;

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

#[allow(unused_imports)]
pub(crate) use crate::proxy::engine::response_pipeline::{
    claude_transform_tool_schema_hints, claude_transformed_json_response_from_context,
    claude_transformed_sse_stream_from_context, claude_transformed_streaming_usage_collector,
    codex_auto_transformed_json_response_from_context,
    codex_auto_transformed_sse_stream_from_context,
    codex_auto_transformed_streaming_usage_collector, create_claude_transformed_logged_stream,
    create_codex_auto_transformed_logged_stream, create_logged_passthrough_stream,
    create_passthrough_logged_stream, decode_raw_proxy_response_body,
    error_usage_record_from_provider_facts_with_request_id_fallback,
    fallback_response_usage_provider_facts, forward_error_usage_record_from_response_context,
    log_non_streaming_proxy_response_body, log_streaming_proxy_response_received,
    non_streaming_response_usage_record_from_provider_body_with_request_id_fallback,
    non_streaming_response_usage_record_from_response_context,
    passthrough_non_stream_proxy_response_from_context,
    passthrough_stream_proxy_response_from_context, passthrough_streaming_usage_collector,
    read_decoded_proxy_response_body, record_claude_transformed_response_usage,
    record_codex_auto_transformed_response_usage, record_forward_core_error_usage,
    record_forward_error_usage, record_forward_error_usage_from_context,
    record_non_streaming_response_usage, record_non_streaming_response_usage_from_context,
    record_transformed_response_usage, record_transformed_response_usage_from_context,
    record_usage_with_proxy_services, record_usage_with_proxy_services_context,
    response_usage_provider_facts, response_usage_provider_facts_from_optional,
    spawn_usage_record_with_proxy_services, spawn_usage_record_with_proxy_services_context,
    streaming_response_usage_record_from_provider_facts,
    streaming_response_usage_record_from_response_context, streaming_usage_collector_from_context,
    transformed_response_usage_record_from_provider_facts_with_request_id_fallback,
    transformed_response_usage_record_from_response_context,
    transformed_streaming_response_usage_record_from_provider_facts_with_request_id_fallback,
    transformed_streaming_response_usage_record_from_response_context,
    transformed_streaming_usage_collector, transformed_streaming_usage_collector_from_context,
    ClaudeTransformedJsonResponseContext, ClaudeTransformedSseStreamContext,
    CodexAutoTransformedJsonResponseContext, CodexAutoTransformedSseStreamContext,
    DecodedProxyResponseBody, ForwardErrorUsageContext, ForwardErrorUsageRecordContext,
    NonStreamingResponseUsageContext, NonStreamingUsageRecordContext, ResponseUsageProviderFacts,
    SseUsageCollector, StreamingResponseUsageContext, StreamingUsageCollectorContext,
    TransformedResponseUsageContext, TransformedResponseUsageRecordContext,
    TransformedStreamingResponseUsageContext, TransformedStreamingUsageCollectorContext,
};

pub(crate) trait ProxyServiceRuntimeResources:
    HostForwardRuntime + Clone + Send + Sync
{
    fn db(&self) -> Arc<Database>;
    fn config(&self) -> Arc<RwLock<ProxyConfig>>;
    fn provider_router(&self) -> Arc<ProviderRouter>;
    fn status(&self) -> Arc<RwLock<ProxyRuntimeStatus>>;
    fn start_time(&self) -> Arc<RwLock<Option<std::time::Instant>>>;
    fn current_providers(&self) -> Arc<RwLock<HashMap<String, CurrentRouteTarget>>>;
    fn events(&self) -> Arc<ProxyEventBus>;
}

pub(crate) trait HostForwardRuntime {
    fn forward_host<'a>(
        &'a self,
        channel_key_runtime_source: &'a (dyn ChannelKeyRuntimeSource + Send + Sync),
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>>;
}

pub(crate) fn forward_with_optional_host_runtime<'a, R>(
    runtime: Option<&'a R>,
    channel_key_runtime_source: &'a (dyn ChannelKeyRuntimeSource + Send + Sync),
    request: ProxyRequest,
    plan: RoutePlan,
) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>>
where
    R: HostForwardRuntime + Sync + 'a,
{
    Box::pin(async move {
        let runtime = runtime.ok_or_else(forwarding_runtime_unavailable_error)?;
        runtime
            .forward_host(channel_key_runtime_source, request, plan)
            .await
    })
}

pub(crate) use crate::proxy_core::api::routing::{
    forwarding_requires_runtime_error as forwarding_runtime_unavailable_error,
    route_plan_no_matching_host_providers_error, route_plan_providers_unconfigured_error,
};

#[cfg(test)]
pub(crate) use crate::proxy_core::api::routing::{
    forwarding_requires_runtime_error_message, route_plan_no_matching_host_providers_error_message,
    route_plan_providers_unconfigured_error_message,
};

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
        failure_threshold: result
            .failure_threshold
            .unwrap_or(DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD),
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
    Ok(channel_health_reset_from_parts(
        reset_plan.channel_id,
        reset_plan.app_type.as_str(),
    ))
}

pub(crate) async fn channel_breaker_stats_with_router_source(
    db: &Database,
    router: &ProviderRouter,
    channel_id: &str,
) -> ProxyCoreResult<ChannelBreakerStats> {
    let app_type = db
        .get_proxy_channel_app_type(channel_id)
        .map_err(|error| app_error("lookup channel app", error))?
        .ok_or_else(|| channel_not_found_error(channel_id))?;
    let stats = router
        .get_channel_circuit_breaker_stats(channel_id, &app_type)
        .await;

    Ok(channel_breaker_stats_from_parts(
        channel_id,
        app_type.as_str(),
        stats,
    ))
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
    let (channels, source) = router
        .list_route_channel_inputs_for_app(&request.app_type)
        .await
        .map_err(|error| app_error("list channel route inputs", error))?;
    let mut response = resolve_channel_route(request, channels, source)?;
    let availability = router
        .route_candidate_circuit_availability(route_candidate_channel_circuit_keys(&response))
        .await;
    apply_route_candidate_circuit_availability(&mut response, availability);
    Ok(response)
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

pub(crate) async fn provider_circuit_breaker_stats_source(
    router: &ProviderRouter,
    provider_id: &str,
    app_type: &str,
) -> Option<CircuitBreakerStats> {
    router
        .get_circuit_breaker_stats(provider_id, app_type)
        .await
}

pub(crate) use crate::proxy_core::api::transport::forward_failure_kind_from_proxy_status;

pub(crate) fn forward_failure_kind_from_proxy_error(error: &ProxyError) -> ForwardFailureKind {
    let upstream_body = match error {
        ProxyError::UpstreamError { body, .. } => body.clone(),
        _ => None,
    };
    forward_failure_kind_from_proxy_status(
        proxy_error_status_kind(error),
        forward_failure_message_from_proxy_error(error),
        upstream_body,
    )
}

fn forward_failure_message_from_proxy_error(error: &ProxyError) -> String {
    let raw_message = match error {
        ProxyError::Timeout(message)
        | ProxyError::ForwardFailed(message)
        | ProxyError::TransformError(message)
        | ProxyError::ConfigError(message)
        | ProxyError::AuthError(message) => message.as_str(),
        _ => "",
    };
    let display_message = error.to_string();
    core_forward_failure_message_from_proxy_status(
        proxy_error_status_kind(error),
        raw_message,
        &display_message,
    )
}

pub(crate) use crate::proxy_core::api::routing::default_route_candidate_from_selection as channel_route_candidate_from_selection;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::routing::resolved_channel_attempt_from_candidate;
pub(crate) use crate::proxy_core::api::routing::resolved_channel_attempt_from_selection;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::codex_proxy_error_code;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::codex_proxy_error_json;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::codex_proxy_error_response;

#[cfg(test)]
pub(crate) use crate::proxy::error_mapper::codex_proxy_error_response as codex_proxy_error_response_from_proxy_error;
#[cfg(test)]
pub(crate) use crate::proxy::error_mapper::{
    codex_proxy_error_json as codex_proxy_error_json_from_proxy_error,
    codex_proxy_error_json_from_host_facts, codex_proxy_error_response_from_host_facts,
    CodexProxyHostErrorFacts,
};

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
    normalize_claude_anthropic_messages, normalize_codex_chat_error_body,
};

pub(crate) fn provider_claude_normalize_anthropic_messages(
    body: &mut Value,
    provider: &Provider,
    api_format: &str,
) -> bool {
    normalize_claude_anthropic_messages(body, &provider.settings_config, api_format)
}

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::{
    normalize_anthropic_tool_thinking_history, normalize_deepseek_thinking_disabled_strip_effort,
    should_normalize_anthropic_tool_thinking_history,
};

#[cfg(test)]
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

pub(crate) fn apply_forward_request_model_mapping_from_provider(
    app_type: &AppType,
    body: Value,
    provider: &Provider,
) -> Result<ModelMappingProjection, ProxyError> {
    if matches!(app_type, AppType::ClaudeDesktop) {
        return provider_claude_desktop_proxy_request_body(body, provider)
            .map(|body| ModelMappingProjection {
                body,
                log_message: None,
            })
            .map_err(|issue| {
                ProxyError::InvalidRequest(claude_desktop_proxy_request_body_issue_message(issue))
            });
    }

    Ok(apply_provider_model_mapping_from_provider(body, provider))
}

fn claude_desktop_proxy_request_body_issue_message(
    issue: ClaudeDesktopProviderProxyRequestBodyIssue,
) -> String {
    match issue {
        ClaudeDesktopProviderProxyRequestBodyIssue::Routes(route_issue) => match route_issue {
            ClaudeDesktopProviderProxyRouteIssue::Missing => {
                "Claude Desktop proxy mode is missing model route mappings".to_string()
            }
            ClaudeDesktopProviderProxyRouteIssue::Empty => {
                "Claude Desktop proxy mode requires at least one model route mapping".to_string()
            }
        },
        ClaudeDesktopProviderProxyRequestBodyIssue::Body(body_issue) => match body_issue {
            ClaudeDesktopProxyRequestBodyIssue::MissingModel => {
                "Claude Desktop request is missing the model field".to_string()
            }
            ClaudeDesktopProxyRequestBodyIssue::UnknownRoute { requested_model } => {
                format!("Claude Desktop model route is not configured: {requested_model}")
            }
        },
    }
}

#[cfg(test)]
pub(crate) fn rewrite_codex_responses_endpoint_to_chat(endpoint: &str) -> (String, Option<String>) {
    crate::proxy_core::api::transport::rewrite_codex_responses_endpoint_to_chat(endpoint)
        .into_parts()
}

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::claude_api_format_needs_transform;
#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::resolve_gemini_native_url;

pub(crate) use crate::proxy_core::api::transport::request_body_stream_flag;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::is_streaming_upstream_request;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transport::decompress_body;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::transforms::strip_sse_field;

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

pub(crate) fn provider_kind_from_provider(provider: &Provider) -> Option<ProviderKind> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        .map(ProviderKind::from)
}

fn provider_managed_auth_classification(provider: &Provider) -> ProviderManagedAuthClassification {
    let provider_kind = provider_kind_from_provider(provider);
    core_classify_provider_managed_auth(ProviderManagedAuthFacts {
        provider_kind: provider_kind.as_ref(),
        anthropic_base_url: provider
            .settings_config
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(Value::as_str),
    })
}

pub(crate) fn provider_is_codex_oauth(provider: &Provider) -> bool {
    provider_managed_auth_classification(provider).is_codex_oauth
}

pub(crate) fn provider_is_github_copilot(provider: &Provider) -> bool {
    provider_managed_auth_classification(provider).is_github_copilot
}

pub(crate) fn provider_uses_managed_account_auth(provider: &Provider) -> bool {
    provider_managed_auth_classification(provider).uses_managed_account
}

pub(crate) fn provider_uses_anthropic_rectifiers(app_type: &AppType, provider: &Provider) -> bool {
    provider_kind_from_app_type_and_config(app_type, provider).uses_anthropic_rectifiers()
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

pub(crate) fn provider_managed_account_binding_input(
    meta: &ProviderMeta,
) -> Option<ManagedAccountBindingInput<'_>> {
    let binding = meta.auth_binding.as_ref()?;
    Some(ManagedAccountBindingInput {
        source: match binding.source {
            AuthBindingSource::ProviderConfig => ManagedAccountBindingSource::ProviderConfig,
            AuthBindingSource::ManagedAccount => ManagedAccountBindingSource::ManagedAccount,
        },
        auth_provider: binding.auth_provider.as_deref(),
        account_id: binding.account_id.as_deref(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderManagedAccountBindingContext<'a> {
    pub binding: Option<ManagedAccountBindingInput<'a>>,
    pub legacy_github_copilot_account_id: Option<&'a str>,
}

pub(crate) fn provider_managed_account_binding_context(
    provider: &Provider,
) -> ProviderManagedAccountBindingContext<'_> {
    let meta = provider.meta.as_ref();
    ProviderManagedAccountBindingContext {
        binding: meta.and_then(provider_managed_account_binding_input),
        legacy_github_copilot_account_id: meta.and_then(|meta| meta.github_account_id.as_deref()),
    }
}

pub(crate) fn provider_managed_account_id_for(
    provider: &Provider,
    auth_provider: &str,
) -> Option<String> {
    let context = provider_managed_account_binding_context(provider);
    core_managed_account_id_for_auth_provider(
        auth_provider,
        context.binding,
        context.legacy_github_copilot_account_id,
    )
}

pub(crate) fn provider_github_copilot_managed_account_id(provider: &Provider) -> Option<String> {
    provider_managed_account_id_for(provider, GITHUB_COPILOT_AUTH_PROVIDER)
}

pub(crate) fn provider_usage_script(provider: Option<&Provider>) -> Option<&UsageScript> {
    provider
        .and_then(|provider| provider.meta.as_ref())
        .and_then(|meta| meta.usage_script.as_ref())
}

pub(crate) use crate::proxy_core::api::ports::usage_script_credentials_from_parts as usage_script_credentials;

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
    let codex_config_has_proxy_placeholder = matches!(app_type, AppType::Codex)
        && codex_config_has_proxy_placeholder(config, placeholder);
    core_live_config_has_proxy_placeholder_for_app(
        &AppKind::from(app_type),
        config,
        placeholder,
        codex_config_has_proxy_placeholder,
    )
}

pub(crate) fn live_backup_snapshot_from_live_config(
    app_type: &AppType,
    config: &Value,
    placeholder: &str,
) -> Option<Value> {
    let codex_config_has_proxy_placeholder = matches!(app_type, AppType::Codex)
        && codex_config_has_proxy_placeholder(config, placeholder);
    core_live_backup_snapshot_from_live_config(
        &AppKind::from(app_type),
        config,
        placeholder,
        codex_config_has_proxy_placeholder,
    )
}

pub(crate) fn provider_settings_with_live_token_sync(
    app_type: &AppType,
    live_config: &Value,
    provider_settings: &Value,
    placeholder: &str,
) -> Result<Option<Value>, LiveTokenProviderSettingsIssue> {
    core_provider_settings_with_live_token_sync(
        &AppKind::from(app_type),
        live_config,
        provider_settings,
        placeholder,
    )
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

fn codex_config_has_proxy_placeholder(config: &Value, placeholder: &str) -> bool {
    config
        .get("config")
        .and_then(Value::as_str)
        .and_then(crate::codex_config::extract_codex_experimental_bearer_token)
        .as_deref()
        == Some(placeholder)
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

pub(crate) fn live_takeover_config_matches_proxy_for_app(
    app_type: &AppType,
    config: &Value,
    proxy_url: &str,
    codex_proxy_base_url: &str,
    placeholder: &str,
) -> bool {
    let codex_facts = if matches!(app_type, AppType::Codex) {
        CodexLiveTakeoverMatchFacts {
            config_has_proxy_placeholder: codex_config_has_proxy_placeholder(config, placeholder),
            config_base_url_matches_proxy: config
                .get("config")
                .and_then(Value::as_str)
                .is_some_and(|config_text| {
                    core_codex_config_has_base_url_matching(config_text, |url| {
                        proxy_urls_match(url, codex_proxy_base_url)
                    })
                }),
        }
    } else {
        CodexLiveTakeoverMatchFacts::default()
    };

    core_live_takeover_config_matches_proxy_for_app(
        &AppKind::from(app_type),
        config,
        proxy_url,
        placeholder,
        codex_facts,
    )
}

fn proxy_urls_match(actual: &str, expected: &str) -> bool {
    core_proxy_urls_match(actual, expected)
}

fn launch_env_vars_from_provider_settings(
    config: &Value,
    app_type: &AppType,
) -> Vec<(String, String)> {
    core_launch_env_vars_from_provider_settings(config, &AppKind::from(app_type))
}

pub(crate) fn provider_claude_models_are_claude_safe(provider: &Provider) -> bool {
    crate::proxy_core::api::auth::claude_desktop_provider_models_are_profile_safe(
        &provider.settings_config,
    )
}

pub(crate) fn provider_claude_desktop_suggested_proxy_routes(
    provider: &Provider,
) -> Option<std::collections::HashMap<String, crate::provider::ClaudeDesktopModelRoute>> {
    let routes = crate::proxy_core::api::auth::claude_desktop_suggested_proxy_routes(
        &provider.settings_config,
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.provider_type.as_deref()),
    );

    (!routes.is_empty()).then(|| {
        routes
            .into_iter()
            .map(|route| {
                (
                    route.route_id,
                    crate::provider::ClaudeDesktopModelRoute {
                        model: route.upstream_model,
                        label_override: route.label_override,
                        supports_1m: Some(route.supports_1m),
                    },
                )
            })
            .collect()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderImportDecision {
    Direct,
    Proxy(std::collections::HashMap<String, crate::provider::ClaudeDesktopModelRoute>),
    Skip,
}

pub(crate) fn provider_claude_desktop_import_decision(
    provider: &Provider,
) -> ClaudeDesktopProviderImportDecision {
    if provider_claude_desktop_direct_importable(provider) {
        return ClaudeDesktopProviderImportDecision::Direct;
    }

    provider_claude_desktop_suggested_proxy_routes(provider)
        .map(ClaudeDesktopProviderImportDecision::Proxy)
        .unwrap_or(ClaudeDesktopProviderImportDecision::Skip)
}

fn provider_claude_desktop_direct_importable(provider: &Provider) -> bool {
    if !provider_claude_models_are_claude_safe(provider) {
        return false;
    }

    provider_claude_desktop_direct_provider_validation(provider).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderDirectValidationIssue {
    Provider(ClaudeDesktopDirectProviderValidationIssue),
    ModelRoute(ClaudeDesktopDirectModelRouteIssue),
    Credentials(ClaudeDesktopDirectGatewayCredentialIssue),
}

pub(crate) fn provider_claude_desktop_direct_provider_validation(
    provider: &Provider,
) -> Result<(), ClaudeDesktopProviderDirectValidationIssue> {
    if let Some(issue) = provider_claude_desktop_direct_validation_issue(provider) {
        return Err(ClaudeDesktopProviderDirectValidationIssue::Provider(issue));
    }

    provider_claude_desktop_direct_inference_model_specs(provider)
        .map_err(ClaudeDesktopProviderDirectValidationIssue::ModelRoute)?;
    claude_desktop_direct_gateway_credentials(&provider.settings_config)
        .map_err(ClaudeDesktopProviderDirectValidationIssue::Credentials)?;

    Ok(())
}

pub(crate) fn provider_claude_desktop_direct_inference_model_specs(
    provider: &Provider,
) -> Result<Vec<ClaudeDesktopGatewayProfileModelSpec>, ClaudeDesktopDirectModelRouteIssue> {
    let route_inputs = provider
        .meta
        .as_ref()
        .into_iter()
        .flat_map(|meta| meta.claude_desktop_model_routes.iter())
        .map(|(route_id, route)| ClaudeDesktopProxyRouteInput {
            route_id,
            upstream_model: &route.model,
            label_override: route.label_override.as_deref(),
            supports_1m: route.supports_1m.unwrap_or(false),
        });

    claude_desktop_direct_inference_model_specs(route_inputs).map(|specs| {
        specs
            .into_iter()
            .map(ClaudeDesktopGatewayProfileModelSpec::from)
            .collect()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderDirectGatewayProfileIssue {
    Credentials(ClaudeDesktopDirectGatewayCredentialIssue),
    ModelRoute(ClaudeDesktopDirectModelRouteIssue),
}

pub(crate) fn provider_claude_desktop_direct_gateway_profile(
    provider: &Provider,
) -> Result<Value, ClaudeDesktopProviderDirectGatewayProfileIssue> {
    let credentials = claude_desktop_direct_gateway_credentials(&provider.settings_config)
        .map_err(ClaudeDesktopProviderDirectGatewayProfileIssue::Credentials)?;
    let model_specs = provider_claude_desktop_direct_inference_model_specs(provider)
        .map_err(ClaudeDesktopProviderDirectGatewayProfileIssue::ModelRoute)?;

    Ok(claude_desktop_gateway_profile(
        &credentials.base_url,
        &credentials.api_key,
        (!model_specs.is_empty()).then_some(model_specs.as_slice()),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderProxyRouteIssue {
    Missing,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderProxyValidationIssue {
    Config(ClaudeDesktopProxyProviderConfigValidationIssue),
    ModelRoutes(ClaudeDesktopProviderProxyRouteIssue),
    CredentialsMissing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderValidationIssue {
    Direct(ClaudeDesktopProviderDirectValidationIssue),
    Proxy(ClaudeDesktopProviderProxyValidationIssue),
}

pub(crate) fn provider_claude_desktop_proxy_model_routes(
    provider: &Provider,
) -> Result<Vec<ClaudeDesktopResolvedProxyRoute>, ClaudeDesktopProviderProxyRouteIssue> {
    let routes = provider
        .meta
        .as_ref()
        .map(|meta| &meta.claude_desktop_model_routes)
        .ok_or(ClaudeDesktopProviderProxyRouteIssue::Missing)?;

    let result = claude_desktop_proxy_model_routes(routes.iter().map(|(route_id, route)| {
        ClaudeDesktopProxyRouteInput {
            route_id,
            upstream_model: &route.model,
            label_override: route.label_override.as_deref(),
            supports_1m: route.supports_1m.unwrap_or(false),
        }
    }));

    if result.is_empty() {
        return Err(ClaudeDesktopProviderProxyRouteIssue::Empty);
    }

    Ok(result)
}

pub(crate) fn provider_claude_desktop_proxy_provider_validation(
    provider: &Provider,
) -> Result<(), ClaudeDesktopProviderProxyValidationIssue> {
    if let Some(issue) = provider_claude_desktop_proxy_config_validation_issue(provider) {
        return Err(ClaudeDesktopProviderProxyValidationIssue::Config(issue));
    }

    provider_claude_desktop_proxy_model_routes(provider)
        .map_err(ClaudeDesktopProviderProxyValidationIssue::ModelRoutes)?;

    if !provider_claude_desktop_proxy_has_base_url_and_key(provider) {
        return Err(ClaudeDesktopProviderProxyValidationIssue::CredentialsMissing);
    }

    Ok(())
}

pub(crate) fn provider_claude_desktop_provider_validation(
    provider: &Provider,
) -> Result<(), ClaudeDesktopProviderValidationIssue> {
    match provider_claude_desktop_mode(provider) {
        crate::provider::ClaudeDesktopMode::Direct => {
            provider_claude_desktop_direct_provider_validation(provider)
                .map_err(ClaudeDesktopProviderValidationIssue::Direct)
        }
        crate::provider::ClaudeDesktopMode::Proxy => {
            provider_claude_desktop_proxy_provider_validation(provider)
                .map_err(ClaudeDesktopProviderValidationIssue::Proxy)
        }
    }
}

pub(crate) fn provider_claude_desktop_proxy_gateway_profile_model_specs(
    provider: &Provider,
) -> Result<Vec<ClaudeDesktopGatewayProfileModelSpec>, ClaudeDesktopProviderProxyRouteIssue> {
    provider_claude_desktop_proxy_model_routes(provider).map(|routes| {
        routes
            .into_iter()
            .map(|route| ClaudeDesktopGatewayProfileModelSpec {
                name: route.route_id,
                label_override: route.label_override,
                supports_1m: route.supports_1m,
            })
            .collect()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderProxyRequestBodyIssue {
    Routes(ClaudeDesktopProviderProxyRouteIssue),
    Body(ClaudeDesktopProxyRequestBodyIssue),
}

pub(crate) fn provider_claude_desktop_proxy_request_body(
    body: Value,
    provider: &Provider,
) -> Result<Value, ClaudeDesktopProviderProxyRequestBodyIssue> {
    let routes = provider_claude_desktop_proxy_model_routes(provider)
        .map_err(ClaudeDesktopProviderProxyRequestBodyIssue::Routes)?;
    let raw_routes = provider
        .meta
        .as_ref()
        .into_iter()
        .flat_map(|meta| meta.claude_desktop_model_routes.iter())
        .map(|(route_id, route)| ClaudeDesktopProxyRouteInput {
            route_id,
            upstream_model: &route.model,
            label_override: route.label_override.as_deref(),
            supports_1m: route.supports_1m.unwrap_or(false),
        });
    let api_format = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.api_format.as_deref());

    claude_desktop_proxy_request_body_with_upstream_model(
        body,
        &provider.settings_config,
        api_format,
        &routes,
        raw_routes,
    )
    .map_err(ClaudeDesktopProviderProxyRequestBodyIssue::Body)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClaudeDesktopProviderStatusFacts {
    pub mode: crate::provider::ClaudeDesktopMode,
    pub expected_base_url: Option<String>,
    pub missing_route_mappings: bool,
}

pub(crate) fn provider_claude_desktop_status_facts(
    provider: &Provider,
    proxy_gateway_base_url: impl FnOnce() -> Option<String>,
) -> ClaudeDesktopProviderStatusFacts {
    let mode = provider_claude_desktop_mode(provider);
    let expected_base_url = match mode {
        crate::provider::ClaudeDesktopMode::Proxy => proxy_gateway_base_url(),
        crate::provider::ClaudeDesktopMode::Direct => {
            claude_desktop_direct_gateway_credentials(&provider.settings_config)
                .ok()
                .map(|credentials| credentials.base_url)
        }
    };
    let missing_route_mappings = matches!(mode, crate::provider::ClaudeDesktopMode::Proxy)
        && provider_claude_desktop_proxy_routes_missing(provider);

    ClaudeDesktopProviderStatusFacts {
        mode,
        expected_base_url,
        missing_route_mappings,
    }
}

pub(crate) fn provider_claude_desktop_mode(
    provider: &Provider,
) -> crate::provider::ClaudeDesktopMode {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.claude_desktop_mode.clone())
        .unwrap_or(crate::provider::ClaudeDesktopMode::Direct)
}

fn provider_claude_desktop_proxy_routes_missing(provider: &Provider) -> bool {
    provider_claude_desktop_proxy_model_routes(provider).is_err()
}

pub(crate) fn provider_claude_desktop_proxy_has_base_url_and_key(provider: &Provider) -> bool {
    crate::proxy_core::api::auth::claude_desktop_proxy_has_base_url_and_key(
        claude_desktop_provider_validation_input(provider),
    )
}

pub(crate) fn provider_claude_desktop_direct_validation_issue(
    provider: &Provider,
) -> Option<ClaudeDesktopDirectProviderValidationIssue> {
    crate::proxy_core::api::auth::claude_desktop_direct_provider_validation_issue(
        claude_desktop_provider_validation_input(provider),
    )
}

pub(crate) fn provider_claude_desktop_proxy_config_validation_issue(
    provider: &Provider,
) -> Option<ClaudeDesktopProxyProviderConfigValidationIssue> {
    crate::proxy_core::api::auth::claude_desktop_proxy_provider_config_validation_issue(
        claude_desktop_provider_validation_input(provider),
    )
}

fn claude_desktop_provider_validation_input(
    provider: &Provider,
) -> crate::proxy_core::api::auth::ClaudeDesktopProviderValidationInput<'_> {
    let meta = provider.meta.as_ref();
    crate::proxy_core::api::auth::ClaudeDesktopProviderValidationInput {
        settings_config: &provider.settings_config,
        api_format: meta.and_then(|meta| meta.api_format.as_deref()),
        claude_desktop_mode_is_proxy: meta.is_some_and(|meta| {
            matches!(
                meta.claude_desktop_mode.as_ref(),
                Some(crate::provider::ClaudeDesktopMode::Proxy)
            )
        }),
        provider_type: meta.and_then(|meta| meta.provider_type.as_deref()),
        is_full_url: meta.and_then(|meta| meta.is_full_url).unwrap_or(false),
    }
}

#[cfg(test)]
pub(crate) fn provider_should_normalize_mimo_anthropic_thinking_history(
    provider: &Provider,
    upstream_model: &str,
) -> bool {
    crate::proxy_core::api::transforms::should_normalize_mimo_anthropic_thinking_history(
        crate::proxy_core::api::transforms::MimoAnthropicThinkingNormalizationInput {
            settings_config: &provider.settings_config,
            api_format: provider
                .meta
                .as_ref()
                .and_then(|meta| meta.api_format.as_deref()),
            upstream_model,
        },
    )
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

pub(crate) fn provider_stream_check_config_override(
    provider: &Provider,
) -> Option<StreamCheckConfigOverride> {
    let config = provider_stream_check_test_config(provider)?;
    Some(StreamCheckConfigOverride {
        timeout_secs: config.timeout_secs,
        max_retries: config.max_retries,
        degraded_threshold_ms: config.degraded_threshold_ms,
    })
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
    let raw = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.custom_user_agent.as_deref());
    core_provider_custom_user_agent_header(raw, is_copilot)
        .ok()
        .flatten()
}

pub(crate) fn model_fetch_custom_user_agent_header(raw: Option<&str>) -> Option<http::HeaderValue> {
    parse_custom_user_agent(raw).ok().flatten()
}

pub(crate) fn provider_bedrock_env_flag(provider: &Provider) -> Option<&str> {
    bedrock_env_flag_from_provider_settings(&provider.settings_config)
}

pub(crate) use crate::proxy_core::api::usage::{
    is_placeholder_pricing_model, usage_route_context_from_selection,
};

pub(crate) fn usage_logging_enabled_from_proxy_config(config: &RwLock<ProxyConfig>) -> bool {
    usage_logging_enabled_from_config_flag(
        config.try_read().ok().map(|config| config.enable_logging),
    )
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

#[allow(unused_imports)]
pub(crate) use crate::proxy::host::cc_switch::database_usage_sink::CcSwitchUsageSink;

#[cfg(test)]
pub(crate) use crate::proxy_core::api::model_catalog::{
    claude_takeover_client_model_for_upstream, claude_takeover_default_display_name,
};
#[cfg(test)]
pub(crate) use crate::proxy_core::api::ports::apply_claude_takeover_fields_with_policy_and_models;
#[cfg(test)]
use crate::proxy_core::api::ports::claude_takeover_model_fields_from_settings as core_claude_takeover_model_fields_from_settings;
use crate::proxy_core::api::ports::{
    apply_claude_takeover_fields_for_provider_facts as core_apply_claude_takeover_fields_for_provider_facts,
    ClaudeTakeoverProviderFacts,
};
pub(crate) use crate::proxy_core::api::ports::{
    apply_claude_takeover_fields_with_policy, ClaudeTakeoverAuthPolicy,
};

#[cfg(test)]
pub(crate) fn provider_claude_takeover_model_fields(
    provider: &Provider,
) -> Vec<(&'static str, String)> {
    core_claude_takeover_model_fields_from_settings(&provider.settings_config)
}

pub(crate) fn apply_claude_takeover_fields_for_provider(
    config: &mut Value,
    proxy_url: &str,
    placeholder: &str,
    provider: &Provider,
) {
    core_apply_claude_takeover_fields_for_provider_facts(
        config,
        proxy_url,
        placeholder,
        ClaudeTakeoverProviderFacts {
            provider_settings_config: &provider.settings_config,
            uses_managed_account: provider_uses_managed_account_auth(provider),
            is_github_copilot: provider_is_github_copilot(provider),
        },
    );
}

#[cfg(test)]
use crate::proxy::host::cc_switch::database_channel_source::proxy_channel_record_to_core;

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
        custom_endpoint_count: meta.map(|meta| meta.custom_endpoints.len()).unwrap_or(0),
    })
}

fn account_ref(provider: &Provider) -> Option<String> {
    provider.meta.as_ref().and_then(|meta| {
        let provider_type = meta.provider_type.as_deref();
        let account_id = provider_type
            .and_then(|provider_type| provider_managed_account_id_for(provider, provider_type));
        provider_account_ref(provider_type, account_id.as_deref())
    })
}

#[cfg(test)]
mod tests {
    use crate::proxy_core::api::ports::{
        json_deep_merge, json_deep_remove, json_remove_array_items, json_value_is_subset,
        normalize_claude_models_in_value,
        provider_supports_legacy_common_config_migration as core_provider_supports_legacy_common_config_migration,
    };

    use super::*;
    use crate::database::ProxyChannelSourceKind;
    use crate::provider::{
        AuthBinding, AuthBindingSource, ClaudeDesktopMode, ClaudeDesktopModelRoute, ProviderMeta,
    };
    use crate::proxy::provider::ProviderAdapter;
    use crate::proxy_core::api::auth::channel_auth_profile_missing_key_error;
    use crate::proxy_core::api::auth::{
        ManagedAccountAuthRuntime, ManagedAccountRuntimeSource as CoreManagedAccountRuntimeSource,
    };
    use crate::proxy_core::api::domain::{
        channel_auth_profile_action, channel_auth_profile_missing_provider_warning,
        ChannelAuthProfileAction,
    };
    use crate::proxy_core::api::errors::{
        proxy_error_http_status_code, proxy_error_response_body,
        upstream_proxy_error_response_body, ProxyCoreError, ProxyErrorStatusKind,
    };
    use crate::proxy_core::api::events::ProxyEventEnvelope;
    use crate::proxy_core::api::management::ChannelKeyRuntimeCandidate;
    use crate::proxy_core::api::model_catalog::CopilotModel;
    use crate::proxy_core::api::session::SessionIdSource;
    use crate::proxy_core::api::transforms::GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX;
    use crate::proxy_core::api::transport::{
        anthropic_beta_header_value, build_upstream_request_headers, forward_upstream_url_plan,
        is_socks_proxy_url, resolve_upstream_request_transport_policy,
        resolve_upstream_send_policy, serialize_upstream_request_body, ForwardUpstreamUrlPlanInput,
        ProxyTransportResponseBody, UpstreamRequestHeadersInput, UpstreamSendPolicyInput,
        UpstreamSseAggregationKind, UpstreamTransportKind, UNSUPPORTED_IMAGE_MARKER,
    };
    use crate::proxy_core::api::usage::{
        usage_selected_provider_missing_log_message, TransformedResponseUsageFormat,
        UsageRecordFailureLogContext, UsageSelectedProviderMissingPhase,
    };

    #[tokio::test]
    async fn non_managed_auth_passes_through_without_app_handle() {
        let auth = ProviderAuthInfo::new("sk-test".to_string(), ProviderAuthStrategy::Bearer);
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            serde_json::json!({}),
            None,
        );

        let runtime_source = default_managed_account_runtime_source();
        let resolved = resolve_managed_account_auth_from_runtime_source(
            runtime_source.as_ref(),
            &provider,
            auth.clone(),
        )
        .await
        .expect("non managed auth");

        assert_eq!(resolved.auth, auth);
        assert_eq!(resolved.codex_oauth_account_id, None);
        assert!(!resolved.should_send_codex_oauth_session_headers);
    }

    #[tokio::test]
    async fn managed_auth_requires_app_handle() {
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            serde_json::json!({}),
            None,
        );

        let runtime_source = default_managed_account_runtime_source();
        let copilot = resolve_managed_account_auth_from_runtime_source(
            runtime_source.as_ref(),
            &provider,
            ProviderAuthInfo::new(
                "PROXY_MANAGED".to_string(),
                ProviderAuthStrategy::GitHubCopilot,
            ),
        )
        .await
        .expect_err("copilot app handle error");
        assert!(matches!(
            copilot,
            ProxyError::AuthError(message)
                if message == "GitHub Copilot 认证不可用（无 AppHandle）"
        ));

        let codex = resolve_managed_account_auth_from_runtime_source(
            runtime_source.as_ref(),
            &provider,
            ProviderAuthInfo::new(
                "PROXY_MANAGED".to_string(),
                ProviderAuthStrategy::CodexOAuth,
            ),
        )
        .await
        .expect_err("codex app handle error");
        assert!(matches!(
            codex,
            ProxyError::AuthError(message)
                if message == "Codex OAuth 认证不可用（无 AppHandle）"
        ));
    }

    #[tokio::test]
    async fn copilot_runtime_helpers_skip_without_app_handle() {
        assert_eq!(copilot_api_endpoint_from_app_handle(None, None).await, None);
        assert_eq!(
            copilot_live_models_from_app_handle(None, None)
                .await
                .expect("skip"),
            None
        );
        assert_eq!(
            copilot_model_vendor_from_app_handle(None, None, "gpt-5").await,
            None
        );
    }

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
            crate::proxy_core::api::domain::unsupported_app_kind_error_message(
                "invalid app: openclaw"
            ),
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
            bridged_request
                .headers
                .get("x-test")
                .and_then(|value| value.to_str().ok()),
            Some("1")
        );
        assert_eq!(
            bridged_request
                .extensions
                .get::<String>()
                .map(String::as_str),
            Some("extension-value")
        );
        assert_eq!(
            bridged_request.body,
            ProxyBody::Json(json!({"model": "gpt-5"}))
        );

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
        assert_eq!(
            forward_request.session_result.source,
            SessionIdSource::Generated
        );
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
        assert_eq!(
            output.record.response_model.as_deref(),
            Some("response-model")
        );
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
            channel_auth_profile_missing_key_error("channel-a", "primary"),
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

        struct TestChannelKeyRuntimeSource {
            expected: Option<(&'static str, &'static str)>,
            candidate: Option<ChannelKeyRuntimeCandidate>,
        }

        impl ChannelKeyRuntimeSource for TestChannelKeyRuntimeSource {
            fn load_channel_key_candidate(
                &self,
                channel_id: &str,
                key_ref: &str,
            ) -> ProxyCoreResult<Option<ChannelKeyRuntimeCandidate>> {
                let Some((expected_channel_id, expected_key_ref)) = self.expected else {
                    panic!("provider auth should not load channel keys");
                };
                assert_eq!(channel_id, expected_channel_id);
                assert_eq!(key_ref, expected_key_ref);
                Ok(self.candidate.clone())
            }
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
        let provider_runtime_source = TestChannelKeyRuntimeSource {
            expected: None,
            candidate: None,
        };
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut provider_attempt),
            &provider_runtime_source,
        )
        .expect("apply provider auth profile");
        assert_eq!(provider_attempt.auth_provider().id, "provider-auth");

        let mut channel_key_attempt =
            attempt_with_auth_ref(&route_provider, "channel-key", "channel-key:primary");
        let channel_key_runtime_source = TestChannelKeyRuntimeSource {
            expected: Some(("channel-key", "primary")),
            candidate: Some(ChannelKeyRuntimeCandidate {
                channel_id: "channel-key".to_string(),
                key_ref: "primary".to_string(),
                key_value: "loaded-channel-key".to_string(),
                status: "enabled".to_string(),
                priority: 10,
                weight: 100,
                last_failure_at: Some(1_771_000_003),
            }),
        };
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut channel_key_attempt),
            &channel_key_runtime_source,
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
            crate::proxy_core::api::errors::error_message_with_context(
                "load config",
                "disk failed"
            ),
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
        assert!(validate_explicit_proxy_url("http://127.0.0.1:7890").is_ok());
        assert!(validate_explicit_proxy_url("socks5h://localhost:1080").is_ok());
        let invalid_scheme =
            validate_explicit_proxy_url("ftp://127.0.0.1:7890").expect_err("invalid scheme");
        assert!(invalid_scheme.contains(
            "Invalid proxy scheme 'ftp' in URL 'ftp://127.0.0.1:7890'. Supported: http, https, socks5, socks5h"
        ));
        let invalid_url = validate_explicit_proxy_url("http://[::1")
            .expect_err("invalid proxy URL should report parse error");
        assert!(invalid_url.contains("Invalid proxy URL 'http://[::1':"));
        assert_eq!(
            invalid_explicit_proxy_url_message("http://user:pass@127.0.0.1:7890", "bad"),
            "Invalid proxy URL 'http://127.0.0.1:7890': bad"
        );
    }

    #[test]
    fn provider_endpoint_adapter_projects_list_normalization_and_last_used() {
        let mut endpoints = HashMap::new();
        endpoints.insert(
            "https://old.example".to_string(),
            CustomEndpoint {
                url: "https://old.example".to_string(),
                added_at: 10,
                last_used: None,
            },
        );
        endpoints.insert(
            "https://new.example".to_string(),
            CustomEndpoint {
                url: "https://new.example".to_string(),
                added_at: 20,
                last_used: Some(1),
            },
        );
        let mut provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            custom_endpoints: endpoints,
            ..ProviderMeta::default()
        });

        let listed = provider_custom_endpoint_list(Some(&provider));
        assert_eq!(
            listed
                .iter()
                .map(|endpoint| endpoint.url.as_str())
                .collect::<Vec<_>>(),
            vec!["https://new.example", "https://old.example"]
        );
        assert!(provider_custom_endpoint_list(None).is_empty());
        assert!(provider_custom_endpoint_list(Some(&Provider::with_id(
            "provider-empty".to_string(),
            "Provider Empty".to_string(),
            json!({}),
            None,
        )))
        .is_empty());

        assert_eq!(
            custom_endpoint_url_key(" https://relay.example.com/v1/// "),
            "https://relay.example.com/v1"
        );
        assert_eq!(
            normalize_custom_endpoint_url(" https://relay.example.com/v1/ ")
                .expect("normalized endpoint URL"),
            "https://relay.example.com/v1"
        );
        let empty_error = normalize_custom_endpoint_url(" / ").expect_err("empty URL");
        assert!(matches!(
            empty_error,
            AppError::Localized {
                key: "provider.endpoint.url_required",
                ..
            }
        ));

        assert!(mark_custom_endpoint_last_used(
            &mut provider,
            "https://old.example",
            1234
        ));
        let old_last_used = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.custom_endpoints.get("https://old.example"))
            .and_then(|endpoint| endpoint.last_used);
        assert_eq!(old_last_used, Some(1234));
        assert!(!mark_custom_endpoint_last_used(
            &mut provider,
            "https://missing.example",
            5678
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
        assert_eq!(CopilotOptimizerConfig::default().warmup_model, "gpt-5-mini");
    }

    #[test]
    fn forwarder_request_source_selects_adapter_for_app() {
        let source = default_forwarder_request_source();

        let claude_adapter = source.adapter_context_for_app(&AppType::Claude);
        let fallback_adapter = source.adapter_context_for_app(&AppType::Hermes);

        assert_eq!(claude_adapter.facts().adapter_name, "Claude");
        assert_eq!(fallback_adapter.facts().adapter_name, "Codex");
    }

    #[tokio::test]
    async fn forwarder_protocol_state_source_skips_codex_chat_enrichment_when_disabled() {
        let source = CcSwitchForwarderProtocolStateSource::new(
            Arc::new(GeminiShadowStore::default()),
            Arc::new(CodexChatHistoryStore::default()),
        );
        let mut body = json!({
            "model": "gpt-5",
            "input": [{
                "type": "function_call_output",
                "call_id": "call-1",
                "output": "{}"
            }]
        });
        let original = body.clone();

        source
            .enrich_codex_chat_request(ForwarderCodexChatProtocolEnrichmentInput {
                body: &mut body,
                enabled: false,
            })
            .await;

        assert_eq!(body, original);
    }

    #[test]
    fn forwarder_request_source_prepares_bedrock_attempt_body() {
        let source = default_forwarder_request_source();
        let provider = Provider::with_id(
            "bedrock-provider".to_string(),
            "Bedrock Provider".to_string(),
            json!({
                "env": {
                    "CLAUDE_CODE_USE_BEDROCK": "1"
                }
            }),
            None,
        );
        let body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "tools": [{"name": "tool1"}],
            "system": [{"type": "text", "text": "sys prompt"}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "hello"}
                ]}
            ]
        });
        let config = OptimizerConfig {
            enabled: true,
            thinking_optimizer: true,
            cache_injection: true,
            cache_ttl: "1h".to_string(),
        };

        let prepared = source.prepare_attempt_body(ForwarderAttemptBodyInput {
            body: &body,
            provider: &provider,
            config: &config,
        });

        assert_eq!(body.get("thinking"), None);
        assert_eq!(prepared["thinking"]["type"], "adaptive");
        assert_eq!(prepared["output_config"]["effort"], "max");
        assert!(prepared["tools"][0].get("cache_control").is_some());
        assert!(prepared["system"][0].get("cache_control").is_some());
        assert!(prepared["messages"][1]["content"][0]
            .get("cache_control")
            .is_some());
    }

    #[test]
    fn forwarder_adapter_context_projects_provider_url_facts() {
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let mut provider = Provider::with_id(
            "copilot-provider".to_string(),
            "Copilot Provider".to_string(),
            json!({
                "base_url": "https://api.githubcopilot.com"
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            is_full_url: Some(true),
            ..Default::default()
        });

        let facts = adapter
            .provider_url_facts(&provider)
            .expect("provider URL facts");

        assert_eq!(facts.base_url, "https://api.githubcopilot.com");
        assert!(facts.is_full_url);
        assert!(facts.is_copilot);
    }

    #[test]
    fn forwarder_adapter_context_projects_adapter_facts() {
        let claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let codex_adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);

        let claude_facts = claude_adapter.facts();
        let codex_facts = codex_adapter.facts();

        assert_eq!(claude_facts.adapter_name, "Claude");
        assert!(claude_facts.is_claude_adapter);
        assert_eq!(codex_facts.adapter_name, "Codex");
        assert!(!codex_facts.is_claude_adapter);
    }

    #[test]
    fn forwarder_request_source_converts_codex_responses_to_chat_body() {
        let source = CcSwitchForwarderRequestSource::new(default_managed_account_runtime_source());
        let provider = Provider::with_id(
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

        let body =
            source.convert_codex_responses_to_chat_body(ForwarderCodexResponsesToChatInput {
                body: json!({
                    "model": "client-model",
                    "instructions": "Stay concise.",
                    "input": "Hello",
                    "max_output_tokens": 64,
                    "stream": true
                }),
                provider: &provider,
            });

        assert_eq!(body["model"], "upstream-model");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "Hello");
        assert_eq!(body["max_tokens"], 64);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn forwarder_request_source_projects_codex_responses_to_chat_gate() {
        let source = default_forwarder_request_source();
        let codex_adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = Provider::with_id(
            "codex-chat".to_string(),
            "Codex Chat".to_string(),
            json!({
                "config": r#"model_provider = "openai"
model = " upstream-model "

[model_providers.openai]
wire_api = "chat"
base_url = "https://api.openai.com/v1"
"#,
            }),
            None,
        );

        assert!(
            source
                .transform_plan(ForwarderTransformPlanInput {
                    app_type: &AppType::Codex,
                    adapter: &codex_adapter,
                    endpoint: "/responses",
                    provider: &provider,
                    resolved_claude_api_format: None,
                })
                .codex_responses_to_chat
        );
        assert!(
            !source
                .transform_plan(ForwarderTransformPlanInput {
                    app_type: &AppType::Claude,
                    adapter: &claude_adapter,
                    endpoint: "/responses",
                    provider: &provider,
                    resolved_claude_api_format: None,
                })
                .codex_responses_to_chat
        );
        assert!(
            !source
                .transform_plan(ForwarderTransformPlanInput {
                    app_type: &AppType::Codex,
                    adapter: &codex_adapter,
                    endpoint: "/chat/completions",
                    provider: &provider,
                    resolved_claude_api_format: None,
                })
                .codex_responses_to_chat
        );
    }

    #[test]
    fn forwarder_request_source_wraps_provider_transform_request() {
        let source = CcSwitchForwarderRequestSource::new(default_managed_account_runtime_source());
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let provider = Provider::with_id(
            "codex-provider".to_string(),
            "Codex Provider".to_string(),
            json!({}),
            None,
        );
        let body = json!({"model": "gpt-5", "messages": []});

        let transformed = source
            .transform_provider_request_body(ForwarderProviderTransformInput {
                adapter: &adapter,
                body: body.clone(),
                provider: &provider,
            })
            .expect("provider transform");

        assert_eq!(transformed, body);
    }

    #[test]
    fn forwarder_request_source_transforms_request_body_and_tracks_outbound_model() {
        let source = default_forwarder_request_source();
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        let no_transform_plan = ForwarderTransformPlan {
            needs_transform: false,
            use_claude_transform: false,
            use_provider_transform: false,
            claude_api_format_for_url: None,
            claude_api_format_for_transform: None,
            codex_responses_to_chat: false,
        };

        let passthrough = source
            .transform_request_body(ForwarderRequestBodyTransformInput {
                adapter: &adapter,
                body: json!({"model": "mapped-model", "messages": []}),
                provider: &provider,
                transform_plan: &no_transform_plan,
                claude_transformed_body: None,
            })
            .expect("passthrough request body");
        assert_eq!(passthrough.body["model"], "mapped-model");
        assert_eq!(passthrough.outbound_model.as_deref(), Some("mapped-model"));

        let claude_transform_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            use_provider_transform: false,
            claude_api_format_for_url: Some("openai_chat".to_string()),
            claude_api_format_for_transform: Some("openai_chat".to_string()),
            codex_responses_to_chat: false,
        };
        let claude_transformed = source
            .transform_request_body(ForwarderRequestBodyTransformInput {
                adapter: &adapter,
                body: json!({"model": "mapped-model", "messages": []}),
                provider: &provider,
                transform_plan: &claude_transform_plan,
                claude_transformed_body: Some(json!({
                    "model": "chat-model",
                    "messages": []
                })),
            })
            .expect("Claude transformed request body");
        assert_eq!(claude_transformed.body["model"], "chat-model");
        assert_eq!(
            claude_transformed.outbound_model.as_deref(),
            Some("mapped-model")
        );
    }

    #[test]
    fn forwarder_request_source_prefers_codex_chat_bridge_over_claude_body() {
        let source = default_forwarder_request_source();
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = Provider::with_id(
            "codex-chat".to_string(),
            "Codex Chat".to_string(),
            json!({
                "config": r#"model_provider = "openai"
model = " upstream-model "

[model_providers.openai]
wire_api = "chat"
base_url = "https://api.openai.com/v1"
"#,
            }),
            None,
        );
        let transform_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            use_provider_transform: false,
            claude_api_format_for_url: Some("openai_chat".to_string()),
            claude_api_format_for_transform: Some("openai_chat".to_string()),
            codex_responses_to_chat: true,
        };

        let transformed = source
            .transform_request_body(ForwarderRequestBodyTransformInput {
                adapter: &adapter,
                body: json!({
                    "model": "client-model",
                    "instructions": "Stay concise.",
                    "input": "Hello"
                }),
                provider: &provider,
                transform_plan: &transform_plan,
                claude_transformed_body: Some(json!({"model": "should-not-win"})),
            })
            .expect("Codex chat bridge body");

        assert_eq!(transformed.body["model"], "upstream-model");
        assert_eq!(transformed.body["messages"][0]["role"], "system");
        assert_eq!(transformed.body["messages"][1]["content"], "Hello");
        assert_eq!(transformed.outbound_model.as_deref(), Some("client-model"));
    }

    #[test]
    fn forwarder_request_source_projects_transform_plan() {
        let source = default_forwarder_request_source();
        let claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let codex_adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let mut claude_provider = Provider::with_id(
            "claude-provider".to_string(),
            "Claude Provider".to_string(),
            json!({}),
            None,
        );
        claude_provider.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });

        let resolved_plan = source.transform_plan(ForwarderTransformPlanInput {
            app_type: &AppType::Claude,
            adapter: &claude_adapter,
            endpoint: "/v1/messages",
            provider: &claude_provider,
            resolved_claude_api_format: Some("gemini_native"),
        });
        assert!(resolved_plan.needs_transform);
        assert!(resolved_plan.use_claude_transform);
        assert!(!resolved_plan.use_provider_transform);
        assert_eq!(
            resolved_plan.claude_api_format_for_url.as_deref(),
            Some("gemini_native")
        );
        assert_eq!(
            resolved_plan.claude_api_format_for_transform.as_deref(),
            Some("gemini_native")
        );
        assert!(!resolved_plan.codex_responses_to_chat);

        let fallback_plan = source.transform_plan(ForwarderTransformPlanInput {
            app_type: &AppType::Claude,
            adapter: &claude_adapter,
            endpoint: "/v1/messages",
            provider: &claude_provider,
            resolved_claude_api_format: None,
        });
        assert!(fallback_plan.needs_transform);
        assert!(fallback_plan.use_claude_transform);
        assert!(!fallback_plan.use_provider_transform);
        assert_eq!(
            fallback_plan.claude_api_format_for_url.as_deref(),
            Some("openai_chat")
        );
        assert_eq!(
            fallback_plan.claude_api_format_for_transform.as_deref(),
            Some("openai_chat")
        );
        assert!(!fallback_plan.codex_responses_to_chat);

        let codex_plan = source.transform_plan(ForwarderTransformPlanInput {
            app_type: &AppType::Codex,
            adapter: &codex_adapter,
            endpoint: "/v1/chat/completions",
            provider: &claude_provider,
            resolved_claude_api_format: None,
        });
        assert!(!codex_plan.needs_transform);
        assert!(!codex_plan.use_claude_transform);
        assert!(!codex_plan.use_provider_transform);
        assert!(codex_plan.claude_api_format_for_url.is_none());
        assert!(codex_plan.claude_api_format_for_transform.is_none());
        assert!(!codex_plan.codex_responses_to_chat);
    }

    #[test]
    fn forwarder_request_source_projects_protocol_preparation() {
        let source = default_forwarder_request_source();
        let claude_transform_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            use_provider_transform: false,
            claude_api_format_for_url: Some("openai_chat".to_string()),
            claude_api_format_for_transform: Some("openai_chat".to_string()),
            codex_responses_to_chat: false,
        };
        let claude_preparation = source.protocol_preparation(ForwarderProtocolPreparationInput {
            transform_plan: &claude_transform_plan,
        });
        assert!(claude_preparation.should_transform_claude_request);
        assert_eq!(
            claude_preparation
                .claude_api_format_for_transform
                .as_deref(),
            Some("openai_chat")
        );
        assert!(!claude_preparation.codex_chat_enrichment_enabled);

        let codex_bridge_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            use_provider_transform: false,
            claude_api_format_for_url: Some("openai_chat".to_string()),
            claude_api_format_for_transform: Some("openai_chat".to_string()),
            codex_responses_to_chat: true,
        };
        let codex_preparation = source.protocol_preparation(ForwarderProtocolPreparationInput {
            transform_plan: &codex_bridge_plan,
        });
        assert!(!codex_preparation.should_transform_claude_request);
        assert!(codex_preparation.claude_api_format_for_transform.is_none());
        assert!(codex_preparation.codex_chat_enrichment_enabled);
    }

    #[test]
    fn forwarder_request_source_plans_codex_upstream_url() {
        let source = default_forwarder_request_source();
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let body = json!({});
        let param_overrides = json!({"api-version": "2026-06-21"});
        let transform_plan = ForwarderTransformPlan {
            needs_transform: false,
            use_claude_transform: false,
            use_provider_transform: false,
            claude_api_format_for_url: None,
            claude_api_format_for_transform: None,
            codex_responses_to_chat: true,
        };

        let plan = source.plan_upstream_url(ForwarderUpstreamUrlInput {
            adapter: &adapter,
            base_url: "https://api.openai.com/v1/chat/completions",
            endpoint: "/v1/responses?foo=bar&api-version=old",
            is_full_url: false,
            transform_plan: &transform_plan,
            is_copilot: false,
            body: &body,
            channel_param_overrides: Some(&param_overrides),
        });

        assert_eq!(
            plan.effective_endpoint,
            "/chat/completions?foo=bar&api-version=old"
        );
        assert_eq!(
            plan.passthrough_query.as_deref(),
            Some("foo=bar&api-version=old")
        );
        assert_eq!(
            plan.url,
            "https://api.openai.com/v1/chat/completions?foo=bar&api-version=2026-06-21"
        );
    }

    #[test]
    fn forwarder_request_source_wraps_copilot_optimizer_sequence() {
        let source = CcSwitchForwarderRequestSource::new(default_managed_account_runtime_source());
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            "tools-2024-04-04".parse().expect("header value"),
        );

        let optimized = source.optimize_copilot_request(ForwarderCopilotRequestOptimizationInput {
            body: json!({
                "model": "claude-sonnet-4",
                "messages": [{"role": "user", "content": "Hello"}]
            }),
            headers: &headers,
            config: &CopilotOptimizerConfig::default(),
        });

        assert_eq!(optimized.classification.initiator, "user");
        assert!(optimized.classification.is_warmup);
        assert_eq!(optimized.body["model"], "gpt-5-mini");
    }

    #[test]
    fn forwarder_request_source_gates_copilot_optimizer_sequence() {
        let source = default_forwarder_request_source();
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            "tools-2024-04-04".parse().expect("header value"),
        );
        let disabled_config = CopilotOptimizerConfig {
            enabled: false,
            ..Default::default()
        };

        let disabled = source.prepare_copilot_request_optimization(
            ForwarderCopilotRequestOptimizationGateInput {
                body: json!({ "model": "claude-sonnet-4" }),
                headers: &headers,
                config: &disabled_config,
                is_copilot: true,
            },
        );
        assert!(disabled.classification.is_none());
        assert_eq!(disabled.body["model"], "claude-sonnet-4");

        let non_copilot = source.prepare_copilot_request_optimization(
            ForwarderCopilotRequestOptimizationGateInput {
                body: json!({ "model": "claude-sonnet-4" }),
                headers: &headers,
                config: &CopilotOptimizerConfig::default(),
                is_copilot: false,
            },
        );
        assert!(non_copilot.classification.is_none());
        assert_eq!(non_copilot.body["model"], "claude-sonnet-4");

        let enabled = source.prepare_copilot_request_optimization(
            ForwarderCopilotRequestOptimizationGateInput {
                body: json!({
                    "model": "claude-sonnet-4",
                    "messages": [{"role": "user", "content": "Hello"}]
                }),
                headers: &headers,
                config: &CopilotOptimizerConfig::default(),
                is_copilot: true,
            },
        );
        assert!(enabled.classification.is_some());
        assert_eq!(enabled.body["model"], "gpt-5-mini");
    }

    #[test]
    fn forwarder_auth_source_prepares_optional_copilot_auth_optimization() {
        let source = default_forwarder_auth_source();
        let headers = HeaderMap::new();
        let config = CopilotOptimizerConfig {
            request_classification: true,
            deterministic_request_id: true,
            ..CopilotOptimizerConfig::default()
        };

        let skipped = source.prepare_optional_copilot_auth_optimization(
            ForwarderMaybeCopilotAuthOptimizationInput {
                classification: None,
                config: &config,
                session_source_body: &json!({}),
                request_body: &json!({}),
                headers: &headers,
            },
        );
        assert!(skipped.is_none());

        let classification = CopilotClassification {
            initiator: "user",
            is_warmup: false,
            is_compact: false,
            is_subagent: true,
        };
        let prepared = source
            .prepare_optional_copilot_auth_optimization(
                ForwarderMaybeCopilotAuthOptimizationInput {
                    classification: Some(classification),
                    config: &config,
                    session_source_body: &json!({
                        "metadata": { "session_id": "session-a" }
                    }),
                    request_body: &json!({
                        "messages": [{"role": "user", "content": "Hello"}]
                    }),
                    headers: &headers,
                },
            )
            .expect("prepared copilot auth optimization");

        assert!(prepared.request_classification_enabled);
        assert_eq!(prepared.initiator, "user");
        assert!(prepared.is_subagent);
        assert!(prepared.deterministic_request_id.is_some());
        assert!(prepared.interaction_id.is_some());
    }

    struct ChannelHeaderAuthProvider;

    impl AuthProvider for ChannelHeaderAuthProvider {
        fn resolve_auth<'a>(
            &'a self,
            app: &'a AppKind,
            provider: &'a ProviderSpec,
            channel: &'a ChannelSpec,
            request: &'a ProxyRequest,
        ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>> {
            let app = app.as_str().to_string();
            let provider_id = provider.id.clone();
            let channel_id = channel.id.clone();
            let requested_model = request.requested_model.clone();
            Box::pin(async move {
                Ok(AuthInfo {
                    headers: vec![("x-core-auth-channel".to_string(), channel_id.clone())],
                    account_ref: Some(provider_id.clone()),
                    metadata: json!({
                        "app": app,
                        "providerId": provider_id,
                        "channelId": channel_id,
                        "requestedModel": requested_model,
                    }),
                })
            })
        }
    }

    #[tokio::test]
    async fn forwarder_auth_source_uses_core_auth_provider_route_context_headers() {
        let source = forwarder_auth_source_from_sources(
            default_managed_account_runtime_source(),
            Arc::new(ChannelHeaderAuthProvider),
        );
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "provider-key"
                }
            }),
            None,
        );
        let attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            ChannelRouteCandidate {
                channel_id: "channel-auth".to_string(),
                provider_id: provider.id.clone(),
                channel_name: "Channel Auth".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                public_model: Some("sonnet-public".to_string()),
                upstream_model: Some("sonnet-upstream".to_string()),
                route_group: "default".to_string(),
                priority: 100,
                weight: 1,
                source_kind: "manual".to_string(),
            },
        );
        let method = Method::POST;
        let body = json!({ "model": "sonnet-public" });
        let headers = HeaderMap::new();

        let resolved = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: false,
                copilot_optimization: None,
            })
            .await
            .expect("resolve auth headers");

        assert_eq!(resolved.codex_oauth_session_headers.len(), 0);
        assert_eq!(resolved.auth_headers.len(), 1);
        assert_eq!(
            resolved.auth_headers[0].0,
            http::HeaderName::from_static("x-core-auth-channel")
        );
        assert_eq!(
            resolved.auth_headers[0].1.to_str().expect("header value"),
            "channel-auth"
        );
        assert!(!resolved
            .auth_headers
            .iter()
            .any(|(name, _)| name == http::header::AUTHORIZATION));
    }

    #[tokio::test]
    async fn forwarder_auth_source_uses_core_codex_oauth_session_header_gate() {
        let source = forwarder_auth_source_from_managed_account_runtime_source(Arc::new(
            StaticManagedAuthResolutionSource,
        ));
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = provider_with_managed_account_binding("codex_oauth", "codex-acct");
        let attempt = ForwardAttempt::from_provider(provider);
        let method = Method::POST;
        let body = json!({ "model": "gpt-5" });
        let headers = HeaderMap::new();

        let without_client_session = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: false,
                copilot_optimization: None,
            })
            .await
            .expect("resolve auth headers without client session");

        assert!(without_client_session
            .codex_oauth_session_headers
            .is_empty());

        let with_client_session = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: true,
                copilot_optimization: None,
            })
            .await
            .expect("resolve auth headers with client session");

        assert!(with_client_session
            .auth_headers
            .iter()
            .any(|(name, value)| {
                name == http::header::AUTHORIZATION
                    && value == http::HeaderValue::from_static("Bearer codex-token:codex-acct")
            }));

        let mut session_headers = HeaderMap::new();
        for (name, value) in with_client_session.codex_oauth_session_headers {
            session_headers.insert(name, value);
        }

        assert_eq!(
            session_headers.get("session_id"),
            Some(&http::HeaderValue::from_static("session-a"))
        );
        assert_eq!(
            session_headers.get("x-client-request-id"),
            Some(&http::HeaderValue::from_static("session-a"))
        );
        assert_eq!(
            session_headers.get("x-codex-window-id"),
            Some(&http::HeaderValue::from_static("session-a:0"))
        );
    }

    #[tokio::test]
    async fn forwarder_auth_source_uses_core_copilot_auth_override_facts() {
        let source = forwarder_auth_source_from_managed_account_runtime_source(Arc::new(
            StaticManagedAuthResolutionSource,
        ));
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = provider_with_managed_account_binding("github_copilot", "copilot-acct");
        let attempt = ForwardAttempt::from_provider(provider);
        let method = Method::POST;
        let body = json!({ "model": "claude-sonnet-4" });
        let headers = HeaderMap::new();

        let resolved = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: false,
                copilot_optimization: Some(ForwarderPreparedCopilotAuthOptimization {
                    request_classification_enabled: true,
                    initiator: "agent",
                    is_subagent: true,
                    deterministic_request_id: Some("request-id".to_string()),
                    interaction_id: Some("interaction-id".to_string()),
                }),
            })
            .await
            .expect("resolve copilot auth headers");

        let mut map = HeaderMap::new();
        for (name, value) in resolved.auth_headers {
            map.insert(name, value);
        }

        assert_eq!(
            map.get(http::header::AUTHORIZATION),
            Some(&http::HeaderValue::from_static(
                "Bearer copilot-token:copilot-acct"
            ))
        );
        assert_eq!(
            map.get("x-initiator"),
            Some(&http::HeaderValue::from_static("agent"))
        );
        assert_eq!(
            map.get("x-interaction-type"),
            Some(&http::HeaderValue::from_static("conversation-subagent"))
        );
        assert_eq!(
            map.get("x-request-id"),
            Some(&http::HeaderValue::from_static("request-id"))
        );
        assert_eq!(
            map.get("x-agent-task-id"),
            Some(&http::HeaderValue::from_static("request-id"))
        );
        assert_eq!(
            map.get("x-interaction-id"),
            Some(&http::HeaderValue::from_static("interaction-id"))
        );
    }

    #[test]
    fn forwarder_request_source_prepares_provider_request_body() {
        let source = default_forwarder_request_source();
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
        let channel = resolved_channel_attempt_from_candidate(ChannelRouteCandidate {
            channel_id: "ch_1".to_string(),
            provider_id: "provider-a".to_string(),
            channel_name: "Relay".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            public_model: Some("sonnet-mapped".to_string()),
            upstream_model: Some("upstream-sonnet[1M]".to_string()),
            route_group: "default".to_string(),
            priority: 100,
            weight: 1,
            source_kind: "manual".to_string(),
        });

        let body = source
            .prepare_provider_request_body(ForwarderProviderRequestBodyInput {
                app_type: &AppType::Claude,
                body: json!({"model": "claude-sonnet", "messages": []}),
                provider: &provider,
                channel: Some(&channel),
                is_copilot: false,
            })
            .expect("prepared body");

        assert_eq!(body["model"], "upstream-sonnet");
    }

    #[test]
    fn forwarder_request_source_normalizes_copilot_model_body() {
        let source = default_forwarder_request_source();
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );

        let body = source
            .prepare_provider_request_body(ForwarderProviderRequestBodyInput {
                app_type: &AppType::Claude,
                body: json!({"model": "claude-sonnet-4-6[1m]", "messages": []}),
                provider: &provider,
                channel: None,
                is_copilot: true,
            })
            .expect("prepared body");

        assert_eq!(body["model"], "claude-sonnet-4.6-1m");
    }

    #[test]
    fn forwarder_request_source_applies_claude_body_policies() {
        let source = default_forwarder_request_source();
        let mut provider = Provider::with_id(
            "claude-normalize".to_string(),
            "Claude Normalize".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic"
                }
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "messages": [{ "role": "user", "content": "hello" }]
        });
        let claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let codex_adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let media_disabled_config = RectifierConfig {
            request_media_fallback: false,
            request_media_heuristic: false,
            ..RectifierConfig::default()
        };

        source.apply_claude_body_policies(ForwarderClaudeBodyPolicyInput {
            adapter: &claude_adapter,
            body: &mut body,
            provider: &provider,
            api_format: Some("anthropic"),
            config: &media_disabled_config,
        });

        assert!(body.get("output_config").is_none());

        let mut skipped_body = json!({
            "model": "deepseek-v4-pro",
            "output_config": { "effort": "max" },
            "messages": [{ "role": "user", "content": "hello" }]
        });
        source.apply_claude_body_policies(ForwarderClaudeBodyPolicyInput {
            adapter: &codex_adapter,
            body: &mut skipped_body,
            provider: &provider,
            api_format: Some("anthropic"),
            config: &media_disabled_config,
        });

        assert!(skipped_body.get("output_config").is_some());
    }

    #[test]
    fn forwarder_request_source_gates_app_media_prevention() {
        let source = default_forwarder_request_source();
        let provider = Provider::with_id("media".to_string(), "Media".to_string(), json!({}), None);
        let default_config = RectifierConfig::default();
        let mut non_codex_body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        assert_eq!(
            source.apply_app_media_prevention(ForwarderAppMediaPreventionInput {
                app_type: &AppType::Claude,
                body: &mut non_codex_body,
                provider: &provider,
                config: &default_config,
            }),
            0
        );
        assert_eq!(non_codex_body["messages"][0]["content"][0]["type"], "image");

        let mut codex_body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        assert_eq!(
            source.apply_app_media_prevention(ForwarderAppMediaPreventionInput {
                app_type: &AppType::Codex,
                body: &mut codex_body,
                provider: &provider,
                config: &default_config,
            }),
            1
        );
        assert_eq!(codex_body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn forwarder_request_source_projects_media_retry_plan() {
        let source = default_forwarder_request_source();
        let provider = Provider::with_id("media".to_string(), "Media".to_string(), json!({}), None);
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let config = RectifierConfig::default();
        let provider_body = json!({
            "model": "vision-rejecting-model",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        let unsupported_image_error = ProxyError::UpstreamError {
            status: 400,
            body: Some(
                r#"{"error":{"message":"This model cannot process image inputs"}}"#.to_string(),
            ),
        };

        let plan = source
            .media_retry_plan(ForwarderMediaRetryPlanInput {
                app: "claude",
                adapter: &adapter,
                provider: &provider,
                already_retried: false,
                provider_body: &provider_body,
                error: &unsupported_image_error,
                config: &config,
            })
            .expect("media retry plan");
        assert_eq!(plan.body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(
            plan.body["messages"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );

        let ordinary_error = ProxyError::UpstreamError {
            status: 400,
            body: Some(r#"{"error":{"message":"bad request"}}"#.to_string()),
        };
        assert!(source
            .media_retry_plan(ForwarderMediaRetryPlanInput {
                app: "claude",
                adapter: &adapter,
                provider: &provider,
                already_retried: false,
                provider_body: &provider_body,
                error: &ordinary_error,
                config: &config,
            })
            .is_none());
    }

    #[test]
    fn forwarder_request_source_builds_upstream_parts_from_adapter_context() {
        let source = default_forwarder_request_source();
        let mut provider = Provider::with_id(
            "headers".to_string(),
            "Headers".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            custom_user_agent: Some("cc-switch-test/2.0".to_string()),
            ..ProviderMeta::default()
        });
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let mut inbound_headers = HeaderMap::new();
        inbound_headers.insert(
            "anthropic-beta",
            http::HeaderValue::from_static("other-beta"),
        );
        let prepared_request = ForwarderPreparedRequest {
            body: json!({ "model": "claude-3", "messages": [] }),
            request_is_streaming: false,
            force_identity_encoding: false,
            body_model_label: "claude-3".to_string(),
            outbound_model: None,
        };

        let request_parts = source
            .build_upstream_request_parts(ForwarderRequestPartsInput {
                method: &http::Method::POST,
                url: "https://upstream.example/v1/messages",
                inbound_headers: &inbound_headers,
                provider: &provider,
                prepared_request: &prepared_request,
                auth_headers: &[],
                channel_header_overrides: None,
                is_copilot: false,
                adapter: &adapter,
                resolved_claude_api_format: Some("anthropic"),
                codex_oauth_session_headers: &[],
            })
            .expect("request parts");

        assert!(request_parts.preserve_exact_header_case);
        assert_eq!(
            request_parts
                .ordered_headers
                .get("anthropic-beta")
                .and_then(|value| value.to_str().ok()),
            Some("claude-code-20250219,other-beta")
        );
        assert_eq!(
            request_parts
                .ordered_headers
                .get(http::header::USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some("cc-switch-test/2.0")
        );
        let serialized_body: Value =
            serde_json::from_slice(&request_parts.body).expect("serialized body");
        assert_eq!(serialized_body, prepared_request.body);
    }

    #[test]
    fn forwarder_request_source_prepares_final_body_model_facts() {
        let source = default_forwarder_request_source();
        let headers = HeaderMap::new();
        let transform_plan = ForwarderTransformPlan {
            needs_transform: false,
            use_claude_transform: false,
            use_provider_transform: false,
            claude_api_format_for_url: None,
            claude_api_format_for_transform: None,
            codex_responses_to_chat: false,
        };

        let prepared = source.prepare_upstream_body(ForwarderRequestPreparationInput {
            app: "codex",
            provider_id: "provider-a",
            endpoint: "/v1/chat/completions",
            api_format: None,
            body: json!({ "model": "upstream-sonnet", "messages": [] }),
            session_client_provided: false,
            transform_plan: &transform_plan,
            initial_outbound_model: Some("initial-model".to_string()),
            headers: &headers,
        });

        assert_eq!(prepared.body_model_label, "upstream-sonnet");
        assert_eq!(prepared.outbound_model.as_deref(), Some("upstream-sonnet"));

        let prepared_without_model =
            source.prepare_upstream_body(ForwarderRequestPreparationInput {
                app: "codex",
                provider_id: "provider-a",
                endpoint: "/v1/chat/completions",
                api_format: None,
                body: json!({ "messages": [] }),
                session_client_provided: false,
                transform_plan: &transform_plan,
                initial_outbound_model: Some("initial-model".to_string()),
                headers: &headers,
            });

        assert_eq!(prepared_without_model.body_model_label, "<none>");
        assert_eq!(
            prepared_without_model.outbound_model.as_deref(),
            Some("initial-model")
        );
    }

    #[test]
    fn forwarder_request_source_projects_anthropic_rectifier_gate() {
        let source = default_forwarder_request_source();
        let mut claude_auth_provider = Provider::with_id(
            "claude-auth".to_string(),
            "Claude Auth".to_string(),
            json!({}),
            None,
        );
        claude_auth_provider.meta = Some(ProviderMeta {
            provider_type: Some("claude_auth".to_string()),
            ..Default::default()
        });
        let default_claude_provider = Provider::with_id(
            "default-provider".to_string(),
            "Default Provider".to_string(),
            json!({}),
            None,
        );

        assert!(
            source.anthropic_rectifiers_enabled(ForwarderAnthropicRectifierGateInput {
                app_type: &AppType::Claude,
                provider: &claude_auth_provider,
            },)
        );
        assert!(
            !source.anthropic_rectifiers_enabled(ForwarderAnthropicRectifierGateInput {
                app_type: &AppType::Codex,
                provider: &claude_auth_provider,
            },)
        );
        assert!(
            source.anthropic_rectifiers_enabled(ForwarderAnthropicRectifierGateInput {
                app_type: &AppType::Claude,
                provider: &default_claude_provider,
            },)
        );
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_projects_success_switch_target() {
        let provider = Provider::with_id(
            "provider-b".to_string(),
            "Provider B".to_string(),
            json!({}),
            None,
        );
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        assert_eq!(
            source.record_success_status("provider-b", &provider).await,
            None
        );

        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );
        let target = source
            .record_success_status("provider-a", &provider)
            .await
            .expect("alternate provider success should schedule switch target");

        assert_eq!(
            target,
            ForwarderFailoverSwitchTarget {
                provider_id: "provider-b".to_string(),
                provider_name: "Provider B".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_current_provider_from_provider() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );

        source.record_current_provider(&provider).await;

        let status = source.status();
        let status = status.read().await;
        assert_eq!(status.current_provider_id.as_deref(), Some("provider-a"));
        assert_eq!(status.current_provider.as_deref(), Some("Provider A"));
    }

    #[test]
    fn forwarder_runtime_state_source_classifies_rectifier_retry_failover() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        match source.rectifier_retry_failure_decision(&ProxyError::Timeout(
            "upstream timed out".to_string(),
        )) {
            ForwarderRectifierRetryFailureDecision::ProviderFailure => {}
            ForwarderRectifierRetryFailureDecision::ClientFailure => {
                panic!("timeout should fail over to the next provider")
            }
        }

        match source.rectifier_retry_failure_decision(&ProxyError::UpstreamError {
            status: 502,
            body: Some("bad gateway".to_string()),
        }) {
            ForwarderRectifierRetryFailureDecision::ProviderFailure => {}
            ForwarderRectifierRetryFailureDecision::ClientFailure => {
                panic!("5xx upstream error should fail over to the next provider")
            }
        }

        match source.rectifier_retry_failure_decision(&ProxyError::UpstreamError {
            status: 400,
            body: Some("invalid request".to_string()),
        }) {
            ForwarderRectifierRetryFailureDecision::ProviderFailure => {
                panic!("client 400 should not fail over after rectifier retry")
            }
            ForwarderRectifierRetryFailureDecision::ClientFailure => {}
        }
    }

    #[test]
    fn forwarder_runtime_state_source_projects_rectifier_retry_logs() {
        let timeout = ProxyError::Timeout("upstream timed out".to_string());

        assert_eq!(
            forwarder_rectifier_retry_success_log_line(
                "claude",
                ForwarderRectifierRetryKind::MediaFallback,
            ),
            "[claude] [Media] Unsupported-image retry succeeded"
        );
        assert_eq!(
            forwarder_rectifier_retry_failure_log_line(
                "claude",
                ForwarderRectifierRetryKind::ThinkingSignature,
                &timeout,
            ),
            "[claude] [RECT-003] 整流重试仍失败: 超时: upstream timed out"
        );
        assert_eq!(
            forwarder_rectifier_retry_success_log_line(
                "claude",
                ForwarderRectifierRetryKind::ThinkingBudget,
            ),
            "[claude] [RECT-011] budget 整流重试成功"
        );
        assert_eq!(
            forwarder_rectifier_retry_failure_label(ForwarderRectifierRetryKind::MediaFallback),
            "media 降级"
        );
        assert_eq!(
            forwarder_rectifier_retry_failure_label(ForwarderRectifierRetryKind::ThinkingSignature),
            "整流"
        );
        assert_eq!(
            forwarder_rectifier_retry_failure_label(ForwarderRectifierRetryKind::ThinkingBudget),
            "budget 整流"
        );
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_terminal_statuses() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        source.record_no_available_provider_status().await;
        {
            let status = source.status();
            let status = status.read().await;
            assert_eq!(status.failed_requests, 1);
            assert_eq!(
                status.last_error.as_deref(),
                Some("所有供应商暂时不可用（熔断器限制）")
            );
        }

        source.record_terminal_failure_status().await;
        let status = source.status();
        let status = status.read().await;
        assert_eq!(status.failed_requests, 2);
        assert_eq!(status.last_error.as_deref(), Some("所有供应商都失败"));
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_request_started_timestamp() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        source.record_request_started_now().await;

        let status = source.status();
        let status = status.read().await;
        assert_eq!(status.total_requests, 1);
        assert!(status.last_request_at.is_some());
    }

    #[test]
    fn forwarder_runtime_state_source_generates_request_ids() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        let request_id = source.next_request_id();

        Uuid::parse_str(&request_id).expect("request id should be a UUID");
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_forward_error_status() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        source
            .record_forward_error_status(&ProxyError::Timeout("upstream timed out".to_string()))
            .await;

        let status = source.status();
        let status = status.read().await;
        assert_eq!(status.failed_requests, 1);
        assert_eq!(
            status.last_error.as_deref(),
            Some("超时: upstream timed out")
        );
    }

    #[test]
    fn forwarder_runtime_state_source_projects_forward_failure_policy() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );
        let provider = Provider::with_id("relay".to_string(), "Relay".to_string(), json!({}), None);
        let retryable =
            source.forward_failure_decision(&ProxyError::Timeout("upstream timed out".to_string()));
        let non_retryable_error = ProxyError::UpstreamError {
            status: 400,
            body: Some(r#"{"error":{"message":"bad request"}}"#.to_string()),
        };
        let non_retryable = source.forward_failure_decision(&non_retryable_error);

        match retryable {
            ForwarderFailureDecision::Retryable => {}
            ForwarderFailureDecision::NonRetryable => {
                panic!("timeout should be retryable")
            }
        }
        assert_eq!(
            retryable_forward_failure_log_line(
                "claude",
                &ProxyError::Timeout("upstream timed out".to_string()),
                &provider,
                1,
                2,
            ),
            "[claude] [FWD-001] Provider Relay 失败，继续尝试下一个 (1/2): 请求超时: upstream timed out"
        );

        match non_retryable {
            ForwarderFailureDecision::Retryable => {
                panic!("client 400 should be non-retryable")
            }
            ForwarderFailureDecision::NonRetryable => {}
        }

        let terminal_log_line =
            terminal_forward_failure_log_line_for_error("claude", 2, 2, Some(&non_retryable_error))
                .expect("terminal failure log for multi-provider attempts");
        assert!(terminal_log_line.starts_with("[claude] [FWD-002] "));
        assert!(terminal_log_line.contains("上游 HTTP 400"));
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_provider_failure_from_provider() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );
        let provider = Provider::with_id("relay".to_string(), "Relay".to_string(), json!({}), None);

        source
            .record_provider_failure(
                &provider,
                &ProxyError::Timeout("upstream timed out".to_string()),
            )
            .await;
        {
            let status = source.status();
            let status = status.read().await;
            assert_eq!(
                status.last_error.as_deref(),
                Some("Provider Relay 失败: 超时: upstream timed out")
            );
        }

        source
            .record_provider_rectifier_retry_failure(
                &provider,
                ForwarderRectifierRetryKind::ThinkingBudget,
                &ProxyError::UpstreamError {
                    status: 502,
                    body: Some("bad gateway".to_string()),
                },
            )
            .await;
        let status = source.status();
        let status = status.read().await;
        assert_eq!(
            status.last_error.as_deref(),
            Some(
                "Provider Relay budget 整流重试失败: 上游错误 (状态码 502): Some(\"bad gateway\")"
            )
        );
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_emits_attempt_phase_events() {
        let provider = Provider::with_id("relay".to_string(), "Relay".to_string(), json!({}), None);
        let attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            ChannelRouteCandidate {
                channel_id: "channel-a".to_string(),
                provider_id: provider.id.clone(),
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
        let events = Arc::new(ProxyEventBus::default());
        let mut subscriber = events.subscribe();
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            events,
        );

        source.emit_attempt_started("req-1", "claude", &attempt);
        let started = subscriber.recv().await.expect("started event");
        assert_eq!(started.event, "channel_attempt");
        assert_eq!(started.payload["requestId"], "req-1");
        assert_eq!(started.payload["channelId"], "channel-a");
        assert!(started.payload.get("error").is_none());

        source.emit_attempt_succeeded("req-1", "claude", &attempt);
        let succeeded = subscriber.recv().await.expect("succeeded event");
        assert_eq!(succeeded.event, "channel_succeeded");
        assert_eq!(succeeded.payload["channelId"], "channel-a");
        assert!(succeeded.payload.get("error").is_none());

        source.emit_attempt_failed_for_error(
            "req-1",
            "claude",
            &attempt,
            &ProxyError::ForwardFailed("upstream failed".to_string()),
        );
        let failed = subscriber.recv().await.expect("failed event");
        assert_eq!(failed.event, "channel_failed");
        assert_eq!(failed.payload["channelId"], "channel-a");
        assert_eq!(failed.payload["error"], "请求转发失败: upstream failed");
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_active_route_target_event() {
        let provider = Provider::with_id("relay".to_string(), "Relay".to_string(), json!({}), None);
        let attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            ChannelRouteCandidate {
                channel_id: "channel-a".to_string(),
                provider_id: provider.id.clone(),
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
        let current_providers = Arc::new(RwLock::new(HashMap::new()));
        let events = Arc::new(ProxyEventBus::default());
        let mut subscriber = events.subscribe();
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            current_providers.clone(),
            events,
        );

        source
            .record_active_route_target("req-route", "claude", &attempt)
            .await;

        let current_providers = current_providers.read().await;
        let target = current_providers
            .get("claude")
            .expect("active route target");
        assert_eq!(target.provider_id, "relay");
        assert_eq!(target.channel_id.as_deref(), Some("channel-a"));
        assert_eq!(target.interface_kind.as_deref(), Some("openai_responses"));
        assert_eq!(target.upstream_model.as_deref(), Some("upstream-sonnet"));

        let route_event = subscriber.recv().await.expect("route selected event");
        assert_eq!(route_event.event, "route_selected");
        assert_eq!(route_event.payload["requestId"], "req-route");
        assert_eq!(route_event.payload["providerId"], "relay");
        assert_eq!(route_event.payload["channelId"], "channel-a");
        assert_eq!(route_event.payload["interfaceKind"], "openai_responses");
    }

    #[tokio::test]
    async fn forwarder_response_source_projects_upstream_error_response() {
        let source = CcSwitchForwarderResponseSource;
        let response = ProxyResponse::buffered(
            http::StatusCode::BAD_REQUEST,
            HeaderMap::new(),
            Bytes::from_static(br#"{"error":"bad request"}"#),
        );

        let error = match source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response,
                request_is_streaming: false,
                non_streaming_timeout: std::time::Duration::from_secs(0),
                streaming_first_byte_timeout: std::time::Duration::from_secs(0),
            })
            .await
        {
            Ok(_) => panic!("expected upstream error"),
            Err(error) => error,
        };

        match error {
            ProxyError::UpstreamError { status, body } => {
                assert_eq!(status, 400);
                assert_eq!(body.as_deref(), Some(r#"{"error":"bad request"}"#));
            }
            other => panic!("expected upstream error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn forwarder_response_source_finalizes_success_and_upstream_error() {
        let source = CcSwitchForwarderResponseSource;
        let success = ProxyResponse::buffered(
            http::StatusCode::OK,
            HeaderMap::new(),
            Bytes::from_static(b"{\"ok\":true}"),
        );
        let success = source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response: success,
                request_is_streaming: false,
                non_streaming_timeout: std::time::Duration::from_secs(0),
                streaming_first_byte_timeout: std::time::Duration::from_secs(0),
            })
            .await
            .expect("success response");
        assert_eq!(success.status(), http::StatusCode::OK);
        assert_eq!(
            success.bytes().await.expect("success body"),
            Bytes::from_static(b"{\"ok\":true}")
        );

        let failure = ProxyResponse::buffered(
            http::StatusCode::BAD_REQUEST,
            HeaderMap::new(),
            Bytes::from_static(b"bad request"),
        );
        let error = match source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response: failure,
                request_is_streaming: false,
                non_streaming_timeout: std::time::Duration::from_secs(0),
                streaming_first_byte_timeout: std::time::Duration::from_secs(0),
            })
            .await
        {
            Ok(_) => panic!("expected upstream error"),
            Err(error) => error,
        };

        match error {
            ProxyError::UpstreamError { status, body } => {
                assert_eq!(status, 400);
                assert_eq!(body.as_deref(), Some("bad request"));
            }
            other => panic!("expected upstream error, got {other:?}"),
        }
    }

    struct StaticCopilotModelsSource {
        endpoint: Option<String>,
        models: Option<Vec<CopilotModel>>,
    }

    impl CoreManagedAccountRuntimeSource for StaticCopilotModelsSource {
        type Error = ProxyError;

        fn resolve_copilot_auth<'a>(
            &'a self,
            _account_id: Option<&'a str>,
            _runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<ProviderAuthInfo, ProxyError>> {
            Box::pin(async move {
                Err(ProxyError::AuthError(
                    "test source does not resolve auth".to_string(),
                ))
            })
        }

        fn resolve_codex_oauth<'a>(
            &'a self,
            _account_id: Option<String>,
            _runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<(ProviderAuthInfo, Option<String>), ProxyError>> {
            Box::pin(async move {
                Err(ProxyError::AuthError(
                    "test source does not resolve oauth".to_string(),
                ))
            })
        }

        fn resolve_copilot_api_endpoint<'a>(
            &'a self,
            _account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move { self.endpoint.clone() })
        }

        fn fetch_copilot_live_models<'a>(
            &'a self,
            _account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Option<Vec<CopilotModel>>, String>> {
            Box::pin(async move { Ok(self.models.clone()) })
        }

        fn resolve_copilot_model_vendor<'a>(
            &'a self,
            _account_id: Option<&'a str>,
            _model_id: &'a str,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move { None })
        }
    }

    struct StaticManagedAuthResolutionSource;

    impl CoreManagedAccountRuntimeSource for StaticManagedAuthResolutionSource {
        type Error = ProxyError;

        fn resolve_copilot_auth<'a>(
            &'a self,
            account_id: Option<&'a str>,
            runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<ProviderAuthInfo, ProxyError>> {
            Box::pin(async move {
                Ok(ProviderAuthInfo::new(
                    format!("copilot-token:{}", account_id.unwrap_or("default")),
                    runtime.provider_auth_strategy(),
                ))
            })
        }

        fn resolve_codex_oauth<'a>(
            &'a self,
            account_id: Option<String>,
            runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<(ProviderAuthInfo, Option<String>), ProxyError>> {
            Box::pin(async move {
                let resolved_account_id = account_id.unwrap_or_else(|| "codex-default".to_string());
                Ok((
                    ProviderAuthInfo::new(
                        format!("codex-token:{resolved_account_id}"),
                        runtime.provider_auth_strategy(),
                    ),
                    Some(resolved_account_id),
                ))
            })
        }

        fn resolve_copilot_api_endpoint<'a>(
            &'a self,
            _account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move { None })
        }

        fn fetch_copilot_live_models<'a>(
            &'a self,
            _account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Option<Vec<CopilotModel>>, String>> {
            Box::pin(async move { Ok(None) })
        }

        fn resolve_copilot_model_vendor<'a>(
            &'a self,
            _account_id: Option<&'a str>,
            _model_id: &'a str,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move { None })
        }
    }

    fn provider_with_managed_account_binding(auth_provider: &str, account_id: &str) -> Provider {
        let mut provider = Provider::with_id(
            format!("{auth_provider}-provider"),
            "Managed Account Provider".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some(auth_provider.to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some(auth_provider.to_string()),
                account_id: Some(account_id.to_string()),
            }),
            ..ProviderMeta::default()
        });
        provider
    }

    #[tokio::test]
    async fn managed_account_runtime_source_resolves_provider_account_bindings() {
        let source = StaticManagedAuthResolutionSource;
        let copilot_provider =
            provider_with_managed_account_binding("github_copilot", "copilot-acct");
        let codex_provider = provider_with_managed_account_binding("codex_oauth", "codex-acct");

        let copilot = source
            .resolve_auth_for_provider(
                &copilot_provider,
                ProviderAuthInfo::new(
                    "PROXY_MANAGED".to_string(),
                    ProviderAuthStrategy::GitHubCopilot,
                ),
            )
            .await
            .expect("copilot managed auth");
        assert_eq!(copilot.auth.api_key, "copilot-token:copilot-acct");
        assert_eq!(copilot.auth.strategy, ProviderAuthStrategy::GitHubCopilot);
        assert_eq!(copilot.codex_oauth_account_id, None);
        assert!(!copilot.should_send_codex_oauth_session_headers);

        let codex = source
            .resolve_auth_for_provider(
                &codex_provider,
                ProviderAuthInfo::new(
                    "PROXY_MANAGED".to_string(),
                    ProviderAuthStrategy::CodexOAuth,
                ),
            )
            .await
            .expect("codex managed auth");
        assert_eq!(codex.auth.api_key, "codex-token:codex-acct");
        assert_eq!(codex.auth.strategy, ProviderAuthStrategy::CodexOAuth);
        assert_eq!(codex.codex_oauth_account_id.as_deref(), Some("codex-acct"));
        assert!(codex.should_send_codex_oauth_session_headers);
    }

    #[tokio::test]
    async fn managed_account_runtime_source_gates_copilot_live_model_by_adapter() {
        let source = StaticCopilotModelsSource {
            endpoint: None,
            models: Some(vec![CopilotModel {
                id: "claude-sonnet-4.6".to_string(),
                name: "Claude Sonnet 4.6".to_string(),
                vendor: "Anthropic".to_string(),
                model_picker_enabled: true,
            }]),
        };
        let provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({}),
            None,
        );
        let mut body = json!({ "model": "claude-sonnet-4-6" });

        source
            .apply_copilot_live_model_for_adapter(&provider, &mut body, false)
            .await;

        assert_eq!(body["model"], "claude-sonnet-4-6");

        source
            .apply_copilot_live_model_for_adapter(&provider, &mut body, true)
            .await;

        assert_eq!(body["model"], "claude-sonnet-4.6");
    }

    #[tokio::test]
    async fn managed_account_runtime_source_applies_copilot_dynamic_base_url() {
        let source = StaticCopilotModelsSource {
            endpoint: Some("https://api.enterprise.githubcopilot.com".to_string()),
            models: None,
        };
        let provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({}),
            None,
        );
        let mut base_url = "https://api.githubcopilot.com".to_string();

        source
            .apply_copilot_dynamic_base_url_for_provider(&provider, &mut base_url, true, false)
            .await;

        assert_eq!(base_url, "https://api.enterprise.githubcopilot.com");
    }

    #[tokio::test]
    async fn managed_account_runtime_source_gates_claude_api_format_by_adapter() {
        let source = StaticCopilotModelsSource {
            endpoint: None,
            models: None,
        };
        let provider = Provider::with_id(
            "claude".to_string(),
            "Claude".to_string(),
            json!({
                "api_format": "openai_chat"
            }),
            None,
        );
        let body = json!({ "model": "claude-sonnet-4" });

        assert_eq!(
            source
                .resolve_claude_api_format_for_adapter(&provider, &body, false, false)
                .await,
            None
        );
        assert_eq!(
            source
                .resolve_claude_api_format_for_adapter(&provider, &body, false, true)
                .await
                .as_deref(),
            Some("openai_chat")
        );
    }

    #[test]
    fn forwarder_request_source_plans_signature_rectifier_retry() {
        let source = default_forwarder_request_source();
        let mut body = json!({
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "private", "signature": "bad"},
                    {"type": "text", "text": "visible", "signature": "bad"}
                ]
            }]
        });
        let error = ProxyError::UpstreamError {
            status: 400,
            body: Some("invalid signature in thinking block".to_string()),
        };

        let plan =
            source.thinking_signature_rectifier_plan(ForwarderThinkingSignatureRectifierInput {
                app: "claude",
                body: &mut body,
                error: &error,
                already_retried: false,
                config: &RectifierConfig::default(),
            });

        assert_eq!(plan, ForwarderRequestRectifierPlan::Retry);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0].get("signature").is_none());
    }

    #[test]
    fn forwarder_request_source_plans_budget_rectifier_retry() {
        let source = default_forwarder_request_source();
        let mut body = json!({
            "messages": [{"role": "user", "content": "Hello"}],
            "max_tokens": 1024,
            "thinking": {"type": "enabled", "budget_tokens": 512}
        });
        let error = ProxyError::UpstreamError {
            status: 400,
            body: Some(
                "thinking.budget_tokens: Input should be greater than or equal to 1024".to_string(),
            ),
        };

        let plan = source.thinking_budget_rectifier_plan(ForwarderThinkingBudgetRectifierInput {
            app: "claude",
            body: &mut body,
            error: &error,
            already_retried: false,
            config: &RectifierConfig::default(),
        });

        assert_eq!(plan, ForwarderRequestRectifierPlan::Retry);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 32000);
        assert_eq!(body["max_tokens"], 64000);
    }

    #[test]
    fn proxy_config_adapter_preserves_management_contracts() {
        use crate::proxy_core::api::auth::{
            resolve_management_auth_decision, ManagementAuthDecision,
        };

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
        let existing_p1_toggle = auto_failover_toggle_plan_from_sources(
            app_config.clone(),
            true,
            vec!["provider-a".to_string()],
            None,
        )
        .expect("toggle with existing P1");
        assert!(existing_p1_toggle.plan.auto_failover_enabled);
        assert_eq!(
            existing_p1_toggle.plan.provider_id_to_switch_to.as_deref(),
            Some("provider-a")
        );
        assert!(existing_p1_toggle
            .plan
            .provider_id_to_add_to_queue
            .is_none());
        let auto_add_current_toggle = auto_failover_toggle_plan_from_sources(
            app_config.clone(),
            true,
            Vec::new(),
            Some("current-provider".to_string()),
        )
        .expect("toggle with empty queue uses current provider");
        assert_eq!(
            auto_add_current_toggle
                .plan
                .provider_id_to_add_to_queue
                .as_deref(),
            Some("current-provider")
        );
        assert_eq!(
            auto_add_current_toggle
                .plan
                .provider_id_to_switch_to
                .as_deref(),
            Some("current-provider")
        );
        let mut takeover_disabled_config = app_config.clone();
        takeover_disabled_config.enabled = false;
        assert_eq!(
            auto_failover_toggle_plan_from_sources(
                takeover_disabled_config,
                true,
                Vec::new(),
                None,
            )
            .expect_err("enabled toggle should require proxy takeover"),
            AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE
        );
        assert!(failover_switch_app_enabled_from_config_result(
            "claude",
            Ok(app_config.clone())
        ));
        let mut switch_disabled_app_config = app_config.clone();
        switch_disabled_app_config.enabled = false;
        assert!(!failover_switch_app_enabled_from_config_result(
            "claude",
            Ok(switch_disabled_app_config)
        ));
        assert!(!failover_switch_app_enabled_from_config_result(
            "claude",
            Err(AppError::Config("missing proxy_config".to_string()))
        ));
        let takeover_disabled_app_config = proxy_app_config_with_enabled(app_config.clone(), false);
        assert!(!takeover_disabled_app_config.enabled);
        assert!(takeover_disabled_app_config.auto_failover_enabled);
        let takeover_enabled_app_config =
            proxy_app_config_with_enabled(takeover_disabled_app_config, true);
        assert!(takeover_enabled_app_config.enabled);
        assert!(takeover_enabled_app_config.auto_failover_enabled);
        let switchback_target = reset_circuit_breaker_switchback_target_from_sources(
            true,
            true,
            true,
            "provider-a",
            Some("provider-b".to_string()),
            vec![
                FailoverQueueItem {
                    provider_id: "provider-a".to_string(),
                    provider_name: "Provider A".to_string(),
                    sort_index: Some(1),
                    provider_notes: None,
                },
                FailoverQueueItem {
                    provider_id: "provider-b".to_string(),
                    provider_name: "Provider B".to_string(),
                    sort_index: Some(2),
                    provider_notes: None,
                },
            ],
            Some("Provider A".to_string()),
        )
        .expect("restored provider should switch back");
        assert_eq!(
            switchback_target,
            ResetCircuitBreakerSwitchbackTarget {
                provider_id: "provider-a".to_string(),
                provider_name: "Provider A".to_string(),
                restored_sort_index: Some(1),
                current_sort_index: Some(2),
            }
        );
        assert!(reset_circuit_breaker_switchback_target_from_sources(
            true,
            true,
            true,
            "provider-a",
            None,
            Vec::<FailoverQueueItem>::new(),
            None,
        )
        .is_none());
        assert!(reset_circuit_breaker_switchback_target_from_sources(
            false,
            true,
            true,
            "provider-a",
            Some("provider-b".to_string()),
            Vec::<FailoverQueueItem>::new(),
            None,
        )
        .is_none());

        let default_proxy_config = ProxyConfig::default();
        let loopback_auth = resolve_management_auth_decision(
            &default_proxy_config.listen_address,
            default_proxy_config.management_auth_token.as_deref(),
            None,
        )
        .expect("loopback auth decision");
        assert_eq!(loopback_auth, ManagementAuthDecision::AllowWithoutToken);
        let mut public_proxy_config = ProxyConfig {
            listen_address: "0.0.0.0".to_string(),
            ..ProxyConfig::default()
        };
        assert_eq!(
            resolve_management_auth_decision(
                &public_proxy_config.listen_address,
                public_proxy_config.management_auth_token.as_deref(),
                None,
            )
            .unwrap_err(),
            ManagementAuthError::RequiredTokenMissing
        );
        assert_eq!(
            resolve_management_auth_decision(
                &public_proxy_config.listen_address,
                public_proxy_config.management_auth_token.as_deref(),
                Some("env-token"),
            )
            .expect("env fallback token"),
            ManagementAuthDecision::RequireToken("env-token".to_string())
        );
        public_proxy_config.management_auth_token = Some(" config-token ".to_string());
        assert_eq!(
            resolve_management_auth_decision(
                &public_proxy_config.listen_address,
                public_proxy_config.management_auth_token.as_deref(),
                Some("env-token"),
            )
            .expect("configured token"),
            ManagementAuthDecision::RequireToken("config-token".to_string())
        );

        assert_eq!(
            CircuitBreakerConfig::from(&app_config),
            CircuitBreakerConfig::default()
        );
        let mut custom_breaker_app_config = app_config.clone();
        custom_breaker_app_config.circuit_failure_threshold = 7;
        custom_breaker_app_config.circuit_timeout_seconds = 45;
        let projected_breaker_config =
            circuit_breaker_config_from_app_config(Some(&custom_breaker_app_config));
        assert_eq!(projected_breaker_config.failure_threshold, 7);
        assert_eq!(projected_breaker_config.timeout_seconds, 45);
        assert_eq!(
            circuit_breaker_config_from_app_config(None),
            CircuitBreakerConfig::default()
        );
        assert_eq!(
            circuit_failure_threshold_from_app_config(Some(&custom_breaker_app_config), 9,),
            7
        );
        assert_eq!(circuit_failure_threshold_from_app_config(None, 9), 9);
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
        let app_summary =
            AppSummaryConfig::new(app_config.enabled, app_config.auto_failover_enabled);
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
                streaming_idle_timeout: ResponseTimeoutConfig::default().streaming.idle_timeout,
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
        assert!(
            !proxy_runtime_config_from_config(ProxyConfig::default(), false).privacy_filter_enabled
        );
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
        assert_eq!(
            fallback.metadata["source"],
            json!("cc_switch_provider_config")
        );
    }

    #[test]
    fn auth_adapter_projects_cc_switch_route_context_source() {
        let provider = ProviderSpec {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        };
        let channel = channel_spec_from_input(ChannelSpecInput {
            id: "channel-a".to_string(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Channel A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            auth_profile_ref: Some("provider:claude:anthropic-main".to_string()),
            models: Vec::new(),
            groups: Vec::new(),
            priority: 0,
            weight: 100,
            retry_policy: Value::Object(Default::default()),
            health_policy: Value::Object(Default::default()),
            header_overrides: Value::Object(Default::default()),
            param_overrides: Value::Object(Default::default()),
            status_code_mapping: Value::Array(Vec::new()),
            tags: Vec::new(),
            metadata: Value::Object(Default::default()),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        });

        let auth = auth_info_from_cc_switch_route_context(&AppKind::Claude, &provider, &channel);

        assert!(auth.headers.is_empty());
        assert_eq!(
            auth.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(auth.metadata["source"], json!("cc_switch_provider_config"));
        assert_eq!(auth.metadata["app"], json!("claude"));
        assert_eq!(auth.metadata["providerId"], json!("provider-a"));
        assert_eq!(auth.metadata["channelId"], json!("channel-a"));
    }

    #[test]
    fn circuit_and_route_adapter_projects_provider_router_contracts() {
        assert_eq!(circuit_breaker_log_codes::OPEN_TO_HALF_OPEN, "CB-001");
        assert_eq!(circuit_breaker_log_codes::HALF_OPEN_TO_CLOSED, "CB-002");
        assert_eq!(
            provider_circuit_key("claude", "provider-a"),
            "claude:provider-a"
        );
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
                "missing".to_string(),
                "provider-b".to_string(),
                "provider-a".to_string(),
            ],
            failover_providers.keys().cloned().collect::<Vec<_>>(),
        );
        assert_eq!(failover_lookups[0].provider_id, "missing");
        assert!(!failover_lookups[0].configured);
        assert_eq!(failover_lookups[1].provider_id, "provider-b");
        assert!(failover_lookups[1].configured);
        assert_eq!(
            failover_lookups[1].circuit_key.as_deref(),
            Some("claude:provider-b")
        );
        let selected_failover = select_failover_provider_ids_from_router_lookup_availability(
            "claude",
            &failover_providers.keys().cloned().collect::<Vec<_>>(),
            failover_lookups.into_iter().map(|lookup| {
                let available = lookup.provider_id == "provider-b";
                (lookup, available)
            }),
        )
        .expect("selected failover provider ids");
        assert_eq!(selected_failover, vec!["provider-b"]);
        assert_eq!(
            crate::proxy_core::api::management::channel_route_source_for_materialized_count(1),
            ChannelRouteSource::MaterializedChannels
        );
        assert_eq!(
            crate::proxy_core::api::management::channel_route_source_for_materialized_count(0),
            ChannelRouteSource::LegacyProjection
        );
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
        assert!(!current_provider_db_fallback_required(Some(
            "settings-provider"
        )));
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
        let current_provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        let selected_current =
            select_current_provider_ids_from_router_source("claude", Some(current_provider))
                .expect("selected current provider id");
        assert_eq!(selected_current, vec!["provider-a"]);
        assert_eq!(
            select_current_provider_ids_from_router_provider_id_source(
                "claude",
                Some("provider-a".to_string())
            )
            .expect("selected current provider id from id"),
            vec!["provider-a"]
        );
        assert!(matches!(
            select_current_provider_ids_from_router_source("claude", None),
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

    struct StaticProviderSource {
        providers: Vec<ProviderSpec>,
        current_provider_id: Option<String>,
    }

    struct StaticRoutePolicySource {
        failover_provider_ids: Vec<String>,
    }

    impl ProviderSource for StaticProviderSource {
        fn list_providers<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Vec<ProviderSpec>>> {
            Box::pin(async move { Ok(self.providers.clone()) })
        }

        fn get_provider<'a>(
            &'a self,
            _app: &'a AppKind,
            provider_id: &'a str,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<ProviderSpec>>> {
            Box::pin(async move {
                Ok(self
                    .providers
                    .iter()
                    .find(|provider| provider.id == provider_id)
                    .cloned())
            })
        }

        fn current_provider_id<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<String>>> {
            Box::pin(async move { Ok(self.current_provider_id.clone()) })
        }
    }

    impl RoutePolicySource for StaticRoutePolicySource {
        fn load_policy<'a>(
            &'a self,
            app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<RoutePolicy>>> {
            Box::pin(async move {
                Ok(Some(
                    crate::proxy_core::api::routing::route_policy_from_failover_provider_ids(
                        app.clone(),
                        self.failover_provider_ids.clone(),
                    ),
                ))
            })
        }
    }

    fn static_provider_spec(id: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: id.to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        }
    }

    #[tokio::test]
    async fn provider_router_provider_source_projects_core_provider_source() {
        let source = StaticProviderSource {
            providers: vec![
                static_provider_spec("provider-a"),
                static_provider_spec("provider-b"),
            ],
            current_provider_id: Some("provider-a".to_string()),
        };

        let provider_ids = provider_ids_from_router_provider_source(&source, "claude")
            .await
            .expect("provider ids");
        assert_eq!(provider_ids, vec!["provider-a", "provider-b"]);

        let current_ids =
            select_current_provider_ids_from_router_provider_source(&source, "claude")
                .await
                .expect("current provider ids");
        assert_eq!(current_ids, vec!["provider-a"]);

        let route_policy_source = StaticRoutePolicySource {
            failover_provider_ids: vec!["provider-b".to_string(), "missing".to_string()],
        };
        let failover_provider_ids =
            failover_provider_ids_from_route_policy_source(&route_policy_source, "claude")
                .await
                .expect("failover provider ids");
        assert_eq!(failover_provider_ids, vec!["provider-b", "missing"]);

        let failover_sources = provider_failover_sources_from_router_provider_source(
            &source,
            "claude",
            failover_provider_ids,
        )
        .await
        .expect("failover sources");
        assert_eq!(
            failover_sources.provider_ids,
            vec!["provider-a", "provider-b"]
        );
        assert_eq!(failover_sources.lookups[0].provider_id, "provider-b");
        assert!(failover_sources.lookups[0].configured);
        assert_eq!(failover_sources.lookups[1].provider_id, "missing");
        assert!(!failover_sources.lookups[1].configured);
    }

    #[test]
    fn proxy_event_adapter_projects_event_stream_contracts() {
        assert_eq!(PROXY_OFFICIAL_WARNING_EVENT, "proxy-official-warning");
        assert_eq!(PROVIDER_SWITCHED_EVENT, "provider-switched");
        assert_eq!(REQUEST_STARTED_EVENT, "request_started");
        assert_eq!(SERVER_STARTED_EVENT, "server_started");
        assert_eq!(SERVER_STOPPED_EVENT, "server_stopped");
        assert_eq!(
            build_proxy_official_warning_event_payload("claude", "Official Claude"),
            json!({
                "appType": "claude",
                "providerName": "Official Claude",
            })
        );
        let official_warning = proxy_official_warning_event_message("claude", "Official Claude");
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
        let official_warning_from_provider =
            proxy_official_warning_event_from_provider("codex", Some(&provider))
                .expect("official warning from provider");
        assert_eq!(
            official_warning_from_provider.event_name,
            "proxy-official-warning"
        );
        assert_eq!(official_warning_from_provider.payload["appType"], "codex");
        assert_eq!(
            official_warning_from_provider.payload["providerName"],
            "Official Codex"
        );
        provider.category = Some("custom".to_string());
        assert!(!provider_is_official_category(&provider));
        assert!(!should_emit_proxy_official_warning_for_provider(&provider));
        assert!(!should_reapply_codex_official_live_for_provider(&provider));
        assert!(proxy_official_warning_event_from_provider("codex", Some(&provider)).is_none());
        provider.category = None;
        assert!(!provider_is_official_category(&provider));
        assert!(!should_emit_proxy_official_warning_for_provider(&provider));
        assert!(!should_reapply_codex_official_live_for_provider(&provider));
        assert!(proxy_official_warning_event_from_provider("codex", Some(&provider)).is_none());
        assert!(proxy_official_warning_event_from_provider("codex", None).is_none());
        assert_eq!(
            build_provider_switched_event_payload("claude", "provider-1", "failover"),
            json!({
                "appType": "claude",
                "providerId": "provider-1",
                "source": "failover",
            })
        );
        let provider_switched = provider_switched_failover_event_message("claude", "provider-1");
        assert_eq!(provider_switched.event_name, "provider-switched");
        assert_eq!(provider_switched.payload["source"], "failover");
        let provider_switched_enabled =
            provider_switched_failover_enabled_event_message("claude", "provider-1");
        assert_eq!(provider_switched_enabled.event_name, "provider-switched");
        assert_eq!(
            provider_switched_enabled.payload["source"],
            "failoverEnabled"
        );
        let server_started = server_started_event_message("127.0.0.1", 15721);
        assert_eq!(server_started.event_name, "server_started");
        assert_eq!(
            server_started.payload,
            json!({"address": "127.0.0.1", "port": 15721})
        );
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

    #[tokio::test]
    async fn codex_chat_transform_adapter_records_non_stream_history() {
        let history = CodexChatHistoryStore::default();
        let tool_context = CodexToolContext::default();
        let response = transform_codex_chat_response_with_history(
            &json!({
                "id": "chatcmpl_1",
                "model": "chat-model",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "read_file",
                                "arguments": "{\"path\":\"README.md\"}"
                            }
                        }],
                        "reasoning_content": "Need the file first."
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 2}
            }),
            &tool_context,
            &history,
        )
        .await
        .expect("transformed response");

        let response_id = response["id"].as_str().expect("response id").to_string();
        let call_id = response["output"]
            .as_array()
            .expect("output array")
            .iter()
            .find(|item| item["type"] == "function_call")
            .and_then(|item| item["call_id"].as_str())
            .expect("call id")
            .to_string();
        let mut request = json!({
            "previous_response_id": response_id,
            "input": [{
                "type": "function_call_output",
                "call_id": call_id,
                "output": "ok"
            }]
        });

        assert_eq!(history.enrich_request(&mut request).await, 1);
        assert_eq!(request["input"][0]["type"], "function_call");
        assert_eq!(
            request["input"][0]["reasoning_content"],
            "Need the file first."
        );
    }

    #[tokio::test]
    async fn codex_chat_stream_transform_adapter_records_history() {
        use futures::StreamExt as _;

        let history = Arc::new(CodexChatHistoryStore::default());
        let upstream = futures::stream::iter(vec![
            Ok::<_, std::io::Error>(Bytes::from_static(
                b"data: {\"id\":\"chatcmpl_stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"reasoning_content\":\"Need stream file.\"}}]}\n\n",
            )),
            Ok(Bytes::from_static(
                b"data: {\"id\":\"chatcmpl_stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_stream\",\"type\":\"function\",\"function\":{\"name\":\"read_file\"}}]}}]}\n\n",
            )),
            Ok(Bytes::from_static(
                b"data: {\"id\":\"chatcmpl_stream\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\\\"README.md\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            )),
            Ok(Bytes::from_static(b"data: [DONE]\n\n")),
        ]);

        let output = transform_codex_chat_sse_with_history(
            upstream,
            CodexToolContext::default(),
            history.clone(),
        )
        .collect::<Vec<_>>()
        .await;
        let bytes = output
            .into_iter()
            .map(|item| item.expect("stream chunk"))
            .collect::<Vec<_>>();
        let combined = String::from_utf8(bytes.concat()).expect("utf8 sse");
        assert!(combined.contains("event: response.output_item.done"));

        let mut request = json!({
            "previous_response_id": "resp_chatcmpl_stream",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_stream",
                "output": "ok"
            }]
        });

        assert_eq!(history.enrich_request(&mut request).await, 1);
        assert_eq!(request["input"][0]["type"], "function_call");
        assert_eq!(
            request["input"][0]["reasoning_content"],
            "Need stream file."
        );
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
        let auth_headers = provider_codex_auth_headers(&ProviderAuthInfo::new(
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
        assert_eq!(
            codex_config_text_from_settings(&json!({"config": 42})),
            None
        );
        assert_eq!(codex_config_text_from_settings(&json!({})), None);
        let codex_auth_settings = json!({"auth": {"OPENAI_API_KEY": "sk-auth"}});
        let auth =
            codex_auth_object_value_from_settings(&codex_auth_settings).expect("codex auth object");
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
        let imported_custom_provider = provider_from_default_live_settings(
            &AppType::Codex,
            api_key_live_provider.settings_config.clone(),
        );
        assert_eq!(imported_custom_provider.category.as_deref(), Some("custom"));
        let imported_claude_provider = provider_from_default_live_settings(
            &AppType::Claude,
            json!({"env": {"ANTHROPIC_API_KEY": "sk-test"}}),
        );
        assert_eq!(imported_claude_provider.category.as_deref(), Some("custom"));
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
        let imported_bearer_provider = provider_from_default_live_settings(
            &AppType::Codex,
            bearer_live_provider.settings_config.clone(),
        );
        assert_eq!(imported_bearer_provider.category.as_deref(), Some("custom"));
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
        assert!(!live_backfill_settings
            .get("config")
            .and_then(Value::as_str)
            .expect("restored config")
            .contains("experimental_bearer_token"));
        let injected_unified_config =
            crate::codex_config::inject_codex_unified_session_bucket("").expect("inject");
        let mut official_unified_backfill = json!({"config": injected_unified_config});
        strip_codex_unified_session_bucket_for_provider_backfill(
            &official_category_provider,
            &mut official_unified_backfill,
        )
        .expect("strip official unified session bucket");
        assert!(!official_unified_backfill
            .get("config")
            .and_then(Value::as_str)
            .expect("official stripped config")
            .contains("model_provider"));
        let mut custom_unified_backfill = json!({
            "config": crate::codex_config::inject_codex_unified_session_bucket("").expect("inject")
        });
        strip_codex_unified_session_bucket_for_provider_backfill(
            &custom_category_provider,
            &mut custom_unified_backfill,
        )
        .expect("custom backfill no-op");
        assert!(custom_unified_backfill
            .get("config")
            .and_then(Value::as_str)
            .expect("custom retained config")
            .contains("model_provider"));
        let write_settings = json!({
            "auth": {"OPENAI_API_KEY": "sk-write"},
            "config": "model = \"gpt-5\""
        });
        let write_parts =
            codex_provider_live_write_parts(&write_settings, &custom_category_provider)
                .expect("codex provider live write parts");
        assert_eq!(write_parts.category, Some("custom"));
        assert_eq!(
            write_parts
                .auth
                .get("OPENAI_API_KEY")
                .and_then(Value::as_str),
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
        let provider_validation_parts =
            provider_settings_validation_parts(&AppType::Codex, &official_live_provider)
                .expect("provider validation parts");
        assert_eq!(provider_validation_parts.codex_config_text, Some(""));
        assert!(matches!(
            provider_settings_validation_parts(&AppType::Codex, &invalid_shape),
            Err(ProviderSettingsValidationIssue::Codex(
                CodexProviderValidationIssue::NotObject
            ))
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
            provider_settings_validation_parts(&AppType::Codex, &missing_auth),
            Err(ProviderSettingsValidationIssue::Codex(
                CodexProviderValidationIssue::MissingAuth
            ))
        ));
        assert!(matches!(
            provider_settings_validation_parts(&AppType::Codex, &auth_not_object),
            Err(ProviderSettingsValidationIssue::Codex(
                CodexProviderValidationIssue::AuthNotObject
            ))
        ));
        let invalid_config = Provider::with_id(
            "codex-live-invalid-config".to_string(),
            "Codex Live Invalid Config".to_string(),
            json!({"auth": {}, "config": 42}),
            None,
        );
        assert!(matches!(
            provider_settings_validation_parts(&AppType::Codex, &invalid_config),
            Err(ProviderSettingsValidationIssue::Codex(
                CodexProviderValidationIssue::ConfigInvalidType
            ))
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
        assert_eq!(
            inferred_profile.effort_value_mode.as_deref(),
            Some("deepseek")
        );
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
        let mut forwarder_body = json!({"model": "client-model"});
        assert_eq!(
            provider_apply_codex_chat_upstream_model(&chat_provider, &mut forwarder_body)
                .as_deref(),
            Some("upstream-model")
        );
        assert_eq!(forwarder_body["model"], "upstream-model");
        let reasoning_options =
            provider_codex_chat_reasoning_options(&reasoning_provider, &forwarder_body)
                .expect("deepseek reasoning options");
        assert_eq!(reasoning_options.supports_effort, Some(true));
        assert_eq!(
            reasoning_options.effort_value_mode.as_deref(),
            Some("deepseek")
        );

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
            parsed_takeover.get("model").and_then(toml::Value::as_str),
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
            extract_gemini_model_from_path("/v1beta/models/gemini-pro:generateContent").as_deref(),
            Some("gemini-pro")
        );
        assert_eq!(
            request_model_for_forward(&AppKind::Codex, "", &json!({"model": " gpt-5 "})).as_deref(),
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
            extract_gemini_api_key_from_settings(&provider.settings_config).as_deref(),
            Some("ya29.access-token")
        );
        assert_eq!(
            extract_gemini_base_url_from_settings(&provider.settings_config).as_deref(),
            Some("https://generativelanguage.googleapis.com/v1beta")
        );
        assert_eq!(
            required_gemini_provider_base_url(&provider).as_deref(),
            Ok("https://generativelanguage.googleapis.com/v1beta")
        );
        assert_eq!(detect_gemini_auth_type(&provider), GeminiAuthType::Generic);
        let google_official_provider = Provider::with_id(
            "google-official".to_string(),
            "Google Gemini".to_string(),
            json!({"env": {}}),
            None,
        );
        assert_eq!(
            detect_gemini_auth_type(&google_official_provider),
            GeminiAuthType::GoogleOfficial
        );
        let mut packy_partner_provider = Provider::with_id(
            "packy-partner".to_string(),
            "Gemini Partner".to_string(),
            json!({"env": {}}),
            None,
        );
        packy_partner_provider.meta = Some(crate::provider::ProviderMeta {
            partner_promotion_key: Some("packycode".to_string()),
            ..crate::provider::ProviderMeta::default()
        });
        assert_eq!(
            detect_gemini_auth_type(&packy_partner_provider),
            GeminiAuthType::Packycode
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
        assert_eq!(
            provider_gemini_auth_strategy(&provider),
            ProviderAuthStrategy::GoogleOAuth
        );
        assert_eq!(provider_auth.api_key, "ya29.access-token");
        assert_eq!(
            provider_auth.access_token.as_deref(),
            Some("ya29.access-token")
        );
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
        assert_eq!(
            gemini_env_string_map_from_settings(&gemini_env_json_from_map(&live_env)),
            live_env
        );
        validate_provider_gemini_settings(&provider)
            .expect("provider Gemini settings should pass basic shape validation");
        let invalid_env_provider = Provider::with_id(
            "gemini-invalid-env".to_string(),
            "Gemini Invalid Env".to_string(),
            json!({"env": "invalid"}),
            None,
        );
        assert!(matches!(
            validate_provider_gemini_settings(&invalid_env_provider),
            Err(AppError::Localized { key, .. }) if key == "gemini.validation.invalid_env"
        ));
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
        assert!(provider_gemini_live_config_object(&Provider::with_id(
            "gemini-null-config".to_string(),
            "Gemini Null Config".to_string(),
            json!({"config": Value::Null}),
            None,
        ))
        .expect("null config should preserve live file")
        .is_none());
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

        let oauth_headers =
            build_gemini_auth_headers("refresh-token", Some("ya29.access-token"), true)
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
        let provider_api_key_headers = provider_gemini_auth_headers(&ProviderAuthInfo::new(
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
        let auth_key =
            extract_claude_auth_key_from_settings(&settings).expect("anthropic auth token");
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

        let explicit_chat_response = provider_claude_transform_response_for_api_format(
            &json!({
                "id": "chatcmpl_2",
                "model": "chat-model",
                "choices": [{
                    "message": {"role": "assistant", "content": "Explicit chat"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 2}
            }),
            "openai_chat",
            None,
            None,
            None,
            None,
        )
        .expect("explicit chat response");
        assert_eq!(
            explicit_chat_response["content"][0]["text"],
            "Explicit chat"
        );

        let explicit_responses_response = provider_claude_transform_response_for_api_format(
            &json!({
                "id": "resp_2",
                "model": "responses-model",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{"type": "output_text", "text": "Explicit responses"}]
                }],
                "usage": {"input_tokens": 1, "output_tokens": 2}
            }),
            "openai_responses",
            None,
            None,
            None,
            None,
        )
        .expect("explicit responses response");
        assert_eq!(
            explicit_responses_response["content"][0]["text"],
            "Explicit responses"
        );

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

    #[tokio::test]
    async fn claude_stream_transform_adapter_dispatches_api_formats() {
        use futures::StreamExt as _;

        let chat_stream = futures::stream::iter(vec![
            Ok::<_, std::io::Error>(Bytes::from_static(
                b"data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
            )),
            Ok(Bytes::from_static(
                b"data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
            )),
            Ok(Bytes::from_static(b"data: [DONE]\n\n")),
        ]);
        let chat_output = provider_claude_transform_sse_for_api_format(
            chat_stream,
            "openai_chat",
            None,
            None,
            None,
            None,
        )
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|item| String::from_utf8(item.expect("chat chunk").to_vec()).expect("chat utf8"))
        .collect::<String>();
        assert!(chat_output.contains("event: message_start"));
        assert!(chat_output.contains("Hi"));

        let responses_stream = futures::stream::iter(vec![Ok::<_, std::io::Error>(
            Bytes::from_static(
                b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-4o\",\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\nevent: response.content_part.added\ndata: {\"type\":\"response.content_part.added\",\"part\":{\"type\":\"output_text\",\"text\":\"\"},\"output_index\":0,\"content_index\":0}\n\nevent: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Done\",\"output_index\":0,\"content_index\":0}\n\nevent: response.content_part.done\ndata: {\"type\":\"response.content_part.done\",\"output_index\":0,\"content_index\":0}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
            ),
        )]);
        let responses_output = provider_claude_transform_sse_for_api_format(
            responses_stream,
            "openai_responses",
            None,
            None,
            None,
            None,
        )
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|item| {
            String::from_utf8(item.expect("responses chunk").to_vec()).expect("responses utf8")
        })
        .collect::<String>();
        assert!(responses_output.contains("event: message_start"));
        assert!(responses_output.contains("Done"));

        let gemini_stream = futures::stream::iter(vec![Ok::<_, std::io::Error>(
            Bytes::from_static(
                b"data: {\"responseId\":\"gemini_1\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"text\":\"Gemini hi\"}]}}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n",
            ),
        )]);
        let gemini_output = provider_claude_transform_sse_for_api_format(
            gemini_stream,
            "gemini_native",
            Some(Arc::new(GeminiShadowStore::default())),
            Some("provider-a".to_string()),
            Some("session-a".to_string()),
            None,
        )
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|item| String::from_utf8(item.expect("gemini chunk").to_vec()).expect("gemini utf8"))
        .collect::<String>();
        assert!(gemini_output.contains("event: message_start"));
        assert!(gemini_output.contains("Gemini hi"));
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
        let info = proxy_server_info_from_parts("127.0.0.1", 15721, "2026-06-21T00:00:00Z");
        assert_eq!(info.address, "127.0.0.1");
        assert_eq!(info.port, 15721);
        assert_eq!(info.started_at, "2026-06-21T00:00:00Z");
        let takeover = proxy_takeover_status_from_parts(true, false, true, false, false);
        assert!(takeover.claude);
        assert!(!takeover.codex);
        assert!(takeover.gemini);
        assert!(!takeover.opencode);
        assert!(!takeover.openclaw);
        let app_config = |app_type: &str, enabled: bool| AppProxyConfig {
            app_type: app_type.to_string(),
            enabled,
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
        let takeover_from_config = proxy_takeover_status_from_enabled_options(
            Some(app_config("claude", true).enabled),
            None,
            Some(app_config("gemini", true).enabled),
            None,
            None,
        );
        assert!(takeover_from_config.claude);
        assert!(!takeover_from_config.codex);
        assert!(takeover_from_config.gemini);
        assert!(!takeover_from_config.opencode);
        assert!(!takeover_from_config.openclaw);

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

        let provider_only = current_route_target_from_provider("codex", "provider-b", "Provider B");
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
        assert!(
            !proxy_hot_switch_should_sync_claude_live_while_proxy_active(&AppType::Claude, false)
        );
        assert!(proxy_hot_switch_should_sync_claude_live_while_proxy_active(
            &AppType::Claude,
            true
        ));
        assert!(
            !proxy_hot_switch_should_sync_claude_live_while_proxy_active(&AppType::Codex, true)
        );

        assert!(!proxy_takeover_marked_state_is_reusable(false, false));
        assert!(!proxy_takeover_marked_state_is_reusable(true, false));
        assert!(!proxy_takeover_marked_state_is_reusable(false, true));
        assert!(proxy_takeover_marked_state_is_reusable(true, true));
        assert!(!proxy_takeover_should_restore_existing_backup_before_retakeover(false, false));
        assert!(proxy_takeover_should_restore_existing_backup_before_retakeover(true, false));
        assert!(!proxy_takeover_should_restore_existing_backup_before_retakeover(false, true));
        assert!(!proxy_takeover_should_restore_existing_backup_before_retakeover(true, true));
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
        let stripped = remove_common_config_from_settings(&AppType::Codex, &applied, codex_snippet)
            .expect("codex remove");
        assert_eq!(stripped, codex_settings);

        let gemini_settings = json!({});
        let gemini_snippet = r#"{"SHARED_REGION": "us-central1"}"#;
        let applied =
            apply_common_config_to_settings(&AppType::Gemini, &gemini_settings, gemini_snippet)
                .expect("gemini apply");
        assert_eq!(applied, json!({"env": {"SHARED_REGION": "us-central1"}}));

        let opencode_snippet = common_config_snippet_from_settings(
            &AppType::OpenCode,
            &json!({
                "npm": "@ai-sdk/openai",
                "options": {
                    "apiKey": "secret",
                    "baseURL": "https://opencode.example",
                    "timeout": 30
                }
            }),
        )
        .expect("opencode snippet");
        assert_eq!(
            serde_json::from_str::<Value>(&opencode_snippet).expect("opencode snippet json"),
            json!({"npm": "@ai-sdk/openai", "options": {"timeout": 30}})
        );
        assert_eq!(
            common_config_snippet_from_settings(&AppType::ClaudeDesktop, &json!({}))
                .expect("claude desktop snippet"),
            ""
        );
        assert_eq!(
            common_config_snippet_from_settings(&AppType::Hermes, &json!({}))
                .expect("hermes snippet"),
            ""
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

        assert!(!provider_common_config_storage_normalization_requires_snippet(&provider));
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

        assert!(provider_common_config_storage_normalization_requires_snippet(&provider));
        assert_eq!(
            normalize_provider_common_config_for_storage(&AppType::Claude, &provider, Some("   "))
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
        assert_eq!(oauth.masked_access_token(), Some("ya29...2345".to_string()));

        let codex_adapter = crate::proxy::provider::CodexAdapter::new();
        let codex_provider = Provider::with_id(
            "codex".to_string(),
            "Codex".to_string(),
            json!({"apiKey": "sk-forwarder-auth"}),
            None,
        );
        let forwarder_auth = codex_adapter
            .extract_auth(&codex_provider)
            .expect("codex forwarder auth info");
        assert_eq!(forwarder_auth.api_key, "sk-forwarder-auth");
        assert_eq!(forwarder_auth.strategy, ProviderAuthStrategy::Bearer);
        let forwarder_auth_headers = codex_adapter
            .get_auth_headers(&forwarder_auth)
            .expect("codex forwarder auth headers");
        assert_eq!(forwarder_auth_headers.len(), 1);
        assert_eq!(forwarder_auth_headers[0].0, http::header::AUTHORIZATION);
        assert_eq!(
            forwarder_auth_headers[0].1.to_str().expect("header value"),
            "Bearer sk-forwarder-auth"
        );

        let missing_auth = Provider::with_id(
            "missing".to_string(),
            "Missing".to_string(),
            json!({}),
            None,
        );
        assert!(codex_adapter.extract_auth(&missing_auth).is_none());
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
        assert_eq!(
            provider_claude_kind(&anthropic_provider),
            ProviderKind::Claude
        );
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
        assert_eq!(
            provider_gemini_kind(&gemini_provider),
            ProviderKind::GeminiCli
        );
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
        assert!(codex_env.contains(&("OPENAI_API_KEY".to_string(), "codex-token".to_string())));

        let gemini_env = provider_launch_env_vars_for_app(&provider, &AppType::Gemini);
        assert!(gemini_env.contains(&(
            "GOOGLE_GEMINI_BASE_URL".to_string(),
            "https://gemini.example.com".to_string()
        )));
        assert!(gemini_env.contains(&("GEMINI_API_KEY".to_string(), "gemini-key".to_string())));
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
            remove_claude_takeover_env_fields_if_present(&mut claude_live, placeholder, |url| url
                .starts_with("http://localhost")),
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
        assert_eq!(
            claude_env.get("OTHER").and_then(Value::as_str),
            Some("kept")
        );

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
        assert!(codex_preserved_auth_live_config_text_for_policy(
            &codex_config_only,
            placeholder,
            true,
            true,
        )
        .expect("enabled preservation should be valid")
        .is_some());
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
        apply_gemini_takeover_env_fields(&mut gemini_config, "http://127.0.0.1:15721", placeholder);
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
        assert_eq!(
            gemini_env.get("OTHER").and_then(Value::as_str),
            Some("kept")
        );
        assert_eq!(
            remove_gemini_takeover_env_fields_if_present(&mut gemini_config, placeholder, |url| {
                url.starts_with("http://127.0.0.1")
            }),
            Some(true)
        );
        let gemini_env = gemini_config
            .get("env")
            .and_then(Value::as_object)
            .expect("gemini env");
        assert!(gemini_env.get("GOOGLE_GEMINI_BASE_URL").is_none());
        assert!(gemini_env.get("GEMINI_API_KEY").is_none());
        assert_eq!(
            gemini_env.get("OTHER").and_then(Value::as_str),
            Some("kept")
        );

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
            gemini_real_env
                .get("GEMINI_API_KEY")
                .and_then(Value::as_str),
            Some("real-key")
        );

        let mut missing_env = json!({});
        assert_eq!(
            remove_gemini_takeover_env_fields_if_present(&mut missing_env, placeholder, |url| url
                .starts_with("http://127.0.0.1")),
            None
        );
        apply_gemini_takeover_env_fields(&mut missing_env, "http://127.0.0.1:15721", placeholder);
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
                assert!(
                    config_text.contains("model_catalog_json = \"cc-switch-model-catalog.json\"")
                );
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
        assert!(sync_provider_settings_with_live_token(
            &AppType::Gemini,
            &json!({ "env": { "GEMINI_API_KEY": "fresh-gemini" } }),
            &mut gemini_provider,
            placeholder,
        )
        .expect("provider token sync should be valid"));
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
    fn claude_desktop_import_decision_selects_direct_proxy_and_skip() {
        let direct_provider = Provider::with_id(
            "direct".to_string(),
            "Direct".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-direct"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_desktop_import_decision(&direct_provider),
            ClaudeDesktopProviderImportDecision::Direct
        );

        let proxy_provider = Provider::with_id(
            "proxy".to_string(),
            "Proxy".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "kimi-k2"
                }
            }),
            None,
        );
        let ClaudeDesktopProviderImportDecision::Proxy(routes) =
            provider_claude_desktop_import_decision(&proxy_provider)
        else {
            panic!("expected proxy import decision");
        };
        assert_eq!(
            routes.get("claude-sonnet-4-6").expect("sonnet route").model,
            "kimi-k2"
        );

        let skipped_provider = Provider::with_id(
            "skip".to_string(),
            "Skip".to_string(),
            json!({"env": {}}),
            None,
        );
        assert_eq!(
            provider_claude_desktop_import_decision(&skipped_provider),
            ClaudeDesktopProviderImportDecision::Skip
        );
    }

    #[test]
    fn claude_desktop_status_facts_project_mode_url_and_missing_routes() {
        let direct_provider = Provider::with_id(
            "direct".to_string(),
            "Direct".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-direct"
                }
            }),
            None,
        );
        let direct = provider_claude_desktop_status_facts(&direct_provider, || {
            Some("http://127.0.0.1:15721/claude-desktop".to_string())
        });
        assert_eq!(direct.mode, ClaudeDesktopMode::Direct);
        assert_eq!(
            direct.expected_base_url.as_deref(),
            Some("https://api.anthropic.com")
        );
        assert!(!direct.missing_route_mappings);

        let mut proxy_provider =
            Provider::with_id("proxy".to_string(), "Proxy".to_string(), json!({}), None);
        proxy_provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            claude_desktop_model_routes: HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "kimi-k2".to_string(),
                    label_override: None,
                    supports_1m: Some(false),
                },
            )]),
            ..Default::default()
        });
        let proxy = provider_claude_desktop_status_facts(&proxy_provider, || {
            Some("http://127.0.0.1:15721/claude-desktop".to_string())
        });
        assert_eq!(proxy.mode, ClaudeDesktopMode::Proxy);
        assert_eq!(
            proxy.expected_base_url.as_deref(),
            Some("http://127.0.0.1:15721/claude-desktop")
        );
        assert!(!proxy.missing_route_mappings);

        let mut missing_routes = proxy_provider.clone();
        missing_routes
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes
            .clear();
        let missing = provider_claude_desktop_status_facts(&missing_routes, || None);
        assert_eq!(missing.mode, ClaudeDesktopMode::Proxy);
        assert!(missing.expected_base_url.is_none());
        assert!(missing.missing_route_mappings);
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
            Some(
                ClaudeDesktopProxyProviderConfigValidationIssue::ApiFormatUnsupported(
                    "unsupported_wire".to_string()
                )
            )
        );
    }

    #[test]
    fn proxy_channel_dao_adapter_projects_validation_and_legacy_projection() {
        assert_eq!(
            normalize_channel_base_url(" https://api.example.com/v1/ "),
            "https://api.example.com/v1"
        );
        assert_eq!(
            stable_channel_id(
                "Claude",
                "Provider A",
                "legacy_primary",
                "https://api.example.com"
            ),
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
        let inputs =
            claude_desktop_model_routes_to_core_inputs([ClaudeDesktopResolvedProxyRoute {
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
        assert_eq!(CODEX_DEFAULT_MODEL_CONTEXT_WINDOW, 128_000);

        let catalog =
            codex_model_catalog_from_settings(&settings, 128_000, &template).expect("catalog");
        let models = catalog
            .get("models")
            .and_then(Value::as_array)
            .expect("models");
        assert_eq!(
            models[0].get("slug").and_then(Value::as_str),
            Some("kimi-k2")
        );
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

        let provider_catalog = provider_model_catalog_from_settings("provider-a", Some(&settings));
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
            provider_model_catalog_from_settings("provider-a", Some(&provider.settings_config),)
                .models,
            provider_catalog.models
        );
        assert_eq!(
            claude_desktop_provider_from_selection_result(
                Ok(vec!["provider-a".to_string()]),
                |_| Ok(Some(provider.clone()))
            )
            .expect("selected provider")
            .id,
            "provider-a"
        );
        assert!(matches!(
            claude_desktop_provider_from_selection_result(Ok(Vec::new()), |_| Ok(None)),
            Err(ProxyCoreError::Unavailable(message))
                if message == "no available claude desktop provider"
        ));
        assert!(matches!(
            claude_desktop_provider_from_selection_result(
                Err(AppError::Message("router failed".to_string())),
                |_| Ok(None)
            ),
            Err(ProxyCoreError::Internal(message))
                if message == "select claude desktop provider: router failed"
        ));
        let mut live_config = codex_live_settings_with_model_catalog(
            json!({"auth": {}, "config": ""}),
            provider.settings_config.get("modelCatalog").cloned(),
        );
        assert_eq!(
            live_config.get("modelCatalog"),
            settings.get("modelCatalog")
        );
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
        let fallback_model_catalog = provider_without_catalog
            .settings_config
            .get("modelCatalog")
            .cloned()
            .unwrap_or_else(|| json!({ "models": [] }));
        live_config =
            codex_live_settings_with_model_catalog(live_config, Some(fallback_model_catalog));
        assert_eq!(
            live_config.get("modelCatalog"),
            Some(&json!({ "models": [] }))
        );

        let client_catalog =
            crate::proxy_core::api::model_catalog::client_model_catalog_from_optional_raw(
                AppKind::Codex.as_str(),
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
        assert_eq!(
            client_model_catalog_raw_from_text("not json"),
            json!({"models": []})
        );
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
                "provider-a".to_string(),
                "provider-b".to_string(),
            ]))
            .expect("candidate ids"),
            vec!["provider-a".to_string(), "provider-b".to_string()]
        );
        assert_eq!(
            route_candidate_provider_ids_from_selection_result(Err(
                AppError::NoProvidersConfigured
            ))
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
            route_selection_for_forward_result(&plan, Some("ch-b"), "provider-a")
                .channel
                .id,
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
        assert_eq!(
            sourced_update.outbound_model.as_deref(),
            Some("upstream-sonnet")
        );
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
        assert_eq!(
            body.get("model").and_then(Value::as_str),
            Some("upstream-sonnet")
        );
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
        let proxy_error_body = codex_proxy_error_json_from_proxy_error(
            "Relay",
            "model-a",
            "/responses",
            &ProxyError::Timeout("slow".to_string()),
        );
        assert_eq!(proxy_error_body["error"]["code"], "cc_switch_timeout");

        let upstream_error_body = codex_proxy_error_json_from_proxy_error(
            "Relay",
            "model-a",
            "/responses",
            &ProxyError::UpstreamError {
                status: 429,
                body: Some("quota exceeded".to_string()),
            },
        );
        assert_eq!(
            upstream_error_body["error"]["code"],
            "cc_switch_upstream_error"
        );
        assert_eq!(upstream_error_body["error"]["upstream_status"], 429);
        assert!(upstream_error_body["error"]["message"]
            .as_str()
            .expect("upstream error message")
            .contains("quota exceeded"));

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
        let proxy_error_response = codex_proxy_error_response_from_proxy_error(
            "Relay",
            "model-a",
            "/responses",
            &ProxyError::AuthError("bad token".to_string()),
        )
        .expect("codex proxy error response");
        assert_eq!(proxy_error_response.status.as_u16(), 401);

        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::Timeout("slow".to_string())),
            ForwardFailureKind::Timeout(message) if message == "slow"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::ForwardFailed(
                "connection reset".to_string()
            )),
            ForwardFailureKind::ForwardFailed(message) if message == "connection reset"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::AuthError("bad token".to_string())),
            ForwardFailureKind::AuthError(message) if message == "bad token"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::ProviderUnhealthy(
                "half-open".to_string()
            )),
            ForwardFailureKind::RetryableOther(_)
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::DatabaseError(
                "write failed".to_string()
            )),
            ForwardFailureKind::Other(_)
        ));
        match forward_failure_kind_from_proxy_error(&ProxyError::UpstreamError {
            status: 429,
            body: Some(r#"{"error":{"message":"rate limit"}}"#.to_string()),
        }) {
            ForwardFailureKind::Upstream { status, body } => {
                assert_eq!(status, 429);
                assert_eq!(
                    body.as_deref(),
                    Some(r#"{"error":{"message":"rate limit"}}"#)
                );
            }
            other => panic!("expected upstream failure, got {other:?}"),
        }
        assert_eq!(
            forwarder_rectifier_error_message(&ProxyError::UpstreamError {
                status: 400,
                body: Some("invalid thinking signature".to_string()),
            })
            .as_deref(),
            Some("invalid thinking signature")
        );
        assert_eq!(
            forwarder_rectifier_error_message(&ProxyError::UpstreamError {
                status: 400,
                body: None,
            }),
            None
        );
        assert_eq!(
            forwarder_rectifier_error_message(&ProxyError::Timeout("slow".to_string())).as_deref(),
            Some("超时: slow")
        );
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

        let mut desktop_provider = Provider::with_id(
            "desktop-proxy".to_string(),
            "Desktop Proxy".to_string(),
            json!({}),
            None,
        );
        desktop_provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            claude_desktop_model_routes: HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-sonnet".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            )]),
            ..ProviderMeta::default()
        });
        let desktop_projection = apply_forward_request_model_mapping_from_provider(
            &AppType::ClaudeDesktop,
            json!({"model": "claude-sonnet-4-6", "messages": []}),
            &desktop_provider,
        )
        .expect("desktop model mapping");
        assert_eq!(
            desktop_projection.body.get("model").and_then(Value::as_str),
            Some("upstream-sonnet")
        );
        assert!(desktop_projection.log_message.is_none());
        let desktop_error = apply_forward_request_model_mapping_from_provider(
            &AppType::ClaudeDesktop,
            json!({"model": "unknown-route", "messages": []}),
            &desktop_provider,
        )
        .expect_err("unknown desktop route");
        assert!(matches!(
            desktop_error,
            ProxyError::InvalidRequest(message) if message.contains("unknown-route")
        ));

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
            apply_forwarder_media_prevention_from_facts(ForwarderMediaPreventionFacts {
                rectifier_enabled: true,
                request_media_fallback: true,
                request_media_heuristic: false,
                body: &mut image_body,
                provider_settings: &text_only_provider.settings_config,
            }),
            1
        );
        assert_eq!(image_body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn upstream_url_adapter_projects_codex_and_gemini_url_rules() {
        let codex_adapter = crate::proxy::provider::CodexAdapter::new();
        assert_eq!(
            codex_adapter.build_url("https://api.openai.com/v1", "/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );

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
        assert_eq!(
            codex_plan.effective_endpoint,
            "/chat/completions?foo=bar&api-version=old"
        );
        assert_eq!(
            codex_plan.passthrough_query.as_deref(),
            Some("foo=bar&api-version=old")
        );
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
        let mut provider =
            Provider::with_id("claude".to_string(), "Claude".to_string(), json!({}), None);
        provider.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..ProviderMeta::default()
        });
        let claude_adapter = crate::proxy::provider::ClaudeAdapter::new();
        let codex_adapter = crate::proxy::provider::CodexAdapter::new();
        assert_eq!(claude_adapter.name(), "Claude");
        assert_eq!(codex_adapter.name(), "Codex");
        let forwarder_claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        assert_eq!(forwarder_claude_adapter.facts().adapter_name, "Claude");
        let forwarder_fallback_adapter =
            forwarder_provider_adapter_context_for_app(&AppType::Hermes);
        assert_eq!(forwarder_fallback_adapter.facts().adapter_name, "Codex");
        let codex_provider = Provider::with_id(
            "codex".to_string(),
            "Codex".to_string(),
            json!({"base_url": "https://relay.example/v1/"}),
            None,
        );
        assert_eq!(
            codex_adapter
                .extract_base_url(&codex_provider)
                .expect("codex base URL"),
            "https://relay.example/v1"
        );
        let missing_base_url = codex_adapter
            .extract_base_url(&provider)
            .expect_err("missing codex base URL should fail");
        assert!(matches!(
            missing_base_url,
            ProxyError::ConfigError(message) if message == "Codex Provider 缺少 base_url 配置"
        ));
        assert_eq!(provider_claude_api_format(&provider), "openai_chat");
        assert_eq!(
            resolve_forwarder_claude_api_format(&provider, true, Some("OpenAI")),
            "openai_responses"
        );
        assert!(claude_adapter.needs_transform(&provider));
        assert!(!codex_adapter.needs_transform(&provider));
        let passthrough_adapter_body = json!({"model": "gpt-4.1"});
        assert_eq!(
            codex_adapter
                .transform_request(passthrough_adapter_body.clone(), &provider)
                .expect("codex passthrough transform"),
            passthrough_adapter_body
        );
        let passthrough_body = json!({"model": "claude-3-5-sonnet"});
        assert_eq!(
            provider_claude_transform_request_for_api_format(
                passthrough_body.clone(),
                &provider,
                "anthropic",
                None,
                None
            )
            .expect("anthropic passthrough"),
            passthrough_body
        );
        assert!(!claude_api_format_needs_transform("anthropic"));
        assert!(claude_api_format_needs_transform("openai_chat"));
        assert!(claude_api_format_needs_transform("openai_responses"));
        assert!(claude_api_format_needs_transform("gemini_native"));
        assert!(!claude_api_format_needs_transform("unknown"));
    }

    #[test]
    fn claude_streaming_decision_adapter_preserves_codex_oauth_aggregation() {
        let mut codex_provider = Provider::with_id(
            "codex-oauth".to_string(),
            "Codex OAuth".to_string(),
            json!({}),
            None,
        );
        codex_provider.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            ..Default::default()
        });
        let mut sse_headers = HeaderMap::new();
        sse_headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );

        let aggregate_decision = provider_claude_transform_streaming_decision(
            &codex_provider,
            false,
            &sse_headers,
            "openai_responses",
        );
        assert!(!aggregate_decision.use_streaming);
        assert!(aggregate_decision.aggregate_codex_oauth_responses_sse);
        assert!(matches!(
            aggregate_decision.response_sse_aggregation,
            Some(UpstreamSseAggregationKind::Responses)
        ));

        let streaming_decision = provider_claude_transform_streaming_decision(
            &codex_provider,
            true,
            &HeaderMap::new(),
            "openai_responses",
        );
        assert!(streaming_decision.use_streaming);
        assert!(!streaming_decision.aggregate_codex_oauth_responses_sse);
        assert!(streaming_decision.response_sse_aggregation.is_none());

        let plain_provider =
            Provider::with_id("plain".to_string(), "Plain".to_string(), json!({}), None);
        let upstream_sse_decision = provider_claude_transform_streaming_decision(
            &plain_provider,
            false,
            &sse_headers,
            "openai_chat",
        );
        assert!(upstream_sse_decision.use_streaming);
        assert!(!upstream_sse_decision.aggregate_codex_oauth_responses_sse);
        assert!(upstream_sse_decision.response_sse_aggregation.is_none());

        let non_stream_chat_decision = provider_claude_transform_streaming_decision(
            &plain_provider,
            false,
            &HeaderMap::new(),
            "openai_chat",
        );
        assert!(!non_stream_chat_decision.use_streaming);
        assert!(!non_stream_chat_decision.aggregate_codex_oauth_responses_sse);
        assert!(matches!(
            non_stream_chat_decision.response_sse_aggregation,
            Some(UpstreamSseAggregationKind::ChatCompletions)
        ));
    }

    #[test]
    fn codex_chat_streaming_decision_adapter_preserves_sse_fallback() {
        let mut sse_headers = HeaderMap::new();
        sse_headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );

        let header_streaming_decision =
            codex_chat_transform_streaming_decision(false, &sse_headers);
        assert!(header_streaming_decision.use_streaming);
        assert!(header_streaming_decision.response_sse_aggregation.is_none());

        let requested_streaming_decision =
            codex_chat_transform_streaming_decision(true, &HeaderMap::new());
        assert!(requested_streaming_decision.use_streaming);
        assert!(requested_streaming_decision
            .response_sse_aggregation
            .is_none());

        let non_stream_decision = codex_chat_transform_streaming_decision(false, &HeaderMap::new());
        assert!(!non_stream_decision.use_streaming);
        assert!(matches!(
            non_stream_decision.response_sse_aggregation,
            Some(UpstreamSseAggregationKind::ChatCompletions)
        ));
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
            headers
                .get(http::header::HOST)
                .and_then(|value| value.to_str().ok()),
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

        assert!(
            serialize_upstream_request_body(&http::Method::GET, &json!({"model": "x"}))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            serialize_upstream_request_body(&http::Method::POST, &json!({"model": "x"})).unwrap(),
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
        let pricing = ModelPricing::from_strings("3.0", "15.0", "0.3", "3.75").expect("pricing");
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

        let missing_pricing =
            usage_record_to_request_log(&record, "response", None, Decimal::new(1, 0), || {
                "fallback".to_string()
            });
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
            probe.app_type.parse::<AppType>().expect("app type"),
            AppType::Claude
        );
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        assert_eq!(provider.id, "provider-a");
        let missing_provider: ProxyCoreResult<Provider> =
            None.ok_or_else(|| channel_test_provider_not_found_error(&probe));
        let missing_provider = missing_provider.expect_err("missing provider");
        assert!(matches!(
            missing_provider,
            ProxyCoreError::Config(message)
                if message == "provider not found for channel channel-a: provider-a"
        ));
        let invalid_probe = ChannelTestProbeRequest {
            app_type: "unknown-app".to_string(),
            ..probe.clone()
        };
        let invalid_app_type: ProxyCoreResult<AppType> = invalid_probe
            .app_type
            .parse::<AppType>()
            .map_err(channel_test_app_type_error);
        assert!(matches!(
            invalid_app_type,
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
    fn stream_check_adapter_resolves_standard_provider_base_urls() {
        let claude_desktop_provider = Provider::with_id(
            "claude-desktop".to_string(),
            "Claude Desktop".to_string(),
            json!({ "env": { "ANTHROPIC_BASE_URL": "https://claude-relay.example/v1" } }),
            None,
        );
        assert_eq!(
            stream_check_provider_base_url(&AppType::ClaudeDesktop, &claude_desktop_provider)
                .expect("Claude Desktop base URL"),
            "https://claude-relay.example/v1"
        );

        let codex_provider = Provider::with_id(
            "codex".to_string(),
            "Codex".to_string(),
            json!({ "base_url": "https://codex-relay.example/v1/" }),
            None,
        );
        assert_eq!(
            stream_check_provider_base_url(&AppType::Codex, &codex_provider)
                .expect("Codex base URL"),
            "https://codex-relay.example/v1"
        );

        let opencode_provider = Provider::with_id(
            "opencode".to_string(),
            "OpenCode".to_string(),
            json!({
                "npm": "@ai-sdk/anthropic",
                "options": {}
            }),
            None,
        );
        assert_eq!(
            stream_check_provider_base_url(&AppType::OpenCode, &opencode_provider)
                .expect("OpenCode base URL"),
            "https://api.anthropic.com"
        );

        let openclaw_provider = Provider::with_id(
            "openclaw".to_string(),
            "OpenClaw".to_string(),
            json!({ "baseUrl": " https://openclaw.example/v1 " }),
            None,
        );
        assert_eq!(
            stream_check_provider_base_url(&AppType::OpenClaw, &openclaw_provider)
                .expect("OpenClaw base URL"),
            "https://openclaw.example/v1"
        );

        let hermes_provider = Provider::with_id(
            "hermes".to_string(),
            "Hermes".to_string(),
            json!({ "base_url": " https://hermes.example " }),
            None,
        );
        assert_eq!(
            stream_check_provider_base_url(&AppType::Hermes, &hermes_provider)
                .expect("Hermes base URL"),
            "https://hermes.example"
        );

        let missing_opencode = Provider::with_id(
            "opencode-missing".to_string(),
            "OpenCode Missing".to_string(),
            json!({
                "npm": "@ai-sdk/openai-compatible",
                "options": {}
            }),
            None,
        );
        assert!(matches!(
            stream_check_provider_base_url(&AppType::OpenCode, &missing_opencode),
            Err(AppError::Localized { key, .. }) if key == "opencode_base_url_missing"
        ));

        let missing_openclaw = Provider::with_id(
            "openclaw-missing".to_string(),
            "OpenClaw Missing".to_string(),
            json!({}),
            None,
        );
        assert!(matches!(
            stream_check_provider_base_url(&AppType::OpenClaw, &missing_openclaw),
            Err(AppError::Localized { key, .. }) if key == "openclaw_base_url_missing"
        ));

        let missing_hermes = Provider::with_id(
            "hermes-missing".to_string(),
            "Hermes Missing".to_string(),
            json!({}),
            None,
        );
        assert!(matches!(
            stream_check_provider_base_url(&AppType::Hermes, &missing_hermes),
            Err(AppError::Localized { key, .. }) if key == "hermes_base_url_missing"
        ));
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
        let credentials =
            openclaw_credential_parts_from_settings(&credential_provider.settings_config);
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
        assert!(matches!(
            typed_plan.config,
            OpenClawLiveWriteConfig::Typed(_)
        ));

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
            provider_from_openclaw_live_config(
                "empty",
                &OpenClawProviderConfig {
                    models: Vec::new(),
                    ..typed_config.clone()
                }
            ),
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
        let credentials = opencode_credential_parts_from_settings(&provider.settings_config)
            .expect("opencode credentials");
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
        assert!(opencode_live_provider_fragment_has_provider_fields(
            &json!({
                "npm": Value::Null
            })
        ));
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
        assert!(opencode_live_provider_fragment_has_provider_fields(
            &json!({
                "options": {}
            })
        ));
        assert!(!opencode_live_provider_fragment_has_provider_fields(
            &json!({
                "name": "Provider"
            })
        ));
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
        let missing_options_provider = Provider::with_id(
            "missing-options".to_string(),
            "Missing Options".to_string(),
            json!({}),
            None,
        );
        assert!(matches!(
            opencode_credential_parts_from_settings(&missing_options_provider.settings_config),
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
        let claude_credentials =
            provider_credential_values(&claude, &AppType::Claude).expect("claude credentials");
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
        let codex_credentials =
            provider_credential_values(&codex, &AppType::Codex).expect("codex credentials");
        assert_eq!(codex_credentials.api_key, "sk-test");
        assert_eq!(codex_credentials.base_url, "https://codex.example/v1");

        let gemini = Provider::with_id(
            "gemini".to_string(),
            "Gemini".to_string(),
            json!({"env": {"GEMINI_API_KEY": "AIza-test"}}),
            None,
        );
        let gemini_credentials =
            provider_credential_values(&gemini, &AppType::Gemini).expect("gemini credentials");
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
        assert_eq!(missing_base_url_spec.zh, "config.toml 中缺少 base_url 配置");
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
        assert!(core_provider_supports_legacy_common_config_migration(
            &AppKind::from(&AppType::Claude)
        ));
        assert!(!core_provider_supports_legacy_common_config_migration(
            &AppKind::from(&AppType::OpenCode)
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

        let live_takeover_apps = live_takeover_app_types();
        assert_eq!(
            live_takeover_apps,
            [AppType::Claude, AppType::Codex, AppType::Gemini]
        );
        assert_eq!(live_token_sync_app_label(&AppType::Claude), Some("Claude"));
        assert_eq!(live_token_sync_app_label(&AppType::Codex), Some("Codex"));
        assert_eq!(live_token_sync_app_label(&AppType::Gemini), Some("Gemini"));
        assert_eq!(live_token_sync_app_label(&AppType::ClaudeDesktop), None);
        assert_eq!(live_token_sync_app_label(&AppType::OpenCode), None);
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
        assert!(!provider_delete_is_current_provider(
            "provider-a",
            None,
            None
        ));
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
        let reset =
            channel_health_reset_from_parts(reset_plan.channel_id, reset_plan.app_type.as_str());
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
            failure_threshold: None,
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

        let override_update = channel_health_attempt_db_update(ChannelAttemptResult {
            channel_id: "channel-a".to_string(),
            success: false,
            status_code: None,
            latency_ms: None,
            failure_threshold: Some(7),
            error_code: Some("timeout".to_string()),
        });

        assert_eq!(override_update.failure_threshold, 7);
        assert_eq!(override_update.response_time_ms, None);
    }

    #[test]
    fn provider_health_adapter_projects_attempt_db_update() {
        let update = provider_health_attempt_db_update(ProviderAttemptResult {
            provider_id: "provider-a".to_string(),
            app: AppKind::Claude,
            success: false,
            failure_threshold: 3,
            error_message: Some("timeout".to_string()),
        });

        assert_eq!(update.provider_id, "provider-a");
        assert_eq!(update.app_type, "claude");
        assert!(!update.success);
        assert_eq!(update.error_msg.as_deref(), Some("timeout"));
        assert_eq!(update.failure_threshold, 3);
    }

    #[test]
    fn stream_check_proxy_target_ids_adapter_projects_current_and_failover_sources() {
        assert!(stream_check_proxy_target_ids_from_sources(
            false,
            Some("current".to_string()),
            vec!["queued".to_string()],
        )
        .is_none());

        let ids = stream_check_proxy_target_ids_from_sources(
            true,
            Some("current".to_string()),
            vec!["queued".to_string(), "current".to_string()],
        )
        .expect("proxy target filter ids");
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("current"));
        assert!(ids.contains("queued"));

        let empty_ids =
            stream_check_proxy_target_ids_from_sources(true, None, Vec::<String>::new())
                .expect("empty proxy target filter ids");
        assert!(empty_ids.is_empty());
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
        let usage_provider_needs_claude_transform = provider_needs_claude_transform(&provider);
        let usage_provider_is_copilot =
            provider_is_github_copilot_upstream(&provider, "https://example.com");
        let stream_check_provider_is_copilot =
            provider_is_github_copilot_stream_check_target(&provider);
        let copilot_account_id = provider_github_copilot_managed_account_id(&provider);
        let models_are_claude_safe = provider_claude_models_are_claude_safe(&provider);
        let stream_check_timeout_secs =
            provider_stream_check_test_config(&provider).and_then(|config| config.timeout_secs);
        assert_eq!(
            provider_usage_script(Some(&provider))
                .and_then(|script| script.template_type.as_deref()),
            Some("github_copilot")
        );
        assert!(provider_usage_script(None).is_none());
        let usage_provider_is_full_url = provider_is_full_url(&provider);
        let provider_user_agent =
            provider_custom_user_agent_header(&provider, false).expect("custom user agent");
        let copilot_provider_user_agent = provider_custom_user_agent_header(&provider, true);
        let model_fetch_user_agent =
            model_fetch_custom_user_agent_header(Some(" cc-switch-model-fetch/1.0 "))
                .expect("model fetch custom user agent");
        assert!(model_fetch_custom_user_agent_header(Some("   ")).is_none());
        assert!(model_fetch_custom_user_agent_header(Some("bad\nua")).is_none());
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

        let spec = proxy_provider_to_core_spec(&provider, &AppType::Claude);
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
        assert!(usage_provider_needs_claude_transform);
        assert!(usage_provider_is_copilot);
        assert!(stream_check_provider_is_copilot);
        assert_eq!(copilot_account_id.as_deref(), Some("acct-1"));
        assert!(models_are_claude_safe);
        assert_eq!(stream_check_timeout_secs, Some(20));
        assert!(usage_provider_is_full_url);
        assert_eq!(
            provider_user_agent,
            http::HeaderValue::from_static("cc-switch-test/1.0")
        );
        assert!(copilot_provider_user_agent.is_none());
        assert_eq!(
            model_fetch_user_agent,
            http::HeaderValue::from_static("cc-switch-model-fetch/1.0")
        );
        assert!(provider_is_github_copilot_upstream(
            &Provider::with_id("plain".to_string(), "Plain".to_string(), json!({}), None,),
            "https://api.githubcopilot.com"
        ));
        assert!(provider_is_codex_oauth(&codex_provider));
        assert_eq!(
            provider_managed_account_id_for(&codex_provider, "codex_oauth").as_deref(),
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
    fn provider_managed_account_binding_projection_uses_core_policy() {
        let mut legacy_provider = Provider::with_id(
            "legacy-copilot".to_string(),
            "Legacy Copilot".to_string(),
            json!({}),
            None,
        );
        legacy_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            github_account_id: Some("legacy-acct".to_string()),
            ..ProviderMeta::default()
        });

        assert_eq!(
            provider_github_copilot_managed_account_id(&legacy_provider).as_deref(),
            Some("legacy-acct")
        );
        let legacy_context = provider_managed_account_binding_context(&legacy_provider);
        assert_eq!(legacy_context.binding, None);
        assert_eq!(
            legacy_context.legacy_github_copilot_account_id,
            Some("legacy-acct")
        );
        assert_eq!(
            account_ref(&legacy_provider).as_deref(),
            Some("github_copilot:legacy-acct")
        );

        let mut default_account_provider = Provider::with_id(
            "default-copilot".to_string(),
            "Default Copilot".to_string(),
            json!({}),
            None,
        );
        default_account_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            github_account_id: Some("legacy-acct".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("github_copilot".to_string()),
                account_id: None,
            }),
            ..ProviderMeta::default()
        });

        assert_eq!(
            provider_github_copilot_managed_account_id(&default_account_provider),
            None
        );
        let default_context = provider_managed_account_binding_context(&default_account_provider);
        let binding = default_context.binding.expect("managed account binding");
        assert_eq!(binding.source, ManagedAccountBindingSource::ManagedAccount);
        assert_eq!(binding.auth_provider, Some("github_copilot"));
        assert_eq!(binding.account_id, None);
        assert_eq!(
            default_context.legacy_github_copilot_account_id,
            Some("legacy-acct")
        );
        assert_eq!(account_ref(&default_account_provider), None);
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

        let spec = proxy_channel_record_to_core_spec(&channel);
        let source_spec = channel_spec_from_source(Some(channel.clone())).expect("channel spec");
        let route_input = proxy_channel_record_to_route_resolve_channel_input(channel.clone());
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
        assert_eq!(
            materialized_source,
            ChannelRouteSource::MaterializedChannels
        );
        assert_eq!(materialized_channels[0].id, "ch-1");
        assert_eq!(legacy_source, ChannelRouteSource::LegacyProjection);
        assert_eq!(legacy_channels[0].id, "ch-1");
        assert_eq!(route_input.channel_id, "ch-1");
        assert_eq!(route_input.provider_id, "provider-1");
        assert_eq!(route_input.channel_name, "Relay A");
        assert_eq!(route_input.status, "enabled");
        assert_eq!(route_input.interface_kind, "openai_chat_completions");
        assert_eq!(route_input.source_kind, "manual");
        assert_eq!(route_input.models.len(), 1);
        assert_eq!(route_input.models[0].public_model, "sonnet");
        assert_eq!(route_input.models[0].upstream_model, "anthropic/sonnet");
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
