use super::error::{ProxyCoreError, ProxyCoreResult};

pub fn validate_management_app_type(app_type: &str) -> ProxyCoreResult<()> {
    if app_type.trim().is_empty() {
        Err(ProxyCoreError::InvalidRequest(
            "app cannot be empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

pub fn validate_route_resolve_app_type(app_type: &str) -> ProxyCoreResult<()> {
    if app_type.trim().is_empty() {
        Err(ProxyCoreError::InvalidRequest(
            "appType/app_type cannot be empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

pub fn normalize_channel_id_path(channel_id: impl AsRef<str>) -> ProxyCoreResult<String> {
    let channel_id = channel_id.as_ref().trim().to_string();
    if channel_id.is_empty() {
        Err(ProxyCoreError::InvalidRequest(
            "channel_id cannot be empty".to_string(),
        ))
    } else {
        Ok(channel_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_channel_id_path, validate_management_app_type, validate_route_resolve_app_type,
    };

    #[test]
    fn validate_management_app_type_rejects_blank_values() {
        assert!(validate_management_app_type("claude").is_ok());
        let error = validate_management_app_type(" ").unwrap_err();

        assert_eq!(error.to_string(), "invalid proxy request: app cannot be empty");
    }

    #[test]
    fn validate_route_resolve_app_type_uses_route_request_message() {
        assert!(validate_route_resolve_app_type("codex").is_ok());
        let error = validate_route_resolve_app_type("").unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid proxy request: appType/app_type cannot be empty"
        );
    }

    #[test]
    fn normalize_channel_id_path_trims_and_rejects_blank_values() {
        assert_eq!(
            normalize_channel_id_path(" channel-a ").expect("normalize id"),
            "channel-a"
        );

        let error = normalize_channel_id_path(" ").unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid proxy request: channel_id cannot be empty"
        );
    }
}
