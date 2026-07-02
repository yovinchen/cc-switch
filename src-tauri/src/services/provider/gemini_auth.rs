//! Gemini authentication type detection
//!
//! Detects whether a Gemini provider uses PackyCode API Key, Google OAuth, or generic API Key.

use crate::error::AppError;
use crate::provider::Provider;
use crate::proxy_core::api::ports::{
    detect_gemini_auth_type as core_detect_gemini_auth_type, GeminiAuthType, GeminiAuthTypeInput,
};

/// Detect Gemini provider authentication type
///
/// One-time detection to avoid repeated calls to `is_packycode_gemini` and `is_google_official_gemini`.
///
/// # Returns
///
/// - `GeminiAuthType::GoogleOfficial`: Google official, uses OAuth
/// - `GeminiAuthType::Packycode`: PackyCode provider, uses API Key
/// - `GeminiAuthType::Generic`: Other generic providers, uses API Key
pub(crate) fn detect_gemini_auth_type(provider: &Provider) -> GeminiAuthType {
    core_detect_gemini_auth_type(GeminiAuthTypeInput {
        name: &provider.name,
        website_url: provider.website_url.as_deref(),
        partner_promotion_key: provider
            .meta
            .as_ref()
            .and_then(|meta| meta.partner_promotion_key.as_deref()),
        settings_config: &provider.settings_config,
    })
}

/// Detect if provider is Google Official Gemini (uses OAuth authentication)
///
/// Google Official Gemini uses OAuth personal authentication, no API Key needed.
///
/// This is a convenience wrapper around `detect_gemini_auth_type`.
pub(crate) fn is_google_official_gemini(provider: &Provider) -> bool {
    detect_gemini_auth_type(provider) == GeminiAuthType::GoogleOfficial
}

/// Ensure Google Official Gemini provider security flag is correctly set (OAuth mode)
///
/// Google Official Gemini uses OAuth personal authentication, no API Key needed.
///
/// # What it does
///
/// Writes to **`~/.gemini/settings.json`** (Gemini client config).
///
/// # Value set
///
/// ```json
/// {
///   "security": {
///     "auth": {
///       "selectedType": "oauth-personal"
///     }
///   }
/// }
/// ```
///
/// # OAuth authentication flow
///
/// 1. User switches to Google Official provider
/// 2. CC-Switch sets `selectedType = "oauth-personal"`
/// 3. User's first use of Gemini CLI will auto-open browser for OAuth login
/// 4. After successful login, credentials saved in Gemini credential store
/// 5. Subsequent requests auto-use saved credentials
///
/// # Error handling
///
/// If provider is not Google Official, function returns `Ok(())` immediately without any operation.
pub(crate) fn ensure_google_oauth_security_flag(provider: &Provider) -> Result<(), AppError> {
    if !is_google_official_gemini(provider) {
        return Ok(());
    }

    // Write to Gemini directory settings.json (~/.gemini/settings.json)
    use crate::gemini_config::write_google_oauth_settings;
    write_google_oauth_settings()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderMeta;
    use serde_json::json;

    #[test]
    fn detects_gemini_auth_type_from_provider_facts() {
        let generic = Provider::with_id(
            "gemini".to_string(),
            "Gemini".to_string(),
            json!({"env": {}}),
            None,
        );
        assert_eq!(detect_gemini_auth_type(&generic), GeminiAuthType::Generic);

        let google_official = Provider::with_id(
            "google-official".to_string(),
            "Google Gemini".to_string(),
            json!({"env": {}}),
            None,
        );
        assert_eq!(
            detect_gemini_auth_type(&google_official),
            GeminiAuthType::GoogleOfficial
        );

        let mut packy_partner = Provider::with_id(
            "packy-partner".to_string(),
            "Gemini Partner".to_string(),
            json!({"env": {}}),
            None,
        );
        packy_partner.meta = Some(ProviderMeta {
            partner_promotion_key: Some("packycode".to_string()),
            ..ProviderMeta::default()
        });
        assert_eq!(
            detect_gemini_auth_type(&packy_partner),
            GeminiAuthType::Packycode
        );
    }
}
