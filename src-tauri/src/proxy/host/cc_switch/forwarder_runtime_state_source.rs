use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::provider::Provider;
use crate::proxy::engine::forward_pipeline::{
    ForwarderAttemptFailedInput, ForwarderAttemptStartedInput, ForwarderFailoverSwitchTarget,
    ForwarderFailureDecision, ForwarderProviderFailureInput,
    ForwarderProviderRectifierRetryFailureInput, ForwarderRectifierRetryFailureDecision,
    ForwarderRectifierRetryFailureLogInput, ForwarderRectifierRetrySuccessLogInput,
    ForwarderRetryableFailureLogInput, ForwarderRuntimeStateSource, ForwarderRuntimeStateSourceRef,
    ForwarderSuccessStatusInput, ForwarderSuccessfulAttemptInput, ForwarderTerminalFailureLogInput,
};
use crate::proxy::error::ProxyError;
use crate::proxy::error_mapper::forward_failure_kind_from_proxy_error;
use crate::proxy::events::ProxyEventBus;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy_core::api::events::{
    attempt_event, request_started_event, route_selected_event, AttemptEventChannel,
    AttemptEventPayloadInput, AttemptEventPhase,
};
use crate::proxy_core::api::ports::{
    current_route_target_from_input, record_active_connection_acquired_status,
    record_active_connection_released_status, record_forward_current_provider_status,
    record_forward_failure_status, record_forward_provider_failure_status,
    record_forward_provider_rectifier_retry_failure_status, record_forward_request_started_status,
    record_forward_success_status, CurrentRouteChannelTargetInput, CurrentRouteTarget,
    CurrentRouteTargetInput, ForwardCurrentProviderStatusInput, ForwardFailureStatusInput,
    ForwardProviderFailureStatusInput, ForwardProviderRectifierRetryFailureStatusInput,
    ForwardRequestStartedStatusInput, ForwardSuccessStatusInput, ProxyRuntimeStatus,
};
use crate::proxy_core::api::transport::{
    build_retryable_forward_failure_log, build_terminal_forward_failure_log,
    categorize_forward_failure, forwarder_failure_log_line,
    forwarder_no_available_provider_status_message, forwarder_rectifier_retry_failure_label,
    forwarder_rectifier_retry_failure_message, forwarder_rectifier_retry_success_message,
    forwarder_terminal_failure_status_message, should_failover_after_rectifier_retry_failure,
    ForwardFailureCategory, ForwarderRectifierRetryKind,
};

pub(crate) struct CcSwitchForwarderRuntimeStateSource {
    status: Arc<RwLock<ProxyRuntimeStatus>>,
    current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    events: Arc<ProxyEventBus>,
}

impl CcSwitchForwarderRuntimeStateSource {
    pub(crate) fn new(
        status: Arc<RwLock<ProxyRuntimeStatus>>,
        current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
        events: Arc<ProxyEventBus>,
    ) -> Self {
        Self {
            status,
            current_providers,
            events,
        }
    }

    #[cfg(test)]
    pub(crate) fn status(&self) -> Arc<RwLock<ProxyRuntimeStatus>> {
        self.status.clone()
    }
}

fn terminal_forward_failure_log_line_for_error(
    app_type: &str,
    attempted_providers: usize,
    total_providers: usize,
    last_error: Option<&ProxyError>,
) -> Option<String> {
    let last_failure = last_error.map(forward_failure_kind_from_proxy_error);
    build_terminal_forward_failure_log(attempted_providers, total_providers, last_failure.as_ref())
        .map(|log| forwarder_failure_log_line(app_type, &log))
}

fn retryable_forward_failure_log_line(
    app_type: &str,
    error: &ProxyError,
    provider: &Provider,
    attempted_providers: usize,
    total_providers: usize,
) -> String {
    let failure = forward_failure_kind_from_proxy_error(error);
    let log = build_retryable_forward_failure_log(
        provider.name.as_str(),
        attempted_providers,
        total_providers,
        &failure,
    );
    forwarder_failure_log_line(app_type, &log)
}

