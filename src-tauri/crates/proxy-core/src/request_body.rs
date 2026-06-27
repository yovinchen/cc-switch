use crate::domain::ResolvedChannelAttempt;
use crate::json_canonical::short_value_hash;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestBodyJsonParseError {
    message: String,
}

impl RequestBodyJsonParseError {
    fn new(error: serde_json::Error) -> Self {
        Self {
            message: format!("Failed to parse request body: {error}"),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RequestBodyJsonParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RequestBodyJsonParseError {}

#[derive(Debug, Clone, Copy)]
pub struct PromptCacheTraceLogInput<'a> {
    pub app: &'a str,
    pub provider_id: &'a str,
    pub endpoint: &'a str,
    pub api_format: Option<&'a str>,
    pub body: &'a Value,
    pub session_client_provided: bool,
}

pub fn prepare_upstream_request_body(request_body: Value) -> Value {
    prepare_upstream_request_body_with_report(request_body).body
}

pub fn parse_json_request_body(bytes: &[u8]) -> Result<Value, RequestBodyJsonParseError> {
    serde_json::from_slice(bytes).map_err(RequestBodyJsonParseError::new)
}

pub fn parse_json_request_body_or_null(bytes: &[u8]) -> Result<Value, RequestBodyJsonParseError> {
    if bytes.is_empty() {
        Ok(Value::Null)
    } else {
        parse_json_request_body(bytes)
    }
}

pub fn forwarder_request_body_model(body: &Value) -> Option<String> {
    body.get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
}

pub fn request_body_read_error_message(error: impl fmt::Display) -> String {
    format!("Failed to read request body: {error}")
}

pub fn request_body_serialize_error_message(error: impl fmt::Display) -> String {
    format!("Failed to serialize request body: {error}")
}

pub fn request_body_filter_log_message(report: &PreparedUpstreamRequestBody) -> Option<String> {
    (!report.removed_private_keys.is_empty()).then(|| {
        format!(
            "[BodyFilter] 过滤私有参数: {:?}",
            report.removed_private_keys
        )
    })
}

pub fn prompt_cache_trace_log_message(input: PromptCacheTraceLogInput<'_>) -> String {
    let prompt_cache_key = input
        .body
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(|key| format!("present(len={})", key.len()))
        .unwrap_or_else(|| "absent".to_string());
    let store = input
        .body
        .get("store")
        .map(value_for_log)
        .unwrap_or_else(|| "absent".to_string());
    let stream = input
        .body
        .get("stream")
        .map(value_for_log)
        .unwrap_or_else(|| "absent".to_string());

    format!(
        "[CacheTrace] app={}, provider={}, endpoint={}, api_format={}, session_client_provided={}, prompt_cache_key={}, store={}, stream={}, instructions_hash={}, tools_hash={}, input_hash={}, include_hash={}, body_hash={}",
        input.app,
        input.provider_id,
        input.endpoint,
        input.api_format.unwrap_or("native"),
        input.session_client_provided,
        prompt_cache_key,
        store,
        stream,
        short_value_hash(input.body.get("instructions")),
        short_value_hash(input.body.get("tools")),
        short_value_hash(input.body.get("input")),
        short_value_hash(input.body.get("include")),
        short_value_hash(Some(input.body)),
    )
}

fn value_for_log(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Null => "null".to_string(),
        Value::Array(values) => format!("array(len={})", values.len()),
        Value::Object(values) => format!("object(len={})", values.len()),
    }
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

pub fn codex_chat_reasoning_requested(body: &Value) -> Option<bool> {
    if let Some(effort) = body
        .pointer("/reasoning/effort")
        .and_then(|value| value.as_str())
    {
        return Some(!matches!(
            effort.trim().to_ascii_lowercase().as_str(),
            "none" | "off" | "disabled"
        ));
    }

    body.get("reasoning").map(|value| !value.is_null())
}

pub fn resolve_codex_provider_upstream_model(
    settings_model: Option<&str>,
    config_model: Option<&str>,
) -> Option<String> {
    settings_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            config_model
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(ToString::to_string)
        })
}

