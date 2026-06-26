use crate::database::{Database, ProxyChannelKeyRecord};
use crate::proxy_core_adapter::{
    app_error, channel_key_runtime_candidate_from_input,
    core_select_enabled_channel_key_runtime_candidate, ChannelKeyRuntimeCandidate,
    ChannelKeyRuntimeCandidateInput, ChannelKeyRuntimeSource, ProxyCoreResult,
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

#[derive(Clone)]
pub(crate) struct CcSwitchChannelKeyRuntimeSource {
    db: Arc<Database>,
}

pub(crate) fn channel_key_runtime_source_from_database(
    db: Arc<Database>,
) -> CcSwitchChannelKeyRuntimeSource {
    CcSwitchChannelKeyRuntimeSource { db }
}

fn load_channel_key_value_from_database(
    db: &Database,
    channel_id: &str,
    key_ref: &str,
) -> ProxyCoreResult<Option<String>> {
    let key = db
        .get_proxy_channel_key(channel_id, key_ref)
        .map_err(|error| app_error("load channel auth key", error))?;
    let selected_key = select_enabled_proxy_channel_key_runtime_candidate(key);
    Ok(selected_key.map(|key| key.key_value))
}

impl ChannelKeyRuntimeSource for CcSwitchChannelKeyRuntimeSource {
    fn load_channel_key_value(
        &self,
        channel_id: &str,
        key_ref: &str,
    ) -> ProxyCoreResult<Option<String>> {
        load_channel_key_value_from_database(self.db.as_ref(), channel_id, key_ref)
    }
}
