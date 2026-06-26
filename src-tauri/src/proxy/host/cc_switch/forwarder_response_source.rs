use futures::{future::BoxFuture, StreamExt};
use std::sync::Arc;

use crate::proxy::error::ProxyError;
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
use crate::proxy_core::api::transport::{
    non_streaming_body_timeout_message, resolve_channel_response_status_mapping,
    streaming_body_ended_before_first_chunk_message, streaming_body_first_chunk_read_error_message,
    streaming_body_first_chunk_timeout_message,
};
use crate::proxy_core_adapter::{
    ForwarderChannelResponseStatusInput, ForwarderResponseFinalizationInput,
    ForwarderResponseSource, ForwarderResponseSourceRef,
};

pub(crate) struct CcSwitchForwarderResponseSource;

impl CcSwitchForwarderResponseSource {
    fn prepare_success_response<'a>(
        &'a self,
        input: ForwarderResponseFinalizationInput,
    ) -> BoxFuture<'a, Result<ProxyResponse, ProxyError>> {
        Box::pin(async move {
            let ForwarderResponseFinalizationInput {
                response,
                request_is_streaming,
                non_streaming_timeout,
                streaming_first_byte_timeout,
            } = input;

            if request_is_streaming {
                return prime_streaming_forward_response(response, streaming_first_byte_timeout)
                    .await;
            }

            if non_streaming_timeout.is_zero() {
                return Ok(response);
            }

            let status = response.status();
            let headers = response.headers().clone();
            let body = tokio::time::timeout(non_streaming_timeout, response.bytes())
                .await
                .map_err(|_| {
                    ProxyError::Timeout(non_streaming_body_timeout_message(non_streaming_timeout))
                })??;

            Ok(ProxyResponse::buffered(status, headers, body))
        })
    }
}

impl ForwarderResponseSource for CcSwitchForwarderResponseSource {
    fn apply_channel_response_status_mapping(
        &self,
        input: ForwarderChannelResponseStatusInput<'_>,
    ) -> Result<ProxyResponse, ProxyError> {
        let ForwarderChannelResponseStatusInput { response, channel } = input;
        let Some(channel) = channel else {
            return Ok(response);
        };

        let status_mapping = resolve_channel_response_status_mapping(
            response.status(),
            &channel.status_code_mapping,
        );
        let Some(status_mapping) = status_mapping else {
            return Ok(response);
        };

        if status_mapping.changed() {
            log::debug!(
                "[ChannelRoute] response status mapped via channel {}: {} -> {}",
                channel.channel_id,
                status_mapping.original_status.as_u16(),
                status_mapping.mapped_status.as_u16()
            );
        }

        Ok(response.with_status(status_mapping.mapped_status))
    }

    fn finalize_upstream_response<'a>(
        &'a self,
        input: ForwarderResponseFinalizationInput,
    ) -> BoxFuture<'a, Result<ProxyResponse, ProxyError>> {
        Box::pin(async move {
            if input.response.status().is_success() {
                return self.prepare_success_response(input).await;
            }

            let status = input.response.status().as_u16();
            let body = String::from_utf8(input.response.bytes().await?.to_vec()).ok();
            Err(ProxyError::UpstreamError { status, body })
        })
    }
}

async fn prime_streaming_forward_response(
    response: ProxyResponse,
    timeout: std::time::Duration,
) -> Result<ProxyResponse, ProxyError> {
    if timeout.is_zero() {
        return Ok(response);
    }

    let status = response.status();
    let headers = response.headers().clone();
    let mut stream = Box::pin(response.bytes_stream());

    let first = tokio::time::timeout(timeout, stream.next())
        .await
        .map_err(|_| ProxyError::Timeout(streaming_body_first_chunk_timeout_message(timeout)))?;

    let Some(first) = first else {
        return Err(ProxyError::ForwardFailed(
            streaming_body_ended_before_first_chunk_message().to_string(),
        ));
    };

    let first = first
        .map_err(|e| ProxyError::ForwardFailed(streaming_body_first_chunk_read_error_message(e)))?;

    let replay = futures::stream::once(async move { Ok(first) }).chain(stream);
    Ok(ProxyResponse::streamed(status, headers, replay))
}

pub(crate) fn default_forwarder_response_source() -> ForwarderResponseSourceRef {
    Arc::new(CcSwitchForwarderResponseSource)
}
