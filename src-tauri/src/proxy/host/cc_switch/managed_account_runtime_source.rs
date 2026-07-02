use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;
use serde_json::Value;
use tauri::Manager;
use tokio::sync::Mutex;

use crate::commands::{CodexOAuthState, CopilotAuthState};
use crate::provider::Provider;
use crate::proxy::codex_oauth_auth::CodexOAuthError;
use crate::proxy::copilot_auth::CopilotAuthError;
use crate::proxy::error::ProxyError;
use crate::proxy::host::cc_switch::provider_projection::provider_managed_account_binding_context;
use crate::proxy::provider::claude_provider_api_format;
use crate::proxy_core::api::auth::{
    managed_account_app_handle_unavailable_error_message,
    managed_account_app_handle_unavailable_log_message,
    managed_account_token_failure_error_message, managed_account_token_failure_fallback_decision,
    managed_account_token_failure_fallback_log_message, managed_account_token_failure_log_message,
    managed_account_token_request_log_message, managed_account_token_success_log_message,
    resolve_copilot_dynamic_base_url_for_binding_with_runtime_source as resolve_core_copilot_dynamic_base_url_for_binding_with_runtime_source,
    resolve_copilot_live_model_for_binding_with_runtime_source as resolve_core_copilot_live_model_for_binding_with_runtime_source,
    resolve_copilot_model_vendor_for_binding_with_runtime_source as resolve_core_copilot_model_vendor_for_binding_with_runtime_source,
    resolve_managed_account_auth_for_binding_with_runtime_source as resolve_core_managed_account_auth_for_binding_with_runtime_source,
    ManagedAccountAuthResolution, ManagedAccountAuthRuntime,
    ManagedAccountRuntimeSource as CoreManagedAccountRuntimeSource, ManagedAccountTokenCacheKey,
    ManagedAccountTokenRefreshFailureKind, ManagedAccountTokenSnapshot, ProviderAuthInfo,
};
use crate::proxy_core::api::model_catalog::CopilotModel;
use crate::proxy_core::api::transforms::resolve_claude_forward_api_format;

pub(crate) type ManagedAccountRuntimeSourceRef = Arc<dyn ManagedAccountRuntimeSource + Send + Sync>;

pub(crate) struct CcSwitchManagedAccountRuntimeSource {
    app_handle: Option<tauri::AppHandle>,
    token_snapshots: Arc<Mutex<HashMap<ManagedAccountTokenCacheKey, ManagedAccountTokenSnapshot>>>,
}

