//! CC Switch forward pipeline adapter.

use crate::proxy::host::cc_switch::channel_key_runtime_source::CcSwitchChannelKeyRuntimeSource;
use crate::proxy_core_adapter::{
    ForwardPipeline, HostForwardRuntime, ProxyCoreResult, ProxyRequest, ProxyResult, RoutePlan,
    forward_with_optional_host_runtime,
};
use futures::future::BoxFuture;

#[derive(Clone)]
pub(crate) struct CcSwitchForwardPipeline<R> {
    runtime: Option<R>,
    channel_key_runtime_source: CcSwitchChannelKeyRuntimeSource,
}

impl<R> CcSwitchForwardPipeline<R> {
    #[cfg(test)]
    pub(crate) fn without_runtime(
        channel_key_runtime_source: CcSwitchChannelKeyRuntimeSource,
    ) -> Self {
        Self {
            runtime: None,
            channel_key_runtime_source,
        }
    }

    pub(crate) fn with_runtime(
        runtime: R,
        channel_key_runtime_source: CcSwitchChannelKeyRuntimeSource,
    ) -> Self {
        Self {
            runtime: Some(runtime),
            channel_key_runtime_source,
        }
    }
}

impl<R> ForwardPipeline for CcSwitchForwardPipeline<R>
where
    R: HostForwardRuntime + Send + Sync,
{
    fn forward<'a>(
        &'a self,
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>> {
        forward_with_optional_host_runtime(
            self.runtime.as_ref(),
            &self.channel_key_runtime_source,
            request,
            plan,
        )
    }
}