fn forwarder_rectifier_retry_success_log_line(
    app_type: &str,
    kind: ForwarderRectifierRetryKind,
) -> String {
    format!(
        "[{app_type}] {}",
        forwarder_rectifier_retry_success_message(kind)
    )
}

fn forwarder_rectifier_retry_failure_log_line(
    app_type: &str,
    kind: ForwarderRectifierRetryKind,
    error: &ProxyError,
) -> String {
    format!(
        "[{app_type}] {}",
        forwarder_rectifier_retry_failure_message(kind, &error.to_string())
    )
}

fn record_forward_success_runtime_status(
    status: &mut ProxyRuntimeStatus,
    current_provider_id_at_start: &str,
    provider_id: &str,
) -> bool {
    record_forward_success_status(
        status,
        ForwardSuccessStatusInput {
            current_provider_id_at_start,
            provider_id,
        },
    )
    .should_switch_current_provider
}

fn record_forward_failure_runtime_status(status: &mut ProxyRuntimeStatus, error_message: &str) {
    record_forward_failure_status(status, ForwardFailureStatusInput { error_message });
}

fn record_forward_request_started_runtime_status(status: &mut ProxyRuntimeStatus, timestamp: &str) {
    record_forward_request_started_status(status, ForwardRequestStartedStatusInput { timestamp });
}

async fn record_forward_active_connection_acquired_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
) {
    let mut status = status.write().await;
    record_active_connection_acquired_status(&mut status);
}

async fn record_forward_active_connection_released_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
) {
    let mut status = status.write().await;
    record_active_connection_released_status(&mut status);
}

async fn record_forward_request_started_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    timestamp: &str,
) {
    let mut status = status.write().await;
    record_forward_request_started_runtime_status(&mut status, timestamp);
}

async fn record_forward_current_provider_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    provider_id: &str,
    provider_name: &str,
) {
    let mut status = status.write().await;
    record_forward_current_provider_status(
        &mut status,
        ForwardCurrentProviderStatusInput {
            provider_id,
            provider_name,
        },
    );
}

async fn record_forward_success_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    current_provider_id_at_start: &str,
    provider_id: &str,
) -> bool {
    let mut status = status.write().await;
    record_forward_success_runtime_status(&mut status, current_provider_id_at_start, provider_id)
}

async fn record_forward_failure_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    error_message: &str,
) {
    let mut status = status.write().await;
    record_forward_failure_runtime_status(&mut status, error_message);
}

async fn record_forward_provider_failure_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    provider: &Provider,
    error: &ProxyError,
) {
    let mut status = status.write().await;
    let error_message = error.to_string();
    record_forward_provider_failure_status(
        &mut status,
        ForwardProviderFailureStatusInput {
            provider_name: provider.name.as_str(),
            error_message: &error_message,
        },
    );
}

async fn record_forward_provider_rectifier_retry_failure_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    provider: &Provider,
    kind: ForwarderRectifierRetryKind,
    error: &ProxyError,
) {
    let mut status = status.write().await;
    let error_message = error.to_string();
    record_forward_provider_rectifier_retry_failure_status(
        &mut status,
        ForwardProviderRectifierRetryFailureStatusInput {
            provider_name: provider.name.as_str(),
            rectifier_label: forwarder_rectifier_retry_failure_label(kind),
            error_message: &error_message,
        },
    );
}

fn current_route_target_from_forward_attempt(
    app_type: &str,
    attempt: &ForwardAttempt,
) -> CurrentRouteTarget {
    let provider = attempt.provider();
    let channel = attempt.channel();
    current_route_target_from_input(CurrentRouteTargetInput {
        app_type,
        provider_id: provider.id.as_str(),
        provider_name: provider.name.as_str(),
        channel: channel.map(|channel| CurrentRouteChannelTargetInput {
            channel_id: channel.channel_id.as_str(),
            channel_name: channel.channel_name.as_str(),
            interface_kind: channel.interface_kind.as_str(),
            public_model: channel.public_model.as_deref(),
            upstream_model: channel.upstream_model.as_deref(),
            pricing_model: channel.pricing_model.as_deref(),
        }),
    })
}

