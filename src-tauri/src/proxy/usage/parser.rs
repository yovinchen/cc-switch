//! Compatibility re-export for usage parsing.
//!
//! The parser implementation lives in `cc-switch-proxy-core`; this module keeps
//! the legacy host import path stable while response pipeline migration
//! continues.

pub use crate::proxy_core::{ApiType, TokenUsage, SESSION_REQUEST_ID_PREFIX};
