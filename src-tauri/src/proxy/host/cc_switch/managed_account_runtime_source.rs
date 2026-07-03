use std::sync::Arc;

use futures::future::BoxFuture;
use serde_json::Value;
use tauri::Manager;
use tokio::sync::Mutex;

use crate::commands::{CodexOAuthState, CopilotAuthState};
#[cfg(test)]
use crate::provider::Provider;
use crate::proxy::codex_oauth_auth::CodexOAuthError;
use crate::proxy::copilot_auth::CopilotAuthError;
use crate::proxy::error::ProxyError;
#[cfg(test)]
use crate::proxy::host::cc_switch::provider_projection::provider_managed_account_binding_context;
use crate::proxy_core::api::auth::{
    managed_account_app_handle_unavailable_error_message,
    managed_account_app_handle_unavailable_log_message, managed_account_token_request_log_message,
    resolve_copilot_dynamic_base_url_for_binding_with_runtime_source as resolve_core_copilot_dynamic_base_url_for_binding_with_runtime_source,
    resolve_copilot_live_model_for_binding_with_runtime_source as resolve_core_copilot_live_model_for_binding_with_runtime_source,
    resolve_copilot_model_vendor_for_binding_with_runtime_source as resolve_core_copilot_model_vendor_for_binding_with_runtime_source,
    resolve_managed_account_auth_for_binding_with_runtime_source as resolve_core_managed_account_auth_for_binding_with_runtime_source,
    CodexOAuthResolution, ManagedAccountAuthResolution, ManagedAccountAuthRuntime,
    ManagedAccountBindingInput, ManagedAccountRuntimeSource as CoreManagedAccountRuntimeSource,
    ManagedAccountTokenCacheKey, ManagedAccountTokenRefreshFailureKind,
    ManagedAccountTokenRefreshFailureResolution, ManagedAccountTokenRefreshSuccess,
    ManagedAccountTokenRefreshSuccessInput, ManagedAccountTokenSnapshot,
    ManagedAccountTokenSnapshotStore, ProviderAuthInfo,
};
use crate::proxy_core::api::model_catalog::CopilotModel;
use crate::proxy_core::api::transforms::resolve_claude_forward_api_format;

pub(crate) type ManagedAccountRuntimeSourceRef = Arc<dyn ManagedAccountRuntimeSource + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ManagedAccountRuntimeBindingFacts<'a> {
    pub(crate) binding: Option<ManagedAccountBindingInput<'a>>,
    pub(crate) legacy_github_copilot_account_id: Option<&'a str>,
}

impl<'a> ManagedAccountRuntimeBindingFacts<'a> {
    pub(crate) fn new(
        binding: Option<ManagedAccountBindingInput<'a>>,
        legacy_github_copilot_account_id: Option<&'a str>,
    ) -> Self {
        Self {
            binding,
            legacy_github_copilot_account_id,
        }
    }
}

pub(crate) struct ManagedAccountAuthForBindingInput<'a> {
    pub(crate) binding_facts: ManagedAccountRuntimeBindingFacts<'a>,
    pub(crate) auth: ProviderAuthInfo,
}

pub(crate) struct ManagedAccountCopilotDynamicBaseUrlForBindingInput<'a> {
    pub(crate) binding_facts: ManagedAccountRuntimeBindingFacts<'a>,
    pub(crate) current_base_url: &'a str,
    pub(crate) is_copilot: bool,
    pub(crate) is_full_url: bool,
}

pub(crate) struct ManagedAccountApplyCopilotDynamicBaseUrlForBindingInput<'a> {
    pub(crate) binding_facts: ManagedAccountRuntimeBindingFacts<'a>,
    pub(crate) base_url: &'a mut String,
    pub(crate) is_copilot: bool,
    pub(crate) is_full_url: bool,
}

pub(crate) struct ManagedAccountCopilotLiveModelForBindingInput<'a> {
    pub(crate) binding_facts: ManagedAccountRuntimeBindingFacts<'a>,
    pub(crate) model_id: &'a str,
}

