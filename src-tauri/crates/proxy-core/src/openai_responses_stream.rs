//! OpenAI Responses SSE -> Anthropic SSE state machine.

use crate::{
    response_transform::{
        build_anthropic_message_delta_event, map_openai_responses_stop_reason_to_anthropic,
        sanitize_anthropic_tool_use_input_json,
    },
    usage::build_anthropic_usage_from_openai_responses,
};
use bytes::Bytes;
use futures::{stream as futures_stream, Stream, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    io,
    pin::Pin,
};

#[derive(Debug, Default)]
pub struct OpenAiResponsesToAnthropicSseState {
    message_id: Option<String>,
    current_model: Option<String>,
    has_sent_message_start: bool,
    has_tool_use: bool,
    next_content_index: u32,
    index_by_key: HashMap<String, u32>,
    open_indices: HashSet<u32>,
    fallback_open_index: Option<u32>,
    current_text_index: Option<u32>,
    tool_index_by_item_id: HashMap<String, u32>,
    tool_name_by_index: HashMap<u32, String>,
    tool_args_by_index: HashMap<u32, String>,
    last_tool_index: Option<u32>,
}

struct OpenAiResponsesToAnthropicSseStreamContext<S> {
    stream: Pin<Box<S>>,
    buffer: String,
    utf8_remainder: Vec<u8>,
    state: OpenAiResponsesToAnthropicSseState,
    pending_events: VecDeque<Bytes>,
    finished: bool,
}

/// Convert an OpenAI Responses SSE byte stream into Anthropic SSE bytes.
///
/// This owns byte/SSE transport mechanics and delegates protocol conversion to
/// `OpenAiResponsesToAnthropicSseState`.
pub fn create_openai_responses_to_anthropic_sse_stream<S, E>(
    stream: S,
) -> impl Stream<Item = Result<Bytes, io::Error>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: Error + Send + 'static,
{
    let context = OpenAiResponsesToAnthropicSseStreamContext {
        stream: Box::pin(stream),
        buffer: String::new(),
        utf8_remainder: Vec::new(),
        state: OpenAiResponsesToAnthropicSseState::new(),
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

                        let mut event_type: Option<String> = None;
                        let mut data_parts: Vec<String> = Vec::new();

                        for line in block.lines() {
                            if let Some(event) = crate::strip_sse_field(line, "event") {
                                event_type = Some(event.trim().to_string());
                            } else if let Some(data) = crate::strip_sse_field(line, "data") {
                                data_parts.push(data.to_string());
                            }
                        }

                        if data_parts.is_empty() {
                            continue;
                        }

                        let Ok(data) = serde_json::from_str::<Value>(&data_parts.join("\n"))
                        else {
                            continue;
                        };
                        let event_name = event_type.as_deref().unwrap_or("");
                        context
                            .pending_events
                            .extend(context.state.handle_event(event_name, &data));
                    }
                }
                Some(Err(error)) => {
                    context.finished = true;
                    return Some((
                        Ok(OpenAiResponsesToAnthropicSseState::stream_error_event(
                            format!("Stream error: {error}"),
                        )),
                        context,
                    ));
                }
                None => {
                    return None;
                }
            }
        }
    })
}

impl OpenAiResponsesToAnthropicSseState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_event(&mut self, event_name: &str, data: &Value) -> Vec<Bytes> {
        let mut events = Vec::new();

