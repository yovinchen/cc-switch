use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy::events::ProxyEventBus;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy_core_adapter::{
    AttemptEventPhase, CurrentRouteTarget, ForwardFailureCategory, ForwarderFailoverSwitchTarget,
    ForwarderFailureDecision, ForwarderRectifierRetryFailureDecision, ForwarderRectifierRetryKind,
    ForwarderRuntimeStateSource, ForwarderRuntimeStateSourceRef, ProxyRuntimeStatus,
    categorize_forward_failure, emit_attempt_event_source, emit_request_started_event_source,
    forward_failure_kind_from_proxy_error, forwarder_no_available_provider_status_message,
    forwarder_rectifier_retry_failure_log_line, forwarder_rectifier_retry_success_log_line,
    forwarder_terminal_failure_status_message,
    record_forward_active_connection_acquired_runtime_source,
    record_forward_active_connection_released_runtime_source,
    record_forward_active_route_target_runtime_source,
    record_forward_current_provider_runtime_source, record_forward_failure_runtime_source,
    record_forward_provider_failure_runtime_source,
    record_forward_provider_rectifier_retry_failure_runtime_source,
    record_forward_request_started_runtime_source, record_forward_success_runtime_source,
    retryable_forward_failure_log_line, should_failover_after_rectifier_retry_failure,
    terminal_forward_failure_log_line_for_error,
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

impl ForwarderRuntimeStateSource for CcSwitchForwarderRuntimeStateSource {
    fn next_request_id(&self) -> String {
        Uuid::new_v4().to_string()
    }

    fn emit_request_started(&self, request_id: &str, app_type: &str) {
        emit_request_started_event_source(self.events.as_ref(), request_id, app_type);
    }

    fn emit_attempt_started(&self, request_id: &str, app_type: &str, attempt: &ForwardAttempt) {
        emit_attempt_event_source(
            self.events.as_ref(),
            request_id,
            app_type,
            attempt,
            AttemptEventPhase::Started,
            None,
        );
    }

    fn emit_attempt_succeeded(&self, request_id: &str, app_type: &str, attempt: &ForwardAttempt) {
        emit_attempt_event_source(
            self.events.as_ref(),
            request_id,
            app_type,
            attempt,
            AttemptEventPhase::Succeeded,
            None,
        );
    }

    fn emit_attempt_failed_for_error(
        &self,
        request_id: &str,
        app_type: &str,
        attempt: &ForwardAttempt,
        error: &ProxyError,
    ) {
        let error_message = error.to_string();
        emit_attempt_event_source(
            self.events.as_ref(),
            request_id,
            app_type,
            attempt,
            AttemptEventPhase::Failed,
            Some(&error_message),
        );
    }

    fn record_active_route_target<'a>(
        &'a self,
        request_id: &'a str,
        app_type: &'a str,
        attempt: &'a ForwardAttempt,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_active_route_target_runtime_source(
                self.current_providers.as_ref(),
                self.events.as_ref(),
                request_id,
                app_type,
                attempt,
            )
            .await;
        })
    }

    fn record_success_status<'a>(
        &'a self,
        current_provider_id_at_start: &'a str,
        provider: &'a Provider,
    ) -> BoxFuture<'a, Option<ForwarderFailoverSwitchTarget>> {
        Box::pin(async move {
            let should_switch = record_forward_success_runtime_source(
                self.status.as_ref(),
                current_provider_id_at_start,
                provider.id.as_str(),
            )
            .await;
            should_switch.then(|| ForwarderFailoverSwitchTarget {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
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
        provider: &'a Provider,
        error: &'a ProxyError,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_provider_failure_runtime_source(self.status.as_ref(), provider, error)
                .await;
        })
    }

    fn record_provider_rectifier_retry_failure<'a>(
        &'a self,
        provider: &'a Provider,
        kind: ForwarderRectifierRetryKind,
        error: &'a ProxyError,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_provider_rectifier_retry_failure_runtime_source(
                self.status.as_ref(),
                provider,
                kind,
                error,
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

    fn log_retryable_forward_failure(
        &self,
        app_type: &str,
        error: &ProxyError,
        provider: &Provider,
        attempted_providers: usize,
        total_providers: usize,
    ) {
        log::warn!(
            "{}",
            retryable_forward_failure_log_line(
                app_type,
                error,
                provider,
                attempted_providers,
                total_providers
            )
        );
    }

    fn log_terminal_forward_failure(
        &self,
        app_type: &str,
        attempted_providers: usize,
        total_providers: usize,
        last_error: Option<&ProxyError>,
    ) {
        if let Some(log_line) = terminal_forward_failure_log_line_for_error(
            app_type,
            attempted_providers,
            total_providers,
            last_error,
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

    fn log_rectifier_retry_success(&self, app_type: &str, kind: ForwarderRectifierRetryKind) {
        log::info!(
            "{}",
            forwarder_rectifier_retry_success_log_line(app_type, kind)
        );
    }

    fn log_rectifier_retry_failure(
        &self,
        app_type: &str,
        kind: ForwarderRectifierRetryKind,
        error: &ProxyError,
    ) {
        log::warn!(
            "{}",
            forwarder_rectifier_retry_failure_log_line(app_type, kind, error)
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

    fn record_request_started_now<'a>(&'a self) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let started_at = chrono::Utc::now().to_rfc3339();
            record_forward_request_started_runtime_source(self.status.as_ref(), &started_at).await;
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
