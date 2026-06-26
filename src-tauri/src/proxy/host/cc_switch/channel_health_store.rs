//! CC Switch channel health store.

use crate::database::Database;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::{
    ChannelAttemptResult, ChannelBreakerStats, ChannelHealthReset, ChannelHealthStore,
};
use crate::proxy_core_adapter::{
    channel_breaker_stats_with_router_source, record_channel_attempt_in_db_source,
    reset_channel_health_with_router_source,
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
