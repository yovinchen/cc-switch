//! 代理服务器模块
//!
//! 提供本地HTTP代理服务，支持多Provider故障转移和请求透传

pub mod circuit_breaker;
pub mod error;
pub mod error_mapper;
pub(crate) mod events;
pub(crate) mod failover_switch;
mod forwarder;
pub mod handler_context;
mod handlers;
pub mod http_client;
pub mod hyper_client;
pub mod provider_router;
pub mod providers;
pub(crate) mod response_adapter;
pub mod response_processor;
pub(crate) mod route_attempt;
pub(crate) mod server;
pub mod session;
pub(crate) mod switch_lock;
pub mod usage;
pub(crate) mod usage_sink_bridge;

// 公开导出给外部使用（commands, services等模块需要）
#[allow(unused_imports)]
pub use crate::proxy_core::ProxyRuntimeStatus as ProxyStatus;
#[allow(unused_imports)]
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerStats, CircuitState};
#[allow(unused_imports)]
pub use error::ProxyError;
pub(crate) use forwarder::{ForwardError, ForwardResult, RequestForwarder};
#[allow(unused_imports)]
pub use provider_router::ProviderRouter;
#[allow(unused_imports)]
pub use session::extract_session_id;
