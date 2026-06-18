pub fn normalize_gemini_model_id(model: &str) -> &str {
    crate::proxy_core::normalize_gemini_model_id(model)
}

pub fn resolve_gemini_native_url(base_url: &str, endpoint: &str, is_full_url: bool) -> String {
    crate::proxy_core::resolve_gemini_native_url(base_url, endpoint, is_full_url)
}

#[cfg(test)]
pub fn build_gemini_native_url(base_url: &str, endpoint: &str) -> String {
    crate::proxy_core::build_gemini_native_url(base_url, endpoint)
}
