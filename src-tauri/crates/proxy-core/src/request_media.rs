#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaPreventionPolicy {
    pub should_attempt: bool,
    pub allow_heuristic: bool,
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

#[cfg(test)]
mod tests {
    use super::{
        resolve_media_prevention_policy, should_check_media_retry, should_trigger_media_retry,
        MediaRetryInput,
    };

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
}
