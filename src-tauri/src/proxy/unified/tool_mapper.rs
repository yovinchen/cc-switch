//! 工具名称映射器
//!
//! 解决不同 API 对工具名称长度限制的问题。
//!
//! ## 背景
//!
//! - OpenAI/Codex: 工具名称限制 64 字符
//! - Claude Code: MCP 工具名称可能很长（如 `mcp__filesystem__read_file`）
//!
//! ## 解决方案
//!
//! 提供工具名称的缩短和恢复机制：
//! - `shorten()`: 将长名称映射为短名称（用于发送请求）
//! - `restore()`: 将短名称恢复为原名称（用于解析响应）

use std::collections::HashMap;
use std::sync::RwLock;

/// 工具名称最大长度（OpenAI 限制）
const MAX_TOOL_NAME_LENGTH: usize = 64;

/// 短名称前缀
const SHORT_PREFIX: &str = "t_";

/// 工具名称映射器
///
/// 线程安全的工具名称缩短/恢复器。
#[derive(Debug, Default)]
pub struct ToolNameMapper {
    /// 原名称 → 短名称
    forward: RwLock<HashMap<String, String>>,
    /// 短名称 → 原名称
    reverse: RwLock<HashMap<String, String>>,
    /// 计数器，用于生成唯一短名称
    counter: RwLock<u32>,
}

impl ToolNameMapper {
    /// 创建新的映射器
    pub fn new() -> Self {
        Self::default()
    }

    /// 缩短工具名称（如果需要）
    ///
    /// 如果名称长度 ≤ 64，直接返回原名称。
    /// 如果名称长度 > 64，生成短名称并记录映射。
    pub fn shorten(&self, name: &str) -> String {
        if name.len() <= MAX_TOOL_NAME_LENGTH {
            return name.to_string();
        }

        // 检查是否已有映射
        {
            let forward = self.forward.read().unwrap();
            if let Some(short) = forward.get(name) {
                return short.clone();
            }
        }

        // 生成新的短名称
        let short_name = {
            let mut counter = self.counter.write().unwrap();
            *counter += 1;
            format!("{}{}", SHORT_PREFIX, *counter)
        };

        // 记录映射
        {
            let mut forward = self.forward.write().unwrap();
            let mut reverse = self.reverse.write().unwrap();
            forward.insert(name.to_string(), short_name.clone());
            reverse.insert(short_name.clone(), name.to_string());
        }

        short_name
    }

    /// 恢复工具名称
    ///
    /// 如果是短名称，恢复为原名称。
    /// 如果不是短名称，直接返回。
    pub fn restore(&self, short_name: &str) -> String {
        if !short_name.starts_with(SHORT_PREFIX) {
            return short_name.to_string();
        }

        let reverse = self.reverse.read().unwrap();
        reverse
            .get(short_name)
            .cloned()
            .unwrap_or_else(|| short_name.to_string())
    }

    /// 批量缩短工具名称
    pub fn shorten_all(&self, names: &[String]) -> Vec<String> {
        names.iter().map(|n| self.shorten(n)).collect()
    }

    /// 批量恢复工具名称
    pub fn restore_all(&self, short_names: &[String]) -> Vec<String> {
        short_names.iter().map(|n| self.restore(n)).collect()
    }

    /// 清除所有映射
    pub fn clear(&self) {
        let mut forward = self.forward.write().unwrap();
        let mut reverse = self.reverse.write().unwrap();
        let mut counter = self.counter.write().unwrap();
        forward.clear();
        reverse.clear();
        *counter = 0;
    }

    /// 获取当前映射数量
    pub fn len(&self) -> usize {
        let forward = self.forward.read().unwrap();
        forward.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 工具定义处理
///
/// 提供工具定义的名称映射功能。
pub mod tools {
    use super::*;
    use serde_json::Value;

    /// 缩短工具定义中的名称
    ///
    /// 处理 OpenAI 格式的 tools 数组
    pub fn shorten_tool_definitions(tools: &mut Value, mapper: &ToolNameMapper) {
        if let Some(arr) = tools.as_array_mut() {
            for tool in arr {
                if let Some(func) = tool.get_mut("function") {
                    if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                        let short_name = mapper.shorten(name);
                        if short_name != name {
                            func["name"] = Value::String(short_name);
                        }
                    }
                }
            }
        }
    }

    /// 恢复工具调用中的名称
    ///
    /// 处理 OpenAI 格式的 tool_calls 数组
    pub fn restore_tool_calls(tool_calls: &mut Value, mapper: &ToolNameMapper) {
        if let Some(arr) = tool_calls.as_array_mut() {
            for tc in arr {
                if let Some(func) = tc.get_mut("function") {
                    if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                        let original_name = mapper.restore(name);
                        if original_name != name {
                            func["name"] = Value::String(original_name);
                        }
                    }
                }
            }
        }
    }

    /// 缩短 Anthropic 工具定义中的名称
    pub fn shorten_anthropic_tools(tools: &mut Value, mapper: &ToolNameMapper) {
        if let Some(arr) = tools.as_array_mut() {
            for tool in arr {
                if let Some(name) = tool.get("name").and_then(|n| n.as_str()) {
                    let short_name = mapper.shorten(name);
                    if short_name != name {
                        tool["name"] = Value::String(short_name);
                    }
                }
            }
        }
    }

