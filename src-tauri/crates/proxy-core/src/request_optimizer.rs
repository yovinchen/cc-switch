use http::HeaderMap;
use serde_json::Value;

pub const BEDROCK_OPTIMIZER_ENV_FLAG: &str = "CLAUDE_CODE_USE_BEDROCK";

pub fn provider_declares_bedrock(use_bedrock_env: Option<&str>) -> bool {
    matches!(use_bedrock_env, Some("1"))
}

pub fn should_apply_bedrock_pre_send_optimizer(
    optimizer_enabled: bool,
    use_bedrock_env: Option<&str>,
) -> bool {
    optimizer_enabled && provider_declares_bedrock(use_bedrock_env)
}

pub fn parse_session_from_user_id(user_id: &str) -> Option<String> {
    user_id.find("_session_").and_then(|position| {
        let session_id = &user_id[position + "_session_".len()..];
        (!session_id.is_empty()).then(|| session_id.to_string())
    })
}

pub fn resolve_copilot_optimizer_session_id(body: &Value, headers: &HeaderMap) -> String {
    let metadata = body.get("metadata");

    metadata
        .and_then(|metadata| metadata.get("user_id"))
        .and_then(Value::as_str)
        .and_then(parse_session_from_user_id)
        .or_else(|| {
            metadata
                .and_then(|metadata| metadata.get("session_id"))
                .and_then(Value::as_str)
                .filter(|session_id| !session_id.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            metadata
                .and_then(|metadata| metadata.get("user_id"))
                .and_then(Value::as_str)
                .filter(|user_id| !user_id.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            headers
                .get("x-session-id")
                .and_then(|value| value.to_str().ok())
                .filter(|session_id| !session_id.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_session_from_user_id, provider_declares_bedrock,
        resolve_copilot_optimizer_session_id, should_apply_bedrock_pre_send_optimizer,
    };
    use http::{HeaderMap, HeaderValue};
    use serde_json::json;

    #[test]
    fn bedrock_provider_detection_matches_existing_env_flag_contract() {
        assert!(provider_declares_bedrock(Some("1")));
        assert!(!provider_declares_bedrock(Some("0")));
        assert!(!provider_declares_bedrock(Some("true")));
        assert!(!provider_declares_bedrock(Some("")));
        assert!(!provider_declares_bedrock(None));
    }

    #[test]
    fn bedrock_pre_send_optimizer_requires_feature_and_provider_flags() {
        assert!(should_apply_bedrock_pre_send_optimizer(true, Some("1")));
        assert!(!should_apply_bedrock_pre_send_optimizer(false, Some("1")));
        assert!(!should_apply_bedrock_pre_send_optimizer(true, Some("0")));
        assert!(!should_apply_bedrock_pre_send_optimizer(true, None));
    }

    #[test]
    fn session_suffix_parser_matches_existing_user_id_contract() {
        assert_eq!(
            parse_session_from_user_id("user_john_session_abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            parse_session_from_user_id("my_app_session_xyz789"),
            Some("xyz789".to_string())
        );
        assert_eq!(
            parse_session_from_user_id("no_session_marker"),
            Some("marker".to_string())
        );
        assert_eq!(parse_session_from_user_id("user_john_abc123"), None);
        assert_eq!(parse_session_from_user_id("_session_"), None);
    }

    #[test]
    fn copilot_optimizer_session_prefers_user_id_suffix() {
        let headers = HeaderMap::new();
        let body = json!({
            "metadata": {
                "user_id": "user_john_session_abc123",
                "session_id": "metadata-session"
            }
        });

        assert_eq!(
            resolve_copilot_optimizer_session_id(&body, &headers),
            "abc123"
        );
    }

    #[test]
    fn copilot_optimizer_session_falls_back_to_metadata_session_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "metadata": {
                "session_id": "metadata-session"
            }
        });

        assert_eq!(
            resolve_copilot_optimizer_session_id(&body, &headers),
            "metadata-session"
        );
    }

    #[test]
    fn copilot_optimizer_session_falls_back_to_raw_user_id_before_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-session-id", HeaderValue::from_static("header-session"));
        let body = json!({
            "metadata": {
                "user_id": "raw-user"
            }
        });

        assert_eq!(
            resolve_copilot_optimizer_session_id(&body, &headers),
            "raw-user"
        );
    }

    #[test]
    fn copilot_optimizer_session_uses_x_session_id_header_last() {
        let mut headers = HeaderMap::new();
        headers.insert("x-session-id", HeaderValue::from_static("header-session"));
        let body = json!({});

        assert_eq!(
            resolve_copilot_optimizer_session_id(&body, &headers),
            "header-session"
        );
    }

    #[test]
    fn copilot_optimizer_session_defaults_to_empty_string() {
        let headers = HeaderMap::new();
        let body = json!({});

        assert_eq!(resolve_copilot_optimizer_session_id(&body, &headers), "");
    }
}
