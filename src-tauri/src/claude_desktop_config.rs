use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "macos", windows))]
use crate::config::get_home_dir;
use crate::config::{atomic_write, delete_file, read_json_file, write_json_file};
use crate::database::Database;
use crate::database::CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID;
use crate::error::AppError;
use crate::provider::{ClaudeDesktopMode, Provider};
use crate::proxy_core_adapter::{
    provider_claude_desktop_direct_validation_issue,
    provider_claude_desktop_proxy_config_validation_issue,
    provider_claude_desktop_proxy_has_base_url_and_key, ClaudeDesktopDirectGatewayCredentialIssue,
    ClaudeDesktopDirectModelRouteIssue, ClaudeDesktopDirectProviderValidationIssue,
    ClaudeDesktopProxyProviderConfigValidationIssue, ClaudeDesktopProxyRequestBodyIssue,
};

pub const PROFILE_ID: &str = "00000000-0000-4000-8000-000000157210";
pub const PROFILE_NAME: &str = "CC Switch";

#[cfg(any(target_os = "macos", windows, test))]
const CONFIG_FILE: &str = "claude_desktop_config.json";
#[cfg(any(target_os = "macos", windows, test))]
const CONFIG_LIBRARY_DIR: &str = "configLibrary";
const GATEWAY_TOKEN_SETTING_KEY: &str = "claude_desktop_gateway_token";

/// Claude Code env 中通过 `[1M]` 后缀声明 1M 上下文能力（匹配用 `eq_ignore_ascii_case`）。
/// Claude Desktop schema 不接受此后缀，import 边界翻译为 `supports1m` 字段。
pub const ONE_M_CONTEXT_MARKER: &str = "[1m]";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopDefaultRoute {
    pub route_id: &'static str,
    pub env_key: &'static str,
    #[serde(rename = "supports1m")]
    pub supports_1m: bool,
}

