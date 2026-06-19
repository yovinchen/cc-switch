//! Stable JSON helpers for cache-sensitive request bodies.
//!
//! The implementation lives in proxy-core; this module keeps the legacy host
//! import path stable during the extraction.

pub(crate) use crate::proxy_core::{
    canonical_json_string, canonicalize_json_string_if_parseable, canonicalize_tool_arguments,
    canonicalize_tool_arguments_str, short_value_hash,
};
