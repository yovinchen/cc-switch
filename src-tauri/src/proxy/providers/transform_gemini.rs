//! Gemini Native format conversion module.
//!
//! Converts Anthropic Messages requests to Gemini `generateContent` requests,
//! and Gemini `GenerateContentResponse` payloads back to Anthropic Messages
//! responses for Claude-compatible clients.

use crate::proxy::error::ProxyError;
use crate::proxy_core::{
    AnthropicToolSchemaHints, GeminiShadowStore, anthropic_request_to_gemini_request,
    extract_anthropic_tool_schema_hints as core_extract_anthropic_tool_schema_hints,
    gemini_response_to_anthropic_message, rectify_gemini_tool_call_args,
    synthesize_gemini_tool_call_id,
};
use serde_json::Value;

/// Generate a unique tool-call id suffix for the core Gemini synthesized-id
/// contract. The host owns randomness; proxy-core owns the visible id shape.
pub(crate) fn synthesize_tool_call_id() -> String {
    synthesize_gemini_tool_call_id(uuid::Uuid::new_v4().simple().to_string())
}

/// Anthropic 请求 → Gemini 原生请求。
///
/// 转换工具库 API：当前无生产调用方（连通性检查不再发真实请求，曾是其唯一 crate 内
/// 消费者），但保留其转换逻辑与下方测试套件，供代理转换路径复用 / 未来接线。
#[allow(dead_code)]
pub fn anthropic_to_gemini(body: Value) -> Result<Value, ProxyError> {
    anthropic_to_gemini_with_shadow(body, None, None, None)
}

pub fn anthropic_to_gemini_with_shadow(
    body: Value,
    shadow_store: Option<&GeminiShadowStore>,
    provider_id: Option<&str>,
    session_id: Option<&str>,
) -> Result<Value, ProxyError> {
    let shadow_turns = shadow_store
        .zip(provider_id)
        .zip(session_id)
        .and_then(|((store, provider_id), session_id)| store.get_session(provider_id, session_id))
        .map(|snapshot| snapshot.turns)
        .unwrap_or_default();

    anthropic_request_to_gemini_request(&body, &shadow_turns).map_err(ProxyError::TransformError)
}

/// Convenience wrapper over [`gemini_to_anthropic_with_shadow_and_hints`]
/// with no shadow store or schema hints. Used by the shared
/// `ProviderAdapter::transform_response` path and by tests.
#[allow(dead_code)] // kept as public API for non-streaming transform paths
pub fn gemini_to_anthropic(body: Value) -> Result<Value, ProxyError> {
    gemini_to_anthropic_with_shadow(body, None, None, None)
}

/// Convenience wrapper for callers that have a shadow store but no tool
/// schema hints. Production call sites funnel through
/// [`gemini_to_anthropic_with_shadow_and_hints`] directly; this helper exists
/// for test ergonomics and future external callers.
#[allow(dead_code)] // kept as public API for shadow-only transform paths
pub fn gemini_to_anthropic_with_shadow(
    body: Value,
    shadow_store: Option<&GeminiShadowStore>,
    provider_id: Option<&str>,
    session_id: Option<&str>,
) -> Result<Value, ProxyError> {
    gemini_to_anthropic_with_shadow_and_hints(body, shadow_store, provider_id, session_id, None)
}

pub fn gemini_to_anthropic_with_shadow_and_hints(
    body: Value,
    shadow_store: Option<&GeminiShadowStore>,
    provider_id: Option<&str>,
    session_id: Option<&str>,
    tool_schema_hints: Option<&AnthropicToolSchemaHints>,
) -> Result<Value, ProxyError> {
    let output =
        gemini_response_to_anthropic_message(&body, tool_schema_hints, synthesize_tool_call_id)
            .map_err(ProxyError::TransformError)?;

    for name in &output.rectified_tool_names {
        log::info!("[Claude/Gemini] Rectified tool args for `{name}`");
    }

    if let (Some(store), Some(provider_id), Some(session_id), Some(shadow_record)) =
        (shadow_store, provider_id, session_id, output.shadow_record)
    {
        store.record_assistant_turn(
            provider_id,
            session_id,
            shadow_record.assistant_content,
            shadow_record.tool_calls,
        );
    }

    Ok(output.response)
}

