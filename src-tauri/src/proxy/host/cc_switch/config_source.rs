//! CC Switch proxy config source.

use crate::app_config::AppType;
use crate::database::Database;
use crate::proxy_core::api::config::{
    proxy_app_config_from_parts, proxy_global_config_from_global_config,
    proxy_runtime_config_from_proxy_config, AppProxyConfig, ProxyAppConfig, ProxyGlobalConfig,
    ProxyRuntimeConfig,
};
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::{config_error_with_context, ProxyCoreResult};
use crate::proxy_core::api::ports::{
    AppSummaryConfig, CopilotOptimizerConfig, OptimizerConfig, ProxyConfigSource, RectifierConfig,
};
use futures::future::BoxFuture;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct CcSwitchConfigSource {
    db: Arc<Database>,
}

impl CcSwitchConfigSource {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

fn cc_switch_app_kinds() -> Vec<AppKind> {
    AppType::all()
        .map(|app| AppKind::from(app.as_str()))
        .collect()
}

fn current_provider_id_from_settings_for_app(app: &AppKind) -> Option<String> {
    app.as_str()
        .parse::<AppType>()
        .ok()
        .as_ref()
        .and_then(crate::settings::get_current_provider)
}

fn proxy_app_config_from_config_source(
    app: AppKind,
    config: AppProxyConfig,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ProxyAppConfig {
    let current_provider_id = current_provider_id_from_settings_for_app(&app);
    proxy_app_config_from_parts(
        app,
        config,
        current_provider_id,
        rectifier,
        optimizer,
        copilot_optimizer,
    )
}

impl ProxyConfigSource for CcSwitchConfigSource {
    fn list_apps<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<Vec<AppKind>>> {
        Box::pin(async move { Ok(cc_switch_app_kinds()) })
    }

    fn load_global<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyGlobalConfig>> {
        Box::pin(async move {
            let config =
                self.db.get_global_proxy_config().await.map_err(|error| {
                    config_error_with_context("load global proxy config", error)
                })?;
            Ok(proxy_global_config_from_global_config(config))
        })
    }

    fn load_app<'a>(&'a self, app: &'a AppKind) -> BoxFuture<'a, ProxyCoreResult<ProxyAppConfig>> {
        Box::pin(async move {
            let config = self
                .db
                .get_proxy_config_for_app(app.as_str())
                .await
                .map_err(|error| config_error_with_context("load app proxy config", error))?;
            Ok(proxy_app_config_from_config_source(
                app.clone(),
                config,
                self.db.get_rectifier_config().unwrap_or_default(),
                self.db.get_optimizer_config().unwrap_or_default(),
                self.db.get_copilot_optimizer_config().unwrap_or_default(),
            ))
        })
    }

    fn load_app_summary<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<AppSummaryConfig>> {
        Box::pin(async move {
            let config = self
                .db
                .get_proxy_config_for_app(app.as_str())
                .await
                .map_err(|error| config_error_with_context("load app summary config", error))?;
            Ok(AppSummaryConfig::new(
                config.enabled,
                config.auto_failover_enabled,
            ))
        })
    }

    fn load_runtime<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeConfig>> {
        Box::pin(async move {
            let config =
                self.db.get_proxy_config().await.map_err(|error| {
                    config_error_with_context("load runtime proxy config", error)
                })?;
            Ok(proxy_runtime_config_from_proxy_config(config, false))
        })
    }
}
