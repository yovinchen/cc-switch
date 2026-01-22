//! Claude (Anthropic) Provider Adapter
//!
//! 支持透传模式、OpenRouter 兼容模式和多协议转换
//!
//! ## 认证模式
//! - **Claude**: Anthropic 官方 API (x-api-key + anthropic-version)
//! - **ClaudeAuth**: 中转服务 (仅 Bearer 认证，无 x-api-key)
//! - **OpenRouter**: 已支持 Claude Code 兼容接口，默认透传（保留旧转换逻辑备用）
//!
//! ## 转换器选择
//! - **legacy**: 使用 transform.rs（旧版转换器）
//! - **rig**: 使用 transform_v2.rs（基于 Rig 设计的统一格式转换器）
//!
//! ## 多协议转换
//! 支持通过 `source_format` 和 `target_format` 配置自定义协议转换：
//! - `source_format`: 源协议格式（默认 "anthropic"）
//! - `target_format`: 目标协议格式（默认 "openai"）

use super::{AuthInfo, AuthStrategy, ProviderAdapter, ProviderType};
use super::protocol_helper;
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::proxy::unified::{
    converter::ProtocolConverter,
    protocol::{ProtocolConfig, ProtocolFormat},
};
use reqwest::RequestBuilder;

/// 转换器类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConverterType {
    /// 旧版转换器 (transform.rs)
    #[default]
    Legacy,
    /// Rig 统一格式转换器 (transform_v2.rs)
    Rig,
}

