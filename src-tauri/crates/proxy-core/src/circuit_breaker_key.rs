pub fn provider_circuit_key(app_type: &str, provider_id: &str) -> String {
    format!("{app_type}:{provider_id}")
}

pub fn channel_circuit_key(app_type: &str, channel_id: &str) -> String {
    format!("channel:{app_type}:{channel_id}")
}

pub fn provider_circuit_key_prefix(app_type: &str) -> String {
    format!("{app_type}:")
}

pub fn channel_circuit_key_prefix(app_type: &str) -> String {
    format!("channel:{app_type}:")
}

pub fn app_type_from_circuit_key(key: &str) -> &str {
    key.strip_prefix("channel:")
        .and_then(|rest| rest.split(':').next())
        .unwrap_or_else(|| key.split(':').next().unwrap_or("claude"))
}

#[cfg(test)]
mod tests {
    use super::{
        app_type_from_circuit_key, channel_circuit_key, channel_circuit_key_prefix,
        provider_circuit_key, provider_circuit_key_prefix,
    };

    #[test]
    fn provider_keys_preserve_existing_shape() {
        assert_eq!(provider_circuit_key("claude", "provider-a"), "claude:provider-a");
        assert_eq!(provider_circuit_key_prefix("claude"), "claude:");
        assert_eq!(app_type_from_circuit_key("claude:provider-a"), "claude");
    }

    #[test]
    fn channel_keys_preserve_existing_shape() {
        assert_eq!(
            channel_circuit_key("claude", "channel-a"),
            "channel:claude:channel-a"
        );
        assert_eq!(channel_circuit_key_prefix("claude"), "channel:claude:");
        assert_eq!(
            app_type_from_circuit_key("channel:claude:channel-a"),
            "claude"
        );
    }

    #[test]
    fn malformed_key_preserves_legacy_app_extraction() {
        assert_eq!(app_type_from_circuit_key(""), "");
        assert_eq!(app_type_from_circuit_key("provider-a"), "provider-a");
    }
}
