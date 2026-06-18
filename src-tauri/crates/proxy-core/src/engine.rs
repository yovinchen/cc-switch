use super::domain::{ChannelQuery, ProxyRequest, ProxyResult, RoutePlan, RouteRequest};
use super::error::{ProxyCoreError, ProxyCoreResult};
use super::ports::ProxyServices;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct ProxyRuntimeState {
    accepted_requests: AtomicU64,
}

impl ProxyRuntimeState {
    pub fn accepted_requests(&self) -> u64 {
        self.accepted_requests.load(Ordering::Relaxed)
    }

    fn mark_accepted(&self) {
        self.accepted_requests.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyCoreStatus {
    pub accepted_requests: u64,
    pub forwarding_enabled: bool,
}

pub struct ProxyEngine<S: ?Sized> {
    services: Arc<S>,
    state: Arc<ProxyRuntimeState>,
}

impl<S> ProxyEngine<S>
where
    S: ProxyServices + ?Sized,
{
    pub fn new(services: Arc<S>) -> Self {
        Self {
            services,
            state: Arc::new(ProxyRuntimeState::default()),
        }
    }

    pub fn services(&self) -> &S {
        self.services.as_ref()
    }

    pub fn state(&self) -> &ProxyRuntimeState {
        self.state.as_ref()
    }

    pub fn status(&self) -> ProxyCoreStatus {
        ProxyCoreStatus {
            accepted_requests: self.state.accepted_requests(),
            forwarding_enabled: false,
        }
    }

    pub async fn plan_route(&self, request: &ProxyRequest) -> ProxyCoreResult<RoutePlan> {
        self.plan_route_with_legacy_projection(request, true).await
    }

    pub async fn plan_materialized_route(
        &self,
        request: &ProxyRequest,
    ) -> ProxyCoreResult<RoutePlan> {
        self.plan_route_with_legacy_projection(request, false).await
    }

    async fn plan_route_with_legacy_projection(
        &self,
        request: &ProxyRequest,
        allow_legacy_projection: bool,
    ) -> ProxyCoreResult<RoutePlan> {
        let providers = self
            .services
            .providers()
            .list_providers(&request.app)
            .await?;
        let channels = self
            .services
            .channels()
            .list_channels(ChannelQuery {
                app: &request.app,
                provider_id: None,
                model: request.requested_model.as_deref(),
                group: request.route_group.as_deref(),
                include_disabled: false,
                allow_legacy_projection,
            })
            .await?;
        let policy = self
            .services
            .route_policies()
            .load_policy(&request.app)
            .await?;

        self.services
            .route_resolver()
            .resolve(RouteRequest {
                request,
                providers: &providers,
                channels: &channels,
                policy: policy.as_ref(),
            })
            .await
    }

    pub async fn handle(&self, _request: ProxyRequest) -> ProxyCoreResult<ProxyResult> {
        self.state.mark_accepted();
        let _route_plan = self.plan_route(&_request).await?;
        Err(ProxyCoreError::Unsupported(
            "ProxyEngine::handle is not wired to the existing forwarding path yet".to_string(),
        ))
    }
}