pub(crate) fn current_route_target_from_provider(
    app_type: &str,
    provider_id: &str,
    provider_name: &str,
) -> CurrentRouteTarget {
    current_route_target_from_input(CurrentRouteTargetInput {
        app_type,
        provider_id,
        provider_name,
        channel: None,
    })
}

pub(crate) async fn set_active_route_target_runtime_source(
    current_providers: &RwLock<HashMap<String, CurrentRouteTarget>>,
    app_type: &str,
    provider_id: &str,
    provider_name: &str,
) {
    let mut current_providers = current_providers.write().await;
    current_providers.insert(
        app_type.to_string(),
        current_route_target_from_provider(app_type, provider_id, provider_name),
    );
}

fn emit_request_started_event_source(events: &ProxyEventBus, request_id: &str, app_type: &str) {
    events.emit_core_event(request_started_event(request_id, app_type));
}

fn attempt_event_payload_input_from_forward_attempt<'a>(
    request_id: &'a str,
    app_type: &'a str,
    attempt: &'a ForwardAttempt,
    error: Option<&'a str>,
) -> AttemptEventPayloadInput<'a> {
    let provider = attempt.provider();
    let channel = attempt.channel().map(|channel| AttemptEventChannel {
        channel_id: channel.channel_id.as_str(),
        channel_name: channel.channel_name.as_str(),
        interface_kind: channel.interface_kind.as_str(),
        public_model: channel.public_model.as_deref(),
        upstream_model: channel.upstream_model.as_deref(),
        pricing_model: channel.pricing_model.as_deref(),
    });

    AttemptEventPayloadInput {
        request_id,
        app_type,
        provider_id: provider.id.as_str(),
        provider_name: provider.name.as_str(),
        channel,
        error,
    }
}

fn emit_attempt_event_source(
    events: &ProxyEventBus,
    request_id: &str,
    app_type: &str,
    attempt: &ForwardAttempt,
    phase: AttemptEventPhase,
    error: Option<&str>,
) {
    events.emit_core_event(attempt_event(
        attempt_event_payload_input_from_forward_attempt(request_id, app_type, attempt, error),
        attempt.is_channel(),
        phase,
    ));
}

async fn record_forward_active_route_target_runtime_source(
    current_providers: &RwLock<HashMap<String, CurrentRouteTarget>>,
    events: &ProxyEventBus,
    request_id: &str,
    app_type: &str,
    attempt: &ForwardAttempt,
) {
    {
        let mut current_providers = current_providers.write().await;
        current_providers.insert(
            app_type.to_string(),
            current_route_target_from_forward_attempt(app_type, attempt),
        );
    }

    events.emit_core_event(route_selected_event(
        attempt_event_payload_input_from_forward_attempt(request_id, app_type, attempt, None),
    ));
}

impl ForwarderRuntimeStateSource for CcSwitchForwarderRuntimeStateSource {
    fn next_request_id(&self) -> String {
        Uuid::new_v4().to_string()
    }

