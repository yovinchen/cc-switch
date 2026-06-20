//! HTTP代理服务器
//!
//! 基于Axum的HTTP服务器，处理代理请求
//!
//! Uses a manual hyper HTTP/1.1 accept loop with `preserve_header_case(true)` so
//! that the original header-name casing from the CLI client is captured in a
//! `HeaderCaseMap` extension.  This map is later forwarded to the upstream via
//! the hyper-based HTTP client, producing wire-level header casing identical to
//! a direct (non-proxied) CLI request.

use super::{
    error::ProxyError, events::ProxyEventBus, failover_switch::FailoverSwitchManager, handlers,
    provider_router::ProviderRouter, providers::codex_chat_history::CodexChatHistoryStore,
};
use crate::database::Database;
use crate::proxy_core_adapter::{
    server_log_codes as log_srv, CircuitBreakerConfig, CurrentRouteTarget, GeminiShadowStore,
    ProxyConfig, ProxyEngine, ProxyRuntimeStatus, ProxyServerInfo,
};
use crate::proxy_core_host::{CcSwitchProxyRuntime, CcSwitchProxyServices};
use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{any, get, post, put},
    Router,
};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};
use tokio::task::JoinHandle;

/// 代理服务器状态（共享）
#[derive(Clone)]
pub struct ProxyState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<ProxyConfig>>,
    pub status: Arc<RwLock<ProxyRuntimeStatus>>,
    pub start_time: Arc<RwLock<Option<std::time::Instant>>>,
    /// 每个应用类型当前使用的 provider/channel target。
    pub current_providers: Arc<RwLock<HashMap<String, CurrentRouteTarget>>>,
    /// 共享的 ProviderRouter（持有熔断器状态，跨请求保持）
    pub provider_router: Arc<ProviderRouter>,
    /// Host adapter surface for the neutral proxy core contracts.
    pub proxy_core_services: Arc<CcSwitchProxyServices>,
    /// Gemini Native shadow state，用于 thoughtSignature / tool call 回放
    pub gemini_shadow: Arc<GeminiShadowStore>,
    /// Codex Chat bridge history，用于恢复 previous_response_id 指向的 tool call
    pub codex_chat_history: Arc<CodexChatHistoryStore>,
    /// AppHandle，用于发射事件和更新托盘菜单
    #[allow(dead_code)]
    pub app_handle: Option<tauri::AppHandle>,
    /// 故障转移切换管理器
    #[allow(dead_code)]
    pub failover_manager: Arc<FailoverSwitchManager>,
    /// 代理事件总线，供外部 SSE 监控和未来 ProxyEventSink 使用。
    pub events: Arc<ProxyEventBus>,
}

impl ProxyState {
    pub(crate) fn proxy_engine(&self) -> ProxyEngine<CcSwitchProxyServices> {
        ProxyEngine::new(self.proxy_core_services.clone())
    }
}

/// 代理HTTP服务器
pub struct ProxyServer {
    config: ProxyConfig,
    state: ProxyState,
    shutdown_tx: Arc<RwLock<Option<oneshot::Sender<()>>>>,
    /// 服务器任务句柄，用于等待服务器实际关闭
    server_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl ProxyServer {
    pub fn new(
        config: ProxyConfig,
        db: Arc<Database>,
        app_handle: Option<tauri::AppHandle>,
    ) -> Self {
        // 创建共享的 ProviderRouter（熔断器状态将跨所有请求保持）
        let provider_router = Arc::new(ProviderRouter::new(db.clone()));
        let events = Arc::new(ProxyEventBus::default());
        // 创建故障转移切换管理器
        let failover_manager = Arc::new(FailoverSwitchManager::new(db.clone()));
        let status = Arc::new(RwLock::new(ProxyRuntimeStatus::default()));
        let current_providers = Arc::new(RwLock::new(HashMap::new()));
        let gemini_shadow = Arc::new(GeminiShadowStore::default());
        let codex_chat_history = Arc::new(CodexChatHistoryStore::default());
        let proxy_core_services =
            Arc::new(CcSwitchProxyServices::with_runtime(CcSwitchProxyRuntime {
                db: db.clone(),
                provider_router: provider_router.clone(),
                status: status.clone(),
                current_providers: current_providers.clone(),
                events: events.clone(),
                gemini_shadow: gemini_shadow.clone(),
                codex_chat_history: codex_chat_history.clone(),
                failover_manager: failover_manager.clone(),
                app_handle: app_handle.clone(),
            }));

        let state = ProxyState {
            db,
            config: Arc::new(RwLock::new(config.clone())),
            status,
            start_time: Arc::new(RwLock::new(None)),
            current_providers,
            provider_router,
            proxy_core_services,
            gemini_shadow,
            codex_chat_history,
            app_handle,
            failover_manager,
            events,
        };

        Self {
            config,
            state,
            shutdown_tx: Arc::new(RwLock::new(None)),
            server_handle: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self) -> Result<ProxyServerInfo, ProxyError> {
        // 检查是否已在运行
        if self.shutdown_tx.read().await.is_some() {
            return Err(ProxyError::AlreadyRunning);
        }

        let addr: SocketAddr =
            format!("{}:{}", self.config.listen_address, self.config.listen_port)
                .parse()
                .map_err(|e| ProxyError::BindFailed(format!("无效的地址: {e}")))?;

        // 创建关闭通道
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // 构建路由
        let app = self.build_router();

        // 绑定监听器
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| ProxyError::BindFailed(e.to_string()))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| ProxyError::BindFailed(e.to_string()))?;
        let actual_port = local_addr.port();

        log::info!("[{}] 代理服务器启动于 {local_addr}", log_srv::STARTED);
        self.state.events.emit(
            "server_started",
            serde_json::json!({
                "address": local_addr.ip().to_string(),
                "port": actual_port,
            }),
        );

        // 更新全局代理端口，用于系统代理检测
        crate::proxy::http_client::set_proxy_port(actual_port);

        // 保存关闭句柄
        *self.shutdown_tx.write().await = Some(shutdown_tx);

        // 更新状态
        let mut status = self.state.status.write().await;
        status.running = true;
        status.address = self.config.listen_address.clone();
        status.port = actual_port;
        drop(status);

        // 记录启动时间
        *self.state.start_time.write().await = Some(std::time::Instant::now());

        // 启动服务器 — 使用手动 hyper HTTP/1.1 accept loop
        // 开启 preserve_header_case 以捕获客户端请求头的原始大小写
        let state = self.state.clone();
        let handle = tokio::spawn(async move {
            let mut shutdown_rx = shutdown_rx;
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        let (stream, _remote_addr) = match result {
                            Ok(v) => v,
                            Err(e) => {
                                log::error!("[{SRV}] accept 失败: {e}", SRV = log_srv::ACCEPT_ERR);
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                continue;
                            }
                        };

                        let app = app.clone();
                        tokio::spawn(async move {
                            // Peek raw TCP bytes to capture original header casing
                            // before hyper parses (and lowercases) the header names.
                            let original_cases = {
                                let mut peek_buf = vec![0u8; 8192];
                                match stream.peek(&mut peek_buf).await {
                                    Ok(n) => {
                                        let cases = super::hyper_client::OriginalHeaderCases::from_raw_bytes(&peek_buf[..n]);
                                        log::debug!(
                                            "[ProxyServer] Peeked {} bytes, captured {} header casings",
                                            n, cases.cases.len()
                                        );
                                        cases
                                    }
                                    Err(e) => {
                                        log::debug!("[ProxyServer] peek failed (non-fatal): {e}");
                                        super::hyper_client::OriginalHeaderCases::default()
                                    }
                                }
                            };

                            // service_fn 将 axum Router（tower::Service）桥接到 hyper
                            let service = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                                let mut router = app.clone();
                                let cases = original_cases.clone();
                                async move {
                                    // 将 hyper::body::Incoming 转为 axum::body::Body，保留 extensions
                                    let (mut parts, body) = req.into_parts();

                                    // Insert our own header case map alongside hyper's internal one
                                    parts.extensions.insert(cases);

                                    let body = axum::body::Body::new(body);
                                    let axum_req = http::Request::from_parts(parts, body);
                                    <Router as tower::Service<http::Request<axum::body::Body>>>::call(&mut router, axum_req).await
                                }
                            });

                            if let Err(e) = hyper::server::conn::http1::Builder::new()
                                .preserve_header_case(true)
                                .serve_connection(TokioIo::new(stream), service)
                                .await
                            {
                                // Connection reset / broken pipe 等在代理场景下很常见，debug 级别
                                log::debug!("[{SRV}] connection error: {e}", SRV = log_srv::CONN_ERR);
                            }
                        });
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }

