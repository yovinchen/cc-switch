//! CC Switch provider projection helpers.

use crate::app_config::AppType;
use crate::database::Database;
use crate::error::AppError;
use crate::provider::{AuthBindingSource as ProviderAuthBindingSource, Provider, ProviderMeta};
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
    resolve_claude_api_format_from_settings, ClaudeTransformStreamingDecision,
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

pub(crate) fn provider_claude_api_format(provider: &Provider) -> &'static str {
    let meta = provider.meta.as_ref();
    resolve_claude_api_format_from_settings(
        meta.and_then(|meta| meta.provider_type.as_deref()),
        meta.and_then(|meta| meta.api_format.as_deref()),
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
    let api_format = provider_claude_api_format(provider);
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
        provider_claude_api_format(provider),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{AuthBinding, AuthBindingSource, ProviderTestConfig, UsageScript};
    use crate::proxy_core::api::auth::claude_desktop_provider_models_are_profile_safe;
    use crate::proxy_core::api::transport::{
        bedrock_env_flag_from_provider_settings, UpstreamSseAggregationKind,
    };

    #[test]
    fn managed_account_binding_projection_uses_core_policy() {
        let mut legacy_provider = Provider::with_id(
            "legacy-copilot".to_string(),
            "Legacy Copilot".to_string(),
            json!({}),
            None,
        );
        legacy_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            github_account_id: Some("legacy-acct".to_string()),
            ..ProviderMeta::default()
        });

        assert_eq!(
            provider_github_copilot_managed_account_id(&legacy_provider).as_deref(),
            Some("legacy-acct")
        );
        let legacy_context = provider_managed_account_binding_context(&legacy_provider);
        assert_eq!(legacy_context.binding, None);
        assert_eq!(
            legacy_context.legacy_github_copilot_account_id,
            Some("legacy-acct")
        );
        let legacy_spec = proxy_provider_to_core_spec(&legacy_provider, &AppType::Claude);
        assert_eq!(
            legacy_spec.account_ref.as_deref(),
            Some("github_copilot:legacy-acct")
        );

        let mut default_account_provider = Provider::with_id(
            "default-copilot".to_string(),
            "Default Copilot".to_string(),
            json!({}),
            None,
        );
        default_account_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            github_account_id: Some("legacy-acct".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("github_copilot".to_string()),
                account_id: None,
            }),
            ..ProviderMeta::default()
        });

        assert_eq!(
            provider_github_copilot_managed_account_id(&default_account_provider),
            None
        );
        let default_context = provider_managed_account_binding_context(&default_account_provider);
        let binding = default_context.binding.expect("managed account binding");
        assert_eq!(binding.source, ManagedAccountBindingSource::ManagedAccount);
        assert_eq!(binding.auth_provider, Some("github_copilot"));
        assert_eq!(binding.account_id, None);
        assert_eq!(
            default_context.legacy_github_copilot_account_id,
            Some("legacy-acct")
        );
        let default_spec = proxy_provider_to_core_spec(&default_account_provider, &AppType::Claude);
        assert_eq!(default_spec.account_ref, None);
    }

    #[test]
    fn provider_kind_projection_uses_core_inference_helpers() {
        assert_eq!(
            infer_claude_provider_kind("gemini_native", true, None, None, &json!({})),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            infer_claude_provider_kind(
                "anthropic",
                false,
                Some("github_copilot"),
                Some("https://example.com"),
                &json!({})
            ),
            ProviderKind::GitHubCopilot
        );
        assert!(is_gemini_oauth_key_shape(" ya29.access-token "));
        assert!(is_gemini_oauth_key_shape(
            r#"{"access_token":"ya29.access-token"}"#
        ));
        assert!(!is_gemini_oauth_key_shape("AIza-api-key"));

        let mut gemini_cli_provider = Provider::with_id(
            "gemini-cli".to_string(),
            "Gemini CLI".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": r#"{"access_token":"ya29.access-token"}"#,
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com"
                }
            }),
            None,
        );
        gemini_cli_provider.meta = Some(ProviderMeta {
            api_format: Some("gemini_native".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_kind(&gemini_cli_provider),
            ProviderKind::GeminiCli
        );

        let mut gemini_cli_raw_provider = Provider::with_id(
            "gemini-cli-raw".to_string(),
            "Gemini CLI Raw".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "\nya29.raw-token-value\n",
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com"
                }
            }),
            None,
        );
        gemini_cli_raw_provider.meta = Some(ProviderMeta {
            api_format: Some("gemini_native".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_kind(&gemini_cli_raw_provider),
            ProviderKind::GeminiCli
        );

        let anthropic_provider = Provider::with_id(
            "anthropic".to_string(),
            "Anthropic".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&anthropic_provider),
            ProviderKind::Claude
        );

        let openrouter_provider = Provider::with_id(
            "openrouter".to_string(),
            "OpenRouter".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                    "OPENROUTER_API_KEY": "sk-or-test"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&openrouter_provider),
            ProviderKind::OpenRouter
        );

        let claude_auth_provider = Provider::with_id(
            "claude-auth".to_string(),
            "Claude Auth".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-test"
                },
                "auth_mode": "bearer_only"
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&claude_auth_provider),
            ProviderKind::ClaudeAuth
        );

        let mut copilot_provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "copilot-token",
                    "ANTHROPIC_BASE_URL": "https://example.com"
                }
            }),
            None,
        );
        copilot_provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_kind(&copilot_provider),
            ProviderKind::GitHubCopilot
        );
        assert_eq!(
            provider_kind_from_app_type_and_config(&AppType::Claude, &copilot_provider),
            ProviderKind::GitHubCopilot
        );

        let copilot_url_provider = Provider::with_id(
            "copilot-url".to_string(),
            "Copilot URL".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_kind(&copilot_url_provider),
            ProviderKind::GitHubCopilot
        );

        let gemini_provider = Provider::with_id(
            "gemini-cli".to_string(),
            "Gemini CLI".to_string(),
            json!({
                "env": {
                    "GEMINI_API_KEY": r#"{"access_token":"ya29.access-token"}"#
                }
            }),
            None,
        );
        assert_eq!(
            provider_gemini_kind(&gemini_provider),
            ProviderKind::GeminiCli
        );
        assert_eq!(
            provider_kind_from_app_type_and_config(&AppType::Gemini, &gemini_provider),
            ProviderKind::GeminiCli
        );

        let gemini_api_key_provider = Provider::with_id(
            "gemini-api-key".to_string(),
            "Gemini API Key".to_string(),
            json!({
                "env": {
                    "GEMINI_API_KEY": "AIza-api-key"
                }
            }),
            None,
        );
        assert_eq!(
            provider_gemini_kind(&gemini_api_key_provider),
            ProviderKind::Gemini
        );
    }

    #[test]
    fn provider_spec_projection_redacts_settings_and_projects_managed_facts() {
        let mut provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "secret-token",
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com",
                    "CLAUDE_CODE_USE_BEDROCK": "1"
                }
            }),
            Some("https://github.com/features/copilot".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            is_full_url: Some(true),
            custom_user_agent: Some("cc-switch-test/1.0".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("github_copilot".to_string()),
                account_id: Some("acct-1".to_string()),
            }),
            usage_script: Some(UsageScript {
                enabled: true,
                language: "javascript".to_string(),
                code: String::new(),
                timeout: None,
                api_key: None,
                base_url: None,
                access_token: None,
                user_id: None,
                template_type: Some("github_copilot".to_string()),
                auto_query_interval: None,
                coding_plan_provider: None,
            }),
            test_config: Some(ProviderTestConfig {
                enabled: true,
                timeout_secs: Some(20),
                degraded_threshold_ms: Some(3000),
                max_retries: None,
            }),
            ..ProviderMeta::default()
        });

        let usage_provider_kind = provider_kind_from_provider(&provider);
        let usage_provider_classification = provider_managed_auth_classification(&provider);
        let usage_provider_is_codex_oauth = usage_provider_classification.is_codex_oauth;
        let usage_provider_is_github_copilot = usage_provider_classification.is_github_copilot;
        let usage_provider_uses_managed_account =
            usage_provider_classification.uses_managed_account;
        let usage_provider_needs_claude_transform = provider_needs_claude_transform(&provider);
        let copilot_account_id = provider_github_copilot_managed_account_id(&provider);
        let models_are_claude_safe =
            claude_desktop_provider_models_are_profile_safe(&provider.settings_config);
        let stream_check_timeout_secs = provider
            .enabled_test_config()
            .and_then(|config| config.timeout_secs);
        assert_eq!(
            provider
                .usage_script()
                .and_then(|script| script.template_type.as_deref()),
            Some("github_copilot")
        );
        assert!(Provider::with_id(
            "without-usage".to_string(),
            "Without Usage".to_string(),
            json!({}),
            None,
        )
        .usage_script()
        .is_none());
        let usage_provider_is_full_url = provider.is_full_url();
        assert_eq!(
            bedrock_env_flag_from_provider_settings(&provider.settings_config),
            Some("1")
        );

        let mut codex_provider = Provider::with_id(
            "codex-oauth".to_string(),
            "Codex OAuth".to_string(),
            json!({}),
            None,
        );
        codex_provider.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("codex_oauth".to_string()),
                account_id: Some("codex-acct-1".to_string()),
            }),
            ..ProviderMeta::default()
        });
        let mut claude_auth_provider = Provider::with_id(
            "claude-auth".to_string(),
            "Claude Auth".to_string(),
            json!({}),
            None,
        );
        claude_auth_provider.meta = Some(ProviderMeta {
            provider_type: Some("claude_auth".to_string()),
            ..ProviderMeta::default()
        });

        let spec = proxy_provider_to_core_spec(&provider, &AppType::Claude);
        let source_spec = provider_spec_from_source(&AppKind::Claude, Some(provider.clone()))
            .expect("provider spec")
            .expect("provider");
        let source_specs =
            provider_specs_from_source(&AppKind::Claude, vec![provider]).expect("provider specs");

        assert_eq!(spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(source_spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(source_specs[0].kind, ProviderKind::GitHubCopilot);
        assert_eq!(usage_provider_kind, Some(ProviderKind::GitHubCopilot));
        assert!(!usage_provider_is_codex_oauth);
        assert!(usage_provider_is_github_copilot);
        assert!(usage_provider_uses_managed_account);
        assert!(usage_provider_needs_claude_transform);
        assert_eq!(copilot_account_id.as_deref(), Some("acct-1"));
        assert!(models_are_claude_safe);
        assert_eq!(stream_check_timeout_secs, Some(20));
        assert!(usage_provider_is_full_url);
        assert!(provider_is_codex_oauth(&codex_provider));
        let codex_context = provider_managed_account_binding_context(&codex_provider);
        let codex_binding = codex_context
            .binding
            .expect("codex managed account binding");
        assert_eq!(
            codex_binding.source,
            ManagedAccountBindingSource::ManagedAccount
        );
        assert_eq!(codex_binding.auth_provider, Some("codex_oauth"));
        assert_eq!(codex_binding.account_id, Some("codex-acct-1"));
        assert!(provider_uses_anthropic_rectifiers(
            &AppType::Claude,
            &claude_auth_provider
        ));
        assert!(!provider_uses_anthropic_rectifiers(
            &AppType::Codex,
            &claude_auth_provider
        ));
        assert_eq!(spec.account_ref.as_deref(), Some("github_copilot:acct-1"));
        let serialized = serde_json::to_string(&spec).expect("serialize spec");
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("ANTHROPIC_AUTH_TOKEN"));
        assert!(!serialized.contains("settingsConfig"));
    }

    #[test]
    fn claude_api_format_projection_uses_core_transform_gate() {
        let mut openai_chat_provider = Provider::with_id(
            "claude-openai-chat".to_string(),
            "Claude OpenAI Chat".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            None,
        );
        openai_chat_provider.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });

        assert_eq!(
            provider_claude_api_format(&openai_chat_provider),
            "openai_chat"
        );
        assert!(provider_needs_claude_transform(&openai_chat_provider));

        let mut meta_precedence_provider = Provider::with_id(
            "claude-meta-precedence".to_string(),
            "Claude Meta Precedence".to_string(),
            json!({
                "api_format": "openai_chat",
                "openrouter_compat_mode": true,
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            None,
        );
        meta_precedence_provider.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });

        assert_eq!(
            provider_claude_api_format(&meta_precedence_provider),
            "anthropic"
        );
        assert!(!provider_needs_claude_transform(&meta_precedence_provider));

        let mut codex_oauth_provider = Provider::with_id(
            "codex-oauth".to_string(),
            "Codex OAuth".to_string(),
            json!({"api_format": "openai_chat"}),
            None,
        );
        codex_oauth_provider.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });

        assert_eq!(
            provider_claude_api_format(&codex_oauth_provider),
            "openai_responses"
        );
        assert!(provider_needs_claude_transform(&codex_oauth_provider));

        let unknown_format_provider = Provider::with_id(
            "claude-unknown-format".to_string(),
            "Claude Unknown Format".to_string(),
            json!({"api_format": "unknown"}),
            None,
        );

        assert_eq!(
            provider_claude_api_format(&unknown_format_provider),
            "anthropic"
        );
        assert!(!provider_needs_claude_transform(&unknown_format_provider));
    }

    #[test]
    fn claude_streaming_decision_preserves_codex_oauth_aggregation() {
        let mut codex_provider = Provider::with_id(
            "codex-oauth".to_string(),
            "Codex OAuth".to_string(),
            json!({}),
            None,
        );
        codex_provider.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            ..Default::default()
        });
        let mut sse_headers = HeaderMap::new();
        sse_headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );

        let aggregate_decision = provider_claude_transform_streaming_decision(
            &codex_provider,
            false,
            &sse_headers,
            "openai_responses",
        );
        assert!(!aggregate_decision.use_streaming);
        assert!(aggregate_decision.aggregate_codex_oauth_responses_sse);
        assert_eq!(
            aggregate_decision.response_sse_aggregation,
            Some(UpstreamSseAggregationKind::Responses)
        );

        let streaming_decision = provider_claude_transform_streaming_decision(
            &codex_provider,
            true,
            &HeaderMap::new(),
            "openai_responses",
        );
        assert!(streaming_decision.use_streaming);
        assert!(!streaming_decision.aggregate_codex_oauth_responses_sse);
        assert!(streaming_decision.response_sse_aggregation.is_none());

        let plain_provider =
            Provider::with_id("plain".to_string(), "Plain".to_string(), json!({}), None);
        let upstream_sse_decision = provider_claude_transform_streaming_decision(
            &plain_provider,
            false,
            &sse_headers,
            "openai_chat",
        );
        assert!(upstream_sse_decision.use_streaming);
        assert!(!upstream_sse_decision.aggregate_codex_oauth_responses_sse);
        assert!(upstream_sse_decision.response_sse_aggregation.is_none());

        let non_stream_chat_decision = provider_claude_transform_streaming_decision(
            &plain_provider,
            false,
            &HeaderMap::new(),
            "openai_chat",
        );
        assert!(!non_stream_chat_decision.use_streaming);
        assert!(!non_stream_chat_decision.aggregate_codex_oauth_responses_sse);
        assert_eq!(
            non_stream_chat_decision.response_sse_aggregation,
            Some(UpstreamSseAggregationKind::ChatCompletions)
        );
    }
}
