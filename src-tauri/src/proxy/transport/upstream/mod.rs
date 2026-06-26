pub(crate) mod hyper_client;
mod reqwest_client;

use crate::proxy::error::ProxyError;
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
use crate::proxy_core_adapter::{
    invalid_upstream_url_error_message, is_socks_proxy_url, resolve_upstream_send_policy,
    ForwarderUpstreamTransportRequest, UpstreamSendPolicyInput, UpstreamTransportKind,
};

pub(crate) async fn send_request(
    request: ForwarderUpstreamTransportRequest,
) -> Result<ProxyResponse, ProxyError> {
    let upstream_proxy_url: Option<String> = crate::proxy::http_client::get_current_proxy_url();
    let is_socks_proxy = is_socks_proxy_url(upstream_proxy_url.as_deref());
    let send_policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
        is_socks_proxy,
        preserve_exact_header_case: request.request_parts.preserve_exact_header_case,
        request_is_streaming: request.request_is_streaming,
        non_streaming_timeout: request.non_streaming_timeout,
        streaming_first_byte_timeout: request.streaming_first_byte_timeout,
    });

    if matches!(send_policy.transport, UpstreamTransportKind::PooledReqwest) {
        return reqwest_client::send_request(
            request,
            send_policy.reqwest_request_timeout,
            send_policy.streaming_header_timeout,
            is_socks_proxy,
        )
        .await;
    }

    let ForwarderUpstreamTransportRequest {
        method,
        url,
        request_parts,
        extensions,
        ..
    } = request;
    let uri: http::Uri = url.parse().map_err(|error| {
        ProxyError::ForwardFailed(invalid_upstream_url_error_message(&url, error))
    })?;

    hyper_client::send_request(
        uri,
        method,
        request_parts.ordered_headers,
        extensions,
        request_parts.body,
        send_policy.base_timeout,
        upstream_proxy_url.as_deref(),
    )
    .await
}
