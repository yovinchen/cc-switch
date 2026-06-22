use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED_PROXY_CORE_FILES: &[&str] = &["src/lib.rs", "src/proxy_core_adapter.rs"];
const ALLOWED_PROXY_ENGINE_CONSTRUCTOR_FILES: &[&str] = &["src/proxy_core_adapter.rs"];

const FORBIDDEN_MARKERS: &[&str] = &["crate::proxy_core::", "cc_switch_proxy_core::"];
const FORBIDDEN_FORWARDER_SELF_PLANNING_MARKERS: &[&str] = &[
    "RequestForwarder::new(",
    ".forward_with_retry(",
    "build_forward_attempts(",
    "create_forwarder(",
];
const FORBIDDEN_REQUEST_CONTEXT_PROVIDER_PRESELECT_MARKERS: &[&str] =
    &["provider_router", ".select_providers("];
const FORBIDDEN_REQUEST_CONTEXT_PROVIDER_ADAPTER_MARKERS: &[&str] =
    &["providers::", "get_claude_api_format("];
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
const FORBIDDEN_FORWARDER_FAILOVER_SWITCH_MARKERS: &[&str] = &[".try_switch("];
const FORBIDDEN_PROXY_CORE_HOST_ERROR_MARKERS: &[&str] = &["ProxyCoreError::"];
const FORBIDDEN_PROXY_CORE_HOST_USAGE_PROJECTION_MARKERS: &[&str] =
    &["missing_pricing_warning_message"];
const FORBIDDEN_PROXY_CORE_HOST_APP_SUMMARY_PROJECTION_MARKERS: &[&str] =
    &["AppSummaryConfig::new("];
