use crate::provider::Provider;
use crate::proxy::usage::parser::{TokenUsage, SESSION_REQUEST_ID_PREFIX};
use crate::proxy_core::{AppKind, ProviderKind, UsageRecord, UsageTokens};
use serde_json::Value;

#[allow(clippy::too_many_arguments)]
pub(crate) fn success_usage_record(
    provider_id: &str,
    provider_kind: Option<ProviderKind>,
    app_type: &str,
    model: &str,
    request_model: &str,
    outbound_model: &str,
    usage: TokenUsage,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    is_streaming: bool,
    status_code: u16,
    session_id: Option<String>,
) -> UsageRecord {
    let response_model = non_empty(model).or_else(|| non_empty(outbound_model));
    let outbound_model = non_empty(outbound_model).unwrap_or_else(|| request_model.to_string());
    let request_model = non_empty(request_model).unwrap_or_else(|| outbound_model.clone());
    let request_id = usage_request_id(&usage);
    let message_id = usage.message_id.clone();

    UsageRecord {
        request_id: Some(request_id),
        message_id,
        app: AppKind::from(app_type),
        provider_id: provider_id.to_string(),
        provider_kind,
        channel_id: None,
        channel_name: None,
        route_group: None,
        request_model,
        outbound_model,
        response_model,
        pricing_model: None,
        tokens: usage_tokens(&usage),
        latency_ms,
        first_token_ms,
        status_code,
        error_message: None,
        session_id,
        is_streaming,
        metadata: Value::Object(Default::default()),
    }
}

fn usage_request_id(usage: &TokenUsage) -> String {
    usage
        .message_id
        .as_ref()
        .map(|message_id| format!("{SESSION_REQUEST_ID_PREFIX}{message_id}"))
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn error_usage_record(
    provider: &Provider,
    app_type: &str,
    request_model: &str,
    outbound_model: Option<&str>,
    status_code: u16,
    error_message: String,
    latency_ms: u64,
    is_streaming: bool,
    session_id: Option<String>,
) -> UsageRecord {
    let outbound_model = outbound_model
        .and_then(non_empty)
        .unwrap_or_else(|| request_model.to_string());
    let request_model = non_empty(request_model).unwrap_or_else(|| outbound_model.clone());

    UsageRecord {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        message_id: None,
        app: AppKind::from(app_type),
        provider_id: provider.id.clone(),
        provider_kind: provider_kind_from_provider(provider),
        channel_id: None,
        channel_name: None,
        route_group: None,
        request_model,
        outbound_model,
        response_model: None,
        pricing_model: None,
        tokens: UsageTokens {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        },
        latency_ms,
        first_token_ms: None,
        status_code,
        error_message: Some(error_message),
        session_id,
        is_streaming,
        metadata: Value::Object(Default::default()),
    }
}

pub(crate) fn provider_kind_from_provider(provider: &Provider) -> Option<ProviderKind> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        .map(ProviderKind::from)
}

fn usage_tokens(usage: &TokenUsage) -> UsageTokens {
    UsageTokens {
        input_tokens: usage.input_tokens as u64,
        output_tokens: usage.output_tokens as u64,
        cache_read_tokens: usage.cache_read_tokens as u64,
        cache_creation_tokens: usage.cache_creation_tokens as u64,
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Provider, ProviderMeta};
    use crate::proxy_core::ProviderKind;
    use serde_json::json;

    #[test]
    fn success_record_preserves_models_tokens_and_provider_kind() {
        let usage = TokenUsage {
            input_tokens: 3,
            output_tokens: 5,
            cache_read_tokens: 7,
            cache_creation_tokens: 11,
            model: None,
            message_id: Some("msg-1".to_string()),
        };

        let record = success_usage_record(
            "provider-a",
            Some(ProviderKind::GitHubCopilot),
            "claude",
            "response-model",
            "request-model",
            "upstream-model",
            usage,
            123,
            Some(45),
            true,
            200,
            Some("session-1".to_string()),
        );

        assert_eq!(record.provider_id, "provider-a");
        assert_eq!(record.provider_kind, Some(ProviderKind::GitHubCopilot));
        assert_eq!(record.message_id.as_deref(), Some("msg-1"));
        assert_eq!(record.request_model, "request-model");
        assert_eq!(record.outbound_model, "upstream-model");
        assert_eq!(record.response_model.as_deref(), Some("response-model"));
        assert_eq!(record.tokens.input_tokens, 3);
        assert_eq!(record.first_token_ms, Some(45));
        assert!(record.is_streaming);
    }

    #[test]
    fn error_record_uses_zero_tokens_and_provider_metadata() {
        let mut provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            ..ProviderMeta::default()
        });

        let record = error_usage_record(
            &provider,
            "codex",
            "client-model",
            Some("upstream-model"),
            502,
            "upstream failed".to_string(),
            321,
            false,
            Some("session-1".to_string()),
        );

        assert_eq!(record.provider_kind, Some(ProviderKind::CodexOAuth));
        assert_eq!(record.request_model, "client-model");
        assert_eq!(record.outbound_model, "upstream-model");
        assert_eq!(record.status_code, 502);
        assert_eq!(record.error_message.as_deref(), Some("upstream failed"));
        assert_eq!(record.tokens.input_tokens, 0);
    }
}
