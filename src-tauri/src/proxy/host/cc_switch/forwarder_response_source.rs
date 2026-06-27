use futures::{future::BoxFuture, StreamExt};
use std::sync::Arc;

use crate::proxy::error::ProxyError;
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
use crate::proxy_core::api::transport::{
    apply_channel_response_header_overrides, non_streaming_body_timeout_message,
    resolve_channel_response_status_mapping, streaming_body_ended_before_first_chunk_message,
    streaming_body_first_chunk_read_error_message, streaming_body_first_chunk_timeout_message,
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
        let mut response = if let Some(status_mapping) = status_mapping {
            if status_mapping.changed() {
                log::debug!(
                    "[ChannelRoute] response status mapped via channel {}: {} -> {}",
                    channel.channel_id,
                    status_mapping.original_status.as_u16(),
                    status_mapping.mapped_status.as_u16()
                );
            }
            response.with_status(status_mapping.mapped_status)
        } else {
            response
        };

        let mut headers = response.headers().clone();
        if let Some(applied_headers) =
            apply_channel_response_header_overrides(&mut headers, &channel.response_overrides)
        {
            log::debug!(
                "[ChannelRoute] response headers overridden via channel {}: {:?}",
                channel.channel_id,
                applied_headers
            );
            response = response.with_headers(headers);
        }

        Ok(response)
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

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{HeaderMap, HeaderValue, StatusCode};
    use serde_json::json;

    use crate::proxy_core::api::routing::ResolvedChannelAttempt;

    #[test]
    fn channel_response_policy_maps_status_and_overrides_headers() {
        let source = CcSwitchForwarderResponseSource;
        let mut headers = HeaderMap::new();
        headers.insert("x-relay-tier", HeaderValue::from_static("old"));
        let response =
            ProxyResponse::buffered(StatusCode::TOO_MANY_REQUESTS, headers, Bytes::from("ok"));
        let channel = ResolvedChannelAttempt {
            channel_id: "channel-a".to_string(),
            channel_name: "Relay A".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_responses".to_string(),
            auth_profile_ref: None,
            public_model: Some("sonnet-public".to_string()),
            upstream_model: Some("upstream-sonnet".to_string()),
            pricing_model: None,
            header_overrides: json!({}),
            param_overrides: json!({}),
            status_code_mapping: json!([{"from": 429, "to": 200}]),
            request_overrides: json!({}),
            response_overrides: json!({
                "headers": {
                    "x-relay-tier": "paid",
                    "x-relay-model": "sonnet"
                }
            }),
            retry_policy: json!({}),
        };

        let response = source
            .apply_channel_response_status_mapping(ForwarderChannelResponseStatusInput {
                response,
                channel: Some(&channel),
            })
            .expect("channel response policy");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("x-relay-tier"),
            Some(&HeaderValue::from_static("paid"))
        );
        assert_eq!(
            response.headers().get("x-relay-model"),
            Some(&HeaderValue::from_static("sonnet"))
        );
    }
}
