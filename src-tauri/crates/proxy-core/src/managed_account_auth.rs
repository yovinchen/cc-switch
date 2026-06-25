use futures::future::BoxFuture;
use http::{HeaderMap, StatusCode};
use thiserror::Error;

use crate::copilot_model_map::{resolve_copilot_model_against_ids, CopilotModel};
use crate::domain::ProviderKind;
use crate::provider_auth::{ProviderAuthInfo, ProviderAuthStrategy};
use crate::request_url::resolved_copilot_dynamic_base_url;

pub const PROXY_AUTH_PLACEHOLDER: &str = "PROXY_MANAGED";
pub const GITHUB_COPILOT_AUTH_PROVIDER: &str = "github_copilot";
pub const CODEX_OAUTH_AUTH_PROVIDER: &str = "codex_oauth";
pub const GITHUB_COPILOT_AUTH_PLACEHOLDER: &str = "copilot_placeholder";
pub const CODEX_OAUTH_AUTH_PLACEHOLDER: &str = "codex_oauth_placeholder";
pub const COPILOT_TOKEN_REFRESH_BUFFER_SECONDS: i64 = 60;
pub const CODEX_OAUTH_TOKEN_REFRESH_BUFFER_MS: i64 = 60_000;
pub const CODEX_OAUTH_DEVICE_CODE_DEFAULT_EXPIRES_IN_SECS: u64 = 900;
pub const CODEX_OAUTH_DEFAULT_TOKEN_EXPIRES_IN_SECS: i64 = 3600;
pub const CODEX_OAUTH_POLLING_SAFETY_MARGIN_SECS: u64 = 3;
pub const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const CODEX_OAUTH_DEVICE_AUTH_USERCODE_URL: &str =
    "https://auth.openai.com/api/accounts/deviceauth/usercode";
pub const CODEX_OAUTH_DEVICE_AUTH_TOKEN_URL: &str =
    "https://auth.openai.com/api/accounts/deviceauth/token";
pub const CODEX_OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const CODEX_OAUTH_DEVICE_VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";
pub const CODEX_OAUTH_DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";

pub type ManagedAccountRuntimeResultFuture<'a, T, E> = BoxFuture<'a, Result<T, E>>;
pub type CodexOAuthResolution = (ProviderAuthInfo, Option<String>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopilotOAuthPollErrorKind {
    AuthorizationPending,
    ExpiredToken,
    AccessDenied,
    NetworkError(String),
}

pub fn copilot_token_is_expiring_soon(expires_at: i64, now: i64) -> bool {
    expires_at - now < COPILOT_TOKEN_REFRESH_BUFFER_SECONDS
}

pub fn copilot_oauth_poll_error_kind(
    error: &str,
    error_description: Option<&str>,
) -> CopilotOAuthPollErrorKind {
    match error {
        "authorization_pending" | "slow_down" => CopilotOAuthPollErrorKind::AuthorizationPending,
        "expired_token" => CopilotOAuthPollErrorKind::ExpiredToken,
        "access_denied" => CopilotOAuthPollErrorKind::AccessDenied,
        other => CopilotOAuthPollErrorKind::NetworkError(format!(
            "{}: {}",
            other,
            error_description.unwrap_or_default()
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexOAuthDevicePollStatusKind {
    AuthorizationPending,
    ExpiredToken,
    Failed,
    Success,
}

pub fn codex_oauth_token_is_expiring_soon(expires_at_ms: i64, now_ms: i64) -> bool {
    expires_at_ms - now_ms < CODEX_OAUTH_TOKEN_REFRESH_BUFFER_MS
}

pub fn codex_oauth_device_code_expires_in_secs(expires_in: Option<u64>) -> u64 {
    expires_in.unwrap_or(CODEX_OAUTH_DEVICE_CODE_DEFAULT_EXPIRES_IN_SECS)
}

pub fn codex_oauth_device_code_expires_at_ms(expires_in: Option<u64>, now_ms: i64) -> i64 {
    now_ms + (codex_oauth_device_code_expires_in_secs(expires_in) as i64) * 1000
}

pub fn codex_oauth_access_token_expires_at_ms(expires_in: Option<i64>, now_ms: i64) -> i64 {
    now_ms + expires_in.unwrap_or(CODEX_OAUTH_DEFAULT_TOKEN_EXPIRES_IN_SECS) * 1000
}

pub fn codex_oauth_pending_device_code_is_expired(expires_at_ms: i64, now_ms: i64) -> bool {
    expires_at_ms <= now_ms
}

pub fn codex_oauth_poll_interval_secs(value: Option<&serde_json::Value>) -> u64 {
    let raw = match value {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(5),
        Some(serde_json::Value::String(s)) => s.parse::<u64>().unwrap_or(5),
        _ => 5,
    };
    raw.max(1) + CODEX_OAUTH_POLLING_SAFETY_MARGIN_SECS
}

pub fn codex_oauth_device_poll_status_kind(status: StatusCode) -> CodexOAuthDevicePollStatusKind {
    match status {
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => {
            CodexOAuthDevicePollStatusKind::AuthorizationPending
        }
        StatusCode::GONE => CodexOAuthDevicePollStatusKind::ExpiredToken,
        status if status.is_success() => CodexOAuthDevicePollStatusKind::Success,
        _ => CodexOAuthDevicePollStatusKind::Failed,
    }
}

pub fn codex_oauth_device_auth_usercode_url() -> &'static str {
    CODEX_OAUTH_DEVICE_AUTH_USERCODE_URL
}

pub fn codex_oauth_device_auth_token_url() -> &'static str {
    CODEX_OAUTH_DEVICE_AUTH_TOKEN_URL
}

pub fn codex_oauth_token_url() -> &'static str {
    CODEX_OAUTH_TOKEN_URL
}

pub fn codex_oauth_device_verification_url() -> &'static str {
    CODEX_OAUTH_DEVICE_VERIFICATION_URL
}

pub fn codex_oauth_device_usercode_request_body() -> serde_json::Value {
    serde_json::json!({ "client_id": CODEX_OAUTH_CLIENT_ID })
}

pub fn codex_oauth_device_auth_token_request_body(
    device_auth_id: &str,
    user_code: &str,
) -> serde_json::Value {
    serde_json::json!({
        "device_auth_id": device_auth_id,
        "user_code": user_code,
    })
}

pub fn codex_oauth_authorization_code_form<'a>(
    code: &'a str,
    code_verifier: &'a str,
) -> [(&'static str, &'a str); 5] {
    [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", CODEX_OAUTH_DEVICE_REDIRECT_URI),
        ("client_id", CODEX_OAUTH_CLIENT_ID),
        ("code_verifier", code_verifier),
    ]
}

pub fn codex_oauth_refresh_token_form(refresh_token: &str) -> [(&'static str, &str); 4] {
    [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CODEX_OAUTH_CLIENT_ID),
        ("scope", "openid profile email"),
    ]
}

pub fn codex_oauth_device_code_request_failure(status: StatusCode, body: impl AsRef<str>) -> String {
    format!("Device Code 请求失败: {status} - {}", body.as_ref())
}

pub fn codex_oauth_device_poll_failure(status: StatusCode, body: impl AsRef<str>) -> String {
    format!("{status} - {}", body.as_ref())
}

pub fn codex_oauth_token_exchange_failure(status: StatusCode, body: impl AsRef<str>) -> String {
    format!("Token 交换失败: {status} - {}", body.as_ref())
}

