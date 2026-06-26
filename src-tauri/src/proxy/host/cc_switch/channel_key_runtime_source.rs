use crate::database::{Database, ProxyChannelKeyRecord};
#[cfg(test)]
use crate::proxy_core_adapter::core_select_enabled_channel_key_runtime_candidate;
use crate::proxy_core_adapter::{
    app_error, channel_key_runtime_candidate_from_input, core_select_channel_key_runtime_candidate,
    ChannelKeyRuntimeCandidate, ChannelKeyRuntimeCandidateInput, ChannelKeyRuntimeSource,
    ProxyCoreResult,
};
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

#[cfg(test)]
pub(crate) fn select_enabled_proxy_channel_key_runtime_candidate<I>(
    keys: I,
) -> Option<ChannelKeyRuntimeCandidate>
where
    I: IntoIterator<Item = ProxyChannelKeyRecord>,
{
    core_select_enabled_channel_key_runtime_candidate(
        keys.into_iter()
            .map(proxy_channel_key_record_to_runtime_candidate),
    )
}

fn select_proxy_channel_key_runtime_candidate<I>(
    keys: I,
    key_ref: &str,
) -> Option<ChannelKeyRuntimeCandidate>
where
    I: IntoIterator<Item = ProxyChannelKeyRecord>,
{
    core_select_channel_key_runtime_candidate(
        keys.into_iter()
            .map(proxy_channel_key_record_to_runtime_candidate),
        key_ref,
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
    Ok(keys.and_then(|keys| select_proxy_channel_key_runtime_candidate(keys, key_ref)))
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
