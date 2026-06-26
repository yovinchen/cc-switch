//! CC Switch proxy runtime resources.

use crate::database::Database;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy::events::ProxyEventBus;
use crate::proxy_core::api::domain::ProxyRequest;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::{
    ChannelKeyRuntimeSource, CurrentRouteTarget, ProxyConfig, ProxyRuntimeStatus,
};
use crate::proxy_core::api::routing::RoutePlan;
use crate::proxy_core::api::transport::ProxyResult;
use crate::proxy_core_adapter::{
    forward_proxy_request_with_cc_switch_runtime, FailoverSwitchSchedulerRef,
    ForwarderAttemptRuntimeSourceRef, ForwarderAuthSourceRef, ForwarderProtocolStateSourceRef,
    ForwarderRequestSourceRef, ForwarderResponseSourceRef, ForwarderRuntimeStateSourceRef,
    ForwarderTransportSourceRef,
};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub(crate) trait ProxyServiceRuntimeResources:
    HostForwardRuntime + Clone + Send + Sync
{
    fn db(&self) -> Arc<Database>;
    fn config(&self) -> Arc<RwLock<ProxyConfig>>;
    fn provider_router(&self) -> Arc<ProviderRouter>;
    fn status(&self) -> Arc<RwLock<ProxyRuntimeStatus>>;
    fn start_time(&self) -> Arc<RwLock<Option<std::time::Instant>>>;
    fn current_providers(&self) -> Arc<RwLock<HashMap<String, CurrentRouteTarget>>>;
    fn events(&self) -> Arc<ProxyEventBus>;
}

pub(crate) trait HostForwardRuntime {
    fn forward_host<'a>(
        &'a self,
        channel_key_runtime_source: &'a (dyn ChannelKeyRuntimeSource + Send + Sync),
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>>;
}

#[derive(Clone)]
pub(crate) struct CcSwitchProxyRuntime {
    pub(crate) db: Arc<Database>,
    pub(crate) config: Arc<RwLock<ProxyConfig>>,
    pub(crate) provider_router: Arc<ProviderRouter>,
    pub(crate) status: Arc<RwLock<ProxyRuntimeStatus>>,
    pub(crate) start_time: Arc<RwLock<Option<std::time::Instant>>>,
    pub(crate) events: Arc<ProxyEventBus>,
    pub(crate) current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    pub(crate) attempt_runtime_source: ForwarderAttemptRuntimeSourceRef,
    pub(crate) protocol_state_source: ForwarderProtocolStateSourceRef,
    pub(crate) runtime_state_source: ForwarderRuntimeStateSourceRef,
    pub(crate) auth_source: ForwarderAuthSourceRef,
    pub(crate) request_source: ForwarderRequestSourceRef,
    pub(crate) transport_source: ForwarderTransportSourceRef,
    pub(crate) response_source: ForwarderResponseSourceRef,
    pub(crate) failover_switch_scheduler: FailoverSwitchSchedulerRef,
}

impl ProxyServiceRuntimeResources for CcSwitchProxyRuntime {
    fn db(&self) -> Arc<Database> {
        self.db.clone()
    }

    fn config(&self) -> Arc<RwLock<ProxyConfig>> {
        self.config.clone()
    }

    fn provider_router(&self) -> Arc<ProviderRouter> {
        self.provider_router.clone()
    }

    fn status(&self) -> Arc<RwLock<ProxyRuntimeStatus>> {
        self.status.clone()
    }

    fn start_time(&self) -> Arc<RwLock<Option<std::time::Instant>>> {
        self.start_time.clone()
    }

    fn current_providers(&self) -> Arc<RwLock<HashMap<String, CurrentRouteTarget>>> {
        self.current_providers.clone()
    }

    fn events(&self) -> Arc<ProxyEventBus> {
        self.events.clone()
    }
}

impl HostForwardRuntime for CcSwitchProxyRuntime {
    fn forward_host<'a>(
        &'a self,
        channel_key_runtime_source: &'a (dyn ChannelKeyRuntimeSource + Send + Sync),
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>> {
        Box::pin(async move {
            forward_proxy_request_with_cc_switch_runtime(
                self,
                channel_key_runtime_source,
                request,
                plan,
            )
            .await
        })
    }
}
