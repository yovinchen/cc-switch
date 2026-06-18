//! Proxy channel migration DAO.
//!
//! The first migration step is intentionally additive: project existing
//! providers and provider_endpoints into channel-shaped records without
//! changing the current provider router or forwarding path.

use crate::app_config::AppType;
use crate::database::{lock_conn, to_json_string, Database};
use crate::error::AppError;
use crate::provider::Provider;
use crate::proxy_core::{
    infer_legacy_channel_interface, infer_legacy_model_routes, legacy_channel_priority,
    stable_channel_id, AppKind, LegacyModelRouteInput, LegacyModelRouteProjection,
    LegacyProviderProjectionInput, ProxyChannelModelWriteRequest, ProxyChannelPatchRequest,
    ProxyChannelWriteRequest,
};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

const DEFAULT_GROUP: &str = "default";
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
    fn as_str(&self) -> &'static str {
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

impl Database {
    pub(crate) fn preview_legacy_proxy_channel_migration(
        &self,
        app_type: &str,
    ) -> Result<ProxyChannelMigrationPreview, AppError> {
        let providers = self.get_all_providers(app_type)?;
        let current_provider_id = self.get_current_provider(app_type)?;
        let app = AppType::from_str(app_type).ok();
        let app_kind = app.as_ref().map(|app| AppKind::from(app.as_str()));
        let mut channels = Vec::new();
        let mut seen_routes = HashSet::new();
        let mut duplicate_count = 0usize;

        for provider in providers.values() {
            let projection = legacy_provider_projection_input(provider);
            let priority = legacy_channel_priority(
                &provider.id,
                provider.in_failover_queue,
                current_provider_id.as_deref(),
            );
            let interface_kind = infer_legacy_channel_interface(app_kind.as_ref(), &projection)
                .as_str()
                .to_string();
            let primary_base_url = app
                .as_ref()
                .map(|app| provider.resolve_usage_credentials(app).0)
                .unwrap_or_default();

            let primary = build_legacy_channel(
                app_type,
                app_kind.as_ref(),
                provider,
                &projection,
                normalize_base_url(&primary_base_url),
                interface_kind.clone(),
                priority,
                ProxyChannelSourceKind::LegacyPrimary,
                None,
            );
            push_channel_or_count_duplicate(
                &mut channels,
                &mut seen_routes,
                &mut duplicate_count,
                primary,
            );

            let mut endpoints: Vec<_> = provider
                .meta
                .as_ref()
                .map(|meta| meta.custom_endpoints.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            endpoints.sort_by(|a, b| a.added_at.cmp(&b.added_at).then_with(|| a.url.cmp(&b.url)));

            for endpoint in endpoints {
                let channel = build_legacy_channel(
                    app_type,
                    app_kind.as_ref(),
                    provider,
                    &projection,
                    normalize_base_url(&endpoint.url),
                    interface_kind.clone(),
                    priority,
                    ProxyChannelSourceKind::LegacyEndpoint,
                    Some(endpoint.url),
                );
                push_channel_or_count_duplicate(
                    &mut channels,
                    &mut seen_routes,
                    &mut duplicate_count,
                    channel,
                );
            }
        }

        let needs_review_count = channels
            .iter()
            .filter(|channel| channel.needs_review)
            .count();
        Ok(ProxyChannelMigrationPreview {
            app_type: app_type.to_string(),
            channels,
            duplicate_count,
            needs_review_count,
        })
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
                    ) VALUES (?1, 'unknown', 0, ?2)",
                    params![channel.id, now],
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

        let mut channels = Vec::new();
        let rows = stmt
            .query_map([app_type], |row| {
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
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        for row in rows {
            channels.push(row.map_err(|e| AppError::Database(e.to_string()))?);
        }

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
        validate_proxy_channel_write_request(&request)?;
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
                normalize_required_string(&request.name, "name")?,
                normalize_required_string(&request.status, "status")?,
                normalize_base_url(&request.base_url),
                normalize_required_string(&request.interface_kind, "interfaceKind")?,
                request.auth_profile_ref,
                to_json_string(&normalized_groups(request.groups))?,
                request.priority,
                request.weight as i64,
                to_json_string(&object_or_default(request.retry_policy))?,
                to_json_string(&object_or_default(request.health_policy))?,
                to_json_string(&object_or_default(request.header_overrides))?,
                to_json_string(&object_or_default(request.param_overrides))?,
                to_json_string(&array_or_default(request.status_code_mapping))?,
                to_json_string(&request.tags)?,
                to_json_string(&object_or_default(request.metadata))?,
                ProxyChannelSourceKind::Manual.as_str(),
                now,
                now,
            ],
        )
        .map_err(|e| AppError::Database(format!("创建 proxy channel 失败: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO proxy_channel_health (
                channel_id, status, consecutive_failures, updated_at
            ) VALUES (?1, 'unknown', 0, ?2)",
            params![channel_id, now],
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

        if let Some(name) = patch.name {
            current.name = normalize_required_string(&name, "name")?;
        }
        if let Some(status) = patch.status {
            current.status = normalize_required_string(&status, "status")?;
        }
        if let Some(base_url) = patch.base_url {
            current.base_url = normalize_base_url(&base_url);
            if current.base_url.is_empty() {
                return Err(AppError::InvalidInput(
                    "baseUrl cannot be empty".to_string(),
                ));
            }
        }
        if let Some(interface_kind) = patch.interface_kind {
            current.interface_kind = normalize_required_string(&interface_kind, "interfaceKind")?;
        }
        if let Some(auth_profile_ref) = patch.auth_profile_ref {
            current.auth_profile_ref = normalize_optional_string(auth_profile_ref);
        }
        if let Some(groups) = patch.groups {
            current.groups = normalized_groups(groups);
        }
        if let Some(priority) = patch.priority {
            current.priority = priority;
        }
        if let Some(weight) = patch.weight {
            current.weight = weight;
        }
        if let Some(retry_policy) = patch.retry_policy {
            current.retry_policy = object_or_default(retry_policy);
        }
        if let Some(health_policy) = patch.health_policy {
            current.health_policy = object_or_default(health_policy);
        }
        if let Some(header_overrides) = patch.header_overrides {
            current.header_overrides = object_or_default(header_overrides);
        }
        if let Some(param_overrides) = patch.param_overrides {
            current.param_overrides = object_or_default(param_overrides);
        }
        if let Some(status_code_mapping) = patch.status_code_mapping {
            current.status_code_mapping = array_or_default(status_code_mapping);
        }
        if let Some(tags) = patch.tags {
            current.tags = tags;
        }
        if let Some(metadata) = patch.metadata {
            current.metadata = object_or_default(metadata);
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
        models: Vec<ProxyChannelModelWriteRequest>,
    ) -> Result<Option<Vec<ProxyChannelModelRecord>>, AppError> {
        let conn = lock_conn!(self.conn);
        if get_proxy_channel_on_conn(&conn, channel_id)?.is_none() {
            return Ok(None);
        }
        replace_proxy_channel_models_on_conn(&conn, channel_id, models)?;
        Ok(Some(list_proxy_channel_models_on_conn(&conn, channel_id)?))
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
                status: "unknown".to_string(),
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

        let (status, consecutive_failures, last_success_at, last_failure_at, disabled_reason) =
            if success {
                ("healthy", 0u32, Some(now), None, None)
            } else {
                let failures = current_failures + 1;
                let status = if failures >= failure_threshold {
                    "unhealthy"
                } else {
                    "degraded"
                };
                (status, failures, None, Some(now), error_msg)
            };

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
                status,
                last_success_at,
                last_failure_at,
                consecutive_failures as i64,
                response_time_ms,
                disabled_reason,
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
    for model in &models {
        validate_proxy_channel_model_write_request(model)?;
    }

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
                normalize_required_string(&model.public_model, "publicModel")?,
                normalize_required_string(&model.upstream_model, "upstreamModel")?,
                to_json_string(&object_or_default(model.capabilities))?,
                model.pricing_model,
                to_json_string(&object_or_default(model.request_overrides))?,
                to_json_string(&object_or_default(model.response_overrides))?,
                now,
                now,
            ],
        )
        .map_err(|e| AppError::Database(format!("写入 proxy channel model 失败: {e}")))?;
    }

    Ok(())
}

fn push_channel_or_count_duplicate(
    channels: &mut Vec<ProxyChannelRecord>,
    seen_routes: &mut HashSet<(String, String, String)>,
    duplicate_count: &mut usize,
    channel: ProxyChannelRecord,
) {
    let route_key = (
        channel.provider_id.clone(),
        channel.interface_kind.clone(),
        channel.base_url.clone(),
    );
    if !seen_routes.insert(route_key) {
        *duplicate_count += 1;
        return;
    }
    channels.push(channel);
}

fn build_legacy_channel(
    app_type: &str,
    app: Option<&AppKind>,
    provider: &Provider,
    projection: &LegacyProviderProjectionInput,
    base_url: String,
    interface_kind: String,
    priority: i64,
    source_kind: ProxyChannelSourceKind,
    source_endpoint_url: Option<String>,
) -> ProxyChannelRecord {
    let id = stable_channel_id(app_type, &provider.id, source_kind.as_str(), &base_url);
    let models = infer_legacy_model_routes(app, projection)
        .into_iter()
        .map(|route| proxy_channel_model_record_from_legacy(&id, route))
        .collect::<Vec<_>>();

    let mut review_reasons = Vec::new();
    if base_url.is_empty() {
        review_reasons.push("missing_base_url".to_string());
    }
    if models.is_empty() {
        review_reasons.push("no_model_mapping_inferred".to_string());
    }

    let needs_review = !review_reasons.is_empty();
    let metadata = json!({
        "migration_source": source_kind.as_str(),
        "provider_name": provider.name.as_str(),
        "provider_sort_index": provider.sort_index,
        "provider_in_failover_queue": provider.in_failover_queue,
        "needs_review": needs_review,
        "review_reasons": review_reasons,
    });
    let name = match &source_kind {
        ProxyChannelSourceKind::LegacyPrimary => format!("{} primary", provider.name),
        ProxyChannelSourceKind::LegacyEndpoint => format!("{} endpoint", provider.name),
        ProxyChannelSourceKind::Manual => provider.name.clone(),
    };

    ProxyChannelRecord {
        id,
        provider_id: provider.id.clone(),
        app_type: app_type.to_string(),
        name,
        status: "enabled".to_string(),
        base_url,
        interface_kind,
        auth_profile_ref: Some(format!("provider:{app_type}:{}", provider.id)),
        groups: vec![DEFAULT_GROUP.to_string()],
        priority,
        weight: 100,
        retry_policy: json!({}),
        health_policy: json!({}),
        header_overrides: json!({}),
        param_overrides: json!({}),
        status_code_mapping: json!([]),
        tags: vec!["legacy".to_string()],
        metadata,
        source_kind,
        source_endpoint_url,
        models,
        needs_review,
        review_reasons,
    }
}

fn proxy_channel_model_record_from_legacy(
    channel_id: &str,
    route: LegacyModelRouteProjection,
) -> ProxyChannelModelRecord {
    ProxyChannelModelRecord {
        channel_id: channel_id.to_string(),
        public_model: route.public_model,
        upstream_model: route.upstream_model,
        capabilities: json!({}),
        pricing_model: None,
        request_overrides: json!({}),
        response_overrides: json!({}),
    }
}

fn normalize_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn default_channel_groups() -> Vec<String> {
    vec![DEFAULT_GROUP.to_string()]
}

fn validate_proxy_channel_write_request(
    request: &ProxyChannelWriteRequest,
) -> Result<(), AppError> {
    let _ = AppType::from_str(&request.app_type)?;
    normalize_required_string(&request.provider_id, "providerId")?;
    normalize_required_string(&request.name, "name")?;
    normalize_required_string(&request.status, "status")?;
    if normalize_base_url(&request.base_url).is_empty() {
        return Err(AppError::InvalidInput(
            "baseUrl cannot be empty".to_string(),
        ));
    }
    normalize_required_string(&request.interface_kind, "interfaceKind")?;
    for model in &request.models {
        validate_proxy_channel_model_write_request(model)?;
    }
    Ok(())
}

fn validate_proxy_channel_model_write_request(
    model: &ProxyChannelModelWriteRequest,
) -> Result<(), AppError> {
    normalize_required_string(&model.public_model, "publicModel")?;
    normalize_required_string(&model.upstream_model, "upstreamModel")?;
    Ok(())
}

fn normalize_required_string(value: &str, field: &str) -> Result<String, AppError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        Err(AppError::InvalidInput(format!("{field} cannot be empty")))
    } else {
        Ok(normalized.to_string())
    }
}