pub(crate) struct ManagedAccountApplyCopilotLiveModelForBindingInput<'a> {
    pub(crate) binding_facts: ManagedAccountRuntimeBindingFacts<'a>,
    pub(crate) body: &'a mut Value,
}

pub(crate) struct ManagedAccountAdapterCopilotLiveModelForBindingInput<'a> {
    pub(crate) binding_facts: ManagedAccountRuntimeBindingFacts<'a>,
    pub(crate) body: &'a mut Value,
    pub(crate) is_copilot: bool,
}

pub(crate) struct ManagedAccountClaudeApiFormatForBindingInput<'a> {
    pub(crate) binding_facts: ManagedAccountRuntimeBindingFacts<'a>,
    pub(crate) provider_api_format: &'a str,
    pub(crate) body: &'a Value,
    pub(crate) is_copilot: bool,
}

pub(crate) struct ManagedAccountAdapterClaudeApiFormatForBindingInput<'a> {
    pub(crate) binding_facts: ManagedAccountRuntimeBindingFacts<'a>,
    pub(crate) provider_api_format: &'a str,
    pub(crate) body: &'a Value,
    pub(crate) is_copilot: bool,
    pub(crate) is_claude_adapter: bool,
}

pub(crate) struct CcSwitchManagedAccountRuntimeSource {
    app_handle: Option<tauri::AppHandle>,
    token_snapshots: Arc<Mutex<ManagedAccountTokenSnapshotStore>>,
    token_cache_clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl CcSwitchManagedAccountRuntimeSource {
    fn new(app_handle: Option<tauri::AppHandle>) -> Self {
        Self::new_with_token_cache_clock(
            app_handle,
            Arc::new(|| chrono::Utc::now().timestamp_millis()),
        )
    }

    fn new_with_token_cache_clock(
        app_handle: Option<tauri::AppHandle>,
        token_cache_clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        Self {
            app_handle,
            token_snapshots: Arc::new(Mutex::new(ManagedAccountTokenSnapshotStore::new())),
            token_cache_clock,
        }
    }

    fn current_time_ms(&self) -> i64 {
        (self.token_cache_clock)()
    }

    async fn record_token_refresh_success(
        &self,
        key: ManagedAccountTokenCacheKey,
        input: ManagedAccountTokenRefreshSuccessInput,
    ) -> ManagedAccountTokenRefreshSuccess {
        let now_ms = self.current_time_ms();
        let mut snapshots = self.token_snapshots.lock().await;
        snapshots.record_refresh_success(key, input, now_ms)
    }

    async fn resolve_token_refresh_failure(
        &self,
        key: &ManagedAccountTokenCacheKey,
        error: &str,
        failure_kind: ManagedAccountTokenRefreshFailureKind,
    ) -> Result<ManagedAccountTokenSnapshot, ProxyError> {
        let resolution = {
            let now_ms = self.current_time_ms();
            let snapshots = self.token_snapshots.lock().await;
            snapshots.resolve_refresh_failure(key, now_ms, failure_kind, error)
        };

        match resolution {
            ManagedAccountTokenRefreshFailureResolution::UseCachedToken {
                snapshot,
                log_message,
            } => {
                log::warn!("{log_message}");
                Ok(snapshot)
            }
            ManagedAccountTokenRefreshFailureResolution::Reject {
                log_message,
                error_message,
            } => {
                log::error!("{log_message}");
                Err(ProxyError::AuthError(error_message))
            }
        }
    }
}

pub(crate) fn managed_account_runtime_source_from_app_handle(
    app_handle: Option<tauri::AppHandle>,
) -> ManagedAccountRuntimeSourceRef {
    Arc::new(CcSwitchManagedAccountRuntimeSource::new(app_handle))
}

#[cfg(test)]
pub(crate) fn default_managed_account_runtime_source() -> ManagedAccountRuntimeSourceRef {
    managed_account_runtime_source_from_app_handle(None)
}

fn forwarder_claude_api_format_for_provider(
    provider_api_format: &str,
    is_copilot: bool,
    copilot_model_vendor: Option<&str>,
) -> String {
    resolve_claude_forward_api_format(provider_api_format, is_copilot, copilot_model_vendor)
}

pub(crate) trait ManagedAccountRuntimeSource:
    CoreManagedAccountRuntimeSource<Error = ProxyError> + Send + Sync
{
    fn resolve_auth_for_binding<'a>(
        &'a self,
        input: ManagedAccountAuthForBindingInput<'a>,
    ) -> BoxFuture<'a, Result<ManagedAccountAuthResolution, ProxyError>> {
        Box::pin(async move {
            resolve_core_managed_account_auth_for_binding_with_runtime_source(
                self,
                input.auth,
                input.binding_facts.binding,
                input.binding_facts.legacy_github_copilot_account_id,
            )
            .await
        })
    }

    fn resolve_copilot_dynamic_base_url_for_binding<'a>(
        &'a self,
        input: ManagedAccountCopilotDynamicBaseUrlForBindingInput<'a>,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            resolve_core_copilot_dynamic_base_url_for_binding_with_runtime_source(
                self,
                input.binding_facts.binding,
                input.binding_facts.legacy_github_copilot_account_id,
                input.current_base_url,
                input.is_copilot,
                input.is_full_url,
            )
            .await
        })
    }

    fn apply_copilot_dynamic_base_url_for_binding<'a>(
        &'a self,
        input: ManagedAccountApplyCopilotDynamicBaseUrlForBindingInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let Some(next_base_url) = self
                .resolve_copilot_dynamic_base_url_for_binding(
                    ManagedAccountCopilotDynamicBaseUrlForBindingInput {
                        binding_facts: input.binding_facts,
                        current_base_url: input.base_url,
                        is_copilot: input.is_copilot,
                        is_full_url: input.is_full_url,
                    },
                )
                .await
            else {
                return;
            };

            log::debug!(
                "[Copilot] 使用动态 API endpoint: {} (原: {})",
                next_base_url,
                input.base_url
            );
            *input.base_url = next_base_url;
        })
    }

    fn resolve_copilot_live_model_for_binding<'a>(
        &'a self,
        input: ManagedAccountCopilotLiveModelForBindingInput<'a>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        Box::pin(async move {
            resolve_core_copilot_live_model_for_binding_with_runtime_source(
                self,
                input.binding_facts.binding,
                input.binding_facts.legacy_github_copilot_account_id,
                input.model_id,
            )
            .await
        })
    }

    fn apply_copilot_live_model_for_binding<'a>(
        &'a self,
        input: ManagedAccountApplyCopilotLiveModelForBindingInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let Some(model_id) = input.body.get("model").and_then(Value::as_str) else {
                return;
            };
            let model_id = model_id.to_string();

            let resolved = match self
                .resolve_copilot_live_model_for_binding(
                    ManagedAccountCopilotLiveModelForBindingInput {
                        binding_facts: input.binding_facts,
                        model_id: &model_id,
                    },
                )
                .await
            {
                Ok(Some(resolved)) => resolved,
                Ok(None) => return,
                Err(err) => {
                    log::debug!("[Copilot] live model list unavailable, skip resolution: {err}");
                    return;
                }
            };

            log::info!("[Copilot] live-model resolve: {model_id} → {resolved}");
            input.body["model"] = Value::String(resolved);
        })
    }

    fn apply_copilot_live_model_for_binding_adapter<'a>(
        &'a self,
        input: ManagedAccountAdapterCopilotLiveModelForBindingInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if !input.is_copilot {
                return;
            }

            self.apply_copilot_live_model_for_binding(
                ManagedAccountApplyCopilotLiveModelForBindingInput {
                    binding_facts: input.binding_facts,
                    body: input.body,
                },
            )
            .await;
        })
    }

    fn resolve_claude_api_format_for_binding<'a>(
        &'a self,
        input: ManagedAccountClaudeApiFormatForBindingInput<'a>,
    ) -> BoxFuture<'a, String> {
        Box::pin(async move {
            let model = input.body.get("model").and_then(Value::as_str);
            let copilot_model_vendor = if let Some(model_id) = model {
                resolve_core_copilot_model_vendor_for_binding_with_runtime_source(
                    self,
                    input.binding_facts.binding,
                    input.binding_facts.legacy_github_copilot_account_id,
                    model_id,
                    input.is_copilot,
                )
                .await
            } else {
                None
            };

            forwarder_claude_api_format_for_provider(
                input.provider_api_format,
                input.is_copilot,
                copilot_model_vendor.as_deref(),
            )
        })
    }

    fn resolve_claude_api_format_for_binding_adapter<'a>(
        &'a self,
        input: ManagedAccountAdapterClaudeApiFormatForBindingInput<'a>,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            if !input.is_claude_adapter {
                return None;
            }

            Some(
                self.resolve_claude_api_format_for_binding(
                    ManagedAccountClaudeApiFormatForBindingInput {
                        binding_facts: input.binding_facts,
                        provider_api_format: input.provider_api_format,
                        body: input.body,
                        is_copilot: input.is_copilot,
                    },
                )
                .await,
            )
        })
    }
}

