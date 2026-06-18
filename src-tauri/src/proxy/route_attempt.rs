//! Forward attempts derived from provider or channel routing.
//!
//! This keeps the existing forwarding pipeline provider-shaped while allowing
//! materialized channels to become the live routing unit.

use crate::app_config::AppType;
use crate::provider::{Provider, ProviderMeta};
use crate::proxy::channel_routing::ChannelRouteCandidate;
use serde_json::{Map, Value};

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

pub(crate) fn apply_channel_model_override(body: &mut Value, attempt: &ForwardAttempt) {
    let Some(channel) = attempt.channel() else {
        return;
    };
    let Some(upstream_model) = channel.upstream_model.as_deref() else {
        return;
    };
    let Some(current_model) = body.get("model").and_then(Value::as_str) else {
        return;
    };

    let should_override = channel
        .public_model
        .as_deref()
        .map(|public_model| current_model == public_model)
        .unwrap_or(false)
        || current_model == upstream_model;

    if should_override && current_model != upstream_model {
        log::debug!(
            "[ChannelRoute] model override via channel {}: {} -> {}",
            channel.channel_id,
            current_model,
            upstream_model
        );
        body["model"] = Value::String(upstream_model.to_string());
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

            if let Some(api_format) = claude_api_format_for_interface(&candidate.interface_kind) {
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
            if let Some(api_format) = codex_api_format_for_interface(&candidate.interface_kind) {
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

fn claude_api_format_for_interface(interface_kind: &str) -> Option<&'static str> {
    match interface_kind {
        "anthropic_messages" => Some("anthropic"),
        "openai_chat_completions" => Some("openai_chat"),
        "openai_responses" => Some("openai_responses"),
        "gemini_native" => Some("gemini_native"),
        _ => None,
    }
}

fn codex_api_format_for_interface(interface_kind: &str) -> Option<&'static str> {
    match interface_kind {
        "openai_chat_completions" => Some("openai_chat"),
        "openai_responses" => Some("openai_responses"),
        _ => None,
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
