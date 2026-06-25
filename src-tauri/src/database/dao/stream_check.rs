//! 流式健康检查日志 DAO

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::services::stream_check::{StreamCheckConfig, StreamCheckResult};

impl Database {
    /// 保存流式检查日志
    pub fn save_stream_check_log(
        &self,
        provider_id: &str,
        provider_name: &str,
        app_type: &str,
        result: &StreamCheckResult,
    ) -> Result<i64, AppError> {
        let conn = lock_conn!(self.conn);

        conn.execute(
            "INSERT INTO stream_check_logs 
             (provider_id, provider_name, app_type, status, success, message, 
              response_time_ms, http_status, model_used, retry_count, tested_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                provider_id,
                provider_name,
                app_type,
                result.status.as_str(),
                result.success,
                result.message,
                result.response_time_ms.map(|t| t as i64),
                result.http_status.map(|s| s as i64),
                result.model_used,
                result.retry_count as i64,
                result.tested_at,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(conn.last_insert_rowid())
    }

    /// 获取流式检查配置
    pub fn get_stream_check_config(&self) -> Result<StreamCheckConfig, AppError> {
        match self.get_setting("stream_check_config")? {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| AppError::Message(format!("解析配置失败: {e}"))),
            None => Ok(StreamCheckConfig::default()),
        }
    }

    /// Delete stream check logs older than `retain_days` days.
    /// Returns the number of deleted rows.
    pub fn cleanup_old_stream_check_logs(&self, retain_days: i64) -> Result<u64, AppError> {
        let cutoff = chrono::Utc::now().timestamp() - retain_days * 86400;
        let conn = lock_conn!(self.conn);
        let deleted = conn
            .execute(
                "DELETE FROM stream_check_logs WHERE tested_at < ?1",
                [cutoff],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        if deleted > 0 {
            log::info!("Cleaned up {deleted} stream_check_logs older than {retain_days} days");
        }
        Ok(deleted as u64)
    }

    /// 保存流式检查配置
    pub fn save_stream_check_config(&self, config: &StreamCheckConfig) -> Result<(), AppError> {
        let json = serde_json::to_string(config)
            .map_err(|e| AppError::Message(format!("序列化配置失败: {e}")))?;
        self.set_setting("stream_check_config", &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_core_adapter::ChannelReachabilityStatus as HealthStatus;
    use serde_json::json;

    #[test]
    fn stream_check_config_round_trips_core_contract() {
        let db = Database::memory().expect("memory db");

        assert_eq!(
            db.get_stream_check_config().expect("default config"),
            StreamCheckConfig::default()
        );

        let config = StreamCheckConfig {
            timeout_secs: 12,
            max_retries: 3,
            degraded_threshold_ms: 2500,
        };
        db.save_stream_check_config(&config).expect("save config");

        assert_eq!(db.get_stream_check_config().expect("saved config"), config);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &db.get_setting("stream_check_config")
                    .expect("read setting")
                    .expect("setting exists")
            )
            .expect("parse saved json"),
            json!({
                "timeoutSecs": 12,
                "maxRetries": 3,
                "degradedThresholdMs": 2500
            })
        );
    }

    #[test]
    fn stream_check_log_status_uses_core_contract() {
        let db = Database::memory().expect("memory db");
        let result = StreamCheckResult {
            status: HealthStatus::Degraded,
            success: true,
            message: "Reachable".to_string(),
            response_time_ms: Some(6100),
            http_status: Some(403),
            model_used: String::new(),
            tested_at: 1_771_000_000,
            retry_count: 1,
            error_category: None,
        };

        db.save_stream_check_log("provider-a", "Provider A", "opencode", &result)
            .expect("save log");

        let status = {
            let conn = db.conn.lock().expect("lock conn");
            conn.query_row(
                "SELECT status FROM stream_check_logs WHERE provider_id = ?1",
                ["provider-a"],
                |row| row.get::<_, String>(0),
            )
            .expect("read status")
        };
        assert_eq!(status, "degraded");
    }
}
