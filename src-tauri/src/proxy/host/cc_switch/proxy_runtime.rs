//! CC Switch proxy runtime resources.

use crate::app_config::AppType;
use crate::database::Database;
use crate::error::AppError;
use crate::proxy::engine::forward_pipeline::{
    FailoverSwitchSchedulerRef, ForwarderAttemptRuntimeSourceRef, ForwarderAuthSourceRef,
    ForwarderProtocolStateSourceRef, ForwarderRequestSourceRef, ForwarderResponseSourceRef,
    ForwarderRuntimeConfig, ForwarderRuntimeOptions, ForwarderRuntimeStateSourceRef,
    ForwarderTransportSourceRef,
};
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy::error_mapper::forward_error_to_core_error;
use crate::proxy::events::ProxyEventBus;
use crate::proxy::host::cc_switch::channel_auth_profile_attempts::required_forward_attempts_from_sources;
use crate::proxy::host::cc_switch::forward_pipeline::forward_result_to_proxy_result;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy::RequestForwarder;
use crate::proxy_core::api::config::{AppProxyConfig, ResponseRuntimePolicy};
use crate::proxy_core::api::domain::ProxyRequest;
use crate::proxy_core::api::domain::{unsupported_app_kind_config_error, AppKind};
use crate::proxy_core::api::errors::{
    config_error_with_context as core_config_error_with_context, ProxyCoreError, ProxyCoreResult,
};
use crate::proxy_core::api::ports::{
    ChannelKeyRuntimeSource, CopilotOptimizerConfig, CurrentRouteTarget, OptimizerConfig,
    ProxyConfig, ProxyRuntimeStatus, RectifierConfig,
};
use crate::proxy_core::api::routing::{
    current_provider_db_fallback_required, current_provider_id_from_sources, RoutePlan,
};
use crate::proxy_core::api::session::SessionIdResult;
use crate::proxy_core::api::transport::{resolve_response_runtime_policy, ProxyResult};
use futures::future::BoxFuture;
use http::{HeaderMap, Method};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub(crate) trait ProxyServiceRuntimeResources:
    HostForwardRuntime + Clone + Send + Sync
{
    fn db(&self) -> Arc<Database>;
    fn config(&self) -> Arc<RwLock<ProxyConfig>>;
    fn provider_router(&self) -> Arc<ProviderRouter>;
    fn status(&self) -> Arc<RwLock<ProxyRuntimeStatus>>;
    fn start_time(&self) -> Arc<RwLock<Option<std::time::Instant>>>;
    fn current_providers(&self) -> Arc<RwLock<HashMap<String, CurrentRouteTarget>>>;
    fn events(&self) -> Arc<ProxyEventBus>;
}

pub(crate) trait HostForwardRuntime {
    fn forward_host<'a>(
        &'a self,
        channel_key_runtime_source: &'a (dyn ChannelKeyRuntimeSource + Send + Sync),
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>>;
}

#[derive(Clone)]
pub(crate) struct CcSwitchProxyRuntime {
    pub(crate) db: Arc<Database>,
    pub(crate) config: Arc<RwLock<ProxyConfig>>,
    pub(crate) provider_router: Arc<ProviderRouter>,
    pub(crate) status: Arc<RwLock<ProxyRuntimeStatus>>,
    pub(crate) start_time: Arc<RwLock<Option<std::time::Instant>>>,
    pub(crate) events: Arc<ProxyEventBus>,
    pub(crate) current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    pub(crate) attempt_runtime_source: ForwarderAttemptRuntimeSourceRef,
    pub(crate) protocol_state_source: ForwarderProtocolStateSourceRef,
    pub(crate) runtime_state_source: ForwarderRuntimeStateSourceRef,
    pub(crate) auth_source: ForwarderAuthSourceRef,
    pub(crate) request_source: ForwarderRequestSourceRef,
    pub(crate) transport_source: ForwarderTransportSourceRef,
    pub(crate) response_source: ForwarderResponseSourceRef,
    pub(crate) failover_switch_scheduler: FailoverSwitchSchedulerRef,
}

impl ProxyServiceRuntimeResources for CcSwitchProxyRuntime {
    fn db(&self) -> Arc<Database> {
        self.db.clone()
    }

    fn config(&self) -> Arc<RwLock<ProxyConfig>> {
        self.config.clone()
    }

    fn provider_router(&self) -> Arc<ProviderRouter> {
        self.provider_router.clone()
    }