    fn record_request_started<'a>(
        &'a self,
        request_id: &'a str,
        app_type: &'a str,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            emit_request_started_event_source(self.events.as_ref(), request_id, app_type);
            let started_at = chrono::Utc::now().to_rfc3339();
            record_forward_request_started_runtime_source(self.status.as_ref(), &started_at).await;
        })
    }

    fn record_attempt_started(&self, input: ForwarderAttemptStartedInput<'_>) {
        emit_attempt_event_source(
            self.events.as_ref(),
            input.request_id,
            input.app_type,
            input.attempt,
            AttemptEventPhase::Started,
            None,
        );
    }

    fn record_successful_attempt<'a>(
        &'a self,
        input: ForwarderSuccessfulAttemptInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            emit_attempt_event_source(
                self.events.as_ref(),
                input.request_id,
                input.app_type,
                input.attempt,
                AttemptEventPhase::Succeeded,
                None,
            );
            record_forward_active_route_target_runtime_source(
                self.current_providers.as_ref(),
                self.events.as_ref(),
                input.request_id,
                input.app_type,
                input.attempt,
            )
            .await;
        })
    }

    fn record_failed_attempt(&self, input: ForwarderAttemptFailedInput<'_>) {
        let error_message = input.error.to_string();
        emit_attempt_event_source(
            self.events.as_ref(),
            input.request_id,
            input.app_type,
            input.attempt,
            AttemptEventPhase::Failed,
            Some(&error_message),
        );
    }

    fn record_success_status<'a>(
        &'a self,
        input: ForwarderSuccessStatusInput<'a>,
    ) -> BoxFuture<'a, Option<ForwarderFailoverSwitchTarget>> {
        Box::pin(async move {
            let should_switch = record_forward_success_runtime_source(
                self.status.as_ref(),
                input.current_provider_id_at_start,
                input.provider.id.as_str(),
            )
            .await;
            should_switch.then(|| ForwarderFailoverSwitchTarget {
                provider_id: input.provider.id.clone(),
                provider_name: input.provider.name.clone(),
            })
        })
    }

    fn record_current_provider<'a>(&'a self, provider: &'a Provider) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_current_provider_runtime_source(
                self.status.as_ref(),
                provider.id.as_str(),
                provider.name.as_str(),
            )
            .await;
        })
    }

    fn record_provider_failure<'a>(
        &'a self,
        input: ForwarderProviderFailureInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_provider_failure_runtime_source(
                self.status.as_ref(),
                input.provider,
                input.error,
            )
            .await;
        })
    }

    fn record_provider_rectifier_retry_failure<'a>(
        &'a self,
        input: ForwarderProviderRectifierRetryFailureInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_provider_rectifier_retry_failure_runtime_source(
                self.status.as_ref(),
                input.provider,
                input.kind,
                input.error,
            )
            .await;
        })
    }

    fn forward_failure_decision(&self, error: &ProxyError) -> ForwarderFailureDecision {
        let failure = forward_failure_kind_from_proxy_error(error);
        match categorize_forward_failure(&failure) {
            ForwardFailureCategory::Retryable => ForwarderFailureDecision::Retryable,
            ForwardFailureCategory::NonRetryable => ForwarderFailureDecision::NonRetryable,
        }
    }

    fn log_retryable_forward_failure(&self, input: ForwarderRetryableFailureLogInput<'_>) {
        log::warn!(
            "{}",
            retryable_forward_failure_log_line(
                input.app_type,
                input.error,
                input.provider,
                input.attempted_providers,
                input.total_providers
            )
        );
    }

    fn log_terminal_forward_failure(&self, input: ForwarderTerminalFailureLogInput<'_>) {
        if let Some(log_line) = terminal_forward_failure_log_line_for_error(
            input.app_type,
            input.attempted_providers,
            input.total_providers,
            input.last_error,
        ) {
            log::warn!("{log_line}");
        }
    }

    fn rectifier_retry_failure_decision(
        &self,
        error: &ProxyError,
    ) -> ForwarderRectifierRetryFailureDecision {
        let failure = forward_failure_kind_from_proxy_error(error);
        if should_failover_after_rectifier_retry_failure(&failure) {
            ForwarderRectifierRetryFailureDecision::ProviderFailure
        } else {
            ForwarderRectifierRetryFailureDecision::ClientFailure
        }
    }

    fn log_rectifier_retry_success(&self, input: ForwarderRectifierRetrySuccessLogInput<'_>) {
        log::info!(
            "{}",
            forwarder_rectifier_retry_success_log_line(input.app_type, input.kind)
        );
    }

    fn log_rectifier_retry_failure(&self, input: ForwarderRectifierRetryFailureLogInput<'_>) {
        log::warn!(
            "{}",
            forwarder_rectifier_retry_failure_log_line(input.app_type, input.kind, input.error)
        );
    }

    fn record_no_available_provider_status<'a>(&'a self) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_failure_runtime_source(
                self.status.as_ref(),
                forwarder_no_available_provider_status_message(),
            )
            .await;
        })
    }

    fn record_terminal_failure_status<'a>(&'a self) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_failure_runtime_source(
                self.status.as_ref(),
                forwarder_terminal_failure_status_message(),
            )
            .await;
        })
    }

    fn record_forward_error_status<'a>(&'a self, error: &'a ProxyError) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let error_message = error.to_string();
            record_forward_failure_runtime_source(self.status.as_ref(), &error_message).await;
        })
    }

    fn record_active_connection_acquired<'a>(&'a self) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_active_connection_acquired_runtime_source(self.status.as_ref()).await;
        })
    }

    fn record_active_connection_released<'a>(&'a self) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_active_connection_released_runtime_source(self.status.as_ref()).await;
        })
    }
}

