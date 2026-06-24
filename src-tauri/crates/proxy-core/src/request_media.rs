use serde_json::{json, Value};

pub const UNSUPPORTED_IMAGE_MARKER: &str = "[Unsupported Image]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaPreventionPolicy {
    pub should_attempt: bool,
    pub allow_heuristic: bool,
}

pub struct ForwarderMediaPreventionFacts<'a> {
    pub rectifier_enabled: bool,
    pub request_media_fallback: bool,
    pub request_media_heuristic: bool,
    pub body: &'a mut Value,
    pub provider_settings: &'a Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaRetryInput<'a> {
    pub adapter_name: &'a str,
    pub rectifier_enabled: bool,
    pub request_media_fallback: bool,
    pub already_retried: bool,
    pub body_has_images: bool,
    pub unsupported_image_error: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForwarderMediaRetryPlanFacts<'a> {
    pub adapter_name: &'a str,
    pub rectifier_enabled: bool,
    pub request_media_fallback: bool,
    pub already_retried: bool,
    pub provider_body: &'a Value,
    pub unsupported_image_error: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForwarderMediaRetryPlanProjection {
    pub body: Value,
    pub replaced_images: usize,
}

pub fn resolve_media_prevention_policy(
    rectifier_enabled: bool,
    request_media_fallback: bool,
    request_media_heuristic: bool,
) -> MediaPreventionPolicy {
    let should_attempt = rectifier_enabled && request_media_fallback;
    MediaPreventionPolicy {
        should_attempt,
        allow_heuristic: should_attempt && request_media_heuristic,
    }
}

pub fn apply_forwarder_media_prevention_from_facts(
    input: ForwarderMediaPreventionFacts<'_>,
) -> usize {
    let policy = resolve_media_prevention_policy(
        input.rectifier_enabled,
        input.request_media_fallback,
        input.request_media_heuristic,
    );
    if !policy.should_attempt {
        return 0;
    }

    replace_images_for_text_only_model(input.body, input.provider_settings, policy.allow_heuristic)
}

pub fn should_check_media_retry(
    adapter_name: &str,
    rectifier_enabled: bool,
    request_media_fallback: bool,
    already_retried: bool,
) -> bool {
    matches!(adapter_name, "Claude" | "Codex")
        && rectifier_enabled
        && request_media_fallback
        && !already_retried
}

pub fn should_trigger_media_retry(input: MediaRetryInput<'_>) -> bool {
    should_check_media_retry(
        input.adapter_name,
        input.rectifier_enabled,
        input.request_media_fallback,
        input.already_retried,
    ) && input.body_has_images
        && input.unsupported_image_error
}

pub fn forwarder_media_retry_plan_from_facts(
    input: ForwarderMediaRetryPlanFacts<'_>,
) -> Option<ForwarderMediaRetryPlanProjection> {
    if !should_trigger_media_retry(MediaRetryInput {
        adapter_name: input.adapter_name,
        rectifier_enabled: input.rectifier_enabled,
        request_media_fallback: input.request_media_fallback,
        already_retried: input.already_retried,
        body_has_images: contains_image_blocks(input.provider_body),
        unsupported_image_error: input.unsupported_image_error,
    }) {
        return None;
    }

    let mut body = input.provider_body.clone();
    let replaced_images = replace_image_blocks_with_marker(&mut body);
    (replaced_images > 0).then_some(ForwarderMediaRetryPlanProjection {
        body,
        replaced_images,
    })
}

/// Replace image blocks before sending when the routed model is text-only.
///
/// Two paths, both reached only when the caller's media-fallback switch is on:
/// - explicit capability from provider/channel model catalog settings is trusted;
/// - the curated `known_text_only_model` list is a heuristic prediction and only
///   runs when `allow_heuristic` is true.
pub fn replace_images_for_text_only_model(
    body: &mut Value,
    provider_settings: &Value,
    allow_heuristic: bool,
) -> usize {
    if !contains_image_blocks(body) {
        return 0;
    }

    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");

    match explicit_model_image_support(provider_settings, model) {
        Some(true) => return 0,
        Some(false) => return replace_images_in_body(body),
        None => {}
    }

    if !allow_heuristic || !known_text_only_model(model) {
        return 0;
    }

    replace_images_in_body(body)
}

pub fn contains_image_blocks(body: &Value) -> bool {
    messages_have_image_blocks(body) || responses_input_has_image_blocks(body.get("input"))
}

pub fn replace_image_blocks_with_marker(body: &mut Value) -> usize {
    replace_images_in_body(body)
}

pub fn is_unsupported_image_error(status: u16, body: Option<&str>) -> bool {
    if !matches!(status, 400 | 415 | 422 | 501) {
        return false;
    }

    let Some(body) = body else {
        return false;
    };

    let message = extract_error_text(body);
    let message = message.to_ascii_lowercase();
    let mentions_image = message.contains("image")
        || message.contains("vision")
        || message.contains("multimodal")
        || message.contains("multi-modal")
        || message.contains("modality")
        || message.contains("modalities")
        || message.contains("media")
        || message.contains("attachment");

    if !mentions_image {
        return false;
    }

    const UNSUPPORTED_HINTS: &[&str] = &[
        "unsupported",
        "not supported",
        "does not support",
        "doesn't support",
        "do not support",
        "don't support",
        "only supports text",
        "text only",
        "text-only",
        "invalid content type",
        "invalid message content",
        "unknown variant",
        "unknown content type",
        "unrecognized content type",
        "cannot process",
        "cannot handle",
        "can't process",
        "can't handle",
        "unable to process",
    ];

    UNSUPPORTED_HINTS.iter().any(|hint| message.contains(hint))
}

fn content_has_image_blocks(content: &Value) -> bool {
    let Some(blocks) = content.as_array() else {
        return false;
    };

    blocks.iter().any(|block| {
        is_image_block_type(block.get("type").and_then(Value::as_str))
            || block.get("content").is_some_and(content_has_image_blocks)
    })
}

fn replace_images_in_body(body: &mut Value) -> usize {
    let message_replacements = body
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .map(|messages| {
            messages
                .iter_mut()
                .filter_map(|message| message.get_mut("content"))
                .map(replace_images_in_content)
                .sum()
        })
        .unwrap_or(0);

    message_replacements
        + body
            .get_mut("input")
            .map(replace_images_in_responses_input)
            .unwrap_or(0)
}

fn replace_images_in_content(content: &mut Value) -> usize {
    replace_images_in_content_with_text_type(content, "text")
}

fn replace_images_in_content_with_text_type(content: &mut Value, text_type: &str) -> usize {
    let Some(blocks) = content.as_array_mut() else {
        return 0;
    };

    let mut replaced = 0usize;
    for block in blocks {
        if is_image_block_type(block.get("type").and_then(Value::as_str)) {
            replace_image_block_with_text_marker(block, text_type);
            replaced += 1;
            continue;
        }

        if let Some(nested_content) = block.get_mut("content") {
            replaced += replace_images_in_content_with_text_type(nested_content, text_type);
        }
    }

    replaced
}

fn messages_have_image_blocks(body: &Value) -> bool {
    body.get("messages")
        .and_then(Value::as_array)
        .is_some_and(|messages| {
            messages
                .iter()
                .filter_map(|message| message.get("content"))
                .any(content_has_image_blocks)
        })
}

fn responses_input_has_image_blocks(input: Option<&Value>) -> bool {
    match input {
        Some(Value::Array(items)) => items.iter().any(responses_input_item_has_image_blocks),
        Some(item @ Value::Object(_)) => responses_input_item_has_image_blocks(item),
        _ => false,
    }
}

fn responses_input_item_has_image_blocks(item: &Value) -> bool {
    if item.get("type").and_then(Value::as_str) == Some("input_image") {
        return true;
    }

    item.get("content").is_some_and(content_has_image_blocks)
}

fn replace_images_in_responses_input(input: &mut Value) -> usize {
    match input {
        Value::Array(items) => items
            .iter_mut()
            .map(replace_images_in_responses_input_item)
            .sum(),
        Value::Object(_) => replace_images_in_responses_input_item(input),
        _ => 0,
    }
}

fn replace_images_in_responses_input_item(item: &mut Value) -> usize {
    let mut replaced = 0usize;

    if item.get("type").and_then(Value::as_str) == Some("input_image") {
        replace_image_block_with_text_marker(item, "input_text");
        replaced += 1;
    }

    if let Some(content) = item.get_mut("content") {
        replaced += replace_images_in_content_with_text_type(content, "input_text");
    }

    replaced
}

fn is_image_block_type(block_type: Option<&str>) -> bool {
    matches!(block_type, Some("image" | "image_url" | "input_image"))
}

fn replace_image_block_with_text_marker(block: &mut Value, text_type: &str) {
    let cache_control = block.get("cache_control").cloned();
    *block = json!({
        "type": text_type,
        "text": UNSUPPORTED_IMAGE_MARKER
    });
    if let (Some(cache_control), Some(object)) = (cache_control, block.as_object_mut()) {
        object.insert("cache_control".to_string(), cache_control);
    }
}

fn explicit_model_image_support(provider_settings: &Value, model: &str) -> Option<bool> {
    [
        provider_settings
            .get("modelCatalog")
            .and_then(|catalog| catalog.get("models")),
        provider_settings.get("modelCatalog"),
        provider_settings.get("models"),
    ]
    .into_iter()
    .flatten()
    .find_map(|value| explicit_model_image_support_in_value(value, model))
}

fn known_text_only_model(model: &str) -> bool {
    let normalized = normalize_model_id(model);
    let tail = normalized.rsplit('/').next().unwrap_or(normalized.as_str());

    const EXACT_TAILS: &[&str] = &[
        "ark-code-latest",
        "deepseek-chat",
        "deepseek-reasoner",
        "deepseek-v4-flash",
        "deepseek-v4-pro",
        "glm-5.1",
        "kat-coder",
        "kat-coder-pro",
        "kat-coder-pro v1",
        "kat-coder-pro v2",
        "kat-coder-pro-v1",
        "kat-coder-pro-v2",
        "ling-2.5-1t",
        "longcat-flash-chat",
        "mimo-v2.5-pro",
        "us.deepseek.r1-v1",
    ];

    const TAIL_PREFIXES: &[&str] = &["minimax-m2.7", "qwen3-coder", "step-3.5-flash"];

    EXACT_TAILS.contains(&tail) || TAIL_PREFIXES.iter().any(|prefix| tail.starts_with(prefix))
}

fn explicit_model_image_support_in_value(value: &Value, model: &str) -> Option<bool> {
    if let Some(models) = value.as_array() {
        return models.iter().find_map(|entry| {
            model_entry_matches(entry, None, model).then(|| explicit_image_support(entry))?
        });
    }

    let object = value.as_object()?;
    object.iter().find_map(|(key, entry)| {
        model_entry_matches(entry, Some(key), model).then(|| explicit_image_support(entry))?
    })
}

fn explicit_image_support(entry: &Value) -> Option<bool> {
    if let Some(value) = entry
        .get("supportsImage")
        .or_else(|| entry.get("supports_image"))
        .or_else(|| entry.get("vision"))
        .and_then(Value::as_bool)
    {
        return Some(value);
    }

    [
        entry.get("input"),
        entry.pointer("/modalities/input"),
        entry.get("input_modalities"),
        entry.get("inputModalities"),
    ]
    .into_iter()
    .flatten()
    .find_map(input_modalities_support_image)
}

fn input_modalities_support_image(value: &Value) -> Option<bool> {
    let modalities = value.as_array()?;
    Some(modalities.iter().any(|item| {
        item.as_str()
            .map(str::trim)
            .is_some_and(|item| item.eq_ignore_ascii_case("image"))
    }))
}

fn extract_error_text(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        let candidates = [
            value.pointer("/error/message"),
            value.pointer("/message"),
            value.pointer("/detail"),
            value.pointer("/error"),
        ];
        if let Some(message) = candidates
            .into_iter()
            .flatten()
            .find_map(|value| value.as_str())
        {
            return message.to_string();
        }

        if let Ok(compact) = serde_json::to_string(&value) {
            return compact;
        }
    }

    body.to_string()
}

