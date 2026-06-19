//! 模型列表获取服务
//!
//! 通过 OpenAI 兼容的 GET /v1/models 端点获取供应商可用模型列表。
//! 主要面向第三方聚合站（硅基流动、OpenRouter 等），以及把 Anthropic
//! 协议挂在兼容子路径上的官方供应商（DeepSeek、Kimi、智谱 GLM 等）。

use futures::future::BoxFuture;
use reqwest::header::HeaderValue;
use std::time::Duration;

use crate::proxy_core::{
    fetch_openai_compatible_models_with_transport, FetchedModel, ModelFetchHttpResponse,
    OpenAiCompatibleModelsRequest, OpenAiCompatibleModelsTransport,
};

struct ReqwestOpenAiCompatibleModelsTransport;

impl OpenAiCompatibleModelsTransport for ReqwestOpenAiCompatibleModelsTransport {
    fn send_openai_compatible_models_request<'a>(
        &'a self,
        request_plan: OpenAiCompatibleModelsRequest<'a>,
    ) -> BoxFuture<'a, Result<ModelFetchHttpResponse, String>> {
        Box::pin(async move {
            log::debug!("[ModelFetch] Trying endpoint: {}", request_plan.url);
            let client = crate::proxy::http_client::get();
            let mut request = client
                .get(request_plan.url)
                .header(
                    request_plan.authorization_header.0,
                    request_plan.authorization_header.1,
                )
                .timeout(Duration::from_secs(request_plan.timeout_secs));
            // 自定义 User-Agent：部分 /models 端点同样有 UA 白名单（如 Kimi Coding Plan），
            // 与转发 / 检测路径共用同一 UA，避免"代理可用但取模型失败"。
            if let Some((header, value)) = request_plan.user_agent_header {
                request = request.header(header, value.clone());
            }

            let response = request
                .send()
                .await
                .map_err(|e| format!("Request failed: {e}"))?;
            let status = response.status();
            let body = if status.is_success() {
                response
                    .bytes()
                    .await
                    .map_err(|e| format!("Failed to parse response: {e}"))?
                    .to_vec()
            } else {
                response.text().await.unwrap_or_default().into_bytes()
            };

            Ok(ModelFetchHttpResponse { status, body })
        })
    }
}

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
    let transport = ReqwestOpenAiCompatibleModelsTransport;
    fetch_openai_compatible_models_with_transport(
        base_url,
        api_key,
        is_full_url,
        models_url_override,
        user_agent.as_ref(),
        &transport,
    )
    .await
}