pub fn extract_anthropic_tool_schema_hints(body: &Value) -> AnthropicToolSchemaHints {
    core_extract_anthropic_tool_schema_hints(body)
}

#[allow(dead_code)]
pub fn rectify_tool_call_args(
    tool_name: &str,
    args: &mut Value,
    tool_schema_hints: Option<&AnthropicToolSchemaHints>,
) -> bool {
    rectify_gemini_tool_call_args(tool_name, args, tool_schema_hints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::{GeminiToolCallMeta, is_synthesized_gemini_tool_call_id};
    use serde_json::json;

    #[test]
    fn anthropic_to_gemini_maps_system_and_messages() {
        let input = json!({
            "model": "gemini-2.5-pro",
            "max_tokens": 128,
            "system": "You are helpful.",
            "messages": [
                { "role": "user", "content": "Hello" }
            ]
        });

        let result = anthropic_to_gemini(input).unwrap();
        assert_eq!(
            result["systemInstruction"]["parts"][0]["text"],
            "You are helpful."
        );
        assert_eq!(result["contents"][0]["role"], "user");
        assert_eq!(result["contents"][0]["parts"][0]["text"], "Hello");
        assert_eq!(result["generationConfig"]["maxOutputTokens"], 128);
    }

    #[test]
    fn anthropic_to_gemini_merges_system_messages_into_system_instruction() {
        let input = json!({
            "model": "gemini-3-pro",
            "system": [{ "type": "text", "text": "Top level system." }],
            "messages": [
                { "role": "system", "content": "Message system." },
                {
                    "role": "system",
                    "content": [{ "type": "text", "text": "Block system." }]
                },
                { "role": "user", "content": "Hello" }
            ]
        });

        let result = anthropic_to_gemini(input).unwrap();

        assert_eq!(
            result["systemInstruction"]["parts"][0]["text"],
            "Top level system.\n\nMessage system.\n\nBlock system."
        );
        assert_eq!(result["contents"].as_array().unwrap().len(), 1);
        assert_eq!(result["contents"][0]["role"], "user");
        assert_eq!(result["contents"][0]["parts"][0]["text"], "Hello");
    }

    #[test]
    fn anthropic_to_gemini_maps_tools_and_tool_results() {
        let input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        { "type": "tool_use", "id": "call_1", "name": "get_weather", "input": { "city": "Tokyo" } }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_1", "content": "Sunny" }
                    ]
                }
            ],
            "tools": [
                {
                    "name": "get_weather",
                    "description": "Weather lookup",
                    "input_schema": { "type": "object", "properties": { "city": { "type": "string" } } }
                }
            ],
            "tool_choice": { "type": "tool", "name": "get_weather" }
        });

        let result = anthropic_to_gemini(input).unwrap();
        assert_eq!(
            result["tools"][0]["functionDeclarations"][0]["name"],
            "get_weather"
        );
        assert!(
            result["tools"][0]["functionDeclarations"][0]
                .get("parameters")
                .is_some()
        );
        assert_eq!(
            result["contents"][0]["parts"][0]["functionCall"]["name"],
            "get_weather"
        );
        assert_eq!(
            result["contents"][1]["parts"][0]["functionResponse"]["name"],
            "get_weather"
        );
        assert_eq!(
            result["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"][0],
            "get_weather"
        );
    }

    #[test]
    fn anthropic_to_gemini_resolves_tool_result_name_from_shadow_content() {
        let store = GeminiShadowStore::with_limits(8, 4);
        store.record_assistant_turn(
            "provider-a",
            "session-1",
            json!({
                "parts": [{
                    "functionCall": {
                        "id": "call_1",
                        "name": "get_weather",
                        "args": { "city": "Tokyo" }
                    }
                }]
            }),
            vec![],
        );

        let input = json!({
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_1", "content": "Sunny" }
                    ]
                }
            ]
        });

        let result = anthropic_to_gemini_with_shadow(
            input,
            Some(&store),
            Some("provider-a"),
            Some("session-1"),
        )
        .unwrap();

        assert_eq!(
            result["contents"][0]["parts"][0]["functionResponse"]["name"],
            "get_weather"
        );
    }

    #[test]
    fn anthropic_to_gemini_rejects_tool_result_without_resolvable_name() {
        let input = json!({
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_1", "content": "Sunny" }
                    ]
                }
            ]
        });

        let error = anthropic_to_gemini(input).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Unable to resolve Gemini functionResponse.name")
        );
    }

    #[test]
    fn anthropic_to_gemini_uses_parameters_json_schema_for_rich_tool_schema() {
        let input = json!({
            "tools": [
                {
                    "name": "search",
                    "description": "Search data",
                    "input_schema": {
                        "$schema": "https://json-schema.org/draft/2020-12/schema",
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" }
                        },
                        "required": ["query"],
                        "additionalProperties": false
                    }
                }
            ]
        });

        let result = anthropic_to_gemini(input).unwrap();
        let declaration = &result["tools"][0]["functionDeclarations"][0];

        assert!(declaration.get("parameters").is_none());
        assert!(declaration.get("parametersJsonSchema").is_some());
        assert!(declaration["parametersJsonSchema"].get("$schema").is_none());
        assert_eq!(
            declaration["parametersJsonSchema"]["additionalProperties"],
            false
        );
    }

    #[test]
    fn gemini_to_anthropic_maps_text_and_usage() {
        let input = json!({
            "responseId": "resp_1",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{ "text": "Hello from Gemini" }]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 12,
                "totalTokenCount": 20,
                "cachedContentTokenCount": 3
            }
        });

        let result = gemini_to_anthropic(input).unwrap();
        assert_eq!(result["id"], "resp_1");
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "Hello from Gemini");
        assert_eq!(result["stop_reason"], "end_turn");
        // input_tokens = promptTokenCount(12) - cachedContentTokenCount(3) = 9（fresh input）。
        // Gemini 的 promptTokenCount 含缓存命中，但 Anthropic 语义要求 input 不含 cache、
        // cache_read 单列；二者相加(9+3)=总输入 12。扣减避免本路径以 app_type=claude
        // 记账时把缓存 token 双重计费。
        assert_eq!(result["usage"]["input_tokens"], 9);
        assert_eq!(result["usage"]["output_tokens"], 8);
        assert_eq!(result["usage"]["cache_read_input_tokens"], 3);
    }

    #[test]
    fn gemini_to_anthropic_maps_function_calls_to_tool_use() {
        let input = json!({
            "responseId": "resp_2",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{
                        "functionCall": {
                            "id": "call_1",
                            "name": "get_weather",
                            "args": { "city": "Tokyo" }
                        }
                    }]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "totalTokenCount": 15
            }
        });

        let result = gemini_to_anthropic(input).unwrap();
        assert_eq!(result["content"][0]["type"], "tool_use");
        assert_eq!(result["content"][0]["id"], "call_1");
        assert_eq!(result["stop_reason"], "tool_use");
    }

    #[test]
    fn gemini_to_anthropic_rectifies_tool_args_from_schema_hints() {
        let input = json!({
            "responseId": "resp_2",
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
            }]
        });
        let hints = extract_anthropic_tool_schema_hints(&json!({
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

        let result =
            gemini_to_anthropic_with_shadow_and_hints(input, None, None, None, Some(&hints))
                .unwrap();

        assert_eq!(result["content"][0]["input"]["skill"], "git-commit");
        assert_eq!(
            result["content"][0]["input"]["args"],
            "详细分析内容 编写提交信息 分多次提交代码"
        );
        assert!(result["content"][0]["input"].get("name").is_none());
        assert!(result["content"][0]["input"].get("parameters").is_none());
    }

    #[test]
    fn gemini_to_anthropic_preserves_legitimate_parameters_arg() {
        let input = json!({
            "responseId": "resp_params",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{
                        "functionCall": {
                            "id": "call_1",
                            "name": "ConfigTool",
                            "args": {
                                "parameters": {
                                    "mode": "safe",
                                    "retries": 2
                                }
                            }
                        }
                    }]
                }
            }]
        });
        let hints = extract_anthropic_tool_schema_hints(&json!({
            "tools": [{
                "name": "ConfigTool",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "mode": { "type": "string" },
                                "retries": { "type": "integer" }
                            }
                        }
                    },
                    "required": ["parameters"]
                }
            }]
        }));

        let result =
            gemini_to_anthropic_with_shadow_and_hints(input, None, None, None, Some(&hints))
                .unwrap();

        assert_eq!(result["content"][0]["input"]["parameters"]["mode"], "safe");
        assert_eq!(result["content"][0]["input"]["parameters"]["retries"], 2);
    }

    #[test]
    fn gemini_to_anthropic_maps_blocked_prompt_to_refusal() {
        let input = json!({
            "responseId": "resp_3",
            "modelVersion": "gemini-2.5-flash",
            "promptFeedback": { "blockReason": "SAFETY" },
            "usageMetadata": {
                "promptTokenCount": 4,
                "totalTokenCount": 4
            }
        });

        let result = gemini_to_anthropic(input).unwrap();
        assert_eq!(result["stop_reason"], "refusal");
        assert_eq!(result["content"][0]["type"], "text");
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("SAFETY")
        );
    }

    #[test]
    fn shadow_replay_aligns_to_latest_turns_after_client_truncation() {
        let store = GeminiShadowStore::with_limits(8, 4);
        // Record 3 shadow turns (assistant messages 0, 1, 2)
        for i in 0..3 {
            store.record_assistant_turn(
                "prov",
                "sess",
                json!({
                    "parts": [{
                        "functionCall": {
                            "id": format!("call_{i}"),
                            "name": format!("tool_{i}"),
                            "args": {}
                        }
                    }]
                }),
                vec![],
            );
        }

        // Client truncates history: only sends assistant messages 1 and 2
        let input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        { "type": "tool_use", "id": "call_1", "name": "tool_1", "input": {} }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_1", "content": "ok" }
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        { "type": "tool_use", "id": "call_2", "name": "tool_2", "input": {} }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_2", "content": "ok" }
                    ]
                }
            ]
        });

        let result =
            anthropic_to_gemini_with_shadow(input, Some(&store), Some("prov"), Some("sess"))
                .unwrap();

        // Shadow turns[1] (tool_1) should align with first assistant message,
        // shadow turns[2] (tool_2) with the second — not turns[0] and turns[1].
        assert_eq!(
            result["contents"][0]["parts"][0]["functionCall"]["name"],
            "tool_1"
        );
        assert_eq!(
            result["contents"][2]["parts"][0]["functionCall"]["name"],
            "tool_2"
        );
    }

    #[test]
    fn shadow_replay_matches_tool_use_turn_by_id_when_position_drifts() {
        let store = GeminiShadowStore::with_limits(8, 4);
        store.record_assistant_turn(
            "prov",
            "sess",
            json!({
                "parts": [{
                    "functionCall": {
                        "id": "call_1",
                        "name": "Bash",
                        "args": { "command": "ls -R" }
                    },
                    "thoughtSignature": "sig-tool-1"
                }]
            }),
            vec![GeminiToolCallMeta::new(
                Some("call_1"),
                "Bash",
                json!({ "command": "ls -R" }),
                Some("sig-tool-1"),
            )],
        );

        let input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "tool_use",
                            "id": "call_1",
                            "name": "default_api:Bash",
                            "input": { "command": "ls -R" }
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_1", "content": "ok" }
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        { "type": "text", "text": "local-only assistant turn without Gemini shadow" }
                    ]
                }
            ]
        });

        let result =
            anthropic_to_gemini_with_shadow(input, Some(&store), Some("prov"), Some("sess"))
                .unwrap();

        assert_eq!(
            result["contents"][0]["parts"][0]["functionCall"]["name"],
            "Bash"
        );
        assert_eq!(
            result["contents"][0]["parts"][0]["thoughtSignature"],
            "sig-tool-1"
        );
    }

    /// Regression for P1: two shadow turns whose suffix-normalized names
    /// collide (e.g. `server_a:search` / `server_b:search` both normalize to
    /// `search`). When the incoming assistant tool_use carries a valid,
    /// different id, exact-id matching must win over the normalized-name
    /// clause — otherwise replay picks the wrong shadow turn and later
    /// tool_result resolution mis-routes.
    #[test]
    fn shadow_replay_prefers_exact_id_match_over_normalized_name_collision() {
        let store = GeminiShadowStore::with_limits(8, 4);
        store.record_assistant_turn(
            "prov",
            "sess",
            json!({
                "parts": [{
                    "functionCall": {
                        "id": "call_a",
                        "name": "server_a:search",
                        "args": { "q": "alpha" }
                    },
                    "thoughtSignature": "sig-a"
                }]
            }),
            vec![GeminiToolCallMeta::new(
                Some("call_a"),
                "server_a:search",
                json!({ "q": "alpha" }),
                Some("sig-a"),
            )],
        );
        store.record_assistant_turn(
            "prov",
            "sess",
            json!({
                "parts": [{
                    "functionCall": {
                        "id": "call_b",
                        "name": "server_b:search",
                        "args": { "q": "beta" }
                    },
                    "thoughtSignature": "sig-b"
                }]
            }),
            vec![GeminiToolCallMeta::new(
                Some("call_b"),
                "server_b:search",
                json!({ "q": "beta" }),
                Some("sig-b"),
            )],
        );

        // Two assistant turns: the first references call_b, the second
        // call_a. Positional fallback would align msg[0] to turn 0 (call_a)
        // and msg[1] to turn 1 (call_b) — both wrong. The old `||` chain
        // would also mis-match through the normalized "search" name.
        let input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        { "type": "tool_use", "id": "call_b", "name": "server_b:search", "input": { "q": "beta" } }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_b", "content": "ok-b" }
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        { "type": "tool_use", "id": "call_a", "name": "server_a:search", "input": { "q": "alpha" } }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_a", "content": "ok-a" }
                    ]
                }
            ]
        });

        let result =
            anthropic_to_gemini_with_shadow(input, Some(&store), Some("prov"), Some("sess"))
                .unwrap();

        // msg[0] replays shadow turn 1 (server_b:search) because id=call_b.
        assert_eq!(
            result["contents"][0]["parts"][0]["functionCall"]["name"],
            "server_b:search"
        );
        assert_eq!(
            result["contents"][0]["parts"][0]["thoughtSignature"],
            "sig-b"
        );
        // msg[2] replays shadow turn 0 (server_a:search) because id=call_a,
        // even though turn 1 was already consumed above.
        assert_eq!(
            result["contents"][2]["parts"][0]["functionCall"]["name"],
            "server_a:search"
        );
        assert_eq!(
            result["contents"][2]["parts"][0]["thoughtSignature"],
            "sig-a"
        );
    }

    /// When the incoming tool_use carries no id (or only empty-string ids),
    /// the layered matcher must still fall back to name-based matching so
    /// that shadow replay keeps working for providers that omit ids.
    #[test]
    fn shadow_replay_falls_back_to_name_when_ids_absent() {
        let store = GeminiShadowStore::with_limits(8, 4);
        store.record_assistant_turn(
            "prov",
            "sess",
            json!({
                "parts": [{
                    "functionCall": {
                        "name": "lookup",
                        "args": {}
                    },
                    "thoughtSignature": "sig-lookup"
                }]
            }),
            vec![GeminiToolCallMeta::new(
                None::<&str>,
                "lookup",
                json!({}),
                Some("sig-lookup"),
            )],
        );

        // id is an empty string; extract_assistant_tool_use_keys filters it
        // out, so tool_use_ids is empty and matching must go through names.
        // A trailing user text turn keeps the assistant turn well-formed
        // without feeding a tool_result back (which would require a real id).
        let input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        { "type": "tool_use", "id": "", "name": "lookup", "input": {} }
                    ]
                },
                {
                    "role": "user",
                    "content": "ack"
                }
            ]
        });

        let result =
            anthropic_to_gemini_with_shadow(input, Some(&store), Some("prov"), Some("sess"))
                .unwrap();

        assert_eq!(
            result["contents"][0]["parts"][0]["functionCall"]["name"],
            "lookup"
        );
        assert_eq!(
            result["contents"][0]["parts"][0]["thoughtSignature"],
            "sig-lookup"
        );
    }

    /// Regression for P1: Gemini 2.x may return parallel calls without ids.
    /// Each Anthropic-visible tool_use must carry a unique id so the Claude
    /// Code client can map tool_result responses back correctly.
    #[test]
    fn gemini_to_anthropic_synthesizes_unique_ids_for_missing_functioncall_ids() {
        let input = json!({
            "responseId": "r1",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [
                        { "functionCall": { "name": "foo", "args": {} } },
                        { "functionCall": { "name": "foo", "args": { "k": 1 } } }
                    ]
                }
            }]
        });

        let result = gemini_to_anthropic(input).unwrap();
        let id0 = result["content"][0]["id"].as_str().unwrap();
        let id1 = result["content"][1]["id"].as_str().unwrap();
        assert!(is_synthesized_gemini_tool_call_id(id0));
        assert!(is_synthesized_gemini_tool_call_id(id1));
        assert_ne!(id0, id1, "synthesized ids must be unique per call");
    }

    /// Ensures the proxy does not leak synthesized ids back to Gemini when
    /// Claude Code replies with a tool_result: the id must be stripped from
    /// both `functionCall.id` and `functionResponse.id`.
    #[test]
    fn tool_result_with_synthesized_id_omits_id_in_gemini_request() {
        let synth = synthesize_tool_call_id();
        let input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        { "type": "tool_use", "id": &synth, "name": "get_weather", "input": { "city": "X" } }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": &synth, "content": "sunny" }
                    ]
                }
            ]
        });

        let result = anthropic_to_gemini(input).unwrap();
        let fc = &result["contents"][0]["parts"][0]["functionCall"];
        assert!(
            fc.get("id").is_none(),
            "synthesized id must not leak upstream in functionCall"
        );
        assert_eq!(fc["name"], "get_weather");
        let fr = &result["contents"][1]["parts"][0]["functionResponse"];
        assert!(
            fr.get("id").is_none(),
            "synthesized id must not leak upstream in functionResponse"
        );
        assert_eq!(fr["name"], "get_weather");
    }

    /// Genuine Gemini-assigned ids must round-trip unchanged so that Gemini
    /// can correlate the tool result with its own prior functionCall entry.
    #[test]
    fn tool_result_with_genuine_gemini_id_round_trips() {
        let input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        { "type": "tool_use", "id": "call_real_1", "name": "get_weather", "input": {} }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_real_1", "content": "ok" }
                    ]
                }
            ]
        });

        let result = anthropic_to_gemini(input).unwrap();
        assert_eq!(
            result["contents"][0]["parts"][0]["functionCall"]["id"],
            "call_real_1"
        );
        assert_eq!(
            result["contents"][1]["parts"][0]["functionResponse"]["id"],
            "call_real_1"
        );
    }

    /// Shadow replay must also strip synthesized ids when it reconstructs
    /// the assistant's `functionCall` parts from a previously recorded turn.
    #[test]
    fn shadow_replay_strips_synthesized_id_from_function_call() {
        let store = GeminiShadowStore::with_limits(8, 4);
        let synth = synthesize_tool_call_id();
        store.record_assistant_turn(
            "prov",
            "sess",
            json!({
                "parts": [{
                    "functionCall": {
                        "id": &synth,
                        "name": "get_weather",
                        "args": { "city": "Tokyo" }
                    }
                }]
            }),
            vec![GeminiToolCallMeta::new(
                Some(synth.clone()),
                "get_weather",
                json!({ "city": "Tokyo" }),
                None::<String>,
            )],
        );

        let input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        { "type": "tool_use", "id": &synth, "name": "get_weather", "input": { "city": "Tokyo" } }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": &synth, "content": "sunny" }
                    ]
                }
            ]
        });

        let result =
            anthropic_to_gemini_with_shadow(input, Some(&store), Some("prov"), Some("sess"))
                .unwrap();
        // The assistant message was replayed from shadow; its synthesized id
        // must be absent from the upstream functionCall representation.
        assert!(
            result["contents"][0]["parts"][0]["functionCall"]
                .get("id")
                .is_none()
        );
        // And the tool_result round-trip must still resolve the name via the
        // shadow map even when the id is synthesized.
        assert_eq!(
            result["contents"][1]["parts"][0]["functionResponse"]["name"],
            "get_weather"
        );
    }

    // ------------------------------------------------------------------
    // Non-streaming shadow id coherence regressions.
    //
    // When Gemini returns a `functionCall` without an id (common in 2.x
    // parallel calls) the proxy must synthesize a single id that is
    // consistent across:
    //   (a) the Anthropic `content[tool_use].id` sent to the client
    //   (b) `shadow_content.parts[].functionCall.id` recorded in shadow
    //   (c) `shadow_turn.tool_calls[].id` recorded in shadow
    // Previously the non-streaming path generated independent UUIDs in (a)
    // and (c), so the next round's `tool_result(tool_use_id=A)` would
    // fail to resolve through `tool_name_by_id` (populated from (c)).
    // ------------------------------------------------------------------

    /// The id surfaced to the Anthropic client must equal the id recorded
    /// in the shadow's `tool_calls` metadata and the shadow's serialized
    /// `functionCall.id`. All three are read back as the same string.
    #[test]
    fn non_stream_shadow_id_matches_client_visible_id() {
        let store = GeminiShadowStore::with_limits(8, 4);
        let body = json!({
            "responseId": "r-coherence",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{
                        "functionCall": { "name": "get_weather", "args": { "city": "Tokyo" } }
                    }]
                }
            }]
        });

        let response = gemini_to_anthropic_with_shadow_and_hints(
            body,
            Some(&store),
            Some("prov"),
            Some("sess"),
            None,
        )
        .unwrap();

        let client_id = response["content"][0]["id"].as_str().unwrap();
        assert!(
            is_synthesized_gemini_tool_call_id(client_id),
            "client-facing id must be synthesized for no-id Gemini responses"
        );

        let snapshot = store.get_session("prov", "sess").expect("shadow recorded");
        // (c) tool_calls metadata must agree with the client-visible id.
        let shadow_tool_call_id = snapshot.turns[0].tool_calls[0]
            .id
            .as_deref()
            .expect("tool_calls id populated");
        assert_eq!(
            shadow_tool_call_id, client_id,
            "shadow.tool_calls id must equal client-visible id"
        );
        // (b) assistant_content parts must agree too, so that
        // `merge_tool_names_from_parts` sees the same id on replay.
        let shadow_part_id = snapshot.turns[0].assistant_content["parts"][0]["functionCall"]["id"]
            .as_str()
            .expect("assistant_content functionCall id populated");
        assert_eq!(
            shadow_part_id, client_id,
            "shadow assistant_content functionCall.id must equal client-visible id"
        );
    }

    /// Scenario A: the client-side history was truncated so the next
    /// request only contains `[tool_result(tool_use_id=A)]` without a
    /// preceding assistant echo. The request must still resolve because
    /// `build_tool_name_map_from_shadow_turns` now surfaces the same id
    /// the client was given.
    #[test]
    fn non_stream_missing_id_scenario_a_truncated_history_resolves() {
        let store = GeminiShadowStore::with_limits(8, 4);
        let turn1 = json!({
            "responseId": "r-truncated",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{
                        "functionCall": { "name": "get_weather", "args": { "city": "Tokyo" } }
                    }]
                }
            }]
        });
        let anthropic_response = gemini_to_anthropic_with_shadow_and_hints(
            turn1,
            Some(&store),
            Some("prov"),
            Some("sess"),
            None,
        )
        .unwrap();
        let client_id = anthropic_response["content"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Turn 2 — client replays ONLY the tool_result. No assistant echo.
        let turn2_input = json!({
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": &client_id, "content": "sunny" }
                    ]
                }
            ]
        });
        let result =
            anthropic_to_gemini_with_shadow(turn2_input, Some(&store), Some("prov"), Some("sess"))
                .expect("scenario A must resolve tool name through shadow");
        assert_eq!(
            result["contents"][0]["parts"][0]["functionResponse"]["name"],
            "get_weather"
        );
    }

    /// Scenario B: the client replays the full history. The proxy picks
    /// the shadow-replay branch (not `convert_message_content_to_parts`),
    /// which strips the synthesized id from the outgoing `functionCall`.
    /// `tool_name_by_id` must still have been populated from the shadow
    /// so the following `tool_result(A)` resolves.
    #[test]
    fn non_stream_missing_id_scenario_b_full_history_replay_resolves() {
        let store = GeminiShadowStore::with_limits(8, 4);
        let turn1 = json!({
            "responseId": "r-full",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{
                        "functionCall": { "name": "get_weather", "args": { "city": "Tokyo" } }
                    }]
                }
            }]
        });
        let anthropic_response = gemini_to_anthropic_with_shadow_and_hints(
            turn1,
            Some(&store),
            Some("prov"),
            Some("sess"),
            None,
        )
        .unwrap();
        let client_id = anthropic_response["content"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Turn 2 — full history: assistant tool_use + tool_result.
        let turn2_input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "tool_use",
                            "id": &client_id,
                            "name": "get_weather",
                            "input": { "city": "Tokyo" }
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": &client_id, "content": "sunny" }
                    ]
                }
            ]
        });
        let result =
            anthropic_to_gemini_with_shadow(turn2_input, Some(&store), Some("prov"), Some("sess"))
                .expect("scenario B must resolve tool name through shadow replay");

        // Shadow-replay path: `functionCall.id` is stripped for the
        // assistant turn (the synthesized id must not leak upstream).
        assert!(
            result["contents"][0]["parts"][0]["functionCall"]
                .get("id")
                .is_none(),
            "synthesized id must not leak to Gemini in shadow replay"
        );
        assert_eq!(
            result["contents"][0]["parts"][0]["functionCall"]["name"],
            "get_weather"
        );
        // The tool_result round-trip resolves through the shadow map.
        assert_eq!(
            result["contents"][1]["parts"][0]["functionResponse"]["name"],
            "get_weather"
        );
    }

    /// Regression: when Gemini returns an id, nothing is synthesized.
    /// The original id is round-tripped in both the Anthropic response
    /// and the shadow store, and it flows back to Gemini on the next
    /// functionResponse.
    #[test]
    fn non_stream_preserves_original_gemini_id_when_present() {
        let store = GeminiShadowStore::with_limits(8, 4);
        let body = json!({
            "responseId": "r-preserve",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "parts": [{
                        "functionCall": {
                            "id": "call_real_1",
                            "name": "get_weather",
                            "args": { "city": "Tokyo" }
                        }
                    }]
                }
            }]
        });

        let response = gemini_to_anthropic_with_shadow_and_hints(
            body,
            Some(&store),
            Some("prov"),
            Some("sess"),
            None,
        )
        .unwrap();
        assert_eq!(response["content"][0]["id"], "call_real_1");
        let snapshot = store.get_session("prov", "sess").unwrap();
        assert_eq!(
            snapshot.turns[0].tool_calls[0].id.as_deref(),
            Some("call_real_1")
        );
        assert_eq!(
            snapshot.turns[0].assistant_content["parts"][0]["functionCall"]["id"],
            "call_real_1"
        );
    }

    /// Defensive: if a shadow turn somehow carries a synthesized
    /// `functionCall.id` (e.g. recorded by this path), replaying it via
    /// `anthropic_to_gemini_with_shadow` must strip the id before sending
    /// upstream, so Gemini never sees the internal identifier.
    #[test]
    fn non_stream_synthesized_id_not_leaked_to_gemini_via_shadow_replay() {
        let store = GeminiShadowStore::with_limits(8, 4);
        let synth = synthesize_tool_call_id();
        store.record_assistant_turn(
            "prov",
            "sess",
            json!({
                "parts": [{
                    "functionCall": {
                        "id": &synth,
                        "name": "get_weather",
                        "args": { "city": "Tokyo" }
                    }
                }]
            }),
            vec![GeminiToolCallMeta::new(
                Some(synth.clone()),
                "get_weather",
                json!({ "city": "Tokyo" }),
                None::<String>,
            )],
        );

        let input = json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "tool_use",
                            "id": &synth,
                            "name": "get_weather",
                            "input": { "city": "Tokyo" }
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": &synth, "content": "sunny" }
                    ]
                }
            ]
        });
        let result =
            anthropic_to_gemini_with_shadow(input, Some(&store), Some("prov"), Some("sess"))
                .unwrap();
        assert!(
            result["contents"][0]["parts"][0]["functionCall"]
                .get("id")
                .is_none(),
            "shadow replay must strip synthesized functionCall.id"
        );
        assert!(
            result["contents"][1]["parts"][0]["functionResponse"]
                .get("id")
                .is_none(),
            "functionResponse.id must also be omitted for synthesized ids"
        );
    }
}