impl CcSwitchManagedAccountRuntimeSource {
    fn new(app_handle: Option<tauri::AppHandle>) -> Self {
        Self {
            app_handle,
            token_snapshots: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn record_token_snapshot(
        &self,
        key: ManagedAccountTokenCacheKey,
        auth: ProviderAuthInfo,
        codex_oauth_account_id: Option<String>,
    ) {
        let mut snapshots = self.token_snapshots.lock().await;
        snapshots.insert(
            key,
            ManagedAccountTokenSnapshot::new(
                auth,
                codex_oauth_account_id,
                chrono::Utc::now().timestamp_millis(),
            ),
        );
    }

    async fn token_snapshot_for_refresh_failure(
        &self,
        key: &ManagedAccountTokenCacheKey,
        account_id: Option<&str>,
        error: &str,
        failure_kind: ManagedAccountTokenRefreshFailureKind,
    ) -> Option<ManagedAccountTokenSnapshot> {
        let snapshot = {
            let snapshots = self.token_snapshots.lock().await;
            snapshots.get(key).cloned()
        }?;
        let decision = managed_account_token_failure_fallback_decision(
            Some(snapshot.cached_at_ms),
            chrono::Utc::now().timestamp_millis(),
            failure_kind,
        );
        if !decision.should_use_cached_token {
            return None;
        }

        log::warn!(
            "{}",
            managed_account_token_failure_fallback_log_message(
                key.runtime(),
                account_id,
                decision.cached_token_age_ms.unwrap_or_default(),
                error,
            )
        );
        Some(snapshot)
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
    provider: &Provider,
    is_copilot: bool,
    copilot_model_vendor: Option<&str>,
) -> String {
    resolve_claude_forward_api_format(
        claude_provider_api_format(provider),
        is_copilot,
        copilot_model_vendor,
    )
}

pub(crate) trait ManagedAccountRuntimeSource:
    CoreManagedAccountRuntimeSource<Error = ProxyError> + Send + Sync
{
    fn resolve_auth_for_provider<'a>(
        &'a self,
        auth_provider: &'a Provider,
        auth: ProviderAuthInfo,
    ) -> BoxFuture<'a, Result<ManagedAccountAuthResolution, ProxyError>> {
        Box::pin(async move {
            let binding_context = provider_managed_account_binding_context(auth_provider);
            resolve_core_managed_account_auth_for_binding_with_runtime_source(
                self,
                auth,
                binding_context.binding,
                binding_context.legacy_github_copilot_account_id,
            )
            .await
        })
    }

    fn resolve_copilot_dynamic_base_url_for_provider<'a>(
        &'a self,
        auth_provider: &'a Provider,
        current_base_url: &'a str,
        is_copilot: bool,
        is_full_url: bool,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            let binding_context = provider_managed_account_binding_context(auth_provider);
            resolve_core_copilot_dynamic_base_url_for_binding_with_runtime_source(
                self,
                binding_context.binding,
                binding_context.legacy_github_copilot_account_id,
                current_base_url,
                is_copilot,
                is_full_url,
            )
            .await
        })
    }

    fn apply_copilot_dynamic_base_url_for_provider<'a>(
        &'a self,
        auth_provider: &'a Provider,
        base_url: &'a mut String,
        is_copilot: bool,
        is_full_url: bool,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let Some(next_base_url) = self
                .resolve_copilot_dynamic_base_url_for_provider(
                    auth_provider,
                    base_url,
                    is_copilot,
                    is_full_url,
                )
                .await
            else {
                return;
            };

            log::debug!(
                "[Copilot] 使用动态 API endpoint: {} (原: {})",
                next_base_url,
                base_url
            );
            *base_url = next_base_url;
        })
    }

    fn resolve_copilot_live_model_for_provider<'a>(
        &'a self,
        auth_provider: &'a Provider,
        model_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        Box::pin(async move {
            let binding_context = provider_managed_account_binding_context(auth_provider);
            resolve_core_copilot_live_model_for_binding_with_runtime_source(
                self,
                binding_context.binding,
                binding_context.legacy_github_copilot_account_id,
                model_id,
            )
            .await
        })
    }

    fn apply_copilot_live_model_for_provider<'a>(
        &'a self,
        auth_provider: &'a Provider,
        body: &'a mut Value,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let Some(model_id) = body.get("model").and_then(Value::as_str) else {
                return;
            };
            let model_id = model_id.to_string();

            let resolved = match self
                .resolve_copilot_live_model_for_provider(auth_provider, &model_id)
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
            body["model"] = Value::String(resolved);
        })
    }

    fn apply_copilot_live_model_for_adapter<'a>(
        &'a self,
        auth_provider: &'a Provider,
        body: &'a mut Value,
        is_copilot: bool,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if !is_copilot {
                return;
            }

            self.apply_copilot_live_model_for_provider(auth_provider, body)
                .await;
        })
    }

    fn resolve_claude_api_format_for_provider<'a>(
        &'a self,
        auth_provider: &'a Provider,
        body: &'a Value,
        is_copilot: bool,
    ) -> BoxFuture<'a, String> {
        Box::pin(async move {
            let model = body.get("model").and_then(Value::as_str);
            let copilot_model_vendor = if let Some(model_id) = model {
                let binding_context = provider_managed_account_binding_context(auth_provider);
                resolve_core_copilot_model_vendor_for_binding_with_runtime_source(
                    self,
                    binding_context.binding,
                    binding_context.legacy_github_copilot_account_id,
                    model_id,
                    is_copilot,
                )
                .await
            } else {
                None
            };

            forwarder_claude_api_format_for_provider(
                auth_provider,
                is_copilot,
                copilot_model_vendor.as_deref(),
            )
        })
    }

    fn resolve_claude_api_format_for_adapter<'a>(
        &'a self,
        auth_provider: &'a Provider,
        body: &'a Value,
        is_copilot: bool,
        is_claude_adapter: bool,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            if !is_claude_adapter {
                return None;
            }

            Some(
                self.resolve_claude_api_format_for_provider(auth_provider, body, is_copilot)
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

async fn copilot_token_from_app_handle(
    app_handle: &tauri::AppHandle,
    account_id: Option<&str>,
) -> Result<String, CopilotAuthError> {
    let copilot_state = app_handle.state::<CopilotAuthState>();
    let copilot_auth = copilot_state.0.read().await;

    match account_id {
        Some(id) => copilot_auth.get_valid_token_for_account(id).await,
        None => copilot_auth.get_valid_token().await,
    }
}

async fn codex_oauth_token_from_app_handle(
    app_handle: &tauri::AppHandle,
    account_id: Option<&str>,
) -> Result<(String, Option<String>), CodexOAuthError> {
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
            Ok((token, resolved_account_id))
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
            match copilot_token_from_app_handle(app_handle, account_id).await {
                Ok(token) => {
                    let auth = runtime.provider_auth_info(token);
                    self.record_token_snapshot(cache_key, auth.clone(), None)
                        .await;
                    log::debug!(
                        "{}",
                        managed_account_token_success_log_message(runtime, account_id)
                    );
                    Ok(auth)
                }
                Err(error) => {
                    let failure_kind = copilot_token_failure_kind(&error);
                    let error = error.to_string();
                    if let Some(snapshot) = self
                        .token_snapshot_for_refresh_failure(
                            &cache_key,
                            account_id,
                            &error,
                            failure_kind,
                        )
                        .await
                    {
                        return Ok(snapshot.auth);
                    }

                    log::error!(
                        "{}",
                        managed_account_token_failure_log_message(runtime, account_id, &error)
                    );
                    Err(ProxyError::AuthError(
                        managed_account_token_failure_error_message(runtime, &error),
                    ))
                }
            }
        })
    }

    fn resolve_codex_oauth<'a>(
        &'a self,
        account_id: Option<String>,
        runtime: ManagedAccountAuthRuntime,
    ) -> BoxFuture<'a, Result<(ProviderAuthInfo, Option<String>), ProxyError>> {
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
            match codex_oauth_token_from_app_handle(app_handle, account_id.as_deref()).await {
                Ok((token, resolved_account_id)) => {
                    let auth = runtime.provider_auth_info(token);
                    self.record_token_snapshot(
                        cache_key,
                        auth.clone(),
                        resolved_account_id.clone(),
                    )
                    .await;
                    log::debug!(
                        "{}",
                        managed_account_token_success_log_message(
                            runtime,
                            resolved_account_id.as_deref()
                        )
                    );
                    Ok((auth, resolved_account_id))
                }
                Err(error) => {
                    let failure_kind = codex_oauth_token_failure_kind(&error);
                    let error = error.to_string();
                    if let Some(snapshot) = self
                        .token_snapshot_for_refresh_failure(
                            &cache_key,
                            account_id.as_deref(),
                            &error,
                            failure_kind,
                        )
                        .await
                    {
                        return Ok((snapshot.auth, snapshot.codex_oauth_account_id));
                    }

                    log::error!(
                        "{}",
                        managed_account_token_failure_log_message(
                            runtime,
                            account_id.as_deref(),
                            &error
                        )
                    );
                    Err(ProxyError::AuthError(
                        managed_account_token_failure_error_message(runtime, &error),
                    ))
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
    runtime_source
        .resolve_auth_for_provider(auth_provider, auth)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn token_snapshot_falls_back_for_recent_retryable_failure() {
        let source = CcSwitchManagedAccountRuntimeSource::new(None);
        let key = ManagedAccountTokenCacheKey::new(
            ManagedAccountAuthRuntime::GitHubCopilot,
            Some("acct"),
        );
        source
            .record_token_snapshot(
                key.clone(),
                ManagedAccountAuthRuntime::GitHubCopilot.provider_auth_info("cached".to_string()),
                None,
            )
            .await;

        let snapshot = source
            .token_snapshot_for_refresh_failure(
                &key,
                Some("acct"),
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .expect("recent retryable failure should use cached token");

        assert_eq!(snapshot.auth.api_key, "cached");
        assert_eq!(
            snapshot.auth.strategy,
            ManagedAccountAuthRuntime::GitHubCopilot.provider_auth_strategy()
        );
    }

    #[tokio::test]
    async fn token_snapshot_cache_is_scoped_by_runtime_and_account() {
        let source = CcSwitchManagedAccountRuntimeSource::new(None);
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
            .record_token_snapshot(
                copilot_account.clone(),
                ManagedAccountAuthRuntime::GitHubCopilot
                    .provider_auth_info("copilot-a".to_string()),
                None,
            )
            .await;

        assert!(source
            .token_snapshot_for_refresh_failure(
                &codex_same_account,
                Some("acct-a"),
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .is_none());
        assert!(source
            .token_snapshot_for_refresh_failure(
                &copilot_other_account,
                Some("acct-b"),
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .is_none());

        let snapshot = source
            .token_snapshot_for_refresh_failure(
                &copilot_account,
                Some("acct-a"),
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .expect("matching runtime/account should use cached token");
        assert_eq!(snapshot.auth.api_key, "copilot-a");
    }

    #[tokio::test]
    async fn token_snapshot_does_not_hide_non_retryable_or_stale_failures() {
        let source = CcSwitchManagedAccountRuntimeSource::new(None);
        let key =
            ManagedAccountTokenCacheKey::new(ManagedAccountAuthRuntime::CodexOAuth, Some("acct"));
        {
            let mut snapshots = source.token_snapshots.lock().await;
            snapshots.insert(
                key.clone(),
                ManagedAccountTokenSnapshot::new(
                    ManagedAccountAuthRuntime::CodexOAuth.provider_auth_info("cached".to_string()),
                    Some("acct".to_string()),
                    chrono::Utc::now().timestamp_millis() - 30_001,
                ),
            );
        }

        assert!(source
            .token_snapshot_for_refresh_failure(
                &key,
                Some("acct"),
                "network timeout",
                ManagedAccountTokenRefreshFailureKind::Retryable,
            )
            .await
            .is_none());

        source
            .record_token_snapshot(
                key.clone(),
                ManagedAccountAuthRuntime::CodexOAuth.provider_auth_info("fresh".to_string()),
                Some("acct".to_string()),
            )
            .await;

        assert!(source
            .token_snapshot_for_refresh_failure(
                &key,
                Some("acct"),
                "refresh token revoked",
                ManagedAccountTokenRefreshFailureKind::Terminal,
            )
            .await
            .is_none());
    }
}
