//! Read-only channel route resolution.
//!
//! This is the bridge between the legacy provider router and the channel-based
//! design. It deliberately does not alter the forwarding path yet.

use crate::database::{ProxyChannelRecord, ProxyChannelSourceKind};
use crate::error::AppError;
use crate::proxy_core::{
    ChannelRouteCandidate, ChannelRouteRejected, ChannelRouteSource, RouteResolveRequest,
    RouteResolveResponse,
};

const DEFAULT_GROUP: &str = "default";

pub(crate) fn resolve_channel_route(
    request: RouteResolveRequest,
    channels: Vec<ProxyChannelRecord>,
    source: ChannelRouteSource,
) -> Result<RouteResolveResponse, AppError> {
    let app_type = normalize_required(&request.app_type, "app_type")?;
    let requested_model = request.requested_model.and_then(normalize_optional);
    let interface_kind = request.interface_kind.and_then(normalize_optional);
    let route_group = request
        .route_group
        .and_then(normalize_optional)
        .unwrap_or_else(|| DEFAULT_GROUP.to_string());

    let mut candidates = Vec::new();
    let mut rejected = Vec::new();

    for channel in channels {
        let mut reasons = Vec::new();

        if channel.status != "enabled" {
            reasons.push(format!("status:{}", channel.status));
        }

        if !channel_groups_match(&channel.groups, &route_group) {
            reasons.push(format!("group_mismatch:{route_group}"));
        }

        if let Some(requested_interface) = interface_kind.as_deref() {
            if !interfaces_compatible(requested_interface, &channel.interface_kind) {
                reasons.push(format!(
                    "interface_mismatch:{requested_interface}->{}",
                    channel.interface_kind
                ));
            }
        }

        let matched_model = match requested_model.as_deref() {
            Some(model) => channel
                .models
                .iter()
                .find(|route| route.public_model == model || route.upstream_model == model),
            None => channel.models.first(),
        };

        if requested_model.is_some() && matched_model.is_none() {
            reasons.push(format!(
                "model_unavailable:{}",
                requested_model.as_deref().unwrap_or_default()
            ));
        }

        if reasons.is_empty() {
            candidates.push(ChannelRouteCandidate {
                channel_id: channel.id,
                provider_id: channel.provider_id,
                channel_name: channel.name,
                base_url: channel.base_url,
                interface_kind: channel.interface_kind,
                public_model: matched_model.map(|model| model.public_model.clone()),
                upstream_model: matched_model.map(|model| model.upstream_model.clone()),
                route_group: route_group.clone(),
                priority: channel.priority,
                weight: channel.weight,
                source_kind: source_kind_label(&channel.source_kind).to_string(),
            });
        } else {
            rejected.push(ChannelRouteRejected {
                channel_id: channel.id,
                provider_id: channel.provider_id,
                channel_name: channel.name,
                reasons,
            });
        }
    }

    candidates.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| right.weight.cmp(&left.weight))
            .then_with(|| left.channel_name.cmp(&right.channel_name))
            .then_with(|| left.channel_id.cmp(&right.channel_id))
    });

    Ok(RouteResolveResponse {
        app_type,
        requested_model,
        interface_kind,
        route_group,
        source,
        candidates,
        rejected,
    })
}

fn normalize_required(value: &str, field: &str) -> Result<String, AppError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        Err(AppError::Config(format!("{field} cannot be empty")))
    } else {
        Ok(normalized.to_string())
    }
}

fn normalize_optional(value: String) -> Option<String> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn channel_groups_match(groups: &[String], requested_group: &str) -> bool {
    if groups.is_empty() {
        return requested_group == DEFAULT_GROUP;
    }
    groups.iter().any(|group| group == requested_group)
}

fn interfaces_compatible(requested: &str, channel: &str) -> bool {
    if requested == channel {
        return true;
    }

    matches!(
        (requested, channel),
        (
            "anthropic_messages",
            "openai_chat_completions" | "openai_responses" | "gemini_native"
        ) | (
            "openai_responses",
            "openai_chat_completions" | "openai_responses"
        ) | (
            "openai_chat_completions",
            "openai_chat_completions" | "openai_responses"
        )
    )
}

fn source_kind_label(source_kind: &ProxyChannelSourceKind) -> &'static str {
    match source_kind {
        ProxyChannelSourceKind::LegacyPrimary => "legacy_primary",
        ProxyChannelSourceKind::LegacyEndpoint => "legacy_endpoint",
        ProxyChannelSourceKind::Manual => "manual",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{ProxyChannelModelRecord, ProxyChannelRecord};
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
            groups: vec![DEFAULT_GROUP.to_string()],
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
                route_group: Some(DEFAULT_GROUP.to_string()),
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
