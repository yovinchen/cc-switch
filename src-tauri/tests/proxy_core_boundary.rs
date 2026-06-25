use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED_PROXY_CORE_FILES: &[&str] = &[
    "src/lib.rs",
    "src/proxy_core_adapter.rs",
    "src/services/model_fetch_transport.rs",
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
const FORBIDDEN_PROXY_ERROR_MAPPER_CODEX_PROJECTION_MARKERS: &[&str] = &[
    "CodexProxyErrorContext",
    "CodexProxyHostErrorFacts",
    "CodexProxyErrorKind",
    "codex_proxy_error_code(",
    "codex_proxy_error_facts(",
    "codex_proxy_error_kind(",
];
const FORBIDDEN_PROXY_ERROR_MAPPER_FORWARD_FAILURE_PROJECTION_MARKERS: &[&str] = &[
    "ForwardFailureKind",
    "forward_failure_kind_from_proxy_status(",
    "forward_failure_message(",
];
const FORBIDDEN_PROXY_ERROR_MAPPER_DISPLAY_MESSAGE_MARKERS: &[&str] = &[
    "ProxyError::UpstreamError",
    "ProxyError::Timeout",
    "ProxyError::ForwardFailed",
    "ProxyError::NoAvailableProvider",
    "ProxyError::AllProvidersCircuitOpen",
    "ProxyError::NoProvidersConfigured",
    "ProxyError::MaxRetriesExceeded",
    "ProxyError::ProviderUnhealthy",
    "ProxyError::DatabaseError",
    "ProxyError::TransformError",
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
const FORBIDDEN_MODEL_FETCH_TRANSPORT_DTO_EXPORT_MARKERS: &[&str] =
    &["pub use crate::proxy_core::api::model_catalog::FetchedModel"];
const FORBIDDEN_MODEL_FETCH_COMMAND_DTO_IMPORT_MARKERS: &[&str] =
    &["services::model_fetch_transport::FetchedModel"];
const FORBIDDEN_MODEL_FETCH_COMMAND_PROVIDER_DETAIL_MARKERS: &[&str] =
    &["crate::provider::parse_custom_user_agent("];
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
const FORBIDDEN_MANAGED_ACCOUNT_AUTH_STRATEGY_MARKERS: &[&str] = &[
    "match auth.strategy",
    "ProviderAuthStrategy::GitHubCopilot",
    "ProviderAuthStrategy::CodexOAuth",
];
const FORBIDDEN_MANAGED_ACCOUNT_AUTH_DTO_MARKERS: &[&str] =
    &["struct ManagedAccountAuthResolution"];
const FORBIDDEN_MANAGED_ACCOUNT_AUTH_PLAN_SOURCE_MARKERS: &[&str] = &[
    "managed_account_auth_plan(",
    "provider_github_copilot_managed_account_id(",
    "provider_codex_oauth_managed_account_id(",
    "ManagedAccountAuthPlan::",
];
const FORBIDDEN_MANAGED_ACCOUNT_AUTH_RUNTIME_TEXT_MARKERS: &[&str] = &[
    "[Copilot] AppHandle 不可用",
    "GitHub Copilot 认证不可用（无 AppHandle）",
    "[Copilot] 使用指定账号",
    "[Copilot] 使用默认账号获取 token",
    "[Copilot] 成功获取 Copilot token",
    "[Copilot] 获取 Copilot token 失败",
    "GitHub Copilot 认证失败:",
    "[CodexOAuth] AppHandle 不可用",
    "Codex OAuth 认证不可用（无 AppHandle）",
    "[CodexOAuth] 使用指定账号",
    "[CodexOAuth] 使用默认账号获取 token",
    "[CodexOAuth] 成功获取 access_token",
    "[CodexOAuth] 获取 access_token 失败",
    "Codex OAuth 认证失败:",
];
const FORBIDDEN_ADAPTER_MANAGED_AUTH_PLAN_RUNTIME_CALL_MARKERS: &[&str] = &[
    "crate::proxy::managed_account_auth::resolve_copilot_auth(",
    "crate::proxy::managed_account_auth::resolve_codex_oauth(",
];
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
const FORBIDDEN_HANDLER_PROXY_REQUEST_BRIDGE_MARKERS: &[&str] = &["ProxyRequest::new("];
const FORBIDDEN_HANDLER_RAW_JSON_BODY_PARSE_MARKERS: &[&str] = &[
    "parse_json_request_body(",
    "parse_json_request_body_or_null(",
    "request_body_stream_flag(",
];
const FORBIDDEN_HANDLER_DIRECT_BODY_COLLECTION_MARKERS: &[&str] =
    &[".collect()", "request_body_read_error_message("];
const FORBIDDEN_HANDLER_PROVIDER_ADAPTER_DECISION_MARKERS: &[&str] = &[
    "get_adapter(",
    ".needs_transform(",
    "super::providers::should_convert_codex_responses_to_chat(",
];
const FORBIDDEN_HANDLER_CODEX_HISTORY_RECORD_MARKERS: &[&str] =
    &[".record_response(", "record_responses_sse_stream("];
const FORBIDDEN_PROTOCOL_HANDLER_FORWARD_CORE_ERROR_MARKERS: &[&str] = &[
    "proxy_core_error_to_proxy_error(error)",
    "record_forward_error_usage(",
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
const FORBIDDEN_PROXY_CORE_ADAPTER_SMALL_HELPER_FACADE_MARKERS: &[&str] = &[
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
    "fn channel_reachability_probe_error(",
    "fn channel_auth_profile_missing_key_error(",
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
    "fn circuit_failure_threshold_from_app_config(",
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
    "fn empty_client_model_catalog_raw(",
    "fn client_model_catalog_raw_from_text(",
    "fn apply_provider_model_mapping(",
    "fn claude_takeover_client_model_for_upstream(",
    "fn claude_takeover_default_display_name(",
    "fn proxy_global_config_from_config(",
    "fn proxy_app_config_from_config_parts(",
    "fn proxy_runtime_config_from_config(",
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
const FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_RESPONSE_TRANSFORM_MARKERS: &[&str] = &[
    "gemini_response_to_anthropic_message(",
    "openai_responses_to_anthropic_message(",
    "openai_chat_to_anthropic_message(",
    "synthesize_gemini_tool_call_id_with_uuid",
    "rectified_tool_names",
    "body.get(\"candidates\")",
    "body.get(\"output\")",
];
const FORBIDDEN_HANDLER_MANAGEMENT_AUTH_DECISION_MARKERS: &[&str] = &[
    "state.config",
    ".config.read()",
    "std::env::var(",
    "CC_SWITCH_PROXY_MANAGEMENT_TOKEN",
    "resolve_management_auth_decision(",
    "validate_management_bearer_header(",
];
const FORBIDDEN_RESPONSE_PROCESSOR_USAGE_PROVIDER_PROJECTION_MARKERS: &[&str] = &[
    "fn create_usage_collector(",
    "SseUsageCollector::new(",
    "provider_kind_from_provider(",
    "AppKind::from(",
    "ctx.provider()?",
    "response_usage_provider_facts(",
    "response_usage_provider_facts_from_optional(",
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
const FORBIDDEN_RESPONSE_PROCESSOR_STREAM_ORCHESTRATION_MARKERS: &[&str] = &[
    "fn create_logged_passthrough_stream(",
    "async_stream::stream!",
    "SseEventScanner",
    "SsePassthroughEventKind",
    "SseUsageAccumulator",
    "SseUsageFinishGuard",
    "StreamingTimeoutPhase",
    "tokio::time::timeout(duration, stream.next())",
    "push_passthrough_bytes(",
];
const FORBIDDEN_RESPONSE_PROCESSOR_BODY_DECODE_PROJECTION_MARKERS: &[&str] = &[
    "已接收上游响应体",
    "decode_response_body(",
    "ResponseBodyDecodeLogLevel::",
    ".status.log_event()",
];
const FORBIDDEN_RESPONSE_PROCESSOR_RESPONSE_LOG_PROJECTION_MARKERS: &[&str] = &[
    "response_headers_log_summary(",
    "get_content_encoding(",
    "已接收上游流式响应",
    "流式响应含 content-encoding",
    "上游响应体内容",
    "String::from_utf8_lossy(",
];
const FORBIDDEN_RESPONSE_BUILD_CONTEXT_LITERAL_MARKERS: &[&str] = &[
    "构建流式响应失败",
    "构建响应失败",
    "构建 SSE 响应失败",
    "构建 Responses 响应失败",
    "构建 Responses 错误响应失败",
    "构建代理错误响应失败",
];
const FORBIDDEN_PROXY_CORE_ADAPTER_RESPONSE_BUILD_CONTEXT_MARKERS: &[&str] = &[
    "enum AxumResponseBuildErrorContext",
    "impl AxumResponseBuildErrorContext",
    "构建流式响应失败",
    "构建响应失败",
    "构建 SSE 响应失败",
    "构建 Responses 响应失败",
    "构建 Responses 错误响应失败",
    "构建代理错误响应失败",
];
const FORBIDDEN_HANDLER_RESPONSE_PARSE_FAILURE_LOG_PROJECTION_MARKERS: &[&str] = &[
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
    "codex_proxy_error_response(",
    "proxy_core_response_to_axum_response(",
    "build_codex_proxy_error_response(",
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
    "转换响应失败",
    "Chat → Responses 响应转换失败",
    "ResponseTransformFailureContext::",
    "response_transform_error_to_proxy_error(",
];
const FORBIDDEN_HANDLER_TRANSFORMED_USAGE_POLICY_MARKERS: &[&str] = &[
    "TransformedResponseUsageFormat::",
    "claude_stream_usage_event_filter",
    "codex_stream_usage_event_filter",
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
    "openai_responses_to_anthropic_message(",
    "openai_chat_to_anthropic_message(",
    "gemini_response_to_anthropic_message_with_shadow(",
    "create_anthropic_sse_stream",
    "create_openai_chat_to_anthropic_sse_stream(",
    "create_openai_responses_to_anthropic_sse_stream(",
    "create_gemini_to_anthropic_sse_stream_with_callbacks(",
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
    "record_codex_chat_response_history(",
];
const FORBIDDEN_HANDLER_CODEX_STREAM_TRANSFORM_MARKERS: &[&str] = &[
    "create_responses_sse_stream_from_chat_with_context(",
    "record_codex_chat_response_sse_history(",
];
const FORBIDDEN_HANDLER_CODEX_STREAMING_DECISION_MARKERS: &[&str] = &[
    "response_headers_indicate_sse(response.headers())",
    "Some(UpstreamSseAggregationKind::ChatCompletions)",
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
fn request_context_does_not_preselect_provider() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handler_context.rs");
    let source = fs::read_to_string(&path).expect("read handler_context.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_REQUEST_CONTEXT_PROVIDER_PRESELECT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handler_context.rs:{} contains provider preselection marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handler_context.rs");
    let source = fs::read_to_string(&path).expect("read handler_context.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_REQUEST_CONTEXT_PROVIDER_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handler_context.rs:{} contains provider adapter marker `{}`",
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
fn proxy_error_mapper_delegates_codex_error_projection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read error_mapper.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_ERROR_MAPPER_CODEX_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/error_mapper.rs:{} contains codex error projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Proxy error mapper must delegate Codex error envelope projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
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
fn proxy_error_mapper_delegates_display_message_policy_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/error_mapper.rs");
    let source = fs::read_to_string(&path).expect("read error_mapper.rs");
    let function = function_slice(
        &source,
        "pub fn get_error_message",
        "pub(crate) fn proxy_core_error_to_proxy_error",
    );

    assert!(
        function.contains("proxy_error_display_message(error)"),
        "Proxy error mapper must delegate display-message policy to proxy_core_adapter"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(function) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_ERROR_MAPPER_DISPLAY_MESSAGE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/error_mapper.rs:{} contains display-message marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Proxy error mapper must not locally maintain ProxyError display messages:\n{}",
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
fn proxy_core_adapter_delegates_proxy_error_display_message_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn proxy_error_display_message",
        "pub(crate) use crate::proxy_core::api::errors::",
    );

    assert!(
        function.contains("proxy_error_display_message_from_status("),
        "adapter must delegate ProxyError display-message selection to proxy-core"
    );
    assert!(
        function.contains("proxy_error_status_kind(error)"),
        "adapter should pass ProxyError status kind into the core display-message policy"
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
            "adapter must not locally format ProxyError display text `{marker}`"
        );
    }
}

#[test]
fn proxy_error_status_projection_lives_in_proxy_core_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let proxy_error_path = manifest_dir.join("src/proxy/error.rs");
    let proxy_error_source = fs::read_to_string(&proxy_error_path).expect("read proxy/error.rs");
    let error_mapper_path = manifest_dir.join("src/proxy/error_mapper.rs");
    let error_mapper_source =
        fs::read_to_string(&error_mapper_path).expect("read proxy/error_mapper.rs");

    let adapter_function = function_slice(
        &adapter_source,
        "pub(crate) fn proxy_error_status_kind",
        "pub(crate) fn proxy_error_status_code",
    );

    assert!(
        adapter_function.contains("ProxyErrorStatusKind::ForwardFailed")
            && adapter_function.contains("ProxyErrorStatusKind::UpstreamError(*status)")
            && adapter_function.contains("ProxyErrorStatusKind::AuthError"),
        "proxy_core_adapter should own host ProxyError to core status-kind projection"
    );
    assert!(
        !proxy_error_source.contains("fn proxy_error_status_kind("),
        "proxy/error.rs should not own host-to-core status-kind projection"
    );
    assert!(
        error_mapper_source.contains("proxy_error_status_kind")
            && !error_mapper_source.contains("use crate::proxy::error::proxy_error_status_kind"),
        "error_mapper should use the adapter status-kind projection"
    );
}

#[test]
fn basic_health_status_handlers_use_management_contracts() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let health_handler = function_slice(&source, "pub async fn health_check", "/// 获取服务状态");
    let status_handler = function_slice(
        &source,
        "pub async fn get_status",
        "/// GET /proxy/v1/events",
    );

    assert!(
        status_handler.contains(".proxy_engine()")
            && status_handler.contains(".proxy_status_response(request)"),
        "status HTTP handler must delegate runtime status response assembly to ProxyEngine"
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
                        "src/proxy/handlers.rs {}:{} contains basic status wrapper marker `{}`",
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
    let path = manifest_dir.join("src/proxy/server.rs");
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
                    "src/proxy/server.rs get_status:{} contains runtime status source marker `{}`",
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
fn provider_list_handler_delegates_sources_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                    "src/proxy/handlers.rs list_proxy_providers:{} contains runtime source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                    "src/proxy/handlers.rs resolve_proxy_route:{} contains route dry-run marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                    "src/proxy/handlers.rs list_proxy_apps:{} contains app-list source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                    "src/proxy/handlers.rs list_all_proxy_channels:{} contains channel DB marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                        "src/proxy/handlers.rs {}:{} contains channel CRUD source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                        "src/proxy/handlers.rs {}:{} contains channel key/model source marker `{}`",
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
fn proxy_channel_runtime_source_delegates_key_selection_to_core_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let dao_path = manifest_dir.join("src/database/dao/proxy_channels.rs");
    let dao_source = fs::read_to_string(&dao_path).expect("read proxy_channels.rs");
    let function = function_slice(
        &adapter_source,
        "fn load_channel_key_value_from_database",
        "impl ChannelKeyRuntimeSource for CcSwitchBorrowedChannelKeyRuntimeSource",
    );

    assert!(
        function.contains(".get_proxy_channel_key(")
            && function.contains("select_enabled_proxy_channel_key_runtime_candidate(")
            && function.contains("channel_key_value_from_runtime_candidate("),
        "channel key runtime source must load raw DB key records and delegate enabled-key selection to the core adapter"
    );
    assert!(
        !function.contains(".get_enabled_proxy_channel_key("),
        "channel key runtime source must not rely on the DAO test convenience selector"
    );
    assert!(
        dao_source.contains("#[cfg(test)]\n    pub(crate) fn get_enabled_proxy_channel_key"),
        "DAO enabled-key selector should remain test-only while production runtime selection lives in the adapter"
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                    "src/proxy/handlers.rs test_proxy_channel:{} contains channel test source marker `{}`",
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
fn channel_list_route_branch_delegates_dry_run_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                    "src/proxy/handlers.rs list_proxy_channels:{} contains channel source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                    "src/proxy/handlers.rs list_proxy_groups:{} contains group source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                    "src/proxy/handlers.rs get_current_proxy_route:{} contains current-route source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                "/// POST /proxy/v1/channels/{channel_id}/breakers/reset",
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
                        "src/proxy/handlers.rs {}:{} contains migration source marker `{}`",
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
fn channel_health_reset_handler_delegates_response_to_proxy_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
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
                    "src/proxy/handlers.rs reset_proxy_channel_breaker:{} contains health reset source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn handle_claude_desktop_models",
        "async fn handle_messages_for_app",
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
                    "src/proxy/handlers.rs handle_claude_desktop_models:{} contains Claude Desktop provider marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_PROXY_REQUEST_BRIDGE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains direct proxy request bridge marker `{}`",
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
fn production_handlers_parse_json_bodies_through_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_RAW_JSON_BODY_PARSE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains direct JSON body parse marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_DIRECT_BODY_COLLECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains direct body collection marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_PROVIDER_ADAPTER_DECISION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains provider decision marker `{}`",
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
fn production_handlers_delegate_codex_history_recording_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CODEX_HISTORY_RECORD_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains direct Codex history recording marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let protocol_handlers = [
        function_slice(
            &source,
            "async fn handle_messages_for_app(",
            "\n}\n\n/// Claude 格式转换处理",
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
            "\n}\n\nasync fn handle_codex_chat_to_responses_transform(",
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
                        "src/proxy/handlers.rs:{} contains forward core error marker `{}`",
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
fn production_provider_adapters_delegate_base_url_errors_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let provider_paths = [
        "src/proxy/providers/claude.rs",
        "src/proxy/providers/codex.rs",
        "src/proxy/providers/gemini.rs",
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
    let path = manifest_dir.join("src/proxy/providers/gemini.rs");
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
                    "src/proxy/providers/gemini.rs extract_auth:{} contains auth info marker `{}`",
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
        "src/proxy/providers/claude.rs",
        "src/proxy/providers/gemini.rs",
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
    let relative = "src/proxy/providers/codex.rs";
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
    let path = manifest_dir.join("src/proxy/providers/codex.rs");
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
                    "src/proxy/providers/codex.rs extract_auth:{} contains auth info marker `{}`",
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
    let path = manifest_dir.join("src/proxy/providers/claude.rs");
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
                    "src/proxy/providers/claude.rs extract_auth:{} contains auth info marker `{}`",
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
        "src/proxy/providers/codex.rs",
        "src/proxy/providers/gemini.rs",
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
    let path = manifest_dir.join("src/proxy/providers/claude.rs");
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
                    "src/proxy/providers/claude.rs get_auth_headers:{} contains auth header marker `{}`",
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
        "src/proxy/providers/claude.rs",
        "src/proxy/providers/codex.rs",
        "src/proxy/providers/gemini.rs",
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
    let path = manifest_dir.join("src/proxy/http_client.rs");
    let source = fs::read_to_string(&path).expect("read http_client.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HTTP_CLIENT_PROXY_URL_VALIDATION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/http_client.rs:{} contains explicit proxy URL validation marker `{}`",
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
fn proxy_core_adapter_delegates_response_parse_failure_log_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    assert!(
        source.contains("upstream_response_parse_failure_log_message"),
        "proxy_core_adapter should expose the core response parse failure log helper"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_ADAPTER_RESPONSE_PARSE_LOG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs:{} contains response parse log marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy_core_adapter must delegate response parse failure log text and body projection to proxy-core:\n{}",
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
    let path = manifest_dir.join("src/proxy/providers/claude.rs");
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
                    "src/proxy/providers/claude.rs needs_transform:{} contains transform decision marker `{}`",
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
    let path = manifest_dir.join("src/proxy/providers/claude.rs");
    let source = fs::read_to_string(&path).expect("read claude provider adapter source");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_COMPAT_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/providers/claude.rs:{} contains Claude compat facade marker `{}`",
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
    let path = manifest_dir.join("src/proxy/providers/claude.rs");
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
                    "src/proxy/providers/claude.rs transform_claude_request_for_api_format:{} contains request transform marker `{}`",
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
    let path = manifest_dir.join("src/proxy/providers/claude.rs");
    let source = fs::read_to_string(&path).expect("read claude provider adapter source");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_NORMALIZE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/providers/claude.rs:{} contains message normalization marker `{}`",
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
fn production_claude_provider_adapter_delegates_response_transforms_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/providers/claude.rs");
    let source = fs::read_to_string(&path).expect("read claude provider adapter source");
    let transform_response = function_slice(
        &source,
        "    fn transform_response(&self, body: serde_json::Value) -> Result<serde_json::Value, ProxyError>",
        "}",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(transform_response) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_CLAUDE_PROVIDER_ADAPTER_RESPONSE_TRANSFORM_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/providers/claude.rs transform_response:{} contains response transform marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Claude provider adapter must delegate response transform dispatch to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_handlers_delegate_management_auth_decisions_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handler = function_slice(
        &source,
        "pub async fn require_proxy_management_auth",
        "/// GET /proxy/v1/apps",
    );

    assert!(
        handler.contains(".proxy_engine()") && handler.contains(".validate_management_auth("),
        "management auth middleware must delegate auth validation to ProxyEngine"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(handler) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_MANAGEMENT_AUTH_DECISION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs require_proxy_management_auth:{} contains management auth decision marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "management auth middleware must delegate token-source decisions to proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn response_processor_delegates_usage_provider_projection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/response_processor.rs");
    let source = fs::read_to_string(&path).expect("read response_processor.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_RESPONSE_PROCESSOR_USAGE_PROVIDER_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/response_processor.rs:{} contains usage provider projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "response processor must build provider usage facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
    );
}

#[test]
fn response_processor_delegates_stream_orchestration_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/response_processor.rs");
    let source = fs::read_to_string(&path).expect("read response_processor.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_RESPONSE_PROCESSOR_STREAM_ORCHESTRATION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/response_processor.rs:{} contains stream orchestration marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "response processor must delegate stream scanner/timeout orchestration to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn response_processor_delegates_body_decode_projection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/response_processor.rs");
    let source = fs::read_to_string(&path).expect("read response_processor.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_RESPONSE_PROCESSOR_BODY_DECODE_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/response_processor.rs:{} contains body decode projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "response processor must delegate body decode/log projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn response_processor_delegates_response_log_projection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/response_processor.rs");
    let source = fs::read_to_string(&path).expect("read response_processor.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_RESPONSE_PROCESSOR_RESPONSE_LOG_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/response_processor.rs:{} contains response log projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "response processor must delegate response header log projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn response_pipeline_delegates_axum_build_context_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files = [
        ("src/proxy/handlers.rs", "read handlers.rs"),
        (
            "src/proxy/response_processor.rs",
            "read response_processor.rs",
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
        violations.is_empty(),
        "response handlers must delegate Axum response build context projection to proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proxy_core_adapter_delegates_response_build_context_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    assert!(
        source.contains("ProxyResponseBuildErrorContext"),
        "proxy_core_adapter should expose the core response build context type"
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
        "proxy_core_adapter must delegate response build context message policy to proxy-core:\n{}",
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
            && source.contains("claude_desktop_direct_provider_validation_issue(")
            && source.contains("claude_desktop_proxy_provider_config_validation_issue(")
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
            "pub(crate) fn provider_claude_desktop_routes_support_1m_by_default",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_RESPONSE_PARSE_FAILURE_LOG_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains response parse failure log marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_RESPONSE_BUILD_ERROR_MAPPING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains response build error mapping marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_RESPONSE_TRANSFORM_ERROR_MAPPING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains response transform error mapping marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_TRANSFORMED_USAGE_POLICY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains transformed usage policy marker `{}`",
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
fn handlers_delegate_transformed_response_build_context_to_response_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let claude_transform = function_slice(
        &source,
        "async fn handle_claude_transform(",
        "\n}\n\n// ============================================================================\n// Codex API",
    );
    let codex_transform = function_slice(
        &source,
        "async fn handle_codex_chat_to_responses_transform(",
        "\n}\n\n/// 把上游 Chat Completions 的错误响应转换为 Responses API 错误形状。",
    );

    let mut violations = Vec::new();
    for transform in [claude_transform, codex_transform] {
        for (line_index, line) in production_lines(transform) {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_HANDLER_TRANSFORMED_RESPONSE_BUILD_CONTEXT_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "src/proxy/handlers.rs:{} contains transformed response build marker `{}`",
                        line_index + 1,
                        marker
                    ));
                }
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CLAUDE_RESPONSE_TRANSFORM_DISPATCH_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains Claude response transform dispatch marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let transform = function_slice(
        &source,
        "async fn handle_claude_transform(",
        "\n}\n\n// ============================================================================\n// Codex API",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(transform) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CLAUDE_STREAMING_DECISION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains Claude streaming decision marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CODEX_NON_STREAM_TRANSFORM_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains Codex non-stream transform marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CODEX_STREAM_TRANSFORM_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains Codex stream transform marker `{}`",
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
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let transform = function_slice(
        &source,
        "async fn handle_codex_chat_to_responses_transform(",
        "\n}\n\n/// 把上游 Chat Completions 的错误响应转换为 Responses API 错误形状。",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(transform) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_HANDLER_CODEX_STREAMING_DECISION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/handlers.rs:{} contains Codex streaming decision marker `{}`",
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
fn response_pipeline_uses_core_sse_header_decision() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let hyper_client = fs::read_to_string(manifest_dir.join("src/proxy/hyper_client.rs"))
        .expect("read hyper_client.rs");
    let response_processor =
        fs::read_to_string(manifest_dir.join("src/proxy/response_processor.rs"))
            .expect("read response_processor.rs");
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
        adapter.contains("response_headers_indicate_sse(response_headers)"),
        "protocol transform handlers should route SSE detection through proxy_core_adapter"
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
fn production_forwarder_delegates_upstream_url_planning_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_URL_PLANNING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains upstream URL planning marker `{}`",
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
fn proxy_core_adapter_delegates_upstream_url_plan_policy_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let impl_slice = function_slice(
        &source,
        "impl ForwarderRequestSource for CcSwitchForwarderRequestSource",
        "pub(crate) fn anthropic_redacted_thinking_placeholder",
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
            !source.contains(marker),
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
fn production_forwarder_delegates_claude_provider_helpers_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_CLAUDE_PROVIDER_COMPAT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains Claude provider compat marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_CODEX_PROVIDER_COMPAT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains Codex provider compat marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_REQUEST_OPTIMIZER_PROVIDER_FACT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains request optimizer provider fact marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_REQUEST_HEADER_PROVIDER_FACT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains request header provider fact marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_REQUEST_URL_PROVIDER_FACT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains request URL provider fact marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_REQUEST_MEDIA_PROVIDER_FACT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains request media provider fact marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_TRANSFORM_GATE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains provider adapter transform gate marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_REQUEST_TRANSFORM_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains provider adapter request transform marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_BASE_URL_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains provider adapter base URL marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_AUTH_INFO_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains provider adapter auth info marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_AUTH_HEADER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains provider adapter auth header marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_URL_BUILD_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains provider adapter URL build marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_NAME_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains provider adapter name marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_REGISTRY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains provider adapter registry marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_PROVIDER_ADAPTER_TRAIT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains provider adapter trait marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_CHANNEL_STATUS_MAPPING_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains channel status mapping marker `{}`",
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
    let provider_mod_path = manifest_dir.join("src/proxy/providers/mod.rs");
    let provider_mod = fs::read_to_string(&provider_mod_path).expect("read providers/mod.rs");
    let proxy_paths = [
        "src/proxy/forwarder.rs",
        "src/proxy/handlers.rs",
        "src/proxy/server.rs",
    ];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&provider_mod) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_MODULE_CODEX_HISTORY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/providers/mod.rs:{} contains Codex history marker `{}`",
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
    let provider_mod_path = manifest_dir.join("src/proxy/providers/mod.rs");
    let provider_mod = fs::read_to_string(&provider_mod_path).expect("read providers/mod.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&provider_mod) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_MODULE_KIND_FACADE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/providers/mod.rs:{} contains provider kind facade marker `{}`",
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
    let provider_mod_path = manifest_dir.join("src/proxy/providers/mod.rs");
    let provider_mod = fs::read_to_string(&provider_mod_path).expect("read providers/mod.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&provider_mod) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_MODULE_MANAGED_AUTH_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/providers/mod.rs:{} contains managed auth marker `{}`",
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
fn model_fetch_commands_use_adapter_dto_entrypoint() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let transport_path = manifest_dir.join("src/services/model_fetch_transport.rs");
    let transport_source =
        fs::read_to_string(&transport_path).expect("read model_fetch_transport.rs");
    let command_paths = ["src/commands/model_fetch.rs", "src/commands/codex_oauth.rs"];

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&transport_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_MODEL_FETCH_TRANSPORT_DTO_EXPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/services/model_fetch_transport.rs:{} contains DTO export marker `{}`",
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
        }
    }

    assert!(
        violations.is_empty(),
        "model fetch commands must use proxy_core_adapter as the FetchedModel DTO entrypoint:\n{}",
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
            && copilot_source.contains("pub use crate::proxy_core_adapter"),
        "copilot_auth.rs should re-export legacy managed-auth command DTOs from proxy_core_adapter"
    );
    assert!(
        codex_source.contains("CodexOAuthStatus"),
        "codex_oauth_auth.rs should consume the core Codex OAuth status DTO"
    );
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

    assert!(
        adapter_source
            .contains("pub(crate) use crate::proxy_core::api::transport::parse_custom_user_agent"),
        "proxy_core_adapter should expose the core custom User-Agent parser"
    );
    assert!(
        !adapter_source.contains("crate::provider::parse_custom_user_agent(")
            && !adapter_source.contains("HeaderValue::from_str("),
        "proxy_core_adapter must not own or call host-local custom User-Agent parsing policy"
    );
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
fn production_proxy_service_delegates_takeover_status_sources_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs get_takeover_status:{} contains takeover status source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                        "src/services/proxy.rs {function_name}:{} contains live takeover app-list marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs set_takeover_for_app:{} contains official-warning source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                        "src/services/proxy.rs {function_name}:{} contains current-provider source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs sync_live_config_to_provider:{} contains live-token sync source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs set_takeover_for_app:{} contains takeover enabled config marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs set_takeover_for_app:{} contains takeover backup source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs set_takeover_for_app:{} contains takeover backup delete marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs set_takeover_for_app:{} contains takeover active flag marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs set_takeover_for_app:{} contains takeover health cleanup marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs start_with_takeover:{} contains start-takeover backup cleanup marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs start_with_takeover:{} contains start-takeover active flag marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs stop_with_restore:{} contains stop-restore enabled config marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs restore_live_config_for_app_inner:{} contains simple restore backup source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs restore_live_config_for_app_with_fallback_inner:{} contains fallback restore backup source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs restore_live_from_ssot_for_app:{} contains SSOT restore provider source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs restore_live_from_ssot_for_app:{} contains SSOT restore live-write marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                        "src/services/proxy.rs {function_name}:{} contains live backup save marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs update_live_backup_from_provider_inner:{} contains existing backup source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs update_live_backup_from_provider_inner:{} contains update backup save marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs hot_switch_provider_inner:{} contains hot-switch source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs stop_with_restore_keep_state:{} contains keep-state active flag marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                        "src/services/proxy.rs {function_name}:{} contains stop-restore cleanup marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs recover_from_crash:{} contains crash-recovery cleanup marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                        "src/services/proxy.rs {function_name}:{} contains global proxy enabled marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                        "src/services/proxy.rs {function_name}:{} contains proxy config source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                        "src/services/proxy.rs {function_name}:{} contains server factory marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVICE_SERVER_TYPE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/services/proxy.rs:{} contains direct server type marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                        "src/services/proxy.rs {function_name}:{} contains effective settings source marker `{}`",
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
    let path = manifest_dir.join("src/services/proxy.rs");
    let source = fs::read_to_string(&path).expect("read services/proxy.rs");
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
                    "src/services/proxy.rs write_claude_live:{} contains provider facade marker `{}`",
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
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "pub(crate) fn provider_with_channel_auth_key",
        "pub(crate) use crate::proxy_core::api::domain::extract_claude_base_url_from_settings",
    );

    assert!(
        function.contains("settings_config_with_channel_auth_key_for_app("),
        "provider_with_channel_auth_key must delegate app-typed channel key settings policy to proxy-core"
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
    let function = function_slice(
        &source,
        "pub(crate) fn apply_channel_auth_profile_providers_from_source",
        "pub(crate) fn apply_channel_auth_profile_providers_from_db",
    );

    assert!(
        function.contains("channel_auth_profile_provider_application("),
        "apply_channel_auth_profile_providers_from_source must delegate channel auth application planning to proxy-core"
    );
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
    let core_ports_path = manifest_dir.join("crates/proxy-core/src/ports.rs");
    let core_ports_source = fs::read_to_string(&core_ports_path).expect("read core ports.rs");
    let services_trait = function_slice(
        &core_ports_source,
        "pub trait ProxyServices",
        "pub trait ProxyConfigSource",
    );
    let source_function = function_slice(
        &source,
        "pub(crate) fn apply_channel_auth_profile_providers_from_source",
        "pub(crate) fn apply_channel_auth_profile_providers_from_db",
    );
    let services_struct = function_slice(
        &source,
        "pub(crate) struct CcSwitchProxyServices",
        "#[allow(dead_code)]\nimpl<R> CcSwitchProxyServices",
    );
    let services_impl = function_slice(
        &source,
        "impl<R> ProxyServices for CcSwitchProxyServices",
        "pub(crate) trait HostForwardRuntime",
    );
    let db_function = function_slice(
        &source,
        "pub(crate) fn apply_channel_auth_profile_providers_from_db",
        "pub(crate) fn required_forward_attempts_from_sources",
    );
    let runtime_source_lookup = function_slice(
        &source,
        "fn load_channel_key_value_from_database",
        "impl ChannelKeyRuntimeSource for CcSwitchBorrowedChannelKeyRuntimeSource",
    );
    let borrowed_runtime_source_impl = function_slice(
        &source,
        "impl ChannelKeyRuntimeSource for CcSwitchBorrowedChannelKeyRuntimeSource",
        "impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource",
    );
    let owned_runtime_source_impl = function_slice(
        &source,
        "impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource",
        "pub(crate) fn apply_channel_auth_profile_providers_from_source",
    );

    assert!(
        core_ports_source.contains("pub trait ChannelKeyRuntimeSource"),
        "proxy-core ports should expose channel key lookup behind a runtime source trait"
    );
    assert!(
        services_trait.contains("channel_key_runtime_source("),
        "proxy-core ProxyServices should expose channel key runtime source as an injectable service"
    );
    assert!(
        !source.contains("trait ChannelKeyRuntimeSource"),
        "proxy_core_adapter should implement the core channel key runtime source, not define a host-local trait"
    );
    assert!(
        source.contains("ChannelKeyRuntimeSource"),
        "proxy_core_adapter should import the core channel key runtime source contract"
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
        source_function.contains("dyn ChannelKeyRuntimeSource")
            && source_function.contains(".load_channel_key_value("),
        "auth profile application should consume the channel key runtime source contract"
    );
    assert!(
        db_function.contains("channel_key_runtime_source_from_db(db)")
            && !db_function.contains(".get_enabled_proxy_channel_key("),
        "DB-backed auth profile application should inject the channel key runtime source instead of inline DB lookup"
    );
    assert!(
        runtime_source_lookup.contains(".get_proxy_channel_key(")
            && runtime_source_lookup.contains("select_enabled_proxy_channel_key_runtime_candidate(")
            && runtime_source_lookup.contains("channel_key_value_from_runtime_candidate("),
        "CC Switch channel key runtime lookup helper should own raw DB lookup, core selection, and key value projection"
    );
    assert!(
        borrowed_runtime_source_impl.contains("load_channel_key_value_from_database(")
            && owned_runtime_source_impl.contains("load_channel_key_value_from_database("),
        "borrowed and owned CC Switch channel key runtime sources should delegate through the shared lookup helper"
    );
}

#[test]
fn proxy_core_adapter_forward_pipeline_injects_channel_key_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let pipeline_struct = function_slice(
        &source,
        "pub(crate) struct CcSwitchForwardPipeline",
        "impl<R> CcSwitchForwardPipeline",
    );
    let pipeline_constructors = function_slice(
        &source,
        "impl<R> CcSwitchForwardPipeline",
        "impl<R> ForwardPipeline for CcSwitchForwardPipeline",
    );
    let pipeline_impl = function_slice(
        &source,
        "impl<R> ForwardPipeline for CcSwitchForwardPipeline",
        "type UsageCallbackWithTiming",
    );
    let host_runtime_trait = function_slice(
        &source,
        "pub(crate) trait HostForwardRuntime",
        "impl ProxyServiceRuntimeResources for CcSwitchProxyRuntime",
    );
    let host_runtime_impl = function_slice(
        &source,
        "impl HostForwardRuntime for CcSwitchProxyRuntime",
        "pub(crate) fn forward_with_optional_host_runtime",
    );
    let optional_runtime_function = function_slice(
        &source,
        "pub(crate) fn forward_with_optional_host_runtime",
        "#[cfg(test)]\npub(crate) use crate::proxy_core::api::routing::{\n    forwarding_requires_runtime_error_message",
    );
    let host_forward_function = function_slice(
        &source,
        "pub(crate) async fn forward_proxy_request_with_host_runtime",
        "#[derive(Clone)]\npub(crate) struct CcSwitchForwardPipeline",
    );
    let attempt_source_function = function_slice(
        &source,
        "pub(crate) fn required_forward_attempts_from_sources",
        "pub(crate) type FailoverSwitchSchedulerRef",
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
        host_runtime_trait.contains("channel_key_runtime_source")
            && host_runtime_impl.contains("channel_key_runtime_source"),
        "HostForwardRuntime should receive channel key runtime source from the pipeline"
    );
    assert!(
        optional_runtime_function
            .contains(".forward_host(channel_key_runtime_source, request, plan)"),
        "optional runtime dispatcher should forward the injected channel key runtime source"
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
fn production_forwarder_delegates_managed_auth_resolution_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_MANAGED_AUTH_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains managed auth marker `{}`",
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
fn production_managed_account_auth_delegates_runtime_plan_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/managed_account_auth.rs");
    let source = fs::read_to_string(&path).expect("read managed_account_auth.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_MANAGED_ACCOUNT_AUTH_STRATEGY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/managed_account_auth.rs:{} contains runtime strategy marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "managed account auth runtime selection must use proxy-core planning instead of local strategy branching:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_managed_account_auth_uses_adapter_resolution_dto() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/managed_account_auth.rs");
    let source = fs::read_to_string(&path).expect("read managed_account_auth.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_MANAGED_ACCOUNT_AUTH_DTO_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/managed_account_auth.rs:{} contains managed auth DTO marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "managed account auth resolution DTO must be owned by proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_managed_account_auth_uses_adapter_plan_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/managed_account_auth.rs");
    let source = fs::read_to_string(&path).expect("read managed_account_auth.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_MANAGED_ACCOUNT_AUTH_PLAN_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/managed_account_auth.rs:{} contains managed auth plan/source marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "managed account auth planning and provider account projection must be owned by proxy_core_adapter:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_managed_account_auth_runtime_text_delegates_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/managed_account_auth.rs");
    let source = fs::read_to_string(&path).expect("read managed_account_auth.rs");

    for marker in [
        "managed_account_app_handle_unavailable_log_message(",
        "managed_account_app_handle_unavailable_error_message(",
        "managed_account_token_request_log_message(",
        "managed_account_token_success_log_message(",
        "managed_account_token_failure_log_message(",
        "managed_account_token_failure_error_message(",
    ] {
        assert!(
            source.contains(marker),
            "managed_account_auth.rs should delegate runtime text marker `{marker}` to proxy_core_adapter"
        );
    }

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_MANAGED_ACCOUNT_AUTH_RUNTIME_TEXT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/managed_account_auth.rs:{} contains managed auth runtime text marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "managed account auth runtime text must be owned by proxy_core_adapter/proxy-core:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_adapter_managed_auth_planning_uses_runtime_source() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
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
        method.contains("provider_managed_account_binding_input")
            && method.contains("github_account_id.as_deref()"),
        "adapter managed-auth provider extension must project CC Switch ProviderMeta into core binding input"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(method) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_ADAPTER_MANAGED_AUTH_PLAN_RUNTIME_CALL_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs ManagedAccountRuntimeSource::resolve_auth_for_provider:{} contains direct runtime call marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "adapter managed-auth provider extension must call core runtime-source orchestration instead of host auth functions directly:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_adapter_managed_auth_runtime_source_is_trait() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");

    assert!(
        source.contains("trait ManagedAccountRuntimeSource"),
        "proxy_core_adapter must expose managed-account runtime reads behind a source trait"
    );
}

#[test]
fn production_adapter_managed_auth_tests_use_runtime_source_surface() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let test_surface = function_slice(
        &source,
        "#[cfg(test)]\npub(crate) async fn resolve_managed_account_auth_from_runtime_source",
        "pub(crate) const SESSION_REQUEST_ID_PREFIX",
    );

    assert!(
        !test_surface.contains("app_handle: Option<&tauri::AppHandle>")
            && !test_surface.contains("managed_account_runtime_source_from_app_handle(app_handle.cloned())"),
        "managed-auth adapter test helpers must use ManagedAccountRuntimeSource directly instead of AppHandle wrappers"
    );
}

#[test]
fn production_forwarder_uses_managed_auth_runtime_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forwarder_path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&forwarder_path).expect("read forwarder.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let auth_source_slice = function_slice(
        &adapter_source,
        "struct CcSwitchForwarderAuthSource",
        "impl ForwarderAuthSource for CcSwitchForwarderAuthSource",
    );
    let request_source_slice = function_slice(
        &adapter_source,
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
                    "src/proxy/forwarder.rs:{} contains managed-auth app_handle source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_FAILOVER_SWITCH_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains failover switch marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    assert!(
        source.contains("failover_switch_scheduler"),
        "RequestForwarder must receive failover switch scheduling as an injected runtime source"
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
                    "src/proxy/forwarder.rs:{} contains failover host resource marker `{}`",
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
    let forwarder_path = manifest_dir.join("src/proxy/forwarder.rs");
    let forwarder_source = fs::read_to_string(&forwarder_path).expect("read forwarder.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    assert!(
        adapter_source.contains("apply_copilot_dynamic_base_url_for_provider"),
        "ManagedAccountRuntimeSource must expose provider-aware Copilot dynamic base URL mutation"
    );
    assert!(
        adapter_source.contains(
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct Copilot dynamic endpoint marker `{}`",
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
    let forwarder_path = manifest_dir.join("src/proxy/forwarder.rs");
    let forwarder_source = fs::read_to_string(&forwarder_path).expect("read forwarder.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    assert!(
        adapter_source.contains("resolve_claude_api_format_for_adapter"),
        "ManagedAccountRuntimeSource must expose adapter-gated Claude API format resolution"
    );
    assert!(
        adapter_source.contains("resolve_core_copilot_model_vendor_for_binding_with_runtime_source("),
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct Claude API format runtime marker `{}`",
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
    let forwarder_path = manifest_dir.join("src/proxy/forwarder.rs");
    let forwarder_source = fs::read_to_string(&forwarder_path).expect("read forwarder.rs");
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct Claude body policy gate marker `{}`",
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
    let forwarder_path = manifest_dir.join("src/proxy/forwarder.rs");
    let forwarder_source = fs::read_to_string(&forwarder_path).expect("read forwarder.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    assert!(
        adapter_source.contains("apply_app_media_prevention"),
        "ForwarderRequestSource must expose app-gated media prevention"
    );

    let impl_slice = function_slice(&forwarder_source, "impl RequestForwarder", "#[cfg(test)]");
    let forbidden_markers = ["matches!(app_type, AppType::Codex)"];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct Codex media prevention gate marker `{}`",
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
    let forwarder_path = manifest_dir.join("src/proxy/forwarder.rs");
    let forwarder_source = fs::read_to_string(&forwarder_path).expect("read forwarder.rs");
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct Claude transform gate marker `{}`",
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
            && !source.contains("pub struct CopilotEndpoints")
            && !source.contains("pub struct QuotaSnapshots")
            && !source.contains("pub struct QuotaDetail"),
        "copilot_auth.rs should not own Copilot usage DTO contracts"
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
    let forwarder_path = manifest_dir.join("src/proxy/forwarder.rs");
    let forwarder_source = fs::read_to_string(&forwarder_path).expect("read forwarder.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");

    assert!(
        adapter_source.contains("apply_copilot_live_model_for_adapter"),
        "ManagedAccountRuntimeSource must expose adapter-gated Copilot live model body resolution"
    );
    assert!(
        adapter_source.contains("resolve_core_copilot_live_model_for_binding_with_runtime_source("),
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct Copilot live model marker `{}`",
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
    let forwarder_path = manifest_dir.join("src/proxy/forwarder.rs");
    let forwarder_source = fs::read_to_string(&forwarder_path).expect("read forwarder.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let struct_slice = function_slice(
        &forwarder_source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&forwarder_source, "impl RequestForwarder", "#[cfg(test)]");
    let request_source_slice = function_slice(
        &adapter_source,
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
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
        &adapter_source,
        "struct CcSwitchForwarderAuthSource",
        "impl ForwarderAuthSource for CcSwitchForwarderAuthSource",
    );
    let auth_trait_slice = function_slice(
        &adapter_source,
        "pub(crate) trait ForwarderAuthSource",
        "struct CcSwitchForwarderAuthSource",
    );
    let auth_impl_slice = function_slice(
        &adapter_source,
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
                    "src/proxy_core_adapter.rs ForwarderAuthSource impl:{} contains direct provider adapter auth marker `{}`",
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct auth assembly marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
    let struct_slice = function_slice(
        &source,
        "pub struct RequestForwarder",
        "impl RequestForwarder",
    );
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
    let runtime_trait_slice = function_slice(
        &adapter_source,
        "pub(crate) trait ForwarderRuntimeStateSource",
        "struct CcSwitchForwarderRuntimeStateSource",
    );
    let runtime_source_slice = function_slice(
        &adapter_source,
        "struct CcSwitchForwarderRuntimeStateSource",
        "impl CcSwitchForwarderRuntimeStateSource",
    );
    let runtime_inherent_impl_slice = function_slice(
        &adapter_source,
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
                    "src/proxy/forwarder.rs RequestForwarder:{} contains runtime state field marker `{}`",
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct runtime state marker `{}`",
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
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let function = function_slice(
        &source,
        "fn forwarder_rectifier_error_message",
        "pub(crate) fn forwarder_request_source_from_managed_account_runtime_source",
    );

    assert!(
        function.contains("core_forwarder_rectifier_error_message("),
        "adapter must delegate rectifier error-message selection to proxy-core"
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct request lifecycle marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    let forbidden_markers = ["emit_attempt_event_source("];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct attempt event marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
    let impl_slice = function_slice(&source, "impl RequestForwarder", "#[cfg(test)]");

    let forbidden_markers = ["record_forward_active_route_target_runtime_source("];
    let mut violations = Vec::new();

    for (line_index, line) in production_lines(impl_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in forbidden_markers {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct active-route target marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct status update marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct provider status marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
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
                    "src/proxy/forwarder.rs RequestForwarder:{} contains protocol state field marker `{}`",
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct protocol state marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
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
        &adapter_source,
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
        "struct CcSwitchForwarderAttemptRuntimeSource",
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
                    "src/proxy/forwarder.rs RequestForwarder:{} contains attempt runtime field marker `{}`",
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct attempt runtime marker `{}`",
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
    let path = manifest_dir.join("src/proxy/failover_switch.rs");
    let source = fs::read_to_string(&path).expect("read failover_switch.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FAILOVER_SWITCH_CONFIG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/failover_switch.rs:{} contains proxy config marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
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
        "super::hyper_client::send_request(",
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct transport marker `{}`",
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
fn production_forwarder_uses_request_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
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
        &adapter_source,
        "impl ForwarderRequestSource for CcSwitchForwarderRequestSource",
        "pub(crate) fn forwarder_request_source_from_managed_account_runtime_source",
    );
    let request_private_impl_slice = function_slice(
        &adapter_source,
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
        adapter_source.contains("forwarder_request_body_model("),
        "default ForwarderRequestSource implementation must retain request body model projection"
    );
    assert!(
        adapter_source.contains("fn transform_provider_request_body"),
        "default ForwarderRequestSource implementation must retain provider transform wrapping"
    );
    assert!(
        adapter_source.contains("fn convert_codex_responses_to_chat_body"),
        "default ForwarderRequestSource implementation must retain Codex Responses to Chat body conversion"
    );
    assert!(
        adapter_source.contains("fn optimize_copilot_request"),
        "default ForwarderRequestSource implementation must retain Copilot optimizer sequencing"
    );
    assert!(
        !adapter_source.contains("fn apply_media_prevention"),
        "default ForwarderRequestSource implementation must not retain a private media prevention replacement method"
    );
    assert!(
        adapter_source.contains("fn apply_forwarder_media_prevention_with_log")
            && request_impl_slice.contains("apply_forwarder_media_prevention_with_log("),
        "default ForwarderRequestSource implementation should delegate media prevention to the adapter helper"
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
        "struct CcSwitchForwarderRequestSource",
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct request assembly marker `{}`",
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
fn production_forwarder_uses_response_source_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");
    let adapter_path = manifest_dir.join("src/proxy_core_adapter.rs");
    let adapter_source = fs::read_to_string(&adapter_path).expect("read proxy_core_adapter.rs");
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
        &adapter_source,
        "impl ForwarderResponseSource for CcSwitchForwarderResponseSource",
        "async fn prime_streaming_forward_response",
    );
    assert!(
        !adapter_source.contains("fn upstream_error_body")
            && !adapter_source.contains("fn upstream_error_response"),
        "default ForwarderResponseSource implementation must not retain private upstream error projection helpers"
    );
    assert!(
        adapter_source.contains("fn prepare_success_response"),
        "default ForwarderResponseSource implementation must retain success response readiness projection"
    );
    assert!(
        response_source_impl_slice.contains("input.response.bytes().await?")
            && response_source_impl_slice.contains("ProxyError::UpstreamError { status, body }"),
        "default ForwarderResponseSource should project upstream error responses inside finalize_upstream_response"
    );
    let response_trait_slice = function_slice(
        &adapter_source,
        "pub(crate) trait ForwarderResponseSource",
        "struct CcSwitchForwarderResponseSource",
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
                    "src/proxy/forwarder.rs impl RequestForwarder:{} contains direct response-readiness marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_RUNTIME_EVENT_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains runtime event marker `{}`",
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
    let path = manifest_dir.join("src/proxy/forwarder.rs");
    let source = fs::read_to_string(&path).expect("read forwarder.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_FORWARDER_ATTEMPT_RUNTIME_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/forwarder.rs:{} contains attempt runtime marker `{}`",
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
    let forwarder_path = manifest_dir.join("src/proxy/forwarder.rs");
    let forwarder_source = fs::read_to_string(&forwarder_path).expect("read forwarder.rs");

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
    let path = manifest_dir.join("src/proxy/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let circuit_runtime =
        function_slice(&source, "    /// 热更新熔断器配置", "\n}\n\n#[cfg(test)]");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(circuit_runtime) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_CIRCUIT_RUNTIME_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/server.rs ProxyServer circuit runtime:{} contains provider router marker `{}`",
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
    let path = manifest_dir.join("src/proxy/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let constructor = function_slice(&source, "    pub fn new", "    pub async fn start");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(constructor) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_RUNTIME_ASSEMBLY_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/server.rs ProxyServer::new:{} contains runtime assembly marker `{}`",
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
    let path = manifest_dir.join("src/proxy/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_HOST_COMPAT_IMPORT_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/server.rs:{} contains host compat import marker `{}`",
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
fn production_proxy_server_delegates_runtime_state_type_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_RUNTIME_STATE_TYPE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/server.rs:{} contains runtime state type marker `{}`",
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
    let path = manifest_dir.join("src/proxy/server.rs");
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
                "src/proxy/server.rs:{} keeps host-state constructor/import in production: `{}`",
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
        "src/proxy/handler_context.rs",
        "src/proxy/handlers.rs",
        "src/proxy/response_processor.rs",
        "src/proxy/server.rs",
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
fn production_proxy_server_delegates_runtime_state_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let runtime_state = function_slice(&source, "    pub async fn start", "    fn build_router");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(runtime_state) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_SERVER_RUNTIME_STATE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/server.rs ProxyServer runtime state:{} contains runtime state marker `{}`",
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
fn production_provider_router_delegates_channel_route_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CHANNEL_ROUTE_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains channel route source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_SELECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains provider selection marker `{}`",
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_PROVIDER_RECORD_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains provider record marker `{}`",
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_FAILOVER_CONFIG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains failover config marker `{}`",
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CIRCUIT_CONFIG_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains circuit config marker `{}`",
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
fn production_provider_router_delegates_route_rejection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_ROUTE_REJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains route rejection marker `{}`",
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_MANAGEMENT_ROUTE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains management route marker `{}`",
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_HEALTH_PERSISTENCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains health persistence marker `{}`",
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CONCRETE_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains concrete router source marker `{}`",
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_COARSE_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains coarse router source marker `{}`",
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
fn production_provider_router_uses_route_channel_inputs() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CHANNEL_DAO_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs:{} contains channel DAO marker `{}`",
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
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let adapter_slice = function_slice(
        &source,
        "struct CcSwitchProviderRouterConfigSource",
        "struct CcSwitchProviderRouterProviderSource",
    );

    assert!(
        adapter_slice.contains("source: CcSwitchConfigSource"),
        "ProviderRouter config source adapter must hold the core-facing CcSwitchConfigSource"
    );
    assert!(
        adapter_slice.contains("auto_failover_enabled_from_router_config_source")
            && adapter_slice.contains("circuit_breaker_config_from_router_config_source")
            && adapter_slice.contains("circuit_failure_threshold_from_router_config_source"),
        "ProviderRouter config source adapter must project router config from ProxyConfigSource"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(adapter_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CONFIG_SOURCE_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs CcSwitchProviderRouterConfigSource:{} contains config source adapter marker `{}`",
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
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let adapter_slice = function_slice(
        &source,
        "struct CcSwitchProviderRouterProviderSource",
        "struct CcSwitchProviderRouterChannelSource",
    );

    assert!(
        adapter_slice.contains("impl ProviderSource for CcSwitchProviderRouterProviderSource"),
        "ProviderRouter provider source adapter must expose a core-facing ProviderSource"
    );
    assert!(
        adapter_slice.contains("route_policies: CcSwitchRoutePolicySource")
            && adapter_slice.contains("failover_provider_ids_from_route_policy_source"),
        "ProviderRouter provider source adapter must read failover queue facts through RoutePolicySource"
    );
    assert!(
        adapter_slice.contains("provider_ids_from_router_provider_source")
            && adapter_slice.contains("select_current_provider_ids_from_router_provider_source"),
        "ProviderRouter provider source adapter must project provider ids through ProviderSource helpers"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(adapter_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_PROVIDER_SOURCE_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs CcSwitchProviderRouterProviderSource:{} contains provider source adapter marker `{}`",
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
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let adapter_slice = function_slice(
        &source,
        "struct CcSwitchProviderRouterChannelSource",
        "struct CcSwitchProviderRouterHealthStore",
    );

    assert!(
        adapter_slice.contains("source: CcSwitchChannelSource"),
        "ProviderRouter channel source adapter must hold the core-facing CcSwitchChannelSource"
    );
    assert!(
        adapter_slice.contains("router_channel_route_inputs_from_channel_source"),
        "ProviderRouter channel source adapter must project route inputs from ChannelSource"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(adapter_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_CHANNEL_SOURCE_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs CcSwitchProviderRouterChannelSource:{} contains channel source adapter marker `{}`",
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
fn production_provider_router_health_store_uses_core_attempt_facts() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_adapter.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_adapter.rs");
    let adapter_slice = function_slice(
        &source,
        "struct CcSwitchProviderRouterHealthStore",
        "pub(crate) fn current_provider_id_from_router_sources",
    );

    assert!(
        adapter_slice.contains("impl ProviderHealthStore for CcSwitchProviderRouterHealthStore")
            && adapter_slice.contains("ProviderAttemptResult")
            && adapter_slice.contains("record_provider_attempt_in_db_source"),
        "ProviderRouter health store adapter must write provider health through core ProviderHealthStore attempt projection"
    );
    assert!(
        adapter_slice.contains("fn record_channel_health<'a>(")
            && adapter_slice.contains("result: ChannelAttemptResult")
            && adapter_slice.contains("BoxFuture<'a, Result<(), AppError>>")
            && adapter_slice.contains("record_channel_health_attempt_from_router_db(&self.db, result)"),
        "ProviderRouter health store adapter must write channel health through core ChannelAttemptResult projection"
    );
    assert!(
        adapter_slice.contains("fn reset_channel_health(&self, reset: ChannelHealthReset)")
            && adapter_slice.contains("reset_channel_health_from_router_db(&self.db, reset)"),
        "ProviderRouter health store adapter must reset channel health through core ChannelHealthReset facts"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(adapter_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_HEALTH_STORE_ADAPTER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_adapter.rs CcSwitchProviderRouterHealthStore:{} contains health store adapter marker `{}`",
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");
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
fn production_provider_router_records_channel_health_with_core_attempt_fact() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");
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
    let path = manifest_dir.join("src/proxy/provider_router.rs");
    let source = fs::read_to_string(&path).expect("read provider_router.rs");
    let struct_slice = function_slice(&source, "pub struct ProviderRouter", "impl ProviderRouter");

    assert!(
        struct_slice.contains("circuit_runtime"),
        "ProviderRouter must hold live circuit state through a runtime object"
    );
    assert!(
        source.contains("struct ProviderRoutingCircuitRuntime"),
        "provider_router.rs must keep live circuit map in ProviderRoutingCircuitRuntime"
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(struct_slice) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROVIDER_ROUTER_LIVE_CIRCUIT_MAP_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/provider_router.rs ProviderRouter:{} contains live circuit map marker `{}`",
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
