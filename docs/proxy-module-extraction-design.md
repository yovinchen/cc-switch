# 代理模块抽离与独立中转接口设计

> 分支：`refactor-proxy-channel-migration-module`
> 基线：`origin/main`
> 日期：2026-06-18
> 状态：已进入分阶段实现

## 当前落地状态

截至 2026-06-18，本分支已经完成以下迁移切片：

1. 新增 host-neutral `proxy_core` 领域模型和 `ProxyServices` 端口，主工程通过 `cc-switch-proxy-core` path crate 引用。
2. 新增 `proxy_channels`、`proxy_channel_models`、`proxy_channel_health` 等 channel 存储，并提供 legacy provider/endpoints 到 channel 的兼容投影。
3. 路由规划已进入 `ProxyEngine`：`plan_route` 支持 legacy projection，`plan_materialized_route` 只使用显式 materialized channel。
4. 现有 live `RequestForwarder` 的 materialized channel 尝试已改为经 `ProxyEngine` 规划，再映射回 host `ForwardAttempt` 执行，保持旧转发链路不回归。
5. `ProxyEngine::handle` 已拥有编排骨架：请求计数、route selected 事件、`ForwardPipeline` 端口调用、`UsageSink` 调用。
6. `CcSwitchEventSink` 已桥接到现有 `ProxyEventBus`，核心事件可以进入 `/proxy/v1/events` 的 SSE 流。
7. `UsageSink` 已升级为完整 `UsageRecord` 并可通过 `CcSwitchUsageSink` 写入现有 `proxy_request_logs`；response pipeline 与 handler 转换路径的成功 usage、forward error 日志都已改为走 `UsageSink`，app-specific 响应转换仍在 host 层。
8. 核心响应体已从请求 `ProxyBody` 拆出为 `ProxyResponseBody`，可以表达 empty/json/bytes/stream，避免把 axum/hyper 类型带入 core crate。
9. `RequestForwarder` 的重试循环已从 attempts 构建中拆出，新增 preplanned attempts 入口，并且预规划入口不再强制持有 `CcSwitchProxyServices`，避免 host runtime adapter 产生自引用。
10. `CcSwitchForwardPipeline` 已接入运行中 `ProxyServer` 的共享 router/status/event/history/failover 运行态，可以把 `ProxyEngine::handle` 的 `RoutePlan` 映射为 host `ForwardAttempt` 并复用现有 HTTP 转发链；后续重点转为迁移 response pipeline、模型列表接口和外部管理 API。
11. Codex `/v1/chat/completions` handler 已改为构造 neutral `ProxyRequest` 并进入 `ProxyEngine::handle`；返回的 `ProxyResult` 暂时桥接回旧 `process_response`，保留现有 usage 解析、流式处理和响应构造。
12. Gemini handler 已改为进入 `ProxyEngine::handle`；模型名继续从 URI 提取，无模型的 `/models` 类端点不会把 `unknown` 写入 route filter，避免误过滤 channel。
13. Codex `/v1/responses` 与 `/v1/responses/compact` handler 已进入 `ProxyEngine::handle`；chat-to-responses 转换仍在 host 层执行，等待后续 response pipeline 迁移。
14. Claude 与 Claude Desktop `/v1/messages` handler 已进入 `ProxyEngine::handle`；核心 `ProxyResult` 会带回 `claudeApiFormat` 等宿主 metadata，host 侧继续复用现有格式转换、SSE/非流式响应处理和用量解析。
15. `ProxyEngine::list_models` 已提供按 app/group/interface 过滤的可路由模型视图，复用 channel source 的 legacy projection；`ProxyEngine::list_model_catalog` 负责生成 `/proxy/v1/apps/{app}/models` 的管理 API response envelope，返回模型对应的 provider/channel/interface 路由信息。
16. Codex 兼容 `/v1/models` 已从 handler 直读配置迁到 `ModelCatalogProvider::load_client_catalog` 与 `ProxyEngine::client_model_catalog`；CC Switch host adapter 保留 `model_catalog_json` stale guard 和 raw catalog 返回语义。
17. `/proxy/v1/channels/{channel_id}/breakers/reset` 已从 handler 直连 DB/router 改为 `ProxyEngine::reset_channel_health`；`CcSwitchHealthStore` 负责同时清内存 circuit breaker 和持久化健康状态。
18. response pipeline 中的 hop-by-hop 响应头清理和重建 body 后实体头清理已迁入 `proxy-core::response_headers`，host `response_processor` 与特殊响应转换分支复用 core helper。
19. response pipeline 的 body 诊断摘要、content header 诊断后缀、SSE 聚合兜底失败诊断消息和未标记 SSE body 嗅探已迁入 `proxy-core::response_diagnostics`，host 只负责把诊断文本包装成现有 `ProxyError`。
20. SSE field 解析、SSE block 分帧和跨 chunk UTF-8 拼接已迁入 `proxy-core::sse`；host `proxy::sse` 仅保留兼容 re-export，现有 Claude/OpenAI/Responses/Gemini/Codex 流式转换路径继续复用同一实现。
21. 非流式兜底使用的 Chat Completions SSE 与 OpenAI Responses SSE 聚合器已迁入 `proxy-core::sse`；host handler 只保留 `ProxyError` 映射和缺失 chat completion id 时的 UUID 生成适配。
22. 非流式响应体的 `content-encoding` 提取和 gzip/x-gzip/deflate/br 解压算法已迁入 `proxy-core::response_body`；host `response_processor` 只保留 body 读取超时、日志和 transport 适配。
23. SSE data 行扫描、跨 chunk UTF-8 缓冲、`[DONE]` 判定和可选 JSON parse 已迁入 `proxy-core::sse::SseEventScanner`；host `response_processor` 只负责 stream timeout、日志、collector 回调和 usage 落库。
24. SSE usage 事件缓存、首个被收集事件计时和 finish-once 防重入已迁入 `proxy-core::sse::SseUsageAccumulator`；host `SseUsageCollector` 只保留异步互斥、usage 事件预过滤、parser/model extractor 回调和 `UsageSink` 落库适配。
25. Claude/OpenAI/Codex/Gemini 的 SSE usage 事件预过滤函数已迁入 `proxy-core::sse`；host `handler_config` 只负责把 core 过滤函数挂入各协议 parser 配置。
26. `TokenUsage` 与 Claude/OpenAI/Codex/Gemini 的 usage JSON 解析器已迁入 `proxy-core::usage`；host `proxy::usage::parser` 仅保留兼容 re-export，usage request_id 的 message_id/session 前缀与 fallback 决策已迁入 core，host `usage_sink_bridge` 只注入随机 UUID 生成器。
27. Claude/OpenAI/Codex/Gemini 的流式 usage model extractor 已迁入 `proxy-core::usage`；host `handler_config` 只保留协议 parser 配置表和 app 绑定。
28. `TokenUsage` 到 `UsageTokens` 的映射、success/error usage record 的 neutral record 构造、request/outbound/response model 归因规则已迁入 `proxy-core::usage`；host `usage_sink_bridge` 只保留 UUID 生成器注入和 provider meta 到 `ProviderKind` 的适配。
29. `UsageRecord` 到 `TokenUsage` 的 host 回填转换、pricing model override/request/response 选择规则已迁入 `proxy-core::usage`；`CcSwitchUsageSink` 只负责读取 DB 计价配置、查询定价、执行 Decimal 成本计算并写入 `UsageLogger`。
30. 非流式 body timeout 与流式 first-byte/idle timeout 的 failover-gated 选择规则已迁入 `proxy-core::response_timeout`；host `RequestContext` 只把 app 配置传入 core，并把返回的 `Duration`/`StreamingTimeoutConfig` 接到现有 transport。
31. Claude transform 是否走 streaming，以及 Codex OAuth Responses 非流请求是否聚合上游 SSE 的路由策略已迁入 `proxy-core::response_transform`；host 只负责识别 provider type 并执行对应的 stream/non-stream transport 分支。
32. 非流式 response body decode outcome、未知编码/失败解码原样透传策略，以及成功解码后的 entity header 清理已收敛到 `proxy-core::response_body::decode_response_body`；host 只根据 outcome 打日志。
33. route-visible 模型列表的管理 API envelope 已迁入 `proxy-core::RoutableModelList` 与 `ProxyEngine::list_model_catalog`；host `/proxy/v1/apps/{app}/models` 只解析 query 并返回 typed JSON。
34. Codex 转发层代理错误的 Responses 风格 JSON envelope、上游错误体归一化和 413 上游体积限制提示已迁入 `proxy-core::codex_error`；host 只把 `ProxyError` 映射成 fallback message/code/status/body。
35. `/proxy/v1/channels/{channel_id}/breakers/reset` 的管理 API response envelope 已迁入 `proxy-core::ChannelHealthResetResponse` 与 `ProxyEngine::reset_channel_health_response`；host 只做 path 解析和 typed JSON 返回。
36. `/proxy/v1/channels/{channel_id}` DELETE 与 `/proxy/v1/channels/{channel_id}/models` GET/PUT 的管理 API response envelope 已迁入 `proxy-core::ChannelDeleteResponse` 与泛型 `ChannelModelsResponse<T>`；host 保留 DB CRUD 和 path 解析。
37. `/proxy/v1/groups` 的 route group 聚合规则和 response envelope 已迁入 `proxy-core::RouteGroupListResponse`；host 只负责按 app 查询可见 channel，并把 app/source/groups 投影为 core 输入。
38. `/proxy/v1/channels` GET 的管理 API list envelope 已迁入泛型 `proxy-core::ChannelListResponse<T>`；host 继续负责 DB 列表查询和可选 app 过滤。
39. `/proxy/v1/apps` GET 的管理 API list envelope 已迁入 `proxy-core::AppListResponse` 与 `AppSummary`；host 继续负责读取 app 配置、provider 数量和 channel 数量。
40. `/proxy/v1/apps/{app}/providers` GET 的脱敏 provider summary 与 response envelope 已迁入 `proxy-core::ProviderListResponse`/`ProviderSummary`；host 继续负责 provider 查询、current/failover/routeCandidate 计算。
41. `/proxy/v1/apps/{app}/channels` GET 的普通 channel 列表与带过滤 dry-run 两种 response envelope 已迁入 `proxy-core::AppChannelResponse`；host 继续负责 provider router 查询和 route filter 解析。
42. `/proxy/v1/apps/{app}/routes/current` GET 的当前路由 response envelope 与 configured provider summary 已迁入 `proxy-core::CurrentRouteResponse<T>`/`CurrentRouteProviderSummary`；host 继续负责 active target runtime map 和当前 provider 查询。
43. `/proxy/v1/apps/{app}/channels/migration/preview` 与 `/materialize` 的 response envelope 已迁入 `proxy-core::ChannelMigrationPreviewResponse<T>`/`ChannelMigrationMaterializeResponse`；host 继续负责旧 provider/endpoint 投影与 DB 写入。
44. `/proxy/v1/route/resolve` 的 request/response/candidate/rejected/source API contract 已迁入 `proxy-core::{RouteResolveRequest, RouteResolveResponse, ChannelRouteCandidate, ChannelRouteRejected, ChannelRouteSource}`；host `channel_routing` 只保留基于 DB channel record 的 dry-run 解析算法。
45. `/proxy/v1/route/resolve` 的 dry-run 过滤、淘汰原因生成、兼容接口匹配和候选排序算法已迁入 `proxy-core::route_resolve::resolve_channel_route`；host `channel_routing` 缩小为 `ProxyChannelRecord -> RouteResolveChannelInput` 适配层和 `AppError` 映射。
46. `/proxy/v1/channels` POST/PATCH 与 `/proxy/v1/channels/{channel_id}/models` PUT 的 request DTO 已迁入 `proxy-core::{ProxyChannelWriteRequest, ProxyChannelPatchRequest, ProxyChannelModelWriteRequest, ProxyChannelModelsReplaceRequest}`；host DB DAO 继续负责持久化、规范化和 provider/app 校验。
47. `/proxy/v1/health` 的 response contract 已迁入 `proxy-core::HealthCheckResponse`；host 只负责注入当前 RFC3339 时间并返回 typed JSON。
48. `/proxy/v1/apps/{app}/models` 与 `/proxy/v1/apps/{app}/channels` 的 query DTO 和别名归一化已迁入 `proxy-core::{AppModelListQuery, AppChannelListQuery}`；host handler 只负责 axum query 提取和调用 core/adapter。
49. `ChannelRouteSource` 的外部 source label 和 `/proxy/v1/apps/{app}/channels` list/route response 组装已迁入 `proxy-core::{ChannelRouteSource::as_str, AppChannelListResponse::from_route_source, AppChannelRouteResponse::from_route_resolve}`；host 不再手写 source 字符串映射。
50. 管理 API 鉴权策略已迁入 `proxy-core::management_auth`；host middleware 只负责读取 `ProxyConfig`/环境变量/header，并把 core 鉴权错误映射为现有 `ProxyError::AuthError`。
51. `/proxy/v1/apps/{app}/providers` 的 provider summary 打标和 response 组装已迁入 `proxy-core::{ProviderSummaryInput, ProviderListResponse::from_provider_inputs}`；host 只负责查询 provider/current/failover/routeCandidate 输入集合。
52. legacy/manual channel 的幂等 ID 生成规则已迁入 `proxy-core::channel_identity::stable_channel_id`；DB DAO 只负责调用 core 函数并写入 schema。
53. legacy channel 投影的 priority、interface kind 和模型路由推断规则已迁入 `proxy-core::legacy_projection`；DB DAO 只负责把宿主 `Provider`/TOML/env/meta 字段适配成 `LegacyProviderProjectionInput` 并构造持久化 record。
54. legacy channel 投影的默认字段、review 标记、metadata、auth profile 引用和 model channel id 绑定已迁入 `proxy-core::LegacyChannelProjection`；DB DAO 只负责把 core projection 映射回当前 SQLite record 形状。
55. `/proxy/v1/channels` 写请求的字段校验、base URL 归一化、group 去重排序、可选 auth ref 裁剪和 JSON object/array 默认值规则已迁入 `proxy-core::channel_request`；host DB DAO 只保留 AppType/provider 校验、错误映射和持久化。
56. 托管账号上游的 `PROXY_MANAGED` 占位符泄漏保护已迁入 `proxy-core::managed_account_auth`；host forwarder 只负责把 core guard 错误映射为现有 `ProxyError::AuthError`。
57. 请求头大小写保真策略已迁入 `proxy-core::request_headers::should_preserve_exact_request_header_case`；host forwarder 只提供 adapter/provider 事实并据此选择 raw hyper 或 pooled reqwest transport。
58. 上游请求体准备中的私有字段过滤、JSON Schema 名称保护和稳定 key 排序已迁入 `proxy-core::request_body`；host forwarder 只负责调用 core report 并保留 debug 日志。
59. 上游请求 transport policy 中的流式请求识别和 `Accept-Encoding: identity` 强制规则已迁入 `proxy-core::request_transport`；host forwarder 只传入 transform/endpoint/body/header 事实并消费 core policy。
60. 上游请求体发送策略中的 GET/HEAD 空 body 规则和 JSON body 序列化已迁入 `proxy-core::request_body`；host forwarder 只负责把序列化错误映射成现有 `ProxyError`。
61. Codex OAuth 上游 session 路由头构造已迁入 `proxy-core::request_headers::build_codex_oauth_session_headers`；host forwarder 保留“只发送客户端提供 session_id”的 gating，并消费 core 生成的 header 集合。
62. 上游请求 header 清理中的连接、追踪和 CDN 类 header strip policy 已迁入 `proxy-core::request_headers::should_strip_forwarded_request_header`；host forwarder 继续负责提供原始 header 与 provider-specific 输入。
63. 上游请求 URL/query 处理中 endpoint query 拆分、beta 参数剥离、Gemini `alt` 参数合并和 full URL query 追加 helper 已迁入 `proxy-core::request_url`；host forwarder 继续负责协议特定 endpoint rewrite 决策。
64. Codex Responses 到 Chat endpoint rewrite、Claude transform endpoint 目标选择、Copilot/Responses/Gemini Native 目标路径和透传 query 组装已迁入 `proxy-core::request_url`；host forwarder 只负责提取并规范化 Gemini 模型作为 core 输入。
65. 转发规划前的 request model 推断、Gemini path 模型提取和 inbound interface kind 推断已迁入 `proxy-core::request_url`；host forwarder/handler_context 只保留兼容 wrapper。
66. 原生 Anthropic 上游请求头策略中的发送条件、`claude-code-20250219` beta 合并和默认 `anthropic-version` 已迁入 `proxy-core::request_headers`；host forwarder 只负责把 provider/app 事实传入 core。
67. Copilot 指纹请求头去重策略已迁入 `proxy-core::request_headers::should_skip_copilot_fingerprint_request_header`；host forwarder 继续负责 Copilot auth header 注入输入。
68. 上游请求有序 HeaderMap 组装、认证头替换位置、`accept-encoding: identity` 补齐、User-Agent 覆写、Anthropic/Copilot/Codex 会话头写入和默认 JSON content-type 补齐已迁入 `proxy-core::request_headers::build_upstream_request_headers`；host forwarder 只提供 auth/session/UA/upstream host 等宿主输入。
69. 上游认证 header 的后处理规则已迁入 `proxy-core::request_headers::build_upstream_auth_headers`；Codex OAuth `chatgpt-account-id` 注入、Copilot optimizer 的 `x-initiator`/`x-interaction-type`/请求 ID 覆写和 `x-interaction-id` 追加由 core 统一处理，host forwarder 只负责通过宿主 manager 解析 token、账号和分类事实。
70. Copilot 上游判定和动态 endpoint 替换条件已迁入 `proxy-core::request_url::{is_github_copilot_upstream, should_resolve_copilot_dynamic_endpoint, resolved_copilot_dynamic_base_url}`；host forwarder 只负责从 `CopilotAuthManager` 读取账号对应 endpoint 并把运行时事实交给 core 决策。
71. provider 可重试失败、单 provider 失败、全部 provider 失败的日志 code/message 策略和上游错误摘要规则已迁入 `proxy-core::forward_failure`；host forwarder 只负责把宿主 `ProxyError` 映射为中立 `ForwardFailureKind` 并执行实际日志输出。
72. provider failover eligibility 与健康度污染边界已迁入 `proxy-core::forward_failure::categorize_forward_failure`；400/405/406/413/414/415/422/501 等客户端请求错误不再由 host forwarder 本地判定，host 只负责把 `ProxyError` 映射为 `ForwardFailureKind`。
73. thinking/media rectifier 重试失败后的 provider/client 归因规则已迁入 `proxy-core::forward_failure::should_failover_after_rectifier_retry_failure`；timeout/forward failed/5xx 继续故障转移，其它整流后失败按客户端侧失败处理，host forwarder 只保留状态更新和 permit 释放。
74. media fallback 的开关组合、预防式图片替换 gate、反应式图片重试 gate 和 Claude/Codex adapter 限制已迁入 `proxy-core::request_media`；host forwarder 继续负责 provider schema 图片能力读取、JSON 图片块替换和 `ProxyError` 到“上游不支持图片”事实的适配。
75. Bedrock pre-send optimizer 的启用 gate 已迁入 `proxy-core::request_optimizer`；host forwarder 只负责从 provider `settings_config.env.CLAUDE_CODE_USE_BEDROCK` 提取事实，并继续执行 thinking optimizer/cache injector 的实际 body mutation。
76. 上游发送 transport 选择、SOCKS5 代理识别、默认 600 秒发送超时、流式 reqwest 24h 请求超时和首包等待超时选择已迁入 `proxy-core::request_transport`；host forwarder 只负责执行 core 选出的 pooled reqwest/raw hyper 路径并映射 transport 错误。
77. `request_started` 与 provider/channel attempt 事件名、事件 payload envelope 已迁入 `proxy-core::event_payload`；host forwarder 只负责把请求生命周期、`ForwardAttempt`/provider/channel 字段适配为 core 输入，并继续通过现有 `ProxyEventBus` 对外推送。
78. Copilot optimizer 的 session id 提取优先级（`metadata.user_id` 的 `_session_` 后缀、`metadata.session_id`、raw `metadata.user_id`、`x-session-id` header）已迁入 `proxy-core::request_optimizer`；host forwarder 继续负责 deterministic request/interaction id 的具体生成。
79. Copilot optimizer 的 deterministic request id 与 interaction id 哈希策略、最后一条 user 内容选择、tool_result/cache_control 排除、UUID v4 格式化和无 user 内容时的 request id fallback 决策已迁入 `proxy-core::request_optimizer`；host forwarder 只负责注入随机 UUID 生成器。
80. Copilot optimizer 的 warmup 小模型降级 gate、目标模型选择和实际 JSON `model` 覆写已迁入 `proxy-core::request_optimizer`；host forwarder 只负责记录已应用的降级模型日志。
81. Copilot optimizer 的 initiator/warmup/compact/subagent 请求分类策略已迁入 `proxy-core::request_optimizer`；host `copilot_optimizer` 保留兼容 wrapper。
82. Copilot optimizer 的 assistant thinking/redacted_thinking block 剥离 mutation 已迁入 `proxy-core::request_optimizer`；host `copilot_optimizer::strip_thinking_blocks` 只保留兼容 wrapper，forwarder 调用顺序不变。
83. Copilot optimizer 的 orphan tool_result sanitize mutation 已迁入 `proxy-core::request_optimizer`；core 固化“只匹配紧邻上一条 assistant 的 tool_use”的 Anthropic 协议语义，host `sanitize_orphan_tool_results` 只保留兼容 wrapper。
84. Copilot optimizer 的 tool_result/text block 合并 mutation 已迁入 `proxy-core::request_optimizer`；core 负责消息内 text 吸收与连续 tool_result-only user 消息合并，host `merge_tool_results` 只保留兼容 wrapper。
85. Copilot optimizer 的生产调用点已从 host `copilot_optimizer` wrapper 改为直接调用 `proxy-core::request_optimizer`；host wrapper 模块仅在测试构建中保留，用作迁移期行为回归测试面。
86. 非流式响应 JSON 解析失败后的错标 SSE 嗅探、Chat/Responses 聚合选择和解析/聚合失败诊断消息已迁入 `proxy-core::response_parse`；host `handlers` 只负责按协议传入可用聚合器、注入缺失 chat completion id 的 UUID，并把 core 错误映射回现有 `ProxyError`。
87. Codex Chat 上游错误体的 JSON/文本解析、非 JSON 预览截断和 Responses 风格 error envelope 归一化已迁入 `proxy-core::codex_error::normalize_codex_chat_error_body`；host 只负责读取 body、记录非 JSON 预览日志、清理响应头并构造 Axum response。
88. 转换后 JSON 响应的实体/hop-by-hop/header content-type 重建策略，以及转换后 SSE 响应的固定 `text/event-stream`/`no-cache` 头，已迁入 `proxy-core::response_headers`；host 只负责把 core header 集合写入 Axum response builder。
89. 转换后 JSON 响应的 header 重建、body 序列化和 neutral `ProxyCoreResponse` 构造已迁入 `proxy-core::response_build::rebuilt_json_proxy_response`；host `handlers` 只保留 `ProxyCoreResponse -> Axum Response` transport adapter 和错误映射。
90. 转换后 SSE 响应的固定 header、OK status 和 stream body neutral `ProxyCoreResponse` 构造已迁入 `proxy-core::response_build::transformed_sse_proxy_response`；host `handlers` 只保留 transform stream 生成、usage/timeout 包装和 Axum transport adapter。
91. `ProxyCoreResponse -> Axum Response` 与 `ProxyCoreResponse -> hyper_client::ProxyResponse` 的宿主 transport 桥接已从 `handlers` 抽到 `proxy::response_adapter`；handler 只保留协议入口编排和错误语义映射。
92. 转换后非流式响应的 usage 提取、全 0 usage 跳过和 response/outbound/request model 归因 fallback 已迁入 `proxy-core::usage::transformed_response_usage`；host `handlers` 只负责把 core 归因结果交给现有 `log_usage`。
93. `ProxyCoreError -> ProxyError` 的宿主错误适配已从 `handlers` 抽到 `proxy::error_mapper::proxy_core_error_to_proxy_error`；handler 不再承载 core/host 错误类型映射表。
94. response body parse/aggregation 专用错误适配已从 `handlers` 抽到 `proxy::error_mapper::response_body_parse_error_to_proxy_error`；core `Upstream` 诊断继续映射为 host `TransformError`，避免被通用 core error 映射当作转发失败。
95. Codex 代理错误响应的 `ProxyError -> Responses error body` 宿主分类已从 `handlers` 抽到 `proxy::error_mapper::codex_proxy_error_json`，JSON body 的 neutral response 构造已迁入 `proxy-core::response_build::json_proxy_response`；handler 只负责计算 HTTP status 并桥接 Axum response。
96. `/proxy/v1/channels` 与 `/proxy/v1/groups` 的 query DTO 已迁入 `proxy-core::{ChannelListQuery, GroupListQuery}`；host handler 只负责 Axum query 提取、appType 校验和 DB/router 查询。
97. 管理 API 的 appType、route resolve appType 和 channel_id path 参数校验已迁入 `proxy-core::management_api`；host handler 只负责调用 core 校验并通过 `management_api_error_to_proxy_error` 保留旧 InvalidRequest 文案。
98. `/proxy/v1/channels` POST 与 `/proxy/v1/channels/{channel_id}` GET/PATCH 已从 `Json<Value>` 和 `json!(channel)` 改为 typed `Json<ProxyChannelRecord>` 返回；外部 JSON shape 不变，handler 不再手写单条 channel record 序列化。
99. 管理 API 鉴权错误的 `ManagementAuthError -> ProxyError` 宿主适配已从 `handlers` 抽到 `proxy::error_mapper::management_auth_error_to_proxy_error`；handler 只保留 header 提取和 core bearer 校验调用。
100. 管理 API bearer `Authorization` header 解析已迁入 `proxy-core::management_auth::validate_management_bearer_header`；host middleware 不再手写 header parse，只把 Axum/header map 交给 core 并映射错误。
101. Claude Desktop gateway 的 bearer header/token 校验和错误文案已迁入 `proxy-core::claude_desktop_gateway_auth`；host handler 只负责从 `claude_desktop_config` 读取/生成 gateway token 并映射 core auth 错误。
102. Claude 非流响应转换中未标记 SSE body 的 api_format -> 聚合策略映射已迁入 `proxy-core::response_transform::claude_transform_unlabeled_sse_aggregation`；handler 只把 core 策略交给 JSON/SSE parse helper。
103. `ProxyResult.metadata` 中 Claude API format 的 key 与 fallback 读取规则已迁入 `proxy-core::{CLAUDE_API_FORMAT_METADATA_KEY, claude_api_format_from_metadata}`；host forward adapter 和 handler 不再各自硬编码 `claudeApiFormat`。
104. channel `interface_kind` 到 Claude/Codex 上游 `api_format` 的映射已迁入 `proxy-core::InterfaceKind`；host `route_attempt` 只把 core 返回的 format 写回当前兼容 Provider meta。
105. Codex provider 的 chat wire API 别名、chat completions URL 和 Responses endpoint 转换条件已迁入 `proxy-core::request_url`；host provider 只负责从 Provider/TOML 读取候选配置值。
106. legacy channel projection 的 Codex `wire_api` 别名判断已复用 `proxy-core::request_url::is_codex_chat_wire_api`，避免迁移投影与运行时 provider adapter 维护两份 chat API alias 列表。
107. Codex provider `build_url` 的 origin-only base URL 判定已迁入 `proxy-core::request_url::is_origin_only_url`；host adapter 只负责按 core 判定结果执行兼容拼接。
108. Claude `api_format` 是否需要 Anthropic <-> 上游格式转换的纯规则已迁入 `proxy-core::response_transform::claude_api_format_needs_transform`；host provider 只负责解析当前 Provider 的 api_format。
109. Claude `api_format` 解析优先级（Codex OAuth 强制 Responses、meta 优先、legacy settings、openrouter compat fallback、默认 anthropic）已迁入 `proxy-core::response_transform::resolve_claude_api_format`；host provider 只负责抽取 Provider 字段。
110. Codex provider 是否使用 Chat Completions 上游的配置优先级（显式 api_format、TOML wire_api、显式 base_url、TOML base_url）已迁入 `proxy-core::request_url::resolve_codex_provider_uses_chat_completions`；host provider 只负责抽取 Provider/TOML 候选值。
111. OpenAI o-series/GPT-5+ reasoning model 识别，以及 Anthropic `output_config.effort`/`thinking` 到 OpenAI `reasoning_effort` 的映射已迁入 `proxy-core::request_body`；host `transform` 仅保留兼容 re-export 和实际格式转换编排。
112. Claude Code 动态 `x-anthropic-billing-header` system 前缀剥离规则已迁入 `proxy-core::request_body::strip_leading_anthropic_billing_header`；OpenAI Chat/Responses host transform 只通过兼容 re-export 复用该请求体规范化策略。
113. Anthropic `tool_choice` 到 OpenAI Chat Completions nested function selector 的映射已迁入 `proxy-core::request_body::map_anthropic_tool_choice_to_openai_chat`；host `transform` 只在实际 Chat 请求转换编排中调用 core helper。
114. Anthropic `tool_choice` 到 OpenAI Responses flat function selector 的映射已迁入 `proxy-core::request_body::map_anthropic_tool_choice_to_openai_responses`；host `transform_responses` 只保留 Responses 请求转换编排。
115. OpenAI Responses `status`/`incomplete_details.reason`/tool-use 到 Anthropic `stop_reason` 的映射已迁入 `proxy-core::response_transform::map_openai_responses_stop_reason_to_anthropic`；host `transform_responses` 只负责从响应体抽取输入事实。
116. OpenAI Responses usage 到 Anthropic fresh input/output/cache bucket 的 JSON 组装已迁入 `proxy-core::usage::build_anthropic_usage_from_openai_responses`；host 非流式与流式 Responses transform 只传入上游 usage 节点。
117. OpenAI Responses function-call arguments 到 Anthropic `tool_use.input` 的 `Read` 空 `pages` 清理规则已迁入 `proxy-core::response_transform::{sanitize_anthropic_tool_use_input,sanitize_anthropic_tool_use_input_json}`；host 非流式与流式 Responses transform 只负责选择 Value 或 JSON 字符串入口。
118. OpenAI Chat Completions 流式请求 `stream_options.include_usage` 注入规则已迁入 `proxy-core::request_body::inject_openai_stream_include_usage`；Claude→Chat 与 Codex Responses→Chat 两个 host 转换路径直接复用 core helper。
119. OpenAI Chat/Responses 工具 JSON Schema 中不兼容的 `format: "uri"` 递归清理规则已迁入 `proxy-core::request_body::clean_openai_tool_schema`；host 转换层只负责选择 Anthropic tool 的 `input_schema`。
120. OpenAI Chat Completions `finish_reason` 到 Anthropic `stop_reason` 的映射已迁入 `proxy-core::response_transform::map_openai_chat_finish_reason_to_anthropic`；host 非流式与流式 Chat transform 只传入上游 finish_reason 和是否已有 tool_use 的事实。
121. OpenAI Chat Completions usage 到 Anthropic fresh input/output/cache bucket 的 JSON 组装已迁入 `proxy-core::usage::{build_anthropic_usage_from_openai_chat,build_anthropic_usage_from_openai_chat_tokens}`；host 非流式传入 usage 节点，流式传入已解析 token 计数。
122. Gemini `usageMetadata` 到 Anthropic fresh input/output/cache bucket 的 JSON 组装已迁入 `proxy-core::usage::build_anthropic_usage_from_gemini`；host 非流式与流式 Gemini transform 只传入上游 usageMetadata 节点。
123. Gemini `finishReason`/blocked 状态到 Anthropic `stop_reason` 的映射已迁入 `proxy-core::response_transform::map_gemini_finish_reason_to_anthropic`；host 非流式与流式 Gemini transform 只传入 finishReason、tool_use 和 blocked 事实。

