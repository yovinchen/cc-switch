use crate::{
    json_canonical::{
        canonical_json_string, canonicalize_json_string_if_parseable, canonicalize_tool_arguments,
        short_sha256_hex,
    },
    request_body::{
        codex_chat_reasoning_requested, inject_openai_stream_include_usage,
        map_codex_chat_reasoning_effort,
    },
    UpstreamSseAggregationKind,
};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

pub const CLAUDE_API_FORMAT_METADATA_KEY: &str = "claudeApiFormat";
pub const CODEX_TOOL_SEARCH_PROXY_NAME: &str = "tool_search";
const THINK_OPEN_TAG: &str = "<think>";
const THINK_CLOSE_TAG: &str = "</think>";
const CUSTOM_TOOL_INPUT_FIELD: &str = "input";
const CUSTOM_TOOL_INPUT_DESCRIPTION: &str = "Raw string input for the original custom tool. Preserve formatting exactly and follow the original tool definition embedded in the description.";
const CUSTOM_TOOL_PRESERVED_METADATA_HEADING: &str = "Original tool definition:";
const CHAT_TOOL_NAME_MAX_LEN: usize = 64;
const EXTRA_CHAT_PASSTHROUGH_FIELDS: &[&str] = &[
    "frequency_penalty",
    "logit_bias",
    "logprobs",
    "metadata",
    "n",
    "parallel_tool_calls",
    "presence_penalty",
    "response_format",
    "seed",
    "service_tier",
    "stop",
    "stream_options",
    "top_logprobs",
    "user",
];

/// Provider-neutral Codex Responses -> Chat Completions reasoning capability hints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexChatReasoningOptions {
    pub supports_thinking: Option<bool>,
    pub supports_effort: Option<bool>,
    pub thinking_param: Option<String>,
    pub effort_param: Option<String>,
    pub effort_value_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexToolKind {
    Function,
    Namespace,
    Custom,
    ToolSearch,
}

#[derive(Debug, Clone)]
pub struct CodexToolSpec {
    pub kind: CodexToolKind,
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CodexToolContext {
    chat_tools: Vec<Value>,
    seen_chat_names: HashSet<String>,
    chat_name_to_spec: HashMap<String, CodexToolSpec>,
    namespace_name_to_chat_name: HashMap<(String, String), String>,
}

impl CodexToolContext {
    pub fn chat_tools(&self) -> &[Value] {
        &self.chat_tools
    }

    pub fn lookup_chat_name(&self, chat_name: &str) -> Option<&CodexToolSpec> {
        self.chat_name_to_spec.get(chat_name)
    }

    pub fn is_custom_tool_chat_name(&self, chat_name: &str) -> bool {
        self.lookup_chat_name(chat_name)
            .is_some_and(|spec| matches!(&spec.kind, CodexToolKind::Custom))
    }

    pub fn chat_name_for_response_function(&self, name: &str, namespace: Option<&str>) -> String {
        if let Some(namespace) = namespace.filter(|value| !value.is_empty()) {
            if let Some(chat_name) = self
                .namespace_name_to_chat_name
                .get(&(namespace.to_string(), name.to_string()))
            {
                return chat_name.clone();
            }
            return flatten_namespace_tool_name(namespace, name);
        }

        name.to_string()
    }

    fn add_chat_tool(&mut self, chat_name: String, spec: CodexToolSpec, chat_tool: Value) {
        if chat_name.trim().is_empty() || self.seen_chat_names.contains(&chat_name) {
            return;
        }
        self.seen_chat_names.insert(chat_name.clone());
        if let Some(namespace) = spec.namespace.as_ref() {
            self.namespace_name_to_chat_name
                .insert((namespace.clone(), spec.name.clone()), chat_name.clone());
        }
        self.chat_name_to_spec.insert(chat_name, spec);
        self.chat_tools.push(chat_tool);
    }

    fn add_function_tool(&mut self, tool: &Value, namespace: Option<&str>) {
        let Some(original_name) = responses_tool_name(tool) else {
            return;
        };
        let chat_name = namespace
            .map(|namespace| flatten_namespace_tool_name(namespace, &original_name))
            .unwrap_or_else(|| original_name.clone());

        let Some(chat_tool) = responses_function_tool_to_chat_tool(tool, &chat_name) else {
            return;
        };
        let spec = CodexToolSpec {
            kind: if namespace.is_some() {
                CodexToolKind::Namespace
            } else {
                CodexToolKind::Function
            },
            name: original_name,
            namespace: namespace.map(ToString::to_string),
        };
        self.add_chat_tool(chat_name, spec, chat_tool);
    }

    fn add_custom_tool(&mut self, tool: &Value) {
        let Some(name) = responses_tool_name(tool) else {
            return;
        };
        let chat_tool = responses_custom_tool_to_chat_tool(&name, tool);
        let spec = CodexToolSpec {
            kind: CodexToolKind::Custom,
            name: name.clone(),
            namespace: None,
        };
        self.add_chat_tool(name, spec, chat_tool);
    }

    fn add_tool_search_tool(&mut self) {
        let chat_tool = json!({
            "type": "function",
            "function": {
                "name": CODEX_TOOL_SEARCH_PROXY_NAME,
                "description": "Search and load Codex tools, plugins, connectors, and MCP namespaces for the current task.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query for tools or connectors to load."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of tool groups to return."
                        }
                    },
                    "required": ["query"]
                }
            }
        });
        let spec = CodexToolSpec {
            kind: CodexToolKind::ToolSearch,
            name: CODEX_TOOL_SEARCH_PROXY_NAME.to_string(),
            namespace: None,
        };
        self.add_chat_tool(CODEX_TOOL_SEARCH_PROXY_NAME.to_string(), spec, chat_tool);
    }

    fn add_namespace_tool(&mut self, namespace_tool: &Value) {
        let Some(namespace) = namespace_tool.get("name").and_then(Value::as_str) else {
            return;
        };
        let Some(children) = namespace_tool
            .get("tools")
            .or_else(|| namespace_tool.get("children"))
            .and_then(Value::as_array)
        else {
            return;
        };

        for child in children {
            if child.get("type").and_then(Value::as_str) == Some("function") {
                self.add_function_tool(child, Some(namespace));
            }
        }
    }

    fn add_response_tool(&mut self, tool: &Value) {
        match tool {
            Value::String(name) => {
                self.add_custom_tool(&json!({
                    "type": "custom",
                    "name": name
                }));
            }
            Value::Object(_) => match tool.get("type").and_then(Value::as_str) {
                Some("function") => self.add_function_tool(tool, None),
                Some("custom") => self.add_custom_tool(tool),
                Some("tool_search") => self.add_tool_search_tool(),
                Some("namespace") => self.add_namespace_tool(tool),
                _ => {}
            },
            _ => {}
        }
    }
}

pub fn build_codex_tool_context_from_request(body: &Value) -> CodexToolContext {
    let mut context = CodexToolContext::default();

    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        for tool in tools {
            context.add_response_tool(tool);
        }
    }

    if let Some(input) = body.get("input") {
        collect_tool_search_output_tools(input, &mut context);
    }

    context
}

pub fn apply_codex_chat_reasoning_options(
    result: &mut Value,
    body: &Value,
    config: Option<&CodexChatReasoningOptions>,
    supports_native_reasoning_effort: bool,
) {
    let Some(config) = config else {
        if supports_native_reasoning_effort {
            if let Some(effort) = body.pointer("/reasoning/effort") {
                result["reasoning_effort"] = effort.clone();
            }
        }
        return;
    };

    let supports_effort = config.supports_effort.unwrap_or(false);
    let supports_thinking = config.supports_thinking.unwrap_or(false) || supports_effort;
    let Some(reasoning_enabled) = codex_chat_reasoning_requested(body) else {
        return;
    };

    if supports_thinking {
        match config
            .thinking_param
            .as_deref()
            .unwrap_or("thinking")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "thinking" => {
                result["thinking"] = json!({
                    "type": if reasoning_enabled { "enabled" } else { "disabled" }
                });
            }
            "enable_thinking" => {
                result["enable_thinking"] = json!(reasoning_enabled);
            }
            "reasoning_split" => {
                result["reasoning_split"] = json!(reasoning_enabled);
            }
            _ => {}
        }
    }

    let effort_param = config
        .effort_param
        .as_deref()
        .unwrap_or("reasoning_effort")
        .trim()
        .to_ascii_lowercase();

    if !reasoning_enabled {
        if effort_param == "reasoning.effort" {
            result["reasoning"] = json!({ "effort": "none" });
        }
        return;
    }

    if !supports_effort {
        return;
    }

    let Some(effort) = body.pointer("/reasoning/effort").and_then(Value::as_str) else {
        return;
    };
    let Some(mapped) = map_codex_chat_reasoning_effort(effort, config.effort_value_mode.as_deref())
    else {
        return;
    };

    match effort_param.as_str() {
        "reasoning_effort" => {
            result["reasoning_effort"] = json!(mapped);
        }
        "reasoning.effort" => {
            result["reasoning"] = json!({ "effort": mapped });
        }
        _ => {}
    }
}

