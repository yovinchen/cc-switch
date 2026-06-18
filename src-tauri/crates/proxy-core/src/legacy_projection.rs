use super::domain::{AppKind, InterfaceKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const CLAUDE_MODEL_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyProviderProjectionInput {
    #[serde(default)]
    pub api_format: Option<String>,
    #[serde(default)]
    pub codex_wire_api: Option<String>,
    #[serde(default)]
    pub codex_model: Option<String>,
    #[serde(default)]
    pub codex_catalog_models: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub claude_desktop_model_routes: Vec<LegacyModelRouteInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyModelRouteInput {
    pub public_model: String,
    pub upstream_model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyModelRouteProjection {
    pub public_model: String,
    pub upstream_model: String,
}

pub fn legacy_channel_priority(
    provider_id: &str,
    in_failover_queue: bool,
    current_provider_id: Option<&str>,
) -> i64 {
    if current_provider_id == Some(provider_id) {
        100
    } else if in_failover_queue {
        50
    } else {
        0
    }
}

pub fn infer_legacy_channel_interface(
    app: Option<&AppKind>,
    provider: &LegacyProviderProjectionInput,
) -> InterfaceKind {
    match app {
        Some(AppKind::Claude | AppKind::ClaudeDesktop) => provider
            .api_format
            .as_deref()
            .map(|format| match InterfaceKind::from_storage(format) {
                InterfaceKind::OpenAiChatCompletions => InterfaceKind::OpenAiChatCompletions,
                InterfaceKind::OpenAiResponses => InterfaceKind::OpenAiResponses,
                InterfaceKind::GeminiNative => InterfaceKind::GeminiNative,
                _ => InterfaceKind::AnthropicMessages,
            })
            .unwrap_or(InterfaceKind::AnthropicMessages),
        Some(AppKind::Codex) => provider
            .codex_wire_api
            .as_deref()
            .map(|wire_api| {
                if is_chat_wire_api(wire_api) {
                    InterfaceKind::OpenAiChatCompletions
                } else {
                    InterfaceKind::OpenAiResponses
                }
            })
            .unwrap_or(InterfaceKind::OpenAiResponses),
        Some(AppKind::Gemini) => InterfaceKind::GeminiNative,
        Some(AppKind::Custom(_)) | None => InterfaceKind::Custom("custom".to_string()),
    }
}

pub fn infer_legacy_model_routes(
    app: Option<&AppKind>,
    provider: &LegacyProviderProjectionInput,
) -> Vec<LegacyModelRouteProjection> {
    match app {
        Some(AppKind::Claude | AppKind::ClaudeDesktop) => infer_claude_models(provider),
        Some(AppKind::Codex) => infer_codex_models(provider),
        Some(AppKind::Gemini) => infer_env_models(provider, &["GEMINI_MODEL"]),
        Some(AppKind::Custom(_)) | None => Vec::new(),
    }
}

pub fn is_chat_wire_api(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "chat"
            | "chat_completions"
            | "chat-completions"
            | "openai_chat"
            | "openai-chat"
            | "openai_chat_completions"
    )
}

fn infer_claude_models(
    provider: &LegacyProviderProjectionInput,
) -> Vec<LegacyModelRouteProjection> {
    let mut routes = infer_env_models(provider, CLAUDE_MODEL_ENV_KEYS);

    for route in &provider.claude_desktop_model_routes {
        push_model_route(&mut routes, &route.public_model, &route.upstream_model);
    }

    routes
}

fn infer_codex_models(provider: &LegacyProviderProjectionInput) -> Vec<LegacyModelRouteProjection> {
    let mut routes = Vec::new();

    if let Some(model) = provider.codex_model.as_deref() {
        push_model_route(&mut routes, model, model);
    }

    for model in &provider.codex_catalog_models {
        push_model_route(&mut routes, model, model);
    }

    routes
}

fn infer_env_models(
    provider: &LegacyProviderProjectionInput,
    keys: &[&str],
) -> Vec<LegacyModelRouteProjection> {
    let mut routes = Vec::new();

    for key in keys {
        if let Some(model) = provider.env.get(*key) {
            push_model_route(&mut routes, model, model);
        }
    }

    routes
}

fn push_model_route(
    routes: &mut Vec<LegacyModelRouteProjection>,
    public_model: &str,
    upstream_model: &str,
) {
    let public_model = public_model.trim();
    let upstream_model = upstream_model.trim();
    if public_model.is_empty()
        || upstream_model.is_empty()
        || routes
            .iter()
            .any(|route| route.public_model == public_model)
    {
        return;
    }

    routes.push(LegacyModelRouteProjection {
        public_model: public_model.to_string(),
        upstream_model: upstream_model.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::{
        infer_legacy_channel_interface, infer_legacy_model_routes, is_chat_wire_api,
        legacy_channel_priority, push_model_route, LegacyModelRouteInput,
        LegacyModelRouteProjection, LegacyProviderProjectionInput,
    };
    use crate::{AppKind, InterfaceKind};

    #[test]
    fn infer_claude_interface_from_api_format_aliases() {
        for value in [
            "openai_chat",
            "openai-chat",
            "openai_chat_completions",
            "chat-completions",
        ] {
            let provider = LegacyProviderProjectionInput {
                api_format: Some(value.to_string()),
                ..Default::default()
            };

            assert_eq!(
                infer_legacy_channel_interface(Some(&AppKind::Claude), &provider),
                InterfaceKind::OpenAiChatCompletions
            );
        }

        let provider = LegacyProviderProjectionInput {
            api_format: Some("responses".to_string()),
            ..Default::default()
        };
        assert_eq!(
            infer_legacy_channel_interface(Some(&AppKind::ClaudeDesktop), &provider),
            InterfaceKind::OpenAiResponses
        );
    }

    #[test]
    fn infer_claude_interface_defaults_unknown_format_to_anthropic() {
        let provider = LegacyProviderProjectionInput {
            api_format: Some("experimental".to_string()),
            ..Default::default()
        };

        assert_eq!(
            infer_legacy_channel_interface(Some(&AppKind::Claude), &provider),
            InterfaceKind::AnthropicMessages
        );
    }

    #[test]
    fn infer_codex_interface_from_wire_api() {
        let chat_provider = LegacyProviderProjectionInput {
            codex_wire_api: Some("chat".to_string()),
            ..Default::default()
        };
        let responses_provider = LegacyProviderProjectionInput {
            codex_wire_api: Some("responses".to_string()),
            ..Default::default()
        };

        assert_eq!(
            infer_legacy_channel_interface(Some(&AppKind::Codex), &chat_provider),
            InterfaceKind::OpenAiChatCompletions
        );
        assert_eq!(
            infer_legacy_channel_interface(Some(&AppKind::Codex), &responses_provider),
            InterfaceKind::OpenAiResponses
        );
        assert_eq!(
            infer_legacy_channel_interface(Some(&AppKind::Codex), &Default::default()),
            InterfaceKind::OpenAiResponses
        );
    }

    #[test]
    fn infer_gemini_and_custom_interfaces() {
        assert_eq!(
            infer_legacy_channel_interface(Some(&AppKind::Gemini), &Default::default()),
            InterfaceKind::GeminiNative
        );
        assert_eq!(
            infer_legacy_channel_interface(
                Some(&AppKind::Custom("opencode".to_string())),
                &Default::default(),
            ),
            InterfaceKind::Custom("custom".to_string())
        );
        assert_eq!(
            infer_legacy_channel_interface(None, &Default::default()),
            InterfaceKind::Custom("custom".to_string())
        );
    }

    #[test]
    fn infer_claude_models_dedups_and_trims_env_and_named_routes() {
        let provider = LegacyProviderProjectionInput {
            env: [
                ("ANTHROPIC_MODEL".to_string(), " claude-sonnet-4 ".to_string()),
                (
                    "ANTHROPIC_SMALL_FAST_MODEL".to_string(),
                    "claude-haiku-4".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                    "claude-haiku-4".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
                    " ".to_string(),
                ),
            ]
            .into_iter()
            .collect(),
            claude_desktop_model_routes: vec![LegacyModelRouteInput {
                public_model: "sonnet-safe".to_string(),
                upstream_model: "claude-sonnet-4".to_string(),
            }],
            ..Default::default()
        };

        assert_eq!(
            infer_legacy_model_routes(Some(&AppKind::Claude), &provider),
            vec![
                LegacyModelRouteProjection {
                    public_model: "claude-sonnet-4".to_string(),
                    upstream_model: "claude-sonnet-4".to_string(),
                },
                LegacyModelRouteProjection {
                    public_model: "claude-haiku-4".to_string(),
                    upstream_model: "claude-haiku-4".to_string(),
                },
                LegacyModelRouteProjection {
                    public_model: "sonnet-safe".to_string(),
                    upstream_model: "claude-sonnet-4".to_string(),
                },
            ]
        );
    }

    #[test]
    fn infer_codex_models_prefers_config_model_then_catalog_without_duplicates() {
        let provider = LegacyProviderProjectionInput {
            codex_model: Some(" gpt-5.4 ".to_string()),
            codex_catalog_models: vec![
                "gpt-5.4".to_string(),
                "gpt-5.4-mini".to_string(),
                "".to_string(),
            ],
            ..Default::default()
        };

        assert_eq!(
            infer_legacy_model_routes(Some(&AppKind::Codex), &provider),
            vec![
                LegacyModelRouteProjection {
                    public_model: "gpt-5.4".to_string(),
                    upstream_model: "gpt-5.4".to_string(),
                },
                LegacyModelRouteProjection {
                    public_model: "gpt-5.4-mini".to_string(),
                    upstream_model: "gpt-5.4-mini".to_string(),
                },
            ]
        );
    }

    #[test]
    fn infer_gemini_models_from_env_and_unknown_app_has_no_models() {
        let provider = LegacyProviderProjectionInput {
            env: [("GEMINI_MODEL".to_string(), "gemini-2.5-pro".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };

        assert_eq!(
            infer_legacy_model_routes(Some(&AppKind::Gemini), &provider),
            vec![LegacyModelRouteProjection {
                public_model: "gemini-2.5-pro".to_string(),
                upstream_model: "gemini-2.5-pro".to_string(),
            }]
        );
        assert!(infer_legacy_model_routes(
            Some(&AppKind::Custom("hermes".to_string())),
            &provider
        )
        .is_empty());
        assert!(infer_legacy_model_routes(None, &provider).is_empty());
    }

    #[test]
    fn model_route_push_dedups_by_public_model() {
        let mut routes = Vec::new();

        push_model_route(&mut routes, " public ", "upstream-a");
        push_model_route(&mut routes, "public", "upstream-b");
        push_model_route(&mut routes, "", "upstream-c");
        push_model_route(&mut routes, "other", " ");

        assert_eq!(
            routes,
            vec![LegacyModelRouteProjection {
                public_model: "public".to_string(),
                upstream_model: "upstream-a".to_string(),
            }]
        );
    }

    #[test]
    fn priority_preserves_current_provider_and_failover_ordering() {
        assert_eq!(legacy_channel_priority("provider-a", false, Some("provider-a")), 100);
        assert_eq!(legacy_channel_priority("provider-a", true, Some("provider-b")), 50);
        assert_eq!(legacy_channel_priority("provider-a", false, Some("provider-b")), 0);
    }

    #[test]
    fn chat_wire_api_aliases_are_recognized() {
        assert!(is_chat_wire_api("chat"));
        assert!(is_chat_wire_api("openai-chat"));
        assert!(is_chat_wire_api("openai_chat_completions"));
        assert!(!is_chat_wire_api("responses"));
    }
}
