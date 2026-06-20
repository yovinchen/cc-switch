use super::domain::DEFAULT_ROUTE_GROUP;
use super::ports::{
    ProxyChannelKeyPatchRequest, ProxyChannelKeyWriteRequest, ProxyChannelModelWriteRequest,
    ProxyChannelWriteRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRequestValidationError {
    pub field: String,
    pub message: String,
}

impl ChannelRequestValidationError {
    fn required(field: &str) -> Self {
        Self {
            field: field.to_string(),
            message: format!("{field} cannot be empty"),
        }
    }

    fn invalid(field: &str, message: impl Into<String>) -> Self {
        Self {
            field: field.to_string(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ChannelRequestValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ChannelRequestValidationError {}

pub fn validate_proxy_channel_write_request_fields(
    request: &ProxyChannelWriteRequest,
) -> Result<(), ChannelRequestValidationError> {
    normalize_required_channel_string(&request.provider_id, "providerId")?;
    normalize_required_channel_string(&request.name, "name")?;
    normalize_required_channel_string(&request.status, "status")?;
    if normalize_channel_base_url(&request.base_url).is_empty() {
        return Err(ChannelRequestValidationError::required("baseUrl"));
    }
    normalize_required_channel_string(&request.interface_kind, "interfaceKind")?;
    validate_optional_channel_auth_profile_ref(request.auth_profile_ref.as_deref())?;
    for model in &request.models {
        validate_proxy_channel_model_write_request_fields(model)?;
    }
    Ok(())
}

pub fn validate_proxy_channel_model_write_request_fields(
    model: &ProxyChannelModelWriteRequest,
) -> Result<(), ChannelRequestValidationError> {
    normalize_required_channel_string(&model.public_model, "publicModel")?;
    normalize_required_channel_string(&model.upstream_model, "upstreamModel")?;
    Ok(())
}

pub fn validate_proxy_channel_key_write_request_fields(
    request: &ProxyChannelKeyWriteRequest,
) -> Result<(), ChannelRequestValidationError> {
    normalize_required_channel_string(&request.key_value, "keyValue")?;
    normalize_required_channel_string(&request.status, "status")?;
    Ok(())
}

pub fn validate_proxy_channel_key_patch_request_fields(
    request: &ProxyChannelKeyPatchRequest,
) -> Result<(), ChannelRequestValidationError> {
    if let Some(key_value) = request.key_value.as_deref() {
        normalize_required_channel_string(key_value, "keyValue")?;
    }
    if let Some(status) = request.status.as_deref() {
        normalize_required_channel_string(status, "status")?;
    }
    Ok(())
}

pub fn normalize_required_channel_string(
    value: &str,
    field: &str,
) -> Result<String, ChannelRequestValidationError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        Err(ChannelRequestValidationError::required(field))
    } else {
        Ok(normalized.to_string())
    }
}

pub fn normalize_optional_channel_string(value: String) -> Option<String> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub fn validate_optional_channel_auth_profile_ref(
    auth_profile_ref: Option<&str>,
) -> Result<(), ChannelRequestValidationError> {
    let Some(auth_profile_ref) = auth_profile_ref.map(str::trim).filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    if let Some(key_ref) = auth_profile_ref.strip_prefix("channel-key:") {
        return validate_auth_profile_part(key_ref).map_err(|_| invalid_auth_profile_ref());
    }

    if let Some(provider_ref) = auth_profile_ref.strip_prefix("provider:") {
        let mut parts = provider_ref.splitn(2, ':');
        let app = parts.next().unwrap_or_default();
        let provider_id = parts.next().unwrap_or_default();
        return validate_auth_profile_part(app)
            .and_then(|_| validate_auth_profile_part(provider_id))
            .map_err(|_| invalid_auth_profile_ref());
    }

    Err(invalid_auth_profile_ref())
}

fn validate_auth_profile_part(value: &str) -> Result<(), ()> {
    if value.trim().is_empty() {
        Err(())
    } else {
        Ok(())
    }
}

fn invalid_auth_profile_ref() -> ChannelRequestValidationError {
    ChannelRequestValidationError::invalid(
        "authProfileRef",
        "authProfileRef must be provider:<app>:<providerId> or channel-key:<keyRef>",
    )
}

pub fn normalize_channel_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

pub fn normalize_channel_groups(groups: Vec<String>) -> Vec<String> {
    let mut normalized: Vec<String> = groups
        .into_iter()
        .map(|group| group.trim().to_string())
        .filter(|group| !group.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();
    if normalized.is_empty() {
        vec![DEFAULT_ROUTE_GROUP.to_string()]
    } else {
        normalized
    }
}

pub fn channel_object_or_default(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        Value::Object(Map::new())
    }
}

pub fn channel_array_or_default(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        Value::Array(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        channel_array_or_default, channel_object_or_default, normalize_channel_base_url,
        normalize_channel_groups, normalize_optional_channel_string,
        normalize_required_channel_string, validate_optional_channel_auth_profile_ref,
        validate_proxy_channel_model_write_request_fields,
        validate_proxy_channel_key_patch_request_fields,
        validate_proxy_channel_key_write_request_fields, validate_proxy_channel_write_request_fields,
    };
    use crate::ports::{
        ProxyChannelKeyPatchRequest, ProxyChannelKeyWriteRequest, ProxyChannelModelWriteRequest,
        ProxyChannelWriteRequest,
    };
    use serde_json::json;

    #[test]
    fn channel_base_url_trims_whitespace_and_trailing_slashes() {
        assert_eq!(
            normalize_channel_base_url(" https://relay.example.com/v1/// "),
            "https://relay.example.com/v1"
        );
    }

    #[test]
    fn channel_groups_trim_sort_dedup_and_default() {
        assert_eq!(
            normalize_channel_groups(vec![
                " beta ".to_string(),
                "default".to_string(),
                "beta".to_string(),
                " ".to_string(),
            ]),
            vec!["beta".to_string(), "default".to_string()]
        );
        assert_eq!(normalize_channel_groups(vec![]), vec!["default".to_string()]);
    }

    #[test]
    fn required_and_optional_strings_follow_management_request_rules() {
        assert_eq!(
            normalize_required_channel_string(" relay ", "name").unwrap(),
            "relay"
        );
        assert_eq!(
            normalize_required_channel_string(" ", "name")
                .unwrap_err()
                .message,
            "name cannot be empty"
        );
        assert_eq!(
            normalize_optional_channel_string(" provider:claude:x ".to_string()).as_deref(),
            Some("provider:claude:x")
        );
        assert_eq!(normalize_optional_channel_string(" ".to_string()), None);
    }

    #[test]
    fn auth_profile_ref_validation_accepts_supported_profiles_and_rejects_empty_parts() {
        validate_optional_channel_auth_profile_ref(None).unwrap();
        validate_optional_channel_auth_profile_ref(Some(" ")).unwrap();
        validate_optional_channel_auth_profile_ref(Some(" channel-key:primary ")).unwrap();
        validate_optional_channel_auth_profile_ref(Some(" provider:claude:provider-a ")).unwrap();

        let message =
            "authProfileRef must be provider:<app>:<providerId> or channel-key:<keyRef>";
        assert_eq!(
            validate_optional_channel_auth_profile_ref(Some("channel-key: "))
                .unwrap_err()
                .message,
            message
        );
        assert_eq!(
            validate_optional_channel_auth_profile_ref(Some("provider:claude: "))
                .unwrap_err()
                .message,
            message
        );
        assert_eq!(
            validate_optional_channel_auth_profile_ref(Some("vault:primary"))
                .unwrap_err()
                .message,
            message
        );
    }

    #[test]
    fn json_defaults_preserve_expected_container_types() {
        assert_eq!(channel_object_or_default(json!({"owner": "ops"})), json!({"owner": "ops"}));
        assert_eq!(channel_object_or_default(json!(["not", "object"])), json!({}));
        assert_eq!(channel_array_or_default(json!(["ok"])), json!(["ok"]));
        assert_eq!(channel_array_or_default(json!({"not": "array"})), json!([]));
    }

    #[test]
    fn write_request_validation_checks_required_fields_and_models() {
        let request = ProxyChannelWriteRequest {
            provider_id: "provider-a".to_string(),
            app_type: "claude".to_string(),
            name: "Relay".to_string(),
            base_url: "https://relay.example.com/v1/".to_string(),
            interface_kind: "openai_responses".to_string(),
            auth_profile_ref: Some("channel-key:primary".to_string()),
            models: vec![ProxyChannelModelWriteRequest {
                public_model: "sonnet".to_string(),
                upstream_model: "claude-sonnet-4".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        validate_proxy_channel_write_request_fields(&request).unwrap();

        let invalid = ProxyChannelWriteRequest {
            base_url: " ".to_string(),
            ..request
        };
        assert_eq!(
            validate_proxy_channel_write_request_fields(&invalid)
                .unwrap_err()
                .message,
            "baseUrl cannot be empty"
        );
    }

    #[test]
    fn model_write_request_validation_checks_public_and_upstream_model() {
        assert_eq!(
            validate_proxy_channel_model_write_request_fields(&ProxyChannelModelWriteRequest {
                public_model: " ".to_string(),
                upstream_model: "upstream".to_string(),
                ..Default::default()
            })
            .unwrap_err()
            .message,
            "publicModel cannot be empty"
        );
        assert_eq!(
            validate_proxy_channel_model_write_request_fields(&ProxyChannelModelWriteRequest {
                public_model: "public".to_string(),
                upstream_model: " ".to_string(),
                ..Default::default()
            })
            .unwrap_err()
            .message,
            "upstreamModel cannot be empty"
        );
    }

    #[test]
    fn channel_key_request_validation_checks_secret_and_status() {
        validate_proxy_channel_key_write_request_fields(&ProxyChannelKeyWriteRequest {
            key_value: " sk-live ".to_string(),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            validate_proxy_channel_key_write_request_fields(&ProxyChannelKeyWriteRequest {
                key_value: " ".to_string(),
                ..Default::default()
            })
            .unwrap_err()
            .message,
            "keyValue cannot be empty"
        );
        assert_eq!(
            validate_proxy_channel_key_patch_request_fields(&ProxyChannelKeyPatchRequest {
                status: Some(" ".to_string()),
                ..Default::default()
            })
            .unwrap_err()
            .message,
            "status cannot be empty"
        );
    }
}
