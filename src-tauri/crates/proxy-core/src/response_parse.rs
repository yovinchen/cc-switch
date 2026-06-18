use crate::{
    aggregate_fallback_diagnostics_message, body_looks_like_sse, chat_sse_to_response_value,
    responses_sse_to_response_value, upstream_body_parse_error_message, ProxyCoreError,
    ProxyCoreResult,
};
use http::HeaderMap;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamSseAggregationKind {
    ChatCompletions,
    Responses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamJsonBodySource {
    Json,
    UnlabeledSse {
        aggregation: UpstreamSseAggregationKind,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamJsonBody {
    pub value: Value,
    pub source: UpstreamJsonBodySource,
}

impl UpstreamJsonBody {
    pub fn json(value: Value) -> Self {
        Self {
            value,
            source: UpstreamJsonBodySource::Json,
        }
    }

    pub fn unlabeled_sse(value: Value, aggregation: UpstreamSseAggregationKind) -> Self {
        Self {
            value,
            source: UpstreamJsonBodySource::UnlabeledSse { aggregation },
        }
    }
}

/// Parse an upstream non-streaming response body as JSON, with an optional
/// fallback for gateways that return Server-Sent Events despite non-streaming
/// client semantics.
pub fn parse_upstream_json_or_unlabeled_sse<F>(
    body: &[u8],
    headers: &HeaderMap,
    parse_error_prefix: &str,
    unlabeled_sse_aggregation: Option<UpstreamSseAggregationKind>,
    mut next_missing_chat_id: F,
) -> ProxyCoreResult<UpstreamJsonBody>
where
    F: FnMut() -> String,
{
    match serde_json::from_slice::<Value>(body) {
        Ok(value) => return Ok(UpstreamJsonBody::json(value)),
        Err(parse_error) => {
            let body_str = String::from_utf8_lossy(body);
            if body_looks_like_sse(&body_str) {
                if let Some(aggregation) = unlabeled_sse_aggregation {
                    let value = match aggregation {
                        UpstreamSseAggregationKind::ChatCompletions => {
                            chat_sse_to_response_value(&body_str, &mut next_missing_chat_id)
                        }
                        UpstreamSseAggregationKind::Responses => {
                            responses_sse_to_response_value(&body_str)
                        }
                    }
                    .map_err(|error| {
                        ProxyCoreError::Upstream(aggregate_fallback_diagnostics_message(
                            &core_error_message(error),
                            headers,
                            &body_str,
                        ))
                    })?;

                    return Ok(UpstreamJsonBody::unlabeled_sse(value, aggregation));
                }
            }

            Err(ProxyCoreError::Upstream(upstream_body_parse_error_message(
                parse_error_prefix,
                parse_error,
                headers,
                &body_str,
            )))
        }
    }
}

fn core_error_message(error: ProxyCoreError) -> String {
    match error {
        ProxyCoreError::Upstream(message) => message,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;

    fn generated_id() -> String {
        "generated-id".to_string()
    }

    #[test]
    fn parses_regular_json_body() {
        let parsed = parse_upstream_json_or_unlabeled_sse(
            br#"{"id":"ok"}"#,
            &HeaderMap::new(),
            "parse failed",
            Some(UpstreamSseAggregationKind::ChatCompletions),
            generated_id,
        )
        .unwrap();

        assert_eq!(parsed.source, UpstreamJsonBodySource::Json);
        assert_eq!(parsed.value["id"], "ok");
    }

    #[test]
    fn aggregates_unlabeled_chat_sse_when_enabled() {
        let sse = "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n";

        let parsed = parse_upstream_json_or_unlabeled_sse(
            sse.as_bytes(),
            &HeaderMap::new(),
            "parse failed",
            Some(UpstreamSseAggregationKind::ChatCompletions),
            generated_id,
        )
        .unwrap();

        assert_eq!(
            parsed.source,
            UpstreamJsonBodySource::UnlabeledSse {
                aggregation: UpstreamSseAggregationKind::ChatCompletions,
            }
        );
        assert_eq!(parsed.value["choices"][0]["message"]["content"], "hi");
    }

    #[test]
    fn aggregates_unlabeled_responses_sse_when_enabled() {
        let sse = "event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"model\":\"m\",\"output\":[]}}\n\n";

        let parsed = parse_upstream_json_or_unlabeled_sse(
            sse.as_bytes(),
            &HeaderMap::new(),
            "parse failed",
            Some(UpstreamSseAggregationKind::Responses),
            generated_id,
        )
        .unwrap();

        assert_eq!(
            parsed.source,
            UpstreamJsonBodySource::UnlabeledSse {
                aggregation: UpstreamSseAggregationKind::Responses,
            }
        );
        assert_eq!(parsed.value["id"], "resp_1");
    }

    #[test]
    fn returns_parse_diagnostics_when_unlabeled_sse_aggregation_is_disabled() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "text/event-stream".parse().unwrap());

        let error = parse_upstream_json_or_unlabeled_sse(
            b"data: {}\n\n",
            &headers,
            "parse failed",
            None,
            generated_id,
        )
        .unwrap_err();

        let message = core_error_message(error);
        assert!(message.contains("parse failed"), "{message}");
        assert!(message.contains("content-type: text/event-stream"), "{message}");
        assert!(message.contains("body[..120]: 'data: {}\\n\\n'"), "{message}");
    }

    #[test]
    fn returns_aggregate_diagnostics_when_unlabeled_sse_aggregation_fails() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());

        let error = parse_upstream_json_or_unlabeled_sse(
            b": keepalive\n\ndata: [DONE]\n\n",
            &headers,
            "parse failed",
            Some(UpstreamSseAggregationKind::ChatCompletions),
            generated_id,
        )
        .unwrap_err();

        let message = core_error_message(error);
        assert!(message.contains("No chat completion choices"), "{message}");
        assert!(message.contains("content-type: application/json"), "{message}");
        assert!(
            message.contains("body[..120]: ': keepalive\\n\\ndata: [DONE]\\n\\n'"),
            "{message}"
        );
    }
}
