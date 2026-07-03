use futures::future::BoxFuture;
use serde_json::Value;
use std::sync::Arc;

use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::engine::forward_pipeline::{
    ForwarderAnthropicRectifierGateInput, ForwarderAppMediaPreventionInput,
    ForwarderAttemptBodyInput, ForwarderClaudeApiFormatInput, ForwarderClaudeBodyPolicyInput,
    ForwarderCodexResponsesToChatInput, ForwarderCopilotDynamicBaseUrlInput,
    ForwarderCopilotLiveModelInput, ForwarderCopilotRequestOptimization,
    ForwarderCopilotRequestOptimizationGateInput, ForwarderCopilotRequestOptimizationInput,
    ForwarderMaybeCopilotRequestOptimization, ForwarderMediaPreventionInput,
    ForwarderMediaRetryPlan, ForwarderMediaRetryPlanInput, ForwarderPreparedRequest,
    ForwarderProviderRequestBodyInput, ForwarderProviderTransformInput,
    ForwarderRequestBodyTransform, ForwarderRequestBodyTransformInput, ForwarderRequestPartsInput,
    ForwarderRequestPreparationInput, ForwarderRequestRectifierPlan, ForwarderRequestSource,
    ForwarderRequestSourceRef, ForwarderThinkingBudgetRectifierInput,
    ForwarderThinkingSignatureRectifierInput, ForwarderTransformPlanInput,
    ForwarderUpstreamRequestLogInput, ForwarderUpstreamRequestParts, ForwarderUpstreamUrlInput,
};
use crate::proxy::error::ProxyError;
use crate::proxy::host::cc_switch::claude_desktop_provider::{
    provider_claude_desktop_proxy_request_body, ClaudeDesktopProviderProxyRequestBodyIssue,
    ClaudeDesktopProviderProxyRouteIssue,
};
#[cfg(test)]
use crate::proxy::host::cc_switch::managed_account_runtime_source::default_managed_account_runtime_source;
use crate::proxy::host::cc_switch::managed_account_runtime_source::ManagedAccountRuntimeSourceRef;
use crate::proxy::host::cc_switch::provider_adapter_context::{
    forwarder_provider_adapter_context_for_app, ForwarderAdapterContext,
};
use crate::proxy::host::cc_switch::provider_projection::provider_uses_anthropic_rectifiers;
use crate::proxy::provider::{
    codex_provider_apply_chat_upstream_model, codex_provider_chat_reasoning_options,
    codex_provider_should_convert_responses_to_chat,
};
use crate::proxy_core::api::auth::{
    validate_managed_account_upstream_auth, ClaudeDesktopProxyRequestBodyIssue,
};
use crate::proxy_core::api::config::{
    cache_injection_log_message, normalize_thinking_type, rectify_anthropic_request,
    rectify_thinking_budget, should_rectify_thinking_budget, should_rectify_thinking_signature,
    thinking_optimization_log_message,
};
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::model_catalog::{
    apply_copilot_model_normalization, apply_provider_model_mapping,
    strip_one_m_suffix_for_upstream, strip_one_m_suffix_for_upstream_from_body,
    ModelMappingProjection,
};
use crate::proxy_core::api::transforms::{
    normalize_claude_anthropic_messages, responses_to_chat_completions_with_options,
};
use crate::proxy_core::api::transport::{
    anthropic_beta_header_value, apply_bedrock_pre_send_optimizers,
    apply_copilot_warmup_model_override, apply_forwarder_media_prevention_from_facts,
    apply_resolved_channel_model_override, apply_resolved_channel_request_overrides,
    bedrock_env_flag_from_provider_settings, build_upstream_request_headers,
    classify_copilot_request, forward_upstream_url_plan, forwarder_media_retry_plan_from_facts,
    forwarder_protocol_preparation_from_transform_plan,
    forwarder_rectifier_error_message as core_forwarder_rectifier_error_message,
    forwarder_request_body_model, forwarder_request_body_transform_action_from_plan,
    forwarder_transform_plan_from_facts, is_openai_o_series, is_unsupported_image_error,
    merge_copilot_tool_results, prepare_upstream_request_body_with_report,
    prompt_cache_trace_log_message,
    provider_custom_user_agent_header as core_provider_custom_user_agent_header,
    request_body_filter_log_message, request_body_serialize_error_message,
    resolve_upstream_request_transport_policy, sanitize_copilot_orphan_tool_results,
    serialize_upstream_request_body, should_apply_bedrock_pre_send_optimizer,
    should_apply_forwarder_media_prevention_for_app, should_preserve_exact_request_header_case,
    should_send_anthropic_request_headers, strip_copilot_thinking_blocks,
    supports_reasoning_effort, upstream_host_header_from_url, ForwardUpstreamUrlPlan,
    ForwardUpstreamUrlPlanInput, ForwarderMediaPreventionFacts, ForwarderMediaRetryPlanFacts,
    ForwarderProtocolPreparation, ForwarderProtocolPreparationInput, ForwarderRectifierErrorInput,
    ForwarderRequestBodyTransformAction, ForwarderTransformPlan, ForwarderTransformPlanFacts,
    PromptCacheTraceLogInput, UpstreamRequestHeadersInput, UNSUPPORTED_IMAGE_MARKER,
};

