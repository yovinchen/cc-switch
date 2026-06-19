use super::domain::{AppKind, ChannelSpec, InterfaceKind};
use super::error::{ProxyCoreError, ProxyCoreResult};
use super::ports::{
    AppChannelListQuery, AppChannelListResponse, AppChannelResponse, AppChannelRouteResponse,
    AppModelListQuery, ChannelListQuery, ChannelModelsResponse, ChannelRecordResponse,
    ChannelRouteCandidate, ChannelRouteRejected, ChannelRouteSource, GroupListQuery,
    RouteGroupListResponse, RouteGroupSourceInput, RouteResolveRequest, RouteResolveResponse,
};

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

    pub fn channel_not_found_error(&self) -> ProxyCoreError {
        channel_not_found_error(&self.channel_id)
    }

    pub fn record_response<T>(
        &self,
        channel: Option<T>,
    ) -> ProxyCoreResult<ChannelRecordResponse<T>> {
        channel
            .map(ChannelRecordResponse::new)
            .ok_or_else(|| self.channel_not_found_error())
    }

    pub fn models_response<T>(
        &self,
        models: Option<Vec<T>>,
    ) -> ProxyCoreResult<ChannelModelsResponse<T>> {
        models
            .map(|models| ChannelModelsResponse::new(self.channel_id.clone(), models))
            .ok_or_else(|| self.channel_not_found_error())
    }

}

pub fn channel_not_found_message(channel_id: impl AsRef<str>) -> String {
    format!("channel not found: {}", channel_id.as_ref())
}

