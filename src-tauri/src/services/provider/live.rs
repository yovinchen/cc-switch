//! Live configuration operations
//!
//! Handles reading and writing live configuration files for Claude, Codex, and Gemini.

use serde_json::{json, Value};

use crate::app_config::AppType;
use crate::config::{get_claude_settings_path, read_json_file, write_json_file};
use crate::database::Database;
use crate::error::AppError;
use crate::openclaw_config::OpenClawProviderConfig;
use crate::provider::{OpenCodeProviderConfig, Provider, ProviderMeta};
use crate::proxy_core::api::domain::openclaw_settings_have_live_provider_fields;
use crate::proxy_core::api::domain::opencode_settings_have_live_provider_fields;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::ports::{
    apply_claude_common_config_to_settings as core_apply_claude_common_config_to_settings,
    apply_gemini_common_config_to_settings as core_apply_gemini_common_config_to_settings,
    codex_config_text_from_settings,
    codex_live_snapshot_parts_from_settings as core_codex_live_snapshot_parts_from_settings,
    codex_provider_backfill_parts_from_settings as core_codex_provider_backfill_parts_from_settings,
    common_config_settings_mutation_issue_message,
    contains_claude_common_config_snippet as core_contains_claude_common_config_snippet,
    contains_gemini_common_config_snippet as core_contains_gemini_common_config_snippet,
    gemini_env_string_map_from_settings, gemini_live_config_object_from_settings,
    gemini_live_settings_from_env_json_and_config, gemini_live_settings_to_write,
    openclaw_live_write_action_decision as core_openclaw_live_write_action_decision,
    openclaw_live_write_config_decision as core_openclaw_live_write_config_decision,
    opencode_live_provider_fragment_decision as core_opencode_live_provider_fragment_decision,
    opencode_live_write_action_decision as core_opencode_live_write_action_decision,
    opencode_live_write_config_decision as core_opencode_live_write_config_decision,
    provider_common_config_storage_normalization_requires_snippet as core_provider_common_config_storage_normalization_requires_snippet,
    provider_default_live_import_category_from_parts as core_provider_default_live_import_category_from_parts,
    provider_default_live_import_settings,
    provider_live_sync_scope_for_app as core_provider_live_sync_scope,
    provider_non_codex_common_config_snippet_from_settings as core_provider_non_codex_common_config_snippet_from_settings,
    provider_should_sync_to_live,
    provider_uses_common_config_from_parts as core_provider_uses_common_config_from_parts,
    proxy_live_config_owned_by_takeover,
    remove_claude_common_config_from_settings as core_remove_claude_common_config_from_settings,
    remove_gemini_common_config_from_settings as core_remove_gemini_common_config_from_settings,
    sanitize_claude_settings_for_live, should_skip_manual_default_live_import,
    should_skip_startup_default_live_import, CodexLiveSnapshotIssue, CodexLiveSnapshotParts,
    CodexProviderBackfillParts, CommonConfigSettingsMutationIssue, CommonConfigSnippetIssue,
    GeminiLiveConfigIssue, OpenClawLiveWriteActionDecision as CoreOpenClawLiveWriteActionDecision,
    OpenClawLiveWriteConfigDecision as CoreOpenClawLiveWriteConfigDecision,
    OpenCodeLiveWriteActionDecision as CoreOpenCodeLiveWriteActionDecision,
    OpenCodeLiveWriteConfigDecision as CoreOpenCodeLiveWriteConfigDecision, ProviderLiveSyncScope,
};
use crate::proxy_core_adapter::restore_codex_settings_for_provider_backfill as adapter_restore_codex_settings_for_provider_backfill;
use crate::services::mcp::McpService;
use crate::store::AppState;

use super::gemini_auth::{
    detect_gemini_auth_type, ensure_google_oauth_security_flag, GeminiAuthType,
};
pub(crate) fn provider_exists_in_live_config(
    app_type: &AppType,
    provider_id: &str,
) -> Result<bool, AppError> {
    match app_type {
        AppType::OpenCode => crate::opencode_config::get_providers()
            .map(|providers| providers.contains_key(provider_id)),
        AppType::OpenClaw => crate::openclaw_config::get_providers()
            .map(|providers| providers.contains_key(provider_id)),
        AppType::Hermes => crate::hermes_config::get_providers()
            .map(|providers| providers.contains_key(provider_id)),
        _ => Ok(false),
    }
}

fn common_config_settings_mutation_issue_to_app_error(
    issue: CommonConfigSettingsMutationIssue,
) -> AppError {
    AppError::Message(common_config_settings_mutation_issue_message(issue))
}

pub(crate) fn common_config_snippet_from_settings(
    app_type: &AppType,
    settings: &Value,
) -> Result<String, CommonConfigSnippetIssue> {
    match app_type {
        AppType::Codex => codex_common_config_snippet_from_settings(settings),
        AppType::Claude
        | AppType::ClaudeDesktop
        | AppType::Gemini
        | AppType::OpenCode
        | AppType::OpenClaw
        | AppType::Hermes => core_provider_non_codex_common_config_snippet_from_settings(
            &AppKind::from(app_type),
            settings,
        )
        .map(|snippet| snippet.expect("known non-Codex app should project common config snippet")),
    }
}

fn codex_common_config_snippet_from_settings(
    settings: &Value,
) -> Result<String, CommonConfigSnippetIssue> {
    let config_toml = codex_config_text_from_settings(settings).unwrap_or("");

    if config_toml.is_empty() {
        return Ok(String::new());
    }

    let mut doc = config_toml
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| CommonConfigSnippetIssue::TomlParse(e.to_string()))?;

    let root = doc.as_table_mut();
    root.remove("model");
    root.remove("model_provider");
    root.remove("base_url");
    root.remove("model_providers");

    let mut cleaned = String::new();
    let mut blank_run = 0usize;
    for line in doc.to_string().lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                cleaned.push('\n');
            }
            continue;
        }
        blank_run = 0;
        cleaned.push_str(line);
        cleaned.push('\n');
    }

    Ok(cleaned.trim().to_string())
}