pub(crate) struct CcSwitchForwarderRequestSource {
    managed_account_runtime_source: ManagedAccountRuntimeSourceRef,
}

impl CcSwitchForwarderRequestSource {
    pub(crate) fn new(managed_account_runtime_source: ManagedAccountRuntimeSourceRef) -> Self {
        Self {
            managed_account_runtime_source,
        }
    }

    pub(crate) fn transform_provider_request_body(
        &self,
        input: ForwarderProviderTransformInput<'_>,
    ) -> Result<Value, ProxyError> {
        input
            .adapter
            .transform_provider_request(input.body, input.provider)
    }

    pub(crate) fn convert_codex_responses_to_chat_body(
        &self,
        input: ForwarderCodexResponsesToChatInput<'_>,
    ) -> Value {
        let mut body = input.body;
        codex_provider_apply_chat_upstream_model(input.provider, &mut body);
        let reasoning_options = codex_provider_chat_reasoning_options(input.provider, &body);
        let model = body
            .get("model")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        responses_to_chat_completions_with_options(
            &body,
            reasoning_options.as_ref(),
            is_openai_o_series(model),
            supports_reasoning_effort(model),
        )
    }

    pub(crate) fn optimize_copilot_request(
        &self,
        input: ForwarderCopilotRequestOptimizationInput<'_>,
    ) -> ForwarderCopilotRequestOptimization {
        let has_anthropic_beta = input.headers.contains_key("anthropic-beta");
        let classification = classify_copilot_request(
            &input.body,
            has_anthropic_beta,
            input.config.compact_detection,
            input.config.subagent_detection,
        );

        log::debug!(
            "[Copilot] 优化器分类: initiator={}, is_warmup={}, is_compact={}, is_subagent={}",
            classification.initiator,
            classification.is_warmup,
            classification.is_compact,
            classification.is_subagent
        );

        let mut body = sanitize_copilot_orphan_tool_results(input.body);

        if input.config.tool_result_merging {
            body = merge_copilot_tool_results(body);
        }

        if input.config.strip_thinking {
            body = strip_copilot_thinking_blocks(body);
        }

        let warmup_override = apply_copilot_warmup_model_override(
            body,
            input.config.warmup_downgrade,
            classification.is_warmup,
            &input.config.warmup_model,
        );
        if let Some(warmup_model) = &warmup_override.applied_model {
            log::info!("[Copilot] Warmup 请求降级到模型: {}", warmup_model);
        }

        ForwarderCopilotRequestOptimization {
            body: warmup_override.body,
            classification,
        }
    }
}

fn apply_provider_model_mapping_from_provider(
    body: Value,
    provider: &Provider,
) -> ModelMappingProjection {
    apply_provider_model_mapping(body, &provider.settings_config)
}

fn apply_forward_request_model_mapping_from_provider(
    app_type: &AppType,
    body: Value,
    provider: &Provider,
) -> Result<ModelMappingProjection, ProxyError> {
    if matches!(app_type, AppType::ClaudeDesktop) {
        return provider_claude_desktop_proxy_request_body(body, provider)
            .map(|body| ModelMappingProjection {
                body,
                log_message: None,
            })
            .map_err(|issue| {
                ProxyError::InvalidRequest(claude_desktop_proxy_request_body_issue_message(issue))
            });
    }

    Ok(apply_provider_model_mapping_from_provider(body, provider))
}

