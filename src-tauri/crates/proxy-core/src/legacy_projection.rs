use super::channel_identity::stable_channel_id;
use super::channel_request::normalize_channel_base_url;
use super::domain::{AppKind, InterfaceKind, DEFAULT_ROUTE_GROUP};
use super::request_url::is_codex_chat_wire_api;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

pub const LEGACY_PRIMARY_SOURCE: &str = "legacy_primary";
pub const LEGACY_ENDPOINT_SOURCE: &str = "legacy_endpoint";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyChannelProjectionInput {
    pub app_type: String,
    #[serde(default)]
    pub app: Option<AppKind>,
    pub provider_id: String,
    pub provider_name: String,
    #[serde(default)]
    pub provider_sort_index: Option<usize>,
    pub provider_in_failover_queue: bool,
    pub base_url: String,
    pub interface_kind: InterfaceKind,
    pub priority: i64,
    pub source_kind: String,
    #[serde(default)]
    pub source_endpoint_url: Option<String>,
    pub provider_projection: LegacyProviderProjectionInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyChannelModelProjection {
    pub channel_id: String,
    pub public_model: String,
    pub upstream_model: String,
    #[serde(default)]
    pub capabilities: Value,
    #[serde(default)]
    pub pricing_model: Option<String>,
    #[serde(default)]
    pub request_overrides: Value,
    #[serde(default)]
    pub response_overrides: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyChannelProjection {
    pub id: String,
    pub provider_id: String,
    pub app_type: String,
    pub name: String,
    pub status: String,
    pub base_url: String,
    pub interface_kind: String,
    pub auth_profile_ref: Option<String>,
    pub groups: Vec<String>,
    pub priority: i64,
    pub weight: u32,
    pub retry_policy: Value,
    pub health_policy: Value,
    pub header_overrides: Value,
    pub param_overrides: Value,
    pub status_code_mapping: Value,
    pub tags: Vec<String>,
    pub metadata: Value,
    pub source_kind: String,
    pub source_endpoint_url: Option<String>,
    pub models: Vec<LegacyChannelModelProjection>,
    pub needs_review: bool,
    pub review_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyEndpointInput {
    pub url: String,
    pub added_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyProviderChannelMigrationInput {
    pub provider_id: String,
    pub provider_name: String,
    #[serde(default)]
    pub provider_sort_index: Option<usize>,
    pub provider_in_failover_queue: bool,
    #[serde(default)]
    pub primary_base_url: String,
    #[serde(default)]
    pub endpoints: Vec<LegacyEndpointInput>,
    pub provider_projection: LegacyProviderProjectionInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyChannelMigrationPlanInput {
    pub app_type: String,
    #[serde(default)]
    pub app: Option<AppKind>,
    #[serde(default)]
    pub current_provider_id: Option<String>,
    #[serde(default)]
    pub providers: Vec<LegacyProviderChannelMigrationInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyChannelMigrationPlan {
    pub app_type: String,
    pub channels: Vec<LegacyChannelProjection>,
    pub duplicate_count: usize,
    pub needs_review_count: usize,
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

pub fn legacy_provider_config_text_from_settings(settings_config: &Value) -> Option<&str> {
    settings_config.get("config").and_then(Value::as_str)
}

pub fn legacy_provider_env_from_settings(settings_config: &Value) -> BTreeMap<String, String> {
    settings_config
        .get("env")
        .and_then(Value::as_object)
        .map(|env| {
            env.iter()
                .filter_map(|(key, value)| {
                    value
                        .as_str()
                        .map(|model| (key.to_string(), model.to_string()))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default()
}

pub fn legacy_provider_codex_catalog_models_from_settings(
    settings_config: &Value,
) -> Vec<String> {
    settings_config
        .get("modelCatalog")
        .and_then(|catalog| catalog.get("models"))
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|entry| {
                    entry
                        .get("model")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn build_legacy_channel_migration_plan(
    input: LegacyChannelMigrationPlanInput,
) -> LegacyChannelMigrationPlan {
    let mut channels = Vec::new();
    let mut seen_routes = HashSet::new();
    let mut duplicate_count = 0usize;

    for provider in input.providers {
        let priority = legacy_channel_priority(
            &provider.provider_id,
            provider.provider_in_failover_queue,
            input.current_provider_id.as_deref(),
        );
        let interface_kind =
            infer_legacy_channel_interface(input.app.as_ref(), &provider.provider_projection);

        let primary = build_legacy_channel_projection(LegacyChannelProjectionInput {
            app_type: input.app_type.clone(),
            app: input.app.clone(),
            provider_id: provider.provider_id.clone(),
            provider_name: provider.provider_name.clone(),
            provider_sort_index: provider.provider_sort_index,
            provider_in_failover_queue: provider.provider_in_failover_queue,
            base_url: normalize_channel_base_url(&provider.primary_base_url),
            interface_kind: interface_kind.clone(),
            priority,
            source_kind: LEGACY_PRIMARY_SOURCE.to_string(),
            source_endpoint_url: None,
            provider_projection: provider.provider_projection.clone(),
        });
        push_channel_projection_or_count_duplicate(
            &mut channels,
            &mut seen_routes,
            &mut duplicate_count,
            primary,
        );

        let mut endpoints = provider.endpoints;
        endpoints.sort_by(|a, b| a.added_at.cmp(&b.added_at).then_with(|| a.url.cmp(&b.url)));
        for endpoint in endpoints {
            let channel = build_legacy_channel_projection(LegacyChannelProjectionInput {
                app_type: input.app_type.clone(),
                app: input.app.clone(),
                provider_id: provider.provider_id.clone(),
                provider_name: provider.provider_name.clone(),
                provider_sort_index: provider.provider_sort_index,
                provider_in_failover_queue: provider.provider_in_failover_queue,
                base_url: normalize_channel_base_url(&endpoint.url),
                interface_kind: interface_kind.clone(),
                priority,
                source_kind: LEGACY_ENDPOINT_SOURCE.to_string(),
                source_endpoint_url: Some(endpoint.url),
                provider_projection: provider.provider_projection.clone(),
            });
            push_channel_projection_or_count_duplicate(
                &mut channels,
                &mut seen_routes,
                &mut duplicate_count,
                channel,
            );
        }
    }

    let needs_review_count = channels
        .iter()
        .filter(|channel| channel.needs_review)
        .count();

    LegacyChannelMigrationPlan {
        app_type: input.app_type,
        channels,
        duplicate_count,
        needs_review_count,
    }
}

fn push_channel_projection_or_count_duplicate(
    channels: &mut Vec<LegacyChannelProjection>,
    seen_routes: &mut HashSet<(String, String, String)>,
    duplicate_count: &mut usize,
    channel: LegacyChannelProjection,
) {
    let route_key = (
        channel.provider_id.clone(),
        channel.interface_kind.clone(),
        channel.base_url.clone(),
    );
    if !seen_routes.insert(route_key) {
        *duplicate_count += 1;
        return;
    }

    channels.push(channel);
}

pub fn build_legacy_channel_projection(
    input: LegacyChannelProjectionInput,
) -> LegacyChannelProjection {
    let id = stable_channel_id(
        &input.app_type,
        &input.provider_id,
        &input.source_kind,
        &input.base_url,
    );
    let models = infer_legacy_model_routes(input.app.as_ref(), &input.provider_projection)
        .into_iter()
        .map(|route| LegacyChannelModelProjection {
            channel_id: id.clone(),
            public_model: route.public_model,
            upstream_model: route.upstream_model,
            capabilities: json!({}),
            pricing_model: None,
            request_overrides: json!({}),
            response_overrides: json!({}),
        })
        .collect::<Vec<_>>();

    let mut review_reasons = Vec::new();
    if input.base_url.is_empty() {
        review_reasons.push("missing_base_url".to_string());
    }
    if models.is_empty() {
        review_reasons.push("no_model_mapping_inferred".to_string());
    }

    let needs_review = !review_reasons.is_empty();
    let auth_profile_ref = Some(format!(
        "provider:{}:{}",
        input.app_type, input.provider_id
    ));
    let metadata = json!({
        "migration_source": input.source_kind.as_str(),
        "provider_name": input.provider_name.as_str(),
        "provider_sort_index": input.provider_sort_index,
        "provider_in_failover_queue": input.provider_in_failover_queue,
        "needs_review": needs_review,
        "review_reasons": review_reasons,
    });
    let name = match input.source_kind.as_str() {
        "legacy_primary" => format!("{} primary", input.provider_name),
        "legacy_endpoint" => format!("{} endpoint", input.provider_name),
        "manual" => input.provider_name.clone(),
        _ => input.provider_name.clone(),
    };

    LegacyChannelProjection {
        id,
        provider_id: input.provider_id,
        app_type: input.app_type.clone(),
        name,
        status: "enabled".to_string(),
        base_url: input.base_url,
        interface_kind: input.interface_kind.as_str().to_string(),
        auth_profile_ref,
        groups: vec![DEFAULT_ROUTE_GROUP.to_string()],
        priority: input.priority,
        weight: 100,
        retry_policy: json!({}),
        health_policy: json!({}),
        header_overrides: json!({}),
        param_overrides: json!({}),
        status_code_mapping: json!([]),
        tags: vec!["legacy".to_string()],
        metadata,
        source_kind: input.source_kind,
        source_endpoint_url: input.source_endpoint_url,
        models,
        needs_review,
        review_reasons,
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
                if is_codex_chat_wire_api(wire_api) {
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
        build_legacy_channel_migration_plan, build_legacy_channel_projection,
        infer_legacy_channel_interface, infer_legacy_model_routes, legacy_channel_priority,
        legacy_provider_codex_catalog_models_from_settings, legacy_provider_config_text_from_settings,
        legacy_provider_env_from_settings, push_model_route, LegacyChannelMigrationPlanInput,
        LegacyChannelProjectionInput, LegacyEndpointInput, LegacyModelRouteInput,
        LegacyModelRouteProjection, LegacyProviderChannelMigrationInput,
        LegacyProviderProjectionInput, LEGACY_ENDPOINT_SOURCE, LEGACY_PRIMARY_SOURCE,
    };
    use crate::domain::{AppKind, InterfaceKind};
    use serde_json::json;

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
    fn legacy_provider_settings_projection_extracts_json_shapes() {
        let settings = json!({
            "config": "model = \"gpt-5.4\"",
            "env": {
                "ANTHROPIC_MODEL": "claude-sonnet-4",
                "IGNORED_NON_STRING": 123
            },
            "modelCatalog": {
                "models": [
                    { "model": "gpt-5.4" },
                    { "model": "gpt-5.4-mini" },
                    { "id": "skip" },
                    { "model": 123 }
                ]
            }
        });

        assert_eq!(
            legacy_provider_config_text_from_settings(&settings),
            Some("model = \"gpt-5.4\"")
        );
        assert_eq!(
            legacy_provider_env_from_settings(&settings)
                .get("ANTHROPIC_MODEL")
                .map(String::as_str),
            Some("claude-sonnet-4")
        );
        assert!(
            !legacy_provider_env_from_settings(&settings)
                .contains_key("IGNORED_NON_STRING")
        );
        assert_eq!(
            legacy_provider_codex_catalog_models_from_settings(&settings),
            vec!["gpt-5.4".to_string(), "gpt-5.4-mini".to_string()]
        );

        let empty = json!({
            "env": [],
            "modelCatalog": { "models": {} }
        });
        assert_eq!(legacy_provider_config_text_from_settings(&empty), None);
        assert!(legacy_provider_env_from_settings(&empty).is_empty());
        assert!(legacy_provider_codex_catalog_models_from_settings(&empty).is_empty());
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
    fn build_legacy_channel_projection_populates_channel_defaults_and_metadata() {
        let projection = build_legacy_channel_projection(LegacyChannelProjectionInput {
            app_type: "claude".to_string(),
            app: Some(AppKind::Claude),
            provider_id: "anthropic-main".to_string(),
            provider_name: "Anthropic Main".to_string(),
            provider_sort_index: Some(7),
            provider_in_failover_queue: true,
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: InterfaceKind::OpenAiResponses,
            priority: 100,
            source_kind: "legacy_primary".to_string(),
            source_endpoint_url: None,
            provider_projection: LegacyProviderProjectionInput {
                env: [(
                    "ANTHROPIC_MODEL".to_string(),
                    "claude-sonnet-4".to_string(),
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
        });

        assert_eq!(projection.provider_id, "anthropic-main");
        assert_eq!(projection.name, "Anthropic Main primary");
        assert_eq!(projection.status, "enabled");
        assert_eq!(projection.interface_kind, "openai_responses");
        assert_eq!(
            projection.auth_profile_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(projection.groups, vec!["default".to_string()]);
        assert_eq!(projection.weight, 100);
        assert_eq!(projection.tags, vec!["legacy".to_string()]);
        assert!(!projection.needs_review);
        assert!(projection.review_reasons.is_empty());
        assert_eq!(projection.metadata["migration_source"], "legacy_primary");
        assert_eq!(projection.metadata["provider_name"], "Anthropic Main");
        assert_eq!(projection.metadata["provider_sort_index"], 7);
        assert_eq!(projection.metadata["provider_in_failover_queue"], true);
        assert_eq!(projection.models.len(), 1);
        assert_eq!(projection.models[0].channel_id, projection.id);
        assert_eq!(projection.models[0].public_model, "claude-sonnet-4");
    }

    #[test]
    fn build_legacy_channel_migration_plan_orders_and_dedupes_legacy_endpoints() {
        let plan = build_legacy_channel_migration_plan(LegacyChannelMigrationPlanInput {
            app_type: "claude".to_string(),
            app: Some(AppKind::Claude),
            current_provider_id: Some("anthropic-main".to_string()),
            providers: vec![LegacyProviderChannelMigrationInput {
                provider_id: "anthropic-main".to_string(),
                provider_name: "Anthropic Main".to_string(),
                provider_sort_index: Some(1),
                provider_in_failover_queue: false,
                primary_base_url: "https://relay-a.example.com/v1/".to_string(),
                endpoints: vec![
                    LegacyEndpointInput {
                        url: "https://relay-c.example.com/v1".to_string(),
                        added_at: 3,
                    },
                    LegacyEndpointInput {
                        url: "https://relay-a.example.com/v1".to_string(),
                        added_at: 1,
                    },
                    LegacyEndpointInput {
                        url: "https://relay-b.example.com/v1/".to_string(),
                        added_at: 2,
                    },
                ],
                provider_projection: LegacyProviderProjectionInput {
                    env: [(
                        "ANTHROPIC_MODEL".to_string(),
                        "claude-sonnet-4".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                    ..Default::default()
                },
            }],
        });

        assert_eq!(plan.app_type, "claude");
        assert_eq!(plan.duplicate_count, 1);
        assert_eq!(plan.needs_review_count, 0);
        assert_eq!(plan.channels.len(), 3);
        assert_eq!(plan.channels[0].source_kind, LEGACY_PRIMARY_SOURCE);
        assert_eq!(plan.channels[0].base_url, "https://relay-a.example.com/v1");
        assert_eq!(plan.channels[0].priority, 100);
        assert_eq!(plan.channels[1].source_kind, LEGACY_ENDPOINT_SOURCE);
        assert_eq!(plan.channels[1].base_url, "https://relay-b.example.com/v1");
        assert_eq!(plan.channels[2].base_url, "https://relay-c.example.com/v1");
        assert!(plan
            .channels
            .iter()
            .all(|channel| channel.interface_kind == "anthropic_messages"));
    }

    #[test]
    fn build_legacy_channel_projection_marks_missing_base_and_models_for_review() {
        let projection = build_legacy_channel_projection(LegacyChannelProjectionInput {
            app_type: "opencode".to_string(),
            app: Some(AppKind::Custom("opencode".to_string())),
            provider_id: "custom-provider".to_string(),
            provider_name: "Custom Provider".to_string(),
            provider_sort_index: None,
            provider_in_failover_queue: false,
            base_url: String::new(),
            interface_kind: InterfaceKind::Custom("custom".to_string()),
            priority: 0,
            source_kind: "legacy_endpoint".to_string(),
            source_endpoint_url: Some(" ".to_string()),
            provider_projection: Default::default(),
        });

        assert_eq!(projection.name, "Custom Provider endpoint");
        assert!(projection.needs_review);
        assert_eq!(
            projection.review_reasons,
            vec![
                "missing_base_url".to_string(),
                "no_model_mapping_inferred".to_string()
            ]
        );
        assert_eq!(projection.metadata["needs_review"], true);
        assert_eq!(
            projection.metadata["review_reasons"],
            serde_json::json!(["missing_base_url", "no_model_mapping_inferred"])
        );
    }
}