impl<T> ManagedAccountRuntimeSource for T where
    T: CoreManagedAccountRuntimeSource<Error = ProxyError> + Send + Sync
{
}

pub(crate) async fn copilot_api_endpoint_from_app_handle(
    app_handle: Option<&tauri::AppHandle>,
    account_id: Option<&str>,
) -> Option<String> {
    let app_handle = app_handle?;
    let copilot_state = app_handle.state::<CopilotAuthState>();
    let copilot_auth = copilot_state.0.read().await;

    Some(match account_id {
        Some(id) => copilot_auth.get_api_endpoint(id).await,
        None => copilot_auth.get_default_api_endpoint().await,
    })
}

pub(crate) async fn copilot_live_models_from_app_handle(
    app_handle: Option<&tauri::AppHandle>,
    account_id: Option<&str>,
) -> Result<Option<Vec<CopilotModel>>, String> {
    let Some(app_handle) = app_handle else {
        return Ok(None);
    };

    let copilot_state = app_handle.state::<CopilotAuthState>();
    let copilot_auth = copilot_state.0.read().await;

    match account_id {
        Some(id) => copilot_auth.fetch_models_for_account(id).await,
        None => copilot_auth.fetch_models().await,
    }
    .map(Some)
    .map_err(|error| error.to_string())
}

