//! CC Switch provider source.

use crate::database::Database;
use crate::error::AppError;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy_core::api::domain::{AppKind, ProviderSpec};
use crate::proxy_core::api::errors::{
    config_error_with_context as core_config_error_with_context, ProxyCoreError, ProxyCoreResult,
};
use crate::proxy_core::api::ports::{CurrentRouteTarget, ProviderSource};
use crate::proxy_core::api::routing::ProviderSelectionFailure;
use crate::proxy_core_adapter::{provider_spec_from_db_source, provider_specs_from_db_source};
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

fn provider_source_error(context: &str, error: AppError) -> ProxyCoreError {
    core_config_error_with_context(context, error)
}

fn current_provider_id_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<Option<String>> {
    db.get_current_provider(app.as_str())
        .map_err(|error| provider_source_error("get current provider", error))
}

async fn active_route_target_from_runtime_source(
    current_providers: &RwLock<HashMap<String, CurrentRouteTarget>>,
    app: &AppKind,
) -> ProxyCoreResult<Option<CurrentRouteTarget>> {
    let current_providers = current_providers.read().await;
    Ok(current_providers.get(app.as_str()).cloned())
}

fn provider_selection_failure_from_app_error(error: &AppError) -> Option<ProviderSelectionFailure> {
    match error {
        AppError::AllProvidersCircuitOpen => {
            Some(ProviderSelectionFailure::AllProvidersCircuitOpen)
        }
        AppError::NoProvidersConfigured => Some(ProviderSelectionFailure::NoProvidersConfigured),
        _ => None,
    }
}

fn route_candidate_provider_ids_from_router_result(
    result: Result<Vec<String>, AppError>,
) -> ProxyCoreResult<Vec<String>> {
    let selection_result = match result {
        Ok(provider_ids) => Ok(provider_ids),
        Err(error) => match provider_selection_failure_from_app_error(&error) {
            Some(failure) => Err(failure),
            None => {
                return Err(provider_source_error(
                    "select route candidate providers",
                    error,
                ))
            }
        },
    };
    Ok(
        crate::proxy_core::api::routing::route_candidate_provider_ids_from_selection_result(
            selection_result,
        ),
    )
}

async fn route_candidate_provider_ids_from_router_source(
    router: &ProviderRouter,
    app: &AppKind,
) -> ProxyCoreResult<Vec<String>> {
    route_candidate_provider_ids_from_router_result(router.select_provider_ids(app.as_str()).await)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_candidate_provider_ids_from_router_result_keeps_core_empty_policy() {
        assert_eq!(
            route_candidate_provider_ids_from_router_result(Ok(vec![
                "provider-a".to_string(),
                "provider-b".to_string()
            ]))
            .expect("candidate ids"),
            vec!["provider-a".to_string(), "provider-b".to_string()]
        );
        assert_eq!(
            route_candidate_provider_ids_from_router_result(Err(AppError::NoProvidersConfigured))
                .expect("empty no providers"),
            Vec::<String>::new()
        );
        assert_eq!(
            route_candidate_provider_ids_from_router_result(Err(AppError::AllProvidersCircuitOpen))
                .expect("empty circuit open"),
            Vec::<String>::new()
        );
        assert!(matches!(
            route_candidate_provider_ids_from_router_result(Err(AppError::Message(
                "router failed".to_string()
            ))),
            Err(ProxyCoreError::Config(message))
                if message == "select route candidate providers: router failed"
        ));
    }
}
