//! 代理服务器模块
//!
//! 提供本地HTTP代理服务，支持多Provider故障转移和请求透传

pub(crate) mod auth_adapter;
pub mod circuit_breaker;
pub(crate) mod codex_chat_history;
pub mod error;
pub mod error_mapper;
pub(crate) mod events;
pub(crate) mod failover_switch;
mod forwarder;
pub mod handler_context;
mod handlers;
pub mod http_client;
pub mod hyper_client;
pub(crate) mod managed_account_auth;
pub mod provider_router;
pub mod providers;
pub(crate) mod response_adapter;
pub mod response_processor;
pub(crate) mod route_attempt;
pub(crate) mod server;
pub(crate) mod switch_lock;
pub mod usage;
pub(crate) mod usage_sink_bridge;

pub(crate) use forwarder::{ForwardError, ForwardResult, RequestForwarder};
