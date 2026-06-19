use crate::{
    json_canonical::{
        canonical_json_string, canonicalize_json_string_if_parseable, canonicalize_tool_arguments,
        canonicalize_tool_arguments_str, short_sha256_hex,
    },
    request_body::{
        clean_openai_tool_schema, codex_chat_reasoning_requested,
        inject_openai_stream_include_usage, is_openai_o_series,
        map_anthropic_tool_choice_to_openai_chat, map_anthropic_tool_choice_to_openai_responses,
        map_codex_chat_reasoning_effort, resolve_reasoning_effort,
        strip_leading_anthropic_billing_header, supports_reasoning_effort,
    },
    session::parse_session_from_user_id,
    usage::{
        build_anthropic_usage_from_openai_chat, build_anthropic_usage_from_openai_responses,
    },
    UpstreamSseAggregationKind,
};
use bytes::Bytes;
use futures::{stream as futures_stream, Stream, StreamExt};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    error::Error,
    io,
    pin::Pin,
};

pub const CLAUDE_API_FORMAT_METADATA_KEY: &str = "claudeApiFormat";
pub const CODEX_TOOL_SEARCH_PROXY_NAME: &str = "tool_search";
pub const ANTHROPIC_TOOL_THINKING_PLACEHOLDER: &str = "tool call";
pub const ANTHROPIC_REDACTED_THINKING_PLACEHOLDER: &str = "[redacted thinking]";
pub const DEEPSEEK_OFFICIAL_ANTHROPIC_URL: &str = "https://api.deepseek.com/anthropic";
const THINK_OPEN_TAG: &str = "<think>";
const THINK_CLOSE_TAG: &str = "</think>";
const CUSTOM_TOOL_INPUT_FIELD: &str = "input";
const CUSTOM_TOOL_INPUT_DESCRIPTION: &str = "Raw string input for the original custom tool. Preserve formatting exactly and follow the original tool definition embedded in the description.";
const CUSTOM_TOOL_PRESERVED_METADATA_HEADING: &str = "Original tool definition:";
const CHAT_TOOL_NAME_MAX_LEN: usize = 64;
// Keep hints lowercase; matching lowercases only the input value.
const REASONING_VENDOR_HINTS: &[&str] = &["moonshot", "kimi", "deepseek", "mimo", "xiaomimimo"];
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudePromptCacheKeySource {
    Explicit,
    Session,
    None,
}

impl ClaudePromptCacheKeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Session => "session",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudePromptCacheKeyResolution {
    pub key: Option<String>,
    pub source: ClaudePromptCacheKeySource,
}

/// Provider-neutral Codex Responses -> Chat Completions reasoning capability hints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexChatReasoningOptions {
    pub supports_thinking: Option<bool>,
    pub supports_effort: Option<bool>,
    pub thinking_param: Option<String>,
    pub effort_param: Option<String>,
    pub effort_value_mode: Option<String>,
}

impl CodexChatReasoningOptions {
    pub fn from_profile(profile: &CodexChatReasoningProfile) -> Self {
        Self {
            supports_thinking: profile.supports_thinking,
            supports_effort: profile.supports_effort,
            thinking_param: profile.thinking_param.clone(),
            effort_param: profile.effort_param.clone(),
            effort_value_mode: profile.effort_value_mode.clone(),
        }
    }
}

/// Provider-neutral Codex Responses -> Chat Completions reasoning profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexChatReasoningProfile {
    pub supports_thinking: Option<bool>,
    pub supports_effort: Option<bool>,
    pub thinking_param: Option<String>,
    pub effort_param: Option<String>,
    pub effort_value_mode: Option<String>,
    pub output_format: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkPrefixDecision {
    NeedMore,
    Reasoning,
    Text,
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

pub fn normalize_codex_chat_reasoning_profile(
    mut profile: CodexChatReasoningProfile,
) -> CodexChatReasoningProfile {
    if profile.supports_effort.unwrap_or(false) && profile.supports_thinking.is_none() {
        profile.supports_thinking = Some(true);
    }
    profile
}

pub fn infer_codex_chat_reasoning_profile(
    provider_name: &str,
    base_url: &str,
    model: &str,
) -> Option<CodexChatReasoningProfile> {
    let name = provider_name.to_ascii_lowercase();
    let base_url = base_url.to_ascii_lowercase();
    let model = model.to_ascii_lowercase();

    // Aggregator platforms decide the reasoning wire shape independently from
    // the hosted model vendor, so platform rules must run before model rules.
    if let Some(profile) = infer_codex_chat_aggregator_reasoning_profile(&name, &base_url) {
        return Some(profile);
    }

    let haystack = format!("{name} {base_url} {model}");

    if haystack.contains("deepseek") {
        return Some(CodexChatReasoningProfile {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("deepseek".to_string()),
            output_format: Some("reasoning_content".to_string()),
        });
    }

    // StepFun exposes reasoning effort only on step-3.5-flash-2603.
    if haystack.contains("stepfun") || haystack.contains("step-3.5-flash-2603") {
        return Some(CodexChatReasoningProfile {
            supports_thinking: Some(true),
            supports_effort: Some(model.contains("2603")),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("low_high".to_string()),
            output_format: Some("reasoning".to_string()),
        });
    }

    if haystack.contains("kimi") || haystack.contains("moonshot") {
        return Some(CodexChatReasoningProfile {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
        });
    }

    if haystack.contains("glm") || haystack.contains("zhipu") || haystack.contains("z.ai") {
        return Some(CodexChatReasoningProfile {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
        });
    }

    if haystack.contains("qwen") || haystack.contains("dashscope") || haystack.contains("bailian") {
        return Some(CodexChatReasoningProfile {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("enable_thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
        });
    }

    if haystack.contains("minimax") {
        return Some(CodexChatReasoningProfile {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("reasoning_split".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_details".to_string()),
        });
    }

    if haystack.contains("mimo") {
        return Some(CodexChatReasoningProfile {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
        });
    }

    None
}

fn infer_codex_chat_aggregator_reasoning_profile(
    name: &str,
    base_url: &str,
) -> Option<CodexChatReasoningProfile> {
    let platform = format!("{name} {base_url}");

    if platform.contains("openrouter") {
        return Some(CodexChatReasoningProfile {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning.effort".to_string()),
            effort_value_mode: Some("openrouter".to_string()),
            output_format: Some("auto".to_string()),
        });
    }

    if platform.contains("siliconflow") {
        return Some(CodexChatReasoningProfile {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("enable_thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            output_format: Some("reasoning_content".to_string()),
        });
    }

    None
}

pub fn apply_codex_oauth_responses_request_contract(
    result: &mut Value,
    source_body: &Value,
    codex_fast_mode: bool,
) {
    result["store"] = json!(false);
    if codex_fast_mode {
        result["service_tier"] = json!("priority");
    }

    const REASONING_MARKER: &str = "reasoning.encrypted_content";
    let mut includes: Vec<Value> = source_body
        .get("include")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if !includes
        .iter()
        .any(|value| value.as_str() == Some(REASONING_MARKER))
    {
        includes.push(json!(REASONING_MARKER));
    }
    result["include"] = json!(includes);

    if let Some(obj) = result.as_object_mut() {
        obj.remove("max_output_tokens");
        obj.remove("temperature");
        obj.remove("top_p");

        obj.entry("instructions".to_string()).or_insert(json!(""));
        obj.entry("tools".to_string()).or_insert(json!([]));
        obj.entry("parallel_tool_calls".to_string())
            .or_insert(json!(false));

        obj.insert("stream".to_string(), json!(true));
    }
}

pub fn anthropic_to_openai_responses_request(
    body: &Value,
    cache_key: Option<&str>,
    is_codex_oauth: bool,
    codex_fast_mode: bool,
) -> Value {
    let mut result = json!({});

    if let Some(model) = body.get("model").and_then(Value::as_str) {
        result["model"] = json!(model);
    }

    if let Some(system) = body.get("system") {
        let instructions = if let Some(text) = system.as_str() {
            strip_leading_anthropic_billing_header(text).to_string()
        } else if let Some(arr) = system.as_array() {
            arr.iter()
                .filter_map(|msg| msg.get("text").and_then(Value::as_str))
                .map(strip_leading_anthropic_billing_header)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n")
        } else {
            String::new()
        };
        if !instructions.is_empty() {
            result["instructions"] = json!(instructions);
        }
    }

    if let Some(messages) = body.get("messages").and_then(Value::as_array) {
        result["input"] = json!(anthropic_messages_to_openai_responses_input(messages));
    }

    if let Some(max_tokens) = body.get("max_tokens") {
        result["max_output_tokens"] = max_tokens.clone();
    }

    for passthrough in ["temperature", "top_p", "stream"] {
        if let Some(value) = body.get(passthrough) {
            result[passthrough] = value.clone();
        }
    }

    if let Some(model) = body.get("model").and_then(Value::as_str) {
        if supports_reasoning_effort(model) {
            if let Some(effort) = resolve_reasoning_effort(body) {
                result["reasoning"] = json!({ "effort": effort });
            }
        }
    }

    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let response_tools: Vec<Value> = tools
            .iter()
            .filter(|tool| tool.get("type").and_then(Value::as_str) != Some("BatchTool"))
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.get("name").and_then(Value::as_str).unwrap_or(""),
                    "description": tool.get("description"),
                    "parameters": clean_openai_tool_schema(
                        tool.get("input_schema").cloned().unwrap_or(json!({}))
                    )
                })
            })
            .collect();

        if !response_tools.is_empty() {
            result["tools"] = json!(response_tools);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = map_anthropic_tool_choice_to_openai_responses(tool_choice);
    }

    if let Some(key) = cache_key {
        result["prompt_cache_key"] = json!(key);
    }

    if is_codex_oauth {
        apply_codex_oauth_responses_request_contract(&mut result, body, codex_fast_mode);
    }

    result
}

pub fn resolve_claude_responses_prompt_cache_key(
    body: &Value,
    explicit_cache_key: Option<&str>,
    session_id: Option<&str>,
    is_copilot: bool,
) -> ClaudePromptCacheKeyResolution {
    if let Some(key) = explicit_cache_key
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        return ClaudePromptCacheKeyResolution {
            key: Some(key.to_string()),
            source: ClaudePromptCacheKeySource::Explicit,
        };
    }

    let session_key = if is_copilot {
        body.get("metadata")
            .and_then(|metadata| {
                metadata
                    .get("user_id")
                    .and_then(Value::as_str)
                    .and_then(parse_session_from_user_id)
                    .or_else(|| {
                        metadata
                            .get("session_id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|session| !session.is_empty())
                            .map(ToString::to_string)
                    })
            })
    } else {
        session_id
            .map(str::trim)
            .filter(|session| !session.is_empty())
            .map(ToString::to_string)
    };

    if let Some(key) = session_key {
        ClaudePromptCacheKeyResolution {
            key: Some(key),
            source: ClaudePromptCacheKeySource::Session,
        }
    } else {
        ClaudePromptCacheKeyResolution {
            key: None,
            source: ClaudePromptCacheKeySource::None,
        }
    }
}

pub fn is_copilot_prompt_cache_provider(
    meta_provider_type: Option<&str>,
    settings_config: &Value,
) -> bool {
    meta_provider_type == Some("github_copilot")
        || settings_config
            .get("baseUrl")
            .and_then(Value::as_str)
            .is_some_and(|url| url.contains("githubcopilot.com"))
}

fn anthropic_messages_to_openai_responses_input(messages: &[Value]) -> Vec<Value> {
    let mut input = Vec::new();

    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let content = message.get("content");

        match content {
            Some(Value::String(text)) => {
                let content_type = if role == "assistant" {
                    "output_text"
                } else {
                    "input_text"
                };
                input.push(json!({
                    "role": role,
                    "content": [{ "type": content_type, "text": text }]
                }));
            }
            Some(Value::Array(blocks)) => {
                let mut message_content = Vec::new();

                for block in blocks {
                    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");

                    match block_type {
                        "text" => {
                            if let Some(text) = block.get("text").and_then(Value::as_str) {
                                let content_type = if role == "assistant" {
                                    "output_text"
                                } else {
                                    "input_text"
                                };
                                message_content
                                    .push(json!({ "type": content_type, "text": text }));
                            }
                        }
                        "image" => {
                            if let Some(source) = block.get("source") {
                                let media_type = source
                                    .get("media_type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("image/png");
                                let data =
                                    source.get("data").and_then(Value::as_str).unwrap_or("");
                                message_content.push(json!({
                                    "type": "input_image",
                                    "image_url": format!("data:{media_type};base64,{data}")
                                }));
                            }
                        }
                        "tool_use" => {
                            if !message_content.is_empty() {
                                input.push(json!({
                                    "role": role,
                                    "content": message_content.clone()
                                }));
                                message_content.clear();
                            }

                            let id = block.get("id").and_then(Value::as_str).unwrap_or("");
                            let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                            let arguments = block.get("input").cloned().unwrap_or(json!({}));

                            input.push(json!({
                                "type": "function_call",
                                "call_id": id,
                                "name": name,
                                "arguments": canonical_json_string(&arguments)
                            }));
                        }
                        "tool_result" => {
                            if !message_content.is_empty() {
                                input.push(json!({
                                    "role": role,
                                    "content": message_content.clone()
                                }));
                                message_content.clear();
                            }

                            let call_id = block
                                .get("tool_use_id")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            let output = match block.get("content") {
                                Some(Value::String(value)) => value.clone(),
                                Some(value) => canonical_json_string(value),
                                None => String::new(),
                            };

                            input.push(json!({
                                "type": "function_call_output",
                                "call_id": call_id,
                                "output": output
                            }));
                        }
                        "thinking" => {}
                        _ => {}
                    }
                }

                if !message_content.is_empty() {
                    input.push(json!({
                        "role": role,
                        "content": message_content
                    }));
                }
            }
            _ => {
                input.push(json!({ "role": role }));
            }
        }
    }

    input
}

