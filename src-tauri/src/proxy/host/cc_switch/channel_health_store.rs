//! CC Switch channel health store.

use crate::database::Database;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::errors::{config_error_with_context, ProxyCoreResult};
use crate::proxy_core::api::management::channel_not_found_error;
use crate::proxy_core::api::ports::{
    channel_breaker_stats_from_parts, channel_health_reset_from_parts, ChannelAttemptResult,
    ChannelBreakerStats, ChannelHealthLookupInput, ChannelHealthReset, ChannelHealthStore,
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
        input: ChannelHealthLookupInput<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelHealthReset>> {
        Box::pin(async move {
            reset_channel_health_with_router_source(&self.db, &self.router, input.channel_id).await
        })
    }

    fn channel_breaker_stats<'a>(
        &'a self,
        input: ChannelHealthLookupInput<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelBreakerStats>> {
        Box::pin(async move {
            channel_breaker_stats_with_router_source(&self.db, &self.router, input.channel_id).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
    use crate::proxy_core::api::domain::AppKind;
    use http::StatusCode;
    use serde_json::json;

    fn save_claude_provider(db: &Database) {
        let provider = Provider::with_id(
            "anthropic-main".to_string(),
            "Anthropic Main".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay-a.example.com/v1/",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider)
            .expect("save provider");
        db.set_current_provider("claude", "anthropic-main")
            .expect("set current provider");
    }

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

    #[tokio::test]
    async fn health_store_records_channel_attempts_in_database() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        db.materialize_legacy_proxy_channels("claude")
            .expect("materialize channels");
        let channel_id = db
            .list_proxy_channels_for_app("claude")
            .expect("list channels")
            .first()
            .expect("channel")
            .id
            .clone();
        let store = CcSwitchChannelHealthStore::new(
            db.clone(),
            Arc::new(provider_router_from_database(db.clone())),
        );

        store
            .record_attempt(ChannelAttemptResult {
                channel_id: channel_id.clone(),
                success: false,
                status_code: Some(StatusCode::TOO_MANY_REQUESTS.as_u16()),
                latency_ms: Some(123),
                failure_threshold: None,
                error_code: Some("rate_limited".to_string()),
            })
            .await
            .expect("record attempt");

        let health = db
            .get_proxy_channel_health(&channel_id)
            .expect("read channel health");
        assert_eq!(health.status, "degraded");
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.response_time_ms, Some(123));
    }

    #[tokio::test]
    async fn health_store_resets_channel_health_through_router() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        db.materialize_legacy_proxy_channels("claude")
            .expect("materialize channels");
        let channel_id = db
            .list_proxy_channels_for_app("claude")
            .expect("list channels")
            .first()
            .expect("channel")
            .id
            .clone();
        db.update_proxy_channel_health_with_threshold(
            &channel_id,
            false,
            Some("rate_limited".to_string()),
            1,
            Some(99),
        )
        .expect("mark unhealthy");
        let store = CcSwitchChannelHealthStore::new(
            db.clone(),
            Arc::new(provider_router_from_database(db.clone())),
        );

        let reset = store
            .reset_channel(ChannelHealthLookupInput {
                channel_id: &channel_id,
            })
            .await
            .expect("reset channel health");

        assert_eq!(reset.channel_id, channel_id);
        assert_eq!(reset.app, AppKind::Claude);
        let health = db
            .get_proxy_channel_health(&reset.channel_id)
            .expect("read channel health");
        assert_eq!(health.status, "unknown");
    }
}
