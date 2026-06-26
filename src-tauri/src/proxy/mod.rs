//! 代理服务器模块
//!
//! 提供本地HTTP代理服务，支持多Provider故障转移和请求透传

pub(crate) mod auth_adapter;
pub mod circuit_breaker;
pub(crate) mod codex_chat_history;
pub(crate) mod codex_oauth_auth;
pub(crate) mod copilot_auth;
pub(crate) mod engine;
pub mod error;
pub mod error_mapper;
pub(crate) mod events;
pub mod handler_context;
pub(crate) mod host;
pub mod provider;
pub(crate) mod response_adapter;
pub(crate) mod route_attempt;
pub(crate) mod switch_lock;
pub(crate) mod transport;

pub(crate) use engine::forward_pipeline::{ForwardError, ForwardResult, RequestForwarder};
