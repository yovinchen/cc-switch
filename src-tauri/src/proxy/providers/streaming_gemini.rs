//! Gemini Native streaming conversion module.
//!
//! Converts Gemini `streamGenerateContent?alt=sse` chunks into Anthropic-style
//! SSE events for Claude-compatible clients.

use super::transform_gemini::{synthesize_tool_call_id, AnthropicToolSchemaHints};
use crate::proxy_core::{GeminiShadowStore, GeminiToAnthropicSseState};
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use std::sync::Arc;

pub fn create_anthropic_sse_stream_from_gemini<E: std::error::Error + Send + 'static>(
    stream: impl Stream<Item = Result<Bytes, E>> + Send + 'static,
    shadow_store: Option<Arc<GeminiShadowStore>>,
    provider_id: Option<String>,
    session_id: Option<String>,
    tool_schema_hints: Option<AnthropicToolSchemaHints>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    async_stream::stream! {
        let mut state = GeminiToAnthropicSseState::new();
        tokio::pin!(stream);

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    let output = state.handle_bytes(
                        bytes.as_ref(),
                        tool_schema_hints.as_ref(),
                        synthesize_tool_call_id,
                    );
                    for name in &output.rectified_tool_names {
                        log::info!("[Claude/Gemini] Rectified tool args for `{name}`");
                    }
                    for event in output.events {
                        yield Ok(event.to_sse_bytes());
                    }
                }
                Err(error) => {
                    yield Err(std::io::Error::other(error.to_string()));
                    return;
                }
            }
        }

        let mut final_output = state.finish();
        final_output.record_shadow(
            shadow_store.as_deref(),
            provider_id.as_deref(),
            session_id.as_deref(),
        );

        for event in final_output.events {
            yield Ok(event.to_sse_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::providers::transform_gemini::anthropic_to_gemini_with_shadow;
    use crate::proxy_core::GeminiShadowStore;
    use serde_json::json;
    use std::sync::Arc;

    fn collect_stream_output(chunks: Vec<&str>) -> String {
        let owned_chunks: Vec<String> = chunks.into_iter().map(ToString::to_string).collect();
        let stream = futures::stream::iter(
            owned_chunks
                .into_iter()
                .map(|chunk| Ok::<Bytes, std::io::Error>(Bytes::from(chunk))),
        );
        let converted = create_anthropic_sse_stream_from_gemini(stream, None, None, None, None);
        futures::executor::block_on(async move {
            converted
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .map(|item| String::from_utf8(item.unwrap().to_vec()).unwrap())
                .collect::<Vec<_>>()
                .join("")
        })
    }

    fn collect_stream_output_with_shadow(
        chunks: Vec<&str>,
        store: Arc<GeminiShadowStore>,
        provider_id: &str,
        session_id: &str,
    ) -> String {
        let owned_chunks: Vec<String> = chunks.into_iter().map(ToString::to_string).collect();
        let stream = futures::stream::iter(
            owned_chunks
                .into_iter()
                .map(|chunk| Ok::<Bytes, std::io::Error>(Bytes::from(chunk))),
        );
        let converted = create_anthropic_sse_stream_from_gemini(
            stream,
            Some(store),
            Some(provider_id.to_string()),
            Some(session_id.to_string()),
            None,
        );
        futures::executor::block_on(async move {
            converted
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .map(|item| String::from_utf8(item.unwrap().to_vec()).unwrap())
                .collect::<Vec<_>>()
                .join("")
        })
    }

    #[test]
    fn converts_text_stream_to_anthropic_sse() {
        let output = collect_stream_output(vec![
            "data: {\"responseId\":\"resp_1\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}],\"usageMetadata\":{\"promptTokenCount\":10,\"totalTokenCount\":13}}\n\n",
            "data: {\"responseId\":\"resp_1\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}],\"usageMetadata\":{\"promptTokenCount\":10,\"totalTokenCount\":15}}\n\n",
        ]);

        assert!(output.contains("event: message_start"));
        assert!(output.contains("\"type\":\"text_delta\""));
        assert!(output.contains("\"text\":\"Hel\""));
        assert!(output.contains("\"text\":\"lo\""));
        assert!(output.contains("\"stop_reason\":\"end_turn\""));
        assert!(output.contains("event: message_stop"));
    }

    #[test]
    fn converts_function_call_stream_to_tool_use_events() {
        let output = collect_stream_output(vec![
            "data: {\"responseId\":\"resp_2\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"call_1\",\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\"}},\"thoughtSignature\":\"sig-1\"}]}}],\"usageMetadata\":{\"promptTokenCount\":5,\"totalTokenCount\":8}}\n\n",
        ]);

        assert!(output.contains("\"type\":\"tool_use\""));
        assert!(output.contains("\"name\":\"get_weather\""));
        assert!(output.contains("\"type\":\"input_json_delta\""));
        assert!(output.contains("\"stop_reason\":\"tool_use\""));
    }

    #[test]
    fn converts_crlf_delimited_stream_to_anthropic_sse() {
        let output = collect_stream_output(vec![
            "data: {\"responseId\":\"resp_3\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hi\"}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":6}}\r\n\r\n",
            "data: {\"responseId\":\"resp_3\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"text\":\"Hi there\"}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":9}}\r\n\r\n",
        ]);

        assert!(output.contains("event: message_start"));
        assert!(output.contains("\"type\":\"text_delta\""));
        assert!(output.contains("\"text\":\"Hi\""));
        assert!(output.contains("\"text\":\" there\""));
        assert!(output.contains("event: message_stop"));
    }

    #[test]
    fn preserves_utf8_boundaries_when_json_payload_spans_chunks() {
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
        let chunk_bytes = chunk.into_bytes();
        let stream = futures::stream::iter([
            Ok::<Bytes, std::io::Error>(Bytes::from(chunk_bytes[..split_at].to_vec())),
            Ok::<Bytes, std::io::Error>(Bytes::from(chunk_bytes[split_at..].to_vec())),
        ]);
        let converted = create_anthropic_sse_stream_from_gemini(stream, None, None, None, None);
        let output = futures::executor::block_on(async move {
            converted
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .map(|item| String::from_utf8(item.unwrap().to_vec()).unwrap())
                .collect::<Vec<_>>()
                .join("")
        });

        assert!(output.contains("你好，Gemini"));
        assert!(!output.contains('\u{fffd}'));
    }

    #[test]
    fn stores_full_text_for_shadow_replay_across_delta_chunks() {
        let store = Arc::new(GeminiShadowStore::with_limits(8, 4));
        let output = collect_stream_output_with_shadow(
            vec![
                "data: {\"responseId\":\"resp_4\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":6}}\n\n",
                "data: {\"responseId\":\"resp_4\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"text\":\"lo\"},{\"text\":\"\",\"thoughtSignature\":\"sig-1\"}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":8}}\n\n",
            ],
            store.clone(),
            "provider-a",
            "session-1",
        );

        assert!(output.contains("\"text\":\"Hel\""));
        assert!(output.contains("\"text\":\"lo\""));

        let shadow = store
            .latest_assistant_content("provider-a", "session-1")
            .unwrap();
        assert_eq!(shadow["parts"][0]["text"], "Hello");
        assert_eq!(shadow["parts"][0]["thoughtSignature"], "sig-1");

        let second_turn = anthropic_to_gemini_with_shadow(
            json!({
                "messages": [
                    { "role": "user", "content": "Hi" },
                    { "role": "assistant", "content": [{ "type": "text", "text": "Hello" }] },
                    { "role": "user", "content": "Continue" }
                ]
            }),
            Some(store.as_ref()),
            Some("provider-a"),
            Some("session-1"),
        )
        .unwrap();

        assert_eq!(second_turn["contents"][1]["role"], "model");
        assert_eq!(second_turn["contents"][1]["parts"][0]["text"], "Hello");
        assert_eq!(
            second_turn["contents"][1]["parts"][0]["thoughtSignature"],
            "sig-1"
        );
    }

    #[test]
    fn stores_tool_shadow_before_tool_use_events_are_fully_drained() {
        let store = Arc::new(GeminiShadowStore::with_limits(8, 4));
        let chunks = vec![
            "data: {\"responseId\":\"resp_tool_shadow\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"call_1\",\"name\":\"Bash\",\"args\":{\"command\":\"ls -R\"}},\"thoughtSignature\":\"sig-tool-1\"}]}}],\"usageMetadata\":{\"promptTokenCount\":5,\"totalTokenCount\":8}}\n\n".to_string(),
        ];
        let stream = futures::stream::iter(
            chunks
                .into_iter()
                .map(|chunk| Ok::<Bytes, std::io::Error>(Bytes::from(chunk))),
        );
        let mut converted = Box::pin(create_anthropic_sse_stream_from_gemini(
            stream,
            Some(store.clone()),
            Some("provider-a".to_string()),
            Some("session-1".to_string()),
            None,
        ));

        futures::executor::block_on(async {
            while let Some(item) = converted.next().await {
                let event = String::from_utf8(item.unwrap().to_vec()).unwrap();
                if event.contains("\"type\":\"tool_use\"") {
                    break;
                }
            }
        });

        let shadow = store
            .latest_assistant_content("provider-a", "session-1")
            .unwrap();
        assert_eq!(shadow["parts"][0]["functionCall"]["name"], "Bash");
        assert_eq!(shadow["parts"][0]["thoughtSignature"], "sig-tool-1");
    }

    #[test]
    fn rectifies_streamed_tool_call_args_from_tool_schema_hints() {
        let owned_chunks = vec![
            "data: {\"responseId\":\"resp_5\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"call_1\",\"name\":\"Bash\",\"args\":{\"args\":\"git status\"}}}]}}],\"usageMetadata\":{\"promptTokenCount\":5,\"totalTokenCount\":8}}\n\n".to_string(),
        ];
        let stream = futures::stream::iter(
            owned_chunks
                .into_iter()
                .map(|chunk| Ok::<Bytes, std::io::Error>(Bytes::from(chunk))),
        );
        let hints = super::super::transform_gemini::extract_anthropic_tool_schema_hints(&json!({
            "tools": [{
                "name": "Bash",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" },
                        "timeout": { "type": "number" }
                    },
                    "required": ["command"]
                }
            }]
        }));
        let converted =
            create_anthropic_sse_stream_from_gemini(stream, None, None, None, Some(hints));
        let output = futures::executor::block_on(async move {
            converted
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .map(|item| String::from_utf8(item.unwrap().to_vec()).unwrap())
                .collect::<Vec<_>>()
                .join("")
        });

        assert!(output.contains("\"partial_json\":\"{\\\"command\\\":\\\"git status\\\"}\""));
    }

    #[test]
    fn rectifies_streamed_skill_args_from_nested_parameters() {
        let payload = json!({
            "responseId": "resp_6",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{
                        "functionCall": {
                            "id": "call_1",
                            "name": "Skill",
                            "args": {
                                "name": "git-commit",
                                "parameters": {
                                    "args": ["详细分析内容 编写提交信息 分多次提交代码"]
                                }
                            }
                        }
                    }]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "totalTokenCount": 8
            }
        });
        let owned_chunks = vec![format!(
            "data: {}\n\n",
            serde_json::to_string(&payload).unwrap()
        )];
        let stream = futures::stream::iter(
            owned_chunks
                .into_iter()
                .map(|chunk| Ok::<Bytes, std::io::Error>(Bytes::from(chunk))),
        );
        let hints = super::super::transform_gemini::extract_anthropic_tool_schema_hints(&json!({
            "tools": [{
                "name": "Skill",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "skill": { "type": "string" },
                        "args": { "type": "string" }
                    },
                    "required": ["skill"]
                }
            }]
        }));
        let converted =
            create_anthropic_sse_stream_from_gemini(stream, None, None, None, Some(hints));
        let output = futures::executor::block_on(async move {
            converted
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .map(|item| String::from_utf8(item.unwrap().to_vec()).unwrap())
                .collect::<Vec<_>>()
                .join("")
        });

        assert!(output.contains("git-commit"));
        assert!(output.contains("详细分析内容 编写提交信息 分多次提交代码"));
        assert!(!output.contains("\\\"parameters\\\""));
    }

    /// Regression for the P1 finding: when Gemini emits two parallel calls to
    /// the same function without providing ids, both must be surfaced to the
    /// Anthropic client with distinct synthesized ids. The previous
    /// name-based fallback in `merge_tool_call_snapshots` collapsed them into
    /// a single entry, causing silent data loss for the first call.
    #[test]
    fn parallel_same_name_no_id_calls_preserve_both() {
        let output = collect_stream_output(vec![
            "data: {\"responseId\":\"r1\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\"}}},{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Osaka\"}}}]}}],\"usageMetadata\":{\"promptTokenCount\":5,\"totalTokenCount\":8}}\n\n",
        ]);

        let tool_use_start_count = output.matches("\"type\":\"tool_use\"").count();
        assert_eq!(
            tool_use_start_count, 2,
            "both parallel calls must survive merge_tool_call_snapshots"
        );
        // `input_json_delta.partial_json` is a string, so the city keys appear
        // JSON-escaped inside the outer SSE `data:` payload. Match against
        // the raw escape sequences rather than the canonical JSON form.
        assert!(output.contains("Tokyo"));
        assert!(output.contains("Osaka"));
        // Each tool_use must carry a non-empty synthesized id so Claude Code
        // can disambiguate the two tool_result round-trips.
        let synth_count = output.matches("\"id\":\"gemini_synth_").count();
        assert_eq!(synth_count, 2);
    }

    /// When Gemini keeps sending the same no-id functionCall across cumulative
    /// chunks, the synthesized id must stay stable so the Anthropic client
    /// sees a single tool_use block with consistent args updates rather than
    /// duplicates.
    #[test]
    fn no_id_tool_call_reuses_synthesized_id_across_cumulative_chunks() {
        let output = collect_stream_output(vec![
            "data: {\"responseId\":\"r2\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\"}}}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":6}}\n\n",
            "data: {\"responseId\":\"r2\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\",\"units\":\"c\"}}}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":9}}\n\n",
        ]);

        assert_eq!(output.matches("\"type\":\"tool_use\"").count(), 1);
        assert!(output.contains("\"units\\\":\\\"c\\\""));
    }

    /// Regression for the follow-up Codex P1: some Gemini relays serialize
    /// an absent functionCall id as `"id": ""` rather than omitting the
    /// field. Without a filter, `Some("")` would reach
    /// `merge_tool_call_snapshots`, two parallel no-id calls would match
    /// each other on the empty-string id, and the second would overwrite
    /// the first — silently losing a call. Also the emitted Anthropic
    /// `tool_use.id` would be the empty string, so tool_result
    /// correlation from the Claude client would break.
    #[test]
    fn parallel_empty_string_id_calls_are_treated_as_missing_and_preserved() {
        let output = collect_stream_output(vec![
            "data: {\"responseId\":\"r3\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"\",\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\"}}},{\"functionCall\":{\"id\":\"\",\"name\":\"get_weather\",\"args\":{\"city\":\"Osaka\"}}}]}}],\"usageMetadata\":{\"promptTokenCount\":5,\"totalTokenCount\":8}}\n\n",
        ]);

        let tool_use_count = output.matches("\"type\":\"tool_use\"").count();
        assert_eq!(
            tool_use_count, 2,
            "both parallel calls must survive even when ids are explicit empty strings"
        );
        assert!(output.contains("Tokyo"));
        assert!(output.contains("Osaka"));
        // No tool_use may emit an empty id — each must get its own
        // synthesized id so tool_result correlation works.
        assert!(
            !output.contains("\"id\":\"\""),
            "empty tool_use id leaked through: {output}"
        );
        let synth_count = output.matches("\"id\":\"gemini_synth_").count();
        assert_eq!(synth_count, 2);
    }

    /// Companion regression: a single-chunk stream whose sole functionCall
    /// carries `"id": ""` must still emit exactly one tool_use with a
    /// synthesized id, not an empty one. This covers the non-parallel
    /// degraded-relay case that the parallel test above subsumes.
    #[test]
    fn single_empty_string_id_tool_call_gets_synthesized_id() {
        let output = collect_stream_output(vec![
            "data: {\"responseId\":\"r4\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"\",\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\"}}}]}}],\"usageMetadata\":{\"promptTokenCount\":3,\"totalTokenCount\":5}}\n\n",
        ]);

        assert_eq!(output.matches("\"type\":\"tool_use\"").count(), 1);
        assert!(!output.contains("\"id\":\"\""));
        assert_eq!(output.matches("\"id\":\"gemini_synth_").count(), 1);
    }

    /// Regression for Codex P1: Gemini's cumulative stream may deliver a
    /// `functionCall` without an id (we synthesize one) and then upgrade
    /// to a genuine id on a later chunk. Without a positional fallback in
    /// the `Some(incoming_id)` branch of `merge_tool_call_snapshots`, the
    /// real id would fail to match the existing synthesized snapshot and
    /// push a second entry — yielding duplicate `tool_use` blocks at
    /// stream end (one synthesized, one real) and breaking tool_result
    /// correlation.
    #[test]
    fn upgraded_real_id_merges_into_existing_synthesized_snapshot() {
        let output = collect_stream_output(vec![
            // Chunk 1: no id -> a `gemini_synth_*` id is assigned.
            "data: {\"responseId\":\"rupg\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\"}}}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":6}}\n\n",
            // Chunk 2: cumulative snapshot upgrades the same call to a
            // real Gemini id. Must merge into the existing slot, not
            // spawn a second snapshot.
            "data: {\"responseId\":\"rupg\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"real_id_abc\",\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\",\"units\":\"c\"}}}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":9}}\n\n",
        ]);

        // Exactly one tool_use block (not two).
        assert_eq!(
            output.matches("\"type\":\"tool_use\"").count(),
            1,
            "id upgrade must merge into the synthesized snapshot, not duplicate it: {output}"
        );
        // The emitted tool_use id is the real Gemini id, not the synthesized one.
        assert!(
            output.contains("\"id\":\"real_id_abc\""),
            "expected real id to win after upgrade: {output}"
        );
        assert!(
            !output.contains("\"id\":\"gemini_synth_"),
            "synthesized id must be dropped when a real id arrives: {output}"
        );
        // Args from the final cumulative snapshot are emitted.
        assert!(output.contains("units"));
    }

    /// Regression for Codex P2: Gemini's cumulative stream may include
    /// `thoughtSignature` on one chunk and omit it on a later cumulative
    /// snapshot of the same call. A blind `tool_call_snapshots[index] =
    /// tool_call` overwrite would drop the signature, so the shadow turn
    /// recorded (and later replayed to Gemini) would miss
    /// `thoughtSignature` and the upstream would reject the follow-up.
    /// `merge_tool_call_snapshots` must retain the prior signature when
    /// the incoming chunk does not carry one.
    #[test]
    fn thought_signature_preserved_when_later_chunk_omits_it() {
        let store = Arc::new(GeminiShadowStore::with_limits(8, 4));
        collect_stream_output_with_shadow(
            vec![
                // Chunk 1: carries thoughtSignature "sig-keep".
                "data: {\"responseId\":\"rsig\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"call_1\",\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\"}},\"thoughtSignature\":\"sig-keep\"}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":6}}\n\n",
                // Chunk 2: cumulative update for the same call, but
                // thoughtSignature is omitted — common for Gemini's
                // one-shot signature fields.
                "data: {\"responseId\":\"rsig\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"call_1\",\"name\":\"get_weather\",\"args\":{\"city\":\"Tokyo\",\"units\":\"c\"}}}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":9}}\n\n",
            ],
            store.clone(),
            "provider-sig",
            "session-sig",
        );

        let shadow = store
            .latest_assistant_content("provider-sig", "session-sig")
            .expect("shadow turn must be recorded");
        assert_eq!(shadow["parts"][0]["functionCall"]["id"], "call_1");
        assert_eq!(
            shadow["parts"][0]["thoughtSignature"], "sig-keep",
            "prior thoughtSignature must survive a later chunk that omits it: {shadow}"
        );
    }
}