fn model_entry_matches(entry: &Value, key: Option<&str>, model: &str) -> bool {
    key.is_some_and(|key| model_ids_match(key, model))
        || ["model", "id", "name"]
            .into_iter()
            .filter_map(|field| entry.get(field).and_then(Value::as_str))
            .any(|candidate| model_ids_match(candidate, model))
}

fn model_ids_match(candidate: &str, model: &str) -> bool {
    let candidate = normalize_model_id(candidate);
    let model = normalize_model_id(model);
    if candidate.is_empty() || model.is_empty() {
        return false;
    }
    if candidate == model {
        return true;
    }

    let candidate_tail = candidate.rsplit('/').next().unwrap_or(candidate.as_str());
    let model_tail = model.rsplit('/').next().unwrap_or(model.as_str());
    candidate_tail == model_tail || candidate == model_tail || candidate_tail == model
}

fn normalize_model_id(value: &str) -> String {
    let model = value.trim().trim_start_matches("models/").trim();
    crate::model_mapping::strip_one_m_suffix_for_upstream(model)
        .trim()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        apply_forwarder_media_prevention_from_facts, contains_image_blocks,
        forwarder_media_retry_plan_from_facts, is_unsupported_image_error,
        replace_image_blocks_with_marker, replace_images_for_text_only_model,
        resolve_media_prevention_policy, should_check_media_retry, should_trigger_media_retry,
        ForwarderMediaPreventionFacts, ForwarderMediaRetryPlanFacts, MediaRetryInput,
        UNSUPPORTED_IMAGE_MARKER,
    };
    use serde_json::json;

    #[test]
    fn media_prevention_requires_master_and_fallback_switches() {
        assert_eq!(
            resolve_media_prevention_policy(true, true, true),
            super::MediaPreventionPolicy {
                should_attempt: true,
                allow_heuristic: true,
            }
        );
        assert_eq!(
            resolve_media_prevention_policy(true, true, false),
            super::MediaPreventionPolicy {
                should_attempt: true,
                allow_heuristic: false,
            }
        );
        assert_eq!(
            resolve_media_prevention_policy(false, true, true),
            super::MediaPreventionPolicy {
                should_attempt: false,
                allow_heuristic: false,
            }
        );
        assert_eq!(
            resolve_media_prevention_policy(true, false, true),
            super::MediaPreventionPolicy {
                should_attempt: false,
                allow_heuristic: false,
            }
        );
    }

    #[test]
    fn forwarder_media_prevention_applies_policy_and_provider_settings() {
        let settings = json!({
            "models": [
                { "id": "deepseek-v4-pro", "input": ["text"] }
            ]
        });
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let replaced = apply_forwarder_media_prevention_from_facts(
            ForwarderMediaPreventionFacts {
                rectifier_enabled: true,
                request_media_fallback: true,
                request_media_heuristic: false,
                body: &mut body,
                provider_settings: &settings,
            },
        );
        assert_eq!(replaced, 1);
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][0]["content"][0]["text"], UNSUPPORTED_IMAGE_MARKER);

        let mut disabled_body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        let disabled_before = disabled_body.clone();
        let disabled_replaced = apply_forwarder_media_prevention_from_facts(
            ForwarderMediaPreventionFacts {
                rectifier_enabled: true,
                request_media_fallback: false,
                request_media_heuristic: true,
                body: &mut disabled_body,
                provider_settings: &settings,
            },
        );
        assert_eq!(disabled_replaced, 0);
        assert_eq!(disabled_body, disabled_before);
    }

    #[test]
    fn media_retry_base_gate_requires_supported_adapter_and_switches() {
        assert!(should_check_media_retry("Claude", true, true, false));
        assert!(should_check_media_retry("Codex", true, true, false));
        assert!(!should_check_media_retry("Gemini", true, true, false));
        assert!(!should_check_media_retry("Claude", false, true, false));
        assert!(!should_check_media_retry("Claude", true, false, false));
        assert!(!should_check_media_retry("Claude", true, true, true));
    }

    #[test]
    fn media_retry_requires_images_and_unsupported_image_error() {
        let base = MediaRetryInput {
            adapter_name: "Claude",
            rectifier_enabled: true,
            request_media_fallback: true,
            already_retried: false,
            body_has_images: true,
            unsupported_image_error: true,
        };

        assert!(should_trigger_media_retry(base));
        assert!(!should_trigger_media_retry(MediaRetryInput {
            body_has_images: false,
            ..base
        }));
        assert!(!should_trigger_media_retry(MediaRetryInput {
            unsupported_image_error: false,
            ..base
        }));
        assert!(!should_trigger_media_retry(MediaRetryInput {
            already_retried: true,
            ..base
        }));
    }

    #[test]
    fn forwarder_media_retry_plan_replaces_images_when_retry_facts_trigger() {
        let body = json!({
            "model": "vision-rejecting-model",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "describe" },
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let plan = forwarder_media_retry_plan_from_facts(ForwarderMediaRetryPlanFacts {
            adapter_name: "Claude",
            rectifier_enabled: true,
            request_media_fallback: true,
            already_retried: false,
            provider_body: &body,
            unsupported_image_error: true,
        })
        .expect("media retry plan");

        assert_eq!(plan.replaced_images, 1);
        assert_eq!(
            plan.body["messages"][0]["content"][1]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
        assert_eq!(body["messages"][0]["content"][1]["type"], "image");

        assert!(forwarder_media_retry_plan_from_facts(ForwarderMediaRetryPlanFacts {
            adapter_name: "Gemini",
            rectifier_enabled: true,
            request_media_fallback: true,
            already_retried: false,
            provider_body: &body,
            unsupported_image_error: true,
        })
        .is_none());
        assert!(forwarder_media_retry_plan_from_facts(ForwarderMediaRetryPlanFacts {
            adapter_name: "Claude",
            rectifier_enabled: true,
            request_media_fallback: true,
            already_retried: false,
            provider_body: &body,
            unsupported_image_error: false,
        })
        .is_none());
    }

    #[test]
    fn explicit_text_modalities_replace_images_even_without_heuristics() {
        let settings = json!({
            "models": [
                { "id": "deepseek-v4-pro", "input": ["text"] }
            ]
        });
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &settings, false);

        assert_eq!(count, 1);
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(
            body["messages"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn heuristic_can_be_disabled_for_curated_text_only_models() {
        let mut body = json!({
            "model": "deepseek/deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &json!({}), false);

        assert_eq!(count, 0);
        assert_eq!(body["messages"][0]["content"][0]["type"], "image");
    }

    #[test]
    fn unknown_models_keep_images_without_explicit_capability() {
        let mut body = json!({
            "model": "unknown-model",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &json!({}), true);

        assert_eq!(count, 0);
        assert_eq!(body["messages"][0]["content"][0]["type"], "image");
    }

    #[test]
    fn curated_text_only_models_replace_chat_image_url_blocks() {
        let mut body = json!({
            "model": "deepseek-v4-flash",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "look" },
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &json!({}), true);

        assert_eq!(count, 1);
        assert_eq!(body["messages"][0]["content"][1]["type"], "text");
        assert_eq!(
            body["messages"][0]["content"][1]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn curated_text_only_models_replace_responses_input_images() {
        let mut body = json!({
            "model": "deepseek-v4-flash",
            "input": [{
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "look" },
                    { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &json!({}), true);

        assert_eq!(count, 1);
        assert_eq!(body["input"][0]["content"][1]["type"], "input_text");
        assert_eq!(
            body["input"][0]["content"][1]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn explicit_capabilities_can_override_curated_model_assumptions() {
        let settings = json!({
            "modelCatalog": {
                "models": [
                    { "model": "deepseek-v4-pro", "modalities": { "input": ["text", "image"] } }
                ]
            }
        });
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &settings, true);

        assert_eq!(count, 0);
        assert_eq!(body["messages"][0]["content"][0]["type"], "image");
    }

    #[test]
    fn explicit_text_capability_can_override_visual_model_ids() {
        let settings = json!({
            "models": [
                { "id": "gpt-4o", "input": ["text"] }
            ]
        });
        let mut body = json!({
            "model": "gpt-4o",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        let count = replace_images_for_text_only_model(&mut body, &settings, true);

        assert_eq!(count, 1);
        assert_eq!(
            body["messages"][0]["content"][0]["text"],
            UNSUPPORTED_IMAGE_MARKER
        );
    }

    #[test]
    fn curated_model_list_distinguishes_known_text_only_and_multimodal_ids() {
        let mut mimo_pro = json!({
            "model": "xiaomi-mimo-token-plan/mimo-v2.5-pro",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        let mut mimo_multimodal = json!({
            "model": "xiaomi-mimo-token-plan/mimo-v2.5",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        let mut kimi_multimodal = json!({
            "model": "kimi/kimi-k2.6",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });
        let mut qwen_coder = json!({
            "model": "therouter/qwen/qwen3-coder-480b",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "abc" } }
                ]
            }]
        });

        assert_eq!(
            replace_images_for_text_only_model(&mut mimo_pro, &json!({}), true),
            1
        );
        assert_eq!(
            replace_images_for_text_only_model(&mut mimo_multimodal, &json!({}), true),
            0
        );
        assert_eq!(
            replace_images_for_text_only_model(&mut kimi_multimodal, &json!({}), true),
            0
        );
        assert_eq!(
            replace_images_for_text_only_model(&mut qwen_coder, &json!({}), true),
            1
        );
    }

    #[test]
    fn marker_replacement_preserves_cache_control_and_nested_blocks() {
        let mut body = json!({
            "messages": [{
                "content": [{
                    "type": "tool_result",
                    "content": [{
                        "type": "image",
                        "source": { "type": "base64", "media_type": "image/png", "data": "abc" },
                        "cache_control": { "type": "ephemeral" }
                    }]
                }]
            }]
        });

        assert!(contains_image_blocks(&body));
        assert_eq!(replace_image_blocks_with_marker(&mut body), 1);
        let block = &body["messages"][0]["content"][0]["content"][0];
        assert_eq!(block["type"], "text");
        assert_eq!(block["text"], UNSUPPORTED_IMAGE_MARKER);
        assert_eq!(block["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn unsupported_image_error_detection_uses_status_and_error_text() {
        assert!(is_unsupported_image_error(
            400,
            Some(r#"{"error":{"message":"This model cannot process media inputs"}}"#)
        ));
        assert!(is_unsupported_image_error(
            422,
            Some(r#"{"message":"attachments are not supported by this model"}"#)
        ));
        assert!(!is_unsupported_image_error(
            400,
            Some(r#"{"error":{"message":"Invalid API key"}}"#)
        ));
        assert!(!is_unsupported_image_error(
            500,
            Some(r#"{"error":{"message":"This model does not support image input"}}"#)
        ));
        assert!(is_unsupported_image_error(
            400,
            Some(
                r#"{"error":{"message":"Failed to deserialize: unknown variant image_url, expected text"}}"#
            )
        ));
    }
}