pub fn responses_to_chat_completions_with_options(
    body: &Value,
    reasoning_config: Option<&CodexChatReasoningOptions>,
    is_openai_o_series_model: bool,
    supports_native_reasoning_effort: bool,
) -> Value {
    let mut result = json!({});
    let tool_context = build_codex_tool_context_from_request(body);

    if let Some(model) = body.get("model") {
        result["model"] = model.clone();
    }

    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        let instructions = responses_instruction_text(instructions);
        if !instructions.is_empty() {
            messages.push(json!({
                "role": "system",
                "content": instructions
            }));
        }
    }

    if let Some(input) = body.get("input") {
        append_responses_input_as_chat_messages(input, &mut messages, &tool_context);
    }
    let messages = collapse_system_messages_to_head(messages);
    result["messages"] = json!(messages);

    if let Some(max_tokens) = body.get("max_output_tokens") {
        if is_openai_o_series_model {
            result["max_completion_tokens"] = max_tokens.clone();
        } else {
            result["max_tokens"] = max_tokens.clone();
        }
    }
    if let Some(max_tokens) = body.get("max_tokens") {
        result["max_tokens"] = max_tokens.clone();
    }
    if let Some(max_tokens) = body.get("max_completion_tokens") {
        result["max_completion_tokens"] = max_tokens.clone();
    }

    for key in ["temperature", "top_p", "stream"] {
        if let Some(value) = body.get(key) {
            result[key] = value.clone();
        }
    }

    apply_codex_chat_reasoning_options(
        &mut result,
        body,
        reasoning_config,
        supports_native_reasoning_effort,
    );

    let tools = tool_context.chat_tools();
    if !tools.is_empty() {
        result["tools"] = json!(tools);
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] =
            responses_tool_choice_to_chat_tool_choice(tool_choice, &tool_context);
    }

    for key in EXTRA_CHAT_PASSTHROUGH_FIELDS {
        if let Some(value) = body.get(*key) {
            result[*key] = value.clone();
        }
    }

    // Strict OpenAI-compatible upstreams (vLLM, enterprise gateways) reject
    // requests that carry tool_choice or parallel_tool_calls without a non-empty
    // tools array. Drop both fields when tools ended up absent or empty after
    // conversion to avoid 503/400 from such providers.
    let has_tools = result
        .get("tools")
        .is_some_and(|value| value.as_array().is_some_and(|array| !array.is_empty()));
    if !has_tools {
        if let Some(obj) = result.as_object_mut() {
            obj.remove("tool_choice");
            obj.remove("parallel_tool_calls");
        }
    }

    inject_openai_stream_include_usage(&mut result);
    result
}

pub fn append_responses_input_as_chat_messages(
    input: &Value,
    messages: &mut Vec<Value>,
    tool_context: &CodexToolContext,
) {
    let mut pending_tool_calls = Vec::new();
    let mut pending_reasoning: Option<String> = None;
    let mut last_assistant_index: Option<usize> = None;

    match input {
        Value::String(text) => {
            messages.push(json!({
                "role": "user",
                "content": text
            }));
        }
        Value::Array(items) => {
            for item in items {
                append_responses_item_as_chat_message(
                    item,
                    messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    &mut last_assistant_index,
                    tool_context,
                );
            }
        }
        Value::Object(_) => {
            append_responses_item_as_chat_message(
                input,
                messages,
                &mut pending_tool_calls,
                &mut pending_reasoning,
                &mut last_assistant_index,
                tool_context,
            );
        }
        _ => {}
    }

    flush_pending_tool_calls(
        messages,
        &mut pending_tool_calls,
        &mut pending_reasoning,
        &mut last_assistant_index,
    );
    backfill_tool_call_reasoning_placeholders(messages);
}

fn append_responses_item_as_chat_message(
    item: &Value,
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
    last_assistant_index: &mut Option<usize>,
    tool_context: &CodexToolContext,
) {
    let item_type = item.get("type").and_then(Value::as_str);
    match item_type {
        Some("function_call") => {
            append_unique_pending_reasoning(pending_reasoning, responses_item_reasoning_text(item));
            pending_tool_calls.push(responses_function_call_to_chat_tool_call_with_context(
                item,
                tool_context,
            ));
        }
        Some("custom_tool_call") => {
            append_unique_pending_reasoning(pending_reasoning, responses_item_reasoning_text(item));
            pending_tool_calls.push(responses_custom_tool_call_to_chat_tool_call(item));
        }
        Some("tool_search_call") => {
            append_unique_pending_reasoning(pending_reasoning, responses_item_reasoning_text(item));
            pending_tool_calls.push(responses_tool_search_call_to_chat_tool_call(item));
        }
        Some("function_call_output") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
            messages.push(responses_function_call_output_to_chat_tool_message(item));
        }
        Some("custom_tool_call_output") | Some("tool_search_output") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
            messages.push(responses_client_tool_output_to_chat_tool_message(item));
        }
        Some("reasoning") => {
            let reasoning = responses_reasoning_item_text(item);
            let attached_to_previous = pending_tool_calls.is_empty()
                && attach_reasoning_to_last_assistant(messages, *last_assistant_index, &reasoning);
            if !attached_to_previous {
                append_pending_reasoning(pending_reasoning, reasoning);
            }
        }
        Some("input_text" | "input_image" | "input_file" | "input_audio") => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
            let role = item
                .get("role")
                .and_then(Value::as_str)
                .map(responses_role_to_chat_role)
                .unwrap_or("user");
            let message = json!({
                "role": role,
                "content": responses_content_to_chat_content(role, &Value::Array(vec![item.clone()]))
            });
            if role == "assistant" {
                let mut message = message;
                attach_pending_reasoning_to_assistant(&mut message, pending_reasoning);
                update_last_assistant_index(messages, &message, last_assistant_index);
                messages.push(message);
                return;
            } else if pending_reasoning.is_some() {
                pending_reasoning.take();
            }
            update_last_assistant_index(messages, &message, last_assistant_index);
            messages.push(message);
        }
        Some("message") | None => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
            if item.get("role").is_some() || item.get("content").is_some() {
                let message = responses_message_item_to_chat_message(item, pending_reasoning);
                update_last_assistant_index(messages, &message, last_assistant_index);
                messages.push(message);
            }
        }
        _ => {
            flush_pending_tool_calls(
                messages,
                pending_tool_calls,
                pending_reasoning,
                last_assistant_index,
            );
            if item.get("role").is_some() || item.get("content").is_some() {
                let message = responses_message_item_to_chat_message(item, pending_reasoning);
                update_last_assistant_index(messages, &message, last_assistant_index);
                messages.push(message);
            }
        }
    }
}

fn flush_pending_tool_calls(
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
    last_assistant_index: &mut Option<usize>,
) {
    if pending_tool_calls.is_empty() {
        return;
    }

    let mut message = json!({
        "role": "assistant",
        "content": null,
        "tool_calls": std::mem::take(pending_tool_calls)
    });
    attach_pending_reasoning_to_assistant(&mut message, pending_reasoning);
    *last_assistant_index = Some(messages.len());
    messages.push(message);
}

fn responses_message_item_to_chat_message(
    item: &Value,
    pending_reasoning: &mut Option<String>,
) -> Value {
    let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
    let chat_role = responses_role_to_chat_role(role);
    let content = item
        .get("content")
        .map(|value| responses_content_to_chat_content(chat_role, value))
        .unwrap_or(Value::Null);

    let mut message = json!({
        "role": chat_role,
        "content": content
    });

    if chat_role == "assistant" {
        append_pending_reasoning(pending_reasoning, responses_message_reasoning_text(item));
        attach_pending_reasoning_to_assistant(&mut message, pending_reasoning);
    } else if pending_reasoning.is_some() {
        pending_reasoning.take();
    }

    message
}

fn update_last_assistant_index(
    messages: &[Value],
    message: &Value,
    last_assistant_index: &mut Option<usize>,
) {
    match message.get("role").and_then(Value::as_str) {
        Some("assistant") => {
            *last_assistant_index = Some(messages.len());
        }
        Some("tool") => {}
        _ => {
            *last_assistant_index = None;
        }
    }
}

fn responses_message_reasoning_text(item: &Value) -> Option<String> {
    responses_item_reasoning_text(item)
}

fn responses_item_reasoning_text(item: &Value) -> Option<String> {
    extract_reasoning_field_text(item)
}

fn responses_reasoning_item_text(item: &Value) -> Option<String> {
    extract_reasoning_summary_text(item)
}

fn collect_tool_search_output_tools(value: &Value, context: &mut CodexToolContext) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_tool_search_output_tools(item, context);
            }
        }
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str) == Some("tool_search_output") {
                if let Some(tools) = obj.get("tools").and_then(Value::as_array) {
                    for tool in tools {
                        context.add_response_tool(tool);
                    }
                }
            }
            for value in obj.values() {
                collect_tool_search_output_tools(value, context);
            }
        }
        _ => {}
    }
}

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

pub fn append_reasoning_content(message: &mut Map<String, Value>, reasoning: &str) -> bool {
    let reasoning = reasoning.trim();
    if reasoning.is_empty() {
        return false;
    }

    match message.get_mut("reasoning_content") {
        Some(Value::String(existing)) if !existing.is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(reasoning);
        }
        _ => {
            message.insert(
                "reasoning_content".to_string(),
                Value::String(reasoning.to_string()),
            );
        }
    }
    true
}

