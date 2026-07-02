#[cfg(test)]
use crate::app_config::AppType;
#[cfg(test)]
use crate::error::AppError;
#[cfg(test)]
use crate::provider::Provider;
#[cfg(test)]
use crate::proxy::engine::forward_pipeline::{
    ForwarderAnthropicRectifierGateInput, ForwarderAppMediaPreventionInput,
    ForwarderAttemptBodyInput, ForwarderAttemptFailedInput, ForwarderAttemptStartedInput,
    ForwarderAuthHeadersInput, ForwarderClaudeBodyPolicyInput,
    ForwarderCodexChatProtocolEnrichmentInput, ForwarderCodexResponsesToChatInput,
    ForwarderCopilotRequestOptimizationGateInput, ForwarderCopilotRequestOptimizationInput,
    ForwarderCurrentProviderInput, ForwarderFailoverSwitchTarget, ForwarderForwardErrorStatusInput,
    ForwarderMediaRetryPlanInput, ForwarderPreparedRequest, ForwarderProtocolStateSource,
    ForwarderProviderFailureInput, ForwarderProviderRectifierRetryFailureInput,
    ForwarderProviderRequestBodyInput, ForwarderProviderTransformInput,
    ForwarderRectifierRetryFailureDecision, ForwarderRequestBodyTransformInput,
    ForwarderRequestPartsInput, ForwarderRequestPreparationInput, ForwarderRequestStartedInput,
    ForwarderRuntimeOptions, ForwarderSuccessStatusInput, ForwarderSuccessfulAttemptInput,
    ForwarderThinkingBudgetRectifierInput, ForwarderThinkingSignatureRectifierInput,
    ForwarderTransformPlanInput, ForwarderUpstreamUrlInput,
};
#[cfg(test)]
use crate::proxy::host::cc_switch::channel_auth_profile_attempts::{
    apply_channel_auth_profile_providers_from_source, forward_attempts_from_plan,
    required_forward_attempts_from_plan,
};
#[cfg(test)]
use crate::proxy::host::cc_switch::forwarder_runtime_state_source::CcSwitchForwarderRuntimeStateSource;
#[cfg(test)]
use crate::proxy::host::cc_switch::proxy_runtime::{
    app_type_from_proxy_core_app, app_type_option_from_proxy_core_app, extract_proxy_session_id,
    forward_current_provider_id_from_source, forward_runtime_request_from_proxy_request,
    forwarder_runtime_config_from_sources, forwarder_runtime_options_from_app_proxy_config,
    response_runtime_policy_from_app_proxy_config,
};
#[cfg(test)]
use crate::proxy::provider::claude_provider_api_format;
#[cfg(test)]
use crate::proxy::route_attempt::ForwardAttempt;
#[cfg(test)]
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
#[cfg(test)]
use http::{HeaderMap, Method};
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use uuid::Uuid;

#[cfg(test)]
use crate::proxy::host::cc_switch::forwarder_protocol_state_source::CcSwitchForwarderProtocolStateSource;

#[cfg(test)]
use crate::proxy::host::cc_switch::forwarder_auth_source::{
    default_forwarder_auth_source, forwarder_auth_source_from_sources,
};

#[cfg(test)]
use crate::proxy::engine::forward_pipeline::{
    ForwarderResponseFinalizationInput, ForwarderResponseSource,
};
#[cfg(test)]
use crate::proxy::host::cc_switch::forwarder_request_source::{
    default_forwarder_request_source, forwarder_rectifier_error_message,
    CcSwitchForwarderRequestSource,
};

#[cfg(test)]
use crate::proxy::host::cc_switch::forwarder_response_source::CcSwitchForwarderResponseSource;

#[cfg(test)]
mod tests {
    use crate::proxy::codex_chat_history::CodexChatHistoryStore;
    use crate::proxy::engine::routing::provider_router_app_error_from_provider_selection_failure;
    use crate::proxy::host::cc_switch::forwarder_auth_source::forwarder_auth_source_from_managed_account_runtime_source;
    use crate::proxy::host::cc_switch::forwarder_runtime_state_source::current_route_target_from_provider;
    use crate::proxy::host::cc_switch::provider_adapter_context::forwarder_provider_adapter_context_for_app;
    use crate::proxy::provider::{
        transform_claude_response_for_api_format, transform_claude_sse_for_api_format,
    };
    use crate::proxy_core::api::config::{
        app_type_from_circuit_key, channel_circuit_key, circuit_breaker_config_from_app_config,
        circuit_failure_threshold_from_app_config, provider_circuit_key, AllowResult,
        AppProxyConfig, CircuitBreakerConfig, CircuitBreakerStats, CircuitState,
    };
    use crate::proxy_core::api::domain::{
        extract_claude_base_url_from_settings, AppKind, ProviderKind,
    };
    use crate::proxy_core::api::ports::{
        app_proxy_config_with_enabled as proxy_app_config_with_enabled,
        apply_codex_takeover_auth_placeholder_if_present, apply_gemini_takeover_env_fields,
        claude_env_credentials_from_settings, claude_takeover_model_fields_from_settings,
        codex_auth_object_value_from_settings, codex_provider_live_write_parts_from_settings,
        ensure_codex_takeover_auth_placeholder, gemini_env_map_from_settings,
        gemini_live_backup_from_effective_settings, gemini_live_settings_from_env_json_and_config,
        gemini_live_settings_to_write, is_local_proxy_url, json_deep_merge, json_deep_remove,
        json_remove_array_items, json_value_is_subset, live_takeover_app_kinds,
        live_token_sync_app_label, normalize_claude_models_in_value,
        normalize_provider_settings_for_storage, provider_additive_live_write_action_for_app,
        provider_additive_update_route_for_app, provider_app_has_current_provider,
        provider_credential_issue_spec, provider_default_live_import_settings,
        provider_delete_is_current_provider, provider_initial_live_config_managed_marker,
        provider_key_change_policy_issue_for_app, provider_key_change_policy_issue_message,
        provider_live_config_presence_error_policy, provider_live_removal_target_for_app,
        provider_live_sync_scope_for_app, provider_omo_switch_pair_for_app_category,
        provider_omo_variant_for_app_category, provider_settings_validation_issue_spec,
        provider_settings_validation_parts_from_settings,
        provider_supports_legacy_common_config_migration as core_provider_supports_legacy_common_config_migration,
        provider_switch_backfill_source_id, provider_switch_dispatch_for_app,
        provider_switch_requires_takeover_lock, provider_switch_should_mark_live_config_managed,
        provider_takeover_live_sync_target_for_app, proxy_live_config_owned_by_takeover,
        proxy_runtime_status_stopped, proxy_switch_should_hot_switch,
        proxy_takeover_marked_state_is_reusable,
        proxy_takeover_should_restore_existing_backup_before_retakeover,
        remove_claude_takeover_env_fields_if_present,
        remove_codex_takeover_auth_placeholder_if_present,
        remove_gemini_takeover_env_fields_if_present, sanitize_claude_settings_for_live,
        should_skip_manual_default_live_import,
        should_skip_provider_legacy_common_config_migration,
        should_skip_startup_default_live_import, AuthInfo, AuthProvider, ClaudeTakeoverAuthPolicy,
        CodexProviderLiveWriteIssue, CodexProviderValidationIssue, ProviderAdditiveLiveWriteAction,
        ProviderAdditiveUpdateRoute, ProviderCredentialIssue, ProviderKeyChangePolicyIssue,
        ProviderLiveConfigPresenceErrorPolicy, ProviderLiveRemovalTarget, ProviderLiveSyncScope,
        ProviderOmoSwitchPair, ProviderOmoVariant, ProviderSettingsValidationIssue,
        ProviderSwitchDispatch, ProviderTakeoverLiveSyncTarget,
    };
    use crate::proxy_core::api::transforms::{
        infer_codex_chat_reasoning_profile, is_copilot_prompt_cache_provider,
        normalize_codex_chat_reasoning_profile, resolve_claude_api_format_from_settings,
        resolve_claude_responses_prompt_cache_key,
        should_preserve_reasoning_content_for_openai_chat, GeminiShadowStore,
    };
    use crate::proxy_core::api::transport::{
        apply_codex_chat_upstream_model_policy, codex_provider_catalog_model_ids_from_settings,
        resolve_codex_provider_upstream_model, ForwarderRectifierRetryKind,
    };
    use serde_json::json;

