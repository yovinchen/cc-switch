//! Model catalog HTTP transport for CC Switch services.
//!
//! `proxy-core` owns request planning and response parsing. This module owns
//! the concrete reqwest execution against the shared proxy HTTP client.

use crate::proxy_core::api::model_catalog::{
    fetch_codex_oauth_models_with_transport, fetch_openai_compatible_models_with_transport,
    CodexOAuthModelsRequest, CodexOAuthModelsTransport, FetchedModel, ModelFetchHttpResponse,
    OpenAiCompatibleModelsRequest, OpenAiCompatibleModelsTransport,
};
use futures::future::BoxFuture;
use reqwest::header::HeaderValue;
use std::time::Duration;

const CODEX_OAUTH_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

struct ReqwestModelFetchTransport;

impl OpenAiCompatibleModelsTransport for ReqwestModelFetchTransport {
    fn send_openai_compatible_models_request<'a>(
        &'a self,
        request_plan: OpenAiCompatibleModelsRequest<'a>,
    ) -> BoxFuture<'a, Result<ModelFetchHttpResponse, String>> {
        Box::pin(async move {
            log::debug!("[ModelFetch] Trying endpoint: {}", request_plan.url);
            let client = crate::proxy::host::cc_switch::global_http_client::get();
            let mut request = client
                .get(request_plan.url)
                .header(
                    request_plan.authorization_header.0,
                    request_plan.authorization_header.1,
                )
                .timeout(Duration::from_secs(request_plan.timeout_secs));

            // Some `/models` endpoints apply the same UA allowlist as
            // forwarding/probing paths, so reuse the provider-level UA when set.
            if let Some((header, value)) = request_plan.user_agent_header {
                request = request.header(header, value.clone());
            }

            let response = request
                .send()
                .await
                .map_err(|e| format!("Request failed: {e}"))?;
            read_model_fetch_response(response).await
        })
    }
}

impl CodexOAuthModelsTransport for ReqwestModelFetchTransport {
    fn send_codex_oauth_models_request<'a>(
        &'a self,
        request_plan: CodexOAuthModelsRequest<'a>,
    ) -> BoxFuture<'a, Result<ModelFetchHttpResponse, String>> {
        Box::pin(async move {
            let client = crate::proxy::host::cc_switch::global_http_client::get();
            let response = client
                .get(request_plan.url)
                .query(&[request_plan.client_version_query])
                .header(
                    request_plan.authorization_header.0,
                    request_plan.authorization_header.1,
                )
                .header(
                    request_plan.originator_header.0,
                    request_plan.originator_header.1,
                )
                .header(
                    request_plan.account_id_header.0,
                    request_plan.account_id_header.1,
                )
                .timeout(Duration::from_secs(request_plan.timeout_secs))
                .send()
                .await
                .map_err(|e| format!("Request failed: {e}"))?;

            read_model_fetch_response(response).await
        })
    }
}

async fn read_model_fetch_response(
    response: reqwest::Response,
) -> Result<ModelFetchHttpResponse, String> {
    let status = response.status();
    let body = if status.is_success() {
        response
            .bytes()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))?
            .to_vec()
    } else {
        response.text().await.unwrap_or_default().into_bytes()
    };

    Ok(ModelFetchHttpResponse { status, body })
}

pub async fn fetch_openai_compatible_models(
    base_url: &str,
    api_key: &str,
    is_full_url: bool,
    models_url_override: Option<&str>,
    user_agent: Option<HeaderValue>,
) -> Result<Vec<FetchedModel>, String> {
    let transport = ReqwestModelFetchTransport;
    fetch_openai_compatible_models_with_transport(
        base_url,
        api_key,
        is_full_url,
        models_url_override,
        user_agent.as_ref(),
        &transport,
    )
    .await
}

pub async fn fetch_codex_oauth_models_with_token(
    token: &str,
    account_id: &str,
) -> Result<Vec<FetchedModel>, String> {
    let transport = ReqwestModelFetchTransport;
    fetch_codex_oauth_models_with_transport(
        token,
        account_id,
        CODEX_OAUTH_CLIENT_VERSION,
        &transport,
    )
    .await
}
