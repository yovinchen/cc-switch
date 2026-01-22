//! 协议转换辅助模块
//!
//! 提供通用的协议配置解析和转换功能，供各适配器使用

use crate::provider::Provider;
use crate::proxy::unified::protocol::{ProtocolConfig, ProtocolFormat};

/// 从 Provider 配置中解析协议配置
///
/// 支持的配置字段：
/// - `source_format`: 源协议格式
/// - `target_format`: 目标协议格式
/// - `preserve_reasoning`: 是否保留思维链（默认 true）
/// - `transform_tools`: 是否转换工具调用（默认 true）
///
/// # Arguments
/// * `provider` - Provider 配置
/// * `default_source` - 默认源格式（如果未配置）
/// * `default_target` - 默认目标格式（如果未配置）
pub fn get_protocol_config(
    provider: &Provider,
    default_source: ProtocolFormat,
    default_target: ProtocolFormat,
) -> ProtocolConfig {
    let settings = &provider.settings_config;

    // 解析源格式
    let source_format = settings
        .get("source_format")
        .and_then(|v| v.as_str())
        .and_then(ProtocolFormat::from_str)
        .unwrap_or(default_source);

    // 解析目标格式
    let target_format = settings
        .get("target_format")
        .and_then(|v| v.as_str())
        .and_then(ProtocolFormat::from_str)
        .unwrap_or(default_target);

    // 解析其他选项
    let preserve_reasoning = settings
        .get("preserve_reasoning")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let transform_tools = settings
        .get("transform_tools")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    ProtocolConfig {
        source_format,
        target_format,
        transform_enabled: source_format != target_format,
        model_mapping: std::collections::HashMap::new(),
        preserve_reasoning,
        transform_tools,
    }
}

/// 检查是否配置了自定义协议转换
///
/// 如果配置了 `source_format` 或 `target_format`，则认为启用了自定义转换
pub fn has_custom_protocol_config(provider: &Provider) -> bool {
    let settings = &provider.settings_config;
    settings.get("source_format").is_some() || settings.get("target_format").is_some()
}

/// 获取协议格式的默认端点
pub fn get_default_endpoint(format: ProtocolFormat) -> &'static str {
    format.default_endpoint()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Provider".to_string(),
            settings_config: config,
            website_url: None,
            category: None,
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
    fn test_get_protocol_config_default() {
        let provider = create_provider(json!({}));
        let config = get_protocol_config(
            &provider,
            ProtocolFormat::Anthropic,
            ProtocolFormat::OpenAIChat,
        );

        assert_eq!(config.source_format, ProtocolFormat::Anthropic);
        assert_eq!(config.target_format, ProtocolFormat::OpenAIChat);
        assert!(config.transform_enabled);
    }

    #[test]
    fn test_get_protocol_config_custom() {
        let provider = create_provider(json!({
            "source_format": "openai",
            "target_format": "gemini"
        }));
        let config = get_protocol_config(
            &provider,
            ProtocolFormat::Anthropic,
            ProtocolFormat::OpenAIChat,
        );

        assert_eq!(config.source_format, ProtocolFormat::OpenAIChat);
        assert_eq!(config.target_format, ProtocolFormat::Gemini);
        assert!(config.transform_enabled);
    }

    #[test]
    fn test_get_protocol_config_same_format() {
        let provider = create_provider(json!({
            "source_format": "anthropic",
            "target_format": "anthropic"
        }));
        let config = get_protocol_config(
            &provider,
            ProtocolFormat::Anthropic,
            ProtocolFormat::OpenAIChat,
        );

        assert_eq!(config.source_format, ProtocolFormat::Anthropic);
        assert_eq!(config.target_format, ProtocolFormat::Anthropic);
        assert!(!config.transform_enabled);
    }

    #[test]
    fn test_has_custom_protocol_config() {
        let provider_no_config = create_provider(json!({}));
        assert!(!has_custom_protocol_config(&provider_no_config));

        let provider_with_source = create_provider(json!({
            "source_format": "openai"
        }));
        assert!(has_custom_protocol_config(&provider_with_source));

        let provider_with_target = create_provider(json!({
            "target_format": "gemini"
        }));
        assert!(has_custom_protocol_config(&provider_with_target));
    }

    #[test]
    fn test_protocol_aliases() {
        let provider = create_provider(json!({
            "source_format": "claude",
            "target_format": "gpt"
        }));
        let config = get_protocol_config(
            &provider,
            ProtocolFormat::OpenAIChat,
            ProtocolFormat::Gemini,
        );

        assert_eq!(config.source_format, ProtocolFormat::Anthropic);
        assert_eq!(config.target_format, ProtocolFormat::OpenAIChat);
    }
}
