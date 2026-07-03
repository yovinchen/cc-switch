use crate::database::{Database, ProxyChannelKeyRecord};
use crate::proxy_core::api::errors::{config_error_with_context, ProxyCoreResult};
use crate::proxy_core::api::management::{
    channel_key_runtime_candidate_from_parts, channel_key_runtime_round_robin_cursor_key,
    channel_key_runtime_selection_tick_from_time_parts,
    effective_channel_key_runtime_selection_policy,
    select_channel_key_runtime_candidate_with_policy, ChannelKeyRuntimeCandidate,
    ChannelKeyRuntimeSelectionInput, ChannelKeyRuntimeSelectionPolicy,
    ChannelKeyRuntimeSelectionStrategy, ChannelKeyRuntimeSelectionTick,
    DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS,
};
use crate::proxy_core::api::ports::{ChannelKeyRuntimeLookupInput, ChannelKeyRuntimeSource};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(crate) fn proxy_channel_key_record_to_runtime_candidate(
    key: ProxyChannelKeyRecord,
) -> ChannelKeyRuntimeCandidate {
    channel_key_runtime_candidate_from_parts(
        key.channel_id,
        key.key_ref,
        key.key_value,
        key.status,
        key.priority,
        key.weight,
        key.last_failure_at,
    )
}

fn select_proxy_channel_key_runtime_candidate<I>(
    keys: I,
    key_ref: &str,
    now_ms: i64,
    policy: ChannelKeyRuntimeSelectionPolicy,
    weighted_roll: u64,
    round_robin_offset: u64,
) -> Option<ChannelKeyRuntimeCandidate>
where
    I: IntoIterator<Item = ProxyChannelKeyRecord>,
{
    select_channel_key_runtime_candidate_with_policy(
        keys.into_iter()
            .map(proxy_channel_key_record_to_runtime_candidate),
        ChannelKeyRuntimeSelectionInput {
            key_ref,
            now_ms,
            weighted_roll,
            round_robin_offset,
            policy,
        },
    )
}

#[derive(Clone)]
pub(crate) struct CcSwitchChannelKeyRuntimeSource {
    db: Arc<Database>,
    round_robin_cursors: Arc<Mutex<HashMap<String, u64>>>,
    selection_clock: Arc<dyn Fn() -> ChannelKeyRuntimeSelectionTick + Send + Sync>,
}

pub(crate) fn channel_key_runtime_source_from_database(
    db: Arc<Database>,
) -> CcSwitchChannelKeyRuntimeSource {
    channel_key_runtime_source_from_database_with_selection_clock(
        db,
        Arc::new(channel_key_runtime_selection_clock),
    )
}

fn channel_key_runtime_source_from_database_with_selection_clock(
    db: Arc<Database>,
    selection_clock: Arc<dyn Fn() -> ChannelKeyRuntimeSelectionTick + Send + Sync>,
) -> CcSwitchChannelKeyRuntimeSource {
    CcSwitchChannelKeyRuntimeSource {
        db,
        round_robin_cursors: Arc::new(Mutex::new(HashMap::new())),
        selection_clock,
    }
}

fn load_channel_key_candidate_from_database(
    db: &Database,
    round_robin_cursors: &Mutex<HashMap<String, u64>>,
    selection_clock: &(dyn Fn() -> ChannelKeyRuntimeSelectionTick + Send + Sync),
    channel_id: &str,
    key_ref: &str,
) -> ProxyCoreResult<Option<ChannelKeyRuntimeCandidate>> {
    let keys = db
        .list_proxy_channel_key_runtime_candidates(channel_id)
        .map_err(|error| config_error_with_context("load channel auth key", error))?;
    let Some(keys) = keys else {
        return Ok(None);
    };
    let policy = channel_key_runtime_selection_policy_from_database(db, channel_id)?;
    let round_robin_offset =
        channel_key_round_robin_offset(round_robin_cursors, channel_id, key_ref, policy.strategy)?;
    let tick = selection_clock();
    Ok(select_proxy_channel_key_runtime_candidate(
        keys,
        key_ref,
        tick.now_ms,
        policy,
        tick.weighted_roll,
        round_robin_offset,
    ))
}

