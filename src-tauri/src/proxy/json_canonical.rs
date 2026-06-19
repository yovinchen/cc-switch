//! Stable JSON helpers for cache-sensitive request bodies.
//!
//! The implementation lives in proxy-core; this module keeps the legacy host
//! import path stable during the extraction.

pub(crate) use crate::proxy_core::{canonical_json_string, short_value_hash};
