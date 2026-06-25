use tauri::State;

use crate::commands::codex_oauth::CodexOAuthState;
use crate::commands::copilot::CopilotAuthState;
use crate::proxy::codex_oauth_auth::CodexOAuthError;
use crate::proxy::copilot_auth::{CopilotAuthError, GitHubAccount, GitHubDeviceCodeResponse};
use crate::proxy_core_adapter::{
    ensure_managed_auth_provider, managed_auth_account_from_parts,
    managed_auth_device_code_response_from_parts, managed_auth_status_from_parts,
    ManagedAuthAccount, ManagedAuthDeviceCodeResponse, ManagedAuthStatus,
    CODEX_OAUTH_AUTH_PROVIDER, GITHUB_COPILOT_AUTH_PROVIDER,
};

fn map_account(
    provider: &str,
    account: GitHubAccount,
    default_account_id: Option<&str>,
) -> ManagedAuthAccount {
    managed_auth_account_from_parts(
        provider,
        account.id,
        account.login,
        account.avatar_url,
        account.authenticated_at,
        account.github_domain,
        default_account_id,
    )
}

fn map_device_code_response(
    provider: &str,
    response: GitHubDeviceCodeResponse,
) -> ManagedAuthDeviceCodeResponse {
    managed_auth_device_code_response_from_parts(
        provider,
        response.device_code,
        response.user_code,
        response.verification_uri,
        response.expires_in,
        response.interval,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub async fn auth_start_login(
    auth_provider: String,
    github_domain: Option<String>,
    copilot_state: State<'_, CopilotAuthState>,
    codex_state: State<'_, CodexOAuthState>,
) -> Result<ManagedAuthDeviceCodeResponse, String> {
    let auth_provider = ensure_managed_auth_provider(&auth_provider)?;
    match auth_provider {
        GITHUB_COPILOT_AUTH_PROVIDER => {
            let auth_manager = copilot_state.0.read().await;
            let response = auth_manager
                .start_device_flow(github_domain.as_deref())
                .await
                .map_err(|e| e.to_string())?;
            Ok(map_device_code_response(auth_provider, response))
        }
        CODEX_OAUTH_AUTH_PROVIDER => {
            let auth_manager = codex_state.0.read().await;
            let response = auth_manager
                .start_device_flow()
                .await
                .map_err(|e| e.to_string())?;
            Ok(map_device_code_response(auth_provider, response))
        }
        _ => unreachable!(),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn auth_poll_for_account(
    auth_provider: String,
    device_code: String,
    github_domain: Option<String>,
    copilot_state: State<'_, CopilotAuthState>,
    codex_state: State<'_, CodexOAuthState>,
) -> Result<Option<ManagedAuthAccount>, String> {
    let auth_provider = ensure_managed_auth_provider(&auth_provider)?;
    match auth_provider {
        GITHUB_COPILOT_AUTH_PROVIDER => {
            let auth_manager = copilot_state.0.write().await;
            match auth_manager
                .poll_for_token(&device_code, github_domain.as_deref())
                .await
            {
                Ok(account) => {
                    let default_account_id = auth_manager.get_status().await.default_account_id;
                    Ok(account.map(|account| {
                        map_account(auth_provider, account, default_account_id.as_deref())
                    }))
                }
                Err(CopilotAuthError::AuthorizationPending) => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        }
        CODEX_OAUTH_AUTH_PROVIDER => {
            let auth_manager = codex_state.0.write().await;
            match auth_manager.poll_for_token(&device_code).await {
                Ok(account) => {
                    let default_account_id = auth_manager.get_status().await.default_account_id;
                    Ok(account.map(|account| {
                        map_account(auth_provider, account, default_account_id.as_deref())
                    }))
                }
                Err(CodexOAuthError::AuthorizationPending) => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        }
        _ => unreachable!(),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn auth_list_accounts(
    auth_provider: String,
    copilot_state: State<'_, CopilotAuthState>,
    codex_state: State<'_, CodexOAuthState>,
) -> Result<Vec<ManagedAuthAccount>, String> {
    let auth_provider = ensure_managed_auth_provider(&auth_provider)?;
    match auth_provider {
        GITHUB_COPILOT_AUTH_PROVIDER => {
            let auth_manager = copilot_state.0.read().await;
            let status = auth_manager.get_status().await;
            let default_account_id = status.default_account_id.clone();
            Ok(status
                .accounts
                .into_iter()
                .map(|account| map_account(auth_provider, account, default_account_id.as_deref()))
                .collect())
        }
        CODEX_OAUTH_AUTH_PROVIDER => {
            let auth_manager = codex_state.0.read().await;
            let status = auth_manager.get_status().await;
            let default_account_id = status.default_account_id.clone();
            Ok(status
                .accounts
                .into_iter()
                .map(|account| map_account(auth_provider, account, default_account_id.as_deref()))
                .collect())
        }
        _ => unreachable!(),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn auth_get_status(
    auth_provider: String,
    copilot_state: State<'_, CopilotAuthState>,
    codex_state: State<'_, CodexOAuthState>,
) -> Result<ManagedAuthStatus, String> {
    let auth_provider = ensure_managed_auth_provider(&auth_provider)?;
    match auth_provider {
        GITHUB_COPILOT_AUTH_PROVIDER => {
            let auth_manager = copilot_state.0.read().await;
            let status = auth_manager.get_status().await;
            let default_account_id = status.default_account_id.clone();
            Ok(managed_auth_status_from_parts(
                auth_provider,
                status.authenticated,
                default_account_id.clone(),
                status.migration_error,
                status
                    .accounts
                    .into_iter()
                    .map(|account| {
                        map_account(auth_provider, account, default_account_id.as_deref())
                    })
                    .collect(),
            ))
        }
        CODEX_OAUTH_AUTH_PROVIDER => {
            let auth_manager = codex_state.0.read().await;
            let status = auth_manager.get_status().await;
            let default_account_id = status.default_account_id.clone();
            Ok(managed_auth_status_from_parts(
                auth_provider,
                status.authenticated,
                default_account_id.clone(),
                None,
                status
                    .accounts
                    .into_iter()
                    .map(|account| {
                        map_account(auth_provider, account, default_account_id.as_deref())
                    })
                    .collect(),
            ))
        }
        _ => unreachable!(),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn auth_remove_account(
    auth_provider: String,
    account_id: String,
    copilot_state: State<'_, CopilotAuthState>,
    codex_state: State<'_, CodexOAuthState>,
) -> Result<(), String> {
    let auth_provider = ensure_managed_auth_provider(&auth_provider)?;
    match auth_provider {
        GITHUB_COPILOT_AUTH_PROVIDER => {
            let auth_manager = copilot_state.0.write().await;
            auth_manager
                .remove_account(&account_id)
                .await
                .map_err(|e| e.to_string())
        }
        CODEX_OAUTH_AUTH_PROVIDER => {
            let auth_manager = codex_state.0.write().await;
            auth_manager
                .remove_account(&account_id)
                .await
                .map_err(|e| e.to_string())
        }
        _ => unreachable!(),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn auth_set_default_account(
    auth_provider: String,
    account_id: String,
    copilot_state: State<'_, CopilotAuthState>,
    codex_state: State<'_, CodexOAuthState>,
) -> Result<(), String> {
    let auth_provider = ensure_managed_auth_provider(&auth_provider)?;
    match auth_provider {
        GITHUB_COPILOT_AUTH_PROVIDER => {
            let auth_manager = copilot_state.0.write().await;
            auth_manager
                .set_default_account(&account_id)
                .await
                .map_err(|e| e.to_string())
        }
        CODEX_OAUTH_AUTH_PROVIDER => {
            let auth_manager = codex_state.0.write().await;
            auth_manager
                .set_default_account(&account_id)
                .await
                .map_err(|e| e.to_string())
        }
        _ => unreachable!(),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn auth_logout(
    auth_provider: String,
    copilot_state: State<'_, CopilotAuthState>,
    codex_state: State<'_, CodexOAuthState>,
) -> Result<(), String> {
    let auth_provider = ensure_managed_auth_provider(&auth_provider)?;
    match auth_provider {
        GITHUB_COPILOT_AUTH_PROVIDER => {
            let auth_manager = copilot_state.0.write().await;
            auth_manager.clear_auth().await.map_err(|e| e.to_string())
        }
        CODEX_OAUTH_AUTH_PROVIDER => {
            let auth_manager = codex_state.0.write().await;
            auth_manager.clear_auth().await.map_err(|e| e.to_string())
        }
        _ => unreachable!(),
    }
}
