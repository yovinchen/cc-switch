//! Forward attempts derived from provider or channel routing.
//!
//! This keeps the existing forwarding pipeline provider-shaped while allowing
//! materialized channels to become the live routing unit.

use crate::app_config::AppType;
use crate::provider::{Provider, ProviderMeta};
use crate::proxy_core::{
    apply_channel_route_model_override, claude_api_format_for_interface_kind,
    codex_api_format_for_interface_kind, ChannelRouteCandidate, RoutePlan,
};
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChannelAttempt {
    pub channel_id: String,
    pub channel_name: String,
    pub base_url: String,
    pub interface_kind: String,
    pub public_model: Option<String>,
    pub upstream_model: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ForwardAttempt {
    provider: Provider,
    channel: Option<ChannelAttempt>,
}

impl ForwardAttempt {
    #[allow(dead_code)]
    pub(crate) fn from_provider(provider: Provider) -> Self {
        Self {
            provider,
            channel: None,
        }
    }

    pub(crate) fn from_channel(
        app_type: &AppType,
        provider: &Provider,
        candidate: ChannelRouteCandidate,
    ) -> Self {
        let mut provider = provider.clone();
        apply_channel_provider_overrides(app_type, &mut provider, &candidate);

        Self {
            provider,
            channel: Some(ChannelAttempt {
                channel_id: candidate.channel_id,
                channel_name: candidate.channel_name,
                base_url: candidate.base_url,
                interface_kind: candidate.interface_kind,
                public_model: candidate.public_model,
                upstream_model: candidate.upstream_model,
            }),
        }
    }

    pub(crate) fn from_core_selection(
        app_type: &AppType,
        provider: &Provider,
        selection: &crate::proxy_core::RouteSelection,
    ) -> Self {
        let candidate = ChannelRouteCandidate {
            channel_id: selection.channel.id.clone(),
            provider_id: selection.channel.provider_id.clone(),
            channel_name: selection.channel.name.clone(),
            base_url: selection.channel.endpoint.base_url.clone(),
            interface_kind: selection.channel.interface.as_str().to_string(),
            public_model: selection
                .model_route
                .as_ref()
                .map(|route| route.public_model.clone()),
            upstream_model: selection
                .model_route
                .as_ref()
                .map(|route| route.upstream_model.clone()),
            route_group: "default".to_string(),
            priority: selection.channel.priority,
            weight: selection.channel.weight,
            source_kind: "proxy_core".to_string(),
        };

        Self::from_channel(app_type, provider, candidate)
    }

    pub(crate) fn provider(&self) -> &Provider {
        &self.provider
    }

    pub(crate) fn channel(&self) -> Option<&ChannelAttempt> {
        self.channel.as_ref()
    }

    pub(crate) fn is_channel(&self) -> bool {
        self.channel.is_some()
    }
}

pub(crate) fn forward_attempts_from_route_plan(
    app_type: &AppType,
    providers: &[Provider],
    plan: &RoutePlan,
) -> Vec<ForwardAttempt> {
    let providers_by_id: HashMap<&str, &Provider> = providers
        .iter()
        .map(|provider| (provider.id.as_str(), provider))
        .collect();

    let selections = if plan.selections.is_empty() {
        std::slice::from_ref(&plan.selection)
    } else {
        plan.selections.as_slice()
    };

    selections
        .iter()
        .filter_map(|selection| {
            providers_by_id
                .get(selection.channel.provider_id.as_str())
                .map(|provider| ForwardAttempt::from_core_selection(app_type, provider, selection))
        })
        .collect()
}

pub(crate) fn apply_channel_model_override(body: &mut Value, attempt: &ForwardAttempt) {
    let Some(channel) = attempt.channel() else {
        return;
    };
    let Some(current_model) = body.get("model").and_then(Value::as_str) else {
        return;
    };
    let current_model = current_model.to_string();

    if let Some(upstream_model) = apply_channel_route_model_override(
        body,
        channel.public_model.as_deref(),
        channel.upstream_model.as_deref(),
    ) {
        log::debug!(
            "[ChannelRoute] model override via channel {}: {} -> {}",
            channel.channel_id,
            current_model,
            upstream_model
        );
    }
}

fn apply_channel_provider_overrides(
    app_type: &AppType,
    provider: &mut Provider,
    candidate: &ChannelRouteCandidate,
) {
    match app_type {
        AppType::Claude | AppType::ClaudeDesktop => {
            set_env_value(
                &mut provider.settings_config,
                "ANTHROPIC_BASE_URL",
                &candidate.base_url,
            );
            if let Some(upstream_model) = candidate.upstream_model.as_deref() {
                set_env_value(
                    &mut provider.settings_config,
                    "ANTHROPIC_MODEL",
                    upstream_model,
                );
            }

            if let Some(api_format) =
                claude_api_format_for_interface_kind(&candidate.interface_kind)
            {
                provider
                    .meta
                    .get_or_insert_with(ProviderMeta::default)
                    .api_format = Some(api_format.to_string());
            }
        }
        AppType::Codex | AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => {
            set_object_value(
                &mut provider.settings_config,
                "base_url",
                &candidate.base_url,
            );
            if let Some(api_format) = codex_api_format_for_interface_kind(&candidate.interface_kind)
            {
                provider
                    .meta
                    .get_or_insert_with(ProviderMeta::default)
                    .api_format = Some(api_format.to_string());
            }
        }
        AppType::Gemini => {
            set_env_value(
                &mut provider.settings_config,
                "GOOGLE_GEMINI_BASE_URL",
                &candidate.base_url,
            );
        }
    }
}

fn set_env_value(settings: &mut Value, key: &str, value: &str) {
    let root = ensure_object(settings);
    let env = root
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()));
    ensure_object(env).insert(key.to_string(), Value::String(value.to_string()));
}