        match event_name {
            "response.created" => {
                let response_obj = response_object_from_event(data);
                if let Some(id) = response_obj.get("id").and_then(|value| value.as_str()) {
                    self.message_id = Some(id.to_string());
                }
                if let Some(model) = response_obj.get("model").and_then(|value| value.as_str()) {
                    self.current_model = Some(model.to_string());
                }

                self.has_sent_message_start = true;
                let start_usage = build_anthropic_usage_from_openai_responses(Some(
                    response_obj.get("usage").unwrap_or(&json!({})),
                ));
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
            }
            "response.content_part.added" => {
                self.ensure_message_start(&mut events);

                if let Some(part) = data.get("part") {
                    let part_type = part.get("type").and_then(|value| value.as_str());
                    if matches!(part_type, Some("output_text") | Some("refusal")) {
                        let index = if let Some(index) = self.current_text_index {
                            index
                        } else {
                            let index = self.resolve_content_index(data);
                            self.current_text_index = Some(index);
                            index
                        };

                        if self.open_indices.contains(&index) {
                            return events;
                        }

                        events.push(content_block_start(index, json!({
                            "type": "text",
                            "text": ""
                        })));
                        self.open_indices.insert(index);
                    }
                }
            }
            "response.output_text.delta" | "response.refusal.delta" => {
                if let Some(delta) = data.get("delta").and_then(|value| value.as_str()) {
                    let index = if let Some(index) = self.current_text_index {
                        index
                    } else {
                        let index = self.resolve_content_index(data);
                        self.current_text_index = Some(index);
                        index
                    };

                    self.ensure_text_block(index, &mut events);
                    events.push(encode_sse(
                        "content_block_delta",
                        &json!({
                            "type": "content_block_delta",
                            "index": index,
                            "delta": {
                                "type": "text_delta",
                                "text": delta
                            }
                        }),
                    ));
                }
            }
            "response.content_part.done" => {}
            "response.output_item.added" => {
                let Some(item) = data.get("item") else {
                    return events;
                };
                let item_type = item.get("type").and_then(|value| value.as_str()).unwrap_or("");
                if item_type != "function_call" {
                    return events;
                }

                self.has_tool_use = true;
                self.close_current_text_block(&mut events);
                self.ensure_message_start(&mut events);

                let call_id = item.get("call_id").and_then(|value| value.as_str()).unwrap_or("");
                let name = item.get("name").and_then(|value| value.as_str()).unwrap_or("");
                let index = if let Some(key) = tool_item_key_from_added(data, item) {
                    if let Some(existing) = self.index_by_key.get(&key).copied() {
                        existing
                    } else {
                        let assigned = self.next_content_index;
                        self.next_content_index += 1;
                        self.index_by_key.insert(key, assigned);
                        assigned
                    }
                } else {
                    let assigned = self.next_content_index;
                    self.next_content_index += 1;
                    assigned
                };

                if let Some(item_id) = item
                    .get("id")
                    .and_then(|value| value.as_str())
                    .or_else(|| data.get("item_id").and_then(|value| value.as_str()))
                {
                    self.tool_index_by_item_id
                        .insert(item_id.to_string(), index);
                }
                self.tool_name_by_index.insert(index, name.to_string());
                self.last_tool_index = Some(index);

                if self.open_indices.contains(&index) {
                    return events;
                }

                self.tool_args_by_index.insert(index, String::new());
                events.push(content_block_start(index, json!({
                    "type": "tool_use",
                    "id": call_id,
                    "name": name
                })));
                self.open_indices.insert(index);
            }
            "response.function_call_arguments.delta" => {
                if let Some(delta) = data.get("delta").and_then(|value| value.as_str()) {
                    let item_id = data.get("item_id").and_then(|value| value.as_str());
                    let index = self
                        .tool_index_for_event(data, item_id)
                        .unwrap_or_else(|| self.allocate_content_index());

                    if !self.open_indices.contains(&index) {
                        events.push(content_block_start(index, json!({
                            "type": "tool_use",
                            "id": data
                                .get("call_id")
                                .and_then(|value| value.as_str())
                                .or(item_id)
                                .unwrap_or(""),
                            "name": data
                                .get("name")
                                .and_then(|value| value.as_str())
                                .unwrap_or("")
                        })));
                        self.open_indices.insert(index);
                    }

                    if self.tool_name_by_index.get(&index).map(String::as_str) == Some("Read") {
                        self.tool_args_by_index
                            .entry(index)
                            .or_default()
                            .push_str(delta);
                        return events;
                    }

                    events.push(encode_sse(
                        "content_block_delta",
                        &json!({
                            "type": "content_block_delta",
                            "index": index,
                            "delta": {
                                "type": "input_json_delta",
                                "partial_json": delta
                            }
                        }),
                    ));
                }
            }
            "response.function_call_arguments.done" => {
                let item_id = data.get("item_id").and_then(|value| value.as_str());
                let Some(index) = self.tool_index_for_event(data, item_id) else {
                    return events;
                };
                if !self.open_indices.remove(&index) {
                    return events;
                }

                if self.tool_name_by_index.get(&index).map(String::as_str) == Some("Read") {
                    let raw = data
                        .get("arguments")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            self.tool_args_by_index
                                .get(&index)
                                .cloned()
                                .unwrap_or_default()
                        });
                    let sanitized = sanitize_anthropic_tool_use_input_json("Read", &raw);
                    if !sanitized.is_empty() {
                        events.push(encode_sse(
                            "content_block_delta",
                            &json!({
                                "type": "content_block_delta",
                                "index": index,
                                "delta": {
                                    "type": "input_json_delta",
                                    "partial_json": sanitized
                                }
                            }),
                        ));
                    }
                }

