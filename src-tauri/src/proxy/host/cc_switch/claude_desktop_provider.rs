//! CC Switch Claude Desktop provider projection helpers.

use crate::provider::{ClaudeDesktopMode, ClaudeDesktopModelRoute, Provider};
use crate::proxy_core::api::auth::{
    claude_desktop_direct_gateway_credentials, claude_desktop_direct_inference_model_specs,
    claude_desktop_direct_provider_validation_issue, claude_desktop_gateway_profile,
    claude_desktop_provider_models_are_profile_safe, claude_desktop_proxy_has_base_url_and_key,
    claude_desktop_proxy_model_routes, claude_desktop_proxy_provider_config_validation_issue,
    claude_desktop_proxy_request_body_with_upstream_model, claude_desktop_suggested_proxy_routes,
    ClaudeDesktopDirectGatewayCredentialIssue, ClaudeDesktopDirectModelRouteIssue,
    ClaudeDesktopDirectProviderValidationIssue, ClaudeDesktopGatewayProfileModelSpec,
    ClaudeDesktopProviderValidationInput, ClaudeDesktopProxyProviderConfigValidationIssue,
    ClaudeDesktopProxyRequestBodyIssue, ClaudeDesktopProxyRouteInput,
    ClaudeDesktopResolvedProxyRoute,
};
use serde_json::Value;
use std::collections::HashMap;

fn provider_claude_models_are_claude_safe(provider: &Provider) -> bool {
    claude_desktop_provider_models_are_profile_safe(&provider.settings_config)
}

