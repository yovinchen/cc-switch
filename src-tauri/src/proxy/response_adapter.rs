use super::{
    error::ProxyError,
    error_mapper::{response_build_error_to_proxy_error, CoreResponseBuildFailureContext},
    hyper_client::ProxyResponse,
};
use crate::proxy_core_adapter::{
    AxumResponseBuildErrorContext, ProxyCoreResponse, ProxyEventEnvelope, ProxyTransportResponse,
    ProxyTransportResponseBody, rebuilt_json_proxy_response, request_body_read_error_message,
    transformed_sse_proxy_response,
};
use axum::response::sse::Event;
use bytes::Bytes;
use futures::Stream;
use http::{HeaderMap, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;

pub(crate) async fn collect_axum_request_body(
    body: axum::body::Body,
) -> Result<Bytes, ProxyError> {
    body.collect()
        .await
        .map_err(|error| ProxyError::Internal(request_body_read_error_message(error)))
        .map(|collected| collected.to_bytes())
}

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

pub(crate) fn proxy_core_response_to_axum_response(
    response: ProxyCoreResponse,
    build_error_context: AxumResponseBuildErrorContext<'_>,
) -> Result<axum::response::Response, ProxyError> {
    proxy_core_response_to_axum_response_with_error_message(
        response,
        build_error_context,
        "Failed to build response",
    )
}

pub(crate) fn rebuilt_json_proxy_response_to_axum_response(
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
    response_build_error_context: CoreResponseBuildFailureContext,
    axum_build_error_context: AxumResponseBuildErrorContext<'_>,
) -> Result<axum::response::Response, ProxyError> {
    let response = rebuilt_json_proxy_response(status, headers, body)
        .map_err(|error| response_build_error_to_proxy_error(response_build_error_context, error))?;
    proxy_core_response_to_axum_response(response, axum_build_error_context)
}

pub(crate) fn transformed_sse_proxy_response_to_axum_response(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    build_error_context: AxumResponseBuildErrorContext<'_>,
) -> Result<axum::response::Response, ProxyError> {
    proxy_core_response_to_axum_response(transformed_sse_proxy_response(stream), build_error_context)
}

pub(crate) fn proxy_core_response_to_axum_response_with_error_message(
    response: ProxyCoreResponse,
    build_error_context: AxumResponseBuildErrorContext<'_>,
    build_error_message: &str,
) -> Result<axum::response::Response, ProxyError> {
    let response = response
        .into_transport_response()
        .map_err(ProxyError::Internal)?;
    let ProxyTransportResponse {
        status,
        headers,
        body,
    } = response;
    let body = match body {
        ProxyTransportResponseBody::Empty => axum::body::Body::from(Bytes::new()),
        ProxyTransportResponseBody::Bytes(body) => axum::body::Body::from(body),
        ProxyTransportResponseBody::Stream(stream) => axum::body::Body::from_stream(stream),
    };

    let mut builder = axum::response::Response::builder().status(status);
    for (key, value) in headers.iter() {
        builder = builder.header(key, value);
    }

    let build_error_context = build_error_context.message();
    builder.body(body).map_err(|error| {
        log::error!("{build_error_context}: {error}");
        ProxyError::Internal(format!("{build_error_message}: {error}"))
    })
}

pub(crate) fn proxy_event_envelope_to_axum_sse_event(event: ProxyEventEnvelope) -> Event {
    let spec = event.to_sse_spec();
    Event::default()
        .id(spec.id)
        .event(spec.event)
        .data(spec.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core_adapter::{ProxyEventEnvelope, ProxyResponseBody};
    use axum::response::{IntoResponse, sse::Sse};
    use http::StatusCode;
    use serde_json::json;
    use std::convert::Infallible;

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

    #[tokio::test]
    async fn proxy_core_response_to_axum_response_preserves_buffered_body_and_headers() {
        let mut headers = http::HeaderMap::new();
        headers.insert("x-test", http::HeaderValue::from_static("yes"));
        let response = ProxyCoreResponse::with_body(
            StatusCode::CREATED,
            headers,
            ProxyResponseBody::bytes(Bytes::from_static(b"ok")),
        );

        let response = proxy_core_response_to_axum_response(
            response,
            AxumResponseBuildErrorContext::TaggedResponse { tag: "test" },
        )
        .expect("bridge");

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.headers().get("x-test"),
            Some(&http::HeaderValue::from_static("yes"))
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, Bytes::from_static(b"ok"));
    }

    #[tokio::test]
    async fn rebuilt_json_proxy_response_to_axum_response_rebuilds_json_headers() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/plain"),
        );
        headers.insert(
            http::header::CONTENT_ENCODING,
            http::HeaderValue::from_static("gzip"),
        );

        let response = rebuilt_json_proxy_response_to_axum_response(
            StatusCode::OK,
            headers,
            json!({"ok": true}),
            CoreResponseBuildFailureContext::ClaudeJson,
            AxumResponseBuildErrorContext::ClaudeResponse,
        )
        .expect("json response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("application/json"))
        );
        assert!(!response.headers().contains_key(http::header::CONTENT_ENCODING));
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, Bytes::from_static(br#"{"ok":true}"#));
    }

    #[tokio::test]
    async fn transformed_sse_proxy_response_to_axum_response_sets_sse_headers() {
        let response = transformed_sse_proxy_response_to_axum_response(
            futures::stream::once(async { Ok(Bytes::from_static(b"data: {}\n\n")) }),
            AxumResponseBuildErrorContext::CodexSse,
        )
        .expect("sse response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/event-stream"))
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, Bytes::from_static(b"data: {}\n\n"));
    }

    #[tokio::test]
    async fn proxy_event_envelope_bridge_serializes_sse_fields() {
        let event = proxy_event_envelope_to_axum_sse_event(ProxyEventEnvelope::new(
            42,
            "request_started",
            "2026-06-20T00:00:00Z",
            json!({"provider": "relay-a"}),
        ));

        let response =
            Sse::new(futures::stream::once(async { Ok::<_, Infallible>(event) })).into_response();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).expect("sse body");

        assert!(text.contains("id: 42\n"), "{text}");
        assert!(text.contains("event: request_started\n"), "{text}");
        assert!(text.contains("\"provider\":\"relay-a\""), "{text}");
        assert!(text.ends_with("\n\n"), "{text}");
    }
}
