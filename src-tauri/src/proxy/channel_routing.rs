//! Read-only channel route resolution.
//!
//! This is the bridge between the legacy provider router and the channel-based
//! design. It deliberately does not alter the forwarding path yet.

use crate::database::ProxyChannelRecord;
use crate::error::AppError;
use crate::proxy_core::{
    resolve_channel_route as resolve_core_channel_route, ChannelRouteSource, ProxyCoreError,
    RouteResolveChannelInput, RouteResolveRequest, RouteResolveResponse,
};
use crate::proxy_core_adapter::ToProxyCoreChannelSpec;

pub(crate) fn resolve_channel_route(
    request: RouteResolveRequest,
    channels: Vec<ProxyChannelRecord>,
    source: ChannelRouteSource,
) -> Result<RouteResolveResponse, AppError> {
    let channels = channels.into_iter().map(route_input_from_channel).collect();
    resolve_core_channel_route(request, channels, source).map_err(proxy_core_error_to_app_error)
}

fn route_input_from_channel(channel: ProxyChannelRecord) -> RouteResolveChannelInput {
    let source_kind = channel.source_kind.as_str().to_string();
    RouteResolveChannelInput::from_channel_spec(channel.to_proxy_core_channel_spec(), source_kind)
}

fn proxy_core_error_to_app_error(error: ProxyCoreError) -> AppError {
    match error {
        ProxyCoreError::Config(message) => AppError::Config(message),
        ProxyCoreError::InvalidRequest(message) => AppError::InvalidInput(message),
        other => AppError::Message(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{ProxyChannelModelRecord, ProxyChannelRecord, ProxyChannelSourceKind};
    use crate::proxy_core::DEFAULT_ROUTE_GROUP;
    use serde_json::json;

    fn channel(id: &str, interface_kind: &str, model: &str, priority: i64) -> ProxyChannelRecord {
        ProxyChannelRecord {
            id: id.to_string(),
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: id.to_string(),
            status: "enabled".to_string(),
            base_url: format!("https://{id}.example.com/v1"),
            interface_kind: interface_kind.to_string(),
            auth_profile_ref: None,
            groups: vec![DEFAULT_ROUTE_GROUP.to_string()],
            priority,
            weight: 100,
            retry_policy: json!({}),
            health_policy: json!({}),
            header_overrides: json!({}),
            param_overrides: json!({}),
            status_code_mapping: json!([]),
            tags: Vec::new(),
            metadata: json!({}),
            source_kind: ProxyChannelSourceKind::Manual,
            source_endpoint_url: None,
            models: vec![ProxyChannelModelRecord {
                channel_id: id.to_string(),
                public_model: model.to_string(),
                upstream_model: format!("upstream-{model}"),
                capabilities: json!({}),
                pricing_model: None,
                request_overrides: json!({}),
                response_overrides: json!({}),
            }],
            needs_review: false,
            review_reasons: Vec::new(),
        }
    }

    #[test]
    fn resolves_matching_model_and_sorts_by_priority() {
        let response = resolve_channel_route(
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("sonnet".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
            vec![
                channel("low", "openai_responses", "sonnet", 1),
                channel("high", "openai_responses", "sonnet", 10),
            ],
            ChannelRouteSource::MaterializedChannels,
        )
        .expect("resolve");

        assert_eq!(response.candidates.len(), 2);
        assert_eq!(response.candidates[0].channel_id, "high");
        assert_eq!(
            response.candidates[0].upstream_model.as_deref(),
            Some("upstream-sonnet")
        );
    }

    #[test]
    fn reports_rejected_reasons() {
        let mut disabled = channel("disabled", "gemini_native", "gemini-pro", 0);
        disabled.status = "manual_disabled".to_string();
        disabled.groups = vec!["paid".to_string()];

        let response = resolve_channel_route(
            RouteResolveRequest {
                app_type: "gemini".to_string(),
                requested_model: Some("gemini-pro".to_string()),
                interface_kind: Some("openai_responses".to_string()),
                route_group: Some(DEFAULT_ROUTE_GROUP.to_string()),
            },
            vec![disabled],
            ChannelRouteSource::LegacyProjection,
        )
        .expect("resolve");

        assert!(response.candidates.is_empty());
        assert_eq!(response.rejected.len(), 1);
        assert!(response.rejected[0]
            .reasons
            .iter()
            .any(|reason| reason == "status:manual_disabled"));
        assert!(response.rejected[0]
            .reasons
            .iter()
            .any(|reason| reason == "group_mismatch:default"));
        assert!(response.rejected[0]
            .reasons
            .iter()
            .any(|reason| reason == "interface_mismatch:openai_responses->gemini_native"));
    }
}
