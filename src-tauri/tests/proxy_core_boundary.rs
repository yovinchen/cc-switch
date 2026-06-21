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
