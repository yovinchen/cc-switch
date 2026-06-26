//! CC Switch management API auth source.

use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::{
    ManagementAuthRuntimeConfig, ManagementAuthSource, ProxyConfig,
};
use futures::future::BoxFuture;
use std::sync::Arc;
use tokio::sync::RwLock;

const PROXY_MANAGEMENT_AUTH_TOKEN_ENV: &str = "CC_SWITCH_PROXY_MANAGEMENT_TOKEN";

#[derive(Clone)]
pub(crate) struct CcSwitchManagementAuthSource {
    config: Arc<RwLock<ProxyConfig>>,
}

impl CcSwitchManagementAuthSource {
    pub(crate) fn new(config: Arc<RwLock<ProxyConfig>>) -> Self {
        Self { config }
    }
}

impl ManagementAuthSource for CcSwitchManagementAuthSource {
    fn load_management_auth_config<'a>(
        &'a self,
    ) -> BoxFuture<'a, ProxyCoreResult<ManagementAuthRuntimeConfig>> {
        Box::pin(async move {
            let config = self.config.read().await;
            Ok(ManagementAuthRuntimeConfig::new(
                config.listen_address.clone(),
                config.management_auth_token.clone(),
                std::env::var(PROXY_MANAGEMENT_AUTH_TOKEN_ENV).ok(),
            ))
        })
    }
}
