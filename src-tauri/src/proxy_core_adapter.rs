#[cfg(test)]
use crate::app_config::AppType;
#[cfg(test)]
use crate::error::AppError;
#[cfg(test)]
use crate::provider::Provider;
#[cfg(test)]
use crate::proxy::engine::forward_pipeline::ForwarderRuntimeOptions;
#[cfg(test)]
use crate::proxy::host::cc_switch::channel_auth_profile_attempts::{
    forward_attempts_from_plan, required_forward_attempts_from_plan,
};
use crate::proxy::host::cc_switch::proxy_runtime::{
    app_type_from_proxy_core_app, app_type_option_from_proxy_core_app,
    forward_current_provider_id_from_source, forward_runtime_request_from_proxy_request,
    forwarder_runtime_config_from_sources, forwarder_runtime_options_from_app_proxy_config,
    response_runtime_policy_from_app_proxy_config,
};
#[cfg(test)]
use crate::proxy::provider::claude_provider_api_format;
#[cfg(test)]
use crate::proxy::route_attempt::ForwardAttempt;
#[cfg(test)]
use http::{HeaderMap, Method};
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use uuid::Uuid;

#[cfg(test)]
use crate::proxy::host::cc_switch::forwarder_request_source::forwarder_rectifier_error_message;

#[cfg(test)]
mod tests {
    use crate::proxy::engine::routing::provider_router_app_error_from_provider_selection_failure;
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
        claude_env_credentials_from_settings, codex_auth_object_value_from_settings,
        codex_provider_live_write_parts_from_settings, gemini_env_map_from_settings,
        gemini_live_backup_from_effective_settings, gemini_live_settings_from_env_json_and_config,
        gemini_live_settings_to_write, live_takeover_app_kinds,
        provider_settings_validation_issue_spec, provider_settings_validation_parts_from_settings,
        proxy_runtime_status_stopped, CodexProviderLiveWriteIssue, CodexProviderValidationIssue,
        ProviderSettingsValidationIssue,
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
    use serde_json::json;

    use super::*;
    use crate::database::{
        ProxyChannelMigrationPreview, ProxyChannelModelRecord, ProxyChannelRecord,
        ProxyChannelSourceKind,
    };
    use crate::provider::{
        AuthBinding, AuthBindingSource, ProviderMeta, ProviderTestConfig, UsageScript,
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
    use crate::proxy_core::api::auth::{
        claude_desktop_model_id_is_profile_safe, extract_gemini_base_url_from_settings,
        ClaudeAuthKeySource, ManagementAuthError, ProviderAuthInfo, ProviderAuthStrategy,
    };
    use crate::proxy_core::api::auth::{
        extract_claude_auth_key_from_settings, extract_gemini_api_key_from_settings,
        is_gemini_oauth_key_shape, ManagedAccountBindingSource,
    };
    use crate::proxy_core::api::config::ResponseTimeoutConfig;
    use crate::proxy_core::api::domain::{
        infer_claude_provider_kind, ChannelHealthPolicy, ChannelOverrides, ProviderMetadata,
        ProviderSpec, RetryPolicy, UpstreamEndpoint,
    };
    use crate::proxy_core::api::errors::{
        selected_provider_display_name_for_error, selected_provider_missing_from_source_message,
        selected_provider_not_applied_message, unselected_provider_fallback_id, ProxyCoreError,
        ProxyErrorStatusKind,
    };
    use crate::proxy_core::api::events::{
        attempt_event, request_started_event, route_selected_event, server_started_event,
        server_stopped_event, AttemptEventChannel, AttemptEventPayloadInput, AttemptEventPhase,
        ProxyCoreEvent, ProxyEventEnvelope,
    };
    use crate::proxy_core::api::management::{ChannelRouteSource, RouteResolveRequest};
    use crate::proxy_core::api::ports::{
        codex_restored_live_settings_parts, gemini_env_json_from_map,
        gemini_env_string_map_from_settings, gemini_live_config_object_from_settings,
        CopilotOptimizerConfig, CurrentRouteTarget, GeminiLiveConfigIssue, OptimizerConfig,
        ProxyConfig, RectifierConfig,
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
        should_normalize_anthropic_tool_thinking_history, CodexProxyErrorContext,
        CodexProxyErrorKind, ANTHROPIC_TOOL_THINKING_PLACEHOLDER,
    };
    use crate::proxy_core::api::transforms::{
        CodexChatReasoningOptions, CodexChatReasoningProfile,
    };
    use crate::proxy_core::api::transport::resolve_response_runtime_policy;
    use crate::proxy_core::api::transport::{
        anthropic_beta_header_value, apply_forwarder_media_prevention_from_facts,
        bedrock_env_flag_from_provider_settings, build_claude_auth_headers,
        build_codex_bearer_auth_headers, build_copilot_auth_headers, build_gemini_auth_headers,
        build_upstream_request_headers, forward_upstream_url_plan, is_socks_proxy_url,
        resolve_upstream_send_policy, serialize_upstream_request_body, ClaudeAuthHeaderKind,
        CopilotAuthHeadersInput, ForwardFailureKind, ForwardUpstreamUrlPlanInput,
        ForwarderMediaPreventionFacts, ProxyBody, ProxyCoreResponse, ProxyRequest,
        ProxyResponseBody, ProxyTransportResponseBody, UpstreamRequestHeadersInput,
        UpstreamSendPolicyInput, UpstreamSseAggregationKind, UpstreamTransportKind,
    };
    use crate::proxy_core::api::usage::{
        usage_selected_provider_missing_log_message, TokenUsage, TransformedResponseUsageFormat,
        UsageRouteContext, UsageSelectedProviderMissingPhase,
    };
    use bytes::Bytes;
    use indexmap::IndexMap;
    use std::sync::Arc;

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
        let route_provider = route_attempt.provider();
        let route_channel = route_attempt.channel().expect("channel route attempt");
        let route_payload = AttemptEventPayloadInput {
            request_id: "req-route",
            app_type: "claude",
            provider_id: route_provider.id.as_str(),
            provider_name: route_provider.name.as_str(),
            channel: Some(AttemptEventChannel {
                channel_id: route_channel.channel_id.as_str(),
                channel_name: route_channel.channel_name.as_str(),
                interface_kind: route_channel.interface_kind.as_str(),
                public_model: route_channel.public_model.as_deref(),
                upstream_model: route_channel.upstream_model.as_deref(),
                pricing_model: route_channel.pricing_model.as_deref(),
            }),
            error: None,
        };
        let route_message = bus.emit_core_event(route_selected_event(route_payload));
        assert_eq!(route_message.event, "route_selected");
        assert_eq!(route_message.payload["requestId"], "req-route");
        assert_eq!(route_message.payload["providerId"], "provider-1");
        assert_eq!(route_message.payload["channelId"], "channel-a");
        assert_eq!(route_message.payload["interfaceKind"], "openai_responses");
        assert_eq!(route_message.payload["upstreamModel"], "upstream-sonnet");

        let failed_attempt_message = bus.emit_core_event(attempt_event(
            AttemptEventPayloadInput {
                request_id: "req-failed",
                error: Some("upstream failed"),
                ..route_payload
            },
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
    fn live_takeover_app_kinds_parse_to_cc_switch_app_types() {
        let live_takeover_apps = live_takeover_app_kinds().map(|app| {
            app.as_str()
                .parse::<AppType>()
                .expect("proxy-core live takeover app kind must be supported by cc-switch")
        });
        assert_eq!(
            live_takeover_apps,
            [AppType::Claude, AppType::Codex, AppType::Gemini]
        );
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
