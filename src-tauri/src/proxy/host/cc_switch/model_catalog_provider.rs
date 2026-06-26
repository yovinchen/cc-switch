//! CC Switch model catalog provider.

use crate::database::Database;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::auth::ClaudeDesktopModelRouteInput;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::model_catalog::ModelCatalog;
use crate::proxy_core::api::ports::ModelCatalogProvider;
use crate::proxy_core_adapter::{
    claude_desktop_model_routes_from_router_source, client_model_catalog_from_app_source,
    provider_model_catalog_from_db_source,
};
use futures::future::BoxFuture;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct CcSwitchModelCatalogProvider {
    db: Arc<Database>,
    router: Arc<ProviderRouter>,
}

impl CcSwitchModelCatalogProvider {
    pub(crate) fn new(db: Arc<Database>, router: Arc<ProviderRouter>) -> Self {
        Self { db, router }
    }
}

impl ModelCatalogProvider for CcSwitchModelCatalogProvider {
    fn load_catalog<'a>(
        &'a self,
        app: &'a AppKind,
        provider_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
        Box::pin(async move { provider_model_catalog_from_db_source(&self.db, app, provider_id) })
    }

    fn load_client_catalog<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
        Box::pin(async move { client_model_catalog_from_app_source(app) })
    }

    fn load_claude_desktop_model_routes<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ClaudeDesktopModelRouteInput>>> {
        Box::pin(async move {
            claude_desktop_model_routes_from_router_source(&self.db, &self.router, app).await
        })
    }
}
