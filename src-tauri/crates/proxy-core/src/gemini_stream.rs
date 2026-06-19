//! Gemini Native streaming state helpers.
//!
//! These helpers keep Gemini cumulative `content.parts` interpretation in the
//! host-neutral core while leaving async transport and SSE emission in the host.

use crate::gemini_shadow::GeminiToolCallMeta;
use crate::gemini_tool_args::{rectify_gemini_tool_call_parts, AnthropicToolSchemaHints};
use serde_json::{json, Value};

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
}
