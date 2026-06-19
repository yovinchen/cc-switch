use http::HeaderMap;
use serde::{Deserialize, Serialize};

pub const CLAUDE_DESKTOP_MODEL_CREATED_AT: &str = "2024-01-01T00:00:00Z";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeDesktopModelRouteInput {
    pub route_id: String,
    pub supports_1m: bool,
}

impl ClaudeDesktopModelRouteInput {
    pub fn new(route_id: impl Into<String>, supports_1m: bool) -> Self {
        Self {
            route_id: route_id.into(),
            supports_1m,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeDesktopModelListItem {
    #[serde(rename = "type")]
    pub object_type: String,
    pub id: String,
    pub created_at: String,
    #[serde(
        default,
        rename = "supports1m",
        skip_serializing_if = "is_false"
    )]
    pub supports_1m: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeDesktopModelListResponse {
    pub data: Vec<ClaudeDesktopModelListItem>,
    pub has_more: bool,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
}

impl ClaudeDesktopModelListResponse {
    pub fn from_routes(routes: impl IntoIterator<Item = ClaudeDesktopModelRouteInput>) -> Self {
        let data: Vec<_> = routes
            .into_iter()
            .map(|route| ClaudeDesktopModelListItem {
                object_type: "model".to_string(),
                id: route.route_id,
                created_at: CLAUDE_DESKTOP_MODEL_CREATED_AT.to_string(),
                supports_1m: route.supports_1m,
            })
            .collect();
        let first_id = data.first().map(|item| item.id.clone());
        let last_id = data.last().map(|item| item.id.clone());

        Self {
            data,
            has_more: false,
            first_id,
            last_id,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::{
        ClaudeDesktopModelListResponse, ClaudeDesktopModelRouteInput,
        validate_claude_desktop_gateway_bearer_header,
        validate_claude_desktop_gateway_bearer_value, ClaudeDesktopGatewayAuthError,
    };
    use http::{HeaderMap, HeaderValue};
    use serde_json::json;

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

    #[test]
    fn model_list_response_serializes_claude_desktop_contract() {
        let response = ClaudeDesktopModelListResponse::from_routes([
            ClaudeDesktopModelRouteInput::new("claude-sonnet-4-6", true),
            ClaudeDesktopModelRouteInput::new("claude-haiku-4-5", false),
        ]);

        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(
            value,
            json!({
                "data": [
                    {
                        "type": "model",
                        "id": "claude-sonnet-4-6",
                        "created_at": "2024-01-01T00:00:00Z",
                        "supports1m": true
                    },
                    {
                        "type": "model",
                        "id": "claude-haiku-4-5",
                        "created_at": "2024-01-01T00:00:00Z"
                    }
                ],
                "has_more": false,
                "first_id": "claude-sonnet-4-6",
                "last_id": "claude-haiku-4-5"
            })
        );
    }
}