pub fn attach_reasoning_content_field(item: &mut Value, reasoning: &str) -> bool {
    let reasoning = reasoning.trim();
    if reasoning.is_empty() {
        return false;
    }

    if let Some(obj) = item.as_object_mut() {
        obj.insert(
            "reasoning_content".to_string(),
            Value::String(reasoning.to_string()),
        );
        return true;
    }

    false
}

pub fn attach_optional_reasoning_content_field(
    item: &mut Value,
    reasoning: Option<&str>,
) -> bool {
    let Some(reasoning) = reasoning else {
        return false;
    };
    attach_reasoning_content_field(item, reasoning)
}

pub fn response_function_call_item(
    item_id: &str,
    status: &str,
    call_id: &str,
    name: &str,
    arguments: &str,
    reasoning: Option<&str>,
) -> Value {
    let mut item = json!({
        "id": item_id,
        "type": "function_call",
        "status": status,
        "call_id": call_id,
        "name": name,
        "arguments": arguments
    });
    attach_optional_reasoning_content_field(&mut item, reasoning);
    item
}

pub fn response_function_call_item_with_namespace(
    item_id: &str,
    status: &str,
    call_id: &str,
    name: &str,
    namespace: Option<&str>,
    arguments: &str,
    reasoning: Option<&str>,
) -> Value {
    let mut item =
        response_function_call_item(item_id, status, call_id, name, arguments, reasoning);
    if let Some(namespace) = namespace.filter(|value| !value.is_empty()) {
        if let Some(obj) = item.as_object_mut() {
            obj.insert("namespace".to_string(), json!(namespace));
        }
    }
    item
}

pub fn custom_tool_input_from_chat_arguments(arguments: &str) -> String {
    if arguments.trim().is_empty() {
        return String::new();
    }
    match serde_json::from_str::<Value>(arguments) {
        Ok(Value::Object(obj)) => obj
            .get("input")
            .and_then(Value::as_str)
            .unwrap_or(arguments)
            .to_string(),
        _ => arguments.to_string(),
    }
}

pub fn response_tool_search_call_item(
    call_id: &str,
    status: &str,
    arguments: &str,
    reasoning: Option<&str>,
) -> Value {
    let parsed_arguments = parse_tool_arguments_object(arguments);
    let mut item = json!({
        "type": "tool_search_call",
        "call_id": call_id,
        "status": status,
        "execution": "client",
        "arguments": parsed_arguments
    });
    attach_optional_reasoning_content_field(&mut item, reasoning);
    item
}

pub fn response_custom_tool_call_item(
    item_id: &str,
    status: &str,
    call_id: &str,
    name: &str,
    arguments: &str,
    reasoning: Option<&str>,
) -> Value {
    let input = custom_tool_input_from_chat_arguments(arguments);
    let mut item = json!({
        "id": item_id,
        "type": "custom_tool_call",
        "status": status,
        "call_id": call_id,
        "name": name,
        "input": input
    });
    attach_optional_reasoning_content_field(&mut item, reasoning);
    item
}

pub fn response_tool_call_item_id(call_id: &str, is_custom_tool: bool) -> String {
    if is_custom_tool {
        format!("ctc_{call_id}")
    } else {
        format!("fc_{call_id}")
    }
}

pub fn response_tool_call_item_id_from_chat_name(
    call_id: &str,
    chat_name: &str,
    tool_context: &CodexToolContext,
) -> String {
    response_tool_call_item_id(call_id, tool_context.is_custom_tool_chat_name(chat_name))
}

pub fn response_tool_call_item_from_chat_name(
    item_id: &str,
    status: &str,
    call_id: &str,
    chat_name: &str,
    arguments: &str,
    reasoning: Option<&str>,
    tool_context: &CodexToolContext,
) -> Value {
    match tool_context.lookup_chat_name(chat_name) {
        Some(spec) if spec.kind == CodexToolKind::ToolSearch => {
            response_tool_search_call_item(call_id, status, arguments, reasoning)
        }
        Some(spec) if spec.kind == CodexToolKind::Custom => response_custom_tool_call_item(
            item_id, status, call_id, &spec.name, arguments, reasoning,
        ),
        Some(spec) => response_function_call_item_with_namespace(
            item_id,
            status,
            call_id,
            &spec.name,
            spec.namespace.as_deref(),
            arguments,
            reasoning,
        ),
        None => {
            response_function_call_item(item_id, status, call_id, chat_name, arguments, reasoning)
        }
    }
}

pub fn chat_completion_to_response_with_context(
    body: &Value,
    tool_context: &CodexToolContext,
) -> Result<Value, String> {
    let choices = body
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| "No choices in chat response".to_string())?;
    let choice = choices
        .first()
        .ok_or_else(|| "Empty choices in chat response".to_string())?;
    let message = choice
        .get("message")
        .ok_or_else(|| "No message in chat choice".to_string())?;

    let response_id = response_id_from_chat_id(body.get("id").and_then(Value::as_str));
    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    let created_at = body.get("created").and_then(Value::as_u64).unwrap_or(0);
    let finish_reason = choice.get("finish_reason").and_then(Value::as_str);

    let reasoning = chat_reasoning_text(message);
    let mut output = Vec::new();
    if let Some(reasoning_item) =
        chat_reasoning_to_response_output_item(reasoning.as_deref(), &response_id)
    {
        output.push(reasoning_item);
    }
    if let Some(message_item) = chat_message_to_response_output_item(message, &response_id) {
        output.push(message_item);
    }
    output.extend(chat_tool_calls_to_response_output_items(
        message,
        reasoning.as_deref(),
        tool_context,
    ));

    let mut response = json!({
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": response_status_from_finish_reason(finish_reason),
        "model": model,
        "output": output,
        "usage": chat_usage_to_responses_usage(body.get("usage"))
    });

    if finish_reason == Some("length") {
        response["incomplete_details"] = json!({ "reason": "max_output_tokens" });
    }

    Ok(response)
}

pub fn chat_tool_calls_to_response_output_items(
    message: &Value,
    reasoning: Option<&str>,
    tool_context: &CodexToolContext,
) -> Vec<Value> {
    let mut output = Vec::new();

    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, tool_call) in tool_calls.iter().enumerate() {
            output.push(chat_tool_call_to_response_item(
                tool_call,
                index,
                reasoning,
                tool_context,
            ));
        }
    } else if let Some(function_call) = message.get("function_call") {
        output.push(chat_legacy_function_call_to_response_item(
            function_call,
            reasoning,
            tool_context,
        ));
    }

    output
}

fn chat_tool_call_to_response_item(
    tool_call: &Value,
    index: usize,
    reasoning: Option<&str>,
    tool_context: &CodexToolContext,
) -> Value {
    let call_id = tool_call
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("call_{index}"));
    let function = tool_call.get("function").unwrap_or(&Value::Null);
    let name = function.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = canonicalize_tool_arguments(function.get("arguments"));

    let item_id = response_tool_call_item_id_from_chat_name(&call_id, name, tool_context);
    response_tool_call_item_from_chat_name(
        &item_id,
        "completed",
        &call_id,
        name,
        &arguments,
        reasoning,
        tool_context,
    )
}

fn chat_legacy_function_call_to_response_item(
    function_call: &Value,
    reasoning: Option<&str>,
    tool_context: &CodexToolContext,
) -> Value {
    let call_id = function_call
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("call_0");
    let name = function_call
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let arguments = canonicalize_tool_arguments(function_call.get("arguments"));

    let item_id = response_tool_call_item_id_from_chat_name(call_id, name, tool_context);
    response_tool_call_item_from_chat_name(
        &item_id,
        "completed",
        call_id,
        name,
        &arguments,
        reasoning,
        tool_context,
    )
}

fn parse_tool_arguments_object(arguments: &str) -> Value {
    if arguments.trim().is_empty() {
        return json!({});
    }
    serde_json::from_str::<Value>(arguments)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({ "query": arguments }))
}

pub fn flatten_namespace_tool_name(namespace: &str, name: &str) -> String {
    let full_name = format!("{namespace}__{name}");
    if full_name.len() <= CHAT_TOOL_NAME_MAX_LEN {
        return full_name;
    }

    let hash = short_sha256_hex(full_name.as_bytes());
    let suffix = format!("__{hash}");
    let prefix_len = CHAT_TOOL_NAME_MAX_LEN.saturating_sub(suffix.len());
    let mut prefix = String::new();
    for ch in full_name.chars() {
        if prefix.len() + ch.len_utf8() > prefix_len {
            break;
        }
        prefix.push(ch);
    }
    format!("{prefix}{suffix}")
}

pub fn responses_tool_name(tool: &Value) -> Option<String> {
    tool.get("function")
        .and_then(|function| function.get("name"))
        .or_else(|| tool.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub fn responses_custom_tool_to_chat_tool(name: &str, tool: &Value) -> Value {
    let description = json!(responses_custom_tool_description(tool));
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": {
                    CUSTOM_TOOL_INPUT_FIELD: {
                        "type": "string",
                        "description": CUSTOM_TOOL_INPUT_DESCRIPTION
                    }
                },
                "required": [CUSTOM_TOOL_INPUT_FIELD]
            }
        }
    })
}

