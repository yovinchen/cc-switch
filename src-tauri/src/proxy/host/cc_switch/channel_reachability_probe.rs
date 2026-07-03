use crate::app_config::AppType;
use crate::database::Database;
use crate::proxy_core::api::errors::{config_error_with_context, ProxyCoreResult};
use crate::proxy_core::api::management::{
    channel_reachability_probe_error,
    channel_reachability_result_from_stream_check_result as stream_check_result_to_channel_reachability,
    channel_test_app_type_error, channel_test_provider_not_found_error, ChannelReachabilityResult,
    ChannelTestProbeRequest,
};
use crate::proxy_core::api::ports::ChannelReachabilityProbe;
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
        .map_err(|error| config_error_with_context("get channel test provider", error))?;
    let provider = provider.ok_or_else(|| channel_test_provider_not_found_error(&request))?;
    let config = db
        .get_stream_check_config()
        .map_err(|error| config_error_with_context("get stream check config", error))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::proxy_core::api::errors::ProxyCoreError;
    use crate::proxy_core::api::management::{ChannelReachabilityStatus, StreamCheckResult};
    use serde_json::json;

    #[test]
    fn reachability_probe_preserves_stream_check_projection_fields() {
        let result = StreamCheckResult {
            status: ChannelReachabilityStatus::Degraded,
            success: true,
            message: "slow but reachable".to_string(),
            response_time_ms: Some(6100),
            http_status: Some(200),
            model_used: String::new(),
            tested_at: 1_797_000_000,
            retry_count: 1,
            error_category: None,
        };

        let reachability = stream_check_result_to_channel_reachability(result);

        assert!(reachability.success);
        assert_eq!(
            reachability.status,
            ChannelReachabilityStatus::Degraded.as_str()
        );
        assert_eq!(reachability.message, "slow but reachable");
        assert_eq!(reachability.latency_ms, Some(6100));
        assert_eq!(reachability.http_status, Some(200));
        assert_eq!(reachability.tested_at, 1_797_000_000);
        assert_eq!(reachability.retry_count, 1);
    }

    #[test]
    fn reachability_probe_preserves_probe_error_contracts() {
        let probe = ChannelTestProbeRequest {
            channel_id: "channel-a".to_string(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
        };
        assert_eq!(
            probe.app_type.parse::<AppType>().expect("app type"),
            AppType::Claude
        );
        let provider = Provider::with_id(
            "provider-a".to_string(),
            "Provider A".to_string(),
            json!({}),
            None,
        );
        assert_eq!(provider.id, "provider-a");
        let missing_provider: ProxyCoreResult<Provider> =
            None.ok_or_else(|| channel_test_provider_not_found_error(&probe));
        let missing_provider = missing_provider.expect_err("missing provider");
        assert!(matches!(
            missing_provider,
            ProxyCoreError::Config(message)
                if message == "provider not found for channel channel-a: provider-a"
        ));
        let invalid_probe = ChannelTestProbeRequest {
            app_type: "unknown-app".to_string(),
            ..probe.clone()
        };
        let invalid_app_type: ProxyCoreResult<AppType> = invalid_probe
            .app_type
            .parse::<AppType>()
            .map_err(channel_test_app_type_error);
        assert!(matches!(
            invalid_app_type,
            Err(ProxyCoreError::InvalidRequest(message))
                if message.contains("unknown-app")
        ));
        assert!(matches!(
            channel_reachability_probe_error("probe failed"),
            ProxyCoreError::Internal(message) if message == "probe failed"
        ));
    }

    #[test]
    fn reachability_probe_preserves_status_string_contracts() {
        for (health_status, reachability_status) in [
            (
                ChannelReachabilityStatus::Operational,
                ChannelReachabilityStatus::Operational,
            ),
            (
                ChannelReachabilityStatus::Failed,
                ChannelReachabilityStatus::Failed,
            ),
        ] {
            let result = StreamCheckResult {
                status: health_status,
                success: false,
                message: String::new(),
                response_time_ms: None,
                http_status: None,
                model_used: String::new(),
                tested_at: 0,
                retry_count: 0,
                error_category: None,
            };

            assert_eq!(
                stream_check_result_to_channel_reachability(result).status,
                reachability_status.as_str()
            );
        }
    }
}
