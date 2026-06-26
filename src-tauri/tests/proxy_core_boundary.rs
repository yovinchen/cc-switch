use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED_PROXY_CORE_FILES: &[&str] = &[
    "src/codex_config.rs",
    "src/commands/codex_oauth.rs",
    "src/commands/copilot.rs",
    "src/commands/model_fetch.rs",
    "src/commands/proxy.rs",
    "src/commands/settings.rs",
    "src/database/dao/proxy.rs",
    "src/database/dao/settings.rs",
    "src/lib.rs",
    "src/proxy/codex_oauth_auth.rs",
    "src/proxy/copilot_auth.rs",
    "src/proxy/engine/context.rs",
    "src/proxy/engine/forward_pipeline.rs",
    "src/proxy/engine/response_pipeline.rs",
    "src/proxy/error_mapper.rs",
    "src/proxy/error.rs",
    "src/proxy/events.rs",
    "src/proxy/host/cc_switch/auth_provider.rs",
    "src/proxy/host/cc_switch/channel_auth_profile_attempts.rs",
    "src/proxy/host/cc_switch/channel_health_store.rs",
    "src/proxy/host/cc_switch/channel_key_runtime_source.rs",
    "src/proxy/host/cc_switch/channel_reachability_probe.rs",
    "src/proxy/host/cc_switch/claude_desktop_gateway_auth_source.rs",
    "src/proxy/host/cc_switch/config_source.rs",
    "src/proxy/host/cc_switch/database_channel_source.rs",
    "src/proxy/host/cc_switch/database_usage_sink.rs",
    "src/proxy/host/cc_switch/event_sink.rs",
    "src/proxy/host/cc_switch/forwarder_attempt_runtime_source.rs",
    "src/proxy/host/cc_switch/forwarder_auth_source.rs",
    "src/proxy/host/cc_switch/forward_pipeline.rs",
    "src/proxy/host/cc_switch/forwarder_response_source.rs",
    "src/proxy/host/cc_switch/forwarder_request_source.rs",
    "src/proxy/host/cc_switch/management_auth_source.rs",
    "src/proxy/host/cc_switch/managed_account_runtime_source.rs",
    "src/proxy/host/cc_switch/model_catalog_provider.rs",
    "src/proxy/host/cc_switch/provider_adapter_context.rs",
    "src/proxy/host/cc_switch/provider_router_channel_source.rs",
    "src/proxy/host/cc_switch/provider_router_config_source.rs",
    "src/proxy/host/cc_switch/provider_router_provider_source.rs",
    "src/proxy/host/cc_switch/provider_source.rs",
    "src/proxy/host/cc_switch/proxy_runtime.rs",
    "src/proxy/host/cc_switch/proxy_services.rs",
    "src/proxy/host/cc_switch/provider_router_health_store.rs",
    "src/proxy/host/cc_switch/route_policy_source.rs",
    "src/proxy/host/cc_switch/route_resolver.rs",
    "src/proxy/host/cc_switch/runtime_status_source.rs",
    "src/proxy/response_adapter.rs",
    "src/proxy/transport/upstream/mod.rs",
    "src/proxy/transport/upstream/reqwest_client.rs",
    "src/proxy_core_adapter.rs",
    "src/services/model_fetch_transport.rs",
    "src/services/provider/gemini_auth.rs",
    "src/services/stream_check.rs",
    "src/services/session_usage.rs",
    "src/services/session_usage_codex.rs",
    "src/services/session_usage_gemini.rs",
    "src/services/session_usage_opencode.rs",
    "src/services/usage_stats.rs",
];
const ALLOWED_PROXY_ENGINE_CONSTRUCTOR_FILES: &[&str] = &["src/proxy_core_adapter.rs"];

const FORBIDDEN_MARKERS: &[&str] = &["crate::proxy_core::", "cc_switch_proxy_core::"];
const FORBIDDEN_FORWARDER_SELF_PLANNING_MARKERS: &[&str] = &[
    "RequestForwarder::new(",
    ".forward_with_retry(",
    "build_forward_attempts(",
    "create_forwarder(",
];
const FORBIDDEN_REQUEST_CONTEXT_PROVIDER_PRESELECT_MARKERS: &[&str] = &[
    "provider_router",
    ".select_providers(",
    ".select_provider_ids(",
];
const FORBIDDEN_REQUEST_CONTEXT_PROVIDER_ADAPTER_MARKERS: &[&str] = &[
    "providers::",
    "get_claude_api_format(",
    "AppKind::from(",
    "selected_route.provider",
    "selected_provider_missing_from_source_message(",
    "request_context_route_update_from_proxy_result(",
];
const FORBIDDEN_PROXY_ERROR_MAPPER_FORWARD_FAILURE_PROJECTION_MARKERS: &[&str] = &[
    "ForwardFailureKind",
    "forward_failure_kind_from_proxy_status(",
    "forward_failure_message(",
];
const FORBIDDEN_FORWARDER_URL_PLANNING_MARKERS: &[&str] = &[
    "rewrite_codex_responses_endpoint_to_chat(",
    "rewrite_claude_transform_endpoint(",
    "claude_transform_endpoint_rewrite_input_from_body(",
    "resolve_gemini_native_url(",
    "append_query_to_full_url(",
    "split_endpoint_and_query(",
    "apply_channel_param_overrides_to_url(",
    "is_codex_chat_full_endpoint_base(",
];
const FORBIDDEN_FORWARDER_CLAUDE_PROVIDER_COMPAT_MARKERS: &[&str] = &[
    "crate::claude_desktop_config::",
    "adapter.name() == \"Claude\"",
    "provider_claude_api_format(",
    "claude_api_format_needs_transform(",
    "provider_uses_anthropic_rectifiers(",
    "provider_claude_normalize_anthropic_messages(",
    "provider_claude_transform_request_for_api_format(",
    "super::providers::get_claude_api_format(",
    "super::providers::normalize_anthropic_messages_for_provider(",
    "super::providers::transform_claude_request_for_api_format(",
];
const FORBIDDEN_FORWARDER_CODEX_PROVIDER_COMPAT_MARKERS: &[&str] = &[
    "super::providers::should_convert_codex_responses_to_chat(",
    "provider_should_convert_codex_responses_to_chat(",
    "provider_apply_codex_chat_upstream_model(",
    "provider_codex_chat_reasoning_options(",
    "provider_is_codex_oauth(",
    "super::providers::apply_codex_chat_upstream_model(",
    "super::providers::resolve_codex_chat_reasoning_options(",
];
const FORBIDDEN_FORWARDER_REQUEST_OPTIMIZER_PROVIDER_FACT_MARKERS: &[&str] =
    &["provider_bedrock_env_flag("];
const FORBIDDEN_FORWARDER_REQUEST_HEADER_PROVIDER_FACT_MARKERS: &[&str] =
    &["provider_custom_user_agent_header("];
const FORBIDDEN_FORWARDER_REQUEST_URL_PROVIDER_FACT_MARKERS: &[&str] = &[
    "provider_is_full_url(",
    "provider_is_github_copilot_upstream(",
];
const FORBIDDEN_FORWARDER_REQUEST_MEDIA_PROVIDER_FACT_MARKERS: &[&str] =
    &[" replace_images_for_text_only_provider_model("];
const FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_TRANSFORM_GATE_MARKERS: &[&str] = &[".needs_transform("];
const FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_REQUEST_TRANSFORM_MARKERS: &[&str] =
    &[".transform_request("];
const FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_BASE_URL_MARKERS: &[&str] = &[".extract_base_url("];
const FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_AUTH_INFO_MARKERS: &[&str] = &[".extract_auth("];
const FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_AUTH_HEADER_MARKERS: &[&str] = &[".get_auth_headers("];
const FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_URL_BUILD_MARKERS: &[&str] = &[".build_url("];
const FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_NAME_MARKERS: &[&str] = &[".name()"];
const FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_REGISTRY_MARKERS: &[&str] = &["get_adapter("];
const FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_TRAIT_MARKERS: &[&str] =
    &["ProviderAdapter", "ForwarderAdapterHandle"];
const FORBIDDEN_FORWARDER_CHANNEL_STATUS_MAPPING_MARKERS: &[&str] = &[
    "mapped_channel_response_status(",
    "invalid_mapped_channel_response_status_message(",
    "StatusCode::from_u16(",
];
const FORBIDDEN_FAILOVER_SWITCH_CONFIG_MARKERS: &[&str] =
    &[".get_proxy_config_for_app(", ".enabled"];
const FORBIDDEN_SWITCH_PROXY_PROVIDER_COMMAND_MARKERS: &[&str] = &[
    ".get_provider_by_id(",
    "should_block_proxy_switch_to_provider(",
];
const FORBIDDEN_RESET_CIRCUIT_BREAKER_COMMAND_MARKERS: &[&str] = &[
    ".get_proxy_config_for_app(",
    ".get_current_provider(",
    ".get_failover_queue(",
    ".get_all_providers(",
    "restored_provider_switchback_decision(",
    "FailoverQueuePosition",
];
const FORBIDDEN_SET_AUTO_FAILOVER_COMMAND_MARKERS: &[&str] = &[
    ".get_proxy_config_for_app(",
    ".get_failover_queue(",
    "get_effective_current_provider(",
    "AppType::from_str(",
    "plan_auto_failover_toggle(",
    "AutoFailoverToggleInput",
    "AUTO_FAILOVER_",
];
const FORBIDDEN_PROVIDER_MODULE_CODEX_HISTORY_MARKERS: &[&str] =
    &["codex_chat_history", "providers::codex_chat_history"];
const FORBIDDEN_PROVIDER_MODULE_KIND_FACADE_MARKERS: &[&str] = &[
    "provider_kind_from_app_type_and_config",
    "get_adapter_for_provider_type",
    "ProviderKind",
];
const FORBIDDEN_PROVIDER_MODULE_MANAGED_AUTH_MARKERS: &[&str] = &[
    "copilot_auth",
    "codex_oauth_auth",
    "CopilotAuthManager",
    "CodexOAuthManager",
];
const FORBIDDEN_PROXY_PROVIDER_AUTH_PATH_MARKERS: &[&str] = &[
    "proxy::providers::copilot_auth",
    "proxy::providers::codex_oauth_auth",
    "providers::copilot_auth",
    "providers::codex_oauth_auth",
];
const FORBIDDEN_MODEL_FETCH_ADAPTER_DTO_EXPORT_MARKERS: &[&str] = &[
    "type FetchedModel = crate::proxy_core::api::model_catalog::FetchedModel",
    "pub use crate::proxy_core::api::model_catalog::FetchedModel",
];
const FORBIDDEN_MODEL_FETCH_COMMAND_DTO_IMPORT_MARKERS: &[&str] =
    &["services::model_fetch_transport::FetchedModel"];
const FORBIDDEN_MODEL_FETCH_COMMAND_PROVIDER_DETAIL_MARKERS: &[&str] =
    &["crate::provider::parse_custom_user_agent("];
const FORBIDDEN_COPILOT_MODEL_ADAPTER_DTO_EXPORT_MARKERS: &[&str] = &[
    "type CopilotModel = crate::proxy_core::api::model_catalog::CopilotModel",
    "pub use crate::proxy_core::api::model_catalog::CopilotModel",
];
const FORBIDDEN_CODEX_MODEL_CONTEXT_ADAPTER_CONST_EXPORT_MARKERS: &[&str] = &[
    "DEFAULT_CODEX_MODEL_CONTEXT_WINDOW as CODEX_DEFAULT_MODEL_CONTEXT_WINDOW",
    "CODEX_DEFAULT_MODEL_CONTEXT_WINDOW",
];
const FORBIDDEN_SETTINGS_CONFIG_ADAPTER_DTO_EXPORT_MARKERS: &[&str] = &[
    "type RectifierConfig = crate::proxy_core::api::ports::RectifierConfig",
    "type OptimizerConfig = crate::proxy_core::api::ports::OptimizerConfig",
    "type CopilotOptimizerConfig = crate::proxy_core::api::ports::CopilotOptimizerConfig",
];
const FORBIDDEN_STREAM_CHECK_ADAPTER_DTO_EXPORT_MARKERS: &[&str] = &[
    "type StreamCheckConfig = crate::proxy_core::api::management::StreamCheckConfig",
    "type StreamCheckResult = crate::proxy_core::api::management::StreamCheckResult",
    "pub use crate::proxy_core::api::management::{StreamCheckConfig, StreamCheckResult",
];
const FORBIDDEN_GEMINI_AUTH_ADAPTER_DTO_EXPORT_MARKERS: &[&str] = &[
    "CodexProviderLiveWriteParts, GeminiAuthType",
    "GeminiAuthType, GeminiAuthTypeInput, GeminiEnvParseIssue",
    "pub(crate) type GeminiAuthType = crate::proxy_core::api::ports::GeminiAuthType",
    "pub(crate) use crate::proxy_core::api::ports::GeminiAuthType",
];
const FORBIDDEN_PROXY_MANAGEMENT_ADAPTER_DTO_EXPORT_MARKERS: &[&str] = &[
    "type GlobalProxyConfig = crate::proxy_core::api::ports::GlobalProxyConfig",
    "pub(crate) use crate::proxy_core::api::ports::GlobalProxyConfig",
    "type ProviderHealth = crate::proxy_core::api::ports::ProviderHealth",
    "pub(crate) use crate::proxy_core::api::ports::ProviderHealth",
    "pub(crate) use crate::proxy_core::api::ports::ProviderHealthUpdateInput",
    "type ProviderAttemptResult = crate::proxy_core::api::ports::ProviderAttemptResult",
    "pub(crate) use crate::proxy_core::api::ports::ProviderAttemptResult",
    "ChannelHealthStore,",
    "ProviderHealthStore,",
];
const FORBIDDEN_STREAM_CHECK_PROVIDER_ADAPTER_MARKERS: &[&str] = &[
    "proxy::providers",
    "get_adapter(",
    "ClaudeAdapter",
    "ProviderAdapter",
    ".extract_base_url(",
];
const FORBIDDEN_STREAM_CHECK_COMMAND_PROXY_TARGET_MARKERS: &[&str] = &[
    "HashSet",
    ".get_current_provider(",
    ".get_failover_queue(",
    "ids.insert(",
];
const FORBIDDEN_PROXY_SERVICE_TAKEOVER_STATUS_MARKERS: &[&str] = &[
    ".get_proxy_config_for_app(",
    "proxy_takeover_status_from_parts(",
];
const FORBIDDEN_PROXY_SERVICE_LIVE_TAKEOVER_APP_LIST_MARKERS: &[&str] = &[
    "[AppType::Claude",
    "AppType::Claude, AppType::Codex, AppType::Gemini",
];
const FORBIDDEN_PROXY_SERVICE_OFFICIAL_WARNING_MARKERS: &[&str] = &[
    "get_effective_current_provider(&self.db, &app)",
    "get_provider_by_id(&current_id",
    "should_emit_proxy_official_warning_for_provider(",
    "proxy_official_warning_event_message(",
];
const FORBIDDEN_PROXY_SERVICE_CURRENT_PROVIDER_SOURCE_MARKERS: &[&str] = &[
    "get_effective_current_provider(&self.db, app_type)",
    ".get_provider_by_id(&current_id, app_type.as_str())",
    "当前供应商不存在，无法接管 Live 配置",
];
const FORBIDDEN_PROXY_SERVICE_LIVE_TOKEN_SYNC_SOURCE_MARKERS: &[&str] = &[
    "get_effective_current_provider(&self.db, &AppType::",
    ".get_provider_by_id(&provider_id,",
    ".update_provider_settings_config(",
    "同步 {app_label} Token 到数据库失败",
    "match app_type",
];
const FORBIDDEN_PROXY_SERVICE_TAKEOVER_ENABLED_CONFIG_MARKERS: &[&str] = &[
    ".get_proxy_config_for_app(",
    ".update_proxy_config_for_app(",
    ".enabled =",
    "current_config.enabled",
    "updated_config.enabled",
];
const FORBIDDEN_PROXY_SERVICE_TAKEOVER_BACKUP_SOURCE_MARKERS: &[&str] =
    &[".get_live_backup(", "读取 {app_type_str} 备份失败"];
const FORBIDDEN_PROXY_SERVICE_TAKEOVER_BACKUP_DELETE_MARKERS: &[&str] =
    &[".delete_live_backup(", "删除 {app_type_str} Live 备份失败"];
const FORBIDDEN_PROXY_SERVICE_TAKEOVER_ACTIVE_FLAG_MARKERS: &[&str] = &[
    ".set_live_takeover_active(",
    ".is_live_takeover_active(",
    "检查接管状态失败",
];
const FORBIDDEN_PROXY_SERVICE_TAKEOVER_HEALTH_CLEANUP_MARKERS: &[&str] = &[
    ".clear_provider_health_for_app(",
    "清除 {app_type_str} 健康状态失败",
];
const FORBIDDEN_PROXY_SERVICE_START_TAKEOVER_BACKUP_CLEANUP_MARKERS: &[&str] =
    &[".delete_all_live_backups(", "清理 Live 备份失败"];
const FORBIDDEN_PROXY_SERVICE_START_TAKEOVER_ACTIVE_FLAG_MARKERS: &[&str] =
    &[".set_live_takeover_active(", "设置接管状态失败"];
const FORBIDDEN_PROXY_SERVICE_STOP_RESTORE_ENABLED_CONFIG_MARKERS: &[&str] = &[
    "[\"claude\", \"codex\", \"gemini\"]",
    ".get_proxy_config_for_app(",
    ".update_proxy_config_for_app(",
    ".enabled =",
    "config.enabled",
];
const FORBIDDEN_PROXY_SERVICE_SIMPLE_RESTORE_BACKUP_SOURCE_MARKERS: &[&str] = &[
    ".get_live_backup(",
    "backup.original_config",
    "解析 Claude 备份失败",
    "解析 Codex 备份失败",
    "解析 Gemini 备份失败",
];
const FORBIDDEN_PROXY_SERVICE_FALLBACK_RESTORE_BACKUP_SOURCE_MARKERS: &[&str] = &[
    ".get_live_backup(",
    "backup.original_config",
    "获取 {app_type_str} Live 备份失败",
    "解析 {app_type_str} 备份失败",
];
const FORBIDDEN_PROXY_SERVICE_SSOT_RESTORE_PROVIDER_SOURCE_MARKERS: &[&str] = &[
    "crate::settings::get_effective_current_provider(",
    ".get_all_providers(",
    "provider_settings_have_proxy_placeholder_for_app(",
    "获取 {app_type:?} 当前供应商失败",
    "读取 {app_type:?} 供应商列表失败",
    "当前供应商配置含代理接管占位符",
];
const FORBIDDEN_PROXY_SERVICE_SSOT_RESTORE_LIVE_WRITE_MARKERS: &[&str] = &[
    "write_live_with_common_config(",
    "写入 {app_type:?} Live 配置失败",
];
const FORBIDDEN_PROXY_SERVICE_LIVE_BACKUP_SAVE_MARKERS: &[&str] = &[
    ".save_live_backup(",
    "serde_json::to_string(&backup_value)",
    "序列化 Claude 配置失败",
    "序列化 Codex 配置失败",
    "序列化 Gemini 配置失败",
    "备份 Claude 配置失败",
    "备份 Codex 配置失败",
    "备份 Gemini 配置失败",
];
const FORBIDDEN_PROXY_SERVICE_UPDATE_BACKUP_EXISTING_SOURCE_MARKERS: &[&str] = &[
    ".get_live_backup(",
    "backup.original_config",
    "读取 {app_type} 现有备份失败",
    "解析 {app_type} 现有备份失败",
];
const FORBIDDEN_PROXY_SERVICE_UPDATE_BACKUP_SAVE_MARKERS: &[&str] = &[
    ".save_live_backup(",
    "serde_json::to_string(&effective_settings)",
    "gemini_live_backup_from_effective_settings(",
    "序列化 Claude 配置失败",
    "序列化 Codex 配置失败",
    "序列化 Gemini 配置失败",
    "更新 {app_type} 备份失败",
];
const FORBIDDEN_PROXY_SERVICE_HOT_SWITCH_SOURCE_MARKERS: &[&str] = &[
    ".get_provider_by_id(",
    "should_block_proxy_switch_to_provider(",
    "crate::settings::get_effective_current_provider(",
    ".get_live_backup(",
    ".set_current_provider(",
    "crate::settings::set_current_provider(",
    "读取供应商失败",
    "供应商不存在",
    "读取当前供应商失败",
    "读取 {app_type} 备份失败",
    "更新当前供应商失败",
    "更新本地当前供应商失败",
    "Cannot switch to official provider during proxy takeover",
];
const FORBIDDEN_PROXY_SERVICE_KEEP_STATE_ACTIVE_FLAG_MARKERS: &[&str] = &[
    ".get_proxy_config()",
    ".update_proxy_config(",
    ".live_takeover_active =",
];
const FORBIDDEN_PROXY_SERVICE_STOP_RESTORE_CLEANUP_MARKERS: &[&str] = &[
    ".set_live_takeover_active(",
    ".delete_all_live_backups(",
    ".clear_all_provider_health(",
    "清除接管状态失败",
    "删除备份失败",
    "重置健康状态失败",
];
const FORBIDDEN_PROXY_SERVICE_GLOBAL_PROXY_ENABLED_MARKERS: &[&str] = &[
    ".get_global_proxy_config(",
    ".update_global_proxy_config(",
    ".proxy_enabled",
    "获取全局代理配置失败",
    "更新代理总开关失败",
];
const FORBIDDEN_PROXY_SERVICE_PROXY_CONFIG_SOURCE_MARKERS: &[&str] = &[
    ".get_proxy_config()",
    ".update_proxy_config(",
    ".live_takeover_active =",
    "获取代理配置失败",
    "保存代理配置失败",
    "保存动态代理端口失败",
];
const FORBIDDEN_PROXY_SERVICE_SERVER_FACTORY_MARKERS: &[&str] = &["ProxyServer::new("];
const FORBIDDEN_PROXY_SERVICE_SERVER_TYPE_MARKERS: &[&str] = &["crate::proxy::server::ProxyServer"];
const FORBIDDEN_PROXY_SERVER_RUNTIME_ASSEMBLY_MARKERS: &[&str] = &[
    "provider_router_from_database(",
    "ProxyEventBus::default(",
    "FailoverSwitchManager::new(",
    "ProxyRuntimeStatus::default(",
    "GeminiShadowStore::default(",
    "CodexChatHistoryStore::default(",
    "CcSwitchProxyServices::with_runtime(",
    "CcSwitchProxyRuntime {",
    "ProxyState {",
];
const FORBIDDEN_PROXY_SERVER_HOST_COMPAT_IMPORT_MARKERS: &[&str] =
    &["crate::proxy_core_host::CcSwitchProxyServices"];
const FORBIDDEN_PROXY_SERVER_RUNTIME_STATE_TYPE_MARKERS: &[&str] =
    &["pub struct ProxyState", "impl ProxyState"];
const FORBIDDEN_PROXY_STATE_SERVER_COMPAT_PATH_MARKERS: &[&str] = &[
    "server::ProxyState",
    "server::{ProxyState",
    "pub use crate::proxy_core_adapter::ProxyState",
];
const FORBIDDEN_PROXY_STATE_HOST_RESOURCE_RETENTION_MARKERS: &[&str] = &[
    "app_handle: Option<tauri::AppHandle>",
    "failover_manager: Arc<FailoverSwitchManager>",
];
const FORBIDDEN_PROXY_SERVICE_EFFECTIVE_SETTINGS_SOURCE_MARKERS: &[&str] = &[
    "build_effective_settings_with_common_config(",
    "get_config_snippet(",
    "self.db.as_ref()",
];
const FORBIDDEN_PROXY_SERVICE_LIVE_WRITE_PROVIDER_FACADE_MARKERS: &[&str] =
    &["crate::services::provider::sanitize_claude_settings_for_live("];
const FORBIDDEN_FORWARDER_MANAGED_AUTH_MARKERS: &[&str] = &[
    "crate::proxy::managed_account_auth",
    "CopilotAuthState",
    "CodexOAuthState",
    "CodexOAuthManager",
    "CopilotAuthManager",
    "fetch_models_for_account(",
    "fetch_models().await",
    "get_api_endpoint(",
    "get_default_api_endpoint(",
    "get_model_vendor_for_account(",
    "get_model_vendor(",
    "get_valid_token_for_account(",
    "get_valid_token().await",
    "default_account_id().await",
    "ProviderAuthInfo::new(token",
];
const FORBIDDEN_ADAPTER_MANAGED_AUTH_PLAN_RUNTIME_CALL_MARKERS: &[&str] =
    &["crate::proxy::managed_account_auth"];
const FORBIDDEN_FORWARDER_FAILOVER_SWITCH_MARKERS: &[&str] = &[".try_switch("];
const FORBIDDEN_FORWARDER_RUNTIME_EVENT_SOURCE_MARKERS: &[&str] = &[
    ".status.write()",
    "record_active_connection_acquired_status(",
    "record_active_connection_released_status(",
    "record_forward_request_started_status(",
    "record_forward_success_status(",
    "record_forward_failure_status(",
    "status.current_provider",
    "status.current_provider_id",
    "status.last_error",
    ".current_providers.write()",
    "current_providers.insert(",
    "current_route_target_from_forward_attempt(",
    "request_started_event_message(",
    "attempt_event_message_from_forward_attempt(",
    "route_selected_event_message_from_forward_attempt(",
    ".events.emit(",
];
const FORBIDDEN_FORWARDER_ATTEMPT_RUNTIME_MARKERS: &[&str] = &[
    ".allow_channel_request(",
    ".allow_provider_request(",
    ".record_channel_result(",
    ".record_result(",
    ".release_channel_permit_neutral(",
    ".release_permit_neutral(",
];
const FORBIDDEN_PROXY_CORE_HOST_ERROR_MARKERS: &[&str] = &["ProxyCoreError::"];
const FORBIDDEN_PROXY_CORE_ADAPTER_PROVIDER_COPILOT_MARKERS: &[&str] = &[
    "providers::copilot_auth::COPILOT_",
    "copilot_auth::COPILOT_",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_MODEL_FETCH_FACADE_MARKERS: &[&str] = &[
    "CodexOAuthModelsRequest",
    "OpenAiCompatibleModelsRequest",
    "ModelFetchHttpResponse",
    "CodexOAuthModelsTransport",
    "OpenAiCompatibleModelsTransport",
    "fetch_openai_compatible_models_with_transport",
    "fetch_codex_oauth_models_with_transport",
];
const FORBIDDEN_PROXY_CORE_HOST_USAGE_PROJECTION_MARKERS: &[&str] =
    &["missing_pricing_warning_message"];
const FORBIDDEN_PROXY_CORE_HOST_APP_SUMMARY_PROJECTION_MARKERS: &[&str] =
    &["AppSummaryConfig::new("];
const FORBIDDEN_PROXY_CORE_HOST_CONFIG_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchConfigSource",
    "impl ProxyConfigSource for CcSwitchConfigSource",
    "cc_switch_app_kinds(",
    "proxy_global_config_from_db_source(",
    "proxy_app_config_from_db_source(",
    "app_summary_config_from_db_source(",
    "proxy_runtime_config_from_db_source(",
    ".get_global_proxy_config(",
    ".get_proxy_config_for_app(",
    ".get_proxy_config(",
    ".get_rectifier_config(",
    ".get_optimizer_config(",
    ".get_copilot_optimizer_config(",
    "proxy_global_config_from_config(",
    "proxy_app_config_from_config_source(",
    "app_summary_config_from_config_source(",
    "proxy_runtime_config_from_config_source(",
];
const FORBIDDEN_PROXY_CORE_HOST_PROVIDER_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchProviderSource",
    "impl ProviderSource for CcSwitchProviderSource",
    "provider_specs_from_db_source(",
    "provider_spec_from_db_source(",
    "current_provider_id_from_db_source(",
    "active_route_target_from_runtime_source(",
    "route_candidate_provider_ids_from_router_source(",
    ".get_all_providers(",
    ".get_provider_by_id(",
    ".get_current_provider(",
    ".select_providers(",
    ".select_provider_ids(",
    "current_providers.read(",
    "current_providers.get(",
    "provider_specs_from_source(",
    "provider_spec_from_source(",
    "route_candidate_provider_ids_from_selection_result(",
];
const FORBIDDEN_PROXY_CORE_HOST_CHANNEL_SPEC_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchChannelSource",
    "impl ChannelSource for CcSwitchChannelSource",
    "channel_specs_from_source_lookup(",
    "channel_spec_from_source_lookup(",
    ".list_channels_for_app(",
    ".list_proxy_channels_for_app(",
    ".get_proxy_channel(",
    "channel_specs_from_source(",
    "channel_spec_from_source(",
];
const FORBIDDEN_PROXY_CORE_HOST_CHANNEL_RECORD_SOURCE_MARKERS: &[&str] = &[
    "create_channel_record_from_db_source(",
    "channel_record_from_db_source(",
    "update_channel_record_from_db_source(",
    "delete_channel_record_from_db_source(",
    ".create_proxy_channel(",
    ".get_proxy_channel(",
    ".update_proxy_channel(",
    ".delete_proxy_channel(",
    "proxy_channel_record_to_core(",
];
const FORBIDDEN_PROXY_CORE_HOST_CHANNEL_KEY_MODEL_SOURCE_MARKERS: &[&str] = &[
    "channel_key_records_from_db_source(",
    "upsert_channel_key_record_from_db_source(",
    "update_channel_key_record_from_db_source(",
    "delete_channel_key_record_from_db_source(",
    "channel_model_records_from_db_source(",
    "replace_channel_model_records_from_db_source(",
    ".list_proxy_channel_keys(",
    ".upsert_proxy_channel_key(",
    ".update_proxy_channel_key(",
    ".delete_proxy_channel_key(",
    ".get_proxy_channel(",
    ".list_proxy_channel_models(",
    ".replace_proxy_channel_models(",
    "proxy_channel_key_record_to_core(",
    "proxy_channel_key_records_to_core(",
    "proxy_channel_model_records_to_core(",
];
const FORBIDDEN_PROXY_CORE_HOST_CHANNEL_RECORD_LIST_SOURCE_MARKERS: &[&str] = &[
    "channel_records_from_db_source(",
    "materialized_channel_records_from_db_source(",
    ".list_channels_for_app(",
    ".list_proxy_channels_for_app(",
    ".list_all_proxy_channels(",
    "proxy_channel_records_to_core(",
];
const FORBIDDEN_PROXY_CORE_HOST_CHANNEL_MIGRATION_SOURCE_MARKERS: &[&str] = &[
    "channel_migration_preview_from_db_source(",
    "channel_migration_materialize_from_db_source(",
    ".preview_legacy_proxy_channel_migration(",
    ".materialize_legacy_proxy_channels(",
    "channel_migration_preview_input_from_result(",
    "channel_migration_materialize_input_from_result(",
];
const FORBIDDEN_PROXY_CORE_HOST_ROUTE_POLICY_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchRoutePolicySource",
    "impl RoutePolicySource for CcSwitchRoutePolicySource",
    "route_policy_from_db_source(",
    ".get_failover_queue(",
    "route_policy_from_source(",
    "route_policy_from_failover_queue(",
];
const FORBIDDEN_PROXY_CORE_HOST_ROUTE_RESOLVER_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchRouteResolver",
    "impl RouteResolver for CcSwitchRouteResolver",
    "route_plan_from_request(",
    "management_route_response_from_router_source(",
    ".resolve_channel_route_dry_run(",
    "app_error(\"resolve channel route dry run\"",
    "crate::proxy_core_adapter::route_plan_from_request(",
];
const FORBIDDEN_PROXY_CORE_HOST_HEALTH_STORE_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchHealthStore",
    "impl ChannelHealthStore for CcSwitchHealthStore",
    "record_channel_attempt_in_db_source(",
    "reset_channel_health_with_router_source(",
    ".update_proxy_channel_health_with_threshold(",
    ".get_proxy_channel_app_type(",
    ".reset_channel_breaker(",
    "channel_health_attempt_db_update(",
    "channel_health_reset_plan_from_lookup(",
    "channel_health_reset_from_plan(",
];
const FORBIDDEN_PROXY_CORE_HOST_REACHABILITY_PROBE_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchChannelReachabilityProbe",
    "impl ChannelReachabilityProbe for CcSwitchChannelReachabilityProbe",
    "probe_channel_reachability_from_db_source(",
    "StreamCheckService",
    ".get_provider_by_id(",
    ".get_stream_check_config(",
    "channel_test_app_type_from_probe_request(",
    "channel_test_provider_from_probe_source(",
    "channel_reachability_probe_error(",
    "stream_check_result_to_channel_reachability(",
];
const FORBIDDEN_PROXY_CORE_HOST_MODEL_CATALOG_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchModelCatalogProvider",
    "impl ModelCatalogProvider for CcSwitchModelCatalogProvider",
    "provider_model_catalog_from_db_source(",
    "client_model_catalog_from_app_source(",
    "claude_desktop_model_routes_from_router_source(",
    ".get_provider_by_id(",
    ".select_providers(",
    ".select_provider_ids(",
    "provider_model_catalog_from_provider(",
    "client_model_catalog_from_source(",
    "claude_desktop_provider_from_selection_result(",
    "crate::claude_desktop_config::proxy_model_routes(",
    "claude_desktop_model_routes_to_core_inputs(",
];
const FORBIDDEN_PROXY_CORE_HOST_USAGE_SINK_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchUsageSink",
    "impl UsageSink for CcSwitchUsageSink",
    "record_usage_in_db_source(",
    "UsageLogger::new(",
    "usage_pricing_config_lookup_from_record(",
    ".resolve_pricing_config(",
    "usage_record_pricing_model(",
    ".get_model_pricing(",
    "usage_record_to_request_log(",
    "log_usage_request_projection_warnings(",
    ".log_request(",
    "usage_error(",
];
const FORBIDDEN_PROXY_CORE_HOST_EVENT_SINK_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchEventSink",
    "impl ProxyEventSink for CcSwitchEventSink",
    "emit_proxy_core_event_bus_source(",
    "emit_proxy_core_event(",
    ".emit(",
];
const FORBIDDEN_PROXY_CORE_HOST_AUTH_PROVIDER_SOURCE_MARKERS: &[&str] = &[
    "struct CcSwitchAuthProvider",
    "impl AuthProvider for CcSwitchAuthProvider",
    "auth_info_from_cc_switch_provider_config(",
];
const FORBIDDEN_PROXY_CORE_HOST_SERVICE_CONTAINER_MARKERS: &[&str] = &[
    "struct CcSwitchProxyServices",
    "impl ProxyServices for CcSwitchProxyServices",
    "CcSwitchConfigSource::new(",
    "CcSwitchProviderSource::new(",
    "CcSwitchChannelSource::new(",
    "CcSwitchRoutePolicySource::new(",
    "CcSwitchChannelHealthStore::new(",
    "CcSwitchUsageSink::new(",
];
const FORBIDDEN_PROXY_CORE_HOST_RUNTIME_TRAIT_IMPL_MARKERS: &[&str] = &[
    "impl ProxyServiceRuntimeResources for CcSwitchProxyRuntime",
    "impl HostForwardRuntime for CcSwitchProxyRuntime",
];
const FORBIDDEN_PROXY_CORE_HOST_RUNTIME_TYPE_DEFINITION_MARKERS: &[&str] = &[
    "pub(crate) struct CcSwitchProxyRuntime",
    "pub(crate) type CcSwitchProxyServices =",
];
const FORBIDDEN_PROXY_CORE_HOST_AUTH_PROFILE_DB_MARKERS: &[&str] = &[
    "fn apply_channel_auth_profile_providers(",
    "get_enabled_proxy_channel_key(",
    "channel_key_value_from_record(",
    "channel_key_value_from_runtime_candidate(",
];
const FORBIDDEN_PROXY_CORE_HOST_FORWARD_CURRENT_PROVIDER_MARKERS: &[&str] = &[
    "current_provider_id_from_settings_for_app_type(",
    "forward_current_provider_id_from_source(",
    "forward_current_provider_id_from_db_sources(",
    ".get_current_provider(",
];
const FORBIDDEN_PROXY_CORE_HOST_FORWARD_CONFIG_SOURCE_MARKERS: &[&str] = &[
    "get_proxy_config_for_app(",
    "get_rectifier_config(",
    "get_optimizer_config(",
    "get_copilot_optimizer_config(",
    "forwarder_runtime_config_from_sources(",
    "forwarder_runtime_config_from_db_sources(",
];
const FORBIDDEN_PROXY_CORE_HOST_FORWARD_ATTEMPT_SOURCE_MARKERS: &[&str] = &[
    ".get_all_providers(",
    "host_providers_for_plan(",
    "required_forward_attempts_from_plan(",
    "required_forward_attempts_from_db_sources(",
    "apply_channel_auth_profile_providers_from_db(",
];
const FORBIDDEN_PROXY_CORE_HOST_FORWARD_PIPELINE_RUNTIME_MARKERS: &[&str] = &[
    "struct CcSwitchForwardPipeline",
    "impl ForwardPipeline for CcSwitchForwardPipeline",
    "forward_with_optional_host_runtime(",
    "forwarding_runtime_unavailable_error(",
    ".ok_or_else(",
    ".forward(request, plan)",
    "runtime.forward(",
];
const FORBIDDEN_PROXY_CORE_HOST_FORWARDER_LAUNCH_MARKERS: &[&str] = &[
    "RequestForwarder::new_preplanned(",
    ".forward_with_preplanned_attempts(",
    "forward_runtime_request_from_proxy_request(",
    "forward_proxy_request_with_host_runtime(",
    "forward_with_preplanned_host_runtime(",
    "forward_error_to_core_error(",
];
const FORBIDDEN_PROXY_CORE_HOST_FORWARDER_RUNTIME_RESOURCE_MARKERS: &[&str] = &[
    "        ForwarderRuntimeHostResources {",
    "forwarder_runtime_host_resources_from_runtime(",
];
const FORBIDDEN_PROXY_SERVER_CIRCUIT_RUNTIME_MARKERS: &[&str] = &[
    ".provider_router.update_all_configs(",
    ".provider_router.update_app_configs(",
    ".provider_router.reset_provider_breaker(",
    ".update_all_configs(",
    ".update_app_configs(",
    ".reset_provider_breaker(",
];
const FORBIDDEN_PROXY_SERVER_RUNTIME_STATE_MARKERS: &[&str] = &[
    "emit_proxy_server_started_event_source(",
    "emit_proxy_server_stopped_event_source(",
    "record_proxy_server_listen_port_runtime_source(",
    "record_proxy_server_started_runtime_source(",
    "record_proxy_server_stopped_runtime_source(",
    "proxy_server_info_from_parts(",
    "chrono::Utc::now()",
    "record_proxy_server_started_status(",
    "record_proxy_server_stopped_status(",
    "apply_proxy_runtime_uptime(",
    "apply_proxy_runtime_active_targets(",
    "current_route_target_from_provider(",
    "server_started_event_message(",
    "server_stopped_event_message(",
    ".events.emit(",
    "set_proxy_port(",
    ".status.write()",
    ".status.read()",
    ".start_time.write()",
    ".start_time.read()",
    ".current_providers.write()",
    ".current_providers.read()",
    "current_providers.insert(",
];
const FORBIDDEN_PROXY_SERVER_ROUTE_ASSEMBLY_MARKERS: &[&str] = &[
    "handlers::",
    "DefaultBodyLimit::max(",
    "middleware::from_fn_with_state(",
    "Router::new()",
    ".route(",
    ".route_layer(",
    ".merge(",
    ".with_state(",
];
const FORBIDDEN_PROXY_SERVER_ACCEPT_LOOP_MARKERS: &[&str] = &[
    "listener.accept()",
    "OriginalHeaderCases",
    "preserve_header_case(",
    "serve_connection(",
    "TokioIo::new(",
    "hyper::service::service_fn(",
    "spawn_proxy_http_accept_loop(",
    "record_proxy_server_stopped_runtime_event_source(",
    "tokio::select!",
];
const FORBIDDEN_PROXY_SERVER_STOP_WAIT_MARKERS: &[&str] = &[
    "tokio::time::timeout(",
    "std::time::Duration::from_secs(5)",
    "ProxyError::StopFailed",
    "ProxyError::StopTimeout",
    "log_srv::STOPPED",
    "log_srv::STOP_TIMEOUT",
    "signal_shutdown(",
    "take_server_handle(",
    "await_proxy_http_accept_loop_stop(",
];
const FORBIDDEN_PROXY_SERVER_LISTENER_BIND_MARKERS: &[&str] = &[
    "SocketAddr",
    "bind_proxy_http_listener(",
    "TcpListener::bind(",
    "ProxyError::BindFailed",
    ".local_addr()",
    ".parse()",
];
const FORBIDDEN_PROXY_SERVER_HANDLE_STORAGE_MARKERS: &[&str] = &[
    "Arc<RwLock<Option",
    "oneshot::Sender",
    "oneshot::Receiver",
    "JoinHandle<",
    "ProxyError::AlreadyRunning",
    "ProxyError::NotRunning",
    "ensure_not_running(",
    "proxy_http_shutdown_channel(",
    "store_shutdown_sender(",
    "store_server_handle(",
    "tokio::sync::",
    "tokio::task::JoinHandle",
];
const FORBIDDEN_PROXY_CORE_CONFIG_SOURCE_APP_CATALOG_MARKERS: &[&str] = &[
    "AppKind::Claude",
    "AppKind::ClaudeDesktop",
    "AppKind::Codex",
    "AppKind::Gemini",
    "Ok(vec![",
];
const FORBIDDEN_PROXY_ENGINE_ROUTE_POLICY_RAW_MARKERS: &[&str] =
    &["failoverProviderIds", ".raw.get("];
const FORBIDDEN_PROVIDER_ROUTER_CHANNEL_ROUTE_SOURCE_MARKERS: &[&str] = &[
    "channel_route_source_for_materialized_records(",
    "channel_route_should_load_legacy_projection(",
    ".list_proxy_channels_for_app(",
    ".preview_legacy_proxy_channel_migration(",
];
const FORBIDDEN_PROVIDER_ROUTER_SELECTION_MARKERS: &[&str] = &[
    "ProviderSelectionInput::",
    "provider_selection_candidate_from_failover_lookup(",
    "provider_failover_circuit_lookups(",
    "current_provider_id_from_router_sources(",
    "crate::settings::get_effective_current_provider(",
    ".get_current_provider(",
    ".get_provider_by_id(",
    ".get_all_providers(",
    ".get_failover_queue(",
];
const FORBIDDEN_PROVIDER_ROUTER_FAILOVER_CONFIG_MARKERS: &[&str] = &[
    ".auto_failover_enabled",
    "默认禁用故障转移",
    ".get_proxy_config_for_app(",
];
const FORBIDDEN_PROVIDER_ROUTER_CIRCUIT_CONFIG_MARKERS: &[&str] = &[
    "circuit_breaker_config_from_app_config(",
    "circuit_failure_threshold_from_app_config(",
    "get_proxy_config_for_app(app_type).await.ok()",
    ".get_proxy_config_for_app(",
];
const FORBIDDEN_PROVIDER_ROUTER_CONFIG_SOURCE_ADAPTER_MARKERS: &[&str] = &[
    "db: Arc<Database>",
    "auto_failover_enabled_from_router_db(",
    "circuit_breaker_config_from_router_db(",
    "circuit_failure_threshold_from_router_db(",
];
const FORBIDDEN_PROVIDER_ROUTER_PROVIDER_SOURCE_ADAPTER_MARKERS: &[&str] = &[
    "provider_failover_sources_from_router_db(",
    "select_current_provider_ids_from_router_db_source(",
    ".get_all_providers(",
    ".get_provider_by_id(",
    ".get_failover_queue(",
];
const FORBIDDEN_PROVIDER_ROUTER_ROUTE_REJECTION_MARKERS: &[&str] =
    &["reject_unavailable_channel_ids(", "unavailable_channel_ids"];
const FORBIDDEN_PROVIDER_ROUTER_MANAGEMENT_ROUTE_MARKERS: &[&str] = &[
    "RouteResolveRequest",
    "RouteResolveResponse",
    "resolve_channel_route(",
    "resolve_core_channel_route(",
    "proxy_channel_route_inputs_to_core(",
    "route_candidate_channel_circuit_keys(",
    "apply_route_candidate_circuit_availability(",
    "app_error_from_proxy_core_error(",
];
const FORBIDDEN_PROVIDER_ROUTER_HEALTH_PERSISTENCE_MARKERS: &[&str] = &[
    ".update_provider_health_with_threshold(",
    ".update_proxy_channel_health_with_threshold(",
    ".reset_proxy_channel_health(",
];
const FORBIDDEN_PROVIDER_ROUTER_CONCRETE_SOURCE_MARKERS: &[&str] =
    &["    db: Arc<Database>,", "self.db"];
const FORBIDDEN_PROVIDER_ROUTER_COARSE_SOURCE_MARKERS: &[&str] = &[
    "trait ProviderRouterSource",
    "dyn ProviderRouterSource",
    "with_source(",
];
const FORBIDDEN_PROVIDER_ROUTER_CHANNEL_DAO_MARKERS: &[&str] = &[
    "ProxyChannelRecord",
    "ProxyChannelSourceKind",
    "ProviderRouterChannelRecord",
    "ProviderRouterChannelModelRecord",
    "channel_route_records(",
];
const FORBIDDEN_PROVIDER_ROUTER_CHANNEL_SOURCE_ADAPTER_MARKERS: &[&str] = &[
    "db: Arc<Database>",
    "router_channel_route_inputs_from_db_source(",
];
const FORBIDDEN_PROVIDER_ROUTER_HEALTH_STORE_ADAPTER_MARKERS: &[&str] = &[
    "record_provider_health_result_from_router_db(",
    "record_provider_health_attempt_from_router_db(",
    "record_channel_health_result_from_router_db(",
    ".update_provider_health_with_threshold(",
    ".update_proxy_channel_health_with_threshold(",
];
const FORBIDDEN_PROVIDER_ROUTER_PROVIDER_RECORD_MARKERS: &[&str] = &[
    "use crate::provider::Provider",
    "IndexMap<String, Provider>",
    "Result<Vec<Provider>",
];
const FORBIDDEN_PROVIDER_ROUTER_LIVE_CIRCUIT_MAP_MARKERS: &[&str] = &[
    "HashMap<String, Arc<CircuitBreaker>>",
    "RwLock<HashMap",
    "breakers:",
];
const PROVIDER_ROUTER_DATABASE_CONSTRUCTOR_MARKER: &str = "ProviderRouter::new(";
const FORBIDDEN_HANDLER_PROXY_REQUEST_BRIDGE_MARKERS: &[&str] = &[
    "ProxyRequest::new(",
    "JsonProxyRequestInput",
    "json_proxy_request_from_input(",
    "codex_responses_proxy_request_from_input(",
    "InterfaceKind::AnthropicMessages",
    "InterfaceKind::OpenAiChatCompletions",
    "InterfaceKind::OpenAiResponses",
    "InterfaceKind::GeminiNative",
];
const FORBIDDEN_HANDLER_PROXY_RESULT_RESPONSE_BRIDGE_MARKERS: &[&str] = &[
    ".apply_proxy_result(",
    "claude_api_format_for_proxy_result(",
    "proxy_core_response_to_proxy_response(",
    "proxy_result_to_proxy_response(",
    "claude_proxy_result_to_proxy_response(",
];
const FORBIDDEN_HANDLER_RAW_JSON_BODY_PARSE_MARKERS: &[&str] = &[
    "parse_json_request_body(",
    "parse_json_request_body_or_null(",
    "parse_json_proxy_request_body(",
    "parse_json_proxy_request_body_or_null(",
    "request_body_stream_flag(",
];
const FORBIDDEN_HANDLER_DIRECT_BODY_COLLECTION_MARKERS: &[&str] = &[
    ".collect()",
    "collect_axum_request_body(",
    "request_body_read_error_message(",
];
const FORBIDDEN_HANDLER_PROVIDER_ADAPTER_DECISION_MARKERS: &[&str] = &[
    "get_adapter(",
    ".needs_transform(",
    "super::providers::should_convert_codex_responses_to_chat(",
];
const FORBIDDEN_HANDLER_RESPONSE_BRANCH_GATE_MARKERS: &[&str] = &[
    "provider_needs_claude_transform(",
    "provider_should_convert_codex_responses_to_chat(",
];
const FORBIDDEN_PROTOCOL_HANDLER_ENDPOINT_BRIDGE_MARKERS: &[&str] =
    &["append_query_to_endpoint_path(", "strip_endpoint_prefix("];
const FORBIDDEN_PROTOCOL_HANDLER_CONTEXT_BRIDGE_MARKERS: &[&str] =
    &["RequestContext::new(", ".with_model_from_uri("];
const FORBIDDEN_GEMINI_HANDLER_ORCHESTRATION_MARKERS: &[&str] = &[
    "collect_json_or_null_proxy_request(",
    "gemini_request_context(",
    "endpoint_from_uri(",
    "into_gemini_proxy_request(",
    "dispatch_proxy_request_to_proxy_response(",
    "gemini_passthrough_response_to_axum_response(",
];
const FORBIDDEN_CODEX_HANDLER_DISPATCH_OUTCOME_MARKERS: &[&str] = &[
    "CodexProxyDispatchResponse",
    "dispatch_codex_proxy_request_to_proxy_response(",
    "codex_response_needs_chat_transform(",
    "codex_chat_to_responses_transformed_response_to_axum_response(",
    "openai_chat_passthrough_response_to_axum_response(",
    "codex_passthrough_response_to_axum_response(",
];
const FORBIDDEN_CODEX_HANDLER_ORCHESTRATION_MARKERS: &[&str] = &[
    "collect_json_proxy_request(",
    ".request_context(",
    "endpoint_for_path(",
    "into_codex_chat_proxy_request(",
    "into_codex_responses_proxy_request(",
    "codex_chat_proxy_request_to_axum_response(",
    "codex_responses_proxy_request_to_axum_response(",
];
const FORBIDDEN_CLAUDE_MESSAGES_HANDLER_ORCHESTRATION_MARKERS: &[&str] = &[
    "collect_json_proxy_request(",
    ".request_context(",
    "endpoint_from_request_uri_stripping_prefix(",
    "into_anthropic_messages_proxy_request(",
    "dispatch_claude_proxy_request_to_proxy_response(",
    "claude_response_needs_transform(",
    "claude_transformed_response_to_axum_response(",
    "claude_passthrough_response_to_axum_response(",
];
const FORBIDDEN_PROTOCOL_HANDLER_ROUTE_METADATA_MARKERS: &[&str] = &[
    "AppType::",
    "handle_messages_for_app(",
    "\"Claude\"",
    "\"claude\"",
    "\"Claude Desktop\"",
    "\"claude-desktop\"",
    "\"/claude-desktop\"",
    "\"/responses\"",
    "\"/responses/compact\"",
];
const FORBIDDEN_PROXY_EVENTS_HANDLER_SSE_ORCHESTRATION_MARKERS: &[&str] = &[
    ".events.subscribe(",
    ".subscribe()",
    "connected_event(",
    "lagged_event(",
    "proxy_event_envelope_to_axum_sse_event(",
    "async_stream::stream!",
    "RecvError::Lagged",
    "RecvError::Closed",
    "KeepAlive::new(",
    "Duration::from_secs(",
    "Sse::new(",
];
const FORBIDDEN_MANAGEMENT_READ_HANDLER_ENGINE_MARKERS: &[&str] = &[
    "AppListRequest::new(",
    "ManagementAppPathRequest::from_path(",
    "AppModelCatalogRequest::from_parts(",
    ".proxy_engine()",
    ".app_list_response(",
    ".provider_list_response(",
    ".list_model_catalog_for_request(",
    ".client_model_catalog_response(",
    "AppKind::Codex",
];
const FORBIDDEN_ROUTE_INSPECTION_HANDLER_ENGINE_MARKERS: &[&str] = &[
    "ChannelListRequest::from_query(",
    "AppChannelManagementRequest::from_parts(",
    "GroupListRequest::from_query(",
    "ManagementAppPathRequest::from_path(",
    "RouteResolveManagementRequest::from_body(",
    ".proxy_engine()",
    ".channel_list_response(",
    ".app_channel_response(",
    ".group_list_response(",
    ".current_route_response(",
    ".resolve_route_response(",
];
const FORBIDDEN_CHANNEL_MUTATION_HANDLER_ENGINE_MARKERS: &[&str] = &[
    "ChannelCreateRequest::from_body(",
    "ChannelPathRequest::from_path(",
    "ChannelKeyPathRequest::from_path(",
    ".proxy_engine()",
    ".create_channel_response(",
    ".channel_record_response(",
    ".update_channel_response(",
    ".delete_channel_response(",
    ".channel_keys_response(",
    ".upsert_channel_key_response(",
    ".update_channel_key_response(",
    ".delete_channel_key_response(",
    ".channel_models_response(",
    ".replace_channel_models_response(",
    ".channel_test_response(",
];
const FORBIDDEN_MIGRATION_BREAKER_HANDLER_ENGINE_MARKERS: &[&str] = &[
    "ManagementAppPathRequest::from_path(",
    "ChannelPathRequest::from_path(",
    ".proxy_engine()",
    ".channel_migration_preview_response(",
    ".channel_migration_materialize_response(",
    ".channel_breaker_stats_response(",
    ".reset_channel_health_response(",
];
const FORBIDDEN_STATUS_MODEL_HANDLER_ENGINE_MARKERS: &[&str] = &[
    "HealthCheckRequest::new(",
    "ProxyStatusRequest::new(",
    "chrono::Utc::now()",
    ".proxy_engine()",
    ".proxy_status_response(",
    ".claude_desktop_model_list_response(",
];
const FORBIDDEN_HANDLER_CODEX_HISTORY_RECORD_MARKERS: &[&str] =
    &[".record_response(", "record_responses_sse_stream("];
const FORBIDDEN_PROTOCOL_HANDLER_FORWARD_CORE_ERROR_MARKERS: &[&str] = &[
    "proxy_core_error_to_proxy_error(error)",
    "record_forward_error_usage(",
    "record_forward_core_error_usage(",
    "let engine = state.proxy_engine();",
    "engine.handle(proxy_request).await",
    "codex_proxy_error_to_axum_response(",
];
const FORBIDDEN_PROVIDER_ADAPTER_BASE_URL_ERROR_MARKERS: &[&str] = &[
    "缺少 base_url 配置",
    ".ok_or_else(|| ProxyError::ConfigError(",
];
const FORBIDDEN_PROVIDER_ADAPTER_AUTH_INFO_MARKERS: &[&str] = &[
    "ProviderAuthInfo::new(",
    "ProviderAuthInfo::with_access_token(",
    "GeminiAdapter::new().parse_oauth_credentials(",
    "parse_gemini_oauth_credentials(&key)",
    "pub fn provider_type(",
    "pub fn parse_oauth_credentials(",
];
const FORBIDDEN_PROVIDER_ADAPTER_TEST_FACADE_MARKERS: &[&str] =
    &["pub fn provider_type(", "pub fn parse_oauth_credentials("];
const FORBIDDEN_CODEX_PROVIDER_ADAPTER_TEST_FACADE_MARKERS: &[&str] = &[
    "fn codex_provider_uses_chat_completions(",
    "fn should_convert_codex_responses_to_chat(",
    "fn apply_codex_chat_upstream_model(",
    "fn resolve_codex_chat_reasoning_config(",
    "fn codex_chat_request_model(",
    "fn codex_chat_reasoning_config_from_profile(",
];
const FORBIDDEN_PROVIDER_ADAPTER_AUTH_HEADER_MARKERS: &[&str] = &[
    "build_codex_bearer_auth_headers(",
    "build_gemini_auth_headers(",
    "build_claude_auth_headers(",
    "build_copilot_auth_headers(",
    "ClaudeAuthHeaderKind::",
    "CopilotAuthHeadersInput",
    ".map_err(|error| ProxyError::AuthError(error.to_string()))",
];
const FORBIDDEN_PROVIDER_ADAPTER_URL_BUILD_MARKERS: &[&str] = &[
    "provider_claude_upstream_url(",
    "provider_codex_upstream_url(",
    "provider_gemini_upstream_url(",
    "crate::proxy_core::",
    "cc_switch_proxy_core::",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_PROVIDER_URL_FACADE_MARKERS: &[&str] = &[
    "fn provider_claude_upstream_url(",
    "fn provider_codex_upstream_url(",
    "fn provider_gemini_upstream_url(",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_ROUTER_CHANNEL_DTO_MARKERS: &[&str] = &[
    "ProviderRouterChannelRecord",
    "ProviderRouterChannelModelRecord",
    "proxy_channel_route_inputs_to_core(",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_DTO_TRAIT_FACADE_MARKERS: &[&str] = &[
    "trait ToProxyCoreProviderSpec",
    "trait ToProxyCoreChannelSpec",
    "trait ToProxyCoreModelRoute",
    "trait ToProxyCoreChannelModelRecord",
    "trait ToProxyCoreChannelRecord",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_SMALL_HELPER_FACADE_MARKERS: &[&str] = &[
    "struct JsonProxyRequestInput",
    "struct ParsedJsonProxyBody",
    "struct CodexResponsesProxyRequest",
    "fn parse_json_proxy_request_body(",
    "fn parse_json_proxy_request_body_or_null(",
    "fn json_proxy_request_from_input(",
    "fn codex_responses_proxy_request_from_input(",
    "fn current_provider_id_from_settings_for_app_type(",
    "fn error_message_with_context(",
    "fn request_model_from_body_for_context(",
    "fn request_model_from_gemini_path_for_context(",
    "fn claude_api_format_from_metadata(",
    "fn extract_gemini_model_from_path(",
    "fn validate_claude_desktop_gateway_bearer_header(",
    "fn claude_desktop_gateway_token_error(",
    "fn proxy_event_envelope_to_sse_spec(",
    "fn apply_route_candidate_circuit_availability(",
    "fn route_candidate_channel_circuit_keys(",
    "fn reject_unavailable_channel_ids(",
    "fn channel_route_source_for_materialized_records(",
    "fn channel_route_should_load_legacy_projection(",
    "fn unsupported_app_kind_error_message(",
    "fn unsupported_app_kind_config_error(",
    "fn route_plan_from_request(",
    "fn route_plan_provider_ids(",
    "fn route_plan_provider_match(",
    "fn usage_script_credentials(",
    "fn forwarding_requires_runtime_error_message(",
    "fn forwarding_requires_runtime_error(",
    "fn route_plan_no_matching_host_providers_error_message(",
    "fn route_plan_no_matching_host_providers_error(",
    "fn route_plan_providers_unconfigured_error_message(",
    "fn route_plan_providers_unconfigured_error(",
    "fn route_plan_selections(",
    "fn route_selection_for_forward_result(",
    "fn resolve_channel_route(",
    "fn channel_test_app_type_error(",
    "fn channel_test_provider_not_found_error(",
    "fn channel_test_app_type_from_probe_request(",
    "fn channel_test_provider_from_probe_source(",
    "fn channel_reachability_probe_error(",
    "fn channel_auth_profile_missing_key_error(",
    "fn channel_key_auth_error(",
    "fn channel_key_value_from_runtime_candidate(",
    "fn gemini_env_json_from_map(",
    "fn gemini_env_string_map_from_settings(",
    "fn parse_gemini_env_file(",
    "fn serialize_gemini_env_file(",
    "fn common_config_snippet_issue_message(",
    "fn openclaw_common_config_value_from_settings(",
    "fn opencode_common_config_value_from_settings(",
    "fn common_config_settings_mutation_issue_message(",
    "fn forwarder_no_available_provider_status_message(",
    "fn forwarder_terminal_failure_status_message(",
    "fn forwarder_failure_log_line(",
    "fn forwarder_rectifier_retry_failure_label(",
    "fn apply_proxy_runtime_active_targets(",
    "fn codex_auth_object_value_from_settings(",
    "fn codex_config_text_from_settings(",
    "fn proxy_live_config_owned_by_takeover(",
    "fn proxy_switch_should_hot_switch(",
    "fn proxy_takeover_marked_state_is_reusable(",
    "fn proxy_takeover_should_restore_existing_backup_before_retakeover(",
    "fn codex_restored_live_settings_parts(",
    "fn provider_settings_validation_issue_spec(",
    "fn provider_live_config_presence_error_policy(",
    "fn provider_delete_is_current_provider(",
    "fn is_local_proxy_url(",
    "fn apply_codex_takeover_auth_placeholder_if_present(",
    "fn ensure_codex_takeover_auth_placeholder(",
    "fn remove_codex_takeover_auth_placeholder_if_present(",
    "fn remove_claude_takeover_env_fields_if_present(",
    "fn apply_gemini_takeover_env_fields(",
    "fn remove_gemini_takeover_env_fields_if_present(",
    "fn apply_claude_takeover_fields_with_policy(",
    "fn apply_claude_takeover_fields_with_policy_and_models(",
    "fn channel_route_candidate_from_selection(",
    "fn resolved_channel_attempt_from_candidate(",
    "fn resolved_channel_attempt_from_selection(",
    "fn forward_failure_kind_from_proxy_status(",
    "fn forward_failure_message_from_proxy_status(",
    "fn apply_channel_param_overrides_to_url(",
    "fn resolve_channel_response_status_mapping(",
    "fn should_transition_open_to_half_open(",
    "fn should_close_half_open_after_success(",
    "fn half_open_probe_allow_result(",
    "fn circuit_breaker_failure_decision(",
    "fn codex_proxy_error_code(",
    "fn codex_proxy_error_json(",
    "fn codex_proxy_error_response(",
    "fn codex_tool_context_from_request(",
    "fn normalize_codex_chat_error_body(",
    "fn should_normalize_anthropic_tool_thinking_history(",
    "fn normalize_anthropic_tool_thinking_history(",
    "fn normalize_deepseek_thinking_disabled_strip_effort(",
    "fn provider_uses_anthropic_messages_format(",
    "fn provider_has_mimo_endpoint(",
    "fn is_mimo_identifier(",
    "fn inject_openai_stream_include_usage(",
    "fn resolve_gemini_native_url(",
    "fn claude_api_format_needs_transform(",
    "fn anthropic_beta_header_value(",
    "fn upstream_host_header_from_url(",
    "fn request_body_stream_flag(",
    "fn is_socks_proxy_url(",
    "fn response_headers_log_summary(",
    "fn response_headers_indicate_sse(",
    "fn get_content_encoding(",
    "fn non_streaming_body_timeout_message(",
    "fn streaming_header_timeout_message(",
    "fn streaming_body_first_chunk_timeout_message(",
    "fn streaming_body_ended_before_first_chunk_message(",
    "fn build_proxy_events_connected_payload(",
    "fn build_proxy_events_lagged_payload(",
    "fn build_server_started_event_payload(",
    "fn build_server_stopped_event_payload(",
    "fn streaming_body_first_chunk_read_error_message(",
    "fn is_official_codex_client_user_agent(",
    "fn build_gemini_native_url(",
    "fn proxy_error_http_status_code(",
    "fn proxy_error_response_body(",
    "fn upstream_proxy_error_response_body(",
    "fn proxy_core_error_from_status_kind(",
    "fn selected_provider_missing_from_source_message(",
    "fn selected_provider_not_applied_message(",
    "fn selected_provider_display_name_for_error(",
    "fn unselected_provider_fallback_id(",
    "fn mask_url_for_log(",
    "fn proxy_url_points_to_loopback_port(",
    "fn default_copilot_github_domain(",
    "fn normalize_github_domain(",
    "fn is_copilot_ghes_domain(",
    "fn copilot_composite_account_id(",
    "fn copilot_github_client_id(",
    "fn copilot_github_device_code_url(",
    "fn copilot_github_oauth_token_url(",
    "fn copilot_github_user_url(",
    "fn copilot_token_url(",
    "fn copilot_usage_url(",
    "fn copilot_api_base(",
    "fn provider_metadata_from_input(",
    "fn provider_account_ref(",
    "fn extract_openclaw_stream_check_base_url(",
    "fn extract_hermes_stream_check_base_url(",
    "fn extract_opencode_stream_check_npm(",
    "fn resolve_opencode_stream_check_base_url(",
    "fn additive_stream_check_base_url_missing_error_spec(",
    "fn opencode_live_provider_fragment_has_provider_fields(",
    "fn channel_health_reset_from_parts(",
    "fn channel_health_reset_from_plan(",
    "fn stream_check_result_to_channel_reachability(",
    "fn channel_reachability_status_from_latency(",
    "fn should_retry_channel_reachability_failure(",
    "fn infer_claude_provider_kind(",
    "fn extract_claude_base_url_from_settings(",
    "fn parse_gemini_oauth_credentials(",
    "fn is_gemini_oauth_key_shape(",
    "fn extract_claude_auth_key_from_settings(",
    "fn settings_config_with_channel_auth_key(",
    "fn channel_auth_profile_missing_key_error_message(",
    "fn channel_auth_profile_missing_key_error(",
    "fn build_codex_bearer_auth_headers(",
    "fn build_gemini_auth_headers(",
    "fn build_claude_auth_headers(",
    "fn build_copilot_auth_headers(",
    "fn parse_copilot_models_response_bytes(",
    "fn parse_copilot_usage_response_bytes(",
    "fn copilot_usage_response_endpoint(",
    "fn copilot_api_endpoint_from_usage_or_default(",
    "fn codex_oauth_token_is_expiring_soon(",
    "fn codex_oauth_device_code_expires_in_secs(",
    "fn codex_oauth_device_code_expires_at_ms(",
    "fn codex_oauth_access_token_expires_at_ms(",
    "fn codex_oauth_pending_device_code_is_expired(",
    "fn codex_oauth_poll_interval_secs(",
    "fn codex_oauth_device_poll_status_kind(",
    "fn codex_oauth_device_auth_usercode_url(",
    "fn codex_oauth_device_auth_token_url(",
    "fn codex_oauth_token_url(",
    "fn codex_oauth_device_verification_url(",
    "fn codex_oauth_device_usercode_request_body(",
    "fn codex_oauth_device_auth_token_request_body(",
    "fn codex_oauth_authorization_code_form(",
    "fn codex_oauth_refresh_token_form(",
    "fn codex_oauth_device_code_request_failure(",
    "fn codex_oauth_device_poll_failure(",
    "fn codex_oauth_token_exchange_failure(",
    "fn codex_oauth_refresh_failure(",
    "fn codex_oauth_missing_pending_user_code_message(",
    "fn codex_oauth_missing_refresh_token_message(",
    "fn codex_oauth_missing_account_id_message(",
    "fn codex_oauth_identity_from_token_claims(",
    "fn unsupported_managed_auth_provider_message(",
    "fn ensure_managed_auth_provider(",
    "fn managed_auth_fallback_default_account_id(",
    "fn compare_managed_auth_account_order(",
    "fn managed_auth_account_from_parts(",
    "fn managed_auth_status_from_parts(",
    "fn managed_auth_device_code_response_from_parts(",
    "fn copilot_token_is_expiring_soon(",
    "fn copilot_oauth_poll_error_kind(",
    "fn codex_default_model_context_window(",
    "fn client_model_catalog_from_optional_raw(",
    "fn build_codex_upstream_url(",
    "fn should_convert_codex_responses_endpoint_to_chat(",
    "fn resolve_codex_provider_uses_chat_completions(",
    "fn build_gemini_upstream_url(",
    "fn build_claude_upstream_url(",
    "fn append_utf8_safe(",
    "fn take_sse_block(",
    "fn inspect_codex_chat_history_sse_block(",
    "fn anthropic_to_openai_responses_request(",
    "fn anthropic_to_openai_chat_request(",
    "fn anthropic_request_to_gemini_request_with_shadow(",
    "fn openai_responses_to_anthropic_message(",
    "fn openai_chat_to_anthropic_message(",
    "fn gemini_response_to_anthropic_message(",
    "fn should_preserve_reasoning_content_for_openai_chat(",
    "fn circuit_breaker_config_from_app_config(",
    "fn circuit_breaker_config_from_router_config_result(",
    "fn circuit_failure_threshold_from_app_config(",
    "fn circuit_failure_threshold_from_router_config_result(",
    "fn proxy_takeover_status_from_config_results(",
    "fn provider_circuit_key(",
    "fn channel_circuit_key(",
    "fn provider_circuit_key_prefix(",
    "fn channel_circuit_key_prefix(",
    "fn app_type_from_circuit_key(",
    "fn select_provider_ids(",
    "fn plan_auto_failover_toggle(",
    "fn failover_switch_pending_key(",
    "fn restored_provider_switchback_decision(",
    "fn provider_failover_circuit_lookups(",
    "fn provider_selection_candidate_from_failover_lookup(",
    "fn current_provider_id_from_sources(",
    "fn current_provider_id_option_from_sources(",
    "fn current_provider_db_fallback_required(",
    "fn should_block_proxy_switch_to_provider_category(",
    "fn build_upstream_request_headers(",
    "fn serialize_upstream_request_body(",
    "fn resolve_upstream_request_transport_policy(",
    "fn is_streaming_upstream_request(",
    "fn resolve_upstream_send_policy(",
    "fn proxy_values_point_to_loopback_port(",
    "fn resolve_codex_provider_upstream_model(",
    "fn codex_provider_catalog_model_ids_from_settings(",
    "fn apply_codex_chat_upstream_model_policy(",
    "fn resolve_response_runtime_policy(",
    "fn apply_channel_route_model_override(",
    "fn apply_resolved_channel_model_override(",
    "fn decode_response_body(",
    "fn decompress_body(",
    "fn strip_sse_field(",
    "fn infer_codex_chat_reasoning_profile(",
    "fn normalize_codex_chat_reasoning_profile(",
    "fn extract_gemini_api_key_from_settings(",
    "fn extract_gemini_base_url_from_settings(",
    "fn resolve_claude_api_format_from_settings(",
    "fn is_copilot_prompt_cache_provider(",
    "fn resolve_claude_responses_prompt_cache_key(",
    "fn passthrough_bytes_proxy_response(",
    "fn passthrough_stream_proxy_response(",
    "fn success_usage_record_with_request_id_fallback(",
    "fn error_usage_record_with_request_id_fallback(",
    "fn transformed_response_usage_record_with_request_id_fallback(",
    "fn transformed_streaming_response_usage_record_with_request_id_fallback(",
    "fn streaming_response_usage_record_with_optional_outbound_model(",
    "fn non_streaming_response_usage_record_from_body_with_request_id_fallback(",
    "fn usage_route_context_from_selection(",
    "fn usage_record_with_route_context(",
    "fn is_placeholder_pricing_model(",
    "fn usage_selected_provider_missing_log_message(",
    "fn usage_record_failure_warning_message(",
    "fn usage_record_debug_log_message(",
    "fn usage_logging_enabled_from_config_flag(",
    "fn normalize_required_channel_string(",
    "fn normalize_channel_base_url(",
    "fn normalize_proxy_channel_write_request_fields(",
    "fn normalize_proxy_channel_patch_request_fields(",
    "fn normalize_proxy_channel_model_write_request_fields(",
    "fn normalize_proxy_channel_models_replace_request_fields(",
    "fn normalize_proxy_channel_key_write_request_fields(",
    "fn normalize_proxy_channel_key_patch_request_fields(",
    "fn stable_channel_id(",
    "fn legacy_channel_priority(",
    "fn legacy_provider_config_text_from_settings(",
    "fn legacy_provider_env_from_settings(",
    "fn legacy_provider_codex_catalog_models_from_settings(",
    "fn infer_legacy_channel_interface(",
    "fn build_legacy_channel_projection(",
    "fn codex_settings_have_model_catalog_specs(",
    "fn codex_model_catalog_from_settings(",
    "fn simplify_codex_model_catalog(",
    "fn provider_model_catalog_from_settings(",
    "fn provider_model_catalog_from_provider(",
    "fn provider_model_catalog_raw_value(",
    "fn attach_codex_model_catalog_from_provider(",
    "fn empty_client_model_catalog_raw(",
    "fn client_model_catalog_raw_from_source(",
    "fn client_model_catalog_raw_from_text(",
    "fn forwarder_apply_codex_chat_upstream_model(",
    "fn forwarder_codex_chat_reasoning_options(",
    "fn forwarder_claude_api_format(",
    "fn forwarder_is_full_url_provider(",
    "fn forwarder_is_github_copilot_upstream(",
    "fn forwarder_provider_adapter_name(",
    "fn forwarder_provider_transform_required(",
    "fn forwarder_provider_transform_request(",
    "fn forwarder_provider_base_url(",
    "fn forwarder_provider_auth_info(",
    "fn forwarder_provider_auth_headers(",
    "fn forwarder_provider_upstream_url(",
    "fn forwarder_provider_adapter_for_app(",
    "fn provider_adapter_name_is_claude(",
    "fn forwarder_is_codex_oauth_provider(",
    "fn forwarder_bedrock_env_flag(",
    "fn forwarder_custom_user_agent_header(",
    "fn forwarder_uses_anthropic_rectifiers(",
    "fn forwarder_should_convert_codex_responses_to_chat(",
    "fn forwarder_claude_normalize_anthropic_messages(",
    "fn forwarder_claude_transform_request_for_api_format(",
    "fn apply_provider_model_mapping(",
    "fn claude_takeover_client_model_for_upstream(",
    "fn claude_takeover_default_display_name(",
    "fn proxy_global_config_from_config(",
    "fn app_summary_config_from_config_source(",
    "fn proxy_app_config_from_config_parts(",
    "fn proxy_runtime_config_from_config(",
    "fn proxy_runtime_config_from_config_source(",
    "fn provider_gemini_api_key(",
    "fn provider_gemini_base_url(",
    "fn auth_info_from_profile_ref(",
    "fn channel_auth_profile_missing_provider_warning(",
    "fn channel_auth_profile_resolution(",
    "fn channel_auth_profile_action(",
    "enum ChannelAuthProfileAction",
    "fn channel_auth_profile_provider_application(",
    "enum ChannelAuthProfileProviderApplication",
    "fn model_route_from_input(",
    "fn channel_spec_from_input(",
    "fn channel_model_record_from_input(",
    "fn channel_key_record_from_input(",
    "fn channel_record_from_input(",
    "fn proxy_runtime_status_stopped(",
    "fn record_active_connection_acquired_status(",
    "fn record_active_connection_released_status(",
    "fn record_proxy_server_stopped_status(",
    "fn apply_proxy_runtime_uptime(",
    "fn proxy_server_info_from_parts(",
    "fn proxy_takeover_status_from_parts(",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_DIRECT_ERROR_MARKERS: &[&str] = &[
    "ProxyCoreError::Config(error_message_with_context(",
    "ProxyCoreError::Internal(error_message_with_context(",
    "ProxyCoreError::InvalidRequest(AppError::InvalidInput(",
];
const FORBIDDEN_HTTP_CLIENT_PROXY_URL_VALIDATION_MARKERS: &[&str] = &[
    "url::Url::parse(",
    "[\"http\", \"https\", \"socks5\", \"socks5h\"]",
    "Invalid proxy scheme",
    "Invalid proxy URL '",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_EXPLICIT_PROXY_URL_VALIDATION_MARKERS: &[&str] = &[
    "url::Url::parse(",
    "SUPPORTED_EXPLICIT_PROXY_SCHEMES",
    "Invalid proxy scheme",
    "Invalid proxy URL '",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_RESPONSE_PARSE_LOG_MARKERS: &[&str] = &[
    "enum UpstreamResponseParseFailureLogContext",
    "let body = String::from_utf8_lossy(body)",
    "解析/聚合上游响应失败",
    "解析/聚合 Chat 上游响应失败",
];
const FORBIDDEN_PROVIDER_ENDPOINT_SERVICE_PROJECTION_MARKERS: &[&str] = &[
    ".custom_endpoints",
    "trim().trim_end_matches('/')",
    "std::cmp::Reverse(",
    ".last_used = Some(",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_CUSTOM_ENDPOINT_URL_POLICY_MARKERS: &[&str] = &[
    "trim().trim_end_matches('/')",
    "\"provider.endpoint.url_required\"",
    "\"URL 不能为空\"",
    "\"URL cannot be empty\"",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_CREDENTIAL_POLICY_MARKERS: &[&str] = &[
    "Regex::new(",
    "base_url\\s*=",
    "config_toml.contains(\"base_url\")",
    "CodexBaseUrlMissing",
    "CodexBaseUrlInvalid",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_BASE_URL_POLICY_MARKERS: &[&str] = &[
    "settings_config.get(\"base_url\")",
    "settings_config.get(\"baseURL\")",
    "provider.settings_config.get(\"config\")",
    "trim_end_matches('/')",
    "base_url = \\\"",
    "base_url = '",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_CONFIG_TOML_POLICY_MARKERS: &[&str] = &[
    "fn codex_wire_api_from_toml(",
    "fn codex_model_from_toml(",
    "fn extract_codex_wire_api(",
    "fn extract_codex_model(",
    "fn codex_config_has_base_url_matching(",
    "parse::<toml::Value>()",
    ".get(\"model_provider\")",
    ".get(\"wire_api\")",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_LIVE_SETTINGS_SHAPE_MARKERS: &[&str] = &[
    "pub(crate) struct CodexLiveSettingsParts",
    "pub(crate) struct CodexProviderLiveWriteParts",
    "pub(crate) struct CodexRestoredLiveSettingsParts",
    "pub(crate) struct CodexLiveSnapshotParts",
    "pub(crate) struct CodexProviderValidationParts",
    "pub(crate) enum CodexProviderLiveWriteIssue",
    "pub(crate) enum CodexLiveSettingsIssue",
    "pub(crate) enum CodexLiveSnapshotIssue",
    "pub(crate) enum CodexProviderValidationIssue",
    ".ok_or(CodexProviderLiveWriteIssue::MissingAuth)",
    ".ok_or(CodexLiveSettingsIssue::NotObject)",
    ".ok_or(CodexLiveSettingsIssue::MissingAuth)",
    ".ok_or(CodexLiveSnapshotIssue::NotObject)",
    ".ok_or(CodexLiveSnapshotIssue::MissingAuth)",
    ".ok_or(CodexProviderValidationIssue::NotObject)",
    ".ok_or(CodexProviderValidationIssue::MissingAuth)",
    "CodexProviderValidationIssue::ConfigInvalidType);",
    "crate::codex_config::codex_auth_has_oauth_login_material",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_DEFAULT_LIVE_IMPORT_CATEGORY_MARKERS: &[&str] = &[
    "crate::codex_config::extract_codex_api_key(",
    "crate::codex_config::codex_auth_has_login_material",
    "has_login_material && !has_provider_key",
    "Some(\"official\")",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_BACKFILL_POLICY_MARKERS: &[&str] = &[
    "pub(crate) struct CodexProviderBackfillParts",
    "crate::codex_config::should_restore_codex_provider_token_for_backfill(",
    "strip_unified_session_bucket: provider.category.as_deref() == Some(\"official\")",
    "restore_provider_token:",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_PROVIDER_SETTINGS_VALIDATION_MARKERS: &[&str] = &[
    "pub(crate) struct ProviderSettingsValidationParts",
    "pub(crate) enum ProviderSettingsValidationIssue",
    "ProviderSettingsValidationIssue::ClaudeSettingsNotObject => LocalizedErrorSpec::new",
    "ProviderSettingsValidationIssue::OpenCodeSettingsNotObject => LocalizedErrorSpec::new",
    "ProviderSettingsValidationIssue::OpenClawSettingsNotObject => LocalizedErrorSpec::new",
    "ProviderSettingsValidationIssue::HermesSettingsNotObject => LocalizedErrorSpec::new",
    "provider_settings_config_is_object(provider)",
    ".map_err(ProviderSettingsValidationIssue::Codex)",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_REQUIRED_BASE_URL_POLICY_MARKERS: &[&str] = &[
    "fn missing_provider_base_url_message",
    "Provider 缺少 base_url 配置",
    ".ok_or_else(",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_GEMINI_LIVE_JSON_POLICY_MARKERS: &[&str] = &[
    "pub(crate) fn gemini_env_value_from_env_json",
    "pub(crate) fn gemini_live_settings_from_env_json_and_config",
    "pub(crate) fn gemini_live_backup_from_effective_settings",
    "\"env\": gemini_env_value_from_env_json",
    "\"env\": settings.get(\"env\")",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_GEMINI_LIVE_CONFIG_POLICY_MARKERS: &[&str] = &[
    "pub(crate) enum GeminiLiveConfigIssue",
    "Some(config) if config.is_object()",
    "Some(config) if config.is_null()",
    "GeminiLiveConfigIssue::InvalidType",
    "merged.as_object_mut()",
    "merged_obj.insert(",
];
const FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_TRANSFORM_DECISION_MARKERS: &[&str] = &[
    "ProviderKind::GitHubCopilot",
    "ProviderKind::CodexOAuth",
    "claude_api_format_needs_transform(",
    "self.get_api_format(provider)",
];
const FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_REQUEST_TRANSFORM_MARKERS: &[&str] = &[
    "anthropic_to_openai_responses_request(",
    "anthropic_to_openai_chat_request(",
    "anthropic_request_to_gemini_request_with_shadow(",
    "provider_claude_responses_prompt_cache_key(",
    "provider_codex_fast_mode_enabled(",
    "provider_should_preserve_reasoning_content_for_openai_chat(",
    "provider_claude_prompt_cache_key(",
    "inject_openai_stream_include_usage(",
    "provider_is_codex_oauth(",
    "match api_format",
];
const FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_COMPAT_FACADE_MARKERS: &[&str] = &[
    "fn get_claude_api_format(",
    "fn normalize_anthropic_messages_for_provider(",
];
const FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_NORMALIZE_MARKERS: &[&str] = &[
    "provider_should_normalize_anthropic_tool_thinking_history(",
    "normalize_anthropic_tool_thinking_history(",
    "provider_normalize_deepseek_thinking_disabled_strip_effort(",
    "api_format.trim()",
];
const FORBIDDEN_HANDLER_MANAGEMENT_AUTH_DECISION_MARKERS: &[&str] = &[
    ".proxy_engine()",
    ".validate_management_auth(",
    "state.config",
    ".config.read()",
    "std::env::var(",
    "CC_SWITCH_PROXY_MANAGEMENT_TOKEN",
    "resolve_management_auth_decision(",
    "validate_management_bearer_header(",
];
const FORBIDDEN_RESPONSE_PIPELINE_USAGE_RECORD_HELPER_MARKERS: &[&str] = &[
    "fn create_usage_collector(",
    "SseUsageCollector::new(",
    "ctx.provider()?",
    "streaming_response_usage_record_from_provider_facts(",
    " streaming_response_usage_record_from_response_context(",
    "non_streaming_response_usage_record_from_response_context(",
    "NonStreamingResponseUsageContext",
    "usage_record_with_route_context(",
    "missing_usage_log_message(",
    "output.log_event(",
    ".usage_sink()",
    "usage_record_debug_log_message(",
    "usage_record_failure_warning_message(",
    "UsageRecordFailureLogContext::UsageRecord",
    "UsageSelectedProviderMissingPhase::StreamingPassthrough",
    "usage logging 已关闭，跳过非流式 usage 解析",
    "usage_logging_enabled_from_config_flag(",
    ".try_read()",
    ".enable_logging",
    "spawn_usage_record_with_proxy_services(",
    "fn spawn_record_usage(",
    "tokio::spawn(async move",
    "streaming_response_usage_record_with_optional_outbound_model(",
    "non_streaming_response_usage_record_from_body_with_request_id_fallback(",
    "non_streaming_response_usage_record_from_provider_body_with_request_id_fallback(",
];
const FORBIDDEN_RESPONSE_PIPELINE_LOGGED_STREAM_POLICY_MARKERS: &[&str] = &[
    "SseEventScanner",
    "SsePassthroughEventKind",
    "strip_sse_field(",
    "take_sse_block(",
    "response_usage_provider_facts_from_optional(",
    "spawn_usage_record_with_proxy_services(",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_SSE_PASSTHROUGH_POLICY_MARKERS: &[&str] = &[
    "SseEventScanner",
    "SsePassthroughEventKind",
    "push_passthrough_bytes(",
    "已接收上游流式首包",
];
const FORBIDDEN_RESPONSE_PROCESSOR_BODY_DECODE_PROJECTION_MARKERS: &[&str] =
    &["read_decoded_body(", "已接收上游响应体"];
const FORBIDDEN_RESPONSE_PROCESSOR_RESPONSE_LOG_PROJECTION_MARKERS: &[&str] = &[
    "response_headers_log_summary(",
    "get_content_encoding(",
    "已接收上游流式响应",
    "流式响应含 content-encoding",
    "上游响应体内容",
    "String::from_utf8_lossy(",
];
const FORBIDDEN_RESPONSE_PROCESSOR_RESPONSE_LOG_CALL_MARKERS: &[&str] = &[];
const FORBIDDEN_RESPONSE_BUILD_CONTEXT_LITERAL_MARKERS: &[&str] = &[
    "Failed to build response",
    "Failed to build streaming response",
    "构建流式响应失败",
    "构建响应失败",
    "构建 SSE 响应失败",
    "构建 Responses 响应失败",
    "构建 Responses 错误响应失败",
    "构建代理错误响应失败",
];
const FORBIDDEN_RESPONSE_ADAPTER_BUILD_ERROR_MESSAGE_MARKERS: &[&str] = &[
    "Failed to build response",
    "Failed to build streaming response",
    "proxy_core_response_to_axum_response_with_error_message",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_RESPONSE_BUILD_CONTEXT_MARKERS: &[&str] = &[
    "enum AxumResponseBuildErrorContext",
    "impl AxumResponseBuildErrorContext",
    "enum CoreResponseBuildFailureContext",
    "impl CoreResponseBuildFailureContext",
    "构建流式响应失败",
    "构建响应失败",
    "构建 SSE 响应失败",
    "构建 Responses 响应失败",
    "构建 Responses 错误响应失败",
    "构建代理错误响应失败",
    "构造 JSON 响应失败",
    "构造 Responses 响应失败",
    "构造 Responses 错误体失败",
    "构造代理错误响应失败",
];
const FORBIDDEN_ERROR_MAPPER_RESPONSE_BUILD_FAILURE_CONTEXT_MARKERS: &[&str] = &[
    "enum CoreResponseBuildFailureContext",
    "impl CoreResponseBuildFailureContext",
    "构造 JSON 响应失败",
    "构造 Responses 响应失败",
    "构造 Responses 错误体失败",
    "构造代理错误响应失败",
];
const FORBIDDEN_ERROR_MAPPER_RESPONSE_TRANSFORM_FAILURE_CONTEXT_MARKERS: &[&str] = &[
    "enum ResponseTransformFailureContext",
    "impl ResponseTransformFailureContext",
    "转换响应失败",
    "Chat → Responses 响应转换失败",
];
const FORBIDDEN_HANDLER_RESPONSE_PARSE_FAILURE_LOG_PROJECTION_MARKERS: &[&str] = &[
    "parse_claude_transform_upstream_json_or_unlabeled_sse(",
    "parse_codex_chat_upstream_json_or_unlabeled_sse(",
    "parse_upstream_json_or_unlabeled_sse(",
    "response_body_parse_error_to_proxy_error(",
    "upstream_response_parse_failure_log_message(",
    "log_unlabeled_sse_fallback_event(",
    "parse_logged_upstream_json_or_unlabeled_sse(",
    "UpstreamResponseParseFailureLogContext::",
    "UnlabeledSseFallbackLogContext::",
    "Failed to parse upstream response",
    "Failed to parse upstream chat response",
    "String::from_utf8_lossy(",
    "解析/聚合上游响应失败",
    "解析/聚合 Chat 上游响应失败",
    ".unlabeled_sse_fallback_log_event(",
    "UnlabeledSseFallbackLogLevel::",
    ".non_json_body_log_message(",
    "normalize_codex_chat_error_body(",
    "log_codex_chat_error_normalization(",
];
const FORBIDDEN_HANDLER_RESPONSE_BUILD_ERROR_MAPPING_MARKERS: &[&str] = &[
    "rebuilt_json_proxy_response(",
    "transformed_sse_proxy_response(",
    "codex_chat_error_proxy_response(",
    "codex_chat_error_response_to_axum_response(",
    "codex_proxy_error_response(",
    "proxy_core_response_to_axum_response(",
    "build_codex_proxy_error_response(",
    "handle_codex_chat_error_response(",
    "CoreResponseBuildFailureContext::CodexResponsesError",
    "CoreResponseBuildFailureContext::CodexProxyError",
    "AxumResponseBuildErrorContext::CodexResponsesError",
    "AxumResponseBuildErrorContext::CodexProxyError",
    "response_build_error_to_proxy_error(",
    "构造 JSON 响应失败",
    "构造 Responses 响应失败",
    "构造 Responses 错误体失败",
    "构造代理错误响应失败",
];
const FORBIDDEN_HANDLER_RESPONSE_TRANSFORM_ERROR_MAPPING_MARKERS: &[&str] = &[
    "claude_response_transform_error_to_proxy_error(",
    "codex_chat_to_responses_transform_error_to_proxy_error(",
    "转换响应失败",
    "Chat → Responses 响应转换失败",
    "ResponseTransformFailureContext::",
    "response_transform_error_to_proxy_error(",
];
const FORBIDDEN_HANDLER_TRANSFORMED_USAGE_POLICY_MARKERS: &[&str] = &[
    "TransformedResponseUsageFormat::",
    "claude_stream_usage_event_filter",
    "codex_stream_usage_event_filter",
    "claude_transformed_streaming_usage_collector(",
    "codex_auto_transformed_streaming_usage_collector(",
    "create_logged_passthrough_stream(",
    "create_claude_transformed_logged_stream(",
    "create_codex_auto_transformed_logged_stream(",
    "ctx.streaming_timeout_config()",
    "record_claude_transformed_response_usage(",
    "record_codex_auto_transformed_response_usage(",
    " record_transformed_response_usage(",
    " transformed_streaming_usage_collector(",
];
const FORBIDDEN_HANDLER_TRANSFORMED_RESPONSE_BUILD_CONTEXT_MARKERS: &[&str] = &[
    "CoreResponseBuildFailureContext::ClaudeJson",
    "CoreResponseBuildFailureContext::CodexResponses",
    "AxumResponseBuildErrorContext::ClaudeSse",
    "AxumResponseBuildErrorContext::CodexSse",
    "AxumResponseBuildErrorContext::ClaudeResponse",
    "AxumResponseBuildErrorContext::CodexResponses",
    "rebuilt_json_proxy_response_to_axum_response(",
    "transformed_sse_proxy_response_to_axum_response(",
];
const FORBIDDEN_HANDLER_CLAUDE_RESPONSE_TRANSFORM_DISPATCH_MARKERS: &[&str] = &[
    "extract_anthropic_tool_schema_hints(",
    "openai_responses_to_anthropic_message(",
    "openai_chat_to_anthropic_message(",
    "gemini_response_to_anthropic_message_with_shadow(",
    "create_anthropic_sse_stream",
    "create_openai_chat_to_anthropic_sse_stream(",
    "create_openai_responses_to_anthropic_sse_stream(",
    "create_gemini_to_anthropic_sse_stream_with_callbacks(",
    "provider_claude_transform_response_for_api_format(",
    "provider_claude_transform_sse_for_api_format(",
    "Rectified tool args",
    "rectified_tool_names",
];
const FORBIDDEN_HANDLER_CLAUDE_STREAMING_DECISION_MARKERS: &[&str] = &[
    "provider_is_codex_oauth(",
    "should_aggregate_codex_oauth_responses_sse(",
    "should_use_claude_transform_streaming(",
    "response_headers_indicate_sse(response.headers())",
    "claude_transform_unlabeled_sse_aggregation(",
    "Some(UpstreamSseAggregationKind::Responses)",
];
const FORBIDDEN_HANDLER_CODEX_NON_STREAM_TRANSFORM_MARKERS: &[&str] = &[
    "chat_completion_to_response_with_context(",
    "codex_tool_context_from_request(",
    "record_codex_chat_response_history(",
    "transform_codex_chat_response_with_history(",
];
const FORBIDDEN_HANDLER_CODEX_STREAM_TRANSFORM_MARKERS: &[&str] = &[
    "create_responses_sse_stream_from_chat_with_context(",
    "record_codex_chat_response_sse_history(",
    "transform_codex_chat_sse_with_history(",
];
const FORBIDDEN_HANDLER_CODEX_STREAMING_DECISION_MARKERS: &[&str] = &[
    "response_headers_indicate_sse(response.headers())",
    "Some(UpstreamSseAggregationKind::ChatCompletions)",
];
const FORBIDDEN_HANDLER_TRANSFORM_STREAMING_DECISION_CALL_MARKERS: &[&str] = &[
    "provider_claude_transform_streaming_decision(",
    "codex_chat_transform_streaming_decision(",
];
const FORBIDDEN_HANDLER_TRANSFORM_RESPONSE_ORCHESTRATION_MARKERS: &[&str] = &[
    "async fn handle_claude_transform(",
    "async fn handle_codex_chat_to_responses_transform(",
    "claude_transformed_sse_stream_from_context(",
    "codex_auto_transformed_sse_stream_from_context(",
    "ClaudeTransformedSseStreamContext",
    "CodexAutoTransformedSseStreamContext",
    "claude_transformed_sse_response_to_axum_response(",
    "codex_transformed_sse_response_to_axum_response(",
    "claude_transformed_upstream_json_response_to_axum_response(",
    "codex_transformed_upstream_json_response_to_axum_response(",
];
const FORBIDDEN_HANDLER_PASSTHROUGH_RESPONSE_PROCESSING_MARKERS: &[&str] = &[
    "engine::response_pipeline::process_response",
    "process_response(",
    "CLAUDE_PARSER_CONFIG",
    "CODEX_PARSER_CONFIG",
    "GEMINI_PARSER_CONFIG",
    "OPENAI_PARSER_CONFIG",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_CLAUDE_STREAMING_DECISION_MARKERS: &[&str] = &[
    "should_aggregate_codex_oauth_responses_sse(",
    "should_use_claude_transform_streaming(",
    "response_headers_indicate_sse(response_headers)",
    "claude_transform_unlabeled_sse_aggregation(",
    "Some(UpstreamSseAggregationKind::Responses)",
];
const PROXY_CORE_MARKER: &str = "crate::proxy_core::";
const PROXY_CORE_API_MARKER: &str = "crate::proxy_core::api";
const PROXY_ENGINE_CONSTRUCTOR_MARKER: &str = "ProxyEngine::new(";

#[test]
fn host_code_uses_proxy_core_through_adapter_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut rust_files = Vec::new();
    collect_rust_files(&manifest_dir.join("src"), &mut rust_files);

    let mut violations = Vec::new();
    for path in rust_files {
        let relative = path
            .strip_prefix(&manifest_dir)
            .expect("source path under manifest dir")
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED_PROXY_CORE_FILES.contains(&relative.as_str()) {
            continue;
        }

        let source = fs::read_to_string(&path).expect("read host source file");
        for (line_index, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{}:{} contains direct proxy-core marker `{}`",
                        relative,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "host code must access proxy-core through approved host adapter files:\n{}",
        violations.join("\n")
    );
}

#[test]
fn session_usage_services_direct_core_access_stays_in_usage_api() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/services/session_usage.rs",
        "src/services/session_usage_codex.rs",
        "src/services/session_usage_gemini.rs",
        "src/services/session_usage_opencode.rs",
        "src/services/usage_stats.rs",
    ];
    let adapter_source = fs::read_to_string(manifest_dir.join("src/proxy_core_adapter.rs"))
        .expect("read proxy_core_adapter.rs");
    let adapter_production = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);

    let mut violations = Vec::new();
    for relative in files {
        let source = fs::read_to_string(manifest_dir.join(relative)).expect("read service file");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            for (column, _) in code.match_indices(PROXY_CORE_MARKER) {
                if !code[column..].starts_with("crate::proxy_core::api::usage") {
                    violations.push(format!(
                        "{}:{} contains non-usage direct proxy-core access: {}",
                        relative,
                        line_index + 1,
                        code.trim()
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "session usage services may only consume proxy_core::api::usage directly:\n{}",
        violations.join("\n")
    );

    assert!(
        !adapter_production.contains("type CostCalculator")
            && !adapter_production.contains("SESSION_REQUEST_ID_PREFIX"),
        "proxy_core_adapter should not re-export session usage cost/request-id contracts"
    );
}

#[test]
fn proxy_core_crate_manifest_remains_host_neutral() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = manifest_dir.join("crates/proxy-core/Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("read proxy-core Cargo.toml");
    let dependency_names = dependency_names_from_manifest(&manifest);
    let forbidden_dependencies = [
        "cc-switch",
        "rusqlite",
        "sqlite",
        "sqlx",
        "tauri",
        "tauri-build",
    ];
    let violations: Vec<String> = dependency_names
        .into_iter()
        .filter(|name| forbidden_dependencies.contains(&name.as_str()))
        .map(|name| format!("crates/proxy-core/Cargo.toml depends on host crate `{name}`"))
        .collect();

    assert!(
        manifest.contains("name = \"cc-switch-proxy-core\""),
        "proxy-core package name should remain stable for host path dependency"
    );
    assert!(
        manifest.contains("path = \"src/mod.rs\""),
        "proxy-core should remain an independent lib crate with an explicit lib path"
    );
    assert!(
        violations.is_empty(),
        "proxy-core must not depend on Tauri, SQLite, or the CC Switch host crate:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_external_example_uses_public_prelude_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("crates/proxy-core/examples/external_relay_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy-core external relay example");
    let required_import = "use cc_switch_proxy_core::api::prelude::*;";
    let forbidden_markers = [
        "use cc_switch::",
        "use cc_switch_proxy_core::api::{",
        "use cc_switch_proxy_core::{",
        "use cc_switch_proxy_core::domain",
        "use cc_switch_proxy_core::ports",
        "use cc_switch_proxy_core::management_api",
        "crate::",
        "super::",
        "src_tauri",
        "tauri::",
        "rusqlite",
        "sqlx",
        "http::",
        "serde_json::",
    ];

    assert!(
        source.contains(required_import),
        "external relay example should enter proxy-core through public prelude import `{required_import}`"
    );

    let mut violations = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "crates/proxy-core/examples/external_relay_host.rs:{} contains forbidden external-host marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "external relay example must stay host-neutral and use only the public prelude:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_public_prelude_smoke_uses_public_prelude_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("crates/proxy-core/tests/public_prelude.rs");
    let source = fs::read_to_string(&path).expect("read proxy-core public prelude test");
    let required_import = "use cc_switch_proxy_core::api::prelude::*;";
    let forbidden_markers = [
        "use cc_switch_proxy_core::api::{",
        "use cc_switch_proxy_core::{",
        "use cc_switch_proxy_core::domain",
        "use cc_switch_proxy_core::ports",
        "use cc_switch_proxy_core::management_api",
        "http::",
        "serde_json::",
    ];

    assert!(
        source.contains(required_import),
        "public prelude smoke should enter proxy-core through `{required_import}`"
    );

    let mut violations = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "crates/proxy-core/tests/public_prelude.rs:{} contains forbidden prelude-smoke marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "public prelude smoke must prove the public prelude without direct internal/core dependency imports:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_handler_context_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handler_context.rs");
    let source = fs::read_to_string(&path).expect("read handler_context.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub(crate) use super::engine::context::*;",
        ],
        "legacy proxy/handler_context.rs must remain a re-export shim after engine/context.rs split"
    );
}

#[test]
fn request_context_does_not_preselect_provider() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/context.rs");
    let source = fs::read_to_string(&path).expect("read engine/context.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_REQUEST_CONTEXT_PROVIDER_PRESELECT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/context.rs:{} contains provider preselection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "RequestContext must wait for ProxyEngine route results before storing selected providers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn request_context_uses_adapter_for_provider_facts() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/context.rs");
    let source = fs::read_to_string(&path).expect("read engine/context.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_REQUEST_CONTEXT_PROVIDER_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/context.rs:{} contains provider adapter marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "RequestContext must consume provider facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn request_context_uses_grouped_api_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/context.rs");
    let source = fs::read_to_string(&path).expect("read engine/context.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for (column, _) in code.match_indices(PROXY_CORE_MARKER) {
            if !code[column..].starts_with(PROXY_CORE_API_MARKER) {
                violations.push(format!(
                    "src/proxy/engine/context.rs:{} contains non-api proxy-core access: {}",
                    line_index + 1,
                    code.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "engine/context.rs must use proxy_core::api as its integration surface:\n{}",
        violations.join("\n")
    );
}

#[test]
fn request_context_owns_core_context_imports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/context.rs");
    let source = fs::read_to_string(&path).expect("read engine/context.rs");
    let adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse axum::",
    );
    let adapter_import_identifiers: Vec<&str> = adapter_import
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|identifier| !identifier.is_empty())
        .collect();

    assert!(
        source.contains("use crate::proxy_core::api::config::{")
            && source.contains("ResponseRuntimePolicy")
            && source.contains("use crate::proxy_core::api::domain::AppKind;")
            && source.contains("use crate::proxy_core::api::errors::{")
            && source.contains("selected_provider_display_name_for_error")
            && source.contains("selected_provider_not_applied_message")
            && source.contains("unselected_provider_fallback_id")
            && source.contains(
                "use crate::proxy_core::api::session::extract_session_id_with_generator;"
            )
            && source.contains("use crate::proxy_core::api::transport::{")
            && source.contains("request_model_for_forward")
            && source.contains("resolve_response_runtime_policy")
            && source.contains("ProxyResult")
            && source.contains(
                "use crate::proxy_core::api::transforms::claude_api_format_from_metadata;"
            )
            && source.contains("use crate::proxy_core::api::usage::UsageRouteContext;"),
        "engine/context.rs should import pure request context contracts directly"
    );

    let mut violations = Vec::new();
    for marker in [
        "AppKind",
        "ProxyResult",
        "ResponseRuntimePolicy",
        "ResponseTimeoutConfig",
        "StreamingTimeoutConfig",
        "UsageRouteContext",
        "claude_api_format_from_metadata",
        "extract_proxy_session_id",
        "request_model_for_forward",
        "response_runtime_policy_from_app_proxy_config",
        "selected_provider_display_name_for_error",
        "selected_provider_not_applied_message",
        "unselected_provider_fallback_id",
    ] {
        if adapter_import_identifiers
            .iter()
            .any(|identifier| identifier == &marker)
        {
            violations.push(format!(
                "engine/context.rs still imports pure core marker `{marker}` from proxy_core_adapter"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "RequestContext should not route pure core context contracts through proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_error_mapper_owns_codex_error_projection() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read error_mapper.rs");

    assert!(
        source.contains("CodexProxyErrorContext")
            && source.contains("CodexProxyHostErrorFacts")
            && source.contains("CodexProxyErrorKind::ForwardFailed")
            && source.contains("codex_proxy_error_code("),
        "error_mapper should own host ProxyError to Codex proxy error context projection"
    );
    assert!(
        source.contains("core_codex_proxy_error_response(")
            && source.contains("core_codex_proxy_error_json("),
        "error_mapper should delegate final Codex proxy error body/response construction to proxy-core"
    );
}

#[test]
fn proxy_core_adapter_reexports_codex_error_projection() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    assert!(
        source.contains("pub(crate) use crate::proxy::error_mapper::{")
            && source.contains("codex_proxy_error_response_from_host_facts")
            && source.contains("codex_proxy_error_response_from_proxy_error")
            && source.contains("CodexProxyHostErrorFacts"),
        "proxy_core_adapter should re-export Codex proxy error projection for compatibility"
    );
    for marker in [
        "struct CodexProxyHostErrorFacts",
        "fn codex_proxy_error_facts_from_proxy_error",
        "fn codex_proxy_error_kind_from_proxy_error",
        "fn codex_proxy_error_context_from_host_facts",
    ] {
        assert!(
            !source.contains(marker),
            "proxy_core_adapter should not own Codex proxy error projection marker `{marker}`"
        );
    }
}

#[test]
fn proxy_error_mapper_delegates_forward_failure_projection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read error_mapper.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_ERROR_MAPPER_FORWARD_FAILURE_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/error_mapper.rs:{} contains forward failure projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Proxy error mapper must delegate forward failure projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_error_mapper_excludes_legacy_status_and_display_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read error_mapper.rs");

    let mut violations = Vec::new();
    for marker in [
        "pub fn map_proxy_error_to_status",
        "pub fn get_error_message",
    ] {
        if source.contains(marker) {
            violations.push(format!(
                "src/proxy/error_mapper.rs keeps pure error policy facade `{marker}`"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "Proxy error mapper must not keep legacy public status/display facades:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_forward_failure_message_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "fn forward_failure_message_from_proxy_error",
        "pub(crate) use crate::proxy_core::api::routing::default_route_candidate_from_selection",
    );

    assert!(
        function.contains("core_forward_failure_message_from_proxy_status("),
        "adapter must delegate forward failure message selection to proxy-core"
    );
    assert!(
        function.contains("proxy_error_status_kind(error)"),
        "adapter should pass ProxyError status kind into the core forward failure message policy"
    );
    assert!(
        !function.contains("=> message.clone()") && !function.contains("_ => error.to_string()"),
        "adapter must not directly choose between raw host messages and display messages"
    );
}

#[test]
fn proxy_error_mapper_delegates_proxy_error_display_message_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read proxy/error_mapper.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn proxy_error_display_message",
        "pub(crate) fn proxy_core_error_to_proxy_error",
    );

    assert!(
        function.contains("proxy_error_display_message_from_status("),
        "error_mapper must delegate ProxyError display-message selection to proxy-core"
    );
    assert!(
        function.contains("proxy_error_status_kind(error)"),
        "error_mapper should pass ProxyError status kind into the core display-message policy"
    );

    for marker in [
        "上游错误",
        "请求超时",
        "转发失败",
        "无可用 Provider",
        "所有供应商已熔断",
        "未配置供应商",
        "所有 Provider 都失败",
        "Provider 不健康",
        "数据库错误",
        "请求/响应转换错误",
    ] {
        assert!(
            !function.contains(marker),
            "error_mapper must not locally format ProxyError display text `{marker}`"
        );
    }
}

#[test]
fn proxy_error_status_projection_lives_in_error_mapper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let proxy_error_path = manifest_dir.join("src/proxy/error.rs");
    let proxy_error_source = fs::read_to_string(&proxy_error_path).expect("read proxy/error.rs");
    let error_mapper_path = manifest_dir.join("src/proxy/error_mapper.rs");
    let error_mapper_source =
        fs::read_to_string(&error_mapper_path).expect("read proxy/error_mapper.rs");

    let error_mapper_function = function_slice(
        &error_mapper_source,
        "pub(crate) fn proxy_error_status_kind",
        "pub(crate) fn proxy_error_status_code",
    );

    assert!(
        error_mapper_function.contains("ProxyErrorStatusKind::ForwardFailed")
            && error_mapper_function.contains("ProxyErrorStatusKind::UpstreamError(*status)")
            && error_mapper_function.contains("ProxyErrorStatusKind::AuthError"),
        "error_mapper should own host ProxyError to core status-kind projection"
    );
    assert!(
        adapter_source.contains("use crate::proxy::error_mapper::{")
            && adapter_source.contains("proxy_error_status_kind")
            && !adapter_source.contains("proxy_error_display_message")
            && !adapter_source.contains("proxy_error_status_code")
            && !adapter_source.contains("pub(crate) fn proxy_error_status_kind")
            && !adapter_source.contains("pub(crate) fn proxy_error_status_code")
            && !adapter_source.contains("pub(crate) fn proxy_error_display_message"),
        "proxy_core_adapter should only retain the local status-kind dependency it still needs"
    );
    assert!(
        !proxy_error_source.contains("fn proxy_error_status_kind("),
        "proxy/error.rs should not own host-to-core status-kind projection"
    );
    assert!(
        !error_mapper_source.contains("use crate::proxy::error::proxy_error_status_kind"),
        "error_mapper should not import status projection from proxy/error.rs"
    );
}

#[test]
fn proxy_error_uses_grouped_api_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error.rs");
    let source = fs::read_to_string(&path).expect("read proxy/error.rs");

    let mut violations = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for (column, _) in code.match_indices(PROXY_CORE_MARKER) {
            if !code[column..].starts_with(PROXY_CORE_API_MARKER) {
                violations.push(format!(
                    "src/proxy/error.rs:{} contains non-api proxy-core access: {}",
                    line_index + 1,
                    code.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy/error.rs must use proxy_core::api as its integration surface:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_error_owns_core_error_response_imports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error.rs");
    let source = fs::read_to_string(&path).expect("read proxy/error.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_production = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);

    assert!(
        source.contains("use crate::proxy_core::api::errors::{")
            && source.contains("proxy_error_http_status_code")
            && source.contains("proxy_error_response_body")
            && source.contains("upstream_proxy_error_response_body"),
        "proxy/error.rs should import HTTP error response contracts directly"
    );

    assert!(
        !source.contains("proxy_core_adapter")
            && !adapter_production.contains("proxy_error_http_status_code")
            && !adapter_production.contains("proxy_error_response_body")
            && !adapter_production.contains("upstream_proxy_error_response_body"),
        "proxy/error.rs should not route HTTP error response contracts through proxy_core_adapter"
    );
}

#[test]
fn production_proxy_error_excludes_legacy_reqwest_category_helper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error.rs");
    let source = fs::read_to_string(&path).expect("read proxy/error.rs");

    let mut violations = Vec::new();
    for marker in [
        "pub enum ErrorCategory",
        "pub fn categorize_error",
        "ClientAbort",
    ] {
        if source.contains(marker) {
            violations.push(format!(
                "src/proxy/error.rs keeps legacy reqwest category helper marker `{marker}`"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy error classification must stay in adapter/core runtime policy, not legacy reqwest helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_error_mapper_delegates_reqwest_send_error_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read proxy/error_mapper.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn reqwest_send_error_to_proxy_error",
        "pub(crate) fn management_api_error_to_proxy_error",
    );

    assert!(
        function.contains("upstream_send_error_projection(")
            && function.contains("UpstreamSendErrorInput")
            && function.contains("is_timeout: error.is_timeout()")
            && function.contains("is_connect: error.is_connect()"),
        "reqwest send error bridge must pass neutral facts into proxy-core"
    );

    for marker in ["请求超时", "连接失败"] {
        assert!(
            !function.contains(marker),
            "reqwest send error bridge must not locally format upstream send error marker `{marker}`"
        );
    }
}

#[test]
fn production_proxy_handlers_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub(crate) use super::transport::http::handlers::*;",
        ],
        "legacy proxy/handlers.rs must remain a re-export shim after transport/http/handlers.rs split"
    );
}

#[test]
fn basic_health_status_handlers_use_management_contracts() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let adapter_path = manifest_dir.join("src/proxy/response_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read response_adapter.rs");
    let health_handler = function_slice(&source, "pub async fn health_check", "/// 获取服务状态");
    let status_handler = function_slice(
        &source,
        "pub async fn get_status",
        "/// GET /proxy/v1/events",
    );

    assert!(
        health_handler.contains("proxy_health_check_to_axum_json_response(")
            && status_handler.contains("dispatch_proxy_status_request_to_axum_json_response("),
        "basic health/status HTTP handlers should delegate response assembly to response_adapter"
    );
    assert!(
        adapter_source.contains("HealthCheckRequest::new()")
            && adapter_source.contains("ProxyStatusRequest::new()")
            && adapter_source.contains(".proxy_status_response(request)"),
        "response_adapter must own basic health/status management contracts and ProxyEngine status call"
    );

    let forbidden_markers = [
        "health_check_source_from_timestamp",
        "proxy_status_source_from_status",
        ".response_from_source(",
        "state.status",
        ".status.read()",
    ];

    let mut violations = Vec::new();
    for (handler_name, handler) in [
        ("health_check", health_handler),
        ("get_status", status_handler),
    ] {
        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs {}:{} contains basic status wrapper marker `{}`",
                        handler_name,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "basic health/status HTTP handlers must keep runtime source wrappers out of Axum handlers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_server_status_delegates_to_proxy_engine_runtime_status() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let function = function_slice(
        &source,
        "pub async fn get_status(&self) -> ProxyRuntimeStatus",
        "/// 更新某个应用类型当前",
    );

    assert!(
        function.contains(".proxy_engine()") && function.contains(".runtime_status()"),
        "ProxyServer::get_status must use ProxyEngine runtime status source"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in [
            "proxy_runtime_status_from_runtime_sources",
            "self.state.status",
            "self.state.current_providers",
        ] {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs get_status:{} contains runtime status source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyServer::get_status must keep runtime status source reads behind ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_gateway_auth_delegates_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/auth_adapter.rs");
    let source = fs::read_to_string(&path).expect("read auth_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) async fn validate_claude_desktop_gateway_auth",
        "}",
    );

    assert!(
        function.contains(".proxy_engine()")
            && function.contains(".validate_claude_desktop_gateway_auth(headers)"),
        "Claude Desktop gateway auth adapter must delegate token lookup and bearer validation to ProxyEngine"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in [
            "state.db",
            "claude_desktop_config",
            "get_or_create_gateway_token",
            "validate_claude_desktop_gateway_bearer_header",
        ] {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/auth_adapter.rs validate_claude_desktop_gateway_auth:{} contains gateway auth source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude Desktop gateway auth token source must stay behind ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_server_claude_desktop_gateway_smoke_uses_adapter_token_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read proxy/server.rs");
    let smoke_slice = function_slice(
        &source,
        "async fn proxy_server_runtime_smoke_serves_claude_desktop_models_with_gateway_auth",
        "async fn management_apps_and_providers_return_sanitized_summaries",
    );

    assert!(
        smoke_slice.contains("get_or_create_claude_desktop_gateway_token_from_db_source("),
        "Claude Desktop gateway runtime smoke should use the adapter-owned token source"
    );

    let forbidden_markers = [
        "claude_desktop_config::get_or_create_gateway_token",
        "crate::claude_desktop_config",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in smoke_slice.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs proxy_server_runtime_smoke_serves_claude_desktop_models_with_gateway_auth:{} contains host token-source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude Desktop gateway smoke must keep token creation behind proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_claude_desktop_gateway_auth_source_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let auth_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/claude_desktop_gateway_auth_source.rs");
    let auth_source =
        fs::read_to_string(&auth_source_path).expect("read claude_desktop_gateway_auth_source.rs");
    let token_source_slice = function_slice(
        &source,
        "pub(crate) fn get_or_create_claude_desktop_gateway_token_from_db_source(",
        "pub(crate) type AttemptEventChannel",
    );

    assert!(
        token_source_slice.contains("CLAUDE_DESKTOP_GATEWAY_TOKEN_SETTING_KEY")
            && token_source_slice.contains(".get_setting(")
            && token_source_slice.contains(".set_setting(")
            && token_source_slice.contains("uuid::Uuid::new_v4()"),
        "proxy_core_adapter should keep Claude Desktop gateway token DB helper"
    );
    assert!(
        auth_source.contains("pub(crate) struct CcSwitchClaudeDesktopGatewayAuthSource")
            && auth_source.contains(
                "impl ClaudeDesktopGatewayAuthSource for CcSwitchClaudeDesktopGatewayAuthSource"
            )
            && auth_source.contains("get_or_create_claude_desktop_gateway_token_from_db_source(")
            && auth_source.contains("claude_desktop_gateway_token_error"),
        "CC Switch Claude Desktop gateway auth source implementation should live in host/cc_switch"
    );
    assert!(
        auth_source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && auth_source
                .contains("use crate::proxy_core::api::ports::ClaudeDesktopGatewayAuthSource;"),
        "CC Switch Claude Desktop gateway auth source should import auth contracts directly from proxy_core"
    );
    let adapter_import = function_slice(
        &auth_source,
        "use crate::proxy_core_adapter::{",
        "};\nuse futures::future::BoxFuture;",
    );
    for adapter_type in ["ClaudeDesktopGatewayAuthSource", "ProxyCoreResult"] {
        assert!(
            !adapter_import.contains(adapter_type),
            "CC Switch Claude Desktop gateway auth source must not import {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        !source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::claude_desktop_gateway_auth_source::CcSwitchClaudeDesktopGatewayAuthSource"
        ) && !source.contains("struct CcSwitchClaudeDesktopGatewayAuthSource")
            && !source
                .contains("impl ClaudeDesktopGatewayAuthSource for CcSwitchClaudeDesktopGatewayAuthSource")
            && !source.contains("fn load_gateway_token<'a>(&'a self)"),
        "proxy_core_adapter should not re-export or own the Claude Desktop gateway auth source"
    );
    let adapter_core_ports_import = function_slice(
        &source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    assert!(
        !adapter_core_ports_import.contains("ClaudeDesktopGatewayAuthSource"),
        "proxy_core_adapter should not re-export ClaudeDesktopGatewayAuthSource"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "get_or_create_gateway_token(",
    ];
    let mut violations = Vec::new();
    for (label, slice) in [
        (
            "get_or_create_claude_desktop_gateway_token_from_db_source",
            token_source_slice,
        ),
        (
            "CcSwitchClaudeDesktopGatewayAuthSource::load_gateway_token",
            auth_source.as_str(),
        ),
    ] {
        for (line_index, line) in production_lines(slice) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy_core_adapter.rs {label}:{} contains host token-source marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop gateway token source out of host config:\n{}",
        violations.join("\n")
    );
}

#[test]
fn provider_list_handler_delegates_sources_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn list_proxy_providers",
        "/// GET /proxy/v1/apps/{app}/models",
    );
    let forbidden_markers = [
        "state.db",
        "provider_router",
        ".select_providers(",
        ".select_provider_ids(",
        "get_all_providers(",
        "get_current_provider(",
        "get_failover_queue(",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs list_proxy_providers:{} contains runtime source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider-list HTTP handler must delegate provider/current/failover/candidate sources to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn route_resolve_handler_delegates_dry_run_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn resolve_proxy_route",
        "/// GET /v1/models",
    );
    let forbidden_markers = [
        "provider_router",
        ".resolve_channel_route_dry_run(",
        "request.request.clone()",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs resolve_proxy_route:{} contains route dry-run marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "route-resolve HTTP handler must delegate dry-run route resolution to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn app_list_handler_delegates_summary_sources_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn list_proxy_apps",
        "/// GET /proxy/v1/apps/{app}/providers",
    );
    let forbidden_markers = [
        "state.db",
        ".get_proxy_config_for_app(",
        ".get_all_providers(",
        ".list_proxy_channels_for_app(",
        "proxy_app_summary_input",
        "app_list_source_from_summaries",
        ".response_from_source(",
        "AppType::all",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs list_proxy_apps:{} contains app-list source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "app-list HTTP handler must delegate config/provider/channel summary sources to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn channel_list_handler_delegates_materialized_records_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn list_all_proxy_channels",
        "/// POST /proxy/v1/channels",
    );
    let forbidden_markers = [
        "state.db",
        "ChannelListPlan",
        ".list_proxy_channels_for_app(",
        ".list_all_proxy_channels(",
        "channel_list_source_from_records",
        ".response_from_source(",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs list_all_proxy_channels:{} contains channel DB marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "channel-list HTTP handler must delegate materialized record loading to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn channel_crud_handlers_delegate_records_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handlers = [
        (
            "create_proxy_channel",
            function_slice(
                &source,
                "pub async fn create_proxy_channel",
                "/// GET /proxy/v1/channels/{channel_id}",
            ),
        ),
        (
            "get_proxy_channel",
            function_slice(
                &source,
                "pub async fn get_proxy_channel",
                "/// PATCH /proxy/v1/channels/{channel_id}",
            ),
        ),
        (
            "update_proxy_channel",
            function_slice(
                &source,
                "pub async fn update_proxy_channel",
                "/// DELETE /proxy/v1/channels/{channel_id}",
            ),
        ),
        (
            "delete_proxy_channel",
            function_slice(
                &source,
                "pub async fn delete_proxy_channel",
                "/// GET /proxy/v1/channels/{channel_id}/keys",
            ),
        ),
    ];

    let forbidden_markers = [
        "state.db",
        ".create_proxy_channel(",
        ".get_proxy_channel(",
        ".update_proxy_channel(",
        ".delete_proxy_channel(",
        "channel_create_source_from_record",
        "channel_record_source_from_record",
        "channel_delete_source_from_deleted",
        ".record_response_from_source(",
        ".delete_response_from_source(",
    ];

    let mut violations = Vec::new();
    for (handler_name, handler) in handlers {
        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs {}:{} contains channel CRUD source marker `{}`",
                        handler_name,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "channel CRUD HTTP handlers must delegate record sources to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn channel_key_and_model_handlers_delegate_sources_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handlers = [
        (
            "list_proxy_channel_keys",
            function_slice(
                &source,
                "pub async fn list_proxy_channel_keys",
                "/// PUT /proxy/v1/channels/{channel_id}/keys/{key_ref}",
            ),
        ),
        (
            "upsert_proxy_channel_key",
            function_slice(
                &source,
                "pub async fn upsert_proxy_channel_key",
                "/// PATCH /proxy/v1/channels/{channel_id}/keys/{key_ref}",
            ),
        ),
        (
            "update_proxy_channel_key",
            function_slice(
                &source,
                "pub async fn update_proxy_channel_key",
                "/// DELETE /proxy/v1/channels/{channel_id}/keys/{key_ref}",
            ),
        ),
        (
            "delete_proxy_channel_key",
            function_slice(
                &source,
                "pub async fn delete_proxy_channel_key",
                "/// GET /proxy/v1/channels/{channel_id}/models",
            ),
        ),
        (
            "list_proxy_channel_models",
            function_slice(
                &source,
                "pub async fn list_proxy_channel_models",
                "/// PUT /proxy/v1/channels/{channel_id}/models",
            ),
        ),
        (
            "replace_proxy_channel_models",
            function_slice(
                &source,
                "pub async fn replace_proxy_channel_models",
                "/// POST /proxy/v1/channels/{channel_id}/test",
            ),
        ),
    ];

    let forbidden_markers = [
        "state.db",
        ".list_proxy_channel_keys(",
        ".upsert_proxy_channel_key(",
        ".update_proxy_channel_key(",
        ".delete_proxy_channel_key(",
        ".get_proxy_channel(",
        ".list_proxy_channel_models(",
        ".replace_proxy_channel_models(",
        "channel_keys_source_from_records",
        "channel_key_record_source_from_record",
        "channel_key_delete_source_from_deleted",
        "channel_models_source_from_records",
        ".keys_response_from_source(",
        ".record_response_from_source(",
        ".delete_response_from_source(",
        ".models_response_from_source(",
    ];

    let mut violations = Vec::new();
    for (handler_name, handler) in handlers {
        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs {}:{} contains channel key/model source marker `{}`",
                        handler_name,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "channel key/model HTTP handlers must delegate subresource sources to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_channel_runtime_source_delegates_key_selection_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let runtime_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/channel_key_runtime_source.rs");
    let runtime_source =
        fs::read_to_string(&runtime_source_path).expect("read channel_key_runtime_source.rs");
    let dao_path = manifest_dir.join("src/database/dao/proxy_channels.rs");
    let dao_source = fs::read_to_string(&dao_path).expect("read proxy_channels.rs");
    let function = function_slice(
        &runtime_source,
        "fn load_channel_key_candidate_from_database",
        "impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource",
    );

    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::channel_key_runtime_source::{"
        ) && !adapter_source
            .contains("channel_key_runtime_source_from_database, CcSwitchChannelKeyRuntimeSource")
            && !adapter_source.contains("fn load_channel_key_candidate_from_database"),
        "proxy_core_adapter should not re-export or own the DB-backed channel-key runtime source"
    );
    assert!(
        function.contains(".list_proxy_channel_key_runtime_candidates(")
            && function.contains("select_proxy_channel_key_runtime_candidate(")
            && !function.contains(".key_value"),
        "channel key runtime source must load runtime DB key records, delegate key-ref/enabled selection to core, and return the selected runtime candidate"
    );
    assert!(
        runtime_source.contains("use crate::proxy_core::api::management::{")
            && runtime_source.contains("channel_key_runtime_candidate_from_input")
            && runtime_source.contains("select_channel_key_runtime_candidate")
            && runtime_source.contains("select_enabled_channel_key_runtime_candidate")
            && runtime_source.contains("ChannelKeyRuntimeCandidateInput"),
        "channel key runtime source should import pure candidate projection/selection directly from proxy_core::api::management"
    );
    for marker in [
        "core_select_channel_key_runtime_candidate",
        "core_select_enabled_channel_key_runtime_candidate",
        "channel_key_runtime_candidate_from_input",
        "ChannelKeyRuntimeCandidateInput",
    ] {
        assert!(
            !adapter_runtime_source.contains(marker),
            "proxy_core_adapter should not re-export pure channel-key candidate helper/type `{marker}` once runtime source owns the call site"
        );
    }
    assert!(
        !function.contains(".get_proxy_channel_key("),
        "channel key runtime source must not perform exact-key DB lookup before core candidate selection"
    );
    assert!(
        !function.contains(".get_enabled_proxy_channel_key("),
        "channel key runtime source must not rely on the DAO test convenience selector"
    );
    assert!(
        dao_source.contains("pub(crate) fn list_proxy_channel_key_runtime_candidates"),
        "DAO should expose a runtime-only channel key candidate list separate from management key listing"
    );
    assert!(
        dao_source.contains("#[cfg(test)]\n    pub(crate) fn get_enabled_proxy_channel_key"),
        "DAO enabled-key selector should remain test-only while production runtime selection lives in the host runtime source"
    );

    let forbidden_markers = ["key.status == \"enabled\"", "key.status != \"enabled\""];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains runtime key policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "runtime channel key selection policy must stay in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn channel_test_handler_delegates_probe_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn test_proxy_channel",
        "/// GET /proxy/v1/apps/{app}/channels",
    );
    let forbidden_markers = [
        "state.db",
        "StreamCheckService",
        "channel_test_plan_from_record",
        "stream_check_result_to_channel_reachability",
        "AppType::from_str",
        "chrono::Utc::now()",
        ".timestamp()",
        ".get_proxy_channel(",
        ".get_provider_by_id(",
        ".get_stream_check_config(",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs test_proxy_channel:{} contains channel test source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "channel test HTTP handler must delegate probe orchestration to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_uses_host_reachability_probe_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let services_path = manifest_dir.join("src/proxy/host/cc_switch/proxy_services.rs");
    let services_source = fs::read_to_string(&services_path).expect("read proxy_services.rs");
    let probe_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/channel_reachability_probe.rs");
    let probe_source =
        fs::read_to_string(&probe_source_path).expect("read channel_reachability_probe.rs");
    let services_struct = function_slice(
        &services_source,
        "pub(crate) struct CcSwitchProxyServices",
        "impl<R> CcSwitchProxyServices",
    );
    let services_impl = services_source.as_str();
    let probe_adapter_import = function_slice(
        &probe_source,
        "use crate::proxy_core_adapter::{",
        "};\nuse crate::services::stream_check::StreamCheckService;",
    );

    assert!(
        !(adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::channel_reachability_probe::{"
        ) || adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::channel_reachability_probe::CcSwitchChannelReachabilityProbe"
        ) || adapter_source
            .contains("impl ChannelReachabilityProbe for CcSwitchChannelReachabilityProbe")
            || adapter_source.contains("probe_channel_reachability_from_db_source(")),
        "DB-backed reachability probe implementation should live in the host cc_switch module without adapter re-export"
    );
    assert!(
        !adapter_source
            .contains("pub(crate) use crate::proxy_core::api::ports::ChannelReachabilityProbe")
            && !adapter_source.contains(
                "pub(crate) use crate::proxy_core::api::ports::{\n    ChannelReachabilityProbe"
            ),
        "proxy_core_adapter should not re-export the ChannelReachabilityProbe port trait"
    );
    assert!(
        probe_source.contains("impl ChannelReachabilityProbe for CcSwitchChannelReachabilityProbe")
            && probe_source.contains("probe_channel_reachability_from_db_source("),
        "host reachability module should implement the proxy-core reachability probe port"
    );
    assert!(
        probe_source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && probe_source.contains(
                "use crate::proxy_core::api::management::{ChannelReachabilityResult, ChannelTestProbeRequest};"
            )
            && probe_source
                .contains("use crate::proxy_core::api::ports::ChannelReachabilityProbe;"),
        "host reachability module should import core reachability contracts directly"
    );
    for adapter_type in [
        "ChannelReachabilityProbe",
        "ChannelReachabilityResult",
        "ChannelTestProbeRequest",
        "ProxyCoreResult",
    ] {
        assert!(
            !probe_adapter_import.contains(adapter_type),
            "host reachability module should not import core contract {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        probe_source.contains(".get_provider_by_id(")
            && probe_source.contains(".get_stream_check_config(")
            && probe_source.contains("StreamCheckService::check_with_retry(")
            && probe_source.contains("stream_check_result_to_channel_reachability("),
        "host reachability module must own DB lookup, stream-check side effects, and core reachability projection"
    );
    assert!(
        services_struct.contains("reachability_probe: CcSwitchChannelReachabilityProbe"),
        "CC Switch service container should own the DB-backed reachability probe"
    );
    assert!(
        services_impl.contains("fn reachability_probe(")
            && services_impl.contains("&self.reachability_probe"),
        "CC Switch ProxyServices implementation should return the reachability probe"
    );
}

#[test]
fn channel_list_route_branch_delegates_dry_run_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn list_proxy_channels",
        "/// GET /proxy/v1/groups",
    );

    let forbidden_markers = [
        "provider_router",
        ".resolve_channel_route_dry_run(",
        ".list_channels_for_app(",
        "AppChannelManagementPlan",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs list_proxy_channels:{} contains channel source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "channel-list HTTP handler must delegate list and route source resolution to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn group_list_handler_delegates_sources_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn list_proxy_groups",
        "/// GET /proxy/v1/apps/{app}/routes/current",
    );

    let forbidden_markers = [
        "provider_router",
        ".list_channels_for_app(",
        "group_list_channel_source_from_records",
        ".response_from_channel_sources(",
        ".app_scope(",
        "AppType::all",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs list_proxy_groups:{} contains group source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "group-list HTTP handler must delegate channel source aggregation to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn current_route_handler_delegates_runtime_sources_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn get_current_proxy_route",
        "/// GET /proxy/v1/apps/{app}/channels/migration/preview",
    );

    let forbidden_markers = [
        "current_providers",
        "state.db",
        ".get_current_provider(",
        ".get_provider_by_id(",
        "current_route_source_from_provider",
        ".current_route_response_from_source(",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs get_current_proxy_route:{} contains current-route source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "current-route HTTP handler must delegate active/configured provider sources to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn channel_migration_handlers_delegate_sources_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handlers = [
        (
            "preview_proxy_channel_migration",
            function_slice(
                &source,
                "pub async fn preview_proxy_channel_migration",
                "/// POST /proxy/v1/apps/{app}/channels/migration/materialize",
            ),
        ),
        (
            "materialize_proxy_channel_migration",
            function_slice(
                &source,
                "pub async fn materialize_proxy_channel_migration",
                "/// GET /proxy/v1/channels/{channel_id}/breakers/stats",
            ),
        ),
    ];

    let forbidden_markers = [
        "state.db",
        ".preview_legacy_proxy_channel_migration(",
        ".materialize_legacy_proxy_channels(",
        "channel_migration_preview_source_from_result",
        "channel_migration_materialize_source_from_result",
        ".migration_preview_response_from_source(",
        ".migration_materialize_response_from_source(",
    ];

    let mut violations = Vec::new();
    for (handler_name, handler) in handlers {
        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs {}:{} contains migration source marker `{}`",
                        handler_name,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "channel migration HTTP handlers must delegate preview/materialize sources to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn channel_breaker_stats_handler_delegates_response_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn get_proxy_channel_breaker_stats",
        "/// POST /proxy/v1/channels/{channel_id}/breakers/reset",
    );
    let forbidden_markers = [
        "state.db",
        "channel_breaker_stats_with_router_source",
        ".get_channel_circuit_breaker_stats(",
        ".breaker_stats_response_from_source(",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs get_proxy_channel_breaker_stats:{} contains breaker stats source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "channel breaker stats HTTP handler must delegate response wrapping to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn channel_health_reset_handler_delegates_response_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn reset_proxy_channel_breaker",
        "/// POST /proxy/v1/route/resolve",
    );
    let forbidden_markers = [
        "channel_health_reset_source_from_response",
        ".health_reset_response_from_source(",
        ".reset_channel_breaker(",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs reset_proxy_channel_breaker:{} contains health reset source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "channel health reset HTTP handler must delegate response wrapping to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_models_handler_delegates_provider_selection_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn handle_claude_desktop_models",
        "\n}\n\n// ============================================================================\n// Codex API",
    );
    let forbidden_markers = [
        "provider_router",
        ".select_providers(",
        ".select_provider_ids(",
        "claude_desktop_config::model_list_response",
        "ProxyError::NoAvailableProvider",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs handle_claude_desktop_models:{} contains Claude Desktop provider marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude Desktop models handler must delegate provider selection and model-list response building to ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_handlers_build_proxy_requests_through_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_PROXY_REQUEST_BRIDGE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains direct proxy request bridge marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production handlers must build ProxyRequest values through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_handlers_delegate_proxy_result_response_bridge_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_PROXY_RESULT_RESPONSE_BRIDGE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains direct proxy result bridge marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production handlers must delegate ProxyResult context updates and response transport bridge to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_handlers_parse_json_bodies_through_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_RAW_JSON_BODY_PARSE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains direct JSON body parse marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production handlers must parse JSON proxy bodies through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_handlers_collect_bodies_through_transport_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_DIRECT_BODY_COLLECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains direct body collection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production handlers must collect Axum request bodies through response_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_handlers_delegate_provider_decisions_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_PROVIDER_ADAPTER_DECISION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains provider decision marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production handlers must delegate provider decisions to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_handlers_delegate_response_branch_gates_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_RESPONSE_BRANCH_GATE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains response branch gate marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production handlers must delegate response branch gates to response_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_protocol_handlers_delegate_endpoint_bridge_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let protocol_handlers = [
        function_slice(
            &source,
            "pub async fn handle_messages(",
            "\n}\n\npub async fn handle_claude_desktop_messages",
        ),
        function_slice(
            &source,
            "pub async fn handle_claude_desktop_messages(",
            "\n}\n\npub async fn handle_claude_desktop_models",
        ),
        function_slice(
            &source,
            "pub async fn handle_chat_completions(",
            "\n}\n\n/// 处理 /v1/responses 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses(",
            "\n}\n\n/// 处理 /v1/responses/compact 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses_compact(",
            "\n}\n\n// ============================================================================\n// Gemini API",
        ),
        function_slice(
            &source,
            "pub async fn handle_gemini(",
            "\n}\n\n#[cfg(test)]",
        ),
    ];

    let mut violations = Vec::new();
    for handler in protocol_handlers {
        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROTOCOL_HANDLER_ENDPOINT_BRIDGE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs:{} contains endpoint bridge marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate endpoint path/query bridge to response_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_protocol_handlers_delegate_request_context_bridge_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let protocol_handlers = [
        function_slice(
            &source,
            "pub async fn handle_messages(",
            "\n}\n\npub async fn handle_claude_desktop_messages",
        ),
        function_slice(
            &source,
            "pub async fn handle_claude_desktop_messages(",
            "\n}\n\npub async fn handle_claude_desktop_models",
        ),
        function_slice(
            &source,
            "pub async fn handle_chat_completions(",
            "\n}\n\n/// 处理 /v1/responses 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses(",
            "\n}\n\n/// 处理 /v1/responses/compact 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses_compact(",
            "\n}\n\n// ============================================================================\n// Gemini API",
        ),
        function_slice(
            &source,
            "pub async fn handle_gemini(",
            "\n}\n\n#[cfg(test)]",
        ),
    ];

    let mut violations = Vec::new();
    for handler in protocol_handlers {
        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROTOCOL_HANDLER_CONTEXT_BRIDGE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs:{} contains request context bridge marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate RequestContext construction to response_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_gemini_handler_delegates_protocol_orchestration_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn handle_gemini(",
        "\n}\n\n#[cfg(test)]",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_GEMINI_HANDLER_ORCHESTRATION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs handle_gemini:{} contains Gemini orchestration marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Gemini handler must delegate protocol orchestration to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_codex_handlers_delegate_dispatch_outcome_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let codex_handlers = [
        function_slice(
            &source,
            "pub async fn handle_chat_completions(",
            "\n}\n\n/// 处理 /v1/responses 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses(",
            "\n}\n\n/// 处理 /v1/responses/compact 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses_compact(",
            "\n}\n\n// ============================================================================\n// Gemini API",
        ),
    ];

    let mut violations = Vec::new();
    for handler in codex_handlers {
        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_CODEX_HANDLER_DISPATCH_OUTCOME_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs:{} contains Codex dispatch outcome marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Codex handlers must delegate dispatch outcome and response branch orchestration to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_codex_handlers_delegate_protocol_orchestration_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let codex_handlers = [
        function_slice(
            &source,
            "pub async fn handle_chat_completions(",
            "\n}\n\n/// 处理 /v1/responses 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses(",
            "\n}\n\n/// 处理 /v1/responses/compact 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses_compact(",
            "\n}\n\n// ============================================================================\n// Gemini API",
        ),
    ];

    let mut violations = Vec::new();
    for handler in codex_handlers {
        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_CODEX_HANDLER_ORCHESTRATION_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs:{} contains Codex orchestration marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Codex handlers must delegate request/context/endpoint orchestration to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_claude_messages_handler_delegates_protocol_orchestration_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn handle_messages(",
        "\n}\n\n// ============================================================================\n// Codex API",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CLAUDE_MESSAGES_HANDLER_ORCHESTRATION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs Claude Messages handlers:{} contains Claude Messages orchestration marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude Messages handler must delegate protocol orchestration to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_handlers_delegate_codex_history_recording_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CODEX_HISTORY_RECORD_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains direct Codex history recording marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production handlers must delegate Codex history recording to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_protocol_handlers_delegate_forward_core_error_usage_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let protocol_handlers = [
        function_slice(
            &source,
            "pub async fn handle_messages(",
            "\n}\n\npub async fn handle_claude_desktop_messages",
        ),
        function_slice(
            &source,
            "pub async fn handle_claude_desktop_messages(",
            "\n}\n\npub async fn handle_claude_desktop_models",
        ),
        function_slice(
            &source,
            "pub async fn handle_chat_completions(",
            "\n}\n\n/// 处理 /v1/responses 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses(",
            "\n}\n\n/// 处理 /v1/responses/compact 请求",
        ),
        function_slice(
            &source,
            "pub async fn handle_responses_compact(",
            "\n}\n\n// ============================================================================\n// Gemini API",
        ),
        function_slice(
            &source,
            "pub async fn handle_gemini(",
            "\n}\n\n#[cfg(test)]",
        ),
    ];

    let mut violations = Vec::new();
    for handler in protocol_handlers {
        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROTOCOL_HANDLER_FORWARD_CORE_ERROR_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs:{} contains forward core error marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate core-error mapping plus forward usage logging to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_protocol_handlers_delegate_route_metadata_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROTOCOL_HANDLER_ROUTE_METADATA_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains protocol route metadata marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate app/tag/prefix/endpoint metadata to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_events_handler_delegates_sse_orchestration_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn stream_proxy_events(",
        "/// Management API auth middleware.",
    );

    assert!(
        handler.contains("proxy_events_request_to_axum_sse_response("),
        "proxy events handler should delegate SSE response construction to response_adapter"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_EVENTS_HANDLER_SSE_ORCHESTRATION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs stream_proxy_events:{} contains SSE orchestration marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy events handler must delegate event subscription and SSE keep-alive construction to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_management_read_handlers_delegate_json_bridge_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handlers = [
        (
            "list_proxy_apps",
            function_slice(
                &source,
                "pub async fn list_proxy_apps(",
                "/// GET /proxy/v1/apps/{app}/providers",
            ),
            "dispatch_proxy_apps_request_to_axum_json_response(",
        ),
        (
            "list_proxy_providers",
            function_slice(
                &source,
                "pub async fn list_proxy_providers(",
                "/// GET /proxy/v1/apps/{app}/models",
            ),
            "dispatch_proxy_providers_request_to_axum_json_response(",
        ),
        (
            "list_proxy_app_models",
            function_slice(
                &source,
                "pub async fn list_proxy_app_models(",
                "/// GET /proxy/v1/channels",
            ),
            "dispatch_proxy_app_models_request_to_axum_json_response(",
        ),
        (
            "handle_models",
            function_slice(
                &source,
                "pub async fn handle_models(",
                "// ============================================================================\n// Claude API",
            ),
            "dispatch_codex_client_model_catalog_request_to_axum_json_response(",
        ),
    ];

    let mut violations = Vec::new();
    for (handler_name, handler, adapter_marker) in handlers {
        if !handler.contains(adapter_marker) {
            violations.push(format!(
                "src/proxy/transport/http/handlers.rs {handler_name} should call `{adapter_marker}`"
            ));
        }

        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_MANAGEMENT_READ_HANDLER_ENGINE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs {handler_name}:{} contains management read engine marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "management read handlers must delegate request construction, engine calls, route metadata, and JSON wrapping to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_route_inspection_handlers_delegate_json_bridge_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handlers = [
        (
            "list_all_proxy_channels",
            function_slice(
                &source,
                "pub async fn list_all_proxy_channels(",
                "/// POST /proxy/v1/channels",
            ),
            "dispatch_proxy_channels_request_to_axum_json_response(",
        ),
        (
            "list_proxy_channels",
            function_slice(
                &source,
                "pub async fn list_proxy_channels(",
                "/// GET /proxy/v1/groups",
            ),
            "dispatch_proxy_app_channels_request_to_axum_json_response(",
        ),
        (
            "list_proxy_groups",
            function_slice(
                &source,
                "pub async fn list_proxy_groups(",
                "/// GET /proxy/v1/apps/{app}/routes/current",
            ),
            "dispatch_proxy_groups_request_to_axum_json_response(",
        ),
        (
            "get_current_proxy_route",
            function_slice(
                &source,
                "pub async fn get_current_proxy_route(",
                "/// GET /proxy/v1/apps/{app}/channels/migration/preview",
            ),
            "dispatch_current_proxy_route_request_to_axum_json_response(",
        ),
        (
            "resolve_proxy_route",
            function_slice(
                &source,
                "pub async fn resolve_proxy_route(",
                "/// GET /v1/models",
            ),
            "dispatch_proxy_route_resolve_request_to_axum_json_response(",
        ),
    ];

    let mut violations = Vec::new();
    for (handler_name, handler, adapter_marker) in handlers {
        if !handler.contains(adapter_marker) {
            violations.push(format!(
                "src/proxy/transport/http/handlers.rs {handler_name} should call `{adapter_marker}`"
            ));
        }

        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_ROUTE_INSPECTION_HANDLER_ENGINE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs {handler_name}:{} contains route inspection engine marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "route inspection handlers must delegate request construction, dry-run/current-route engine calls, and JSON wrapping to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_channel_mutation_handlers_delegate_json_bridge_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handlers = [
        (
            "create_proxy_channel",
            function_slice(
                &source,
                "pub async fn create_proxy_channel(",
                "/// GET /proxy/v1/channels/{channel_id}",
            ),
            "dispatch_create_proxy_channel_request_to_axum_json_response(",
        ),
        (
            "get_proxy_channel",
            function_slice(
                &source,
                "pub async fn get_proxy_channel(",
                "/// PATCH /proxy/v1/channels/{channel_id}",
            ),
            "dispatch_get_proxy_channel_request_to_axum_json_response(",
        ),
        (
            "update_proxy_channel",
            function_slice(
                &source,
                "pub async fn update_proxy_channel(",
                "/// DELETE /proxy/v1/channels/{channel_id}",
            ),
            "dispatch_update_proxy_channel_request_to_axum_json_response(",
        ),
        (
            "delete_proxy_channel",
            function_slice(
                &source,
                "pub async fn delete_proxy_channel(",
                "/// GET /proxy/v1/channels/{channel_id}/keys",
            ),
            "dispatch_delete_proxy_channel_request_to_axum_json_response(",
        ),
        (
            "list_proxy_channel_keys",
            function_slice(
                &source,
                "pub async fn list_proxy_channel_keys(",
                "/// PUT /proxy/v1/channels/{channel_id}/keys/{key_ref}",
            ),
            "dispatch_proxy_channel_keys_request_to_axum_json_response(",
        ),
        (
            "upsert_proxy_channel_key",
            function_slice(
                &source,
                "pub async fn upsert_proxy_channel_key(",
                "/// PATCH /proxy/v1/channels/{channel_id}/keys/{key_ref}",
            ),
            "dispatch_upsert_proxy_channel_key_request_to_axum_json_response(",
        ),
        (
            "update_proxy_channel_key",
            function_slice(
                &source,
                "pub async fn update_proxy_channel_key(",
                "/// DELETE /proxy/v1/channels/{channel_id}/keys/{key_ref}",
            ),
            "dispatch_update_proxy_channel_key_request_to_axum_json_response(",
        ),
        (
            "delete_proxy_channel_key",
            function_slice(
                &source,
                "pub async fn delete_proxy_channel_key(",
                "/// GET /proxy/v1/channels/{channel_id}/models",
            ),
            "dispatch_delete_proxy_channel_key_request_to_axum_json_response(",
        ),
        (
            "list_proxy_channel_models",
            function_slice(
                &source,
                "pub async fn list_proxy_channel_models(",
                "/// PUT /proxy/v1/channels/{channel_id}/models",
            ),
            "dispatch_proxy_channel_models_request_to_axum_json_response(",
        ),
        (
            "replace_proxy_channel_models",
            function_slice(
                &source,
                "pub async fn replace_proxy_channel_models(",
                "/// POST /proxy/v1/channels/{channel_id}/test",
            ),
            "dispatch_replace_proxy_channel_models_request_to_axum_json_response(",
        ),
        (
            "test_proxy_channel",
            function_slice(
                &source,
                "pub async fn test_proxy_channel(",
                "/// GET /proxy/v1/apps/{app}/channels",
            ),
            "dispatch_proxy_channel_test_request_to_axum_json_response(",
        ),
    ];

    let mut violations = Vec::new();
    for (handler_name, handler, adapter_marker) in handlers {
        if !handler.contains(adapter_marker) {
            violations.push(format!(
                "src/proxy/transport/http/handlers.rs {handler_name} should call `{adapter_marker}`"
            ));
        }

        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_CHANNEL_MUTATION_HANDLER_ENGINE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs {handler_name}:{} contains channel mutation engine marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "channel mutation handlers must delegate path/body request construction, engine calls, and JSON wrapping to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_migration_and_breaker_handlers_delegate_json_bridge_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handlers = [
        (
            "preview_proxy_channel_migration",
            function_slice(
                &source,
                "pub async fn preview_proxy_channel_migration(",
                "/// POST /proxy/v1/apps/{app}/channels/migration/materialize",
            ),
            "dispatch_preview_proxy_channel_migration_request_to_axum_json_response(",
        ),
        (
            "materialize_proxy_channel_migration",
            function_slice(
                &source,
                "pub async fn materialize_proxy_channel_migration(",
                "/// GET /proxy/v1/channels/{channel_id}/breakers/stats",
            ),
            "dispatch_materialize_proxy_channel_migration_request_to_axum_json_response(",
        ),
        (
            "get_proxy_channel_breaker_stats",
            function_slice(
                &source,
                "pub async fn get_proxy_channel_breaker_stats(",
                "/// POST /proxy/v1/channels/{channel_id}/breakers/reset",
            ),
            "dispatch_proxy_channel_breaker_stats_request_to_axum_json_response(",
        ),
        (
            "reset_proxy_channel_breaker",
            function_slice(
                &source,
                "pub async fn reset_proxy_channel_breaker(",
                "/// POST /proxy/v1/route/resolve",
            ),
            "dispatch_reset_proxy_channel_breaker_request_to_axum_json_response(",
        ),
    ];

    let mut violations = Vec::new();
    for (handler_name, handler, adapter_marker) in handlers {
        if !handler.contains(adapter_marker) {
            violations.push(format!(
                "src/proxy/transport/http/handlers.rs {handler_name} should call `{adapter_marker}`"
            ));
        }

        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_MIGRATION_BREAKER_HANDLER_ENGINE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs {handler_name}:{} contains migration/breaker engine marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "migration and breaker handlers must delegate path request construction, engine calls, and JSON wrapping to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_status_and_desktop_model_handlers_delegate_json_bridge_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handlers = [
        (
            "health_check",
            function_slice(&source, "pub async fn health_check(", "/// 获取服务状态"),
            "proxy_health_check_to_axum_json_response(",
        ),
        (
            "get_status",
            function_slice(
                &source,
                "pub async fn get_status(",
                "/// GET /proxy/v1/events",
            ),
            "dispatch_proxy_status_request_to_axum_json_response(",
        ),
        (
            "handle_claude_desktop_models",
            function_slice(
                &source,
                "pub async fn handle_claude_desktop_models(",
                "\n}\n\n// ============================================================================\n// Codex API",
            ),
            "dispatch_claude_desktop_models_request_to_axum_json_response(",
        ),
    ];

    let mut violations = Vec::new();
    for (handler_name, handler, adapter_marker) in handlers {
        if !handler.contains(adapter_marker) {
            violations.push(format!(
                "src/proxy/transport/http/handlers.rs {handler_name} should call `{adapter_marker}`"
            ));
        }

        for (line_index, line) in production_lines(handler) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_STATUS_MODEL_HANDLER_ENGINE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/transport/http/handlers.rs {handler_name}:{} contains status/model engine marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "status and Claude Desktop model handlers must delegate response construction, engine calls, and JSON wrapping to response_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_adapter_registry_uses_core_app_policy() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider/mod.rs");
    let source = fs::read_to_string(&path).expect("read provider adapter registry");
    let get_adapter = function_slice(
        &source,
        "pub fn get_adapter(app_type: &AppType) -> Box<dyn ProviderAdapter> {",
        "\n}\n\n#[cfg(test)]",
    );

    assert!(
        get_adapter.contains("provider_adapter_kind_for_app_type(app_type)"),
        "provider adapter registry must delegate app-to-adapter selection to proxy_core_adapter"
    );

    let forbidden_markers = [
        "AppType::Claude",
        "AppType::ClaudeDesktop",
        "AppType::Codex",
        "AppType::Gemini",
        "AppType::OpenCode",
        "AppType::OpenClaw",
        "AppType::Hermes",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(get_adapter) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/mod.rs get_adapter:{} contains host app branch marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider adapter registry must not reimplement app-to-adapter selection in host code:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_provider_adapter_selection_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let adapter_function = function_slice(
        &source,
        "pub(crate) fn provider_adapter_kind_for_app_type(",
        "\n}\n\npub(crate) fn cc_switch_app_kinds()",
    );

    assert!(
        adapter_function.contains(
            "crate::proxy_core::api::domain::provider_adapter_kind_for_app(&AppKind::from(app_type))"
        ),
        "proxy_core_adapter must delegate app-to-adapter selection to proxy-core"
    );
}

#[test]
fn production_provider_adapters_delegate_base_url_errors_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let provider_paths = [
        "src/proxy/provider/claude.rs",
        "src/proxy/provider/codex.rs",
        "src/proxy/provider/gemini.rs",
    ];

    let mut violations = Vec::new();
    for relative in provider_paths {
        let path = manifest_dir.join(relative);
        let source = fs::read_to_string(&path).expect("read provider adapter source");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROVIDER_ADAPTER_BASE_URL_ERROR_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{}:{} contains base URL error marker `{}`",
                        relative,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider adapters must delegate required base_url extraction errors to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_gemini_provider_adapter_delegates_auth_info_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider/gemini.rs");
    let source = fs::read_to_string(&path).expect("read gemini provider adapter source");
    let extract_auth = function_slice(
        &source,
        "    fn extract_auth(&self, provider: &Provider)",
        "    fn build_url(&self, base_url: &str, endpoint: &str) -> String",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(extract_auth) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ADAPTER_AUTH_INFO_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/gemini.rs extract_auth:{} contains auth info marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Gemini provider adapter must delegate auth info construction to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_adapters_exclude_provider_kind_test_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let provider_paths = [
        "src/proxy/provider/claude.rs",
        "src/proxy/provider/gemini.rs",
    ];

    let mut violations = Vec::new();
    for relative in provider_paths {
        let source =
            fs::read_to_string(manifest_dir.join(relative)).expect("read provider adapter");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROVIDER_ADAPTER_TEST_FACADE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{}:{} contains test facade marker `{}`",
                        relative,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Provider kind and credential parser tests belong in proxy_core_adapter/proxy-core, not provider adapter cfg(test) facades:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_codex_provider_adapter_excludes_strategy_test_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let relative = "src/proxy/provider/codex.rs";
    let source = fs::read_to_string(manifest_dir.join(relative)).expect("read Codex provider");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CODEX_PROVIDER_ADAPTER_TEST_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "{}:{} contains Codex strategy test facade marker `{}`",
                    relative,
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Codex provider strategy tests must call proxy_core_adapter helpers directly instead of adding cfg(test) facades:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_codex_provider_adapter_delegates_auth_info_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider/codex.rs");
    let source = fs::read_to_string(&path).expect("read codex provider adapter source");
    let extract_auth = function_slice(
        &source,
        "    fn extract_auth(&self, provider: &Provider)",
        "    fn build_url(&self, base_url: &str, endpoint: &str) -> String",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(extract_auth) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ADAPTER_AUTH_INFO_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/codex.rs extract_auth:{} contains auth info marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Codex provider adapter must delegate auth info construction to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_claude_provider_adapter_delegates_auth_info_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider/claude.rs");
    let source = fs::read_to_string(&path).expect("read claude provider adapter source");
    let extract_auth = function_slice(
        &source,
        "    fn extract_auth(&self, provider: &Provider)",
        "    fn build_url(&self, base_url: &str, endpoint: &str) -> String",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(extract_auth) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ADAPTER_AUTH_INFO_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/claude.rs extract_auth:{} contains auth info marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude provider adapter must delegate auth info construction to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_simple_provider_adapters_delegate_auth_headers_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let provider_paths = [
        "src/proxy/provider/codex.rs",
        "src/proxy/provider/gemini.rs",
    ];

    let mut violations = Vec::new();
    for relative in provider_paths {
        let path = manifest_dir.join(relative);
        let source = fs::read_to_string(&path).expect("read provider adapter source");
        let get_auth_headers =
            function_slice(&source, "    fn get_auth_headers(", "\n}\n\n#[cfg(test)]");
        for (line_index, line) in production_lines(get_auth_headers) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROVIDER_ADAPTER_AUTH_HEADER_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{} get_auth_headers:{} contains auth header marker `{}`",
                        relative,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "simple provider adapters must delegate auth header construction and error text to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_claude_provider_adapter_delegates_auth_headers_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider/claude.rs");
    let source = fs::read_to_string(&path).expect("read claude provider adapter source");
    let get_auth_headers = function_slice(
        &source,
        "    fn get_auth_headers(",
        "    fn needs_transform(&self, provider: &Provider) -> bool",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(get_auth_headers) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ADAPTER_AUTH_HEADER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/claude.rs get_auth_headers:{} contains auth header marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude provider adapter must delegate auth header construction and error text to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_adapters_delegate_url_building_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let provider_paths = [
        "src/proxy/provider/claude.rs",
        "src/proxy/provider/codex.rs",
        "src/proxy/provider/gemini.rs",
    ];

    let mut violations = Vec::new();
    for relative in provider_paths {
        let path = manifest_dir.join(relative);
        let source = fs::read_to_string(&path).expect("read provider adapter source");
        let build_url = function_slice(
            &source,
            "    fn build_url(&self, base_url: &str, endpoint: &str) -> String",
            "    fn get_auth_headers(",
        );
        for (line_index, line) in production_lines(build_url) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROVIDER_ADAPTER_URL_BUILD_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{} build_url:{} contains URL build marker `{}`",
                        relative,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider adapters must delegate upstream URL building to proxy_core_adapter build helpers without direct core access or provider URL facades:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_excludes_provider_url_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_PROVIDER_URL_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains provider URL facade marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter should expose core upstream URL builders directly instead of provider_* one-line facades:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_excludes_router_channel_dto_bridge() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_ROUTER_CHANNEL_DTO_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains router channel DTO marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter should project DB channel records directly to route resolve inputs:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_excludes_dto_trait_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_DTO_TRAIT_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains DTO trait facade marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter should project DTOs through direct functions instead of single-impl trait facades:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_excludes_small_helper_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_SMALL_HELPER_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains small helper facade marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter should expose small pure core helpers directly instead of local one-line facades:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_generic_error_construction_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_DIRECT_ERROR_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains direct error construction marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must delegate generic ProxyCoreError construction to proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_http_client_delegates_explicit_proxy_url_validation_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/global_http_client.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/global_http_client.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HTTP_CLIENT_PROXY_URL_VALIDATION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/global_http_client.rs:{} contains explicit proxy URL validation marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production HTTP client must delegate explicit proxy URL parsing, scheme allowlist, and error projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_http_client_excludes_legacy_update_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/global_http_client.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/global_http_client.rs");

    let mut violations = Vec::new();
    for marker in ["pub fn update_proxy", "pub fn is_proxy_enabled"] {
        if source.contains(marker) {
            violations.push(format!(
                "src/proxy/host/cc_switch/global_http_client.rs keeps legacy global proxy facade `{marker}`"
            ));
        }
    }

    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        if code.contains("#[allow(dead_code)]") {
            violations.push(format!(
                "src/proxy/host/cc_switch/global_http_client.rs:{} keeps dead-code allowance",
                line_index + 1
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "production HTTP client must expose only active global proxy lifecycle operations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_http_client_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/http_client.rs");
    let source = fs::read_to_string(&path).expect("read http_client.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub use super::host::cc_switch::global_http_client::*;",
        ],
        "legacy proxy/http_client.rs must remain a re-export shim after host global HTTP client split"
    );
}

#[test]
fn proxy_core_adapter_delegates_explicit_proxy_url_validation_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_EXPLICIT_PROXY_URL_VALIDATION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains explicit proxy URL validation marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must delegate explicit proxy URL parsing, scheme allowlist, and error text to proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_error_mapper_delegates_response_parse_failure_log_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read proxy/error_mapper.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn parse_logged_upstream_json_or_unlabeled_sse",
        "pub(crate) fn parse_claude_transform_upstream_json_or_unlabeled_sse",
    );

    assert!(
        function.contains("parse_upstream_json_or_unlabeled_sse(")
            && function.contains("upstream_response_parse_failure_log_message(")
            && function.contains("response_body_parse_error_to_proxy_error("),
        "error_mapper should call core response parse/log helpers and keep host error mapping local"
    );

    let mut violations = Vec::new();
    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_RESPONSE_PARSE_LOG_MARKERS {
        if function.contains(marker) {
            violations.push(format!(
                "src/proxy/error_mapper.rs parse helper contains response parse log marker `{marker}`"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "error_mapper must delegate response parse failure log text and body projection to proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_excludes_error_mapper_transport_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for marker in [
        "parse_upstream_json_or_unlabeled_sse",
        "upstream_response_parse_failure_log_message",
        "UpstreamResponseParseFailureLogContext",
        "UnlabeledSseFallbackLogContext",
        "UpstreamJsonBodySource",
        "UnlabeledSseFallbackLogLevel",
        "log_unlabeled_sse_fallback_event",
        "upstream_send_error_projection",
        "UpstreamSendErrorInput",
    ] {
        if source.contains(marker) {
            violations.push(format!(
                "src/proxy_core_adapter.rs still exposes error_mapper transport facade `{marker}`"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter should not re-export transport helpers that are now owned by error_mapper:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_endpoint_service_delegates_projection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/services/provider/endpoints.rs");
    let source = fs::read_to_string(&path).expect("read provider endpoint service");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ENDPOINT_SERVICE_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/services/provider/endpoints.rs:{} contains endpoint projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider endpoint service must delegate custom endpoint normalization, sorting, and last-used mutation to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_custom_endpoint_url_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    assert!(
        source
            .contains("pub(crate) use crate::proxy_core::api::management::custom_endpoint_url_key"),
        "proxy_core_adapter should expose the core custom endpoint URL key helper"
    );

    let slice = function_slice(
        &source,
        "pub(crate) fn normalize_custom_endpoint_url",
        "pub(crate) fn mark_custom_endpoint_last_used",
    );
    assert!(
        slice.contains("crate::proxy_core::api::management::normalize_custom_endpoint_url")
            && slice.contains("custom_endpoint_url_issue_spec"),
        "proxy_core_adapter should delegate custom endpoint URL normalization and issue specs to core"
    );

    let mut violations = Vec::new();
    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_CUSTOM_ENDPOINT_URL_POLICY_MARKERS {
        if slice.contains(marker) {
            violations.push(*marker);
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep custom endpoint URL policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_codex_credential_value_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    assert!(
        source.contains("#[cfg(test)]\npub(crate) type ProviderCredentialValues")
            && source.contains("#[cfg(test)]\npub(crate) fn provider_credential_values"),
        "proxy_core_adapter credential value helper should remain test-only after production facade removal"
    );

    let slice = function_slice(
        &source,
        "pub(crate) fn provider_credential_values",
        "pub(crate) struct OpenCodeLiveProviderFragment",
    );
    assert!(
        slice.contains("core_provider_codex_credential_values_from_parts")
            && slice.contains("CodexCredentialParts"),
        "proxy_core_adapter should delegate Codex credential value policy to core"
    );

    let mut violations = Vec::new();
    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_CREDENTIAL_POLICY_MARKERS {
        if slice.contains(marker) {
            violations.push(*marker);
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Codex credential base_url parsing/error policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_service_excludes_credential_extract_facade() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/services/provider/mod.rs");
    let source = fs::read_to_string(&path).expect("read services/provider/mod.rs");
    let provider_service_impl = function_slice(
        &source,
        "impl ProviderService {",
        "#[derive(Debug, Clone, Deserialize)]",
    );

    assert!(
        !provider_service_impl.contains("fn extract_credentials(")
            && !provider_service_impl.contains("#[allow(dead_code)]"),
        "ProviderService production impl should not retain test-only credential extraction facades"
    );
}

#[test]
fn proxy_core_adapter_delegates_codex_base_url_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    let slice = function_slice(
        &source,
        "pub(crate) fn provider_codex_base_url",
        "pub(crate) fn required_codex_provider_base_url",
    );
    assert!(
        slice.contains("core_codex_base_url_from_settings"),
        "proxy_core_adapter should delegate Codex base URL extraction to core"
    );

    let mut violations = Vec::new();
    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_BASE_URL_POLICY_MARKERS {
        if slice.contains(marker) {
            violations.push(*marker);
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Codex base URL settings/config parsing in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_codex_config_toml_projection_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let production_source = production_lines(&source)
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        production_source.contains("codex_config_text_from_settings")
            && production_source.contains("core_codex_wire_api_from_config_toml")
            && production_source.contains("core_codex_model_from_config_toml")
            && production_source.contains("core_codex_config_has_base_url_matching"),
        "proxy_core_adapter should delegate Codex config text/wire_api/model/base_url projection to core"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_CONFIG_TOML_POLICY_MARKERS {
            if code.contains(marker) {
                violations.push(format!("line {}: {}", line_index + 1, marker));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Codex config TOML projection policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_codex_live_settings_shape_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let production_source = production_lines(&source)
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");

    for marker in [
        "codex_auth_object_value_from_settings(",
        "core_codex_provider_live_write_parts_from_settings(",
        "codex_restored_live_settings_parts",
        "core_codex_live_settings_parts_from_settings(",
        "core_codex_live_snapshot_parts_from_settings(",
        "core_codex_auth_has_oauth_login_material(",
    ] {
        assert!(
            production_source.contains(marker),
            "proxy_core_adapter should delegate Codex live/settings shape marker `{marker}` to core"
        );
    }

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_LIVE_SETTINGS_SHAPE_MARKERS {
            if code.contains(marker) {
                violations.push(format!("line {}: {}", line_index + 1, marker));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Codex live/settings JSON shape policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_provider_settings_validation_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let production_source = production_lines(&source)
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        production_source.contains("provider_settings_validation_issue_spec"),
        "proxy_core_adapter should expose provider settings validation issue specs through the core re-export"
    );
    let slice = function_slice(
        &source,
        "pub(crate) fn provider_settings_validation_parts",
        "pub(crate) enum CodexBackupProjectionIssue",
    );

    assert!(
        slice.contains("core_provider_settings_validation_parts_from_settings(")
            && slice.contains("AppKind::from(app_type)"),
        "proxy_core_adapter should delegate provider settings validation policy to core"
    );

    let mut violations = Vec::new();
    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_PROVIDER_SETTINGS_VALIDATION_MARKERS {
        if slice.contains(marker) {
            violations.push(*marker);
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep provider settings validation dispatch/spec policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_default_live_import_category_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let slice = function_slice(
        &source,
        "pub(crate) fn provider_from_default_live_settings",
        "pub(crate) fn codex_provider_live_write_parts",
    );

    assert!(
        slice
            .matches("core_provider_default_live_import_category_from_parts")
            .count()
            >= 1,
        "proxy_core_adapter should delegate default live import category decisions to core"
    );

    let mut violations = Vec::new();
    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_DEFAULT_LIVE_IMPORT_CATEGORY_MARKERS {
        if slice.contains(marker) {
            violations.push(*marker);
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep default live import category policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_codex_backfill_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let slice = function_slice(
        &source,
        "pub(crate) fn provider_codex_backfill_parts",
        "pub(crate) fn restore_codex_settings_for_provider_backfill",
    );

    assert!(
        slice.contains("core_codex_provider_backfill_parts_from_settings("),
        "proxy_core_adapter should delegate Codex provider backfill policy to core"
    );

    let mut violations = Vec::new();
    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_CODEX_BACKFILL_POLICY_MARKERS {
        if slice.contains(marker) {
            violations.push(*marker);
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Codex provider backfill policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_required_provider_base_url_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    let slice = function_slice(
        &source,
        "pub(crate) fn required_codex_provider_base_url",
        "fn provider_codex_config_text",
    );
    assert!(
        slice.matches("core_required_provider_base_url").count() == 3,
        "proxy_core_adapter should delegate required provider base URL errors to core for Codex/Gemini/Claude"
    );

    let mut violations = Vec::new();
    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_REQUIRED_BASE_URL_POLICY_MARKERS {
        if slice.contains(marker) {
            violations.push(*marker);
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep required provider base URL error policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_gemini_live_json_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let production_source = production_lines(&source)
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        production_source.contains("pub(crate) use crate::proxy_core::api::ports::{")
            && production_source.contains("gemini_live_settings_from_env_json_and_config")
            && production_source.contains("gemini_live_backup_from_effective_settings"),
        "proxy_core_adapter should expose Gemini live JSON helpers from core for live write/backup flows"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_GEMINI_LIVE_JSON_POLICY_MARKERS {
            if code.contains(marker) {
                violations.push(format!("line {}: {}", line_index + 1, marker));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Gemini live env/config JSON shape policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_gemini_live_config_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let production_source = production_lines(&source)
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        production_source.contains("core_gemini_live_config_object_from_settings")
            && production_source.contains(
                "pub(crate) use crate::proxy_core::api::ports::gemini_live_settings_to_write"
            ),
        "proxy_core_adapter should delegate Gemini live config selection/merge policy to core"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_GEMINI_LIVE_CONFIG_POLICY_MARKERS {
            if code.contains(marker) {
                violations.push(format!("line {}: {}", line_index + 1, marker));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Gemini live config selection/merge policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_claude_provider_adapter_delegates_transform_decision_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider/claude.rs");
    let source = fs::read_to_string(&path).expect("read claude provider adapter source");
    let needs_transform = function_slice(
        &source,
        "    fn needs_transform(&self, provider: &Provider) -> bool",
        "    fn transform_request(",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(needs_transform) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_TRANSFORM_DECISION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/claude.rs needs_transform:{} contains transform decision marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude provider adapter must delegate transform decision policy to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_claude_provider_adapter_excludes_compat_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider/claude.rs");
    let source = fs::read_to_string(&path).expect("read claude provider adapter source");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_COMPAT_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/claude.rs:{} contains Claude compat facade marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude provider adapter must not add host-local compat facades for adapter-owned policy:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_claude_provider_adapter_delegates_request_transforms_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider/claude.rs");
    let source = fs::read_to_string(&path).expect("read claude provider adapter source");
    let transform_request_helper = function_slice(
        &source,
        "fn transform_claude_request_for_api_format(",
        "/// Claude 适配器",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(transform_request_helper) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_REQUEST_TRANSFORM_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/claude.rs transform_claude_request_for_api_format:{} contains request transform marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude provider adapter must delegate request transform dispatch to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_claude_provider_adapter_delegates_message_normalization_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider/claude.rs");
    let source = fs::read_to_string(&path).expect("read claude provider adapter source");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_NORMALIZE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/claude.rs:{} contains message normalization marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude provider adapter must delegate message normalization policy to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_adapter_excludes_response_transform_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/proxy/provider/adapter.rs",
        "src/proxy/provider/claude.rs",
    ];

    let mut violations = Vec::new();
    for relative in files {
        let path = manifest_dir.join(relative);
        let source = fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {relative}"));
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains("fn transform_response(") {
                violations.push(format!(
                    "{relative}:{} contains provider response transform surface",
                    line_index + 1
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Provider adapters must not expose response transform methods; response dispatch belongs to proxy_core_adapter/core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_handlers_delegate_management_auth_decisions_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn require_proxy_management_auth",
        "/// GET /proxy/v1/apps",
    );

    assert!(
        handler.contains("validate_proxy_management_auth(&state, request.headers()).await?"),
        "management auth middleware must delegate auth validation to auth_adapter"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_MANAGEMENT_AUTH_DECISION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs require_proxy_management_auth:{} contains management auth decision marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "management auth middleware must delegate token-source decisions and engine calls to auth_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_response_processor_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/response_processor.rs");
    let source = fs::read_to_string(&path).expect("read response_processor.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub(crate) use super::engine::response_pipeline::*;",
        ],
        "legacy proxy/response_processor.rs must remain a re-export shim after engine/response_pipeline.rs split"
    );
}

#[test]
fn response_pipeline_owns_usage_provider_facts_projection() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_reexport = function_slice(
        &adapter_source,
        "#[cfg(test)]\n#[allow(unused_imports)]\npub(crate) use crate::proxy::engine::response_pipeline::{",
        "};\n\npub(crate) use crate::proxy_core::api::routing::{",
    );
    let function = function_slice(
        &source,
        "pub(crate) struct ResponseUsageProviderFacts",
        "pub(crate) struct StreamingResponseUsageContext",
    );

    assert!(
        function.contains("pub(crate) struct ResponseUsageProviderFacts")
            && function.contains("pub(crate) fn response_usage_provider_facts")
            && function.contains("pub(crate) fn fallback_response_usage_provider_facts")
            && function.contains("pub(crate) fn response_usage_provider_facts_from_optional")
            && function.contains("provider_kind_from_provider(provider)")
            && function.contains("AppKind::from(app_type)")
            && function.contains("usage_selected_provider_missing_log_message("),
        "response pipeline should own response usage provider facts projection"
    );
    for marker in FORBIDDEN_RESPONSE_PIPELINE_USAGE_RECORD_HELPER_MARKERS {
        assert!(
            !function.contains(marker),
            "response pipeline provider facts projection must not own usage record helper marker `{marker}`"
        );
    }
    assert!(
        adapter_reexport.contains("ResponseUsageProviderFacts")
            && adapter_reexport.contains("response_usage_provider_facts")
            && adapter_reexport.contains("fallback_response_usage_provider_facts")
            && adapter_reexport.contains("response_usage_provider_facts_from_optional")
            && !adapter_source.contains("pub(crate) struct ResponseUsageProviderFacts")
            && !adapter_source.contains("pub(crate) fn response_usage_provider_facts")
            && !adapter_source.contains("pub(crate) fn fallback_response_usage_provider_facts")
            && !adapter_source
                .contains("pub(crate) fn response_usage_provider_facts_from_optional"),
        "proxy_core_adapter should expose response usage provider facts only to adapter tests"
    );
}

#[test]
fn response_pipeline_owns_logged_stream_runtime_loop() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_reexport = function_slice(
        &adapter_source,
        "#[cfg(test)]\n#[allow(unused_imports)]\npub(crate) use crate::proxy::engine::response_pipeline::{",
        "};\n\npub(crate) use crate::proxy_core::api::routing::{",
    );
    let function = function_slice(
        &source,
        "pub(crate) struct SseUsageCollector",
        "pub(crate) fn passthrough_streaming_usage_collector",
    );

    assert!(
        function.contains("SseUsageAccumulator::new(")
            && function.contains("SseUsageFinishGuard")
            && function.contains("pub(crate) fn create_logged_passthrough_stream")
            && function.contains("async_stream::stream!")
            && function.contains("SsePassthroughStreamState::new()")
            && function.contains("tokio::time::timeout(")
            && function.contains("collector.finish().await")
            && function.contains("guard.disarm()"),
        "response pipeline should own logged stream runtime loop and collector finish guard"
    );
    for marker in FORBIDDEN_RESPONSE_PIPELINE_LOGGED_STREAM_POLICY_MARKERS {
        assert!(
            !function.contains(marker),
            "response pipeline logged-stream loop must delegate policy marker `{marker}`"
        );
    }
    assert!(
        !adapter_reexport.contains("create_logged_passthrough_stream")
            && !adapter_reexport.contains("SseUsageCollector")
            && !adapter_source.contains("pub(crate) struct SseUsageCollector")
            && !adapter_source.contains("struct SseUsageFinishGuard")
            && !adapter_source.contains("pub(crate) fn create_logged_passthrough_stream"),
        "proxy_core_adapter should not re-export logged stream runtime loop internals"
    );
}

#[test]
fn response_pipeline_owns_passthrough_stream_response_construction() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn passthrough_stream_proxy_response_from_context",
        "pub(crate) fn record_non_streaming_response_usage",
    );

    assert!(
        function.contains("log_streaming_proxy_response_received(&headers, status, ctx.tag)")
            && function.contains("create_passthrough_logged_stream(")
            && function
                .contains("passthrough_stream_proxy_response(status, headers, logged_stream)"),
        "response pipeline should own passthrough streaming response construction"
    );
    assert!(
        !function.contains("async_stream::stream!")
            && !function.contains("SsePassthroughStreamState::new()")
            && !function.contains("SseUsageFinishGuard")
            && !function.contains("tokio::time::timeout(duration, stream.next())"),
        "response pipeline should still delegate logged-stream internals"
    );
    assert!(
        !adapter_source.contains("passthrough_stream_proxy_response_from_context")
            && !adapter_source
                .contains("pub(crate) fn passthrough_stream_proxy_response_from_context"),
        "proxy_core_adapter should not re-export passthrough streaming construction"
    );
}

#[test]
fn response_pipeline_owns_passthrough_usage_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_reexport = function_slice(
        &adapter_source,
        "#[cfg(test)]\n#[allow(unused_imports)]\npub(crate) use crate::proxy::engine::response_pipeline::{",
        "};\n\npub(crate) use crate::proxy_core::api::routing::{",
    );
    let usage_slice = function_slice(
        &source,
        "pub(crate) struct StreamingResponseUsageContext",
        "pub(crate) fn transformed_streaming_usage_collector",
    );

    assert!(
        usage_slice.contains("pub(crate) struct StreamingUsageCollectorContext")
            && usage_slice.contains("pub(crate) fn streaming_usage_collector_from_context")
            && usage_slice.contains("SseUsageCollector::new(")
            && usage_slice.contains("streaming_response_usage_record_from_response_context(")
            && usage_slice.contains("spawn_usage_record_with_proxy_services(")
            && usage_slice.contains("pub(crate) struct NonStreamingUsageRecordContext")
            && usage_slice
                .contains("pub(crate) fn record_non_streaming_response_usage_from_context")
            && usage_slice.contains("non_streaming_response_usage_record_from_response_context(")
            && usage_slice.contains("UsageSelectedProviderMissingPhase::StreamingPassthrough")
            && usage_slice.contains("fn usage_logging_enabled_from_state(state: &ProxyState)")
            && usage_slice.contains("context.parser_config.stream_parser")
            && usage_slice.contains("context.parser_config.response_parser")
            && usage_slice.contains("output.log_event(context.body.len())"),
        "response pipeline should own passthrough usage runtime orchestration"
    );
    assert!(
        usage_slice.contains("pub(crate) fn streaming_response_usage_record_from_provider_facts")
            && usage_slice
                .contains("streaming_response_usage_record_with_optional_outbound_model(")
            && usage_slice
                .contains("pub(crate) fn streaming_response_usage_record_from_response_context")
            && usage_slice.contains(
                "non_streaming_response_usage_record_from_provider_body_with_request_id_fallback("
            )
            && usage_slice.contains(
                "pub(crate) fn non_streaming_response_usage_record_from_response_context"
            )
            && usage_slice.contains(
                "non_streaming_response_usage_record_from_body_with_request_id_fallback("
            )
            && usage_slice.contains("usage_record_with_route_context(")
            && !usage_slice.contains("success_usage_record_with_request_id_fallback("),
        "response pipeline should own bottom passthrough usage-record construction"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::engine::response_pipeline::{")
            && adapter_source.contains("StreamingUsageCollectorContext")
            && adapter_source.contains("streaming_usage_collector_from_context")
            && adapter_source.contains("NonStreamingUsageRecordContext")
            && adapter_source.contains("record_non_streaming_response_usage_from_context")
            && adapter_source.contains("streaming_response_usage_record_from_provider_facts")
            && adapter_source.contains("streaming_response_usage_record_from_response_context")
            && adapter_source
                .contains("non_streaming_response_usage_record_from_response_context")
            && adapter_source.contains(
                "non_streaming_response_usage_record_from_provider_body_with_request_id_fallback"
            )
            && adapter_source.contains("record_non_streaming_response_usage")
            && !adapter_reexport.contains("passthrough_streaming_usage_collector")
            && !adapter_reexport.contains("create_passthrough_logged_stream")
            && !adapter_source.contains("pub(crate) fn usage_logging_enabled_from_proxy_config")
            && !adapter_source.contains("pub(crate) struct StreamingUsageCollectorContext")
            && !adapter_source.contains("pub(crate) fn streaming_usage_collector_from_context")
            && !adapter_source.contains("pub(crate) struct NonStreamingUsageRecordContext")
            && !adapter_source
                .contains("pub(crate) fn record_non_streaming_response_usage_from_context")
            && !adapter_source
                .contains("pub(crate) fn streaming_response_usage_record_from_provider_facts")
            && !adapter_source
                .contains("pub(crate) fn streaming_response_usage_record_from_response_context")
            && !adapter_source.contains(
                "pub(crate) fn non_streaming_response_usage_record_from_provider_body_with_request_id_fallback"
            )
            && !adapter_source
                .contains("pub(crate) fn non_streaming_response_usage_record_from_response_context")
            && !adapter_source.contains("pub(crate) fn passthrough_streaming_usage_collector")
            && !adapter_source.contains("pub(crate) fn create_passthrough_logged_stream")
            && !adapter_source.contains("pub(crate) fn record_non_streaming_response_usage("),
        "proxy_core_adapter should re-export only externally needed passthrough usage runtime orchestration"
    );
}

#[test]
fn response_pipeline_owns_transformed_streaming_usage_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_reexport = function_slice(
        &adapter_source,
        "#[cfg(test)]\n#[allow(unused_imports)]\npub(crate) use crate::proxy::engine::response_pipeline::{",
        "};\n\npub(crate) use crate::proxy_core::api::routing::{",
    );
    let function = function_slice(
        &source,
        "pub(crate) struct TransformedResponseUsageContext",
        "pub(crate) fn claude_transform_tool_schema_hints",
    );

    assert!(
        function.contains("pub(crate) struct TransformedStreamingUsageCollectorContext")
            && function
                .contains("pub(crate) fn transformed_streaming_usage_collector_from_context")
            && function.contains("TransformedStreamingUsageCollectorContext {")
            && function
                .contains("transformed_streaming_response_usage_record_from_response_context(")
            && function.contains("spawn_usage_record_with_proxy_services_context(")
            && function.contains("pub(crate) struct TransformedResponseUsageRecordContext")
            && function.contains("pub(crate) fn record_transformed_response_usage_from_context")
            && function.contains("transformed_response_usage_record_from_response_context(")
            && function.contains("usage_logging_enabled_from_state(state)")
            && function.contains("state.proxy_core_services.clone()")
            && function.contains("TransformedResponseUsageFormat::Claude")
            && function.contains("TransformedResponseUsageFormat::CodexAuto")
            && function.contains("claude_stream_usage_event_filter")
            && function.contains("codex_stream_usage_event_filter")
            && function.contains("create_logged_passthrough_stream(")
            && function.contains("ctx.streaming_timeout_config()"),
        "response pipeline should own transformed usage runtime orchestration"
    );
    assert!(
        function.contains(
            "pub(crate) fn transformed_response_usage_record_from_provider_facts_with_request_id_fallback"
        )
            && function.contains("transformed_response_usage_record_with_request_id_fallback(")
            && function.contains(
                "pub(crate) fn transformed_streaming_response_usage_record_from_provider_facts_with_request_id_fallback"
            )
            && function.contains(
                "transformed_streaming_response_usage_record_with_request_id_fallback("
            )
            && function.contains("pub(crate) fn transformed_response_usage_record_from_response_context")
            && function
                .contains("pub(crate) fn transformed_streaming_response_usage_record_from_response_context")
            && function.contains("usage_record_with_route_context("),
        "response pipeline should own bottom transformed usage-record construction"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::engine::response_pipeline::{")
            && adapter_source.contains("TransformedResponseUsageRecordContext")
            && adapter_source.contains("record_transformed_response_usage_from_context")
            && adapter_source
                .contains("transformed_response_usage_record_from_provider_facts_with_request_id_fallback")
            && adapter_source.contains("transformed_response_usage_record_from_response_context")
            && adapter_source.contains(
                "transformed_streaming_response_usage_record_from_provider_facts_with_request_id_fallback"
            )
            && adapter_source
                .contains("transformed_streaming_response_usage_record_from_response_context")
            && !adapter_reexport.contains("TransformedStreamingUsageCollectorContext")
            && !adapter_reexport.contains("transformed_streaming_usage_collector_from_context")
            && !adapter_reexport.contains("transformed_streaming_usage_collector")
            && !adapter_reexport.contains("claude_transformed_streaming_usage_collector")
            && !adapter_reexport.contains("codex_auto_transformed_streaming_usage_collector")
            && !adapter_reexport.contains("create_claude_transformed_logged_stream")
            && !adapter_reexport.contains("create_codex_auto_transformed_logged_stream")
            && !adapter_source
                .contains("pub(crate) struct TransformedStreamingUsageCollectorContext")
            && !adapter_source
                .contains("pub(crate) fn transformed_streaming_usage_collector_from_context")
            && !adapter_source.contains("pub(crate) struct TransformedResponseUsageRecordContext")
            && !adapter_source
                .contains("pub(crate) fn record_transformed_response_usage_from_context")
            && !adapter_source.contains(
                "pub(crate) fn transformed_response_usage_record_from_provider_facts_with_request_id_fallback"
            )
            && !adapter_source
                .contains("pub(crate) fn transformed_response_usage_record_from_response_context")
            && !adapter_source.contains(
                "pub(crate) fn transformed_streaming_response_usage_record_from_provider_facts_with_request_id_fallback"
            )
            && !adapter_source.contains(
                "pub(crate) fn transformed_streaming_response_usage_record_from_response_context"
            )
            && !adapter_source.contains("pub(crate) fn transformed_streaming_usage_collector(")
            && !adapter_source.contains("pub(crate) fn create_claude_transformed_logged_stream(")
            && !adapter_source
                .contains("pub(crate) fn create_codex_auto_transformed_logged_stream("),
        "proxy_core_adapter should not re-export transformed streaming internals"
    );
}

#[test]
fn response_pipeline_owns_forward_error_usage_and_sink_scheduling() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let response_adapter_path = manifest_dir.join("src/proxy/response_adapter.rs");
    let response_adapter_source =
        fs::read_to_string(&response_adapter_path).expect("read proxy/response_adapter.rs");
    let adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse axum::response",
    );
    let adapter_import_identifiers: Vec<&str> = adapter_import
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|identifier| !identifier.is_empty())
        .collect();

    assert!(
        source.contains("pub(crate) async fn record_usage_with_proxy_services")
            && source.contains("pub(crate) fn spawn_usage_record_with_proxy_services")
            && source.contains("pub(crate) fn spawn_usage_record_with_proxy_services_context")
            && source.contains("record_usage_with_proxy_services_context(")
            && source.contains("tokio::spawn(async move")
            && source.contains(".usage_sink().record_usage(record).await")
            && source.contains("usage_record_debug_log_message(&record)")
            && source.contains("usage_record_failure_warning_message(failure_context, error)"),
        "response pipeline should own reusable usage sink scheduling"
    );
    assert!(
        source.contains("pub(crate) fn record_forward_error_usage(")
            && source.contains("pub(crate) fn record_forward_core_error_usage(")
            && source.contains("error_mapper::{")
            && source.contains("proxy_error_display_message")
            && source.contains("proxy_error_status_code")
            && source.contains("pub(crate) struct ForwardErrorUsageContext")
            && source.contains("pub(crate) struct ForwardErrorUsageRecordContext")
            && source.contains("pub(crate) fn record_forward_error_usage_from_context")
            && source.contains(
                "pub(crate) fn error_usage_record_from_provider_facts_with_request_id_fallback"
            )
            && source.contains("pub(crate) fn forward_error_usage_record_from_response_context")
            && source.contains("UsageRecordFailureLogContext::ForwardError")
            && source.contains("fallback_response_usage_provider_facts(")
            && source.contains("error_usage_record_with_request_id_fallback("),
        "response pipeline should own forward-error usage record orchestration"
    );
    let mut import_violations = Vec::new();
    for marker in ["proxy_error_display_message", "proxy_error_status_code"] {
        if adapter_import_identifiers
            .iter()
            .any(|identifier| identifier == &marker)
        {
            import_violations.push(format!(
                "response_pipeline still imports error projection marker `{marker}` from proxy_core_adapter"
            ));
        }
    }
    assert!(
        import_violations.is_empty(),
        "response_pipeline should route ProxyError display/status projection through error_mapper:\n{}",
        import_violations.join("\n")
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::engine::response_pipeline::{")
            && adapter_source.contains("record_usage_with_proxy_services")
            && adapter_source.contains("spawn_usage_record_with_proxy_services")
            && adapter_source.contains("spawn_usage_record_with_proxy_services_context")
            && adapter_source.contains("record_forward_error_usage")
            && !adapter_source.contains("record_forward_core_error_usage")
            && adapter_source.contains("ForwardErrorUsageContext")
            && adapter_source.contains("ForwardErrorUsageRecordContext")
            && adapter_source.contains("record_forward_error_usage_from_context")
            && adapter_source
                .contains("error_usage_record_from_provider_facts_with_request_id_fallback")
            && adapter_source.contains("forward_error_usage_record_from_response_context")
            && !adapter_source.contains("pub(crate) fn spawn_usage_record_with_proxy_services")
            && !adapter_source
                .contains("pub(crate) fn spawn_usage_record_with_proxy_services_context")
            && !adapter_source.contains("pub(crate) fn record_forward_error_usage(")
            && !adapter_source.contains("pub(crate) fn record_forward_core_error_usage(")
            && !adapter_source.contains("pub(crate) struct ForwardErrorUsageContext")
            && !adapter_source.contains("pub(crate) struct ForwardErrorUsageRecordContext")
            && !adapter_source.contains("pub(crate) fn record_forward_error_usage_from_context")
            && !adapter_source.contains(
                "pub(crate) fn error_usage_record_from_provider_facts_with_request_id_fallback"
            )
            && !adapter_source
                .contains("pub(crate) fn forward_error_usage_record_from_response_context"),
        "proxy_core_adapter should re-export, not own, forward-error usage and sink scheduling"
    );
    assert!(
        response_adapter_source.contains("engine::response_pipeline::{")
            && response_adapter_source.contains("record_forward_core_error_usage")
            && !function_slice(
                &response_adapter_source,
                "use crate::proxy_core_adapter::{",
                "};\nuse axum::",
            )
            .contains("record_forward_core_error_usage"),
        "response_adapter should import forward-core error usage mapping directly from response_pipeline"
    );
}

#[test]
fn response_pipeline_owns_transformed_sse_stream_wrappers() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_reexport = function_slice(
        &adapter_source,
        "#[cfg(test)]\n#[allow(unused_imports)]\npub(crate) use crate::proxy::engine::response_pipeline::{",
        "};\n\npub(crate) use crate::proxy_core::api::routing::{",
    );
    let response_adapter_path = manifest_dir.join("src/proxy/response_adapter.rs");
    let response_adapter_source =
        fs::read_to_string(&response_adapter_path).expect("read proxy/response_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn claude_transform_tool_schema_hints",
        "pub(crate) fn record_non_streaming_response_usage",
    );

    assert!(
        function.contains("pub(crate) struct ClaudeTransformedSseStreamContext")
            && function.contains("pub(crate) fn claude_transformed_sse_stream_from_context")
            && function.contains("pub(crate) struct CodexAutoTransformedSseStreamContext")
            && function.contains("pub(crate) fn codex_auto_transformed_sse_stream_from_context")
            && function.contains("provider_claude_transform_sse_for_api_format(")
            && function.contains("transform_codex_chat_sse_with_history(")
            && function.contains("create_claude_transformed_logged_stream(")
            && function.contains("create_codex_auto_transformed_logged_stream("),
        "response pipeline should own transformed SSE wrapper orchestration"
    );
    assert!(
        !function.contains("create_logged_passthrough_stream(")
            && !function.contains("SsePassthroughStreamState::new()")
            && !function.contains("async_stream::stream!")
            && !function
                .contains("transformed_streaming_response_usage_record_from_response_context(")
            && !function.contains("spawn_usage_record_with_proxy_services_context("),
        "response pipeline should keep shared logged-stream internals and usage records delegated"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::engine::response_pipeline::{")
            && !adapter_reexport.contains("claude_transform_tool_schema_hints")
            && !adapter_source.contains("ClaudeTransformedSseStreamContext")
            && !adapter_source.contains("claude_transformed_sse_stream_from_context")
            && !adapter_source.contains("CodexAutoTransformedSseStreamContext")
            && !adapter_source.contains("codex_auto_transformed_sse_stream_from_context")
            && !adapter_source.contains("fn claude_transform_tool_schema_hints(")
            && !adapter_source.contains("pub(crate) struct ClaudeTransformedSseStreamContext")
            && !adapter_source.contains("pub(crate) fn claude_transformed_sse_stream_from_context")
            && !adapter_source.contains("pub(crate) struct CodexAutoTransformedSseStreamContext")
            && !adapter_source
                .contains("pub(crate) fn codex_auto_transformed_sse_stream_from_context"),
        "proxy_core_adapter should not route transformed SSE wrapper orchestration"
    );
    assert!(
        response_adapter_source.contains("engine::response_pipeline::{")
            && response_adapter_source.contains("ClaudeTransformedSseStreamContext")
            && response_adapter_source.contains("claude_transformed_sse_stream_from_context")
            && response_adapter_source.contains("CodexAutoTransformedSseStreamContext")
            && response_adapter_source.contains("codex_auto_transformed_sse_stream_from_context")
            && !function_slice(
                &response_adapter_source,
                "use crate::proxy_core_adapter::{",
                "};\nuse axum::",
            )
            .contains("TransformedSseStreamContext"),
        "response_adapter should import transformed SSE wrappers directly from response_pipeline"
    );
}

#[test]
fn response_pipeline_owns_transformed_json_response_wrappers() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let response_adapter_path = manifest_dir.join("src/proxy/response_adapter.rs");
    let response_adapter_source =
        fs::read_to_string(&response_adapter_path).expect("read proxy/response_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn record_transformed_response_usage(",
        "pub(crate) fn record_non_streaming_response_usage",
    );

    assert!(
        function.contains("TransformedResponseUsageRecordContext {")
            && function.contains("usage_logging_enabled_from_state(state)")
            && function.contains("state.proxy_core_services.clone()")
            && function.contains("pub(crate) struct ClaudeTransformedJsonResponseContext")
            && function.contains("pub(crate) fn claude_transformed_json_response_from_context")
            && function.contains("provider_claude_transform_response_for_api_format(")
            && function.contains("TransformedResponseUsageFormat::Claude")
            && function.contains("pub(crate) struct CodexAutoTransformedJsonResponseContext")
            && function.contains("pub(crate) async fn codex_auto_transformed_json_response_from_context")
            && function.contains("transform_codex_chat_response_with_history(")
            && function.contains("TransformedResponseUsageFormat::CodexAuto"),
        "response pipeline should own transformed JSON wrapper orchestration and usage runtime source"
    );
    assert!(
        !function.contains("transformed_response_usage_record_from_response_context(")
            && !function.contains("spawn_usage_record_with_proxy_services_context(")
            && !function.contains("create_logged_passthrough_stream(")
            && !function.contains("SsePassthroughStreamState::new()")
            && !function.contains("async_stream::stream!"),
        "response pipeline should delegate transformed usage record construction and stream internals"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::engine::response_pipeline::{")
            && adapter_source.contains("record_transformed_response_usage")
            && adapter_source.contains("record_claude_transformed_response_usage")
            && adapter_source.contains("record_codex_auto_transformed_response_usage")
            && !adapter_source.contains("ClaudeTransformedJsonResponseContext")
            && !adapter_source.contains("claude_transformed_json_response_from_context")
            && !adapter_source.contains("CodexAutoTransformedJsonResponseContext")
            && !adapter_source.contains("codex_auto_transformed_json_response_from_context")
            && !adapter_source.contains("pub(crate) fn record_transformed_response_usage(")
            && !adapter_source.contains("pub(crate) struct ClaudeTransformedJsonResponseContext")
            && !adapter_source
                .contains("pub(crate) fn claude_transformed_json_response_from_context")
            && !adapter_source
                .contains("pub(crate) struct CodexAutoTransformedJsonResponseContext")
            && !adapter_source
                .contains("pub(crate) async fn codex_auto_transformed_json_response_from_context"),
        "proxy_core_adapter should not route transformed JSON wrapper orchestration"
    );
    assert!(
        response_adapter_source.contains("engine::response_pipeline::{")
            && response_adapter_source.contains("ClaudeTransformedJsonResponseContext")
            && response_adapter_source.contains("claude_transformed_json_response_from_context")
            && response_adapter_source.contains("CodexAutoTransformedJsonResponseContext")
            && response_adapter_source
                .contains("codex_auto_transformed_json_response_from_context")
            && !function_slice(
                &response_adapter_source,
                "use crate::proxy_core_adapter::{",
                "};\nuse axum::",
            )
            .contains("TransformedJsonResponseContext"),
        "response_adapter should import transformed JSON wrappers directly from response_pipeline"
    );
}

#[test]
fn response_pipeline_owns_body_decode_transport_bridge() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let response_adapter_path = manifest_dir.join("src/proxy/response_adapter.rs");
    let response_adapter_source =
        fs::read_to_string(&response_adapter_path).expect("read proxy/response_adapter.rs");
    let decode_slice = function_slice(
        &source,
        "pub(crate) struct DecodedProxyResponseBody",
        "/// 检测响应是否为 SSE 流式响应",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_RESPONSE_PROCESSOR_BODY_DECODE_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/response_pipeline.rs:{} contains body decode projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "response processor must keep decode projection text in proxy-core and avoid local message formatting:\n{}",
        violations.join("\n")
    );
    assert!(
        decode_slice.contains("pub(crate) fn decode_raw_proxy_response_body")
            && decode_slice.contains("pub(crate) async fn read_decoded_proxy_response_body")
            && decode_slice.contains("response.bytes()")
            && decode_slice.contains("tokio::time::timeout(")
            && decode_slice.contains("non_streaming_body_timeout_message(")
            && decode_slice.contains("decode_response_body(")
            && decode_slice.contains("ResponseBodyDecodeLogLevel::"),
        "response pipeline should own the non-streaming transport body read/decode bridge"
    );
    assert!(
        !adapter_source.contains("read_decoded_proxy_response_body")
            && !adapter_source.contains("DecodedProxyResponseBody")
            && !adapter_source.contains("decode_raw_proxy_response_body")
            && !adapter_source.contains("pub(crate) struct DecodedProxyResponseBody")
            && !adapter_source.contains("pub(crate) fn decode_raw_proxy_response_body"),
        "proxy_core_adapter should not re-export response body decode bridge helpers"
    );
    assert!(
        response_adapter_source.contains("engine::response_pipeline::{")
            && response_adapter_source.contains("read_decoded_proxy_response_body"),
        "response_adapter should import response body decode bridge helpers directly from response_pipeline"
    );
    let direct_core_refs: Vec<String> = production_lines(&source)
        .filter_map(|(line_index, line)| {
            let code = line.split("//").next().unwrap_or_default();
            (code.contains("crate::proxy_core::")
                && !code.contains("crate::proxy_core::api::transport")
                && !code.contains("crate::proxy_core::api::usage")
                && !code.contains("crate::proxy_core::api::config")
                && !code.contains("crate::proxy_core::api::domain")
                && !code.contains("crate::proxy_core::api::errors")
                && !code.contains("crate::proxy_core::api::ports")
                && !code.contains("crate::proxy_core::api::transforms"))
            .then(|| {
                format!(
                    "src/proxy/engine/response_pipeline.rs:{} contains non-response proxy-core marker",
                    line_index + 1
                )
            })
        })
        .collect();
    assert!(
        direct_core_refs.is_empty(),
        "response pipeline direct proxy-core access should stay limited to response-facing core config/domain/error/ports/transport/transforms/usage APIs:\n{}",
        direct_core_refs.join("\n")
    );
}

#[test]
fn response_pipeline_owns_core_usage_transport_imports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse axum::response",
    );
    let adapter_import_identifiers: Vec<&str> = adapter_import
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|identifier| !identifier.is_empty())
        .collect();

    assert!(
        source.contains("use crate::proxy_core::api::config::StreamingTimeoutConfig;")
            && source.contains("use crate::proxy_core::api::domain::{AppKind, ProviderKind};")
            && source.contains("use crate::proxy_core::api::ports::ProxyServices;")
            && source.contains("use crate::proxy_core::api::transport::{")
            && source.contains("passthrough_bytes_proxy_response")
            && source.contains("passthrough_stream_proxy_response")
            && source.contains("response_headers_indicate_sse")
            && source.contains("ProxyCoreResponse")
            && source.contains("ProxyResponseBuildErrorContext as AxumResponseBuildErrorContext")
            && source.contains("use crate::proxy_core::api::usage::{")
            && source.contains("usage_logging_enabled_from_config_flag")
            && source.contains("usage_selected_provider_missing_log_message")
            && source.contains("StreamUsageEventFilter")
            && source.contains("TransformedResponseUsageFormat")
            && source.contains("UsageParserConfig")
            && source.contains("UsageRecordFailureLogContext")
            && source.contains("UsageSelectedProviderMissingPhase")
            && source.contains("use crate::proxy_core::api::transforms::{")
            && source.contains("claude_stream_usage_event_filter")
            && source.contains("codex_stream_usage_event_filter")
            && source.contains("extract_anthropic_tool_schema_hints")
            && source.contains("AnthropicToolSchemaHints")
            && source.contains("CodexToolContext")
            && source.contains("SsePassthroughStreamState")
            && source.contains("SseUsageAccumulator"),
        "response_pipeline should import pure core response contracts directly"
    );

    let mut violations = Vec::new();
    for marker in [
        "AppKind",
        "ProviderKind",
        "ProxyServices",
        "response_headers_indicate_sse",
        "ProxyCoreResponse",
        "passthrough_bytes_proxy_response",
        "passthrough_stream_proxy_response",
        "AxumResponseBuildErrorContext",
        "ProxyResponseBuildErrorContext",
        "StreamUsageEventFilter",
        "StreamingTimeoutConfig",
        "TokenUsage",
        "TransformedResponseUsageFormat",
        "UsageParserConfig",
        "UsageRecordFailureLogContext",
        "UsageRouteContext",
        "UsageSelectedProviderMissingPhase",
        "usage_logging_enabled_from_config_flag",
        "usage_selected_provider_missing_log_message",
        "claude_stream_usage_event_filter",
        "codex_stream_usage_event_filter",
        "extract_anthropic_tool_schema_hints",
        "AnthropicToolSchemaHints",
        "CodexToolContext",
        "SsePassthroughStreamState",
        "SseUsageAccumulator",
    ] {
        if adapter_import_identifiers
            .iter()
            .any(|identifier| identifier == &marker)
        {
            violations.push(format!(
                "response_pipeline still imports pure core marker `{marker}` from proxy_core_adapter"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "response_pipeline should not route pure core usage/transport contracts through proxy_core_adapter:\n{}",
        violations.join("\n")
    );
    assert!(
        !adapter_source.contains("pub(crate) type SsePassthroughStreamState")
            && !adapter_source.contains("pub(crate) type SseUsageAccumulator")
            && !adapter_source.contains("claude_stream_usage_event_filter,")
            && !adapter_source.contains("codex_stream_usage_event_filter,")
            && !adapter_source.contains("extract_anthropic_tool_schema_hints,"),
        "proxy_core_adapter should not keep response-pipeline-only SSE usage state/filter or tool-schema extractor shims"
    );
}

#[test]
fn response_pipeline_keeps_response_log_projection_text_in_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        let markers = FORBIDDEN_RESPONSE_PROCESSOR_RESPONSE_LOG_PROJECTION_MARKERS
            .iter()
            .chain(FORBIDDEN_RESPONSE_PROCESSOR_RESPONSE_LOG_CALL_MARKERS.iter());
        for marker in markers {
            if code.contains(*marker) {
                violations.push(format!(
                    "src/proxy/engine/response_pipeline.rs:{} contains response log projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "response pipeline must keep response log projection text in proxy-core event specs:\n{}",
        violations.join("\n")
    );
}

#[test]
fn response_pipeline_delegates_response_log_projection_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn decode_raw_proxy_response_body",
        "/// 检测响应是否为 SSE 流式响应",
    );

    assert!(
        function.contains("non_streaming_response_received_log_event(")
            && function.contains("streaming_response_received_log_events(")
            && function.contains("non_streaming_response_body_log_event(")
            && function.contains("emit_response_log_event("),
        "response pipeline must consume response log event specs from proxy-core"
    );

    for marker in FORBIDDEN_RESPONSE_PROCESSOR_RESPONSE_LOG_PROJECTION_MARKERS {
        assert!(
            !function.contains(marker),
            "response pipeline must not locally format response log projection marker `{marker}`"
        );
    }
}

#[test]
fn response_pipeline_delegates_sse_passthrough_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn create_logged_passthrough_stream",
        "pub(crate) fn passthrough_streaming_usage_collector",
    );

    assert!(
        function.contains("SsePassthroughStreamState::new()")
            && function.contains(".inspect_chunk("),
        "response_pipeline must use proxy-core SSE passthrough state for chunk policy"
    );

    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_SSE_PASSTHROUGH_POLICY_MARKERS {
        assert!(
            !function.contains(marker),
            "response_pipeline must not locally project SSE passthrough policy marker `{marker}`"
        );
    }
}

#[test]
fn response_pipeline_uses_core_axum_build_context_policy() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let response_pipeline_source =
        fs::read_to_string(manifest_dir.join("src/proxy/engine/response_pipeline.rs"))
            .expect("read engine/response_pipeline.rs");
    let files = [
        ("src/proxy/transport/http/handlers.rs", "read handlers.rs"),
        (
            "src/proxy/engine/response_pipeline.rs",
            "read engine/response_pipeline.rs",
        ),
    ];

    let mut violations = Vec::new();
    for (relative_path, read_context) in files {
        let source = fs::read_to_string(manifest_dir.join(relative_path)).expect(read_context);
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_RESPONSE_BUILD_CONTEXT_LITERAL_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{}:{} contains response build context literal `{}`",
                        relative_path,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        response_pipeline_source
            .contains("ProxyResponseBuildErrorContext as AxumResponseBuildErrorContext"),
        "response_pipeline should import Axum response build context from proxy_core::api::transport"
    );
    assert!(
        violations.is_empty(),
        "response handlers must delegate Axum response build context policy to proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn response_adapter_delegates_build_error_message_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/response_adapter.rs");
    let source = fs::read_to_string(&path).expect("read response_adapter.rs");

    assert!(
        source.contains(".internal_error_prefix()"),
        "response_adapter should use proxy-core response build error message prefixes"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_RESPONSE_ADAPTER_BUILD_ERROR_MESSAGE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/response_adapter.rs:{} contains response build error message marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "response_adapter must delegate response build error message policy to proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_excludes_response_build_context_facade() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    assert!(
        !source.contains("AxumResponseBuildErrorContext")
            && !source.contains("CoreResponseBuildFailureContext")
            && !source.contains("ProxyResponseBuildErrorContext")
            && !source.contains("ProxyResponseBuildFailureContext"),
        "proxy_core_adapter should no longer expose response build contexts; response_pipeline/response_adapter/error_mapper import them directly"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_RESPONSE_BUILD_CONTEXT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains response build context marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must not keep response build context message policy facades:\n{}",
        violations.join("\n")
    );
}

#[test]
fn error_mapper_delegates_response_build_failure_context_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read error_mapper.rs");

    assert!(
        source.contains("CoreResponseBuildFailureContext"),
        "error_mapper should consume the core response build failure context"
    );
    assert!(
        source.contains("ProxyResponseBuildFailureContext as CoreResponseBuildFailureContext"),
        "error_mapper should import response build failure context from proxy_core::api::transport"
    );
    assert!(
        source.contains(".log_prefix()"),
        "error_mapper should use proxy-core response build failure log prefixes"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_ERROR_MAPPER_RESPONSE_BUILD_FAILURE_CONTEXT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/error_mapper.rs:{} contains response build failure context marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "error_mapper must delegate response build failure context policy to proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn error_mapper_delegates_response_transform_failure_context_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read error_mapper.rs");

    assert!(
        source.contains("CoreResponseTransformFailureContext"),
        "error_mapper should consume the core response transform failure context"
    );
    assert!(
        source
            .contains("ProxyResponseTransformFailureContext as CoreResponseTransformFailureContext"),
        "error_mapper should import response transform failure context from proxy_core::api::transforms"
    );
    assert!(
        source.contains(".log_prefix()"),
        "error_mapper should use proxy-core response transform failure log prefixes"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_ERROR_MAPPER_RESPONSE_TRANSFORM_FAILURE_CONTEXT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/error_mapper.rs:{} contains response transform failure context marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "error_mapper must delegate response transform failure context policy to proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_claude_desktop_provider_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    assert!(
        source.contains("ClaudeDesktopProviderValidationInput"),
        "proxy_core_adapter should project Provider facts into the core Claude Desktop validation input"
    );
    assert!(
        source.contains("claude_desktop_proxy_has_base_url_and_key(")
            && source.contains("claude_desktop_provider_models_are_profile_safe(")
            && source.contains("claude_desktop_direct_provider_validation_issue(")
            && source.contains("claude_desktop_proxy_provider_config_validation_issue(")
            && source.contains("claude_desktop_suggested_proxy_routes(")
            && source.contains("claude_desktop_gateway_token_error")
            && source.contains("claude_desktop_provider_selection_error")
            && source.contains("claude_desktop_provider_unavailable_error"),
        "proxy_core_adapter should delegate Claude Desktop provider validation policy to core"
    );
    assert!(
        !source.contains("enum ClaudeDesktopDirectProviderValidationIssue")
            && !source.contains("enum ClaudeDesktopProxyProviderConfigValidationIssue"),
        "proxy_core_adapter must re-export Claude Desktop validation issue types from core"
    );

    let policy_slices = [
        function_slice(
            &source,
            "pub(crate) fn provider_claude_models_are_claude_safe",
            "pub(crate) fn provider_claude_desktop_suggested_proxy_routes",
        ),
        function_slice(
            &source,
            "pub(crate) fn provider_claude_desktop_suggested_proxy_routes",
            "pub(crate) fn provider_claude_desktop_proxy_has_base_url_and_key",
        ),
        function_slice(
            &source,
            "pub(crate) fn provider_claude_desktop_proxy_has_base_url_and_key",
            "pub(crate) fn provider_claude_desktop_direct_validation_issue",
        ),
        function_slice(
            &source,
            "pub(crate) fn provider_claude_desktop_direct_validation_issue",
            "pub(crate) fn provider_claude_desktop_proxy_config_validation_issue",
        ),
        function_slice(
            &source,
            "pub(crate) fn provider_claude_desktop_proxy_config_validation_issue",
            "fn claude_desktop_provider_validation_input",
        ),
        function_slice(
            &source,
            "pub(crate) fn claude_desktop_provider_from_selection_result",
            "pub(crate) async fn claude_desktop_model_routes_from_router_source",
        ),
    ];
    let forbidden_markers = [
        "\"ANTHROPIC_AUTH_TOKEN\"",
        "\"ANTHROPIC_API_KEY\"",
        "\"OPENROUTER_API_KEY\"",
        "\"OPENAI_API_KEY\"",
        "\"GEMINI_API_KEY\"",
        "Some(\"github_copilot\") | Some(\"codex_oauth\")",
        "\"openai_chat\" | \"openai_responses\" | \"gemini_native\"",
        "settings_config.is_object()",
        "\"select claude desktop provider:",
        "\"no available claude desktop provider\"",
        "\"ANTHROPIC_MODEL\"",
        "\"ANTHROPIC_DEFAULT_HAIKU_MODEL\"",
        "\"ANTHROPIC_DEFAULT_SONNET_MODEL\"",
        "\"ANTHROPIC_DEFAULT_OPUS_MODEL\"",
        "crate::claude_desktop_config::is_claude_safe_model_id",
        "ProxyCoreError::Internal(",
        "ProxyCoreError::Unavailable(",
    ];

    let mut violations = Vec::new();
    for slice in policy_slices {
        for (line_index, line) in production_lines(slice) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy_core_adapter.rs:{} contains Claude Desktop provider policy marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop provider key/api_format/provider_type policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn provider_command_delegates_claude_desktop_route_suggestions_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/commands/provider.rs");
    let source = fs::read_to_string(&path).expect("read commands/provider.rs");
    let suggestion_slice = function_slice(
        &source,
        "pub(crate) fn suggested_claude_desktop_routes(",
        "#[allow(non_snake_case)]",
    );

    assert!(
        suggestion_slice.contains("provider_claude_desktop_suggested_proxy_routes("),
        "commands/provider should delegate Claude Desktop route suggestion policy through proxy_core_adapter"
    );

    let forbidden_markers = [
        "provider_claude_env_settings",
        "provider_claude_desktop_routes_support_1m_by_default",
        "ONE_M_CONTEXT_MARKER",
        "is_claude_safe_model_id(",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "_NAME",
        "values_mut()",
        "supports_1m_default",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(suggestion_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/commands/provider.rs suggested_claude_desktop_routes:{} contains route suggestion policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "commands/provider must keep Claude Desktop route suggestion policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn provider_command_delegates_claude_desktop_import_decision_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/commands/provider.rs");
    let source = fs::read_to_string(&path).expect("read commands/provider.rs");
    let import_slice = function_slice(
        &source,
        "pub fn import_claude_desktop_providers_from_claude(",
        "pub fn ensure_claude_desktop_official_provider(",
    );

    assert!(
        import_slice.contains("provider_claude_desktop_import_decision(")
            && import_slice.contains("ClaudeDesktopProviderImportDecision::Direct")
            && import_slice.contains("ClaudeDesktopProviderImportDecision::Proxy")
            && import_slice.contains("ClaudeDesktopProviderImportDecision::Skip"),
        "commands/provider should delegate Claude Desktop import mode/route decision to proxy_core_adapter"
    );

    let forbidden_markers = [
        "is_compatible_direct_provider(",
        "provider_claude_models_are_claude_safe(",
        "suggested_claude_desktop_routes(provider)",
        "provider_claude_desktop_suggested_proxy_routes(",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_MODEL",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(import_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/commands/provider.rs import_claude_desktop_providers_from_claude:{} contains import decision policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "commands/provider must keep Claude Desktop import decision policy in proxy_core_adapter/core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_import_decision_policy() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let import_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_import_decision(",
        "pub(crate) fn provider_claude_desktop_proxy_has_base_url_and_key(",
    );

    assert!(
        import_slice.contains("provider_claude_desktop_direct_importable(")
            && import_slice.contains("provider_claude_desktop_suggested_proxy_routes(")
            && import_slice.contains("ClaudeDesktopProviderImportDecision::Direct")
            && import_slice.contains("ClaudeDesktopProviderImportDecision::Proxy")
            && import_slice.contains("ClaudeDesktopProviderImportDecision::Skip"),
        "proxy_core_adapter should own Claude Desktop import direct/proxy/skip projection"
    );

    let forbidden_markers = [
        "validate_direct_provider(",
        "is_compatible_direct_provider(",
        "crate::claude_desktop_config",
        "state.db",
        "save_provider(",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(import_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_import_decision:{} contains host-owned import marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop import policy free of command/db/file side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_status_provider_facts() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let status_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_status_facts(",
        "pub(crate) fn provider_claude_desktop_proxy_has_base_url_and_key(",
    );

    assert!(
        status_slice.contains("provider_claude_desktop_mode(")
            && status_slice.contains("claude_desktop_direct_gateway_credentials(")
            && status_slice.contains("provider_claude_desktop_proxy_routes_missing("),
        "proxy_core_adapter should own Claude Desktop status provider-derived facts"
    );

    let forbidden_markers = [
        "proxy_gateway_base_url_from_db",
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(status_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_status_facts:{} contains host-owned status marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop status provider facts free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_provider_mode_policy() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let mode_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_mode(",
        "fn provider_claude_desktop_proxy_routes_missing(",
    );

    assert!(
        mode_slice.contains("claude_desktop_mode.clone()")
            && mode_slice.contains("ClaudeDesktopMode::Direct"),
        "proxy_core_adapter should own Claude Desktop provider mode defaulting policy"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
        "proxy_gateway_base_url_from_db",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(mode_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_mode:{} contains host-owned mode marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop provider mode policy free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_direct_model_specs_provider_projection() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let direct_specs_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_direct_inference_model_specs(",
        "#[derive(Debug, Clone, PartialEq, Eq)]",
    );

    assert!(
        direct_specs_slice.contains("claude_desktop_direct_inference_model_specs(")
            && direct_specs_slice.contains("ClaudeDesktopProxyRouteInput")
            && direct_specs_slice.contains("ClaudeDesktopGatewayProfileModelSpec::from"),
        "proxy_core_adapter should own Claude Desktop direct model route projection from Provider"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
        "proxy_gateway_base_url_from_db",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(direct_specs_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_direct_inference_model_specs:{} contains host-owned direct specs marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop direct model specs projection free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_direct_gateway_profile() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let direct_profile_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_direct_gateway_profile(",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
    );

    assert!(
        direct_profile_slice.contains("claude_desktop_direct_gateway_credentials(")
            && direct_profile_slice
                .contains("provider_claude_desktop_direct_inference_model_specs(")
            && direct_profile_slice.contains("claude_desktop_gateway_profile(")
            && direct_profile_slice
                .contains("ClaudeDesktopProviderDirectGatewayProfileIssue::Credentials")
            && direct_profile_slice
                .contains("ClaudeDesktopProviderDirectGatewayProfileIssue::ModelRoute"),
        "proxy_core_adapter should own Claude Desktop Direct provider profile assembly"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
        "proxy_gateway_base_url_from_db",
        "get_or_create_gateway_token",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(direct_profile_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_direct_gateway_profile:{} contains host-owned direct profile marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop Direct profile assembly free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_direct_provider_validation() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let direct_validation_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_direct_provider_validation(",
        "pub(crate) fn provider_claude_desktop_direct_inference_model_specs(",
    );

    assert!(
        direct_validation_slice.contains("provider_claude_desktop_direct_validation_issue(")
            && direct_validation_slice
                .contains("provider_claude_desktop_direct_inference_model_specs(")
            && direct_validation_slice.contains("claude_desktop_direct_gateway_credentials(")
            && direct_validation_slice
                .contains("ClaudeDesktopProviderDirectValidationIssue::Provider")
            && direct_validation_slice
                .contains("ClaudeDesktopProviderDirectValidationIssue::ModelRoute")
            && direct_validation_slice
                .contains("ClaudeDesktopProviderDirectValidationIssue::Credentials"),
        "proxy_core_adapter should own Claude Desktop Direct provider validation assembly"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
        "proxy_gateway_base_url_from_db",
        "get_or_create_gateway_token",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(direct_validation_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_direct_provider_validation:{} contains host-owned direct validation marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop Direct validation assembly free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_proxy_provider_validation() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let proxy_validation_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_proxy_provider_validation(",
        "pub(crate) fn provider_claude_desktop_proxy_gateway_profile_model_specs(",
    );

    assert!(
        proxy_validation_slice.contains("provider_claude_desktop_proxy_config_validation_issue(")
            && proxy_validation_slice.contains("provider_claude_desktop_proxy_model_routes(")
            && proxy_validation_slice
                .contains("provider_claude_desktop_proxy_has_base_url_and_key(")
            && proxy_validation_slice.contains("ClaudeDesktopProviderProxyValidationIssue::Config")
            && proxy_validation_slice
                .contains("ClaudeDesktopProviderProxyValidationIssue::ModelRoutes")
            && proxy_validation_slice
                .contains("ClaudeDesktopProviderProxyValidationIssue::CredentialsMissing"),
        "proxy_core_adapter should own Claude Desktop Proxy provider validation assembly"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
        "proxy_gateway_base_url_from_db",
        "get_or_create_gateway_token",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(proxy_validation_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_proxy_provider_validation:{} contains host-owned proxy validation marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop Proxy validation assembly free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_provider_validation_dispatch() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let validation_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_provider_validation(",
        "pub(crate) fn provider_claude_desktop_proxy_gateway_profile_model_specs(",
    );

    assert!(
        validation_slice.contains("provider_claude_desktop_mode(provider)")
            && validation_slice.contains("provider_claude_desktop_direct_provider_validation(")
            && validation_slice.contains("provider_claude_desktop_proxy_provider_validation(")
            && validation_slice.contains("ClaudeDesktopProviderValidationIssue::Direct")
            && validation_slice.contains("ClaudeDesktopProviderValidationIssue::Proxy"),
        "proxy_core_adapter should own Claude Desktop provider validation mode dispatch"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
        "proxy_gateway_base_url_from_db",
        "get_or_create_gateway_token",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(validation_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_provider_validation:{} contains host-owned validation dispatch marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop provider validation dispatch free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_proxy_model_routes_provider_projection() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let proxy_routes_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_proxy_model_routes(",
        "#[derive(Debug, Clone, PartialEq, Eq)]",
    );

    assert!(
        proxy_routes_slice.contains("claude_desktop_proxy_model_routes(")
            && proxy_routes_slice.contains("ClaudeDesktopProxyRouteInput")
            && proxy_routes_slice.contains("ClaudeDesktopProviderProxyRouteIssue::Missing")
            && proxy_routes_slice.contains("ClaudeDesktopProviderProxyRouteIssue::Empty"),
        "proxy_core_adapter should own Claude Desktop proxy route projection from Provider"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
        "proxy_gateway_base_url_from_db",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(proxy_routes_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_proxy_model_routes:{} contains host-owned proxy route marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop proxy route projection free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_proxy_gateway_profile_model_specs() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let profile_specs_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_proxy_gateway_profile_model_specs(",
        "#[derive(Debug, Clone, PartialEq, Eq)]",
    );

    assert!(
        profile_specs_slice.contains("provider_claude_desktop_proxy_model_routes(")
            && profile_specs_slice.contains("ClaudeDesktopGatewayProfileModelSpec")
            && profile_specs_slice.contains("name: route.route_id"),
        "proxy_core_adapter should own proxy route to gateway profile model spec projection"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
        "proxy_gateway_base_url_from_db",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(profile_specs_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_proxy_gateway_profile_model_specs:{} contains host-owned profile spec marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop proxy profile spec projection free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_owns_claude_desktop_proxy_request_body_provider_projection() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let request_body_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_desktop_proxy_request_body(",
        "pub(crate) struct ClaudeDesktopProviderStatusFacts",
    );

    assert!(
        request_body_slice.contains("provider_claude_desktop_proxy_model_routes(")
            && request_body_slice
                .contains("claude_desktop_proxy_request_body_with_upstream_model(")
            && request_body_slice.contains("ClaudeDesktopProviderProxyRequestBodyIssue::Routes")
            && request_body_slice.contains("ClaudeDesktopProviderProxyRequestBodyIssue::Body"),
        "proxy_core_adapter should own Claude Desktop proxy request-body Provider projection"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config",
        "state.db",
        "get_effective_current_provider",
        "proxy_gateway_base_url_from_db",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(request_body_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs provider_claude_desktop_proxy_request_body:{} contains host-owned request body marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude Desktop proxy request-body projection free of host side effects:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_does_not_retain_proxy_route_projection_wrapper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");

    assert!(
        !source.contains("pub fn proxy_model_routes(") && !source.contains("ResolvedModelRoute"),
        "claude_desktop_config should not retain proxy route projection wrapper DTOs after model-route sources moved to proxy_core_adapter"
    );
}

#[test]
fn claude_desktop_config_delegates_proxy_request_route_lookup_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let request_mapping_slice = function_slice(
        &source,
        "pub fn map_proxy_request_model(",
        "pub fn proxy_gateway_base_url_from_db(",
    );

    assert!(
        request_mapping_slice.contains("provider_claude_desktop_proxy_request_body("),
        "claude_desktop_config should delegate provider proxy request body mapping to proxy_core_adapter"
    );

    let forbidden_markers = [
        ".meta",
        "claude_desktop_model_routes",
        "ClaudeDesktopProxyRouteInput",
        "ClaudeDesktopResolvedProxyRoute",
        "provider.settings_config",
        "api_format.as_deref",
        "claude_desktop_proxy_request_upstream_model(",
        "provider_should_normalize_mimo_anthropic_thinking_history(",
        "normalize_anthropic_tool_thinking_history(",
        ".get(\"model\")",
        "body[\"model\"]",
        ".map(str::trim)",
        "strip_one_m_suffix_for_route_lookup",
        "legacy_raw_route_upstream_model",
        "is_compatible_opus_route_alias",
        "claude_role_keyword",
        "LEGACY_OPUS_ROUTE_ID",
        "ONE_M_CONTEXT_MARKER",
        "is_claude_safe_model_id(",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(request_mapping_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/claude_desktop_config.rs map_proxy_request_model:{} contains request body mapping policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep proxy request body mapping policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_mimo_thinking_history_normalization_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let request_mapping_slice = function_slice(
        &source,
        "pub fn map_proxy_request_model(",
        "pub fn proxy_gateway_base_url_from_db(",
    );

    assert!(
        request_mapping_slice.contains("provider_claude_desktop_proxy_request_body("),
        "claude_desktop_config should delegate MiMo thinking-history normalization to proxy_core_adapter/core"
    );
    assert!(
        !source.contains("fn normalize_mimo_anthropic_thinking_history("),
        "claude_desktop_config should not keep a duplicate MiMo thinking-history normalizer"
    );

    let forbidden_markers = [
        "MIMO_REDACTED_THINKING_PLACEHOLDER",
        "MIMO_TOOL_CALL_THINKING_PLACEHOLDER",
        "\"redacted_thinking\"",
        "\"tool_use\"",
        "\"signature\"",
        "\"tool call\"",
        "\"[redacted thinking]\"",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(request_mapping_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/claude_desktop_config.rs map_proxy_request_model:{} contains MiMo thinking-history policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep MiMo thinking-history mutation policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_does_not_retain_direct_provider_validation_wrapper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");

    assert!(
        !source.contains("pub fn validate_direct_provider("),
        "claude_desktop_config should not retain a Direct provider validation wrapper after provider validation dispatch moved to proxy_core_adapter"
    );
}

#[test]
fn claude_desktop_config_does_not_retain_proxy_provider_validation_wrapper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");

    assert!(
        !source.contains("pub fn validate_proxy_provider("),
        "claude_desktop_config should not retain a Proxy provider validation wrapper after provider validation dispatch moved to proxy_core_adapter"
    );
}

#[test]
fn claude_desktop_config_delegates_provider_validation_dispatch_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let validation_slice = function_slice(
        &source,
        "pub fn validate_provider(",
        "pub fn map_proxy_request_model(",
    );

    assert!(
        validation_slice.contains("provider_claude_desktop_provider_validation("),
        "claude_desktop_config should delegate provider validation mode dispatch to proxy_core_adapter"
    );

    let forbidden_markers = [
        "match provider_mode(provider)",
        "validate_direct_provider(provider)",
        "validate_proxy_provider(provider)",
        "ClaudeDesktopMode::Direct",
        "ClaudeDesktopMode::Proxy",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(validation_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/claude_desktop_config.rs validate_provider:{} contains validation dispatch marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep provider validation mode dispatch in proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_does_not_retain_direct_model_specs_wrapper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");

    assert!(
        !source.contains("fn direct_inference_model_specs("),
        "claude_desktop_config should not retain a Direct model specs wrapper after provider validation/profile assembly moved to proxy_core_adapter"
    );
}

#[test]
fn claude_desktop_config_does_not_retain_direct_gateway_credentials_wrapper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");

    assert!(
        !source.contains("pub fn direct_gateway_credentials(")
            && !source.contains("struct DirectGatewayCredentials"),
        "claude_desktop_config should not retain Direct gateway credential wrapper DTOs after provider validation/profile assembly moved to proxy_core_adapter"
    );
}

#[test]
fn proxy_management_auth_delegates_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/auth_adapter.rs");
    let source = fs::read_to_string(&path).expect("read auth_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) async fn validate_proxy_management_auth",
        "}",
    );

    assert!(
        function.contains(".proxy_engine()") && function.contains(".validate_management_auth(headers)"),
        "management auth adapter must delegate token-source lookup and bearer validation to ProxyEngine"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in [
            "state.config",
            ".config.read()",
            "std::env::var(",
            "CC_SWITCH_PROXY_MANAGEMENT_TOKEN",
            "resolve_management_auth_decision(",
            "validate_management_bearer_header(",
        ] {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/auth_adapter.rs validate_proxy_management_auth:{} contains management auth decision marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "management auth adapter must keep token-source decisions behind ProxyEngine:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_proxy_gateway_origin_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let gateway_url_slice = function_slice(
        &source,
        "pub fn proxy_gateway_base_url_from_db(",
        "fn apply_provider_to_paths(",
    );

    assert!(
        gateway_url_slice.contains("proxy_live_urls_from_listen_parts("),
        "claude_desktop_config should delegate proxy gateway origin formatting to proxy_core_adapter/core"
    );
    assert!(
        gateway_url_slice.contains("claude_desktop_proxy_gateway_base_url("),
        "claude_desktop_config should delegate Claude Desktop gateway endpoint formatting to proxy_core_adapter/core"
    );
    assert!(
        !source.contains("fn proxy_origin_from_parts("),
        "claude_desktop_config should not keep a duplicate proxy origin formatter"
    );

    let forbidden_markers = [
        "\"0.0.0.0\"",
        "\"::\"",
        "connect_host",
        "starts_with('[')",
        "CLAUDE_DESKTOP_PROXY_PREFIX",
        "\"/claude-desktop\"",
        "format!(\"{proxy_origin}",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(gateway_url_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/claude_desktop_config.rs proxy_gateway_base_url_from_db:{} contains proxy origin policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep proxy origin and endpoint formatting in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_profile_stale_model_detection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let status_slice = function_slice(
        &source,
        "pub fn get_status(",
        "pub fn get_config_library_path(",
    );

    assert!(
        status_slice.contains("claude_desktop_profile_has_unsafe_model_ids("),
        "claude_desktop_config should delegate Claude Desktop profile stale model detection to proxy_core_adapter/core"
    );
    assert!(
        status_slice.contains("claude_desktop_profile_gateway_base_url("),
        "claude_desktop_config should delegate Claude Desktop profile gateway base URL extraction to proxy_core_adapter/core"
    );

    let forbidden_markers = [
        "\"inferenceGatewayBaseUrl\"",
        "\"inferenceModels\"",
        "is_claude_safe_model_id(",
        "!is_claude_safe_model_id",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(status_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/claude_desktop_config.rs get_status:{} contains profile stale-model policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep profile stale-model detection in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_status_provider_facts_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let status_slice = function_slice(
        &source,
        "pub fn get_status(",
        "pub fn get_config_library_path(",
    );

    assert!(
        status_slice.contains("provider_claude_desktop_status_facts("),
        "claude_desktop_config should delegate current-provider status facts to proxy_core_adapter"
    );

    let forbidden_markers = [
        "match mode",
        "direct_gateway_credentials(provider).ok()",
        "proxy_model_routes(provider).is_err()",
        "matches!(provider_mode(provider), ClaudeDesktopMode::Proxy)",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(status_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/claude_desktop_config.rs get_status:{} contains provider status policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep current-provider status facts in proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_gateway_token_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let status_slice = function_slice(
        &source,
        "pub fn get_status(",
        "pub fn get_config_library_path(",
    );
    let apply_slice = function_slice(
        &source,
        "fn apply_provider_to_paths_inner(",
        "fn write_deployment_mode(",
    );

    assert!(
        status_slice.contains("claude_desktop_gateway_token_configured_from_db_source(")
            && apply_slice.contains("get_or_create_claude_desktop_gateway_token_from_db_source(")
            && !source.contains("pub fn get_or_create_gateway_token("),
        "claude_desktop_config should use proxy_core_adapter gateway token source without retaining a host wrapper"
    );

    let forbidden_markers = [
        "get_or_create_gateway_token(",
        "GATEWAY_TOKEN_SETTING_KEY",
        "\"claude_desktop_gateway_token\"",
        ".get_setting(",
        ".set_setting(",
        "uuid::Uuid::new_v4()",
    ];
    let mut violations = Vec::new();
    for (label, slice) in [
        ("get_status", status_slice),
        ("apply_provider_to_paths_inner", apply_slice),
    ] {
        for (line_index, line) in production_lines(slice) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/claude_desktop_config.rs {label}:{} contains gateway token-source marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep gateway token DB source in proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_provider_mode_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let mode_slice = function_slice(
        &source,
        "pub fn provider_mode(",
        "fn direct_gateway_credential_issue_to_error(",
    );

    assert!(
        mode_slice.contains("provider_claude_desktop_mode("),
        "claude_desktop_config should delegate provider mode defaulting to proxy_core_adapter"
    );

    let forbidden_markers = [
        ".meta",
        "claude_desktop_mode.clone()",
        "unwrap_or(ClaudeDesktopMode::Direct)",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(mode_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/claude_desktop_config.rs provider_mode:{} contains mode policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep provider mode defaulting in proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_default_proxy_route_catalog_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&source_path).expect("read claude_desktop_config.rs");
    let command_path = manifest_dir.join("src/commands/provider.rs");
    let command_source = fs::read_to_string(&command_path).expect("read commands/provider.rs");
    let default_routes_slice = function_slice(
        &source,
        "pub fn default_proxy_routes(",
        "pub fn is_compatible_direct_provider(",
    );

    assert!(
        default_routes_slice.contains("claude_desktop_default_proxy_routes("),
        "claude_desktop_config should delegate default proxy route catalog to proxy_core_adapter/core"
    );
    assert!(
        !source.contains("DEFAULT_PROXY_ROUTES"),
        "claude_desktop_config should not keep a duplicate DEFAULT_PROXY_ROUTES catalog"
    );
    assert!(
        !command_source.contains("DEFAULT_PROXY_ROUTES"),
        "commands/provider should consume default_proxy_routes instead of the old host constant"
    );

    let forbidden_markers = [
        "\"claude-sonnet-4-6\"",
        "\"claude-opus-4-8\"",
        "\"claude-haiku-4-5\"",
        "\"claude-fable-5\"",
        "\"ANTHROPIC_DEFAULT_SONNET_MODEL\"",
        "\"ANTHROPIC_DEFAULT_OPUS_MODEL\"",
        "\"ANTHROPIC_DEFAULT_HAIKU_MODEL\"",
        "\"ANTHROPIC_DEFAULT_FABLE_MODEL\"",
        "CURRENT_OPUS_ROUTE_ID",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(default_routes_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/claude_desktop_config.rs default_proxy_routes:{} contains default route catalog marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep default proxy route catalog in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_gateway_profile_json_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let apply_slice = function_slice(
        &source,
        "fn apply_provider_to_paths_inner(",
        "fn restore_official_at_paths_inner(",
    );

    assert!(
        apply_slice.contains("claude_desktop_gateway_profile(")
            && apply_slice.contains("provider_claude_desktop_direct_gateway_profile(")
            && apply_slice.contains("provider_claude_desktop_proxy_gateway_profile_model_specs("),
        "claude_desktop_config should delegate gateway profile JSON construction to proxy_core_adapter/core"
    );
    assert!(
        !source.contains("fn build_gateway_profile(")
            && !source.contains("fn inference_model_json(")
            && !source.contains("struct InferenceModelSpec"),
        "claude_desktop_config should not keep duplicate gateway profile JSON builders"
    );

    let forbidden_markers = [
        "\"coworkEgressAllowedHosts\"",
        "\"disableDeploymentModeChooser\"",
        "\"inferenceGatewayApiKey\"",
        "\"inferenceGatewayAuthScheme\"",
        "\"inferenceGatewayBaseUrl\"",
        "\"inferenceProvider\"",
        "\"inferenceModels\"",
        "\"labelOverride\"",
        "\"supports1m\"",
        "ClaudeDesktopGatewayProfileModelSpec {",
        "route.route_id.clone()",
        "route.label_override.clone()",
        "direct_gateway_credentials(provider)",
        "direct_inference_model_specs(provider)",
        "json!({",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(apply_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/claude_desktop_config.rs apply_provider_to_paths_inner:{} contains gateway profile JSON marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep gateway profile JSON policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_local_config_json_transforms_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let deployment_slice = function_slice(
        &source,
        "fn write_deployment_mode(",
        "fn remove_cc_switch_enterprise_config(",
    );
    let cleanup_slice = function_slice(
        &source,
        "fn remove_cc_switch_enterprise_config(",
        "fn write_meta(",
    );

    assert!(
        deployment_slice.contains("claude_desktop_config_with_deployment_mode("),
        "write_deployment_mode should delegate config object normalization and deploymentMode mutation to proxy_core_adapter/core"
    );
    assert!(
        cleanup_slice.contains("claude_desktop_config_without_gateway_enterprise_config("),
        "remove_cc_switch_enterprise_config should delegate enterprise gateway-key cleanup to proxy_core_adapter/core"
    );

    let mut violations = Vec::new();
    for (slice_name, slice, forbidden_markers) in [
        (
            "write_deployment_mode",
            deployment_slice,
            &[
                "\"deploymentMode\"",
                "as_object_mut",
                "Value::String",
                "json!({",
            ][..],
        ),
        (
            "remove_cc_switch_enterprise_config",
            cleanup_slice,
            &[
                "\"enterpriseConfig\"",
                "\"disableDeploymentModeChooser\"",
                "\"inferenceGatewayApiKey\"",
                "\"inferenceGatewayAuthScheme\"",
                "\"inferenceGatewayBaseUrl\"",
                "\"inferenceProvider\"",
                ".remove(",
                "as_object_mut",
            ][..],
        ),
    ] {
        for (line_index, line) in production_lines(slice) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/claude_desktop_config.rs {slice_name}:{} contains local config JSON policy marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep local config JSON mutation policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn claude_desktop_config_delegates_meta_json_policy_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/claude_desktop_config.rs");
    let source = fs::read_to_string(&path).expect("read claude_desktop_config.rs");
    let write_slice = function_slice(&source, "fn write_meta(", "fn read_applied_id(");
    let applied_slice =
        function_slice(&source, "fn read_applied_id(", "fn meta_has_profile_entry(");
    let entry_slice = function_slice(
        &source,
        "fn meta_has_profile_entry(",
        "fn is_supported_platform(",
    );

    assert!(
        write_slice.contains("claude_desktop_meta_with_profile_entry("),
        "write_meta should delegate appliedId/entries mutation to proxy_core_adapter/core"
    );
    assert!(
        applied_slice.contains("claude_desktop_meta_applied_id("),
        "read_applied_id should delegate appliedId parsing to proxy_core_adapter/core"
    );
    assert!(
        entry_slice.contains("claude_desktop_meta_has_profile_entry("),
        "meta_has_profile_entry should delegate entries lookup to proxy_core_adapter/core"
    );

    let mut violations = Vec::new();
    for (slice_name, slice, forbidden_markers) in [
        (
            "write_meta",
            write_slice,
            &[
                "\"appliedId\"",
                "\"entries\"",
                "as_object_mut",
                "Value::Array",
                "json!({",
                "entries.retain",
            ][..],
        ),
        (
            "read_applied_id",
            applied_slice,
            &["\"appliedId\"", "Value::as_str"][..],
        ),
        (
            "meta_has_profile_entry",
            entry_slice,
            &["\"entries\"", "Value::as_array", ".any("][..],
        ),
    ] {
        for (line_index, line) in production_lines(slice) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/claude_desktop_config.rs {slice_name}:{} contains meta JSON policy marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "claude_desktop_config must keep meta JSON policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_managed_provider_classification_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let classification_slice = function_slice(
        &source,
        "pub(crate) fn provider_kind_from_provider(",
        "pub(crate) fn provider_uses_anthropic_rectifiers",
    );

    assert!(
        classification_slice.contains("core_classify_provider_managed_auth(")
            && classification_slice.contains("ProviderManagedAuthFacts"),
        "proxy_core_adapter must delegate managed-provider classification to proxy-core"
    );

    let forbidden_markers = [
        "core_provider_kind_is_codex_oauth(",
        "core_provider_kind_is_github_copilot(",
        "core_provider_kind_uses_managed_account_auth(",
        "provider_kind_is_codex_oauth(",
        "provider_kind_is_github_copilot(",
        "provider_kind_uses_managed_account_auth(",
        "githubcopilot.com",
        "chatgpt.com/backend-api/codex",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(classification_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs managed provider classification:{} contains host-local marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep managed provider classification in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_mimo_thinking_normalization_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    let slice = function_slice(
        &source,
        "pub(crate) fn provider_should_normalize_mimo_anthropic_thinking_history",
        "pub(crate) fn provider_stream_check_test_config",
    );

    let delegates_to_core = slice.contains("MimoAnthropicThinkingNormalizationInput")
        && slice.contains(
            "crate::proxy_core::api::transforms::should_normalize_mimo_anthropic_thinking_history(",
        );
    assert!(
        delegates_to_core,
        "proxy_core_adapter should project Provider facts into the core MiMo thinking normalization gate"
    );

    let forbidden_markers = [
        "provider_uses_anthropic_messages_format(",
        "provider_has_mimo_endpoint(",
        "is_mimo_identifier(",
        "\"api_format\"",
        "\"ANTHROPIC_BASE_URL\"",
        "\"base_url\"",
        "\"baseURL\"",
        "\"apiEndpoint\"",
        "xiaomimimo",
        ".contains(\"mimo\")",
    ];
    let mut violations = Vec::new();
    for marker in forbidden_markers {
        if slice.contains(marker) {
            violations.push(marker);
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep MiMo endpoint/model/api_format policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_response_parse_failure_logging_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_RESPONSE_PARSE_FAILURE_LOG_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains response parse failure log marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate response parse failure log projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_response_build_error_mapping_to_error_mapper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_RESPONSE_BUILD_ERROR_MAPPING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains response build error mapping marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate response build error mapping to error_mapper:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_response_transform_error_mapping_to_error_mapper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_RESPONSE_TRANSFORM_ERROR_MAPPING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains response transform error mapping marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate response transform error mapping to error_mapper:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_transformed_usage_policy_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_TRANSFORMED_USAGE_POLICY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains transformed usage policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate transformed usage format/filter policy to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn response_pipeline_owns_non_stream_passthrough_response_construction() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/response_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/response_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn passthrough_non_stream_proxy_response_from_context",
        "/// 内部使用量记录函数",
    );

    assert!(
        function.contains("log_non_streaming_proxy_response_body(&body, ctx.tag)")
            && function.contains("record_non_streaming_response_usage(")
            && function.contains("passthrough_bytes_proxy_response(status, headers, body)"),
        "response pipeline should own non-streaming passthrough response construction"
    );
    assert!(
        !function.contains("non_streaming_response_usage_record_from_response_context(")
            && !function.contains("response_usage_provider_facts_from_optional(")
            && !function.contains("spawn_usage_record_with_proxy_services("),
        "response pipeline should still delegate usage projection and persistence internals"
    );
    assert!(
        !adapter_source.contains("passthrough_non_stream_proxy_response_from_context")
            && !adapter_source
                .contains("pub(crate) fn passthrough_non_stream_proxy_response_from_context"),
        "proxy_core_adapter should not re-export non-streaming passthrough construction"
    );
}

#[test]
fn handlers_delegate_transformed_response_build_context_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_TRANSFORMED_RESPONSE_BUILD_CONTEXT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains transformed response build marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate transformed response build contexts to response_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_claude_response_transform_dispatch_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CLAUDE_RESPONSE_TRANSFORM_DISPATCH_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains Claude response transform dispatch marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate Claude response transform dispatch to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_claude_streaming_decision_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CLAUDE_STREAMING_DECISION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains Claude streaming decision marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude transform handler must delegate streaming/aggregation decisions to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_codex_non_stream_transform_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CODEX_NON_STREAM_TRANSFORM_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains Codex non-stream transform marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate Codex non-stream transform and history recording to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_codex_stream_transform_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CODEX_STREAM_TRANSFORM_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains Codex stream transform marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate Codex stream transform and history recording to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_codex_chat_streaming_decision_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CODEX_STREAMING_DECISION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains Codex streaming decision marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Codex Chat->Responses handler must delegate streaming/aggregation decisions to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_transform_streaming_decision_calls_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_TRANSFORM_STREAMING_DECISION_CALL_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains transform streaming decision call marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must call transform streaming decisions through response_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_transform_response_orchestration_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_TRANSFORM_RESPONSE_ORCHESTRATION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains transform response orchestration marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate transform response orchestration to response_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn handlers_delegate_passthrough_response_processing_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_PASSTHROUGH_RESPONSE_PROCESSING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/handlers.rs:{} contains passthrough response processing marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol handlers must delegate passthrough response processing to response_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_claude_transform_streaming_decision_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let claude_decision = function_slice(
        &source,
        "pub(crate) fn provider_claude_transform_streaming_decision",
        "pub(crate) use crate::proxy_core::api::domain::infer_claude_provider_kind",
    );

    assert!(
        claude_decision.contains("core_claude_transform_streaming_decision("),
        "Claude transform streaming decision must be delegated to proxy-core"
    );
    assert!(
        !source.contains("pub(crate) fn codex_chat_transform_streaming_decision")
            && !source.contains("core_codex_chat_transform_streaming_decision"),
        "Codex Chat transform streaming decision should not keep a one-hop proxy_core_adapter facade"
    );

    for marker in FORBIDDEN_PROXY_CORE_ADAPTER_CLAUDE_STREAMING_DECISION_MARKERS {
        assert!(
            !claude_decision.contains(marker),
            "proxy_core_adapter must not locally compose Claude streaming decision marker `{marker}`"
        );
    }
}

#[test]
fn production_proxy_hyper_client_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/hyper_client.rs");
    let source = fs::read_to_string(&path).expect("read hyper_client.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub(crate) use super::transport::upstream::hyper_client::*;",
        ],
        "legacy proxy/hyper_client.rs must remain a re-export shim after transport/upstream/hyper_client.rs split"
    );
}

#[test]
fn response_pipeline_uses_core_sse_header_decision() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let hyper_client =
        fs::read_to_string(manifest_dir.join("src/proxy/transport/upstream/hyper_client.rs"))
            .expect("read transport/upstream/hyper_client.rs");
    let response_processor =
        fs::read_to_string(manifest_dir.join("src/proxy/engine/response_pipeline.rs"))
            .expect("read engine/response_pipeline.rs");
    let response_adapter = fs::read_to_string(manifest_dir.join("src/proxy/response_adapter.rs"))
        .expect("read response_adapter.rs");
    let adapter = fs::read_to_string(manifest_dir.join("src/proxy_core_adapter.rs"))
        .expect("read proxy_core_adapter.rs");

    assert!(
        !hyper_client.contains("fn is_sse("),
        "SSE response detection should stay in proxy-core response header helpers, not on the host ProxyResponse type"
    );
    assert!(
        response_processor.contains("response_headers_indicate_sse(response.headers())"),
        "response_processor should delegate SSE detection to proxy-core"
    );
    assert!(
        adapter.contains("core_claude_transform_streaming_decision(")
            && !adapter.contains("core_codex_chat_transform_streaming_decision(")
            && response_adapter.contains(
                "codex_chat_transform_streaming_decision(requested_streaming, response_headers)"
            ),
        "provider-aware Claude decision stays in proxy_core_adapter; pure Codex Chat decision should be called from response_adapter/core"
    );
}

#[test]
fn usage_sink_bridge_module_removed_after_adapter_migration() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/usage_sink_bridge.rs");

    assert!(
        !path.exists(),
        "usage_sink_bridge should stay deleted; add usage entrypoints to proxy_core_adapter instead"
    );
}

#[test]
fn production_proxy_usage_logger_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/usage/logger.rs");
    let source = fs::read_to_string(&path).expect("read usage/logger.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub use super::super::host::cc_switch::database_usage_sink::*;",
        ],
        "legacy proxy/usage/logger.rs must remain a re-export shim after host database usage sink split"
    );
}

#[test]
fn production_forwarder_stays_preplanned_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut rust_files = Vec::new();
    collect_rust_files(&manifest_dir.join("src"), &mut rust_files);

    let mut violations = Vec::new();
    for path in rust_files {
        let relative = path
            .strip_prefix(&manifest_dir)
            .expect("source path under manifest dir")
            .to_string_lossy()
            .replace('\\', "/");

        let source = fs::read_to_string(&path).expect("read host source file");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_FORWARDER_SELF_PLANNING_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{}:{} contains legacy forwarder self-planning marker `{}`",
                        relative,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production forwarder code must execute preplanned attempts from ProxyEngine/ForwardPipeline:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_forwarder_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub(crate) use super::engine::forward_pipeline::*;",
        ],
        "legacy proxy/forwarder.rs must remain a re-export shim after engine/forward_pipeline.rs split"
    );
}

#[test]
fn production_forwarder_delegates_upstream_url_planning_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_URL_PLANNING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains upstream URL planning marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production forwarder must delegate upstream URL planning to proxy_core_adapter::forward_upstream_url_plan:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_claude_request_format_dispatch_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let request_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_transform_request_for_api_format",
        "#[cfg(test)]\npub(crate) fn provider_claude_transform_response",
    );

    assert!(
        request_slice.contains("claude_request_transform_for_api_format("),
        "Claude request api_format dispatch must be delegated to proxy-core"
    );

    let forbidden_markers = [
        "\"openai_responses\"",
        "\"openai_chat\"",
        "\"gemini_native\"",
        "anthropic_to_openai_responses_request(",
        "anthropic_to_openai_chat_request(",
        "anthropic_request_to_gemini_request_with_shadow(",
        "inject_openai_stream_include_usage(",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(request_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs Claude request api_format dispatch:{} contains host-local marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude request api_format dispatch in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_claude_transform_gate_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let gate_slice = function_slice(
        &source,
        "pub(crate) fn provider_needs_claude_transform",
        "pub(crate) fn provider_claude_transform_streaming_decision",
    );

    assert!(
        gate_slice.contains("core_claude_provider_transform_required("),
        "Claude transform gate must delegate provider-kind/api-format policy to proxy-core"
    );

    let forbidden_markers = [
        "return true",
        "claude_api_format_needs_transform(provider_claude_api_format(provider))",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(gate_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs Claude transform gate:{} contains host-local marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude transform gate policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_codex_responses_to_chat_gate_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let gate_slice = function_slice(
        &source,
        "fn with_provider_codex_chat_completions_facts",
        "pub(crate) fn provider_codex_upstream_model",
    );

    assert!(
        gate_slice.contains("core_codex_provider_uses_chat_completions")
            && gate_slice.contains("core_codex_responses_to_chat_conversion_required("),
        "Codex Responses to Chat gate must delegate provider and endpoint policy to proxy-core"
    );

    let forbidden_markers = [
        "resolve_codex_provider_uses_chat_completions(",
        "should_convert_codex_responses_endpoint_to_chat(",
        "provider_codex_uses_chat_completions(provider)",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(gate_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs Codex Responses to Chat gate:{} contains host-local marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Codex Responses to Chat conversion policy in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_claude_message_normalization_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let normalize_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_normalize_anthropic_messages",
        "#[cfg(test)]\npub(crate) use crate::proxy_core::api::transport::inject_openai_stream_include_usage;",
    );

    assert!(
        normalize_slice.contains("normalize_claude_anthropic_messages("),
        "Claude message normalization composition must be delegated to proxy-core"
    );

    let forbidden_markers = [
        "api_format.trim()",
        "provider_should_normalize_anthropic_tool_thinking_history(",
        "normalize_anthropic_tool_thinking_history(",
        "provider_normalize_deepseek_thinking_disabled_strip_effort(",
        "normalize_deepseek_thinking_disabled_strip_effort(",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(normalize_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs Claude message normalization:{} contains host-local marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude message normalization composition in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_claude_response_format_dispatch_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let response_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_transform_response_for_api_format",
        "pub(crate) fn provider_claude_transform_sse_for_api_format",
    );
    let stream_slice = function_slice(
        &source,
        "pub(crate) fn provider_claude_transform_sse_for_api_format",
        "pub(crate) fn provider_should_preserve_reasoning_content_for_openai_chat",
    );

    assert!(
        response_slice.contains("claude_response_to_anthropic_message_for_api_format("),
        "Claude non-streaming response api_format dispatch must be delegated to proxy-core"
    );
    assert!(
        stream_slice.contains("create_claude_to_anthropic_sse_stream_for_api_format("),
        "Claude SSE response api_format dispatch must be delegated to proxy-core"
    );

    let forbidden_markers = [
        "\"openai_responses\"",
        "\"gemini_native\"",
        "openai_responses_to_anthropic_message(",
        "openai_chat_to_anthropic_message(",
        "gemini_response_to_anthropic_message_with_shadow(",
        "create_openai_responses_to_anthropic_sse_stream(",
        "create_openai_chat_to_anthropic_sse_stream(",
        "create_gemini_to_anthropic_sse_stream_with_callbacks(",
    ];
    let mut violations = Vec::new();
    for slice in [response_slice, stream_slice] {
        for (line_index, line) in production_lines(slice) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in forbidden_markers {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy_core_adapter.rs Claude response api_format dispatch:{} contains host-local marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep Claude response api_format dispatch in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_upstream_url_plan_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let request_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/forwarder_request_source.rs");
    let request_source =
        fs::read_to_string(&request_source_path).expect("read forwarder_request_source.rs");
    let impl_slice = function_slice(
        &request_source,
        "impl ForwarderRequestSource for CcSwitchForwarderRequestSource",
        "pub(crate) fn forwarder_rectifier_error_message",
    );
    let function = function_slice(
        impl_slice,
        "fn plan_upstream_url(&self, input: ForwarderUpstreamUrlInput<'_>) -> ForwardUpstreamUrlPlan",
        "fn prepare_copilot_request_optimization",
    );

    assert!(
        function.contains("forward_upstream_url_plan("),
        "ForwarderRequestSource should delegate upstream URL planning to proxy-core"
    );

    for marker in [
        "pub(crate) struct ForwardUpstreamUrlPlanInput",
        "pub(crate) struct ForwardUpstreamUrlPlan",
        "pub(crate) fn forward_upstream_url_plan(",
    ] {
        assert!(
            !adapter_source.contains(marker),
            "proxy_core_adapter must not keep upstream URL plan policy marker `{marker}`"
        );
    }

    for marker in [
        "rewrite_claude_transform_endpoint(",
        "append_query_to_full_url(",
        "apply_channel_param_overrides_to_url(",
        "is_codex_chat_full_endpoint_base(",
        "resolve_gemini_native_url(",
    ] {
        assert!(
            !function.contains(marker),
            "ForwarderRequestSource::plan_upstream_url must not inline upstream URL policy marker `{marker}`"
        );
    }
}

#[test]
fn proxy_core_adapter_delegates_provider_url_facts_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let path = manifest_dir.join("src/proxy/host/cc_switch/provider_adapter_context.rs");
    let source = fs::read_to_string(&path).expect("read provider_adapter_context.rs");
    let function = function_slice(
        &source,
        "    pub(crate) fn provider_url_facts(",
        "    pub(crate) fn provider_transform_required",
    );

    assert!(
        function.contains("forwarder_provider_url_facts("),
        "ForwarderAdapterContext::provider_url_facts must delegate URL facts projection to proxy-core"
    );
    assert!(
        source.contains("use crate::proxy_core::api::transport::{")
            && source.contains("forwarder_provider_url_facts")
            && source.contains("ForwarderProviderUrlFactsInput"),
        "ForwarderAdapterContext should import pure provider URL facts helper/input directly from proxy_core::api::transport"
    );
    for marker in [
        "forwarder_provider_url_facts",
        "ForwarderProviderUrlFactsInput",
    ] {
        assert!(
            !adapter_runtime_source.contains(marker),
            "proxy_core_adapter should not re-export pure provider URL facts helper/input `{marker}` once provider_adapter_context owns the call site"
        );
    }

    let forbidden_markers = [
        "ForwarderProviderUrlFacts {",
        "provider_is_github_copilot_upstream(",
        "is_github_copilot_upstream(",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/provider_adapter_context.rs provider_url_facts:{} contains host-local marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        adapter_source
            .contains("pub(crate) use crate::proxy::host::cc_switch::provider_adapter_context::{")
            && adapter_source.contains("ForwarderAdapterContext"),
        "proxy_core_adapter should expose provider adapter context through a host-module re-export"
    );
    assert!(
        !adapter_source.contains("pub(crate) struct ForwarderAdapterContext"),
        "proxy_core_adapter must not own ForwarderAdapterContext after host split"
    );

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must keep provider URL facts projection in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_claude_provider_helpers_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_CLAUDE_PROVIDER_COMPAT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains Claude provider compat marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume Claude provider facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_codex_provider_helpers_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_CODEX_PROVIDER_COMPAT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains Codex provider compat marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume Codex provider facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_request_optimizer_provider_facts_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_REQUEST_OPTIMIZER_PROVIDER_FACT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains request optimizer provider fact marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume request optimizer provider facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_request_header_provider_facts_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_REQUEST_HEADER_PROVIDER_FACT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains request header provider fact marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume request header provider facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_request_url_provider_facts_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_REQUEST_URL_PROVIDER_FACT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains request URL provider fact marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume request URL provider facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_request_media_provider_facts_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_REQUEST_MEDIA_PROVIDER_FACT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains request media provider fact marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume request media provider facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_provider_adapter_transform_gate_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_TRANSFORM_GATE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains provider adapter transform gate marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume provider adapter transform gates through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_provider_adapter_request_transform_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_REQUEST_TRANSFORM_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains provider adapter request transform marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume provider adapter request transforms through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_provider_adapter_base_url_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_BASE_URL_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains provider adapter base URL marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume provider adapter base URL extraction through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_provider_adapter_auth_info_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_AUTH_INFO_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains provider adapter auth info marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume provider adapter auth info extraction through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_provider_adapter_auth_headers_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_AUTH_HEADER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains provider adapter auth header marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume provider adapter auth headers through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_provider_adapter_url_building_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_URL_BUILD_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains provider adapter URL build marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume provider adapter URL building through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_provider_adapter_name_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_NAME_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains provider adapter name marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume provider adapter names through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_provider_adapter_registry_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_REGISTRY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains provider adapter registry marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must obtain provider adapters through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_uses_adapter_context_without_provider_trait() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_TRAIT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains provider adapter trait marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must use proxy_core_adapter adapter context without importing provider adapter handles or traits:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_channel_status_mapping_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_CHANNEL_STATUS_MAPPING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains channel status mapping marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume channel response status mapping through proxy_core_adapter decision helper:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_module_excludes_codex_chat_history_state() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let provider_mod_path = manifest_dir.join("src/proxy/provider/mod.rs");
    let provider_mod = fs::read_to_string(&provider_mod_path).expect("read providers/mod.rs");
    let proxy_paths = [
        "src/proxy/engine/forward_pipeline.rs",
        "src/proxy/transport/http/handlers.rs",
        "src/proxy/transport/http/server.rs",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&provider_mod) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_MODULE_CODEX_HISTORY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/mod.rs:{} contains Codex history marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    for relative in proxy_paths {
        let source = fs::read_to_string(manifest_dir.join(relative)).expect("read proxy source");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains("providers::codex_chat_history") {
                violations.push(format!(
                    "{}:{} imports Codex history through provider module",
                    relative,
                    line_index + 1
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Codex chat history must live at proxy module scope, not under provider adapters:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_module_excludes_provider_kind_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let provider_mod_path = manifest_dir.join("src/proxy/provider/mod.rs");
    let provider_mod = fs::read_to_string(&provider_mod_path).expect("read providers/mod.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&provider_mod) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_MODULE_KIND_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/mod.rs:{} contains provider kind facade marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Provider kind inference belongs behind proxy_core_adapter, not provider registry:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_module_excludes_managed_auth_modules() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let provider_mod_path = manifest_dir.join("src/proxy/provider/mod.rs");
    let provider_mod = fs::read_to_string(&provider_mod_path).expect("read providers/mod.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&provider_mod) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_MODULE_MANAGED_AUTH_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider/mod.rs:{} contains managed auth marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Managed account auth modules must live at proxy module scope, not under provider adapters:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_providers_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/providers/mod.rs");
    let source = fs::read_to_string(&path).expect("read providers/mod.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec!["#[allow(unused_imports)]", "pub use super::provider::*;",],
        "legacy proxy/providers/mod.rs must remain a re-export shim after provider module split"
    );
}

#[test]
fn production_sources_do_not_import_managed_auth_through_providers() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    let mut files = Vec::new();
    collect_rust_files(&src_dir, &mut files);

    let mut violations = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file).expect("read rust source");
        let relative = file
            .strip_prefix(&manifest_dir)
            .expect("source under manifest dir")
            .display()
            .to_string();
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_PROVIDER_AUTH_PATH_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{}:{} imports managed auth through provider module marker `{}`",
                        relative,
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Managed account auth imports must target proxy::{{copilot_auth,codex_oauth_auth}}, not proxy::providers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_excludes_model_fetch_transport_facades() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_MODEL_FETCH_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains model fetch facade marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Model fetch transport should use proxy_core::api::model_catalog directly instead of proxy_core_adapter facades:\n{}",
        violations.join("\n")
    );
}

#[test]
fn model_fetch_commands_use_core_dto_entrypoint() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let command_paths = ["src/commands/model_fetch.rs", "src/commands/codex_oauth.rs"];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_MODEL_FETCH_ADAPTER_DTO_EXPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains DTO export marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    for relative in command_paths {
        let source = fs::read_to_string(manifest_dir.join(relative)).expect("read command source");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_MODEL_FETCH_COMMAND_DTO_IMPORT_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{}:{} contains service DTO import marker `{}`",
                        relative,
                        line_index + 1,
                        marker
                    ));
                }
            }
            if code.contains("proxy_core_adapter") && code.contains("FetchedModel") {
                violations.push(format!(
                    "{}:{} imports FetchedModel from proxy_core_adapter",
                    relative,
                    line_index + 1
                ));
            }
            let direct_core =
                code.contains("crate::proxy_core::") || code.contains("cc_switch_proxy_core::");
            if direct_core
                && code.trim() != "use crate::proxy_core::api::model_catalog::FetchedModel;"
            {
                violations.push(format!(
                    "{}:{} contains non-DTO direct proxy-core import `{}`",
                    relative,
                    line_index + 1,
                    code.trim()
                ));
            }
        }
        assert!(
            source.contains("use crate::proxy_core::api::model_catalog::FetchedModel;"),
            "{} must import FetchedModel directly from proxy_core",
            relative
        );
    }

    assert!(
        violations.is_empty(),
        "model fetch commands must use proxy_core::api::model_catalog as the FetchedModel DTO entrypoint:\n{}",
        violations.join("\n")
    );
}

#[test]
fn copilot_model_callers_use_core_dto_entrypoint() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let caller_paths = ["src/commands/copilot.rs", "src/proxy/copilot_auth.rs"];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_COPILOT_MODEL_ADAPTER_DTO_EXPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains CopilotModel DTO export marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    for relative in caller_paths {
        let source = fs::read_to_string(manifest_dir.join(relative)).expect("read caller source");
        let allowed_direct_core_lines = match relative {
            "src/commands/copilot.rs" => {
                &["use crate::proxy_core::api::model_catalog::CopilotModel;"][..]
            }
            "src/proxy/copilot_auth.rs" => &[
                "pub use crate::proxy_core::api::auth::{",
                "use crate::proxy_core::api::model_catalog::CopilotModel;",
                "pub use crate::proxy_core::api::model_catalog::CopilotUsageResponse;",
            ][..],
            _ => &[][..],
        };
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains("proxy_core_adapter") && code.contains("CopilotModel") {
                violations.push(format!(
                    "{}:{} imports CopilotModel from proxy_core_adapter",
                    relative,
                    line_index + 1
                ));
            }
            let direct_core =
                code.contains("crate::proxy_core::") || code.contains("cc_switch_proxy_core::");
            if direct_core && !allowed_direct_core_lines.contains(&code.trim()) {
                violations.push(format!(
                    "{}:{} contains non-DTO direct proxy-core import `{}`",
                    relative,
                    line_index + 1,
                    code.trim()
                ));
            }
        }
        assert!(
            source.contains("use crate::proxy_core::api::model_catalog::CopilotModel;"),
            "{} must import CopilotModel directly from proxy_core",
            relative
        );
    }

    assert!(
        violations.is_empty(),
        "Copilot model DTO callers must use proxy_core::api::model_catalog as the CopilotModel entrypoint:\n{}",
        violations.join("\n")
    );
}

#[test]
fn settings_runtime_config_callers_use_core_dto_entrypoint() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let caller_paths = [
        "src/commands/settings.rs",
        "src/database/dao/settings.rs",
        "src/proxy/engine/forward_pipeline.rs",
    ];
    let required_import =
        "use crate::proxy_core::api::ports::{CopilotOptimizerConfig, OptimizerConfig, RectifierConfig};";

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_SETTINGS_CONFIG_ADAPTER_DTO_EXPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains settings config DTO export marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    for relative in caller_paths {
        let source = fs::read_to_string(manifest_dir.join(relative)).expect("read caller source");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains("proxy_core_adapter")
                && (code.contains("RectifierConfig")
                    || code.contains("OptimizerConfig")
                    || code.contains("CopilotOptimizerConfig"))
            {
                violations.push(format!(
                    "{}:{} imports settings config DTOs from proxy_core_adapter",
                    relative,
                    line_index + 1
                ));
            }
            let direct_core =
                code.contains("crate::proxy_core::") || code.contains("cc_switch_proxy_core::");
            if direct_core && code.trim() != required_import {
                violations.push(format!(
                    "{}:{} contains non-settings-config direct proxy-core import `{}`",
                    relative,
                    line_index + 1,
                    code.trim()
                ));
            }
        }
        assert!(
            source.contains(required_import),
            "{} must import settings runtime config DTOs directly from proxy_core",
            relative
        );
    }

    assert!(
        violations.is_empty(),
        "settings runtime config callers must use proxy_core::api::ports as the DTO entrypoint:\n{}",
        violations.join("\n")
    );
}

#[test]
fn codex_config_uses_core_model_context_window_constant() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let codex_config_path = manifest_dir.join("src/codex_config.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let codex_config_source = fs::read_to_string(&codex_config_path).expect("read codex_config.rs");
    let required_import =
        "use crate::proxy_core::api::model_catalog::DEFAULT_CODEX_MODEL_CONTEXT_WINDOW;";

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CODEX_MODEL_CONTEXT_ADAPTER_CONST_EXPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains Codex model context const export marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    for (line_index, line) in production_lines(&codex_config_source) {
        let code = line.split("//").next().unwrap_or_default();
        if code.contains("proxy_core_adapter")
            && (code.contains("CODEX_DEFAULT_MODEL_CONTEXT_WINDOW")
                || code.contains("DEFAULT_CODEX_MODEL_CONTEXT_WINDOW"))
        {
            violations.push(format!(
                "src/codex_config.rs:{} imports Codex model context window constant from proxy_core_adapter",
                line_index + 1
            ));
        }
        let direct_core =
            code.contains("crate::proxy_core::") || code.contains("cc_switch_proxy_core::");
        if direct_core && code.trim() != required_import {
            violations.push(format!(
                "src/codex_config.rs:{} contains non-Codex-model-context direct proxy-core import `{}`",
                line_index + 1,
                code.trim()
            ));
        }
    }

    assert!(
        codex_config_source.contains(required_import),
        "codex_config.rs must import the Codex model context fallback constant directly from proxy_core"
    );
    assert!(
        codex_config_source.contains(".unwrap_or(DEFAULT_CODEX_MODEL_CONTEXT_WINDOW)"),
        "codex_config.rs should use the direct core Codex model context fallback"
    );
    assert!(
        violations.is_empty(),
        "Codex model context fallback constant must bypass proxy_core_adapter aliases:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_management_dto_callers_use_core_entrypoints() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let caller_specs = [
        (
            "src/commands/proxy.rs",
            &[
                "use crate::proxy_core::api::ports::GlobalProxyConfig;",
                "use crate::proxy_core::api::ports::ProviderHealth;",
            ][..],
        ),
        (
            "src/database/dao/proxy.rs",
            &[
                "use crate::proxy_core::api::ports::GlobalProxyConfig;",
                "use crate::proxy_core::api::ports::{ProviderHealth, ProviderHealthUpdateInput};",
            ][..],
        ),
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_MANAGEMENT_ADAPTER_DTO_EXPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains proxy management DTO export marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    for (relative, required_imports) in caller_specs {
        let source = fs::read_to_string(manifest_dir.join(relative)).expect("read caller source");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains("proxy_core_adapter")
                && (code.contains("GlobalProxyConfig")
                    || code.contains("ProviderHealth")
                    || code.contains("ProviderHealthUpdateInput"))
            {
                violations.push(format!(
                    "{}:{} imports proxy management DTOs from proxy_core_adapter",
                    relative,
                    line_index + 1
                ));
            }
            let direct_core =
                code.contains("crate::proxy_core::") || code.contains("cc_switch_proxy_core::");
            if direct_core && !required_imports.contains(&code.trim()) {
                violations.push(format!(
                    "{}:{} contains non-proxy-management-DTO direct proxy-core import `{}`",
                    relative,
                    line_index + 1,
                    code.trim()
                ));
            }
        }
        for required_import in required_imports {
            assert!(
                source.contains(required_import),
                "{} must contain required proxy management DTO import `{}`",
                relative,
                required_import
            );
        }
    }

    assert!(
        violations.is_empty(),
        "proxy management DTO callers must use proxy_core::api::ports as the DTO entrypoint:\n{}",
        violations.join("\n")
    );
}

#[test]
fn managed_auth_commands_delegate_provider_validation_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/commands/auth.rs");
    let source = fs::read_to_string(&path).expect("read commands/auth.rs");

    for marker in [
        "const AUTH_PROVIDER_",
        "fn ensure_auth_provider(",
        "Unsupported auth provider:",
        "pub struct ManagedAuthAccount",
        "pub struct ManagedAuthStatus",
        "pub struct ManagedAuthDeviceCodeResponse",
        "provider: provider.to_string()",
        "is_default: default_account_id",
    ] {
        assert!(
            !source.contains(marker),
            "commands/auth.rs should not own managed-auth command contract marker `{marker}`"
        );
    }

    assert!(
        source.contains("ensure_managed_auth_provider(")
            && source.contains("GITHUB_COPILOT_AUTH_PROVIDER")
            && source.contains("CODEX_OAUTH_AUTH_PROVIDER")
            && source.contains("ManagedAuthAccount")
            && source.contains("ManagedAuthStatus")
            && source.contains("ManagedAuthDeviceCodeResponse")
            && source.contains("managed_auth_account_from_parts(")
            && source.contains("managed_auth_status_from_parts(")
            && source.contains("managed_auth_device_code_response_from_parts("),
        "commands/auth.rs should consume core managed-auth command contracts through proxy_core_adapter"
    );
}

#[test]
fn production_managed_auth_account_selection_delegates_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    for relative_path in ["src/proxy/copilot_auth.rs", "src/proxy/codex_oauth_auth.rs"] {
        let path = manifest_dir.join(relative_path);
        let source = fs::read_to_string(&path).expect("read managed auth source");
        let fallback_slice = function_slice(
            &source,
            "fn fallback_default_account_id",
            "    fn sorted_accounts",
        );
        let sort_slice = function_slice(
            &source,
            "fn sorted_accounts",
            "    async fn resolve_default_account_id",
        );

        for marker in [
            ".max_by(",
            "authenticated_at.cmp",
            "id_b.cmp(id_a)",
            "let a_default",
            "let b_default",
            "b_default.cmp",
        ] {
            assert!(
                !fallback_slice.contains(marker) && !sort_slice.contains(marker),
                "{relative_path} should not own managed-auth account selection marker `{marker}`"
            );
        }

        assert!(
            fallback_slice.contains("managed_auth_fallback_default_account_id(")
                && fallback_slice.contains("ManagedAuthDefaultAccountCandidate::new(")
                && sort_slice.contains("compare_managed_auth_account_order(")
                && sort_slice.contains("ManagedAuthAccountSortKey::new("),
            "{relative_path} should delegate managed-auth default selection and account ordering to core"
        );
    }
}

#[test]
fn production_managed_auth_legacy_command_dtos_delegate_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let copilot_path = manifest_dir.join("src/proxy/copilot_auth.rs");
    let copilot_source = fs::read_to_string(&copilot_path).expect("read copilot_auth.rs");
    let codex_path = manifest_dir.join("src/proxy/codex_oauth_auth.rs");
    let codex_source = fs::read_to_string(&codex_path).expect("read codex_oauth_auth.rs");

    for marker in [
        "pub struct GitHubDeviceCodeResponse",
        "pub struct GitHubAccount",
        "pub struct CopilotAuthStatus",
    ] {
        assert!(
            !copilot_source.contains(marker),
            "copilot_auth.rs should not own legacy managed-auth command DTO marker `{marker}`"
        );
    }
    assert!(
        !codex_source.contains("pub struct CodexOAuthStatus"),
        "codex_oauth_auth.rs should not own Codex OAuth status DTO"
    );

    assert!(
        copilot_source.contains("CopilotAuthStatus")
            && copilot_source.contains("GitHubAccount")
            && copilot_source.contains("GitHubDeviceCodeResponse")
            && copilot_source.contains("pub use crate::proxy_core::api::auth::{")
            && !copilot_source.contains("proxy_core_adapter::{CopilotAuthStatus")
            && !copilot_source.contains("proxy_core_adapter::CopilotAuthStatus")
            && !copilot_source.contains("proxy_core_adapter::GitHubAccount")
            && !copilot_source.contains("proxy_core_adapter::GitHubDeviceCodeResponse"),
        "copilot_auth.rs should re-export legacy managed-auth command DTOs directly from proxy_core"
    );
    assert!(
        codex_source.contains("use crate::proxy_core::api::auth::CodexOAuthStatus;")
            && !codex_source.contains("proxy_core_adapter::CodexOAuthStatus")
            && !codex_source.contains("CodexOAuthDevicePollStatusKind, CodexOAuthStatus"),
        "codex_oauth_auth.rs should consume the core Codex OAuth status DTO directly"
    );

    for (line_index, line) in production_lines(&codex_source) {
        let code = line.split("//").next().unwrap_or_default();
        let direct_core =
            code.contains("crate::proxy_core::") || code.contains("cc_switch_proxy_core::");
        if direct_core && code.trim() != "use crate::proxy_core::api::auth::CodexOAuthStatus;" {
            panic!(
                "src/proxy/codex_oauth_auth.rs:{} contains non-DTO direct proxy-core import `{}`",
                line_index + 1,
                code.trim()
            );
        }
    }
}

#[test]
fn production_managed_auth_status_assembly_delegates_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let copilot_path = manifest_dir.join("src/proxy/copilot_auth.rs");
    let copilot_source = fs::read_to_string(&copilot_path).expect("read copilot_auth.rs");
    let codex_path = manifest_dir.join("src/proxy/codex_oauth_auth.rs");
    let codex_source = fs::read_to_string(&codex_path).expect("read codex_oauth_auth.rs");
    let copilot_status_slice = function_slice(
        &copilot_source,
        "pub async fn get_status",
        "    pub async fn clear_auth",
    );
    let codex_status_slice = function_slice(
        &codex_source,
        "pub async fn get_status",
        "    // ==================== 内部方法",
    );

    for marker in ["let authenticated", "let username", "account_list.first()"] {
        assert!(
            !copilot_status_slice.contains(marker) && !codex_status_slice.contains(marker),
            "managed-auth get_status should not own legacy status assembly marker `{marker}`"
        );
    }

    assert!(
        copilot_status_slice.contains("copilot_auth_status_from_parts(")
            && codex_status_slice.contains("codex_oauth_status_from_parts("),
        "managed-auth get_status implementations should delegate legacy status assembly to core"
    );
}

#[test]
fn model_fetch_command_delegates_user_agent_parsing_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/commands/model_fetch.rs");
    let source = fs::read_to_string(&path).expect("read model_fetch.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_MODEL_FETCH_COMMAND_PROVIDER_DETAIL_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/commands/model_fetch.rs:{} contains provider detail marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "model fetch command must delegate provider-specific User-Agent parsing to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_custom_user_agent_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let provider_path = manifest_dir.join("src/provider.rs");
    let provider_source = fs::read_to_string(&provider_path).expect("read provider.rs");

    let provider_user_agent_slice = function_slice(
        &adapter_source,
        "pub(crate) fn provider_custom_user_agent_header(",
        "pub(crate) fn model_fetch_custom_user_agent_header(",
    );

    assert!(
        adapter_source.contains("parse_custom_user_agent")
            && adapter_source.contains("core_provider_custom_user_agent_header"),
        "proxy_core_adapter should expose core custom User-Agent helpers"
    );
    assert!(
        provider_user_agent_slice.contains("core_provider_custom_user_agent_header("),
        "proxy_core_adapter should delegate provider custom User-Agent policy to core"
    );
    assert!(
        !adapter_source.contains("crate::provider::parse_custom_user_agent(")
            && !adapter_source.contains("HeaderValue::from_str("),
        "proxy_core_adapter must not own or call host-local custom User-Agent parsing policy"
    );
    for marker in [
        "if is_copilot",
        ".custom_user_agent_header().ok().flatten()",
    ] {
        assert!(
            !provider_user_agent_slice.contains(marker),
            "proxy_core_adapter must not own provider custom User-Agent policy marker `{marker}`"
        );
    }
    assert!(
        provider_source.contains("crate::proxy_core_adapter::parse_custom_user_agent(raw)")
            && !provider_source.contains("HeaderValue::from_str("),
        "provider.rs should keep only a compatibility wrapper around the adapter/core User-Agent parser"
    );
}

#[test]
fn production_stream_check_delegates_provider_adapters_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/services/stream_check.rs");
    let source = fs::read_to_string(&path).expect("read stream_check.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_STREAM_CHECK_PROVIDER_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/services/stream_check.rs:{} contains provider adapter marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    let retry_loop_slice = function_slice(
        &source,
        "pub async fn check_with_retry",
        "/// 合并供应商单独配置",
    );
    let merge_config_slice =
        function_slice(&source, "fn merge_provider_config", "async fn check_once");
    let build_result_slice = function_slice(&source, "fn build_result", "fn should_retry");
    assert!(
        source.contains("merge_stream_check_config(")
            && source.contains("stream_check_failed_result_with_retry_count(")
            && source.contains("stream_check_result_from_probe_result("),
        "stream_check should delegate config merging and result construction to proxy_core_adapter/core helpers"
    );
    assert!(
        retry_loop_slice.contains("stream_check_failed_result_with_retry_count(")
            && !retry_loop_slice.contains("message: \"Check failed\"")
            && !retry_loop_slice.contains("HealthStatus::Failed"),
        "check_with_retry should delegate terminal fallback failure envelope policy to core"
    );
    assert!(
        merge_config_slice.contains("merge_stream_check_config(")
            && merge_config_slice.contains("provider_stream_check_config_override(provider)")
            && !merge_config_slice.contains("timeout_secs:")
            && !merge_config_slice.contains("max_retries:")
            && !merge_config_slice.contains("degraded_threshold_ms:"),
        "merge_provider_config should only project provider overrides into the core config merge helper"
    );
    assert!(
        build_result_slice.contains("stream_check_result_from_probe_result(")
            && !build_result_slice.contains("status:")
            && !build_result_slice.contains("success:")
            && !build_result_slice.contains("message:")
            && !build_result_slice.contains("channel_reachability_status_from_latency(")
            && !build_result_slice.contains("HealthStatus::Failed"),
        "build_result should delegate StreamCheckResult envelope policy to core"
    );
    assert!(
        violations.is_empty(),
        "stream_check must resolve provider adapter facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn stream_check_service_owns_core_dto_reexports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let service_path = manifest_dir.join("src/services/stream_check.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let service_source = fs::read_to_string(&service_path).expect("read stream_check.rs");
    let required_reexport =
        "pub use crate::proxy_core::api::management::{StreamCheckConfig, StreamCheckResult};";

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_STREAM_CHECK_ADAPTER_DTO_EXPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains stream check DTO export marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    for (line_index, line) in production_lines(&service_source) {
        let code = line.split("//").next().unwrap_or_default();
        if code.contains("proxy_core_adapter")
            && (code.contains("StreamCheckConfig") || code.contains("StreamCheckResult"))
        {
            violations.push(format!(
                "src/services/stream_check.rs:{} imports stream check DTOs from proxy_core_adapter",
                line_index + 1
            ));
        }
        let direct_core =
            code.contains("crate::proxy_core::") || code.contains("cc_switch_proxy_core::");
        if direct_core && code.trim() != required_reexport {
            violations.push(format!(
                "src/services/stream_check.rs:{} contains non-stream-check direct proxy-core import `{}`",
                line_index + 1,
                code.trim()
            ));
        }
    }

    assert!(
        service_source.contains(required_reexport),
        "stream_check service must re-export public stream check DTOs directly from proxy_core"
    );
    assert!(
        violations.is_empty(),
        "stream_check public DTOs must bypass proxy_core_adapter aliases:\n{}",
        violations.join("\n")
    );
}

#[test]
fn gemini_auth_service_owns_core_auth_type_reexport() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let service_path = manifest_dir.join("src/services/provider/gemini_auth.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let service_source = fs::read_to_string(&service_path).expect("read gemini_auth.rs");
    let required_import = "pub(crate) use crate::proxy_core::api::ports::GeminiAuthType;";

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_GEMINI_AUTH_ADAPTER_DTO_EXPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains Gemini auth DTO export marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    for (line_index, line) in production_lines(&service_source) {
        let code = line.split("//").next().unwrap_or_default();
        if code.contains("proxy_core_adapter") && code.contains("GeminiAuthType") {
            violations.push(format!(
                "src/services/provider/gemini_auth.rs:{} imports GeminiAuthType from proxy_core_adapter",
                line_index + 1
            ));
        }
        let direct_core =
            code.contains("crate::proxy_core::") || code.contains("cc_switch_proxy_core::");
        if direct_core && code.trim() != required_import {
            violations.push(format!(
                "src/services/provider/gemini_auth.rs:{} contains non-GeminiAuthType direct proxy-core import `{}`",
                line_index + 1,
                code.trim()
            ));
        }
    }

    assert!(
        service_source.contains(required_import),
        "Gemini auth service must re-export GeminiAuthType directly from proxy_core"
    );
    assert!(
        service_source.contains("crate::proxy_core_adapter::detect_gemini_auth_type(provider)"),
        "Gemini auth service should keep provider-aware detection behind proxy_core_adapter"
    );
    assert!(
        violations.is_empty(),
        "Gemini auth type DTO must bypass proxy_core_adapter aliases:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_stream_check_command_delegates_proxy_target_filter_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/commands/stream_check.rs");
    let source = fs::read_to_string(&path).expect("read commands/stream_check.rs");
    let function = function_slice(
        &source,
        "pub async fn stream_check_all_providers",
        "/// 获取连通性检查配置",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_STREAM_CHECK_COMMAND_PROXY_TARGET_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/commands/stream_check.rs stream_check_all_providers:{} contains proxy-target filter marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        function.contains("stream_check_failed_result(")
            && !function.contains("HealthStatus::Failed")
            && !function.contains("status:")
            && !function.contains("success:")
            && !function.contains("response_time_ms:")
            && !function.contains("http_status:"),
        "stream_check_all_providers command should delegate fallback failure result envelopes to core"
    );
    assert!(
        violations.is_empty(),
        "stream_check_all_providers command must delegate proxy-target filter source projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_additive_stream_check_error_specs_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "fn missing_stream_check_base_url_error",
        "pub(crate) fn stream_check_proxy_target_ids_from_sources",
    );

    assert!(
        function.contains("core_additive_stream_check_base_url_missing_error_spec("),
        "stream-check missing base URL errors must be delegated to proxy-core"
    );
    for marker in [
        "opencode_base_url_missing",
        "openclaw_base_url_missing",
        "hermes_base_url_missing",
    ] {
        assert!(
            !function.contains(marker),
            "proxy_core_adapter must not own additive stream-check error marker `{marker}`"
        );
    }
}

#[test]
fn production_services_proxy_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub use crate::proxy::host::cc_switch::live_takeover::*;",
        ],
        "legacy services/proxy.rs must remain a re-export shim after live takeover host split"
    );
}

#[test]
fn production_proxy_service_delegates_takeover_status_sources_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn get_takeover_status",
        "/// 为指定应用开启/关闭 Live 接管",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_TAKEOVER_STATUS_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs get_takeover_status:{} contains takeover status source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::get_takeover_status must delegate proxy_config source reads and status projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_live_takeover_app_catalog_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let functions = [
        (
            "restore_live_configs",
            function_slice(
                &source,
                "async fn restore_live_configs",
                "async fn restore_live_config_for_app_with_fallback",
            ),
        ),
        (
            "detect_takeover_in_live_configs",
            function_slice(
                &source,
                "pub fn detect_takeover_in_live_configs",
                "/// 从供应商配置更新 Live 备份",
            ),
        ),
    ];

    let mut violations = Vec::new();
    for (function_name, function) in functions {
        for (line_index, line) in production_lines(function) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_SERVICE_LIVE_TAKEOVER_APP_LIST_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/host/cc_switch/live_takeover.rs {function_name}:{} contains live takeover app-list marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService must get the live-takeover app catalog from proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_official_warning_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn set_takeover_for_app",
        "fn read_claude_live",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_OFFICIAL_WARNING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs set_takeover_for_app:{} contains official-warning source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::set_takeover_for_app must delegate official-provider warning source reads and projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_current_provider_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let functions = [
        (
            "get_current_provider_for_app",
            function_slice(
                &source,
                "fn get_current_provider_for_app",
                "fn require_current_provider_for_app",
            ),
        ),
        (
            "require_current_provider_for_app",
            function_slice(
                &source,
                "fn require_current_provider_for_app",
                "/// 设置 AppHandle",
            ),
        ),
    ];

    let mut violations = Vec::new();
    for (function_name, function) in functions {
        for (line_index, line) in production_lines(function) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_SERVICE_CURRENT_PROVIDER_SOURCE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/host/cc_switch/live_takeover.rs {function_name}:{} contains current-provider source marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService current-provider helpers must delegate source reads and required-provider errors to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_live_token_sync_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "async fn sync_live_config_to_provider",
        "fn sync_live_token_to_provider_settings",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_LIVE_TOKEN_SYNC_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs sync_live_config_to_provider:{} contains live-token sync source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::sync_live_config_to_provider must delegate current-provider source reads and app support labels to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_takeover_enabled_config_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn set_takeover_for_app",
        "/// 同步 Live 配置中的 Token",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_TAKEOVER_ENABLED_CONFIG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs set_takeover_for_app:{} contains takeover enabled config marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::set_takeover_for_app must delegate proxy_config enabled reads and writes to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_takeover_backup_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn set_takeover_for_app",
        "/// 同步 Live 配置中的 Token",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_TAKEOVER_BACKUP_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs set_takeover_for_app:{} contains takeover backup source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::set_takeover_for_app must delegate takeover backup existence reads to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_takeover_backup_delete_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn set_takeover_for_app",
        "/// 同步 Live 配置中的 Token",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_TAKEOVER_BACKUP_DELETE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs set_takeover_for_app:{} contains takeover backup delete marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::set_takeover_for_app must delegate takeover backup deletion to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_takeover_active_flag_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn set_takeover_for_app",
        "/// 同步 Live 配置中的 Token",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_TAKEOVER_ACTIVE_FLAG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs set_takeover_for_app:{} contains takeover active flag marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::set_takeover_for_app must delegate legacy active-flag compatibility reads/writes to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_takeover_health_cleanup_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn set_takeover_for_app",
        "/// 同步 Live 配置中的 Token",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_TAKEOVER_HEALTH_CLEANUP_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs set_takeover_for_app:{} contains takeover health cleanup marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::set_takeover_for_app must delegate provider-health cleanup to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_start_takeover_backup_cleanup_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn start_with_takeover",
        "/// 为指定应用开启/关闭 Live 接管",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_START_TAKEOVER_BACKUP_CLEANUP_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs start_with_takeover:{} contains start-takeover backup cleanup marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::start_with_takeover must delegate all-backup cleanup to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_start_takeover_active_flag_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn start_with_takeover",
        "/// 为指定应用开启/关闭 Live 接管",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_START_TAKEOVER_ACTIVE_FLAG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs start_with_takeover:{} contains start-takeover active flag marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::start_with_takeover must delegate legacy active-flag compatibility writes to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_stop_restore_enabled_config_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn stop_with_restore",
        "/// 停止代理服务器（恢复 Live 配置，但保留 settings 表中的代理状态）",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_STOP_RESTORE_ENABLED_CONFIG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs stop_with_restore:{} contains stop-restore enabled config marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::stop_with_restore must delegate bulk enabled-state cleanup to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_simple_restore_backup_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "async fn restore_live_config_for_app_inner",
        "/// 恢复原始 Live 配置",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_SIMPLE_RESTORE_BACKUP_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs restore_live_config_for_app_inner:{} contains simple restore backup source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::restore_live_config_for_app_inner must delegate backup source reads and parse-error projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_fallback_restore_backup_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "async fn restore_live_config_for_app_with_fallback_inner",
        "fn write_live_config_for_app",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_FALLBACK_RESTORE_BACKUP_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs restore_live_config_for_app_with_fallback_inner:{} contains fallback restore backup source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::restore_live_config_for_app_with_fallback_inner must delegate backup source reads and parse-error projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_ssot_restore_provider_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "fn restore_live_from_ssot_for_app",
        "fn cleanup_takeover_placeholders_in_live_for_app",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_SSOT_RESTORE_PROVIDER_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs restore_live_from_ssot_for_app:{} contains SSOT restore provider source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::restore_live_from_ssot_for_app must delegate current-provider source reads and placeholder guard to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_ssot_restore_live_write_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "fn restore_live_from_ssot_for_app",
        "fn cleanup_takeover_placeholders_in_live_for_app",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_SSOT_RESTORE_LIVE_WRITE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs restore_live_from_ssot_for_app:{} contains SSOT restore live-write marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::restore_live_from_ssot_for_app must delegate SSOT live writes to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_live_backup_save_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let functions = [
        (
            "backup_live_configs",
            function_slice(
                &source,
                "async fn backup_live_configs",
                "/// 备份指定应用的 Live 配置（严格模式：目标配置不存在则返回错误）",
            ),
        ),
        (
            "backup_live_config_strict",
            function_slice(
                &source,
                "async fn backup_live_config_strict",
                "/// 构造写入 Live 的代理地址",
            ),
        ),
    ];

    let mut violations = Vec::new();
    for (function_name, function) in functions {
        for (line_index, line) in production_lines(function) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_SERVICE_LIVE_BACKUP_SAVE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/host/cc_switch/live_takeover.rs {function_name}:{} contains live backup save marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService must delegate live backup serialization and persistence to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_update_backup_existing_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "async fn update_live_backup_from_provider_inner",
        "pub async fn hot_switch_provider",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_UPDATE_BACKUP_EXISTING_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs update_live_backup_from_provider_inner:{} contains existing backup source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::update_live_backup_from_provider_inner must delegate existing backup source reads to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_update_backup_save_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "async fn update_live_backup_from_provider_inner",
        "pub async fn hot_switch_provider",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_UPDATE_BACKUP_SAVE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs update_live_backup_from_provider_inner:{} contains update backup save marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::update_live_backup_from_provider_inner must delegate provider-derived backup persistence to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_hot_switch_sources_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub(crate) async fn hot_switch_provider_inner",
        "#[cfg(test)]",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_HOT_SWITCH_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs hot_switch_provider_inner:{} contains hot-switch source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::hot_switch_provider_inner must delegate provider/current/backup source reads and current-provider persistence to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_keep_state_active_flag_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn stop_with_restore_keep_state",
        "/// 备份各应用的 Live 配置",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_KEEP_STATE_ACTIVE_FLAG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs stop_with_restore_keep_state:{} contains keep-state active flag marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::stop_with_restore_keep_state must delegate legacy active-flag cleanup to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_stop_restore_cleanup_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let functions = [
        (
            "stop_with_restore",
            function_slice(
                &source,
                "pub async fn stop_with_restore",
                "/// 停止代理服务器（恢复 Live 配置，但保留 settings 表中的代理状态）",
            ),
        ),
        (
            "stop_with_restore_keep_state",
            function_slice(
                &source,
                "pub async fn stop_with_restore_keep_state",
                "/// 备份各应用的 Live 配置",
            ),
        ),
    ];

    let mut violations = Vec::new();
    for (function_name, function) in functions {
        for (line_index, line) in production_lines(function) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_SERVICE_STOP_RESTORE_CLEANUP_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/host/cc_switch/live_takeover.rs {function_name}:{} contains stop-restore cleanup marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService stop restore paths must delegate DB cleanup to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_crash_recovery_cleanup_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "pub async fn recover_from_crash",
        "/// 检测 Live 配置是否处于\"被接管\"的残留状态",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_STOP_RESTORE_CLEANUP_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs recover_from_crash:{} contains crash-recovery cleanup marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService::recover_from_crash must delegate DB cleanup to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_global_proxy_enabled_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let functions = [
        (
            "start",
            function_slice(
                &source,
                "pub async fn start",
                "async fn start_before_takeover_if_ephemeral_port",
            ),
        ),
        (
            "stop",
            function_slice(
                &source,
                "pub async fn stop",
                "pub async fn stop_with_restore",
            ),
        ),
    ];

    let mut violations = Vec::new();
    for (function_name, function) in functions {
        for (line_index, line) in production_lines(function) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_SERVICE_GLOBAL_PROXY_ENABLED_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/host/cc_switch/live_takeover.rs {function_name}:{} contains global proxy enabled marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService start/stop must delegate global proxy_enabled persistence to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_proxy_config_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let functions = [
        (
            "start",
            function_slice(
                &source,
                "pub async fn start",
                "async fn persist_ephemeral_listen_port_if_needed",
            ),
        ),
        (
            "persist_ephemeral_listen_port_if_needed",
            function_slice(
                &source,
                "async fn persist_ephemeral_listen_port_if_needed",
                "async fn start_before_takeover_if_ephemeral_port",
            ),
        ),
        (
            "start_before_takeover_if_ephemeral_port",
            function_slice(
                &source,
                "async fn start_before_takeover_if_ephemeral_port",
                "/// 启动代理服务器（带 Live 配置接管）",
            ),
        ),
        (
            "build_proxy_urls",
            function_slice(
                &source,
                "async fn build_proxy_urls",
                "/// 接管各应用的 Live 配置",
            ),
        ),
        (
            "get_config",
            function_slice(&source, "pub async fn get_config", "/// 更新代理配置"),
        ),
        (
            "update_config",
            function_slice(
                &source,
                "pub async fn update_config",
                "/// 检查服务器是否正在运行",
            ),
        ),
    ];

    let mut violations = Vec::new();
    for (function_name, function) in functions {
        for (line_index, line) in production_lines(function) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_SERVICE_PROXY_CONFIG_SOURCE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/host/cc_switch/live_takeover.rs {function_name}:{} contains proxy config source marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService must delegate proxy_config source reads and persistence to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_server_factory_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let functions = [
        (
            "start",
            function_slice(
                &source,
                "pub async fn start",
                "async fn persist_ephemeral_listen_port_if_needed",
            ),
        ),
        (
            "update_config",
            function_slice(
                &source,
                "pub async fn update_config",
                "/// 检查服务器是否正在运行",
            ),
        ),
    ];

    let mut violations = Vec::new();
    for (function_name, function) in functions {
        for (line_index, line) in production_lines(function) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_SERVICE_SERVER_FACTORY_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/host/cc_switch/live_takeover.rs {function_name}:{} contains server factory marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService must delegate ProxyServer construction to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_imports_server_type_from_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_SERVER_TYPE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs:{} contains direct server type marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService must import the running proxy server type through proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_effective_settings_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let functions = [
        (
            "claude_provider_with_effective_settings",
            function_slice(
                &source,
                "fn claude_provider_with_effective_settings",
                "pub async fn sync_claude_live_from_provider_while_proxy_active",
            ),
        ),
        (
            "sync_codex_live_from_provider_while_proxy_active",
            function_slice(
                &source,
                "pub async fn sync_codex_live_from_provider_while_proxy_active",
                "fn get_current_provider_for_app",
            ),
        ),
        (
            "update_live_backup_from_provider_inner",
            function_slice(
                &source,
                "async fn update_live_backup_from_provider_inner",
                "pub async fn hot_switch_provider",
            ),
        ),
        (
            "hot_switch_provider_inner",
            function_slice(
                &source,
                "pub(crate) async fn hot_switch_provider_inner",
                "#[cfg(test)]",
            ),
        ),
    ];

    let mut violations = Vec::new();
    for (function_name, function) in functions {
        for (line_index, line) in production_lines(function) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_SERVICE_EFFECTIVE_SETTINGS_SOURCE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/host/cc_switch/live_takeover.rs {function_name}:{} contains effective settings source marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService must delegate common-config effective settings source reads to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_service_delegates_live_write_provider_facade_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/live_takeover.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/live_takeover.rs");
    let function = function_slice(
        &source,
        "    fn write_claude_live",
        "    fn read_codex_live",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_LIVE_WRITE_PROVIDER_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/live_takeover.rs write_claude_live:{} contains provider facade marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProxyService must call live write sanitizers through proxy_core_adapter, not services::provider facade:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_client_model_catalog_source_selection_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn client_model_catalog_from_app_source",
        "pub(crate) fn codex_client_model_catalog_raw_from_active_config",
    );

    assert!(
        function.contains("client_model_catalog_source_for_app"),
        "client model catalog app selection should be delegated to proxy-core"
    );

    let forbidden_markers = ["match app", "AppKind::Codex"];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs client_model_catalog_from_app_source:{} contains source selection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "client model catalog source selection belongs in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_route_candidate_empty_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn route_candidate_provider_ids_from_selection_result",
        "pub(crate) async fn route_candidate_provider_ids_from_router_source",
    );

    assert!(
        function.contains(
            "crate::proxy_core::api::routing::route_candidate_provider_ids_from_selection_result"
        ),
        "route candidate empty-selection policy should be delegated to proxy-core"
    );

    let forbidden_markers = [
        "AppError::NoProvidersConfigured",
        "AppError::AllProvidersCircuitOpen",
        "Ok(Vec::new())",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs route_candidate_provider_ids_from_selection_result:{} contains local empty-policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "route candidate empty-selection policy belongs in proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_live_placeholder_app_dispatch_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn live_config_has_proxy_placeholder_for_app",
        "pub(crate) fn live_backup_snapshot_from_live_config",
    );

    assert!(
        function.contains("core_live_config_has_proxy_placeholder_for_app("),
        "proxy_core_adapter must delegate live placeholder app dispatch to proxy-core"
    );
    assert!(
        function.contains("codex_config_has_proxy_placeholder(config, placeholder)"),
        "proxy_core_adapter should only project the host TOML bearer-token fact for Codex"
    );

    for marker in [
        "AppType::Claude =>",
        "AppType::Codex =>",
        "AppType::Gemini =>",
    ] {
        assert!(
            !function.contains(marker),
            "proxy_core_adapter must not keep app-specific placeholder dispatch marker `{marker}`"
        );
    }
}

#[test]
fn proxy_core_adapter_delegates_live_backup_snapshot_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn live_backup_snapshot_from_live_config",
        "pub(crate) fn provider_settings_with_live_token_sync",
    );

    assert!(
        function.contains("core_live_backup_snapshot_from_live_config("),
        "proxy_core_adapter must delegate live backup snapshot policy to proxy-core"
    );
    assert!(
        function.contains("codex_config_has_proxy_placeholder(config, placeholder)"),
        "proxy_core_adapter should only project the host TOML placeholder fact for Codex"
    );

    for marker in ["Some(config.clone())", "return Some", "return None"] {
        assert!(
            !function.contains(marker),
            "proxy_core_adapter must not keep live backup snapshot policy marker `{marker}`"
        );
    }
}

#[test]
fn production_provider_live_excludes_legacy_snapshot_restore_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/services/provider/live.rs");
    let source = fs::read_to_string(&path).expect("read services/provider/live.rs");

    for marker in [
        "pub(crate) enum LiveSnapshot",
        "impl LiveSnapshot",
        "pub(crate) fn restore(&self)",
        "crate::config::write_text_file(",
    ] {
        assert!(
            !source.contains(marker),
            "services/provider/live.rs must not retain legacy live snapshot restore marker `{marker}`"
        );
    }
}

#[test]
fn proxy_core_adapter_delegates_live_takeover_match_app_dispatch_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn live_takeover_config_matches_proxy_for_app",
        "fn proxy_urls_match",
    );

    assert!(
        function.contains("core_live_takeover_config_matches_proxy_for_app("),
        "proxy_core_adapter must delegate live takeover proxy-match app dispatch to proxy-core"
    );
    assert!(
        function.contains("CodexLiveTakeoverMatchFacts"),
        "proxy_core_adapter should project Codex TOML placeholder/base_url facts for proxy-core"
    );
    assert!(
        function.contains("codex_config_has_base_url_matching("),
        "proxy_core_adapter should keep host TOML base_url parsing as a projected Codex fact"
    );

    for marker in ["AppType::Claude =>", "AppType::Gemini =>"] {
        assert!(
            !function.contains(marker),
            "proxy_core_adapter must not keep app-specific takeover-match dispatch marker `{marker}`"
        );
    }
}

#[test]
fn proxy_core_adapter_delegates_hot_switch_takeover_policies_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn proxy_hot_switch_should_refresh_codex_live_from_backup",
        "pub(crate) fn toml_value_is_subset",
    );

    for marker in [
        "core_proxy_hot_switch_should_refresh_codex_live_from_backup(",
        "core_proxy_hot_switch_should_sync_codex_live_while_proxy_active(",
        "core_proxy_hot_switch_should_sync_claude_live_while_proxy_active(",
    ] {
        assert!(
            function.contains(marker),
            "proxy_core_adapter must delegate hot-switch takeover policy marker `{marker}` to proxy-core"
        );
    }

    for marker in ["has_live_backup || live_taken_over", "matches!(app_type"] {
        assert!(
            !function.contains(marker),
            "proxy_core_adapter must not keep hot-switch takeover policy marker `{marker}`"
        );
    }
}

#[test]
fn proxy_core_adapter_delegates_channel_key_settings_policy_to_typed_core_helper() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/auth_provider.rs");
    let source = fs::read_to_string(&source_path).expect("read auth_provider.rs");
    let function = source.as_str();
    let attempts_path =
        manifest_dir.join("src/proxy/host/cc_switch/channel_auth_profile_attempts.rs");
    let attempts_source =
        fs::read_to_string(&attempts_path).expect("read channel_auth_profile_attempts.rs");

    assert!(
        source.contains("pub(crate) fn provider_with_channel_auth_key(")
            && !adapter_runtime_source.contains("provider_with_channel_auth_key"),
        "provider_with_channel_auth_key should live in auth_provider.rs without a proxy_core_adapter re-export"
    );
    assert!(
        attempts_source.contains(
            "use crate::proxy::host::cc_switch::auth_provider::provider_with_channel_auth_key;",
        ) && !attempts_source
            .contains("proxy_core_adapter::{\n    provider_with_channel_auth_key"),
        "channel auth profile attempts should import provider_with_channel_auth_key from host auth_provider.rs"
    );
    assert!(
        function.contains("settings_config_with_channel_auth_key_for_app("),
        "provider_with_channel_auth_key must delegate app-typed channel key settings policy to proxy-core"
    );
    assert!(
        source.contains("use crate::proxy_core::api::auth::settings_config_with_channel_auth_key_for_app;"),
        "auth_provider.rs should import channel-key settings policy directly from proxy_core::api::auth"
    );
    assert!(
        !adapter_runtime_source.contains("settings_config_with_channel_auth_key_for_app"),
        "proxy_core_adapter should not re-export channel-key settings policy once auth_provider owns the call site"
    );
    assert!(
        function.contains("&AppKind::from(app_type)"),
        "provider_with_channel_auth_key should project AppType into core AppKind before applying channel key policy"
    );
    assert!(
        !function.contains("settings_config_with_channel_auth_key("),
        "provider_with_channel_auth_key must not call the legacy string channel key settings helper"
    );
    assert!(
        !function.contains("app_type.as_str()"),
        "provider_with_channel_auth_key must not pass app strings for channel key settings policy"
    );
}

#[test]
fn proxy_core_adapter_delegates_channel_auth_application_plan_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&source);
    let attempt_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/channel_auth_profile_attempts.rs");
    let attempt_source =
        fs::read_to_string(&attempt_source_path).expect("read channel_auth_profile_attempts.rs");
    let function = function_slice(
        &attempt_source,
        "pub(crate) fn apply_channel_auth_profile_providers_from_source",
        "pub(crate) fn required_forward_attempts_from_sources",
    );

    assert!(
        source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::channel_auth_profile_attempts::{"
        ) && !source.contains("fn apply_channel_auth_profile_providers_from_source(")
            && !source.contains("fn required_forward_attempts_from_sources("),
        "proxy_core_adapter should re-export, not own, the auth-profile attempt source helpers"
    );
    assert!(
        function.contains("channel_auth_profile_provider_application("),
        "apply_channel_auth_profile_providers_from_source must delegate channel auth application planning to proxy-core"
    );
    assert!(
        attempt_source.contains("use crate::proxy_core::api::auth::channel_auth_profile_missing_key_error;")
            && attempt_source.contains("use crate::proxy_core::api::domain::{")
            && attempt_source.contains("channel_auth_profile_provider_application")
            && attempt_source.contains("ChannelAuthProfileProviderApplication"),
        "channel auth profile attempt source should import pure auth/application rules directly from proxy_core::api"
    );
    for marker in [
        "channel_auth_profile_missing_provider_warning",
        "channel_auth_profile_action",
        "ChannelAuthProfileAction",
        "channel_auth_profile_provider_application",
        "ChannelAuthProfileProviderApplication",
        "channel_auth_profile_missing_key_error",
    ] {
        assert!(
            !adapter_runtime_source.contains(marker),
            "proxy_core_adapter should not re-export pure channel auth profile rule `{marker}` once attempt source owns the call site"
        );
    }
    assert!(
        function.contains("providers.contains_key(provider_id)"),
        "adapter should pass provider availability as a fact into the core channel auth plan"
    );
    assert!(
        !function.contains("channel_auth_profile_action("),
        "adapter must not bypass the core channel auth application plan with the lower-level action helper"
    );
    assert!(
        !function.contains("missing_provider_warning"),
        "adapter must not own missing-provider warning selection"
    );
}

#[test]
fn proxy_core_adapter_uses_channel_key_runtime_source_for_auth_profile_lookup() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let runtime_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/channel_key_runtime_source.rs");
    let runtime_source =
        fs::read_to_string(&runtime_source_path).expect("read channel_key_runtime_source.rs");
    let attempt_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/channel_auth_profile_attempts.rs");
    let attempt_source =
        fs::read_to_string(&attempt_source_path).expect("read channel_auth_profile_attempts.rs");
    let core_ports_path = manifest_dir.join("crates/proxy-core/src/ports.rs");
    let core_ports_source = fs::read_to_string(&core_ports_path).expect("read core ports.rs");
    let services_trait = function_slice(
        &core_ports_source,
        "pub trait ProxyServices",
        "pub trait ProxyConfigSource",
    );
    let services_path = manifest_dir.join("src/proxy/host/cc_switch/proxy_services.rs");
    let services_source = fs::read_to_string(&services_path).expect("read proxy_services.rs");
    let services_struct = function_slice(
        &services_source,
        "pub(crate) struct CcSwitchProxyServices",
        "impl<R> CcSwitchProxyServices",
    );
    let services_impl = services_source.as_str();
    let runtime_source_lookup = function_slice(
        &runtime_source,
        "fn load_channel_key_candidate_from_database",
        "impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource",
    );
    let source_function = function_slice(
        &attempt_source,
        "pub(crate) fn apply_channel_auth_profile_providers_from_source",
        "pub(crate) fn required_forward_attempts_from_sources",
    );
    let adapter_core_ports_import = function_slice(
        &source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );

    assert!(
        core_ports_source.contains("pub trait ChannelKeyRuntimeSource"),
        "proxy-core ports should expose channel key lookup behind a runtime source trait"
    );
    assert!(
        core_ports_source.contains("fn load_channel_key_candidate(")
            && core_ports_source.contains("ProxyCoreResult<Option<ChannelKeyRuntimeCandidate>>")
            && core_ports_source.contains("pub fn select_channel_key_runtime_candidate"),
        "ChannelKeyRuntimeSource should return the selected runtime candidate and core should own key-ref candidate selection"
    );
    assert!(
        services_trait.contains("channel_key_runtime_source("),
        "proxy-core ProxyServices should expose channel key runtime source as an injectable service"
    );
    assert!(
        !source.contains("trait ChannelKeyRuntimeSource"),
        "proxy_core_adapter should consume the core channel key runtime source, not define a host-local trait"
    );
    assert!(
        source.contains("ChannelKeyRuntimeSource"),
        "proxy_core_adapter should import the core channel key runtime source contract"
    );
    assert!(
        !adapter_core_ports_import.contains("ChannelKeyRuntimeSource"),
        "proxy_core_adapter should consume ChannelKeyRuntimeSource internally, not re-export the port trait"
    );
    assert!(
        runtime_source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && runtime_source
                .contains("use crate::proxy_core::api::ports::ChannelKeyRuntimeSource;")
            && attempt_source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && attempt_source.contains("use crate::proxy_core::api::ports::ChannelKeyRuntimeSource;"),
        "channel-key runtime and auth-profile host sources should import core result/port contracts directly"
    );
    assert!(
        !runtime_source.contains("ChannelKeyRuntimeSource, ProxyCoreResult")
            && !attempt_source.contains("ChannelKeyRuntimeSource,\n    ProxyCoreResult")
            && !attempt_source.contains("ProxyCoreResult, RoutePlan"),
        "channel-key host modules should not import ChannelKeyRuntimeSource or ProxyCoreResult through proxy_core_adapter"
    );
    assert!(
        !source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::channel_key_runtime_source::{"
        ) && runtime_source.contains("impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource")
            && !source.contains("impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource")
            && !source.contains("fn load_channel_key_candidate_from_database"),
        "DB-backed channel-key runtime source implementation should live in the host cc_switch module without adapter re-export"
    );
    assert!(
        services_struct.contains("channel_key_runtime_source: CcSwitchChannelKeyRuntimeSource"),
        "CC Switch service container should own the DB-backed channel key runtime source"
    );
    assert!(
        services_impl.contains("fn channel_key_runtime_source(")
            && services_impl.contains("&self.channel_key_runtime_source"),
        "CC Switch ProxyServices implementation should return the channel key runtime source"
    );
    assert!(
        source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::channel_auth_profile_attempts::{"
        ) && !source.contains("fn apply_channel_auth_profile_providers_from_source(")
            && !source.contains("fn required_forward_attempts_from_sources(")
            && source_function.contains("dyn ChannelKeyRuntimeSource")
            && source_function.contains(".load_channel_key_candidate(")
            && source_function.contains("key_candidate.key_value"),
        "host auth-profile attempt source should consume the channel key runtime source contract"
    );
    assert!(
        !source.contains("fn apply_channel_auth_profile_providers_from_db(")
            && !source.contains("CcSwitchBorrowedChannelKeyRuntimeSource")
            && !source.contains("fn channel_key_runtime_source_from_db("),
        "adapter must not retain test-only DB auth-profile convenience wrappers after source injection"
    );
    assert!(
        runtime_source_lookup.contains(".list_proxy_channel_key_runtime_candidates(")
            && runtime_source_lookup.contains("select_proxy_channel_key_runtime_candidate(")
            && !runtime_source_lookup.contains(".key_value"),
        "CC Switch channel key runtime lookup helper should own runtime DB candidate loading, core key-ref selection, and preserve selected candidate metadata"
    );
    assert!(
        runtime_source.contains("impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource")
            && runtime_source.contains("load_channel_key_candidate_from_database("),
        "owned CC Switch channel key runtime source should delegate through the shared lookup helper"
    );
}

#[test]
fn proxy_core_adapter_forward_pipeline_injects_channel_key_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let pipeline_path = manifest_dir.join("src/proxy/host/cc_switch/forward_pipeline.rs");
    let pipeline_source = fs::read_to_string(&pipeline_path).expect("read forward_pipeline.rs");
    let runtime_path = manifest_dir.join("src/proxy/host/cc_switch/proxy_runtime.rs");
    let runtime_source = fs::read_to_string(&runtime_path).expect("read proxy_runtime.rs");
    let attempt_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/channel_auth_profile_attempts.rs");
    let attempt_source =
        fs::read_to_string(&attempt_source_path).expect("read channel_auth_profile_attempts.rs");
    let pipeline_struct = function_slice(
        &pipeline_source,
        "pub(crate) struct CcSwitchForwardPipeline",
        "impl<R> CcSwitchForwardPipeline",
    );
    let pipeline_constructors = function_slice(
        &pipeline_source,
        "impl<R> CcSwitchForwardPipeline",
        "impl<R> ForwardPipeline for CcSwitchForwardPipeline",
    );
    let pipeline_impl = pipeline_source.as_str();
    let host_runtime_trait = function_slice(
        &runtime_source,
        "pub(crate) trait HostForwardRuntime",
        "#[derive(Clone)]",
    );
    let host_runtime_impl = runtime_source.as_str();
    let optional_runtime_function = pipeline_source.as_str();
    let host_forward_function = function_slice(
        &source,
        "pub(crate) async fn forward_proxy_request_with_host_runtime",
        "#[cfg(test)]\n#[allow(unused_imports)]\npub(crate) use crate::proxy::engine::response_pipeline::{",
    );
    let adapter_core_ports_import = function_slice(
        &source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    let adapter_production_routing_import = function_slice(
        &source,
        "pub(crate) use crate::proxy_core::api::routing::{\n    route_plan_no_matching_host_providers_error",
        "};\n\n#[cfg(test)]",
    );
    let attempt_source_function = attempt_source.as_str();

    assert!(
        !source.contains("pub(crate) use crate::proxy::host::cc_switch::forward_pipeline::CcSwitchForwardPipeline")
            && !source.contains("pub(crate) struct CcSwitchForwardPipeline")
            && !source.contains("impl<R> ForwardPipeline for CcSwitchForwardPipeline"),
        "proxy_core_adapter should not re-export or own the CC Switch forward pipeline"
    );
    assert!(
        pipeline_struct.contains("channel_key_runtime_source: CcSwitchChannelKeyRuntimeSource"),
        "CC Switch forward pipeline should own the DB-backed channel key runtime source"
    );
    assert!(
        pipeline_constructors
            .matches("channel_key_runtime_source: CcSwitchChannelKeyRuntimeSource")
            .count()
            >= 2,
        "CC Switch forward pipeline constructors should require an injected channel key runtime source"
    );
    assert!(
        pipeline_impl.contains("&self.channel_key_runtime_source"),
        "ForwardPipeline implementation should pass its channel key runtime source into host runtime dispatch"
    );
    assert!(
        !source.contains("pub(crate) trait HostForwardRuntime")
            && !source.contains("pub(crate) trait ProxyServiceRuntimeResources")
            && !source.contains("pub(crate) fn forward_with_optional_host_runtime"),
        "proxy_core_adapter should not own host runtime trait contracts or optional runtime dispatch"
    );
    assert!(
        !adapter_core_ports_import.contains("ForwardPipeline"),
        "proxy_core_adapter should not re-export the ForwardPipeline port for the host forward pipeline"
    );
    assert!(
        !adapter_production_routing_import
            .contains("forwarding_requires_runtime_error as forwarding_runtime_unavailable_error"),
        "proxy_core_adapter should keep runtime-unavailable alias test-only"
    );
    assert!(
        host_runtime_trait.contains("channel_key_runtime_source")
            && host_runtime_impl.contains("channel_key_runtime_source"),
        "HostForwardRuntime should receive channel key runtime source from the pipeline"
    );
    assert!(
        optional_runtime_function.contains("fn forward_with_optional_host_runtime")
            && optional_runtime_function.contains("forwarding_requires_runtime_error")
            &&
        optional_runtime_function
            .contains(".forward_host(channel_key_runtime_source, request, plan)"),
        "host forward pipeline optional runtime dispatcher should forward the injected channel key runtime source"
    );
    assert!(
        host_forward_function.contains("required_forward_attempts_from_sources(")
            && !host_forward_function.contains("required_forward_attempts_from_db_sources("),
        "host forward runtime should build attempts through source-injected auth profile handling"
    );
    assert!(
        attempt_source_function.contains("apply_channel_auth_profile_providers_from_source("),
        "required forward attempts source helper should delegate auth profile application through the source contract"
    );
}

#[test]
fn proxy_core_adapter_delegates_proxy_runtime_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let runtime_path = manifest_dir.join("src/proxy/host/cc_switch/proxy_runtime.rs");
    let runtime_source = fs::read_to_string(&runtime_path).expect("read proxy_runtime.rs");

    assert!(
        runtime_source.contains("pub(crate) struct CcSwitchProxyRuntime")
            && runtime_source.contains("pub(crate) trait ProxyServiceRuntimeResources")
            && runtime_source.contains("pub(crate) trait HostForwardRuntime")
            && runtime_source.contains("impl ProxyServiceRuntimeResources for CcSwitchProxyRuntime")
            && runtime_source.contains("impl HostForwardRuntime for CcSwitchProxyRuntime")
            && runtime_source.contains("forward_proxy_request_with_cc_switch_runtime("),
        "CC Switch proxy runtime data shape and runtime trait impls should live in host/cc_switch/proxy_runtime.rs"
    );
    assert!(
        adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime"
        ) && !adapter_source.contains("pub(crate) struct CcSwitchProxyRuntime")
            && !adapter_source
                .contains("impl ProxyServiceRuntimeResources for CcSwitchProxyRuntime")
            && !adapter_source.contains("impl HostForwardRuntime for CcSwitchProxyRuntime"),
        "proxy_core_adapter should re-export, not own, the CC Switch proxy runtime"
    );
}

#[test]
fn proxy_core_adapter_delegates_proxy_state_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let state_path = manifest_dir.join("src/proxy/host/cc_switch/proxy_state.rs");
    let state_source = fs::read_to_string(&state_path).expect("read proxy_state.rs");

    assert!(
        state_source.contains("pub struct ProxyState")
            && state_source.contains(
                "pub proxy_core_services: Arc<CcSwitchProxyServices<CcSwitchProxyRuntime>>"
            )
            && !state_source.contains("ProxyEngine::new(")
            && !state_source.contains("impl ProxyState"),
        "CC Switch proxy state data shape should live in host/cc_switch/proxy_state.rs"
    );
    assert!(
        adapter_source
            .contains("pub(crate) use crate::proxy::host::cc_switch::proxy_state::ProxyState")
            && !adapter_source.contains("\npub struct ProxyState")
            && !adapter_source.contains("type CcSwitchProxyRuntimeServices")
            && adapter_source.contains("\nimpl ProxyState")
            && adapter_source
                .contains("ProxyEngine<CcSwitchProxyServices<CcSwitchProxyRuntime>>")
            && adapter_source.contains("ProxyEngine::new(self.proxy_core_services.clone())"),
        "proxy_core_adapter should re-export the state and keep only the ProxyEngine construction boundary"
    );
}

#[test]
fn proxy_core_adapter_delegates_proxy_services_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let services_path = manifest_dir.join("src/proxy/host/cc_switch/proxy_services.rs");
    let services_source = fs::read_to_string(&services_path).expect("read proxy_services.rs");

    assert!(
        services_source.contains("pub(crate) struct CcSwitchProxyServices")
            && services_source.contains("impl<R> ProxyServices for CcSwitchProxyServices"),
        "CC Switch proxy service container should live in host/cc_switch/proxy_services.rs"
    );
    assert!(
        adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::proxy_services::CcSwitchProxyServices"
        ) && !adapter_source.contains("type CcSwitchProxyRuntimeServices")
            && !adapter_source.contains("pub(crate) struct CcSwitchProxyServices")
            && !adapter_source.contains("impl<R> ProxyServices for CcSwitchProxyServices"),
        "proxy_core_adapter should re-export the generic CC Switch proxy service container without a runtime alias"
    );
}

#[test]
fn proxy_core_adapter_delegates_http_server_lifecycle_to_transport_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let server_path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let server_source = fs::read_to_string(&server_path).expect("read transport/http/server.rs");

    assert!(
        server_source.contains("pub(crate) struct ProxyHttpServerHandles")
            && server_source.contains("pub(crate) async fn start_proxy_http_server")
            && server_source.contains("pub(crate) async fn bind_proxy_http_listener")
            && server_source.contains("pub(crate) fn proxy_http_router_from_state")
            && server_source.contains("pub(crate) fn spawn_proxy_http_accept_loop")
            && server_source.contains("pub(crate) async fn await_proxy_http_accept_loop_stop")
            && server_source.contains("pub(crate) async fn stop_proxy_http_server"),
        "HTTP server lifecycle and route/accept-loop helpers should live in transport/http/server.rs"
    );
    assert!(
        !adapter_source.contains("pub(crate) use crate::proxy::transport::http::server::{")
            && !adapter_source.contains("ProxyHttpServerHandles")
            && !adapter_source.contains("start_proxy_http_server")
            && !adapter_source.contains("stop_proxy_http_server")
            && !adapter_source.contains("pub(crate) struct ProxyHttpServerHandles")
            && !adapter_source.contains("pub(crate) async fn start_proxy_http_server")
            && !adapter_source.contains("pub(crate) fn spawn_proxy_http_accept_loop"),
        "proxy_core_adapter should not re-export or own the HTTP server lifecycle"
    );
}

#[test]
fn production_forwarder_delegates_managed_auth_resolution_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_MANAGED_AUTH_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains managed auth marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production forwarder must delegate managed account runtime access through proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_module_excludes_managed_account_auth_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mod_path = manifest_dir.join("src/proxy/mod.rs");
    let source = fs::read_to_string(&mod_path).expect("read proxy/mod.rs");
    let removed_path = manifest_dir.join("src/proxy/managed_account_auth.rs");

    assert!(
        !removed_path.exists(),
        "managed account runtime source should live in proxy_core_adapter, not src/proxy/managed_account_auth.rs"
    );

    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        if code.contains("managed_account_auth") {
            panic!(
                "src/proxy/mod.rs:{} declares managed account auth module after adapter-owned source migration",
                line_index + 1
            );
        }
    }
}

#[test]
fn production_cc_switch_host_owns_managed_account_tauri_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let host_path = manifest_dir.join("src/proxy/host/cc_switch/managed_account_runtime_source.rs");
    let host_source =
        fs::read_to_string(&host_path).expect("read managed_account_runtime_source.rs");
    let host_runtime_source = function_slice(
        &host_source,
        "impl CoreManagedAccountRuntimeSource for CcSwitchManagedAccountRuntimeSource",
        "#[cfg(test)]\npub(crate) async fn resolve_managed_account_auth_from_runtime_source",
    );

    for marker in [
        "managed_account_app_handle_unavailable_log_message(",
        "managed_account_app_handle_unavailable_error_message(",
        "managed_account_token_request_log_message(",
        "managed_account_token_success_log_message(",
        "managed_account_token_failure_log_message(",
        "managed_account_token_failure_error_message(",
        "copilot_token_from_app_handle(",
        "codex_oauth_token_from_app_handle(",
        "copilot_api_endpoint_from_app_handle(",
        "copilot_live_models_from_app_handle(",
        "copilot_model_vendor_from_app_handle(",
    ] {
        assert!(
            host_runtime_source.contains(marker),
            "host managed_account_runtime_source should own marker `{marker}`"
        );
    }
    assert!(
        host_source.contains("use crate::proxy_core::api::auth::{")
            && host_source.contains("managed_account_app_handle_unavailable_error_message")
            && host_source.contains("managed_account_token_failure_error_message")
            && host_source.contains("resolve_managed_account_auth_for_binding_with_runtime_source as resolve_core_managed_account_auth_for_binding_with_runtime_source")
            && host_source.contains("ManagedAccountRuntimeSource as CoreManagedAccountRuntimeSource")
            && host_source.contains("ProviderAuthInfo")
            && host_source.contains("use crate::proxy_core::api::model_catalog::CopilotModel;"),
        "managed_account_runtime_source.rs should import pure runtime source contracts and diagnostics directly from proxy_core::api"
    );
    for marker in [
        "managed_account_app_handle_unavailable_error_message",
        "managed_account_app_handle_unavailable_log_message",
        "managed_account_token_failure_error_message",
        "managed_account_token_failure_log_message",
        "managed_account_token_request_log_message",
        "managed_account_token_success_log_message",
        "resolve_core_copilot_dynamic_base_url_for_binding_with_runtime_source",
        "resolve_core_copilot_live_model_for_binding_with_runtime_source",
        "resolve_core_copilot_model_vendor_for_binding_with_runtime_source",
        "resolve_core_managed_account_auth_for_binding_with_runtime_source",
        "ManagedAccountAuthResolution",
        "ManagedAccountAuthRuntime",
        "CoreManagedAccountRuntimeSource",
    ] {
        assert!(
            !adapter_runtime_source.contains(marker),
            "proxy_core_adapter should not re-export managed-account runtime source helper/type `{marker}` once host runtime source owns the call site"
        );
    }
    assert!(
        adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::managed_account_runtime_source::{"
        ) && !adapter_source.contains(
            "impl CoreManagedAccountRuntimeSource for CcSwitchManagedAccountRuntimeSource"
        ) && !adapter_source.contains("fn copilot_token_from_app_handle(")
            && !adapter_source.contains("fn codex_oauth_token_from_app_handle("),
        "proxy_core_adapter should re-export, not own, the managed-account Tauri runtime source"
    );

    let forbidden_markers = ["crate::proxy::managed_account_auth"];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains proxy managed-auth module marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "managed account runtime source must not call the removed proxy managed-auth module:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_adapter_managed_auth_planning_uses_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/managed_account_runtime_source.rs");
    let source = fs::read_to_string(&path).expect("read managed_account_runtime_source.rs");
    let method = function_slice(
        &source,
        "fn resolve_auth_for_provider<'a>",
        "fn resolve_copilot_dynamic_base_url_for_provider<'a>",
    );

    assert!(
        method.contains("resolve_core_managed_account_auth_for_binding_with_runtime_source("),
        "adapter managed-auth provider extension must delegate runtime-token resolution to proxy-core"
    );
    assert!(
        method.contains("provider_managed_account_binding_context")
            && method.contains("binding_context.binding")
            && method.contains("binding_context.legacy_github_copilot_account_id"),
        "adapter managed-auth provider extension must consume structured CC Switch ProviderMeta binding context"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(method) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_ADAPTER_MANAGED_AUTH_PLAN_RUNTIME_CALL_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "host managed_account_runtime_source.rs ManagedAccountRuntimeSource::resolve_auth_for_provider:{} contains direct runtime call marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "host managed-auth provider extension must call core runtime-source orchestration instead of host auth functions directly:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_adapter_managed_auth_runtime_source_is_trait() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let host_path = manifest_dir.join("src/proxy/host/cc_switch/managed_account_runtime_source.rs");
    let host_source =
        fs::read_to_string(&host_path).expect("read managed_account_runtime_source.rs");

    assert!(
        host_source.contains("trait ManagedAccountRuntimeSource")
            && adapter_source.contains("ManagedAccountRuntimeSourceRef"),
        "managed-account runtime reads must stay behind a host source trait re-exported through adapter"
    );
}

#[test]
fn production_adapter_managed_auth_tests_use_runtime_source_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let host_path = manifest_dir.join("src/proxy/host/cc_switch/managed_account_runtime_source.rs");
    let source = fs::read_to_string(&host_path).expect("read managed_account_runtime_source.rs");
    let start = source
        .find("#[cfg(test)]\npub(crate) async fn resolve_managed_account_auth_from_runtime_source")
        .expect("find runtime-source test helper");
    let test_surface = &source[start..];

    assert!(
        test_surface.contains("runtime_source: &(dyn ManagedAccountRuntimeSource + Send + Sync)")
            && test_surface.contains(".resolve_auth_for_provider("),
        "managed-auth host test helper must keep using ManagedAccountRuntimeSource directly"
    );
    assert!(
        !test_surface.contains("app_handle: Option<&tauri::AppHandle>")
            && !test_surface.contains(
                "managed_account_runtime_source_from_app_handle(app_handle.cloned())"
            ),
        "managed-auth adapter test helpers must use ManagedAccountRuntimeSource directly instead of AppHandle wrappers"
    );
}
#[test]
fn production_forwarder_uses_managed_auth_runtime_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forwarder_path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&forwarder_path).expect("read engine/forward_pipeline.rs");
    let auth_source_path = manifest_dir.join("src/proxy/host/cc_switch/forwarder_auth_source.rs");
    let auth_source = fs::read_to_string(&auth_source_path).expect("read forwarder_auth_source.rs");
    let request_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/forwarder_request_source.rs");
    let request_source =
        fs::read_to_string(&request_source_path).expect("read forwarder_request_source.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let auth_source_slice = function_slice(
        &auth_source,
        "struct CcSwitchForwarderAuthSource",
        "impl ForwarderAuthSource for CcSwitchForwarderAuthSource",
    );
    let request_source_slice = function_slice(
        &request_source,
        "struct CcSwitchForwarderRequestSource",
        "impl ForwarderRequestSource for CcSwitchForwarderRequestSource",
    );

    assert!(
        !struct_slice.contains("managed_account_runtime_source")
            && struct_slice.contains("auth_source")
            && struct_slice.contains("request_source"),
        "RequestForwarder must depend on auth/request sources instead of holding managed-account runtime directly"
    );
    assert!(
        auth_source_slice
            .contains("managed_account_runtime_source: ManagedAccountRuntimeSourceRef")
            && request_source_slice
                .contains("managed_account_runtime_source: ManagedAccountRuntimeSourceRef"),
        "Forwarder auth/request sources must own managed-account runtime reads"
    );

    let forbidden_markers = [
        "resolve_managed_account_auth(self.app_handle.as_ref()",
        "resolve_copilot_api_endpoint(self.app_handle.as_ref()",
        "fetch_copilot_live_models(self.app_handle.as_ref()",
        "resolve_copilot_model_vendor(self.app_handle.as_ref()",
        "resolve_managed_account_auth_from_runtime_source(",
        "resolve_copilot_api_endpoint_from_runtime_source(",
        "fetch_copilot_live_models_from_runtime_source(",
        "resolve_copilot_model_vendor_from_runtime_source(",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains managed-auth app_handle source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must use the runtime managed-account source instead of app_handle wrappers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_failover_switch_scheduling_to_manager() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_FAILOVER_SWITCH_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains failover switch marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production forwarder must delegate failover switch scheduling to FailoverSwitchManager:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_uses_failover_switch_scheduler_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let failover_source_path = manifest_dir.join("src/proxy/host/cc_switch/failover_switch.rs");
    let failover_source =
        fs::read_to_string(&failover_source_path).expect("read failover_switch.rs");

    assert!(
        source.contains("failover_switch_scheduler"),
        "RequestForwarder must receive failover switch scheduling as an injected runtime source"
    );
    assert!(
        failover_source.contains("struct CcSwitchFailoverSwitchScheduler")
            && failover_source
                .contains("impl FailoverSwitchScheduler for CcSwitchFailoverSwitchScheduler"),
        "default failover switch scheduler implementation should live in the CC Switch host module"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::host::cc_switch::failover_switch::failover_switch_scheduler_from_runtime_sources")
            && !adapter_source.contains("struct CcSwitchFailoverSwitchScheduler"),
        "proxy_core_adapter should re-export, not own, the default failover switch scheduler"
    );

    let forbidden_markers = [
        "failover_switch::FailoverSwitchManager",
        "failover_manager:",
        "app_handle: Option<tauri::AppHandle>",
        "self.app_handle.clone()",
        "self.failover_manager",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains failover host resource marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must use an injected failover switch scheduler instead of host manager/AppHandle resources:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_copilot_dynamic_base_url_to_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forwarder_path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let forwarder_source =
        fs::read_to_string(&forwarder_path).expect("read engine/forward_pipeline.rs");
    let source_path =
        manifest_dir.join("src/proxy/host/cc_switch/managed_account_runtime_source.rs");
    let runtime_source =
        fs::read_to_string(&source_path).expect("read managed_account_runtime_source.rs");

    assert!(
        runtime_source.contains("apply_copilot_dynamic_base_url_for_provider"),
        "ManagedAccountRuntimeSource must expose provider-aware Copilot dynamic base URL mutation"
    );
    assert!(
        runtime_source.contains(
            "resolve_core_copilot_dynamic_base_url_for_binding_with_runtime_source("
        ),
        "ManagedAccountRuntimeSource must delegate Copilot dynamic endpoint account binding to proxy-core"
    );

    let impl_slice = function_slice(&forwarder_source, "impl RequestForwarder", "#[cfg(test)]");
    let forbidden_markers = [
        "resolve_copilot_dynamic_base_url_for_provider(",
        "resolve_copilot_api_endpoint_for_provider(",
        "resolved_copilot_dynamic_base_url(",
        "should_resolve_copilot_dynamic_endpoint(",
        "使用动态 API endpoint",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct Copilot dynamic endpoint marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must resolve Copilot dynamic base URLs through the managed-account runtime source:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_claude_api_format_to_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forwarder_path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let forwarder_source =
        fs::read_to_string(&forwarder_path).expect("read engine/forward_pipeline.rs");
    let source_path =
        manifest_dir.join("src/proxy/host/cc_switch/managed_account_runtime_source.rs");
    let runtime_source =
        fs::read_to_string(&source_path).expect("read managed_account_runtime_source.rs");

    assert!(
        runtime_source.contains("resolve_claude_api_format_for_adapter"),
        "ManagedAccountRuntimeSource must expose adapter-gated Claude API format resolution"
    );
    assert!(
        runtime_source.contains("resolve_core_copilot_model_vendor_for_binding_with_runtime_source("),
        "ManagedAccountRuntimeSource must delegate Copilot model vendor runtime gating to proxy-core"
    );

    let impl_slice = function_slice(&forwarder_source, "impl RequestForwarder", "#[cfg(test)]");
    let forbidden_markers = [
        "resolve_claude_api_format_for_provider(",
        "resolve_copilot_model_vendor_for_provider(",
        "resolve_forwarder_claude_api_format(",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct Claude API format runtime marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must resolve Claude API format through the managed-account runtime source:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_claude_body_policy_gate_to_request_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forwarder_path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let forwarder_source =
        fs::read_to_string(&forwarder_path).expect("read engine/forward_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    assert!(
        adapter_source.contains("api_format: Option<&'a str>"),
        "ForwarderRequestSource must own optional Claude API format gating for body policies"
    );
    let body_policy_input_slice = function_slice(
        &adapter_source,
        "pub(crate) struct ForwarderClaudeBodyPolicyInput",
        "pub(crate) struct ForwarderCodexResponsesToChatInput",
    );
    assert!(
        body_policy_input_slice.contains("adapter: &'a ForwarderAdapterContext"),
        "ForwarderRequestSource must receive adapter context for Claude body policy gating"
    );
    assert!(
        !body_policy_input_slice.contains("adapter_facts: &'a ForwarderAdapterFacts")
            && !body_policy_input_slice.contains("is_claude_adapter: bool"),
        "ForwarderClaudeBodyPolicyInput must not expose adapter facts or a split Claude-adapter gate"
    );

    let impl_slice = function_slice(&forwarder_source, "impl RequestForwarder", "#[cfg(test)]");
    let forbidden_markers = ["if let Some(api_format) = resolved_claude_api_format.as_deref()"];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct Claude body policy gate marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must delegate Claude body policy gating to the request source:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_codex_media_prevention_gate_to_request_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forwarder_path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let forwarder_source =
        fs::read_to_string(&forwarder_path).expect("read engine/forward_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    assert!(
        adapter_source.contains("apply_app_media_prevention"),
        "ForwarderRequestSource must expose app-gated media prevention"
    );
    assert!(
        adapter_source.contains("should_apply_forwarder_media_prevention_for_app"),
        "ForwarderRequestSource must delegate app media-prevention policy to proxy-core"
    );

    let impl_slice = function_slice(&forwarder_source, "impl RequestForwarder", "#[cfg(test)]");
    let adapter_media_prevention_slice = function_slice(
        &adapter_source,
        "fn apply_app_media_prevention",
        "fn media_retry_plan",
    );
    let forbidden_markers = ["matches!(app_type, AppType::Codex)"];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct Codex media prevention gate marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }
    for (line_index, line) in production_lines(adapter_media_prevention_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in ["matches!(input.app_type, AppType::Codex)"] {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs apply_app_media_prevention:{} contains adapter-local media prevention app gate marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must delegate Codex media prevention gating to the request source:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_claude_transform_gate_to_request_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forwarder_path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let forwarder_source =
        fs::read_to_string(&forwarder_path).expect("read engine/forward_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    assert!(
        adapter_source.contains("use_claude_transform"),
        "ForwarderTransformPlan must expose the Claude transform execution gate"
    );
    assert!(
        adapter_source.contains("use_provider_transform"),
        "ForwarderTransformPlan must expose the provider transform execution gate"
    );

    let impl_slice = function_slice(&forwarder_source, "impl RequestForwarder", "#[cfg(test)]");
    let forbidden_markers = ["if is_claude_adapter {", "} else if needs_transform {"];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct Claude transform gate marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must consume ForwarderTransformPlan for Claude transform gating:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_copilot_auth_delegates_usage_contract_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/copilot_auth.rs");
    let source = fs::read_to_string(&path).expect("read copilot_auth.rs");
    let fetch_usage_slice = function_slice(
        &source,
        "pub async fn fetch_usage_for_account",
        "/// 获取 Copilot 使用量信息（向后兼容",
    );
    let fetch_endpoint_slice = function_slice(
        &source,
        "async fn fetch_and_cache_endpoint",
        "async fn get_endpoint_lock",
    );

    assert!(
        !source.contains("pub struct CopilotUsageResponse")
            && !source.contains("proxy_core_adapter::CopilotUsageResponse")
            && !source.contains("pub struct CopilotEndpoints")
            && !source.contains("pub struct QuotaSnapshots")
            && !source.contains("pub struct QuotaDetail"),
        "copilot_auth.rs should not own Copilot usage DTO contracts"
    );
    assert!(
        source.contains("pub use crate::proxy_core::api::model_catalog::CopilotUsageResponse;"),
        "copilot_auth.rs should re-export CopilotUsageResponse directly from proxy_core"
    );
    assert!(
        fetch_usage_slice.contains("parse_copilot_usage_response_bytes(")
            && fetch_usage_slice.contains("copilot_usage_response_endpoint(&usage)")
            && !fetch_usage_slice.contains(".json()")
            && !fetch_usage_slice.contains("usage.endpoints"),
        "fetch_usage_for_account should delegate usage response parsing and endpoint extraction to core"
    );
    assert!(
        fetch_endpoint_slice.contains("parse_copilot_usage_response_bytes(")
            && fetch_endpoint_slice.contains("copilot_api_endpoint_from_usage_or_default(")
            && !fetch_endpoint_slice.contains("match usage.endpoints")
            && !fetch_endpoint_slice.contains("copilot_api_base(&domain)"),
        "fetch_and_cache_endpoint should delegate usage endpoint fallback policy to core"
    );
}

#[test]
fn production_copilot_auth_delegates_oauth_contract_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/copilot_auth.rs");
    let source = fs::read_to_string(&path).expect("read copilot_auth.rs");
    let token_slice = function_slice(&source, "impl CopilotToken", "/// Copilot Token API 响应");
    let poll_slice = function_slice(
        &source,
        "pub async fn poll_for_token",
        "        // 获取 access_token",
    );

    assert!(
        !source.contains("TOKEN_REFRESH_BUFFER_SECONDS")
            && !token_slice.contains("expires_at - now"),
        "copilot_auth.rs should delegate token refresh-buffer expiry policy to core"
    );

    for marker in [
        "\"authorization_pending\"",
        "\"slow_down\"",
        "\"expired_token\"",
        "\"access_denied\"",
        "error.as_str()",
        "error_description.unwrap_or_default()",
    ] {
        assert!(
            !poll_slice.contains(marker),
            "poll_for_token should not own OAuth polling error-code policy marker `{marker}`"
        );
    }

    assert!(
        token_slice.contains("copilot_token_is_expiring_soon(")
            && poll_slice.contains("copilot_oauth_poll_error_kind(")
            && poll_slice.contains("CopilotOAuthPollErrorKind::AuthorizationPending")
            && poll_slice.contains("CopilotOAuthPollErrorKind::ExpiredToken")
            && poll_slice.contains("CopilotOAuthPollErrorKind::AccessDenied")
            && poll_slice.contains("CopilotOAuthPollErrorKind::NetworkError"),
        "copilot_auth.rs should map core OAuth polling classifications into host errors"
    );
}

#[test]
fn production_copilot_token_response_keeps_consumed_fields_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/copilot_auth.rs");
    let source = fs::read_to_string(&path).expect("read copilot_auth.rs");
    let token_response_slice = function_slice(
        &source,
        "struct CopilotTokenResponse",
        "/// GitHub 用户信息",
    );

    assert!(
        token_response_slice.contains("token: String")
            && token_response_slice.contains("expires_at: i64"),
        "CopilotTokenResponse should keep the fields consumed by runtime token creation"
    );
    assert!(
        !token_response_slice.contains("refresh_in")
            && !token_response_slice.contains("#[allow(dead_code)]"),
        "CopilotTokenResponse should not preserve unused upstream token response fields"
    );
}

#[test]
fn production_codex_oauth_auth_delegates_device_poll_contract_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/codex_oauth_auth.rs");
    let source = fs::read_to_string(&path).expect("read codex_oauth_auth.rs");
    let start_slice = function_slice(
        &source,
        "pub async fn start_device_flow",
        "    /// 轮询 Device Code 状态",
    );
    let poll_slice = function_slice(
        &source,
        "pub async fn poll_for_token",
        "    /// 用 authorization_code + code_verifier 换取 tokens",
    );
    let token_slice = function_slice(
        &source,
        "impl CachedAccessToken",
        "/// 进行中的 Device Code 条目",
    );
    let valid_token_slice = function_slice(
        &source,
        "pub async fn get_valid_token_for_account",
        "    /// 获取默认账号的有效 token",
    );
    let exchange_slice = function_slice(
        &source,
        "async fn exchange_code_for_tokens",
        "    /// 用 refresh_token 刷新 access_token",
    );
    let refresh_slice = function_slice(
        &source,
        "async fn refresh_with_token",
        "    // ==================== Token 获取",
    );
    let identity_slice = function_slice(&source, "fn extract_identity_from_tokens", "#[cfg(test)]");

    for marker in [
        "const TOKEN_REFRESH_BUFFER_MS",
        "const DEVICE_CODE_DEFAULT_EXPIRES_IN",
        "const POLLING_SAFETY_MARGIN_SECS",
        "const CODEX_CLIENT_ID",
        "const DEVICE_AUTH_USERCODE_URL",
        "const DEVICE_AUTH_TOKEN_URL",
        "const OAUTH_TOKEN_URL",
        "const DEVICE_VERIFICATION_URL",
        "const DEVICE_REDIRECT_URI",
        "fn parse_interval(",
        "fn compute_expires_at_ms(",
        "serde_json::json!",
        ".form(&[",
        "Device Code 请求失败",
        "Token 交换失败",
        "Refresh 失败",
        "未找到对应的 user_code",
        "响应缺少 refresh_token",
        "无法从 token 中提取 account_id",
        "{status} - {text}",
        "struct IdTokenClaims",
        "struct OrgClaim",
        "struct OpenAiAuthClaim",
    ] {
        assert!(
            !source.contains(marker),
            "codex_oauth_auth.rs should not own Codex OAuth request/timing contract marker `{marker}`"
        );
    }

    for marker in [
        "StatusCode::FORBIDDEN",
        "StatusCode::NOT_FOUND",
        "StatusCode::GONE",
        "status.is_success()",
    ] {
        assert!(
            !poll_slice.contains(marker),
            "poll_for_token should not own Codex OAuth device poll status policy marker `{marker}`"
        );
    }

    for marker in [
        "claims.openai_auth",
        "claims.organizations.first()",
        "let mut account_id",
        ".or_else(||",
    ] {
        assert!(
            !identity_slice.contains(marker),
            "extract_identity_from_tokens should not own Codex OAuth identity fallback marker `{marker}`"
        );
    }

    assert!(
        start_slice.contains("codex_oauth_poll_interval_secs(")
            && start_slice.contains("codex_oauth_device_auth_usercode_url(")
            && start_slice.contains("codex_oauth_device_usercode_request_body(")
            && start_slice.contains("codex_oauth_device_code_request_failure(")
            && start_slice.contains("codex_oauth_device_code_expires_in_secs(")
            && start_slice.contains("codex_oauth_device_code_expires_at_ms(")
            && start_slice.contains("codex_oauth_pending_device_code_is_expired(")
            && start_slice.contains("codex_oauth_device_verification_url(")
            && poll_slice.contains("codex_oauth_missing_pending_user_code_message(")
            && poll_slice.contains("codex_oauth_device_auth_token_url(")
            && poll_slice.contains("codex_oauth_device_auth_token_request_body(")
            && poll_slice.contains("codex_oauth_pending_device_code_is_expired(")
            && poll_slice.contains("codex_oauth_device_poll_status_kind(")
            && poll_slice.contains("CodexOAuthDevicePollStatusKind::AuthorizationPending")
            && poll_slice.contains("CodexOAuthDevicePollStatusKind::ExpiredToken")
            && poll_slice.contains("CodexOAuthDevicePollStatusKind::Failed")
            && poll_slice.contains("CodexOAuthDevicePollStatusKind::Success")
            && poll_slice.contains("codex_oauth_device_poll_failure(")
            && poll_slice.contains("codex_oauth_missing_refresh_token_message(")
            && poll_slice.contains("codex_oauth_missing_account_id_message(")
            && exchange_slice.contains("codex_oauth_authorization_code_form(")
            && exchange_slice.contains("codex_oauth_token_url(")
            && exchange_slice.contains("codex_oauth_token_exchange_failure(")
            && refresh_slice.contains("codex_oauth_refresh_token_form(")
            && refresh_slice.contains("codex_oauth_token_url(")
            && refresh_slice.contains("codex_oauth_refresh_failure(")
            && token_slice.contains("codex_oauth_token_is_expiring_soon(")
            && valid_token_slice.contains("codex_oauth_access_token_expires_at_ms(")
            && source.contains("CodexOAuthTokenClaims")
            && identity_slice.contains("codex_oauth_identity_from_token_claims("),
        "codex_oauth_auth.rs should delegate request, timing, device poll status, and token identity contracts to core"
    );
}

#[test]
fn production_forwarder_delegates_copilot_live_model_resolution_to_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forwarder_path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let forwarder_source =
        fs::read_to_string(&forwarder_path).expect("read engine/forward_pipeline.rs");
    let source_path =
        manifest_dir.join("src/proxy/host/cc_switch/managed_account_runtime_source.rs");
    let runtime_source =
        fs::read_to_string(&source_path).expect("read managed_account_runtime_source.rs");

    assert!(
        runtime_source.contains("apply_copilot_live_model_for_adapter"),
        "ManagedAccountRuntimeSource must expose adapter-gated Copilot live model body resolution"
    );
    assert!(
        runtime_source.contains("resolve_core_copilot_live_model_for_binding_with_runtime_source("),
        "ManagedAccountRuntimeSource must delegate Copilot live model account binding to proxy-core"
    );

    let impl_slice = function_slice(&forwarder_source, "impl RequestForwarder", "#[cfg(test)]");
    let forbidden_markers = [
        "apply_copilot_live_model_for_provider(",
        "if is_copilot {",
        "apply_copilot_live_model_resolution(",
        "resolve_copilot_live_model_for_provider(",
        "fetch_copilot_live_models_for_provider(",
        "resolve_copilot_model_against_ids(",
        "live-model resolve:",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct Copilot live model marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must resolve Copilot live models through the managed-account runtime source:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_uses_request_source_for_managed_account_runtime() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forwarder_path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let forwarder_source =
        fs::read_to_string(&forwarder_path).expect("read engine/forward_pipeline.rs");
    let request_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/forwarder_request_source.rs");
    let request_source =
        fs::read_to_string(&request_source_path).expect("read forwarder_request_source.rs");
    let struct_slice = function_slice(
        &forwarder_source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&forwarder_source, "impl RequestForwarder", "#[cfg(test)]");
    let request_source_slice = function_slice(
        &request_source,
        "struct CcSwitchForwarderRequestSource",
        "impl ForwarderRequestSource for CcSwitchForwarderRequestSource",
    );

    assert!(
        !struct_slice.contains("managed_account_runtime_source"),
        "RequestForwarder must not hold managed-account runtime source directly"
    );
    assert!(
        request_source_slice
            .contains("managed_account_runtime_source: ManagedAccountRuntimeSourceRef"),
        "ForwarderRequestSource must own managed-account runtime source for request-side decisions"
    );

    let required_request_source_calls = [
        ".apply_copilot_live_model_for_adapter(",
        ".apply_copilot_dynamic_base_url_for_provider(",
        ".resolve_claude_api_format_for_adapter(",
    ];
    for marker in required_request_source_calls {
        assert!(
            impl_slice.contains(marker),
            "RequestForwarder must call request source method `{marker}`"
        );
    }
}

#[test]
fn production_forwarder_uses_auth_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    assert!(
        struct_slice.contains("auth_source"),
        "RequestForwarder must receive upstream auth header assembly as an injected source"
    );

    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let auth_source_path = manifest_dir.join("src/proxy/host/cc_switch/forwarder_auth_source.rs");
    let auth_source = fs::read_to_string(&auth_source_path).expect("read forwarder_auth_source.rs");
    assert!(
        adapter_source.contains(
            "type ForwarderAuthHeaders = crate::proxy_core::api::transport::ForwarderAuthHeaders"
        ),
        "ForwarderAuthHeaders DTO must be owned by proxy-core and exposed through an adapter alias"
    );
    let auth_input_slice = function_slice(
        &adapter_source,
        "pub(crate) struct ForwarderAuthHeadersInput",
        "pub(crate) trait ForwarderAuthSource",
    );
    let auth_source_slice = function_slice(
        &auth_source,
        "struct CcSwitchForwarderAuthSource",
        "impl ForwarderAuthSource for CcSwitchForwarderAuthSource",
    );
    let auth_trait_slice = function_slice(
        &adapter_source,
        "pub(crate) trait ForwarderAuthSource",
        "pub(crate) use crate::proxy::host::cc_switch::forwarder_auth_source::",
    );
    let auth_impl_slice = function_slice(
        &auth_source,
        "impl ForwarderAuthSource for CcSwitchForwarderAuthSource",
        "pub(crate) fn forwarder_auth_source_from_managed_account_runtime_source",
    );

    assert!(
        !auth_input_slice.contains("ManagedAccountRuntimeSourceRef"),
        "ForwarderAuthHeadersInput must not carry managed-account runtime source through RequestForwarder"
    );
    assert!(
        auth_source_slice
            .contains("managed_account_runtime_source: ManagedAccountRuntimeSourceRef"),
        "ForwarderAuthSource must own managed-account runtime source for auth resolution"
    );
    assert!(
        auth_source_slice.contains("auth_provider: AuthProviderRef"),
        "ForwarderAuthSource must own the core AuthProvider for route-context auth resolution"
    );
    assert!(
        auth_impl_slice.contains(".auth_provider") && auth_impl_slice.contains(".resolve_auth("),
        "ForwarderAuthSource must call core AuthProvider before assembling upstream auth headers"
    );
    assert!(
        auth_input_slice.contains("attempt:")
            && auth_input_slice.contains("request_body:")
            && auth_input_slice.contains("request_headers:"),
        "ForwarderAuthHeadersInput must carry route/request facts for core AuthProvider resolution"
    );
    assert!(
        auth_impl_slice.contains("fn prepare_optional_copilot_auth_optimization"),
        "default ForwarderAuthSource implementation must retain direct Copilot auth override preparation"
    );
    assert!(
        !auth_trait_slice.contains("prepare_copilot_auth_optimization"),
        "ForwarderAuthSource trait must not expose direct Copilot auth override helper"
    );
    assert!(
        auth_source.contains("use crate::proxy_core::api::domain::AppKind;")
            && auth_source.contains(
                "use crate::proxy_core::api::routing::auth_channel_spec_from_attempt;"
            )
            && auth_source.contains("use crate::proxy_core::api::transport::{")
            && auth_source.contains("auth_provider_proxy_request_from_context")
            && auth_source.contains("finalize_forwarder_auth_headers")
            && auth_source.contains("prepare_optional_copilot_auth_optimization_for_forwarder")
            && auth_source.contains("resolve_auth_provider_headers")
            && auth_source.contains("AuthProviderHeaderResolution")
            && auth_source.contains("ForwarderAuthHeaderFinalizationInput"),
        "default ForwarderAuthSource should import pure auth/header helper contracts directly from proxy_core::api"
    );
    for marker in [
        "type ForwarderAuthHeaderFinalizationInput",
        "auth_channel_spec_from_attempt",
        "auth_provider_proxy_request_from_context",
        "finalize_forwarder_auth_headers",
        "prepare_optional_copilot_auth_optimization_for_forwarder",
        "resolve_auth_provider_headers",
        "AuthProviderHeaderResolution",
    ] {
        assert!(
            !adapter_runtime_source.contains(marker),
            "proxy_core_adapter should not re-export pure auth helper/type `{marker}` once auth source owns the call site"
        );
    }
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::host::cc_switch::forwarder_auth_source::forwarder_auth_source_from_managed_account_runtime_source")
            && !adapter_source.contains("struct CcSwitchForwarderAuthSource"),
        "proxy_core_adapter should re-export the forwarder auth source factory without owning the implementation"
    );
    let auth_source_forbidden_markers = [
        "forwarder_provider_auth_info(",
        "forwarder_provider_auth_headers(",
    ];
    let mut auth_source_violations = Vec::new();
    for (line_index, line) in production_lines(auth_impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in auth_source_forbidden_markers {
            if code.contains(marker) {
                auth_source_violations.push(format!(
                    "src/proxy/host/cc_switch/forwarder_auth_source.rs ForwarderAuthSource impl:{} contains direct provider adapter auth marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }
    assert!(
        auth_source_violations.is_empty(),
        "ForwarderAuthSource must use ForwarderAdapterContext for provider adapter auth fallback:\n{}",
        auth_source_violations.join("\n")
    );

    let impl_forbidden_markers = [
        "forwarder_provider_auth_info(",
        "forwarder_provider_auth_headers(",
        ".resolve_auth_for_provider(",
        "build_codex_oauth_session_headers(",
        "build_upstream_auth_headers(",
        "CopilotAuthHeaderOverrides",
        "UpstreamAuthHeadersInput",
        "resolve_copilot_optimizer_session_id(",
        "resolve_copilot_request_id_with_fallback(",
        "resolve_copilot_deterministic_interaction_id(",
        "prepare_copilot_auth_optimization(",
        "ForwarderCopilotAuthOptimizationInput",
        ".copilot_optimizer_config.request_classification",
        ".copilot_optimizer_config.deterministic_request_id",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in impl_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct auth assembly marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must use an injected auth source for upstream auth header assembly:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_uses_runtime_state_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let runtime_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/forwarder_runtime_state_source.rs");
    let runtime_source =
        fs::read_to_string(&runtime_source_path).expect("read forwarder_runtime_state_source.rs");
    let runtime_trait_slice = function_slice(
        &adapter_source,
        "pub(crate) trait ForwarderRuntimeStateSource",
        "pub(crate) use crate::proxy::host::cc_switch::forwarder_runtime_state_source",
    );
    let runtime_source_slice = function_slice(
        &runtime_source,
        "struct CcSwitchForwarderRuntimeStateSource",
        "impl CcSwitchForwarderRuntimeStateSource",
    );
    let runtime_inherent_impl_slice = function_slice(
        &runtime_source,
        "impl CcSwitchForwarderRuntimeStateSource",
        "impl ForwarderRuntimeStateSource for CcSwitchForwarderRuntimeStateSource",
    );
    let failure_decision_slice = function_slice(
        &adapter_source,
        "pub(crate) enum ForwarderFailureDecision",
        "pub(crate) enum ForwarderRectifierRetryFailureDecision",
    );
    let rectifier_retry_failure_decision_slice = function_slice(
        &adapter_source,
        "pub(crate) enum ForwarderRectifierRetryFailureDecision",
        "pub(crate) type ForwarderRectifierRetryKind",
    );
    let provider_failure_runtime_source_slice = function_slice(
        &adapter_source,
        "pub(crate) async fn record_forward_provider_failure_runtime_source",
        "pub(crate) async fn record_forward_provider_rectifier_retry_failure_runtime_source",
    );
    let provider_rectifier_failure_runtime_source_slice = function_slice(
        &adapter_source,
        "pub(crate) async fn record_forward_provider_rectifier_retry_failure_runtime_source",
        "pub(crate) fn record_proxy_server_started_status",
    );

    assert!(
        struct_slice.contains("runtime_state_source"),
        "RequestForwarder must receive status/current-provider/events as one injected runtime state source"
    );
    assert!(
        runtime_source_slice.contains("status: Arc<RwLock<ProxyRuntimeStatus>>")
            && runtime_source_slice.contains("current_providers: Arc<RwLock")
            && runtime_source_slice.contains("events: Arc<ProxyEventBus>"),
        "default ForwarderRuntimeStateSource implementation must own status/current-provider/events runtime resources"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::host::cc_switch::forwarder_runtime_state_source::forwarder_runtime_state_source_from_runtime_parts")
            && !adapter_source.contains("struct CcSwitchForwarderRuntimeStateSource"),
        "proxy_core_adapter should re-export, not own, the default forwarder runtime state source"
    );
    assert!(
        !runtime_trait_slice.contains("fn status(") && !runtime_trait_slice.contains("fn events("),
        "ForwarderRuntimeStateSource trait must not expose runtime status or event bus read handles"
    );
    assert!(
        adapter_source.contains("fn forwarder_rectifier_retry_success_log_line")
            && adapter_source.contains("fn forwarder_rectifier_retry_failure_log_line"),
        "adapter must retain rectifier retry log-line projection helpers"
    );
    assert!(
        adapter_source.contains("fn terminal_forward_failure_log_line_for_error"),
        "adapter must retain terminal failure log-line projection helper"
    );
    assert!(
        adapter_source.contains("fn retryable_forward_failure_log_line"),
        "adapter must retain retryable failure log-line projection helper"
    );
    assert!(
        !runtime_inherent_impl_slice.contains("fn rectifier_retry_success_log_line")
            && !runtime_inherent_impl_slice.contains("fn rectifier_retry_failure_log_line"),
        "default ForwarderRuntimeStateSource implementation must not retain private rectifier log-line helpers"
    );
    assert!(
        !runtime_inherent_impl_slice.contains("fn terminal_forward_failure_log_line_for_error"),
        "default ForwarderRuntimeStateSource implementation must not retain private terminal failure log-line helper"
    );
    assert!(
        !runtime_inherent_impl_slice.contains("fn retryable_forward_failure_log_line"),
        "default ForwarderRuntimeStateSource implementation must not retain private retryable failure log-line helper"
    );
    assert!(
        !runtime_trait_slice.contains("rectifier_retry_success_log_line")
            && !runtime_trait_slice.contains("rectifier_retry_failure_log_line"),
        "ForwarderRuntimeStateSource trait must not expose internal rectifier retry log-line helpers"
    );
    assert!(
        !runtime_trait_slice.contains("terminal_forward_failure_log_line_for_error"),
        "ForwarderRuntimeStateSource trait must not expose internal terminal failure log-line helper"
    );
    assert!(
        !runtime_trait_slice.contains("retryable_forward_failure_log_line"),
        "ForwarderRuntimeStateSource trait must not expose internal retryable failure log-line helper"
    );
    assert!(
        runtime_trait_slice.contains("fn forward_failure_decision(&self, error: &ProxyError)"),
        "ForwarderRuntimeStateSource failure classification must only depend on ProxyError"
    );
    assert!(
        failure_decision_slice.contains("Retryable")
            && !failure_decision_slice.contains("error_message"),
        "ForwarderFailureDecision must not carry formatted error messages back to RequestForwarder"
    );
    assert!(
        rectifier_retry_failure_decision_slice.contains("ProviderFailure")
            && !rectifier_retry_failure_decision_slice.contains("error_message"),
        "ForwarderRectifierRetryFailureDecision must not carry formatted error messages back to RequestForwarder"
    );
    assert!(
        runtime_trait_slice.contains("emit_attempt_failed_for_error")
            && runtime_trait_slice.contains("error: &ProxyError"),
        "ForwarderRuntimeStateSource must own attempt-failed error message projection"
    );
    assert!(
        provider_failure_runtime_source_slice.contains("provider: &Provider")
            && provider_failure_runtime_source_slice.contains("error: &ProxyError")
            && !provider_failure_runtime_source_slice.contains("provider_name: &str")
            && !provider_failure_runtime_source_slice.contains("error_message: &str"),
        "provider failure runtime source helper must consume structured provider/error facts"
    );
    assert!(
        provider_rectifier_failure_runtime_source_slice.contains("provider: &Provider")
            && provider_rectifier_failure_runtime_source_slice
                .contains("kind: ForwarderRectifierRetryKind")
            && provider_rectifier_failure_runtime_source_slice.contains("error: &ProxyError")
            && !provider_rectifier_failure_runtime_source_slice.contains("provider_name: &str")
            && !provider_rectifier_failure_runtime_source_slice.contains("rectifier_label: &str")
            && !provider_rectifier_failure_runtime_source_slice.contains("error_message: &str"),
        "provider rectifier failure runtime source helper must consume structured provider/kind/error facts"
    );
    assert!(
        impl_slice.contains("log_rectifier_retry_success(")
            && impl_slice.contains("log_rectifier_retry_failure("),
        "RequestForwarder must trigger rectifier retry logging through runtime-state behavior methods"
    );
    assert!(
        impl_slice.contains("log_terminal_forward_failure("),
        "RequestForwarder must trigger terminal failure logging through a runtime-state behavior method"
    );
    assert!(
        impl_slice.contains("log_retryable_forward_failure("),
        "RequestForwarder must trigger retryable failure logging through a runtime-state behavior method"
    );
    assert!(
        impl_slice.contains("forward_failure_decision(&e)"),
        "RequestForwarder must classify forward failures without passing retry log context"
    );

    let struct_forbidden_markers = [
        "status: Arc<RwLock<ProxyRuntimeStatus>>",
        "current_providers: Arc<RwLock",
        "events: Arc<ProxyEventBus>",
    ];
    let impl_forbidden_markers = [
        "self.status",
        "self.current_providers",
        "self.events",
        "= should_failover_after_rectifier_retry_failure(",
        "forward_failure_kind_from_proxy_error(",
        "categorize_forward_failure(",
        "build_retryable_forward_failure_log(",
        "build_terminal_forward_failure_log(",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(struct_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in struct_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs RequestForwarder:{} contains runtime state field marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }
    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in impl_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct runtime state marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must use an injected runtime state source instead of direct status/current-provider/events fields:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_rectifier_error_message_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/forwarder_request_source.rs");
    let source = fs::read_to_string(&path).expect("read forwarder_request_source.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn forwarder_rectifier_error_message",
        "pub(crate) fn forwarder_request_source_from_managed_account_runtime_source",
    );

    assert!(
        function.contains("core_forwarder_rectifier_error_message("),
        "default request source must delegate rectifier error-message selection to proxy-core"
    );
    assert!(
        function.contains("ForwarderRectifierErrorInput::Upstream"),
        "adapter should project upstream error body facts into the core rectifier error input"
    );
    assert!(
        function.contains("ForwarderRectifierErrorInput::Other"),
        "adapter should project non-upstream error text into the core rectifier error input"
    );
    assert!(
        !function.contains("body.clone()"),
        "adapter must not own upstream-body rectifier message policy"
    );
}

#[test]
fn production_forwarder_active_connection_guard_uses_runtime_state_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let guard_slice = function_slice(
        &source,
        "pub(crate) struct ActiveConnectionGuard",
        "pub(crate) trait ForwarderRuntimeStateSource",
    );

    assert!(
        guard_slice.contains("runtime_state_source"),
        "ActiveConnectionGuard must receive active connection lifecycle through the runtime state source"
    );

    let forbidden_markers = [
        "status: Arc<RwLock<ProxyRuntimeStatus>>",
        "Arc<RwLock<ProxyRuntimeStatus>>",
        "record_forward_active_connection_acquired_runtime_source(",
        "record_forward_active_connection_released_runtime_source(",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(guard_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs ActiveConnectionGuard:{} contains direct active-connection runtime marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "active connection guard must use runtime state source lifecycle methods instead of direct status/helper calls:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_request_lifecycle_uses_runtime_state_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    let forbidden_markers = [
        "emit_request_started_event_source(",
        "record_forward_request_started_runtime_source(",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct request lifecycle marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "request lifecycle event/status updates must use runtime state source methods:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_attempt_events_use_runtime_state_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    let forbidden_markers = ["emit_attempt_event_source("];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct attempt event marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "attempt lifecycle event emission must use runtime state source methods:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_active_route_target_uses_runtime_state_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    let forbidden_markers = ["record_forward_active_route_target_runtime_source("];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct active-route target marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "active route target writes and route-selected events must use runtime state source methods:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_status_updates_use_runtime_state_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    let forbidden_markers = [
        "record_forward_success_runtime_source(",
        "record_forward_failure_runtime_source(",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct status update marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "success/failure status updates must use runtime state source methods:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_provider_status_updates_use_runtime_state_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    let forbidden_markers = [
        "record_forward_current_provider_runtime_source(",
        "record_forward_provider_failure_runtime_source(",
        "record_forward_provider_rectifier_retry_failure_runtime_source(",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct provider status marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider/current status updates must use runtime state source methods:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_uses_protocol_state_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let protocol_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/forwarder_protocol_state_source.rs");
    let protocol_source =
        fs::read_to_string(&protocol_source_path).expect("read forwarder_protocol_state_source.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    assert!(
        struct_slice.contains("protocol_state_source"),
        "RequestForwarder must receive Gemini/Codex protocol state as one injected source"
    );
    assert!(
        source.contains("ForwarderCodexChatProtocolEnrichmentInput"),
        "ForwarderProtocolStateSource must receive Codex chat enrichment gates as input"
    );
    assert!(
        protocol_source.contains("struct CcSwitchForwarderProtocolStateSource")
            && protocol_source.contains(
                "impl ForwarderProtocolStateSource for CcSwitchForwarderProtocolStateSource"
            ),
        "default ForwarderProtocolStateSource implementation should live in the CC Switch host module"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::host::cc_switch::forwarder_protocol_state_source::forwarder_protocol_state_source_from_runtime_parts")
            && !adapter_source.contains("struct CcSwitchForwarderProtocolStateSource"),
        "proxy_core_adapter should re-export, not own, the default forwarder protocol state source"
    );

    let struct_forbidden_markers = [
        "gemini_shadow: Arc<GeminiShadowStore>",
        "codex_chat_history: Arc<CodexChatHistoryStore>",
    ];
    let impl_forbidden_markers = [
        "self.gemini_shadow",
        "self.codex_chat_history",
        "self.protocol_state_source.gemini_shadow()",
        "self.protocol_state_source.codex_chat_history()",
        "let restored = self",
        "Restored or enriched",
        "if codex_responses_to_chat {",
        "unwrap_or(\"anthropic\")",
        "then_some(self.session_id.as_str())",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(struct_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in struct_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs RequestForwarder:{} contains protocol state field marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }
    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in impl_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct protocol state marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must use an injected protocol state source instead of direct Gemini/Codex state fields:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_uses_attempt_runtime_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let attempt_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/forwarder_attempt_runtime_source.rs");
    let attempt_source =
        fs::read_to_string(&attempt_source_path).expect("read forwarder_attempt_runtime_source.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    assert!(
        struct_slice.contains("attempt_runtime_source"),
        "RequestForwarder must receive attempt runtime as one injected source"
    );
    assert!(
        adapter_source.contains("pub(crate) struct ForwarderAttemptAllowInput"),
        "ForwarderAttemptRuntimeSource must receive allow facts through an input DTO"
    );
    assert!(
        adapter_source.contains("pub(crate) enum ForwarderAttemptAllowDecision"),
        "ForwarderAttemptRuntimeSource must return a structured allow decision"
    );
    assert!(
        adapter_source.contains("    Stop,"),
        "ForwarderAttemptAllowDecision::Stop must not carry formatted log-line payloads"
    );
    assert!(
        adapter_source.contains("attempts: &'a [ForwardAttempt]"),
        "ForwarderAttemptAllowInput must carry all route attempts for default runtime compatibility decisions"
    );
    assert!(
        adapter_source.contains("attempted_providers: usize")
            && adapter_source.contains("max_attempts: usize"),
        "ForwarderAttemptAllowInput must carry max-attempt policy facts"
    );
    assert!(
        impl_slice.contains("allow(ForwarderAttemptAllowInput {")
            && impl_slice.contains("attempts: &attempts,")
            && impl_slice.contains("attempted_providers,")
            && impl_slice.contains("max_attempts: self.max_attempts,"),
        "RequestForwarder must pass attempt allow facts as an input DTO"
    );
    let attempt_runtime_impl_slice = function_slice(
        &attempt_source,
        "impl ForwarderAttemptRuntimeSource for CcSwitchForwarderAttemptRuntimeSource",
        "pub(crate) fn forwarder_attempt_runtime_source_from_router",
    );
    assert!(
        !adapter_source.contains("fn should_bypass_circuit_breaker"),
        "default ForwarderAttemptRuntimeSource implementation must not retain a private circuit-breaker bypass helper"
    );
    assert!(
        !adapter_source.contains("fn attempt_limit_reached"),
        "default ForwarderAttemptRuntimeSource implementation must not retain a private max-attempt helper"
    );
    assert!(
        attempt_runtime_impl_slice
            .contains("forwarder_attempt_runtime_decision(ForwarderAttemptRuntimeDecisionInput")
            && attempt_runtime_impl_slice.contains("runtime_decision.bypass_circuit_breaker")
            && attempt_runtime_impl_slice.contains("runtime_decision.limit_log_line"),
        "default ForwarderAttemptRuntimeSource implementation should delegate attempt limit and circuit-bypass policy to core"
    );
    assert!(
        attempt_source.contains("use crate::proxy_core::api::transport::{")
            && attempt_source.contains("forwarder_attempt_runtime_decision")
            && attempt_source.contains("ForwarderAttemptRuntimeDecisionInput"),
        "default ForwarderAttemptRuntimeSource should import pure attempt decision helper/input directly from proxy_core::api::transport"
    );
    for marker in [
        "forwarder_attempt_runtime_decision",
        "ForwarderAttemptRuntimeDecisionInput",
    ] {
        assert!(
            !adapter_runtime_source.contains(marker),
            "proxy_core_adapter should not re-export pure attempt runtime helper/input `{marker}` once attempt runtime source owns the call site"
        );
    }
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::host::cc_switch::forwarder_attempt_runtime_source::forwarder_attempt_runtime_source_from_router")
            && !adapter_source.contains("struct CcSwitchForwarderAttemptRuntimeSource"),
        "proxy_core_adapter should re-export, not own, the default forwarder attempt runtime source"
    );
    assert!(
        !adapter_source.contains("pub(crate) fn forwarder_attempt_limit_reached_log_line")
            && !adapter_source.contains("pub(crate) fn forwarder_should_bypass_circuit_breaker"),
        "proxy_core_adapter must not retain host-owned forward attempt limit or circuit-bypass policy helpers"
    );
    assert!(
        !impl_slice.contains("limit.log_line"),
        "RequestForwarder must not consume formatted max-attempt log-line payloads from attempt allow decisions"
    );
    let attempt_runtime_trait_slice = function_slice(
        &adapter_source,
        "pub(crate) trait ForwarderAttemptRuntimeSource",
        "pub(crate) use crate::proxy::host::cc_switch::forwarder_attempt_runtime_source",
    );
    let attempt_failure_runtime_source_slice = function_slice(
        &adapter_source,
        "pub(crate) async fn record_forward_attempt_failure_runtime_source",
        "pub(crate) async fn release_forward_attempt_permit_neutral_runtime_source",
    );
    assert!(
        !attempt_runtime_trait_slice.contains("should_bypass_circuit_breaker"),
        "ForwarderAttemptRuntimeSource trait must not expose the internal legacy circuit-breaker bypass helper"
    );
    assert!(
        !attempt_runtime_trait_slice.contains("attempt_limit_reached"),
        "ForwarderAttemptRuntimeSource trait must not expose the internal max-attempt helper"
    );
    assert!(
        attempt_failure_runtime_source_slice.contains("error: &ProxyError")
            && !attempt_failure_runtime_source_slice.contains("error_message: &str"),
        "attempt failure runtime source helper must consume ProxyError and own message projection"
    );

    let struct_forbidden_markers = ["router: Arc<ProviderRouter>"];
    let impl_forbidden_markers = [
        "self.router",
        "allow_forward_attempt_runtime_source(",
        "record_forward_attempt_success_runtime_source(",
        "record_forward_attempt_failure_runtime_source(",
        "release_forward_attempt_permit_neutral_runtime_source(",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(struct_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in struct_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs RequestForwarder:{} contains attempt runtime field marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }
    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in impl_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct attempt runtime marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must use an injected attempt runtime source instead of direct router/helper calls:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_failover_switch_delegates_proxy_config_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/failover_switch.rs");
    let source = fs::read_to_string(&path).expect("read host/cc_switch/failover_switch.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FAILOVER_SWITCH_CONFIG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/failover_switch.rs:{} contains proxy config marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "FailoverSwitchManager must delegate proxy_config reads and enabled policy to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_failover_switch_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/failover_switch.rs");
    let source = fs::read_to_string(&path).expect("read failover_switch.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub(crate) use super::host::cc_switch::failover_switch::*;",
        ],
        "legacy proxy/failover_switch.rs must remain a re-export shim after host/cc_switch split"
    );
}

#[test]
fn production_switch_proxy_provider_command_delegates_provider_policy_to_service() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/commands/proxy.rs");
    let source = fs::read_to_string(&path).expect("read commands/proxy.rs");
    let function = function_slice(
        &source,
        "pub async fn switch_proxy_provider",
        "// ==================== 故障转移相关命令 ====================",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_SWITCH_PROXY_PROVIDER_COMMAND_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/commands/proxy.rs switch_proxy_provider:{} contains provider policy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "switch_proxy_provider command must delegate provider lookup and takeover policy to ProxyService:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_reset_circuit_breaker_command_delegates_switchback_sources_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/commands/proxy.rs");
    let source = fs::read_to_string(&path).expect("read commands/proxy.rs");
    let function = function_slice(
        &source,
        "pub async fn reset_circuit_breaker",
        "/// 获取熔断器配置",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_RESET_CIRCUIT_BREAKER_COMMAND_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/commands/proxy.rs reset_circuit_breaker:{} contains switchback source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "reset_circuit_breaker command must delegate switchback source reads and decision projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_get_circuit_breaker_stats_command_delegates_to_proxy_service() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/commands/proxy.rs");
    let source = fs::read_to_string(&path).expect("read commands/proxy.rs");
    let start = source
        .find("pub async fn get_circuit_breaker_stats")
        .expect("find get_circuit_breaker_stats");
    let function = &source[start..];

    assert!(
        function.contains(".get_provider_circuit_breaker_stats(")
            && !function.contains("Ok(None)")
            && !function.contains("let _ ="),
        "get_circuit_breaker_stats command should expose runtime stats through ProxyService, not return a placeholder"
    );
}

#[test]
fn production_set_auto_failover_command_delegates_plan_sources_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/commands/failover.rs");
    let source = fs::read_to_string(&path).expect("read commands/failover.rs");
    let start = source
        .find("pub async fn set_auto_failover_enabled")
        .expect("missing set_auto_failover_enabled");
    let function = &source[start..];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_SET_AUTO_FAILOVER_COMMAND_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/commands/failover.rs set_auto_failover_enabled:{} contains auto-failover source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "set_auto_failover_enabled command must delegate plan source reads and core input construction to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_uses_transport_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    assert!(
        struct_slice.contains("transport_source"),
        "RequestForwarder must receive upstream transport execution as an injected source"
    );

    let transport_request_slice = function_slice(
        &adapter_source,
        "pub(crate) struct ForwarderUpstreamTransportRequest",
        "pub(crate) trait ForwarderTransportSource",
    );
    assert!(
        transport_request_slice.contains("request_parts: ForwarderUpstreamRequestParts"),
        "ForwarderUpstreamTransportRequest must carry cohesive request parts"
    );
    assert!(
        !transport_request_slice.contains("ordered_headers: HeaderMap")
            && !transport_request_slice.contains("body: Vec<u8>")
            && !transport_request_slice.contains("preserve_exact_header_case: bool"),
        "ForwarderUpstreamTransportRequest must not expose split request parts"
    );
    let forbidden_request_part_splits = [
        "let ordered_headers = request_parts.ordered_headers",
        "let body_bytes = request_parts.body",
        "let preserve_exact_header_case = request_parts.preserve_exact_header_case",
    ];
    for marker in forbidden_request_part_splits {
        assert!(
            !impl_slice.contains(marker),
            "RequestForwarder must pass ForwarderUpstreamRequestParts cohesively instead of split marker `{marker}`"
        );
    }

    let impl_forbidden_markers = [
        "super::http_client::get_current_proxy_url(",
        "super::http_client::get(",
        "transport::upstream::hyper_client::send_request(",
        "reqwest_send_error_to_proxy_error(",
        "resolve_upstream_send_policy(",
        "UpstreamSendPolicyInput",
        "UpstreamTransportKind::",
        "is_socks_proxy_url(",
        "invalid_upstream_url_error_message(",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in impl_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct transport marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must send upstream requests through an injected transport source:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_transport_source_delegates_to_upstream_transport_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_source = fs::read_to_string(manifest_dir.join("src/proxy_core_adapter.rs"))
        .expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let transport_source = fs::read_to_string(
        manifest_dir.join("src/proxy/host/cc_switch/forwarder_transport_source.rs"),
    )
    .expect("read forwarder_transport_source.rs");
    let upstream_source =
        fs::read_to_string(manifest_dir.join("src/proxy/transport/upstream/mod.rs"))
            .expect("read transport/upstream/mod.rs");
    let reqwest_source =
        fs::read_to_string(manifest_dir.join("src/proxy/transport/upstream/reqwest_client.rs"))
            .expect("read transport/upstream/reqwest_client.rs");
    let impl_slice = function_slice(
        &transport_source,
        "impl ForwarderTransportSource for CcSwitchForwarderTransportSource",
        "pub(crate) fn default_forwarder_transport_source",
    );

    assert!(
        impl_slice.contains("send_request(request).await"),
        "default ForwarderTransportSource must delegate upstream transport execution to proxy::transport::upstream"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::host::cc_switch::forwarder_transport_source::default_forwarder_transport_source")
            && !adapter_source.contains("struct CcSwitchForwarderTransportSource"),
        "proxy_core_adapter should re-export, not own, the default forwarder transport source"
    );

    let forbidden_adapter_markers = [
        "crate::proxy::http_client::get_current_proxy_url(",
        "crate::proxy::http_client::get(",
        "crate::proxy::host::cc_switch::global_http_client::get_current_proxy_url(",
        "crate::proxy::host::cc_switch::global_http_client::get(",
        "resolve_upstream_send_policy(",
        "UpstreamSendPolicyInput",
        "UpstreamTransportKind::",
        "reqwest_send_error_to_proxy_error(",
        "transport::upstream::hyper_client::send_request(",
        "invalid_upstream_url_error_message(",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_adapter_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/forwarder_transport_source.rs default transport source:{} contains direct upstream transport marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "default ForwarderTransportSource must stay a thin upstream transport module delegate:\n{}",
        violations.join("\n")
    );
    assert!(
        upstream_source.contains("resolve_upstream_send_policy(UpstreamSendPolicyInput")
            && upstream_source.contains("reqwest_client::send_request(")
            && upstream_source.contains("hyper_client::send_request("),
        "transport/upstream/mod.rs must own upstream transport policy dispatch"
    );
    assert!(
        upstream_source.contains("use crate::proxy_core::api::transport::{")
            && upstream_source.contains("invalid_upstream_url_error_message")
            && upstream_source.contains("is_socks_proxy_url")
            && upstream_source.contains("resolve_upstream_send_policy")
            && upstream_source.contains("UpstreamSendPolicyInput")
            && upstream_source.contains("UpstreamTransportKind"),
        "transport/upstream/mod.rs should import pure upstream send policy helpers directly from proxy_core::api::transport"
    );
    assert!(
        reqwest_source
            .contains("use crate::proxy_core::api::transport::streaming_header_timeout_message;"),
        "transport/upstream/reqwest_client.rs should import streaming header timeout diagnostics directly from proxy_core::api::transport"
    );
    for marker in [
        "type UpstreamSendPolicyInput",
        "type UpstreamTransportKind",
        "invalid_upstream_url_error_message",
        "is_socks_proxy_url",
        "resolve_upstream_send_policy",
        "streaming_header_timeout_message",
    ] {
        assert!(
            !adapter_runtime_source.contains(marker),
            "proxy_core_adapter should not re-export pure upstream transport helper/type `{marker}` once transport/upstream owns the call site"
        );
    }
    assert!(
        reqwest_source.contains("crate::proxy::host::cc_switch::global_http_client::get()")
            && reqwest_source.contains("reqwest_send_error_to_proxy_error"),
        "transport/upstream/reqwest_client.rs must own pooled reqwest upstream send execution"
    );
}

#[test]
fn production_forwarder_uses_request_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let request_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/forwarder_request_source.rs");
    let request_source =
        fs::read_to_string(&request_source_path).expect("read forwarder_request_source.rs");
    let core_transport_path = manifest_dir.join("crates/proxy-core/src/request_transport.rs");
    let core_transport_source =
        fs::read_to_string(&core_transport_path).expect("read request_transport.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");
    let request_impl_slice = function_slice(
        &request_source,
        "impl ForwarderRequestSource for CcSwitchForwarderRequestSource",
        "pub(crate) fn forwarder_rectifier_error_message",
    );
    let request_private_impl_slice = function_slice(
        &request_source,
        "impl CcSwitchForwarderRequestSource",
        "fn apply_forwarder_media_prevention_with_log",
    );

    assert!(
        struct_slice.contains("request_source"),
        "RequestForwarder must receive upstream request assembly as an injected source"
    );
    assert!(
        adapter_source.contains("body_model_label"),
        "ForwarderPreparedRequest must include finalized body model facts for logging"
    );
    assert!(
        adapter_source.contains("outbound_model"),
        "ForwarderPreparedRequest must include finalized outbound model attribution"
    );
    let request_preparation_input_slice = function_slice(
        &adapter_source,
        "pub(crate) struct ForwarderRequestPreparationInput",
        "pub(crate) struct ForwarderPreparedRequest",
    );
    assert!(
        request_preparation_input_slice.contains("transform_plan: &'a ForwarderTransformPlan"),
        "ForwarderRequestPreparationInput must carry the cohesive transform plan"
    );
    assert!(
        !request_preparation_input_slice.contains("needs_transform: bool")
            && !request_preparation_input_slice.contains("codex_responses_to_chat: bool"),
        "ForwarderRequestPreparationInput must not expose split transport-policy transform facts"
    );
    assert!(
        source.contains("log_upstream_request"),
        "ForwarderRequestSource must own upstream request logging"
    );
    assert!(
        source.contains("transform_request_body"),
        "ForwarderRequestSource must own transformed request body selection"
    );
    let has_transform_plan_alias = adapter_source.contains(
        "pub(crate) type ForwarderTransformPlan = crate::proxy_core::api::transport::ForwarderTransformPlan;",
    ) || adapter_source.contains(
        "type ForwarderTransformPlan =\n    crate::proxy_core::api::transport::ForwarderTransformPlan",
    );
    assert!(
        has_transform_plan_alias,
        "ForwarderTransformPlan DTO must be owned by proxy-core and exposed through an adapter alias"
    );
    let transform_plan_slice = function_slice(
        &core_transport_source,
        "pub struct ForwarderTransformPlan",
        "pub struct ForwarderTransformPlanFacts",
    );
    assert!(
        transform_plan_slice.contains("codex_responses_to_chat: bool"),
        "ForwarderTransformPlan must carry the Codex Responses to Chat gate fact"
    );
    let upstream_url_input_slice = function_slice(
        &adapter_source,
        "pub(crate) struct ForwarderUpstreamUrlInput",
        "pub(crate) struct ForwarderMediaPreventionInput",
    );
    assert!(
        upstream_url_input_slice.contains("transform_plan: &'a ForwarderTransformPlan"),
        "ForwarderUpstreamUrlInput must carry the cohesive transform plan"
    );
    assert!(
        !upstream_url_input_slice.contains("codex_responses_to_chat: bool")
            && !upstream_url_input_slice.contains("use_claude_transform: bool")
            && !upstream_url_input_slice.contains("claude_api_format: Option"),
        "ForwarderUpstreamUrlInput must not expose split transform-plan URL facts"
    );
    assert!(
        adapter_source.contains(
            "type ForwarderProtocolPreparationInput<'a> =\n    crate::proxy_core::api::transport::ForwarderProtocolPreparationInput<'a>"
        ) && adapter_source.contains(
            "type ForwarderProtocolPreparation =\n    crate::proxy_core::api::transport::ForwarderProtocolPreparation"
        ),
        "ForwarderProtocolPreparation DTOs must be owned by proxy-core and exposed through adapter aliases"
    );
    let protocol_preparation_input_slice = function_slice(
        &core_transport_source,
        "pub struct ForwarderProtocolPreparationInput",
        "pub enum ForwarderRequestBodyTransformAction",
    );
    assert!(
        protocol_preparation_input_slice.contains("transform_plan: &'a ForwarderTransformPlan"),
        "ForwarderProtocolPreparationInput must carry the cohesive transform plan"
    );
    let protocol_preparation_slice = function_slice(
        &core_transport_source,
        "pub struct ForwarderProtocolPreparation",
        "pub fn forwarder_transform_plan_from_facts",
    );
    assert!(
        protocol_preparation_slice.contains("should_transform_claude_request: bool")
            && protocol_preparation_slice.contains("codex_chat_enrichment_enabled: bool"),
        "ForwarderProtocolPreparation must expose protocol-preparation decisions"
    );
    let adapter_context_inputs = [
        (
            "ForwarderUpstreamRequestLogInput",
            function_slice(
                &adapter_source,
                "pub(crate) struct ForwarderUpstreamRequestLogInput",
                "pub(crate) struct ForwarderCopilotRequestOptimizationInput",
            ),
        ),
        (
            "ForwarderRequestPartsInput",
            function_slice(
                &adapter_source,
                "pub(crate) struct ForwarderRequestPartsInput",
                "pub(crate) struct ForwarderUpstreamRequestParts",
            ),
        ),
    ];
    for (input_name, input_slice) in adapter_context_inputs {
        assert!(
            input_slice.contains("adapter: &'a ForwarderAdapterContext")
                && !input_slice.contains("adapter_facts: &'a ForwarderAdapterFacts"),
            "{input_name} must use ForwarderAdapterContext instead of detached adapter facts"
        );
        assert!(
            !input_slice.contains("adapter_name: &'a str")
                && !input_slice.contains("is_claude_adapter: bool"),
            "{input_name} must not expose split adapter name or Claude-adapter facts"
        );
    }
    let transform_plan_input_slice = function_slice(
        &adapter_source,
        "pub(crate) struct ForwarderTransformPlanInput",
        "pub(crate) struct ForwarderUpstreamUrlInput",
    );
    assert!(
        transform_plan_input_slice.contains("adapter: &'a ForwarderAdapterContext")
            && !transform_plan_input_slice.contains("adapter_facts: &'a ForwarderAdapterFacts"),
        "ForwarderTransformPlanInput must use ForwarderAdapterContext instead of detached adapter facts"
    );
    let claude_api_format_input_slice = function_slice(
        &adapter_source,
        "pub(crate) struct ForwarderClaudeApiFormatInput",
        "pub(crate) struct ForwarderCopilotRequestOptimization",
    );
    assert!(
        claude_api_format_input_slice.contains("adapter: &'a ForwarderAdapterContext")
            && !claude_api_format_input_slice.contains("adapter_facts: &'a ForwarderAdapterFacts"),
        "ForwarderClaudeApiFormatInput must use ForwarderAdapterContext instead of detached adapter facts"
    );
    let media_retry_plan_input_slice = function_slice(
        &adapter_source,
        "pub(crate) struct ForwarderMediaRetryPlanInput",
        "pub(crate) struct ForwarderThinkingSignatureRectifierInput",
    );
    assert!(
        media_retry_plan_input_slice.contains("adapter: &'a ForwarderAdapterContext")
            && !media_retry_plan_input_slice
                .contains("adapter_facts: &'a ForwarderAdapterFacts"),
        "ForwarderMediaRetryPlanInput must use ForwarderAdapterContext instead of detached adapter facts"
    );
    let prepared_request_inputs = [
        (
            "ForwarderUpstreamRequestLogInput",
            function_slice(
                &adapter_source,
                "pub(crate) struct ForwarderUpstreamRequestLogInput",
                "pub(crate) struct ForwarderCopilotRequestOptimizationInput",
            ),
        ),
        (
            "ForwarderRequestPartsInput",
            function_slice(
                &adapter_source,
                "pub(crate) struct ForwarderRequestPartsInput",
                "pub(crate) struct ForwarderUpstreamRequestParts",
            ),
        ),
    ];
    for (input_name, input_slice) in prepared_request_inputs {
        assert!(
            input_slice.contains("prepared_request: &'a ForwarderPreparedRequest"),
            "{input_name} must carry the cohesive prepared request"
        );
        assert!(
            !input_slice.contains("filtered_body: &'a Value")
                && !input_slice.contains("body_model_label: &'a str")
                && !input_slice.contains("force_identity_encoding: bool"),
            "{input_name} must not expose split prepared request facts"
        );
    }
    let rectifier_config_inputs = [
        (
            "ForwarderClaudeBodyPolicyInput",
            function_slice(
                &adapter_source,
                "pub(crate) struct ForwarderClaudeBodyPolicyInput",
                "pub(crate) struct ForwarderCodexResponsesToChatInput",
            ),
        ),
        (
            "ForwarderMediaPreventionInput",
            function_slice(
                &adapter_source,
                "pub(crate) struct ForwarderMediaPreventionInput",
                "pub(crate) struct ForwarderAppMediaPreventionInput",
            ),
        ),
        (
            "ForwarderAppMediaPreventionInput",
            function_slice(
                &adapter_source,
                "pub(crate) struct ForwarderAppMediaPreventionInput",
                "pub(crate) struct ForwarderMediaRetryPlanInput",
            ),
        ),
        (
            "ForwarderMediaRetryPlanInput",
            function_slice(
                &adapter_source,
                "pub(crate) struct ForwarderMediaRetryPlanInput",
                "pub(crate) struct ForwarderThinkingSignatureRectifierInput",
            ),
        ),
    ];
    for (input_name, input_slice) in rectifier_config_inputs {
        assert!(
            input_slice.contains("config: &'a RectifierConfig"),
            "{input_name} must carry the cohesive rectifier config"
        );
        assert!(
            !input_slice.contains("rectifier_enabled: bool")
                && !input_slice.contains("request_media_fallback: bool")
                && !input_slice.contains("request_media_heuristic: bool"),
            "{input_name} must not expose split rectifier/media switch facts"
        );
    }
    assert!(
        !impl_slice.contains("let adapter_name = adapter_facts.adapter_name")
            && !impl_slice.contains("let is_claude_adapter = adapter_facts.is_claude_adapter"),
        "RequestForwarder must not split adapter facts into scalar locals for request assembly"
    );
    let forbidden_rectifier_config_access = [
        "rectifier_enabled: self.rectifier_config.enabled",
        "request_media_fallback: self.rectifier_config.request_media_fallback",
        "request_media_heuristic: self.rectifier_config.request_media_heuristic",
    ];
    for marker in forbidden_rectifier_config_access {
        assert!(
            !impl_slice.contains(marker),
            "RequestForwarder must pass RectifierConfig cohesively instead of split marker `{marker}`"
        );
    }
    let forbidden_prepared_request_access = [
        "let filtered_body = prepared_request.body",
        "let request_model = prepared_request.body_model_label",
        "let force_identity_encoding = prepared_request.force_identity_encoding",
    ];
    for marker in forbidden_prepared_request_access {
        assert!(
            !impl_slice.contains(marker),
            "RequestForwarder must pass ForwarderPreparedRequest cohesively instead of split marker `{marker}`"
        );
    }
    let forbidden_protocol_plan_access = [
        "let codex_responses_to_chat = transform_plan.codex_responses_to_chat",
        "transform_plan.codex_responses_to_chat",
        "transform_plan.use_claude_transform",
        "transform_plan.claude_api_format_for_transform",
    ];
    for marker in forbidden_protocol_plan_access {
        assert!(
            !impl_slice.contains(marker),
            "RequestForwarder must use ForwarderProtocolPreparation instead of direct protocol transform-plan marker `{marker}`"
        );
    }
    assert!(
        request_source.contains("forwarder_request_body_model("),
        "default ForwarderRequestSource implementation must retain request body model projection"
    );
    assert!(
        request_source.contains("fn transform_provider_request_body"),
        "default ForwarderRequestSource implementation must retain provider transform wrapping"
    );
    assert!(
        request_source.contains("fn convert_codex_responses_to_chat_body"),
        "default ForwarderRequestSource implementation must retain Codex Responses to Chat body conversion"
    );
    assert!(
        request_source.contains("fn optimize_copilot_request"),
        "default ForwarderRequestSource implementation must retain Copilot optimizer sequencing"
    );
    assert!(
        request_source.contains("use crate::proxy_core::api::transport::{")
            && request_source.contains("classify_copilot_request")
            && request_source.contains("sanitize_copilot_orphan_tool_results")
            && request_source.contains("merge_copilot_tool_results")
            && request_source.contains("strip_copilot_thinking_blocks")
            && request_source.contains("apply_copilot_warmup_model_override"),
        "default ForwarderRequestSource should import pure Copilot optimizer helpers directly from proxy_core::api::transport"
    );
    for marker in [
        "classify_copilot_request",
        "sanitize_copilot_orphan_tool_results",
        "merge_copilot_tool_results",
        "strip_copilot_thinking_blocks",
        "apply_copilot_warmup_model_override",
    ] {
        assert!(
            !adapter_source.contains(marker),
            "proxy_core_adapter should not re-export pure Copilot optimizer helper `{marker}` once request source owns the call site"
        );
    }
    let request_transport_import_slice = function_slice(
        &request_source,
        "use crate::proxy_core::api::transport::{",
        "};\nuse crate::proxy_core_adapter::*;",
    );
    for marker in [
        "anthropic_beta_header_value",
        "build_upstream_request_headers",
        "forward_upstream_url_plan",
        "forwarder_media_retry_plan_from_facts",
        "forwarder_protocol_preparation_from_transform_plan",
        "forwarder_rectifier_error_message as core_forwarder_rectifier_error_message",
        "forwarder_request_body_model",
        "forwarder_request_body_transform_action_from_plan",
        "forwarder_transform_plan_from_facts",
        "is_openai_o_series",
        "is_unsupported_image_error",
        "prepare_upstream_request_body_with_report",
        "prompt_cache_trace_log_message",
        "request_body_filter_log_message",
        "request_body_serialize_error_message",
        "resolve_upstream_request_transport_policy",
        "serialize_upstream_request_body",
        "should_preserve_exact_request_header_case",
        "should_send_anthropic_request_headers",
        "supports_reasoning_effort",
        "upstream_host_header_from_url",
        "ForwardUpstreamUrlPlanInput",
        "ForwarderMediaRetryPlanFacts",
        "ForwarderRectifierErrorInput",
        "ForwarderRequestBodyTransformAction",
        "ForwarderTransformPlanFacts",
        "PromptCacheTraceLogInput",
        "UpstreamRequestHeadersInput",
        "UNSUPPORTED_IMAGE_MARKER",
    ] {
        assert!(
            request_transport_import_slice.contains(marker),
            "default ForwarderRequestSource should import pure request helper/type `{marker}` directly from proxy_core::api::transport"
        );
    }
    assert!(
        request_source.contains(
            "use crate::proxy_core::api::transforms::responses_to_chat_completions_with_options;"
        ),
        "default ForwarderRequestSource should import Codex Responses-to-Chat conversion directly from proxy_core::api::transforms"
    );
    let adapter_runtime_lines: Vec<&str> = adapter_runtime_source.lines().collect();
    for marker in [
        "type ForwarderTransformPlanFacts",
        "type ForwarderMediaRetryPlanFacts",
        "type PromptCacheTraceLogInput",
        "type UpstreamRequestHeadersInput",
        "responses_to_chat_completions_with_options",
        "forward_upstream_url_plan",
        "forwarder_media_retry_plan_from_facts",
        "forwarder_protocol_preparation_from_transform_plan",
        "core_forwarder_rectifier_error_message",
        "forwarder_request_body_model",
        "forwarder_request_body_transform_action_from_plan",
        "forwarder_transform_plan_from_facts",
        "is_openai_o_series",
        "is_unsupported_image_error",
        "prepare_upstream_request_body_with_report",
        "prompt_cache_trace_log_message",
        "request_body_filter_log_message",
        "request_body_serialize_error_message",
        "resolve_upstream_request_transport_policy",
        "should_preserve_exact_request_header_case",
        "should_send_anthropic_request_headers",
        "supports_reasoning_effort",
        "ForwardUpstreamUrlPlanInput",
        "ForwarderRectifierErrorInput",
        "ForwarderRequestBodyTransformAction",
        "UNSUPPORTED_IMAGE_MARKER",
        "anthropic_beta_header_value",
        "build_upstream_request_headers",
        "serialize_upstream_request_body",
        "upstream_host_header_from_url",
    ] {
        let has_production_marker =
            adapter_runtime_lines
                .iter()
                .enumerate()
                .any(|(line_index, line)| {
                    let previous = line_index
                        .checked_sub(1)
                        .and_then(|previous_index| adapter_runtime_lines.get(previous_index))
                        .map(|line| line.trim())
                        .unwrap_or("");
                    let code = line.split("//").next().unwrap_or_default();
                    previous != "#[cfg(test)]" && code.contains(marker)
                });
        assert!(
            !has_production_marker,
            "proxy_core_adapter should not re-export pure request helper/type `{marker}` once request source owns the call site"
        );
    }
    assert!(
        !request_source.contains("fn apply_media_prevention"),
        "default ForwarderRequestSource implementation must not retain a private media prevention replacement method"
    );
    assert!(
        request_source.contains("fn apply_forwarder_media_prevention_with_log")
            && request_impl_slice.contains("apply_forwarder_media_prevention_with_log("),
        "default ForwarderRequestSource implementation should delegate media prevention to the host helper"
    );
    assert!(
        impl_slice.contains("adapter.provider_url_facts(provider)")
            && !request_impl_slice.contains("provider_url_facts(")
            && !request_impl_slice.contains("forwarder_provider_base_url("),
        "RequestForwarder must read provider URL facts from ForwarderAdapterContext instead of exposing them on ForwarderRequestSource"
    );
    assert!(
        request_private_impl_slice.contains(".adapter")
            && request_private_impl_slice
                .contains(".transform_provider_request(input.body, input.provider)")
            && !request_private_impl_slice.contains("forwarder_provider_transform_request(")
            && !request_private_impl_slice.contains("forwarder_provider_transform_required(")
            && request_impl_slice.contains("input.adapter.provider_transform_required(input.provider)")
            && !request_impl_slice.contains("forwarder_provider_transform_required("),
        "default ForwarderRequestSource implementation must use ForwarderAdapterContext for provider transform facts/actions"
    );
    assert!(
        request_impl_slice.contains(".provider_upstream_url(base_url, effective_endpoint)")
            && !request_impl_slice.contains("forwarder_provider_upstream_url(")
            && !request_impl_slice.contains("input.adapter.adapter()"),
        "default ForwarderRequestSource implementation must use ForwarderAdapterContext for upstream URL assembly"
    );
    let request_trait_slice = function_slice(
        &adapter_source,
        "pub(crate) trait ForwarderRequestSource",
        "pub(crate) use crate::proxy::host::cc_switch::forwarder_request_source",
    );
    assert!(
        !request_trait_slice.contains("request_body_model")
            && !request_trait_slice.contains("transform_provider_request_body")
            && !request_trait_slice.contains("convert_codex_responses_to_chat_body")
            && !request_trait_slice.contains("codex_responses_to_chat_enabled")
            && !request_trait_slice.contains("optimize_copilot_request")
            && !request_trait_slice.contains("apply_media_prevention")
            && !request_trait_slice.contains("adapter_facts")
            && !request_trait_slice.contains("provider_url_facts"),
        "ForwarderRequestSource trait must not expose internal request body model, provider transform, Codex bridge body/gate, Copilot optimizer, media prevention, adapter facts, or provider URL facts helpers"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::host::cc_switch::forwarder_request_source::forwarder_request_source_from_managed_account_runtime_source")
            && !adapter_source.contains("struct CcSwitchForwarderRequestSource"),
        "proxy_core_adapter should re-export, not own, the default forwarder request source"
    );

    let impl_forbidden_markers = [
        "prepare_upstream_request_body_with_report(",
        "request_body_filter_log_message(",
        "prompt_cache_trace_log_message(",
        "PromptCacheTraceLogInput",
        "resolve_upstream_request_transport_policy(",
        "upstream_host_header_from_url(",
        "anthropic_beta_header_value(",
        "build_upstream_request_headers(",
        "serialize_upstream_request_body(",
        "request_body_serialize_error_message(",
        "validate_managed_account_upstream_auth(",
        "UpstreamRequestHeadersInput",
        "forwarder_custom_user_agent_header(",
        "should_preserve_exact_request_header_case(",
        "forwarder_is_codex_oauth_provider(",
        "classify_copilot_request(",
        "sanitize_copilot_orphan_tool_results(",
        "merge_copilot_tool_results(",
        "strip_copilot_thinking_blocks(",
        "apply_copilot_warmup_model_override(",
        "resolve_media_prevention_policy(",
        "forwarder_replace_images_for_text_only_provider_model(",
        "should_check_media_retry(",
        "should_trigger_media_retry(",
        "contains_image_blocks(",
        "is_unsupported_image_error(",
        "replace_image_blocks_with_marker(",
        "MediaRetryInput",
        "should_rectify_thinking_signature(",
        "rectify_anthropic_request(",
        "thinking_signature_core_config(",
        "should_rectify_thinking_budget(",
        "rectify_thinking_budget(",
        "thinking_budget_core_config(",
        "apply_forward_request_model_mapping_from_provider(",
        "normalize_thinking_type(",
        "apply_channel_model_override(",
        "apply_copilot_model_normalization(",
        "strip_one_m_suffix_for_upstream(",
        "strip_one_m_suffix_for_upstream_from_body(",
        "optimize_copilot_request(",
        ".copilot_optimizer_config.enabled",
        "forwarder_claude_normalize_anthropic_messages(",
        "provider_claude_normalize_anthropic_messages(",
        "should_apply_bedrock_pre_send_optimizer(",
        "forwarder_bedrock_env_flag(",
        "apply_bedrock_pre_send_optimizers(",
        "thinking_optimization_log_message(",
        "cache_injection_log_message(",
        "forwarder_apply_codex_chat_upstream_model(",
        "provider_apply_codex_chat_upstream_model(",
        "forwarder_codex_chat_reasoning_options(",
        "provider_codex_chat_reasoning_options(",
        "responses_to_chat_completions_with_options(",
        "is_openai_o_series(",
        "supports_reasoning_effort(",
        "forwarder_provider_transform_request(",
        "convert_codex_responses_to_chat_body(",
        "transform_provider_request_body(",
        "forwarder_should_convert_codex_responses_to_chat(",
        "forward_upstream_url_plan(",
        "ForwardUpstreamUrlPlanInput",
        "forwarder_provider_upstream_url(",
        "forwarder_provider_base_url(",
        "forwarder_is_full_url_provider(",
        "forwarder_is_github_copilot_upstream(",
        "forwarder_provider_adapter_name(",
        "provider_adapter_name_is_claude(",
        "forwarder_claude_transform_required(",
        "forwarder_provider_transform_required(",
        "forwarder_claude_api_format(",
        "forwarder_uses_anthropic_rectifiers(",
        "forwarder_provider_adapter_for_app(",
        ".get(\"model\")",
        "request_body_model(&mapped_body)",
        "request_body_model(&filtered_body)",
        "if let Some(model) = prepared_request",
        "outbound_model = Some(",
        ">>> 请求 URL",
        "请求体内容",
        "serde_json::to_string(&filtered_body)",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in impl_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct request assembly marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must use an injected request source for upstream body/header assembly:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_forward_model_mapping_uses_adapter_claude_desktop_projection() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let mapping_slice = function_slice(
        &source,
        "pub(crate) fn apply_forward_request_model_mapping_from_provider(",
        "#[cfg(test)]",
    );

    assert!(
        mapping_slice.contains("provider_claude_desktop_proxy_request_body(")
            && mapping_slice.contains("ModelMappingProjection"),
        "proxy_core_adapter should map Claude Desktop forward request bodies through adapter projection"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config::map_proxy_request_model(",
        "map_proxy_request_model(body, provider)",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(mapping_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs apply_forward_request_model_mapping_from_provider:{} contains host request body projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must not map Claude Desktop forward request bodies through host config:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_uses_response_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let response_source_path =
        manifest_dir.join("src/proxy/host/cc_switch/forwarder_response_source.rs");
    let response_source =
        fs::read_to_string(&response_source_path).expect("read forwarder_response_source.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    assert!(
        struct_slice.contains("response_source"),
        "RequestForwarder must receive upstream response readiness/body reads as an injected source"
    );
    assert!(
        adapter_source.contains("pub(crate) struct ForwarderChannelResponseStatusInput"),
        "ForwarderResponseSource must receive channel status mapping facts through an input DTO"
    );
    assert!(
        adapter_source.contains("channel: Option<&'a ResolvedChannelAttempt>"),
        "ForwarderChannelResponseStatusInput must carry the selected channel facts"
    );
    assert!(
        impl_slice.contains("apply_channel_response_status_mapping(")
            && impl_slice.contains("ForwarderChannelResponseStatusInput {"),
        "RequestForwarder must pass channel response status facts as a response-source input DTO"
    );
    let response_source_impl_slice = function_slice(
        &response_source,
        "impl ForwarderResponseSource for CcSwitchForwarderResponseSource",
        "async fn prime_streaming_forward_response",
    );
    assert!(
        !response_source.contains("fn upstream_error_body")
            && !response_source.contains("fn upstream_error_response"),
        "default ForwarderResponseSource implementation must not retain private upstream error projection helpers"
    );
    assert!(
        response_source.contains("fn prepare_success_response"),
        "default ForwarderResponseSource implementation must retain success response readiness projection"
    );
    assert!(
        response_source_impl_slice.contains("input.response.bytes().await?")
            && response_source_impl_slice.contains("ProxyError::UpstreamError { status, body }"),
        "default ForwarderResponseSource should project upstream error responses inside finalize_upstream_response"
    );
    assert!(
        adapter_source.contains("pub(crate) use crate::proxy::host::cc_switch::forwarder_response_source::default_forwarder_response_source")
            && !adapter_source.contains("struct CcSwitchForwarderResponseSource;"),
        "proxy_core_adapter should re-export, not own, the default forwarder response source"
    );
    let response_trait_slice = function_slice(
        &adapter_source,
        "pub(crate) trait ForwarderResponseSource",
        "pub(crate) use crate::proxy::host::cc_switch::forwarder_response_source",
    );
    assert!(
        !response_trait_slice.contains("upstream_error_body")
            && !response_trait_slice.contains("upstream_error_response")
            && !response_trait_slice.contains("prepare_success_response"),
        "ForwarderResponseSource trait must not expose internal response readiness or upstream error projection helpers"
    );
    assert!(
        adapter_source.contains("finalize_upstream_response"),
        "ForwarderResponseSource must expose upstream response success/error finalization"
    );
    assert!(
        adapter_source.contains("pub(crate) struct ForwarderResponseFinalizationInput"),
        "ForwarderResponseSource must receive response finalization facts through an input DTO"
    );
    let response_core_transport_import_slice = function_slice(
        &response_source,
        "use crate::proxy_core::api::transport::{",
        "};\nuse crate::proxy_core_adapter::{",
    );
    for marker in [
        "non_streaming_body_timeout_message",
        "resolve_channel_response_status_mapping",
        "streaming_body_ended_before_first_chunk_message",
        "streaming_body_first_chunk_read_error_message",
        "streaming_body_first_chunk_timeout_message",
    ] {
        assert!(
            response_core_transport_import_slice.contains(marker),
            "default ForwarderResponseSource should import pure response helper `{marker}` directly from proxy_core::api::transport"
        );
        assert!(
            !adapter_runtime_source.contains(marker),
            "proxy_core_adapter should not re-export pure response helper `{marker}` once response source owns the call site"
        );
    }
    assert!(
        adapter_source.contains("response: ProxyResponse")
            && adapter_source.contains("request_is_streaming: bool")
            && adapter_source.contains("non_streaming_timeout: std::time::Duration")
            && adapter_source.contains("streaming_first_byte_timeout: std::time::Duration"),
        "ForwarderResponseFinalizationInput must carry response, streaming mode, and timeout facts"
    );
    assert!(
        impl_slice.contains("finalize_upstream_response(ForwarderResponseFinalizationInput {"),
        "RequestForwarder must pass response finalization facts as a response-source input DTO"
    );

    let impl_forbidden_markers = [
        "response.bytes().await",
        "response.bytes_stream()",
        "tokio::time::timeout(",
        "ProxyResponse::buffered(",
        "ProxyResponse::streamed(",
        "futures::stream::once(",
        "non_streaming_body_timeout_message(",
        "streaming_body_first_chunk_timeout_message(",
        "streaming_body_ended_before_first_chunk_message(",
        "streaming_body_first_chunk_read_error_message(",
        "resolve_channel_response_status_mapping(",
        "status.as_u16()",
        "status.is_success()",
        "ProxyError::UpstreamError {",
        "upstream_error_body(response)",
        "upstream_error_response(response)",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in impl_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs impl RequestForwarder:{} contains direct response-readiness marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "forwarder must use an injected response source for body reads and streaming priming:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_runtime_events_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_RUNTIME_EVENT_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains runtime event marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production forwarder must delegate runtime status, active-target, and event side effects to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_delegates_attempt_runtime_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_ATTEMPT_RUNTIME_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs:{} contains attempt runtime marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production forwarder must delegate circuit allow, health result, and neutral permit side effects to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_projects_errors_through_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_ERROR_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains direct core error marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must project core errors through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_projects_usage_warnings_through_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_USAGE_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains usage projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must emit usage projection warnings through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_projects_app_summary_through_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_APP_SUMMARY_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains app summary projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must build app summary config through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_config_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CONFIG_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains config source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate config source DB/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_config_source_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/config_source.rs");
    let source = fs::read_to_string(&source_path).expect("read config_source.rs");
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    let source_adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse futures::future::BoxFuture;",
    );

    assert!(
        source.contains("pub(crate) struct CcSwitchConfigSource")
            && source.contains("impl ProxyConfigSource for CcSwitchConfigSource")
            && source.contains("cc_switch_app_kinds()")
            && source.contains("proxy_global_config_from_db_source(&self.db).await")
            && source.contains("proxy_app_config_from_db_source(&self.db, app).await")
            && source.contains("app_summary_config_from_db_source(&self.db, app).await")
            && source.contains("proxy_runtime_config_from_db_source(&self.db).await"),
        "CC Switch config source should live in host/cc_switch/config_source.rs"
    );
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::config_source::CcSwitchConfigSource"
        ) && !adapter_source.contains("pub(crate) struct CcSwitchConfigSource")
            && !adapter_source.contains("impl ProxyConfigSource for CcSwitchConfigSource"),
        "proxy_core_adapter should not own or re-export the CC Switch config source"
    );
    assert!(
        source.contains(
            "use crate::proxy_core::api::config::{ProxyAppConfig, ProxyGlobalConfig, ProxyRuntimeConfig};"
        ) && source.contains("use crate::proxy_core::api::domain::AppKind;")
            && source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && source
                .contains("use crate::proxy_core::api::ports::{AppSummaryConfig, ProxyConfigSource};"),
        "CC Switch config source should import core config/source contracts directly"
    );
    for adapter_type in [
        "AppSummaryConfig",
        "ProxyAppConfig",
        "ProxyConfigSource",
        "ProxyCoreAppKind",
        "ProxyCoreResult",
        "ProxyGlobalConfig",
        "ProxyRuntimeConfig",
    ] {
        assert!(
            !source_adapter_import.contains(adapter_type),
            "config source should not import {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        !adapter_core_ports_import.contains("AppSummaryConfig")
            && !adapter_core_ports_import.contains("ProxyConfigSource"),
        "proxy_core_adapter should not re-export config source port contracts"
    );
}

#[test]
fn production_proxy_core_host_delegates_provider_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_PROVIDER_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains provider source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate provider source DB/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_provider_source_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/provider_source.rs");
    let source = fs::read_to_string(&source_path).expect("read provider_source.rs");
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    let source_adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse futures::future::BoxFuture;",
    );

    assert!(
        source.contains("pub(crate) struct CcSwitchProviderSource")
            && source.contains("impl ProviderSource for CcSwitchProviderSource")
            && source.contains("provider_specs_from_db_source(&self.db, app)")
            && source.contains("current_provider_id_from_db_source(&self.db, app)")
            && source.contains(
                "active_route_target_from_runtime_source(&self.current_providers, app).await"
            )
            && source.contains(
                "route_candidate_provider_ids_from_router_source(&self.router, app).await"
            ),
        "CC Switch provider source should live in host/cc_switch/provider_source.rs"
    );
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::provider_source::CcSwitchProviderSource"
        ) && !adapter_source.contains("pub(crate) struct CcSwitchProviderSource")
            && !adapter_source.contains("impl ProviderSource for CcSwitchProviderSource"),
        "proxy_core_adapter should not re-export or own the CC Switch provider source"
    );
    assert!(
        source.contains("use crate::proxy_core::api::domain::{AppKind, ProviderSpec};")
            && source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && source.contains(
                "use crate::proxy_core::api::ports::{CurrentRouteTarget, ProviderSource};"
            ),
        "CC Switch provider source should import core provider/source contracts directly"
    );
    for adapter_type in [
        "CurrentRouteTarget",
        "ProviderSource",
        "ProviderSpec",
        "ProxyCoreAppKind",
        "ProxyCoreResult",
    ] {
        assert!(
            !source_adapter_import.contains(adapter_type),
            "provider source should not import {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        !adapter_core_ports_import.contains("ProviderSource"),
        "proxy_core_adapter should not re-export the ProviderSource port trait"
    );
}

#[test]
fn production_proxy_core_host_delegates_channel_spec_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_SPEC_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains channel spec source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate channel spec source DB/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_channel_record_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_RECORD_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains channel record source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate channel record DB/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_channel_key_model_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_KEY_MODEL_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains channel key/model source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate channel key/model DB/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_channel_record_list_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_RECORD_LIST_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains channel record list source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate channel record list DB/router projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_channel_migration_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_MIGRATION_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains channel migration source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate channel migration DB/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_route_policy_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_ROUTE_POLICY_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains route policy source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate route policy DB/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_route_policy_source_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/route_policy_source.rs");
    let source = fs::read_to_string(&source_path).expect("read route_policy_source.rs");
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    let source_adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::route_policy_from_db_source;",
        "use futures::future::BoxFuture;",
    );

    assert!(
        source.contains("pub(crate) struct CcSwitchRoutePolicySource")
            && source.contains("impl RoutePolicySource for CcSwitchRoutePolicySource")
            && source.contains("route_policy_from_db_source(&self.db, app)"),
        "CC Switch route policy source should live in host/cc_switch/route_policy_source.rs"
    );
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::route_policy_source::CcSwitchRoutePolicySource"
        ) && !adapter_source.contains("pub(crate) struct CcSwitchRoutePolicySource")
            && !adapter_source.contains("impl RoutePolicySource for CcSwitchRoutePolicySource"),
        "proxy_core_adapter should not own or re-export the CC Switch route policy source"
    );
    assert!(
        source.contains("use crate::proxy_core::api::domain::AppKind;")
            && source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && source.contains("use crate::proxy_core::api::ports::RoutePolicySource;")
            && source.contains("use crate::proxy_core::api::routing::RoutePolicy;"),
        "CC Switch route policy source should import core route-policy contracts directly"
    );
    assert!(
        !source_adapter_import.contains("ProxyCoreAppKind")
            && !source_adapter_import.contains("ProxyCoreResult")
            && !source_adapter_import.contains("RoutePolicy")
            && !source_adapter_import.contains("RoutePolicySource"),
        "route policy source should not import route-policy contracts through proxy_core_adapter"
    );
    assert!(
        !adapter_core_ports_import.contains("RoutePolicySource"),
        "proxy_core_adapter should not re-export the RoutePolicySource port trait"
    );
}

#[test]
fn production_proxy_core_host_delegates_route_resolver_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_ROUTE_RESOLVER_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains route resolver source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate route resolver router/error wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_route_resolver_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/route_resolver.rs");
    let source = fs::read_to_string(&source_path).expect("read route_resolver.rs");
    let host_path = manifest_dir.join("src/proxy_core_host.rs");
    let host_source = fs::read_to_string(&host_path).expect("read proxy_core_host.rs");

    assert!(
        source.contains("pub(crate) struct CcSwitchRouteResolver")
            && source.contains("impl RouteResolver for CcSwitchRouteResolver")
            && source.contains("build_route_plan(request)")
            && source
                .contains("management_route_response_from_router_source(&self.router, request)"),
        "CC Switch route resolver should live in host/cc_switch/route_resolver.rs"
    );
    assert!(
        source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && source.contains(
                "use crate::proxy_core::api::management::{RouteResolveRequest, RouteResolveResponse};"
            )
            && source.contains("use crate::proxy_core::api::ports::RouteResolver;")
            && source.contains("use crate::proxy_core::api::routing::{build_route_plan, RoutePlan};")
            && source.contains("pub(crate) use crate::proxy_core::api::routing::RouteRequest;"),
        "CC Switch route resolver should import route contracts directly from proxy_core"
    );
    let adapter_import =
        function_slice(&source, "use crate::proxy_core_adapter::", ";\nuse futures");
    for adapter_type in [
        "ProxyCoreResult",
        "RoutePlan",
        "RouteRequest",
        "RouteResolveRequest",
        "RouteResolveResponse",
        "RouteResolver",
        "route_plan_from_request",
    ] {
        assert!(
            !adapter_import.contains(adapter_type),
            "CC Switch route resolver must not import {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::route_resolver::CcSwitchRouteResolver"
        ) && !adapter_source.contains("pub(crate) struct CcSwitchRouteResolver")
            && !adapter_source.contains("impl RouteResolver for CcSwitchRouteResolver"),
        "proxy_core_adapter should not re-export or own the CC Switch route resolver"
    );
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    assert!(
        !adapter_core_ports_import.contains("RouteResolver"),
        "proxy_core_adapter should not re-export RouteResolver"
    );
    let adapter_routing_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::routing::{\n    failover_config_read_error_log_line",
        "};\n#[cfg(test)]\npub(crate) use crate::proxy_core::api::transforms::claude_api_format_from_metadata;",
    );
    assert!(
        !adapter_routing_import.contains("RouteRequest"),
        "proxy_core_adapter should not re-export RouteRequest"
    );
    assert!(
        !adapter_source.contains("route_plan_from_request"),
        "proxy_core_adapter should not re-export route planning through route_plan_from_request"
    );
    let host_adapter_import =
        function_slice(&host_source, "use crate::proxy_core_adapter::{", "};");
    assert!(
        !host_adapter_import.contains("RouteRequest"),
        "proxy_core_host test harness should not import RouteRequest through proxy_core_adapter"
    );
    assert!(
        host_source.contains("use crate::proxy::host::cc_switch::route_resolver::RouteRequest;"),
        "proxy_core_host test harness should import RouteRequest through the host route resolver module"
    );
}

#[test]
fn production_proxy_core_host_delegates_health_store_sources_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_HEALTH_STORE_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains health store source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate health store DB/router/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_channel_health_store_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/channel_health_store.rs");
    let source = fs::read_to_string(&source_path).expect("read channel_health_store.rs");

    assert!(
        source.contains("pub(crate) struct CcSwitchChannelHealthStore")
            && source.contains("impl ChannelHealthStore for CcSwitchChannelHealthStore")
            && source.contains("record_channel_attempt_in_db_source")
            && source.contains("reset_channel_health_with_router_source")
            && source.contains("channel_breaker_stats_with_router_source"),
        "CC Switch channel health store should live in host/cc_switch/channel_health_store.rs"
    );
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::channel_health_store::CcSwitchChannelHealthStore"
        ) && !adapter_source.contains("pub(crate) struct CcSwitchChannelHealthStore")
            && !adapter_source.contains("impl ChannelHealthStore for CcSwitchChannelHealthStore"),
        "proxy_core_adapter should not re-export or own the CC Switch channel health store"
    );
}

#[test]
fn production_proxy_core_host_delegates_reachability_probe_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_REACHABILITY_PROBE_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains reachability probe source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate reachability probe DB/service/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_model_catalog_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_MODEL_CATALOG_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains model catalog source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate model catalog DB/router/projection wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_model_catalog_provider_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/model_catalog_provider.rs");
    let source = fs::read_to_string(&source_path).expect("read model_catalog_provider.rs");

    assert!(
        source.contains("pub(crate) struct CcSwitchModelCatalogProvider")
            && source.contains("impl ModelCatalogProvider for CcSwitchModelCatalogProvider")
            && source.contains("provider_model_catalog_from_db_source(&self.db, app, provider_id)")
            && source.contains("client_model_catalog_from_app_source(app)")
            && source.contains(
                "claude_desktop_model_routes_from_router_source(&self.db, &self.router, app).await"
            ),
        "CC Switch model catalog provider should live in host/cc_switch/model_catalog_provider.rs"
    );
    assert!(
        source.contains("use crate::proxy_core::api::auth::ClaudeDesktopModelRouteInput;")
            && source.contains("use crate::proxy_core::api::domain::AppKind;")
            && source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && source.contains("use crate::proxy_core::api::model_catalog::ModelCatalog;")
            && source.contains("use crate::proxy_core::api::ports::ModelCatalogProvider;"),
        "CC Switch model catalog provider should import model catalog contracts directly from proxy_core"
    );
    let adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse futures::future::BoxFuture;",
    );
    for adapter_type in [
        "ClaudeDesktopModelRouteInput",
        "ModelCatalog",
        "ModelCatalogProvider",
        "ProxyCoreAppKind",
        "ProxyCoreResult",
    ] {
        assert!(
            !adapter_import.contains(adapter_type),
            "CC Switch model catalog provider must not import {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::model_catalog_provider::CcSwitchModelCatalogProvider"
        ) && !adapter_source.contains("pub(crate) struct CcSwitchModelCatalogProvider")
            && !adapter_source
                .contains("impl ModelCatalogProvider for CcSwitchModelCatalogProvider"),
        "proxy_core_adapter should not re-export or own the CC Switch model catalog provider"
    );
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    assert!(
        !adapter_core_ports_import.contains("ModelCatalogProvider"),
        "proxy_core_adapter should not re-export ModelCatalogProvider"
    );
}

#[test]
fn proxy_core_adapter_model_routes_source_uses_adapter_projection() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let source_slice = function_slice(
        &source,
        "pub(crate) async fn claude_desktop_model_routes_from_router_source(",
        "pub(crate) fn codex_live_settings_with_model_catalog(",
    );

    assert!(
        source_slice.contains("provider_claude_desktop_proxy_model_routes(")
            && source_slice.contains("claude_desktop_model_routes_to_core_inputs("),
        "proxy_core_adapter should project Claude Desktop model routes without calling host config"
    );

    let forbidden_markers = [
        "crate::claude_desktop_config::proxy_model_routes(",
        "ResolvedModelRoute",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(source_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs claude_desktop_model_routes_from_router_source:{} contains host route projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must not route Claude Desktop model route source through host config:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_usage_sink_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_USAGE_SINK_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains usage sink source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate usage sink pricing/projection/logging wiring to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_usage_sink_source_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let usage_sink_path = manifest_dir.join("src/proxy/host/cc_switch/database_usage_sink.rs");
    let usage_sink_source =
        fs::read_to_string(&usage_sink_path).expect("read database_usage_sink.rs");
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );

    assert!(
        usage_sink_source.contains("pub(crate) struct CcSwitchUsageSink")
            && usage_sink_source.contains("impl UsageSink for CcSwitchUsageSink")
            && usage_sink_source.contains("record_usage_in_db_source(")
            && usage_sink_source.contains("UsageLogger::new("),
        "CC Switch usage sink implementation should live in host/cc_switch/database_usage_sink.rs"
    );
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::database_usage_sink::CcSwitchUsageSink"
        ) && !adapter_source.contains("pub(crate) struct CcSwitchUsageSink")
            && !adapter_source.contains("impl UsageSink for CcSwitchUsageSink")
            && !adapter_source.contains("pub(crate) async fn record_usage_in_db_source("),
        "proxy_core_adapter should not re-export or own the CC Switch usage sink source"
    );
    assert!(
        usage_sink_source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && usage_sink_source.contains("use crate::proxy_core::api::ports::UsageSink;")
            && usage_sink_source.contains(
                "use crate::proxy_core::api::usage::{CostBreakdown, ModelPricing, TokenUsage, UsageRecord};"
            ),
        "CC Switch usage sink should import core usage sink/result/DTO contracts directly"
    );
    assert!(
        !adapter_core_ports_import.contains("UsageSink"),
        "proxy_core_adapter should not re-export the UsageSink port trait"
    );
    assert!(
        !usage_sink_source.contains("ProxyCoreResult, TokenUsage, UsageRecord, UsageSink"),
        "CC Switch usage sink should not import UsageSink or usage DTO contracts through proxy_core_adapter"
    );
}

#[test]
fn proxy_core_adapter_delegates_management_auth_source_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/management_auth_source.rs");
    let source = fs::read_to_string(&source_path).expect("read management_auth_source.rs");

    assert!(
        source.contains("pub(crate) struct CcSwitchManagementAuthSource")
            && source.contains("impl ManagementAuthSource for CcSwitchManagementAuthSource")
            && source.contains("PROXY_MANAGEMENT_AUTH_TOKEN_ENV")
            && source.contains("ManagementAuthRuntimeConfig::new(")
            && source.contains("config.management_auth_token.clone()"),
        "CC Switch management auth source should live in host/cc_switch/management_auth_source.rs"
    );
    assert!(
        source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && source.contains("use crate::proxy_core::api::ports::{")
            && source.contains("ManagementAuthRuntimeConfig")
            && source.contains("ManagementAuthSource")
            && source.contains("ProxyConfig"),
        "CC Switch management auth source should import management auth contracts directly from proxy_core"
    );
    let adapter_import = function_slice(
        &source,
        "use crate::proxy_core",
        ";\nuse futures::future::BoxFuture;",
    );
    assert!(
        !adapter_import.contains("proxy_core_adapter"),
        "CC Switch management auth source must not import management auth contracts through proxy_core_adapter"
    );
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::management_auth_source::CcSwitchManagementAuthSource"
        ) && !adapter_source.contains("struct CcSwitchManagementAuthSource")
            && !adapter_source.contains("impl ManagementAuthSource for CcSwitchManagementAuthSource")
            && !adapter_source.contains("PROXY_MANAGEMENT_AUTH_TOKEN_ENV"),
        "proxy_core_adapter should not re-export or own the CC Switch management auth source"
    );
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    for adapter_type in ["ManagementAuthRuntimeConfig", "ManagementAuthSource"] {
        assert!(
            !adapter_core_ports_import.contains(adapter_type),
            "proxy_core_adapter should not re-export {adapter_type}"
        );
    }
}

#[test]
fn proxy_core_adapter_delegates_runtime_status_source_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/runtime_status_source.rs");
    let source = fs::read_to_string(&source_path).expect("read runtime_status_source.rs");

    assert!(
        source.contains("pub(crate) struct CcSwitchRuntimeStatusSource")
            && source.contains("impl RuntimeStatusSource for CcSwitchRuntimeStatusSource")
            && source.contains("proxy_runtime_status_from_runtime_sources(")
            && source.contains("apply_proxy_runtime_uptime(")
            && source.contains("apply_proxy_runtime_active_targets("),
        "CC Switch runtime status source should live in host/cc_switch/runtime_status_source.rs"
    );
    assert!(
        source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && source.contains("use crate::proxy_core::api::ports::{")
            && source.contains("CurrentRouteTarget")
            && source.contains("ProxyRuntimeStatus")
            && source.contains("RuntimeStatusSource"),
        "CC Switch runtime status source should import runtime status contracts directly from proxy_core"
    );
    let adapter_import = function_slice(
        &source,
        "use crate::proxy_core",
        ";\nuse futures::future::BoxFuture;",
    );
    assert!(
        !adapter_import.contains("proxy_core_adapter"),
        "CC Switch runtime status source must not import runtime status contracts through proxy_core_adapter"
    );
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::runtime_status_source::CcSwitchRuntimeStatusSource"
        ) && !adapter_source.contains("struct CcSwitchRuntimeStatusSource")
            && !adapter_source.contains("impl RuntimeStatusSource for CcSwitchRuntimeStatusSource")
            && !adapter_source
                .contains("pub(crate) async fn proxy_runtime_status_from_runtime_sources("),
        "proxy_core_adapter should not re-export or own the CC Switch runtime status source"
    );
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    assert!(
        !adapter_core_ports_import.contains("RuntimeStatusSource"),
        "proxy_core_adapter should not re-export RuntimeStatusSource"
    );
    for adapter_helper in [
        "apply_proxy_runtime_active_targets",
        "apply_proxy_runtime_uptime",
    ] {
        assert!(
            !adapter_source.contains(adapter_helper),
            "proxy_core_adapter should not re-export runtime status helper {adapter_helper}"
        );
    }
}

#[test]
fn production_proxy_core_host_delegates_event_sink_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_EVENT_SINK_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains event sink source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate event sink projection and bus dispatch to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_event_sink_source_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let event_sink_path = manifest_dir.join("src/proxy/host/cc_switch/event_sink.rs");
    let event_sink_source = fs::read_to_string(&event_sink_path).expect("read event_sink.rs");

    assert!(
        event_sink_source.contains("pub(crate) struct CcSwitchEventSink")
            && event_sink_source.contains("impl ProxyEventSink for CcSwitchEventSink")
            && event_sink_source.contains("emit_proxy_core_event_bus_source("),
        "CC Switch event sink implementation should live in host/cc_switch/event_sink.rs"
    );
    assert!(
        event_sink_source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && event_sink_source.contains("use crate::proxy_core::api::events::ProxyCoreEvent;")
            && event_sink_source.contains("use crate::proxy_core::api::ports::ProxyEventSink;"),
        "CC Switch event sink should import event contracts directly from proxy_core"
    );
    let adapter_import = function_slice(
        &event_sink_source,
        "use crate::proxy_core_adapter::",
        ";\n\n#[derive(Clone, Default)]",
    );
    for adapter_type in ["ProxyCoreEvent", "ProxyCoreResult", "ProxyEventSink"] {
        assert!(
            !adapter_import.contains(adapter_type),
            "CC Switch event sink must not import {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::event_sink::CcSwitchEventSink"
        ) && !adapter_source.contains("pub(crate) struct CcSwitchEventSink")
            && !adapter_source.contains("impl ProxyEventSink for CcSwitchEventSink"),
        "proxy_core_adapter should not re-export or own the CC Switch event sink source"
    );
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    assert!(
        !adapter_core_ports_import.contains("ProxyEventSink"),
        "proxy_core_adapter should not re-export ProxyEventSink"
    );
}

#[test]
fn production_proxy_core_host_delegates_auth_provider_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_AUTH_PROVIDER_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains auth provider source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate auth provider profile projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_auth_provider_source_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_runtime_source = adapter_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&adapter_source);
    let auth_source_path = manifest_dir.join("src/proxy/host/cc_switch/auth_provider.rs");
    let auth_source = fs::read_to_string(&auth_source_path).expect("read auth_provider.rs");

    assert!(
        auth_source.contains("pub(crate) struct CcSwitchAuthProvider")
            && auth_source.contains("impl AuthProvider for CcSwitchAuthProvider")
            && auth_source.contains("auth_info_from_cc_switch_route_context(")
            && auth_source.contains("cc_switch_provider_config"),
        "CC Switch auth provider implementation should live in host/cc_switch/auth_provider.rs"
    );
    assert!(
        auth_source.contains("use crate::proxy_core::api::ports::{")
            && auth_source.contains("auth_info_from_route_context")
            && auth_source.contains("AuthInfo")
            && auth_source.contains("AuthProvider")
            && auth_source.contains("use crate::proxy_core::api::domain::{")
            && auth_source.contains("AppKind")
            && auth_source.contains("ChannelSpec")
            && auth_source.contains("ProviderSpec")
            && auth_source.contains("ProxyRequest"),
        "CC Switch auth provider source should import core auth provider port contracts directly from proxy_core::api"
    );
    for marker in [
        "auth_info_from_profile_ref",
        "auth_info_from_route_context",
        "settings_config_with_channel_auth_key_for_app",
    ] {
        assert!(
            !adapter_runtime_source.contains(marker),
            "proxy_core_adapter should not re-export pure auth provider helper `{marker}` once auth_provider owns the call site"
        );
    }
    assert!(
        adapter_runtime_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::auth_provider::auth_info_from_cc_switch_route_context;",
        ) && !adapter_runtime_source.contains("CcSwitchAuthProvider")
            && !adapter_runtime_source.contains("provider_with_channel_auth_key")
            && !adapter_source.contains("pub(crate) struct CcSwitchAuthProvider")
            && !adapter_source.contains("impl AuthProvider for CcSwitchAuthProvider")
            && !adapter_source.contains("pub(crate) fn auth_info_from_cc_switch_route_context("),
        "proxy_core_adapter should not expose the host CC Switch auth provider source beyond the test-only auth-info bridge"
    );
}

#[test]
fn production_proxy_services_excludes_test_constructor_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let services_path = manifest_dir.join("src/proxy/host/cc_switch/proxy_services.rs");
    let services_source = fs::read_to_string(&services_path).expect("read proxy_services.rs");
    let pipeline_path = manifest_dir.join("src/proxy/host/cc_switch/forward_pipeline.rs");
    let pipeline_source = fs::read_to_string(&pipeline_path).expect("read forward_pipeline.rs");
    let services_slice = services_source.as_str();
    let lines: Vec<&str> = services_slice.lines().collect();
    let services_adapter_import = if services_source.contains("use crate::proxy_core_adapter::{") {
        function_slice(&services_source, "use crate::proxy_core_adapter::{", "};")
    } else {
        ""
    };

    let mut violations = Vec::new();
    for marker in [
        "pub(crate) fn new(db: Arc<Database>) -> Self {",
        "pub(crate) fn with_event_bus(db: Arc<Database>, events: Arc<ProxyEventBus>) -> Self {",
        "fn with_optional_event_bus(db: Arc<Database>, events: Option<Arc<ProxyEventBus>>) -> Self {",
    ] {
        for (line_index, line) in lines.iter().enumerate() {
            if line.trim() != marker {
                continue;
            }

            let previous = line_index
                .checked_sub(1)
                .and_then(|index| lines.get(index))
                .map(|line| line.trim())
                .unwrap_or_default();
            if previous != "#[cfg(test)]" {
                violations.push(format!(
                    "src/proxy/host/cc_switch/proxy_services.rs CcSwitchProxyServices:{} keeps test service constructor in production: `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    for (line_index, line) in production_lines(services_slice) {
        let code = line.split("//").next().unwrap_or_default();
        if code.contains("#[allow(dead_code)]") {
            violations.push(format!(
                "src/proxy/host/cc_switch/proxy_services.rs service container:{} keeps dead-code allowance",
                line_index + 1
            ));
        }
    }

    if !services_source
        .contains("#[cfg(test)]\n#[derive(Clone, Default)]\nstruct DefaultRuntimeStatusSource;")
    {
        violations.push(
            "src/proxy/host/cc_switch/proxy_services.rs keeps default runtime status source outside test cfg"
                .to_string(),
        );
    }
    if !services_source
        .contains("#[cfg(test)]\nimpl RuntimeStatusSource for DefaultRuntimeStatusSource")
    {
        violations.push(
            "src/proxy/host/cc_switch/proxy_services.rs keeps default runtime status source impl outside test cfg"
                .to_string(),
        );
    }
    if !pipeline_source.contains("    #[cfg(test)]\n    pub(crate) fn without_runtime(") {
        violations.push(
            "src/proxy/host/cc_switch/forward_pipeline.rs keeps no-runtime forward pipeline constructor outside test cfg"
                .to_string(),
        );
    }
    if !services_source.contains("use crate::proxy_core::api::ports::{")
        || !services_source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
        || !services_source
            .contains("use crate::proxy_core::api::ports::{ProxyConfig, ProxyRuntimeStatus};")
        || !services_source.contains("fn config(&self) -> &(dyn ProxyConfigSource + Send + Sync)")
    {
        violations.push(
            "src/proxy/host/cc_switch/proxy_services.rs must import core service ports directly"
                .to_string(),
        );
    }
    for adapter_type in [
        "AuthProvider",
        "ChannelHealthStore",
        "ChannelKeyRuntimeSource",
        "ChannelReachabilityProbe",
        "ChannelSource",
        "ClaudeDesktopGatewayAuthSource",
        "ForwardPipeline",
        "ManagementAuthSource",
        "ModelCatalogProvider",
        "ProviderSource",
        "ProxyConfigSource",
        "ProxyEventSink",
        "ProxyServices",
        "RoutePolicySource",
        "RouteResolver",
        "RuntimeStatusSource",
        "UsageSink",
    ] {
        if services_adapter_import.contains(adapter_type) {
            violations.push(format!(
                "src/proxy/host/cc_switch/proxy_services.rs imports core service port {adapter_type} through proxy_core_adapter"
            ));
        }
    }
    for adapter_import in [
        "use crate::proxy_core_adapter::{ProxyConfig, ProxyCoreResult, ProxyRuntimeStatus};",
        "crate::proxy_core_adapter::ProxyConfigSource",
    ] {
        if services_source.contains(adapter_import) {
            violations.push(format!(
                "src/proxy/host/cc_switch/proxy_services.rs keeps adapter import `{adapter_import}`"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy service container should expose only runtime-backed construction:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_service_container_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_SERVICE_CONTAINER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains service container marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate ProxyServices container assembly to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_auth_profile_db_injection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_AUTH_PROFILE_DB_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains auth-profile DB injection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate auth-profile DB injection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_forward_pipeline_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_PIPELINE_RUNTIME_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains forward pipeline marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate ForwardPipeline runtime selection/dispatch to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_runtime_trait_impls_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_RUNTIME_TRAIT_IMPL_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains runtime trait impl marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate runtime trait implementations to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_runtime_type_definitions_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_RUNTIME_TYPE_DEFINITION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains runtime type definition marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate runtime type definitions to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_forward_current_provider_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_CURRENT_PROVIDER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains current-provider source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate forward current-provider source selection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_forward_runtime_config_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_CONFIG_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains forward runtime config source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate forward runtime config source selection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forwarder_runtime_config_reaches_forwarder_as_single_input() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let forwarder_path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let forwarder_source =
        fs::read_to_string(&forwarder_path).expect("read engine/forward_pipeline.rs");

    let bridge_slice = function_slice(
        &adapter_source,
        "pub(crate) async fn forward_with_preplanned_host_runtime",
        "pub(crate) async fn forward_proxy_request_with_host_runtime",
    );
    let constructor_slice = function_slice(
        &forwarder_source,
        "pub(crate) fn new_preplanned",
        "async fn record_success_result",
    );

    assert!(
        bridge_slice.contains("forwarder_config: ForwarderRuntimeConfig"),
        "host forward bridge must receive grouped forwarder runtime config"
    );
    assert!(
        bridge_slice.contains("        forwarder_config,"),
        "host forward bridge must pass grouped forwarder runtime config into RequestForwarder"
    );
    assert!(
        constructor_slice.contains("runtime_config: ForwarderRuntimeConfig"),
        "RequestForwarder constructor must receive grouped forwarder runtime config"
    );
    assert!(
        constructor_slice.contains("let ForwarderRuntimeConfig {"),
        "RequestForwarder constructor must own runtime config destructuring"
    );

    let bridge_forbidden_markers = [
        "let forwarder_options = forwarder_config.options",
        "forwarder_options.non_streaming_timeout",
        "forwarder_options.streaming_first_byte_timeout",
        "forwarder_options.streaming_idle_timeout",
        "forwarder_options.max_retries",
        "forwarder_config.rectifier",
        "forwarder_config.optimizer",
        "forwarder_config.copilot_optimizer",
    ];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(bridge_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in bridge_forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs forward bridge:{} splits runtime config marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "host forward bridge must not split forwarder runtime config before constructing RequestForwarder:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_forward_attempt_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_ATTEMPT_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains forward attempt source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate forward attempt source selection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_forwarder_launch_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARDER_LAUNCH_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs:{} contains forwarder launch marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate RequestForwarder launch/execution to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_core_host_delegates_forwarder_runtime_resources_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARDER_RUNTIME_RESOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchProxyRuntime:{} contains forwarder runtime resource marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy_core_host must delegate ForwarderRuntimeHostResources assembly to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_delegates_circuit_runtime_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let circuit_runtime =
        function_slice(&source, "    /// 热更新熔断器配置", "\n}\n\n#[cfg(test)]");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(circuit_runtime) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_CIRCUIT_RUNTIME_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs ProxyServer circuit runtime:{} contains provider router marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must delegate circuit runtime side effects to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_delegates_runtime_assembly_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let constructor = function_slice(&source, "    pub fn new", "    pub async fn start");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(constructor) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_RUNTIME_ASSEMBLY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs ProxyServer::new:{} contains runtime assembly marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must delegate runtime assembly to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_imports_runtime_services_from_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_HOST_COMPAT_IMPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs:{} contains host compat import marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must import runtime services from proxy_core_adapter, not proxy_core_host compat module:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub(crate) use super::transport::http::server::ProxyServer;",
        ],
        "legacy proxy/server.rs must remain a re-export shim after transport/http/server.rs split"
    );
}

#[test]
fn production_proxy_server_delegates_runtime_state_type_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_RUNTIME_STATE_TYPE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs:{} contains runtime state type marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must keep ProxyState type definition in proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_keeps_host_state_constructor_test_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let lines: Vec<&str> = source.lines().collect();

    let mut violations = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed != "pub fn new(" && trimmed != "use crate::database::Database;" {
            continue;
        }

        let previous = line_index
            .checked_sub(1)
            .and_then(|index| lines.get(index))
            .map(|line| line.trim())
            .unwrap_or_default();
        if previous != "#[cfg(test)]" {
            violations.push(format!(
                "src/proxy/transport/http/server.rs:{} keeps host-state constructor/import in production: `{}`",
                line_index + 1,
                trimmed
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must receive adapter-built ProxyState instead of constructing host state from Database/AppHandle:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_state_imports_use_adapter_path() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/proxy/auth_adapter.rs",
        "src/proxy/engine/context.rs",
        "src/proxy/transport/http/handlers.rs",
        "src/proxy/engine/response_pipeline.rs",
        "src/proxy/transport/http/server.rs",
    ];

    let mut violations = Vec::new();
    for relative in files {
        let path = manifest_dir.join(relative);
        let source = fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {relative}"));
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_PROXY_STATE_SERVER_COMPAT_PATH_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{relative}:{} contains ProxyState server compat marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production proxy modules must import ProxyState directly from proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_state_does_not_retain_injected_host_resources() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/host/cc_switch/proxy_state.rs");
    let source = fs::read_to_string(&path).expect("read proxy_state.rs");
    let proxy_state = source.as_str();

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(proxy_state) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_STATE_HOST_RESOURCE_RETENTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/proxy_state.rs ProxyState:{} retains host resource marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyState must leave host resources inside injected runtime sources:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_lib_does_not_compile_proxy_core_host_compat_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/lib.rs");
    let source = fs::read_to_string(&path).expect("read lib.rs");
    let lines: Vec<&str> = source.lines().collect();

    let mut violations = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        if line.trim() != "mod proxy_core_host;" {
            continue;
        }

        let previous = line_index
            .checked_sub(1)
            .and_then(|index| lines.get(index))
            .map(|line| line.trim())
            .unwrap_or_default();
        if previous != "#[cfg(test)]" {
            violations.push(format!(
                "src/lib.rs:{} declares proxy_core_host without #[cfg(test)]",
                line_index + 1
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "production lib.rs must keep proxy_core_host as test-only compatibility module:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_host_compat_surface_stays_test_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");
    let lines: Vec<&str> = source.lines().collect();
    let test_module_start = lines
        .windows(2)
        .position(|window| {
            window[0].trim() == "#[cfg(test)]" && window[1].trim_start().starts_with("mod tests")
        })
        .expect("proxy_core_host.rs should contain a cfg(test) tests module");

    let mut pending_test_cfg = false;
    let mut in_test_gated_item = false;
    let mut violations = Vec::new();

    for (line_index, line) in lines.iter().take(test_module_start).enumerate() {
        let code = line.split("//").next().unwrap_or_default().trim();
        if code.is_empty() {
            continue;
        }

        if in_test_gated_item {
            if code.ends_with(';') {
                in_test_gated_item = false;
            }
            continue;
        }

        if code == "#[cfg(test)]" {
            pending_test_cfg = true;
            continue;
        }

        let is_compat_import = code.starts_with("use ") || code.starts_with("pub(crate) use ");
        if pending_test_cfg && is_compat_import {
            pending_test_cfg = false;
            in_test_gated_item = !code.ends_with(';');
            continue;
        }

        violations.push(format!(
            "src/proxy_core_host.rs:{} contains non-test-only compat surface `{}`",
            line_index + 1,
            code
        ));
    }

    assert!(
        violations.is_empty(),
        "proxy_core_host.rs must remain a test-only compatibility shell; production services belong in proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_delegates_runtime_state_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let runtime_state = function_slice(&source, "    pub async fn start", "    fn build_router");
    let start_orchestration = function_slice(
        &source,
        "pub(crate) async fn start_proxy_http_server",
        "pub(crate) async fn bind_proxy_http_listener",
    );
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let bound_source = function_slice(
        &adapter_source,
        "pub(crate) fn record_proxy_server_bound_runtime_source",
        "pub(crate) async fn record_proxy_server_started_info_runtime_source",
    );
    let started_source = function_slice(
        &adapter_source,
        "pub(crate) async fn record_proxy_server_started_info_runtime_source",
        "pub(crate) async fn record_proxy_server_stopped_runtime_source",
    );
    let stopped_source = function_slice(
        &adapter_source,
        "pub(crate) async fn record_proxy_server_stopped_runtime_event_source",
        "pub(crate) async fn set_active_route_target_runtime_source",
    );
    let accept_loop_source = function_slice(
        &source,
        "pub(crate) fn spawn_proxy_http_accept_loop",
        "pub(crate) async fn await_proxy_http_accept_loop_stop",
    );

    assert!(
        runtime_state.contains(
            "start_proxy_http_server(&self.config, self.state.clone(), &self.http_server_handles)"
        ),
        "production ProxyServer::start must delegate start orchestration to proxy_core_adapter"
    );

    assert!(
        start_orchestration.contains("record_proxy_server_bound_runtime_source(")
            && start_orchestration.contains("record_proxy_server_started_info_runtime_source("),
        "HTTP transport start orchestration must use adapter-owned start runtime side-effect helpers"
    );

    assert!(
        accept_loop_source
            .contains("record_proxy_server_stopped_runtime_event_source(&state).await"),
        "HTTP transport accept loop must call adapter-owned stopped runtime side effects"
    );

    assert!(
        bound_source.contains("emit_proxy_server_started_event_source(")
            && bound_source.contains("record_proxy_server_listen_port_runtime_source(")
            && started_source.contains("record_proxy_server_started_runtime_source(")
            && started_source.contains("proxy_server_info_from_parts(")
            && stopped_source.contains("record_proxy_server_stopped_runtime_source(")
            && stopped_source.contains("emit_proxy_server_stopped_event_source("),
        "proxy_core_adapter must own server start/stop event, status, port, and info side effects"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(runtime_state) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_RUNTIME_STATE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs ProxyServer runtime state:{} contains runtime state marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must delegate runtime state projection/mutation to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_delegates_route_assembly_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let start_slice = function_slice(&source, "    pub async fn start", "    pub async fn stop");
    let adapter_start = function_slice(
        &source,
        "pub(crate) async fn start_proxy_http_server",
        "pub(crate) async fn bind_proxy_http_listener",
    );
    let adapter_router = function_slice(
        &source,
        "pub(crate) fn proxy_http_router_from_state",
        "pub(crate) fn spawn_proxy_http_accept_loop",
    );

    assert!(
        start_slice.contains(
            "start_proxy_http_server(&self.config, self.state.clone(), &self.http_server_handles)"
        ),
        "ProxyServer::start must delegate Axum route assembly through proxy_core_adapter"
    );

    assert!(
        adapter_start.contains("let app = proxy_http_router_from_state(state.clone());"),
        "HTTP transport start orchestration must build the Axum router from runtime state"
    );

    assert!(
        adapter_router.contains("/proxy/v1/health")
            && adapter_router.contains("/claude-desktop/v1/models")
            && adapter_router.contains("/v1/chat/completions")
            && adapter_router.contains("/gemini/v1beta/*path")
            && adapter_router.contains("middleware::from_fn_with_state(")
            && adapter_router.contains("DefaultBodyLimit::max(200 * 1024 * 1024)")
            && adapter_router.contains(".with_state(state)"),
        "HTTP transport module must own the management, protocol, middleware, and body-limit route tree"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(start_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_ROUTE_ASSEMBLY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs ProxyServer::start:{} contains route assembly marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must delegate Axum route assembly to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_delegates_accept_loop_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let start_slice = function_slice(&source, "    pub async fn start", "    pub async fn stop");
    let start_orchestration = function_slice(
        &source,
        "pub(crate) async fn start_proxy_http_server",
        "pub(crate) async fn bind_proxy_http_listener",
    );
    let accept_loop = function_slice(
        &source,
        "pub(crate) fn spawn_proxy_http_accept_loop",
        "pub(crate) async fn await_proxy_http_accept_loop_stop",
    );

    assert!(
        start_slice.contains(
            "start_proxy_http_server(&self.config, self.state.clone(), &self.http_server_handles)"
        ),
        "ProxyServer::start must delegate start orchestration to proxy_core_adapter"
    );

    assert!(
        start_orchestration
            .contains("spawn_proxy_http_accept_loop(listener, app, shutdown_rx, state)"),
        "HTTP transport start orchestration must launch the delegated Hyper accept loop"
    );

    assert!(
        accept_loop.contains("listener.accept()")
            && accept_loop.contains("OriginalHeaderCases::from_raw_bytes(")
            && accept_loop.contains(".preserve_header_case(true)")
            && accept_loop.contains("serve_connection(TokioIo::new(stream), service)")
            && accept_loop.contains("record_proxy_server_stopped_runtime_event_source(&state).await"),
        "HTTP transport module must own accept-loop, header-case capture, connection serving, and stop side effects"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(start_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_ACCEPT_LOOP_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs ProxyServer::start:{} contains accept-loop marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must delegate Hyper accept-loop and header-case capture to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_delegates_listener_bind_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let start_slice = function_slice(&source, "    pub async fn start", "    pub async fn stop");
    let start_orchestration = function_slice(
        &source,
        "pub(crate) async fn start_proxy_http_server",
        "pub(crate) async fn bind_proxy_http_listener",
    );
    let listener_bind = function_slice(
        &source,
        "pub(crate) async fn bind_proxy_http_listener",
        "pub(crate) fn proxy_http_router_from_state",
    );

    assert!(
        start_slice.contains(
            "start_proxy_http_server(&self.config, self.state.clone(), &self.http_server_handles)"
        ),
        "ProxyServer::start must delegate start orchestration to proxy_core_adapter"
    );

    assert!(
        start_orchestration.contains("bind_proxy_http_listener(config).await?"),
        "HTTP transport start orchestration must bind the listener through the lifecycle helper"
    );

    assert!(
        listener_bind.contains("format!(\"{}:{}\", config.listen_address, config.listen_port)")
            && listener_bind.contains("tokio::net::TcpListener::bind(&addr)")
            && listener_bind.contains(".local_addr()")
            && listener_bind.contains("ProxyError::BindFailed(format!(\"无效的地址: {e}\"))")
            && listener_bind.contains("ProxyError::BindFailed(e.to_string())"),
        "HTTP transport module must own listener address parsing, bind, local address lookup, and bind error mapping"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(start_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_LISTENER_BIND_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs ProxyServer::start:{} contains listener-bind marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must delegate listener binding to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_delegates_stop_wait_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let stop_slice = function_slice(
        &source,
        "    pub async fn stop",
        "    pub async fn get_status",
    );
    let stop_wait = function_slice(
        &source,
        "pub(crate) async fn await_proxy_http_accept_loop_stop",
        "/// 代理HTTP服务器",
    );

    assert!(
        stop_slice.contains("stop_proxy_http_server(&self.http_server_handles).await"),
        "ProxyServer::stop must delegate shutdown, handle take, and accept-loop wait to proxy_core_adapter"
    );

    assert!(
        stop_wait.contains("tokio::time::timeout(std::time::Duration::from_secs(5), handle).await")
            && stop_wait.contains("server_log_codes::STOPPED")
            && stop_wait.contains("server_log_codes::TASK_ERROR")
            && stop_wait.contains("server_log_codes::STOP_TIMEOUT")
            && stop_wait.contains("ProxyError::StopFailed(e.to_string())")
            && stop_wait.contains("ProxyError::StopTimeout")
            && stop_wait.contains("handles.signal_shutdown().await?")
            && stop_wait.contains("handles.take_server_handle().await")
            && stop_wait.contains("await_proxy_http_accept_loop_stop(handle).await"),
        "HTTP transport module must own accept-loop stop wait timeout, shutdown signaling, handle taking, logging, and error mapping"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(stop_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_STOP_WAIT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs ProxyServer::stop:{} contains stop-wait marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must delegate accept-loop stop wait/error mapping to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_server_delegates_handle_storage_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let server_lifecycle = function_slice(
        &source,
        "pub struct ProxyServer",
        "    pub async fn get_status",
    );
    let adapter_handles = function_slice(
        &source,
        "pub(crate) struct ProxyHttpServerHandles",
        "pub(crate) async fn bind_proxy_http_listener",
    );

    assert!(
        server_lifecycle.contains("http_server_handles: ProxyHttpServerHandles")
            && server_lifecycle.contains("ProxyHttpServerHandles::new()")
            && server_lifecycle.contains(
                "start_proxy_http_server(&self.config, self.state.clone(), &self.http_server_handles)"
            )
            && server_lifecycle.contains("stop_proxy_http_server(&self.http_server_handles).await"),
        "ProxyServer must delegate shutdown sender/server handle storage and running gates to HTTP transport helpers"
    );

    assert!(
        adapter_handles.contains("shutdown_tx: Arc<RwLock<Option<oneshot::Sender<()>>>>")
            && adapter_handles.contains("server_handle: Arc<RwLock<Option<JoinHandle<()>>>>")
            && adapter_handles.contains("ProxyError::AlreadyRunning")
            && adapter_handles.contains("ProxyError::NotRunning")
            && adapter_handles.contains("pub(crate) fn proxy_http_shutdown_channel()")
            && adapter_handles.contains("handles.ensure_not_running().await?")
            && adapter_handles.contains("proxy_http_shutdown_channel()")
            && adapter_handles.contains("handles.store_shutdown_sender(shutdown_tx).await")
            && adapter_handles.contains("handles.store_server_handle(handle).await"),
        "HTTP transport module must own HTTP server handle storage and running-state gates"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(server_lifecycle) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_HANDLE_STORAGE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/transport/http/server.rs ProxyServer lifecycle:{} contains handle-storage marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ProxyServer must not retain shutdown sender/server handle storage or running-state gates:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_config_source_requires_host_app_catalog() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("crates/proxy-core/src/ports.rs");
    let source = fs::read_to_string(&path).expect("read proxy-core ports.rs");
    let trait_source = function_slice(
        &source,
        "pub trait ProxyConfigSource",
        "pub trait ProviderSource",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(trait_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_CONFIG_SOURCE_APP_CATALOG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "crates/proxy-core/src/ports.rs ProxyConfigSource:{} contains host app catalog marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy-core config source must require host-provided app catalog:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_engine_delegates_route_policy_raw_contract() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("crates/proxy-core/src/engine.rs");
    let source = fs::read_to_string(&path).expect("read proxy-core engine.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_ENGINE_ROUTE_POLICY_RAW_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "crates/proxy-core/src/engine.rs:{} contains route policy raw marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy engine must consume route policy facts through domain helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_proxy_provider_router_legacy_module_is_reexport_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");
    let production_code: Vec<&str> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        production_code,
        vec![
            "#[allow(unused_imports)]",
            "pub(crate) use super::engine::routing::*;",
        ],
        "legacy proxy/provider_router.rs must remain a re-export shim after engine/routing.rs split"
    );
}

#[test]
fn production_provider_router_delegates_channel_route_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CHANNEL_ROUTE_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains channel route source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must delegate channel source fallback decisions to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_delegates_provider_selection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_SELECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains provider selection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must delegate provider selection to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_selects_provider_ids_not_provider_records() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_PROVIDER_RECORD_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains provider record marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must expose provider ids and leave provider records to host adapters:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_delegates_failover_config_fallback_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_FAILOVER_CONFIG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains failover config marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must delegate auto-failover config fallback to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_delegates_circuit_config_fallback_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CIRCUIT_CONFIG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains circuit config marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must delegate circuit config fallback to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_circuit_breaker_keeps_state_accessor_test_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/circuit_breaker.rs");
    let source = fs::read_to_string(&path).expect("read circuit_breaker.rs");
    let state_accessor_slice = function_slice(&source, "/// 获取当前状态", "/// 获取统计信息");

    assert!(
        state_accessor_slice.contains("#[cfg(test)]")
            && !state_accessor_slice.contains("#[allow(dead_code)]"),
        "CircuitBreaker::get_state should remain a test-only accessor, not production dead code"
    );
}

#[test]
fn production_provider_router_delegates_route_rejection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_ROUTE_REJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains route rejection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must delegate route rejection projection to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_delegates_management_route_resolution_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_MANAGEMENT_ROUTE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains management route marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must delegate management dry-run route resolution to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_delegates_health_persistence_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_HEALTH_PERSISTENCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains health persistence marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must delegate health persistence to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_uses_injected_source_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CONCRETE_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains concrete router source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must use an injected source adapter instead of holding Database directly:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_uses_split_source_ports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_COARSE_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains coarse router source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must keep provider/channel/config/health source ports split:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_provider_router_sources_to_host_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/provider_router_sources.rs");
    let source = fs::read_to_string(&source_path).expect("read provider_router_sources.rs");

    assert!(
        source.contains("pub(crate) struct CcSwitchProviderRouterSources")
            && source.contains("pub(crate) fn from_database(db: Arc<Database>)")
            && source.contains("CcSwitchProviderRouterConfigSource::new(")
            && source.contains("CcSwitchProviderRouterProviderSource::new(")
            && source.contains("CcSwitchProviderRouterChannelSource::new(")
            && source.contains("CcSwitchProviderRouterHealthStore::new(")
            && source.contains("pub(crate) fn provider_router_from_database("),
        "CC Switch ProviderRouter source assembly should live in host/cc_switch/provider_router_sources.rs"
    );
    assert!(
        adapter_source.contains(
            "use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;"
        ) && !adapter_source.contains(
            "pub(crate) use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database"
        ) && !adapter_source.contains("CcSwitchProviderRouterSources")
            && !adapter_source.contains("pub(crate) struct CcSwitchProviderRouterSources")
            && !adapter_source.contains("pub(crate) fn provider_router_from_database("),
        "proxy_core_adapter should only use the ProviderRouter factory privately and not re-export or own the source assembly"
    );
}

#[test]
fn production_provider_router_uses_route_channel_inputs() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CHANNEL_DAO_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs:{} contains channel DAO marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must use core route channel inputs instead of database DAO records or router-local records:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_config_source_uses_core_config_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path =
        manifest_dir.join("src/proxy/host/cc_switch/provider_router_config_source.rs");
    let source = fs::read_to_string(&source_path).expect("read provider_router_config_source.rs");
    let source_adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse futures::future::BoxFuture;",
    );

    assert!(
        source.contains("source: CcSwitchConfigSource")
            && source.contains("impl ProviderRouterConfigSource for CcSwitchProviderRouterConfigSource"),
        "ProviderRouter config source must hold the core-facing CcSwitchConfigSource in host/cc_switch"
    );
    assert!(
        source.contains("auto_failover_enabled_from_router_config_source")
            && source.contains("circuit_breaker_config_from_router_config_source")
            && source.contains("circuit_failure_threshold_from_router_config_source"),
        "ProviderRouter config source must project router config from ProxyConfigSource"
    );
    assert!(
        source.contains(
            "use crate::proxy::host::cc_switch::config_source::CcSwitchConfigSource;"
        ) && source.contains("use crate::proxy_core::api::config::CircuitBreakerConfig;"),
        "ProviderRouter config source should import host config source and core config contract directly"
    );
    for adapter_type in ["CcSwitchConfigSource", "CircuitBreakerConfig"] {
        assert!(
            !source_adapter_import.contains(adapter_type),
            "ProviderRouter config source should not import {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        !adapter_source.contains("struct CcSwitchProviderRouterConfigSource")
            && !adapter_source
                .contains("impl ProviderRouterConfigSource for CcSwitchProviderRouterConfigSource"),
        "proxy_core_adapter should not own the ProviderRouter config source"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CONFIG_SOURCE_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/provider_router_config_source.rs:{} contains config source adapter marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProviderRouter config source adapter must use core ProxyConfigSource instead of DB-specific config helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_provider_source_uses_core_provider_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path =
        manifest_dir.join("src/proxy/host/cc_switch/provider_router_provider_source.rs");
    let source = fs::read_to_string(&source_path).expect("read provider_router_provider_source.rs");
    let adapter_core_ports_import = function_slice(
        &adapter_source,
        "pub(crate) use crate::proxy_core::api::ports::{\n    channel_breaker_stats_from_parts",
        "};\nuse crate::proxy_core::api::ports::{",
    );
    let source_adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse futures::future::BoxFuture;",
    );

    assert!(
        source.contains("impl ProviderSource for CcSwitchProviderRouterProviderSource"),
        "ProviderRouter provider source must expose a core-facing ProviderSource"
    );
    assert!(
        source.contains("route_policies: CcSwitchRoutePolicySource")
            && source.contains("failover_provider_ids_from_route_policy_source"),
        "ProviderRouter provider source must read failover queue facts through RoutePolicySource"
    );
    assert!(
        source.contains("provider_failover_sources_from_router_provider_source")
            && source.contains("select_current_provider_ids_from_router_provider_source"),
        "ProviderRouter provider source must project provider ids through ProviderSource helpers"
    );
    assert!(
        !adapter_source.contains("struct CcSwitchProviderRouterProviderSource")
            && !adapter_source
                .contains("impl ProviderSource for CcSwitchProviderRouterProviderSource")
            && !adapter_source.contains(
                "impl ProviderRouterProviderSource for CcSwitchProviderRouterProviderSource"
            ),
        "proxy_core_adapter should not own the ProviderRouter provider source"
    );
    assert!(
        source.contains(
            "use crate::proxy::host::cc_switch::route_policy_source::CcSwitchRoutePolicySource;"
        ) && source.contains("use crate::proxy_core::api::domain::{AppKind, ProviderSpec};")
            && source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && source.contains("use crate::proxy_core::api::ports::ProviderSource;"),
        "ProviderRouter provider source should import host route-policy source and core provider contracts directly"
    );
    for adapter_type in [
        "CcSwitchRoutePolicySource",
        "ProviderSource",
        "ProviderSpec",
        "ProxyCoreAppKind",
        "ProxyCoreResult",
    ] {
        assert!(
            !source_adapter_import.contains(adapter_type),
            "ProviderRouter provider source should not import {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        !adapter_core_ports_import.contains("ProviderSource"),
        "proxy_core_adapter should not re-export ProviderSource"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_PROVIDER_SOURCE_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/provider_router_provider_source.rs:{} contains provider source adapter marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProviderRouter provider source adapter must use core ProviderSource projection instead of DB-specific router provider helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_channel_source_uses_core_channel_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path =
        manifest_dir.join("src/proxy/host/cc_switch/provider_router_channel_source.rs");
    let source = fs::read_to_string(&source_path).expect("read provider_router_channel_source.rs");
    let source_adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse futures::future::BoxFuture;",
    );

    assert!(
        source.contains("source: CcSwitchChannelSource")
            && source.contains("impl ProviderRouterChannelSource for CcSwitchProviderRouterChannelSource"),
        "ProviderRouter channel source must hold the core-facing CcSwitchChannelSource in host/cc_switch"
    );
    assert!(
        source.contains("router_channel_route_inputs_from_channel_source"),
        "ProviderRouter channel source must project route inputs from ChannelSource"
    );
    assert!(
        source.contains("use crate::proxy_core::api::management::ChannelRouteSource;")
            && source
                .contains("use crate::proxy_core::api::routing::RouteResolveChannelInput;"),
        "ProviderRouter channel source should import route input contracts directly from proxy_core"
    );
    for adapter_type in ["ChannelRouteSource", "RouteResolveChannelInput"] {
        assert!(
            !source_adapter_import.contains(adapter_type),
            "ProviderRouter channel source should not import core route contract {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        !adapter_source.contains("struct CcSwitchProviderRouterChannelSource")
            && !adapter_source.contains(
                "impl ProviderRouterChannelSource for CcSwitchProviderRouterChannelSource"
            ),
        "proxy_core_adapter should not own the ProviderRouter channel source"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CHANNEL_SOURCE_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/provider_router_channel_source.rs:{} contains channel source adapter marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProviderRouter channel source adapter must use core ChannelSource instead of DB-specific route helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_cc_switch_channel_source_lives_in_host_database_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let services_path = manifest_dir.join("src/proxy/host/cc_switch/proxy_services.rs");
    let services_source = fs::read_to_string(&services_path).expect("read proxy_services.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/database_channel_source.rs");
    let source =
        fs::read_to_string(&source_path).expect("read host/cc_switch/database_channel_source.rs");
    let source_adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse futures::future::BoxFuture;",
    );

    assert!(
        services_source.contains(
            "use crate::proxy::host::cc_switch::database_channel_source::CcSwitchChannelSource;"
        ),
        "proxy_services should import the CC Switch channel source from the host database module"
    );
    assert!(
        !adapter_source.contains("struct CcSwitchChannelSource")
            && !adapter_source.contains("impl ChannelSource for CcSwitchChannelSource"),
        "CcSwitchChannelSource implementation must not remain embedded in proxy_core_adapter.rs"
    );
    for marker in [
        "pub(crate) fn channel_model_records_from_db_source",
        "pub(crate) fn replace_channel_model_records_from_db_source",
        "pub(crate) fn create_channel_record_from_db_source",
        "pub(crate) fn channel_record_from_db_source",
        "pub(crate) fn update_channel_record_from_db_source",
        "pub(crate) fn delete_channel_record_from_db_source",
        "pub(crate) fn channel_records_from_db_source",
        "pub(crate) fn materialized_channel_records_from_db_source",
        "pub(crate) fn channel_key_records_from_db_source",
        "pub(crate) fn upsert_channel_key_record_from_db_source",
        "pub(crate) fn update_channel_key_record_from_db_source",
        "pub(crate) fn delete_channel_key_record_from_db_source",
        "pub(crate) fn channel_migration_preview_from_db_source",
        "pub(crate) fn channel_migration_materialize_from_db_source",
    ] {
        assert!(
            !adapter_source.contains(marker),
            "proxy_core_adapter should not own channel DB source helper `{marker}`"
        );
        assert!(
            source.contains(marker),
            "host/cc_switch/database_channel_source.rs should own channel DB source helper `{marker}`"
        );
    }
    assert!(
        source.contains("struct CcSwitchChannelSource")
            && source.contains("impl ChannelSource for CcSwitchChannelSource")
            && source.contains("channel_specs_from_source_lookup")
            && source.contains("channel_records_from_db_source")
            && source.contains("channel_migration_materialize_from_db_source"),
        "host/cc_switch/database_channel_source.rs must own the DB-backed ChannelSource wrapper"
    );
    assert!(
        source.contains("use crate::proxy_core::api::domain::{")
            && source.contains("channel_matches_query")
            && source.contains("channel_spec_from_input")
            && source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && source.contains("use crate::proxy_core::api::management::{")
            && source.contains("channel_record_from_input")
            && source.contains("ChannelMigrationMaterializeInput")
            && source.contains("use crate::proxy_core::api::ports::ChannelSource;")
            && source.contains("use crate::proxy_core::api::routing::{ChannelQuery, ChannelSpec};"),
        "host/cc_switch/database_channel_source.rs should import channel contracts directly from proxy_core"
    );
    for adapter_type in [
        "AppKind",
        "ChannelKeyRecord",
        "ChannelMigrationMaterializeInput",
        "ChannelMigrationPreviewInput",
        "ChannelModelRecord",
        "ChannelQuery",
        "ChannelRecord",
        "ChannelRouteSource",
        "ChannelSource",
        "ChannelSpec",
        "ModelRouteInput",
        "ProxyChannelWriteRequest",
        "ProxyCoreResult",
    ] {
        assert!(
            !source_adapter_import.contains(adapter_type),
            "database channel source should not import {adapter_type} through proxy_core_adapter"
        );
    }
}

#[test]
fn production_provider_router_health_store_uses_core_attempt_facts() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let source_path = manifest_dir.join("src/proxy/host/cc_switch/provider_router_health_store.rs");
    let source = fs::read_to_string(&source_path).expect("read provider_router_health_store.rs");
    let direct_core_imports = [
        "use crate::proxy_core::api::domain::AppKind;",
        "use crate::proxy_core::api::errors::ProxyCoreResult;",
        "use crate::proxy_core::api::ports::{",
    ];
    let adapter_import = function_slice(&source, "use crate::proxy_core_adapter::{", "};");

    assert!(
        source.contains("impl ProviderHealthStore for CcSwitchProviderRouterHealthStore")
            && source.contains("ProviderAttemptResult")
            && source.contains("record_provider_attempt_in_db_source"),
        "ProviderRouter health store must write provider health through core ProviderHealthStore attempt projection"
    );
    assert!(
        direct_core_imports
            .iter()
            .all(|direct_import| source.contains(direct_import))
            && source.contains(
                "ChannelAttemptResult, ChannelHealthReset, ProviderAttemptResult, ProviderHealthStore,"
            ),
        "ProviderRouter health store must import health attempt facts and ports directly from proxy_core"
    );
    for adapter_type in [
        "AppKind",
        "ChannelAttemptResult",
        "ChannelHealthReset",
        "ProviderAttemptResult",
        "ProviderHealthStore",
        "ProxyCoreResult",
    ] {
        assert!(
            !adapter_import.contains(adapter_type),
            "ProviderRouter health store must not import {adapter_type} through proxy_core_adapter"
        );
    }
    assert!(
        source.contains("fn record_channel_health<'a>(")
            && source.contains("result: ChannelAttemptResult")
            && source.contains("BoxFuture<'a, Result<(), AppError>>")
            && source.contains("record_channel_health_attempt_from_router_db(&self.db, result)"),
        "ProviderRouter health store must write channel health through core ChannelAttemptResult projection"
    );
    assert!(
        source.contains("fn reset_channel_health(&self, reset: ChannelHealthReset)")
            && source.contains("reset_channel_health_from_router_db(&self.db, reset)"),
        "ProviderRouter health store must reset channel health through core ChannelHealthReset facts"
    );
    assert!(
        !adapter_source.contains("struct CcSwitchProviderRouterHealthStore")
            && !adapter_source
                .contains("impl ProviderHealthStore for CcSwitchProviderRouterHealthStore")
            && !adapter_source
                .contains("impl ProviderRouterHealthStore for CcSwitchProviderRouterHealthStore"),
        "proxy_core_adapter should not own the ProviderRouter health store"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&adapter_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_MANAGEMENT_ADAPTER_DTO_EXPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains proxy management DTO export marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        if code.contains("proxy_core_adapter") && code.contains("ProviderAttemptResult") {
            violations.push(format!(
                "src/proxy/host/cc_switch/provider_router_health_store.rs:{} imports ProviderAttemptResult from proxy_core_adapter",
                line_index + 1
            ));
        }
        let direct_core =
            code.contains("crate::proxy_core::") || code.contains("cc_switch_proxy_core::");
        if direct_core && !direct_core_imports.contains(&code.trim()) {
            violations.push(format!(
                "src/proxy/host/cc_switch/provider_router_health_store.rs:{} contains unexpected direct proxy-core import `{}`",
                line_index + 1,
                code.trim()
            ));
        }
        for marker in FORBIDDEN_PROVIDER_ROUTER_HEALTH_STORE_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/host/cc_switch/provider_router_health_store.rs:{} contains health store adapter marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ProviderRouter health store adapter must route health writes through attempt facts instead of direct DB health writes:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_provider_router_resets_channel_health_with_core_reset_fact() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");
    let router_slice = function_slice(
        &source,
        "pub(crate) trait ProviderRouterHealthStore",
        "    /// 仅释放 HalfOpen permit",
    );

    assert!(
        router_slice.contains("fn reset_channel_health(&self, reset: ChannelHealthReset)")
            && router_slice.contains("channel_health_reset_from_parts(channel_id, app_type)")
            && router_slice.contains(".reset_channel_health(channel_health_reset_from_parts"),
        "ProviderRouter channel reset must pass the app-scoped core ChannelHealthReset fact to its health store"
    );
    assert!(
        !router_slice.contains(".reset_channel_health(channel_id)"),
        "ProviderRouter must not reset channel health through a bare channel_id"
    );
}

#[test]
fn production_channel_health_store_reads_channel_breaker_stats_through_core_port() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let adapter_slice = function_slice(
        &adapter_source,
        "pub(crate) async fn channel_breaker_stats_with_router_source",
        "pub(crate) fn proxy_response_to_core_response",
    );

    assert!(
        adapter_slice.contains(".get_proxy_channel_app_type(channel_id)")
            && adapter_slice.contains(".get_channel_circuit_breaker_stats(channel_id, &app_type)")
            && adapter_slice.contains("channel_breaker_stats_from_parts("),
        "ChannelHealthStore adapter helper must expose channel breaker stats through a core stats fact"
    );

    let store_path = manifest_dir.join("src/proxy/host/cc_switch/channel_health_store.rs");
    let store_source = fs::read_to_string(&store_path).expect("read channel_health_store.rs");
    let store_adapter_import =
        function_slice(&store_source, "use crate::proxy_core_adapter::{", "};");
    assert!(
        store_source.contains("fn channel_breaker_stats<'a>(")
            && store_source
                .contains("channel_breaker_stats_with_router_source(&self.db, &self.router, channel_id).await"),
        "ChannelHealthStore host source must route breaker stats through the adapter helper"
    );
    assert!(
        store_source.contains("use crate::proxy_core::api::errors::ProxyCoreResult;")
            && store_source.contains("use crate::proxy_core::api::ports::{")
            && store_source.contains(
                "ChannelAttemptResult, ChannelBreakerStats, ChannelHealthReset, ChannelHealthStore,"
            ),
        "ChannelHealthStore host source must import channel health facts and ports directly from proxy_core"
    );
    for adapter_type in [
        "ChannelAttemptResult",
        "ChannelBreakerStats",
        "ChannelHealthReset",
        "ChannelHealthStore",
        "ProxyCoreResult",
    ] {
        assert!(
            !store_adapter_import.contains(adapter_type),
            "ChannelHealthStore host source must not import {adapter_type} through proxy_core_adapter"
        );
    }

    let router_path = manifest_dir.join("src/proxy/engine/routing.rs");
    let router_source = fs::read_to_string(&router_path).expect("read engine/routing.rs");
    let stats_slice = function_slice(
        &router_source,
        "pub async fn get_channel_circuit_breaker_stats",
        "    async fn failure_threshold_for_app",
    );

    assert!(
        !stats_slice.contains("#[cfg(test)]"),
        "ProviderRouter channel breaker stats must stay production-visible for management API"
    );
}

#[test]
fn production_provider_router_records_channel_health_with_core_attempt_fact() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");
    let router_slice = function_slice(
        &source,
        "pub(crate) trait ProviderRouterHealthStore",
        "    /// 重置熔断器（手动恢复）",
    );

    assert!(
        router_slice.contains("result: ChannelAttemptResult")
            && router_slice.contains("BoxFuture<'a, Result<(), AppError>>")
            && router_slice.contains(".record_channel_health(ChannelAttemptResult {"),
        "ProviderRouter channel write must pass the core ChannelAttemptResult fact to its health store"
    );
    assert!(
        !router_slice.contains("record_channel_health(\n            channel_id,"),
        "ProviderRouter must not write channel health through primitive channel health arguments"
    );
}

#[test]
fn production_provider_router_delegates_live_circuit_map_to_runtime() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/routing.rs");
    let source = fs::read_to_string(&path).expect("read engine/routing.rs");
    let struct_slice = function_slice(&source, "pub struct ProviderRouter", "impl ProviderRouter");

    assert!(
        struct_slice.contains("circuit_runtime"),
        "ProviderRouter must hold live circuit state through a runtime object"
    );
    assert!(
        source.contains("struct ProviderRoutingCircuitRuntime"),
        "engine/routing.rs must keep live circuit map in ProviderRoutingCircuitRuntime"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(struct_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_LIVE_CIRCUIT_MAP_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/routing.rs ProviderRouter:{} contains live circuit map marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "provider router must delegate live circuit map ownership to runtime:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_constructs_provider_router_through_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut rust_files = Vec::new();
    collect_rust_files(&manifest_dir.join("src"), &mut rust_files);

    let mut violations = Vec::new();
    for path in rust_files {
        let relative = path
            .strip_prefix(&manifest_dir)
            .expect("source path under manifest dir")
            .to_string_lossy()
            .replace('\\', "/");

        let source = fs::read_to_string(&path).expect("read host source file");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains(PROVIDER_ROUTER_DATABASE_CONSTRUCTOR_MARKER) {
                violations.push(format!(
                    "{}:{} contains direct `{}`",
                    relative,
                    line_index + 1,
                    PROVIDER_ROUTER_DATABASE_CONSTRUCTOR_MARKER
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production host code must construct ProviderRouter through proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_host_constructs_proxy_engine_through_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut rust_files = Vec::new();
    collect_rust_files(&manifest_dir.join("src"), &mut rust_files);

    let mut violations = Vec::new();
    for path in rust_files {
        let relative = path
            .strip_prefix(&manifest_dir)
            .expect("source path under manifest dir")
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED_PROXY_ENGINE_CONSTRUCTOR_FILES.contains(&relative.as_str()) {
            continue;
        }

        let source = fs::read_to_string(&path).expect("read host source file");
        for (line_index, line) in production_lines(&source) {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains(PROXY_ENGINE_CONSTRUCTOR_MARKER) {
                violations.push(format!(
                    "{}:{} contains direct production `{}`",
                    relative,
                    line_index + 1,
                    PROXY_ENGINE_CONSTRUCTOR_MARKER
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production host code must construct ProxyEngine through src/proxy_core_adapter.rs:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_excludes_proxy_engine_constructor_facade() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    assert!(
        !source.contains("fn proxy_engine_from_services"),
        "proxy_core_adapter should construct ProxyEngine at the owning call site instead of keeping a one-hop constructor facade"
    );
}

#[test]
fn production_forward_attempt_excludes_provider_only_constructor() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/route_attempt.rs");
    let source = fs::read_to_string(&path).expect("read route_attempt.rs");
    let lines: Vec<&str> = source.lines().collect();

    let mut violations = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        if line.trim() != "pub(crate) fn from_provider(provider: Provider) -> Self {" {
            continue;
        }

        let previous = line_index
            .checked_sub(1)
            .and_then(|index| lines.get(index))
            .map(|line| line.trim())
            .unwrap_or_default();
        if previous != "#[cfg(test)]" {
            violations.push(format!(
                "src/proxy/route_attempt.rs:{} keeps provider-only ForwardAttempt constructor in production",
                line_index + 1
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "production ForwardAttempt must be created from ProxyEngine route selections, not provider-only fallback constructors:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_forward_error_excludes_host_provider_payload() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/engine/forward_pipeline.rs");
    let source = fs::read_to_string(&path).expect("read engine/forward_pipeline.rs");
    let forward_error = function_slice(
        &source,
        "pub struct ForwardError",
        "pub struct RequestForwarder",
    );

    let forbidden_markers = ["Provider", "provider:"];
    let mut violations = Vec::new();
    for (line_index, line) in production_lines(forward_error) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/engine/forward_pipeline.rs ForwardError:{} contains host provider payload marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production ForwardError should carry neutral error facts, not host Provider payloads:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_uses_grouped_api_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for (column, _) in code.match_indices(PROXY_CORE_MARKER) {
            if !code[column..].starts_with(PROXY_CORE_API_MARKER) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains non-api proxy-core access: {}",
                    line_index + 1,
                    code.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter.rs must use proxy_core::api as its integration surface:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_error_mapper_uses_grouped_api_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read proxy/error_mapper.rs");

    let mut violations = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for (column, _) in code.match_indices(PROXY_CORE_MARKER) {
            if !code[column..].starts_with(PROXY_CORE_API_MARKER) {
                violations.push(format!(
                    "src/proxy/error_mapper.rs:{} contains non-api proxy-core access: {}",
                    line_index + 1,
                    code.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy/error_mapper.rs must use proxy_core::api as its integration surface:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_response_adapter_uses_grouped_api_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/response_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy/response_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for (column, _) in code.match_indices(PROXY_CORE_MARKER) {
            if !code[column..].starts_with(PROXY_CORE_API_MARKER) {
                violations.push(format!(
                    "src/proxy/response_adapter.rs:{} contains non-api proxy-core access: {}",
                    line_index + 1,
                    code.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy/response_adapter.rs must use proxy_core::api as its integration surface:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_response_adapter_owns_core_transport_imports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/response_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy/response_adapter.rs");
    let adapter_import = function_slice(
        &source,
        "use crate::proxy_core_adapter::{",
        "};\nuse axum::",
    );
    let adapter_import_identifiers: Vec<&str> = adapter_import
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|identifier| !identifier.is_empty())
        .collect();

    assert!(
        source.contains("crate::proxy_core::api::auth::ClaudeDesktopModelListResponse")
            && source.contains("crate::proxy_core::api::domain::AppKind")
            && source.contains("crate::proxy_core::api::transport::{")
            && source.contains("extract_gemini_model_from_path")
            && source.contains("parse_json_request_body")
            && source.contains("parse_json_request_body_or_null")
            && source.contains("request_body_stream_flag")
            && source.contains("ProxyBody")
            && source.contains("crate::proxy_core::api::events::ProxyEventEnvelope")
            && source.contains("crate::proxy_core::api::management::{")
            && source.contains("crate::proxy_core::api::model_catalog::{")
            && source.contains("crate::proxy_core::api::ports::{")
            && source.contains("crate::proxy_core::api::routing::InterfaceKind")
            && source.contains("crate::proxy_core::api::transforms::{")
            && source.contains("build_codex_tool_context_from_request")
            && source.contains("crate::proxy_core::api::usage::{"),
        "response_adapter should import core auth/domain/transport/event/management/model_catalog/ports/routing/transforms/usage contracts directly"
    );

    let mut violations = Vec::new();
    for marker in [
        "AppKind",
        "InterfaceKind",
        "append_query_to_endpoint_path",
        "extract_gemini_model_from_path",
        "parse_json_request_body",
        "parse_json_request_body_or_null",
        "rebuilt_json_proxy_response",
        "request_body_read_error_message",
        "request_body_stream_flag",
        "strip_endpoint_prefix",
        "transformed_sse_proxy_response",
        "ProxyBody",
        "ProxyCoreResponse",
        "ProxyEventEnvelope",
        "ProxyRequest",
        "ProxyResult",
        "ProxyTransportResponse",
        "ProxyTransportResponseBody",
        "AxumResponseBuildErrorContext",
        "CoreResponseBuildFailureContext",
        "ClaudeDesktopModelListResponse",
        "ClaudeTransformStreamingDecision",
        "CodexChatTransformStreamingDecision",
        "CodexToolContext",
        "build_codex_tool_context_from_request",
        "CodexResponsesProxyRequest",
        "JsonProxyRequestInput",
        "codex_responses_proxy_request_from_input",
        "json_proxy_request_from_input",
        "parse_json_proxy_request_body",
        "parse_json_proxy_request_body_or_null",
        "CurrentRouteTarget",
        "ProxyRuntimeStatus",
        "ClientModelCatalogResponse",
        "RoutableModelList",
        "AppChannelListQuery",
        "AppChannelManagementRequest",
        "AppChannelResponse",
        "AppListRequest",
        "AppListResponse",
        "AppModelCatalogRequest",
        "AppModelListQuery",
        "ChannelBreakerStatsResponse",
        "ChannelCreateRequest",
        "ChannelDeleteResponse",
        "ChannelHealthResetResponse",
        "ChannelKeyDeleteResponse",
        "ChannelKeyPathRequest",
        "ChannelKeyRecord",
        "ChannelKeyRecordResponse",
        "ChannelKeysResponse",
        "ChannelListQuery",
        "ChannelListRequest",
        "ChannelListResponse",
        "ChannelMigrationMaterializeResponse",
        "ChannelMigrationPreviewResponse",
        "ChannelModelRecord",
        "ChannelModelsResponse",
        "ChannelPathRequest",
        "ChannelRecord",
        "ChannelRecordResponse",
        "ChannelRouteCandidate",
        "ChannelRouteRejected",
        "ChannelTestResponse",
        "CurrentRouteResponse",
        "GroupListQuery",
        "GroupListRequest",
        "HealthCheckRequest",
        "HealthCheckResponse",
        "ManagementAppPathRequest",
        "ProviderListResponse",
        "ProxyChannelKeyPatchRequest",
        "ProxyChannelKeyWriteRequest",
        "ProxyChannelModelsReplaceRequest",
        "ProxyChannelPatchRequest",
        "ProxyChannelTestRequest",
        "ProxyChannelWriteRequest",
        "ProxyStatusRequest",
        "ProxyStatusResponse",
        "RouteGroupListResponse",
        "RouteResolveManagementRequest",
        "RouteResolveRequest",
        "RouteResolveResponse",
        "UpstreamSseAggregationKind",
        "CLAUDE_PARSER_CONFIG",
        "CODEX_PARSER_CONFIG",
        "GEMINI_PARSER_CONFIG",
        "OPENAI_PARSER_CONFIG",
    ] {
        if adapter_import_identifiers
            .iter()
            .any(|identifier| identifier == &marker)
        {
            violations.push(format!(
                "response_adapter still imports core API marker `{marker}` from proxy_core_adapter"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "response_adapter should not route pure core auth/domain/transport/event/management/model_catalog/ports/routing/transforms/usage contracts through proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn http_handlers_route_signature_dtos_through_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/transport/http/handlers.rs");
    let source = fs::read_to_string(&path).expect("read proxy/transport/http/handlers.rs");
    let response_adapter_import = function_slice(
        &source,
        "response_adapter::{",
        "};\nuse crate::proxy_core_adapter::ProxyState;",
    );
    let response_adapter_import_identifiers: Vec<&str> = response_adapter_import
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|identifier| !identifier.is_empty())
        .collect();
    let proxy_core_adapter_imports: Vec<String> = production_lines(&source)
        .map(|(_, line)| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| line.contains("proxy_core_adapter"))
        .map(str::to_string)
        .collect();

    let mut missing = Vec::new();
    for marker in [
        "AppChannelListQuery",
        "AppChannelResponse",
        "AppListResponse",
        "AppModelListQuery",
        "ChannelBreakerStatsResponse",
        "ChannelDeleteResponse",
        "ChannelHealthResetResponse",
        "ChannelKeyDeleteResponse",
        "ChannelKeyRecord",
        "ChannelKeyRecordResponse",
        "ChannelKeysResponse",
        "ChannelListQuery",
        "ChannelListResponse",
        "ChannelMigrationMaterializeResponse",
        "ChannelMigrationPreviewResponse",
        "ChannelModelRecord",
        "ChannelModelsResponse",
        "ChannelRecord",
        "ChannelRecordResponse",
        "ChannelRouteCandidate",
        "ChannelRouteRejected",
        "ChannelTestResponse",
        "ClaudeDesktopModelListResponse",
        "ClientModelCatalogResponse",
        "CurrentRouteResponse",
        "CurrentRouteTarget",
        "GroupListQuery",
        "HealthCheckResponse",
        "ProviderListResponse",
        "ProxyChannelKeyPatchRequest",
        "ProxyChannelKeyWriteRequest",
        "ProxyChannelModelsReplaceRequest",
        "ProxyChannelPatchRequest",
        "ProxyChannelTestRequest",
        "ProxyChannelWriteRequest",
        "ProxyRuntimeStatus",
        "ProxyStatusResponse",
        "RoutableModelList",
        "RouteGroupListResponse",
        "RouteResolveRequest",
        "RouteResolveResponse",
    ] {
        if !response_adapter_import_identifiers
            .iter()
            .any(|identifier| identifier == &marker)
        {
            missing.push(format!(
                "handlers.rs response_adapter import is missing signature DTO `{marker}`"
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "HTTP handler signature DTOs should come through response_adapter:\n{}",
        missing.join("\n")
    );
    assert_eq!(
        proxy_core_adapter_imports,
        vec!["use crate::proxy_core_adapter::ProxyState;"],
        "HTTP handlers should keep only runtime state on proxy_core_adapter"
    );
}

#[test]
fn proxy_events_uses_grouped_api_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/events.rs");
    let source = fs::read_to_string(&path).expect("read proxy/events.rs");

    let mut violations = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for (column, _) in code.match_indices(PROXY_CORE_MARKER) {
            if !code[column..].starts_with(PROXY_CORE_API_MARKER) {
                violations.push(format!(
                    "src/proxy/events.rs:{} contains non-api proxy-core access: {}",
                    line_index + 1,
                    code.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy/events.rs must use proxy_core::api as its integration surface:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_events_owns_core_event_stream_imports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/events.rs");
    let source = fs::read_to_string(&path).expect("read proxy/events.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    assert!(
        source.contains("use crate::proxy_core::api::events::{")
            && source.contains("build_proxy_events_connected_payload")
            && source.contains("build_proxy_events_lagged_payload")
            && source.contains("ProxyEventEnvelope")
            && source.contains("PROXY_EVENTS_CONNECTED_EVENT")
            && source.contains("PROXY_EVENTS_LAGGED_EVENT"),
        "proxy/events.rs should import event stream contracts directly from proxy_core::api::events"
    );

    assert!(
        !source.contains("proxy_core_adapter")
            && !source.contains("proxy_events_connected_message")
            && !source.contains("proxy_events_lagged_message"),
        "proxy/events.rs should not route event stream contracts through proxy_core_adapter"
    );

    assert!(
        !adapter_source.contains("fn proxy_events_connected_message(")
            && !adapter_source.contains("fn proxy_events_lagged_message("),
        "proxy_core_adapter should not keep one-hop proxy event stream message facades"
    );
}

#[test]
fn proxy_core_adapter_owns_copilot_header_constants() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_PROVIDER_COPILOT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains provider Copilot constant marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must own Copilot header constants instead of reading provider module constants:\n{}",
        violations.join("\n")
    );
}

fn function_slice<'a>(source: &'a str, start_marker: &str, end_marker: &str) -> &'a str {
    let start = source
        .find(start_marker)
        .unwrap_or_else(|| panic!("missing start marker {start_marker}"));
    let tail = &source[start..];
    let end = tail
        .find(end_marker)
        .unwrap_or_else(|| panic!("missing end marker {end_marker}"));
    &tail[..end]
}

fn dependency_names_from_manifest(manifest: &str) -> Vec<String> {
    let mut in_dependency_section = false;
    let mut names = Vec::new();

    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section = line.trim_matches(['[', ']']);
            in_dependency_section = section == "dependencies"
                || section == "build-dependencies"
                || section == "dev-dependencies"
                || section.ends_with(".dependencies");
            continue;
        }

        if !in_dependency_section {
            continue;
        }

        if let Some((name, _)) = line.split_once('=') {
            names.push(name.trim().trim_matches('"').to_string());
        }
    }

    names
}

fn production_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    let lines: Vec<&str> = source.lines().collect();
    let production_len = lines
        .windows(2)
        .position(|window| {
            window[0].trim() == "#[cfg(test)]" && window[1].trim_start().starts_with("mod tests")
        })
        .unwrap_or(lines.len());

    lines.into_iter().take(production_len).enumerate()
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read source directory") {
        let entry = entry.expect("read source entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
