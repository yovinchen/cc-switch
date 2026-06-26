use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy::provider::{get_adapter, ProviderAdapter};
use crate::proxy_core_adapter::{
    forwarder_provider_url_facts, provider_is_full_url, ForwarderProviderUrlFacts,
    ForwarderProviderUrlFactsInput, ProviderAuthInfo,
};

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

    pub(crate) fn provider_auth_info(&self, provider: &Provider) -> Option<ProviderAuthInfo> {
        self.adapter().extract_auth(provider)
    }

    pub(crate) fn provider_auth_headers(
        &self,
        auth: &ProviderAuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        self.adapter().get_auth_headers(auth)
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
                is_full_url: provider_is_full_url(provider),
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
