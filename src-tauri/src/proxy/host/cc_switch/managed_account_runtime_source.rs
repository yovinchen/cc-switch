use std::sync::Arc;

use futures::future::BoxFuture;
use serde_json::Value;
use tauri::Manager;
use tokio::sync::Mutex;

use crate::commands::{CodexOAuthState, CopilotAuthState};
#[cfg(test)]
use crate::provider::{AuthBinding, AuthBindingSource, Provider, ProviderMeta};
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
    ManagedAccountTokenCacheKey, ManagedAccountTokenRefreshFailureInput,
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
        input: ManagedAccountTokenRefreshFailureInput,
    ) -> Result<ManagedAccountTokenSnapshot, ProxyError> {
        let resolution = {
            let now_ms = self.current_time_ms();
            let snapshots = self.token_snapshots.lock().await;
            snapshots.resolve_refresh_failure(key, now_ms, input)
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

fn copilot_token_refresh_failure_input(
    error: &CopilotAuthError,
) -> ManagedAccountTokenRefreshFailureInput {
    if matches!(
        error,
        CopilotAuthError::CopilotTokenFetchFailed(_)
            | CopilotAuthError::NetworkError(_)
            | CopilotAuthError::ParseError(_)
            | CopilotAuthError::IoError(_)
    ) {
        ManagedAccountTokenRefreshFailureInput::retryable(error.to_string())
    } else {
        ManagedAccountTokenRefreshFailureInput::terminal(error.to_string())
    }
}

fn codex_oauth_token_refresh_failure_input(
    error: &CodexOAuthError,
) -> ManagedAccountTokenRefreshFailureInput {
    if matches!(
        error,
        CodexOAuthError::TokenFetchFailed(_)
            | CodexOAuthError::NetworkError(_)
            | CodexOAuthError::ParseError(_)
            | CodexOAuthError::IoError(_)
    ) {
        ManagedAccountTokenRefreshFailureInput::retryable(error.to_string())
    } else {
        ManagedAccountTokenRefreshFailureInput::terminal(error.to_string())
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
                    let failure_input = copilot_token_refresh_failure_input(&error);
                    self.resolve_token_refresh_failure(&cache_key, failure_input)
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
                    let failure_input = codex_oauth_token_refresh_failure_input(&error);
                    self.resolve_token_refresh_failure(&cache_key, failure_input)
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
pub(crate) struct StaticCopilotModelsSource {
    pub(crate) endpoint: Option<String>,
    pub(crate) models: Option<Vec<CopilotModel>>,
}

#[cfg(test)]
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
    ) -> BoxFuture<'a, Result<CodexOAuthResolution, ProxyError>> {
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

#[cfg(test)]
pub(crate) struct StaticManagedAuthResolutionSource;

#[cfg(test)]
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
    ) -> BoxFuture<'a, Result<CodexOAuthResolution, ProxyError>> {
        Box::pin(async move {
            let resolved_account_id = account_id.unwrap_or_else(|| "codex-default".to_string());
            Ok(CodexOAuthResolution::new(
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

#[cfg(test)]
pub(crate) fn managed_account_test_provider_with_binding(
    auth_provider: &str,
    account_id: &str,
) -> Provider {
    let mut provider = Provider::with_id(
        format!("{auth_provider}-provider"),
        "Managed Account Provider".to_string(),
        serde_json::json!({}),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::host::cc_switch::provider_projection::provider_claude_api_format;
    use crate::proxy_core::api::auth::{
        ManagedAccountTokenRefreshFailureKind, ProviderAuthStrategy,
    };

    fn runtime_source_with_fixed_token_clock(now_ms: i64) -> CcSwitchManagedAccountRuntimeSource {
        CcSwitchManagedAccountRuntimeSource::new_with_token_cache_clock(
            None,
            Arc::new(move || now_ms),
        )
    }

    #[test]
    fn copilot_token_refresh_failure_input_names_retryable_failures() {
        for error in [
            CopilotAuthError::CopilotTokenFetchFailed("upstream 502".to_string()),
            CopilotAuthError::NetworkError("timeout".to_string()),
            CopilotAuthError::ParseError("bad json".to_string()),
            CopilotAuthError::IoError("disk busy".to_string()),
        ] {
            assert_eq!(
                copilot_token_refresh_failure_input(&error).failure_kind,
                ManagedAccountTokenRefreshFailureKind::Retryable
            );
            assert_eq!(
                copilot_token_refresh_failure_input(&error).error,
                error.to_string()
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
                copilot_token_refresh_failure_input(&error).failure_kind,
                ManagedAccountTokenRefreshFailureKind::Terminal
            );
            assert_eq!(
                copilot_token_refresh_failure_input(&error).error,
                error.to_string()
            );
        }
    }

    #[test]
    fn codex_oauth_token_refresh_failure_input_names_retryable_failures() {
        for error in [
            CodexOAuthError::TokenFetchFailed("upstream 502".to_string()),
            CodexOAuthError::NetworkError("timeout".to_string()),
            CodexOAuthError::ParseError("bad json".to_string()),
            CodexOAuthError::IoError("disk busy".to_string()),
        ] {
            assert_eq!(
                codex_oauth_token_refresh_failure_input(&error).failure_kind,
                ManagedAccountTokenRefreshFailureKind::Retryable
            );
            assert_eq!(
                codex_oauth_token_refresh_failure_input(&error).error,
                error.to_string()
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
                codex_oauth_token_refresh_failure_input(&error).failure_kind,
                ManagedAccountTokenRefreshFailureKind::Terminal
            );
            assert_eq!(
                codex_oauth_token_refresh_failure_input(&error).error,
                error.to_string()
            );
        }
    }

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

    #[tokio::test]
    async fn runtime_source_resolves_provider_account_bindings() {
        let source = StaticManagedAuthResolutionSource;
        let copilot_provider =
            managed_account_test_provider_with_binding("github_copilot", "copilot-acct");
        let codex_provider =
            managed_account_test_provider_with_binding("codex_oauth", "codex-acct");
        let copilot_binding_context = provider_managed_account_binding_context(&copilot_provider);
        let codex_binding_context = provider_managed_account_binding_context(&codex_provider);

        let copilot = source
            .resolve_auth_for_binding(ManagedAccountAuthForBindingInput {
                binding_facts: ManagedAccountRuntimeBindingFacts::new(
                    copilot_binding_context.binding,
                    copilot_binding_context.legacy_github_copilot_account_id,
                ),
                auth: ProviderAuthInfo::new(
                    "PROXY_MANAGED".to_string(),
                    ProviderAuthStrategy::GitHubCopilot,
                ),
            })
            .await
            .expect("copilot managed auth");
        assert_eq!(copilot.auth.api_key, "copilot-token:copilot-acct");
        assert_eq!(copilot.auth.strategy, ProviderAuthStrategy::GitHubCopilot);
        assert_eq!(copilot.codex_oauth_account_id, None);
        assert!(!copilot.should_send_codex_oauth_session_headers);

        let codex = source
            .resolve_auth_for_binding(ManagedAccountAuthForBindingInput {
                binding_facts: ManagedAccountRuntimeBindingFacts::new(
                    codex_binding_context.binding,
                    codex_binding_context.legacy_github_copilot_account_id,
                ),
                auth: ProviderAuthInfo::new(
                    "PROXY_MANAGED".to_string(),
                    ProviderAuthStrategy::CodexOAuth,
                ),
            })
            .await
            .expect("codex managed auth");
        assert_eq!(codex.auth.api_key, "codex-token:codex-acct");
        assert_eq!(codex.auth.strategy, ProviderAuthStrategy::CodexOAuth);
        assert_eq!(codex.codex_oauth_account_id.as_deref(), Some("codex-acct"));
        assert!(codex.should_send_codex_oauth_session_headers);
    }

    #[tokio::test]
    async fn runtime_source_gates_copilot_live_model_by_adapter() {
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
            serde_json::json!({}),
            None,
        );
        let mut body = serde_json::json!({ "model": "claude-sonnet-4-6" });
        let binding_context = provider_managed_account_binding_context(&provider);

        source
            .apply_copilot_live_model_for_binding_adapter(
                ManagedAccountAdapterCopilotLiveModelForBindingInput {
                    binding_facts: ManagedAccountRuntimeBindingFacts::new(
                        binding_context.binding,
                        binding_context.legacy_github_copilot_account_id,
                    ),
                    body: &mut body,
                    is_copilot: false,
                },
            )
            .await;

        assert_eq!(body["model"], "claude-sonnet-4-6");

        source
            .apply_copilot_live_model_for_binding_adapter(
                ManagedAccountAdapterCopilotLiveModelForBindingInput {
                    binding_facts: ManagedAccountRuntimeBindingFacts::new(
                        binding_context.binding,
                        binding_context.legacy_github_copilot_account_id,
                    ),
                    body: &mut body,
                    is_copilot: true,
                },
            )
            .await;

        assert_eq!(body["model"], "claude-sonnet-4.6");
    }

    #[tokio::test]
    async fn runtime_source_applies_copilot_dynamic_base_url() {
        let source = StaticCopilotModelsSource {
            endpoint: Some("https://api.enterprise.githubcopilot.com".to_string()),
            models: None,
        };
        let provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            serde_json::json!({}),
            None,
        );
        let mut base_url = "https://api.githubcopilot.com".to_string();
        let binding_context = provider_managed_account_binding_context(&provider);

        source
            .apply_copilot_dynamic_base_url_for_binding(
                ManagedAccountApplyCopilotDynamicBaseUrlForBindingInput {
                    binding_facts: ManagedAccountRuntimeBindingFacts::new(
                        binding_context.binding,
                        binding_context.legacy_github_copilot_account_id,
                    ),
                    base_url: &mut base_url,
                    is_copilot: true,
                    is_full_url: false,
                },
            )
            .await;

        assert_eq!(base_url, "https://api.enterprise.githubcopilot.com");
    }

    #[tokio::test]
    async fn runtime_source_gates_claude_api_format_by_adapter() {
        let source = StaticCopilotModelsSource {
            endpoint: None,
            models: None,
        };
        let provider = Provider::with_id(
            "claude".to_string(),
            "Claude".to_string(),
            serde_json::json!({
                "api_format": "openai_chat"
            }),
            None,
        );
        let body = serde_json::json!({ "model": "claude-sonnet-4" });
        let binding_context = provider_managed_account_binding_context(&provider);

        assert_eq!(
            source
                .resolve_claude_api_format_for_binding_adapter(
                    ManagedAccountAdapterClaudeApiFormatForBindingInput {
                        binding_facts: ManagedAccountRuntimeBindingFacts::new(
                            binding_context.binding,
                            binding_context.legacy_github_copilot_account_id,
                        ),
                        provider_api_format: provider_claude_api_format(&provider),
                        body: &body,
                        is_copilot: false,
                        is_claude_adapter: false,
                    },
                )
                .await,
            None
        );
        assert_eq!(
            source
                .resolve_claude_api_format_for_binding_adapter(
                    ManagedAccountAdapterClaudeApiFormatForBindingInput {
                        binding_facts: ManagedAccountRuntimeBindingFacts::new(
                            binding_context.binding,
                            binding_context.legacy_github_copilot_account_id,
                        ),
                        provider_api_format: provider_claude_api_format(&provider),
                        body: &body,
                        is_copilot: false,
                        is_claude_adapter: true,
                    },
                )
                .await
                .as_deref(),
            Some("openai_chat")
        );
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
                ManagedAccountTokenRefreshFailureInput::new(
                    ManagedAccountTokenRefreshFailureKind::Retryable,
                    "network timeout",
                ),
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
                ManagedAccountTokenRefreshFailureInput::new(
                    ManagedAccountTokenRefreshFailureKind::Retryable,
                    "network timeout",
                ),
            )
            .await
            .is_err());
        assert!(source
            .resolve_token_refresh_failure(
                &copilot_other_account,
                ManagedAccountTokenRefreshFailureInput::new(
                    ManagedAccountTokenRefreshFailureKind::Retryable,
                    "network timeout",
                ),
            )
            .await
            .is_err());

        let snapshot = source
            .resolve_token_refresh_failure(
                &copilot_account,
                ManagedAccountTokenRefreshFailureInput::new(
                    ManagedAccountTokenRefreshFailureKind::Retryable,
                    "network timeout",
                ),
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
                ManagedAccountTokenRefreshFailureInput::new(
                    ManagedAccountTokenRefreshFailureKind::Retryable,
                    "network timeout",
                ),
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
                ManagedAccountTokenRefreshFailureInput::new(
                    ManagedAccountTokenRefreshFailureKind::Terminal,
                    "refresh token revoked",
                ),
            )
            .await
            .is_err());
    }
}
