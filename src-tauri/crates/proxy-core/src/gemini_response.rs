//! Gemini Native non-streaming response helpers.

use crate::{
    gemini_shadow::{GeminiShadowStore, GeminiToolCallMeta},
    gemini_stream::{ensure_gemini_function_call_ids, extract_gemini_function_call_meta},
    gemini_tool_args::{rectify_gemini_tool_call_parts, AnthropicToolSchemaHints},
    response_transform::map_gemini_finish_reason_to_anthropic,
    usage::build_anthropic_usage_from_gemini,
};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct GeminiResponseShadowRecord {
    pub assistant_content: Value,
    pub tool_calls: Vec<GeminiToolCallMeta>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeminiToAnthropicMessageOutput {
    pub response: Value,
    pub shadow_record: Option<GeminiResponseShadowRecord>,
    pub rectified_tool_names: Vec<String>,
}

pub fn gemini_response_to_anthropic_message<F>(
    body: &Value,
    tool_schema_hints: Option<&AnthropicToolSchemaHints>,
    mut synthesize_tool_call_id: F,
) -> Result<GeminiToAnthropicMessageOutput, String>
where
    F: FnMut() -> String,
{
    if let Some(block_reason) = body
        .get("promptFeedback")
        .and_then(|value| value.get("blockReason"))
        .and_then(|value| value.as_str())
    {
        let text = format!("Request blocked by Gemini safety filters: {block_reason}");
        return Ok(GeminiToAnthropicMessageOutput {
            response: json!({
                "id": body.get("responseId").and_then(|value| value.as_str()).unwrap_or(""),
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "text", "text": text }],
                "model": body.get("modelVersion").and_then(|value| value.as_str()).unwrap_or(""),
                "stop_reason": "refusal",
                "stop_sequence": Value::Null,
                "usage": build_anthropic_usage_from_gemini(body.get("usageMetadata"))
            }),
            shadow_record: None,
            rectified_tool_names: Vec::new(),
        });
    }

    let candidate = body
        .get("candidates")
        .and_then(|value| value.as_array())
        .and_then(|value| value.first())
        .ok_or_else(|| "No candidates in Gemini response".to_string())?;

    let parts = candidate
        .get("content")
        .and_then(|value| value.get("parts"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let mut rectified_parts = parts.clone();
    let rectified_tool_names =
        rectify_gemini_tool_call_parts(&mut rectified_parts, tool_schema_hints);

    // Keep the Anthropic-visible response, shadow content, and shadow metadata
    // on one synthesized-id source for Gemini responses that omit function ids.
    ensure_gemini_function_call_ids(&mut rectified_parts, &mut synthesize_tool_call_id);

    let mut content = Vec::new();
    let mut has_tool_use = false;

    for part in &rectified_parts {
        if part.get("thought").and_then(|value| value.as_bool()) == Some(true) {
            continue;
        }

        if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
            if !text.is_empty() {
                content.push(json!({
                    "type": "text",
                    "text": text
                }));
            }
            continue;
        }

        if let Some(function_call) = part.get("functionCall") {
            has_tool_use = true;
            let id = function_call
                .get("id")
                .and_then(|value| value.as_str())
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(&mut synthesize_tool_call_id);
            content.push(json!({
                "type": "tool_use",
                "id": id,
                "name": function_call.get("name").and_then(|value| value.as_str()).unwrap_or(""),
                "input": function_call.get("args").cloned().unwrap_or_else(|| json!({}))
            }));
        }
    }

    let stop_reason = json!(map_gemini_finish_reason_to_anthropic(
        candidate
            .get("finishReason")
            .and_then(|value| value.as_str()),
        has_tool_use,
        false,
    ));

    let response = json!({
        "id": body.get("responseId").and_then(|value| value.as_str()).unwrap_or(""),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": body.get("modelVersion").and_then(|value| value.as_str()).unwrap_or(""),
        "stop_reason": stop_reason,
        "stop_sequence": Value::Null,
        "usage": build_anthropic_usage_from_gemini(body.get("usageMetadata"))
    });

    let shadow_record = candidate.get("content").map(|content| {
        let mut assistant_content = content.clone();
        if let Some(parts_value) = assistant_content.get_mut("parts") {
            *parts_value = json!(rectified_parts.clone());
        }
        GeminiResponseShadowRecord {
            assistant_content,
            tool_calls: extract_gemini_function_call_meta(
                &rectified_parts,
                Some(&mut synthesize_tool_call_id),
            ),
        }
    });

    Ok(GeminiToAnthropicMessageOutput {
        response,
        shadow_record,
        rectified_tool_names,
    })
}

