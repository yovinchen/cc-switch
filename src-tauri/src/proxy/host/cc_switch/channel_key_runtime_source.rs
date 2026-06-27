use crate::database::{Database, ProxyChannelKeyRecord};
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::management::{
    channel_key_runtime_candidate_from_input,
    select_channel_key_runtime_candidate_with_weighted_roll, ChannelKeyRuntimeCandidate,
    ChannelKeyRuntimeCandidateInput, DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS,
};
use crate::proxy_core::api::ports::ChannelKeyRuntimeSource;
use crate::proxy_core::api::routing::effective_channel_key_failure_cooldown_ms;
use crate::proxy_core_adapter::app_error;
use std::sync::Arc;

pub(crate) fn proxy_channel_key_record_to_runtime_candidate(
    key: ProxyChannelKeyRecord,
) -> ChannelKeyRuntimeCandidate {
    channel_key_runtime_candidate_from_input(ChannelKeyRuntimeCandidateInput {
        channel_id: key.channel_id,
        key_ref: key.key_ref,
        key_value: key.key_value,
        status: key.status,
        priority: key.priority,
        weight: key.weight,
        last_failure_at: key.last_failure_at,
    })
}

fn select_proxy_channel_key_runtime_candidate<I>(
    keys: I,
    key_ref: &str,
    now_ms: i64,
    failure_cooldown_ms: i64,
    weighted_roll: u64,
) -> Option<ChannelKeyRuntimeCandidate>
where
    I: IntoIterator<Item = ProxyChannelKeyRecord>,
{
    select_channel_key_runtime_candidate_with_weighted_roll(
        keys.into_iter()
            .map(proxy_channel_key_record_to_runtime_candidate),
        key_ref,
        now_ms,
        failure_cooldown_ms,
        weighted_roll,
    )
}

#[derive(Clone)]
pub(crate) struct CcSwitchChannelKeyRuntimeSource {
    db: Arc<Database>,
}

pub(crate) fn channel_key_runtime_source_from_database(
    db: Arc<Database>,
) -> CcSwitchChannelKeyRuntimeSource {
    CcSwitchChannelKeyRuntimeSource { db }
}

fn load_channel_key_candidate_from_database(
    db: &Database,
    channel_id: &str,
    key_ref: &str,
) -> ProxyCoreResult<Option<ChannelKeyRuntimeCandidate>> {
    let keys = db
        .list_proxy_channel_key_runtime_candidates(channel_id)
        .map_err(|error| app_error("load channel auth key", error))?;
    let failure_cooldown_ms = channel_key_failure_cooldown_ms_from_database(db, channel_id)?;
    let (now_ms, weighted_roll) = channel_key_runtime_selection_clock();
    Ok(keys.and_then(|keys| {
        select_proxy_channel_key_runtime_candidate(
            keys,
            key_ref,
            now_ms,
            failure_cooldown_ms,
            weighted_roll,
        )
    }))
}

fn channel_key_failure_cooldown_ms_from_database(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<i64> {
    let channel = db
        .get_proxy_channel(channel_id)
        .map_err(|error| app_error("load channel key health policy", error))?;

    Ok(channel
        .map(|channel| {
            effective_channel_key_failure_cooldown_ms(
                DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS,
                &channel.health_policy,
            )
        })
        .unwrap_or(DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS))
}

fn channel_key_runtime_selection_clock() -> (i64, u64) {
    let now = chrono::Utc::now();
    let now_ms = now.timestamp_millis();
    let weighted_roll = now
        .timestamp_nanos_opt()
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_else(|| u64::try_from(now_ms).unwrap_or_default());

    (now_ms, weighted_roll)
}

impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource {
    fn load_channel_key_candidate(
        &self,
        channel_id: &str,
        key_ref: &str,
    ) -> ProxyCoreResult<Option<ChannelKeyRuntimeCandidate>> {
        load_channel_key_candidate_from_database(self.db.as_ref(), channel_id, key_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::proxy_core_adapter::{ProxyChannelKeyWriteRequest, ProxyChannelWriteRequest};
    use serde_json::json;

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
            DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS,
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
            DEFAULT_CHANNEL_KEY_FAILURE_COOLDOWN_MS,
            1,
        )
        .expect("selected second weighted candidate");
        assert_eq!(second.key_ref, "beta");
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
            .load_channel_key_candidate("channel-key-candidate", "primary")
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
            .load_channel_key_candidate("channel-key-candidate", "backup")
            .expect("load backup channel key")
            .expect("selected backup candidate");
        assert_eq!(backup.key_ref, "backup");
        assert_eq!(backup.key_value, "sk-backup-candidate");
        assert_eq!(backup.priority, 100);

        let wildcard = source
            .load_channel_key_candidate("channel-key-candidate", "*")
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
            .load_channel_key_candidate("channel-key-candidate", "*")
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
            .load_channel_key_candidate("channel-key-candidate", "*")
            .expect("load wildcard channel key after cooldown")
            .expect("selected recovered candidate");
        assert_eq!(recovered.key_ref, "backup");
        assert_eq!(recovered.key_value, "sk-backup-candidate");

        let _ = db
            .update_proxy_channel(
                "channel-key-candidate",
                crate::proxy_core_adapter::ProxyChannelPatchRequest {
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
            .load_channel_key_candidate("channel-key-candidate", "*")
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
                .load_channel_key_candidate("channel-key-candidate", "primary")
                .expect("load disabled channel key")
                .is_none(),
            "disabled keys should not produce runtime candidates"
        );
        assert!(
            source
                .load_channel_key_candidate("channel-key-candidate", "missing")
                .expect("load missing channel key")
                .is_none(),
            "missing key refs should not fall back to another channel key"
        );
    }
}
