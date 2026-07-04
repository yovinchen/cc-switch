//! Streaming response logging and usage collection runtime.

use super::context::RequestContext;
use super::response_usage::{
    response_usage_provider_facts_from_optional, spawn_usage_record_with_proxy_services,
    spawn_usage_record_with_proxy_services_context,
    streaming_response_usage_record_from_response_context,
    transformed_streaming_response_usage_record_from_response_context,
    usage_logging_enabled_from_state, StreamingResponseUsageContext,
    TransformedStreamingResponseUsageContext,
};
use crate::provider::Provider;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy_core::api::config::StreamingTimeoutConfig;
use crate::proxy_core::api::ports::ProxyServices;
use crate::proxy_core::api::transforms::{
    claude_stream_usage_event_filter, codex_stream_usage_event_filter, SsePassthroughStreamState,
    SseUsageAccumulator,
};
use crate::proxy_core::api::usage::{
    StreamUsageEventFilter, TransformedResponseUsageFormat, UsageParserConfig,
    UsageRecordFailureLogContext, UsageRouteContext, UsageSelectedProviderMissingPhase,
};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) struct StreamingUsageCollectorContext<'a, S> {
    pub(crate) usage_logging_enabled: bool,
    pub(crate) services: Arc<S>,
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) app_type: &'a str,
    pub(crate) tag: &'static str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) start_time: std::time::Instant,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
    pub(crate) parser_config: &'a UsageParserConfig,
}

pub(crate) fn streaming_usage_collector_from_context<S>(
    context: StreamingUsageCollectorContext<'_, S>,
) -> Option<SseUsageCollector>
where
    S: ProxyServices + Send + Sync + 'static,
{
    if !context.usage_logging_enabled {
        return None;
    }

    // Use the request app_type instead of parser_config.app_type_str:
    // Claude Desktop streaming passthrough reuses CLAUDE_PARSER_CONFIG, but usage
    // must stay under claude-desktop so provider pricing overrides resolve.
    let provider_facts = match response_usage_provider_facts_from_optional(
        context.provider,
        context.app_type,
        context.tag,
        UsageSelectedProviderMissingPhase::StreamingPassthrough,
    ) {
        Ok(provider_facts) => provider_facts,
        Err(message) => {
            log::warn!("{message}");
            return None;
        }
    };

    let services = context.services;
    let request_model = context.request_model.to_string();
    let outbound_model = context.outbound_model.map(str::to_string);
    let route_context = context.route_context.cloned();
    let tag = context.tag;
    let start_time = context.start_time;
    let stream_parser = context.parser_config.stream_parser;
    let model_extractor = context.parser_config.model_extractor;
    let stream_event_filter = context.parser_config.stream_event_filter;
    let session_id = context.session_id.to_string();
    let status_code = context.status_code;

    Some(SseUsageCollector::new(
        start_time,
        stream_event_filter,
        move |events, first_token_ms| {
            let latency_ms = start_time.elapsed().as_millis() as u64;
            let output = streaming_response_usage_record_from_response_context(
                StreamingResponseUsageContext {
                    events: &events,
                    stream_parser,
                    model_extractor,
                    provider_facts: &provider_facts,
                    request_model: &request_model,
                    outbound_model: outbound_model.as_deref(),
                    route_context: route_context.as_ref(),
                    latency_ms,
                    first_token_ms,
                    status_code,
                    session_id: &session_id,
                },
                || uuid::Uuid::new_v4().to_string(),
            );

            if let Some(message) = output.missing_usage_log_message(tag) {
                log::debug!("{message}");
            }

            spawn_usage_record_with_proxy_services(services.clone(), output.record);
        },
    ))
}

pub(crate) struct TransformedStreamingUsageCollectorContext<'a, S> {
    pub(crate) usage_logging_enabled: bool,
    pub(crate) services: Arc<S>,
    pub(crate) provider: Option<&'a Provider>,
    pub(crate) app_type: &'a str,
    pub(crate) tag: &'static str,
    pub(crate) request_model: &'a str,
    pub(crate) outbound_model: Option<&'a str>,
    pub(crate) route_context: Option<&'a UsageRouteContext>,
    pub(crate) start_time: std::time::Instant,
    pub(crate) status_code: u16,
    pub(crate) session_id: &'a str,
    pub(crate) usage_format: TransformedResponseUsageFormat,
    pub(crate) stream_event_filter: StreamUsageEventFilter,
}