因此，本分支目前已把主要转发入口（Claude Messages、Claude Desktop Messages、Codex Chat Completions、Codex Responses、Codex Responses Compact、Gemini Native）切到 `ProxyEngine`，并开始把管理查询类能力、Codex 客户端模型目录、legacy channel 投影构造、channel 写请求规范化、托管账号上游安全保护、请求头 transport 策略、请求 header strip policy、Anthropic request header policy、Copilot fingerprint header policy、Codex OAuth session header 构造、ordered request header assembly、upstream auth header finalization、Copilot endpoint selection、forward failure log policy、provider failure retry classification、rectifier retry failover classification、media fallback gate policy、Bedrock optimizer gate policy、usage request-id/model fallback policy、success/error usage record construction、transformed response usage attribution、SSE aggregate fallback diagnostics、非流式 JSON/SSE 解析兜底策略、Codex Chat 上游错误体归一化、Codex proxy error body 分类、转换响应 header 重建策略、转换后 JSON/SSE neutral response 构造、host response transport/error adapter、response parse error adapter、Copilot optimizer session/deterministic ID/fallback/warmup/classification policy、Copilot warmup model body override、Copilot thinking-strip/orphan-sanitize/tool-result-merge mutation、Copilot optimizer production call site、upstream send transport policy、proxy event payload contracts、上游请求体准备/发送策略、上游请求 transport policy、上游请求 URL/query helper、endpoint rewrite policy、route hint inference 和请求日志写入收敛到 core 可复用接口。app-specific response transform 仍是宿主层兼容桥；下一阶段需要把响应转换、剩余模型目录生成策略和剩余外部管理 API 继续收敛到独立代理模块边界内。

