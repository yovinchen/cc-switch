use crate::response_transform::claude_api_format_needs_transform;
use http::HeaderMap;
use serde_json::{Map, Value};
use std::net::IpAddr;
use std::time::Duration;

pub const DEFAULT_UPSTREAM_SEND_TIMEOUT: Duration = Duration::from_secs(600);
pub const STREAMING_REQWEST_REQUEST_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
pub const SYSTEM_PROXY_ENV_KEYS: [&str; 6] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
];
pub const SUPPORTED_EXPLICIT_PROXY_SCHEMES: [&str; 4] = ["http", "https", "socks5", "socks5h"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamRequestTransportPolicy {
    pub is_streaming_request: bool,
    pub force_identity_encoding: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamTransportKind {
    PooledReqwest,
    RawHyper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamSendPolicy {
    pub transport: UpstreamTransportKind,
    pub base_timeout: Duration,
    pub reqwest_request_timeout: Option<Duration>,
    pub streaming_header_timeout: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamSendPolicyInput {
    pub is_socks_proxy: bool,
    pub preserve_exact_header_case: bool,
    pub request_is_streaming: bool,
    pub non_streaming_timeout: Duration,
    pub streaming_first_byte_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwarderTransformPlan {
    pub needs_transform: bool,
    pub use_claude_transform: bool,
    pub use_provider_transform: bool,
    pub claude_api_format_for_url: Option<String>,
    pub claude_api_format_for_transform: Option<String>,
    pub codex_responses_to_chat: bool,
}

pub struct ForwarderTransformPlanFacts<'a> {
    pub codex_responses_to_chat: bool,
    pub adapter_is_claude: bool,
    pub resolved_claude_api_format: Option<&'a str>,
    pub fallback_claude_api_format: Option<&'a str>,
    pub provider_transform_required: bool,
}

pub struct ForwarderProtocolPreparationInput<'a> {
    pub transform_plan: &'a ForwarderTransformPlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwarderRequestBodyTransformAction {
    ConvertCodexResponsesToChat,
    UseClaudeTransformedBody,
    ApplyProviderTransform,
    Passthrough,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwarderProtocolPreparation {
    pub should_transform_claude_request: bool,
    pub claude_api_format_for_transform: Option<String>,
    pub codex_chat_enrichment_enabled: bool,
}

pub fn forwarder_transform_plan_from_facts(
    input: ForwarderTransformPlanFacts<'_>,
) -> ForwarderTransformPlan {
    let fallback_claude_api_format = input
        .adapter_is_claude
        .then_some(input.fallback_claude_api_format)
        .flatten();
    let claude_api_format = input
        .resolved_claude_api_format
        .or(fallback_claude_api_format);
    let needs_transform = match input.resolved_claude_api_format {
        Some(api_format) => claude_api_format_needs_transform(api_format),
        None => input.provider_transform_required,
    };

    ForwarderTransformPlan {
        needs_transform,
        use_claude_transform: needs_transform && input.adapter_is_claude,
        use_provider_transform: needs_transform && !input.adapter_is_claude,
        claude_api_format_for_url: claude_api_format.map(str::to_string),
        claude_api_format_for_transform: input
            .adapter_is_claude
            .then(|| claude_api_format.unwrap_or("anthropic").to_string()),
        codex_responses_to_chat: input.codex_responses_to_chat,
    }
}

pub fn forwarder_protocol_preparation_from_transform_plan(
    input: ForwarderProtocolPreparationInput<'_>,
) -> ForwarderProtocolPreparation {
    let should_transform_claude_request =
        input.transform_plan.use_claude_transform && !input.transform_plan.codex_responses_to_chat;

    ForwarderProtocolPreparation {
        should_transform_claude_request,
        claude_api_format_for_transform: should_transform_claude_request
            .then(|| input.transform_plan.claude_api_format_for_transform.clone())
            .flatten(),
        codex_chat_enrichment_enabled: input.transform_plan.codex_responses_to_chat,
    }
}

pub fn forwarder_request_body_transform_action_from_plan(
    plan: &ForwarderTransformPlan,
    claude_transformed_body_available: bool,
) -> ForwarderRequestBodyTransformAction {
    if plan.codex_responses_to_chat {
        return ForwarderRequestBodyTransformAction::ConvertCodexResponsesToChat;
    }

    if plan.use_claude_transform {
        return if claude_transformed_body_available {
            ForwarderRequestBodyTransformAction::UseClaudeTransformedBody
        } else {
            ForwarderRequestBodyTransformAction::Passthrough
        };
    }

    if plan.use_provider_transform {
        return ForwarderRequestBodyTransformAction::ApplyProviderTransform;
    }

    ForwarderRequestBodyTransformAction::Passthrough
}

pub fn resolve_upstream_request_transport_policy(
    needs_transform: bool,
    codex_responses_to_chat: bool,
    endpoint: &str,
    body: &Value,
    headers: &HeaderMap,
) -> UpstreamRequestTransportPolicy {
    let is_streaming_request = is_streaming_upstream_request(endpoint, body, headers);

    UpstreamRequestTransportPolicy {
        is_streaming_request,
        force_identity_encoding: needs_transform
            || codex_responses_to_chat
            || is_streaming_request,
    }
}

pub fn request_body_stream_flag(body: &Value) -> bool {
    body.get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub fn is_streaming_upstream_request(endpoint: &str, body: &Value, headers: &HeaderMap) -> bool {
    if request_body_stream_flag(body) {
        return true;
    }

    if endpoint.contains("streamGenerateContent") || endpoint.contains("alt=sse") {
        return true;
    }

    headers
        .get(http::header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|accept| accept.contains("text/event-stream"))
        .unwrap_or(false)
}

pub fn is_socks_proxy_url(upstream_proxy_url: Option<&str>) -> bool {
    upstream_proxy_url
        .map(|url| url.starts_with("socks5"))
        .unwrap_or(false)
}

pub fn proxy_url_points_to_loopback_port(value: &str, loopback_port: u16) -> bool {
    let Some((host, port)) = parse_proxy_authority(value) else {
        return false;
    };

    port == Some(loopback_port) && proxy_host_is_loopback(&host)
}

pub fn proxy_values_point_to_loopback_port<I, V>(values: I, loopback_port: u16) -> bool
where
    I: IntoIterator<Item = V>,
    V: AsRef<str>,
{
    values.into_iter().any(|value| {
        let value = value.as_ref().trim();
        !value.is_empty() && proxy_url_points_to_loopback_port(value, loopback_port)
    })
}

pub fn invalid_explicit_proxy_url_message(
    proxy_url: &str,
    error: impl std::fmt::Display,
) -> String {
    format!(
        "Invalid proxy URL '{}': {}",
        crate::secret::mask_url_for_log(proxy_url),
        error
    )
}

pub fn invalid_explicit_proxy_scheme_message(proxy_url: &str, scheme: &str) -> String {
    format!(
        "Invalid proxy scheme '{}' in URL '{}'. Supported: {}",
        scheme,
        crate::secret::mask_url_for_log(proxy_url),
        SUPPORTED_EXPLICIT_PROXY_SCHEMES.join(", ")
    )
}

pub fn validate_explicit_proxy_url(proxy_url: &str) -> Result<(), String> {
    let parsed = proxy_url
        .parse::<http::Uri>()
        .map_err(|error| invalid_explicit_proxy_url_message(proxy_url, error))?;
    let scheme = parsed
        .scheme_str()
        .ok_or_else(|| invalid_explicit_proxy_url_message(proxy_url, "missing scheme"))?;

    if !SUPPORTED_EXPLICIT_PROXY_SCHEMES.contains(&scheme) {
        return Err(invalid_explicit_proxy_scheme_message(proxy_url, scheme));
    }

    if parsed.authority().is_none() {
        return Err(invalid_explicit_proxy_url_message(
            proxy_url,
            "missing authority",
        ));
    }

    Ok(())
}

fn proxy_host_is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    host.parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

fn parse_proxy_authority(value: &str) -> Option<(String, Option<u16>)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let authority_without_userinfo = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    let authority = authority_without_userinfo
        .rsplit_once('@')
        .map(|(_, host_port)| host_port)
        .unwrap_or(authority_without_userinfo)
        .trim();

    if authority.is_empty() {
        return None;
    }

    if let Some(rest) = authority.strip_prefix('[') {
        let (host, rest) = rest.split_once(']')?;
        let port = rest.strip_prefix(':').and_then(|value| value.parse().ok());
        return Some((host.to_string(), port));
    }

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, port.parse().ok()),
        _ => (authority, None),
    };

    Some((host.to_string(), port))
}

pub fn resolve_upstream_send_policy(input: UpstreamSendPolicyInput) -> UpstreamSendPolicy {
    let base_timeout = if input.non_streaming_timeout.is_zero() {
        DEFAULT_UPSTREAM_SEND_TIMEOUT
    } else {
        input.non_streaming_timeout
    };
    let transport = if input.is_socks_proxy || !input.preserve_exact_header_case {
        UpstreamTransportKind::PooledReqwest
    } else {
        UpstreamTransportKind::RawHyper
    };

    let (reqwest_request_timeout, streaming_header_timeout) =
        if matches!(transport, UpstreamTransportKind::PooledReqwest) {
            if input.request_is_streaming {
                let header_timeout = if input.streaming_first_byte_timeout.is_zero() {
                    base_timeout
                } else {
                    input.streaming_first_byte_timeout
                };
                (
                    Some(STREAMING_REQWEST_REQUEST_TIMEOUT),
                    Some(header_timeout),
                )
            } else if input.non_streaming_timeout.is_zero() {
                (None, None)
            } else {
                (Some(input.non_streaming_timeout), None)
            }
        } else {
            (None, None)
        };

    UpstreamSendPolicy {
        transport,
        base_timeout,
        reqwest_request_timeout,
        streaming_header_timeout,
    }
}

pub fn mapped_channel_response_status(status: u16, mapping: &Value) -> Option<u16> {
    match mapping {
        Value::Array(entries) => entries
            .iter()
            .filter_map(Value::as_object)
            .find_map(|entry| mapped_status_from_object(status, entry)),
        Value::Object(entries) => mapped_status_from_map(status, entries),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelResponseStatusMapping {
    pub original_status: http::StatusCode,
    pub mapped_status: http::StatusCode,
}

impl ChannelResponseStatusMapping {
    pub fn changed(self) -> bool {
        self.original_status != self.mapped_status
    }
}

pub fn resolve_channel_response_status_mapping(
    status: http::StatusCode,
    mapping: &Value,
) -> Option<ChannelResponseStatusMapping> {
    let mapped_status = mapped_channel_response_status(status.as_u16(), mapping)
        .and_then(|status| http::StatusCode::from_u16(status).ok())?;

    Some(ChannelResponseStatusMapping {
        original_status: status,
        mapped_status,
    })
}

pub fn invalid_mapped_channel_response_status_message(
    mapped: u16,
    error: impl std::fmt::Display,
) -> String {
    format!("invalid mapped channel response status {mapped}: {error}")
}

fn mapped_status_from_object(status: u16, entry: &Map<String, Value>) -> Option<u16> {
    let from = entry
        .get("from")
        .or_else(|| entry.get("source"))
        .or_else(|| entry.get("status"))
        .and_then(status_code_from_value)?;
    if from != status {
        return None;
    }

    entry
        .get("to")
        .or_else(|| entry.get("target"))
        .or_else(|| entry.get("statusCode"))
        .and_then(status_code_from_value)
}

fn mapped_status_from_map(status: u16, entries: &Map<String, Value>) -> Option<u16> {
    entries
        .get(&status.to_string())
        .and_then(status_code_from_value)
}

fn status_code_from_value(value: &Value) -> Option<u16> {
    match value {
        Value::Number(number) => number.as_u64().and_then(valid_status_code),
        Value::String(value) => value.trim().parse::<u64>().ok().and_then(valid_status_code),
        _ => None,
    }
}

fn valid_status_code(value: u64) -> Option<u16> {
    let value = u16::try_from(value).ok()?;
    http::StatusCode::from_u16(value).ok()?;
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::{
        forwarder_request_body_transform_action_from_plan,
        forwarder_protocol_preparation_from_transform_plan, forwarder_transform_plan_from_facts,
        invalid_explicit_proxy_url_message, invalid_mapped_channel_response_status_message,
        is_socks_proxy_url, is_streaming_upstream_request, mapped_channel_response_status,
        proxy_url_points_to_loopback_port, proxy_values_point_to_loopback_port,
        request_body_stream_flag, resolve_channel_response_status_mapping,
        resolve_upstream_request_transport_policy, resolve_upstream_send_policy,
        validate_explicit_proxy_url, ForwarderProtocolPreparationInput,
        ForwarderRequestBodyTransformAction, ForwarderTransformPlan, ForwarderTransformPlanFacts,
        UpstreamSendPolicyInput, UpstreamTransportKind, DEFAULT_UPSTREAM_SEND_TIMEOUT,
        STREAMING_REQWEST_REQUEST_TIMEOUT,
    };
    use http::{header::ACCEPT, HeaderMap, HeaderValue};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn forwarder_transform_plan_projects_host_facts() {
        let resolved_claude = forwarder_transform_plan_from_facts(ForwarderTransformPlanFacts {
            codex_responses_to_chat: false,
            adapter_is_claude: true,
            resolved_claude_api_format: Some("gemini_native"),
            fallback_claude_api_format: Some("openai_chat"),
            provider_transform_required: false,
        });
        assert!(resolved_claude.needs_transform);
        assert!(resolved_claude.use_claude_transform);
        assert!(!resolved_claude.use_provider_transform);
        assert_eq!(
            resolved_claude.claude_api_format_for_url.as_deref(),
            Some("gemini_native")
        );
        assert_eq!(
            resolved_claude.claude_api_format_for_transform.as_deref(),
            Some("gemini_native")
        );
        assert!(!resolved_claude.codex_responses_to_chat);

        let fallback_claude = forwarder_transform_plan_from_facts(ForwarderTransformPlanFacts {
            codex_responses_to_chat: false,
            adapter_is_claude: true,
            resolved_claude_api_format: None,
            fallback_claude_api_format: Some("openai_chat"),
            provider_transform_required: true,
        });
        assert!(fallback_claude.needs_transform);
        assert!(fallback_claude.use_claude_transform);
        assert!(!fallback_claude.use_provider_transform);
        assert_eq!(
            fallback_claude.claude_api_format_for_url.as_deref(),
            Some("openai_chat")
        );
        assert_eq!(
            fallback_claude.claude_api_format_for_transform.as_deref(),
            Some("openai_chat")
        );

        let provider_transform = forwarder_transform_plan_from_facts(ForwarderTransformPlanFacts {
            codex_responses_to_chat: true,
            adapter_is_claude: false,
            resolved_claude_api_format: None,
            fallback_claude_api_format: Some("openai_chat"),
            provider_transform_required: true,
        });
        assert!(provider_transform.needs_transform);
        assert!(!provider_transform.use_claude_transform);
        assert!(provider_transform.use_provider_transform);
        assert!(provider_transform.claude_api_format_for_url.is_none());
        assert!(provider_transform.claude_api_format_for_transform.is_none());
        assert!(provider_transform.codex_responses_to_chat);
    }

    #[test]
    fn forwarder_protocol_preparation_projects_transform_plan() {
        let claude_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            use_provider_transform: false,
            claude_api_format_for_url: Some("openai_chat".to_string()),
            claude_api_format_for_transform: Some("openai_chat".to_string()),
            codex_responses_to_chat: false,
        };
        let claude_preparation = forwarder_protocol_preparation_from_transform_plan(
            ForwarderProtocolPreparationInput {
                transform_plan: &claude_plan,
            },
        );
        assert!(claude_preparation.should_transform_claude_request);
        assert_eq!(
            claude_preparation.claude_api_format_for_transform.as_deref(),
            Some("openai_chat")
        );
        assert!(!claude_preparation.codex_chat_enrichment_enabled);

        let codex_bridge_plan = ForwarderTransformPlan {
            codex_responses_to_chat: true,
            ..claude_plan
        };
        let codex_preparation = forwarder_protocol_preparation_from_transform_plan(
            ForwarderProtocolPreparationInput {
                transform_plan: &codex_bridge_plan,
            },
        );
        assert!(!codex_preparation.should_transform_claude_request);
        assert!(codex_preparation.claude_api_format_for_transform.is_none());
        assert!(codex_preparation.codex_chat_enrichment_enabled);
    }

    #[test]
    fn forwarder_request_body_transform_action_preserves_execution_precedence() {
        let passthrough_plan = ForwarderTransformPlan {
            needs_transform: false,
            use_claude_transform: false,
            use_provider_transform: false,
            claude_api_format_for_url: None,
            claude_api_format_for_transform: None,
            codex_responses_to_chat: false,
        };
        assert_eq!(
            forwarder_request_body_transform_action_from_plan(&passthrough_plan, false),
            ForwarderRequestBodyTransformAction::Passthrough
        );

        let provider_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_provider_transform: true,
            ..passthrough_plan.clone()
        };
        assert_eq!(
            forwarder_request_body_transform_action_from_plan(&provider_plan, false),
            ForwarderRequestBodyTransformAction::ApplyProviderTransform
        );

        let claude_plan = ForwarderTransformPlan {
            needs_transform: true,
            use_claude_transform: true,
            ..passthrough_plan.clone()
        };
        assert_eq!(
            forwarder_request_body_transform_action_from_plan(&claude_plan, true),
            ForwarderRequestBodyTransformAction::UseClaudeTransformedBody
        );
        assert_eq!(
            forwarder_request_body_transform_action_from_plan(&claude_plan, false),
            ForwarderRequestBodyTransformAction::Passthrough
        );

        let codex_bridge_plan = ForwarderTransformPlan {
            codex_responses_to_chat: true,
            use_claude_transform: true,
            use_provider_transform: true,
            ..passthrough_plan
        };
        assert_eq!(
            forwarder_request_body_transform_action_from_plan(&codex_bridge_plan, true),
            ForwarderRequestBodyTransformAction::ConvertCodexResponsesToChat
        );
    }

    #[test]
    fn stream_flag_marks_request_as_streaming_and_forces_identity() {
        let headers = HeaderMap::new();
        assert!(request_body_stream_flag(&json!({ "stream": true })));
        assert!(!request_body_stream_flag(&json!({ "stream": "true" })));

        let policy = resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({ "stream": true }),
            &headers,
        );

        assert!(policy.is_streaming_request);
        assert!(policy.force_identity_encoding);
    }

    #[test]
    fn gemini_sse_endpoint_marks_request_as_streaming() {
        let headers = HeaderMap::new();

        assert!(is_streaming_upstream_request(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
            &json!({ "model": "gemini-2.5-pro" }),
            &headers
        ));
    }

    #[test]
    fn sse_accept_header_marks_request_as_streaming() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));

        let policy = resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({ "model": "gpt-5" }),
            &headers,
        );

        assert!(policy.is_streaming_request);
        assert!(policy.force_identity_encoding);
    }

    #[test]
    fn transform_paths_force_identity_even_for_non_streaming_requests() {
        let headers = HeaderMap::new();

        let transform_policy = resolve_upstream_request_transport_policy(
            true,
            false,
            "/v1/messages",
            &json!({ "model": "claude-sonnet-4" }),
            &headers,
        );
        let codex_chat_policy = resolve_upstream_request_transport_policy(
            false,
            true,
            "/v1/chat/completions",
            &json!({ "model": "gpt-5" }),
            &headers,
        );

        assert!(!transform_policy.is_streaming_request);
        assert!(transform_policy.force_identity_encoding);
        assert!(!codex_chat_policy.is_streaming_request);
        assert!(codex_chat_policy.force_identity_encoding);
    }

    #[test]
    fn ordinary_requests_allow_automatic_compression() {
        let headers = HeaderMap::new();

        let policy = resolve_upstream_request_transport_policy(
            false,
            false,
            "/v1/responses",
            &json!({ "model": "gpt-5" }),
            &headers,
        );

        assert!(!policy.is_streaming_request);
        assert!(!policy.force_identity_encoding);
    }

    #[test]
    fn socks_proxy_detection_requires_socks5_prefix() {
        assert!(is_socks_proxy_url(Some("socks5://127.0.0.1:1080")));
        assert!(!is_socks_proxy_url(Some("http://127.0.0.1:8080")));
        assert!(!is_socks_proxy_url(None));
    }

    #[test]
    fn loopback_proxy_detection_requires_matching_local_port() {
        assert!(proxy_url_points_to_loopback_port(
            "http://127.0.0.1:15721",
            15721
        ));
        assert!(proxy_url_points_to_loopback_port(
            "socks5://localhost:15721",
            15721
        ));
        assert!(proxy_url_points_to_loopback_port("127.0.0.1:15721", 15721));
        assert!(proxy_url_points_to_loopback_port("[::1]:15721", 15721));
        assert!(proxy_url_points_to_loopback_port(
            "http://user:pass@127.0.0.1:15721",
            15721
        ));

        assert!(!proxy_url_points_to_loopback_port(
            "http://127.0.0.1:7890",
            15721
        ));
        assert!(!proxy_url_points_to_loopback_port(
            "socks5://localhost:1080",
            15721
        ));
        assert!(!proxy_url_points_to_loopback_port(
            "http://192.168.1.10:15721",
            15721
        ));
        assert!(!proxy_url_points_to_loopback_port("", 15721));
    }

    #[test]
    fn loopback_proxy_detection_scans_trimmed_values() {
        assert!(proxy_values_point_to_loopback_port(
            ["", " http://127.0.0.1:15721 "],
            15721
        ));
        assert!(!proxy_values_point_to_loopback_port(
            ["", "http://127.0.0.1:7890", "http://10.0.0.2:15721"],
            15721
        ));
    }

    #[test]
    fn explicit_proxy_url_validation_preserves_host_contract() {
        assert!(validate_explicit_proxy_url("http://127.0.0.1:7890").is_ok());
        assert!(validate_explicit_proxy_url("https://proxy.example.com").is_ok());
        assert!(validate_explicit_proxy_url("socks5://localhost:1080").is_ok());
        assert!(validate_explicit_proxy_url("socks5h://localhost:1080").is_ok());

        let invalid_scheme =
            validate_explicit_proxy_url("ftp://127.0.0.1:7890").expect_err("invalid scheme");
        assert_eq!(
            invalid_scheme,
            "Invalid proxy scheme 'ftp' in URL 'ftp://127.0.0.1:7890'. Supported: http, https, socks5, socks5h"
        );

        let invalid_url = validate_explicit_proxy_url("http://[::1")
            .expect_err("invalid proxy URL should report parse error");
        assert!(invalid_url.contains("Invalid proxy URL 'http://[::1':"));

        assert_eq!(
            validate_explicit_proxy_url("localhost:1080").expect_err("missing scheme"),
            "Invalid proxy URL 'localhost:1080': missing scheme"
        );
        let missing_authority =
            validate_explicit_proxy_url("http:///proxy").expect_err("missing authority");
        assert!(missing_authority.contains("Invalid proxy URL 'http:///proxy':"));
        assert_eq!(
            invalid_explicit_proxy_url_message("http://user:pass@127.0.0.1:7890", "bad"),
            "Invalid proxy URL 'http://127.0.0.1:7890': bad"
        );
    }

    #[test]
    fn send_policy_uses_reqwest_when_socks_proxy_is_active() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: true,
            preserve_exact_header_case: true,
            request_is_streaming: false,
            non_streaming_timeout: Duration::from_secs(12),
            streaming_first_byte_timeout: Duration::from_secs(3),
        });

        assert_eq!(policy.transport, UpstreamTransportKind::PooledReqwest);
        assert_eq!(policy.base_timeout, Duration::from_secs(12));
        assert_eq!(policy.reqwest_request_timeout, Some(Duration::from_secs(12)));
        assert_eq!(policy.streaming_header_timeout, None);
    }

    #[test]
    fn send_policy_uses_reqwest_when_exact_header_case_is_not_required() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: false,
            preserve_exact_header_case: false,
            request_is_streaming: false,
            non_streaming_timeout: Duration::from_secs(0),
            streaming_first_byte_timeout: Duration::from_secs(0),
        });

        assert_eq!(policy.transport, UpstreamTransportKind::PooledReqwest);
        assert_eq!(policy.base_timeout, DEFAULT_UPSTREAM_SEND_TIMEOUT);
        assert_eq!(policy.reqwest_request_timeout, None);
        assert_eq!(policy.streaming_header_timeout, None);
    }

    #[test]
    fn send_policy_keeps_raw_hyper_when_exact_header_case_is_required_without_socks() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: false,
            preserve_exact_header_case: true,
            request_is_streaming: false,
            non_streaming_timeout: Duration::from_secs(30),
            streaming_first_byte_timeout: Duration::from_secs(5),
        });

        assert_eq!(policy.transport, UpstreamTransportKind::RawHyper);
        assert_eq!(policy.base_timeout, Duration::from_secs(30));
        assert_eq!(policy.reqwest_request_timeout, None);
        assert_eq!(policy.streaming_header_timeout, None);
    }

    #[test]
    fn streaming_reqwest_policy_uses_long_request_timeout_and_header_timeout() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: false,
            preserve_exact_header_case: false,
            request_is_streaming: true,
            non_streaming_timeout: Duration::from_secs(20),
            streaming_first_byte_timeout: Duration::from_secs(4),
        });

        assert_eq!(policy.transport, UpstreamTransportKind::PooledReqwest);
        assert_eq!(
            policy.reqwest_request_timeout,
            Some(STREAMING_REQWEST_REQUEST_TIMEOUT)
        );
        assert_eq!(
            policy.streaming_header_timeout,
            Some(Duration::from_secs(4))
        );
    }

    #[test]
    fn streaming_reqwest_header_timeout_falls_back_to_base_timeout() {
        let policy = resolve_upstream_send_policy(UpstreamSendPolicyInput {
            is_socks_proxy: false,
            preserve_exact_header_case: false,
            request_is_streaming: true,
            non_streaming_timeout: Duration::from_secs(0),
            streaming_first_byte_timeout: Duration::from_secs(0),
        });

        assert_eq!(
            policy.streaming_header_timeout,
            Some(DEFAULT_UPSTREAM_SEND_TIMEOUT)
        );
    }

    #[test]
    fn maps_channel_response_status_from_array_or_object_rules() {
        assert_eq!(
            mapped_channel_response_status(
                429,
                &json!([
                    {"from": 500, "to": 502},
                    {"from": 429, "to": 503}
                ])
            ),
            Some(503)
        );
        assert_eq!(
            mapped_channel_response_status(
                418,
                &json!({
                    "418": "502",
                    "429": 503
                })
            ),
            Some(502)
        );
    }

    #[test]
    fn formats_invalid_mapped_channel_response_status_errors() {
        assert_eq!(
            invalid_mapped_channel_response_status_message(99, "invalid status code"),
            "invalid mapped channel response status 99: invalid status code"
        );
    }

    #[test]
    fn resolves_channel_response_status_mapping_with_valid_status_codes() {
        let mapping = resolve_channel_response_status_mapping(
            http::StatusCode::TOO_MANY_REQUESTS,
            &json!([{"from": 429, "to": 200}]),
        )
        .expect("status mapping");

        assert_eq!(mapping.original_status, http::StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(mapping.mapped_status, http::StatusCode::OK);
        assert!(mapping.changed());

        assert_eq!(
            resolve_channel_response_status_mapping(
                http::StatusCode::TOO_MANY_REQUESTS,
                &json!([{"from": 429, "to": "rate_limited"}])
            ),
            None
        );
    }

    #[test]
    fn ignores_invalid_channel_response_status_rules() {
        assert_eq!(
            mapped_channel_response_status(
                429,
                &json!([
                    {"from": 429, "to": "rate_limited"},
                    {"from": 429, "to": 99}
                ])
            ),
            None
        );
        assert_eq!(mapped_channel_response_status(429, &json!("bad")), None);
    }
}
