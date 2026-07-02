//! CC Switch provider projection helpers.

use crate::app_config::AppType;
use crate::database::Database;
use crate::error::AppError;
use crate::provider::{AuthBindingSource as ProviderAuthBindingSource, Provider, ProviderMeta};
use crate::proxy::provider::claude_provider_api_format;
use crate::proxy_core::api::auth::{
    classify_provider_managed_auth, extract_claude_auth_key_from_settings,
    extract_gemini_api_key_from_settings, is_gemini_oauth_key_shape,
    managed_account_id_for_auth_provider, ManagedAccountBindingInput, ManagedAccountBindingSource,
    ProviderManagedAuthFacts, GITHUB_COPILOT_AUTH_PROVIDER,
};
use crate::proxy_core::api::domain::{
    extract_claude_base_url_from_settings, infer_claude_provider_kind, provider_account_ref,
    provider_metadata_from_input, unsupported_app_kind_config_error, AppKind, ProviderKind,
    ProviderMetadata, ProviderMetadataInput, ProviderSpec,
};
use crate::proxy_core::api::errors::{
    config_error_with_context as core_config_error_with_context, ProxyCoreError, ProxyCoreResult,
};
use crate::proxy_core::api::ports::{
    codex_config_text_from_settings, codex_wire_api_from_config_toml,
};
use crate::proxy_core::api::transforms::{
    claude_provider_transform_required, claude_transform_streaming_decision,
    ClaudeTransformStreamingDecision,
};
use crate::proxy_core::api::transport::{
    codex_responses_to_chat_conversion_required, CodexProviderChatCompletionsFacts,
    CodexResponsesToChatConversionFacts,
};
use http::HeaderMap;
use serde_json::{json, Value};

fn provider_projection_error(context: &str, error: AppError) -> ProxyCoreError {
    core_config_error_with_context(context, error)
}

fn app_type_from_provider_source_app(app: &AppKind) -> ProxyCoreResult<AppType> {
    app.as_str()
        .parse::<AppType>()
        .map_err(unsupported_app_kind_config_error)
}

pub(crate) fn provider_claude_auth_key(
    provider: &Provider,
) -> Option<crate::proxy_core::api::auth::ClaudeAuthKey> {
    extract_claude_auth_key_from_settings(&provider.settings_config)
}

pub(crate) fn provider_kind_from_provider(provider: &Provider) -> Option<ProviderKind> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        .map(ProviderKind::from)
}

pub(crate) fn provider_managed_auth_classification(
    provider: &Provider,
) -> crate::proxy_core::api::auth::ProviderManagedAuthClassification {
    let provider_kind = provider_kind_from_provider(provider);
    classify_provider_managed_auth(ProviderManagedAuthFacts {
        provider_kind: provider_kind.as_ref(),
        anthropic_base_url: provider
            .settings_config
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(serde_json::Value::as_str),
    })
}

pub(crate) fn provider_is_codex_oauth(provider: &Provider) -> bool {
    provider_managed_auth_classification(provider).is_codex_oauth
}

fn provider_codex_config_text(provider: &Provider) -> Option<&str> {
    codex_config_text_from_settings(&provider.settings_config)
}

