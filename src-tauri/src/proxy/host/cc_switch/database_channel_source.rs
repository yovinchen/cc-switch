use crate::database::{
    Database, ProxyChannelKeyRecord, ProxyChannelMaterializeResult, ProxyChannelMigrationPreview,
    ProxyChannelModelRecord, ProxyChannelRecord,
};
use crate::error::AppError;
use crate::proxy_core_adapter::{
    app_error, app_write_error, channel_key_record_from_input, channel_matches_query,
    channel_model_record_from_input, channel_record_from_input,
    channel_route_source_for_materialized_count, channel_spec_from_input, AppKind,
    ChannelKeyRecord, ChannelKeyRecordInput, ChannelMigrationMaterializeInput,
    ChannelMigrationPreviewInput, ChannelModelRecord, ChannelModelRecordInput, ChannelQuery,
    ChannelRecord, ChannelRecordInput, ChannelRouteSource, ChannelSource, ChannelSpec,
    ChannelSpecInput, ModelRouteInput, ProxyChannelKeyPatchRequest, ProxyChannelKeyWriteRequest,
    ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest, ProxyChannelWriteRequest,
    ProxyCoreResult,
};
use futures::future::BoxFuture;
use std::sync::Arc;

fn proxy_channel_model_record_to_model_route_input(
    model: &ProxyChannelModelRecord,
) -> ModelRouteInput {
    ModelRouteInput {
        public_model: model.public_model.clone(),
        upstream_model: model.upstream_model.clone(),
        capabilities: model.capabilities.clone(),
        pricing_model: model.pricing_model.clone(),
        request_overrides: model.request_overrides.clone(),
        response_overrides: model.response_overrides.clone(),
    }
}

pub(crate) fn proxy_channel_record_to_core_spec(channel: &ProxyChannelRecord) -> ChannelSpec {
    channel_spec_from_input(ChannelSpecInput {
        id: channel.id.clone(),
        provider_id: channel.provider_id.clone(),
        app_type: channel.app_type.clone(),
        name: channel.name.clone(),
        status: channel.status.clone(),
        base_url: channel.base_url.clone(),
        interface_kind: channel.interface_kind.clone(),
        auth_profile_ref: channel.auth_profile_ref.clone(),
        models: channel
            .models
            .iter()
            .map(proxy_channel_model_record_to_model_route_input)
            .collect(),
        groups: channel.groups.clone(),
        priority: channel.priority,
        weight: channel.weight,
        retry_policy: channel.retry_policy.clone(),
        health_policy: channel.health_policy.clone(),
        header_overrides: channel.header_overrides.clone(),
        param_overrides: channel.param_overrides.clone(),
        status_code_mapping: channel.status_code_mapping.clone(),
        tags: channel.tags.clone(),
        metadata: channel.metadata.clone(),
        source_ref: channel.source_endpoint_url.clone(),
        needs_review: channel.needs_review,
        review_reasons: channel.review_reasons.clone(),
    })
}

pub(crate) fn proxy_channel_records_to_core_specs_for_query(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
    query: &ChannelQuery<'_>,
) -> Vec<ChannelSpec> {
    channels
        .into_iter()
        .map(|channel| proxy_channel_record_to_core_spec(&channel))
        .filter(|channel| channel_matches_query(channel, query))
        .collect()
}

pub(crate) fn channel_specs_from_source(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
    query: &ChannelQuery<'_>,
) -> Vec<ChannelSpec> {
    proxy_channel_records_to_core_specs_for_query(channels, query)
}

pub(crate) fn channel_specs_from_source_lookup(
    db: &Database,
    query: ChannelQuery<'_>,
) -> ProxyCoreResult<Vec<ChannelSpec>> {
    let channels = if query.allow_legacy_projection {
        channel_route_records_from_db_source(db, query.app.as_str())?.0
    } else {
        db.list_proxy_channels_for_app(query.app.as_str())
            .map_err(|error| app_error("list materialized channels", error))?
    };
    Ok(channel_specs_from_source(channels, &query))
}

pub(crate) fn channel_spec_from_source(channel: Option<ProxyChannelRecord>) -> Option<ChannelSpec> {
    channel.map(|channel| proxy_channel_record_to_core_spec(&channel))
}