pub fn anthropic_to_openai_chat_request(
    body: &Value,
    preserve_reasoning_content: bool,
) -> Value {
    let mut result = json!({});

    if let Some(model) = body.get("model").and_then(Value::as_str) {
        result["model"] = json!(model);
    }

    let mut messages = Vec::new();

    if let Some(system) = body.get("system") {
        if let Some(text) = system.as_str() {
            let text = strip_leading_anthropic_billing_header(text);
            if !text.is_empty() {
                messages.push(json!({"role": "system", "content": text}));
            }
        } else if let Some(parts) = system.as_array() {
            for part in parts {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    let text = strip_leading_anthropic_billing_header(text);
                    if !text.is_empty() {
                        messages.push(json!({"role": "system", "content": text}));
                    }
                }
            }
        }
    }

    if let Some(input_messages) = body.get("messages").and_then(Value::as_array) {
        for message in input_messages {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user");
            let converted = anthropic_message_to_openai_chat_messages(
                role,
                message.get("content"),
                preserve_reasoning_content,
            );
            messages.extend(converted);
        }
    }

    normalize_openai_chat_system_messages(&mut messages);
    result["messages"] = json!(messages);

    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    if let Some(max_tokens) = body.get("max_tokens") {
        if is_openai_o_series(model) {
            result["max_completion_tokens"] = max_tokens.clone();
        } else {
            result["max_tokens"] = max_tokens.clone();
        }
    }

    for passthrough in ["temperature", "top_p", "stream"] {
        if let Some(value) = body.get(passthrough) {
            result[passthrough] = value.clone();
        }
    }
    if let Some(stop) = body.get("stop_sequences") {
        result["stop"] = stop.clone();
    }

    if supports_reasoning_effort(model) {
        if let Some(effort) = resolve_reasoning_effort(body) {
            result["reasoning_effort"] = json!(effort);
        }
    }

    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let openai_tools: Vec<Value> = tools
            .iter()
            .filter(|tool| tool.get("type").and_then(Value::as_str) != Some("BatchTool"))
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.get("name").and_then(Value::as_str).unwrap_or(""),
                        "description": tool.get("description"),
                        "parameters": clean_openai_tool_schema(
                            tool.get("input_schema").cloned().unwrap_or(json!({}))
                        )
                    }
                })
            })
            .collect();

        if !openai_tools.is_empty() {
            result["tools"] = json!(openai_tools);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = map_anthropic_tool_choice_to_openai_chat(tool_choice);
    }

    result
}

fn normalize_openai_chat_system_messages(messages: &mut Vec<Value>) {
    let system_count = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        .count();

    if system_count == 0 {
        return;
    }

    if system_count == 1 {
        if let Some(index) = messages
            .iter()
            .position(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        {
            if index > 0 {
                let message = messages.remove(index);
                messages.insert(0, message);
            }
        }
        return;
    }

    let mut parts = Vec::new();
    messages.retain(|message| {
        if message.get("role").and_then(Value::as_str) != Some("system") {
            return true;
        }

        match message.get("content") {
            Some(Value::String(text)) if !text.is_empty() => parts.push(text.clone()),
            Some(Value::Array(content_parts)) => {
                let text = content_parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            _ => {}
        }

        false
    });

    if !parts.is_empty() {
        messages.insert(0, json!({"role": "system", "content": parts.join("\n")}));
    }
}

fn anthropic_message_to_openai_chat_messages(
    role: &str,
    content: Option<&Value>,
    preserve_reasoning_content: bool,
) -> Vec<Value> {
    let mut result = Vec::new();
    let Some(content) = content else {
        result.push(json!({"role": role, "content": null}));
        return result;
    };

    if let Some(text) = content.as_str() {
        result.push(json!({"role": role, "content": text}));
        return result;
    }

    if let Some(blocks) = content.as_array() {
        let mut content_parts = Vec::new();
        let mut tool_calls = Vec::new();
        let mut reasoning_parts = Vec::new();

        for block in blocks {
            let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");

            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        content_parts.push(json!({"type": "text", "text": text}));
                    }
                }
                "image" => {
                    if let Some(source) = block.get("source") {
                        let media_type = source
                            .get("media_type")
                            .and_then(Value::as_str)
                            .unwrap_or("image/png");
                        let data = source.get("data").and_then(Value::as_str).unwrap_or("");
                        content_parts.push(json!({
                            "type": "image_url",
                            "image_url": {"url": format!("data:{};base64,{}", media_type, data)}
                        }));
                    }
                }
                "tool_use" => {
                    let id = block.get("id").and_then(Value::as_str).unwrap_or("");
                    let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                    let input = block.get("input").cloned().unwrap_or(json!({}));
                    tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": canonical_json_string(&input)
                        }
                    }));
                }
                "tool_result" => {
                    let tool_use_id = block
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let content_str = match block.get("content") {
                        Some(Value::String(value)) => value.clone(),
                        Some(value) => canonical_json_string(value),
                        None => String::new(),
                    };
                    result.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_use_id,
                        "content": content_str
                    }));
                }
                "thinking" => {
                    if let Some(thinking) = block.get("thinking").and_then(Value::as_str) {
                        if !thinking.is_empty() {
                            reasoning_parts.push(thinking.to_string());
                        }
                    }
                }
                "redacted_thinking" if preserve_reasoning_content => {
                    reasoning_parts.push(ANTHROPIC_REDACTED_THINKING_PLACEHOLDER.to_string());
                }
                _ => {}
            }
        }

        if !content_parts.is_empty() || !tool_calls.is_empty() {
            let mut message = json!({"role": role});

            if content_parts.is_empty() {
                message["content"] = Value::Null;
            } else if content_parts.len() == 1 {
                if let Some(text) = content_parts[0].get("text") {
                    message["content"] = text.clone();
                } else {
                    message["content"] = json!(content_parts);
                }
            } else {
                message["content"] = json!(content_parts);
            }

            if !tool_calls.is_empty() {
                message["tool_calls"] = json!(tool_calls);
            }

            if preserve_reasoning_content && role == "assistant" && !tool_calls.is_empty() {
                let reasoning_content = if reasoning_parts.is_empty() {
                    ANTHROPIC_TOOL_THINKING_PLACEHOLDER.to_string()
                } else {
                    reasoning_parts.join("\n")
                };
                message["reasoning_content"] = json!(reasoning_content);
            }

            result.push(message);
        }

        return result;
    }

    result.push(json!({"role": role, "content": content}));
    result
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

pub fn resolve_claude_api_format_from_settings(
    provider_type: Option<&str>,
    meta_api_format: Option<&str>,
    settings_config: &Value,
) -> &'static str {
    resolve_claude_api_format(
        provider_type,
        meta_api_format,
        settings_config
            .get("api_format")
            .and_then(Value::as_str),
        settings_config.get("openrouter_compat_mode"),
    )
}

pub fn resolve_claude_forward_api_format(
    configured_api_format: &str,
    is_copilot: bool,
    copilot_model_vendor: Option<&str>,
) -> String {
    if !is_copilot {
        return configured_api_format.to_string();
    }

    if copilot_model_vendor.is_some_and(|vendor| vendor.eq_ignore_ascii_case("openai")) {
        "openai_responses".to_string()
    } else {
        "openai_chat".to_string()
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

pub fn is_reasoning_vendor_identifier(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    REASONING_VENDOR_HINTS
        .iter()
        .any(|hint| value.contains(hint))
}

pub fn should_preserve_reasoning_content_for_openai_chat(
    settings_config: &Value,
    body: &Value,
) -> bool {
    body.get("model")
        .and_then(Value::as_str)
        .is_some_and(is_reasoning_vendor_identifier)
        || settings_config_reasoning_vendor_endpoint(settings_config)
}

pub fn should_normalize_anthropic_tool_thinking_history(
    settings_config: &Value,
    body: &Value,
    api_format: &str,
) -> bool {
    if api_format.trim() != "anthropic" {
        return false;
    }

    body.get("model")
        .and_then(Value::as_str)
        .is_some_and(is_reasoning_vendor_identifier)
        || settings_config_reasoning_vendor_endpoint(settings_config)
}

fn settings_config_reasoning_vendor_endpoint(settings_config: &Value) -> bool {
    settings_config_endpoint_candidates(settings_config)
    .into_iter()
    .flatten()
    .any(is_reasoning_vendor_identifier)
}

fn settings_config_endpoint_candidates(settings_config: &Value) -> [Option<&str>; 4] {
    [
        settings_config
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
            .and_then(Value::as_str),
        settings_config.get("base_url").and_then(Value::as_str),
        settings_config.get("baseURL").and_then(Value::as_str),
        settings_config.get("apiEndpoint").and_then(Value::as_str),
    ]
}

pub fn is_deepseek_official_anthropic_endpoint(settings_config: &Value) -> bool {
    settings_config_endpoint_candidates(settings_config)
        .into_iter()
        .flatten()
        .any(|url| url.trim_end_matches('/') == DEEPSEEK_OFFICIAL_ANTHROPIC_URL)
}

/// Normalize Anthropic-compatible tool-call history for providers that reject
/// assistant `tool_use` turns without a plain non-empty `thinking` block.
pub fn normalize_anthropic_tool_thinking_history(body: &mut Value) -> bool {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };

    let mut changed = false;

    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }

        let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };

        let has_tool_use = content
            .iter()
            .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"));
        if !has_tool_use {
            continue;
        }

        let mut has_thinking = false;

        for block in content.iter_mut() {
            match block.get("type").and_then(Value::as_str) {
                Some("thinking") => {
                    let has_non_empty_thinking = block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty());

                    if let Some(obj) = block.as_object_mut() {
                        if !has_non_empty_thinking {
                            obj.insert(
                                "thinking".to_string(),
                                json!(ANTHROPIC_TOOL_THINKING_PLACEHOLDER),
                            );
                            changed = true;
                        }
                        if obj.remove("signature").is_some() {
                            changed = true;
                        }
                    }

                    has_thinking = true;
                }
                Some("redacted_thinking") => {
                    *block = json!({
                        "type": "thinking",
                        "thinking": ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
                    });
                    has_thinking = true;
                    changed = true;
                }
                _ => {}
            }
        }

        if !has_thinking {
            content.insert(
                0,
                json!({
                    "type": "thinking",
                    "thinking": ANTHROPIC_TOOL_THINKING_PLACEHOLDER
                }),
            );
            changed = true;
        }
    }

    changed
}

