use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PROXY_EVENTS_CONNECTED_EVENT: &str = "proxy_events_connected";
pub const PROXY_EVENTS_LAGGED_EVENT: &str = "proxy_events_lagged";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyEventEnvelope {
    pub id: u64,
    pub event: String,
    pub timestamp: String,
    pub payload: Value,
}

impl ProxyEventEnvelope {
    pub fn new(
        id: u64,
        event: impl Into<String>,
        timestamp: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            id,
            event: event.into(),
            timestamp: timestamp.into(),
            payload,
        }
    }

    pub fn sse_data_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptEventPhase {
    Started,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptEventChannel<'a> {
    pub channel_id: &'a str,
    pub channel_name: &'a str,
    pub interface_kind: &'a str,
    pub public_model: Option<&'a str>,
    pub upstream_model: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptEventPayloadInput<'a> {
    pub request_id: &'a str,
    pub app_type: &'a str,
    pub provider_id: &'a str,
    pub provider_name: &'a str,
    pub channel: Option<AttemptEventChannel<'a>>,
    pub error: Option<&'a str>,
}

pub fn attempt_event_name(is_channel_attempt: bool, phase: AttemptEventPhase) -> &'static str {
    match (is_channel_attempt, phase) {
        (true, AttemptEventPhase::Started) => "channel_attempt",
        (true, AttemptEventPhase::Succeeded) => "channel_succeeded",
        (true, AttemptEventPhase::Failed) => "channel_failed",
        (false, AttemptEventPhase::Started) => "provider_attempt",
        (false, AttemptEventPhase::Succeeded) => "provider_succeeded",
        (false, AttemptEventPhase::Failed) => "provider_failed",
    }
}

pub fn build_attempt_event_payload(input: AttemptEventPayloadInput<'_>) -> Value {
    let mut payload = json!({
        "requestId": input.request_id,
        "appType": input.app_type,
        "providerId": input.provider_id,
        "providerName": input.provider_name,
    });

    if let Value::Object(ref mut object) = payload {
        if let Some(channel) = input.channel {
            object.insert(
                "channelId".to_string(),
                Value::String(channel.channel_id.to_string()),
            );
            object.insert(
                "channelName".to_string(),
                Value::String(channel.channel_name.to_string()),
            );
            object.insert(
                "interfaceKind".to_string(),
                Value::String(channel.interface_kind.to_string()),
            );
            if let Some(public_model) = channel.public_model {
                object.insert(
                    "publicModel".to_string(),
                    Value::String(public_model.to_string()),
                );
            }
            if let Some(upstream_model) = channel.upstream_model {
                object.insert(
                    "upstreamModel".to_string(),
                    Value::String(upstream_model.to_string()),
                );
            }
        }

        if let Some(error) = input.error {
            object.insert("error".to_string(), Value::String(error.to_string()));
        }
    }

    payload
}

pub fn build_request_started_event_payload(request_id: &str, app_type: &str) -> Value {
    json!({
        "requestId": request_id,
        "appType": app_type,
    })
}

pub fn build_proxy_events_connected_payload(buffer_size: usize) -> Value {
    json!({
        "bufferSize": buffer_size,
    })
}