pub(crate) fn channel_spec_from_source_lookup(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<Option<ChannelSpec>> {
    let channel = db
        .get_proxy_channel(channel_id)
        .map_err(|error| app_error("get channel", error))?;
    Ok(channel_spec_from_source(channel))
}

pub(crate) fn channel_route_records_from_sources(
    materialized_channels: Vec<ProxyChannelRecord>,
    load_legacy_projection: impl FnOnce() -> Result<ProxyChannelMigrationPreview, AppError>,
) -> Result<(Vec<ProxyChannelRecord>, ChannelRouteSource), AppError> {
    let source = channel_route_source_for_materialized_count(materialized_channels.len());
    if source != ChannelRouteSource::LegacyProjection {
        return Ok((materialized_channels, source));
    }

    let preview = load_legacy_projection()?;
    Ok((preview.channels, source))
}

pub(crate) fn channel_route_records_from_db_source(
    db: &Database,
    app_type: &str,
) -> ProxyCoreResult<(Vec<ProxyChannelRecord>, ChannelRouteSource)> {
    let channels = db
        .list_proxy_channels_for_app(app_type)
        .map_err(|error| app_error("list channel route records", error))?;
    channel_route_records_from_sources(channels, || {
        db.preview_legacy_proxy_channel_migration(app_type)
    })
    .map_err(|error| app_error("load channel route records", error))
}

fn proxy_channel_model_record_to_core_input(
    model: &ProxyChannelModelRecord,
) -> ChannelModelRecordInput {
    ChannelModelRecordInput {
        channel_id: model.channel_id.clone(),
        public_model: model.public_model.clone(),
        upstream_model: model.upstream_model.clone(),
        capabilities: model.capabilities.clone(),
        pricing_model: model.pricing_model.clone(),
        request_overrides: model.request_overrides.clone(),
        response_overrides: model.response_overrides.clone(),
    }
}

fn proxy_channel_model_record_to_core(model: &ProxyChannelModelRecord) -> ChannelModelRecord {
    channel_model_record_from_input(proxy_channel_model_record_to_core_input(model))
}

pub(crate) fn proxy_channel_model_records_to_core(
    models: Vec<ProxyChannelModelRecord>,
) -> Vec<ChannelModelRecord> {
    models
        .iter()
        .map(proxy_channel_model_record_to_core)
        .collect()
}

pub(crate) fn channel_model_records_from_db_source(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<Option<Vec<ChannelModelRecord>>> {
    let channel_exists = db
        .get_proxy_channel(channel_id)
        .map_err(|error| app_error("get channel for model records", error))?
        .is_some();

    if !channel_exists {
        return Ok(None);
    }

    let models = db
        .list_proxy_channel_models(channel_id)
        .map_err(|error| app_error("list channel model records", error))?;
    Ok(Some(proxy_channel_model_records_to_core(models)))
}

pub(crate) fn replace_channel_model_records_from_db_source(
    db: &Database,
    channel_id: &str,
    request: ProxyChannelModelsReplaceRequest,
) -> ProxyCoreResult<Option<Vec<ChannelModelRecord>>> {
    let models = db
        .replace_proxy_channel_models(channel_id, request)
        .map_err(|error| app_write_error("replace channel model records", error))?;
    Ok(models.map(proxy_channel_model_records_to_core))
}

pub(crate) fn proxy_channel_record_to_core(channel: ProxyChannelRecord) -> ChannelRecord {
    channel_record_from_input(ChannelRecordInput {
        id: channel.id,
        provider_id: channel.provider_id,
        app_type: channel.app_type,
        name: channel.name,
        status: channel.status,
        base_url: channel.base_url,
        interface_kind: channel.interface_kind,
        auth_profile_ref: channel.auth_profile_ref,
        groups: channel.groups,
        priority: channel.priority,
        weight: channel.weight,
        retry_policy: channel.retry_policy,
        health_policy: channel.health_policy,
        header_overrides: channel.header_overrides,
        param_overrides: channel.param_overrides,
        status_code_mapping: channel.status_code_mapping,
        tags: channel.tags,
        metadata: channel.metadata,
        source_kind: channel.source_kind.as_str().to_string(),
        source_endpoint_url: channel.source_endpoint_url,
        models: channel
            .models
            .iter()
            .map(proxy_channel_model_record_to_core_input)
            .collect(),
        needs_review: channel.needs_review,
        review_reasons: channel.review_reasons,
    })
}

pub(crate) fn create_channel_record_from_db_source(
    db: &Database,
    request: ProxyChannelWriteRequest,
) -> ProxyCoreResult<ChannelRecord> {
    let channel = db
        .create_proxy_channel(request)
        .map_err(|error| app_write_error("create channel record", error))?;
    Ok(proxy_channel_record_to_core(channel))
}

pub(crate) fn channel_record_from_db_source(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<Option<ChannelRecord>> {
    let channel = db
        .get_proxy_channel(channel_id)
        .map_err(|error| app_error("get channel record", error))?;
    Ok(channel.map(proxy_channel_record_to_core))
}

pub(crate) fn update_channel_record_from_db_source(
    db: &Database,
    channel_id: &str,
    patch: ProxyChannelPatchRequest,
) -> ProxyCoreResult<Option<ChannelRecord>> {
    let channel = db
        .update_proxy_channel(channel_id, patch)
        .map_err(|error| app_write_error("update channel record", error))?;
    Ok(channel.map(proxy_channel_record_to_core))
}

pub(crate) fn delete_channel_record_from_db_source(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<bool> {
    db.delete_proxy_channel(channel_id)
        .map_err(|error| app_error("delete channel record", error))
}

pub(crate) fn proxy_channel_records_to_core(
    channels: Vec<ProxyChannelRecord>,
) -> Vec<ChannelRecord> {
    channels
        .into_iter()
        .map(proxy_channel_record_to_core)
        .collect()
}

pub(crate) fn channel_records_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<(ChannelRouteSource, Vec<ChannelRecord>)> {
    let (channels, source) = channel_route_records_from_db_source(db, app.as_str())?;
    Ok((source, proxy_channel_records_to_core(channels)))
}

pub(crate) fn materialized_channel_records_from_db_source(
    db: &Database,
    app: Option<&AppKind>,
) -> ProxyCoreResult<Vec<ChannelRecord>> {
    let channels = match app {
        Some(app) => db
            .list_proxy_channels_for_app(app.as_str())
            .map_err(|error| app_error("list materialized channel records", error))?,
        None => db
            .list_all_proxy_channels()
            .map_err(|error| app_error("list materialized channel records", error))?,
    };
    Ok(proxy_channel_records_to_core(channels))
}

pub(crate) fn proxy_channel_key_record_to_core(key: ProxyChannelKeyRecord) -> ChannelKeyRecord {
    channel_key_record_from_input(ChannelKeyRecordInput {
        channel_id: key.channel_id,
        key_ref: key.key_ref,
        status: key.status,
        priority: key.priority,
        weight: key.weight,
        last_failure_at: key.last_failure_at,
    })
}

pub(crate) fn proxy_channel_key_records_to_core(
    keys: Vec<ProxyChannelKeyRecord>,
) -> Vec<ChannelKeyRecord> {
    keys.into_iter()
        .map(proxy_channel_key_record_to_core)
        .collect()
}

pub(crate) fn channel_key_records_from_db_source(
    db: &Database,
    channel_id: &str,
) -> ProxyCoreResult<Option<Vec<ChannelKeyRecord>>> {
    let keys = db
        .list_proxy_channel_keys(channel_id)
        .map_err(|error| app_error("list channel key records", error))?;
    Ok(keys.map(proxy_channel_key_records_to_core))
}

pub(crate) fn upsert_channel_key_record_from_db_source(
    db: &Database,
    channel_id: &str,
    key_ref: &str,
    request: ProxyChannelKeyWriteRequest,
) -> ProxyCoreResult<Option<ChannelKeyRecord>> {
    let key = db
        .upsert_proxy_channel_key(channel_id, key_ref, request)
        .map_err(|error| app_write_error("upsert channel key record", error))?;
    Ok(Some(proxy_channel_key_record_to_core(key)))
}

pub(crate) fn update_channel_key_record_from_db_source(
    db: &Database,
    channel_id: &str,
    key_ref: &str,
    patch: ProxyChannelKeyPatchRequest,
) -> ProxyCoreResult<Option<ChannelKeyRecord>> {
    let key = db
        .update_proxy_channel_key(channel_id, key_ref, patch)
        .map_err(|error| app_write_error("update channel key record", error))?;
    Ok(key.map(proxy_channel_key_record_to_core))
}

pub(crate) fn delete_channel_key_record_from_db_source(
    db: &Database,
    channel_id: &str,
    key_ref: &str,
) -> ProxyCoreResult<bool> {
    db.delete_proxy_channel_key(channel_id, key_ref)
        .map_err(|error| app_error("delete channel key record", error))
}

pub(crate) fn channel_migration_preview_input_from_result(
    preview: ProxyChannelMigrationPreview,
) -> ChannelMigrationPreviewInput<ChannelRecord> {
    ChannelMigrationPreviewInput::new(
        preview.app_type,
        proxy_channel_records_to_core(preview.channels),
        preview.duplicate_count,
        preview.needs_review_count,
    )
}

pub(crate) fn channel_migration_preview_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<ChannelMigrationPreviewInput<ChannelRecord>> {
    let preview = db
        .preview_legacy_proxy_channel_migration(app.as_str())
        .map_err(|error| app_error("preview legacy channel migration", error))?;
    Ok(channel_migration_preview_input_from_result(preview))
}

pub(crate) fn channel_migration_materialize_input_from_result(
    result: ProxyChannelMaterializeResult,
) -> ChannelMigrationMaterializeInput {
    ChannelMigrationMaterializeInput::new(
        result.app_type,
        result.previewed_channels,
        result.inserted_channels,
        result.inserted_models,
        result.inserted_health_rows,
        result.duplicate_count,
        result.needs_review_count,
    )
}

pub(crate) fn channel_migration_materialize_from_db_source(
    db: &Database,
    app: &AppKind,
) -> ProxyCoreResult<ChannelMigrationMaterializeInput> {
    let result = db
        .materialize_legacy_proxy_channels(app.as_str())
        .map_err(|error| app_error("materialize legacy channel migration", error))?;
    Ok(channel_migration_materialize_input_from_result(result))
}

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