            // 服务器停止后更新状态
            state.status.write().await.running = false;
            *state.start_time.write().await = None;
            state.events.emit("server_stopped", serde_json::json!({}));
        });

        // 保存服务器任务句柄
        *self.server_handle.write().await = Some(handle);

        Ok(ProxyServerInfo {
            address: self.config.listen_address.clone(),
            port: actual_port,
            started_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub async fn stop(&self) -> Result<(), ProxyError> {
        // 1. 发送关闭信号
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            let _ = tx.send(());
        } else {
            return Err(ProxyError::NotRunning);
        }

        // 2. 等待服务器任务结束（带 5 秒超时保护）
        if let Some(handle) = self.server_handle.write().await.take() {
            match tokio::time::timeout(std::time::Duration::from_secs(5), handle).await {
                Ok(Ok(())) => {
                    log::info!("[{}] 代理服务器已完全停止", log_srv::STOPPED);
                    Ok(())
                }
                Ok(Err(e)) => {
                    log::warn!("[{}] 代理服务器任务异常终止: {e}", log_srv::TASK_ERROR);
                    Err(ProxyError::StopFailed(e.to_string()))
                }
                Err(_) => {
                    log::warn!(
                        "[{}] 代理服务器停止超时（5秒），强制继续",
                        log_srv::STOP_TIMEOUT
                    );
                    Err(ProxyError::StopTimeout)
                }
            }
        } else {
            Ok(())
        }
    }

    pub async fn get_status(&self) -> ProxyRuntimeStatus {
        let mut status = self.state.status.read().await.clone();

        // 计算运行时间
        if let Some(start) = *self.state.start_time.read().await {
            status.uptime_seconds = start.elapsed().as_secs();
        }

        // 从 current_providers HashMap 获取每个应用类型当前正在使用的 provider
        let current_providers = self.state.current_providers.read().await;
        status.active_targets = current_providers.values().cloned().collect();
        status
            .active_targets
            .sort_by(|left, right| left.app_type.cmp(&right.app_type));

        status
    }

    /// 更新某个应用类型当前“目标供应商”（用于 UI 展示 active_targets）
    ///
    /// 注意：这不代表该供应商一定已经处理过请求，而是用于“热切换/启用故障转移立即切 P1”
    /// 等场景下，让 UI 能立刻反映最新目标。
    pub async fn set_active_target(&self, app_type: &str, provider_id: &str, provider_name: &str) {
        let mut current_providers = self.state.current_providers.write().await;
        current_providers.insert(
            app_type.to_string(),
            CurrentRouteTarget {
                app_type: app_type.to_string(),
                provider_id: provider_id.to_string(),
                provider_name: provider_name.to_string(),
                channel_id: None,
                channel_name: None,
                interface_kind: None,
                public_model: None,
                upstream_model: None,
            },
        );
    }

    fn build_router(&self) -> Router {
        let management_routes = Router::new()
            .route("/proxy/v1/health", get(handlers::health_check))
            .route("/proxy/v1/status", get(handlers::get_status))
            .route("/proxy/v1/events", get(handlers::stream_proxy_events))
            .route("/proxy/v1/apps", get(handlers::list_proxy_apps))
            .route(
                "/proxy/v1/apps/:app/providers",
                get(handlers::list_proxy_providers),
            )
            .route(
                "/proxy/v1/apps/:app/models",
                get(handlers::list_proxy_app_models),
            )
            .route(
                "/proxy/v1/channels",
                get(handlers::list_all_proxy_channels).post(handlers::create_proxy_channel),
            )
            .route(
                "/proxy/v1/channels/:channel_id",
                get(handlers::get_proxy_channel)
                    .patch(handlers::update_proxy_channel)
                    .delete(handlers::delete_proxy_channel),
            )
            .route(
                "/proxy/v1/channels/:channel_id/keys",
                get(handlers::list_proxy_channel_keys),
            )
            .route(
                "/proxy/v1/channels/:channel_id/keys/:key_ref",
                put(handlers::upsert_proxy_channel_key)
                    .patch(handlers::update_proxy_channel_key)
                    .delete(handlers::delete_proxy_channel_key),
            )
            .route(
                "/proxy/v1/channels/:channel_id/models",
                get(handlers::list_proxy_channel_models)
                    .put(handlers::replace_proxy_channel_models),
            )
            .route(
                "/proxy/v1/channels/:channel_id/test",
                post(handlers::test_proxy_channel),
            )
            .route(
                "/proxy/v1/apps/:app/channels",
                get(handlers::list_proxy_channels),
            )
            .route(
                "/proxy/v1/apps/:app/routes/current",
                get(handlers::get_current_proxy_route),
            )
            .route(
                "/proxy/v1/apps/:app/channels/migration/preview",
                get(handlers::preview_proxy_channel_migration),
            )
            .route(
                "/proxy/v1/apps/:app/channels/migration/materialize",
                post(handlers::materialize_proxy_channel_migration),
            )
            .route(
                "/proxy/v1/channels/:channel_id/breakers/reset",
                post(handlers::reset_proxy_channel_breaker),
            )
            .route(
                "/proxy/v1/route/resolve",
                post(handlers::resolve_proxy_route),
            )
            .route("/proxy/v1/groups", get(handlers::list_proxy_groups))
            .route_layer(middleware::from_fn_with_state(
                self.state.clone(),
                handlers::require_proxy_management_auth,
            ));

        Router::new()
            // 健康检查
            .route("/health", get(handlers::health_check))
            .route("/status", get(handlers::get_status))
            // Versioned management API (channel migration surface)
            .merge(management_routes)
            // Claude API (支持带前缀和不带前缀两种格式)
            .route("/v1/messages", post(handlers::handle_messages))
            .route("/claude/v1/messages", post(handlers::handle_messages))
            // Claude Desktop 3P 本地 gateway（独立 provider namespace）
            .route(
                "/claude-desktop/v1/models",
                get(handlers::handle_claude_desktop_models),
            )
            .route(
                "/claude-desktop/v1/messages",
                post(handlers::handle_claude_desktop_messages),
            )
            // OpenAI Chat Completions API (Codex CLI，支持带前缀和不带前缀)
            .route("/chat/completions", post(handlers::handle_chat_completions))
            .route(
                "/v1/chat/completions",
                post(handlers::handle_chat_completions),
            )
            .route(
                "/v1/v1/chat/completions",
                post(handlers::handle_chat_completions),
            )
            .route(
                "/codex/v1/chat/completions",
                post(handlers::handle_chat_completions),
            )
            // OpenAI Models API (Codex CLI reachability check)
            .route("/models", get(handlers::handle_models))
            .route("/v1/models", get(handlers::handle_models))
            // OpenAI Responses API (Codex CLI，支持带前缀和不带前缀)
            .route("/responses", post(handlers::handle_responses))
            .route("/v1/responses", post(handlers::handle_responses))
            .route("/v1/v1/responses", post(handlers::handle_responses))
            .route("/codex/v1/responses", post(handlers::handle_responses))
            // OpenAI Responses Compact API (Codex CLI 远程压缩，透传)
            .route(
                "/responses/compact",
                post(handlers::handle_responses_compact),
            )
            .route(
                "/v1/responses/compact",
                post(handlers::handle_responses_compact),
            )
            .route(
                "/v1/v1/responses/compact",
                post(handlers::handle_responses_compact),
            )
            .route(
                "/codex/v1/responses/compact",
                post(handlers::handle_responses_compact),
            )
            // Gemini API (支持带前缀和不带前缀)
            //
            // 用 `any(..)` 覆盖所有 HTTP 方法：除了 POST `:generateContent` /
            // `:streamGenerateContent` / `:countTokens` 之外，Gemini SDK / CLI 还会发
            // GET `/models`、GET `/models/<id>` 等只读端点。如果只挂 POST，这些 GET
            // 请求会在路由层 404，绕过本地代理的统计、整流和故障转移。
            .route("/v1beta/*path", any(handlers::handle_gemini))
            .route("/gemini/v1beta/*path", any(handlers::handle_gemini))
            // Gemini 的 GA 版本也叫 /v1，给原 SDK 留一条出口
            .route("/gemini/v1/*path", any(handlers::handle_gemini))
            // 提高默认请求体大小限制（避免 413 Payload Too Large）
            .layer(DefaultBodyLimit::max(200 * 1024 * 1024))
            .with_state(self.state.clone())
    }

    /// 在不重启服务的情况下更新运行时配置
    pub async fn apply_runtime_config(&self, config: &ProxyConfig) {
        *self.state.config.write().await = config.clone();
    }

    /// 热更新熔断器配置
    ///
    /// 将新配置应用到所有已创建的熔断器实例
    pub async fn update_circuit_breaker_configs(&self, config: CircuitBreakerConfig) {
        self.state.provider_router.update_all_configs(config).await;
    }

    pub async fn update_circuit_breaker_config_for_app(
        &self,
        app_type: &str,
        config: CircuitBreakerConfig,
    ) {
        self.state
            .provider_router
            .update_app_configs(app_type, config)
            .await;
    }

    /// 重置指定 Provider 的熔断器
    pub async fn reset_provider_circuit_breaker(&self, provider_id: &str, app_type: &str) {
        self.state
            .provider_router
            .reset_provider_breaker(provider_id, app_type)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::proxy_core_adapter::RouteResolveRequest;
    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request, StatusCode},
    };
    use serde_json::{json, Value};
    use tower::Service;

    #[test]
    fn build_router_accepts_management_routes() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let server = ProxyServer::new(ProxyConfig::default(), db, None);

        let _router = server.build_router();
    }

    #[tokio::test]
    async fn event_stream_management_route_returns_sse_response() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let server = ProxyServer::new(ProxyConfig::default(), db, None);
        let mut router = server.build_router();

        let response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(content_type.starts_with("text/event-stream"));
    }

    #[tokio::test]
    async fn management_routes_require_token_for_non_loopback_listener() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let config = ProxyConfig {
            listen_address: "0.0.0.0".to_string(),
            ..ProxyConfig::default()
        };
        let server = ProxyServer::new(config, db, None);
        let mut router = server.build_router();

        let response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn management_routes_accept_configured_bearer_token() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let config = ProxyConfig {
            listen_address: "0.0.0.0".to_string(),
            management_auth_token: Some("secret-token".to_string()),
            ..ProxyConfig::default()
        };
        let server = ProxyServer::new(config, db, None);
        let mut router = server.build_router();

        let rejected = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps")
                .header(axum::http::header::AUTHORIZATION, "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);

        let accepted = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps")
                .header(axum::http::header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(accepted.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn claude_desktop_gateway_requires_bearer_token() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let server = ProxyServer::new(ProxyConfig::default(), db, None);
        let mut router = server.build_router();

        let response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/claude-desktop/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = response_json(response).await;
        assert_eq!(
            body["error"]["message"],
            "认证失败: Claude Desktop gateway 缺少 Authorization 头"
        );
    }

    #[tokio::test]
    async fn management_apps_and_providers_return_sanitized_summaries() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_API_KEY": "secret-key"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();
        db.set_current_provider("claude", "a").unwrap();

        let server = ProxyServer::new(ProxyConfig::default(), db, None);
        let mut router = server.build_router();

        let apps_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(apps_response.status(), StatusCode::OK);
        let apps = response_json(apps_response).await;
        let claude = apps["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|app| app["appType"] == "claude")
            .expect("claude app summary");
        assert_eq!(claude["providerCount"], 1);

        let providers_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps/claude/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(providers_response.status(), StatusCode::OK);
        let providers = response_json(providers_response).await;
        let provider = &providers["providers"].as_array().unwrap()[0];
        assert_eq!(provider["id"], "a");
        assert_eq!(provider["name"], "Provider A");
        assert_eq!(provider["current"], true);
        assert_eq!(provider["routeCandidate"], true);
        assert!(provider.get("settingsConfig").is_none());
        assert!(provider.to_string().find("secret-key").is_none());
    }

    #[tokio::test]
    async fn current_route_management_route_reports_configured_and_active_channel_target() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_API_KEY": "secret-key"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();
        db.set_current_provider("claude", "a").unwrap();

        let server = ProxyServer::new(ProxyConfig::default(), db, None);
        let mut router = server.build_router();

        let configured_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps/claude/routes/current")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(configured_response.status(), StatusCode::OK);
        let configured = response_json(configured_response).await;
        assert_eq!(configured["appType"], "claude");
        assert_eq!(configured["active"], false);
        assert!(configured["target"].is_null());
        assert_eq!(configured["configuredProvider"]["id"], "a");
        assert!(configured.to_string().find("secret-key").is_none());

        server.state.current_providers.write().await.insert(
            "claude".to_string(),
            CurrentRouteTarget {
                app_type: "claude".to_string(),
                provider_id: "a".to_string(),
                provider_name: "Provider A".to_string(),
                channel_id: Some("channel-a".to_string()),
                channel_name: Some("Relay A".to_string()),
                interface_kind: Some("openai_responses".to_string()),
                public_model: Some("public-sonnet".to_string()),
                upstream_model: Some("upstream-sonnet".to_string()),
            },
        );

        let active_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps/claude/routes/current")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(active_response.status(), StatusCode::OK);
        let active = response_json(active_response).await;
        assert_eq!(active["active"], true);
        assert_eq!(active["target"]["providerId"], "a");
        assert_eq!(active["target"]["channelId"], "channel-a");
        assert_eq!(active["target"]["interfaceKind"], "openai_responses");
        assert_eq!(active["target"]["upstreamModel"], "upstream-sonnet");
        assert!(active.to_string().find("secret-key").is_none());
    }

    #[tokio::test]
    async fn proxy_health_route_returns_core_contract() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let server = ProxyServer::new(ProxyConfig::default(), db, None);
        let mut router = server.build_router();

        let response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let health = response_json(response).await;
        assert_eq!(health["status"], "healthy");
        assert!(health["timestamp"]
            .as_str()
            .is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok()));
    }

    #[tokio::test]
    async fn proxy_server_runtime_smoke_exposes_versioned_management_api() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "runtime-provider".to_string(),
            "Runtime Provider".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://runtime.example.com/v1",
                    "ANTHROPIC_API_KEY": "provider-secret"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();
        db.set_current_provider("claude", "runtime-provider")
            .unwrap();

        let config = ProxyConfig {
            listen_address: "127.0.0.1".to_string(),
            listen_port: 0,
            ..ProxyConfig::default()
        };
        let server = ProxyServer::new(config, db, None);
        let info = server.start().await.expect("start proxy server");
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("reqwest client");
        let base_url = format!("http://127.0.0.1:{}", info.port);

        let smoke = async {
            let health_response = client
                .get(format!("{base_url}/proxy/v1/health"))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if health_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected health status: {}",
                    health_response.status()
                ));
            }
            let health = health_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if health["status"] != "healthy" {
                return Err(format!("unexpected health body: {health}"));
            }

            let status_response = client
                .get(format!("{base_url}/proxy/v1/status"))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if status_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected status response: {}",
                    status_response.status()
                ));
            }
            let status = status_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if status["running"] != true {
                return Err(format!("unexpected status body: {status}"));
            }
            if status["port"].as_u64() != Some(u64::from(info.port)) {
                return Err(format!("status did not report actual port: {status}"));
            }

            let apps_response = client
                .get(format!("{base_url}/proxy/v1/apps"))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if apps_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected apps status: {}",
                    apps_response.status()
                ));
            }
            let apps = apps_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            let claude_app = apps["apps"]
                .as_array()
                .and_then(|apps| apps.iter().find(|app| app["appType"] == "claude"))
                .ok_or_else(|| format!("apps response missing claude app: {apps}"))?;
            if claude_app["providerCount"] != 1 {
                return Err(format!("unexpected claude app summary: {claude_app}"));
            }

            let providers_response = client
                .get(format!("{base_url}/proxy/v1/apps/claude/providers"))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if providers_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected providers status: {}",
                    providers_response.status()
                ));
            }
            let providers = providers_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            let runtime_provider = providers["providers"]
                .as_array()
                .and_then(|providers| {
                    providers
                        .iter()
                        .find(|provider| provider["id"] == "runtime-provider")
                })
                .ok_or_else(|| {
                    format!("providers response missing runtime provider: {providers}")
                })?;
            if runtime_provider["name"] != "Runtime Provider"
                || runtime_provider["current"] != true
                || runtime_provider["routeCandidate"] != true
                || runtime_provider.get("settingsConfig").is_some()
                || runtime_provider.to_string().contains("provider-secret")
            {
                return Err(format!("unexpected provider summary: {runtime_provider}"));
            }

            let create_response = client
                .post(format!("{base_url}/proxy/v1/channels"))
                .json(&json!({
                    "providerId": "runtime-provider",
                    "appType": "claude",
                    "name": "Runtime Relay",
                    "baseUrl": "https://runtime-relay.example.com/v1",
                    "interfaceKind": "openai_responses",
                    "authProfileRef": "channel-key:primary",
                    "models": [{
                        "publicModel": "runtime-public",
                        "upstreamModel": "runtime-upstream"
                    }]
                }))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if create_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected create channel status: {}",
                    create_response.status()
                ));
            }
            let created = create_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            let channel_id = created["id"]
                .as_str()
                .ok_or_else(|| format!("created channel missing id: {created}"))?;
            if created["authProfileRef"] != "channel-key:primary" {
                return Err(format!("unexpected created channel body: {created}"));
            }

            let app_channels_response = client
                .get(format!("{base_url}/proxy/v1/apps/claude/channels"))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if app_channels_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected app channels status: {}",
                    app_channels_response.status()
                ));
            }
            let app_channels = app_channels_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if app_channels["source"] != "materialized_channels"
                || app_channels["channels"].as_array().map(Vec::len) != Some(1)
                || app_channels["channels"][0]["id"] != channel_id
                || app_channels["channels"][0]["models"].as_array().map(Vec::len) != Some(1)
                || app_channels["channels"][0]["models"][0]["publicModel"] != "runtime-public"
                || app_channels.get("rejected").is_some()
            {
                return Err(format!("unexpected app channels body: {app_channels}"));
            }

            let filtered_channels_response = client
                .get(format!(
                    "{base_url}/proxy/v1/apps/claude/channels?requestedModel=runtime-public&interfaceKind=anthropic_messages"
                ))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if filtered_channels_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected filtered channels status: {}",
                    filtered_channels_response.status()
                ));
            }
            let filtered_channels = filtered_channels_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if filtered_channels["requestedModel"] != "runtime-public"
                || filtered_channels["interfaceKind"] != "anthropic_messages"
                || filtered_channels["routeGroup"] != "default"
                || filtered_channels["source"] != "materialized_channels"
                || filtered_channels["channels"].as_array().map(Vec::len) != Some(1)
                || filtered_channels["channels"][0]["channelId"] != channel_id
                || filtered_channels["channels"][0]["upstreamModel"] != "runtime-upstream"
                || !filtered_channels["rejected"]
                    .as_array()
                    .is_some_and(Vec::is_empty)
            {
                return Err(format!(
                    "unexpected filtered channels body: {filtered_channels}"
                ));
            }

            let app_models_response = client
                .get(format!(
                    "{base_url}/proxy/v1/apps/claude/models?group=default&interface=anthropic_messages"
                ))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if app_models_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected app models status: {}",
                    app_models_response.status()
                ));
            }
            let app_models = app_models_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if app_models["routeGroup"] != "default"
                || app_models["interfaceKind"] != "anthropic_messages"
                || app_models["models"].as_array().map(Vec::len) != Some(1)
                || app_models["models"][0]["publicModel"] != "runtime-public"
                || app_models["models"][0]["upstreamModel"] != "runtime-upstream"
                || app_models["models"][0]["channelName"] != "Runtime Relay"
            {
                return Err(format!("unexpected app models body: {app_models}"));
            }

            let groups_response = client
                .get(format!("{base_url}/proxy/v1/groups?appType=claude"))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if groups_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected groups status: {}",
                    groups_response.status()
                ));
            }
            let groups = groups_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if groups["appType"] != "claude"
                || groups["sources"].as_array().map(Vec::len) != Some(1)
                || groups["sources"][0] != "materialized_channels"
                || groups["groups"].as_array().map(Vec::len) != Some(1)
                || groups["groups"][0]["name"] != "default"
                || groups["groups"][0]["channelCount"] != 1
                || groups["groups"][0]["appTypes"][0] != "claude"
            {
                return Err(format!("unexpected groups body: {groups}"));
            }

            let route_response = client
                .post(format!("{base_url}/proxy/v1/route/resolve"))
                .json(&json!({
                    "appType": "claude",
                    "requestedModel": "runtime-public",
                    "interfaceKind": "anthropic_messages"
                }))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if route_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected route resolve status: {}",
                    route_response.status()
                ));
            }
            let route = route_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if route["source"] != "materialized_channels"
                || route["candidates"].as_array().map(Vec::len) != Some(1)
                || route["candidates"][0]["channelId"] != channel_id
                || route["candidates"][0]["upstreamModel"] != "runtime-upstream"
            {
                return Err(format!("unexpected route resolve body: {route}"));
            }

            let upsert_key_response = client
                .put(format!(
                    "{base_url}/proxy/v1/channels/{channel_id}/keys/primary"
                ))
                .json(&json!({
                    "keyValue": "sk-runtime-channel",
                    "status": "enabled",
                    "priority": 50,
                    "weight": 60
                }))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if upsert_key_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected upsert key status: {}",
                    upsert_key_response.status()
                ));
            }
            let upserted_key = upsert_key_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if upserted_key["keyRef"] != "primary"
                || upserted_key["priority"] != 50
                || upserted_key["weight"] != 60
                || upserted_key.get("keyValue").is_some()
                || upserted_key.to_string().contains("sk-runtime-channel")
            {
                return Err(format!("unexpected upsert key body: {upserted_key}"));
            }

            let list_keys_response = client
                .get(format!("{base_url}/proxy/v1/channels/{channel_id}/keys"))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if list_keys_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected list keys status: {}",
                    list_keys_response.status()
                ));
            }
            let keys = list_keys_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if keys["keys"].as_array().map(Vec::len) != Some(1)
                || keys["keys"][0]["keyRef"] != "primary"
                || keys.to_string().contains("sk-runtime-channel")
            {
                return Err(format!("unexpected list keys body: {keys}"));
            }

            let patch_key_response = client
                .patch(format!(
                    "{base_url}/proxy/v1/channels/{channel_id}/keys/primary"
                ))
                .json(&json!({
                    "status": "disabled",
                    "weight": 10
                }))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if patch_key_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected patch key status: {}",
                    patch_key_response.status()
                ));
            }
            let patched_key = patch_key_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if patched_key["status"] != "disabled"
                || patched_key["weight"] != 10
                || patched_key.to_string().contains("sk-runtime-channel")
            {
                return Err(format!("unexpected patch key body: {patched_key}"));
            }

            let delete_key_response = client
                .delete(format!(
                    "{base_url}/proxy/v1/channels/{channel_id}/keys/primary"
                ))
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if delete_key_response.status() != StatusCode::OK {
                return Err(format!(
                    "unexpected delete key status: {}",
                    delete_key_response.status()
                ));
            }
            let deleted_key = delete_key_response
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())?;
            if deleted_key["channelId"] != channel_id
                || deleted_key["keyRef"] != "primary"
                || deleted_key["deleted"] != true
            {
                return Err(format!("unexpected delete key body: {deleted_key}"));
            }

            Ok::<(), String>(())
        }
        .await;
        let stop = server.stop().await;

        assert!(stop.is_ok(), "stop proxy server: {stop:?}");
        smoke.expect("runtime management smoke");
    }

    #[tokio::test]
    async fn app_channel_management_route_applies_route_filters() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();
        db.set_current_provider("claude", "a").unwrap();
        db.materialize_legacy_proxy_channels("claude").unwrap();

        let server = ProxyServer::new(ProxyConfig::default(), db, None);
        let mut router = server.build_router();

        let unfiltered_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps/claude/channels")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(unfiltered_response.status(), StatusCode::OK);
        let unfiltered = response_json(unfiltered_response).await;
        assert_eq!(unfiltered["channels"].as_array().unwrap().len(), 1);
        assert!(unfiltered.get("rejected").is_none());

        let matched_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps/claude/channels?requestedModel=claude-sonnet-4&interfaceKind=anthropic_messages")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(matched_response.status(), StatusCode::OK);
        let matched = response_json(matched_response).await;
        assert_eq!(matched["requestedModel"], "claude-sonnet-4");
        assert_eq!(matched["interfaceKind"], "anthropic_messages");
        assert_eq!(matched["routeGroup"], "default");
        assert_eq!(matched["channels"].as_array().unwrap().len(), 1);
        assert!(matched["rejected"].as_array().unwrap().is_empty());
        assert_eq!(matched["channels"][0]["upstreamModel"], "claude-sonnet-4");

        let rejected_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps/claude/channels?model=missing-model&group=default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(rejected_response.status(), StatusCode::OK);
        let rejected = response_json(rejected_response).await;
        assert!(rejected["channels"].as_array().unwrap().is_empty());
        assert_eq!(rejected["rejected"].as_array().unwrap().len(), 1);
        assert_eq!(
            rejected["rejected"][0]["reasons"][0],
            "model_unavailable:missing-model"
        );
    }

    #[tokio::test]
    async fn route_resolve_management_route_returns_core_contract() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();
        db.materialize_legacy_proxy_channels("claude").unwrap();

        let server = ProxyServer::new(ProxyConfig::default(), db, None);
        let mut router = server.build_router();
        let body = serde_json::to_vec(&RouteResolveRequest {
            app_type: "claude".to_string(),
            requested_model: Some("claude-sonnet-4".to_string()),
            interface_kind: Some("anthropic_messages".to_string()),
            route_group: None,
        })
        .unwrap();

        let response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::POST)
                .uri("/proxy/v1/route/resolve")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let resolved = response_json(response).await;
        assert_eq!(resolved["appType"], "claude");
        assert_eq!(resolved["source"], "materialized_channels");
        assert_eq!(resolved["routeGroup"], "default");
        assert_eq!(resolved["candidates"].as_array().unwrap().len(), 1);
        assert_eq!(resolved["candidates"][0]["providerId"], "a");
        assert!(resolved["rejected"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn group_management_route_lists_visible_channel_groups() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();
        db.materialize_legacy_proxy_channels("claude").unwrap();

        let server = ProxyServer::new(ProxyConfig::default(), db, None);
        let mut router = server.build_router();

        let response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/groups?appType=claude")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let groups = response_json(response).await;
        assert_eq!(groups["appType"], "claude");
        assert_eq!(groups["sources"][0], "materialized_channels");
        assert_eq!(groups["groups"].as_array().unwrap().len(), 1);
        assert_eq!(groups["groups"][0]["name"], "default");
        assert_eq!(groups["groups"][0]["channelCount"], 1);
        assert_eq!(groups["groups"][0]["appTypes"][0], "claude");
    }

    #[tokio::test]
    async fn channel_crud_management_routes_manage_manual_channels_and_models() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_API_KEY": "secret-key"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();

        let server = ProxyServer::new(ProxyConfig::default(), db.clone(), None);
        let mut router = server.build_router();

        let invalid_create_response = Service::call(
            &mut router,
            json_request(
                Method::POST,
                "/proxy/v1/channels",
                json!({
                    "providerId": "a",
                    "appType": "claude",
                    "name": "Invalid Auth Relay",
                    "baseUrl": "https://invalid-auth.example.com/v1",
                    "interfaceKind": "openai_responses",
                    "authProfileRef": "channel-key:"
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(invalid_create_response.status(), StatusCode::BAD_REQUEST);
        let invalid_create = response_json(invalid_create_response).await;
        assert_eq!(
            invalid_create["error"]["message"],
            "无效的请求: 无效输入: authProfileRef must be provider:<app>:<providerId> or channel-key:<keyRef>"
        );

        let create_response = Service::call(
            &mut router,
            json_request(
                Method::POST,
                "/proxy/v1/channels",
                json!({
                    "providerId": "a",
                    "appType": "claude",
                    "name": "Manual Relay",
                    "baseUrl": "https://manual.example.com/v1/",
                    "interfaceKind": "openai_responses",
                    "authProfileRef": "channel-key:manual-relay",
                    "priority": 80,
                    "weight": 40,
                    "healthPolicy": {
                        "mode": "http",
                        "path": "/healthz",
                        "intervalSeconds": 30
                    },
                    "headerOverrides": {
                        "x-relay-profile": "manual"
                    },
                    "paramOverrides": {
                        "api-version": "2026-06-20"
                    },
                    "statusCodeMapping": [{
                        "from": 429,
                        "to": "rate_limited"
                    }],
                    "tags": ["manual", "relay"],
                    "metadata": {
                        "owner": "integration-test"
                    },
                    "models": [{
                        "publicModel": "sonnet-public",
                        "upstreamModel": "upstream-sonnet"
                    }]
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let created = response_json(create_response).await;
        let channel_id = created["id"].as_str().unwrap().to_string();
        assert_eq!(created["baseUrl"], "https://manual.example.com/v1");
        assert_eq!(created["authProfileRef"], "channel-key:manual-relay");
        assert_eq!(created["priority"], 80);
        assert_eq!(created["weight"], 40);
        assert_eq!(created["healthPolicy"]["mode"], "http");
        assert_eq!(created["healthPolicy"]["path"], "/healthz");
        assert_eq!(created["headerOverrides"]["x-relay-profile"], "manual");
        assert_eq!(created["paramOverrides"]["api-version"], "2026-06-20");
        assert_eq!(created["statusCodeMapping"][0]["from"], 429);
        assert_eq!(created["tags"][0], "manual");
        assert_eq!(created["metadata"]["owner"], "integration-test");
        assert_eq!(created["models"].as_array().unwrap().len(), 1);

        let route_response = Service::call(
            &mut router,
            json_request(
                Method::POST,
                "/proxy/v1/route/resolve",
                json!({
                    "appType": "claude",
                    "requestedModel": "sonnet-public",
                    "interfaceKind": "openai_responses"
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(route_response.status(), StatusCode::OK);
        let route = response_json(route_response).await;
        let candidate = &route["candidates"].as_array().unwrap()[0];
        assert_eq!(candidate["channelId"], channel_id);
        assert_eq!(candidate["providerId"], "a");
        assert_eq!(candidate["priority"], 80);
        assert_eq!(candidate["weight"], 40);
        assert_eq!(candidate["publicModel"], "sonnet-public");
        assert_eq!(candidate["upstreamModel"], "upstream-sonnet");

        let list_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/channels?appType=claude")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list = response_json(list_response).await;
        assert_eq!(list["channels"].as_array().unwrap().len(), 1);
        assert_eq!(
            list["channels"][0]["headerOverrides"]["x-relay-profile"],
            "manual"
        );
        assert_eq!(
            list["channels"][0]["paramOverrides"]["api-version"],
            "2026-06-20"
        );

        let get_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri(format!("/proxy/v1/channels/{channel_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let fetched = response_json(get_response).await;
        assert_eq!(fetched["authProfileRef"], "channel-key:manual-relay");
        assert_eq!(fetched["healthPolicy"]["intervalSeconds"], 30);
        assert_eq!(fetched["statusCodeMapping"][0]["to"], "rate_limited");

        let upsert_key_response = Service::call(
            &mut router,
            json_request(
                Method::PUT,
                &format!("/proxy/v1/channels/{channel_id}/keys/primary"),
                json!({
                    "keyValue": "sk-channel-secret",
                    "status": "enabled",
                    "priority": 10,
                    "weight": 80
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(upsert_key_response.status(), StatusCode::OK);
        let upserted_key = response_json(upsert_key_response).await;
        assert_eq!(upserted_key["channelId"], channel_id);
        assert_eq!(upserted_key["keyRef"], "primary");
        assert_eq!(upserted_key["status"], "enabled");
        assert_eq!(upserted_key["priority"], 10);
        assert_eq!(upserted_key["weight"], 80);
        assert!(upserted_key.get("keyValue").is_none());
        assert!(upserted_key.to_string().find("sk-channel-secret").is_none());

        let list_keys_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri(format!("/proxy/v1/channels/{channel_id}/keys"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(list_keys_response.status(), StatusCode::OK);
        let keys = response_json(list_keys_response).await;
        assert_eq!(keys["channelId"], channel_id);
        assert_eq!(keys["keys"].as_array().unwrap().len(), 1);
        assert_eq!(keys["keys"][0]["keyRef"], "primary");
        assert!(keys["keys"][0].get("keyValue").is_none());
        assert!(keys.to_string().find("sk-channel-secret").is_none());

        let patch_key_response = Service::call(
            &mut router,
            json_request(
                Method::PATCH,
                &format!("/proxy/v1/channels/{channel_id}/keys/primary"),
                json!({
                    "status": "disabled",
                    "weight": 20
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(patch_key_response.status(), StatusCode::OK);
        let patched_key = response_json(patch_key_response).await;
        assert_eq!(patched_key["status"], "disabled");
        assert_eq!(patched_key["weight"], 20);
        assert!(patched_key.get("keyValue").is_none());
        assert!(db
            .get_enabled_proxy_channel_key(&channel_id, "primary")
            .unwrap()
            .is_none());

        let delete_key_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/proxy/v1/channels/{channel_id}/keys/primary"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(delete_key_response.status(), StatusCode::OK);
        let deleted_key = response_json(delete_key_response).await;
        assert_eq!(deleted_key["channelId"], channel_id);
        assert_eq!(deleted_key["keyRef"], "primary");
        assert_eq!(deleted_key["deleted"], true);

        let list_keys_after_delete_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri(format!("/proxy/v1/channels/{channel_id}/keys"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(list_keys_after_delete_response.status(), StatusCode::OK);
        let keys_after_delete = response_json(list_keys_after_delete_response).await;
        assert!(keys_after_delete["keys"].as_array().unwrap().is_empty());

        let patch_response = Service::call(
            &mut router,
            json_request(
                Method::PATCH,
                &format!("/proxy/v1/channels/{channel_id}"),
                json!({
                    "status": "disabled",
                    "weight": 25,
                    "groups": ["default", "beta"]
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(patch_response.status(), StatusCode::OK);
        let patched = response_json(patch_response).await;
        assert_eq!(patched["status"], "disabled");
        assert_eq!(patched["weight"], 25);

        let replace_models_response = Service::call(
            &mut router,
            json_request(
                Method::PUT,
                &format!("/proxy/v1/channels/{channel_id}/models"),
                json!({
                    "models": [{
                        "publicModel": "haiku-public",
                        "upstreamModel": "upstream-haiku"
                    }]
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(replace_models_response.status(), StatusCode::OK);
        let replaced = response_json(replace_models_response).await;
        assert_eq!(replaced["models"].as_array().unwrap().len(), 1);
        assert_eq!(replaced["models"][0]["publicModel"], "haiku-public");

        let get_models_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri(format!("/proxy/v1/channels/{channel_id}/models"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(get_models_response.status(), StatusCode::OK);
        let models = response_json(get_models_response).await;
        assert_eq!(models["models"][0]["upstreamModel"], "upstream-haiku");

        let test_response = Service::call(
            &mut router,
            json_request(
                Method::POST,
                &format!("/proxy/v1/channels/{channel_id}/test"),
                json!({
                    "model": "missing-model",
                    "interfaceKind": "openai_responses"
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(test_response.status(), StatusCode::OK);
        let test_result = response_json(test_response).await;
        assert_eq!(test_result["channelId"], channel_id);
        assert_eq!(test_result["providerId"], "a");
        assert_eq!(test_result["model"], "missing-model");
        assert_eq!(test_result["modelAvailable"], false);
        assert_eq!(test_result["success"], false);
        assert!(test_result["failureReason"]
            .as_str()
            .unwrap()
            .contains("model not mapped"));

        let delete_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/proxy/v1/channels/{channel_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);
        let deleted = response_json(delete_response).await;
        assert_eq!(deleted["deleted"], true);
        assert!(db.get_proxy_channel(&channel_id).unwrap().is_none());
    }

    #[tokio::test]
    async fn app_model_management_route_lists_routable_models() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_API_KEY": "secret-key"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();

        let server = ProxyServer::new(ProxyConfig::default(), db, None);
        let mut router = server.build_router();

        for body in [
            json!({
                "providerId": "a",
                "appType": "claude",
                "name": "Default Responses",
                "baseUrl": "https://default.example.com/v1",
                "interfaceKind": "openai_responses",
                "groups": ["default"],
                "priority": 100,
                "models": [{
                    "publicModel": "sonnet-public",
                    "upstreamModel": "upstream-sonnet"
                }]
            }),
            json!({
                "providerId": "a",
                "appType": "claude",
                "name": "Beta Responses",
                "baseUrl": "https://beta.example.com/v1",
                "interfaceKind": "openai_responses",
                "groups": ["beta"],
                "priority": 90,
                "models": [{
                    "publicModel": "haiku-public",
                    "upstreamModel": "upstream-haiku"
                }]
            }),
            json!({
                "providerId": "a",
                "appType": "claude",
                "name": "Embeddings",
                "baseUrl": "https://embeddings.example.com/v1",
                "interfaceKind": "embeddings",
                "groups": ["default"],
                "priority": 80,
                "models": [{
                    "publicModel": "embedding-3",
                    "upstreamModel": "embedding-3"
                }]
            }),
        ] {
            let response = Service::call(
                &mut router,
                json_request(Method::POST, "/proxy/v1/channels", body),
            )
            .await
            .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let default_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps/claude/models?group=default&interface=anthropic_messages")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(default_response.status(), StatusCode::OK);
        let default_models = response_json(default_response).await;
        assert_eq!(default_models["routeGroup"], "default");
        assert_eq!(default_models["interfaceKind"], "anthropic_messages");
        let models = default_models["models"].as_array().unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["publicModel"], "sonnet-public");
        assert_eq!(models[0]["channelName"], "Default Responses");

        let beta_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps/claude/models?group=beta")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(beta_response.status(), StatusCode::OK);
        let beta_models = response_json(beta_response).await;
        let models = beta_models["models"].as_array().unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["publicModel"], "haiku-public");
    }

    #[tokio::test]
    async fn channel_migration_management_routes_preview_materialize_and_reset_breaker() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let provider = Provider::with_id(
            "a".to_string(),
            "Provider A".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://primary.example.com/v1",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider).unwrap();

        let server = ProxyServer::new(ProxyConfig::default(), db.clone(), None);
        let mut router = server.build_router();

        let preview_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::GET)
                .uri("/proxy/v1/apps/claude/channels/migration/preview")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(preview_response.status(), StatusCode::OK);
        let preview = response_json(preview_response).await;
        assert_eq!(preview["appType"], "claude");
        assert_eq!(preview["channels"].as_array().unwrap().len(), 1);

        let materialize_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::POST)
                .uri("/proxy/v1/apps/claude/channels/migration/materialize")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(materialize_response.status(), StatusCode::OK);
        let materialize = response_json(materialize_response).await;
        assert_eq!(materialize["appType"], "claude");
        assert_eq!(materialize["insertedChannels"], 1);
        assert_eq!(materialize["insertedHealthRows"], 1);

        let stored = db.list_proxy_channels_for_app("claude").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].base_url, "https://primary.example.com/v1");

        let mut config = db.get_proxy_config_for_app("claude").await.unwrap();
        config.circuit_failure_threshold = 1;
        config.circuit_timeout_seconds = 60;
        db.update_proxy_config_for_app(config).await.unwrap();

        let channel_id = stored[0].id.clone();
        server
            .state
            .provider_router
            .record_channel_result(
                &channel_id,
                "claude",
                false,
                false,
                Some("upstream failed".to_string()),
                Some(123),
            )
            .await
            .unwrap();

        let blocked = server
            .state
            .provider_router
            .resolve_channel_route_dry_run(RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            })
            .await
            .unwrap();
        assert!(blocked.candidates.is_empty());

        let reset_response = Service::call(
            &mut router,
            Request::builder()
                .method(Method::POST)
                .uri(format!("/proxy/v1/channels/{channel_id}/breakers/reset"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(reset_response.status(), StatusCode::OK);
        let reset = response_json(reset_response).await;
        assert_eq!(reset["channelId"], channel_id);
        assert_eq!(reset["appType"], "claude");
        assert_eq!(reset["reset"], true);

        let recovered = server
            .state
            .provider_router
            .resolve_channel_route_dry_run(RouteResolveRequest {
                app_type: "claude".to_string(),
                requested_model: Some("claude-sonnet-4".to_string()),
                interface_kind: Some("anthropic_messages".to_string()),
                route_group: None,
            })
            .await
            .unwrap();
        assert_eq!(recovered.candidates.len(), 1);
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        serde_json::from_slice(&bytes).expect("json response")
    }

    fn json_request(method: Method, uri: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }
}
