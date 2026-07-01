//! CC Switch ProviderRouter provider source.

use crate::app_config::AppType;
use crate::database::Database;
use crate::error::AppError;
use crate::proxy::engine::routing::{ProviderFailoverRouterSources, ProviderRouterProviderSource};
use crate::proxy::host::cc_switch::provider_projection::{
    provider_spec_from_db_source, provider_specs_from_db_source,
};
use crate::proxy::host::cc_switch::route_policy_source::CcSwitchRoutePolicySource;
use crate::proxy_core::api::domain::{unsupported_app_kind_config_error, AppKind, ProviderSpec};
use crate::proxy_core::api::errors::{ProxyCoreError, ProxyCoreResult};
use crate::proxy_core::api::ports::{ProviderSource, RoutePolicySource};
use crate::proxy_core::api::routing::{
    current_provider_db_fallback_required, current_provider_id_option_from_sources,
    provider_failover_circuit_lookups, route_policy_failover_provider_ids, select_provider_ids,
    ProviderSelectionFailure, ProviderSelectionInput,
};
use crate::proxy_core::api::transport::{
    forwarder_all_providers_circuit_open_log_line, forwarder_no_providers_configured_log_line,
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

async fn provider_ids_from_router_provider_source(
    source: &(dyn ProviderSource + Send + Sync),
    app_type: &str,
) -> Result<Vec<String>, AppError> {
    let app = AppKind::from(app_type);
    let providers = source
        .list_providers(&app)
        .await
        .map_err(provider_router_provider_source_app_error)?;
    Ok(providers.into_iter().map(|provider| provider.id).collect())
}

async fn select_current_provider_ids_from_router_provider_source(
    source: &(dyn ProviderSource + Send + Sync),
    app_type: &str,
) -> Result<Vec<String>, AppError> {
    let app = AppKind::from(app_type);
    let current_provider_id = source
        .current_provider_id(&app)
        .await
        .map_err(provider_router_provider_source_app_error)?;
    let current_provider_id = match current_provider_id {
        Some(current_provider_id) => source
            .get_provider(&app, &current_provider_id)
            .await
            .map_err(provider_router_provider_source_app_error)?
            .map(|provider| provider.id),
        None => None,
    };

    select_current_provider_ids_from_router_provider_id_source(app_type, current_provider_id)
}

fn select_current_provider_ids_from_router_provider_id_source(
    app_type: &str,
    current_provider_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    let selected_ids =
        select_provider_ids(ProviderSelectionInput::current(current_provider_id.clone()))
            .map_err(|error| provider_selection_failure_app_error(app_type, error))?;

    Ok(selected_ids
        .into_iter()
        .filter(|provider_id| current_provider_id.as_ref() == Some(provider_id))
        .collect())
}

async fn failover_provider_ids_from_route_policy_source(
    source: &(dyn RoutePolicySource + Send + Sync),
    app_type: &str,
) -> Result<Vec<String>, AppError> {
    let app = AppKind::from(app_type);
    let policy = source
        .load_policy(&app)
        .await
        .map_err(provider_router_provider_source_app_error)?;
    Ok(policy
        .as_ref()
        .map(route_policy_failover_provider_ids)
        .unwrap_or_default())
}

async fn provider_failover_sources_from_router_provider_source(
    source: &(dyn ProviderSource + Send + Sync),
    app_type: &str,
    failover_provider_ids: impl IntoIterator<Item = String>,
) -> Result<ProviderFailoverRouterSources, AppError> {
    let provider_ids = provider_ids_from_router_provider_source(source, app_type).await?;
    let lookups = provider_failover_circuit_lookups(
        app_type,
        failover_provider_ids.into_iter().collect::<Vec<_>>(),
        provider_ids.clone(),
    );
    Ok(ProviderFailoverRouterSources {
        provider_ids,
        lookups,
    })
}

fn current_provider_id_from_router_sources(
    app_type: &str,
    load_settings_current_provider_id: impl FnOnce(&AppType) -> Option<String>,
    load_db_current_provider_id: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let settings_current_provider_id = app_type
        .parse::<AppType>()
        .ok()
        .and_then(|app| load_settings_current_provider_id(&app));
    let db_current_provider_id =
        if current_provider_db_fallback_required(settings_current_provider_id.as_deref()) {
            load_db_current_provider_id()
        } else {
            None
        };
    current_provider_id_option_from_sources(
        settings_current_provider_id.as_deref(),
        db_current_provider_id.as_deref(),
    )
}

fn provider_selection_failure_app_error(
    app_type: &str,
    error: ProviderSelectionFailure,
) -> AppError {
    match error {
        ProviderSelectionFailure::AllProvidersCircuitOpen => {
            log::warn!(
                "{}",
                forwarder_all_providers_circuit_open_log_line(app_type)
            );
            AppError::AllProvidersCircuitOpen
        }
        ProviderSelectionFailure::NoProvidersConfigured => {
            log::warn!("{}", forwarder_no_providers_configured_log_line(app_type));
            AppError::NoProvidersConfigured
        }
    }
}

fn provider_router_provider_source_app_error(error: ProxyCoreError) -> AppError {
    match error {
        ProxyCoreError::Config(message) => AppError::Config(message),
        ProxyCoreError::InvalidRequest(message) => AppError::InvalidInput(message),
        other => AppError::Message(other.to_string()),
    }
}

fn app_type_from_provider_source_app(app: &AppKind) -> ProxyCoreResult<AppType> {
    app.as_str()
        .parse::<AppType>()
        .map_err(unsupported_app_kind_config_error)
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
            let app_type = app_type_from_provider_source_app(app)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::api::domain::{ProviderKind, ProviderMetadata};
    use crate::proxy_core::api::routing::{route_policy_from_failover_provider_ids, RoutePolicy};

    struct StaticProviderSource {
        providers: Vec<ProviderSpec>,
        current_provider_id: Option<String>,
    }

    struct StaticRoutePolicySource {
        failover_provider_ids: Vec<String>,
    }

    impl ProviderSource for StaticProviderSource {
        fn list_providers<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Vec<ProviderSpec>>> {
            Box::pin(async move { Ok(self.providers.clone()) })
        }

        fn get_provider<'a>(
            &'a self,
            _app: &'a AppKind,
            provider_id: &'a str,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<ProviderSpec>>> {
            Box::pin(async move {
                Ok(self
                    .providers
                    .iter()
                    .find(|provider| provider.id == provider_id)
                    .cloned())
            })
        }

        fn current_provider_id<'a>(
            &'a self,
            _app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<String>>> {
            Box::pin(async move { Ok(self.current_provider_id.clone()) })
        }
    }

    impl RoutePolicySource for StaticRoutePolicySource {
        fn load_policy<'a>(
            &'a self,
            app: &'a AppKind,
        ) -> BoxFuture<'a, ProxyCoreResult<Option<RoutePolicy>>> {
            Box::pin(async move {
                Ok(Some(route_policy_from_failover_provider_ids(
                    app.clone(),
                    self.failover_provider_ids.clone(),
                )))
            })
        }
    }

    fn static_provider_spec(id: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: id.to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        }
    }

    #[test]
    fn current_provider_id_from_router_sources_prefers_settings_and_db_fallback() {
        let mut db_lookup_called_for_settings = false;
        assert_eq!(
            current_provider_id_from_router_sources(
                "claude",
                |app| {
                    assert_eq!(app, &AppType::Claude);
                    Some("settings-provider".to_string())
                },
                || {
                    db_lookup_called_for_settings = true;
                    Some("db-provider".to_string())
                },
            ),
            Some("settings-provider".to_string())
        );
        assert!(!db_lookup_called_for_settings);

        let mut settings_lookup_called_for_unknown = false;
        assert_eq!(
            current_provider_id_from_router_sources(
                "unknown-app",
                |_| {
                    settings_lookup_called_for_unknown = true;
                    Some("settings-provider".to_string())
                },
                || Some("db-provider".to_string()),
            ),
            Some("db-provider".to_string())
        );
        assert!(!settings_lookup_called_for_unknown);

        let mut db_lookup_called_for_empty = false;
        assert_eq!(
            current_provider_id_from_router_sources(
                "claude",
                |_| Some(String::new()),
                || {
                    db_lookup_called_for_empty = true;
                    Some("db-provider".to_string())
                },
            ),
            Some(String::new())
        );
        assert!(!db_lookup_called_for_empty);
    }

    #[tokio::test]
    async fn provider_router_provider_source_projects_core_provider_source() {
        let source = StaticProviderSource {
            providers: vec![
                static_provider_spec("provider-a"),
                static_provider_spec("provider-b"),
            ],
            current_provider_id: Some("provider-a".to_string()),
        };

        let provider_ids = provider_ids_from_router_provider_source(&source, "claude")
            .await
            .expect("provider ids");
        assert_eq!(provider_ids, vec!["provider-a", "provider-b"]);

        let current_ids =
            select_current_provider_ids_from_router_provider_source(&source, "claude")
                .await
                .expect("current provider ids");
        assert_eq!(current_ids, vec!["provider-a"]);

        let no_current_source = StaticProviderSource {
            providers: source.providers.clone(),
            current_provider_id: None,
        };
        assert!(matches!(
            select_current_provider_ids_from_router_provider_source(&no_current_source, "claude")
                .await,
            Err(AppError::NoProvidersConfigured)
        ));

        let route_policy_source = StaticRoutePolicySource {
            failover_provider_ids: vec!["provider-b".to_string(), "missing".to_string()],
        };
        let failover_provider_ids =
            failover_provider_ids_from_route_policy_source(&route_policy_source, "claude")
                .await
                .expect("failover provider ids");
        assert_eq!(failover_provider_ids, vec!["provider-b", "missing"]);

        let failover_sources = provider_failover_sources_from_router_provider_source(
            &source,
            "claude",
            failover_provider_ids,
        )
        .await
        .expect("failover sources");
        assert_eq!(
            failover_sources.provider_ids,
            vec!["provider-a", "provider-b"]
        );
        assert_eq!(failover_sources.lookups[0].provider_id, "provider-b");
        assert!(failover_sources.lookups[0].configured);
        assert_eq!(failover_sources.lookups[1].provider_id, "missing");
        assert!(!failover_sources.lookups[1].configured);
    }
}
