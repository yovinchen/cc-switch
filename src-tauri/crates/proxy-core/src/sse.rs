use crate::{
    error::{ProxyCoreError, ProxyCoreResult},
    response_transform::extract_reasoning_field_text,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Instant};

#[inline]
pub fn strip_sse_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    line.strip_prefix(&format!("{field}: "))
        .or_else(|| line.strip_prefix(&format!("{field}:")))
}

#[inline]
pub fn take_sse_block(buffer: &mut String) -> Option<String> {
    let mut best: Option<(usize, usize)> = None;

    for (delimiter, len) in [("\r\n\r\n", 4usize), ("\n\n", 2usize)] {
        if let Some(pos) = buffer.find(delimiter) {
            if best.is_none_or(|(best_pos, _)| pos < best_pos) {
                best = Some((pos, len));
            }
        }
    }

    let (pos, len) = best?;
    let block = buffer[..pos].to_string();
    buffer.drain(..pos + len);
    Some(block)
}

/// Append raw bytes to a UTF-8 `String` buffer, correctly handling multi-byte
/// characters that are split across chunk boundaries.
///
/// `remainder` accumulates trailing bytes from the previous chunk that form an
/// incomplete UTF-8 sequence (at most 3 bytes under normal operation). On each
/// call the remainder is prepended to `new_bytes`, the longest valid UTF-8
/// prefix is appended to `buffer`, and any trailing incomplete bytes are saved
/// back into `remainder` for the next call.
///
/// A defensive guard discards `remainder` via lossy conversion if it ever
/// exceeds 3 bytes, which cannot happen with well-formed UTF-8 streams.
pub fn append_utf8_safe(buffer: &mut String, remainder: &mut Vec<u8>, new_bytes: &[u8]) {
    let (owned, bytes): (Option<Vec<u8>>, &[u8]) = if remainder.is_empty() {
        (None, new_bytes)
    } else if remainder.len() > 3 {
        buffer.push_str(&String::from_utf8_lossy(remainder));
        remainder.clear();
        (None, new_bytes)
    } else {
        let mut combined = std::mem::take(remainder);
        combined.extend_from_slice(new_bytes);
        (Some(combined), &[])
    };
    let input = owned.as_deref().unwrap_or(bytes);

    let mut pos = 0;
    loop {
        match std::str::from_utf8(&input[pos..]) {
            Ok(s) => {
                buffer.push_str(s);
                return;
            }
            Err(e) => {
                let valid_up_to = pos + e.valid_up_to();
                let valid_slice = &input[pos..valid_up_to];
                match std::str::from_utf8(valid_slice) {
                    Ok(valid) => buffer.push_str(valid),
                    Err(_) => buffer.push_str(&String::from_utf8_lossy(valid_slice)),
                }
                if let Some(invalid_len) = e.error_len() {
                    buffer.push('\u{FFFD}');
                    pos = valid_up_to + invalid_len;
                } else {
                    *remainder = input[valid_up_to..].to_vec();
                    return;
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SseDataEvent {
    pub data: String,
    pub parsed: Option<Value>,
    pub done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsePassthroughEventKind {
    Done,
    Collect,
    Data,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SsePassthroughEvent {
    pub data: String,
    pub parsed: Option<Value>,
    pub kind: SsePassthroughEventKind,
}

impl SsePassthroughEvent {
    pub fn log_message(&self, tag: &str) -> String {
        match self.kind {
            SsePassthroughEventKind::Done => format!("[{tag}] <<< SSE: [DONE]"),
            SsePassthroughEventKind::Collect if self.parsed.is_some() => {
                format!("[{tag}] <<< SSE 事件: {}", self.data)
            }
            SsePassthroughEventKind::Collect | SsePassthroughEventKind::Data => {
                format!("[{tag}] <<< SSE 数据: {}", self.data)
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct SseEventScanner {
    buffer: String,
    utf8_remainder: Vec<u8>,
}

impl SseEventScanner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_bytes<F>(&mut self, bytes: &[u8], mut should_parse: F) -> Vec<SseDataEvent>
    where
        F: FnMut(&str) -> bool,
    {
        append_utf8_safe(&mut self.buffer, &mut self.utf8_remainder, bytes);

        let mut events = Vec::new();
        while let Some(event_text) = take_sse_block(&mut self.buffer) {
            if event_text.trim().is_empty() {
                continue;
            }

            for line in event_text.lines() {
                if let Some(data) = strip_sse_field(line, "data") {
                    let done = data.trim() == "[DONE]";
                    let parsed = if done || !should_parse(data) {
                        None
                    } else {
                        serde_json::from_str::<Value>(data).ok()
                    };
                    events.push(SseDataEvent {
                        data: data.to_string(),
                        parsed,
                        done,
                    });
                }
            }
        }
        events
    }

    pub fn push_passthrough_bytes<F>(
        &mut self,
        bytes: &[u8],
        should_collect: F,
    ) -> Vec<SsePassthroughEvent>
    where
        F: FnMut(&str) -> bool,
    {
        self.push_bytes(bytes, should_collect)
            .into_iter()
            .map(|event| {
                let kind = if event.done {
                    SsePassthroughEventKind::Done
                } else if event.parsed.is_some() {
                    SsePassthroughEventKind::Collect
                } else {
                    SsePassthroughEventKind::Data
                };
                SsePassthroughEvent {
                    data: event.data,
                    parsed: event.parsed,
                    kind,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SseUsageSnapshot {
    pub events: Vec<Value>,
    pub first_token_ms: Option<u64>,
}

#[derive(Debug)]
pub struct SseUsageAccumulator {
    events: Vec<Value>,
    first_event_time: Option<Instant>,
    start_time: Instant,
    finished: bool,
}

impl SseUsageAccumulator {
    pub fn new(start_time: Instant) -> Self {
        Self {
            events: Vec::new(),
            first_event_time: None,
            start_time,
            finished: false,
        }
    }

    pub fn push(&mut self, event: Value) {
        if self.finished {
            return;
        }

        if self.first_event_time.is_none() {
            self.first_event_time = Some(Instant::now());
        }
        self.events.push(event);
    }

    pub fn finish(&mut self) -> Option<SseUsageSnapshot> {
        if self.finished {
            return None;
        }

        self.finished = true;
        Some(SseUsageSnapshot {
            events: std::mem::take(&mut self.events),
            first_token_ms: self
                .first_event_time
                .map(|time| time.saturating_duration_since(self.start_time).as_millis() as u64),
        })
    }
}

pub fn claude_stream_usage_event_filter(data: &str) -> bool {
    data.contains("\"message_start\"") || data.contains("\"message_delta\"")
}

pub fn openai_stream_usage_event_filter(data: &str) -> bool {
    data.contains("\"usage\"")
}

pub fn codex_stream_usage_event_filter(data: &str) -> bool {
    data.contains("\"response.completed\"") || data.contains("\"usage\"")
}

pub fn gemini_stream_usage_event_filter(data: &str) -> bool {
    data.contains("\"usageMetadata\"")
}

pub fn responses_sse_to_response_value(body: &str) -> ProxyCoreResult<Value> {
    let mut buffer = body.trim_start_matches('\u{feff}').to_string();
    let mut completed_response: Option<Value> = None;
    let mut output_items = Vec::new();

    let mut process_block = |block: &str, strict: bool| -> ProxyCoreResult<()> {
        if !strict && completed_response.is_some() {
            return Ok(());
        }
        let mut event_name = "";
        let mut data_lines: Vec<&str> = Vec::new();

        for line in block.lines() {
            let line = line.trim_start();
            if let Some(evt) = strip_sse_field(line, "event") {
                event_name = evt.trim();
            } else if let Some(data) = strip_sse_field(line, "data") {
                data_lines.push(data);
            }
        }

        if data_lines.is_empty() {
            return Ok(());
        }

        let data_str = data_lines.join("\n");
        if data_str.trim() == "[DONE]" {
            return Ok(());
        }

        let data: Value = match serde_json::from_str(&data_str) {
            Ok(value) => value,
            Err(_) if !strict => return Ok(()),
            Err(error) => {
                return Err(sse_aggregation_error(format!(
                    "Failed to parse upstream SSE event: {error}"
                )));
            }
        };

        match event_name {
            "response.output_item.done" => {
                if let Some(item) = data.get("item") {
                    output_items.push(item.clone());
                }
            }
            "response.completed" => {
                completed_response = Some(data.get("response").cloned().unwrap_or(data));
            }
            "response.failed" => {
                let message = data
                    .pointer("/response/error/message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("response.failed event received");
                return Err(sse_aggregation_error(message));
            }
            _ => {}
        }
        Ok(())
    };

    while let Some(block) = take_sse_block(&mut buffer) {
        process_block(&block, true)?;
    }
    process_block(&buffer, false)?;

    let mut response = completed_response
        .ok_or_else(|| sse_aggregation_error("No response.completed event in upstream SSE"))?;

    if !output_items.is_empty() {
        if let Some(obj) = response.as_object_mut() {
            obj.insert("output".to_string(), Value::Array(output_items));
        } else {
            return Err(sse_aggregation_error(
                "response.completed payload is not an object",
            ));
        }
    }

    Ok(response)
}

pub fn chat_sse_to_response_value<F>(body: &str, mut next_missing_id: F) -> ProxyCoreResult<Value>
where
    F: FnMut() -> String,
{
    let mut buffer = body.trim_start_matches('\u{feff}').to_string();

    let mut id = Value::Null;
    let mut created = Value::Null;
    let mut model = Value::Null;
    let mut content = String::new();
    let mut reasoning_content = String::new();
    let mut tool_calls: BTreeMap<usize, Value> = BTreeMap::new();
    let mut finish_reason = Value::Null;
    let mut usage = Value::Null;
    let mut saw_choice = false;
    let mut saw_done = false;

    let mut process_event =
        |event_name: &str, data_str: &str, strict: bool| -> ProxyCoreResult<()> {
            let trimmed = data_str.trim();
            if trimmed == "[DONE]" {
                saw_done = true;
                return Ok(());
            }
            if trimmed.is_empty() {
                return Ok(());
            }
            let chunk: Value = match serde_json::from_str(data_str) {
                Ok(value) => value,
                Err(_) if !strict => return Ok(()),
                Err(error) => {
                    return Err(sse_aggregation_error(format!(
                        "Failed to parse upstream SSE chunk: {error}"
                    )));
                }
            };

            if event_name.eq_ignore_ascii_case("error") {
                let message = chunk
                    .get("error")
                    .and_then(error_event_message)
                    .or_else(|| error_event_message(&chunk))
                    .unwrap_or_else(|| "upstream error event in SSE stream".to_string());
                return Err(sse_aggregation_error(message));
            }

            if let Some(message) = chunk
                .get("error")
                .filter(|error| !error.is_null())
                .and_then(error_event_message)
            {
                return Err(sse_aggregation_error(message));
            }

            for (slot, key) in [
                (&mut id, "id"),
                (&mut created, "created"),
                (&mut model, "model"),
            ] {
                if slot.is_null() {
                    if let Some(value) = chunk
                        .get(key)
                        .filter(|value| envelope_value_meaningful(value))
                    {
                        *slot = value.clone();
                    }
                }
            }

            if let Some(value) = chunk.get("usage").filter(|value| !value.is_null()) {
                usage = value.clone();
            }

            let Some(choice) = chunk
                .get("choices")
                .and_then(|choices| choices.as_array())
                .and_then(|choices| {
                    choices.iter().find(|choice| {
                        choice
                            .get("index")
                            .and_then(|index| index.as_u64())
                            .unwrap_or(0)
                            == 0
                    })
                })
            else {
                return Ok(());
            };

            saw_choice = true;

            if finish_reason.is_null() {
                if let Some(value) = choice.get("finish_reason").filter(|value| !value.is_null()) {
                    finish_reason = value.clone();
                }
            }

            let delta_nonempty = choice
                .get("delta")
                .and_then(|delta| delta.as_object())
                .is_some_and(|delta| !delta.is_empty());
            let (payload, is_full_message) = if delta_nonempty {
                (choice.get("delta").unwrap(), false)
            } else if let Some(message) = choice.get("message") {
                (message, true)
            } else if let Some(delta) = choice.get("delta") {
                (delta, false)
            } else {
                return Ok(());
            };

            if is_full_message {
                content.clear();
                reasoning_content.clear();
                tool_calls.clear();
            }

            match payload.get("content") {
                Some(Value::String(text)) => content.push_str(text),
                Some(Value::Array(parts)) => {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                            content.push_str(text);
                        } else if let Some(refusal) =
                            part.get("refusal").and_then(|value| value.as_str())
                        {
                            content.push_str(refusal);
                        }
                    }
                }
                _ => {}
            }

            if let Some(refusal) = payload.get("refusal").and_then(|value| value.as_str()) {
                content.push_str(refusal);
            }
            if let Some(text) = extract_reasoning_field_text(payload) {
                reasoning_content.push_str(&text);
            }
            if let Some(deltas) = payload.get("tool_calls").and_then(|value| value.as_array()) {
                for (pos, tool_call) in deltas.iter().enumerate() {
                    merge_tool_call_delta(&mut tool_calls, tool_call, pos);
                }
            } else if let Some(function_call) = payload
                .get("function_call")
                .filter(|value| !value.is_null())
            {
                let synthetic = json!({
                    "index": 0,
                    "id": function_call
                        .get("id")
                        .and_then(|value| value.as_str())
                        .unwrap_or(""),
                    "type": "function",
                    "function": function_call,
                });
                merge_tool_call_delta(&mut tool_calls, &synthetic, 0);
            }
            Ok(())
        };

    while let Some(block) = take_sse_block(&mut buffer) {
        if let Some((event, data)) = sse_block_parts(&block) {
            process_event(&event, &data, true)?;
        }
    }
    if let Some((event, data)) = sse_block_parts(&buffer) {
        process_event(&event, &data, false)?;
    }

    if !saw_choice {
        return Err(sse_aggregation_error(
            "No chat completion choices in upstream SSE",
        ));
    }
    if finish_reason.is_null() && !saw_done {
        return Err(sse_aggregation_error(
            "Upstream SSE stream appears truncated (no finish_reason or [DONE] marker)",
        ));
    }

    let tool_calls: Vec<Value> = tool_calls
        .into_iter()
        .filter(|(_, tool_call)| {
            tool_call["id"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
                || tool_call["function"]["name"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
                || tool_call["function"]["arguments"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
        })
        .map(|(index, mut tool_call)| {
            if tool_call["id"].as_str().is_none_or(str::is_empty) {
                tool_call["id"] = json!(format!("tool_call_{index}"));
            }
            if tool_call["function"]["name"]
                .as_str()
                .is_none_or(str::is_empty)
            {
                tool_call["function"]["name"] = json!("unknown_tool");
            }
            tool_call
        })
        .collect();

    let mut message = serde_json::Map::new();
    message.insert("role".to_string(), json!("assistant"));
    message.insert("content".to_string(), json!(content));
    if !reasoning_content.is_empty() {
        message.insert("reasoning_content".to_string(), json!(reasoning_content));
    }
    if !tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(tool_calls));
    }

    let id = if envelope_value_meaningful(&id) {
        id
    } else {
        let generated = next_missing_id();
        if generated.is_empty() {
            json!("cc-switch-generated-chatcmpl")
        } else {
            json!(generated)
        }
    };

    let mut response = json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": finish_reason,
        }],
    });
    if !usage.is_null() {
        response["usage"] = usage;
    }
    Ok(response)
}

fn sse_aggregation_error(message: impl Into<String>) -> ProxyCoreError {
    ProxyCoreError::Upstream(message.into())
}

fn sse_block_parts(block: &str) -> Option<(String, String)> {
    let mut event_name = String::new();
    let mut data_lines: Vec<&str> = Vec::new();
    for line in block.lines() {
        let line = line.trim_start();
        if let Some(event) = strip_sse_field(line, "event") {
            event_name = event.trim().to_string();
        } else if let Some(data) = strip_sse_field(line, "data") {
            data_lines.push(data);
        }
    }
    (!data_lines.is_empty()).then(|| (event_name, data_lines.join("\n")))
}

fn error_event_message(error: &Value) -> Option<String> {
    if let Some(message) = error.get("message").and_then(|value| value.as_str()) {
        return (!message.is_empty()).then(|| message.to_string());
    }
    if let Some(message) = error.as_str() {
        return (!message.is_empty()).then(|| message.to_string());
    }
    None
}

fn envelope_value_meaningful(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        Value::Number(number) => number.as_f64() != Some(0.0),
        _ => true,
    }
}

fn merge_tool_call_delta(
    tool_calls: &mut BTreeMap<usize, Value>,
    delta: &Value,
    fallback_index: usize,
) {
    let index = delta
        .get("index")
        .and_then(|index| index.as_u64())
        .map(|index| index as usize)
        .unwrap_or(fallback_index);
    let target = tool_calls.entry(index).or_insert_with(|| {
        json!({
            "id": "",
            "type": "function",
            "function": {"name": "", "arguments": ""}
        })
    });
    if let Some(value) = delta
        .get("id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
    {
        target["id"] = json!(value);
    }
    if let Some(function) = delta.get("function") {
        if let Some(name) = function
            .get("name")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
        {
            target["function"]["name"] = json!(name);
        }
        match function.get("arguments") {
            Some(Value::String(args)) => {
                if let Some(existing) = target["function"]["arguments"].as_str() {
                    target["function"]["arguments"] = json!(format!("{existing}{args}"));
                }
            }
            Some(value @ (Value::Object(_) | Value::Array(_))) => {
                let serialized = serde_json::to_string(value).unwrap_or_default();
                if let Some(existing) = target["function"]["arguments"].as_str() {
                    target["function"]["arguments"] = json!(format!("{existing}{serialized}"));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::response_transform::openai_chat_to_anthropic_message;

    use super::{
        append_utf8_safe, chat_sse_to_response_value, responses_sse_to_response_value,
        strip_sse_field, take_sse_block, SseEventScanner, SsePassthroughEventKind,
        SseUsageAccumulator,
    };
    use serde_json::{json, Value};
    use std::time::{Duration, Instant};

    fn generated_id_factory() -> impl FnMut() -> String {
        let mut next = 0usize;
        move || {
            next += 1;
            format!("generated-{next}")
        }
    }

    #[test]
    fn strip_sse_field_accepts_optional_space() {
        assert_eq!(
            strip_sse_field("data: {\"ok\":true}", "data"),
            Some("{\"ok\":true}")
        );
        assert_eq!(
            strip_sse_field("data:{\"ok\":true}", "data"),
            Some("{\"ok\":true}")
        );
        assert_eq!(
            strip_sse_field("event: message_start", "event"),
            Some("message_start")
        );
        assert_eq!(
            strip_sse_field("event:message_start", "event"),
            Some("message_start")
        );
        assert_eq!(strip_sse_field("id:1", "data"), None);
    }

    #[test]
    fn take_sse_block_supports_lf_delimiters() {
        let mut buffer = "data: {\"ok\":true}\n\nrest".to_string();

        assert_eq!(
            take_sse_block(&mut buffer),
            Some("data: {\"ok\":true}".to_string())
        );
        assert_eq!(buffer, "rest");
    }

    #[test]
    fn take_sse_block_supports_crlf_delimiters() {
        let mut buffer = "data: {\"ok\":true}\r\n\r\nrest".to_string();

        assert_eq!(
            take_sse_block(&mut buffer),
            Some("data: {\"ok\":true}".to_string())
        );
        assert_eq!(buffer, "rest");
    }

    #[test]
    fn push_passthrough_bytes_marks_collectable_json_events() {
        let mut scanner = SseEventScanner::new();

        let events = scanner.push_passthrough_bytes(b"data: {\"usage\":true}\n\n", |_| true);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, SsePassthroughEventKind::Collect);
        assert_eq!(events[0].parsed, Some(json!({"usage": true})));
        assert_eq!(
            events[0].log_message("REQ-1"),
            "[REQ-1] <<< SSE 事件: {\"usage\":true}"
        );
    }

    #[test]
    fn push_passthrough_bytes_marks_non_collectable_events_as_data() {
        let mut scanner = SseEventScanner::new();

        let events = scanner.push_passthrough_bytes(b"data: {\"usage\":true}\n\n", |_| false);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, SsePassthroughEventKind::Data);
        assert!(events[0].parsed.is_none());
        assert_eq!(
            events[0].log_message("REQ-1"),
            "[REQ-1] <<< SSE 数据: {\"usage\":true}"
        );
    }

    #[test]
    fn push_passthrough_bytes_marks_done_events() {
        let mut scanner = SseEventScanner::new();

        let events = scanner.push_passthrough_bytes(b"data: [DONE]\n\n", |_| true);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, SsePassthroughEventKind::Done);
        assert!(events[0].parsed.is_none());
        assert_eq!(events[0].log_message("REQ-1"), "[REQ-1] <<< SSE: [DONE]");
    }

    #[test]
    fn ascii_passthrough() {
        let mut buf = String::new();
        let mut rem = Vec::new();
        append_utf8_safe(&mut buf, &mut rem, b"hello world");
        assert_eq!(buf, "hello world");
        assert!(rem.is_empty());
    }

    #[test]
    fn complete_multibyte_in_single_chunk() {
        let mut buf = String::new();
        let mut rem = Vec::new();
        let text = "\u{4f60}\u{597d}\u{4e16}\u{754c}";

        append_utf8_safe(&mut buf, &mut rem, text.as_bytes());

        assert_eq!(buf, text);
        assert!(rem.is_empty());
    }

    #[test]
    fn split_multibyte_across_two_chunks() {
        let text = "\u{4f60}";
        let bytes = text.as_bytes();
        assert_eq!(bytes.len(), 3);

        let mut buf = String::new();
        let mut rem = Vec::new();

        append_utf8_safe(&mut buf, &mut rem, &bytes[..2]);
        assert_eq!(buf, "");
        assert_eq!(rem.len(), 2);

        append_utf8_safe(&mut buf, &mut rem, &bytes[2..]);
        assert_eq!(buf, text);
        assert!(rem.is_empty());
    }

    #[test]
    fn split_four_byte_char_across_chunks() {
        let text = "\u{1F600}";
        let bytes = text.as_bytes();
        assert_eq!(bytes.len(), 4);

        let mut buf = String::new();
        let mut rem = Vec::new();

        append_utf8_safe(&mut buf, &mut rem, &bytes[..1]);
        assert_eq!(buf, "");
        assert_eq!(rem.len(), 1);

        append_utf8_safe(&mut buf, &mut rem, &bytes[1..2]);
        assert_eq!(buf, "");
        assert_eq!(rem.len(), 2);

        append_utf8_safe(&mut buf, &mut rem, &bytes[2..3]);
        assert_eq!(buf, "");
        assert_eq!(rem.len(), 3);

        append_utf8_safe(&mut buf, &mut rem, &bytes[3..]);
        assert_eq!(buf, text);
        assert!(rem.is_empty());
    }

    #[test]
    fn mixed_ascii_and_split_multibyte() {
        let text = "hi\u{4f60}";
        let bytes = text.as_bytes();
        assert_eq!(bytes.len(), 5);

        let mut buf = String::new();
        let mut rem = Vec::new();

        append_utf8_safe(&mut buf, &mut rem, &bytes[..3]);
        assert_eq!(buf, "hi");
        assert_eq!(rem.len(), 1);

        append_utf8_safe(&mut buf, &mut rem, &bytes[3..]);
        assert_eq!(buf, text);
        assert!(rem.is_empty());
    }

    #[test]
    fn multiple_split_characters_in_sequence() {
        let text = "\u{4f60}\u{597d}";
        let bytes = text.as_bytes();

        let mut buf = String::new();
        let mut rem = Vec::new();

        append_utf8_safe(&mut buf, &mut rem, &bytes[..4]);
        assert_eq!(buf, "\u{4f60}");
        assert_eq!(rem.len(), 1);

        append_utf8_safe(&mut buf, &mut rem, &bytes[4..]);
        assert_eq!(buf, text);
        assert!(rem.is_empty());
    }

    #[test]
    fn empty_chunks_are_harmless() {
        let mut buf = String::new();
        let mut rem = Vec::new();

        append_utf8_safe(&mut buf, &mut rem, b"");
        assert_eq!(buf, "");
        assert!(rem.is_empty());

        append_utf8_safe(&mut buf, &mut rem, b"ok");
        assert_eq!(buf, "ok");

        append_utf8_safe(&mut buf, &mut rem, b"");
        assert_eq!(buf, "ok");
    }

    #[test]
    fn sse_json_with_multibyte_split_at_boundary() {
        let text = "\u{4f60}\u{597d}";
        let json_line = format!("data: {{\"text\":\"{text}\"}}\n\n");
        let bytes = json_line.as_bytes();
        let split_start = bytes
            .windows(3)
            .position(|window| window == "\u{4f60}".as_bytes())
            .unwrap();
        let split_point = split_start + 1;

        let mut buf = String::new();
        let mut rem = Vec::new();

        append_utf8_safe(&mut buf, &mut rem, &bytes[..split_point]);
        append_utf8_safe(&mut buf, &mut rem, &bytes[split_point..]);

        assert_eq!(buf, json_line);
        assert!(rem.is_empty());

        let data = strip_sse_field(buf.lines().next().unwrap(), "data").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(parsed["text"], text);
    }

    #[test]
    fn invalid_bytes_flushed_immediately_not_accumulated() {
        let mut buf = String::new();
        let mut rem = Vec::new();

        append_utf8_safe(&mut buf, &mut rem, b"hi\xFFok");

        assert!(rem.is_empty());
        assert!(buf.contains("hi"));
        assert!(buf.contains("ok"));
        assert!(buf.contains('\u{FFFD}'));
    }

    #[test]
    fn invalid_byte_in_slow_path_flushed_immediately() {
        let mut buf = String::new();
        let mut rem = Vec::new();
        let text = "\u{4f60}";

        append_utf8_safe(&mut buf, &mut rem, &text.as_bytes()[..1]);
        assert_eq!(rem.len(), 1);

        append_utf8_safe(&mut buf, &mut rem, b"\xFFworld");

        assert!(rem.is_empty());
        assert!(buf.contains("world"));
    }

    #[test]
    fn defensive_guard_flushes_oversized_remainder() {
        let mut buf = String::new();
        let mut rem = Vec::new();

        rem.extend_from_slice(b"\x80\x80\x80\x80");
        assert_eq!(rem.len(), 4);

        append_utf8_safe(&mut buf, &mut rem, b"hello");

        assert!(rem.is_empty());
        assert!(buf.contains("hello"));
        let replacement_count = buf.chars().filter(|&c| c == '\u{FFFD}').count();
        assert_eq!(replacement_count, 4);
    }

    #[test]
    fn sse_event_scanner_parses_json_data_events() {
        let mut scanner = SseEventScanner::new();

        let events = scanner.push_bytes(b"data: {\"usage\":{\"total_tokens\":3}}\n\n", |_| true);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\"usage\":{\"total_tokens\":3}}");
        assert_eq!(
            events[0].parsed.as_ref().unwrap()["usage"]["total_tokens"],
            3
        );
        assert!(!events[0].done);
    }

    #[test]
    fn sse_event_scanner_preserves_utf8_across_chunks() {
        let text = "data: {\"text\":\"\u{4f60}\"}\n\n";
        let bytes = text.as_bytes();
        let split = bytes
            .windows(3)
            .position(|window| window == "\u{4f60}".as_bytes())
            .unwrap()
            + 1;
        let mut scanner = SseEventScanner::new();

        assert!(scanner.push_bytes(&bytes[..split], |_| true).is_empty());
        let events = scanner.push_bytes(&bytes[split..], |_| true);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].parsed.as_ref().unwrap()["text"], "\u{4f60}");
    }

    #[test]
    fn sse_event_scanner_marks_done_without_parsing() {
        let mut scanner = SseEventScanner::new();

        let events = scanner.push_bytes(b"data: [DONE]\n\n", |_| true);

        assert_eq!(events.len(), 1);
        assert!(events[0].done);
        assert!(events[0].parsed.is_none());
    }

    #[test]
    fn sse_event_scanner_filter_can_skip_json_parse() {
        let mut scanner = SseEventScanner::new();

        let events = scanner.push_bytes(b"data: {\"usage\":{\"total_tokens\":3}}\n\n", |_| false);

        assert_eq!(events.len(), 1);
        assert!(events[0].parsed.is_none());
    }

    #[test]
    fn sse_event_scanner_handles_crlf_delimiters() {
        let mut scanner = SseEventScanner::new();

        let events = scanner.push_bytes(b"data: {\"ok\":true}\r\n\r\n", |_| true);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].parsed.as_ref().unwrap()["ok"], true);
    }

    #[test]
    fn sse_usage_accumulator_returns_events_and_first_token_time_once() {
        let start_time = Instant::now() - Duration::from_millis(25);
        let mut accumulator = SseUsageAccumulator::new(start_time);

        accumulator.push(json!({"usage":{"input_tokens":1}}));
        accumulator.push(json!({"usage":{"output_tokens":2}}));

        let snapshot = accumulator.finish().unwrap();

        assert_eq!(snapshot.events.len(), 2);
        assert!(snapshot.first_token_ms.unwrap() >= 25);
        assert!(accumulator.finish().is_none());
    }

    #[test]
    fn sse_usage_accumulator_finishes_empty_stream() {
        let mut accumulator = SseUsageAccumulator::new(Instant::now());

        let snapshot = accumulator.finish().unwrap();

        assert!(snapshot.events.is_empty());
        assert!(snapshot.first_token_ms.is_none());
    }

    #[test]
    fn sse_usage_accumulator_ignores_push_after_finish() {
        let mut accumulator = SseUsageAccumulator::new(Instant::now());

        assert!(accumulator.finish().is_some());
        accumulator.push(json!({"usage":{"input_tokens":1}}));

        assert!(accumulator.finish().is_none());
    }

    #[test]
    fn stream_usage_event_filters_match_protocol_usage_markers() {
        assert!(super::claude_stream_usage_event_filter(
            r#"{"type":"message_delta","usage":{"output_tokens":1}}"#
        ));
        assert!(!super::claude_stream_usage_event_filter(
            r#"{"type":"content_block_delta","delta":{"text":"hi"}}"#
        ));
        assert!(super::openai_stream_usage_event_filter(
            r#"{"choices":[],"usage":{"total_tokens":3}}"#
        ));
        assert!(!super::openai_stream_usage_event_filter(
            r#"{"choices":[{"delta":{"content":"hi"}}]}"#
        ));
        assert!(super::codex_stream_usage_event_filter(
            r#"{"type":"response.completed","response":{"usage":{"total_tokens":3}}}"#
        ));
        assert!(super::gemini_stream_usage_event_filter(
            r#"{"usageMetadata":{"totalTokenCount":3}}"#
        ));
        assert!(!super::gemini_stream_usage_event_filter(
            r#"{"candidates":[{"content":{"parts":[{"text":"hi"}]}}]}"#
        ));
    }

    #[test]
    fn responses_sse_to_response_value_collects_output_items() {
        let sse = r#"event: response.output_item.done
data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hello"}]}}

event: response.completed
data: {"type":"response.completed","response":{"id":"resp_1","status":"completed","model":"gpt-5.4","output":[],"usage":{"input_tokens":10,"output_tokens":2}}}

"#;

        let response = responses_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "resp_1");
        assert_eq!(response["output"][0]["type"], "message");
        assert_eq!(response["output"][0]["content"][0]["text"], "hello");
    }

    #[test]
    fn responses_sse_completed_then_trailing_failed_keeps_success() {
        let sse = "event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_ok\",\"status\":\"completed\",\"model\":\"gpt-5.4\",\"output\":[]}}\n\n\
event: response.failed\n\
data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"boom\"}}}\n";

        let response = responses_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "resp_ok");
    }

    #[test]
    fn responses_sse_to_response_value_handles_missing_trailing_blank_line() {
        let sse = "event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_tail\",\"status\":\"completed\",\"model\":\"gpt-5.4\",\"output\":[],\"usage\":{\"input_tokens\":3,\"output_tokens\":1}}}\n";

        let response = responses_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "resp_tail");
    }

    #[test]
    fn responses_sse_to_response_value_ignores_truncated_trailing_block() {
        let sse = "event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_ok\",\"status\":\"completed\",\"model\":\"gpt-5.4\",\"output\":[],\"usage\":{\"input_tokens\":3,\"output_tokens\":1}}}\n\
\n\
event: response.extra\n\
data: {\"type\":\"resp";

        let response = responses_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "resp_ok");
    }

    #[test]
    fn responses_sse_to_response_value_handles_crlf_delimiters() {
        let sse = "event: response.output_item.done\r\n\
data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hi\"}]}}\r\n\
\r\n\
event: response.completed\r\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_crlf\",\"status\":\"completed\",\"model\":\"gpt-5.4\",\"output\":[],\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\r\n\
\r\n";

        let response = responses_sse_to_response_value(sse).unwrap();

        assert_eq!(response["id"], "resp_crlf");
        assert_eq!(response["output"][0]["type"], "message");
        assert_eq!(response["output"][0]["content"][0]["text"], "hi");
    }

    #[test]
    fn responses_sse_to_response_value_returns_err_on_response_failed() {
        let sse = "event: response.failed\n\
data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"upstream blew up\"}}}\n\n";

        let error = responses_sse_to_response_value(sse).unwrap_err();

        assert!(error.to_string().contains("upstream blew up"), "{error}");
    }

    #[test]
    fn responses_sse_to_response_value_errors_when_no_completed_event() {
        let sse = "event: response.output_item.done\n\
data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\"}}\n\n";

        assert!(responses_sse_to_response_value(sse).is_err());
    }

    #[test]
    fn chat_sse_to_response_value_collects_reasoning_alias() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"kimi-k2.6\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning\":\"think\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning\":{\"content\":\"ing\"},\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(
            response["choices"][0]["message"]["reasoning_content"],
            "thinking"
        );
        assert_eq!(response["choices"][0]["message"]["content"], "ok");
    }

    #[test]
    fn chat_sse_to_response_value_aggregates_text_finish_reason_and_usage() {
        let sse = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":123,\"model\":\"gpt-5.4\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"total_tokens\":12}}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["id"], "chatcmpl-1");
        assert_eq!(response["object"], "chat.completion");
        assert_eq!(response["model"], "gpt-5.4");
        assert_eq!(response["choices"][0]["message"]["role"], "assistant");
        assert_eq!(response["choices"][0]["message"]["content"], "Hello");
        assert_eq!(response["choices"][0]["finish_reason"], "stop");
        assert_eq!(response["usage"]["prompt_tokens"], 10);
    }

    #[test]
    fn chat_sse_to_response_value_merges_tool_call_argument_fragments() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"SF\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        let tool_call = &response["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tool_call["id"], "call_1");
        assert_eq!(tool_call["function"]["name"], "get_weather");
        assert_eq!(tool_call["function"]["arguments"], "{\"city\":\"SF\"}");
        assert_eq!(response["choices"][0]["finish_reason"], "tool_calls");
    }

    #[test]
    fn chat_sse_to_response_value_collects_reasoning_details() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"mimo\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\"think\"}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\"ing\"}],\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(
            response["choices"][0]["message"]["reasoning_content"],
            "thinking"
        );
        assert_eq!(response["choices"][0]["message"]["content"], "ok");
    }

    #[test]
    fn chat_sse_to_response_value_rejects_truncated_stream() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"par\"},\"finish_reason\":null}]}\n\n";

        let error = chat_sse_to_response_value(sse, generated_id_factory()).unwrap_err();

        assert!(error.to_string().contains("truncated"), "{error}");
    }

    #[test]
    fn chat_sse_to_response_value_event_error_fails_even_after_complete_choice() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":\"stop\"}]}\n\n\
event: error\n\
data: {\"message\":\"insufficient_user_quota\",\"code\":429}\n\n";

        let error = chat_sse_to_response_value(sse, generated_id_factory()).unwrap_err();

        assert!(
            error.to_string().contains("insufficient_user_quota"),
            "{error}"
        );
    }

    #[test]
    fn chat_sse_to_response_value_uses_supplied_missing_id_factory() {
        let sse = "data: {\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["id"], "generated-1");
    }

    #[test]
    fn chat_sse_to_response_value_skips_azure_placeholder_envelope() {
        let sse = "data: {\"id\":\"\",\"model\":\"\",\"created\":0,\"object\":\"\",\"choices\":[],\"prompt_filter_results\":[]}\n\n\
data: {\"id\":\"chatcmpl-real\",\"model\":\"gpt-5.4\",\"created\":42,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["id"], "chatcmpl-real");
        assert_eq!(response["model"], "gpt-5.4");
        assert_eq!(response["created"], 42);
    }

    #[test]
    fn chat_sse_to_response_value_tolerates_null_error_field() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"error\":null,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_first_finish_reason_wins() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["finish_reason"], "tool_calls");
    }

    #[test]
    fn chat_sse_to_response_value_unwraps_message_shaped_fake_stream() {
        let sse = "data: {\"id\":\"c1\",\"object\":\"chat.completion\",\"model\":\"m\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"full answer\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "full answer");
        assert_eq!(response["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn chat_sse_to_response_value_message_snapshot_overrides_deltas() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"par\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"full\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "full");
    }

    #[test]
    fn chat_sse_to_response_value_backfills_sparse_tool_call_ids() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"name\":\"f2\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        let tool_calls = response["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "tool_call_1");
        assert_eq!(tool_calls[0]["function"]["name"], "f2");
    }

    #[test]
    fn chat_sse_to_response_value_strips_bom_before_parsing() {
        let sse = "\u{feff}data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_collects_reasoning_content() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"deepseek-r2\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"think\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"ing\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(
            response["choices"][0]["message"]["reasoning_content"],
            "thinking"
        );
        assert_eq!(response["choices"][0]["message"]["content"], "ok");
    }

    #[test]
    fn chat_sse_to_response_value_handles_missing_trailing_blank_line() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_handles_crlf_delimiters() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\r\n\
