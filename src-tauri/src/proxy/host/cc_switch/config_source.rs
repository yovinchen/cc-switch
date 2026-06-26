//! CC Switch proxy config source.

use crate::database::Database;
use crate::proxy_core_adapter::{
    app_summary_config_from_db_source, cc_switch_app_kinds, proxy_app_config_from_db_source,
    proxy_global_config_from_db_source, proxy_runtime_config_from_db_source, AppSummaryConfig,
    ProxyAppConfig, ProxyConfigSource, ProxyCoreAppKind as AppKind, ProxyCoreResult,
    ProxyGlobalConfig, ProxyRuntimeConfig,
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

impl ProxyConfigSource for CcSwitchConfigSource {
    fn list_apps<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<Vec<AppKind>>> {
        Box::pin(async move { Ok(cc_switch_app_kinds()) })
    }

    fn load_global<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyGlobalConfig>> {
        Box::pin(async move { proxy_global_config_from_db_source(&self.db).await })
    }

    fn load_app<'a>(&'a self, app: &'a AppKind) -> BoxFuture<'a, ProxyCoreResult<ProxyAppConfig>> {
        Box::pin(async move { proxy_app_config_from_db_source(&self.db, app).await })
    }

    fn load_app_summary<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<AppSummaryConfig>> {
        Box::pin(async move { app_summary_config_from_db_source(&self.db, app).await })
    }

    fn load_runtime<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeConfig>> {
        Box::pin(async move { proxy_runtime_config_from_db_source(&self.db).await })
    }
}
