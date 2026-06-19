pub fn mask_secret(value: &str) -> String {
    if value.chars().count() > 8 {
        let prefix: String = value.chars().take(4).collect();
        let suffix: String = value
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{prefix}...{suffix}")
    } else {
        "***".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::mask_secret;

    #[test]
    fn mask_secret_keeps_prefix_and_suffix_for_long_values() {
        assert_eq!(mask_secret("sk-1234567890abcdef"), "sk-1...cdef");
    }

    #[test]
    fn mask_secret_redacts_short_values() {
        assert_eq!(mask_secret("short"), "***");
        assert_eq!(mask_secret("12345678"), "***");
    }

    #[test]
    fn mask_secret_handles_nine_char_boundary() {
        assert_eq!(mask_secret("123456789"), "1234...6789");
    }

    #[test]
    fn mask_secret_is_utf8_safe() {
        assert!(!mask_secret("测试⚠️1234567890").is_empty());
    }
}
