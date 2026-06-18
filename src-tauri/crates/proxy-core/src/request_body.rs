use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

const ANTHROPIC_BILLING_HEADER_PREFIX: &str = "x-anthropic-billing-header:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBodyFilterResult {
    pub body: Value,
    #[serde(default)]
    pub removed_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedUpstreamRequestBody {
    pub body: Value,
    #[serde(default)]
    pub removed_private_keys: Vec<String>,
}

pub fn prepare_upstream_request_body(request_body: Value) -> Value {
    prepare_upstream_request_body_with_report(request_body).body
}

pub fn serialize_upstream_request_body(
    method: &http::Method,
    body: &Value,
) -> serde_json::Result<Vec<u8>> {
    if !method_allows_upstream_request_body(method) {
        return Ok(Vec::new());
    }

    serde_json::to_vec(body)
}

pub fn method_allows_upstream_request_body(method: &http::Method) -> bool {
    !matches!(method, &http::Method::GET | &http::Method::HEAD)
}

pub fn is_openai_o_series(model: &str) -> bool {
    model.len() > 1
        && model.starts_with('o')
        && model.as_bytes().get(1).is_some_and(|byte| byte.is_ascii_digit())
}

pub fn supports_reasoning_effort(model: &str) -> bool {
    is_openai_o_series(model)
        || model
            .to_lowercase()
            .strip_prefix("gpt-")
            .and_then(|rest| rest.chars().next())
            .is_some_and(|value| value.is_ascii_digit() && value >= '5')
}

pub fn resolve_reasoning_effort(body: &Value) -> Option<&'static str> {
    if let Some(effort) = body
        .pointer("/output_config/effort")
        .and_then(|value| value.as_str())
    {
        return match effort {
            "low" => Some("low"),
            "medium" => Some("medium"),
            "high" => Some("high"),
            "max" => Some("xhigh"),
            _ => None,
        };
    }

    let thinking = body.get("thinking")?;
    match thinking.get("type").and_then(|value| value.as_str()) {
        Some("adaptive") => Some("xhigh"),
        Some("enabled") => {
            let budget = thinking
                .get("budget_tokens")
                .and_then(|value| value.as_u64());
            match budget {
                Some(value) if value < 4_000 => Some("low"),
                Some(value) if value < 16_000 => Some("medium"),
                Some(_) => Some("high"),
                None => Some("high"),
            }
        }
        _ => None,
    }
}

pub fn strip_leading_anthropic_billing_header(text: &str) -> &str {
    if !text.starts_with(ANTHROPIC_BILLING_HEADER_PREFIX) {
        return text;
    }

    let Some(line_end) = text
        .as_bytes()
        .iter()
        .position(|byte| *byte == b'\n' || *byte == b'\r')
    else {
        return "";
    };

    let bytes = text.as_bytes();
    let mut rest_start = line_end + 1;
    if bytes[line_end] == b'\r' && bytes.get(line_end + 1) == Some(&b'\n') {
        rest_start += 1;
    }

    let rest = &text[rest_start..];
    if let Some(stripped) = rest.strip_prefix("\r\n") {
        stripped
    } else if let Some(stripped) = rest.strip_prefix('\n') {
        stripped
    } else if let Some(stripped) = rest.strip_prefix('\r') {
        stripped
    } else {
        rest
    }
}

pub fn map_anthropic_tool_choice_to_openai_chat(tool_choice: &Value) -> Value {
    match tool_choice {
        Value::String(value) => match value.as_str() {
            "any" => Value::String("required".to_string()),
            _ => Value::String(value.clone()),
        },
        Value::Object(object) => match object.get("type").and_then(|value| value.as_str()) {
            Some("any") => Value::String("required".to_string()),
            Some("auto") => Value::String("auto".to_string()),
            Some("none") => Value::String("none".to_string()),
            Some("tool") => {
                let name = object
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                serde_json::json!({
                    "type": "function",
                    "function": {"name": name}
                })
            }
            _ => tool_choice.clone(),
        },
        _ => tool_choice.clone(),
    }
}

pub fn prepare_upstream_request_body_with_report(
    request_body: Value,
) -> PreparedUpstreamRequestBody {
    let filtered = filter_private_params_with_whitelist_report(request_body, &[]);

    PreparedUpstreamRequestBody {
        body: canonicalize_request_body_value(filtered.body),
        removed_private_keys: filtered.removed_keys,
    }
}

pub fn filter_private_params(body: Value) -> Value {
    filter_private_params_with_whitelist(body, &[])
}

pub fn filter_private_params_with_whitelist(body: Value, whitelist: &[String]) -> Value {
    filter_private_params_with_whitelist_report(body, whitelist).body
}

pub fn filter_private_params_with_whitelist_report(
    body: Value,
    whitelist: &[String],
) -> RequestBodyFilterResult {
    let whitelist_set: HashSet<&str> = whitelist.iter().map(String::as_str).collect();
    let mut removed_keys = Vec::new();
    let body = filter_recursive_with_whitelist(
        body,
        &mut Vec::new(),
        &mut removed_keys,
        &whitelist_set,
    );

    RequestBodyFilterResult { body, removed_keys }
}

