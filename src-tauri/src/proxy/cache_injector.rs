//! Cache 断点注入器
//!
//! 核心注入规则位于 proxy-core；本模块保留旧 host API 和日志格式。

use super::types::OptimizerConfig;
use serde_json::Value;

fn cache_injection_config(config: &OptimizerConfig) -> crate::proxy_core::CacheInjectionConfig {
    crate::proxy_core::CacheInjectionConfig {
        enabled: config.cache_injection,
        ttl: config.cache_ttl.clone(),
    }
}

/// 在请求体关键位置注入 cache_control 断点
pub fn inject(body: &mut Value, config: &OptimizerConfig) {
    let report = crate::proxy_core::inject_cache_control(body, &cache_injection_config(config));
    if !report.enabled {
        return;
    }

    if report.budget_exhausted {
        if report.existing > 0 {
            log::info!(
                "[OPT] cache: ttl-upgrade({}->{},existing={})",
                report.existing,
                report.ttl,
                report.existing
            );
        } else {
            log::info!("[OPT] cache: no-op(existing={})", report.existing);
        }
        return;
    }

    log::info!(
        "[OPT] cache: {}bp({},{},pre={})",
        report.injected.len(),
        report.injected.join("+"),
        report.ttl,
        report.existing,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn default_config() -> OptimizerConfig {
        OptimizerConfig {
            enabled: true,
            thinking_optimizer: true,
            cache_injection: true,
            cache_ttl: "1h".to_string(),
        }
    }

    #[test]
    fn wrapper_injects_cache_breakpoints() {
        let mut body = json!({
            "model": "test",
            "tools": [{"name": "tool1"}, {"name": "tool2"}],
            "system": [{"type": "text", "text": "sys prompt"}],
            "messages": [
                {"role": "assistant", "content": [{"type": "text", "text": "hello"}]}
            ]
        });

        inject(&mut body, &default_config());

        assert!(body["tools"][1].get("cache_control").is_some());
        assert!(body["system"][0].get("cache_control").is_some());
        assert!(body["messages"][0]["content"][0]
            .get("cache_control")
            .is_some());
    }

    #[test]
    fn wrapper_respects_disabled_cache_injection() {
        let config = OptimizerConfig {
            cache_injection: false,
            ..default_config()
        };
        let mut body = json!({
            "model": "test",
            "tools": [{"name": "tool1"}],
            "system": [{"type": "text", "text": "sys"}],
            "messages": [{"role": "assistant", "content": [{"type": "text", "text": "ok"}]}]
        });
        let original = body.clone();

        inject(&mut body, &config);

        assert_eq!(body, original);
    }
}
