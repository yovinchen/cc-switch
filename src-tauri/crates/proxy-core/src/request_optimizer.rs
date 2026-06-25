use std::collections::HashSet;

use crate::cache_injector::{CacheInjectionReport, inject_cache_control};
use crate::ports::{CopilotOptimizerConfig, OptimizerConfig};
use crate::request_headers::{
    build_codex_oauth_session_headers_for_forwarder,
    build_copilot_auth_header_overrides_for_forwarder, build_upstream_auth_headers,
    should_log_copilot_subagent_auth_override, CopilotAuthHeaderOverrideFacts,
    UpstreamAuthHeadersInput,
};
use crate::thinking_optimizer::{ThinkingOptimizationReport, optimize_thinking};
use http::{HeaderMap, HeaderName, HeaderValue};
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

pub struct CopilotAuthOptimizationPreparationInput<'a> {
    pub classification: CopilotClassification,
    pub request_classification_enabled: bool,
    pub deterministic_request_id_enabled: bool,
    pub session_source_body: &'a Value,
    pub request_body: &'a Value,
    pub headers: &'a HeaderMap,
}

pub struct OptionalCopilotAuthOptimizationPreparationInput<'a> {
    pub classification: Option<CopilotClassification>,
    pub config: &'a CopilotOptimizerConfig,
    pub session_source_body: &'a Value,
    pub request_body: &'a Value,
    pub headers: &'a HeaderMap,
}

pub struct ForwarderAuthHeaderFinalizationInput<'a> {
    pub base_auth_headers: &'a [(HeaderName, HeaderValue)],
    pub should_send_codex_oauth_session_headers: bool,
    pub session_client_provided: bool,
    pub session_id: &'a str,
    pub codex_oauth_account_id: Option<&'a str>,
    pub copilot_optimization: Option<&'a PreparedCopilotAuthOptimization>,
}