pub fn codex_provider_catalog_model_ids_from_settings(settings_config: &Value) -> HashSet<String> {
    settings_config
        .get("modelCatalog")
        .and_then(|catalog| catalog.get("models"))
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model.get("model").and_then(Value::as_str))
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn apply_codex_chat_upstream_model_policy(
    body: &mut Value,
    uses_chat_completions: bool,
    upstream_model: Option<&str>,
    catalog_model_ids: &HashSet<String>,
) -> Option<String> {
    if !uses_chat_completions {
        return None;
    }

    if let Some(request_model) = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        if catalog_model_ids.contains(request_model) {
            return Some(request_model.to_string());
        }
    }

    let upstream_model = upstream_model.map(str::trim).filter(|model| !model.is_empty())?;
    body["model"] = Value::String(upstream_model.to_string());
    Some(upstream_model.to_string())
}

pub fn apply_channel_route_model_override(
    body: &mut Value,
    public_model: Option<&str>,
    upstream_model: Option<&str>,
) -> Option<String> {
    let upstream_model = upstream_model.map(str::trim).filter(|model| !model.is_empty())?;
    let current_model = body.get("model").and_then(Value::as_str)?;

    let should_override = public_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(|public_model| current_model == public_model)
        .unwrap_or(false)
        || current_model == upstream_model;

    if should_override && current_model != upstream_model {
        body["model"] = Value::String(upstream_model.to_string());
        Some(upstream_model.to_string())
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRouteModelOverride {
    pub channel_id: String,
    pub previous_model: String,
    pub upstream_model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRouteRequestOverrideApplication {
    pub channel_id: String,
    pub applied_keys: Vec<String>,
}

pub fn apply_resolved_channel_model_override(
    body: &mut Value,
    channel: &ResolvedChannelAttempt,
) -> Option<ChannelRouteModelOverride> {
    let previous_model = body.get("model").and_then(Value::as_str)?.to_string();
    let upstream_model = apply_channel_route_model_override(
        body,
        channel.public_model.as_deref(),
        channel.upstream_model.as_deref(),
    )?;

    Some(ChannelRouteModelOverride {
        channel_id: channel.channel_id.clone(),
        previous_model,
        upstream_model,
    })
}

pub fn apply_resolved_channel_request_overrides(
    body: &mut Value,
    channel: &ResolvedChannelAttempt,
) -> Option<ChannelRouteRequestOverrideApplication> {
    let request_overrides = channel.request_overrides.as_object()?;
    if request_overrides.is_empty() {
        return None;
    }

    let body_object = body.as_object_mut()?;
    let mut applied_keys = Vec::with_capacity(request_overrides.len());
    for (key, value) in request_overrides {
        body_object.insert(key.clone(), value.clone());
        applied_keys.push(key.clone());
    }
    applied_keys.sort();

    Some(ChannelRouteRequestOverrideApplication {
        channel_id: channel.channel_id.clone(),
        applied_keys,
    })
}

pub fn map_codex_chat_reasoning_effort(
    effort: &str,
    mode: Option<&str>,
) -> Option<&'static str> {
    let effort = effort.trim().to_ascii_lowercase();
    if matches!(effort.as_str(), "none" | "off" | "disabled") {
        return None;
    }

    match mode.unwrap_or("passthrough") {
        "deepseek" => match effort.as_str() {
            "max" | "xhigh" => Some("max"),
            _ => Some("high"),
        },
        "low_high" => match effort.as_str() {
            "minimal" | "low" => Some("low"),
            _ => Some("high"),
        },
        // OpenRouter accepts xhigh|high|medium|low|minimal, but not max.
        "openrouter" => match effort.as_str() {
            "max" | "xhigh" => Some("xhigh"),
            "high" => Some("high"),
            "medium" => Some("medium"),
            "low" => Some("low"),
            "minimal" => Some("minimal"),
            _ => None,
        },
        _ => match effort.as_str() {
            "minimal" => Some("minimal"),
            "low" => Some("low"),
            "medium" => Some("medium"),
            "high" => Some("high"),
            "xhigh" => Some("xhigh"),
            "max" => Some("max"),
            _ => None,
        },
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

pub fn map_anthropic_tool_choice_to_openai_responses(tool_choice: &Value) -> Value {
    match tool_choice {
        Value::String(_) => tool_choice.clone(),
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
                    "name": name
                })
            }
            _ => tool_choice.clone(),
        },
        _ => tool_choice.clone(),
    }
}