                events.push(content_block_stop(index));
                if let Some(item_id) = item_id {
                    self.tool_index_by_item_id.remove(item_id);
                }
                self.tool_name_by_index.remove(&index);
                self.tool_args_by_index.remove(&index);
            }
            "response.refusal.done" => {
                let index = self.current_text_index.take().or_else(|| {
                    content_part_key(data)
                        .and_then(|key| self.index_by_key.get(&key).copied())
                        .or(self.fallback_open_index)
                });
                if let Some(index) = index {
                    if !self.open_indices.remove(&index) {
                        return events;
                    }
                    events.push(content_block_stop(index));
                    if self.fallback_open_index == Some(index) {
                        self.fallback_open_index = None;
                    }
                }
            }
            "response.reasoning.delta" => {
                if let Some(delta) = data
                    .get("delta")
                    .or_else(|| data.get("text"))
                    .and_then(|value| value.as_str())
                {
                    self.close_current_text_block(&mut events);
                    let index = self.resolve_content_index(data);

                    if !self.open_indices.contains(&index) {
                        events.push(content_block_start(index, json!({
                            "type": "thinking",
                            "thinking": ""
                        })));
                        self.open_indices.insert(index);
                    }

                    events.push(encode_sse(
                        "content_block_delta",
                        &json!({
                            "type": "content_block_delta",
                            "index": index,
                            "delta": {
                                "type": "thinking_delta",
                                "thinking": delta
                            }
                        }),
                    ));
                }
            }
            "response.reasoning.done" => {
                let index = content_part_key(data)
                    .and_then(|key| self.index_by_key.get(&key).copied())
                    .or(self.fallback_open_index);
                if let Some(index) = index {
                    if !self.open_indices.remove(&index) {
                        return events;
                    }
                    events.push(content_block_stop(index));
                    if self.fallback_open_index == Some(index) {
                        self.fallback_open_index = None;
                    }
                }
            }
            "response.completed" => {
                let response_obj = response_object_from_event(data);
                let stop_reason = map_openai_responses_stop_reason_to_anthropic(
                    response_obj.get("status").and_then(|value| value.as_str()),
                    self.has_tool_use,
                    response_obj
                        .pointer("/incomplete_details/reason")
                        .and_then(|value| value.as_str()),
                );

                self.close_all_open_blocks(&mut events);
                self.fallback_open_index = None;

                let usage_json = build_anthropic_usage_from_openai_responses(Some(
                    response_obj.get("usage").unwrap_or(&json!({})),
                ));
                let delta_event =
                    build_anthropic_message_delta_event(stop_reason, Some(usage_json));
                events.push(encode_sse("message_delta", &delta_event));
                events.push(encode_sse("message_stop", &json!({"type": "message_stop"})));
            }
            "response.output_text.done" => {
                if let Some(index) = self.current_text_index.take() {
                    if self.open_indices.remove(&index) {
                        events.push(content_block_stop(index));
                    }
                    if self.fallback_open_index == Some(index) {
                        self.fallback_open_index = None;
                    }
                }
            }
            "response.output_item.done" | "response.in_progress" => {}
            _ => {}
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

    fn ensure_message_start(&mut self, events: &mut Vec<Bytes>) {
        if self.has_sent_message_start {
            return;
        }

        events.push(encode_sse(
            "message_start",
            &json!({
                "type": "message_start",
                "message": {
                    "id": self.message_id.clone().unwrap_or_default(),
                    "type": "message",
                    "role": "assistant",
                    "model": self.current_model.clone().unwrap_or_default(),
                    "usage": { "input_tokens": 0, "output_tokens": 0 }
                }
            }),
        ));
        self.has_sent_message_start = true;
    }

    fn ensure_text_block(&mut self, index: u32, events: &mut Vec<Bytes>) {
        if self.open_indices.contains(&index) {
            return;
        }

        events.push(content_block_start(index, json!({
            "type": "text",
            "text": ""
        })));
        self.open_indices.insert(index);
    }

    fn close_current_text_block(&mut self, events: &mut Vec<Bytes>) {
        let Some(index) = self.current_text_index.take() else {
            return;
        };

        if self.open_indices.remove(&index) {
            events.push(content_block_stop(index));
        }
        if self.fallback_open_index == Some(index) {
            self.fallback_open_index = None;
        }
    }

    fn close_all_open_blocks(&mut self, events: &mut Vec<Bytes>) {
        if self.open_indices.is_empty() {
            return;
        }

        let mut remaining: Vec<u32> = self.open_indices.iter().copied().collect();
        remaining.sort_unstable();
        for index in remaining {
            events.push(content_block_stop(index));
            self.open_indices.remove(&index);
        }
    }

    fn resolve_content_index(&mut self, data: &Value) -> u32 {
        if let Some(key) = content_part_key(data) {
            if let Some(existing) = self.index_by_key.get(&key).copied() {
                existing
            } else {
                let assigned = self.allocate_content_index();
                self.index_by_key.insert(key, assigned);
                assigned
            }
        } else if let Some(existing) = self.fallback_open_index {
            existing
        } else {
            let assigned = self.allocate_content_index();
            self.fallback_open_index = Some(assigned);
            assigned
        }
    }

    fn allocate_content_index(&mut self) -> u32 {
        let assigned = self.next_content_index;
        self.next_content_index += 1;
        assigned
    }

    fn tool_index_for_event(&self, data: &Value, item_id: Option<&str>) -> Option<u32> {
        item_id
            .and_then(|id| self.tool_index_by_item_id.get(id).copied())
            .or_else(|| {
                tool_item_key_from_event(data).and_then(|key| self.index_by_key.get(&key).copied())
            })
            .or(self.last_tool_index)
    }
}

