//! Proxy session-id host adapter.
//!
//! Session extraction policy lives in `proxy-core`; the Tauri host only
//! supplies UUID generation for requests that do not carry a stable client
//! session id.

use crate::proxy_core::SessionIdResult;
use axum::http::HeaderMap;
use uuid::Uuid;

pub fn extract_session_id(
    headers: &HeaderMap,
    body: &serde_json::Value,
    client_format: &str,
) -> SessionIdResult {
    crate::proxy_core::extract_session_id_with_generator(headers, body, client_format, || {
        Uuid::new_v4().to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::SessionIdSource;
    use serde_json::json;

    #[test]
    fn host_adapter_generates_uuid_when_core_needs_new_session_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_session_id(&headers, &body, "claude");

        uuid::Uuid::parse_str(&result.session_id).expect("generated session id should be a UUID");
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }
}