#[derive(Debug, Clone)]
struct ClaudeDesktopPaths {
    normal_config_path: PathBuf,
    threep_config_path: PathBuf,
    config_library_path: PathBuf,
    profile_path: PathBuf,
    meta_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectGatewayCredentials {
    pub base_url: String,
    pub api_key: String,
}

impl From<crate::proxy_core_adapter::ClaudeDesktopDirectGatewayCredentials>
    for DirectGatewayCredentials
{
    fn from(credentials: crate::proxy_core_adapter::ClaudeDesktopDirectGatewayCredentials) -> Self {
        Self {
            base_url: credentials.base_url,
            api_key: credentials.api_key,
        }
    }
}

#[derive(Debug, Clone)]
struct FileSnapshot {
    path: PathBuf,
    content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopStatus {
    pub supported: bool,
    pub configured: bool,
    pub applied_id: Option<String>,
    pub profile_path: Option<String>,
    pub config_library_path: Option<String>,
    pub mode: Option<ClaudeDesktopMode>,
    pub expected_base_url: Option<String>,
    pub actual_base_url: Option<String>,
    pub proxy_running: bool,
    pub stale_raw_models: bool,
    pub missing_route_mappings: bool,
    pub gateway_token_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModelRoute {
    pub route_id: String,
    pub upstream_model: String,
    pub label_override: Option<String>,
    pub supports_1m: bool,
}

impl From<crate::proxy_core_adapter::ClaudeDesktopResolvedProxyRoute> for ResolvedModelRoute {
    fn from(route: crate::proxy_core_adapter::ClaudeDesktopResolvedProxyRoute) -> Self {
        Self {
            route_id: route.route_id,
            upstream_model: route.upstream_model,
            label_override: route.label_override,
            supports_1m: route.supports_1m,
        }
    }
}

pub fn apply_provider(db: &Database, provider: &Provider) -> Result<(), AppError> {
    let paths = current_platform_paths()?;
    apply_provider_to_paths(db, provider, &paths)
}

pub fn get_status(db: &Database, proxy_running: bool) -> Result<ClaudeDesktopStatus, AppError> {
    if !is_supported_platform() {
        return Ok(ClaudeDesktopStatus {
            supported: false,
            configured: false,
            applied_id: None,
            profile_path: None,
            config_library_path: None,
            mode: None,
            expected_base_url: None,
            actual_base_url: None,
            proxy_running,
            stale_raw_models: false,
            missing_route_mappings: false,
            gateway_token_configured: false,
        });
    }

    let paths = current_platform_paths()?;
    let applied_id = read_applied_id(&paths.meta_path);
    let configured = paths.profile_path.exists() || meta_has_profile_entry(&paths.meta_path);
    let profile = read_json_or_empty(&paths.profile_path).unwrap_or_else(|_| json!({}));
    let actual_base_url =
        crate::proxy_core_adapter::claude_desktop_profile_gateway_base_url(&profile);
    let stale_raw_models =
        crate::proxy_core_adapter::claude_desktop_profile_has_unsafe_model_ids(&profile);
    let gateway_token_configured = db
        .get_setting(GATEWAY_TOKEN_SETTING_KEY)
        .ok()
        .flatten()
        .is_some_and(|token| !token.trim().is_empty());
    let current_provider = crate::settings::get_effective_current_provider(
        db,
        &crate::app_config::AppType::ClaudeDesktop,
    )
    .ok()
    .flatten()
    .and_then(|id| db.get_provider_by_id(&id, "claude-desktop").ok().flatten());
    let mode = current_provider.as_ref().map(provider_mode);
    let expected_base_url = match mode {
        Some(ClaudeDesktopMode::Proxy) => proxy_gateway_base_url_from_db(db).ok(),
        Some(ClaudeDesktopMode::Direct) => current_provider
            .as_ref()
            .and_then(|provider| direct_gateway_credentials(provider).ok())
            .map(|credentials| credentials.base_url),
        None => None,
    };
    let missing_route_mappings = current_provider.as_ref().is_some_and(|provider| {
        matches!(provider_mode(provider), ClaudeDesktopMode::Proxy)
            && proxy_model_routes(provider).is_err()
    });

    Ok(ClaudeDesktopStatus {
        supported: true,
        configured,
        applied_id,
        profile_path: Some(paths.profile_path.display().to_string()),
        config_library_path: Some(paths.config_library_path.display().to_string()),
        mode,
        expected_base_url,
        actual_base_url,
        proxy_running,
        stale_raw_models,
        missing_route_mappings,
        gateway_token_configured,
    })
}

pub fn get_config_library_path() -> Result<PathBuf, AppError> {
    Ok(current_platform_paths()?.config_library_path)
}

pub fn default_proxy_routes() -> Vec<ClaudeDesktopDefaultRoute> {
    crate::proxy_core_adapter::claude_desktop_default_proxy_routes()
        .iter()
        .map(|route| ClaudeDesktopDefaultRoute {
            route_id: route.route_id,
            env_key: route.env_key,
            supports_1m: route.supports_1m,
        })
        .collect()
}

#[cfg(test)]
pub fn is_compatible_direct_provider(provider: &Provider) -> bool {
    validate_direct_provider(provider).is_ok()
}

pub fn is_official_provider(provider: &Provider) -> bool {
    provider.id == CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID
}

pub fn provider_mode(provider: &Provider) -> ClaudeDesktopMode {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.claude_desktop_mode.clone())
        .unwrap_or(ClaudeDesktopMode::Direct)
}

pub fn get_or_create_gateway_token(db: &Database) -> Result<String, AppError> {
    if let Some(token) = db.get_setting(GATEWAY_TOKEN_SETTING_KEY)? {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let token = format!("ccs-{}", uuid::Uuid::new_v4().simple());
    db.set_setting(GATEWAY_TOKEN_SETTING_KEY, &token)?;
    Ok(token)
}

fn direct_gateway_credential_issue_to_error(
    issue: ClaudeDesktopDirectGatewayCredentialIssue,
) -> AppError {
    match issue {
        ClaudeDesktopDirectGatewayCredentialIssue::EnvMissing => AppError::localized(
            "claude_desktop.provider.env_missing",
            "Claude Desktop 直连供应商缺少 env 配置",
            "Claude Desktop direct provider is missing env configuration",
        ),
        ClaudeDesktopDirectGatewayCredentialIssue::BaseUrlMissing => AppError::localized(
            "claude_desktop.provider.base_url_missing",
            "Claude Desktop 直连供应商缺少 ANTHROPIC_BASE_URL",
            "Claude Desktop direct provider is missing ANTHROPIC_BASE_URL",
        ),
        ClaudeDesktopDirectGatewayCredentialIssue::AuthTokenMissing => AppError::localized(
            "claude_desktop.provider.auth_token_missing",
            "Claude Desktop 直连供应商缺少 ANTHROPIC_AUTH_TOKEN（Bearer Token）",
            "Claude Desktop direct provider is missing ANTHROPIC_AUTH_TOKEN (Bearer Token)",
        ),
    }
}

pub fn direct_gateway_credentials(
    provider: &Provider,
) -> Result<DirectGatewayCredentials, AppError> {
    crate::proxy_core_adapter::claude_desktop_direct_gateway_credentials(&provider.settings_config)
        .map(DirectGatewayCredentials::from)
        .map_err(direct_gateway_credential_issue_to_error)
}

pub fn validate_direct_provider(provider: &Provider) -> Result<(), AppError> {
    if is_official_provider(provider) {
        return Ok(());
    }

    if let Some(issue) = provider_claude_desktop_direct_validation_issue(provider) {
        return Err(direct_validation_issue_to_error(issue));
    }

    direct_inference_model_specs(provider)?;
    direct_gateway_credentials(provider)?;
    Ok(())
}

pub fn validate_proxy_provider(provider: &Provider) -> Result<(), AppError> {
    if is_official_provider(provider) {
        return Ok(());
    }

    if let Some(issue) = provider_claude_desktop_proxy_config_validation_issue(provider) {
        return Err(proxy_config_validation_issue_to_error(issue));
    }

    proxy_model_routes(provider)?;

    if !provider_claude_desktop_proxy_has_base_url_and_key(provider) {
        return Err(AppError::localized(
            "claude_desktop.provider.credentials_missing",
            "Claude Desktop 本地路由供应商缺少 Base URL 或 API Key",
            "Claude Desktop proxy provider is missing Base URL or API key",
        ));
    }

    Ok(())
}

fn direct_validation_issue_to_error(issue: ClaudeDesktopDirectProviderValidationIssue) -> AppError {
    match issue {
        ClaudeDesktopDirectProviderValidationIssue::SettingsNotObject => AppError::localized(
            "claude_desktop.provider.settings_not_object",
            "Claude Desktop 直连供应商配置必须是 JSON 对象",
            "Claude Desktop direct provider configuration must be a JSON object",
        ),
        ClaudeDesktopDirectProviderValidationIssue::ApiFormatUnsupported => AppError::localized(
            "claude_desktop.provider.api_format_unsupported",
            "Claude Desktop 第一阶段只支持原生 Anthropic Messages API",
            "Claude Desktop phase 1 only supports native Anthropic Messages API",
        ),
        ClaudeDesktopDirectProviderValidationIssue::ProxyModeUnsupported => AppError::localized(
            "claude_desktop.provider.mode_unsupported",
            "该供应商是 Claude Desktop 本地路由模式，不能按直连模式写入",
            "This Claude Desktop provider uses proxy mode and cannot be written as direct mode",
        ),
        ClaudeDesktopDirectProviderValidationIssue::ManagedProviderTypeUnsupported => {
            AppError::localized(
                "claude_desktop.provider.type_unsupported",
                "Claude Desktop 直连模式不支持需要本地代理转换的供应商",
                "Claude Desktop direct mode does not support providers that require local proxy conversion",
            )
        }
        ClaudeDesktopDirectProviderValidationIssue::FullUrlUnsupported => AppError::localized(
            "claude_desktop.provider.full_url_unsupported",
            "Claude Desktop 直连模式不支持完整 URL 端点配置",
            "Claude Desktop direct mode does not support full URL endpoint configuration",
        ),
    }
}

fn proxy_config_validation_issue_to_error(
    issue: ClaudeDesktopProxyProviderConfigValidationIssue,
) -> AppError {
    match issue {
        ClaudeDesktopProxyProviderConfigValidationIssue::SettingsNotObject => AppError::localized(
            "claude_desktop.provider.settings_not_object",
            "Claude Desktop 本地路由供应商配置必须是 JSON 对象",
            "Claude Desktop proxy provider configuration must be a JSON object",
        ),
        ClaudeDesktopProxyProviderConfigValidationIssue::ApiFormatUnsupported(api_format) => {
            AppError::localized(
                "claude_desktop.provider.api_format_unsupported",
                format!("Claude Desktop 本地路由模式不支持 API 格式: {api_format}"),
                format!("Claude Desktop proxy mode does not support API format: {api_format}"),
            )
        }
    }
}

fn direct_model_route_issue_to_error(issue: ClaudeDesktopDirectModelRouteIssue) -> AppError {
    match issue {
        ClaudeDesktopDirectModelRouteIssue::InvalidRouteId { route_id } => AppError::localized(
            "claude_desktop.provider.route_invalid",
            format!(
                "Claude Desktop 直连模型必须使用 claude-* 或 anthropic/claude-* 名称: {route_id}"
            ),
            format!(
                "Claude Desktop direct model must use a claude-* or anthropic/claude-* name: {route_id}"
            ),
        ),
        ClaudeDesktopDirectModelRouteIssue::MappingUnsupported {
            route_id,
            upstream_model,
        } => AppError::localized(
            "claude_desktop.provider.direct_mapping_unsupported",
            format!(
                "Claude Desktop 直连模式不能映射模型: {route_id} -> {upstream_model}；非 Claude 官方模型请使用本地路由模式"
            ),
            format!(
                "Claude Desktop direct mode cannot map models: {route_id} -> {upstream_model}; use proxy mode for non-Claude official models"
            ),
        ),
    }
}

pub fn validate_provider(provider: &Provider) -> Result<(), AppError> {
    if is_official_provider(provider) {
        return Ok(());
    }

    match provider_mode(provider) {
        ClaudeDesktopMode::Direct => validate_direct_provider(provider),
        ClaudeDesktopMode::Proxy => validate_proxy_provider(provider),
    }
}

fn direct_inference_model_specs(
    provider: &Provider,
) -> Result<Vec<crate::proxy_core_adapter::ClaudeDesktopGatewayProfileModelSpec>, AppError> {
    let Some(routes) = provider
        .meta
        .as_ref()
        .map(|meta| &meta.claude_desktop_model_routes)
    else {
        return Ok(Vec::new());
    };

    crate::proxy_core_adapter::claude_desktop_direct_inference_model_specs(routes.iter().map(
        |(route_id, route)| crate::proxy_core_adapter::ClaudeDesktopProxyRouteInput {
            route_id,
            upstream_model: &route.model,
            label_override: route.label_override.as_deref(),
            supports_1m: route.supports_1m.unwrap_or(false),
        },
    ))
    .map(|specs| {
        specs
            .into_iter()
            .map(crate::proxy_core_adapter::ClaudeDesktopGatewayProfileModelSpec::from)
            .collect()
    })
    .map_err(direct_model_route_issue_to_error)
}

pub fn proxy_model_routes(provider: &Provider) -> Result<Vec<ResolvedModelRoute>, AppError> {
    let routes = provider
        .meta
        .as_ref()
        .map(|meta| &meta.claude_desktop_model_routes)
        .ok_or_else(|| {
            AppError::localized(
                "claude_desktop.provider.routes_missing",
                "Claude Desktop 本地路由模式缺少模型路由映射",
                "Claude Desktop proxy mode is missing model route mappings",
            )
        })?;

    let result = crate::proxy_core_adapter::claude_desktop_proxy_model_routes(routes.iter().map(
        |(route_id, route)| crate::proxy_core_adapter::ClaudeDesktopProxyRouteInput {
            route_id,
            upstream_model: &route.model,
            label_override: route.label_override.as_deref(),
            supports_1m: route.supports_1m.unwrap_or(false),
        },
    ))
    .into_iter()
    .map(ResolvedModelRoute::from)
    .collect::<Vec<_>>();

    if result.is_empty() {
        return Err(AppError::localized(
            "claude_desktop.provider.routes_missing",
            "Claude Desktop 本地路由模式至少需要一个模型路由映射",
            "Claude Desktop proxy mode requires at least one model route mapping",
        ));
    }

    Ok(result)
}

pub fn map_proxy_request_model(mut body: Value, provider: &Provider) -> Result<Value, AppError> {
    let routes = proxy_model_routes(provider)?;
    let raw_routes = provider
        .meta
        .as_ref()
        .into_iter()
        .flat_map(|meta| meta.claude_desktop_model_routes.iter())
        .map(
            |(route_id, route)| crate::proxy_core_adapter::ClaudeDesktopProxyRouteInput {
                route_id,
                upstream_model: &route.model,
                label_override: route.label_override.as_deref(),
                supports_1m: route.supports_1m.unwrap_or(false),
            },
        );
    let core_routes = routes
        .iter()
        .map(
            |route| crate::proxy_core_adapter::ClaudeDesktopResolvedProxyRoute {
                route_id: route.route_id.clone(),
                upstream_model: route.upstream_model.clone(),
                label_override: route.label_override.clone(),
                supports_1m: route.supports_1m,
            },
        )
        .collect::<Vec<_>>();

    let api_format = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.api_format.as_deref());
    body = crate::proxy_core_adapter::claude_desktop_proxy_request_body_with_upstream_model(
        body,
        &provider.settings_config,
        api_format,
        &core_routes,
        raw_routes,
    )
    .map_err(|issue| match issue {
        ClaudeDesktopProxyRequestBodyIssue::MissingModel => AppError::localized(
            "claude_desktop.provider.model_missing",
            "Claude Desktop 请求缺少 model 字段",
            "Claude Desktop request is missing the model field",
        ),
        ClaudeDesktopProxyRequestBodyIssue::UnknownRoute { requested_model } => {
            AppError::localized(
                "claude_desktop.provider.route_unknown",
                format!("Claude Desktop 模型路由未配置: {requested_model}"),
                format!("Claude Desktop model route is not configured: {requested_model}"),
            )
        }
    })?;

    Ok(body)
}

pub fn proxy_gateway_base_url_from_db(db: &Database) -> Result<String, AppError> {
    // get_proxy_config is async-tagged but its body is fully synchronous (rusqlite
    // under a Mutex), so block_on cannot deadlock the calling thread.
    let config = futures::executor::block_on(db.get_proxy_config())?;
    let (proxy_origin, _) = crate::proxy_core_adapter::proxy_live_urls_from_listen_parts(
        &config.listen_address,
        config.listen_port,
    )
    .ok_or_else(|| {
        AppError::Config(
            "Claude Desktop 代理地址需要真实监听端口；请先启动本地代理或使用固定端口".to_string(),
        )
    })?;
    Ok(crate::proxy_core_adapter::claude_desktop_proxy_gateway_base_url(&proxy_origin))
}

fn apply_provider_to_paths(
    db: &Database,
    provider: &Provider,
    paths: &ClaudeDesktopPaths,
) -> Result<(), AppError> {
    if is_official_provider(provider) {
        return restore_official_at_paths(paths);
    }

    validate_provider(provider)?;
    with_rollback(paths, |paths| {
        apply_provider_to_paths_inner(db, provider, paths)
    })
}

fn restore_official_at_paths(paths: &ClaudeDesktopPaths) -> Result<(), AppError> {
    with_rollback(paths, restore_official_at_paths_inner)
}

fn with_rollback<F>(paths: &ClaudeDesktopPaths, op: F) -> Result<(), AppError>
where
    F: FnOnce(&ClaudeDesktopPaths) -> Result<(), AppError>,
{
    let snapshots = snapshot_files(paths)?;
    match op(paths) {
        Ok(()) => Ok(()),
        Err(err) => match restore_snapshots(&snapshots) {
            Ok(()) => Err(err),
            Err(rollback_err) => {
                log::error!("Failed to rollback Claude Desktop config after error: {rollback_err}");
                Err(AppError::Message(format!(
                    "{err}; rollback failed: {rollback_err}"
                )))
            }
        },
    }
}

fn apply_provider_to_paths_inner(
    db: &Database,
    provider: &Provider,
    paths: &ClaudeDesktopPaths,
) -> Result<(), AppError> {
    let profile = match provider_mode(provider) {
        ClaudeDesktopMode::Direct => {
            let credentials = direct_gateway_credentials(provider)?;
            let model_specs = direct_inference_model_specs(provider)?;
            crate::proxy_core_adapter::claude_desktop_gateway_profile(
                &credentials.base_url,
                &credentials.api_key,
                (!model_specs.is_empty()).then_some(model_specs.as_slice()),
            )
        }
        ClaudeDesktopMode::Proxy => {
            let base_url = proxy_gateway_base_url_from_db(db)?;
            let api_key = get_or_create_gateway_token(db)?;
            let routes = proxy_model_routes(provider)?;
            let model_specs = routes
                .iter()
                .map(
                    |route| crate::proxy_core_adapter::ClaudeDesktopGatewayProfileModelSpec {
                        name: route.route_id.clone(),
                        label_override: route.label_override.clone(),
                        supports_1m: route.supports_1m,
                    },
                )
                .collect::<Vec<_>>();
            crate::proxy_core_adapter::claude_desktop_gateway_profile(
                &base_url,
                &api_key,
                Some(model_specs.as_slice()),
            )
        }
    };

    write_deployment_mode(&paths.normal_config_path, "3p")?;
    write_deployment_mode(&paths.threep_config_path, "3p")?;
    write_json_file(&paths.profile_path, &profile)?;
    write_meta(&paths.meta_path, Some(PROFILE_ID))?;

    Ok(())
}

fn restore_official_at_paths_inner(paths: &ClaudeDesktopPaths) -> Result<(), AppError> {
    write_deployment_mode(&paths.normal_config_path, "1p")?;
    write_deployment_mode(&paths.threep_config_path, "1p")?;
    remove_cc_switch_enterprise_config(&paths.threep_config_path)?;

    if paths.profile_path.exists() {
        delete_file(&paths.profile_path)?;
    }
    write_meta(&paths.meta_path, None)?;

    Ok(())
}

fn read_json_or_empty(path: &Path) -> Result<Value, AppError> {
    let value = if path.exists() {
        read_json_file(path)?
    } else {
        json!({})
    };

    if value.is_object() {
        Ok(value)
    } else {
        Ok(json!({}))
    }
}

fn snapshot_files(paths: &ClaudeDesktopPaths) -> Result<Vec<FileSnapshot>, AppError> {
    [
        &paths.normal_config_path,
        &paths.threep_config_path,
        &paths.profile_path,
        &paths.meta_path,
    ]
    .into_iter()
    .map(|path| {
        let content = if path.exists() {
            Some(fs::read(path).map_err(|e| AppError::io(path, e))?)
        } else {
            None
        };
        Ok(FileSnapshot {
            path: path.clone(),
            content,
        })
    })
    .collect()
}

fn restore_snapshots(snapshots: &[FileSnapshot]) -> Result<(), AppError> {
    for snapshot in snapshots {
        match &snapshot.content {
            Some(content) => {
                if let Some(parent) = snapshot.path.parent() {
                    fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
                }
                atomic_write(&snapshot.path, content)?;
            }
            None => {
                delete_file(&snapshot.path)?;
            }
        }
    }
    Ok(())
}

fn write_deployment_mode(path: &Path, mode: &str) -> Result<(), AppError> {
    let value = crate::proxy_core_adapter::claude_desktop_config_with_deployment_mode(
        read_json_or_empty(path)?,
        mode,
    );
    write_json_file(path, &value)
}

fn remove_cc_switch_enterprise_config(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }

