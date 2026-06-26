use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy_core_adapter::{
    AppKind, AuthInfo, AuthProvider, ChannelSpec, ProviderSpec, ProxyCoreResult, ProxyRequest,
    auth_info_from_route_context, settings_config_with_channel_auth_key_for_app,
};
use futures::future::BoxFuture;

#[cfg(test)]
use crate::proxy_core_adapter::{AuthProfileRef, auth_info_from_profile_ref};

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
