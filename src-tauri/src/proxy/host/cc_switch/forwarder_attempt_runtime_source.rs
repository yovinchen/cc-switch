use futures::future::BoxFuture;
use std::sync::Arc;

use crate::database::Database;
use crate::proxy::engine::forward_pipeline::{
    ForwarderAttemptAllowDecision, ForwarderAttemptAllowInput, ForwarderAttemptFailureInput,
    ForwarderAttemptNeutralReleaseInput, ForwarderAttemptRuntimeSource,
    ForwarderAttemptRuntimeSourceRef, ForwarderAttemptSuccessInput,
};
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy::error::ProxyError;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy_core::api::config::AllowResult;
use crate::proxy_core::api::routing::effective_forward_max_attempts_for_channel;
use crate::proxy_core::api::transport::{
    forwarder_attempt_runtime_decision, ForwarderAttemptRuntimeDecisionInput,
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

async fn allow_forward_attempt_runtime_source(
    router: &ProviderRouter,
    attempt: &ForwardAttempt,
    app_type: &str,
    bypass_circuit_breaker: bool,
) -> AllowResult {
    if bypass_circuit_breaker {
        return AllowResult {
            allowed: true,
            used_half_open_permit: false,
        };
    }

    if let Some(channel) = attempt.channel() {
        router
            .allow_channel_request(&channel.channel_id, app_type)
            .await
    } else {
        router
            .allow_provider_request(&attempt.provider().id, app_type)
            .await
    }
}

async fn record_forward_attempt_success_runtime_source(
    router: &Arc<ProviderRouter>,
    attempt: &ForwardAttempt,
    app_type: &str,
    used_half_open_permit: bool,
) {
    if let Some(channel) = attempt.channel() {
        if used_half_open_permit {
            if let Err(error) = router
                .record_channel_result(&channel.channel_id, app_type, true, true, None, None)
                .await
            {
                log::warn!(
                    "[{app_type}] 记录 Channel 成功结果失败: channel_id={}, error={error}",
                    channel.channel_id
                );
            }
            return;
        }

        let router = router.clone();
        let channel_id = channel.channel_id.clone();
        let app_type = app_type.to_string();
        tokio::spawn(async move {
            if let Err(error) = router
                .record_channel_result(&channel_id, &app_type, false, true, None, None)
                .await
            {
                log::warn!(
                    "[{app_type}] 异步记录 Channel 成功结果失败: channel_id={channel_id}, error={error}"
                );
            }
        });
        return;
    }

    let provider_id = attempt.provider().id.clone();
    if used_half_open_permit {
        if let Err(error) = router
            .record_result(&provider_id, app_type, true, true, None)
            .await
        {
            log::warn!(
                "[{app_type}] 记录 Provider 成功结果失败: provider_id={provider_id}, error={error}"
            );
        }
        return;
    }

    let router = router.clone();
    let app_type = app_type.to_string();
    tokio::spawn(async move {
        if let Err(error) = router
            .record_result(&provider_id, &app_type, false, true, None)
            .await
        {
            log::warn!(
                "[{app_type}] 异步记录 Provider 成功结果失败: provider_id={provider_id}, error={error}"
            );
        }
    });
}

async fn record_forward_attempt_failure_runtime_source(
    router: &ProviderRouter,
    attempt: &ForwardAttempt,
    app_type: &str,
    used_half_open_permit: bool,
    error: &ProxyError,
) {
    let error_message = error.to_string();
    if let Some(channel) = attempt.channel() {
        let _ = router
            .record_channel_result(
                &channel.channel_id,
                app_type,
                used_half_open_permit,
                false,
                Some(error_message),
                None,
            )
            .await;
        return;
    }

    let _ = router
        .record_result(
            &attempt.provider().id,
            app_type,
            used_half_open_permit,
            false,
            Some(error_message),
        )
        .await;
}

async fn release_forward_attempt_permit_neutral_runtime_source(
    router: &ProviderRouter,
    attempt: &ForwardAttempt,
    app_type: &str,
    used_half_open_permit: bool,
) {
    if let Some(channel) = attempt.channel() {
        router
            .release_channel_permit_neutral(&channel.channel_id, app_type, used_half_open_permit)
            .await;
        return;
    }

    router
        .release_permit_neutral(&attempt.provider().id, app_type, used_half_open_permit)
        .await;
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
            let max_attempts = effective_forward_max_attempts_for_channel(
                input.max_attempts,
                input.attempts.first().and_then(ForwardAttempt::channel),
            );
            let runtime_decision =
                forwarder_attempt_runtime_decision(ForwarderAttemptRuntimeDecisionInput {
                    app_type: input.app_type,
                    attempted_providers: input.attempted_providers,
                    max_attempts,
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

    fn record_success<'a>(&'a self, input: ForwarderAttemptSuccessInput<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_attempt_success_runtime_source(
                &self.router,
                input.attempt,
                input.app_type,
                input.used_half_open_permit,
            )
            .await;
        })
    }

    fn record_failure<'a>(&'a self, input: ForwarderAttemptFailureInput<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            record_forward_attempt_failure_runtime_source(
                self.router.as_ref(),
                input.attempt,
                input.app_type,
                input.used_half_open_permit,
                input.error,
            )
            .await;
            record_selected_channel_key_failure(
                self.db.as_ref(),
                input.attempt,
                chrono::Utc::now().timestamp_millis(),
            );
        })
    }

    fn release_attempt_permit_neutral<'a>(
        &'a self,
        input: ForwarderAttemptNeutralReleaseInput<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            release_forward_attempt_permit_neutral_runtime_source(
                self.router.as_ref(),
                input.attempt,
                input.app_type,
                input.used_half_open_permit,
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
    use crate::proxy_core::api::management::{
        ProxyChannelKeyWriteRequest, ProxyChannelWriteRequest,
    };
    use crate::proxy_core::api::ports::{ChannelKeyRuntimeLookupInput, ChannelKeyRuntimeSource};
    use crate::proxy_core::api::routing::ChannelRouteCandidate;
    use serde_json::json;

    fn lookup<'a>(channel_id: &'a str, key_ref: &'a str) -> ChannelKeyRuntimeLookupInput<'a> {
        ChannelKeyRuntimeLookupInput {
            channel_id,
            key_ref,
        }
    }

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
        let error = ProxyError::ForwardFailed("upstream timeout".to_string());

        source
            .record_failure(ForwarderAttemptFailureInput {
                attempt: &attempt,
                app_type: "claude",
                used_half_open_permit: false,
                error: &error,
            })
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
        let plain_channel_error = ProxyError::ForwardFailed("plain channel failed".to_string());
        source
            .record_failure(ForwarderAttemptFailureInput {
                attempt: &plain_channel_attempt,
                app_type: "claude",
                used_half_open_permit: false,
                error: &plain_channel_error,
            })
            .await;
        let provider_attempt = ForwardAttempt::from_provider(provider);
        let provider_error = ProxyError::ForwardFailed("provider failed".to_string());
        source
            .record_failure(ForwarderAttemptFailureInput {
                attempt: &provider_attempt,
                app_type: "claude",
                used_half_open_permit: false,
                error: &provider_error,
            })
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
            .load_channel_key_candidate(lookup("runtime-key-wildcard", "*"))
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
        let error = ProxyError::ForwardFailed("wildcard key failed".to_string());
        source
            .record_failure(ForwarderAttemptFailureInput {
                attempt: &attempt,
                app_type: "claude",
                used_half_open_permit: false,
                error: &error,
            })
            .await;

        let fallback = key_source
            .load_channel_key_candidate(lookup("runtime-key-wildcard", "*"))
            .expect("load fallback wildcard key")
            .expect("selected fallback key");
        assert_eq!(fallback.key_ref, "primary");
    }
}