fn claude_desktop_proxy_request_body_issue_message(
    issue: ClaudeDesktopProviderProxyRequestBodyIssue,
) -> String {
    match issue {
        ClaudeDesktopProviderProxyRequestBodyIssue::Routes(route_issue) => match route_issue {
            ClaudeDesktopProviderProxyRouteIssue::Missing => {
                "Claude Desktop proxy mode is missing model route mappings".to_string()
            }
            ClaudeDesktopProviderProxyRouteIssue::Empty => {
                "Claude Desktop proxy mode requires at least one model route mapping".to_string()
            }
        },
        ClaudeDesktopProviderProxyRequestBodyIssue::Body(body_issue) => match body_issue {
            ClaudeDesktopProxyRequestBodyIssue::MissingModel => {
                "Claude Desktop request is missing the model field".to_string()
            }
            ClaudeDesktopProxyRequestBodyIssue::UnknownRoute { requested_model } => {
                format!("Claude Desktop model route is not configured: {requested_model}")
            }
        },
    }
}

fn normalize_claude_anthropic_messages_for_provider(
    body: &mut Value,
    provider: &Provider,
    api_format: &str,
) -> bool {
    normalize_claude_anthropic_messages(body, &provider.settings_config, api_format)
}

#[cfg(test)]
pub(crate) fn test_normalize_claude_anthropic_messages_for_provider(
    body: &mut Value,
    provider: &Provider,
    api_format: &str,
) -> bool {
    normalize_claude_anthropic_messages_for_provider(body, provider, api_format)
}

fn apply_forwarder_media_prevention_with_log(input: ForwarderMediaPreventionInput<'_>) -> usize {
    let replaced_images =
        apply_forwarder_media_prevention_from_facts(ForwarderMediaPreventionFacts {
            rectifier_enabled: input.config.enabled,
            request_media_fallback: input.config.request_media_fallback,
            request_media_heuristic: input.config.request_media_heuristic,
            body: input.body,
            provider_settings: &input.provider.settings_config,
        });
    if replaced_images > 0 {
        let model = input
            .body
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("");
        log::info!(
            "[Media] Replaced {replaced_images} image block(s) with {} for text-only provider={}, model={}",
            UNSUPPORTED_IMAGE_MARKER,
            input.provider.id,
            model
        );
    }
    replaced_images
}

impl ForwarderRequestSource for CcSwitchForwarderRequestSource {
    fn adapter_context_for_app(&self, app_type: &AppType) -> ForwarderAdapterContext {
        forwarder_provider_adapter_context_for_app(app_type)
    }

    fn prepare_attempt_body(&self, input: ForwarderAttemptBodyInput<'_>) -> Value {
        if !should_apply_bedrock_pre_send_optimizer(
            input.config.enabled,
            bedrock_env_flag_from_provider_settings(&input.provider.settings_config),
        ) {
            return input.body.clone();
        }

        let mut body = input.body.clone();
        let report = apply_bedrock_pre_send_optimizers(&mut body, input.config);
        if let Some(message) = report
            .thinking
            .as_ref()
            .and_then(thinking_optimization_log_message)
        {
            log::info!("{message}");
        }
        if let Some(message) = report.cache.as_ref().and_then(cache_injection_log_message) {
            log::info!("{message}");
        }
        body
    }

