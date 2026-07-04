use super::{
    engine::context::RequestContext, error::ProxyError,
    transport::http::request_body::ParsedHttpJsonProxyRequest,
};
use crate::app_config::AppType;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::routing::InterfaceKind;
use crate::proxy_core::api::transforms::{build_codex_tool_context_from_request, CodexToolContext};
use crate::proxy_core::api::transport::{
    endpoint_from_path_and_query, endpoint_from_path_query_stripping_prefix,
    extract_gemini_model_from_path, ProxyBody, ProxyRequest,
};
use http::Uri;

pub(crate) struct CodexResponsesProxyRequest {
    pub(crate) request: ProxyRequest,
    pub(crate) tool_context: CodexToolContext,
}

impl ParsedHttpJsonProxyRequest {
    pub(crate) async fn request_context(
        &self,
        state: &ProxyState,
        app_type: AppType,
        tag: &'static str,
        app_type_str: &'static str,
    ) -> Result<RequestContext, ProxyError> {
        RequestContext::new(
            state,
            &self.body,
            &self.headers,
            app_type,
            tag,
            app_type_str,
        )
        .await
    }

    pub(crate) async fn codex_request_context(
        &self,
        state: &ProxyState,
    ) -> Result<RequestContext, ProxyError> {
        self.request_context(state, AppType::Codex, "Codex", "codex")
            .await
    }

    pub(crate) async fn gemini_request_context(
        &self,
        state: &ProxyState,
        uri: &Uri,
    ) -> Result<RequestContext, ProxyError> {
        self.request_context(state, AppType::Gemini, "Gemini", "gemini")
            .await
            .map(|ctx| ctx.with_model_from_uri(uri))
    }

    pub(crate) fn endpoint_from_request_uri_stripping_prefix(
        &self,
        strip_prefix: Option<&str>,
    ) -> String {
        endpoint_from_path_query_stripping_prefix(self.uri.path(), self.uri.query(), strip_prefix)
    }

    pub(crate) fn endpoint_for_path(&self, path: &str) -> String {
        endpoint_from_path_and_query(path, self.uri.query())
    }

    fn into_json_proxy_request(
        self,
        app_type: AppType,
        endpoint: String,
        inbound_interface: InterfaceKind,
        requested_model: Option<String>,
    ) -> ProxyRequest {
        ProxyRequest::new(
            AppKind::from(&app_type),
            self.method,
            endpoint,
            inbound_interface,
            ProxyBody::Json(self.body),
        )
        .with_observed_request_context(requested_model, self.headers, self.extensions)
    }

    pub(crate) fn into_anthropic_messages_proxy_request(
        self,
        app_type: AppType,
        endpoint: String,
        requested_model: Option<String>,
    ) -> ProxyRequest {
        self.into_json_proxy_request(
            app_type,
            endpoint,
            InterfaceKind::AnthropicMessages,
            requested_model,
        )
    }

    pub(crate) fn into_codex_chat_proxy_request(
        self,
        endpoint: String,
        requested_model: Option<String>,
    ) -> ProxyRequest {
        self.into_json_proxy_request(
            AppType::Codex,
            endpoint,
            InterfaceKind::OpenAiChatCompletions,
            requested_model,
        )
    }

    pub(crate) fn into_codex_responses_proxy_request(
        self,
        endpoint: String,
        requested_model: Option<String>,
    ) -> CodexResponsesProxyRequest {
        let tool_context = build_codex_tool_context_from_request(&self.body);
        let request = self.into_json_proxy_request(
            AppType::Codex,
            endpoint,
            InterfaceKind::OpenAiResponses,
            requested_model,
        );
        CodexResponsesProxyRequest {
            request,
            tool_context,
        }
    }

    pub(crate) fn into_gemini_proxy_request(self, endpoint: String) -> ProxyRequest {
        let requested_model = extract_gemini_model_from_path(&endpoint);
        self.into_json_proxy_request(
            AppType::Gemini,
            endpoint,
            InterfaceKind::GeminiNative,
            requested_model,
        )
    }
}
