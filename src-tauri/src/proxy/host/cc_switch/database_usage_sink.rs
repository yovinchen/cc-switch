//! Usage Logger - 记录 API 请求使用情况

use crate::database::{Database, PRICING_SOURCE_REQUEST, PRICING_SOURCE_RESPONSE};
use crate::error::AppError;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::UsageSink;
use crate::proxy_core::api::usage::{CostBreakdown, ModelPricing, TokenUsage, UsageRecord};
use crate::proxy_core_adapter::{
    log_usage_request_projection_warnings, usage_error, usage_pricing_config_lookup_from_record,
    usage_record_pricing_model, usage_record_to_request_log,
};
use crate::services::usage_stats::find_model_pricing_row;
use futures::future::BoxFuture;
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;

/// 请求日志
#[derive(Debug, Clone)]
pub struct RequestLog {
    pub request_id: String,
    pub provider_id: String,
    pub app_type: String,
    pub model: String,
    pub request_model: String,
    /// 写入时实际用于计价的模型名（pricing_model_source 解析后的结果）。
    /// 落库供回填使用：缺价行补价后必须按写入时的基准重算，而不是
    /// 用 model/request_model 猜——路由接管下三者可能各不相同。
    /// 错误行（未计价）为空字符串。
    pub pricing_model: String,
    pub usage: TokenUsage,
    pub cost: Option<CostBreakdown>,
    pub latency_ms: u64,
    pub first_token_ms: Option<u64>,
    pub status_code: u16,
    pub error_message: Option<String>,
    pub session_id: Option<String>,
    /// 供应商类型 (claude, claude_auth, codex, gemini, gemini_cli, openrouter)
    pub provider_type: Option<String>,
    /// Materialized proxy channel selected for this request, when routing used the extracted proxy module.
    pub channel_id: Option<String>,
    pub channel_name: Option<String>,
    pub route_group: Option<String>,
    /// 是否为流式请求
    pub is_streaming: bool,
    /// 成本倍数
    pub cost_multiplier: String,
}

/// 使用量记录器
pub struct UsageLogger<'a> {
    db: &'a Database,
}

impl<'a> UsageLogger<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 记录成功的请求
    pub fn log_request(&self, log: &RequestLog) -> Result<(), AppError> {
        let conn = crate::database::lock_conn!(self.db.conn);

        let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) =
            if let Some(cost) = &log.cost {
                (
                    cost.input_cost.to_string(),
                    cost.output_cost.to_string(),
                    cost.cache_read_cost.to_string(),
                    cost.cache_creation_cost.to_string(),
                    cost.total_cost.to_string(),
                )
            } else {
                (
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                )
            };

        let created_at = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT OR REPLACE INTO proxy_request_logs (
                request_id, provider_id, app_type, model, request_model, pricing_model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
                latency_ms, first_token_ms, status_code, error_message, session_id,
                provider_type, channel_id, channel_name, route_group, is_streaming, cost_multiplier, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)",
            rusqlite::params![
                log.request_id,
                log.provider_id,
                log.app_type,
                log.model,
                log.request_model,
                log.pricing_model,
                log.usage.input_tokens,
                log.usage.output_tokens,
                log.usage.cache_read_tokens,
                log.usage.cache_creation_tokens,
                input_cost,
                output_cost,
                cache_read_cost,
                cache_creation_cost,
                total_cost,
                log.latency_ms as i64,
                log.first_token_ms.map(|v| v as i64),
                log.status_code as i64,
                log.error_message,
                log.session_id,
                log.provider_type,
                log.channel_id,
                log.channel_name,
                log.route_group,
                log.is_streaming as i64,
                log.cost_multiplier,
                created_at,
            ],
        )
        .map_err(|e| AppError::Database(format!("记录请求日志失败: {e}")))?;

        // 通知前端使用统计有更新（200ms 防抖合并，不阻塞写入路径）
        crate::usage_events::notify_log_recorded();

        Ok(())
    }

    /// 获取模型定价
    pub fn get_model_pricing(&self, model_id: &str) -> Result<Option<ModelPricing>, AppError> {
        let conn = crate::database::lock_conn!(self.db.conn);
        let row = find_model_pricing_row(&conn, model_id)?;
        match row {
            Some((input, output, cache_read, cache_creation)) => {
                ModelPricing::from_strings(&input, &output, &cache_read, &cache_creation)
                    .map(Some)
                    .map_err(|e| AppError::Database(format!("解析定价数据失败: {e}")))
            }
            None => Ok(None),
        }
    }

    /// 获取有效的倍率与计费模式来源（供应商优先，未配置则回退全局默认）
    pub async fn resolve_pricing_config(
        &self,
        provider_id: &str,
        app_type: &str,
    ) -> (Decimal, String) {
        // Claude Desktop 网关没有独立的全局计费配置（proxy_config 的 CHECK 仅
        // 允许 claude/codex/gemini，前端也只暴露三项），全局默认继承 claude；
        // 供应商级 meta 覆盖仍按 claude-desktop 查找（providers 表按该 app_type 存）。
        let default_app_type = if app_type == "claude-desktop" {
            "claude"
        } else {
            app_type
        };
        let default_multiplier_raw =
            match self.db.get_default_cost_multiplier(default_app_type).await {
                Ok(value) => value,
                Err(e) => {
                    log::warn!("[USG-003] 获取默认倍率失败 (app_type={app_type}): {e}");
                    "1".to_string()
                }
            };
        let default_multiplier = match Decimal::from_str(&default_multiplier_raw) {
            Ok(value) => value,
            Err(e) => {
                log::warn!(
                    "[USG-003] 默认倍率解析失败 (app_type={app_type}): {default_multiplier_raw} - {e}"
                );
                Decimal::from(1)
            }
        };

        let default_pricing_source_raw =
            match self.db.get_pricing_model_source(default_app_type).await {
                Ok(value) => value,
                Err(e) => {
                    log::warn!("[USG-003] 获取默认计费模式失败 (app_type={app_type}): {e}");
                    PRICING_SOURCE_RESPONSE.to_string()
                }
            };
        let default_pricing_source = if default_pricing_source_raw == PRICING_SOURCE_RESPONSE
            || default_pricing_source_raw == PRICING_SOURCE_REQUEST
        {
            default_pricing_source_raw
        } else {
            log::warn!(
                "[USG-003] 默认计费模式无效 (app_type={app_type}): {default_pricing_source_raw}"
            );
            PRICING_SOURCE_RESPONSE.to_string()
        };

        let provider = self
            .db
            .get_provider_by_id(provider_id, app_type)
            .ok()
            .flatten();

        let (provider_multiplier, provider_pricing_source) = provider
            .as_ref()
            .and_then(|p| p.meta.as_ref())
            .map(|meta| {
                (
                    meta.cost_multiplier.as_deref(),
                    meta.pricing_model_source.as_deref(),
                )
            })
            .unwrap_or((None, None));

        let cost_multiplier = match provider_multiplier {
            Some(value) => match Decimal::from_str(value) {
                Ok(parsed) => parsed,
                Err(e) => {
                    log::warn!(
                        "[USG-003] 供应商倍率解析失败 (provider_id={provider_id}): {value} - {e}"
                    );
                    default_multiplier
                }
            },
            None => default_multiplier,
        };

        let pricing_model_source = match provider_pricing_source {
            Some(value) if value == PRICING_SOURCE_RESPONSE || value == PRICING_SOURCE_REQUEST => {
                value.to_string()
            }
            Some(value) => {
                log::warn!("[USG-003] 供应商计费模式无效 (provider_id={provider_id}): {value}");
                default_pricing_source.clone()
            }
            None => default_pricing_source.clone(),
        };

        (cost_multiplier, pricing_model_source)
    }
}

