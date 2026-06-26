//! CC Switch ProviderRouter provider source.

use crate::database::Database;
use crate::error::AppError;
use crate::proxy::engine::routing::{ProviderFailoverRouterSources, ProviderRouterProviderSource};
use crate::proxy_core_adapter::{
    CcSwitchRoutePolicySource, ProviderSource, ProviderSpec, ProxyCoreAppKind as AppKind,
    ProxyCoreResult, app_type_from_proxy_core_app, current_provider_id_from_router_sources,
    failover_provider_ids_from_route_policy_source,
    provider_failover_sources_from_router_provider_source, provider_spec_from_db_source,
    provider_specs_from_db_source, select_current_provider_ids_from_router_provider_source,
};
use futures::future::BoxFuture;
use std::sync::Arc;

pub(crate) struct CcSwitchProviderRouterProviderSource {
    db: Arc<Database>,
    route_policies: CcSwitchRoutePolicySource,
}

impl CcSwitchProviderRouterProviderSource {
    pub(crate) fn new(db: Arc<Database>, route_policies: CcSwitchRoutePolicySource) -> Self {
        Self { db, route_policies }
    }
}

impl ProviderSource for CcSwitchProviderRouterProviderSource {
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
        Box::pin(async move {
            let app_type = app_type_from_proxy_core_app(app)?;
            Ok(current_provider_id_from_router_sources(
                app_type.as_str(),
                |app_enum| {
                    crate::settings::get_effective_current_provider(&self.db, app_enum)
                        .ok()
                        .flatten()
                },
                || {
                    self.db
                        .get_current_provider(app_type.as_str())
                        .ok()
                        .flatten()
                },
            ))
        })
    }
}

impl ProviderRouterProviderSource for CcSwitchProviderRouterProviderSource {
    fn failover_sources<'a>(
        &'a self,
        app_type: &'a str,
    ) -> BoxFuture<'a, Result<ProviderFailoverRouterSources, AppError>> {
        Box::pin(async move {
            let failover_provider_ids =
                failover_provider_ids_from_route_policy_source(&self.route_policies, app_type)
                    .await?;
            provider_failover_sources_from_router_provider_source(
                self,
                app_type,
                failover_provider_ids,
            )
            .await
        })
    }

    fn current_provider_ids<'a>(
        &'a self,
        app_type: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>, AppError>> {
        Box::pin(async move {
            select_current_provider_ids_from_router_provider_source(self, app_type).await
        })
    }
}
