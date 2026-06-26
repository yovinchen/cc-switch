use crate::database::Database;
use crate::proxy_core_adapter::{
    AppKind, ChannelKeyRecord, ChannelMigrationMaterializeInput, ChannelMigrationPreviewInput,
    ChannelModelRecord, ChannelQuery, ChannelRecord, ChannelRouteSource, ChannelSource,
    ChannelSpec, ProxyChannelKeyPatchRequest, ProxyChannelKeyWriteRequest,
    ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest, ProxyChannelWriteRequest,
    ProxyCoreResult, channel_key_records_from_db_source,
    channel_migration_materialize_from_db_source, channel_migration_preview_from_db_source,
    channel_model_records_from_db_source, channel_record_from_db_source,
    channel_records_from_db_source, channel_spec_from_source_lookup,
    channel_specs_from_source_lookup, create_channel_record_from_db_source,
    delete_channel_key_record_from_db_source, delete_channel_record_from_db_source,
    materialized_channel_records_from_db_source, replace_channel_model_records_from_db_source,
    update_channel_key_record_from_db_source, update_channel_record_from_db_source,
    upsert_channel_key_record_from_db_source,
};
use futures::future::BoxFuture;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct CcSwitchChannelSource {
    db: Arc<Database>,
}

impl CcSwitchChannelSource {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl ChannelSource for CcSwitchChannelSource {
    fn list_channels<'a>(
        &'a self,
        query: ChannelQuery<'a>,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelSpec>>> {
        Box::pin(async move { channel_specs_from_source_lookup(&self.db, query) })
    }

    fn get_channel<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelSpec>>> {
        Box::pin(async move { channel_spec_from_source_lookup(&self.db, channel_id) })
    }

    fn create_channel_record<'a>(
        &'a self,
        request: ProxyChannelWriteRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelRecord>> {
        Box::pin(async move { create_channel_record_from_db_source(&self.db, request) })
    }

    fn get_channel_record<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelRecord>>> {
        Box::pin(async move { channel_record_from_db_source(&self.db, channel_id) })
    }

    fn update_channel_record<'a>(
        &'a self,
        channel_id: &'a str,
        patch: ProxyChannelPatchRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelRecord>>> {
        Box::pin(async move { update_channel_record_from_db_source(&self.db, channel_id, patch) })
    }

    fn delete_channel_record<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<bool>> {
        Box::pin(async move { delete_channel_record_from_db_source(&self.db, channel_id) })
    }

    fn list_channel_key_records<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<Vec<ChannelKeyRecord>>>> {
        Box::pin(async move { channel_key_records_from_db_source(&self.db, channel_id) })
    }

    fn upsert_channel_key_record<'a>(
        &'a self,
        channel_id: &'a str,
        key_ref: &'a str,
        request: ProxyChannelKeyWriteRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelKeyRecord>>> {
        Box::pin(async move {
            upsert_channel_key_record_from_db_source(&self.db, channel_id, key_ref, request)
        })
    }

    fn update_channel_key_record<'a>(
        &'a self,
        channel_id: &'a str,
        key_ref: &'a str,
        patch: ProxyChannelKeyPatchRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<ChannelKeyRecord>>> {
        Box::pin(async move {
            update_channel_key_record_from_db_source(&self.db, channel_id, key_ref, patch)
        })
    }

    fn delete_channel_key_record<'a>(
        &'a self,
        channel_id: &'a str,
        key_ref: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<bool>> {
        Box::pin(
            async move { delete_channel_key_record_from_db_source(&self.db, channel_id, key_ref) },
        )
    }

    fn list_channel_model_records<'a>(
        &'a self,
        channel_id: &'a str,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<Vec<ChannelModelRecord>>>> {
        Box::pin(async move { channel_model_records_from_db_source(&self.db, channel_id) })
    }

    fn replace_channel_model_records<'a>(
        &'a self,
        channel_id: &'a str,
        request: ProxyChannelModelsReplaceRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<Vec<ChannelModelRecord>>>> {
        Box::pin(async move {
            replace_channel_model_records_from_db_source(&self.db, channel_id, request)
        })
    }

    fn list_channel_records<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<(ChannelRouteSource, Vec<ChannelRecord>)>> {
        Box::pin(async move { channel_records_from_db_source(&self.db, app) })
    }

    fn list_materialized_channel_records<'a>(
        &'a self,
        app: Option<&'a AppKind>,
    ) -> BoxFuture<'a, ProxyCoreResult<Vec<ChannelRecord>>> {
        Box::pin(async move { materialized_channel_records_from_db_source(&self.db, app) })
    }

    fn preview_legacy_channel_migration<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelMigrationPreviewInput<ChannelRecord>>> {
        Box::pin(async move { channel_migration_preview_from_db_source(&self.db, app) })
    }

    fn materialize_legacy_channel_migration<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelMigrationMaterializeInput>> {
        Box::pin(async move { channel_migration_materialize_from_db_source(&self.db, app) })
    }
}
