use crate::deeplink::{import_provider_from_deeplink, DeepLinkImportRequest};
use crate::models::deeplink::{ConfigImportRequest, DeepLinkRequest, ProviderImportRequest};
use crate::services::DeepLinkService;
use crate::store::AppState;
use tauri::State;

/// Parse a deep link URL and return the parsed request for frontend confirmation
#[tauri::command]
pub fn parse_deeplink(url: String) -> Result<DeepLinkRequest, String> {
    log::info!("Parsing deep link URL: {url}");
    DeepLinkRequest::from_url(&url).map_err(|e| {
        log::error!("Failed to parse deep link URL: {e}");
        e.to_string()
    })
}

/// Import a provider from a deep link request (after user confirmation)
#[tauri::command]
pub fn import_from_deeplink(
    state: State<AppState>,
    request: DeepLinkRequest,
) -> Result<String, String> {
    match request {
        DeepLinkRequest::Provider(provider_request) => {
            log::info!(
                "Importing provider from deep link: {} for app {}",
                provider_request.name,
                provider_request.app
            );

            let provider_id = import_provider_from_deeplink(
                &state,
                into_legacy_provider_request(provider_request),
            )
            .map_err(|e| {
                log::error!("Failed to import provider from deep link: {e}");
                e.to_string()
            })?;

            log::info!("Successfully imported provider with ID: {provider_id}");
            Ok(provider_id)
        }
        DeepLinkRequest::Config(config_request) => {
            log::info!(
                "Importing provider config from deep link for app {}",
                config_request.app
            );

            let provider =
                DeepLinkService::handle_config_import(&state, config_request).map_err(|e| {
                    log::error!("Failed to import provider config from deep link: {e}");
                    e.to_string()
                })?;
            let provider_id = provider.id.clone();

            log::info!("Successfully imported provider from config with ID: {provider_id}");
            Ok(provider_id)
        }
    }
}

/// Import a provider configuration (config resource) from deep link
#[tauri::command]
pub fn import_config_from_deeplink(
    state: State<AppState>,
    request: ConfigImportRequest,
) -> Result<String, String> {
    log::info!(
        "Importing provider from config deep link for app {}",
        request.app
    );

    let provider = DeepLinkService::handle_config_import(&state, request).map_err(|e| {
        log::error!("Failed to import config from deep link: {e}");
        e.to_string()
    })?;

    let provider_id = provider.id.clone();
    log::info!("Successfully imported provider with ID: {provider_id}");

    Ok(provider_id)
}

fn into_legacy_provider_request(request: ProviderImportRequest) -> DeepLinkImportRequest {
    DeepLinkImportRequest {
        version: request.version,
        resource: request.resource,
        app: request.app,
        name: request.name,
        homepage: request.homepage,
        endpoint: request.endpoint,
        api_key: request.api_key,
        model: request.model,
        notes: request.notes,
    }
}