pub fn codex_oauth_refresh_failure(status: StatusCode, body: impl AsRef<str>) -> String {
    format!("Refresh 失败: {status} - {}", body.as_ref())
}

pub fn codex_oauth_missing_pending_user_code_message() -> &'static str {
    "未找到对应的 user_code，请重新启动登录流程"
}

pub fn codex_oauth_missing_refresh_token_message() -> &'static str {
    "响应缺少 refresh_token"
}

pub fn codex_oauth_missing_account_id_message() -> &'static str {
    "无法从 token 中提取 account_id"
}

pub fn unsupported_managed_auth_provider_message(auth_provider: &str) -> String {
    format!("Unsupported auth provider: {auth_provider}")
}

pub fn ensure_managed_auth_provider(auth_provider: &str) -> Result<&'static str, String> {
    match auth_provider {
        GITHUB_COPILOT_AUTH_PROVIDER => Ok(GITHUB_COPILOT_AUTH_PROVIDER),
        CODEX_OAUTH_AUTH_PROVIDER => Ok(CODEX_OAUTH_AUTH_PROVIDER),
        other => Err(unsupported_managed_auth_provider_message(other)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedAccountAuthRuntime {
    GitHubCopilot,
    CodexOAuth,
}

impl ManagedAccountAuthRuntime {
    pub fn provider_auth_strategy(self) -> ProviderAuthStrategy {
        match self {
            Self::GitHubCopilot => ProviderAuthStrategy::GitHubCopilot,
            Self::CodexOAuth => ProviderAuthStrategy::CodexOAuth,
        }
    }

    pub fn log_label(self) -> &'static str {
        match self {
            Self::GitHubCopilot => "Copilot",
            Self::CodexOAuth => "CodexOAuth",
        }
    }

    pub fn auth_error_label(self) -> &'static str {
        match self {
            Self::GitHubCopilot => "GitHub Copilot",
            Self::CodexOAuth => "Codex OAuth",
        }
    }

    pub fn token_label(self) -> &'static str {
        match self {
            Self::GitHubCopilot => "Copilot token",
            Self::CodexOAuth => "access_token",
        }
    }

    pub fn provider_auth_info(self, token: String) -> ProviderAuthInfo {
        ProviderAuthInfo::new(token, self.provider_auth_strategy())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedAccountAuthPlan {
    Passthrough {
        auth: ProviderAuthInfo,
    },
    ResolveRuntimeToken {
        runtime: ManagedAccountAuthRuntime,
        account_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedAccountAuthResolution {
    pub auth: ProviderAuthInfo,
    pub codex_oauth_account_id: Option<String>,
    pub should_send_codex_oauth_session_headers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedAccountBindingSource {
    ProviderConfig,
    ManagedAccount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedAccountBindingInput<'a> {
    pub source: ManagedAccountBindingSource,
    pub auth_provider: Option<&'a str>,
    pub account_id: Option<&'a str>,
}

impl ManagedAccountAuthResolution {
    pub fn passthrough(auth: ProviderAuthInfo) -> Self {
        Self {
            auth,
            codex_oauth_account_id: None,
            should_send_codex_oauth_session_headers: false,
        }
    }

    pub fn runtime_token(
        auth: ProviderAuthInfo,
        codex_oauth_account_id: Option<String>,
        should_send_codex_oauth_session_headers: bool,
    ) -> Self {
        Self {
            auth,
            codex_oauth_account_id,
            should_send_codex_oauth_session_headers,
        }
    }
}

pub fn managed_account_id_for_auth_provider(
    auth_provider: &str,
    binding: Option<ManagedAccountBindingInput<'_>>,
    legacy_github_copilot_account_id: Option<&str>,
) -> Option<String> {
    if let Some(binding) = binding {
        if binding.source == ManagedAccountBindingSource::ManagedAccount
            && binding.auth_provider == Some(auth_provider)
        {
            return binding.account_id.map(str::to_string);
        }
    }

    if auth_provider == GITHUB_COPILOT_AUTH_PROVIDER {
        return legacy_github_copilot_account_id.map(str::to_string);
    }

    None
}

pub fn provider_kind_uses_managed_account_auth(
    provider_kind: Option<&ProviderKind>,
    anthropic_base_url: Option<&str>,
) -> bool {
    provider_kind_is_github_copilot(provider_kind, anthropic_base_url)
        || provider_kind_is_codex_oauth(provider_kind)
        || anthropic_base_url
            .map(|base_url| base_url.contains("chatgpt.com/backend-api/codex"))
            .unwrap_or(false)
}

pub fn provider_kind_is_github_copilot(
    provider_kind: Option<&ProviderKind>,
    anthropic_base_url: Option<&str>,
) -> bool {
    matches!(provider_kind, Some(ProviderKind::GitHubCopilot))
        || anthropic_base_url
            .map(|base_url| base_url.contains("githubcopilot.com"))
            .unwrap_or(false)
}

pub fn provider_kind_is_codex_oauth(provider_kind: Option<&ProviderKind>) -> bool {
    matches!(provider_kind, Some(ProviderKind::CodexOAuth))
}

pub fn managed_provider_auth_info_for_provider_kind(
    provider_kind: &ProviderKind,
) -> Option<ProviderAuthInfo> {
    match provider_kind {
        ProviderKind::GitHubCopilot => Some(ProviderAuthInfo::new(
            GITHUB_COPILOT_AUTH_PLACEHOLDER.to_string(),
            ProviderAuthStrategy::GitHubCopilot,
        )),
        ProviderKind::CodexOAuth => Some(ProviderAuthInfo::new(
            CODEX_OAUTH_AUTH_PLACEHOLDER.to_string(),
            ProviderAuthStrategy::CodexOAuth,
        )),
        _ => None,
    }
}

impl ManagedAccountAuthPlan {
    pub fn should_send_codex_oauth_session_headers(&self) -> bool {
        matches!(
            self,
            Self::ResolveRuntimeToken {
                runtime: ManagedAccountAuthRuntime::CodexOAuth,
                ..
            }
        )
    }
}

pub trait ManagedAccountRuntimeSource: Send + Sync {
    type Error;

    fn resolve_copilot_auth<'a>(
        &'a self,
        account_id: Option<&'a str>,
        runtime: ManagedAccountAuthRuntime,
    ) -> BoxFuture<'a, Result<ProviderAuthInfo, Self::Error>>;

    fn resolve_codex_oauth<'a>(
        &'a self,
        account_id: Option<String>,
        runtime: ManagedAccountAuthRuntime,
    ) -> ManagedAccountRuntimeResultFuture<'a, CodexOAuthResolution, Self::Error>;

    fn resolve_copilot_api_endpoint<'a>(
        &'a self,
        account_id: Option<&'a str>,
    ) -> BoxFuture<'a, Option<String>>;

    fn fetch_copilot_live_models<'a>(
        &'a self,
        account_id: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Option<Vec<CopilotModel>>, String>>;

    fn resolve_copilot_model_vendor<'a>(
        &'a self,
        account_id: Option<&'a str>,
        model_id: &'a str,
    ) -> BoxFuture<'a, Option<String>>;
}

