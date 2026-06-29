//! Custom endpoints management
//!
//! Handles CRUD operations for provider custom endpoints.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::app_config::AppType;
use crate::error::AppError;
use crate::proxy_core::api::management::{
    custom_endpoint_url_issue_spec, custom_endpoint_url_key,
    normalize_custom_endpoint_url as core_normalize_custom_endpoint_url,
};
use crate::settings::CustomEndpoint;
use crate::store::AppState;

/// Get custom endpoints list for a provider
pub fn get_custom_endpoints(
    state: &AppState,
    app_type: AppType,
    provider_id: &str,
) -> Result<Vec<CustomEndpoint>, AppError> {
    let providers = state.db.get_all_providers(app_type.as_str())?;
    Ok(providers
        .get(provider_id)
        .map(|provider| provider.custom_endpoint_list())
        .unwrap_or_default())
}

/// Add a custom endpoint to a provider
pub fn add_custom_endpoint(
    state: &AppState,
    app_type: AppType,
    provider_id: &str,
    url: String,
) -> Result<(), AppError> {
    let normalized = normalize_custom_endpoint_url(&url)?;

    state
        .db
        .add_custom_endpoint(app_type.as_str(), provider_id, &normalized)?;
    Ok(())
}

/// Remove a custom endpoint from a provider
pub fn remove_custom_endpoint(
    state: &AppState,
    app_type: AppType,
    provider_id: &str,
    url: String,
) -> Result<(), AppError> {
    let normalized = custom_endpoint_url_key(&url);
    state
        .db
        .remove_custom_endpoint(app_type.as_str(), provider_id, &normalized)?;
    Ok(())
}

/// Update endpoint last used timestamp
pub fn update_endpoint_last_used(
    state: &AppState,
    app_type: AppType,
    provider_id: &str,
    url: String,
) -> Result<(), AppError> {
    let normalized = custom_endpoint_url_key(&url);

    // Get provider, update last_used, save back
    let mut providers = state.db.get_all_providers(app_type.as_str())?;
    if let Some(provider) = providers.get_mut(provider_id) {
        if provider.mark_custom_endpoint_last_used(&normalized, now_millis()) {
            state.db.save_provider(app_type.as_str(), provider)?;
        }
    }
    Ok(())
}

/// Get current timestamp in milliseconds
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn normalize_custom_endpoint_url(url: &str) -> Result<String, AppError> {
    core_normalize_custom_endpoint_url(url).map_err(|issue| {
        let spec = custom_endpoint_url_issue_spec(issue);
        AppError::localized(spec.key, spec.zh, spec.en)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_endpoint_url_and_maps_empty_url_error() {
        assert_eq!(
            normalize_custom_endpoint_url(" https://relay.example.com/v1/ ")
                .expect("normalized endpoint URL"),
            "https://relay.example.com/v1"
        );

        let empty_error = normalize_custom_endpoint_url(" / ").expect_err("empty URL");
        assert!(matches!(
            empty_error,
            AppError::Localized {
                key: "provider.endpoint.url_required",
                ..
            }
        ));
    }
}
