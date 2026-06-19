//! Codex OAuth model list service.
//!
//! ChatGPT Codex exposes models through a backend endpoint that is not an
//! OpenAI-compatible `/v1/models` endpoint.

use crate::proxy_core::{
    build_codex_oauth_models_request, parse_codex_oauth_models,
    truncate_codex_oauth_models_error_body, FetchedModel,
};
use std::time::Duration;

const CODEX_OAUTH_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn fetch_models_with_token(
    token: &str,
    account_id: &str,
) -> Result<Vec<FetchedModel>, String> {
    let client = crate::proxy::http_client::get();
    let request = build_codex_oauth_models_request(token, account_id, CODEX_OAUTH_CLIENT_VERSION);
    let response = client
        .get(request.url)
        .query(&[request.client_version_query])
        .header(
            request.authorization_header.0,
            request.authorization_header.1,
        )
        .header(request.originator_header.0, request.originator_header.1)
        .header(request.account_id_header.0, request.account_id_header.1)
        .timeout(Duration::from_secs(request.timeout_secs))
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body =
            truncate_codex_oauth_models_error_body(response.text().await.unwrap_or_default());
        return Err(format!("HTTP {status}: {body}"));
    }

    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    Ok(parse_codex_oauth_models(&value))
}