pub fn gemini_response_to_anthropic_message_with_shadow<F>(
    body: &Value,
    shadow_store: Option<&GeminiShadowStore>,
    provider_id: Option<&str>,
    session_id: Option<&str>,
    tool_schema_hints: Option<&AnthropicToolSchemaHints>,
    synthesize_tool_call_id: F,
) -> Result<GeminiToAnthropicMessageOutput, String>
where
    F: FnMut() -> String,
{
    let output =
        gemini_response_to_anthropic_message(body, tool_schema_hints, synthesize_tool_call_id)?;

    if let (Some(store), Some(provider_id), Some(session_id), Some(shadow_record)) = (
        shadow_store,
        provider_id,
        session_id,
        output.shadow_record.as_ref(),
    ) {
        store.record_assistant_turn(
            provider_id,
            session_id,
            shadow_record.assistant_content.clone(),
            shadow_record.tool_calls.clone(),
        );
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemini_stream::is_synthesized_gemini_tool_call_id;

    fn next_synth(counter: &mut usize) -> String {
        *counter += 1;
        crate::gemini_stream::synthesize_gemini_tool_call_id(counter.to_string())
    }

    #[test]
    fn maps_blocked_prompt_to_anthropic_refusal() {
        let input = json!({
            "responseId": "resp_blocked",
            "modelVersion": "gemini-2.5-flash",
            "promptFeedback": { "blockReason": "SAFETY" },
            "usageMetadata": { "promptTokenCount": 4, "totalTokenCount": 4 }
        });

        let output =
            gemini_response_to_anthropic_message(&input, None, || "unused".to_string()).unwrap();

        assert_eq!(output.response["stop_reason"], "refusal");
        assert!(
            output.response["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("SAFETY")
        );
        assert!(output.shadow_record.is_none());
    }

    #[test]
    fn maps_text_response_and_usage() {
        let input = json!({
            "responseId": "resp_1",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": { "parts": [{ "text": "Hello" }] }
            }],
            "usageMetadata": {
                "promptTokenCount": 12,
                "totalTokenCount": 20,
                "cachedContentTokenCount": 3
            }
        });

        let output =
            gemini_response_to_anthropic_message(&input, None, || "unused".to_string()).unwrap();

        assert_eq!(output.response["id"], "resp_1");
        assert_eq!(output.response["content"][0]["text"], "Hello");
        assert_eq!(output.response["stop_reason"], "end_turn");
        assert_eq!(output.response["usage"]["input_tokens"], 9);
        assert_eq!(output.response["usage"]["output_tokens"], 8);
        assert!(output.shadow_record.is_some());
    }

    #[test]
    fn synthesizes_one_id_source_for_missing_function_call_ids() {
        let input = json!({
            "responseId": "resp_tool",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{
                        "functionCall": { "name": "lookup", "args": { "q": "rust" } }
                    }]
                }
            }]
        });
        let mut counter = 0usize;

        let output =
            gemini_response_to_anthropic_message(&input, None, || next_synth(&mut counter))
                .unwrap();
        let client_id = output.response["content"][0]["id"].as_str().unwrap();
        let shadow_record = output.shadow_record.expect("shadow record");

        assert!(is_synthesized_gemini_tool_call_id(client_id));
        assert_eq!(output.response["stop_reason"], "tool_use");
        assert_eq!(
            shadow_record.assistant_content["parts"][0]["functionCall"]["id"],
            client_id
        );
        assert_eq!(shadow_record.tool_calls[0].id.as_deref(), Some(client_id));
    }

    #[test]
    fn response_with_shadow_records_assistant_turn() {
        let store = GeminiShadowStore::with_limits(8, 4);
        let input = json!({
            "responseId": "resp_shadow",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{
                        "functionCall": {
                            "id": "call_1",
                            "name": "lookup",
                            "args": { "q": "rust" }
                        }
                    }]
                }
            }]
        });

        let output = gemini_response_to_anthropic_message_with_shadow(
            &input,
            Some(&store),
            Some("provider-a"),
            Some("session-1"),
            None,
            || "unused".to_string(),
        )
        .unwrap();

        assert_eq!(output.response["content"][0]["id"], "call_1");
        let snapshot = store
            .get_session("provider-a", "session-1")
            .expect("shadow recorded");
        assert_eq!(
            snapshot.turns[0].tool_calls[0].id.as_deref(),
            Some("call_1")
        );
        assert_eq!(
            snapshot.turns[0].assistant_content["parts"][0]["functionCall"]["name"],
            "lookup"
        );
    }
}
