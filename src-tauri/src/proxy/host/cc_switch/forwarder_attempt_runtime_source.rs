use futures::future::BoxFuture;
use std::sync::Arc;

use crate::database::Database;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy::error::ProxyError;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy_core::api::transport::{
    forwarder_attempt_runtime_decision, ForwarderAttemptRuntimeDecisionInput,
};
use crate::proxy_core_adapter::{
    allow_forward_attempt_runtime_source, record_forward_attempt_failure_runtime_source,
    record_forward_attempt_success_runtime_source,
    release_forward_attempt_permit_neutral_runtime_source, ForwarderAttemptAllowDecision,
    ForwarderAttemptAllowInput, ForwarderAttemptRuntimeSource, ForwarderAttemptRuntimeSourceRef,
};

struct CcSwitchForwarderAttemptRuntimeSource {
    router: Arc<ProviderRouter>,
    db: Arc<Database>,
}

impl CcSwitchForwarderAttemptRuntimeSource {
    fn new(router: Arc<ProviderRouter>, db: Arc<Database>) -> Self {
        Self { router, db }
    }
}

fn record_selected_channel_key_failure(db: &Database, attempt: &ForwardAttempt, failed_at_ms: i64) {
    let Some(channel) = attempt.channel() else {
        return;
    };
    let Some(key_ref) = attempt.channel_auth_key_ref() else {
        return;
    };

    match db.record_proxy_channel_key_failure(&channel.channel_id, key_ref, failed_at_ms) {
        Ok(true) => {}
        Ok(false) => {
            log::warn!(
                "[ChannelKey] selected channel auth key not found while recording failure: channel_id={}, key_ref={key_ref}",
                channel.channel_id
            );
        }
        Err(error) => {
            log::warn!(
                "[ChannelKey] failed to record selected channel auth key failure: channel_id={}, key_ref={key_ref}, error={error}",
                channel.channel_id
            );
        }
    }
}

impl ForwarderAttemptRuntimeSource for CcSwitchForwarderAttemptRuntimeSource {
    fn allow<'a>(
        &'a self,
        input: ForwarderAttemptAllowInput<'a>,
    ) -> BoxFuture<'a, ForwarderAttemptAllowDecision> {
        Box::pin(async move {
            let runtime_decision =
                forwarder_attempt_runtime_decision(ForwarderAttemptRuntimeDecisionInput {
                    app_type: input.app_type,
                    attempted_providers: input.attempted_providers,
                    max_attempts: input.max_attempts,
                    attempts_len: input.attempts.len(),
                    single_attempt_is_channel: input
                        .attempts
                        .first()
                        .is_some_and(ForwardAttempt::is_channel),
                });
            if let Some(log_line) = runtime_decision.limit_log_line {
                log::warn!("{log_line}");
                return ForwarderAttemptAllowDecision::Stop;
            }

            let permit = allow_forward_attempt_runtime_source(
                self.router.as_ref(),
                input.attempt,
                input.app_type,
                runtime_decision.bypass_circuit_breaker,
            )
            .await;
            if permit.allowed {
                ForwarderAttemptAllowDecision::Allowed {
                    used_half_open_permit: permit.used_half_open_permit,
                }
            } else {
                ForwarderAttemptAllowDecision::Skipped
            }
        })
    }

    fn record_success<'a>(
        &'a self,
        attempt: &'a ForwardAttempt,
        app_type: &'a str,
        used_half_open_permit: bool,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_attempt_success_runtime_source(
                &self.router,
                attempt,
                app_type,
                used_half_open_permit,
            )
            .await;
        })
    }

    fn record_failure<'a>(
        &'a self,
        attempt: &'a ForwardAttempt,
        app_type: &'a str,
        used_half_open_permit: bool,
        error: &'a ProxyError,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_attempt_failure_runtime_source(
                self.router.as_ref(),
                attempt,
                app_type,
                used_half_open_permit,
                error,
            )
            .await;
            record_selected_channel_key_failure(
                self.db.as_ref(),
                attempt,
                chrono::Utc::now().timestamp_millis(),
            );
        })
    }

    fn release_attempt_permit_neutral<'a>(
        &'a self,
        attempt: &'a ForwardAttempt,
        app_type: &'a str,
        used_half_open_permit: bool,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            release_forward_attempt_permit_neutral_runtime_source(
                self.router.as_ref(),
                attempt,
                app_type,
                used_half_open_permit,
            )
            .await;
        })
    }
}

