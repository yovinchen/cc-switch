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
        current_provider_id_from_sources, current_provider_id_option_from_sources,
        select_provider_ids, should_attempt_restored_provider_switchback,
        should_block_proxy_switch_to_provider_category, ProviderSelectionCandidate,
        ProviderSelectionFailure, ProviderSelectionInput,
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
