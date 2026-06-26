//! Lightweight proxy event bus.
//!
//! This is the host-side backing for the versioned `/proxy/v1/events` stream.
//! Keeping it independent from Tauri lets it become the future `ProxyEventSink`
//! implementation when the forwarding engine is moved behind service ports.

use crate::proxy_core::api::events::{
    build_proxy_events_connected_payload, build_proxy_events_lagged_payload, ProxyEventEnvelope,
    PROXY_EVENTS_CONNECTED_EVENT, PROXY_EVENTS_LAGGED_EVENT,
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
}