fn toml_value_is_subset(target: &toml_edit::Value, source: &toml_edit::Value) -> bool {
    match (target, source) {
        (toml_edit::Value::String(target), toml_edit::Value::String(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Integer(target), toml_edit::Value::Integer(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Float(target), toml_edit::Value::Float(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Boolean(target), toml_edit::Value::Boolean(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Datetime(target), toml_edit::Value::Datetime(source)) => {
            target.value() == source.value()
        }
        (toml_edit::Value::Array(target), toml_edit::Value::Array(source)) => {
            toml_array_contains_subset(target, source)
        }
        (toml_edit::Value::InlineTable(target), toml_edit::Value::InlineTable(source)) => {
            source.iter().all(|(key, source_item)| {
                target
                    .get(key)
                    .is_some_and(|target_item| toml_value_is_subset(target_item, source_item))
            })
        }
        _ => false,
    }
}

fn toml_array_contains_subset(target: &toml_edit::Array, source: &toml_edit::Array) -> bool {
    let mut matched = vec![false; target.len()];
    let target_items: Vec<&toml_edit::Value> = target.iter().collect();

    source.iter().all(|source_item| {
        if let Some((index, _)) = target_items
            .iter()
            .enumerate()
            .find(|(index, target_item)| {
                !matched[*index] && toml_value_is_subset(target_item, source_item)
            })
        {
            matched[index] = true;
            true
        } else {
            false
        }
    })
}

fn toml_remove_array_items(target: &mut toml_edit::Array, source: &toml_edit::Array) {
    for source_item in source.iter() {
        let index = {
            let target_items: Vec<&toml_edit::Value> = target.iter().collect();
            target_items
                .iter()
                .enumerate()
                .find(|(_, target_item)| toml_value_is_subset(target_item, source_item))
                .map(|(index, _)| index)
        };

        if let Some(index) = index {
            target.remove(index);
        }
    }
}

fn toml_item_is_subset(target: &toml_edit::Item, source: &toml_edit::Item) -> bool {
    if let Some(source_table) = source.as_table_like() {
        let Some(target_table) = target.as_table_like() else {
            return false;
        };
        return source_table.iter().all(|(key, source_item)| {
            target_table
                .get(key)
                .is_some_and(|target_item| toml_item_is_subset(target_item, source_item))
        });
    }

    match (target.as_value(), source.as_value()) {
        (Some(target_value), Some(source_value)) => {
            toml_value_is_subset(target_value, source_value)
        }
        _ => false,
    }
}

fn merge_toml_item(target: &mut toml_edit::Item, source: &toml_edit::Item) {
    if let Some(source_table) = source.as_table_like() {
        if let Some(target_table) = target.as_table_like_mut() {
            merge_toml_table_like(target_table, source_table);
            return;
        }
    }

    *target = source.clone();
}

fn merge_toml_table_like(target: &mut dyn toml_edit::TableLike, source: &dyn toml_edit::TableLike) {
    for (key, source_item) in source.iter() {
        match target.get_mut(key) {
            Some(target_item) => merge_toml_item(target_item, source_item),
            None => {
                target.insert(key, source_item.clone());
            }
        }
    }
}

fn remove_toml_item(target: &mut toml_edit::Item, source: &toml_edit::Item) {
    if let Some(source_table) = source.as_table_like() {
        if let Some(target_table) = target.as_table_like_mut() {
            remove_toml_table_like(target_table, source_table);
            if target_table.is_empty() {
                *target = toml_edit::Item::None;
            }
            return;
        }
    }

    if let Some(source_value) = source.as_value() {
        let mut remove_item = false;

        if let Some(target_value) = target.as_value_mut() {
            match (target_value, source_value) {
                (toml_edit::Value::Array(target_arr), toml_edit::Value::Array(source_arr)) => {
                    toml_remove_array_items(target_arr, source_arr);
                    remove_item = target_arr.is_empty();
                }
                (target_value, source_value)
                    if toml_value_is_subset(target_value, source_value) =>
                {
                    remove_item = true;
                }
                _ => {}
            }
        }

        if remove_item {
            *target = toml_edit::Item::None;
        }
    }
}

fn remove_toml_table_like(
    target: &mut dyn toml_edit::TableLike,
    source: &dyn toml_edit::TableLike,
) {
    let keys: Vec<String> = source.iter().map(|(key, _)| key.to_string()).collect();

    for key in keys {
        let mut remove_key = false;
        if let (Some(target_item), Some(source_item)) = (target.get_mut(&key), source.get(&key)) {
            remove_toml_item(target_item, source_item);
            remove_key = target_item.is_none()
                || target_item
                    .as_table_like()
                    .is_some_and(|table_like| table_like.is_empty());
        }

        if remove_key {
            target.remove(&key);
        }
    }
}

fn contains_common_config_snippet(app_type: &AppType, settings: &Value, snippet: &str) -> bool {
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        return false;
    }

    match app_type {
        AppType::Claude => core_contains_claude_common_config_snippet(settings, trimmed),
        AppType::Codex => {
            let config_toml = codex_config_text_from_settings(settings).unwrap_or("");
            if config_toml.trim().is_empty() {
                return false;
            }

            let target_doc = match config_toml.parse::<toml_edit::DocumentMut>() {
                Ok(doc) => doc,
                Err(_) => return false,
            };
            let source_doc = match trimmed.parse::<toml_edit::DocumentMut>() {
                Ok(doc) => doc,
                Err(_) => return false,
            };

            toml_item_is_subset(target_doc.as_item(), source_doc.as_item())
        }
        AppType::Gemini => core_contains_gemini_common_config_snippet(settings, trimmed),
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes | AppType::ClaudeDesktop => false,
    }
}

pub(crate) fn provider_uses_common_config(
    app_type: &AppType,
    provider: &Provider,
    snippet: Option<&str>,
) -> bool {
    let explicit_enabled = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.common_config_enabled);
    let settings_contains_snippet = explicit_enabled.is_none()
        && snippet.is_some_and(|value| {
            contains_common_config_snippet(app_type, &provider.settings_config, value)
        });

    core_provider_uses_common_config_from_parts(
        explicit_enabled,
        snippet,
        settings_contains_snippet,
    )
}

fn provider_common_config_storage_normalization_requires_snippet(provider: &Provider) -> bool {
    let explicit_enabled = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.common_config_enabled);

    core_provider_common_config_storage_normalization_requires_snippet(explicit_enabled)
}

#[cfg(test)]
fn apply_common_config_to_settings(
    app_type: &AppType,
    settings: &Value,
    snippet: &str,
) -> Result<Value, AppError> {
    apply_common_config_to_settings_core(app_type, settings, snippet)
        .map_err(common_config_settings_mutation_issue_to_app_error)
}

fn apply_common_config_to_settings_core(
    app_type: &AppType,
    settings: &Value,
    snippet: &str,
) -> Result<Value, CommonConfigSettingsMutationIssue> {
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        return Ok(settings.clone());
    }

    match app_type {
        AppType::Claude => core_apply_claude_common_config_to_settings(settings, trimmed),
        AppType::Codex => {
            let mut result = settings.clone();
            let config_toml = codex_config_text_from_settings(settings).unwrap_or("");
            let mut target_doc = if config_toml.trim().is_empty() {
                toml_edit::DocumentMut::new()
            } else {
                config_toml.parse::<toml_edit::DocumentMut>().map_err(|e| {
                    CommonConfigSettingsMutationIssue::CodexApplyTargetToml(e.to_string())
                })?
            };
            let source_doc = trimmed.parse::<toml_edit::DocumentMut>().map_err(|e| {
                CommonConfigSettingsMutationIssue::CodexCommonConfigSnippetToml(e.to_string())
            })?;

            merge_toml_table_like(target_doc.as_table_mut(), source_doc.as_table());
            if let Some(obj) = result.as_object_mut() {
                obj.insert("config".to_string(), Value::String(target_doc.to_string()));
            }
            Ok(result)
        }
        AppType::Gemini => core_apply_gemini_common_config_to_settings(settings, trimmed),
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes | AppType::ClaudeDesktop => {
            Ok(settings.clone())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderEffectiveSettingsWarning {
    CommonConfigApply(CommonConfigSettingsMutationIssue),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProviderEffectiveSettingsResult {
    pub(crate) settings: Value,
    pub(crate) warnings: Vec<ProviderEffectiveSettingsWarning>,
}

pub(crate) fn build_effective_settings_with_common_config(
    app_type: &AppType,
    provider: &Provider,
    snippet: Option<&str>,
) -> ProviderEffectiveSettingsResult {
    let mut settings = provider.settings_config.clone();
    let mut warnings = Vec::new();

    if provider_uses_common_config(app_type, provider, snippet) {
        if let Some(snippet_text) = snippet {
            match apply_common_config_to_settings_core(app_type, &settings, snippet_text) {
                Ok(applied_settings) => settings = applied_settings,
                Err(issue) => {
                    warnings.push(ProviderEffectiveSettingsWarning::CommonConfigApply(issue))
                }
            }
        }
    }

    ProviderEffectiveSettingsResult { settings, warnings }
}

pub(crate) fn build_effective_settings_with_common_config_from_db(
    db: &Database,
    app_type: &AppType,
    provider: &Provider,
) -> Result<Value, AppError> {
    let snippet = db.get_config_snippet(app_type.as_str())?;
    let result =
        build_effective_settings_with_common_config(app_type, provider, snippet.as_deref());
    log_provider_effective_settings_warnings(app_type, provider, result.warnings);

    Ok(result.settings)
}

pub(crate) fn remove_common_config_from_settings(
    app_type: &AppType,
    settings: &Value,
    snippet: &str,
) -> Result<Value, AppError> {
    remove_common_config_from_settings_core(app_type, settings, snippet)
        .map_err(common_config_settings_mutation_issue_to_app_error)
}

fn remove_common_config_from_settings_core(
    app_type: &AppType,
    settings: &Value,
    snippet: &str,
) -> Result<Value, CommonConfigSettingsMutationIssue> {
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        return Ok(settings.clone());
    }

    match app_type {
        AppType::Claude => core_remove_claude_common_config_from_settings(settings, trimmed),
        AppType::Codex => {
            let mut result = settings.clone();
            let config_toml = codex_config_text_from_settings(settings).unwrap_or("");
            let mut target_doc = if config_toml.trim().is_empty() {
                toml_edit::DocumentMut::new()
            } else {
                config_toml.parse::<toml_edit::DocumentMut>().map_err(|e| {
                    CommonConfigSettingsMutationIssue::CodexRemoveTargetToml(e.to_string())
                })?
            };
            let source_doc = trimmed.parse::<toml_edit::DocumentMut>().map_err(|e| {
                CommonConfigSettingsMutationIssue::CodexCommonConfigSnippetToml(e.to_string())
            })?;

            remove_toml_table_like(target_doc.as_table_mut(), source_doc.as_table());
            if let Some(obj) = result.as_object_mut() {
                obj.insert("config".to_string(), Value::String(target_doc.to_string()));
            }
            Ok(result)
        }
        AppType::Gemini => core_remove_gemini_common_config_from_settings(settings, trimmed),
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes | AppType::ClaudeDesktop => {
            Ok(settings.clone())
        }
    }
}

pub(crate) fn write_live_with_common_config(
    db: &Database,
    app_type: &AppType,
    provider: &Provider,
) -> Result<(), AppError> {
    let mut effective_provider = provider.clone();
    effective_provider.settings_config =
        build_effective_settings_with_common_config_from_db(db, app_type, provider)?;

    if matches!(app_type, AppType::ClaudeDesktop) {
        crate::claude_desktop_config::apply_provider(db, &effective_provider)?;
        log::info!(
            "Claude Desktop 3P profile '{}' written for provider '{}'",
            crate::claude_desktop_config::PROFILE_ID,
            effective_provider.id
        );
        return Ok(());
    }

    write_live_snapshot(app_type, &effective_provider)
}

pub(crate) fn strip_common_config_from_live_settings(
    db: &Database,
    app_type: &AppType,
    provider: &Provider,
    live_settings: Value,
) -> Value {
    let snippet = match db.get_config_snippet(app_type.as_str()) {
        Ok(snippet) => snippet,
        Err(err) => {
            log::warn!(
                "Failed to load common config for {} while backfilling '{}': {err}",
                app_type.as_str(),
                provider.id
            );
            return restore_live_settings_for_provider_backfill(app_type, provider, live_settings);
        }
    };

    strip_common_config_from_live_settings_for_backfill(
        app_type,
        provider,
        live_settings,
        snippet.as_deref(),
    )
}

fn provider_codex_config_text(provider: &Provider) -> Option<&str> {
    codex_config_text_from_settings(&provider.settings_config)
}

fn provider_from_default_live_settings(app_type: &AppType, settings_config: Value) -> Provider {
    let mut provider = Provider::with_id(
        "default".to_string(),
        "default".to_string(),
        settings_config,
        None,
    );
    let codex_config_has_provider_key = if matches!(app_type, AppType::Codex) {
        provider_codex_config_text(&provider)
            .and_then(crate::codex_config::extract_codex_experimental_bearer_token)
            .is_some()
    } else {
        false
    };
    provider.category = Some(
        core_provider_default_live_import_category_from_parts(
            &AppKind::from(app_type),
            provider.settings_config.get("auth"),
            codex_config_has_provider_key,
        )
        .to_string(),
    );

    provider
}

fn provider_codex_backfill_parts(provider: &Provider) -> CodexProviderBackfillParts<'_> {
    core_codex_provider_backfill_parts_from_settings(
        provider.category.as_deref(),
        &provider.settings_config,
    )
}

fn strip_codex_unified_session_bucket_for_provider_backfill(
    provider: &Provider,
    settings: &mut Value,
) -> Result<(), AppError> {
    let backfill_parts = provider_codex_backfill_parts(provider);
    if backfill_parts.strip_unified_session_bucket {
        crate::codex_config::strip_codex_unified_session_bucket_from_settings(settings)?;
    }
    Ok(())
}

fn provider_codex_live_snapshot_parts(
    provider: &Provider,
) -> Result<CodexLiveSnapshotParts<'_>, CodexLiveSnapshotIssue> {
    core_codex_live_snapshot_parts_from_settings(
        &provider.settings_config,
        provider.category.as_deref(),
    )
}

fn codex_live_settings_with_model_catalog(
    mut live_settings: Value,
    model_catalog: Option<Value>,
) -> Value {
    if let (Some(root), Some(model_catalog)) = (live_settings.as_object_mut(), model_catalog) {
        root.insert("modelCatalog".to_string(), model_catalog);
    }

    live_settings
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProviderBackfillSettingsWarning {
    CommonConfigStrip(CommonConfigSettingsMutationIssue),
    CodexSettingsRestore(String),
    CodexUnifiedSessionBucketStrip(String),
}

#[derive(Debug, Clone, PartialEq)]
struct ProviderBackfillSettingsResult {
    settings: Value,
    warnings: Vec<ProviderBackfillSettingsWarning>,
}

fn restore_live_settings_for_provider_backfill(
    app_type: &AppType,
    provider: &Provider,
    live_settings: Value,
) -> Value {
    let result =
        restore_live_settings_for_provider_backfill_result(app_type, provider, live_settings);
    log_provider_backfill_settings_warnings(app_type, provider, result.warnings);

    result.settings
}

fn restore_live_settings_for_provider_backfill_result(
    app_type: &AppType,
    provider: &Provider,
    live_settings: Value,
) -> ProviderBackfillSettingsResult {
    if !matches!(app_type, AppType::Codex) {
        return ProviderBackfillSettingsResult {
            settings: live_settings,
            warnings: Vec::new(),
        };
    }

    let mut settings = live_settings;
    let mut warnings = Vec::new();
    if let Err(err) = adapter_restore_codex_settings_for_provider_backfill(provider, &mut settings)
    {
        warnings.push(ProviderBackfillSettingsWarning::CodexSettingsRestore(
            err.to_string(),
        ));
    }

    if let Err(err) =
        strip_codex_unified_session_bucket_for_provider_backfill(provider, &mut settings)
    {
        warnings
            .push(ProviderBackfillSettingsWarning::CodexUnifiedSessionBucketStrip(err.to_string()));
    }

    // `modelCatalog` is a cc-switch-private field whose SSOT is the DB. Live's
    // `config.toml` only carries a lossy projection that proxy takeover/restore
    // cycles and Codex.app config rewrites can drop.
    settings = codex_live_settings_with_model_catalog(
        settings,
        provider.settings_config.get("modelCatalog").cloned(),
    );

    ProviderBackfillSettingsResult { settings, warnings }
}

fn strip_common_config_from_live_settings_for_backfill(
    app_type: &AppType,
    provider: &Provider,
    live_settings: Value,
    snippet: Option<&str>,
) -> Value {
    let mut warnings = Vec::new();
    let backfill_settings = if provider_uses_common_config(app_type, provider, snippet) {
        match snippet {
            Some(snippet_text) => {
                match remove_common_config_from_settings_core(
                    app_type,
                    &live_settings,
                    snippet_text,
                ) {
                    Ok(settings) => settings,
                    Err(issue) => {
                        warnings.push(ProviderBackfillSettingsWarning::CommonConfigStrip(issue));
                        live_settings
                    }
                }
            }
            None => live_settings,
        }
    } else {
        live_settings
    };

    log_provider_backfill_settings_warnings(app_type, provider, warnings);
    restore_live_settings_for_provider_backfill(app_type, provider, backfill_settings)
}

fn log_provider_backfill_settings_warnings(
    app_type: &AppType,
    provider: &Provider,
    warnings: Vec<ProviderBackfillSettingsWarning>,
) {
    for warning in warnings {
        match warning {
            ProviderBackfillSettingsWarning::CommonConfigStrip(issue) => {
                let err = common_config_settings_mutation_issue_to_app_error(issue);
                log::warn!(
                    "Failed to strip common config for {} provider '{}': {err}",
                    app_type.as_str(),
                    provider.id
                );
            }
            ProviderBackfillSettingsWarning::CodexSettingsRestore(err) => {
                log::warn!(
                    "Failed to restore Codex settings while backfilling '{}': {err}",
                    provider.id
                );
            }
            ProviderBackfillSettingsWarning::CodexUnifiedSessionBucketStrip(err) => {
                log::warn!(
                    "Failed to strip unified session bucket while backfilling '{}': {err}",
                    provider.id
                );
            }
        }
    }
}

fn log_provider_effective_settings_warnings(
    app_type: &AppType,
    provider: &Provider,
    warnings: Vec<ProviderEffectiveSettingsWarning>,
) {
    for warning in warnings {
        match warning {
            ProviderEffectiveSettingsWarning::CommonConfigApply(issue) => {
                let err = common_config_settings_mutation_issue_to_app_error(issue);
                log::warn!(
                    "Failed to apply common config for {} provider '{}': {err}",
                    app_type.as_str(),
                    provider.id
                );
            }
        }
    }
}

pub(crate) fn normalize_provider_common_config_for_storage(
    db: &Database,
    app_type: &AppType,
    provider: &mut Provider,
) -> Result<(), AppError> {
    if !provider_common_config_storage_normalization_requires_snippet(provider) {
        return Ok(());
    }

    let snippet = db.get_config_snippet(app_type.as_str())?;
    match normalize_provider_common_config_for_storage_from_snippet(
        app_type,
        provider,
        snippet.as_deref(),
    ) {
        Ok(Some(settings)) => provider.settings_config = settings,
        Ok(None) => {}
        Err(issue) => {
            let err = common_config_settings_mutation_issue_to_app_error(issue);
            log::warn!(
                "Failed to normalize common config before saving {} provider '{}': {err}",
                app_type.as_str(),
                provider.id
            );
        }
    }

    Ok(())
}

fn normalize_provider_common_config_for_storage_from_snippet(
    app_type: &AppType,
    provider: &Provider,
    snippet: Option<&str>,
) -> Result<Option<Value>, CommonConfigSettingsMutationIssue> {
    if !provider_common_config_storage_normalization_requires_snippet(provider) {
        return Ok(None);
    }

    let Some(snippet) = snippet.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };

    remove_common_config_from_settings_core(app_type, &provider.settings_config, snippet).map(Some)
}

fn provider_openclaw_has_live_provider_fields(provider: &Provider) -> bool {
    openclaw_settings_have_live_provider_fields(&provider.settings_config)
}

#[derive(Debug, Clone)]
enum OpenClawLiveWriteConfig {
    Typed(OpenClawProviderConfig),
    Raw { config: Value, parse_error: String },
    Invalid { parse_error: String },
}

#[derive(Debug, Clone)]
struct OpenClawLiveWritePlan {
    config: OpenClawLiveWriteConfig,
}

#[derive(Debug, Clone)]
enum OpenClawLiveWriteAction {
    Typed(OpenClawProviderConfig),
    Raw {
        config: Value,
        parse_error: String,
    },
    Reject {
        parse_error: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
struct OpenClawLiveWriteProjection {
    action: OpenClawLiveWriteAction,
}

fn provider_openclaw_live_write_plan(provider: &Provider) -> OpenClawLiveWritePlan {
    let config_to_write = provider.settings_config.clone();
    let typed_config = serde_json::from_value::<OpenClawProviderConfig>(config_to_write.clone());
    let decision = core_openclaw_live_write_config_decision(
        config_to_write,
        typed_config.as_ref().err().map(|error| error.to_string()),
        provider_openclaw_has_live_provider_fields(provider),
    );

    let config = match decision {
        CoreOpenClawLiveWriteConfigDecision::Typed => {
            OpenClawLiveWriteConfig::Typed(typed_config.expect("typed OpenClaw config"))
        }
        CoreOpenClawLiveWriteConfigDecision::Raw {
            config,
            parse_error,
        } => OpenClawLiveWriteConfig::Raw {
            config,
            parse_error,
        },
        CoreOpenClawLiveWriteConfigDecision::Invalid { parse_error } => {
            OpenClawLiveWriteConfig::Invalid { parse_error }
        }
    };

    OpenClawLiveWritePlan { config }
}

fn provider_openclaw_live_write_projection(provider: &Provider) -> OpenClawLiveWriteProjection {
    let plan = provider_openclaw_live_write_plan(provider);
    let action = match plan.config {
        OpenClawLiveWriteConfig::Typed(config) => {
            let decision = core_openclaw_live_write_action_decision(
                &provider.id,
                CoreOpenClawLiveWriteConfigDecision::Typed,
            );
            match decision {
                CoreOpenClawLiveWriteActionDecision::Typed => {
                    OpenClawLiveWriteAction::Typed(config)
                }
                other => unreachable!("typed OpenClaw plan produced non-typed action: {other:?}"),
            }
        }
        OpenClawLiveWriteConfig::Raw {
            config,
            parse_error,
        } => match core_openclaw_live_write_action_decision(
            &provider.id,
            CoreOpenClawLiveWriteConfigDecision::Raw {
                config,
                parse_error,
            },
        ) {
            CoreOpenClawLiveWriteActionDecision::Raw {
                config,
                parse_error,
            } => OpenClawLiveWriteAction::Raw {
                config,
                parse_error,
            },
            other => unreachable!("raw OpenClaw plan produced non-raw action: {other:?}"),
        },
        OpenClawLiveWriteConfig::Invalid { parse_error } => {
            match core_openclaw_live_write_action_decision(
                &provider.id,
                CoreOpenClawLiveWriteConfigDecision::Invalid { parse_error },
            ) {
                CoreOpenClawLiveWriteActionDecision::Reject {
                    parse_error,
                    message,
                } => OpenClawLiveWriteAction::Reject {
                    parse_error,
                    message,
                },
                other => {
                    unreachable!("invalid OpenClaw plan produced non-reject action: {other:?}")
                }
            }
        }
    };

    OpenClawLiveWriteProjection { action }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenClawLiveImportIssue {
    EmptyId,
    NoModels,
    Serialization(String),
}

fn provider_from_openclaw_live_config(
    id: &str,
    config: &OpenClawProviderConfig,
) -> Result<Provider, OpenClawLiveImportIssue> {
    if id.trim().is_empty() {
        return Err(OpenClawLiveImportIssue::EmptyId);
    }
    if config.models.is_empty() {
        return Err(OpenClawLiveImportIssue::NoModels);
    }

    let settings_config = serde_json::to_value(config)
        .map_err(|error| OpenClawLiveImportIssue::Serialization(error.to_string()))?;
    let display_name = config
        .models
        .first()
        .and_then(|model| model.name.clone())
        .unwrap_or_else(|| id.to_string());
    let mut provider = Provider::with_id(id.to_string(), display_name, settings_config, None);
    provider.meta = Some(ProviderMeta {
        live_config_managed: Some(true),
        ..Default::default()
    });

    Ok(provider)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HermesLiveImportIssue {
    EmptyName,
}

fn provider_from_hermes_live_config(
    name: &str,
    config: Value,
) -> Result<Provider, HermesLiveImportIssue> {
    if name.trim().is_empty() {
        return Err(HermesLiveImportIssue::EmptyName);
    }

    let mut provider = Provider::with_id(name.to_string(), name.to_string(), config, None);
    provider.meta = Some(ProviderMeta {
        live_config_managed: Some(true),
        ..Default::default()
    });

    Ok(provider)
}

struct OpenCodeLiveProviderFragment {
    config: Value,
    from_full_config: bool,
}

fn provider_opencode_live_provider_fragment(provider: &Provider) -> OpenCodeLiveProviderFragment {
    let fragment =
        core_opencode_live_provider_fragment_decision(&provider.id, &provider.settings_config);
    OpenCodeLiveProviderFragment {
        config: fragment.config,
        from_full_config: fragment.from_full_config,
    }
}

#[derive(Debug, Clone)]
enum OpenCodeLiveWriteConfig {
    Typed(OpenCodeProviderConfig),
    Raw { config: Value, parse_error: String },
    Invalid { parse_error: String },
}

#[derive(Debug, Clone)]
struct OpenCodeLiveWritePlan {
    config: OpenCodeLiveWriteConfig,
    from_full_config: bool,
}

#[derive(Debug, Clone)]
enum OpenCodeLiveWriteAction {
    Typed(OpenCodeProviderConfig),
    Raw {
        config: Value,
        parse_error: String,
    },
    Reject {
        parse_error: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
struct OpenCodeLiveWriteProjection {
    action: OpenCodeLiveWriteAction,
    from_full_config: bool,
}

fn provider_opencode_live_write_plan(provider: &Provider) -> OpenCodeLiveWritePlan {
    let fragment = provider_opencode_live_provider_fragment(provider);
    let config_to_write = fragment.config;
    let has_live_provider_fields = opencode_settings_have_live_provider_fields(&config_to_write);
    let typed_config = serde_json::from_value::<OpenCodeProviderConfig>(config_to_write.clone());
    let decision = core_opencode_live_write_config_decision(
        config_to_write,
        typed_config.as_ref().err().map(|error| error.to_string()),
        has_live_provider_fields,
    );

    let config = match decision {
        CoreOpenCodeLiveWriteConfigDecision::Typed => {
            OpenCodeLiveWriteConfig::Typed(typed_config.expect("typed OpenCode config"))
        }
        CoreOpenCodeLiveWriteConfigDecision::Raw {
            config,
            parse_error,
        } => OpenCodeLiveWriteConfig::Raw {
            config,
            parse_error,
        },
        CoreOpenCodeLiveWriteConfigDecision::Invalid { parse_error } => {
            OpenCodeLiveWriteConfig::Invalid { parse_error }
        }
    };

    OpenCodeLiveWritePlan {
        config,
        from_full_config: fragment.from_full_config,
    }
}

fn provider_opencode_live_write_projection(provider: &Provider) -> OpenCodeLiveWriteProjection {
    let plan = provider_opencode_live_write_plan(provider);
    let action = match plan.config {
        OpenCodeLiveWriteConfig::Typed(config) => {
            match core_opencode_live_write_action_decision(
                &provider.id,
                CoreOpenCodeLiveWriteConfigDecision::Typed,
            ) {
                CoreOpenCodeLiveWriteActionDecision::Typed => {
                    OpenCodeLiveWriteAction::Typed(config)
                }
                other => unreachable!("typed OpenCode plan produced non-typed action: {other:?}"),
            }
        }
        OpenCodeLiveWriteConfig::Raw {
            config,
            parse_error,
        } => match core_opencode_live_write_action_decision(
            &provider.id,
            CoreOpenCodeLiveWriteConfigDecision::Raw {
                config,
                parse_error,
            },
        ) {
            CoreOpenCodeLiveWriteActionDecision::Raw {
                config,
                parse_error,
            } => OpenCodeLiveWriteAction::Raw {
                config,
                parse_error,
            },
            other => unreachable!("raw OpenCode plan produced non-raw action: {other:?}"),
        },
        OpenCodeLiveWriteConfig::Invalid { parse_error } => {
            match core_opencode_live_write_action_decision(
                &provider.id,
                CoreOpenCodeLiveWriteConfigDecision::Invalid { parse_error },
            ) {
                CoreOpenCodeLiveWriteActionDecision::Reject {
                    parse_error,
                    message,
                } => OpenCodeLiveWriteAction::Reject {
                    parse_error,
                    message,
                },
                other => {
                    unreachable!("invalid OpenCode plan produced non-reject action: {other:?}")
                }
            }
        }
    };

    OpenCodeLiveWriteProjection {
        action,
        from_full_config: plan.from_full_config,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenCodeLiveImportIssue {
    Serialization(String),
}

fn provider_from_opencode_live_config(
    id: &str,
    config: &OpenCodeProviderConfig,
) -> Result<Provider, OpenCodeLiveImportIssue> {
    let settings_config = serde_json::to_value(config)
        .map_err(|error| OpenCodeLiveImportIssue::Serialization(error.to_string()))?;
    let mut provider = Provider::with_id(
        id.to_string(),
        config.name.clone().unwrap_or_else(|| id.to_string()),
        settings_config,
        None,
    );
    provider.meta = Some(ProviderMeta {
        live_config_managed: Some(true),
        ..Default::default()
    });

    Ok(provider)
}

/// Write live configuration snapshot for a provider
pub(crate) fn write_live_snapshot(app_type: &AppType, provider: &Provider) -> Result<(), AppError> {
    match app_type {
        AppType::Claude => {
            let path = get_claude_settings_path();
            let settings = sanitize_claude_settings_for_live(&provider.settings_config);
            write_json_file(&path, &settings)?;
        }
        AppType::ClaudeDesktop => {
            return Err(AppError::localized(
                "claude_desktop.live.requires_db_context",
                "Claude Desktop 配置写入需要通过供应商切换流程执行",
                "Claude Desktop configuration must be written through the provider switch flow",
            ));
        }
        AppType::Codex => {
            let parts =
                provider_codex_live_snapshot_parts(provider).map_err(|issue| match issue {
                    CodexLiveSnapshotIssue::NotObject => {
                        AppError::Config("Codex 供应商配置必须是 JSON 对象".to_string())
                    }
                    CodexLiveSnapshotIssue::MissingAuth => {
                        AppError::Config("Codex 供应商配置缺少 'auth' 字段".to_string())
                    }
                })?;

            crate::codex_config::write_codex_provider_live_with_catalog(
                &provider.settings_config,
                parts.category,
                parts.auth,
                parts.config_text,
            )?;
        }
        AppType::Gemini => {
            // Delegate to write_gemini_live which handles env file writing correctly
            write_gemini_live(provider)?;
        }
        AppType::OpenCode => {
            // OpenCode uses additive mode - write provider to config
            use crate::opencode_config;

            let projection = provider_opencode_live_write_projection(provider);
            if projection.from_full_config {
                log::warn!(
                    "OpenCode provider '{}' has full config structure in settings_config, attempting to extract fragment",
                    provider.id
                );
            }

            match projection.action {
                OpenCodeLiveWriteAction::Typed(config) => {
                    opencode_config::set_typed_provider(&provider.id, &config)?;
                    log::info!("OpenCode provider '{}' written to live config", provider.id);
                }
                OpenCodeLiveWriteAction::Raw {
                    config,
                    parse_error,
                } => {
                    log::warn!(
                        "Failed to parse OpenCode provider config for '{}': {}",
                        provider.id,
                        parse_error
                    );
                    opencode_config::set_provider(&provider.id, config)?;
                    log::info!(
                        "OpenCode provider '{}' written as raw JSON to live config",
                        provider.id
                    );
                }
                OpenCodeLiveWriteAction::Reject {
                    parse_error,
                    message,
                } => {
                    log::warn!(
                        "Failed to parse OpenCode provider config for '{}': {}",
                        provider.id,
                        parse_error
                    );
                    return Err(AppError::Message(message));
                }
            }
        }
        AppType::OpenClaw => {
            // OpenClaw uses additive mode - write provider to config
            use crate::openclaw_config;

            let projection = provider_openclaw_live_write_projection(provider);
            match projection.action {
                OpenClawLiveWriteAction::Typed(config) => {
                    openclaw_config::set_typed_provider(&provider.id, &config)?;
                    log::info!("OpenClaw provider '{}' written to live config", provider.id);
                }
                OpenClawLiveWriteAction::Raw {
                    config,
                    parse_error,
                } => {
                    log::warn!(
                        "Failed to parse OpenClaw provider config for '{}': {}",
                        provider.id,
                        parse_error
                    );
                    openclaw_config::set_provider(&provider.id, config)?;
                    log::info!(
                        "OpenClaw provider '{}' written as raw JSON to live config",
                        provider.id
                    );
                }
                OpenClawLiveWriteAction::Reject {
                    parse_error,
                    message,
                } => {
                    log::warn!(
                        "Failed to parse OpenClaw provider config for '{}': {}",
                        provider.id,
                        parse_error
                    );
                    return Err(AppError::Message(message));
                }
            }
        }
        AppType::Hermes => {
            crate::hermes_config::set_provider(&provider.id, provider.settings_config.clone())?;
            log::debug!("Hermes provider '{}' written to live config", provider.id);
        }
    }
    Ok(())
}

/// Sync all providers to live configuration (for additive mode apps)
///
/// Writes all providers from the database to the live configuration file.
/// Used for OpenCode and other additive mode applications.
fn sync_all_providers_to_live(state: &AppState, app_type: &AppType) -> Result<(), AppError> {
    let providers = state.db.get_all_providers(app_type.as_str())?;
    let mut synced_count = 0usize;

    for provider in providers.values() {
        if !provider_should_sync_to_live(provider.live_config_managed()) {
            continue;
        }

        if let Err(e) = write_live_with_common_config(state.db.as_ref(), app_type, provider) {
            log::warn!(
                "Failed to sync {:?} provider '{}' to live: {e}",
                app_type,
                provider.id
            );
            continue;
        }
        synced_count += 1;
    }

    log::info!("Synced {synced_count} {app_type:?} providers to live config");
    Ok(())
}

pub(crate) fn sync_current_provider_for_app_to_live(
    state: &AppState,
    app_type: &AppType,
) -> Result<(), AppError> {
    match core_provider_live_sync_scope(&AppKind::from(app_type)) {
        ProviderLiveSyncScope::AllProviders => sync_all_providers_to_live(state, app_type)?,
        ProviderLiveSyncScope::CurrentProvider => {
            let current_id =
                match crate::settings::get_effective_current_provider(&state.db, app_type)? {
                    Some(id) => id,
                    None => return Ok(()),
                };

            let providers = state.db.get_all_providers(app_type.as_str())?;
            if let Some(provider) = providers.get(&current_id) {
                write_live_with_common_config(state.db.as_ref(), app_type, provider)?;
            }
        }
    }

    McpService::sync_all_enabled(state)?;

    Ok(())
}

fn sync_current_provider_for_app_respecting_takeover(
    state: &AppState,
    app_type: &AppType,
) -> Result<(), AppError> {
    let current_id = match crate::settings::get_effective_current_provider(&state.db, app_type)? {
        Some(id) => id,
        None => return Ok(()),
    };

    let providers = state.db.get_all_providers(app_type.as_str())?;
    let Some(provider) = providers.get(&current_id) else {
        return Ok(());
    };

    let has_live_backup = futures::executor::block_on(state.db.get_live_backup(app_type.as_str()))
        .ok()
        .flatten()
        .is_some();
    let live_taken_over = state
        .proxy_service
        .detect_takeover_in_live_config_for_app(app_type);

    // `enabled` is set only after takeover writes complete. During that
    // activation window, backup/live placeholders are the authoritative signal
    // that normal provider sync must not rewrite the managed live file.
    if proxy_live_config_owned_by_takeover(has_live_backup, live_taken_over) {
        if matches!(app_type, AppType::ClaudeDesktop) {
            write_live_with_common_config(state.db.as_ref(), app_type, provider)?;
        } else {
            futures::executor::block_on(
                state
                    .proxy_service
                    .update_live_backup_from_provider(app_type.as_str(), provider),
            )
            .map_err(|e| AppError::Message(format!("更新 Live 备份失败: {e}")))?;
        }
        return Ok(());
    }

    write_live_with_common_config(state.db.as_ref(), app_type, provider)
}

/// Sync current provider to live configuration
///
/// 使用有效的当前供应商 ID（验证过存在性）。
/// 优先从本地 settings 读取，验证后 fallback 到数据库的 is_current 字段。
/// 这确保了配置导入后无效 ID 会自动 fallback 到数据库。
///
/// For additive mode apps (OpenCode), all providers are synced instead of just the current one.
pub fn sync_current_to_live(state: &AppState) -> Result<(), AppError> {
    // Sync providers based on mode
    for app_type in AppType::all() {
        match core_provider_live_sync_scope(&AppKind::from(&app_type)) {
            // Additive mode: sync ALL providers
            ProviderLiveSyncScope::AllProviders => sync_all_providers_to_live(state, &app_type)?,
            // Switch mode: sync only current provider. During proxy takeover,
            // update the restore backup instead of rewriting the taken-over
            // live file.
            ProviderLiveSyncScope::CurrentProvider => {
                sync_current_provider_for_app_respecting_takeover(state, &app_type)?;
            }
        }
    }

    // MCP sync
    McpService::sync_all_enabled(state)?;

    // Skill sync
    for app_type in AppType::all() {
        if let Err(e) = crate::services::skill::SkillService::sync_to_app(&state.db, &app_type) {
            log::warn!("同步 Skill 到 {app_type:?} 失败: {e}");
            // Continue syncing other apps, don't abort
        }
    }

    Ok(())
}

/// Read current live settings for an app type
pub fn read_live_settings(app_type: AppType) -> Result<Value, AppError> {
    match app_type {
        AppType::Codex => {
            let result = crate::codex_config::read_codex_live_settings()?;
            // `modelCatalog` is a cc-switch private field that lives only in
            // the DB SSOT plus the `cc-switch-model-catalog.json` projection
            // file — it is never inlined into `auth.json` or `config.toml`.
            // Reverse-parse the projection so the edit form for the active
            // Codex provider doesn't see an empty mapping table.
            let model_catalog =
                crate::codex_config::read_codex_model_catalog_simplified_from_live()
                    .ok()
                    .flatten();
            Ok(codex_live_settings_with_model_catalog(result, model_catalog))
        }
        AppType::Claude => {
            let path = get_claude_settings_path();
            if !path.exists() {
                return Err(AppError::localized(
                    "claude.live.missing",
                    "Claude Code 配置文件不存在",
                    "Claude settings file is missing",
                ));
            }
            read_json_file(&path)
        }
        AppType::ClaudeDesktop => Err(AppError::localized(
            "claude_desktop.live.read_unsupported",
            "Claude Desktop 3P 配置不支持作为通用 live 配置导入，请使用“从 Claude 导入兼容供应商”。",
            "Claude Desktop 3P configuration cannot be imported as a generic live config. Use 'Import compatible providers from Claude' instead.",
        )),
        AppType::Gemini => {
            use crate::gemini_config::{
                env_to_json, get_gemini_env_path, get_gemini_settings_path, read_gemini_env,
            };

            // Read .env file (environment variables)
            let env_path = get_gemini_env_path();
            if !env_path.exists() {
                return Err(AppError::localized(
                    "gemini.env.missing",
                    "Gemini .env 文件不存在",
                    "Gemini .env file not found",
                ));
            }

            let env_map = read_gemini_env()?;
            let env_json = env_to_json(&env_map);

            // Read settings.json file (MCP config etc.)
            let settings_path = get_gemini_settings_path();
            let config_obj = if settings_path.exists() {
                read_json_file(&settings_path)?
            } else {
                json!({})
            };

            // Return complete structure: { "env": {...}, "config": {...} }
            Ok(gemini_live_settings_from_env_json_and_config(
                &env_json, config_obj,
            ))
        }
        AppType::OpenCode => {
            use crate::opencode_config::{get_opencode_config_path, read_opencode_config};

            let config_path = get_opencode_config_path();
            if !config_path.exists() {
                return Err(AppError::localized(
                    "opencode.config.missing",
                    "OpenCode 配置文件不存在",
                    "OpenCode configuration file not found",
                ));
            }

            let config = read_opencode_config()?;
            Ok(config)
        }
        AppType::OpenClaw => {
            use crate::openclaw_config::{get_openclaw_config_path, read_openclaw_config};

            let config_path = get_openclaw_config_path();
            if !config_path.exists() {
                return Err(AppError::localized(
                    "openclaw.config.missing",
                    "OpenClaw 配置文件不存在",
                    "OpenClaw configuration file not found",
                ));
            }

            let config = read_openclaw_config()?;
            Ok(config)
        }
        AppType::Hermes => {
            let config_path = crate::hermes_config::get_hermes_config_path();
            if !config_path.exists() {
                return Err(AppError::localized(
                    "hermes.config.missing",
                    "Hermes 配置文件不存在",
                    "Hermes configuration file not found",
                ));
            }
            let yaml_config = crate::hermes_config::read_hermes_config()?;
            let config = crate::hermes_config::yaml_to_json(&yaml_config)?;
            Ok(config)
        }
    }
}

/// Import default configuration from live files
///
/// Returns `Ok(true)` if a provider was actually imported,
/// `Ok(false)` if skipped (providers already exist for this app).
pub fn import_default_config(state: &AppState, app_type: AppType) -> Result<bool, AppError> {
    // 允许 "只有官方 seed 预设" 的情况下继续导入 live：
    // - 启动编排顺序是先 import 后 seed，新用户启动时 providers 为空，导入照常
    // - 老用户已有非 seed provider，跳过导入（正确）
    // - 用户手动点 ProviderEmptyState 的导入按钮时，与官方 seed 共存而不被阻塞
    let has_non_official_seed_provider =
        state.db.has_non_official_seed_provider(app_type.as_str())?;
    if should_skip_manual_default_live_import(
        &AppKind::from(&app_type),
        has_non_official_seed_provider,
    ) {
        return Ok(false);
    }

    // 拒绝把"被代理接管的 Live"导入为供应商：接管期间 Live 里只有
    // PROXY_MANAGED 占位符和本地代理地址，不是用户的真实配置。一旦导入，
    // 它会成为 current provider（SSOT），后续"无备份恢复"路径会把占位符
    // 当真实配置写回 Live，永久卡在已失效的本地代理上。
    // 典型触发场景：代理接管开启时切换 app_config_dir 并重启，新数据库首启导入。
    if state
        .proxy_service
        .detect_takeover_in_live_config_for_app(&app_type)
    {
        return Err(AppError::localized(
            "provider.import.live_taken_over",
            "Live 配置当前处于代理接管状态（包含占位符），不能导入为供应商。请先关闭代理接管或恢复 Live 配置后重试。",
            "The live config is currently taken over by the proxy (contains placeholders) and cannot be imported as a provider. Disable proxy takeover or restore the live config first.",
        ));
    }

    let settings_config = match app_type {
        AppType::Codex => crate::codex_config::read_codex_live_settings()?,
        AppType::Claude => {
            let settings_path = get_claude_settings_path();
            if !settings_path.exists() {
                return Err(AppError::localized(
                    "claude.live.missing",
                    "Claude Code 配置文件不存在",
                    "Claude settings file is missing",
                ));
            }
            read_json_file::<Value>(&settings_path)?
        }
        AppType::ClaudeDesktop => {
            return Err(AppError::localized(
                "claude_desktop.import_unsupported",
                "Claude Desktop 3P 配置不能通过通用导入读取，请使用“从 Claude 导入兼容供应商”。",
                "Claude Desktop 3P config cannot be imported through the generic import flow. Use 'Import compatible providers from Claude' instead.",
            ));
        }
        AppType::Gemini => {
            use crate::gemini_config::{
                env_to_json, get_gemini_env_path, get_gemini_settings_path, read_gemini_env,
            };

            // Read .env file (environment variables)
            let env_path = get_gemini_env_path();
            if !env_path.exists() {
                return Err(AppError::localized(
                    "gemini.live.missing",
                    "Gemini 配置文件不存在",
                    "Gemini configuration file is missing",
                ));
            }

            let env_map = read_gemini_env()?;
            let env_json = env_to_json(&env_map);

            // Read settings.json file (MCP config etc.)
            let settings_path = get_gemini_settings_path();
            let config_obj = if settings_path.exists() {
                read_json_file(&settings_path)?
            } else {
                json!({})
            };

            // Return complete structure: { "env": {...}, "config": {...} }
            gemini_live_settings_from_env_json_and_config(&env_json, config_obj)
        }
        // OpenCode, OpenClaw and Hermes use additive mode and are handled by early return above
        AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => {
            unreachable!("additive mode apps are handled by early return")
        }
    };

    let settings_config =
        provider_default_live_import_settings(&AppKind::from(&app_type), settings_config);
    let provider = provider_from_default_live_settings(&app_type, settings_config);

    state.db.save_provider(app_type.as_str(), &provider)?;
    state
        .db
        .set_current_provider(app_type.as_str(), &provider.id)?;
    crate::settings::set_current_provider(&app_type, Some(provider.id.as_str()))?;

    Ok(true) // 真正导入了
}

/// Decide whether startup should auto-import the current live config as `default`.
///
/// This is intentionally stricter than the manual import path:
/// if the app already has any provider row at all (including official seeds),
/// startup must skip auto-import to avoid recreating `default` on each launch.
pub fn should_import_default_config_on_startup(
    state: &AppState,
    app_type: &AppType,
) -> Result<bool, AppError> {
    let has_any_provider = state.db.has_any_provider_for_app(app_type.as_str())?;
    Ok(!should_skip_startup_default_live_import(
        &AppKind::from(app_type),
        has_any_provider,
    ))
}

/// Write Gemini live configuration with authentication handling
pub(crate) fn write_gemini_live(provider: &Provider) -> Result<(), AppError> {
    use crate::gemini_config::{get_gemini_settings_path, write_gemini_env_atomic};

    // One-time auth type detection to avoid repeated detection
    let auth_type = detect_gemini_auth_type(provider);

    let env_map = gemini_env_string_map_from_settings(&provider.settings_config);

    // Prepare config to write to ~/.gemini/settings.json
    // Behavior:
    // - config is object: use it (merge with existing to preserve mcpServers etc.)
    // - config is null or absent: preserve existing file content
    let settings_path = get_gemini_settings_path();
    let mut config_to_write: Option<Value> = None;

    match gemini_live_config_object_from_settings(&provider.settings_config) {
        Ok(Some(config_value)) => {
            let existing_settings = if settings_path.exists() {
                read_json_file::<Value>(&settings_path).unwrap_or_else(|_| json!({}))
            } else {
                json!({})
            };
            config_to_write =
                gemini_live_settings_to_write(Some(existing_settings), Some(config_value));
        }
        Ok(None) => {
            // config is null or absent: don't modify existing settings.json (preserve mcpServers etc.)
        }
        Err(GeminiLiveConfigIssue::InvalidType) => {
            return Err(AppError::localized(
                "gemini.validation.invalid_config",
                "Gemini 配置格式错误: config 必须是对象或 null",
                "Gemini config invalid: config must be an object or null",
            ));
        }
    }

    // If no config specified or config is null, preserve existing file
    if config_to_write.is_none() && settings_path.exists() {
        config_to_write =
            gemini_live_settings_to_write(Some(read_json_file(&settings_path)?), None);
    }

    match auth_type {
        GeminiAuthType::GoogleOfficial => {
            // Google Official uses OAuth, no API key validation needed.
            // Write user's env vars as-is (e.g. GEMINI_MODEL, custom vars).
            write_gemini_env_atomic(&env_map)?;
        }
        GeminiAuthType::Packycode | GeminiAuthType::Generic => {
            // API Key mode -- require GEMINI_API_KEY
            crate::gemini_config::validate_gemini_settings_strict(&provider.settings_config)?;
            write_gemini_env_atomic(&env_map)?;
        }
    }

    if let Some(config_value) = config_to_write {
        write_json_file(&settings_path, &config_value)?;
    }

    // Set security.auth.selectedType based on auth type
    // - Google Official: OAuth mode
    // - All others: API Key mode
    match auth_type {
        GeminiAuthType::GoogleOfficial => ensure_google_oauth_security_flag(provider)?,
        GeminiAuthType::Packycode | GeminiAuthType::Generic => {
            crate::gemini_config::write_packycode_settings()?;
        }
    }

    Ok(())
}

/// Remove an OpenCode provider from the live configuration
///
/// This is specific to OpenCode's additive mode - removing a provider
/// from the opencode.json file.
pub(crate) fn remove_opencode_provider_from_live(provider_id: &str) -> Result<(), AppError> {
    use crate::opencode_config;

    // Check if OpenCode config directory exists
    if !opencode_config::get_opencode_dir().exists() {
        log::debug!("OpenCode config directory doesn't exist, skipping removal of '{provider_id}'");
        return Ok(());
    }

    opencode_config::remove_provider(provider_id)?;
    log::info!("OpenCode provider '{provider_id}' removed from live config");

    Ok(())
}

/// Import all providers from OpenCode live config to database
///
/// This imports existing providers from ~/.config/opencode/opencode.json
/// into the CC Switch database. Each provider found will be added to the
/// database with is_current set to false.
pub fn import_opencode_providers_from_live(state: &AppState) -> Result<usize, AppError> {
    use crate::opencode_config;

    let providers = opencode_config::get_typed_providers()?;
    if providers.is_empty() {
        return Ok(0);
    }

    let mut imported = 0;
    let existing_ids = state.db.get_provider_ids("opencode")?;

    for (id, config) in providers {
        // Skip if already exists in database
        if existing_ids.contains(&id) {
            log::debug!("OpenCode provider '{id}' already exists in database, skipping");
            continue;
        }

        let provider = match provider_from_opencode_live_config(&id, &config) {
            Ok(provider) => provider,
            Err(OpenCodeLiveImportIssue::Serialization(error)) => {
                log::warn!("Failed to serialize OpenCode provider '{id}': {error}");
                continue;
            }
        };

        // Save to database
        if let Err(e) = state.db.save_provider("opencode", &provider) {
            log::warn!("Failed to import OpenCode provider '{id}': {e}");
            continue;
        }

        imported += 1;
        log::info!("Imported OpenCode provider '{id}' from live config");
    }

    Ok(imported)
}

/// Import all providers from OpenClaw live config to database
///
/// This imports existing providers from ~/.openclaw/openclaw.json
/// into the CC Switch database. Each provider found will be added to the
/// database with is_current set to false.
pub fn import_openclaw_providers_from_live(state: &AppState) -> Result<usize, AppError> {
    use crate::openclaw_config;

    let providers = openclaw_config::get_typed_providers()?;
    if providers.is_empty() {
        return Ok(0);
    }

    let mut imported = 0;
    let existing_ids = state.db.get_provider_ids("openclaw")?;

    for (id, config) in providers {
        let provider = match provider_from_openclaw_live_config(&id, &config) {
            Ok(provider) => provider,
            Err(OpenClawLiveImportIssue::EmptyId) => {
                log::warn!("Skipping OpenClaw provider with empty id");
                continue;
            }
            Err(OpenClawLiveImportIssue::NoModels) => {
                log::warn!("Skipping OpenClaw provider '{id}': no models defined");
                continue;
            }
            Err(OpenClawLiveImportIssue::Serialization(error)) => {
                log::warn!("Failed to serialize OpenClaw provider '{id}': {error}");
                continue;
            }
        };

        // Skip if already exists in database
        if existing_ids.contains(&id) {
            log::debug!("OpenClaw provider '{id}' already exists in database, skipping");
            continue;
        }

        // Save to database
        if let Err(e) = state.db.save_provider("openclaw", &provider) {
            log::warn!("Failed to import OpenClaw provider '{id}': {e}");
            continue;
        }

        imported += 1;
        log::info!("Imported OpenClaw provider '{id}' from live config");
    }

    Ok(imported)
}

/// Import all providers from Hermes live config to database
///
/// This imports existing providers from ~/.hermes/config.yaml
/// into the CC Switch database. Each provider found will be added to the
/// database with is_current set to false.
pub fn import_hermes_providers_from_live(state: &AppState) -> Result<usize, AppError> {
    use crate::hermes_config;

    let providers = hermes_config::get_providers()?;
    if providers.is_empty() {
        return Ok(0);
    }

    let mut imported = 0;
    let existing_ids = state.db.get_provider_ids("hermes")?;

    for (name, config) in providers {
        let provider = match provider_from_hermes_live_config(&name, config) {
            Ok(provider) => provider,
            Err(HermesLiveImportIssue::EmptyName) => {
                log::warn!("Skipping Hermes provider with empty name");
                continue;
            }
        };

        // Skip if already exists in database
        if existing_ids.contains(&name) {
            log::debug!("Hermes provider '{name}' already exists in database, skipping");
            continue;
        }

        // Save to database
        if let Err(e) = state.db.save_provider("hermes", &provider) {
            log::warn!("Failed to import Hermes provider '{name}': {e}");
            continue;
        }

        imported += 1;
        log::info!("Imported Hermes provider '{name}' from live config");
    }

    Ok(imported)
}

/// Remove a Hermes provider from live config
///
/// This removes a specific provider from ~/.hermes/config.yaml
/// without affecting other providers in the file.
pub fn remove_hermes_provider_from_live(provider_id: &str) -> Result<(), AppError> {
    use crate::hermes_config;

    // Check if Hermes config directory exists
    if !hermes_config::get_hermes_dir().exists() {
        log::debug!("Hermes config directory doesn't exist, skipping removal of '{provider_id}'");
        return Ok(());
    }

    hermes_config::remove_provider(provider_id)?;
    log::info!("Hermes provider '{provider_id}' removed from live config");

    Ok(())
}

/// Remove an OpenClaw provider from live config
///
/// This removes a specific provider from ~/.openclaw/openclaw.json
/// without affecting other providers in the file.
pub fn remove_openclaw_provider_from_live(provider_id: &str) -> Result<(), AppError> {
    use crate::openclaw_config;

    // Check if OpenClaw config directory exists
    if !openclaw_config::get_openclaw_dir().exists() {
        log::debug!("OpenClaw config directory doesn't exist, skipping removal of '{provider_id}'");
        return Ok(());
    }

    openclaw_config::remove_provider(provider_id)?;
    log::info!("OpenClaw provider '{provider_id}' removed from live config");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core::api::ports::{
        openclaw_common_config_value_from_settings, openclaw_credential_parts_from_settings,
        opencode_common_config_value_from_settings, opencode_credential_parts_from_settings,
        OpenCodeCredentialIssue,
    };
    use serde_json::json;
    use toml_edit::DocumentMut;

    #[test]
    fn hermes_live_import_projects_provider_settings() {
        let settings = json!({
            "apiKey": "sk-hermes",
            "baseUrl": "https://hermes.example",
            "models": {
                "fast": "claude-sonnet"
            }
        });
        let imported_provider =
            provider_from_hermes_live_config("hermes-provider", settings.clone())
                .expect("import provider");
        assert_eq!(imported_provider.id, "hermes-provider");
        assert_eq!(imported_provider.name, "hermes-provider");
        assert_eq!(imported_provider.settings_config, settings);
        assert_eq!(
            imported_provider
                .meta
                .as_ref()
                .and_then(|meta| meta.live_config_managed),
            Some(true)
        );
        assert!(matches!(
            provider_from_hermes_live_config("   ", json!({})),
            Err(HermesLiveImportIssue::EmptyName)
        ));
    }

    #[test]
    fn openclaw_live_provider_shape_projects_provider_settings() {
        let credential_provider = Provider::with_id(
            "openclaw-credentials".to_string(),
            "OpenClaw Credentials".to_string(),
            json!({
                "apiKey": "sk-openclaw",
                "baseUrl": "https://openclaw.example"
            }),
            None,
        );
        let credentials =
            openclaw_credential_parts_from_settings(&credential_provider.settings_config);
        assert_eq!(credentials.api_key, Some("sk-openclaw"));
        assert_eq!(credentials.base_url, Some("https://openclaw.example"));
        let common_config = openclaw_common_config_value_from_settings(&json!({
            "apiKey": "sk-openclaw",
            "baseUrl": "https://openclaw.example",
            "api": {"chat": "/v1/chat/completions"},
            "models": {"fast": "claude-sonnet"}
        }));
        assert_eq!(
            common_config,
            json!({
                "api": {"chat": "/v1/chat/completions"},
                "models": {"fast": "claude-sonnet"}
            })
        );

        for settings in [
            json!({"baseUrl": []}),
            json!({"api": {"key": "sk-test"}}),
            json!({"models": {}}),
        ] {
            let provider = Provider::with_id(
                "openclaw-provider".to_string(),
                "OpenClaw Provider".to_string(),
                settings,
                None,
            );
            assert!(matches!(
                provider_openclaw_live_write_plan(&provider).config,
                OpenClawLiveWriteConfig::Raw { .. }
            ));
        }

        let typed_plan = provider_openclaw_live_write_plan(&credential_provider);
        assert!(matches!(
            typed_plan.config,
            OpenClawLiveWriteConfig::Typed(_)
        ));

        let typed_config = serde_json::from_value::<OpenClawProviderConfig>(json!({
            "baseUrl": "https://openclaw.example",
            "apiKey": "sk-openclaw",
            "models": [
                {
                    "id": "claude-sonnet-4",
                    "name": "Claude Sonnet 4"
                }
            ]
        }))
        .expect("typed openclaw provider config");
        let imported_provider = provider_from_openclaw_live_config("anthropic", &typed_config)
            .expect("import provider");
        assert_eq!(imported_provider.id, "anthropic");
        assert_eq!(imported_provider.name, "Claude Sonnet 4");
        assert_eq!(
            imported_provider.settings_config,
            serde_json::to_value(&typed_config).expect("serialized typed config")
        );
        assert_eq!(
            imported_provider
                .meta
                .as_ref()
                .and_then(|meta| meta.live_config_managed),
            Some(true)
        );
        let unnamed_config = serde_json::from_value::<OpenClawProviderConfig>(json!({
            "models": [
                {
                    "id": "claude-sonnet-4"
                }
            ]
        }))
        .expect("typed openclaw provider config");
        let imported_unnamed_provider =
            provider_from_openclaw_live_config("anthropic", &unnamed_config)
                .expect("import provider");
        assert_eq!(imported_unnamed_provider.name, "anthropic");
        assert!(matches!(
            provider_from_openclaw_live_config("   ", &typed_config),
            Err(OpenClawLiveImportIssue::EmptyId)
        ));
        assert!(matches!(
            provider_from_openclaw_live_config(
                "empty",
                &OpenClawProviderConfig {
                    models: Vec::new(),
                    ..typed_config.clone()
                }
            ),
            Err(OpenClawLiveImportIssue::NoModels)
        ));

        let raw_provider = Provider::with_id(
            "raw-openclaw".to_string(),
            "Raw OpenClaw".to_string(),
            json!({
                "models": {}
            }),
            None,
        );
        let plan = provider_openclaw_live_write_plan(&raw_provider);
        match plan.config {
            OpenClawLiveWriteConfig::Raw {
                config,
                parse_error,
            } => {
                assert_eq!(config, json!({"models": {}}));
                assert!(parse_error.contains("invalid type"));
            }
            other => panic!("expected raw OpenClaw write plan, got {other:?}"),
        }
        assert!(matches!(
            provider_openclaw_live_write_projection(&raw_provider).action,
            OpenClawLiveWriteAction::Raw { .. }
        ));

        let provider = Provider::with_id(
            "invalid-provider".to_string(),
            "Invalid Provider".to_string(),
            json!({"name": "Provider"}),
            None,
        );

        let plan = provider_openclaw_live_write_plan(&provider);
        assert!(matches!(plan.config, OpenClawLiveWriteConfig::Typed(_)));

        let provider = Provider::with_id(
            "invalid-provider".to_string(),
            "Invalid Provider".to_string(),
            json!("invalid"),
            None,
        );
        let plan = provider_openclaw_live_write_plan(&provider);
        assert!(matches!(
            plan.config,
            OpenClawLiveWriteConfig::Invalid { .. }
        ));
        match provider_openclaw_live_write_projection(&provider).action {
            OpenClawLiveWriteAction::Reject { message, .. } => {
                assert!(message.contains("OpenClaw provider 'invalid-provider'"));
                assert!(message.contains("baseUrl"));
            }
            other => panic!("expected reject OpenClaw write action, got {other:?}"),
        }
    }

    #[test]
    fn opencode_live_provider_fragment_projects_provider_settings() {
        let provider = Provider::with_id(
            "openai".to_string(),
            "OpenAI".to_string(),
            json!({
                "npm": "@ai-sdk/openai",
                "options": {"apiKey": "sk-test"}
            }),
            None,
        );
        let credentials = opencode_credential_parts_from_settings(&provider.settings_config)
            .expect("opencode credentials");
        assert_eq!(credentials.api_key, Some("sk-test"));
        assert_eq!(credentials.base_url, None);
        let common_config = opencode_common_config_value_from_settings(&json!({
            "npm": "@ai-sdk/openai",
            "options": {
                "apiKey": "sk-test",
                "baseURL": "https://opencode.example",
                "timeout": 30
            },
            "models": {"fast": "gpt-4o-mini"}
        }));
        assert_eq!(
            common_config,
            json!({
                "npm": "@ai-sdk/openai",
                "options": {"timeout": 30},
                "models": {"fast": "gpt-4o-mini"}
            })
        );
        let fragment = provider_opencode_live_provider_fragment(&provider);
        assert_eq!(fragment.config, provider.settings_config);
        assert!(!fragment.from_full_config);
        assert!(opencode_settings_have_live_provider_fields(
            &fragment.config
        ));
        let typed_config =
            serde_json::from_value::<OpenCodeProviderConfig>(provider.settings_config.clone())
                .expect("typed opencode provider config");
        let imported_provider =
            provider_from_opencode_live_config("openai", &typed_config).expect("import provider");
        assert_eq!(imported_provider.id, "openai");
        assert_eq!(imported_provider.name, "openai");
        assert_eq!(
            imported_provider.settings_config,
            serde_json::to_value(&typed_config).expect("serialized typed config")
        );
        assert_eq!(
            imported_provider
                .meta
                .as_ref()
                .and_then(|meta| meta.live_config_managed),
            Some(true)
        );
        let named_config = OpenCodeProviderConfig {
            name: Some("OpenAI".to_string()),
            ..typed_config.clone()
        };
        let imported_named_provider =
            provider_from_opencode_live_config("openai", &named_config).expect("import provider");
        assert_eq!(imported_named_provider.name, "OpenAI");
        let plan = provider_opencode_live_write_plan(&provider);
        assert!(!plan.from_full_config);
        assert!(matches!(plan.config, OpenCodeLiveWriteConfig::Typed(_)));
        assert!(opencode_settings_have_live_provider_fields(&json!({
            "npm": Value::Null
        })));
        let raw_provider = Provider::with_id(
            "raw".to_string(),
            "Raw".to_string(),
            json!({
                "npm": Value::Null
            }),
            None,
        );
        let plan = provider_opencode_live_write_plan(&raw_provider);
        match plan.config {
            OpenCodeLiveWriteConfig::Raw {
                config,
                parse_error,
            } => {
                assert_eq!(config, json!({"npm": Value::Null}));
                assert!(parse_error.contains("invalid type"));
            }
            other => panic!("expected raw OpenCode write plan, got {other:?}"),
        }
        assert!(matches!(
            provider_opencode_live_write_projection(&raw_provider).action,
            OpenCodeLiveWriteAction::Raw { .. }
        ));
        assert!(opencode_settings_have_live_provider_fields(&json!({
            "options": {}
        })));
        assert!(!opencode_settings_have_live_provider_fields(&json!({
            "name": "Provider"
        })));
        let invalid_provider = Provider::with_id(
            "invalid".to_string(),
            "Invalid".to_string(),
            json!({
                "name": "Provider"
            }),
            None,
        );
        let plan = provider_opencode_live_write_plan(&invalid_provider);
        assert!(matches!(
            plan.config,
            OpenCodeLiveWriteConfig::Invalid { .. }
        ));
        match provider_opencode_live_write_projection(&invalid_provider).action {
            OpenCodeLiveWriteAction::Reject { message, .. } => {
                assert!(message.contains("OpenCode provider 'invalid'"));
                assert!(message.contains("npm"));
            }
            other => panic!("expected reject OpenCode write action, got {other:?}"),
        }
        let missing_options_provider = Provider::with_id(
            "missing-options".to_string(),
            "Missing Options".to_string(),
            json!({}),
            None,
        );
        assert!(matches!(
            opencode_credential_parts_from_settings(&missing_options_provider.settings_config),
            Err(OpenCodeCredentialIssue::MissingOptions)
        ));

        let provider = Provider::with_id(
            "openai".to_string(),
            "OpenAI".to_string(),
            json!({
                "$schema": "https://opencode.ai/config.json",
                "provider": {
                    "openai": {
                        "npm": "@ai-sdk/openai",
                        "options": {"apiKey": "sk-nested"}
                    }
                }
            }),
            None,
        );
        let fragment = provider_opencode_live_provider_fragment(&provider);
        assert_eq!(
            fragment.config,
            json!({
                "npm": "@ai-sdk/openai",
                "options": {"apiKey": "sk-nested"}
            })
        );
        assert!(fragment.from_full_config);
        let plan = provider_opencode_live_write_plan(&provider);
        assert!(plan.from_full_config);
        assert!(matches!(plan.config, OpenCodeLiveWriteConfig::Typed(_)));

        let provider = Provider::with_id(
            "missing".to_string(),
            "Missing".to_string(),
            json!({
                "$schema": "https://opencode.ai/config.json",
                "provider": {}
            }),
            None,
        );
        let fragment = provider_opencode_live_provider_fragment(&provider);
        assert_eq!(fragment.config, provider.settings_config);
        assert!(fragment.from_full_config);
    }

    #[test]
    fn claude_common_config_apply_and_remove_roundtrip_for_non_overlapping_fields() {
        let settings = json!({
            "env": {
                "ANTHROPIC_API_KEY": "sk-test"
            }
        });
        let snippet = r#"{
  "includeCoAuthoredBy": false,
  "env": {
    "CLAUDE_CODE_USE_BEDROCK": "1"
  }
}"#;

        let applied =
            apply_common_config_to_settings(&AppType::Claude, &settings, snippet).unwrap();
        assert_eq!(applied["includeCoAuthoredBy"], json!(false));
        assert_eq!(applied["env"]["CLAUDE_CODE_USE_BEDROCK"], json!("1"));

        let stripped =
            remove_common_config_from_settings(&AppType::Claude, &applied, snippet).unwrap();
        assert_eq!(stripped, settings);
    }

    #[test]
    fn codex_common_config_apply_and_remove_roundtrip_for_non_overlapping_fields() {
        let settings = json!({
            "auth": {
                "OPENAI_API_KEY": "sk-test"
            },
            "config": "model_provider = \"openai\"\n[general]\nmodel = \"gpt-5\"\n"
        });
        let snippet = "[shared]\nreasoning = \"medium\"\n";

        let applied = apply_common_config_to_settings(&AppType::Codex, &settings, snippet).unwrap();
        let applied_config = applied["config"].as_str().unwrap_or_default();
        assert!(applied_config.contains("[shared]"));
        assert!(applied_config.contains("reasoning = \"medium\""));

        let stripped =
            remove_common_config_from_settings(&AppType::Codex, &applied, snippet).unwrap();
        assert_eq!(stripped, settings);
    }

    #[test]
    fn explicit_common_config_flag_overrides_legacy_subset_detection() {
        let mut provider = Provider::with_id(
            "claude-test".to_string(),
            "Claude Test".to_string(),
            json!({
                "includeCoAuthoredBy": false
            }),
            None,
        );
        provider.meta = Some(crate::provider::ProviderMeta {
            common_config_enabled: Some(false),
            ..Default::default()
        });

        assert!(
            !provider_uses_common_config(
                &AppType::Claude,
                &provider,
                Some(r#"{ "includeCoAuthoredBy": false }"#),
            ),
            "explicit false should win over legacy subset detection"
        );
    }

    #[test]
    fn claude_common_config_array_subset_detection_and_strip_preserve_extra_items() {
        let settings = json!({
            "allowedTools": ["tool1", "tool2"]
        });
        let snippet = r#"{
  "allowedTools": ["tool1"]
}"#;

        assert!(
            contains_common_config_snippet(&AppType::Claude, &settings, snippet),
            "array subset should be detected for legacy providers"
        );

        let stripped =
            remove_common_config_from_settings(&AppType::Claude, &settings, snippet).unwrap();
        assert_eq!(
            stripped,
            json!({
                "allowedTools": ["tool2"]
            })
        );
    }

    #[test]
    fn codex_common_config_array_subset_detection_and_strip_preserve_extra_items() {
        let settings = json!({
            "auth": {},
            "config": "allowed_tools = [\"tool1\", \"tool2\"]\n"
        });
        let snippet = "allowed_tools = [\"tool1\"]\n";

        assert!(
            contains_common_config_snippet(&AppType::Codex, &settings, snippet),
            "TOML array subset should be detected for legacy providers"
        );

        let stripped =
            remove_common_config_from_settings(&AppType::Codex, &settings, snippet).unwrap();
        assert_eq!(stripped["auth"], json!({}));
        let stripped_config = stripped["config"].as_str().unwrap_or_default();
        let parsed = stripped_config
            .parse::<DocumentMut>()
            .expect("stripped codex config should remain valid TOML");
        let allowed_tools = parsed["allowed_tools"]
            .as_array()
            .expect("allowed_tools should remain an array");
        let values: Vec<&str> = allowed_tools
            .iter()
            .map(|value| value.as_str().expect("tool id should be string"))
            .collect();
        assert_eq!(values, vec!["tool2"]);
    }

    #[test]
    fn gemini_common_config_subset_detection_reads_env_object() {
        let settings = json!({
            "env": {
                "SHARED_REGION": "us-central1",
                "EXTRA_FLAG": "enabled"
            }
        });
        let snippet = r#"{"SHARED_REGION": "us-central1"}"#;

        assert!(
            contains_common_config_snippet(&AppType::Gemini, &settings, snippet),
            "Gemini common config should be matched inside env"
        );
        assert!(
            !contains_common_config_snippet(&AppType::Gemini, &json!({"env": "invalid"}), snippet),
            "non-object env should not match Gemini common config"
        );
    }

    #[test]
    fn provider_effective_settings_apply_common_config_returns_warnings() {
        let mut provider = Provider::with_id(
            "claude-test".to_string(),
            "Claude Test".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            }),
            None,
        );
        provider.meta = Some(crate::provider::ProviderMeta {
            common_config_enabled: Some(true),
            ..Default::default()
        });

        let result = build_effective_settings_with_common_config(
            &AppType::Claude,
            &provider,
            Some(r#"{ "includeCoAuthoredBy": false }"#),
        );
        assert!(result.warnings.is_empty());
        assert_eq!(
            result.settings,
            json!({
                "includeCoAuthoredBy": false,
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            })
        );

        let result =
            build_effective_settings_with_common_config(&AppType::Claude, &provider, Some("{"));
        assert!(matches!(
            result.warnings.as_slice(),
            [ProviderEffectiveSettingsWarning::CommonConfigApply(_)]
        ));
        assert_eq!(result.settings, provider.settings_config);
    }

    #[test]
    fn provider_common_config_storage_normalization_requires_explicit_enablement() {
        let mut provider = Provider::with_id(
            "claude-test".to_string(),
            "Claude Test".to_string(),
            json!({
                "includeCoAuthoredBy": false,
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            }),
            None,
        );
        let snippet = r#"{ "includeCoAuthoredBy": false }"#;

        assert!(!provider_common_config_storage_normalization_requires_snippet(&provider));
        assert_eq!(
            normalize_provider_common_config_for_storage_from_snippet(
                &AppType::Claude,
                &provider,
                Some(snippet)
            )
            .expect("disabled storage normalization"),
            None
        );

        provider.meta = Some(crate::provider::ProviderMeta {
            common_config_enabled: Some(true),
            ..Default::default()
        });

        assert!(provider_common_config_storage_normalization_requires_snippet(&provider));
        assert_eq!(
            normalize_provider_common_config_for_storage_from_snippet(
                &AppType::Claude,
                &provider,
                Some("   ")
            )
            .expect("empty snippet"),
            None
        );
        assert_eq!(
            normalize_provider_common_config_for_storage_from_snippet(
                &AppType::Claude,
                &provider,
                Some(snippet)
            )
            .expect("enabled storage normalization"),
            Some(json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            }))
        );
    }

    #[test]
    fn provider_backfill_common_config_strip_keeps_original_on_invalid_snippet() {
        let mut provider = Provider::with_id(
            "claude-test".to_string(),
            "Claude Test".to_string(),
            json!({}),
            None,
        );
        provider.meta = Some(crate::provider::ProviderMeta {
            common_config_enabled: Some(true),
            ..Default::default()
        });

        let live_settings = json!({
            "includeCoAuthoredBy": false,
            "env": {
                "ANTHROPIC_API_KEY": "sk-test"
            }
        });
        let stripped = strip_common_config_from_live_settings_for_backfill(
            &AppType::Claude,
            &provider,
            live_settings.clone(),
            Some(r#"{ "includeCoAuthoredBy": false }"#),
        );
        assert_eq!(
            stripped,
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "sk-test"
                }
            })
        );

        let fallback = strip_common_config_from_live_settings_for_backfill(
            &AppType::Claude,
            &provider,
            live_settings.clone(),
            Some("{"),
        );
        assert_eq!(fallback, live_settings);
    }

    #[test]
    fn codex_write_live_snapshot_rejects_missing_auth_before_file_write() {
        let provider = Provider::with_id(
            "codex-missing-auth".to_string(),
            "Codex Missing Auth".to_string(),
            json!({"config": ""}),
            None,
        );

        let err = write_live_snapshot(&AppType::Codex, &provider)
            .expect_err("missing auth should be rejected before writing live files");

        assert!(err.to_string().contains("Codex 供应商配置缺少 'auth' 字段"));
    }

    #[test]
    fn gemini_write_live_rejects_invalid_config_shape_before_file_write() {
        let provider = Provider::with_id(
            "gemini-invalid-config".to_string(),
            "Gemini Invalid Config".to_string(),
            json!({
                "env": {"GEMINI_API_KEY": "AIza-test"},
                "config": "not-object"
            }),
            None,
        );

        let err = write_gemini_live(&provider)
            .expect_err("invalid Gemini config should be rejected before writing live files");

        assert!(matches!(
            err,
            AppError::Localized {
                key: "gemini.validation.invalid_config",
                ..
            }
        ));
    }

    #[test]
    fn default_live_import_classifies_codex_and_claude_providers() {
        let official = provider_from_default_live_settings(
            &AppType::Codex,
            json!({
                "auth": {
                    "tokens": {"id_token": "id-token"},
                    "auth_mode": "chatgpt"
                },
                "config": ""
            }),
        );
        assert_eq!(official.id, "default");
        assert_eq!(official.name, "default");
        assert_eq!(official.category.as_deref(), Some("official"));

        let custom = provider_from_default_live_settings(
            &AppType::Codex,
            json!({
                "auth": {"OPENAI_API_KEY": "sk-test"},
                "config": ""
            }),
        );
        assert_eq!(custom.category.as_deref(), Some("custom"));

        let bearer = provider_from_default_live_settings(
            &AppType::Codex,
            json!({
                "auth": {"tokens": {"id_token": "id-token"}},
                "config": r#"model_provider = "custom"

[model_providers.custom]
experimental_bearer_token = "bearer-token"
"#
            }),
        );
        assert_eq!(bearer.category.as_deref(), Some("custom"));

        let claude = provider_from_default_live_settings(
            &AppType::Claude,
            json!({"env": {"ANTHROPIC_API_KEY": "sk-test"}}),
        );
        assert_eq!(claude.category.as_deref(), Some("custom"));
    }

    #[test]
    fn codex_backfill_strategy_restores_custom_tokens_and_strips_official_unified_session() {
        let mut custom_provider = Provider::with_id(
            "codex-live-api-key".to_string(),
            "Codex Live API Key".to_string(),
            json!({
                "auth": {"OPENAI_API_KEY": "sk-test"},
                "config": ""
            }),
            None,
        );
        custom_provider.category = Some("custom".to_string());
        let custom_backfill_parts = provider_codex_backfill_parts(&custom_provider);
        assert!(custom_backfill_parts.restore_provider_token);
        assert!(!custom_backfill_parts.strip_unified_session_bucket);

        let mut official_provider = custom_provider.clone();
        official_provider.category = Some("official".to_string());
        let official_backfill_parts = provider_codex_backfill_parts(&official_provider);
        assert!(!official_backfill_parts.restore_provider_token);
        assert!(official_backfill_parts.strip_unified_session_bucket);

        let mut live_backfill_settings = json!({
            "auth": {},
            "config": r#"model_provider = "custom"

[model_providers.custom]
experimental_bearer_token = "live-token"
"#
        });
        adapter_restore_codex_settings_for_provider_backfill(
            &custom_provider,
            &mut live_backfill_settings,
        )
        .expect("restore codex provider backfill");
        assert_eq!(
            live_backfill_settings
                .get("auth")
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str),
            Some("live-token")
        );
        assert!(!live_backfill_settings
            .get("config")
            .and_then(Value::as_str)
            .expect("restored config")
            .contains("experimental_bearer_token"));

        let injected_unified_config =
            crate::codex_config::inject_codex_unified_session_bucket("").expect("inject");
        let mut official_unified_backfill = json!({"config": injected_unified_config});
        strip_codex_unified_session_bucket_for_provider_backfill(
            &official_provider,
            &mut official_unified_backfill,
        )
        .expect("strip official unified session bucket");
        assert!(!official_unified_backfill
            .get("config")
            .and_then(Value::as_str)
            .expect("official stripped config")
            .contains("model_provider"));

        let mut custom_unified_backfill = json!({
            "config": crate::codex_config::inject_codex_unified_session_bucket("").expect("inject")
        });
        strip_codex_unified_session_bucket_for_provider_backfill(
            &custom_provider,
            &mut custom_unified_backfill,
        )
        .expect("custom backfill no-op");
        assert!(custom_unified_backfill
            .get("config")
            .and_then(Value::as_str)
            .expect("custom retained config")
            .contains("model_provider"));
    }

    #[test]
    fn codex_live_snapshot_parts_keep_legacy_auth_shape_tolerance() {
        let invalid_shape = Provider::with_id(
            "codex-live-invalid".to_string(),
            "Codex Live Invalid".to_string(),
            json!("not-object"),
            None,
        );
        let missing_auth = Provider::with_id(
            "codex-live-missing-auth".to_string(),
            "Codex Live Missing Auth".to_string(),
            json!({"config": ""}),
            None,
        );
        let mut auth_not_object = Provider::with_id(
            "codex-live-auth-string".to_string(),
            "Codex Live Auth String".to_string(),
            json!({"auth": "sk-test"}),
            None,
        );
        auth_not_object.category = Some("custom".to_string());

        let snapshot_parts = provider_codex_live_snapshot_parts(&auth_not_object)
            .expect("snapshot keeps legacy auth shape tolerance");
        assert_eq!(snapshot_parts.category, Some("custom"));
        assert_eq!(snapshot_parts.auth, &json!("sk-test"));
        assert_eq!(snapshot_parts.config_text, None);
        assert!(matches!(
            provider_codex_live_snapshot_parts(&invalid_shape),
            Err(CodexLiveSnapshotIssue::NotObject)
        ));
        assert!(matches!(
            provider_codex_live_snapshot_parts(&missing_auth),
            Err(CodexLiveSnapshotIssue::MissingAuth)
        ));
    }

    #[test]
    fn codex_live_settings_with_model_catalog_only_overlays_object_settings() {
        let settings = json!({
            "modelCatalog": {
                "models": [
                    {"model": "deepseek-v4"},
                    {"id": "kimi-k2"}
                ]
            }
        });

        let live_config = codex_live_settings_with_model_catalog(
            json!({"auth": {}, "config": ""}),
            settings.get("modelCatalog").cloned(),
        );
        assert_eq!(
            live_config.get("modelCatalog"),
            settings.get("modelCatalog")
        );
        assert_eq!(
            codex_live_settings_with_model_catalog(json!({"auth": {}}), None),
            json!({"auth": {}})
        );
        assert_eq!(
            codex_live_settings_with_model_catalog(
                json!("not-object"),
                Some(json!({"models": []}))
            ),
            json!("not-object")
        );
    }

    #[test]
    fn codex_switch_backfill_preserves_stored_model_catalog_when_live_lacks_it() {
        // Reproduces the data-loss bug: switching away from a Codex provider
        // backfills the outgoing provider from Live, but Live's config.toml had
        // already lost its `model_catalog_json` projection (proxy cycle /
        // Codex.app rewrite), so `read_live_settings` reconstructs no catalog.
        // The stored mapping must survive the backfill.
        let mut provider = Provider::with_id(
            "deepseek".to_string(),
            "DeepSeek".to_string(),
            json!({
                "auth": { "OPENAI_API_KEY": "sk-deepseek" },
                "config": "model_provider = \"custom\"\nmodel = \"deepseek-v4-pro\"\n",
                "modelCatalog": {
                    "models": [
                        { "model": "deepseek-v4-pro", "contextWindow": 1_000_000 }
                    ]
                }
            }),
            None,
        );
        provider.category = Some("cn_official".to_string());

        // Live snapshot as captured during switch: no `modelCatalog` field.
        let live_settings = json!({
            "auth": { "OPENAI_API_KEY": "sk-deepseek" },
            "config": "model_provider = \"custom\"\nmodel = \"deepseek-v4-pro\"\n"
        });

        let result =
            restore_live_settings_for_provider_backfill(&AppType::Codex, &provider, live_settings);

        assert_eq!(
            result.get("modelCatalog"),
            provider.settings_config.get("modelCatalog"),
            "switch-away backfill must keep the DB-stored modelCatalog when Live has none"
        );
    }

    #[test]
    fn codex_switch_backfill_keeps_live_catalog_when_db_has_none() {
        // When the DB provider has no stored catalog, a catalog reconstructed
        // from Live (if any) should be left intact — the DB-preference overlay
        // must not wipe it.
        let mut provider = Provider::with_id(
            "deepseek".to_string(),
            "DeepSeek".to_string(),
            json!({
                "auth": { "OPENAI_API_KEY": "sk-deepseek" },
                "config": "model_provider = \"custom\"\nmodel = \"deepseek-v4-pro\"\n"
            }),
            None,
        );
        provider.category = Some("cn_official".to_string());

        let live_settings = json!({
            "auth": { "OPENAI_API_KEY": "sk-deepseek" },
            "config": "model_provider = \"custom\"\nmodel = \"deepseek-v4-pro\"\n",
            "modelCatalog": { "models": [ { "model": "deepseek-v4-pro" } ] }
        });

        let result = restore_live_settings_for_provider_backfill(
            &AppType::Codex,
            &provider,
            live_settings.clone(),
        );

        assert_eq!(
            result.get("modelCatalog"),
            live_settings.get("modelCatalog"),
            "backfill must keep the Live-reconstructed catalog when the DB has none"
        );
    }
}
