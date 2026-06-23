use crate::proxy::{
    error::ProxyError,
    error_mapper::{get_error_message, map_proxy_error_to_status},
    handler_context::RequestContext,
    server::ProxyState,
};
use crate::proxy_core_adapter::{
    record_forward_error_usage_from_context, record_transformed_response_usage_from_context,
    transformed_streaming_usage_collector_from_context, usage_logging_enabled_from_proxy_config,
    ForwardErrorUsageRecordContext, SseUsageCollector, StreamUsageEventFilter,
    TransformedResponseUsageFormat, TransformedResponseUsageRecordContext,
    TransformedStreamingUsageCollectorContext,
};
#[cfg(test)]
use crate::proxy_core_adapter::{
    success_usage_record_from_app_type_with_request_id_fallback, ProviderKind, TokenUsage,
    UsageRecord,
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
    record_forward_error_usage_from_context(ForwardErrorUsageRecordContext {
        services: state.proxy_core_services.clone(),
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
    });
}

pub(crate) fn record_transformed_response_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    body: &Value,
    format: TransformedResponseUsageFormat,
    status_code: u16,
) {
    record_transformed_response_usage_from_context(TransformedResponseUsageRecordContext {
        usage_logging_enabled: usage_logging_enabled_from_proxy_config(state.config.as_ref()),
        services: state.proxy_core_services.clone(),
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
    });
}

pub(crate) fn transformed_streaming_usage_collector(
    state: &ProxyState,
    ctx: &RequestContext,
    status_code: u16,
    usage_format: TransformedResponseUsageFormat,
    stream_event_filter: StreamUsageEventFilter,
) -> Option<SseUsageCollector> {
    transformed_streaming_usage_collector_from_context(
        TransformedStreamingUsageCollectorContext {
            usage_logging_enabled: usage_logging_enabled_from_proxy_config(state.config.as_ref()),
            services: state.proxy_core_services.clone(),
            provider: ctx.provider_for_usage(),
            app_type: ctx.app_type_str,
            tag: ctx.tag,
            request_model: &ctx.request_model,
            outbound_model: ctx.outbound_model.as_deref(),
            route_context: ctx.usage_route_context.as_ref(),
            start_time: ctx.start_time,
            status_code,
            session_id: &ctx.session_id,
            usage_format,
            stream_event_filter,
        },
    )
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