fn response_object_from_event(data: &Value) -> &Value {
    data.get("response").unwrap_or(data)
}

fn content_part_key(data: &Value) -> Option<String> {
    if let (Some(item_id), Some(content_index)) = (
        data.get("item_id").and_then(|value| value.as_str()),
        data.get("content_index").and_then(|value| value.as_u64()),
    ) {
        return Some(format!("part:{item_id}:{content_index}"));
    }
    if let (Some(output_index), Some(content_index)) = (
        data.get("output_index").and_then(|value| value.as_u64()),
        data.get("content_index").and_then(|value| value.as_u64()),
    ) {
        return Some(format!("part:out:{output_index}:{content_index}"));
    }
    None
}

fn tool_item_key_from_added(data: &Value, item: &Value) -> Option<String> {
    if let Some(item_id) = item.get("id").and_then(|value| value.as_str()) {
        return Some(format!("tool:{item_id}"));
    }
    if let Some(item_id) = data.get("item_id").and_then(|value| value.as_str()) {
        return Some(format!("tool:{item_id}"));
    }
    if let Some(output_index) = data.get("output_index").and_then(|value| value.as_u64()) {
        return Some(format!("tool:out:{output_index}"));
    }
    None
}

fn tool_item_key_from_event(data: &Value) -> Option<String> {
    if let Some(item_id) = data.get("item_id").and_then(|value| value.as_str()) {
        return Some(format!("tool:{item_id}"));
    }
    if let Some(output_index) = data.get("output_index").and_then(|value| value.as_u64()) {
        return Some(format!("tool:out:{output_index}"));
    }
    None
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

fn encode_sse(event_name: &str, event: &Value) -> Bytes {
    Bytes::from(format!(
        "event: {event_name}\ndata: {}\n\n",
        serde_json::to_string(event).unwrap_or_default()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_events(events: &[(&str, Value)]) -> String {
        let mut state = OpenAiResponsesToAnthropicSseState::new();
        events
            .iter()
            .flat_map(|(event, data)| state.handle_event(event, data))
            .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).to_string())
            .collect()
    }

    fn collect_stream_output(chunks: Vec<Result<Bytes, io::Error>>) -> String {
        futures::executor::block_on(async move {
            let upstream = futures_stream::iter(chunks);
            let converted = create_openai_responses_to_anthropic_sse_stream(upstream);
            let chunks: Vec<_> = converted.collect().await;

            chunks
                .into_iter()
                .map(|chunk| String::from_utf8_lossy(chunk.unwrap().as_ref()).to_string())
                .collect()
        })
    }

    fn collect_stream_events(input: &str) -> Vec<Value> {
        let output = collect_stream_output(vec![Ok(Bytes::from(input.as_bytes().to_vec()))]);
        parse_anthropic_events(&output)
    }

    fn parse_anthropic_events(output: &str) -> Vec<Value> {
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
    fn response_created_uses_wrapped_response_object() {
        let output = collect_events(&[(
            "response.created",
            json!({
                "type": "response.created",
                "response": {
                    "id": "resp_1",
                    "model": "gpt-4o",
                    "usage": {"input_tokens": 12, "output_tokens": 0}
                }
            }),
        )]);

        assert!(output.contains("\"id\":\"resp_1\""));
        assert!(output.contains("\"model\":\"gpt-4o\""));
        assert!(output.contains("\"input_tokens\":12"));
    }

    #[test]
    fn stream_conversion_with_wrapped_response_events() {
        let input = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-4o\",\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"get_weather\"}}\n\n",
            "event: response.function_call_arguments.delta\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{\\\"city\\\":\\\"Tokyo\\\"}\"}\n\n",
            "event: response.function_call_arguments.done\n",
            "data: {\"type\":\"response.function_call_arguments.done\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":12,\"output_tokens\":3}}}\n\n"
        );

        let output = collect_stream_output(vec![Ok(Bytes::from(input.as_bytes().to_vec()))]);

        assert!(output.contains("\"type\":\"message_start\""));
        assert!(output.contains("\"id\":\"resp_1\""));
        assert!(output.contains("\"model\":\"gpt-4o\""));
        assert!(output.contains("\"type\":\"tool_use\""));
        assert!(output.contains("\"name\":\"get_weather\""));
        assert!(output.contains("\"type\":\"input_json_delta\""));
        assert!(output.contains("\"stop_reason\":\"tool_use\""));
        assert!(output.contains("\"input_tokens\":12"));
        assert!(output.contains("\"output_tokens\":3"));
        assert!(output.contains("\"type\":\"message_stop\""));
    }

    #[test]
    fn routes_interleaved_tool_deltas_by_item_id() {
        let output = collect_events(&[
            (
                "response.created",
                json!({"response": {"id": "resp_2", "model": "gpt-4o"}}),
            ),
            (
                "response.output_item.added",
                json!({"item": {"id": "fc_1", "type": "function_call", "call_id": "call_1", "name": "first_tool"}}),
            ),
            (
                "response.output_item.added",
                json!({"item": {"id": "fc_2", "type": "function_call", "call_id": "call_2", "name": "second_tool"}}),
            ),
            (
                "response.function_call_arguments.delta",
                json!({"item_id": "fc_2", "delta": "{\"b\":2}"}),
            ),
            (
                "response.function_call_arguments.delta",
                json!({"item_id": "fc_1", "delta": "{\"a\":1}"}),
            ),
        ]);

        let call_1_start = output.find("\"id\":\"call_1\"").unwrap();
        let call_2_start = output.find("\"id\":\"call_2\"").unwrap();
        let b_delta = output.find("\"partial_json\":\"{\\\"b\\\":2}\"").unwrap();
        let a_delta = output.find("\"partial_json\":\"{\\\"a\\\":1}\"").unwrap();

        assert!(call_1_start < call_2_start);
        assert!(call_2_start < b_delta);
        assert!(call_1_start < a_delta);
    }

    #[test]
    fn stream_routes_interleaved_tool_deltas_by_item_id() {
        let input = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\",\"model\":\"gpt-4o\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"fc_1\",\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"first_tool\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"fc_2\",\"type\":\"function_call\",\"call_id\":\"call_2\",\"name\":\"second_tool\"}}\n\n",
            "event: response.function_call_arguments.delta\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_2\",\"delta\":\"{\\\"b\\\":2}\"}\n\n",
            "event: response.function_call_arguments.delta\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_1\",\"delta\":\"{\\\"a\\\":1}\"}\n\n",
            "event: response.function_call_arguments.done\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"item_id\":\"fc_1\"}\n\n",
            "event: response.function_call_arguments.done\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"item_id\":\"fc_2\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":8,\"output_tokens\":4}}}\n\n"
        );

        let events = collect_stream_events(input);
        let tool_index_by_call: HashMap<String, u64> = events
            .iter()
            .filter(|event| event_type(event) == Some("content_block_start"))
            .filter_map(|event| {
                let content_block = event.get("content_block")?;
                if content_block.get("type").and_then(Value::as_str) != Some("tool_use") {
                    return None;
                }
                Some((
                    content_block.get("id")?.as_str()?.to_string(),
                    event.get("index")?.as_u64()?,
                ))
            })
            .collect();

        let delta_indices: Vec<u64> = events
            .iter()
            .filter(|event| event_type(event) == Some("content_block_delta"))
            .filter(|event| {
                event.pointer("/delta/type").and_then(Value::as_str)
                    == Some("input_json_delta")
            })
            .filter_map(|event| event.get("index").and_then(Value::as_u64))
            .collect();

        assert_eq!(delta_indices.len(), 2);
        assert_eq!(delta_indices[0], *tool_index_by_call.get("call_2").unwrap());
        assert_eq!(delta_indices[1], *tool_index_by_call.get("call_1").unwrap());
        assert_ne!(
            tool_index_by_call.get("call_1"),
            tool_index_by_call.get("call_2")
        );
    }

    #[test]
    fn read_tool_done_sanitizes_buffered_empty_pages() {
        let output = collect_events(&[
            (
                "response.output_item.added",
                json!({"item": {"id": "fc_read", "type": "function_call", "call_id": "call_read", "name": "Read"}}),
            ),
            (
                "response.function_call_arguments.delta",
                json!({
                    "item_id": "fc_read",
                    "delta": "{\"file_path\":\"/tmp/demo.py\",\"limit\":2000,\"offset\":0,\"pages\":\"\"}"
                }),
            ),
            (
                "response.function_call_arguments.done",
                json!({"item_id": "fc_read"}),
            ),
        ]);

        assert!(output.contains("\"name\":\"Read\""));
        assert!(output.contains(
            "\"partial_json\":\"{\\\"file_path\\\":\\\"/tmp/demo.py\\\",\\\"limit\\\":2000,\\\"offset\\\":0}"
        ));
        assert!(!output.contains("\\\"pages\\\":\\\"\\\""));
    }

    #[test]
    fn stream_read_tool_drops_empty_pages() {
        let input = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_read\",\"model\":\"gpt-5.5\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"fc_read\",\"type\":\"function_call\",\"call_id\":\"call_read\",\"name\":\"Read\"}}\n\n",
            "event: response.function_call_arguments.delta\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_read\",\"delta\":\"{\\\"file_path\\\":\\\"/tmp/demo.py\\\",\\\"limit\\\":2000,\\\"offset\\\":0,\\\"pages\\\":\\\"\\\"}\"}\n\n",
            "event: response.function_call_arguments.done\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"item_id\":\"fc_read\",\"arguments\":\"{\\\"file_path\\\":\\\"/tmp/demo.py\\\",\\\"limit\\\":2000,\\\"offset\\\":0,\\\"pages\\\":\\\"\\\"}\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n"
        );

        let output = collect_stream_output(vec![Ok(Bytes::from(input.as_bytes().to_vec()))]);

        assert!(output.contains("\"name\":\"Read\""));
        assert!(output.contains(
            "\"partial_json\":\"{\\\"file_path\\\":\\\"/tmp/demo.py\\\",\\\"limit\\\":2000,\\\"offset\\\":0}"
        ));
        assert!(!output.contains("\\\"pages\\\":\\\"\\\""));
    }

    #[test]
    fn stream_read_tool_duplicate_start_preserves_buffered_args() {
        let input = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_read\",\"model\":\"gpt-5.5\"}}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"fc_read\",\"type\":\"function_call\",\"call_id\":\"call_read\",\"name\":\"Read\"}}\n\n",
            "event: response.function_call_arguments.delta\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_read\",\"delta\":\"{\\\"file_path\\\":\\\"/tmp/demo.py\\\",\\\"limit\\\":2000,\\\"offset\\\":0,\\\"pages\\\":\\\"\\\"}\"}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"fc_read\",\"type\":\"function_call\",\"call_id\":\"call_read\",\"name\":\"Read\"}}\n\n",
            "event: response.function_call_arguments.done\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"item_id\":\"fc_read\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n"
        );

        let output = collect_stream_output(vec![Ok(Bytes::from(input.as_bytes().to_vec()))]);

        assert_eq!(output.matches("event: content_block_start").count(), 1);
        assert_eq!(output.matches("event: content_block_stop").count(), 1);
        assert!(output.contains(
            "\"partial_json\":\"{\\\"file_path\\\":\\\"/tmp/demo.py\\\",\\\"limit\\\":2000,\\\"offset\\\":0}"
        ));
        assert!(!output.contains("\\\"pages\\\":\\\"\\\""));
    }

    #[test]
    fn stream_reasoning_delta_emits_thinking_blocks() {
        let input = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_r\",\"model\":\"o3\",\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
            "event: response.reasoning.delta\n",
            "data: {\"type\":\"response.reasoning.delta\",\"delta\":\"Let me think...\"}\n\n",
            "event: response.reasoning.done\n",
            "data: {\"type\":\"response.reasoning.done\"}\n\n",
            "event: response.content_part.added\n",
            "data: {\"type\":\"response.content_part.added\",\"part\":{\"type\":\"output_text\",\"text\":\"\"},\"output_index\":0,\"content_index\":0}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"42\",\"output_index\":0,\"content_index\":0}\n\n",
            "event: response.content_part.done\n",
            "data: {\"type\":\"response.content_part.done\",\"output_index\":0,\"content_index\":0}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":5,\"output_tokens\":10}}}\n\n"
        );

        let output = collect_stream_output(vec![Ok(Bytes::from(input.as_bytes().to_vec()))]);

        assert!(output.contains("\"type\":\"thinking\""));
        assert!(output.contains("\"type\":\"thinking_delta\""));
        assert!(output.contains("\"thinking\":\"Let me think...\""));
        assert!(output.contains("\"type\":\"text_delta\""));
        assert!(output.contains("\"text\":\"42\""));
        assert!(output.contains("\"stop_reason\":\"end_turn\""));
    }

    #[test]
    fn stream_text_parts_are_merged_into_one_text_block() {
        let input = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_merge\",\"model\":\"gpt-5.4\",\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
            "event: response.content_part.added\n",
            "data: {\"type\":\"response.content_part.added\",\"part\":{\"type\":\"output_text\",\"text\":\"\"},\"output_index\":0,\"content_index\":0}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"你\",\"output_index\":0,\"content_index\":0}\n\n",
            "event: response.content_part.done\n",
            "data: {\"type\":\"response.content_part.done\",\"output_index\":0,\"content_index\":0}\n\n",
            "event: response.content_part.added\n",
            "data: {\"type\":\"response.content_part.added\",\"part\":{\"type\":\"output_text\",\"text\":\"\"},\"output_index\":0,\"content_index\":1}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"好\",\"output_index\":0,\"content_index\":1}\n\n",
            "event: response.content_part.done\n",
            "data: {\"type\":\"response.content_part.done\",\"output_index\":0,\"content_index\":1}\n\n",
            "event: response.output_text.done\n",
            "data: {\"type\":\"response.output_text.done\",\"output_index\":0,\"content_index\":1}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":5,\"output_tokens\":2}}}\n\n"
        );

        let events = collect_stream_events(input);
        let text_starts = events
            .iter()
            .filter(|event| event_type(event) == Some("content_block_start"))
            .filter(|event| {
                event.pointer("/content_block/type").and_then(Value::as_str) == Some("text")
            })
            .count();
        let text_stops = events
            .iter()
            .filter(|event| event_type(event) == Some("content_block_stop"))
            .count();
        let text_deltas: Vec<String> = events
            .iter()
            .filter(|event| event_type(event) == Some("content_block_delta"))
            .filter(|event| event.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta"))
            .filter_map(|event| {
                event
                    .pointer("/delta/text")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .collect();

        assert_eq!(text_starts, 1);
        assert_eq!(text_stops, 1);
        assert_eq!(text_deltas, vec!["你".to_string(), "好".to_string()]);
    }

    #[test]
    fn stream_preserves_multibyte_text_split_across_chunks() {
        let full = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_cn\",\"model\":\"gpt-4o\",\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"你好世界\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":5,\"output_tokens\":4}}}\n\n"
        );
        let bytes = full.as_bytes();
        let ni_start = bytes.windows(3).position(|w| w == "你".as_bytes()).unwrap();
        let split_point = ni_start + 2;

        let output = collect_stream_output(vec![
            Ok(Bytes::from(bytes[..split_point].to_vec())),
            Ok(Bytes::from(bytes[split_point..].to_vec())),
        ]);

        assert!(output.contains("你好世界"));
        assert!(!output.contains('\u{FFFD}'));
    }
}
