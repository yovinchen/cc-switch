use std::sync::Arc;

use futures::future::BoxFuture;

use crate::proxy::error::ProxyError;
use crate::proxy::host::cc_switch::auth_provider::CcSwitchAuthProvider;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::routing::auth_channel_spec_from_attempt;
use crate::proxy_core::api::transport::{
    auth_provider_proxy_request_from_context, finalize_forwarder_auth_headers,
    prepare_optional_copilot_auth_optimization_for_forwarder, resolve_auth_provider_headers,
    AuthProviderHeaderResolution, ForwarderAuthHeaderFinalizationInput,
};
use crate::proxy_core_adapter::{
    proxy_core_error_to_proxy_error, proxy_provider_to_core_spec, AuthProviderRef,
    ForwarderAuthHeaders, ForwarderAuthHeadersInput, ForwarderAuthSource, ForwarderAuthSourceRef,
    ForwarderMaybeCopilotAuthOptimizationInput, ForwarderPreparedCopilotAuthOptimization,
    ManagedAccountRuntimeSourceRef,
};

#[cfg(test)]
use crate::proxy_core_adapter::default_managed_account_runtime_source;

struct CcSwitchForwarderAuthSource {
    managed_account_runtime_source: ManagedAccountRuntimeSourceRef,
    auth_provider: AuthProviderRef,
}

impl CcSwitchForwarderAuthSource {
    fn new(
        managed_account_runtime_source: ManagedAccountRuntimeSourceRef,
        auth_provider: AuthProviderRef,
    ) -> Self {
        Self {
            managed_account_runtime_source,
            auth_provider,
        }
    }
}

impl ForwarderAuthSource for CcSwitchForwarderAuthSource {
    fn prepare_optional_copilot_auth_optimization(
        &self,
        input: ForwarderMaybeCopilotAuthOptimizationInput<'_>,
    ) -> Option<ForwarderPreparedCopilotAuthOptimization> {
        prepare_optional_copilot_auth_optimization_for_forwarder(input, || {
            uuid::Uuid::new_v4().to_string()
        })
    }

    fn resolve_upstream_auth_headers<'a>(
        &'a self,
        input: ForwarderAuthHeadersInput<'a>,
    ) -> BoxFuture<'a, Result<ForwarderAuthHeaders, ProxyError>> {
        Box::pin(async move {
            let app = AppKind::from(input.app_type);
            let attempt_provider = input.attempt.provider();
            let provider = proxy_provider_to_core_spec(attempt_provider, input.app_type);
            let channel = auth_channel_spec_from_attempt(
                &app,
                attempt_provider.id.as_str(),
                attempt_provider.name.as_str(),
                input.attempt.channel(),
            );
            let request = auth_provider_proxy_request_from_context(
                app.clone(),
                input.method.clone(),
                input.endpoint,
                &channel,
                input.request_body,
                input.request_headers,
            );
            let core_auth = self
                .auth_provider
                .resolve_auth(&app, &provider, &channel, &request)
                .await
                .map_err(proxy_core_error_to_proxy_error)?;

            let mut codex_oauth_account_id: Option<String> = None;
            let mut should_send_codex_oauth_session_headers = false;
            let auth_headers = match resolve_auth_provider_headers(&core_auth)
                .map_err(proxy_core_error_to_proxy_error)?
            {
                AuthProviderHeaderResolution::Explicit(headers) => headers,
                AuthProviderHeaderResolution::Fallback => {
                    let auth_provider = input.attempt.auth_provider();
                    if let Some(mut auth) = input.adapter.provider_auth_info(auth_provider) {
                        let managed_auth = self
                            .managed_account_runtime_source
                            .resolve_auth_for_provider(auth_provider, auth)
                            .await?;
                        auth = managed_auth.auth;
                        should_send_codex_oauth_session_headers =
                            managed_auth.should_send_codex_oauth_session_headers;
                        codex_oauth_account_id = managed_auth.codex_oauth_account_id;

                        input.adapter.provider_auth_headers(&auth)?
                    } else {
                        Vec::new()
                    }
                }
            };

            let finalized_auth_headers =
                finalize_forwarder_auth_headers(ForwarderAuthHeaderFinalizationInput {
                    base_auth_headers: &auth_headers,
                    should_send_codex_oauth_session_headers,
                    session_client_provided: input.session_client_provided,
                    session_id: input.session_id,
                    codex_oauth_account_id: codex_oauth_account_id.as_deref(),
                    copilot_optimization: input.copilot_optimization.as_ref(),
                });

            if finalized_auth_headers.should_log_copilot_subagent_auth_override {
                log::info!(
                    "[Copilot] 子代理请求: x-initiator=agent, x-interaction-type=conversation-subagent"
                );
            }

            Ok(finalized_auth_headers)
        })
    }
}

pub(crate) fn forwarder_auth_source_from_managed_account_runtime_source(
    managed_account_runtime_source: ManagedAccountRuntimeSourceRef,
) -> ForwarderAuthSourceRef {
    forwarder_auth_source_from_sources(
        managed_account_runtime_source,
        Arc::new(CcSwitchAuthProvider),
    )
}

pub(crate) fn forwarder_auth_source_from_sources(
    managed_account_runtime_source: ManagedAccountRuntimeSourceRef,
    auth_provider: AuthProviderRef,
) -> ForwarderAuthSourceRef {
    Arc::new(CcSwitchForwarderAuthSource::new(
        managed_account_runtime_source,
        auth_provider,
    ))
}

#[cfg(test)]
pub(crate) fn default_forwarder_auth_source() -> ForwarderAuthSourceRef {
    forwarder_auth_source_from_managed_account_runtime_source(
        default_managed_account_runtime_source(),
    )
}