pub async fn resolve_managed_account_auth_with_runtime_source<S>(
    runtime_source: &S,
    auth: ProviderAuthInfo,
    github_copilot_account_id: Option<String>,
    codex_oauth_account_id: Option<String>,
) -> Result<ManagedAccountAuthResolution, S::Error>
where
    S: ManagedAccountRuntimeSource + ?Sized,
{
    let plan = managed_account_auth_plan(auth, github_copilot_account_id, codex_oauth_account_id);
    let should_send_codex_oauth_session_headers = plan.should_send_codex_oauth_session_headers();

    match plan {
        ManagedAccountAuthPlan::ResolveRuntimeToken {
            runtime: runtime @ ManagedAccountAuthRuntime::GitHubCopilot,
            account_id,
        } => {
            let auth = runtime_source
                .resolve_copilot_auth(account_id.as_deref(), runtime)
                .await?;
            Ok(ManagedAccountAuthResolution::runtime_token(
                auth,
                None,
                should_send_codex_oauth_session_headers,
            ))
        }
        ManagedAccountAuthPlan::ResolveRuntimeToken {
            runtime: runtime @ ManagedAccountAuthRuntime::CodexOAuth,
            account_id,
        } => {
            let (auth, codex_oauth_account_id) = runtime_source
                .resolve_codex_oauth(account_id, runtime)
                .await?;
            Ok(ManagedAccountAuthResolution::runtime_token(
                auth,
                codex_oauth_account_id,
                should_send_codex_oauth_session_headers,
            ))
        }
        ManagedAccountAuthPlan::Passthrough { auth } => {
            Ok(ManagedAccountAuthResolution::passthrough(auth))
        }
    }
}

pub async fn resolve_managed_account_auth_for_binding_with_runtime_source<S>(
    runtime_source: &S,
    auth: ProviderAuthInfo,
    binding: Option<ManagedAccountBindingInput<'_>>,
    legacy_github_copilot_account_id: Option<&str>,
) -> Result<ManagedAccountAuthResolution, S::Error>
where
    S: ManagedAccountRuntimeSource + ?Sized,
{
    let github_copilot_account_id = managed_account_id_for_auth_provider(
        GITHUB_COPILOT_AUTH_PROVIDER,
        binding,
        legacy_github_copilot_account_id,
    );
    let codex_oauth_account_id =
        managed_account_id_for_auth_provider(CODEX_OAUTH_AUTH_PROVIDER, binding, None);

    resolve_managed_account_auth_with_runtime_source(
        runtime_source,
        auth,
        github_copilot_account_id,
        codex_oauth_account_id,
    )
    .await
}

pub async fn resolve_copilot_dynamic_base_url_with_runtime_source<S>(
    runtime_source: &S,
    account_id: Option<&str>,
    current_base_url: &str,
    is_copilot: bool,
    is_full_url: bool,
) -> Option<String>
where
    S: ManagedAccountRuntimeSource + ?Sized,
{
    let dynamic_endpoint = runtime_source
        .resolve_copilot_api_endpoint(account_id)
        .await?;
    resolved_copilot_dynamic_base_url(current_base_url, &dynamic_endpoint, is_copilot, is_full_url)
}

pub async fn resolve_copilot_dynamic_base_url_for_binding_with_runtime_source<S>(
    runtime_source: &S,
    binding: Option<ManagedAccountBindingInput<'_>>,
    legacy_github_copilot_account_id: Option<&str>,
    current_base_url: &str,
    is_copilot: bool,
    is_full_url: bool,
) -> Option<String>
where
    S: ManagedAccountRuntimeSource + ?Sized,
{
    let account_id = managed_account_id_for_auth_provider(
        GITHUB_COPILOT_AUTH_PROVIDER,
        binding,
        legacy_github_copilot_account_id,
    );
    resolve_copilot_dynamic_base_url_with_runtime_source(
        runtime_source,
        account_id.as_deref(),
        current_base_url,
        is_copilot,
        is_full_url,
    )
    .await
}

pub async fn resolve_copilot_live_model_with_runtime_source<S>(
    runtime_source: &S,
    account_id: Option<&str>,
    model_id: &str,
) -> Result<Option<String>, String>
where
    S: ManagedAccountRuntimeSource + ?Sized,
{
    let Some(models) = runtime_source.fetch_copilot_live_models(account_id).await? else {
        return Ok(None);
    };

    Ok(resolve_copilot_model_against_ids(
        model_id,
        models.iter().map(|model| model.id.as_str()),
    ))
}

pub async fn resolve_copilot_live_model_for_binding_with_runtime_source<S>(
    runtime_source: &S,
    binding: Option<ManagedAccountBindingInput<'_>>,
    legacy_github_copilot_account_id: Option<&str>,
    model_id: &str,
) -> Result<Option<String>, String>
where
    S: ManagedAccountRuntimeSource + ?Sized,
{
    let account_id = managed_account_id_for_auth_provider(
        GITHUB_COPILOT_AUTH_PROVIDER,
        binding,
        legacy_github_copilot_account_id,
    );
    resolve_copilot_live_model_with_runtime_source(runtime_source, account_id.as_deref(), model_id)
        .await
}

pub async fn resolve_copilot_model_vendor_with_runtime_source<S>(
    runtime_source: &S,
    account_id: Option<&str>,
    model_id: &str,
    is_copilot: bool,
) -> Option<String>
where
    S: ManagedAccountRuntimeSource + ?Sized,
{
    if !is_copilot {
        return None;
    }

    runtime_source
        .resolve_copilot_model_vendor(account_id, model_id)
        .await
}

pub async fn resolve_copilot_model_vendor_for_binding_with_runtime_source<S>(
    runtime_source: &S,
    binding: Option<ManagedAccountBindingInput<'_>>,
    legacy_github_copilot_account_id: Option<&str>,
    model_id: &str,
    is_copilot: bool,
) -> Option<String>
where
    S: ManagedAccountRuntimeSource + ?Sized,
{
    let account_id = managed_account_id_for_auth_provider(
        GITHUB_COPILOT_AUTH_PROVIDER,
        binding,
        legacy_github_copilot_account_id,
    );
    resolve_copilot_model_vendor_with_runtime_source(
        runtime_source,
        account_id.as_deref(),
        model_id,
        is_copilot,
    )
    .await
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManagedAccountAuthError {
    #[error(
        "Managed account proxy auth was not resolved; PROXY_MANAGED must not be sent upstream"
    )]
    PlaceholderForwarded,
}

pub fn managed_account_auth_plan(
    auth: ProviderAuthInfo,
    github_copilot_account_id: Option<String>,
    codex_oauth_account_id: Option<String>,
) -> ManagedAccountAuthPlan {
    match auth.strategy {
        ProviderAuthStrategy::GitHubCopilot => ManagedAccountAuthPlan::ResolveRuntimeToken {
            runtime: ManagedAccountAuthRuntime::GitHubCopilot,
            account_id: github_copilot_account_id,
        },
        ProviderAuthStrategy::CodexOAuth => ManagedAccountAuthPlan::ResolveRuntimeToken {
            runtime: ManagedAccountAuthRuntime::CodexOAuth,
            account_id: codex_oauth_account_id,
        },
        _ => ManagedAccountAuthPlan::Passthrough { auth },
    }
}

pub fn managed_account_app_handle_unavailable_log_message(
    runtime: ManagedAccountAuthRuntime,
) -> String {
    format!("[{}] AppHandle 不可用", runtime.log_label())
}

pub fn managed_account_app_handle_unavailable_error_message(
    runtime: ManagedAccountAuthRuntime,
) -> String {
    format!("{} 认证不可用（无 AppHandle）", runtime.auth_error_label())
}