当前原则：核心 crate 可以新增端口和领域字段，但不得引入 `tauri`、`Database`、settings、commands、services 等宿主依赖；现有 runtime 行为必须继续通过 targeted tests 证明不回归。

## 背景

当前代理能力集中在 `src-tauri/src/proxy/` 与 `src-tauri/src/services/proxy.rs` 两处：

- `src-tauri/src/proxy/` 负责本地 HTTP server、路由、provider 选择、请求转发、协议转换、故障转移、熔断、响应处理、用量解析和用量落库。
- `src-tauri/src/services/proxy.rs` 负责桌面应用宿主逻辑，包括启动/停止代理、更新配置、接管/恢复 Claude/Codex/Gemini Live 配置、托盘/UI 联动。

这使代理能力目前更像“嵌入在 Tauri 应用里的功能模块”，而不是可独立集成的中转模块。重构目标是把可复用的代理中转能力抽离成独立模块，让 CC Switch 桌面端只是一个宿主适配器；未来外部进程、CLI、服务端网关或其他应用也可以通过稳定接口复用同一套中转能力。

## 目标

1. 把请求中转核心抽成独立代理模块，提供稳定的 Rust API 和 HTTP API。
2. 让代理核心不直接依赖 Tauri、SQLite `Database`、桌面配置文件路径、托盘事件或 `AppState`。
3. 保留现有行为：Claude、Claude Desktop gateway、Codex Responses/Chat、Gemini Native、本地路由、故障转移、熔断、格式转换、用量统计、header casing 保真。
4. 让 CC Switch 现有 UI/命令层继续可用，只是通过宿主适配器调用独立代理模块。
5. 支持将代理模块作为“集成中转”暴露给外部调用方：HTTP route、Rust library API、状态/事件/用量外部接口。
6. 支持中转站式 channel/route 配置：每个地址可以独立选择上游地址、接口协议、模型列表、模型映射、权重、优先级、key、header/参数覆盖和健康策略。

