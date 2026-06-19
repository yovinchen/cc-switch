//! Gemini Native tool-call argument rectification.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnthropicToolSchemaHint {
    expected_keys: Vec<String>,
    required_keys: Vec<String>,
}

pub type AnthropicToolSchemaHints = HashMap<String, AnthropicToolSchemaHint>;

pub fn extract_anthropic_tool_schema_hints(body: &Value) -> AnthropicToolSchemaHints {
    body.get("tools")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            let name = tool.get("name").and_then(|value| value.as_str())?;
            let input_schema = tool
                .get("input_schema")
                .and_then(|value| value.as_object())?;
            let properties = input_schema
                .get("properties")
                .and_then(|value| value.as_object())?;
            if properties.is_empty() {
                return None;
            }

            let expected_keys = properties.keys().cloned().collect::<Vec<_>>();
            let required_keys = input_schema
                .get("required")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            Some((
                name.to_string(),
                AnthropicToolSchemaHint {
                    expected_keys,
                    required_keys,
                },
            ))
        })
        .collect()
}

pub fn rectify_gemini_tool_call_parts(
    parts: &mut [Value],
    tool_schema_hints: Option<&AnthropicToolSchemaHints>,
) -> Vec<String> {
    let mut changed_tools = Vec::new();

    for part in parts {
        let Some(function_call) = part
            .get_mut("functionCall")
            .and_then(|value| value.as_object_mut())
        else {
            continue;
        };
        let Some(name) = function_call
            .get("name")
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
        else {
            continue;
        };
        let Some(args) = function_call.get_mut("args") else {
            continue;
        };

        if rectify_gemini_tool_call_args(&name, args, tool_schema_hints) {
            changed_tools.push(name);
        }
    }

    changed_tools
}

pub fn rectify_gemini_tool_call_args(
    tool_name: &str,
    args: &mut Value,
    tool_schema_hints: Option<&AnthropicToolSchemaHints>,
) -> bool {
    let Some(tool_schema_hints) = tool_schema_hints else {
        return false;
    };
    let Some(hint) = tool_schema_hints.get(tool_name) else {
        return false;
    };
    let Some(args_object) = args.as_object_mut() else {
        return false;
    };
    if args_object.is_empty() || hint.expected_keys.is_empty() {
        return false;
    }
    let mut changed = false;

    if hint.expected_keys.iter().any(|key| key == "skill") && !args_object.contains_key("skill") {
        if let Some(value) = args_object.remove("name") {
            args_object.insert("skill".to_string(), value);
            changed = true;
        }
    }

    let expects_parameters_key = hint.expected_keys.iter().any(|key| key == "parameters");
    if !expects_parameters_key {
        let extracted_parameters = args_object
            .get("parameters")
            .and_then(|value| value.as_object())
            .map(|parameters_object| {
                hint.expected_keys
                    .iter()
                    .filter_map(|expected_key| {
                        if args_object.contains_key(expected_key) {
                            return None;
                        }
                        let value = parameters_object.get(expected_key)?;
                        let normalized_value = match value {
                            Value::Array(values) if values.len() == 1 => values[0].clone(),
                            _ => value.clone(),
                        };
                        Some((expected_key.clone(), normalized_value))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if !extracted_parameters.is_empty() {
            for (expected_key, normalized_value) in extracted_parameters {
                args_object.insert(expected_key, normalized_value);
            }
            args_object.remove("parameters");
            changed = true;
        }
    }

    if hint
        .required_keys
        .iter()
        .all(|key| args_object.contains_key(key.as_str()))
    {
        return changed;
    }

    let expected_key_set = hint
        .expected_keys
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let unexpected_keys = args_object
        .keys()
        .filter(|key| !expected_key_set.contains(key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if unexpected_keys.len() != 1 {
        return changed;
    }

    let target_key = hint
        .required_keys
        .iter()
        .find(|key| !args_object.contains_key(key.as_str()))
        .cloned()
        .or_else(|| {
            if hint.expected_keys.len() == 1 && args_object.len() == 1 {
                hint.expected_keys.first().cloned()
            } else {
                None
            }
        });
    let Some(target_key) = target_key else {
        return false;
    };
    if args_object.contains_key(&target_key) {
        return false;
    }

    let source_key = &unexpected_keys[0];
    let Some(value) = args_object.remove(source_key) else {
        return false;
    };
    args_object.insert(target_key, value);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_schema_hints_from_anthropic_tools() {
        let hints = extract_anthropic_tool_schema_hints(&json!({
            "tools": [{
                "name": "Skill",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "skill": { "type": "string" },
                        "args": { "type": "string" }
                    },
                    "required": ["skill"]
                }
            }]
        }));

        let hint = hints.get("Skill").expect("hint");
        assert_eq!(hint.expected_keys.len(), 2);
        assert!(hint.expected_keys.contains(&"args".to_string()));
        assert!(hint.expected_keys.contains(&"skill".to_string()));
        assert_eq!(hint.required_keys, vec!["skill".to_string()]);
    }

    #[test]
    fn rectifies_name_and_nested_parameters() {
        let hints = extract_anthropic_tool_schema_hints(&json!({
            "tools": [{
                "name": "Skill",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "skill": { "type": "string" },
                        "args": { "type": "string" }
                    },
                    "required": ["skill", "args"]
                }
            }]
        }));
        let mut args = json!({
            "name": "git-commit",
            "parameters": {
                "args": ["详细分析内容 编写提交信息 分多次提交代码"]
            }
        });

        assert!(rectify_gemini_tool_call_args("Skill", &mut args, Some(&hints)));
        assert_eq!(args["skill"], "git-commit");
        assert_eq!(args["args"], "详细分析内容 编写提交信息 分多次提交代码");
        assert!(args.get("parameters").is_none());
        assert!(args.get("name").is_none());
    }

    #[test]
    fn rectifies_single_unexpected_key_to_missing_required_key() {
        let hints = extract_anthropic_tool_schema_hints(&json!({
            "tools": [{
                "name": "Lookup",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                }
            }]
        }));
        let mut parts = vec![json!({
            "functionCall": {
                "name": "Lookup",
                "args": {
                    "payload": "weather"
                }
            }
        })];

        assert_eq!(
            rectify_gemini_tool_call_parts(&mut parts, Some(&hints)),
            vec!["Lookup".to_string()]
        );
        assert_eq!(parts[0]["functionCall"]["args"]["query"], "weather");
    }
}
