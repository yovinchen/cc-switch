//! CC Switch model catalog provider.

use crate::database::Database;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::auth::ClaudeDesktopModelRouteInput;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::model_catalog::{
    client_model_catalog_from_optional_raw, client_model_catalog_raw_from_text,
    client_model_catalog_source_for_app, empty_client_model_catalog_raw, ClientModelCatalogSource,
    ModelCatalog,
};
use crate::proxy_core::api::ports::ModelCatalogProvider;
use crate::proxy_core_adapter::{
    claude_desktop_model_routes_from_router_source, provider_model_catalog_from_db_source,
};
use futures::future::BoxFuture;
use serde_json::Value;
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

fn client_model_catalog_from_app_source(app: &AppKind) -> ProxyCoreResult<ModelCatalog> {
    let source = client_model_catalog_source_for_app(app.as_str());
    let raw = match source {
        ClientModelCatalogSource::CodexActiveConfig => {
            Some(codex_client_model_catalog_raw_from_active_config())
        }
        ClientModelCatalogSource::Empty => None,
    };
    Ok(client_model_catalog_from_optional_raw(app.as_str(), raw))
}

fn codex_client_model_catalog_raw_from_active_config() -> Value {
    let generated_path = crate::codex_config::get_codex_model_catalog_path();
    let active_catalog_path = match crate::codex_config::read_codex_config_text() {
        Ok(config_text) => {
            crate::codex_config::resolve_cc_switch_catalog_path(&config_text, &generated_path)
        }
        Err(_) => None,
    };

    if let Some(catalog_path) = active_catalog_path.as_ref().filter(|path| path.exists()) {
        let text = std::fs::read_to_string(catalog_path).unwrap_or_default();
        client_model_catalog_raw_from_text(&text)
    } else {
        if active_catalog_path.is_none() {
            log::debug!(
                "[models] stale guard: catalog not served (model_catalog_json not set to cc-switch catalog)"
            );
        }
        empty_client_model_catalog_raw()
    }
}