fn responses_custom_tool_description(tool: &Value) -> String {
    let mut description = String::new();
    description.push_str(CUSTOM_TOOL_PRESERVED_METADATA_HEADING);
    description.push_str("\n```json\n");
    description.push_str(&serialize_tool_definition_for_description(tool));
    description.push_str("\n```");
    description
}

fn serialize_tool_definition_for_description(tool: &Value) -> String {
    canonical_json_string(tool)
}

pub fn responses_function_tool_to_chat_tool(tool: &Value, chat_name: &str) -> Option<Value> {
    if tool.get("type").and_then(Value::as_str) != Some("function") {
        return None;
    }

    if let Some(function) = tool.get("function") {
        let mut chat_tool = json!({
            "type": "function",
            "function": function.clone()
        });
        if let Some(obj) = chat_tool
            .get_mut("function")
            .and_then(Value::as_object_mut)
        {
            obj.insert("name".to_string(), json!(chat_name));
            if let Some(strict) = tool.get("strict").cloned() {
                obj.entry("strict".to_string()).or_insert(strict);
            }
        }
        return Some(chat_tool);
    }

    let mut function = json!({
        "name": chat_name,
        "description": tool.get("description").cloned().unwrap_or(Value::Null),
        "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({}))
    });
    if let Some(strict) = tool.get("strict") {
        function["strict"] = strict.clone();
    }

    Some(json!({
        "type": "function",
        "function": function
    }))
}

pub fn responses_function_call_to_chat_tool_call(item: &Value, chat_name: &str) -> Value {
    let call_id = item
        .get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let arguments = canonicalize_tool_arguments(item.get("arguments"));

    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": chat_name,
            "arguments": arguments
        }
    })
}

pub fn responses_function_call_to_chat_tool_call_with_context(
    item: &Value,
    tool_context: &CodexToolContext,
) -> Value {
    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
    let namespace = item.get("namespace").and_then(Value::as_str);
    let chat_name = tool_context.chat_name_for_response_function(name, namespace);
    responses_function_call_to_chat_tool_call(item, &chat_name)
}

pub fn responses_tool_choice_to_chat_function_selector(chat_name: &str) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": chat_name
        }
    })
}

pub fn responses_tool_choice_to_chat_tool_choice(
    tool_choice: &Value,
    tool_context: &CodexToolContext,
) -> Value {
    match tool_choice {
        Value::Object(obj) if obj.get("type").and_then(Value::as_str) == Some("function") => {
            let name = obj.get("name").and_then(Value::as_str).unwrap_or("");
            let namespace = obj.get("namespace").and_then(Value::as_str);
            let chat_name = tool_context.chat_name_for_response_function(name, namespace);
            responses_tool_choice_to_chat_function_selector(&chat_name)
        }
        Value::Object(obj) if obj.get("type").and_then(Value::as_str) == Some("tool_search") => {
            responses_tool_choice_to_chat_function_selector(CODEX_TOOL_SEARCH_PROXY_NAME)
        }
        Value::Object(obj) if obj.get("type").and_then(Value::as_str) == Some("custom") => {
            let name = obj.get("name").and_then(Value::as_str).unwrap_or("");
            responses_tool_choice_to_chat_function_selector(name)
        }
        _ => tool_choice.clone(),
    }
}

pub fn responses_function_call_output_to_chat_tool_message(item: &Value) -> Value {
    let call_id = item.get("call_id").and_then(Value::as_str).unwrap_or("");
    let output = match item.get("output") {
        Some(Value::String(s)) => canonicalize_json_string_if_parseable(s),
        Some(v) => canonical_json_string(v),
        None => String::new(),
    };

    json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": output
    })
}

pub fn responses_client_tool_output_to_chat_tool_message(item: &Value) -> Value {
    let call_id = item.get("call_id").and_then(Value::as_str).unwrap_or("");
    let output = canonical_json_string(item);

    json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": output
    })
}

pub fn chat_reasoning_text(message: &Value) -> Option<String> {
    if let Some(reasoning) = extract_reasoning_field_text(message) {
        return Some(reasoning);
    }

    if let Some(content) = message.get("content").and_then(Value::as_str) {
        if let Some((reasoning, _answer)) = split_leading_think_block(content) {
            if !reasoning.is_empty() {
                return Some(reasoning);
            }
        }
    }

    None
}

pub fn chat_reasoning_to_response_output_item(
    reasoning: Option<&str>,
    response_id: &str,
) -> Option<Value> {
    let reasoning = reasoning?;
    if reasoning.is_empty() {
        return None;
    }

    Some(json!({
        "id": format!("rs_{response_id}"),
        "type": "reasoning",
        "summary": [{
            "type": "summary_text",
            "text": reasoning
        }]
    }))
}

pub fn chat_message_to_response_output_item(message: &Value, response_id: &str) -> Option<Value> {
    let mut content = Vec::new();

    if let Some(text) = message.get("content").and_then(Value::as_str) {
        let text = split_leading_think_block(text)
            .map(|(_reasoning, answer)| answer)
            .unwrap_or_else(|| text.to_string());
        if !text.is_empty() {
            content.push(json!({
                "type": "output_text",
                "text": text,
                "annotations": []
            }));
        }
    } else if let Some(parts) = message.get("content").and_then(Value::as_array) {
        for part in parts {
            let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
            match part_type {
                "text" | "output_text" => {
                    if let Some(text) = part.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            content.push(json!({
                                "type": "output_text",
                                "text": text,
                                "annotations": []
                            }));
                        }
                    }
                }
                "refusal" => {
                    if let Some(text) = part.get("refusal").and_then(Value::as_str) {
                        if !text.is_empty() {
                            content.push(json!({
                                "type": "refusal",
                                "refusal": text
                            }));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) {
        if !refusal.is_empty() {
            content.push(json!({
                "type": "refusal",
                "refusal": refusal
            }));
        }
    }

    if content.is_empty() {
        return None;
    }

    Some(json!({
        "id": format!("{response_id}_msg"),
        "type": "message",
        "status": "completed",
        "role": "assistant",
        "content": content
    }))
}

pub fn responses_custom_tool_call_to_chat_tool_call(item: &Value) -> Value {
    let call_id = item
        .get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
    let input = item.get("input").cloned().unwrap_or_else(|| json!(""));

    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": canonical_json_string(&json!({ CUSTOM_TOOL_INPUT_FIELD: input }))
        }
    })
}

pub fn responses_tool_search_call_to_chat_tool_call(item: &Value) -> Value {
    let call_id = item
        .get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let arguments = item
        .get("arguments")
        .map(canonical_json_string)
        .unwrap_or_else(|| "{}".to_string());

    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": CODEX_TOOL_SEARCH_PROXY_NAME,
            "arguments": arguments
        }
    })
}

pub fn chat_usage_to_responses_usage(usage: Option<&Value>) -> Value {
    let Some(usage) = usage.filter(|value| value.is_object() && !value.is_null()) else {
        return json!({
            "input_tokens": 0,
            "output_tokens": 0,
            "total_tokens": 0,
            "output_tokens_details": { "reasoning_tokens": 0 }
        });
    };

    let input_tokens = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(input_tokens + output_tokens);

    let mut result = json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": total_tokens
    });

    if let Some(cached) = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cached_tokens"))
        .and_then(Value::as_u64)
    {
        result["input_tokens_details"] = json!({ "cached_tokens": cached });
    }

    if let Some(details) = usage
        .get("completion_tokens_details")
        .filter(|value| value.is_object())
    {
        let mut details = details.clone();
        if details.get("reasoning_tokens").is_none() {
            details["reasoning_tokens"] = json!(0);
        }
        result["output_tokens_details"] = details;
    } else {
        result["output_tokens_details"] = json!({ "reasoning_tokens": 0 });
    }

    if let Some(cache_read) = usage.get("cache_read_input_tokens") {
        result["cache_read_input_tokens"] = cache_read.clone();
    }
    if let Some(cache_creation) = usage.get("cache_creation_input_tokens") {
        result["cache_creation_input_tokens"] = cache_creation.clone();
    }

    result
}

pub fn response_id_from_chat_id(id: Option<&str>) -> String {
    let id = id.unwrap_or("ccswitch");
    if id.starts_with("resp_") {
        id.to_string()
    } else {
        format!("resp_{id}")
    }
}

pub fn response_status_from_finish_reason(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("length") => "incomplete",
        _ => "completed",
    }
}

pub fn responses_role_to_chat_role(role: &str) -> &'static str {
    match role {
        "system" | "developer" => "system",
        "assistant" => "assistant",
        "tool" => "tool",
        "user" | "latest_reminder" => "user",
        _ => "user",
    }
}

pub fn responses_instruction_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.as_str())
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        other => other.as_str().unwrap_or_default().to_string(),
    }
}

/// MiniMax 严格要求 messages 中只能首条出现 `role=system`。
/// 将所有 system 消息合并到首位，避免 Codex developer/system 指令出现在中间。
pub fn collapse_system_messages_to_head(messages: Vec<Value>) -> Vec<Value> {
    let mut system_chunks: Vec<String> = Vec::new();
    let mut rest: Vec<Value> = Vec::with_capacity(messages.len());

    for msg in messages {
        if msg.get("role").and_then(Value::as_str) == Some("system") {
            if let Some(text) = msg.get("content").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    system_chunks.push(text.to_string());
                }
                continue;
            }
        }
        rest.push(msg);
    }

    let mut out: Vec<Value> = Vec::with_capacity(rest.len() + 1);
    if !system_chunks.is_empty() {
        out.push(json!({
            "role": "system",
            "content": system_chunks.join("\n\n")
        }));
    }
    out.extend(rest);
    out
}

