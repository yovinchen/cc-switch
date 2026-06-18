use serde_json::Value;

const ONE_M_CONTEXT_MARKER: &str = "[1m]";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelMapping {
    pub haiku_model: Option<String>,
    pub sonnet_model: Option<String>,
    pub opus_model: Option<String>,
    pub fable_model: Option<String>,
    pub default_model: Option<String>,
}

impl ModelMapping {
    pub fn has_mapping(&self) -> bool {
        self.haiku_model.is_some()
            || self.sonnet_model.is_some()
            || self.opus_model.is_some()
            || self.fable_model.is_some()
            || self.default_model.is_some()
    }

    pub fn map_model(&self, original_model: &str) -> String {
        let model_lower = original_model.to_lowercase();

        if model_lower.contains("fable") {
            if let Some(model) = &self.fable_model {
                return model.clone();
            }
            if let Some(model) = &self.opus_model {
                return model.clone();
            }
        }
        if model_lower.contains("haiku") {
            if let Some(model) = &self.haiku_model {
                return model.clone();
            }
        }
        if model_lower.contains("opus") {
            if let Some(model) = &self.opus_model {
                return model.clone();
            }
        }
        if model_lower.contains("sonnet") {
            if let Some(model) = &self.sonnet_model {
                return model.clone();
            }
        }

        self.default_model
            .clone()
            .unwrap_or_else(|| original_model.to_string())
    }
}

pub fn apply_model_mapping_to_body(
    mut body: Value,
    mapping: &ModelMapping,
) -> (Value, Option<String>, Option<String>) {
    if !mapping.has_mapping() {
        let original = body
            .get("model")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        return (body, original, None);
    }

    let original_model = body
        .get("model")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    if let Some(original) = &original_model {
        let mapped = mapping.map_model(original);
        if mapped != *original {
            body["model"] = Value::String(mapped.clone());
            return (body, Some(original.clone()), Some(mapped));
        }
    }

    (body, original_model, None)
}

pub fn strip_one_m_suffix_for_upstream(model: &str) -> &str {
    let trimmed = model.trim_end();
    let marker = ONE_M_CONTEXT_MARKER.as_bytes();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= marker.len()
        && bytes[bytes.len() - marker.len()..].eq_ignore_ascii_case(marker)
    {
        return trimmed[..trimmed.len() - marker.len()].trim_end();
    }
    model
}

pub fn strip_one_m_suffix_for_upstream_from_body(mut body: Value) -> Value {
    let Some(model) = body.get("model").and_then(Value::as_str) else {
        return body;
    };

    let stripped = strip_one_m_suffix_for_upstream(model);
    if stripped != model {
        body["model"] = Value::String(stripped.to_string());
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mapping() -> ModelMapping {
        ModelMapping {
            haiku_model: Some("haiku-mapped".to_string()),
            sonnet_model: Some("sonnet-mapped".to_string()),
            opus_model: Some("opus-mapped".to_string()),
            fable_model: Some("fable-mapped".to_string()),
            default_model: Some("default-model".to_string()),
        }
    }

    #[test]
    fn maps_model_families_by_priority() {
        let mapping = mapping();

        assert_eq!(
            mapping.map_model("claude-sonnet-4-5-20250929"),
            "sonnet-mapped"
        );
        assert_eq!(mapping.map_model("claude-haiku-4-5"), "haiku-mapped");
        assert_eq!(mapping.map_model("claude-opus-4-5"), "opus-mapped");
        assert_eq!(mapping.map_model("claude-fable-5"), "fable-mapped");
        assert_eq!(mapping.map_model("some-unknown-model"), "default-model");
    }

    #[test]
    fn maps_fable_to_opus_when_fable_mapping_is_missing() {
        let mapping = ModelMapping {
            opus_model: Some("opus-mapped".to_string()),
            default_model: Some("default-model".to_string()),
            ..ModelMapping::default()
        };

        assert_eq!(mapping.map_model("claude-fable-5"), "opus-mapped");
    }

    #[test]
    fn preserves_original_without_mapping() {
        let mapping = ModelMapping::default();
        let body = json!({"model": "claude-sonnet-4-5"});

        let (result, original, mapped) = apply_model_mapping_to_body(body, &mapping);

        assert_eq!(result["model"], "claude-sonnet-4-5");
        assert_eq!(original.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(mapped, None);
    }

    #[test]
    fn applies_model_mapping_to_body() {
        let body = json!({"model": "Claude-SONNET-4-5"});

        let (result, original, mapped) = apply_model_mapping_to_body(body, &mapping());

        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(original.as_deref(), Some("Claude-SONNET-4-5"));
        assert_eq!(mapped.as_deref(), Some("sonnet-mapped"));
    }

    #[test]
    fn strips_one_m_suffix_before_upstream() {
        assert_eq!(
            strip_one_m_suffix_for_upstream("deepseek-v4-pro[1M]"),
            "deepseek-v4-pro"
        );
        assert_eq!(
            strip_one_m_suffix_for_upstream("deepseek-v4-pro [1M]"),
            "deepseek-v4-pro"
        );
        assert_eq!(
            strip_one_m_suffix_for_upstream("deepseek-v4-pro"),
            "deepseek-v4-pro"
        );
    }

    #[test]
    fn strips_one_m_suffix_in_body() {
        let body = json!({"model": "deepseek-v4-pro[1M]"});
        let result = strip_one_m_suffix_for_upstream_from_body(body);

        assert_eq!(result["model"], "deepseek-v4-pro");
    }
}
