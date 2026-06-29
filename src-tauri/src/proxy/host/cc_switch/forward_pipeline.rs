//! CC Switch forward pipeline adapter.

use crate::provider::Provider;
use crate::proxy::host::cc_switch::channel_key_runtime_source::CcSwitchChannelKeyRuntimeSource;
use crate::proxy::host::cc_switch::proxy_runtime::HostForwardRuntime;
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
use crate::proxy::ForwardResult;
use crate::proxy_core::api::domain::ProxyRequest;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::{ChannelKeyRuntimeSource, ForwardPipeline};
use crate::proxy_core::api::routing::{
    forwarding_requires_runtime_error, select_route_for_forward_result, RoutePlan,
};
use crate::proxy_core::api::transforms::CLAUDE_API_FORMAT_METADATA_KEY;
use crate::proxy_core::api::transport::{ProxyCoreResponse, ProxyResponseBody, ProxyResult};
use bytes::Bytes;
use futures::{future::BoxFuture, Stream, StreamExt};
use serde_json::{json, Map, Value};

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

pub(crate) fn proxy_response_to_core_response<G>(
    response: ProxyResponse,
    connection_guard: Option<G>,
) -> ProxyCoreResponse
where
    G: Send + 'static,
{
    match response {
        ProxyResponse::Buffered {
            status,
            headers,
            body,
        } => ProxyCoreResponse::with_body(status, headers, ProxyResponseBody::bytes(body)),
        ProxyResponse::Streamed {
            status,
            headers,
            stream,
        } => ProxyCoreResponse::with_body(
            status,
            headers,
            ProxyResponseBody::stream(stream_with_connection_guard(stream, connection_guard)),
        ),
        other => {
            let status = other.status();
            let headers = other.headers().clone();
            ProxyCoreResponse::with_body(
                status,
                headers,
                ProxyResponseBody::stream(stream_with_connection_guard(
                    other.bytes_stream(),
                    connection_guard,
                )),
            )
        }
    }
}

pub(crate) fn forward_result_to_proxy_result(
    result: ForwardResult,
    plan: RoutePlan,
) -> ProxyResult {
    let ForwardResult {
        response,
        provider,
        claude_api_format,
        outbound_model,
        selected_channel,
        connection_guard,
    } = result;
    let selected_channel_id = selected_channel
        .as_ref()
        .map(|channel| channel.channel_id.as_str());
    let response = proxy_response_to_core_response(response, connection_guard);

    proxy_result_from_forward_parts(
        response,
        plan,
        &provider,
        claude_api_format,
        outbound_model,
        selected_channel_id,
    )
}

fn stream_with_connection_guard<S, G>(
    stream: S,
    connection_guard: Option<G>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    G: Send + 'static,
{
    async_stream::stream! {
        let _connection_guard = connection_guard;
        tokio::pin!(stream);
        while let Some(chunk) = stream.next().await {
            yield chunk;
        }
    }
}

pub(crate) fn proxy_result_from_forward_parts(
    response: ProxyCoreResponse,
    plan: RoutePlan,
    provider: &Provider,
    claude_api_format: Option<String>,
    outbound_model: Option<String>,
    selected_channel_id: Option<&str>,
) -> ProxyResult {
    let selected_route = select_route_for_forward_result(&plan, selected_channel_id, &provider.id);
    let mut metadata = Map::new();
    metadata.insert("hostProviderId".to_string(), json!(provider.id.clone()));
    metadata.insert("hostProviderName".to_string(), json!(provider.name.clone()));
    metadata.insert(
        CLAUDE_API_FORMAT_METADATA_KEY.to_string(),
        json!(claude_api_format),
    );
    metadata.insert("selectedChannelId".to_string(), json!(selected_channel_id));

    ProxyResult {
        response,
        selected_route,
        outbound_model,
        usage_record: None,
        metadata: Value::Object(metadata),
    }
}
