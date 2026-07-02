//! CC Switch HTTP proxy runtime state.

use crate::database::Database;
use crate::proxy::codex_chat_history::CodexChatHistoryStore;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy::events::ProxyEventBus;
use crate::proxy::host::cc_switch::failover_switch::{
    failover_switch_scheduler_from_runtime_sources, FailoverSwitchManager,
};
use crate::proxy::host::cc_switch::forwarder_attempt_runtime_source::forwarder_attempt_runtime_source_from_runtime_sources;
use crate::proxy::host::cc_switch::forwarder_auth_source::forwarder_auth_source_from_managed_account_runtime_source;
use crate::proxy::host::cc_switch::forwarder_protocol_state_source::forwarder_protocol_state_source_from_runtime_parts;
use crate::proxy::host::cc_switch::forwarder_request_source::forwarder_request_source_from_managed_account_runtime_source;
use crate::proxy::host::cc_switch::forwarder_response_source::default_forwarder_response_source;
use crate::proxy::host::cc_switch::forwarder_runtime_state_source::forwarder_runtime_state_source_from_runtime_parts;
use crate::proxy::host::cc_switch::forwarder_transport_source::default_forwarder_transport_source;
use crate::proxy::host::cc_switch::managed_account_runtime_source::managed_account_runtime_source_from_app_handle;
use crate::proxy::host::cc_switch::provider_router_sources::provider_router_from_database;
use crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime;
use crate::proxy::host::cc_switch::proxy_services::CcSwitchProxyServices;
use crate::proxy::host::cc_switch::request_context_provider_source::CcSwitchRequestContextProviderSource;
use crate::proxy_core::api::engine::ProxyEngine;
use crate::proxy_core::api::ports::{CurrentRouteTarget, ProxyConfig, ProxyRuntimeStatus};
use crate::proxy_core::api::transforms::GeminiShadowStore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 代理服务器状态（共享）
#[derive(Clone)]
pub struct ProxyState {
    pub config: Arc<RwLock<ProxyConfig>>,
    pub status: Arc<RwLock<ProxyRuntimeStatus>>,
    pub start_time: Arc<RwLock<Option<std::time::Instant>>>,
    /// 每个应用类型当前使用的 provider/channel target。
    pub current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    /// 共享的 ProviderRouter（持有熔断器状态，跨请求保持）
    pub provider_router: Arc<ProviderRouter>,
    /// Host adapter surface for the neutral proxy core contracts.
    pub proxy_core_services: Arc<CcSwitchProxyServices<CcSwitchProxyRuntime>>,
    /// Host source used when a routed core result must hydrate the selected Provider.
    pub request_context_provider_source: Arc<CcSwitchRequestContextProviderSource>,
    /// Gemini Native shadow state，用于 thoughtSignature / tool call 回放
    pub gemini_shadow: Arc<GeminiShadowStore>,
    /// Codex Chat bridge history，用于恢复 previous_response_id 指向的 tool call
    pub codex_chat_history: Arc<CodexChatHistoryStore>,
    /// 代理事件总线，供外部 SSE 监控和未来 ProxyEventSink 使用。
    pub events: Arc<ProxyEventBus>,
}

impl ProxyState {
    pub(crate) fn proxy_engine(&self) -> ProxyEngine<CcSwitchProxyServices<CcSwitchProxyRuntime>> {
        ProxyEngine::new(self.proxy_core_services.clone())
    }
}

pub(crate) fn proxy_state_from_runtime_sources(
    config: ProxyConfig,
    db: Arc<Database>,
    app_handle: Option<tauri::AppHandle>,
) -> ProxyState {
    let provider_router = Arc::new(provider_router_from_database(db.clone()));
    let events = Arc::new(ProxyEventBus::default());
    let failover_manager = Arc::new(FailoverSwitchManager::new(db.clone()));
    let config = Arc::new(RwLock::new(config));
    let status = Arc::new(RwLock::new(ProxyRuntimeStatus::default()));
    let start_time = Arc::new(RwLock::new(None));
    let current_providers = Arc::new(RwLock::new(HashMap::new()));
    let gemini_shadow = Arc::new(GeminiShadowStore::default());
    let codex_chat_history = Arc::new(CodexChatHistoryStore::default());
    let request_context_provider_source =
        Arc::new(CcSwitchRequestContextProviderSource::new(db.clone()));
    let attempt_runtime_source =
        forwarder_attempt_runtime_source_from_runtime_sources(provider_router.clone(), db.clone());
    let protocol_state_source = forwarder_protocol_state_source_from_runtime_parts(
        gemini_shadow.clone(),
        codex_chat_history.clone(),
    );
    let runtime_state_source = forwarder_runtime_state_source_from_runtime_parts(
        status.clone(),
        current_providers.clone(),
        events.clone(),
    );
    let failover_switch_scheduler = failover_switch_scheduler_from_runtime_sources(
        failover_manager.clone(),
        app_handle.clone(),
    );
    let managed_account_runtime_source =
        managed_account_runtime_source_from_app_handle(app_handle.clone());
    let auth_source = forwarder_auth_source_from_managed_account_runtime_source(
        managed_account_runtime_source.clone(),
    );
    let request_source = forwarder_request_source_from_managed_account_runtime_source(
        managed_account_runtime_source.clone(),
    );
    let transport_source = default_forwarder_transport_source();
    let response_source = default_forwarder_response_source();
    let proxy_core_services = Arc::new(CcSwitchProxyServices::with_runtime(CcSwitchProxyRuntime {
        db: db.clone(),
        config: config.clone(),
        provider_router: provider_router.clone(),
        status: status.clone(),
        start_time: start_time.clone(),
        events: events.clone(),
        current_providers: current_providers.clone(),
        attempt_runtime_source,
        protocol_state_source,
        runtime_state_source,
        auth_source,
        request_source,
        transport_source,
        response_source,
        failover_switch_scheduler,
    }));

    ProxyState {
        config,
        status,
        start_time,
        current_providers,
        provider_router,
        proxy_core_services,
        request_context_provider_source,
        gemini_shadow,
        codex_chat_history,
        events,
    }
}
