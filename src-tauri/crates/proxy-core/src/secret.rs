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

pub fn mask_url_for_log(url: &str) -> String {
    if let Ok(uri) = url.parse::<http::Uri>() {
        if let (Some(scheme), Some(authority)) = (uri.scheme_str(), uri.authority()) {
            let authority = authority.as_str();
            let masked_authority = authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host);
            return format!("{scheme}://{masked_authority}");
        }
    }

    if url.chars().count() > 20 {
        format!("{}...", url.chars().take(20).collect::<String>())
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{mask_secret, mask_url_for_log};

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

    #[test]
    fn mask_url_for_log_strips_userinfo_and_preserves_target() {
        assert_eq!(
            mask_url_for_log("http://user:pass@127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            mask_url_for_log("socks5://admin:secret@proxy.example.com:1080"),
            "socks5://proxy.example.com:1080"
        );
        assert_eq!(
            mask_url_for_log("https://user:pass@proxy.example.com"),
            "https://proxy.example.com"
        );
    }

    #[test]
    fn mask_url_for_log_preserves_urls_without_userinfo() {
        assert_eq!(
            mask_url_for_log("http://127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            mask_url_for_log("http://proxy.example.com"),
            "http://proxy.example.com"
        );
    }

    #[test]
    fn mask_url_for_log_truncates_invalid_long_urls_safely() {
        assert_eq!(
            mask_url_for_log("not a valid url with spaces and secrets"),
            "not a valid url with..."
        );
        assert!(!mask_url_for_log("令牌不是url但是很长很长1234567890").is_empty());
    }
}
