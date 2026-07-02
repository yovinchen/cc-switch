use crate::database::Database;
use crate::proxy::events::ProxyEventBus;
use crate::proxy::host::cc_switch::global_http_client::set_proxy_port;
use crate::proxy::host::cc_switch::proxy_state::proxy_state_from_runtime_sources;
use crate::proxy::host::cc_switch::proxy_state::ProxyState;
use crate::proxy::transport::http::server::ProxyServer;
use crate::proxy_core::api::events::{server_started_event, server_stopped_event};
use crate::proxy_core::api::ports::{
    proxy_server_info_from_parts,
    record_proxy_server_started_status as core_record_proxy_server_started_status,
    record_proxy_server_stopped_status, ProxyConfig, ProxyRuntimeStatus, ProxyServerInfo,
    ProxyServerStartedStatusInput,
};
use std::sync::Arc;
use tokio::sync::RwLock;

pub(crate) fn proxy_server_from_runtime_config(
    config: ProxyConfig,
    db: Arc<Database>,
    app_handle: Option<tauri::AppHandle>,
) -> ProxyServer {
    let state = proxy_state_from_runtime_sources(config.clone(), db, app_handle);
    ProxyServer::from_runtime_state(config, state)
}

fn emit_proxy_server_started_event_source(events: &ProxyEventBus, address: &str, port: u16) {
    events.emit_core_event(server_started_event(address, port));
}

fn emit_proxy_server_stopped_event_source(events: &ProxyEventBus) {
    events.emit_core_event(server_stopped_event());
}

fn record_proxy_server_started_status(status: &mut ProxyRuntimeStatus, address: &str, port: u16) {
    core_record_proxy_server_started_status(
        status,
        ProxyServerStartedStatusInput { address, port },
    );
}

fn record_proxy_server_listen_port_runtime_source(port: u16) {
    set_proxy_port(port);
}

pub(crate) async fn record_proxy_server_started_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    start_time: &RwLock<Option<std::time::Instant>>,
    address: &str,
    port: u16,
) {
    {
        let mut status = status.write().await;
        record_proxy_server_started_status(&mut status, address, port);
    }
    *start_time.write().await = Some(std::time::Instant::now());
}

pub(crate) fn record_proxy_server_bound_runtime_source(
    state: &ProxyState,
    bound_address: &str,
    port: u16,
) {
    emit_proxy_server_started_event_source(state.events.as_ref(), bound_address, port);
    record_proxy_server_listen_port_runtime_source(port);
}

pub(crate) async fn record_proxy_server_started_info_runtime_source(
    state: &ProxyState,
    listen_address: &str,
    port: u16,
) -> ProxyServerInfo {
    record_proxy_server_started_runtime_source(
        state.status.as_ref(),
        state.start_time.as_ref(),
        listen_address,
        port,
    )
    .await;

    proxy_server_info_from_parts(
        listen_address.to_string(),
        port,
        chrono::Utc::now().to_rfc3339(),
    )
}

pub(crate) async fn record_proxy_server_stopped_runtime_source(
    status: &RwLock<ProxyRuntimeStatus>,
    start_time: &RwLock<Option<std::time::Instant>>,
) {
    {
        let mut status = status.write().await;
        record_proxy_server_stopped_status(&mut status);
    }
    *start_time.write().await = None;
}

pub(crate) async fn record_proxy_server_stopped_runtime_event_source(state: &ProxyState) {
    record_proxy_server_stopped_runtime_source(state.status.as_ref(), state.start_time.as_ref())
        .await;
    emit_proxy_server_stopped_event_source(state.events.as_ref());
}
