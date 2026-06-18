//! Prompt cache breakpoint injection.

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheInjectionConfig {
    pub enabled: bool,
    pub ttl: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheInjectionReport {
    pub enabled: bool,
    pub existing: usize,
    pub ttl: String,
    pub injected: Vec<String>,
    pub budget_exhausted: bool,
}

pub fn inject_cache_control(
    body: &mut Value,
    config: &CacheInjectionConfig,
) -> CacheInjectionReport {
    let mut report = CacheInjectionReport {
        enabled: config.enabled,
        ttl: config.ttl.clone(),
        ..CacheInjectionReport::default()
    };

    if !config.enabled {
        return report;
    }

    let existing = count_existing(body);
    report.existing = existing;

    upgrade_existing_ttl(body, &config.ttl);

    let mut budget = 4_usize.saturating_sub(existing);
    if budget == 0 {
        report.budget_exhausted = true;
        return report;
    }

    if budget > 0 {
        if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) {
            if let Some(last) = tools.last_mut() {
                if last.get("cache_control").is_none() {
                    if let Some(object) = last.as_object_mut() {
                        object.insert(
                            "cache_control".to_string(),
                            make_cache_control(&config.ttl),
                        );
                    }
                    budget -= 1;
                    report.injected.push("tools".to_string());
                }
            }
        }
    }

    if budget > 0 {
        if let Some(text) = body
            .get("system")
            .and_then(Value::as_str)
            .map(ToString::to_string)
        {
            body["system"] = json!([{"type": "text", "text": text}]);
        }

        if let Some(system) = body.get_mut("system").and_then(Value::as_array_mut) {
            if let Some(last) = system.last_mut() {
                if last.get("cache_control").is_none() {
                    if let Some(object) = last.as_object_mut() {
                        object.insert(
                            "cache_control".to_string(),
                            make_cache_control(&config.ttl),
                        );
                    }
                    budget -= 1;
                    report.injected.push("system".to_string());
                }
            }
        }
    }

    if budget > 0 {
        if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
            if let Some(assistant_msg) = messages
                .iter_mut()
                .rev()
                .find(|m| m.get("role").and_then(Value::as_str) == Some("assistant"))
            {
                if let Some(content) = assistant_msg
                    .get_mut("content")
                    .and_then(Value::as_array_mut)
                {
                    if let Some(block) = content.iter_mut().rev().find(|b| {
                        let block_type = b.get("type").and_then(Value::as_str).unwrap_or("");
                        block_type != "thinking" && block_type != "redacted_thinking"
                    }) {
                        if block.get("cache_control").is_none() {
                            if let Some(object) = block.as_object_mut() {
                                object.insert(
                                    "cache_control".to_string(),
                                    make_cache_control(&config.ttl),
                                );
                            }
                            report.injected.push("msgs".to_string());
                        }
                    }
                }
            }
        }
    }

    report
}

fn make_cache_control(ttl: &str) -> Value {
    if ttl == "5m" {
        json!({"type": "ephemeral"})
    } else {
        json!({"type": "ephemeral", "ttl": ttl})
    }
}

fn count_existing(body: &Value) -> usize {
    let mut count = 0;

    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        count += tools
            .iter()
            .filter(|tool| tool.get("cache_control").is_some())
            .count();
    }

    if let Some(system) = body.get("system").and_then(Value::as_array) {
        count += system
            .iter()
            .filter(|block| block.get("cache_control").is_some())
            .count();
    }

    if let Some(messages) = body.get("messages").and_then(Value::as_array) {
        for msg in messages {
            if let Some(content) = msg.get("content").and_then(Value::as_array) {
                count += content
                    .iter()
                    .filter(|block| block.get("cache_control").is_some())
                    .count();
            }
        }
    }

    count
}