    /// 恢复 Anthropic tool_use 中的名称
    pub fn restore_anthropic_tool_use(content: &mut Value, mapper: &ToolNameMapper) {
        if let Some(arr) = content.as_array_mut() {
            for item in arr {
                if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                        let original_name = mapper.restore(name);
                        if original_name != name {
                            item["name"] = Value::String(original_name);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_short_name_not_changed() {
        let mapper = ToolNameMapper::new();
        let name = "get_weather";
        assert_eq!(mapper.shorten(name), "get_weather");
        assert_eq!(mapper.restore("get_weather"), "get_weather");
    }

    #[test]
    fn test_long_name_shortened() {
        let mapper = ToolNameMapper::new();
        let long_name = "a".repeat(100); // 100 字符，超过 64

        let short = mapper.shorten(&long_name);
        assert!(short.len() <= MAX_TOOL_NAME_LENGTH);
        assert!(short.starts_with(SHORT_PREFIX));

        // 恢复
        let restored = mapper.restore(&short);
        assert_eq!(restored, long_name);
    }

    #[test]
    fn test_consistent_mapping() {
        let mapper = ToolNameMapper::new();
        let long_name = "mcp__filesystem__read_file__with__very__long__path__name__component";

        // 多次缩短应该返回相同结果
        let short1 = mapper.shorten(long_name);
        let short2 = mapper.shorten(long_name);
        assert_eq!(short1, short2);
    }

    #[test]
    fn test_multiple_mappings() {
        let mapper = ToolNameMapper::new();
        let name1 = "a".repeat(100);
        let name2 = "b".repeat(100);

        let short1 = mapper.shorten(&name1);
        let short2 = mapper.shorten(&name2);

        // 不同的长名称应该有不同的短名称
        assert_ne!(short1, short2);

        // 都能正确恢复
        assert_eq!(mapper.restore(&short1), name1);
        assert_eq!(mapper.restore(&short2), name2);
    }

    #[test]
    fn test_clear() {
        let mapper = ToolNameMapper::new();
        let long_name = "a".repeat(100);

        mapper.shorten(&long_name);
        assert_eq!(mapper.len(), 1);

        mapper.clear();
        assert!(mapper.is_empty());
    }

    #[test]
    fn test_shorten_tool_definitions() {
        let mapper = ToolNameMapper::new();
        let long_name = "very_long_tool_name_that_exceeds_the_sixty_four_character_limit_for_openai";

        let mut tools = json!([
            {
                "type": "function",
                "function": {
                    "name": long_name,
                    "description": "A tool"
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "short_tool",
                    "description": "Another tool"
                }
            }
        ]);

        tools::shorten_tool_definitions(&mut tools, &mapper);

        // 长名称被缩短
        let shortened = tools[0]["function"]["name"].as_str().unwrap();
        assert!(shortened.starts_with(SHORT_PREFIX));
        assert!(shortened.len() <= MAX_TOOL_NAME_LENGTH);

        // 短名称保持不变
        assert_eq!(tools[1]["function"]["name"], "short_tool");
    }

    #[test]
    fn test_restore_tool_calls() {
        let mapper = ToolNameMapper::new();
        let long_name = "very_long_tool_name_that_exceeds_the_sixty_four_character_limit_for_openai";

        // 先缩短
        let short_name = mapper.shorten(long_name);

        let mut tool_calls = json!([
            {
                "id": "call_123",
                "type": "function",
                "function": {
                    "name": short_name,
                    "arguments": "{}"
                }
            }
        ]);

        tools::restore_tool_calls(&mut tool_calls, &mapper);

        // 名称被恢复
        assert_eq!(tool_calls[0]["function"]["name"], long_name);
    }

    #[test]
    fn test_shorten_anthropic_tools() {
        let mapper = ToolNameMapper::new();
        let long_name = "mcp__server__very_long_tool_name_that_exceeds_limit_of_sixty_four_chars";

        let mut tools = json!([
            {
                "name": long_name,
                "description": "A tool",
                "input_schema": {}
            }
        ]);

        tools::shorten_anthropic_tools(&mut tools, &mapper);

        let shortened = tools[0]["name"].as_str().unwrap();
        assert!(shortened.starts_with(SHORT_PREFIX));
    }

    #[test]
    fn test_restore_anthropic_tool_use() {
        let mapper = ToolNameMapper::new();
        let long_name = "mcp__server__very_long_tool_name_that_exceeds_limit_of_sixty_four_chars";
        let short_name = mapper.shorten(long_name);

        let mut content = json!([
            {
                "type": "text",
                "text": "Let me help you"
            },
            {
                "type": "tool_use",
                "id": "call_123",
                "name": short_name,
                "input": {}
            }
        ]);

        tools::restore_anthropic_tool_use(&mut content, &mapper);

        // 只有 tool_use 类型的被恢复
        assert_eq!(content[1]["name"], long_name);
        assert_eq!(content[0]["text"], "Let me help you"); // text 不受影响
    }
}