pub fn canonicalize_request_body_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(canonicalize_request_body_value)
                .collect(),
        ),
        Value::Object(map) => {
            let mut entries = map.into_iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));

            let mut sorted = serde_json::Map::new();
            for (key, value) in entries {
                sorted.insert(key, canonicalize_request_body_value(value));
            }
            Value::Object(sorted)
        }
        other => other,
    }
}

fn filter_recursive_with_whitelist(
    value: Value,
    path: &mut Vec<String>,
    removed_keys: &mut Vec<String>,
    whitelist: &HashSet<&str>,
) -> Value {
    match value {
        Value::Object(map) => {
            let is_schema_name_map = path.last().is_some_and(|key| matches_schema_name_map(key));
            let filtered: serde_json::Map<String, Value> = map
                .into_iter()
                .filter_map(|(key, value)| {
                    if key.starts_with('_')
                        && !whitelist.contains(key.as_str())
                        && !is_schema_name_map
                    {
                        removed_keys.push(key);
                        None
                    } else {
                        path.push(key.clone());
                        let filtered_value =
                            filter_recursive_with_whitelist(value, path, removed_keys, whitelist);
                        path.pop();
                        Some((key, filtered_value))
                    }
                })
                .collect();

            Value::Object(filtered)
        }
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| filter_recursive_with_whitelist(value, path, removed_keys, whitelist))
                .collect(),
        ),
        other => other,
    }
}