    use super::*;
    use crate::database::{
        ProxyChannelMigrationPreview, ProxyChannelModelRecord, ProxyChannelRecord,
        ProxyChannelSourceKind,
    };
    use crate::provider::{
        AuthBinding, AuthBindingSource, ProviderMeta, ProviderTestConfig, UsageScript,
    };
    use crate::proxy::engine::forward_pipeline::{
        ForwarderRequestRectifierPlan, ForwarderRuntimeStateSource,
    };
    use crate::proxy::engine::response_pipeline::{
        forward_error_usage_record_from_response_context,
        non_streaming_response_usage_record_from_response_context, response_usage_provider_facts,
        response_usage_provider_facts_from_optional,
        streaming_response_usage_record_from_response_context,
        transformed_response_usage_record_from_response_context,
        transformed_streaming_response_usage_record_from_response_context,
        ForwardErrorUsageContext, NonStreamingResponseUsageContext, StreamingResponseUsageContext,
        TransformedResponseUsageContext, TransformedStreamingResponseUsageContext,
    };
    use crate::proxy::error::ProxyError;
    use crate::proxy::error_mapper::forward_failure_kind_from_proxy_error;
    use crate::proxy::events::ProxyEventBus;
    use crate::proxy::host::cc_switch::database_channel_source::{
        channel_route_records_from_sources, channel_spec_from_source,
        proxy_channel_record_to_core_spec,
    };
    use crate::proxy::host::cc_switch::managed_account_runtime_source::{
        copilot_api_endpoint_from_app_handle, copilot_live_models_from_app_handle,
        copilot_model_vendor_from_app_handle, default_managed_account_runtime_source,
        resolve_managed_account_auth_from_runtime_source,
        ManagedAccountAdapterClaudeApiFormatInput, ManagedAccountAdapterCopilotLiveModelInput,
        ManagedAccountApplyCopilotDynamicBaseUrlInput, ManagedAccountAuthForProviderInput,
        ManagedAccountRuntimeSource,
    };
    use crate::proxy::host::cc_switch::provider_projection::{
        provider_claude_auth_key, provider_claude_base_url, provider_claude_kind,
        provider_claude_transform_streaming_decision, provider_gemini_kind,
        provider_github_copilot_managed_account_id, provider_is_codex_oauth,
        provider_kind_from_app_type_and_config, provider_kind_from_provider,
        provider_managed_account_binding_context, provider_managed_auth_classification,
        provider_needs_claude_transform, provider_spec_from_source, provider_specs_from_source,
        provider_uses_anthropic_rectifiers, proxy_provider_to_core_spec,
    };
    use crate::proxy::provider::ProviderAdapter;
    use crate::proxy::provider::{
        codex_provider_apply_chat_upstream_model, codex_provider_chat_reasoning_options,
        codex_provider_chat_reasoning_profile, codex_provider_should_convert_responses_to_chat,
        codex_provider_upstream_model, codex_provider_uses_chat_completions,
    };
    use crate::proxy_core::api::auth::channel_auth_profile_missing_key_error;
    use crate::proxy_core::api::auth::{
        claude_desktop_model_id_is_profile_safe, extract_gemini_base_url_from_settings,
        validate_claude_desktop_gateway_bearer_header, ClaudeAuthKeySource,
        ClaudeDesktopGatewayAuthError, ManagedAccountAuthRuntime,
        ManagedAccountRuntimeSource as CoreManagedAccountRuntimeSource, ManagementAuthError,
        ProviderAuthInfo, ProviderAuthStrategy,
    };
    use crate::proxy_core::api::auth::{
        extract_claude_auth_key_from_settings, extract_gemini_api_key_from_settings,
        is_gemini_oauth_key_shape, ManagedAccountBindingSource,
    };
    use crate::proxy_core::api::config::ResponseTimeoutConfig;
    use crate::proxy_core::api::domain::{
        channel_auth_profile_action, channel_auth_profile_missing_provider_warning,
        channel_spec_from_input, infer_claude_provider_kind, ChannelAuthProfileAction,
        ChannelHealthPolicy, ChannelOverrides, ChannelSpecInput, ModelCapabilities, ModelRoute,
        ProviderMetadata, ProviderSpec, RetryPolicy, UpstreamEndpoint,
    };
    use crate::proxy_core::api::errors::{
        proxy_error_http_status_code, proxy_error_response_body,
        selected_provider_display_name_for_error, selected_provider_missing_from_source_message,
        selected_provider_not_applied_message, unselected_provider_fallback_id,
        upstream_proxy_error_response_body, ProxyCoreError, ProxyCoreResult, ProxyErrorStatusKind,
    };
    use crate::proxy_core::api::events::{
        attempt_event, request_started_event, route_selected_event, server_started_event,
        server_stopped_event, AttemptEventChannel, AttemptEventPayloadInput, AttemptEventPhase,
        ProxyCoreEvent, ProxyEventEnvelope,
    };
    use crate::proxy_core::api::management::{
        ChannelKeyRuntimeCandidate, ChannelRouteSource, ChannelTestProbeRequest,
        RouteResolveRequest, StreamCheckResult,
    };
    use crate::proxy_core::api::model_catalog::{CopilotModel, DEFAULT_CODEX_MODEL_CONTEXT_WINDOW};
    use crate::proxy_core::api::ports::{
        codex_restored_live_settings_parts, gemini_env_json_from_map,
        gemini_env_string_map_from_settings, gemini_live_config_object_from_settings,
        ChannelKeyRuntimeLookupInput, ChannelKeyRuntimeSource, CopilotOptimizerConfig,
        CurrentRouteTarget, GeminiLiveConfigIssue, OptimizerConfig, ProxyConfig,
        ProxyRuntimeStatus, RectifierConfig,
    };
    use crate::proxy_core::api::routing::{
        apply_route_candidate_circuit_availability, current_provider_id_from_sources,
        resolve_channel_route, route_candidate_channel_circuit_keys, select_provider_ids,
        ChannelRouteCandidate, ChannelSpec, ChannelStatus, InterfaceKind,
        ProviderSelectionCandidate, ProviderSelectionFailure, ProviderSelectionInput, RoutePlan,
        RouteResolveChannelInput, RouteResolveModelInput, RouteSelection,
    };
    use crate::proxy_core::api::session::SessionIdSource;
    use crate::proxy_core::api::transforms::{
        normalize_anthropic_tool_thinking_history, normalize_claude_anthropic_messages,
        normalize_deepseek_thinking_disabled_strip_effort,
        should_normalize_anthropic_tool_thinking_history,
        should_normalize_mimo_anthropic_thinking_history, CodexProxyErrorContext,
        CodexProxyErrorKind, MimoAnthropicThinkingNormalizationInput,
        ANTHROPIC_TOOL_THINKING_PLACEHOLDER,
    };
    use crate::proxy_core::api::transforms::{
        CodexChatReasoningOptions, CodexChatReasoningProfile,
    };
    use crate::proxy_core::api::transport::resolve_response_runtime_policy;
    use crate::proxy_core::api::transport::{
        anthropic_beta_header_value, apply_forwarder_media_prevention_from_facts,
        bedrock_env_flag_from_provider_settings, build_claude_auth_headers,
        build_codex_bearer_auth_headers, build_copilot_auth_headers, build_gemini_auth_headers,
        build_upstream_request_headers, forward_upstream_url_plan,
        is_official_codex_client_user_agent, is_socks_proxy_url, resolve_upstream_send_policy,
        serialize_upstream_request_body, ClaudeAuthHeaderKind, CopilotAuthHeadersInput,
        CopilotClassification, ForwardFailureKind, ForwardUpstreamUrlPlanInput,
        ForwarderMediaPreventionFacts, ForwarderProtocolPreparationInput, ForwarderTransformPlan,
        OptionalCopilotAuthOptimizationPreparationInput, PreparedCopilotAuthOptimization,
        ProxyBody, ProxyCoreResponse, ProxyRequest, ProxyResponseBody, ProxyTransportResponseBody,
        UpstreamRequestHeadersInput, UpstreamSendPolicyInput, UpstreamSseAggregationKind,
        UpstreamTransportKind, UNSUPPORTED_IMAGE_MARKER,
    };
    use crate::proxy_core::api::usage::{
        usage_selected_provider_missing_log_message, TokenUsage, TransformedResponseUsageFormat,
        UsageRecord, UsageRecordFailureLogContext, UsageRouteContext,
        UsageSelectedProviderMissingPhase,
    };
    use crate::settings::CustomEndpoint;
    use bytes::Bytes;
    use futures::future::BoxFuture;
    use indexmap::IndexMap;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn attempt_event_payload_input_from_forward_attempt<'a>(
        request_id: &'a str,
        app_type: &'a str,
        attempt: &'a ForwardAttempt,
        error: Option<&'a str>,
    ) -> AttemptEventPayloadInput<'a> {
        let provider = attempt.provider();
        let channel = attempt.channel().map(|channel| AttemptEventChannel {
            channel_id: channel.channel_id.as_str(),
            channel_name: channel.channel_name.as_str(),
            interface_kind: channel.interface_kind.as_str(),
            public_model: channel.public_model.as_deref(),
            upstream_model: channel.upstream_model.as_deref(),
            pricing_model: channel.pricing_model.as_deref(),
        });

        AttemptEventPayloadInput {
            request_id,
            app_type,
            provider_id: provider.id.as_str(),
            provider_name: provider.name.as_str(),
            channel,
            error,
        }
    }

    fn live_takeover_app_types() -> [AppType; 3] {
        live_takeover_app_kinds().map(|app| {
            app.as_str()
                .parse::<AppType>()
                .expect("proxy-core live takeover app kind must be supported by cc-switch")
        })
    }

    fn provider_key_change_policy_issue(
        app_type: &AppType,
        existing_provider: Option<&Provider>,
    ) -> Option<ProviderKeyChangePolicyIssue> {
        provider_key_change_policy_issue_for_app(
            &AppKind::from(app_type),
            existing_provider.and_then(|provider| provider.category.as_deref()),
        )
    }

    fn provider_additive_live_write_action(
        app_type: &AppType,
        provider: &Provider,
        add_to_live: bool,
    ) -> ProviderAdditiveLiveWriteAction {
        provider_additive_live_write_action_for_app(
            &AppKind::from(app_type),
            provider.category.as_deref(),
            add_to_live,
        )
    }

    fn codex_api_key_from_auth_and_config(
        auth: Option<&Value>,
        config_text: Option<&str>,
    ) -> Option<String> {
        crate::codex_config::extract_codex_api_key(auth, config_text)
    }

    fn select_current_provider_ids_from_router_source(
        app_type: &str,
        current: Option<Provider>,
    ) -> Result<Vec<String>, AppError> {
        let current_provider_id = current.map(|provider| provider.id);
        let selected_ids =
            select_provider_ids(ProviderSelectionInput::current(current_provider_id.clone()))
                .map_err(|error| {
                    provider_router_app_error_from_provider_selection_failure(app_type, error)
                })?;

        Ok(selected_ids
            .into_iter()
            .filter(|provider_id| current_provider_id.as_ref() == Some(provider_id))
            .collect())
    }

    fn provider_credential_values_with_issue(
        provider: &Provider,
        app_type: &AppType,
    ) -> Result<
        crate::proxy_core::api::ports::ProviderCredentialValues,
        crate::proxy_core::api::ports::ProviderCredentialIssue,
    > {
        match app_type {
            AppType::Codex => {
                let auth = crate::proxy_core::api::ports::codex_auth_object_value_from_settings(
                    &provider.settings_config,
                )
                .ok_or(crate::proxy_core::api::ports::ProviderCredentialIssue::CodexAuthMissing)?;
                let config_toml = crate::proxy_core::api::ports::codex_config_text_from_settings(
                    &provider.settings_config,
                )
                .unwrap_or("");
                crate::proxy_core::api::ports::provider_codex_credential_values_from_parts(
                    crate::proxy_core::api::ports::CodexCredentialParts {
                        api_key: codex_api_key_from_auth_and_config(Some(auth), Some(config_toml)),
                        config_toml: Some(config_toml.to_string()),
                    },
                )
            }
            AppType::Claude
            | AppType::ClaudeDesktop
            | AppType::Gemini
            | AppType::OpenCode
            | AppType::OpenClaw
            | AppType::Hermes => {
                crate::proxy_core::api::ports::provider_non_codex_credential_values_from_settings(
                    &AppKind::from(app_type),
                    &provider.settings_config,
                )
                .map(|values| values.expect("known non-Codex app should project credential values"))
            }
        }
    }

    #[tokio::test]
    async fn non_managed_auth_passes_through_without_app_handle() {
        let auth = ProviderAuthInfo::new("sk-test".to_string(), ProviderAuthStrategy::Bearer);
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            serde_json::json!({}),
            None,
        );

        let runtime_source = default_managed_account_runtime_source();
        let resolved = resolve_managed_account_auth_from_runtime_source(
            runtime_source.as_ref(),
            &provider,
            auth.clone(),
        )
        .await
        .expect("non managed auth");

        assert_eq!(resolved.auth, auth);
        assert_eq!(resolved.codex_oauth_account_id, None);
        assert!(!resolved.should_send_codex_oauth_session_headers);
    }

    #[tokio::test]
    async fn managed_auth_requires_app_handle() {
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            serde_json::json!({}),
            None,
        );

        let runtime_source = default_managed_account_runtime_source();
        let copilot = resolve_managed_account_auth_from_runtime_source(
            runtime_source.as_ref(),
            &provider,
            ProviderAuthInfo::new(
                "PROXY_MANAGED".to_string(),
                ProviderAuthStrategy::GitHubCopilot,
            ),
        )
        .await
        .expect_err("copilot app handle error");
        assert!(matches!(
            copilot,
            ProxyError::AuthError(message)
                if message == "GitHub Copilot 认证不可用（无 AppHandle）"
        ));

        let codex = resolve_managed_account_auth_from_runtime_source(
            runtime_source.as_ref(),
            &provider,
            ProviderAuthInfo::new(
                "PROXY_MANAGED".to_string(),
                ProviderAuthStrategy::CodexOAuth,
            ),
        )
        .await
        .expect_err("codex app handle error");
        assert!(matches!(
            codex,
            ProxyError::AuthError(message)
                if message == "Codex OAuth 认证不可用（无 AppHandle）"
        ));
    }

    #[tokio::test]
    async fn copilot_runtime_helpers_skip_without_app_handle() {
        assert_eq!(copilot_api_endpoint_from_app_handle(None, None).await, None);
        assert_eq!(
            copilot_live_models_from_app_handle(None, None)
                .await
                .expect("skip"),
            None
        );
        assert_eq!(
            copilot_model_vendor_from_app_handle(None, None, "gpt-5").await,
            None
        );
    }

    #[test]
    fn app_type_conversion_preserves_known_and_custom_names() {
        assert_eq!(AppKind::from(&AppType::Claude), AppKind::Claude);
        assert_eq!(
            AppKind::from(&AppType::ClaudeDesktop),
            AppKind::ClaudeDesktop
        );
        assert_eq!(AppKind::from(&AppType::Codex), AppKind::Codex);
        assert_eq!(
            AppKind::from(&AppType::OpenClaw),
            AppKind::Custom("openclaw".to_string())
        );
        assert_eq!(
            app_type_from_proxy_core_app(&AppKind::Claude).expect("claude app"),
            AppType::Claude
        );
        assert_eq!(
            app_type_option_from_proxy_core_app(&AppKind::Custom("openclaw".to_string())),
            Some(AppType::OpenClaw)
        );
        assert!(matches!(
            app_type_from_proxy_core_app(&AppKind::Custom("unknown-app".to_string())),
            Err(ProxyCoreError::Config(message))
                if message.starts_with("unsupported app kind:")
                    && message.contains("unknown-app")
        ));
        assert_eq!(
            crate::proxy_core::api::domain::unsupported_app_kind_error_message(
                "invalid app: openclaw"
            ),
            "unsupported app kind: invalid app: openclaw"
        );

        let forward_request = forward_runtime_request_from_proxy_request(ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Bytes(Bytes::from_static(br#"{"ok":true}"#)),
        ))
        .expect("forward request");
        assert_eq!(forward_request.app_type, AppType::Claude);
        assert_eq!(forward_request.method, Method::POST);
        assert_eq!(forward_request.endpoint, "/v1/messages");
        assert_eq!(forward_request.body, json!({"ok": true}));
        assert_eq!(
            forward_request.session_result.source,
            SessionIdSource::Generated
        );
        assert!(!forward_request.session_result.client_provided);
        Uuid::parse_str(&forward_request.session_result.session_id)
            .expect("generated forward session id should be a UUID");

        let invalid_request = match forward_runtime_request_from_proxy_request(ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Bytes(Bytes::from_static(b"{bad-json")),
        )) {
            Ok(_) => panic!("invalid JSON body should fail"),
            Err(error) => error,
        };
        assert!(matches!(
            invalid_request,
            ProxyCoreError::InvalidRequest(message) if message.contains("invalid JSON body")
        ));
    }

    #[test]
    fn response_usage_helpers_project_provider_and_app_facts() {
        let mut provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..ProviderMeta::default()
        });

        let facts = response_usage_provider_facts(&provider, AppType::ClaudeDesktop.as_str());
        assert_eq!(facts.provider_id, "provider-a");
        assert_eq!(facts.provider_kind, Some(ProviderKind::GitHubCopilot));
        assert_eq!(facts.app, AppKind::ClaudeDesktop);

        let optional_facts = response_usage_provider_facts_from_optional(
            Some(&provider),
            AppType::ClaudeDesktop.as_str(),
            "Claude Desktop",
            UsageSelectedProviderMissingPhase::StreamingPassthrough,
        )
        .expect("provider facts");
        assert_eq!(optional_facts.provider_id, "provider-a");

        let missing_provider = response_usage_provider_facts_from_optional(
            None,
            AppType::ClaudeDesktop.as_str(),
            "Claude Desktop",
            UsageSelectedProviderMissingPhase::StreamingPassthrough,
        )
        .unwrap_err();
        assert_eq!(
            missing_provider,
            usage_selected_provider_missing_log_message(
                "Claude Desktop",
                UsageSelectedProviderMissingPhase::StreamingPassthrough
            )
        );

        fn parsed_stream_usage(_events: &[Value]) -> Option<TokenUsage> {
            Some(TokenUsage {
                input_tokens: 4,
                output_tokens: 6,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                model: None,
                message_id: None,
            })
        }

        fn extracted_stream_model(events: &[Value], fallback: &str) -> String {
            events
                .iter()
                .find_map(|event| event.get("model").and_then(Value::as_str))
                .unwrap_or(fallback)
                .to_string()
        }

        let route_context = UsageRouteContext {
            channel_id: "channel-1".to_string(),
            channel_name: "Channel One".to_string(),
            route_group: "beta".to_string(),
            pricing_model: Some("route-price-model".to_string()),
        };
        let error_record = forward_error_usage_record_from_response_context(
            ForwardErrorUsageContext {
                provider: Some(&provider),
                fallback_provider_id: "fallback-provider",
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                status_code: 502,
                error_message: "upstream failed".to_string(),
                latency_ms: 321,
                is_streaming: false,
                session_id: "session-error",
            },
            || "request-error".to_string(),
        );
        assert_eq!(error_record.provider_id, "provider-a");
        assert_eq!(
            error_record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(error_record.app, AppKind::ClaudeDesktop);
        assert_eq!(error_record.request_model, "request-model");
        assert_eq!(error_record.outbound_model, "outbound-model");
        assert_eq!(error_record.status_code, 502);
        assert_eq!(
            error_record.error_message.as_deref(),
            Some("upstream failed")
        );
        assert_eq!(error_record.tokens.input_tokens, 0);
        assert_eq!(error_record.channel_id.as_deref(), Some("channel-1"));

        let transformed_body = json!({
            "id": "msg_1",
            "model": "claude-response-model",
            "usage": {
                "input_tokens": 3,
                "output_tokens": 5
            }
        });
        let transformed_record = transformed_response_usage_record_from_response_context(
            TransformedResponseUsageContext {
                body: &transformed_body,
                format: TransformedResponseUsageFormat::Claude,
                provider: Some(&provider),
                tag: "Claude Desktop",
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                latency_ms: 123,
                status_code: 200,
                session_id: "session-transformed",
            },
            || "request-transformed".to_string(),
        )
        .expect("transformed provider facts")
        .expect("transformed usage record");
        assert_eq!(transformed_record.provider_id, "provider-a");
        assert_eq!(
            transformed_record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(transformed_record.app, AppKind::ClaudeDesktop);
        assert_eq!(
            transformed_record.response_model.as_deref(),
            Some("claude-response-model")
        );
        assert_eq!(transformed_record.tokens.input_tokens, 3);
        assert!(!transformed_record.is_streaming);
        assert_eq!(
            transformed_record.channel_name.as_deref(),
            Some("Channel One")
        );

        let missing_transformed = transformed_response_usage_record_from_response_context(
            TransformedResponseUsageContext {
                body: &transformed_body,
                format: TransformedResponseUsageFormat::Claude,
                provider: None,
                tag: "Claude Desktop",
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: None,
                route_context: None,
                latency_ms: 123,
                status_code: 200,
                session_id: "session-transformed",
            },
            || "request-missing".to_string(),
        )
        .unwrap_err();
        assert_eq!(
            missing_transformed,
            usage_selected_provider_missing_log_message(
                "Claude Desktop",
                UsageSelectedProviderMissingPhase::TransformedResponse
            )
        );

        let stream_events = vec![json!({"model": "stream-response-model"})];
        let stream_output = streaming_response_usage_record_from_response_context(
            StreamingResponseUsageContext {
                events: &stream_events,
                stream_parser: parsed_stream_usage,
                model_extractor: extracted_stream_model,
                provider_facts: &optional_facts,
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                latency_ms: 456,
                first_token_ms: Some(12),
                status_code: 200,
                session_id: "session-stream",
            },
            || "request-stream".to_string(),
        );
        assert_eq!(stream_output.record.provider_id, "provider-a");
        assert_eq!(
            stream_output.record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(stream_output.record.app, AppKind::ClaudeDesktop);
        assert_eq!(
            stream_output.record.response_model.as_deref(),
            Some("stream-response-model")
        );
        assert_eq!(stream_output.record.outbound_model, "outbound-model");
        assert_eq!(stream_output.record.tokens.input_tokens, 4);
        assert_eq!(stream_output.record.tokens.output_tokens, 6);
        assert!(stream_output.record.is_streaming);
        assert_eq!(stream_output.record.first_token_ms, Some(12));
        assert_eq!(
            stream_output.record.channel_id.as_deref(),
            Some("channel-1")
        );
        assert_eq!(
            stream_output.record.channel_name.as_deref(),
            Some("Channel One")
        );
        assert_eq!(stream_output.record.route_group.as_deref(), Some("beta"));

        let transformed_stream_events = vec![
            json!({
                "type": "message_start",
                "message": {
                    "id": "msg_stream_1",
                    "model": "claude-stream-model",
                    "usage": {
                        "input_tokens": 7
                    }
                }
            }),
            json!({
                "type": "message_delta",
                "usage": {
                    "output_tokens": 11
                }
            }),
        ];
        let transformed_stream_record =
            transformed_streaming_response_usage_record_from_response_context(
                TransformedStreamingResponseUsageContext {
                    events: &transformed_stream_events,
                    format: TransformedResponseUsageFormat::Claude,
                    provider_facts: &optional_facts,
                    request_model: "request-model",
                    outbound_model: Some("outbound-model"),
                    route_context: Some(&route_context),
                    latency_ms: 654,
                    first_token_ms: Some(34),
                    status_code: 200,
                    session_id: "session-transformed-stream",
                },
                || "request-transformed-stream".to_string(),
            )
            .expect("transformed streaming usage record");
        assert_eq!(transformed_stream_record.provider_id, "provider-a");
        assert_eq!(
            transformed_stream_record.response_model.as_deref(),
            Some("claude-stream-model")
        );
        assert_eq!(transformed_stream_record.tokens.input_tokens, 7);
        assert_eq!(transformed_stream_record.tokens.output_tokens, 11);
        assert_eq!(transformed_stream_record.first_token_ms, Some(34));
        assert!(transformed_stream_record.is_streaming);
        assert_eq!(
            transformed_stream_record.route_group.as_deref(),
            Some("beta")
        );

        let response_body =
            br#"{"model":"response-model","usage":{"prompt_tokens":2,"completion_tokens":3}}"#;
        let output = non_streaming_response_usage_record_from_response_context(
            NonStreamingResponseUsageContext {
                body: response_body,
                response_parser: TokenUsage::from_openai_response,
                provider: Some(&provider),
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: Some("outbound-model"),
                route_context: Some(&route_context),
                latency_ms: 123,
                status_code: 200,
                session_id: "session-1",
            },
            || "request-1".to_string(),
        )
        .expect("non-streaming usage record");

        assert!(output.usage_found);
        assert_eq!(output.record.provider_id, "provider-a");
        assert_eq!(
            output.record.provider_kind,
            Some(ProviderKind::GitHubCopilot)
        );
        assert_eq!(output.record.app, AppKind::ClaudeDesktop);
        assert_eq!(
            output.record.response_model.as_deref(),
            Some("response-model")
        );
        assert_eq!(output.record.outbound_model, "outbound-model");
        assert_eq!(output.record.tokens.input_tokens, 2);
        assert_eq!(output.record.tokens.output_tokens, 3);
        assert_eq!(output.record.channel_id.as_deref(), Some("channel-1"));
        assert_eq!(output.record.channel_name.as_deref(), Some("Channel One"));
        assert_eq!(output.record.route_group.as_deref(), Some("beta"));

        let missing = non_streaming_response_usage_record_from_response_context(
            NonStreamingResponseUsageContext {
                body: b"{}",
                response_parser: TokenUsage::from_openai_response,
                provider: None,
                app_type: AppType::ClaudeDesktop.as_str(),
                request_model: "request-model",
                outbound_model: None,
                route_context: None,
                latency_ms: 123,
                status_code: 200,
                session_id: "session-1",
            },
            || "request-2".to_string(),
        )
        .unwrap_err();
        assert_eq!(
            missing,
            selected_provider_not_applied_message(AppType::ClaudeDesktop.as_str())
        );
    }

    #[test]
    fn channel_auth_profile_warning_adapter_projects_optional_ref() {
        assert_eq!(
            channel_auth_profile_missing_provider_warning(
                "claude",
                Some("provider:claude:missing"),
            ),
            "[claude] channel auth profile references missing provider: provider:claude:missing"
        );
        assert_eq!(
            channel_auth_profile_missing_provider_warning("claude", None),
            "[claude] channel auth profile references missing provider: "
        );
        assert!(matches!(
            channel_auth_profile_action(
                "claude",
                Some("provider:claude:provider-a"),
                Some("channel-a")
            ),
            ChannelAuthProfileAction::Provider {
                provider_id,
                missing_provider_warning,
            } if provider_id == "provider-a"
                && missing_provider_warning.contains("provider:claude:provider-a")
        ));
        assert!(matches!(
            channel_auth_profile_action("claude", Some("channel-key:primary"), Some("channel-a")),
            ChannelAuthProfileAction::ChannelKey { channel_id, key_ref }
                if channel_id == "channel-a" && key_ref == "primary"
        ));
        assert!(matches!(
            channel_auth_profile_action("claude", Some("channel-key:primary"), None),
            ChannelAuthProfileAction::Ignore
        ));
        assert!(matches!(
            channel_auth_profile_missing_key_error("channel-a", "primary"),
            ProxyCoreError::Auth(message)
                if message.contains("channel_id=channel-a")
                    && message.contains("key_ref=primary")
        ));

        let provider = Provider::with_id(
            "route-provider".to_string(),
            "Route Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "route-key" } }),
            None,
        );
        let auth_provider =
            crate::proxy::host::cc_switch::auth_provider::provider_with_channel_auth_key(
                &AppType::Claude,
                &provider,
                "channel-key",
            );
        assert_eq!(
            provider
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("route-key")
        );
        assert_eq!(
            auth_provider
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("channel-key")
        );

        let route_provider = provider.clone();
        fn attempt_with_auth_ref(
            route_provider: &Provider,
            channel_id: &str,
            auth_profile_ref: &str,
        ) -> ForwardAttempt {
            let provider = ProviderSpec {
                id: route_provider.id.clone(),
                name: route_provider.name.clone(),
                kind: ProviderKind::Claude,
                account_ref: None,
                metadata: ProviderMetadata::default(),
            };
            let channel = ChannelSpec {
                id: channel_id.to_string(),
                provider_id: route_provider.id.clone(),
                app: AppKind::Claude,
                name: channel_id.to_string(),
                status: ChannelStatus::Enabled,
                endpoint: UpstreamEndpoint {
                    base_url: format!("https://{channel_id}.example.com/v1"),
                    path_template: None,
                    api_version: None,
                    timeout_profile: None,
                },
                interface: InterfaceKind::AnthropicMessages,
                auth_profile: Some(crate::proxy_core::api::domain::AuthProfileRef::new(
                    auth_profile_ref,
                )),
                models: Vec::new(),
                groups: vec!["default".to_string()],
                priority: 0,
                weight: 100,
                retry_policy: RetryPolicy {
                    raw: Value::Object(Default::default()),
                },
                health_policy: ChannelHealthPolicy {
                    raw: Value::Object(Default::default()),
                },
                overrides: ChannelOverrides {
                    headers: Value::Object(Default::default()),
                    params: Value::Object(Default::default()),
                    status_code_mapping: Value::Array(Vec::new()),
                    model_mapping: Value::Object(Default::default()),
                },
                tags: Vec::new(),
                metadata: Value::Object(Default::default()),
                source_ref: None,
                needs_review: false,
                review_reasons: Vec::new(),
            };
            let selection = crate::proxy_core::api::routing::route_selection_from_parts(
                provider,
                channel,
                None,
                InterfaceKind::AnthropicMessages,
            );
            ForwardAttempt::from_core_selection(&AppType::Claude, route_provider, &selection)
        }

        struct TestChannelKeyRuntimeSource {
            expected: Option<(&'static str, &'static str)>,
            candidate: Option<ChannelKeyRuntimeCandidate>,
        }

        impl ChannelKeyRuntimeSource for TestChannelKeyRuntimeSource {
            fn load_channel_key_candidate(
                &self,
                input: ChannelKeyRuntimeLookupInput<'_>,
            ) -> ProxyCoreResult<Option<ChannelKeyRuntimeCandidate>> {
                let Some((expected_channel_id, expected_key_ref)) = self.expected else {
                    panic!("provider auth should not load channel keys");
                };
                assert_eq!(input.channel_id, expected_channel_id);
                assert_eq!(input.key_ref, expected_key_ref);
                Ok(self.candidate.clone())
            }
        }

        let provider_auth = Provider::with_id(
            "provider-auth".to_string(),
            "Provider Auth".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-auth-key" } }),
            None,
        );
        let mut providers = IndexMap::new();
        providers.insert(route_provider.id.clone(), route_provider.clone());
        providers.insert(provider_auth.id.clone(), provider_auth);
        let mut provider_attempt = attempt_with_auth_ref(
            &route_provider,
            "channel-a",
            "provider:claude:provider-auth",
        );
        let provider_runtime_source = TestChannelKeyRuntimeSource {
            expected: None,
            candidate: None,
        };
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut provider_attempt),
            &provider_runtime_source,
        )
        .expect("apply provider auth profile");
        assert_eq!(provider_attempt.auth_provider().id, "provider-auth");

        let mut channel_key_attempt =
            attempt_with_auth_ref(&route_provider, "channel-key", "channel-key:primary");
        let channel_key_runtime_source = TestChannelKeyRuntimeSource {
            expected: Some(("channel-key", "primary")),
            candidate: Some(ChannelKeyRuntimeCandidate {
                channel_id: "channel-key".to_string(),
                key_ref: "primary".to_string(),
                key_value: "loaded-channel-key".to_string(),
                status: "enabled".to_string(),
                priority: 10,
                weight: 100,
                last_failure_at: Some(1_771_000_003),
            }),
        };
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut channel_key_attempt),
            &channel_key_runtime_source,
        )
        .expect("apply channel key auth profile");
        assert_eq!(
            channel_key_attempt
                .auth_provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("loaded-channel-key")
        );
    }

    #[test]
    fn claude_desktop_gateway_auth_adapter_projects_bearer_validation() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer gateway-token"),
        );

        validate_claude_desktop_gateway_bearer_header(&headers, "gateway-token")
            .expect("valid bearer");

        assert_eq!(
            validate_claude_desktop_gateway_bearer_header(&HeaderMap::new(), "gateway-token")
                .unwrap_err(),
            ClaudeDesktopGatewayAuthError::MissingAuthorizationHeader
        );
        assert_eq!(
            validate_claude_desktop_gateway_bearer_header(&headers, "wrong-token").unwrap_err(),
            ClaudeDesktopGatewayAuthError::InvalidToken
        );
    }

    #[test]
    fn claude_desktop_profile_model_id_policy_rejects_unsafe_aliases() {
        assert!(!claude_desktop_model_id_is_profile_safe(
            "claude-sonnet-4-6 [1m]"
        ));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "  claude-sonnet-4-6  [1M]  "
        ));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-old"));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "claude-3-5-sonnet-20241022"
        ));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "claude-deepseek-v4-pro"
        ));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-gpt-5-4"));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-"));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "anthropic/claude-"
        ));
        assert!(!claude_desktop_model_id_is_profile_safe("sonnet"));
        assert!(!claude_desktop_model_id_is_profile_safe("sonnet-"));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-sonnet-"));
        assert!(!claude_desktop_model_id_is_profile_safe("claude-opus-"));
        assert!(!claude_desktop_model_id_is_profile_safe(
            "anthropic/claude-haiku-"
        ));
        assert!(claude_desktop_model_id_is_profile_safe(
            "  claude-sonnet-4-6  "
        ));
        assert!(claude_desktop_model_id_is_profile_safe(
            "anthropic/claude-opus-4-8"
        ));
    }

    #[test]
    fn proxy_error_status_adapter_projects_http_contract() {
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::ForwardFailed),
            502
        );
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::AuthError),
            401
        );
        assert_eq!(
            proxy_error_http_status_code(ProxyErrorStatusKind::UpstreamError(42)),
            502
        );
        assert_eq!(
            crate::proxy_core::api::errors::error_message_with_context(
                "load config",
                "disk failed"
            ),
            "load config: disk failed"
        );
        assert_eq!(
            proxy_error_response_body("bad")["error"]["type"],
            "proxy_error"
        );
        assert_eq!(
            upstream_proxy_error_response_body(502, Some("bad gateway"))["error"]["message"],
            "bad gateway"
        );
    }

    #[test]
    fn global_proxy_adapter_projects_masking_and_loopback_policy() {
        use crate::proxy_core::api::transport::{
            invalid_explicit_proxy_url_message, proxy_values_point_to_loopback_port,
            validate_explicit_proxy_url, SYSTEM_PROXY_ENV_KEYS,
        };

        assert_eq!(
            SYSTEM_PROXY_ENV_KEYS,
            [
                "HTTP_PROXY",
                "http_proxy",
                "HTTPS_PROXY",
                "https_proxy",
                "ALL_PROXY",
                "all_proxy"
            ]
        );
        assert_eq!(
            crate::proxy_core::api::security::mask_url_for_log("http://user:pass@127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert!(
            crate::proxy_core::api::transport::proxy_url_points_to_loopback_port(
                "socks5://localhost:15721",
                15721
            )
        );
        assert!(
            !crate::proxy_core::api::transport::proxy_url_points_to_loopback_port(
                "http://127.0.0.1:7890",
                15721
            )
        );
        assert!(proxy_values_point_to_loopback_port(
            ["", " http://127.0.0.1:15721 "],
            15721
        ));
        assert!(validate_explicit_proxy_url("http://127.0.0.1:7890").is_ok());
        assert!(validate_explicit_proxy_url("socks5h://localhost:1080").is_ok());
        let invalid_scheme =
            validate_explicit_proxy_url("ftp://127.0.0.1:7890").expect_err("invalid scheme");
        assert!(invalid_scheme.contains(
            "Invalid proxy scheme 'ftp' in URL 'ftp://127.0.0.1:7890'. Supported: http, https, socks5, socks5h"
        ));
        let invalid_url = validate_explicit_proxy_url("http://[::1")
            .expect_err("invalid proxy URL should report parse error");
        assert!(invalid_url.contains("Invalid proxy URL 'http://[::1':"));
        assert_eq!(
            invalid_explicit_proxy_url_message("http://user:pass@127.0.0.1:7890", "bad"),
            "Invalid proxy URL 'http://127.0.0.1:7890': bad"
        );
    }

    #[test]
    fn provider_endpoint_adapter_projects_list_and_last_used() {
        let mut endpoints = HashMap::new();
        endpoints.insert(
            "https://old.example".to_string(),
            CustomEndpoint {
                url: "https://old.example".to_string(),
                added_at: 10,
                last_used: None,
            },
        );
        endpoints.insert(
            "https://new.example".to_string(),
            CustomEndpoint {
                url: "https://new.example".to_string(),
                added_at: 20,
                last_used: Some(1),
            },
        );
        let mut provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            custom_endpoints: endpoints,
            ..ProviderMeta::default()
        });

        let listed = provider.custom_endpoint_list();
        assert_eq!(
            listed
                .iter()
                .map(|endpoint| endpoint.url.as_str())
                .collect::<Vec<_>>(),
            vec!["https://new.example", "https://old.example"]
        );
        assert!(Provider::with_id(
            "provider-empty".to_string(),
            "Provider Empty".to_string(),
            json!({}),
            None,
        )
        .custom_endpoint_list()
        .is_empty());

        assert!(provider.mark_custom_endpoint_last_used("https://old.example", 1234));
        let old_last_used = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.custom_endpoints.get("https://old.example"))
            .and_then(|endpoint| endpoint.last_used);
        assert_eq!(old_last_used, Some(1234));
        assert!(!provider.mark_custom_endpoint_last_used("https://missing.example", 5678));
    }

    #[test]
    fn settings_config_adapter_preserves_frontend_contracts() {
        assert_eq!(
            serde_json::to_value(RectifierConfig::default()).expect("rectifier"),
            json!({
                "enabled": true,
                "requestThinkingSignature": true,
                "requestThinkingBudget": true,
                "requestMediaFallback": true,
                "requestMediaHeuristic": true
            })
        );
        assert_eq!(
            serde_json::to_value(OptimizerConfig::default()).expect("optimizer"),
            json!({
                "enabled": false,
                "thinkingOptimizer": true,
                "cacheInjection": true,
                "cacheTtl": "1h"
            })
        );
        assert_eq!(CopilotOptimizerConfig::default().warmup_model, "gpt-5-mini");
    }

    #[test]
    fn forwarder_request_source_selects_adapter_for_app() {
        let source = default_forwarder_request_source();

        let claude_adapter = source.adapter_context_for_app(&AppType::Claude);
        let fallback_adapter = source.adapter_context_for_app(&AppType::Hermes);

        assert_eq!(claude_adapter.facts().adapter_name, "Claude");
        assert_eq!(fallback_adapter.facts().adapter_name, "Codex");
    }

    #[tokio::test]
    async fn forwarder_protocol_state_source_skips_codex_chat_enrichment_when_disabled() {
        let source = CcSwitchForwarderProtocolStateSource::new(
            Arc::new(GeminiShadowStore::default()),
            Arc::new(CodexChatHistoryStore::default()),
        );
        let mut body = json!({
            "model": "gpt-5",
            "input": [{
                "type": "function_call_output",
                "call_id": "call-1",
                "output": "{}"
            }]
        });
        let original = body.clone();

        source
            .enrich_codex_chat_request(ForwarderCodexChatProtocolEnrichmentInput {
                body: &mut body,
                enabled: false,
            })
            .await;

        assert_eq!(body, original);
    }

    #[test]
    fn forwarder_request_source_prepares_bedrock_attempt_body() {
        let source = default_forwarder_request_source();
        let provider = Provider::with_id(
            "bedrock-provider".to_string(),
            "Bedrock Provider".to_string(),
            json!({
                "env": {
                    "CLAUDE_CODE_USE_BEDROCK": "1"
                }
            }),
            None,
        );
        let body = json!({
            "model": "anthropic.claude-opus-4-6-20250514-v1:0",
            "max_tokens": 16384,
            "tools": [{"name": "tool1"}],
            "system": [{"type": "text", "text": "sys prompt"}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "hello"}
                ]}
            ]
        });
        let config = OptimizerConfig {
            enabled: true,
            thinking_optimizer: true,
            cache_injection: true,
            cache_ttl: "1h".to_string(),
        };

        let prepared = source.prepare_attempt_body(ForwarderAttemptBodyInput {
            body: &body,
            provider: &provider,
            config: &config,
        });

        assert_eq!(body.get("thinking"), None);
        assert_eq!(prepared["thinking"]["type"], "adaptive");
        assert_eq!(prepared["output_config"]["effort"], "max");
        assert!(prepared["tools"][0].get("cache_control").is_some());
        assert!(prepared["system"][0].get("cache_control").is_some());
        assert!(prepared["messages"][1]["content"][0]
            .get("cache_control")
            .is_some());
    }

    #[test]
    fn forwarder_adapter_context_projects_provider_url_facts() {
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let mut provider = Provider::with_id(
            "copilot-provider".to_string(),
            "Copilot Provider".to_string(),
            json!({
                "base_url": "https://api.githubcopilot.com"
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            is_full_url: Some(true),
            ..Default::default()
        });

        let facts = adapter
            .provider_url_facts(&provider)
            .expect("provider URL facts");

        assert_eq!(facts.base_url, "https://api.githubcopilot.com");
        assert!(facts.is_full_url);
        assert!(facts.is_copilot);
    }

    #[test]
    fn forwarder_adapter_context_projects_adapter_facts() {
        let claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let codex_adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);

        let claude_facts = claude_adapter.facts();
        let codex_facts = codex_adapter.facts();

        assert_eq!(claude_facts.adapter_name, "Claude");
        assert!(claude_facts.is_claude_adapter);
        assert_eq!(codex_facts.adapter_name, "Codex");
        assert!(!codex_facts.is_claude_adapter);
    }

    #[test]
    fn forwarder_request_source_converts_codex_responses_to_chat_body() {
        let source = CcSwitchForwarderRequestSource::new(default_managed_account_runtime_source());
        let provider = Provider::with_id(
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

        let body =
            source.convert_codex_responses_to_chat_body(ForwarderCodexResponsesToChatInput {
                body: json!({
                    "model": "client-model",
                    "instructions": "Stay concise.",
                    "input": "Hello",
                    "max_output_tokens": 64,
                    "stream": true
                }),
                provider: &provider,
            });

        assert_eq!(body["model"], "upstream-model");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "Hello");
        assert_eq!(body["max_tokens"], 64);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn forwarder_request_source_projects_codex_responses_to_chat_gate() {
        let source = default_forwarder_request_source();
        let codex_adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = Provider::with_id(
            "codex-chat".to_string(),
            "Codex Chat".to_string(),
            json!({
                "config": r#"model_provider = "openai"
model = " upstream-model "

[model_providers.openai]
wire_api = "chat"
base_url = "https://api.openai.com/v1"
"#,
            }),
            None,
        );

        assert!(
            source
                .transform_plan(ForwarderTransformPlanInput {
                    app_type: &AppType::Codex,
                    adapter: &codex_adapter,
                    endpoint: "/responses",
                    provider: &provider,
                    resolved_claude_api_format: None,
                })
                .codex_responses_to_chat
        );
        assert!(
            !source
                .transform_plan(ForwarderTransformPlanInput {
                    app_type: &AppType::Claude,
                    adapter: &claude_adapter,
                    endpoint: "/responses",
                    provider: &provider,
                    resolved_claude_api_format: None,
                })
                .codex_responses_to_chat
        );
        assert!(
            !source
                .transform_plan(ForwarderTransformPlanInput {
                    app_type: &AppType::Codex,
                    adapter: &codex_adapter,
                    endpoint: "/chat/completions",
                    provider: &provider,
                    resolved_claude_api_format: None,
                })
                .codex_responses_to_chat
        );
    }

    #[test]
    fn forwarder_request_source_wraps_provider_transform_request() {
        let source = CcSwitchForwarderRequestSource::new(default_managed_account_runtime_source());
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let provider = Provider::with_id(
            "codex-provider".to_string(),
            "Codex Provider".to_string(),
            json!({}),
            None,
        );
        let body = json!({"model": "gpt-5", "messages": []});

        let transformed = source
            .transform_provider_request_body(ForwarderProviderTransformInput {
                adapter: &adapter,
                body: body.clone(),
                provider: &provider,
            })
            .expect("provider transform");

        assert_eq!(transformed, body);
    }

    #[test]
    fn forwarder_request_source_transforms_request_body_and_tracks_outbound_model() {
        let source = default_forwarder_request_source();
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        let no_transform_plan = ForwarderTransformPlan {
            needs_transform: false,
            use_claude_transform: false,
            use_provider_transform: false,
            claude_api_format_for_url: None,
            claude_api_format_for_transform: None,
            codex_responses_to_chat: false,
        };

        let passthrough = source
            .transform_request_body(ForwarderRequestBodyTransformInput {
                adapter: &adapter,
                body: json!({"model": "mapped-model", "messages": []}),
                provider: &provider,
                transform_plan: &no_transform_plan,
                claude_transformed_body: None,
            })
            .expect("passthrough request body");
        assert_eq!(passthrough.body["model"], "mapped-model");
        assert_eq!(passthrough.outbound_model.as_deref(), Some("mapped-model"));

        let claude_transform_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            use_provider_transform: false,
            claude_api_format_for_url: Some("openai_chat".to_string()),
            claude_api_format_for_transform: Some("openai_chat".to_string()),
            codex_responses_to_chat: false,
        };
        let claude_transformed = source
            .transform_request_body(ForwarderRequestBodyTransformInput {
                adapter: &adapter,
                body: json!({"model": "mapped-model", "messages": []}),
                provider: &provider,
                transform_plan: &claude_transform_plan,
                claude_transformed_body: Some(json!({
                    "model": "chat-model",
                    "messages": []
                })),
            })
            .expect("Claude transformed request body");
        assert_eq!(claude_transformed.body["model"], "chat-model");
        assert_eq!(
            claude_transformed.outbound_model.as_deref(),
            Some("mapped-model")
        );
    }

    #[test]
    fn forwarder_request_source_prefers_codex_chat_bridge_over_claude_body() {
        let source = default_forwarder_request_source();
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = Provider::with_id(
            "codex-chat".to_string(),
            "Codex Chat".to_string(),
            json!({
                "config": r#"model_provider = "openai"
model = " upstream-model "

[model_providers.openai]
wire_api = "chat"
base_url = "https://api.openai.com/v1"
"#,
            }),
            None,
        );
        let transform_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            use_provider_transform: false,
            claude_api_format_for_url: Some("openai_chat".to_string()),
            claude_api_format_for_transform: Some("openai_chat".to_string()),
            codex_responses_to_chat: true,
        };

        let transformed = source
            .transform_request_body(ForwarderRequestBodyTransformInput {
                adapter: &adapter,
                body: json!({
                    "model": "client-model",
                    "instructions": "Stay concise.",
                    "input": "Hello"
                }),
                provider: &provider,
                transform_plan: &transform_plan,
                claude_transformed_body: Some(json!({"model": "should-not-win"})),
            })
            .expect("Codex chat bridge body");

        assert_eq!(transformed.body["model"], "upstream-model");
        assert_eq!(transformed.body["messages"][0]["role"], "system");
        assert_eq!(transformed.body["messages"][1]["content"], "Hello");
        assert_eq!(transformed.outbound_model.as_deref(), Some("client-model"));
    }

    #[test]
    fn forwarder_request_source_projects_transform_plan() {
        let source = default_forwarder_request_source();
        let claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let codex_adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let mut claude_provider = Provider::with_id(
            "claude-provider".to_string(),
            "Claude Provider".to_string(),
            json!({}),
            None,
        );
        claude_provider.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });

        let resolved_plan = source.transform_plan(ForwarderTransformPlanInput {
            app_type: &AppType::Claude,
            adapter: &claude_adapter,
            endpoint: "/v1/messages",
            provider: &claude_provider,
            resolved_claude_api_format: Some("gemini_native"),
        });
        assert!(resolved_plan.needs_transform);
        assert!(resolved_plan.use_claude_transform);
        assert!(!resolved_plan.use_provider_transform);
        assert_eq!(
            resolved_plan.claude_api_format_for_url.as_deref(),
            Some("gemini_native")
        );
        assert_eq!(
            resolved_plan.claude_api_format_for_transform.as_deref(),
            Some("gemini_native")
        );
        assert!(!resolved_plan.codex_responses_to_chat);

        let fallback_plan = source.transform_plan(ForwarderTransformPlanInput {
            app_type: &AppType::Claude,
            adapter: &claude_adapter,
            endpoint: "/v1/messages",
            provider: &claude_provider,
            resolved_claude_api_format: None,
        });
        assert!(fallback_plan.needs_transform);
        assert!(fallback_plan.use_claude_transform);
        assert!(!fallback_plan.use_provider_transform);
        assert_eq!(
            fallback_plan.claude_api_format_for_url.as_deref(),
            Some("openai_chat")
        );
        assert_eq!(
            fallback_plan.claude_api_format_for_transform.as_deref(),
            Some("openai_chat")
        );
        assert!(!fallback_plan.codex_responses_to_chat);

        let codex_plan = source.transform_plan(ForwarderTransformPlanInput {
            app_type: &AppType::Codex,
            adapter: &codex_adapter,
            endpoint: "/v1/chat/completions",
            provider: &claude_provider,
            resolved_claude_api_format: None,
        });
        assert!(!codex_plan.needs_transform);
        assert!(!codex_plan.use_claude_transform);
        assert!(!codex_plan.use_provider_transform);
        assert!(codex_plan.claude_api_format_for_url.is_none());
        assert!(codex_plan.claude_api_format_for_transform.is_none());
        assert!(!codex_plan.codex_responses_to_chat);
    }

    #[test]
    fn forwarder_request_source_projects_protocol_preparation() {
        let source = default_forwarder_request_source();
        let claude_transform_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            use_provider_transform: false,
            claude_api_format_for_url: Some("openai_chat".to_string()),
            claude_api_format_for_transform: Some("openai_chat".to_string()),
            codex_responses_to_chat: false,
        };
        let claude_preparation = source.protocol_preparation(ForwarderProtocolPreparationInput {
            transform_plan: &claude_transform_plan,
        });
        assert!(claude_preparation.should_transform_claude_request);
        assert_eq!(
            claude_preparation
                .claude_api_format_for_transform
                .as_deref(),
            Some("openai_chat")
        );
        assert!(!claude_preparation.codex_chat_enrichment_enabled);

        let codex_bridge_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            use_provider_transform: false,
            claude_api_format_for_url: Some("openai_chat".to_string()),
            claude_api_format_for_transform: Some("openai_chat".to_string()),
            codex_responses_to_chat: true,
        };
        let codex_preparation = source.protocol_preparation(ForwarderProtocolPreparationInput {
            transform_plan: &codex_bridge_plan,
        });
        assert!(!codex_preparation.should_transform_claude_request);
        assert!(codex_preparation.claude_api_format_for_transform.is_none());
        assert!(codex_preparation.codex_chat_enrichment_enabled);
    }

    #[test]
    fn forwarder_request_source_plans_codex_upstream_url() {
        let source = default_forwarder_request_source();
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let body = json!({});
        let param_overrides = json!({"api-version": "2026-06-21"});
        let transform_plan = ForwarderTransformPlan {
            needs_transform: false,
            use_claude_transform: false,
            use_provider_transform: false,
            claude_api_format_for_url: None,
            claude_api_format_for_transform: None,
            codex_responses_to_chat: true,
        };

        let plan = source.plan_upstream_url(ForwarderUpstreamUrlInput {
            adapter: &adapter,
            base_url: "https://api.openai.com/v1/chat/completions",
            endpoint: "/v1/responses?foo=bar&api-version=old",
            is_full_url: false,
            transform_plan: &transform_plan,
            is_copilot: false,
            body: &body,
            channel_param_overrides: Some(&param_overrides),
        });

        assert_eq!(
            plan.effective_endpoint,
            "/chat/completions?foo=bar&api-version=old"
        );
        assert_eq!(
            plan.passthrough_query.as_deref(),
            Some("foo=bar&api-version=old")
        );
        assert_eq!(
            plan.url,
            "https://api.openai.com/v1/chat/completions?foo=bar&api-version=2026-06-21"
        );
    }

    #[test]
    fn forwarder_request_source_wraps_copilot_optimizer_sequence() {
        let source = CcSwitchForwarderRequestSource::new(default_managed_account_runtime_source());
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            "tools-2024-04-04".parse().expect("header value"),
        );

        let optimized = source.optimize_copilot_request(ForwarderCopilotRequestOptimizationInput {
            body: json!({
                "model": "claude-sonnet-4",
                "messages": [{"role": "user", "content": "Hello"}]
            }),
            headers: &headers,
            config: &CopilotOptimizerConfig::default(),
        });

        assert_eq!(optimized.classification.initiator, "user");
        assert!(optimized.classification.is_warmup);
        assert_eq!(optimized.body["model"], "gpt-5-mini");
    }

    #[test]
    fn forwarder_request_source_gates_copilot_optimizer_sequence() {
        let source = default_forwarder_request_source();
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            "tools-2024-04-04".parse().expect("header value"),
        );
        let disabled_config = CopilotOptimizerConfig {
            enabled: false,
            ..Default::default()
        };

        let disabled = source.prepare_copilot_request_optimization(
            ForwarderCopilotRequestOptimizationGateInput {
                body: json!({ "model": "claude-sonnet-4" }),
                headers: &headers,
                config: &disabled_config,
                is_copilot: true,
            },
        );
        assert!(disabled.classification.is_none());
        assert_eq!(disabled.body["model"], "claude-sonnet-4");

        let non_copilot = source.prepare_copilot_request_optimization(
            ForwarderCopilotRequestOptimizationGateInput {
                body: json!({ "model": "claude-sonnet-4" }),
                headers: &headers,
                config: &CopilotOptimizerConfig::default(),
                is_copilot: false,
            },
        );
        assert!(non_copilot.classification.is_none());
        assert_eq!(non_copilot.body["model"], "claude-sonnet-4");

        let enabled = source.prepare_copilot_request_optimization(
            ForwarderCopilotRequestOptimizationGateInput {
                body: json!({
                    "model": "claude-sonnet-4",
                    "messages": [{"role": "user", "content": "Hello"}]
                }),
                headers: &headers,
                config: &CopilotOptimizerConfig::default(),
                is_copilot: true,
            },
        );
        assert!(enabled.classification.is_some());
        assert_eq!(enabled.body["model"], "gpt-5-mini");
    }

    #[test]
    fn forwarder_auth_source_prepares_optional_copilot_auth_optimization() {
        let source = default_forwarder_auth_source();
        let headers = HeaderMap::new();
        let config = CopilotOptimizerConfig {
            request_classification: true,
            deterministic_request_id: true,
            ..CopilotOptimizerConfig::default()
        };

        let skipped = source.prepare_optional_copilot_auth_optimization(
            OptionalCopilotAuthOptimizationPreparationInput {
                classification: None,
                config: &config,
                session_source_body: &json!({}),
                request_body: &json!({}),
                headers: &headers,
            },
        );
        assert!(skipped.is_none());

        let classification = CopilotClassification {
            initiator: "user",
            is_warmup: false,
            is_compact: false,
            is_subagent: true,
        };
        let prepared = source
            .prepare_optional_copilot_auth_optimization(
                OptionalCopilotAuthOptimizationPreparationInput {
                    classification: Some(classification),
                    config: &config,
                    session_source_body: &json!({
                        "metadata": { "session_id": "session-a" }
                    }),
                    request_body: &json!({
                        "messages": [{"role": "user", "content": "Hello"}]
                    }),
                    headers: &headers,
                },
            )
            .expect("prepared copilot auth optimization");

        assert!(prepared.request_classification_enabled);
        assert_eq!(prepared.initiator, "user");
        assert!(prepared.is_subagent);
        assert!(prepared.deterministic_request_id.is_some());
        assert!(prepared.interaction_id.is_some());
    }

    struct ChannelHeaderAuthProvider;

    impl AuthProvider for ChannelHeaderAuthProvider {
        fn resolve_auth<'a>(
            &'a self,
            app: &'a AppKind,
            provider: &'a ProviderSpec,
            channel: &'a ChannelSpec,
            request: &'a ProxyRequest,
        ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>> {
            let app = app.as_str().to_string();
            let provider_id = provider.id.clone();
            let channel_id = channel.id.clone();
            let requested_model = request.requested_model.clone();
            Box::pin(async move {
                Ok(AuthInfo {
                    headers: vec![("x-core-auth-channel".to_string(), channel_id.clone())],
                    account_ref: Some(provider_id.clone()),
                    metadata: json!({
                        "app": app,
                        "providerId": provider_id,
                        "channelId": channel_id,
                        "requestedModel": requested_model,
                    }),
                })
            })
        }
    }

    #[tokio::test]
    async fn forwarder_auth_source_uses_core_auth_provider_route_context_headers() {
        let source = forwarder_auth_source_from_sources(
            default_managed_account_runtime_source(),
            Arc::new(ChannelHeaderAuthProvider),
        );
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "provider-key"
                }
            }),
            None,
        );
        let attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            ChannelRouteCandidate {
                channel_id: "channel-auth".to_string(),
                provider_id: provider.id.clone(),
                channel_name: "Channel Auth".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                public_model: Some("sonnet-public".to_string()),
                upstream_model: Some("sonnet-upstream".to_string()),
                route_group: "default".to_string(),
                priority: 100,
                weight: 1,
                source_kind: "manual".to_string(),
            },
        );
        let method = Method::POST;
        let body = json!({ "model": "sonnet-public" });
        let headers = HeaderMap::new();

        let resolved = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: false,
                copilot_optimization: None,
            })
            .await
            .expect("resolve auth headers");

        assert_eq!(resolved.codex_oauth_session_headers.len(), 0);
        assert_eq!(resolved.auth_headers.len(), 1);
        assert_eq!(
            resolved.auth_headers[0].0,
            http::HeaderName::from_static("x-core-auth-channel")
        );
        assert_eq!(
            resolved.auth_headers[0].1.to_str().expect("header value"),
            "channel-auth"
        );
        assert!(!resolved
            .auth_headers
            .iter()
            .any(|(name, _)| name == http::header::AUTHORIZATION));
    }

    #[tokio::test]
    async fn forwarder_auth_source_uses_core_codex_oauth_session_header_gate() {
        let source = forwarder_auth_source_from_managed_account_runtime_source(Arc::new(
            StaticManagedAuthResolutionSource,
        ));
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = provider_with_managed_account_binding("codex_oauth", "codex-acct");
        let attempt = ForwardAttempt::from_provider(provider);
        let method = Method::POST;
        let body = json!({ "model": "gpt-5" });
        let headers = HeaderMap::new();

        let without_client_session = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: false,
                copilot_optimization: None,
            })
            .await
            .expect("resolve auth headers without client session");

        assert!(without_client_session
            .codex_oauth_session_headers
            .is_empty());

        let with_client_session = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: true,
                copilot_optimization: None,
            })
            .await
            .expect("resolve auth headers with client session");

        assert!(with_client_session
            .auth_headers
            .iter()
            .any(|(name, value)| {
                name == http::header::AUTHORIZATION
                    && value == http::HeaderValue::from_static("Bearer codex-token:codex-acct")
            }));

        let mut session_headers = HeaderMap::new();
        for (name, value) in with_client_session.codex_oauth_session_headers {
            session_headers.insert(name, value);
        }

        assert_eq!(
            session_headers.get("session_id"),
            Some(&http::HeaderValue::from_static("session-a"))
        );
        assert_eq!(
            session_headers.get("x-client-request-id"),
            Some(&http::HeaderValue::from_static("session-a"))
        );
        assert_eq!(
            session_headers.get("x-codex-window-id"),
            Some(&http::HeaderValue::from_static("session-a:0"))
        );
    }

    #[tokio::test]
    async fn forwarder_auth_source_uses_core_copilot_auth_override_facts() {
        let source = forwarder_auth_source_from_managed_account_runtime_source(Arc::new(
            StaticManagedAuthResolutionSource,
        ));
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = provider_with_managed_account_binding("github_copilot", "copilot-acct");
        let attempt = ForwardAttempt::from_provider(provider);
        let method = Method::POST;
        let body = json!({ "model": "claude-sonnet-4" });
        let headers = HeaderMap::new();

        let resolved = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: false,
                copilot_optimization: Some(PreparedCopilotAuthOptimization {
                    request_classification_enabled: true,
                    initiator: "agent",
                    is_subagent: true,
                    deterministic_request_id: Some("request-id".to_string()),
                    interaction_id: Some("interaction-id".to_string()),
                }),
            })
            .await
            .expect("resolve copilot auth headers");

        let mut map = HeaderMap::new();
        for (name, value) in resolved.auth_headers {
            map.insert(name, value);
        }

        assert_eq!(
            map.get(http::header::AUTHORIZATION),
            Some(&http::HeaderValue::from_static(
                "Bearer copilot-token:copilot-acct"
            ))
        );
        assert_eq!(
            map.get("x-initiator"),
            Some(&http::HeaderValue::from_static("agent"))
        );
        assert_eq!(
            map.get("x-interaction-type"),
            Some(&http::HeaderValue::from_static("conversation-subagent"))
        );
        assert_eq!(
            map.get("x-request-id"),
            Some(&http::HeaderValue::from_static("request-id"))
        );
        assert_eq!(
            map.get("x-agent-task-id"),
            Some(&http::HeaderValue::from_static("request-id"))
        );
        assert_eq!(
            map.get("x-interaction-id"),
            Some(&http::HeaderValue::from_static("interaction-id"))
        );
    }

    #[test]
    fn forwarder_request_source_prepares_provider_request_body() {
        let source = default_forwarder_request_source();
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-mapped"
                }
            }),
            None,
        );
        let channel = crate::proxy_core::api::routing::resolved_channel_attempt_from_candidate(
            ChannelRouteCandidate {
                channel_id: "ch_1".to_string(),
                provider_id: "provider-a".to_string(),
                channel_name: "Relay".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                public_model: Some("sonnet-mapped".to_string()),
                upstream_model: Some("upstream-sonnet[1M]".to_string()),
                route_group: "default".to_string(),
                priority: 100,
                weight: 1,
                source_kind: "manual".to_string(),
            },
        );

        let body = source
            .prepare_provider_request_body(ForwarderProviderRequestBodyInput {
                app_type: &AppType::Claude,
                body: json!({"model": "claude-sonnet", "messages": []}),
                provider: &provider,
                channel: Some(&channel),
                is_copilot: false,
            })
            .expect("prepared body");

        assert_eq!(body["model"], "upstream-sonnet");
    }

    #[test]
    fn forwarder_request_source_normalizes_copilot_model_body() {
        let source = default_forwarder_request_source();
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );

        let body = source
            .prepare_provider_request_body(ForwarderProviderRequestBodyInput {
                app_type: &AppType::Claude,
                body: json!({"model": "claude-sonnet-4-6[1m]", "messages": []}),
                provider: &provider,
                channel: None,
                is_copilot: true,
            })
            .expect("prepared body");

        assert_eq!(body["model"], "claude-sonnet-4.6-1m");
    }

    #[test]
    fn forwarder_request_source_applies_claude_body_policies() {
        let source = default_forwarder_request_source();
        let mut provider = Provider::with_id(
            "claude-normalize".to_string(),
            "Claude Normalize".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic"
                }
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "messages": [{ "role": "user", "content": "hello" }]
        });
        let claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let codex_adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let media_disabled_config = RectifierConfig {
            request_media_fallback: false,
            request_media_heuristic: false,
            ..RectifierConfig::default()
        };

        source.apply_claude_body_policies(ForwarderClaudeBodyPolicyInput {
            adapter: &claude_adapter,
            body: &mut body,
            provider: &provider,
            api_format: Some("anthropic"),
            config: &media_disabled_config,
        });

        assert!(body.get("output_config").is_none());

        let mut skipped_body = json!({
            "model": "deepseek-v4-pro",
            "output_config": { "effort": "max" },
            "messages": [{ "role": "user", "content": "hello" }]
        });
        source.apply_claude_body_policies(ForwarderClaudeBodyPolicyInput {
            adapter: &codex_adapter,
            body: &mut skipped_body,
            provider: &provider,
            api_format: Some("anthropic"),
            config: &media_disabled_config,
        });

        assert!(skipped_body.get("output_config").is_some());
    }

    #[test]
    fn forwarder_request_source_gates_app_media_prevention() {
        let source = default_forwarder_request_source();
        let provider = Provider::with_id("media".to_string(), "Media".to_string(), json!({}), None);
        let default_config = RectifierConfig::default();
        let mut non_codex_body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        assert_eq!(
            source.apply_app_media_prevention(ForwarderAppMediaPreventionInput {
                app_type: &AppType::Claude,
                body: &mut non_codex_body,
                provider: &provider,
                config: &default_config,
            }),
            0
        );
        assert_eq!(non_codex_body["messages"][0]["content"][0]["type"], "image");

        let mut codex_body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        assert_eq!(
            source.apply_app_media_prevention(ForwarderAppMediaPreventionInput {
                app_type: &AppType::Codex,
                body: &mut codex_body,
                provider: &provider,
                config: &default_config,
            }),
            1
        );
        assert_eq!(codex_body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn forwarder_request_source_projects_media_retry_plan() {
        let source = default_forwarder_request_source();
        let provider = Provider::with_id("media".to_string(), "Media".to_string(), json!({}), None);
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let config = RectifierConfig::default();
        let provider_body = json!({
            "model": "vision-rejecting-model",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        let unsupported_image_error = ProxyError::UpstreamError {
            status: 400,
            body: Some(
                r#"{"error":{"message":"This model cannot process image inputs"}}"#.to_string(),
            ),
        };

        let plan = source
            .media_retry_plan(ForwarderMediaRetryPlanInput {
                app: "claude",
                adapter: &adapter,
                provider: &provider,
                already_retried: false,
                provider_body: &provider_body,
                error: &unsupported_image_error,
                config: &config,
            })
            .expect("media retry plan");
        assert_eq!(plan.body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(
            plan.body["messages"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );

        let ordinary_error = ProxyError::UpstreamError {
            status: 400,
            body: Some(r#"{"error":{"message":"bad request"}}"#.to_string()),
        };
        assert!(source
            .media_retry_plan(ForwarderMediaRetryPlanInput {
                app: "claude",
                adapter: &adapter,
                provider: &provider,
                already_retried: false,
                provider_body: &provider_body,
                error: &ordinary_error,
                config: &config,
            })
            .is_none());
    }

    #[test]
    fn forwarder_request_source_builds_upstream_parts_from_adapter_context() {
        let source = default_forwarder_request_source();
        let mut provider = Provider::with_id(
            "headers".to_string(),
            "Headers".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            custom_user_agent: Some("cc-switch-test/2.0".to_string()),
            ..ProviderMeta::default()
        });
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let mut inbound_headers = HeaderMap::new();
        inbound_headers.insert(
            "anthropic-beta",
            http::HeaderValue::from_static("other-beta"),
        );
        let prepared_request = ForwarderPreparedRequest {
            body: json!({ "model": "claude-3", "messages": [] }),
            request_is_streaming: false,
            force_identity_encoding: false,
            body_model_label: "claude-3".to_string(),
            outbound_model: None,
        };

        let request_parts = source
            .build_upstream_request_parts(ForwarderRequestPartsInput {
                method: &http::Method::POST,
                url: "https://upstream.example/v1/messages",
                inbound_headers: &inbound_headers,
                provider: &provider,
                prepared_request: &prepared_request,
                auth_headers: &[],
                channel_header_overrides: None,
                is_copilot: false,
                adapter: &adapter,
                resolved_claude_api_format: Some("anthropic"),
                codex_oauth_session_headers: &[],
            })
            .expect("request parts");

        assert!(request_parts.preserve_exact_header_case);
        assert_eq!(
            request_parts
                .ordered_headers
                .get("anthropic-beta")
                .and_then(|value| value.to_str().ok()),
            Some("claude-code-20250219,other-beta")
        );
        assert_eq!(
            request_parts
                .ordered_headers
                .get(http::header::USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some("cc-switch-test/2.0")
        );
        let serialized_body: Value =
            serde_json::from_slice(&request_parts.body).expect("serialized body");
        assert_eq!(serialized_body, prepared_request.body);
    }

    #[test]
    fn forwarder_request_source_prepares_final_body_model_facts() {
        let source = default_forwarder_request_source();
        let headers = HeaderMap::new();
        let transform_plan = ForwarderTransformPlan {
            needs_transform: false,
            use_claude_transform: false,
            use_provider_transform: false,
            claude_api_format_for_url: None,
            claude_api_format_for_transform: None,
            codex_responses_to_chat: false,
        };

        let prepared = source.prepare_upstream_body(ForwarderRequestPreparationInput {
            app: "codex",
            provider_id: "provider-a",
            endpoint: "/v1/chat/completions",
            api_format: None,
            body: json!({ "model": "upstream-sonnet", "messages": [] }),
            session_client_provided: false,
            transform_plan: &transform_plan,
            initial_outbound_model: Some("initial-model".to_string()),
            headers: &headers,
        });

        assert_eq!(prepared.body_model_label, "upstream-sonnet");
        assert_eq!(prepared.outbound_model.as_deref(), Some("upstream-sonnet"));

        let prepared_without_model =
            source.prepare_upstream_body(ForwarderRequestPreparationInput {
                app: "codex",
                provider_id: "provider-a",
                endpoint: "/v1/chat/completions",
                api_format: None,
                body: json!({ "messages": [] }),
                session_client_provided: false,
                transform_plan: &transform_plan,
                initial_outbound_model: Some("initial-model".to_string()),
                headers: &headers,
            });

        assert_eq!(prepared_without_model.body_model_label, "<none>");
        assert_eq!(
            prepared_without_model.outbound_model.as_deref(),
            Some("initial-model")
        );
    }

    #[test]
    fn forwarder_request_source_projects_anthropic_rectifier_gate() {
        let source = default_forwarder_request_source();
        let mut claude_auth_provider = Provider::with_id(
            "claude-auth".to_string(),
            "Claude Auth".to_string(),
            json!({}),
            None,
        );
        claude_auth_provider.meta = Some(ProviderMeta {
            provider_type: Some("claude_auth".to_string()),
            ..Default::default()
        });
        let default_claude_provider = Provider::with_id(
            "default-provider".to_string(),
            "Default Provider".to_string(),
            json!({}),
            None,
        );

        assert!(
            source.anthropic_rectifiers_enabled(ForwarderAnthropicRectifierGateInput {
                app_type: &AppType::Claude,
                provider: &claude_auth_provider,
            },)
        );
        assert!(
            !source.anthropic_rectifiers_enabled(ForwarderAnthropicRectifierGateInput {
                app_type: &AppType::Codex,
                provider: &claude_auth_provider,
            },)
        );
        assert!(
            source.anthropic_rectifiers_enabled(ForwarderAnthropicRectifierGateInput {
                app_type: &AppType::Claude,
                provider: &default_claude_provider,
            },)
        );
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_projects_success_switch_target() {
        let provider = Provider::with_id(
            "provider-b".to_string(),
            "Provider B".to_string(),
            json!({}),
            None,
        );
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        assert_eq!(
            source
                .record_success_status(ForwarderSuccessStatusInput {
                    current_provider_id_at_start: "provider-b",
                    provider: &provider,
                })
                .await,
            None
        );

        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );
        let target = source
            .record_success_status(ForwarderSuccessStatusInput {
                current_provider_id_at_start: "provider-a",
                provider: &provider,
            })
            .await
            .expect("alternate provider success should schedule switch target");

        assert_eq!(
            target,
            ForwarderFailoverSwitchTarget {
                provider_id: "provider-b".to_string(),
                provider_name: "Provider B".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_current_provider_from_provider() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );

        source
            .record_current_provider(ForwarderCurrentProviderInput {
                provider: &provider,
            })
            .await;

        let status = source.status();
        let status = status.read().await;
        assert_eq!(status.current_provider_id.as_deref(), Some("provider-a"));
        assert_eq!(status.current_provider.as_deref(), Some("Provider A"));
    }

    #[test]
    fn forwarder_runtime_state_source_classifies_rectifier_retry_failover() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        match source.rectifier_retry_failure_decision(&ProxyError::Timeout(
            "upstream timed out".to_string(),
        )) {
            ForwarderRectifierRetryFailureDecision::ProviderFailure => {}
            ForwarderRectifierRetryFailureDecision::ClientFailure => {
                panic!("timeout should fail over to the next provider")
            }
        }

        match source.rectifier_retry_failure_decision(&ProxyError::UpstreamError {
            status: 502,
            body: Some("bad gateway".to_string()),
        }) {
            ForwarderRectifierRetryFailureDecision::ProviderFailure => {}
            ForwarderRectifierRetryFailureDecision::ClientFailure => {
                panic!("5xx upstream error should fail over to the next provider")
            }
        }

        match source.rectifier_retry_failure_decision(&ProxyError::UpstreamError {
            status: 400,
            body: Some("invalid request".to_string()),
        }) {
            ForwarderRectifierRetryFailureDecision::ProviderFailure => {
                panic!("client 400 should not fail over after rectifier retry")
            }
            ForwarderRectifierRetryFailureDecision::ClientFailure => {}
        }
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_terminal_statuses() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        source.record_no_available_provider_status().await;
        {
            let status = source.status();
            let status = status.read().await;
            assert_eq!(status.failed_requests, 1);
            assert_eq!(
                status.last_error.as_deref(),
                Some("所有供应商暂时不可用（熔断器限制）")
            );
        }

        source.record_terminal_failure_status().await;
        let status = source.status();
        let status = status.read().await;
        assert_eq!(status.failed_requests, 2);
        assert_eq!(status.last_error.as_deref(), Some("所有供应商都失败"));
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_request_started_timestamp() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        source
            .record_request_started(ForwarderRequestStartedInput {
                request_id: "req-1",
                app_type: "claude",
            })
            .await;

        let status = source.status();
        let status = status.read().await;
        assert_eq!(status.total_requests, 1);
        assert!(status.last_request_at.is_some());
    }

    #[test]
    fn forwarder_runtime_state_source_generates_request_ids() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        let request_id = source.next_request_id();

        Uuid::parse_str(&request_id).expect("request id should be a UUID");
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_forward_error_status() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );

        let timeout_error = ProxyError::Timeout("upstream timed out".to_string());
        source
            .record_forward_error_status(ForwarderForwardErrorStatusInput {
                error: &timeout_error,
            })
            .await;

        let status = source.status();
        let status = status.read().await;
        assert_eq!(status.failed_requests, 1);
        assert_eq!(
            status.last_error.as_deref(),
            Some("超时: upstream timed out")
        );
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_provider_failure_from_provider() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );
        let provider = Provider::with_id("relay".to_string(), "Relay".to_string(), json!({}), None);

        let timeout_error = ProxyError::Timeout("upstream timed out".to_string());
        source
            .record_provider_failure(ForwarderProviderFailureInput {
                provider: &provider,
                error: &timeout_error,
            })
            .await;
        {
            let status = source.status();
            let status = status.read().await;
            assert_eq!(
                status.last_error.as_deref(),
                Some("Provider Relay 失败: 超时: upstream timed out")
            );
        }

        let upstream_error = ProxyError::UpstreamError {
            status: 502,
            body: Some("bad gateway".to_string()),
        };
        source
            .record_provider_rectifier_retry_failure(ForwarderProviderRectifierRetryFailureInput {
                provider: &provider,
                kind: ForwarderRectifierRetryKind::ThinkingBudget,
                error: &upstream_error,
            })
            .await;
        let status = source.status();
        let status = status.read().await;
        assert_eq!(
            status.last_error.as_deref(),
            Some(
                "Provider Relay budget 整流重试失败: 上游错误 (状态码 502): Some(\"bad gateway\")"
            )
        );
    }

    fn runtime_route_selection(provider_id: &str, channel_id: &str) -> RouteSelection {
        let provider = ProviderSpec {
            id: provider_id.to_string(),
            name: "Relay".to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        };
        let channel = ChannelSpec {
            id: channel_id.to_string(),
            provider_id: provider_id.to_string(),
            app: AppKind::Claude,
            name: "Relay A".to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: "https://relay.example.com/v1".to_string(),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::OpenAiResponses,
            auth_profile: None,
            models: Vec::new(),
            groups: vec!["default".to_string()],
            priority: 100,
            weight: 50,
            retry_policy: RetryPolicy::default(),
            health_policy: ChannelHealthPolicy::default(),
            overrides: ChannelOverrides::default(),
            tags: Vec::new(),
            metadata: json!({}),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        };
        let model_route = ModelRoute {
            public_model: "public-sonnet".to_string(),
            upstream_model: "upstream-sonnet".to_string(),
            capabilities: ModelCapabilities::default(),
            pricing_model: Some("sonnet-price".to_string()),
            request_overrides: json!({}),
            response_overrides: json!({}),
        };

        RouteSelection {
            provider,
            channel,
            model_route: Some(model_route),
            inbound_interface: InterfaceKind::AnthropicMessages,
            outbound_interface: InterfaceKind::OpenAiResponses,
        }
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_emits_attempt_phase_events() {
        let provider = Provider::with_id("relay".to_string(), "Relay".to_string(), json!({}), None);
        let selection = runtime_route_selection(&provider.id, "channel-a");
        let attempt = ForwardAttempt::from_core_selection(&AppType::Claude, &provider, &selection);
        let events = Arc::new(ProxyEventBus::default());
        let mut subscriber = events.subscribe();
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            events,
        );

        source.record_attempt_started(ForwarderAttemptStartedInput {
            request_id: "req-1",
            app_type: "claude",
            attempt: &attempt,
        });
        let started = subscriber.recv().await.expect("started event");
        assert_eq!(started.event, "channel_attempt");
        assert_eq!(started.payload["requestId"], "req-1");
        assert_eq!(started.payload["channelId"], "channel-a");
        assert_eq!(started.payload["pricingModel"], "sonnet-price");
        assert!(started.payload.get("error").is_none());

        source
            .record_successful_attempt(ForwarderSuccessfulAttemptInput {
                request_id: "req-1",
                app_type: "claude",
                attempt: &attempt,
            })
            .await;
        let succeeded = subscriber.recv().await.expect("succeeded event");
        assert_eq!(succeeded.event, "channel_succeeded");
        assert_eq!(succeeded.payload["channelId"], "channel-a");
        assert_eq!(succeeded.payload["pricingModel"], "sonnet-price");
        assert!(succeeded.payload.get("error").is_none());
        let route_selected = subscriber.recv().await.expect("route selected event");
        assert_eq!(route_selected.event, "route_selected");

        let forward_error = ProxyError::ForwardFailed("upstream failed".to_string());
        source.record_failed_attempt(ForwarderAttemptFailedInput {
            request_id: "req-1",
            app_type: "claude",
            attempt: &attempt,
            error: &forward_error,
        });
        let failed = subscriber.recv().await.expect("failed event");
        assert_eq!(failed.event, "channel_failed");
        assert_eq!(failed.payload["channelId"], "channel-a");
        assert_eq!(failed.payload["pricingModel"], "sonnet-price");
        assert_eq!(failed.payload["error"], "请求转发失败: upstream failed");
    }

    #[tokio::test]
    async fn forwarder_runtime_state_source_records_active_route_target_event() {
        let provider = Provider::with_id("relay".to_string(), "Relay".to_string(), json!({}), None);
        let selection = runtime_route_selection(&provider.id, "channel-a");
        let attempt = ForwardAttempt::from_core_selection(&AppType::Claude, &provider, &selection);
        let current_providers = Arc::new(RwLock::new(HashMap::new()));
        let events = Arc::new(ProxyEventBus::default());
        let mut subscriber = events.subscribe();
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            current_providers.clone(),
            events,
        );

        source
            .record_successful_attempt(ForwarderSuccessfulAttemptInput {
                request_id: "req-route",
                app_type: "claude",
                attempt: &attempt,
            })
            .await;

        let current_providers = current_providers.read().await;
        let target = current_providers
            .get("claude")
            .expect("active route target");
        assert_eq!(target.provider_id, "relay");
        assert_eq!(target.channel_id.as_deref(), Some("channel-a"));
        assert_eq!(target.interface_kind.as_deref(), Some("openai_responses"));
        assert_eq!(target.upstream_model.as_deref(), Some("upstream-sonnet"));
        assert_eq!(target.pricing_model.as_deref(), Some("sonnet-price"));

        let succeeded = subscriber.recv().await.expect("succeeded event");
        assert_eq!(succeeded.event, "channel_succeeded");
        assert_eq!(succeeded.payload["channelId"], "channel-a");

        let route_event = subscriber.recv().await.expect("route selected event");
        assert_eq!(route_event.event, "route_selected");
        assert_eq!(route_event.payload["requestId"], "req-route");
        assert_eq!(route_event.payload["providerId"], "relay");
        assert_eq!(route_event.payload["channelId"], "channel-a");
        assert_eq!(route_event.payload["interfaceKind"], "openai_responses");
        assert_eq!(route_event.payload["pricingModel"], "sonnet-price");
    }

    #[tokio::test]
    async fn forwarder_response_source_projects_upstream_error_response() {
        let source = CcSwitchForwarderResponseSource;
        let response = ProxyResponse::buffered(
            http::StatusCode::BAD_REQUEST,
            HeaderMap::new(),
            Bytes::from_static(br#"{"error":"bad request"}"#),
        );

        let error = match source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response,
                request_is_streaming: false,
                non_streaming_timeout: std::time::Duration::from_secs(0),
                streaming_first_byte_timeout: std::time::Duration::from_secs(0),
            })
            .await
        {
            Ok(_) => panic!("expected upstream error"),
            Err(error) => error,
        };

        match error {
            ProxyError::UpstreamError { status, body } => {
                assert_eq!(status, 400);
                assert_eq!(body.as_deref(), Some(r#"{"error":"bad request"}"#));
            }
            other => panic!("expected upstream error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn forwarder_response_source_finalizes_success_and_upstream_error() {
        let source = CcSwitchForwarderResponseSource;
        let success = ProxyResponse::buffered(
            http::StatusCode::OK,
            HeaderMap::new(),
            Bytes::from_static(b"{\"ok\":true}"),
        );
        let success = source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response: success,
                request_is_streaming: false,
                non_streaming_timeout: std::time::Duration::from_secs(0),
                streaming_first_byte_timeout: std::time::Duration::from_secs(0),
            })
            .await
            .expect("success response");
        assert_eq!(success.status(), http::StatusCode::OK);
        assert_eq!(
            success.bytes().await.expect("success body"),
            Bytes::from_static(b"{\"ok\":true}")
        );

        let failure = ProxyResponse::buffered(
            http::StatusCode::BAD_REQUEST,
            HeaderMap::new(),
            Bytes::from_static(b"bad request"),
        );
        let error = match source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response: failure,
                request_is_streaming: false,
                non_streaming_timeout: std::time::Duration::from_secs(0),
                streaming_first_byte_timeout: std::time::Duration::from_secs(0),
            })
            .await
        {
            Ok(_) => panic!("expected upstream error"),
            Err(error) => error,
        };

        match error {
            ProxyError::UpstreamError { status, body } => {
                assert_eq!(status, 400);
                assert_eq!(body.as_deref(), Some("bad request"));
            }
            other => panic!("expected upstream error, got {other:?}"),
        }
    }

    struct StaticCopilotModelsSource {
        endpoint: Option<String>,
        models: Option<Vec<CopilotModel>>,
    }

    impl CoreManagedAccountRuntimeSource for StaticCopilotModelsSource {
        type Error = ProxyError;

        fn resolve_copilot_auth<'a>(
            &'a self,
            _account_id: Option<&'a str>,
            _runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<ProviderAuthInfo, ProxyError>> {
            Box::pin(async move {
                Err(ProxyError::AuthError(
                    "test source does not resolve auth".to_string(),
                ))
            })
        }

        fn resolve_codex_oauth<'a>(
            &'a self,
            _account_id: Option<String>,
            _runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<(ProviderAuthInfo, Option<String>), ProxyError>> {
            Box::pin(async move {
                Err(ProxyError::AuthError(
                    "test source does not resolve oauth".to_string(),
                ))
            })
        }

        fn resolve_copilot_api_endpoint<'a>(
            &'a self,
            _account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move { self.endpoint.clone() })
        }

        fn fetch_copilot_live_models<'a>(
            &'a self,
            _account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Option<Vec<CopilotModel>>, String>> {
            Box::pin(async move { Ok(self.models.clone()) })
        }

        fn resolve_copilot_model_vendor<'a>(
            &'a self,
            _account_id: Option<&'a str>,
            _model_id: &'a str,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move { None })
        }
    }

    struct StaticManagedAuthResolutionSource;

    impl CoreManagedAccountRuntimeSource for StaticManagedAuthResolutionSource {
        type Error = ProxyError;

        fn resolve_copilot_auth<'a>(
            &'a self,
            account_id: Option<&'a str>,
            runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<ProviderAuthInfo, ProxyError>> {
            Box::pin(async move {
                Ok(ProviderAuthInfo::new(
                    format!("copilot-token:{}", account_id.unwrap_or("default")),
                    runtime.provider_auth_strategy(),
                ))
            })
        }

        fn resolve_codex_oauth<'a>(
            &'a self,
            account_id: Option<String>,
            runtime: ManagedAccountAuthRuntime,
        ) -> BoxFuture<'a, Result<(ProviderAuthInfo, Option<String>), ProxyError>> {
            Box::pin(async move {
                let resolved_account_id = account_id.unwrap_or_else(|| "codex-default".to_string());
                Ok((
                    ProviderAuthInfo::new(
                        format!("codex-token:{resolved_account_id}"),
                        runtime.provider_auth_strategy(),
                    ),
                    Some(resolved_account_id),
                ))
            })
        }

        fn resolve_copilot_api_endpoint<'a>(
            &'a self,
            _account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move { None })
        }

        fn fetch_copilot_live_models<'a>(
            &'a self,
            _account_id: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Option<Vec<CopilotModel>>, String>> {
            Box::pin(async move { Ok(None) })
        }

        fn resolve_copilot_model_vendor<'a>(
            &'a self,
            _account_id: Option<&'a str>,
            _model_id: &'a str,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async move { None })
        }
    }

    fn provider_with_managed_account_binding(auth_provider: &str, account_id: &str) -> Provider {
        let mut provider = Provider::with_id(
            format!("{auth_provider}-provider"),
            "Managed Account Provider".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some(auth_provider.to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some(auth_provider.to_string()),
                account_id: Some(account_id.to_string()),
            }),
            ..ProviderMeta::default()
        });
        provider
    }

    #[tokio::test]
    async fn managed_account_runtime_source_resolves_provider_account_bindings() {
        let source = StaticManagedAuthResolutionSource;
        let copilot_provider =
            provider_with_managed_account_binding("github_copilot", "copilot-acct");
        let codex_provider = provider_with_managed_account_binding("codex_oauth", "codex-acct");

        let copilot = source
            .resolve_auth_for_provider(ManagedAccountAuthForProviderInput {
                auth_provider: &copilot_provider,
                auth: ProviderAuthInfo::new(
                    "PROXY_MANAGED".to_string(),
                    ProviderAuthStrategy::GitHubCopilot,
                ),
            })
            .await
            .expect("copilot managed auth");
        assert_eq!(copilot.auth.api_key, "copilot-token:copilot-acct");
        assert_eq!(copilot.auth.strategy, ProviderAuthStrategy::GitHubCopilot);
        assert_eq!(copilot.codex_oauth_account_id, None);
        assert!(!copilot.should_send_codex_oauth_session_headers);

        let codex = source
            .resolve_auth_for_provider(ManagedAccountAuthForProviderInput {
                auth_provider: &codex_provider,
                auth: ProviderAuthInfo::new(
                    "PROXY_MANAGED".to_string(),
                    ProviderAuthStrategy::CodexOAuth,
                ),
            })
            .await
            .expect("codex managed auth");
        assert_eq!(codex.auth.api_key, "codex-token:codex-acct");
        assert_eq!(codex.auth.strategy, ProviderAuthStrategy::CodexOAuth);
        assert_eq!(codex.codex_oauth_account_id.as_deref(), Some("codex-acct"));
        assert!(codex.should_send_codex_oauth_session_headers);
    }

    #[tokio::test]
    async fn managed_account_runtime_source_gates_copilot_live_model_by_adapter() {
        let source = StaticCopilotModelsSource {
            endpoint: None,
            models: Some(vec![CopilotModel {
                id: "claude-sonnet-4.6".to_string(),
                name: "Claude Sonnet 4.6".to_string(),
                vendor: "Anthropic".to_string(),
                model_picker_enabled: true,
            }]),
        };
        let provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({}),
            None,
        );
        let mut body = json!({ "model": "claude-sonnet-4-6" });

        source
            .apply_copilot_live_model_for_adapter(ManagedAccountAdapterCopilotLiveModelInput {
                auth_provider: &provider,
                body: &mut body,
                is_copilot: false,
            })
            .await;

        assert_eq!(body["model"], "claude-sonnet-4-6");

        source
            .apply_copilot_live_model_for_adapter(ManagedAccountAdapterCopilotLiveModelInput {
                auth_provider: &provider,
                body: &mut body,
                is_copilot: true,
            })
            .await;

        assert_eq!(body["model"], "claude-sonnet-4.6");
    }

    #[tokio::test]
    async fn managed_account_runtime_source_applies_copilot_dynamic_base_url() {
        let source = StaticCopilotModelsSource {
            endpoint: Some("https://api.enterprise.githubcopilot.com".to_string()),
            models: None,
        };
        let provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({}),
            None,
        );
        let mut base_url = "https://api.githubcopilot.com".to_string();

        source
            .apply_copilot_dynamic_base_url_for_provider(
                ManagedAccountApplyCopilotDynamicBaseUrlInput {
                    auth_provider: &provider,
                    base_url: &mut base_url,
                    is_copilot: true,
                    is_full_url: false,
                },
            )
            .await;

        assert_eq!(base_url, "https://api.enterprise.githubcopilot.com");
    }

    #[tokio::test]
    async fn managed_account_runtime_source_gates_claude_api_format_by_adapter() {
        let source = StaticCopilotModelsSource {
            endpoint: None,
            models: None,
        };
        let provider = Provider::with_id(
            "claude".to_string(),
            "Claude".to_string(),
            json!({
                "api_format": "openai_chat"
            }),
            None,
        );
        let body = json!({ "model": "claude-sonnet-4" });

        assert_eq!(
            source
                .resolve_claude_api_format_for_adapter(ManagedAccountAdapterClaudeApiFormatInput {
                    auth_provider: &provider,
                    body: &body,
                    is_copilot: false,
                    is_claude_adapter: false,
                })
                .await,
            None
        );
        assert_eq!(
            source
                .resolve_claude_api_format_for_adapter(ManagedAccountAdapterClaudeApiFormatInput {
                    auth_provider: &provider,
                    body: &body,
                    is_copilot: false,
                    is_claude_adapter: true,
                })
                .await
                .as_deref(),
            Some("openai_chat")
        );
    }

    #[test]
    fn forwarder_request_source_plans_signature_rectifier_retry() {
        let source = default_forwarder_request_source();
        let mut body = json!({
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "private", "signature": "bad"},
                    {"type": "text", "text": "visible", "signature": "bad"}
                ]
            }]
        });
        let error = ProxyError::UpstreamError {
            status: 400,
            body: Some("invalid signature in thinking block".to_string()),
        };

        let plan =
            source.thinking_signature_rectifier_plan(ForwarderThinkingSignatureRectifierInput {
                app: "claude",
                body: &mut body,
                error: &error,
                already_retried: false,
                config: &RectifierConfig::default(),
            });

        assert_eq!(plan, ForwarderRequestRectifierPlan::Retry);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0].get("signature").is_none());
    }

    #[test]
    fn forwarder_request_source_plans_budget_rectifier_retry() {
        let source = default_forwarder_request_source();
        let mut body = json!({
            "messages": [{"role": "user", "content": "Hello"}],
            "max_tokens": 1024,
            "thinking": {"type": "enabled", "budget_tokens": 512}
        });
        let error = ProxyError::UpstreamError {
            status: 400,
            body: Some(
                "thinking.budget_tokens: Input should be greater than or equal to 1024".to_string(),
            ),
        };

        let plan = source.thinking_budget_rectifier_plan(ForwarderThinkingBudgetRectifierInput {
            app: "claude",
            body: &mut body,
            error: &error,
            already_retried: false,
            config: &RectifierConfig::default(),
        });

        assert_eq!(plan, ForwarderRequestRectifierPlan::Retry);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 32000);
        assert_eq!(body["max_tokens"], 64000);
    }

    #[test]
    fn proxy_config_adapter_preserves_management_contracts() {
        use crate::proxy_core::api::auth::{
            resolve_management_auth_decision, ManagementAuthDecision,
        };

        let proxy_config = serde_json::to_value(ProxyConfig::default()).expect("proxy config");
        assert_eq!(
            proxy_config.get("listen_address").and_then(Value::as_str),
            Some("127.0.0.1")
        );
        assert_eq!(
            proxy_config
                .get("streaming_first_byte_timeout")
                .and_then(Value::as_u64),
            Some(60)
        );

        let app_config = AppProxyConfig {
            app_type: "claude".to_string(),
            enabled: true,
            auto_failover_enabled: true,
            max_retries: 3,
            streaming_first_byte_timeout: 60,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
            circuit_failure_threshold: 4,
            circuit_success_threshold: 2,
            circuit_timeout_seconds: 60,
            circuit_error_rate_threshold: 0.6,
            circuit_min_requests: 10,
        };
        let takeover_disabled_app_config = proxy_app_config_with_enabled(app_config.clone(), false);
        assert!(!takeover_disabled_app_config.enabled);
        assert!(takeover_disabled_app_config.auto_failover_enabled);
        let takeover_enabled_app_config =
            proxy_app_config_with_enabled(takeover_disabled_app_config, true);
        assert!(takeover_enabled_app_config.enabled);
        assert!(takeover_enabled_app_config.auto_failover_enabled);
        let default_proxy_config = ProxyConfig::default();
        let loopback_auth = resolve_management_auth_decision(
            &default_proxy_config.listen_address,
            default_proxy_config.management_auth_token.as_deref(),
            None,
        )
        .expect("loopback auth decision");
        assert_eq!(loopback_auth, ManagementAuthDecision::AllowWithoutToken);
        let mut public_proxy_config = ProxyConfig {
            listen_address: "0.0.0.0".to_string(),
            ..ProxyConfig::default()
        };
        assert_eq!(
            resolve_management_auth_decision(
                &public_proxy_config.listen_address,
                public_proxy_config.management_auth_token.as_deref(),
                None,
            )
            .unwrap_err(),
            ManagementAuthError::RequiredTokenMissing
        );
        assert_eq!(
            resolve_management_auth_decision(
                &public_proxy_config.listen_address,
                public_proxy_config.management_auth_token.as_deref(),
                Some("env-token"),
            )
            .expect("env fallback token"),
            ManagementAuthDecision::RequireToken("env-token".to_string())
        );
        public_proxy_config.management_auth_token = Some(" config-token ".to_string());
        assert_eq!(
            resolve_management_auth_decision(
                &public_proxy_config.listen_address,
                public_proxy_config.management_auth_token.as_deref(),
                Some("env-token"),
            )
            .expect("configured token"),
            ManagementAuthDecision::RequireToken("config-token".to_string())
        );

        assert_eq!(
            CircuitBreakerConfig::from(&app_config),
            CircuitBreakerConfig::default()
        );
        let mut custom_breaker_app_config = app_config.clone();
        custom_breaker_app_config.circuit_failure_threshold = 7;
        custom_breaker_app_config.circuit_timeout_seconds = 45;
        let projected_breaker_config =
            circuit_breaker_config_from_app_config(Some(&custom_breaker_app_config));
        assert_eq!(projected_breaker_config.failure_threshold, 7);
        assert_eq!(projected_breaker_config.timeout_seconds, 45);
        assert_eq!(
            circuit_breaker_config_from_app_config(None),
            CircuitBreakerConfig::default()
        );
        assert_eq!(
            circuit_failure_threshold_from_app_config(Some(&custom_breaker_app_config), 9,),
            7
        );
        assert_eq!(circuit_failure_threshold_from_app_config(None, 9), 9);
        let enabled_policy = response_runtime_policy_from_app_proxy_config(&app_config);
        assert_eq!(enabled_policy.max_retries, 3);
        assert_eq!(enabled_policy.timeout.non_streaming_timeout, 600);
        assert_eq!(enabled_policy.timeout.streaming.first_byte_timeout, 60);
        assert_eq!(enabled_policy.timeout.streaming.idle_timeout, 120);
        assert_eq!(
            forwarder_runtime_options_from_app_proxy_config(&app_config),
            ForwarderRuntimeOptions {
                non_streaming_timeout: 600,
                streaming_first_byte_timeout: 60,
                streaming_idle_timeout: 120,
                max_retries: 3,
            }
        );
        let forwarder_config = forwarder_runtime_config_from_sources(
            &app_config,
            RectifierConfig {
                request_media_fallback: false,
                ..RectifierConfig::default()
            },
            OptimizerConfig {
                enabled: true,
                cache_ttl: "2h".to_string(),
                ..OptimizerConfig::default()
            },
            CopilotOptimizerConfig {
                warmup_model: "gpt-5".to_string(),
                ..CopilotOptimizerConfig::default()
            },
        );
        assert_eq!(
            forwarder_config.options,
            ForwarderRuntimeOptions {
                non_streaming_timeout: 600,
                streaming_first_byte_timeout: 60,
                streaming_idle_timeout: 120,
                max_retries: 3,
            }
        );
        assert!(!forwarder_config.rectifier.request_media_fallback);
        assert!(forwarder_config.optimizer.enabled);
        assert_eq!(forwarder_config.optimizer.cache_ttl, "2h");
        assert_eq!(forwarder_config.copilot_optimizer.warmup_model, "gpt-5");
        let app_summary = crate::proxy_core::api::ports::AppSummaryConfig::new(
            app_config.enabled,
            app_config.auto_failover_enabled,
        );
        assert!(app_summary.enabled);
        assert!(app_summary.auto_failover_enabled);

        let mut disabled_app_config = app_config.clone();
        disabled_app_config.auto_failover_enabled = false;
        let disabled_policy = response_runtime_policy_from_app_proxy_config(&disabled_app_config);
        assert_eq!(disabled_policy.max_retries, 0);
        assert_eq!(disabled_policy.timeout, ResponseTimeoutConfig::default());
        assert_eq!(
            forwarder_runtime_options_from_app_proxy_config(&disabled_app_config),
            ForwarderRuntimeOptions {
                non_streaming_timeout: ResponseTimeoutConfig::default().non_streaming_timeout,
                streaming_first_byte_timeout: ResponseTimeoutConfig::default()
                    .streaming
                    .first_byte_timeout,
                streaming_idle_timeout: ResponseTimeoutConfig::default().streaming.idle_timeout,
                max_retries: 0,
            }
        );
        assert_eq!(
            serde_json::to_value(crate::proxy_core::api::ports::GlobalProxyConfig {
                proxy_enabled: true,
                listen_address: "127.0.0.1".to_string(),
                listen_port: crate::proxy_core::api::ports::DEFAULT_PROXY_LISTEN_PORT,
                enable_logging: true,
            })
            .expect("global proxy config")
            .get("proxyEnabled")
            .and_then(Value::as_bool),
            Some(true)
        );
        assert!(
            !crate::proxy_core::api::config::proxy_runtime_config_from_proxy_config(
                ProxyConfig::default(),
                false
            )
            .privacy_filter_enabled
        );
    }

    #[test]
    fn auth_adapter_projects_cc_switch_provider_config_source() {
        let auth_profile =
            crate::proxy_core::api::domain::AuthProfileRef::new("provider:claude:anthropic-main");
        let auth =
            crate::proxy::host::cc_switch::auth_provider::auth_info_from_cc_switch_provider_config(
                Some(&auth_profile),
            );
        assert!(auth.headers.is_empty());
        assert_eq!(
            auth.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(auth.metadata["source"], json!("cc_switch_provider_config"));

        let fallback =
            crate::proxy::host::cc_switch::auth_provider::auth_info_from_cc_switch_provider_config(
                None,
            );
        assert!(fallback.account_ref.is_none());
        assert_eq!(
            fallback.metadata["source"],
            json!("cc_switch_provider_config")
        );
    }

    #[test]
    fn auth_adapter_projects_cc_switch_route_context_source() {
        let provider = ProviderSpec {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        };
        let channel = channel_spec_from_input(ChannelSpecInput {
            id: "channel-a".to_string(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Channel A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            auth_profile_ref: Some("provider:claude:anthropic-main".to_string()),
            models: Vec::new(),
            groups: Vec::new(),
            priority: 0,
            weight: 100,
            retry_policy: Value::Object(Default::default()),
            health_policy: Value::Object(Default::default()),
            header_overrides: Value::Object(Default::default()),
            param_overrides: Value::Object(Default::default()),
            status_code_mapping: Value::Array(Vec::new()),
            tags: Vec::new(),
            metadata: Value::Object(Default::default()),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        });

        let auth =
            crate::proxy::host::cc_switch::auth_provider::auth_info_from_cc_switch_route_context(
                &AppKind::Claude,
                &provider,
                &channel,
            );

        assert!(auth.headers.is_empty());
        assert_eq!(
            auth.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(auth.metadata["source"], json!("cc_switch_provider_config"));
        assert_eq!(auth.metadata["app"], json!("claude"));
        assert_eq!(auth.metadata["providerId"], json!("provider-a"));
        assert_eq!(auth.metadata["channelId"], json!("channel-a"));
    }

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
        let current_provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        let selected_current =
            select_current_provider_ids_from_router_source("claude", Some(current_provider))
                .expect("selected current provider id");
        assert_eq!(selected_current, vec!["provider-a"]);
        assert!(matches!(
            select_current_provider_ids_from_router_source("claude", None),
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
    fn proxy_event_adapter_projects_event_stream_contracts() {
        let bus = ProxyEventBus::default();

        assert_eq!(
            crate::proxy_core::api::events::PROXY_OFFICIAL_WARNING_EVENT,
            "proxy-official-warning"
        );
        assert_eq!(
            crate::proxy_core::api::events::PROVIDER_SWITCHED_EVENT,
            "provider-switched"
        );
        assert_eq!(
            crate::proxy_core::api::events::REQUEST_STARTED_EVENT,
            "request_started"
        );
        assert_eq!(
            crate::proxy_core::api::events::SERVER_STARTED_EVENT,
            "server_started"
        );
        assert_eq!(
            crate::proxy_core::api::events::SERVER_STOPPED_EVENT,
            "server_stopped"
        );
        let official_warning = bus.emit_core_event(
            crate::proxy_core::api::events::proxy_official_warning_event(
                "claude",
                "Official Claude",
            ),
        );
        assert_eq!(official_warning.event, "proxy-official-warning");
        assert_eq!(
            official_warning.payload,
            json!({
                "appType": "claude",
                "providerName": "Official Claude",
            })
        );
        let mut provider = Provider::with_id(
            "official-codex".to_string(),
            "Official Codex".to_string(),
            json!({}),
            None,
        );
        provider.category = Some("official".to_string());
        assert!(
            crate::proxy_core::api::ports::provider_category_is_official(
                provider.category.as_deref()
            )
        );
        assert!(
            crate::proxy_core::api::ports::should_emit_proxy_official_warning_for_provider_category(
                provider.category.as_deref()
            )
        );
        assert!(
            crate::proxy_core::api::ports::should_reapply_codex_official_live_for_provider_category(
                provider.category.as_deref()
            )
        );
        let official_warning_from_core = bus.emit_core_event(
            crate::proxy_core::api::events::proxy_official_warning_event("codex", &provider.name),
        );
        assert_eq!(official_warning_from_core.event, "proxy-official-warning");
        assert_eq!(official_warning_from_core.payload["appType"], "codex");
        assert_eq!(
            official_warning_from_core.payload["providerName"],
            "Official Codex"
        );
        provider.category = Some("custom".to_string());
        assert!(
            !crate::proxy_core::api::ports::provider_category_is_official(
                provider.category.as_deref()
            )
        );
        assert!(
            !crate::proxy_core::api::ports::should_emit_proxy_official_warning_for_provider_category(
                provider.category.as_deref()
            )
        );
        assert!(
            !crate::proxy_core::api::ports::should_reapply_codex_official_live_for_provider_category(
                provider.category.as_deref()
            )
        );
        provider.category = None;
        assert!(
            !crate::proxy_core::api::ports::provider_category_is_official(
                provider.category.as_deref()
            )
        );
        assert!(
            !crate::proxy_core::api::ports::should_emit_proxy_official_warning_for_provider_category(
                provider.category.as_deref()
            )
        );
        assert!(
            !crate::proxy_core::api::ports::should_reapply_codex_official_live_for_provider_category(
                provider.category.as_deref()
            )
        );
        let provider_switched = bus.emit_core_event(
            crate::proxy_core::api::events::provider_switched_failover_event(
                "claude",
                "provider-1",
            ),
        );
        assert_eq!(provider_switched.event, "provider-switched");
        assert_eq!(provider_switched.payload["source"], "failover");
        let provider_switched_enabled = bus.emit_core_event(
            crate::proxy_core::api::events::provider_switched_failover_enabled_event(
                "claude",
                "provider-1",
            ),
        );
        assert_eq!(provider_switched_enabled.event, "provider-switched");
        assert_eq!(
            provider_switched_enabled.payload["source"],
            "failoverEnabled"
        );
        let server_started = bus.emit_core_event(server_started_event("127.0.0.1", 15721));
        assert_eq!(server_started.event, "server_started");
        assert_eq!(
            server_started.payload,
            json!({"address": "127.0.0.1", "port": 15721})
        );
        let server_stopped = bus.emit_core_event(server_stopped_event());
        assert_eq!(server_stopped.event, "server_stopped");
        assert!(server_stopped
            .payload
            .as_object()
            .is_some_and(|object| object.is_empty()));

        let envelope = ProxyEventEnvelope::new(
            42,
            "request_started",
            "2026-06-20T00:00:00Z",
            json!({"provider": "relay-a"}),
        );
        let spec = envelope.to_sse_spec();

        assert_eq!(spec.id, "42");
        assert_eq!(spec.event, "request_started");
        assert!(spec.data.contains("\"provider\":\"relay-a\""));

        let request_started = bus.emit_core_event(request_started_event("req-start", "claude"));
        assert_eq!(request_started.event, "request_started");
        assert_eq!(request_started.payload["requestId"], "req-start");
        assert_eq!(request_started.payload["appType"], "claude");

        let message = bus.emit_core_event(ProxyCoreEvent {
            event_type: crate::proxy_core::api::events::ProxyCoreEventType::RouteSelected,
            request_id: Some("req-1".to_string()),
            channel_id: Some("channel-a".to_string()),
            payload: json!({"attemptCount": 2}),
        });
        assert_eq!(message.event, "route_selected");
        assert_eq!(message.payload["requestId"], "req-1");
        assert_eq!(message.payload["channelId"], "channel-a");
        assert_eq!(message.payload["attemptCount"], 2);

        let route_provider = Provider::with_id(
            "provider-1".to_string(),
            "Relay Provider".to_string(),
            json!({}),
            None,
        );
        let route_attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &route_provider,
            ChannelRouteCandidate {
                channel_id: "channel-a".to_string(),
                provider_id: route_provider.id.clone(),
                channel_name: "Relay A".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "openai_responses".to_string(),
                public_model: Some("public-sonnet".to_string()),
                upstream_model: Some("upstream-sonnet".to_string()),
                route_group: "default".to_string(),
                priority: 100,
                weight: 50,
                source_kind: "manual".to_string(),
            },
        );
        let route_message = bus.emit_core_event(route_selected_event(
            attempt_event_payload_input_from_forward_attempt(
                "req-route",
                "claude",
                &route_attempt,
                None,
            ),
        ));
        assert_eq!(route_message.event, "route_selected");
        assert_eq!(route_message.payload["requestId"], "req-route");
        assert_eq!(route_message.payload["providerId"], "provider-1");
        assert_eq!(route_message.payload["channelId"], "channel-a");
        assert_eq!(route_message.payload["interfaceKind"], "openai_responses");
        assert_eq!(route_message.payload["upstreamModel"], "upstream-sonnet");

        let failed_attempt_message = bus.emit_core_event(attempt_event(
            attempt_event_payload_input_from_forward_attempt(
                "req-failed",
                "claude",
                &route_attempt,
                Some("upstream failed"),
            ),
            route_attempt.is_channel(),
            AttemptEventPhase::Failed,
        ));
        assert_eq!(failed_attempt_message.event, "channel_failed");
        assert_eq!(failed_attempt_message.payload["requestId"], "req-failed");
        assert_eq!(failed_attempt_message.payload["channelId"], "channel-a");
        assert_eq!(failed_attempt_message.payload["error"], "upstream failed");

        let emitted = bus.emit_core_event(ProxyCoreEvent {
            event_type: crate::proxy_core::api::events::ProxyCoreEventType::RouteSelected,
            request_id: Some("req-2".to_string()),
            channel_id: Some("channel-b".to_string()),
            payload: json!({"attemptCount": 1}),
        });
        assert_eq!(emitted.event, "route_selected");
        assert_eq!(emitted.payload["requestId"], "req-2");
        assert_eq!(emitted.payload["channelId"], "channel-b");
        assert_eq!(emitted.payload["attemptCount"], 1);
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
            codex_api_key_from_auth_and_config(Some(auth), Some("")).as_deref(),
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
    fn proxy_response_adapter_projects_transport_body_contracts() {
        let response = ProxyCoreResponse::with_body(
            http::StatusCode::CREATED,
            HeaderMap::new(),
            ProxyResponseBody::json(json!({"ok": true})),
        );
        let transport = response
            .into_transport_response()
            .expect("transport response");

        assert_eq!(transport.status, http::StatusCode::CREATED);
        match transport.body {
            ProxyTransportResponseBody::Bytes(body) => {
                assert_eq!(body.as_ref(), br#"{"ok":true}"#);
            }
            _ => panic!("expected buffered bytes transport body"),
        }
    }

    #[test]
    fn handler_context_adapter_projects_runtime_and_model_helpers() {
        let disabled_policy = resolve_response_runtime_policy(false, 3, 600, 60, 120);
        assert_eq!(disabled_policy.max_retries, 0);
        assert_eq!(disabled_policy.timeout, ResponseTimeoutConfig::default());

        let enabled_policy = resolve_response_runtime_policy(true, 3, 600, 60, 120);
        assert_eq!(enabled_policy.max_retries, 3);
        assert_eq!(enabled_policy.timeout.non_streaming_timeout, 600);
        assert_eq!(enabled_policy.timeout.streaming.first_byte_timeout, 60);
        assert_eq!(enabled_policy.timeout.streaming.idle_timeout, 120);

        assert_eq!(
            crate::proxy_core::api::transport::extract_gemini_model_from_path(
                "/v1beta/models/gemini-pro:generateContent"
            )
            .as_deref(),
            Some("gemini-pro")
        );
        assert_eq!(
            crate::proxy_core::api::transport::request_model_for_forward(
                &AppKind::Codex,
                "",
                &json!({"model": " gpt-5 "})
            )
            .as_deref(),
            Some("gpt-5")
        );
        assert_eq!(
            crate::proxy_core::api::transport::request_model_for_forward(
                &AppKind::Claude,
                "",
                &json!({"model": "  "})
            ),
            None
        );
        assert_eq!(
            crate::proxy_core::api::transport::request_model_for_forward(
                &AppKind::Gemini,
                "/v1beta/models/gemini-pro:generateContent",
                &Value::Null,
            )
            .as_deref(),
            Some("gemini-pro")
        );
        assert_eq!(
            crate::proxy_core::api::transforms::claude_api_format_from_metadata(
                &json!({"claudeApiFormat": "openai_chat"}),
                "anthropic"
            ),
            "openai_chat"
        );
        assert_eq!(
            crate::proxy_core::api::transforms::claude_api_format_from_metadata(
                &json!({"apiFormat": " "}),
                "anthropic"
            ),
            "anthropic"
        );
    }

    #[test]
    fn gemini_provider_adapter_projects_auth_settings_and_url_helpers() {
        let settings = json!({
            "env": {
                "GEMINI_API_KEY": " ya29.access-token ",
                "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com/v1beta/"
            }
        });
        assert_eq!(
            extract_gemini_api_key_from_settings(&settings).as_deref(),
            Some("ya29.access-token")
        );
        assert_eq!(
            extract_gemini_base_url_from_settings(&settings).as_deref(),
            Some("https://generativelanguage.googleapis.com/v1beta")
        );
        let env = gemini_env_map_from_settings(&settings).expect("gemini env map");
        assert_eq!(
            env.get("GEMINI_API_KEY").and_then(Value::as_str),
            Some(" ya29.access-token ")
        );
        assert!(gemini_env_map_from_settings(&json!({"env": "invalid"})).is_none());
        assert_eq!(
            crate::proxy_core::api::ports::gemini_env_value_from_env_json(
                &json!({"env": {"A": "B"}})
            ),
            json!({"A": "B"})
        );
        assert_eq!(
            crate::proxy_core::api::ports::gemini_env_value_from_env_json(&json!({})),
            json!({})
        );
        assert_eq!(
            gemini_live_settings_from_env_json_and_config(
                &json!({"env": {"GEMINI_API_KEY": "sk-test"}}),
                json!({"mcpServers": {"server": {}}})
            ),
            json!({
                "env": {"GEMINI_API_KEY": "sk-test"},
                "config": {"mcpServers": {"server": {}}}
            })
        );
        assert_eq!(
            gemini_live_settings_from_env_json_and_config(&json!({}), json!({})),
            json!({"env": {}, "config": {}})
        );
        assert_eq!(
            gemini_live_backup_from_effective_settings(&json!({
                "env": {"GEMINI_API_KEY": "key"},
                "config": {"mcpServers": {"kept-out-of-env-backup": {}}}
            })),
            json!({"env": {"GEMINI_API_KEY": "key"}})
        );
        assert_eq!(
            gemini_live_backup_from_effective_settings(&json!({
                "config": {"mcpServers": {}}
            })),
            json!({"env": {}})
        );
        let provider = Provider::with_id(
            "gemini".to_string(),
            "Gemini".to_string(),
            settings.clone(),
            None,
        );
        assert_eq!(
            extract_gemini_api_key_from_settings(&provider.settings_config).as_deref(),
            Some("ya29.access-token")
        );
        assert_eq!(
            extract_gemini_base_url_from_settings(&provider.settings_config).as_deref(),
            Some("https://generativelanguage.googleapis.com/v1beta")
        );
        assert_eq!(
            crate::proxy_core::api::ports::required_provider_base_url(
                "Gemini",
                extract_gemini_base_url_from_settings(&provider.settings_config)
            )
            .as_deref(),
            Ok("https://generativelanguage.googleapis.com/v1beta")
        );
        assert_eq!(
            crate::proxy_core::api::ports::required_provider_base_url("Gemini", None).unwrap_err(),
            "Gemini Provider 缺少 base_url 配置"
        );
        let live_env = gemini_env_string_map_from_settings(&provider.settings_config);
        assert_eq!(
            live_env.get("GEMINI_API_KEY").map(String::as_str),
            Some(" ya29.access-token ")
        );
        assert_eq!(
            gemini_env_string_map_from_settings(&gemini_env_json_from_map(&live_env)),
            live_env
        );
        crate::gemini_config::validate_gemini_settings_basic(&provider.settings_config)
            .expect("provider Gemini settings should pass basic shape validation");
        let invalid_env_provider = Provider::with_id(
            "gemini-invalid-env".to_string(),
            "Gemini Invalid Env".to_string(),
            json!({"env": "invalid"}),
            None,
        );
        assert!(matches!(
            crate::gemini_config::validate_gemini_settings_basic(
                &invalid_env_provider.settings_config
            ),
            Err(AppError::Localized { key, .. }) if key == "gemini.validation.invalid_env"
        ));
        crate::gemini_config::validate_gemini_settings_strict(&provider.settings_config)
            .expect("provider Gemini settings should be valid for API key mode");
        assert_eq!(
            gemini_live_config_object_from_settings(&json!({"config": {"mcpServers": {}}}))
                .expect("config object")
                .and_then(Value::as_object)
                .map(|obj| obj.contains_key("mcpServers")),
            Some(true)
        );
        assert!(
            gemini_live_config_object_from_settings(&json!({"config": Value::Null}))
                .expect("null config should preserve live file")
                .is_none()
        );
        assert!(matches!(
            gemini_live_config_object_from_settings(&json!({"config": "not-object"})),
            Err(GeminiLiveConfigIssue::InvalidType)
        ));
        assert_eq!(
            gemini_live_settings_to_write(
                Some(json!({
                    "mcpServers": {"existing": {}},
                    "security": {"auth": {"selectedType": "oauth-personal"}}
                })),
                Some(&json!({
                    "security": {"auth": {"selectedType": "api-key"}},
                    "ui": {"theme": "dark"}
                })),
            ),
            Some(json!({
                "mcpServers": {"existing": {}},
                "security": {"auth": {"selectedType": "api-key"}},
                "ui": {"theme": "dark"}
            }))
        );
        assert_eq!(
            gemini_live_settings_to_write(Some(json!({"mcpServers": {}})), None),
            Some(json!({"mcpServers": {}}))
        );
        assert_eq!(
            gemini_live_settings_to_write(None, Some(&json!({"ui": {"theme": "dark"}}))),
            Some(json!({"ui": {"theme": "dark"}}))
        );

        let creds =
            crate::proxy_core::api::auth::parse_gemini_oauth_credentials("ya29.access-token")
                .expect("direct oauth token should parse");
        assert_eq!(creds.access_token, "ya29.access-token");
        assert!(!creds.needs_refresh());
        assert_eq!(
            crate::proxy_core::api::transforms::build_gemini_upstream_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "/v1beta/models/gemini-pro:generateContent",
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent"
        );

        let oauth_headers =
            build_gemini_auth_headers("refresh-token", Some("ya29.access-token"), true)
                .expect("oauth headers");
        assert_eq!(oauth_headers[0].0.as_str(), "authorization");
        assert_eq!(
            oauth_headers[0].1,
            http::HeaderValue::from_static("Bearer ya29.access-token")
        );
        let api_key_headers =
            build_gemini_auth_headers("AIza-api-key", None, false).expect("api key headers");
        assert_eq!(api_key_headers[0].0.as_str(), "x-goog-api-key");
        assert_eq!(
            api_key_headers[0].1,
            http::HeaderValue::from_static("AIza-api-key")
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

    #[test]
    fn proxy_server_adapter_projects_runtime_contracts() {
        assert_eq!(crate::proxy_core::api::logging::srv::STARTED, "SRV-001");
        assert_eq!(crate::proxy_core::api::logging::srv::STOPPED, "SRV-002");
        assert_eq!(crate::proxy_core::api::logging::srv::ACCEPT_ERR, "SRV-005");
        let _shadow_store = GeminiShadowStore::default();
        let stopped = proxy_runtime_status_stopped();
        assert!(!stopped.running);
        assert_eq!(stopped.port, 0);
        assert!(stopped.active_targets.is_empty());
        let info = crate::proxy_core::api::ports::proxy_server_info_from_parts(
            "127.0.0.1",
            15721,
            "2026-06-21T00:00:00Z",
        );
        assert_eq!(info.address, "127.0.0.1");
        assert_eq!(info.port, 15721);
        assert_eq!(info.started_at, "2026-06-21T00:00:00Z");
        let takeover = crate::proxy_core::api::ports::proxy_takeover_status_from_parts(
            true, false, true, false, false,
        );
        assert!(takeover.claude);
        assert!(!takeover.codex);
        assert!(takeover.gemini);
        assert!(!takeover.opencode);
        assert!(!takeover.openclaw);
        let app_config = |app_type: &str, enabled: bool| AppProxyConfig {
            app_type: app_type.to_string(),
            enabled,
            auto_failover_enabled: true,
            max_retries: 3,
            streaming_first_byte_timeout: 60,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
            circuit_failure_threshold: 4,
            circuit_success_threshold: 2,
            circuit_timeout_seconds: 60,
            circuit_error_rate_threshold: 0.6,
            circuit_min_requests: 10,
        };
        let takeover_from_config =
            crate::proxy_core::api::ports::proxy_takeover_status_from_enabled_options(
                Some(app_config("claude", true).enabled),
                None,
                Some(app_config("gemini", true).enabled),
                None,
                None,
            );
        assert!(takeover_from_config.claude);
        assert!(!takeover_from_config.codex);
        assert!(takeover_from_config.gemini);
        assert!(!takeover_from_config.opencode);
        assert!(!takeover_from_config.openclaw);

        assert_eq!(
            crate::proxy_core::api::ports::proxy_live_urls_from_listen_parts("127.0.0.1", 15721),
            Some((
                "http://127.0.0.1:15721".to_string(),
                "http://127.0.0.1:15721/v1".to_string()
            ))
        );
        assert_eq!(
            crate::proxy_core::api::ports::proxy_live_urls_from_listen_parts("0.0.0.0", 15721),
            Some((
                "http://127.0.0.1:15721".to_string(),
                "http://127.0.0.1:15721/v1".to_string()
            ))
        );
        assert_eq!(
            crate::proxy_core::api::ports::proxy_live_urls_from_listen_parts("::", 15721),
            Some((
                "http://[::1]:15721".to_string(),
                "http://[::1]:15721/v1".to_string()
            ))
        );
        assert_eq!(
            crate::proxy_core::api::ports::proxy_live_urls_from_listen_parts("fd00::1", 15721),
            Some((
                "http://[fd00::1]:15721".to_string(),
                "http://[fd00::1]:15721/v1".to_string()
            ))
        );
        assert_eq!(
            crate::proxy_core::api::ports::proxy_live_urls_from_listen_parts("127.0.0.1", 0),
            None
        );

        let target = CurrentRouteTarget {
            app_type: "claude".to_string(),
            provider_id: "provider-a".to_string(),
            provider_name: "Provider A".to_string(),
            channel_id: Some("channel-a".to_string()),
            channel_name: Some("Channel A".to_string()),
            interface_kind: Some("anthropic_messages".to_string()),
            public_model: Some("sonnet-public".to_string()),
            upstream_model: Some("upstream-sonnet".to_string()),
            pricing_model: Some("sonnet-price".to_string()),
        };
        assert_eq!(
            serde_json::to_value(target).expect("serialize target"),
            json!({
                "appType": "claude",
                "providerId": "provider-a",
                "providerName": "Provider A",
                "channelId": "channel-a",
                "channelName": "Channel A",
                "interfaceKind": "anthropic_messages",
                "publicModel": "sonnet-public",
                "upstreamModel": "upstream-sonnet",
                "pricingModel": "sonnet-price"
            })
        );

        let provider_only = current_route_target_from_provider("codex", "provider-b", "Provider B");
        assert_eq!(provider_only.app_type, "codex");
        assert_eq!(provider_only.provider_id, "provider-b");
        assert_eq!(provider_only.provider_name, "Provider B");
        assert!(provider_only.channel_id.is_none());
        assert!(provider_only.interface_kind.is_none());
        assert!(provider_only.pricing_model.is_none());
    }

    #[test]
    fn proxy_switch_policy_adapter_preserves_takeover_state_rules() {
        assert!(!proxy_live_config_owned_by_takeover(false, false));
        assert!(proxy_live_config_owned_by_takeover(true, false));
        assert!(proxy_live_config_owned_by_takeover(false, true));
        assert!(!proxy_switch_should_hot_switch(false, false));
        assert!(proxy_switch_should_hot_switch(true, false));
        assert!(proxy_switch_should_hot_switch(false, true));

        assert!(!proxy_takeover_marked_state_is_reusable(false, false));
        assert!(!proxy_takeover_marked_state_is_reusable(true, false));
        assert!(!proxy_takeover_marked_state_is_reusable(false, true));
        assert!(proxy_takeover_marked_state_is_reusable(true, true));
        assert!(!proxy_takeover_should_restore_existing_backup_before_retakeover(false, false));
        assert!(proxy_takeover_should_restore_existing_backup_before_retakeover(true, false));
        assert!(!proxy_takeover_should_restore_existing_backup_before_retakeover(false, true));
        assert!(!proxy_takeover_should_restore_existing_backup_before_retakeover(true, true));
    }

    #[test]
    fn sanitize_claude_settings_for_live_strips_host_only_fields() {
        let sanitized = sanitize_claude_settings_for_live(&json!({
            "api_format": "anthropic",
            "apiFormat": "openai",
            "openrouter_compat_mode": true,
            "openrouterCompatMode": true,
            "env": {
                "ANTHROPIC_API_KEY": "sk-test"
            },
            "includeCoAuthoredBy": false
        }));

        assert_eq!(
            sanitized,
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                },
                "includeCoAuthoredBy": false
            })
        );
    }

    #[test]
    fn json_subset_helpers_match_and_remove_array_items_once() {
        let target = json!({
            "allowedTools": [
                { "name": "tool-a", "scope": "global" },
                { "name": "tool-b", "scope": "local" },
                { "name": "tool-a", "scope": "project" }
            ],
            "env": {
                "A": "1",
                "B": "2"
            }
        });
        let source = json!({
            "allowedTools": [
                { "name": "tool-a" },
                { "name": "tool-b", "scope": "local" }
            ],
            "env": {
                "A": "1"
            }
        });
        assert!(json_value_is_subset(&target, &source));

        let mut target_arr = target["allowedTools"].as_array().cloned().unwrap();
        let source_arr = source["allowedTools"].as_array().unwrap();
        json_remove_array_items(&mut target_arr, source_arr);
        assert_eq!(
            target_arr,
            vec![json!({ "name": "tool-a", "scope": "project" })]
        );
    }

    #[test]
    fn json_deep_merge_and_remove_preserve_unrelated_fields() {
        let mut target = json!({
            "env": {
                "ANTHROPIC_API_KEY": "sk-test"
            },
            "allowedTools": ["tool-a", "tool-b"],
            "includeCoAuthoredBy": true
        });
        let source = json!({
            "env": {
                "CLAUDE_CODE_USE_BEDROCK": "1"
            },
            "allowedTools": ["tool-a"],
            "includeCoAuthoredBy": false
        });

        json_deep_merge(&mut target, &source);
        assert_eq!(target["env"]["ANTHROPIC_API_KEY"], json!("sk-test"));
        assert_eq!(target["env"]["CLAUDE_CODE_USE_BEDROCK"], json!("1"));
        assert_eq!(target["allowedTools"], json!(["tool-a"]));
        assert_eq!(target["includeCoAuthoredBy"], json!(false));

        json_deep_remove(&mut target, &source);
        assert_eq!(
            target,
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            })
        );
    }

    #[test]
    fn provider_auth_adapter_projects_strategy_contracts() {
        let bearer =
            ProviderAuthInfo::new("provider-token".to_string(), ProviderAuthStrategy::Bearer);
        assert_eq!(bearer.strategy, ProviderAuthStrategy::Bearer);
        assert_eq!(bearer.masked_key(), "prov...oken");
        assert!(bearer.access_token.is_none());

        let oauth = ProviderAuthInfo::with_access_token(
            "refresh-token".to_string(),
            "ya29.access-token-12345".to_string(),
        );
        assert_eq!(oauth.strategy, ProviderAuthStrategy::GoogleOAuth);
        assert_eq!(oauth.masked_access_token(), Some("ya29...2345".to_string()));

        let codex_adapter = crate::proxy::provider::CodexAdapter::new();
        let codex_provider = Provider::with_id(
            "codex".to_string(),
            "Codex".to_string(),
            json!({"apiKey": "sk-forwarder-auth"}),
            None,
        );
        let forwarder_auth = codex_adapter
            .extract_auth(&codex_provider)
            .expect("codex forwarder auth info");
        assert_eq!(forwarder_auth.api_key, "sk-forwarder-auth");
        assert_eq!(forwarder_auth.strategy, ProviderAuthStrategy::Bearer);
        let forwarder_auth_headers = codex_adapter
            .get_auth_headers(&forwarder_auth)
            .expect("codex forwarder auth headers");
        assert_eq!(forwarder_auth_headers.len(), 1);
        assert_eq!(forwarder_auth_headers[0].0, http::header::AUTHORIZATION);
        assert_eq!(
            forwarder_auth_headers[0].1.to_str().expect("header value"),
            "Bearer sk-forwarder-auth"
        );

        let missing_auth = Provider::with_id(
            "missing".to_string(),
            "Missing".to_string(),
            json!({}),
            None,
        );
        assert!(codex_adapter.extract_auth(&missing_auth).is_none());
    }

    #[test]
    fn provider_kind_adapter_projects_inference_helpers() {
        assert_eq!(
            infer_claude_provider_kind("gemini_native", true, None, None, &json!({})),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            infer_claude_provider_kind(
                "anthropic",
                false,
                Some("github_copilot"),
                Some("https://example.com"),
                &json!({})
            ),
            ProviderKind::GitHubCopilot
        );
        assert!(is_gemini_oauth_key_shape(" ya29.access-token "));
        assert!(is_gemini_oauth_key_shape(
            r#"{"access_token":"ya29.access-token"}"#
        ));
        assert!(!is_gemini_oauth_key_shape("AIza-api-key"));
        let mut gemini_cli_provider = Provider::with_id(
            "gemini-cli".to_string(),
            "Gemini CLI".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": r#"{"access_token":"ya29.access-token"}"#,
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com"
                }
            }),
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
        let mut gemini_cli_raw_provider = Provider::with_id(
            "gemini-cli-raw".to_string(),
            "Gemini CLI Raw".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "\nya29.raw-token-value\n",
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com"
                }
            }),
            None,
        );
        gemini_cli_raw_provider.meta = Some(ProviderMeta {
            api_format: Some("gemini_native".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_kind(&gemini_cli_raw_provider),
            ProviderKind::GeminiCli
        );
        let anthropic_provider = Provider::with_id(
            "anthropic".to_string(),
            "Anthropic".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&anthropic_provider),
            ProviderKind::Claude
        );
        let openrouter_provider = Provider::with_id(
            "openrouter".to_string(),
            "OpenRouter".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                    "OPENROUTER_API_KEY": "sk-or-test"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&openrouter_provider),
            ProviderKind::OpenRouter
        );
        let claude_auth_provider = Provider::with_id(
            "claude-auth".to_string(),
            "Claude Auth".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-test"
                },
                "auth_mode": "bearer_only"
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&claude_auth_provider),
            ProviderKind::ClaudeAuth
        );
        let mut copilot_provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "copilot-token",
                    "ANTHROPIC_BASE_URL": "https://example.com"
                }
            }),
            None,
        );
        copilot_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_kind(&copilot_provider),
            ProviderKind::GitHubCopilot
        );
        assert_eq!(
            provider_kind_from_app_type_and_config(&AppType::Claude, &copilot_provider),
            ProviderKind::GitHubCopilot
        );
        let copilot_url_provider = Provider::with_id(
            "copilot-url".to_string(),
            "Copilot URL".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&copilot_url_provider),
            ProviderKind::GitHubCopilot
        );
        let gemini_provider = Provider::with_id(
            "gemini-cli".to_string(),
            "Gemini CLI".to_string(),
            json!({
                "env": {
                    "GEMINI_API_KEY": r#"{"access_token":"ya29.access-token"}"#
                }
            }),
            None,
        );
        assert_eq!(
            provider_gemini_kind(&gemini_provider),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            provider_kind_from_app_type_and_config(&AppType::Gemini, &gemini_provider),
            ProviderKind::GeminiCli
        );
        let gemini_api_key_provider = Provider::with_id(
            "gemini-api-key".to_string(),
            "Gemini API Key".to_string(),
            json!({
                "env": {
                    "GEMINI_API_KEY": "AIza-api-key"
                }
            }),
            None,
        );
        assert_eq!(
            provider_gemini_kind(&gemini_api_key_provider),
            ProviderKind::Gemini
        );
    }

    #[test]
    fn proxy_adapter_classifies_local_proxy_urls_for_takeover_cleanup() {
        for url in [
            " http://127.0.0.1:15721 ",
            "http://localhost:15721",
            "http://0.0.0.0:15721",
            "http://[::1]:15721",
            "http://[::]:15721",
            "http://::1:15721",
            "http://:::15721",
        ] {
            assert!(is_local_proxy_url(url), "{url} should be local");
        }

        for url in [
            "https://127.0.0.1:15721",
            "socks5://localhost:15721",
            "http://relay.example/v1",
            "",
        ] {
            assert!(!is_local_proxy_url(url), "{url} should not be local");
        }
    }

    #[test]
    fn proxy_placeholder_adapter_projects_app_specific_live_detection() {
        let placeholder = "PROXY_MANAGED";

        let mut claude_live = json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": placeholder,
                "ANTHROPIC_API_KEY": "real-key",
                "ANTHROPIC_BASE_URL": "http://localhost:15721",
                "OTHER": "kept"
            }
        });
        assert_eq!(
            remove_claude_takeover_env_fields_if_present(&mut claude_live, placeholder, |url| url
                .starts_with("http://localhost")),
            Some(true)
        );
        let claude_env = claude_live
            .get("env")
            .and_then(Value::as_object)
            .expect("claude env");
        assert!(claude_env.get("ANTHROPIC_AUTH_TOKEN").is_none());
        assert!(claude_env.get("ANTHROPIC_BASE_URL").is_none());
        assert_eq!(
            claude_env.get("ANTHROPIC_API_KEY").and_then(Value::as_str),
            Some("real-key")
        );
        assert_eq!(
            claude_env.get("OTHER").and_then(Value::as_str),
            Some("kept")
        );

        let mut claude_real_config = json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "real-token",
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        });
        assert_eq!(
            remove_claude_takeover_env_fields_if_present(
                &mut claude_real_config,
                placeholder,
                |url| url.starts_with("http://localhost")
            ),
            Some(false)
        );
        let claude_real_env = claude_real_config
            .get("env")
            .and_then(Value::as_object)
            .expect("claude env");
        assert_eq!(
            claude_real_env
                .get("ANTHROPIC_AUTH_TOKEN")
                .and_then(Value::as_str),
            Some("real-token")
        );
        assert_eq!(
            claude_real_env
                .get("ANTHROPIC_BASE_URL")
                .and_then(Value::as_str),
            Some("https://api.anthropic.com")
        );

        let mut claude_missing_env = json!({});
        assert_eq!(
            remove_claude_takeover_env_fields_if_present(
                &mut claude_missing_env,
                placeholder,
                |url| url.starts_with("http://localhost")
            ),
            None
        );

        let mut codex_live = json!({"auth": {"OPENAI_API_KEY": "real-key"}});
        assert!(apply_codex_takeover_auth_placeholder_if_present(
            &mut codex_live,
            placeholder
        ));
        assert_eq!(
            codex_live
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some(placeholder)
        );
        assert!(remove_codex_takeover_auth_placeholder_if_present(
            &mut codex_live,
            placeholder
        ));
        assert!(codex_live
            .get("auth")
            .and_then(|auth| auth.get("OPENAI_API_KEY"))
            .is_none());

        let mut codex_live_with_real_auth = json!({"auth": {"OPENAI_API_KEY": "real-key"}});
        assert!(!remove_codex_takeover_auth_placeholder_if_present(
            &mut codex_live_with_real_auth,
            placeholder
        ));
        assert_eq!(
            codex_live_with_real_auth
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some("real-key")
        );

        let mut codex_live_without_auth = json!({"config": ""});
        assert!(!apply_codex_takeover_auth_placeholder_if_present(
            &mut codex_live_without_auth,
            placeholder
        ));
        assert!(codex_live_without_auth.get("auth").is_none());
        assert!(ensure_codex_takeover_auth_placeholder(
            &mut codex_live_without_auth,
            placeholder
        ));
        assert_eq!(
            codex_live_without_auth
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some(placeholder)
        );

        let mut gemini_config = json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://gemini.example",
                "GEMINI_API_KEY": "real-key",
                "OTHER": "kept"
            }
        });
        apply_gemini_takeover_env_fields(&mut gemini_config, "http://127.0.0.1:15721", placeholder);
        let gemini_env = gemini_config
            .get("env")
            .and_then(Value::as_object)
            .expect("gemini env");
        assert_eq!(
            gemini_env
                .get("GOOGLE_GEMINI_BASE_URL")
                .and_then(Value::as_str),
            Some("http://127.0.0.1:15721")
        );
        assert_eq!(
            gemini_env.get("GEMINI_API_KEY").and_then(Value::as_str),
            Some(placeholder)
        );
        assert_eq!(
            gemini_env.get("OTHER").and_then(Value::as_str),
            Some("kept")
        );
        assert_eq!(
            remove_gemini_takeover_env_fields_if_present(&mut gemini_config, placeholder, |url| {
                url.starts_with("http://127.0.0.1")
            }),
            Some(true)
        );
        let gemini_env = gemini_config
            .get("env")
            .and_then(Value::as_object)
            .expect("gemini env");
        assert!(gemini_env.get("GOOGLE_GEMINI_BASE_URL").is_none());
        assert!(gemini_env.get("GEMINI_API_KEY").is_none());
        assert_eq!(
            gemini_env.get("OTHER").and_then(Value::as_str),
            Some("kept")
        );

        let mut gemini_real_config = json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://gemini.example",
                "GEMINI_API_KEY": "real-key"
            }
        });
        assert_eq!(
            remove_gemini_takeover_env_fields_if_present(
                &mut gemini_real_config,
                placeholder,
                |url| url.starts_with("http://127.0.0.1")
            ),
            Some(false)
        );
        let gemini_real_env = gemini_real_config
            .get("env")
            .and_then(Value::as_object)
            .expect("gemini env");
        assert_eq!(
            gemini_real_env
                .get("GOOGLE_GEMINI_BASE_URL")
                .and_then(Value::as_str),
            Some("https://gemini.example")
        );
        assert_eq!(
            gemini_real_env
                .get("GEMINI_API_KEY")
                .and_then(Value::as_str),
            Some("real-key")
        );

        let mut missing_env = json!({});
        assert_eq!(
            remove_gemini_takeover_env_fields_if_present(&mut missing_env, placeholder, |url| url
                .starts_with("http://127.0.0.1")),
            None
        );
        apply_gemini_takeover_env_fields(&mut missing_env, "http://127.0.0.1:15721", placeholder);
        assert_eq!(
            missing_env
                .get("env")
                .and_then(|env| env.get("GEMINI_API_KEY"))
                .and_then(Value::as_str),
            Some(placeholder)
        );
    }

    #[test]
    fn claude_desktop_mimo_gate_adapter_requires_anthropic_format() {
        let should_normalize = |provider: &Provider, upstream_model: &str| {
            should_normalize_mimo_anthropic_thinking_history(
                MimoAnthropicThinkingNormalizationInput {
                    settings_config: &provider.settings_config,
                    api_format: provider
                        .meta
                        .as_ref()
                        .and_then(|meta| meta.api_format.as_deref()),
                    upstream_model,
                },
            )
        };

        let anthropic_provider = Provider::with_id(
            "anthropic-mimo".to_string(),
            "Anthropic MiMo".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.example.com"
                }
            }),
            None,
        );
        assert!(should_normalize(&anthropic_provider, "mimo-v2.5-pro"));

        let endpoint_provider = Provider::with_id(
            "mimo-endpoint".to_string(),
            "MiMo Endpoint".to_string(),
            json!({
                "baseURL": "https://api.xiaomimimo.com/anthropic"
            }),
            None,
        );
        assert!(should_normalize(&endpoint_provider, "claude-sonnet-4-6"));

        let mut openai_provider = endpoint_provider.clone();
        openai_provider.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        assert!(!should_normalize(&openai_provider, "mimo-v2.5-pro"));
    }

    #[test]
    fn copilot_account_adapter_projects_domain_and_composite_id_rules() {
        use crate::proxy_core::api::model_catalog::{
            copilot_composite_account_id, default_copilot_github_domain, is_copilot_ghes_domain,
            normalize_github_domain, COPILOT_PUBLIC_GITHUB_DOMAIN,
        };

        assert_eq!(COPILOT_PUBLIC_GITHUB_DOMAIN, "github.com");
        assert_eq!(default_copilot_github_domain(), "github.com");
        assert_eq!(
            normalize_github_domain("https://Company.GHE.Com/api/v3?foo=bar").unwrap(),
            "company.ghe.com"
        );
        assert!(!is_copilot_ghes_domain("github.com"));
        assert!(is_copilot_ghes_domain("company.ghe.com"));
        assert_eq!(copilot_composite_account_id("github.com", 12345), "12345");
        assert_eq!(
            copilot_composite_account_id("company.ghe.com", 12345),
            "company.ghe.com:12345"
        );
    }

    #[test]
    fn copilot_transport_adapter_projects_urls_and_model_parsing() {
        use crate::proxy_core::api::model_catalog::{
            copilot_api_base, copilot_github_client_id, copilot_github_device_code_url,
            copilot_github_oauth_token_url, copilot_github_user_url, copilot_token_url,
            copilot_usage_url, parse_copilot_models_response_bytes,
        };

        assert_eq!(
            copilot_github_client_id("github.com"),
            "Iv1.b507a08c87ecfe98"
        );
        assert_eq!(
            copilot_github_client_id("company.ghe.com"),
            "Ov23li8tweQw6odWQebz"
        );
        assert_eq!(
            copilot_github_device_code_url("company.ghe.com"),
            "https://company.ghe.com/login/device/code"
        );
        assert_eq!(
            copilot_github_oauth_token_url("company.ghe.com"),
            "https://company.ghe.com/login/oauth/access_token"
        );
        assert_eq!(
            copilot_github_user_url("github.com"),
            "https://api.github.com/user"
        );
        assert_eq!(
            copilot_token_url("company.ghe.com"),
            "https://company.ghe.com/api/v3/copilot_internal/v2/token"
        );
        assert_eq!(
            copilot_usage_url("company.ghe.com"),
            "https://company.ghe.com/api/v3/copilot_internal/user"
        );
        assert_eq!(
            copilot_api_base("company.ghe.com"),
            "https://copilot-api.company.ghe.com"
        );

        let models = parse_copilot_models_response_bytes(
            br#"{
            "data": [
                {
                    "id": "gpt-5.4",
                    "name": "GPT-5.4",
                    "vendor": "OpenAI",
                    "model_picker_enabled": true
                },
                {
                    "id": "hidden",
                    "name": "Hidden",
                    "vendor": "GitHub",
                    "model_picker_enabled": false
                }
            ]
        }"#,
        )
        .unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-5.4");
        assert_eq!(models[0].vendor, "OpenAI");
    }

    #[test]
    fn codex_catalog_adapter_builds_and_simplifies_model_catalog() {
        use crate::proxy_core::api::model_catalog::{
            build_codex_model_catalog_from_settings, has_codex_model_catalog_specs,
            simplify_codex_model_catalog,
        };

        let settings = json!({
            "modelCatalog": {
                "models": [
                    {
                        "model": "kimi-k2",
                        "displayName": "Kimi K2",
                        "contextWindow": "64000"
                    }
                ]
            }
        });
        let template = json!({
            "slug": "gpt-5.5",
            "display_name": "GPT-5.5",
            "context_window": 272000,
            "model_messages": {"base": "template"}
        });

        assert!(has_codex_model_catalog_specs(&settings));
        assert_eq!(DEFAULT_CODEX_MODEL_CONTEXT_WINDOW, 128_000);

        let catalog = build_codex_model_catalog_from_settings(&settings, 128_000, &template)
            .expect("catalog");
        let models = catalog
            .get("models")
            .and_then(Value::as_array)
            .expect("models");
        assert_eq!(
            models[0].get("slug").and_then(Value::as_str),
            Some("kimi-k2")
        );
        assert_eq!(
            models[0].get("context_window").and_then(Value::as_u64),
            Some(64_000)
        );

        let simplified = simplify_codex_model_catalog(&catalog.to_string(), 128_000)
            .expect("simplified catalog");
        assert_eq!(
            simplified["models"][0].get("model").and_then(Value::as_str),
            Some("kimi-k2")
        );
        assert_eq!(
            simplified["models"][0]
                .get("displayName")
                .and_then(Value::as_str),
            Some("Kimi K2")
        );
    }

    #[test]
    fn model_catalog_adapter_projects_provider_settings_and_client_raw() {
        let settings = json!({
            "model": " claude-sonnet-4 ",
            "env": {
                "ANTHROPIC_MODEL": "claude-opus-4"
            },
            "modelCatalog": {
                "models": [
                    {"model": "deepseek-v4"},
                    {"id": "kimi-k2"}
                ]
            }
        });

        let provider_catalog =
            crate::proxy_core::api::model_catalog::provider_model_catalog_from_settings(
                "provider-a",
                Some(&settings),
            );
        assert_eq!(provider_catalog.provider_id, "provider-a");
        assert_eq!(
            provider_catalog.models,
            vec![
                "claude-opus-4".to_string(),
                "claude-sonnet-4".to_string(),
                "deepseek-v4".to_string(),
                "kimi-k2".to_string()
            ]
        );
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            settings.clone(),
            None,
        );
        assert_eq!(
            crate::proxy_core::api::model_catalog::provider_model_catalog_from_settings(
                "provider-a",
                Some(&provider.settings_config),
            )
            .models,
            provider_catalog.models
        );
        let client_catalog =
            crate::proxy_core::api::model_catalog::client_model_catalog_from_optional_raw(
                AppKind::Codex.as_str(),
                Some(json!({
                    "models": [
                        {"id": " gpt-5 "},
                        {"model": "o4-mini"},
                        {"id": "gpt-5"}
                    ]
                })),
            );
        assert_eq!(client_catalog.provider_id, "codex");
        assert_eq!(
            client_catalog.models,
            vec!["gpt-5".to_string(), "o4-mini".to_string()]
        );
    }

    #[test]
    fn route_plan_adapter_projects_provider_ids_and_forward_selection() {
        fn selection(channel_id: &str, provider_id: &str) -> RouteSelection {
            let provider = ProviderSpec {
                id: provider_id.to_string(),
                name: provider_id.to_string(),
                kind: ProviderKind::Claude,
                account_ref: None,
                metadata: ProviderMetadata::default(),
            };
            let channel = ChannelSpec {
                id: channel_id.to_string(),
                provider_id: provider_id.to_string(),
                app: AppKind::Claude,
                name: channel_id.to_string(),
                status: ChannelStatus::Enabled,
                endpoint: UpstreamEndpoint {
                    base_url: "https://api.example.com".to_string(),
                    path_template: None,
                    api_version: None,
                    timeout_profile: None,
                },
                interface: InterfaceKind::OpenAiChatCompletions,
                auth_profile: None,
                models: Vec::new(),
                groups: vec!["default".to_string()],
                priority: 0,
                weight: 100,
                retry_policy: RetryPolicy {
                    raw: Value::Object(Default::default()),
                },
                health_policy: ChannelHealthPolicy {
                    raw: Value::Object(Default::default()),
                },
                overrides: ChannelOverrides {
                    headers: Value::Object(Default::default()),
                    params: Value::Object(Default::default()),
                    status_code_mapping: Value::Array(Vec::new()),
                    model_mapping: Value::Object(Default::default()),
                },
                tags: Vec::new(),
                metadata: Value::Object(Default::default()),
                source_ref: None,
                needs_review: false,
                review_reasons: Vec::new(),
            };

            crate::proxy_core::api::routing::route_selection_from_parts(
                provider,
                channel,
                None,
                InterfaceKind::OpenAiChatCompletions,
            )
        }

        let primary = selection("ch-a", "provider-a");
        let plan = RoutePlan {
            selection: primary.clone(),
            selections: vec![
                primary,
                selection("ch-b", "provider-b"),
                selection("ch-c", "provider-a"),
            ],
            attempts: Vec::new(),
        };

        assert_eq!(
            crate::proxy_core::api::routing::route_plan_provider_ids(&plan),
            vec!["provider-a".to_string(), "provider-b".to_string()]
        );
        assert_eq!(
            crate::proxy_core::api::routing::select_route_for_forward_result(
                &plan,
                Some("ch-b"),
                "provider-a"
            )
            .channel
            .id,
            "ch-b"
        );
        assert_eq!(
            crate::proxy_core::api::routing::select_route_for_forward_result(
                &plan,
                Some("ch-b"),
                "provider-a"
            )
            .outbound_interface,
            InterfaceKind::OpenAiChatCompletions
        );
        let selected = crate::proxy_core::api::routing::select_route_for_forward_result(
            &plan,
            Some("ch-b"),
            "provider-a",
        );
        let candidate =
            crate::proxy_core::api::routing::default_route_candidate_from_selection(&selected);
        assert_eq!(candidate.channel_id, "ch-b");
        assert_eq!(candidate.route_group, "default");
        assert_eq!(candidate.source_kind, "proxy_core");
        let resolved =
            crate::proxy_core::api::routing::resolved_channel_attempt_from_candidate(candidate);
        assert_eq!(resolved.channel_id, "ch-b");
        let host_provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        let attempts = forward_attempts_from_plan(&AppType::Claude, &[host_provider], &plan);
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].provider().id, "provider-a");
        assert_eq!(attempts[1].provider().id, "provider-a");
        let missing_attempts =
            required_forward_attempts_from_plan(&AppType::Claude, &[], &plan).unwrap_err();
        assert!(matches!(
            missing_attempts,
            ProxyCoreError::Unavailable(message)
                if message == "route plan has no matching host providers"
        ));
        assert_eq!(
            crate::proxy_core::api::routing::forwarding_requires_runtime_error_message(),
            "cc-switch forwarding requires a proxy server runtime"
        );
        assert!(matches!(
            crate::proxy_core::api::routing::forwarding_requires_runtime_error(),
            ProxyCoreError::Unsupported(message)
                if message == "cc-switch forwarding requires a proxy server runtime"
        ));
        assert_eq!(
            crate::proxy_core::api::routing::route_plan_no_matching_host_providers_error_message(),
            "route plan has no matching host providers"
        );
        assert!(matches!(
            crate::proxy_core::api::routing::route_plan_no_matching_host_providers_error(),
            ProxyCoreError::Unavailable(message)
                if message == "route plan has no matching host providers"
        ));
        assert_eq!(
            crate::proxy_core::api::routing::route_plan_providers_unconfigured_error_message(),
            "route plan providers are not configured in host database"
        );
        let policy = crate::proxy_core::api::routing::route_policy_from_failover_provider_ids(
            AppKind::Claude,
            vec!["provider-b".to_string()],
        );
        assert_eq!(policy.app, AppKind::Claude);
        assert_eq!(policy.raw["failoverProviderIds"], json!(["provider-b"]));

        let policy = crate::proxy_core::api::routing::route_policy_from_failover_provider_ids(
            AppKind::Claude,
            vec![("provider-b".to_string(), Some(1))]
                .into_iter()
                .map(|(provider_id, _sort_index)| provider_id),
        );
        assert_eq!(policy.app, AppKind::Claude);
        assert_eq!(policy.raw["failoverProviderIds"], json!(["provider-b"]));

        let mut body = json!({"model": "sonnet-public"});
        assert_eq!(
            crate::proxy_core::api::transport::apply_channel_route_model_override(
                &mut body,
                Some("sonnet-public"),
                Some("upstream-sonnet")
            )
            .as_deref(),
            Some("upstream-sonnet")
        );
        assert_eq!(
            body.get("model").and_then(Value::as_str),
            Some("upstream-sonnet")
        );
    }

    #[test]
    fn error_mapper_adapter_projects_error_contracts() {
        assert!(matches!(
            crate::proxy_core::api::errors::config_error_with_context(
                "load config",
                AppError::Message("disk failed".to_string())
            ),
            ProxyCoreError::Config(message)
                if message == "load config: disk failed"
        ));
        assert!(matches!(
            crate::proxy_core::api::errors::internal_error_with_context(
                "record usage",
                AppError::Message("db failed".to_string())
            ),
            ProxyCoreError::Internal(message)
                if message == "record usage: db failed"
        ));
        assert_eq!(
            selected_provider_missing_from_source_message("provider-a", "host database"),
            "selected provider is missing from host database: provider-a"
        );
        assert_eq!(
            selected_provider_not_applied_message("codex"),
            "selected provider is not available before route result is applied: codex"
        );
        assert_eq!(
            selected_provider_display_name_for_error(Some("Provider A"), "Codex"),
            "Provider A"
        );
        assert_eq!(
            selected_provider_display_name_for_error(None, "Codex"),
            "Codex"
        );
        assert_eq!(unselected_provider_fallback_id("codex"), "unselected:codex");
        assert_eq!(
            crate::proxy_core::api::transforms::codex_proxy_error_code(
                CodexProxyErrorKind::ForwardFailed
            ),
            "cc_switch_forward_failed"
        );
        let json_body =
            crate::proxy_core::api::transforms::codex_proxy_error_json(CodexProxyErrorContext {
                provider_name: "Relay",
                request_model: "model-a",
                endpoint: "/responses",
                fallback_message: "failed",
                fallback_code: "cc_switch_forward_failed",
                upstream_status: None,
                upstream_body: None,
            });
        assert_eq!(json_body["error"]["provider"], "Relay");
        let facts_body = crate::proxy::error_mapper::codex_proxy_error_json_from_host_facts(
            "Relay",
            "model-a",
            "/responses",
            crate::proxy::error_mapper::CodexProxyHostErrorFacts {
                status: ProxyErrorStatusKind::ForwardFailed,
                message: "failed",
                kind: CodexProxyErrorKind::ForwardFailed,
                upstream_status: None,
                upstream_body: None,
            },
        );
        assert_eq!(facts_body["error"]["code"], "cc_switch_forward_failed");
        assert_eq!(facts_body["error"]["provider"], "Relay");
        let proxy_error_body = crate::proxy::error_mapper::codex_proxy_error_json(
            "Relay",
            "model-a",
            "/responses",
            &ProxyError::Timeout("slow".to_string()),
        );
        assert_eq!(proxy_error_body["error"]["code"], "cc_switch_timeout");

        let upstream_error_body = crate::proxy::error_mapper::codex_proxy_error_json(
            "Relay",
            "model-a",
            "/responses",
            &ProxyError::UpstreamError {
                status: 429,
                body: Some("quota exceeded".to_string()),
            },
        );
        assert_eq!(
            upstream_error_body["error"]["code"],
            "cc_switch_upstream_error"
        );
        assert_eq!(upstream_error_body["error"]["upstream_status"], 429);
        assert!(upstream_error_body["error"]["message"]
            .as_str()
            .expect("upstream error message")
            .contains("quota exceeded"));

        let response = crate::proxy_core::api::transforms::codex_proxy_error_response(
            ProxyErrorStatusKind::AuthError,
            CodexProxyErrorContext {
                provider_name: "Relay",
                request_model: "model-a",
                endpoint: "/responses",
                fallback_message: "bad token",
                fallback_code: "cc_switch_auth_error",
                upstream_status: None,
                upstream_body: None,
            },
        )
        .expect("codex error response");
        assert_eq!(response.status.as_u16(), 401);
        let facts_response =
            crate::proxy::error_mapper::codex_proxy_error_response_from_host_facts(
                "Relay",
                "model-a",
                "/responses",
                crate::proxy::error_mapper::CodexProxyHostErrorFacts {
                    status: ProxyErrorStatusKind::AuthError,
                    message: "bad token",
                    kind: CodexProxyErrorKind::AuthError,
                    upstream_status: None,
                    upstream_body: None,
                },
            )
            .expect("codex facts error response");
        assert_eq!(facts_response.status.as_u16(), 401);
        let proxy_error_response = crate::proxy::error_mapper::codex_proxy_error_response(
            "Relay",
            "model-a",
            "/responses",
            &ProxyError::AuthError("bad token".to_string()),
        )
        .expect("codex proxy error response");
        assert_eq!(proxy_error_response.status.as_u16(), 401);

        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::Timeout("slow".to_string())),
            ForwardFailureKind::Timeout(message) if message == "slow"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::ForwardFailed(
                "connection reset".to_string()
            )),
            ForwardFailureKind::ForwardFailed(message) if message == "connection reset"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::AuthError("bad token".to_string())),
            ForwardFailureKind::AuthError(message) if message == "bad token"
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::ProviderUnhealthy(
                "half-open".to_string()
            )),
            ForwardFailureKind::RetryableOther(_)
        ));
        assert!(matches!(
            forward_failure_kind_from_proxy_error(&ProxyError::DatabaseError(
                "write failed".to_string()
            )),
            ForwardFailureKind::Other(_)
        ));
        match forward_failure_kind_from_proxy_error(&ProxyError::UpstreamError {
            status: 429,
            body: Some(r#"{"error":{"message":"rate limit"}}"#.to_string()),
        }) {
            ForwardFailureKind::Upstream { status, body } => {
                assert_eq!(status, 429);
                assert_eq!(
                    body.as_deref(),
                    Some(r#"{"error":{"message":"rate limit"}}"#)
                );
            }
            other => panic!("expected upstream failure, got {other:?}"),
        }
        assert_eq!(
            forwarder_rectifier_error_message(&ProxyError::UpstreamError {
                status: 400,
                body: Some("invalid thinking signature".to_string()),
            })
            .as_deref(),
            Some("invalid thinking signature")
        );
        assert_eq!(
            forwarder_rectifier_error_message(&ProxyError::UpstreamError {
                status: 400,
                body: None,
            }),
            None
        );
        assert_eq!(
            forwarder_rectifier_error_message(&ProxyError::Timeout("slow".to_string())).as_deref(),
            Some("超时: slow")
        );
        assert_eq!(
            ManagementAuthError::MissingBearerToken.message(),
            "Missing management bearer token"
        );
    }

    #[test]
    fn codex_handler_adapter_projects_tool_context_and_chat_error() {
        let context =
            crate::proxy_core::api::transforms::build_codex_tool_context_from_request(&json!({
                "tools": [
                    {
                        "type": "custom",
                        "name": "apply_patch"
                    }
                ]
            }));

        assert_eq!(context.chat_tools().len(), 1);
        assert!(context.is_custom_tool_chat_name("apply_patch"));

        let normalized =
            crate::proxy_core::api::transforms::normalize_codex_chat_error_body(b"Unauthorized");
        assert!(normalized.non_json_body_log_message().is_some());
        assert!(normalized.response_error.get("error").is_some());
    }

    #[test]
    fn claude_body_normalization_adapter_projects_stream_and_thinking_rules() {
        let mut stream_body = json!({"stream": true});
        crate::proxy_core::api::transport::inject_openai_stream_include_usage(&mut stream_body);
        assert_eq!(stream_body["stream_options"]["include_usage"], true);

        let settings = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic"
            }
        });
        let mut disabled_body = json!({
            "model": "deepseek-v4-pro",
            "thinking": {"type": "disabled"},
            "output_config": {"effort": "high", "temperature": 0.2},
            "reasoning_effort": "high"
        });

        assert!(normalize_deepseek_thinking_disabled_strip_effort(
            &mut disabled_body,
            &settings
        ));
        assert!(disabled_body.get("reasoning_effort").is_none());
        assert!(disabled_body["output_config"].get("effort").is_none());
        assert_eq!(disabled_body["output_config"]["temperature"], json!(0.2));

        let mut tool_body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {}}
                ]
            }]
        });

        assert!(should_normalize_anthropic_tool_thinking_history(
            &settings,
            &tool_body,
            "anthropic"
        ));
        assert!(normalize_anthropic_tool_thinking_history(&mut tool_body));
        assert_eq!(
            tool_body["messages"][0]["content"][0]["thinking"],
            ANTHROPIC_TOOL_THINKING_PLACEHOLDER
        );
    }

    #[test]
    fn media_prevention_adapter_projects_core_policy() {
        let mut image_body = json!({
            "model": "text-model",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        let text_only_provider = Provider::with_id(
            "provider-c".to_string(),
            "Provider C".to_string(),
            json!({
                "models": [ { "id": "text-model", "input": ["text"] } ]
            }),
            None,
        );

        assert_eq!(
            apply_forwarder_media_prevention_from_facts(ForwarderMediaPreventionFacts {
                rectifier_enabled: true,
                request_media_fallback: true,
                request_media_heuristic: false,
                body: &mut image_body,
                provider_settings: &text_only_provider.settings_config,
            }),
            1
        );
        assert_eq!(image_body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn upstream_url_adapter_projects_codex_and_gemini_url_rules() {
        let codex_adapter = crate::proxy::provider::CodexAdapter::new();
        assert_eq!(
            codex_adapter.build_url("https://api.openai.com/v1", "/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );

        let (endpoint, passthrough_query) =
            crate::proxy_core::api::transport::rewrite_codex_responses_endpoint_to_chat(
                "/v1/responses?foo=bar",
            )
            .into_parts();
        assert_eq!(endpoint, "/chat/completions?foo=bar");
        assert_eq!(passthrough_query.as_deref(), Some("foo=bar"));

        let codex_plan = forward_upstream_url_plan(
            ForwardUpstreamUrlPlanInput {
                base_url: "https://api.openai.com/v1/chat/completions",
                endpoint: "/v1/responses?foo=bar&api-version=old",
                is_full_url: false,
                codex_responses_to_chat: true,
                use_claude_transform: false,
                is_copilot: false,
                claude_api_format: None,
                body: &json!({}),
                channel_param_overrides: Some(&json!({"api-version": "2026-06-21"})),
            },
            |base_url, effective_endpoint| format!("{base_url}{effective_endpoint}"),
        );
        assert_eq!(
            codex_plan.effective_endpoint,
            "/chat/completions?foo=bar&api-version=old"
        );
        assert_eq!(
            codex_plan.passthrough_query.as_deref(),
            Some("foo=bar&api-version=old")
        );
        assert_eq!(
            codex_plan.url,
            "https://api.openai.com/v1/chat/completions?foo=bar&api-version=2026-06-21"
        );

        assert_eq!(
            crate::proxy_core::api::transforms::build_gemini_native_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "/v1beta/models/gemini-2.5-pro:generateContent",
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
        );
        assert_eq!(
            crate::proxy_core::api::transforms::resolve_gemini_native_url(
                "https://relay.example/custom/generate-content",
                "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
                true,
            ),
            "https://relay.example/custom/generate-content?alt=sse"
        );

        let gemini_plan = forward_upstream_url_plan(
            ForwardUpstreamUrlPlanInput {
                base_url: "https://relay.example/custom/generate-content",
                endpoint: "/v1/messages?beta=true",
                is_full_url: true,
                codex_responses_to_chat: false,
                use_claude_transform: true,
                is_copilot: false,
                claude_api_format: Some("gemini_native"),
                body: &json!({"model": "gemini-2.5-flash", "stream": true}),
                channel_param_overrides: None,
            },
            |base_url, effective_endpoint| format!("{base_url}{effective_endpoint}"),
        );
        assert_eq!(
            gemini_plan.effective_endpoint,
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(gemini_plan.passthrough_query.as_deref(), Some("alt=sse"));
        assert_eq!(
            gemini_plan.url,
            "https://relay.example/custom/generate-content?alt=sse"
        );
    }

    #[test]
    fn claude_api_format_adapter_projects_transform_gate() {
        let mut provider =
            Provider::with_id("claude".to_string(), "Claude".to_string(), json!({}), None);
        provider.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..ProviderMeta::default()
        });
        let claude_adapter = crate::proxy::provider::ClaudeAdapter::new();
        let codex_adapter = crate::proxy::provider::CodexAdapter::new();
        assert_eq!(claude_adapter.name(), "Claude");
        assert_eq!(codex_adapter.name(), "Codex");
        let forwarder_claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        assert_eq!(forwarder_claude_adapter.facts().adapter_name, "Claude");
        let forwarder_fallback_adapter =
            forwarder_provider_adapter_context_for_app(&AppType::Hermes);
        assert_eq!(forwarder_fallback_adapter.facts().adapter_name, "Codex");
        let codex_provider = Provider::with_id(
            "codex".to_string(),
            "Codex".to_string(),
            json!({"base_url": "https://relay.example/v1/"}),
            None,
        );
        assert_eq!(
            codex_adapter
                .extract_base_url(&codex_provider)
                .expect("codex base URL"),
            "https://relay.example/v1"
        );
        let missing_base_url = codex_adapter
            .extract_base_url(&provider)
            .expect_err("missing codex base URL should fail");
        assert!(matches!(
            missing_base_url,
            ProxyError::ConfigError(message) if message == "Codex Provider 缺少 base_url 配置"
        ));
        assert_eq!(claude_provider_api_format(&provider), "openai_chat");
        assert_eq!(
            crate::proxy_core::api::transforms::resolve_claude_forward_api_format(
                claude_provider_api_format(&provider),
                true,
                Some("OpenAI")
            ),
            "openai_responses"
        );
        assert!(claude_adapter.needs_transform(&provider));
        assert!(!codex_adapter.needs_transform(&provider));
        let passthrough_adapter_body = json!({"model": "gpt-4.1"});
        assert_eq!(
            codex_adapter
                .transform_request(passthrough_adapter_body.clone(), &provider)
                .expect("codex passthrough transform"),
            passthrough_adapter_body
        );
        let passthrough_body = json!({"model": "claude-3-5-sonnet"});
        assert_eq!(
            crate::proxy::provider::transform_claude_request_for_api_format(
                passthrough_body.clone(),
                &provider,
                "anthropic",
                None,
                None
            )
            .expect("anthropic passthrough"),
            passthrough_body
        );
        assert!(
            !crate::proxy_core::api::transforms::claude_api_format_needs_transform("anthropic")
        );
        assert!(
            crate::proxy_core::api::transforms::claude_api_format_needs_transform("openai_chat")
        );
        assert!(
            crate::proxy_core::api::transforms::claude_api_format_needs_transform(
                "openai_responses"
            )
        );
        assert!(
            crate::proxy_core::api::transforms::claude_api_format_needs_transform("gemini_native")
        );
        assert!(!crate::proxy_core::api::transforms::claude_api_format_needs_transform("unknown"));
    }

    #[test]
    fn claude_streaming_decision_adapter_preserves_codex_oauth_aggregation() {
        let mut codex_provider = Provider::with_id(
            "codex-oauth".to_string(),
            "Codex OAuth".to_string(),
            json!({}),
            None,
        );
        codex_provider.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            ..Default::default()
        });
        let mut sse_headers = HeaderMap::new();
        sse_headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );

        let aggregate_decision = provider_claude_transform_streaming_decision(
            &codex_provider,
            false,
            &sse_headers,
            "openai_responses",
        );
        assert!(!aggregate_decision.use_streaming);
        assert!(aggregate_decision.aggregate_codex_oauth_responses_sse);
        assert!(matches!(
            aggregate_decision.response_sse_aggregation,
            Some(UpstreamSseAggregationKind::Responses)
        ));

        let streaming_decision = provider_claude_transform_streaming_decision(
            &codex_provider,
            true,
            &HeaderMap::new(),
            "openai_responses",
        );
        assert!(streaming_decision.use_streaming);
        assert!(!streaming_decision.aggregate_codex_oauth_responses_sse);
        assert!(streaming_decision.response_sse_aggregation.is_none());

        let plain_provider =
            Provider::with_id("plain".to_string(), "Plain".to_string(), json!({}), None);
        let upstream_sse_decision = provider_claude_transform_streaming_decision(
            &plain_provider,
            false,
            &sse_headers,
            "openai_chat",
        );
        assert!(upstream_sse_decision.use_streaming);
        assert!(!upstream_sse_decision.aggregate_codex_oauth_responses_sse);
        assert!(upstream_sse_decision.response_sse_aggregation.is_none());

        let non_stream_chat_decision = provider_claude_transform_streaming_decision(
            &plain_provider,
            false,
            &HeaderMap::new(),
            "openai_chat",
        );
        assert!(!non_stream_chat_decision.use_streaming);
        assert!(!non_stream_chat_decision.aggregate_codex_oauth_responses_sse);
        assert!(matches!(
            non_stream_chat_decision.response_sse_aggregation,
            Some(UpstreamSseAggregationKind::ChatCompletions)
        ));
    }

    #[test]
    fn codex_chat_streaming_decision_core_preserves_sse_fallback() {
        let mut sse_headers = HeaderMap::new();
        sse_headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );

        let header_streaming_decision =
            crate::proxy_core::api::transforms::codex_chat_transform_streaming_decision(
                false,
                &sse_headers,
            );
        assert!(header_streaming_decision.use_streaming);
        assert!(header_streaming_decision.response_sse_aggregation.is_none());

        let requested_streaming_decision =
            crate::proxy_core::api::transforms::codex_chat_transform_streaming_decision(
                true,
                &HeaderMap::new(),
            );
        assert!(requested_streaming_decision.use_streaming);
        assert!(requested_streaming_decision
            .response_sse_aggregation
            .is_none());

        let non_stream_decision =
            crate::proxy_core::api::transforms::codex_chat_transform_streaming_decision(
                false,
                &HeaderMap::new(),
            );
        assert!(!non_stream_decision.use_streaming);
        assert!(matches!(
            non_stream_decision.response_sse_aggregation,
            Some(UpstreamSseAggregationKind::ChatCompletions)
        ));
    }

    #[test]
    fn upstream_request_adapter_projects_headers_and_body_serialization() {
        let mut inbound_headers = HeaderMap::new();
        inbound_headers.insert(http::header::HOST, http::HeaderValue::from_static("local"));
        inbound_headers.insert(
            http::header::ACCEPT_ENCODING,
            http::HeaderValue::from_static("gzip"),
        );

        let auth_headers = [(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer token"),
        )];
        let anthropic_beta = anthropic_beta_header_value(Some("other-beta"));
        let headers = build_upstream_request_headers(UpstreamRequestHeadersInput {
            inbound_headers: &inbound_headers,
            upstream_host: Some("upstream.example"),
            auth_headers: &auth_headers,
            channel_header_overrides: None,
            force_identity_encoding: true,
            custom_user_agent: None,
            is_copilot: false,
            should_send_anthropic_headers: true,
            anthropic_beta_value: Some(&anthropic_beta),
            codex_oauth_session_headers: &[],
            ensure_json_content_type: true,
        });

        assert_eq!(
            headers
                .get(http::header::HOST)
                .and_then(|value| value.to_str().ok()),
            Some("upstream.example")
        );
        assert_eq!(
            headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer token")
        );
        assert_eq!(
            headers
                .get(http::header::ACCEPT_ENCODING)
                .and_then(|value| value.to_str().ok()),
            Some("identity")
        );
        assert_eq!(
            headers
                .get("anthropic-beta")
                .and_then(|value| value.to_str().ok()),
            Some("claude-code-20250219,other-beta")
        );
        assert_eq!(
            headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        assert!(
            serialize_upstream_request_body(&http::Method::GET, &json!({"model": "x"}))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            serialize_upstream_request_body(&http::Method::POST, &json!({"model": "x"})).unwrap(),
            br#"{"model":"x"}"#
        );
    }

    #[test]
    fn upstream_transport_adapter_projects_request_and_send_policy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::ACCEPT,
            http::HeaderValue::from_static("text/event-stream"),
        );

        let request_policy =
            crate::proxy_core::api::transport::resolve_upstream_request_transport_policy(
                false,
                false,
                "/v1/responses",
                &json!({"model": "gpt-5"}),
                &headers,
            );
        assert!(request_policy.is_streaming_request);
        assert!(request_policy.force_identity_encoding);
        assert!(
            crate::proxy_core::api::transport::is_streaming_upstream_request(
                "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
                &json!({"model": "gemini-2.5-pro"}),
                &HeaderMap::new()
            )
        );
        assert!(is_socks_proxy_url(Some("socks5://127.0.0.1:1080")));

        let send_policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: true,
            preserve_exact_header_case: true,
            request_is_streaming: true,
            non_streaming_timeout: std::time::Duration::from_secs(5),
            streaming_first_byte_timeout: std::time::Duration::from_secs(1),
        });

        assert_eq!(send_policy.transport, UpstreamTransportKind::PooledReqwest);
        assert_eq!(
            send_policy.streaming_header_timeout,
            Some(std::time::Duration::from_secs(1))
        );
    }

    #[test]
    fn codex_user_agent_adapter_projects_official_client_policy() {
        assert!(is_official_codex_client_user_agent("codex_vscode/1.0.0"));
        assert!(is_official_codex_client_user_agent("codex_vscode/2.3.4"));
        assert!(is_official_codex_client_user_agent("codex_vscode/0.1"));
        assert!(is_official_codex_client_user_agent("codex_cli_rs/1.0.0"));
        assert!(is_official_codex_client_user_agent("codex_cli_rs/0.5.2"));
        assert!(!is_official_codex_client_user_agent("Mozilla/5.0"));
        assert!(!is_official_codex_client_user_agent("curl/7.68.0"));
        assert!(!is_official_codex_client_user_agent(
            "python-requests/2.25.1"
        ));
        assert!(!is_official_codex_client_user_agent("codex_other/1.0.0"));
        assert!(!is_official_codex_client_user_agent(""));
        assert!(!is_official_codex_client_user_agent(
            "some codex_vscode/1.0.0"
        ));
        assert!(!is_official_codex_client_user_agent(
            "prefix_codex_cli_rs/1.0.0"
        ));
    }

    #[test]
    fn usage_record_adapter_builds_request_log_and_missing_pricing_signal() {
        let record = UsageRecord {
            request_id: Some("req-usage-1".to_string()),
            message_id: Some("msg-usage-1".to_string()),
            app: AppKind::Claude,
            provider_id: "provider-a".to_string(),
            provider_kind: Some(ProviderKind::Claude),
            channel_id: Some("channel-a".to_string()),
            channel_name: Some("Channel A".to_string()),
            route_group: Some("default".to_string()),
            request_model: "public-sonnet".to_string(),
            outbound_model: "upstream-sonnet".to_string(),
            response_model: Some("upstream-sonnet".to_string()),
            pricing_model: None,
            tokens: crate::proxy_core::api::usage::UsageTokens {
                input_tokens: 1_000,
                output_tokens: 500,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
            latency_ms: 42,
            first_token_ms: Some(7),
            status_code: 200,
            error_message: None,
            session_id: Some("session-a".to_string()),
            is_streaming: true,
            metadata: json!({}),
        };
        assert_eq!(
            usage_selected_provider_missing_log_message(
                "Claude",
                UsageSelectedProviderMissingPhase::StreamingPassthrough,
            ),
            "[Claude] 跳过流式 usage 收集：ProxyEngine 尚未回填 selected provider"
        );
        assert_eq!(
            usage_selected_provider_missing_log_message(
                "Claude",
                UsageSelectedProviderMissingPhase::TransformedResponse,
            ),
            "[Claude] 跳过转换响应 usage 记录：ProxyEngine 尚未回填 selected provider"
        );
        assert_eq!(
            usage_selected_provider_missing_log_message(
                "Codex",
                UsageSelectedProviderMissingPhase::TransformedStreaming,
            ),
            "[Codex] 跳过转换流式 usage 收集：ProxyEngine 尚未回填 selected provider"
        );
        assert_eq!(
            crate::proxy_core::api::usage::usage_record_failure_warning_message(
                UsageRecordFailureLogContext::ForwardError,
                "db failed"
            ),
            "记录失败请求日志失败: db failed"
        );
        assert_eq!(
            crate::proxy_core::api::usage::usage_record_failure_warning_message(
                UsageRecordFailureLogContext::UsageRecord,
                "db failed"
            ),
            "[USG-001] 记录使用量失败: db failed"
        );
        assert_eq!(
            crate::proxy_core::api::usage::usage_record_debug_log_message(&record),
            "[claude] 记录请求日志: provider=provider-a, model=upstream-sonnet, streaming=true, status=200, latency_ms=42, first_token_ms=Some(7), session=session-a, input=1000, output=500, cache_read=0, cache_creation=0"
        );
        assert!(crate::proxy_core::api::usage::usage_logging_enabled_from_config_flag(Some(true)));
        assert!(
            !crate::proxy_core::api::usage::usage_logging_enabled_from_config_flag(Some(false))
        );
        assert!(crate::proxy_core::api::usage::usage_logging_enabled_from_config_flag(None));
    }

    #[test]
    fn claude_takeover_adapter_projects_one_m_marker_and_display_name() {
        assert_eq!(
            crate::proxy_core::api::model_catalog::claude_takeover_client_model_for_upstream(
                "claude-sonnet-4-6",
                true,
                "deepseek-v4-pro[1M]"
            ),
            "claude-sonnet-4-6[1M]"
        );
        assert_eq!(
            crate::proxy_core::api::model_catalog::claude_takeover_client_model_for_upstream(
                "claude-haiku-4-5",
                false,
                "deepseek-v4-flash[1M]"
            ),
            "claude-haiku-4-5"
        );
        assert_eq!(
            crate::proxy_core::api::model_catalog::claude_takeover_default_display_name(
                "deepseek-v4-ultra [1m]  "
            ),
            "deepseek-v4-ultra"
        );

        let provider = Provider::with_id(
            "takeover-model-provider".to_string(),
            "Takeover Model Provider".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "deepseek-v4-flash",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "deepseek-v4-pro[1M]",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "DeepSeek V4 Pro",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "deepseek-v4-ultra [1m]"
                }
            }),
            None,
        );
        let fields = claude_takeover_model_fields_from_settings(&provider.settings_config);

        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "claude-haiku-4-5".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
            "deepseek-v4-flash".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "claude-sonnet-4-6[1M]".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
            "DeepSeek V4 Pro".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "claude-opus-4-8[1M]".to_string()
        )));
        assert!(fields.contains(&(
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "deepseek-v4-ultra".to_string()
        )));

        let mut live_config = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://old.example",
                "ANTHROPIC_AUTH_TOKEN": "old-token",
                "ANTHROPIC_MODEL": "stale-model",
                "OPENAI_API_KEY": "old-openai",
                "OTHER": "kept"
            }
        });
        crate::proxy_core::api::ports::apply_claude_takeover_fields_with_policy_and_models(
            &mut live_config,
            "http://127.0.0.1:15721",
            "PROXY_MANAGED",
            ClaudeTakeoverAuthPolicy::ManagedAccount {
                keep_auth_token: true,
            },
            vec![(
                "ANTHROPIC_DEFAULT_SONNET_MODEL",
                "claude-sonnet-4-6".to_string(),
            )],
        );
        let env = live_config
            .get("env")
            .and_then(Value::as_object)
            .expect("env");
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
            Some("http://127.0.0.1:15721")
        );
        assert!(env.get("ANTHROPIC_MODEL").is_none());
        assert!(env.get("OPENAI_API_KEY").is_none());
        assert_eq!(
            env.get("ANTHROPIC_API_KEY").and_then(Value::as_str),
            Some("PROXY_MANAGED")
        );
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str),
            Some("PROXY_MANAGED")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                .and_then(Value::as_str),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(env.get("OTHER").and_then(Value::as_str), Some("kept"));
    }

    #[test]
    fn host_session_adapter_generates_uuid_when_core_needs_new_session_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_proxy_session_id(&headers, &body, "claude");

        uuid::Uuid::parse_str(&result.session_id).expect("generated session id should be a UUID");
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }

    #[test]
    fn stream_check_adapter_preserves_reachability_fields() {
        use crate::proxy_core::api::management::{
            channel_reachability_probe_error,
            channel_reachability_result_from_stream_check_result as stream_check_result_to_channel_reachability,
            channel_test_app_type_error, channel_test_provider_not_found_error,
            ChannelReachabilityStatus,
        };

        let result = StreamCheckResult {
            status: ChannelReachabilityStatus::Degraded,
            success: true,
            message: "slow but reachable".to_string(),
            response_time_ms: Some(6100),
            http_status: Some(200),
            model_used: String::new(),
            tested_at: 1_797_000_000,
            retry_count: 1,
            error_category: None,
        };

        let reachability = stream_check_result_to_channel_reachability(result);

        assert!(reachability.success);
        assert_eq!(
            reachability.status,
            ChannelReachabilityStatus::Degraded.as_str()
        );
        assert_eq!(reachability.message, "slow but reachable");
        assert_eq!(reachability.latency_ms, Some(6100));
        assert_eq!(reachability.http_status, Some(200));
        assert_eq!(reachability.tested_at, 1_797_000_000);
        assert_eq!(reachability.retry_count, 1);

        let probe = ChannelTestProbeRequest {
            channel_id: "channel-a".to_string(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
        };
        assert_eq!(
            probe.app_type.parse::<AppType>().expect("app type"),
            AppType::Claude
        );
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        assert_eq!(provider.id, "provider-a");
        let missing_provider: ProxyCoreResult<Provider> =
            None.ok_or_else(|| channel_test_provider_not_found_error(&probe));
        let missing_provider = missing_provider.expect_err("missing provider");
        assert!(matches!(
            missing_provider,
            ProxyCoreError::Config(message)
                if message == "provider not found for channel channel-a: provider-a"
        ));
        let invalid_probe = ChannelTestProbeRequest {
            app_type: "unknown-app".to_string(),
            ..probe.clone()
        };
        let invalid_app_type: ProxyCoreResult<AppType> = invalid_probe
            .app_type
            .parse::<AppType>()
            .map_err(channel_test_app_type_error);
        assert!(matches!(
            invalid_app_type,
            Err(ProxyCoreError::InvalidRequest(message))
                if message.contains("unknown-app")
        ));
        assert!(matches!(
            channel_reachability_probe_error("probe failed"),
            ProxyCoreError::Internal(message) if message == "probe failed"
        ));

        for (health_status, reachability_status) in [
            (
                ChannelReachabilityStatus::Operational,
                ChannelReachabilityStatus::Operational,
            ),
            (
                ChannelReachabilityStatus::Failed,
                ChannelReachabilityStatus::Failed,
            ),
        ] {
            let result = StreamCheckResult {
                status: health_status,
                success: false,
                message: String::new(),
                response_time_ms: None,
                http_status: None,
                model_used: String::new(),
                tested_at: 0,
                retry_count: 0,
                error_category: None,
            };

            assert_eq!(
                stream_check_result_to_channel_reachability(result).status,
                reachability_status.as_str()
            );
        }
    }

    #[test]
    fn provider_credentials_adapter_extracts_app_specific_values() {
        let claude = Provider::with_id(
            "claude".to_string(),
            "Claude".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token",
                    "ANTHROPIC_BASE_URL": "https://claude.example"
                }
            }),
            None,
        );
        let claude_credentials = provider_credential_values_with_issue(&claude, &AppType::Claude)
            .expect("claude credentials");
        assert_eq!(claude_credentials.api_key, "token");
        assert_eq!(claude_credentials.base_url, "https://claude.example");

        let codex = Provider::with_id(
            "codex".to_string(),
            "Codex".to_string(),
            json!({
                "auth": {"OPENAI_API_KEY": "sk-test"},
                "config": "base_url = \"https://codex.example/v1\"\n"
            }),
            None,
        );
        let codex_credentials = provider_credential_values_with_issue(&codex, &AppType::Codex)
            .expect("codex credentials");
        assert_eq!(codex_credentials.api_key, "sk-test");
        assert_eq!(codex_credentials.base_url, "https://codex.example/v1");

        let gemini = Provider::with_id(
            "gemini".to_string(),
            "Gemini".to_string(),
            json!({"env": {"GEMINI_API_KEY": "AIza-test"}}),
            None,
        );
        let gemini_credentials = provider_credential_values_with_issue(&gemini, &AppType::Gemini)
            .expect("gemini credentials");
        assert_eq!(gemini_credentials.api_key, "AIza-test");
        assert_eq!(
            gemini_credentials.base_url,
            "https://generativelanguage.googleapis.com"
        );

        let gemini_custom = Provider::with_id(
            "gemini-custom".to_string(),
            "Gemini Custom".to_string(),
            json!({
                "env": {
                    "GEMINI_API_KEY": "AIza-test",
                    "GOOGLE_GEMINI_BASE_URL": "https://gemini.example"
                }
            }),
            None,
        );
        let gemini_custom_credentials =
            provider_credential_values_with_issue(&gemini_custom, &AppType::Gemini)
                .expect("custom gemini credentials");
        assert_eq!(gemini_custom_credentials.api_key, "AIza-test");
        assert_eq!(gemini_custom_credentials.base_url, "https://gemini.example");

        let opencode = Provider::with_id(
            "opencode".to_string(),
            "OpenCode".to_string(),
            json!({
                "options": {
                    "apiKey": "sk-opencode",
                    "baseURL": "https://opencode.example"
                }
            }),
            None,
        );
        let opencode_credentials =
            provider_credential_values_with_issue(&opencode, &AppType::OpenCode)
                .expect("opencode credentials");
        assert_eq!(opencode_credentials.api_key, "sk-opencode");
        assert_eq!(opencode_credentials.base_url, "https://opencode.example");

        let openclaw = Provider::with_id(
            "openclaw".to_string(),
            "OpenClaw".to_string(),
            json!({
                "apiKey": "sk-openclaw",
                "baseUrl": "https://openclaw.example"
            }),
            None,
        );
        let openclaw_credentials =
            provider_credential_values_with_issue(&openclaw, &AppType::OpenClaw)
                .expect("openclaw credentials");
        assert_eq!(openclaw_credentials.api_key, "sk-openclaw");
        assert_eq!(openclaw_credentials.base_url, "https://openclaw.example");

        let missing_codex_base_url = Provider::with_id(
            "codex-missing-base-url".to_string(),
            "Codex Missing Base URL".to_string(),
            json!({"auth": {"OPENAI_API_KEY": "sk-test"}, "config": ""}),
            None,
        );
        assert_eq!(
            provider_credential_values_with_issue(&missing_codex_base_url, &AppType::Codex),
            Err(ProviderCredentialIssue::CodexBaseUrlMissing)
        );
        let missing_base_url_spec =
            provider_credential_issue_spec(ProviderCredentialIssue::CodexBaseUrlMissing);
        assert_eq!(missing_base_url_spec.key, "provider.codex.base_url.missing");
        assert_eq!(missing_base_url_spec.zh, "config.toml 中缺少 base_url 配置");
        assert_eq!(
            missing_base_url_spec.en,
            "base_url is missing from config.toml"
        );
        let missing_options_spec =
            provider_credential_issue_spec(ProviderCredentialIssue::OpenCodeOptionsMissing);
        assert_eq!(
            missing_options_spec.key,
            "provider.opencode.options.missing"
        );
        assert_eq!(
            missing_options_spec.en,
            "Invalid configuration: missing options section"
        );
    }

    #[test]
    fn claude_model_normalization_adapter_backfills_default_model_keys() {
        let mut settings = json!({
            "env": {
                "ANTHROPIC_MODEL": "claude-sonnet",
                "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-opus"
            }
        });

        assert!(normalize_claude_models_in_value(&mut settings));

        let env = settings
            .get("env")
            .and_then(Value::as_object)
            .expect("env object");
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
                .and_then(Value::as_str),
            Some("claude-haiku")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                .and_then(Value::as_str),
            Some("claude-sonnet")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_OPUS_MODEL")
                .and_then(Value::as_str),
            Some("claude-opus")
        );
        assert!(env.get("ANTHROPIC_SMALL_FAST_MODEL").is_none());

        assert!(!normalize_claude_models_in_value(&mut settings));

        let imported = provider_default_live_import_settings(
            &AppKind::from(&AppType::Claude),
            json!({
                "env": {
                    "ANTHROPIC_MODEL": "claude-sonnet",
                    "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku"
                }
            }),
        );
        assert_eq!(
            imported["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"].as_str(),
            Some("claude-haiku")
        );
        assert_eq!(
            imported["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"].as_str(),
            Some("claude-sonnet")
        );
        assert!(imported["env"].get("ANTHROPIC_SMALL_FAST_MODEL").is_none());

        let mut saved = json!({
            "env": {
                "ANTHROPIC_MODEL": "claude-sonnet",
                "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku"
            }
        });
        assert!(normalize_provider_settings_for_storage(
            &AppKind::from(&AppType::Claude),
            &mut saved
        ));
        assert_eq!(
            saved["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"].as_str(),
            Some("claude-haiku")
        );
        assert!(saved["env"].get("ANTHROPIC_SMALL_FAST_MODEL").is_none());

        let codex_settings = json!({"config": "model = \"gpt-5\""});
        assert_eq!(
            provider_default_live_import_settings(
                &AppKind::from(&AppType::Codex),
                codex_settings.clone()
            ),
            codex_settings
        );
        let mut codex_saved = codex_settings.clone();
        assert!(!normalize_provider_settings_for_storage(
            &AppKind::from(&AppType::Codex),
            &mut codex_saved
        ));
        assert_eq!(codex_saved, codex_settings);
    }

    #[test]
    fn default_live_import_skip_policy_distinguishes_manual_and_startup() {
        assert!(should_skip_manual_default_live_import(
            &AppKind::from(&AppType::OpenCode),
            false
        ));
        assert!(should_skip_startup_default_live_import(
            &AppKind::from(&AppType::OpenCode),
            false
        ));

        assert!(!should_skip_manual_default_live_import(
            &AppKind::from(&AppType::Claude),
            false
        ));
        assert!(should_skip_manual_default_live_import(
            &AppKind::from(&AppType::Claude),
            true
        ));

        assert!(!should_skip_startup_default_live_import(
            &AppKind::from(&AppType::Claude),
            false
        ));
        assert!(should_skip_startup_default_live_import(
            &AppKind::from(&AppType::Claude),
            true
        ));
    }

    #[test]
    fn provider_live_sync_scope_uses_all_only_for_additive_apps() {
        assert_eq!(
            provider_live_sync_scope_for_app(&AppKind::from(&AppType::OpenCode)),
            ProviderLiveSyncScope::AllProviders
        );
        assert_eq!(
            provider_live_sync_scope_for_app(&AppKind::from(&AppType::OpenClaw)),
            ProviderLiveSyncScope::AllProviders
        );
        assert_eq!(
            provider_live_sync_scope_for_app(&AppKind::from(&AppType::Claude)),
            ProviderLiveSyncScope::CurrentProvider
        );
        assert_eq!(
            provider_live_sync_scope_for_app(&AppKind::from(&AppType::ClaudeDesktop)),
            ProviderLiveSyncScope::CurrentProvider
        );
    }

    #[test]
    fn provider_current_provider_scope_excludes_additive_apps() {
        assert!(!provider_app_has_current_provider(&AppKind::from(
            &AppType::OpenCode
        )));
        assert!(!provider_app_has_current_provider(&AppKind::from(
            &AppType::OpenClaw
        )));
        assert!(provider_app_has_current_provider(&AppKind::from(
            &AppType::Claude
        )));
        assert!(provider_app_has_current_provider(&AppKind::from(
            &AppType::Codex
        )));
    }

    #[test]
    fn provider_initial_live_config_managed_marker_only_applies_to_additive_apps() {
        assert_eq!(
            provider_initial_live_config_managed_marker(&AppKind::from(&AppType::OpenCode), true),
            Some(true)
        );
        assert_eq!(
            provider_initial_live_config_managed_marker(&AppKind::from(&AppType::OpenClaw), false),
            Some(false)
        );
        assert_eq!(
            provider_initial_live_config_managed_marker(&AppKind::from(&AppType::Claude), true),
            None
        );
    }

    #[test]
    fn provider_legacy_common_config_migration_skips_additive_and_empty_snippets() {
        assert!(core_provider_supports_legacy_common_config_migration(
            &AppKind::from(&AppType::Claude)
        ));
        assert!(!core_provider_supports_legacy_common_config_migration(
            &AppKind::from(&AppType::OpenCode)
        ));
        assert!(!should_skip_provider_legacy_common_config_migration(
            &AppKind::from(&AppType::Claude),
            "legacy = true"
        ));
        assert!(should_skip_provider_legacy_common_config_migration(
            &AppKind::from(&AppType::Claude),
            "  \n  "
        ));
        assert!(should_skip_provider_legacy_common_config_migration(
            &AppKind::from(&AppType::OpenClaw),
            "legacy = true"
        ));
    }

    #[test]
    fn provider_live_config_presence_error_policy_tolerates_db_only_providers() {
        assert_eq!(
            provider_live_config_presence_error_policy(None),
            ProviderLiveConfigPresenceErrorPolicy::Strict
        );
        assert_eq!(
            provider_live_config_presence_error_policy(Some(true)),
            ProviderLiveConfigPresenceErrorPolicy::Strict
        );
        assert_eq!(
            provider_live_config_presence_error_policy(Some(false)),
            ProviderLiveConfigPresenceErrorPolicy::TreatErrorAsMissing
        );
    }

    #[test]
    fn provider_key_change_policy_blocks_non_additive_and_omo_providers() {
        assert_eq!(
            provider_key_change_policy_issue(&AppType::Claude, None),
            Some(ProviderKeyChangePolicyIssue::UnsupportedAppMode)
        );

        let mut omo_provider = Provider::with_id(
            "omo-provider".to_string(),
            "OMO Provider".to_string(),
            json!({}),
            None,
        );
        omo_provider.category = Some("omo".to_string());
        assert_eq!(
            provider_key_change_policy_issue(&AppType::OpenCode, Some(&omo_provider)),
            Some(ProviderKeyChangePolicyIssue::ExclusiveCurrentStateProvider)
        );

        let mut custom_provider = Provider::with_id(
            "custom-provider".to_string(),
            "Custom Provider".to_string(),
            json!({}),
            None,
        );
        custom_provider.category = Some("custom".to_string());
        assert_eq!(
            provider_key_change_policy_issue(&AppType::OpenCode, Some(&custom_provider)),
            None
        );
        assert_eq!(
            provider_key_change_policy_issue(&AppType::OpenClaw, None),
            None
        );
        assert_eq!(
            provider_key_change_policy_issue_message(
                ProviderKeyChangePolicyIssue::UnsupportedAppMode
            ),
            "Only additive-mode providers support changing provider key"
        );
    }

    #[test]
    fn provider_additive_live_write_action_skips_omo_and_unrequested_writes() {
        let mut omo_provider = Provider::with_id(
            "omo-provider".to_string(),
            "OMO Provider".to_string(),
            json!({}),
            None,
        );
        omo_provider.category = Some("omo-slim".to_string());
        assert_eq!(
            provider_additive_live_write_action(&AppType::OpenCode, &omo_provider, true),
            ProviderAdditiveLiveWriteAction::SkipExclusiveCurrentStateProvider
        );

        let custom_provider = Provider::with_id(
            "custom-provider".to_string(),
            "Custom Provider".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            provider_additive_live_write_action(&AppType::OpenCode, &custom_provider, false),
            ProviderAdditiveLiveWriteAction::SkipNotRequested
        );
        assert_eq!(
            provider_additive_live_write_action(&AppType::OpenClaw, &custom_provider, true),
            ProviderAdditiveLiveWriteAction::Write
        );
    }

    #[test]
    fn provider_omo_switch_pair_maps_enable_and_disable_variants() {
        let mut standard_provider = Provider::with_id(
            "omo-provider".to_string(),
            "OMO Provider".to_string(),
            json!({}),
            None,
        );
        standard_provider.category = Some("omo".to_string());
        assert_eq!(
            provider_omo_variant_for_app_category(&AppKind::from(&AppType::OpenCode), Some("omo")),
            Some(ProviderOmoVariant::Standard)
        );
        assert_eq!(
            provider_omo_switch_pair_for_app_category(
                &AppKind::from(&AppType::OpenCode),
                standard_provider.category.as_deref()
            ),
            Some(ProviderOmoSwitchPair {
                enable: ProviderOmoVariant::Standard,
                disable: ProviderOmoVariant::Slim,
            })
        );

        let mut slim_provider = standard_provider.clone();
        slim_provider.category = Some("omo-slim".to_string());
        assert_eq!(
            provider_omo_variant_for_app_category(
                &AppKind::from(&AppType::OpenCode),
                Some("omo-slim")
            ),
            Some(ProviderOmoVariant::Slim)
        );
        assert_eq!(
            provider_omo_switch_pair_for_app_category(
                &AppKind::from(&AppType::OpenCode),
                slim_provider.category.as_deref()
            ),
            Some(ProviderOmoSwitchPair {
                enable: ProviderOmoVariant::Slim,
                disable: ProviderOmoVariant::Standard,
            })
        );

        let custom_provider = Provider::with_id(
            "custom-provider".to_string(),
            "Custom Provider".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            provider_omo_switch_pair_for_app_category(
                &AppKind::from(&AppType::OpenCode),
                custom_provider.category.as_deref()
            ),
            None
        );
        assert_eq!(
            provider_omo_variant_for_app_category(
                &AppKind::from(&AppType::OpenCode),
                Some("custom")
            ),
            None
        );
        assert_eq!(
            provider_omo_switch_pair_for_app_category(
                &AppKind::from(&AppType::Claude),
                standard_provider.category.as_deref()
            ),
            None
        );
        assert_eq!(
            provider_omo_variant_for_app_category(&AppKind::from(&AppType::Claude), Some("omo")),
            None
        );
    }

    #[test]
    fn provider_switch_dispatch_routes_exclusive_and_desktop_to_normal_flow() {
        let mut omo_provider = Provider::with_id(
            "omo-provider".to_string(),
            "OMO Provider".to_string(),
            json!({}),
            None,
        );
        omo_provider.category = Some("omo".to_string());
        assert_eq!(
            provider_switch_dispatch_for_app(
                &AppKind::from(&AppType::OpenCode),
                omo_provider.category.as_deref()
            ),
            ProviderSwitchDispatch::Normal
        );

        let normal_provider = Provider::with_id(
            "normal-provider".to_string(),
            "Normal Provider".to_string(),
            json!({}),
            None,
        );
        assert_eq!(
            provider_switch_dispatch_for_app(
                &AppKind::from(&AppType::ClaudeDesktop),
                normal_provider.category.as_deref()
            ),
            ProviderSwitchDispatch::Normal
        );
        assert_eq!(
            provider_switch_dispatch_for_app(
                &AppKind::from(&AppType::OpenCode),
                normal_provider.category.as_deref()
            ),
            ProviderSwitchDispatch::TakeoverAware
        );
        assert_eq!(
            provider_switch_dispatch_for_app(
                &AppKind::from(&AppType::Claude),
                normal_provider.category.as_deref()
            ),
            ProviderSwitchDispatch::TakeoverAware
        );

        assert!(provider_switch_requires_takeover_lock(&AppKind::from(
            &AppType::Claude
        )));
        assert!(provider_switch_requires_takeover_lock(&AppKind::from(
            &AppType::Codex
        )));
        assert!(provider_switch_requires_takeover_lock(&AppKind::from(
            &AppType::Gemini
        )));
        assert!(!provider_switch_requires_takeover_lock(&AppKind::from(
            &AppType::ClaudeDesktop
        )));
        assert!(!provider_switch_requires_takeover_lock(&AppKind::from(
            &AppType::OpenCode
        )));
        assert!(!provider_switch_requires_takeover_lock(&AppKind::from(
            &AppType::OpenClaw
        )));
        assert!(!provider_switch_requires_takeover_lock(&AppKind::from(
            &AppType::Hermes
        )));

        let live_takeover_apps = live_takeover_app_types();
        assert_eq!(
            live_takeover_apps,
            [AppType::Claude, AppType::Codex, AppType::Gemini]
        );
        assert_eq!(
            live_token_sync_app_label(&AppKind::from(&AppType::Claude)),
            Some("Claude")
        );
        assert_eq!(
            live_token_sync_app_label(&AppKind::from(&AppType::Codex)),
            Some("Codex")
        );
        assert_eq!(
            live_token_sync_app_label(&AppKind::from(&AppType::Gemini)),
            Some("Gemini")
        );
        assert_eq!(
            live_token_sync_app_label(&AppKind::from(&AppType::ClaudeDesktop)),
            None
        );
        assert_eq!(
            live_token_sync_app_label(&AppKind::from(&AppType::OpenCode)),
            None
        );
    }

    #[test]
    fn provider_takeover_live_sync_target_keeps_desktop_on_live_config() {
        assert_eq!(
            provider_takeover_live_sync_target_for_app(&AppKind::from(&AppType::ClaudeDesktop)),
            ProviderTakeoverLiveSyncTarget::LiveConfig
        );
        assert_eq!(
            provider_takeover_live_sync_target_for_app(&AppKind::from(&AppType::Claude)),
            ProviderTakeoverLiveSyncTarget::LiveBackup
        );
        assert_eq!(
            provider_takeover_live_sync_target_for_app(&AppKind::from(&AppType::Codex)),
            ProviderTakeoverLiveSyncTarget::LiveBackup
        );
        assert_eq!(
            provider_takeover_live_sync_target_for_app(&AppKind::from(&AppType::Gemini)),
            ProviderTakeoverLiveSyncTarget::LiveBackup
        );
        assert_eq!(
            provider_takeover_live_sync_target_for_app(&AppKind::from(&AppType::OpenCode)),
            ProviderTakeoverLiveSyncTarget::LiveBackup
        );
    }

    #[test]
    fn provider_live_removal_target_only_covers_additive_live_configs() {
        assert_eq!(
            provider_live_removal_target_for_app(&AppKind::from(&AppType::OpenCode)),
            Some(ProviderLiveRemovalTarget::OpenCode)
        );
        assert_eq!(
            provider_live_removal_target_for_app(&AppKind::from(&AppType::OpenClaw)),
            Some(ProviderLiveRemovalTarget::OpenClaw)
        );
        assert_eq!(
            provider_live_removal_target_for_app(&AppKind::from(&AppType::Hermes)),
            Some(ProviderLiveRemovalTarget::Hermes)
        );
        assert_eq!(
            provider_live_removal_target_for_app(&AppKind::from(&AppType::Claude)),
            None
        );
        assert_eq!(
            provider_live_removal_target_for_app(&AppKind::from(&AppType::ClaudeDesktop)),
            None
        );
        assert_eq!(
            provider_live_removal_target_for_app(&AppKind::from(&AppType::Codex)),
            None
        );
        assert_eq!(
            provider_live_removal_target_for_app(&AppKind::from(&AppType::Gemini)),
            None
        );
    }

    #[test]
    fn provider_delete_is_current_provider_checks_local_and_db_sources() {
        assert!(provider_delete_is_current_provider(
            "provider-a",
            Some("provider-a"),
            None
        ));
        assert!(provider_delete_is_current_provider(
            "provider-a",
            None,
            Some("provider-a")
        ));
        assert!(provider_delete_is_current_provider(
            "provider-a",
            Some("provider-a"),
            Some("provider-a")
        ));
        assert!(!provider_delete_is_current_provider(
            "provider-a",
            Some("provider-b"),
            Some("provider-c")
        ));
        assert!(!provider_delete_is_current_provider(
            "provider-a",
            None,
            None
        ));
    }

    #[test]
    fn provider_additive_update_route_keeps_omo_separate_from_live_presence() {
        assert_eq!(
            provider_additive_update_route_for_app(&AppKind::from(&AppType::OpenCode), Some("omo")),
            Some(ProviderAdditiveUpdateRoute::OmoVariant(
                ProviderOmoVariant::Standard
            ))
        );
        assert_eq!(
            provider_additive_update_route_for_app(
                &AppKind::from(&AppType::OpenCode),
                Some("omo-slim")
            ),
            Some(ProviderAdditiveUpdateRoute::OmoVariant(
                ProviderOmoVariant::Slim
            ))
        );
        assert_eq!(
            provider_additive_update_route_for_app(
                &AppKind::from(&AppType::OpenCode),
                Some("custom")
            ),
            Some(ProviderAdditiveUpdateRoute::LiveConfigPresence)
        );
        assert_eq!(
            provider_additive_update_route_for_app(&AppKind::from(&AppType::OpenClaw), None),
            Some(ProviderAdditiveUpdateRoute::LiveConfigPresence)
        );
        assert_eq!(
            provider_additive_update_route_for_app(&AppKind::from(&AppType::Hermes), None),
            Some(ProviderAdditiveUpdateRoute::LiveConfigPresence)
        );
        assert_eq!(
            provider_additive_update_route_for_app(&AppKind::from(&AppType::Claude), Some("omo")),
            None
        );
    }

    #[test]
    fn provider_switch_backfill_source_id_requires_exclusive_different_current() {
        assert_eq!(
            provider_switch_backfill_source_id(
                &AppKind::from(&AppType::Claude),
                Some("current"),
                "target"
            ),
            Some("current")
        );
        assert_eq!(
            provider_switch_backfill_source_id(
                &AppKind::from(&AppType::Claude),
                Some("target"),
                "target"
            ),
            None
        );
        assert_eq!(
            provider_switch_backfill_source_id(&AppKind::from(&AppType::Claude), None, "target"),
            None
        );
        assert_eq!(
            provider_switch_backfill_source_id(
                &AppKind::from(&AppType::OpenCode),
                Some("current"),
                "target"
            ),
            None
        );
    }

    #[test]
    fn provider_switch_should_mark_live_config_managed_only_for_unmanaged_additive() {
        assert!(provider_switch_should_mark_live_config_managed(
            &AppKind::from(&AppType::OpenCode),
            None
        ));
        assert!(provider_switch_should_mark_live_config_managed(
            &AppKind::from(&AppType::OpenCode),
            Some(false)
        ));
        assert!(!provider_switch_should_mark_live_config_managed(
            &AppKind::from(&AppType::OpenCode),
            Some(true)
        ));
        assert!(!provider_switch_should_mark_live_config_managed(
            &AppKind::from(&AppType::Claude),
            None
        ));
    }

    #[test]
    fn provider_conversion_uses_inferred_provider_kind_without_settings_leak() {
        let mut provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "secret-token",
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com",
                    "CLAUDE_CODE_USE_BEDROCK": "1"
                }
            }),
            Some("https://github.com/features/copilot".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            is_full_url: Some(true),
            custom_user_agent: Some("cc-switch-test/1.0".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("github_copilot".to_string()),
                account_id: Some("acct-1".to_string()),
            }),
            usage_script: Some(UsageScript {
                enabled: true,
                language: "javascript".to_string(),
                code: String::new(),
                timeout: None,
                api_key: None,
                base_url: None,
                access_token: None,
                user_id: None,
                template_type: Some("github_copilot".to_string()),
                auto_query_interval: None,
                coding_plan_provider: None,
            }),
            test_config: Some(ProviderTestConfig {
                enabled: true,
                timeout_secs: Some(20),
                degraded_threshold_ms: Some(3000),
                max_retries: None,
            }),
            ..ProviderMeta::default()
        });

        let usage_provider_kind = provider_kind_from_provider(&provider);
        let usage_provider_classification = provider_managed_auth_classification(&provider);
        let usage_provider_is_codex_oauth = usage_provider_classification.is_codex_oauth;
        let usage_provider_is_github_copilot = usage_provider_classification.is_github_copilot;
        let usage_provider_uses_managed_account =
            usage_provider_classification.uses_managed_account;
        let usage_provider_needs_claude_transform = provider_needs_claude_transform(&provider);
        let copilot_account_id = provider_github_copilot_managed_account_id(&provider);
        let models_are_claude_safe =
            crate::proxy_core::api::auth::claude_desktop_provider_models_are_profile_safe(
                &provider.settings_config,
            );
        let stream_check_timeout_secs = provider
            .enabled_test_config()
            .and_then(|config| config.timeout_secs);
        assert_eq!(
            provider
                .usage_script()
                .and_then(|script| script.template_type.as_deref()),
            Some("github_copilot")
        );
        assert!(Provider::with_id(
            "without-usage".to_string(),
            "Without Usage".to_string(),
            json!({}),
            None,
        )
        .usage_script()
        .is_none());
        let usage_provider_is_full_url = provider.is_full_url();
        assert_eq!(
            bedrock_env_flag_from_provider_settings(&provider.settings_config),
            Some("1")
        );
        let mut codex_provider = Provider::with_id(
            "codex-oauth".to_string(),
            "Codex OAuth".to_string(),
            json!({}),
            None,
        );
        codex_provider.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("codex_oauth".to_string()),
                account_id: Some("codex-acct-1".to_string()),
            }),
            ..ProviderMeta::default()
        });
        let mut claude_auth_provider = Provider::with_id(
            "claude-auth".to_string(),
            "Claude Auth".to_string(),
            json!({}),
            None,
        );
        claude_auth_provider.meta = Some(ProviderMeta {
            provider_type: Some("claude_auth".to_string()),
            ..ProviderMeta::default()
        });

        let spec = proxy_provider_to_core_spec(&provider, &AppType::Claude);
        let source_spec = provider_spec_from_source(&AppKind::Claude, Some(provider.clone()))
            .expect("provider spec")
            .expect("provider");
        let source_specs =
            provider_specs_from_source(&AppKind::Claude, vec![provider]).expect("provider specs");

        assert_eq!(spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(source_spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(source_specs[0].kind, ProviderKind::GitHubCopilot);
        assert_eq!(usage_provider_kind, Some(ProviderKind::GitHubCopilot));
        assert!(!usage_provider_is_codex_oauth);
        assert!(usage_provider_is_github_copilot);
        assert!(usage_provider_uses_managed_account);
        assert!(usage_provider_needs_claude_transform);
        assert_eq!(copilot_account_id.as_deref(), Some("acct-1"));
        assert!(models_are_claude_safe);
        assert_eq!(stream_check_timeout_secs, Some(20));
        assert!(usage_provider_is_full_url);
        assert!(provider_is_codex_oauth(&codex_provider));
        let codex_context = provider_managed_account_binding_context(&codex_provider);
        let codex_binding = codex_context
            .binding
            .expect("codex managed account binding");
        assert_eq!(
            codex_binding.source,
            ManagedAccountBindingSource::ManagedAccount
        );
        assert_eq!(codex_binding.auth_provider, Some("codex_oauth"));
        assert_eq!(codex_binding.account_id, Some("codex-acct-1"));
        assert!(provider_uses_anthropic_rectifiers(
            &AppType::Claude,
            &claude_auth_provider
        ));
        assert!(!provider_uses_anthropic_rectifiers(
            &AppType::Codex,
            &claude_auth_provider
        ));
        assert_eq!(spec.account_ref.as_deref(), Some("github_copilot:acct-1"));
        let serialized = serde_json::to_string(&spec).expect("serialize spec");
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("ANTHROPIC_AUTH_TOKEN"));
        assert!(!serialized.contains("settingsConfig"));
    }

    #[test]
    fn provider_managed_account_binding_projection_uses_core_policy() {
        let mut legacy_provider = Provider::with_id(
            "legacy-copilot".to_string(),
            "Legacy Copilot".to_string(),
            json!({}),
            None,
        );
        legacy_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            github_account_id: Some("legacy-acct".to_string()),
            ..ProviderMeta::default()
        });

        assert_eq!(
            provider_github_copilot_managed_account_id(&legacy_provider).as_deref(),
            Some("legacy-acct")
        );
        let legacy_context = provider_managed_account_binding_context(&legacy_provider);
        assert_eq!(legacy_context.binding, None);
        assert_eq!(
            legacy_context.legacy_github_copilot_account_id,
            Some("legacy-acct")
        );
        let legacy_spec = proxy_provider_to_core_spec(&legacy_provider, &AppType::Claude);
        assert_eq!(
            legacy_spec.account_ref.as_deref(),
            Some("github_copilot:legacy-acct")
        );

        let mut default_account_provider = Provider::with_id(
            "default-copilot".to_string(),
            "Default Copilot".to_string(),
            json!({}),
            None,
        );
        default_account_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            github_account_id: Some("legacy-acct".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("github_copilot".to_string()),
                account_id: None,
            }),
            ..ProviderMeta::default()
        });

        assert_eq!(
            provider_github_copilot_managed_account_id(&default_account_provider),
            None
        );
        let default_context = provider_managed_account_binding_context(&default_account_provider);
        let binding = default_context.binding.expect("managed account binding");
        assert_eq!(binding.source, ManagedAccountBindingSource::ManagedAccount);
        assert_eq!(binding.auth_provider, Some("github_copilot"));
        assert_eq!(binding.account_id, None);
        assert_eq!(
            default_context.legacy_github_copilot_account_id,
            Some("legacy-acct")
        );
        let default_spec = proxy_provider_to_core_spec(&default_account_provider, &AppType::Claude);
        assert_eq!(default_spec.account_ref, None);
    }

    #[test]
    fn channel_conversion_preserves_endpoint_interface_models_and_groups() {
        let channel = ProxyChannelRecord {
            id: "ch-1".to_string(),
            provider_id: "provider-1".to_string(),
            app_type: "claude".to_string(),
            name: "Relay A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_chat_completions".to_string(),
            auth_profile_ref: Some("provider:claude:provider-1".to_string()),
            groups: vec!["default".to_string(), "paid".to_string()],
            priority: 50,
            weight: 80,
            retry_policy: json!({"maxAttempts": 2}),
            health_policy: json!({"breaker": "standard"}),
            header_overrides: json!({"x-test": "1"}),
            param_overrides: json!({"stream": true}),
            status_code_mapping: json!([{"from": 429, "to": 503}]),
            tags: vec!["manual".to_string()],
            metadata: json!({"owner": "ops"}),
            source_kind: ProxyChannelSourceKind::Manual,
            source_endpoint_url: Some("https://relay.example.com/v1".to_string()),
            models: vec![ProxyChannelModelRecord {
                channel_id: "ch-1".to_string(),
                public_model: "sonnet".to_string(),
                upstream_model: "anthropic/sonnet".to_string(),
                capabilities: json!({"tools": true}),
                pricing_model: Some("standard".to_string()),
                request_overrides: json!({"temperature": 0.2}),
                response_overrides: json!({}),
            }],
            needs_review: false,
            review_reasons: Vec::new(),
        };

        let spec = proxy_channel_record_to_core_spec(&channel);
        let source_spec = channel_spec_from_source(Some(channel.clone())).expect("channel spec");
        let (materialized_channels, materialized_source) =
            channel_route_records_from_sources(vec![channel.clone()], || {
                panic!("materialized channels must not load legacy projection")
            })
            .expect("materialized route records");
        let (legacy_channels, legacy_source) =
            channel_route_records_from_sources(Vec::new(), || {
                Ok(ProxyChannelMigrationPreview {
                    app_type: "claude".to_string(),
                    channels: vec![channel.clone()],
                    duplicate_count: 0,
                    needs_review_count: 0,
                })
            })
            .expect("legacy route records");

        assert_eq!(source_spec.id, "ch-1");
        assert_eq!(
            materialized_source,
            ChannelRouteSource::MaterializedChannels
        );
        assert_eq!(materialized_channels[0].id, "ch-1");
        assert_eq!(legacy_source, ChannelRouteSource::LegacyProjection);
        assert_eq!(legacy_channels[0].id, "ch-1");
        assert_eq!(spec.app, AppKind::Claude);
        assert_eq!(spec.endpoint.base_url, "https://relay.example.com/v1");
        assert_eq!(spec.interface, InterfaceKind::OpenAiChatCompletions);
        assert_eq!(spec.groups, vec!["default".to_string(), "paid".to_string()]);
        assert_eq!(spec.models.len(), 1);
        assert_eq!(spec.models[0].public_model, "sonnet");
        assert_eq!(spec.models[0].upstream_model, "anthropic/sonnet");
        assert_eq!(
            spec.auth_profile.as_ref().map(|value| value.0.as_str()),
            Some("provider:claude:provider-1")
        );
    }
}
