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

#[cfg(test)]
mod tests {
    use super::{
        select_provider_ids, ProviderSelectionCandidate, ProviderSelectionFailure,
        ProviderSelectionInput,
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
}
