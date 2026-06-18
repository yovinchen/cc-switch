use crate::UpstreamSseAggregationKind;
use serde_json::{json, Value};

pub const CLAUDE_API_FORMAT_METADATA_KEY: &str = "claudeApiFormat";
const THINK_OPEN_TAG: &str = "<think>";
const THINK_CLOSE_TAG: &str = "</think>";

pub fn claude_api_format_from_metadata(metadata: &Value, fallback: &str) -> String {
    metadata
        .get(CLAUDE_API_FORMAT_METADATA_KEY)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

pub fn claude_api_format_needs_transform(api_format: &str) -> bool {
    matches!(
        api_format,
        "openai_chat" | "openai_responses" | "gemini_native"
    )
}

pub fn resolve_claude_api_format(
    provider_type: Option<&str>,
    meta_api_format: Option<&str>,
    settings_api_format: Option<&str>,
    openrouter_compat_mode: Option<&Value>,
) -> &'static str {
    if provider_type == Some("codex_oauth") {
        return "openai_responses";
    }

    if let Some(api_format) = meta_api_format {
        return normalize_configured_claude_api_format(api_format);
    }

    if let Some(api_format) = settings_api_format {
        return normalize_configured_claude_api_format(api_format);
    }

    if openrouter_compat_mode_enabled(openrouter_compat_mode) {
        "openai_chat"
    } else {
        "anthropic"
    }
}

fn normalize_configured_claude_api_format(api_format: &str) -> &'static str {
    match api_format {
        "openai_chat" => "openai_chat",
        "openai_responses" => "openai_responses",
        "gemini_native" => "gemini_native",
        _ => "anthropic",
    }
}

fn openrouter_compat_mode_enabled(raw: Option<&Value>) -> bool {
    match raw {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(number)) => number.as_i64().unwrap_or(0) != 0,
        Some(Value::String(value)) => {
            let normalized = value.trim().to_lowercase();
            normalized == "true" || normalized == "1"
        }
        _ => false,
    }
}

pub fn should_aggregate_codex_oauth_responses_sse(
    requested_streaming: bool,
    api_format: &str,
    is_codex_oauth: bool,
) -> bool {
    !requested_streaming && is_codex_oauth && api_format == "openai_responses"
}

pub fn should_use_claude_transform_streaming(
    requested_streaming: bool,
    upstream_is_sse: bool,
    api_format: &str,
    is_codex_oauth: bool,
) -> bool {
    requested_streaming || upstream_is_sse || (is_codex_oauth && api_format == "openai_responses")
}

pub fn claude_transform_unlabeled_sse_aggregation(
    api_format: &str,
) -> Option<UpstreamSseAggregationKind> {
    match api_format {
        "gemini_native" => None,
        "openai_responses" => Some(UpstreamSseAggregationKind::Responses),
        _ => Some(UpstreamSseAggregationKind::ChatCompletions),
    }
}

pub fn map_openai_responses_stop_reason_to_anthropic(
    status: Option<&str>,
    has_tool_use: bool,
    incomplete_reason: Option<&str>,
) -> Option<&'static str> {
    status.map(|value| match value {
        "completed" if has_tool_use => "tool_use",
        "incomplete"
            if matches!(
                incomplete_reason,
                Some("max_output_tokens") | Some("max_tokens")
            ) || incomplete_reason.is_none() =>
        {
            "max_tokens"
        }
        "incomplete" => "end_turn",
        _ => "end_turn",
    })
}

pub fn map_openai_chat_finish_reason_to_anthropic(
    finish_reason: Option<&str>,
    has_tool_use: bool,
) -> Option<&'static str> {
    finish_reason
        .map(|value| match value {
            "stop" => "end_turn",
            "length" => "max_tokens",
            "tool_calls" | "function_call" => "tool_use",
            "content_filter" => "end_turn",
            _ => "end_turn",
        })
        .or(if has_tool_use { Some("tool_use") } else { None })
}

pub fn map_gemini_finish_reason_to_anthropic(
    finish_reason: Option<&str>,
    has_tool_use: bool,
    blocked: bool,
) -> &'static str {
    if blocked {
        return "refusal";
    }

    match finish_reason {
        Some("MAX_TOKENS") => "max_tokens",
        Some("SAFETY")
        | Some("RECITATION")
        | Some("SPII")
        | Some("BLOCKLIST")
        | Some("PROHIBITED_CONTENT") => "refusal",
        _ if has_tool_use => "tool_use",
        _ => "end_turn",
    }
}

