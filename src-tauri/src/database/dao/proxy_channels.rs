//! Proxy channel migration DAO.
//!
//! The first migration step is intentionally additive: project existing
//! providers and provider_endpoints into channel-shaped records without
//! changing the current provider router or forwarding path.

use crate::app_config::AppType;
use crate::database::{lock_conn, to_json_string, Database};
use crate::error::AppError;
use crate::proxy_core_adapter::{
    channel_health_update_from_input, legacy_channel_migration_preview_from_providers,
    normalize_channel_base_url as normalize_base_url,
    normalize_proxy_channel_key_patch_request_fields,
    normalize_proxy_channel_key_write_request_fields,
    normalize_proxy_channel_model_write_request_fields,
    normalize_proxy_channel_models_replace_request_fields,
    normalize_proxy_channel_patch_request_fields, normalize_proxy_channel_write_request_fields,
    normalize_required_channel_string, stable_channel_id, ChannelHealthUpdateInput,
    ChannelRequestValidationError, ProxyChannelKeyPatchRequest, ProxyChannelKeyWriteRequest,
    ProxyChannelModelWriteRequest, ProxyChannelModelsReplaceRequest, ProxyChannelPatchRequest,
    ProxyChannelWriteRequest, CHANNEL_HEALTH_UNKNOWN_STATUS,
};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

const LEGACY_PRIMARY_SOURCE: &str = "legacy_primary";
const LEGACY_ENDPOINT_SOURCE: &str = "legacy_endpoint";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProxyChannelSourceKind {
    LegacyPrimary,
    LegacyEndpoint,
    Manual,
}

