//! 模型映射模块
//!
//! 在请求转发前，根据 Provider 配置替换请求中的模型名称
//!
//! ## 设计原则
//!
//! 这是**唯一的模型映射入口**，转换器（transform_v2.rs、converter.rs）不再执行模型映射。
//! 这样确保：
//! 1. 模型映射只执行一次，避免重复映射导致的问题
//! 2. 所有映射配置在一处统一管理
//! 3. 配置优先级清晰明确
//!
//! ## 配置优先级（从高到低）
//!
//! 1. 显式 `model_mapping` 配置（精确匹配）
//! 2. `ANTHROPIC_REASONING_MODEL`（当启用 thinking 模式时）
//! 3. 按模型类型匹配（haiku/opus/sonnet）
//! 4. 按模型族匹配（claude/gemini/gpt）
//! 5. 回退到默认模型或原始模型

use crate::provider::Provider;
use serde_json::Value;
use std::collections::HashMap;

/// 模型映射配置
pub struct ModelMapping {
    /// 显式模型映射表（精确匹配，优先级最高）
    pub explicit_mapping: HashMap<String, String>,
    /// Anthropic Haiku 模型
    pub haiku_model: Option<String>,
    /// Anthropic Sonnet 模型
    pub sonnet_model: Option<String>,
    /// Anthropic Opus 模型
    pub opus_model: Option<String>,
    /// Anthropic 默认模型
    pub anthropic_model: Option<String>,
    /// Anthropic 推理模型（thinking 模式）
    pub reasoning_model: Option<String>,
    /// Gemini 模型
    pub gemini_model: Option<String>,
    /// OpenAI 模型
    pub openai_model: Option<String>,
    /// 通用默认模型
    pub default_model: Option<String>,
}