fn matches_schema_name_map(key: &str) -> bool {
    matches!(
        key,
        "properties" | "patternProperties" | "definitions" | "$defs"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        canonicalize_request_body_value, filter_private_params,
        filter_private_params_with_whitelist, filter_private_params_with_whitelist_report,
        is_openai_o_series, method_allows_upstream_request_body,
        map_anthropic_tool_choice_to_openai_chat, prepare_upstream_request_body_with_report,
        resolve_reasoning_effort, serialize_upstream_request_body,
        strip_leading_anthropic_billing_header, supports_reasoning_effort,
    };
    use http::Method;
    use serde_json::json;

    #[test]
    fn filters_private_fields_recursively_but_preserves_schema_property_names() {
        let input = json!({
            "model": "claude-3",
            "_internal": "drop",
            "messages": [{"role": "user", "_token": "drop"}],
            "tools": [{
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "_id": {"type": "string", "_private_note": "drop"},
                        "_meta": {"type": "object"}
                    },
                    "_schema_note": "drop"
                }
            }]
        });

        let result = filter_private_params_with_whitelist_report(input, &[]);

        assert!(result.body.get("_internal").is_none());
        assert!(result.body["messages"][0].get("_token").is_none());
        assert!(result.body["tools"][0]["input_schema"]["properties"]
            .get("_id")
            .is_some());
        assert!(result.body["tools"][0]["input_schema"]["properties"]["_id"]
            .get("_private_note")
            .is_none());
        assert!(result.body["tools"][0]["input_schema"]
            .get("_schema_note")
            .is_none());
        assert_eq!(
            result.removed_keys,
            vec![
                "_internal".to_string(),
                "_token".to_string(),
                "_private_note".to_string(),
                "_schema_note".to_string(),
            ]
        );
    }

    #[test]
    fn filter_private_params_with_whitelist_keeps_named_private_fields() {
        let input = json!({
            "_metadata": {"keep": true},
            "_internal": "drop",
            "nested": {"_metadata": "keep", "_internal": "drop"}
        });
        let whitelist = vec!["_metadata".to_string()];

        let output = filter_private_params_with_whitelist(input, &whitelist);

        assert!(output.get("_metadata").is_some());
        assert!(output.get("_internal").is_none());
        assert!(output["nested"].get("_metadata").is_some());
        assert!(output["nested"].get("_internal").is_none());
    }

    #[test]
    fn filter_private_params_preserves_primitive_values() {
        assert_eq!(filter_private_params(json!(42)), json!(42));
        assert_eq!(filter_private_params(json!("text")), json!("text"));
        assert_eq!(filter_private_params(json!(true)), json!(true));
        assert_eq!(filter_private_params(json!(null)), json!(null));
    }

    #[test]
    fn canonicalize_request_body_value_sorts_object_keys_recursively() {
        let value = canonicalize_request_body_value(json!({
            "z": 1,
            "a": {"d": true, "b": false},
            "list": [{"y": 2, "x": 1}]
        }));

        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            r#"{"a":{"b":false,"d":true},"list":[{"x":1,"y":2}],"z":1}"#
        );
    }

    #[test]
    fn prepare_upstream_request_body_filters_and_canonicalizes() {
        let prepared = prepare_upstream_request_body_with_report(json!({
            "z": 1,
            "_internal": "drop",
            "tools": [{
                "name": "lookup",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "_id": {"_private_note": "drop", "type": "string"},
                        "b": {"type": "number"},
                        "a": {"type": "string"}
                    }
                }
            }],
            "a": 2
        }));

        assert_eq!(
            serde_json::to_string(&prepared.body).unwrap(),
            r#"{"a":2,"tools":[{"name":"lookup","parameters":{"properties":{"_id":{"type":"string"},"a":{"type":"string"},"b":{"type":"number"}},"type":"object"}}],"z":1}"#
        );
        assert_eq!(
            prepared.removed_private_keys,
            vec!["_internal".to_string(), "_private_note".to_string()]
        );
    }

    #[test]
    fn get_and_head_do_not_send_upstream_request_body() {
        let body = json!({"model": "gpt-5"});

        assert!(!method_allows_upstream_request_body(&Method::GET));
        assert!(!method_allows_upstream_request_body(&Method::HEAD));
        assert_eq!(
            serialize_upstream_request_body(&Method::GET, &body).unwrap(),
            Vec::<u8>::new()
        );
        assert_eq!(
            serialize_upstream_request_body(&Method::HEAD, &body).unwrap(),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn non_safe_methods_serialize_json_body() {
        let body = canonicalize_request_body_value(json!({"b": 2, "a": 1}));

        assert!(method_allows_upstream_request_body(&Method::POST));
        assert_eq!(
            serialize_upstream_request_body(&Method::POST, &body).unwrap(),
            br#"{"a":1,"b":2}"#.to_vec()
        );
    }

    #[test]
    fn detects_openai_reasoning_model_families() {
        assert!(is_openai_o_series("o1"));
        assert!(is_openai_o_series("o3-mini"));
        assert!(is_openai_o_series("o4-mini"));
        assert!(!is_openai_o_series("gpt-4o"));
        assert!(!is_openai_o_series("o"));

        assert!(supports_reasoning_effort("o1"));
        assert!(supports_reasoning_effort("gpt-5"));
        assert!(supports_reasoning_effort("gpt-5-codex"));
        assert!(!supports_reasoning_effort("gpt-4o"));
        assert!(!supports_reasoning_effort("claude-sonnet-4-6"));
    }

    #[test]
    fn resolves_reasoning_effort_from_output_config_first() {
        assert_eq!(
            resolve_reasoning_effort(&json!({
                "output_config": {"effort": "low"},
                "thinking": {"type": "adaptive"}
            })),
            Some("low")
        );
        assert_eq!(
            resolve_reasoning_effort(&json!({"output_config": {"effort": "max"}})),
            Some("xhigh")
        );
        assert_eq!(
            resolve_reasoning_effort(&json!({"output_config": {"effort": "turbo"}})),
            None
        );
    }

    #[test]
    fn resolves_reasoning_effort_from_thinking_budget() {
        assert_eq!(
            resolve_reasoning_effort(
                &json!({"thinking": {"type": "enabled", "budget_tokens": 1024}})
            ),
            Some("low")
        );
        assert_eq!(
            resolve_reasoning_effort(
                &json!({"thinking": {"type": "enabled", "budget_tokens": 8000}})
            ),
            Some("medium")
        );
        assert_eq!(
            resolve_reasoning_effort(
                &json!({"thinking": {"type": "enabled", "budget_tokens": 32000}})
            ),
            Some("high")
        );
        assert_eq!(
            resolve_reasoning_effort(&json!({"thinking": {"type": "enabled"}})),
            Some("high")
        );
        assert_eq!(
            resolve_reasoning_effort(&json!({"thinking": {"type": "adaptive"}})),
            Some("xhigh")
        );
        assert_eq!(
            resolve_reasoning_effort(&json!({"thinking": {"type": "disabled"}})),
            None
        );
    }

    #[test]
    fn strips_only_leading_anthropic_billing_header() {
        assert_eq!(
            strip_leading_anthropic_billing_header(
                "x-anthropic-billing-header:cch=abc\n\nKeep this prompt"
            ),
            "Keep this prompt"
        );
        assert_eq!(
            strip_leading_anthropic_billing_header(
                "x-anthropic-billing-header:cch=abc\r\nKeep this prompt"
            ),
            "Keep this prompt"
        );
        assert_eq!(
            strip_leading_anthropic_billing_header("Keep\nx-anthropic-billing-header:cch=abc"),
            "Keep\nx-anthropic-billing-header:cch=abc"
        );
        assert_eq!(
            strip_leading_anthropic_billing_header("x-anthropic-billing-header:cch=abc"),
            ""
        );
    }

    #[test]
    fn maps_anthropic_tool_choice_to_openai_chat_shape() {
        assert_eq!(
            map_anthropic_tool_choice_to_openai_chat(&json!("any")),
            json!("required")
        );
        assert_eq!(
            map_anthropic_tool_choice_to_openai_chat(&json!("auto")),
            json!("auto")
        );
        assert_eq!(
            map_anthropic_tool_choice_to_openai_chat(&json!({"type": "any"})),
            json!("required")
        );
        assert_eq!(
            map_anthropic_tool_choice_to_openai_chat(&json!({"type": "none"})),
            json!("none")
        );
        assert_eq!(
            map_anthropic_tool_choice_to_openai_chat(
                &json!({"type": "tool", "name": "search"})
            ),
            json!({"type": "function", "function": {"name": "search"}})
        );
    }
}
