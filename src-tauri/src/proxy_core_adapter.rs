use crate::app_config::AppType;
use crate::database::{ProxyChannelModelRecord, ProxyChannelRecord};
use crate::provider::Provider;
use crate::proxy::providers::ProviderType;
use crate::proxy_core::{
    AppKind, AuthProfileRef, ChannelHealthPolicy, ChannelModelRecord, ChannelOverrides,
    ChannelRecord, ChannelSpec, ChannelStatus, InterfaceKind, ModelCapabilities, ModelRoute,
    ProviderKind, ProviderMetadata, ProviderSpec, RetryPolicy, UpstreamEndpoint,
};
use serde_json::{json, Value};

impl From<&AppType> for AppKind {
    fn from(value: &AppType) -> Self {
        match value {
            AppType::Claude => Self::Claude,
            AppType::ClaudeDesktop => Self::ClaudeDesktop,
            AppType::Codex => Self::Codex,
            AppType::Gemini => Self::Gemini,
            AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => {
                Self::Custom(value.as_str().to_string())
            }
        }
    }
}

impl From<ProviderType> for ProviderKind {
    fn from(value: ProviderType) -> Self {
        match value {
            ProviderType::Claude => Self::Claude,
            ProviderType::ClaudeAuth => Self::ClaudeAuth,
            ProviderType::Codex => Self::Codex,
            ProviderType::Gemini => Self::Gemini,
            ProviderType::GeminiCli => Self::GeminiCli,
            ProviderType::OpenRouter => Self::OpenRouter,
            ProviderType::GitHubCopilot => Self::GitHubCopilot,
            ProviderType::CodexOAuth => Self::CodexOAuth,
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreProviderSpec {
    fn to_proxy_core_provider_spec(&self, app_type: &AppType) -> ProviderSpec;
}

impl ToProxyCoreProviderSpec for Provider {
    fn to_proxy_core_provider_spec(&self, app_type: &AppType) -> ProviderSpec {
        let kind = ProviderKind::from(ProviderType::from_app_type_and_config(app_type, self));
        let metadata = provider_metadata_without_secrets(self);

        ProviderSpec {
            id: self.id.clone(),
            name: self.name.clone(),
            kind,
            account_ref: account_ref(self),
            metadata,
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelSpec {
    fn to_proxy_core_channel_spec(&self) -> ChannelSpec;
}

impl ToProxyCoreChannelSpec for ProxyChannelRecord {
    fn to_proxy_core_channel_spec(&self) -> ChannelSpec {
        ChannelSpec {
            id: self.id.clone(),
            provider_id: self.provider_id.clone(),
            app: AppKind::from(self.app_type.as_str()),
            name: self.name.clone(),
            status: ChannelStatus::from_storage(&self.status),
            endpoint: UpstreamEndpoint {
                base_url: self.base_url.clone(),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::from_storage(&self.interface_kind),
            auth_profile: self.auth_profile_ref.clone().map(AuthProfileRef::new),
            models: self
                .models
                .iter()
                .map(ProxyChannelModelRecord::to_proxy_core_model_route)
                .collect(),
            groups: self.groups.clone(),
            priority: self.priority,
            weight: self.weight,
            retry_policy: RetryPolicy {
                raw: object_or_empty(self.retry_policy.clone()),
            },
            health_policy: ChannelHealthPolicy {
                raw: object_or_empty(self.health_policy.clone()),
            },
            overrides: ChannelOverrides {
                headers: object_or_empty(self.header_overrides.clone()),
                params: object_or_empty(self.param_overrides.clone()),
                status_code_mapping: array_or_empty(self.status_code_mapping.clone()),
                model_mapping: Value::Object(Default::default()),
            },
            tags: self.tags.clone(),
            metadata: object_or_empty(self.metadata.clone()),
            source_ref: self.source_endpoint_url.clone(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreModelRoute {
    fn to_proxy_core_model_route(&self) -> ModelRoute;
}

impl ToProxyCoreModelRoute for ProxyChannelModelRecord {
    fn to_proxy_core_model_route(&self) -> ModelRoute {
        ModelRoute {
            public_model: self.public_model.clone(),
            upstream_model: self.upstream_model.clone(),
            capabilities: ModelCapabilities {
                raw: object_or_empty(self.capabilities.clone()),
            },
            pricing_model: self.pricing_model.clone(),
            request_overrides: object_or_empty(self.request_overrides.clone()),
            response_overrides: object_or_empty(self.response_overrides.clone()),
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord;
}

impl ToProxyCoreChannelModelRecord for ProxyChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord {
        ChannelModelRecord::from_model_route(
            self.channel_id.clone(),
            self.to_proxy_core_model_route(),
        )
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelRecord {
    fn to_proxy_core_channel_record(&self) -> ChannelRecord;
}

impl ToProxyCoreChannelRecord for ProxyChannelRecord {
    fn to_proxy_core_channel_record(&self) -> ChannelRecord {
        ChannelRecord {
            id: self.id.clone(),
            provider_id: self.provider_id.clone(),
            app_type: self.app_type.clone(),
            name: self.name.clone(),
            status: self.status.clone(),
            base_url: self.base_url.clone(),
            interface_kind: self.interface_kind.clone(),
            auth_profile_ref: self.auth_profile_ref.clone(),
            groups: self.groups.clone(),
            priority: self.priority,
            weight: self.weight,
            retry_policy: self.retry_policy.clone(),
            health_policy: self.health_policy.clone(),
            header_overrides: self.header_overrides.clone(),
            param_overrides: self.param_overrides.clone(),
            status_code_mapping: self.status_code_mapping.clone(),
            tags: self.tags.clone(),
            metadata: self.metadata.clone(),
            source_kind: self.source_kind.as_str().to_string(),
            source_endpoint_url: self.source_endpoint_url.clone(),
            models: self
                .models
                .iter()
                .map(ProxyChannelModelRecord::to_proxy_core_channel_model_record)
                .collect(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        }
    }
}

fn provider_metadata_without_secrets(provider: &Provider) -> ProviderMetadata {
    let mut labels = Vec::new();
    if provider.in_failover_queue {
        labels.push("failover".to_string());
    }
    if let Some(category) = provider.category.as_deref() {
        labels.push(category.to_string());
    }

    let meta = provider.meta.as_ref();
    let raw = json!({
        "websiteUrl": provider.website_url,
        "category": provider.category,
        "sortIndex": provider.sort_index,
        "notes": provider.notes,
        "icon": provider.icon,
        "iconColor": provider.icon_color,
        "inFailoverQueue": provider.in_failover_queue,
        "providerType": meta.and_then(|meta| meta.provider_type.clone()),
        "apiFormat": meta.and_then(|meta| meta.api_format.clone()),
        "authBinding": meta.and_then(|meta| meta.auth_binding.as_ref()).map(|binding| json!(binding)),
        "endpointAutoSelect": meta.and_then(|meta| meta.endpoint_auto_select),
        "customEndpointCount": meta.map(|meta| meta.custom_endpoints.len()).unwrap_or(0),
    });

    ProviderMetadata { labels, raw }
}

fn account_ref(provider: &Provider) -> Option<String> {
    provider.meta.as_ref().and_then(|meta| {
        meta.provider_type.as_deref().and_then(|provider_type| {
            meta.managed_account_id_for(provider_type)
                .map(|account_id| format!("{provider_type}:{account_id}"))
        })
    })
}

fn object_or_empty(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        Value::Object(Default::default())
    }
}

fn array_or_empty(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        Value::Array(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::ProxyChannelSourceKind;
    use crate::provider::{AuthBinding, AuthBindingSource, ProviderMeta};

    #[test]
    fn app_type_conversion_preserves_known_and_custom_names() {
        assert_eq!(AppKind::from(&AppType::Claude), AppKind::Claude);
        assert_eq!(
            AppKind::from(&AppType::ClaudeDesktop),
            AppKind::ClaudeDesktop
        );
        assert_eq!(AppKind::from(&AppType::Codex), AppKind::Codex);
        assert_eq!(
            AppKind::from(&AppType::OpenClaw),
            AppKind::Custom("openclaw".to_string())
        );
    }

    #[test]
    fn provider_conversion_uses_inferred_provider_kind_without_settings_leak() {
        let mut provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "secret-token",
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
                }
            }),
            Some("https://github.com/features/copilot".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("github_copilot".to_string()),
                account_id: Some("acct-1".to_string()),
            }),
            ..ProviderMeta::default()
        });

        let spec = provider.to_proxy_core_provider_spec(&AppType::Claude);

        assert_eq!(spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(spec.account_ref.as_deref(), Some("github_copilot:acct-1"));
        let serialized = serde_json::to_string(&spec).expect("serialize spec");
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("ANTHROPIC_AUTH_TOKEN"));
        assert!(!serialized.contains("settingsConfig"));
    }

    #[test]
    fn channel_conversion_preserves_endpoint_interface_models_and_groups() {
        let channel = ProxyChannelRecord {
            id: "ch-1".to_string(),
            provider_id: "provider-1".to_string(),
            app_type: "claude".to_string(),
            name: "Relay A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_chat_completions".to_string(),
            auth_profile_ref: Some("provider:claude:provider-1".to_string()),
            groups: vec!["default".to_string(), "paid".to_string()],
            priority: 50,
            weight: 80,
            retry_policy: json!({"maxAttempts": 2}),
            health_policy: json!({"breaker": "standard"}),
            header_overrides: json!({"x-test": "1"}),
            param_overrides: json!({"stream": true}),
            status_code_mapping: json!([{"from": 429, "to": 503}]),
            tags: vec!["manual".to_string()],
            metadata: json!({"owner": "ops"}),
            source_kind: ProxyChannelSourceKind::Manual,
            source_endpoint_url: Some("https://relay.example.com/v1".to_string()),
            models: vec![ProxyChannelModelRecord {
                channel_id: "ch-1".to_string(),
                public_model: "sonnet".to_string(),
                upstream_model: "anthropic/sonnet".to_string(),
                capabilities: json!({"tools": true}),
                pricing_model: Some("standard".to_string()),
                request_overrides: json!({"temperature": 0.2}),
                response_overrides: json!({}),
            }],
            needs_review: false,
            review_reasons: Vec::new(),
        };

        let spec = channel.to_proxy_core_channel_spec();

        assert_eq!(spec.app, AppKind::Claude);
        assert_eq!(spec.endpoint.base_url, "https://relay.example.com/v1");
        assert_eq!(spec.interface, InterfaceKind::OpenAiChatCompletions);
        assert_eq!(spec.groups, vec!["default".to_string(), "paid".to_string()]);
        assert_eq!(spec.models.len(), 1);
        assert_eq!(spec.models[0].public_model, "sonnet");
        assert_eq!(spec.models[0].upstream_model, "anthropic/sonnet");
        assert_eq!(
            spec.auth_profile.as_ref().map(|value| value.0.as_str()),
            Some("provider:claude:provider-1")
        );
    }
}