## 非目标

1. 不在本次重构中重写协议转换算法。
2. 不在第一阶段直接破坏现有数据库 schema；新增 channel/route 结构必须通过兼容投影和可回滚迁移落地。
3. 不把 Live 配置接管逻辑放进独立代理核心。接管 Claude/Codex/Gemini 配置是 CC Switch 桌面宿主职责。
4. 不新增包管理器或运行时依赖作为默认方案。
5. 不改变用户可见的默认端口、路由路径和代理开关语义。

## 当前结构分析

### 代码规模

`src-tauri/src/proxy` 当前约 4.5 万行 Rust，职责分布如下：

| 区域 | 代表文件 | 当前职责 |
| --- | --- | --- |
| HTTP server | `server.rs`, `handlers.rs` | Axum/hyper 服务、路由表、HTTP 请求转为代理请求 |
| 请求上下文 | `handler_context.rs` | 从 DB/settings 读取 app 配置、当前 provider、session、模型名 |
| 转发核心 | `forwarder.rs` | provider 重试、熔断记录、请求体处理、认证、上游请求、状态统计 |
| provider 适配 | `providers/*` | Claude/Codex/Gemini URL、认证、请求/响应转换、SSE 转换 |
| 响应处理 | `response_processor.rs` | 流式/非流式响应透传、解压、用量收集、日志落库 |
| 故障转移 | `provider_router.rs`, `circuit_breaker.rs`, `failover_switch.rs` | provider 选择、熔断状态、故障转移后切换当前 provider |
| 用量 | `usage/*` | token 解析、计价、请求日志写入 |
| 工具能力 | `media_sanitizer.rs`, `thinking_*`, `cache_injector.rs`, `body_filter.rs` | 请求前/错误后修正、优化、过滤 |

### 关键耦合点

当前 proxy 模块直接依赖这些宿主概念：

| 耦合对象 | 出现场景 | 抽离问题 |
| --- | --- | --- |
| `crate::database::Database` | `ProxyState`, `ProviderRouter`, `UsageLogger`, `FailoverSwitchManager` | 独立模块无法在无 SQLite 宿主中复用 |
| `tauri::AppHandle` | `ProxyState`, `RequestForwarder`, `FailoverSwitchManager` | 核心逻辑会被桌面事件系统锁死 |
| `crate::app_config::AppType` | handlers、adapter、provider type 推断 | 外部调用方无法定义自己的 app namespace |
| `crate::provider::Provider` | provider adapter、router、media sanitizer | provider 数据模型绑在 CC Switch 配置结构上 |
| `crate::settings` | 当前 provider 读取、有效 provider 读取 | 核心逻辑隐式读宿主全局配置 |
| `crate::commands::{CodexOAuthState, CopilotAuthState}` | `forwarder.rs` 认证刷新 | 认证管理和请求转发耦合 |
| `crate::services::usage_stats` | `UsageLogger` 定价查询 | 用量记录无法替换为外部 sink |
| `crate::claude_desktop_config`, `crate::codex_config` | model list、gateway auth | 协议入口混入桌面配置文件细节 |

### 当前请求链路

```text
HTTP request
  -> server.rs route
  -> handlers.rs parse body / decide app
  -> RequestContext::new
       -> db.get_proxy_config_for_app
       -> db.get_*_config
       -> settings::get_current_provider
       -> ProviderRouter::select_providers
  -> RequestForwarder::forward_with_retry
       -> provider adapter
       -> request transforms / optimizers / rectifiers
       -> hyper_client::send_request
       -> circuit + status + failover switch updates
  -> response_processor.rs
       -> streaming/non-streaming pass-through
       -> usage parser
       -> UsageLogger -> Database
  -> HTTP response
```

这个链路的问题不是“文件太多”，而是核心路径同时负责：

- 协议中转
- provider 决策
- 桌面状态同步
- DB 读写
- UI/托盘事件
- 用量计费
- Live 配置接管后的供应商切换

独立模块必须先把这些副作用变成接口，再迁移实现。

## 设计原则

1. **核心只认接口，不认宿主**：核心可以调用 `ProviderSource`、`UsageSink`、`EventSink`，但不能知道 SQLite、Tauri 或托盘。
2. **请求链路保持单入口**：HTTP route 和 Rust API 都应进入同一个 `ProxyEngine`，避免两套行为。
3. **协议转换先搬后改**：现有转换代码测试覆盖较多，迁移时只改变依赖方向，不做算法重写。
4. **状态显式化**：运行状态、channel 健康、provider 聚合健康、shadow history、Codex history、active connection 都由明确的 runtime state 管理。
5. **宿主适配薄化**：CC Switch 桌面端只提供配置、持久化、事件、认证、Live 接管，不再承载代理中转算法。
6. **外部接口稳定优先**：对外 HTTP API 使用版本化路径，内部文件结构可以逐步演进。
7. **Channel 是路由单元**：provider 只表示供应商/账号归属，真正参与选择、熔断、重试和模型映射的是 channel。

## 中转站参考模型

参考 NewAPI、One API 类中转站的结构，代理迁移不能继续把“地址”当作 provider 的附属 URL 字段。NewAPI 的公开说明把项目定位为 LLM gateway 和 AI 资产管理系统，覆盖组织鉴权、多模型管理、用量分析和成本核算；其能力清单包含 token 分组、模型限制、用量计费、OpenAI Responses/Realtime、Claude Messages、Gemini、rerank 等多接口支持、channel 加权随机、失败自动重试、模型级限流和 OpenAI/Claude/Gemini 格式转换。其 channel 数据模型也把 `base_url`、`models`、`group`、`model_mapping`、`status_code_mapping`、`priority`、`weight`、`auto_ban`、`param_override`、`header_override`、`setting`、`tag`、多 key 轮询/随机等字段放在 channel 层。

迁移到 CC Switch 时应吸收的是结构思想，不是照搬后台：

- **Provider**：供应商或账号归属，例如 Anthropic、OpenAI compatible、Gemini、GitHub Copilot、Claude Auth、Codex OAuth。
- **Channel**：可路由地址，也是一次上游尝试的最小单位。一个 provider 可以有多个 channel；同一 provider 下的不同 channel 可以指向不同 base URL、不同接口协议、不同模型集合、不同 key、不同权重和不同健康状态。
- **Interface**：该 channel 对外/对上游使用的协议族，例如 Anthropic Messages、OpenAI Chat Completions、OpenAI Responses、Gemini Native、Gemini OpenAI compatible、Azure OpenAI、自定义接口。
- **Model route**：对外暴露模型名与上游模型名的映射，同时携带能力、计价 key、参数覆盖和转换要求。
- **Route group**：面向用户、app 或场景的候选集合，用于表达默认组、付费组、低延迟组、备用组、私有地址组。

因此请求链路应从：

```text
app -> current provider / failover provider queue -> provider base_url
```

调整为：

```text
app + inbound interface + requested model + group
  -> route resolver
  -> channel candidates
  -> provider adapter + protocol transformer
  -> upstream endpoint
```

现有 `provider_endpoints` 可以作为 channel 的兼容来源，但不能继续作为最终设计。它目前只表达“provider 下的候选 URL”，缺少接口类型、模型集合、模型映射、权重/优先级、key 策略、参数覆盖、状态码映射和按 channel 健康状态等中转迁移必需字段。

### Channel 配置示例

同一个 provider 可以拆出多个 channel，每个 channel 独立选择地址、接口和模型：

```yaml
provider:
  id: openai-compatible-main
  kind: openrouter
  name: OpenAI Compatible Pool

channels:
  - id: ch-openai-chat-primary
    provider_id: openai-compatible-main
    name: primary-chat
    base_url: https://relay-a.example.com/v1
    interface: openai_chat_completions
    groups: [default, paid]
    priority: 100
    weight: 80
    auth_profile: keyring://relay-a/default
    models:
      - public_model: gpt-4.1
        upstream_model: openai/gpt-4.1
        pricing_model: gpt-4.1
      - public_model: claude-sonnet
        upstream_model: anthropic/claude-sonnet-4
        transform: claude_to_openai_chat
    overrides:
      headers:
        X-Relay-Client: cc-switch
      params:
        stream_options:
          include_usage: true

  - id: ch-openai-responses-backup
    provider_id: openai-compatible-main
    name: backup-responses
    base_url: https://relay-b.example.com/v1
    interface: openai_responses
    groups: [paid]
    priority: 90
    weight: 20
    auth_profile: keyring://relay-b/team
    models:
      - public_model: gpt-4.1
        upstream_model: gpt-4.1-2025-04-14

  - id: ch-gemini-openai-compat
    provider_id: openai-compatible-main
    name: gemini-openai-compatible
    base_url: https://generativelanguage.googleapis.com/v1beta/openai
    interface: gemini_openai_compatible
    groups: [default]
    priority: 80
    weight: 10
    auth_profile: keyring://google/gemini
    models:
      - public_model: gemini-2.5-pro
        upstream_model: gemini-2.5-pro
```

这个结构允许 UI/API 在“供应商”下展开“地址”，并对每个地址单独配置：

- 地址信息：base URL、API version、path template、timeout。
- 接口类型：Anthropic Messages、OpenAI Chat、OpenAI Responses、Gemini Native、Gemini OpenAI compatible、Azure OpenAI、自定义。
- 模型：可见模型、上游模型、模型能力、计价模型、请求/响应参数覆盖。
- 路由：group、priority、weight、retry、quota/rate limit、auto-ban。
- 认证：单 key、多 key、轮询 key、随机 key、账号 OAuth profile。

## 推荐目标架构

### 分层

```text
cc-switch desktop host
  commands/proxy.rs
  services/proxy.rs
  database adapters
  live takeover adapters
  tauri event adapter
        |
        v
proxy host adapter layer
  CcSwitchProviderSource
  CcSwitchChannelSource
  CcSwitchConfigSource
  CcSwitchHealthStore
  CcSwitchUsageSink
  CcSwitchEventSink
  CcSwitchAuthProvider
        |
        v
proxy core
  ProxyEngine
  RouteResolver
  ForwardPipeline
  Protocol adapters
  Channel registry
  Response pipeline
  Circuit breaker
  Runtime state
        |
        v
proxy transports
  HTTP gateway (Axum/hyper)
  Rust API
```

### 最终目录建议

目标态建议拆为一个独立 path crate：

