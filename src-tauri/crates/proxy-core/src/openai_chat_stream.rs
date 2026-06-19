//! OpenAI Chat Completions SSE -> Anthropic SSE state machine.

use crate::{
    response_transform::{
        build_anthropic_message_delta_event, map_openai_chat_finish_reason_to_anthropic,
    },
    usage::build_anthropic_usage_from_openai_chat_tokens,
};
use bytes::Bytes;
use futures::{stream as futures_stream, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    io,
    pin::Pin,
};

const INFINITE_WHITESPACE_THRESHOLD: usize = 500;

#[derive(Debug, Deserialize)]
struct OpenAiChatStreamChunk {
    #[serde(default)]
    id: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Delta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default, alias = "reasoning_content")]
    reasoning: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct DeltaToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<DeltaFunction>,
}

#[derive(Debug, Deserialize)]
struct DeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(default)]
    cache_read_input_tokens: Option<u32>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: u32,
}

#[derive(Debug, Clone)]
struct ToolBlockState {
    anthropic_index: u32,
    id: String,
    name: String,
    started: bool,
    pending_args: String,
    consecutive_whitespace: usize,
    aborted: bool,
}

#[derive(Debug, Default)]
pub struct OpenAiChatToAnthropicSseState {
    message_id: Option<String>,
    current_model: Option<String>,
    next_content_index: u32,
    has_sent_message_start: bool,
    has_emitted_message_delta: bool,
    pending_message_delta: Option<(Option<String>, Option<Value>)>,
    has_sent_message_stop: bool,
    latest_usage: Option<Value>,
    current_non_tool_block_type: Option<NonToolBlockType>,
    current_non_tool_block_index: Option<u32>,
    tool_blocks_by_index: HashMap<usize, ToolBlockState>,
    open_tool_block_indices: HashSet<u32>,
}

struct OpenAiChatToAnthropicSseStreamContext<S> {
    stream: Pin<Box<S>>,
    buffer: String,
    utf8_remainder: Vec<u8>,
    state: OpenAiChatToAnthropicSseState,
    pending_events: VecDeque<Bytes>,
    finished: bool,
}

/// Convert an OpenAI Chat Completions SSE byte stream into Anthropic SSE bytes.
///
/// This owns only byte/SSE transport mechanics; protocol decisions remain in
/// `OpenAiChatToAnthropicSseState`.
pub fn create_openai_chat_to_anthropic_sse_stream<S, E>(
    stream: S,
) -> impl Stream<Item = Result<Bytes, io::Error>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: Error + Send + 'static,
{
    let context = OpenAiChatToAnthropicSseStreamContext {
        stream: Box::pin(stream),
        buffer: String::new(),
        utf8_remainder: Vec::new(),
        state: OpenAiChatToAnthropicSseState::new(),
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

                        for line in block.lines() {
                            if let Some(data) = crate::strip_sse_field(line, "data") {
                                context.pending_events.extend(context.state.handle_data(data));
                            }
                        }
                    }
                }
                Some(Err(error)) => {
                    context.finished = true;
                    return Some((
                        Ok(OpenAiChatToAnthropicSseState::stream_error_event(format!(
                            "Stream error: {error}"
                        ))),
                        context,
                    ));
                }
                None => {
                    context.finished = true;
                    context.pending_events.extend(context.state.finish());
                }
            }
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NonToolBlockType {
    Thinking,
    Text,
}

impl OpenAiChatToAnthropicSseState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_data(&mut self, data: &str) -> Vec<Bytes> {
        if data.trim() == "[DONE]" {
            return self.handle_done();
        }

        let Ok(chunk) = serde_json::from_str::<OpenAiChatStreamChunk>(data) else {
            return Vec::new();
        };

