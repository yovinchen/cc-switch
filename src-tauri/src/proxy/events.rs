//! Lightweight proxy event bus.
//!
//! This is the host-side backing for the versioned `/proxy/v1/events` stream.
//! Keeping it independent from Tauri lets it become the future `ProxyEventSink`
//! implementation when the forwarding engine is moved behind service ports.

use crate::proxy_core::api::events::{
    build_proxy_events_connected_payload, build_proxy_events_lagged_payload, ProxyCoreEvent,
    ProxyEventEnvelope, PROXY_EVENTS_CONNECTED_EVENT, PROXY_EVENTS_LAGGED_EVENT,
};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;

const EVENT_BUFFER_SIZE: usize = 256;

#[derive(Debug)]
pub struct ProxyEventBus {
    sequence: AtomicU64,
    sender: broadcast::Sender<ProxyEventEnvelope>,
}

impl Default for ProxyEventBus {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUFFER_SIZE);
        Self {
            sequence: AtomicU64::new(0),
            sender,
        }
    }
}

impl ProxyEventBus {
    pub fn subscribe(&self) -> broadcast::Receiver<ProxyEventEnvelope> {
        self.sender.subscribe()
    }

    pub fn emit(&self, event: impl Into<String>, payload: Value) -> ProxyEventEnvelope {
        let envelope = self.envelope(event, payload);
        let _ = self.sender.send(envelope.clone());
        envelope
    }

    pub fn emit_core_event(&self, event: ProxyCoreEvent) -> ProxyEventEnvelope {
        self.emit(event.event_type.event_name(), event.into_event_payload())
    }

    pub fn connected_event(&self) -> ProxyEventEnvelope {
        self.envelope(
            PROXY_EVENTS_CONNECTED_EVENT,
            build_proxy_events_connected_payload(EVENT_BUFFER_SIZE),
        )
    }

    pub fn lagged_event(&self, skipped: u64) -> ProxyEventEnvelope {
        self.envelope(
            PROXY_EVENTS_LAGGED_EVENT,
            build_proxy_events_lagged_payload(skipped),
        )
    }

