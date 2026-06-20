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
    pub fn from_settings_config(settings_config: &Value) -> Self {
        let env = settings_config.get("env");

        Self {
            haiku_model: env_string(env, "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
            sonnet_model: env_string(env, "ANTHROPIC_DEFAULT_SONNET_MODEL"),
            opus_model: env_string(env, "ANTHROPIC_DEFAULT_OPUS_MODEL"),
            fable_model: env_string(env, "ANTHROPIC_DEFAULT_FABLE_MODEL"),
            default_model: env_string(env, "ANTHROPIC_MODEL"),
        }
    }

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

fn env_string(env: Option<&Value>, key: &str) -> Option<String> {
    env.and_then(|env| env.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(String::from)
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

pub fn model_mapping_log_message(original: Option<&str>, mapped: Option<&str>) -> Option<String> {
    match (original, mapped) {
        (Some(original), Some(mapped)) => Some(format!(
            "[ModelMapper] 模型映射: {original} \u{2192} {mapped}"
        )),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelMappingProjection {
    pub body: Value,
    pub log_message: Option<String>,
}

pub fn apply_provider_model_mapping(
    body: Value,
    provider_settings: &Value,
) -> ModelMappingProjection {
    let mapping = ModelMapping::from_settings_config(provider_settings);
    let (body, original_model, mapped_model) = apply_model_mapping_to_body(body, &mapping);
    let log_message = model_mapping_log_message(original_model.as_deref(), mapped_model.as_deref());

    ModelMappingProjection { body, log_message }
}

pub fn strip_one_m_suffix_for_upstream(model: &str) -> &str {
    if !has_one_m_suffix_for_upstream(model) {
        return model;
    }

    let trimmed = model.trim_end();
    let marker = ONE_M_CONTEXT_MARKER.as_bytes();
    trimmed[..trimmed.len() - marker.len()].trim_end()
}

pub fn has_one_m_suffix_for_upstream(model: &str) -> bool {
    let trimmed = model.trim_end();
    let marker = ONE_M_CONTEXT_MARKER.as_bytes();
    let bytes = trimmed.as_bytes();
    bytes.len() >= marker.len() && bytes[bytes.len() - marker.len()..].eq_ignore_ascii_case(marker)
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
    fn builds_mapping_from_provider_settings_env() {
        let mapping = ModelMapping::from_settings_config(&json!({
            "env": {
                "ANTHROPIC_MODEL": "default-model",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "haiku-mapped",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-mapped",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-mapped",
                "ANTHROPIC_DEFAULT_FABLE_MODEL": "fable-mapped"
            }
        }));

        assert_eq!(mapping.haiku_model.as_deref(), Some("haiku-mapped"));
        assert_eq!(mapping.sonnet_model.as_deref(), Some("sonnet-mapped"));
        assert_eq!(mapping.opus_model.as_deref(), Some("opus-mapped"));
        assert_eq!(mapping.fable_model.as_deref(), Some("fable-mapped"));
        assert_eq!(mapping.default_model.as_deref(), Some("default-model"));
    }

    #[test]
    fn provider_settings_mapping_ignores_empty_env_values() {
        let mapping = ModelMapping::from_settings_config(&json!({
            "env": {
                "ANTHROPIC_MODEL": "",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "haiku-mapped"
            }
        }));

        assert_eq!(mapping.haiku_model.as_deref(), Some("haiku-mapped"));
        assert_eq!(mapping.default_model, None);
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
    fn maps_fable_to_default_when_specific_mappings_are_missing() {
        let mapping = ModelMapping {
            default_model: Some("default-model".to_string()),
            ..ModelMapping::default()
        };

        assert_eq!(mapping.map_model("claude-fable-5"), "default-model");
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
    fn body_mapping_ignores_thinking_fields() {
        let body = json!({
            "model": "claude-sonnet-4-5",
            "thinking": {"type": "adaptive"}
        });

        let (result, _original, mapped) = apply_model_mapping_to_body(body, &mapping());

        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(result["thinking"]["type"], "adaptive");
        assert_eq!(mapped.as_deref(), Some("sonnet-mapped"));
    }

    #[test]
    fn log_message_matches_mapping_result() {
        assert_eq!(
            model_mapping_log_message(Some("claude-sonnet"), Some("sonnet-mapped")).as_deref(),
            Some("[ModelMapper] \u{6A21}\u{578B}\u{6620}\u{5C04}: claude-sonnet \u{2192} sonnet-mapped")
        );
        assert!(model_mapping_log_message(Some("claude-sonnet"), None).is_none());
    }

    #[test]
    fn provider_model_mapping_projects_body_and_log_message() {
        let projection = apply_provider_model_mapping(
            json!({"model": "claude-sonnet", "messages": []}),
            &json!({
                "env": {
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-mapped"
                }
            }),
        );

        assert_eq!(
            projection.body.get("model").and_then(Value::as_str),
            Some("sonnet-mapped")
        );
        assert_eq!(
            projection.log_message.as_deref(),
            Some("[ModelMapper] \u{6A21}\u{578B}\u{6620}\u{5C04}: claude-sonnet \u{2192} sonnet-mapped")
        );

        let unchanged = apply_provider_model_mapping(json!({"model": "unknown"}), &json!({}));
        assert_eq!(
            unchanged.body.get("model").and_then(Value::as_str),
            Some("unknown")
        );
        assert!(unchanged.log_message.is_none());
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
    fn detects_one_m_suffix_before_upstream() {
        assert!(has_one_m_suffix_for_upstream("deepseek-v4-pro[1M]"));
        assert!(has_one_m_suffix_for_upstream("deepseek-v4-pro [1m]  "));
        assert!(!has_one_m_suffix_for_upstream("deepseek-v4-pro"));
    }

    #[test]
    fn strips_one_m_suffix_in_body() {
        let body = json!({"model": "deepseek-v4-pro[1M]"});
        let result = strip_one_m_suffix_for_upstream_from_body(body);

        assert_eq!(result["model"], "deepseek-v4-pro");
    }
}
