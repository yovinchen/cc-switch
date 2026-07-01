//! CC Switch model catalog provider.

use crate::database::Database;
use crate::error::AppError;
use crate::provider::Provider;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy::host::cc_switch::claude_desktop_provider::provider_claude_desktop_proxy_model_routes;
use crate::proxy_core::api::auth::{
    claude_desktop_provider_selection_error, claude_desktop_provider_unavailable_error,
    ClaudeDesktopModelRouteInput, ClaudeDesktopResolvedProxyRoute,
};
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::{
    config_error_with_context, internal_error_with_context, ProxyCoreError, ProxyCoreResult,
};
use crate::proxy_core::api::model_catalog::{
    client_model_catalog_from_optional_raw, client_model_catalog_raw_from_text,
    client_model_catalog_source_for_app, empty_client_model_catalog_raw,
    provider_model_catalog_from_settings, ClientModelCatalogSource, ModelCatalog,
};
use crate::proxy_core::api::ports::ModelCatalogProvider;
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

fn model_catalog_app_error(context: &str, error: AppError) -> ProxyCoreError {
    config_error_with_context(context, error)
}

fn provider_model_catalog_from_db_source(
    db: &Database,
    app: &AppKind,
    provider_id: &str,
) -> ProxyCoreResult<ModelCatalog> {
    let provider = db
        .get_provider_by_id(provider_id, app.as_str())
        .map_err(|error| internal_error_with_context("load model catalog", error))?;
    Ok(provider_model_catalog_from_settings(
        provider_id,
        provider.as_ref().map(|provider| &provider.settings_config),
    ))
}

async fn claude_desktop_model_routes_from_router_source(
    db: &Database,
    router: &ProviderRouter,
    app: &AppKind,
) -> ProxyCoreResult<Vec<ClaudeDesktopModelRouteInput>> {
    let provider_ids = router.select_provider_ids(app.as_str()).await;
    let provider = claude_desktop_provider_from_selection_result(provider_ids, |provider_id| {
        db.get_provider_by_id(provider_id, app.as_str())
    })?;
    let routes = provider_claude_desktop_proxy_model_routes(&provider).map_err(|issue| {
        model_catalog_app_error(
            "load claude desktop model routes",
            AppError::Config(format!(
                "Claude Desktop proxy model routes unavailable: {issue:?}"
            )),
        )
    })?;
    Ok(claude_desktop_model_routes_to_core_inputs(routes))
}

fn claude_desktop_provider_from_selection_result(
    result: Result<Vec<String>, AppError>,
    load_provider: impl FnOnce(&str) -> Result<Option<Provider>, AppError>,
) -> ProxyCoreResult<Provider> {
    let provider_ids = result.map_err(claude_desktop_provider_selection_error)?;
    let provider_id = provider_ids
        .into_iter()
        .next()
        .ok_or_else(claude_desktop_provider_unavailable_error)?;
    load_provider(&provider_id)
        .map_err(|error| model_catalog_app_error("load claude desktop provider", error))?
        .ok_or_else(claude_desktop_provider_unavailable_error)
}

fn claude_desktop_model_routes_to_core_inputs(
    routes: impl IntoIterator<Item = ClaudeDesktopResolvedProxyRoute>,
) -> Vec<ClaudeDesktopModelRouteInput> {
    routes
        .into_iter()
        .map(|route| ClaudeDesktopModelRouteInput::new(route.route_id, route.supports_1m))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::api::auth::ClaudeDesktopModelListResponse;
    use serde_json::json;

    #[test]
    fn claude_desktop_model_routes_to_core_inputs_preserve_route_contract() {
        let inputs =
            claude_desktop_model_routes_to_core_inputs([ClaudeDesktopResolvedProxyRoute {
                route_id: "claude-sonnet-4-6".to_string(),
                upstream_model: "anthropic/claude-sonnet-4-6".to_string(),
                label_override: None,
                supports_1m: true,
            }]);
        let response = ClaudeDesktopModelListResponse::from_routes(inputs.clone());

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].route_id, "claude-sonnet-4-6");
        assert!(inputs[0].supports_1m);
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].id, "claude-sonnet-4-6");
        assert!(response.data[0].supports_1m);
        assert_eq!(response.first_id.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(response.last_id.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn model_catalog_provider_projects_provider_settings_client_raw_and_selection_errors() {
        let settings = json!({
            "model": " claude-sonnet-4 ",
            "env": {
                "ANTHROPIC_MODEL": "claude-opus-4"
            },
            "modelCatalog": {
                "models": [
                    {"model": "deepseek-v4"},
                    {"id": "kimi-k2"}
                ]
            }
        });

        let provider_catalog = provider_model_catalog_from_settings("provider-a", Some(&settings));
        assert_eq!(provider_catalog.provider_id, "provider-a");
        assert_eq!(
            provider_catalog.models,
            vec![
                "claude-opus-4".to_string(),
                "claude-sonnet-4".to_string(),
                "deepseek-v4".to_string(),
                "kimi-k2".to_string()
            ]
        );
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            settings.clone(),
            None,
        );
        assert_eq!(
            provider_model_catalog_from_settings("provider-a", Some(&provider.settings_config))
                .models,
            provider_catalog.models
        );
        assert_eq!(
            claude_desktop_provider_from_selection_result(
                Ok(vec!["provider-a".to_string()]),
                |_| Ok(Some(provider.clone()))
            )
            .expect("selected provider")
            .id,
            "provider-a"
        );
        assert!(matches!(
            claude_desktop_provider_from_selection_result(Ok(Vec::new()), |_| Ok(None)),
            Err(ProxyCoreError::Unavailable(message))
                if message == "no available claude desktop provider"
        ));
        assert!(matches!(
            claude_desktop_provider_from_selection_result(
                Err(AppError::Message("router failed".to_string())),
                |_| Ok(None)
            ),
            Err(ProxyCoreError::Internal(message))
                if message == "select claude desktop provider: router failed"
        ));
        let client_catalog = client_model_catalog_from_optional_raw(
            AppKind::Codex.as_str(),
            Some(json!({
                "models": [
                    {"id": " gpt-5 "},
                    {"model": "o4-mini"},
                    {"id": "gpt-5"}
                ]
            })),
        );
        assert_eq!(client_catalog.provider_id, "codex");
        assert_eq!(
            client_catalog.models,
            vec!["gpt-5".to_string(), "o4-mini".to_string()]
        );
    }
}
