#[cfg(test)]
use crate::database::Database;
#[cfg(test)]
use crate::error::AppError;
#[cfg(test)]
use crate::proxy::codex_chat_history::CodexChatHistoryStore;
#[cfg(test)]
use crate::proxy::events::ProxyEventBus;
#[cfg(test)]
use crate::proxy::host::cc_switch::auth_provider::CcSwitchAuthProvider;
#[cfg(test)]
use crate::proxy::host::cc_switch::channel_auth_profile_attempts::{
    apply_channel_auth_profile_providers_from_source, forward_attempts_from_plan,
    host_providers_for_plan,
};
#[cfg(test)]
use crate::proxy::host::cc_switch::channel_key_runtime_source::channel_key_runtime_source_from_database;
#[cfg(test)]
use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
#[cfg(test)]
use crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime;
#[cfg(test)]
use crate::proxy::host::cc_switch::route_resolver::RouteRequest;
#[cfg(test)]
use crate::proxy::transport::upstream::hyper_client::ProxyResponse;
#[cfg(test)]
use crate::proxy_core_adapter::{
    client_model_catalog_from_optional_raw, forward_result_to_proxy_result,
    management_route_response_from_router_source, AppKind, AuthProfileRef, AuthProvider,
    ChannelAttemptResult, ChannelQuery, ChannelSpec, GeminiShadowStore, ProviderSpec,
    ProxyCoreEvent, ProxyRequest, ProxyServices, RoutePlan,
};
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::RwLock;

