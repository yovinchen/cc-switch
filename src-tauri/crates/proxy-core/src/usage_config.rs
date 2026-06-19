use serde_json::Value;

use crate::sse::{
    claude_stream_usage_event_filter, codex_stream_usage_event_filter,
    gemini_stream_usage_event_filter, openai_stream_usage_event_filter,
};
use crate::usage::{
    claude_stream_model_extractor, codex_auto_stream_model_extractor,
    gemini_stream_model_extractor, openai_stream_model_extractor, TokenUsage,
};

/// Streaming response usage parser.
pub type StreamUsageParser = fn(&[Value]) -> Option<TokenUsage>;

/// Non-streaming response usage parser.
pub type ResponseUsageParser = fn(&Value) -> Option<TokenUsage>;

/// Extracts the effective model from collected streaming usage events.
pub type StreamModelExtractor = fn(&[Value], &str) -> String;

/// Hot-path prefilter for raw SSE `data:` payloads before JSON parsing.
pub type StreamUsageEventFilter = fn(&str) -> bool;

#[derive(Clone, Copy)]
pub struct UsageParserConfig {
    pub stream_parser: StreamUsageParser,
    pub response_parser: ResponseUsageParser,
    pub model_extractor: StreamModelExtractor,
    pub stream_event_filter: Option<StreamUsageEventFilter>,
    pub app_type_str: &'static str,
}

pub const CLAUDE_PARSER_CONFIG: UsageParserConfig = UsageParserConfig {
    stream_parser: TokenUsage::from_claude_stream_events,
    response_parser: TokenUsage::from_claude_response,
    model_extractor: claude_stream_model_extractor,
    stream_event_filter: Some(claude_stream_usage_event_filter),
    app_type_str: "claude",
};

pub const OPENAI_PARSER_CONFIG: UsageParserConfig = UsageParserConfig {
    stream_parser: TokenUsage::from_openai_stream_events,
    response_parser: TokenUsage::from_openai_response,
    model_extractor: openai_stream_model_extractor,
    stream_event_filter: Some(openai_stream_usage_event_filter),
    app_type_str: "codex",
};

pub const CODEX_PARSER_CONFIG: UsageParserConfig = UsageParserConfig {
    stream_parser: TokenUsage::from_codex_stream_events_auto,
    response_parser: TokenUsage::from_codex_response_auto,
    model_extractor: codex_auto_stream_model_extractor,
    stream_event_filter: Some(codex_stream_usage_event_filter),
    app_type_str: "codex",
};

pub const GEMINI_PARSER_CONFIG: UsageParserConfig = UsageParserConfig {
    stream_parser: TokenUsage::from_gemini_stream_chunks,
    response_parser: TokenUsage::from_gemini_response,
    model_extractor: gemini_stream_model_extractor,
    stream_event_filter: Some(gemini_stream_usage_event_filter),
    app_type_str: "gemini",
};

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CLAUDE_PARSER_CONFIG, CODEX_PARSER_CONFIG, GEMINI_PARSER_CONFIG};

    #[test]
    fn parser_configs_keep_protocol_labels_and_prefilters() {
        assert_eq!(CLAUDE_PARSER_CONFIG.app_type_str, "claude");
        assert_eq!(CODEX_PARSER_CONFIG.app_type_str, "codex");
        assert_eq!(GEMINI_PARSER_CONFIG.app_type_str, "gemini");

        assert!(CLAUDE_PARSER_CONFIG
            .stream_event_filter
            .is_some_and(|filter| filter(r#"{"type":"message_delta","usage":{}}"#)));
        assert!(CODEX_PARSER_CONFIG
            .stream_event_filter
            .is_some_and(|filter| filter(r#"{"type":"response.completed","response":{}}"#)));
        assert!(GEMINI_PARSER_CONFIG
            .stream_event_filter
            .is_some_and(|filter| filter(r#"{"usageMetadata":{}}"#)));
    }

    #[test]
    fn parser_configs_call_underlying_usage_parsers() {
        let openai_response = json!({
            "model": "gpt-4.1",
            "usage": {
                "prompt_tokens": 3,
                "completion_tokens": 5
            }
        });

        let usage = (super::OPENAI_PARSER_CONFIG.response_parser)(&openai_response)
            .expect("openai usage should parse");

        assert_eq!(usage.input_tokens, 3);
        assert_eq!(usage.output_tokens, 5);
        assert_eq!(usage.model.as_deref(), Some("gpt-4.1"));
    }
}