fn with_provider_codex_chat_completions_facts<T>(
    provider: &Provider,
    evaluate: impl FnOnce(CodexProviderChatCompletionsFacts<'_>) -> T,
) -> T {
    let config_text = provider_codex_config_text(provider);
    let wire_api = config_text.and_then(codex_wire_api_from_config_toml);
    let config_base_url = config_text.and_then(crate::codex_config::extract_codex_base_url);

    evaluate(CodexProviderChatCompletionsFacts {
        api_format: provider
            .meta
            .as_ref()
            .and_then(|meta| meta.api_format.as_deref())
            .or_else(|| {
                provider
                    .settings_config
                    .get("api_format")
                    .and_then(Value::as_str)
            })
            .or_else(|| {
                provider
                    .settings_config
                    .get("apiFormat")
                    .and_then(Value::as_str)
            }),
        wire_api: wire_api.as_deref(),
        base_url: provider
            .settings_config
            .get("base_url")
            .or_else(|| provider.settings_config.get("baseURL"))
            .and_then(Value::as_str),
        config_base_url: config_base_url.as_deref(),
    })
}

pub(crate) fn provider_codex_responses_to_chat_conversion_required(
    provider: &Provider,
    endpoint: &str,
) -> bool {
    with_provider_codex_chat_completions_facts(provider, |provider_facts| {
        codex_responses_to_chat_conversion_required(CodexResponsesToChatConversionFacts {
            provider: provider_facts,
            endpoint,
        })
    })
}

pub(crate) fn provider_claude_base_url(provider: &Provider) -> Option<String> {
    extract_claude_base_url_from_settings(
        provider_is_codex_oauth(provider),
        &provider.settings_config,
    )
}

pub(crate) fn provider_gemini_kind(provider: &Provider) -> ProviderKind {
    if extract_gemini_api_key_from_settings(&provider.settings_config)
        .as_deref()
        .map(is_gemini_oauth_key_shape)
        .unwrap_or(false)
    {
        ProviderKind::GeminiCli
    } else {
        ProviderKind::Gemini
    }
}

pub(crate) fn provider_claude_kind(provider: &Provider) -> ProviderKind {
    let api_format = claude_provider_api_format(provider);
    let uses_google_oauth = provider_claude_auth_key(provider)
        .map(|auth_key| is_gemini_oauth_key_shape(&auth_key.key))
        .unwrap_or(false);
    let meta_provider_type = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref());
    let base_url = provider_claude_base_url(provider);

    infer_claude_provider_kind(
        api_format,
        uses_google_oauth,
        meta_provider_type,
        base_url.as_deref(),
        &provider.settings_config,
    )
}

pub(crate) fn provider_kind_from_app_type_and_config(
    app_type: &AppType,
    provider: &Provider,
) -> ProviderKind {
    match app_type {
        AppType::Claude | AppType::ClaudeDesktop => provider_claude_kind(provider),
        AppType::Codex => ProviderKind::Codex,
        AppType::Gemini => provider_gemini_kind(provider),
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => ProviderKind::Codex,
    }
}

fn provider_metadata_without_secrets(provider: &Provider) -> ProviderMetadata {
    let meta = provider.meta.as_ref();
    provider_metadata_from_input(ProviderMetadataInput {
        website_url: provider.website_url.clone(),
        category: provider.category.clone(),
        sort_index: provider.sort_index,
        notes: provider.notes.clone(),
        icon: provider.icon.clone(),
        icon_color: provider.icon_color.clone(),
        in_failover_queue: provider.in_failover_queue,
        provider_type: meta.and_then(|meta| meta.provider_type.clone()),
        api_format: meta.and_then(|meta| meta.api_format.clone()),
        auth_binding: meta
            .and_then(|meta| meta.auth_binding.as_ref())
            .map(|binding| json!(binding)),
        endpoint_auto_select: meta.and_then(|meta| meta.endpoint_auto_select),
        custom_endpoint_count: meta.map(|meta| meta.custom_endpoints.len()).unwrap_or(0),
    })
}

