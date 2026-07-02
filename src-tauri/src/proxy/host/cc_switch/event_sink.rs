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