\r\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\r\n\
\r\n\
data: [DONE]\r\n\
\r\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
        assert_eq!(response["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn chat_sse_to_response_value_propagates_upstream_error_event() {
        let sse = "data: {\"error\":{\"message\":\"rate limited by gateway\",\"code\":429}}\n\n";

        let error = chat_sse_to_response_value(sse, generated_id_factory()).unwrap_err();

        assert!(
            error.to_string().contains("rate limited by gateway"),
            "{error}"
        );
    }

    #[test]
    fn chat_sse_to_response_value_accepts_done_marker_without_finish_reason() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
        assert_eq!(response["choices"][0]["finish_reason"], Value::Null);
    }

    #[test]
    fn chat_sse_to_response_value_rejects_stream_without_chunks() {
        let error =
            chat_sse_to_response_value(": keepalive\n\ndata: [DONE]\n\n", generated_id_factory())
                .unwrap_err();

        assert!(
            error.to_string().contains("No chat completion choices"),
            "{error}"
        );
    }

    #[test]
    fn chat_sse_to_response_value_rejects_choiceless_stream_despite_done() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":0,\"total_tokens\":1}}\n\n\
data: [DONE]\n\n";

        let error = chat_sse_to_response_value(sse, generated_id_factory()).unwrap_err();

        assert!(
            error.to_string().contains("No chat completion choices"),
            "{error}"
        );
    }

    #[test]
    fn chat_sse_to_response_value_huge_tool_call_index_does_not_oom() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":4000000000,\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();
        let tool_calls = response["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "tool_call_4000000000");
        assert_eq!(tool_calls[0]["function"]["name"], "f");
    }

    #[test]
    fn chat_sse_to_response_value_empty_delta_falls_back_to_message_snapshot() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{},\"message\":{\"role\":\"assistant\",\"content\":\"full answer\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "full answer");
        assert_eq!(response["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn chat_sse_to_response_value_empty_delta_scaffold_does_not_wipe_real_content() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"message\":{},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" there\"},\"message\":{},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi there");
    }

    #[test]
    fn chat_sse_to_response_value_object_form_tool_arguments_preserved() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"tool_calls\":[{\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":{\"city\":\"SF\"}}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();
        let args = response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap();
        let parsed: Value = serde_json::from_str(args).unwrap();

        assert_eq!(parsed["city"], "SF");
    }

    #[test]
    fn chat_sse_to_response_value_collects_refusal() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"refusal\":\"I can't help with that.\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(
            response["choices"][0]["message"]["content"],
            "I can't help with that."
        );
    }

    #[test]
    fn chat_sse_to_response_value_maps_legacy_function_call() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":null,\"function_call\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\\\"SF\\\"}\"}},\"finish_reason\":\"function_call\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();
        let tool_call = &response["choices"][0]["message"]["tool_calls"][0];

        assert_eq!(tool_call["function"]["name"], "get_weather");
        assert_eq!(tool_call["function"]["arguments"], "{\"city\":\"SF\"}");
    }

    #[test]
    fn chat_sse_to_response_value_tolerates_empty_error_placeholder() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"error\":{},\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_tolerates_truncated_residual_after_complete() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n\
data: {\"usage\":{\"prompt_to";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn chat_sse_to_response_value_float_zero_does_not_freeze_envelope() {
        let sse = "data: {\"id\":\"\",\"model\":\"\",\"created\":0.0,\"choices\":[]}\n\n\
data: {\"id\":\"chatcmpl-real\",\"model\":\"m\",\"created\":42,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["created"], 42);
        assert_eq!(response["id"], "chatcmpl-real");
    }

    #[test]
    fn chat_sse_to_response_value_synthesizes_id_when_absent() {
        let sse = "data: {\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";
        let mut next_missing_id = generated_id_factory();

        let first = chat_sse_to_response_value(sse, &mut next_missing_id).unwrap();
        let second = chat_sse_to_response_value(sse, &mut next_missing_id).unwrap();
        let first_id = first["id"].as_str().unwrap();
        let second_id = second["id"].as_str().unwrap();

        assert!(!first_id.is_empty());
        assert_ne!(first_id, second_id);
    }

    #[test]
    fn chat_sse_to_response_value_accepts_indented_data_lines() {
        let sse = "  data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let response = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();

        assert_eq!(response["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn aggregated_chat_sse_round_trips_through_openai_to_anthropic() {
        let sse = "data: {\"id\":\"chatcmpl-9\",\"created\":1,\"model\":\"gpt-5.4\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-9\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":1,\"total_tokens\":5}}\n\n\
data: [DONE]\n\n";

        let aggregated = chat_sse_to_response_value(sse, generated_id_factory()).unwrap();
        let anthropic = openai_chat_to_anthropic_message(&aggregated).unwrap();

        assert_eq!(anthropic["model"], "gpt-5.4");
        assert_eq!(anthropic["content"][0]["type"], "text");
        assert_eq!(anthropic["content"][0]["text"], "Hi");
        assert_eq!(anthropic["stop_reason"], "end_turn");
    }
}