pub fn build_anthropic_message_delta_event(
    stop_reason: Option<&str>,
    usage: Option<Value>,
) -> Value {
    let usage = usage
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({ "input_tokens": 0, "output_tokens": 0 }));

    json!({
        "type": "message_delta",
        "delta": {
            "stop_reason": stop_reason,
            "stop_sequence": null
        },
        "usage": usage
    })
}

/// Extract reasoning text from common Chat-compatible upstream response fields.
///
/// Priority: `reasoning_content` > string/object `reasoning` > `reasoning_details`.
pub fn extract_reasoning_field_text(value: &Value) -> Option<String> {
    for key in ["reasoning_content", "reasoning"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }

    if let Some(reasoning) = value.get("reasoning") {
        for key in ["content", "text", "summary"] {
            if let Some(text) = reasoning.get(key).and_then(Value::as_str) {
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
    }

    if let Some(details) = value.get("reasoning_details") {
        if let Some(text) = extract_reasoning_details_text(details) {
            return Some(text);
        }
    }

    None
}

fn extract_reasoning_details_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => (!text.is_empty()).then(|| text.to_string()),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(extract_reasoning_detail_part_text)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            (!text.is_empty()).then_some(text)
        }
        Value::Object(_) => extract_reasoning_detail_part_text(value),
        _ => None,
    }
}

fn extract_reasoning_detail_part_text(value: &Value) -> Option<String> {
    for key in ["text", "content", "summary"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }

    if let Some(parts) = value.get("parts").and_then(Value::as_array) {
        let text = parts
            .iter()
            .filter_map(extract_reasoning_detail_part_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        return (!text.is_empty()).then_some(text);
    }

    None
}

pub fn extract_reasoning_summary_text(value: &Value) -> Option<String> {
    for key in ["reasoning_content", "content", "text"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }

    let summary = value.get("summary")?;
    if let Some(text) = summary.as_str() {
        return (!text.is_empty()).then(|| text.to_string());
    }

    let parts = summary.as_array()?;
    let text = parts
        .iter()
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .or_else(|| part.get("content").and_then(Value::as_str))
                .or_else(|| part.as_str())
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    (!text.is_empty()).then_some(text)
}

pub fn codex_response_item_call_id(item: &Value) -> Option<String> {
    item.get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub fn is_empty_json_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(value) => value.trim().is_empty(),
        Value::Array(value) => value.is_empty(),
        Value::Object(value) => value.is_empty(),
        _ => false,
    }
}

pub fn split_leading_think_block(text: &str) -> Option<(String, String)> {
    let leading_ws_len = text.len() - text.trim_start().len();
    let after_ws = &text[leading_ws_len..];
    if !after_ws.starts_with(THINK_OPEN_TAG) {
        return None;
    }

    let body_start = leading_ws_len + THINK_OPEN_TAG.len();
    let close_relative = text[body_start..].find(THINK_CLOSE_TAG)?;
    let close_start = body_start + close_relative;
    let answer_start = close_start + THINK_CLOSE_TAG.len();

    Some((
        text[body_start..close_start].trim().to_string(),
        strip_think_answer_separator(&text[answer_start..]).to_string(),
    ))
}

pub fn strip_leading_think_open_tag(text: &str) -> Option<String> {
    let leading_ws_len = text.len() - text.trim_start().len();
    let after_ws = &text[leading_ws_len..];
    after_ws
        .strip_prefix(THINK_OPEN_TAG)
        .map(|value| value.trim().to_string())
}

fn strip_think_answer_separator(text: &str) -> &str {
    text.trim_start_matches(['\r', '\n', '\t', ' '])
}

pub fn sanitize_anthropic_tool_use_input(name: &str, input: Value) -> Value {
    if name != "Read" {
        return input;
    }

    match input {
        Value::Object(mut object) => {
            if matches!(object.get("pages"), Some(Value::String(value)) if value.is_empty()) {
                object.remove("pages");
            }
            Value::Object(object)
        }
        other => other,
    }
}

