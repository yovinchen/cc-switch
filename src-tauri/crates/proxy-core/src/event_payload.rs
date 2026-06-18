use serde_json::{json, Value};

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

#[cfg(test)]
mod tests {
    use super::{
        attempt_event_name, build_attempt_event_payload, build_request_started_event_payload,
        AttemptEventChannel, AttemptEventPayloadInput, AttemptEventPhase,
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
}
