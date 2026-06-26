use std::time::Duration;

use crate::proxy::error::ProxyError;
use crate::proxy::error_mapper::reqwest_send_error_to_proxy_error;
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
use crate::proxy_core_adapter::{
    streaming_header_timeout_message, ForwarderUpstreamTransportRequest,
};

pub(crate) async fn send_request(
    request: ForwarderUpstreamTransportRequest,
    reqwest_request_timeout: Option<Duration>,
    streaming_header_timeout: Option<Duration>,
    is_socks_proxy: bool,
) -> Result<ProxyResponse, ProxyError> {
    log::debug!(
        "[Forwarder] Using pooled reqwest client (preserve_exact_header_case={}, socks_proxy={})",
        request.request_parts.preserve_exact_header_case,
        is_socks_proxy
    );

    let client = crate::proxy::http_client::get();
    let mut outbound = client.request(request.method, &request.url);
    if let Some(request_timeout) = reqwest_request_timeout {
        outbound = outbound.timeout(request_timeout);
    }
    for (key, value) in &request.request_parts.ordered_headers {
        outbound = outbound.header(key, value);
    }
    let send = outbound.body(request.request_parts.body).send();
    let send_result = if let Some(header_timeout) = streaming_header_timeout {
        tokio::time::timeout(header_timeout, send)
            .await
            .map_err(|_| ProxyError::Timeout(streaming_header_timeout_message(header_timeout)))?
    } else {
        send.await
    };
    let reqwest_resp = send_result.map_err(reqwest_send_error_to_proxy_error)?;
    Ok(ProxyResponse::Reqwest(reqwest_resp))
}