pub(crate) async fn copilot_model_vendor_from_app_handle(
    app_handle: Option<&tauri::AppHandle>,
    account_id: Option<&str>,
    model_id: &str,
) -> Option<String> {
    let Some(app_handle) = app_handle else {
        log::debug!("[Copilot] AppHandle unavailable, fallback to chat/completions");
        return None;
    };

    let copilot_state = app_handle.state::<CopilotAuthState>();
    let copilot_auth = copilot_state.0.read().await;

    let vendor_result = match account_id {
        Some(id) => {
            copilot_auth
                .get_model_vendor_for_account(id, model_id)
                .await
        }
        None => copilot_auth.get_model_vendor(model_id).await,
    };

    match vendor_result {
        Ok(Some(vendor)) => Some(vendor),
        Ok(None) => {
            log::debug!(
                "[Copilot] Model vendor unavailable for {model_id}, fallback to chat/completions"
            );
            None
        }
        Err(error) => {
            log::warn!(
                "[Copilot] Failed to resolve model vendor for {model_id}, fallback to chat/completions: {error}"
            );
            None
        }
    }
}

async fn copilot_refresh_success_from_app_handle(
    app_handle: &tauri::AppHandle,
    account_id: Option<&str>,
) -> Result<ManagedAccountTokenRefreshSuccessInput, CopilotAuthError> {
    let copilot_state = app_handle.state::<CopilotAuthState>();
    let copilot_auth = copilot_state.0.read().await;

    let token_result = match account_id {
        Some(id) => copilot_auth.get_valid_token_for_account(id).await,
        None => copilot_auth.get_valid_token().await,
    };

    token_result.map(|token| ManagedAccountTokenRefreshSuccessInput::copilot(token, account_id))
}

