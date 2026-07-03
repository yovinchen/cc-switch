use futures::future::BoxFuture;
use std::sync::Arc;

use crate::proxy::events::ProxyEventBus;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::events::ProxyCoreEvent;
use crate::proxy_core::api::ports::ProxyEventSink;

#[derive(Clone, Default)]
pub(crate) struct CcSwitchEventSink {
    events: Option<Arc<ProxyEventBus>>,
}

impl CcSwitchEventSink {
    pub(crate) fn new(events: Option<Arc<ProxyEventBus>>) -> Self {
        Self { events }
    }
}

impl ProxyEventSink for CcSwitchEventSink {
    fn emit_event<'a>(&'a self, event: ProxyCoreEvent) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move {
            if let Some(events) = self.events.as_ref() {
                events.emit_core_event(event);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn proxy_core_event_type<T: serde::de::DeserializeOwned>(value: &str) -> T {
        serde_json::from_value(json!(value)).expect("proxy core event type")
    }

    #[tokio::test]
    async fn event_sink_bridges_core_events_to_proxy_event_bus() {
        let events = Arc::new(ProxyEventBus::default());
        let mut subscriber = events.subscribe();
        let sink = CcSwitchEventSink::new(Some(events));

        sink.emit_event(ProxyCoreEvent {
            event_type: proxy_core_event_type("route_selected"),
            request_id: Some("req-1".to_string()),
            channel_id: Some("channel-a".to_string()),
            payload: json!({
                "attemptCount": 2,
            }),
        })
        .await
        .expect("emit event");

        let event = subscriber.recv().await.expect("receive event");
        assert_eq!(event.event, "route_selected");
        assert_eq!(event.payload["requestId"], "req-1");
        assert_eq!(event.payload["channelId"], "channel-a");
        assert_eq!(event.payload["attemptCount"], 2);
    }
}
