use super::copilot_auth::CopilotModel;
use serde_json::Value;

pub fn apply_copilot_model_normalization(body: Value) -> Value {
    let original = body
        .get("model")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let normalized_body = crate::proxy_core::apply_copilot_model_normalization(body);

    if let (Some(original), Some(normalized)) = (
        original.as_deref(),
        normalized_body
            .get("model")
            .and_then(|value| value.as_str()),
    ) {
        if original != normalized {
            log::debug!("[CopilotNormalizer] {original} -> {normalized}");
        }
    }

    normalized_body
}

pub fn resolve_against_models(client_id: &str, models: &[CopilotModel]) -> Option<String> {
    crate::proxy_core::resolve_copilot_model_against_ids(
        client_id,
        models.iter().map(|model| model.id.as_str()),
    )
}
