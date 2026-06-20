use crate::{
    json_proxy_response, proxy_error_http_status_code, ProxyCoreResponse, ProxyCoreResult,
    ProxyErrorStatusKind,
};
use http::StatusCode;
use serde_json::{json, Value};

const CODEX_ERROR_MESSAGE_LIMIT: usize = 1800;
const CODEX_RAW_ERROR_BODY_LIMIT: usize = 1024;

#[derive(Debug, Clone, Copy)]
pub struct CodexProxyErrorContext<'a> {
    pub provider_name: &'a str,
    pub request_model: &'a str,
    pub endpoint: &'a str,
    pub fallback_message: &'a str,
    pub fallback_code: &'a str,
    pub upstream_status: Option<u16>,
    pub upstream_body: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexProxyErrorKind {
    ForwardFailed,
    Timeout,
    NoAvailableProvider,
    AllProvidersCircuitOpen,
    NoProvidersConfigured,
    MaxRetriesExceeded,
    ProviderUnhealthy,
    ConfigError,
    TransformError,
    InvalidRequest,
    AuthError,
    UpstreamError,
    DatabaseError,
    InternalError,
    ProxyError,
}

pub fn codex_proxy_error_code(kind: CodexProxyErrorKind) -> &'static str {
    match kind {
        CodexProxyErrorKind::ForwardFailed => "cc_switch_forward_failed",
        CodexProxyErrorKind::Timeout => "cc_switch_timeout",
        CodexProxyErrorKind::NoAvailableProvider => "cc_switch_no_available_provider",
        CodexProxyErrorKind::AllProvidersCircuitOpen => "cc_switch_all_providers_circuit_open",
        CodexProxyErrorKind::NoProvidersConfigured => "cc_switch_no_providers_configured",
        CodexProxyErrorKind::MaxRetriesExceeded => "cc_switch_max_retries_exceeded",
        CodexProxyErrorKind::ProviderUnhealthy => "cc_switch_provider_unhealthy",
        CodexProxyErrorKind::ConfigError => "cc_switch_config_error",
        CodexProxyErrorKind::TransformError => "cc_switch_transform_error",
        CodexProxyErrorKind::InvalidRequest => "cc_switch_invalid_request",
        CodexProxyErrorKind::AuthError => "cc_switch_auth_error",
        CodexProxyErrorKind::UpstreamError => "cc_switch_upstream_error",
        CodexProxyErrorKind::DatabaseError => "cc_switch_database_error",
        CodexProxyErrorKind::InternalError => "cc_switch_internal_error",
        CodexProxyErrorKind::ProxyError => "cc_switch_proxy_error",
    }
}

pub fn codex_proxy_error_json(ctx: CodexProxyErrorContext<'_>) -> Value {
    let parsed_upstream_body = ctx.upstream_body.map(|body| {
        serde_json::from_str::<Value>(body).unwrap_or_else(|_| Value::String(body.to_string()))
    });
    let (mut body, upstream_status) = match ctx.upstream_status {
        Some(status) => (
            codex_upstream_error_to_response_error(parsed_upstream_body.as_ref()),
            Some(status),
        ),
        None => (
            json!({
                "error": {
                    "message": ctx.fallback_message,
                    "type": "proxy_error",
                    "code": ctx.fallback_code,
                    "param": Value::Null,
                }
            }),
            None,
        ),
    };

    let Some(error_obj) = body.get_mut("error").and_then(Value::as_object_mut) else {
        return body;
    };

    let message = if upstream_status == Some(413) {
        format!(
            concat!(
                "Upstream provider rejected the request with HTTP 413 (Payload Too Large). ",
                "The request body exceeds the upstream gateway's size limit; this is the ",
                "provider's server-side limit, not a CC Switch limit. ",
                "Provider: {provider}; model: {model}; endpoint: {endpoint}. ",
                "To recover, shrink the request: run /compact, remove large pasted logs or ",
                "inline images, or ask the provider to raise its request body limit ",
                "(e.g. nginx client_max_body_size)."
            ),
            provider = ctx.provider_name,
            model = ctx.request_model,
            endpoint = ctx.endpoint,
        )
    } else {
        let cause = error_obj
            .get("message")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .filter(|message| !message.trim().is_empty())
            .unwrap_or_else(|| ctx.fallback_message.to_string());
        let status_fragment = upstream_status
            .map(|status| format!("; upstream_status: HTTP {status}"))
            .unwrap_or_default();
        format!(
            "CC Switch local proxy failed while handling Codex endpoint {endpoint}. Provider: {provider}; model: {model}{status_fragment}; cause: {cause}",
            endpoint = ctx.endpoint,
            provider = ctx.provider_name,
            model = ctx.request_model,
        )
    };

    error_obj.insert(
        "message".to_string(),
        Value::String(compact_error_message(&message, CODEX_ERROR_MESSAGE_LIMIT)),
    );

    if error_obj
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::is_empty)
        .unwrap_or(true)
    {
        error_obj.insert("type".to_string(), Value::String("proxy_error".to_string()));
    }

    if error_obj.get("code").map(Value::is_null).unwrap_or(true) {
        error_obj.insert(
            "code".to_string(),
            Value::String(ctx.fallback_code.to_string()),
        );
    }

    if !error_obj.contains_key("param") {
        error_obj.insert("param".to_string(), Value::Null);
    }

    error_obj.insert(
        "provider".to_string(),
        Value::String(ctx.provider_name.to_string()),
    );
    error_obj.insert(
        "model".to_string(),
        Value::String(ctx.request_model.to_string()),
    );
    error_obj.insert(
        "endpoint".to_string(),
        Value::String(ctx.endpoint.to_string()),
    );
    if let Some(status) = upstream_status {
        error_obj.insert(
            "upstream_status".to_string(),
            Value::Number(serde_json::Number::from(status)),
        );
    }

    body
}