    if let Some(value) =
        crate::proxy_core_adapter::claude_desktop_config_without_gateway_enterprise_config(
            read_json_or_empty(path)?,
        )
    {
        write_json_file(path, &value)?;
    }

    Ok(())
}

fn write_meta(path: &Path, applied_profile_id: Option<&str>) -> Result<(), AppError> {
    let value = crate::proxy_core_adapter::claude_desktop_meta_with_profile_entry(
        read_json_or_empty(path)?,
        PROFILE_ID,
        PROFILE_NAME,
        applied_profile_id,
    );
    write_json_file(path, &value)
}

fn read_applied_id(path: &Path) -> Option<String> {
    read_json_or_empty(path)
        .ok()
        .and_then(|value| crate::proxy_core_adapter::claude_desktop_meta_applied_id(&value))
}

fn meta_has_profile_entry(path: &Path) -> bool {
    read_json_or_empty(path).ok().is_some_and(|value| {
        crate::proxy_core_adapter::claude_desktop_meta_has_profile_entry(&value, PROFILE_ID)
    })
}

fn is_supported_platform() -> bool {
    cfg!(any(target_os = "macos", windows))
}

#[allow(clippy::needless_return)]
fn current_platform_paths() -> Result<ClaudeDesktopPaths, AppError> {
    #[cfg(target_os = "macos")]
    {
        return Ok(macos_paths_from_home(&get_home_dir()));
    }

    #[cfg(windows)]
    {
        let local_app_data = windows_local_app_data_dir();
        return Ok(windows_paths_from_local_app_data(&local_app_data));
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    {
        Err(unsupported_platform_error())
    }
}

#[cfg(target_os = "macos")]
fn macos_paths_from_home(home: &Path) -> ClaudeDesktopPaths {
    let app_support = home.join("Library").join("Application Support");
    paths_from_dirs(app_support.join("Claude"), app_support.join("Claude-3p"))
}

#[cfg(windows)]
fn windows_local_app_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| get_home_dir().join("AppData").join("Local"))
}

