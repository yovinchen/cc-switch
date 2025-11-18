use std::str::FromStr;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Local;
use log::{error, info, warn};
use serde_json::{json, Map, Value};
use toml::Value as TomlValue;
use uuid::Uuid;

use crate::app_config::AppType;
use crate::error::AppError;
use crate::models::deeplink::ConfigImportRequest;
use crate::provider::Provider;
use crate::store::AppState;

use super::ProviderService;

/// 深链接 Service：处理完整配置导入。
/// - 支持 Claude settings.json / Gemini config.json / Codex config.toml
/// - 统一的安全校验：Base64 解码、大小限制、危险字段过滤
pub struct DeepLinkService;

impl DeepLinkService {
    pub const MAX_CONFIG_SIZE: usize = 100 * 1024; // 100KB
    const DANGEROUS_FIELDS: [&'static str; 3] = ["__proto__", "constructor", "prototype"];

    /// 处理配置导入请求：解码、校验、按应用解析、持久化 Provider。
    pub fn handle_config_import(
        state: &AppState,
        request: ConfigImportRequest,
    ) -> Result<Provider, AppError> {
        info!(
            "Handling deep link config import: app='{}', format_hint={:?}",
            request.app, request.format
        );

        let raw = BASE64.decode(request.data.as_bytes()).map_err(|err| {
            error!("Config import Base64 decode failed: {err}");
            AppError::InvalidInput(format!("Base64 解码失败: {err}"))
        })?;

        let content = String::from_utf8(raw).map_err(|err| {
            error!("Config import payload is not valid UTF-8: {err}");
            AppError::InvalidInput(format!("配置内容必须为 UTF-8 字符串: {err}"))
        })?;

        Self::validate_config_size(&content)?;

        let app_type = AppType::from_str(&request.app).map_err(|err| {
            error!("Unsupported app '{}' for config import: {err}", request.app);
            err
        })?;

        if let Some(format_hint) = request.format.as_deref() {
            let expected = match app_type {
                AppType::Codex => "toml",
                _ => "json",
            };
            if format_hint != expected {
                error!("Config import format mismatch: expected {expected}, got {format_hint}");
                return Err(AppError::InvalidInput(format!(
                    "配置格式与应用类型不匹配，期望 {expected} 实际 {format_hint}"
                )));
            }
        }

        let provider = match app_type.clone() {
            AppType::Claude => Self::import_claude_config(&content)?,
            AppType::Gemini => Self::import_gemini_config(&content)?,
            AppType::Codex => Self::import_codex_config(&content)?,
        };

        let _ = ProviderService::add(state, app_type, provider.clone())?;

        info!(
            "Config import succeeded: provider {} ({})",
            provider.id, provider.name
        );

        Ok(provider)
    }

    /// 解析 Claude settings.json，生成 Provider。
    /// 会校验 env.ANTHROPIC_AUTH_TOKEN / ANTHROPIC_BASE_URL，并过滤危险字段。
    pub fn import_claude_config(content: &str) -> Result<Provider, AppError> {
        info!("Parsing Claude settings payload ({} bytes)", content.len());

        let mut config: Value = serde_json::from_str(content).map_err(|err| {
            error!("Invalid Claude settings JSON: {err}");
            AppError::InvalidInput(format!("Claude 配置解析失败: {err}"))
        })?;

        if !config.is_object() {
            error!("Claude config root is not an object");
            return Err(AppError::InvalidInput(
                "Claude 配置必须是 JSON 对象".to_string(),
            ));
        }

        Self::sanitize_config(&mut config)?;

        let env = config
            .get_mut("env")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                error!("Claude config missing env object");
                AppError::InvalidInput("Claude 配置缺少 env 字段".to_string())
            })?;

        let api_key = env
            .get("ANTHROPIC_AUTH_TOKEN")
            .or_else(|| env.get("ANTHROPIC_API_KEY"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                error!("Claude config missing ANTHROPIC_AUTH_TOKEN");
                AppError::InvalidInput("Claude 配置缺少 ANTHROPIC_AUTH_TOKEN".to_string())
            })?;