fn provider_managed_account_binding_input(
    meta: &ProviderMeta,
) -> Option<ManagedAccountBindingInput<'_>> {
    let binding = meta.auth_binding.as_ref()?;
    Some(ManagedAccountBindingInput {
        source: match binding.source {
            ProviderAuthBindingSource::ProviderConfig => {
                ManagedAccountBindingSource::ProviderConfig
            }
            ProviderAuthBindingSource::ManagedAccount => {
                ManagedAccountBindingSource::ManagedAccount
            }
        },
        auth_provider: binding.auth_provider.as_deref(),
        account_id: binding.account_id.as_deref(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderManagedAccountBindingContext<'a> {
    pub binding: Option<ManagedAccountBindingInput<'a>>,
    pub legacy_github_copilot_account_id: Option<&'a str>,
}

pub(crate) fn provider_managed_account_binding_context(
    provider: &Provider,
) -> ProviderManagedAccountBindingContext<'_> {
    let meta = provider.meta.as_ref();
    ProviderManagedAccountBindingContext {
        binding: meta.and_then(provider_managed_account_binding_input),
        legacy_github_copilot_account_id: meta.and_then(|meta| meta.github_account_id.as_deref()),
    }
}

fn managed_account_id_for_provider(provider: &Provider, auth_provider: &str) -> Option<String> {
    let context = provider_managed_account_binding_context(provider);
    managed_account_id_for_auth_provider(
        auth_provider,
        context.binding,
        context.legacy_github_copilot_account_id,
    )
}

pub(crate) fn provider_github_copilot_managed_account_id(provider: &Provider) -> Option<String> {
    managed_account_id_for_provider(provider, GITHUB_COPILOT_AUTH_PROVIDER)
}

pub(crate) fn provider_needs_claude_transform(provider: &Provider) -> bool {
    claude_provider_transform_required(
        provider_claude_kind(provider).needs_transform(),
        claude_provider_api_format(provider),
    )
}

pub(crate) fn provider_claude_transform_streaming_decision(
    provider: &Provider,
    requested_streaming: bool,
    response_headers: &HeaderMap,
    api_format: &str,
) -> ClaudeTransformStreamingDecision {
    claude_transform_streaming_decision(
        requested_streaming,
        response_headers,
        api_format,
        provider_is_codex_oauth(provider),
    )
}

pub(crate) fn provider_uses_anthropic_rectifiers(app_type: &AppType, provider: &Provider) -> bool {
    provider_kind_from_app_type_and_config(app_type, provider).uses_anthropic_rectifiers()
}

fn account_ref(provider: &Provider) -> Option<String> {
    provider.meta.as_ref().and_then(|meta| {
        let provider_type = meta.provider_type.as_deref();
        let account_id = provider_type
            .and_then(|provider_type| managed_account_id_for_provider(provider, provider_type));
        provider_account_ref(provider_type, account_id.as_deref())
    })
}

pub(crate) fn proxy_provider_to_core_spec(provider: &Provider, app_type: &AppType) -> ProviderSpec {
    let kind = provider_kind_from_app_type_and_config(app_type, provider);
    let metadata = provider_metadata_without_secrets(provider);

    ProviderSpec {
        id: provider.id.clone(),
        name: provider.name.clone(),
        kind,
        account_ref: account_ref(provider),
        metadata,
    }
}

pub(crate) fn proxy_providers_to_core_specs(
    providers: impl IntoIterator<Item = Provider>,
    app_type: &AppType,
) -> Vec<ProviderSpec> {
    providers
        .into_iter()
        .map(|provider| proxy_provider_to_core_spec(&provider, app_type))
        .collect()
}

pub(crate) fn provider_specs_from_source(
    app: &AppKind,
    providers: impl IntoIterator<Item = Provider>,
) -> ProxyCoreResult<Vec<ProviderSpec>> {
    let app_type = app_type_from_provider_source_app(app)?;
    Ok(proxy_providers_to_core_specs(providers, &app_type))
}

pub(crate) fn provider_specs_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<Vec<ProviderSpec>> {
    let providers = db
        .get_all_providers(app.as_str())
        .map_err(|error| provider_projection_error("list providers", error))?;
    provider_specs_from_source(app, providers.into_values())
}

pub(crate) fn provider_spec_from_source(
    app: &AppKind,
    provider: Option<Provider>,
) -> ProxyCoreResult<Option<ProviderSpec>> {
    let app_type = app_type_from_provider_source_app(app)?;
    Ok(provider.map(|provider| proxy_provider_to_core_spec(&provider, &app_type)))
}

pub(crate) fn provider_spec_from_db_source(
    db: &Database,
    app: &AppKind,
    provider_id: &str,
) -> ProxyCoreResult<Option<ProviderSpec>> {
    let provider = db
        .get_provider_by_id(provider_id, app.as_str())
        .map_err(|error| provider_projection_error("get provider", error))?;
    provider_spec_from_source(app, provider)
}
