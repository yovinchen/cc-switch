use futures::future::BoxFuture;
use serde_json::Value;
use std::sync::Arc;

use crate::app_config::AppType;
use crate::proxy::error::ProxyError;
use crate::proxy_core_adapter::*;

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
        provider_apply_codex_chat_upstream_model(input.provider, &mut body);
        let reasoning_options = provider_codex_chat_reasoning_options(input.provider, &body);
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
            provider_bedrock_env_flag(input.provider),
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

        provider_claude_normalize_anthropic_messages(input.body, input.provider, api_format);
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
            && provider_should_convert_codex_responses_to_chat(input.provider, input.endpoint);
        let adapter_facts = input.adapter.facts();
        let fallback_claude_api_format = adapter_facts
            .is_claude_adapter
            .then(|| provider_claude_api_format(input.provider));
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
            self.managed_account_runtime_source
                .apply_copilot_live_model_for_adapter(input.provider, input.body, input.is_copilot)
                .await;
        })
    }

    fn apply_copilot_dynamic_base_url_for_provider<'a>(
        &'a self,
        input: ForwarderCopilotDynamicBaseUrlInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.managed_account_runtime_source
                .apply_copilot_dynamic_base_url_for_provider(
                    input.provider,
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
            self.managed_account_runtime_source
                .resolve_claude_api_format_for_adapter(
                    input.provider,
                    input.body,
                    input.is_copilot,
                    input.adapter.facts().is_claude_adapter,
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
        let custom_user_agent = provider_custom_user_agent_header(input.provider, input.is_copilot);

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
            provider_is_codex_oauth(input.provider),
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
