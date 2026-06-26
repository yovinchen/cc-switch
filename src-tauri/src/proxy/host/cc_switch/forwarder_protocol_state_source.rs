use futures::future::BoxFuture;
use serde_json::Value;
use std::sync::Arc;

use crate::proxy::codex_chat_history::CodexChatHistoryStore;
use crate::proxy_core_adapter::{
    ForwarderClaudeProtocolTransformInput, ForwarderCodexChatProtocolEnrichmentInput,
    ForwarderProtocolStateSource, ForwarderProtocolStateSourceRef, GeminiShadowStore,
    provider_claude_transform_request_for_api_format,
};

pub(crate) struct CcSwitchForwarderProtocolStateSource {
    gemini_shadow: Arc<GeminiShadowStore>,
    codex_chat_history: Arc<CodexChatHistoryStore>,
}

impl CcSwitchForwarderProtocolStateSource {
    pub(crate) fn new(
        gemini_shadow: Arc<GeminiShadowStore>,
        codex_chat_history: Arc<CodexChatHistoryStore>,
    ) -> Self {
        Self {
            gemini_shadow,
            codex_chat_history,
        }
    }
}

impl ForwarderProtocolStateSource for CcSwitchForwarderProtocolStateSource {
    fn enrich_codex_chat_request<'a>(
        &'a self,
        input: ForwarderCodexChatProtocolEnrichmentInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if !input.enabled {
                return;
            }

            let restored = self.codex_chat_history.enrich_request(input.body).await;
            if restored > 0 {
                log::debug!(
                    "[Codex] Restored or enriched {restored} cached function call item(s) for Chat upstream"
                );
            }
        })
    }

    fn transform_claude_request(
        &self,
        input: ForwarderClaudeProtocolTransformInput<'_>,
    ) -> Result<Value, String> {
        let api_format = input.api_format.unwrap_or("anthropic");
        let session_id = input.session_client_provided.then_some(input.session_id);

        provider_claude_transform_request_for_api_format(
            input.body,
            input.provider,
            api_format,
            session_id,
            Some(self.gemini_shadow.as_ref()),
        )
    }
}

pub(crate) fn forwarder_protocol_state_source_from_runtime_parts(
    gemini_shadow: Arc<GeminiShadowStore>,
    codex_chat_history: Arc<CodexChatHistoryStore>,
) -> ForwarderProtocolStateSourceRef {
    Arc::new(CcSwitchForwarderProtocolStateSource::new(
        gemini_shadow,
        codex_chat_history,
    ))
}
