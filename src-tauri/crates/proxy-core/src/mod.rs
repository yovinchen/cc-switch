//! Host-neutral proxy core contracts.
//!
//! This module is intentionally additive for the first extraction step. Runtime
//! traffic still uses the existing proxy module, while new code can start
//! depending on these neutral domain types and service ports.

pub mod api;
pub mod cache_injector;
pub mod claude_auth;
pub mod codex_chat_history;
pub mod codex_error;
pub mod claude_desktop_gateway_auth;
pub mod channel_identity;
pub mod channel_request;
pub mod circuit_breaker_config;
pub mod circuit_breaker_key;
pub mod copilot_model_map;
pub mod cost;
pub mod domain;
pub mod engine;
pub mod error;
pub mod event_payload;
pub mod forward_failure;
pub mod gemini_auth;
pub mod gemini_request;
pub mod gemini_response;
pub mod gemini_shadow;
pub mod gemini_schema;
pub mod gemini_stream;
pub mod gemini_tool_args;
pub mod gemini_url;
pub mod json_canonical;
pub mod legacy_projection;
pub mod log_codes;
pub mod management_api;
pub mod managed_account_auth;
pub mod management_auth;
pub mod model_fetch;
pub mod model_mapping;
pub mod openai_chat_stream;
pub mod openai_responses_stream;
pub mod ports;
pub mod provider_auth;
pub mod provider_selection;
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
pub mod secret;
pub mod session;
pub mod sse;
pub mod thinking_budget_rectifier;
pub mod thinking_rectifier;
pub mod thinking_optimizer;
pub mod usage;
pub mod usage_config;
