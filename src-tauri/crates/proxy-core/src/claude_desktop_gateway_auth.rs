use http::HeaderMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeDesktopGatewayAuthError {
    MissingAuthorizationHeader,
    InvalidAuthorizationHeader,
    InvalidToken,
}

impl ClaudeDesktopGatewayAuthError {
    pub fn message(self) -> &'static str {
        match self {
            Self::MissingAuthorizationHeader => {
                "Claude Desktop gateway 缺少 Authorization 头"
            }
            Self::InvalidAuthorizationHeader => "Authorization 头格式无效",
            Self::InvalidToken => "Claude Desktop gateway token 无效",
        }
    }
}

pub fn validate_claude_desktop_gateway_bearer_value(
    authorization: Option<&str>,
    expected_token: &str,
) -> Result<(), ClaudeDesktopGatewayAuthError> {
    let value = authorization.ok_or(ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader)?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .unwrap_or("")
        .trim();

    if token != expected_token {
        return Err(ClaudeDesktopGatewayAuthError::InvalidToken);
    }

    Ok(())
}

pub fn validate_claude_desktop_gateway_bearer_header(
    headers: &HeaderMap,
    expected_token: &str,
) -> Result<(), ClaudeDesktopGatewayAuthError> {
    let value = headers
        .get(http::header::AUTHORIZATION)
        .map(|value| {
            value
                .to_str()
                .map_err(|_| ClaudeDesktopGatewayAuthError::InvalidAuthorizationHeader)
        })
        .transpose()?;

    validate_claude_desktop_gateway_bearer_value(value, expected_token)
}

#[cfg(test)]
mod tests {
    use super::{
        validate_claude_desktop_gateway_bearer_header,
        validate_claude_desktop_gateway_bearer_value, ClaudeDesktopGatewayAuthError,
    };
    use http::{HeaderMap, HeaderValue};

    #[test]
    fn gateway_bearer_value_accepts_existing_scheme_variants() {
        validate_claude_desktop_gateway_bearer_value(
            Some("Bearer gateway-token"),
            "gateway-token",
        )
        .expect("valid bearer");
        validate_claude_desktop_gateway_bearer_value(
            Some("bearer gateway-token "),
            "gateway-token",
        )
        .expect("valid bearer");
    }

    #[test]
    fn gateway_bearer_value_preserves_existing_error_messages() {
        assert_eq!(
            validate_claude_desktop_gateway_bearer_value(None, "gateway-token").unwrap_err(),
            ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader
        );
        assert_eq!(
            validate_claude_desktop_gateway_bearer_value(Some("Basic gateway-token"), "gateway-token")
                .unwrap_err(),
            ClaudeDesktopGatewayAuthError::InvalidToken
        );
        assert_eq!(
            ClaudeDesktopGatewayAuthError::InvalidToken.message(),
            "Claude Desktop gateway token 无效"
        );
    }

    #[test]
    fn gateway_bearer_header_reads_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer gateway-token"),
        );

        validate_claude_desktop_gateway_bearer_header(&headers, "gateway-token")
            .expect("valid bearer");

        assert_eq!(
            validate_claude_desktop_gateway_bearer_header(&HeaderMap::new(), "gateway-token")
                .unwrap_err(),
            ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader
        );
    }
}
