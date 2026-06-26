//! CC Switch provider source.

use crate::database::Database;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core_adapter::{
    CurrentRouteTarget, ProviderSource, ProviderSpec, ProxyCoreAppKind as AppKind, ProxyCoreResult,
    active_route_target_from_runtime_source, current_provider_id_from_db_source,
    provider_spec_from_db_source, provider_specs_from_db_source,
    route_candidate_provider_ids_from_router_source,
};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub(crate) struct CcSwitchProviderSource {
    db: Arc<Database>,
    router: Arc<ProviderRouter>,
    current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
}

impl CcSwitchProviderSource {
    pub(crate) fn new(
        db: Arc<Database>,
        router: Arc<ProviderRouter>,
        current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    ) -> Self {
        Self {
            db,
            router,
            current_providers,
        }
    }
}

impl ProviderSource for CcSwitchProviderSource {
    fn list_providers<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ProviderSpec>>> {
        Box::pin(async move { provider_specs_from_db_source(&self.db, app) })
    }

    fn get_provider<'a>(
        &'a self,
        app: &'a AppKind,
        provider_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ProviderSpec>>> {
        Box::pin(async move { provider_spec_from_db_source(&self.db, app, provider_id) })
    }

    fn current_provider_id<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<String>>> {
        Box::pin(async move { current_provider_id_from_db_source(&self.db, app) })
    }

    fn active_route_target<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<CurrentRouteTarget>>> {
        Box::pin(async move {
            active_route_target_from_runtime_source(&self.current_providers, app).await
        })
    }

    fn route_candidate_provider_ids<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<String>>> {
        Box::pin(
            async move { route_candidate_provider_ids_from_router_source(&self.router, app).await },
        )
    }
}
