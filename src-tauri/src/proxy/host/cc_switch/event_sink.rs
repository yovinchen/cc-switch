use futures::future::BoxFuture;
use std::sync::Arc;

use crate::proxy::events::ProxyEventBus;
use crate::proxy_core_adapter::{
    ProxyCoreEvent, ProxyCoreResult, ProxyEventSink, emit_proxy_core_event_bus_source,
};

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
                emit_proxy_core_event_bus_source(events.as_ref(), event);
            }
            Ok(())
        })
    }
}