pub fn sanitize_anthropic_tool_use_input_json(name: &str, raw: &str) -> String {
    if name != "Read" || raw.is_empty() {
        return raw.to_string();
    }

    let Ok(input) = serde_json::from_str::<Value>(raw) else {
        return raw.to_string();
    };

    serde_json::to_string(&sanitize_anthropic_tool_use_input(name, input))
        .unwrap_or_else(|_| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn metadata_with_claude_api_format(value: &str) -> Value {
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            CLAUDE_API_FORMAT_METADATA_KEY.to_string(),
            Value::String(value.to_string()),
        );
        Value::Object(metadata)
    }

    #[test]
    fn claude_api_format_from_metadata_uses_non_empty_metadata_value() {
        let metadata = metadata_with_claude_api_format("openai_responses");

        assert_eq!(
            claude_api_format_from_metadata(&metadata, "openai_chat"),
            "openai_responses"
        );
    }

    #[test]
    fn claude_api_format_from_metadata_falls_back_for_missing_or_blank_values() {
        assert_eq!(
            claude_api_format_from_metadata(&json!({}), "openai_chat"),
            "openai_chat"
        );
        assert_eq!(
            claude_api_format_from_metadata(
                &metadata_with_claude_api_format(" "),
                "openai_chat"
            ),
            "openai_chat"
        );
    }

    #[test]
    fn claude_api_format_needs_transform_only_for_translated_formats() {
        assert!(!claude_api_format_needs_transform("anthropic"));
        assert!(claude_api_format_needs_transform("openai_chat"));
        assert!(claude_api_format_needs_transform("openai_responses"));
        assert!(claude_api_format_needs_transform("gemini_native"));
        assert!(!claude_api_format_needs_transform("unknown"));
    }

    #[test]
    fn resolve_claude_api_format_uses_codex_oauth_first() {
        assert_eq!(
            resolve_claude_api_format(
                Some("codex_oauth"),
                Some("anthropic"),
                Some("openai_chat"),
                None,
            ),
            "openai_responses"
        );
    }

    #[test]
    fn resolve_claude_api_format_prefers_meta_then_settings_then_openrouter() {
        assert_eq!(
            resolve_claude_api_format(
                None,
                Some("gemini_native"),
                Some("openai_chat"),
                Some(&json!(true)),
            ),
            "gemini_native"
        );
        assert_eq!(
            resolve_claude_api_format(None, Some("unknown"), Some("openai_chat"), None),
            "anthropic"
        );
        assert_eq!(
            resolve_claude_api_format(None, None, Some("unknown"), Some(&json!("1"))),
            "anthropic"
        );
        assert_eq!(
            resolve_claude_api_format(None, None, None, Some(&json!("1"))),
            "openai_chat"
        );
        assert_eq!(resolve_claude_api_format(None, None, None, None), "anthropic");
    }

    #[test]
    fn claude_transform_streaming_uses_requested_streaming() {
        assert!(should_use_claude_transform_streaming(
            true,
            false,
            "openai_chat",
            false,
        ));
    }

    #[test]
    fn claude_transform_streaming_uses_upstream_sse() {
        assert!(should_use_claude_transform_streaming(
            false,
            true,
            "openai_chat",
            false,
        ));
    }

    #[test]
    fn codex_oauth_responses_force_streaming_by_default() {
        assert!(should_use_claude_transform_streaming(
            false,
            false,
            "openai_responses",
            true,
        ));
    }

    #[test]
    fn regular_openai_responses_can_stay_non_streaming() {
        assert!(!should_use_claude_transform_streaming(
            false,
            false,
            "openai_responses",
            false,
        ));
    }

    #[test]
    fn codex_oauth_responses_aggregates_only_for_non_streaming_requests() {
        assert!(should_aggregate_codex_oauth_responses_sse(
            false,
            "openai_responses",
            true,
        ));
        assert!(!should_aggregate_codex_oauth_responses_sse(
            true,
            "openai_responses",
            true,
        ));
        assert!(!should_aggregate_codex_oauth_responses_sse(
            false,
            "openai_chat",
            true,
        ));
        assert!(!should_aggregate_codex_oauth_responses_sse(
            false,
            "openai_responses",
            false,
        ));
    }

    #[test]
    fn claude_transform_unlabeled_sse_aggregation_matches_api_format() {
        assert_eq!(
            claude_transform_unlabeled_sse_aggregation("openai_responses"),
            Some(UpstreamSseAggregationKind::Responses)
        );
        assert_eq!(
            claude_transform_unlabeled_sse_aggregation("openai_chat"),
            Some(UpstreamSseAggregationKind::ChatCompletions)
        );
        assert_eq!(
            claude_transform_unlabeled_sse_aggregation("gemini_native"),
            None
        );
    }

    #[test]
    fn maps_openai_responses_status_to_anthropic_stop_reason() {
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(Some("completed"), true, None),
            Some("tool_use")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(
                Some("incomplete"),
                false,
                Some("max_output_tokens")
            ),
            Some("max_tokens")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(
                Some("incomplete"),
                false,
                Some("max_tokens")
            ),
            Some("max_tokens")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(Some("incomplete"), false, None),
            Some("max_tokens")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(
                Some("incomplete"),
                false,
                Some("content_filter")
            ),
            Some("end_turn")
        );
        assert_eq!(
            map_openai_responses_stop_reason_to_anthropic(None, true, Some("max_tokens")),
            None
        );
    }

    #[test]
    fn maps_openai_chat_finish_reason_to_anthropic_stop_reason() {
        assert_eq!(
            map_openai_chat_finish_reason_to_anthropic(Some("stop"), false),
            Some("end_turn")
        );
        assert_eq!(
            map_openai_chat_finish_reason_to_anthropic(Some("length"), false),
            Some("max_tokens")
        );
        assert_eq!(
            map_openai_chat_finish_reason_to_anthropic(Some("tool_calls"), false),
            Some("tool_use")
        );
        assert_eq!(
            map_openai_chat_finish_reason_to_anthropic(Some("function_call"), false),
            Some("tool_use")
        );
        assert_eq!(
            map_openai_chat_finish_reason_to_anthropic(Some("content_filter"), false),
            Some("end_turn")
        );
        assert_eq!(
            map_openai_chat_finish_reason_to_anthropic(Some("unknown"), false),
            Some("end_turn")
        );
        assert_eq!(
            map_openai_chat_finish_reason_to_anthropic(None, true),
            Some("tool_use")
        );
        assert_eq!(
            map_openai_chat_finish_reason_to_anthropic(None, false),
            None
        );
    }

    #[test]
    fn maps_gemini_finish_reason_to_anthropic_stop_reason() {
        assert_eq!(
            map_gemini_finish_reason_to_anthropic(Some("MAX_TOKENS"), false, false),
            "max_tokens"
        );
        assert_eq!(
            map_gemini_finish_reason_to_anthropic(Some("SAFETY"), false, false),
            "refusal"
        );
        assert_eq!(
            map_gemini_finish_reason_to_anthropic(Some("RECITATION"), false, false),
            "refusal"
        );
        assert_eq!(
            map_gemini_finish_reason_to_anthropic(Some("STOP"), true, false),
            "tool_use"
        );
        assert_eq!(
            map_gemini_finish_reason_to_anthropic(Some("STOP"), false, false),
            "end_turn"
        );
        assert_eq!(
            map_gemini_finish_reason_to_anthropic(Some("UNKNOWN"), false, false),
            "end_turn"
        );
        assert_eq!(
            map_gemini_finish_reason_to_anthropic(None, false, true),
            "refusal"
        );
    }

    #[test]
    fn builds_anthropic_message_delta_event_with_usage_fallback() {
        let event =
            build_anthropic_message_delta_event(Some("end_turn"), Some(json!({"input_tokens": 7})));
        assert_eq!(event["type"], "message_delta");
        assert_eq!(event["delta"]["stop_reason"], "end_turn");
        assert!(event["delta"]["stop_sequence"].is_null());
        assert_eq!(event["usage"]["input_tokens"], 7);

        let fallback = build_anthropic_message_delta_event(None, None);
        assert!(fallback["delta"]["stop_reason"].is_null());
        assert_eq!(fallback["usage"]["input_tokens"], 0);
        assert_eq!(fallback["usage"]["output_tokens"], 0);

        let non_object = build_anthropic_message_delta_event(Some("tool_use"), Some(json!(42)));
        assert_eq!(non_object["usage"]["input_tokens"], 0);
        assert_eq!(non_object["usage"]["output_tokens"], 0);
    }

    #[test]
    fn extracts_reasoning_field_text_from_common_shapes() {
        assert_eq!(
            extract_reasoning_field_text(&json!({"reasoning_content": "think"})).as_deref(),
            Some("think")
        );
        assert_eq!(
            extract_reasoning_field_text(&json!({"reasoning": {"summary": "nested"}})).as_deref(),
            Some("nested")
        );
        assert_eq!(
            extract_reasoning_field_text(&json!({
                "reasoning_details": [
                    {"text": "first"},
                    {"parts": [{"content": "second"}]}
                ]
            }))
            .as_deref(),
            Some("first\n\nsecond")
        );
        assert_eq!(extract_reasoning_field_text(&json!({"reasoning": ""})), None);
    }

    #[test]
    fn extracts_reasoning_summary_text_from_response_items() {
        assert_eq!(
            extract_reasoning_summary_text(&json!({"summary": "direct"})).as_deref(),
            Some("direct")
        );
        assert_eq!(
            extract_reasoning_summary_text(&json!({
                "summary": [
                    {"text": "first"},
                    {"content": "second"},
                    "third"
                ]
            }))
            .as_deref(),
            Some("first\n\nsecond\n\nthird")
        );
        assert_eq!(
            extract_reasoning_summary_text(&json!({"reasoning_content": "compat"})).as_deref(),
            Some("compat")
        );
    }

    #[test]
    fn extracts_codex_response_item_call_ids() {
        assert_eq!(
            codex_response_item_call_id(&json!({"call_id": " call_1 ", "id": "fallback"}))
                .as_deref(),
            Some("call_1")
        );
        assert_eq!(
            codex_response_item_call_id(&json!({"id": " item_1 "})).as_deref(),
            Some("item_1")
        );
        assert_eq!(codex_response_item_call_id(&json!({"call_id": "  "})), None);
    }

    #[test]
    fn detects_empty_json_values_for_history_merge() {
        assert!(is_empty_json_value(&Value::Null));
        assert!(is_empty_json_value(&json!("  ")));
        assert!(is_empty_json_value(&json!([])));
        assert!(is_empty_json_value(&json!({})));
        assert!(!is_empty_json_value(&json!(0)));
        assert!(!is_empty_json_value(&json!("text")));
    }

    #[test]
    fn splits_and_strips_leading_think_tags() {
        assert_eq!(
            split_leading_think_block(" \n<think> plan </think>\n\nanswer"),
            Some(("plan".to_string(), "answer".to_string()))
        );
        assert_eq!(split_leading_think_block("answer only"), None);
        assert_eq!(
            strip_leading_think_open_tag("\t<think>still thinking").as_deref(),
            Some("still thinking")
        );
        assert_eq!(strip_leading_think_open_tag("answer"), None);
    }

    #[test]
    fn sanitizes_read_tool_empty_pages_for_anthropic() {
        let input = sanitize_anthropic_tool_use_input(
            "Read",
            json!({
                "file_path": "/tmp/demo.py",
                "limit": 2000,
                "offset": 0,
                "pages": ""
            }),
        );

        assert_eq!(input["file_path"], "/tmp/demo.py");
        assert_eq!(input["limit"], 2000);
        assert_eq!(input["offset"], 0);
        assert!(input.get("pages").is_none());
    }

    #[test]
    fn read_tool_sanitizer_preserves_other_tools_and_non_empty_pages() {
        let other_tool = sanitize_anthropic_tool_use_input(
            "Search",
            json!({
                "query": "pages",
                "pages": ""
            }),
        );
        assert_eq!(other_tool["pages"], "");

        let non_empty_pages = sanitize_anthropic_tool_use_input(
            "Read",
            json!({
                "file_path": "/tmp/demo.py",
                "pages": "1-2"
            }),
        );
        assert_eq!(non_empty_pages["pages"], "1-2");
    }

    #[test]
    fn sanitizes_read_tool_json_without_rewriting_invalid_input() {
        assert_eq!(
            sanitize_anthropic_tool_use_input_json(
                "Read",
                r#"{"file_path":"/tmp/demo.py","pages":""}"#
            ),
            r#"{"file_path":"/tmp/demo.py"}"#
        );
        assert_eq!(
            sanitize_anthropic_tool_use_input_json("Read", "{not-json"),
            "{not-json"
        );
        assert_eq!(
            sanitize_anthropic_tool_use_input_json("Search", r#"{"pages":""}"#),
            r#"{"pages":""}"#
        );
    }
}
