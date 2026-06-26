use futures::future::BoxFuture;
use std::sync::Arc;

use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy::error::ProxyError;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy_core_adapter::{
    allow_forward_attempt_runtime_source, forwarder_attempt_runtime_decision,
    record_forward_attempt_failure_runtime_source, record_forward_attempt_success_runtime_source,
    release_forward_attempt_permit_neutral_runtime_source, ForwarderAttemptAllowDecision,
    ForwarderAttemptAllowInput, ForwarderAttemptRuntimeDecisionInput,
    ForwarderAttemptRuntimeSource, ForwarderAttemptRuntimeSourceRef,
};

struct CcSwitchForwarderAttemptRuntimeSource {
    router: Arc<ProviderRouter>,
}

impl CcSwitchForwarderAttemptRuntimeSource {
    fn new(router: Arc<ProviderRouter>) -> Self {
        Self { router }
    }
}

impl ForwarderAttemptRuntimeSource for CcSwitchForwarderAttemptRuntimeSource {
    fn allow<'a>(
        &'a self,
        input: ForwarderAttemptAllowInput<'a>,
    ) -> BoxFuture<'a, ForwarderAttemptAllowDecision> {
        Box::pin(async move {
            let runtime_decision =
                forwarder_attempt_runtime_decision(ForwarderAttemptRuntimeDecisionInput {
                    app_type: input.app_type,
                    attempted_providers: input.attempted_providers,
                    max_attempts: input.max_attempts,
                    attempts_len: input.attempts.len(),
                    single_attempt_is_channel: input
                        .attempts
                        .first()
                        .is_some_and(ForwardAttempt::is_channel),
                });
            if let Some(log_line) = runtime_decision.limit_log_line {
                log::warn!("{log_line}");
                return ForwarderAttemptAllowDecision::Stop;
            }

            let permit = allow_forward_attempt_runtime_source(
                self.router.as_ref(),
                input.attempt,
                input.app_type,
                runtime_decision.bypass_circuit_breaker,
            )
            .await;
            if permit.allowed {
                ForwarderAttemptAllowDecision::Allowed {
                    used_half_open_permit: permit.used_half_open_permit,
                }
            } else {
                ForwarderAttemptAllowDecision::Skipped
            }
        })
    }

    fn record_success<'a>(
        &'a self,
        attempt: &'a ForwardAttempt,
        app_type: &'a str,
        used_half_open_permit: bool,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_attempt_success_runtime_source(
                &self.router,
                attempt,
                app_type,
                used_half_open_permit,
            )
            .await;
        })
    }

    fn record_failure<'a>(
        &'a self,
        attempt: &'a ForwardAttempt,
        app_type: &'a str,
        used_half_open_permit: bool,
        error: &'a ProxyError,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_attempt_failure_runtime_source(
                self.router.as_ref(),
                attempt,
                app_type,
                used_half_open_permit,
                error,
            )
            .await;
        })
    }

    fn release_attempt_permit_neutral<'a>(
        &'a self,
        attempt: &'a ForwardAttempt,
        app_type: &'a str,
        used_half_open_permit: bool,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            release_forward_attempt_permit_neutral_runtime_source(
                self.router.as_ref(),
                attempt,
                app_type,
                used_half_open_permit,
            )
            .await;
        })
    }
}

pub(crate) fn forwarder_attempt_runtime_source_from_router(
    router: Arc<ProviderRouter>,
) -> ForwarderAttemptRuntimeSourceRef {
    Arc::new(CcSwitchForwarderAttemptRuntimeSource::new(router))
}
