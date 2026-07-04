use super::context::RequestContext;
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy::error_mapper::{
    proxy_core_error_to_proxy_error, proxy_error_display_message, proxy_error_status_code,
};
use crate::proxy::host::cc_switch::provider_projection::provider_kind_from_provider;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy_core::api::domain::{AppKind, ProviderKind};
use crate::proxy_core::api::errors::{selected_provider_not_applied_message, ProxyCoreError};
use crate::proxy_core::api::ports::ProxyServices;
use crate::proxy_core::api::usage::{
    error_usage_record_with_request_id_fallback,
    non_streaming_response_usage_record_from_body_with_request_id_fallback,
    streaming_response_usage_record_with_optional_outbound_model,
    transformed_response_usage_record_with_request_id_fallback,
    transformed_streaming_response_usage_record_with_request_id_fallback,
    usage_logging_enabled_from_config_flag, usage_record_debug_log_message,
    usage_record_failure_warning_message, usage_record_with_route_context,
    usage_selected_provider_missing_log_message, NonStreamingResponseUsageRecord,
    StreamingResponseUsageRecord, TokenUsage, TransformedResponseUsageFormat, UsageParserConfig,
    UsageRecord, UsageRecordFailureLogContext, UsageRouteContext,
    UsageSelectedProviderMissingPhase,
};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct ResponseUsageProviderFacts {
    pub(crate) provider_id: String,
    pub(crate) provider_kind: Option<ProviderKind>,
    pub(crate) app: AppKind,
}

pub(crate) fn response_usage_provider_facts(
    provider: &Provider,
    app_type: &str,
) -> ResponseUsageProviderFacts {
    ResponseUsageProviderFacts {
        provider_id: provider.id.clone(),
        provider_kind: provider_kind_from_provider(provider),
        app: AppKind::from(app_type),
    }
}

pub(crate) fn fallback_response_usage_provider_facts(
    provider_id: String,
    app_type: &str,
) -> ResponseUsageProviderFacts {
    ResponseUsageProviderFacts {
        provider_id,
        provider_kind: None,
        app: AppKind::from(app_type),
    }
}

pub(crate) fn response_usage_provider_facts_from_optional(
    provider: Option<&Provider>,
    app_type: &str,
    tag: &str,
    phase: UsageSelectedProviderMissingPhase,
) -> Result<ResponseUsageProviderFacts, String> {
    provider
        .map(|provider| response_usage_provider_facts(provider, app_type))
        .ok_or_else(|| usage_selected_provider_missing_log_message(tag, phase))
}

pub(crate) fn usage_logging_enabled_from_state(state: &ProxyState) -> bool {
    usage_logging_enabled_from_config_flag(
        state
            .config
            .try_read()
            .ok()
            .map(|config| config.enable_logging),
    )
}

#[allow(dead_code)]
pub(crate) async fn record_usage_with_proxy_services(
    services: &(dyn ProxyServices + Send + Sync),
    record: UsageRecord,
) {
    record_usage_with_proxy_services_context(
        services,
        record,
        UsageRecordFailureLogContext::UsageRecord,
    )
    .await
}

/// Test-only bridge for legacy response-pipeline pricing regression coverage.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) async fn record_success_usage_for_test(
    services: &(dyn ProxyServices + Send + Sync),
    provider_id: &str,
    provider_kind: Option<ProviderKind>,
    app_type: &str,
    model: &str,
    request_model: &str,
    outbound_model: &str,
    usage: TokenUsage,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    is_streaming: bool,
    status_code: u16,
    session_id: Option<String>,
) {
    let record = crate::proxy_core::api::usage::success_usage_record_with_request_id_fallback(
        provider_id,
        provider_kind,
        AppKind::from(app_type),
        model,
        request_model,
        outbound_model,
        usage,
        latency_ms,
        first_token_ms,
        is_streaming,
        status_code,
        session_id,
        || uuid::Uuid::new_v4().to_string(),
    );

    record_usage_with_proxy_services(services, record).await;
}

pub(crate) fn spawn_usage_record_with_proxy_services<S>(services: Arc<S>, record: UsageRecord)
where
    S: ProxyServices + Send + Sync + 'static,
{
    spawn_usage_record_with_proxy_services_context(
        services,
        record,
        UsageRecordFailureLogContext::UsageRecord,
    );
}

pub(crate) fn spawn_usage_record_with_proxy_services_context<S>(
    services: Arc<S>,
    record: UsageRecord,
    failure_context: UsageRecordFailureLogContext,
) where
    S: ProxyServices + Send + Sync + 'static,
{
    tokio::spawn(async move {
        record_usage_with_proxy_services_context(services.as_ref(), record, failure_context).await;
    });
}

