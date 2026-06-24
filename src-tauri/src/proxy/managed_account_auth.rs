use crate::commands::{CodexOAuthState, CopilotAuthState};
use crate::proxy::error::ProxyError;
use crate::proxy_core_adapter::{CopilotModel, ManagedAccountAuthRuntime, ProviderAuthInfo};
use tauri::Manager;

pub(crate) async fn resolve_copilot_api_endpoint(
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

pub(crate) async fn fetch_copilot_live_models(
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

pub(crate) async fn resolve_copilot_model_vendor(
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

pub(crate) async fn resolve_copilot_auth(
    app_handle: Option<&tauri::AppHandle>,
    account_id: Option<&str>,
    runtime: ManagedAccountAuthRuntime,
) -> Result<ProviderAuthInfo, ProxyError> {
    let Some(app_handle) = app_handle else {
        log::error!("[Copilot] AppHandle 不可用");
        return Err(ProxyError::AuthError(
            "GitHub Copilot 认证不可用（无 AppHandle）".to_string(),
        ));
    };

    let copilot_state = app_handle.state::<CopilotAuthState>();
    let copilot_auth = copilot_state.0.read().await;

    let token_result = match account_id {
        Some(id) => {
            log::debug!("[Copilot] 使用指定账号 {id} 获取 token");
            copilot_auth.get_valid_token_for_account(id).await
        }
        None => {
            log::debug!("[Copilot] 使用默认账号获取 token");
            copilot_auth.get_valid_token().await
        }
    };

    match token_result {
        Ok(token) => {
            log::debug!(
                "[Copilot] 成功获取 Copilot token (account={})",
                account_id.unwrap_or("default")
            );
            Ok(runtime.provider_auth_info(token))
        }
        Err(error) => {
            log::error!(
                "[Copilot] 获取 Copilot token 失败 (account={}): {error}",
                account_id.unwrap_or("default")
            );
            Err(ProxyError::AuthError(format!(
                "GitHub Copilot 认证失败: {error}"
            )))
        }
    }
}

pub(crate) async fn resolve_codex_oauth(
    app_handle: Option<&tauri::AppHandle>,
    account_id: Option<String>,
    runtime: ManagedAccountAuthRuntime,
) -> Result<(ProviderAuthInfo, Option<String>), ProxyError> {
    let Some(app_handle) = app_handle else {
        log::error!("[CodexOAuth] AppHandle 不可用");
        return Err(ProxyError::AuthError(
            "Codex OAuth 认证不可用（无 AppHandle）".to_string(),
        ));
    };

    let codex_state = app_handle.state::<CodexOAuthState>();
    let codex_auth = codex_state.0.read().await;

    let token_result = match &account_id {
        Some(id) => {
            log::debug!("[CodexOAuth] 使用指定账号 {id} 获取 token");
            codex_auth.get_valid_token_for_account(id).await
        }
        None => {
            log::debug!("[CodexOAuth] 使用默认账号获取 token");
            codex_auth.get_valid_token().await
        }
    };

    match token_result {
        Ok(token) => {
            let resolved_account_id = match account_id {
                Some(id) => Some(id),
                None => codex_auth.default_account_id().await,
            };
            log::debug!(
                "[CodexOAuth] 成功获取 access_token (account={})",
                resolved_account_id.as_deref().unwrap_or("default")
            );
            Ok((runtime.provider_auth_info(token), resolved_account_id))
        }
        Err(error) => {
            log::error!("[CodexOAuth] 获取 access_token 失败: {error}");
            Err(ProxyError::AuthError(format!(
                "Codex OAuth 认证失败: {error}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::proxy_core_adapter::{
        default_managed_account_runtime_source, resolve_managed_account_auth_from_runtime_source,
        ProviderAuthStrategy,
    };

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
        assert_eq!(resolve_copilot_api_endpoint(None, None).await, None);
        assert_eq!(
            fetch_copilot_live_models(None, None).await.expect("skip"),
            None
        );
        assert_eq!(
            resolve_copilot_model_vendor(None, None, "gpt-5").await,
            None
        );
    }
}
