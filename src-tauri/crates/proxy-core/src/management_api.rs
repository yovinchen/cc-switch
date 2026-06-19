use super::domain::{AppKind, InterfaceKind};
use super::error::{ProxyCoreError, ProxyCoreResult};
use super::ports::AppModelListQuery;

pub fn validate_management_app_type(app_type: &str) -> ProxyCoreResult<()> {
    if app_type.trim().is_empty() {
        Err(ProxyCoreError::InvalidRequest(
            "app cannot be empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

pub fn validate_route_resolve_app_type(app_type: &str) -> ProxyCoreResult<()> {
    if app_type.trim().is_empty() {
        Err(ProxyCoreError::InvalidRequest(
            "appType/app_type cannot be empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

pub fn normalize_channel_id_path(channel_id: impl AsRef<str>) -> ProxyCoreResult<String> {
    let channel_id = channel_id.as_ref().trim().to_string();
    if channel_id.is_empty() {
        Err(ProxyCoreError::InvalidRequest(
            "channel_id cannot be empty".to_string(),
        ))
    } else {
        Ok(channel_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppModelCatalogRequest {
    pub app: AppKind,
    pub app_type: String,
    pub route_group: Option<String>,
    pub interface_kind: Option<InterfaceKind>,
}

impl AppModelCatalogRequest {
    pub fn from_parts(
        app_type: impl AsRef<str>,
        query: AppModelListQuery,
    ) -> ProxyCoreResult<Self> {
        let app_type = app_type.as_ref().trim().to_string();
        validate_management_app_type(&app_type)?;

        Ok(Self {
            app: AppKind::from(app_type.as_str()),
            app_type,
            route_group: query.route_group(),
            interface_kind: query.interface_kind(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_channel_id_path, validate_management_app_type, validate_route_resolve_app_type,
        AppModelCatalogRequest,
    };
    use crate::{AppModelListQuery, AppKind, InterfaceKind};

    #[test]
    fn validate_management_app_type_rejects_blank_values() {
        assert!(validate_management_app_type("claude").is_ok());
        let error = validate_management_app_type(" ").unwrap_err();

        assert_eq!(error.to_string(), "invalid proxy request: app cannot be empty");
    }

    #[test]
    fn validate_route_resolve_app_type_uses_route_request_message() {
        assert!(validate_route_resolve_app_type("codex").is_ok());
        let error = validate_route_resolve_app_type("").unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid proxy request: appType/app_type cannot be empty"
        );
    }

    #[test]
    fn normalize_channel_id_path_trims_and_rejects_blank_values() {
        assert_eq!(
            normalize_channel_id_path(" channel-a ").expect("normalize id"),
            "channel-a"
        );

        let error = normalize_channel_id_path(" ").unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid proxy request: channel_id cannot be empty"
        );
    }

    #[test]
    fn app_model_catalog_request_normalizes_path_and_query_aliases() {
        let query = serde_json::from_value::<AppModelListQuery>(serde_json::json!({
            "group": " beta ",
            "interface": "openai-responses"
        }))
        .expect("query");

        let request =
            AppModelCatalogRequest::from_parts(" claude ", query).expect("catalog request");

        assert_eq!(request.app, AppKind::Claude);
        assert_eq!(request.app_type, "claude");
        assert_eq!(request.route_group.as_deref(), Some("beta"));
        assert_eq!(request.interface_kind, Some(InterfaceKind::OpenAiResponses));
    }
}