pub(crate) async fn record_usage_with_proxy_services_context(
    services: &(dyn ProxyServices + Send + Sync),
    record: UsageRecord,
    failure_context: UsageRecordFailureLogContext,
) {
    log::debug!("{}", usage_record_debug_log_message(&record));

    if let Err(error) = services.usage_sink().record_usage(record).await {
        log::warn!(
            "{}",
            usage_record_failure_warning_message(failure_context, error)
        );
    }
}

pub(crate) fn record_forward_error_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    is_streaming: bool,
    error: &ProxyError,
) {
    record_forward_error_usage_from_context(ForwardErrorUsageRecordContext {
        services: state.proxy_core_services.clone(),
        provider: ctx.provider_for_usage(),
        fallback_provider_id: &ctx.fallback_provider_id(),
        app_type: ctx.app_type_str,
        request_model: &ctx.request_model,
        outbound_model: ctx.outbound_model.as_deref(),
        route_context: ctx.usage_route_context.as_ref(),
        status_code: proxy_error_status_code(error),
        error_message: proxy_error_display_message(error),
        latency_ms: ctx.latency_ms(),
        is_streaming,
        session_id: &ctx.session_id,
    });
}

pub(crate) fn record_forward_core_error_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    is_streaming: bool,
    error: ProxyCoreError,
) -> ProxyError {
    let error = proxy_core_error_to_proxy_error(error);
    record_forward_error_usage(state, ctx, is_streaming, &error);
    error
}

pub(crate) struct ForwardErrorUsageContext<'a> {
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) fallback_provider_id: &'a str,
    pub(crate) app_type: &'a str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) status_code: u16,
    pub(crate) error_message: String,
    pub(crate) latency_ms: u64,
    pub(crate) is_streaming: bool,
    pub(crate) session_id: &'a str,
}

pub(crate) struct ForwardErrorUsageRecordContext<'a, S> {
    pub(crate) services: Arc<S>,
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) fallback_provider_id: &'a str,
    pub(crate) app_type: &'a str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) status_code: u16,
    pub(crate) error_message: String,
    pub(crate) latency_ms: u64,
    pub(crate) is_streaming: bool,
    pub(crate) session_id: &'a str,
}

