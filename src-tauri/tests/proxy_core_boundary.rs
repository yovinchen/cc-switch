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
const FORBIDDEN_REQUEST_CONTEXT_PROVIDER_PRESELECT_MARKERS: &[&str] =
    &["provider_router", ".select_providers(", ".select_provider_ids("];
const FORBIDDEN_REQUEST_CONTEXT_PROVIDER_ADAPTER_MARKERS: &[&str] =
    &[
        "providers::",
        "get_claude_api_format(",
        "AppKind::from(",
        "selected_route.provider",
        "selected_provider_missing_from_source_message(",
        "request_context_route_update_from_proxy_result(",
    ];
const FORBIDDEN_PROXY_ERROR_MAPPER_CODEX_PROJECTION_MARKERS: &[&str] =
    &[
        "CodexProxyErrorContext",
        "CodexProxyHostErrorFacts",
        "CodexProxyErrorKind",
        "codex_proxy_error_code(",
        "codex_proxy_error_facts(",
        "codex_proxy_error_kind(",
    ];
const FORBIDDEN_PROXY_ERROR_MAPPER_FORWARD_FAILURE_PROJECTION_MARKERS: &[&str] =
    &[
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
    "super::providers::get_claude_api_format(",
    "super::providers::normalize_anthropic_messages_for_provider(",
    "super::providers::transform_claude_request_for_api_format(",
];
const FORBIDDEN_FORWARDER_CODEX_PROVIDER_COMPAT_MARKERS: &[&str] = &[
    "super::providers::should_convert_codex_responses_to_chat(",
    "provider_should_convert_codex_responses_to_chat(",
    "super::providers::apply_codex_chat_upstream_model(",
    "super::providers::resolve_codex_chat_reasoning_options(",
];
const FORBIDDEN_FORWARDER_CHANNEL_STATUS_MAPPING_MARKERS: &[&str] = &[
    "mapped_channel_response_status(",
    "invalid_mapped_channel_response_status_message(",
    "StatusCode::from_u16(",
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
const FORBIDDEN_FORWARDER_MANAGED_AUTH_MARKERS: &[&str] = &[
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
const FORBIDDEN_PROXY_CORE_ADAPTER_PROVIDER_COPILOT_MARKERS: &[&str] =
    &["providers::copilot_auth::COPILOT_", "copilot_auth::COPILOT_"];
const FORBIDDEN_PROXY_CORE_ADAPTER_MODEL_FETCH_FACADE_MARKERS: &[&str] = &[
    "FetchedModel",
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
const FORBIDDEN_PROXY_CORE_HOST_AUTH_PROFILE_DB_MARKERS: &[&str] = &[
    "fn apply_channel_auth_profile_providers(",
    "get_enabled_proxy_channel_key(",
    "channel_key_value_from_record(",
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
    "forward_with_preplanned_host_runtime(",
    "forward_error_to_core_error(",
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
const FORBIDDEN_PROVIDER_ROUTER_FAILOVER_CONFIG_MARKERS: &[&str] =
    &[".auto_failover_enabled", "默认禁用故障转移", ".get_proxy_config_for_app("];
const FORBIDDEN_PROVIDER_ROUTER_CIRCUIT_CONFIG_MARKERS: &[&str] = &[
    "circuit_breaker_config_from_app_config(",
    "circuit_failure_threshold_from_app_config(",
    "get_proxy_config_for_app(app_type).await.ok()",
    ".get_proxy_config_for_app(",
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
const FORBIDDEN_PROVIDER_ROUTER_COARSE_SOURCE_MARKERS: &[&str] =
    &["trait ProviderRouterSource", "dyn ProviderRouterSource", "with_source("];
const FORBIDDEN_PROVIDER_ROUTER_CHANNEL_DAO_MARKERS: &[&str] = &[
    "ProxyChannelRecord",
    "ProxyChannelSourceKind",
    "ProviderRouterChannelRecord",
    "ProviderRouterChannelModelRecord",
    "channel_route_records(",
];
const FORBIDDEN_PROVIDER_ROUTER_PROVIDER_RECORD_MARKERS: &[&str] = &[
    "use crate::provider::Provider",
    "IndexMap<String, Provider>",
    "Result<Vec<Provider>",
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
const FORBIDDEN_PROTOCOL_HANDLER_FORWARD_CORE_ERROR_MARKERS: &[&str] =
    &["proxy_core_error_to_proxy_error(error)", "record_forward_error_usage("];
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
    "fn request_model_from_body_for_context(",
    "fn request_model_from_gemini_path_for_context(",
    "fn claude_api_format_from_metadata(",
    "fn extract_gemini_model_from_path(",
    "fn validate_claude_desktop_gateway_bearer_header(",
    "fn proxy_event_envelope_to_sse_spec(",
    "fn apply_route_candidate_circuit_availability(",
    "fn route_candidate_channel_circuit_keys(",
    "fn reject_unavailable_channel_ids(",
    "fn route_plan_from_request(",
    "fn route_plan_provider_ids(",
    "fn route_plan_provider_match(",
    "fn forwarding_requires_runtime_error_message(",
    "fn route_plan_no_matching_host_providers_error_message(",
    "fn route_plan_providers_unconfigured_error_message(",
    "fn route_plan_selections(",
    "fn route_selection_for_forward_result(",
    "fn resolve_channel_route(",
    "fn channel_route_candidate_from_selection(",
    "fn resolved_channel_attempt_from_candidate(",
    "fn resolved_channel_attempt_from_selection(",
    "fn forward_failure_kind_from_proxy_status(",
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
    "fn build_codex_bearer_auth_headers(",
    "fn build_gemini_auth_headers(",
    "fn build_claude_auth_headers(",
    "fn build_copilot_auth_headers(",
    "fn parse_copilot_models_response_bytes(",
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
const FORBIDDEN_HTTP_CLIENT_PROXY_URL_VALIDATION_MARKERS: &[&str] = &[
    "url::Url::parse(",
    "[\"http\", \"https\", \"socks5\", \"socks5h\"]",
    "Invalid proxy scheme",
    "Invalid proxy URL '",
];
const FORBIDDEN_PROVIDER_ENDPOINT_SERVICE_PROJECTION_MARKERS: &[&str] = &[
    ".custom_endpoints",
    "trim().trim_end_matches('/')",
    "std::cmp::Reverse(",
    ".last_used = Some(",
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
    "std::env::var(",
    "CC_SWITCH_PROXY_MANAGEMENT_TOKEN",
    "resolve_management_auth_decision(",
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
fn basic_health_status_handlers_use_direct_management_responses() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/handlers.rs");
    let source = fs::read_to_string(&path).expect("read handlers.rs");
    let handlers = [
        (
            "health_check",
            function_slice(&source, "pub async fn health_check", "/// 获取服务状态"),
        ),
        (
            "get_status",
            function_slice(
                &source,
                "pub async fn get_status",
                "/// GET /proxy/v1/events",
            ),
        ),
    ];
    let forbidden_markers = [
        "health_check_source_from_timestamp",
        "proxy_status_source_from_status",
        ".response_from_source(",
    ];

    let mut violations = Vec::new();
    for (handler_name, handler) in handlers {
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
        "basic health/status HTTP handlers must use direct management responses:\n{}",
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
        let source = fs::read_to_string(manifest_dir.join(relative)).expect("read provider adapter");
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
        let get_auth_headers = function_slice(
            &source,
            "    fn get_auth_headers(",
            "\n}\n\n#[cfg(test)]",
        );
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
    let proxy_paths = ["src/proxy/forwarder.rs", "src/proxy/handlers.rs", "src/proxy/server.rs"];

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
        "production forwarder must delegate managed account token resolution to proxy::managed_account_auth:\n{}",
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
fn production_proxy_core_host_delegates_forward_current_provider_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");
    let forward_source = function_slice(
        &source,
        "impl HostForwardRuntime for CcSwitchProxyRuntime",
        "impl CcSwitchProxyRuntime",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(forward_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_CURRENT_PROVIDER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchProxyRuntime::forward_host:{} contains current-provider source marker `{}`",
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
    let forward_source = function_slice(
        &source,
        "impl HostForwardRuntime for CcSwitchProxyRuntime",
        "impl CcSwitchProxyRuntime",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(forward_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_CONFIG_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchProxyRuntime::forward_host:{} contains forward runtime config source marker `{}`",
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
fn production_proxy_core_host_delegates_forward_attempt_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");
    let forward_source = function_slice(
        &source,
        "impl HostForwardRuntime for CcSwitchProxyRuntime",
        "impl CcSwitchProxyRuntime",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(forward_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_ATTEMPT_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchProxyRuntime::forward_host:{} contains forward attempt source marker `{}`",
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
fn production_proxy_server_delegates_circuit_runtime_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let circuit_runtime = function_slice(
        &source,
        "    /// 热更新熔断器配置",
        "\n}\n\n#[cfg(test)]",
    );

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
fn production_proxy_server_delegates_runtime_state_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/server.rs");
    let source = fs::read_to_string(&path).expect("read server.rs");
    let runtime_state = function_slice(
        &source,
        "    pub async fn start",
        "    fn build_router",
    );

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
