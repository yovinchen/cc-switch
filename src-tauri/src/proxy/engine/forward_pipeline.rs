//! 请求转发器
//!
//! 负责将请求转发到上游Provider，支持故障转移

use crate::proxy::host::cc_switch::provider_adapter_context::ForwarderAdapterContext;
#[cfg(test)]
use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
use crate::proxy::{
    error::ProxyError, route_attempt::ForwardAttempt,
    transport::upstream::hyper_client::ProxyResponse,
};
use crate::proxy_core::api::ports::{CopilotOptimizerConfig, OptimizerConfig, RectifierConfig};
#[cfg(test)]
use crate::proxy_core_adapter::{
    prepare_upstream_request_body_with_report, provider_bedrock_env_flag, provider_is_codex_oauth,
    should_preserve_exact_request_header_case, validate_managed_account_upstream_auth,
};
use crate::proxy_core_adapter::{
    ActiveConnectionGuard, FailoverSwitchSchedulerRef, ForwarderAnthropicRectifierGateInput,
    ForwarderAppMediaPreventionInput, ForwarderAttemptAllowDecision, ForwarderAttemptAllowInput,
    ForwarderAttemptBodyInput, ForwarderAttemptRuntimeSourceRef, ForwarderAuthHeadersInput,
    ForwarderAuthSourceRef, ForwarderChannelResponseStatusInput, ForwarderClaudeApiFormatInput,
    ForwarderClaudeBodyPolicyInput, ForwarderClaudeProtocolTransformInput,
    ForwarderCodexChatProtocolEnrichmentInput, ForwarderCopilotDynamicBaseUrlInput,
    ForwarderCopilotLiveModelInput, ForwarderCopilotRequestOptimizationGateInput,
    ForwarderFailureDecision, ForwarderMaybeCopilotAuthOptimizationInput,
    ForwarderMediaRetryPlanInput, ForwarderProtocolPreparationInput,
    ForwarderProtocolStateSourceRef, ForwarderProviderRequestBodyInput, ForwarderProviderUrlFacts,
    ForwarderRectifierRetryFailureDecision, ForwarderRectifierRetryKind,
    ForwarderRequestBodyTransformInput, ForwarderRequestPartsInput,
    ForwarderRequestPreparationInput, ForwarderRequestRectifierPlan, ForwarderRequestSourceRef,
    ForwarderResponseFinalizationInput, ForwarderResponseSourceRef, ForwarderRuntimeConfig,
    ForwarderRuntimeStateSourceRef, ForwarderThinkingBudgetRectifierInput,
    ForwarderThinkingSignatureRectifierInput, ForwarderTransformPlanInput,
    ForwarderTransportSourceRef, ForwarderUpstreamRequestLogInput,
    ForwarderUpstreamTransportRequest, ForwarderUpstreamUrlInput, ResolvedChannelAttempt,
};
use crate::{app_config::AppType, provider::Provider};
use http::Extensions;
use serde_json::Value;

pub struct ForwardResult {
    pub response: ProxyResponse,
    pub provider: Provider,
    pub claude_api_format: Option<String>,
    /// 实际发往上游的模型名（路由接管/模型映射后的真值）。
    ///
    /// usage 归因不能依赖 ctx.request_model（映射前的客户端别名）：上游响应
    /// 缺失 model 或回显别名时，接管流量会被记成 claude-* 并按其定价计费。
    pub outbound_model: Option<String>,
    /// 实际成功的 channel，用于 core adapter 把结果映射回真实路由选择。
    pub(crate) selected_channel: Option<ResolvedChannelAttempt>,
    /// 活跃连接 RAII guard：随响应一起流转到 response_processor / handle_claude_transform，
    /// 最终被 move 进流式 body future（或非流式响应作用域），覆盖整个响应生命周期。
    pub(crate) connection_guard: Option<ActiveConnectionGuard>,
}

struct ForwarderUpstreamSuccess {
    response: ProxyResponse,
    claude_api_format: Option<String>,
    outbound_model: Option<String>,
}

pub struct ForwardError {
    pub error: ProxyError,
}

pub struct RequestForwarder {
    attempt_runtime_source: ForwarderAttemptRuntimeSourceRef,
    protocol_state_source: ForwarderProtocolStateSourceRef,
    runtime_state_source: ForwarderRuntimeStateSourceRef,
    auth_source: ForwarderAuthSourceRef,
    request_source: ForwarderRequestSourceRef,
    transport_source: ForwarderTransportSourceRef,
    response_source: ForwarderResponseSourceRef,
    failover_switch_scheduler: FailoverSwitchSchedulerRef,
    /// 请求开始时的"当前供应商 ID"（用于判断是否需要同步 UI/托盘）
    current_provider_id_at_start: String,
    /// 代理会话 ID（用于 Gemini Native shadow replay）
    session_id: String,
    /// Session ID 是否由客户端提供；生成值不能作为上游缓存身份。
    session_client_provided: bool,
    /// 整流器配置
    rectifier_config: RectifierConfig,
    /// 优化器配置
    optimizer_config: OptimizerConfig,
    /// Copilot 优化器配置
    copilot_optimizer_config: CopilotOptimizerConfig,
    /// 非流式请求超时（秒）
    non_streaming_timeout: std::time::Duration,
    /// 流式请求响应头等待超时（秒）
    streaming_first_byte_timeout: std::time::Duration,
    /// 单个客户端请求最多尝试的 provider 数。
    ///
    /// 由 `AppProxyConfig.max_retries` (UI: "请求失败时的重试次数, 0-10") 派生：
    /// `max_attempts = max_retries + 1`，所以 max_retries=0 表示仅尝试一家、
    /// max_retries=3（默认）表示最多 4 家。loop 同时受 providers.len() 自然限制。
    max_attempts: usize,
}

impl RequestForwarder {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_preplanned(
        attempt_runtime_source: ForwarderAttemptRuntimeSourceRef,
        protocol_state_source: ForwarderProtocolStateSourceRef,
        runtime_state_source: ForwarderRuntimeStateSourceRef,
        auth_source: ForwarderAuthSourceRef,
        request_source: ForwarderRequestSourceRef,
        transport_source: ForwarderTransportSourceRef,
        response_source: ForwarderResponseSourceRef,
        failover_switch_scheduler: FailoverSwitchSchedulerRef,
        runtime_config: ForwarderRuntimeConfig,
        current_provider_id_at_start: String,
        session_id: String,
        session_client_provided: bool,
    ) -> Self {
        let ForwarderRuntimeConfig {
            options,
            rectifier,
            optimizer,
            copilot_optimizer,
        } = runtime_config;
        // max_retries 是「失败后重试次数」语义，attempt 上限 = retries + 1。
        // saturating_add 防止 u32::MAX + 1 溢出。
        let max_attempts = (options.max_retries as usize).saturating_add(1);
        Self {
            attempt_runtime_source,
            protocol_state_source,
            runtime_state_source,
            auth_source,
            request_source,
            transport_source,
            response_source,
            failover_switch_scheduler,
            current_provider_id_at_start,
            session_id,
            session_client_provided,
            rectifier_config: rectifier,
            optimizer_config: optimizer,
            copilot_optimizer_config: copilot_optimizer,
            non_streaming_timeout: std::time::Duration::from_secs(options.non_streaming_timeout),
            streaming_first_byte_timeout: std::time::Duration::from_secs(
                options.streaming_first_byte_timeout,
            ),
            max_attempts,
        }
    }

    async fn record_success_result(
        &self,
        request_id: &str,
        attempt: &ForwardAttempt,
        app_type: &str,
        used_half_open_permit: bool,
    ) {
        self.attempt_runtime_source
            .record_success(attempt, app_type, used_half_open_permit)
            .await;
        self.runtime_state_source
            .emit_attempt_succeeded(request_id, app_type, attempt);
    }

    async fn record_success_status_and_maybe_switch(&self, app_type: &str, provider: &Provider) {
        let switch_target = self
            .runtime_state_source
            .record_success_status(self.current_provider_id_at_start.as_str(), provider)
            .await;
        if let Some(target) = switch_target {
            self.failover_switch_scheduler
                .schedule_switch(app_type, target);
        }
    }

