use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::host::cc_switch::auth_provider::provider_with_channel_auth_key;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy_core::api::auth::channel_auth_profile_missing_key_error;
use crate::proxy_core::api::domain::{
    channel_auth_profile_provider_application, ChannelAuthProfileProviderApplication,
};
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::{ChannelKeyRuntimeLookupInput, ChannelKeyRuntimeSource};
use crate::proxy_core::api::routing::{
    route_plan_no_matching_host_providers_error, route_plan_provider_match,
    route_plan_providers_unconfigured_error, RoutePlan,
};
use indexmap::IndexMap;

pub(crate) fn host_providers_for_plan(
    providers: &IndexMap<String, Provider>,
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<Provider>> {
    let provider_match = route_plan_provider_match(plan, providers.keys().map(String::as_str));
    let has_matches = provider_match.has_matches();
    let matching: Vec<_> = provider_match
        .matched_provider_ids
        .iter()
        .filter_map(|provider_id| providers.get(provider_id.as_str()).cloned())
        .collect();
    if !has_matches {
        return Err(route_plan_providers_unconfigured_error());
    }
    Ok(matching)
}

pub(crate) fn forward_attempts_from_plan(
    app_type: &AppType,
    providers: &[Provider],
    plan: &RoutePlan,
) -> Vec<ForwardAttempt> {
    crate::proxy::route_attempt::forward_attempts_from_route_plan(app_type, providers, plan)
}

pub(crate) fn required_forward_attempts_from_plan(
    app_type: &AppType,
    providers: &[Provider],
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<ForwardAttempt>> {
    let attempts = forward_attempts_from_plan(app_type, providers, plan);
    if attempts.is_empty() {
        return Err(route_plan_no_matching_host_providers_error());
    }
    Ok(attempts)
}

pub(crate) fn apply_channel_auth_profile_providers_from_source(
    app_type: &AppType,
    providers: &IndexMap<String, Provider>,
    attempts: &mut [ForwardAttempt],
    channel_key_runtime_source: &(dyn ChannelKeyRuntimeSource + Send + Sync),
) -> ProxyCoreResult<()> {
    for attempt in attempts {
        let auth_profile_ref = attempt
            .channel()
            .and_then(|channel| channel.auth_profile_ref.as_ref())
            .map(String::as_str);
        let channel_id = attempt.channel().map(|channel| channel.channel_id.as_str());
        match channel_auth_profile_provider_application(
            app_type.as_str(),
            auth_profile_ref,
            channel_id,
            |provider_id| providers.contains_key(provider_id),
        ) {
            ChannelAuthProfileProviderApplication::UseProvider { provider_id } => {
                let provider = providers
                    .get(&provider_id)
                    .expect("provider availability was checked by proxy-core")
                    .clone();
                attempt.set_auth_provider(provider);
            }
            ChannelAuthProfileProviderApplication::MissingProvider { warning } => {
                log::warn!("{warning}");
                continue;
            }
            ChannelAuthProfileProviderApplication::UseChannelKey {
                channel_id,
                key_ref,
            } => {
                let Some(key_candidate) = channel_key_runtime_source.load_channel_key_candidate(
                    ChannelKeyRuntimeLookupInput {
                        channel_id: &channel_id,
                        key_ref: &key_ref,
                    },
                )?
                else {
                    return Err(channel_auth_profile_missing_key_error(
                        &channel_id,
                        &key_ref,
                    ));
                };
                attempt.set_channel_auth_provider(
                    provider_with_channel_auth_key(
                        app_type,
                        attempt.provider(),
                        &key_candidate.key_value,
                    ),
                    key_candidate.key_ref,
                );
            }
            ChannelAuthProfileProviderApplication::Ignore => {
                continue;
            }
        }
    }
    Ok(())
}

pub(crate) fn required_forward_attempts_from_sources(
    app_type: &AppType,
    all_providers: &IndexMap<String, Provider>,
    plan: &RoutePlan,
    channel_key_runtime_source: &(dyn ChannelKeyRuntimeSource + Send + Sync),
) -> ProxyCoreResult<Vec<ForwardAttempt>> {
    let providers = host_providers_for_plan(all_providers, plan)?;
    let mut attempts = required_forward_attempts_from_plan(app_type, &providers, plan)?;
    apply_channel_auth_profile_providers_from_source(
        app_type,
        all_providers,
        &mut attempts,
        channel_key_runtime_source,
    )?;
    Ok(attempts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::proxy::host::cc_switch::channel_key_runtime_source::channel_key_runtime_source_from_database;
    use crate::proxy_core::api::auth::channel_auth_profile_missing_key_error;
    use crate::proxy_core::api::domain::{
        channel_auth_profile_action, channel_auth_profile_missing_provider_warning, AppKind,
        AuthProfileRef, ChannelAuthProfileAction, ChannelHealthPolicy, ChannelOverrides,
        ProviderKind, ProviderMetadata, ProviderSpec, RetryPolicy, UpstreamEndpoint,
    };
    use crate::proxy_core::api::errors::{ProxyCoreError, ProxyCoreResult};
    use crate::proxy_core::api::management::{
        ChannelKeyRuntimeCandidate, ProxyChannelKeyWriteRequest, ProxyChannelWriteRequest,
    };
    use crate::proxy_core::api::ports::{ChannelKeyRuntimeLookupInput, ChannelKeyRuntimeSource};
    use crate::proxy_core::api::routing::{
        route_selection_from_parts, ChannelSpec, ChannelStatus, InterfaceKind, RoutePlan,
        RouteSelection,
    };
    use serde_json::{json, Value};
    use std::sync::Arc;

    fn route_selection_with_auth_ref(
        route_provider: &Provider,
        channel_id: &str,
        auth_profile_ref: Option<&str>,
    ) -> RouteSelection {
        let provider = ProviderSpec {
            id: route_provider.id.clone(),
            name: route_provider.name.clone(),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        };
        let channel = ChannelSpec {
            id: channel_id.to_string(),
            provider_id: route_provider.id.clone(),
            app: AppKind::Claude,
            name: channel_id.to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: format!("https://{channel_id}.example.com/v1"),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::AnthropicMessages,
            auth_profile: auth_profile_ref.map(AuthProfileRef::new),
            models: Vec::new(),
            groups: vec!["default".to_string()],
            priority: 0,
            weight: 100,
            retry_policy: RetryPolicy {
                raw: Value::Object(Default::default()),
            },
            health_policy: ChannelHealthPolicy {
                raw: Value::Object(Default::default()),
            },
            overrides: ChannelOverrides {
                headers: Value::Object(Default::default()),
                params: Value::Object(Default::default()),
                status_code_mapping: Value::Array(Vec::new()),
                model_mapping: Value::Object(Default::default()),
            },
            tags: Vec::new(),
            metadata: Value::Object(Default::default()),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        };
        route_selection_from_parts(provider, channel, None, InterfaceKind::AnthropicMessages)
    }

    fn attempt_with_auth_ref(
        route_provider: &Provider,
        channel_id: &str,
        auth_profile_ref: &str,
    ) -> ForwardAttempt {
        let selection =
            route_selection_with_auth_ref(route_provider, channel_id, Some(auth_profile_ref));
        ForwardAttempt::from_core_selection(&AppType::Claude, route_provider, &selection)
    }

    fn route_plan(
        route_provider: &Provider,
        channel_id: &str,
        auth_profile_ref: &str,
    ) -> RoutePlan {
        let selection =
            route_selection_with_auth_ref(route_provider, channel_id, Some(auth_profile_ref));
        RoutePlan {
            selection,
            selections: Vec::new(),
            attempts: Vec::new(),
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

    fn create_channel_with_auth_profile(db: &Database, channel_id: &str, auth_profile_ref: &str) {
        db.create_proxy_channel(ProxyChannelWriteRequest {
            id: Some(channel_id.to_string()),
            provider_id: "anthropic-main".to_string(),
            app_type: "claude".to_string(),
            name: channel_id.to_string(),
            base_url: format!("https://{channel_id}.example.com/v1"),
            interface_kind: "anthropic_messages".to_string(),
            auth_profile_ref: Some(auth_profile_ref.to_string()),
            ..ProxyChannelWriteRequest::default()
        })
        .expect("create channel");
    }

    fn apply_channel_auth_profile_providers_with_runtime_source(
        db: &Arc<Database>,
        providers: &IndexMap<String, Provider>,
        attempts: &mut [ForwardAttempt],
    ) -> ProxyCoreResult<()> {
        let channel_key_runtime_source = channel_key_runtime_source_from_database(db.clone());
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            providers,
            attempts,
            &channel_key_runtime_source,
        )
    }

    struct TestChannelKeyRuntimeSource {
        expected: Option<(&'static str, &'static str)>,
        candidate: Option<ChannelKeyRuntimeCandidate>,
    }

    impl ChannelKeyRuntimeSource for TestChannelKeyRuntimeSource {
        fn load_channel_key_candidate(
            &self,
            input: ChannelKeyRuntimeLookupInput<'_>,
        ) -> ProxyCoreResult<Option<ChannelKeyRuntimeCandidate>> {
            let Some((expected_channel_id, expected_key_ref)) = self.expected else {
                panic!("provider auth should not load channel keys");
            };
            assert_eq!(input.channel_id, expected_channel_id);
            assert_eq!(input.key_ref, expected_key_ref);
            Ok(self.candidate.clone())
        }
    }

    #[test]
    fn channel_auth_profile_rules_project_expected_warnings_and_errors() {
        assert_eq!(
            channel_auth_profile_missing_provider_warning(
                "claude",
                Some("provider:claude:missing"),
            ),
            "[claude] channel auth profile references missing provider: provider:claude:missing"
        );
        assert_eq!(
            channel_auth_profile_missing_provider_warning("claude", None),
            "[claude] channel auth profile references missing provider: "
        );
        assert!(matches!(
            channel_auth_profile_action(
                "claude",
                Some("provider:claude:provider-a"),
                Some("channel-a")
            ),
            ChannelAuthProfileAction::Provider {
                provider_id,
                missing_provider_warning,
            } if provider_id == "provider-a"
                && missing_provider_warning.contains("provider:claude:provider-a")
        ));
        assert!(matches!(
            channel_auth_profile_action("claude", Some("channel-key:primary"), Some("channel-a")),
            ChannelAuthProfileAction::ChannelKey { channel_id, key_ref }
                if channel_id == "channel-a" && key_ref == "primary"
        ));
        assert!(matches!(
            channel_auth_profile_action("claude", Some("channel-key:primary"), None),
            ChannelAuthProfileAction::Ignore
        ));
        assert!(matches!(
            channel_auth_profile_missing_key_error("channel-a", "primary"),
            ProxyCoreError::Auth(message)
                if message.contains("channel_id=channel-a")
                    && message.contains("key_ref=primary")
        ));
    }

    #[test]
    fn channel_auth_profile_source_applies_provider_and_channel_key_auth() {
        let provider = Provider::with_id(
            "route-provider".to_string(),
            "Route Provider".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "route-key" } }),
            None,
        );
        let auth_provider =
            provider_with_channel_auth_key(&AppType::Claude, &provider, "channel-key");
        assert_eq!(
            provider
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("route-key")
        );
        assert_eq!(
            auth_provider
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("channel-key")
        );

        let route_provider = provider.clone();
        let provider_auth = Provider::with_id(
            "provider-auth".to_string(),
            "Provider Auth".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-auth-key" } }),
            None,
        );
        let mut providers = IndexMap::new();
        providers.insert(route_provider.id.clone(), route_provider.clone());
        providers.insert(provider_auth.id.clone(), provider_auth);

        let mut provider_attempt = attempt_with_auth_ref(
            &route_provider,
            "channel-a",
            "provider:claude:provider-auth",
        );
        let provider_runtime_source = TestChannelKeyRuntimeSource {
            expected: None,
            candidate: None,
        };
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut provider_attempt),
            &provider_runtime_source,
        )
        .expect("apply provider auth profile");
        assert_eq!(provider_attempt.auth_provider().id, "provider-auth");
        assert_eq!(provider_attempt.provider().id, "route-provider");
        assert_eq!(
            provider_attempt
                .auth_provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("provider-auth-key")
        );

        let mut channel_key_attempt =
            attempt_with_auth_ref(&route_provider, "channel-key", "channel-key:primary");
        let channel_key_runtime_source = TestChannelKeyRuntimeSource {
            expected: Some(("channel-key", "primary")),
            candidate: Some(ChannelKeyRuntimeCandidate {
                channel_id: "channel-key".to_string(),
                key_ref: "primary".to_string(),
                key_value: "loaded-channel-key".to_string(),
                status: "enabled".to_string(),
                priority: 10,
                weight: 100,
                last_failure_at: Some(1_771_000_003),
            }),
        };
        apply_channel_auth_profile_providers_from_source(
            &AppType::Claude,
            &providers,
            std::slice::from_mut(&mut channel_key_attempt),
            &channel_key_runtime_source,
        )
        .expect("apply channel key auth profile");
        assert_eq!(
            channel_key_attempt
                .auth_provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("loaded-channel-key")
        );
    }

    #[test]
    fn channel_auth_profile_source_ignores_cross_app_and_spaced_provider_refs() {
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
        providers.insert(route_provider.id.clone(), route_provider.clone());
        providers.insert(auth_provider.id.clone(), auth_provider);
        let runtime_source = TestChannelKeyRuntimeSource {
            expected: None,
            candidate: None,
        };

        for auth_profile_ref in [
            "provider:codex:auth-provider",
            "provider:claude: auth-provider",
        ] {
            let mut attempt =
                attempt_with_auth_ref(&route_provider, "channel-auth", auth_profile_ref);
            apply_channel_auth_profile_providers_from_source(
                &AppType::Claude,
                &providers,
                std::slice::from_mut(&mut attempt),
                &runtime_source,
            )
            .expect("apply ignored provider auth profile");

            assert_eq!(attempt.provider().id, "route-provider");
            assert_eq!(attempt.auth_provider().id, "route-provider");
        }
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
        providers.insert(route_provider.id.clone(), route_provider.clone());
        let plan = route_plan(&route_provider, "channel-auth", "channel-key:manual");
        let route_providers = host_providers_for_plan(&providers, &plan).expect("route providers");
        let mut attempts = forward_attempts_from_plan(&AppType::Claude, &route_providers, &plan);
        let db = Arc::new(Database::memory().expect("memory db"));
        let error = apply_channel_auth_profile_providers_with_runtime_source(
            &db,
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
    fn db_channel_key_auth_profile_sets_auth_key_without_changing_route_provider() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        create_channel_with_auth_profile(&db, "channel-auth-key", "channel-key:primary");
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
        let route_provider = providers.get("anthropic-main").expect("route provider");
        let plan = route_plan(route_provider, "channel-auth-key", "channel-key:primary");
        let route_providers = host_providers_for_plan(&providers, &plan).expect("route providers");
        let mut attempts = forward_attempts_from_plan(&AppType::Claude, &route_providers, &plan);
        apply_channel_auth_profile_providers_with_runtime_source(&db, &providers, &mut attempts)
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
        assert_eq!(attempts[0].channel_auth_key_ref(), Some("primary"));

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
    fn db_channel_key_auth_profile_keeps_same_key_ref_scoped_per_channel() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);

        for (channel_id, key_value) in [
            ("channel-auth-a", "sk-channel-a"),
            ("channel-auth-b", "sk-channel-b"),
        ] {
            create_channel_with_auth_profile(&db, channel_id, "channel-key:primary");
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
        let route_provider = providers.get("anthropic-main").expect("route provider");
        let mut attempts = Vec::new();
        for channel_id in ["channel-auth-a", "channel-auth-b"] {
            let plan = route_plan(route_provider, channel_id, "channel-key:primary");
            let route_providers =
                host_providers_for_plan(&providers, &plan).expect("route providers");
            attempts.extend(forward_attempts_from_plan(
                &AppType::Claude,
                &route_providers,
                &plan,
            ));
        }

        apply_channel_auth_profile_providers_with_runtime_source(&db, &providers, &mut attempts)
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
        assert_eq!(
            attempts
                .iter()
                .map(|attempt| attempt.channel_auth_key_ref())
                .collect::<Vec<_>>(),
            vec![Some("primary"), Some("primary")]
        );
    }

    #[test]
    fn db_channel_key_wildcard_auth_profile_selects_best_enabled_key_for_channel() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        create_channel_with_auth_profile(&db, "channel-auth-wildcard", "channel-key:*");
        for (key_ref, key_value, status, priority, weight) in [
            ("disabled-best", "sk-disabled", "disabled", 200, 100),
            ("primary", "sk-primary", "enabled", 10, 100),
            ("backup", "sk-backup", "enabled", 100, 50),
        ] {
            db.upsert_proxy_channel_key(
                "channel-auth-wildcard",
                key_ref,
                ProxyChannelKeyWriteRequest {
                    key_value: key_value.to_string(),
                    status: status.to_string(),
                    priority,
                    weight,
                },
            )
            .expect("upsert wildcard channel key");
        }

        let providers = db.get_all_providers("claude").expect("load providers");
        let route_provider = providers.get("anthropic-main").expect("route provider");
        let plan = route_plan(route_provider, "channel-auth-wildcard", "channel-key:*");
        let route_providers = host_providers_for_plan(&providers, &plan).expect("route providers");
        let mut attempts = forward_attempts_from_plan(&AppType::Claude, &route_providers, &plan);

        apply_channel_auth_profile_providers_with_runtime_source(&db, &providers, &mut attempts)
            .expect("apply wildcard channel key auth profile");

        assert_eq!(
            attempts[0]
                .auth_provider()
                .settings_config
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("sk-backup")
        );
        assert_eq!(attempts[0].channel_auth_key_ref(), Some("backup"));
    }

    #[test]
    fn required_forward_attempts_from_plan_errors_without_matching_host_provider() {
        let route_provider = Provider::with_id(
            "route-provider".to_string(),
            "Route Provider".to_string(),
            json!({}),
            None,
        );
        let selection = route_selection_with_auth_ref(&route_provider, "channel-a", None);
        let plan = RoutePlan {
            selection: selection.clone(),
            selections: vec![selection],
            attempts: Vec::new(),
        };

        let error = required_forward_attempts_from_plan(&AppType::Claude, &[], &plan)
            .expect_err("missing host providers should fail");

        assert!(matches!(
            error,
            ProxyCoreError::Unavailable(message)
                if message == "route plan has no matching host providers"
        ));
    }
}