pub(crate) fn transformed_streaming_usage_collector_from_context<S>(
    context: TransformedStreamingUsageCollectorContext<'_, S>,
) -> Option<SseUsageCollector>
where
    S: ProxyServices + Send + Sync + 'static,
{
    if !context.usage_logging_enabled {
        return None;
    }

    let provider_facts = match response_usage_provider_facts_from_optional(
        context.provider,
        context.app_type,
        context.tag,
        UsageSelectedProviderMissingPhase::TransformedStreaming,
    ) {
        Ok(provider_facts) => provider_facts,
        Err(message) => {
            log::warn!("{message}");
            return None;
        }
    };

    let services = context.services;
    let request_model = context.request_model.to_string();
    let outbound_model = context.outbound_model.map(str::to_string);
    let route_context = context.route_context.cloned();
    let start_time = context.start_time;
    let status_code = context.status_code;
    let session_id = context.session_id.to_string();
    let usage_format = context.usage_format;

    Some(SseUsageCollector::new(
        start_time,
        Some(context.stream_event_filter),
        move |events, first_token_ms| {
            let latency_ms = start_time.elapsed().as_millis() as u64;
            let Some(record) = transformed_streaming_response_usage_record_from_response_context(
                TransformedStreamingResponseUsageContext {
                    events: &events,
                    format: usage_format,
                    provider_facts: &provider_facts,
                    request_model: &request_model,
                    outbound_model: outbound_model.as_deref(),
                    route_context: route_context.as_ref(),
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

            spawn_usage_record_with_proxy_services_context(
                services.clone(),
                record,
                UsageRecordFailureLogContext::UsageRecord,
            );
        },
    ))
}

#[derive(Clone)]
pub(crate) struct SseUsageCollector {
    inner: Arc<SseUsageCollectorInner>,
}

struct SseUsageCollectorInner {
    accumulator: Mutex<SseUsageAccumulator>,
    on_complete: Arc<dyn Fn(Vec<Value>, Option<u64>) + Send + Sync + 'static>,
    should_collect: Option<StreamUsageEventFilter>,
}

impl SseUsageCollector {
    pub(crate) fn new(
        start_time: std::time::Instant,
        should_collect: Option<StreamUsageEventFilter>,
        callback: impl Fn(Vec<Value>, Option<u64>) + Send + Sync + 'static,
    ) -> Self {
        let on_complete = Arc::new(callback);
        Self {
            inner: Arc::new(SseUsageCollectorInner {
                accumulator: Mutex::new(SseUsageAccumulator::new(start_time)),
                on_complete,
                should_collect,
            }),
        }
    }

    pub(crate) fn should_collect(&self, data: &str) -> bool {
        self.inner
            .should_collect
            .map(|filter| filter(data))
            .unwrap_or(true)
    }

    pub(crate) async fn push(&self, event: Value) {
        let mut accumulator = self.inner.accumulator.lock().await;
        accumulator.push(event);
    }

    pub(crate) async fn finish(&self) {
        let snapshot = {
            let mut accumulator = self.inner.accumulator.lock().await;
            accumulator.finish()
        };
        if let Some(snapshot) = snapshot {
            (self.inner.on_complete)(snapshot.events, snapshot.first_token_ms);
        }
    }
}

struct SseUsageFinishGuard {
    collector: Option<SseUsageCollector>,
}

impl SseUsageFinishGuard {
    fn new(collector: SseUsageCollector) -> Self {
        Self {
            collector: Some(collector),
        }
    }

    fn disarm(&mut self) {
        self.collector = None;
    }
}

impl Drop for SseUsageFinishGuard {
    fn drop(&mut self) {
        if let Some(collector) = self.collector.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    collector.finish().await;
                });
            } else {
                log::warn!("SSE 用量收尾保护触发时 Tokio runtime 不可用，跳过异步 finish");
            }
        }
    }
}