    fn prepare_provider_request_body(
        &self,
        input: ForwarderProviderRequestBodyInput<'_>,
    ) -> Result<Value, ProxyError> {
        let projection = apply_forward_request_model_mapping_from_provider(
            input.app_type,
            input.body,
            input.provider,
        )?;
        if let Some(message) = projection.log_message {
            log::debug!("{message}");
        }

        let mut body = normalize_thinking_type(projection.body);

        if let Some(channel) = input.channel {
            if let Some(override_result) = apply_resolved_channel_model_override(&mut body, channel)
            {
                log::debug!(
                    "[ChannelRoute] model override via channel {}: {} -> {}",
                    override_result.channel_id,
                    override_result.previous_model,
                    override_result.upstream_model
                );
            }
            if let Some(application) = apply_resolved_channel_request_overrides(&mut body, channel)
            {
                log::debug!(
                    "[ChannelRoute] request overrides via channel {}: {:?}",
                    application.channel_id,
                    application.applied_keys
                );
            }
        }

        if input.is_copilot {
            let original_model = body
                .get("model")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
            body = apply_copilot_model_normalization(body);
            if let (Some(original), Some(normalized)) = (
                original_model.as_deref(),
                body.get("model").and_then(|value| value.as_str()),
            ) {
                if original != normalized {
                    log::debug!("[CopilotNormalizer] {original} -> {normalized}");
                }
            }
            return Ok(body);
        }

        let one_m_model_change = body.get("model").and_then(Value::as_str).and_then(|model| {
            let stripped = strip_one_m_suffix_for_upstream(model);
            (stripped != model).then(|| (model.to_string(), stripped.to_string()))
        });
        body = strip_one_m_suffix_for_upstream_from_body(body);
        if let Some((model, stripped)) = one_m_model_change {
            log::debug!("[ModelMapper] 去除本地 1M 标记: {model} → {stripped}");
        }

        Ok(body)
    }

    fn apply_claude_body_policies(&self, input: ForwarderClaudeBodyPolicyInput<'_>) {
        if !input.adapter.facts().is_claude_adapter {
            return;
        }
        let Some(api_format) = input.api_format else {
            return;
        };

        normalize_claude_anthropic_messages_for_provider(input.body, input.provider, api_format);
        apply_forwarder_media_prevention_with_log(ForwarderMediaPreventionInput {
            body: input.body,
            provider: input.provider,
            config: input.config,
        });
    }

    fn transform_request_body(
        &self,
        input: ForwarderRequestBodyTransformInput<'_>,
    ) -> Result<ForwarderRequestBodyTransform, ProxyError> {
        let outbound_model = forwarder_request_body_model(&input.body);
        let action = forwarder_request_body_transform_action_from_plan(
            input.transform_plan,
            input.claude_transformed_body.is_some(),
        );
        let body = match action {
            ForwarderRequestBodyTransformAction::ConvertCodexResponsesToChat => self
                .convert_codex_responses_to_chat_body(ForwarderCodexResponsesToChatInput {
                    body: input.body,
                    provider: input.provider,
                }),
            ForwarderRequestBodyTransformAction::UseClaudeTransformedBody => {
                input.claude_transformed_body.unwrap_or(input.body)
            }
            ForwarderRequestBodyTransformAction::ApplyProviderTransform => self
                .transform_provider_request_body(ForwarderProviderTransformInput {
                    adapter: input.adapter,
                    body: input.body,
                    provider: input.provider,
                })?,
            ForwarderRequestBodyTransformAction::Passthrough => input.body,
        };

        Ok(ForwarderRequestBodyTransform {
            body,
            outbound_model,
        })
    }

    fn transform_plan(&self, input: ForwarderTransformPlanInput<'_>) -> ForwarderTransformPlan {
        let codex_responses_to_chat = matches!(input.app_type, AppType::Codex)
            && codex_provider_should_convert_responses_to_chat(input.provider, input.endpoint);
        let adapter_facts = input.adapter.facts();
        let fallback_claude_api_format = input.adapter.fallback_claude_api_format(input.provider);
        let provider_transform_required = input.resolved_claude_api_format.is_none()
            && input.adapter.provider_transform_required(input.provider);

        forwarder_transform_plan_from_facts(ForwarderTransformPlanFacts {
            codex_responses_to_chat,
            adapter_is_claude: adapter_facts.is_claude_adapter,
            resolved_claude_api_format: input.resolved_claude_api_format,
            fallback_claude_api_format,
            provider_transform_required,
        })
    }

    fn protocol_preparation(
        &self,
        input: ForwarderProtocolPreparationInput<'_>,
    ) -> ForwarderProtocolPreparation {
        forwarder_protocol_preparation_from_transform_plan(input)
    }