async fn codex_oauth_refresh_success_from_app_handle(
    app_handle: &tauri::AppHandle,
    account_id: Option<&str>,
) -> Result<ManagedAccountTokenRefreshSuccessInput, CodexOAuthError> {
    let codex_state = app_handle.state::<CodexOAuthState>();
    let codex_auth = codex_state.0.read().await;

    let token_result = match account_id {
        Some(id) => codex_auth.get_valid_token_for_account(id).await,
        None => codex_auth.get_valid_token().await,
    };

    match token_result {
        Ok(token) => {
            let resolved_account_id = match account_id {
                Some(id) => Some(id.to_string()),
                None => codex_auth.default_account_id().await,
            };
            Ok(ManagedAccountTokenRefreshSuccessInput::codex_oauth(
                token,
                resolved_account_id,
            ))
        }
        Err(error) => Err(error),
    }
}

fn copilot_token_failure_kind(error: &CopilotAuthError) -> ManagedAccountTokenRefreshFailureKind {
    if matches!(
        error,
        CopilotAuthError::CopilotTokenFetchFailed(_)
            | CopilotAuthError::NetworkError(_)
            | CopilotAuthError::ParseError(_)
            | CopilotAuthError::IoError(_)
    ) {
        ManagedAccountTokenRefreshFailureKind::Retryable
    } else {
        ManagedAccountTokenRefreshFailureKind::Terminal
    }
}

fn codex_oauth_token_failure_kind(
    error: &CodexOAuthError,
) -> ManagedAccountTokenRefreshFailureKind {
    if matches!(
        error,
        CodexOAuthError::TokenFetchFailed(_)
            | CodexOAuthError::NetworkError(_)
            | CodexOAuthError::ParseError(_)
            | CodexOAuthError::IoError(_)
    ) {
        ManagedAccountTokenRefreshFailureKind::Retryable
    } else {
        ManagedAccountTokenRefreshFailureKind::Terminal
    }
}

impl CoreManagedAccountRuntimeSource for CcSwitchManagedAccountRuntimeSource {
    type Error = ProxyError;

    fn resolve_copilot_auth<'a>(
        &'a self,
        account_id: Option<&'a str>,
        runtime: ManagedAccountAuthRuntime,
    ) -> BoxFuture<'a, Result<ProviderAuthInfo, ProxyError>> {
        Box::pin(async move {
            let Some(app_handle) = self.app_handle.as_ref() else {
                log::error!(
                    "{}",
                    managed_account_app_handle_unavailable_log_message(runtime)
                );
                return Err(ProxyError::AuthError(
                    managed_account_app_handle_unavailable_error_message(runtime),
                ));
            };

            log::debug!(
                "{}",
                managed_account_token_request_log_message(runtime, account_id)
            );

            let cache_key = ManagedAccountTokenCacheKey::new(runtime, account_id);
            match copilot_refresh_success_from_app_handle(app_handle, account_id).await {
                Ok(refresh_success) => {
                    let success = self
                        .record_token_refresh_success(cache_key, refresh_success)
                        .await;
                    log::debug!("{}", success.log_message);
                    Ok(success.auth)
                }
                Err(error) => {
                    let failure_kind = copilot_token_failure_kind(&error);
                    let error = error.to_string();
                    self.resolve_token_refresh_failure(&cache_key, &error, failure_kind)
                        .await
                        .map(|snapshot| snapshot.auth)
                }
            }
        })
    }

    fn resolve_codex_oauth<'a>(
        &'a self,
        account_id: Option<String>,
        runtime: ManagedAccountAuthRuntime,
    ) -> BoxFuture<'a, Result<CodexOAuthResolution, ProxyError>> {
        Box::pin(async move {
            let Some(app_handle) = self.app_handle.as_ref() else {
                log::error!(
                    "{}",
                    managed_account_app_handle_unavailable_log_message(runtime)
                );
                return Err(ProxyError::AuthError(
                    managed_account_app_handle_unavailable_error_message(runtime),
                ));
            };

            log::debug!(
                "{}",
                managed_account_token_request_log_message(runtime, account_id.as_deref())
            );

            let cache_key = ManagedAccountTokenCacheKey::new(runtime, account_id.as_deref());
            match codex_oauth_refresh_success_from_app_handle(app_handle, account_id.as_deref())
                .await
            {
                Ok(refresh_success) => {
                    let success = self
                        .record_token_refresh_success(cache_key, refresh_success)
                        .await;
                    log::debug!("{}", success.log_message);
                    Ok(CodexOAuthResolution::from_refresh_success(success))
                }
                Err(error) => {
                    let failure_kind = codex_oauth_token_failure_kind(&error);
                    let error = error.to_string();
                    self.resolve_token_refresh_failure(&cache_key, &error, failure_kind)
                        .await
                        .map(CodexOAuthResolution::from_snapshot)
                }
            }
        })
    }

    fn resolve_copilot_api_endpoint<'a>(
        &'a self,
        account_id: Option<&'a str>,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            copilot_api_endpoint_from_app_handle(self.app_handle.as_ref(), account_id).await
        })
    }

    fn fetch_copilot_live_models<'a>(
        &'a self,
        account_id: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Option<Vec<CopilotModel>>, String>> {
        Box::pin(async move {
            copilot_live_models_from_app_handle(self.app_handle.as_ref(), account_id).await
        })
    }

    fn resolve_copilot_model_vendor<'a>(
        &'a self,
        account_id: Option<&'a str>,
        model_id: &'a str,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            copilot_model_vendor_from_app_handle(self.app_handle.as_ref(), account_id, model_id)
                .await
        })
    }
}

