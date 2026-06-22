//! 故障转移队列命令
//!
//! 管理代理模式下的故障转移队列（基于 providers 表的 in_failover_queue 字段）

use crate::database::FailoverQueueItem;
use crate::provider::Provider;
use crate::proxy_core_adapter::{
    build_provider_switched_event_payload, plan_auto_failover_toggle, AutoFailoverToggleInput,
    ProxyCoreError, AUTO_FAILOVER_EMPTY_QUEUE_WITHOUT_CURRENT_PROVIDER_MESSAGE,
    AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE, PROVIDER_SWITCHED_EVENT,
    PROVIDER_SWITCHED_SOURCE_FAILOVER_ENABLED,
};
use crate::store::AppState;
use std::str::FromStr;
use tauri::Emitter;

fn auto_failover_plan_error_to_string(error: ProxyCoreError) -> String {
    match error {
        ProxyCoreError::InvalidRequest(message) => message,
        other => other.to_string(),
    }
}

/// 获取故障转移队列
#[tauri::command]
pub async fn get_failover_queue(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<Vec<FailoverQueueItem>, String> {
    state
        .db
        .get_failover_queue(&app_type)
        .map_err(|e| e.to_string())
}

/// 获取可添加到故障转移队列的供应商（不在队列中的）
#[tauri::command]
pub async fn get_available_providers_for_failover(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<Vec<Provider>, String> {
    state
        .db
        .get_available_providers_for_failover(&app_type)
        .map_err(|e| e.to_string())
}

/// 添加供应商到故障转移队列
#[tauri::command]
pub async fn add_to_failover_queue(
    state: tauri::State<'_, AppState>,
    app_type: String,
    provider_id: String,
) -> Result<(), String> {
    state
        .db
        .add_to_failover_queue(&app_type, &provider_id)
        .map_err(|e| e.to_string())
}

/// 从故障转移队列移除供应商
#[tauri::command]
pub async fn remove_from_failover_queue(
    state: tauri::State<'_, AppState>,
    app_type: String,
    provider_id: String,
) -> Result<(), String> {
    state
        .db
        .remove_from_failover_queue(&app_type, &provider_id)
        .map_err(|e| e.to_string())
}

/// 获取指定应用的自动故障转移开关状态（从 proxy_config 表读取）
#[tauri::command]
pub async fn get_auto_failover_enabled(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<bool, String> {
    state
        .db
        .get_proxy_config_for_app(&app_type)
        .await
        .map(|config| config.auto_failover_enabled)
        .map_err(|e| e.to_string())
}

/// 设置指定应用的自动故障转移开关状态（写入 proxy_config 表）
///
/// 注意：关闭故障转移时不会清除队列，队列内容会保留供下次开启时使用
#[tauri::command]
pub async fn set_auto_failover_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    app_type: String,
    enabled: bool,
) -> Result<(), String> {
    log::info!(
        "[Failover] Setting auto_failover_enabled: app_type='{app_type}', enabled={enabled}"
    );

    // 读取当前配置
    let mut config = state
        .db
        .get_proxy_config_for_app(&app_type)
        .await
        .map_err(|e| e.to_string())?;

    if enabled && !config.enabled {
        return Err(AUTO_FAILOVER_ENABLE_REQUIRES_PROXY_TAKEOVER_MESSAGE.to_string());
    }

    // 队列为空时把当前供应商自动加入作为 P1，避免用户陷入"必须先加队列才能开启"的死锁
    let mut current_provider_id = None;
    let queued_provider_ids = if enabled {
        let queue = state
            .db
            .get_failover_queue(&app_type)
            .map_err(|e| e.to_string())?;

        if queue.is_empty() {
            let app_enum = crate::app_config::AppType::from_str(&app_type)
                .map_err(|_| format!("无效的应用类型: {app_type}"))?;

            let current_id = crate::settings::get_effective_current_provider(&state.db, &app_enum)
                .map_err(|e| e.to_string())?;

            let Some(current_id) = current_id else {
                return Err(
                    AUTO_FAILOVER_EMPTY_QUEUE_WITHOUT_CURRENT_PROVIDER_MESSAGE.to_string(),
                );
            };

            current_provider_id = Some(current_id);
        }

        queue
            .into_iter()
            .map(|item| item.provider_id)
            .collect()
    } else {
        Vec::new()
    };

    let plan = plan_auto_failover_toggle(AutoFailoverToggleInput::new(
        enabled,
        config.enabled,
        queued_provider_ids,
        current_provider_id,
    ))
    .map_err(auto_failover_plan_error_to_string)?;

    let mut auto_added_provider_id = None;
    if let Some(provider_id) = plan.provider_id_to_add_to_queue.as_ref() {
        state
            .db
            .add_to_failover_queue(&app_type, provider_id)
            .map_err(|e| e.to_string())?;
        auto_added_provider_id = Some(provider_id.clone());
    }

    // 开启前先切到 P1。只有切换成功后才写入 auto_failover_enabled=true，
    // 避免 P1 不可切换（例如 official provider）时留下“开关已开但目标未切”的脏状态。
    if let Some(provider_id) = plan.provider_id_to_switch_to.as_ref() {
        if let Err(e) = state
            .proxy_service
            .switch_proxy_target(&app_type, provider_id)
            .await
        {
            if let Some(provider_id) = auto_added_provider_id {
                let _ = state.db.remove_from_failover_queue(&app_type, &provider_id);
            }
            return Err(e);
        }
    }

    // 更新 auto_failover_enabled 字段
    config.auto_failover_enabled = plan.auto_failover_enabled;

    // 写回数据库
    state
        .db
        .update_proxy_config_for_app(config)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(provider_id) = plan.provider_id_to_switch_to.as_ref() {
        // 发射 provider-switched 事件（让前端刷新当前供应商）
        let event_data = build_provider_switched_event_payload(
            &app_type,
            provider_id,
            PROVIDER_SWITCHED_SOURCE_FAILOVER_ENABLED,
        );
        let _ = app.emit(PROVIDER_SWITCHED_EVENT, event_data);
    }

    // 刷新托盘菜单，确保状态同步
    if let Ok(new_menu) = crate::tray::create_tray_menu(&app, &state) {
        if let Some(tray) = app.tray_by_id(crate::tray::TRAY_ID) {
            let _ = tray.set_menu(Some(new_menu));
        }
    }

    Ok(())
}