pub(crate) async fn record_usage_in_db_source(
    db: &Database,
    record: UsageRecord,
) -> ProxyCoreResult<()> {
    let logger = UsageLogger::new(db);
    let lookup = usage_pricing_config_lookup_from_record(&record);
    let (multiplier, pricing_model_source) = logger
        .resolve_pricing_config(&lookup.provider_id, &lookup.app_type)
        .await;
    let pricing_model = usage_record_pricing_model(&record, &pricing_model_source);
    let pricing = logger
        .get_model_pricing(&pricing_model)
        .map_err(|error| usage_error("load model pricing", error))?;
    let projection = usage_record_to_request_log(
        &record,
        &pricing_model_source,
        pricing.as_ref(),
        multiplier,
        || uuid::Uuid::new_v4().to_string(),
    );

    log_usage_request_projection_warnings(&projection);

    logger
        .log_request(&projection.log)
        .map_err(|error| usage_error("record usage", error))
}

#[derive(Clone)]
pub(crate) struct CcSwitchUsageSink {
    db: Arc<Database>,
}

impl CcSwitchUsageSink {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl UsageSink for CcSwitchUsageSink {
    fn record_usage<'a>(&'a self, record: UsageRecord) -> BoxFuture<'a, ProxyCoreResult<()>> {
        Box::pin(async move { record_usage_in_db_source(&self.db, record).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_request() -> Result<(), AppError> {
        let db = Database::memory()?;

        let logger = UsageLogger::new(&db);

        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            model: None,
            message_id: None,
        };

        let log = RequestLog {
            request_id: "req-123".to_string(),
            provider_id: "provider-1".to_string(),
            app_type: "claude".to_string(),
            model: "test-model".to_string(),
            request_model: "req-model".to_string(),
            pricing_model: "test-model".to_string(),
            usage,
            cost: None,
            latency_ms: 100,
            first_token_ms: None,
            status_code: 200,
            error_message: None,
            session_id: None,
            provider_type: Some("claude".to_string()),
            channel_id: Some("channel-1".to_string()),
            channel_name: Some("Relay One".to_string()),
            route_group: Some("default".to_string()),
            is_streaming: false,
            cost_multiplier: "1".to_string(),
        };

        logger.log_request(&log)?;

        // 验证记录已插入
        let conn = crate::database::lock_conn!(db.conn);
        let (count, request_model, channel_id, channel_name, route_group): (
            i64,
            String,
            String,
            String,
            String,
        ) = conn
            .query_row(
                "SELECT COUNT(*), request_model, channel_id, channel_name, route_group
                 FROM proxy_request_logs WHERE request_id = 'req-123'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(request_model, "req-model");
        assert_eq!(channel_id, "channel-1");
        assert_eq!(channel_name, "Relay One");
        assert_eq!(route_group, "default");
        Ok(())
    }
}