pub fn normalize_deepseek_thinking_disabled_strip_effort(
    body: &mut Value,
    settings_config: &Value,
) -> bool {
    if !is_deepseek_official_anthropic_endpoint(settings_config) {
        return false;
    }

    let thinking_type = body
        .get("thinking")
        .and_then(|thinking| thinking.get("type"))
        .and_then(Value::as_str);

    if thinking_type != Some("disabled") {
        return false;
    }

    let mut changed = false;

    if let Some(output_config) = body
        .get_mut("output_config")
        .and_then(Value::as_object_mut)
    {
        changed |= output_config.remove("effort").is_some();
        if output_config.is_empty() {
            if let Some(body) = body.as_object_mut() {
                body.remove("output_config");
            }
        }
    }

    if let Some(body) = body.as_object_mut() {
        changed |= body.remove("reasoning_effort").is_some();
    }

    changed
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

pub fn openai_responses_to_anthropic_message(body: &Value) -> Result<Value, String> {
    let output = body
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| "No output in response".to_string())?;

    let mut content = Vec::new();
    let mut has_tool_use = false;

    for item in output {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");

        match item_type {
            "message" => {
                if let Some(msg_content) = item.get("content").and_then(Value::as_array) {
                    for block in msg_content {
                        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                        if block_type == "output_text" {
                            if let Some(text) = block.get("text").and_then(Value::as_str) {
                                if !text.is_empty() {
                                    content.push(json!({"type": "text", "text": text}));
                                }
                            }
                        } else if block_type == "refusal" {
                            if let Some(refusal) = block.get("refusal").and_then(Value::as_str) {
                                if !refusal.is_empty() {
                                    content.push(json!({"type": "text", "text": refusal}));
                                }
                            }
                        }
                    }
                }
            }
            "function_call" => {
                let call_id = item.get("call_id").and_then(Value::as_str).unwrap_or("");
                let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                let args_str = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                let input = sanitize_anthropic_tool_use_input(name, input);

                content.push(json!({
                    "type": "tool_use",
                    "id": call_id,
                    "name": name,
                    "input": input
                }));
                has_tool_use = true;
            }
            "reasoning" => {
                if let Some(summary) = item.get("summary").and_then(Value::as_array) {
                    let thinking_text = summary
                        .iter()
                        .filter_map(|summary_item| {
                            if summary_item.get("type").and_then(Value::as_str)
                                == Some("summary_text")
                            {
                                summary_item.get("text").and_then(Value::as_str)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("");

                    if !thinking_text.is_empty() {
                        content.push(json!({
                            "type": "thinking",
                            "thinking": thinking_text
                        }));
                    }
                }
            }
            _ => {}
        }
    }

    let stop_reason = map_openai_responses_stop_reason_to_anthropic(
        body.get("status").and_then(Value::as_str),
        has_tool_use,
        body.pointer("/incomplete_details/reason")
            .and_then(Value::as_str),
    );
    let usage_json = build_anthropic_usage_from_openai_responses(body.get("usage"));

    Ok(json!({
        "id": body.get("id").and_then(Value::as_str).unwrap_or(""),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": body.get("model").and_then(Value::as_str).unwrap_or(""),
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": usage_json
    }))
}

pub fn openai_chat_to_anthropic_message(body: &Value) -> Result<Value, String> {
    let choices = body
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| "No choices in response".to_string())?;
    let choice = choices
        .first()
        .ok_or_else(|| "Empty choices array".to_string())?;
    let message = choice
        .get("message")
        .ok_or_else(|| "No message in choice".to_string())?;

    let mut content = Vec::new();
    let mut has_tool_use = false;

    if let Some(reasoning_content) = message.get("reasoning_content").and_then(Value::as_str) {
        if !reasoning_content.is_empty() {
            content.push(json!({"type": "thinking", "thinking": reasoning_content}));
        }
    }

    if let Some(message_content) = message.get("content") {
        if let Some(text) = message_content.as_str() {
            if !text.is_empty() {
                content.push(json!({"type": "text", "text": text}));
            }
        } else if let Some(parts) = message_content.as_array() {
            for part in parts {
                let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
                match part_type {
                    "text" | "output_text" => {
                        if let Some(text) = part.get("text").and_then(Value::as_str) {
                            if !text.is_empty() {
                                content.push(json!({"type": "text", "text": text}));
                            }
                        }
                    }
                    "refusal" => {
                        if let Some(refusal) = part.get("refusal").and_then(Value::as_str) {
                            if !refusal.is_empty() {
                                content.push(json!({"type": "text", "text": refusal}));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) {
        if !refusal.is_empty() {
            content.push(json!({"type": "text", "text": refusal}));
        }
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        has_tool_use = !tool_calls.is_empty();
        for tool_call in tool_calls {
            let id = tool_call.get("id").and_then(Value::as_str).unwrap_or("");
            let empty_obj = json!({});
            let function = tool_call.get("function").unwrap_or(&empty_obj);
            let name = function.get("name").and_then(Value::as_str).unwrap_or("");
            let args_str = function
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));

            content.push(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input
            }));
        }
    }

    if !has_tool_use {
        if let Some(function_call) = message.get("function_call") {
            let id = function_call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let name = function_call
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let has_arguments = function_call.get("arguments").is_some();

            let input = match function_call.get("arguments") {
                Some(Value::String(value)) => serde_json::from_str(value).unwrap_or(json!({})),
                Some(value @ Value::Object(_)) | Some(value @ Value::Array(_)) => value.clone(),
                _ => json!({}),
            };

            if !name.is_empty() || has_arguments {
                content.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input
                }));
                has_tool_use = true;
            }
        }
    }

    let stop_reason = map_openai_chat_finish_reason_to_anthropic(
        choice.get("finish_reason").and_then(Value::as_str),
        has_tool_use,
    );
    let usage_json = build_anthropic_usage_from_openai_chat(body.get("usage"));

    Ok(json!({
        "id": body.get("id").and_then(Value::as_str).unwrap_or(""),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": body.get("model").and_then(Value::as_str).unwrap_or(""),
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": usage_json
    }))
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

pub fn chat_delta_reasoning_text(delta: &Value) -> Option<String> {
    extract_reasoning_field_text(delta)
}

pub fn leading_think_prefix_decision(buffer: &str) -> ThinkPrefixDecision {
    let trimmed = buffer.trim_start();
    if trimmed.is_empty() {
        return ThinkPrefixDecision::NeedMore;
    }

    if trimmed.starts_with("<think>") {
        return ThinkPrefixDecision::Reasoning;
    }

    if "<think>".starts_with(trimmed) {
        return ThinkPrefixDecision::NeedMore;
    }

    ThinkPrefixDecision::Text
}

pub fn extract_chat_sse_error(value: &Value) -> (String, Option<String>) {
    let error = value.get("error").unwrap_or(value);
    let message = error
        .as_str()
        .map(ToString::to_string)
        .or_else(|| {
            error
                .get("message")
                .or_else(|| error.get("detail"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| error.to_string());
    let error_type = error
        .get("type")
        .or_else(|| error.get("code"))
        .and_then(Value::as_str)
        .map(ToString::to_string);

    (message, error_type)
}

pub fn codex_chat_stream_response(
    response_id: &str,
    created_at: u64,
    status: &str,
    model: &str,
    output: Vec<Value>,
    latest_usage: Option<&Value>,
) -> Value {
    json!({
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": status,
        "model": model,
        "output": output,
        "usage": latest_usage.cloned().unwrap_or_else(|| {
            json!({
                "input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": 0,
                "output_tokens_details": { "reasoning_tokens": 0 }
            })
        })
    })
}

pub fn codex_chat_stream_started_events(response: Value) -> Vec<Bytes> {
    vec![
        sse_event(
            "response.created",
            json!({
                "type": "response.created",
                "response": response.clone()
            }),
        ),
        sse_event(
            "response.in_progress",
            json!({
                "type": "response.in_progress",
                "response": response
            }),
        ),
    ]
}

pub fn codex_chat_stream_completed_event(response: Value) -> Bytes {
    sse_event(
        "response.completed",
        json!({
            "type": "response.completed",
            "response": response
        }),
    )
}

pub fn codex_chat_stream_output_item_added_event(output_index: u32, item: Value) -> Bytes {
    sse_event(
        "response.output_item.added",
        json!({
            "type": "response.output_item.added",
            "output_index": output_index,
            "item": item
        }),
    )
}

pub fn codex_chat_stream_output_item_done_event(output_index: u32, item: Value) -> Bytes {
    sse_event(
        "response.output_item.done",
        json!({
            "type": "response.output_item.done",
            "output_index": output_index,
            "item": item
        }),
    )
}

pub fn codex_chat_stream_reasoning_in_progress_item(item_id: &str) -> Value {
    json!({
        "id": item_id,
        "type": "reasoning",
        "status": "in_progress",
        "summary": []
    })
}

pub fn codex_chat_stream_reasoning_completed_item(item_id: &str, text: &str) -> Value {
    json!({
        "id": item_id,
        "type": "reasoning",
        "summary": [{
            "type": "summary_text",
            "text": text
        }]
    })
}

pub fn codex_chat_stream_reasoning_summary_part_added_event(
    item_id: &str,
    output_index: u32,
) -> Bytes {
    sse_event(
        "response.reasoning_summary_part.added",
        json!({
            "type": "response.reasoning_summary_part.added",
            "item_id": item_id,
            "output_index": output_index,
            "summary_index": 0,
            "part": {
                "type": "summary_text",
                "text": ""
            }
        }),
    )
}

pub fn codex_chat_stream_reasoning_summary_text_delta_event(
    item_id: &str,
    output_index: u32,
    delta: &str,
) -> Bytes {
    sse_event(
        "response.reasoning_summary_text.delta",
        json!({
            "type": "response.reasoning_summary_text.delta",
            "item_id": item_id,
            "output_index": output_index,
            "summary_index": 0,
            "delta": delta
        }),
    )
}

pub fn codex_chat_stream_reasoning_summary_text_done_event(
    item_id: &str,
    output_index: u32,
    text: &str,
) -> Bytes {
    sse_event(
        "response.reasoning_summary_text.done",
        json!({
            "type": "response.reasoning_summary_text.done",
            "item_id": item_id,
            "output_index": output_index,
            "summary_index": 0,
            "text": text
        }),
    )
}

pub fn codex_chat_stream_reasoning_summary_part_done_event(
    item_id: &str,
    output_index: u32,
    text: &str,
) -> Bytes {
    sse_event(
        "response.reasoning_summary_part.done",
        json!({
            "type": "response.reasoning_summary_part.done",
            "item_id": item_id,
            "output_index": output_index,
            "summary_index": 0,
            "part": {
                "type": "summary_text",
                "text": text
            }
        }),
    )
}

pub fn codex_chat_stream_text_in_progress_item(item_id: &str) -> Value {
    json!({
        "id": item_id,
        "type": "message",
        "status": "in_progress",
        "role": "assistant",
        "content": []
    })
}

pub fn codex_chat_stream_text_completed_item(item_id: &str, text: &str) -> Value {
    json!({
        "id": item_id,
        "type": "message",
        "status": "completed",
        "role": "assistant",
        "content": [{
            "type": "output_text",
            "text": text,
            "annotations": []
        }]
    })
}

pub fn codex_chat_stream_content_part_added_event(item_id: &str, output_index: u32) -> Bytes {
    sse_event(
        "response.content_part.added",
        json!({
            "type": "response.content_part.added",
            "item_id": item_id,
            "output_index": output_index,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": "",
                "annotations": []
            }
        }),
    )
}

pub fn codex_chat_stream_output_text_delta_event(
    item_id: &str,
    output_index: u32,
    delta: &str,
) -> Bytes {
    sse_event(
        "response.output_text.delta",
        json!({
            "type": "response.output_text.delta",
            "item_id": item_id,
            "output_index": output_index,
            "content_index": 0,
            "delta": delta
        }),
    )
}

pub fn codex_chat_stream_output_text_done_event(
    item_id: &str,
    output_index: u32,
    text: &str,
) -> Bytes {
    sse_event(
        "response.output_text.done",
        json!({
            "type": "response.output_text.done",
            "item_id": item_id,
            "output_index": output_index,
            "content_index": 0,
            "text": text
        }),
    )
}

pub fn codex_chat_stream_content_part_done_event(
    item_id: &str,
    output_index: u32,
    text: &str,
) -> Bytes {
    sse_event(
        "response.content_part.done",
        json!({
            "type": "response.content_part.done",
            "item_id": item_id,
            "output_index": output_index,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": text,
                "annotations": []
            }
        }),
    )
}

pub fn codex_chat_stream_function_call_arguments_delta_event(
    item_id: &str,
    output_index: u32,
    delta: &str,
) -> Bytes {
    sse_event(
        "response.function_call_arguments.delta",
        json!({
            "type": "response.function_call_arguments.delta",
            "item_id": item_id,
            "output_index": output_index,
            "delta": delta
        }),
    )
}

pub fn codex_chat_stream_function_call_arguments_done_event(
    item_id: &str,
    output_index: u32,
    arguments: &str,
) -> Bytes {
    sse_event(
        "response.function_call_arguments.done",
        json!({
            "type": "response.function_call_arguments.done",
            "item_id": item_id,
            "output_index": output_index,
            "arguments": arguments
        }),
    )
}

pub fn codex_chat_stream_custom_tool_call_input_delta_event(
    item_id: &str,
    output_index: u32,
    delta: &str,
) -> Bytes {
    sse_event(
        "response.custom_tool_call_input.delta",
        json!({
            "type": "response.custom_tool_call_input.delta",
            "item_id": item_id,
            "output_index": output_index,
            "delta": delta
        }),
    )
}

pub fn codex_chat_stream_custom_tool_call_input_done_event(
    item_id: &str,
    output_index: u32,
    input: &str,
) -> Bytes {
    sse_event(
        "response.custom_tool_call_input.done",
        json!({
            "type": "response.custom_tool_call_input.done",
            "item_id": item_id,
            "output_index": output_index,
            "input": input
        }),
    )
}

pub fn codex_chat_stream_failed_event(
    mut response: Value,
    message: impl Into<String>,
    error_type: Option<&str>,
) -> Bytes {
    let mut error = json!({ "message": message.into() });
    if let Some(error_type) = error_type.filter(|value| !value.is_empty()) {
        error["type"] = json!(error_type);
    }
    response["error"] = error;

    sse_event(
        "response.failed",
        json!({
            "type": "response.failed",
            "response": response
        }),
    )
}

#[derive(Debug, Default)]
struct CodexChatStreamTextItemState {
    output_index: Option<u32>,
    item_id: String,
    text: String,
    added: bool,
    done: bool,
}

#[derive(Debug, Default)]
struct CodexChatStreamReasoningItemState {
    output_index: Option<u32>,
    item_id: String,
    text: String,
    added: bool,
    done: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum CodexChatStreamInlineThinkMode {
    #[default]
    Detecting,
    Reasoning,
    Text,
}

#[derive(Debug, Default)]
struct CodexChatStreamInlineThinkState {
    mode: CodexChatStreamInlineThinkMode,
    buffer: String,
}

#[derive(Debug, Default)]
struct CodexChatStreamToolCallState {
    output_index: Option<u32>,
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    reasoning_content: String,
    added: bool,
    done: bool,
}

#[derive(Debug)]
pub struct CodexChatToResponsesState {
    response_started: bool,
    completed: bool,
    response_id: String,
    model: String,
    created_at: u64,
    next_output_index: u32,
    text: CodexChatStreamTextItemState,
    reasoning: CodexChatStreamReasoningItemState,
    inline_think: CodexChatStreamInlineThinkState,
    tools: BTreeMap<usize, CodexChatStreamToolCallState>,
    output_items: Vec<(u32, Value)>,
    latest_usage: Option<Value>,
    finish_reason: Option<String>,
    tool_context: CodexToolContext,
}

struct CodexChatToResponsesSseStreamContext<S> {
    stream: Pin<Box<S>>,
    buffer: String,
    utf8_remainder: Vec<u8>,
    state: CodexChatToResponsesState,
    pending_events: VecDeque<Bytes>,
    finished: bool,
}

pub fn create_codex_chat_to_responses_sse_stream<S, E>(
    stream: S,
) -> impl Stream<Item = Result<Bytes, io::Error>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: Error + Send + 'static,
{
    create_codex_chat_to_responses_sse_stream_with_context(stream, CodexToolContext::default())
}

/// Convert an OpenAI Chat Completions SSE byte stream into Codex Responses SSE bytes.
///
/// The transport wrapper owns UTF-8 safe byte buffering, SSE block/data parsing,
/// upstream error bridging, and end-of-stream finalization. Protocol conversion
/// remains in `CodexChatToResponsesState`.
pub fn create_codex_chat_to_responses_sse_stream_with_context<S, E>(
    stream: S,
    tool_context: CodexToolContext,
) -> impl Stream<Item = Result<Bytes, io::Error>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: Error + Send + 'static,
{
    let context = CodexChatToResponsesSseStreamContext {
        stream: Box::pin(stream),
        buffer: String::new(),
        utf8_remainder: Vec::new(),
        state: CodexChatToResponsesState::with_tool_context(tool_context),
        pending_events: VecDeque::new(),
        finished: false,
    };

    futures_stream::unfold(context, |mut context| async move {
        loop {
            if let Some(event) = context.pending_events.pop_front() {
                return Some((Ok(event), context));
            }

            if context.finished {
                return None;
            }

            match context.stream.as_mut().next().await {
                Some(Ok(bytes)) => {
                    crate::append_utf8_safe(
                        &mut context.buffer,
                        &mut context.utf8_remainder,
                        &bytes,
                    );

                    while let Some(block) = crate::take_sse_block(&mut context.buffer) {
                        if block.trim().is_empty() {
                            continue;
                        }

                        let mut event_name: Option<String> = None;
                        let mut data_parts: Vec<String> = Vec::new();
                        for line in block.lines() {
                            if let Some(event) = crate::strip_sse_field(line, "event") {
                                event_name = Some(event.trim().to_string());
                            }
                            if let Some(data) = crate::strip_sse_field(line, "data") {
                                data_parts.push(data.to_string());
                            }
                        }

                        if data_parts.is_empty() {
                            continue;
                        }

                        let data = data_parts.join("\n");
                        if data.trim() == "[DONE]" {
                            context.pending_events.extend(context.state.finalize());
                            continue;
                        }

                        let Ok(chunk) = serde_json::from_str::<Value>(&data) else {
                            continue;
                        };

                        if event_name.as_deref() == Some("error") || chunk.get("error").is_some() {
                            let (message, error_type) = extract_chat_sse_error(&chunk);
                            context
                                .pending_events
                                .push_back(context.state.failed_event(message, error_type));
                            context.finished = true;
                            break;
                        }

                        context
                            .pending_events
                            .extend(context.state.handle_chat_chunk(&chunk));
                    }
                }
                Some(Err(error)) => {
                    context.pending_events.push_back(context.state.failed_event(
                        format!("Stream error: {error}"),
                        Some("stream_error".to_string()),
                    ));
                    context.finished = true;
                }
                None => {
                    context.finished = true;
                    if context.state.is_completed() || context.state.has_finish_reason() {
                        context.pending_events.extend(context.state.finalize());
                    } else if context.state.has_substantive_output() {
                        context.state.set_finish_reason("length");
                        context.pending_events.extend(context.state.finalize());
                    } else {
                        context.pending_events.push_back(context.state.failed_event(
                            "Upstream Chat Completions stream ended before sending finish_reason"
                                .to_string(),
                            Some("stream_truncated".to_string()),
                        ));
                    }
                }
            }
        }
    })
}

impl Default for CodexChatToResponsesState {
    fn default() -> Self {
        Self {
            response_started: false,
            completed: false,
            response_id: "resp_ccswitch".to_string(),
            model: String::new(),
            created_at: 0,
            next_output_index: 0,
            text: CodexChatStreamTextItemState::default(),
            reasoning: CodexChatStreamReasoningItemState::default(),
            inline_think: CodexChatStreamInlineThinkState::default(),
            tools: BTreeMap::new(),
            output_items: Vec::new(),
            latest_usage: None,
            finish_reason: None,
            tool_context: CodexToolContext::default(),
        }
    }
}

impl CodexChatToResponsesState {
    pub fn with_tool_context(tool_context: CodexToolContext) -> Self {
        Self {
            tool_context,
            ..Self::default()
        }
    }

    pub fn handle_chat_chunk(&mut self, chunk: &Value) -> Vec<Bytes> {
        let mut events = Vec::new();

        if let Some(id) = chunk.get("id").and_then(|v| v.as_str()) {
            self.response_id = response_id_from_chat_id(Some(id));
        }
        if let Some(model) = chunk.get("model").and_then(|v| v.as_str()) {
            if !model.is_empty() {
                self.model = model.to_string();
            }
        }
        if let Some(created) = chunk.get("created").and_then(|v| v.as_u64()) {
            self.created_at = created;
        }

        events.extend(self.ensure_response_started());

        if let Some(usage) = chunk.get("usage").filter(|v| !v.is_null()) {
            self.latest_usage = Some(chat_usage_to_responses_usage(Some(usage)));
        }

        let Some(choice) = chunk
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|choices| choices.first())
        else {
            return events;
        };

        if let Some(delta) = choice.get("delta") {
            if let Some(reasoning) = chat_delta_reasoning_text(delta) {
                events.extend(self.push_reasoning_delta(&reasoning));
                self.append_reasoning_to_active_tools(&reasoning);
            }

            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                if !content.is_empty() {
                    events.extend(self.push_content_delta(content));
                }
            }

            if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                events.extend(self.flush_inline_think_at_boundary());
                let reasoning_for_tool_call = self.current_reasoning_text();
                events.extend(self.finalize_reasoning());
                for tool_call in tool_calls {
                    events.extend(
                        self.push_tool_call_delta(tool_call, reasoning_for_tool_call.as_deref()),
                    );
                }
            }
        }

        if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
            self.finish_reason = Some(finish_reason.to_string());
        }

        events
    }

    pub fn finalize(&mut self) -> Vec<Bytes> {
        if self.completed {
            return Vec::new();
        }

        let mut events = self.ensure_response_started();
        events.extend(self.flush_inline_think_at_boundary());
        events.extend(self.finalize_reasoning());
        events.extend(self.finalize_text());
        events.extend(self.finalize_tools());

        let status = response_status_from_finish_reason(self.finish_reason.as_deref());
        let mut response = self.base_response(status, self.completed_output_items());
        if status == "incomplete" {
            response["incomplete_details"] = json!({ "reason": "max_output_tokens" });
        }

        events.push(codex_chat_stream_completed_event(response));
        self.completed = true;
        events
    }

    pub fn failed_event(&mut self, message: String, error_type: Option<String>) -> Bytes {
        self.completed = true;
        let response = self.base_response("failed", self.completed_output_items());
        codex_chat_stream_failed_event(response, message, error_type.as_deref())
    }

    pub fn has_substantive_output(&self) -> bool {
        !self.text.text.trim().is_empty()
            || !self.reasoning.text.trim().is_empty()
            || !self.inline_think.buffer.trim().is_empty()
            || !self.output_items.is_empty()
            || self.tools.values().any(|state| {
                state.added
                    || !state.call_id.trim().is_empty()
                    || !state.name.trim().is_empty()
                    || !state.arguments.trim().is_empty()
                    || !state.reasoning_content.trim().is_empty()
            })
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }

    pub fn has_finish_reason(&self) -> bool {
        self.finish_reason.is_some()
    }

    pub fn set_finish_reason(&mut self, finish_reason: impl Into<String>) {
        self.finish_reason = Some(finish_reason.into());
    }

    fn push_content_delta(&mut self, delta: &str) -> Vec<Bytes> {
        match self.inline_think.mode {
            CodexChatStreamInlineThinkMode::Text => {
                let mut events = self.finalize_reasoning();
                events.extend(self.push_text_delta(delta));
                events
            }
            CodexChatStreamInlineThinkMode::Detecting => {
                self.inline_think.buffer.push_str(delta);
                match leading_think_prefix_decision(&self.inline_think.buffer) {
                    ThinkPrefixDecision::NeedMore => Vec::new(),
                    ThinkPrefixDecision::Reasoning => {
                        self.inline_think.mode = CodexChatStreamInlineThinkMode::Reasoning;
                        self.drain_complete_inline_think()
                    }
                    ThinkPrefixDecision::Text => {
                        self.inline_think.mode = CodexChatStreamInlineThinkMode::Text;
                        let text = std::mem::take(&mut self.inline_think.buffer);
                        let mut events = self.finalize_reasoning();
                        events.extend(self.push_text_delta(&text));
                        events
                    }
                }
            }
            CodexChatStreamInlineThinkMode::Reasoning => {
                self.inline_think.buffer.push_str(delta);
                self.drain_complete_inline_think()
            }
        }
    }

    fn drain_complete_inline_think(&mut self) -> Vec<Bytes> {
        let Some((reasoning, answer)) = split_leading_think_block(&self.inline_think.buffer) else {
            return Vec::new();
        };

        self.inline_think.mode = CodexChatStreamInlineThinkMode::Text;
        self.inline_think.buffer.clear();

        let mut events = Vec::new();
        if !reasoning.is_empty() {
            events.extend(self.push_reasoning_delta(&reasoning));
            events.extend(self.finalize_reasoning());
        }
        if !answer.is_empty() {
            events.extend(self.push_text_delta(&answer));
        }

        events
    }

    fn flush_inline_think_at_boundary(&mut self) -> Vec<Bytes> {
        match self.inline_think.mode {
            CodexChatStreamInlineThinkMode::Text => Vec::new(),
            CodexChatStreamInlineThinkMode::Detecting => {
                self.inline_think.mode = CodexChatStreamInlineThinkMode::Text;
                let text = std::mem::take(&mut self.inline_think.buffer);
                if text.is_empty() {
                    Vec::new()
                } else {
                    let mut events = self.finalize_reasoning();
                    events.extend(self.push_text_delta(&text));
                    events
                }
            }
            CodexChatStreamInlineThinkMode::Reasoning => {
                let buffered = std::mem::take(&mut self.inline_think.buffer);
                self.inline_think.mode = CodexChatStreamInlineThinkMode::Text;
                if let Some((reasoning, answer)) = split_leading_think_block(&buffered) {
                    let mut events = Vec::new();
                    if !reasoning.is_empty() {
                        events.extend(self.push_reasoning_delta(&reasoning));
                        events.extend(self.finalize_reasoning());
                    }
                    if !answer.is_empty() {
                        events.extend(self.push_text_delta(&answer));
                    }
                    return events;
                }

                let reasoning = strip_leading_think_open_tag(&buffered).unwrap_or(buffered);
                if reasoning.is_empty() {
                    Vec::new()
                } else {
                    let mut events = self.push_reasoning_delta(&reasoning);
                    events.extend(self.finalize_reasoning());
                    events
                }
            }
        }
    }

    fn ensure_response_started(&mut self) -> Vec<Bytes> {
        if self.response_started {
            return Vec::new();
        }

        self.response_started = true;
        let response = self.base_response("in_progress", Vec::new());

        codex_chat_stream_started_events(response)
    }

    fn push_reasoning_delta(&mut self, delta: &str) -> Vec<Bytes> {
        let mut events = Vec::new();

        if !self.reasoning.added {
            let output_index = self.next_output_index();
            let item_id = format!("rs_{}", self.response_id);
            self.reasoning.output_index = Some(output_index);
            self.reasoning.item_id = item_id.clone();
            self.reasoning.added = true;

            events.push(codex_chat_stream_output_item_added_event(
                output_index,
                codex_chat_stream_reasoning_in_progress_item(&item_id),
            ));
            events.push(codex_chat_stream_reasoning_summary_part_added_event(
                &self.reasoning.item_id,
                output_index,
            ));
        }

        self.reasoning.text.push_str(delta);
        let output_index = self.reasoning.output_index.unwrap_or(0);
        events.push(codex_chat_stream_reasoning_summary_text_delta_event(
            &self.reasoning.item_id,
            output_index,
            delta,
        ));

        events
    }

    fn push_text_delta(&mut self, delta: &str) -> Vec<Bytes> {
        let mut events = Vec::new();

        if !self.text.added {
            let output_index = self.next_output_index();
            let item_id = format!("{}_msg", self.response_id);
            self.text.output_index = Some(output_index);
            self.text.item_id = item_id.clone();
            self.text.added = true;

            events.push(codex_chat_stream_output_item_added_event(
                output_index,
                codex_chat_stream_text_in_progress_item(&item_id),
            ));
            events.push(codex_chat_stream_content_part_added_event(
                &self.text.item_id,
                output_index,
            ));
        }

        self.text.text.push_str(delta);
        let output_index = self.text.output_index.unwrap_or(0);
        events.push(codex_chat_stream_output_text_delta_event(
            &self.text.item_id,
            output_index,
            delta,
        ));

        events
    }

    fn current_reasoning_text(&self) -> Option<String> {
        (!self.reasoning.text.trim().is_empty()).then(|| self.reasoning.text.trim().to_string())
    }

    fn push_tool_call_delta(&mut self, tool_call: &Value, reasoning: Option<&str>) -> Vec<Bytes> {
        let chat_index = tool_call.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let id_delta = tool_call
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let function = tool_call.get("function").unwrap_or(&Value::Null);
        let name_delta = function
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let args_delta = function
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut should_add = false;
        let mut output_index = None;
        let mut item_id = String::new();
        let mut pending_arguments = String::new();
        let current_name: String;

        {
            let state = self.tools.entry(chat_index).or_default();
            if let Some(id) = id_delta {
                state.call_id = id;
            }
            if let Some(name) = name_delta {
                state.name = name;
            }
            if !args_delta.is_empty() {
                state.arguments.push_str(&args_delta);
            }
            if state.reasoning_content.is_empty() {
                if let Some(reasoning) = reasoning.map(str::trim).filter(|value| !value.is_empty())
                {
                    state.reasoning_content = reasoning.to_string();
                }
            }

            if !state.added && (!state.call_id.is_empty() || !state.name.is_empty()) {
                should_add = true;
                pending_arguments = state.arguments.clone();
            } else if state.added {
                output_index = state.output_index;
                item_id = state.item_id.clone();
            }
            current_name = state.name.clone();
        }

        let is_custom_tool = self.tool_context.is_custom_tool_chat_name(&current_name);
        let mut events = Vec::new();

        if should_add {
            let assigned = self.next_output_index();
            let Some(state) = self.tools.get_mut(&chat_index) else {
                return events;
            };
            state.added = true;
            if state.call_id.is_empty() {
                state.call_id = format!("call_{chat_index}");
            }
            if state.name.is_empty() {
                state.name = "unknown_tool".to_string();
            }
            state.output_index = Some(assigned);
            let is_custom_tool = self.tool_context.is_custom_tool_chat_name(&state.name);
            state.item_id = response_tool_call_item_id_from_chat_name(
                &state.call_id,
                &state.name,
                &self.tool_context,
            );
            item_id = state.item_id.clone();

            let item = response_tool_call_item_from_chat_name(
                &item_id,
                "in_progress",
                &state.call_id,
                &state.name,
                "",
                Some(&state.reasoning_content),
                &self.tool_context,
            );

            events.push(codex_chat_stream_output_item_added_event(assigned, item));

            if !pending_arguments.is_empty() && !is_custom_tool {
                events.push(codex_chat_stream_function_call_arguments_delta_event(
                    &state.item_id,
                    assigned,
                    &pending_arguments,
                ));
            }
        } else if !args_delta.is_empty() && !is_custom_tool {
            if let Some(output_index) = output_index {
                events.push(codex_chat_stream_function_call_arguments_delta_event(
                    &item_id,
                    output_index,
                    &args_delta,
                ));
            }
        }

        events
    }

    fn append_reasoning_to_active_tools(&mut self, delta: &str) {
        if delta.trim().is_empty() {
            return;
        }

        for state in self.tools.values_mut().filter(|state| !state.done) {
            if state.reasoning_content.is_empty() {
                state.reasoning_content = delta.trim_start().to_string();
            } else {
                state.reasoning_content.push_str(delta);
            }
        }
    }

    fn finalize_reasoning(&mut self) -> Vec<Bytes> {
        if !self.reasoning.added || self.reasoning.done {
            return Vec::new();
        }

        let output_index = self.reasoning.output_index.unwrap_or(0);
        let item_id = self.reasoning.item_id.clone();
        let text = self.reasoning.text.clone();
        let item = codex_chat_stream_reasoning_completed_item(&item_id, &text);
        self.output_items.push((output_index, item.clone()));
        self.reasoning.done = true;

        vec![
            codex_chat_stream_reasoning_summary_text_done_event(
                &self.reasoning.item_id,
                output_index,
                &self.reasoning.text,
            ),
            codex_chat_stream_reasoning_summary_part_done_event(
                &self.reasoning.item_id,
                output_index,
                &self.reasoning.text,
            ),
            codex_chat_stream_output_item_done_event(output_index, item),
        ]
    }

    fn finalize_text(&mut self) -> Vec<Bytes> {
        if !self.text.added || self.text.done {
            return Vec::new();
        }

        let output_index = self.text.output_index.unwrap_or(0);
        let item = codex_chat_stream_text_completed_item(&self.text.item_id, &self.text.text);
        self.output_items.push((output_index, item.clone()));
        self.text.done = true;

        vec![
            codex_chat_stream_output_text_done_event(
                &self.text.item_id,
                output_index,
                &self.text.text,
            ),
            codex_chat_stream_content_part_done_event(
                &self.text.item_id,
                output_index,
                &self.text.text,
            ),
            codex_chat_stream_output_item_done_event(output_index, item),
        ]
    }

    fn finalize_tools(&mut self) -> Vec<Bytes> {
        let mut events = Vec::new();
        let keys: Vec<usize> = self.tools.keys().copied().collect();

        for key in keys {
            let mut add_event: Option<Bytes> = None;
            if self.tools.get(&key).map(|state| state.done).unwrap_or(true) {
                continue;
            }

            if self
                .tools
                .get(&key)
                .map(|state| !state.added && !state.done)
                .unwrap_or(false)
            {
                let assigned = self.next_output_index();
                let Some(state) = self.tools.get_mut(&key) else {
                    continue;
                };
                state.added = true;
                if state.call_id.is_empty() {
                    state.call_id = format!("call_{key}");
                }
                if state.name.is_empty() {
                    state.name = "unknown_tool".to_string();
                }
                state.output_index = Some(assigned);
                state.item_id = response_tool_call_item_id_from_chat_name(
                    &state.call_id,
                    &state.name,
                    &self.tool_context,
                );
                let item = response_tool_call_item_from_chat_name(
                    &state.item_id,
                    "in_progress",
                    &state.call_id,
                    &state.name,
                    "",
                    Some(&state.reasoning_content),
                    &self.tool_context,
                );
                add_event = Some(codex_chat_stream_output_item_added_event(assigned, item));
            }

            if let Some(event) = add_event {
                events.push(event);
            }

            let Some(state) = self.tools.get_mut(&key) else {
                continue;
            };
            let output_index = state.output_index.unwrap_or(0);
            let arguments = canonicalize_tool_arguments_str(&state.arguments);
            let is_custom_tool = self.tool_context.is_custom_tool_chat_name(&state.name);
            let item = response_tool_call_item_from_chat_name(
                &state.item_id,
                "completed",
                &state.call_id,
                &state.name,
                &arguments,
                Some(&state.reasoning_content),
                &self.tool_context,
            );
            state.done = true;
            self.output_items.push((output_index, item.clone()));

            if is_custom_tool {
                let input = custom_tool_input_from_chat_arguments(&arguments);
                if !input.is_empty() {
                    events.push(codex_chat_stream_custom_tool_call_input_delta_event(
                        &state.item_id,
                        output_index,
                        &input,
                    ));
                }
                events.push(codex_chat_stream_custom_tool_call_input_done_event(
                    &state.item_id,
                    output_index,
                    &input,
                ));
            } else {
                events.push(codex_chat_stream_function_call_arguments_done_event(
                    &state.item_id,
                    output_index,
                    &arguments,
                ));
            }
            events.push(codex_chat_stream_output_item_done_event(output_index, item));
        }

        events
    }

    fn completed_output_items(&self) -> Vec<Value> {
        let mut output_items = self.output_items.clone();
        output_items.sort_by_key(|(output_index, _)| *output_index);
        output_items
            .into_iter()
            .map(|(_, item)| item)
            .collect::<Vec<_>>()
    }

    fn base_response(&self, status: &str, output: Vec<Value>) -> Value {
        codex_chat_stream_response(
            &self.response_id,
            self.created_at,
            status,
            &self.model,
            output,
            self.latest_usage.as_ref(),
        )
    }

    fn next_output_index(&mut self) -> u32 {
        let index = self.next_output_index;
        self.next_output_index += 1;
        index
    }
}

