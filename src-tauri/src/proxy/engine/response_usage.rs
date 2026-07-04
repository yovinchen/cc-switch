use super::context::RequestContext;
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy::error_mapper::{
    proxy_core_error_to_proxy_error, proxy_error_display_message, proxy_error_status_code,
};
use crate::proxy::host::cc_switch::provider_projection::provider_kind_from_provider;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy_core::api::domain::{AppKind, ProviderKind};
use crate::proxy_core::api::errors::ProxyCoreError;
use crate::proxy_core::api::ports::ProxyServices;
use crate::proxy_core::api::usage::{
    error_usage_record_with_request_id_fallback, usage_logging_enabled_from_config_flag,
    usage_record_debug_log_message, usage_record_failure_warning_message,
    usage_record_with_route_context, usage_selected_provider_missing_log_message, UsageRecord,
    UsageRecordFailureLogContext, UsageRouteContext, UsageSelectedProviderMissingPhase,
};
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
