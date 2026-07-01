//! CC Switch route policy source.

use crate::database::Database;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::{config_error_with_context, ProxyCoreResult};
use crate::proxy_core::api::ports::RoutePolicySource;
use crate::proxy_core::api::routing::{route_policy_from_failover_provider_ids, RoutePolicy};
use futures::future::BoxFuture;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct CcSwitchRoutePolicySource {
    db: Arc<Database>,
}

impl CcSwitchRoutePolicySource {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl RoutePolicySource for CcSwitchRoutePolicySource {
    fn load_policy<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<RoutePolicy>>> {
        Box::pin(async move {
            let queue = self
                .db
                .get_failover_queue(app.as_str())
                .map_err(|error| config_error_with_context("load route policy", error))?;
            Ok(Some(route_policy_from_failover_provider_ids(
                app.clone(),
                queue.into_iter().map(|item| item.provider_id),
            )))
        })
    }
}