```text
src-tauri/
  crates/
    cc-proxy-core/
      Cargo.toml
      src/
        lib.rs
        domain/
          app.rs
          channel.rs
          config.rs
          error.rs
          provider.rs
          request.rs
          response.rs
          route.rs
          usage.rs
        engine/
          mod.rs
          context.rs
          forward_pipeline.rs
          routing.rs
          retry.rs
          status.rs
        ports/
          mod.rs
          auth.rs
          channel_source.rs
          config.rs
          events.rs
          health.rs
          model_catalog.rs
          provider_source.rs
          route_policy.rs
          usage.rs
        provider/
          mod.rs
          adapter.rs
          claude.rs
          codex.rs
          gemini.rs
          transforms/
        transport/
          http/
            mod.rs
            router.rs
            handlers.rs
            server.rs
          upstream/
            hyper_client.rs
            reqwest_client.rs
        runtime/
          circuit_breaker.rs
          shadow.rs
          session.rs
          sse.rs
        middleware/
          body_filter.rs
          cache_injector.rs
          media_sanitizer.rs
          thinking_budget_rectifier.rs
          thinking_optimizer.rs
          thinking_rectifier.rs
        observability/
          log_codes.rs
          usage_parser.rs
          usage_calculator.rs
    cc-switch-proxy-host/
      Cargo.toml
      src/
        lib.rs
        database_provider_source.rs
        database_channel_source.rs
        database_route_policy.rs
        database_config_source.rs
        database_usage_sink.rs
        tauri_event_sink.rs
        live_takeover.rs
```

如果担心一次性改 Cargo 结构风险过高，可以先在同一 crate 内落成：

```text
src-tauri/src/proxy_core/
src-tauri/src/proxy_host/
```

但验收标准必须一样：`proxy_core` 不允许 import `tauri`、`crate::database`、`crate::settings`、`crate::commands`、`crate::services`。等依赖方向稳定后再移动到 path crate。

## 核心领域类型

独立模块需要自己的中立类型，不应直接复用 `AppType` 和 `Provider`。

```rust
pub enum AppKind {
    Claude,
    ClaudeDesktop,
    Codex,
    Gemini,
    Custom(String),
}

pub struct ProviderSpec {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub account_ref: Option<String>,
    pub metadata: ProviderMetadata,
}

pub enum ProviderKind {
    Claude,
    ClaudeAuth,
    Codex,
    Gemini,
    GeminiCli,
    OpenRouter,
    GitHubCopilot,
    CodexOAuth,
    Custom(String),
}

pub struct ChannelSpec {
    pub id: String,
    pub provider_id: String,
    pub name: String,
    pub status: ChannelStatus,
    pub endpoint: UpstreamEndpoint,
    pub interface: InterfaceKind,
    pub auth_profile: AuthProfileRef,
    pub models: Vec<ModelRoute>,
    pub groups: Vec<String>,
    pub priority: i64,
    pub weight: u32,
    pub retry_policy: RetryPolicy,
    pub health_policy: ChannelHealthPolicy,
    pub overrides: ChannelOverrides,
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
}

pub enum ChannelStatus {
    Enabled,
    ManuallyDisabled,
    AutoDisabled,
    Draining,
}

pub struct UpstreamEndpoint {
    pub base_url: String,
    pub path_template: Option<String>,
    pub api_version: Option<String>,
    pub timeout_profile: Option<String>,
}

pub enum InterfaceKind {
    AnthropicMessages,
    OpenAiChatCompletions,
    OpenAiResponses,
    GeminiNative,
    GeminiOpenAiCompatible,
    AzureOpenAi,
    Rerank,
    Embeddings,
    Custom(String),
}

pub struct ModelRoute {
    pub public_model: String,
    pub upstream_model: String,
    pub capabilities: ModelCapabilities,
    pub pricing_model: Option<String>,
    pub request_overrides: serde_json::Value,
    pub response_overrides: serde_json::Value,
}

pub struct ChannelOverrides {
    pub headers: http::HeaderMap,
    pub params: serde_json::Value,
    pub status_code_mapping: Vec<StatusCodeMapping>,
    pub model_mapping: Vec<ModelMappingRule>,
}

pub struct RouteSelection {
    pub provider: ProviderSpec,
    pub channel: ChannelSpec,
    pub model_route: Option<ModelRoute>,
    pub inbound_interface: InterfaceKind,
    pub outbound_interface: InterfaceKind,
}

pub struct ProxyRequest {
    pub app: AppKind,
    pub method: http::Method,
    pub endpoint: String,
    pub inbound_interface: InterfaceKind,
    pub requested_model: Option<String>,
    pub route_group: Option<String>,
    pub headers: http::HeaderMap,
    pub extensions: http::Extensions,
    pub body: ProxyBody,
    pub client_request_id: Option<String>,
}

pub enum ProxyBody {
    Empty,
    Json(serde_json::Value),
    Bytes(bytes::Bytes),
}

pub struct ProxyResult {
    pub response: ProxyCoreResponse,
    pub selected_route: RouteSelection,
    pub outbound_model: Option<String>,
    pub usage_record: Option<UsageRecord>,
}

pub enum ProxyResponseBody {
    Empty,
    Json(serde_json::Value),
    Bytes(bytes::Bytes),
    Stream(ProxyByteStream),
}
```

`ChannelSpec` 是这次迁移的关键。它对应中转站里的“渠道/地址”，不是简单 URL 列表。每个 channel 都可以声明：

- 自己的 `base_url`、API version、path 模板和 timeout profile。
- 自己的上游接口协议，例如同一个 provider 下一个地址走 OpenAI Chat，另一个地址走 Responses 或 Gemini OpenAI compatible。
- 自己允许的模型集合和 public model -> upstream model 映射。
- 自己的 key/auth profile，后续可以扩展多 key、轮询 key、随机 key 和 key 级禁用。
- 自己的 header override、param override、状态码映射、计价模型和健康/熔断策略。

这样才能支持“同一个供应商配置多个不同中转地址，每个地址暴露不同模型和不同接口”的迁移目标。

### 为什么要引入 `Custom`

当前代码里 `OpenCode`、`OpenClaw`、`Hermes` 不支持代理，只在 `get_adapter` 里 fallback 到 Codex。独立模块如果要对外作为中转，需要允许外部宿主定义自己的 app namespace，否则未来接入新客户端时又会改核心枚举。

## 核心接口设计

### `ProxyEngine`

```rust
pub struct ProxyEngine<S> {
    services: Arc<S>,
    state: Arc<ProxyRuntimeState>,
}

impl<S: ProxyServices> ProxyEngine<S> {
    pub async fn handle(&self, req: ProxyRequest) -> Result<ProxyResult, ProxyError>;
    pub async fn status(&self) -> ProxyStatus;
    pub async fn set_runtime_config(&self, config: ProxyRuntimeConfig) -> Result<(), ProxyError>;
    pub async fn reset_provider_breaker(&self, app: AppKind, provider_id: &str);
}
```

`ProxyEngine::handle` 是唯一业务入口。HTTP handlers 和外部 Rust 调用都必须走它。

### `ProxyServices`

不新增 `async-trait` 的情况下，可以使用 `futures::future::BoxFuture`。

```rust
pub trait ProxyServices: Send + Sync + 'static {
    fn config(&self) -> &dyn ProxyConfigSource;
    fn providers(&self) -> &dyn ProviderSource;
    fn channels(&self) -> &dyn ChannelSource;
    fn route_policies(&self) -> &dyn RoutePolicySource;
    fn route_resolver(&self) -> &dyn RouteResolver;
    fn health_store(&self) -> &dyn ChannelHealthStore;
    fn auth_provider(&self) -> &dyn AuthProvider;
    fn model_catalog(&self) -> &dyn ModelCatalogProvider;
    fn usage_sink(&self) -> &dyn UsageSink;
    fn event_sink(&self) -> &dyn ProxyEventSink;
    fn forward_pipeline(&self) -> &dyn ForwardPipeline;
}
```

### 配置接口

```rust
pub trait ProxyConfigSource: Send + Sync {
    fn global_config<'a>(&'a self) -> BoxFuture<'a, Result<ProxyGlobalConfig, ProxyError>>;
    fn app_config<'a>(&'a self, app: &'a AppKind)
        -> BoxFuture<'a, Result<ProxyAppConfig, ProxyError>>;
    fn rectifier_config<'a>(&'a self) -> BoxFuture<'a, Result<RectifierConfig, ProxyError>>;
    fn optimizer_config<'a>(&'a self) -> BoxFuture<'a, Result<OptimizerConfig, ProxyError>>;
    fn copilot_optimizer_config<'a>(&'a self)
        -> BoxFuture<'a, Result<CopilotOptimizerConfig, ProxyError>>;
}
```

当前 `RequestContext::new` 里读 DB/settings 的逻辑应移动到 `CcSwitchConfigSource`。

### Provider、Channel 与 Route 接口

```rust
pub trait ProviderSource: Send + Sync {
    fn current_provider_id<'a>(&'a self, app: &'a AppKind)
        -> BoxFuture<'a, Result<Option<String>, ProxyError>>;

    fn get_provider<'a>(&'a self, app: &'a AppKind, provider_id: &'a str)
        -> BoxFuture<'a, Result<Option<ProviderSpec>, ProxyError>>;
}

pub trait ChannelSource: Send + Sync {
    fn list_channels<'a>(&'a self, query: ChannelQuery<'a>)
        -> BoxFuture<'a, Result<Vec<ChannelSpec>, ProxyError>>;

    fn get_channel<'a>(&'a self, channel_id: &'a str)
        -> BoxFuture<'a, Result<Option<ChannelSpec>, ProxyError>>;

    fn list_channel_models<'a>(&'a self, channel_id: &'a str)
        -> BoxFuture<'a, Result<Vec<ModelRoute>, ProxyError>>;
}

pub trait RoutePolicySource: Send + Sync {
    fn route_groups<'a>(&'a self, app: &'a AppKind)
        -> BoxFuture<'a, Result<Vec<RouteGroup>, ProxyError>>;

    fn policy_for<'a>(&'a self, app: &'a AppKind, group: Option<&'a str>)
        -> BoxFuture<'a, Result<RoutePolicy, ProxyError>>;
}
```

当前 `ProviderRouter::select_providers` 应拆成三部分：

- `ProviderSource` 只负责读取供应商/账号元数据。
- `ChannelSource` 负责读取可路由 channel，包括现有 provider 主 URL、`provider_endpoints` 投影出来的兼容 channel，以及未来新增的独立 channel 表。
- `RouteResolver` 负责按 app、接口、模型、group、优先级、权重、熔断、限流和 retry 策略生成尝试计划。

这样核心仍拥有路由算法，宿主只提供数据。

### RouteResolver

`RouteResolver` 是核心算法，不是持久化接口：

```rust
pub struct RouteRequest {
    pub app: AppKind,
    pub inbound_interface: InterfaceKind,
    pub requested_model: Option<String>,
    pub route_group: Option<String>,
    pub requires_streaming: bool,
    pub requires_tools: bool,
}

pub struct RoutePlan {
    pub attempts: Vec<RouteSelection>,
    pub policy: RoutePolicy,
}

pub trait RouteResolver {
    fn resolve(
        &self,
        request: RouteRequest,
        providers: Vec<ProviderSpec>,
        channels: Vec<ChannelSpec>,
        runtime: &ProxyRuntimeState,
    ) -> Result<RoutePlan, ProxyError>;
}
```

解析顺序建议固定为：

1. 过滤未启用、手动禁用、自动禁用或 draining 的 channel。
2. 过滤 group 不匹配的 channel；未指定 group 时默认使用 `default` 或 app 当前 group。
3. 过滤接口不兼容且没有转换路径的 channel。
4. 按请求模型匹配 `ModelRoute.public_model` 或 `ModelRoute.upstream_model`，并得到最终上游模型名。
5. 过滤不满足工具、流式、vision、embedding、rerank 等能力要求的 channel。
6. 应用 channel 级熔断、限流、余额/额度、auto-ban 状态和最近失败窗口。
7. 按 priority 降序分层，同层使用 weight 加权随机；失败后按 retry policy 继续下一 channel。

