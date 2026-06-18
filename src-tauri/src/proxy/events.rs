//! Lightweight proxy event bus.
//!
//! This is the host-side backing for the versioned `/proxy/v1/events` stream.
//! Keeping it independent from Tauri lets it become the future `ProxyEventSink`
//! implementation when the forwarding engine is moved behind service ports.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;

const EVENT_BUFFER_SIZE: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyEventEnvelope {
    pub id: u64,
    pub event: String,
    pub timestamp: String,
    pub payload: Value,
}

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
            "proxy_events_connected",
            json!({
                "bufferSize": EVENT_BUFFER_SIZE,
            }),
        )
    }

    pub fn lagged_event(&self, skipped: u64) -> ProxyEventEnvelope {
        self.envelope(
            "proxy_events_lagged",
            json!({
                "skipped": skipped,
            }),
        )
    }

    fn envelope(&self, event: impl Into<String>, payload: Value) -> ProxyEventEnvelope {
        ProxyEventEnvelope {
            id: self.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            event: event.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
