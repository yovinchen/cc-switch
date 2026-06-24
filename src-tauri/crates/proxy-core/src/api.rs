//! Stable integration surface for the extracted proxy module.
//!
//! Host adapters and external integrations should depend on these grouped
//! modules rather than the internal file layout.

pub mod auth {
    pub use crate::claude_auth::*;
    pub use crate::claude_desktop_gateway_auth::*;
    pub use crate::gemini_auth::*;
    pub use crate::managed_account_auth::*;
    pub use crate::management_auth::*;
    pub use crate::provider_auth::*;
}

pub mod config {
    pub use crate::cache_injector::*;
    pub use crate::circuit_breaker_config::*;
    pub use crate::circuit_breaker_key::*;
    pub use crate::ports::{
        app_proxy_config_defaults_for_app, copilot_optimizer_config_spec_from_config,
        optimizer_config_spec_from_config, proxy_app_config_from_parts,
        proxy_global_config_from_global_config, proxy_runtime_config_from_proxy_config,
        rectifier_config_spec_from_config, AppProxyConfig, AppSummaryConfig,
        CopilotOptimizerConfigSpec, OptimizerConfigSpec, ProxyAppConfig, ProxyGlobalConfig,
        ProxyRuntimeConfig, RectifierConfigSpec,
    };
    pub use crate::response_timeout::{
        ResponseRuntimePolicy, ResponseTimeoutConfig, StreamingTimeoutConfig,
    };
    pub use crate::thinking_budget_rectifier::{
        rectify_thinking_budget, should_rectify_thinking_budget, ThinkingBudgetRectifierConfig,
    };
    pub use crate::thinking_optimizer::{
        thinking_optimization_log_message, ThinkingOptimizerConfig,
    };
    pub use crate::thinking_rectifier::{
        normalize_thinking_type, rectify_anthropic_request, should_rectify_thinking_signature,
        ThinkingSignatureRectifierConfig,
    };
}

pub mod domain {
    pub use crate::domain::*;
}

pub mod engine {
    pub use crate::engine::{ProxyCoreStatus, ProxyEngine, ProxyRuntimeState};
}

pub mod errors {
    pub use crate::error::*;
}

pub mod events {
    pub use crate::event_payload::*;
    pub use crate::ports::{ProxyCoreEvent, ProxyCoreEventType};
}

pub mod management {
    pub use crate::management_api::*;
    pub use crate::ports::{
        channel_key_record_from_input, channel_model_record_from_input,
        channel_route_source_for_materialized_count,
        channel_health_update_from_input,
        channel_reachability_result_from_stream_check_result,
        channel_reachability_status_from_latency, channel_record_from_input, plan_channel_test,
        provider_health_update_from_input, should_retry_channel_reachability_failure,
        AppChannelListQuery, AppChannelResponse, AppListResponse, AppModelListQuery,
        AppSummaryInput, ChannelDeleteResponse, CHANNEL_HEALTH_UNKNOWN_STATUS,
        ChannelHealthResetResponse, ChannelHealthUpdate, ChannelHealthUpdateInput,
        ChannelKeyDeleteResponse, ChannelKeyRecord,
        ChannelKeyRecordInput, ChannelKeyRecordResponse, ChannelKeysResponse,
        ChannelListQuery, ChannelListResponse, ChannelMigrationMaterializeInput,
        ChannelMigrationMaterializeResponse, ChannelMigrationPreviewInput,
        ChannelMigrationPreviewResponse, ChannelModelRecord, ChannelModelRecordInput,
        ChannelModelsResponse, ChannelReachabilityInput, ChannelReachabilityResult,
        ChannelReachabilityStatus, ChannelRecord, ChannelRecordInput, ChannelRecordResponse,
        ChannelRouteRejected, ChannelRouteSource, ChannelTestPlan, ChannelTestProbeRequest,
        ChannelTestResponse,
        CurrentRouteProviderSummaryInput, CurrentRouteResponse, GroupListQuery,
        HealthCheckResponse, ProviderHealthUpdate, ProviderHealthUpdateInput,
        ProviderListResponse, ProviderSummaryInput, ProxyChannelKeyPatchRequest,
        ProxyChannelKeyWriteRequest, ProxyChannelModelWriteRequest,
        ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest, ProxyChannelTestRequest,
        ProxyChannelWriteRequest, ProxyStatusResponse, RouteGroupListResponse, StreamCheckConfig,
        StreamCheckResult, RouteResolveRequest, RouteResolveResponse,
    };
}

