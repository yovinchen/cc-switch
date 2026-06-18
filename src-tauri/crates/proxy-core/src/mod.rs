//! Host-neutral proxy core contracts.
//!
//! This module is intentionally additive for the first extraction step. Runtime
//! traffic still uses the existing proxy module, while new code can start
//! depending on these neutral domain types and service ports.

pub mod domain;
pub mod engine;
pub mod error;
pub mod ports;
pub mod response_headers;

pub use domain::*;
pub use engine::*;
pub use error::*;
pub use ports::*;
pub use response_headers::*;