const FORBIDDEN_PROXY_CORE_HOST_CONFIG_SOURCE_MARKERS: &[&str] = &[
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
    ".get_all_providers(",
    ".get_provider_by_id(",
    ".get_current_provider(",
    "provider_specs_from_source(",
    "provider_spec_from_source(",
];
const FORBIDDEN_PROXY_CORE_HOST_CHANNEL_SPEC_SOURCE_MARKERS: &[&str] = &[
    ".list_channels_for_app(",
    ".list_proxy_channels_for_app(",
    ".get_proxy_channel(",
    "channel_specs_from_source(",
    "channel_spec_from_source(",
];
const FORBIDDEN_PROXY_CORE_HOST_CHANNEL_RECORD_SOURCE_MARKERS: &[&str] = &[
    ".create_proxy_channel(",
    ".get_proxy_channel(",
    ".update_proxy_channel(",
    ".delete_proxy_channel(",
    "proxy_channel_record_to_core(",
];
const FORBIDDEN_PROXY_CORE_HOST_CHANNEL_KEY_MODEL_SOURCE_MARKERS: &[&str] = &[
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
    ".list_channels_for_app(",
    ".list_proxy_channels_for_app(",
    ".list_all_proxy_channels(",
    "proxy_channel_records_to_core(",
];
const FORBIDDEN_PROXY_CORE_HOST_CHANNEL_MIGRATION_SOURCE_MARKERS: &[&str] = &[
    ".preview_legacy_proxy_channel_migration(",
    ".materialize_legacy_proxy_channels(",
    "channel_migration_preview_input_from_result(",
    "channel_migration_materialize_input_from_result(",
];
const FORBIDDEN_PROXY_CORE_HOST_ROUTE_POLICY_SOURCE_MARKERS: &[&str] = &[
    ".get_failover_queue(",
    "route_policy_from_source(",
    "route_policy_from_failover_queue(",
];
const FORBIDDEN_PROXY_CORE_HOST_HEALTH_STORE_SOURCE_MARKERS: &[&str] = &[
    ".update_proxy_channel_health_with_threshold(",
    ".get_proxy_channel_app_type(",
    ".reset_channel_breaker(",
    "channel_health_attempt_db_update(",
    "channel_health_reset_plan_from_lookup(",
    "channel_health_reset_from_plan(",
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
const FORBIDDEN_PROXY_CORE_HOST_FORWARDER_LAUNCH_MARKERS: &[&str] = &[
    "RequestForwarder::new_preplanned(",
    ".forward_with_preplanned_attempts(",
    "forward_runtime_request_from_proxy_request(",
    "forward_with_preplanned_host_runtime(",
    "forward_error_to_core_error(",
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
];
const FORBIDDEN_PROVIDER_ROUTER_SELECTION_MARKERS: &[&str] = &[
    "ProviderSelectionInput::",
    "select_provider_ids(",
    "provider_selection_candidate_from_failover_lookup(",
    "provider_failover_circuit_lookups(",
];
const FORBIDDEN_PROVIDER_ROUTER_FAILOVER_CONFIG_MARKERS: &[&str] =
    &[".auto_failover_enabled", "默认禁用故障转移"];
const FORBIDDEN_PROVIDER_ROUTER_CIRCUIT_CONFIG_MARKERS: &[&str] = &[
    "circuit_breaker_config_from_app_config(",
    "circuit_failure_threshold_from_app_config(",
    "get_proxy_config_for_app(app_type).await.ok()",
];
const FORBIDDEN_PROVIDER_ROUTER_ROUTE_REJECTION_MARKERS: &[&str] =
    &["reject_unavailable_channel_ids(", "unavailable_channel_ids"];
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
const FORBIDDEN_HANDLER_MANAGEMENT_AUTH_DECISION_MARKERS: &[&str] = &[
    "std::env::var(",
    "CC_SWITCH_PROXY_MANAGEMENT_TOKEN",
    "resolve_management_auth_decision(",
];
const FORBIDDEN_RESPONSE_PROCESSOR_USAGE_PROVIDER_PROJECTION_MARKERS: &[&str] = &[
    "provider_kind_from_provider(",
    "AppKind::from(",
    "streaming_response_usage_record_with_optional_outbound_model(",
    "non_streaming_response_usage_record_from_body_with_request_id_fallback(",
];
const FORBIDDEN_USAGE_SINK_PROVIDER_PROJECTION_MARKERS: &[&str] = &[
    "provider_kind_from_provider(",
    "AppKind::from(",
    "error_usage_record_with_request_id_fallback(",
    "transformed_response_usage_record_with_request_id_fallback(",
    "transformed_streaming_response_usage_record_with_request_id_fallback(",
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
        "host code must access proxy-core through src/proxy_core_adapter.rs:\n{}",
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
fn usage_sink_bridge_delegates_provider_projection_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy/usage_sink_bridge.rs");
    let source = fs::read_to_string(&path).expect("read usage_sink_bridge.rs");

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(&source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_USAGE_SINK_PROVIDER_PROJECTION_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy/usage_sink_bridge.rs:{} contains usage provider projection marker `{}`",
                    line_index + 1,
                    marker
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "usage sink bridge must build provider usage facts through proxy_core_adapter helpers:\n{}",
        violations.join("\n")
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
    let config_source = function_slice(
        &source,
        "impl ProxyConfigSource for CcSwitchConfigSource",
        "#[derive(Clone)]\nstruct CcSwitchProviderSource",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(config_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CONFIG_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchConfigSource:{} contains config source marker `{}`",
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
    let provider_source = function_slice(
        &source,
        "impl ProviderSource for CcSwitchProviderSource",
        "#[derive(Clone)]\nstruct CcSwitchChannelSource",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(provider_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_PROVIDER_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchProviderSource:{} contains provider source marker `{}`",
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
    let channel_spec_source = function_slice(
        &source,
        "impl ChannelSource for CcSwitchChannelSource",
        "    fn create_channel_record",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(channel_spec_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_SPEC_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchChannelSource:{} contains channel spec source marker `{}`",
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
    let channel_record_source = function_slice(
        &source,
        "    fn create_channel_record",
        "    fn list_channel_key_records",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(channel_record_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_RECORD_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchChannelSource records:{} contains channel record source marker `{}`",
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
    let channel_key_model_source = function_slice(
        &source,
        "    fn list_channel_key_records",
        "    fn list_channel_records",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(channel_key_model_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_KEY_MODEL_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchChannelSource key/model:{} contains channel key/model source marker `{}`",
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
    let channel_record_list_source = function_slice(
        &source,
        "    fn list_channel_records",
        "    fn preview_legacy_channel_migration",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(channel_record_list_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_RECORD_LIST_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchChannelSource record-list:{} contains channel record list source marker `{}`",
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
    let channel_migration_source = function_slice(
        &source,
        "    fn preview_legacy_channel_migration",
        "\n}\n\n#[derive(Clone)]\nstruct CcSwitchRoutePolicySource",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(channel_migration_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_CHANNEL_MIGRATION_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchChannelSource migration:{} contains channel migration source marker `{}`",
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
    let route_policy_source = function_slice(
        &source,
        "impl RoutePolicySource for CcSwitchRoutePolicySource",
        "#[derive(Clone)]\nstruct CcSwitchRouteResolver",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(route_policy_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_ROUTE_POLICY_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchRoutePolicySource:{} contains route policy source marker `{}`",
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
fn production_proxy_core_host_delegates_health_store_sources_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");
    let health_store = function_slice(
        &source,
        "impl ChannelHealthStore for CcSwitchHealthStore",
        "#[derive(Clone)]\nstruct CcSwitchChannelReachabilityProbe",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(health_store) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_HEALTH_STORE_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchHealthStore:{} contains health store source marker `{}`",
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
fn production_proxy_core_host_delegates_forward_current_provider_source_to_adapter() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/proxy_core_host.rs");
    let source = fs::read_to_string(&path).expect("read proxy_core_host.rs");
    let forward_source = function_slice(
        &source,
        "    async fn forward(",
        "\n}\n\n#[cfg(test)]",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(forward_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_CURRENT_PROVIDER_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchProxyRuntime::forward:{} contains current-provider source marker `{}`",
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
        "    async fn forward(",
        "\n}\n\n#[cfg(test)]",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(forward_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_CONFIG_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchProxyRuntime::forward:{} contains forward runtime config source marker `{}`",
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
        "    async fn forward(",
        "\n}\n\n#[cfg(test)]",
    );

    let mut violations = Vec::new();
    for (line_index, line) in production_lines(forward_source) {
        let code = line.split("//").next().unwrap_or_default();
        for marker in FORBIDDEN_PROXY_CORE_HOST_FORWARD_ATTEMPT_SOURCE_MARKERS {
            if code.contains(marker) {
                violations.push(format!(
                    "src/proxy_core_host.rs CcSwitchProxyRuntime::forward:{} contains forward attempt source marker `{}`",
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