    async fn complete_successful_attempt(
        &self,
        request_id: &str,
        app_type: &str,
        attempt: &ForwardAttempt,
        used_half_open_permit: bool,
        success: ForwarderUpstreamSuccess,
    ) -> ForwardResult {
        self.record_success_result(request_id, attempt, app_type, used_half_open_permit)
            .await;
        self.runtime_state_source
            .record_active_route_target(request_id, app_type, attempt)
            .await;
        self.record_success_status_and_maybe_switch(app_type, attempt.provider())
            .await;

        ForwardResult {
            response: success.response,
            provider: attempt.provider().clone(),
            claude_api_format: success.claude_api_format,
            outbound_model: success.outbound_model,
            selected_channel: attempt.channel().cloned(),
            connection_guard: None,
        }
    }

    async fn record_failure_result(
        &self,
        request_id: &str,
        attempt: &ForwardAttempt,
        app_type: &str,
        used_half_open_permit: bool,
        error: &ProxyError,
    ) {
        self.attempt_runtime_source
            .record_failure(attempt, app_type, used_half_open_permit, error)
            .await;
        self.runtime_state_source
            .emit_attempt_failed_for_error(request_id, app_type, attempt, error);
    }

    async fn release_attempt_permit_neutral(
        &self,
        attempt: &ForwardAttempt,
        app_type: &str,
        used_half_open_permit: bool,
    ) {
        self.attempt_runtime_source
            .release_attempt_permit_neutral(attempt, app_type, used_half_open_permit)
            .await;
    }

    /// 整流（thinking signature 或 budget）重试失败后的统一收尾。
    ///
    /// `None` 表示已记录熔断器并累积 `last_error`，
    /// 调用方应 `continue` 让下一家 provider 继续故障转移；
    /// `Some(ForwardError)` 表示是客户端错误，没有 provider 能修复，
    /// 调用方应直接 `return` 把错误返回给客户端。
    #[allow(clippy::too_many_arguments)]
    async fn handle_rectifier_retry_failure(
        &self,
        retry_err: ProxyError,
        request_id: &str,
        attempt: &ForwardAttempt,
        app_type_str: &str,
        used_half_open_permit: bool,
        retry_kind: ForwarderRectifierRetryKind,
        last_error: &mut Option<ProxyError>,
    ) -> Option<ForwardError> {
        let provider = attempt.provider();
        match self
            .runtime_state_source
            .rectifier_retry_failure_decision(&retry_err)
        {
            ForwarderRectifierRetryFailureDecision::ProviderFailure => {
                self.record_failure_result(
                    request_id,
                    attempt,
                    app_type_str,
                    used_half_open_permit,
                    &retry_err,
                )
                .await;
                self.runtime_state_source
                    .record_provider_rectifier_retry_failure(provider, retry_kind, &retry_err)
                    .await;
                *last_error = Some(retry_err);
                None
            }
            ForwarderRectifierRetryFailureDecision::ClientFailure => {
                self.release_attempt_permit_neutral(attempt, app_type_str, used_half_open_permit)
                    .await;
                self.runtime_state_source
                    .record_forward_error_status(&retry_err)
                    .await;
                Some(ForwardError { error: retry_err })
            }
        }
    }

