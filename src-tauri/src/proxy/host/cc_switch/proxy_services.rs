//! CC Switch implementation of the proxy core service container.

#[cfg(test)]
use crate::database::Database;
#[cfg(test)]
use crate::proxy::events::ProxyEventBus;
use crate::proxy::host::cc_switch::auth_provider::CcSwitchAuthProvider;
use crate::proxy::host::cc_switch::channel_health_store::CcSwitchChannelHealthStore;
use crate::proxy::host::cc_switch::channel_key_runtime_source::{
    CcSwitchChannelKeyRuntimeSource, channel_key_runtime_source_from_database,
};
use crate::proxy::host::cc_switch::channel_reachability_probe::CcSwitchChannelReachabilityProbe;
use crate::proxy::host::cc_switch::claude_desktop_gateway_auth_source::CcSwitchClaudeDesktopGatewayAuthSource;
use crate::proxy::host::cc_switch::config_source::CcSwitchConfigSource;
use crate::proxy::host::cc_switch::database_channel_source::CcSwitchChannelSource;
use crate::proxy::host::cc_switch::database_usage_sink::CcSwitchUsageSink;
use crate::proxy::host::cc_switch::event_sink::CcSwitchEventSink;
use crate::proxy::host::cc_switch::forward_pipeline::CcSwitchForwardPipeline;
use crate::proxy::host::cc_switch::management_auth_source::CcSwitchManagementAuthSource;
use crate::proxy::host::cc_switch::model_catalog_provider::CcSwitchModelCatalogProvider;
#[cfg(test)]
use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
use crate::proxy::host::cc_switch::provider_source::CcSwitchProviderSource;
use crate::proxy::host::cc_switch::route_policy_source::CcSwitchRoutePolicySource;
use crate::proxy::host::cc_switch::route_resolver::CcSwitchRouteResolver;
use crate::proxy::host::cc_switch::runtime_status_source::CcSwitchRuntimeStatusSource;
use crate::proxy_core_adapter::{
    AuthProvider, ChannelHealthStore, ChannelKeyRuntimeSource, ChannelReachabilityProbe,
    ChannelSource, ClaudeDesktopGatewayAuthSource, ForwardPipeline, HostForwardRuntime,
    ManagementAuthSource, ModelCatalogProvider, ProviderSource, ProxyEventSink,
    ProxyServiceRuntimeResources, ProxyServices, RoutePolicySource, RouteResolver,
    RuntimeStatusSource, UsageSink,
};
#[cfg(test)]
use crate::proxy_core_adapter::{ProxyConfig, ProxyCoreResult, ProxyRuntimeStatus};
#[cfg(test)]
use futures::future::BoxFuture;
#[cfg(test)]
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::RwLock;

#[cfg(test)]
#[derive(Clone, Default)]
struct DefaultRuntimeStatusSource;

#[cfg(test)]
impl RuntimeStatusSource for DefaultRuntimeStatusSource {
    fn load_status<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeStatus>> {
        Box::pin(async { Ok(ProxyRuntimeStatus::default()) })
    }
}

#[derive(Clone)]
pub(crate) struct CcSwitchProxyServices<R> {
    config: CcSwitchConfigSource,
    providers: CcSwitchProviderSource,
    channels: CcSwitchChannelSource,
    route_policies: CcSwitchRoutePolicySource,
    route_resolver: CcSwitchRouteResolver,
    health_store: CcSwitchChannelHealthStore,
    reachability_probe: CcSwitchChannelReachabilityProbe,
    auth_provider: CcSwitchAuthProvider,
    claude_desktop_gateway_auth_source: CcSwitchClaudeDesktopGatewayAuthSource,
    management_auth_source: CcSwitchManagementAuthSource,
    channel_key_runtime_source: CcSwitchChannelKeyRuntimeSource,
    model_catalog: CcSwitchModelCatalogProvider,
    runtime_status_source: Arc<dyn RuntimeStatusSource + Send + Sync>,
    usage_sink: CcSwitchUsageSink,
    event_sink: CcSwitchEventSink,
    forward_pipeline: CcSwitchForwardPipeline<R>,
}

impl<R> CcSwitchProxyServices<R> {
    #[cfg(test)]
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self::with_optional_event_bus(db, None)
    }

    #[cfg(test)]
    pub(crate) fn with_event_bus(db: Arc<Database>, events: Arc<ProxyEventBus>) -> Self {
        Self::with_optional_event_bus(db, Some(events))
    }

    #[cfg(test)]
    fn with_optional_event_bus(db: Arc<Database>, events: Option<Arc<ProxyEventBus>>) -> Self {
        let router = Arc::new(provider_router_from_database(db.clone()));
        let channel_key_runtime_source = channel_key_runtime_source_from_database(db.clone());
        Self {
            config: CcSwitchConfigSource::new(db.clone()),
            providers: CcSwitchProviderSource::new(
                db.clone(),
                router.clone(),
                Arc::new(RwLock::new(HashMap::new())),
            ),
            channels: CcSwitchChannelSource::new(db.clone()),
            route_policies: CcSwitchRoutePolicySource::new(db.clone()),
            route_resolver: CcSwitchRouteResolver::new(router.clone()),
            health_store: CcSwitchChannelHealthStore::new(db.clone(), router.clone()),
            reachability_probe: CcSwitchChannelReachabilityProbe::new(db.clone()),
            auth_provider: CcSwitchAuthProvider,
            claude_desktop_gateway_auth_source: CcSwitchClaudeDesktopGatewayAuthSource::new(
                db.clone(),
            ),
            management_auth_source: CcSwitchManagementAuthSource::new(Arc::new(RwLock::new(
                ProxyConfig::default(),
            ))),
            channel_key_runtime_source: channel_key_runtime_source.clone(),
            model_catalog: CcSwitchModelCatalogProvider::new(db.clone(), router.clone()),
            runtime_status_source: Arc::new(DefaultRuntimeStatusSource),
            usage_sink: CcSwitchUsageSink::new(db.clone()),
            event_sink: CcSwitchEventSink::new(events),
            forward_pipeline: CcSwitchForwardPipeline::without_runtime(channel_key_runtime_source),
        }
    }
}