    fn plan_upstream_url(&self, input: ForwarderUpstreamUrlInput<'_>) -> ForwardUpstreamUrlPlan {
        forward_upstream_url_plan(
            ForwardUpstreamUrlPlanInput {
                base_url: input.base_url,
                endpoint: input.endpoint,
                is_full_url: input.is_full_url,
                codex_responses_to_chat: input.transform_plan.codex_responses_to_chat,
                use_claude_transform: input.transform_plan.use_claude_transform,
                is_copilot: input.is_copilot,
                claude_api_format: input.transform_plan.claude_api_format_for_url.as_deref(),
                body: input.body,
                channel_param_overrides: input.channel_param_overrides,
            },
            |base_url, effective_endpoint| {
                input
                    .adapter
                    .provider_upstream_url(base_url, effective_endpoint)
            },
        )
    }

    fn prepare_copilot_request_optimization(
        &self,
        input: ForwarderCopilotRequestOptimizationGateInput<'_>,
    ) -> ForwarderMaybeCopilotRequestOptimization {
        if !input.is_copilot || !input.config.enabled {
            return ForwarderMaybeCopilotRequestOptimization {
                body: input.body,
                classification: None,
            };
        }

        let optimized = self.optimize_copilot_request(ForwarderCopilotRequestOptimizationInput {
            body: input.body,
            headers: input.headers,
            config: input.config,
        });

        ForwarderMaybeCopilotRequestOptimization {
            body: optimized.body,
            classification: Some(optimized.classification),
        }
    }