这能表达 NewAPI 类中转常见策略：高优先级优先、同优先级按权重分流、失败自动重试、channel 自动禁用、按用户组或模型限制可见范围。

### 健康与熔断接口

熔断器内存态应保留在核心 runtime；持久化健康状态通过端口写回宿主。

```rust
pub trait ChannelHealthStore: Send + Sync {
    fn record_attempt<'a>(
        &'a self,
        result: ChannelAttemptResult,
    ) -> BoxFuture<'a, ProxyCoreResult<()>>;

    fn reset_channel<'a>(&'a self, channel_id: &'a str)
        -> BoxFuture<'a, ProxyCoreResult<ChannelHealthReset>>;
}
```

当前实现已落到 `ChannelHealthStore::record_attempt` 与 `reset_channel`：`record_attempt` 写入 channel 健康统计，`reset_channel` 由 host adapter 查询 channel 所属 app，并复用 `ProviderRouter::reset_channel_breaker` 同时清内存 circuit breaker 与 DB 健康状态；`ProxyEngine::reset_channel_health_response` 在 core 内包装管理 API response envelope。健康状态必须以 channel 为主键。provider 级状态只能作为聚合视图，否则同一 provider 下一个地址失败会误伤另一个健康地址。

### 用量接口

用量解析可以留在核心，落库必须抽象。

```rust
pub trait UsageSink: Send + Sync {
    fn enabled(&self) -> bool;

    fn record<'a>(&'a self, event: UsageRecord)
        -> BoxFuture<'a, Result<(), ProxyError>>;
}
```

`UsageRecord` 应携带：

- app
- provider_id/provider_kind
- channel_id/channel_name
- route_group
- request_model
- outbound_model
- upstream_interface
- response_model
- pricing_model
- token usage
- latency/first_token_ms
- status/error
- session_id
- streaming

当前 `UsageLogger` 和定价查询已经可以通过 `CcSwitchUsageSink` 复用。`UsageRecord` 必须保持完整字段；不能退回只含 token/model 的简化 hint，否则会丢失 provider、pricing、latency、status 和 session 语义。

### 事件接口

所有 UI/托盘/外部监控都通过事件接口，不由核心直接调用 Tauri。

```rust
pub enum ProxyEvent {
    ServerStarted { address: String, port: u16 },
    ServerStopped,
    RequestStarted { request_id: String, app: AppKind },
    ChannelAttempt { request_id: String, provider_id: String, channel_id: String },
    ChannelSucceeded { request_id: String, provider_id: String, channel_id: String },
    ChannelFailed { request_id: String, provider_id: String, channel_id: String, error: String },
    FailoverSelected { app: AppKind, provider_id: String, channel_id: String, provider_name: String },
    UsageObserved { request_id: String },
    StatusChanged { status: ProxyStatus },
}

pub trait ProxyEventSink: Send + Sync {
    fn emit(&self, event: ProxyEvent);
}
```

`FailoverSwitchManager` 里当前直接调用 `tauri::AppHandle`、`proxy_service.hot_switch_provider`、托盘菜单和前端事件。重构后：

1. 核心只发 `ProxyEvent::FailoverSelected`。
2. CC Switch 宿主收到事件后决定是否更新 DB 当前 provider、Live 配置、托盘菜单和前端事件。

### 认证接口

当前 `forwarder.rs` 直接依赖 `CodexOAuthState`、`CopilotAuthState` 和 provider-specific manager。独立模块应把“获取/刷新可用 token”抽象出来。

```rust
pub trait AuthProvider: Send + Sync {
    fn resolve_auth<'a>(
        &'a self,
        app: &'a AppKind,
        provider: &'a ProviderSpec,
        channel: &'a ChannelSpec,
        request: &'a ProxyRequest,
    ) -> BoxFuture<'a, Result<AuthInfo, ProxyError>>;
}
```

provider adapter 仍负责将 `AuthInfo` 变成 header，但 token 刷新和宿主账号状态读取不在 adapter 内完成。channel 可以指向同一个 provider 的不同 key/auth profile，必须避免把一个 channel 的 key 泄漏到另一个 channel。

### Model catalog 接口

`GET /v1/models` 已从 `handlers::handle_models` 直读 Codex 配置文件迁出。当前端口分两类模型视图：

```rust
pub trait ModelCatalogProvider: Send + Sync {
    fn load_catalog<'a>(&'a self, app: &'a AppKind, provider_id: &'a str)
        -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>>;

    fn load_client_catalog<'a>(&'a self, app: &'a AppKind)
        -> BoxFuture<'a, ProxyCoreResult<ModelCatalog>>;
}

impl ProxyEngine {
    pub async fn list_models(
        &self,
        app: &AppKind,
        group: Option<&str>,
        inbound_interface: Option<&InterfaceKind>,
    ) -> ProxyCoreResult<Vec<RoutableModel>>;

    pub async fn list_model_catalog(
        &self,
        app: &AppKind,
        app_type: impl Into<String>,
        group: Option<&str>,
        inbound_interface: Option<&InterfaceKind>,
    ) -> ProxyCoreResult<RoutableModelList>;

    pub async fn client_model_catalog(&self, app: &AppKind)
        -> ProxyCoreResult<ModelCatalog>;
}
```

`ProxyEngine::list_models` 是 route-visible 视图，用于管理 API 或未来可控的客户端 catalog 生成；它按 app、group、inbound interface 过滤 channel，避免暴露当前 group 不可用的 channel。`ProxyEngine::list_model_catalog` 在 core 内包装 `/proxy/v1/apps/{app}/models` 对外 envelope，使管理 API response shape 不再由 host handler 手写。`ProxyEngine::client_model_catalog` 保留客户端兼容 raw catalog 语义；当前 Codex `/v1/models` handler 只调用该 core 方法并返回 `ModelCatalog.raw`。

Channel 管理 API 的部分 contract 也已开始收敛到 core：`management_api` 固化 appType、route resolve appType 和 channel_id path 参数校验，`HealthCheckResponse` 表示 `/proxy/v1/health` 存活检查结果，`ProxyChannelWriteRequest`/`ProxyChannelPatchRequest` 表示 channel 创建与更新请求，`ProxyChannelModelWriteRequest`/`ProxyChannelModelsReplaceRequest` 表示 channel 模型替换请求，`AppModelListQuery`/`AppChannelListQuery` 表示 app 模型目录和 app channel 列表的 query alias/归一化契约，`ChannelListQuery`/`GroupListQuery` 表示全局 channel 列表和 route group 列表的 appType query 契约，`AppListResponse`/`AppSummary` 表示 app namespace 列表结果，`ProviderSummaryInput`/`ProviderListResponse::from_provider_inputs` 负责脱敏 provider 候选列表和 current/failover/routeCandidate 标记，`AppChannelResponse` 表示 app 维度 channel 列表和带过滤 dry-run 候选结果，`AppChannelListResponse::from_route_source` 与 `AppChannelRouteResponse::from_route_resolve` 负责把 route source 和 dry-run 结果包装成稳定外部 envelope，`CurrentRouteResponse<T>`/`CurrentRouteProviderSummary` 表示当前路由状态，`ChannelMigrationPreviewResponse<T>`/`ChannelMigrationMaterializeResponse` 表示旧 provider/endpoint 到 channel 表的迁移结果，`RouteResolveRequest`/`RouteResolveResponse`/`ChannelRouteCandidate`/`ChannelRouteRejected`/`ChannelRouteSource` 表示 `/proxy/v1/route/resolve` dry-run API contract，`ChannelRouteSource::as_str` 固化外部 source label，`stable_channel_id` 固化 legacy/manual channel 幂等 ID 规则，`resolve_channel_route` 负责 dry-run 的过滤、接口兼容、淘汰原因和排序规则，`ChannelListResponse<T>` 表示全局 channel 列表结果，`ChannelHealthResetResponse` 表示 breaker reset 结果，`ChannelDeleteResponse` 表示 channel 删除结果，`ChannelModelsResponse<T>` 表示 channel model 列表/替换结果，`RouteGroupListResponse` 负责 `/proxy/v1/groups` 的默认组补齐、group 计数、appTypes 聚合和 sources 去重排序。host handler 仍负责 HTTP path/query 提取、DB CRUD、provider router 查询和 host record 类型，但外部请求/响应 shape 不再散落在 handler 或 DAO 的本地 DTO 定义中。

CC Switch 桌面宿主在 `CcSwitchModelCatalogProvider::load_client_catalog` 中实现 Codex `model_catalog_json` 文件读取和 stale guard；外部宿主可以返回自己的模型目录。后续如需让 Codex `/v1/models` 完全使用 route-visible 目录，应在 core 内生成 Codex 兼容 raw catalog，而不是让 handler 重新拼装。

## HTTP 对外接口

### 兼容入口

保留现有客户端入口：

| 路径 | 语义 |
| --- | --- |
| `POST /v1/messages` | Claude Messages |
| `POST /claude/v1/messages` | Claude Messages 带 app 前缀 |
| `GET /claude-desktop/v1/models` | Claude Desktop gateway 模型列表 |
| `POST /claude-desktop/v1/messages` | Claude Desktop gateway Messages |
| `POST /v1/chat/completions` | OpenAI Chat / Codex |
| `POST /v1/responses` | OpenAI Responses / Codex |
| `POST /v1/responses/compact` | Codex compact |
| `ANY /v1beta/*path` | Gemini |
| `ANY /gemini/v1beta/*path` | Gemini 带 app 前缀 |
| `ANY /gemini/v1/*path` | Gemini GA |

### 新增稳定管理接口

建议新增版本化管理 API，供外部集成使用：

| 路径 | 方法 | 用途 |
| --- | --- | --- |
| `/proxy/v1/health` | GET | 存活检查 |
| `/proxy/v1/status` | GET | 运行状态、active targets、统计 |
| `/proxy/v1/apps` | GET | 已注册 app namespace |
| `/proxy/v1/apps/{app}/providers` | GET | 当前 app 的 provider 候选 |
| `/proxy/v1/apps/{app}/models` | GET | 当前 app 可路由模型，支持 group/interface 过滤，并返回对应 provider/channel 信息 |
| `/proxy/v1/apps/{app}/channels` | GET | 当前 app 可见 channel 候选，支持 group/model/interface 过滤 |
| `/proxy/v1/apps/{app}/channels/migration/preview` | GET | 只读预览旧 provider/endpoint 到 channel 的投影结果和需人工复核项 |
| `/proxy/v1/apps/{app}/channels/migration/materialize` | POST | 将旧 provider/endpoint 投影幂等写入 channel 表 |
| `/proxy/v1/channels` | GET/POST | channel 列表和创建 |
| `/proxy/v1/channels/{channel_id}` | GET/PATCH/DELETE | 查询、更新、删除单个 channel |
| `/proxy/v1/channels/{channel_id}/models` | GET/PUT | 查询或替换 channel 模型映射 |
| `/proxy/v1/channels/{channel_id}/test` | POST | 使用指定模型和接口测试该 channel |
| `/proxy/v1/channels/{channel_id}/breakers/reset` | POST | 重置 channel 熔断器 |
| `/proxy/v1/groups` | GET/POST | route group 列表和创建 |
| `/proxy/v1/route/resolve` | POST | dry-run 路由解析，返回候选 channel 和淘汰原因 |
| `/proxy/v1/apps/{app}/routes/current` | GET | 当前实际 route/provider/channel |
| `/proxy/v1/events` | GET | SSE 事件流，供外部监控 |