    fn status(&self) -> Arc<RwLock<ProxyRuntimeStatus>> {
        self.status.clone()
    }

    fn start_time(&self) -> Arc<RwLock<Option<std::time::Instant>>> {
        self.start_time.clone()
    }

    fn current_providers(&self) -> Arc<RwLock<HashMap<String, CurrentRouteTarget>>> {
        self.current_providers.clone()
    }

    fn events(&self) -> Arc<ProxyEventBus> {
        self.events.clone()
    }
}

impl HostForwardRuntime for CcSwitchProxyRuntime {
    fn forward_host<'a>(
        &'a self,
        channel_key_runtime_source: &'a (dyn ChannelKeyRuntimeSource + Send + Sync),
        request: ProxyRequest,
        plan: RoutePlan,
    ) -> BoxFuture<'a, ProxyCoreResult<ProxyResult>> {
        Box::pin(async move {
            forward_proxy_request_with_cc_switch_runtime(
                self,
                channel_key_runtime_source,
                request,
                plan,
            )
            .await
        })
    }
}

fn app_error(context: &str, error: AppError) -> ProxyCoreError {
    core_config_error_with_context(context, error)
}

fn current_provider_id_from_settings_for_app(app: &AppKind) -> Option<String> {
    app_type_option_from_proxy_core_app(app)
        .as_ref()
        .and_then(crate::settings::get_current_provider)
}

pub(crate) fn forward_current_provider_id_from_source(
    settings_current_provider_id: Option<&str>,
    load_db_current_provider_id: impl FnOnce() -> Option<String>,
) -> String {
    let db_current_provider_id =
        if current_provider_db_fallback_required(settings_current_provider_id) {
            load_db_current_provider_id()
        } else {
            None
        };
    current_provider_id_from_sources(
        settings_current_provider_id,
        db_current_provider_id.as_deref(),
    )
}

fn forward_current_provider_id_from_db_sources(db: &Database, app_type: &AppType) -> String {
    let app = AppKind::from(app_type);
    let settings_current_provider_id = current_provider_id_from_settings_for_app(&app);
    forward_current_provider_id_from_source(settings_current_provider_id.as_deref(), || {
        db.get_current_provider(app_type.as_str()).ok().flatten()
    })
}

pub(crate) fn response_runtime_policy_from_app_proxy_config(
    config: &AppProxyConfig,
) -> ResponseRuntimePolicy {
    resolve_response_runtime_policy(
        config.auto_failover_enabled,
        config.max_retries,
        config.non_streaming_timeout as u64,
        config.streaming_first_byte_timeout as u64,
        config.streaming_idle_timeout as u64,
    )
}

pub(crate) fn forwarder_runtime_options_from_app_proxy_config(
    config: &AppProxyConfig,
) -> ForwarderRuntimeOptions {
    let policy = response_runtime_policy_from_app_proxy_config(config);
    ForwarderRuntimeOptions {
        non_streaming_timeout: policy.timeout.non_streaming_timeout,
        streaming_first_byte_timeout: policy.timeout.streaming.first_byte_timeout,
        streaming_idle_timeout: policy.timeout.streaming.idle_timeout,
        max_retries: policy.max_retries,
    }
}

pub(crate) fn forwarder_runtime_config_from_sources(
    app_config: &AppProxyConfig,
    rectifier: RectifierConfig,
    optimizer: OptimizerConfig,
    copilot_optimizer: CopilotOptimizerConfig,
) -> ForwarderRuntimeConfig {
    ForwarderRuntimeConfig {
        options: forwarder_runtime_options_from_app_proxy_config(app_config),
        rectifier,
        optimizer,
        copilot_optimizer,
    }
}

async fn forwarder_runtime_config_from_db_sources(
    db: &Database,
    app_type: &AppType,
) -> ProxyCoreResult<ForwarderRuntimeConfig> {
    let app_config = db
        .get_proxy_config_for_app(app_type.as_str())
        .await
        .map_err(|error| app_error("load app proxy config", error))?;

    Ok(forwarder_runtime_config_from_sources(
        &app_config,
        db.get_rectifier_config().unwrap_or_default(),
        db.get_optimizer_config().unwrap_or_default(),
        db.get_copilot_optimizer_config().unwrap_or_default(),
    ))
}

impl From<&AppType> for AppKind {
    fn from(value: &AppType) -> Self {
        Self::from(value.as_str())
    }
}

pub(crate) fn app_type_option_from_proxy_core_app(app: &AppKind) -> Option<AppType> {
    app.as_str().parse::<AppType>().ok()
}