pub mod logging {
    pub use crate::log_codes::{cb, fwd, fo, rsp, srv, usg};
}

pub mod model_catalog {
    pub use crate::copilot_model_map::*;
    pub use crate::domain::RoutableModelList;
    pub use crate::model_fetch::*;
    pub use crate::model_mapping::*;
    pub use crate::ports::{ClientModelCatalogResponse, ModelCatalog};
}

pub mod ports {
    pub use crate::domain::{ChannelAttemptPlan, ChannelAttemptResult, ProviderAttemptResult};
    pub use crate::ports::{
        app_proxy_config_defaults_for_app, auth_info_from_profile_ref,
        auth_info_from_route_context, AppProxyConfig, AppSummaryConfig, AuthInfo, AuthProvider,
        ChannelHealthReset, ChannelHealthResetResponse, ChannelHealthStore,
        ChannelReachabilityProbe, ChannelModelRecord, ChannelSource,
        CopilotOptimizerConfig, CopilotOptimizerConfigSpec, CurrentRouteChannelTargetInput,
        CurrentRouteTarget, CurrentRouteTargetInput, ForwardCurrentProviderStatusInput,
        ForwardFailureStatusInput, ForwardPipeline, ForwardProviderFailureStatusInput,
        ForwardProviderRectifierRetryFailureStatusInput, ForwardRequestStartedStatusInput,
        ForwardSuccessStatusInput, ForwardSuccessStatusUpdate,
        GlobalProxyConfig, ModelCatalog, ModelCatalogProvider, OptimizerConfig,
        OptimizerConfigSpec,
        DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD, DEFAULT_PROXY_LISTEN_ADDRESS,
        DEFAULT_PROXY_LISTEN_PORT, app_proxy_config_raw, app_proxy_config_with_enabled,
        apply_proxy_runtime_active_targets, channel_health_reset_from_parts,
        copilot_optimizer_config_spec_from_config, current_route_target_from_input,
        optimizer_config_spec_from_config,
        proxy_app_config_from_parts, proxy_config_with_ephemeral_listen_port,
        proxy_config_preserving_live_takeover_active, proxy_config_with_live_takeover_active,
        proxy_global_config_from_global_config, proxy_runtime_config_from_proxy_config,
        proxy_runtime_status_stopped, live_takeover_app_kinds, provider_switch_dispatch_for_app,
        provider_switch_requires_takeover_lock, ProviderSwitchDispatch,
        proxy_live_urls_from_listen_parts, proxy_server_info_from_parts,
        proxy_takeover_status_from_enabled_options,
        proxy_takeover_status_from_parts, apply_proxy_runtime_uptime,
        record_active_connection_acquired_status, record_active_connection_released_status,
        record_forward_current_provider_status, record_forward_failure_status,
        record_forward_provider_failure_status,
        record_forward_provider_rectifier_retry_failure_status,
        record_forward_request_started_status, record_forward_success_status,
        record_proxy_server_started_status, record_proxy_server_stopped_status,
        rectifier_config_spec_from_config, ProviderHealth, ProviderHealthStore,
        ProviderHealthUpdate, ProviderHealthUpdateInput, ProviderSource, ProxyAppConfig,
        ProxyConfig, ProxyConfigSource, ProxyCoreEvent, ProxyCoreEventType, ProxyEventSink,
        ProxyGlobalConfig, ProxyRuntimeConfig, ProxyRuntimeStatus, ProxyServerInfo,
        ProxyServerStartedStatusInput, ProxyServices, ProxyTakeoverStatus, RectifierConfig,
        RectifierConfigSpec, RoutePolicySource, RouteResolver, UsageSink,
    };
}

