use sha2::{Digest, Sha256};

pub fn stable_channel_id(
    app_type: &str,
    provider_id: &str,
    source_kind: &str,
    base_url: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(app_type.as_bytes());
    hasher.update([0]);
    hasher.update(provider_id.as_bytes());
    hasher.update([0]);
    hasher.update(source_kind.as_bytes());
    hasher.update([0]);
    hasher.update(base_url.as_bytes());
    let digest = hasher.finalize();
    let suffix = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "legacy-{}-{}-{}-{suffix}",
        slug_part(app_type),
        slug_part(provider_id),
        source_kind.replace('_', "-")
    )
}

fn slug_part(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 48 {
            break;
        }
    }
    slug.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::stable_channel_id;

    #[test]
    fn stable_channel_id_preserves_legacy_projection_format() {
        let channel_id = stable_channel_id(
            "Claude",
            "Provider A/Primary",
            "legacy_primary",
            "https://primary.example.com/v1",
        );

        assert_eq!(
            channel_id,
            "legacy-claude-provider-a-primary-legacy-primary-20a750bbc548"
        );
    }

    #[test]
    fn stable_channel_id_changes_with_route_identity_parts() {
        let primary = stable_channel_id("claude", "provider-a", "legacy_primary", "https://a/v1");
        let endpoint =
            stable_channel_id("claude", "provider-a", "legacy_endpoint", "https://a/v1");
        let other_base =
            stable_channel_id("claude", "provider-a", "legacy_primary", "https://b/v1");

        assert_ne!(primary, endpoint);
        assert_ne!(primary, other_base);
    }

    #[test]
    fn stable_channel_id_truncates_long_slug_parts() {
        let channel_id = stable_channel_id(
            "claude-with-a-very-long-application-name-that-keeps-going",
            "provider-with-a-very-long-provider-name-that-keeps-going",
            "legacy_endpoint",
            "https://primary.example.com/v1",
        );

        assert!(channel_id.starts_with(
            "legacy-claude-with-a-very-long-application-name-that-ke-provider-with-a-very-long-provider-name-that-kee-legacy-endpoint-"
        ));
    }
}