#[cfg(test)]
use crate::proxy_core_adapter::DEFAULT_ROUTE_GROUP;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppType;
    use crate::provider::Provider;
    use crate::proxy_core_adapter::{
        proxy_response_to_core_response, ChannelStatus, ProviderKind, ProxyBody,
        ProxyChannelKeyWriteRequest, ProxyChannelModelWriteRequest, ProxyChannelWriteRequest,
        ProxyConfig, ProxyCoreChannelOverrides as ChannelOverrides, ProxyCoreError,
        ProxyCoreEventType, ProxyCoreInterfaceKind as InterfaceKind,
        ProxyCoreModelCapabilities as ModelCapabilities, ProxyCoreModelRoute as ModelRoute,
        ProxyCoreResult, ProxyCoreUpstreamEndpoint as UpstreamEndpoint, ProxyEngine,
        ProxyResponseBody, ProxyRuntimeStatus, ResolvedChannelAttempt, RetryPolicy,
        RouteResolveRequest, RouteSelection, UsageRecord, UsageTokens,
    };
    use bytes::Bytes;
    use futures::StreamExt;
    use http::{Method, StatusCode};
    use indexmap::IndexMap;
    use serde_json::json;
    use std::ffi::OsString;

    type CcSwitchProxyServices =
        crate::proxy::host::cc_switch::proxy_services::CcSwitchProxyServices<
            crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime,
        >;

    struct IsolatedTestHome {
        _dir: tempfile::TempDir,
        original_test_home: Option<OsString>,
    }

    impl IsolatedTestHome {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("temp home");
            let original_test_home = std::env::var_os("CC_SWITCH_TEST_HOME");
            std::env::set_var("CC_SWITCH_TEST_HOME", dir.path());
            Self {
                _dir: dir,
                original_test_home,
            }
        }
    }

    impl Drop for IsolatedTestHome {
        fn drop(&mut self) {
            match &self.original_test_home {
                Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

    fn save_claude_provider(db: &Database) {
        let provider = Provider::with_id(
            "anthropic-main".to_string(),
            "Anthropic Main".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay-a.example.com/v1/",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider)
            .expect("save provider");
        db.set_current_provider("claude", "anthropic-main")
            .expect("set current provider");
    }

    fn create_materialized_channel(
        db: &Database,
        id: &str,
        priority: i64,
        weight: u32,
        upstream_model: &str,
    ) {
        db.create_proxy_channel(ProxyChannelWriteRequest {
            id: Some(id.to_string()),
            provider_id: "anthropic-main".to_string(),
            app_type: "claude".to_string(),
            name: id.to_string(),
            base_url: format!("https://{id}.example.com/v1"),
            interface_kind: "openai_responses".to_string(),
            priority,
            weight,
            models: vec![ProxyChannelModelWriteRequest {
                public_model: "sonnet-public".to_string(),
                upstream_model: upstream_model.to_string(),
                ..ProxyChannelModelWriteRequest::default()
            }],
            ..ProxyChannelWriteRequest::default()
        })
        .expect("create materialized channel");
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

    fn apply_channel_auth_profile_providers_with_runtime_source(
        db: &Arc<Database>,
        app_type: &AppType,
        providers: &IndexMap<String, Provider>,
        attempts: &mut [crate::proxy::route_attempt::ForwardAttempt],
    ) -> ProxyCoreResult<()> {
        let channel_key_runtime_source = channel_key_runtime_source_from_database(db.clone());
        apply_channel_auth_profile_providers_from_source(
            app_type,
            providers,
            attempts,
            &channel_key_runtime_source,
        )
    }

    #[test]
    fn channel_provider_auth_profile_sets_auth_provider_without_changing_route_provider() {
        let route_provider = Provider::with_id(
            "route-provider".to_string(),
            "Route Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "route-key" } }),
            None,
        );
        let auth_provider = Provider::with_id(
            "auth-provider".to_string(),
            "Auth Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "auth-key" } }),
            None,
        );
        let mut providers = IndexMap::new();
        providers.insert(route_provider.id.clone(), route_provider);
        providers.insert(auth_provider.id.clone(), auth_provider);
        let mut plan = route_plan("route-provider", "channel-auth");
        plan.selection.channel.auth_profile =
            Some(AuthProfileRef::new("provider:claude:auth-provider"));

        let route_providers = host_providers_for_plan(&providers, &plan).expect("route providers");
        let mut attempts = forward_attempts_from_plan(&AppType::Claude, &route_providers, &plan);
        let db = Arc::new(Database::memory().expect("memory db"));
        apply_channel_auth_profile_providers_with_runtime_source(
            &db,
            &AppType::Claude,
            &providers,
            &mut attempts,
        )
        .expect("apply auth profiles");

        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].provider().id, "route-provider");
        assert_eq!(attempts[0].auth_provider().id, "auth-provider");
        assert_eq!(
            attempts[0]
                .auth_provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("auth-key")
        );
    }

    #[test]
    fn channel_auth_profile_ignores_unknown_or_cross_app_provider_refs() {
        let route_provider = Provider::with_id(
            "route-provider".to_string(),
            "Route Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "route-key" } }),
            None,
        );
        let mut providers = IndexMap::new();
        providers.insert(route_provider.id.clone(), route_provider);
        let mut plan = route_plan("route-provider", "channel-auth");
        plan.selection.channel.auth_profile =
            Some(AuthProfileRef::new("provider:codex:auth-provider"));

        let route_providers = host_providers_for_plan(&providers, &plan).expect("route providers");
        let mut attempts = forward_attempts_from_plan(&AppType::Claude, &route_providers, &plan);
        let db = Arc::new(Database::memory().expect("memory db"));
        apply_channel_auth_profile_providers_with_runtime_source(
            &db,
            &AppType::Claude,
            &providers,
            &mut attempts,
        )
        .expect("apply cross-app profile");

        assert_eq!(attempts[0].provider().id, "route-provider");
        assert_eq!(attempts[0].auth_provider().id, "route-provider");
    }

    #[test]
    fn channel_provider_auth_profile_preserves_provider_id_spacing() {
        let route_provider = Provider::with_id(
            "route-provider".to_string(),
            "Route Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "route-key" } }),
            None,
        );
        let auth_provider = Provider::with_id(
            "auth-provider".to_string(),
            "Auth Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "auth-key" } }),
            None,
        );
        let mut providers = IndexMap::new();
        providers.insert(route_provider.id.clone(), route_provider);
        providers.insert(auth_provider.id.clone(), auth_provider);
        let mut plan = route_plan("route-provider", "channel-auth");
        plan.selection.channel.auth_profile =
            Some(AuthProfileRef::new("provider:claude: auth-provider"));

        let route_providers = host_providers_for_plan(&providers, &plan).expect("route providers");
        let mut attempts = forward_attempts_from_plan(&AppType::Claude, &route_providers, &plan);
        let db = Arc::new(Database::memory().expect("memory db"));
        apply_channel_auth_profile_providers_with_runtime_source(
            &db,
            &AppType::Claude,
            &providers,
            &mut attempts,
        )
        .expect("apply spaced provider profile");

        assert_eq!(attempts[0].provider().id, "route-provider");
        assert_eq!(attempts[0].auth_provider().id, "route-provider");
    }

    #[test]
    fn channel_key_auth_profile_fails_closed_for_missing_key() {
        let route_provider = Provider::with_id(
            "route-provider".to_string(),
            "Route Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "route-key" } }),
            None,
        );
        let mut providers = IndexMap::new();
        providers.insert(route_provider.id.clone(), route_provider);
        let mut plan = route_plan("route-provider", "channel-auth");
        plan.selection.channel.auth_profile = Some(AuthProfileRef::new("channel-key:manual"));
        let route_providers = host_providers_for_plan(&providers, &plan).expect("route providers");
        let mut attempts = forward_attempts_from_plan(&AppType::Claude, &route_providers, &plan);
        let db = Arc::new(Database::memory().expect("memory db"));
        let error = apply_channel_auth_profile_providers_with_runtime_source(
            &db,
            &AppType::Claude,
            &providers,
            &mut attempts,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ProxyCoreError::Auth(message)
                if message.contains("channel_id=channel-auth")
                    && message.contains("key_ref=manual")
        ));
    }

    #[test]
    fn channel_key_auth_profile_sets_auth_key_without_changing_route_provider() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        db.create_proxy_channel(ProxyChannelWriteRequest {
            id: Some("channel-auth-key".to_string()),
            provider_id: "anthropic-main".to_string(),
            app_type: "claude".to_string(),
            name: "Channel Key Relay".to_string(),
            base_url: "https://channel-key.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            auth_profile_ref: Some("channel-key:primary".to_string()),
            ..Default::default()
        })
        .expect("create channel");
        db.upsert_proxy_channel_key(
            "channel-auth-key",
            "primary",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-channel-key".to_string(),
                status: "enabled".to_string(),
                priority: 10,
                weight: 100,
            },
        )
        .expect("upsert channel key");
        let providers = db.get_all_providers("claude").expect("load providers");
        let mut plan = route_plan("anthropic-main", "channel-auth-key");
        plan.selection.channel.auth_profile = Some(AuthProfileRef::new("channel-key:primary"));

        let route_providers = host_providers_for_plan(&providers, &plan).expect("route providers");
        let mut attempts = forward_attempts_from_plan(&AppType::Claude, &route_providers, &plan);
        apply_channel_auth_profile_providers_with_runtime_source(
            &db,
            &AppType::Claude,
            &providers,
            &mut attempts,
        )
        .expect("apply channel key auth profile");

        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].provider().id, "anthropic-main");
        assert_eq!(attempts[0].auth_provider().id, "anthropic-main");
        assert_eq!(
            attempts[0]
                .provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            None
        );
        assert_eq!(
            attempts[0]
                .auth_provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("sk-channel-key")
        );

        db.upsert_proxy_channel_key(
            "channel-auth-key",
            "primary",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-channel-key".to_string(),
                status: "disabled".to_string(),
                priority: 10,
                weight: 100,
            },
        )
        .expect("disable channel key");
        let mut attempts = forward_attempts_from_plan(&AppType::Claude, &route_providers, &plan);
        let error = apply_channel_auth_profile_providers_with_runtime_source(
            &db,
            &AppType::Claude,
            &providers,
            &mut attempts,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ProxyCoreError::Auth(message)
                if message.contains("channel_id=channel-auth-key")
                    && message.contains("key_ref=primary")
        ));
    }

    #[test]
    fn channel_key_auth_profile_keeps_same_key_ref_scoped_per_channel() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);

        for (channel_id, key_value) in [
            ("channel-auth-a", "sk-channel-a"),
            ("channel-auth-b", "sk-channel-b"),
        ] {
            db.create_proxy_channel(ProxyChannelWriteRequest {
                id: Some(channel_id.to_string()),
                provider_id: "anthropic-main".to_string(),
                app_type: "claude".to_string(),
                name: channel_id.to_string(),
                base_url: format!("https://{channel_id}.example.com/v1"),
                interface_kind: "anthropic_messages".to_string(),
                auth_profile_ref: Some("channel-key:primary".to_string()),
                ..ProxyChannelWriteRequest::default()
            })
            .expect("create channel");
            db.upsert_proxy_channel_key(
                channel_id,
                "primary",
                ProxyChannelKeyWriteRequest {
                    key_value: key_value.to_string(),
                    status: "enabled".to_string(),
                    priority: 10,
                    weight: 100,
                },
            )
            .expect("upsert channel key");
        }

        let providers = db.get_all_providers("claude").expect("load providers");
        let mut attempts = Vec::new();
        for channel_id in ["channel-auth-a", "channel-auth-b"] {
            let mut plan = route_plan("anthropic-main", channel_id);
            plan.selection.channel.auth_profile = Some(AuthProfileRef::new("channel-key:primary"));
            let route_providers =
                host_providers_for_plan(&providers, &plan).expect("route providers");
            attempts.extend(forward_attempts_from_plan(
                &AppType::Claude,
                &route_providers,
                &plan,
            ));
        }

        apply_channel_auth_profile_providers_with_runtime_source(
            &db,
            &AppType::Claude,
            &providers,
            &mut attempts,
        )
        .expect("apply channel key auth profiles");

        let auth_keys: Vec<_> = attempts
            .iter()
            .map(|attempt| {
                attempt
                    .auth_provider()
                    .settings_config
                    .pointer("/env/ANTHROPIC_API_KEY")
                    .and_then(Value::as_str)
                    .expect("channel auth key")
                    .to_string()
            })
            .collect();
        assert_eq!(auth_keys, vec!["sk-channel-a", "sk-channel-b"]);
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
        let current_providers = Arc::new(RwLock::new(std::collections::HashMap::new()));
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
                crate::proxy_core_adapter::forwarder_attempt_runtime_source_from_router(
                    provider_router,
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
            auth_source: crate::proxy_core_adapter::default_forwarder_auth_source(),
            request_source: crate::proxy_core_adapter::default_forwarder_request_source(),
            transport_source:
                crate::proxy::host::cc_switch::forwarder_transport_source::default_forwarder_transport_source(
                ),
            response_source:
                crate::proxy::host::cc_switch::forwarder_response_source::default_forwarder_response_source(
                ),
            failover_switch_scheduler: crate::proxy_core_adapter::noop_failover_switch_scheduler(),
        }
    }

    #[tokio::test]
    async fn config_source_projects_proxy_configs_through_core() {
        let _home = IsolatedTestHome::new();
        let db = Arc::new(Database::memory().expect("memory db"));
        let services = CcSwitchProxyServices::new(db);

        let global = services
            .config()
            .load_global()
            .await
            .expect("load global config");
        assert_eq!(global.bind_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(global.bind_port, Some(15721));
        assert_eq!(global.raw["listenAddress"], json!("127.0.0.1"));

        let app = services
            .config()
            .load_app(&AppKind::Claude)
            .await
            .expect("load app config");
        assert_eq!(app.app, Some(AppKind::Claude));
        assert_eq!(app.default_group.as_deref(), Some(DEFAULT_ROUTE_GROUP));
        assert_eq!(app.raw["appType"], json!("claude"));
        assert!(app.raw["currentProviderId"].is_null());
        assert!(app.rectifier.enabled);
        assert_eq!(app.rectifier.raw["enabled"], json!(true));
        assert_eq!(app.optimizer.raw["cacheTtl"], json!("1h"));
        assert_eq!(
            app.copilot_optimizer.raw["warmupModel"],
            json!("gpt-5-mini")
        );

        let summary = services
            .config()
            .load_app_summary(&AppKind::Claude)
            .await
            .expect("load app summary config");
        assert_eq!(summary.enabled, app.enabled);
        assert_eq!(
            summary.auto_failover_enabled,
            app.raw["autoFailoverEnabled"].as_bool().unwrap_or_default()
        );

        let runtime = services
            .config()
            .load_runtime()
            .await
            .expect("load runtime config");
        assert!(!runtime.privacy_filter_enabled);
        assert!(runtime.route_events_enabled);
        assert_eq!(runtime.raw["enable_logging"], json!(true));
    }

    #[tokio::test]
    async fn provider_source_projects_db_providers_through_adapter() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        let services = CcSwitchProxyServices::new(db);

        let providers = services
            .providers()
            .list_providers(&AppKind::Claude)
            .await
            .expect("list providers");

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "anthropic-main");
        assert_eq!(providers[0].name, "Anthropic Main");
        assert_eq!(providers[0].kind, ProviderKind::Claude);
        assert_eq!(providers[0].account_ref, None);
        assert!(providers[0].metadata.raw.get("env").is_none());

        let provider = services
            .providers()
            .get_provider(&AppKind::Claude, "anthropic-main")
            .await
            .expect("get provider")
            .expect("provider");

        assert_eq!(provider.id, "anthropic-main");
        assert_eq!(provider.kind, ProviderKind::Claude);
        assert!(provider.metadata.raw.get("env").is_none());
    }

    #[tokio::test]
    async fn route_policy_source_projects_failover_queue_through_core() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        db.add_to_failover_queue("claude", "anthropic-main")
            .expect("add failover provider");
        let services = CcSwitchProxyServices::new(db);

        let policy = services
            .route_policies()
            .load_policy(&AppKind::Claude)
            .await
            .expect("load policy")
            .expect("policy");

        assert_eq!(policy.app, AppKind::Claude);
        assert!(policy.groups.is_empty());
        assert_eq!(policy.raw["defaultGroup"], json!(DEFAULT_ROUTE_GROUP));
        assert_eq!(policy.raw["failoverProviderIds"], json!(["anthropic-main"]));
    }

    #[tokio::test]
    async fn auth_provider_projects_profile_ref_through_core() {
        let provider = CcSwitchAuthProvider;
        let request = proxy_request();
        let mut plan = route_plan("anthropic-main", "channel-auth");
        plan.selection.channel.auth_profile =
            Some(AuthProfileRef::new("provider:claude:anthropic-main"));

        let auth = provider
            .resolve_auth(
                &request.app,
                &plan.selection.provider,
                &plan.selection.channel,
                &request,
            )
            .await
            .expect("resolve auth");

        assert!(auth.headers.is_empty());
        assert_eq!(
            auth.account_ref.as_deref(),
            Some("provider:claude:anthropic-main")
        );
        assert_eq!(auth.metadata["source"], json!("cc_switch_provider_config"));
        assert_eq!(auth.metadata["app"], json!("claude"));
        assert_eq!(auth.metadata["providerId"], json!("anthropic-main"));
        assert_eq!(auth.metadata["channelId"], json!("channel-auth"));
    }

    #[tokio::test]
    async fn channel_source_projects_legacy_provider_channels_through_ports() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        let services = CcSwitchProxyServices::new(db);

        let channels = services
            .channels()
            .list_channels(ChannelQuery {
                app: &AppKind::Claude,
                provider_id: Some("anthropic-main"),
                model: Some("claude-sonnet-4"),
                group: Some(DEFAULT_ROUTE_GROUP),
                include_disabled: false,
                allow_legacy_projection: true,
            })
            .await
            .expect("list channels");

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].provider_id, "anthropic-main");
        assert_eq!(
            channels[0].endpoint.base_url,
            "https://relay-a.example.com/v1"
        );
        assert_eq!(channels[0].models[0].public_model, "claude-sonnet-4");
    }

    #[tokio::test]
    async fn route_resolver_selects_highest_priority_matching_channel() {
        let services = CcSwitchProxyServices::new(Arc::new(Database::memory().expect("memory db")));
        let providers = vec![provider_spec("provider-a")];
        let channels = vec![
            channel_spec("low", 1, "sonnet"),
            channel_spec("high", 10, "sonnet"),
        ];
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({})),
        );
        request.requested_model = Some("sonnet".to_string());

        let plan = services
            .route_resolver()
            .resolve(RouteRequest {
                request: &request,
                providers: &providers,
                channels: &channels,
                policy: None,
            })
            .await
            .expect("resolve route");

        assert_eq!(plan.selection.channel.id, "high");
        assert_eq!(
            plan.selection
                .model_route
                .as_ref()
                .map(|route| route.upstream_model.as_str()),
            Some("upstream-sonnet")
        );
        assert_eq!(plan.attempts.len(), 2);
        assert_eq!(plan.selections.len(), 2);
    }

    #[test]
    fn model_catalog_from_raw_extracts_supported_client_model_ids() {
        let catalog = client_model_catalog_from_optional_raw(
            AppKind::Codex.as_str(),
            Some(json!({
                "models": [
                    {"id": " gpt-5 "},
                    {"model": "o4-mini"},
                    {"name": "gemini-2.5-pro"},
                    "claude-sonnet-4",
                    {"id": "gpt-5"}
                ]
            })),
        );

        assert_eq!(
            catalog.models,
            vec![
                "claude-sonnet-4".to_string(),
                "gemini-2.5-pro".to_string(),
                "gpt-5".to_string(),
                "o4-mini".to_string(),
            ]
        );
        assert_eq!(catalog.provider_id, "codex");
    }

    #[tokio::test]
    async fn model_catalog_provider_loads_provider_catalog_from_settings() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        let services = CcSwitchProxyServices::new(db);

        let catalog = services
            .model_catalog()
            .load_catalog(&AppKind::Claude, "anthropic-main")
            .await
            .expect("load provider catalog");

        assert_eq!(catalog.provider_id, "anthropic-main");
        assert_eq!(catalog.models, vec!["claude-sonnet-4".to_string()]);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn model_catalog_provider_loads_codex_client_catalog_file() {
        let _home = IsolatedTestHome::new();
        let codex_dir = crate::codex_config::get_codex_config_dir();
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            crate::codex_config::get_codex_config_path(),
            "model_catalog_json = \"cc-switch-model-catalog.json\"\n",
        )
        .expect("write codex config");
        std::fs::write(
            crate::codex_config::get_codex_model_catalog_path(),
            r#"{"models":[{"id":"gpt-5"},{"model":"o4-mini"}]}"#,
        )
        .expect("write codex model catalog");
        let services = CcSwitchProxyServices::new(Arc::new(Database::memory().expect("memory db")));

        let catalog = services
            .model_catalog()
            .load_client_catalog(&AppKind::Codex)
            .await
            .expect("load client catalog");

        assert_eq!(catalog.provider_id, "codex");
        assert_eq!(
            catalog.models,
            vec!["gpt-5".to_string(), "o4-mini".to_string()]
        );
        assert_eq!(
            catalog.raw,
            json!({"models": [{"id": "gpt-5"}, {"model": "o4-mini"}]})
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn model_catalog_provider_ignores_user_owned_codex_catalog_file() {
        let _home = IsolatedTestHome::new();
        let codex_dir = crate::codex_config::get_codex_config_dir();
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            crate::codex_config::get_codex_config_path(),
            "model_catalog_json = \"my-custom-catalog.json\"\n",
        )
        .expect("write codex config");
        std::fs::write(
            codex_dir.join("my-custom-catalog.json"),
            r#"{"models":[{"id":"user-model"}]}"#,
        )
        .expect("write user catalog");
        let services = CcSwitchProxyServices::new(Arc::new(Database::memory().expect("memory db")));

        let catalog = services
            .model_catalog()
            .load_client_catalog(&AppKind::Codex)
            .await
            .expect("load client catalog");

        assert_eq!(catalog.models, Vec::<String>::new());
        assert_eq!(catalog.raw, json!({"models": []}));
    }

    #[tokio::test]
    async fn model_catalog_provider_uses_core_empty_client_catalog_default() {
        let services = CcSwitchProxyServices::new(Arc::new(Database::memory().expect("memory db")));

        let catalog = services
            .model_catalog()
            .load_client_catalog(&AppKind::Gemini)
            .await
            .expect("load client catalog");

        assert_eq!(catalog.provider_id, "gemini");
        assert_eq!(catalog.models, Vec::<String>::new());
        assert_eq!(catalog.raw, json!({"models": []}));
    }

    #[tokio::test]
    async fn health_store_records_channel_attempts_in_database() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        db.materialize_legacy_proxy_channels("claude")
            .expect("materialize channels");
        let channel_id = db
            .list_proxy_channels_for_app("claude")
            .expect("list channels")
            .first()
            .expect("channel")
            .id
            .clone();
        let services = CcSwitchProxyServices::new(db.clone());

        services
            .health_store()
            .record_attempt(ChannelAttemptResult {
                channel_id: channel_id.clone(),
                success: false,
                status_code: Some(StatusCode::TOO_MANY_REQUESTS.as_u16()),
                latency_ms: Some(123),
                failure_threshold: None,
                error_code: Some("rate_limited".to_string()),
            })
            .await
            .expect("record attempt");

        let health = db
            .get_proxy_channel_health(&channel_id)
            .expect("read channel health");
        assert_eq!(health.status, "degraded");
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.response_time_ms, Some(123));
    }

    #[tokio::test]
    async fn health_store_resets_channel_health_through_router() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        db.materialize_legacy_proxy_channels("claude")
            .expect("materialize channels");
        let channel_id = db
            .list_proxy_channels_for_app("claude")
            .expect("list channels")
            .first()
            .expect("channel")
            .id
            .clone();
        db.update_proxy_channel_health_with_threshold(
            &channel_id,
            false,
            Some("rate_limited".to_string()),
            1,
            Some(99),
        )
        .expect("mark unhealthy");
        let services = CcSwitchProxyServices::new(db.clone());

        let reset = services
            .health_store()
            .reset_channel(&channel_id)
            .await
            .expect("reset channel health");

        assert_eq!(reset.channel_id, channel_id);
        assert_eq!(reset.app, AppKind::Claude);
        let health = db
            .get_proxy_channel_health(&reset.channel_id)
            .expect("read channel health");
        assert_eq!(health.status, "unknown");
    }

    #[tokio::test]
    async fn event_sink_bridges_core_events_to_proxy_event_bus() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let events = Arc::new(ProxyEventBus::default());
        let mut subscriber = events.subscribe();
        let services = CcSwitchProxyServices::with_event_bus(db, events);

        services
            .event_sink()
            .emit_event(ProxyCoreEvent {
                event_type: ProxyCoreEventType::RouteSelected,
                request_id: Some("req-1".to_string()),
                channel_id: Some("channel-a".to_string()),
                payload: json!({
                    "attemptCount": 2,
                }),
            })
            .await
            .expect("emit event");

        let event = subscriber.recv().await.expect("receive event");
        assert_eq!(event.event, "route_selected");
        assert_eq!(event.payload["requestId"], "req-1");
        assert_eq!(event.payload["channelId"], "channel-a");
        assert_eq!(event.payload["attemptCount"], 2);
    }

    #[tokio::test]
    async fn forward_pipeline_without_runtime_reports_unsupported() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let services = CcSwitchProxyServices::new(db);

        let err = services
            .forward_pipeline()
            .forward(proxy_request(), route_plan("provider-a", "channel-a"))
            .await
            .expect_err("plain services do not own server runtime");

        assert!(matches!(err, ProxyCoreError::Unsupported(_)));
    }

    #[tokio::test]
    async fn runtime_forward_pipeline_requires_matching_host_provider() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let services = CcSwitchProxyServices::with_runtime(runtime(db));

        let err = services
            .forward_pipeline()
            .forward(proxy_request(), route_plan("missing-provider", "channel-a"))
            .await
            .expect_err("missing provider should stop before forwarding");

        assert!(matches!(err, ProxyCoreError::Unavailable(_)));
        assert!(err
            .to_string()
            .contains("route plan providers are not configured"));
    }

    #[test]
    fn proxy_response_bridge_preserves_buffered_body() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        let response = ProxyResponse::buffered(
            StatusCode::CREATED,
            headers,
            Bytes::from_static(br#"{"ok":true}"#),
        );

        let core_response = proxy_response_to_core_response(response, Option::<()>::None);

        assert_eq!(core_response.status, StatusCode::CREATED);
        assert_eq!(
            core_response
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        match core_response.body {
            ProxyResponseBody::Bytes(body) => {
                assert_eq!(body, Bytes::from_static(br#"{"ok":true}"#))
            }
            other => panic!("expected bytes body, got {other:?}"),
        }
    }

    #[test]
    fn forward_result_bridge_projects_metadata_and_successful_channel() {
        let primary = route_plan("provider-a", "channel-a").selection;
        let fallback = route_plan("provider-a", "channel-b").selection;
        let plan = RoutePlan {
            selection: primary.clone(),
            selections: vec![primary, fallback],
            attempts: Vec::new(),
        };
        let result = crate::proxy::ForwardResult {
            response: ProxyResponse::buffered(
                StatusCode::OK,
                http::HeaderMap::new(),
                Bytes::from_static(b"{}"),
            ),
            provider: Provider::with_id(
                "provider-a".to_string(),
                "Provider A".to_string(),
                json!({}),
                None,
            ),
            claude_api_format: Some("messages".to_string()),
            outbound_model: Some("upstream-sonnet".to_string()),
            selected_channel: Some(ResolvedChannelAttempt {
                channel_id: "channel-b".to_string(),
                channel_name: "Channel B".to_string(),
                base_url: "https://fallback.example.com/v1".to_string(),
                interface_kind: "openai_responses".to_string(),
                auth_profile_ref: None,
                public_model: Some("sonnet".to_string()),
                upstream_model: Some("upstream-sonnet".to_string()),
                header_overrides: json!({}),
                param_overrides: json!({}),
                status_code_mapping: json!([]),
            }),
            connection_guard: None,
        };

        let proxy_result = forward_result_to_proxy_result(result, plan);

        assert_eq!(proxy_result.selected_route.channel.id, "channel-b");
        assert_eq!(
            proxy_result.outbound_model.as_deref(),
            Some("upstream-sonnet")
        );
        assert_eq!(
            proxy_result
                .metadata
                .get("hostProviderId")
                .and_then(Value::as_str),
            Some("provider-a")
        );
        assert_eq!(
            proxy_result
                .metadata
                .get("hostProviderName")
                .and_then(Value::as_str),
            Some("Provider A")
        );
        assert_eq!(
            proxy_result
                .metadata
                .get("claudeApiFormat")
                .and_then(Value::as_str),
            Some("messages")
        );
        assert_eq!(
            proxy_result
                .metadata
                .get("selectedChannelId")
                .and_then(Value::as_str),
            Some("channel-b")
        );
    }

    #[tokio::test]
    async fn proxy_response_bridge_wraps_streamed_body() {
        let response = ProxyResponse::streamed(
            StatusCode::OK,
            http::HeaderMap::new(),
            futures::stream::once(async { Ok(Bytes::from_static(b"chunk")) }),
        );

        let core_response = proxy_response_to_core_response(response, Option::<()>::None);

        match core_response.body {
            ProxyResponseBody::Stream(mut stream) => {
                let chunk = stream.next().await.expect("chunk").expect("stream item");
                assert_eq!(chunk, Bytes::from_static(b"chunk"));
                assert!(stream.next().await.is_none());
            }
            other => panic!("expected stream body, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn usage_sink_records_complete_usage_records() -> Result<(), AppError> {
        let db = Arc::new(Database::memory().expect("memory db"));
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO model_pricing (
                    model_id,
                    display_name,
                    input_cost_per_million,
                    output_cost_per_million,
                    cache_read_cost_per_million,
                    cache_creation_cost_per_million
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "upstream-sonnet",
                    "Upstream Sonnet",
                    "3.0",
                    "15.0",
                    "0.3",
                    "3.75"
                ],
            )
            .expect("insert pricing");
        }
        let services = CcSwitchProxyServices::new(db.clone());

        services
            .usage_sink()
            .record_usage(UsageRecord {
                request_id: Some("req-usage-1".to_string()),
                message_id: Some("msg-usage-1".to_string()),
                app: AppKind::Claude,
                provider_id: "provider-a".to_string(),
                provider_kind: Some(ProviderKind::Claude),
                channel_id: Some("channel-a".to_string()),
                channel_name: Some("Channel A".to_string()),
                route_group: Some(DEFAULT_ROUTE_GROUP.to_string()),
                request_model: "public-sonnet".to_string(),
                outbound_model: "upstream-sonnet".to_string(),
                response_model: Some("upstream-sonnet".to_string()),
                pricing_model: None,
                tokens: UsageTokens {
                    input_tokens: 1_000,
                    output_tokens: 500,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                },
                latency_ms: 42,
                first_token_ms: Some(7),
                status_code: 200,
                error_message: None,
                session_id: Some("session-a".to_string()),
                is_streaming: true,
                metadata: json!({}),
            })
            .await
            .expect("record usage");

        type UsageRow = (
            String,
            String,
            String,
            String,
            String,
            i64,
            i64,
            i64,
            Option<i64>,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            String,
        );

        let conn = crate::database::lock_conn!(db.conn);
        let row: UsageRow = conn
            .query_row(
                "SELECT
                    provider_id,
                    app_type,
                    model,
                    request_model,
                    pricing_model,
                    input_tokens,
                    output_tokens,
                    latency_ms,
                    first_token_ms,
                    status_code,
                    session_id,
                    provider_type,
                    channel_id,
                    channel_name,
                    route_group,
                    is_streaming,
                    total_cost_usd
                 FROM proxy_request_logs
                 WHERE request_id = 'req-usage-1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                        row.get(14)?,
                        row.get(15)?,
                        row.get(16)?,
                    ))
                },
            )
            .expect("usage row");

        assert_eq!(row.0, "provider-a");
        assert_eq!(row.1, "claude");
        assert_eq!(row.2, "upstream-sonnet");
        assert_eq!(row.3, "public-sonnet");
        assert_eq!(row.4, "upstream-sonnet");
        assert_eq!(row.5, 1_000);
        assert_eq!(row.6, 500);
        assert_eq!(row.7, 42);
        assert_eq!(row.8, Some(7));
        assert_eq!(row.9, 200);
        assert_eq!(row.10.as_deref(), Some("session-a"));
        assert_eq!(row.11.as_deref(), Some("claude"));
        assert_eq!(row.12.as_deref(), Some("channel-a"));
        assert_eq!(row.13.as_deref(), Some("Channel A"));
        assert_eq!(row.14.as_deref(), Some(DEFAULT_ROUTE_GROUP));
        assert_eq!(row.15, 1);
        assert_ne!(row.16, "0");
        Ok(())
    }

    #[tokio::test]
    async fn proxy_engine_plans_routes_through_cc_switch_services() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        let services = Arc::new(CcSwitchProxyServices::new(db));
        let engine = ProxyEngine::new(services);
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({})),
        );
        request.requested_model = Some("claude-sonnet-4".to_string());

        let plan = engine.plan_route(&request).await.expect("plan route");

        assert_eq!(plan.selection.provider.id, "anthropic-main");
        assert_eq!(plan.selection.channel.provider_id, "anthropic-main");
        assert_eq!(
            plan.selection
                .model_route
                .as_ref()
                .map(|route| route.upstream_model.as_str()),
            Some("claude-sonnet-4")
        );
    }

    #[tokio::test]
    async fn proxy_engine_materialized_plan_does_not_fallback_to_legacy_projection() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        let services = Arc::new(CcSwitchProxyServices::new(db.clone()));
        let engine = ProxyEngine::new(services);
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({})),
        );
        request.requested_model = Some("claude-sonnet-4".to_string());

        let err = engine
            .plan_materialized_route(&request)
            .await
            .expect_err("empty channel table should not fallback to legacy projection");
        assert!(matches!(err, ProxyCoreError::Unavailable(_)));

        db.materialize_legacy_proxy_channels("claude")
            .expect("materialize channels");
        let plan = engine
            .plan_materialized_route(&request)
            .await
            .expect("materialized plan");
        assert_eq!(plan.selection.channel.provider_id, "anthropic-main");
    }

    #[tokio::test]
    async fn route_dry_run_matches_proxy_engine_materialized_plan_order() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        create_materialized_channel(&db, "channel-low", 10, 100, "upstream-low");
        create_materialized_channel(&db, "channel-high", 100, 20, "upstream-high");

        let router = provider_router_from_database(db.clone());
        let dry_run = management_route_response_from_router_source(
            &router,
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("sonnet-public".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
        )
        .await
        .expect("dry-run route");

        let services = Arc::new(CcSwitchProxyServices::new(db));
        let engine = ProxyEngine::new(services);
        let mut request = ProxyRequest::new(
            AppKind::Claude,
            Method::POST,
            "/v1/messages",
            InterfaceKind::AnthropicMessages,
            ProxyBody::Json(json!({ "model": "sonnet-public", "messages": [] })),
        );
        request.requested_model = Some("sonnet-public".to_string());

        let plan = engine
            .plan_materialized_route(&request)
            .await
            .expect("materialized plan");
        let plan_channel_ids = plan
            .selections
            .iter()
            .map(|selection| selection.channel.id.as_str())
            .collect::<Vec<_>>();
        let dry_run_channel_ids = dry_run
            .candidates
            .iter()
            .map(|candidate| candidate.channel_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(dry_run_channel_ids, vec!["channel-high", "channel-low"]);
        assert_eq!(plan_channel_ids, dry_run_channel_ids);
        assert_eq!(plan.selection.channel.id, "channel-high");
        assert_eq!(
            plan.selection
                .model_route
                .as_ref()
                .map(|route| route.upstream_model.as_str()),
            Some("upstream-high")
        );
    }
}
