use crate::commands::{CodexOAuthState, CopilotAuthState};
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy_core_adapter::{ProviderAuthInfo, ProviderAuthStrategy};
use tauri::Manager;

#[derive(Debug)]
pub(crate) struct ManagedAccountAuthResolution {
    pub(crate) auth: ProviderAuthInfo,
    pub(crate) codex_oauth_account_id: Option<String>,
    pub(crate) should_send_codex_oauth_session_headers: bool,
}

pub(crate) async fn resolve_managed_account_auth(
    app_handle: Option<&tauri::AppHandle>,
    auth_provider: &Provider,
    auth: ProviderAuthInfo,
) -> Result<ManagedAccountAuthResolution, ProxyError> {
    match auth.strategy {
        ProviderAuthStrategy::GitHubCopilot => {
            let auth = resolve_copilot_auth(app_handle, auth_provider).await?;
            Ok(ManagedAccountAuthResolution {
                auth,
                codex_oauth_account_id: None,
                should_send_codex_oauth_session_headers: false,
            })
        }
        ProviderAuthStrategy::CodexOAuth => {
            let (auth, codex_oauth_account_id) =
                resolve_codex_oauth(app_handle, auth_provider).await?;
            Ok(ManagedAccountAuthResolution {
                auth,
                codex_oauth_account_id,
                should_send_codex_oauth_session_headers: true,
            })
        }
        _ => Ok(ManagedAccountAuthResolution {
            auth,
            codex_oauth_account_id: None,
            should_send_codex_oauth_session_headers: false,
        }),
    }
}

async fn resolve_copilot_auth(
    app_handle: Option<&tauri::AppHandle>,
    auth_provider: &Provider,
) -> Result<ProviderAuthInfo, ProxyError> {
    let Some(app_handle) = app_handle else {
        log::error!("[Copilot] AppHandle 不可用");
        return Err(ProxyError::AuthError(
            "GitHub Copilot 认证不可用（无 AppHandle）".to_string(),
        ));
    };

    let copilot_state = app_handle.state::<CopilotAuthState>();
    let copilot_auth = copilot_state.0.read().await;
    let account_id = auth_provider
        .meta
        .as_ref()
        .and_then(|m| m.managed_account_id_for("github_copilot"));

    let token_result = match &account_id {
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
                account_id.as_deref().unwrap_or("default")
            );
            Ok(ProviderAuthInfo::new(
                token,
                ProviderAuthStrategy::GitHubCopilot,
            ))
        }
        Err(error) => {
            log::error!(
                "[Copilot] 获取 Copilot token 失败 (account={}): {error}",
                account_id.as_deref().unwrap_or("default")
            );
            Err(ProxyError::AuthError(format!(
                "GitHub Copilot 认证失败: {error}"
            )))
        }
    }
}

async fn resolve_codex_oauth(
    app_handle: Option<&tauri::AppHandle>,
    auth_provider: &Provider,
) -> Result<(ProviderAuthInfo, Option<String>), ProxyError> {
    let Some(app_handle) = app_handle else {
        log::error!("[CodexOAuth] AppHandle 不可用");
        return Err(ProxyError::AuthError(
            "Codex OAuth 认证不可用（无 AppHandle）".to_string(),
        ));
    };

    let codex_state = app_handle.state::<CodexOAuthState>();
    let codex_auth = codex_state.0.read().await;
    let account_id = auth_provider
        .meta
        .as_ref()
        .and_then(|m| m.managed_account_id_for("codex_oauth"));

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
            Ok((
                ProviderAuthInfo::new(token, ProviderAuthStrategy::CodexOAuth),
                resolved_account_id,
            ))
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

    #[tokio::test]
    async fn non_managed_auth_passes_through_without_app_handle() {
        let auth = ProviderAuthInfo::new("sk-test".to_string(), ProviderAuthStrategy::Bearer);
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            serde_json::json!({}),
            None,
        );

        let resolved = resolve_managed_account_auth(None, &provider, auth.clone())
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

        let copilot = resolve_managed_account_auth(
            None,
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

        let codex = resolve_managed_account_auth(
            None,
            &provider,
            ProviderAuthInfo::new("PROXY_MANAGED".to_string(), ProviderAuthStrategy::CodexOAuth),
        )
        .await
        .expect_err("codex app handle error");
        assert!(matches!(
            codex,
            ProxyError::AuthError(message)
                if message == "Codex OAuth 认证不可用（无 AppHandle）"
        ));
    }
}
