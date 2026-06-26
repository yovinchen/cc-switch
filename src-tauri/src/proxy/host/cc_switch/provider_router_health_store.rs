//! CC Switch ProviderRouter health store.

use crate::database::Database;
use crate::error::AppError;
use crate::proxy::engine::routing::ProviderRouterHealthStore;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::{
    ChannelAttemptResult, ChannelHealthReset, ProviderAttemptResult, ProviderHealthStore,
};
use crate::proxy_core_adapter::{
    app_error_from_proxy_core_error, record_channel_health_attempt_from_router_db,
    record_provider_attempt_in_db_source, reset_channel_health_from_router_db,
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
            .map_err(app_error_from_proxy_core_error)
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
