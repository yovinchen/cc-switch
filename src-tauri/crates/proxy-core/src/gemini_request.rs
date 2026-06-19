//! Gemini Native request helpers.

use crate::{is_synthesized_gemini_tool_call_id, normalize_gemini_tool_result_response};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub const GEMINI_SYSTEM_INSTRUCTION_TYPE_ERROR: &str =
    "Anthropic system must be a string or an array";

pub fn build_gemini_system_instruction(
    system: Option<&Value>,
    messages: Option<&[Value]>,
) -> Result<Option<Value>, &'static str> {
    let mut texts = Vec::new();

    if let Some(system) = system {
        collect_gemini_system_texts(system, &mut texts)?;
    }

    if let Some(messages) = messages {
        for message in messages {
            if message.get("role").and_then(|value| value.as_str()) != Some("system") {
                continue;
            }
            if let Some(content) = message.get("content") {
                collect_gemini_system_texts(content, &mut texts)?;
            }
        }
    }

    if texts.is_empty() {
        return Ok(None);
    }

    Ok(Some(json!({
        "parts": [{ "text": texts.join("\n\n") }]
    })))
}

fn collect_gemini_system_texts(value: &Value, texts: &mut Vec<String>) -> Result<(), &'static str> {
    if let Some(text) = value.as_str() {
        if !text.is_empty() {
            texts.push(text.to_string());
        }
        return Ok(());
    }

    let Some(blocks) = value.as_array() else {
        return Err(GEMINI_SYSTEM_INSTRUCTION_TYPE_ERROR);
    };

    texts.extend(
        blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(|value| value.as_str()))
            .filter(|text| !text.is_empty())
            .map(ToString::to_string),
    );

    Ok(())
}

pub fn build_gemini_generation_config(body: &Value) -> Option<Value> {
    let mut config = Map::new();

    if let Some(value) = body.get("max_tokens") {
        config.insert("maxOutputTokens".to_string(), value.clone());
    }
    if let Some(value) = body.get("temperature") {
        config.insert("temperature".to_string(), value.clone());
    }
    if let Some(value) = body.get("top_p") {
        config.insert("topP".to_string(), value.clone());
    }
    if let Some(value) = body.get("stop_sequences") {
        config.insert("stopSequences".to_string(), value.clone());
    }

    if config.is_empty() {
        None
    } else {
        Some(Value::Object(config))
    }
}

pub fn map_gemini_tool_choice_to_config(
    tool_choice: Option<&Value>,
) -> Result<Option<Value>, String> {
    let Some(tool_choice) = tool_choice else {
        return Ok(None);
    };

    match tool_choice {
        Value::String(choice) => Ok(match choice.as_str() {
            "auto" => Some(json!({
                "functionCallingConfig": { "mode": "AUTO" }
            })),
            "none" => Some(json!({
                "functionCallingConfig": { "mode": "NONE" }
            })),
            other => {
                return Err(format!("Unsupported Gemini tool_choice string: {other}"));
            }
        }),
        Value::Object(object) => {
            let Some(choice_type) = object.get("type").and_then(|value| value.as_str()) else {
                return Ok(None);
            };

            let config = match choice_type {
                "auto" => json!({ "mode": "AUTO" }),
                "none" => json!({ "mode": "NONE" }),
                "any" => json!({ "mode": "ANY" }),
                "tool" => {
                    let name = object
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    json!({
                        "mode": "ANY",
                        "allowedFunctionNames": [name]
                    })
                }
                other => {
                    return Err(format!("Unsupported Gemini tool_choice type: {other}"));
                }
            };

            Ok(Some(json!({ "functionCallingConfig": config })))
        }
        _ => Ok(None),
    }
}

