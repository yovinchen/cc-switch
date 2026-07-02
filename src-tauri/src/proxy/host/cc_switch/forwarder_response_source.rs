use futures::{future::BoxFuture, StreamExt};
use std::sync::Arc;

use crate::proxy::engine::forward_pipeline::{
    ForwarderChannelResponseStatusInput, ForwarderResponseFinalizationInput,
    ForwarderResponseSource, ForwarderResponseSourceRef,
};
use crate::proxy::error::ProxyError;
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
use crate::proxy_core::api::transport::{
    apply_channel_response_policy, non_streaming_body_timeout_message,
    streaming_body_ended_before_first_chunk_message, streaming_body_first_chunk_read_error_message,
    streaming_body_first_chunk_timeout_message, upstream_error_response_projection,
    upstream_success_response_finalization_plan, UpstreamSuccessResponseFinalizationPlan,
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

            match upstream_success_response_finalization_plan(
                request_is_streaming,
                non_streaming_timeout,
                streaming_first_byte_timeout,
            ) {
                UpstreamSuccessResponseFinalizationPlan::Passthrough => Ok(response),
                UpstreamSuccessResponseFinalizationPlan::PrimeStreaming { timeout } => {
                    prime_streaming_forward_response(response, timeout).await
                }
                UpstreamSuccessResponseFinalizationPlan::BufferNonStreaming { timeout } => {
                    let status = response.status();
                    let headers = response.headers().clone();
                    let body = tokio::time::timeout(timeout, response.bytes())
                        .await
                        .map_err(|_| {
                            ProxyError::Timeout(non_streaming_body_timeout_message(timeout))
                        })??;

                    Ok(ProxyResponse::buffered(status, headers, body))
                }
            }
        })
    }
}

impl ForwarderResponseSource for CcSwitchForwarderResponseSource {
    fn apply_channel_response_status_mapping(
        &self,
        input: ForwarderChannelResponseStatusInput<'_>,
    ) -> Result<ProxyResponse, ProxyError> {
        let ForwarderChannelResponseStatusInput {
            mut response,
            channel,
        } = input;
        let Some(channel) = channel else {
            return Ok(response);
        };

        let mut status = response.status();
        let mut headers = response.headers().clone();
        let policy = apply_channel_response_policy(
            &mut status,
            &mut headers,
            &channel.status_code_mapping,
            &channel.response_overrides,
        );

        if let Some(status_mapping) = policy.status_mapping {
            if status_mapping.changed() {
                log::debug!(
                    "[ChannelRoute] response status mapped via channel {}: {} -> {}",
                    channel.channel_id,
                    status_mapping.original_status.as_u16(),
                    status_mapping.mapped_status.as_u16()
                );
            }
            response = response.with_status(status);
        }

        if let Some(applied_headers) = policy.applied_headers {
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

            let status = input.response.status();
            let body = input.response.bytes().await?;
            let projection = upstream_error_response_projection(status, &body);
            Err(ProxyError::UpstreamError {
                status: projection.status,
                body: projection.body,
            })
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

    #[tokio::test]
    async fn upstream_error_projection_uses_core_body_policy() {
        let source = CcSwitchForwarderResponseSource;
        let response = ProxyResponse::buffered(
            StatusCode::BAD_GATEWAY,
            HeaderMap::new(),
            Bytes::from_static(b"{\"error\":\"bad_gateway\"}"),
        );

        let result = source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response,
                request_is_streaming: false,
                non_streaming_timeout: std::time::Duration::ZERO,
                streaming_first_byte_timeout: std::time::Duration::ZERO,
            })
            .await;

        match result {
            Err(ProxyError::UpstreamError { status, body }) => {
                assert_eq!(status, 502);
                assert_eq!(body.as_deref(), Some("{\"error\":\"bad_gateway\"}"));
            }
            Ok(_) => panic!("expected upstream error projection, got successful response"),
            Err(error) => panic!("expected upstream error projection, got {error:?}"),
        }
    }

    #[tokio::test]
    async fn success_response_finalization_buffers_non_streaming_body() {
        let source = CcSwitchForwarderResponseSource;
        let response =
            ProxyResponse::buffered(StatusCode::OK, HeaderMap::new(), Bytes::from_static(b"ok"));

        let result = source
            .finalize_upstream_response(ForwarderResponseFinalizationInput {
                response,
                request_is_streaming: false,
                non_streaming_timeout: std::time::Duration::from_secs(1),
                streaming_first_byte_timeout: std::time::Duration::ZERO,
            })
            .await
            .expect("success response");

        assert_eq!(result.status(), StatusCode::OK);
        assert_eq!(
            result.bytes().await.expect("body"),
            Bytes::from_static(b"ok")
        );
    }
}
