//! CC Switch ProviderRouter channel source.

use crate::error::AppError;
use crate::proxy::engine::routing::ProviderRouterChannelSource;
use crate::proxy::host::cc_switch::database_channel_source::CcSwitchChannelSource;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::ProxyCoreError;
use crate::proxy_core::api::management::{ChannelRecord, ChannelRouteSource};
use crate::proxy_core::api::ports::ChannelSource;
use crate::proxy_core::api::routing::{
    route_resolve_channel_input_from_record, RouteResolveChannelInput,
    RouteResolveChannelRecordInput, RouteResolveModelRecordInput,
};
use futures::future::BoxFuture;

pub(crate) struct CcSwitchProviderRouterChannelSource {
    source: CcSwitchChannelSource,
}

impl CcSwitchProviderRouterChannelSource {
    pub(crate) fn new(source: CcSwitchChannelSource) -> Self {
        Self { source }
    }
}

impl ProviderRouterChannelSource for CcSwitchProviderRouterChannelSource {
    fn channel_route_inputs<'a>(
        &'a self,
        app_type: &'a str,
    ) -> BoxFuture<'a, Result<(Vec<RouteResolveChannelInput>, ChannelRouteSource), AppError>> {
        Box::pin(async move {
            router_channel_route_inputs_from_channel_source(&self.source, app_type)
                .await
                .map_err(provider_router_channel_source_app_error)
        })
    }
}

async fn router_channel_route_inputs_from_channel_source(
    source: &(dyn ChannelSource + Send + Sync),
    app_type: &str,
) -> Result<(Vec<RouteResolveChannelInput>, ChannelRouteSource), ProxyCoreError> {
    let app = AppKind::from(app_type);
    let (route_source, channels) = source.list_channel_records(&app).await?;
    Ok((
        channel_records_to_route_resolve_channel_inputs(channels),
        route_source,
    ))
}

fn channel_records_to_route_resolve_channel_inputs(
    channels: impl IntoIterator<Item = ChannelRecord>,
) -> Vec<RouteResolveChannelInput> {
    channels
        .into_iter()
        .map(channel_record_to_route_resolve_channel_input)
        .collect()
}

fn channel_record_to_route_resolve_channel_input(
    channel: ChannelRecord,
) -> RouteResolveChannelInput {
    route_resolve_channel_input_from_record(RouteResolveChannelRecordInput {
        channel_id: channel.id,
        provider_id: channel.provider_id,
        channel_name: channel.name,
        status: channel.status,
        base_url: channel.base_url,
        interface_kind: channel.interface_kind,
        groups: channel.groups,
        models: channel
            .models
            .into_iter()
            .map(|model| RouteResolveModelRecordInput {
                public_model: model.public_model,
                upstream_model: model.upstream_model,
            })
            .collect(),
        priority: channel.priority,
        weight: channel.weight,
        health_policy: channel.health_policy,
        source_kind: channel.source_kind,
    })
}

fn provider_router_channel_source_app_error(error: ProxyCoreError) -> AppError {
    match error {
        ProxyCoreError::Config(message) => AppError::Config(message),
        ProxyCoreError::InvalidRequest(message) => AppError::InvalidInput(message),
        other => AppError::Message(other.to_string()),
    }
}