pub fn codex_proxy_error_response(
    status_kind: ProxyErrorStatusKind,
    ctx: CodexProxyErrorContext<'_>,
) -> ProxyCoreResult<ProxyCoreResponse> {
    let status = StatusCode::from_u16(proxy_error_http_status_code(status_kind))
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    json_proxy_response(status, codex_proxy_error_json(ctx))
}

pub fn codex_upstream_error_to_response_error(body: Option<&Value>) -> Value {
    let Some(value) = body else {
        return json!({
            "error": {
                "message": "Upstream returned an empty error response",
                "type": "upstream_error",
                "code": Value::Null,
                "param": Value::Null,
            }
        });
    };

    if let Some(text) = value.as_str() {
        return json!({
            "error": {
                "message": text,
                "type": "upstream_error",
                "code": Value::Null,
                "param": Value::Null,
            }
        });
    }

    let source = value.get("error").unwrap_or(value);
    let message = source
        .get("message")
        .or_else(|| source.get("detail"))
        .or_else(|| source.get("status_msg"))
        .or_else(|| source.pointer("/base_resp/status_msg"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| source.as_str().map(ToString::to_string))
        .unwrap_or_else(|| {
            serde_json::to_string(source).unwrap_or_else(|_| "Upstream error".to_string())
        });
    let error_type = source
        .get("type")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| "upstream_error".to_string());
    let code = source
        .get("code")
        .cloned()
        .or_else(|| source.pointer("/base_resp/status_code").cloned())
        .unwrap_or(Value::Null);
    let param = source.get("param").cloned().unwrap_or(Value::Null);

    json!({
        "error": {
            "message": message,
            "type": error_type,
            "code": code,
            "param": param,
        }
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodexChatErrorNormalization {
    pub response_error: Value,
    pub non_json_body_preview: Option<String>,
}

impl CodexChatErrorNormalization {
    pub fn non_json_body_log_message(&self) -> Option<String> {
        self.non_json_body_preview
            .as_ref()
            .map(|preview| format!("[Codex] Chat 错误响应不是合法 JSON，按文本透传: {preview}"))
    }
}

/// Normalize an upstream Chat Completions error body into the OpenAI Responses
/// error envelope used by Codex clients.
pub fn normalize_codex_chat_error_body(body: &[u8]) -> CodexChatErrorNormalization {
    match serde_json::from_slice::<Value>(body) {
        Ok(value) => CodexChatErrorNormalization {
            response_error: codex_upstream_error_to_response_error(Some(&value)),
            non_json_body_preview: None,
        },
        Err(_) => {
            let preview = raw_error_body_preview(body, CODEX_RAW_ERROR_BODY_LIMIT);
            CodexChatErrorNormalization {
                response_error: codex_upstream_error_to_response_error(Some(&Value::String(
                    preview.clone(),
                ))),
                non_json_body_preview: Some(preview),
            }
        }
    }
}

fn compact_error_message(message: &str, max_chars: usize) -> String {
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let truncated = normalized
        .chars()
        .take(max_chars)
        .collect::<String>()
        .trim_end()
        .to_string();
    format!("{truncated}…(truncated)")
}

fn raw_error_body_preview(body: &[u8], max_bytes: usize) -> String {
    let lossy = String::from_utf8_lossy(body);
    if lossy.len() <= max_bytes {
        return lossy.into_owned();
    }

    let mut end = max_bytes;
    while end > 0 && !lossy.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…(truncated)", &lossy[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_proxy_error_code_preserves_host_contract_values() {
        let cases = [
            (
                CodexProxyErrorKind::ForwardFailed,
                "cc_switch_forward_failed",
            ),
            (CodexProxyErrorKind::Timeout, "cc_switch_timeout"),
            (
                CodexProxyErrorKind::NoAvailableProvider,
                "cc_switch_no_available_provider",
            ),
            (
                CodexProxyErrorKind::AllProvidersCircuitOpen,
                "cc_switch_all_providers_circuit_open",
            ),
            (
                CodexProxyErrorKind::NoProvidersConfigured,
                "cc_switch_no_providers_configured",
            ),
            (
                CodexProxyErrorKind::MaxRetriesExceeded,
                "cc_switch_max_retries_exceeded",
            ),
            (
                CodexProxyErrorKind::ProviderUnhealthy,
                "cc_switch_provider_unhealthy",
            ),
            (CodexProxyErrorKind::ConfigError, "cc_switch_config_error"),
            (
                CodexProxyErrorKind::TransformError,
                "cc_switch_transform_error",
            ),
            (
                CodexProxyErrorKind::InvalidRequest,
                "cc_switch_invalid_request",
            ),
            (CodexProxyErrorKind::AuthError, "cc_switch_auth_error"),
            (
                CodexProxyErrorKind::UpstreamError,
                "cc_switch_upstream_error",
            ),
            (
                CodexProxyErrorKind::DatabaseError,
                "cc_switch_database_error",
            ),
            (
                CodexProxyErrorKind::InternalError,
                "cc_switch_internal_error",
            ),
            (CodexProxyErrorKind::ProxyError, "cc_switch_proxy_error"),
        ];

        for (kind, expected) in cases {
            assert_eq!(codex_proxy_error_code(kind), expected);
        }
    }

    #[test]
    fn codex_proxy_forward_error_includes_context_and_cause() {
        let body = codex_proxy_error_json(CodexProxyErrorContext {
            provider_name: "DeepSeek",
            request_model: "deepseek-chat",
            endpoint: "/responses",
            fallback_message: "连接失败: dns lookup failed",
            fallback_code: "cc_switch_forward_failed",
            upstream_status: None,
            upstream_body: None,
        });

        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("CC Switch local proxy failed"));
        assert!(message.contains("DeepSeek"));
        assert!(message.contains("deepseek-chat"));
        assert!(message.contains("/responses"));
        assert!(message.contains("dns lookup failed"));
        assert_eq!(body["error"]["code"], "cc_switch_forward_failed");
        assert_eq!(body["error"]["provider"], "DeepSeek");
        assert_eq!(body["error"]["model"], "deepseek-chat");
    }

    #[test]
    fn codex_proxy_error_response_builds_json_response_with_status() {
        let response = codex_proxy_error_response(
            ProxyErrorStatusKind::AuthError,
            CodexProxyErrorContext {
                provider_name: "DeepSeek",
                request_model: "deepseek-chat",
                endpoint: "/responses",
                fallback_message: "bad token",
                fallback_code: "cc_switch_auth_error",
                upstream_status: None,
                upstream_body: None,
            },
        )
        .expect("response");

        assert_eq!(response.status, StatusCode::UNAUTHORIZED);

        let body = match response.body {
            crate::ProxyResponseBody::Bytes(body) => body,
            other => panic!("expected bytes body, got {other:?}"),
        };
        let value: Value = serde_json::from_slice(&body).expect("json body");

        assert_eq!(value["error"]["code"], "cc_switch_auth_error");
        assert_eq!(value["error"]["provider"], "DeepSeek");
        assert_eq!(value["error"]["model"], "deepseek-chat");
    }

    #[test]
    fn codex_proxy_upstream_error_normalizes_nonstandard_body() {
        let body = codex_proxy_error_json(CodexProxyErrorContext {
            provider_name: "MiniMax",
            request_model: "abab6.5s",
            endpoint: "/responses",
            fallback_message: "upstream returned 502",
            fallback_code: "cc_switch_upstream_error",
            upstream_status: Some(502),
            upstream_body: Some(
                r#"{"base_resp":{"status_code":2013,"status_msg":"upstream gateway failed"}}"#,
            ),
        });

        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("upstream_status: HTTP 502"));
        assert!(message.contains("upstream gateway failed"));
        assert_eq!(body["error"]["code"], 2013);
        assert_eq!(body["error"]["upstream_status"], 502);
    }

    #[test]
    fn codex_proxy_413_points_to_upstream_not_local_proxy() {
        let body = codex_proxy_error_json(CodexProxyErrorContext {
            provider_name: "HCAI",
            request_model: "gpt-5.5",
            endpoint: "/responses",
            fallback_message: "request entity too large",
            fallback_code: "cc_switch_upstream_error",
            upstream_status: Some(413),
            upstream_body: Some(
                "<html>\r\n<head><title>413 Request Entity Too Large</title></head>\r\n\
                 <body>\r\n<center><h1>413 Request Entity Too Large</h1></center>\r\n\
                 <hr><center>nginx/1.29.6</center>\r\n</body>\r\n</html>",
            ),
        });

        let message = body["error"]["message"].as_str().unwrap();
        assert!(!message.contains("CC Switch local proxy failed"));
        assert!(message.contains("413"));
        assert!(message.to_lowercase().contains("upstream"));
        assert!(message.contains("/compact"));
        assert!(!message.contains("<html>"));
        assert!(!message.contains("nginx/1.29.6"));
        assert_eq!(body["error"]["upstream_status"], 413);
        assert_eq!(body["error"]["provider"], "HCAI");
        assert_eq!(body["error"]["model"], "gpt-5.5");
        assert_eq!(body["error"]["endpoint"], "/responses");
    }

    #[test]
    fn upstream_error_normalizer_handles_detail_field_and_plain_text() {
        let detail = json!({"detail": "quota exceeded"});
        let normalized = codex_upstream_error_to_response_error(Some(&detail));
        assert_eq!(normalized["error"]["message"], "quota exceeded");

        let text = Value::String("plain upstream error".to_string());
        let normalized = codex_upstream_error_to_response_error(Some(&text));
        assert_eq!(normalized["error"]["message"], "plain upstream error");
    }

    #[test]
    fn codex_chat_error_body_normalizes_json_without_preview() {
        let normalized = normalize_codex_chat_error_body(
            br#"{"base_resp":{"status_code":2013,"status_msg":"bad role"}}"#,
        );

        assert_eq!(normalized.response_error["error"]["message"], "bad role");
        assert_eq!(normalized.response_error["error"]["code"], 2013);
        assert_eq!(normalized.non_json_body_preview, None);
        assert!(normalized.non_json_body_log_message().is_none());
    }

    #[test]
    fn codex_chat_error_body_wraps_plain_text_with_preview() {
        let normalized = normalize_codex_chat_error_body(b"Unauthorized");

        assert_eq!(
            normalized.response_error["error"]["message"],
            "Unauthorized"
        );
        assert_eq!(
            normalized.non_json_body_preview.as_deref(),
            Some("Unauthorized")
        );
        assert_eq!(
            normalized.non_json_body_log_message().as_deref(),
            Some("[Codex] Chat 错误响应不是合法 JSON，按文本透传: Unauthorized")
        );
    }

    #[test]
    fn codex_chat_error_body_truncates_non_json_preview_on_char_boundary() {
        let body = format!("{}你", "a".repeat(CODEX_RAW_ERROR_BODY_LIMIT - 1));

        let normalized = normalize_codex_chat_error_body(body.as_bytes());
        let preview = normalized.non_json_body_preview.unwrap();

        assert!(preview.ends_with("…(truncated)"), "{preview}");
        assert!(preview.starts_with(&"a".repeat(CODEX_RAW_ERROR_BODY_LIMIT - 1)));
        assert!(!preview.contains('你'));
        assert_eq!(normalized.response_error["error"]["message"], preview);
    }
}