    fn apply_copilot_live_model_for_adapter<'a>(
        &'a self,
        input: ForwarderCopilotLiveModelInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            input
                .adapter
                .apply_copilot_live_model_for_adapter(
                    input.provider,
                    &self.managed_account_runtime_source,
                    input.body,
                    input.is_copilot,
                )
                .await;
        })
    }

    fn apply_copilot_dynamic_base_url_for_provider<'a>(
        &'a self,
        input: ForwarderCopilotDynamicBaseUrlInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            input
                .adapter
                .apply_copilot_dynamic_base_url_for_provider(
                    input.provider,
                    &self.managed_account_runtime_source,
                    input.base_url,
                    input.is_copilot,
                    input.is_full_url,
                )
                .await;
        })
    }

    fn resolve_claude_api_format_for_adapter<'a>(
        &'a self,
        input: ForwarderClaudeApiFormatInput<'a>,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            input
                .adapter
                .resolve_claude_api_format_for_adapter(
                    input.provider,
                    &self.managed_account_runtime_source,
                    input.body,
                    input.is_copilot,
                )
                .await
        })
    }

    fn apply_app_media_prevention(&self, input: ForwarderAppMediaPreventionInput<'_>) -> usize {
        if !should_apply_forwarder_media_prevention_for_app(&AppKind::from(input.app_type)) {
            return 0;
        }

        apply_forwarder_media_prevention_with_log(ForwarderMediaPreventionInput {
            body: input.body,
            provider: input.provider,
            config: input.config,
        })
    }

    fn media_retry_plan(
        &self,
        input: ForwarderMediaRetryPlanInput<'_>,
    ) -> Option<ForwarderMediaRetryPlan> {
        let adapter_facts = input.adapter.facts();
        let unsupported_image_error = match input.error {
            ProxyError::UpstreamError { status, body } => {
                is_unsupported_image_error(*status, body.as_deref())
            }
            _ => false,
        };

        let retry_plan = forwarder_media_retry_plan_from_facts(ForwarderMediaRetryPlanFacts {
            adapter_name: adapter_facts.adapter_name,
            rectifier_enabled: input.config.enabled,
            request_media_fallback: input.config.request_media_fallback,
            already_retried: input.already_retried,
            provider_body: input.provider_body,
            unsupported_image_error,
        })?;

        let model = retry_plan
            .body
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("");
        log::info!(
            "[{}] [Media] Upstream rejected image input; retrying provider={} model={} with {} image block(s) replaced by {}",
            input.app,
            input.provider.id,
            model,
            retry_plan.replaced_images,
            UNSUPPORTED_IMAGE_MARKER
        );

        Some(ForwarderMediaRetryPlan {
            body: retry_plan.body,
        })
    }

    fn anthropic_rectifiers_enabled(
        &self,
        input: ForwarderAnthropicRectifierGateInput<'_>,
    ) -> bool {
        provider_uses_anthropic_rectifiers(input.app_type, input.provider)
    }

    fn thinking_signature_rectifier_plan(
        &self,
        input: ForwarderThinkingSignatureRectifierInput<'_>,
    ) -> ForwarderRequestRectifierPlan {
        let error_message = forwarder_rectifier_error_message(input.error);
        if !should_rectify_thinking_signature(
            error_message.as_deref(),
            &input.config.thinking_signature_core_config(),
        ) {
            return ForwarderRequestRectifierPlan::NotTriggered;
        }

        if input.already_retried {
            log::warn!("[{}] [RECT-005] 整流器已触发过，不再重试", input.app);
            return ForwarderRequestRectifierPlan::AlreadyRetried;
        }

        let rectified = rectify_anthropic_request(input.body);
        if !rectified.applied {
            log::warn!(
                "[{}] [RECT-006] thinking 签名整流器触发但无可整流内容，继续检查 budget；若 budget 也未命中则按客户端错误返回",
                input.app
            );
            return ForwarderRequestRectifierPlan::TriggeredUnchanged;
        }

        log::info!(
            "[{}] [RECT-001] thinking 签名整流器触发, 移除 {} thinking blocks, {} redacted_thinking blocks, {} signature fields",
            input.app,
            rectified.removed_thinking_blocks,
            rectified.removed_redacted_thinking_blocks,
            rectified.removed_signature_fields
        );
        ForwarderRequestRectifierPlan::Retry
    }

    fn thinking_budget_rectifier_plan(
        &self,
        input: ForwarderThinkingBudgetRectifierInput<'_>,
    ) -> ForwarderRequestRectifierPlan {
        let error_message = forwarder_rectifier_error_message(input.error);
        if !should_rectify_thinking_budget(
            error_message.as_deref(),
            &input.config.thinking_budget_core_config(),
        ) {
            return ForwarderRequestRectifierPlan::NotTriggered;
        }

        if input.already_retried {
            log::warn!("[{}] [RECT-013] budget 整流器已触发过，不再重试", input.app);
            return ForwarderRequestRectifierPlan::AlreadyRetried;
        }

        let budget_rectified = rectify_thinking_budget(input.body);
        if !budget_rectified.applied {
            log::warn!(
                "[{}] [RECT-014] budget 整流器触发但无可整流内容，不做无意义重试",
                input.app
            );
            return ForwarderRequestRectifierPlan::TriggeredUnchanged;
        }

        log::info!(
            "[{}] [RECT-010] thinking budget 整流器触发, before={:?}, after={:?}",
            input.app,
            budget_rectified.before,
            budget_rectified.after
        );
        ForwarderRequestRectifierPlan::Retry
    }

    fn prepare_upstream_body(
        &self,
        input: ForwarderRequestPreparationInput<'_>,
    ) -> ForwarderPreparedRequest {
        let prepared_body = prepare_upstream_request_body_with_report(input.body);
        if let Some(message) = request_body_filter_log_message(&prepared_body) {
            log::debug!("{message}");
        }
        let filtered_body = prepared_body.body;

        if log::log_enabled!(log::Level::Debug) {
            log::debug!(
                "{}",
                prompt_cache_trace_log_message(PromptCacheTraceLogInput {
                    app: input.app,
                    provider_id: input.provider_id,
                    endpoint: input.endpoint,
                    api_format: input.api_format,
                    body: &filtered_body,
                    session_client_provided: input.session_client_provided,
                })
            );
        }

        let body_model = forwarder_request_body_model(&filtered_body);
        let body_model_label = body_model.clone().unwrap_or_else(|| "<none>".to_string());
        let outbound_model = body_model.clone().or(input.initial_outbound_model);

        let transport_policy = resolve_upstream_request_transport_policy(
            input.transform_plan.needs_transform,
            input.transform_plan.codex_responses_to_chat,
            input.endpoint,
            &filtered_body,
            input.headers,
        );

        ForwarderPreparedRequest {
            body: filtered_body,
            request_is_streaming: transport_policy.is_streaming_request,
            force_identity_encoding: transport_policy.force_identity_encoding,
            body_model_label,
            outbound_model,
        }
    }

    fn log_upstream_request(&self, input: ForwarderUpstreamRequestLogInput<'_>) {
        let tag = input.adapter.facts().adapter_name;
        let url = input.url;
        let request_model = &input.prepared_request.body_model_label;

        log::info!("[{tag}] >>> 请求 URL: {url} (model={request_model})");
        if log::log_enabled!(log::Level::Debug) {
            if let Ok(body_str) = serde_json::to_string(&input.prepared_request.body) {
                log::debug!(
                    "[{tag}] >>> 请求体内容 ({}字节): {}",
                    body_str.len(),
                    body_str
                );
            }
        }
    }

    fn build_upstream_request_parts(
        &self,
        input: ForwarderRequestPartsInput<'_>,
    ) -> Result<ForwarderUpstreamRequestParts, ProxyError> {
        let adapter_facts = input.adapter.facts();
        let upstream_host = upstream_host_header_from_url(input.url);
        let should_send_anthropic_headers = should_send_anthropic_request_headers(
            adapter_facts.adapter_name,
            input.resolved_claude_api_format,
        );
        let anthropic_beta_value = if should_send_anthropic_headers {
            Some(anthropic_beta_header_value(
                input
                    .inbound_headers
                    .get("anthropic-beta")
                    .and_then(|beta| beta.to_str().ok()),
            ))
        } else {
            None
        };
        let custom_user_agent =
            custom_user_agent_header_for_provider(input.provider, input.is_copilot);

        let ordered_headers = build_upstream_request_headers(UpstreamRequestHeadersInput {
            inbound_headers: input.inbound_headers,
            upstream_host: upstream_host.as_deref(),
            auth_headers: input.auth_headers,
            channel_header_overrides: input.channel_header_overrides,
            force_identity_encoding: input.prepared_request.force_identity_encoding,
            custom_user_agent: custom_user_agent.as_ref(),
            is_copilot: input.is_copilot,
            should_send_anthropic_headers,
            anthropic_beta_value: anthropic_beta_value.as_deref(),
            codex_oauth_session_headers: input.codex_oauth_session_headers,
            ensure_json_content_type: true,
        });

        let body = serialize_upstream_request_body(input.method, &input.prepared_request.body)
            .map_err(|error| ProxyError::Internal(request_body_serialize_error_message(error)))?;

        validate_managed_account_upstream_auth(input.url, &ordered_headers)
            .map_err(|error| ProxyError::AuthError(error.to_string()))?;

        let preserve_exact_header_case = should_preserve_exact_request_header_case(
            adapter_facts.adapter_name,
            input.provider.is_codex_oauth(),
            input.is_copilot,
            input.resolved_claude_api_format,
        );

        Ok(ForwarderUpstreamRequestParts {
            ordered_headers,
            body,
            preserve_exact_header_case,
        })
    }
}