pub(crate) fn forwarder_runtime_state_source_from_runtime_parts(
    status: Arc<RwLock<ProxyRuntimeStatus>>,
    current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    events: Arc<ProxyEventBus>,
) -> ForwarderRuntimeStateSourceRef {
    Arc::new(CcSwitchForwarderRuntimeStateSource::new(
        status,
        current_providers,
        events,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projects_rectifier_retry_logs() {
        let timeout = ProxyError::Timeout("upstream timed out".to_string());

        assert_eq!(
            forwarder_rectifier_retry_success_log_line(
                "claude",
                ForwarderRectifierRetryKind::MediaFallback,
            ),
            "[claude] [Media] Unsupported-image retry succeeded"
        );
        assert_eq!(
            forwarder_rectifier_retry_failure_log_line(
                "claude",
                ForwarderRectifierRetryKind::ThinkingSignature,
                &timeout,
            ),
            "[claude] [RECT-003] 整流重试仍失败: 超时: upstream timed out"
        );
        assert_eq!(
            forwarder_rectifier_retry_success_log_line(
                "claude",
                ForwarderRectifierRetryKind::ThinkingBudget,
            ),
            "[claude] [RECT-011] budget 整流重试成功"
        );
        assert_eq!(
            forwarder_rectifier_retry_failure_label(ForwarderRectifierRetryKind::MediaFallback),
            "media 降级"
        );
        assert_eq!(
            forwarder_rectifier_retry_failure_label(ForwarderRectifierRetryKind::ThinkingSignature),
            "整流"
        );
        assert_eq!(
            forwarder_rectifier_retry_failure_label(ForwarderRectifierRetryKind::ThinkingBudget),
            "budget 整流"
        );
    }

    #[test]
    fn projects_forward_failure_policy() {
        let source = CcSwitchForwarderRuntimeStateSource::new(
            Arc::new(RwLock::new(ProxyRuntimeStatus::default())),
            Arc::new(RwLock::new(HashMap::new())),
            Arc::new(ProxyEventBus::default()),
        );
        let provider = Provider::with_id("relay".to_string(), "Relay".to_string(), json!({}), None);
        let retryable =
            source.forward_failure_decision(&ProxyError::Timeout("upstream timed out".to_string()));
        let non_retryable_error = ProxyError::UpstreamError {
            status: 400,
            body: Some(r#"{"error":{"message":"bad request"}}"#.to_string()),
        };
        let non_retryable = source.forward_failure_decision(&non_retryable_error);

        match retryable {
            ForwarderFailureDecision::Retryable => {}
            ForwarderFailureDecision::NonRetryable => {
                panic!("timeout should be retryable")
            }
        }
        assert_eq!(
            retryable_forward_failure_log_line(
                "claude",
                &ProxyError::Timeout("upstream timed out".to_string()),
                &provider,
                1,
                2,
            ),
            "[claude] [FWD-001] Provider Relay 失败，继续尝试下一个 (1/2): 请求超时: upstream timed out"
        );

        match non_retryable {
            ForwarderFailureDecision::Retryable => {
                panic!("client 400 should be non-retryable")
            }
            ForwarderFailureDecision::NonRetryable => {}
        }

        let terminal_log_line =
            terminal_forward_failure_log_line_for_error("claude", 2, 2, Some(&non_retryable_error))
                .expect("terminal failure log for multi-provider attempts");
        assert!(terminal_log_line.starts_with("[claude] [FWD-002] "));
        assert!(terminal_log_line.contains("上游 HTTP 400"));
    }
}
