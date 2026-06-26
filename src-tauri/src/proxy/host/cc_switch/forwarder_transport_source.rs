use futures::future::BoxFuture;
use std::sync::Arc;

use crate::proxy::error::ProxyError;
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
use crate::proxy::transport::upstream::send_request;
use crate::proxy_core_adapter::{
    ForwarderTransportSource, ForwarderTransportSourceRef, ForwarderUpstreamTransportRequest,
};

struct CcSwitchForwarderTransportSource;

impl ForwarderTransportSource for CcSwitchForwarderTransportSource {
    fn send_upstream_request<'a>(
        &'a self,
        request: ForwarderUpstreamTransportRequest,
    ) -> BoxFuture<'a, Result<ProxyResponse, ProxyError>> {
        Box::pin(async move { send_request(request).await })
    }
}

pub(crate) fn default_forwarder_transport_source() -> ForwarderTransportSourceRef {
    Arc::new(CcSwitchForwarderTransportSource)
}