#[cfg(windows)]
fn windows_paths_from_local_app_data(local_app_data: &Path) -> ClaudeDesktopPaths {
    let normal_dir = pick_windows_claude_dir(local_app_data, false)
        .unwrap_or_else(|| local_app_data.join("Claude"));
    let threep_dir = pick_windows_claude_dir(local_app_data, true)
        .unwrap_or_else(|| local_app_data.join("Claude-3p"));
    paths_from_dirs(normal_dir, threep_dir)
}

#[cfg(windows)]
fn pick_windows_claude_dir(local_app_data: &Path, threep: bool) -> Option<PathBuf> {
    let exact_name = if threep { "Claude-3p" } else { "Claude" };
    let exact = local_app_data.join(exact_name);
    if exact.exists() {
        return Some(exact);
    }

    let mut candidates: Vec<PathBuf> = std::fs::read_dir(local_app_data)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                return false;
            };
            let starts = name.starts_with("Claude");
            let is_threep = name.contains("-3p");
            starts && is_threep == threep
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

#[cfg(any(target_os = "macos", windows, test))]
fn paths_from_dirs(normal_dir: PathBuf, threep_dir: PathBuf) -> ClaudeDesktopPaths {
    let config_library_path = threep_dir.join(CONFIG_LIBRARY_DIR);
    let profile_path = config_library_path.join(format!("{PROFILE_ID}.json"));
    let meta_path = config_library_path.join("_meta.json");

    ClaudeDesktopPaths {
        normal_config_path: normal_dir.join(CONFIG_FILE),
        threep_config_path: threep_dir.join(CONFIG_FILE),
        config_library_path,
        profile_path,
        meta_path,
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn unsupported_platform_error() -> AppError {
    AppError::localized(
        "claude_desktop.unsupported_platform",
        "当前平台暂不支持 Claude Desktop 3P 配置。第一阶段仅支持 macOS 和 Windows。",
        "Claude Desktop 3P configuration is not supported on this platform yet. Phase 1 only supports macOS and Windows.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::provider::{ClaudeDesktopModelRoute, ProviderMeta};
    use crate::proxy_core_adapter::ProxyConfig;
    use serde_json::json;
    use tempfile::TempDir;

    fn test_paths(home: &Path) -> ClaudeDesktopPaths {
        paths_from_dirs(
            home.join("Library")
                .join("Application Support")
                .join("Claude"),
            home.join("Library")
                .join("Application Support")
                .join("Claude-3p"),
        )
    }

    fn test_db() -> Database {
        Database::memory().expect("memory db")
    }

    fn set_proxy_port(db: &Database, port: u16) {
        let config = ProxyConfig {
            listen_port: port,
            ..ProxyConfig::default()
        };
        futures::executor::block_on(db.update_proxy_config(config)).expect("update proxy config");
    }

    fn direct_provider(id: &str) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            "Direct".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://gateway.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "test-token",
                    "ANTHROPIC_MODEL": "ignored-by-desktop"
                }
            }),
            Some("https://example.com".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });
        provider
    }

    #[test]
    fn proxy_gateway_base_url_rejects_unresolved_ephemeral_port() {
        let db = test_db();
        set_proxy_port(&db, 0);

        let err = proxy_gateway_base_url_from_db(&db)
            .expect_err("unresolved ephemeral port should not produce a :0 URL");
        assert!(
            err.to_string().contains("真实监听端口"),
            "unexpected error: {err}"
        );
    }

    fn official_provider() -> Provider {
        let mut provider = Provider::with_id(
            CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID.to_string(),
            "Claude Desktop Official".to_string(),
            json!({"env": {}}),
            Some("https://claude.ai/download".to_string()),
        );
        provider.category = Some("official".to_string());
        provider
    }

    fn proxy_provider(id: &str) -> Provider {
        let mut provider = direct_provider(id);
        provider.name = "Proxy".to_string();
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            api_format: Some("openai_chat".to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "kimi-k2".to_string(),
                    label_override: Some("Kimi K2".to_string()),
                    supports_1m: Some(true),
                },
            )]),
            ..Default::default()
        });
        provider
    }

    fn mimo_anthropic_proxy_provider(id: &str) -> Provider {
        let mut provider = direct_provider(id);
        provider.name = "MiMo Proxy".to_string();
        provider.settings_config = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.xiaomimimo.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "test-token"
            }
        });
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            api_format: Some("anthropic".to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "mimo-v2.5-pro".to_string(),
                    label_override: Some("MiMo v2.5 Pro".to_string()),
                    supports_1m: Some(true),
                },
            )]),
            ..Default::default()
        });
        provider
    }

    fn oauth_proxy_provider(id: &str, provider_type: &str, api_format: &str) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            "OAuth Proxy".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://oauth-upstream.example.com"
                }
            }),
            Some("https://example.com".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            api_format: Some(api_format.to_string()),
            provider_type: Some(provider_type.to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "gpt-5.4".to_string(),
                    label_override: Some("GPT-5.4".to_string()),
                    supports_1m: Some(false),
                },
            )]),
            ..Default::default()
        });
        provider
    }

    fn direct_provider_with_models(id: &str) -> Provider {
        let mut provider = direct_provider(id);
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Direct),
            api_format: Some("anthropic".to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "claude-sonnet-4-6".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            )]),
            ..Default::default()
        });
        provider
    }

    #[test]
    fn claude_desktop_apply_writes_3p_profile_and_meta() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = direct_provider("direct");
        let db = test_db();

        apply_provider_to_paths(&db, &provider, &paths).expect("apply provider");

        let normal: Value = read_json_file(&paths.normal_config_path).expect("read normal config");
        let threep: Value = read_json_file(&paths.threep_config_path).expect("read 3p config");
        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
        let meta: Value = read_json_file(&paths.meta_path).expect("read meta");

        assert_eq!(normal["deploymentMode"], json!("3p"));
        assert_eq!(threep["deploymentMode"], json!("3p"));
        assert_eq!(profile["inferenceProvider"], json!("gateway"));
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            json!("https://gateway.example.com")
        );
        assert_eq!(profile["inferenceGatewayApiKey"], json!("test-token"));
        assert_eq!(profile["inferenceGatewayAuthScheme"], json!("bearer"));
        assert_eq!(profile["disableDeploymentModeChooser"], json!(true));
        assert_eq!(profile["coworkEgressAllowedHosts"], json!(["*"]));
        assert!(profile.get("inferenceModels").is_none());
        assert_eq!(meta["appliedId"], json!(PROFILE_ID));
        assert!(meta["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .any(|entry| entry["id"] == json!(PROFILE_ID) && entry["name"] == json!(PROFILE_NAME)));
    }

    #[test]
    fn claude_desktop_direct_can_write_optional_safe_model_ids() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = direct_provider_with_models("direct-models");
        let db = test_db();

        apply_provider_to_paths(&db, &provider, &paths).expect("apply provider");

        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            json!("https://gateway.example.com")
        );
        assert_eq!(
            profile["inferenceModels"],
            json!([{ "name": "claude-sonnet-4-6", "supports1m": true }])
        );
    }

    #[test]
    fn claude_desktop_direct_rejects_model_mapping_to_non_claude_upstream() {
        let mut provider = direct_provider_with_models("direct-non-claude");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes
            .get_mut("claude-sonnet-4-6")
            .expect("route")
            .model = "mimo-v2.5-pro".to_string();

        let err = validate_provider(&provider).expect_err("direct mapping should fail");
        assert!(err.to_string().contains("本地路由模式"));
    }

    #[test]
    fn claude_desktop_proxy_apply_writes_local_gateway_profile_with_safe_models() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = proxy_provider("proxy");
        let db = test_db();

        apply_provider_to_paths(&db, &provider, &paths).expect("apply proxy provider");

        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            json!("http://127.0.0.1:15721/claude-desktop")
        );
        assert_eq!(profile["inferenceGatewayAuthScheme"], json!("bearer"));
        assert_eq!(profile["coworkEgressAllowedHosts"], json!(["*"]));
        assert_ne!(profile["inferenceGatewayApiKey"], json!("test-token"));
        assert!(profile["inferenceGatewayApiKey"]
            .as_str()
            .expect("gateway token")
            .starts_with("ccs-"));
        assert_eq!(
            profile["inferenceModels"],
            json!([{ "name": "claude-sonnet-4-6", "labelOverride": "Kimi K2", "supports1m": true }])
        );
        assert!(!profile.to_string().contains("kimi-k2"));
    }

    #[test]
    fn claude_desktop_proxy_accepts_managed_oauth_providers_without_static_key() {
        for (provider_type, api_format) in [
            ("github_copilot", "openai_chat"),
            ("codex_oauth", "openai_responses"),
        ] {
            let provider = oauth_proxy_provider(provider_type, provider_type, api_format);
            validate_proxy_provider(&provider).expect("oauth proxy provider should validate");

            let temp = TempDir::new().expect("tempdir");
            let paths = test_paths(temp.path());
            let db = test_db();
            apply_provider_to_paths(&db, &provider, &paths).expect("apply oauth proxy provider");

            let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
            assert_eq!(
                profile["inferenceGatewayBaseUrl"],
                json!("http://127.0.0.1:15721/claude-desktop")
            );
            assert_eq!(
                profile["inferenceModels"],
                json!([{ "name": "claude-sonnet-4-6", "labelOverride": "GPT-5.4" }])
            );
        }
    }

    #[test]
    fn claude_desktop_proxy_maps_known_route_and_rejects_unknown_route() {
        let provider = proxy_provider("proxy");

        let mapped = map_proxy_request_model(
            json!({"model": "claude-sonnet-4-6", "messages": []}),
            &provider,
        )
        .expect("map route");
        assert_eq!(mapped["model"], json!("kimi-k2"));

        let models = serde_json::to_value(
            crate::proxy_core_adapter::ClaudeDesktopModelListResponse::from_routes(
                crate::proxy_core_adapter::claude_desktop_model_routes_to_core_inputs(
                    proxy_model_routes(&provider).expect("model routes"),
                ),
            ),
        )
        .unwrap();
        assert_eq!(models["data"][0]["id"], json!("claude-sonnet-4-6"));
        assert_eq!(models["data"][0]["supports1m"], json!(true));

        let err = map_proxy_request_model(json!({"model": "claude-opus-4-8"}), &provider)
            .expect_err("unknown route should fail");
        assert!(err.to_string().contains("claude-opus-4-8"));
    }

    #[test]
    fn claude_desktop_proxy_maps_dated_role_alias_via_keyword() {
        // 复现反馈：Claude Desktop 子 agent 请求带发布日期后缀的完整官方名
        // （claude-haiku-4-5-20251001），与 manifest 的简短 route_id（claude-haiku-4-5）
        // 不精确相等，旧逻辑会报 route_unknown。角色关键词回落应将其映射到 Haiku 档。
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([
            (
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "deepseek-v4-pro".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-opus-4-8".to_string(),
                ClaudeDesktopModelRoute {
                    model: "deepseek-v4-pro".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-haiku-4-5".to_string(),
                ClaudeDesktopModelRoute {
                    model: "deepseek-v4-flash".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
        ]);

        let mapped = map_proxy_request_model(
            json!({"model": "claude-haiku-4-5-20251001", "messages": []}),
            &provider,
        )
        .expect("dated Haiku alias should map via role keyword");
        assert_eq!(mapped["model"], json!("deepseek-v4-flash"));

        let mapped_sonnet = map_proxy_request_model(
            json!({"model": "claude-sonnet-4-5-20250101", "messages": []}),
            &provider,
        )
        .expect("dated Sonnet alias should map via role keyword");
        assert_eq!(mapped_sonnet["model"], json!("deepseek-v4-pro"));

        // 不含任何角色关键词的模型仍然报错，避免被误映射。
        let err = map_proxy_request_model(json!({"model": "gpt-5"}), &provider)
            .expect_err("model without a role keyword should still fail");
        assert!(err.to_string().contains("gpt-5"));
    }

    #[test]
    fn claude_desktop_proxy_maps_fable_to_opus_tier() {
        // issue #4026/#4049：老用户只配 Sonnet/Opus/Haiku 三档、未显式配置
        // fable 档时，fable 请求按官方分类器降级方向回落到 opus 档兜底。
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([
            (
                "claude-opus-4-8".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-opus".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-sonnet".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
        ]);

        let mapped = map_proxy_request_model(
            json!({"model": "claude-fable-5", "messages": []}),
            &provider,
        )
        .expect("fable should fall back to the opus tier");
        assert_eq!(mapped["model"], json!("upstream-opus"));

        // 带 [1m] 标记与日期后缀的形态也应命中同一回落。
        let mapped_one_m = map_proxy_request_model(
            json!({"model": "claude-fable-5[1m]", "messages": []}),
            &provider,
        )
        .expect("fable with [1m] marker should fall back to the opus tier");
        assert_eq!(mapped_one_m["model"], json!("upstream-opus"));

        let mapped_dated = map_proxy_request_model(
            json!({"model": "claude-fable-5-20260609", "messages": []}),
            &provider,
        )
        .expect("dated fable alias should fall back to the opus tier");
        assert_eq!(mapped_dated["model"], json!("upstream-opus"));
    }

    #[test]
    fn claude_desktop_proxy_fable_without_opus_route_still_errors() {
        // 没有 opus 档可回落时保持精确报错语义，不静默落到其他档。
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([(
            "claude-sonnet-4-6".to_string(),
            ClaudeDesktopModelRoute {
                model: "upstream-sonnet".to_string(),
                label_override: None,
                supports_1m: Some(true),
            },
        )]);

        let err = map_proxy_request_model(
            json!({"model": "claude-fable-5", "messages": []}),
            &provider,
        )
        .expect_err("fable without an opus route should fail");
        assert!(err.to_string().contains("claude-fable-5"));
    }

    #[test]
    fn claude_desktop_proxy_maps_fable_to_dedicated_route() {
        // Desktop 1.12603.1+ fail-all 校验已放行 claude-fable-5，用户可显式配置
        // 独立 fable 档；此时 fable 请求精确命中 fable 档，不再降级到 opus。
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([
            (
                "claude-opus-4-8".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-opus".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-fable-5".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-fable".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
        ]);

        // 精确匹配优先命中 fable 档
        let mapped = map_proxy_request_model(
            json!({"model": "claude-fable-5", "messages": []}),
            &provider,
        )
        .expect("explicit fable route should match");
        assert_eq!(mapped["model"], json!("upstream-fable"));

        // 带日期后缀经角色关键词回落仍归 fable 档，而非降级 opus
        let mapped_dated = map_proxy_request_model(
            json!({"model": "claude-fable-5-20260609", "messages": []}),
            &provider,
        )
        .expect("dated fable alias should map via fable role keyword");
        assert_eq!(mapped_dated["model"], json!("upstream-fable"));
    }

    #[test]
    fn claude_desktop_proxy_accepts_opus_4_7_4_8_alias_during_rollout() {
        let mut provider = proxy_provider("proxy");
        let current_routes = std::collections::HashMap::from([(
            "claude-opus-4-8".to_string(),
            ClaudeDesktopModelRoute {
                model: "upstream-opus-new".to_string(),
                label_override: None,
                supports_1m: Some(true),
            },
        )]);
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = current_routes;

        let mapped = map_proxy_request_model(
            json!({"model": "claude-opus-4-7", "messages": []}),
            &provider,
        )
        .expect("legacy Opus route should map to current route");
        assert_eq!(mapped["model"], json!("upstream-opus-new"));

        let legacy_routes = std::collections::HashMap::from([(
            "claude-opus-4-7".to_string(),
            ClaudeDesktopModelRoute {
                model: "upstream-opus-legacy".to_string(),
                label_override: None,
                supports_1m: Some(true),
            },
        )]);
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = legacy_routes;

        let mapped = map_proxy_request_model(
            json!({"model": "claude-opus-4-8", "messages": []}),
            &provider,
        )
        .expect("current Opus route should map to legacy saved route");
        assert_eq!(mapped["model"], json!("upstream-opus-legacy"));
    }

    #[test]
    fn claude_desktop_mimo_anthropic_rewrites_redacted_thinking_for_tool_history() {
        let provider = mimo_anthropic_proxy_provider("mimo");

        let mapped = map_proxy_request_model(
            json!({
                "model": "claude-sonnet-4-6",
                "messages": [{
                    "role": "assistant",
                    "content": [
                        {"type": "redacted_thinking", "data": "opaque"},
                        {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {"path": "README.md"}}
                    ]
                }]
            }),
            &provider,
        )
        .expect("map MiMo route");

        assert_eq!(mapped["model"], json!("mimo-v2.5-pro"));
        assert_eq!(
            mapped["messages"][0]["content"][0]["type"],
            json!("thinking")
        );
        assert_eq!(
            mapped["messages"][0]["content"][0]["thinking"],
            json!("[redacted thinking]")
        );
        assert_eq!(
            mapped["messages"][0]["content"][1]["type"],
            json!("tool_use")
        );
    }

    #[test]
    fn claude_desktop_mimo_anthropic_injects_thinking_for_tool_history_without_one() {
        let provider = mimo_anthropic_proxy_provider("mimo");

        let mapped = map_proxy_request_model(
            json!({
                "model": "claude-sonnet-4-6",
                "messages": [{
                    "role": "assistant",
                    "content": [
                        {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {"path": "README.md"}}
                    ]
                }]
            }),
            &provider,
        )
        .expect("map MiMo route");

        assert_eq!(
            mapped["messages"][0]["content"][0]["type"],
            json!("thinking")
        );
        assert_eq!(
            mapped["messages"][0]["content"][0]["thinking"],
            json!("tool call")
        );
        assert_eq!(
            mapped["messages"][0]["content"][1]["type"],
            json!("tool_use")
        );
    }

    #[test]
    fn claude_desktop_mimo_anthropic_keeps_thinking_text_but_drops_signature() {
        let provider = mimo_anthropic_proxy_provider("mimo");

        let mapped = map_proxy_request_model(
            json!({
                "model": "claude-sonnet-4-6",
                "messages": [{
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "Need to inspect the file.", "signature": "anthropic-signature"},
                        {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {"path": "README.md"}}
                    ]
                }]
            }),
            &provider,
        )
        .expect("map MiMo route");

        assert_eq!(
            mapped["messages"][0]["content"][0]["thinking"],
            json!("Need to inspect the file.")
        );
        assert!(mapped["messages"][0]["content"][0]
            .get("signature")
            .is_none());
    }

    #[test]
    fn claude_desktop_proxy_repairs_legacy_unsafe_route_without_colliding() {
        let mut provider = proxy_provider("proxy");
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            api_format: Some("openai_chat".to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([
                (
                    "claude-deepseek-v4-pro".to_string(),
                    ClaudeDesktopModelRoute {
                        model: "deepseek-v4-pro".to_string(),
                        label_override: None,
                        supports_1m: Some(true),
                    },
                ),
                (
                    "claude-old".to_string(),
                    ClaudeDesktopModelRoute {
                        model: "legacy-upstream".to_string(),
                        label_override: None,
                        supports_1m: Some(false),
                    },
                ),
                (
                    "claude-sonnet-4-6".to_string(),
                    ClaudeDesktopModelRoute {
                        model: "claude-sonnet-4-6".to_string(),
                        label_override: None,
                        supports_1m: Some(false),
                    },
                ),
            ]),
            ..Default::default()
        });

        let routes = proxy_model_routes(&provider).expect("routes");
        assert_eq!(routes.len(), 3);
        let repaired = routes
            .iter()
            .find(|route| route.upstream_model == "deepseek-v4-pro")
            .expect("repaired route");
        assert_eq!(repaired.route_id, "claude-opus-4-8");
        assert_eq!(repaired.label_override.as_deref(), Some("deepseek-v4-pro"));
        assert!(repaired.supports_1m);
        let repaired_old = routes
            .iter()
            .find(|route| route.upstream_model == "legacy-upstream")
            .expect("legacy route should be repaired");
        assert_eq!(repaired_old.route_id, "claude-haiku-4-5");
        assert_eq!(
            repaired_old.label_override.as_deref(),
            Some("legacy-upstream")
        );

        let mapped = map_proxy_request_model(
            json!({"model": "claude-opus-4-8", "messages": []}),
            &provider,
        )
        .expect("map repaired route");
        assert_eq!(mapped["model"], json!("deepseek-v4-pro"));

        let legacy_mapped =
            map_proxy_request_model(json!({"model": "claude-old", "messages": []}), &provider)
                .expect("map stale profile route");
        assert_eq!(legacy_mapped["model"], json!("legacy-upstream"));
    }

    #[test]
    fn claude_desktop_proxy_strips_1m_suffix_before_route_lookup() {
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([
            (
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-sonnet".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-opus-4-8".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-opus".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
        ]);

        let mapped = map_proxy_request_model(
            json!({"model": "claude-opus-4-8[1m]", "messages": []}),
            &provider,
        )
        .expect("compact 1M suffix should map to Opus route");
        assert_eq!(mapped["model"], json!("upstream-opus"));

        let mapped = map_proxy_request_model(
            json!({"model": "claude-sonnet-4-6 [1M]", "messages": []}),
            &provider,
        )
        .expect("spaced uppercase 1M suffix should map to Sonnet route");
        assert_eq!(mapped["model"], json!("upstream-sonnet"));

        let err = map_proxy_request_model(json!({"model": "gpt-5[1m]", "messages": []}), &provider)
            .expect_err("non-Claude route should still fail after stripping 1M suffix");
        assert!(err.to_string().contains("gpt-5[1m]"));
    }

    #[test]
    fn claude_desktop_rejects_1m_suffix_as_model_id() {
        let is_safe = crate::proxy_core_adapter::claude_desktop_model_id_is_profile_safe;

        assert!(!is_safe("claude-sonnet-4-6 [1m]"));
        assert!(!is_safe("  claude-sonnet-4-6  [1M]  "));
        assert!(!is_safe("claude-old"));
        assert!(!is_safe("claude-3-5-sonnet-20241022"));
        assert!(!is_safe("claude-deepseek-v4-pro"));
        assert!(!is_safe("claude-gpt-5-4"));
        assert!(!is_safe("claude-"));
        assert!(!is_safe("anthropic/claude-"));
        assert!(!is_safe("sonnet"));
        assert!(!is_safe("sonnet-"));
        // 角色前缀后无实际标识的退化值必须拒绝
        assert!(!is_safe("claude-sonnet-"));
        assert!(!is_safe("claude-opus-"));
        assert!(!is_safe("anthropic/claude-haiku-"));
        assert!(is_safe("  claude-sonnet-4-6  "));
        assert!(is_safe("anthropic/claude-opus-4-8"));
    }

    #[test]
    fn claude_desktop_apply_rolls_back_when_profile_write_fails() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = direct_provider("direct");
        let db = test_db();

        write_json_file(
            &paths.normal_config_path,
            &json!({"deploymentMode": "1p", "normal": true}),
        )
        .expect("write normal");
        write_json_file(
            &paths.threep_config_path,
            &json!({"deploymentMode": "1p", "threep": true}),
        )
        .expect("write 3p");
        fs::write(&paths.config_library_path, "not a directory").expect("block profile parent");

        apply_provider_to_paths(&db, &provider, &paths).expect_err("apply should fail");

        let normal: Value = read_json_file(&paths.normal_config_path).expect("read normal config");
        let threep: Value = read_json_file(&paths.threep_config_path).expect("read 3p config");

        assert_eq!(normal, json!({"deploymentMode": "1p", "normal": true}));
        assert_eq!(threep, json!({"deploymentMode": "1p", "threep": true}));
        assert!(!paths.profile_path.exists());
    }

    #[test]
    fn claude_desktop_write_meta_recovers_non_object_meta_file() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        if let Some(parent) = paths.meta_path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&paths.meta_path, "[]").expect("write invalid meta shape");

        write_meta(&paths.meta_path, Some(PROFILE_ID)).expect("write meta");

        let meta: Value = read_json_file(&paths.meta_path).expect("read meta");
        assert_eq!(meta["appliedId"], json!(PROFILE_ID));
        assert!(meta["entries"].as_array().is_some());
    }

    #[test]
    fn claude_desktop_restore_switches_to_1p_and_removes_cc_switch_profile() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = direct_provider("direct");
        let db = test_db();

        apply_provider_to_paths(&db, &provider, &paths).expect("apply provider");
        restore_official_at_paths(&paths).expect("restore official");

        let normal: Value = read_json_file(&paths.normal_config_path).expect("read normal config");
        let threep: Value = read_json_file(&paths.threep_config_path).expect("read 3p config");
        let meta: Value = read_json_file(&paths.meta_path).expect("read meta");

        assert_eq!(normal["deploymentMode"], json!("1p"));
        assert_eq!(threep["deploymentMode"], json!("1p"));
        assert!(!paths.profile_path.exists());
        assert!(meta.get("appliedId").is_none());
        assert!(!meta["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .any(|entry| entry["id"] == json!(PROFILE_ID)));
    }

    #[test]
    fn claude_desktop_restore_removes_gateway_enterprise_config_only() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        write_json_file(
            &paths.threep_config_path,
            &json!({
                "deploymentMode": "3p",
                "enterpriseConfig": {
                    "disableDeploymentModeChooser": true,
                    "inferenceGatewayApiKey": "ccs-token",
                    "inferenceGatewayAuthScheme": "bearer",
                    "inferenceGatewayBaseUrl": "http://127.0.0.1:15721",
                    "inferenceProvider": "gateway",
                    "managedSetting": true
                }
            }),
        )
        .expect("seed 3p config");

        restore_official_at_paths(&paths).expect("restore official");

        let threep: Value = read_json_file(&paths.threep_config_path).expect("read 3p config");
        assert_eq!(threep["deploymentMode"], json!("1p"));
        assert_eq!(
            threep["enterpriseConfig"],
            json!({ "managedSetting": true })
        );
    }

    #[test]
    fn claude_desktop_official_provider_restores_1p_mode() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let direct = direct_provider("direct");
        let db = test_db();

        apply_provider_to_paths(&db, &direct, &paths).expect("apply direct provider");
        apply_provider_to_paths(&db, &official_provider(), &paths)
            .expect("restore official provider");

        let normal: Value = read_json_file(&paths.normal_config_path).expect("read normal config");
        let threep: Value = read_json_file(&paths.threep_config_path).expect("read 3p config");
        let meta: Value = read_json_file(&paths.meta_path).expect("read meta");

        assert_eq!(normal["deploymentMode"], json!("1p"));
        assert_eq!(threep["deploymentMode"], json!("1p"));
        assert!(!paths.profile_path.exists());
        assert!(meta.get("appliedId").is_none());
    }

    #[test]
    fn claude_desktop_compatibility_filters_non_direct_providers() {
        let direct = direct_provider("direct");
        assert!(is_compatible_direct_provider(&direct));

        let mut claude_official = Provider::with_id(
            "claude-official".to_string(),
            "Claude Official".to_string(),
            json!({"env": {}}),
            Some("https://www.anthropic.com/claude-code".to_string()),
        );
        claude_official.category = Some("official".to_string());
        assert!(!is_compatible_direct_provider(&claude_official));

        let mut openai_format = direct_provider("openai");
        openai_format.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        assert!(!is_compatible_direct_provider(&openai_format));

        let mut copilot = direct_provider("copilot");
        copilot.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..Default::default()
        });
        assert!(!is_compatible_direct_provider(&copilot));

        let mut full_url = direct_provider("full_url");
        full_url.meta = Some(ProviderMeta {
            is_full_url: Some(true),
            ..Default::default()
        });
        assert!(!is_compatible_direct_provider(&full_url));

        let missing_bearer = Provider::with_id(
            "x-api-key".to_string(),
            "x-api-key".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://gateway.example.com",
                    "ANTHROPIC_API_KEY": "sk-ant"
                }
            }),
            None,
        );
        assert!(!is_compatible_direct_provider(&missing_bearer));
    }
}