pub mod routing {
    pub use crate::channel_identity::*;
    pub use crate::channel_request::*;
    pub use crate::domain::{
        ChannelQuery, ChannelSpec, ChannelStatus, InterfaceKind, ResolvedChannelAttempt, RoutePlan,
        RoutePlanProviderMatch, RoutePolicy, RouteRequest, RouteSelection, DEFAULT_ROUTE_GROUP,
        auth_channel_spec_from_attempt, default_auth_interface_for_app_kind,
        forwarding_requires_runtime_error_message, interfaces_compatible, build_route_plan,
        route_group_matches,
        route_plan_no_matching_host_providers_error_message, route_plan_provider_ids,
        route_plan_provider_match,
        route_plan_providers_unconfigured_error_message, route_plan_selections,
        route_policy_failover_provider_ids, route_policy_from_failover_provider_ids,
        route_selection_from_parts, select_route_for_forward_result,
    };
    pub use crate::legacy_projection::*;
    pub use crate::ports::ChannelRouteCandidate;
    pub use crate::provider_selection::*;
    pub use crate::route_resolve::*;
}

pub mod security {
    pub use crate::secret::*;
}

pub mod session {
    pub use crate::session::{
        extract_session_id_with_generator, proxy_session_request_metadata, ClientFormat,
        ProxySessionRequestMetadata, SessionIdResult, SessionIdSource,
    };
}

pub mod transport {
    pub use crate::domain::{
        auth_provider_proxy_request_from_context, ProxyBody, ProxyCoreResponse, ProxyRequest,
        ProxyResponseBody, ProxyResult, ProxyTransportResponse, ProxyTransportResponseBody,
    };
    pub use crate::forward_failure::*;
    pub use crate::request_body::*;
    pub use crate::request_headers::*;
    pub use crate::request_media::*;
    pub use crate::request_optimizer::*;
    pub use crate::request_transport::*;
    pub use crate::request_url::*;
    pub use crate::response_body::*;
    pub use crate::response_build::*;
    pub use crate::response_diagnostics::*;
    pub use crate::response_headers::*;
    pub use crate::response_parse::*;
    pub use crate::response_timeout::*;
}

pub mod transforms {
    pub use crate::codex_chat_history::*;
    pub use crate::codex_error::*;
    pub use crate::gemini_request::*;
    pub use crate::gemini_response::*;
    pub use crate::gemini_schema::*;
    pub use crate::gemini_shadow::*;
    pub use crate::gemini_stream::*;
    pub use crate::gemini_tool_args::*;
    pub use crate::gemini_url::*;
    pub use crate::json_canonical::*;
    pub use crate::openai_chat_stream::*;
    pub use crate::openai_responses_stream::*;
    pub use crate::response_transform::*;
    pub use crate::sse::*;
}

pub mod usage {
    pub use crate::cost::*;
    pub use crate::domain::{UsageRecord, UsageTokens};
    pub use crate::usage::*;
    pub use crate::usage_config::*;
}

pub mod prelude {
    pub use futures::future::BoxFuture;

