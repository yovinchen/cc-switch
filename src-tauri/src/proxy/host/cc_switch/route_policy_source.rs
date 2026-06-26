//! CC Switch route policy source.

use crate::database::Database;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::RoutePolicySource;
use crate::proxy_core::api::routing::RoutePolicy;
use crate::proxy_core_adapter::route_policy_from_db_source;
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
        Box::pin(async move { route_policy_from_db_source(&self.db, app) })
    }
}