impl ModelMapping {
    /// 从 Provider 配置中提取模型映射
    ///
    /// 支持以下配置来源：
    /// - `settings_config.model_mapping`: 显式映射表（精确匹配）
    /// - `settings_config.env.ANTHROPIC_*`: Anthropic 相关配置
    /// - `settings_config.env.GEMINI_MODEL`: Gemini 模型
    /// - `settings_config.env.OPENAI_MODEL`: OpenAI 模型
    /// - `settings_config.env.MODEL`: 通用默认模型
    pub fn from_provider(provider: &Provider) -> Self {
        let env = provider.settings_config.get("env");

        // 解析显式映射表
        let explicit_mapping = provider
            .settings_config
            .get("model_mapping")
            .and_then(|m| m.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| {
                        v.as_str()
                            .filter(|s| !s.is_empty())
                            .map(|s| (k.clone(), s.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            explicit_mapping,
            haiku_model: env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_HAIKU_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            sonnet_model: env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_SONNET_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            opus_model: env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_OPUS_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            anthropic_model: env
                .and_then(|e| e.get("ANTHROPIC_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            reasoning_model: env
                .and_then(|e| e.get("ANTHROPIC_REASONING_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            gemini_model: env
                .and_then(|e| e.get("GEMINI_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            openai_model: env
                .and_then(|e| e.get("OPENAI_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            default_model: env
                .and_then(|e| e.get("MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
        }
    }

    /// 检查是否配置了任何模型映射
    pub fn has_mapping(&self) -> bool {
        !self.explicit_mapping.is_empty()
            || self.haiku_model.is_some()
            || self.sonnet_model.is_some()
            || self.opus_model.is_some()
            || self.anthropic_model.is_some()
            || self.reasoning_model.is_some()
            || self.gemini_model.is_some()
            || self.openai_model.is_some()
            || self.default_model.is_some()
    }

    /// 根据原始模型名称获取映射后的模型
    ///
    /// 映射优先级：
    /// 1. 显式 model_mapping（精确匹配）
    /// 2. thinking 模式使用推理模型
    /// 3. 按模型类型匹配（haiku/opus/sonnet）
    /// 4. 按模型族匹配（claude/gemini/gpt）
    /// 5. 回退到默认模型
    /// 6. 无映射则保持原样
    pub fn map_model(&self, original_model: &str, has_thinking: bool) -> String {
        let model_lower = original_model.to_lowercase();

        // 1. 显式映射（精确匹配，优先级最高）
        if let Some(mapped) = self.explicit_mapping.get(original_model) {
            log::debug!("[ModelMapper] 显式映射: {original_model} → {mapped}");
            return mapped.clone();
        }

        // 2. thinking 模式优先使用推理模型
        if has_thinking {
            if let Some(ref m) = self.reasoning_model {
                log::debug!("[ModelMapper] 推理模型映射: {original_model} → {m}");
                return m.clone();
            }
        }

        // 3. 按模型类型匹配（Anthropic 模型族）
        if model_lower.contains("haiku") {
            if let Some(ref m) = self.haiku_model {
                log::debug!("[ModelMapper] Haiku 映射: {original_model} → {m}");
                return m.clone();
            }
        }
        if model_lower.contains("opus") {
            if let Some(ref m) = self.opus_model {
                log::debug!("[ModelMapper] Opus 映射: {original_model} → {m}");
                return m.clone();
            }
        }
        if model_lower.contains("sonnet") {
            if let Some(ref m) = self.sonnet_model {
                log::debug!("[ModelMapper] Sonnet 映射: {original_model} → {m}");
                return m.clone();
            }
        }

        // 4. 按模型族匹配
        // 4.1 Claude 模型族
        if model_lower.contains("claude") {
            if let Some(ref m) = self.anthropic_model {
                log::debug!("[ModelMapper] Anthropic 默认映射: {original_model} → {m}");
                return m.clone();
            }
        }
        // 4.2 Gemini 模型族
        if model_lower.contains("gemini") {
            if let Some(ref m) = self.gemini_model {
                log::debug!("[ModelMapper] Gemini 映射: {original_model} → {m}");
                return m.clone();
            }
        }
        // 4.3 OpenAI 模型族（gpt, o1, o3 等）
        if model_lower.contains("gpt")
            || model_lower.contains("o1")
            || model_lower.contains("o3")
        {
            if let Some(ref m) = self.openai_model {
                log::debug!("[ModelMapper] OpenAI 映射: {original_model} → {m}");
                return m.clone();
            }
        }

        // 5. 回退到默认模型（按优先级尝试）
        // 5.1 Anthropic 默认（因为主要用于 Claude 转发）
        if let Some(ref m) = self.anthropic_model {
            log::debug!("[ModelMapper] 回退 Anthropic 默认: {original_model} → {m}");
            return m.clone();
        }
        // 5.2 Gemini 默认
        if let Some(ref m) = self.gemini_model {
            log::debug!("[ModelMapper] 回退 Gemini 默认: {original_model} → {m}");
            return m.clone();
        }
        // 5.3 OpenAI 默认
        if let Some(ref m) = self.openai_model {
            log::debug!("[ModelMapper] 回退 OpenAI 默认: {original_model} → {m}");
            return m.clone();
        }
        // 5.4 通用默认
        if let Some(ref m) = self.default_model {
            log::debug!("[ModelMapper] 回退通用默认: {original_model} → {m}");
            return m.clone();
        }

        // 6. 无映射，保持原样
        log::debug!("[ModelMapper] 无映射，保持原样: {original_model}");
        original_model.to_string()
    }
}

/// 检测请求是否启用了 thinking 模式
pub fn has_thinking_enabled(body: &Value) -> bool {
    body.get("thinking")
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("type"))
        .and_then(|t| t.as_str())
        == Some("enabled")
}

/// 对请求体应用模型映射
///
/// 返回 (映射后的请求体, 原始模型名, 映射后模型名)
pub fn apply_model_mapping(
    mut body: Value,
    provider: &Provider,
) -> (Value, Option<String>, Option<String>) {
    let mapping = ModelMapping::from_provider(provider);

    // 如果没有配置映射，直接返回
    if !mapping.has_mapping() {
        let original = body.get("model").and_then(|m| m.as_str()).map(String::from);
        return (body, original, None);
    }

    // 提取原始模型名
    let original_model = body.get("model").and_then(|m| m.as_str()).map(String::from);

    if let Some(ref original) = original_model {
        let has_thinking = has_thinking_enabled(&body);
        let mapped = mapping.map_model(original, has_thinking);

        if mapped != *original {
            log::debug!("[ModelMapper] 模型映射: {original} → {mapped}");
            body["model"] = serde_json::json!(mapped);
            return (body, Some(original.clone()), Some(mapped));
        }
    }

    (body, original_model, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider_with_mapping() -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test".to_string(),
            settings_config: json!({
                "env": {
                    "ANTHROPIC_MODEL": "default-model",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "haiku-mapped",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-mapped",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-mapped",
                    "ANTHROPIC_REASONING_MODEL": "reasoning-model"
                }
            }),
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

    fn create_provider_without_mapping() -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test".to_string(),
            settings_config: json!({}),
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

    fn create_provider_with_reasoning_only() -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test".to_string(),
            settings_config: json!({
                "env": {
                    "ANTHROPIC_REASONING_MODEL": "reasoning-only-model"
                }
            }),
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
    fn test_sonnet_mapping() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "claude-sonnet-4-5-20250929"});
        let (result, original, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(original, Some("claude-sonnet-4-5-20250929".to_string()));
        assert_eq!(mapped, Some("sonnet-mapped".to_string()));
    }

    #[test]
    fn test_haiku_mapping() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "claude-haiku-4-5"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "haiku-mapped");
        assert_eq!(mapped, Some("haiku-mapped".to_string()));
    }

    #[test]
    fn test_opus_mapping() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "claude-opus-4-5"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "opus-mapped");
        assert_eq!(mapped, Some("opus-mapped".to_string()));
    }

    #[test]
    fn test_thinking_mode() {
        let provider = create_provider_with_mapping();
        let body = json!({
            "model": "claude-sonnet-4-5",
            "thinking": {"type": "enabled"}
        });
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "reasoning-model");
        assert_eq!(mapped, Some("reasoning-model".to_string()));
    }

    #[test]
    fn test_reasoning_only_mapping_in_thinking_mode() {
        let provider = create_provider_with_reasoning_only();
        let body = json!({
            "model": "claude-sonnet-4-5",
            "thinking": {"type": "enabled"}
        });
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "reasoning-only-model");
        assert_eq!(mapped, Some("reasoning-only-model".to_string()));
    }

    #[test]
    fn test_reasoning_only_mapping_does_not_affect_non_thinking() {
        let provider = create_provider_with_reasoning_only();
        let body = json!({
            "model": "claude-sonnet-4-5",
            "thinking": {"type": "disabled"}
        });
        let (result, original, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "claude-sonnet-4-5");
        assert_eq!(original, Some("claude-sonnet-4-5".to_string()));
        assert!(mapped.is_none());
    }

    #[test]
    fn test_thinking_disabled() {
        let provider = create_provider_with_mapping();
        let body = json!({
            "model": "claude-sonnet-4-5",
            "thinking": {"type": "disabled"}
        });
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(mapped, Some("sonnet-mapped".to_string()));
    }

    #[test]
    fn test_unknown_model_uses_default() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "some-unknown-model"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "default-model");
        assert_eq!(mapped, Some("default-model".to_string()));
    }

    #[test]
    fn test_no_mapping_configured() {
        let provider = create_provider_without_mapping();
        let body = json!({"model": "claude-sonnet-4-5"});
        let (result, original, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "claude-sonnet-4-5");
        assert_eq!(original, Some("claude-sonnet-4-5".to_string()));
        assert!(mapped.is_none());
    }

    #[test]
    fn test_case_insensitive() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "Claude-SONNET-4-5"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(mapped, Some("sonnet-mapped".to_string()));
    }
}
