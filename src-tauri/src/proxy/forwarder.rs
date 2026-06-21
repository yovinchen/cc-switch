//! 请求转发器
//!
//! 负责将请求转发到上游Provider，支持故障转移

use super::hyper_client::ProxyResponse;
use super::{
    error::ProxyError,
    error_mapper::{forward_failure_kind_from_proxy_error, reqwest_send_error_to_proxy_error},
    events::ProxyEventBus,
    failover_switch::FailoverSwitchManager,
    provider_router::ProviderRouter,
    providers::{
        codex_chat_history::CodexChatHistoryStore, get_adapter,
        provider_kind_from_app_type_and_config, ProviderAdapter,
    },
    route_attempt::{apply_channel_model_override, ForwardAttempt},
};
use crate::proxy::managed_account_auth::{
    fetch_copilot_live_models, resolve_copilot_api_endpoint, resolve_copilot_model_vendor,
    resolve_managed_account_auth,
};
use crate::proxy_core_adapter::{
    apply_bedrock_pre_send_optimizers, apply_copilot_model_normalization,
    apply_copilot_warmup_model_override, attempt_event_name,
    bedrock_env_flag_from_provider_settings, build_attempt_event_payload,
    build_codex_oauth_session_headers, build_request_started_event_payload,
    build_retryable_forward_failure_log, build_terminal_forward_failure_log,
    build_upstream_auth_headers, cache_injection_log_message, categorize_forward_failure,
    classify_copilot_request, contains_image_blocks, forward_upstream_url_plan,
    is_github_copilot_upstream, is_openai_o_series, is_unsupported_image_error,
    mapped_channel_response_status, merge_copilot_tool_results, normalize_thinking_type,
    prepare_upstream_request_body_with_report, prompt_cache_trace_log_message,
    rectify_anthropic_request, rectify_thinking_budget, replace_image_blocks_with_marker,
    record_forward_success_status, replace_images_for_text_only_model,
    request_body_filter_log_message, resolve_claude_forward_api_format,
    resolve_copilot_deterministic_interaction_id, resolve_copilot_model_against_ids,
    resolve_copilot_optimizer_session_id, resolve_copilot_request_id_with_fallback,
    resolve_media_prevention_policy, resolved_copilot_dynamic_base_url,
    responses_to_chat_completions_with_options, sanitize_copilot_orphan_tool_results,
    should_apply_bedrock_pre_send_optimizer,
    should_check_media_retry, should_failover_after_rectifier_retry_failure,
    should_preserve_exact_request_header_case, should_rectify_thinking_budget,
    should_rectify_thinking_signature, should_resolve_copilot_dynamic_endpoint,
    should_send_anthropic_request_headers, should_trigger_media_retry,
    strip_copilot_thinking_blocks, strip_one_m_suffix_for_upstream,
    strip_one_m_suffix_for_upstream_from_body, supports_reasoning_effort,
    thinking_optimization_log_message, validate_managed_account_upstream_auth,
    AttemptEventChannel, AttemptEventPayloadInput, AttemptEventPhase,
    CopilotAuthHeaderOverrides, CopilotOptimizerConfig, CurrentRouteTarget, ForwardFailureCategory,
    ForwardUpstreamUrlPlanInput, GeminiShadowStore, MediaRetryInput, OptimizerConfig,
    PromptCacheTraceLogInput,
    ProviderKind, ProxyRuntimeStatus, RectifierConfig, ResolvedChannelAttempt,
    UpstreamAuthHeadersInput, UpstreamRequestHeadersInput, UpstreamSendPolicyInput,
    UpstreamTransportKind, UNSUPPORTED_IMAGE_MARKER,
};
use crate::{app_config::AppType, provider::Provider};
use futures::StreamExt;
use http::Extensions;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

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

pub struct ForwardError {
    pub error: ProxyError,
    #[allow(dead_code)]
    pub provider: Option<Provider>,
}

/// 活跃连接 RAII guard
///
/// 构造时把 `ProxyRuntimeStatus.active_connections` +1；Drop 时在 tokio runtime 上调度
/// 一个异步任务执行 -1，从而支持把 guard move 进流式 body future（stream 自然结束
/// 时 guard 与 future 一起 drop）。
///
/// 设计动机：之前在请求 wrapper 出口处同步 -1，但流式响应的 body 实际
/// 在 `create_logged_passthrough_stream` 内还会继续 yield 字节流，导致 UI 的
/// `active_connections` 计数过早归零。RAII guard 让"减量"由 Rust 类型系统驱动，
/// 不需要每条出口路径都手动调用。
pub(crate) struct ActiveConnectionGuard {
    status: Arc<RwLock<ProxyRuntimeStatus>>,
}

impl ActiveConnectionGuard {
    pub(crate) async fn acquire(status: Arc<RwLock<ProxyRuntimeStatus>>) -> Self {
        {
            let mut s = status.write().await;
            s.active_connections = s.active_connections.saturating_add(1);
        }
        Self { status }
    }
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        // Drop 不能 await：把减量操作调度到 tokio runtime
        let status = self.status.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let mut s = status.write().await;
                s.active_connections = s.active_connections.saturating_sub(1);
            });
        }
        // 没有 runtime 时静默丢失计数（仅 UI 展示用，可接受最终一致性）
    }
}

pub struct RequestForwarder {
    /// 共享的 ProviderRouter（持有熔断器状态）
    router: Arc<ProviderRouter>,
    status: Arc<RwLock<ProxyRuntimeStatus>>,
    current_providers: Arc<RwLock<std::collections::HashMap<String, CurrentRouteTarget>>>,
    events: Arc<ProxyEventBus>,
    gemini_shadow: Arc<GeminiShadowStore>,
    codex_chat_history: Arc<CodexChatHistoryStore>,
    /// 故障转移切换管理器
    failover_manager: Arc<FailoverSwitchManager>,
    /// AppHandle，用于发射事件和更新托盘
    app_handle: Option<tauri::AppHandle>,
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
    /// 预防式 media 降级：发送前对 text-only 模型把图片块替换为标记。
    ///
    /// 受 `enabled && request_media_fallback` 管辖；其中"启发式模型名单预测"
    /// 再受 `request_media_heuristic` 单独管辖（显式声明 text-only 始终生效）。
    /// 返回被替换的图片块数量（0 = 未触发或开关关闭）。
    fn apply_media_prevention(&self, body: &mut Value, provider: &Provider) -> usize {
        let policy = resolve_media_prevention_policy(
            self.rectifier_config.enabled,
            self.rectifier_config.request_media_fallback,
            self.rectifier_config.request_media_heuristic,
        );
        if !policy.should_attempt {
            return 0;
        }
        let replaced_images = replace_images_for_text_only_model(
            body,
            &provider.settings_config,
            policy.allow_heuristic,
        );
        if replaced_images > 0 {
            let model = body.get("model").and_then(Value::as_str).unwrap_or("");
            log::info!(
                "[Media] Replaced {replaced_images} image block(s) with {} for text-only provider={}, model={}",
                UNSUPPORTED_IMAGE_MARKER,
                provider.id,
                model
            );
        }
        replaced_images
    }