pub fn build_proxy_events_lagged_payload(skipped: u64) -> Value {
    json!({
        "skipped": skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        attempt_event_name, build_attempt_event_payload, build_proxy_events_connected_payload,
        build_proxy_events_lagged_payload, build_request_started_event_payload,
        AttemptEventChannel, AttemptEventPayloadInput, AttemptEventPhase, ProxyEventEnvelope,
    };

    #[test]
    fn attempt_event_names_distinguish_channel_and_provider_attempts() {
        assert_eq!(
            attempt_event_name(true, AttemptEventPhase::Started),
            "channel_attempt"
        );
        assert_eq!(
            attempt_event_name(false, AttemptEventPhase::Started),
            "provider_attempt"
        );
        assert_eq!(
            attempt_event_name(true, AttemptEventPhase::Succeeded),
            "channel_succeeded"
        );
        assert_eq!(
            attempt_event_name(false, AttemptEventPhase::Succeeded),
            "provider_succeeded"
        );
        assert_eq!(
            attempt_event_name(true, AttemptEventPhase::Failed),
            "channel_failed"
        );
        assert_eq!(
            attempt_event_name(false, AttemptEventPhase::Failed),
            "provider_failed"
        );
    }

    #[test]
    fn attempt_payload_includes_channel_fields_when_present() {
        let payload = build_attempt_event_payload(AttemptEventPayloadInput {
            request_id: "req-1",
            app_type: "claude",
            provider_id: "provider-1",
            provider_name: "Provider 1",
            channel: Some(AttemptEventChannel {
                channel_id: "channel-a",
                channel_name: "Relay A",
                interface_kind: "openai_responses",
                public_model: Some("public-sonnet"),
                upstream_model: Some("upstream-sonnet"),
            }),
            error: Some("upstream failed"),
        });

        assert_eq!(payload["requestId"], "req-1");
        assert_eq!(payload["providerId"], "provider-1");
        assert_eq!(payload["channelId"], "channel-a");
        assert_eq!(payload["interfaceKind"], "openai_responses");
        assert_eq!(payload["publicModel"], "public-sonnet");
        assert_eq!(payload["upstreamModel"], "upstream-sonnet");
        assert_eq!(payload["error"], "upstream failed");
    }

    #[test]
    fn provider_attempt_payload_omits_channel_and_error_fields() {
        let payload = build_attempt_event_payload(AttemptEventPayloadInput {
            request_id: "req-1",
            app_type: "codex",
            provider_id: "provider-1",
            provider_name: "Provider 1",
            channel: None,
            error: None,
        });

        assert_eq!(payload["requestId"], "req-1");
        assert_eq!(payload["appType"], "codex");
        assert!(payload.get("channelId").is_none());
        assert!(payload.get("error").is_none());
    }

    #[test]
    fn request_started_payload_contains_request_and_app_identity() {
        let payload = build_request_started_event_payload("req-1", "claude");

        assert_eq!(payload["requestId"], "req-1");
        assert_eq!(payload["appType"], "claude");
    }

    #[test]
    fn proxy_event_envelope_uses_camel_case_contract() {
        let envelope = ProxyEventEnvelope::new(
            7,
            "request_started",
            "2026-06-19T00:00:00Z",
            build_request_started_event_payload("req-1", "claude"),
        );

        let serialized = serde_json::to_value(envelope).expect("serialize envelope");

        assert_eq!(serialized["id"], 7);
        assert_eq!(serialized["event"], "request_started");
        assert_eq!(serialized["timestamp"], "2026-06-19T00:00:00Z");
        assert_eq!(serialized["payload"]["requestId"], "req-1");
    }

    #[test]
    fn proxy_event_envelope_sse_data_serializes_envelope() {
        let envelope = ProxyEventEnvelope::new(
            7,
            "proxy_events_connected",
            "2026-06-19T00:00:00Z",
            build_proxy_events_connected_payload(256),
        );

        let serialized: serde_json::Value =
            serde_json::from_str(&envelope.sse_data_json()).expect("serialize envelope");

        assert_eq!(serialized["id"], 7);
        assert_eq!(serialized["event"], "proxy_events_connected");
        assert_eq!(serialized["payload"]["bufferSize"], 256);
    }

    #[test]
    fn proxy_event_stream_control_payloads_keep_existing_shape() {
        assert_eq!(
            build_proxy_events_connected_payload(256),
            serde_json::json!({ "bufferSize": 256 })
        );
        assert_eq!(
            build_proxy_events_lagged_payload(3),
            serde_json::json!({ "skipped": 3 })
        );
    }
}