pub fn anthropic_message_content_to_gemini_parts(
    content: Option<&Value>,
    role: &str,
    tool_name_by_id: &mut HashMap<String, String>,
    thought_signature_by_id: &HashMap<String, String>,
) -> Result<Vec<Value>, String> {
    let Some(content) = content else {
        return Ok(Vec::new());
    };

    if let Some(text) = content.as_str() {
        return Ok(vec![json!({ "text": text })]);
    }

    let Some(blocks) = content.as_array() else {
        return Err("Anthropic message content must be a string or array".to_string());
    };

    let mut parts = Vec::new();

    for block in blocks {
        let block_type = block
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        match block_type {
            "text" => {
                if let Some(text) = block.get("text").and_then(|value| value.as_str()) {
                    parts.push(json!({ "text": text }));
                }
            }
            "image" => {
                let source = block
                    .get("source")
                    .ok_or_else(|| "Gemini image block missing source".to_string())?;

                let source_type = source
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");

                if source_type != "base64" {
                    return Err(format!(
                        "Gemini Native only supports base64 image sources, got `{source_type}`"
                    ));
                }

                parts.push(json!({
                    "inlineData": {
                        "mimeType": source.get("media_type").and_then(|value| value.as_str()).unwrap_or("image/png"),
                        "data": source.get("data").and_then(|value| value.as_str()).unwrap_or("")
                    }
                }));
            }
            "document" => {
                let source = block
                    .get("source")
                    .ok_or_else(|| "Gemini document block missing source".to_string())?;

                let source_type = source
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");

                if source_type != "base64" {
                    return Err(format!(
                        "Gemini Native only supports base64 document sources, got `{source_type}`"
                    ));
                }

                parts.push(json!({
                    "inlineData": {
                        "mimeType": source.get("media_type").and_then(|value| value.as_str()).unwrap_or("application/pdf"),
                        "data": source.get("data").and_then(|value| value.as_str()).unwrap_or("")
                    }
                }));
            }
            "tool_use" => {
                if role != "assistant" {
                    return Err("tool_use blocks are only valid in assistant messages".to_string());
                }

                let id = block
                    .get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let name = block
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if !id.is_empty() && !name.is_empty() {
                    tool_name_by_id.insert(id.to_string(), name.to_string());
                }

                // A synthesized id is an internal proxy identifier; Gemini
                // disambiguates those calls by order, matching its prior
                // no-id response shape.
                let mut function_call = json!({
                    "name": name,
                    "args": block.get("input").cloned().unwrap_or_else(|| json!({}))
                });
                if !id.is_empty() && !is_synthesized_gemini_tool_call_id(id) {
                    function_call["id"] = json!(id);
                }

                if let Some(sig) = thought_signature_by_id.get(id) {
                    function_call["thoughtSignature"] = json!(sig);
                }

                parts.push(json!({ "functionCall": function_call }));
            }
            "tool_result" => {
                let tool_use_id = block
                    .get("tool_use_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let name = tool_name_by_id
                    .get(tool_use_id)
                    .cloned()
                    .or_else(|| {
                        blocks.iter().find_map(|block| {
                            let block_type = block.get("type").and_then(|value| value.as_str())?;
                            if block_type != "tool_use" {
                                return None;
                            }
                            let id = block.get("id").and_then(|value| value.as_str())?;
                            if id != tool_use_id {
                                return None;
                            }
                            block
                                .get("name")
                                .and_then(|value| value.as_str())
                                .map(ToString::to_string)
                        })
                    })
                    .ok_or_else(|| {
                        format!(
                            "Unable to resolve Gemini functionResponse.name for tool_use_id `{tool_use_id}`"
                        )
                    })?;

                let mut function_response = json!({
                    "name": name,
                    "response": normalize_gemini_tool_result_response(block.get("content"))
                });
                if !tool_use_id.is_empty() && !is_synthesized_gemini_tool_call_id(tool_use_id) {
                    function_response["id"] = json!(tool_use_id);
                }

                parts.push(json!({ "functionResponse": function_response }));
            }
            "thinking" | "redacted_thinking" => {}
            _ => {}
        }
    }

    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn builds_gemini_system_instruction_from_top_level_and_message_systems() {
        let messages = vec![
            json!({ "role": "system", "content": "Message system." }),
            json!({
                "role": "system",
                "content": [{ "type": "text", "text": "Block system." }]
            }),
            json!({ "role": "user", "content": "Hello" }),
        ];

        assert_eq!(
            build_gemini_system_instruction(
                Some(&json!([{ "type": "text", "text": "Top level system." }])),
                Some(&messages)
            )
            .unwrap(),
            Some(json!({
                "parts": [{ "text": "Top level system.\n\nMessage system.\n\nBlock system." }]
            }))
        );
    }

    #[test]
    fn omits_empty_gemini_system_instruction() {
        assert_eq!(build_gemini_system_instruction(None, None).unwrap(), None);
        assert_eq!(
            build_gemini_system_instruction(Some(&json!("")), Some(&[])).unwrap(),
            None
        );
    }

    #[test]
    fn rejects_non_string_or_array_gemini_system_instruction() {
        assert_eq!(
            build_gemini_system_instruction(Some(&json!({ "text": "bad" })), None).unwrap_err(),
            GEMINI_SYSTEM_INSTRUCTION_TYPE_ERROR
        );
    }

    #[test]
    fn builds_gemini_generation_config_from_anthropic_fields() {
        assert_eq!(
            build_gemini_generation_config(&json!({
                "model": "claude-sonnet",
                "max_tokens": 128,
                "temperature": 0.2,
                "top_p": 0.9,
                "stop_sequences": ["END"],
                "stream": true
            })),
            Some(json!({
                "maxOutputTokens": 128,
                "temperature": 0.2,
                "topP": 0.9,
                "stopSequences": ["END"]
            }))
        );
    }

    #[test]
    fn omits_empty_gemini_generation_config() {
        assert_eq!(
            build_gemini_generation_config(&json!({ "model": "x" })),
            None
        );
    }

    #[test]
    fn maps_gemini_tool_choice_to_function_calling_config() {
        assert_eq!(
            map_gemini_tool_choice_to_config(Some(&json!("auto"))).unwrap(),
            Some(json!({ "functionCallingConfig": { "mode": "AUTO" } }))
        );
        assert_eq!(
            map_gemini_tool_choice_to_config(Some(&json!("none"))).unwrap(),
            Some(json!({ "functionCallingConfig": { "mode": "NONE" } }))
        );
        assert_eq!(
            map_gemini_tool_choice_to_config(Some(&json!({ "type": "any" }))).unwrap(),
            Some(json!({ "functionCallingConfig": { "mode": "ANY" } }))
        );
        assert_eq!(
            map_gemini_tool_choice_to_config(Some(&json!({
                "type": "tool",
                "name": "get_weather"
            })))
            .unwrap(),
            Some(json!({
                "functionCallingConfig": {
                    "mode": "ANY",
                    "allowedFunctionNames": ["get_weather"]
                }
            }))
        );
        assert_eq!(map_gemini_tool_choice_to_config(None).unwrap(), None);
        assert_eq!(
            map_gemini_tool_choice_to_config(Some(&json!({ "name": "missing_type" }))).unwrap(),
            None
        );
    }

    #[test]
    fn rejects_unsupported_gemini_tool_choice_values() {
        assert_eq!(
            map_gemini_tool_choice_to_config(Some(&json!("required"))).unwrap_err(),
            "Unsupported Gemini tool_choice string: required"
        );
        assert_eq!(
            map_gemini_tool_choice_to_config(Some(&json!({ "type": "function" }))).unwrap_err(),
            "Unsupported Gemini tool_choice type: function"
        );
    }

    #[test]
    fn converts_anthropic_tool_use_and_result_parts() {
        let content = json!([
            { "type": "tool_use", "id": "call_1", "name": "lookup", "input": { "q": "rust" } },
            { "type": "tool_result", "tool_use_id": "call_1", "content": "ok" }
        ]);
        let mut names = HashMap::new();
        let signatures = HashMap::from([("call_1".to_string(), "sig-1".to_string())]);

        let parts = anthropic_message_content_to_gemini_parts(
            Some(&content),
            "assistant",
            &mut names,
            &signatures,
        )
        .unwrap();

        assert_eq!(parts[0]["functionCall"]["id"], "call_1");
        assert_eq!(parts[0]["functionCall"]["name"], "lookup");
        assert_eq!(parts[0]["functionCall"]["args"]["q"], "rust");
        assert_eq!(parts[0]["functionCall"]["thoughtSignature"], "sig-1");
        assert_eq!(parts[1]["functionResponse"]["id"], "call_1");
        assert_eq!(parts[1]["functionResponse"]["name"], "lookup");
        assert_eq!(parts[1]["functionResponse"]["response"]["content"], "ok");
        assert_eq!(names.get("call_1").map(String::as_str), Some("lookup"));
    }

    #[test]
    fn strips_synthesized_ids_from_gemini_request_parts() {
        let synth_id = crate::synthesize_gemini_tool_call_id("test-id");
        let content = json!([
            { "type": "tool_use", "id": synth_id, "name": "lookup", "input": {} },
            { "type": "tool_result", "tool_use_id": synth_id, "content": "ok" }
        ]);
        let mut names = HashMap::new();

        let parts = anthropic_message_content_to_gemini_parts(
            Some(&content),
            "assistant",
            &mut names,
            &HashMap::new(),
        )
        .unwrap();

        assert!(parts[0]["functionCall"].get("id").is_none());
        assert!(parts[1]["functionResponse"].get("id").is_none());
        assert_eq!(parts[1]["functionResponse"]["name"], "lookup");
    }

    #[test]
    fn rejects_tool_result_without_resolvable_name() {
        let content = json!([
            { "type": "tool_result", "tool_use_id": "call_missing", "content": "ok" }
        ]);
        let error = anthropic_message_content_to_gemini_parts(
            Some(&content),
            "user",
            &mut HashMap::new(),
            &HashMap::new(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            "Unable to resolve Gemini functionResponse.name for tool_use_id `call_missing`"
        );
    }
}
