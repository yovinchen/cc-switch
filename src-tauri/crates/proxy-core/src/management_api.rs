use super::domain::{AppKind, InterfaceKind};
use super::error::{ProxyCoreError, ProxyCoreResult};
use super::ports::{AppChannelListQuery, AppModelListQuery, ChannelListQuery, GroupListQuery, RouteResolveRequest};

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
pub struct ManagementAppPathRequest {
    pub app_type: String,
}

impl ManagementAppPathRequest {
    pub fn from_path(app_type: impl AsRef<str>) -> ProxyCoreResult<Self> {
        let app_type = app_type.as_ref().trim().to_string();
        validate_management_app_type(&app_type)?;

        Ok(Self { app_type })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelPathRequest {
    pub channel_id: String,
}

impl ChannelPathRequest {
    pub fn from_path(channel_id: impl AsRef<str>) -> ProxyCoreResult<Self> {
        Ok(Self {
            channel_id: normalize_channel_id_path(channel_id)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RouteResolveManagementRequest {
    pub request: RouteResolveRequest,
}

impl RouteResolveManagementRequest {
    pub fn from_body(request: RouteResolveRequest) -> ProxyCoreResult<Self> {
        validate_route_resolve_app_type(&request.app_type)?;

        Ok(Self { request })
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

#[derive(Debug, Clone)]
pub struct AppChannelManagementRequest {
    pub app_type: String,
    pub route_request: Option<RouteResolveRequest>,
}

impl AppChannelManagementRequest {
    pub fn from_parts(
        app_type: impl AsRef<str>,
        query: AppChannelListQuery,
    ) -> ProxyCoreResult<Self> {
        let app_type = app_type.as_ref().trim().to_string();
        validate_management_app_type(&app_type)?;
        let route_request = if query.has_route_filters() {
            Some(query.into_route_request(app_type.clone()))
        } else {
            None
        };

        Ok(Self {
            app_type,
            route_request,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelListRequest {
    pub app_type: Option<String>,
}

impl ChannelListRequest {
    pub fn from_query(query: ChannelListQuery) -> ProxyCoreResult<Self> {
        Ok(Self {
            app_type: normalize_optional_management_app_type(query.app_type())?,
        })
    }

    pub fn app_type(&self) -> Option<&str> {
        self.app_type.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupListRequest {
    pub app_type: Option<String>,
}

impl GroupListRequest {
    pub fn from_query(query: GroupListQuery) -> ProxyCoreResult<Self> {
        Ok(Self {
            app_type: normalize_optional_management_app_type(query.app_type())?,
        })
    }

    pub fn app_type(&self) -> Option<&str> {
        self.app_type.as_deref()
    }
}

fn normalize_optional_management_app_type(app_type: Option<String>) -> ProxyCoreResult<Option<String>> {
    if let Some(app_type) = app_type {
        validate_management_app_type(&app_type)?;
        Ok(Some(app_type))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppChannelManagementRequest, AppModelCatalogRequest, ChannelListRequest,
        ChannelPathRequest, GroupListRequest, ManagementAppPathRequest, RouteResolveManagementRequest,
        normalize_channel_id_path, validate_management_app_type, validate_route_resolve_app_type,
    };
    use crate::{
        AppChannelListQuery, AppKind, AppModelListQuery, ChannelListQuery, GroupListQuery,
        InterfaceKind,
    };

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
    fn management_app_path_request_normalizes_path_app() {
        let request = ManagementAppPathRequest::from_path(" claude ").expect("request");

        assert_eq!(request.app_type, "claude");
    }

    #[test]
    fn channel_path_request_normalizes_path_id() {
        let request = ChannelPathRequest::from_path(" channel-a ").expect("request");

        assert_eq!(request.channel_id, "channel-a");
    }

    #[test]
    fn route_resolve_management_request_validates_body_app_type() {
        let request = RouteResolveManagementRequest::from_body(crate::RouteResolveRequest {
            app_type: "codex".to_string(),
            requested_model: None,
            interface_kind: None,
            route_group: None,
        })
        .expect("request");

        assert_eq!(request.request.app_type, "codex");

        let error = RouteResolveManagementRequest::from_body(crate::RouteResolveRequest {
            app_type: String::new(),
            requested_model: None,
            interface_kind: None,
            route_group: None,
        })
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid proxy request: appType/app_type cannot be empty"
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

    #[test]
    fn app_channel_management_request_builds_route_request_when_filters_exist() {
        let query = serde_json::from_value::<AppChannelListQuery>(serde_json::json!({
            "model": "sonnet",
            "group": "beta",
            "interface": "openai_responses"
        }))
        .expect("query");

        let request =
            AppChannelManagementRequest::from_parts(" claude ", query).expect("request");
        let route_request = request.route_request.expect("route request");

        assert_eq!(request.app_type, "claude");
        assert_eq!(route_request.app_type, "claude");
        assert_eq!(route_request.requested_model.as_deref(), Some("sonnet"));
        assert_eq!(
            route_request.interface_kind.as_deref(),
            Some("openai_responses")
        );
        assert_eq!(route_request.route_group.as_deref(), Some("beta"));
    }

    #[test]
    fn app_channel_management_request_keeps_plain_list_when_no_filters_exist() {
        let query = serde_json::from_value::<AppChannelListQuery>(serde_json::json!({}))
            .expect("query");

        let request = AppChannelManagementRequest::from_parts("claude", query).expect("request");

        assert_eq!(request.app_type, "claude");
        assert!(request.route_request.is_none());
    }

    #[test]
    fn channel_list_request_validates_optional_app_filter() {
        let query = serde_json::from_value::<ChannelListQuery>(serde_json::json!({
            "appType": " claude "
        }))
        .expect("query");
        let request = ChannelListRequest::from_query(query).expect("request");

        assert_eq!(request.app_type(), Some("claude"));

        let blank_query = serde_json::from_value::<ChannelListQuery>(serde_json::json!({
            "appType": " "
        }))
        .expect("query");
        let error = ChannelListRequest::from_query(blank_query).unwrap_err();

        assert_eq!(error.to_string(), "invalid proxy request: app cannot be empty");
    }

    #[test]
    fn group_list_request_preserves_absent_app_filter() {
        let query = serde_json::from_value::<GroupListQuery>(serde_json::json!({}))
            .expect("query");
        let request = GroupListRequest::from_query(query).expect("request");

        assert_eq!(request.app_type(), None);
    }
}
