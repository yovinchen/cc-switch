use http::HeaderMap;
use thiserror::Error;

pub const PROXY_AUTH_PLACEHOLDER: &str = "PROXY_MANAGED";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManagedAccountAuthError {
    #[error(
        "Managed account proxy auth was not resolved; PROXY_MANAGED must not be sent upstream"
    )]
    PlaceholderForwarded,
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
        validate_managed_account_upstream_auth, ManagedAccountAuthError,
    };
    use http::{HeaderMap, HeaderValue};

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