管理接口必须支持鉴权。CC Switch 本地默认可继续监听 `127.0.0.1` 并允许无 token；一旦监听 `0.0.0.0` 或外部宿主启用，应要求 bearer token。该策略已收敛到 `proxy-core::management_auth`：core 负责 loopback 判定、配置 token/env fallback 优先级和 bearer header 语义；host 负责读取配置、读取 `CC_SWITCH_PROXY_MANAGEMENT_TOKEN`、解析 Axum header 并映射错误。

## 请求处理流水线

目标流水线：

```text
ProxyRequest
  -> classify app/inbound interface
  -> load app config
  -> resolve session/request model/group
  -> query providers + channels
  -> route resolver
       - filter status/group/interface/model/capability/quota/health
       - priority + weight + retry policy
       - produce channel attempt plan
  -> for route in retry plan
       -> channel circuit allow
       -> clone request body
       -> pre-send middleware
            - optimizer
            - cache injector
            - media prevention
            - private param filter
       -> provider adapter
            - selected channel base URL
            - selected outbound interface
            - selected upstream model
            - auth headers
            - header/param override
            - request transform
            - URL build
       -> upstream transport
       -> response classifier
       -> reactive rectifier retry if needed
       -> channel circuit result
  -> response pipeline
       - pass-through
       - SSE transform
       - response transform
       - usage collection
       - event emit
  -> ProxyResult
```

当前已迁移到 core 的 response pipeline 子能力包括响应头清理、响应体诊断、非流式 body 解压、timeout 选择、Claude transform 路由策略、Codex 代理错误 envelope 和 SSE 文本/聚合工具：`strip_hop_by_hop_response_headers` 移除 hop-by-hop 头和 `Connection` 点名扩展头，`strip_entity_headers_for_rebuilt_body` 移除重建 body 后失真的实体头，`prepare_rebuilt_json_response_headers` 负责转换后 JSON body 的实体/hop-by-hop/content-type 头重建，`json_proxy_response`/`rebuilt_json_proxy_response` 负责 JSON 响应的 header 重建、body 序列化与 `ProxyCoreResponse` 构造，`transformed_sse_response_headers` 负责转换后 SSE 固定响应头，`transformed_sse_proxy_response` 负责转换后 SSE 响应的固定 header、OK status 和 stream body `ProxyCoreResponse` 构造，`body_looks_like_sse`/`body_diagnostics_suffix`/`body_snippet` 负责未标记 SSE body 嗅探与错误现场摘要，`get_content_encoding`/`decompress_body`/`decode_response_body` 负责 gzip/x-gzip/deflate/br 的 content-encoding 判定与解压、未知编码/失败解码原样透传和成功解码后的 header 一致性处理，`resolve_response_timeout_config` 负责 failover-gated 非流式 body timeout 与流式 first-byte/idle timeout 选择规则，`should_use_claude_transform_streaming`/`should_aggregate_codex_oauth_responses_sse` 负责 Claude transform 的 streaming 与非流 SSE 聚合路由策略，`codex_proxy_error_json`/`codex_upstream_error_to_response_error`/`normalize_codex_chat_error_body` 负责 Codex 转发层代理错误和 Chat 上游错误响应的 Responses 风格 envelope、上游错误体归一化、非 JSON 预览截断和 413 上游体积限制提示，`strip_sse_field`/`take_sse_block`/`append_utf8_safe` 负责 SSE 字段提取、事件分帧和跨 chunk UTF-8 拼接，`SseEventScanner` 负责流式 data 行扫描、`[DONE]` 判定和可选 JSON parse，`SseUsageAccumulator` 负责 usage 事件缓存、首个被收集事件计时和 finish-once 防重入，`claude_stream_usage_event_filter`/`openai_stream_usage_event_filter`/`codex_stream_usage_event_filter`/`gemini_stream_usage_event_filter` 负责热路径 usage 事件预过滤，`TokenUsage` 及其 Claude/OpenAI/Codex/Gemini usage JSON parser 与 stream model extractor 负责协议用量解析和模型归因，`usage_tokens_from_token_usage`/`normalize_usage_models`/`normalize_error_usage_models`/`token_usage_from_usage_record`/`resolve_usage_record_pricing_models`/`transformed_response_usage` 负责 `UsageRecord` 的 token bucket 映射、模型归因、pricing model 选择和转换后非流响应 usage 归因规则，`chat_sse_to_response_value`/`responses_sse_to_response_value` 负责错标 SSE 非流式兜底聚合。body 读取、日志和 app-specific 响应转换仍在 host 层，`proxy::response_adapter` 集中承接 Axum/旧 `hyper_client::ProxyResponse` transport 桥接，`proxy::error_mapper` 集中承接 core/host error 映射、response parse 专用错误适配和 Codex proxy error body 分类；后续应按纯逻辑先行、transport adapter 后置的顺序继续抽离。

同一 provider 下多个 channel 的行为必须互相隔离：

- 一个 channel 触发熔断不影响同 provider 下其他 channel。
- 一个 channel 的模型映射只影响该 channel，不改写 provider 全局配置。
- 一个 channel 的 header/param override 只在构造该次上游请求时合并，不写回宿主配置。
- 重试计划记录每次尝试的 `provider_id + channel_id + upstream_model + outbound_interface`，便于 usage、审计和 UI 展示。

## 当前文件迁移映射

| 现文件 | 目标位置 | 处理方式 |
| --- | --- | --- |
| `server.rs` | `transport/http/server.rs` | 去掉 `Database`/`tauri`，只持有 `ProxyEngine` |
| `handlers.rs` | `transport/http/handlers.rs` + `engine` | HTTP 解析留 transport，业务处理移到 engine |
| `handler_context.rs` | `engine/context.rs` | DB/settings 读取改为 service traits |
| `forwarder.rs` | `engine/forward_pipeline.rs` | 切掉 Tauri/AppHandle/Database 依赖 |
| `provider_router.rs` | `engine/routing.rs` | 改为 channel route resolver；provider 数据读取下沉到 `ProviderSource`，channel 数据读取下沉到 `ChannelSource` |
| `failover_switch.rs` | `host/cc_switch` | 核心只发 failover event |
| `response_processor.rs` | `engine/response_pipeline.rs` | 用量落库改为 `UsageSink` |
| `usage/logger.rs` | `host/cc_switch/database_usage_sink.rs` | 只保留 parser/calculator 在核心 |
| `providers/*` | `provider/*` | 先迁移类型依赖，再移动文件 |
| `hyper_client.rs` | `transport/upstream/hyper_client.rs` | 保留 header casing 行为 |
| `http_client.rs` | `transport/upstream/reqwest_client.rs` 或 host shared | 需区分“上游请求客户端”和“应用全局 HTTP 客户端” |
| `types.rs` | `domain/config.rs`, `domain/status.rs` | 拆分领域类型 |
| `services/proxy.rs` | `host/cc_switch/live_takeover.rs` | 保留桌面宿主逻辑 |
| `provider_endpoints` 相关 DB 访问 | `host/cc_switch/database_channel_source.rs` | 兼容投影为 channel，后续迁移到独立 channel 表 |

## 分阶段实施计划

### Phase 0：行为锁定

先补齐/固定回归测试，不做抽离。

必须覆盖：

1. Claude `/v1/messages` 透传。
2. Claude → OpenAI Chat 转换，流式与非流式。
3. Claude → OpenAI Responses 转换，流式与非流式。
4. Codex `/v1/responses` 透传。
5. Codex Responses → Chat upstream 转换。
6. Gemini GET/POST/stream endpoint。
7. `preserve_header_case` 行为。
8. non-streaming timeout 与 streaming first-byte/idle timeout。
9. auto failover 开关关闭时只尝试当前 provider。
10. auto failover 开启时按队列和 max_retries 尝试。
11. 同 provider 多 channel 时按 group、interface、model 映射选中正确 channel。
12. channel 优先级、权重、熔断和失败重试顺序稳定。
13. 熔断 HalfOpen permit 释放。
14. usage logging 可关闭时不解析 SSE 热路径。
15. Claude Desktop gateway auth。

