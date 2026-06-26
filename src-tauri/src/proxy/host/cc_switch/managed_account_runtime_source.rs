use std::sync::Arc;

use futures::future::BoxFuture;
use serde_json::Value;
use tauri::Manager;

use crate::commands::{CodexOAuthState, CopilotAuthState};
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy_core_adapter::{
    CopilotModel, CoreManagedAccountRuntimeSource, ManagedAccountAuthResolution,
    ManagedAccountAuthRuntime, ProviderAuthInfo,
    managed_account_app_handle_unavailable_error_message,
    managed_account_app_handle_unavailable_log_message,
    managed_account_token_failure_error_message, managed_account_token_failure_log_message,
    managed_account_token_request_log_message, managed_account_token_success_log_message,
    provider_managed_account_binding_input,
    resolve_core_copilot_dynamic_base_url_for_binding_with_runtime_source,
    resolve_core_copilot_live_model_for_binding_with_runtime_source,
    resolve_core_copilot_model_vendor_for_binding_with_runtime_source,
    resolve_core_managed_account_auth_for_binding_with_runtime_source,
    resolve_forwarder_claude_api_format,
};

pub(crate) type ManagedAccountRuntimeSourceRef = Arc<dyn ManagedAccountRuntimeSource + Send + Sync>;

pub(crate) struct CcSwitchManagedAccountRuntimeSource {
    app_handle: Option<tauri::AppHandle>,
}

impl CcSwitchManagedAccountRuntimeSource {
    fn new(app_handle: Option<tauri::AppHandle>) -> Self {
        Self { app_handle }
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

pub(crate) trait ManagedAccountRuntimeSource:
    CoreManagedAccountRuntimeSource<Error = ProxyError> + Send + Sync
{
    fn resolve_auth_for_provider<'a>(
        &'a self,
        auth_provider: &'a Provider,
        auth: ProviderAuthInfo,
    ) -> BoxFuture<'a, Result<ManagedAccountAuthResolution, ProxyError>> {
        Box::pin(async move {
            let meta = auth_provider.meta.as_ref();
            resolve_core_managed_account_auth_for_binding_with_runtime_source(
                self,
                auth,
                meta.and_then(provider_managed_account_binding_input),
                meta.and_then(|meta| meta.github_account_id.as_deref()),
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
            let meta = auth_provider.meta.as_ref();
            resolve_core_copilot_dynamic_base_url_for_binding_with_runtime_source(
                self,
                meta.and_then(provider_managed_account_binding_input),
                meta.and_then(|meta| meta.github_account_id.as_deref()),
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
            let meta = auth_provider.meta.as_ref();
            resolve_core_copilot_live_model_for_binding_with_runtime_source(
                self,
                meta.and_then(provider_managed_account_binding_input),
                meta.and_then(|meta| meta.github_account_id.as_deref()),
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
                let meta = auth_provider.meta.as_ref();
                resolve_core_copilot_model_vendor_for_binding_with_runtime_source(
                    self,
                    meta.and_then(provider_managed_account_binding_input),
                    meta.and_then(|meta| meta.github_account_id.as_deref()),
                    model_id,
                    is_copilot,
                )
                .await
            } else {
                None
            };

            resolve_forwarder_claude_api_format(
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
) -> Result<String, String> {
    let copilot_state = app_handle.state::<CopilotAuthState>();
    let copilot_auth = copilot_state.0.read().await;

    match account_id {
        Some(id) => copilot_auth.get_valid_token_for_account(id).await,
        None => copilot_auth.get_valid_token().await,
    }
    .map_err(|error| error.to_string())
}

async fn codex_oauth_token_from_app_handle(
    app_handle: &tauri::AppHandle,
    account_id: Option<&str>,
) -> Result<(String, Option<String>), String> {
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
        Err(error) => Err(error.to_string()),
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

            match copilot_token_from_app_handle(app_handle, account_id).await {
                Ok(token) => {
                    log::debug!(
                        "{}",
                        managed_account_token_success_log_message(runtime, account_id)
                    );
                    Ok(runtime.provider_auth_info(token))
                }
                Err(error) => {
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

            match codex_oauth_token_from_app_handle(app_handle, account_id.as_deref()).await {
                Ok((token, resolved_account_id)) => {
                    log::debug!(
                        "{}",
                        managed_account_token_success_log_message(
                            runtime,
                            resolved_account_id.as_deref()
                        )
                    );
                    Ok((runtime.provider_auth_info(token), resolved_account_id))
                }
                Err(error) => {
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
