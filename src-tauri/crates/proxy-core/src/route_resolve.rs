use super::domain::{
    claude_api_format_for_interface_kind, codex_api_format_for_interface_kind, AppKind,
    ChannelSpec, ModelRoute, ResolvedChannelAttempt, RouteSelection, DEFAULT_ROUTE_GROUP,
};
use super::error::{ProxyCoreError, ProxyCoreResult};
use super::ports::{
    ChannelRouteCandidate, ChannelRouteRejected, ChannelRouteSource, RouteResolveRequest,
    RouteResolveResponse,
};
use serde_json::{Map, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteResolveModelInput {
    pub public_model: String,
    pub upstream_model: String,
}

impl RouteResolveModelInput {
    pub fn from_model_route(model: ModelRoute) -> Self {
        Self {
            public_model: model.public_model,
            upstream_model: model.upstream_model,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteResolveChannelInput {
    pub channel_id: String,
    pub provider_id: String,
    pub channel_name: String,
    pub status: String,
    pub base_url: String,
    pub interface_kind: String,
    pub groups: Vec<String>,
    pub models: Vec<RouteResolveModelInput>,
    pub priority: i64,
    pub weight: u32,
    pub source_kind: String,
}

impl RouteResolveChannelInput {
    pub fn from_channel_spec(channel: ChannelSpec, source_kind: impl Into<String>) -> Self {
        Self {
            channel_id: channel.id,
            provider_id: channel.provider_id,
            channel_name: channel.name,
            status: channel.status.as_str().to_string(),
            base_url: channel.endpoint.base_url,
            interface_kind: channel.interface.as_str().to_string(),
            groups: channel.groups,
            models: channel
                .models
                .into_iter()
                .map(RouteResolveModelInput::from_model_route)
                .collect(),
            priority: channel.priority,
            weight: channel.weight,
            source_kind: source_kind.into(),
        }
    }
}

pub fn route_candidate_from_selection(
    selection: &RouteSelection,
    route_group: impl Into<String>,
    source_kind: impl Into<String>,
) -> ChannelRouteCandidate {
    ChannelRouteCandidate {
        channel_id: selection.channel.id.clone(),
        provider_id: selection.channel.provider_id.clone(),
        channel_name: selection.channel.name.clone(),
        base_url: selection.channel.endpoint.base_url.clone(),
        interface_kind: selection.channel.interface.as_str().to_string(),
        public_model: selection
            .model_route
            .as_ref()
            .map(|route| route.public_model.clone()),
        upstream_model: selection
            .model_route
            .as_ref()
            .map(|route| route.upstream_model.clone()),
        route_group: route_group.into(),
        priority: selection.channel.priority,
        weight: selection.channel.weight,
        source_kind: source_kind.into(),
    }
}

pub fn resolved_channel_attempt_from_candidate(
    candidate: ChannelRouteCandidate,
) -> ResolvedChannelAttempt {
    ResolvedChannelAttempt {
        channel_id: candidate.channel_id,
        channel_name: candidate.channel_name,
        base_url: candidate.base_url,
        interface_kind: candidate.interface_kind,
        public_model: candidate.public_model,
        upstream_model: candidate.upstream_model,
        header_overrides: Value::Object(Default::default()),
        param_overrides: Value::Object(Default::default()),
    }
}

pub fn resolved_channel_attempt_from_selection(
    selection: &RouteSelection,
) -> ResolvedChannelAttempt {
    ResolvedChannelAttempt {
        channel_id: selection.channel.id.clone(),
        channel_name: selection.channel.name.clone(),
        base_url: selection.channel.endpoint.base_url.clone(),
        interface_kind: selection.channel.interface.as_str().to_string(),
        public_model: selection
            .model_route
            .as_ref()
            .map(|route| route.public_model.clone()),
        upstream_model: selection
            .model_route
            .as_ref()
            .map(|route| route.upstream_model.clone()),
        header_overrides: selection.channel.overrides.headers.clone(),
        param_overrides: selection.channel.overrides.params.clone(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelProviderSettingTarget {
    Env,
    Root,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelProviderSettingOverride {
    pub target: ChannelProviderSettingTarget,
    pub key: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChannelProviderOverridePlan {
    pub settings: Vec<ChannelProviderSettingOverride>,
    pub api_format: Option<String>,
}

pub fn channel_provider_override_plan(
    app: &AppKind,
    candidate: &ChannelRouteCandidate,
) -> ChannelProviderOverridePlan {
    let mut plan = ChannelProviderOverridePlan::default();

    match app {
        AppKind::Claude | AppKind::ClaudeDesktop => {
            plan.settings.push(ChannelProviderSettingOverride {
                target: ChannelProviderSettingTarget::Env,
                key: "ANTHROPIC_BASE_URL",
                value: candidate.base_url.clone(),
            });
            if let Some(upstream_model) = candidate.upstream_model.as_deref() {
                plan.settings.push(ChannelProviderSettingOverride {
                    target: ChannelProviderSettingTarget::Env,
                    key: "ANTHROPIC_MODEL",
                    value: upstream_model.to_string(),
                });
            }
            plan.api_format =
                claude_api_format_for_interface_kind(&candidate.interface_kind).map(str::to_string);
        }
        AppKind::Codex => {
            plan.settings.push(ChannelProviderSettingOverride {
                target: ChannelProviderSettingTarget::Root,
                key: "base_url",
                value: candidate.base_url.clone(),
            });
            plan.api_format =
                codex_api_format_for_interface_kind(&candidate.interface_kind).map(str::to_string);
        }
        AppKind::Custom(name) if is_openai_compatible_app_name(name) => {
            plan.settings.push(ChannelProviderSettingOverride {
                target: ChannelProviderSettingTarget::Root,
                key: "base_url",
                value: candidate.base_url.clone(),
            });
            plan.api_format =
                codex_api_format_for_interface_kind(&candidate.interface_kind).map(str::to_string);
        }
        AppKind::Gemini => {
            plan.settings.push(ChannelProviderSettingOverride {
                target: ChannelProviderSettingTarget::Env,
                key: "GOOGLE_GEMINI_BASE_URL",
                value: candidate.base_url.clone(),
            });
        }
        AppKind::Custom(_) => {}
    }

    plan
}

pub fn apply_channel_provider_settings_overrides(
    settings: &mut Value,
    plan: &ChannelProviderOverridePlan,
) {
    for setting in &plan.settings {
        match setting.target {
            ChannelProviderSettingTarget::Env => {
                set_env_value(settings, setting.key, &setting.value);
            }
            ChannelProviderSettingTarget::Root => {
                set_object_value(settings, setting.key, &setting.value);
            }
        }
    }
}

fn set_env_value(settings: &mut Value, key: &str, value: &str) {
    let root = ensure_object(settings);
    let env = root
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()));
    ensure_object(env).insert(key.to_string(), Value::String(value.to_string()));
}

fn set_object_value(settings: &mut Value, key: &str, value: &str) {
    ensure_object(settings).insert(key.to_string(), Value::String(value.to_string()));
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("value forced to object")
}

fn is_openai_compatible_app_name(name: &str) -> bool {
    matches!(name, "opencode" | "openclaw" | "hermes")
}

pub fn resolve_channel_route(
    request: RouteResolveRequest,
    channels: Vec<RouteResolveChannelInput>,
    source: ChannelRouteSource,
) -> ProxyCoreResult<RouteResolveResponse> {
    let app_type = normalize_required(&request.app_type, "app_type")?;
    let requested_model = request.requested_model.and_then(normalize_optional);
    let interface_kind = request.interface_kind.and_then(normalize_optional);
    let route_group = request
        .route_group
        .and_then(normalize_optional)
        .unwrap_or_else(|| DEFAULT_ROUTE_GROUP.to_string());

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
                channel_id: channel.channel_id,
                provider_id: channel.provider_id,
                channel_name: channel.channel_name,
                base_url: channel.base_url,
                interface_kind: channel.interface_kind,
                public_model: matched_model.map(|model| model.public_model.clone()),
                upstream_model: matched_model.map(|model| model.upstream_model.clone()),
                route_group: route_group.clone(),
                priority: channel.priority,
                weight: channel.weight,
                source_kind: channel.source_kind,
            });
        } else {
            rejected.push(ChannelRouteRejected {
                channel_id: channel.channel_id,
                provider_id: channel.provider_id,
                channel_name: channel.channel_name,
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

pub fn resolve_channel_route_from_specs<I, S>(
    request: RouteResolveRequest,
    channels: I,
    source: ChannelRouteSource,
) -> ProxyCoreResult<RouteResolveResponse>
where
    I: IntoIterator<Item = (ChannelSpec, S)>,
    S: Into<String>,
{
    resolve_channel_route(
        request,
        channels
            .into_iter()
            .map(|(channel, source_kind)| {
                RouteResolveChannelInput::from_channel_spec(channel, source_kind)
            })
            .collect(),
        source,
    )
}

pub fn reject_unavailable_route_candidates(
    response: &mut RouteResolveResponse,
    mut is_unavailable: impl FnMut(&ChannelRouteCandidate) -> bool,
) {
    let candidates = std::mem::take(&mut response.candidates);
    let mut available = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        if is_unavailable(&candidate) {
            response.rejected.push(ChannelRouteRejected {
                channel_id: candidate.channel_id,
                provider_id: candidate.provider_id,
                channel_name: candidate.channel_name,
                reasons: vec!["circuit_open".to_string()],
            });
        } else {
            available.push(candidate);
        }
    }

    response.candidates = available;
}

pub fn reject_unavailable_channel_ids<I, S>(
    response: &mut RouteResolveResponse,
    unavailable_channel_ids: I,
) where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let unavailable_channel_ids: HashSet<String> = unavailable_channel_ids
        .into_iter()
        .map(|channel_id| channel_id.as_ref().to_string())
        .collect();

    reject_unavailable_route_candidates(response, |candidate| {
        unavailable_channel_ids.contains(&candidate.channel_id)
    });
}

fn normalize_required(value: &str, field: &str) -> ProxyCoreResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        Err(ProxyCoreError::Config(format!("{field} cannot be empty")))
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
        return requested_group == DEFAULT_ROUTE_GROUP;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AppKind, ChannelOverrides, ChannelSpec, ChannelStatus, InterfaceKind, ModelCapabilities,
        ModelRoute, ProviderKind, ProviderMetadata, ProviderSpec, RouteSelection, UpstreamEndpoint,
    };
    use serde_json::json;

    fn channel(
        channel_id: &str,
        interface_kind: &str,
        public_model: &str,
        priority: i64,
    ) -> RouteResolveChannelInput {
        RouteResolveChannelInput {
            channel_id: channel_id.to_string(),
            provider_id: format!("provider-{channel_id}"),
            channel_name: format!("Channel {channel_id}"),
            status: "enabled".to_string(),
            base_url: format!("https://{channel_id}.example.com/v1"),
            interface_kind: interface_kind.to_string(),
            groups: Vec::new(),
            models: vec![RouteResolveModelInput {
                public_model: public_model.to_string(),
                upstream_model: format!("upstream-{public_model}"),
            }],
            priority,
            weight: 100,
            source_kind: "manual".to_string(),
        }
    }

    fn selection() -> RouteSelection {
        let model_route = ModelRoute {
            public_model: "sonnet-public".to_string(),
            upstream_model: "upstream-sonnet".to_string(),
            capabilities: ModelCapabilities::default(),
            pricing_model: None,
            request_overrides: json!({}),
            response_overrides: json!({}),
        };

        RouteSelection {
            provider: ProviderSpec {
                id: "provider-a".to_string(),
                name: "Provider A".to_string(),
                kind: ProviderKind::Claude,
                account_ref: None,
                metadata: ProviderMetadata::default(),
            },
            channel: ChannelSpec {
                id: "channel-a".to_string(),
                provider_id: "provider-a".to_string(),
                app: AppKind::Claude,
                name: "Channel A".to_string(),
                status: ChannelStatus::Enabled,
                endpoint: UpstreamEndpoint {
                    base_url: "https://relay.example.com/v1".to_string(),
                    path_template: None,
                    api_version: None,
                    timeout_profile: None,
                },
                interface: InterfaceKind::OpenAiResponses,
                auth_profile: None,
                models: vec![model_route.clone()],
                groups: vec![DEFAULT_ROUTE_GROUP.to_string()],
                priority: 50,
                weight: 20,
                retry_policy: Default::default(),
                health_policy: Default::default(),
                overrides: ChannelOverrides::default(),
                tags: Vec::new(),
                metadata: json!({}),
                source_ref: None,
                needs_review: false,
                review_reasons: Vec::new(),
            },
            model_route: Some(model_route),
            inbound_interface: InterfaceKind::AnthropicMessages,
            outbound_interface: InterfaceKind::OpenAiResponses,
        }
    }

    #[test]
    fn maps_route_selection_to_channel_route_candidate() {
        let candidate = route_candidate_from_selection(&selection(), "paid", "proxy_core");

        assert_eq!(candidate.channel_id, "channel-a");
        assert_eq!(candidate.provider_id, "provider-a");
        assert_eq!(candidate.channel_name, "Channel A");
        assert_eq!(candidate.base_url, "https://relay.example.com/v1");
        assert_eq!(candidate.interface_kind, "openai_responses");
        assert_eq!(candidate.public_model.as_deref(), Some("sonnet-public"));
        assert_eq!(candidate.upstream_model.as_deref(), Some("upstream-sonnet"));
        assert_eq!(candidate.route_group, "paid");
        assert_eq!(candidate.priority, 50);
        assert_eq!(candidate.weight, 20);
        assert_eq!(candidate.source_kind, "proxy_core");
    }

    #[test]
    fn maps_route_candidate_to_resolved_channel_attempt() {
        let attempt = resolved_channel_attempt_from_candidate(route_candidate_from_selection(
            &selection(),
            "paid",
            "proxy_core",
        ));

        assert_eq!(attempt.channel_id, "channel-a");
        assert_eq!(attempt.channel_name, "Channel A");
        assert_eq!(attempt.base_url, "https://relay.example.com/v1");
        assert_eq!(attempt.interface_kind, "openai_responses");
        assert_eq!(attempt.public_model.as_deref(), Some("sonnet-public"));
        assert_eq!(attempt.upstream_model.as_deref(), Some("upstream-sonnet"));

        assert_eq!(
            serde_json::to_value(&attempt).expect("serialize channel attempt"),
            json!({
                "channelId": "channel-a",
                "channelName": "Channel A",
                "baseUrl": "https://relay.example.com/v1",
                "interfaceKind": "openai_responses",
                "publicModel": "sonnet-public",
                "upstreamModel": "upstream-sonnet"
            })
        );
    }

    #[test]
    fn builds_route_input_from_channel_spec() {
        let input =
            RouteResolveChannelInput::from_channel_spec(selection().channel, "legacy_endpoint");

        assert_eq!(input.channel_id, "channel-a");
        assert_eq!(input.provider_id, "provider-a");
        assert_eq!(input.channel_name, "Channel A");
        assert_eq!(input.status, "enabled");
        assert_eq!(input.base_url, "https://relay.example.com/v1");
        assert_eq!(input.interface_kind, "openai_responses");
        assert_eq!(input.groups, vec![DEFAULT_ROUTE_GROUP.to_string()]);
        assert_eq!(input.models.len(), 1);
        assert_eq!(input.models[0].public_model, "sonnet-public");
        assert_eq!(input.models[0].upstream_model, "upstream-sonnet");
        assert_eq!(input.priority, 50);
        assert_eq!(input.weight, 20);
        assert_eq!(input.source_kind, "legacy_endpoint");
    }

    #[test]
    fn channel_provider_override_plan_sets_claude_env_and_api_format() {
        let candidate =
            route_candidate_from_selection(&selection(), DEFAULT_ROUTE_GROUP, "proxy_core");

        let plan = channel_provider_override_plan(&AppKind::Claude, &candidate);

        assert_eq!(
            plan.settings,
            vec![
                ChannelProviderSettingOverride {
                    target: ChannelProviderSettingTarget::Env,
                    key: "ANTHROPIC_BASE_URL",
                    value: "https://relay.example.com/v1".to_string(),
                },
                ChannelProviderSettingOverride {
                    target: ChannelProviderSettingTarget::Env,
                    key: "ANTHROPIC_MODEL",
                    value: "upstream-sonnet".to_string(),
                },
            ]
        );
        assert_eq!(plan.api_format.as_deref(), Some("openai_responses"));
    }

    #[test]
    fn channel_provider_settings_overrides_update_env_and_preserve_existing_values() {
        let candidate =
            route_candidate_from_selection(&selection(), DEFAULT_ROUTE_GROUP, "proxy_core");
        let plan = channel_provider_override_plan(&AppKind::Claude, &candidate);
        let mut settings = serde_json::json!({
            "env": {
                "ANTHROPIC_API_KEY": "keep-key",
                "ANTHROPIC_BASE_URL": "https://old.example.com/v1"
            }
        });

        apply_channel_provider_settings_overrides(&mut settings, &plan);

        assert_eq!(
            settings
                .pointer("/env/ANTHROPIC_BASE_URL")
                .and_then(Value::as_str),
            Some("https://relay.example.com/v1")
        );
        assert_eq!(
            settings
                .pointer("/env/ANTHROPIC_MODEL")
                .and_then(Value::as_str),
            Some("upstream-sonnet")
        );
        assert_eq!(
            settings
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("keep-key")
        );
    }

    #[test]
    fn channel_provider_settings_overrides_update_root_and_coerce_non_object() {
        let candidate =
            route_candidate_from_selection(&selection(), DEFAULT_ROUTE_GROUP, "proxy_core");
        let plan = channel_provider_override_plan(&AppKind::Codex, &candidate);
        let mut settings = Value::Null;

        apply_channel_provider_settings_overrides(&mut settings, &plan);

        assert_eq!(
            settings.get("base_url").and_then(Value::as_str),
            Some("https://relay.example.com/v1")
        );
    }

    #[test]
    fn channel_provider_override_plan_sets_codex_root_base_url_and_api_format() {
        let candidate =
            route_candidate_from_selection(&selection(), DEFAULT_ROUTE_GROUP, "proxy_core");

        let plan = channel_provider_override_plan(&AppKind::Codex, &candidate);

        assert_eq!(
            plan.settings,
            vec![ChannelProviderSettingOverride {
                target: ChannelProviderSettingTarget::Root,
                key: "base_url",
                value: "https://relay.example.com/v1".to_string(),
            }]
        );
        assert_eq!(plan.api_format.as_deref(), Some("openai_responses"));

        let opencode_plan =
            channel_provider_override_plan(&AppKind::Custom("opencode".to_string()), &candidate);
        assert_eq!(opencode_plan, plan);
    }

    #[test]
    fn channel_provider_override_plan_sets_gemini_env_without_api_format() {
        let candidate =
            route_candidate_from_selection(&selection(), DEFAULT_ROUTE_GROUP, "proxy_core");

        let plan = channel_provider_override_plan(&AppKind::Gemini, &candidate);

        assert_eq!(
            plan.settings,
            vec![ChannelProviderSettingOverride {
                target: ChannelProviderSettingTarget::Env,
                key: "GOOGLE_GEMINI_BASE_URL",
                value: "https://relay.example.com/v1".to_string(),
            }]
        );
        assert_eq!(plan.api_format, None);
    }

    #[test]
    fn resolves_matching_channels_in_priority_order() {
        let response = resolve_channel_route(
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("sonnet".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
            vec![
                channel("low", "openai_responses", "sonnet", 10),
                channel("high", "anthropic_messages", "sonnet", 100),
            ],
            ChannelRouteSource::MaterializedChannels,
        )
        .expect("resolve route");

        assert_eq!(response.candidates.len(), 2);
        assert_eq!(response.candidates[0].channel_id, "high");
        assert_eq!(
            response.candidates[0].upstream_model.as_deref(),
            Some("upstream-sonnet")
        );
    }

    #[test]
    fn resolves_channel_route_from_channel_specs() {
        let response = resolve_channel_route_from_specs(
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("sonnet-public".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: Some(DEFAULT_ROUTE_GROUP.to_string()),
            },
            vec![(selection().channel, "manual")],
            ChannelRouteSource::MaterializedChannels,
        )
        .expect("resolve route");

        assert_eq!(response.candidates.len(), 1);
        assert_eq!(response.candidates[0].channel_id, "channel-a");
        assert_eq!(response.candidates[0].source_kind, "manual");
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
        .expect("resolve route");

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

    #[test]
    fn rejects_unavailable_candidates_as_circuit_open() {
        let mut response = resolve_channel_route(
            RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("sonnet".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            },
            vec![
                channel("open", "openai_responses", "sonnet", 10),
                channel("blocked", "openai_responses", "sonnet", 100),
            ],
            ChannelRouteSource::MaterializedChannels,
        )
        .expect("resolve route");

        reject_unavailable_channel_ids(&mut response, ["blocked"]);

        assert_eq!(response.candidates.len(), 1);
        assert_eq!(response.candidates[0].channel_id, "open");
        assert_eq!(response.rejected.len(), 1);
        assert_eq!(response.rejected[0].channel_id, "blocked");
        assert_eq!(response.rejected[0].provider_id, "provider-blocked");
        assert_eq!(response.rejected[0].channel_name, "Channel blocked");
        assert_eq!(response.rejected[0].reasons, vec!["circuit_open"]);
    }
}
