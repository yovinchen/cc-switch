//! 模型列表获取服务
//!
//! 通过 OpenAI 兼容的 GET /v1/models 端点获取供应商可用模型列表。
//! 主要面向第三方聚合站（硅基流动、OpenRouter 等），以及把 Anthropic
//! 协议挂在兼容子路径上的官方供应商（DeepSeek、Kimi、智谱 GLM 等）。

use reqwest::header::HeaderValue;
use reqwest::StatusCode;
use std::time::Duration;

use crate::proxy_core::{
    build_models_url_candidates, build_openai_compatible_models_request,
    parse_models_response_bytes, truncate_model_fetch_error_body, FetchedModel,
};

const FETCH_TIMEOUT_SECS: u64 = 15;

/// 获取供应商的可用模型列表
///
/// 使用 OpenAI 兼容的 GET /v1/models 端点，按候选列表顺序尝试。
pub async fn fetch_models(
    base_url: &str,
    api_key: &str,
    is_full_url: bool,
    models_url_override: Option<&str>,
    user_agent: Option<HeaderValue>,
) -> Result<Vec<FetchedModel>, String> {
    if api_key.is_empty() {
        return Err("API Key is required to fetch models".to_string());
    }

    let candidates = build_models_url_candidates(base_url, is_full_url, models_url_override)?;
    let client = crate::proxy::http_client::get();
    let mut last_err: Option<String> = None;

    for url in &candidates {
        log::debug!("[ModelFetch] Trying endpoint: {url}");
        let request_plan =
            build_openai_compatible_models_request(url, api_key, user_agent.as_ref());
        let mut request = client
            .get(request_plan.url)
            .header(
                request_plan.authorization_header.0,
                request_plan.authorization_header.1,
            )
            .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS));
        // 自定义 User-Agent：部分 /models 端点同样有 UA 白名单（如 Kimi Coding Plan），
        // 与转发 / 检测路径共用同一 UA，避免"代理可用但取模型失败"。
        if let Some((header, value)) = request_plan.user_agent_header {
            request = request.header(header, value.clone());
        }
        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                return Err(format!("Request failed: {e}"));
            }
        };

        let status = response.status();

        if status.is_success() {
            let body = response
                .bytes()
                .await
                .map_err(|e| format!("Failed to parse response: {e}"))?;
            let models = parse_models_response_bytes(&body)
                .map_err(|e| format!("Failed to parse response: {e}"))?;
            return Ok(models);
        }

        if status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED {
            let body = truncate_model_fetch_error_body(response.text().await.unwrap_or_default());
            last_err = Some(format!("HTTP {status}: {body}"));
            continue;
        }

        let body = truncate_model_fetch_error_body(response.text().await.unwrap_or_default());
        return Err(format!("HTTP {status}: {body}"));
    }

    Err(format!(
        "All candidates failed: {}",
        last_err.unwrap_or_else(|| "no candidates".to_string())
    ))
}
