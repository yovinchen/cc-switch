use http::HeaderMap;
use thiserror::Error;

use crate::provider_auth::{ProviderAuthInfo, ProviderAuthStrategy};

pub const PROXY_AUTH_PLACEHOLDER: &str = "PROXY_MANAGED";

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
        headers_contain_proxy_auth_placeholder, is_managed_account_upstream_url,
        managed_account_auth_plan, validate_managed_account_upstream_auth, ManagedAccountAuthError,
        ManagedAccountAuthPlan, ManagedAccountAuthRuntime, PROXY_AUTH_PLACEHOLDER,
    };
    use http::{HeaderMap, HeaderValue};

    use crate::provider_auth::{ProviderAuthInfo, ProviderAuthStrategy};

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