    /// Forward a request using attempts planned by a caller such as ProxyEngine.
    ///
    /// This keeps request-scope accounting in one place while allowing the route
    /// planning step to move out of RequestForwarder incrementally.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn forward_with_preplanned_attempts(
        &self,
        app_type: &AppType,
        method: http::Method,
        endpoint: &str,
        body: Value,
        headers: axum::http::HeaderMap,
        extensions: Extensions,
        attempts: Vec<ForwardAttempt>,
    ) -> Result<ForwardResult, ForwardError> {
        let request_id = self.runtime_state_source.next_request_id();
        self.runtime_state_source
            .emit_request_started(&request_id, app_type.as_str());
        let guard = ActiveConnectionGuard::acquire(self.runtime_state_source.clone()).await;
        self.runtime_state_source.record_request_started_now().await;
        let result = self
            .forward_preplanned_attempts_inner(
                &request_id,
                app_type,
                method,
                endpoint,
                body,
                headers,
                extensions,
                attempts,
            )
            .await;

        result.map(|mut fr| {
            fr.connection_guard = Some(guard);
            fr
        })
    }

    /// 实际转发逻辑（不包含客户端维度的入口/出口计数，也不构建 route attempts）。
    #[allow(clippy::too_many_arguments)]
    async fn forward_preplanned_attempts_inner(
        &self,
        request_id: &str,
        app_type: &AppType,
        method: http::Method,
        endpoint: &str,
        body: Value,
        headers: axum::http::HeaderMap,
        extensions: Extensions,
        attempts: Vec<ForwardAttempt>,
    ) -> Result<ForwardResult, ForwardError> {
        // 获取适配器上下文，底层 ProviderAdapter 保持在 adapter 边界内。
        let adapter = self.request_source.adapter_context_for_app(app_type);
        let app_type_str = app_type.as_str();

        if attempts.is_empty() {
            return Err(ForwardError {
                error: ProxyError::NoAvailableProvider,
            });
        }

        let mut last_error = None;
        let mut attempted_providers = 0usize;

        // 依次尝试每个供应商
        for attempt in attempts.iter() {
            let provider = attempt.provider();
            // 整流器重试标记：每个 provider 独立持有，避免标记跨 provider 短路故障转移
            // —— 首家 provider 整流后被 5xx/timeout 击落时，下家仍能用整流后的请求体走整流流程
            let mut rectifier_retried = false;
            let mut budget_rectifier_retried = false;
            let mut media_rectifier_retried = false;

            // 发起请求前先获取熔断器放行许可（HalfOpen 会占用探测名额）
            // 单 Provider 场景下跳过此检查，避免熔断器阻塞所有请求
            let used_half_open_permit = match self
                .attempt_runtime_source
                .allow(ForwarderAttemptAllowInput {
                    attempt,
                    app_type: app_type_str,
                    attempts: &attempts,
                    attempted_providers,
                    max_attempts: self.max_attempts,
                })
                .await
            {
                ForwarderAttemptAllowDecision::Stop => break,
                ForwarderAttemptAllowDecision::Skipped => continue,
                ForwarderAttemptAllowDecision::Allowed {
                    used_half_open_permit,
                } => used_half_open_permit,
            };
            self.runtime_state_source
                .emit_attempt_started(request_id, app_type_str, attempt);

            let mut provider_body =
                self.request_source
                    .prepare_attempt_body(ForwarderAttemptBodyInput {
                        body: &body,
                        provider,
                        config: &self.optimizer_config,
                    });

            attempted_providers += 1;

            // 更新状态中的当前 Provider 信息（per-attempt 维度的标识）
            //
            // total_requests / last_request_at / active_connections 已由
            // forward_with_preplanned_attempts 在客户端请求维度统一处理，这里只刷
            // 新「正在尝试哪个 provider」的展示字段。
            self.runtime_state_source
                .record_current_provider(provider)
                .await;

            // 转发请求（每个 Provider 只尝试一次，重试由客户端控制）
            match self
                .forward(
                    app_type,
                    &method,
                    attempt,
                    endpoint,
                    &provider_body,
                    &headers,
                    &extensions,
                    &adapter,
                )
                .await
            {
                Ok(success) => {
                    // 成功：普通闭合熔断状态异步记录，避免阻塞流式首包返回；
                    // HalfOpen 探测仍同步等待，保证 permit 与熔断状态及时释放。
                    return Ok(self
                        .complete_successful_attempt(
                            request_id,
                            app_type_str,
                            attempt,
                            used_half_open_permit,
                            success,
                        )
                        .await);
                }
                Err(e) => {
                    // 检测是否需要触发整流器（仅 Claude/ClaudeAuth 供应商）
                    let is_anthropic_provider = self.request_source.anthropic_rectifiers_enabled(
                        ForwarderAnthropicRectifierGateInput { app_type, provider },
                    );
                    let mut signature_rectifier_non_retryable_client_error = false;

                    if let Some(media_retry) =
                        self.request_source
                            .media_retry_plan(ForwarderMediaRetryPlanInput {
                                app: app_type_str,
                                adapter: &adapter,
                                provider,
                                already_retried: media_rectifier_retried,
                                provider_body: &provider_body,
                                error: &e,
                                config: &self.rectifier_config,
                            })
                    {
                        let _ = std::mem::replace(&mut media_rectifier_retried, true);
                        let retry_kind = ForwarderRectifierRetryKind::MediaFallback;

                        match self
                            .forward(
                                app_type,
                                &method,
                                attempt,
                                endpoint,
                                &media_retry.body,
                                &headers,
                                &extensions,
                                &adapter,
                            )
                            .await
                        {
                            Ok(success) => {
                                self.runtime_state_source
                                    .log_rectifier_retry_success(app_type_str, retry_kind);
                                return Ok(self
                                    .complete_successful_attempt(
                                        request_id,
                                        app_type_str,
                                        attempt,
                                        used_half_open_permit,
                                        success,
                                    )
                                    .await);
                            }
                            Err(retry_err) => {
                                self.runtime_state_source.log_rectifier_retry_failure(
                                    app_type_str,
                                    retry_kind,
                                    &retry_err,
                                );
                                if let Some(err) = self
                                    .handle_rectifier_retry_failure(
                                        retry_err,
                                        request_id,
                                        attempt,
                                        app_type_str,
                                        used_half_open_permit,
                                        retry_kind,
                                        &mut last_error,
                                    )
                                    .await
                                {
                                    return Err(err);
                                }
                                continue;
                            }
                        }
                    }

                    if is_anthropic_provider {
                        match self.request_source.thinking_signature_rectifier_plan(
                            ForwarderThinkingSignatureRectifierInput {
                                app: app_type_str,
                                body: &mut provider_body,
                                error: &e,
                                already_retried: rectifier_retried,
                                config: &self.rectifier_config,
                            },
                        ) {
                            ForwarderRequestRectifierPlan::NotTriggered => {}
                            ForwarderRequestRectifierPlan::AlreadyRetried => {
                                self.release_attempt_permit_neutral(
                                    attempt,
                                    app_type_str,
                                    used_half_open_permit,
                                )
                                .await;
                                self.runtime_state_source
                                    .record_forward_error_status(&e)
                                    .await;
                                return Err(ForwardError { error: e });
                            }
                            ForwarderRequestRectifierPlan::TriggeredUnchanged => {
                                signature_rectifier_non_retryable_client_error = true;
                            }
                            ForwarderRequestRectifierPlan::Retry => {
                                let _ = std::mem::replace(&mut rectifier_retried, true);
                                let retry_kind = ForwarderRectifierRetryKind::ThinkingSignature;

                                match self
                                    .forward(
                                        app_type,
                                        &method,
                                        attempt,
                                        endpoint,
                                        &provider_body,
                                        &headers,
                                        &extensions,
                                        &adapter,
                                    )
                                    .await
                                {
                                    Ok(success) => {
                                        self.runtime_state_source
                                            .log_rectifier_retry_success(app_type_str, retry_kind);
                                        return Ok(self
                                            .complete_successful_attempt(
                                                request_id,
                                                app_type_str,
                                                attempt,
                                                used_half_open_permit,
                                                success,
                                            )
                                            .await);
                                    }
                                    Err(retry_err) => {
                                        self.runtime_state_source.log_rectifier_retry_failure(
                                            app_type_str,
                                            retry_kind,
                                            &retry_err,
                                        );
                                        if let Some(err) = self
                                            .handle_rectifier_retry_failure(
                                                retry_err,
                                                request_id,
                                                attempt,
                                                app_type_str,
                                                used_half_open_permit,
                                                retry_kind,
                                                &mut last_error,
                                            )
                                            .await
                                        {
                                            return Err(err);
                                        }
                                        continue;
                                    }
                                }
                            }
                        }
                    }

                    // 检测是否需要触发 budget 整流器（仅 Claude/ClaudeAuth 供应商）
                    if is_anthropic_provider {
                        match self.request_source.thinking_budget_rectifier_plan(
                            ForwarderThinkingBudgetRectifierInput {
                                app: app_type_str,
                                body: &mut provider_body,
                                error: &e,
                                already_retried: budget_rectifier_retried,
                                config: &self.rectifier_config,
                            },
                        ) {
                            ForwarderRequestRectifierPlan::NotTriggered => {}
                            ForwarderRequestRectifierPlan::AlreadyRetried
                            | ForwarderRequestRectifierPlan::TriggeredUnchanged => {
                                self.release_attempt_permit_neutral(
                                    attempt,
                                    app_type_str,
                                    used_half_open_permit,
                                )
                                .await;
                                self.runtime_state_source
                                    .record_forward_error_status(&e)
                                    .await;
                                return Err(ForwardError { error: e });
                            }
                            ForwarderRequestRectifierPlan::Retry => {
                                let _ = std::mem::replace(&mut budget_rectifier_retried, true);
                                let retry_kind = ForwarderRectifierRetryKind::ThinkingBudget;

                                match self
                                    .forward(
                                        app_type,
                                        &method,
                                        attempt,
                                        endpoint,
                                        &provider_body,
                                        &headers,
                                        &extensions,
                                        &adapter,
                                    )
                                    .await
                                {
                                    Ok(success) => {
                                        self.runtime_state_source
                                            .log_rectifier_retry_success(app_type_str, retry_kind);
                                        return Ok(self
                                            .complete_successful_attempt(
                                                request_id,
                                                app_type_str,
                                                attempt,
                                                used_half_open_permit,
                                                success,
                                            )
                                            .await);
                                    }
                                    Err(retry_err) => {
                                        self.runtime_state_source.log_rectifier_retry_failure(
                                            app_type_str,
                                            retry_kind,
                                            &retry_err,
                                        );
                                        if let Some(err) = self
                                            .handle_rectifier_retry_failure(
                                                retry_err,
                                                request_id,
                                                attempt,
                                                app_type_str,
                                                used_half_open_permit,
                                                retry_kind,
                                                &mut last_error,
                                            )
                                            .await
                                        {
                                            return Err(err);
                                        }
                                        continue;
                                    }
                                }
                            }
                        }
                    }

                    if signature_rectifier_non_retryable_client_error {
                        self.release_attempt_permit_neutral(
                            attempt,
                            app_type_str,
                            used_half_open_permit,
                        )
                        .await;
                        self.runtime_state_source
                            .record_forward_error_status(&e)
                            .await;
                        return Err(ForwardError { error: e });
                    }

                    // 先分类错误，决定是否计入 provider 健康度
                    // —— NonRetryable 是客户端层错误，无论换哪家 provider 都会被拒绝，
                    //    不应污染熔断器和数据库健康度（与 release_permit_neutral 同语义）。
                    let failure_decision = self.runtime_state_source.forward_failure_decision(&e);

                    match failure_decision {
                        ForwarderFailureDecision::Retryable => {
                            // 可重试：真正的 provider 故障 → 记录失败并更新熔断器/DB 健康度
                            self.record_failure_result(
                                request_id,
                                attempt,
                                app_type_str,
                                used_half_open_permit,
                                &e,
                            )
                            .await;

                            self.runtime_state_source
                                .record_provider_failure(provider, &e)
                                .await;

                            self.runtime_state_source.log_retryable_forward_failure(
                                app_type_str,
                                &e,
                                provider,
                                attempted_providers,
                                attempts.len(),
                            );

                            last_error = Some(e);
                            // 继续尝试下一个供应商
                            continue;
                        }
                        ForwarderFailureDecision::NonRetryable => {
                            // 不可重试：客户端层错误或客户端断连 → 不污染健康度，仅释放 HalfOpen permit
                            self.release_attempt_permit_neutral(
                                attempt,
                                app_type_str,
                                used_half_open_permit,
                            )
                            .await;
                            self.runtime_state_source
                                .record_forward_error_status(&e)
                                .await;
                            return Err(ForwardError { error: e });
                        }
                    }
                }
            }
        }

        if attempted_providers == 0 {
            // providers 列表非空，但全部被熔断器拒绝（典型：HalfOpen 探测名额被占用）
            self.runtime_state_source
                .record_no_available_provider_status()
                .await;
            return Err(ForwardError {
                error: ProxyError::NoAvailableProvider,
            });
        }

        // 所有供应商都失败了
        self.runtime_state_source
            .record_terminal_failure_status()
            .await;

        self.runtime_state_source.log_terminal_forward_failure(
            app_type_str,
            attempted_providers,
            attempts.len(),
            last_error.as_ref(),
        );

        Err(ForwardError {
            error: last_error.unwrap_or(ProxyError::MaxRetriesExceeded),
        })
    }

    /// 转发单个请求（使用适配器）
    ///
    /// 成功时返回上游响应以及最终发往上游的模型名（所有映射/改写之后）。
    #[allow(clippy::too_many_arguments)]
    async fn forward(
        &self,
        app_type: &AppType,
        method: &http::Method,
        attempt: &ForwardAttempt,
        endpoint: &str,
        body: &Value,
        headers: &axum::http::HeaderMap,
        extensions: &Extensions,
        adapter: &ForwarderAdapterContext,
    ) -> Result<ForwarderUpstreamSuccess, ProxyError> {
        let provider = attempt.provider();
        let ForwarderProviderUrlFacts {
            mut base_url,
            is_full_url,
            is_copilot,
        } = adapter.provider_url_facts(provider)?;

        let mut mapped_body = self.request_source.prepare_provider_request_body(
            ForwarderProviderRequestBodyInput {
                app_type,
                body: body.clone(),
                provider,
                channel: attempt.channel(),
                is_copilot,
            },
        )?;
        self.request_source
            .apply_copilot_live_model_for_adapter(ForwarderCopilotLiveModelInput {
                provider,
                body: &mut mapped_body,
                is_copilot,
            })
            .await;

        // --- Copilot 优化器：分类 + 请求体优化（在格式转换之前执行） ---
        // 执行顺序（与 copilot-api 对齐）由 request source 保持：
        //   1. 先在原始 body 上分类（保留 tool_result 语义，避免误判为 user）
        //   2. 再清洗孤立 tool_result（防止上游 API 报错）
        //   3. 再合并 tool_result + text（减少 premium 计费）
        let optimized = self.request_source.prepare_copilot_request_optimization(
            ForwarderCopilotRequestOptimizationGateInput {
                body: mapped_body,
                headers,
                config: &self.copilot_optimizer_config,
                is_copilot,
            },
        );
        mapped_body = optimized.body;
        let copilot_optimization = self.auth_source.prepare_optional_copilot_auth_optimization(
            ForwarderMaybeCopilotAuthOptimizationInput {
                classification: optimized.classification,
                config: &self.copilot_optimizer_config,
                session_source_body: body,
                request_body: &mapped_body,
                headers,
            },
        );

        self.request_source
            .apply_copilot_dynamic_base_url_for_provider(ForwarderCopilotDynamicBaseUrlInput {
                provider,
                base_url: &mut base_url,
                is_copilot,
                is_full_url,
            })
            .await;
        let resolved_claude_api_format = self
            .request_source
            .resolve_claude_api_format_for_adapter(ForwarderClaudeApiFormatInput {
                adapter,
                provider,
                body: &mapped_body,
                is_copilot,
            })
            .await;
        self.request_source
            .apply_claude_body_policies(ForwarderClaudeBodyPolicyInput {
                adapter,
                body: &mut mapped_body,
                provider,
                api_format: resolved_claude_api_format.as_deref(),
                config: &self.rectifier_config,
            });
        let transform_plan = self
            .request_source
            .transform_plan(ForwarderTransformPlanInput {
                app_type,
                adapter,
                endpoint,
                provider,
                resolved_claude_api_format: resolved_claude_api_format.as_deref(),
            });
        let protocol_preparation =
            self.request_source
                .protocol_preparation(ForwarderProtocolPreparationInput {
                    transform_plan: &transform_plan,
                });
        let url_plan = self
            .request_source
            .plan_upstream_url(ForwarderUpstreamUrlInput {
                adapter,
                base_url: &base_url,
                endpoint,
                is_full_url,
                transform_plan: &transform_plan,
                is_copilot,
                body: &mapped_body,
                channel_param_overrides: attempt.channel().map(|channel| &channel.param_overrides),
            });
        let effective_endpoint = url_plan.effective_endpoint;
        let url = url_plan.url;

        let claude_transformed_body = if protocol_preparation.should_transform_claude_request {
            Some(
                self.protocol_state_source
                    .transform_claude_request(ForwarderClaudeProtocolTransformInput {
                        body: mapped_body.clone(),
                        provider,
                        api_format: protocol_preparation
                            .claude_api_format_for_transform
                            .as_deref(),
                        session_id: &self.session_id,
                        session_client_provided: self.session_client_provided,
                    })
                    .map_err(ProxyError::TransformError)?,
            )
        } else {
            None
        };

        self.protocol_state_source
            .enrich_codex_chat_request(ForwarderCodexChatProtocolEnrichmentInput {
                body: &mut mapped_body,
                enabled: protocol_preparation.codex_chat_enrichment_enabled,
            })
            .await;
        let transformed_request =
            self.request_source
                .transform_request_body(ForwarderRequestBodyTransformInput {
                    adapter,
                    body: mapped_body,
                    provider,
                    transform_plan: &transform_plan,
                    claude_transformed_body,
                })?;
        let mut request_body = transformed_request.body;
        let initial_outbound_model = transformed_request.outbound_model;

        self.request_source
            .apply_app_media_prevention(ForwarderAppMediaPreventionInput {
                app_type,
                body: &mut request_body,
                provider,
                config: &self.rectifier_config,
            });

        let prepared_request =
            self.request_source
                .prepare_upstream_body(ForwarderRequestPreparationInput {
                    app: app_type.as_str(),
                    provider_id: provider.id.as_str(),
                    endpoint: &effective_endpoint,
                    api_format: resolved_claude_api_format.as_deref(),
                    body: request_body,
                    session_client_provided: self.session_client_provided,
                    transform_plan: &transform_plan,
                    initial_outbound_model,
                    headers,
                });

        let auth_headers = self
            .auth_source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter,
                app_type,
                method,
                endpoint,
                request_body: body,
                request_headers: headers,
                attempt,
                session_id: &self.session_id,
                session_client_provided: self.session_client_provided,
                copilot_optimization,
            })
            .await?;
        let codex_oauth_session_headers = auth_headers.codex_oauth_session_headers;
        let auth_headers = auth_headers.auth_headers;

        let request_parts =
            self.request_source
                .build_upstream_request_parts(ForwarderRequestPartsInput {
                    method,
                    url: &url,
                    inbound_headers: headers,
                    provider,
                    prepared_request: &prepared_request,
                    auth_headers: &auth_headers,
                    channel_header_overrides: attempt
                        .channel()
                        .map(|channel| &channel.header_overrides),
                    is_copilot,
                    adapter,
                    resolved_claude_api_format: resolved_claude_api_format.as_deref(),
                    codex_oauth_session_headers: &codex_oauth_session_headers,
                })?;
        self.request_source
            .log_upstream_request(ForwarderUpstreamRequestLogInput {
                adapter,
                url: &url,
                prepared_request: &prepared_request,
            });
        let request_is_streaming = prepared_request.request_is_streaming;
        let outbound_model = prepared_request.outbound_model.clone();

        // 发送请求
        let response = self
            .transport_source
            .send_upstream_request(ForwarderUpstreamTransportRequest {
                method: method.clone(),
                url: url.clone(),
                request_parts,
                extensions: extensions.clone(),
                request_is_streaming,
                non_streaming_timeout: self.non_streaming_timeout,
                streaming_first_byte_timeout: self.streaming_first_byte_timeout,
            })
            .await?;

        let response = self.response_source.apply_channel_response_status_mapping(
            ForwarderChannelResponseStatusInput {
                response,
                channel: attempt.channel(),
            },
        )?;

        let response = self
            .response_source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response,
                request_is_streaming,
                non_streaming_timeout: self.non_streaming_timeout,
                streaming_first_byte_timeout: self.streaming_first_byte_timeout,
            })
            .await?;
        Ok(ForwarderUpstreamSuccess {
            response,
            claude_api_format: resolved_claude_api_format,
            outbound_model,
        })
    }

    /// 故障转移开启时，成功不能只看上游响应头。
    ///
    /// - 非流式：先把完整 body 读到内存，读超时/连接中断会回到 retry loop 尝试下一家。
    /// - 流式：至少等首个 chunk 到达，避免上游返回 200 后一直不吐 SSE 时被误记成功。
    #[cfg(test)]
    async fn prepare_success_response_for_failover(
        &self,
        response: ProxyResponse,
        request_is_streaming: bool,
    ) -> Result<ProxyResponse, ProxyError> {
        self.response_source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response,
                request_is_streaming,
                non_streaming_timeout: self.non_streaming_timeout,
                streaming_first_byte_timeout: self.streaming_first_byte_timeout,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::proxy::codex_chat_history::CodexChatHistoryStore;
    use crate::proxy::events::ProxyEventBus;
    use crate::proxy_core::api::auth::ManagedAccountAuthError;
    use crate::proxy_core::api::transport::build_codex_oauth_session_headers;
    use crate::proxy_core_adapter::ProxyRuntimeStatus;
    use crate::proxy_core_adapter::{canonical_json_string, short_value_hash};
    use crate::proxy_core_adapter::{
        claude_transform_endpoint_rewrite_input_from_body as transform_endpoint_rewrite_input,
        interface_kind_for_forward, request_model_for_forward,
        rewrite_claude_transform_endpoint as rewrite_transform_endpoint, AppKind,
        GeminiShadowStore, ResolvedChannelAttempt,
    };
    use axum::http::header::{HeaderValue, ACCEPT};
    use axum::http::HeaderMap;
    use bytes::Bytes;
    use http::StatusCode;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::RwLock;

    fn test_provider_with_type(provider_type: Option<&str>) -> Provider {
        Provider {
            id: "provider-1".to_string(),
            name: "Provider 1".to_string(),
            settings_config: json!({}),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: provider_type.map(|value| crate::provider::ProviderMeta {
                provider_type: Some(value.to_string()),
                ..Default::default()
            }),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn bedrock_optimizer_env_flag_is_extracted_from_provider_settings() {
        let mut provider = test_provider_with_type(None);
        provider.settings_config = json!({
            "env": {
                "CLAUDE_CODE_USE_BEDROCK": "1"
            }
        });

        assert_eq!(provider_bedrock_env_flag(&provider), Some("1"));
    }

    struct TestForwarder {
        forwarder: RequestForwarder,
        status: Arc<RwLock<ProxyRuntimeStatus>>,
        events: Arc<ProxyEventBus>,
    }

    impl std::ops::Deref for TestForwarder {
        type Target = RequestForwarder;

        fn deref(&self) -> &Self::Target {
            &self.forwarder
        }
    }

    impl std::ops::DerefMut for TestForwarder {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.forwarder
        }
    }

    fn test_forwarder(
        non_streaming_timeout: Duration,
        streaming_first_byte_timeout: Duration,
    ) -> TestForwarder {
        let db = Arc::new(Database::memory().expect("memory db"));
        let status = Arc::new(RwLock::new(ProxyRuntimeStatus::default()));
        let current_providers = Arc::new(RwLock::new(HashMap::new()));
        let events = Arc::new(ProxyEventBus::default());
        let gemini_shadow = Arc::new(GeminiShadowStore::new());
        let codex_chat_history = Arc::new(CodexChatHistoryStore::default());
        let router = Arc::new(provider_router_from_database(db.clone()));

        let forwarder = RequestForwarder {
            attempt_runtime_source:
                crate::proxy::host::cc_switch::forwarder_attempt_runtime_source::forwarder_attempt_runtime_source_from_runtime_sources(router, db.clone()),
            protocol_state_source:
                crate::proxy::host::cc_switch::forwarder_protocol_state_source::forwarder_protocol_state_source_from_runtime_parts(
                    gemini_shadow,
                    codex_chat_history,
                ),
            runtime_state_source:
                crate::proxy::host::cc_switch::forwarder_runtime_state_source::forwarder_runtime_state_source_from_runtime_parts(
                    status.clone(),
                    current_providers,
                    events.clone(),
                ),
            auth_source:
                crate::proxy::host::cc_switch::forwarder_auth_source::default_forwarder_auth_source(
                ),
            request_source:
                crate::proxy::host::cc_switch::forwarder_request_source::default_forwarder_request_source(
                ),
            transport_source:
                crate::proxy::host::cc_switch::forwarder_transport_source::default_forwarder_transport_source(
                ),
            response_source:
                crate::proxy::host::cc_switch::forwarder_response_source::default_forwarder_response_source(
                ),
            failover_switch_scheduler: crate::proxy_core_adapter::noop_failover_switch_scheduler(),
            current_provider_id_at_start: String::new(),
            session_id: String::new(),
            session_client_provided: false,
            rectifier_config: RectifierConfig::default(),
            optimizer_config: OptimizerConfig::default(),
            copilot_optimizer_config: CopilotOptimizerConfig::default(),
            non_streaming_timeout,
            streaming_first_byte_timeout,
            max_attempts: 1,
        };
        TestForwarder {
            forwarder,
            status,
            events,
        }
    }

    #[tokio::test]
    async fn successful_attempt_completion_projects_forward_result_and_route_event() {
        let forwarder = test_forwarder(Duration::from_secs(0), Duration::from_secs(0));
        let mut subscriber = forwarder.events.subscribe();
        let provider = test_provider_with_type(None);
        let attempt = ForwardAttempt::from_provider(provider.clone());
        let result = forwarder
            .complete_successful_attempt(
                "req-success",
                "claude",
                &attempt,
                false,
                ForwarderUpstreamSuccess {
                    response: ProxyResponse::buffered(
                        StatusCode::OK,
                        HeaderMap::new(),
                        Bytes::from_static(b"{\"ok\":true}"),
                    ),
                    claude_api_format: Some("openai_chat".to_string()),
                    outbound_model: Some("upstream-sonnet".to_string()),
                },
            )
            .await;

        assert_eq!(result.provider.id, provider.id);
        assert_eq!(result.claude_api_format.as_deref(), Some("openai_chat"));
        assert_eq!(result.outbound_model.as_deref(), Some("upstream-sonnet"));
        assert!(result.selected_channel.is_none());
        assert_eq!(result.response.status(), StatusCode::OK);
        assert_eq!(
            result.response.bytes().await.expect("response body"),
            Bytes::from_static(b"{\"ok\":true}")
        );

        let success_event = subscriber.recv().await.expect("success event");
        assert_eq!(success_event.event, "provider_succeeded");
        assert_eq!(success_event.payload["requestId"], "req-success");
        assert_eq!(success_event.payload["providerId"], "provider-1");

        let route_event = subscriber.recv().await.expect("route selected event");
        assert_eq!(route_event.event, "route_selected");
        assert_eq!(route_event.payload["requestId"], "req-success");
        assert_eq!(route_event.payload["providerId"], "provider-1");
    }

    #[tokio::test]
    async fn preplanned_forwarding_reuses_request_scope_accounting() {
        let forwarder = test_forwarder(Duration::from_secs(0), Duration::from_secs(0));
        let mut subscriber = forwarder.events.subscribe();

        let result = forwarder
            .forward_with_preplanned_attempts(
                &AppType::Claude,
                http::Method::POST,
                "/v1/messages",
                json!({"model": "claude-sonnet-4"}),
                axum::http::HeaderMap::new(),
                Extensions::new(),
                Vec::new(),
            )
            .await;

        let error = result.err().expect("empty attempts should fail");
        assert!(matches!(error.error, ProxyError::NoAvailableProvider));

        let started_event = subscriber.recv().await.expect("request started");
        assert_eq!(started_event.event, "request_started");
        assert_eq!(started_event.payload["appType"], "claude");

        let status = forwarder.status.read().await;
        assert_eq!(status.total_requests, 1);
    }

    #[test]
    fn canonical_json_sorts_object_keys_for_cache_trace_hashes() {
        let left = json!({
            "tools": [
                {
                    "parameters": {
                        "properties": {
                            "b": {"type": "string"},
                            "a": {"type": "number"}
                        },
                        "type": "object"
                    },
                    "name": "lookup"
                }
            ]
        });
        let right = json!({
            "tools": [
                {
                    "name": "lookup",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "a": {"type": "number"},
                            "b": {"type": "string"}
                        }
                    }
                }
            ]
        });

        assert_eq!(canonical_json_string(&left), canonical_json_string(&right));
        assert_eq!(
            short_value_hash(Some(&left)),
            short_value_hash(Some(&right))
        );
    }

    #[test]
    fn prepare_upstream_request_body_filters_private_fields_and_canonicalizes_order() {
        let body = json!({
            "z": 1,
            "_internal": "drop",
            "tools": [
                {
                    "name": "lookup",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "_id": {
                                "_private_note": "drop",
                                "type": "string"
                            },
                            "b": {"type": "number"},
                            "a": {"type": "string"}
                        }
                    }
                }
            ],
            "a": 2
        });

        let prepared = prepare_upstream_request_body_with_report(body).body;

        assert!(prepared.get("_internal").is_none());
        assert!(prepared["tools"][0]["parameters"]["properties"]
            .get("_id")
            .is_some());
        assert!(prepared["tools"][0]["parameters"]["properties"]["_id"]
            .get("_private_note")
            .is_none());
        assert_eq!(
            serde_json::to_string(&prepared).unwrap(),
            r#"{"a":2,"tools":[{"name":"lookup","parameters":{"properties":{"_id":{"type":"string"},"a":{"type":"string"},"b":{"type":"number"}},"type":"object"}}],"z":1}"#
        );
    }

    #[tokio::test]
    async fn non_streaming_success_is_buffered_before_marking_provider_successful() {
        let forwarder = test_forwarder(Duration::from_secs(1), Duration::from_secs(1));
        let response = ProxyResponse::streamed(
            StatusCode::OK,
            HeaderMap::new(),
            futures::stream::once(async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok::<Bytes, std::io::Error>(Bytes::from_static(b"{\"ok\":true}"))
            }),
        );

        let prepared = forwarder
            .prepare_success_response_for_failover(response, false)
            .await
            .expect("response should be buffered");

        assert_eq!(
            prepared.bytes().await.unwrap(),
            Bytes::from_static(b"{\"ok\":true}")
        );
    }

    #[test]
    fn channel_status_code_mapping_rewrites_response_status() {
        let forwarder = test_forwarder(Duration::from_secs(0), Duration::from_secs(0));
        let attempt = ForwardAttempt::from_resolved_channel_for_test(
            test_provider_with_type(None),
            ResolvedChannelAttempt {
                channel_id: "channel-a".to_string(),
                channel_name: "Relay A".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "openai_responses".to_string(),
                auth_profile_ref: None,
                public_model: None,
                upstream_model: None,
                header_overrides: json!({}),
                param_overrides: json!({}),
                status_code_mapping: json!([{"from": 429, "to": 200}]),
                retry_policy: json!({}),
            },
        );
        let response = ProxyResponse::buffered(
            StatusCode::TOO_MANY_REQUESTS,
            HeaderMap::new(),
            Bytes::from_static(b"ok"),
        );

        let mapped = forwarder
            .response_source
            .apply_channel_response_status_mapping(ForwarderChannelResponseStatusInput {
                response,
                channel: attempt.channel(),
            })
            .expect("status mapping");

        assert_eq!(mapped.status(), StatusCode::OK);
    }

    #[test]
    fn channel_status_code_mapping_ignores_non_numeric_targets() {
        let forwarder = test_forwarder(Duration::from_secs(0), Duration::from_secs(0));
        let attempt = ForwardAttempt::from_resolved_channel_for_test(
            test_provider_with_type(None),
            ResolvedChannelAttempt {
                channel_id: "channel-a".to_string(),
                channel_name: "Relay A".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "openai_responses".to_string(),
                auth_profile_ref: None,
                public_model: None,
                upstream_model: None,
                header_overrides: json!({}),
                param_overrides: json!({}),
                status_code_mapping: json!([{"from": 429, "to": "rate_limited"}]),
                retry_policy: json!({}),
            },
        );
        let response = ProxyResponse::buffered(
            StatusCode::TOO_MANY_REQUESTS,
            HeaderMap::new(),
            Bytes::from_static(b"rate limited"),
        );

        let mapped = forwarder
            .response_source
            .apply_channel_response_status_mapping(ForwarderChannelResponseStatusInput {
                response,
                channel: attempt.channel(),
            })
            .expect("status mapping");

        assert_eq!(mapped.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn non_streaming_body_read_error_is_retryable_before_success_record() {
        let forwarder = test_forwarder(Duration::from_secs(1), Duration::from_secs(1));
        let response = ProxyResponse::streamed(
            StatusCode::OK,
            HeaderMap::new(),
            futures::stream::once(async {
                Err::<Bytes, std::io::Error>(std::io::Error::other("body boom"))
            }),
        );

        let err = match forwarder
            .prepare_success_response_for_failover(response, false)
            .await
        {
            Ok(_) => panic!("body read errors should fail the attempt"),
            Err(err) => err,
        };

        assert!(matches!(err, ProxyError::ForwardFailed(_)));
    }

    #[tokio::test]
    async fn streaming_success_primes_first_chunk_and_replays_it() {
        let forwarder = test_forwarder(Duration::from_secs(1), Duration::from_secs(1));
        let response = ProxyResponse::streamed(
            StatusCode::OK,
            HeaderMap::new(),
            futures::stream::iter(vec![
                Ok::<Bytes, std::io::Error>(Bytes::from_static(b"first")),
                Ok::<Bytes, std::io::Error>(Bytes::from_static(b"second")),
            ]),
        );

        let prepared = forwarder
            .prepare_success_response_for_failover(response, true)
            .await
            .expect("stream should be primed");

        assert_eq!(
            prepared.bytes().await.unwrap(),
            Bytes::from_static(b"firstsecond")
        );
    }

    #[tokio::test]
    async fn streaming_first_chunk_error_is_retryable_before_success_record() {
        let forwarder = test_forwarder(Duration::from_secs(1), Duration::from_secs(1));
        let response = ProxyResponse::streamed(
            StatusCode::OK,
            HeaderMap::new(),
            futures::stream::once(async {
                Err::<Bytes, std::io::Error>(std::io::Error::other("first chunk boom"))
            }),
        );

        let err = match forwarder
            .prepare_success_response_for_failover(response, true)
            .await
        {
            Ok(_) => panic!("first chunk errors should fail the attempt"),
            Err(err) => err,
        };

        assert!(matches!(err, ProxyError::ForwardFailed(_)));
    }

    #[test]
    fn codex_oauth_session_headers_match_codex_cache_identity() {
        let headers = build_codex_oauth_session_headers("session-123");
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.insert(name, value);
        }

        assert_eq!(
            map.get("session_id"),
            Some(&HeaderValue::from_static("session-123"))
        );
        assert_eq!(
            map.get("x-client-request-id"),
            Some(&HeaderValue::from_static("session-123"))
        );
        assert_eq!(
            map.get("x-codex-window-id"),
            Some(&HeaderValue::from_static("session-123:0"))
        );
    }

    #[test]
    fn managed_account_upstream_rejects_proxy_managed_placeholder_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );

        let err = validate_managed_account_upstream_auth(
            "https://api.githubcopilot.com/chat/completions",
            &headers,
        )
        .expect_err("placeholder should be rejected before upstream");

        assert_eq!(err, ManagedAccountAuthError::PlaceholderForwarded);
    }

    #[test]
    fn codex_oauth_upstream_rejects_proxy_managed_placeholder_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );

        let err = validate_managed_account_upstream_auth(
            "https://chatgpt.com/backend-api/codex/responses",
            &headers,
        )
        .expect_err("placeholder should be rejected before upstream");

        assert_eq!(err, ManagedAccountAuthError::PlaceholderForwarded);
    }

    #[test]
    fn non_managed_upstream_allows_proxy_managed_placeholder_guard() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );

        validate_managed_account_upstream_auth("https://api.example.com/v1/messages", &headers)
            .expect("guard is scoped to managed-account upstreams");
    }

    #[test]
    fn exact_header_case_preserved_for_native_claude_only() {
        let provider = test_provider_with_type(None);

        assert!(should_preserve_exact_request_header_case(
            "Claude",
            provider_is_codex_oauth(&provider),
            false,
            Some("anthropic"),
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Claude",
            provider_is_codex_oauth(&provider),
            false,
            Some("openai_responses"),
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Codex",
            provider_is_codex_oauth(&provider),
            false,
            None
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Gemini",
            provider_is_codex_oauth(&provider),
            false,
            None
        ));
    }

    #[test]
    fn exact_header_case_skipped_for_codex_oauth_and_copilot() {
        let codex_oauth = test_provider_with_type(Some("codex_oauth"));
        let copilot = test_provider_with_type(Some("github_copilot"));

        assert!(!should_preserve_exact_request_header_case(
            "Claude",
            provider_is_codex_oauth(&codex_oauth),
            false,
            Some("openai_responses"),
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Claude",
            provider_is_codex_oauth(&copilot),
            true,
            Some("openai_chat"),
        ));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_strips_beta_for_chat_completions() {
        let (endpoint, passthrough_query) =
            rewrite_transform_endpoint(transform_endpoint_rewrite_input(
                "/v1/messages?beta=true&foo=bar",
                "openai_chat",
                false,
                &json!({ "model": "gpt-5.4" }),
            ))
            .into_parts();

        assert_eq!(endpoint, "/v1/chat/completions?foo=bar");
        assert_eq!(passthrough_query.as_deref(), Some("foo=bar"));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_strips_beta_for_responses() {
        let (endpoint, passthrough_query) =
            rewrite_transform_endpoint(transform_endpoint_rewrite_input(
                "/claude/v1/messages?beta=true&x-id=1",
                "openai_responses",
                false,
                &json!({ "model": "gpt-5.4" }),
            ))
            .into_parts();

        assert_eq!(endpoint, "/v1/responses?x-id=1");
        assert_eq!(passthrough_query.as_deref(), Some("x-id=1"));
    }

    #[test]
    fn rewrite_codex_responses_endpoint_to_chat_preserves_query() {
        let (endpoint, passthrough_query) =
            crate::proxy_core_adapter::rewrite_codex_responses_endpoint_to_chat(
                "/v1/responses?foo=bar",
            );

        assert_eq!(endpoint, "/chat/completions?foo=bar");
        assert_eq!(passthrough_query.as_deref(), Some("foo=bar"));
    }

    #[test]
    fn rewrite_codex_responses_compact_endpoint_to_chat_preserves_query() {
        let (endpoint, passthrough_query) =
            crate::proxy_core_adapter::rewrite_codex_responses_endpoint_to_chat(
                "/v1/responses/compact?foo=bar",
            );

        assert_eq!(endpoint, "/chat/completions?foo=bar");
        assert_eq!(passthrough_query.as_deref(), Some("foo=bar"));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_uses_copilot_path() {
        let (endpoint, passthrough_query) =
            rewrite_transform_endpoint(transform_endpoint_rewrite_input(
                "/v1/messages?beta=true&x-id=1",
                "anthropic",
                true,
                &json!({ "model": "claude-sonnet-4-6" }),
            ))
            .into_parts();

        assert_eq!(endpoint, "/chat/completions?x-id=1");
        assert_eq!(passthrough_query.as_deref(), Some("x-id=1"));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_uses_copilot_responses_path() {
        let (endpoint, passthrough_query) =
            rewrite_transform_endpoint(transform_endpoint_rewrite_input(
                "/v1/messages?beta=true&x-id=1",
                "openai_responses",
                true,
                &json!({ "model": "gpt-5.4" }),
            ))
            .into_parts();

        assert_eq!(endpoint, "/v1/responses?x-id=1");
        assert_eq!(passthrough_query.as_deref(), Some("x-id=1"));
    }

    #[test]
    fn rewrite_claude_transform_endpoint_maps_gemini_generate_content() {
        let (endpoint, passthrough_query) =
            rewrite_transform_endpoint(transform_endpoint_rewrite_input(
                "/v1/messages?beta=true&x-id=1",
                "gemini_native",
                false,
                &json!({ "model": "gemini-2.5-pro" }),
            ))
            .into_parts();

        assert_eq!(
            endpoint,
            "/v1beta/models/gemini-2.5-pro:generateContent?x-id=1"
        );
        assert_eq!(passthrough_query.as_deref(), Some("x-id=1"));
    }

    /// Regression: body.model arriving as the resource-name form
    /// `models/gemini-2.5-pro` must not produce a doubled
    /// `/v1beta/models/models/...` path.
    #[test]
    fn rewrite_claude_transform_endpoint_strips_gemini_model_resource_prefix() {
        let (endpoint, _) = rewrite_transform_endpoint(transform_endpoint_rewrite_input(
            "/v1/messages",
            "gemini_native",
            false,
            &json!({ "model": "models/gemini-2.5-pro" }),
        ))
        .into_parts();

        assert_eq!(endpoint, "/v1beta/models/gemini-2.5-pro:generateContent");
    }

    #[test]
    fn rewrite_claude_transform_endpoint_maps_gemini_streaming() {
        let (endpoint, passthrough_query) =
            rewrite_transform_endpoint(transform_endpoint_rewrite_input(
                "/v1/messages?beta=true",
                "gemini_native",
                false,
                &json!({ "model": "gemini-2.5-flash", "stream": true }),
            ))
            .into_parts();

        assert_eq!(
            endpoint,
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(passthrough_query.as_deref(), Some("alt=sse"));
    }

    #[test]
    fn append_query_to_full_url_preserves_existing_query_string() {
        let url = crate::proxy_core_adapter::append_query_to_full_url(
            "https://relay.example/api?foo=bar",
            Some("x-id=1"),
        );

        assert_eq!(url, "https://relay.example/api?foo=bar&x-id=1");
    }

    #[test]
    fn route_request_model_and_interface_follow_inbound_shape() {
        let body = json!({ "model": "gpt-5.4" });
        let codex_app = AppKind::from(&AppType::Codex);
        let gemini_app = AppKind::from(&AppType::Gemini);
        let claude_app = AppKind::from(&AppType::Claude);

        assert_eq!(
            request_model_for_forward(&codex_app, "/v1/responses", &body).as_deref(),
            Some("gpt-5.4")
        );
        assert_eq!(
            interface_kind_for_forward(&codex_app, "/v1/responses?stream=1"),
            Some("openai_responses")
        );
        assert_eq!(
            interface_kind_for_forward(&codex_app, "/v1/chat/completions"),
            Some("openai_chat_completions")
        );
        assert_eq!(
            request_model_for_forward(
                &gemini_app,
                "/v1beta/models/gemini-2.0-flash:generateContent",
                &json!({})
            )
            .as_deref(),
            Some("gemini-2.0-flash")
        );
        assert_eq!(
            interface_kind_for_forward(&claude_app, "/v1/messages"),
            Some("anthropic_messages")
        );
    }

    #[test]
    fn build_gemini_native_url_uses_origin_when_base_ends_with_v1beta() {
        let url = crate::proxy_core_adapter::build_gemini_native_url(
            "https://generativelanguage.googleapis.com/v1beta",
            "/v1beta/models/gemini-2.5-pro:generateContent",
        );

        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
        );
    }

    #[test]
    fn build_gemini_native_url_uses_origin_when_base_already_contains_models_prefix() {
        let url = crate::proxy_core_adapter::build_gemini_native_url(
            "https://generativelanguage.googleapis.com/v1beta/models",
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
        );

        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn resolve_gemini_native_url_keeps_opaque_full_url_as_is() {
        let url = crate::proxy_core_adapter::resolve_gemini_native_url(
            "https://relay.example/custom/generate-content",
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
            true,
        );

        assert_eq!(url, "https://relay.example/custom/generate-content?alt=sse");
    }

    #[test]
    fn force_identity_for_stream_flag_requests() {
        let headers = HeaderMap::new();

        let policy = crate::proxy_core_adapter::resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({ "stream": true }),
            &headers,
        );

        assert!(policy.force_identity_encoding);
    }

    #[test]
    fn force_identity_for_gemini_stream_endpoints() {
        let headers = HeaderMap::new();

        let policy = crate::proxy_core_adapter::resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
            &json!({ "model": "gemini-2.5-pro" }),
            &headers,
        );

        assert!(policy.force_identity_encoding);
    }

    #[test]
    fn streaming_request_detects_gemini_sse_without_body_stream_flag() {
        let headers = HeaderMap::new();

        assert!(crate::proxy_core_adapter::is_streaming_upstream_request(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
            &json!({ "model": "gemini-2.5-pro" }),
            &headers
        ));
    }

    #[test]
    fn force_identity_for_sse_accept_header() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));

        let policy = crate::proxy_core_adapter::resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({ "model": "gpt-5" }),
            &headers,
        );

        assert!(policy.force_identity_encoding);
    }

    #[test]
    fn non_streaming_requests_allow_automatic_compression() {
        let headers = HeaderMap::new();

        let policy = crate::proxy_core_adapter::resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({ "model": "gpt-5" }),
            &headers,
        );

        assert!(!policy.force_identity_encoding);
    }

    // ===== P3: forwarder 层 media 开关回归测试 =====
    // 验证 gate 在 forwarder 这一层的"接线"，而非 request_media 纯函数本身。

    fn forwarder_with_rectifier(config: RectifierConfig) -> TestForwarder {
        let mut fwd = test_forwarder(Duration::from_secs(1), Duration::from_secs(1));
        fwd.rectifier_config = config;
        fwd
    }

    fn provider_with_settings(settings_config: Value) -> Provider {
        let mut p = test_provider_with_type(Some("anthropic"));
        p.settings_config = settings_config;
        p
    }

    fn body_with_image(model: &str) -> Value {
        json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        })
    }

    fn body_with_codex_input_image(model: &str) -> Value {
        json!({
            "model": model,
            "input": [{
                "role": "user",
                "content": [
                    { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                ]
            }]
        })
    }

    fn image_unsupported_error() -> ProxyError {
        ProxyError::UpstreamError {
            status: 400,
            body: Some(
                r#"{"error":{"message":"This model does not support image input"}}"#.to_string(),
            ),
        }
    }

    fn apply_media_prevention_for_test(
        fwd: &RequestForwarder,
        body: &mut Value,
        provider: &Provider,
    ) -> usize {
        fwd.request_source
            .apply_app_media_prevention(ForwarderAppMediaPreventionInput {
                app_type: &AppType::Codex,
                body,
                provider,
                config: &fwd.rectifier_config,
            })
    }

    fn media_retry_should_trigger_for_test(
        fwd: &RequestForwarder,
        app_type: &AppType,
        already_retried: bool,
        provider_body: &Value,
        error: &ProxyError,
    ) -> bool {
        let provider = provider_with_settings(json!({}));
        let adapter = fwd.request_source.adapter_context_for_app(app_type);
        fwd.request_source
            .media_retry_plan(ForwarderMediaRetryPlanInput {
                app: app_type.as_str(),
                adapter: &adapter,
                provider: &provider,
                already_retried,
                provider_body,
                error,
                config: &fwd.rectifier_config,
            })
            .is_some()
    }

    #[test]
    fn prevention_replaces_when_all_switches_on_and_model_in_heuristic_list() {
        let fwd = forwarder_with_rectifier(RectifierConfig::default());
        let provider = provider_with_settings(json!({}));
        let mut body = body_with_image("deepseek-v4-pro");

        let replaced = apply_media_prevention_for_test(&fwd, &mut body, &provider);

        assert_eq!(replaced, 1, "默认全开 + 名单内模型应预替换");
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn prevention_skipped_when_media_fallback_off() {
        // 关闭 request_media_fallback：即使名单命中也不预替换。
        let fwd = forwarder_with_rectifier(RectifierConfig {
            request_media_fallback: false,
            ..RectifierConfig::default()
        });
        let provider = provider_with_settings(json!({}));
        let mut body = body_with_image("deepseek-v4-pro");

        let replaced = apply_media_prevention_for_test(&fwd, &mut body, &provider);

        assert_eq!(replaced, 0);
        assert_eq!(body["messages"][0]["content"][0]["type"], "image");
    }

    #[test]
    fn prevention_skipped_when_master_switch_off() {
        let fwd = forwarder_with_rectifier(RectifierConfig {
            enabled: false,
            ..RectifierConfig::default()
        });
        let provider = provider_with_settings(json!({}));
        let mut body = body_with_image("deepseek-v4-pro");

        assert_eq!(
            apply_media_prevention_for_test(&fwd, &mut body, &provider),
            0
        );
        assert_eq!(body["messages"][0]["content"][0]["type"], "image");
    }

    #[test]
    fn prevention_heuristic_off_skips_list_but_keeps_explicit_text_only() {
        // 关闭 request_media_heuristic：名单预测失效，但显式声明 text-only 仍预替换。
        let fwd = forwarder_with_rectifier(RectifierConfig {
            request_media_heuristic: false,
            ..RectifierConfig::default()
        });

        // (a) 名单内模型、无显式声明 → 不再预替换
        let bare_provider = provider_with_settings(json!({}));
        let mut list_body = body_with_image("deepseek-v4-pro");
        assert_eq!(
            apply_media_prevention_for_test(&fwd, &mut list_body, &bare_provider),
            0,
            "heuristic 关闭后名单模型不应被预替换"
        );
        assert_eq!(list_body["messages"][0]["content"][0]["type"], "image");

        // (b) 显式声明 text-only → 仍预替换（声明驱动，不受 heuristic 开关影响）
        let declared_provider = provider_with_settings(json!({
            "models": [ { "id": "some-text-model", "input": ["text"] } ]
        }));
        let mut declared_body = body_with_image("some-text-model");
        assert_eq!(
            apply_media_prevention_for_test(&fwd, &mut declared_body, &declared_provider),
            1,
            "显式 text-only 即使关闭 heuristic 也应预替换"
        );
        assert_eq!(declared_body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn reactive_triggers_when_all_switches_on() {
        let fwd = forwarder_with_rectifier(RectifierConfig::default());
        let body = body_with_image("any-model");
        assert!(media_retry_should_trigger_for_test(
            &fwd,
            &AppType::Claude,
            false,
            &body,
            &image_unsupported_error()
        ));
    }

    #[test]
    fn reactive_triggers_for_codex_image_url_deserialize_errors() {
        let fwd = forwarder_with_rectifier(RectifierConfig::default());
        let body = body_with_codex_input_image("deepseek-v4-flash");
        let error = ProxyError::UpstreamError {
            status: 400,
            body: Some(
                r#"{"error":{"message":"Failed to deserialize the JSON body into the target type: messages[11]: unknown variant image_url, expected text"}}"#
                    .to_string(),
            ),
        };

        assert!(media_retry_should_trigger_for_test(
            &fwd,
            &AppType::Codex,
            false,
            &body,
            &error
        ));
    }

    #[test]
    fn reactive_skipped_when_media_fallback_off() {
        // 关闭 request_media_fallback：上游报图片错误也不触发兜底重试。
        let fwd = forwarder_with_rectifier(RectifierConfig {
            request_media_fallback: false,
            ..RectifierConfig::default()
        });
        let body = body_with_image("any-model");
        assert!(!media_retry_should_trigger_for_test(
            &fwd,
            &AppType::Claude,
            false,
            &body,
            &image_unsupported_error()
        ));
    }

    #[test]
    fn reactive_skipped_when_master_switch_off() {
        let fwd = forwarder_with_rectifier(RectifierConfig {
            enabled: false,
            ..RectifierConfig::default()
        });
        let body = body_with_image("any-model");
        assert!(!media_retry_should_trigger_for_test(
            &fwd,
            &AppType::Claude,
            false,
            &body,
            &image_unsupported_error()
        ));
    }

    #[test]
    fn reactive_unaffected_by_heuristic_switch() {
        // 关闭 request_media_heuristic 不影响反应式兜底——它是上游实测错误后的恢复，不是预测。
        let fwd = forwarder_with_rectifier(RectifierConfig {
            request_media_heuristic: false,
            ..RectifierConfig::default()
        });
        let body = body_with_image("any-model");
        assert!(media_retry_should_trigger_for_test(
            &fwd,
            &AppType::Claude,
            false,
            &body,
            &image_unsupported_error()
        ));
    }
}