    pub use super::auth::{ClaudeDesktopModelRouteInput, ProviderAuthInfo, ProviderAuthStrategy};
    pub use super::config::{
        AppSummaryConfig, ProxyAppConfig, ProxyGlobalConfig, ProxyRuntimeConfig,
        ResponseRuntimePolicy,
    };
    pub use super::domain::{
        AppKind, AuthProfileRef, ChannelAttemptResult, InterfaceKind, ModelRoute, ProviderKind,
        ProviderSpec, ProxyRequest, ProxyResult, RoutePlan, RoutePolicy, RouteRequest,
        RouteSelection, UsageTokens,
    };
    pub use super::engine::ProxyEngine;
    pub use super::errors::{ProxyCoreError, ProxyCoreResult};
    pub use super::events::{ProxyCoreEvent, ProxyCoreEventType};
    pub use super::management::{
        AppChannelListQuery, AppChannelResponse, AppListResponse, AppModelListQuery,
        ChannelDeleteResponse, ChannelHealthResetResponse, ChannelKeyDeleteResponse,
        ChannelKeyRecord, ChannelKeyRecordResponse, ChannelKeysResponse, ChannelListQuery,
        ChannelListResponse, ChannelMigrationMaterializeResponse, ChannelMigrationPreviewResponse,
        ChannelModelRecord, ChannelModelsResponse, ChannelReachabilityResult, ChannelRecord,
        ChannelRecordResponse, ChannelTestProbeRequest, ChannelTestResponse, CurrentRouteResponse,
        GroupListQuery, HealthCheckResponse, ProviderListResponse, ProxyChannelKeyPatchRequest,
        ProxyChannelKeyWriteRequest, ProxyChannelModelWriteRequest,
        ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest, ProxyChannelTestRequest,
        ProxyChannelWriteRequest, ProxyStatusResponse, RouteGroupListResponse, RouteResolveRequest,
        RouteResolveResponse,
    };
    pub use super::model_catalog::{
        ClientModelCatalogResponse, FetchedModel, ModelCatalog, RoutableModelList,
    };
    pub use super::ports::CurrentRouteTarget;
    pub use super::ports::{
        AuthInfo, AuthProvider, ChannelHealthReset, ChannelHealthStore, ChannelReachabilityProbe,
        ChannelSource, ForwardPipeline, ModelCatalogProvider, ProviderAttemptResult,
        ProviderHealthStore, ProviderSource, ProxyConfigSource, ProxyEventSink, ProxyServices,
        RoutePolicySource, RouteResolver, UsageSink,
    };
    pub use super::routing::{ChannelQuery, ChannelSpec, DEFAULT_ROUTE_GROUP};
    pub use super::transport::{
        ProxyBody, ProxyCoreResponse, ProxyResponseBody, ProxyTransportResponse,
        ProxyTransportResponseBody,
    };
    pub use super::usage::{TokenUsage, UsageRecord};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_public_api_exposes_integration_contracts() {
        let app = domain::AppKind::from("claude");
        let request = management::HealthCheckRequest::new();
        let body = transport::ProxyBody::Empty;
        let runtime = config::ProxyRuntimeConfig::default();
        let event_payload = events::build_proxy_events_connected_payload(16);

        assert_eq!(app.as_str(), "claude");
        assert_eq!(
            request.response_from_source(management::HealthCheckSource::new("now")),
            management::HealthCheckResponse {
                status: "healthy".to_string(),
                timestamp: "now".to_string()
            }
        );
        assert_eq!(body.into_json().expect("json body"), serde_json::json!({}));
        assert!(!runtime.privacy_filter_enabled);
        assert_eq!(event_payload["bufferSize"], 16);
        assert_eq!(routing::DEFAULT_ROUTE_GROUP, "default");
        assert_eq!(logging::cb::MANUAL_RESET, "CB-006");
    }

    #[test]
    fn prelude_exposes_channel_management_contracts() {
        use prelude::*;

        let write_request = ProxyChannelWriteRequest {
            app_type: "claude".to_string(),
            provider_id: "provider-a".to_string(),
            name: "relay-a".to_string(),
            base_url: "https://relay.example/v1".to_string(),
            interface_kind: "anthropic".to_string(),
            models: vec![ProxyChannelModelWriteRequest {
                public_model: "claude-sonnet".to_string(),
                upstream_model: "relay-sonnet".to_string(),
                ..ProxyChannelModelWriteRequest::default()
            }],
            ..ProxyChannelWriteRequest::default()
        };
        let route_request = RouteResolveRequest {
            app_type: write_request.app_type.clone(),
            requested_model: Some("claude-sonnet".to_string()),
            interface_kind: Some(write_request.interface_kind.clone()),
            route_group: None,
        };
        let channel_list: ChannelListResponse<ChannelRecord> =
            ChannelListResponse::new(Vec::new());
        let test_request = ProxyChannelTestRequest {
            model: route_request.requested_model.clone(),
            interface_kind: route_request.interface_kind.clone(),
        };

        assert_eq!(write_request.models[0].upstream_model, "relay-sonnet");
        assert!(channel_list.channels.is_empty());
        assert_eq!(test_request.requested_model(), Some("claude-sonnet"));
        assert_eq!(test_request.requested_interface(), Some("anthropic"));
    }

    #[test]
    fn prelude_exposes_model_catalog_contracts() {
        use prelude::*;

        let fetched = FetchedModel {
            id: "relay-sonnet".to_string(),
            owned_by: Some("provider-a".to_string()),
        };
        let catalog = ModelCatalog {
            provider_id: "provider-a".to_string(),
            models: vec![fetched.id.clone()],
            raw: serde_json::json!({ "models": [{ "id": fetched.id }] }),
        };
        let client_response = ClientModelCatalogResponse::from_catalog(catalog);
        let routable = RoutableModelList::new(
            "claude",
            Some("default".to_string()),
            Some("anthropic".to_string()),
            Vec::new(),
        );

        assert_eq!(client_response.raw["models"][0]["id"], "relay-sonnet");
        assert_eq!(routable.app_type, "claude");
        assert_eq!(routable.interface_kind.as_deref(), Some("anthropic"));
    }

