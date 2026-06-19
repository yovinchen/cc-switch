//! Gemini Native request helpers.

use serde_json::{Map, Value, json};

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
