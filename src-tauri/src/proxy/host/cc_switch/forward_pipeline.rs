//! CC Switch forward pipeline adapter.

use crate::proxy::host::cc_switch::channel_key_runtime_source::CcSwitchChannelKeyRuntimeSource;
use crate::proxy::host::cc_switch::proxy_runtime::HostForwardRuntime;
use crate::proxy_core::api::domain::ProxyRequest;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::{ChannelKeyRuntimeSource, ForwardPipeline};
use crate::proxy_core::api::routing::{forwarding_requires_runtime_error, RoutePlan};
use crate::proxy_core::api::transport::ProxyResult;
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

fn forward_with_optional_host_runtime<'a, R>(
    runtime: Option<&'a R>,
    channel_key_runtime_source: &'a (dyn ChannelKeyRuntimeSource + Send + Sync),
    request: ProxyRequest,
    plan: RoutePlan,
) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>>
where
    R: HostForwardRuntime + Sync + 'a,
{
    Box::pin(async move {
        let runtime = runtime.ok_or_else(forwarding_requires_runtime_error)?;
        runtime
            .forward_host(channel_key_runtime_source, request, plan)
            .await
    })
}
