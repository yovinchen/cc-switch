use std::sync::Arc;

use futures::future::BoxFuture;

use crate::proxy::engine::forward_pipeline::{
    ForwarderAuthHeadersInput, ForwarderAuthSource, ForwarderAuthSourceRef,
};
use crate::proxy::error::ProxyError;
use crate::proxy::error_mapper::proxy_core_error_to_proxy_error;
use crate::proxy::host::cc_switch::auth_provider::CcSwitchAuthProvider;
use crate::proxy::host::cc_switch::managed_account_runtime_source::{
    ManagedAccountAuthForBindingInput, ManagedAccountRuntimeBindingFacts,
    ManagedAccountRuntimeSourceRef,
};
use crate::proxy::host::cc_switch::provider_projection::{
    provider_managed_account_binding_context, proxy_provider_to_core_spec,
};
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::ports::AuthProvider;
use crate::proxy_core::api::routing::auth_channel_spec_from_attempt;
use crate::proxy_core::api::transport::{
    auth_provider_proxy_request_from_context, finalize_forwarder_auth_headers,
    prepare_optional_copilot_auth_optimization_for_forwarder, resolve_auth_provider_headers,
    AuthProviderHeaderResolution, ForwarderAuthHeaderFinalizationInput, ForwarderAuthHeaders,
    OptionalCopilotAuthOptimizationPreparationInput, PreparedCopilotAuthOptimization,
};

#[cfg(test)]
use crate::proxy::host::cc_switch::managed_account_runtime_source::default_managed_account_runtime_source;

