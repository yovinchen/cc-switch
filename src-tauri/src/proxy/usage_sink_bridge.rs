use crate::proxy::{
    error::ProxyError,
    error_mapper::{get_error_message, map_proxy_error_to_status},
    handler_context::RequestContext,
    response_processor::SseUsageCollector,
    server::ProxyState,
};
use crate::proxy_core_adapter::{
    forward_error_usage_record_from_response_context, record_usage_with_proxy_services_context,
    response_usage_provider_facts_from_optional,
    transformed_response_usage_record_from_response_context,
    transformed_streaming_response_usage_record_from_response_context,
    usage_logging_enabled_from_config_flag, ForwardErrorUsageContext, ProxyServices,
    StreamUsageEventFilter, TransformedResponseUsageContext, TransformedResponseUsageFormat,
    TransformedStreamingResponseUsageContext, UsageRecord, UsageRecordFailureLogContext,
    UsageSelectedProviderMissingPhase,
};
#[cfg(test)]
use crate::proxy_core_adapter::{
    success_usage_record_from_app_type_with_request_id_fallback, ProviderKind, TokenUsage,
};
use serde_json::Value;

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn success_usage_record(
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
) -> UsageRecord {
    success_usage_record_from_app_type_with_request_id_fallback(
        provider_id,
        provider_kind,
        app_type,
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
    )
}

pub(crate) fn record_forward_error_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    is_streaming: bool,
    error: &ProxyError,
) {
    let record = forward_error_usage_record_from_response_context(
        ForwardErrorUsageContext {
            provider: ctx.provider_for_usage(),
            fallback_provider_id: &ctx.fallback_provider_id(),
            app_type: ctx.app_type_str,
            request_model: &ctx.request_model,
            outbound_model: ctx.outbound_model.as_deref(),
            route_context: ctx.usage_route_context.as_ref(),
            status_code: map_proxy_error_to_status(error),
            error_message: get_error_message(error),
            latency_ms: ctx.latency_ms(),
            is_streaming,
            session_id: &ctx.session_id,
        },
        || uuid::Uuid::new_v4().to_string(),
    );

    let services = state.proxy_core_services.clone();
    spawn_usage_record(services, record, UsageRecordFailureLogContext::ForwardError);
}

pub(crate) fn record_transformed_response_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    body: &Value,
    format: TransformedResponseUsageFormat,
    status_code: u16,
) {
    if !usage_logging_enabled(state) {
        return;
    }

    let record = match transformed_response_usage_record_from_response_context(
        TransformedResponseUsageContext {
            body,
            format,
            provider: ctx.provider_for_usage(),
            tag: ctx.tag,
            app_type: ctx.app_type_str,
            request_model: &ctx.request_model,
            outbound_model: ctx.outbound_model.as_deref(),
            route_context: ctx.usage_route_context.as_ref(),
            latency_ms: ctx.latency_ms(),
            status_code,
            session_id: &ctx.session_id,
        },
        || uuid::Uuid::new_v4().to_string(),
    ) {
        Ok(Some(record)) => record,
        Ok(None) => return,
        Err(message) => {
            log::warn!("{}", message);
            return;
        }
    };

    let services = state.proxy_core_services.clone();
    spawn_usage_record(services, record, UsageRecordFailureLogContext::UsageRecord);
}

pub(crate) fn transformed_streaming_usage_collector(
    state: &ProxyState,
    ctx: &RequestContext,
    status_code: u16,
    usage_format: TransformedResponseUsageFormat,
    stream_event_filter: StreamUsageEventFilter,
) -> Option<SseUsageCollector> {
    if !usage_logging_enabled(state) {
        return None;
    }

    let provider_facts = match response_usage_provider_facts_from_optional(
        ctx.provider_for_usage(),
        ctx.app_type_str,
        ctx.tag,
        UsageSelectedProviderMissingPhase::TransformedStreaming,
    ) {
        Ok(provider_facts) => provider_facts,
        Err(message) => {
            log::warn!("{}", message);
            return None;
        }
    };

    let services = state.proxy_core_services.clone();
    let request_model = ctx.request_model.clone();
    let outbound_model = ctx.outbound_model.clone();
    let start_time = ctx.start_time;
    let session_id = ctx.session_id.clone();
    let usage_route_context = ctx.usage_route_context.clone();

    Some(SseUsageCollector::new(
        start_time,
        Some(stream_event_filter),
        move |events, first_token_ms| {
            let latency_ms = start_time.elapsed().as_millis() as u64;
            let Some(record) = transformed_streaming_response_usage_record_from_response_context(
                TransformedStreamingResponseUsageContext {
                    events: &events,
                    format: usage_format,
                    provider_facts: &provider_facts,
                    request_model: &request_model,
                    outbound_model: outbound_model.as_deref(),
                    route_context: usage_route_context.as_ref(),
                    latency_ms,
                    first_token_ms,
                    status_code,
                    session_id: &session_id,
                },
                || uuid::Uuid::new_v4().to_string(),
            ) else {
                log::debug!("{}", usage_format.missing_streaming_usage_log_message());
                return;
            };

            let services = services.clone();
            spawn_usage_record(services, record, UsageRecordFailureLogContext::UsageRecord);
        },
    ))
}

fn usage_logging_enabled(state: &ProxyState) -> bool {
    usage_logging_enabled_from_config_flag(
        state
            .config
            .try_read()
            .ok()
            .map(|config| config.enable_logging),
    )
}

fn spawn_usage_record<S>(
    services: std::sync::Arc<S>,
    record: UsageRecord,
    failure_context: UsageRecordFailureLogContext,
) where
    S: ProxyServices + Send + Sync + 'static,
{
    tokio::spawn(async move {
        record_usage_with_proxy_services_context(services.as_ref(), record, failure_context).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core_adapter::ProviderKind;

    #[test]
    fn success_record_preserves_models_tokens_and_provider_kind() {
        let usage = TokenUsage {
            input_tokens: 3,
            output_tokens: 5,
            cache_read_tokens: 7,
            cache_creation_tokens: 11,
            model: None,
            message_id: Some("msg-1".to_string()),
        };

        let record = success_usage_record(
            "provider-a",
            Some(ProviderKind::GitHubCopilot),
            "claude",
            "response-model",
            "request-model",
            "upstream-model",
            usage,
            123,
            Some(45),
            true,
            200,
            Some("session-1".to_string()),
        );

        assert_eq!(record.provider_id, "provider-a");
        assert_eq!(record.provider_kind, Some(ProviderKind::GitHubCopilot));
        assert_eq!(record.message_id.as_deref(), Some("msg-1"));
        assert_eq!(record.request_model, "request-model");
        assert_eq!(record.outbound_model, "upstream-model");
        assert_eq!(record.response_model.as_deref(), Some("response-model"));
        assert_eq!(record.tokens.input_tokens, 3);
        assert_eq!(record.first_token_ms, Some(45));
        assert!(record.is_streaming);
    }
}