pub fn append_pending_reasoning(
    pending_reasoning: &mut Option<String>,
    reasoning: Option<String>,
) {
    let Some(reasoning) = reasoning else {
        return;
    };
    let reasoning = reasoning.trim();
    if reasoning.is_empty() {
        return;
    }

    match pending_reasoning {
        Some(existing) if !existing.is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(reasoning);
        }
        _ => {
            *pending_reasoning = Some(reasoning.to_string());
        }
    }
}

pub fn append_unique_pending_reasoning(
    pending_reasoning: &mut Option<String>,
    reasoning: Option<String>,
) {
    let Some(reasoning) = reasoning else {
        return;
    };
    let reasoning = reasoning.trim();
    if reasoning.is_empty() {
        return;
    }

    match pending_reasoning {
        Some(existing) if existing.contains(reasoning) => {}
        Some(existing) if !existing.is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(reasoning);
        }
        _ => {
            *pending_reasoning = Some(reasoning.to_string());
        }
    }
}

pub fn attach_pending_reasoning_to_assistant(
    message: &mut Value,
    pending_reasoning: &mut Option<String>,
) {
    let Some(reasoning) = pending_reasoning.take() else {
        return;
    };
    if reasoning.trim().is_empty() {
        return;
    }

    if let Some(obj) = message.as_object_mut() {
        append_reasoning_content(obj, &reasoning);
    }
}

/// Backfill assistant tool-call messages with a non-empty `reasoning_content`.
pub fn backfill_tool_call_reasoning_placeholders(messages: &mut [Value]) {
    for message in messages.iter_mut() {
        let is_assistant_tool_call = message.get("role").and_then(Value::as_str)
            == Some("assistant")
            && message
                .get("tool_calls")
                .and_then(Value::as_array)
                .is_some_and(|calls| !calls.is_empty());
        if is_assistant_tool_call {
            ensure_tool_call_reasoning_content(message);
        }
    }
}

pub fn ensure_tool_call_reasoning_content(message: &mut Value) {
    let Some(obj) = message.as_object_mut() else {
        return;
    };
    let has_reasoning = obj
        .get("reasoning_content")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty());
    if !has_reasoning {
        obj.insert(
            "reasoning_content".to_string(),
            Value::String("tool call".to_string()),
        );
    }
}

pub fn attach_reasoning_to_last_assistant(
    messages: &mut [Value],
    last_assistant_index: Option<usize>,
    reasoning: &Option<String>,
) -> bool {
    let Some(reasoning) = reasoning
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return true;
    };
    let Some(index) = last_assistant_index else {
        return false;
    };
    let Some(message) = messages.get_mut(index) else {
        return false;
    };
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return false;
    }

    if let Some(obj) = message.as_object_mut() {
        append_reasoning_content(obj, reasoning);
        return true;
    }

    false
}

