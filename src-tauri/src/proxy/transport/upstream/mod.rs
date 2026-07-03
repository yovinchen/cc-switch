pub(crate) mod hyper_client;
mod reqwest_client;

use crate::proxy::engine::forward_pipeline::ForwarderUpstreamTransportRequest;
use crate::proxy::error::ProxyError;
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
use crate::proxy_core::api::transport::{
    invalid_upstream_url_error_message, is_socks_proxy_url, resolve_upstream_send_policy,
    ProxyCoreResponse, ProxyTransportResponse, ProxyTransportResponseBody,
    UpstreamSendPolicyInput, UpstreamTransportKind,
};
use bytes::Bytes;

pub(crate) fn proxy_core_response_to_proxy_response(
    response: ProxyCoreResponse,
) -> Result<ProxyResponse, ProxyError> {
    let response = response
        .into_transport_response()
        .map_err(ProxyError::Internal)?;
    let ProxyTransportResponse {
        status,
        headers,
        body,
    } = response;

    let response = match body {
        ProxyTransportResponseBody::Empty => ProxyResponse::buffered(status, headers, Bytes::new()),
        ProxyTransportResponseBody::Bytes(body) => ProxyResponse::buffered(status, headers, body),
        ProxyTransportResponseBody::Stream(stream) => {
            ProxyResponse::streamed(status, headers, stream)
        }
    };

    Ok(response)
}

pub(crate) async fn send_request(
    request: ForwarderUpstreamTransportRequest,
) -> Result<ProxyResponse, ProxyError> {
    let upstream_proxy_url: Option<String> =
        crate::proxy::host::cc_switch::global_http_client::get_current_proxy_url();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::api::transport::ProxyResponseBody;
    use http::StatusCode;

    #[tokio::test]
    async fn proxy_core_response_bridge_preserves_stream_body() {
        let response = ProxyCoreResponse::with_body(
            StatusCode::OK,
            http::HeaderMap::new(),
            ProxyResponseBody::stream(futures::stream::once(async {
                Ok(Bytes::from_static(b"chunk"))
            })),
        );

        let proxy_response = proxy_core_response_to_proxy_response(response).expect("bridge");

        assert_eq!(proxy_response.status(), StatusCode::OK);
        let body = proxy_response.bytes().await.expect("body");
        assert_eq!(body, Bytes::from_static(b"chunk"));
    }
}