pub(crate) fn record_forward_error_usage_from_context<S>(
    context: ForwardErrorUsageRecordContext<'_, S>,
) where
    S: ProxyServices + Send + Sync + 'static,
{
    let record = forward_error_usage_record_from_response_context(
        ForwardErrorUsageContext {
            provider: context.provider,
            fallback_provider_id: context.fallback_provider_id,
            app_type: context.app_type,
            request_model: context.request_model,
            outbound_model: context.outbound_model,
            route_context: context.route_context,
            status_code: context.status_code,
            error_message: context.error_message,
            latency_ms: context.latency_ms,
            is_streaming: context.is_streaming,
            session_id: context.session_id,
        },
        || uuid::Uuid::new_v4().to_string(),
    );

    spawn_usage_record_with_proxy_services_context(
        context.services,
        record,
        UsageRecordFailureLogContext::ForwardError,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn error_usage_record_from_provider_facts_with_request_id_fallback(
    provider_facts: &ResponseUsageProviderFacts,
    request_model: &str,
    outbound_model: Option<&str>,
    status_code: u16,
    error_message: String,
    latency_ms: u64,
    is_streaming: bool,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> UsageRecord {
    error_usage_record_with_request_id_fallback(
        &provider_facts.provider_id,
        provider_facts.provider_kind.clone(),
        provider_facts.app.clone(),
        request_model,
        outbound_model,
        status_code,
        error_message,
        latency_ms,
        is_streaming,
        session_id,
        request_id_fallback,
    )
}

pub(crate) fn forward_error_usage_record_from_response_context(
    context: ForwardErrorUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> UsageRecord {
    let provider_facts = context
        .provider
        .map(|provider| response_usage_provider_facts(provider, context.app_type))
        .unwrap_or_else(|| {
            fallback_response_usage_provider_facts(
                context.fallback_provider_id.to_string(),
                context.app_type,
            )
        });
    let record = error_usage_record_from_provider_facts_with_request_id_fallback(
        &provider_facts,
        context.request_model,
        context.outbound_model,
        context.status_code,
        context.error_message,
        context.latency_ms,
        context.is_streaming,
        Some(context.session_id.to_string()),
        request_id_fallback,
    );
    usage_record_with_route_context(record, context.route_context)
}

pub(crate) struct StreamingResponseUsageContext<'a> {
    pub(crate) events: &'a [Value],
    pub(crate) stream_parser: fn(&[Value]) -> Option<TokenUsage>,
    pub(crate) model_extractor: fn(&[Value], &str) -> String,
    pub(crate) provider_facts: &'a ResponseUsageProviderFacts,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) first_token_ms: Option<u64>,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn streaming_response_usage_record_from_provider_facts(
    events: &[Value],
    stream_parser: fn(&[Value]) -> Option<TokenUsage>,
    model_extractor: fn(&[Value], &str) -> String,
    provider_facts: &ResponseUsageProviderFacts,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> StreamingResponseUsageRecord {
    streaming_response_usage_record_with_optional_outbound_model(
        events,
        stream_parser,
        model_extractor,
        &provider_facts.provider_id,
        provider_facts.provider_kind.clone(),
        provider_facts.app.clone(),
        request_model,
        outbound_model,
        latency_ms,
        first_token_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

pub(crate) fn streaming_response_usage_record_from_response_context(
    context: StreamingResponseUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> StreamingResponseUsageRecord {
    let mut output = streaming_response_usage_record_from_provider_facts(
        context.events,
        context.stream_parser,
        context.model_extractor,
        context.provider_facts,
        context.request_model,
        context.outbound_model,
        context.latency_ms,
        context.first_token_ms,
        context.status_code,
        Some(context.session_id.to_string()),
        request_id_fallback,
    );
    output.record = usage_record_with_route_context(output.record, context.route_context);
    output
}

pub(crate) struct NonStreamingResponseUsageContext<'a> {
    pub(crate) body: &'a [u8],
    pub(crate) response_parser: fn(&Value) -> Option<TokenUsage>,
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) app_type: &'a str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn non_streaming_response_usage_record_from_provider_body_with_request_id_fallback(
    body: &[u8],
    response_parser: fn(&Value) -> Option<TokenUsage>,
    provider: &Provider,
    app_type: &str,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> NonStreamingResponseUsageRecord {
    let provider_facts = response_usage_provider_facts(provider, app_type);
    non_streaming_response_usage_record_from_body_with_request_id_fallback(
        body,
        response_parser,
        &provider_facts.provider_id,
        provider_facts.provider_kind,
        provider_facts.app,
        request_model,
        outbound_model,
        latency_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

pub(crate) fn non_streaming_response_usage_record_from_response_context(
    context: NonStreamingResponseUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> Result<NonStreamingResponseUsageRecord, String> {
    let provider = context
        .provider
        .ok_or_else(|| selected_provider_not_applied_message(context.app_type))?;
    let mut output =
        non_streaming_response_usage_record_from_provider_body_with_request_id_fallback(
            context.body,
            context.response_parser,
            provider,
            context.app_type,
            context.request_model,
            context.outbound_model,
            context.latency_ms,
            context.status_code,
            Some(context.session_id.to_string()),
            request_id_fallback,
        );
    output.record = usage_record_with_route_context(output.record, context.route_context);
    Ok(output)
}

pub(crate) struct NonStreamingUsageRecordContext<'a, S> {
    pub(crate) usage_logging_enabled: bool,
    pub(crate) services: Arc<S>,
    pub(crate) body: &'a [u8],
    pub(crate) parser_config: &'a UsageParserConfig,
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) app_type: &'a str,
    pub(crate) tag: &'static str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

pub(crate) fn record_non_streaming_response_usage_from_context<S>(
    context: NonStreamingUsageRecordContext<'_, S>,
) -> Result<(), String>
where
    S: ProxyServices + Send + Sync + 'static,
{
    if !context.usage_logging_enabled {
        log::debug!(
            "[{}] usage logging 已关闭，跳过非流式 usage 解析",
            context.tag
        );
        return Ok(());
    }

    let output = non_streaming_response_usage_record_from_response_context(
        NonStreamingResponseUsageContext {
            body: context.body,
            response_parser: context.parser_config.response_parser,
            provider: context.provider,
            app_type: context.app_type,
            request_model: context.request_model,
            outbound_model: context.outbound_model,
            route_context: context.route_context,
            latency_ms: context.latency_ms,
            status_code: context.status_code,
            session_id: context.session_id,
        },
        || uuid::Uuid::new_v4().to_string(),
    )?;

    if let Some(event) = output.log_event(context.body.len()) {
        log::debug!(
            "{}",
            event.message(context.tag, context.parser_config.app_type_str)
        );
    }

    spawn_usage_record_with_proxy_services(context.services, output.record);
    Ok(())
}

pub(crate) struct TransformedResponseUsageContext<'a> {
    pub(crate) body: &'a Value,
    pub(crate) format: TransformedResponseUsageFormat,
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) tag: &'a str,
    pub(crate) app_type: &'a str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

pub(crate) struct TransformedResponseUsageRecordContext<'a, S> {
    pub(crate) usage_logging_enabled: bool,
    pub(crate) services: Arc<S>,
    pub(crate) body: &'a Value,
    pub(crate) format: TransformedResponseUsageFormat,
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) tag: &'a str,
    pub(crate) app_type: &'a str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

pub(crate) fn record_transformed_response_usage_from_context<S>(
    context: TransformedResponseUsageRecordContext<'_, S>,
) where
    S: ProxyServices + Send + Sync + 'static,
{
    if !context.usage_logging_enabled {
        return;
    }

    let record = match transformed_response_usage_record_from_response_context(
        TransformedResponseUsageContext {
            body: context.body,
            format: context.format,
            provider: context.provider,
            tag: context.tag,
            app_type: context.app_type,
            request_model: context.request_model,
            outbound_model: context.outbound_model,
            route_context: context.route_context,
            latency_ms: context.latency_ms,
            status_code: context.status_code,
            session_id: context.session_id,
        },
        || uuid::Uuid::new_v4().to_string(),
    ) {
        Ok(Some(record)) => record,
        Ok(None) => return,
        Err(message) => {
            log::warn!("{message}");
            return;
        }
    };

    spawn_usage_record_with_proxy_services_context(
        context.services,
        record,
        UsageRecordFailureLogContext::UsageRecord,
    );
}

pub(crate) struct TransformedStreamingResponseUsageContext<'a> {
    pub(crate) events: &'a [Value],
    pub(crate) format: TransformedResponseUsageFormat,
    pub(crate) provider_facts: &'a ResponseUsageProviderFacts,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) latency_ms: u64,
    pub(crate) first_token_ms: Option<u64>,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn transformed_response_usage_record_from_provider_facts_with_request_id_fallback(
    body: &Value,
    format: TransformedResponseUsageFormat,
    provider_facts: &ResponseUsageProviderFacts,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> Option<UsageRecord> {
    transformed_response_usage_record_with_request_id_fallback(
        body,
        format,
        &provider_facts.provider_id,
        provider_facts.provider_kind.clone(),
        provider_facts.app.clone(),
        request_model,
        outbound_model,
        latency_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn transformed_streaming_response_usage_record_from_provider_facts_with_request_id_fallback(
    events: &[Value],
    format: TransformedResponseUsageFormat,
    provider_facts: &ResponseUsageProviderFacts,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    status_code: u16,
    session_id: Option<String>,
    request_id_fallback: impl FnOnce() -> String,
) -> Option<UsageRecord> {
    transformed_streaming_response_usage_record_with_request_id_fallback(
        events,
        format,
        &provider_facts.provider_id,
        provider_facts.provider_kind.clone(),
        provider_facts.app.clone(),
        request_model,
        outbound_model,
        latency_ms,
        first_token_ms,
        status_code,
        session_id,
        request_id_fallback,
    )
}

pub(crate) fn transformed_response_usage_record_from_response_context(
    context: TransformedResponseUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> Result<Option<UsageRecord>, String> {
    let provider_facts = response_usage_provider_facts_from_optional(
        context.provider,
        context.app_type,
        context.tag,
        UsageSelectedProviderMissingPhase::TransformedResponse,
    )?;
    Ok(
        transformed_response_usage_record_from_provider_facts_with_request_id_fallback(
            context.body,
            context.format,
            &provider_facts,
            context.request_model,
            context.outbound_model,
            context.latency_ms,
            context.status_code,
            Some(context.session_id.to_string()),
            request_id_fallback,
        )
        .map(|record| usage_record_with_route_context(record, context.route_context)),
    )
}

pub(crate) fn transformed_streaming_response_usage_record_from_response_context(
    context: TransformedStreamingResponseUsageContext<'_>,
    request_id_fallback: impl FnOnce() -> String,
) -> Option<UsageRecord> {
    transformed_streaming_response_usage_record_from_provider_facts_with_request_id_fallback(
        context.events,
        context.format,
        context.provider_facts,
        context.request_model,
        context.outbound_model,
        context.latency_ms,
        context.first_token_ms,
        context.status_code,
        Some(context.session_id.to_string()),
        request_id_fallback,
    )
    .map(|record| usage_record_with_route_context(record, context.route_context))
}