pub struct ForwarderAuthHeaders {
    pub auth_headers: Vec<(HeaderName, HeaderValue)>,
    pub codex_oauth_session_headers: Vec<(HeaderName, HeaderValue)>,
    pub should_log_copilot_subagent_auth_override: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCopilotAuthOptimization {
    pub request_classification_enabled: bool,
    pub initiator: &'static str,
    pub is_subagent: bool,
    pub deterministic_request_id: Option<String>,
    pub interaction_id: Option<String>,
}

impl PreparedCopilotAuthOptimization {
    pub fn as_header_override_facts(&self) -> CopilotAuthHeaderOverrideFacts<'_> {
        CopilotAuthHeaderOverrideFacts {
            request_classification_enabled: self.request_classification_enabled,
            initiator: self.initiator,
            is_subagent: self.is_subagent,
            deterministic_request_id: self.deterministic_request_id.as_deref(),
            interaction_id: self.interaction_id.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CopilotWarmupModelOverrideResult {
    pub body: Value,
    pub applied_model: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BedrockPreSendOptimizationReport {
    pub thinking: Option<ThinkingOptimizationReport>,
    pub cache: Option<CacheInjectionReport>,
}

pub fn provider_declares_bedrock(use_bedrock_env: Option<&str>) -> bool {
    matches!(use_bedrock_env, Some("1"))
}

pub fn bedrock_env_flag_from_provider_settings(settings: &Value) -> Option<&str> {
    settings
        .get("env")
        .and_then(|env| env.get(BEDROCK_OPTIMIZER_ENV_FLAG))
        .and_then(Value::as_str)
}

pub fn should_apply_bedrock_pre_send_optimizer(
    optimizer_enabled: bool,
    use_bedrock_env: Option<&str>,
) -> bool {
    optimizer_enabled && provider_declares_bedrock(use_bedrock_env)
}

pub fn apply_bedrock_pre_send_optimizers(
    body: &mut Value,
    config: &OptimizerConfig,
) -> BedrockPreSendOptimizationReport {
    if !config.enabled {
        return BedrockPreSendOptimizationReport::default();
    }

    let thinking = config
        .thinking_optimizer
        .then(|| optimize_thinking(body, &config.thinking_optimizer_core_config()));
    let cache = config
        .cache_injection
        .then(|| inject_cache_control(body, &config.cache_injection_core_config()));

    BedrockPreSendOptimizationReport { thinking, cache }
}

pub fn resolve_copilot_warmup_model_override(
    warmup_downgrade_enabled: bool,
    is_warmup_request: bool,
    warmup_model: &str,
) -> Option<&str> {
    (warmup_downgrade_enabled && is_warmup_request).then_some(warmup_model)
}

pub fn apply_copilot_warmup_model_override(
    mut body: Value,
    warmup_downgrade_enabled: bool,
    is_warmup_request: bool,
    warmup_model: &str,
) -> CopilotWarmupModelOverrideResult {
    let applied_model =
        resolve_copilot_warmup_model_override(warmup_downgrade_enabled, is_warmup_request, warmup_model)
            .map(str::to_string);

    if let Some(model) = &applied_model {
        body["model"] = Value::String(model.clone());
    }

    CopilotWarmupModelOverrideResult {
        body,
        applied_model,
    }
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

pub fn resolve_copilot_request_id_with_fallback(
    body: &Value,
    session_id: &str,
    fallback: impl FnOnce() -> String,
) -> String {
    resolve_copilot_deterministic_request_id(body, session_id).unwrap_or_else(fallback)
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

pub fn prepare_copilot_auth_optimization_for_forwarder(
    input: CopilotAuthOptimizationPreparationInput<'_>,
    fallback_request_id: impl FnOnce() -> String,
) -> PreparedCopilotAuthOptimization {
    let session_id = resolve_copilot_optimizer_session_id(input.session_source_body, input.headers);
    let deterministic_request_id = input.deterministic_request_id_enabled.then(|| {
        resolve_copilot_request_id_with_fallback(input.request_body, &session_id, fallback_request_id)
    });
    let interaction_id = resolve_copilot_deterministic_interaction_id(&session_id);

    PreparedCopilotAuthOptimization {
        request_classification_enabled: input.request_classification_enabled,
        initiator: input.classification.initiator,
        is_subagent: input.classification.is_subagent,
        deterministic_request_id,
        interaction_id,
    }
}

pub fn prepare_optional_copilot_auth_optimization_for_forwarder(
    input: OptionalCopilotAuthOptimizationPreparationInput<'_>,
    fallback_request_id: impl FnOnce() -> String,
) -> Option<PreparedCopilotAuthOptimization> {
    input
        .classification
        .map(|classification| {
            prepare_copilot_auth_optimization_for_forwarder(
                CopilotAuthOptimizationPreparationInput {
                    classification,
                    request_classification_enabled: input.config.request_classification,
                    deterministic_request_id_enabled: input.config.deterministic_request_id,
                    session_source_body: input.session_source_body,
                    request_body: input.request_body,
                    headers: input.headers,
                },
                fallback_request_id,
            )
        })
}

pub fn finalize_forwarder_auth_headers(
    input: ForwarderAuthHeaderFinalizationInput<'_>,
) -> ForwarderAuthHeaders {
    let codex_oauth_session_headers = build_codex_oauth_session_headers_for_forwarder(
        input.should_send_codex_oauth_session_headers,
        input.session_client_provided,
        input.session_id,
    );
    let copilot_overrides = input
        .copilot_optimization
        .map(|optimization| {
            build_copilot_auth_header_overrides_for_forwarder(
                optimization.as_header_override_facts(),
            )
        });
    let auth_headers = build_upstream_auth_headers(UpstreamAuthHeadersInput {
        base_auth_headers: input.base_auth_headers,
        codex_oauth_account_id: input.codex_oauth_account_id,
        copilot_overrides,
    });
    let should_log_copilot_subagent_auth_override =
        should_log_copilot_subagent_auth_override(copilot_overrides);

    ForwarderAuthHeaders {
        auth_headers,
        codex_oauth_session_headers,
        should_log_copilot_subagent_auth_override,
    }
}

/// Merge user tool_result and text blocks so Copilot treats tool continuations as agent turns.
pub fn merge_copilot_tool_results(mut body: Value) -> Value {
    let messages = match body.get_mut("messages").and_then(Value::as_array_mut) {
        Some(messages) if !messages.is_empty() => messages,
        _ => return body,
    };

    for message in messages.iter_mut() {
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }

        let content = match message.get("content").and_then(Value::as_array) {
            Some(blocks) => blocks,
            None => continue,
        };

        let mut tool_results: Vec<Value> = Vec::new();
        let mut text_blocks: Vec<Value> = Vec::new();
        let mut valid = true;

        for block in content {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_result") => tool_results.push(block.clone()),
                Some("text") => text_blocks.push(block.clone()),
                _ => {
                    valid = false;
                    break;
                }
            }
        }

        if !valid || tool_results.is_empty() || text_blocks.is_empty() {
            continue;
        }

        message["content"] = Value::Array(merge_blocks_into_tool_results(
            tool_results,
            text_blocks,
        ));
    }

    let messages = match body.get("messages").and_then(Value::as_array) {
        Some(messages) => messages.clone(),
        None => return body,
    };
    if messages.len() <= 1 {
        return body;
    }

    let mut merged_messages: Vec<Value> = Vec::with_capacity(messages.len());
    let mut index = 0;

    while index < messages.len() {
        if is_tool_result_only_message(&messages[index]) {
            let mut combined_content: Vec<Value> = Vec::new();
            while index < messages.len() && is_tool_result_only_message(&messages[index]) {
                if let Some(content) = messages[index].get("content").and_then(Value::as_array) {
                    combined_content.extend(content.iter().cloned());
                }
                index += 1;
            }

            if !combined_content.is_empty() {
                merged_messages.push(serde_json::json!({
                    "role": "user",
                    "content": combined_content
                }));
            }
        } else {
            merged_messages.push(messages[index].clone());
            index += 1;
        }
    }

    body["messages"] = Value::Array(merged_messages);
    body
}

/// Convert tool_result blocks without a matching adjacent assistant tool_use into text blocks.
pub fn sanitize_copilot_orphan_tool_results(mut body: Value) -> Value {
    let messages = match body.get_mut("messages").and_then(Value::as_array_mut) {
        Some(messages) if messages.len() >= 2 => messages,
        _ => return body,
    };

    for index in 1..messages.len() {
        if messages[index].get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }

        let previous_tool_use_ids: HashSet<String> =
            if messages[index - 1].get("role").and_then(Value::as_str) == Some("assistant") {
                messages[index - 1]
                    .get("content")
                    .and_then(Value::as_array)
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|block| {
                                block.get("type").and_then(Value::as_str) == Some("tool_use")
                            })
                            .filter_map(|block| {
                                block.get("id").and_then(Value::as_str).map(str::to_string)
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                HashSet::new()
            };

        let Some(content) = messages[index]
            .get_mut("content")
            .and_then(Value::as_array_mut)
        else {
            continue;
        };

        for block in content.iter_mut() {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }

            let tool_use_id = block
                .get("tool_use_id")
                .and_then(Value::as_str)
                .unwrap_or("");

            if tool_use_id.is_empty() || !previous_tool_use_ids.contains(tool_use_id) {
                let content_text = match block.get("content") {
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Array(blocks)) => blocks
                        .iter()
                        .filter_map(|block| block.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };

                *block = serde_json::json!({
                    "type": "text",
                    "text": format!("[Tool result for {}]: {}", tool_use_id, content_text)
                });
            }
        }
    }

    body
}

/// Strip Anthropic thinking blocks from assistant messages before forwarding to Copilot.
pub fn strip_copilot_thinking_blocks(mut body: Value) -> Value {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return body;
    };

    for message in messages.iter_mut() {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }

        let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };

        content.retain(|block| {
            !matches!(
                block.get("type").and_then(Value::as_str),
                Some("thinking") | Some("redacted_thinking")
            )
        });
    }

    body
}

fn merge_blocks_into_tool_results(
    mut tool_results: Vec<Value>,
    text_blocks: Vec<Value>,
) -> Vec<Value> {
    if tool_results.len() == text_blocks.len() {
        for (tool_result, text_block) in tool_results.iter_mut().zip(text_blocks.iter()) {
            append_text_to_tool_result(tool_result, text_block);
        }
    } else if let Some(last_tool_result) = tool_results.last_mut() {
        for text_block in &text_blocks {
            append_text_to_tool_result(last_tool_result, text_block);
        }
    }

    tool_results
}

fn append_text_to_tool_result(tool_result: &mut Value, text_block: &Value) {
    let text = text_block
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("");
    if text.trim().is_empty() {
        return;
    }

    match tool_result.get_mut("content") {
        Some(Value::String(existing)) => {
            existing.push('\n');
            existing.push_str(text);
        }
        Some(Value::Array(blocks)) => {
            blocks.push(serde_json::json!({"type": "text", "text": text}));
        }
        _ => {
            tool_result["content"] = Value::String(text.to_string());
        }
    }
}

fn is_tool_result_only_message(message: &Value) -> bool {
    if message.get("role").and_then(Value::as_str) != Some("user") {
        return false;
    }

    match message.get("content").and_then(Value::as_array) {
        Some(blocks) if !blocks.is_empty() => blocks
            .iter()
            .all(|block| block.get("type").and_then(Value::as_str) == Some("tool_result")),
        _ => false,
    }
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
    use crate::ports::{CopilotOptimizerConfig, OptimizerConfig};

    use super::{
        apply_bedrock_pre_send_optimizers, apply_copilot_warmup_model_override,
        classify_copilot_request, finalize_forwarder_auth_headers, merge_copilot_tool_results,
        bedrock_env_flag_from_provider_settings, parse_session_from_user_id,
        prepare_copilot_auth_optimization_for_forwarder,
        prepare_optional_copilot_auth_optimization_for_forwarder,
        provider_declares_bedrock, resolve_copilot_optimizer_session_id,
        sanitize_copilot_orphan_tool_results, should_apply_bedrock_pre_send_optimizer,
        CopilotAuthOptimizationPreparationInput, CopilotClassification,
        ForwarderAuthHeaderFinalizationInput, OptionalCopilotAuthOptimizationPreparationInput,
        PreparedCopilotAuthOptimization,
        resolve_copilot_deterministic_interaction_id, resolve_copilot_deterministic_request_id,
        resolve_copilot_request_id_with_fallback, resolve_copilot_warmup_model_override,
        strip_copilot_thinking_blocks,
    };
    use http::{HeaderMap, HeaderName, HeaderValue};
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
    fn bedrock_env_flag_is_projected_from_provider_settings() {
        let settings = json!({
            "env": {
                "CLAUDE_CODE_USE_BEDROCK": "1"
            }
        });

        assert_eq!(
            bedrock_env_flag_from_provider_settings(&settings),
            Some("1")
        );
        assert_eq!(
            bedrock_env_flag_from_provider_settings(&json!({ "env": {} })),
            None
        );
        assert_eq!(
            bedrock_env_flag_from_provider_settings(&json!({ "CLAUDE_CODE_USE_BEDROCK": "1" })),
            None
        );
    }

    #[test]
    fn bedrock_pre_send_optimizer_requires_feature_and_provider_flags() {
        assert!(should_apply_bedrock_pre_send_optimizer(true, Some("1")));
        assert!(!should_apply_bedrock_pre_send_optimizer(false, Some("1")));
        assert!(!should_apply_bedrock_pre_send_optimizer(true, Some("0")));
        assert!(!should_apply_bedrock_pre_send_optimizer(true, None));
    }

    #[test]
    fn bedrock_pre_send_optimizer_applies_enabled_mutations_and_reports() {
        let mut body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "tools": [{"name": "tool1"}],
            "system": [{"type": "text", "text": "sys prompt"}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "hello"}
                ]}
            ]
        });
        let config = OptimizerConfig {
            enabled: true,
            thinking_optimizer: true,
            cache_injection: true,
            cache_ttl: "1h".to_string(),
        };

        let report = apply_bedrock_pre_send_optimizers(&mut body, &config);

        assert!(report.thinking.is_some());
        let cache_report = report.cache.as_ref().expect("cache should run");
        assert_eq!(
            cache_report.injected,
            vec!["tools".to_string(), "system".to_string(), "msgs".to_string()]
        );
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], "max");
        assert!(body["tools"][0].get("cache_control").is_some());
        assert!(body["system"][0].get("cache_control").is_some());
        assert!(
            body["messages"][1]["content"][0]
                .get("cache_control")
                .is_some()
        );
    }

    #[test]
    fn bedrock_pre_send_optimizer_respects_global_switch() {
        let original = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "messages": [{"role": "user", "content": "hello"}]
        });
        let mut body = original.clone();
        let config = OptimizerConfig {
            enabled: false,
            thinking_optimizer: true,
            cache_injection: true,
            cache_ttl: "1h".to_string(),
        };

        let report = apply_bedrock_pre_send_optimizers(&mut body, &config);

        assert_eq!(report, Default::default());
        assert_eq!(body, original);
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
    fn copilot_warmup_model_override_applies_model_body_mutation() {
        let body = json!({
            "model": "claude-sonnet-4",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = apply_copilot_warmup_model_override(body, true, true, "gpt-4o-mini");

        assert_eq!(result.body["model"], "gpt-4o-mini");
        assert_eq!(result.applied_model, Some("gpt-4o-mini".to_string()));
    }

    #[test]
    fn copilot_warmup_model_override_leaves_body_when_gate_is_closed() {
        let body = json!({
            "model": "claude-sonnet-4",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = apply_copilot_warmup_model_override(body.clone(), false, true, "gpt-4o-mini");

        assert_eq!(result.body, body);
        assert_eq!(result.applied_model, None);
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
    fn copilot_request_id_uses_fallback_without_user_content() {
        let body = json!({
            "messages": [{"role": "assistant", "content": "Hi"}]
        });

        assert_eq!(
            resolve_copilot_request_id_with_fallback(&body, "s", || "fallback-id".to_string()),
            "fallback-id"
        );
    }

    #[test]
    fn copilot_request_id_prefers_deterministic_id_over_fallback() {
        let body = json!({
            "messages": [{"role": "user", "content": "test"}]
        });

        let request_id =
            resolve_copilot_request_id_with_fallback(&body, "session", || "fallback-id".to_string());

        assert_ne!(request_id, "fallback-id");
        assert_eq!(request_id.len(), 36);
        assert_eq!(request_id.as_bytes()[14], b'4');
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

    #[test]
    fn prepares_copilot_auth_optimization_for_forwarder() {
        let mut headers = HeaderMap::new();
        headers.insert("x-session-id", HeaderValue::from_static("header-session"));
        let session_source_body = json!({
            "metadata": { "session_id": "body-session" }
        });
        let request_body = json!({
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let prepared = prepare_copilot_auth_optimization_for_forwarder(
            CopilotAuthOptimizationPreparationInput {
                classification: CopilotClassification {
                    initiator: "agent",
                    is_warmup: false,
                    is_compact: false,
                    is_subagent: true,
                },
                request_classification_enabled: true,
                deterministic_request_id_enabled: true,
                session_source_body: &session_source_body,
                request_body: &request_body,
                headers: &headers,
            },
            || "fallback-id".to_string(),
        );

        assert!(prepared.request_classification_enabled);
        assert_eq!(prepared.initiator, "agent");
        assert!(prepared.is_subagent);
        assert_ne!(
            prepared.deterministic_request_id.as_deref(),
            Some("fallback-id")
        );
        assert!(prepared.interaction_id.is_some());

        let facts = prepared.as_header_override_facts();
        assert!(facts.request_classification_enabled);
        assert_eq!(facts.initiator, "agent");
        assert!(facts.is_subagent);
        assert_eq!(
            facts.deterministic_request_id,
            prepared.deterministic_request_id.as_deref()
        );
        assert_eq!(facts.interaction_id, prepared.interaction_id.as_deref());
    }

    #[test]
    fn prepares_optional_copilot_auth_optimization_for_forwarder() {
        let headers = HeaderMap::new();
        let config = CopilotOptimizerConfig {
            request_classification: false,
            deterministic_request_id: false,
            ..CopilotOptimizerConfig::default()
        };

        let skipped = prepare_optional_copilot_auth_optimization_for_forwarder(
            OptionalCopilotAuthOptimizationPreparationInput {
                classification: None,
                config: &config,
                session_source_body: &json!({}),
                request_body: &json!({}),
                headers: &headers,
            },
            || panic!("fallback request id should not be generated when classification is absent"),
        );
        assert_eq!(skipped, None);

        let prepared = prepare_optional_copilot_auth_optimization_for_forwarder(
            OptionalCopilotAuthOptimizationPreparationInput {
                classification: Some(CopilotClassification {
                    initiator: "user",
                    is_warmup: false,
                    is_compact: false,
                    is_subagent: false,
                }),
                config: &config,
                session_source_body: &json!({
                    "metadata": { "session_id": "session-a" }
                }),
                request_body: &json!({
                    "messages": [{"role": "user", "content": "Hello"}]
                }),
                headers: &headers,
            },
            || "fallback-id".to_string(),
        )
        .expect("prepared copilot auth optimization");

        assert!(!prepared.request_classification_enabled);
        assert_eq!(prepared.initiator, "user");
        assert!(!prepared.is_subagent);
        assert_eq!(prepared.deterministic_request_id, None);
        assert!(prepared.interaction_id.is_some());
    }

    #[test]
    fn finalizes_forwarder_auth_headers_with_session_account_and_copilot_overrides() {
        let base_auth_headers = vec![
            (
                HeaderName::from_static("authorization"),
                HeaderValue::from_static("Bearer token"),
            ),
            (
                HeaderName::from_static("x-request-id"),
                HeaderValue::from_static("old-request"),
            ),
            (
                HeaderName::from_static("x-agent-task-id"),
                HeaderValue::from_static("old-task"),
            ),
            (
                HeaderName::from_static("x-initiator"),
                HeaderValue::from_static("user"),
            ),
            (
                HeaderName::from_static("x-interaction-type"),
                HeaderValue::from_static("conversation-agent"),
            ),
        ];
        let copilot_optimization = PreparedCopilotAuthOptimization {
            request_classification_enabled: true,
            initiator: "agent",
            is_subagent: true,
            deterministic_request_id: Some("request-id".to_string()),
            interaction_id: Some("interaction-id".to_string()),
        };

        let finalized = finalize_forwarder_auth_headers(ForwarderAuthHeaderFinalizationInput {
            base_auth_headers: &base_auth_headers,
            should_send_codex_oauth_session_headers: true,
            session_client_provided: true,
            session_id: "session-123",
            codex_oauth_account_id: Some("account-1"),
            copilot_optimization: Some(&copilot_optimization),
        });

        let mut auth_headers = HeaderMap::new();
        for (name, value) in finalized.auth_headers {
            auth_headers.insert(name, value);
        }
        assert_eq!(
            auth_headers.get("authorization"),
            Some(&HeaderValue::from_static("Bearer token"))
        );
        assert_eq!(
            auth_headers.get("x-request-id"),
            Some(&HeaderValue::from_static("request-id"))
        );
        assert_eq!(
            auth_headers.get("x-agent-task-id"),
            Some(&HeaderValue::from_static("request-id"))
        );
        assert_eq!(
            auth_headers.get("x-initiator"),
            Some(&HeaderValue::from_static("agent"))
        );
        assert_eq!(
            auth_headers.get("x-interaction-type"),
            Some(&HeaderValue::from_static("conversation-subagent"))
        );
        assert_eq!(
            auth_headers.get("x-interaction-id"),
            Some(&HeaderValue::from_static("interaction-id"))
        );
        assert_eq!(
            auth_headers.get("chatgpt-account-id"),
            Some(&HeaderValue::from_static("account-1"))
        );

        let mut session_headers = HeaderMap::new();
        for (name, value) in finalized.codex_oauth_session_headers {
            session_headers.insert(name, value);
        }
        assert_eq!(
            session_headers.get("session_id"),
            Some(&HeaderValue::from_static("session-123"))
        );
        assert!(finalized.should_log_copilot_subagent_auth_override);
    }

    #[test]
    fn copilot_tool_result_merge_absorbs_text_blocks_inside_user_message() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "file contents"},
                    {"type": "text", "text": "skill output"}
                ]}
            ]
        });

        let merged = merge_copilot_tool_results(body);
        let content = merged["messages"][0]["content"].as_array().unwrap();

        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "tool_result");
        assert!(content[0]["content"]
            .as_str()
            .unwrap()
            .contains("file contents"));
        assert!(content[0]["content"].as_str().unwrap().contains("skill output"));
    }

    #[test]
    fn copilot_tool_result_merge_maps_equal_counts_positionally() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "result1"},
                    {"type": "text", "text": "text1"},
                    {"type": "tool_result", "tool_use_id": "t2", "content": "result2"},
                    {"type": "text", "text": "text2"}
                ]}
            ]
        });

        let merged = merge_copilot_tool_results(body);
        let content = merged["messages"][0]["content"].as_array().unwrap();

        assert_eq!(content.len(), 2);
        assert!(content[0]["content"].as_str().unwrap().contains("text1"));
        assert!(content[1]["content"].as_str().unwrap().contains("text2"));
    }

    #[test]
    fn copilot_tool_result_merge_combines_consecutive_tool_result_only_messages() {
        let body = json!({
            "messages": [
                {"role": "user", "content": "Read files"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {}},
                    {"type": "tool_use", "id": "t2", "name": "Read", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "file1"}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t2", "content": "file2"}
                ]}
            ]
        });

        let merged = merge_copilot_tool_results(body);
        let messages = merged["messages"].as_array().unwrap();

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn copilot_tool_result_merge_skips_messages_with_other_block_types() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "result"},
                    {"type": "image", "source": {"data": "..."}},
                    {"type": "text", "text": "caption"}
                ]}
            ]
        });

        let merged = merge_copilot_tool_results(body);
        let content = merged["messages"][0]["content"].as_array().unwrap();

        assert_eq!(content.len(), 3);
        assert_eq!(content[1]["type"], "image");
    }

    #[test]
    fn copilot_orphan_tool_result_sanitize_keeps_adjacent_matches_and_converts_orphans() {
        let body = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "tool_1", "name": "read", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tool_1", "content": "file contents"},
                    {"type": "tool_result", "tool_use_id": "tool_orphan", "content": "orphan data"}
                ]}
            ]
        });

        let sanitized = sanitize_copilot_orphan_tool_results(body);
        let content = sanitized["messages"][1]["content"].as_array().unwrap();

        assert_eq!(content[0]["type"], "tool_result");
        assert_eq!(content[1]["type"], "text");
        assert!(content[1]["text"].as_str().unwrap().contains("tool_orphan"));
        assert!(content[1]["text"].as_str().unwrap().contains("orphan data"));
    }

    #[test]
    fn copilot_orphan_tool_result_sanitize_requires_adjacent_assistant_tool_use() {
        let body = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "old_tool", "name": "search", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "old_tool", "content": "found"}
                ]},
                {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "old_tool", "content": "stale"}
                ]}
            ]
        });

        let sanitized = sanitize_copilot_orphan_tool_results(body);

        assert_eq!(sanitized["messages"][1]["content"][0]["type"], "tool_result");
        assert_eq!(sanitized["messages"][3]["content"][0]["type"], "text");
    }

    #[test]
    fn copilot_orphan_tool_result_sanitize_handles_empty_id_and_array_content() {
        let body = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "tool_1", "name": "read", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "", "content": [
                        {"type": "text", "text": "first"},
                        {"type": "text", "text": "second"}
                    ]},
                    {"type": "tool_result", "content": "missing id"}
                ]}
            ]
        });

        let sanitized = sanitize_copilot_orphan_tool_results(body);
        let content = sanitized["messages"][1]["content"].as_array().unwrap();

        assert_eq!(content[0]["type"], "text");
        assert!(content[0]["text"].as_str().unwrap().contains("first\nsecond"));
        assert_eq!(content[1]["type"], "text");
    }

    #[test]
    fn copilot_thinking_strip_removes_only_assistant_thinking_blocks() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "thinking", "thinking": "leave user content alone"},
                    {"type": "text", "text": "hi"}
                ]},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "internal", "signature": "sig"},
                    {"type": "redacted_thinking", "data": "opaque"},
                    {"type": "text", "text": "hello"},
                    {"type": "tool_use", "id": "t1", "name": "read", "input": {}, "signature": "keep"}
                ]},
                {"role": "assistant", "content": "plain response"}
            ]
        });

        let stripped = strip_copilot_thinking_blocks(body);

        let user_content = stripped["messages"][0]["content"].as_array().unwrap();
        assert_eq!(user_content.len(), 2);

        let assistant_content = stripped["messages"][1]["content"].as_array().unwrap();
        assert_eq!(assistant_content.len(), 2);
        assert_eq!(assistant_content[0]["type"], "text");
        assert_eq!(assistant_content[1]["type"], "tool_use");
        assert_eq!(assistant_content[1]["signature"], "keep");
        assert_eq!(stripped["messages"][2]["content"], "plain response");
    }

    #[test]
    fn copilot_thinking_strip_preserves_body_without_messages() {
        let body = json!({"model": "claude-sonnet-4"});

        assert_eq!(strip_copilot_thinking_blocks(body.clone()), body);
    }
}
