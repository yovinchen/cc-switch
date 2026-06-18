use http::HeaderMap;
use serde_json::Value;
use sha2::{Digest, Sha256};

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

pub fn resolve_copilot_warmup_model_override<'a>(
    warmup_downgrade_enabled: bool,
    is_warmup_request: bool,
    warmup_model: &'a str,
) -> Option<&'a str> {
    (warmup_downgrade_enabled && is_warmup_request).then_some(warmup_model)
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

pub fn resolve_copilot_deterministic_request_id(body: &Value, session_id: &str) -> Option<String> {
    find_last_user_content(body).map(|content| {
        let mut hasher = Sha256::new();
        hasher.update(session_id.as_bytes());
        hasher.update(content.as_bytes());
        uuid_v4_string_from_hash(&hasher.finalize())
    })
}

pub fn resolve_copilot_deterministic_interaction_id(session_id: &str) -> Option<String> {
    if session_id.is_empty() {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(b"interaction:");
    hasher.update(session_id.as_bytes());
    Some(uuid_v4_string_from_hash(&hasher.finalize()))
}

fn find_last_user_content(body: &Value) -> Option<String> {
    let messages = body.get("messages").and_then(Value::as_array)?;

    for message in messages.iter().rev() {
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }

        let content = message.get("content")?;
        if let Some(content) = content.as_str() {
            return Some(content.to_string());
        }

        if let Some(blocks) = content.as_array() {
            let filtered = blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) != Some("tool_result"))
                .map(|block| {
                    let mut block = block.clone();
                    if let Some(object) = block.as_object_mut() {
                        object.remove("cache_control");
                    }
                    block
                })
                .collect::<Vec<_>>();

            if !filtered.is_empty() {
                return Some(serde_json::to_string(&filtered).unwrap_or_default());
            }
        }
    }

    None
}

fn uuid_v4_string_from_hash(hash: &[u8]) -> String {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u64::from_be_bytes([
            0, 0, bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ])
    )
}

#[cfg(test)]
mod tests {
    use super::{
        parse_session_from_user_id, provider_declares_bedrock,
        resolve_copilot_optimizer_session_id, should_apply_bedrock_pre_send_optimizer,
        resolve_copilot_deterministic_interaction_id, resolve_copilot_deterministic_request_id,
        resolve_copilot_warmup_model_override,
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
    fn copilot_warmup_model_override_requires_switch_and_warmup_classification() {
        assert_eq!(
            resolve_copilot_warmup_model_override(true, true, "gpt-4o-mini"),
            Some("gpt-4o-mini")
        );
        assert_eq!(
            resolve_copilot_warmup_model_override(false, true, "gpt-4o-mini"),
            None
        );
        assert_eq!(
            resolve_copilot_warmup_model_override(true, false, "gpt-4o-mini"),
            None
        );
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

    #[test]
    fn copilot_deterministic_request_id_is_stable_for_same_session_and_content() {
        let body = json!({
            "messages": [{"role": "user", "content": "Hello"}]
        });

        assert_eq!(
            resolve_copilot_deterministic_request_id(&body, "session1"),
            resolve_copilot_deterministic_request_id(&body, "session1")
        );
    }

    #[test]
    fn copilot_deterministic_request_id_varies_by_content_and_session() {
        let left = json!({
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let right = json!({
            "messages": [{"role": "user", "content": "Goodbye"}]
        });

        assert_ne!(
            resolve_copilot_deterministic_request_id(&left, "session1"),
            resolve_copilot_deterministic_request_id(&right, "session1")
        );
        assert_ne!(
            resolve_copilot_deterministic_request_id(&left, "session1"),
            resolve_copilot_deterministic_request_id(&left, "session2")
        );
    }

    #[test]
    fn copilot_deterministic_request_id_ignores_tool_result_blocks() {
        let body_one = json!({
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi"},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "version_A"}
                ]},
                {"role": "user", "content": "do something"}
            ]
        });
        let body_two = json!({
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi"},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "version_B"}
                ]},
                {"role": "user", "content": "do something"}
            ]
        });

        assert_eq!(
            resolve_copilot_deterministic_request_id(&body_one, "s"),
            resolve_copilot_deterministic_request_id(&body_two, "s")
        );
    }

    #[test]
    fn copilot_deterministic_request_id_returns_none_without_user_content() {
        let body = json!({
            "messages": [{"role": "assistant", "content": "Hi"}]
        });

        assert_eq!(resolve_copilot_deterministic_request_id(&body, "s"), None);
    }

    #[test]
    fn copilot_deterministic_request_id_is_uuid_formatted() {
        let body = json!({
            "messages": [{"role": "user", "content": "test"}]
        });
        let id = resolve_copilot_deterministic_request_id(&body, "session").unwrap();

        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[14], b'4');
        assert_eq!(id.as_bytes()[8], b'-');
        assert_eq!(id.as_bytes()[13], b'-');
        assert_eq!(id.as_bytes()[18], b'-');
        assert_eq!(id.as_bytes()[23], b'-');
    }

    #[test]
    fn copilot_deterministic_interaction_id_is_stable_and_session_scoped() {
        assert_eq!(
            resolve_copilot_deterministic_interaction_id("session_abc"),
            resolve_copilot_deterministic_interaction_id("session_abc")
        );
        assert_ne!(
            resolve_copilot_deterministic_interaction_id("session_abc"),
            resolve_copilot_deterministic_interaction_id("session_def")
        );
        assert_eq!(resolve_copilot_deterministic_interaction_id(""), None);
    }

    #[test]
    fn copilot_deterministic_interaction_id_differs_from_request_id() {
        let body = json!({
            "messages": [{"role": "user", "content": "Hello"}]
        });

        assert_ne!(
            resolve_copilot_deterministic_interaction_id("session_abc"),
            resolve_copilot_deterministic_request_id(&body, "session_abc")
        );
    }
}
