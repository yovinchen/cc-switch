use super::domain::{AppKind, ChannelSpec, InterfaceKind, ProviderSpec};
use super::error::{ProxyCoreError, ProxyCoreResult};
use super::ports::{
    AppChannelListQuery, AppChannelListResponse, AppChannelResponse, AppChannelRouteResponse,
    AppListResponse, AppModelListQuery, AppSummaryInput, ChannelDeleteResponse, ChannelListQuery,
    ChannelListResponse, ChannelMigrationMaterializeResponse, ChannelMigrationPreviewResponse,
    ChannelModelsResponse, ChannelRecordResponse, ChannelRouteCandidate, ChannelRouteRejected,
    ChannelRouteSource, ChannelTestInput, ChannelTestResponse, CurrentRouteProviderSummaryInput,
    CurrentRouteResponse, GroupListQuery, HealthCheckResponse, ProviderListResponse,
    ProxyChannelWriteRequest, ProxyStatusResponse, RouteGroupListResponse, RouteGroupSourceInput,
    RouteResolveRequest, RouteResolveResponse,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AppListRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppListSource {
    pub apps: Vec<AppSummaryInput>,
}

impl AppListSource {
    pub fn new(apps: Vec<AppSummaryInput>) -> Self {
        Self { apps }
    }
}

impl AppListRequest {
    pub fn new() -> Self {
        Self
    }

    pub fn response(&self, apps: Vec<AppSummaryInput>) -> AppListResponse {
        AppListResponse::from_app_inputs(apps)
    }

    pub fn response_from_source(&self, source: AppListSource) -> AppListResponse {
        self.response(source.apps)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HealthCheckRequest;

impl HealthCheckRequest {
    pub fn new() -> Self {
        Self
    }

    pub fn response(&self, timestamp: impl Into<String>) -> HealthCheckResponse {
        HealthCheckResponse::healthy(timestamp)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProxyStatusRequest;

impl ProxyStatusRequest {
    pub fn new() -> Self {
        Self
    }

    pub fn response<T>(&self, status: T) -> ProxyStatusResponse<T> {
        ProxyStatusResponse::new(status)
    }
}

#[derive(Debug, Clone)]
pub struct ChannelCreateRequest {
    request: ProxyChannelWriteRequest,
}

impl ChannelCreateRequest {
    pub fn from_body(request: ProxyChannelWriteRequest) -> Self {
        Self { request }
    }

    pub fn into_body(self) -> ProxyChannelWriteRequest {
        self.request
    }

    pub fn record_response<T>(&self, channel: T) -> ChannelRecordResponse<T> {
        ChannelRecordResponse::new(channel)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagementAppPathRequest {
    pub app_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentRouteSource<T> {
    pub active_target: Option<T>,
    pub configured_provider: Option<CurrentRouteProviderSummaryInput>,
}

impl<T> CurrentRouteSource<T> {
    pub fn new(
        active_target: Option<T>,
        configured_provider: Option<CurrentRouteProviderSummaryInput>,
    ) -> Self {
        Self {
            active_target,
            configured_provider,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelMigrationPreviewSource<T> {
    pub channels: Vec<T>,
    pub duplicate_count: usize,
    pub needs_review_count: usize,
}

impl<T> ChannelMigrationPreviewSource<T> {
    pub fn new(channels: Vec<T>, duplicate_count: usize, needs_review_count: usize) -> Self {
        Self {
            channels,
            duplicate_count,
            needs_review_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelMigrationMaterializeSource {
    pub previewed_channels: usize,
    pub inserted_channels: usize,
    pub inserted_models: usize,
    pub inserted_health_rows: usize,
    pub duplicate_count: usize,
    pub needs_review_count: usize,
}

impl ChannelMigrationMaterializeSource {
    pub fn new(
        previewed_channels: usize,
        inserted_channels: usize,
        inserted_models: usize,
        inserted_health_rows: usize,
        duplicate_count: usize,
        needs_review_count: usize,
    ) -> Self {
        Self {
            previewed_channels,
            inserted_channels,
            inserted_models,
            inserted_health_rows,
            duplicate_count,
            needs_review_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderListSource {
    pub providers: Vec<ProviderSpec>,
    pub current_provider: Option<String>,
    pub failover_provider_ids: Vec<String>,
    pub route_candidate_ids: Vec<String>,
}

impl ProviderListSource {
    pub fn new(
        providers: impl IntoIterator<Item = ProviderSpec>,
        current_provider: Option<String>,
        failover_provider_ids: Vec<String>,
        route_candidate_ids: Vec<String>,
    ) -> Self {
        Self {
            providers: providers.into_iter().collect(),
            current_provider,
            failover_provider_ids,
            route_candidate_ids,
        }
    }
}

impl ManagementAppPathRequest {
    pub fn from_path(app_type: impl AsRef<str>) -> ProxyCoreResult<Self> {
        let app_type = app_type.as_ref().trim().to_string();
        validate_management_app_type(&app_type)?;

        Ok(Self { app_type })
    }

    pub fn current_route_response<T>(
        &self,
        active_target: Option<T>,
        configured_provider: Option<CurrentRouteProviderSummaryInput>,
    ) -> CurrentRouteResponse<T> {
        CurrentRouteResponse::from_inputs(
            self.app_type.clone(),
            active_target,
            configured_provider,
        )
    }

    pub fn current_route_response_from_source<T>(
        &self,
        source: CurrentRouteSource<T>,
    ) -> CurrentRouteResponse<T> {
        self.current_route_response(source.active_target, source.configured_provider)
    }

    pub fn provider_list_response(
        &self,
        providers: impl IntoIterator<Item = ProviderSpec>,
        current_provider: Option<&str>,
        failover_provider_ids: &[String],
        route_candidate_ids: &[String],
    ) -> ProviderListResponse {
        ProviderListResponse::from_provider_specs(
            self.app_type.clone(),
            providers,
            current_provider,
            failover_provider_ids,
            route_candidate_ids,
        )
    }

    pub fn provider_list_response_from_source(
        &self,
        source: ProviderListSource,
    ) -> ProviderListResponse {
        self.provider_list_response(
            source.providers,
            source.current_provider.as_deref(),
            &source.failover_provider_ids,
            &source.route_candidate_ids,
        )
    }

    pub fn migration_preview_response<T>(
        &self,
        channels: Vec<T>,
        duplicate_count: usize,
        needs_review_count: usize,
    ) -> ChannelMigrationPreviewResponse<T> {
        ChannelMigrationPreviewResponse::new(
            self.app_type.clone(),
            channels,
            duplicate_count,
            needs_review_count,
        )
    }

    pub fn migration_preview_response_from_source<T>(
        &self,
        source: ChannelMigrationPreviewSource<T>,
    ) -> ChannelMigrationPreviewResponse<T> {
        self.migration_preview_response(
            source.channels,
            source.duplicate_count,
            source.needs_review_count,
        )
    }

    pub fn migration_materialize_response(
        &self,
        previewed_channels: usize,
        inserted_channels: usize,
        inserted_models: usize,
        inserted_health_rows: usize,
        duplicate_count: usize,
        needs_review_count: usize,
    ) -> ChannelMigrationMaterializeResponse {
        ChannelMigrationMaterializeResponse::new(
            self.app_type.clone(),
            previewed_channels,
            inserted_channels,
            inserted_models,
            inserted_health_rows,
            duplicate_count,
            needs_review_count,
        )
    }

    pub fn migration_materialize_response_from_source(
        &self,
        source: ChannelMigrationMaterializeSource,
    ) -> ChannelMigrationMaterializeResponse {
        self.migration_materialize_response(
            source.previewed_channels,
            source.inserted_channels,
            source.inserted_models,
            source.inserted_health_rows,
            source.duplicate_count,
            source.needs_review_count,
        )
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

    pub fn delete_response(&self, deleted: bool) -> ChannelDeleteResponse {
        ChannelDeleteResponse::new(self.channel_id.clone(), deleted)
    }

    pub fn test_response(&self, input: ChannelTestInput) -> ChannelTestResponse {
        ChannelTestResponse::from_input(input)
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
    route_request: Option<RouteResolveRequest>,
}

#[derive(Debug, Clone)]
pub enum AppChannelManagementPlan {
    Route(RouteResolveRequest),
    List { app_type: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppChannelListSource<T> {
    pub source: ChannelRouteSource,
    pub channels: Vec<T>,
}

impl<T> AppChannelListSource<T> {
    pub fn new(source: ChannelRouteSource, channels: Vec<T>) -> Self {
        Self { source, channels }
    }
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

    pub fn plan(&self) -> AppChannelManagementPlan {
        match &self.route_request {
            Some(route_request) => AppChannelManagementPlan::Route(route_request.clone()),
            None => AppChannelManagementPlan::List {
                app_type: self.app_type.clone(),
            },
        }
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

    pub fn response_from_list_source<T>(
        &self,
        source: AppChannelListSource<T>,
    ) -> AppChannelResponse<T, ChannelRouteCandidate, ChannelRouteRejected> {
        self.list_response(&source.source, source.channels)
    }

    pub fn route_response<T>(
        &self,
        response: RouteResolveResponse,
    ) -> AppChannelResponse<T, ChannelRouteCandidate, ChannelRouteRejected> {
        AppChannelResponse::Route(AppChannelRouteResponse::from_route_resolve(response))
    }

    pub fn response_from_route_resolution<T>(
        &self,
        response: RouteResolveResponse,
    ) -> AppChannelResponse<T, ChannelRouteCandidate, ChannelRouteRejected> {
        self.route_response(response)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelListRequest {
    pub app_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelListPlan {
    App { app_type: String },
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelListSource<T> {
    pub channels: Vec<T>,
}

impl<T> ChannelListSource<T> {
    pub fn new(channels: Vec<T>) -> Self {
        Self { channels }
    }
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

    pub fn plan(&self) -> ChannelListPlan {
        match &self.app_type {
            Some(app_type) => ChannelListPlan::App {
                app_type: app_type.clone(),
            },
            None => ChannelListPlan::All,
        }
    }

    pub fn response<T>(&self, channels: Vec<T>) -> ChannelListResponse<T> {
        ChannelListResponse::new(channels)
    }

    pub fn response_from_source<T>(&self, source: ChannelListSource<T>) -> ChannelListResponse<T> {
        self.response(source.channels)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupListRequest {
    pub app_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupListChannelSource {
    pub app_type: String,
    pub source: ChannelRouteSource,
    pub channels: Vec<ChannelSpec>,
}

impl GroupListChannelSource {
    pub fn new(
        app_type: impl Into<String>,
        source: ChannelRouteSource,
        channels: Vec<ChannelSpec>,
    ) -> Self {
        Self {
            app_type: app_type.into(),
            source,
            channels,
        }
    }
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

    pub fn response_from_channel_sources(
        &self,
        sources: impl IntoIterator<Item = GroupListChannelSource>,
    ) -> RouteGroupListResponse {
        self.response(
            sources
                .into_iter()
                .map(|source| self.source_input(source.app_type, &source.source, source.channels)),
        )
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
        AppChannelListSource, AppChannelManagementPlan, AppChannelManagementRequest,
        AppListRequest, AppListSource, AppModelCatalogRequest, ChannelCreateRequest, ChannelListPlan,
        ChannelListRequest, ChannelListSource, ChannelMigrationMaterializeSource,
        ChannelMigrationPreviewSource, ChannelPathRequest, CurrentRouteSource,
        GroupListChannelSource, GroupListRequest, HealthCheckRequest, ManagementAppPathRequest,
        ProviderListSource, ProxyStatusRequest,
        RouteResolveManagementRequest, channel_not_found_message, normalize_channel_id_path,
        validate_management_app_type, validate_route_resolve_app_type,
    };
    use crate::{
        AppChannelListQuery, AppKind, AppModelListQuery, AppSummaryInput, ChannelHealthPolicy,
        ChannelListQuery, ChannelOverrides, ChannelRouteSource, ChannelSpec, ChannelStatus,
        ChannelTestInput, GroupListQuery, InterfaceKind, ProviderKind, ProviderMetadata,
        ProviderSpec, ProxyChannelWriteRequest, RetryPolicy, RouteResolveResponse,
        UpstreamEndpoint,
    };
    use serde_json::json;

    fn provider_spec(id: &str) -> ProviderSpec {
        ProviderSpec {
            id: id.to_string(),
            name: format!("{id} Provider"),
            kind: ProviderKind::Claude,
            account_ref: None,
            metadata: ProviderMetadata::default(),
        }
    }

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
    fn app_list_request_wraps_app_summary_response() {
        let request = AppListRequest::new();

        let response = request.response_from_source(AppListSource::new(vec![
            AppSummaryInput::new("claude", true, false, 2, 3),
        ]));

        assert_eq!(response.apps.len(), 1);
        assert_eq!(response.apps[0].app_type, "claude");
        assert!(response.apps[0].enabled);
        assert_eq!(response.apps[0].provider_count, 2);
        assert_eq!(response.apps[0].channel_count, 3);
    }

    #[test]
    fn health_check_request_wraps_healthy_response() {
        let request = HealthCheckRequest::new();

        let response = request.response("2026-06-19T00:00:00Z");

        assert_eq!(response.status, "healthy");
        assert_eq!(response.timestamp, "2026-06-19T00:00:00Z");
    }

    #[test]
    fn proxy_status_request_wraps_status_response() {
        let request = ProxyStatusRequest::new();

        let response = request.response("running");

        assert_eq!(response.status, "running");
    }

    #[test]
    fn channel_create_request_preserves_body_and_wraps_record_response() {
        let body = ProxyChannelWriteRequest {
            id: Some("channel-a".to_string()),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Claude Channel".to_string(),
            base_url: "https://api.example.com".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            ..ProxyChannelWriteRequest::default()
        };
        let request = ChannelCreateRequest::from_body(body.clone());

        let response = request.record_response("channel-record");

        assert_eq!(response.channel, "channel-record");
        assert_eq!(request.into_body().provider_id, body.provider_id);
    }

    #[test]
    fn management_app_path_request_normalizes_path_app() {
        let request = ManagementAppPathRequest::from_path(" claude ").expect("request");

        assert_eq!(request.app_type, "claude");
    }

    #[test]
    fn management_app_path_request_wraps_current_route_response() {
        let request = ManagementAppPathRequest::from_path("claude").expect("request");

        let response = request.current_route_response_from_source(CurrentRouteSource::new(
            Some("target-a"),
            None,
        ));

        assert_eq!(response.app_type, "claude");
        assert!(response.active);
        assert_eq!(response.target, Some("target-a"));
        assert!(response.configured_provider.is_none());
    }

    #[test]
    fn management_app_path_request_wraps_provider_list_response() {
        let request = ManagementAppPathRequest::from_path("claude").expect("request");
        let failover_ids = vec!["provider-b".to_string()];
        let route_candidate_ids = vec!["provider-a".to_string()];

        let response = request.provider_list_response_from_source(ProviderListSource::new(
            vec![provider_spec("provider-a"), provider_spec("provider-b")],
            Some("provider-a".to_string()),
            failover_ids,
            route_candidate_ids,
        ));

        assert_eq!(response.app_type, "claude");
        assert_eq!(response.providers.len(), 2);
        assert!(response.providers[0].current);
        assert!(response.providers[0].route_candidate);
        assert!(!response.providers[0].in_failover_queue);
        assert!(!response.providers[1].current);
        assert!(!response.providers[1].route_candidate);
        assert!(response.providers[1].in_failover_queue);
    }

    #[test]
    fn management_app_path_request_wraps_migration_preview_response() {
        let request = ManagementAppPathRequest::from_path("claude").expect("request");

        let response = request.migration_preview_response_from_source(
            ChannelMigrationPreviewSource::new(vec!["channel-a"], 2, 1),
        );

        assert_eq!(response.app_type, "claude");
        assert_eq!(response.channels, vec!["channel-a"]);
        assert_eq!(response.duplicate_count, 2);
        assert_eq!(response.needs_review_count, 1);
    }

    #[test]
    fn management_app_path_request_wraps_migration_materialize_response() {
        let request = ManagementAppPathRequest::from_path("claude").expect("request");

        let response = request.migration_materialize_response_from_source(
            ChannelMigrationMaterializeSource::new(4, 3, 2, 1, 5, 6),
        );

        assert_eq!(response.app_type, "claude");
        assert_eq!(response.previewed_channels, 4);
        assert_eq!(response.inserted_channels, 3);
        assert_eq!(response.inserted_models, 2);
        assert_eq!(response.inserted_health_rows, 1);
        assert_eq!(response.duplicate_count, 5);
        assert_eq!(response.needs_review_count, 6);
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
    fn channel_path_request_wraps_delete_response() {
        let request = ChannelPathRequest::from_path("channel-a").expect("request");
        let response = request.delete_response(true);

        assert_eq!(response.channel_id, "channel-a");
        assert!(response.deleted);
    }

    #[test]
    fn channel_path_request_wraps_test_response() {
        let request = ChannelPathRequest::from_path("channel-a").expect("request");
        let response = request.test_response(ChannelTestInput {
            channel_id: request.channel_id.clone(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            channel_name: "Primary".to_string(),
            base_url: "https://api.example.com".to_string(),
            interface_kind: "anthropic_messages".to_string(),
            model: None,
            model_available: None,
            success: true,
            status: "operational".to_string(),
            message: "Reachable".to_string(),
            latency_ms: Some(12),
            http_status: Some(401),
            tested_at: 1_771_000_000,
            retry_count: 0,
            failure_reason: None,
        });

        assert_eq!(response.channel_id, "channel-a");
        assert_eq!(response.provider_id, "provider-a");
        assert_eq!(response.http_status, Some(401));
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
        let route_request = match request.plan() {
            AppChannelManagementPlan::Route(route_request) => route_request,
            AppChannelManagementPlan::List { .. } => panic!("expected route plan"),
        };

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
        match request.plan() {
            AppChannelManagementPlan::List { app_type } => assert_eq!(app_type, "claude"),
            AppChannelManagementPlan::Route(_) => panic!("expected list plan"),
        }
    }

    #[test]
    fn app_channel_management_request_builds_list_response() {
        let query = serde_json::from_value::<AppChannelListQuery>(serde_json::json!({}))
            .expect("query");
        let request = AppChannelManagementRequest::from_parts("claude", query).expect("request");

        let response = request.response_from_list_source(AppChannelListSource::new(
            ChannelRouteSource::MaterializedChannels,
            vec!["channel-a"],
        ));
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
            request.response_from_route_resolution(RouteResolveResponse {
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
    fn channel_list_request_plans_app_or_all_queries() {
        let app_query = serde_json::from_value::<ChannelListQuery>(serde_json::json!({
            "appType": "claude"
        }))
        .expect("query");
        let app_request = ChannelListRequest::from_query(app_query).expect("request");
        let all_query =
            serde_json::from_value::<ChannelListQuery>(serde_json::json!({})).expect("query");
        let all_request = ChannelListRequest::from_query(all_query).expect("request");

        assert_eq!(
            app_request.plan(),
            ChannelListPlan::App {
                app_type: "claude".to_string()
            }
        );
        assert_eq!(all_request.plan(), ChannelListPlan::All);
    }

    #[test]
    fn channel_list_request_wraps_list_response() {
        let query = serde_json::from_value::<ChannelListQuery>(serde_json::json!({
            "appType": "claude"
        }))
        .expect("query");
        let request = ChannelListRequest::from_query(query).expect("request");

        let response =
            request.response_from_source(ChannelListSource::new(vec!["channel-a", "channel-b"]));

        assert_eq!(response.channels, vec!["channel-a", "channel-b"]);
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

        let response = request.response_from_channel_sources(vec![GroupListChannelSource::new(
            "claude",
            ChannelRouteSource::MaterializedChannels,
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