    /// 反应式 media 重试判定：上游因图片输入报错后，是否应替换图片块并对同一供应商重试一次。
    ///
    /// 受 `enabled && request_media_fallback` 管辖；不涉及 `request_media_heuristic`——
    /// 这里是上游"实测"错误后的纯恢复，不是预测，故启发式开关与它无关。
    fn media_retry_should_trigger(
        &self,
        adapter_name: &str,
        already_retried: bool,
        provider_body: &Value,
        error: &ProxyError,
    ) -> bool {
        if !should_check_media_retry(
            adapter_name,
            self.rectifier_config.enabled,
            self.rectifier_config.request_media_fallback,
            already_retried,
        ) {
            return false;
        }

        should_trigger_media_retry(MediaRetryInput {
            adapter_name,
            rectifier_enabled: self.rectifier_config.enabled,
            request_media_fallback: self.rectifier_config.request_media_fallback,
            already_retried,
            body_has_images: contains_image_blocks(provider_body),
            unsupported_image_error: match error {
                ProxyError::UpstreamError { status, body } => {
                    is_unsupported_image_error(*status, body.as_deref())
                }
                _ => false,
            },
        })
    }

    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_preplanned(
        router: Arc<ProviderRouter>,
        non_streaming_timeout: u64,
        status: Arc<RwLock<ProxyRuntimeStatus>>,
        current_providers: Arc<RwLock<std::collections::HashMap<String, CurrentRouteTarget>>>,
        events: Arc<ProxyEventBus>,
        gemini_shadow: Arc<GeminiShadowStore>,
        codex_chat_history: Arc<CodexChatHistoryStore>,
        failover_manager: Arc<FailoverSwitchManager>,
        app_handle: Option<tauri::AppHandle>,
        current_provider_id_at_start: String,
        session_id: String,
        session_client_provided: bool,
        streaming_first_byte_timeout: u64,
        _streaming_idle_timeout: u64,
        rectifier_config: RectifierConfig,
        optimizer_config: OptimizerConfig,
        copilot_optimizer_config: CopilotOptimizerConfig,
        max_retries: u32,
    ) -> Self {
        // max_retries 是「失败后重试次数」语义，attempt 上限 = retries + 1。
        // saturating_add 防止 u32::MAX + 1 溢出。
        let max_attempts = (max_retries as usize).saturating_add(1);
        Self {
            router,
            status,
            current_providers,
            events,
            gemini_shadow,
            codex_chat_history,
            failover_manager,
            app_handle,
            current_provider_id_at_start,
            session_id,
            session_client_provided,
            rectifier_config,
            optimizer_config,
            copilot_optimizer_config,
            non_streaming_timeout: std::time::Duration::from_secs(non_streaming_timeout),
            streaming_first_byte_timeout: std::time::Duration::from_secs(
                streaming_first_byte_timeout,
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
        if let Some(channel) = attempt.channel() {
            if used_half_open_permit {
                if let Err(e) = self
                    .router
                    .record_channel_result(&channel.channel_id, app_type, true, true, None, None)
                    .await
                {
                    log::warn!(
                        "[{app_type}] 记录 Channel 成功结果失败: channel_id={}, error={e}",
                        channel.channel_id
                    );
                }
                self.emit_attempt_succeeded(request_id, app_type, attempt);
                return;
            }

            let router = self.router.clone();
            let channel_id = channel.channel_id.clone();
            let app_type_owned = app_type.to_string();
            tokio::spawn(async move {
                if let Err(e) = router
                    .record_channel_result(&channel_id, &app_type_owned, false, true, None, None)
                    .await
                {
                    log::warn!(
                        "[{app_type_owned}] 异步记录 Channel 成功结果失败: channel_id={channel_id}, error={e}"
                    );
                }
            });
            self.emit_attempt_succeeded(request_id, app_type, attempt);
            return;
        }

        let provider_id = &attempt.provider().id;
        if used_half_open_permit {
            if let Err(e) = self
                .router
                .record_result(provider_id, app_type, true, true, None)
                .await
            {
                log::warn!(
                    "[{app_type}] 记录 Provider 成功结果失败: provider_id={provider_id}, error={e}"
                );
            }
            self.emit_attempt_succeeded(request_id, app_type, attempt);
            return;
        }

        let router = self.router.clone();
        let provider_id = provider_id.clone();
        let app_type_owned = app_type.to_string();
        tokio::spawn(async move {
            if let Err(e) = router
                .record_result(&provider_id, &app_type_owned, false, true, None)
                .await
            {
                log::warn!(
                    "[{app_type_owned}] 异步记录 Provider 成功结果失败: provider_id={provider_id}, error={e}"
                );
            }
        });
        self.emit_attempt_succeeded(request_id, app_type, attempt);
    }

    async fn record_active_target(
        &self,
        request_id: &str,
        app_type: &str,
        attempt: &ForwardAttempt,
    ) {
        let provider = attempt.provider();
        let channel = attempt.channel();
        let target = CurrentRouteTarget {
            app_type: app_type.to_string(),
            provider_id: provider.id.clone(),
            provider_name: provider.name.clone(),
            channel_id: channel.map(|channel| channel.channel_id.clone()),
            channel_name: channel.map(|channel| channel.channel_name.clone()),
            interface_kind: channel.map(|channel| channel.interface_kind.clone()),
            public_model: channel.and_then(|channel| channel.public_model.clone()),
            upstream_model: channel.and_then(|channel| channel.upstream_model.clone()),
        };

        let mut current_providers = self.current_providers.write().await;
        current_providers.insert(app_type.to_string(), target);
        self.events.emit(
            "route_selected",
            attempt_event_payload(request_id, app_type, attempt, None),
        );
    }

    async fn record_success_status_and_maybe_switch(&self, app_type: &str, provider: &Provider) {
        let mut status = self.status.write().await;
        let should_switch = record_forward_success_status(
            &mut status,
            self.current_provider_id_at_start.as_str(),
            provider.id.as_str(),
        );
        if should_switch {
            self.schedule_failover_switch(app_type, provider);
        }
    }

    fn schedule_failover_switch(&self, app_type: &str, provider: &Provider) {
        self.failover_manager.clone().spawn_try_switch(
            self.app_handle.clone(),
            app_type.to_string(),
            provider.id.clone(),
            provider.name.clone(),
        );
    }

    fn emit_request_started(&self, request_id: &str, app_type: &str) {
        self.events.emit(
            "request_started",
            build_request_started_event_payload(request_id, app_type),
        );
    }

    fn emit_attempt_started(&self, request_id: &str, app_type: &str, attempt: &ForwardAttempt) {
        self.events.emit(
            attempt_event_name(attempt.is_channel(), AttemptEventPhase::Started),
            attempt_event_payload(request_id, app_type, attempt, None),
        );
    }

    fn emit_attempt_succeeded(&self, request_id: &str, app_type: &str, attempt: &ForwardAttempt) {
        self.events.emit(
            attempt_event_name(attempt.is_channel(), AttemptEventPhase::Succeeded),
            attempt_event_payload(request_id, app_type, attempt, None),
        );
    }

    fn emit_attempt_failed(
        &self,
        request_id: &str,
        app_type: &str,
        attempt: &ForwardAttempt,
        error: &str,
    ) {
        self.events.emit(
            attempt_event_name(attempt.is_channel(), AttemptEventPhase::Failed),
            attempt_event_payload(request_id, app_type, attempt, Some(error)),
        );
    }

    async fn record_failure_result(
        &self,
        request_id: &str,
        attempt: &ForwardAttempt,
        app_type: &str,
        used_half_open_permit: bool,
        error_msg: String,
    ) {
        if let Some(channel) = attempt.channel() {
            let _ = self
                .router
                .record_channel_result(
                    &channel.channel_id,
                    app_type,
                    used_half_open_permit,
                    false,
                    Some(error_msg.clone()),
                    None,
                )
                .await;
            self.emit_attempt_failed(request_id, app_type, attempt, &error_msg);
            return;
        }

        let _ = self
            .router
            .record_result(
                &attempt.provider().id,
                app_type,
                used_half_open_permit,
                false,
                Some(error_msg.clone()),
            )
            .await;
        self.emit_attempt_failed(request_id, app_type, attempt, &error_msg);
    }

    async fn release_attempt_permit_neutral(
        &self,
        attempt: &ForwardAttempt,
        app_type: &str,
        used_half_open_permit: bool,
    ) {
        if let Some(channel) = attempt.channel() {
            self.router
                .release_channel_permit_neutral(
                    &channel.channel_id,
                    app_type,
                    used_half_open_permit,
                )
                .await;
            return;
        }

        self.router
            .release_permit_neutral(&attempt.provider().id, app_type, used_half_open_permit)
            .await;
    }

    /// 整流（thinking signature 或 budget）重试失败后的统一收尾。
    ///
    /// `None` 表示已记录熔断器、累积 `last_error`/`last_provider`，
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
        rectifier_label: &str,
        last_error: &mut Option<ProxyError>,
        last_provider: &mut Option<Provider>,
    ) -> Option<ForwardError> {
        let provider = attempt.provider();
        // Provider 错误：本家上游/网络确实出问题，下一家 provider 可能可用 → 继续故障转移。
        // 客户端错误：整流后请求仍违法，下一家也修不好 → 直接返回。
        let failure = forward_failure_kind_from_proxy_error(&retry_err);
        let is_provider_error = should_failover_after_rectifier_retry_failure(&failure);

        if is_provider_error {
            self.record_failure_result(
                request_id,
                attempt,
                app_type_str,
                used_half_open_permit,
                retry_err.to_string(),
            )
            .await;
            {
                let mut status = self.status.write().await;
                status.last_error = Some(format!(
                    "Provider {} {rectifier_label}重试失败: {}",
                    provider.name, retry_err
                ));
            }
            *last_error = Some(retry_err);
            *last_provider = Some(provider.clone());
            return None;
        }

        self.release_attempt_permit_neutral(attempt, app_type_str, used_half_open_permit)
            .await;
        let mut status = self.status.write().await;
        status.failed_requests += 1;
        status.last_error = Some(retry_err.to_string());
        if status.total_requests > 0 {
            status.success_rate =
                (status.success_requests as f32 / status.total_requests as f32) * 100.0;
        }
        Some(ForwardError {
            error: retry_err,
            provider: Some(provider.clone()),
        })
    }

    /// Forward a request using attempts planned by a caller such as ProxyEngine.
    ///
    /// This keeps request-scope accounting in one place while allowing the route
    /// planning step to move out of RequestForwarder incrementally.
    #[allow(dead_code)]
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
        let request_id = uuid::Uuid::new_v4().to_string();
        self.emit_request_started(&request_id, app_type.as_str());
        let guard = ActiveConnectionGuard::acquire(self.status.clone()).await;
        {
            let mut s = self.status.write().await;
            s.total_requests = s.total_requests.saturating_add(1);
            s.last_request_at = Some(chrono::Utc::now().to_rfc3339());
        }
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
        // 获取适配器
        let adapter = get_adapter(app_type);
        let app_type_str = app_type.as_str();

        if attempts.is_empty() {
            return Err(ForwardError {
                error: ProxyError::NoAvailableProvider,
                provider: None,
            });
        }

        let mut last_error = None;
        let mut last_provider = None;
        let mut attempted_providers = 0usize;

        // Legacy 单 Provider 场景下跳过熔断器检查（故障转移关闭时）。
        // Materialized channel attempts are already explicit route units and
        // should use channel-level breaker state.
        let bypass_circuit_breaker = attempts.len() == 1 && !attempts[0].is_channel();

        // 依次尝试每个供应商
        for attempt in attempts.iter() {
            let provider = attempt.provider();
            // 整流器重试标记：每个 provider 独立持有，避免标记跨 provider 短路故障转移
            // —— 首家 provider 整流后被 5xx/timeout 击落时，下家仍能用整流后的请求体走整流流程
            let mut rectifier_retried = false;
            let mut budget_rectifier_retried = false;
            let mut media_rectifier_retried = false;

            // 上限检查：尊重用户在 AppProxyConfig.max_retries 上配置的「重试次数」。
            // 放在熔断器 allow 检查之前，避免在已经超限时还占用 HalfOpen 探测名额。
            if attempted_providers >= self.max_attempts {
                log::warn!(
                    "[{app_type_str}] 已达最大尝试次数上限 ({}/{}), 停止故障转移",
                    attempted_providers,
                    self.max_attempts
                );
                break;
            }

            // 发起请求前先获取熔断器放行许可（HalfOpen 会占用探测名额）
            // 单 Provider 场景下跳过此检查，避免熔断器阻塞所有请求
            let (allowed, used_half_open_permit) = if bypass_circuit_breaker {
                (true, false)
            } else if let Some(channel) = attempt.channel() {
                let permit = self
                    .router
                    .allow_channel_request(&channel.channel_id, app_type_str)
                    .await;
                (permit.allowed, permit.used_half_open_permit)
            } else {
                let permit = self
                    .router
                    .allow_provider_request(&provider.id, app_type_str)
                    .await;
                (permit.allowed, permit.used_half_open_permit)
            };

            if !allowed {
                continue;
            }
            self.emit_attempt_started(request_id, app_type_str, attempt);

            // PRE-SEND 优化器：每个 provider 独立决定是否优化
            // clone body 以避免 Bedrock 优化字段泄漏到非 Bedrock provider（failover 场景）
            let mut provider_body = if should_apply_bedrock_pre_send_optimizer(
                self.optimizer_config.enabled,
                bedrock_env_flag_from_provider_settings(&provider.settings_config),
            ) {
                let mut b = body.clone();
                let report = apply_bedrock_pre_send_optimizers(&mut b, &self.optimizer_config);
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
                b
            } else {
                body.clone()
            };

            attempted_providers += 1;

            // 更新状态中的当前 Provider 信息（per-attempt 维度的标识）
            //
            // total_requests / last_request_at / active_connections 已由
            // forward_with_preplanned_attempts 在客户端请求维度统一处理，这里只刷
            // 新「正在尝试哪个 provider」的展示字段。
            {
                let mut status = self.status.write().await;
                status.current_provider = Some(provider.name.clone());
                status.current_provider_id = Some(provider.id.clone());
            }

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
                    adapter.as_ref(),
                )
                .await
            {
                Ok((response, claude_api_format, outbound_model)) => {
                    // 成功：普通闭合熔断状态异步记录，避免阻塞流式首包返回；
                    // HalfOpen 探测仍同步等待，保证 permit 与熔断状态及时释放。
                    self.record_success_result(
                        request_id,
                        attempt,
                        app_type_str,
                        used_half_open_permit,
                    )
                    .await;

                    // 更新当前应用类型使用的 provider/channel
                    self.record_active_target(request_id, app_type_str, attempt)
                        .await;

                    self.record_success_status_and_maybe_switch(app_type_str, provider)
                        .await;

                    return Ok(ForwardResult {
                        response,
                        provider: provider.clone(),
                        claude_api_format,
                        outbound_model,
                        selected_channel: attempt.channel().cloned(),
                        connection_guard: None,
                    });
                }
                Err(e) => {
                    // 检测是否需要触发整流器（仅 Claude/ClaudeAuth 供应商）
                    let provider_type = provider_kind_from_app_type_and_config(app_type, provider);
                    let is_anthropic_provider = matches!(
                        provider_type,
                        ProviderKind::Claude | ProviderKind::ClaudeAuth
                    );
                    let mut signature_rectifier_non_retryable_client_error = false;

                    if self.media_retry_should_trigger(
                        adapter.name(),
                        media_rectifier_retried,
                        &provider_body,
                        &e,
                    ) {
                        let mut media_body = provider_body.clone();
                        let replaced_images = replace_image_blocks_with_marker(&mut media_body);

                        if replaced_images > 0 {
                            let _ = std::mem::replace(&mut media_rectifier_retried, true);
                            let model = media_body
                                .get("model")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            log::info!(
                                "[{app_type_str}] [Media] Upstream rejected image input; retrying provider={} model={} with {replaced_images} image block(s) replaced by {}",
                                provider.id,
                                model,
                                UNSUPPORTED_IMAGE_MARKER
                            );

                            match self
                                .forward(
                                    app_type,
                                    &method,
                                    attempt,
                                    endpoint,
                                    &media_body,
                                    &headers,
                                    &extensions,
                                    adapter.as_ref(),
                                )
                                .await
                            {
                                Ok((response, claude_api_format, outbound_model)) => {
                                    log::info!(
                                        "[{app_type_str}] [Media] Unsupported-image retry succeeded"
                                    );
                                    self.record_success_result(
                                        request_id,
                                        attempt,
                                        app_type_str,
                                        used_half_open_permit,
                                    )
                                    .await;

                                    self.record_active_target(request_id, app_type_str, attempt)
                                        .await;

                                    self.record_success_status_and_maybe_switch(
                                        app_type_str,
                                        provider,
                                    )
                                    .await;

                                    return Ok(ForwardResult {
                                        response,
                                        provider: provider.clone(),
                                        claude_api_format,
                                        outbound_model,
                                        selected_channel: attempt.channel().cloned(),
                                        connection_guard: None,
                                    });
                                }
                                Err(retry_err) => {
                                    log::warn!(
                                        "[{app_type_str}] [Media] Unsupported-image retry still failed: {retry_err}"
                                    );
                                    if let Some(err) = self
                                        .handle_rectifier_retry_failure(
                                            retry_err,
                                            request_id,
                                            attempt,
                                            app_type_str,
                                            used_half_open_permit,
                                            "media 降级",
                                            &mut last_error,
                                            &mut last_provider,
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

                    if is_anthropic_provider {
                        let error_message = match &e {
                            ProxyError::UpstreamError { body, .. } => body.clone(),
                            _ => Some(e.to_string()),
                        };
                        if should_rectify_thinking_signature(
                            error_message.as_deref(),
                            &self.rectifier_config.thinking_signature_core_config(),
                        ) {
                            // 已经重试过：直接返回错误（不可重试客户端错误）
                            if rectifier_retried {
                                log::warn!("[{app_type_str}] [RECT-005] 整流器已触发过，不再重试");
                                // 释放 HalfOpen permit（不记录熔断器，这是客户端兼容性问题）
                                self.release_attempt_permit_neutral(
                                    attempt,
                                    app_type_str,
                                    used_half_open_permit,
                                )
                                .await;
                                let mut status = self.status.write().await;
                                status.failed_requests += 1;
                                status.last_error = Some(e.to_string());
                                if status.total_requests > 0 {
                                    status.success_rate = (status.success_requests as f32
                                        / status.total_requests as f32)
                                        * 100.0;
                                }
                                return Err(ForwardError {
                                    error: e,
                                    provider: Some(provider.clone()),
                                });
                            }

                            // 首次触发：整流请求体
                            let rectified = rectify_anthropic_request(&mut provider_body);

                            // 整流未生效：继续尝试 budget 整流路径，避免误判后短路
                            if !rectified.applied {
                                log::warn!(
                                    "[{app_type_str}] [RECT-006] thinking 签名整流器触发但无可整流内容，继续检查 budget；若 budget 也未命中则按客户端错误返回"
                                );
                                signature_rectifier_non_retryable_client_error = true;
                            } else {
                                log::info!(
                                    "[{}] [RECT-001] thinking 签名整流器触发, 移除 {} thinking blocks, {} redacted_thinking blocks, {} signature fields",
                                    app_type_str,
                                    rectified.removed_thinking_blocks,
                                    rectified.removed_redacted_thinking_blocks,
                                    rectified.removed_signature_fields
                                );

                                // 标记已重试（当前逻辑下重试后必定 return，保留标记以备将来扩展）
                                let _ = std::mem::replace(&mut rectifier_retried, true);

                                // 使用同一供应商重试（不计入熔断器）
                                match self
                                    .forward(
                                        app_type,
                                        &method,
                                        attempt,
                                        endpoint,
                                        &provider_body,
                                        &headers,
                                        &extensions,
                                        adapter.as_ref(),
                                    )
                                    .await
                                {
                                    Ok((response, claude_api_format, outbound_model)) => {
                                        log::info!("[{app_type_str}] [RECT-002] 整流重试成功");
                                        self.record_success_result(
                                            request_id,
                                            attempt,
                                            app_type_str,
                                            used_half_open_permit,
                                        )
                                        .await;

                                        // 更新当前应用类型使用的 provider/channel
                                        self.record_active_target(
                                            request_id,
                                            app_type_str,
                                            attempt,
                                        )
                                        .await;

                                        self.record_success_status_and_maybe_switch(
                                            app_type_str,
                                            provider,
                                        )
                                        .await;

                                        return Ok(ForwardResult {
                                            response,
                                            provider: provider.clone(),
                                            claude_api_format,
                                            outbound_model,
                                            selected_channel: attempt.channel().cloned(),
                                            connection_guard: None,
                                        });
                                    }
                                    Err(retry_err) => {
                                        log::warn!(
                                            "[{app_type_str}] [RECT-003] 整流重试仍失败: {retry_err}"
                                        );
                                        if let Some(err) = self
                                            .handle_rectifier_retry_failure(
                                                retry_err,
                                                request_id,
                                                attempt,
                                                app_type_str,
                                                used_half_open_permit,
                                                "整流",
                                                &mut last_error,
                                                &mut last_provider,
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
                        let error_message = match &e {
                            ProxyError::UpstreamError { body, .. } => body.clone(),
                            _ => Some(e.to_string()),
                        };
                        if should_rectify_thinking_budget(
                            error_message.as_deref(),
                            &self.rectifier_config.thinking_budget_core_config(),
                        ) {
                            // 已经重试过：直接返回错误（不可重试客户端错误）
                            if budget_rectifier_retried {
                                log::warn!(
                                    "[{app_type_str}] [RECT-013] budget 整流器已触发过，不再重试"
                                );
                                self.release_attempt_permit_neutral(
                                    attempt,
                                    app_type_str,
                                    used_half_open_permit,
                                )
                                .await;
                                let mut status = self.status.write().await;
                                status.failed_requests += 1;
                                status.last_error = Some(e.to_string());
                                if status.total_requests > 0 {
                                    status.success_rate = (status.success_requests as f32
                                        / status.total_requests as f32)
                                        * 100.0;
                                }
                                return Err(ForwardError {
                                    error: e,
                                    provider: Some(provider.clone()),
                                });
                            }

                            let budget_rectified = rectify_thinking_budget(&mut provider_body);
                            if !budget_rectified.applied {
                                log::warn!(
                                    "[{app_type_str}] [RECT-014] budget 整流器触发但无可整流内容，不做无意义重试"
                                );
                                self.release_attempt_permit_neutral(
                                    attempt,
                                    app_type_str,
                                    used_half_open_permit,
                                )
                                .await;
                                let mut status = self.status.write().await;
                                status.failed_requests += 1;
                                status.last_error = Some(e.to_string());
                                if status.total_requests > 0 {
                                    status.success_rate = (status.success_requests as f32
                                        / status.total_requests as f32)
                                        * 100.0;
                                }
                                return Err(ForwardError {
                                    error: e,
                                    provider: Some(provider.clone()),
                                });
                            }

                            log::info!(
                                "[{}] [RECT-010] thinking budget 整流器触发, before={:?}, after={:?}",
                                app_type_str,
                                budget_rectified.before,
                                budget_rectified.after
                            );

                            let _ = std::mem::replace(&mut budget_rectifier_retried, true);

                            // 使用同一供应商重试（不计入熔断器）
                            match self
                                .forward(
                                    app_type,
                                    &method,
                                    attempt,
                                    endpoint,
                                    &provider_body,
                                    &headers,
                                    &extensions,
                                    adapter.as_ref(),
                                )
                                .await
                            {
                                Ok((response, claude_api_format, outbound_model)) => {
                                    log::info!("[{app_type_str}] [RECT-011] budget 整流重试成功");
                                    self.record_success_result(
                                        request_id,
                                        attempt,
                                        app_type_str,
                                        used_half_open_permit,
                                    )
                                    .await;

                                    self.record_active_target(request_id, app_type_str, attempt)
                                        .await;

                                    self.record_success_status_and_maybe_switch(
                                        app_type_str,
                                        provider,
                                    )
                                    .await;

                                    return Ok(ForwardResult {
                                        response,
                                        provider: provider.clone(),
                                        claude_api_format,
                                        outbound_model,
                                        selected_channel: attempt.channel().cloned(),
                                        connection_guard: None,
                                    });
                                }
                                Err(retry_err) => {
                                    log::warn!(
                                        "[{app_type_str}] [RECT-012] budget 整流重试仍失败: {retry_err}"
                                    );
                                    if let Some(err) = self
                                        .handle_rectifier_retry_failure(
                                            retry_err,
                                            request_id,
                                            attempt,
                                            app_type_str,
                                            used_half_open_permit,
                                            "budget 整流",
                                            &mut last_error,
                                            &mut last_provider,
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

                    if signature_rectifier_non_retryable_client_error {
                        self.release_attempt_permit_neutral(
                            attempt,
                            app_type_str,
                            used_half_open_permit,
                        )
                        .await;
                        let mut status = self.status.write().await;
                        status.failed_requests += 1;
                        status.last_error = Some(e.to_string());
                        if status.total_requests > 0 {
                            status.success_rate = (status.success_requests as f32
                                / status.total_requests as f32)
                                * 100.0;
                        }
                        return Err(ForwardError {
                            error: e,
                            provider: Some(provider.clone()),
                        });
                    }

                    // 先分类错误，决定是否计入 provider 健康度
                    // —— NonRetryable 是客户端层错误，无论换哪家 provider 都会被拒绝，
                    //    不应污染熔断器和数据库健康度（与 release_permit_neutral 同语义）。
                    let failure = forward_failure_kind_from_proxy_error(&e);
                    let category = categorize_forward_failure(&failure);

                    match category {
                        ForwardFailureCategory::Retryable => {
                            // 可重试：真正的 provider 故障 → 记录失败并更新熔断器/DB 健康度
                            self.record_failure_result(
                                request_id,
                                attempt,
                                app_type_str,
                                used_half_open_permit,
                                e.to_string(),
                            )
                            .await;

                            {
                                let mut status = self.status.write().await;
                                status.last_error =
                                    Some(format!("Provider {} 失败: {}", provider.name, e));
                            }

                            let failure_log = build_retryable_forward_failure_log(
                                &provider.name,
                                attempted_providers,
                                attempts.len(),
                                &failure,
                            );
                            log::warn!(
                                "[{app_type_str}] [{}] {}",
                                failure_log.code,
                                failure_log.message
                            );

                            last_error = Some(e);
                            last_provider = Some(provider.clone());
                            // 继续尝试下一个供应商
                            continue;
                        }
                        ForwardFailureCategory::NonRetryable => {
                            // 不可重试：客户端层错误或客户端断连 → 不污染健康度，仅释放 HalfOpen permit
                            self.release_attempt_permit_neutral(
                                attempt,
                                app_type_str,
                                used_half_open_permit,
                            )
                            .await;
                            {
                                let mut status = self.status.write().await;
                                status.failed_requests += 1;
                                status.last_error = Some(e.to_string());
                                if status.total_requests > 0 {
                                    status.success_rate = (status.success_requests as f32
                                        / status.total_requests as f32)
                                        * 100.0;
                                }
                            }
                            return Err(ForwardError {
                                error: e,
                                provider: Some(provider.clone()),
                            });
                        }
                    }
                }
            }
        }

        if attempted_providers == 0 {
            // providers 列表非空，但全部被熔断器拒绝（典型：HalfOpen 探测名额被占用）
            {
                let mut status = self.status.write().await;
                status.failed_requests += 1;
                status.last_error = Some("所有供应商暂时不可用（熔断器限制）".to_string());
                if status.total_requests > 0 {
                    status.success_rate =
                        (status.success_requests as f32 / status.total_requests as f32) * 100.0;
                }
            }
            return Err(ForwardError {
                error: ProxyError::NoAvailableProvider,
                provider: None,
            });
        }

        // 所有供应商都失败了
        {
            let mut status = self.status.write().await;
            status.failed_requests += 1;
            status.last_error = Some("所有供应商都失败".to_string());
            if status.total_requests > 0 {
                status.success_rate =
                    (status.success_requests as f32 / status.total_requests as f32) * 100.0;
            }
        }

        let last_failure = last_error
            .as_ref()
            .map(forward_failure_kind_from_proxy_error);
        if let Some(failure_log) = build_terminal_forward_failure_log(
            attempted_providers,
            attempts.len(),
            last_failure.as_ref(),
        ) {
            log::warn!(
                "[{app_type_str}] [{}] {}",
                failure_log.code,
                failure_log.message
            );
        }

        Err(ForwardError {
            error: last_error.unwrap_or(ProxyError::MaxRetriesExceeded),
            provider: last_provider,
        })
    }

    /// 转发单个请求（使用适配器）
    ///
    /// 成功时返回 `(response, claude_api_format, outbound_model)`，其中
    /// `outbound_model` 是最终发往上游的模型名（所有映射/改写之后）。
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
        adapter: &dyn ProviderAdapter,
    ) -> Result<(ProxyResponse, Option<String>, Option<String>), ProxyError> {
        let provider = attempt.provider();
        // 使用适配器提取 base_url
        let mut base_url = adapter.extract_base_url(provider)?;

        let is_full_url = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.is_full_url)
            .unwrap_or(false);

        // GitHub Copilot API 使用 /chat/completions（无 /v1 前缀）
        let is_copilot = is_github_copilot_upstream(
            provider
                .meta
                .as_ref()
                .and_then(|m| m.provider_type.as_deref()),
            &base_url,
        );

        // 应用模型映射（独立于格式转换）
        // Claude Desktop proxy 模式必须先把 Desktop 可见的 claude-* route
        // 映射成真实上游模型名，并且未知 route 要直接报错，不能使用默认模型兜底。
        let mapped_body = if matches!(app_type, AppType::ClaudeDesktop) {
            crate::claude_desktop_config::map_proxy_request_model(body.clone(), provider)
                .map_err(|e| ProxyError::InvalidRequest(e.to_string()))?
        } else {
            let projection = crate::proxy_core_adapter::apply_provider_model_mapping(
                body.clone(),
                &provider.settings_config,
            );
            if let Some(message) = projection.log_message {
                log::debug!("{message}");
            }
            projection.body
        };

        // 与 CCH 对齐：请求前不做 thinking 主动改写（仅保留兼容入口）
        let mut mapped_body = normalize_thinking_type(mapped_body);
        apply_channel_model_override(&mut mapped_body, attempt);

        if is_copilot {
            let original_model = mapped_body
                .get("model")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
            mapped_body = apply_copilot_model_normalization(mapped_body);
            if let (Some(original), Some(normalized)) = (
                original_model.as_deref(),
                mapped_body.get("model").and_then(|value| value.as_str()),
            ) {
                if original != normalized {
                    log::debug!("[CopilotNormalizer] {original} -> {normalized}");
                }
            }
            self.apply_copilot_live_model_resolution(provider, &mut mapped_body)
                .await;
        } else {
            let one_m_model_change =
                mapped_body
                    .get("model")
                    .and_then(Value::as_str)
                    .and_then(|model| {
                        let stripped = strip_one_m_suffix_for_upstream(model);
                        (stripped != model).then(|| (model.to_string(), stripped.to_string()))
                    });
            mapped_body = strip_one_m_suffix_for_upstream_from_body(mapped_body);
            if let Some((model, stripped)) = one_m_model_change {
                log::debug!("[ModelMapper] 去除本地 1M 标记: {model} → {stripped}");
            }
        }

        // --- Copilot 优化器：分类 + 请求体优化（在格式转换之前执行） ---
        // 注意：确定性 ID 也在此处计算，因为 mapped_body 在格式转换时会被 move
        //
        // 执行顺序（与 copilot-api 对齐）：
        //   1. 先在原始 body 上分类（保留 tool_result 语义，避免误判为 user）
        //   2. 再清洗孤立 tool_result（防止上游 API 报错）
        //   3. 再合并 tool_result + text（减少 premium 计费）
        let copilot_optimization = if is_copilot && self.copilot_optimizer_config.enabled {
            // 1. 在原始 body 上分类 — 必须在清洗/合并之前执行
            //    孤立 tool_result 仍保持 tool_result 类型，分类能正确识别为 agent
            let has_anthropic_beta = headers.contains_key("anthropic-beta");
            let classification = classify_copilot_request(
                &mapped_body,
                has_anthropic_beta,
                self.copilot_optimizer_config.compact_detection,
                self.copilot_optimizer_config.subagent_detection,
            );

            log::debug!(
                "[Copilot] 优化器分类: initiator={}, is_warmup={}, is_compact={}, is_subagent={}",
                classification.initiator,
                classification.is_warmup,
                classification.is_compact,
                classification.is_subagent
            );

            // 2. 孤立 tool_result 清理 — 分类完成后再清洗
            //    防止上游 API 因不匹配的 tool_result 报错导致重试/重复计费
            mapped_body = sanitize_copilot_orphan_tool_results(mapped_body);

            // 3. Tool result 合并 — 将 [tool_result, text] 变为 [tool_result(含text)]
            if self.copilot_optimizer_config.tool_result_merging {
                mapped_body = merge_copilot_tool_results(mapped_body);
            }

            // 3.5. 主动剥离 thinking block — Copilot 走 OpenAI 兼容端点不识别该块
            //      避免上游拒绝后由 rectifier 反应式重试（首次请求已消耗 quota）
            if self.copilot_optimizer_config.strip_thinking {
                mapped_body = strip_copilot_thinking_blocks(mapped_body);
            }

            // 4. Warmup 小模型降级
            let warmup_override = apply_copilot_warmup_model_override(
                mapped_body,
                self.copilot_optimizer_config.warmup_downgrade,
                classification.is_warmup,
                &self.copilot_optimizer_config.warmup_model,
            );
            if let Some(warmup_model) = &warmup_override.applied_model {
                log::info!("[Copilot] Warmup 请求降级到模型: {}", warmup_model);
            }
            mapped_body = warmup_override.body;

            // 预计算确定性 Request ID（在 body 被 move 之前）
            // Session 提取优先级由 proxy-core::request_optimizer 固化：
            //   1. metadata.user_id 中的 _session_ 后缀
            //   2. metadata.session_id（直接字段）
            //   3. raw metadata.user_id（整串 fallback）
            //   4. x-session-id header
            let session_id = resolve_copilot_optimizer_session_id(body, headers);
            let det_request_id = if self.copilot_optimizer_config.deterministic_request_id {
                Some(resolve_copilot_request_id_with_fallback(
                    &mapped_body,
                    &session_id,
                    || uuid::Uuid::new_v4().to_string(),
                ))
            } else {
                None
            };

            // 从 session ID 派生稳定的 interaction ID（同一主对话共享）
            let interaction_id = resolve_copilot_deterministic_interaction_id(&session_id);

            Some((classification, det_request_id, interaction_id))
        } else {
            None
        };

        if should_resolve_copilot_dynamic_endpoint(is_copilot, is_full_url) {
            if let Some(dynamic_endpoint) =
                resolve_copilot_api_endpoint(self.app_handle.as_ref(), provider).await
            {
                if let Some(next_base_url) = resolved_copilot_dynamic_base_url(
                    &base_url,
                    &dynamic_endpoint,
                    is_copilot,
                    is_full_url,
                ) {
                    log::debug!(
                        "[Copilot] 使用动态 API endpoint: {} (原: {})",
                        next_base_url,
                        base_url
                    );
                    base_url = next_base_url;
                }
            }
        }
        let resolved_claude_api_format = if adapter.name() == "Claude" {
            Some(
                self.resolve_claude_api_format(provider, &mapped_body, is_copilot)
                    .await,
            )
        } else {
            None
        };
        if adapter.name() == "Claude" {
            if let Some(api_format) = resolved_claude_api_format.as_deref() {
                super::providers::normalize_anthropic_messages_for_provider(
                    &mut mapped_body,
                    provider,
                    api_format,
                );
                self.apply_media_prevention(&mut mapped_body, provider);
            }
        }
        let needs_transform = match resolved_claude_api_format.as_deref() {
            Some(api_format) => {
                crate::proxy_core_adapter::claude_api_format_needs_transform(api_format)
            }
            None => adapter.needs_transform(provider),
        };
        let codex_responses_to_chat = matches!(app_type, AppType::Codex)
            && super::providers::should_convert_codex_responses_to_chat(provider, endpoint);
        let claude_api_format_for_url = resolved_claude_api_format.as_deref().or_else(|| {
            (adapter.name() == "Claude").then(|| super::providers::get_claude_api_format(provider))
        });
        let url_plan = forward_upstream_url_plan(
            ForwardUpstreamUrlPlanInput {
                base_url: &base_url,
                endpoint,
                is_full_url,
                codex_responses_to_chat,
                use_claude_transform: needs_transform && adapter.name() == "Claude",
                is_copilot,
                claude_api_format: claude_api_format_for_url,
                body: &mapped_body,
                channel_param_overrides: attempt.channel().map(|channel| &channel.param_overrides),
            },
            |base_url, effective_endpoint| adapter.build_url(base_url, effective_endpoint),
        );
        let effective_endpoint = url_plan.effective_endpoint;
        let url = url_plan.url;

        // 记录映射后的出站模型名（此时 mapped_body 已完成接管映射 / [1m] 剥离 /
        // Copilot 归一化）。格式转换后若 body 仍带 model 字段会在下方刷新覆盖；
        // gemini_native 等模型在 URL 中的格式则保留此处的转换前真值。
        let mut outbound_model = mapped_body
            .get("model")
            .and_then(|m| m.as_str())
            .filter(|m| !m.is_empty())
            .map(str::to_string);

        // 转换请求体（如果需要）
        let mut request_body = if codex_responses_to_chat {
            let mut mapped_body = mapped_body;
            let restored = self
                .codex_chat_history
                .enrich_request(&mut mapped_body)
                .await;
            if restored > 0 {
                log::debug!(
                    "[Codex] Restored or enriched {restored} cached function call item(s) for Chat upstream"
                );
            }
            super::providers::apply_codex_chat_upstream_model(provider, &mut mapped_body);
            let reasoning_options =
                super::providers::resolve_codex_chat_reasoning_options(provider, &mapped_body);
            let model = mapped_body
                .get("model")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            responses_to_chat_completions_with_options(
                &mapped_body,
                reasoning_options.as_ref(),
                is_openai_o_series(model),
                supports_reasoning_effort(model),
            )
        } else if needs_transform {
            if adapter.name() == "Claude" {
                let api_format = resolved_claude_api_format
                    .as_deref()
                    .unwrap_or_else(|| super::providers::get_claude_api_format(provider));
                super::providers::transform_claude_request_for_api_format(
                    mapped_body,
                    provider,
                    api_format,
                    self.session_client_provided
                        .then_some(self.session_id.as_str()),
                    Some(self.gemini_shadow.as_ref()),
                )?
            } else {
                adapter.transform_request(mapped_body, provider)?
            }
        } else {
            mapped_body
        };

        if matches!(app_type, AppType::Codex) {
            self.apply_media_prevention(&mut request_body, provider);
        }

        // 过滤私有参数（以 `_` 开头的字段），防止内部信息泄露到上游
        // 默认使用空白名单，过滤所有 _ 前缀字段
        let prepared_body = prepare_upstream_request_body_with_report(request_body);
        if let Some(message) = request_body_filter_log_message(&prepared_body) {
            log::debug!("{message}");
        }
        let filtered_body = prepared_body.body;
        // 出站 body 定稿后刷新真值（覆盖 Codex chat 上游模型覆写、转换层模型改写）
        if let Some(m) = filtered_body
            .get("model")
            .and_then(|m| m.as_str())
            .filter(|m| !m.is_empty())
        {
            outbound_model = Some(m.to_string());
        }
        if log::log_enabled!(log::Level::Debug) {
            log::debug!(
                "{}",
                prompt_cache_trace_log_message(PromptCacheTraceLogInput {
                    app: app_type.as_str(),
                    provider_id: provider.id.as_str(),
                    endpoint: &effective_endpoint,
                    api_format: resolved_claude_api_format.as_deref(),
                    body: &filtered_body,
                    session_client_provided: self.session_client_provided,
                })
            );
        }
        let transport_policy = crate::proxy_core_adapter::resolve_upstream_request_transport_policy(
            needs_transform,
            codex_responses_to_chat,
            &effective_endpoint,
            &filtered_body,
            headers,
        );
        let request_is_streaming = transport_policy.is_streaming_request;
        let force_identity_encoding = transport_policy.force_identity_encoding;

        // Codex OAuth 需要注入的 ChatGPT-Account-Id（在动态 token 获取期间填充）
        let mut codex_oauth_account_id: Option<String> = None;
        let mut should_send_codex_oauth_session_headers = false;

        // 获取认证头（提前准备，用于内联替换）
        let auth_provider = attempt.auth_provider();
        let mut auth_headers = if let Some(mut auth) = adapter.extract_auth(auth_provider) {
            let managed_auth =
                resolve_managed_account_auth(self.app_handle.as_ref(), auth_provider, auth).await?;
            auth = managed_auth.auth;
            should_send_codex_oauth_session_headers =
                managed_auth.should_send_codex_oauth_session_headers;
            codex_oauth_account_id = managed_auth.codex_oauth_account_id;

            adapter.get_auth_headers(&auth)?
        } else {
            Vec::new()
        };

        let codex_oauth_session_headers =
            if should_send_codex_oauth_session_headers && self.session_client_provided {
                build_codex_oauth_session_headers(&self.session_id)
            } else {
                Vec::new()
            };

        // 自定义 User-Agent：与 stream_check / model_fetch 共用 parse_custom_user_agent，
        // 运行时静默忽略非法值（前端在输入处给非阻断提示，不在保存时阻断）。
        // Copilot 指纹 UA 不可覆盖。
        let custom_user_agent = if is_copilot {
            None
        } else {
            provider
                .meta
                .as_ref()
                .and_then(|meta| meta.custom_user_agent_header().ok().flatten())
        };

        // --- Copilot 优化器：动态 header 注入 ---
        let copilot_auth_header_overrides = copilot_optimization.as_ref().map(
            |(classification, det_request_id, interaction_id)| CopilotAuthHeaderOverrides {
                initiator: self
                    .copilot_optimizer_config
                    .request_classification
                    .then_some(classification.initiator),
                is_subagent: classification.is_subagent,
                deterministic_request_id: det_request_id.as_deref(),
                interaction_id: interaction_id.as_deref(),
            },
        );

        auth_headers = build_upstream_auth_headers(UpstreamAuthHeadersInput {
            base_auth_headers: &auth_headers,
            codex_oauth_account_id: codex_oauth_account_id.as_deref(),
            copilot_overrides: copilot_auth_header_overrides,
        });

        if let Some((ref classification, _, _)) = copilot_optimization {
            if classification.is_subagent {
                log::info!(
                    "[Copilot] 子代理请求: x-initiator=agent, x-interaction-type=conversation-subagent"
                );
            }
        }

        // 预计算上游 host 值（用于在原位替换 host header）
        let upstream_host = crate::proxy_core_adapter::upstream_host_header_from_url(&url);

        let should_send_anthropic_headers = should_send_anthropic_request_headers(
            adapter.name(),
            resolved_claude_api_format.as_deref(),
        );

        // 预计算 anthropic-beta 值（仅 Claude）
        let anthropic_beta_value = if should_send_anthropic_headers {
            Some(crate::proxy_core_adapter::anthropic_beta_header_value(
                headers
                    .get("anthropic-beta")
                    .and_then(|beta| beta.to_str().ok()),
            ))
        } else {
            None
        };

        let ordered_headers =
            crate::proxy_core_adapter::build_upstream_request_headers(UpstreamRequestHeadersInput {
                inbound_headers: headers,
                upstream_host: upstream_host.as_deref(),
                auth_headers: &auth_headers,
                channel_header_overrides: attempt
                    .channel()
                    .map(|channel| &channel.header_overrides),
                force_identity_encoding,
                custom_user_agent: custom_user_agent.as_ref(),
                is_copilot,
                should_send_anthropic_headers,
                anthropic_beta_value: anthropic_beta_value.as_deref(),
                codex_oauth_session_headers: &codex_oauth_session_headers,
                ensure_json_content_type: true,
            });

        let body_bytes =
            crate::proxy_core_adapter::serialize_upstream_request_body(method, &filtered_body)
                .map_err(|e| {
                    ProxyError::Internal(format!("Failed to serialize request body: {e}"))
                })?;

        validate_managed_account_upstream_auth(&url, &ordered_headers)
            .map_err(|error| ProxyError::AuthError(error.to_string()))?;

        // 输出请求信息日志
        let tag = adapter.name();
        let request_model = filtered_body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("<none>");
        log::info!("[{tag}] >>> 请求 URL: {url} (model={request_model})");
        if log::log_enabled!(log::Level::Debug) {
            if let Ok(body_str) = serde_json::to_string(&filtered_body) {
                log::debug!(
                    "[{tag}] >>> 请求体内容 ({}字节): {}",
                    body_str.len(),
                    body_str
                );
            }
        }

        // 获取全局代理 URL
        let upstream_proxy_url: Option<String> = super::http_client::get_current_proxy_url();

        let preserve_exact_header_case = should_preserve_exact_request_header_case(
            adapter.name(),
            provider.is_codex_oauth(),
            is_copilot,
            resolved_claude_api_format.as_deref(),
        );
        let send_policy = crate::proxy_core_adapter::resolve_upstream_send_policy(
            UpstreamSendPolicyInput {
                is_socks_proxy: crate::proxy_core_adapter::is_socks_proxy_url(
                    upstream_proxy_url.as_deref(),
                ),
                preserve_exact_header_case,
                request_is_streaming,
                non_streaming_timeout: self.non_streaming_timeout,
                streaming_first_byte_timeout: self.streaming_first_byte_timeout,
            },
        );

        // 发送请求
        let response = if matches!(send_policy.transport, UpstreamTransportKind::PooledReqwest) {
            // OpenAI / Copilot / Codex 类后端不依赖原始 header 大小写；走 reqwest
            // 连接池，避免 raw TCP/TLS path 每次请求都重新握手。SOCKS5 也只能走 reqwest。
            log::debug!(
                "[Forwarder] Using pooled reqwest client (preserve_exact_header_case={}, socks_proxy={})",
                preserve_exact_header_case,
                crate::proxy_core_adapter::is_socks_proxy_url(upstream_proxy_url.as_deref())
            );
            let client = super::http_client::get();
            let mut request = client.request(method.clone(), &url);
            if let Some(request_timeout) = send_policy.reqwest_request_timeout {
                request = request.timeout(request_timeout);
            }
            for (key, value) in &ordered_headers {
                request = request.header(key, value);
            }
            let send = request.body(body_bytes).send();
            let send_result = if let Some(header_timeout) = send_policy.streaming_header_timeout {
                tokio::time::timeout(header_timeout, send)
                    .await
                    .map_err(|_| {
                        ProxyError::Timeout(format!(
                            "流式响应首包超时: {}s（上游未返回响应头）",
                            header_timeout.as_secs()
                        ))
                    })?
            } else {
                send.await
            };
            let reqwest_resp = send_result.map_err(reqwest_send_error_to_proxy_error)?;
            ProxyResponse::Reqwest(reqwest_resp)
        } else {
            // HTTP 代理或直连：走 hyper raw write（保持 header 大小写）
            // 如果有 HTTP 代理，hyper_client 会用 CONNECT 隧道穿过代理
            let uri: http::Uri = url
                .parse()
                .map_err(|e| ProxyError::ForwardFailed(format!("Invalid URL '{url}': {e}")))?;
            super::hyper_client::send_request(
                uri,
                method.clone(),
                ordered_headers,
                extensions.clone(),
                body_bytes,
                send_policy.base_timeout,
                upstream_proxy_url.as_deref(),
            )
            .await?
        };

        let response = self.apply_channel_response_status_mapping(response, attempt)?;

        // 检查响应状态
        let status = response.status();

        if status.is_success() {
            let response = self
                .prepare_success_response_for_failover(response, request_is_streaming)
                .await?;
            Ok((response, resolved_claude_api_format, outbound_model))
        } else {
            let status_code = status.as_u16();
            let body_text = String::from_utf8(response.bytes().await?.to_vec()).ok();

            Err(ProxyError::UpstreamError {
                status: status_code,
                body: body_text,
            })
        }
    }

    fn apply_channel_response_status_mapping(
        &self,
        response: ProxyResponse,
        attempt: &ForwardAttempt,
    ) -> Result<ProxyResponse, ProxyError> {
        let Some(channel) = attempt.channel() else {
            return Ok(response);
        };

        let status = response.status();
        let Some(mapped) =
            mapped_channel_response_status(status.as_u16(), &channel.status_code_mapping)
        else {
            return Ok(response);
        };
        let mapped_status = http::StatusCode::from_u16(mapped).map_err(|error| {
            ProxyError::Internal(format!(
                "invalid mapped channel response status {mapped}: {error}"
            ))
        })?;

        if mapped_status != status {
            log::debug!(
                "[ChannelRoute] response status mapped via channel {}: {} -> {}",
                channel.channel_id,
                status.as_u16(),
                mapped_status.as_u16()
            );
        }

        Ok(response.with_status(mapped_status))
    }

    /// 故障转移开启时，成功不能只看上游响应头。
    ///
    /// - 非流式：先把完整 body 读到内存，读超时/连接中断会回到 retry loop 尝试下一家。
    /// - 流式：至少等首个 chunk 到达，避免上游返回 200 后一直不吐 SSE 时被误记成功。
    async fn prepare_success_response_for_failover(
        &self,
        response: ProxyResponse,
        request_is_streaming: bool,
    ) -> Result<ProxyResponse, ProxyError> {
        if request_is_streaming {
            return self.prime_streaming_response(response).await;
        }

        if self.non_streaming_timeout.is_zero() {
            return Ok(response);
        }

        let status = response.status();
        let headers = response.headers().clone();
        let body_timeout = self.non_streaming_timeout;
        let body = tokio::time::timeout(body_timeout, response.bytes())
            .await
            .map_err(|_| {
                ProxyError::Timeout(format!(
                    "响应体读取超时: {}s（上游发完响应头后 body 未到达）",
                    body_timeout.as_secs()
                ))
            })??;

        Ok(ProxyResponse::buffered(status, headers, body))
    }

    async fn prime_streaming_response(
        &self,
        response: ProxyResponse,
    ) -> Result<ProxyResponse, ProxyError> {
        if self.streaming_first_byte_timeout.is_zero() {
            return Ok(response);
        }

        let status = response.status();
        let headers = response.headers().clone();
        let timeout = self.streaming_first_byte_timeout;
        let mut stream = Box::pin(response.bytes_stream());

        let first = tokio::time::timeout(timeout, stream.next())
            .await
            .map_err(|_| {
                ProxyError::Timeout(format!(
                    "流式响应首包超时: {}s（上游已返回响应头但未返回数据）",
                    timeout.as_secs()
                ))
            })?;

        let Some(first) = first else {
            return Err(ProxyError::ForwardFailed(
                "流式响应在首包到达前结束".to_string(),
            ));
        };

        let first =
            first.map_err(|e| ProxyError::ForwardFailed(format!("读取流式响应首包失败: {e}")))?;

        let replay = futures::stream::once(async move { Ok(first) }).chain(stream);
        Ok(ProxyResponse::streamed(status, headers, replay))
    }

    async fn resolve_claude_api_format(
        &self,
        provider: &Provider,
        body: &Value,
        is_copilot: bool,
    ) -> String {
        let model = body.get("model").and_then(|value| value.as_str());
        let copilot_model_vendor = if is_copilot {
            match model {
                Some(model_id) => {
                    resolve_copilot_model_vendor(self.app_handle.as_ref(), provider, model_id)
                        .await
                }
                None => None,
            }
        } else {
            None
        };

        resolve_claude_forward_api_format(
            super::providers::get_claude_api_format(provider),
            is_copilot,
            copilot_model_vendor.as_deref(),
        )
    }

    /// 用 Copilot live `/models` 列表确认 model ID 真实可用，找不到时按 family 降级。
    /// 命中缓存后是同步的；首次请求或 5 min 缓存过期后会触发一次 HTTP。
    async fn apply_copilot_live_model_resolution(
        &self,
        provider: &Provider,
        body: &mut serde_json::Value,
    ) {
        let Some(model_id) = body.get("model").and_then(|v| v.as_str()) else {
            return;
        };
        let model_id = model_id.to_string();

        let models = match fetch_copilot_live_models(self.app_handle.as_ref(), provider).await {
            Ok(Some(models)) => models,
            Ok(None) => return,
            Err(err) => {
                log::debug!("[Copilot] live model list unavailable, skip resolution: {err}");
                return;
            }
        };

        if let Some(resolved) = resolve_copilot_model_against_ids(
            &model_id,
            models.iter().map(|model| model.id.as_str()),
        ) {
            log::info!("[Copilot] live-model resolve: {model_id} → {resolved}");
            body["model"] = serde_json::Value::String(resolved);
        }
    }
}

fn attempt_event_payload(
    request_id: &str,
    app_type: &str,
    attempt: &ForwardAttempt,
    error: Option<&str>,
) -> Value {
    let provider = attempt.provider();
    let channel = attempt.channel().map(|channel| AttemptEventChannel {
        channel_id: channel.channel_id.as_str(),
        channel_name: channel.channel_name.as_str(),
        interface_kind: channel.interface_kind.as_str(),
        public_model: channel.public_model.as_deref(),
        upstream_model: channel.upstream_model.as_deref(),
    });

    build_attempt_event_payload(AttemptEventPayloadInput {
        request_id,
        app_type,
        provider_id: provider.id.as_str(),
        provider_name: provider.name.as_str(),
        channel,
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::proxy_core_adapter::ManagedAccountAuthError;
    use crate::proxy_core_adapter::{canonical_json_string, short_value_hash};
    use crate::proxy_core_adapter::{
        interface_kind_for_forward, request_model_for_forward, AppKind, ChannelRouteCandidate,
        ResolvedChannelAttempt,
        claude_transform_endpoint_rewrite_input_from_body as transform_endpoint_rewrite_input,
        rewrite_claude_transform_endpoint as rewrite_transform_endpoint,
    };
    use axum::http::header::{HeaderValue, ACCEPT};
    use axum::http::HeaderMap;
    use bytes::Bytes;
    use http::StatusCode;
    use serde_json::json;
    use std::collections::HashMap;
    use std::time::Duration;

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

        assert_eq!(
            bedrock_env_flag_from_provider_settings(&provider.settings_config),
            Some("1")
        );
    }

    fn test_forwarder(
        non_streaming_timeout: Duration,
        streaming_first_byte_timeout: Duration,
    ) -> RequestForwarder {
        let db = Arc::new(Database::memory().expect("memory db"));

        RequestForwarder {
            router: Arc::new(ProviderRouter::new(db.clone())),
            status: Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            current_providers: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(ProxyEventBus::default()),
            gemini_shadow: Arc::new(GeminiShadowStore::new()),
            codex_chat_history: Arc::new(CodexChatHistoryStore::default()),
            failover_manager: Arc::new(FailoverSwitchManager::new(db)),
            app_handle: None,
            current_provider_id_at_start: String::new(),
            session_id: String::new(),
            session_client_provided: false,
            rectifier_config: RectifierConfig::default(),
            optimizer_config: OptimizerConfig::default(),
            copilot_optimizer_config: CopilotOptimizerConfig::default(),
            non_streaming_timeout,
            streaming_first_byte_timeout,
            max_attempts: 1,
        }
    }

    #[tokio::test]
    async fn forwarder_event_helpers_emit_channel_attempt_payloads() {
        let forwarder = test_forwarder(Duration::from_secs(0), Duration::from_secs(0));
        let mut subscriber = forwarder.events.subscribe();
        let provider = test_provider_with_type(None);
        let attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            ChannelRouteCandidate {
                channel_id: "channel-a".to_string(),
                provider_id: provider.id.clone(),
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

        forwarder.emit_attempt_started("req-1", "claude", &attempt);
        let attempt_event = subscriber.recv().await.expect("attempt event");
        assert_eq!(attempt_event.event, "channel_attempt");
        assert_eq!(attempt_event.payload["requestId"], "req-1");
        assert_eq!(attempt_event.payload["providerId"], "provider-1");
        assert_eq!(attempt_event.payload["channelId"], "channel-a");
        assert_eq!(attempt_event.payload["interfaceKind"], "openai_responses");
        assert_eq!(attempt_event.payload["upstreamModel"], "upstream-sonnet");

        forwarder.emit_attempt_succeeded("req-1", "claude", &attempt);
        let success_event = subscriber.recv().await.expect("success event");
        assert_eq!(success_event.event, "channel_succeeded");
        assert_eq!(success_event.payload["channelName"], "Relay A");
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
            },
        );
        let response = ProxyResponse::buffered(
            StatusCode::TOO_MANY_REQUESTS,
            HeaderMap::new(),
            Bytes::from_static(b"ok"),
        );

        let mapped = forwarder
            .apply_channel_response_status_mapping(response, &attempt)
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
            },
        );
        let response = ProxyResponse::buffered(
            StatusCode::TOO_MANY_REQUESTS,
            HeaderMap::new(),
            Bytes::from_static(b"rate limited"),
        );

        let mapped = forwarder
            .apply_channel_response_status_mapping(response, &attempt)
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
            provider.is_codex_oauth(),
            false,
            Some("anthropic"),
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Claude",
            provider.is_codex_oauth(),
            false,
            Some("openai_responses"),
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Codex",
            provider.is_codex_oauth(),
            false,
            None
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Gemini",
            provider.is_codex_oauth(),
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
            codex_oauth.is_codex_oauth(),
            false,
            Some("openai_responses"),
        ));
        assert!(!should_preserve_exact_request_header_case(
            "Claude",
            copilot.is_codex_oauth(),
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

    fn forwarder_with_rectifier(config: RectifierConfig) -> RequestForwarder {
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
    #[test]
    fn prevention_replaces_when_all_switches_on_and_model_in_heuristic_list() {
        let fwd = forwarder_with_rectifier(RectifierConfig::default());
        let provider = provider_with_settings(json!({}));
        let mut body = body_with_image("deepseek-v4-pro");

        let replaced = fwd.apply_media_prevention(&mut body, &provider);

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

        let replaced = fwd.apply_media_prevention(&mut body, &provider);

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

        assert_eq!(fwd.apply_media_prevention(&mut body, &provider), 0);
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
            fwd.apply_media_prevention(&mut list_body, &bare_provider),
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
            fwd.apply_media_prevention(&mut declared_body, &declared_provider),
            1,
            "显式 text-only 即使关闭 heuristic 也应预替换"
        );
        assert_eq!(declared_body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn reactive_triggers_when_all_switches_on() {
        let fwd = forwarder_with_rectifier(RectifierConfig::default());
        let body = body_with_image("any-model");
        assert!(fwd.media_retry_should_trigger("Claude", false, &body, &image_unsupported_error()));
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

        assert!(fwd.media_retry_should_trigger("Codex", false, &body, &error));
    }

    #[test]
    fn reactive_skipped_when_media_fallback_off() {
        // 关闭 request_media_fallback：上游报图片错误也不触发兜底重试。
        let fwd = forwarder_with_rectifier(RectifierConfig {
            request_media_fallback: false,
            ..RectifierConfig::default()
        });
        let body = body_with_image("any-model");
        assert!(!fwd.media_retry_should_trigger(
            "Claude",
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
        assert!(!fwd.media_retry_should_trigger(
            "Claude",
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
        assert!(fwd.media_retry_should_trigger("Claude", false, &body, &image_unsupported_error()));
    }
}