        self.handle_chunk(chunk)
    }

    pub fn finish(&mut self) -> Vec<Bytes> {
        let mut events = Vec::new();
        let emitted_pending_message_delta = self.emit_pending_message_delta(&mut events);

        if emitted_pending_message_delta && !self.has_sent_message_stop {
            events.push(encode_sse("message_stop", &json!({"type": "message_stop"})));
            self.has_sent_message_stop = true;
        }

        events
    }

    pub fn stream_error_event(message: impl AsRef<str>) -> Bytes {
        encode_sse(
            "error",
            &json!({
                "type": "error",
                "error": {
                    "type": "stream_error",
                    "message": message.as_ref()
                }
            }),
        )
    }

    fn handle_done(&mut self) -> Vec<Bytes> {
        let mut events = Vec::new();
        self.emit_pending_message_delta(&mut events);
        events.push(encode_sse("message_stop", &json!({"type": "message_stop"})));
        self.has_sent_message_stop = true;
        events
    }

    fn handle_chunk(&mut self, chunk: OpenAiChatStreamChunk) -> Vec<Bytes> {
        let mut events = Vec::new();

        if self.message_id.is_none() && !chunk.id.is_empty() {
            self.message_id = Some(chunk.id.clone());
        }
        if self.current_model.is_none() && !chunk.model.is_empty() {
            self.current_model = Some(chunk.model.clone());
        }

        let chunk_usage_json = chunk.usage.as_ref().map(build_anthropic_usage_json);
        if let Some(usage_json) = &chunk_usage_json {
            self.latest_usage = Some(usage_json.clone());
            if let Some((_, pending_usage)) = self.pending_message_delta.as_mut() {
                *pending_usage = Some(usage_json.clone());
            }
        }

        let Some(choice) = chunk.choices.first() else {
            return events;
        };

        self.ensure_message_start(&mut events, chunk.usage.as_ref());
        self.handle_reasoning(choice, &mut events);
        self.handle_text(choice, &mut events);
        self.handle_tool_calls(choice, &mut events);
        self.handle_finish_reason(choice, chunk_usage_json, &mut events);

        events
    }

    fn ensure_message_start(&mut self, events: &mut Vec<Bytes>, usage: Option<&Usage>) {
        if self.has_sent_message_start {
            return;
        }

        let start_usage = if let Some(usage) = usage {
            start_usage_json(usage)
        } else {
            json!({
                "input_tokens": 0,
                "output_tokens": 0
            })
        };

        events.push(encode_sse(
            "message_start",
            &json!({
                "type": "message_start",
                "message": {
                    "id": self.message_id.clone().unwrap_or_default(),
                    "type": "message",
                    "role": "assistant",
                    "model": self.current_model.clone().unwrap_or_default(),
                    "usage": start_usage
                }
            }),
        ));
        self.has_sent_message_start = true;
    }

    fn handle_reasoning(&mut self, choice: &StreamChoice, events: &mut Vec<Bytes>) {
        let Some(reasoning) = &choice.delta.reasoning else {
            return;
        };

        if self.current_non_tool_block_type != Some(NonToolBlockType::Thinking) {
            self.close_current_non_tool_block(events);
            let index = self.allocate_content_index();
            events.push(content_block_start(
                index,
                json!({
                    "type": "thinking",
                    "thinking": ""
                }),
            ));
            self.current_non_tool_block_type = Some(NonToolBlockType::Thinking);
            self.current_non_tool_block_index = Some(index);
        }

        if let Some(index) = self.current_non_tool_block_index {
            events.push(encode_sse(
                "content_block_delta",
                &json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {
                        "type": "thinking_delta",
                        "thinking": reasoning
                    }
                }),
            ));
        }
    }

    fn handle_text(&mut self, choice: &StreamChoice, events: &mut Vec<Bytes>) {
        let Some(content) = &choice.delta.content else {
            return;
        };
        if content.is_empty() {
            return;
        }

        if self.current_non_tool_block_type != Some(NonToolBlockType::Text) {
            self.close_current_non_tool_block(events);
            let index = self.allocate_content_index();
            events.push(content_block_start(
                index,
                json!({
                    "type": "text",
                    "text": ""
                }),
            ));
            self.current_non_tool_block_type = Some(NonToolBlockType::Text);
            self.current_non_tool_block_index = Some(index);
        }

        if let Some(index) = self.current_non_tool_block_index {
            events.push(encode_sse(
                "content_block_delta",
                &json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {
                        "type": "text_delta",
                        "text": content
                    }
                }),
            ));
        }
    }

    fn handle_tool_calls(&mut self, choice: &StreamChoice, events: &mut Vec<Bytes>) {
        let Some(tool_calls) = &choice.delta.tool_calls else {
            return;
        };
        if tool_calls.is_empty() {
            return;
        }

        self.close_current_non_tool_block(events);
        self.current_non_tool_block_type = None;

        for tool_call in tool_calls {
            let Some(update) = self.update_tool_block(tool_call) else {
                continue;
            };

            if update.should_start {
                events.push(content_block_start(
                    update.anthropic_index,
                    json!({
                        "type": "tool_use",
                        "id": update.id,
                        "name": update.name
                    }),
                ));
                self.open_tool_block_indices.insert(update.anthropic_index);
            }

            if let Some(args) = update.pending_after_start {
                events.push(input_json_delta(update.anthropic_index, args));
            }

            if let Some(args) = update.immediate_delta {
                events.push(input_json_delta(update.anthropic_index, args));
            }
        }
    }

    fn update_tool_block(&mut self, tool_call: &DeltaToolCall) -> Option<ToolBlockUpdate> {
        let state = self
            .tool_blocks_by_index
            .entry(tool_call.index)
            .or_insert_with(|| {
                let index = self.next_content_index;
                self.next_content_index += 1;
                ToolBlockState {
                    anthropic_index: index,
                    id: String::new(),
                    name: String::new(),
                    started: false,
                    pending_args: String::new(),
                    consecutive_whitespace: 0,
                    aborted: false,
                }
            });

        if state.aborted {
            return None;
        }

        if let Some(id) = &tool_call.id {
            state.id = id.clone();
        }
        if let Some(name) = tool_call
            .function
            .as_ref()
            .and_then(|function| function.name.as_ref())
        {
            state.name = name.clone();
        }

        let should_start = !state.started && !state.id.is_empty() && !state.name.is_empty();
        if should_start {
            state.started = true;
        }
        let pending_after_start = if should_start && !state.pending_args.is_empty() {
            Some(std::mem::take(&mut state.pending_args))
        } else {
            None
        };

        let immediate_delta = tool_call
            .function
            .as_ref()
            .and_then(|function| function.arguments.clone())
            .and_then(|args| {
                for ch in args.chars() {
                    if ch.is_whitespace() {
                        state.consecutive_whitespace += 1;
                    } else {
                        state.consecutive_whitespace = 0;
                    }
                }

                if state.consecutive_whitespace >= INFINITE_WHITESPACE_THRESHOLD {
                    state.aborted = true;
                    None
                } else if state.started {
                    Some(args)
                } else {
                    state.pending_args.push_str(&args);
                    None
                }
            });

        Some(ToolBlockUpdate {
            anthropic_index: state.anthropic_index,
            id: state.id.clone(),
            name: state.name.clone(),
            should_start,
            pending_after_start,
            immediate_delta,
        })
    }

    fn handle_finish_reason(
        &mut self,
        choice: &StreamChoice,
        chunk_usage_json: Option<Value>,
        events: &mut Vec<Bytes>,
    ) {
        let Some(finish_reason) = &choice.finish_reason else {
            return;
        };

        let stop_reason = map_openai_chat_finish_reason_to_anthropic(Some(finish_reason), false)
            .map(ToString::to_string);
        let usage_json = chunk_usage_json.or_else(|| self.latest_usage.clone());

        if self.has_emitted_message_delta {
            if let (Some((_, pending_usage)), Some(usage_json)) =
                (&mut self.pending_message_delta, usage_json)
            {
                *pending_usage = Some(usage_json);
            }
            return;
        }
        self.has_emitted_message_delta = true;

        self.close_current_non_tool_block(events);
        self.current_non_tool_block_type = None;
        self.start_late_tool_blocks(events);
        self.close_open_tool_blocks(events);

        self.pending_message_delta = Some((stop_reason, usage_json));
    }

    fn start_late_tool_blocks(&mut self, events: &mut Vec<Bytes>) {
        let mut late_tool_starts: Vec<(u32, String, String, String)> = Vec::new();

        for (tool_idx, state) in self.tool_blocks_by_index.iter_mut() {
            if state.started {
                continue;
            }
            let has_payload =
                !state.pending_args.is_empty() || !state.id.is_empty() || !state.name.is_empty();
            if !has_payload {
                continue;
            }

            let fallback_id = if state.id.is_empty() {
                format!("tool_call_{tool_idx}")
            } else {
                state.id.clone()
            };
            let fallback_name = if state.name.is_empty() {
                "unknown_tool".to_string()
            } else {
                state.name.clone()
            };
            state.started = true;
            let pending = std::mem::take(&mut state.pending_args);
            late_tool_starts.push((
                state.anthropic_index,
                fallback_id,
                fallback_name,
                pending,
            ));
        }

        late_tool_starts.sort_unstable_by_key(|(index, _, _, _)| *index);
        for (index, id, name, pending) in late_tool_starts {
            events.push(content_block_start(
                index,
                json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name
                }),
            ));
            self.open_tool_block_indices.insert(index);
            if !pending.is_empty() {
                events.push(input_json_delta(index, pending));
            }
        }
    }

    fn close_open_tool_blocks(&mut self, events: &mut Vec<Bytes>) {
        if self.open_tool_block_indices.is_empty() {
            return;
        }

        let mut indices: Vec<u32> = self.open_tool_block_indices.iter().copied().collect();
        indices.sort_unstable();
        for index in indices {
            events.push(content_block_stop(index));
        }
        self.open_tool_block_indices.clear();
    }

    fn close_current_non_tool_block(&mut self, events: &mut Vec<Bytes>) {
        let Some(index) = self.current_non_tool_block_index.take() else {
            return;
        };
        events.push(content_block_stop(index));
    }

    fn emit_pending_message_delta(&mut self, events: &mut Vec<Bytes>) -> bool {
        let Some((stop_reason, usage_json)) = self.pending_message_delta.take() else {
            return false;
        };

        let event = build_anthropic_message_delta_event(stop_reason.as_deref(), usage_json);
        events.push(encode_sse("message_delta", &event));
        true
    }

    fn allocate_content_index(&mut self) -> u32 {
        let index = self.next_content_index;
        self.next_content_index += 1;
        index
    }
}