pub(crate) type AuthProviderRef = Arc<dyn AuthProvider + Send + Sync>;

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
        input: OptionalCopilotAuthOptimizationPreparationInput<'_>,
    ) -> Option<PreparedCopilotAuthOptimization> {
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
                        let binding_context =
                            provider_managed_account_binding_context(auth_provider);
                        let managed_auth = self
                            .managed_account_runtime_source
                            .resolve_auth_for_binding(ManagedAccountAuthForBindingInput {
                                binding_facts: ManagedAccountRuntimeBindingFacts::new(
                                    binding_context.binding,
                                    binding_context.legacy_github_copilot_account_id,
                                ),
                                auth,
                            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppType;
    use crate::provider::Provider;
    use crate::proxy::engine::forward_pipeline::ForwarderAuthHeadersInput;
    use crate::proxy::host::cc_switch::managed_account_runtime_source::{
        managed_account_test_provider_with_binding, StaticManagedAuthResolutionSource,
    };
    use crate::proxy::host::cc_switch::provider_adapter_context::forwarder_provider_adapter_context_for_app;
    use crate::proxy::route_attempt::ForwardAttempt;
    use crate::proxy_core::api::domain::{AppKind, ProviderSpec};
    use crate::proxy_core::api::errors::ProxyCoreResult;
    use crate::proxy_core::api::ports::{AuthInfo, CopilotOptimizerConfig};
    use crate::proxy_core::api::routing::{ChannelRouteCandidate, ChannelSpec};
    use crate::proxy_core::api::transport::{
        CopilotClassification, OptionalCopilotAuthOptimizationPreparationInput,
        PreparedCopilotAuthOptimization, ProxyRequest,
    };
    use futures::future::BoxFuture;
    use http::{HeaderMap, Method};
    use serde_json::json;

    #[test]
    fn forwarder_auth_source_prepares_optional_copilot_auth_optimization() {
        let source = default_forwarder_auth_source();
        let headers = HeaderMap::new();
        let config = CopilotOptimizerConfig {
            request_classification: true,
            deterministic_request_id: true,
            ..CopilotOptimizerConfig::default()
        };

        let skipped = source.prepare_optional_copilot_auth_optimization(
            OptionalCopilotAuthOptimizationPreparationInput {
                classification: None,
                config: &config,
                session_source_body: &json!({}),
                request_body: &json!({}),
                headers: &headers,
            },
        );
        assert!(skipped.is_none());

        let classification = CopilotClassification {
            initiator: "user",
            is_warmup: false,
            is_compact: false,
            is_subagent: true,
        };
        let prepared = source
            .prepare_optional_copilot_auth_optimization(
                OptionalCopilotAuthOptimizationPreparationInput {
                    classification: Some(classification),
                    config: &config,
                    session_source_body: &json!({
                        "metadata": { "session_id": "session-a" }
                    }),
                    request_body: &json!({
                        "messages": [{"role": "user", "content": "Hello"}]
                    }),
                    headers: &headers,
                },
            )
            .expect("prepared copilot auth optimization");

        assert!(prepared.request_classification_enabled);
        assert_eq!(prepared.initiator, "user");
        assert!(prepared.is_subagent);
        assert!(prepared.deterministic_request_id.is_some());
        assert!(prepared.interaction_id.is_some());
    }

    struct ChannelHeaderAuthProvider;

    impl AuthProvider for ChannelHeaderAuthProvider {
        fn resolve_auth<'a>(
            &'a self,
            app: &'a AppKind,
            provider: &'a ProviderSpec,
            channel: &'a ChannelSpec,
            request: &'a ProxyRequest,
        ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>> {
            let app = app.as_str().to_string();
            let provider_id = provider.id.clone();
            let channel_id = channel.id.clone();
            let requested_model = request.requested_model.clone();
            Box::pin(async move {
                Ok(AuthInfo {
                    headers: vec![("x-core-auth-channel".to_string(), channel_id.clone())],
                    account_ref: Some(provider_id.clone()),
                    metadata: json!({
                        "app": app,
                        "providerId": provider_id,
                        "channelId": channel_id,
                        "requestedModel": requested_model,
                    }),
                })
            })
        }
    }

    #[tokio::test]
    async fn forwarder_auth_source_uses_core_auth_provider_route_context_headers() {
        let source = forwarder_auth_source_from_sources(
            default_managed_account_runtime_source(),
            Arc::new(ChannelHeaderAuthProvider),
        );
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "provider-key"
                }
            }),
            None,
        );
        let attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            ChannelRouteCandidate {
                channel_id: "channel-auth".to_string(),
                provider_id: provider.id.clone(),
                channel_name: "Channel Auth".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                public_model: Some("sonnet-public".to_string()),
                upstream_model: Some("sonnet-upstream".to_string()),
                route_group: "default".to_string(),
                priority: 100,
                weight: 1,
                source_kind: "manual".to_string(),
            },
        );
        let method = Method::POST;
        let body = json!({ "model": "sonnet-public" });
        let headers = HeaderMap::new();

        let resolved = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: false,
                copilot_optimization: None,
            })
            .await
            .expect("resolve auth headers");

        assert_eq!(resolved.codex_oauth_session_headers.len(), 0);
        assert_eq!(resolved.auth_headers.len(), 1);
        assert_eq!(
            resolved.auth_headers[0].0,
            http::HeaderName::from_static("x-core-auth-channel")
        );
        assert_eq!(
            resolved.auth_headers[0].1.to_str().expect("header value"),
            "channel-auth"
        );
        assert!(!resolved
            .auth_headers
            .iter()
            .any(|(name, _)| name == http::header::AUTHORIZATION));
    }

    #[tokio::test]
    async fn forwarder_auth_source_uses_core_codex_oauth_session_header_gate() {
        let source = forwarder_auth_source_from_managed_account_runtime_source(Arc::new(
            StaticManagedAuthResolutionSource,
        ));
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = managed_account_test_provider_with_binding("codex_oauth", "codex-acct");
        let attempt = ForwardAttempt::from_provider(provider);
        let method = Method::POST;
        let body = json!({ "model": "gpt-5" });
        let headers = HeaderMap::new();

        let without_client_session = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: false,
                copilot_optimization: None,
            })
            .await
            .expect("resolve auth headers without client session");

        assert!(without_client_session
            .codex_oauth_session_headers
            .is_empty());

        let with_client_session = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: true,
                copilot_optimization: None,
            })
            .await
            .expect("resolve auth headers with client session");

        assert!(with_client_session
            .auth_headers
            .iter()
            .any(|(name, value)| {
                name == http::header::AUTHORIZATION
                    && value == http::HeaderValue::from_static("Bearer codex-token:codex-acct")
            }));

        let mut session_headers = HeaderMap::new();
        for (name, value) in with_client_session.codex_oauth_session_headers {
            session_headers.insert(name, value);
        }

        assert_eq!(
            session_headers.get("session_id"),
            Some(&http::HeaderValue::from_static("session-a"))
        );
        assert_eq!(
            session_headers.get("x-client-request-id"),
            Some(&http::HeaderValue::from_static("session-a"))
        );
        assert_eq!(
            session_headers.get("x-codex-window-id"),
            Some(&http::HeaderValue::from_static("session-a:0"))
        );
    }

    #[tokio::test]
    async fn forwarder_auth_source_uses_core_copilot_auth_override_facts() {
        let source = forwarder_auth_source_from_managed_account_runtime_source(Arc::new(
            StaticManagedAuthResolutionSource,
        ));
        let adapter = forwarder_provider_adapter_context_for_app(&AppType::Claude);
        let provider = managed_account_test_provider_with_binding("github_copilot", "copilot-acct");
        let attempt = ForwardAttempt::from_provider(provider);
        let method = Method::POST;
        let body = json!({ "model": "claude-sonnet-4" });
        let headers = HeaderMap::new();

        let resolved = source
            .resolve_upstream_auth_headers(ForwarderAuthHeadersInput {
                adapter: &adapter,
                app_type: &AppType::Claude,
                method: &method,
                endpoint: "/v1/messages",
                request_body: &body,
                request_headers: &headers,
                attempt: &attempt,
                session_id: "session-a",
                session_client_provided: false,
                copilot_optimization: Some(PreparedCopilotAuthOptimization {
                    request_classification_enabled: true,
                    initiator: "agent",
                    is_subagent: true,
                    deterministic_request_id: Some("request-id".to_string()),
                    interaction_id: Some("interaction-id".to_string()),
                }),
            })
            .await
            .expect("resolve copilot auth headers");

        let mut map = HeaderMap::new();
        for (name, value) in resolved.auth_headers {
            map.insert(name, value);
        }

        assert_eq!(
            map.get(http::header::AUTHORIZATION),
            Some(&http::HeaderValue::from_static(
                "Bearer copilot-token:copilot-acct"
            ))
        );
        assert_eq!(
            map.get("x-initiator"),
            Some(&http::HeaderValue::from_static("agent"))
        );
        assert_eq!(
            map.get("x-interaction-type"),
            Some(&http::HeaderValue::from_static("conversation-subagent"))
        );
        assert_eq!(
            map.get("x-request-id"),
            Some(&http::HeaderValue::from_static("request-id"))
        );
        assert_eq!(
            map.get("x-agent-task-id"),
            Some(&http::HeaderValue::from_static("request-id"))
        );
        assert_eq!(
            map.get("x-interaction-id"),
            Some(&http::HeaderValue::from_static("interaction-id"))
        );
    }
}
