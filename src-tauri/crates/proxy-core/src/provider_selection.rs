use super::circuit_breaker_key::provider_circuit_key;
use crate::error::{ProxyCoreError, ProxyCoreResult};
use std::collections::HashSet;

pub const AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE: &str =
    "需要先启用该应用的代理接管，再开启故障转移";
pub const AUTO_FAILOVER_EMPTY_QUEUE_WITHOUT_CURRENT_PROVIDER_MESSAGE: &str =
    "故障转移队列为空，且未设置当前供应商，无法开启故障转移";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSelectionCandidate {
    pub provider_id: String,
    pub configured: bool,
    pub available: bool,
}

impl ProviderSelectionCandidate {
    pub fn new(
        provider_id: impl Into<String>,
        configured: bool,
        available: bool,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            configured,
            available,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFailoverCircuitLookup {
    pub provider_id: String,
    pub circuit_key: Option<String>,
    pub configured: bool,
}

impl ProviderFailoverCircuitLookup {
    pub fn new(
        provider_id: impl Into<String>,
        circuit_key: Option<String>,
        configured: bool,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            circuit_key,
            configured,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSelectionInput {
    pub auto_failover_enabled: bool,
    pub current_provider_id: Option<String>,
    pub failover_candidates: Vec<ProviderSelectionCandidate>,
}

impl ProviderSelectionInput {
    pub fn current(current_provider_id: Option<String>) -> Self {
        Self {
            auto_failover_enabled: false,
            current_provider_id,
            failover_candidates: Vec::new(),
        }
    }

    pub fn failover(failover_candidates: Vec<ProviderSelectionCandidate>) -> Self {
        Self {
            auto_failover_enabled: true,
            current_provider_id: None,
            failover_candidates,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoFailoverToggleInput {
    pub requested_enabled: bool,
    pub app_proxy_enabled: bool,
    pub queued_provider_ids: Vec<String>,
    pub current_provider_id: Option<String>,
}

impl AutoFailoverToggleInput {
    pub fn new(
        requested_enabled: bool,
        app_proxy_enabled: bool,
        queued_provider_ids: Vec<String>,
        current_provider_id: Option<String>,
    ) -> Self {
        Self {
            requested_enabled,
            app_proxy_enabled,
            queued_provider_ids,
            current_provider_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoFailoverTogglePlan {
    pub auto_failover_enabled: bool,
    pub provider_id_to_add_to_queue: Option<String>,
    pub provider_id_to_switch_to: Option<String>,
}

impl AutoFailoverTogglePlan {
    pub fn disabled() -> Self {
        Self {
            auto_failover_enabled: false,
            provider_id_to_add_to_queue: None,
            provider_id_to_switch_to: None,
        }
    }

    pub fn enabled(
        provider_id_to_add_to_queue: Option<String>,
        provider_id_to_switch_to: String,
    ) -> Self {
        Self {
            auto_failover_enabled: true,
            provider_id_to_add_to_queue,
            provider_id_to_switch_to: Some(provider_id_to_switch_to),
        }
    }
}

pub fn plan_auto_failover_toggle(
    input: AutoFailoverToggleInput,
) -> ProxyCoreResult<AutoFailoverTogglePlan> {
    if !input.requested_enabled {
        return Ok(AutoFailoverTogglePlan::disabled());
    }

    if !input.app_proxy_enabled {
        return Err(ProxyCoreError::InvalidRequest(
            AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE.to_string(),
        ));
    }

    if let Some(provider_id) = input.queued_provider_ids.into_iter().next() {
        return Ok(AutoFailoverTogglePlan::enabled(None, provider_id));
    }

    let provider_id =
        input
            .current_provider_id
            .ok_or_else(|| {
                ProxyCoreError::InvalidRequest(
                    AUTO_FAILOVER_EMPTY_QUEUE_WITHOUT_CURRENT_PROVIDER_MESSAGE.to_string(),
                )
            })?;

    Ok(AutoFailoverTogglePlan::enabled(
        Some(provider_id.clone()),
        provider_id,
    ))
}

pub fn failover_switch_pending_key(app_type: &str, provider_id: &str) -> String {
    format!("{app_type}:{provider_id}")
}

pub fn provider_failover_circuit_lookups(
    app_type: &str,
    ordered_provider_ids: impl IntoIterator<Item = String>,
    configured_provider_ids: impl IntoIterator<Item = String>,
) -> Vec<ProviderFailoverCircuitLookup> {
    let configured_provider_ids = configured_provider_ids.into_iter().collect::<HashSet<_>>();

    ordered_provider_ids
        .into_iter()
        .map(|provider_id| {
            let configured = configured_provider_ids.contains(&provider_id);
            let circuit_key = configured.then(|| provider_circuit_key(app_type, &provider_id));
            ProviderFailoverCircuitLookup::new(provider_id, circuit_key, configured)
        })
        .collect()
}

pub fn provider_selection_candidate_from_failover_lookup(
    lookup: ProviderFailoverCircuitLookup,
    available: bool,
) -> ProviderSelectionCandidate {
    ProviderSelectionCandidate::new(lookup.provider_id, lookup.configured, available)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSelectionFailure {
    NoProvidersConfigured,
    AllProvidersCircuitOpen,
}

pub fn select_provider_ids(
    input: ProviderSelectionInput,
) -> Result<Vec<String>, ProviderSelectionFailure> {
    let mut selected = Vec::new();
    let mut total_providers = 0usize;
    let mut circuit_open_count = 0usize;

    if input.auto_failover_enabled {
        total_providers = input.failover_candidates.len();

        for candidate in input.failover_candidates {
            if !candidate.configured {
                continue;
            }

            if candidate.available {
                selected.push(candidate.provider_id);
            } else {
                circuit_open_count += 1;
            }
        }
    } else if let Some(current_provider_id) = input.current_provider_id {
        total_providers = 1;
        selected.push(current_provider_id);
    }

    if selected.is_empty() {
        if total_providers > 0 && circuit_open_count == total_providers {
            Err(ProviderSelectionFailure::AllProvidersCircuitOpen)
        } else {
            Err(ProviderSelectionFailure::NoProvidersConfigured)
        }
    } else {
        Ok(selected)
    }
}

pub fn current_provider_id_option_from_sources(
    settings_current_provider_id: Option<&str>,
    db_current_provider_id: Option<&str>,
) -> Option<String> {
    settings_current_provider_id
        .or(db_current_provider_id)
        .map(str::to_string)
}

pub fn current_provider_db_fallback_required(
    settings_current_provider_id: Option<&str>,
) -> bool {
    settings_current_provider_id.is_none()
}

pub fn current_provider_id_from_sources(
    settings_current_provider_id: Option<&str>,
    db_current_provider_id: Option<&str>,
) -> String {
    current_provider_id_option_from_sources(settings_current_provider_id, db_current_provider_id)
        .unwrap_or_default()
}

pub fn should_block_proxy_switch_to_provider_category(
    proxy_takeover_active: bool,
    provider_category: Option<&str>,
) -> bool {
    proxy_takeover_active && provider_category == Some("official")
}

pub fn should_attempt_restored_provider_switchback(
    proxy_takeover_active: bool,
    auto_failover_enabled: bool,
    proxy_service_running: bool,
    restored_sort_index: Option<usize>,
    current_sort_index: Option<usize>,
) -> bool {
    proxy_takeover_active
        && auto_failover_enabled
        && proxy_service_running
        && matches!(
            (restored_sort_index, current_sort_index),
            (Some(restored), Some(current)) if restored < current
        )
}

#[cfg(test)]
mod tests {
    use super::{
        current_provider_db_fallback_required, current_provider_id_from_sources,
        current_provider_id_option_from_sources, failover_switch_pending_key,
        plan_auto_failover_toggle, provider_failover_circuit_lookups,
        provider_selection_candidate_from_failover_lookup, select_provider_ids,
        should_attempt_restored_provider_switchback,
        should_block_proxy_switch_to_provider_category, AutoFailoverToggleInput,
        AutoFailoverTogglePlan, ProviderFailoverCircuitLookup, ProviderSelectionCandidate,
        ProviderSelectionFailure, ProviderSelectionInput,
        AUTO_FAILOVER_EMPTY_QUEUE_WITHOUT_CURRENT_PROVIDER_MESSAGE,
        AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE,
    };

    #[test]
    fn failover_disabled_selects_current_provider_only() {
        let selected = select_provider_ids(ProviderSelectionInput::current(Some(
            "provider-a".to_string(),
        )))
        .expect("selected providers");

        assert_eq!(selected, vec!["provider-a"]);
    }

    #[test]
    fn failover_enabled_uses_queue_order_and_skips_missing_providers() {
        let selected = select_provider_ids(ProviderSelectionInput::failover(vec![
            ProviderSelectionCandidate::new("missing", false, true),
            ProviderSelectionCandidate::new("provider-b", true, true),
            ProviderSelectionCandidate::new("provider-a", true, true),
        ]))
        .expect("selected providers");

        assert_eq!(selected, vec!["provider-b", "provider-a"]);
    }

    #[test]
    fn failover_circuit_lookups_preserve_queue_and_mark_missing_providers() {
        let lookups = provider_failover_circuit_lookups(
            "claude",
            vec![
                "missing".to_string(),
                "provider-b".to_string(),
                "provider-a".to_string(),
            ],
            vec!["provider-a".to_string(), "provider-b".to_string()],
        );

        assert_eq!(
            lookups,
            vec![
                ProviderFailoverCircuitLookup::new("missing", None, false),
                ProviderFailoverCircuitLookup::new(
                    "provider-b",
                    Some("claude:provider-b".to_string()),
                    true
                ),
                ProviderFailoverCircuitLookup::new(
                    "provider-a",
                    Some("claude:provider-a".to_string()),
                    true
                )
            ]
        );
    }

    #[test]
    fn failover_lookup_projects_selection_candidate() {
        let configured = provider_selection_candidate_from_failover_lookup(
            ProviderFailoverCircuitLookup::new(
                "provider-a",
                Some("claude:provider-a".to_string()),
                true,
            ),
            false,
        );
        let missing = provider_selection_candidate_from_failover_lookup(
            ProviderFailoverCircuitLookup::new("missing", None, false),
            true,
        );

        assert_eq!(
            configured,
            ProviderSelectionCandidate::new("provider-a", true, false)
        );
        assert_eq!(
            missing,
            ProviderSelectionCandidate::new("missing", false, true)
        );
    }

    #[test]
    fn auto_failover_toggle_plan_keeps_disable_side_effect_free() {
        let plan = plan_auto_failover_toggle(AutoFailoverToggleInput::new(
            false,
            true,
            vec!["provider-a".to_string()],
            Some("provider-b".to_string()),
        ))
        .expect("toggle plan");

        assert_eq!(plan, AutoFailoverTogglePlan::disabled());
    }

    #[test]
    fn auto_failover_toggle_plan_requires_proxy_takeover() {
        let error = plan_auto_failover_toggle(AutoFailoverToggleInput::new(
            true,
            false,
            vec!["provider-a".to_string()],
            None,
        ))
        .expect_err("disabled app should reject enable");

        assert_eq!(
            error.to_string(),
            format!(
                "invalid proxy request: {AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE}"
            )
        );
    }

    #[test]
    fn auto_failover_toggle_plan_switches_to_existing_p1() {
        let plan = plan_auto_failover_toggle(AutoFailoverToggleInput::new(
            true,
            true,
            vec!["provider-a".to_string(), "provider-b".to_string()],
            Some("provider-c".to_string()),
        ))
        .expect("toggle plan");

        assert_eq!(
            plan,
            AutoFailoverTogglePlan::enabled(None, "provider-a".to_string())
        );
    }

    #[test]
    fn auto_failover_toggle_plan_auto_adds_current_provider_for_empty_queue() {
        let plan = plan_auto_failover_toggle(AutoFailoverToggleInput::new(
            true,
            true,
            Vec::new(),
            Some("provider-current".to_string()),
        ))
        .expect("toggle plan");

        assert_eq!(
            plan,
            AutoFailoverTogglePlan::enabled(
                Some("provider-current".to_string()),
                "provider-current".to_string()
            )
        );
    }

    #[test]
    fn auto_failover_toggle_plan_rejects_empty_queue_without_current_provider() {
        let error = plan_auto_failover_toggle(AutoFailoverToggleInput::new(
            true,
            true,
            Vec::new(),
            None,
        ))
        .expect_err("empty queue without current provider should reject enable");

        assert_eq!(
            error.to_string(),
            format!(
                "invalid proxy request: {AUTO_FAILOVER_EMPTY_QUEUE_WITHOUT_CURRENT_PROVIDER_MESSAGE}"
            )
        );
    }

    #[test]
    fn failover_switch_pending_key_uses_app_and_provider_identity() {
        assert_eq!(
            failover_switch_pending_key("claude", "provider-a"),
            "claude:provider-a"
        );
    }

    #[test]
    fn failover_enabled_reports_all_configured_candidates_circuit_open() {
        let error = select_provider_ids(ProviderSelectionInput::failover(vec![
            ProviderSelectionCandidate::new("provider-a", true, false),
            ProviderSelectionCandidate::new("provider-b", true, false),
        ]))
        .expect_err("all providers should be circuit open");

        assert_eq!(error, ProviderSelectionFailure::AllProvidersCircuitOpen);
    }

    #[test]
    fn missing_queue_entries_keep_no_providers_failure_shape() {
        let error = select_provider_ids(ProviderSelectionInput::failover(vec![
            ProviderSelectionCandidate::new("missing", false, true),
            ProviderSelectionCandidate::new("provider-a", true, false),
        ]))
        .expect_err("missing queue entry prevents all-open classification");

        assert_eq!(error, ProviderSelectionFailure::NoProvidersConfigured);
    }

    #[test]
    fn current_provider_source_resolution_preserves_settings_priority() {
        assert!(!current_provider_db_fallback_required(Some("settings-provider")));
        assert!(current_provider_db_fallback_required(None));
        assert_eq!(
            current_provider_id_from_sources(Some("settings-provider"), Some("db-provider")),
            "settings-provider"
        );
        assert_eq!(
            current_provider_id_option_from_sources(Some("settings-provider"), Some("db-provider")),
            Some("settings-provider".to_string())
        );
        assert_eq!(
            current_provider_id_from_sources(None, Some("db-provider")),
            "db-provider"
        );
        assert_eq!(
            current_provider_id_option_from_sources(None, Some("db-provider")),
            Some("db-provider".to_string())
        );
        assert_eq!(current_provider_id_from_sources(None, None), "");
        assert_eq!(current_provider_id_option_from_sources(None, None), None);
    }

    #[test]
    fn current_provider_source_resolution_treats_empty_settings_value_as_present() {
        assert!(!current_provider_db_fallback_required(Some("")));
        assert_eq!(
            current_provider_id_from_sources(Some(""), Some("db-provider")),
            ""
        );
        assert_eq!(
            current_provider_id_option_from_sources(Some(""), Some("db-provider")),
            Some(String::new())
        );
    }

    #[test]
    fn official_provider_switch_block_only_applies_during_proxy_takeover() {
        assert!(should_block_proxy_switch_to_provider_category(
            true,
            Some("official")
        ));
        assert!(!should_block_proxy_switch_to_provider_category(
            false,
            Some("official")
        ));
        assert!(!should_block_proxy_switch_to_provider_category(
            true,
            Some("custom")
        ));
        assert!(!should_block_proxy_switch_to_provider_category(true, None));
    }

    #[test]
    fn restored_provider_switchback_requires_active_failover_and_higher_priority() {
        assert!(should_attempt_restored_provider_switchback(
            true,
            true,
            true,
            Some(1),
            Some(2)
        ));

        assert!(!should_attempt_restored_provider_switchback(
            false,
            true,
            true,
            Some(1),
            Some(2)
        ));
        assert!(!should_attempt_restored_provider_switchback(
            true,
            false,
            true,
            Some(1),
            Some(2)
        ));
        assert!(!should_attempt_restored_provider_switchback(
            true,
            true,
            false,
            Some(1),
            Some(2)
        ));
        assert!(!should_attempt_restored_provider_switchback(
            true,
            true,
            true,
            Some(2),
            Some(2)
        ));
        assert!(!should_attempt_restored_provider_switchback(
            true,
            true,
            true,
            Some(3),
            Some(2)
        ));
        assert!(!should_attempt_restored_provider_switchback(
            true,
            true,
            true,
            None,
            Some(2)
        ));
        assert!(!should_attempt_restored_provider_switchback(
            true,
            true,
            true,
            Some(1),
            None
        ));
    }
}
