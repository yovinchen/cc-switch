#[cfg(test)]
use crate::app_config::AppType;
#[cfg(test)]
use crate::error::AppError;
#[cfg(test)]
use crate::provider::Provider;
use crate::proxy::host::cc_switch::proxy_runtime::forward_current_provider_id_from_source;
#[cfg(test)]
use crate::proxy::provider::claude_provider_api_format;

#[cfg(test)]
mod tests {
    use crate::proxy::engine::routing::provider_router_app_error_from_provider_selection_failure;
    use crate::proxy::provider::{
        transform_claude_response_for_api_format, transform_claude_sse_for_api_format,
    };
    use crate::proxy_core::api::config::{
        app_type_from_circuit_key, channel_circuit_key, provider_circuit_key, AllowResult,
        CircuitBreakerStats, CircuitState,
    };
    use crate::proxy_core::api::domain::{
        extract_claude_base_url_from_settings, AppKind, ProviderKind,
    };
    use crate::proxy_core::api::ports::{
        claude_env_credentials_from_settings, codex_auth_object_value_from_settings,
        codex_provider_live_write_parts_from_settings, provider_settings_validation_issue_spec,
        provider_settings_validation_parts_from_settings, CodexProviderLiveWriteIssue,
        CodexProviderValidationIssue, ProviderSettingsValidationIssue,
    };
    use crate::proxy_core::api::transforms::{
        infer_codex_chat_reasoning_profile, is_copilot_prompt_cache_provider,
        normalize_codex_chat_reasoning_profile, resolve_claude_api_format_from_settings,
        resolve_claude_responses_prompt_cache_key,
        should_preserve_reasoning_content_for_openai_chat, GeminiShadowStore,
    };
    use crate::proxy_core::api::transport::{
        apply_codex_chat_upstream_model_policy, codex_provider_catalog_model_ids_from_settings,
        resolve_codex_provider_upstream_model,
    };
    use serde_json::{json, Value};

    use super::*;
    use crate::provider::ProviderMeta;
    use crate::proxy::host::cc_switch::provider_projection::{
        provider_claude_auth_key, provider_claude_base_url, provider_claude_kind,
        provider_needs_claude_transform,
    };
    use crate::proxy::provider::{
        codex_provider_apply_chat_upstream_model, codex_provider_chat_reasoning_options,
        codex_provider_chat_reasoning_profile, codex_provider_should_convert_responses_to_chat,
        codex_provider_upstream_model, codex_provider_uses_chat_completions,
    };
    use crate::proxy_core::api::auth::{
        extract_claude_auth_key_from_settings, ClaudeAuthKeySource,
    };
    use crate::proxy_core::api::management::{ChannelRouteSource, RouteResolveRequest};
    use crate::proxy_core::api::ports::codex_restored_live_settings_parts;
    use crate::proxy_core::api::routing::{
        apply_route_candidate_circuit_availability, current_provider_id_from_sources,
        resolve_channel_route, route_candidate_channel_circuit_keys, select_provider_ids,
        ProviderSelectionCandidate, ProviderSelectionFailure, ProviderSelectionInput,
        RouteResolveChannelInput, RouteResolveModelInput,
    };
    use crate::proxy_core::api::transforms::normalize_claude_anthropic_messages;
    use crate::proxy_core::api::transforms::{
        CodexChatReasoningOptions, CodexChatReasoningProfile,
    };
    use crate::proxy_core::api::transport::{
        build_claude_auth_headers, build_codex_bearer_auth_headers, build_copilot_auth_headers,
        ClaudeAuthHeaderKind, CopilotAuthHeadersInput,
    };
    use bytes::Bytes;
    use indexmap::IndexMap;
    use std::sync::Arc;

