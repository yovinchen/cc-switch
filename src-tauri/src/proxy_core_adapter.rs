use crate::app_config::AppType;
use crate::claude_desktop_config::ResolvedModelRoute;
use crate::database::{ProxyChannelModelRecord, ProxyChannelRecord};
use crate::provider::Provider;
use crate::proxy::providers::provider_kind_from_app_type_and_config;
use crate::proxy_core::{
    AppKind, AppSummaryInput, AuthProfileRef, ChannelHealthPolicy, ChannelModelRecord,
    ChannelOverrides, ChannelReachabilityInput, ChannelReachabilityResult,
    ChannelReachabilityStatus, ChannelRecord, ChannelSpec, ChannelStatus,
    ClaudeDesktopModelListResponse, ClaudeDesktopModelRouteInput,
    CurrentRouteProviderSummaryInput, InterfaceKind, ModelCapabilities, ModelRoute,
    ModelCatalog, ProviderMetadata, ProviderSpec, RetryPolicy, RouteResolveChannelInput,
    RouteResolveModelInput, SessionIdResult, UpstreamEndpoint,
};
use crate::services::stream_check::{HealthStatus, StreamCheckResult};
use http::HeaderMap;
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) fn synthesize_gemini_tool_call_id_with_uuid() -> String {
    crate::proxy_core::synthesize_gemini_tool_call_id(Uuid::new_v4().simple().to_string())
}