pub(crate) fn app_type_from_proxy_core_app(app: &AppKind) -> ProxyCoreResult<AppType> {
    app.as_str()
        .parse::<AppType>()
        .map_err(unsupported_app_kind_config_error)
}

pub(crate) struct ForwardRuntimeRequest {
    pub(crate) app_type: AppType,
    pub(crate) method: Method,
    pub(crate) endpoint: String,
    pub(crate) headers: HeaderMap,
    pub(crate) extensions: http::Extensions,
    pub(crate) body: Value,
    pub(crate) session_result: SessionIdResult,
}

pub(crate) fn forward_runtime_request_from_proxy_request(
    request: ProxyRequest,
) -> ProxyCoreResult<ForwardRuntimeRequest> {
    let ProxyRequest {
        app,
        method,
        endpoint,
        headers,
        extensions,
        body,
        ..
    } = request;
    let app_type = app_type_from_proxy_core_app(&app)?;
    let body = body.into_json()?;
    let session_result = extract_proxy_session_id(&headers, &body, app_type.as_str());
    Ok(ForwardRuntimeRequest {
        app_type,
        method,
        endpoint,
        headers,
        extensions,
        body,
        session_result,
    })
}

#[derive(Clone)]
struct ForwarderRuntimeHostResources {
    attempt_runtime_source: ForwarderAttemptRuntimeSourceRef,
    protocol_state_source: ForwarderProtocolStateSourceRef,
    runtime_state_source: ForwarderRuntimeStateSourceRef,
    auth_source: ForwarderAuthSourceRef,
    request_source: ForwarderRequestSourceRef,
    transport_source: ForwarderTransportSourceRef,
    response_source: ForwarderResponseSourceRef,
    failover_switch_scheduler: FailoverSwitchSchedulerRef,
}

fn forwarder_runtime_host_resources_from_runtime(
    runtime: &CcSwitchProxyRuntime,
) -> ForwarderRuntimeHostResources {
    ForwarderRuntimeHostResources {
        attempt_runtime_source: runtime.attempt_runtime_source.clone(),
        protocol_state_source: runtime.protocol_state_source.clone(),
        runtime_state_source: runtime.runtime_state_source.clone(),
        auth_source: runtime.auth_source.clone(),
        request_source: runtime.request_source.clone(),
        transport_source: runtime.transport_source.clone(),
        response_source: runtime.response_source.clone(),
        failover_switch_scheduler: runtime.failover_switch_scheduler.clone(),
    }
}

async fn forward_proxy_request_with_cc_switch_runtime(
    runtime: &CcSwitchProxyRuntime,
    channel_key_runtime_source: &(dyn ChannelKeyRuntimeSource + Send + Sync),
    request: ProxyRequest,
    plan: RoutePlan,
) -> ProxyCoreResult<ProxyResult> {
    forward_proxy_request_with_host_runtime(
        &runtime.db,
        forwarder_runtime_host_resources_from_runtime(runtime),
        channel_key_runtime_source,
        request,
        plan,
    )
    .await
}

async fn forward_with_preplanned_host_runtime(
    resources: ForwarderRuntimeHostResources,
    request: ForwardRuntimeRequest,
    plan: RoutePlan,
    forwarder_config: ForwarderRuntimeConfig,
    current_provider_id: String,
    attempts: Vec<ForwardAttempt>,
) -> ProxyCoreResult<ProxyResult> {
    let ForwarderRuntimeHostResources {
        attempt_runtime_source,
        protocol_state_source,
        runtime_state_source,
        auth_source,
        request_source,
        transport_source,
        response_source,
        failover_switch_scheduler,
    } = resources;
    let ForwardRuntimeRequest {
        app_type,
        method,
        endpoint,
        headers,
        extensions,
        body,
        session_result,
    } = request;
    let forwarder = RequestForwarder::new_preplanned(
        attempt_runtime_source,
        protocol_state_source,
        runtime_state_source,
        auth_source,
        request_source,
        transport_source,
        response_source,
        failover_switch_scheduler,
        forwarder_config,
        current_provider_id,
        session_result.session_id,
        session_result.client_provided,
    );

    let result = forwarder
        .forward_with_preplanned_attempts(
            &app_type, method, &endpoint, body, headers, extensions, attempts,
        )
        .await
        .map_err(forward_error_to_core_error)?;
    Ok(forward_result_to_proxy_result(result, plan))
}

