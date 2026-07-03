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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::proxy::host::cc_switch::channel_key_runtime_source::channel_key_runtime_source_from_database;
    use crate::proxy_core::api::domain::{
        AppKind, ChannelOverrides, ModelCapabilities, ModelRoute, ProviderKind, ProviderSpec,
        ProxyBody, RetryPolicy, UpstreamEndpoint,
    };
    use crate::proxy_core::api::errors::ProxyCoreError;
    use crate::proxy_core::api::routing::{
        ChannelSpec, ChannelStatus, InterfaceKind, ResolvedChannelAttempt, RouteSelection,
        DEFAULT_ROUTE_GROUP,
    };
    use http::{Method, StatusCode};
    use std::sync::Arc;

    fn provider_spec(id: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: id.to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: Default::default(),
        }
    }

    fn channel_spec(id: &str, priority: i64, model: &str) -> ChannelSpec {
        ChannelSpec {
            id: id.to_string(),
            provider_id: "provider-a".to_string(),
            app: AppKind::Claude,
            name: id.to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: format!("https://{id}.example.com/v1"),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::OpenAiResponses,
            auth_profile: None,
            models: vec![ModelRoute {
                public_model: model.to_string(),
                upstream_model: format!("upstream-{model}"),
                capabilities: ModelCapabilities::default(),
                pricing_model: None,
                request_overrides: json!({}),
                response_overrides: json!({}),
            }],
            groups: vec![DEFAULT_ROUTE_GROUP.to_string()],
            priority,
            weight: 100,
            retry_policy: RetryPolicy::default(),
            health_policy: Default::default(),
            overrides: ChannelOverrides::default(),
            tags: Vec::new(),
            metadata: json!({}),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        }
    }

    fn route_plan(provider_id: &str, channel_id: &str) -> RoutePlan {
        let mut channel = channel_spec(channel_id, 100, "sonnet");
        channel.provider_id = provider_id.to_string();
        let model_route = channel.models.first().cloned();
        let selection = RouteSelection {
            provider: provider_spec(provider_id),
            channel,
            model_route,
            inbound_interface: InterfaceKind::AnthropicMessages,
            outbound_interface: InterfaceKind::OpenAiResponses,
        };
        RoutePlan {
            selection,
            selections: Vec::new(),
            attempts: Vec::new(),
        }
    }

    fn proxy_request() -> ProxyRequest {
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({ "model": "sonnet", "messages": [] })),
        );
        request.requested_model = Some("sonnet".to_string());
        request
    }

    #[tokio::test]
    async fn forward_pipeline_without_runtime_reports_unsupported() {
        let pipeline: CcSwitchForwardPipeline<
            crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime,
        > = CcSwitchForwardPipeline::without_runtime(channel_key_runtime_source_from_database(
            Arc::new(Database::memory().expect("memory db")),
        ));

        let err = pipeline
            .forward(proxy_request(), route_plan("provider-a", "channel-a"))
            .await
            .expect_err("plain services do not own server runtime");

        assert!(matches!(err, ProxyCoreError::Unsupported(_)));
    }

    #[test]
    fn proxy_response_bridge_preserves_buffered_body() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        let response = ProxyResponse::buffered(
            StatusCode::CREATED,
            headers,
            Bytes::from_static(br#"{"ok":true}"#),
        );

        let core_response = proxy_response_to_core_response(response, Option::<()>::None);

        assert_eq!(core_response.status, StatusCode::CREATED);
        assert_eq!(
            core_response
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        match core_response.body {
            ProxyResponseBody::Bytes(body) => {
                assert_eq!(body, Bytes::from_static(br#"{"ok":true}"#))
            }
            other => panic!("expected bytes body, got {other:?}"),
        }
    }

    #[test]
    fn forward_result_bridge_projects_metadata_and_successful_channel() {
        let primary = route_plan("provider-a", "channel-a").selection;
        let fallback = route_plan("provider-a", "channel-b").selection;
        let plan = RoutePlan {
            selection: primary.clone(),
            selections: vec![primary, fallback],
            attempts: Vec::new(),
        };
        let result = ForwardResult {
            response: ProxyResponse::buffered(
                StatusCode::OK,
                http::HeaderMap::new(),
                Bytes::from_static(b"{}"),
            ),
            provider: Provider::with_id(
                "provider-a".to_string(),
                "Provider A".to_string(),
                json!({}),
                None,
            ),
            claude_api_format: Some("messages".to_string()),
            outbound_model: Some("upstream-sonnet".to_string()),
            selected_channel: Some(ResolvedChannelAttempt {
                channel_id: "channel-b".to_string(),
                channel_name: "Channel B".to_string(),
                base_url: "https://fallback.example.com/v1".to_string(),
                interface_kind: "openai_responses".to_string(),
                auth_profile_ref: None,
                public_model: Some("sonnet".to_string()),
                upstream_model: Some("upstream-sonnet".to_string()),
                pricing_model: None,
                header_overrides: json!({}),
                param_overrides: json!({}),
                status_code_mapping: json!([]),
                request_overrides: json!({}),
                response_overrides: json!({}),
                retry_policy: json!({}),
            }),
            connection_guard: None,
        };

        let proxy_result = forward_result_to_proxy_result(result, plan);

        assert_eq!(proxy_result.selected_route.channel.id, "channel-b");
        assert_eq!(
            proxy_result.outbound_model.as_deref(),
            Some("upstream-sonnet")
        );
        assert_eq!(
            proxy_result
                .metadata
                .get("hostProviderId")
                .and_then(Value::as_str),
            Some("provider-a")
        );
        assert_eq!(
            proxy_result
                .metadata
                .get("hostProviderName")
                .and_then(Value::as_str),
            Some("Provider A")
        );
        assert_eq!(
            proxy_result
                .metadata
                .get("claudeApiFormat")
                .and_then(Value::as_str),
            Some("messages")
        );
        assert_eq!(
            proxy_result
                .metadata
                .get("selectedChannelId")
                .and_then(Value::as_str),
            Some("channel-b")
        );
    }

    #[tokio::test]
    async fn proxy_response_bridge_wraps_streamed_body() {
        let response = ProxyResponse::streamed(
            StatusCode::OK,
            http::HeaderMap::new(),
            futures::stream::once(async { Ok(Bytes::from_static(b"chunk")) }),
        );

        let core_response = proxy_response_to_core_response(response, Option::<()>::None);

        match core_response.body {
            ProxyResponseBody::Stream(mut stream) => {
                let chunk = stream.next().await.expect("chunk").expect("stream item");
                assert_eq!(chunk, Bytes::from_static(b"chunk"));
                assert!(stream.next().await.is_none());
            }
            other => panic!("expected stream body, got {other:?}"),
        }
    }
}