        let base_url = env
            .get("ANTHROPIC_BASE_URL")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                error!("Claude config missing ANTHROPIC_BASE_URL");
                AppError::InvalidInput("Claude 配置缺少 ANTHROPIC_BASE_URL".to_string())
            })?;

        info!("Claude config extracted base URL: {base_url}");
        info!("Claude config token length: {}", api_key.len());

        Ok(Self::build_provider(config))
    }

    /// 解析 Gemini config.json，生成 Provider。
    /// 支持根级字段 apiKey/baseURL/model 或 env.GEMINI_API_KEY/GOOGLE_GEMINI_BASE_URL。
    pub fn import_gemini_config(content: &str) -> Result<Provider, AppError> {
        info!("Parsing Gemini config payload ({} bytes)", content.len());

        let mut config: Value = serde_json::from_str(content).map_err(|err| {
            error!("Invalid Gemini config JSON: {err}");
            AppError::InvalidInput(format!("Gemini 配置解析失败: {err}"))
        })?;

        if !config.is_object() {
            error!("Gemini config root is not an object");
            return Err(AppError::InvalidInput(
                "Gemini 配置必须是 JSON 对象".to_string(),
            ));
        }

        Self::sanitize_config(&mut config)?;

        let api_key = config
            .get("apiKey")
            .and_then(Value::as_str)
            .or_else(|| {
                config
                    .pointer("/env/GEMINI_API_KEY")
                    .and_then(Value::as_str)
            })
            .ok_or_else(|| {
                error!("Gemini config missing apiKey");
                AppError::InvalidInput("Gemini 配置缺少 apiKey 字段".to_string())
            })?
            .to_string();

        let base_url = config
            .get("baseURL")
            .and_then(Value::as_str)
            .or_else(|| {
                config
                    .pointer("/env/GOOGLE_GEMINI_BASE_URL")
                    .and_then(Value::as_str)
            })
            .ok_or_else(|| {
                error!("Gemini config missing baseURL");
                AppError::InvalidInput("Gemini 配置缺少 baseURL 字段".to_string())
            })?
            .to_string();

        let model = config
            .get("model")
            .and_then(Value::as_str)
            .or_else(|| {
                config
                    .pointer("/env/GOOGLE_GEMINI_MODEL")
                    .and_then(Value::as_str)
            })
            .map(|value| value.to_string());

        info!("Gemini config extracted base URL: {base_url}");

        // 将 env 合并回配置，保证后续写入 .env 时字段齐全
        let env_value = config
            .as_object_mut()
            .expect("validated object")
            .entry("env")
            .or_insert_with(|| Value::Object(Map::new()));

        let env_map = env_value
            .as_object_mut()
            .ok_or_else(|| AppError::InvalidInput("Gemini 配置 env 必须是对象".to_string()))?;

        env_map.insert("GEMINI_API_KEY".to_string(), Value::String(api_key.clone()));
        env_map.insert(
            "GOOGLE_GEMINI_BASE_URL".to_string(),
            Value::String(base_url.clone()),
        );
        if let Some(model_value) = model {
            env_map.insert(
                "GOOGLE_GEMINI_MODEL".to_string(),
                Value::String(model_value),
            );
        }

        Ok(Self::build_provider(config))
    }

    /// 解析 Codex config.toml，生成 Provider。
    /// 读取 [api] 段的 base_url 与 model，并保留完整 TOML 文本。
    pub fn import_codex_config(content: &str) -> Result<Provider, AppError> {
        info!("Parsing Codex config payload ({} bytes)", content.len());

        let config: TomlValue = toml::from_str(content).map_err(|err| {
            error!("Invalid Codex config TOML: {err}");
            AppError::InvalidInput(format!("Codex 配置解析失败: {err}"))
        })?;

        let table = config.as_table().ok_or_else(|| {
            error!("Codex config root is not a table");
            AppError::InvalidInput("Codex 配置必须是 TOML 表".to_string())
        })?;

        let api_table = table.get("api").and_then(|v| v.as_table()).ok_or_else(|| {
            error!("Codex config missing [api] section");
            AppError::InvalidInput("Codex 配置缺少 [api] 节".to_string())
        })?;

        let base_url = api_table
            .get("base_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                error!("Codex config missing api.base_url");
                AppError::InvalidInput("Codex 配置缺少 api.base_url".to_string())
            })?;

        let model = api_table
            .get("model")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);

        info!("Codex config extracted base URL: {base_url}");

        let mut settings = Map::new();
        settings.insert(
            "auth".to_string(),
            json!({ "OPENAI_API_KEY": "" }), // 由用户后续填写
        );
        settings.insert("config".to_string(), Value::String(content.to_string()));
        settings.insert(
            "parsedConfig".to_string(),
            serde_json::to_value(&config).map_err(|err| {
                error!("Failed to convert Codex TOML to JSON: {err}");
                AppError::InvalidInput(format!("Codex 配置转换失败: {err}"))
            })?,
        );
        if let Some(model_value) = model {
            settings.insert("defaultModel".to_string(), Value::String(model_value));
        }

        Ok(Self::build_provider(Value::Object(settings)))
    }

    /// 配置字符串大小校验（上限 100KB）
    fn validate_config_size(content: &str) -> Result<(), AppError> {
        let size = content.len();
        if size > Self::MAX_CONFIG_SIZE {
            error!(
                "Config payload too large: {size} bytes (limit {} bytes)",
                Self::MAX_CONFIG_SIZE
            );
            return Err(AppError::InvalidInput(format!(
                "配置内容过大（最大 {}KB）",
                Self::MAX_CONFIG_SIZE / 1024
            )));
        }
        Ok(())
    }

    /// 移除原型污染相关字段，递归处理对象/数组。
    fn sanitize_config(value: &mut Value) -> Result<(), AppError> {
        match value {
            Value::Object(map) => {
                for field in Self::DANGEROUS_FIELDS {
                    if map.remove(field).is_some() {
                        warn!("Removed dangerous field '{field}' from config");
                    }
                }
                for child in map.values_mut() {
                    Self::sanitize_config(child)?;
                }
            }
            Value::Array(items) => {
                for item in items {
                    Self::sanitize_config(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// 构造 Provider，使用 UUID 生成唯一 ID，并添加导入备注。
    fn build_provider(settings_config: Value) -> Provider {
        let provider_id = Uuid::new_v4().to_string();
        let provider_name = format!("从配置导入 {}", Local::now().format("%Y-%m-%d %H:%M:%S"));

        let mut provider = Provider::with_id(provider_id, provider_name, settings_config, None);
        provider.notes = Some("通过深链接导入".to_string());
        provider
    }
}

#[cfg(test)]
mod tests {
    use super::DeepLinkService;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn validate_config_size_rejects_large_payload() {
        let payload = "x".repeat(DeepLinkService::MAX_CONFIG_SIZE + 1);
        let err = DeepLinkService::validate_config_size(&payload).unwrap_err();
        assert!(err.to_string().contains("过大"));
    }

    #[test]
    fn sanitize_config_removes_proto_fields() {
        let mut value = json!({
            "__proto__": "danger",
            "nested": {
                "constructor": "bad",
                "safe": true
            }
        });
        DeepLinkService::sanitize_config(&mut value).unwrap();
        assert!(value.get("__proto__").is_none());
        assert!(value.pointer("/nested/constructor").is_none());
    }

    #[test]
    fn import_claude_config_builds_provider() {
        let config = r#"{
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test",
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            }
        }"#;

        let provider = DeepLinkService::import_claude_config(config).unwrap();
        assert!(Uuid::parse_str(&provider.id).is_ok());
        let token = provider
            .settings_config
            .pointer("/env/ANTHROPIC_AUTH_TOKEN")
            .and_then(|v| v.as_str());
        assert_eq!(token, Some("sk-ant-test"));
        assert_eq!(provider.notes.as_deref(), Some("通过深链接导入"));
    }

    #[test]
    fn import_gemini_config_maps_env() {
        let config = r#"{
            "apiKey": "AIza-test",
            "baseURL": "https://generativelanguage.googleapis.com",
            "model": "gemini-pro"
        }"#;

        let provider = DeepLinkService::import_gemini_config(config).unwrap();
        let api_key = provider
            .settings_config
            .pointer("/env/GEMINI_API_KEY")
            .and_then(|v| v.as_str());
        assert_eq!(api_key, Some("AIza-test"));
        assert_eq!(provider.notes.as_deref(), Some("通过深链接导入"));
    }

    #[test]
    fn import_codex_config_converts_toml() {
        let config = r#"
        [api]
        base_url = "https://api.openai.com/v1"
        model = "gpt-5"
        "#;

        let provider = DeepLinkService::import_codex_config(config).unwrap();
        assert!(Uuid::parse_str(&provider.id).is_ok());
        assert!(provider.settings_config.get("parsedConfig").is_some());
        assert!(provider
            .settings_config
            .get("config")
            .and_then(|v| v.as_str())
            .unwrap()
            .contains("base_url"));
    }
}