#[cfg(test)]
pub(crate) async fn resolve_managed_account_auth_from_runtime_source(
    runtime_source: &(dyn ManagedAccountRuntimeSource + Send + Sync),
    auth_provider: &Provider,
    auth: ProviderAuthInfo,
) -> Result<ManagedAccountAuthResolution, ProxyError> {
    let binding_context = provider_managed_account_binding_context(auth_provider);
    runtime_source
        .resolve_auth_for_binding(ManagedAccountAuthForBindingInput {
            binding_facts: ManagedAccountRuntimeBindingFacts::new(
                binding_context.binding,
                binding_context.legacy_github_copilot_account_id,
            ),
            auth,
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_source_with_fixed_token_clock(now_ms: i64) -> CcSwitchManagedAccountRuntimeSource {
        CcSwitchManagedAccountRuntimeSource::new_with_token_cache_clock(
            None,
            Arc::new(move || now_ms),
        )
    }

    #[test]
    fn copilot_token_failure_kind_allows_only_transient_refresh_failures() {
        for error in [
            CopilotAuthError::CopilotTokenFetchFailed("upstream 502".to_string()),
            CopilotAuthError::NetworkError("timeout".to_string()),
            CopilotAuthError::ParseError("bad json".to_string()),
            CopilotAuthError::IoError("disk busy".to_string()),
        ] {
            assert_eq!(
                copilot_token_failure_kind(&error),
                ManagedAccountTokenRefreshFailureKind::Retryable
            );
        }

        for error in [
            CopilotAuthError::DeviceFlowNotStarted,
            CopilotAuthError::AuthorizationPending,
            CopilotAuthError::AccessDenied,
            CopilotAuthError::ExpiredToken,
            CopilotAuthError::GitHubTokenInvalid,
            CopilotAuthError::NoCopilotSubscription,
            CopilotAuthError::AccountNotFound("acct".to_string()),
            CopilotAuthError::InvalidDomain("example.invalid".to_string()),
        ] {
            assert_eq!(
                copilot_token_failure_kind(&error),
                ManagedAccountTokenRefreshFailureKind::Terminal
            );
        }
    }

    #[test]
    fn codex_oauth_token_failure_kind_allows_only_transient_refresh_failures() {
        for error in [
            CodexOAuthError::TokenFetchFailed("upstream 502".to_string()),
            CodexOAuthError::NetworkError("timeout".to_string()),
            CodexOAuthError::ParseError("bad json".to_string()),
            CodexOAuthError::IoError("disk busy".to_string()),
        ] {
            assert_eq!(
                codex_oauth_token_failure_kind(&error),
                ManagedAccountTokenRefreshFailureKind::Retryable
            );
        }

        for error in [
            CodexOAuthError::AuthorizationPending,
            CodexOAuthError::AccessDenied,
            CodexOAuthError::ExpiredToken,
            CodexOAuthError::RefreshTokenInvalid,
            CodexOAuthError::AccountNotFound("acct".to_string()),
        ] {
            assert_eq!(
                codex_oauth_token_failure_kind(&error),
                ManagedAccountTokenRefreshFailureKind::Terminal
            );
        }
    }

    #[tokio::test]
    async fn token_snapshot_falls_back_for_recent_retryable_failure() {
        let source = runtime_source_with_fixed_token_clock(42_000);
        let key = ManagedAccountTokenCacheKey::new(
            ManagedAccountAuthRuntime::GitHubCopilot,
            Some("acct"),
        );
        source
            .record_token_refresh_success(
                key.clone(),
                ManagedAccountTokenRefreshSuccessInput::copilot("cached".to_string(), Some("acct")),
            )
            .await;

        let snapshot = source
            .resolve_token_refresh_failure(
                &key,
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .expect("recent retryable failure should use cached token");

        assert_eq!(snapshot.auth.api_key, "cached");
        assert_eq!(snapshot.cached_at_ms, 42_000);
        assert_eq!(
            snapshot.auth.strategy,
            ManagedAccountAuthRuntime::GitHubCopilot.provider_auth_strategy()
        );
    }

    #[tokio::test]
    async fn token_snapshot_cache_is_scoped_by_runtime_and_account() {
        let source = runtime_source_with_fixed_token_clock(42_000);
        let copilot_account = ManagedAccountTokenCacheKey::new(
            ManagedAccountAuthRuntime::GitHubCopilot,
            Some("acct-a"),
        );
        let codex_same_account =
            ManagedAccountTokenCacheKey::new(ManagedAccountAuthRuntime::CodexOAuth, Some("acct-a"));
        let copilot_other_account = ManagedAccountTokenCacheKey::new(
            ManagedAccountAuthRuntime::GitHubCopilot,
            Some("acct-b"),
        );

        source
            .record_token_refresh_success(
                copilot_account.clone(),
                ManagedAccountTokenRefreshSuccessInput::copilot(
                    "copilot-a".to_string(),
                    Some("acct-a"),
                ),
            )
            .await;

        assert!(source
            .resolve_token_refresh_failure(
                &codex_same_account,
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .is_err());
        assert!(source
            .resolve_token_refresh_failure(
                &copilot_other_account,
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .is_err());

        let snapshot = source
            .resolve_token_refresh_failure(
                &copilot_account,
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .expect("matching runtime/account should use cached token");
        assert_eq!(snapshot.auth.api_key, "copilot-a");
    }

    #[tokio::test]
    async fn token_snapshot_does_not_hide_non_retryable_or_stale_failures() {
        let source = runtime_source_with_fixed_token_clock(42_000);
        let key =
            ManagedAccountTokenCacheKey::new(ManagedAccountAuthRuntime::CodexOAuth, Some("acct"));
        {
            let mut snapshots = source.token_snapshots.lock().await;
            snapshots.record_snapshot(
                key.clone(),
                ManagedAccountTokenSnapshot::new(
                    ManagedAccountAuthRuntime::CodexOAuth.provider_auth_info("cached".to_string()),
                    Some("acct".to_string()),
                    11_999,
                ),
            );
        }

        assert!(source
            .resolve_token_refresh_failure(
                &key,
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .is_err());

        source
            .record_token_refresh_success(
                key.clone(),
                ManagedAccountTokenRefreshSuccessInput::codex_oauth(
                    "fresh".to_string(),
                    Some("acct".to_string()),
                ),
            )
            .await;

        assert!(source
            .resolve_token_refresh_failure(
                &key,
                "refresh token revoked",
                ManagedAccountTokenRefreshFailureKind::Terminal,
            )
            .await
            .is_err());
    }
}