pub fn managed_account_token_request_log_message(
    runtime: ManagedAccountAuthRuntime,
    account_id: Option<&str>,
) -> String {
    match account_id {
        Some(id) => format!("[{}] 使用指定账号 {id} 获取 token", runtime.log_label()),
        None => format!("[{}] 使用默认账号获取 token", runtime.log_label()),
    }
}

pub fn managed_account_token_success_log_message(
    runtime: ManagedAccountAuthRuntime,
    account_label: Option<&str>,
) -> String {
    format!(
        "[{}] 成功获取 {} (account={})",
        runtime.log_label(),
        runtime.token_label(),
        account_label.unwrap_or("default")
    )
}

pub fn managed_account_token_failure_log_message(
    runtime: ManagedAccountAuthRuntime,
    account_id: Option<&str>,
    error: &str,
) -> String {
    if runtime == ManagedAccountAuthRuntime::GitHubCopilot {
        return format!(
            "[{}] 获取 {} 失败 (account={}): {error}",
            runtime.log_label(),
            runtime.token_label(),
            account_id.unwrap_or("default")
        );
    }

    format!(
        "[{}] 获取 {} 失败: {error}",
        runtime.log_label(),
        runtime.token_label()
    )
}

pub fn managed_account_token_failure_error_message(
    runtime: ManagedAccountAuthRuntime,
    error: &str,
) -> String {
    format!("{} 认证失败: {error}", runtime.auth_error_label())
}

pub fn validate_managed_account_upstream_auth(
    url: &str,
    headers: &HeaderMap,
) -> Result<(), ManagedAccountAuthError> {
    if is_managed_account_upstream_url(url) && headers_contain_proxy_auth_placeholder(headers) {
        Err(ManagedAccountAuthError::PlaceholderForwarded)
    } else {
        Ok(())
    }
}

pub fn is_managed_account_upstream_url(url: &str) -> bool {
    let Ok(uri) = url.parse::<http::Uri>() else {
        return false;
    };

    let Some(host) = uri.host().map(str::to_ascii_lowercase) else {
        return false;
    };

    host == "githubcopilot.com"
        || host.ends_with(".githubcopilot.com")
        || (host == "chatgpt.com" && uri.path().starts_with("/backend-api/codex"))
}