pub(crate) fn forwarder_rectifier_error_message(error: &ProxyError) -> Option<String> {
    match error {
        ProxyError::UpstreamError { body, .. } => {
            core_forwarder_rectifier_error_message(ForwarderRectifierErrorInput::Upstream {
                body: body.as_deref(),
            })
        }
        _ => {
            let message = error.to_string();
            core_forwarder_rectifier_error_message(ForwarderRectifierErrorInput::Other {
                message: &message,
            })
        }
    }
}

fn custom_user_agent_header_for_provider(
    provider: &Provider,
    is_copilot: bool,
) -> Option<http::HeaderValue> {
    let raw = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.custom_user_agent.as_deref());
    core_provider_custom_user_agent_header(raw, is_copilot)
        .ok()
        .flatten()
}

pub(crate) fn forwarder_request_source_from_managed_account_runtime_source(
    managed_account_runtime_source: ManagedAccountRuntimeSourceRef,
) -> ForwarderRequestSourceRef {
    Arc::new(CcSwitchForwarderRequestSource::new(
        managed_account_runtime_source,
    ))
}

#[cfg(test)]
pub(crate) fn default_forwarder_request_source() -> ForwarderRequestSourceRef {
    forwarder_request_source_from_managed_account_runtime_source(
        default_managed_account_runtime_source(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ClaudeDesktopMode, ClaudeDesktopModelRoute, Provider, ProviderMeta};
    use crate::proxy_core::api::ports::{CopilotOptimizerConfig, OptimizerConfig, RectifierConfig};
    use crate::proxy_core::api::routing::{
        resolved_channel_attempt_from_candidate, ChannelRouteCandidate, ResolvedChannelAttempt,
    };
    use http::HeaderMap;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn custom_user_agent_header_uses_core_policy_and_suppresses_copilot() {
        let mut provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            custom_user_agent: Some("cc-switch-test/1.0".to_string()),
            ..ProviderMeta::default()
        });

        let header =
            custom_user_agent_header_for_provider(&provider, false).expect("custom user agent");

        assert_eq!(header, http::HeaderValue::from_static("cc-switch-test/1.0"));
        assert!(custom_user_agent_header_for_provider(&provider, true).is_none());
    }

    #[test]
    fn forwarder_request_source_selects_adapter_for_app() {
        let source = default_forwarder_request_source();

        let claude_adapter = source.adapter_context_for_app(&AppType::Claude);
        let fallback_adapter = source.adapter_context_for_app(&AppType::Hermes);

        assert_eq!(claude_adapter.facts().adapter_name, "Claude");
        assert_eq!(fallback_adapter.facts().adapter_name, "Codex");
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
        let channel = resolved_channel_attempt_from_candidate(ChannelRouteCandidate {
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
        });

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
    fn provider_request_body_applies_channel_request_overrides_after_model_override() {
        let source = CcSwitchForwarderRequestSource::new(default_managed_account_runtime_source());
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        let channel = ResolvedChannelAttempt {
            channel_id: "channel-a".to_string(),
            channel_name: "Relay A".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_responses".to_string(),
            auth_profile_ref: None,
            public_model: Some("sonnet-public".to_string()),
            upstream_model: Some("upstream-sonnet".to_string()),
            pricing_model: None,
            header_overrides: json!({}),
            param_overrides: json!({}),
            status_code_mapping: json!([]),
            request_overrides: json!({
                "stream": false,
                "temperature": 0.2
            }),
            response_overrides: json!({}),
            retry_policy: json!({}),
        };

        let body = source
            .prepare_provider_request_body(ForwarderProviderRequestBodyInput {
                app_type: &AppType::Claude,
                body: json!({
                    "model": "sonnet-public",
                    "stream": true,
                    "temperature": 0.9
                }),
                provider: &provider,
                channel: Some(&channel),
                is_copilot: false,
            })
            .expect("provider request body");

        assert_eq!(body["model"], "upstream-sonnet");
        assert_eq!(body["stream"], json!(false));
        assert_eq!(body["temperature"], json!(0.2));
    }

    #[test]
    fn provider_request_body_projects_model_mapping_and_desktop_routes() {
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

        let projection = apply_provider_model_mapping_from_provider(
            json!({"model": "claude-sonnet", "messages": []}),
            &provider,
        );

        assert_eq!(
            projection.body.get("model").and_then(Value::as_str),
            Some("sonnet-mapped")
        );
        assert_eq!(
            projection.log_message.as_deref(),
            Some("[ModelMapper] 模型映射: claude-sonnet \u{2192} sonnet-mapped")
        );

        let unchanged = apply_provider_model_mapping_from_provider(
            json!({"model": "unknown"}),
            &Provider::with_id(
                "provider-b".to_string(),
                "Provider B".to_string(),
                json!({}),
                None,
            ),
        );
        assert_eq!(
            unchanged.body.get("model").and_then(Value::as_str),
            Some("unknown")
        );
        assert!(unchanged.log_message.is_none());

        let mut desktop_provider = Provider::with_id(
            "desktop-proxy".to_string(),
            "Desktop Proxy".to_string(),
            json!({}),
            None,
        );
        desktop_provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            claude_desktop_model_routes: HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-sonnet".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            )]),
            ..ProviderMeta::default()
        });
        let desktop_projection = apply_forward_request_model_mapping_from_provider(
            &AppType::ClaudeDesktop,
            json!({"model": "claude-sonnet-4-6", "messages": []}),
            &desktop_provider,
        )
        .expect("desktop model mapping");
        assert_eq!(
            desktop_projection.body.get("model").and_then(Value::as_str),
            Some("upstream-sonnet")
        );
        assert!(desktop_projection.log_message.is_none());
        let desktop_error = apply_forward_request_model_mapping_from_provider(
            &AppType::ClaudeDesktop,
            json!({"model": "unknown-route", "messages": []}),
            &desktop_provider,
        )
        .expect_err("unknown desktop route");
        assert!(matches!(
            desktop_error,
            ProxyError::InvalidRequest(message) if message.contains("unknown-route")
        ));
    }
}