impl<R> CcSwitchProxyServices<R>
where
    R: ProxyServiceRuntimeResources,
{
    pub(crate) fn with_runtime(runtime: R) -> Self {
        let db = runtime.db();
        let provider_router = runtime.provider_router();
        let channel_key_runtime_source = channel_key_runtime_source_from_database(db.clone());
        Self {
            config: CcSwitchConfigSource::new(db.clone()),
            providers: CcSwitchProviderSource::new(
                db.clone(),
                provider_router.clone(),
                runtime.current_providers(),
            ),
            channels: CcSwitchChannelSource::new(db.clone()),
            route_policies: CcSwitchRoutePolicySource::new(db.clone()),
            route_resolver: CcSwitchRouteResolver::new(provider_router.clone()),
            health_store: CcSwitchChannelHealthStore::new(db.clone(), provider_router.clone()),
            reachability_probe: CcSwitchChannelReachabilityProbe::new(db.clone()),
            auth_provider: CcSwitchAuthProvider,
            claude_desktop_gateway_auth_source: CcSwitchClaudeDesktopGatewayAuthSource::new(
                db.clone(),
            ),
            management_auth_source: CcSwitchManagementAuthSource::new(runtime.config()),
            channel_key_runtime_source: channel_key_runtime_source.clone(),
            model_catalog: CcSwitchModelCatalogProvider::new(db.clone(), provider_router.clone()),
            runtime_status_source: Arc::new(CcSwitchRuntimeStatusSource::new(
                runtime.status(),
                runtime.start_time(),
                runtime.current_providers(),
            )),
            usage_sink: CcSwitchUsageSink::new(db),
            event_sink: CcSwitchEventSink::new(Some(runtime.events())),
            forward_pipeline: CcSwitchForwardPipeline::with_runtime(
                runtime,
                channel_key_runtime_source,
            ),
        }
    }
}

impl<R> ProxyServices for CcSwitchProxyServices<R>
where
    R: HostForwardRuntime + Send + Sync + 'static,
{
    fn config(&self) -> &(dyn crate::proxy_core_adapter::ProxyConfigSource + Send + Sync) {
        &self.config
    }

    fn providers(&self) -> &(dyn ProviderSource + Send + Sync) {
        &self.providers
    }

    fn channels(&self) -> &(dyn ChannelSource + Send + Sync) {
        &self.channels
    }

    fn route_policies(&self) -> &(dyn RoutePolicySource + Send + Sync) {
        &self.route_policies
    }

    fn route_resolver(&self) -> &(dyn RouteResolver + Send + Sync) {
        &self.route_resolver
    }

    fn health_store(&self) -> &(dyn ChannelHealthStore + Send + Sync) {
        &self.health_store
    }

    fn reachability_probe(&self) -> &(dyn ChannelReachabilityProbe + Send + Sync) {
        &self.reachability_probe
    }

    fn auth_provider(&self) -> &(dyn AuthProvider + Send + Sync) {
        &self.auth_provider
    }

    fn claude_desktop_gateway_auth_source(
        &self,
    ) -> &(dyn ClaudeDesktopGatewayAuthSource + Send + Sync) {
        &self.claude_desktop_gateway_auth_source
    }

    fn management_auth_source(&self) -> &(dyn ManagementAuthSource + Send + Sync) {
        &self.management_auth_source
    }

    fn channel_key_runtime_source(&self) -> &(dyn ChannelKeyRuntimeSource + Send + Sync) {
        &self.channel_key_runtime_source
    }

    fn model_catalog(&self) -> &(dyn ModelCatalogProvider + Send + Sync) {
        &self.model_catalog
    }

    fn runtime_status_source(&self) -> &(dyn RuntimeStatusSource + Send + Sync) {
        self.runtime_status_source.as_ref()
    }

    fn usage_sink(&self) -> &(dyn UsageSink + Send + Sync) {
        &self.usage_sink
    }

    fn event_sink(&self) -> &(dyn ProxyEventSink + Send + Sync) {
        &self.event_sink
    }

    fn forward_pipeline(&self) -> &(dyn ForwardPipeline + Send + Sync) {
        &self.forward_pipeline
    }
}