pub fn responses_content_to_chat_content(_role: &str, content: &Value) -> Value {
    if content.is_null() || content.is_string() {
        return content.clone();
    }

    let Some(parts) = content.as_array() else {
        return content.clone();
    };

    let mut chat_parts: Vec<Value> = Vec::new();
    let mut has_non_text_part = false;

    for part in parts {
        let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
        match part_type {
            "input_text" | "output_text" | "text" => {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    if !text.is_empty() {
                        chat_parts.push(json!({
                            "type": "text",
                            "text": text
                        }));
                    }
                }
            }
            "refusal" => {
                if let Some(text) = part.get("refusal").and_then(Value::as_str) {
                    if !text.is_empty() {
                        chat_parts.push(json!({
                            "type": "text",
                            "text": text
                        }));
                    }
                }
            }
            "input_image" => {
                if let Some(image_url) = part.get("image_url") {
                    let image_url = if image_url.is_object() {
                        image_url.clone()
                    } else {
                        json!({ "url": image_url.as_str().unwrap_or_default() })
                    };
                    chat_parts.push(json!({
                        "type": "image_url",
                        "image_url": image_url
                    }));
                    has_non_text_part = true;
                }
            }
            "input_file" => {
                if let Some(file) = responses_input_file_to_chat_file(part) {
                    chat_parts.push(json!({
                        "type": "file",
                        "file": file
                    }));
                    has_non_text_part = true;
                }
            }
            "input_audio" => {
                if let Some(input_audio) = part.get("input_audio") {
                    chat_parts.push(json!({
                        "type": "input_audio",
                        "input_audio": input_audio.clone()
                    }));
                    has_non_text_part = true;
                }
            }
            _ => {}
        }
    }

    if !has_non_text_part {
        return Value::String(
            chat_parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    Value::Array(chat_parts)
}

fn responses_input_file_to_chat_file(part: &Value) -> Option<Value> {
    let mut file = serde_json::Map::new();
    let has_supported_file_ref = part.get("file_id").is_some() || part.get("file_data").is_some();
    if !has_supported_file_ref {
        return None;
    }

    for key in ["file_id", "file_data", "filename"] {
        if let Some(value) = part.get(key) {
            file.insert(key.to_string(), value.clone());
        }
    }
    Some(Value::Object(file))
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
    fn codex_chat_reasoning_options_pass_native_effort_without_provider_config() {
        let body = json!({
            "model": "o4-mini",
            "reasoning": {"effort": "high"}
        });
        let mut result = json!({});

        apply_codex_chat_reasoning_options(&mut result, &body, None, true);

        assert_eq!(result["reasoning_effort"], "high");
    }

    #[test]
    fn codex_chat_reasoning_options_map_deepseek_effort_and_thinking() {
        let body = json!({
            "model": "deepseek-reasoner",
            "reasoning": {"effort": "xhigh"}
        });
        let config = CodexChatReasoningOptions {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("deepseek".to_string()),
        };
        let mut result = json!({});

        apply_codex_chat_reasoning_options(&mut result, &body, Some(&config), false);

        assert_eq!(result["thinking"]["type"], "enabled");
        assert_eq!(result["reasoning_effort"], "max");
        assert!(result.get("reasoning").is_none());
    }

    #[test]
    fn codex_chat_reasoning_options_map_openrouter_native_effort() {
        let body = json!({
            "model": "openai/gpt-5",
            "reasoning": {"effort": "max"}
        });
        let config = CodexChatReasoningOptions {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning.effort".to_string()),
            effort_value_mode: Some("openrouter".to_string()),
        };
        let mut result = json!({});

        apply_codex_chat_reasoning_options(&mut result, &body, Some(&config), false);

        assert_eq!(result["reasoning"]["effort"], "xhigh");
        assert!(result.get("reasoning_effort").is_none());
        assert!(result.get("thinking").is_none());
    }

    #[test]
    fn codex_chat_reasoning_options_preserve_openrouter_explicit_none() {
        let config = CodexChatReasoningOptions {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning.effort".to_string()),
            effort_value_mode: Some("openrouter".to_string()),
        };

        for body in [
            json!({"reasoning": {"effort": "none"}}),
            json!({"reasoning": null}),
        ] {
            let mut result = json!({});

            apply_codex_chat_reasoning_options(&mut result, &body, Some(&config), false);

            assert_eq!(result["reasoning"]["effort"], "none");
            assert!(result.get("reasoning_effort").is_none());
            assert!(result.get("thinking").is_none());
        }
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
    fn appends_reasoning_content_without_overwriting_existing_text() {
        let mut message = json!({"reasoning_content": "first"})
            .as_object()
            .expect("object")
            .clone();

        assert!(append_reasoning_content(&mut message, " second "));
        assert_eq!(
            message["reasoning_content"],
            Value::String("first\n\nsecond".to_string())
        );
        assert!(!append_reasoning_content(&mut message, "   "));
    }

    #[test]
    fn builds_codex_response_function_call_items() {
        let item = response_function_call_item_with_namespace(
            "fc_1",
            "completed",
            "call_1",
            "shell",
            Some("tools"),
            r#"{"cmd":"pwd"}"#,
            Some("plan"),
        );

        assert_eq!(item["id"], "fc_1");
        assert_eq!(item["type"], "function_call");
        assert_eq!(item["status"], "completed");
        assert_eq!(item["call_id"], "call_1");
        assert_eq!(item["name"], "shell");
        assert_eq!(item["namespace"], "tools");
        assert_eq!(item["arguments"], r#"{"cmd":"pwd"}"#);
        assert_eq!(item["reasoning_content"], "plan");

        let no_reasoning =
            response_function_call_item("fc_2", "completed", "call_2", "read", "{}", None);
        assert!(no_reasoning.get("reasoning_content").is_none());
    }

    #[test]
    fn maps_chat_usage_to_responses_usage_shape() {
        let usage = chat_usage_to_responses_usage(Some(&json!({
            "prompt_tokens": 4,
            "completion_tokens": 6,
            "total_tokens": 10,
            "prompt_tokens_details": { "cached_tokens": 2 },
            "completion_tokens_details": { "reasoning_tokens": 3 },
            "cache_read_input_tokens": 5,
            "cache_creation_input_tokens": 7
        })));

        assert_eq!(usage["input_tokens"], 4);
        assert_eq!(usage["output_tokens"], 6);
        assert_eq!(usage["total_tokens"], 10);
        assert_eq!(usage["input_tokens_details"]["cached_tokens"], 2);
        assert_eq!(usage["output_tokens_details"]["reasoning_tokens"], 3);
        assert_eq!(usage["cache_read_input_tokens"], 5);
        assert_eq!(usage["cache_creation_input_tokens"], 7);

        let fallback = chat_usage_to_responses_usage(None);
        assert_eq!(fallback["input_tokens"], 0);
        assert_eq!(fallback["output_tokens_details"]["reasoning_tokens"], 0);
    }

    #[test]
    fn extracts_custom_tool_input_and_maps_response_identity() {
        assert_eq!(
            custom_tool_input_from_chat_arguments(r#"{"input":"run tests","extra":true}"#),
            "run tests"
        );
        assert_eq!(
            custom_tool_input_from_chat_arguments(r#"{"query":"fallback"}"#),
            r#"{"query":"fallback"}"#
        );
        assert_eq!(custom_tool_input_from_chat_arguments(" plain "), " plain ");

        assert_eq!(response_id_from_chat_id(Some("chatcmpl_1")), "resp_chatcmpl_1");
        assert_eq!(response_id_from_chat_id(Some("resp_1")), "resp_1");
        assert_eq!(response_id_from_chat_id(None), "resp_ccswitch");
        assert_eq!(response_status_from_finish_reason(Some("length")), "incomplete");
        assert_eq!(response_status_from_finish_reason(Some("stop")), "completed");
        assert_eq!(response_status_from_finish_reason(None), "completed");
        assert_eq!(response_tool_call_item_id("call_1", false), "fc_call_1");
        assert_eq!(response_tool_call_item_id("call_1", true), "ctc_call_1");
    }

    #[test]
    fn builds_codex_response_tool_search_and_custom_tool_call_items() {
        let tool_search = response_tool_search_call_item(
            "call_search",
            "completed",
            r#"{"query":"Gmail search emails","limit":10}"#,
            Some("look up tool"),
        );
        assert_eq!(tool_search["type"], "tool_search_call");
        assert_eq!(tool_search["execution"], "client");
        assert_eq!(tool_search["arguments"]["query"], "Gmail search emails");
        assert_eq!(tool_search["arguments"]["limit"], 10);
        assert_eq!(tool_search["reasoning_content"], "look up tool");

        let fallback = response_tool_search_call_item(
            "call_plain",
            "completed",
            "Gmail search emails",
            None,
        );
        assert_eq!(fallback["arguments"], json!({"query": "Gmail search emails"}));

        let empty = response_tool_search_call_item("call_empty", "completed", "  ", None);
        assert_eq!(empty["arguments"], json!({}));

        let custom = response_custom_tool_call_item(
            "ctc_call_patch",
            "completed",
            "call_patch",
            "apply_patch",
            r#"{"input":"*** Begin Patch\n*** End Patch"}"#,
            Some("apply edit"),
        );
        assert_eq!(custom["id"], "ctc_call_patch");
        assert_eq!(custom["type"], "custom_tool_call");
        assert_eq!(custom["call_id"], "call_patch");
        assert_eq!(custom["name"], "apply_patch");
        assert_eq!(custom["input"], "*** Begin Patch\n*** End Patch");
        assert_eq!(custom["reasoning_content"], "apply edit");
    }

    #[test]
    fn maps_codex_responses_tool_definitions_to_chat_tools() {
        assert_eq!(
            flatten_namespace_tool_name("mcp__gmail", "search"),
            "mcp__gmail__search"
        );
        let long_name = flatten_namespace_tool_name(
            "mcp__very_long_namespace_name_that_needs_to_be_shortened",
            "very_long_tool_name_that_also_needs_hashing",
        );
        assert!(long_name.len() <= 64);
        assert!(long_name.starts_with("mcp__very_long_namespace"));

        assert_eq!(
            responses_tool_name(&json!({"function": {"name": " lookup "}})).as_deref(),
            Some("lookup")
        );
        assert_eq!(responses_tool_name(&json!({"name": "  "})), None);

        let function_tool = responses_function_tool_to_chat_tool(
            &json!({
                "type": "function",
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {"type": "object"},
                "strict": true
            }),
            "weather__get_weather",
        )
        .expect("function tool");
        assert_eq!(function_tool["function"]["name"], "weather__get_weather");
        assert_eq!(function_tool["function"]["strict"], true);

        let nested_function_tool = responses_function_tool_to_chat_tool(
            &json!({
                "type": "function",
                "function": {
                    "name": "original",
                    "description": "Nested shape",
                    "parameters": {"type": "object"}
                },
                "strict": true
            }),
            "renamed",
        )
        .expect("nested function tool");
        assert_eq!(nested_function_tool["function"]["name"], "renamed");
        assert_eq!(nested_function_tool["function"]["strict"], true);

        let custom_tool = responses_custom_tool_to_chat_tool(
            "apply_patch",
            &json!({
                "type": "custom",
                "name": "apply_patch",
                "format": {"type": "grammar", "syntax": "lark"}
            }),
        );
        let description = custom_tool["function"]["description"]
            .as_str()
            .expect("description");
        assert!(description.starts_with("Original tool definition:"));
        assert!(description.contains("\"syntax\":\"lark\""));
        assert_eq!(
            custom_tool["function"]["parameters"]["required"][0],
            CUSTOM_TOOL_INPUT_FIELD
        );
    }

    #[test]
    fn builds_codex_tool_context_from_request_tools_and_tool_search_output() {
        let context = build_codex_tool_context_from_request(&json!({
            "tools": [
                {"type": "function", "name": "get_weather", "parameters": {"type": "object"}},
                {"type": "custom", "name": "apply_patch"},
                {"type": "tool_search"}
            ],
            "input": [{
                "type": "tool_search_output",
                "call_id": "call_tool_search_1",
                "tools": [{
                    "type": "namespace",
                    "name": "mcp__codex_apps__gmail",
                    "tools": [{
                        "type": "function",
                        "name": "_search_emails",
                        "parameters": {"type": "object"}
                    }]
                }]
            }]
        }));

        let tool_names = context
            .chat_tools()
            .iter()
            .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(tool_names.contains(&"get_weather"));
        assert!(tool_names.contains(&"apply_patch"));
        assert!(tool_names.contains(&CODEX_TOOL_SEARCH_PROXY_NAME));
        assert!(tool_names.contains(&"mcp__codex_apps__gmail___search_emails"));

        assert!(context.is_custom_tool_chat_name("apply_patch"));
        assert_eq!(
            context
                .lookup_chat_name("mcp__codex_apps__gmail___search_emails")
                .expect("namespace spec")
                .namespace
                .as_deref(),
            Some("mcp__codex_apps__gmail")
        );
        assert_eq!(
            context.chat_name_for_response_function(
                "_search_emails",
                Some("mcp__codex_apps__gmail")
            ),
            "mcp__codex_apps__gmail___search_emails"
        );

        let custom_id = response_tool_call_item_id_from_chat_name(
            "call_patch",
            "apply_patch",
            &context,
        );
        assert_eq!(custom_id, "ctc_call_patch");
        let custom_item = response_tool_call_item_from_chat_name(
            &custom_id,
            "completed",
            "call_patch",
            "apply_patch",
            r#"{"input":"patch"}"#,
            Some("edit"),
            &context,
        );
        assert_eq!(custom_item["type"], "custom_tool_call");
        assert_eq!(custom_item["input"], "patch");
        assert_eq!(custom_item["reasoning_content"], "edit");

        let tool_search = response_tool_call_item_from_chat_name(
            "fc_call_search",
            "completed",
            "call_search",
            CODEX_TOOL_SEARCH_PROXY_NAME,
            r#"{"query":"gmail"}"#,
            None,
            &context,
        );
        assert_eq!(tool_search["type"], "tool_search_call");
        assert_eq!(tool_search["arguments"]["query"], "gmail");

        let namespace_item = response_tool_call_item_from_chat_name(
            "fc_call_gmail",
            "completed",
            "call_gmail",
            "mcp__codex_apps__gmail___search_emails",
            r#"{"query":"in:inbox"}"#,
            None,
            &context,
        );
        assert_eq!(namespace_item["type"], "function_call");
        assert_eq!(namespace_item["namespace"], "mcp__codex_apps__gmail");
        assert_eq!(namespace_item["name"], "_search_emails");

        let fallback = response_tool_call_item_from_chat_name(
            "fc_call_unknown",
            "completed",
            "call_unknown",
            "unknown_tool",
            "{}",
            None,
            &context,
        );
        assert_eq!(fallback["type"], "function_call");
        assert_eq!(fallback["name"], "unknown_tool");
    }

    #[test]
    fn maps_codex_chat_tool_calls_to_response_output_items() {
        let context = build_codex_tool_context_from_request(&json!({
            "tools": [{"type": "custom", "name": "apply_patch"}]
        }));

        let custom_items = chat_tool_calls_to_response_output_items(
            &json!({
                "tool_calls": [{
                    "id": "call_patch",
                    "function": {
                        "name": "apply_patch",
                        "arguments": {"input": "*** Begin Patch\n*** End Patch"}
                    }
                }]
            }),
            Some("edit"),
            &context,
        );
        assert_eq!(custom_items[0]["id"], "ctc_call_patch");
        assert_eq!(custom_items[0]["type"], "custom_tool_call");
        assert_eq!(custom_items[0]["input"], "*** Begin Patch\n*** End Patch");
        assert_eq!(custom_items[0]["reasoning_content"], "edit");

        let fallback_items = chat_tool_calls_to_response_output_items(
            &json!({
                "tool_calls": [{
                    "function": {
                        "name": "lookup",
                        "arguments": {"b": 2, "a": 1}
                    }
                }]
            }),
            None,
            &context,
        );
        assert_eq!(fallback_items[0]["id"], "fc_call_0");
        assert_eq!(fallback_items[0]["call_id"], "call_0");
        assert_eq!(fallback_items[0]["name"], "lookup");
        assert_eq!(fallback_items[0]["arguments"], r#"{"a":1,"b":2}"#);

        let legacy_items = chat_tool_calls_to_response_output_items(
            &json!({
                "function_call": {
                    "name": "legacy_lookup",
                    "arguments": {"query": "mail"}
                }
            }),
            None,
            &context,
        );
        assert_eq!(legacy_items[0]["id"], "fc_call_0");
        assert_eq!(legacy_items[0]["call_id"], "call_0");
        assert_eq!(legacy_items[0]["name"], "legacy_lookup");
    }

    #[test]
    fn converts_codex_chat_completion_to_response_with_context() {
        let context = build_codex_tool_context_from_request(&json!({
            "tools": [{"type": "custom", "name": "apply_patch"}]
        }));
        let response = chat_completion_to_response_with_context(
            &json!({
                "id": "chatcmpl_123",
                "created": 42,
                "model": "gpt-5.4",
                "choices": [{
                    "finish_reason": "length",
                    "message": {
                        "role": "assistant",
                        "content": "Done",
                        "reasoning_content": "Need edit.",
                        "tool_calls": [{
                            "id": "call_patch",
                            "function": {
                                "name": "apply_patch",
                                "arguments": {"input": "*** Begin Patch\n*** End Patch"}
                            }
                        }]
                    }
                }],
                "usage": {
                    "prompt_tokens": 3,
                    "completion_tokens": 5,
                    "total_tokens": 8
                }
            }),
            &context,
        )
        .expect("response");

        assert_eq!(response["id"], "resp_chatcmpl_123");
        assert_eq!(response["status"], "incomplete");
        assert_eq!(response["incomplete_details"]["reason"], "max_output_tokens");
        assert_eq!(response["output"][0]["type"], "reasoning");
        assert_eq!(response["output"][1]["type"], "message");
        assert_eq!(response["output"][2]["type"], "custom_tool_call");
        assert_eq!(response["usage"]["input_tokens"], 3);
        assert_eq!(response["usage"]["output_tokens"], 5);

        let error =
            chat_completion_to_response_with_context(&json!({"choices": []}), &context).unwrap_err();
        assert_eq!(error, "Empty choices in chat response");
    }

    #[test]
    fn maps_codex_responses_special_tool_calls_to_chat_tool_calls() {
        let function = responses_function_call_to_chat_tool_call(
            &json!({
                "type": "function_call",
                "call_id": "call_lookup",
                "name": "lookup",
                "arguments": {"b": 2, "a": 1}
            }),
            "namespace__lookup",
        );
        assert_eq!(function["id"], "call_lookup");
        assert_eq!(function["function"]["name"], "namespace__lookup");
        assert_eq!(function["function"]["arguments"], r#"{"a":1,"b":2}"#);

        let tool_choice = responses_tool_choice_to_chat_function_selector("namespace__lookup");
        assert_eq!(tool_choice["type"], "function");
        assert_eq!(tool_choice["function"]["name"], "namespace__lookup");

        let custom = responses_custom_tool_call_to_chat_tool_call(&json!({
            "type": "custom_tool_call",
            "call_id": "call_patch",
            "name": "apply_patch",
            "input": "*** Begin Patch\n*** End Patch"
        }));
        assert_eq!(custom["id"], "call_patch");
        assert_eq!(custom["function"]["name"], "apply_patch");
        assert_eq!(
            custom["function"]["arguments"],
            r#"{"input":"*** Begin Patch\n*** End Patch"}"#
        );

        let tool_search = responses_tool_search_call_to_chat_tool_call(&json!({
            "type": "tool_search_call",
            "call_id": "call_search",
            "arguments": {"limit": 10, "query": "gmail"}
        }));
        assert_eq!(tool_search["id"], "call_search");
        assert_eq!(
            tool_search["function"]["name"],
            CODEX_TOOL_SEARCH_PROXY_NAME
        );
        assert_eq!(
            tool_search["function"]["arguments"],
            r#"{"limit":10,"query":"gmail"}"#
        );
    }

    #[test]
    fn maps_codex_responses_contextual_tool_names_to_chat() {
        let context = build_codex_tool_context_from_request(&json!({
            "tools": [
                {"type": "tool_search"},
                {"type": "custom", "name": "apply_patch"},
                {
                    "type": "namespace",
                    "name": "mcp__svc",
                    "tools": [{
                        "type": "function",
                        "name": "lookup",
                        "parameters": {"type": "object"}
                    }]
                }
            ]
        }));

        let function = responses_function_call_to_chat_tool_call_with_context(
            &json!({
                "type": "function_call",
                "call_id": "call_lookup",
                "namespace": "mcp__svc",
                "name": "lookup",
                "arguments": {"query": "mail"}
            }),
            &context,
        );
        assert_eq!(function["function"]["name"], "mcp__svc__lookup");

        let function_choice = responses_tool_choice_to_chat_tool_choice(
            &json!({"type": "function", "namespace": "mcp__svc", "name": "lookup"}),
            &context,
        );
        assert_eq!(function_choice["function"]["name"], "mcp__svc__lookup");

        let custom_choice = responses_tool_choice_to_chat_tool_choice(
            &json!({"type": "custom", "name": "apply_patch"}),
            &context,
        );
        assert_eq!(custom_choice["function"]["name"], "apply_patch");

        let tool_search_choice =
            responses_tool_choice_to_chat_tool_choice(&json!({"type": "tool_search"}), &context);
        assert_eq!(
            tool_search_choice["function"]["name"],
            CODEX_TOOL_SEARCH_PROXY_NAME
        );

        let unknown_choice = json!({"type": "allowed_tools", "mode": "auto"});
        assert_eq!(
            responses_tool_choice_to_chat_tool_choice(&unknown_choice, &context),
            unknown_choice
        );
    }

    #[test]
    fn appends_codex_responses_input_as_chat_messages() {
        let request = json!({
            "tools": [{
                "type": "function",
                "name": "lookup_weather",
                "parameters": {"type": "object"}
            }],
            "input": [
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Weather?"}]
                },
                {
                    "type": "reasoning",
                    "summary": [{"text": "Need current weather."}]
                },
                {
                    "type": "function_call",
                    "call_id": "call_weather",
                    "name": "lookup_weather",
                    "arguments": {"city": "Tokyo"}
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_weather",
                    "output": "Sunny"
                }
            ]
        });
        let context = build_codex_tool_context_from_request(&request);
        let mut messages = Vec::new();

        append_responses_input_as_chat_messages(&request["input"], &mut messages, &context);

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "Weather?");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(
            messages[1]["tool_calls"][0]["function"]["name"],
            "lookup_weather"
        );
        assert_eq!(messages[1]["reasoning_content"], "Need current weather.");
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call_weather");
        assert_eq!(messages[2]["content"], "Sunny");
    }

    #[test]
    fn converts_codex_responses_request_to_chat_with_options() {
        let request = json!({
            "model": "gpt-5.4",
            "instructions": "You are concise.",
            "input": [
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Weather?"}]
                },
                {
                    "type": "function_call",
                    "call_id": "call_weather",
                    "name": "get_weather",
                    "arguments": {"city": "Tokyo"}
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_weather",
                    "output": "Sunny"
                }
            ],
            "tools": [{
                "type": "function",
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {"type": "object"},
                "strict": true
            }],
            "tool_choice": {"type": "function", "name": "get_weather"},
            "max_output_tokens": 100,
            "reasoning": {"effort": "high"},
            "stream": true,
            "parallel_tool_calls": true
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, true);

        assert_eq!(result["model"], "gpt-5.4");
        assert_eq!(result["messages"][0]["role"], "system");
        assert_eq!(result["messages"][1]["content"], "Weather?");
        assert_eq!(
            result["messages"][2]["tool_calls"][0]["function"]["arguments"],
            r#"{"city":"Tokyo"}"#
        );
        assert_eq!(result["messages"][3]["role"], "tool");
        assert_eq!(result["tools"][0]["function"]["name"], "get_weather");
        assert_eq!(result["tool_choice"]["function"]["name"], "get_weather");
        assert_eq!(result["max_tokens"], 100);
        assert_eq!(result["reasoning_effort"], "high");
        assert_eq!(result["stream_options"]["include_usage"], true);
        assert_eq!(result["parallel_tool_calls"], true);

        let o_series = responses_to_chat_completions_with_options(
            &json!({"model": "o4-mini", "max_output_tokens": 7}),
            None,
            true,
            false,
        );
        assert_eq!(o_series["max_completion_tokens"], 7);
        assert!(o_series.get("max_tokens").is_none());
    }

    #[test]
    fn codex_responses_request_to_chat_drops_tool_fields_without_tools() {
        let request = json!({
            "model": "gpt-5.4",
            "input": "hello",
            "tool_choice": {"type": "function", "name": "missing_tool"},
            "parallel_tool_calls": true
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, false);

        assert!(result.get("tools").is_none());
        assert!(result.get("tool_choice").is_none());
        assert!(result.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn maps_codex_responses_tool_outputs_to_chat_tool_messages() {
        let json_output = responses_function_call_output_to_chat_tool_message(&json!({
            "type": "function_call_output",
            "call_id": "call_lookup",
            "output": "{ \"z\": true, \"a\": [2, 1] }"
        }));
        assert_eq!(json_output["role"], "tool");
        assert_eq!(json_output["tool_call_id"], "call_lookup");
        assert_eq!(json_output["content"], r#"{"a":[2,1],"z":true}"#);

        let text_output = responses_function_call_output_to_chat_tool_message(&json!({
            "type": "function_call_output",
            "call_id": "call_read",
            "output": "plain text result"
        }));
        assert_eq!(text_output["content"], "plain text result");

        let client_output = responses_client_tool_output_to_chat_tool_message(&json!({
            "type": "tool_search_output",
            "call_id": "call_search",
            "status": "completed",
            "tools": [{"name": "search_docs", "type": "function"}]
        }));
        assert_eq!(client_output["role"], "tool");
        assert_eq!(client_output["tool_call_id"], "call_search");
        assert_eq!(
            client_output["content"],
            r#"{"call_id":"call_search","status":"completed","tools":[{"name":"search_docs","type":"function"}],"type":"tool_search_output"}"#
        );
    }

    #[test]
    fn maps_chat_assistant_reasoning_and_message_to_responses_items() {
        let message = json!({
            "role": "assistant",
            "reasoning_content": "inspect state",
            "content": "done"
        });
        assert_eq!(
            chat_reasoning_text(&message).as_deref(),
            Some("inspect state")
        );

        let inline = json!({
            "role": "assistant",
            "content": "<think>plan</think>\n\nanswer"
        });
        assert_eq!(chat_reasoning_text(&inline).as_deref(), Some("plan"));

        let reasoning_item =
            chat_reasoning_to_response_output_item(Some("inspect state"), "resp_1")
                .expect("reasoning item");
        assert_eq!(reasoning_item["id"], "rs_resp_1");
        assert_eq!(reasoning_item["type"], "reasoning");
        assert_eq!(reasoning_item["summary"][0]["text"], "inspect state");
        assert!(chat_reasoning_to_response_output_item(Some(""), "resp_1").is_none());

        let message_item =
            chat_message_to_response_output_item(&inline, "resp_1").expect("message item");
        assert_eq!(message_item["id"], "resp_1_msg");
        assert_eq!(message_item["role"], "assistant");
        assert_eq!(message_item["content"][0]["type"], "output_text");
        assert_eq!(message_item["content"][0]["text"], "answer");

        let parts = chat_message_to_response_output_item(
            &json!({
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "hello"},
                    {"type": "refusal", "refusal": "no"}
                ],
                "refusal": "top-level refusal"
            }),
            "resp_2",
        )
        .expect("parts message");
        assert_eq!(parts["content"][0]["text"], "hello");
        assert_eq!(parts["content"][1]["refusal"], "no");
        assert_eq!(parts["content"][2]["refusal"], "top-level refusal");
    }

    #[test]
    fn maps_responses_roles_and_pending_reasoning_to_chat_messages() {
        assert_eq!(responses_role_to_chat_role("developer"), "system");
        assert_eq!(responses_role_to_chat_role("latest_reminder"), "user");
        assert_eq!(responses_role_to_chat_role("unknown"), "user");

        let mut pending = None;
        append_pending_reasoning(&mut pending, Some(" first ".to_string()));
        append_pending_reasoning(&mut pending, Some("second".to_string()));
        assert_eq!(pending.as_deref(), Some("first\n\nsecond"));

        append_unique_pending_reasoning(&mut pending, Some("second".to_string()));
        assert_eq!(pending.as_deref(), Some("first\n\nsecond"));
        append_unique_pending_reasoning(&mut pending, Some("third".to_string()));
        assert_eq!(pending.as_deref(), Some("first\n\nsecond\n\nthird"));

        let mut message = json!({"role": "assistant", "content": ""});
        attach_pending_reasoning_to_assistant(&mut message, &mut pending);
        assert!(pending.is_none());
        assert_eq!(
            message["reasoning_content"],
            Value::String("first\n\nsecond\n\nthird".to_string())
        );
    }

    #[test]
    fn extracts_responses_instruction_text() {
        assert_eq!(
            responses_instruction_text(&json!("You are concise.")),
            "You are concise."
        );
        assert_eq!(
            responses_instruction_text(&json!([
                {"type": "input_text", "text": "first"},
                "second",
                {"type": "input_text", "text": ""},
                {"type": "ignored"}
            ])),
            "first\n\nsecond"
        );
        assert_eq!(responses_instruction_text(&json!({"text": "ignored"})), "");
    }

    #[test]
    fn collapses_system_messages_to_head_preserving_non_system_order() {
        let input = vec![
            json!({"role": "system", "content": "S1"}),
            json!({"role": "user", "content": "U1"}),
            json!({"role": "assistant", "content": "A1"}),
            json!({"role": "system", "content": "  "}),
            json!({"role": "system", "content": "S2"}),
            json!({"role": "user", "content": "U2"}),
        ];

        let out = collapse_system_messages_to_head(input);

        assert_eq!(out.len(), 4);
        assert_eq!(out[0], json!({"role": "system", "content": "S1\n\nS2"}));
        assert_eq!(out[1]["content"], "U1");
        assert_eq!(out[2]["content"], "A1");
        assert_eq!(out[3]["content"], "U2");
    }

    #[test]
    fn maps_responses_content_text_parts_to_chat_text() {
        let content = responses_content_to_chat_content(
            "user",
            &json!([
                {"type": "input_text", "text": "hello"},
                {"type": "output_text", "text": "world"},
                {"type": "text", "text": ""},
                {"type": "refusal", "refusal": "no"}
            ]),
        );

        assert_eq!(content, Value::String("hello\nworld\nno".to_string()));
    }

    #[test]
    fn maps_responses_content_media_parts_to_chat_parts() {
        let content = responses_content_to_chat_content(
            "user",
            &json!([
                {"type": "input_text", "text": "see attachments"},
                {"type": "input_image", "image_url": "data:image/png;base64,abc"},
                {
                    "type": "input_file",
                    "file_id": "file_123",
                    "file_url": "https://example.com/spec.pdf",
                    "filename": "spec.pdf"
                },
                {
                    "type": "input_audio",
                    "input_audio": {
                        "data": "UklGRg==",
                        "format": "wav"
                    }
                }
            ]),
        );
        let parts = content.as_array().expect("content parts");

        assert_eq!(parts[0], json!({"type": "text", "text": "see attachments"}));
        assert_eq!(
            parts[1],
            json!({
                "type": "image_url",
                "image_url": {"url": "data:image/png;base64,abc"}
            })
        );
        assert_eq!(
            parts[2],
            json!({
                "type": "file",
                "file": {
                    "file_id": "file_123",
                    "filename": "spec.pdf"
                }
            })
        );
        assert_eq!(
            parts[3],
            json!({
                "type": "input_audio",
                "input_audio": {
                    "data": "UklGRg==",
                    "format": "wav"
                }
            })
        );
    }

    #[test]
    fn drops_unsupported_responses_input_file_url_only_parts() {
        let text_only = responses_content_to_chat_content(
            "user",
            &json!([
                {"type": "input_text", "text": "summarize url"},
                {
                    "type": "input_file",
                    "file_url": "https://example.com/spec.pdf"
                }
            ]),
        );
        assert_eq!(text_only, Value::String("summarize url".to_string()));

        let file_only = responses_content_to_chat_content(
            "user",
            &json!([
                {
                    "type": "input_file",
                    "file_url": "https://example.com/spec.pdf"
                }
            ]),
        );
        assert_eq!(file_only, Value::String(String::new()));
    }

    #[test]
    fn backfills_and_attaches_reasoning_to_last_assistant() {
        let mut messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{"id": "call_1"}]
            }),
            json!({
                "role": "user",
                "content": "next"
            }),
        ];

        backfill_tool_call_reasoning_placeholders(&mut messages);
        assert_eq!(messages[0]["reasoning_content"], "tool call");
        assert!(messages[1].get("reasoning_content").is_none());

        let reasoning = Some("real reasoning".to_string());
        assert!(attach_reasoning_to_last_assistant(
            &mut messages[..1],
            Some(0),
            &reasoning
        ));
        assert_eq!(
            messages[0]["reasoning_content"],
            Value::String("tool call\n\nreal reasoning".to_string())
        );
        assert!(!attach_reasoning_to_last_assistant(
            &mut messages,
            Some(1),
            &reasoning
        ));
        assert!(attach_reasoning_to_last_assistant(&mut messages, Some(0), &None));
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
