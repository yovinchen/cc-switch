use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy::host::cc_switch::managed_account_runtime_source::{
    ManagedAccountAuthForBindingInput, ManagedAccountRuntimeBindingFacts,
    ManagedAccountRuntimeSourceRef,
};
use crate::proxy::host::cc_switch::provider_projection::provider_managed_account_binding_context;
use crate::proxy::provider::{get_adapter, ProviderAdapter};
use crate::proxy_core::api::auth::ProviderAuthInfo;
use crate::proxy_core::api::transport::{
    forwarder_provider_url_facts, ForwarderProviderUrlFacts, ForwarderProviderUrlFactsInput,
};
use futures::future::BoxFuture;

type ForwarderAdapterHandle = dyn ProviderAdapter;

pub(crate) struct ForwarderAdapterContext {
    adapter: Box<ForwarderAdapterHandle>,
    facts: ForwarderAdapterFacts,
}

impl ForwarderAdapterContext {
    fn new(adapter: Box<ForwarderAdapterHandle>) -> Self {
        let facts = ForwarderAdapterFacts::from_adapter(adapter.as_ref());
        Self { adapter, facts }
    }

    fn adapter(&self) -> &ForwarderAdapterHandle {
        self.adapter.as_ref()
    }

    pub(crate) fn facts(&self) -> &ForwarderAdapterFacts {
        &self.facts
    }

    fn provider_auth_info(&self, provider: &Provider) -> Option<ProviderAuthInfo> {
        self.adapter().extract_auth(provider)
    }

    fn provider_auth_headers(
        &self,
        auth: &ProviderAuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        self.adapter().get_auth_headers(auth)
    }

    pub(crate) fn resolve_provider_fallback_auth_headers<'a>(
        &'a self,
        provider: &'a Provider,
        managed_account_runtime_source: &'a ManagedAccountRuntimeSourceRef,
    ) -> BoxFuture<'a, Result<ProviderFallbackAuthHeaders, ProxyError>> {
        Box::pin(async move {
            let Some(mut auth) = self.provider_auth_info(provider) else {
                return Ok(ProviderFallbackAuthHeaders::default());
            };

            let binding_context = provider_managed_account_binding_context(provider);
            let managed_auth = managed_account_runtime_source
                .resolve_auth_for_binding(ManagedAccountAuthForBindingInput {
                    binding_facts: ManagedAccountRuntimeBindingFacts::new(
                        binding_context.binding,
                        binding_context.legacy_github_copilot_account_id,
                    ),
                    auth,
                })
                .await?;
            auth = managed_auth.auth;

            Ok(ProviderFallbackAuthHeaders {
                auth_headers: self.provider_auth_headers(&auth)?,
                should_send_codex_oauth_session_headers: managed_auth
                    .should_send_codex_oauth_session_headers,
                codex_oauth_account_id: managed_auth.codex_oauth_account_id,
            })
        })
    }

    pub(crate) fn provider_url_facts(
        &self,
        provider: &Provider,
    ) -> Result<ForwarderProviderUrlFacts, ProxyError> {
        let base_url = self.adapter().extract_base_url(provider)?;
        Ok(forwarder_provider_url_facts(
            ForwarderProviderUrlFactsInput {
                provider_type: provider
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.provider_type.as_deref()),
                is_full_url: provider_full_url_flag(provider),
                base_url,
            },
        ))
    }

    pub(crate) fn provider_transform_required(&self, provider: &Provider) -> bool {
        self.adapter().needs_transform(provider)
    }

    pub(crate) fn transform_provider_request(
        &self,
        body: serde_json::Value,
        provider: &Provider,
    ) -> Result<serde_json::Value, ProxyError> {
        self.adapter().transform_request(body, provider)
    }

    pub(crate) fn provider_upstream_url(&self, base_url: &str, endpoint: &str) -> String {
        self.adapter().build_url(base_url, endpoint)
    }
}

pub(crate) fn forwarder_provider_adapter_context_for_app(
    app_type: &AppType,
) -> ForwarderAdapterContext {
    ForwarderAdapterContext::new(get_adapter(app_type))
}

fn provider_full_url_flag(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.is_full_url)
        .unwrap_or(false)
}

#[derive(Default)]
pub(crate) struct ProviderFallbackAuthHeaders {
    pub(crate) auth_headers: Vec<(http::HeaderName, http::HeaderValue)>,
    pub(crate) should_send_codex_oauth_session_headers: bool,
    pub(crate) codex_oauth_account_id: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) struct ForwarderAdapterFacts {
    pub(crate) adapter_name: &'static str,
    pub(crate) is_claude_adapter: bool,
}

impl ForwarderAdapterFacts {
    fn from_adapter(adapter: &ForwarderAdapterHandle) -> Self {
        let adapter_name = adapter.name();
        Self {
            adapter_name,
            is_claude_adapter: adapter_name == "Claude",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderMeta;
    use crate::proxy::host::cc_switch::managed_account_runtime_source::{
        managed_account_test_provider_with_binding, ManagedAccountRuntimeSourceRef,
        StaticManagedAuthResolutionSource,
    };
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn forwarder_adapter_context_projects_provider_url_facts() {
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);
        let mut provider = Provider::with_id(
            "copilot-provider".to_string(),
            "Copilot Provider".to_string(),
            json!({
                "base_url": "https://api.githubcopilot.com"
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            is_full_url: Some(true),
            ..Default::default()
        });

        let facts = adapter
            .provider_url_facts(&provider)
            .expect("provider URL facts");

        assert_eq!(facts.base_url, "https://api.githubcopilot.com");
        assert!(facts.is_full_url);
        assert!(facts.is_copilot);
    }

    #[test]
    fn forwarder_adapter_context_projects_adapter_facts() {
        let claude_adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let codex_adapter = forwarder_provider_adapter_context_for_app(&AppType::Codex);

        let claude_facts = claude_adapter.facts();
        let codex_facts = codex_adapter.facts();

        assert_eq!(claude_facts.adapter_name, "Claude");
        assert!(claude_facts.is_claude_adapter);
        assert_eq!(codex_facts.adapter_name, "Codex");
        assert!(!codex_facts.is_claude_adapter);
    }

    #[tokio::test]
    async fn forwarder_adapter_context_resolves_managed_account_fallback_headers() {
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = managed_account_test_provider_with_binding("codex_oauth", "codex-acct");
        let runtime_source: ManagedAccountRuntimeSourceRef =
            Arc::new(StaticManagedAuthResolutionSource);

        let fallback = adapter
            .resolve_provider_fallback_auth_headers(&provider, &runtime_source)
            .await
            .expect("fallback auth headers");

        assert_eq!(
            fallback.codex_oauth_account_id.as_deref(),
            Some("codex-acct")
        );
        assert!(fallback.should_send_codex_oauth_session_headers);
        assert!(fallback.auth_headers.iter().any(|(name, value)| {
            name == http::header::AUTHORIZATION
                && value == http::HeaderValue::from_static("Bearer codex-token:codex-acct")
        }));
    }
}