/// Claude 适配器
pub struct ClaudeAdapter;

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self
    }

    /// 获取转换器类型
    ///
    /// 从 Provider 配置中读取 `converter` 字段：
    /// - "rig" 或 "v2": 使用 Rig 统一格式转换器
    /// - "legacy" 或 "v1" 或其他: 使用旧版转换器（默认）
    pub fn get_converter_type(&self, provider: &Provider) -> ConverterType {
        let raw = provider.settings_config.get("converter");
        match raw {
            Some(serde_json::Value::String(value)) => {
                let normalized = value.trim().to_lowercase();
                match normalized.as_str() {
                    "rig" | "v2" | "unified" => ConverterType::Rig,
                    _ => ConverterType::Legacy,
                }
            }
            _ => ConverterType::Legacy,
        }
    }

    /// 获取协议配置
    ///
    /// 从 Provider 配置中解析 `source_format` 和 `target_format`
    /// 默认：Anthropic → OpenAI
    pub fn get_protocol_config(&self, provider: &Provider) -> ProtocolConfig {
        protocol_helper::get_protocol_config(
            provider,
            ProtocolFormat::Anthropic,
            ProtocolFormat::OpenAIChat,
        )
    }

    /// 检查是否配置了自定义协议转换
    pub fn has_custom_protocol(&self, provider: &Provider) -> bool {
        protocol_helper::has_custom_protocol_config(provider)
    }

    /// 获取供应商类型
    ///
    /// 根据 base_url 和 auth_mode 检测具体的供应商类型：
    /// - ChatCompletions: chat_completions_mode 为 true（优先级最高）
    /// - OpenRouter: base_url 包含 openrouter.ai
    /// - ClaudeAuth: auth_mode 为 bearer_only
    /// - Claude: 默认 Anthropic 官方
    pub fn provider_type(&self, provider: &Provider) -> ProviderType {
        // 检测 ChatCompletions 模式（优先级最高）
        if self.is_chat_completions_mode(provider) {
            return ProviderType::ChatCompletions;
        }

        // 检测 OpenRouter
        if self.is_openrouter(provider) {
            return ProviderType::OpenRouter;
        }

        // 检测 ClaudeAuth (仅 Bearer 认证)
        if self.is_bearer_only_mode(provider) {
            return ProviderType::ClaudeAuth;
        }

        ProviderType::Claude
    }

    /// 检测是否使用 OpenRouter
    fn is_openrouter(&self, provider: &Provider) -> bool {
        if let Ok(base_url) = self.extract_base_url(provider) {
            return base_url.contains("openrouter.ai");
        }
        false
    }

    /// 检测是否启用 ChatCompletions 兼容模式
    fn is_chat_completions_mode(&self, provider: &Provider) -> bool {
        let raw = provider.settings_config.get("chat_completions_mode");
        match raw {
            Some(serde_json::Value::Bool(enabled)) => *enabled,
            Some(serde_json::Value::Number(num)) => num.as_i64().unwrap_or(0) != 0,
            Some(serde_json::Value::String(value)) => {
                let normalized = value.trim().to_lowercase();
                normalized == "true" || normalized == "1"
            }
            _ => false,
        }
    }

    /// 检测 OpenRouter 是否启用兼容模式
    fn is_openrouter_compat_enabled(&self, provider: &Provider) -> bool {
        if !self.is_openrouter(provider) {
            return false;
        }

        let raw = provider.settings_config.get("openrouter_compat_mode");
        match raw {
            Some(serde_json::Value::Bool(enabled)) => *enabled,
            Some(serde_json::Value::Number(num)) => num.as_i64().unwrap_or(0) != 0,
            Some(serde_json::Value::String(value)) => {
                let normalized = value.trim().to_lowercase();
                normalized == "true" || normalized == "1"
            }
            // OpenRouter now supports Claude Code compatible API, default to passthrough
            _ => false,
        }
    }

    /// 检测是否为仅 Bearer 认证模式
    fn is_bearer_only_mode(&self, provider: &Provider) -> bool {
        // 检查 settings_config 中的 auth_mode
        if let Some(auth_mode) = provider
            .settings_config
            .get("auth_mode")
            .and_then(|v| v.as_str())
        {
            if auth_mode == "bearer_only" {
                return true;
            }
        }

        // 检查 env 中的 AUTH_MODE
        if let Some(env) = provider.settings_config.get("env") {
            if let Some(auth_mode) = env.get("AUTH_MODE").and_then(|v| v.as_str()) {
                if auth_mode == "bearer_only" {
                    return true;
                }
            }
        }

        false
    }

    /// 从 Provider 配置中提取 API Key
    fn extract_key(&self, provider: &Provider) -> Option<String> {
        if let Some(env) = provider.settings_config.get("env") {
            // Anthropic 标准 key
            if let Some(key) = env
                .get("ANTHROPIC_AUTH_TOKEN")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                log::debug!("[Claude] 使用 ANTHROPIC_AUTH_TOKEN");
                return Some(key.to_string());
            }
            if let Some(key) = env
                .get("ANTHROPIC_API_KEY")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                log::debug!("[Claude] 使用 ANTHROPIC_API_KEY");
                return Some(key.to_string());
            }
            // OpenRouter key
            if let Some(key) = env
                .get("OPENROUTER_API_KEY")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                log::debug!("[Claude] 使用 OPENROUTER_API_KEY");
                return Some(key.to_string());
            }
            // 备选 OpenAI key (用于 OpenRouter)
            if let Some(key) = env
                .get("OPENAI_API_KEY")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                log::debug!("[Claude] 使用 OPENAI_API_KEY");
                return Some(key.to_string());
            }
        }

        // 尝试直接获取
        if let Some(key) = provider
            .settings_config
            .get("apiKey")
            .or_else(|| provider.settings_config.get("api_key"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            log::debug!("[Claude] 使用 apiKey/api_key");
            return Some(key.to_string());
        }

        log::warn!("[Claude] 未找到有效的 API Key");
        None
    }
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAdapter for ClaudeAdapter {
    fn name(&self) -> &'static str {
        "Claude"
    }

    fn extract_base_url(&self, provider: &Provider) -> Result<String, ProxyError> {
        // 1. 从 env 中获取
        if let Some(env) = provider.settings_config.get("env") {
            if let Some(url) = env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()) {
                return Ok(url.trim_end_matches('/').to_string());
            }
        }

        // 2. 尝试直接获取
        if let Some(url) = provider
            .settings_config
            .get("base_url")
            .and_then(|v| v.as_str())
        {
            return Ok(url.trim_end_matches('/').to_string());
        }

        if let Some(url) = provider
            .settings_config
            .get("baseURL")
            .and_then(|v| v.as_str())
        {
            return Ok(url.trim_end_matches('/').to_string());
        }

        if let Some(url) = provider
            .settings_config
            .get("apiEndpoint")
            .and_then(|v| v.as_str())
        {
            return Ok(url.trim_end_matches('/').to_string());
        }

        Err(ProxyError::ConfigError(
            "Claude Provider 缺少 base_url 配置".to_string(),
        ))
    }

    fn extract_auth(&self, provider: &Provider) -> Option<AuthInfo> {
        let provider_type = self.provider_type(provider);
        let strategy = match provider_type {
            ProviderType::OpenRouter => AuthStrategy::Bearer,
            ProviderType::ClaudeAuth => AuthStrategy::ClaudeAuth,
            _ => AuthStrategy::Anthropic,
        };

        self.extract_key(provider)
            .map(|key| AuthInfo::new(key, strategy))
    }

    fn build_url(&self, base_url: &str, endpoint: &str) -> String {
        // NOTE:
        // 过去 OpenRouter 只有 OpenAI Chat Completions 兼容接口，需要把 Claude 的 `/v1/messages`
        // 映射到 `/v1/chat/completions`，并做 Anthropic ↔ OpenAI 的格式转换。
        //
        // 现在 OpenRouter 已推出 Claude Code 兼容接口，因此默认直接透传 endpoint。
        // 如需回退旧逻辑，可在 forwarder 中根据 needs_transform 改写 endpoint。

        let base = format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            endpoint.trim_start_matches('/')
        );

        // 为 /v1/messages 端点添加 ?beta=true 参数
        // 这是某些上游服务（如 DuckCoding）验证请求来源的关键参数
        if endpoint.contains("/v1/messages") && !endpoint.contains("?") {
            format!("{base}?beta=true")
        } else {
            base
        }
    }

    fn add_auth_headers(&self, request: RequestBuilder, auth: &AuthInfo) -> RequestBuilder {
        // 注意：anthropic-version 由 forwarder.rs 统一处理（透传客户端值或设置默认值）
        // 这里不再设置 anthropic-version，避免 header 重复
        match auth.strategy {
            // Anthropic 官方: Authorization Bearer + x-api-key
            AuthStrategy::Anthropic => request
                .header("Authorization", format!("Bearer {}", auth.api_key))
                .header("x-api-key", &auth.api_key),
            // ClaudeAuth 中转服务: 仅 Bearer，无 x-api-key
            AuthStrategy::ClaudeAuth => {
                request.header("Authorization", format!("Bearer {}", auth.api_key))
            }
            // OpenRouter: Bearer
            AuthStrategy::Bearer => {
                request.header("Authorization", format!("Bearer {}", auth.api_key))
            }
            _ => request,
        }
    }

    fn needs_transform(&self, provider: &Provider) -> bool {
        // 1. 检查是否配置了自定义协议转换
        if self.has_custom_protocol(provider) {
            let config = self.get_protocol_config(provider);
            return config.needs_transform();
        }

        // 2. ChatCompletions 模式或 OpenRouter 兼容模式都需要转换
        self.is_chat_completions_mode(provider) || self.is_openrouter_compat_enabled(provider)
    }

    fn transform_request(
        &self,
        body: serde_json::Value,
        provider: &Provider,
    ) -> Result<serde_json::Value, ProxyError> {
        // 检查是否使用自定义协议转换
        if self.has_custom_protocol(provider) {
            let config = self.get_protocol_config(provider);
            log::debug!(
                "[Claude] 使用多协议转换: {} -> {}",
                config.source_format,
                config.target_format
            );
            return ProtocolConverter::convert_request(body, &config, provider);
        }

        // 根据配置选择转换器
        match self.get_converter_type(provider) {
            ConverterType::Rig => {
                log::debug!("[Claude] 使用 Rig 转换器 (transform_v2)");
                super::transform_v2::UnifiedConverter::anthropic_to_openai(body, provider)
            }
            ConverterType::Legacy => {
                log::debug!("[Claude] 使用 Legacy 转换器 (transform)");
                super::transform::anthropic_to_openai(body, provider)
            }
        }
    }

    fn transform_response(&self, body: serde_json::Value) -> Result<serde_json::Value, ProxyError> {
        // 响应转换目前两个转换器实现相同，使用 legacy
        super::transform::openai_to_anthropic(body)
    }

    fn transform_response_with_provider(
        &self,
        body: serde_json::Value,
        provider: &Provider,
    ) -> Result<serde_json::Value, ProxyError> {
        // 检查是否使用自定义协议转换
        if self.has_custom_protocol(provider) {
            let config = self.get_protocol_config(provider);
            // 响应转换是请求转换的逆过程
            let response_config = ProtocolConfig::transform(config.target_format, config.source_format);
            log::debug!(
                "[Claude] 使用多协议响应转换: {} -> {}",
                response_config.source_format,
                response_config.target_format
            );
            return ProtocolConverter::convert_response(body, &response_config);
        }

        // 默认使用 legacy 转换器
        self.transform_response(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Claude".to_string(),
            settings_config: config,
            website_url: None,
            category: Some("claude".to_string()),
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn test_extract_base_url_from_env() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }));

        let url = adapter.extract_base_url(&provider).unwrap();
        assert_eq!(url, "https://api.anthropic.com");
    }

    #[test]
    fn test_extract_auth_anthropic() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-ant-test-key");
        assert_eq!(auth.strategy, AuthStrategy::Anthropic);
    }

    #[test]
    fn test_extract_auth_anthropic_api_key() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_API_KEY": "sk-ant-test-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-ant-test-key");
        assert_eq!(auth.strategy, AuthStrategy::Anthropic);
    }

    #[test]
    fn test_extract_auth_openrouter() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test-key"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-or-test-key");
        assert_eq!(auth.strategy, AuthStrategy::Bearer);
    }

    #[test]
    fn test_extract_auth_claude_auth_mode() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-proxy-key"
            },
            "auth_mode": "bearer_only"
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-proxy-key");
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
    }

    #[test]
    fn test_extract_auth_claude_auth_env_mode() {
        let adapter = ClaudeAdapter::new();
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-proxy-key",
                "AUTH_MODE": "bearer_only"
            }
        }));

        let auth = adapter.extract_auth(&provider).unwrap();
        assert_eq!(auth.api_key, "sk-proxy-key");
        assert_eq!(auth.strategy, AuthStrategy::ClaudeAuth);
    }

    #[test]
    fn test_provider_type_detection() {
        let adapter = ClaudeAdapter::new();

        // Anthropic 官方
        let anthropic = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
            }
        }));
        assert_eq!(adapter.provider_type(&anthropic), ProviderType::Claude);

        // OpenRouter
        let openrouter = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test"
            }
        }));
        assert_eq!(adapter.provider_type(&openrouter), ProviderType::OpenRouter);

        // ClaudeAuth
        let claude_auth = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-test"
            },
            "auth_mode": "bearer_only"
        }));
        assert_eq!(
            adapter.provider_type(&claude_auth),
            ProviderType::ClaudeAuth
        );
    }

    #[test]
    fn test_build_url_anthropic() {
        let adapter = ClaudeAdapter::new();
        // /v1/messages 端点会自动添加 ?beta=true 参数
        let url = adapter.build_url("https://api.anthropic.com", "/v1/messages");
        assert_eq!(url, "https://api.anthropic.com/v1/messages?beta=true");
    }

    #[test]
    fn test_build_url_openrouter() {
        let adapter = ClaudeAdapter::new();
        // /v1/messages 端点会自动添加 ?beta=true 参数
        let url = adapter.build_url("https://openrouter.ai/api", "/v1/messages");
        assert_eq!(url, "https://openrouter.ai/api/v1/messages?beta=true");
    }

    #[test]
    fn test_build_url_no_beta_for_other_endpoints() {
        let adapter = ClaudeAdapter::new();
        // 非 /v1/messages 端点不添加 ?beta=true
        let url = adapter.build_url("https://api.anthropic.com", "/v1/complete");
        assert_eq!(url, "https://api.anthropic.com/v1/complete");
    }

    #[test]
    fn test_build_url_preserve_existing_query() {
        let adapter = ClaudeAdapter::new();
        // 已有查询参数时不重复添加
        let url = adapter.build_url("https://api.anthropic.com", "/v1/messages?foo=bar");
        assert_eq!(url, "https://api.anthropic.com/v1/messages?foo=bar");
    }

    #[test]
    fn test_needs_transform() {
        let adapter = ClaudeAdapter::new();

        let anthropic_provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }));
        assert!(!adapter.needs_transform(&anthropic_provider));

        // OpenRouter provider without explicit setting now defaults to passthrough (no transform)
        let openrouter_provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api"
            }
        }));
        assert!(!adapter.needs_transform(&openrouter_provider));

        // OpenRouter provider with explicit compat mode enabled should transform
        let openrouter_enabled = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api"
            },
            "openrouter_compat_mode": true
        }));
        assert!(adapter.needs_transform(&openrouter_enabled));

        let openrouter_disabled = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api"
            },
            "openrouter_compat_mode": false
        }));
        assert!(!adapter.needs_transform(&openrouter_disabled));
    }

    #[test]
    fn test_converter_type_default() {
        let adapter = ClaudeAdapter::new();
        
        // 默认使用 Legacy 转换器
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            }
        }));
        assert_eq!(adapter.get_converter_type(&provider), ConverterType::Legacy);
    }

    #[test]
    fn test_converter_type_rig() {
        let adapter = ClaudeAdapter::new();
        
        // 显式指定 rig 转换器
        let provider_rig = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "converter": "rig"
        }));
        assert_eq!(adapter.get_converter_type(&provider_rig), ConverterType::Rig);

        // v2 也映射到 Rig
        let provider_v2 = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "converter": "v2"
        }));
        assert_eq!(adapter.get_converter_type(&provider_v2), ConverterType::Rig);

        // unified 也映射到 Rig
        let provider_unified = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "converter": "unified"
        }));
        assert_eq!(adapter.get_converter_type(&provider_unified), ConverterType::Rig);
    }

    #[test]
    fn test_converter_type_legacy() {
        let adapter = ClaudeAdapter::new();
        
        // 显式指定 legacy 转换器
        let provider_legacy = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "converter": "legacy"
        }));
        assert_eq!(adapter.get_converter_type(&provider_legacy), ConverterType::Legacy);

        // v1 也映射到 Legacy
        let provider_v1 = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "converter": "v1"
        }));
        assert_eq!(adapter.get_converter_type(&provider_v1), ConverterType::Legacy);
    }

    #[test]
    fn test_converter_type_case_insensitive() {
        let adapter = ClaudeAdapter::new();
        
        // 大小写不敏感
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com"
            },
            "converter": "RIG"
        }));
        assert_eq!(adapter.get_converter_type(&provider), ConverterType::Rig);
    }
}
