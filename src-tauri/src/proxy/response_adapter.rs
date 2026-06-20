use super::{error::ProxyError, hyper_client::ProxyResponse};
use crate::proxy_core::{ProxyCoreResponse, ProxyTransportResponse, ProxyTransportResponseBody};
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

pub(crate) fn proxy_core_response_to_axum_response(
    response: ProxyCoreResponse,
    build_error_context: &str,
) -> Result<axum::response::Response, ProxyError> {
    proxy_core_response_to_axum_response_with_error_message(
        response,
        build_error_context,
        "Failed to build response",
    )
}

pub(crate) fn proxy_core_response_to_axum_response_with_error_message(
    response: ProxyCoreResponse,
    build_error_context: &str,
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

    builder.body(body).map_err(|error| {
        log::error!("{build_error_context}: {error}");
        ProxyError::Internal(format!("{build_error_message}: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::{ProxyCoreResponse, ProxyResponseBody};
    use http::StatusCode;
    use http_body_util::BodyExt as _;

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

        let response = proxy_core_response_to_axum_response(response, "test").expect("bridge");

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.headers().get("x-test"),
            Some(&http::HeaderValue::from_static("yes"))
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, Bytes::from_static(b"ok"));
    }
}
