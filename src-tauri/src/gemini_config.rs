use crate::config::write_text_file;
use crate::error::AppError;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// 获取 Gemini 配置目录路径（支持设置覆盖）
pub fn get_gemini_dir() -> PathBuf {
    if let Some(custom) = crate::settings::get_gemini_override_dir() {
        return custom;
    }

    dirs::home_dir()
        .expect("无法获取用户主目录")
        .join(".gemini")
}

/// 获取 Gemini .env 文件路径
pub fn get_gemini_env_path() -> PathBuf {
    get_gemini_dir().join(".env")
}

/// 解析 .env 文件内容为键值对
pub fn parse_env_file(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        // 跳过空行和注释
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // 解析 KEY=VALUE
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            map.insert(key, value);
        }
    }

    map
}

/// 将键值对序列化为 .env 格式
pub fn serialize_env_file(map: &HashMap<String, String>) -> String {
    let mut lines = Vec::new();

    // 按键排序以保证输出稳定
    let mut keys: Vec<_> = map.keys().collect();
    keys.sort();

    for key in keys {
        if let Some(value) = map.get(key) {
            lines.push(format!("{}={}", key, value));
        }
    }

    lines.join("\n")
}

/// 读取 Gemini .env 文件
pub fn read_gemini_env() -> Result<HashMap<String, String>, AppError> {
    let path = get_gemini_env_path();

    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| AppError::io(&path, e))?;

    Ok(parse_env_file(&content))
}

/// 写入 Gemini .env 文件（原子操作）
pub fn write_gemini_env_atomic(map: &HashMap<String, String>) -> Result<(), AppError> {
    let path = get_gemini_env_path();

    // 确保目录存在
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::io(parent, e))?;
    }

    let content = serialize_env_file(map);
    write_text_file(&path, &content)?;

    Ok(())
}

/// 从 .env 格式转换为 Provider.settings_config (JSON Value)
pub fn env_to_json(env_map: &HashMap<String, String>) -> Value {
    let mut json_map = serde_json::Map::new();

    for (key, value) in env_map {
        json_map.insert(key.clone(), Value::String(value.clone()));
    }

    serde_json::json!({ "env": json_map })
}

/// 从 Provider.settings_config (JSON Value) 提取 .env 格式
pub fn json_to_env(settings: &Value) -> Result<HashMap<String, String>, AppError> {
    let mut env_map = HashMap::new();

    if let Some(env_obj) = settings.get("env").and_then(|v| v.as_object()) {
        for (key, value) in env_obj {
            if let Some(val_str) = value.as_str() {
                env_map.insert(key.clone(), val_str.to_string());
            }
        }
    }

    Ok(env_map)
}

/// 验证 Gemini 配置的必需字段
pub fn validate_gemini_settings(settings: &Value) -> Result<(), AppError> {
    let env_map = json_to_env(settings)?;

    // 检查必需字段
    if !env_map.contains_key("GEMINI_API_KEY") {
        return Err(AppError::localized(
            "gemini.validation.missing_api_key",
            "Gemini 配置缺少必需字段: GEMINI_API_KEY",
            "Gemini config missing required field: GEMINI_API_KEY",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_file() {
        let content = r#"
# Comment line
GOOGLE_GEMINI_BASE_URL=https://example.com
GEMINI_API_KEY=sk-test123
GEMINI_MODEL=gemini-2.5-pro

# Another comment
"#;

        let map = parse_env_file(content);

        assert_eq!(map.len(), 3);
        assert_eq!(map.get("GOOGLE_GEMINI_BASE_URL"), Some(&"https://example.com".to_string()));
        assert_eq!(map.get("GEMINI_API_KEY"), Some(&"sk-test123".to_string()));
        assert_eq!(map.get("GEMINI_MODEL"), Some(&"gemini-2.5-pro".to_string()));
    }

    #[test]
    fn test_serialize_env_file() {
        let mut map = HashMap::new();
        map.insert("GEMINI_API_KEY".to_string(), "sk-test".to_string());
        map.insert("GEMINI_MODEL".to_string(), "gemini-2.5-pro".to_string());

        let content = serialize_env_file(&map);

        assert!(content.contains("GEMINI_API_KEY=sk-test"));
        assert!(content.contains("GEMINI_MODEL=gemini-2.5-pro"));
    }

    #[test]
    fn test_env_json_conversion() {
        let mut env_map = HashMap::new();
        env_map.insert("GEMINI_API_KEY".to_string(), "test-key".to_string());

        let json = env_to_json(&env_map);
        let converted = json_to_env(&json).unwrap();

        assert_eq!(converted.get("GEMINI_API_KEY"), Some(&"test-key".to_string()));
    }
}
