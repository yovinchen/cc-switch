//! Client format and session identity extraction.

use http::HeaderMap;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientFormat {
    Claude,
    Codex,
    OpenAI,
    Gemini,
    GeminiCli,
    Unknown,
}

impl ClientFormat {
    pub fn from_path(path: &str) -> Self {
        if path.contains("/v1/messages") {
            ClientFormat::Claude
        } else if path.contains("/v1/responses") {
            ClientFormat::Codex
        } else if path.contains("/v1/chat/completions") {
            ClientFormat::OpenAI
        } else if path.contains("/v1internal/") && path.contains("generateContent") {
            ClientFormat::GeminiCli
        } else if path.contains("generateContent") {
            ClientFormat::Gemini
        } else {
            ClientFormat::Unknown
        }
    }

    pub fn from_body(body: &Value) -> Self {
        if body.get("messages").is_some()
            && body.get("model").is_some()
            && body.get("response_format").is_none()
            && body.get("contents").is_none()
        {
            if body.get("max_tokens").is_some() {
                return ClientFormat::Claude;
            }
            return ClientFormat::OpenAI;
        }

        if body.get("input").is_some() {
            return ClientFormat::Codex;
        }

        if body.get("contents").is_some() {
            return ClientFormat::Gemini;
        }

        ClientFormat::Unknown
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ClientFormat::Claude => "claude",
            ClientFormat::Codex => "codex",
            ClientFormat::OpenAI => "openai",
            ClientFormat::Gemini => "gemini",
            ClientFormat::GeminiCli => "gemini_cli",
            ClientFormat::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for ClientFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxySessionRequestMetadata {
    pub client_format: ClientFormat,
    pub model: Option<String>,
    pub is_streaming: bool,
}

pub fn proxy_session_request_metadata(
    request_url: &str,
    body: Option<&Value>,
) -> ProxySessionRequestMetadata {
    let mut client_format = ClientFormat::from_path(request_url);
    if client_format == ClientFormat::Unknown {
        if let Some(body) = body {
            client_format = ClientFormat::from_body(body);
        }
    }

    let is_streaming = body
        .and_then(|body| body.get("stream"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let model = body
        .and_then(|body| body.get("model"))
        .and_then(Value::as_str)
        .map(ToString::to_string);

    ProxySessionRequestMetadata {
        client_format,
        model,
        is_streaming,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIdSource {
    MetadataUserId,
    MetadataSessionId,
    Header,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionIdResult {
    pub session_id: String,
    pub source: SessionIdSource,
    pub client_provided: bool,
}

pub fn extract_session_id_with_generator(
    headers: &HeaderMap,
    body: &Value,
    client_format: &str,
    generate_session_id: impl FnOnce() -> String,
) -> SessionIdResult {
    if client_format == "claude" {
        if let Some(result) = extract_claude_session(headers, body) {
            return result;
        }
    }

    if client_format == "codex" || client_format == "openai" {
        if let Some(result) = extract_codex_session(headers, body) {
            return result;
        }
    }

    if let Some(result) = extract_from_metadata(body) {
        return result;
    }

    SessionIdResult {
        session_id: generate_session_id(),
        source: SessionIdSource::Generated,
        client_provided: false,
    }
}

fn extract_claude_session(headers: &HeaderMap, body: &Value) -> Option<SessionIdResult> {
    for header_name in &["x-claude-code-session-id", "claude-code-session-id"] {
        if let Some(value) = headers.get(*header_name) {
            if let Ok(session_id) = value.to_str() {
                if !session_id.is_empty() {
                    return Some(SessionIdResult {
                        session_id: session_id.to_string(),
                        source: SessionIdSource::Header,
                        client_provided: true,
                    });
                }
            }
        }
    }

    extract_from_metadata(body)
}

fn extract_codex_session(headers: &HeaderMap, body: &Value) -> Option<SessionIdResult> {
    for header_name in &["session_id", "x-session-id"] {
        if let Some(value) = headers.get(*header_name) {
            if let Ok(session_id) = value.to_str() {
                if session_id.len() > 20 {
                    return Some(SessionIdResult {
                        session_id: format!("codex_{session_id}"),
                        source: SessionIdSource::Header,
                        client_provided: true,
                    });
                }
            }
        }
    }

    if let Some(session_id) = body
        .get("metadata")
        .and_then(|metadata| metadata.get("session_id"))
        .and_then(Value::as_str)
    {
        if session_id.len() > 10 {
            return Some(SessionIdResult {
                session_id: format!("codex_{session_id}"),
                source: SessionIdSource::MetadataSessionId,
                client_provided: true,
            });
        }
    }

    None
}

fn extract_from_metadata(body: &Value) -> Option<SessionIdResult> {
    let metadata = body.get("metadata")?;

    if let Some(user_id) = metadata.get("user_id").and_then(Value::as_str) {
        if let Some(session_id) = parse_session_from_user_id(user_id) {
            return Some(SessionIdResult {
                session_id,
                source: SessionIdSource::MetadataUserId,
                client_provided: true,
            });
        }
    }

    if let Some(session_id) = metadata.get("session_id").and_then(Value::as_str) {
        if !session_id.is_empty() {
            return Some(SessionIdResult {
                session_id: session_id.to_string(),
                source: SessionIdSource::MetadataSessionId,
                client_provided: true,
            });
        }
    }

    None
}

pub fn parse_session_from_user_id(user_id: &str) -> Option<String> {
    if let Some(pos) = user_id.find("_session_") {
        let session_id = &user_id[pos + 9..];
        if !session_id.is_empty() {
            return Some(session_id.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn generated_id() -> String {
        "generated-session".to_string()
    }

    #[test]
    fn client_format_from_path_claude() {
        assert_eq!(
            ClientFormat::from_path("/v1/messages"),
            ClientFormat::Claude
        );
        assert_eq!(
            ClientFormat::from_path("/api/v1/messages"),
            ClientFormat::Claude
        );
    }

    #[test]
    fn client_format_from_path_codex() {
        assert_eq!(
            ClientFormat::from_path("/v1/responses"),
            ClientFormat::Codex
        );
    }

    #[test]
    fn client_format_from_path_openai() {
        assert_eq!(
            ClientFormat::from_path("/v1/chat/completions"),
            ClientFormat::OpenAI
        );
    }

    #[test]
    fn client_format_from_path_gemini() {
        assert_eq!(
            ClientFormat::from_path("/v1beta/models/gemini-pro:generateContent"),
            ClientFormat::Gemini
        );
    }

    #[test]
    fn client_format_from_path_gemini_cli() {
        assert_eq!(
            ClientFormat::from_path("/v1internal/models/gemini-pro:generateContent"),
            ClientFormat::GeminiCli
        );
    }

    #[test]
    fn client_format_from_body_claude() {
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}],
            "max_tokens": 1024
        });
        assert_eq!(ClientFormat::from_body(&body), ClientFormat::Claude);
    }

    #[test]
    fn client_format_from_body_codex() {
        let body = json!({"input": "Write a function"});
        assert_eq!(ClientFormat::from_body(&body), ClientFormat::Codex);
    }

    #[test]
    fn client_format_from_body_gemini() {
        let body = json!({"contents": [{"parts": [{"text": "Hello"}]}]});
        assert_eq!(ClientFormat::from_body(&body), ClientFormat::Gemini);
    }

    #[test]
    fn client_format_as_str() {
        assert_eq!(ClientFormat::Claude.as_str(), "claude");
        assert_eq!(ClientFormat::Codex.as_str(), "codex");
        assert_eq!(ClientFormat::OpenAI.as_str(), "openai");
        assert_eq!(ClientFormat::Gemini.as_str(), "gemini");
        assert_eq!(ClientFormat::GeminiCli.as_str(), "gemini_cli");
        assert_eq!(ClientFormat::Unknown.as_str(), "unknown");
    }

    #[test]
    fn proxy_session_metadata_uses_path_format_model_and_stream_flag() {
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}],
            "stream": true
        });

        let metadata = proxy_session_request_metadata("/v1/messages", Some(&body));

        assert_eq!(metadata.client_format, ClientFormat::Claude);
        assert_eq!(metadata.model, Some("claude-3-5-sonnet".to_string()));
        assert!(metadata.is_streaming);
    }

    #[test]
    fn proxy_session_metadata_uses_body_format_when_path_unknown() {
        let body = json!({
            "contents": [{"parts": [{"text": "Hello"}]}]
        });

        let metadata = proxy_session_request_metadata("/custom/path", Some(&body));

        assert_eq!(metadata.client_format, ClientFormat::Gemini);
        assert_eq!(metadata.model, None);
        assert!(!metadata.is_streaming);
    }

    #[test]
    fn extracts_session_from_claude_metadata_user_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}],
            "metadata": {
                "user_id": "user_john_doe_session_abc123def456"
            }
        });

        let result = extract_session_id_with_generator(&headers, &body, "claude", generated_id);

        assert_eq!(result.session_id, "abc123def456");
        assert_eq!(result.source, SessionIdSource::MetadataUserId);
        assert!(result.client_provided);
    }

    #[test]
    fn extracts_session_from_claude_metadata_session_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}],
            "metadata": {
                "session_id": "my-session-123"
            }
        });

        let result = extract_session_id_with_generator(&headers, &body, "claude", generated_id);

        assert_eq!(result.session_id, "my-session-123");
        assert_eq!(result.source, SessionIdSource::MetadataSessionId);
        assert!(result.client_provided);
    }

    #[test]
    fn extracts_session_from_claude_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-claude-code-session-id",
            "d937243f-2702-4f20-97b6-c9682235ab81"
                .parse()
                .unwrap(),
        );
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_session_id_with_generator(&headers, &body, "claude", generated_id);

        assert_eq!(result.session_id, "d937243f-2702-4f20-97b6-c9682235ab81");
        assert_eq!(result.source, SessionIdSource::Header);
        assert!(result.client_provided);
    }

    #[test]
    fn claude_header_precedes_metadata() {
        let mut headers = HeaderMap::new();
        headers.insert("x-claude-code-session-id", "header-session-123".parse().unwrap());
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}],
            "metadata": {
                "session_id": "my-session-123"
            }
        });

        let result = extract_session_id_with_generator(&headers, &body, "claude", generated_id);

        assert_eq!(result.session_id, "header-session-123");
        assert_eq!(result.source, SessionIdSource::Header);
        assert!(result.client_provided);
    }

    #[test]
    fn codex_previous_response_id_is_not_stable_session_identity() {
        let headers = HeaderMap::new();
        let body = json!({
            "input": "Write a function",
            "previous_response_id": "resp_abc123def456789"
        });

        let result = extract_session_id_with_generator(&headers, &body, "codex", generated_id);

        assert_eq!(result.session_id, "generated-session");
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }

    #[test]
    fn extracts_generated_session_when_not_found() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_session_id_with_generator(&headers, &body, "claude", generated_id);

        assert_eq!(result.session_id, "generated-session");
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }

    #[test]
    fn parses_session_from_user_id() {
        assert_eq!(
            parse_session_from_user_id("user_john_session_abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            parse_session_from_user_id("my_app_session_xyz789"),
            Some("xyz789".to_string())
        );
        assert_eq!(
            parse_session_from_user_id("no_session_marker"),
            Some("marker".to_string())
        );
        assert_eq!(parse_session_from_user_id("user_john_abc123"), None);
        assert_eq!(parse_session_from_user_id("_session_"), None);
    }
}