async fn forward_proxy_request_with_host_runtime(
    db: &Database,
    resources: ForwarderRuntimeHostResources,
    channel_key_runtime_source: &(dyn ChannelKeyRuntimeSource + Send + Sync),
    request: ProxyRequest,
    plan: RoutePlan,
) -> ProxyCoreResult<ProxyResult> {
    let forward_request = forward_runtime_request_from_proxy_request(request)?;
    let app_type = forward_request.app_type.clone();
    let forwarder_config = forwarder_runtime_config_from_db_sources(db, &app_type).await?;
    let current_provider_id = forward_current_provider_id_from_db_sources(db, &app_type);
    let all_providers = db
        .get_all_providers(app_type.as_str())
        .map_err(|error| app_error("load host providers", error))?;
    let attempts = required_forward_attempts_from_sources(
        &app_type,
        &all_providers,
        &plan,
        channel_key_runtime_source,
    )?;

    forward_with_preplanned_host_runtime(
        resources,
        forward_request,
        plan,
        forwarder_config,
        current_provider_id,
        attempts,
    )
    .await
}

pub(crate) fn extract_proxy_session_id(
    headers: &HeaderMap,
    body: &Value,
    client_format: &str,
) -> SessionIdResult {
    crate::proxy_core::api::session::extract_session_id_with_generator(
        headers,
        body,
        client_format,
        || Uuid::new_v4().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::codex_chat_history::CodexChatHistoryStore;
    use crate::proxy::events::ProxyEventBus;
    use crate::proxy::host::cc_switch::channel_key_runtime_source::channel_key_runtime_source_from_database;
    use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
    use crate::proxy_core::api::config::ResponseTimeoutConfig;
    use crate::proxy_core::api::domain::{
        ChannelOverrides, ModelCapabilities, ModelRoute, ProviderKind, ProviderSpec, RetryPolicy,
        UpstreamEndpoint,
    };
    use crate::proxy_core::api::routing::{
        ChannelSpec, ChannelStatus, InterfaceKind, RouteSelection, DEFAULT_ROUTE_GROUP,
    };
    use crate::proxy_core::api::session::SessionIdSource;
    use crate::proxy_core::api::transforms::GeminiShadowStore;
    use crate::proxy_core::api::transport::ProxyBody;
    use bytes::Bytes;
    use serde_json::json;

    fn app_proxy_config_fixture() -> AppProxyConfig {
        AppProxyConfig {
            app_type: "claude".to_string(),
            enabled: true,
            auto_failover_enabled: true,
            max_retries: 3,
            streaming_first_byte_timeout: 60,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
            circuit_failure_threshold: 4,
            circuit_success_threshold: 2,
            circuit_timeout_seconds: 60,
            circuit_error_rate_threshold: 0.6,
            circuit_min_requests: 10,
        }
    }

    fn provider_spec(id: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: id.to_string(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: Default::default(),
        }
    }

    fn channel_spec(id: &str, priority: i64, model: &str) -> ChannelSpec {
        ChannelSpec {
            id: id.to_string(),
            provider_id: "provider-a".to_string(),
            app: AppKind::Claude,
            name: id.to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: format!("https://{id}.example.com/v1"),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::OpenAiResponses,
            auth_profile: None,
            models: vec![ModelRoute {
                public_model: model.to_string(),
                upstream_model: format!("upstream-{model}"),
                capabilities: ModelCapabilities::default(),
                pricing_model: None,
                request_overrides: json!({}),
                response_overrides: json!({}),
            }],
            groups: vec![DEFAULT_ROUTE_GROUP.to_string()],
            priority,
            weight: 100,
            retry_policy: RetryPolicy::default(),
            health_policy: Default::default(),
            overrides: ChannelOverrides::default(),
            tags: Vec::new(),
            metadata: json!({}),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        }
    }

    fn route_plan(provider_id: &str, channel_id: &str) -> RoutePlan {
        let mut channel = channel_spec(channel_id, 100, "sonnet");
        channel.provider_id = provider_id.to_string();
        let model_route = channel.models.first().cloned();
        let selection = RouteSelection {
            provider: provider_spec(provider_id),
            channel,
            model_route,
            inbound_interface: InterfaceKind::AnthropicMessages,
            outbound_interface: InterfaceKind::OpenAiResponses,
        };
        RoutePlan {
            selection,
            selections: Vec::new(),
            attempts: Vec::new(),
        }
    }

    fn proxy_request() -> ProxyRequest {
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({ "model": "sonnet", "messages": [] })),
        );
        request.requested_model = Some("sonnet".to_string());
        request
    }

    fn runtime(db: Arc<Database>) -> CcSwitchProxyRuntime {
        let events = Arc::new(ProxyEventBus::default());
        let status = Arc::new(RwLock::new(ProxyRuntimeStatus::default()));
        let current_providers = Arc::new(RwLock::new(HashMap::new()));
        let gemini_shadow = Arc::new(GeminiShadowStore::default());
        let codex_chat_history = Arc::new(CodexChatHistoryStore::default());
        let provider_router = Arc::new(provider_router_from_database(db.clone()));
        CcSwitchProxyRuntime {
            db: db.clone(),
            config: Arc::new(RwLock::new(ProxyConfig::default())),
            provider_router: provider_router.clone(),
            status: status.clone(),
            start_time: Arc::new(RwLock::new(None)),
            events: events.clone(),
            current_providers: current_providers.clone(),
            attempt_runtime_source:
                crate::proxy::host::cc_switch::forwarder_attempt_runtime_source::forwarder_attempt_runtime_source_from_runtime_sources(
                    provider_router,
                    db.clone(),
                ),
            protocol_state_source:
                crate::proxy::host::cc_switch::forwarder_protocol_state_source::forwarder_protocol_state_source_from_runtime_parts(
                    gemini_shadow,
                    codex_chat_history,
                ),
            runtime_state_source:
                crate::proxy::host::cc_switch::forwarder_runtime_state_source::forwarder_runtime_state_source_from_runtime_parts(
                    status,
                    current_providers,
                    events,
                ),
            auth_source:
                crate::proxy::host::cc_switch::forwarder_auth_source::default_forwarder_auth_source(
                ),
            request_source:
                crate::proxy::host::cc_switch::forwarder_request_source::default_forwarder_request_source(
                ),
            transport_source:
                crate::proxy::host::cc_switch::forwarder_transport_source::default_forwarder_transport_source(
                ),
            response_source:
                crate::proxy::host::cc_switch::forwarder_response_source::default_forwarder_response_source(
                ),
            failover_switch_scheduler:
                crate::proxy::host::cc_switch::failover_switch::noop_failover_switch_scheduler(),
        }
    }

    #[tokio::test]
    async fn runtime_forward_pipeline_requires_matching_host_provider() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let runtime = runtime(db.clone());
        let channel_key_runtime_source = channel_key_runtime_source_from_database(db);

        let err = runtime
            .forward_host(
                &channel_key_runtime_source,
                proxy_request(),
                route_plan("missing-provider", "channel-a"),
            )
            .await
            .expect_err("missing provider should stop before forwarding");

        assert!(matches!(err, ProxyCoreError::Unavailable(_)));
        assert!(err
            .to_string()
            .contains("route plan providers are not configured"));
    }

    #[test]
    fn app_type_conversion_preserves_known_and_custom_names() {
        assert_eq!(AppKind::from(&AppType::Claude), AppKind::Claude);
        assert_eq!(
            AppKind::from(&AppType::ClaudeDesktop),
            AppKind::ClaudeDesktop
        );
        assert_eq!(AppKind::from(&AppType::Codex), AppKind::Codex);
        assert_eq!(
            AppKind::from(&AppType::OpenClaw),
            AppKind::Custom("openclaw".to_string())
        );
        assert_eq!(
            app_type_from_proxy_core_app(&AppKind::Claude).expect("claude app"),
            AppType::Claude
        );
        assert_eq!(
            app_type_option_from_proxy_core_app(&AppKind::Custom("openclaw".to_string())),
            Some(AppType::OpenClaw)
        );
        assert!(matches!(
            app_type_from_proxy_core_app(&AppKind::Custom("unknown-app".to_string())),
            Err(ProxyCoreError::Config(message))
                if message.starts_with("unsupported app kind:")
                    && message.contains("unknown-app")
        ));
        assert_eq!(
            crate::proxy_core::api::domain::unsupported_app_kind_error_message(
                "invalid app: openclaw"
            ),
            "unsupported app kind: invalid app: openclaw"
        );

        let forward_request = forward_runtime_request_from_proxy_request(ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Bytes(Bytes::from_static(br#"{"ok":true}"#)),
        ))
        .expect("forward request");
        assert_eq!(forward_request.app_type, AppType::Claude);
        assert_eq!(forward_request.method, Method::POST);
        assert_eq!(forward_request.endpoint, "/v1/messages");
        assert_eq!(forward_request.body, json!({"ok": true}));
        assert_eq!(
            forward_request.session_result.source,
            SessionIdSource::Generated
        );
        assert!(!forward_request.session_result.client_provided);
        Uuid::parse_str(&forward_request.session_result.session_id)
            .expect("generated forward session id should be a UUID");

        let invalid_request = match forward_runtime_request_from_proxy_request(ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Bytes(Bytes::from_static(b"{bad-json")),
        )) {
            Ok(_) => panic!("invalid JSON body should fail"),
            Err(error) => error,
        };
        assert!(matches!(
            invalid_request,
            ProxyCoreError::InvalidRequest(message) if message.contains("invalid JSON body")
        ));
    }

    #[test]
    fn forward_current_provider_source_prefers_settings_without_db_lookup() {
        let mut db_lookup_called_for_settings = false;
        assert_eq!(
            forward_current_provider_id_from_source(Some("settings-provider"), || {
                db_lookup_called_for_settings = true;
                Some("db-provider".to_string())
            }),
            "settings-provider"
        );
        assert!(!db_lookup_called_for_settings);

        assert_eq!(
            forward_current_provider_id_from_source(None, || Some("db-provider".to_string())),
            "db-provider"
        );
        assert_eq!(forward_current_provider_id_from_source(None, || None), "");
    }

    #[test]
    fn runtime_policy_and_options_follow_app_proxy_config() {
        let app_config = app_proxy_config_fixture();

        let enabled_policy = response_runtime_policy_from_app_proxy_config(&app_config);
        assert_eq!(enabled_policy.max_retries, 3);
        assert_eq!(enabled_policy.timeout.non_streaming_timeout, 600);
        assert_eq!(enabled_policy.timeout.streaming.first_byte_timeout, 60);
        assert_eq!(enabled_policy.timeout.streaming.idle_timeout, 120);
        assert_eq!(
            forwarder_runtime_options_from_app_proxy_config(&app_config),
            ForwarderRuntimeOptions {
                non_streaming_timeout: 600,
                streaming_first_byte_timeout: 60,
                streaming_idle_timeout: 120,
                max_retries: 3,
            }
        );

        let mut disabled_app_config = app_config;
        disabled_app_config.auto_failover_enabled = false;
        let disabled_policy = response_runtime_policy_from_app_proxy_config(&disabled_app_config);
        assert_eq!(disabled_policy.max_retries, 0);
        assert_eq!(disabled_policy.timeout, ResponseTimeoutConfig::default());
        assert_eq!(
            forwarder_runtime_options_from_app_proxy_config(&disabled_app_config),
            ForwarderRuntimeOptions {
                non_streaming_timeout: ResponseTimeoutConfig::default().non_streaming_timeout,
                streaming_first_byte_timeout: ResponseTimeoutConfig::default()
                    .streaming
                    .first_byte_timeout,
                streaming_idle_timeout: ResponseTimeoutConfig::default().streaming.idle_timeout,
                max_retries: 0,
            }
        );
    }

    #[test]
    fn forwarder_runtime_config_preserves_runtime_sources() {
        let app_config = app_proxy_config_fixture();

        let forwarder_config = forwarder_runtime_config_from_sources(
            &app_config,
            RectifierConfig {
                request_media_fallback: false,
                ..RectifierConfig::default()
            },
            OptimizerConfig {
                enabled: true,
                cache_ttl: "2h".to_string(),
                ..OptimizerConfig::default()
            },
            CopilotOptimizerConfig {
                warmup_model: "gpt-5".to_string(),
                ..CopilotOptimizerConfig::default()
            },
        );

        assert_eq!(
            forwarder_config.options,
            ForwarderRuntimeOptions {
                non_streaming_timeout: 600,
                streaming_first_byte_timeout: 60,
                streaming_idle_timeout: 120,
                max_retries: 3,
            }
        );
        assert!(!forwarder_config.rectifier.request_media_fallback);
        assert!(forwarder_config.optimizer.enabled);
        assert_eq!(forwarder_config.optimizer.cache_ttl, "2h");
        assert_eq!(forwarder_config.copilot_optimizer.warmup_model, "gpt-5");
    }

    #[test]
    fn extract_proxy_session_id_generates_uuid_when_core_needs_new_session_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_proxy_session_id(&headers, &body, "claude");

        uuid::Uuid::parse_str(&result.session_id).expect("generated session id should be a UUID");
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }
}