pub fn inject_openai_stream_include_usage(body: &mut Value) {
    let is_stream = body
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !is_stream {
        return;
    }

    match body.get_mut("stream_options") {
        Some(Value::Object(options)) => {
            options.insert("include_usage".to_string(), Value::Bool(true));
        }
        _ => {
            body["stream_options"] = serde_json::json!({ "include_usage": true });
        }
    }
}

pub fn clean_openai_tool_schema(mut schema: Value) -> Value {
    if let Some(object) = schema.as_object_mut() {
        if object.get("format").and_then(|value| value.as_str()) == Some("uri") {
            object.remove("format");
        }

        if let Some(properties) = object
            .get_mut("properties")
            .and_then(|value| value.as_object_mut())
        {
            for value in properties.values_mut() {
                *value = clean_openai_tool_schema(value.clone());
            }
        }

        if let Some(items) = object.get_mut("items") {
            *items = clean_openai_tool_schema(items.clone());
        }
    }

    schema
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
        apply_channel_route_model_override, apply_codex_chat_upstream_model_policy,
        apply_resolved_channel_model_override, apply_resolved_channel_request_overrides,
        canonicalize_request_body_value, clean_openai_tool_schema, codex_chat_reasoning_requested,
        codex_provider_catalog_model_ids_from_settings, filter_private_params,
        filter_private_params_with_whitelist, filter_private_params_with_whitelist_report,
        forwarder_request_body_model, inject_openai_stream_include_usage, is_openai_o_series,
        map_anthropic_tool_choice_to_openai_chat, map_anthropic_tool_choice_to_openai_responses,
        map_codex_chat_reasoning_effort, method_allows_upstream_request_body,
        parse_json_request_body, parse_json_request_body_or_null,
        prepare_upstream_request_body_with_report, prompt_cache_trace_log_message,
        request_body_filter_log_message, request_body_read_error_message,
        request_body_serialize_error_message, resolve_codex_provider_upstream_model,
        resolve_reasoning_effort, serialize_upstream_request_body,
        strip_leading_anthropic_billing_header, supports_reasoning_effort,
        PromptCacheTraceLogInput,
    };
    use crate::domain::ResolvedChannelAttempt;
    use http::Method;
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn forwarder_request_body_model_projects_non_empty_raw_model() {
        assert_eq!(
            forwarder_request_body_model(&json!({ "model": "upstream-sonnet" })).as_deref(),
            Some("upstream-sonnet")
        );
        assert_eq!(forwarder_request_body_model(&json!({ "model": "" })), None);
        assert_eq!(forwarder_request_body_model(&json!({})), None);
        assert_eq!(
            forwarder_request_body_model(&json!({ "model": "  " })).as_deref(),
            Some("  ")
        );
    }

    #[test]
    fn parses_json_request_body_with_stable_error_message() {
        assert_eq!(
            parse_json_request_body(br#"{"model":"gpt-5"}"#).unwrap(),
            json!({"model": "gpt-5"})
        );

        let error = parse_json_request_body(b"{").unwrap_err();
        assert!(error
            .message()
            .starts_with("Failed to parse request body:"));
        assert_eq!(error.to_string(), error.message());
    }

    #[test]
    fn parses_empty_json_request_body_as_null_when_allowed() {
        assert_eq!(parse_json_request_body_or_null(b"").unwrap(), json!(null));
        assert_eq!(
            parse_json_request_body_or_null(br#"{"stream":true}"#).unwrap(),
            json!({"stream": true})
        );
    }

    #[test]
    fn formats_request_body_read_errors() {
        assert_eq!(
            request_body_read_error_message("connection reset"),
            "Failed to read request body: connection reset"
        );
    }

    #[test]
    fn formats_request_body_serialize_errors() {
        assert_eq!(
            request_body_serialize_error_message("bad value"),
            "Failed to serialize request body: bad value"
        );
    }

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
    fn request_body_filter_log_message_reports_removed_private_keys() {
        let clean = prepare_upstream_request_body_with_report(json!({"model": "gpt-5"}));
        assert_eq!(request_body_filter_log_message(&clean), None);

        let filtered = prepare_upstream_request_body_with_report(json!({
            "_internal": "drop",
            "messages": [{"role": "user", "content": "hi"}]
        }));

        assert_eq!(
            request_body_filter_log_message(&filtered).as_deref(),
            Some(r#"[BodyFilter] 过滤私有参数: ["_internal"]"#)
        );
    }

    #[test]
    fn prompt_cache_trace_log_message_summarizes_cache_relevant_fields() {
        let body = json!({
            "prompt_cache_key": "cache-key",
            "store": false,
            "stream": true,
            "instructions": "Be concise",
            "tools": [{"type": "function", "name": "lookup"}],
            "input": [{"role": "user", "content": "hi"}],
            "include": ["reasoning.encrypted_content"]
        });

        let message = prompt_cache_trace_log_message(PromptCacheTraceLogInput {
            app: "Claude",
            provider_id: "provider-1",
            endpoint: "/v1/responses",
            api_format: Some("openai_responses"),
            body: &body,
            session_client_provided: true,
        });

        assert!(message.starts_with("[CacheTrace] app=Claude"), "{message}");
        assert!(message.contains("provider=provider-1"), "{message}");
        assert!(message.contains("endpoint=/v1/responses"), "{message}");
        assert!(message.contains("api_format=openai_responses"), "{message}");
        assert!(
            message.contains("session_client_provided=true"),
            "{message}"
        );
        assert!(message.contains("prompt_cache_key=present(len=9)"), "{message}");
        assert!(message.contains("store=false"), "{message}");
        assert!(message.contains("stream=true"), "{message}");
        assert!(message.contains("tools_hash="), "{message}");
        assert!(message.contains("body_hash="), "{message}");
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
    fn detects_codex_chat_reasoning_request_state() {
        assert_eq!(
            codex_chat_reasoning_requested(&json!({"reasoning": {"effort": "high"}})),
            Some(true)
        );
        assert_eq!(
            codex_chat_reasoning_requested(&json!({"reasoning": {"effort": "none"}})),
            Some(false)
        );
        assert_eq!(
            codex_chat_reasoning_requested(&json!({"reasoning": null})),
            Some(false)
        );
        assert_eq!(codex_chat_reasoning_requested(&json!({})), None);
    }

    #[test]
    fn resolves_codex_upstream_model_preferring_settings_model() {
        assert_eq!(
            resolve_codex_provider_upstream_model(
                Some(" deepseek-v4-flash "),
                Some("kimi-k2")
            )
            .as_deref(),
            Some("deepseek-v4-flash")
        );
        assert_eq!(
            resolve_codex_provider_upstream_model(Some(" "), Some(" kimi-k2 ")).as_deref(),
            Some("kimi-k2")
        );
        assert!(resolve_codex_provider_upstream_model(None, Some(" ")).is_none());
    }

    #[test]
    fn extracts_codex_catalog_model_ids_from_settings() {
        let ids = codex_provider_catalog_model_ids_from_settings(&json!({
            "modelCatalog": {
                "models": [
                    { "model": "deepseek-v4-flash" },
                    { "model": "  " },
                    { "id": "not-used-for-codex-chat-selection" },
                    { "model": "kimi-k2" }
                ]
            }
        }));

        assert_eq!(
            ids,
            HashSet::from(["deepseek-v4-flash".to_string(), "kimi-k2".to_string()])
        );
    }

    #[test]
    fn applies_codex_chat_upstream_model_when_required() {
        let mut body = json!({"model": "client-placeholder", "input": "ping"});
        let selected = apply_codex_chat_upstream_model_policy(
            &mut body,
            true,
            Some("deepseek-v4-flash"),
            &HashSet::new(),
        );

        assert_eq!(selected.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(body["model"], "deepseek-v4-flash");
    }

    #[test]
    fn keeps_codex_catalog_model_selection_for_chat_provider() {
        let mut body = json!({"model": "kimi-k2", "input": "ping"});
        let selected = apply_codex_chat_upstream_model_policy(
            &mut body,
            true,
            Some("deepseek-v4-flash"),
            &HashSet::from(["kimi-k2".to_string()]),
        );

        assert_eq!(selected.as_deref(), Some("kimi-k2"));
        assert_eq!(body["model"], "kimi-k2");
    }

    #[test]
    fn skips_codex_upstream_model_policy_for_responses_provider() {
        let mut body = json!({"model": "client-placeholder", "input": "ping"});
        let selected = apply_codex_chat_upstream_model_policy(
            &mut body,
            false,
            Some("deepseek-v4-flash"),
            &HashSet::new(),
        );

        assert!(selected.is_none());
        assert_eq!(body["model"], "client-placeholder");
    }

    #[test]
    fn channel_route_model_override_rewrites_public_model() {
        let mut body = json!({"model": "sonnet-public"});
        let selected = apply_channel_route_model_override(
            &mut body,
            Some("sonnet-public"),
            Some("upstream-sonnet"),
        );

        assert_eq!(selected.as_deref(), Some("upstream-sonnet"));
        assert_eq!(body["model"], "upstream-sonnet");
    }

    #[test]
    fn channel_route_model_override_skips_unmatched_or_already_upstream_model() {
        let mut unrelated = json!({"model": "other-model"});
        let selected = apply_channel_route_model_override(
            &mut unrelated,
            Some("sonnet-public"),
            Some("upstream-sonnet"),
        );
        assert!(selected.is_none());
        assert_eq!(unrelated["model"], "other-model");

        let mut already_upstream = json!({"model": "upstream-sonnet"});
        let selected = apply_channel_route_model_override(
            &mut already_upstream,
            Some("sonnet-public"),
            Some("upstream-sonnet"),
        );
        assert!(selected.is_none());
        assert_eq!(already_upstream["model"], "upstream-sonnet");
    }

    #[test]
    fn resolved_channel_model_override_returns_log_context() {
        let channel = ResolvedChannelAttempt {
            channel_id: "ch_1".to_string(),
            channel_name: "Relay".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_responses".to_string(),
            auth_profile_ref: None,
            public_model: Some("sonnet-public".to_string()),
            upstream_model: Some("upstream-sonnet".to_string()),
            header_overrides: json!({}),
            param_overrides: json!({}),
            status_code_mapping: json!([]),
            request_overrides: json!({}),
            retry_policy: json!({}),
        };
        let mut body = json!({"model": "sonnet-public"});

        let override_result =
            apply_resolved_channel_model_override(&mut body, &channel).expect("override result");

        assert_eq!(body["model"], "upstream-sonnet");
        assert_eq!(override_result.channel_id, "ch_1");
        assert_eq!(override_result.previous_model, "sonnet-public");
        assert_eq!(override_result.upstream_model, "upstream-sonnet");

        let mut unrelated = json!({"model": "other-model"});
        assert!(apply_resolved_channel_model_override(&mut unrelated, &channel).is_none());
        assert_eq!(unrelated["model"], "other-model");
    }

    #[test]
    fn resolved_channel_request_overrides_shallow_merge_body_object() {
        let channel = ResolvedChannelAttempt {
            channel_id: "ch_1".to_string(),
            channel_name: "Relay".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_responses".to_string(),
            auth_profile_ref: None,
            public_model: Some("sonnet-public".to_string()),
            upstream_model: Some("upstream-sonnet".to_string()),
            header_overrides: json!({}),
            param_overrides: json!({}),
            status_code_mapping: json!([]),
            request_overrides: json!({
                "temperature": 0.2,
                "metadata": { "route": "relay" }
            }),
            retry_policy: json!({}),
        };
        let mut body = json!({
            "model": "upstream-sonnet",
            "temperature": 0.9,
            "metadata": { "client": "desktop" },
            "stream": true
        });

        let application = apply_resolved_channel_request_overrides(&mut body, &channel)
            .expect("request override application");

        assert_eq!(body["temperature"], json!(0.2));
        assert_eq!(body["metadata"], json!({ "route": "relay" }));
        assert_eq!(body["stream"], json!(true));
        assert_eq!(application.channel_id, "ch_1");
        assert_eq!(application.applied_keys, vec!["metadata", "temperature"]);
    }

    #[test]
    fn resolved_channel_request_overrides_skip_empty_or_non_object_inputs() {
        let mut channel = ResolvedChannelAttempt {
            channel_id: "ch_1".to_string(),
            channel_name: "Relay".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_responses".to_string(),
            auth_profile_ref: None,
            public_model: None,
            upstream_model: None,
            header_overrides: json!({}),
            param_overrides: json!({}),
            status_code_mapping: json!([]),
            request_overrides: json!({}),
            retry_policy: json!({}),
        };

        let mut body = json!({"model": "upstream-sonnet"});
        assert!(apply_resolved_channel_request_overrides(&mut body, &channel).is_none());
        assert_eq!(body, json!({"model": "upstream-sonnet"}));

        channel.request_overrides = json!(["temperature"]);
        assert!(apply_resolved_channel_request_overrides(&mut body, &channel).is_none());
        assert_eq!(body, json!({"model": "upstream-sonnet"}));

        channel.request_overrides = json!({"temperature": 0.2});
        let mut scalar_body = json!("not-json-object");
        assert!(
            apply_resolved_channel_request_overrides(&mut scalar_body, &channel).is_none()
        );
        assert_eq!(scalar_body, json!("not-json-object"));
    }

    #[test]
    fn maps_codex_chat_reasoning_effort_for_provider_modes() {
        assert_eq!(
            map_codex_chat_reasoning_effort("xhigh", Some("deepseek")),
            Some("max")
        );
        assert_eq!(
            map_codex_chat_reasoning_effort("medium", Some("deepseek")),
            Some("high")
        );
        assert_eq!(
            map_codex_chat_reasoning_effort("minimal", Some("low_high")),
            Some("low")
        );
        assert_eq!(
            map_codex_chat_reasoning_effort("max", Some("openrouter")),
            Some("xhigh")
        );
        assert_eq!(
            map_codex_chat_reasoning_effort("turbo", Some("openrouter")),
            None
        );
        assert_eq!(
            map_codex_chat_reasoning_effort("none", Some("deepseek")),
            None
        );
        assert_eq!(map_codex_chat_reasoning_effort("max", None), Some("max"));
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

    #[test]
    fn maps_anthropic_tool_choice_to_openai_responses_shape() {
        assert_eq!(
            map_anthropic_tool_choice_to_openai_responses(&json!("any")),
            json!("any")
        );
        assert_eq!(
            map_anthropic_tool_choice_to_openai_responses(&json!({"type": "any"})),
            json!("required")
        );
        assert_eq!(
            map_anthropic_tool_choice_to_openai_responses(&json!({"type": "auto"})),
            json!("auto")
        );
        assert_eq!(
            map_anthropic_tool_choice_to_openai_responses(
                &json!({"type": "tool", "name": "search"})
            ),
            json!({"type": "function", "name": "search"})
        );
    }

    #[test]
    fn injects_openai_stream_include_usage_only_for_streaming_requests() {
        let mut non_streaming = json!({"stream": false});
        inject_openai_stream_include_usage(&mut non_streaming);
        assert!(non_streaming.get("stream_options").is_none());

        let mut streaming = json!({"stream": true});
        inject_openai_stream_include_usage(&mut streaming);
        assert_eq!(streaming["stream_options"]["include_usage"], true);
    }

    #[test]
    fn injects_openai_stream_include_usage_preserves_existing_options() {
        let mut body = json!({
            "stream": true,
            "stream_options": {
                "continuous_usage_stats": true,
                "include_usage": false
            }
        });

        inject_openai_stream_include_usage(&mut body);

        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["stream_options"]["continuous_usage_stats"], true);
    }

    #[test]
    fn cleans_openai_tool_schema_uri_formats_recursively() {
        let cleaned = clean_openai_tool_schema(json!({
            "type": "object",
            "format": "uri",
            "properties": {
                "url": {"type": "string", "format": "uri"},
                "date": {"type": "string", "format": "date-time"},
                "nested": {
                    "type": "array",
                    "items": {"type": "string", "format": "uri"}
                }
            }
        }));

        assert!(cleaned.get("format").is_none());
        assert!(cleaned["properties"]["url"].get("format").is_none());
        assert!(cleaned["properties"]["nested"]["items"]
            .get("format")
            .is_none());
        assert_eq!(cleaned["properties"]["date"]["format"], "date-time");
    }
}
