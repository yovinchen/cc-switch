use http::HeaderMap;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const BEDROCK_OPTIMIZER_ENV_FLAG: &str = "CLAUDE_CODE_USE_BEDROCK";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotClassification {
    pub initiator: &'static str,
    pub is_warmup: bool,
    pub is_compact: bool,
    pub is_subagent: bool,
}

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

pub fn classify_copilot_request(
    body: &Value,
    has_anthropic_beta: bool,
    compact_detection: bool,
    subagent_detection: bool,
) -> CopilotClassification {
    let is_compact = compact_detection && is_copilot_compact_request(body);
    let is_subagent = subagent_detection && detect_copilot_subagent(body);

    let messages = match body.get("messages").and_then(Value::as_array) {
        Some(messages) if !messages.is_empty() => messages,
        _ => {
            return CopilotClassification {
                initiator: "user",
                is_warmup: is_copilot_warmup_request(body, has_anthropic_beta, false),
                is_compact: false,
                is_subagent,
            }
        }
    };

    let last_message = &messages[messages.len() - 1];
    let role = last_message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("");

    if role != "user" {
        return CopilotClassification {
            initiator: if is_subagent { "agent" } else { "user" },
            is_warmup: false,
            is_compact,
            is_subagent,
        };
    }

    let is_user_initiated = match last_message.get("content") {
        Some(Value::Array(blocks)) => !blocks
            .iter()
            .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_result")),
        Some(Value::String(_)) => true,
        _ => false,
    };

    let initiator = if is_subagent || !is_user_initiated || is_compact {
        "agent"
    } else {
        "user"
    };

    CopilotClassification {
        initiator,
        is_warmup: initiator == "user"
            && is_copilot_warmup_request(body, has_anthropic_beta, is_compact),
        is_compact,
        is_subagent,
    }
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

fn is_copilot_warmup_request(body: &Value, has_anthropic_beta: bool, is_compact: bool) -> bool {
    if !has_anthropic_beta || is_compact {
        return false;
    }

    body.get("tools")
        .and_then(Value::as_array)
        .is_none_or(|tools| tools.is_empty())
}

fn is_copilot_compact_request(body: &Value) -> bool {
    let system_text = extract_system_text(body);
    if system_text
        .starts_with("You are a helpful AI assistant tasked with summarizing conversations")
    {
        return true;
    }

    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return false;
    };

    let Some(last_message) = messages.last() else {
        return false;
    };
    if last_message.get("role").and_then(Value::as_str) != Some("user") {
        return false;
    }

    let text = extract_text_from_message(last_message);
    text.contains("CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.")
        || (text.contains("Pending Tasks:") && text.contains("Current Work:"))
}

fn detect_copilot_subagent(body: &Value) -> bool {
    if extract_system_text(body).contains("__SUBAGENT_MARKER__") {
        return true;
    }

    if let Some(messages) = body.get("messages").and_then(Value::as_array) {
        for message in messages {
            if message.get("role").and_then(Value::as_str) != Some("user") {
                continue;
            }
            if extract_text_from_message(message).contains("__SUBAGENT_MARKER__") {
                return true;
            }
        }
    }

    body.pointer("/metadata/user_id")
        .and_then(Value::as_str)
        .is_some_and(|user_id| user_id.contains("_agent_"))
}

fn extract_system_text(body: &Value) -> String {
    match body.get("system") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn extract_text_from_message(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| {
                (block.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| block.get("text").and_then(Value::as_str))
                    .flatten()
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
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
        classify_copilot_request, parse_session_from_user_id, provider_declares_bedrock,
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
    fn copilot_classifies_user_text_as_user_initiated() {
        let body = json!({
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let classification = classify_copilot_request(&body, false, true, false);

        assert_eq!(classification.initiator, "user");
        assert!(!classification.is_compact);
        assert!(!classification.is_warmup);
        assert!(!classification.is_subagent);
    }

    #[test]
    fn copilot_classifies_tool_result_turn_as_agent_initiated() {
        let body = json!({
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_123", "content": "ok"},
                    {"type": "text", "text": "continue"}
                ]
            }]
        });

        let classification = classify_copilot_request(&body, true, true, false);

        assert_eq!(classification.initiator, "agent");
        assert!(!classification.is_warmup);
    }

    #[test]
    fn copilot_classifies_compact_system_prompt_as_agent() {
        let body = json!({
            "system": "You are a helpful AI assistant tasked with summarizing conversations. Please create a summary.",
            "messages": [{"role": "user", "content": "Summarize"}]
        });

        let classification = classify_copilot_request(&body, false, true, false);

        assert_eq!(classification.initiator, "agent");
        assert!(classification.is_compact);
    }

    #[test]
    fn copilot_compact_detection_can_be_disabled() {
        let body = json!({
            "system": "You are a helpful AI assistant tasked with summarizing conversations.",
            "messages": [{"role": "user", "content": "Summarize"}]
        });

        let classification = classify_copilot_request(&body, false, false, false);

        assert_eq!(classification.initiator, "user");
        assert!(!classification.is_compact);
    }

    #[test]
    fn copilot_warmup_requires_anthropic_beta_no_tools_and_user_initiator() {
        let warmup = json!({
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let with_tools = json!({
            "tools": [{"name": "Read"}],
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let agent = json!({
            "messages": [{
                "role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "toolu_123", "content": "ok"}]
            }]
        });

        assert!(classify_copilot_request(&warmup, true, true, false).is_warmup);
        assert!(!classify_copilot_request(&warmup, false, true, false).is_warmup);
        assert!(!classify_copilot_request(&with_tools, true, true, false).is_warmup);
        assert!(!classify_copilot_request(&agent, true, true, false).is_warmup);
    }

    #[test]
    fn copilot_subagent_detection_forces_agent_initiator() {
        let body = json!({
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": "{\"__SUBAGENT_MARKER__\":{\"agent_id\":\"a\"}}"}]
            }]
        });

        let classification = classify_copilot_request(&body, false, true, true);

        assert_eq!(classification.initiator, "agent");
        assert!(classification.is_subagent);
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