pub(crate) fn create_logged_passthrough_stream<G>(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    tag: &'static str,
    usage_collector: Option<SseUsageCollector>,
    timeout_config: StreamingTimeoutConfig,
    connection_guard: Option<G>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send
where
    G: Send + 'static,
{
    async_stream::stream! {
        let _conn_guard = connection_guard;
        let mut passthrough_state = SsePassthroughStreamState::new();
        let mut collector = usage_collector;
        let mut finish_guard = collector.clone().map(SseUsageFinishGuard::new);
        let inspect_sse_events =
            collector.is_some() || log::log_enabled!(log::Level::Debug);

        tokio::pin!(stream);

        loop {
            let timeout_phase = passthrough_state.timeout_phase();
            let timeout_duration = timeout_config.duration_for_phase(timeout_phase);

            let chunk_result = match timeout_duration {
                Some(duration) => {
                    match tokio::time::timeout(duration, stream.next()).await {
                        Ok(Some(chunk)) => Some(chunk),
                        Ok(None) => None,
                        Err(_) => {
                            log::error!(
                                "[{tag}] {} ({}秒)",
                                timeout_phase.timeout_message(),
                                duration.as_secs()
                            );
                            yield Err(std::io::Error::other(timeout_phase.timeout_message()));
                            break;
                        }
                    }
                }
                None => stream.next().await,
            };

            match chunk_result {
                Some(Ok(bytes)) => {
                    let inspection = passthrough_state.inspect_chunk(
                        &bytes,
                        tag,
                        inspect_sse_events,
                        |data| {
                            collector
                                .as_ref()
                                .map(|collector| collector.should_collect(data))
                                .unwrap_or(false)
                        },
                    );
                    if let Some(message) = inspection.first_chunk_log_message {
                        log::debug!("{message}");
                    }
                    for event in inspection.event_actions {
                        if let (Some(collector), Some(json_value)) =
                            (&collector, event.usage_event)
                        {
                            collector.push(json_value).await;
                        }
                        log::debug!("{}", event.log_message);
                    }

                    yield Ok(bytes);
                }
                Some(Err(e)) => {
                    log::error!("[{tag}] 流错误: {e}");
                    yield Err(std::io::Error::other(e.to_string()));
                    break;
                }
                None => {
                    break;
                }
            }
        }

        if let Some(c) = collector.take() {
            c.finish().await;
        }
        if let Some(guard) = &mut finish_guard {
            guard.disarm();
        }
    }
}

pub(crate) fn passthrough_streaming_usage_collector(
    state: &ProxyState,
    ctx: &RequestContext,
    status_code: u16,
    parser_config: &UsageParserConfig,
) -> Option<SseUsageCollector> {
    streaming_usage_collector_from_context(StreamingUsageCollectorContext {
        usage_logging_enabled: usage_logging_enabled_from_state(state),
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
        parser_config,
    })
}

pub(crate) fn create_passthrough_logged_stream<G>(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    state: &ProxyState,
    ctx: &RequestContext,
    status_code: u16,
    parser_config: &UsageParserConfig,
    connection_guard: Option<G>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    G: Send + 'static,
{
    let usage_collector =
        passthrough_streaming_usage_collector(state, ctx, status_code, parser_config);
    create_logged_passthrough_stream(
        stream,
        ctx.tag,
        usage_collector,
        ctx.streaming_timeout_config(),
        connection_guard,
    )
}

pub(crate) fn transformed_streaming_usage_collector(
    state: &ProxyState,
    ctx: &RequestContext,
    status_code: u16,
    usage_format: TransformedResponseUsageFormat,
    stream_event_filter: StreamUsageEventFilter,
) -> Option<SseUsageCollector> {
    transformed_streaming_usage_collector_from_context(TransformedStreamingUsageCollectorContext {
        usage_logging_enabled: usage_logging_enabled_from_state(state),
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
    })
}

pub(crate) fn claude_transformed_streaming_usage_collector(
    state: &ProxyState,
    ctx: &RequestContext,
    status_code: u16,
) -> Option<SseUsageCollector> {
    transformed_streaming_usage_collector(
        state,
        ctx,
        status_code,
        TransformedResponseUsageFormat::Claude,
        claude_stream_usage_event_filter,
    )
}

pub(crate) fn codex_auto_transformed_streaming_usage_collector(
    state: &ProxyState,
    ctx: &RequestContext,
    status_code: u16,
) -> Option<SseUsageCollector> {
    transformed_streaming_usage_collector(
        state,
        ctx,
        status_code,
        TransformedResponseUsageFormat::CodexAuto,
        codex_stream_usage_event_filter,
    )
}

pub(crate) fn create_claude_transformed_logged_stream<G>(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    state: &ProxyState,
    ctx: &RequestContext,
    status_code: u16,
    connection_guard: Option<G>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    G: Send + 'static,
{
    let usage_collector = claude_transformed_streaming_usage_collector(state, ctx, status_code);
    create_logged_passthrough_stream(
        stream,
        "Claude/OpenRouter",
        usage_collector,
        ctx.streaming_timeout_config(),
        connection_guard,
    )
}

pub(crate) fn create_codex_auto_transformed_logged_stream<G>(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    state: &ProxyState,
    ctx: &RequestContext,
    status_code: u16,
    connection_guard: Option<G>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    G: Send + 'static,
{
    let usage_collector = codex_auto_transformed_streaming_usage_collector(state, ctx, status_code);
    create_logged_passthrough_stream(
        stream,
        ctx.tag,
        usage_collector,
        ctx.streaming_timeout_config(),
        connection_guard,
    )
}
