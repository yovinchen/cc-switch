use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy_core::api::auth::settings_config_with_channel_auth_key_for_app;
use crate::proxy_core::api::domain::{AppKind, ChannelSpec, ProviderSpec, ProxyRequest};
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::{auth_info_from_route_context, AuthInfo, AuthProvider};
use futures::future::BoxFuture;

#[cfg(test)]
use crate::proxy_core::api::domain::AuthProfileRef;
#[cfg(test)]
use crate::proxy_core::api::ports::auth_info_from_profile_ref;

const CC_SWITCH_PROVIDER_CONFIG_AUTH_SOURCE: &str = "cc_switch_provider_config";

#[cfg(test)]
pub(crate) fn auth_info_from_cc_switch_provider_config(
    auth_profile: Option<&AuthProfileRef>,
) -> AuthInfo {
    auth_info_from_profile_ref(auth_profile, CC_SWITCH_PROVIDER_CONFIG_AUTH_SOURCE)
}

pub(crate) fn auth_info_from_cc_switch_route_context(
    app: &AppKind,
    provider: &ProviderSpec,
    channel: &ChannelSpec,
) -> AuthInfo {
    auth_info_from_route_context(
        app,
        provider,
        channel,
        CC_SWITCH_PROVIDER_CONFIG_AUTH_SOURCE,
    )
}

#[derive(Clone, Default)]
pub(crate) struct CcSwitchAuthProvider;

impl AuthProvider for CcSwitchAuthProvider {
    fn resolve_auth<'a>(
        &'a self,
        app: &'a AppKind,
        provider: &'a ProviderSpec,
        channel: &'a ChannelSpec,
        _request: &'a ProxyRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>> {
        Box::pin(async move {
            Ok(auth_info_from_cc_switch_route_context(
                app, provider, channel,
            ))
        })
    }
}

pub(crate) fn provider_with_channel_auth_key(
    app_type: &AppType,
    provider: &Provider,
    key_value: &str,
) -> Provider {
    let mut auth_provider = provider.clone();
    auth_provider.settings_config = settings_config_with_channel_auth_key_for_app(
        &AppKind::from(app_type),
        &provider.settings_config,
        key_value,
    );
    auth_provider
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::api::domain::{
        channel_spec_from_input, ChannelSpecInput, ProviderKind, ProviderMetadata,
    };
    use serde_json::{json, Value};

    #[test]
    fn auth_adapter_projects_cc_switch_provider_config_source() {
        let auth_profile = AuthProfileRef::new("provider:claude:anthropic-main");
        let auth = auth_info_from_cc_switch_provider_config(Some(&auth_profile));
        assert!(auth.headers.is_empty());
        assert_eq!(
            auth.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(auth.metadata["source"], json!("cc_switch_provider_config"));

        let fallback = auth_info_from_cc_switch_provider_config(None);
        assert!(fallback.account_ref.is_none());
        assert_eq!(
            fallback.metadata["source"],
            json!("cc_switch_provider_config")
        );
    }

    #[test]
    fn auth_adapter_projects_cc_switch_route_context_source() {
        let provider = ProviderSpec {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        };
        let channel = channel_spec_from_input(ChannelSpecInput {
            id: "channel-a".to_string(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Channel A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            auth_profile_ref: Some("provider:claude:anthropic-main".to_string()),
            models: Vec::new(),
            groups: Vec::new(),
            priority: 0,
            weight: 100,
            retry_policy: Value::Object(Default::default()),
            health_policy: Value::Object(Default::default()),
            header_overrides: Value::Object(Default::default()),
            param_overrides: Value::Object(Default::default()),
            status_code_mapping: Value::Array(Vec::new()),
            tags: Vec::new(),
            metadata: Value::Object(Default::default()),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        });

        let auth = auth_info_from_cc_switch_route_context(&AppKind::Claude, &provider, &channel);

        assert!(auth.headers.is_empty());
        assert_eq!(
            auth.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(auth.metadata["source"], json!("cc_switch_provider_config"));
        assert_eq!(auth.metadata["app"], json!("claude"));
        assert_eq!(auth.metadata["providerId"], json!("provider-a"));
        assert_eq!(auth.metadata["channelId"], json!("channel-a"));
    }
}