fn set_object_value(settings: &mut Value, key: &str, value: &str) {
    ensure_object(settings).insert(key.to_string(), Value::String(value.to_string()));
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("value forced to object")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::{
        AppKind, ChannelOverrides, ChannelSpec, ChannelStatus, InterfaceKind, ModelCapabilities,
        ModelRoute, ProviderKind, ProviderMetadata, ProviderSpec, RoutePlan, RouteSelection,
        UpstreamEndpoint,
    };
    use serde_json::json;

    fn candidate(interface_kind: &str) -> ChannelRouteCandidate {
        ChannelRouteCandidate {
            channel_id: "ch_1".to_string(),
            provider_id: "p1".to_string(),
            channel_name: "Relay".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: interface_kind.to_string(),
            public_model: Some("sonnet-public".to_string()),
            upstream_model: Some("upstream-sonnet".to_string()),
            route_group: "default".to_string(),
            priority: 100,
            weight: 1,
            source_kind: "manual".to_string(),
        }
    }

    fn route_selection(provider_id: &str, channel_id: &str) -> RouteSelection {
        let provider = ProviderSpec {
            id: provider_id.to_string(),
            name: provider_id.to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        };
        let channel = ChannelSpec {
            id: channel_id.to_string(),
            provider_id: provider_id.to_string(),
            app: AppKind::Claude,
            name: channel_id.to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: format!("https://{channel_id}.example.com/v1"),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::OpenAiResponses,
            auth_profile: None,
            models: vec![ModelRoute {
                public_model: "sonnet-public".to_string(),
                upstream_model: "upstream-sonnet".to_string(),
                capabilities: ModelCapabilities::default(),
                pricing_model: None,
                request_overrides: json!({}),
                response_overrides: json!({}),
            }],
            groups: vec!["default".to_string()],
            priority: 100,
            weight: 1,
            retry_policy: Default::default(),
            health_policy: Default::default(),
            overrides: ChannelOverrides::default(),
            tags: Vec::new(),
            metadata: json!({}),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        };

        RouteSelection {
            provider,
            channel,
            model_route: Some(ModelRoute {
                public_model: "sonnet-public".to_string(),
                upstream_model: "upstream-sonnet".to_string(),
                capabilities: ModelCapabilities::default(),
                pricing_model: None,
                request_overrides: json!({}),
                response_overrides: json!({}),
            }),
            inbound_interface: InterfaceKind::AnthropicMessages,
            outbound_interface: InterfaceKind::OpenAiResponses,
        }
    }

    #[test]
    fn channel_attempt_overrides_claude_base_url_api_format_and_model() {
        let provider = Provider::with_id(
            "p1".to_string(),
            "Provider".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://old.example.com/v1",
                    "ANTHROPIC_MODEL": "old-model",
                    "ANTHROPIC_API_KEY": "keep-key"
                }
            }),
            None,
        );

        let attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            candidate("openai_responses"),
        );

        assert_eq!(
            attempt
                .provider()
                .settings_config
                .pointer("/env/ANTHROPIC_BASE_URL")
                .and_then(Value::as_str),
            Some("https://relay.example.com/v1")
        );
        assert_eq!(
            attempt
                .provider()
                .settings_config
                .pointer("/env/ANTHROPIC_MODEL")
                .and_then(Value::as_str),
            Some("upstream-sonnet")
        );
        assert_eq!(
            attempt
                .provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("keep-key")
        );
        assert_eq!(
            attempt
                .provider()
                .meta
                .as_ref()
                .and_then(|meta| meta.api_format.as_deref()),
            Some("openai_responses")
        );
    }

    #[test]
    fn channel_attempt_overrides_codex_base_url_and_api_format() {
        let provider = Provider::with_id(
            "p1".to_string(),
            "Provider".to_string(),
            json!({
                "base_url": "https://old.example.com/v1",
                "api_key": "keep-key"
            }),
            None,
        );

        let attempt = ForwardAttempt::from_channel(
            &AppType::Codex,
            &provider,
            candidate("openai_chat_completions"),
        );

        assert_eq!(
            attempt
                .provider()
                .settings_config
                .get("base_url")
                .and_then(Value::as_str),
            Some("https://relay.example.com/v1")
        );
        assert_eq!(
            attempt
                .provider()
                .settings_config
                .get("api_key")
                .and_then(Value::as_str),
            Some("keep-key")
        );
        assert_eq!(
            attempt
                .provider()
                .meta
                .as_ref()
                .and_then(|meta| meta.api_format.as_deref()),
            Some("openai_chat")
        );
    }

    #[test]
    fn route_plan_mapping_uses_matching_host_providers_only() {
        let provider = Provider::with_id("p1".to_string(), "Provider".to_string(), json!({}), None);
        let matching = route_selection("p1", "ch_matching");
        let missing = route_selection("missing-provider", "ch_missing");
        let plan = RoutePlan {
            selection: matching.clone(),
            selections: vec![matching, missing],
            attempts: Vec::new(),
        };

        let attempts = forward_attempts_from_route_plan(&AppType::Claude, &[provider], &plan);

        assert_eq!(attempts.len(), 1);
        let attempt = &attempts[0];
        assert_eq!(attempt.provider().id, "p1");
        assert_eq!(
            attempt.channel().map(|channel| channel.channel_id.as_str()),
            Some("ch_matching")
        );
        assert_eq!(
            attempt
                .provider()
                .settings_config
                .pointer("/env/ANTHROPIC_BASE_URL")
                .and_then(Value::as_str),
            Some("https://ch_matching.example.com/v1")
        );
    }

    #[test]
    fn route_plan_mapping_falls_back_to_primary_selection() {
        let provider = Provider::with_id("p1".to_string(), "Provider".to_string(), json!({}), None);
        let plan = RoutePlan {
            selection: route_selection("p1", "ch_primary"),
            selections: Vec::new(),
            attempts: Vec::new(),
        };

        let attempts = forward_attempts_from_route_plan(&AppType::Claude, &[provider], &plan);

        assert_eq!(attempts.len(), 1);
        assert_eq!(
            attempts[0]
                .channel()
                .map(|channel| channel.channel_id.as_str()),
            Some("ch_primary")
        );
    }

    #[test]
    fn channel_model_override_rewrites_public_model_only() {
        let provider = Provider::with_id("p1".to_string(), "Provider".to_string(), json!({}), None);
        let attempt =
            ForwardAttempt::from_channel(&AppType::Codex, &provider, candidate("openai_responses"));
        let mut body = json!({ "model": "sonnet-public" });

        apply_channel_model_override(&mut body, &attempt);

        assert_eq!(body["model"], "upstream-sonnet");

        let mut unrelated = json!({ "model": "other-model" });
        apply_channel_model_override(&mut unrelated, &attempt);
        assert_eq!(unrelated["model"], "other-model");
    }
}