pub fn channel_not_found_error(channel_id: impl AsRef<str>) -> ProxyCoreError {
    ProxyCoreError::InvalidRequest(channel_not_found_message(channel_id))
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

    pub fn list_response<T>(
        &self,
        source: &ChannelRouteSource,
        channels: Vec<T>,
    ) -> AppChannelResponse<T, ChannelRouteCandidate, ChannelRouteRejected> {
        AppChannelResponse::List(AppChannelListResponse::from_route_source(
            self.app_type.clone(),
            source,
            channels,
        ))
    }

    pub fn route_response<T>(
        &self,
        response: RouteResolveResponse,
    ) -> AppChannelResponse<T, ChannelRouteCandidate, ChannelRouteRejected> {
        AppChannelResponse::Route(AppChannelRouteResponse::from_route_resolve(response))
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

    pub fn app_scope<I, S>(&self, all_app_types: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        match &self.app_type {
            Some(app_type) => vec![app_type.clone()],
            None => all_app_types.into_iter().map(Into::into).collect(),
        }
    }

    pub fn source_input(
        &self,
        app_type: impl Into<String>,
        source: &ChannelRouteSource,
        channels: impl IntoIterator<Item = ChannelSpec>,
    ) -> RouteGroupSourceInput {
        RouteGroupSourceInput::from_channel_specs(app_type, source, channels)
    }

    pub fn response(
        &self,
        sources: impl IntoIterator<Item = RouteGroupSourceInput>,
    ) -> RouteGroupListResponse {
        RouteGroupListResponse::from_sources(self.app_type.clone(), sources)
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
        ChannelPathRequest, GroupListRequest, ManagementAppPathRequest,
        RouteResolveManagementRequest, channel_not_found_message, normalize_channel_id_path,
        validate_management_app_type, validate_route_resolve_app_type,
    };
    use crate::{
        AppChannelListQuery, AppKind, AppModelListQuery, ChannelHealthPolicy, ChannelListQuery,
        ChannelOverrides, ChannelRouteSource, ChannelSpec, ChannelStatus, GroupListQuery,
        InterfaceKind, RetryPolicy, RouteResolveResponse, UpstreamEndpoint,
    };
    use serde_json::json;

    fn channel_spec(id: &str, app: AppKind, groups: Vec<String>) -> ChannelSpec {
        ChannelSpec {
            id: id.to_string(),
            provider_id: "provider-a".to_string(),
            app,
            name: id.to_string(),
            status: ChannelStatus::Enabled,
            endpoint: UpstreamEndpoint {
                base_url: "https://api.example.com".to_string(),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::AnthropicMessages,
            auth_profile: None,
            models: Vec::new(),
            groups,
            priority: 0,
            weight: 100,
            retry_policy: RetryPolicy::default(),
            health_policy: ChannelHealthPolicy::default(),
            overrides: ChannelOverrides::default(),
            tags: Vec::new(),
            metadata: json!({}),
            source_ref: None,
            needs_review: false,
            review_reasons: Vec::new(),
        }
    }

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
    fn channel_path_request_centralizes_not_found_message() {
        let request = ChannelPathRequest::from_path(" channel-a ").expect("request");

        assert_eq!(
            channel_not_found_message(&request.channel_id),
            "channel not found: channel-a"
        );
        assert_eq!(
            request.channel_not_found_error().to_string(),
            "invalid proxy request: channel not found: channel-a"
        );
    }

    #[test]
    fn channel_path_request_wraps_optional_record_response() {
        let request = ChannelPathRequest::from_path("channel-a").expect("request");
        let response = request
            .record_response(Some("record-a"))
            .expect("record response");

        assert_eq!(response.channel, "record-a");

        let error = request.record_response::<&str>(None).unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid proxy request: channel not found: channel-a"
        );
    }

    #[test]
    fn channel_path_request_wraps_models_response() {
        let request = ChannelPathRequest::from_path("channel-a").expect("request");

        let response = request
            .models_response(Some(vec!["sonnet"]))
            .expect("models response");
        assert_eq!(response.channel_id, "channel-a");
        assert_eq!(response.models, vec!["sonnet"]);

        let error = request.models_response::<&str>(None).unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid proxy request: channel not found: channel-a"
        );
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
    fn app_channel_management_request_builds_list_response() {
        let query = serde_json::from_value::<AppChannelListQuery>(serde_json::json!({}))
            .expect("query");
        let request = AppChannelManagementRequest::from_parts("claude", query).expect("request");

        let response = request.list_response(&ChannelRouteSource::MaterializedChannels, vec![
            "channel-a",
        ]);
        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["source"], "materialized_channels");
        assert_eq!(value["channels"][0], "channel-a");
        assert!(value.get("rejected").is_none());
    }

    #[test]
    fn app_channel_management_request_builds_route_response() {
        let query = serde_json::from_value::<AppChannelListQuery>(serde_json::json!({
            "model": "sonnet"
        }))
        .expect("query");
        let request = AppChannelManagementRequest::from_parts("claude", query).expect("request");

        let response: crate::AppChannelResponse<&str, _, _> =
            request.route_response(RouteResolveResponse {
                app_type: "claude".to_string(),
                requested_model: Some("sonnet".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: "default".to_string(),
                source: ChannelRouteSource::LegacyProjection,
                candidates: Vec::new(),
                rejected: Vec::new(),
            });
        let value = serde_json::to_value(response).expect("serialize response");

        assert_eq!(value["appType"], "claude");
        assert_eq!(value["source"], "legacy_projection");
        assert_eq!(value["requestedModel"], "sonnet");
        assert_eq!(value["interfaceKind"], "anthropic_messages");
        assert_eq!(value["routeGroup"], "default");
        assert_eq!(value["channels"], json!([]));
        assert_eq!(value["rejected"], json!([]));
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

    #[test]
    fn group_list_request_builds_app_scope_from_filter_or_host_apps() {
        let query = serde_json::from_value::<GroupListQuery>(serde_json::json!({}))
            .expect("query");
        let request = GroupListRequest::from_query(query).expect("request");

        assert_eq!(
            request.app_scope(["claude", "codex", "custom"]),
            vec![
                "claude".to_string(),
                "codex".to_string(),
                "custom".to_string()
            ]
        );

        let query = serde_json::from_value::<GroupListQuery>(serde_json::json!({
            "appType": " codex "
        }))
        .expect("query");
        let request = GroupListRequest::from_query(query).expect("request");

        assert_eq!(request.app_scope(["claude", "codex"]), vec!["codex"]);
    }

    #[test]
    fn group_list_request_wraps_sources_into_route_group_response() {
        let query = serde_json::from_value::<GroupListQuery>(serde_json::json!({
            "appType": "claude"
        }))
        .expect("query");
        let request = GroupListRequest::from_query(query).expect("request");

        let response = request.response(vec![request.source_input(
            "claude",
            &ChannelRouteSource::MaterializedChannels,
            vec![
                channel_spec("channel-a", AppKind::Claude, vec![]),
                channel_spec("channel-b", AppKind::Claude, vec!["beta".to_string()]),
            ],
        )]);

        assert_eq!(response.app_type.as_deref(), Some("claude"));
        assert_eq!(response.sources, vec!["materialized_channels"]);
        assert_eq!(response.groups.len(), 2);
        assert_eq!(response.groups[0].name, "beta");
        assert_eq!(response.groups[0].app_types, vec!["claude"]);
        assert_eq!(response.groups[0].channel_count, 1);
        assert_eq!(response.groups[1].name, "default");
        assert_eq!(response.groups[1].app_types, vec!["claude"]);
        assert_eq!(response.groups[1].channel_count, 1);
    }
}
