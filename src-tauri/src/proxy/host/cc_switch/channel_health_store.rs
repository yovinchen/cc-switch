//! CC Switch channel health store.

use crate::database::Database;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::errors::{config_error_with_context, ProxyCoreResult};
use crate::proxy_core::api::management::channel_not_found_error;
use crate::proxy_core::api::ports::{
    channel_breaker_stats_from_parts, channel_health_reset_from_parts, ChannelAttemptResult,
    ChannelBreakerStats, ChannelHealthReset, ChannelHealthStore,
    DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD,
};
use futures::future::BoxFuture;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct CcSwitchChannelHealthStore {
    db: Arc<Database>,
    router: Arc<ProviderRouter>,
}

impl CcSwitchChannelHealthStore {
    pub(crate) fn new(db: Arc<Database>, router: Arc<ProviderRouter>) -> Self {
        Self { db, router }
    }
}

#[derive(Debug)]
struct ChannelHealthResetPlan {
    channel_id: String,
    app_type: String,
}

fn channel_health_reset_plan_from_lookup(
    channel_id: &str,
    app_type: Option<String>,
) -> ProxyCoreResult<ChannelHealthResetPlan> {
    let app_type = app_type.ok_or_else(|| channel_not_found_error(channel_id))?;
    Ok(ChannelHealthResetPlan {
        channel_id: channel_id.to_string(),
        app_type,
    })
}

struct ChannelHealthAttemptDbUpdate {
    channel_id: String,
    success: bool,
    error_code: Option<String>,
    failure_threshold: u32,
    response_time_ms: Option<i64>,
}

fn channel_health_attempt_db_update(result: ChannelAttemptResult) -> ChannelHealthAttemptDbUpdate {
    ChannelHealthAttemptDbUpdate {
        channel_id: result.channel_id,
        success: result.success,
        error_code: result.error_code,
        failure_threshold: result
            .failure_threshold
            .unwrap_or(DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD),
        response_time_ms: result.latency_ms.map(|latency| latency as i64),
    }
}

pub(super) fn record_channel_attempt_in_db_source(
    db: &Database,
    result: ChannelAttemptResult,
) -> ProxyCoreResult<()> {
    let update = channel_health_attempt_db_update(result);
    db.update_proxy_channel_health_with_threshold(
        &update.channel_id,
        update.success,
        update.error_code,
        update.failure_threshold,
        update.response_time_ms,
    )
    .map_err(|error| config_error_with_context("record channel attempt", error))
}

async fn reset_channel_health_with_router_source(
    db: &Database,
    router: &ProviderRouter,
    channel_id: &str,
) -> ProxyCoreResult<ChannelHealthReset> {
    let app_type = db
        .get_proxy_channel_app_type(channel_id)
        .map_err(|error| config_error_with_context("lookup channel app", error))?;
    let reset_plan = channel_health_reset_plan_from_lookup(channel_id, app_type)?;
    router
        .reset_channel_breaker(&reset_plan.channel_id, &reset_plan.app_type)
        .await
        .map_err(|error| config_error_with_context("reset channel health", error))?;
    Ok(channel_health_reset_from_parts(
        reset_plan.channel_id,
        reset_plan.app_type.as_str(),
    ))
}

async fn channel_breaker_stats_with_router_source(
    db: &Database,
    router: &ProviderRouter,
    channel_id: &str,
) -> ProxyCoreResult<ChannelBreakerStats> {
    let app_type = db
        .get_proxy_channel_app_type(channel_id)
        .map_err(|error| config_error_with_context("lookup channel app", error))?
        .ok_or_else(|| channel_not_found_error(channel_id))?;
    let stats = router
        .get_channel_circuit_breaker_stats(channel_id, &app_type)
        .await;

    Ok(channel_breaker_stats_from_parts(
        channel_id,
        app_type.as_str(),
        stats,
    ))
}

impl ChannelHealthStore for CcSwitchChannelHealthStore {
    fn record_attempt<'a>(
        &'a self,
        result: ChannelAttemptResult,
    ) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move { record_channel_attempt_in_db_source(&self.db, result) })
    }

    fn reset_channel<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelHealthReset>> {
        Box::pin(async move {
            reset_channel_health_with_router_source(&self.db, &self.router, channel_id).await
        })
    }

    fn channel_breaker_stats<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelBreakerStats>> {
        Box::pin(async move {
            channel_breaker_stats_with_router_source(&self.db, &self.router, channel_id).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::api::domain::AppKind;

    #[test]
    fn channel_health_store_projects_attempt_db_update() {
        let reset_plan =
            channel_health_reset_plan_from_lookup("channel-a", Some("claude".to_string()))
                .expect("reset plan");
        assert_eq!(reset_plan.channel_id, "channel-a");
        assert_eq!(reset_plan.app_type, "claude");
        let reset =
            channel_health_reset_from_parts(reset_plan.channel_id, reset_plan.app_type.as_str());
        assert_eq!(reset.channel_id, "channel-a");
        assert_eq!(reset.app, AppKind::Claude);
        let missing = channel_health_reset_plan_from_lookup("missing-channel", None)
            .expect_err("missing channel should fail");
        assert!(missing.to_string().contains("missing-channel"));

        let update = channel_health_attempt_db_update(ChannelAttemptResult {
            channel_id: "channel-a".to_string(),
            success: false,
            status_code: Some(429),
            latency_ms: Some(123),
            failure_threshold: None,
            error_code: Some("rate_limited".to_string()),
        });

        assert_eq!(update.channel_id, "channel-a");
        assert!(!update.success);
        assert_eq!(update.error_code.as_deref(), Some("rate_limited"));
        assert_eq!(
            update.failure_threshold,
            DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD
        );
        assert_eq!(update.response_time_ms, Some(123));

        let override_update = channel_health_attempt_db_update(ChannelAttemptResult {
            channel_id: "channel-a".to_string(),
            success: false,
            status_code: None,
            latency_ms: None,
            failure_threshold: Some(7),
            error_code: Some("timeout".to_string()),
        });

        assert_eq!(override_update.failure_threshold, 7);
        assert_eq!(override_update.response_time_ms, None);
    }
}