    #[test]
    fn prelude_exposes_host_service_contracts() {
        use prelude::*;
        use std::sync::Arc;

        struct StubServices;

        fn unavailable<'a, T: Send + 'a>() -> BoxFuture<'a, ProxyCoreResult<T>> {
            Box::pin(async {
                Err(ProxyCoreError::Unavailable(
                    "stub service is compile-only".to_string(),
                ))
            })
        }

        impl ProxyConfigSource for StubServices {
            fn list_apps<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<Vec<AppKind>>> {
                Box::pin(async { Ok(vec![AppKind::Claude]) })
            }

            fn load_global<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyGlobalConfig>> {
                Box::pin(async { Ok(ProxyGlobalConfig::default()) })
            }

            fn load_app<'a>(
                &'a self,
                app: &'a AppKind,
            ) -> BoxFuture<'a, ProxyCoreResult<ProxyAppConfig>> {
                Box::pin(async move {
                    Ok(ProxyAppConfig {
                        app: Some(app.clone()),
                        enabled: true,
                        ..ProxyAppConfig::default()
                    })
                })
            }

            fn load_runtime<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<ProxyRuntimeConfig>> {
                Box::pin(async { Ok(ProxyRuntimeConfig::default()) })
            }
        }

        impl ProviderSource for StubServices {
            fn list_providers<'a>(
                &'a self,
                _app: &'a AppKind,
            ) -> BoxFuture<'a, ProxyCoreResult<Vec<ProviderSpec>>> {
                Box::pin(async { Ok(Vec::new()) })
            }

            fn get_provider<'a>(
                &'a self,
                _app: &'a AppKind,
                _provider_id: &'a str,
            ) -> BoxFuture<'a, ProxyCoreResult<Option<ProviderSpec>>> {
                Box::pin(async { Ok(None) })
            }
        }

        impl ChannelSource for StubServices {
            fn list_channels<'a>(
                &'a self,
                _query: ChannelQuery<'a>,
            ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelSpec>>> {
                Box::pin(async { Ok(Vec::new()) })
            }

            fn get_channel<'a>(
                &'a self,
                _channel_id: &'a str,
            ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelSpec>>> {
                Box::pin(async { Ok(None) })
            }
        }

        impl RoutePolicySource for StubServices {
            fn load_policy<'a>(
                &'a self,
                app: &'a AppKind,
            ) -> BoxFuture<'a, ProxyCoreResult<Option<RoutePolicy>>> {
                let app = app.clone();
                Box::pin(async move {
                    Ok(Some(RoutePolicy {
                        app,
                        groups: Vec::new(),
                        raw: serde_json::json!({}),
                    }))
                })
            }
        }

        impl RouteResolver for StubServices {
            fn resolve<'a>(
                &'a self,
                _request: RouteRequest<'a>,
            ) -> BoxFuture<'a, ProxyCoreResult<RoutePlan>> {
                unavailable()
            }
        }

        impl ChannelHealthStore for StubServices {
            fn record_attempt<'a>(
                &'a self,
                _result: ChannelAttemptResult,
            ) -> BoxFuture<'a, ProxyCoreResult<()>> {
                Box::pin(async { Ok(()) })
            }

            fn reset_channel<'a>(
                &'a self,
                channel_id: &'a str,
            ) -> BoxFuture<'a, ProxyCoreResult<ChannelHealthReset>> {
                let channel_id = channel_id.to_string();
                Box::pin(async move {
                    Ok(ChannelHealthReset {
                        channel_id,
                        app: AppKind::Claude,
                    })
                })
            }
        }

        impl ChannelReachabilityProbe for StubServices {
            fn probe_channel<'a>(
                &'a self,
                _request: ChannelTestProbeRequest,
            ) -> BoxFuture<'a, ProxyCoreResult<ChannelReachabilityResult>> {
                Box::pin(async {
                    Ok(ChannelReachabilityResult {
                        success: true,
                        status: "operational".to_string(),
                        message: "ok".to_string(),
                        latency_ms: Some(1),
                        http_status: Some(200),
                        tested_at: 1,
                        retry_count: 0,
                    })
                })
            }
        }

        impl AuthProvider for StubServices {
            fn resolve_auth<'a>(
                &'a self,
                _app: &'a AppKind,
                _provider: &'a ProviderSpec,
                channel: &'a ChannelSpec,
                _request: &'a ProxyRequest,
            ) -> BoxFuture<'a, ProxyCoreResult<AuthInfo>> {
                Box::pin(async move {
                    Ok(AuthInfo {
                        account_ref: channel.auth_profile.as_ref().map(|value| value.0.clone()),
                        ..AuthInfo::default()
                    })
                })
            }
        }

        impl ModelCatalogProvider for StubServices {
            fn load_catalog<'a>(
                &'a self,
                _app: &'a AppKind,
                provider_id: &'a str,
            ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
                let provider_id = provider_id.to_string();
                Box::pin(async move {
                    Ok(ModelCatalog {
                        provider_id,
                        models: Vec::new(),
                        raw: serde_json::json!({ "data": [] }),
                    })
                })
            }

            fn load_client_catalog<'a>(
                &'a self,
                app: &'a AppKind,
            ) -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>> {
                self.load_catalog(app, "client")
            }
        }

        impl UsageSink for StubServices {
            fn record_usage<'a>(
                &'a self,
                _record: UsageRecord,
            ) -> BoxFuture<'a, ProxyCoreResult<()>> {
                Box::pin(async { Ok(()) })
            }
        }

        impl ProxyEventSink for StubServices {
            fn emit_event<'a>(
                &'a self,
                _event: ProxyCoreEvent,
            ) -> BoxFuture<'a, ProxyCoreResult<()>> {
                Box::pin(async { Ok(()) })
            }
        }

        impl ForwardPipeline for StubServices {
            fn forward<'a>(
                &'a self,
                _request: ProxyRequest,
                _plan: RoutePlan,
            ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>> {
                unavailable()
            }
        }

        impl ProxyServices for StubServices {
            fn config(&self) -> &(dyn ProxyConfigSource + Send + Sync) {
                self
            }

            fn providers(&self) -> &(dyn ProviderSource + Send + Sync) {
                self
            }

            fn channels(&self) -> &(dyn ChannelSource + Send + Sync) {
                self
            }

            fn route_policies(&self) -> &(dyn RoutePolicySource + Send + Sync) {
                self
            }

            fn route_resolver(&self) -> &(dyn RouteResolver + Send + Sync) {
                self
            }

            fn health_store(&self) -> &(dyn ChannelHealthStore + Send + Sync) {
                self
            }

            fn reachability_probe(&self) -> &(dyn ChannelReachabilityProbe + Send + Sync) {
                self
            }

            fn auth_provider(&self) -> &(dyn AuthProvider + Send + Sync) {
                self
            }

            fn model_catalog(&self) -> &(dyn ModelCatalogProvider + Send + Sync) {
                self
            }

            fn usage_sink(&self) -> &(dyn UsageSink + Send + Sync) {
                self
            }

            fn event_sink(&self) -> &(dyn ProxyEventSink + Send + Sync) {
                self
            }

            fn forward_pipeline(&self) -> &(dyn ForwardPipeline + Send + Sync) {
                self
            }
        }

        let engine = ProxyEngine::new(Arc::new(StubServices));
        let app_config = ProxyAppConfig {
            app: Some(AppKind::Claude),
            enabled: true,
            ..ProxyAppConfig::default()
        };
        let summary = AppSummaryConfig::from_proxy_app_config(&app_config);
        let _desktop_routes: Vec<ClaudeDesktopModelRouteInput> = Vec::new();
        let _tokens = UsageTokens {
            input_tokens: 1,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };

        assert_eq!(engine.status().accepted_requests, 0);
        assert!(summary.enabled);
        assert!(!ProxyRuntimeConfig::default().route_events_enabled);
        let _probe: &(dyn ChannelReachabilityProbe + Send + Sync) =
            engine.services().reachability_probe();
        let _events: &(dyn ProxyEventSink + Send + Sync) = engine.services().event_sink();
    }
}