struct ToolBlockUpdate {
    anthropic_index: u32,
    id: String,
    name: String,
    should_start: bool,
    pending_after_start: Option<String>,
    immediate_delta: Option<String>,
}

fn build_anthropic_usage_json(usage: &Usage) -> Value {
    build_anthropic_usage_from_openai_chat_tokens(
        usage.prompt_tokens as u64,
        usage.completion_tokens as u64,
        extract_cache_read_tokens(usage).unwrap_or(0) as u64,
        usage.cache_creation_input_tokens.unwrap_or(0) as u64,
    )
}

fn start_usage_json(usage: &Usage) -> Value {
    let cached = extract_cache_read_tokens(usage).unwrap_or(0);
    let cache_creation = usage.cache_creation_input_tokens.unwrap_or(0);
    let input = usage
        .prompt_tokens
        .saturating_sub(cached)
        .saturating_sub(cache_creation);
    let mut start_usage = json!({
        "input_tokens": input,
        "output_tokens": 0
    });
    if cached > 0 {
        start_usage["cache_read_input_tokens"] = json!(cached);
    }
    if cache_creation > 0 {
        start_usage["cache_creation_input_tokens"] = json!(cache_creation);
    }
    start_usage
}

fn extract_cache_read_tokens(usage: &Usage) -> Option<u32> {
    if let Some(value) = usage.cache_read_input_tokens {
        return Some(value);
    }
    usage
        .prompt_tokens_details
        .as_ref()
        .map(|details| details.cached_tokens)
        .filter(|value| *value > 0)
}