impl ProxyChannelSourceKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::LegacyPrimary => LEGACY_PRIMARY_SOURCE,
            Self::LegacyEndpoint => LEGACY_ENDPOINT_SOURCE,
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxyChannelModelRecord {
    pub channel_id: String,
    pub public_model: String,
    pub upstream_model: String,
    pub capabilities: Value,
    pub pricing_model: Option<String>,
    pub request_overrides: Value,
    pub response_overrides: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxyChannelRecord {
    pub id: String,
    pub provider_id: String,
    pub app_type: String,
    pub name: String,
    pub status: String,
    pub base_url: String,
    pub interface_kind: String,
    pub auth_profile_ref: Option<String>,
    pub groups: Vec<String>,
    pub priority: i64,
    pub weight: u32,
    pub retry_policy: Value,
    pub health_policy: Value,
    pub header_overrides: Value,
    pub param_overrides: Value,
    pub status_code_mapping: Value,
    pub tags: Vec<String>,
    pub metadata: Value,
    pub source_kind: ProxyChannelSourceKind,
    pub source_endpoint_url: Option<String>,
    pub models: Vec<ProxyChannelModelRecord>,
    pub needs_review: bool,
    pub review_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxyChannelMigrationPreview {
    pub app_type: String,
    pub channels: Vec<ProxyChannelRecord>,
    pub duplicate_count: usize,
    pub needs_review_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxyChannelMaterializeResult {
    pub app_type: String,
    pub previewed_channels: usize,
    pub inserted_channels: usize,
    pub inserted_models: usize,
    pub inserted_health_rows: usize,
    pub duplicate_count: usize,
    pub needs_review_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct ProxyChannelHealth {
    pub channel_id: String,
    pub status: String,
    pub last_success_at: Option<i64>,
    pub last_failure_at: Option<i64>,
    pub consecutive_failures: u32,
    pub response_time_ms: Option<i64>,
    pub disabled_reason: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxyChannelKeyRecord {
    pub channel_id: String,
    pub key_ref: String,
    #[serde(skip_serializing)]
    pub key_value: String,
    pub status: String,
    pub priority: i64,
    pub weight: u32,
    pub last_failure_at: Option<i64>,
}

impl Database {
    pub(crate) fn preview_legacy_proxy_channel_migration(
        &self,
        app_type: &str,
    ) -> Result<ProxyChannelMigrationPreview, AppError> {
        let providers = self.get_all_providers(app_type)?;
        let current_provider_id = self.get_current_provider(app_type)?;
        let app = AppType::from_str(app_type).ok();
        Ok(legacy_channel_migration_preview_from_providers(
            app_type,
            app.as_ref(),
            current_provider_id.as_deref(),
            providers.values(),
        ))
    }

    pub(crate) fn materialize_legacy_proxy_channels(
        &self,
        app_type: &str,
    ) -> Result<ProxyChannelMaterializeResult, AppError> {
        let preview = self.preview_legacy_proxy_channel_migration(app_type)?;
        let conn = lock_conn!(self.conn);
        let now = chrono::Utc::now().timestamp_millis();
        let mut inserted_channels = 0usize;
        let mut inserted_models = 0usize;
        let mut inserted_health_rows = 0usize;

        for channel in &preview.channels {
            let groups_json = to_json_string(&channel.groups)?;
            let retry_policy_json = to_json_string(&channel.retry_policy)?;
            let health_policy_json = to_json_string(&channel.health_policy)?;
            let header_override_json = to_json_string(&channel.header_overrides)?;
            let param_override_json = to_json_string(&channel.param_overrides)?;
            let status_code_mapping_json = to_json_string(&channel.status_code_mapping)?;
            let tags_json = to_json_string(&channel.tags)?;
            let metadata_json = to_json_string(&channel.metadata)?;

            inserted_channels += conn
                .execute(
                    "INSERT OR IGNORE INTO proxy_channels (
                        id, provider_id, app_type, name, status, base_url, interface_kind,
                        auth_profile_ref, groups_json, priority, weight, retry_policy_json,
                        health_policy_json, header_override_json, param_override_json,
                        status_code_mapping_json, tags_json, metadata_json, source_kind,
                        source_endpoint_url, created_at, updated_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                        ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22
                    )",
                    params![
                        channel.id,
                        channel.provider_id,
                        channel.app_type,
                        channel.name,
                        channel.status,
                        channel.base_url,
                        channel.interface_kind,
                        channel.auth_profile_ref,
                        groups_json,
                        channel.priority,
                        channel.weight,
                        retry_policy_json,
                        health_policy_json,
                        header_override_json,
                        param_override_json,
                        status_code_mapping_json,
                        tags_json,
                        metadata_json,
                        channel.source_kind.as_str(),
                        channel.source_endpoint_url,
                        now,
                        now,
                    ],
                )
                .map_err(|e| AppError::Database(format!("写入 proxy channel 失败: {e}")))?;

            inserted_health_rows += conn
                .execute(
                    "INSERT OR IGNORE INTO proxy_channel_health (
                        channel_id, status, consecutive_failures, updated_at
                    ) VALUES (?1, ?2, 0, ?3)",
                    params![channel.id, CHANNEL_HEALTH_UNKNOWN_STATUS, now],
                )
                .map_err(|e| AppError::Database(format!("写入 proxy channel health 失败: {e}")))?;

            for model in &channel.models {
                inserted_models += conn
                    .execute(
                        "INSERT OR IGNORE INTO proxy_channel_models (
                            channel_id, public_model, upstream_model, capabilities_json,
                            pricing_model, request_overrides_json, response_overrides_json,
                            created_at, updated_at
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![
                            model.channel_id,
                            model.public_model,
                            model.upstream_model,
                            to_json_string(&model.capabilities)?,
                            model.pricing_model,
                            to_json_string(&model.request_overrides)?,
                            to_json_string(&model.response_overrides)?,
                            now,
                            now,
                        ],
                    )
                    .map_err(|e| {
                        AppError::Database(format!("写入 proxy channel model 失败: {e}"))
                    })?;
            }
        }

        Ok(ProxyChannelMaterializeResult {
            app_type: preview.app_type,
            previewed_channels: preview.channels.len(),
            inserted_channels,
            inserted_models,
            inserted_health_rows,
            duplicate_count: preview.duplicate_count,
            needs_review_count: preview.needs_review_count,
        })
    }

    pub(crate) fn list_proxy_channels_for_app(
        &self,
        app_type: &str,
    ) -> Result<Vec<ProxyChannelRecord>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, provider_id, app_type, name, status, base_url, interface_kind,
                    auth_profile_ref, groups_json, priority, weight, retry_policy_json,
                    health_policy_json, header_override_json, param_override_json,
                    status_code_mapping_json, tags_json, metadata_json, source_kind,
                    source_endpoint_url
                 FROM proxy_channels
                 WHERE app_type = ?1
                 ORDER BY priority DESC, weight DESC, name ASC, id ASC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let rows = stmt
            .query_map([app_type], map_proxy_channel_row)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut channels = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;

        for channel in &mut channels {
            channel.models = list_proxy_channel_models_on_conn(&conn, &channel.id)?;
        }

        Ok(channels)
    }

    pub(crate) fn list_proxy_channel_models(
        &self,
        channel_id: &str,
    ) -> Result<Vec<ProxyChannelModelRecord>, AppError> {
        let conn = lock_conn!(self.conn);
        list_proxy_channel_models_on_conn(&conn, channel_id)
    }

    pub(crate) fn list_all_proxy_channels(&self) -> Result<Vec<ProxyChannelRecord>, AppError> {
        let conn = lock_conn!(self.conn);
        list_proxy_channels_on_conn(&conn, None)
    }

    pub(crate) fn get_proxy_channel(
        &self,
        channel_id: &str,
    ) -> Result<Option<ProxyChannelRecord>, AppError> {
        let conn = lock_conn!(self.conn);
        get_proxy_channel_on_conn(&conn, channel_id)
    }

    pub(crate) fn create_proxy_channel(
        &self,
        request: ProxyChannelWriteRequest,
    ) -> Result<ProxyChannelRecord, AppError> {
        let request = normalize_proxy_channel_write_request(request)?;
        if self
            .get_provider_by_id(&request.provider_id, &request.app_type)?
            .is_none()
        {
            return Err(AppError::InvalidInput(format!(
                "provider not found: {} ({})",
                request.provider_id, request.app_type
            )));
        }

        let conn = lock_conn!(self.conn);
        let now = chrono::Utc::now().timestamp_millis();
        let channel_id = request.id.clone().unwrap_or_else(|| {
            stable_channel_id(
                &request.app_type,
                &request.provider_id,
                ProxyChannelSourceKind::Manual.as_str(),
                &normalize_base_url(&request.base_url),
            )
        });

        conn.execute(
            "INSERT INTO proxy_channels (
                id, provider_id, app_type, name, status, base_url, interface_kind,
                auth_profile_ref, groups_json, priority, weight, retry_policy_json,
                health_policy_json, header_override_json, param_override_json,
                status_code_mapping_json, tags_json, metadata_json, source_kind,
                source_endpoint_url, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18, ?19, NULL, ?20, ?21
            )",
            params![
                channel_id,
                request.provider_id,
                request.app_type,
                request.name,
                request.status,
                request.base_url,
                request.interface_kind,
                request.auth_profile_ref,
                to_json_string(&request.groups)?,
                request.priority,
                request.weight as i64,
                to_json_string(&request.retry_policy)?,
                to_json_string(&request.health_policy)?,
                to_json_string(&request.header_overrides)?,
                to_json_string(&request.param_overrides)?,
                to_json_string(&request.status_code_mapping)?,
                to_json_string(&request.tags)?,
                to_json_string(&request.metadata)?,
                ProxyChannelSourceKind::Manual.as_str(),
                now,
                now,
            ],
        )
        .map_err(|e| AppError::Database(format!("创建 proxy channel 失败: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO proxy_channel_health (
                channel_id, status, consecutive_failures, updated_at
            ) VALUES (?1, ?2, 0, ?3)",
            params![channel_id, CHANNEL_HEALTH_UNKNOWN_STATUS, now],
        )
        .map_err(|e| AppError::Database(format!("创建 proxy channel health 失败: {e}")))?;

        replace_proxy_channel_models_on_conn(&conn, &channel_id, request.models)?;
        get_proxy_channel_on_conn(&conn, &channel_id)?
            .ok_or_else(|| AppError::Database("created channel could not be reloaded".to_string()))
    }

    pub(crate) fn update_proxy_channel(
        &self,
        channel_id: &str,
        patch: ProxyChannelPatchRequest,
    ) -> Result<Option<ProxyChannelRecord>, AppError> {
        let conn = lock_conn!(self.conn);
        let Some(mut current) = get_proxy_channel_on_conn(&conn, channel_id)? else {
            return Ok(None);
        };
        let auth_profile_ref_present = patch.auth_profile_ref.is_some();
        let patch = normalize_proxy_channel_patch_request_fields(patch)
            .map_err(channel_request_error_to_app_error)?;

        if let Some(name) = patch.name {
            current.name = name;
        }
        if let Some(status) = patch.status {
            current.status = status;
        }
        if let Some(base_url) = patch.base_url {
            current.base_url = base_url;
        }
        if let Some(interface_kind) = patch.interface_kind {
            current.interface_kind = interface_kind;
        }
        if auth_profile_ref_present {
            current.auth_profile_ref = patch.auth_profile_ref;
        }
        if let Some(groups) = patch.groups {
            current.groups = groups;
        }
        if let Some(priority) = patch.priority {
            current.priority = priority;
        }
        if let Some(weight) = patch.weight {
            current.weight = weight;
        }
        if let Some(retry_policy) = patch.retry_policy {
            current.retry_policy = retry_policy;
        }
        if let Some(health_policy) = patch.health_policy {
            current.health_policy = health_policy;
        }
        if let Some(header_overrides) = patch.header_overrides {
            current.header_overrides = header_overrides;
        }
        if let Some(param_overrides) = patch.param_overrides {
            current.param_overrides = param_overrides;
        }
        if let Some(status_code_mapping) = patch.status_code_mapping {
            current.status_code_mapping = status_code_mapping;
        }
        if let Some(tags) = patch.tags {
            current.tags = tags;
        }
        if let Some(metadata) = patch.metadata {
            current.metadata = metadata;
        }

        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE proxy_channels SET
                name = ?1,
                status = ?2,
                base_url = ?3,
                interface_kind = ?4,
                auth_profile_ref = ?5,
                groups_json = ?6,
                priority = ?7,
                weight = ?8,
                retry_policy_json = ?9,
                health_policy_json = ?10,
                header_override_json = ?11,
                param_override_json = ?12,
                status_code_mapping_json = ?13,
                tags_json = ?14,
                metadata_json = ?15,
                updated_at = ?16
             WHERE id = ?17",
            params![
                current.name,
                current.status,
                current.base_url,
                current.interface_kind,
                current.auth_profile_ref,
                to_json_string(&current.groups)?,
                current.priority,
                current.weight as i64,
                to_json_string(&current.retry_policy)?,
                to_json_string(&current.health_policy)?,
                to_json_string(&current.header_overrides)?,
                to_json_string(&current.param_overrides)?,
                to_json_string(&current.status_code_mapping)?,
                to_json_string(&current.tags)?,
                to_json_string(&current.metadata)?,
                now,
                channel_id,
            ],
        )
        .map_err(|e| AppError::Database(format!("更新 proxy channel 失败: {e}")))?;

        get_proxy_channel_on_conn(&conn, channel_id)
    }

    pub(crate) fn delete_proxy_channel(&self, channel_id: &str) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let deleted = conn
            .execute("DELETE FROM proxy_channels WHERE id = ?1", [channel_id])
            .map_err(|e| AppError::Database(format!("删除 proxy channel 失败: {e}")))?;
        Ok(deleted > 0)
    }

    pub(crate) fn replace_proxy_channel_models(
        &self,
        channel_id: &str,
        request: ProxyChannelModelsReplaceRequest,
    ) -> Result<Option<Vec<ProxyChannelModelRecord>>, AppError> {
        let request = normalize_proxy_channel_models_replace_request_fields(request)
            .map_err(channel_request_error_to_app_error)?;
        let conn = lock_conn!(self.conn);
        if get_proxy_channel_on_conn(&conn, channel_id)?.is_none() {
            return Ok(None);
        }
        replace_proxy_channel_models_on_conn(&conn, channel_id, request.models)?;
        Ok(Some(list_proxy_channel_models_on_conn(&conn, channel_id)?))
    }

    pub(crate) fn upsert_proxy_channel_key(
        &self,
        channel_id: &str,
        key_ref: &str,
        request: ProxyChannelKeyWriteRequest,
    ) -> Result<ProxyChannelKeyRecord, AppError> {
        let request = normalize_proxy_channel_key_write_request_fields(request)
            .map_err(channel_request_error_to_app_error)?;
        let conn = lock_conn!(self.conn);
        if get_proxy_channel_on_conn(&conn, channel_id)?.is_none() {
            return Err(AppError::InvalidInput(format!(
                "channel not found: {channel_id}"
            )));
        }

        let key_ref = normalize_required_string(key_ref, "keyRef")?;
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO proxy_channel_keys (
                channel_id, key_ref, key_value, status, priority, weight, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(channel_id, key_ref) DO UPDATE SET
                key_value = excluded.key_value,
                status = excluded.status,
                priority = excluded.priority,
                weight = excluded.weight,
                updated_at = excluded.updated_at",
            params![
                channel_id,
                &key_ref,
                &request.key_value,
                &request.status,
                request.priority,
                request.weight as i64,
                now,
                now,
            ],
        )
        .map_err(|e| AppError::Database(format!("写入 proxy channel key 失败: {e}")))?;

        get_proxy_channel_key_on_conn(&conn, channel_id, &key_ref)?
            .ok_or_else(|| AppError::Database("channel key could not be reloaded".to_string()))
    }

    pub(crate) fn list_proxy_channel_keys(
        &self,
        channel_id: &str,
    ) -> Result<Option<Vec<ProxyChannelKeyRecord>>, AppError> {
        let conn = lock_conn!(self.conn);
        if get_proxy_channel_on_conn(&conn, channel_id)?.is_none() {
            return Ok(None);
        }
        Ok(Some(list_proxy_channel_keys_on_conn(&conn, channel_id)?))
    }

    pub(crate) fn list_proxy_channel_key_runtime_candidates(
        &self,
        channel_id: &str,
    ) -> Result<Option<Vec<ProxyChannelKeyRecord>>, AppError> {
        let conn = lock_conn!(self.conn);
        if get_proxy_channel_on_conn(&conn, channel_id)?.is_none() {
            return Ok(None);
        }
        Ok(Some(list_proxy_channel_key_runtime_candidates_on_conn(
            &conn, channel_id,
        )?))
    }

    #[cfg(test)]
    pub(crate) fn get_proxy_channel_key(
        &self,
        channel_id: &str,
        key_ref: &str,
    ) -> Result<Option<ProxyChannelKeyRecord>, AppError> {
        let conn = lock_conn!(self.conn);
        get_proxy_channel_key_on_conn(&conn, channel_id, key_ref)
    }

    pub(crate) fn update_proxy_channel_key(
        &self,
        channel_id: &str,
        key_ref: &str,
        patch: ProxyChannelKeyPatchRequest,
    ) -> Result<Option<ProxyChannelKeyRecord>, AppError> {
        let patch = normalize_proxy_channel_key_patch_request_fields(patch)
            .map_err(channel_request_error_to_app_error)?;
        let conn = lock_conn!(self.conn);
        let Some(mut current) = get_proxy_channel_key_on_conn(&conn, channel_id, key_ref)? else {
            return Ok(None);
        };

        if let Some(key_value) = patch.key_value {
            current.key_value = key_value;
        }
        if let Some(status) = patch.status {
            current.status = status;
        }
        if let Some(priority) = patch.priority {
            current.priority = priority;
        }
        if let Some(weight) = patch.weight {
            current.weight = weight;
        }

        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE proxy_channel_keys SET
                key_value = ?1,
                status = ?2,
                priority = ?3,
                weight = ?4,
                updated_at = ?5
             WHERE channel_id = ?6 AND key_ref = ?7",
            params![
                current.key_value,
                current.status,
                current.priority,
                current.weight as i64,
                now,
                channel_id,
                key_ref,
            ],
        )
        .map_err(|e| AppError::Database(format!("更新 proxy channel key 失败: {e}")))?;

        get_proxy_channel_key_on_conn(&conn, channel_id, key_ref)
    }

    pub(crate) fn delete_proxy_channel_key(
        &self,
        channel_id: &str,
        key_ref: &str,
    ) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let deleted = conn
            .execute(
                "DELETE FROM proxy_channel_keys WHERE channel_id = ?1 AND key_ref = ?2",
                params![channel_id, key_ref],
            )
            .map_err(|e| AppError::Database(format!("删除 proxy channel key 失败: {e}")))?;
        Ok(deleted > 0)
    }

    pub(crate) fn get_proxy_channel_app_type(
        &self,
        channel_id: &str,
    ) -> Result<Option<String>, AppError> {
        let conn = lock_conn!(self.conn);
        let result = conn.query_row(
            "SELECT app_type FROM proxy_channels WHERE id = ?1",
            [channel_id],
            |row| row.get(0),
        );

        match result {
            Ok(app_type) => Ok(Some(app_type)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn get_proxy_channel_health(
        &self,
        channel_id: &str,
    ) -> Result<ProxyChannelHealth, AppError> {
        let conn = lock_conn!(self.conn);
        let result = conn.query_row(
            "SELECT channel_id, status, last_success_at, last_failure_at,
                    consecutive_failures, response_time_ms, disabled_reason, updated_at
             FROM proxy_channel_health
             WHERE channel_id = ?1",
            [channel_id],
            |row| {
                Ok(ProxyChannelHealth {
                    channel_id: row.get(0)?,
                    status: row.get(1)?,
                    last_success_at: row.get(2)?,
                    last_failure_at: row.get(3)?,
                    consecutive_failures: row.get::<_, i64>(4)?.max(0) as u32,
                    response_time_ms: row.get(5)?,
                    disabled_reason: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        );

        match result {
            Ok(health) => Ok(health),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ProxyChannelHealth {
                channel_id: channel_id.to_string(),
                status: CHANNEL_HEALTH_UNKNOWN_STATUS.to_string(),
                last_success_at: None,
                last_failure_at: None,
                consecutive_failures: 0,
                response_time_ms: None,
                disabled_reason: None,
                updated_at: chrono::Utc::now().timestamp_millis(),
            }),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    pub(crate) fn update_proxy_channel_health_with_threshold(
        &self,
        channel_id: &str,
        success: bool,
        error_msg: Option<String>,
        failure_threshold: u32,
        response_time_ms: Option<i64>,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        let now = chrono::Utc::now().timestamp_millis();
        let current_failures = conn
            .query_row(
                "SELECT consecutive_failures FROM proxy_channel_health WHERE channel_id = ?1",
                [channel_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            .max(0) as u32;

        let update = channel_health_update_from_input(ChannelHealthUpdateInput {
            current_consecutive_failures: current_failures,
            success,
            error_msg,
            failure_threshold,
            timestamp_ms: now,
        });

        conn.execute(
            "INSERT OR REPLACE INTO proxy_channel_health (
                channel_id, status, last_success_at, last_failure_at,
                consecutive_failures, response_time_ms, disabled_reason, updated_at
            ) VALUES (
                ?1, ?2,
                COALESCE(?3, (SELECT last_success_at FROM proxy_channel_health WHERE channel_id = ?1)),
                COALESCE(?4, (SELECT last_failure_at FROM proxy_channel_health WHERE channel_id = ?1)),
                ?5,
                COALESCE(?6, (SELECT response_time_ms FROM proxy_channel_health WHERE channel_id = ?1)),
                ?7, ?8
            )",
            params![
                channel_id,
                update.status,
                update.last_success_at,
                update.last_failure_at,
                update.consecutive_failures as i64,
                response_time_ms,
                update.disabled_reason,
                now,
            ],
        )
        .map_err(|e| AppError::Database(format!("更新 proxy channel health 失败: {e}")))?;

        Ok(())
    }

    pub(crate) fn reset_proxy_channel_health(&self, channel_id: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM proxy_channel_health WHERE channel_id = ?1",
            [channel_id],
        )
        .map_err(|e| AppError::Database(format!("重置 proxy channel health 失败: {e}")))?;
        Ok(())
    }
}

fn get_proxy_channel_key_on_conn(
    conn: &Connection,
    channel_id: &str,
    key_ref: &str,
) -> Result<Option<ProxyChannelKeyRecord>, AppError> {
    conn.query_row(
        "SELECT channel_id, key_ref, key_value, status, priority, weight, last_failure_at
         FROM proxy_channel_keys
         WHERE channel_id = ?1 AND key_ref = ?2",
        params![channel_id, key_ref],
        |row| {
            Ok(ProxyChannelKeyRecord {
                channel_id: row.get(0)?,
                key_ref: row.get(1)?,
                key_value: row.get(2)?,
                status: row.get(3)?,
                priority: row.get(4)?,
                weight: row.get::<_, i64>(5)?.max(0) as u32,
                last_failure_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| AppError::Database(e.to_string()))
}

fn list_proxy_channel_keys_on_conn(
    conn: &Connection,
    channel_id: &str,
) -> Result<Vec<ProxyChannelKeyRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT channel_id, key_ref, status, priority, weight, last_failure_at
             FROM proxy_channel_keys
             WHERE channel_id = ?1
             ORDER BY priority DESC, weight DESC, key_ref ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([channel_id], |row| {
            Ok(ProxyChannelKeyRecord {
                channel_id: row.get(0)?,
                key_ref: row.get(1)?,
                key_value: String::new(),
                status: row.get(2)?,
                priority: row.get(3)?,
                weight: row.get::<_, i64>(4)?.max(0) as u32,
                last_failure_at: row.get(5)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))
}

fn list_proxy_channel_key_runtime_candidates_on_conn(
    conn: &Connection,
    channel_id: &str,
) -> Result<Vec<ProxyChannelKeyRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT channel_id, key_ref, key_value, status, priority, weight, last_failure_at
             FROM proxy_channel_keys
             WHERE channel_id = ?1
             ORDER BY priority DESC, weight DESC, key_ref ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([channel_id], |row| {
            Ok(ProxyChannelKeyRecord {
                channel_id: row.get(0)?,
                key_ref: row.get(1)?,
                key_value: row.get(2)?,
                status: row.get(3)?,
                priority: row.get(4)?,
                weight: row.get::<_, i64>(5)?.max(0) as u32,
                last_failure_at: row.get(6)?,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))
}

fn list_proxy_channel_models_on_conn(
    conn: &Connection,
    channel_id: &str,
) -> Result<Vec<ProxyChannelModelRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT channel_id, public_model, upstream_model, capabilities_json,
                    pricing_model, request_overrides_json, response_overrides_json
                 FROM proxy_channel_models
                 WHERE channel_id = ?1
                 ORDER BY public_model ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([channel_id], |row| {
            Ok(ProxyChannelModelRecord {
                channel_id: row.get(0)?,
                public_model: row.get(1)?,
                upstream_model: row.get(2)?,
                capabilities: parse_json_or_default(row.get::<_, String>(3)?.as_str()),
                pricing_model: row.get(4)?,
                request_overrides: parse_json_or_default(row.get::<_, String>(5)?.as_str()),
                response_overrides: parse_json_or_default(row.get::<_, String>(6)?.as_str()),
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))
}

fn list_proxy_channels_on_conn(
    conn: &Connection,
    app_type: Option<&str>,
) -> Result<Vec<ProxyChannelRecord>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, provider_id, app_type, name, status, base_url, interface_kind,
                    auth_profile_ref, groups_json, priority, weight, retry_policy_json,
                    health_policy_json, header_override_json, param_override_json,
                    status_code_mapping_json, tags_json, metadata_json, source_kind,
                    source_endpoint_url
                 FROM proxy_channels
                 WHERE (?1 IS NULL OR app_type = ?1)
                 ORDER BY app_type ASC, priority DESC, weight DESC, name ASC, id ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![app_type], map_proxy_channel_row)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut channels = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Database(e.to_string()))?;

    for channel in &mut channels {
        channel.models = list_proxy_channel_models_on_conn(conn, &channel.id)?;
    }

    Ok(channels)
}

fn get_proxy_channel_on_conn(
    conn: &Connection,
    channel_id: &str,
) -> Result<Option<ProxyChannelRecord>, AppError> {
    let mut channel = conn
        .query_row(
            "SELECT id, provider_id, app_type, name, status, base_url, interface_kind,
                    auth_profile_ref, groups_json, priority, weight, retry_policy_json,
                    health_policy_json, header_override_json, param_override_json,
                    status_code_mapping_json, tags_json, metadata_json, source_kind,
                    source_endpoint_url
                 FROM proxy_channels
                 WHERE id = ?1",
            [channel_id],
            map_proxy_channel_row,
        )
        .optional()
        .map_err(|e| AppError::Database(e.to_string()))?;

    if let Some(channel) = &mut channel {
        channel.models = list_proxy_channel_models_on_conn(conn, &channel.id)?;
    }

    Ok(channel)
}

fn map_proxy_channel_row(row: &Row<'_>) -> rusqlite::Result<ProxyChannelRecord> {
    let source_kind = match row.get::<_, String>(18)?.as_str() {
        LEGACY_PRIMARY_SOURCE => ProxyChannelSourceKind::LegacyPrimary,
        LEGACY_ENDPOINT_SOURCE => ProxyChannelSourceKind::LegacyEndpoint,
        _ => ProxyChannelSourceKind::Manual,
    };

    Ok(ProxyChannelRecord {
        id: row.get(0)?,
        provider_id: row.get(1)?,
        app_type: row.get(2)?,
        name: row.get(3)?,
        status: row.get(4)?,
        base_url: row.get(5)?,
        interface_kind: row.get(6)?,
        auth_profile_ref: row.get(7)?,
        groups: parse_json_or_default(row.get::<_, String>(8)?.as_str()),
        priority: row.get(9)?,
        weight: row.get::<_, i64>(10)?.max(0) as u32,
        retry_policy: parse_json_or_default(row.get::<_, String>(11)?.as_str()),
        health_policy: parse_json_or_default(row.get::<_, String>(12)?.as_str()),
        header_overrides: parse_json_or_default(row.get::<_, String>(13)?.as_str()),
        param_overrides: parse_json_or_default(row.get::<_, String>(14)?.as_str()),
        status_code_mapping: parse_json_or_default(row.get::<_, String>(15)?.as_str()),
        tags: parse_json_or_default(row.get::<_, String>(16)?.as_str()),
        metadata: parse_json_or_default(row.get::<_, String>(17)?.as_str()),
        source_kind,
        source_endpoint_url: row.get(19)?,
        models: Vec::new(),
        needs_review: false,
        review_reasons: Vec::new(),
    })
}

fn replace_proxy_channel_models_on_conn(
    conn: &Connection,
    channel_id: &str,
    models: Vec<ProxyChannelModelWriteRequest>,
) -> Result<(), AppError> {
    let models = models
        .into_iter()
        .map(normalize_proxy_channel_model_write_request_fields)
        .collect::<Result<Vec<_>, _>>()
        .map_err(channel_request_error_to_app_error)?;

    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "DELETE FROM proxy_channel_models WHERE channel_id = ?1",
        [channel_id],
    )
    .map_err(|e| AppError::Database(format!("删除 proxy channel models 失败: {e}")))?;

    for model in models {
        conn.execute(
            "INSERT INTO proxy_channel_models (
                channel_id, public_model, upstream_model, capabilities_json,
                pricing_model, request_overrides_json, response_overrides_json,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                channel_id,
                model.public_model,
                model.upstream_model,
                to_json_string(&model.capabilities)?,
                model.pricing_model,
                to_json_string(&model.request_overrides)?,
                to_json_string(&model.response_overrides)?,
                now,
                now,
            ],
        )
        .map_err(|e| AppError::Database(format!("写入 proxy channel model 失败: {e}")))?;
    }

    Ok(())
}

fn normalize_proxy_channel_write_request(
    request: ProxyChannelWriteRequest,
) -> Result<ProxyChannelWriteRequest, AppError> {
    let _ = AppType::from_str(&request.app_type)?;
    normalize_proxy_channel_write_request_fields(request)
        .map_err(channel_request_error_to_app_error)
}

fn normalize_required_string(value: &str, field: &str) -> Result<String, AppError> {
    normalize_required_channel_string(value, field).map_err(channel_request_error_to_app_error)
}

fn channel_request_error_to_app_error(error: ChannelRequestValidationError) -> AppError {
    AppError::InvalidInput(error.message)
}

fn parse_json_or_default<T>(value: &str) -> T
where
    T: serde::de::DeserializeOwned + Default,
{
    serde_json::from_str(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ClaudeDesktopModelRoute, Provider, ProviderMeta};
    use crate::settings::CustomEndpoint;
    use serde_json::json;
    use std::collections::HashMap;

    fn save_claude_provider(db: &Database) {
        let mut custom_endpoints = HashMap::new();
        custom_endpoints.insert(
            "https://relay-b.example.com/v1/".to_string(),
            CustomEndpoint {
                url: "https://relay-b.example.com/v1/".to_string(),
                added_at: 2,
                last_used: None,
            },
        );
        custom_endpoints.insert(
            "https://relay-a.example.com/v1".to_string(),
            CustomEndpoint {
                url: "https://relay-a.example.com/v1".to_string(),
                added_at: 1,
                last_used: None,
            },
        );

        let mut routes = HashMap::new();
        routes.insert(
            "sonnet-safe".to_string(),
            ClaudeDesktopModelRoute {
                model: "claude-sonnet-4".to_string(),
                label_override: Some("Sonnet".to_string()),
                supports_1m: None,
            },
        );

        let mut provider = Provider::with_id(
            "anthropic-main".to_string(),
            "Anthropic Main".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay-a.example.com/v1/",
                    "ANTHROPIC_MODEL": "claude-sonnet-4",
                    "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku-4"
                }
            }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            api_format: Some("openai_responses".to_string()),
            custom_endpoints,
            claude_desktop_model_routes: routes,
            ..ProviderMeta::default()
        });
        db.save_provider("claude", &provider)
            .expect("save provider");
        db.set_current_provider("claude", "anthropic-main")
            .expect("set current");
    }

    #[test]
    fn preview_projects_primary_and_unique_legacy_endpoints() {
        let db = Database::memory().expect("memory db");
        save_claude_provider(&db);

        let preview = db
            .preview_legacy_proxy_channel_migration("claude")
            .expect("preview");

        assert_eq!(preview.channels.len(), 2);
        assert_eq!(preview.duplicate_count, 1);
        assert_eq!(preview.needs_review_count, 0);

        let primary = &preview.channels[0];
        assert_eq!(primary.provider_id, "anthropic-main");
        assert_eq!(primary.source_kind, ProxyChannelSourceKind::LegacyPrimary);
        assert_eq!(primary.base_url, "https://relay-a.example.com/v1");
        assert_eq!(primary.interface_kind, "openai_responses");
        assert_eq!(primary.priority, 100);
        assert_eq!(primary.models.len(), 3);
        assert!(primary
            .models
            .iter()
            .any(|model| model.public_model == "sonnet-safe"
                && model.upstream_model == "claude-sonnet-4"));

        let endpoint = &preview.channels[1];
        assert_eq!(endpoint.source_kind, ProxyChannelSourceKind::LegacyEndpoint);
        assert_eq!(endpoint.base_url, "https://relay-b.example.com/v1");
    }

    #[test]
    fn materialize_is_idempotent() {
        let db = Database::memory().expect("memory db");
        save_claude_provider(&db);

        let first = db
            .materialize_legacy_proxy_channels("claude")
            .expect("first materialize");
        assert_eq!(first.previewed_channels, 2);
        assert_eq!(first.inserted_channels, 2);
        assert_eq!(first.inserted_health_rows, 2);
        assert_eq!(first.inserted_models, 6);

        let second = db
            .materialize_legacy_proxy_channels("claude")
            .expect("second materialize");
        assert_eq!(second.inserted_channels, 0);
        assert_eq!(second.inserted_health_rows, 0);
        assert_eq!(second.inserted_models, 0);

        let stored = db
            .list_proxy_channels_for_app("claude")
            .expect("list channels");
        assert_eq!(stored.len(), 2);
        assert!(stored.iter().all(|channel| !channel.models.is_empty()));

        let direct_models = db
            .list_proxy_channel_models(&stored[0].id)
            .expect("list channel models");
        assert_eq!(direct_models.len(), stored[0].models.len());

        assert_eq!(
            db.get_proxy_channel_app_type(&stored[0].id)
                .expect("channel app type")
                .as_deref(),
            Some("claude")
        );
        assert!(db
            .get_proxy_channel_app_type("missing-channel")
            .expect("missing channel app type")
            .is_none());
    }

    #[test]
    fn channel_health_tracks_threshold_and_reset() {
        let db = Database::memory().expect("memory db");
        save_claude_provider(&db);
        db.materialize_legacy_proxy_channels("claude")
            .expect("materialize");

        let stored = db
            .list_proxy_channels_for_app("claude")
            .expect("list channels");
        let channel_id = &stored[0].id;

        let initial = db
            .get_proxy_channel_health(channel_id)
            .expect("initial health");
        assert_eq!(initial.status, "unknown");
        assert_eq!(initial.consecutive_failures, 0);

        db.update_proxy_channel_health_with_threshold(
            channel_id,
            false,
            Some("first failure".to_string()),
            2,
            Some(120),
        )
        .expect("record first failure");
        let degraded = db
            .get_proxy_channel_health(channel_id)
            .expect("degraded health");
        assert_eq!(degraded.status, "degraded");
        assert_eq!(degraded.consecutive_failures, 1);
        assert_eq!(degraded.response_time_ms, Some(120));
        assert_eq!(degraded.disabled_reason.as_deref(), Some("first failure"));

        db.update_proxy_channel_health_with_threshold(
            channel_id,
            false,
            Some("second failure".to_string()),
            2,
            None,
        )
        .expect("record second failure");
        let unhealthy = db
            .get_proxy_channel_health(channel_id)
            .expect("unhealthy health");
        assert_eq!(unhealthy.status, "unhealthy");
        assert_eq!(unhealthy.consecutive_failures, 2);
        assert_eq!(unhealthy.response_time_ms, Some(120));

        db.update_proxy_channel_health_with_threshold(channel_id, true, None, 2, Some(45))
            .expect("record success");
        let healthy = db
            .get_proxy_channel_health(channel_id)
            .expect("healthy health");
        assert_eq!(healthy.status, "healthy");
        assert_eq!(healthy.consecutive_failures, 0);
        assert_eq!(healthy.response_time_ms, Some(45));
        assert!(healthy.last_success_at.is_some());

        db.reset_proxy_channel_health(channel_id)
            .expect("reset health");
        let reset = db
            .get_proxy_channel_health(channel_id)
            .expect("reset health");
        assert_eq!(reset.status, "unknown");
        assert_eq!(reset.consecutive_failures, 0);
    }

    #[test]
    fn manual_channel_crud_and_model_replacement() {
        let db = Database::memory().expect("memory db");
        save_claude_provider(&db);

        let created = db
            .create_proxy_channel(ProxyChannelWriteRequest {
                provider_id: "anthropic-main".to_string(),
                app_type: "claude".to_string(),
                name: "Manual Relay".to_string(),
                base_url: "https://manual.example.com/v1/".to_string(),
                interface_kind: "openai_responses".to_string(),
                auth_profile_ref: Some(" channel-key:primary ".to_string()),
                priority: 77,
                models: vec![ProxyChannelModelWriteRequest {
                    public_model: "sonnet-public".to_string(),
                    upstream_model: "upstream-sonnet".to_string(),
                    capabilities: json!({"tools": true}),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .expect("create manual channel");

        assert_eq!(created.source_kind, ProxyChannelSourceKind::Manual);
        assert_eq!(created.base_url, "https://manual.example.com/v1");
        assert_eq!(
            created.auth_profile_ref.as_deref(),
            Some("channel-key:primary")
        );
        assert_eq!(created.groups, vec!["default".to_string()]);
        assert_eq!(created.models.len(), 1);

        let listed = db.list_all_proxy_channels().expect("list all channels");
        assert_eq!(listed.len(), 1);

        let patched = db
            .update_proxy_channel(
                &created.id,
                ProxyChannelPatchRequest {
                    name: Some("Manual Relay Updated".to_string()),
                    status: Some("disabled".to_string()),
                    groups: Some(vec!["beta".to_string(), "default".to_string()]),
                    weight: Some(25),
                    metadata: Some(json!({"owner": "ops"})),
                    ..Default::default()
                },
            )
            .expect("patch channel")
            .expect("patched channel");

        assert_eq!(patched.name, "Manual Relay Updated");
        assert_eq!(patched.status, "disabled");
        assert_eq!(patched.weight, 25);
        assert_eq!(patched.metadata["owner"], "ops");

        let invalid_auth_profile = db
            .update_proxy_channel(
                &created.id,
                ProxyChannelPatchRequest {
                    auth_profile_ref: Some("channel-key: ".to_string()),
                    ..Default::default()
                },
            )
            .expect_err("reject empty channel key auth profile");
        assert!(matches!(
            invalid_auth_profile,
            AppError::InvalidInput(message)
                if message == "authProfileRef must be provider:<app>:<providerId> or channel-key:<keyRef>"
        ));

        let cleared_auth_profile = db
            .update_proxy_channel(
                &created.id,
                ProxyChannelPatchRequest {
                    auth_profile_ref: Some(" ".to_string()),
                    ..Default::default()
                },
            )
            .expect("clear auth profile")
            .expect("patched channel");
        assert_eq!(cleared_auth_profile.auth_profile_ref, None);

        let replaced_models = db
            .replace_proxy_channel_models(
                &created.id,
                ProxyChannelModelsReplaceRequest {
                    models: vec![ProxyChannelModelWriteRequest {
                        public_model: "haiku-public".to_string(),
                        upstream_model: "upstream-haiku".to_string(),
                        ..Default::default()
                    }],
                },
            )
            .expect("replace models")
            .expect("models after replace");
        assert_eq!(replaced_models.len(), 1);
        assert_eq!(replaced_models[0].public_model, "haiku-public");

        assert!(db
            .delete_proxy_channel(&created.id)
            .expect("delete channel"));
        assert!(db
            .get_proxy_channel(&created.id)
            .expect("get deleted channel")
            .is_none());
        assert!(!db
            .delete_proxy_channel(&created.id)
            .expect("delete missing channel"));
    }

    #[test]
    fn channel_key_storage_reads_enabled_keys_without_serializing_secret() {
        let db = Database::memory().expect("memory db");
        save_claude_provider(&db);
        let created = db
            .create_proxy_channel(ProxyChannelWriteRequest {
                provider_id: "anthropic-main".to_string(),
                app_type: "claude".to_string(),
                name: "Manual Relay".to_string(),
                base_url: "https://manual-key.example.com/v1".to_string(),
                interface_kind: "anthropic_messages".to_string(),
                ..Default::default()
            })
            .expect("create manual channel");

        let key = db
            .upsert_proxy_channel_key(
                &created.id,
                "primary",
                ProxyChannelKeyWriteRequest {
                    key_value: " sk-channel-secret ".to_string(),
                    status: " enabled ".to_string(),
                    priority: 10,
                    weight: 80,
                },
            )
            .expect("upsert channel key");

        assert_eq!(key.key_value, "sk-channel-secret");
        assert_eq!(key.weight, 80);
        let stored_key = db
            .get_proxy_channel_key(&created.id, "primary")
            .expect("read stored key")
            .expect("stored key");
        assert_eq!(stored_key.status, "enabled");
        assert_eq!(stored_key.key_value, "sk-channel-secret");
        let serialized = serde_json::to_value(&key).expect("serialize key");
        assert!(serialized.get("keyValue").is_none());

        let listed = db
            .list_proxy_channel_keys(&created.id)
            .expect("list channel keys")
            .expect("channel exists");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].key_ref, "primary");
        assert_eq!(listed[0].key_value, "");
        assert_eq!(listed[0].status, "enabled");
        assert_eq!(listed[0].priority, 10);
        assert_eq!(listed[0].weight, 80);
        let listed_serialized = serde_json::to_value(&listed[0]).expect("serialize listed key");
        assert!(listed_serialized.get("keyValue").is_none());

        let runtime_candidates = db
            .list_proxy_channel_key_runtime_candidates(&created.id)
            .expect("list runtime channel keys")
            .expect("channel exists");
        assert_eq!(runtime_candidates.len(), 1);
        assert_eq!(runtime_candidates[0].key_ref, "primary");
        assert_eq!(runtime_candidates[0].key_value, "sk-channel-secret");
        let runtime_candidate_serialized =
            serde_json::to_value(&runtime_candidates[0]).expect("serialize runtime candidate");
        assert!(
            runtime_candidate_serialized.get("keyValue").is_none(),
            "runtime DB records should keep secret material in memory but not expose it through serde"
        );

        let patched = db
            .update_proxy_channel_key(
                &created.id,
                "primary",
                ProxyChannelKeyPatchRequest {
                    key_value: Some(" sk-rotated-secret ".to_string()),
                    status: Some(" disabled ".to_string()),
                    priority: Some(5),
                    weight: Some(20),
                },
            )
            .expect("patch channel key")
            .expect("patched channel key");
        assert_eq!(patched.key_value, "sk-rotated-secret");
        assert_eq!(patched.status, "disabled");
        assert_eq!(patched.priority, 5);
        assert_eq!(patched.weight, 20);
        let patched_stored_key = db
            .get_proxy_channel_key(&created.id, "primary")
            .expect("read patched key")
            .expect("patched key");
        assert_eq!(patched_stored_key.status, "disabled");

        assert!(db
            .list_proxy_channel_keys("missing-channel")
            .expect("list missing channel keys")
            .is_none());
        assert!(db
            .update_proxy_channel_key(
                &created.id,
                "missing",
                ProxyChannelKeyPatchRequest {
                    status: Some("disabled".to_string()),
                    ..Default::default()
                },
            )
            .expect("patch missing key")
            .is_none());

        assert!(db
            .delete_proxy_channel_key(&created.id, "primary")
            .expect("delete channel key"));
        assert!(!db
            .delete_proxy_channel_key(&created.id, "primary")
            .expect("delete missing channel key"));
        assert!(db
            .list_proxy_channel_keys(&created.id)
            .expect("list keys after delete")
            .expect("channel exists")
            .is_empty());

        db.upsert_proxy_channel_key(
            &created.id,
            "primary",
            ProxyChannelKeyWriteRequest {
                key_value: "sk-channel-secret".to_string(),
                status: "disabled".to_string(),
                priority: 10,
                weight: 80,
            },
        )
        .expect("disable channel key");
        let disabled_key = db
            .get_proxy_channel_key(&created.id, "primary")
            .expect("read disabled key")
            .expect("disabled key");
        assert_eq!(disabled_key.status, "disabled");
    }

    #[test]
    fn codex_projection_infers_wire_api_and_models() {
        let db = Database::memory().expect("memory db");
        let provider = Provider::with_id(
            "codex-relay".to_string(),
            "Codex Relay".to_string(),
            json!({
                "config": "model_provider = \"custom\"\nmodel = \"gpt-5.4\"\n\n[model_providers.custom]\nbase_url = \"https://codex.example.com/v1\"\nwire_api = \"chat\"\n",
                "modelCatalog": {
                    "models": [
                        { "model": "gpt-5.4" },
                        { "model": "gpt-5.4-mini" }
                    ]
                }
            }),
            None,
        );
        db.save_provider("codex", &provider).expect("save provider");

        let preview = db
            .preview_legacy_proxy_channel_migration("codex")
            .expect("preview");
        assert_eq!(preview.channels.len(), 1);
        let channel = &preview.channels[0];
        assert_eq!(channel.interface_kind, "openai_chat_completions");
        assert_eq!(channel.base_url, "https://codex.example.com/v1");
        assert_eq!(channel.models.len(), 2);
    }
}