fn normalize_optional_string(value: String) -> Option<String> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn normalized_groups(groups: Vec<String>) -> Vec<String> {
    let mut normalized: Vec<String> = groups
        .into_iter()
        .map(|group| group.trim().to_string())
        .filter(|group| !group.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();
    if normalized.is_empty() {
        default_channel_groups()
    } else {
        normalized
    }
}

fn object_or_default(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        json!({})
    }
}

fn array_or_default(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        json!([])
    }
}

fn legacy_provider_projection_input(provider: &Provider) -> LegacyProviderProjectionInput {
    let config_text = provider
        .settings_config
        .get("config")
        .and_then(|value| value.as_str());
    let env = provider
        .settings_config
        .get("env")
        .and_then(|value| value.as_object())
        .map(|env| {
            env.iter()
                .filter_map(|(key, value)| {
                    value
                        .as_str()
                        .map(|model| (key.to_string(), model.to_string()))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let codex_catalog_models = provider
        .settings_config
        .get("modelCatalog")
        .and_then(|catalog| catalog.get("models"))
        .and_then(|models| models.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|entry| {
                    entry
                        .get("model")
                        .and_then(|value| value.as_str())
                        .map(ToString::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let (api_format, claude_desktop_model_routes) = provider
        .meta
        .as_ref()
        .map(|meta| {
            let routes = meta
                .claude_desktop_model_routes
                .iter()
                .map(|(public_model, route)| LegacyModelRouteInput {
                    public_model: public_model.clone(),
                    upstream_model: route.model.clone(),
                })
                .collect::<Vec<_>>();
            (meta.api_format.clone(), routes)
        })
        .unwrap_or_default();

    LegacyProviderProjectionInput {
        api_format,
        codex_wire_api: config_text.and_then(extract_codex_wire_api),
        codex_model: config_text.and_then(extract_codex_model),
        codex_catalog_models,
        env,
        claude_desktop_model_routes,
    }
}

fn extract_codex_wire_api(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<toml::Value>().ok()?;
    if let Some(active_provider) = doc.get("model_provider").and_then(|value| value.as_str()) {
        if let Some(wire_api) = doc
            .get("model_providers")
            .and_then(|providers| providers.get(active_provider))
            .and_then(|provider| provider.get("wire_api"))
            .and_then(|value| value.as_str())
        {
            return Some(wire_api.to_string());
        }
    }
    doc.get("wire_api")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn extract_codex_model(config_text: &str) -> Option<String> {
    let doc = config_text.parse::<toml::Value>().ok()?;
    doc.get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
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
    use crate::provider::{ClaudeDesktopModelRoute, ProviderMeta};
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

        let replaced_models = db
            .replace_proxy_channel_models(
                &created.id,
                vec![ProxyChannelModelWriteRequest {
                    public_model: "haiku-public".to_string(),
                    upstream_model: "upstream-haiku".to_string(),
                    ..Default::default()
                }],
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
