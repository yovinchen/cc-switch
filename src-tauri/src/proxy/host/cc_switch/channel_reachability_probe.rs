use crate::app_config::AppType;
use crate::database::Database;
use crate::proxy_core_adapter::{
    ChannelReachabilityProbe, ChannelReachabilityResult, ChannelTestProbeRequest, ProxyCoreResult,
    app_error, channel_reachability_probe_error, channel_test_app_type_error,
    channel_test_provider_not_found_error, stream_check_result_to_channel_reachability,
};
use crate::services::stream_check::StreamCheckService;
use futures::future::BoxFuture;
use std::sync::Arc;

pub(crate) async fn probe_channel_reachability_from_db_source(
    db: &Database,
    request: ChannelTestProbeRequest,
) -> ProxyCoreResult<ChannelReachabilityResult> {
    let app_type = request
        .app_type
        .parse::<AppType>()
        .map_err(channel_test_app_type_error)?;
    let provider = db
        .get_provider_by_id(&request.provider_id, &request.app_type)
        .map_err(|error| app_error("get channel test provider", error))?;
    let provider = provider.ok_or_else(|| channel_test_provider_not_found_error(&request))?;
    let config = db
        .get_stream_check_config()
        .map_err(|error| app_error("get stream check config", error))?;
    let result =
        StreamCheckService::check_with_retry(&app_type, &provider, &config, Some(request.base_url))
            .await
            .map_err(channel_reachability_probe_error)?;

    Ok(stream_check_result_to_channel_reachability(result))
}

#[derive(Clone)]
pub(crate) struct CcSwitchChannelReachabilityProbe {
    db: Arc<Database>,
}

impl CcSwitchChannelReachabilityProbe {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl ChannelReachabilityProbe for CcSwitchChannelReachabilityProbe {
    fn probe_channel<'a>(
        &'a self,
        request: ChannelTestProbeRequest,
    ) -> BoxFuture<'a, ProxyCoreResult<ChannelReachabilityResult>> {
        Box::pin(async move { probe_channel_reachability_from_db_source(&self.db, request).await })
    }
}
