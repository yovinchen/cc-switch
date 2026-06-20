use crate::provider::Provider;
use crate::proxy::{
    error::ProxyError,
    error_mapper::{get_error_message, map_proxy_error_to_status},
    handler_context::RequestContext,
    server::ProxyState,
};
use crate::proxy_core::{
    AppKind, ProviderKind, ProxyServices, TransformedResponseUsageFormat, UsageRecord,
    error_usage_record_with_request_id_fallback,
    transformed_response_usage_record_with_request_id_fallback,
};
#[cfg(test)]
use crate::proxy_core::{TokenUsage, success_usage_record_with_request_id_fallback};
use serde_json::Value;

#[cfg(test)]
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
    success_usage_record_with_request_id_fallback(
        provider_id,
        provider_kind,
        AppKind::from(app_type),
        model,
        request_model,
        outbound_model,
        usage,
        latency_ms,
        first_token_ms,
        is_streaming,
        status_code,
        session_id,
        || uuid::Uuid::new_v4().to_string(),
    )
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
    error_usage_record_with_request_id_fallback(
        &provider.id,
        provider_kind_from_provider(provider),
        AppKind::from(app_type),
        request_model,
        outbound_model,
        status_code,
        error_message,
        latency_ms,
        is_streaming,
        session_id,
        || uuid::Uuid::new_v4().to_string(),
    )
}

pub(crate) fn record_forward_error_usage(
    state: &ProxyState,
    ctx: &RequestContext,
    is_streaming: bool,
    error: &ProxyError,
) {
    let record = error_usage_record(
        &ctx.provider,
        ctx.app_type_str,
        &ctx.request_model,
        ctx.outbound_model.as_deref(),
        map_proxy_error_to_status(error),
        get_error_message(error),
        ctx.latency_ms(),
        is_streaming,
        Some(ctx.session_id.clone()),
    );

    let services = state.proxy_core_services.clone();
    tokio::spawn(async move {
        if let Err(e) = services.usage_sink().record_usage(record).await {
            log::warn!("记录失败请求日志失败: {e}");
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn transformed_response_usage_record(
    body: &Value,
    format: TransformedResponseUsageFormat,
    provider: &Provider,
    app_type: &str,
    request_model: &str,
    outbound_model: Option<&str>,
    latency_ms: u64,
    status_code: u16,
    session_id: Option<String>,
) -> Option<UsageRecord> {
    transformed_response_usage_record_with_request_id_fallback(
        body,
        format,
        &provider.id,
        provider_kind_from_provider(provider),
        AppKind::from(app_type),
        request_model,
        outbound_model,
        latency_ms,
        status_code,
        session_id,
        || uuid::Uuid::new_v4().to_string(),
    )
}

pub(crate) fn provider_kind_from_provider(provider: &Provider) -> Option<ProviderKind> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        .map(ProviderKind::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Provider, ProviderMeta};
    use crate::proxy_core::{AppKind, ProviderKind};
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

    #[test]
    fn transformed_response_record_adapts_host_provider_and_app() {
        let mut provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..ProviderMeta::default()
        });

        let record = transformed_response_usage_record(
            &json!({
                "id": "msg_1",
                "model": "claude-response-model",
                "usage": {
                    "input_tokens": 3,
                    "output_tokens": 5
                }
            }),
            TransformedResponseUsageFormat::Claude,
            &provider,
            "claude-desktop",
            "request-model",
            Some("outbound-model"),
            123,
            200,
            Some("session-1".to_string()),
        )
        .expect("usage record");

        assert_eq!(record.app, AppKind::ClaudeDesktop);
        assert_eq!(record.provider_id, "provider-a");
        assert_eq!(record.provider_kind, Some(ProviderKind::GitHubCopilot));
        assert_eq!(
            record.response_model.as_deref(),
            Some("claude-response-model")
        );
        assert_eq!(record.request_model, "request-model");
        assert_eq!(record.outbound_model, "outbound-model");
        assert_eq!(record.tokens.input_tokens, 3);
        assert!(!record.is_streaming);
    }
}