fn content_block_start(index: u32, content_block: Value) -> Bytes {
    encode_sse(
        "content_block_start",
        &json!({
            "type": "content_block_start",
            "index": index,
            "content_block": content_block
        }),
    )
}

fn content_block_stop(index: u32) -> Bytes {
    encode_sse(
        "content_block_stop",
        &json!({
            "type": "content_block_stop",
            "index": index
        }),
    )
}

fn input_json_delta(index: u32, partial_json: String) -> Bytes {
    encode_sse(
        "content_block_delta",
        &json!({
            "type": "content_block_delta",
            "index": index,
            "delta": {
                "type": "input_json_delta",
                "partial_json": partial_json
            }
        }),
    )
}

fn encode_sse(event_name: &str, event: &Value) -> Bytes {
    Bytes::from(format!(
        "event: {event_name}\ndata: {}\n\n",
        serde_json::to_string(event).unwrap_or_default()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_events(data: &[&str], finish: bool) -> Vec<Value> {
        let mut state = OpenAiChatToAnthropicSseState::new();
        let mut chunks = Vec::new();
        for data in data {
            chunks.extend(state.handle_data(data));
        }
        if finish {
            chunks.extend(state.finish());
        }
        let output = chunks
            .into_iter()
            .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).to_string())
            .collect::<String>();

        output
            .split("\n\n")
            .filter_map(|block| {
                let data = block
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))?;
                serde_json::from_str::<Value>(data).ok()
            })
            .collect()
    }

    fn collect_stream_output(chunks: Vec<Result<Bytes, io::Error>>) -> String {
        futures::executor::block_on(async move {
            let upstream = futures_stream::iter(chunks);
            let converted = create_openai_chat_to_anthropic_sse_stream(upstream);
            let chunks: Vec<_> = converted.collect().await;

            chunks
                .into_iter()
                .map(|chunk| String::from_utf8_lossy(chunk.unwrap().as_ref()).to_string())
                .collect()
        })
    }

    fn collect_stream_events(input: &str) -> Vec<Value> {
        let output = collect_stream_output(vec![Ok(Bytes::from(input.as_bytes().to_vec()))]);

        output
            .split("\n\n")
            .filter_map(|block| {
                let data = block
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))?;
                serde_json::from_str::<Value>(data).ok()
            })
            .collect()
    }

    fn event_type(event: &Value) -> Option<&str> {
        event.get("type").and_then(Value::as_str)
    }

    #[test]
    fn routes_tool_call_deltas_by_index() {
        let events = collect_events(
            &[
                r#"{"id":"chatcmpl_1","model":"gpt-4o","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_0","type":"function","function":{"name":"first_tool"}}]}}]}"#,
                r#"{"id":"chatcmpl_1","model":"gpt-4o","choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_1","type":"function","function":{"name":"second_tool"}}]}}]}"#,
                r#"{"id":"chatcmpl_1","model":"gpt-4o","choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"{\"b\":2}"}}]}}]}"#,
                r#"{"id":"chatcmpl_1","model":"gpt-4o","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"a\":1}"}}]}}]}"#,
                r#"{"id":"chatcmpl_1","model":"gpt-4o","choices":[{"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":8,"completion_tokens":4}}"#,
                "[DONE]",
            ],
            false,
        );

        let tool_indices: HashMap<String, u64> = events
            .iter()
            .filter(|event| event.get("type").and_then(Value::as_str) == Some("content_block_start"))
            .filter(|event| event.pointer("/content_block/type").and_then(Value::as_str) == Some("tool_use"))
            .filter_map(|event| {
                Some((
                    event.pointer("/content_block/id")?.as_str()?.to_string(),
                    event.get("index")?.as_u64()?,
                ))
            })
            .collect();

        assert_eq!(tool_indices.len(), 2);
        assert_ne!(tool_indices.get("call_0"), tool_indices.get("call_1"));
    }

    #[test]
    fn stream_routes_tool_call_deltas_by_index() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_0\",\"type\":\"function\",\"function\":{\"name\":\"first_tool\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"second_tool\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"arguments\":\"{\\\"b\\\":2}\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":1}\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":4}}\n\n",
            "data: [DONE]\n\n"
        );

        let events = collect_stream_events(input);

        let tool_indices: HashMap<String, u64> = events
            .iter()
            .filter(|event| event_type(event) == Some("content_block_start"))
            .filter(|event| {
                event.pointer("/content_block/type").and_then(Value::as_str) == Some("tool_use")
            })
            .filter_map(|event| {
                Some((
                    event.pointer("/content_block/id")?.as_str()?.to_string(),
                    event.get("index")?.as_u64()?,
                ))
            })
            .collect();

        assert_eq!(tool_indices.len(), 2);

        let deltas: Vec<(u64, String)> = events
            .iter()
            .filter(|event| event_type(event) == Some("content_block_delta"))
            .filter(|event| {
                event.pointer("/delta/type").and_then(Value::as_str)
                    == Some("input_json_delta")
            })
            .filter_map(|event| {
                Some((
                    event.get("index")?.as_u64()?,
                    event.pointer("/delta/partial_json")?.as_str()?.to_string(),
                ))
            })
            .collect();

        assert_eq!(deltas.len(), 2);
        let second_idx = deltas
            .iter()
            .find_map(|(index, payload)| (payload == "{\"b\":2}").then_some(*index))
            .unwrap();
        let first_idx = deltas
            .iter()
            .find_map(|(index, payload)| (payload == "{\"a\":1}").then_some(*index))
            .unwrap();

        assert_eq!(second_idx, *tool_indices.get("call_1").unwrap());
        assert_eq!(first_idx, *tool_indices.get("call_0").unwrap());
        assert!(events.iter().any(|event| {
            event_type(event) == Some("message_delta")
                && event.pointer("/delta/stop_reason").and_then(Value::as_str) == Some("tool_use")
        }));
    }

    #[test]
    fn stream_delays_tool_start_until_id_and_name_are_ready() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_0\",\"type\":\"function\",\"function\":{\"name\":\"first_tool\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"1}\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":6,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n"
        );

        let events = collect_stream_events(input);
        let starts: Vec<&Value> = events
            .iter()
            .filter(|event| event_type(event) == Some("content_block_start"))
            .filter(|event| {
                event.pointer("/content_block/type").and_then(Value::as_str) == Some("tool_use")
            })
            .collect();

        assert_eq!(starts.len(), 1);
        assert_eq!(
            starts[0].pointer("/content_block/id").and_then(Value::as_str),
            Some("call_0")
        );
        assert_eq!(
            starts[0]
                .pointer("/content_block/name")
                .and_then(Value::as_str),
            Some("first_tool")
        );

        let deltas: Vec<&str> = events
            .iter()
            .filter(|event| event_type(event) == Some("content_block_delta"))
            .filter(|event| {
                event.pointer("/delta/type").and_then(Value::as_str)
                    == Some("input_json_delta")
            })
            .filter_map(|event| event.pointer("/delta/partial_json").and_then(Value::as_str))
            .collect();

        assert!(deltas.contains(&"{\"a\":"));
        assert!(deltas.contains(&"1}"));
    }

    #[test]
    fn stream_preserves_multibyte_text_split_across_chunks() {
        let full = concat!(
            "data: {\"id\":\"chatcmpl_3\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_3\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n"
        );
        let bytes = full.as_bytes();
        let ni_start = bytes.windows(3).position(|w| w == "你".as_bytes()).unwrap();
        let split_point = ni_start + 1;

        let output = collect_stream_output(vec![
            Ok(Bytes::from(bytes[..split_point].to_vec())),
            Ok(Bytes::from(bytes[split_point..].to_vec())),
        ]);

        assert!(output.contains("你好"));
        assert!(!output.contains('\u{FFFD}'));
    }

    #[test]
    fn duplicate_finish_reason_emits_only_one_terminal_delta() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_dup\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"id\":\"chatcmpl_dup\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n",
            "data: [DONE]\n\n"
        );

        let events = collect_stream_events(input);
        let message_deltas: Vec<&Value> = events
            .iter()
            .filter(|event| event_type(event) == Some("message_delta"))
            .collect();

        assert_eq!(message_deltas.len(), 1);
        assert_eq!(
            message_deltas[0]
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64),
            Some(10)
        );
        assert_eq!(
            message_deltas[0]
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64),
            Some(5)
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event_type(event) == Some("message_stop"))
                .count(),
            1
        );
    }

    #[test]
    fn usage_only_chunk_after_finish_updates_pending_delta() {
        let events = collect_events(
            &[
                r#"{"id":"chatcmpl_split","model":"glm-5.1","choices":[{"delta":{"tool_calls":[{"index":0,"id":"tool-0924","type":"function","function":{"name":"Bash","arguments":"{\"command\":\"pwd\"}"}}]}}]}"#,
                r#"{"id":"chatcmpl_split","model":"glm-5.1","choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
                r#"{"choices":[],"usage":{"prompt_tokens":13312,"completion_tokens":79,"prompt_tokens_details":{"cached_tokens":100}}}"#,
                "[DONE]",
            ],
            false,
        );

        let message_delta = events
            .iter()
            .find(|event| event.get("type").and_then(Value::as_str) == Some("message_delta"))
            .expect("message_delta");

        assert_eq!(
            message_delta
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64),
            Some(13212)
        );
        assert_eq!(
            message_delta
                .pointer("/usage/cache_read_input_tokens")
                .and_then(Value::as_u64),
            Some(100)
        );
    }

    #[test]
    fn usage_only_stream_chunk_after_finish_updates_pending_delta() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_split\",\"model\":\"glm-5.1\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"tool-0924\",\"type\":\"function\",\"function\":{\"name\":\"Bash\",\"arguments\":\"{\\\"command\\\":\\\"pwd\\\"}\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_split\",\"model\":\"glm-5.1\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":13312,\"completion_tokens\":79,\"prompt_tokens_details\":{\"cached_tokens\":100}}}\n\n",
            "data: [DONE]\n\n"
        );

        let events = collect_stream_events(input);
        let message_deltas: Vec<&Value> = events
            .iter()
            .filter(|event| event_type(event) == Some("message_delta"))
            .collect();

        assert_eq!(message_deltas.len(), 1);
        let message_delta = message_deltas[0];
        assert_eq!(
            message_delta
                .pointer("/delta/stop_reason")
                .and_then(Value::as_str),
            Some("tool_use")
        );
        assert_eq!(
            message_delta
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64),
            Some(13212)
        );
        assert_eq!(
            message_delta
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64),
            Some(79)
        );
        assert_eq!(
            message_delta
                .pointer("/usage/cache_read_input_tokens")
                .and_then(Value::as_u64),
            Some(100)
        );
    }

    #[test]
    fn usage_chunk_subtracts_cache_buckets_from_input() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_cc\",\"model\":\"glm-5.1\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"tool-1\",\"type\":\"function\",\"function\":{\"name\":\"Bash\",\"arguments\":\"{\\\"command\\\":\\\"pwd\\\"}\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_cc\",\"model\":\"glm-5.1\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":1000,\"completion_tokens\":50,\"prompt_tokens_details\":{\"cached_tokens\":600},\"cache_creation_input_tokens\":300}}\n\n",
            "data: [DONE]\n\n"
        );

        let events = collect_stream_events(input);
        let message_delta = events
            .iter()
            .find(|event| event_type(event) == Some("message_delta"))
            .expect("message_delta");

        assert_eq!(
            message_delta
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64),
            Some(100)
        );
        assert_eq!(
            message_delta
                .pointer("/usage/cache_read_input_tokens")
                .and_then(Value::as_u64),
            Some(600)
        );
        assert_eq!(
            message_delta
                .pointer("/usage/cache_creation_input_tokens")
                .and_then(Value::as_u64),
            Some(300)
        );
    }

    #[test]
    fn usage_chunk_clamps_fresh_input_when_cache_exceeds_prompt() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_uf\",\"model\":\"glm-5.1\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"tool-1\",\"type\":\"function\",\"function\":{\"name\":\"Bash\",\"arguments\":\"{\\\"command\\\":\\\"pwd\\\"}\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_uf\",\"model\":\"glm-5.1\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":50,\"prompt_tokens_details\":{\"cached_tokens\":80},\"cache_creation_input_tokens\":50}}\n\n",
            "data: [DONE]\n\n"
        );

        let events = collect_stream_events(input);
        let message_delta = events
            .iter()
            .find(|event| event_type(event) == Some("message_delta"))
            .expect("message_delta");

        assert_eq!(
            message_delta
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            message_delta
                .pointer("/usage/cache_read_input_tokens")
                .and_then(Value::as_u64),
            Some(80)
        );
        assert_eq!(
            message_delta
                .pointer("/usage/cache_creation_input_tokens")
                .and_then(Value::as_u64),
            Some(50)
        );
    }

    #[test]
    fn message_delta_includes_zero_usage_when_stream_has_no_usage() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_no_usage\",\"model\":\"gpt-5.5\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_0\",\"type\":\"function\",\"function\":{\"name\":\"get_time\",\"arguments\":\"{}\"}}]}}]}\n\n",
            "data: {\"id\":\"chatcmpl_no_usage\",\"model\":\"gpt-5.5\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );

        let events = collect_stream_events(input);
        let message_deltas: Vec<&Value> = events
            .iter()
            .filter(|event| event_type(event) == Some("message_delta"))
            .collect();

        assert_eq!(message_deltas.len(), 1);
        let message_delta = message_deltas[0];
        assert_eq!(
            message_delta
                .pointer("/delta/stop_reason")
                .and_then(Value::as_str),
            Some("tool_use")
        );
        assert_eq!(
            message_delta
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            message_delta
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64),
            Some(0)
        );
    }

    #[test]
    fn stream_finalizes_after_finish_when_done_is_missing() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_no_done\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_no_done\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n"
        );

        let events = collect_stream_events(input);

        assert!(events.iter().any(|event| {
            event_type(event) == Some("message_delta")
                && event.pointer("/delta/stop_reason").and_then(Value::as_str) == Some("end_turn")
        }));
        assert_eq!(events.last().and_then(|event| event_type(event)), Some("message_stop"));
    }

    #[test]
    fn stream_end_without_finish_reason_does_not_emit_success_terminal_events() {
        let events = collect_events(
            &[r#"{"id":"chatcmpl_truncated","model":"gpt-4o","choices":[{"delta":{"content":"hello"}}]}"#],
            true,
        );

        assert!(!events
            .iter()
            .any(|event| event.get("type").and_then(Value::as_str) == Some("message_delta")));
        assert!(!events
            .iter()
            .any(|event| event.get("type").and_then(Value::as_str) == Some("message_stop")));
    }

    #[test]
    fn byte_stream_end_without_finish_reason_does_not_emit_success_terminal_events() {
        let input = "data: {\"id\":\"chatcmpl_truncated\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\n";

        let events = collect_stream_events(input);

        assert!(!events
            .iter()
            .any(|event| event_type(event) == Some("message_delta")));
        assert!(!events
            .iter()
            .any(|event| event_type(event) == Some("message_stop")));
    }

    #[test]
    fn stream_error_does_not_emit_success_terminal_events() {
        let output = collect_stream_output(vec![Err(io::Error::other("upstream disconnected"))]);

        let events: Vec<Value> = output
            .split("\n\n")
            .filter_map(|block| {
                let data = block
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))?;
                serde_json::from_str::<Value>(data).ok()
            })
            .collect();

        assert!(events.iter().any(|event| event_type(event) == Some("error")));
        assert!(!events
            .iter()
            .any(|event| event_type(event) == Some("message_delta")));
        assert!(!events
            .iter()
            .any(|event| event_type(event) == Some("message_stop")));
    }
}