fn upgrade_existing_ttl(body: &mut Value, ttl: &str) {
    let upgrade = |value: &mut Value| {
        if let Some(cache_control) = value.get_mut("cache_control").and_then(Value::as_object_mut)
        {
            if ttl == "5m" {
                cache_control.remove("ttl");
            } else {
                cache_control.insert("ttl".to_string(), json!(ttl));
            }
        }
    };

    if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            upgrade(tool);
        }
    }

    if let Some(system) = body.get_mut("system").and_then(Value::as_array_mut) {
        for block in system {
            upgrade(block);
        }
    }

    if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
        for msg in messages {
            if let Some(content) = msg.get_mut("content").and_then(Value::as_array_mut) {
                for block in content {
                    upgrade(block);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn default_config() -> CacheInjectionConfig {
        CacheInjectionConfig {
            enabled: true,
            ttl: "1h".to_string(),
        }
    }

    #[test]
    fn empty_body_has_no_injection() {
        let mut body = json!({"model": "test", "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]});
        let original = body.clone();

        let report = inject_cache_control(&mut body, &default_config());

        assert_eq!(body, original);
        assert!(report.injected.is_empty());
        assert_eq!(report.existing, 0);
    }

    #[test]
    fn injects_three_breakpoints() {
        let mut body = json!({
            "model": "test",
            "tools": [{"name": "tool1"}, {"name": "tool2"}],
            "system": [{"type": "text", "text": "sys prompt"}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "hello"}
                ]}
            ]
        });

        let report = inject_cache_control(&mut body, &default_config());

        assert_eq!(report.injected, vec!["tools", "system", "msgs"]);
        assert!(body["tools"][1].get("cache_control").is_some());
        assert_eq!(body["tools"][1]["cache_control"]["ttl"], "1h");
        assert!(body["system"][0].get("cache_control").is_some());
        assert!(body["messages"][1]["content"][0]
            .get("cache_control")
            .is_some());
    }

    #[test]
    fn existing_four_breakpoints_only_upgrades_ttl() {
        let mut body = json!({
            "model": "test",
            "tools": [
                {"name": "t1", "cache_control": {"type": "ephemeral", "ttl": "5m"}},
                {"name": "t2", "cache_control": {"type": "ephemeral", "ttl": "5m"}}
            ],
            "system": [
                {"type": "text", "text": "sys", "cache_control": {"type": "ephemeral", "ttl": "5m"}}
            ],
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "text", "text": "ok", "cache_control": {"type": "ephemeral", "ttl": "5m"}}
                ]}
            ]
        });

        let report = inject_cache_control(&mut body, &default_config());

        assert!(report.budget_exhausted);
        assert_eq!(report.existing, 4);
        assert!(report.injected.is_empty());
        assert_eq!(body["tools"][0]["cache_control"]["ttl"], "1h");
        assert_eq!(body["tools"][1]["cache_control"]["ttl"], "1h");
        assert_eq!(body["system"][0]["cache_control"]["ttl"], "1h");
        assert_eq!(
            body["messages"][0]["content"][0]["cache_control"]["ttl"],
            "1h"
        );
    }

    #[test]
    fn existing_two_injects_two_more() {
        let mut body = json!({
            "model": "test",
            "tools": [
                {"name": "t1", "cache_control": {"type": "ephemeral"}},
                {"name": "t2", "cache_control": {"type": "ephemeral"}}
            ],
            "system": [{"type": "text", "text": "sys"}],
            "messages": [
                {"role": "assistant", "content": [{"type": "text", "text": "ok"}]}
            ]
        });

        let report = inject_cache_control(&mut body, &default_config());

        assert_eq!(report.existing, 2);
        assert_eq!(report.injected, vec!["system", "msgs"]);
        assert!(body["system"][0].get("cache_control").is_some());
        assert!(body["messages"][0]["content"][0]
            .get("cache_control")
            .is_some());
    }

    #[test]
    fn system_string_converted_to_array() {
        let mut body = json!({
            "model": "test",
            "system": "You are a helpful assistant",
            "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]
        });

        inject_cache_control(&mut body, &default_config());

        assert!(body["system"].is_array());
        let system = body["system"].as_array().unwrap();
        assert_eq!(system.len(), 1);
        assert_eq!(system[0]["type"], "text");
        assert_eq!(system[0]["text"], "You are a helpful assistant");
        assert!(system[0].get("cache_control").is_some());
    }

    #[test]
    fn ttl_5m_has_no_ttl_field() {
        let config = CacheInjectionConfig {
            ttl: "5m".to_string(),
            ..default_config()
        };
        let mut body = json!({
            "model": "test",
            "tools": [{"name": "tool1"}],
            "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]
        });

        inject_cache_control(&mut body, &config);

        let cache_control = &body["tools"][0]["cache_control"];
        assert_eq!(cache_control["type"], "ephemeral");
        assert!(cache_control.get("ttl").is_none() || cache_control["ttl"].is_null());
    }

    #[test]
    fn disabled_config_leaves_body_unchanged() {
        let config = CacheInjectionConfig {
            enabled: false,
            ..default_config()
        };
        let mut body = json!({
            "model": "test",
            "tools": [{"name": "tool1"}],
            "system": [{"type": "text", "text": "sys"}],
            "messages": [{"role": "assistant", "content": [{"type": "text", "text": "ok"}]}]
        });
        let original = body.clone();

        let report = inject_cache_control(&mut body, &config);

        assert_eq!(body, original);
        assert!(!report.enabled);
        assert!(report.injected.is_empty());
    }

    #[test]
    fn skips_thinking_blocks_in_assistant() {
        let mut body = json!({
            "model": "test",
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "hmm"},
                    {"type": "text", "text": "result"},
                    {"type": "redacted_thinking", "data": "xxx"}
                ]}
            ]
        });

        let report = inject_cache_control(&mut body, &default_config());

        assert_eq!(report.injected, vec!["msgs"]);
        assert!(body["messages"][0]["content"][1]
            .get("cache_control")
            .is_some());
        assert!(body["messages"][0]["content"][0]
            .get("cache_control")
            .is_none());
        assert!(body["messages"][0]["content"][2]
            .get("cache_control")
            .is_none());
    }
}
