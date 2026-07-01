//! CC Switch ProviderRouter health store.

use crate::database::Database;
use crate::error::AppError;
use crate::proxy::engine::routing::ProviderRouterHealthStore;
use crate::proxy::host::cc_switch::channel_health_store::record_channel_attempt_in_db_source;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::{config_error_with_context, ProxyCoreError, ProxyCoreResult};
use crate::proxy_core::api::ports::{
    ChannelAttemptResult, ChannelHealthReset, ProviderAttemptResult, ProviderHealthStore,
};
use futures::future::BoxFuture;
use std::sync::Arc;

pub(crate) struct CcSwitchProviderRouterHealthStore {
    db: Arc<Database>,
}

impl CcSwitchProviderRouterHealthStore {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

struct ProviderHealthAttemptDbUpdate {
    provider_id: String,
    app_type: String,
    success: bool,
    error_msg: Option<String>,
    failure_threshold: u32,
}

fn provider_health_attempt_db_update(
    result: ProviderAttemptResult,
) -> ProviderHealthAttemptDbUpdate {
    ProviderHealthAttemptDbUpdate {
        provider_id: result.provider_id,
        app_type: result.app.as_str().to_string(),
        success: result.success,
        error_msg: result.error_message,
        failure_threshold: result.failure_threshold,
    }
}

async fn record_provider_attempt_in_db_source(
    db: &Database,
    result: ProviderAttemptResult,
) -> ProxyCoreResult<()> {
    let update = provider_health_attempt_db_update(result);
    db.update_provider_health_with_threshold(
        &update.provider_id,
        &update.app_type,
        update.success,
        update.error_msg,
        update.failure_threshold,
    )
    .await
    .map_err(|error| config_error_with_context("record provider attempt", error))
}

fn record_channel_health_attempt_from_router_db(
    db: &Database,
    result: ChannelAttemptResult,
) -> Result<(), AppError> {
    record_channel_attempt_in_db_source(db, result).map_err(provider_router_health_store_app_error)
}

fn reset_channel_health_from_router_db(
    db: &Database,
    reset: ChannelHealthReset,
) -> Result<(), AppError> {
    db.reset_proxy_channel_health(&reset.channel_id)
}

fn provider_router_health_store_app_error(error: ProxyCoreError) -> AppError {
    match error {
        ProxyCoreError::Config(message) => AppError::Config(message),
        ProxyCoreError::InvalidRequest(message) => AppError::InvalidInput(message),
        other => AppError::Message(other.to_string()),
    }
}

impl ProviderHealthStore for CcSwitchProviderRouterHealthStore {
    fn record_attempt<'a>(
        &'a self,
        result: ProviderAttemptResult,
    ) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move { record_provider_attempt_in_db_source(&self.db, result).await })
    }
}

impl ProviderRouterHealthStore for CcSwitchProviderRouterHealthStore {
    fn record_provider_health<'a>(
        &'a self,
        provider_id: &'a str,
        app_type: &'a str,
        success: bool,
        error_msg: Option<String>,
        failure_threshold: u32,
    ) -> BoxFuture<'a, Result<(), AppError>> {
        Box::pin(async move {
            ProviderHealthStore::record_attempt(
                self,
                ProviderAttemptResult {
                    provider_id: provider_id.to_string(),
                    app: AppKind::from(app_type),
                    success,
                    failure_threshold,
                    error_message: error_msg,
                },
            )
            .await
            .map_err(provider_router_health_store_app_error)
        })
    }

    fn record_channel_health<'a>(
        &'a self,
        result: ChannelAttemptResult,
    ) -> BoxFuture<'a, Result<(), AppError>> {
        Box::pin(async move { record_channel_health_attempt_from_router_db(&self.db, result) })
    }

    fn reset_channel_health(&self, reset: ChannelHealthReset) -> Result<(), AppError> {
        reset_channel_health_from_router_db(&self.db, reset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_health_store_projects_attempt_db_update() {
        let update = provider_health_attempt_db_update(ProviderAttemptResult {
            provider_id: "provider-a".to_string(),
            app: AppKind::Claude,
            success: false,
            failure_threshold: 3,
            error_message: Some("timeout".to_string()),
        });

        assert_eq!(update.provider_id, "provider-a");
        assert_eq!(update.app_type, "claude");
        assert!(!update.success);
        assert_eq!(update.error_msg.as_deref(), Some("timeout"));
        assert_eq!(update.failure_threshold, 3);
    }
}
