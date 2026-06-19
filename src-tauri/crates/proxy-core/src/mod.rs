//! Host-neutral proxy core contracts.
//!
//! This module is intentionally additive for the first extraction step. Runtime
//! traffic still uses the existing proxy module, while new code can start
//! depending on these neutral domain types and service ports.

pub mod cache_injector;
pub mod codex_error;
pub mod claude_desktop_gateway_auth;
pub mod channel_identity;
pub mod channel_request;
pub mod copilot_model_map;
pub mod domain;
pub mod engine;
pub mod error;
pub mod event_payload;
pub mod forward_failure;
pub mod gemini_schema;
pub mod gemini_url;
pub mod json_canonical;
pub mod legacy_projection;
pub mod management_api;
pub mod managed_account_auth;
pub mod management_auth;
pub mod model_mapping;
pub mod ports;
pub mod response_body;
pub mod response_build;
pub mod response_diagnostics;
pub mod response_headers;
pub mod response_parse;
pub mod request_body;
pub mod request_headers;
pub mod request_media;
pub mod request_optimizer;
pub mod request_transport;
pub mod request_url;
pub mod response_timeout;
pub mod response_transform;
pub mod route_resolve;
pub mod session;
pub mod sse;
pub mod thinking_budget_rectifier;
pub mod thinking_rectifier;
pub mod thinking_optimizer;
pub mod usage;

pub use cache_injector::*;
pub use codex_error::*;
pub use claude_desktop_gateway_auth::*;
pub use channel_identity::*;
pub use channel_request::*;
pub use copilot_model_map::*;
pub use domain::*;
pub use engine::*;
pub use error::*;
pub use event_payload::*;
pub use forward_failure::*;
pub use gemini_schema::*;
pub use gemini_url::*;
pub use json_canonical::*;
pub use legacy_projection::*;
pub use management_api::*;
pub use managed_account_auth::*;
pub use management_auth::*;
pub use model_mapping::*;
pub use ports::*;
pub use response_body::*;
pub use response_build::*;
pub use response_diagnostics::*;
pub use response_headers::*;
pub use response_parse::*;
pub use request_body::*;
pub use request_headers::*;
pub use request_media::*;
pub use request_optimizer::*;
pub use request_transport::*;
pub use request_url::*;
pub use response_timeout::*;
pub use response_transform::*;
pub use route_resolve::*;
pub use session::{
    extract_session_id_with_generator, ClientFormat, SessionIdResult, SessionIdSource,
};
pub use sse::*;
pub use thinking_budget_rectifier::*;
pub use thinking_rectifier::*;
pub use thinking_optimizer::*;
pub use usage::*;