pub(crate) fn provider_claude_desktop_suggested_proxy_routes(
    provider: &Provider,
) -> Option<HashMap<String, ClaudeDesktopModelRoute>> {
    let routes = claude_desktop_suggested_proxy_routes(
        &provider.settings_config,
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.provider_type.as_deref()),
    );

    (!routes.is_empty()).then(|| {
        routes
            .into_iter()
            .map(|route| {
                (
                    route.route_id,
                    ClaudeDesktopModelRoute {
                        model: route.upstream_model,
                        label_override: route.label_override,
                        supports_1m: Some(route.supports_1m),
                    },
                )
            })
            .collect()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderImportDecision {
    Direct,
    Proxy(HashMap<String, ClaudeDesktopModelRoute>),
    Skip,
}

pub(crate) fn provider_claude_desktop_import_decision(
    provider: &Provider,
) -> ClaudeDesktopProviderImportDecision {
    if provider_claude_desktop_direct_importable(provider) {
        return ClaudeDesktopProviderImportDecision::Direct;
    }

    provider_claude_desktop_suggested_proxy_routes(provider)
        .map(ClaudeDesktopProviderImportDecision::Proxy)
        .unwrap_or(ClaudeDesktopProviderImportDecision::Skip)
}

fn provider_claude_desktop_direct_importable(provider: &Provider) -> bool {
    if !provider_claude_models_are_claude_safe(provider) {
        return false;
    }

    provider_claude_desktop_direct_provider_validation(provider).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderDirectValidationIssue {
    Provider(ClaudeDesktopDirectProviderValidationIssue),
    ModelRoute(ClaudeDesktopDirectModelRouteIssue),
    Credentials(ClaudeDesktopDirectGatewayCredentialIssue),
}

pub(crate) fn provider_claude_desktop_direct_provider_validation(
    provider: &Provider,
) -> Result<(), ClaudeDesktopProviderDirectValidationIssue> {
    if let Some(issue) = provider_claude_desktop_direct_validation_issue(provider) {
        return Err(ClaudeDesktopProviderDirectValidationIssue::Provider(issue));
    }

    provider_claude_desktop_direct_inference_model_specs(provider)
        .map_err(ClaudeDesktopProviderDirectValidationIssue::ModelRoute)?;
    claude_desktop_direct_gateway_credentials(&provider.settings_config)
        .map_err(ClaudeDesktopProviderDirectValidationIssue::Credentials)?;

    Ok(())
}

fn provider_claude_desktop_direct_inference_model_specs(
    provider: &Provider,
) -> Result<Vec<ClaudeDesktopGatewayProfileModelSpec>, ClaudeDesktopDirectModelRouteIssue> {
    let route_inputs = provider
        .meta
        .as_ref()
        .into_iter()
        .flat_map(|meta| meta.claude_desktop_model_routes.iter())
        .map(|(route_id, route)| ClaudeDesktopProxyRouteInput {
            route_id,
            upstream_model: &route.model,
            label_override: route.label_override.as_deref(),
            supports_1m: route.supports_1m.unwrap_or(false),
        });

    claude_desktop_direct_inference_model_specs(route_inputs).map(|specs| {
        specs
            .into_iter()
            .map(ClaudeDesktopGatewayProfileModelSpec::from)
            .collect()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderDirectGatewayProfileIssue {
    Credentials(ClaudeDesktopDirectGatewayCredentialIssue),
    ModelRoute(ClaudeDesktopDirectModelRouteIssue),
}

pub(crate) fn provider_claude_desktop_direct_gateway_profile(
    provider: &Provider,
) -> Result<Value, ClaudeDesktopProviderDirectGatewayProfileIssue> {
    let credentials = claude_desktop_direct_gateway_credentials(&provider.settings_config)
        .map_err(ClaudeDesktopProviderDirectGatewayProfileIssue::Credentials)?;
    let model_specs = provider_claude_desktop_direct_inference_model_specs(provider)
        .map_err(ClaudeDesktopProviderDirectGatewayProfileIssue::ModelRoute)?;

    Ok(claude_desktop_gateway_profile(
        &credentials.base_url,
        &credentials.api_key,
        (!model_specs.is_empty()).then_some(model_specs.as_slice()),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderProxyRouteIssue {
    Missing,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderProxyValidationIssue {
    Config(ClaudeDesktopProxyProviderConfigValidationIssue),
    ModelRoutes(ClaudeDesktopProviderProxyRouteIssue),
    CredentialsMissing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderValidationIssue {
    Direct(ClaudeDesktopProviderDirectValidationIssue),
    Proxy(ClaudeDesktopProviderProxyValidationIssue),
}

pub(crate) fn provider_claude_desktop_proxy_model_routes(
    provider: &Provider,
) -> Result<Vec<ClaudeDesktopResolvedProxyRoute>, ClaudeDesktopProviderProxyRouteIssue> {
    let routes = provider
        .meta
        .as_ref()
        .map(|meta| &meta.claude_desktop_model_routes)
        .ok_or(ClaudeDesktopProviderProxyRouteIssue::Missing)?;

    let result = claude_desktop_proxy_model_routes(routes.iter().map(|(route_id, route)| {
        ClaudeDesktopProxyRouteInput {
            route_id,
            upstream_model: &route.model,
            label_override: route.label_override.as_deref(),
            supports_1m: route.supports_1m.unwrap_or(false),
        }
    }));

    if result.is_empty() {
        return Err(ClaudeDesktopProviderProxyRouteIssue::Empty);
    }

    Ok(result)
}

fn provider_claude_desktop_proxy_provider_validation(
    provider: &Provider,
) -> Result<(), ClaudeDesktopProviderProxyValidationIssue> {
    if let Some(issue) = provider_claude_desktop_proxy_config_validation_issue(provider) {
        return Err(ClaudeDesktopProviderProxyValidationIssue::Config(issue));
    }

    provider_claude_desktop_proxy_model_routes(provider)
        .map_err(ClaudeDesktopProviderProxyValidationIssue::ModelRoutes)?;

    if !provider_claude_desktop_proxy_has_base_url_and_key(provider) {
        return Err(ClaudeDesktopProviderProxyValidationIssue::CredentialsMissing);
    }

    Ok(())
}

pub(crate) fn provider_claude_desktop_provider_validation(
    provider: &Provider,
) -> Result<(), ClaudeDesktopProviderValidationIssue> {
    match provider_claude_desktop_mode(provider) {
        ClaudeDesktopMode::Direct => provider_claude_desktop_direct_provider_validation(provider)
            .map_err(ClaudeDesktopProviderValidationIssue::Direct),
        ClaudeDesktopMode::Proxy => provider_claude_desktop_proxy_provider_validation(provider)
            .map_err(ClaudeDesktopProviderValidationIssue::Proxy),
    }
}

pub(crate) fn provider_claude_desktop_proxy_gateway_profile_model_specs(
    provider: &Provider,
) -> Result<Vec<ClaudeDesktopGatewayProfileModelSpec>, ClaudeDesktopProviderProxyRouteIssue> {
    provider_claude_desktop_proxy_model_routes(provider).map(|routes| {
        routes
            .into_iter()
            .map(|route| ClaudeDesktopGatewayProfileModelSpec {
                name: route.route_id,
                label_override: route.label_override,
                supports_1m: route.supports_1m,
            })
            .collect()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudeDesktopProviderProxyRequestBodyIssue {
    Routes(ClaudeDesktopProviderProxyRouteIssue),
    Body(ClaudeDesktopProxyRequestBodyIssue),
}

pub(crate) fn provider_claude_desktop_proxy_request_body(
    body: Value,
    provider: &Provider,
) -> Result<Value, ClaudeDesktopProviderProxyRequestBodyIssue> {
    let routes = provider_claude_desktop_proxy_model_routes(provider)
        .map_err(ClaudeDesktopProviderProxyRequestBodyIssue::Routes)?;
    let raw_routes = provider
        .meta
        .as_ref()
        .into_iter()
        .flat_map(|meta| meta.claude_desktop_model_routes.iter())
        .map(|(route_id, route)| ClaudeDesktopProxyRouteInput {
            route_id,
            upstream_model: &route.model,
            label_override: route.label_override.as_deref(),
            supports_1m: route.supports_1m.unwrap_or(false),
        });
    let api_format = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.api_format.as_deref());

    claude_desktop_proxy_request_body_with_upstream_model(
        body,
        &provider.settings_config,
        api_format,
        &routes,
        raw_routes,
    )
    .map_err(ClaudeDesktopProviderProxyRequestBodyIssue::Body)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClaudeDesktopProviderStatusFacts {
    pub mode: ClaudeDesktopMode,
    pub expected_base_url: Option<String>,
    pub missing_route_mappings: bool,
}

pub(crate) fn provider_claude_desktop_status_facts(
    provider: &Provider,
    proxy_gateway_base_url: impl FnOnce() -> Option<String>,
) -> ClaudeDesktopProviderStatusFacts {
    let mode = provider_claude_desktop_mode(provider);
    let expected_base_url = match mode {
        ClaudeDesktopMode::Proxy => proxy_gateway_base_url(),
        ClaudeDesktopMode::Direct => {
            claude_desktop_direct_gateway_credentials(&provider.settings_config)
                .ok()
                .map(|credentials| credentials.base_url)
        }
    };
    let missing_route_mappings = matches!(mode, ClaudeDesktopMode::Proxy)
        && provider_claude_desktop_proxy_routes_missing(provider);

    ClaudeDesktopProviderStatusFacts {
        mode,
        expected_base_url,
        missing_route_mappings,
    }
}

pub(crate) fn provider_claude_desktop_mode(provider: &Provider) -> ClaudeDesktopMode {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.claude_desktop_mode.clone())
        .unwrap_or(ClaudeDesktopMode::Direct)
}

fn provider_claude_desktop_proxy_routes_missing(provider: &Provider) -> bool {
    provider_claude_desktop_proxy_model_routes(provider).is_err()
}

fn provider_claude_desktop_proxy_has_base_url_and_key(provider: &Provider) -> bool {
    claude_desktop_proxy_has_base_url_and_key(claude_desktop_provider_validation_input(provider))
}

fn provider_claude_desktop_direct_validation_issue(
    provider: &Provider,
) -> Option<ClaudeDesktopDirectProviderValidationIssue> {
    claude_desktop_direct_provider_validation_issue(claude_desktop_provider_validation_input(
        provider,
    ))
}

fn provider_claude_desktop_proxy_config_validation_issue(
    provider: &Provider,
) -> Option<ClaudeDesktopProxyProviderConfigValidationIssue> {
    claude_desktop_proxy_provider_config_validation_issue(claude_desktop_provider_validation_input(
        provider,
    ))
}

fn claude_desktop_provider_validation_input(
    provider: &Provider,
) -> ClaudeDesktopProviderValidationInput<'_> {
    let meta = provider.meta.as_ref();
    ClaudeDesktopProviderValidationInput {
        settings_config: &provider.settings_config,
        api_format: meta.and_then(|meta| meta.api_format.as_deref()),
        claude_desktop_mode_is_proxy: meta.is_some_and(|meta| {
            matches!(
                meta.claude_desktop_mode.as_ref(),
                Some(ClaudeDesktopMode::Proxy)
            )
        }),
        provider_type: meta.and_then(|meta| meta.provider_type.as_deref()),
        is_full_url: meta.and_then(|meta| meta.is_full_url).unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ClaudeDesktopModelRoute, ProviderMeta};
    use serde_json::json;

    #[test]
    fn claude_desktop_proxy_credentials_preserve_oauth_key_policy() {
        let proxy_provider = Provider::with_id(
            "proxy".to_string(),
            "Proxy".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-provider"
                }
            }),
            None,
        );
        assert!(provider_claude_desktop_proxy_has_base_url_and_key(
            &proxy_provider
        ));

        let missing_key = Provider::with_id(
            "missing-key".to_string(),
            "Missing Key".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.example.com"
                }
            }),
            None,
        );
        assert!(!provider_claude_desktop_proxy_has_base_url_and_key(
            &missing_key
        ));

        let mut typed_oauth = missing_key.clone();
        typed_oauth.meta = Some(ProviderMeta {
            provider_type: Some("codex_oauth".to_string()),
            ..Default::default()
        });
        assert!(provider_claude_desktop_proxy_has_base_url_and_key(
            &typed_oauth
        ));

        let url_heuristic_only = Provider::with_id(
            "chatgpt-url".to_string(),
            "ChatGPT URL".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex"
                }
            }),
            None,
        );
        assert!(!provider_claude_desktop_proxy_has_base_url_and_key(
            &url_heuristic_only
        ));
    }

    #[test]
    fn claude_desktop_import_decision_selects_direct_proxy_and_skip() {
        let direct_provider = Provider::with_id(
            "direct".to_string(),
            "Direct".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-direct"
                }
            }),
            None,
        );
        assert_eq!(
            provider_claude_desktop_import_decision(&direct_provider),
            ClaudeDesktopProviderImportDecision::Direct
        );

        let proxy_provider = Provider::with_id(
            "proxy".to_string(),
            "Proxy".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "kimi-k2"
                }
            }),
            None,
        );
        let ClaudeDesktopProviderImportDecision::Proxy(routes) =
            provider_claude_desktop_import_decision(&proxy_provider)
        else {
            panic!("expected proxy import decision");
        };
        assert_eq!(
            routes.get("claude-sonnet-4-6").expect("sonnet route").model,
            "kimi-k2"
        );

        let skipped_provider = Provider::with_id(
            "skip".to_string(),
            "Skip".to_string(),
            json!({"env": {}}),
            None,
        );
        assert_eq!(
            provider_claude_desktop_import_decision(&skipped_provider),
            ClaudeDesktopProviderImportDecision::Skip
        );
    }

    #[test]
    fn claude_desktop_status_facts_project_mode_url_and_missing_routes() {
        let direct_provider = Provider::with_id(
            "direct".to_string(),
            "Direct".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-direct"
                }
            }),
            None,
        );
        let direct = provider_claude_desktop_status_facts(&direct_provider, || {
            Some("http://127.0.0.1:15721/claude-desktop".to_string())
        });
        assert_eq!(direct.mode, ClaudeDesktopMode::Direct);
        assert_eq!(
            direct.expected_base_url.as_deref(),
            Some("https://api.anthropic.com")
        );
        assert!(!direct.missing_route_mappings);

        let mut proxy_provider =
            Provider::with_id("proxy".to_string(), "Proxy".to_string(), json!({}), None);
        proxy_provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            claude_desktop_model_routes: HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "kimi-k2".to_string(),
                    label_override: None,
                    supports_1m: Some(false),
                },
            )]),
            ..Default::default()
        });
        let proxy = provider_claude_desktop_status_facts(&proxy_provider, || {
            Some("http://127.0.0.1:15721/claude-desktop".to_string())
        });
        assert_eq!(proxy.mode, ClaudeDesktopMode::Proxy);
        assert_eq!(
            proxy.expected_base_url.as_deref(),
            Some("http://127.0.0.1:15721/claude-desktop")
        );
        assert!(!proxy.missing_route_mappings);

        let mut missing_routes = proxy_provider.clone();
        missing_routes
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes
            .clear();
        let missing = provider_claude_desktop_status_facts(&missing_routes, || None);
        assert_eq!(missing.mode, ClaudeDesktopMode::Proxy);
        assert!(missing.expected_base_url.is_none());
        assert!(missing.missing_route_mappings);
    }

    #[test]
    fn claude_desktop_validation_projects_config_issues() {
        let non_object = Provider::with_id(
            "bad-settings".to_string(),
            "Bad Settings".to_string(),
            Value::Null,
            None,
        );
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&non_object),
            Some(ClaudeDesktopDirectProviderValidationIssue::SettingsNotObject)
        );
        assert_eq!(
            provider_claude_desktop_proxy_config_validation_issue(&non_object),
            Some(ClaudeDesktopProxyProviderConfigValidationIssue::SettingsNotObject)
        );

        let mut direct_openai = Provider::with_id(
            "direct-openai".to_string(),
            "Direct OpenAI".to_string(),
            json!({}),
            None,
        );
        direct_openai.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&direct_openai),
            Some(ClaudeDesktopDirectProviderValidationIssue::ApiFormatUnsupported)
        );
        assert_eq!(
            provider_claude_desktop_proxy_config_validation_issue(&direct_openai),
            None
        );

        let mut direct_proxy_mode = direct_openai.clone();
        direct_proxy_mode.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&direct_proxy_mode),
            Some(ClaudeDesktopDirectProviderValidationIssue::ProxyModeUnsupported)
        );

        let mut direct_managed = direct_openai.clone();
        direct_managed.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            provider_type: Some("github_copilot".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&direct_managed),
            Some(ClaudeDesktopDirectProviderValidationIssue::ManagedProviderTypeUnsupported)
        );

        let mut direct_full_url = direct_openai.clone();
        direct_full_url.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            is_full_url: Some(true),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_direct_validation_issue(&direct_full_url),
            Some(ClaudeDesktopDirectProviderValidationIssue::FullUrlUnsupported)
        );

        let mut proxy_bad_format = direct_openai.clone();
        proxy_bad_format.meta = Some(ProviderMeta {
            api_format: Some("unsupported_wire".to_string()),
            ..Default::default()
        });
        assert_eq!(
            provider_claude_desktop_proxy_config_validation_issue(&proxy_bad_format),
            Some(
                ClaudeDesktopProxyProviderConfigValidationIssue::ApiFormatUnsupported(
                    "unsupported_wire".to_string()
                )
            )
        );
    }
}