pub fn headers_contain_proxy_auth_placeholder(headers: &HeaderMap) -> bool {
    headers.values().any(|value| {
        value
            .to_str()
            .map(|value| value.contains(PROXY_AUTH_PLACEHOLDER))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        codex_oauth_access_token_expires_at_ms, codex_oauth_device_code_expires_at_ms,
        codex_oauth_device_code_expires_in_secs, codex_oauth_device_code_request_failure,
        codex_oauth_device_auth_token_request_body, codex_oauth_device_auth_token_url,
        codex_oauth_device_auth_usercode_url, codex_oauth_device_poll_status_kind,
        codex_oauth_device_poll_failure, codex_oauth_device_usercode_request_body,
        codex_oauth_device_verification_url, codex_oauth_authorization_code_form,
        codex_oauth_missing_account_id_message, codex_oauth_missing_pending_user_code_message,
        codex_oauth_missing_refresh_token_message, codex_oauth_pending_device_code_is_expired,
        codex_oauth_poll_interval_secs, codex_oauth_refresh_failure, codex_oauth_refresh_token_form,
        codex_oauth_token_exchange_failure, codex_oauth_token_is_expiring_soon,
        codex_oauth_token_url,
        copilot_oauth_poll_error_kind, copilot_token_is_expiring_soon,
        ensure_managed_auth_provider, headers_contain_proxy_auth_placeholder,
        is_managed_account_upstream_url,
        managed_account_app_handle_unavailable_error_message,
        managed_account_app_handle_unavailable_log_message,
        managed_account_token_failure_error_message, managed_account_token_failure_log_message,
        managed_account_token_request_log_message, managed_account_token_success_log_message,
        managed_account_auth_plan, managed_provider_auth_info_for_provider_kind,
        provider_kind_is_codex_oauth, provider_kind_is_github_copilot,
        provider_kind_uses_managed_account_auth,
        resolve_copilot_dynamic_base_url_for_binding_with_runtime_source,
        resolve_copilot_dynamic_base_url_with_runtime_source,
        resolve_copilot_live_model_for_binding_with_runtime_source,
        resolve_copilot_live_model_with_runtime_source,
        resolve_copilot_model_vendor_for_binding_with_runtime_source,
        resolve_copilot_model_vendor_with_runtime_source,
        resolve_managed_account_auth_for_binding_with_runtime_source,
        resolve_managed_account_auth_with_runtime_source, validate_managed_account_upstream_auth,
        CodexOAuthDevicePollStatusKind, CopilotOAuthPollErrorKind, ManagedAccountAuthError,
        ManagedAccountAuthPlan, ManagedAccountAuthResolution, ManagedAccountAuthRuntime,
        ManagedAccountBindingInput, ManagedAccountBindingSource, ManagedAccountRuntimeSource,
        CODEX_OAUTH_AUTH_PLACEHOLDER, CODEX_OAUTH_AUTH_PROVIDER,
        GITHUB_COPILOT_AUTH_PLACEHOLDER, GITHUB_COPILOT_AUTH_PROVIDER, PROXY_AUTH_PLACEHOLDER,
    };
    use futures::{executor::block_on, future::BoxFuture};
    use http::{HeaderMap, HeaderValue, StatusCode};

    use crate::copilot_model_map::CopilotModel;
    use crate::domain::ProviderKind;
    use crate::provider_auth::{ProviderAuthInfo, ProviderAuthStrategy};

    struct StaticManagedRuntimeSource;

    #[test]
    fn copilot_oauth_token_expiry_policy_uses_refresh_buffer() {
        let now = 1_771_000_000;

        assert!(!copilot_token_is_expiring_soon(now + 3_600, now));
        assert!(copilot_token_is_expiring_soon(now + 30, now));
        assert!(copilot_token_is_expiring_soon(now - 1, now));
        assert!(!copilot_token_is_expiring_soon(now + 60, now));
        assert!(copilot_token_is_expiring_soon(now + 59, now));
    }

    #[test]
    fn copilot_oauth_poll_error_codes_map_to_contract_kinds() {
        assert_eq!(
            copilot_oauth_poll_error_kind("authorization_pending", None),
            CopilotOAuthPollErrorKind::AuthorizationPending
        );
        assert_eq!(
            copilot_oauth_poll_error_kind("slow_down", Some("wait")),
            CopilotOAuthPollErrorKind::AuthorizationPending
        );
        assert_eq!(
            copilot_oauth_poll_error_kind("expired_token", None),
            CopilotOAuthPollErrorKind::ExpiredToken
        );
        assert_eq!(
            copilot_oauth_poll_error_kind("access_denied", None),
            CopilotOAuthPollErrorKind::AccessDenied
        );
        assert_eq!(
            copilot_oauth_poll_error_kind("unknown", Some("detail")),
            CopilotOAuthPollErrorKind::NetworkError("unknown: detail".to_string())
        );
        assert_eq!(
            copilot_oauth_poll_error_kind("unknown", None),
            CopilotOAuthPollErrorKind::NetworkError("unknown: ".to_string())
        );
    }

    #[test]
    fn codex_oauth_token_expiry_policy_uses_refresh_buffer() {
        let now = 1_771_000_000_000;

        assert!(!codex_oauth_token_is_expiring_soon(now + 3_600_000, now));
        assert!(codex_oauth_token_is_expiring_soon(now + 30_000, now));
        assert!(codex_oauth_token_is_expiring_soon(now - 1, now));
        assert!(!codex_oauth_token_is_expiring_soon(now + 60_000, now));
        assert!(codex_oauth_token_is_expiring_soon(now + 59_999, now));
    }

    #[test]
    fn codex_oauth_device_and_access_expiry_policies_use_defaults() {
        let now = 1_771_000_000_000;

        assert_eq!(codex_oauth_device_code_expires_in_secs(Some(120)), 120);
        assert_eq!(codex_oauth_device_code_expires_in_secs(None), 900);
        assert_eq!(
            codex_oauth_device_code_expires_at_ms(Some(120), now),
            now + 120_000
        );
        assert_eq!(
            codex_oauth_device_code_expires_at_ms(None, now),
            now + 900_000
        );
        assert_eq!(
            codex_oauth_access_token_expires_at_ms(Some(1800), now),
            now + 1_800_000
        );
        assert_eq!(
            codex_oauth_access_token_expires_at_ms(None, now),
            now + 3_600_000
        );
        assert!(codex_oauth_pending_device_code_is_expired(now, now));
        assert!(codex_oauth_pending_device_code_is_expired(now - 1, now));
        assert!(!codex_oauth_pending_device_code_is_expired(now + 1, now));
    }

    #[test]
    fn codex_oauth_poll_interval_handles_server_shapes() {
        assert_eq!(
            codex_oauth_poll_interval_secs(Some(&serde_json::Value::Number(
                serde_json::Number::from(5)
            ))),
            8
        );
        assert_eq!(
            codex_oauth_poll_interval_secs(Some(&serde_json::Value::String("10".to_string()))),
            13
        );
        assert_eq!(codex_oauth_poll_interval_secs(None), 8);
        assert_eq!(
            codex_oauth_poll_interval_secs(Some(&serde_json::Value::Number(
                serde_json::Number::from(0)
            ))),
            4
        );
        assert_eq!(
            codex_oauth_poll_interval_secs(Some(&serde_json::Value::String("bad".to_string()))),
            8
        );
    }

    #[test]
    fn codex_oauth_device_poll_status_maps_http_contract() {
        assert_eq!(
            codex_oauth_device_poll_status_kind(StatusCode::FORBIDDEN),
            CodexOAuthDevicePollStatusKind::AuthorizationPending
        );
        assert_eq!(
            codex_oauth_device_poll_status_kind(StatusCode::NOT_FOUND),
            CodexOAuthDevicePollStatusKind::AuthorizationPending
        );
        assert_eq!(
            codex_oauth_device_poll_status_kind(StatusCode::GONE),
            CodexOAuthDevicePollStatusKind::ExpiredToken
        );
        assert_eq!(
            codex_oauth_device_poll_status_kind(StatusCode::OK),
            CodexOAuthDevicePollStatusKind::Success
        );
        assert_eq!(
            codex_oauth_device_poll_status_kind(StatusCode::INTERNAL_SERVER_ERROR),
            CodexOAuthDevicePollStatusKind::Failed
        );
    }

    #[test]
    fn codex_oauth_request_contract_builds_urls_bodies_and_forms() {
        assert_eq!(
            codex_oauth_device_auth_usercode_url(),
            "https://auth.openai.com/api/accounts/deviceauth/usercode"
        );
        assert_eq!(
            codex_oauth_device_auth_token_url(),
            "https://auth.openai.com/api/accounts/deviceauth/token"
        );
        assert_eq!(
            codex_oauth_token_url(),
            "https://auth.openai.com/oauth/token"
        );
        assert_eq!(
            codex_oauth_device_verification_url(),
            "https://auth.openai.com/codex/device"
        );
        assert_eq!(
            codex_oauth_device_usercode_request_body(),
            serde_json::json!({ "client_id": "app_EMoamEEZ73f0CkXaXp7hrann" })
        );
        assert_eq!(
            codex_oauth_device_auth_token_request_body("device-123", "USER-456"),
            serde_json::json!({
                "device_auth_id": "device-123",
                "user_code": "USER-456",
            })
        );
        assert_eq!(
            codex_oauth_authorization_code_form("code-123", "verifier-456"),
            [
                ("grant_type", "authorization_code"),
                ("code", "code-123"),
                ("redirect_uri", "https://auth.openai.com/deviceauth/callback"),
                ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
                ("code_verifier", "verifier-456"),
            ]
        );
        assert_eq!(
            codex_oauth_refresh_token_form("refresh-123"),
            [
                ("grant_type", "refresh_token"),
                ("refresh_token", "refresh-123"),
                ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
                ("scope", "openid profile email"),
            ]
        );
    }

    #[test]
    fn codex_oauth_request_failure_messages_match_legacy_text() {
        assert_eq!(
            codex_oauth_device_code_request_failure(StatusCode::BAD_GATEWAY, "upstream"),
            "Device Code 请求失败: 502 Bad Gateway - upstream"
        );
        assert_eq!(
            codex_oauth_device_poll_failure(StatusCode::BAD_REQUEST, "pending weirdly"),
            "400 Bad Request - pending weirdly"
        );
        assert_eq!(
            codex_oauth_token_exchange_failure(StatusCode::UNAUTHORIZED, "bad"),
            "Token 交换失败: 401 Unauthorized - bad"
        );
        assert_eq!(
            codex_oauth_refresh_failure(StatusCode::FORBIDDEN, "revoked"),
            "Refresh 失败: 403 Forbidden - revoked"
        );
        assert_eq!(
            codex_oauth_missing_pending_user_code_message(),
            "未找到对应的 user_code，请重新启动登录流程"
        );
        assert_eq!(
            codex_oauth_missing_refresh_token_message(),
            "响应缺少 refresh_token"
        );
        assert_eq!(
            codex_oauth_missing_account_id_message(),
            "无法从 token 中提取 account_id"
        );
    }

    #[test]
    fn managed_auth_provider_validation_accepts_supported_providers() {
        assert_eq!(
            ensure_managed_auth_provider("github_copilot").unwrap(),
            GITHUB_COPILOT_AUTH_PROVIDER
        );
        assert_eq!(
            ensure_managed_auth_provider("codex_oauth").unwrap(),
            CODEX_OAUTH_AUTH_PROVIDER
        );
        assert_eq!(
            ensure_managed_auth_provider("unknown").unwrap_err(),
            "Unsupported auth provider: unknown"
        );
    }

    impl ManagedAccountRuntimeSource for StaticManagedRuntimeSource {
        type Error = String;

        fn resolve_copilot_auth<'a>(
            &'a self,
            account_id: Option<&'a str>,
            runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<ProviderAuthInfo, Self::Error>> {
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
        ) -> BoxFuture<'a, Result<(ProviderAuthInfo, Option<String>), Self::Error>> {
            Box::pin(async move {
                let account_id = account_id.unwrap_or_else(|| "codex-default".to_string());
                Ok((
                    ProviderAuthInfo::new(
                        format!("codex-token:{account_id}"),
                        runtime.provider_auth_strategy(),
                    ),
                    Some(account_id),
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

    struct StaticCopilotRuntimeSource {
        endpoint: Option<String>,
        models: Option<Vec<CopilotModel>>,
        vendor: Option<String>,
    }

    impl ManagedAccountRuntimeSource for StaticCopilotRuntimeSource {
        type Error = String;

        fn resolve_copilot_auth<'a>(
            &'a self,
            _account_id: Option<&'a str>,
            _runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<ProviderAuthInfo, Self::Error>> {
            Box::pin(async move { Err("auth not used".to_string()) })
        }

        fn resolve_codex_oauth<'a>(
            &'a self,
            _account_id: Option<String>,
            _runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<(ProviderAuthInfo, Option<String>), Self::Error>> {
            Box::pin(async move { Err("oauth not used".to_string()) })
        }

        fn resolve_copilot_api_endpoint<'a>(
            &'a self,
            account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move {
                assert_eq!(account_id, Some("copilot-account"));
                self.endpoint.clone()
            })
        }

        fn fetch_copilot_live_models<'a>(
            &'a self,
            account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Option<Vec<CopilotModel>>, String>> {
            Box::pin(async move {
                assert_eq!(account_id, Some("copilot-account"));
                Ok(self.models.clone())
            })
        }

        fn resolve_copilot_model_vendor<'a>(
            &'a self,
            account_id: Option<&'a str>,
            model_id: &'a str,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move {
                assert_eq!(account_id, Some("copilot-account"));
                assert_eq!(model_id, "claude-sonnet-4");
                self.vendor.clone()
            })
        }
    }

    #[test]
    fn managed_account_plan_passes_through_non_runtime_auth() {
        let auth = ProviderAuthInfo::new("sk-test".to_string(), ProviderAuthStrategy::Bearer);

        let plan = managed_account_auth_plan(
            auth.clone(),
            Some("copilot-account".to_string()),
            Some("codex-account".to_string()),
        );

        assert_eq!(plan, ManagedAccountAuthPlan::Passthrough { auth });
        assert!(!plan.should_send_codex_oauth_session_headers());
    }

    #[test]
    fn managed_account_plan_resolves_copilot_runtime_token() {
        let auth = ProviderAuthInfo::new(
            PROXY_AUTH_PLACEHOLDER.to_string(),
            ProviderAuthStrategy::GitHubCopilot,
        );

        let plan = managed_account_auth_plan(auth, Some("copilot-account".to_string()), None);

        assert_eq!(
            plan,
            ManagedAccountAuthPlan::ResolveRuntimeToken {
                runtime: ManagedAccountAuthRuntime::GitHubCopilot,
                account_id: Some("copilot-account".to_string()),
            }
        );
        assert!(!plan.should_send_codex_oauth_session_headers());
        assert_eq!(
            ManagedAccountAuthRuntime::GitHubCopilot.provider_auth_strategy(),
            ProviderAuthStrategy::GitHubCopilot
        );
        let auth = ManagedAccountAuthRuntime::GitHubCopilot.provider_auth_info("token".to_string());
        assert_eq!(auth.api_key, "token");
        assert_eq!(auth.strategy, ProviderAuthStrategy::GitHubCopilot);
    }

    #[test]
    fn managed_account_plan_resolves_codex_runtime_token_with_session_headers() {
        let auth = ProviderAuthInfo::new(
            PROXY_AUTH_PLACEHOLDER.to_string(),
            ProviderAuthStrategy::CodexOAuth,
        );

        let plan = managed_account_auth_plan(auth, None, Some("codex-account".to_string()));

        assert_eq!(
            plan,
            ManagedAccountAuthPlan::ResolveRuntimeToken {
                runtime: ManagedAccountAuthRuntime::CodexOAuth,
                account_id: Some("codex-account".to_string()),
            }
        );
        assert!(plan.should_send_codex_oauth_session_headers());
        assert_eq!(
            ManagedAccountAuthRuntime::CodexOAuth.provider_auth_strategy(),
            ProviderAuthStrategy::CodexOAuth
        );
        let auth = ManagedAccountAuthRuntime::CodexOAuth.provider_auth_info("token".to_string());
        assert_eq!(auth.api_key, "token");
        assert_eq!(auth.strategy, ProviderAuthStrategy::CodexOAuth);
    }

    #[test]
    fn managed_account_runtime_messages_preserve_host_contract() {
        assert_eq!(
            managed_account_app_handle_unavailable_log_message(
                ManagedAccountAuthRuntime::GitHubCopilot
            ),
            "[Copilot] AppHandle 不可用"
        );
        assert_eq!(
            managed_account_app_handle_unavailable_error_message(
                ManagedAccountAuthRuntime::GitHubCopilot
            ),
            "GitHub Copilot 认证不可用（无 AppHandle）"
        );
        assert_eq!(
            managed_account_token_request_log_message(
                ManagedAccountAuthRuntime::GitHubCopilot,
                Some("acct-1"),
            ),
            "[Copilot] 使用指定账号 acct-1 获取 token"
        );
        assert_eq!(
            managed_account_token_success_log_message(
                ManagedAccountAuthRuntime::GitHubCopilot,
                None,
            ),
            "[Copilot] 成功获取 Copilot token (account=default)"
        );
        assert_eq!(
            managed_account_token_failure_log_message(
                ManagedAccountAuthRuntime::GitHubCopilot,
                Some("acct-1"),
                "expired",
            ),
            "[Copilot] 获取 Copilot token 失败 (account=acct-1): expired"
        );
        assert_eq!(
            managed_account_token_failure_error_message(
                ManagedAccountAuthRuntime::GitHubCopilot,
                "expired",
            ),
            "GitHub Copilot 认证失败: expired"
        );

        assert_eq!(
            managed_account_app_handle_unavailable_log_message(
                ManagedAccountAuthRuntime::CodexOAuth
            ),
            "[CodexOAuth] AppHandle 不可用"
        );
        assert_eq!(
            managed_account_app_handle_unavailable_error_message(
                ManagedAccountAuthRuntime::CodexOAuth
            ),
            "Codex OAuth 认证不可用（无 AppHandle）"
        );
        assert_eq!(
            managed_account_token_request_log_message(ManagedAccountAuthRuntime::CodexOAuth, None),
            "[CodexOAuth] 使用默认账号获取 token"
        );
        assert_eq!(
            managed_account_token_success_log_message(
                ManagedAccountAuthRuntime::CodexOAuth,
                Some("codex-default"),
            ),
            "[CodexOAuth] 成功获取 access_token (account=codex-default)"
        );
        assert_eq!(
            managed_account_token_failure_log_message(
                ManagedAccountAuthRuntime::CodexOAuth,
                Some("codex-default"),
                "revoked",
            ),
            "[CodexOAuth] 获取 access_token 失败: revoked"
        );
        assert_eq!(
            managed_account_token_failure_error_message(
                ManagedAccountAuthRuntime::CodexOAuth,
                "revoked",
            ),
            "Codex OAuth 认证失败: revoked"
        );
    }

    #[test]
    fn managed_account_resolution_contract_preserves_runtime_session_facts() {
        let passthrough = ManagedAccountAuthResolution::passthrough(ProviderAuthInfo::new(
            "sk-test".to_string(),
            ProviderAuthStrategy::Bearer,
        ));
        assert_eq!(passthrough.auth.api_key, "sk-test");
        assert_eq!(passthrough.codex_oauth_account_id, None);
        assert!(!passthrough.should_send_codex_oauth_session_headers);

        let runtime = ManagedAccountAuthResolution::runtime_token(
            ProviderAuthInfo::new(
                "runtime-token".to_string(),
                ProviderAuthStrategy::CodexOAuth,
            ),
            Some("codex-account".to_string()),
            true,
        );
        assert_eq!(runtime.auth.api_key, "runtime-token");
        assert_eq!(
            runtime.codex_oauth_account_id.as_deref(),
            Some("codex-account")
        );
        assert!(runtime.should_send_codex_oauth_session_headers);
    }

    #[test]
    fn managed_account_binding_input_resolves_current_and_legacy_account_ids() {
        let github_binding = ManagedAccountBindingInput {
            source: ManagedAccountBindingSource::ManagedAccount,
            auth_provider: Some(GITHUB_COPILOT_AUTH_PROVIDER),
            account_id: Some("copilot-account"),
        };
        assert_eq!(
            super::managed_account_id_for_auth_provider(
                GITHUB_COPILOT_AUTH_PROVIDER,
                Some(github_binding),
                Some("legacy-account"),
            )
            .as_deref(),
            Some("copilot-account")
        );

        let codex_binding = ManagedAccountBindingInput {
            source: ManagedAccountBindingSource::ManagedAccount,
            auth_provider: Some(CODEX_OAUTH_AUTH_PROVIDER),
            account_id: Some("codex-account"),
        };
        assert_eq!(
            super::managed_account_id_for_auth_provider(
                CODEX_OAUTH_AUTH_PROVIDER,
                Some(codex_binding),
                Some("legacy-account"),
            )
            .as_deref(),
            Some("codex-account")
        );

        assert_eq!(
            super::managed_account_id_for_auth_provider(
                GITHUB_COPILOT_AUTH_PROVIDER,
                None,
                Some("legacy-account"),
            )
            .as_deref(),
            Some("legacy-account")
        );
    }

    #[test]
    fn managed_account_binding_input_preserves_source_and_empty_account_semantics() {
        let provider_config_binding = ManagedAccountBindingInput {
            source: ManagedAccountBindingSource::ProviderConfig,
            auth_provider: Some(GITHUB_COPILOT_AUTH_PROVIDER),
            account_id: Some("ignored-account"),
        };
        assert_eq!(
            super::managed_account_id_for_auth_provider(
                GITHUB_COPILOT_AUTH_PROVIDER,
                Some(provider_config_binding),
                Some("legacy-account"),
            )
            .as_deref(),
            Some("legacy-account")
        );

        let default_account_binding = ManagedAccountBindingInput {
            source: ManagedAccountBindingSource::ManagedAccount,
            auth_provider: Some(GITHUB_COPILOT_AUTH_PROVIDER),
            account_id: None,
        };
        assert_eq!(
            super::managed_account_id_for_auth_provider(
                GITHUB_COPILOT_AUTH_PROVIDER,
                Some(default_account_binding),
                Some("legacy-account"),
            ),
            None
        );
    }

    #[test]
    fn provider_kind_uses_managed_account_auth_matches_runtime_provider_facts() {
        assert!(provider_kind_uses_managed_account_auth(
            Some(&ProviderKind::GitHubCopilot),
            None,
        ));
        assert!(provider_kind_uses_managed_account_auth(
            Some(&ProviderKind::CodexOAuth),
            None,
        ));
        assert!(provider_kind_uses_managed_account_auth(
            None,
            Some("https://api.githubcopilot.com"),
        ));
        assert!(provider_kind_uses_managed_account_auth(
            Some(&ProviderKind::Claude),
            Some("https://chatgpt.com/backend-api/codex"),
        ));
        assert!(!provider_kind_uses_managed_account_auth(
            Some(&ProviderKind::Claude),
            Some("https://api.anthropic.com"),
        ));
    }

    #[test]
    fn provider_kind_runtime_classification_matches_meta_and_base_url_facts() {
        assert!(provider_kind_is_github_copilot(
            Some(&ProviderKind::GitHubCopilot),
            None,
        ));
        assert!(provider_kind_is_github_copilot(
            Some(&ProviderKind::Claude),
            Some("https://api.githubcopilot.com"),
        ));
        assert!(!provider_kind_is_github_copilot(
            Some(&ProviderKind::CodexOAuth),
            Some("https://chatgpt.com/backend-api/codex"),
        ));

        assert!(provider_kind_is_codex_oauth(Some(
            &ProviderKind::CodexOAuth
        )));
        assert!(!provider_kind_is_codex_oauth(Some(
            &ProviderKind::GitHubCopilot
        )));
        assert!(!provider_kind_is_codex_oauth(None));
    }

    #[test]
    fn managed_provider_auth_info_uses_runtime_placeholder_contract() {
        let copilot = managed_provider_auth_info_for_provider_kind(&ProviderKind::GitHubCopilot)
            .expect("copilot auth info");
        assert_eq!(copilot.api_key, GITHUB_COPILOT_AUTH_PLACEHOLDER);
        assert_eq!(copilot.strategy, ProviderAuthStrategy::GitHubCopilot);
        assert_eq!(copilot.access_token, None);

        let codex = managed_provider_auth_info_for_provider_kind(&ProviderKind::CodexOAuth)
            .expect("codex oauth auth info");
        assert_eq!(codex.api_key, CODEX_OAUTH_AUTH_PLACEHOLDER);
        assert_eq!(codex.strategy, ProviderAuthStrategy::CodexOAuth);
        assert_eq!(codex.access_token, None);

        assert!(managed_provider_auth_info_for_provider_kind(&ProviderKind::Claude).is_none());
    }

    #[test]
    fn managed_account_runtime_source_resolution_passes_through_non_runtime_auth() {
        let source = StaticManagedRuntimeSource;
        let auth = ProviderAuthInfo::new("sk-test".to_string(), ProviderAuthStrategy::Bearer);

        let resolved = block_on(resolve_managed_account_auth_with_runtime_source(
            &source,
            auth.clone(),
            Some("copilot-account".to_string()),
            Some("codex-account".to_string()),
        ))
        .expect("managed auth passthrough");

        assert_eq!(resolved, ManagedAccountAuthResolution::passthrough(auth));
    }

    #[test]
    fn managed_account_runtime_source_resolution_uses_bound_accounts() {
        let source = StaticManagedRuntimeSource;

        let copilot = block_on(resolve_managed_account_auth_with_runtime_source(
            &source,
            ProviderAuthInfo::new(
                PROXY_AUTH_PLACEHOLDER.to_string(),
                ProviderAuthStrategy::GitHubCopilot,
            ),
            Some("copilot-account".to_string()),
            Some("codex-account".to_string()),
        ))
        .expect("copilot auth resolution");
        assert_eq!(copilot.auth.api_key, "copilot-token:copilot-account");
        assert_eq!(copilot.auth.strategy, ProviderAuthStrategy::GitHubCopilot);
        assert_eq!(copilot.codex_oauth_account_id, None);
        assert!(!copilot.should_send_codex_oauth_session_headers);

        let codex = block_on(resolve_managed_account_auth_with_runtime_source(
            &source,
            ProviderAuthInfo::new(
                PROXY_AUTH_PLACEHOLDER.to_string(),
                ProviderAuthStrategy::CodexOAuth,
            ),
            Some("copilot-account".to_string()),
            Some("codex-account".to_string()),
        ))
        .expect("codex oauth resolution");
        assert_eq!(codex.auth.api_key, "codex-token:codex-account");
        assert_eq!(codex.auth.strategy, ProviderAuthStrategy::CodexOAuth);
        assert_eq!(
            codex.codex_oauth_account_id.as_deref(),
            Some("codex-account")
        );
        assert!(codex.should_send_codex_oauth_session_headers);
    }

    #[test]
    fn managed_account_runtime_source_resolution_uses_binding_input_accounts() {
        let source = StaticManagedRuntimeSource;

        let copilot = block_on(
            resolve_managed_account_auth_for_binding_with_runtime_source(
                &source,
                ProviderAuthInfo::new(
                    PROXY_AUTH_PLACEHOLDER.to_string(),
                    ProviderAuthStrategy::GitHubCopilot,
                ),
                Some(ManagedAccountBindingInput {
                    source: ManagedAccountBindingSource::ManagedAccount,
                    auth_provider: Some(GITHUB_COPILOT_AUTH_PROVIDER),
                    account_id: Some("copilot-binding-account"),
                }),
                Some("legacy-account"),
            ),
        )
        .expect("copilot auth from binding");
        assert_eq!(
            copilot.auth.api_key,
            "copilot-token:copilot-binding-account"
        );

        let codex = block_on(
            resolve_managed_account_auth_for_binding_with_runtime_source(
                &source,
                ProviderAuthInfo::new(
                    PROXY_AUTH_PLACEHOLDER.to_string(),
                    ProviderAuthStrategy::CodexOAuth,
                ),
                Some(ManagedAccountBindingInput {
                    source: ManagedAccountBindingSource::ManagedAccount,
                    auth_provider: Some(CODEX_OAUTH_AUTH_PROVIDER),
                    account_id: Some("codex-binding-account"),
                }),
                Some("legacy-account"),
            ),
        )
        .expect("codex auth from binding");
        assert_eq!(codex.auth.api_key, "codex-token:codex-binding-account");
        assert_eq!(
            codex.codex_oauth_account_id.as_deref(),
            Some("codex-binding-account")
        );
        assert!(codex.should_send_codex_oauth_session_headers);

        let legacy = block_on(
            resolve_managed_account_auth_for_binding_with_runtime_source(
                &source,
                ProviderAuthInfo::new(
                    PROXY_AUTH_PLACEHOLDER.to_string(),
                    ProviderAuthStrategy::GitHubCopilot,
                ),
                None,
                Some("legacy-account"),
            ),
        )
        .expect("legacy copilot auth");
        assert_eq!(legacy.auth.api_key, "copilot-token:legacy-account");
    }

    #[test]
    fn managed_account_runtime_source_resolves_copilot_dynamic_base_url() {
        let source = StaticCopilotRuntimeSource {
            endpoint: Some("https://api.enterprise.githubcopilot.com".to_string()),
            models: None,
            vendor: None,
        };

        let base_url = block_on(resolve_copilot_dynamic_base_url_with_runtime_source(
            &source,
            Some("copilot-account"),
            "https://api.githubcopilot.com",
            true,
            false,
        ));

        assert_eq!(
            base_url.as_deref(),
            Some("https://api.enterprise.githubcopilot.com")
        );
    }

    #[test]
    fn managed_account_runtime_source_resolves_copilot_live_model_ids() {
        let source = StaticCopilotRuntimeSource {
            endpoint: None,
            models: Some(vec![CopilotModel {
                id: "claude-sonnet-4.6".to_string(),
                name: "Claude Sonnet 4.6".to_string(),
                vendor: "Anthropic".to_string(),
                model_picker_enabled: true,
            }]),
            vendor: None,
        };

        let model = block_on(resolve_copilot_live_model_with_runtime_source(
            &source,
            Some("copilot-account"),
            "claude-sonnet-4-6",
        ))
        .expect("live model source");

        assert_eq!(model.as_deref(), Some("claude-sonnet-4.6"));
    }

    #[test]
    fn managed_account_runtime_source_resolves_copilot_model_vendor_when_enabled() {
        let source = StaticCopilotRuntimeSource {
            endpoint: None,
            models: None,
            vendor: Some("Anthropic".to_string()),
        };

        let vendor = block_on(resolve_copilot_model_vendor_with_runtime_source(
            &source,
            Some("copilot-account"),
            "claude-sonnet-4",
            true,
        ));
        assert_eq!(vendor.as_deref(), Some("Anthropic"));

        let skipped = block_on(resolve_copilot_model_vendor_with_runtime_source(
            &source,
            Some("copilot-account"),
            "claude-sonnet-4",
            false,
        ));
        assert_eq!(skipped, None);
    }

    #[test]
    fn managed_account_runtime_source_resolves_copilot_facts_from_binding_input() {
        let source = StaticCopilotRuntimeSource {
            endpoint: Some("https://api.enterprise.githubcopilot.com".to_string()),
            models: Some(vec![CopilotModel {
                id: "claude-sonnet-4.6".to_string(),
                name: "Claude Sonnet 4.6".to_string(),
                vendor: "Anthropic".to_string(),
                model_picker_enabled: true,
            }]),
            vendor: Some("Anthropic".to_string()),
        };
        let binding = Some(ManagedAccountBindingInput {
            source: ManagedAccountBindingSource::ManagedAccount,
            auth_provider: Some(GITHUB_COPILOT_AUTH_PROVIDER),
            account_id: Some("copilot-account"),
        });

        let base_url = block_on(
            resolve_copilot_dynamic_base_url_for_binding_with_runtime_source(
                &source,
                binding,
                Some("legacy-account"),
                "https://api.githubcopilot.com",
                true,
                false,
            ),
        );
        assert_eq!(
            base_url.as_deref(),
            Some("https://api.enterprise.githubcopilot.com")
        );

        let model = block_on(resolve_copilot_live_model_for_binding_with_runtime_source(
            &source,
            binding,
            Some("legacy-account"),
            "claude-sonnet-4-6",
        ))
        .expect("live model source");
        assert_eq!(model.as_deref(), Some("claude-sonnet-4.6"));

        let vendor = block_on(
            resolve_copilot_model_vendor_for_binding_with_runtime_source(
                &source,
                binding,
                Some("legacy-account"),
                "claude-sonnet-4",
                true,
            ),
        );
        assert_eq!(vendor.as_deref(), Some("Anthropic"));
    }

    #[test]
    fn managed_account_url_detection_covers_copilot_and_codex_hosts() {
        assert!(is_managed_account_upstream_url(
            "https://api.githubcopilot.com/chat/completions"
        ));
        assert!(is_managed_account_upstream_url(
            "https://githubcopilot.com/chat/completions"
        ));
        assert!(is_managed_account_upstream_url(
            "https://chatgpt.com/backend-api/codex/responses"
        ));
        assert!(!is_managed_account_upstream_url(
            "https://chatgpt.com/public-api/responses"
        ));
        assert!(!is_managed_account_upstream_url(
            "https://api.example.com/v1/messages"
        ));
        assert!(!is_managed_account_upstream_url("not a uri"));
    }

    #[test]
    fn header_scan_detects_proxy_managed_placeholder_values() {
        let mut headers = HeaderMap::new();
        assert!(!headers_contain_proxy_auth_placeholder(&headers));

        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );
        assert!(headers_contain_proxy_auth_placeholder(&headers));
    }

    #[test]
    fn managed_account_guard_rejects_unresolved_placeholder_only_for_managed_hosts() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );

        assert_eq!(
            validate_managed_account_upstream_auth(
                "https://api.githubcopilot.com/chat/completions",
                &headers,
            )
            .unwrap_err(),
            ManagedAccountAuthError::PlaceholderForwarded
        );
        validate_managed_account_upstream_auth("https://api.example.com/v1/messages", &headers)
            .expect("non-managed upstreams are outside this guard");
    }
}