    #[test]
    fn circuit_and_route_adapter_projects_provider_router_contracts() {
        assert_eq!(
            crate::proxy_core::api::logging::cb::OPEN_TO_HALF_OPEN,
            "CB-001"
        );
        assert_eq!(
            crate::proxy_core::api::logging::cb::HALF_OPEN_TO_CLOSED,
            "CB-002"
        );
        assert_eq!(
            provider_circuit_key("claude", "provider-a"),
            "claude:provider-a"
        );
        assert_eq!(
            channel_circuit_key("claude", "channel-a"),
            "channel:claude:channel-a"
        );
        assert_eq!(
            app_type_from_circuit_key("channel:claude:channel-a"),
            "claude"
        );
        assert_eq!(CircuitState::HalfOpen.to_string(), "half_open");

        let allow_result = AllowResult {
            allowed: true,
            used_half_open_permit: true,
        };
        assert!(allow_result.allowed);
        assert!(allow_result.used_half_open_permit);

        let stats = CircuitBreakerStats {
            state: CircuitState::Open,
            consecutive_failures: 4,
            consecutive_successes: 0,
            total_requests: 10,
            failed_requests: 6,
        };
        assert_eq!(
            serde_json::to_value(stats).expect("serialize circuit stats"),
            json!({
                "state": "open",
                "consecutiveFailures": 4,
                "consecutiveSuccesses": 0,
                "totalRequests": 10,
                "failedRequests": 6
            })
        );

        let selected = select_provider_ids(ProviderSelectionInput::failover(vec![
            ProviderSelectionCandidate::new("missing", false, true),
            ProviderSelectionCandidate::new("provider-b", true, true),
            ProviderSelectionCandidate::new("provider-a", true, false),
        ]))
        .expect("selected provider ids");
        assert_eq!(selected, vec!["provider-b"]);
        let mut failover_providers = IndexMap::new();
        failover_providers.insert(
            "provider-a".to_string(),
            Provider::with_id(
                "provider-a".to_string(),
                "Provider A".to_string(),
                json!({}),
                None,
            ),
        );
        failover_providers.insert(
            "provider-b".to_string(),
            Provider::with_id(
                "provider-b".to_string(),
                "Provider B".to_string(),
                json!({}),
                None,
            ),
        );
        let failover_lookups = crate::proxy_core::api::routing::provider_failover_circuit_lookups(
            "claude",
            vec![
                "missing".to_string(),
                "provider-b".to_string(),
                "provider-a".to_string(),
            ],
            failover_providers.keys().cloned().collect::<Vec<_>>(),
        );
        assert_eq!(failover_lookups[0].provider_id, "missing");
        assert!(!failover_lookups[0].configured);
        assert_eq!(failover_lookups[1].provider_id, "provider-b");
        assert!(failover_lookups[1].configured);
        assert_eq!(
            failover_lookups[1].circuit_key.as_deref(),
            Some("claude:provider-b")
        );
        assert_eq!(
            crate::proxy_core::api::management::channel_route_source_for_materialized_count(1),
            ChannelRouteSource::MaterializedChannels
        );
        assert_eq!(
            crate::proxy_core::api::management::channel_route_source_for_materialized_count(0),
            ChannelRouteSource::LegacyProjection
        );
        assert!(matches!(
            provider_router_app_error_from_provider_selection_failure(
                "claude",
                ProviderSelectionFailure::AllProvidersCircuitOpen,
            ),
            AppError::AllProvidersCircuitOpen
        ));
        assert!(matches!(
            provider_router_app_error_from_provider_selection_failure(
                "claude",
                ProviderSelectionFailure::NoProvidersConfigured,
            ),
            AppError::NoProvidersConfigured
        ));
        assert_eq!(
            current_provider_id_from_sources(Some("settings-provider"), Some("db-provider")),
            "settings-provider"
        );
        assert_eq!(
            crate::proxy_core::api::routing::current_provider_id_option_from_sources(
                Some("settings-provider"),
                Some("db-provider"),
            ),
            Some("settings-provider".to_string())
        );
        assert!(
            !crate::proxy_core::api::routing::current_provider_db_fallback_required(Some(
                "settings-provider"
            ))
        );
        assert!(!crate::proxy_core::api::routing::current_provider_db_fallback_required(Some("")));
        assert!(crate::proxy_core::api::routing::current_provider_db_fallback_required(None));
        assert_eq!(
            crate::proxy_core::api::routing::current_provider_id_option_from_sources(None, None),
            None
        );
        let mut db_lookup_called = false;
        assert_eq!(
            forward_current_provider_id_from_source(Some("settings-provider"), || {
                db_lookup_called = true;
                Some("db-provider".to_string())
            }),
            "settings-provider"
        );
        assert!(!db_lookup_called);
        assert_eq!(
            forward_current_provider_id_from_source(None, || Some("db-provider".to_string())),
            "db-provider"
        );
        assert_eq!(forward_current_provider_id_from_source(None, || None), "");
        let selected_current = select_provider_ids(ProviderSelectionInput::current(Some(
            "provider-a".to_string(),
        )))
        .map_err(|error| provider_router_app_error_from_provider_selection_failure("claude", error))
        .expect("selected current provider id");
        assert_eq!(selected_current, vec!["provider-a"]);
        assert!(matches!(
            select_provider_ids(ProviderSelectionInput::current(None)).map_err(|error| {
                provider_router_app_error_from_provider_selection_failure("claude", error)
            }),
            Err(AppError::NoProvidersConfigured)
        ));

        let mut response = resolve_channel_route(
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
            vec![RouteResolveChannelInput {
                channel_id: "channel-a".to_string(),
                provider_id: "provider-a".to_string(),
                channel_name: "Provider A".to_string(),
                status: "enabled".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                groups: vec!["default".to_string()],
                models: vec![RouteResolveModelInput {
                    public_model: "claude-sonnet-4".to_string(),
                    upstream_model: "upstream-sonnet".to_string(),
                }],
                priority: 100,
                weight: 1,
                health_policy: json!({}),
                source_kind: "legacy_provider".to_string(),
            }],
            ChannelRouteSource::MaterializedChannels,
        )
        .expect("route response");
        assert_eq!(response.candidates.len(), 1);

        let circuit_lookups = route_candidate_channel_circuit_keys(&response);
        assert_eq!(circuit_lookups.len(), 1);
        assert_eq!(circuit_lookups[0].channel_id, "channel-a");
        assert_eq!(circuit_lookups[0].circuit_key, "channel:claude:channel-a");
        apply_route_candidate_circuit_availability(
            &mut response,
            [(circuit_lookups[0].clone(), false)],
        );
        assert!(response.candidates.is_empty());
        assert_eq!(response.rejected.len(), 1);
        assert_eq!(response.rejected[0].reasons, vec!["circuit_open"]);
    }