    fn envelope(&self, event: impl Into<String>, payload: Value) -> ProxyEventEnvelope {
        ProxyEventEnvelope::new(
            self.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            event,
            chrono::Utc::now().to_rfc3339(),
            payload,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppType;
    use crate::provider::Provider;
    use crate::proxy::route_attempt::ForwardAttempt;
    use crate::proxy_core::api::events::{
        attempt_event, request_started_event, route_selected_event, server_started_event,
        server_stopped_event, AttemptEventChannel, AttemptEventPayloadInput, AttemptEventPhase,
    };
    use crate::proxy_core::api::routing::ChannelRouteCandidate;
    use serde_json::json;

    #[tokio::test]
    async fn event_bus_broadcasts_ordered_envelopes() {
        let bus = ProxyEventBus::default();
        let mut subscriber = bus.subscribe();

        let emitted = bus.emit("request_started", json!({ "appType": "claude" }));
        let received = subscriber.recv().await.expect("event");

        assert_eq!(emitted, received);
        assert_eq!(received.id, 1);
        assert_eq!(received.event, "request_started");
        assert_eq!(received.payload["appType"], "claude");
    }

    #[test]
    fn event_bus_projects_connected_and_lagged_messages() {
        let bus = ProxyEventBus::default();

        let connected = bus.connected_event();
        assert_eq!(connected.event, "proxy_events_connected");
        assert_eq!(connected.payload["bufferSize"], EVENT_BUFFER_SIZE);

        let lagged = bus.lagged_event(7);
        assert_eq!(lagged.event, "proxy_events_lagged");
        assert_eq!(lagged.payload["skipped"], 7);
    }

    #[test]
    fn event_bus_projects_core_events() {
        let bus = ProxyEventBus::default();

        let envelope = bus.emit_core_event(crate::proxy_core::api::events::request_started_event(
            "req-1", "claude",
        ));

        assert_eq!(envelope.event, "request_started");
        assert_eq!(envelope.payload["requestId"], "req-1");
        assert_eq!(envelope.payload["appType"], "claude");
    }

    #[test]
    fn event_bus_projects_event_stream_contracts() {
        let bus = ProxyEventBus::default();

        assert_eq!(
            crate::proxy_core::api::events::PROXY_OFFICIAL_WARNING_EVENT,
            "proxy-official-warning"
        );
        assert_eq!(
            crate::proxy_core::api::events::PROVIDER_SWITCHED_EVENT,
            "provider-switched"
        );
        assert_eq!(
            crate::proxy_core::api::events::REQUEST_STARTED_EVENT,
            "request_started"
        );
        assert_eq!(
            crate::proxy_core::api::events::SERVER_STARTED_EVENT,
            "server_started"
        );
        assert_eq!(
            crate::proxy_core::api::events::SERVER_STOPPED_EVENT,
            "server_stopped"
        );
        let official_warning = bus.emit_core_event(
            crate::proxy_core::api::events::proxy_official_warning_event(
                "claude",
                "Official Claude",
            ),
        );
        assert_eq!(official_warning.event, "proxy-official-warning");
        assert_eq!(
            official_warning.payload,
            json!({
                "appType": "claude",
                "providerName": "Official Claude",
            })
        );
        let mut provider = Provider::with_id(
            "official-codex".to_string(),
            "Official Codex".to_string(),
            json!({}),
            None,
        );
        provider.category = Some("official".to_string());
        assert!(
            crate::proxy_core::api::ports::provider_category_is_official(
                provider.category.as_deref()
            )
        );
        assert!(
            crate::proxy_core::api::ports::should_emit_proxy_official_warning_for_provider_category(
                provider.category.as_deref()
            )
        );
        assert!(
            crate::proxy_core::api::ports::should_reapply_codex_official_live_for_provider_category(
                provider.category.as_deref()
            )
        );
        let official_warning_from_core = bus.emit_core_event(
            crate::proxy_core::api::events::proxy_official_warning_event("codex", &provider.name),
        );
        assert_eq!(official_warning_from_core.event, "proxy-official-warning");
        assert_eq!(official_warning_from_core.payload["appType"], "codex");
        assert_eq!(
            official_warning_from_core.payload["providerName"],
            "Official Codex"
        );
        provider.category = Some("custom".to_string());
        assert!(
            !crate::proxy_core::api::ports::provider_category_is_official(
                provider.category.as_deref()
            )
        );
        assert!(
            !crate::proxy_core::api::ports::should_emit_proxy_official_warning_for_provider_category(
                provider.category.as_deref()
            )
        );
        assert!(
            !crate::proxy_core::api::ports::should_reapply_codex_official_live_for_provider_category(
                provider.category.as_deref()
            )
        );
        provider.category = None;
        assert!(
            !crate::proxy_core::api::ports::provider_category_is_official(
                provider.category.as_deref()
            )
        );
        assert!(
            !crate::proxy_core::api::ports::should_emit_proxy_official_warning_for_provider_category(
                provider.category.as_deref()
            )
        );
        assert!(
            !crate::proxy_core::api::ports::should_reapply_codex_official_live_for_provider_category(
                provider.category.as_deref()
            )
        );
        let provider_switched = bus.emit_core_event(
            crate::proxy_core::api::events::provider_switched_failover_event(
                "claude",
                "provider-1",
            ),
        );
        assert_eq!(provider_switched.event, "provider-switched");
        assert_eq!(provider_switched.payload["source"], "failover");
        let provider_switched_enabled = bus.emit_core_event(
            crate::proxy_core::api::events::provider_switched_failover_enabled_event(
                "claude",
                "provider-1",
            ),
        );
        assert_eq!(provider_switched_enabled.event, "provider-switched");
        assert_eq!(
            provider_switched_enabled.payload["source"],
            "failoverEnabled"
        );
        let server_started = bus.emit_core_event(server_started_event("127.0.0.1", 15721));
        assert_eq!(server_started.event, "server_started");
        assert_eq!(
            server_started.payload,
            json!({"address": "127.0.0.1", "port": 15721})
        );
        let server_stopped = bus.emit_core_event(server_stopped_event());
        assert_eq!(server_stopped.event, "server_stopped");
        assert!(server_stopped
            .payload
            .as_object()
            .is_some_and(|object| object.is_empty()));

        let envelope = ProxyEventEnvelope::new(
            42,
            "request_started",
            "2026-06-20T00:00:00Z",
            json!({"provider": "relay-a"}),
        );
        let spec = envelope.to_sse_spec();

        assert_eq!(spec.id, "42");
        assert_eq!(spec.event, "request_started");
        assert!(spec.data.contains("\"provider\":\"relay-a\""));

        let request_started = bus.emit_core_event(request_started_event("req-start", "claude"));
        assert_eq!(request_started.event, "request_started");
        assert_eq!(request_started.payload["requestId"], "req-start");
        assert_eq!(request_started.payload["appType"], "claude");

        let message = bus.emit_core_event(ProxyCoreEvent {
            event_type: crate::proxy_core::api::events::ProxyCoreEventType::RouteSelected,
            request_id: Some("req-1".to_string()),
            channel_id: Some("channel-a".to_string()),
            payload: json!({"attemptCount": 2}),
        });
        assert_eq!(message.event, "route_selected");
        assert_eq!(message.payload["requestId"], "req-1");
        assert_eq!(message.payload["channelId"], "channel-a");
        assert_eq!(message.payload["attemptCount"], 2);

        let route_provider = Provider::with_id(
            "provider-1".to_string(),
            "Relay Provider".to_string(),
            json!({}),
            None,
        );
        let route_attempt = ForwardAttempt::from_channel(
            &AppType::Claude,
            &route_provider,
            ChannelRouteCandidate {
                channel_id: "channel-a".to_string(),
                provider_id: route_provider.id.clone(),
                channel_name: "Relay A".to_string(),
                base_url: "https://relay.example.com/v1".to_string(),
                interface_kind: "openai_responses".to_string(),
                public_model: Some("public-sonnet".to_string()),
                upstream_model: Some("upstream-sonnet".to_string()),
                route_group: "default".to_string(),
                priority: 100,
                weight: 50,
                source_kind: "manual".to_string(),
            },
        );
        let route_provider = route_attempt.provider();
        let route_channel = route_attempt.channel().expect("channel route attempt");
        let route_payload = AttemptEventPayloadInput {
            request_id: "req-route",
            app_type: "claude",
            provider_id: route_provider.id.as_str(),
            provider_name: route_provider.name.as_str(),
            channel: Some(AttemptEventChannel {
                channel_id: route_channel.channel_id.as_str(),
                channel_name: route_channel.channel_name.as_str(),
                interface_kind: route_channel.interface_kind.as_str(),
                public_model: route_channel.public_model.as_deref(),
                upstream_model: route_channel.upstream_model.as_deref(),
                pricing_model: route_channel.pricing_model.as_deref(),
            }),
            error: None,
        };
        let route_message = bus.emit_core_event(route_selected_event(route_payload));
        assert_eq!(route_message.event, "route_selected");
        assert_eq!(route_message.payload["requestId"], "req-route");
        assert_eq!(route_message.payload["providerId"], "provider-1");
        assert_eq!(route_message.payload["channelId"], "channel-a");
        assert_eq!(route_message.payload["interfaceKind"], "openai_responses");
        assert_eq!(route_message.payload["upstreamModel"], "upstream-sonnet");

        let failed_attempt_message = bus.emit_core_event(attempt_event(
            AttemptEventPayloadInput {
                request_id: "req-failed",
                error: Some("upstream failed"),
                ..route_payload
            },
            route_attempt.is_channel(),
            AttemptEventPhase::Failed,
        ));
        assert_eq!(failed_attempt_message.event, "channel_failed");
        assert_eq!(failed_attempt_message.payload["requestId"], "req-failed");
        assert_eq!(failed_attempt_message.payload["channelId"], "channel-a");
        assert_eq!(failed_attempt_message.payload["error"], "upstream failed");

        let emitted = bus.emit_core_event(ProxyCoreEvent {
            event_type: crate::proxy_core::api::events::ProxyCoreEventType::RouteSelected,
            request_id: Some("req-2".to_string()),
            channel_id: Some("channel-b".to_string()),
            payload: json!({"attemptCount": 1}),
        });
        assert_eq!(emitted.event, "route_selected");
        assert_eq!(emitted.payload["requestId"], "req-2");
        assert_eq!(emitted.payload["channelId"], "channel-b");
        assert_eq!(emitted.payload["attemptCount"], 1);
    }
}