pub fn sse_event(event: &str, data: Value) -> Bytes {
    Bytes::from(format!(
        "event: {event}\ndata: {}\n\n",
        serde_json::to_string(&data).unwrap_or_default()
    ))
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
    fn codex_chat_reasoning_profile_normalizes_effort_as_thinking_support() {
        let profile = normalize_codex_chat_reasoning_profile(CodexChatReasoningProfile {
            supports_effort: Some(true),
            ..Default::default()
        });

        assert_eq!(profile.supports_thinking, Some(true));
    }

    #[test]
    fn codex_chat_reasoning_options_project_from_profile() {
        let profile = CodexChatReasoningProfile {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".to_string()),
            effort_param: Some("reasoning_effort".to_string()),
            effort_value_mode: Some("deepseek".to_string()),
            output_format: Some("reasoning_content".to_string()),
        };

        let options = CodexChatReasoningOptions::from_profile(&profile);

        assert_eq!(options.supports_thinking, Some(true));
        assert_eq!(options.supports_effort, Some(true));
        assert_eq!(options.thinking_param.as_deref(), Some("thinking"));
        assert_eq!(options.effort_param.as_deref(), Some("reasoning_effort"));
        assert_eq!(options.effort_value_mode.as_deref(), Some("deepseek"));
    }

    #[test]
    fn codex_chat_reasoning_profile_infers_deepseek_effort_support() {
        let profile = infer_codex_chat_reasoning_profile(
            "DeepSeek",
            "https://api.deepseek.com",
            "deepseek-v4-pro",
        )
        .unwrap();

        assert_eq!(profile.supports_thinking, Some(true));
        assert_eq!(profile.supports_effort, Some(true));
        assert_eq!(profile.effort_value_mode.as_deref(), Some("deepseek"));
        assert_eq!(profile.output_format.as_deref(), Some("reasoning_content"));
    }

    #[test]
    fn codex_chat_reasoning_profile_prefers_openrouter_platform_over_model_vendor() {
        let profile = infer_codex_chat_reasoning_profile(
            "OpenRouter",
            "https://openrouter.ai/api/v1",
            "deepseek/deepseek-chat-v3.1",
        )
        .unwrap();

        assert_eq!(profile.thinking_param.as_deref(), Some("none"));
        assert_eq!(profile.effort_param.as_deref(), Some("reasoning.effort"));
        assert_eq!(profile.effort_value_mode.as_deref(), Some("openrouter"));
        assert_eq!(profile.supports_effort, Some(true));
    }

    #[test]
    fn codex_chat_reasoning_profile_prefers_siliconflow_platform_over_model_vendor() {
        let profile = infer_codex_chat_reasoning_profile(
            "SiliconFlow",
            "https://api.siliconflow.cn/v1",
            "MiniMaxAI/MiniMax-M2.7",
        )
        .unwrap();

        assert_eq!(profile.thinking_param.as_deref(), Some("enable_thinking"));
        assert_eq!(profile.supports_effort, Some(false));
        assert_eq!(profile.output_format.as_deref(), Some("reasoning_content"));
    }

    #[test]
    fn codex_oauth_responses_contract_sets_store_include_and_fast_tier() {
        let source = json!({
            "include": ["existing"]
        });
        let mut result = json!({
            "model": "gpt-5"
        });

        apply_codex_oauth_responses_request_contract(&mut result, &source, true);

        assert_eq!(result["store"], false);
        assert_eq!(result["service_tier"], "priority");
        assert_eq!(
            result["include"],
            json!(["existing", "reasoning.encrypted_content"])
        );
    }

    #[test]
    fn codex_oauth_responses_contract_deduplicates_reasoning_include() {
        let source = json!({
            "include": ["reasoning.encrypted_content"]
        });
        let mut result = json!({});

        apply_codex_oauth_responses_request_contract(&mut result, &source, false);

        assert_eq!(result["include"], json!(["reasoning.encrypted_content"]));
        assert!(result.get("service_tier").is_none());
    }

    #[test]
    fn codex_oauth_responses_contract_strips_unsupported_fields_and_defaults_required() {
        let source = json!({});
        let mut result = json!({
            "max_output_tokens": 1024,
            "temperature": 0.2,
            "top_p": 0.9,
            "stream": false
        });

        apply_codex_oauth_responses_request_contract(&mut result, &source, false);

        assert!(result.get("max_output_tokens").is_none());
        assert!(result.get("temperature").is_none());
        assert!(result.get("top_p").is_none());
        assert_eq!(result["instructions"], "");
        assert_eq!(result["tools"], json!([]));
        assert_eq!(result["parallel_tool_calls"], false);
        assert_eq!(result["stream"], true);
    }

    #[test]
    fn claude_responses_prompt_cache_key_prefers_explicit_key() {
        let body = json!({
            "metadata": {
                "user_id": "user_session_from_metadata",
                "session_id": "metadata-session"
            }
        });

        let resolved = resolve_claude_responses_prompt_cache_key(
            &body,
            Some(" explicit-cache "),
            Some("session-123"),
            true,
        );

        assert_eq!(resolved.key.as_deref(), Some("explicit-cache"));
        assert_eq!(resolved.source, ClaudePromptCacheKeySource::Explicit);
    }

    #[test]
    fn claude_responses_prompt_cache_key_uses_non_copilot_session() {
        let body = json!({
            "metadata": {
                "user_id": "user_session_ignored",
                "session_id": "also-ignored"
            }
        });

        let resolved =
            resolve_claude_responses_prompt_cache_key(&body, None, Some(" session-123 "), false);

        assert_eq!(resolved.key.as_deref(), Some("session-123"));
        assert_eq!(resolved.source, ClaudePromptCacheKeySource::Session);
    }

    #[test]
    fn claude_responses_prompt_cache_key_uses_copilot_user_id_session() {
        let body = json!({
            "metadata": {
                "user_id": "user_42_session_metadata-session",
                "session_id": "fallback-session"
            }
        });

        let resolved =
            resolve_claude_responses_prompt_cache_key(&body, None, Some("ignored"), true);

        assert_eq!(resolved.key.as_deref(), Some("metadata-session"));
        assert_eq!(resolved.source, ClaudePromptCacheKeySource::Session);
    }

    #[test]
    fn claude_responses_prompt_cache_key_falls_back_to_copilot_metadata_session_id() {
        let body = json!({
            "metadata": {
                "user_id": "without-marker",
                "session_id": " metadata-session "
            }
        });

        let resolved =
            resolve_claude_responses_prompt_cache_key(&body, None, Some("ignored"), true);

        assert_eq!(resolved.key.as_deref(), Some("metadata-session"));
        assert_eq!(resolved.source, ClaudePromptCacheKeySource::Session);

        let missing = resolve_claude_responses_prompt_cache_key(&json!({}), None, None, true);
        assert_eq!(missing.key, None);
        assert_eq!(missing.source, ClaudePromptCacheKeySource::None);
    }

    #[test]
    fn copilot_prompt_cache_provider_uses_legacy_host_detection_inputs() {
        assert!(is_copilot_prompt_cache_provider(
            Some("github_copilot"),
            &json!({})
        ));
        assert!(is_copilot_prompt_cache_provider(
            None,
            &json!({"baseUrl": "https://api.githubcopilot.com"})
        ));

        assert!(!is_copilot_prompt_cache_provider(
            Some("github-copilot"),
            &json!({})
        ));
        assert!(!is_copilot_prompt_cache_provider(
            None,
            &json!({"base_url": "https://api.githubcopilot.com"})
        ));
    }

    #[test]
    fn converts_anthropic_message_to_openai_responses_request() {
        let input = json!({
            "model": "gpt-4o",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": "Hello"}],
            "temperature": 0.2,
            "top_p": 0.9
        });

        let result = anthropic_to_openai_responses_request(&input, None, false, false);

        assert_eq!(result["model"], "gpt-4o");
        assert_eq!(result["max_output_tokens"], 1024);
        assert_eq!(result["input"][0]["role"], "user");
        assert_eq!(result["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(result["input"][0]["content"][0]["text"], "Hello");
        assert_eq!(result["temperature"], 0.2);
        assert_eq!(result["top_p"], 0.9);
        assert!(result.get("stop_sequences").is_none());
    }

    #[test]
    fn converts_anthropic_tool_use_and_tool_result_to_openai_responses_items() {
        let input = json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "Calling tool"},
                        {"type": "tool_use", "id": "call_1", "name": "get_weather", "input": {"location": "Tokyo"}}
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {"type": "tool_result", "tool_use_id": "call_1", "content": {"temperature": 22}}
                    ]
                }
            ]
        });

        let result = anthropic_to_openai_responses_request(&input, None, false, false);

        assert_eq!(result["input"][0]["role"], "assistant");
        assert_eq!(result["input"][1]["type"], "function_call");
        assert_eq!(result["input"][1]["call_id"], "call_1");
        assert_eq!(result["input"][1]["name"], "get_weather");
        assert_eq!(result["input"][1]["arguments"], "{\"location\":\"Tokyo\"}");
        assert_eq!(result["input"][2]["type"], "function_call_output");
        assert_eq!(result["input"][2]["call_id"], "call_1");
        assert_eq!(result["input"][2]["output"], "{\"temperature\":22}");
    }

    #[test]
    fn converts_anthropic_image_and_tools_to_openai_responses_request() {
        let input = json!({
            "model": "gpt-4o",
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "image",
                    "source": {"media_type": "image/jpeg", "data": "abc123"}
                }]
            }],
            "tools": [{
                "type": "custom",
                "name": "ignored"
            }, {
                "type": "function",
                "name": "search",
                "description": "Search",
                "input_schema": {"type": "object", "properties": {"url": {"type": "string", "format": "uri"}}}
            }]
        });

        let result = anthropic_to_openai_responses_request(&input, Some("cache-key"), false, false);

        assert_eq!(
            result["input"][0]["content"][0]["image_url"],
            "data:image/jpeg;base64,abc123"
        );
        assert_eq!(result["tools"][0]["name"], "ignored");
        assert_eq!(result["tools"][1]["name"], "search");
        assert!(result["tools"][1]["parameters"]["properties"]["url"]
            .get("format")
            .is_none());
        assert_eq!(result["prompt_cache_key"], "cache-key");
    }

    #[test]
    fn converts_anthropic_to_openai_responses_with_codex_oauth_contract() {
        let input = json!({
            "model": "gpt-5",
            "max_tokens": 1024,
            "temperature": 0.2,
            "top_p": 0.9,
            "stream": false,
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = anthropic_to_openai_responses_request(&input, None, true, true);

        assert_eq!(result["store"], false);
        assert_eq!(result["service_tier"], "priority");
        assert_eq!(result["include"], json!(["reasoning.encrypted_content"]));
        assert!(result.get("max_output_tokens").is_none());
        assert!(result.get("temperature").is_none());
        assert!(result.get("top_p").is_none());
        assert_eq!(result["stream"], true);
        assert_eq!(result["tools"], json!([]));
        assert_eq!(result["parallel_tool_calls"], false);
    }

    #[test]
    fn converts_anthropic_message_to_openai_chat_request() {
        let input = json!({
            "model": "o3-mini",
            "max_tokens": 1024,
            "temperature": 0.2,
            "top_p": 0.9,
            "stop_sequences": ["stop"],
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = anthropic_to_openai_chat_request(&input, false);

        assert_eq!(result["model"], "o3-mini");
        assert_eq!(result["max_completion_tokens"], 1024);
        assert!(result.get("max_tokens").is_none());
        assert_eq!(result["messages"][0], json!({"role": "user", "content": "Hello"}));
        assert_eq!(result["temperature"], 0.2);
        assert_eq!(result["top_p"], 0.9);
        assert_eq!(result["stop"], json!(["stop"]));
    }

    #[test]
    fn converts_anthropic_system_array_to_single_openai_chat_system_message() {
        let input = json!({
            "model": "gpt-4o",
            "system": [
                {"type": "text", "text": "First"},
                {"type": "text", "text": "Second", "cache_control": {"type": "ephemeral"}}
            ],
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = anthropic_to_openai_chat_request(&input, false);

        assert_eq!(
            result["messages"][0],
            json!({"role": "system", "content": "First\nSecond"})
        );
        assert_eq!(result["messages"][1]["role"], "user");
    }

    #[test]
    fn converts_anthropic_chat_request_strips_billing_header_and_cache_control() {
        let input = json!({
            "model": "glm-5.1",
            "max_tokens": 1024,
            "system": [
                {"type": "text", "text": "x-anthropic-billing-header: cch=a7754;\n\nStable prompt", "cache_control": {"type": "ephemeral"}}
            ],
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Hello", "cache_control": {"type": "ephemeral", "ttl": "5m"}}
                ]
            }],
            "tools": [{
                "name": "search",
                "description": "Search the web",
                "input_schema": {"type": "object"},
                "cache_control": {"type": "ephemeral"}
            }]
        });

        let result = anthropic_to_openai_chat_request(&input, false);

        assert_eq!(result["messages"][0]["content"], "Stable prompt");
        assert!(result["messages"][0].get("cache_control").is_none());
        assert_eq!(result["messages"][1]["content"], "Hello");
        assert!(result["messages"][1].get("cache_control").is_none());
        assert!(result["tools"][0].get("cache_control").is_none());
        assert!(result.get("prompt_cache_key").is_none());
    }

    #[test]
    fn converts_anthropic_tool_use_and_result_to_openai_chat_messages() {
        let input = json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "Calling"},
                        {"type": "tool_use", "id": "call_1", "name": "search", "input": {"query": "rust"}}
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {"type": "tool_result", "tool_use_id": "call_1", "content": {"ok": true}}
                    ]
                }
            ]
        });

        let result = anthropic_to_openai_chat_request(&input, false);

        assert_eq!(result["messages"][0]["role"], "assistant");
        assert_eq!(result["messages"][0]["content"], "Calling");
        assert_eq!(result["messages"][0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(
            result["messages"][0]["tool_calls"][0]["function"]["arguments"],
            "{\"query\":\"rust\"}"
        );
        assert_eq!(result["messages"][1]["role"], "tool");
        assert_eq!(result["messages"][1]["tool_call_id"], "call_1");
        assert_eq!(result["messages"][1]["content"], "{\"ok\":true}");
    }

    #[test]
    fn converts_anthropic_thinking_to_openai_chat_reasoning_content_when_requested() {
        let input = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Need a tool."},
                    {"type": "tool_use", "id": "call_1", "name": "Read", "input": {}}
                ]
            }]
        });

        let result = anthropic_to_openai_chat_request(&input, true);

        assert_eq!(result["messages"][0]["content"], Value::Null);
        assert_eq!(result["messages"][0]["reasoning_content"], "Need a tool.");

        let generic = anthropic_to_openai_chat_request(&input, false);
        assert!(generic["messages"][0].get("reasoning_content").is_none());
    }

    #[test]
    fn converts_anthropic_tool_use_reasoning_placeholders_when_requested() {
        let missing_reasoning = json!({
            "model": "kimi-k2.6",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_1", "name": "Read", "input": {}}
                ]
            }]
        });
        let redacted_reasoning = json!({
            "model": "mimo-v2.5-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "redacted_thinking", "data": "opaque"},
                    {"type": "tool_use", "id": "call_2", "name": "Edit", "input": {}}
                ]
            }]
        });

        let missing = anthropic_to_openai_chat_request(&missing_reasoning, true);
        let redacted = anthropic_to_openai_chat_request(&redacted_reasoning, true);

        assert_eq!(missing["messages"][0]["reasoning_content"], "tool call");
        assert_eq!(
            redacted["messages"][0]["reasoning_content"],
            ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
        );
    }

    #[test]
    fn converts_anthropic_chat_request_maps_reasoning_effort_and_tool_choice() {
        let input = json!({
            "model": "gpt-5.4",
            "max_tokens": 1024,
            "output_config": {"effort": "max"},
            "messages": [{"role": "user", "content": "Search"}],
            "tools": [{
                "name": "search",
                "description": "Search the web",
                "input_schema": {"type": "object", "properties": {}}
            }],
            "tool_choice": {"type": "tool", "name": "search"}
        });

        let result = anthropic_to_openai_chat_request(&input, false);

        assert_eq!(result["max_tokens"], 1024);
        assert_eq!(result["reasoning_effort"], "xhigh");
        assert_eq!(
            result["tool_choice"],
            json!({"type": "function", "function": {"name": "search"}})
        );
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
    fn codex_chat_streaming_helpers_extract_reasoning_and_errors() {
        assert_eq!(
            chat_delta_reasoning_text(&json!({"reasoning_content": "think"})).as_deref(),
            Some("think")
        );

        assert_eq!(leading_think_prefix_decision(""), ThinkPrefixDecision::NeedMore);
        assert_eq!(
            leading_think_prefix_decision("  <thi"),
            ThinkPrefixDecision::NeedMore
        );
        assert_eq!(
            leading_think_prefix_decision("\n<think>plan"),
            ThinkPrefixDecision::Reasoning
        );
        assert_eq!(
            leading_think_prefix_decision("answer"),
            ThinkPrefixDecision::Text
        );

        assert_eq!(
            extract_chat_sse_error(&json!({"error": {"message": "bad", "type": "invalid"}})),
            ("bad".to_string(), Some("invalid".to_string()))
        );
        assert_eq!(
            extract_chat_sse_error(&json!({"detail": "quota", "code": "rate_limit"})),
            ("quota".to_string(), Some("rate_limit".to_string()))
        );

        let event = sse_event("response.failed", json!({"type": "response.failed"}));
        assert_eq!(
            std::str::from_utf8(&event).unwrap(),
            "event: response.failed\ndata: {\"type\":\"response.failed\"}\n\n"
        );
    }

    #[test]
    fn codex_chat_stream_response_uses_default_usage_when_missing() {
        let response = codex_chat_stream_response(
            "resp_1",
            123,
            "in_progress",
            "gpt-5",
            vec![json!({"id": "msg_1", "type": "message"})],
            None,
        );

        assert_eq!(response["id"], "resp_1");
        assert_eq!(response["created_at"], 123);
        assert_eq!(response["status"], "in_progress");
        assert_eq!(response["model"], "gpt-5");
        assert_eq!(response["output"][0]["id"], "msg_1");
        assert_eq!(response["usage"]["input_tokens"], 0);
        assert_eq!(response["usage"]["output_tokens_details"]["reasoning_tokens"], 0);
    }

    #[test]
    fn codex_chat_stream_failed_event_wraps_error_in_response_envelope() {
        let response = codex_chat_stream_response(
            "resp_1",
            123,
            "failed",
            "gpt-5",
            Vec::new(),
            Some(&json!({
                "input_tokens": 1,
                "output_tokens": 2,
                "total_tokens": 3,
                "output_tokens_details": { "reasoning_tokens": 0 }
            })),
        );

        let event = codex_chat_stream_failed_event(
            response,
            "quota exceeded",
            Some("rate_limit_exceeded"),
        );
        let event = std::str::from_utf8(&event).expect("event utf8");

        assert!(event.starts_with("event: response.failed\n"));
        assert!(event.contains("\"type\":\"response.failed\""));
        assert!(event.contains("\"status\":\"failed\""));
        assert!(event.contains("\"message\":\"quota exceeded\""));
        assert!(event.contains("\"type\":\"rate_limit_exceeded\""));
        assert!(event.contains("\"total_tokens\":3"));
    }

    #[test]
    fn codex_chat_stream_lifecycle_events_wrap_response_envelopes() {
        let response = codex_chat_stream_response(
            "resp_1",
            123,
            "in_progress",
            "gpt-5",
            Vec::new(),
            None,
        );

        let events = codex_chat_stream_started_events(response);
        let created = std::str::from_utf8(&events[0]).expect("created event utf8");
        let in_progress = std::str::from_utf8(&events[1]).expect("in_progress event utf8");

        assert!(created.starts_with("event: response.created\n"));
        assert!(created.contains("\"type\":\"response.created\""));
        assert!(created.contains("\"status\":\"in_progress\""));
        assert!(in_progress.starts_with("event: response.in_progress\n"));
        assert!(in_progress.contains("\"type\":\"response.in_progress\""));
        assert!(in_progress.contains("\"id\":\"resp_1\""));

        let completed = codex_chat_stream_completed_event(codex_chat_stream_response(
            "resp_1",
            123,
            "completed",
            "gpt-5",
            Vec::new(),
            None,
        ));
        let completed = std::str::from_utf8(&completed).expect("completed event utf8");

        assert!(completed.starts_with("event: response.completed\n"));
        assert!(completed.contains("\"type\":\"response.completed\""));
        assert!(completed.contains("\"status\":\"completed\""));
    }

    #[test]
    fn codex_chat_stream_reasoning_item_events_use_responses_shape() {
        let in_progress = codex_chat_stream_reasoning_in_progress_item("rs_resp_1");
        assert_eq!(in_progress["type"], "reasoning");
        assert_eq!(in_progress["status"], "in_progress");
        assert_eq!(in_progress["summary"], json!([]));

        let completed = codex_chat_stream_reasoning_completed_item("rs_resp_1", "plan");
        assert_eq!(completed["summary"][0]["type"], "summary_text");
        assert_eq!(completed["summary"][0]["text"], "plan");

        let added = codex_chat_stream_output_item_added_event(0, in_progress);
        let part_added = codex_chat_stream_reasoning_summary_part_added_event("rs_resp_1", 0);
        let delta = codex_chat_stream_reasoning_summary_text_delta_event("rs_resp_1", 0, "pl");
        let text_done = codex_chat_stream_reasoning_summary_text_done_event("rs_resp_1", 0, "plan");
        let part_done = codex_chat_stream_reasoning_summary_part_done_event("rs_resp_1", 0, "plan");
        let item_done = codex_chat_stream_output_item_done_event(0, completed);

        let combined = [added, part_added, delta, text_done, part_done, item_done].concat();
        let combined = std::str::from_utf8(&combined).expect("events utf8");

        assert!(combined.contains("event: response.output_item.added"));
        assert!(combined.contains("event: response.reasoning_summary_part.added"));
        assert!(combined.contains("event: response.reasoning_summary_text.delta"));
        assert!(combined.contains("event: response.reasoning_summary_text.done"));
        assert!(combined.contains("event: response.reasoning_summary_part.done"));
        assert!(combined.contains("event: response.output_item.done"));
        assert!(combined.contains("\"item_id\":\"rs_resp_1\""));
    }

    #[test]
    fn codex_chat_stream_text_item_events_use_responses_shape() {
        let in_progress = codex_chat_stream_text_in_progress_item("resp_1_msg");
        assert_eq!(in_progress["type"], "message");
        assert_eq!(in_progress["status"], "in_progress");
        assert_eq!(in_progress["role"], "assistant");
        assert_eq!(in_progress["content"], json!([]));

        let completed = codex_chat_stream_text_completed_item("resp_1_msg", "hello");
        assert_eq!(completed["status"], "completed");
        assert_eq!(completed["content"][0]["type"], "output_text");
        assert_eq!(completed["content"][0]["text"], "hello");
        assert_eq!(completed["content"][0]["annotations"], json!([]));

        let added = codex_chat_stream_output_item_added_event(1, in_progress);
        let part_added = codex_chat_stream_content_part_added_event("resp_1_msg", 1);
        let delta = codex_chat_stream_output_text_delta_event("resp_1_msg", 1, "he");
        let text_done = codex_chat_stream_output_text_done_event("resp_1_msg", 1, "hello");
        let part_done = codex_chat_stream_content_part_done_event("resp_1_msg", 1, "hello");
        let item_done = codex_chat_stream_output_item_done_event(1, completed);

        let combined = [added, part_added, delta, text_done, part_done, item_done].concat();
        let combined = std::str::from_utf8(&combined).expect("events utf8");

        assert!(combined.contains("event: response.output_item.added"));
        assert!(combined.contains("event: response.content_part.added"));
        assert!(combined.contains("event: response.output_text.delta"));
        assert!(combined.contains("event: response.output_text.done"));
        assert!(combined.contains("event: response.content_part.done"));
        assert!(combined.contains("event: response.output_item.done"));
        assert!(combined.contains("\"item_id\":\"resp_1_msg\""));
        assert!(combined.contains("\"content_index\":0"));
    }

    #[test]
    fn codex_chat_stream_tool_argument_events_use_responses_shape() {
        let function_delta =
            codex_chat_stream_function_call_arguments_delta_event("fc_call_1", 2, "{\"a\":");
        let function_done =
            codex_chat_stream_function_call_arguments_done_event("fc_call_1", 2, "{\"a\":1}");
        let custom_delta =
            codex_chat_stream_custom_tool_call_input_delta_event("ctc_call_2", 3, "ls");
        let custom_done =
            codex_chat_stream_custom_tool_call_input_done_event("ctc_call_2", 3, "ls -la");

        let combined = [function_delta, function_done, custom_delta, custom_done].concat();
        let combined = std::str::from_utf8(&combined).expect("events utf8");

        assert!(combined.contains("event: response.function_call_arguments.delta"));
        assert!(combined.contains("\"type\":\"response.function_call_arguments.delta\""));
        assert!(combined.contains("\"delta\":\"{\\\"a\\\":\""));
        assert!(combined.contains("event: response.function_call_arguments.done"));
        assert!(combined.contains("\"arguments\":\"{\\\"a\\\":1}\""));
        assert!(combined.contains("event: response.custom_tool_call_input.delta"));
        assert!(combined.contains("\"delta\":\"ls\""));
        assert!(combined.contains("event: response.custom_tool_call_input.done"));
        assert!(combined.contains("\"input\":\"ls -la\""));
    }

    fn collect_codex_chat_stream(chunks: Vec<&str>) -> String {
        collect_codex_chat_stream_with_context(chunks, CodexToolContext::default())
    }

    fn collect_codex_chat_stream_with_context(
        chunks: Vec<&str>,
        tool_context: CodexToolContext,
    ) -> String {
        futures::executor::block_on(async move {
            let chunks: Vec<Result<Bytes, io::Error>> = chunks
                .into_iter()
                .map(|chunk| Ok(Bytes::copy_from_slice(chunk.as_bytes())))
                .collect();
            let upstream = futures_stream::iter(chunks);
            let converted =
                create_codex_chat_to_responses_sse_stream_with_context(upstream, tool_context);
            let bytes: Vec<Bytes> = converted.map(|item| item.unwrap()).collect().await;
            String::from_utf8(bytes.concat()).unwrap()
        })
    }

    fn collect_codex_chat_stream_from_results(chunks: Vec<Result<Bytes, io::Error>>) -> String {
        futures::executor::block_on(async move {
            let upstream = futures_stream::iter(chunks);
            let converted = create_codex_chat_to_responses_sse_stream(upstream);
            let bytes: Vec<Bytes> = converted.map(|item| item.unwrap()).collect().await;
            String::from_utf8(bytes.concat()).unwrap()
        })
    }

    #[test]
    fn codex_chat_stream_converts_text_to_responses_sse() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"id\":\"chatcmpl_1\",\"created\":123,\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"created\":123,\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2,\"total_tokens\":6}}\n\n",
            "data: [DONE]\n\n",
        ]);

        assert!(output.contains("event: response.created"));
        assert!(output.contains("event: response.output_text.delta"));
        assert!(output.contains("\"text\":\"Hello\""));
        assert!(output.contains("event: response.completed"));
        assert!(output.contains("\"input_tokens\":4"));
    }

    #[test]
    fn codex_chat_stream_converts_reasoning_content_to_responses_events() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"id\":\"chatcmpl_reason\",\"created\":123,\"model\":\"deepseek-reasoner\",\"choices\":[{\"delta\":{\"reasoning_content\":\"Need context. \"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_reason\",\"created\":123,\"model\":\"deepseek-reasoner\",\"choices\":[{\"delta\":{\"reasoning\":\"Now answer. \"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_reason\",\"created\":123,\"model\":\"deepseek-reasoner\",\"choices\":[{\"delta\":{\"content\":\"Done\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":6,\"total_tokens\":10,\"completion_tokens_details\":{\"reasoning_tokens\":3}}}\n\n",
            "data: [DONE]\n\n",
        ]);

        assert!(output.contains("event: response.reasoning_summary_part.added"));
        assert!(output.contains("event: response.reasoning_summary_text.delta"));
        assert!(output.contains("event: response.reasoning_summary_text.done"));
        assert!(output.contains("Need context. Now answer. "));
        assert!(output.contains("\"type\":\"reasoning\""));
        assert!(output.contains("\"text\":\"Done\""));
        assert!(output.contains("\"reasoning_tokens\":3"));

        let reasoning_pos = output.find("\"type\":\"reasoning\"").unwrap();
        let message_pos = output.find("\"type\":\"message\"").unwrap();
        assert!(reasoning_pos < message_pos);
    }

    #[test]
    fn codex_chat_stream_strips_inline_think_tags() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"id\":\"chatcmpl_minimax\",\"created\":123,\"model\":\"MiniMax-M2.7\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"<think>\\nNeed\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_minimax\",\"created\":123,\"model\":\"MiniMax-M2.7\",\"choices\":[{\"delta\":{\"content\":\" context.</think>\\n\\npong\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"id\":\"chatcmpl_minimax\",\"created\":123,\"model\":\"MiniMax-M2.7\",\"choices\":[],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":6,\"total_tokens\":10,\"completion_tokens_details\":{\"reasoning_tokens\":3}}}\n\n",
        ]);

        assert!(output.contains("event: response.reasoning_summary_text.delta"));
        assert!(output.contains("Need context."));
        assert!(output.contains("\"text\":\"pong\""));
        assert!(output.contains("\"reasoning_tokens\":3"));
        assert!(!output.contains("<think>"));
        assert!(!output.contains("</think>"));
        assert!(output.contains("event: response.completed"));
    }

    #[test]
    fn codex_chat_stream_converts_tool_calls_to_responses_sse() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\\\"Tokyo\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ]);

        assert!(output.contains("event: response.function_call_arguments.delta"));
        assert!(output.contains("event: response.function_call_arguments.done"));
        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("\"call_id\":\"call_1\""));
    }

    #[test]
    fn codex_chat_stream_restores_custom_tool_input_events() {
        let context = build_codex_tool_context_from_request(&json!({
            "model": "gpt-5.4",
            "tools": [{ "type": "custom", "name": "exec" }]
        }));
        let output = collect_codex_chat_stream_with_context(
            vec![
                "data: {\"id\":\"chatcmpl_custom\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_custom\",\"type\":\"function\",\"function\":{\"name\":\"exec\"}}]}}]}\n\n",
                "data: {\"id\":\"chatcmpl_custom\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"input\\\":\"}}]}}]}\n\n",
                "data: {\"id\":\"chatcmpl_custom\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"ls -la\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n",
            ],
            context,
        );

        assert!(output.contains("event: response.custom_tool_call_input.delta"));
        assert!(output.contains("event: response.custom_tool_call_input.done"));
        assert!(!output.contains("event: response.function_call_arguments.delta"));
        assert!(!output.contains("event: response.function_call_arguments.done"));
        assert!(output.contains("\"id\":\"ctc_call_custom\""));
        assert!(output.contains("\"type\":\"custom_tool_call\""));
        assert!(output.contains("\"name\":\"exec\""));
        assert!(output.contains("\"input\":\"ls -la\""));
    }

    #[test]
    fn codex_chat_stream_canonicalizes_tool_arguments_on_done_events() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"id\":\"chatcmpl_args\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_args\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{ \\\"b\\\": 2,\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_args\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\" \\\"a\\\": 1 }\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ]);

        assert!(output.contains(r#""arguments":"{\"a\":1,\"b\":2}""#));
    }

    #[test]
    fn codex_chat_stream_preserves_reasoning_content_on_tool_call_items() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"id\":\"chatcmpl_tool_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"reasoning_content\":\"Need file.\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\\\"README.md\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ]);

        assert!(output.contains("event: response.output_item.done"));
        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("\"reasoning_content\":\"Need file.\""));
    }

    #[test]
    fn codex_chat_stream_preserves_late_reasoning_content_on_tool_call_items() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"id\":\"chatcmpl_tool_late_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_late_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\\\"README.md\\\"}\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_late_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"reasoning_content\":\"Need file.\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_tool_late_reasoning\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ]);

        assert!(output.contains("event: response.output_item.done"));
        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("\"reasoning_content\":\"Need file.\""));
    }

    #[test]
    fn codex_chat_stream_restores_namespace_on_tool_call_items() {
        let context = build_codex_tool_context_from_request(&json!({
            "model": "gpt-5.4",
            "input": [{
                "type": "tool_search_output",
                "call_id": "call_tool_search_1",
                "tools": [{
                    "type": "namespace",
                    "name": "mcp__codex_apps__gmail",
                    "tools": [{
                        "type": "function",
                        "name": "_search_emails",
                        "description": "Search Gmail.",
                        "parameters": {"type": "object"}
                    }]
                }]
            }]
        }));
        let output = collect_codex_chat_stream_with_context(
            vec![
                "data: {\"id\":\"chatcmpl_gmail\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_gmail\",\"type\":\"function\",\"function\":{\"name\":\"mcp__codex_apps__gmail___search_emails\"}}]}}]}\n\n",
                "data: {\"id\":\"chatcmpl_gmail\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"query\\\":\\\"in:inbox\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n",
            ],
            context,
        );

        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("\"namespace\":\"mcp__codex_apps__gmail\""));
        assert!(output.contains("\"name\":\"_search_emails\""));
        assert!(output.contains(r#""arguments":"{\"query\":\"in:inbox\"}""#));
    }

    #[test]
    fn codex_chat_stream_restores_tool_search_on_tool_call_items() {
        let context = build_codex_tool_context_from_request(&json!({
            "model": "gpt-5.4",
            "tools": [{"type": "tool_search"}],
            "input": "Search for Gmail tools."
        }));
        let output = collect_codex_chat_stream_with_context(
            vec![
                "data: {\"id\":\"chatcmpl_tool_search\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_tool_search_1\",\"type\":\"function\",\"function\":{\"name\":\"tool_search\"}}]}}]}\n\n",
                "data: {\"id\":\"chatcmpl_tool_search\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"query\\\":\\\"Gmail search emails\\\",\\\"limit\\\":10}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n",
            ],
            context,
        );

        assert!(output.contains("\"type\":\"tool_search_call\""));
        assert!(output.contains("\"execution\":\"client\""));
        assert!(output.contains("\"call_id\":\"call_tool_search_1\""));
        assert!(output.contains("\"query\":\"Gmail search emails\""));
    }

    #[test]
    fn codex_chat_stream_error_emits_failed_without_completed() {
        let output = collect_codex_chat_stream_from_results(vec![Err(io::Error::other("boom"))]);

        assert!(output.contains("event: response.failed"));
        assert!(!output.contains("event: response.completed"));
    }

    #[test]
    fn codex_chat_stream_end_with_output_without_finish_reason_is_incomplete() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"id\":\"chatcmpl_truncated\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
        ]);

        assert!(output.contains("event: response.completed"));
        assert!(output.contains("\"status\":\"incomplete\""));
        assert!(output.contains("\"incomplete_details\":{\"reason\":\"max_output_tokens\"}"));
        assert!(!output.contains("event: response.failed"));
    }

    #[test]
    fn codex_chat_stream_end_without_output_or_finish_reason_fails() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"id\":\"chatcmpl_truncated\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{}}]}\n\n",
        ]);

        assert!(output.contains("event: response.failed"));
        assert!(output.contains("stream_truncated"));
        assert!(!output.contains("event: response.completed"));
    }

    #[test]
    fn codex_chat_stream_error_event_emits_failed_without_completed() {
        let output = collect_codex_chat_stream(vec![
            "event: error\ndata: {\"error\":{\"message\":\"bad request\",\"type\":\"invalid_request_error\"}}\n\n",
            "data: [DONE]\n\n",
        ]);

        assert!(output.contains("event: response.failed"));
        assert!(output.contains("bad request"));
        assert!(output.contains("invalid_request_error"));
        assert!(!output.contains("event: response.completed"));
    }

    #[test]
    fn codex_chat_stream_data_only_error_emits_failed_without_completed() {
        let output = collect_codex_chat_stream(vec![
            "data: {\"error\":{\"message\":\"quota exceeded\",\"code\":\"rate_limit_exceeded\"}}\n\n",
            "data: [DONE]\n\n",
        ]);

        assert!(output.contains("event: response.failed"));
        assert!(output.contains("quota exceeded"));
        assert!(output.contains("rate_limit_exceeded"));
        assert!(!output.contains("event: response.completed"));
    }

    #[test]
    fn codex_chat_to_responses_state_handles_text_chunk_and_finalize() {
        let mut state = CodexChatToResponsesState::default();

        let events = state.handle_chat_chunk(&json!({
            "id": "chatcmpl_1",
            "created": 123,
            "model": "gpt-5",
            "choices": [{
                "delta": { "content": "partial" }
            }]
        }));
        let event_bytes = events.concat();
        let events = std::str::from_utf8(&event_bytes).expect("events utf8");

        assert!(events.contains("event: response.created"));
        assert!(events.contains("event: response.output_text.delta"));
        assert!(state.has_substantive_output());
        assert!(!state.has_finish_reason());

        state.set_finish_reason("length");
        let completed = state.finalize();
        let completed_bytes = completed.concat();
        let completed = std::str::from_utf8(&completed_bytes).expect("completed utf8");

        assert!(state.is_completed());
        assert!(completed.contains("event: response.completed"));
        assert!(completed.contains("\"status\":\"incomplete\""));
        assert!(completed.contains("\"incomplete_details\":{\"reason\":\"max_output_tokens\"}"));
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
    fn resolve_claude_api_format_from_settings_projects_legacy_settings() {
        assert_eq!(
            resolve_claude_api_format_from_settings(
                None,
                Some("openai_responses"),
                &json!({
                    "api_format": "openai_chat",
                    "openrouter_compat_mode": true,
                }),
            ),
            "openai_responses"
        );
        assert_eq!(
            resolve_claude_api_format_from_settings(
                None,
                None,
                &json!({"api_format": "openai_chat"}),
            ),
            "openai_chat"
        );
        assert_eq!(
            resolve_claude_api_format_from_settings(
                None,
                None,
                &json!({"openrouter_compat_mode": "true"}),
            ),
            "openai_chat"
        );
        assert_eq!(
            resolve_claude_api_format_from_settings(
                Some("codex_oauth"),
                Some("anthropic"),
                &json!({"api_format": "openai_chat"}),
            ),
            "openai_responses"
        );
    }

    #[test]
    fn resolve_claude_forward_api_format_applies_copilot_vendor_policy() {
        assert_eq!(
            resolve_claude_forward_api_format("gemini_native", false, Some("OpenAI")),
            "gemini_native"
        );
        assert_eq!(
            resolve_claude_forward_api_format("openai_chat", true, Some("OpenAI")),
            "openai_responses"
        );
        assert_eq!(
            resolve_claude_forward_api_format("openai_responses", true, Some("openai")),
            "openai_responses"
        );
        assert_eq!(
            resolve_claude_forward_api_format("openai_responses", true, Some("Anthropic")),
            "openai_chat"
        );
        assert_eq!(
            resolve_claude_forward_api_format("openai_responses", true, None),
            "openai_chat"
        );
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
    fn reasoning_vendor_detection_uses_model_or_endpoint_hints() {
        assert!(is_reasoning_vendor_identifier("moonshotai/kimi-k2"));
        assert!(is_reasoning_vendor_identifier("https://api.deepseek.com/anthropic"));
        assert!(!is_reasoning_vendor_identifier("https://api.anthropic.com"));

        assert!(should_preserve_reasoning_content_for_openai_chat(
            &json!({}),
            &json!({"model": "mimo-v2.5-pro"})
        ));
        assert!(should_preserve_reasoning_content_for_openai_chat(
            &json!({"env": {"ANTHROPIC_BASE_URL": "https://relay.example.com/deepseek"}}),
            &json!({"model": "claude-sonnet-4"})
        ));
        assert!(!should_preserve_reasoning_content_for_openai_chat(
            &json!({"base_url": "https://api.anthropic.com"}),
            &json!({"model": "claude-sonnet-4"})
        ));
    }

    #[test]
    fn thinking_history_normalization_gate_requires_anthropic_format_and_reasoning_vendor() {
        let settings = json!({"apiEndpoint": "https://gateway.example.com/kimi"});
        let body = json!({"model": "claude-sonnet-4"});

        assert!(should_normalize_anthropic_tool_thinking_history(
            &settings,
            &body,
            "anthropic"
        ));
        assert!(!should_normalize_anthropic_tool_thinking_history(
            &settings,
            &body,
            "openai_chat"
        ));
    }

    #[test]
    fn normalizes_anthropic_tool_thinking_history_for_tool_use_turns() {
        let mut body = json!({
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "calling tool"},
                    {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {}}
                ]
            }, {
                "role": "assistant",
                "content": [
                    {"type": "redacted_thinking", "data": "opaque"},
                    {"type": "tool_use", "id": "toolu_2", "name": "Edit", "input": {}}
                ]
            }, {
                "role": "assistant",
                "content": [
                    {
                        "type": "thinking",
                        "thinking": "Need the file.",
                        "signature": "anthropic-signature"
                    },
                    {"type": "tool_use", "id": "toolu_3", "name": "Read", "input": {}}
                ]
            }]
        });

        assert!(normalize_anthropic_tool_thinking_history(&mut body));

        assert_eq!(
            body["messages"][0]["content"][0]["thinking"],
            ANTHROPIC_TOOL_THINKING_PLACEHOLDER
        );
        assert_eq!(
            body["messages"][1]["content"][0]["thinking"],
            ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
        );
        assert_eq!(body["messages"][2]["content"][0]["thinking"], "Need the file.");
        assert!(body["messages"][2]["content"][0].get("signature").is_none());
    }

    #[test]
    fn detects_deepseek_official_anthropic_endpoint_from_settings_config() {
        assert!(is_deepseek_official_anthropic_endpoint(&json!({
            "env": {"ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic/"}
        })));
        assert!(is_deepseek_official_anthropic_endpoint(&json!({
            "base_url": "https://api.deepseek.com/anthropic"
        })));
        assert!(is_deepseek_official_anthropic_endpoint(&json!({
            "baseURL": "https://api.deepseek.com/anthropic"
        })));
        assert!(is_deepseek_official_anthropic_endpoint(&json!({
            "apiEndpoint": "https://api.deepseek.com/anthropic"
        })));
        assert!(!is_deepseek_official_anthropic_endpoint(&json!({
            "env": {"ANTHROPIC_BASE_URL": "https://api.anthropic.com"}
        })));
    }

    #[test]
    fn deepseek_thinking_disabled_strips_conflicting_effort_fields() {
        let settings = json!({
            "env": {"ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic"}
        });
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": {"type": "disabled"},
            "output_config": {"effort": "max"},
            "reasoning_effort": "high"
        });

        assert!(normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &settings
        ));
        assert!(body.get("output_config").is_none());
        assert!(body.get("reasoning_effort").is_none());
        assert_eq!(body["thinking"]["type"], "disabled");
    }

    #[test]
    fn deepseek_thinking_disabled_keeps_other_output_config_fields() {
        let settings = json!({"base_url": "https://api.deepseek.com/anthropic"});
        let mut body = json!({
            "thinking": {"type": "disabled"},
            "output_config": {"effort": "max", "temperature": 0.5}
        });

        assert!(normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &settings
        ));
        assert_eq!(body["output_config"]["temperature"], 0.5);
        assert!(body["output_config"].get("effort").is_none());
    }

    #[test]
    fn deepseek_thinking_disabled_ignores_other_endpoints_or_thinking_modes() {
        let mut body = json!({
            "thinking": {"type": "disabled"},
            "output_config": {"effort": "max"}
        });
        let original = body.clone();

        assert!(!normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            &json!({"base_url": "https://api.anthropic.com"})
        ));
        assert_eq!(body, original);

        let settings = json!({"base_url": "https://api.deepseek.com/anthropic"});
        let mut enabled = json!({
            "thinking": {"type": "enabled"},
            "output_config": {"effort": "max"}
        });
        let original = enabled.clone();

        assert!(!normalize_deepseek_thinking_disabled_strip_effort(
            &mut enabled,
            &settings
        ));
        assert_eq!(enabled, original);
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
    fn converts_openai_responses_message_to_anthropic_message() {
        let input = json!({
            "id": "resp_123",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "Hello"}]
            }],
            "usage": {"input_tokens": 10, "output_tokens": 20}
        });

        let result = openai_responses_to_anthropic_message(&input).unwrap();

        assert_eq!(result["id"], "resp_123");
        assert_eq!(result["content"][0], json!({"type": "text", "text": "Hello"}));
        assert_eq!(result["stop_reason"], "end_turn");
        assert_eq!(result["usage"]["input_tokens"], 10);
        assert_eq!(result["usage"]["output_tokens"], 20);
    }

    #[test]
    fn converts_openai_responses_function_call_to_anthropic_tool_use() {
        let input = json!({
            "id": "resp_tool",
            "status": "completed",
            "model": "gpt-5.5",
            "output": [{
                "type": "function_call",
                "call_id": "call_read",
                "name": "Read",
                "arguments": "{\"file_path\":\"/tmp/demo.py\",\"pages\":\"\"}"
            }]
        });

        let result = openai_responses_to_anthropic_message(&input).unwrap();

        assert_eq!(result["content"][0]["type"], "tool_use");
        assert_eq!(result["content"][0]["id"], "call_read");
        assert_eq!(result["content"][0]["name"], "Read");
        assert_eq!(result["content"][0]["input"]["file_path"], "/tmp/demo.py");
        assert!(result["content"][0]["input"].get("pages").is_none());
        assert_eq!(result["stop_reason"], "tool_use");
    }

    #[test]
    fn converts_openai_responses_reasoning_summary_to_anthropic_thinking() {
        let input = json!({
            "id": "resp_reasoning",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "reasoning",
                "summary": [
                    {"type": "summary_text", "text": "Think"},
                    {"type": "summary_text", "text": " now"}
                ]
            }]
        });

        let result = openai_responses_to_anthropic_message(&input).unwrap();

        assert_eq!(
            result["content"][0],
            json!({"type": "thinking", "thinking": "Think now"})
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
    fn converts_openai_chat_message_to_anthropic_message() {
        let input = json!({
            "id": "chatcmpl_123",
            "model": "gpt-4o",
            "choices": [{
                "message": {"role": "assistant", "content": "Hello"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 20}
        });

        let result = openai_chat_to_anthropic_message(&input).unwrap();

        assert_eq!(result["id"], "chatcmpl_123");
        assert_eq!(result["content"][0], json!({"type": "text", "text": "Hello"}));
        assert_eq!(result["stop_reason"], "end_turn");
        assert_eq!(result["usage"]["input_tokens"], 10);
        assert_eq!(result["usage"]["output_tokens"], 20);
    }

    #[test]
    fn converts_openai_chat_response_preserves_id_for_usage_dedup() {
        let input = json!({
            "id": "chatcmpl-claude-compatible",
            "model": "claude-sonnet-4-5",
            "choices": [{
                "message": {"role": "assistant", "content": "Hello"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "prompt_tokens_details": {"cached_tokens": 80}
            }
        });

        let result = openai_chat_to_anthropic_message(&input).unwrap();
        let usage = crate::TokenUsage::from_claude_response(&result)
            .expect("converted Anthropic response should parse usage");

        assert_eq!(result["id"], "chatcmpl-claude-compatible");
        assert_eq!(result["usage"]["input_tokens"], 20);
        assert_eq!(result["usage"]["cache_read_input_tokens"], 80);
        assert_eq!(
            usage.message_id.as_deref(),
            Some("chatcmpl-claude-compatible")
        );
        assert_eq!(
            format!(
                "{}{}",
                crate::SESSION_REQUEST_ID_PREFIX,
                usage.message_id.as_deref().unwrap()
            ),
            "session:chatcmpl-claude-compatible"
        );
    }

    #[test]
    fn converts_openai_chat_tool_calls_to_anthropic_tool_use() {
        let input = json!({
            "id": "chatcmpl_tool",
            "model": "gpt-4o",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "search", "arguments": "{\"query\":\"rust\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });

        let result = openai_chat_to_anthropic_message(&input).unwrap();

        assert_eq!(result["content"][0]["type"], "tool_use");
        assert_eq!(result["content"][0]["id"], "call_1");
        assert_eq!(result["content"][0]["name"], "search");
        assert_eq!(result["content"][0]["input"]["query"], "rust");
        assert_eq!(result["stop_reason"], "tool_use");
    }

    #[test]
    fn converts_openai_chat_reasoning_and_content_parts_to_anthropic_content() {
        let input = json!({
            "id": "chatcmpl_reasoning",
            "model": "deepseek-v4-pro",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "inspect state",
                    "content": [
                        {"type": "output_text", "text": "Answer"},
                        {"type": "refusal", "refusal": "No"}
                    ]
                },
                "finish_reason": "stop"
            }]
        });

        let result = openai_chat_to_anthropic_message(&input).unwrap();

        assert_eq!(
            result["content"][0],
            json!({"type": "thinking", "thinking": "inspect state"})
        );
        assert_eq!(result["content"][1], json!({"type": "text", "text": "Answer"}));
        assert_eq!(result["content"][2], json!({"type": "text", "text": "No"}));
    }

    #[test]
    fn converts_openai_chat_legacy_function_call_to_anthropic_tool_use() {
        let input = json!({
            "id": "chatcmpl_legacy",
            "model": "gpt-4o",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "function_call": {
                        "id": "legacy_1",
                        "name": "lookup",
                        "arguments": "{\"id\":42}"
                    }
                },
                "finish_reason": "function_call"
            }]
        });

        let result = openai_chat_to_anthropic_message(&input).unwrap();

        assert_eq!(result["content"][0]["type"], "tool_use");
        assert_eq!(result["content"][0]["id"], "legacy_1");
        assert_eq!(result["content"][0]["name"], "lookup");
        assert_eq!(result["content"][0]["input"]["id"], 42);
        assert_eq!(result["stop_reason"], "tool_use");
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
            "model": "qwen3-7-max",
            "tool_choice": "auto",
            "input": "hi"
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, false);

        assert!(result.get("tools").is_none());
        assert!(result.get("tool_choice").is_none());
        assert_eq!(result["model"], "qwen3-7-max");
    }

    #[test]
    fn codex_responses_request_to_chat_drops_tool_fields_when_tools_empty() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [],
            "tool_choice": "auto",
            "input": "hi"
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, false);

        assert!(result.get("tools").is_none());
        assert!(result.get("tool_choice").is_none());
    }

    #[test]
    fn codex_responses_request_to_chat_drops_parallel_tool_calls_without_tools() {
        let request = json!({
            "model": "gpt-5.4",
            "tool_choice": "auto",
            "parallel_tool_calls": true
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, false);

        assert!(result.get("tools").is_none());
        assert!(result.get("tool_choice").is_none());
        assert!(result.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn codex_responses_request_to_chat_drops_tool_fields_when_all_tools_filtered() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{"type": "function"}],
            "tool_choice": "auto",
            "input": "hi"
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, false);

        assert!(result.get("tools").is_none());
        assert!(result.get("tool_choice").is_none());
    }

    #[test]
    fn codex_responses_request_to_chat_keeps_tool_fields_when_tools_present() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{
                "type": "function",
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {"type": "object"}
            }],
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "input": "hi"
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, false);

        assert_eq!(result["tool_choice"], "auto");
        assert_eq!(result["parallel_tool_calls"], true);
        assert_eq!(result["tools"][0]["function"]["name"], "get_weather");
    }

    #[test]
    fn codex_responses_request_to_chat_maps_function_tool_choice_when_tools_present() {
        let request = json!({
            "model": "gpt-5.4",
            "tools": [{
                "type": "function",
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {"type": "object"}
            }],
            "tool_choice": {"type": "function", "name": "get_weather"},
            "input": "hi"
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, false);

        assert_eq!(result["tool_choice"]["type"], "function");
        assert_eq!(result["tool_choice"]["function"]["name"], "get_weather");
    }

    #[test]
    fn codex_responses_request_to_chat_drops_tool_choice_none_without_tools() {
        let request = json!({
            "model": "gpt-5.4",
            "tool_choice": "none",
            "input": "hi"
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, false);

        assert!(result.get("tool_choice").is_none());
    }

    #[test]
    fn codex_responses_request_to_chat_keeps_tool_search_discovered_tools() {
        let request = json!({
            "model": "gpt-5.4",
            "tool_choice": "auto",
            "input": [{
                "type": "tool_search_output",
                "call_id": "call_ts_1",
                "status": "completed",
                "execution": "client",
                "tools": [{
                    "type": "function",
                    "name": "search_docs",
                    "description": "Search documentation.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"}
                        }
                    }
                }]
            }]
        });

        let result = responses_to_chat_completions_with_options(&request, None, false, false);

        assert_eq!(result["tool_choice"], "auto");
        assert_eq!(result["tools"][0]["function"]["name"], "search_docs");
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
