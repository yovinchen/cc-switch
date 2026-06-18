use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagementAuthDecision {
    AllowWithoutToken,
    RequireToken(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagementAuthError {
    RequiredTokenMissing,
    MissingBearerToken,
    InvalidAuthorizationHeader,
    InvalidBearerToken,
}

impl ManagementAuthError {
    pub fn message(self) -> &'static str {
        match self {
            Self::RequiredTokenMissing => {
                "Proxy management token is required for non-loopback listeners"
            }
            Self::MissingBearerToken => "Missing management bearer token",
            Self::InvalidAuthorizationHeader => "Invalid Authorization header",
            Self::InvalidBearerToken => "Invalid management bearer token",
        }
    }
}

pub fn resolve_management_auth_decision(
    listen_address: &str,
    configured_token: Option<&str>,
    fallback_token: Option<&str>,
) -> Result<ManagementAuthDecision, ManagementAuthError> {
    let configured_token = normalized_management_token(configured_token);
    let external_listener = !is_loopback_listen_address(listen_address);

    if configured_token.is_none() && !external_listener {
        return Ok(ManagementAuthDecision::AllowWithoutToken);
    }

    configured_token
        .or_else(|| {
            if external_listener {
                normalized_management_token(fallback_token)
            } else {
                None
            }
        })
        .map(ManagementAuthDecision::RequireToken)
        .ok_or(ManagementAuthError::RequiredTokenMissing)
}

pub fn normalized_management_token(token: Option<&str>) -> Option<String> {
    token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
}

pub fn is_loopback_listen_address(listen_address: &str) -> bool {
    let listen_address = listen_address.trim();
    listen_address.eq_ignore_ascii_case("localhost")
        || listen_address
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

pub fn validate_management_bearer_value(
    authorization: Option<&str>,
    expected_token: &str,
) -> Result<(), ManagementAuthError> {
    let value = authorization.ok_or(ManagementAuthError::MissingBearerToken)?;
    let (scheme, token) = value
        .split_once(' ')
        .ok_or(ManagementAuthError::InvalidAuthorizationHeader)?;

    if !scheme.eq_ignore_ascii_case("bearer") || token.trim() != expected_token {
        return Err(ManagementAuthError::InvalidBearerToken);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        is_loopback_listen_address, resolve_management_auth_decision,
        validate_management_bearer_value, ManagementAuthDecision, ManagementAuthError,
    };

    #[test]
    fn loopback_without_token_allows_management_access() {
        let decision = resolve_management_auth_decision("127.0.0.1", None, Some("env-token"))
            .expect("resolve auth");

        assert_eq!(decision, ManagementAuthDecision::AllowWithoutToken);
    }

    #[test]
    fn loopback_with_configured_token_requires_that_token() {
        let decision =
            resolve_management_auth_decision("localhost", Some(" config-token "), Some("env-token"))
                .expect("resolve auth");

        assert_eq!(
            decision,
            ManagementAuthDecision::RequireToken("config-token".to_string())
        );
    }

    #[test]
    fn external_listener_uses_configured_token_before_fallback() {
        let decision = resolve_management_auth_decision(
            "0.0.0.0",
            Some("config-token"),
            Some("env-token"),
        )
        .expect("resolve auth");

        assert_eq!(
            decision,
            ManagementAuthDecision::RequireToken("config-token".to_string())
        );
    }

    #[test]
    fn external_listener_uses_fallback_token_when_config_is_empty() {
        let decision =
            resolve_management_auth_decision("0.0.0.0", Some(" "), Some(" env-token "))
                .expect("resolve auth");

        assert_eq!(
            decision,
            ManagementAuthDecision::RequireToken("env-token".to_string())
        );
    }

    #[test]
    fn external_listener_requires_a_token() {
        let error =
            resolve_management_auth_decision("192.168.1.10", None, Some(" ")).unwrap_err();

        assert_eq!(error, ManagementAuthError::RequiredTokenMissing);
        assert_eq!(
            error.message(),
            "Proxy management token is required for non-loopback listeners"
        );
    }

    #[test]
    fn loopback_detection_matches_existing_management_policy() {
        assert!(is_loopback_listen_address("localhost"));
        assert!(is_loopback_listen_address(" LOCALHOST "));
        assert!(is_loopback_listen_address("127.0.0.1"));
        assert!(is_loopback_listen_address("::1"));
        assert!(!is_loopback_listen_address("0.0.0.0"));
        assert!(!is_loopback_listen_address("192.168.1.10"));
    }

    #[test]
    fn bearer_validation_accepts_case_insensitive_scheme_and_trimmed_token() {
        validate_management_bearer_value(Some("bearer secret-token "), "secret-token")
            .expect("valid bearer");
        validate_management_bearer_value(Some("Bearer secret-token"), "secret-token")
            .expect("valid bearer");
    }

    #[test]
    fn bearer_validation_rejects_missing_malformed_and_wrong_tokens() {
        assert_eq!(
            validate_management_bearer_value(None, "secret-token").unwrap_err(),
            ManagementAuthError::MissingBearerToken
        );
        assert_eq!(
            validate_management_bearer_value(Some("Bearer"), "secret-token").unwrap_err(),
            ManagementAuthError::InvalidAuthorizationHeader
        );
        assert_eq!(
            validate_management_bearer_value(Some("Basic secret-token"), "secret-token")
                .unwrap_err(),
            ManagementAuthError::InvalidBearerToken
        );
        assert_eq!(
            validate_management_bearer_value(Some("Bearer other-token"), "secret-token")
                .unwrap_err(),
            ManagementAuthError::InvalidBearerToken
        );
    }
}