fn channel_key_runtime_selection_policy_from_database(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<ChannelKeyRuntimeSelectionPolicy> {
    let channel = db
        .get_proxy_channel(channel_id)
        .map_err(|error| config_error_with_context("load channel key health policy", error))?;

    Ok(channel
        .map(|channel| {
            effective_channel_key_runtime_selection_policy(
                DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS,
                &channel.health_policy,
            )
        })
        .unwrap_or_default())
}

fn channel_key_round_robin_offset(
    round_robin_cursors: &Mutex<HashMap<String, u64>>,
    channel_id: &str,
    key_ref: &str,
    strategy: ChannelKeyRuntimeSelectionStrategy,
) -> ProxyCoreResult<u64> {
    let Some(cursor_key) =
        channel_key_runtime_round_robin_cursor_key(channel_id, key_ref, strategy)
    else {
        return Ok(0);
    };

    let mut cursors = round_robin_cursors.lock().map_err(|error| {
        config_error_with_context("advance channel key round-robin cursor", error)
    })?;
    let offset = cursors.entry(cursor_key).or_insert(0);
    let current = *offset;
    *offset = (*offset).wrapping_add(1);
    Ok(current)
}

fn channel_key_runtime_selection_clock() -> ChannelKeyRuntimeSelectionTick {
    let now = chrono::Utc::now();
    channel_key_runtime_selection_tick_from_time_parts(
        now.timestamp_millis(),
        now.timestamp_nanos_opt(),
    )
}

impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource {
    fn load_channel_key_candidate(
        &self,
        input: ChannelKeyRuntimeLookupInput<'_>,
    ) -> ProxyCoreResult<Option<ChannelKeyRuntimeCandidate>> {
        load_channel_key_candidate_from_database(
            self.db.as_ref(),
            self.round_robin_cursors.as_ref(),
            self.selection_clock.as_ref(),
            input.channel_id,
            input.key_ref,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::proxy_core::api::management::{
        ChannelKeyRuntimeSelectionStrategy, ProxyChannelKeyWriteRequest, ProxyChannelPatchRequest,
        ProxyChannelWriteRequest,
    };
    use serde_json::json;

    fn lookup<'a>(channel_id: &'a str, key_ref: &'a str) -> ChannelKeyRuntimeLookupInput<'a> {
        ChannelKeyRuntimeLookupInput {
            channel_id,
            key_ref,
        }
    }

    fn proxy_channel_key_record(
        key_ref: &str,
        priority: i64,
        weight: u32,
        last_failure_at: Option<i64>,
    ) -> ProxyChannelKeyRecord {
        ProxyChannelKeyRecord {
            channel_id: "channel-key-candidate".to_string(),
            key_ref: key_ref.to_string(),
            key_value: format!("sk-{key_ref}"),
            status: "enabled".to_string(),
            priority,
            weight,
            last_failure_at,
        }
    }

    #[test]
    fn channel_key_runtime_source_passes_weighted_roll_to_core_selector() {
        let now_ms = 1_771_000_120_000;
        let first = select_proxy_channel_key_runtime_candidate(
            vec![
                proxy_channel_key_record("alpha", 20, 1, None),
                proxy_channel_key_record("beta", 20, 3, None),
                proxy_channel_key_record("lower-priority", 10, 100, None),
            ],
            "*",
            now_ms,
            ChannelKeyRuntimeSelectionPolicy::weighted(DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS),
            0,
            0,
        )
        .expect("selected first weighted candidate");
        assert_eq!(first.key_ref, "alpha");

        let second = select_proxy_channel_key_runtime_candidate(
            vec![
                proxy_channel_key_record("alpha", 20, 1, None),
                proxy_channel_key_record("beta", 20, 3, None),
                proxy_channel_key_record("lower-priority", 10, 100, None),
            ],
            "*",
            now_ms,
            ChannelKeyRuntimeSelectionPolicy::weighted(DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS),
            1,
            0,
        )
        .expect("selected second weighted candidate");
        assert_eq!(second.key_ref, "beta");
    }

    #[test]
    fn channel_key_runtime_source_passes_random_roll_to_core_selector() {
        let now_ms = 1_771_000_120_000;
        let selected = select_proxy_channel_key_runtime_candidate(
            vec![
                proxy_channel_key_record("alpha", 20, 1_000, None),
                proxy_channel_key_record("beta", 20, 1, None),
                proxy_channel_key_record("gamma", 20, 1, None),
                proxy_channel_key_record("lower-priority", 10, 1, None),
            ],
            "*",
            now_ms,
            ChannelKeyRuntimeSelectionPolicy::random(DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS),
            2,
            0,
        )
        .expect("selected random candidate");

        assert_eq!(selected.key_ref, "gamma");
    }

    #[test]
    fn channel_key_runtime_source_reads_selection_policy_from_channel_health_policy() {
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
            id: Some("channel-key-policy".to_string()),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Channel Key Policy".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            health_policy: json!({
                "channelKeySelectionStrategy": "priority",
                "channelKeyFailureCooldownMs": 5_000
            }),
            ..Default::default()
        })
        .expect("create channel");

        let policy =
            channel_key_runtime_selection_policy_from_database(db.as_ref(), "channel-key-policy")
                .expect("load channel key selection policy");
        assert_eq!(
            policy.strategy,
            ChannelKeyRuntimeSelectionStrategy::Priority
        );
        assert_eq!(policy.failure_cooldown_ms, 5_000);

        let _ = db
            .update_proxy_channel(
                "channel-key-policy",
                ProxyChannelPatchRequest {
                    health_policy: Some(json!({
                        "channelKeySelectionStrategy": "random",
                        "keyFailureCooldownMs": 5_000
                    })),
                    ..Default::default()
                },
            )
            .expect("patch random key selection policy");
        let random_policy =
            channel_key_runtime_selection_policy_from_database(db.as_ref(), "channel-key-policy")
                .expect("load random channel key selection policy");
        assert_eq!(
            random_policy.strategy,
            ChannelKeyRuntimeSelectionStrategy::Random
        );
        assert_eq!(random_policy.failure_cooldown_ms, 5_000);

        let selected = select_proxy_channel_key_runtime_candidate(
            vec![
                proxy_channel_key_record("alpha", 20, 1, None),
                proxy_channel_key_record("beta", 20, 1, None),
            ],
            "*",
            1_771_000_120_000,
            policy,
            1,
            0,
        )
        .expect("selected priority candidate from configured policy");
        assert_eq!(selected.key_ref, "alpha");
    }

    #[test]
    fn db_backed_channel_key_runtime_source_round_robins_wildcard_keys() {
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
            id: Some("channel-key-round-robin".to_string()),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Channel Key Round Robin".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            health_policy: json!({"channelKeySelectionStrategy": "roundRobin"}),
            ..Default::default()
        })
        .expect("create channel");
        db.upsert_proxy_channel_key(
            "channel-key-round-robin",
            "alpha",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-alpha".to_string(),
                status: "enabled".to_string(),
                priority: 20,
                weight: 1,
            },
        )
        .expect("upsert alpha key");
        db.upsert_proxy_channel_key(
            "channel-key-round-robin",
            "beta",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-beta".to_string(),
                status: "enabled".to_string(),
                priority: 20,
                weight: 1,
            },
        )
        .expect("upsert beta key");

        let source = channel_key_runtime_source_from_database(db);
        let first = source
            .load_channel_key_candidate(lookup("channel-key-round-robin", "*"))
            .expect("load first wildcard key")
            .expect("selected first key");
        let second = source
            .load_channel_key_candidate(lookup("channel-key-round-robin", "*"))
            .expect("load second wildcard key")
            .expect("selected second key");
        let third = source
            .load_channel_key_candidate(lookup("channel-key-round-robin", "*"))
            .expect("load third wildcard key")
            .expect("selected third key");

        assert_eq!(first.key_ref, "alpha");
        assert_eq!(second.key_ref, "beta");
        assert_eq!(third.key_ref, "alpha");
    }

    #[test]
    fn db_backed_channel_key_runtime_source_scopes_round_robin_cursor_by_channel() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({ "env": { "ANTHROPIC_API_KEY": "provider-key" } }),
            None,
        );
        db.save_provider("claude", &provider)
            .expect("save provider");
        for channel_id in ["channel-key-round-robin-a", "channel-key-round-robin-b"] {
            db.create_proxy_channel(ProxyChannelWriteRequest {
                id: Some(channel_id.to_string()),
                provider_id: "provider-a".to_string(),
                app_type: "claude".to_string(),
                name: format!("Channel Key Round Robin {channel_id}"),
                base_url: format!("https://{channel_id}.relay.example.com/v1"),
                interface_kind: "anthropic_messages".to_string(),
                health_policy: json!({"channelKeySelectionStrategy": "roundRobin"}),
                ..Default::default()
            })
            .expect("create channel");
            for key_ref in ["alpha", "beta"] {
                db.upsert_proxy_channel_key(
                    channel_id,
                    key_ref,
                    ProxyChannelKeyWriteRequest {
                        key_value: format!("sk-{channel_id}-{key_ref}"),
                        status: "enabled".to_string(),
                        priority: 20,
                        weight: 1,
                    },
                )
                .expect("upsert channel key");
            }
        }

        let source = channel_key_runtime_source_from_database(db);
        let first_a = source
            .load_channel_key_candidate(lookup("channel-key-round-robin-a", "*"))
            .expect("load first channel a wildcard key")
            .expect("selected first channel a key");
        let first_b = source
            .load_channel_key_candidate(lookup("channel-key-round-robin-b", "*"))
            .expect("load first channel b wildcard key")
            .expect("selected first channel b key");
        let second_a = source
            .load_channel_key_candidate(lookup("channel-key-round-robin-a", "*"))
            .expect("load second channel a wildcard key")
            .expect("selected second channel a key");
        let second_b = source
            .load_channel_key_candidate(lookup("channel-key-round-robin-b", "*"))
            .expect("load second channel b wildcard key")
            .expect("selected second channel b key");

        assert_eq!(first_a.key_ref, "alpha");
        assert_eq!(first_b.key_ref, "alpha");
        assert_eq!(second_a.key_ref, "beta");
        assert_eq!(second_b.key_ref, "beta");
        assert_eq!(first_a.key_value, "sk-channel-key-round-robin-a-alpha");
        assert_eq!(first_b.key_value, "sk-channel-key-round-robin-b-alpha");
    }

    #[test]
    fn db_backed_channel_key_runtime_source_uses_injected_weighted_roll() {
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
            id: Some("channel-key-weighted".to_string()),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Channel Key Weighted".to_string(),
            base_url: "https://weighted.relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            health_policy: json!({"channelKeySelectionStrategy": "weightedRandom"}),
            ..Default::default()
        })
        .expect("create channel");
        db.upsert_proxy_channel_key(
            "channel-key-weighted",
            "alpha",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-alpha".to_string(),
                status: "enabled".to_string(),
                priority: 20,
                weight: 1,
            },
        )
        .expect("upsert alpha key");
        db.upsert_proxy_channel_key(
            "channel-key-weighted",
            "beta",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-beta".to_string(),
                status: "enabled".to_string(),
                priority: 20,
                weight: 3,
            },
        )
        .expect("upsert beta key");

        let source = channel_key_runtime_source_from_database_with_selection_clock(
            db,
            Arc::new(|| ChannelKeyRuntimeSelectionTick::new(1_771_000_120_000, 1)),
        );
        let selected = source
            .load_channel_key_candidate(lookup("channel-key-weighted", "*"))
            .expect("load weighted wildcard key")
            .expect("selected weighted key");

        assert_eq!(selected.key_ref, "beta");
        assert_eq!(selected.key_value, "sk-beta");
    }

    #[test]
    fn db_backed_channel_key_runtime_source_returns_selected_candidate_metadata() {
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
            id: Some("channel-key-candidate".to_string()),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Channel Key Candidate".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            ..Default::default()
        })
        .expect("create channel");
        db.upsert_proxy_channel_key(
            "channel-key-candidate",
            "primary",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-channel-candidate".to_string(),
                status: "enabled".to_string(),
                priority: 42,
                weight: 7,
            },
        )
        .expect("upsert channel key");
        db.upsert_proxy_channel_key(
            "channel-key-candidate",
            "backup",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-backup-candidate".to_string(),
                status: "enabled".to_string(),
                priority: 100,
                weight: 100,
            },
        )
        .expect("upsert backup channel key");

        let source = channel_key_runtime_source_from_database(db.clone());
        let candidate = source
            .load_channel_key_candidate(lookup("channel-key-candidate", "primary"))
            .expect("load channel key")
            .expect("selected candidate");

        assert_eq!(candidate.channel_id, "channel-key-candidate");
        assert_eq!(candidate.key_ref, "primary");
        assert_eq!(candidate.key_value, "sk-channel-candidate");
        assert_eq!(candidate.status, "enabled");
        assert_eq!(candidate.priority, 42);
        assert_eq!(candidate.weight, 7);
        assert_eq!(candidate.last_failure_at, None);

        let backup = source
            .load_channel_key_candidate(lookup("channel-key-candidate", "backup"))
            .expect("load backup channel key")
            .expect("selected backup candidate");
        assert_eq!(backup.key_ref, "backup");
        assert_eq!(backup.key_value, "sk-backup-candidate");
        assert_eq!(backup.priority, 100);

        let wildcard = source
            .load_channel_key_candidate(lookup("channel-key-candidate", "*"))
            .expect("load wildcard channel key")
            .expect("selected wildcard candidate");
        assert_eq!(wildcard.key_ref, "backup");
        assert_eq!(wildcard.key_value, "sk-backup-candidate");
        assert_eq!(wildcard.priority, 100);

        {
            let conn = db.conn.lock().expect("lock db");
            let recent_failure_at = chrono::Utc::now().timestamp_millis()
                - (DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS / 2);
            conn.execute(
                "UPDATE proxy_channel_keys SET last_failure_at = ?1
                 WHERE channel_id = ?2 AND key_ref = ?3",
                (recent_failure_at, "channel-key-candidate", "backup"),
            )
            .expect("mark backup channel key recently failed");
        }

        let fallback = source
            .load_channel_key_candidate(lookup("channel-key-candidate", "*"))
            .expect("load wildcard channel key after failure")
            .expect("selected healthy fallback candidate");
        assert_eq!(fallback.key_ref, "primary");
        assert_eq!(fallback.key_value, "sk-channel-candidate");

        {
            let conn = db.conn.lock().expect("lock db");
            let expired_failure_at = chrono::Utc::now().timestamp_millis()
                - (DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS + 1);
            conn.execute(
                "UPDATE proxy_channel_keys SET last_failure_at = ?1
                 WHERE channel_id = ?2 AND key_ref = ?3",
                (expired_failure_at, "channel-key-candidate", "backup"),
            )
            .expect("mark backup channel key failure expired");
        }

        let recovered = source
            .load_channel_key_candidate(lookup("channel-key-candidate", "*"))
            .expect("load wildcard channel key after cooldown")
            .expect("selected recovered candidate");
        assert_eq!(recovered.key_ref, "backup");
        assert_eq!(recovered.key_value, "sk-backup-candidate");

        let _ = db
            .update_proxy_channel(
                "channel-key-candidate",
                ProxyChannelPatchRequest {
                    health_policy: Some(json!({"keyFailureCooldownMs": 1})),
                    ..Default::default()
                },
            )
            .expect("patch channel key cooldown policy");
        {
            let conn = db.conn.lock().expect("lock db");
            let recent_failure_at = chrono::Utc::now().timestamp_millis() - 10;
            conn.execute(
                "UPDATE proxy_channel_keys SET last_failure_at = ?1
                 WHERE channel_id = ?2 AND key_ref = ?3",
                (recent_failure_at, "channel-key-candidate", "backup"),
            )
            .expect("mark backup channel key failure outside custom cooldown");
        }

        let custom_cooldown_recovered = source
            .load_channel_key_candidate(lookup("channel-key-candidate", "*"))
            .expect("load wildcard channel key after custom cooldown")
            .expect("selected custom cooldown candidate");
        assert_eq!(custom_cooldown_recovered.key_ref, "backup");
        assert_eq!(custom_cooldown_recovered.key_value, "sk-backup-candidate");

        db.upsert_proxy_channel_key(
            "channel-key-candidate",
            "primary",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-channel-candidate".to_string(),
                status: "disabled".to_string(),
                priority: 42,
                weight: 7,
            },
        )
        .expect("disable channel key");

        assert!(
            source
                .load_channel_key_candidate(lookup("channel-key-candidate", "primary"))
                .expect("load disabled channel key")
                .is_none(),
            "disabled keys should not produce runtime candidates"
        );
        assert!(
            source
                .load_channel_key_candidate(lookup("channel-key-candidate", "missing"))
                .expect("load missing channel key")
                .is_none(),
            "missing key refs should not fall back to another channel key"
        );
    }
}