pub(crate) fn forwarder_attempt_runtime_source_from_runtime_sources(
    router: Arc<ProviderRouter>,
    db: Arc<Database>,
) -> ForwarderAttemptRuntimeSourceRef {
    Arc::new(CcSwitchForwarderAttemptRuntimeSource::new(router, db))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppType;
    use crate::provider::Provider;
    use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
    use crate::proxy_core::api::ports::ChannelKeyRuntimeSource;
    use crate::proxy_core_adapter::{
        ChannelRouteCandidate, ProxyChannelKeyWriteRequest, ProxyChannelWriteRequest,
    };
    use serde_json::json;

    fn candidate(channel_id: &str) -> ChannelRouteCandidate {
        ChannelRouteCandidate {
            channel_id: channel_id.to_string(),
            provider_id: "provider-a".to_string(),
            channel_name: "Relay".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            public_model: Some("sonnet-public".to_string()),
            upstream_model: Some("upstream-sonnet".to_string()),
            route_group: "default".to_string(),
            priority: 100,
            weight: 1,
            source_kind: "manual".to_string(),
        }
    }

    #[tokio::test]
    async fn record_failure_updates_selected_channel_key_last_failure_at() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-key" } }),
            None,
        );
        db.save_provider("claude", &provider)
            .expect("save provider");
        db.create_proxy_channel(ProxyChannelWriteRequest {
            id: Some("runtime-key-failure".to_string()),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Runtime Key Failure".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            auth_profile_ref: Some("channel-key:primary".to_string()),
            ..Default::default()
        })
        .expect("create channel");
        db.upsert_proxy_channel_key(
            "runtime-key-failure",
            "primary",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-channel-primary".to_string(),
                status: "enabled".to_string(),
                priority: 10,
                weight: 100,
            },
        )
        .expect("upsert channel key");
        db.upsert_proxy_channel_key(
            "runtime-key-failure",
            "backup",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-channel-backup".to_string(),
                status: "enabled".to_string(),
                priority: 5,
                weight: 100,
            },
        )
        .expect("upsert backup channel key");

        let mut attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            candidate("runtime-key-failure"),
        );
        attempt.set_channel_auth_key_ref("primary");
        let source = CcSwitchForwarderAttemptRuntimeSource::new(
            Arc::new(provider_router_from_database(db.clone())),
            db.clone(),
        );
        let before = chrono::Utc::now().timestamp_millis();

        source
            .record_failure(
                &attempt,
                "claude",
                false,
                &ProxyError::ForwardFailed("upstream timeout".to_string()),
            )
            .await;

        let after = chrono::Utc::now().timestamp_millis();
        let stored_key = db
            .get_proxy_channel_key("runtime-key-failure", "primary")
            .expect("read channel key")
            .expect("channel key");
        let last_failure_at = stored_key
            .last_failure_at
            .expect("selected channel key failure timestamp");
        assert!(
            (before..=after).contains(&last_failure_at),
            "last_failure_at={last_failure_at} should be between {before} and {after}"
        );
        let backup_key = db
            .get_proxy_channel_key("runtime-key-failure", "backup")
            .expect("read backup channel key")
            .expect("backup channel key");
        assert_eq!(backup_key.last_failure_at, None);
    }

    #[tokio::test]
    async fn record_failure_does_not_update_channel_keys_without_selected_key_ref() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-key" } }),
            None,
        );
        db.save_provider("claude", &provider)
            .expect("save provider");
        db.create_proxy_channel(ProxyChannelWriteRequest {
            id: Some("runtime-key-neutral".to_string()),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Runtime Key Neutral".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            ..Default::default()
        })
        .expect("create channel");
        db.upsert_proxy_channel_key(
            "runtime-key-neutral",
            "primary",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-channel-primary".to_string(),
                status: "enabled".to_string(),
                priority: 10,
                weight: 100,
            },
        )
        .expect("upsert channel key");

        let source = CcSwitchForwarderAttemptRuntimeSource::new(
            Arc::new(provider_router_from_database(db.clone())),
            db.clone(),
        );
        let plain_channel_attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            candidate("runtime-key-neutral"),
        );
        source
            .record_failure(
                &plain_channel_attempt,
                "claude",
                false,
                &ProxyError::ForwardFailed("plain channel failed".to_string()),
            )
            .await;
        let provider_attempt = ForwardAttempt::from_provider(provider);
        source
            .record_failure(
                &provider_attempt,
                "claude",
                false,
                &ProxyError::ForwardFailed("provider failed".to_string()),
            )
            .await;

        let stored_key = db
            .get_proxy_channel_key("runtime-key-neutral", "primary")
            .expect("read channel key")
            .expect("channel key");
        assert_eq!(stored_key.last_failure_at, None);
    }

    #[tokio::test]
    async fn selected_wildcard_key_failure_changes_next_runtime_selection() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-key" } }),
            None,
        );
        db.save_provider("claude", &provider)
            .expect("save provider");
        db.create_proxy_channel(ProxyChannelWriteRequest {
            id: Some("runtime-key-wildcard".to_string()),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Runtime Key Wildcard".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            auth_profile_ref: Some("channel-key:*".to_string()),
            ..Default::default()
        })
        .expect("create channel");
        for (key_ref, key_value, priority) in [
            ("primary", "sk-channel-primary", 10),
            ("backup", "sk-channel-backup", 100),
        ] {
            db.upsert_proxy_channel_key(
                "runtime-key-wildcard",
                key_ref,
                ProxyChannelKeyWriteRequest {
                    key_value: key_value.to_string(),
                    status: "enabled".to_string(),
                    priority,
                    weight: 100,
                },
            )
            .expect("upsert channel key");
        }

        let key_source =
            crate::proxy::host::cc_switch::channel_key_runtime_source::channel_key_runtime_source_from_database(
                db.clone(),
            );
        let selected = key_source
            .load_channel_key_candidate("runtime-key-wildcard", "*")
            .expect("load wildcard key")
            .expect("selected wildcard key");
        assert_eq!(selected.key_ref, "backup");

        let mut attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &provider,
            candidate("runtime-key-wildcard"),
        );
        attempt.set_channel_auth_key_ref(selected.key_ref);
        let source = CcSwitchForwarderAttemptRuntimeSource::new(
            Arc::new(provider_router_from_database(db.clone())),
            db.clone(),
        );
        source
            .record_failure(
                &attempt,
                "claude",
                false,
                &ProxyError::ForwardFailed("wildcard key failed".to_string()),
            )
            .await;

        let fallback = key_source
            .load_channel_key_candidate("runtime-key-wildcard", "*")
            .expect("load fallback wildcard key")
            .expect("selected fallback key");
        assert_eq!(fallback.key_ref, "primary");
    }
}
