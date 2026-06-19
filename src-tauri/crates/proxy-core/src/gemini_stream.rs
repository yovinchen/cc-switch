//! Gemini Native streaming state helpers.
//!
//! These helpers keep Gemini cumulative `content.parts` interpretation in the
//! host-neutral core while leaving async transport and SSE emission in the host.

use crate::gemini_shadow::{
    GeminiShadowSessionSnapshot, GeminiShadowStore, GeminiToolCallMeta,
};
use crate::gemini_tool_args::{rectify_gemini_tool_call_parts, AnthropicToolSchemaHints};
use crate::response_transform::{
    build_anthropic_message_delta_event, map_gemini_finish_reason_to_anthropic,
};
use crate::sse::{append_utf8_safe, strip_sse_field, take_sse_block};
use crate::usage::build_anthropic_usage_from_gemini;
use bytes::Bytes;
use serde_json::{json, Value};
use std::collections::HashSet;

/// Prefix used for Anthropic-visible tool call ids synthesized when Gemini's
/// `functionCall` omits an id.
pub const GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX: &str = "gemini_synth_";

/// Returns true if `id` is an internal Gemini tool-call id synthesized by this
/// proxy and therefore must not be sent back to Gemini upstream.
pub fn is_synthesized_gemini_tool_call_id(id: &str) -> bool {
    id.starts_with(GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX)
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeminiStreamPartsUpdate {
    pub visible_text: String,
    pub text_thought_signature: Option<String>,
    pub tool_calls: Vec<GeminiToolCallMeta>,
    pub rectified_tool_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeminiStreamSseEvent {
    pub event_name: &'static str,
    pub payload: Value,
}

impl GeminiStreamSseEvent {
    pub fn to_sse_bytes(&self) -> Bytes {
        encode_gemini_stream_sse(self.event_name, &self.payload)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeminiStreamChunkOutput {
    pub events: Vec<GeminiStreamSseEvent>,
    pub rectified_tool_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeminiStreamBlockOutput {
    pub events: Vec<GeminiStreamSseEvent>,
    pub rectified_tool_names: Vec<String>,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeminiStreamShadowRecord {
    pub assistant_content: Value,
    pub tool_calls: Vec<GeminiToolCallMeta>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeminiStreamFinalOutput {
    pub events: Vec<GeminiStreamSseEvent>,
    pub shadow_record: Option<GeminiStreamShadowRecord>,
}

impl GeminiStreamFinalOutput {
    pub fn record_shadow(
        &mut self,
        shadow_store: Option<&GeminiShadowStore>,
        provider_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Option<GeminiShadowSessionSnapshot> {
        let shadow_store = shadow_store?;
        let provider_id = provider_id?;
        let session_id = session_id?;
        let shadow_record = self.shadow_record.take()?;

        Some(shadow_store.record_assistant_turn(
            provider_id,
            session_id,
            shadow_record.assistant_content,
            shadow_record.tool_calls,
        ))
    }
}

#[derive(Debug, Default)]
pub struct GeminiToAnthropicSseState {
    buffer: String,
    utf8_remainder: Vec<u8>,
    message_id: Option<String>,
    current_model: Option<String>,
    has_sent_message_start: bool,
    accumulated_text: String,
    text_block_index: Option<u32>,
    next_content_index: u32,
    open_indices: HashSet<u32>,
    tool_call_snapshots: Vec<GeminiToolCallMeta>,
    text_thought_signature: Option<String>,
    latest_usage: Option<Value>,
    latest_finish_reason: Option<String>,
    blocked_text: Option<String>,
}

impl GeminiToAnthropicSseState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_bytes<F>(
        &mut self,
        bytes: &[u8],
        tool_schema_hints: Option<&AnthropicToolSchemaHints>,
        mut synthesize_tool_call_id: F,
    ) -> GeminiStreamBlockOutput
    where
        F: FnMut() -> String,
    {
        let mut output = GeminiStreamBlockOutput {
            events: Vec::new(),
            rectified_tool_names: Vec::new(),
            done: false,
        };

        append_utf8_safe(&mut self.buffer, &mut self.utf8_remainder, bytes);

        while let Some(block) = take_sse_block(&mut self.buffer) {
            let block_output =
                self.handle_sse_block(&block, tool_schema_hints, &mut synthesize_tool_call_id);
            output.events.extend(block_output.events);
            output
                .rectified_tool_names
                .extend(block_output.rectified_tool_names);
            if block_output.done {
                output.done = true;
                break;
            }
        }

        output
    }

    pub fn handle_sse_block<F>(
        &mut self,
        block: &str,
        tool_schema_hints: Option<&AnthropicToolSchemaHints>,
        synthesize_tool_call_id: F,
    ) -> GeminiStreamBlockOutput
    where
        F: FnMut() -> String,
    {
        let mut output = GeminiStreamBlockOutput {
            events: Vec::new(),
            rectified_tool_names: Vec::new(),
            done: false,
        };

        if block.trim().is_empty() {
            return output;
        }

        let mut data_lines: Vec<String> = Vec::new();
        for line in block.lines() {
            if let Some(data) = strip_sse_field(line, "data") {
                data_lines.push(data.to_string());
            }
        }

        if data_lines.is_empty() {
            return output;
        }

        let data = data_lines.join("\n");
        if data.trim() == "[DONE]" {
            output.done = true;
            return output;
        }

        let Ok(chunk_json) = serde_json::from_str::<Value>(&data) else {
            return output;
        };

        let chunk_output =
            self.handle_chunk(&chunk_json, tool_schema_hints, synthesize_tool_call_id);
        output.events = chunk_output.events;
        output.rectified_tool_names = chunk_output.rectified_tool_names;
        output
    }

    pub fn handle_chunk<F>(
        &mut self,
        chunk_json: &Value,
        tool_schema_hints: Option<&AnthropicToolSchemaHints>,
        synthesize_tool_call_id: F,
    ) -> GeminiStreamChunkOutput
    where
        F: FnMut() -> String,
    {
        let mut output = GeminiStreamChunkOutput {
            events: Vec::new(),
            rectified_tool_names: Vec::new(),
        };

        if self.message_id.is_none() {
            self.message_id = chunk_json
                .get("responseId")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
        }
        if self.current_model.is_none() {
            self.current_model = chunk_json
                .get("modelVersion")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
        }
        if self.latest_usage.is_none() {
            self.latest_usage = chunk_json.get("usageMetadata").cloned();
        }

        self.ensure_message_start(chunk_json.get("usageMetadata"), &mut output.events);

        if let Some(reason) = chunk_json
            .get("promptFeedback")
            .and_then(|value| value.get("blockReason"))
            .and_then(|value| value.as_str())
        {
            self.blocked_text = Some(format!("Request blocked by Gemini safety filters: {reason}"));
        }

        let Some(candidate) = chunk_json
            .get("candidates")
            .and_then(|value| value.as_array())
            .and_then(|value| value.first())
        else {
            return output;
        };

        if let Some(reason) = candidate.get("finishReason").and_then(|value| value.as_str()) {
            self.latest_finish_reason = Some(reason.to_string());
        }
        if let Some(usage) = chunk_json.get("usageMetadata") {
            self.latest_usage = Some(usage.clone());
        }

        let Some(parts) = candidate
            .get("content")
            .and_then(|value| value.get("parts"))
            .and_then(|value| value.as_array())
        else {
            return output;
        };

        let parts_update = analyze_gemini_stream_parts(parts, tool_schema_hints);
        output.rectified_tool_names = parts_update.rectified_tool_names;

        if let Some(signature) = parts_update.text_thought_signature {
            self.text_thought_signature = Some(signature);
        }
        merge_gemini_tool_call_snapshots(
            &mut self.tool_call_snapshots,
            parts_update.tool_calls,
            synthesize_tool_call_id,
        );

        self.push_visible_text(parts_update.visible_text, &mut output.events);
        output
    }

    pub fn finish(mut self) -> GeminiStreamFinalOutput {
        let mut events = Vec::new();

        if !self.has_sent_message_start {
            self.ensure_message_start(self.latest_usage.clone().as_ref(), &mut events);
        }

        if self.accumulated_text.is_empty() {
            if let Some(blocked_text) = self.blocked_text.clone() {
                let index = self.text_block_index.unwrap_or_else(|| self.allocate_content_index());
                self.text_block_index = Some(index);
                if !self.open_indices.contains(&index) {
                    events.push(event(
                        "content_block_start",
                        gemini_stream_text_block_start_event(index),
                    ));
                    self.open_indices.insert(index);
                }
                events.push(event(
                    "content_block_delta",
                    gemini_stream_text_delta_event(index, blocked_text),
                ));
            }
        }

        if let Some(index) = self.text_block_index {
            if self.open_indices.remove(&index) {
                events.push(event(
                    "content_block_stop",
                    gemini_stream_content_block_stop_event(index),
                ));
            }
        }

        let tool_calls = std::mem::take(&mut self.tool_call_snapshots);
        let shadow_record = self.build_shadow_record(&tool_calls);

        // Known trade-off: Gemini's cumulative stream may interleave text and
        // tool calls, but we emit all `tool_use` blocks after the final text
        // block. Target Anthropic-compatible clients consume tool calls by
        // scanning blocks and do not depend on strict text/tool interleaving.
        for tool_call in &tool_calls {
            let index = self.allocate_content_index();
            events.push(event(
                "content_block_start",
                gemini_stream_tool_block_start_event(index, tool_call),
            ));
            events.push(event(
                "content_block_delta",
                gemini_stream_tool_input_delta_event(index, &tool_call.args),
            ));
            events.push(event(
                "content_block_stop",
                gemini_stream_content_block_stop_event(index),
            ));
        }

        events.push(event(
            "message_delta",
            gemini_stream_message_delta_event(
                self.latest_finish_reason.as_deref(),
                !tool_calls.is_empty(),
                self.blocked_text.is_some(),
                self.latest_usage.as_ref(),
            ),
        ));
        events.push(event("message_stop", gemini_stream_message_stop_event()));

        GeminiStreamFinalOutput {
            events,
            shadow_record,
        }
    }

    fn ensure_message_start(
        &mut self,
        usage: Option<&Value>,
        events: &mut Vec<GeminiStreamSseEvent>,
    ) {
        if self.has_sent_message_start {
            return;
        }

        events.push(event(
            "message_start",
            gemini_stream_message_start_event(
                self.message_id.as_deref(),
                self.current_model.as_deref(),
                usage,
            ),
        ));
        self.has_sent_message_start = true;
    }

    fn push_visible_text(&mut self, visible_text: String, events: &mut Vec<GeminiStreamSseEvent>) {
        if visible_text.is_empty() {
            return;
        }

        let is_cumulative = visible_text.starts_with(&self.accumulated_text);
        let delta = if is_cumulative {
            visible_text[self.accumulated_text.len()..].to_string()
        } else {
            visible_text.clone()
        };

        if delta.is_empty() {
            return;
        }

        let index = self
            .text_block_index
            .unwrap_or_else(|| self.allocate_content_index());
        self.text_block_index = Some(index);

        if !self.open_indices.contains(&index) {
            events.push(event(
                "content_block_start",
                gemini_stream_text_block_start_event(index),
            ));
            self.open_indices.insert(index);
        }

        events.push(event(
            "content_block_delta",
            gemini_stream_text_delta_event(index, delta.as_str()),
        ));

        if is_cumulative {
            self.accumulated_text = visible_text;
        } else {
            self.accumulated_text.push_str(&delta);
        }
    }

    fn build_shadow_record(
        &self,
        tool_calls: &[GeminiToolCallMeta],
    ) -> Option<GeminiStreamShadowRecord> {
        let shadow_text = if self.accumulated_text.is_empty() {
            self.blocked_text.as_deref()
        } else {
            Some(self.accumulated_text.as_str())
        };
        let shadow_parts = build_gemini_stream_shadow_assistant_parts(
            shadow_text,
            self.text_thought_signature.as_deref(),
            tool_calls,
        );
        if shadow_parts.is_empty() {
            return None;
        }

        Some(GeminiStreamShadowRecord {
            assistant_content: json!({ "parts": shadow_parts }),
            tool_calls: tool_calls.to_vec(),
        })
    }

    fn allocate_content_index(&mut self) -> u32 {
        let assigned = self.next_content_index;
        self.next_content_index += 1;
        assigned
    }
}

pub fn analyze_gemini_stream_parts(
    parts: &[Value],
    tool_schema_hints: Option<&AnthropicToolSchemaHints>,
) -> GeminiStreamPartsUpdate {
    let mut rectified_parts = parts.to_vec();
    let rectified_tool_names =
        rectify_gemini_tool_call_parts(&mut rectified_parts, tool_schema_hints);

    GeminiStreamPartsUpdate {
        visible_text: extract_visible_text(&rectified_parts),
        text_thought_signature: extract_text_thought_signature(parts),
        tool_calls: extract_tool_calls(&rectified_parts),
        rectified_tool_names,
    }
}

pub fn merge_gemini_tool_call_snapshots<F>(
    tool_call_snapshots: &mut Vec<GeminiToolCallMeta>,
    incoming: Vec<GeminiToolCallMeta>,
    mut synthesize_tool_call_id: F,
) where
    F: FnMut() -> String,
{
    // Gemini's `streamGenerateContent?alt=sse` delivers each chunk as the
    // cumulative snapshot of `content.parts`. For the same tool call across
    // chunks we therefore need to map an incoming entry back to whichever
    // snapshot entry it describes:
    //
    // 1. If both sides carry a genuine Gemini id, match by id.
    // 2. Otherwise match by position in the cumulative `parts` array. This is
    //    how parallel no-id calls stay distinguishable.
    //
    // Matching by tool name would silently merge two parallel calls to the same
    // function into one entry, losing one call's args.
    for (position, mut tool_call) in incoming.into_iter().enumerate() {
        if tool_call.id.as_deref() == Some("") {
            tool_call.id = None;
        }

        let existing_index = match tool_call.id.as_deref() {
            Some(incoming_id) => tool_call_snapshots
                .iter()
                .position(|existing| existing.id.as_deref() == Some(incoming_id))
                .or_else(|| {
                    tool_call_snapshots
                        .get(position)
                        .filter(|existing| {
                            matches!(
                                existing.id.as_deref(),
                                Some(id) if is_synthesized_gemini_tool_call_id(id)
                            )
                        })
                        .map(|_| position)
                }),
            None => tool_call_snapshots
                .get(position)
                .filter(|existing| match existing.id.as_deref() {
                    Some(id) => is_synthesized_gemini_tool_call_id(id),
                    None => true,
                })
                .map(|_| position),
        };

        if let Some(index) = existing_index {
            let preserved_id = tool_call_snapshots[index].id.clone();
            tool_call.id = tool_call.id.or(preserved_id);

            if tool_call.thought_signature.is_none() {
                tool_call
                    .thought_signature
                    .clone_from(&tool_call_snapshots[index].thought_signature);
            }
        }

        if tool_call.id.is_none() {
            tool_call.id = Some(synthesize_tool_call_id());
        }

        match existing_index {
            Some(index) => tool_call_snapshots[index] = tool_call,
            None => tool_call_snapshots.push(tool_call),
        }
    }
}

pub fn build_gemini_stream_shadow_assistant_parts(
    text: Option<&str>,
    text_thought_signature: Option<&str>,
    tool_calls: &[GeminiToolCallMeta],
) -> Vec<Value> {
    let mut parts = Vec::new();

    if text.filter(|text| !text.is_empty()).is_some() || text_thought_signature.is_some() {
        let mut part = json!({
            "text": text.unwrap_or("")
        });
        if let Some(signature) = text_thought_signature {
            part["thoughtSignature"] = json!(signature);
        }
        parts.push(part);
    }

    for tool_call in tool_calls {
        let mut part = json!({
            "functionCall": {
                "id": tool_call.id.clone().unwrap_or_default(),
                "name": tool_call.name,
                "args": tool_call.args
            }
        });

        if let Some(signature) = &tool_call.thought_signature {
            part["thoughtSignature"] = json!(signature);
        }

        parts.push(part);
    }

    parts
}

pub fn gemini_stream_message_start_event(
    message_id: Option<&str>,
    model: Option<&str>,
    usage: Option<&Value>,
) -> Value {
    json!({
        "type": "message_start",
        "message": {
            "id": message_id.unwrap_or_default(),
            "type": "message",
            "role": "assistant",
            "model": model.unwrap_or_default(),
            "usage": build_anthropic_usage_from_gemini(usage)
        }
    })
}

pub fn gemini_stream_text_block_start_event(index: u32) -> Value {
    json!({
        "type": "content_block_start",
        "index": index,
        "content_block": {
            "type": "text",
            "text": ""
        }
    })
}

pub fn gemini_stream_text_delta_event(index: u32, text: impl Into<String>) -> Value {
    json!({
        "type": "content_block_delta",
        "index": index,
        "delta": {
            "type": "text_delta",
            "text": text.into()
        }
    })
}

pub fn gemini_stream_content_block_stop_event(index: u32) -> Value {
    json!({
        "type": "content_block_stop",
        "index": index
    })
}

pub fn gemini_stream_tool_block_start_event(index: u32, tool_call: &GeminiToolCallMeta) -> Value {
    json!({
        "type": "content_block_start",
        "index": index,
        "content_block": {
            "type": "tool_use",
            "id": tool_call.id.clone().unwrap_or_default(),
            "name": tool_call.name.as_str()
        }
    })
}

pub fn gemini_stream_tool_input_delta_event(index: u32, args: &Value) -> Value {
    json!({
        "type": "content_block_delta",
        "index": index,
        "delta": {
            "type": "input_json_delta",
            "partial_json": serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string())
        }
    })
}

pub fn gemini_stream_message_delta_event(
    latest_finish_reason: Option<&str>,
    has_tool_calls: bool,
    blocked: bool,
    usage: Option<&Value>,
) -> Value {
    let stop_reason =
        map_gemini_finish_reason_to_anthropic(latest_finish_reason, has_tool_calls, blocked);
    let usage = build_anthropic_usage_from_gemini(usage);
    build_anthropic_message_delta_event(Some(stop_reason), Some(usage))
}

pub fn gemini_stream_message_stop_event() -> Value {
    json!({ "type": "message_stop" })
}

pub fn encode_gemini_stream_sse(event_name: &str, payload: &Value) -> Bytes {
    Bytes::from(format!(
        "event: {event_name}\ndata: {}\n\n",
        serde_json::to_string(payload).unwrap_or_default()
    ))
}

fn event(event_name: &'static str, payload: Value) -> GeminiStreamSseEvent {
    GeminiStreamSseEvent {
        event_name,
        payload,
    }
}

fn extract_visible_text(parts: &[Value]) -> String {
    parts
        .iter()
        .filter(|part| part.get("thought").and_then(|value| value.as_bool()) != Some(true))
        .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
        .collect::<String>()
}

fn extract_tool_calls(parts: &[Value]) -> Vec<GeminiToolCallMeta> {
    parts
        .iter()
        .filter_map(|part| {
            let function_call = part.get("functionCall")?;
            let id = function_call
                .get("id")
                .and_then(|value| value.as_str())
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            Some(GeminiToolCallMeta::new(
                id,
                function_call
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
                function_call
                    .get("args")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
                part.get("thoughtSignature")
                    .or_else(|| part.get("thought_signature"))
                    .and_then(|value| value.as_str()),
            ))
        })
        .collect()
}

fn extract_text_thought_signature(parts: &[Value]) -> Option<String> {
    parts
        .iter()
        .filter(|part| part.get("text").is_some() && part.get("functionCall").is_none())
        .filter_map(|part| {
            part.get("thoughtSignature")
                .or_else(|| part.get("thought_signature"))
                .and_then(|value| value.as_str())
        })
        .next_back()
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next_synth(counter: &mut usize) -> String {
        *counter += 1;
        format!("{GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX}{counter}")
    }

    fn render_events(events: &[GeminiStreamSseEvent]) -> String {
        events
            .iter()
            .map(|event| {
                format!(
                    "event: {}\ndata: {}\n\n",
                    event.event_name,
                    serde_json::to_string(&event.payload).unwrap()
                )
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn analyzes_visible_text_signature_and_tool_calls() {
        let parts = vec![
            json!({ "text": "Hel" }),
            json!({ "text": "lo", "thoughtSignature": "sig-text" }),
            json!({
                "functionCall": {
                    "id": "",
                    "name": "get_weather",
                    "args": { "city": "Tokyo" }
                },
                "thoughtSignature": "sig-tool"
            }),
        ];

        let update = analyze_gemini_stream_parts(&parts, None);

        assert_eq!(update.visible_text, "Hello");
        assert_eq!(update.text_thought_signature.as_deref(), Some("sig-text"));
        assert_eq!(update.tool_calls.len(), 1);
        assert_eq!(update.tool_calls[0].id, None);
        assert_eq!(update.tool_calls[0].name, "get_weather");
        assert_eq!(update.tool_calls[0].thought_signature.as_deref(), Some("sig-tool"));
    }

    #[test]
    fn merge_preserves_parallel_no_id_calls() {
        let incoming = vec![
            GeminiToolCallMeta::new(
                Option::<String>::None,
                "get_weather",
                json!({ "city": "Tokyo" }),
                Option::<String>::None,
            ),
            GeminiToolCallMeta::new(
                Option::<String>::None,
                "get_weather",
                json!({ "city": "Osaka" }),
                Option::<String>::None,
            ),
        ];
        let mut snapshots = Vec::new();
        let mut counter = 0;

        merge_gemini_tool_call_snapshots(&mut snapshots, incoming, || next_synth(&mut counter));

        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].args["city"], "Tokyo");
        assert_eq!(snapshots[1].args["city"], "Osaka");
        assert_ne!(snapshots[0].id, snapshots[1].id);
        assert!(is_synthesized_gemini_tool_call_id(
            snapshots[0].id.as_deref().unwrap()
        ));
    }

    #[test]
    fn merge_upgrades_synthesized_id_to_real_id_by_position() {
        let mut snapshots = Vec::new();
        let mut counter = 0;

        merge_gemini_tool_call_snapshots(
            &mut snapshots,
            vec![GeminiToolCallMeta::new(
                Option::<String>::None,
                "get_weather",
                json!({ "city": "Tokyo" }),
                Some("sig-keep"),
            )],
            || next_synth(&mut counter),
        );
        merge_gemini_tool_call_snapshots(
            &mut snapshots,
            vec![GeminiToolCallMeta::new(
                Some("real_call_1"),
                "get_weather",
                json!({ "city": "Tokyo", "units": "c" }),
                Option::<String>::None,
            )],
            || next_synth(&mut counter),
        );

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id.as_deref(), Some("real_call_1"));
        assert_eq!(snapshots[0].args["units"], "c");
        assert_eq!(snapshots[0].thought_signature.as_deref(), Some("sig-keep"));
    }

    #[test]
    fn builds_shadow_assistant_parts() {
        let tool_calls = vec![GeminiToolCallMeta::new(
            Some("call_1"),
            "Bash",
            json!({ "command": "git status" }),
            Some("sig-tool"),
        )];

        let parts = build_gemini_stream_shadow_assistant_parts(
            Some("Done"),
            Some("sig-text"),
            &tool_calls,
        );

        assert_eq!(parts[0]["text"], "Done");
        assert_eq!(parts[0]["thoughtSignature"], "sig-text");
        assert_eq!(parts[1]["functionCall"]["id"], "call_1");
        assert_eq!(parts[1]["functionCall"]["args"]["command"], "git status");
        assert_eq!(parts[1]["thoughtSignature"], "sig-tool");
    }

    #[test]
    fn builds_anthropic_sse_event_payloads() {
        let usage = json!({
            "promptTokenCount": 5,
            "totalTokenCount": 8
        });
        let tool_call = GeminiToolCallMeta::new(
            Some("call_1"),
            "Bash",
            json!({ "command": "git status" }),
            Option::<String>::None,
        );

        let start = gemini_stream_message_start_event(
            Some("resp_1"),
            Some("gemini-2.5-pro"),
            Some(&usage),
        );
        assert_eq!(start["type"], "message_start");
        assert_eq!(start["message"]["id"], "resp_1");
        assert_eq!(start["message"]["usage"]["input_tokens"], 5);

        let text_start = gemini_stream_text_block_start_event(0);
        assert_eq!(text_start["content_block"]["type"], "text");

        let text_delta = gemini_stream_text_delta_event(0, "hello");
        assert_eq!(text_delta["delta"]["type"], "text_delta");
        assert_eq!(text_delta["delta"]["text"], "hello");

        let tool_start = gemini_stream_tool_block_start_event(1, &tool_call);
        assert_eq!(tool_start["content_block"]["type"], "tool_use");
        assert_eq!(tool_start["content_block"]["id"], "call_1");

        let tool_delta = gemini_stream_tool_input_delta_event(1, &tool_call.args);
        assert_eq!(tool_delta["delta"]["type"], "input_json_delta");
        assert_eq!(
            tool_delta["delta"]["partial_json"],
            r#"{"command":"git status"}"#
        );

        let stop = gemini_stream_content_block_stop_event(1);
        assert_eq!(stop["type"], "content_block_stop");

        let message_delta =
            gemini_stream_message_delta_event(Some("STOP"), true, false, Some(&usage));
        assert_eq!(message_delta["type"], "message_delta");
        assert_eq!(message_delta["delta"]["stop_reason"], "tool_use");

        assert_eq!(gemini_stream_message_stop_event()["type"], "message_stop");
    }

    #[test]
    fn encodes_sse_event_bytes() {
        let event = GeminiStreamSseEvent {
            event_name: "message_stop",
            payload: gemini_stream_message_stop_event(),
        };
        let encoded = String::from_utf8(event.to_sse_bytes().to_vec()).unwrap();

        assert_eq!(encoded, "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
    }

    #[test]
    fn state_emits_cumulative_text_deltas_and_final_stop() {
        let mut state = GeminiToAnthropicSseState::new();
        let first = state.handle_chunk(
            &json!({
                "responseId": "resp_1",
                "modelVersion": "gemini-2.5-pro",
                "candidates": [{
                    "content": { "parts": [{ "text": "Hel" }] }
                }],
                "usageMetadata": { "promptTokenCount": 10, "totalTokenCount": 13 }
            }),
            None,
            || "unused".to_string(),
        );
        let second = state.handle_chunk(
            &json!({
                "responseId": "resp_1",
                "modelVersion": "gemini-2.5-pro",
                "candidates": [{
                    "finishReason": "STOP",
                    "content": { "parts": [{ "text": "Hello" }] }
                }],
                "usageMetadata": { "promptTokenCount": 10, "totalTokenCount": 15 }
            }),
            None,
            || "unused".to_string(),
        );
        let final_output = state.finish();
        let mut events = first.events;
        events.extend(second.events);
        events.extend(final_output.events);
        let output = render_events(&events);

        assert!(output.contains("\"id\":\"resp_1\""));
        assert!(output.contains("\"model\":\"gemini-2.5-pro\""));
        assert!(output.contains("\"text\":\"Hel\""));
        assert!(output.contains("\"text\":\"lo\""));
        assert!(output.contains("\"stop_reason\":\"end_turn\""));
        assert!(output.contains("event: message_stop"));
    }

    #[test]
    fn state_handles_sse_blocks_and_done_marker() {
        let mut state = GeminiToAnthropicSseState::new();
        let output = state.handle_sse_block(
            "event: message\ndata: {\"responseId\":\"resp_block\",\"modelVersion\":\"gemini-2.5-pro\",\ndata: \"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hi\"}]}}]}\n\n",
            None,
            || "unused".to_string(),
        );
        let rendered = render_events(&output.events);

        assert!(!output.done);
        assert!(rendered.contains("\"id\":\"resp_block\""));
        assert!(rendered.contains("\"text\":\"Hi\""));

        let invalid = state.handle_sse_block("data: {not-json}\n\n", None, || "unused".to_string());
        assert!(!invalid.done);
        assert!(invalid.events.is_empty());

        let done = state.handle_sse_block("data: [DONE]\n\n", None, || "unused".to_string());
        assert!(done.done);
        assert!(done.events.is_empty());

        let empty = state.handle_sse_block(": keepalive\n\n", None, || "unused".to_string());
        assert!(!empty.done);
        assert!(empty.events.is_empty());
    }

    #[test]
    fn state_handles_split_utf8_and_sse_blocks_from_bytes() {
        let mut state = GeminiToAnthropicSseState::new();
        let payload = json!({
            "responseId": "resp_utf8",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{ "text": "你好，Gemini" }]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 4,
                "totalTokenCount": 8
            }
        });
        let chunk = format!("data: {}\n\n", serde_json::to_string(&payload).unwrap());
        let split_at = chunk.find("你好").unwrap() + 1;
        let bytes = chunk.into_bytes();

        let first = state.handle_bytes(&bytes[..split_at], None, || "unused".to_string());
        assert!(first.events.is_empty());

        let second = state.handle_bytes(&bytes[split_at..], None, || "unused".to_string());
        let mut events = second.events;
        events.extend(state.finish().events);
        let output = render_events(&events);

        assert!(output.contains("你好，Gemini"));
        assert!(!output.contains('\u{fffd}'));
        assert!(output.contains("\"stop_reason\":\"end_turn\""));
    }

    #[test]
    fn state_records_shadow_before_tool_events() {
        let mut state = GeminiToAnthropicSseState::new();
        let mut counter = 0;
        let chunk_output = state.handle_chunk(
            &json!({
                "responseId": "resp_tool",
                "modelVersion": "gemini-2.5-pro",
                "candidates": [{
                    "finishReason": "STOP",
                    "content": {
                        "parts": [{
                            "functionCall": {
                                "name": "Bash",
                                "args": { "command": "git status" }
                            },
                            "thoughtSignature": "sig-tool"
                        }]
                    }
                }],
                "usageMetadata": { "promptTokenCount": 5, "totalTokenCount": 8 }
            }),
            None,
            || next_synth(&mut counter),
        );
        let final_output = state.finish();

        assert_eq!(chunk_output.events[0].event_name, "message_start");
        let shadow_record = final_output
            .shadow_record
            .expect("tool call must be shadow-recorded");
        assert_eq!(
            shadow_record.assistant_content["parts"][0]["functionCall"]["name"],
            "Bash"
        );
        assert_eq!(
            shadow_record.assistant_content["parts"][0]["thoughtSignature"],
            "sig-tool"
        );

        let rendered = render_events(&final_output.events);
        assert!(rendered.contains("\"type\":\"tool_use\""));
        assert!(rendered.contains("\"stop_reason\":\"tool_use\""));
    }

    #[test]
    fn final_output_records_shadow_when_context_is_available() {
        let store = GeminiShadowStore::with_limits(8, 4);
        let mut output = GeminiStreamFinalOutput {
            events: Vec::new(),
            shadow_record: Some(GeminiStreamShadowRecord {
                assistant_content: json!({
                    "parts": [{ "text": "Hello", "thoughtSignature": "sig-1" }]
                }),
                tool_calls: Vec::new(),
            }),
        };

        let snapshot =
            output.record_shadow(Some(&store), Some("provider-a"), Some("session-1"));

        assert!(snapshot.is_some());
        assert!(output.shadow_record.is_none());
        assert_eq!(
            store
                .latest_assistant_content("provider-a", "session-1")
                .unwrap()["parts"][0]["thoughtSignature"],
            "sig-1"
        );
    }

    #[test]
    fn final_output_keeps_shadow_when_context_is_missing() {
        let mut output = GeminiStreamFinalOutput {
            events: Vec::new(),
            shadow_record: Some(GeminiStreamShadowRecord {
                assistant_content: json!({ "parts": [{ "text": "Hello" }] }),
                tool_calls: Vec::new(),
            }),
        };

        let snapshot = output.record_shadow(None, Some("provider-a"), Some("session-1"));

        assert!(snapshot.is_none());
        assert!(output.shadow_record.is_some());
    }

    #[test]
    fn state_emits_blocked_prompt_text_when_no_content_arrives() {
        let mut state = GeminiToAnthropicSseState::new();
        state.handle_chunk(
            &json!({
                "responseId": "resp_blocked",
                "modelVersion": "gemini-2.5-pro",
                "promptFeedback": { "blockReason": "SAFETY" },
                "usageMetadata": { "promptTokenCount": 3, "totalTokenCount": 3 }
            }),
            None,
            || "unused".to_string(),
        );

        let final_output = state.finish();
        let output = render_events(&final_output.events);

        assert!(output.contains("Request blocked by Gemini safety filters: SAFETY"));
        assert!(output.contains("\"stop_reason\":\"refusal\""));
        assert_eq!(
            final_output.shadow_record.unwrap().assistant_content["parts"][0]["text"],
            "Request blocked by Gemini safety filters: SAFETY"
        );
    }
}
