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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::api::routing::DEFAULT_ROUTE_GROUP;
    use serde_json::json;
    use std::ffi::OsString;

    struct IsolatedTestHome {
        _dir: tempfile::TempDir,
        original_test_home: Option<OsString>,
    }

    impl IsolatedTestHome {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("temp home");
            let original_test_home = std::env::var_os("CC_SWITCH_TEST_HOME");
            std::env::set_var("CC_SWITCH_TEST_HOME", dir.path());
            Self {
                _dir: dir,
                original_test_home,
            }
        }
    }

    impl Drop for IsolatedTestHome {
        fn drop(&mut self) {
            match &self.original_test_home {
                Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

    #[tokio::test]
    async fn config_source_projects_proxy_configs_through_core() {
        let _home = IsolatedTestHome::new();
        let db = Arc::new(Database::memory().expect("memory db"));
        let source = CcSwitchConfigSource::new(db);

        let global = source.load_global().await.expect("load global config");
        assert_eq!(global.bind_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(global.bind_port, Some(15721));
        assert_eq!(global.raw["listenAddress"], json!("127.0.0.1"));

        let app = source
            .load_app(&AppKind::Claude)
            .await
            .expect("load app config");
        assert_eq!(app.app, Some(AppKind::Claude));
        assert_eq!(app.default_group.as_deref(), Some(DEFAULT_ROUTE_GROUP));
        assert_eq!(app.raw["appType"], json!("claude"));
        assert!(app.raw["currentProviderId"].is_null());
        assert!(app.rectifier.enabled);
        assert_eq!(app.rectifier.raw["enabled"], json!(true));
        assert_eq!(app.optimizer.raw["cacheTtl"], json!("1h"));
        assert_eq!(
            app.copilot_optimizer.raw["warmupModel"],
            json!("gpt-5-mini")
        );

        let summary = source
            .load_app_summary(&AppKind::Claude)
            .await
            .expect("load app summary config");
        assert_eq!(summary.enabled, app.enabled);
        assert_eq!(
            summary.auto_failover_enabled,
            app.raw["autoFailoverEnabled"].as_bool().unwrap_or_default()
        );

        let runtime = source.load_runtime().await.expect("load runtime config");
        assert!(!runtime.privacy_filter_enabled);
        assert!(runtime.route_events_enabled);
        assert_eq!(runtime.raw["enable_logging"], json!(true));
    }
}