推荐命令：

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml proxy --lib
cargo test --manifest-path src-tauri/Cargo.toml services::proxy --lib
node_modules/.bin/tsc --noEmit
```

### Phase 1：引入中立领域类型

新增 `proxy_core` 模块但不改运行路径。

1. 定义 `AppKind`、`ProviderSpec`、`ProviderKind`、`ChannelSpec`、`InterfaceKind`、`ModelRoute`、`RouteSelection`、`ProxyRequest`、`ProxyResult`、`ProxyError`。
2. 写 `From<AppType> for AppKind`、`From<Provider> for ProviderSpec` 适配层。
3. 写从现有 provider 主 URL 和 `provider_endpoints` 到 `ChannelSpec` 的兼容投影。
4. 禁止核心新类型反向依赖 `crate::app_config` 和 `crate::provider`。
5. 为当前 `ProviderAdapter` 增加使用 `RouteSelection` 的并行 trait，旧 trait 暂留。

验收：

- 新类型单测通过。
- 旧代理路径不变。

### Phase 2：抽 `ProxyServices` 端口

新增 traits 和 CC Switch adapter 实现。

1. `CcSwitchConfigSource` 包装现有 DB/settings 读取。
2. `CcSwitchProviderSource` 包装 provider/current provider 读取。
3. `CcSwitchChannelSource` 包装 provider 主 URL、`provider_endpoints` 和未来 channel 表读取。
4. `CcSwitchRoutePolicySource` 包装 failover queue、group 和优先级/权重策略。
5. `CcSwitchHealthStore` 包装 channel health 写入；兼容期可同时写 provider health 聚合。
6. `CcSwitchUsageSink` 包装 `UsageLogger`；写入必须使用完整 `UsageRecord`，不能用简化 hint 直接写账单。
7. `CcSwitchEventSink` 包装 `ProxyEventBus`，再由宿主决定是否转发到 Tauri/UI/托盘。
8. `CcSwitchAuthProvider` 包装 Codex/Copilot OAuth token 刷新和 channel key 选择。

验收：

- `RequestContext::new` 不再直接读 DB/settings，而是走接口。
- 行为测试保持通过。

### Phase 2.5：Channel 存储兼容与迁移设计

在不破坏现有配置的前提下引入新存储模型：

1. 新增 `proxy_channels`：`id`、`provider_id`、`name`、`status`、`base_url`、`interface_kind`、`auth_profile_ref`、`groups`、`priority`、`weight`、`retry_policy_json`、`health_policy_json`、`header_override_json`、`param_override_json`、`status_code_mapping_json`、`tags`、`metadata_json`。
2. 新增 `proxy_channel_models`：`channel_id`、`public_model`、`upstream_model`、`capabilities_json`、`pricing_model`、`request_overrides_json`、`response_overrides_json`。
3. 新增 `proxy_channel_keys`：`channel_id`、`key_ref` 或加密后的 key、`status`、`priority`、`weight`、`last_failure`；第一阶段可以只预留，不马上迁移现有账号 token。
4. 新增 `proxy_channel_health`：`channel_id`、`status`、`last_success_at`、`last_failure_at`、`consecutive_failures`、`response_time_ms`、`disabled_reason`。
5. 保留旧 `providers`、`provider_endpoints`、provider health 和 failover queue；host adapter 先把旧数据投影成 channel，等 UI 和迁移脚本稳定后再写入新表。

迁移规则：

- 每个现有 provider 生成一个默认 channel，继承 provider 主 base URL、provider 类型、账号/auth、默认模型集合和当前健康状态。
- 每条 `provider_endpoints` 生成一个附加 channel，默认继承 provider 的接口类型、模型集合和 auth，base URL 来自 endpoint URL。
- 现有 failover queue 映射为默认 route group 的 channel 顺序；如果只有 provider 粒度，则该 provider 下默认 channel 排在前面，附加 channel 跟随。
- 无法推断的 interface/model mapping 写入 `metadata_json` 并标记 `migration_needs_review`，不静默丢弃。
- 兼容期所有写入仍可落在旧表，新 channel 表只读或双写；切换前提供 dry-run diff。

验收：

- 同一份旧配置经兼容投影后，当前默认 provider 行为不变。
- 新增 channel 表为空时，系统仍可从旧 provider 和 `provider_endpoints` 解析路由。
- 新增 channel 表有数据时，route resolver 优先使用 channel 表；旧数据仅作为 fallback。
- live forwarder 在 channel 表为空时继续使用 legacy provider attempt；显式 materialize 后使用 channel-backed attempt，base URL、接口格式和匹配模型映射来自 channel，认证仍继承 provider。

### Phase 3：抽 `ProxyEngine`

把 handler 内业务逻辑移到 engine。

1. HTTP handler 只负责读取 body、解析 endpoint、构造 `ProxyRequest`、把 `ProxyResult` 转 HTTP response。
2. `ProxyEngine::handle` 负责 route 解析、forward pipeline、response pipeline。
3. Claude/Codex/Gemini 特殊处理改成 protocol handler，挂在 engine 内部。

验收：

- `handlers.rs` 大幅变薄。
- 相同 `ProxyRequest` 通过 HTTP 和 Rust API 得到一致行为。
- 相同 provider 下两个 channel 使用不同 base URL、接口和模型时，可以稳定选中预期 channel。

### Phase 4：剥离 Tauri 和 Database

在 `proxy_core` 内执行 import 防线。

禁止出现：

```text
tauri
crate::database
crate::settings
crate::commands
crate::services
crate::store::AppState
crate::config
```

如果仍需要宿主能力，必须新增/扩展 port trait。

验收：

```bash
rg -n "tauri|crate::database|crate::settings|crate::commands|crate::services|crate::store|crate::config" src-tauri/src/proxy_core
```

结果应为空或只出现在测试 fixture/注释的白名单位置。

### Phase 5：拆为 path crate

当 `proxy_core` 已无宿主依赖后，移动到：

```text
src-tauri/crates/proxy-core
```

`cc-switch` 主 crate 通过 path dependency 引用。

验收：

```bash
cargo test --manifest-path src-tauri/crates/cc-proxy-core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml proxy --lib
```

### Phase 6：对外管理 API 与事件流

在 HTTP transport 中新增 `/proxy/v1/*` 管理接口。

1. `GET /proxy/v1/status`
2. `GET /proxy/v1/events`
3. `GET /proxy/v1/apps`
4. `GET /proxy/v1/apps/{app}/providers`
5. `GET /proxy/v1/apps/{app}/channels`
6. `GET /proxy/v1/apps/{app}/channels/migration/preview`
7. `POST /proxy/v1/apps/{app}/channels/migration/materialize`
8. `GET/POST /proxy/v1/channels`
9. `GET/PATCH/DELETE /proxy/v1/channels/{channel_id}`
10. `GET/PUT /proxy/v1/channels/{channel_id}/models`
11. `POST /proxy/v1/channels/{channel_id}/test`
12. `POST /proxy/v1/route/resolve`
13. `POST /proxy/v1/channels/{channel_id}/breakers/reset`

CC Switch 前端可以继续用 Tauri commands；外部集成用 HTTP API。

## 数据与状态边界

### 留在核心内存态

- active connections
- total/success/failed counters
- uptime
- current active target view
- circuit breaker runtime state
- channel retry cursor and weighted selection state
- Gemini shadow state
- Codex Chat history state
- in-flight request guards

### 留在宿主持久化

- provider definitions
- channel definitions
- channel model mappings
- route groups and route policies
- current provider
- failover queue
- proxy config
- channel health persistence and provider health aggregation
- request logs
- pricing config
- model catalog
- managed account auth states
- live config backup/takeover state

### 原因

核心内存态与请求处理生命周期强相关，适合由 proxy runtime 管；宿主持久化与产品/数据库/UI 强相关，必须由 CC Switch host adapter 管。

## 兼容性要求

1. 默认端口仍为 `127.0.0.1:15721`。
2. 原有 client path 不变。
3. `Claude Desktop` gateway token 行为不变。
4. `Codex` `/v1/models` reachability check 仍返回 CC Switch 管理的 model catalog。
5. `live_takeover_active` 语义不变。
6. 代理运行中更新端口仍需要重启 server，并同步 Live 配置。
7. 故障转移切换成功后 UI/托盘仍能显示真实 provider。
8. Usage 计价的 fallback 顺序不变：response model -> outbound model -> request model。
9. 未配置 channel 表时，旧 provider 主地址和 `provider_endpoints` 必须自动投影为默认 channel，用户无感迁移。
10. 配置了多 channel 后，旧 provider 粒度 UI 仍显示可理解的 provider 名称，但详情页应能展开到 channel。

## 风险与控制

| 风险 | 影响 | 控制 |
| --- | --- | --- |
| Header casing 保真被破坏 | Claude/Codex 客户端兼容性回退 | `hyper_client` 和 `server` 原始 header case 捕获先整体搬迁，增加集成测试 |
| Streaming guard 生命周期变化 | active connection 过早归零或泄露 | 保留 RAII guard 模型，把 guard 纳入 `ProxyResult` stream owner |
| 用量解析从响应 pipeline 拆出后漏记 | 统计不准 | `UsageSink` 只替换落库，不替换 parser；先 snapshot 当前 parser tests |
| 故障转移事件异步化后 UI 不更新 | 用户看不到实际 provider | 核心发事件，host adapter 同步现有 `hot_switch_provider` 路径 |
| OAuth token 刷新被移出 forwarder 后时序改变 | Copilot/Codex OAuth 请求失败 | `AuthProvider` 测试 token 缓存、刷新、失败回退 |
| Live 接管逻辑误入核心 | 独立模块仍不可复用 | import 防线和 code review checklist 强制拦截 |
| 一次性移动 4.5 万行导致冲突大 | 难 review、难回滚 | 按端口、engine、transport、crate 分阶段小提交 |
| channel 与 provider 健康边界混淆 | 一个地址失败误伤同 provider 其他地址 | 熔断、健康、auto-ban 全部以 `channel_id` 为主键，provider 只做聚合展示 |
| 模型映射歧义 | 请求被发往不支持的模型或计价错误 | `RouteResolver` 输出淘汰原因；`/proxy/v1/route/resolve` 支持 dry-run；usage 记录 public/upstream/pricing model |
| channel key 串用 | 多地址/多账号时认证泄漏 | `AuthProvider::resolve_auth` 同时接收 provider 和 channel；测试确保 header 不跨 channel 复用 |
| 权重随机导致测试不稳定 | 路由测试 flaky | route resolver 注入 deterministic RNG/seed；单元测试固定 seed |
| 旧 `provider_endpoints` 迁移重复 | 生成重复 channel 或顺序变化 | 迁移脚本按 provider_id + normalized base_url 去重，dry-run 输出差异 |

## 测试策略

### 单元测试

- domain 类型序列化/反序列化
- provider type 推断
- route classification
- route resolver group/interface/model filtering
- model mapping public -> upstream
- priority + weighted selection with deterministic seed
- channel status code mapping and auto-ban decision
- request middleware 顺序
- retry/max_attempts/circuit policy
- usage parser/calculator
- SSE chunk parser
- response body decompression

### 集成测试

使用本地 mock upstream：

1. Anthropic Messages non-stream。
2. Anthropic Messages SSE。
3. OpenAI Chat non-stream。
4. OpenAI Chat SSE with tool calls。
5. OpenAI Responses non-stream。
6. OpenAI Responses SSE。
7. Gemini `generateContent`。
8. Gemini `streamGenerateContent`。
9. Gemini `GET /models`。
10. Upstream 500/timeout 后故障转移。
11. Upstream 401/400 不故障转移。
12. HalfOpen provider 成功恢复。
13. request body 413 错误体转换。
14. 同 provider 两个 channel：不同 base URL、不同接口、不同模型映射。
15. 同模型多个 channel：priority 优先，同 priority 按 weight 分流。
16. group 限制：默认组不可见的 channel 不出现在 route-visible 模型 API（如 `/proxy/v1/apps/{app}/models`）和 route plan；Codex `/v1/models` 继续按客户端 raw catalog 兼容语义返回。
17. channel param/header override 只影响当前 channel。
18. channel test API 返回上游延迟、模型可用性和失败原因。

### 宿主适配测试

- `CcSwitchProviderSource` 与现有 DB provider 表兼容。
- `CcSwitchChannelSource` 可以从 provider 主 URL、`provider_endpoints` 和新 channel 表生成一致候选。
- 旧配置迁移 dry-run 可以输出新增、重复、需人工确认的 channel。
- `CcSwitchUsageSink` 写入 `proxy_request_logs` 字段完整。
- `CcSwitchEventSink` 可以驱动托盘和前端事件。
- `ProxyService` start/stop/update_config 行为不变。
- takeover on/off 不被核心 crate 影响。

## 推荐提交拆分

1. `test(proxy): lock current gateway behavior`
2. `refactor(proxy): introduce host-neutral domain types`
3. `refactor(proxy): model routable channels and model routes`
4. `refactor(proxy): add service ports for provider channel and config access`
5. `refactor(proxy): project legacy endpoints into channels`
6. `refactor(proxy): route request context through proxy services`
7. `refactor(proxy): move channel routing behind route resolver`
8. `refactor(proxy): move forwarding pipeline behind ProxyEngine`
9. `refactor(proxy): move HTTP transport to proxy core facade`
10. `refactor(proxy): move cc-switch persistence behind host adapters`
11. `refactor(proxy): split proxy core into path crate`
12. `feat(proxy): expose versioned channel management API`
13. `chore(proxy): remove legacy proxy facade`

## 验收清单

完成后应满足：

- `cc-proxy-core` 可以独立 `cargo test`。
- `cc-proxy-core` 不依赖 Tauri、SQLite Database、CC Switch settings/config/service modules。
- CC Switch 现有代理 UI 和 Tauri commands 行为不变。
- 外部调用方可以通过 Rust API 构造 `ProxyEngine` 并注入自己的 service adapters。
- 外部调用方可以通过 HTTP `/proxy/v1/*` 查询状态、channel、route dry-run 和集成事件。
- 每个 channel 可以独立配置 base URL、接口协议、模型映射、权重、优先级、key、header/param override 和健康策略。
- 旧 provider 主地址与 `provider_endpoints` 可以无损投影为 channel。
- 所有现有代理路由兼容。
- 所有流式/非流式协议转换兼容。
- 故障转移、channel 熔断、usage、header casing 都有测试覆盖。

## 参考资料

- NewAPI README: https://github.com/QuantumNous/new-api
- NewAPI channel model: https://github.com/QuantumNous/new-api/blob/main/model/channel.go

## 结论

这次重构的核心不是简单把 `src-tauri/src/proxy` 移到新目录，而是把代理能力从“桌面应用副作用驱动”改成“可注入宿主能力的中转引擎”。中转迁移的关键抽象是 channel：每个地址都能独立声明接口、模型、key、路由权重和健康策略。推荐先用 `proxy_core` 模块完成依赖倒置和旧配置 channel 投影，再拆成 path crate。这样可以在每一步保留可运行产品，降低大规模移动代码导致的行为回归风险。
