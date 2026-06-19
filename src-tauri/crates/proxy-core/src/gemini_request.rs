//! Gemini Native request helpers.

use serde_json::{Map, Value};

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
