//! CC Switch HTTP proxy runtime state.

use crate::database::Database;
use crate::proxy::codex_chat_history::CodexChatHistoryStore;
use crate::proxy::engine::routing::ProviderRouter;
use crate::proxy::events::ProxyEventBus;
use crate::proxy::host::cc_switch::proxy_runtime::CcSwitchProxyRuntime;
use crate::proxy::host::cc_switch::proxy_services::CcSwitchProxyServices;
use crate::proxy_core::api::ports::{CurrentRouteTarget, ProxyConfig, ProxyRuntimeStatus};
use crate::proxy_core::api::transforms::GeminiShadowStore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 代理服务器状态（共享）
#[derive(Clone)]
pub struct ProxyState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<ProxyConfig>>,
    pub status: Arc<RwLock<ProxyRuntimeStatus>>,
    pub start_time: Arc<RwLock<Option<std::time::Instant>>>,
    /// 每个应用类型当前使用的 provider/channel target。
    pub current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    /// 共享的 ProviderRouter（持有熔断器状态，跨请求保持）
    pub provider_router: Arc<ProviderRouter>,
    /// Host adapter surface for the neutral proxy core contracts.
    pub proxy_core_services: Arc<CcSwitchProxyServices<CcSwitchProxyRuntime>>,
    /// Gemini Native shadow state，用于 thoughtSignature / tool call 回放
    pub gemini_shadow: Arc<GeminiShadowStore>,
    /// Codex Chat bridge history，用于恢复 previous_response_id 指向的 tool call
    pub codex_chat_history: Arc<CodexChatHistoryStore>,
    /// 代理事件总线，供外部 SSE 监控和未来 ProxyEventSink 使用。
    pub events: Arc<ProxyEventBus>,
}
