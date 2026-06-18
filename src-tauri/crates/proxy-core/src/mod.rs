//! Host-neutral proxy core contracts.
//!
//! This module is intentionally additive for the first extraction step. Runtime
//! traffic still uses the existing proxy module, while new code can start
//! depending on these neutral domain types and service ports.

pub mod codex_error;
pub mod domain;
pub mod engine;
pub mod error;
pub mod ports;
pub mod response_body;
pub mod response_diagnostics;
pub mod response_headers;
pub mod response_timeout;
pub mod response_transform;
pub mod route_resolve;
pub mod sse;
pub mod usage;

pub use codex_error::*;
pub use domain::*;
pub use engine::*;
pub use error::*;
pub use ports::*;
pub use response_body::*;
pub use response_diagnostics::*;
pub use response_headers::*;
pub use response_timeout::*;
pub use response_transform::*;
pub use route_resolve::*;
pub use sse::*;
pub use usage::*;
