//! Codex OAuth model list service.
//!
//! ChatGPT Codex exposes models through a backend endpoint that is not an
//! OpenAI-compatible `/v1/models` endpoint.

use crate::proxy_core::FetchedModel;

pub async fn fetch_models_with_token(
    token: &str,
    account_id: &str,
) -> Result<Vec<FetchedModel>, String> {
    crate::proxy::model_fetch_transport::fetch_codex_oauth_models_with_token(token, account_id)
        .await
}
