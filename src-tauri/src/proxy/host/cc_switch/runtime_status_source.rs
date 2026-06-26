//! CC Switch proxy runtime status source.

use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::{
    apply_proxy_runtime_active_targets, apply_proxy_runtime_uptime, CurrentRouteTarget,
    ProxyRuntimeStatus, RuntimeStatusSource,
};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub(crate) async fn proxy_runtime_status_from_runtime_sources(
    status: &RwLock<ProxyRuntimeStatus>,
    start_time: &RwLock<Option<Instant>>,
    current_providers: &RwLock<HashMap<String, CurrentRouteTarget>>,
) -> ProxyRuntimeStatus {
    let mut status = status.read().await.clone();

    if let Some(start) = start_time.read().await.as_ref().copied() {
        apply_proxy_runtime_uptime(&mut status, start.elapsed().as_secs());
    }

    let current_providers = current_providers.read().await;
    apply_proxy_runtime_active_targets(&mut status, current_providers.values().cloned());

    status
}

#[derive(Clone)]
pub(crate) struct CcSwitchRuntimeStatusSource {
    status: Arc<RwLock<ProxyRuntimeStatus>>,
    start_time: Arc<RwLock<Option<Instant>>>,
    current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
}

impl CcSwitchRuntimeStatusSource {
    pub(crate) fn new(
        status: Arc<RwLock<ProxyRuntimeStatus>>,
        start_time: Arc<RwLock<Option<Instant>>>,
        current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    ) -> Self {
        Self {
            status,
            start_time,
            current_providers,
        }
    }
}

impl RuntimeStatusSource for CcSwitchRuntimeStatusSource {
    fn load_status<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeStatus>> {
        Box::pin(async move {
            Ok(proxy_runtime_status_from_runtime_sources(
                &self.status,
                &self.start_time,
                &self.current_providers,
            )
            .await)
        })
    }
}
