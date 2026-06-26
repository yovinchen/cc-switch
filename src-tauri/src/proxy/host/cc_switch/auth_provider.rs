use crate::proxy_core_adapter::{
    AppKind, AuthInfo, AuthProvider, ChannelSpec, ProviderSpec, ProxyCoreResult, ProxyRequest,
    auth_info_from_route_context,
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
