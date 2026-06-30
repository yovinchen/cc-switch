use crate::database::Database;
use crate::proxy::host::cc_switch::proxy_state::proxy_state_from_runtime_sources;
use crate::proxy::transport::http::server::ProxyServer;
use crate::proxy_core::api::ports::ProxyConfig;
use std::sync::Arc;

pub(crate) fn proxy_server_from_runtime_config(
    config: ProxyConfig,
    db: Arc<Database>,
    app_handle: Option<tauri::AppHandle>,
) -> ProxyServer {
    let state = proxy_state_from_runtime_sources(config.clone(), db, app_handle);
    ProxyServer::from_runtime_state(config, state)
}
