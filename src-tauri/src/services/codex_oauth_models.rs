//! Codex OAuth model list service.
//!
//! ChatGPT Codex exposes models through a backend endpoint that is not an
//! OpenAI-compatible `/v1/models` endpoint.

use crate::proxy_core::{
    fetch_codex_oauth_models_with_transport, CodexOAuthModelsRequest, CodexOAuthModelsTransport,
    FetchedModel, ModelFetchHttpResponse,
};
use futures::future::BoxFuture;
use std::time::Duration;

const CODEX_OAUTH_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

struct ReqwestCodexOAuthModelsTransport;

impl CodexOAuthModelsTransport for ReqwestCodexOAuthModelsTransport {
    fn send_codex_oauth_models_request<'a>(
        &'a self,
        request_plan: CodexOAuthModelsRequest<'a>,
    ) -> BoxFuture<'a, Result<ModelFetchHttpResponse, String>> {
        Box::pin(async move {
            let client = crate::proxy::http_client::get();
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
        })
    }
}

pub async fn fetch_models_with_token(
    token: &str,
    account_id: &str,
) -> Result<Vec<FetchedModel>, String> {
    let transport = ReqwestCodexOAuthModelsTransport;
    fetch_codex_oauth_models_with_transport(
        token,
        account_id,
        CODEX_OAUTH_CLIENT_VERSION,
        &transport,
    )
    .await
}