    #[test]
    fn codex_provider_adapter_projects_chat_policy_headers_and_reasoning() {
        assert!(
            crate::proxy_core::api::transport::resolve_codex_provider_uses_chat_completions(
                Some("openai_chat"),
                None,
                None,
                None
            )
        );
        assert!(
            crate::proxy_core::api::transport::should_convert_codex_responses_endpoint_to_chat(
                true,
                "/v1/responses"
            )
        );
        assert_eq!(
            crate::proxy_core::api::transport::build_codex_upstream_url(
                "https://api.openai.com",
                "/chat/completions",
            ),
            "https://api.openai.com/v1/chat/completions"
        );

        let headers = build_codex_bearer_auth_headers("sk-test").expect("bearer header");
        assert_eq!(headers[0].0.as_str(), "authorization");
        assert_eq!(
            headers[0].1,
            http::HeaderValue::from_static("Bearer sk-test")
        );
        assert_eq!(
            crate::proxy_core::api::ports::codex_config_text_from_settings(&json!({
                "config": "model = \"gpt-5\""
            })),
            Some("model = \"gpt-5\"")
        );
        assert_eq!(
            crate::proxy_core::api::ports::codex_config_text_from_settings(&json!({"config": 42})),
            None
        );
        assert_eq!(
            crate::proxy_core::api::ports::codex_config_text_from_settings(&json!({})),
            None
        );
        let codex_auth_settings = json!({"auth": {"OPENAI_API_KEY": "sk-auth"}});
        let auth =
            codex_auth_object_value_from_settings(&codex_auth_settings).expect("codex auth object");
        assert_eq!(
            crate::codex_config::extract_codex_api_key(Some(auth), Some("")).as_deref(),
            Some("sk-auth")
        );
        assert!(codex_auth_object_value_from_settings(&json!({"auth": "sk-auth"})).is_none());
        let restored_settings = json!({
            "auth": {"OPENAI_API_KEY": "sk-restored"},
            "config": "model = \"gpt-5\""
        });
        let restored_parts = codex_restored_live_settings_parts(&restored_settings);
        let expected_auth = json!({"OPENAI_API_KEY": "sk-restored"});
        let expected_config = json!("model = \"gpt-5\"");
        assert_eq!(restored_parts.auth, Some(&expected_auth));
        assert_eq!(restored_parts.config, Some(&expected_config));
        let official_live_provider = Provider::with_id(
            "codex-live-official".to_string(),
            "Codex Live Official".to_string(),
            json!({
                "auth": {
                    "tokens": {"id_token": "id-token"},
                    "auth_mode": "chatgpt"
                },
                "config": ""
            }),
            None,
        );
        let api_key_live_provider = Provider::with_id(
            "codex-live-api-key".to_string(),
            "Codex Live API Key".to_string(),
            json!({
                "auth": {"OPENAI_API_KEY": "sk-test"},
                "config": ""
            }),
            None,
        );
        let mut custom_category_provider = api_key_live_provider.clone();
        custom_category_provider.category = Some("custom".to_string());
        let write_settings = json!({
            "auth": {"OPENAI_API_KEY": "sk-write"},
            "config": "model = \"gpt-5\""
        });
        let write_parts = codex_provider_live_write_parts_from_settings(
            &write_settings,
            custom_category_provider.category.as_deref(),
        )
        .expect("codex provider live write parts");
        assert_eq!(write_parts.category, Some("custom"));
        assert_eq!(
            write_parts
                .auth
                .get("OPENAI_API_KEY")
                .and_then(Value::as_str),
            Some("sk-write")
        );
        assert_eq!(write_parts.config_text, Some("model = \"gpt-5\""));
        assert!(matches!(
            codex_provider_live_write_parts_from_settings(
                &json!({"config": "model = \"gpt-5\""}),
                custom_category_provider.category.as_deref(),
            ),
            Err(CodexProviderLiveWriteIssue::MissingAuth)
        ));
        let invalid_shape = Provider::with_id(
            "codex-live-invalid".to_string(),
            "Codex Live Invalid".to_string(),
            json!("not-object"),
            None,
        );
        let missing_auth = Provider::with_id(
            "codex-live-missing-auth".to_string(),
            "Codex Live Missing Auth".to_string(),
            json!({"config": ""}),
            None,
        );
        let mut auth_not_object = Provider::with_id(
            "codex-live-auth-string".to_string(),
            "Codex Live Auth String".to_string(),
            json!({"auth": "sk-test"}),
            None,
        );
        auth_not_object.category = Some("custom".to_string());
        let provider_validation_parts = provider_settings_validation_parts_from_settings(
            &AppKind::from(&AppType::Codex),
            &official_live_provider.settings_config,
        )
        .expect("provider validation parts");
        assert_eq!(provider_validation_parts.codex_config_text, Some(""));
        assert!(matches!(
            provider_settings_validation_parts_from_settings(
                &AppKind::from(&AppType::Codex),
                &invalid_shape.settings_config,
            ),
            Err(ProviderSettingsValidationIssue::Codex(
                CodexProviderValidationIssue::NotObject
            ))
        ));
        assert!(matches!(
            provider_settings_validation_parts_from_settings(
                &AppKind::from(&AppType::Claude),
                &invalid_shape.settings_config,
            ),
            Err(ProviderSettingsValidationIssue::ClaudeSettingsNotObject)
        ));
        assert!(matches!(
            provider_settings_validation_parts_from_settings(
                &AppKind::from(&AppType::OpenCode),
                &invalid_shape.settings_config,
            ),
            Err(ProviderSettingsValidationIssue::OpenCodeSettingsNotObject)
        ));
        assert!(matches!(
            provider_settings_validation_parts_from_settings(
                &AppKind::from(&AppType::Codex),
                &missing_auth.settings_config,
            ),
            Err(ProviderSettingsValidationIssue::Codex(
                CodexProviderValidationIssue::MissingAuth
            ))
        ));
        assert!(matches!(
            provider_settings_validation_parts_from_settings(
                &AppKind::from(&AppType::Codex),
                &auth_not_object.settings_config,
            ),
            Err(ProviderSettingsValidationIssue::Codex(
                CodexProviderValidationIssue::AuthNotObject
            ))
        ));
        let invalid_config = Provider::with_id(
            "codex-live-invalid-config".to_string(),
            "Codex Live Invalid Config".to_string(),
            json!({"auth": {}, "config": 42}),
            None,
        );
        assert!(matches!(
            provider_settings_validation_parts_from_settings(
                &AppKind::from(&AppType::Codex),
                &invalid_config.settings_config,
            ),
            Err(ProviderSettingsValidationIssue::Codex(
                CodexProviderValidationIssue::ConfigInvalidType
            ))
        ));
        let validation_spec = provider_settings_validation_issue_spec(
            ProviderSettingsValidationIssue::Codex(CodexProviderValidationIssue::MissingAuth),
            "codex-live-missing-auth",
        );
        assert_eq!(validation_spec.key, "provider.codex.auth.missing");
        assert_eq!(
            validation_spec.zh,
            "供应商 codex-live-missing-auth 缺少 auth 配置"
        );
        assert_eq!(
            validation_spec.en,
            "Provider codex-live-missing-auth is missing auth configuration"
        );
        assert_eq!(
            provider_settings_validation_issue_spec(
                ProviderSettingsValidationIssue::OpenClawSettingsNotObject,
                "openclaw-invalid",
            )
            .key,
            "provider.openclaw.settings.not_object"
        );
        let chat_provider = Provider::with_id(
            "codex-chat".to_string(),
            "Codex Chat".to_string(),
            json!({
                "config": r#"model_provider = "openai"
model = " upstream-model "

[model_providers.openai]
wire_api = "chat"
base_url = "https://api.openai.com/v1"
"#,
                "modelCatalog": {
                    "models": [{"model": "catalog-model"}]
                }
            }),
            None,
        );
        assert!(codex_provider_uses_chat_completions(&chat_provider));
        assert!(codex_provider_should_convert_responses_to_chat(
            &chat_provider,
            "/responses"
        ));
        assert!(!codex_provider_should_convert_responses_to_chat(
            &chat_provider,
            "/chat/completions"
        ));
        assert_eq!(
            codex_provider_upstream_model(&chat_provider).as_deref(),
            Some("upstream-model")
        );
        assert!(
            codex_provider_catalog_model_ids_from_settings(&chat_provider.settings_config)
                .contains("catalog-model")
        );
        let reasoning_provider = Provider::with_id(
            "codex-reasoning".to_string(),
            "DeepSeek Relay".to_string(),
            json!({
                "config": r#"model_provider = "deepseek"
model = "deepseek-v4-pro"

[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat"
"#
            }),
            None,
        );
        let inferred_profile =
            codex_provider_chat_reasoning_profile(&reasoning_provider, Some("deepseek-v4-pro"))
                .expect("deepseek reasoning profile");
        assert_eq!(inferred_profile.supports_effort, Some(true));
        assert_eq!(
            inferred_profile.effort_value_mode.as_deref(),
            Some("deepseek")
        );
        let mut explicit_reasoning_provider = Provider::with_id(
            "codex-explicit-reasoning".to_string(),
            "Explicit Reasoning".to_string(),
            json!({}),
            None,
        );
        explicit_reasoning_provider.meta = Some(ProviderMeta {
            codex_chat_reasoning: Some(crate::provider::CodexChatReasoningConfig {
                supports_thinking: Some(false),
                supports_effort: Some(false),
                thinking_param: Some("none".to_string()),
                effort_param: Some("none".to_string()),
                effort_value_mode: None,
                output_format: Some("auto".to_string()),
            }),
            ..Default::default()
        });
        let explicit_profile = codex_provider_chat_reasoning_profile(
            &explicit_reasoning_provider,
            Some("deepseek-v4-pro"),
        )
        .expect("explicit reasoning profile");
        assert_eq!(explicit_profile.supports_thinking, Some(false));
        assert_eq!(explicit_profile.effort_param.as_deref(), Some("none"));

        assert_eq!(
            resolve_codex_provider_upstream_model(Some(" upstream-model "), None).as_deref(),
            Some("upstream-model")
        );
        let catalog_model_ids = codex_provider_catalog_model_ids_from_settings(&json!({
            "modelCatalog": {
                "models": [{"model": "catalog-model"}]
            }
        }));
        assert!(catalog_model_ids.contains("catalog-model"));
        let mut body = json!({"model": "client-model"});
        assert_eq!(
            apply_codex_chat_upstream_model_policy(
                &mut body,
                true,
                Some("upstream-model"),
                &catalog_model_ids,
            )
            .as_deref(),
            Some("upstream-model")
        );
        assert_eq!(body["model"], "upstream-model");
        let mut forwarder_body = json!({"model": "client-model"});
        assert_eq!(
            codex_provider_apply_chat_upstream_model(&chat_provider, &mut forwarder_body)
                .as_deref(),
            Some("upstream-model")
        );
        assert_eq!(forwarder_body["model"], "upstream-model");
        let reasoning_options =
            codex_provider_chat_reasoning_options(&reasoning_provider, &forwarder_body)
                .expect("deepseek reasoning options");
        assert_eq!(reasoning_options.supports_effort, Some(true));
        assert_eq!(
            reasoning_options.effort_value_mode.as_deref(),
            Some("deepseek")
        );

        let profile = normalize_codex_chat_reasoning_profile(CodexChatReasoningProfile {
            supports_effort: Some(true),
            effort_param: Some("reasoning_effort".to_string()),
            ..CodexChatReasoningProfile::default()
        });
        assert_eq!(profile.supports_thinking, Some(true));
        let options = CodexChatReasoningOptions::from_profile(&profile);
        assert_eq!(options.supports_effort, Some(true));
        assert_eq!(
            infer_codex_chat_reasoning_profile(
                "DeepSeek Relay",
                "https://api.deepseek.com",
                "deepseek-chat",
            )
            .expect("deepseek reasoning profile")
            .supports_effort,
            Some(true)
        );
    }

    #[test]
    fn claude_provider_adapter_projects_config_auth_url_and_cache_helpers() {
        let settings = json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": " claude-token ",
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com/v1/"
            }
        });
        assert_eq!(
            resolve_claude_api_format_from_settings(None, Some("openai_chat"), &settings),
            "openai_chat"
        );
        let auth_key =
            extract_claude_auth_key_from_settings(&settings).expect("anthropic auth token");
        assert_eq!(auth_key.key, "claude-token");
        assert_eq!(auth_key.source, ClaudeAuthKeySource::AnthropicAuthToken);
        assert_eq!(
            extract_claude_base_url_from_settings(false, &settings).as_deref(),
            Some("https://api.anthropic.com/v1")
        );
        let env_credentials =
            claude_env_credentials_from_settings(&settings).expect("claude env credentials");
        assert_eq!(env_credentials.api_key, Some(" claude-token "));
        assert_eq!(
            env_credentials.base_url,
            Some("https://api.anthropic.com/v1/")
        );
        assert!(claude_env_credentials_from_settings(&json!({"env": "invalid"})).is_none());
        let mut provider = Provider::with_id(
            "claude".to_string(),
            "Claude".to_string(),
            settings.clone(),
            None,
        );
        provider.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        assert_eq!(claude_provider_api_format(&provider), "openai_chat");
        assert!(provider_needs_claude_transform(&provider));
        let no_transform_provider = Provider::with_id(
            "claude-no-transform".to_string(),
            "Claude No Transform".to_string(),
            json!({"env": {"ANTHROPIC_BASE_URL": "https://api.anthropic.com/v1/"}}),
            None,
        );
        assert!(!provider_needs_claude_transform(&no_transform_provider));
        let provider_auth_key = provider_claude_auth_key(&provider).expect("provider auth token");
        assert_eq!(provider_auth_key.key, "claude-token");
        assert_eq!(
            provider_claude_base_url(&provider).as_deref(),
            Some("https://api.anthropic.com/v1")
        );
        let mut gemini_cli_provider = Provider::with_id(
            "claude-gemini-cli".to_string(),
            "Claude Gemini CLI".to_string(),
            json!({"env": {
                "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                "ANTHROPIC_API_KEY": "{\"access_token\":\"ya29.valid\",\"refresh_token\":\"rt\"}"
            }}),
            None,
        );
        gemini_cli_provider.meta = Some(ProviderMeta {
            api_format: Some("gemini_native".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_kind(&gemini_cli_provider),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            crate::proxy_core::api::transport::build_claude_upstream_url(
                "https://api.anthropic.com/v1",
                "/v1/messages",
            ),
            "https://api.anthropic.com/v1/messages"
        );

        let bearer_headers =
            build_claude_auth_headers(ClaudeAuthHeaderKind::Bearer, "claude-token", None)
                .expect("bearer headers");
        assert_eq!(bearer_headers[0].0.as_str(), "authorization");
        assert_eq!(
            bearer_headers[0].1,
            http::HeaderValue::from_static("Bearer claude-token")
        );
        let copilot_headers = build_copilot_auth_headers(CopilotAuthHeadersInput {
            api_key: "copilot-token",
            request_id: "request-1",
            editor_version: "vscode/1",
            editor_plugin_version: "plugin/1",
            integration_id: "integration-1",
            user_agent: "copilot-test",
            github_api_version: "2022-11-28",
        })
        .expect("copilot headers");
        assert!(copilot_headers
            .iter()
            .any(|(name, value)| name.as_str() == "x-request-id" && value == "request-1"));

        assert!(is_copilot_prompt_cache_provider(
            Some("github_copilot"),
            &json!({})
        ));
        let cache_key = resolve_claude_responses_prompt_cache_key(
            &json!({"metadata": {"session_id": "session-1"}}),
            None,
            Some("fallback-session"),
            true,
        );
        assert_eq!(cache_key.key.as_deref(), Some("session-1"));
        assert_eq!(cache_key.source.as_str(), "session");
    }

    #[test]
    fn claude_provider_projects_response_facades() {
        let chat_response =
            crate::proxy_core::api::transforms::openai_chat_to_anthropic_message(&json!({
            "id": "chatcmpl_1",
            "model": "chat-model",
            "choices": [{
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
            }))
            .expect("chat response");
        assert_eq!(chat_response["content"][0]["text"], "Hi");
        let delegated_chat_response = transform_claude_response_for_api_format(
            &json!({
            "id": "chatcmpl_1",
            "model": "chat-model",
            "choices": [{
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
            }),
            "openai_chat",
            None,
            None,
            None,
            None,
        )
        .expect("delegated chat response");
        assert_eq!(delegated_chat_response["content"][0]["text"], "Hi");

        let responses_response =
            crate::proxy_core::api::transforms::openai_responses_to_anthropic_message(&json!({
            "id": "resp_1",
            "model": "responses-model",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "Done"}]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 2}
            }))
            .expect("responses response");
        assert_eq!(responses_response["content"][0]["text"], "Done");
        let delegated_responses_response = transform_claude_response_for_api_format(
            &json!({
            "id": "resp_1",
            "model": "responses-model",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "Done"}]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 2}
            }),
            "openai_responses",
            None,
            None,
            None,
            None,
        )
        .expect("delegated responses response");
        assert_eq!(delegated_responses_response["content"][0]["text"], "Done");

        let explicit_chat_response = transform_claude_response_for_api_format(
            &json!({
                "id": "chatcmpl_2",
                "model": "chat-model",
                "choices": [{
                    "message": {"role": "assistant", "content": "Explicit chat"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 2}
            }),
            "openai_chat",
            None,
            None,
            None,
            None,
        )
        .expect("explicit chat response");
        assert_eq!(
            explicit_chat_response["content"][0]["text"],
            "Explicit chat"
        );

        let explicit_responses_response = transform_claude_response_for_api_format(
            &json!({
                "id": "resp_2",
                "model": "responses-model",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{"type": "output_text", "text": "Explicit responses"}]
                }],
                "usage": {"input_tokens": 1, "output_tokens": 2}
            }),
            "openai_responses",
            None,
            None,
            None,
            None,
        )
        .expect("explicit responses response");
        assert_eq!(
            explicit_responses_response["content"][0]["text"],
            "Explicit responses"
        );

        let gemini_output =
            crate::proxy_core::api::transforms::gemini_response_to_anthropic_message(
                &json!({
                    "responseId": "gemini_1",
                    "candidates": [{
                        "content": {
                            "role": "model",
                            "parts": [{"text": "Gemini hi"}]
                        },
                        "finishReason": "STOP"
                    }],
                    "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 2}
                }),
                None,
                || "toolu_test".to_string(),
            )
            .expect("gemini response");
        assert_eq!(gemini_output.response["content"][0]["text"], "Gemini hi");
        let delegated_gemini_response = transform_claude_response_for_api_format(
            &json!({
            "responseId": "gemini_1",
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Gemini hi"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 1, "candidatesTokenCount": 2}
            }),
            "gemini_native",
            None,
            None,
            None,
            None,
        )
        .expect("delegated gemini response");
        assert_eq!(delegated_gemini_response["content"][0]["text"], "Gemini hi");

        assert!(should_preserve_reasoning_content_for_openai_chat(
            &json!({}),
            &json!({"model": "deepseek-v4-pro"})
        ));
        let reasoning_provider = Provider::with_id(
            "reasoning".to_string(),
            "Reasoning".to_string(),
            json!({}),
            None,
        );
        assert!(should_preserve_reasoning_content_for_openai_chat(
            &reasoning_provider.settings_config,
            &json!({"model": "deepseek-v4-pro"})
        ));

        let mut normalize_provider = Provider::with_id(
            "claude-normalize".to_string(),
            "Claude Normalize".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic"
                }
            }),
            None,
        );
        normalize_provider.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });
        let mut normalize_body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "messages": [{ "role": "user", "content": "hello" }]
        });
        assert!(normalize_claude_anthropic_messages(
            &mut normalize_body,
            &normalize_provider.settings_config,
            "anthropic"
        ));
        assert!(normalize_body.get("output_config").is_none());
        let mut non_anthropic_body = normalize_body.clone();
        assert!(!normalize_claude_anthropic_messages(
            &mut non_anthropic_body,
            &normalize_provider.settings_config,
            "openai_chat"
        ));
    }

    #[tokio::test]
    async fn claude_stream_transform_provider_dispatches_api_formats() {
        use futures::StreamExt as _;

        let chat_stream = futures::stream::iter(vec![
            Ok::<_, std::io::Error>(Bytes::from_static(
                b"data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
            )),
            Ok(Bytes::from_static(
                b"data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
            )),
            Ok(Bytes::from_static(b"data: [DONE]\n\n")),
        ]);
        let chat_output =
            transform_claude_sse_for_api_format(chat_stream, "openai_chat", None, None, None, None)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .map(|item| {
                    String::from_utf8(item.expect("chat chunk").to_vec()).expect("chat utf8")
                })
                .collect::<String>();
        assert!(chat_output.contains("event: message_start"));
        assert!(chat_output.contains("Hi"));

        let responses_stream = futures::stream::iter(vec![Ok::<_, std::io::Error>(
            Bytes::from_static(
                b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-4o\",\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\nevent: response.content_part.added\ndata: {\"type\":\"response.content_part.added\",\"part\":{\"type\":\"output_text\",\"text\":\"\"},\"output_index\":0,\"content_index\":0}\n\nevent: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Done\",\"output_index\":0,\"content_index\":0}\n\nevent: response.content_part.done\ndata: {\"type\":\"response.content_part.done\",\"output_index\":0,\"content_index\":0}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
            ),
        )]);
        let responses_output = transform_claude_sse_for_api_format(
            responses_stream,
            "openai_responses",
            None,
            None,
            None,
            None,
        )
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|item| {
            String::from_utf8(item.expect("responses chunk").to_vec()).expect("responses utf8")
        })
        .collect::<String>();
        assert!(responses_output.contains("event: message_start"));
        assert!(responses_output.contains("Done"));

        let gemini_stream = futures::stream::iter(vec![Ok::<_, std::io::Error>(
            Bytes::from_static(
                b"data: {\"responseId\":\"gemini_1\",\"modelVersion\":\"gemini-2.5-pro\",\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"parts\":[{\"text\":\"Gemini hi\"}]}}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n",
            ),
        )]);
        let gemini_output = transform_claude_sse_for_api_format(
            gemini_stream,
            "gemini_native",
            Some(Arc::new(GeminiShadowStore::default())),
            Some("provider-a".to_string()),
            Some("session-a".to_string()),
            None,
        )
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|item| String::from_utf8(item.expect("gemini chunk").to_vec()).expect("gemini utf8"))
        .collect::<String>();
        assert!(gemini_output.contains("event: message_start"));
        assert!(gemini_output.contains("Gemini hi"));
    }
}
