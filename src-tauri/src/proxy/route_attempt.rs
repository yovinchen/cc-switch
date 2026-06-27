//! Forward attempts derived from provider or channel routing.
//!
//! This keeps the existing forwarding pipeline provider-shaped while allowing
//! materialized channels to become the live routing unit.

use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy_core::api::routing::{
    default_route_candidate_from_selection, resolved_channel_attempt_from_selection,
    route_plan_selections, ResolvedChannelAttempt, RoutePlan, RouteSelection,
};
#[cfg(test)]
use crate::proxy_core::api::routing::{
    resolved_channel_attempt_from_candidate, ChannelRouteCandidate,
};
#[cfg(test)]
use crate::proxy_core::api::transport::apply_resolved_channel_model_override;
use crate::proxy_core_adapter::apply_channel_provider_overrides;
#[cfg(test)]
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(crate) struct ForwardAttempt {
    provider: Provider,
    auth_provider: Option<Provider>,
    channel: Option<ResolvedChannelAttempt>,
    channel_auth_key_ref: Option<String>,
}

impl ForwardAttempt {
    #[cfg(test)]
    pub(crate) fn from_provider(provider: Provider) -> Self {
        Self {
            provider,
            auth_provider: None,
            channel: None,
            channel_auth_key_ref: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_channel(
        app_type: &AppType,
        provider: &Provider,
        candidate: ChannelRouteCandidate,
    ) -> Self {
        let mut provider = provider.clone();
        apply_channel_provider_overrides(app_type, &mut provider, &candidate);

        Self {
            provider,
            auth_provider: None,
            channel: Some(resolved_channel_attempt_from_candidate(candidate)),
            channel_auth_key_ref: None,
        }
    }

    pub(crate) fn from_core_selection(
        app_type: &AppType,
        provider: &Provider,
        selection: &RouteSelection,
    ) -> Self {
        let candidate = default_route_candidate_from_selection(selection);
        let mut provider = provider.clone();
        apply_channel_provider_overrides(app_type, &mut provider, &candidate);

        Self {
            provider,
            auth_provider: None,
            channel: Some(resolved_channel_attempt_from_selection(selection)),
            channel_auth_key_ref: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_resolved_channel_for_test(
        provider: Provider,
        channel: ResolvedChannelAttempt,
    ) -> Self {
        Self {
            provider,
            auth_provider: None,
            channel: Some(channel),
            channel_auth_key_ref: None,
        }
    }

    pub(crate) fn provider(&self) -> &Provider {
        &self.provider
    }

    pub(crate) fn auth_provider(&self) -> &Provider {
        self.auth_provider.as_ref().unwrap_or(&self.provider)
    }

    pub(crate) fn set_auth_provider(&mut self, provider: Provider) {
        self.auth_provider = Some(provider);
        self.channel_auth_key_ref = None;
    }

    pub(crate) fn set_channel_auth_provider(
        &mut self,
        provider: Provider,
        key_ref: impl Into<String>,
    ) {
        self.auth_provider = Some(provider);
        self.channel_auth_key_ref = Some(key_ref.into());
    }

    #[cfg(test)]
    pub(crate) fn set_channel_auth_key_ref(&mut self, key_ref: impl Into<String>) {
        self.channel_auth_key_ref = Some(key_ref.into());
    }

    pub(crate) fn channel_auth_key_ref(&self) -> Option<&str> {
        self.channel_auth_key_ref.as_deref()
    }

    pub(crate) fn channel(&self) -> Option<&ResolvedChannelAttempt> {
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

    route_plan_selections(plan)
        .iter()
        .filter_map(|selection| {
            providers_by_id
                .get(selection.channel.provider_id.as_str())
                .map(|provider| ForwardAttempt::from_core_selection(app_type, provider, selection))
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn apply_channel_model_override(body: &mut serde_json::Value, attempt: &ForwardAttempt) {
    let Some(channel) = attempt.channel() else {
        return;
    };

    if let Some(override_result) = apply_resolved_channel_model_override(body, channel) {
        log::debug!(
            "[ChannelRoute] model override via channel {}: {} -> {}",
            override_result.channel_id,
            override_result.previous_model,
            override_result.upstream_model
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core_adapter::{
        ChannelRouteCandidate, ProviderKind, ProxyCoreAppKind as AppKind,
        ProxyCoreChannelOverrides as ChannelOverrides, ProxyCoreChannelSpec as ChannelSpec,
        ProxyCoreChannelStatus as ChannelStatus, ProxyCoreInterfaceKind as InterfaceKind,
        ProxyCoreModelCapabilities as ModelCapabilities, ProxyCoreModelRoute as ModelRoute,
        ProxyCoreProviderMetadata as ProviderMetadata, ProxyCoreProviderSpec as ProviderSpec,
        ProxyCoreUpstreamEndpoint as UpstreamEndpoint, RoutePlan, RouteSelection,
    };
    use serde_json::json;

    fn auth_profile_ref<T: serde::de::DeserializeOwned>(value: &str) -> T {
        serde_json::from_value(json!(value)).expect("auth profile ref")
    }

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
                pricing_model: Some("sonnet-price".to_string()),
                request_overrides: json!({"temperature": 0.2}),
                response_overrides: json!({"headers": {"x-relay-model": "sonnet"}}),
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
                pricing_model: Some("sonnet-price".to_string()),
                request_overrides: json!({"temperature": 0.2}),
                response_overrides: json!({"headers": {"x-relay-model": "sonnet"}}),
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
    fn channel_attempt_overrides_opencode_like_codex() {
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
            &AppType::OpenCode,
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
    fn channel_attempt_carries_header_and_param_overrides() {
        let provider = Provider::with_id("p1".to_string(), "Provider".to_string(), json!({}), None);
        let mut selection = route_selection("p1", "ch_override");
        selection.channel.auth_profile = Some(auth_profile_ref("channel-key:manual-relay"));
        selection.channel.overrides = ChannelOverrides {
            headers: json!({ "x-relay-profile": "manual" }),
            params: json!({ "api-version": "2026-06-20" }),
            status_code_mapping: json!([{ "from": 429, "to": 503 }]),
            ..ChannelOverrides::default()
        };

        let attempt = ForwardAttempt::from_core_selection(&AppType::Claude, &provider, &selection);
        let channel = attempt.channel().expect("channel attempt");

        assert_eq!(
            channel.auth_profile_ref.as_deref(),
            Some("channel-key:manual-relay")
        );
        assert_eq!(channel.header_overrides["x-relay-profile"], "manual");
        assert_eq!(channel.param_overrides["api-version"], "2026-06-20");
        assert_eq!(channel.status_code_mapping[0]["to"], 503);
        assert_eq!(channel.pricing_model.as_deref(), Some("sonnet-price"));
        assert_eq!(channel.request_overrides["temperature"], json!(0.2));
        assert_eq!(
            channel.response_overrides["headers"]["x-relay-model"],
            "sonnet"
        );
    }

    #[test]
    fn channel_attempt_can_use_separate_auth_provider() {
        let provider = Provider::with_id("p1".to_string(), "Provider".to_string(), json!({}), None);
        let auth_provider = Provider::with_id(
            "auth-provider".to_string(),
            "Auth Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "auth-key" } }),
            None,
        );
        let mut attempt = ForwardAttempt::from_core_selection(
            &AppType::Claude,
            &provider,
            &route_selection("p1", "ch_auth"),
        );

        attempt.set_auth_provider(auth_provider);

        assert_eq!(attempt.provider().id, "p1");
        assert_eq!(attempt.auth_provider().id, "auth-provider");
        assert_eq!(
            attempt
                .auth_provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("auth-key")
        );
    }

    #[test]
    fn channel_auth_key_ref_tracks_selected_key_and_clears_for_provider_auth() {
        let provider = Provider::with_id("p1".to_string(), "Provider".to_string(), json!({}), None);
        let channel_auth_provider = Provider::with_id(
            "p1".to_string(),
            "Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "channel-key" } }),
            None,
        );
        let provider_auth = Provider::with_id(
            "auth-provider".to_string(),
            "Auth Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-key" } }),
            None,
        );
        let mut attempt = ForwardAttempt::from_core_selection(
            &AppType::Claude,
            &provider,
            &route_selection("p1", "ch_auth_key"),
        );

        attempt.set_channel_auth_provider(channel_auth_provider, "primary");
        assert_eq!(attempt.channel_auth_key_ref(), Some("primary"));

        attempt.set_auth_provider(provider_auth);
        assert_eq!(attempt.channel_auth_key_ref(), None);
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