impl From<&AppType> for AppKind {
    fn from(value: &AppType) -> Self {
        match value {
            AppType::Claude => Self::Claude,
            AppType::ClaudeDesktop => Self::ClaudeDesktop,
            AppType::Codex => Self::Codex,
            AppType::Gemini => Self::Gemini,
            AppType::OpenCode | AppType::OpenClaw | AppType::Hermes => {
                Self::Custom(value.as_str().to_string())
            }
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreProviderSpec {
    fn to_proxy_core_provider_spec(&self, app_type: &AppType) -> ProviderSpec;
}

impl ToProxyCoreProviderSpec for Provider {
    fn to_proxy_core_provider_spec(&self, app_type: &AppType) -> ProviderSpec {
        let kind = provider_kind_from_app_type_and_config(app_type, self);
        let metadata = provider_metadata_without_secrets(self);

        ProviderSpec {
            id: self.id.clone(),
            name: self.name.clone(),
            kind,
            account_ref: account_ref(self),
            metadata,
        }
    }
}

pub(crate) fn proxy_providers_to_core_specs(
    app_type: &AppType,
    providers: impl IntoIterator<Item = Provider>,
) -> Vec<ProviderSpec> {
    providers
        .into_iter()
        .map(|provider| provider.to_proxy_core_provider_spec(app_type))
        .collect()
}

pub(crate) fn proxy_current_route_provider_summary_input(
    provider: Provider,
    app_type: &AppType,
) -> CurrentRouteProviderSummaryInput {
    CurrentRouteProviderSummaryInput::from_provider_spec(
        provider.to_proxy_core_provider_spec(app_type),
    )
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelSpec {
    fn to_proxy_core_channel_spec(&self) -> ChannelSpec;
}

impl ToProxyCoreChannelSpec for ProxyChannelRecord {
    fn to_proxy_core_channel_spec(&self) -> ChannelSpec {
        ChannelSpec {
            id: self.id.clone(),
            provider_id: self.provider_id.clone(),
            app: AppKind::from(self.app_type.as_str()),
            name: self.name.clone(),
            status: ChannelStatus::from_storage(&self.status),
            endpoint: UpstreamEndpoint {
                base_url: self.base_url.clone(),
                path_template: None,
                api_version: None,
                timeout_profile: None,
            },
            interface: InterfaceKind::from_storage(&self.interface_kind),
            auth_profile: self.auth_profile_ref.clone().map(AuthProfileRef::new),
            models: self
                .models
                .iter()
                .map(ProxyChannelModelRecord::to_proxy_core_model_route)
                .collect(),
            groups: self.groups.clone(),
            priority: self.priority,
            weight: self.weight,
            retry_policy: RetryPolicy {
                raw: object_or_empty(self.retry_policy.clone()),
            },
            health_policy: ChannelHealthPolicy {
                raw: object_or_empty(self.health_policy.clone()),
            },
            overrides: ChannelOverrides {
                headers: object_or_empty(self.header_overrides.clone()),
                params: object_or_empty(self.param_overrides.clone()),
                status_code_mapping: array_or_empty(self.status_code_mapping.clone()),
                model_mapping: Value::Object(Default::default()),
            },
            tags: self.tags.clone(),
            metadata: object_or_empty(self.metadata.clone()),
            source_ref: self.source_endpoint_url.clone(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        }
    }
}

pub(crate) fn proxy_channel_record_to_core_spec(channel: &ProxyChannelRecord) -> ChannelSpec {
    channel.to_proxy_core_channel_spec()
}

pub(crate) fn proxy_channel_specs_to_core(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
) -> Vec<ChannelSpec> {
    channels
        .into_iter()
        .map(|channel| channel.to_proxy_core_channel_spec())
        .collect()
}

pub(crate) fn proxy_channel_route_inputs_to_core(
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
) -> Vec<RouteResolveChannelInput> {
    channels
        .into_iter()
        .map(|channel| {
            let ProxyChannelRecord {
                id,
                provider_id,
                name,
                status,
                base_url,
                interface_kind,
                groups,
                models,
                priority,
                weight,
                source_kind,
                ..
            } = channel;

            RouteResolveChannelInput {
                channel_id: id,
                provider_id,
                channel_name: name,
                status,
                base_url,
                interface_kind,
                groups,
                models: models
                    .into_iter()
                    .map(|model| RouteResolveModelInput {
                        public_model: model.public_model,
                        upstream_model: model.upstream_model,
                    })
                    .collect(),
                priority,
                weight,
                source_kind: source_kind.as_str().to_string(),
            }
        })
        .collect()
}

pub(crate) fn proxy_app_summary_input(
    app_type: &AppType,
    enabled: bool,
    auto_failover_enabled: bool,
    providers: impl IntoIterator<Item = Provider>,
    channels: impl IntoIterator<Item = ProxyChannelRecord>,
) -> AppSummaryInput {
    AppSummaryInput::from_specs(
        app_type.as_str(),
        enabled,
        auto_failover_enabled,
        proxy_providers_to_core_specs(app_type, providers),
        proxy_channel_specs_to_core(channels),
    )
}

pub(crate) fn claude_desktop_model_routes_to_core_response(
    routes: impl IntoIterator<Item = ResolvedModelRoute>,
) -> ClaudeDesktopModelListResponse {
    ClaudeDesktopModelListResponse::from_routes(
        routes
            .into_iter()
            .map(|route| ClaudeDesktopModelRouteInput::new(route.route_id, route.supports_1m)),
    )
}

pub(crate) fn codex_default_model_context_window() -> u64 {
    crate::proxy_core::DEFAULT_CODEX_MODEL_CONTEXT_WINDOW
}

pub(crate) fn codex_settings_have_model_catalog_specs(settings: &Value) -> bool {
    crate::proxy_core::has_codex_model_catalog_specs(settings)
}

pub(crate) fn codex_model_catalog_from_settings(
    settings: &Value,
    default_context_window: u64,
    template: &Value,
) -> Option<Value> {
    crate::proxy_core::build_codex_model_catalog_from_settings(
        settings,
        default_context_window,
        template,
    )
}

pub(crate) fn simplify_codex_model_catalog(
    catalog_text: &str,
    default_context_window: u64,
) -> Option<Value> {
    crate::proxy_core::simplify_codex_model_catalog(catalog_text, default_context_window)
}

pub(crate) fn provider_model_catalog_from_settings(
    provider_id: &str,
    settings: Option<&Value>,
) -> ModelCatalog {
    crate::proxy_core::provider_model_catalog_from_settings(provider_id, settings)
}

pub(crate) fn client_model_catalog_from_raw(app: &AppKind, raw: Value) -> ModelCatalog {
    crate::proxy_core::client_model_catalog_from_raw(app.as_str(), raw)
}

const CLAUDE_ONE_M_MARKER_FOR_CLIENT: &str = "[1M]";

pub(crate) fn claude_takeover_client_model_for_upstream(
    takeover_model: &str,
    supports_one_m: bool,
    upstream_model: &str,
) -> String {
    let mut client_model = takeover_model.to_string();
    if supports_one_m && crate::proxy_core::has_one_m_suffix_for_upstream(upstream_model) {
        client_model.push_str(CLAUDE_ONE_M_MARKER_FOR_CLIENT);
    }
    client_model
}

pub(crate) fn claude_takeover_default_display_name(upstream_model: &str) -> String {
    crate::proxy_core::strip_one_m_suffix_for_upstream(upstream_model)
        .trim()
        .to_string()
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreModelRoute {
    fn to_proxy_core_model_route(&self) -> ModelRoute;
}

impl ToProxyCoreModelRoute for ProxyChannelModelRecord {
    fn to_proxy_core_model_route(&self) -> ModelRoute {
        ModelRoute {
            public_model: self.public_model.clone(),
            upstream_model: self.upstream_model.clone(),
            capabilities: ModelCapabilities {
                raw: object_or_empty(self.capabilities.clone()),
            },
            pricing_model: self.pricing_model.clone(),
            request_overrides: object_or_empty(self.request_overrides.clone()),
            response_overrides: object_or_empty(self.response_overrides.clone()),
        }
    }
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord;
}

impl ToProxyCoreChannelModelRecord for ProxyChannelModelRecord {
    fn to_proxy_core_channel_model_record(&self) -> ChannelModelRecord {
        ChannelModelRecord::from_model_route(
            self.channel_id.clone(),
            self.to_proxy_core_model_route(),
        )
    }
}

pub(crate) fn proxy_channel_model_records_to_core(
    models: Vec<ProxyChannelModelRecord>,
) -> Vec<ChannelModelRecord> {
    models
        .iter()
        .map(ProxyChannelModelRecord::to_proxy_core_channel_model_record)
        .collect()
}

#[allow(dead_code)]
pub(crate) trait ToProxyCoreChannelRecord {
    fn to_proxy_core_channel_record(&self) -> ChannelRecord;
}

impl ToProxyCoreChannelRecord for ProxyChannelRecord {
    fn to_proxy_core_channel_record(&self) -> ChannelRecord {
        ChannelRecord {
            id: self.id.clone(),
            provider_id: self.provider_id.clone(),
            app_type: self.app_type.clone(),
            name: self.name.clone(),
            status: self.status.clone(),
            base_url: self.base_url.clone(),
            interface_kind: self.interface_kind.clone(),
            auth_profile_ref: self.auth_profile_ref.clone(),
            groups: self.groups.clone(),
            priority: self.priority,
            weight: self.weight,
            retry_policy: self.retry_policy.clone(),
            health_policy: self.health_policy.clone(),
            header_overrides: self.header_overrides.clone(),
            param_overrides: self.param_overrides.clone(),
            status_code_mapping: self.status_code_mapping.clone(),
            tags: self.tags.clone(),
            metadata: self.metadata.clone(),
            source_kind: self.source_kind.as_str().to_string(),
            source_endpoint_url: self.source_endpoint_url.clone(),
            models: self
                .models
                .iter()
                .map(ProxyChannelModelRecord::to_proxy_core_channel_model_record)
                .collect(),
            needs_review: self.needs_review,
            review_reasons: self.review_reasons.clone(),
        }
    }
}

pub(crate) fn proxy_channel_record_to_core(channel: ProxyChannelRecord) -> ChannelRecord {
    channel.to_proxy_core_channel_record()
}

pub(crate) fn proxy_channel_records_to_core(
    channels: Vec<ProxyChannelRecord>,
) -> Vec<ChannelRecord> {
    channels
        .into_iter()
        .map(proxy_channel_record_to_core)
        .collect()
}

pub(crate) fn extract_proxy_session_id(
    headers: &HeaderMap,
    body: &Value,
    client_format: &str,
) -> SessionIdResult {
    crate::proxy_core::extract_session_id_with_generator(headers, body, client_format, || {
        Uuid::new_v4().to_string()
    })
}

pub(crate) fn stream_check_result_to_channel_reachability(
    result: StreamCheckResult,
) -> ChannelReachabilityResult {
    ChannelReachabilityResult::from_input(ChannelReachabilityInput {
        success: result.success,
        status: stream_check_health_status_to_channel_reachability(&result.status),
        message: result.message,
        latency_ms: result.response_time_ms,
        http_status: result.http_status,
        tested_at: result.tested_at,
        retry_count: result.retry_count,
    })
}

fn stream_check_health_status_to_channel_reachability(
    status: &HealthStatus,
) -> ChannelReachabilityStatus {
    match status {
        HealthStatus::Operational => ChannelReachabilityStatus::Operational,
        HealthStatus::Degraded => ChannelReachabilityStatus::Degraded,
        HealthStatus::Failed => ChannelReachabilityStatus::Failed,
    }
}

fn provider_metadata_without_secrets(provider: &Provider) -> ProviderMetadata {
    let mut labels = Vec::new();
    if provider.in_failover_queue {
        labels.push("failover".to_string());
    }
    if let Some(category) = provider.category.as_deref() {
        labels.push(category.to_string());
    }

    let meta = provider.meta.as_ref();
    let raw = json!({
        "websiteUrl": provider.website_url,
        "category": provider.category,
        "sortIndex": provider.sort_index,
        "notes": provider.notes,
        "icon": provider.icon,
        "iconColor": provider.icon_color,
        "inFailoverQueue": provider.in_failover_queue,
        "providerType": meta.and_then(|meta| meta.provider_type.clone()),
        "apiFormat": meta.and_then(|meta| meta.api_format.clone()),
        "authBinding": meta.and_then(|meta| meta.auth_binding.as_ref()).map(|binding| json!(binding)),
        "endpointAutoSelect": meta.and_then(|meta| meta.endpoint_auto_select),
        "customEndpointCount": meta.map(|meta| meta.custom_endpoints.len()).unwrap_or(0),
    });

    ProviderMetadata { labels, raw }
}

fn account_ref(provider: &Provider) -> Option<String> {
    provider.meta.as_ref().and_then(|meta| {
        meta.provider_type.as_deref().and_then(|provider_type| {
            meta.managed_account_id_for(provider_type)
                .map(|account_id| format!("{provider_type}:{account_id}"))
        })
    })
}

fn object_or_empty(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        Value::Object(Default::default())
    }
}

fn array_or_empty(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        Value::Array(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::ProxyChannelSourceKind;
    use crate::provider::{AuthBinding, AuthBindingSource, ProviderMeta};
    use crate::proxy_core::{
        GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX, ProviderKind, SessionIdSource,
    };

    #[test]
    fn app_type_conversion_preserves_known_and_custom_names() {
        assert_eq!(AppKind::from(&AppType::Claude), AppKind::Claude);
        assert_eq!(
            AppKind::from(&AppType::ClaudeDesktop),
            AppKind::ClaudeDesktop
        );
        assert_eq!(AppKind::from(&AppType::Codex), AppKind::Codex);
        assert_eq!(
            AppKind::from(&AppType::OpenClaw),
            AppKind::Custom("openclaw".to_string())
        );
    }

    #[test]
    fn claude_desktop_model_routes_to_core_response_preserves_route_contract() {
        let response =
            claude_desktop_model_routes_to_core_response([ResolvedModelRoute {
                route_id: "claude-sonnet-4-6".to_string(),
                upstream_model: "anthropic/claude-sonnet-4-6".to_string(),
                label_override: None,
                supports_1m: true,
            }]);

        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].id, "claude-sonnet-4-6");
        assert!(response.data[0].supports_1m);
        assert_eq!(response.first_id.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(response.last_id.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn codex_catalog_adapter_builds_and_simplifies_model_catalog() {
        let settings = json!({
            "modelCatalog": {
                "models": [
                    {
                        "model": "kimi-k2",
                        "displayName": "Kimi K2",
                        "contextWindow": "64000"
                    }
                ]
            }
        });
        let template = json!({
            "slug": "gpt-5.5",
            "display_name": "GPT-5.5",
            "context_window": 272000,
            "model_messages": {"base": "template"}
        });

        assert!(codex_settings_have_model_catalog_specs(&settings));
        assert_eq!(codex_default_model_context_window(), 128_000);

        let catalog = codex_model_catalog_from_settings(&settings, 128_000, &template)
            .expect("catalog");
        let models = catalog
            .get("models")
            .and_then(Value::as_array)
            .expect("models");
        assert_eq!(models[0].get("slug").and_then(Value::as_str), Some("kimi-k2"));
        assert_eq!(
            models[0].get("context_window").and_then(Value::as_u64),
            Some(64_000)
        );

        let simplified = simplify_codex_model_catalog(&catalog.to_string(), 128_000)
            .expect("simplified catalog");
        assert_eq!(
            simplified["models"][0].get("model").and_then(Value::as_str),
            Some("kimi-k2")
        );
        assert_eq!(
            simplified["models"][0]
                .get("displayName")
                .and_then(Value::as_str),
            Some("Kimi K2")
        );
    }

    #[test]
    fn model_catalog_adapter_projects_provider_settings_and_client_raw() {
        let settings = json!({
            "model": " claude-sonnet-4 ",
            "env": {
                "ANTHROPIC_MODEL": "claude-opus-4"
            },
            "modelCatalog": {
                "models": [
                    {"model": "deepseek-v4"},
                    {"id": "kimi-k2"}
                ]
            }
        });

        let provider_catalog =
            provider_model_catalog_from_settings("provider-a", Some(&settings));
        assert_eq!(provider_catalog.provider_id, "provider-a");
        assert_eq!(
            provider_catalog.models,
            vec![
                "claude-opus-4".to_string(),
                "claude-sonnet-4".to_string(),
                "deepseek-v4".to_string(),
                "kimi-k2".to_string()
            ]
        );

        let client_catalog = client_model_catalog_from_raw(
            &AppKind::Codex,
            json!({
                "models": [
                    {"id": " gpt-5 "},
                    {"model": "o4-mini"},
                    {"id": "gpt-5"}
                ]
            }),
        );
        assert_eq!(client_catalog.provider_id, "codex");
        assert_eq!(
            client_catalog.models,
            vec!["gpt-5".to_string(), "o4-mini".to_string()]
        );
    }

    #[test]
    fn claude_takeover_adapter_projects_one_m_marker_and_display_name() {
        assert_eq!(
            claude_takeover_client_model_for_upstream(
                "claude-sonnet-4-6",
                true,
                "deepseek-v4-pro[1M]"
            ),
            "claude-sonnet-4-6[1M]"
        );
        assert_eq!(
            claude_takeover_client_model_for_upstream(
                "claude-haiku-4-5",
                false,
                "deepseek-v4-flash[1M]"
            ),
            "claude-haiku-4-5"
        );
        assert_eq!(
            claude_takeover_default_display_name("deepseek-v4-ultra [1m]  "),
            "deepseek-v4-ultra"
        );
    }

    #[test]
    fn host_session_adapter_generates_uuid_when_core_needs_new_session_id() {
        let headers = HeaderMap::new();
        let body = json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = extract_proxy_session_id(&headers, &body, "claude");

        uuid::Uuid::parse_str(&result.session_id).expect("generated session id should be a UUID");
        assert_eq!(result.source, SessionIdSource::Generated);
        assert!(!result.client_provided);
    }

    #[test]
    fn gemini_tool_call_id_adapter_uses_core_prefix_contract() {
        let id = synthesize_gemini_tool_call_id_with_uuid();

        assert!(id.starts_with(GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX));
        assert!(id.len() > GEMINI_SYNTHESIZED_TOOL_CALL_ID_PREFIX.len());
    }

    #[test]
    fn stream_check_adapter_preserves_reachability_fields() {
        let result = StreamCheckResult {
            status: HealthStatus::Degraded,
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

        for (health_status, reachability_status) in [
            (
                HealthStatus::Operational,
                ChannelReachabilityStatus::Operational,
            ),
            (HealthStatus::Failed, ChannelReachabilityStatus::Failed),
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

    #[test]
    fn provider_conversion_uses_inferred_provider_kind_without_settings_leak() {
        let mut provider = Provider::with_id(
            "copilot".to_string(),
            "Copilot".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "secret-token",
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com"
                }
            }),
            Some("https://github.com/features/copilot".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("github_copilot".to_string()),
                account_id: Some("acct-1".to_string()),
            }),
            ..ProviderMeta::default()
        });

        let spec = provider.to_proxy_core_provider_spec(&AppType::Claude);

        assert_eq!(spec.kind, ProviderKind::GitHubCopilot);
        assert_eq!(spec.account_ref.as_deref(), Some("github_copilot:acct-1"));
        let serialized = serde_json::to_string(&spec).expect("serialize spec");
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("ANTHROPIC_AUTH_TOKEN"));
        assert!(!serialized.contains("settingsConfig"));
    }

    #[test]
    fn channel_conversion_preserves_endpoint_interface_models_and_groups() {
        let channel = ProxyChannelRecord {
            id: "ch-1".to_string(),
            provider_id: "provider-1".to_string(),
            app_type: "claude".to_string(),
            name: "Relay A".to_string(),
            status: "enabled".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            interface_kind: "openai_chat_completions".to_string(),
            auth_profile_ref: Some("provider:claude:provider-1".to_string()),
            groups: vec!["default".to_string(), "paid".to_string()],
            priority: 50,
            weight: 80,
            retry_policy: json!({"maxAttempts": 2}),
            health_policy: json!({"breaker": "standard"}),
            header_overrides: json!({"x-test": "1"}),
            param_overrides: json!({"stream": true}),
            status_code_mapping: json!([{"from": 429, "to": 503}]),
            tags: vec!["manual".to_string()],
            metadata: json!({"owner": "ops"}),
            source_kind: ProxyChannelSourceKind::Manual,
            source_endpoint_url: Some("https://relay.example.com/v1".to_string()),
            models: vec![ProxyChannelModelRecord {
                channel_id: "ch-1".to_string(),
                public_model: "sonnet".to_string(),
                upstream_model: "anthropic/sonnet".to_string(),
                capabilities: json!({"tools": true}),
                pricing_model: Some("standard".to_string()),
                request_overrides: json!({"temperature": 0.2}),
                response_overrides: json!({}),
            }],
            needs_review: false,
            review_reasons: Vec::new(),
        };

        let spec = channel.to_proxy_core_channel_spec();

        assert_eq!(spec.app, AppKind::Claude);
        assert_eq!(spec.endpoint.base_url, "https://relay.example.com/v1");
        assert_eq!(spec.interface, InterfaceKind::OpenAiChatCompletions);
        assert_eq!(spec.groups, vec!["default".to_string(), "paid".to_string()]);
        assert_eq!(spec.models.len(), 1);
        assert_eq!(spec.models[0].public_model, "sonnet");
        assert_eq!(spec.models[0].upstream_model, "anthropic/sonnet");
        assert_eq!(
            spec.auth_profile.as_ref().map(|value| value.0.as_str()),
            Some("provider:claude:provider-1")
        );
    }
}
