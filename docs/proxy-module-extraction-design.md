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
20. SSE field 解析、SSE block 分帧和跨 chunk UTF-8 拼接已迁入 `proxy-core::sse`；host 调用方已直接引用 core helper，`proxy::sse` 兼容模块已删除，现有 Claude/OpenAI/Responses/Gemini/Codex 流式转换路径继续复用同一实现。
21. 非流式兜底使用的 Chat Completions SSE 与 OpenAI Responses SSE 聚合器已迁入 `proxy-core::sse`；host handler 只保留 `ProxyError` 映射和缺失 chat completion id 时的 UUID 生成适配。
22. 非流式响应体的 `content-encoding` 提取和 gzip/x-gzip/deflate/br 解压算法已迁入 `proxy-core::response_body`；host `response_processor` 只保留 body 读取超时、日志和 transport 适配。
23. SSE data 行扫描、跨 chunk UTF-8 缓冲、`[DONE]` 判定和可选 JSON parse 已迁入 `proxy-core::sse::SseEventScanner`；host `response_processor` 只负责 stream timeout、日志、collector 回调和 usage 落库。
24. SSE usage 事件缓存、首个被收集事件计时和 finish-once 防重入已迁入 `proxy-core::sse::SseUsageAccumulator`；host `SseUsageCollector` 只保留异步互斥、usage 事件预过滤、parser/model extractor 回调和 `UsageSink` 落库适配。
25. Claude/OpenAI/Codex/Gemini 的 SSE usage 事件预过滤函数已迁入 `proxy-core::sse`；协议 parser 配置表也已由 `proxy-core::usage_config` 统一维护，host handler 直接消费 core 配置。
26. `TokenUsage` 与 Claude/OpenAI/Codex/Gemini 的 usage JSON 解析器已迁入 `proxy-core::usage`；host 调用方已直接引用 core 类型/常量，`proxy::usage::parser` 兼容模块已删除，usage request_id 的 message_id/session 前缀与 fallback 决策由 core 维护，host `usage_sink_bridge` 只注入随机 UUID 生成器。
27. Claude/OpenAI/Codex/Gemini 的流式 usage model extractor 已迁入 `proxy-core::usage`；host `handler_config` 兼容模块已删除，host handler/response processor 直接消费 core parser 配置。
28. `TokenUsage` 到 `UsageTokens` 的映射、success/error usage record 的 neutral record 构造、request/outbound/response model 归因规则已迁入 `proxy-core::usage`；host `usage_sink_bridge` 只保留 UUID 生成器注入和 provider meta 到 `ProviderKind` 的适配。
29. `UsageRecord` 到 `TokenUsage` 的 host 回填转换、pricing model override/request/response 选择规则已迁入 `proxy-core::usage`；`CcSwitchUsageSink` 只负责读取 DB 计价配置、查询定价、执行 Decimal 成本计算并写入 `UsageLogger`。
30. 非流式 body timeout 与流式 first-byte/idle timeout 的 failover-gated 选择规则已迁入 `proxy-core::response_timeout`；host `RequestContext` 只把 app 配置传入 core，并把返回的 `Duration`/`StreamingTimeoutConfig` 接到现有 transport。
31. Claude transform 是否走 streaming，以及 Codex OAuth Responses 非流请求是否聚合上游 SSE 的路由策略已迁入 `proxy-core::response_transform`；host 只负责识别 provider type 并执行对应的 stream/non-stream transport 分支。
32. 非流式 response body decode outcome、未知编码/失败解码原样透传策略，以及成功解码后的 entity header 清理已收敛到 `proxy-core::response_body::decode_response_body`；host 只根据 outcome 打日志。
33. route-visible 模型列表的管理 API envelope 已迁入 `proxy-core::RoutableModelList` 与 `ProxyEngine::list_model_catalog`；host `/proxy/v1/apps/{app}/models` 只解析 query 并返回 typed JSON。
34. Codex 转发层代理错误的 Responses 风格 JSON envelope、上游错误体归一化和 413 上游体积限制提示已迁入 `proxy-core::codex_error`；host 只把 `ProxyError` 映射成 fallback message/code/status/body。
35. `/proxy/v1/channels/{channel_id}/breakers/reset` 的管理 API response envelope 已迁入 `proxy-core::ChannelHealthResetResponse` 与 `ProxyEngine::reset_channel_health_response`；host 只做 path 解析和 typed JSON 返回。
36. `/proxy/v1/channels/{channel_id}` DELETE 与 `/proxy/v1/channels/{channel_id}/models` GET/PUT 的管理 API response envelope 和模型记录 contract 已迁入 `proxy-core::ChannelDeleteResponse`、`ChannelModelRecord` 与泛型 `ChannelModelsResponse<T>`；host 保留 DB CRUD 和 path 解析。
37. `/proxy/v1/groups` 的 route group 聚合规则和 response envelope 已迁入 `proxy-core::RouteGroupListResponse`；host 只负责按 app 查询可见 channel，并把 app/source/groups 投影为 core 输入。
38. `/proxy/v1/channels` GET 的管理 API list envelope 与 channel record payload 已迁入 `proxy-core::ChannelListResponse<ChannelRecord>`；host 继续负责 DB 列表查询和可选 app 过滤。
39. `/proxy/v1/apps` GET 的管理 API list envelope 已迁入 `proxy-core::AppListResponse` 与 `AppSummary`；host 继续负责读取 app 配置、provider 数量和 channel 数量。
40. `/proxy/v1/apps/{app}/providers` GET 的脱敏 provider summary 与 response envelope 已迁入 `proxy-core::ProviderListResponse`/`ProviderSummary`；host 继续负责 provider 查询、current/failover/routeCandidate 计算。
41. `/proxy/v1/apps/{app}/channels` GET 的普通 channel 列表 payload 与带过滤 dry-run 两种 response envelope 已迁入 `proxy-core::ChannelRecord`/`AppChannelResponse`；host 继续负责 provider router 查询和 route filter 解析。
42. `/proxy/v1/apps/{app}/routes/current` GET 的当前路由 response envelope、active target contract 与 configured provider summary 已迁入 `proxy-core::CurrentRouteResponse<T>`/`CurrentRouteTarget`/`CurrentRouteProviderSummary`；host 继续负责 active target runtime map 和当前 provider 查询。
43. `/proxy/v1/apps/{app}/channels/migration/preview` 与 `/materialize` 的 response envelope 和 preview channel payload 已迁入 `proxy-core::ChannelMigrationPreviewResponse<ChannelRecord>`/`ChannelMigrationMaterializeResponse`；host 继续负责旧 provider/endpoint 投影与 DB 写入。
44. `/proxy/v1/route/resolve` 的 request/response/candidate/rejected/source API contract 已迁入 `proxy-core::{RouteResolveRequest, RouteResolveResponse, ChannelRouteCandidate, ChannelRouteRejected, ChannelRouteSource}`；host `channel_routing` 只保留基于 DB channel record 的 dry-run 解析算法。
45. `/proxy/v1/route/resolve` 的 dry-run 过滤、淘汰原因生成、兼容接口匹配、候选排序算法和 circuit-open 候选转 rejected 的 response mutation 已迁入 `proxy-core::route_resolve`；host `channel_routing` 缩小为 `ProxyChannelRecord -> RouteResolveChannelInput` 适配层和 `AppError` 映射，source kind 字符串使用 DAO enum 的唯一出口。
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
65. 转发规划前的 request model 推断、Gemini path 模型提取和 inbound interface kind 推断已迁入 `proxy-core::request_url`；host 调用方直接引用 core helper。
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
81. Copilot optimizer 的 initiator/warmup/compact/subagent 请求分类策略已迁入 `proxy-core::request_optimizer`；host forwarder 直接调用 core 分类函数。
82. Copilot optimizer 的 assistant thinking/redacted_thinking block 剥离 mutation 已迁入 `proxy-core::request_optimizer`；host forwarder 直接调用 core mutation，调用顺序不变。
83. Copilot optimizer 的 orphan tool_result sanitize mutation 已迁入 `proxy-core::request_optimizer`；core 固化“只匹配紧邻上一条 assistant 的 tool_use”的 Anthropic 协议语义，host forwarder 直接调用 core mutation。
84. Copilot optimizer 的 tool_result/text block 合并 mutation 已迁入 `proxy-core::request_optimizer`；core 负责消息内 text 吸收与连续 tool_result-only user 消息合并，host forwarder 直接调用 core mutation。
85. Copilot optimizer 的生产调用点已从 host `copilot_optimizer` wrapper 改为直接调用 `proxy-core::request_optimizer`；测试用 host wrapper 也已删除，行为回归以 core `request_optimizer` 测试为准。
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
98. `/proxy/v1/channels` POST 与 `/proxy/v1/channels/{channel_id}` GET/PATCH 已迁入 `proxy-core::ChannelRecord` 与透明 `ChannelRecordResponse<T>`；外部 JSON shape 不变，handler 不再暴露 DB channel record。
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
111. OpenAI o-series/GPT-5+ reasoning model 识别，以及 Anthropic `output_config.effort`/`thinking` 到 OpenAI `reasoning_effort` 的映射已迁入 `proxy-core::request_body`；host Chat/Responses/Codex transform 调用方已直接引用 core helper，`transform` 不再对外 re-export 这些纯规则。
112. Claude Code 动态 `x-anthropic-billing-header` system 前缀剥离规则已迁入 `proxy-core::request_body::strip_leading_anthropic_billing_header`；OpenAI Chat/Responses host transform 调用方已直接引用 core 请求体规范化策略。
113. Anthropic `tool_choice` 到 OpenAI Chat Completions nested function selector 的映射已迁入 `proxy-core::request_body::map_anthropic_tool_choice_to_openai_chat`；Chat 请求整体转换也已由 `proxy-core::response_transform::anthropic_to_openai_chat_request` 承担。
114. Anthropic `tool_choice` 到 OpenAI Responses flat function selector 的映射已迁入 `proxy-core::request_body::map_anthropic_tool_choice_to_openai_responses`；Responses 请求整体转换也已由 `proxy-core::response_transform::anthropic_to_openai_responses_request` 承担。
115. OpenAI Responses `status`/`incomplete_details.reason`/tool-use 到 Anthropic `stop_reason` 的映射已迁入 `proxy-core::response_transform::map_openai_responses_stop_reason_to_anthropic`；Responses 响应整体转换由 core 抽取输入事实。
116. OpenAI Responses usage 到 Anthropic fresh input/output/cache bucket 的 JSON 组装已迁入 `proxy-core::usage::build_anthropic_usage_from_openai_responses`；host 非流式与流式 Responses transform 只传入上游 usage 节点。
117. OpenAI Responses function-call arguments 到 Anthropic `tool_use.input` 的 `Read` 空 `pages` 清理规则已迁入 `proxy-core::response_transform::{sanitize_anthropic_tool_use_input,sanitize_anthropic_tool_use_input_json}`；host 非流式与流式 Responses transform 只负责选择 Value 或 JSON 字符串入口。
118. OpenAI Chat Completions 流式请求 `stream_options.include_usage` 注入规则已迁入 `proxy-core::request_body::inject_openai_stream_include_usage`；Claude→Chat 与 Codex Responses→Chat 两个 host 转换路径直接复用 core helper。
119. OpenAI Chat/Responses 工具 JSON Schema 中不兼容的 `format: "uri"` 递归清理规则已迁入 `proxy-core::request_body::clean_openai_tool_schema`；host 转换层只负责选择 Anthropic tool 的 `input_schema`。
120. OpenAI Chat Completions `finish_reason` 到 Anthropic `stop_reason` 的映射已迁入 `proxy-core::response_transform::map_openai_chat_finish_reason_to_anthropic`；host 非流式与流式 Chat transform 只传入上游 finish_reason 和是否已有 tool_use 的事实。
121. OpenAI Chat Completions usage 到 Anthropic fresh input/output/cache bucket 的 JSON 组装已迁入 `proxy-core::usage::{build_anthropic_usage_from_openai_chat,build_anthropic_usage_from_openai_chat_tokens}`；host 非流式传入 usage 节点，流式传入已解析 token 计数。
122. Gemini `usageMetadata` 到 Anthropic fresh input/output/cache bucket 的 JSON 组装已迁入 `proxy-core::usage::build_anthropic_usage_from_gemini`；host 非流式与流式 Gemini transform 只传入上游 usageMetadata 节点。
123. Gemini `finishReason`/blocked 状态到 Anthropic `stop_reason` 的映射已迁入 `proxy-core::response_transform::map_gemini_finish_reason_to_anthropic`；host 非流式与流式 Gemini transform 只传入 finishReason、tool_use 和 blocked 事实。
124. Anthropic streaming `message_delta` event envelope 构造已迁入 `proxy-core::response_transform::build_anthropic_message_delta_event`；host Chat/Responses/Gemini streaming transform 只传入 stop_reason 与 usage JSON。
125. Gemini FunctionDeclaration schema 规范化、`parameters`/`parametersJsonSchema` 通道选择和无参数 object schema 兜底已整体迁入 `proxy-core::gemini_schema`；host Gemini request transform 只调用 core builder。
126. Chat/Responses 兼容上游 reasoning 字段与 summary item 的文本提取规则已迁入 `proxy-core::response_transform::{extract_reasoning_field_text,extract_reasoning_summary_text}`；host Codex Chat transform 与 core SSE 聚合复用同一套解析规则。
127. Codex Chat 中转配置中的 reasoning 请求开关判定与 effort 值映射已迁入 `proxy-core::request_body::{codex_chat_reasoning_requested,map_codex_chat_reasoning_effort}`；host 只负责把 provider config 投影为 core DTO。
128. Codex Chat history/inline-think 兼容所需的 response item call_id 提取、空 JSON 值判断和 `<think>` 前缀拆分规则已迁入 `proxy-core::response_transform`；host 调用方已改为直接引用 core，`codex_chat_common` 兼容模块已删除。
129. Gemini Native base URL/full URL 规范化、opaque relay 保护、Google/Vertex host 判定和模型 ID `models/` 前缀裁剪已整体迁入 `proxy-core::gemini_url`；host 调用方已改为直接引用 core，`proxy::gemini_url` 兼容模块已删除。
130. GitHub Copilot Claude 4.x 模型 ID dash/dot/`[1m]` 归一化与 live model id fallback 算法已迁入 `proxy-core::copilot_model_map`；host Copilot model map 只负责日志和 `CopilotModel.id` 投影。
131. Provider env 模型映射（haiku/sonnet/opus/fable/default）与 Claude Code 本地 `[1M]` 上游剥离规则已迁入 `proxy-core::model_mapping`；host `model_mapper` facade 已删除，forwarder 直接从 `Provider.settings_config` 构造 core `ModelMapping` 并调用 core 映射与日志 helper。
132. cache-sensitive JSON canonical string、tool arguments 空值/结构化规范化和短 SHA-256 trace hash 已迁入 `proxy-core::json_canonical`；host 调用方已改为直接引用 core，`proxy::json_canonical` 兼容模块已删除。
133. Anthropic thinking `budget_tokens` 1024 约束错误触发判定、budget snapshot/result 和请求体整流规则已迁入 `proxy-core::thinking_budget_rectifier`；host 只负责把 `RectifierConfig` 投影成 core 开关，返回类型直接使用 core result。
134. Bedrock prompt cache 断点注入、既有 cache_control TTL 升级、system string 转 block、thinking block 跳过规则和 cache 优化日志消息策略已迁入 `proxy-core::cache_injector`；host `cache_injector` facade 已删除，forwarder 只负责 `OptimizerConfig` 投影和日志输出。
135. Anthropic thinking signature 错误触发判定、thinking/redacted_thinking block 清理、非 thinking signature 字段剥离和顶层 thinking 兜底删除规则已迁入 `proxy-core::thinking_rectifier`；host 只负责把 `RectifierConfig` 投影成 core 开关，返回类型直接使用 core result。
136. Bedrock thinking optimizer 的 haiku skip、opus/sonnet adaptive thinking、legacy budget 注入、anthropic_beta 去重规则和 thinking 优化日志消息策略已迁入 `proxy-core::thinking_optimizer`；host `thinking_optimizer` facade 已删除，forwarder 只负责 `OptimizerConfig` 投影和日志输出。
137. 客户端请求格式识别（Claude/Codex/OpenAI/Gemini/Gemini CLI）和 Claude/Codex session id 提取优先级已迁入 `proxy-core::session`；host `session` 只保留 `ProxySession` 生命周期对象和 UUID 生成适配，调用方直接引用 core session 类型。
138. 代理日志错误码契约（CB/SRV/FWD/FO/RSP/USG）已迁入 `proxy-core::log_codes`；host `log_codes` 兼容模块已删除，调用方统一直接引用 core 日志码。
139. 媒体图片块检测/替换、text-only 模型启发式、channel/provider model catalog 显式图片能力判断、unsupported image 错误文本识别和 unsupported image marker 常量已迁入 `proxy-core::request_media`；host 调用方直接引用 core，`ProxyError` 只在 forwarder 调用点适配为 status/body 事实。
140. 请求体私有字段递归过滤和白名单过滤已迁入 `proxy-core::request_body`；host `body_filter` 兼容模块已删除，调用方统一使用 core 的 request body helper。
141. Claude/OpenAI/Codex/Gemini usage parser 配置表、流式模型提取器绑定和 SSE usage 事件预过滤器绑定已迁入 `proxy-core::usage_config`；host 调用方已直接引用 core parser config/type，`handler_config` 兼容模块已删除。
142. Codex Chat/Responses 兼容所需的 reasoning_content 拼接、Responses function_call item 组装和 namespace/reasoning 附加规则已迁入 `proxy-core::response_transform`；host 非流式、streaming 与 history 调用方已直接引用 core helper。
143. Provider 认证 header value 构造和非法 credential 字节校验已迁入 `proxy-core::request_headers::auth_header_value`；host provider adapter 调用点直接引用 core helper，并只在调用处把 `ProxyCoreError::Auth` 映射为 `ProxyError::AuthError`。
144. Codex Chat Completions 到 Responses 的 response id/status 映射、usage JSON 形状归一化和 custom tool arguments input 提取规则已迁入 `proxy-core::response_transform`；host 非流式与流式调用点已直接引用 core，不再保留 Codex Chat provider facade。
145. Codex Responses 到 Chat 的 role 映射、pending reasoning 拼接/去重、assistant reasoning 回填和 tool_call reasoning_content 占位兜底已迁入 `proxy-core::response_transform`；完整 input 遍历状态机见第 159 条。
146. Codex Responses content 到 Chat content 的 text/refusal/image/file/audio 形状转换规则已迁入 `proxy-core::response_transform::responses_content_to_chat_content`；request 级遍历与转换编排也已收敛到 core。
147. Codex Responses instructions 文本归一化与 Chat system message 头部归并规则已迁入 `proxy-core::response_transform::{responses_instruction_text,collapse_system_messages_to_head}`；请求 envelope 组装顺序由 core 统一维护。
148. Codex Chat tool_search/custom tool call 到 Responses item 的 JSON 参数解析、fallback query 包装、custom input 提取和 reasoning_content 附加已迁入 `proxy-core::response_transform`；tool context 分发也由 core item builder 处理。
149. Codex Responses function/custom tool 定义到 Chat tools 的形状转换、namespace tool name 压平、tool name 提取，以及 custom/tool_search call 到 Chat tool_call 的 JSON 组装已迁入 `proxy-core::response_transform`；`CodexToolContext` 已归属 core，负责上下文索引、去重和 name/spec 反查。
150. Codex Responses function_call 到 Chat tool_call 的 id/arguments JSON 组装已迁入 `proxy-core::response_transform::responses_function_call_to_chat_tool_call`；context-aware 的 namespace chat name 解析入口也已迁入 `responses_function_call_to_chat_tool_call_with_context`。
151. Codex Responses function/custom/tool_search tool_choice 到 Chat function selector 的 JSON 组装与未知 tool_choice 原样透传规则已迁入 `proxy-core::response_transform::{responses_tool_choice_to_chat_function_selector,responses_tool_choice_to_chat_tool_choice}`；core 直接消费 `CodexToolContext`。
152. Codex Responses function_call_output/custom_tool_call_output/tool_search_output 到 Chat `role=tool` 消息的 call_id/content 组装已迁入 `proxy-core::response_transform::{responses_function_call_output_to_chat_tool_message,responses_client_tool_output_to_chat_tool_message}`；pending tool_calls flush 已由 core input 遍历状态机维护。
153. Codex Chat assistant reasoning 文本提取、inline `<think>` 拆分和 assistant message/refusal 到 Responses output item 的 JSON 组装已迁入 `proxy-core::response_transform::{chat_reasoning_text,chat_reasoning_to_response_output_item,chat_message_to_response_output_item}`；choices 遍历与 output 序列合并也由 core 维护。
154. Codex Chat tool_call 到 Responses item id 的 `fc_`/`ctc_` 前缀规则已迁入 `proxy-core::response_transform::response_tool_call_item_id`；非流式和 streaming 路径只传入 `CodexToolContext` 判定是否 custom tool。
155. Codex Responses/Chat tool context 的 chat tool 去重、chat name 到 spec 反查、namespace function name 映射和 tool_search_output 加载工具扫描已迁入 `proxy-core::response_transform::{CodexToolContext,build_codex_tool_context_from_request}`；provider `transform_codex_chat` fixture 已删除，协议回归测试迁入 core。
156. Codex Chat tool_call 的 chat_name/spec 到 Responses function/custom/tool_search item 的分发规则已迁入 `proxy-core::response_transform::{response_tool_call_item_id_from_chat_name,response_tool_call_item_from_chat_name}`；非流式与 streaming host 路径只传入 `CodexToolContext` 和已规范化 arguments。
157. Codex Responses 到 Chat 的 reasoning option 写入规则已迁入 `proxy-core::response_transform::{CodexChatReasoningOptions,apply_codex_chat_reasoning_options}`，包括 provider config 投影后的 `thinking`/`enable_thinking`/`reasoning_split` 开关、`reasoning_effort`/`reasoning.effort` 目标形状、DeepSeek/OpenRouter effort 值映射和显式 `none` 关闭语义；host provider 调用点只负责把 `CodexChatReasoningConfig` 投影成 core DTO，并传入模型是否原生支持顶层 `reasoning_effort` 的事实。
158. Codex Responses 到 Chat 的 context-aware tool name 解析已迁入 `proxy-core::response_transform::{responses_function_call_to_chat_tool_call_with_context,responses_tool_choice_to_chat_tool_choice}`；namespace function、custom tool、tool_search 和未知 tool_choice 的兼容分发不再由 host wrapper 维护。
159. Codex Responses input 到 Chat messages 的遍历状态机已迁入 `proxy-core::response_transform::append_responses_input_as_chat_messages`，包括 pending tool_calls flush、reasoning item 归属、assistant last-index 回填、顶层 content part 处理和 tool output 消息组装；request envelope helper 直接复用该 core 状态机。
160. Codex Chat `tool_calls`/legacy `function_call` 到 Responses output item 的遍历和 fallback call_id 规则已迁入 `proxy-core::response_transform::chat_tool_calls_to_response_output_items`；非流式 response helper 直接把 assistant message、reasoning 文本和 `CodexToolContext` 交给 core。
161. 非流式 Codex Chat completion 到 Responses response 的整体 JSON 组装已迁入 `proxy-core::response_transform::chat_completion_to_response_with_context`，包括 choices/message 校验、response id/status、reasoning/message/tool output 拼装、usage 归一化和 length incomplete_details；host handler 只在调用点映射 core 字符串错误。
162. Codex Responses 到 Chat Completions 的 request envelope 组装已迁入 `proxy-core::response_transform::responses_to_chat_completions_with_options`，包括 instructions/input/messages、max token 字段选择、temperature/top_p/stream 透传、reasoning option 应用、tools/tool_choice、extra passthrough、无 tools 时 tool 字段清理和 stream usage 注入；host forwarder 只负责传入 provider reasoning options、模型是否 o-series 和是否原生支持 `reasoning_effort` 的事实。
163. Codex streaming transform 与 handler 对 `CodexToolContext`、Chat usage/id/status 和 tool-call item builder 的依赖已改为直接引用 `proxy-core`，不再通过 `transform_codex_chat` facade 中转；provider `transform_codex_chat` 模块已删除。
164. Codex Chat SSE 转 Responses SSE 的无状态 helper 已迁入 `proxy-core::response_transform`，包括 `chat_delta_reasoning_text`、`leading_think_prefix_decision`/`ThinkPrefixDecision`、`extract_chat_sse_error` 和 `sse_event`；host streaming 状态机只消费 core 的协议判定与事件封装函数。
165. Codex Chat streaming 路径对 tool argument canonicalization 与 inline `<think>` 拆分 helper 的调用已改为直接引用 `proxy-core`，`proxy::json_canonical` 和 `codex_chat_common` 不再为 streaming 维持这些已迁移 helper 的兼容 re-export。
166. Codex Chat history 缓存/恢复路径已直接引用 `proxy-core` 的 response item call id 与 empty-value helper，并删除空置的 `codex_chat_common` 宿主兼容模块；历史缓存仍保留在宿主层，因为它依赖异步共享状态和跨请求生命周期。
167. Anthropic/OpenAI 转换模块与 forwarder cache trace 已直接引用 `proxy-core` 的 canonical JSON/hash helper，并删除 `proxy::json_canonical` 宿主兼容模块；canonicalization 现在只有 core 源头，减少迁移期间的双路径心智负担。
168. Response handler、通用 streaming、Gemini streaming、Codex Chat streaming、Responses streaming 和 Codex Chat history 的 SSE parsing/UTF-8 helper 调用已直接引用 `proxy-core`，并删除 `proxy::sse` 宿主兼容模块；宿主 streaming 状态机仍保留各自业务状态，但底层 SSE 文本处理只有 core 源头。
169. Gemini Native URL/model helper 调用已从 `forwarder` 直连 `proxy-core`，并删除 `proxy::gemini_url` 宿主兼容模块；Gemini URL 拼接、opaque full URL 保留和模型 id normalize 策略统一由 core 维护。
170. 请求体私有字段过滤的宿主 `proxy::body_filter` 兼容模块已删除；过滤实现、白名单规则和 schema field-name 保护测试均由 `proxy-core::request_body` 维护。
171. Circuit breaker 与 server 的日志码调用已改为直接引用 `proxy-core::log_codes`，并删除 `proxy::log_codes` 宿主兼容模块；CB/SRV/FWD/FO/RSP/USG 日志码继续由 core 作为稳定契约维护。
172. Handler/response processor 的 usage parser config/type 调用已改为直接引用 `proxy-core::usage_config`，并删除 `proxy::handler_config` 宿主兼容模块；协议级 parser 配置表继续由 core 维护。
173. Proxy usage、response pipeline、session usage services 与 proxy-core host adapter 的 `TokenUsage`/`SESSION_REQUEST_ID_PREFIX` 调用已改为直接引用 `proxy-core::usage`，并删除 `proxy::usage::parser` 宿主兼容模块；usage 解析类型与 session request-id 前缀只有 core 源头。
174. Forwarder 的 Claude transform gate 已直接引用 `proxy-core::claude_api_format_needs_transform`，并删除 `providers` 模块上的同名 re-export；provider module 不再为该 core 纯规则提供二次出口。
175. 测试专用 `proxy::copilot_optimizer` wrapper 已删除；Copilot optimizer 分类、ID、tool_result merge/sanitize 与 thinking strip 行为只在 `proxy-core::request_optimizer` 维护并测试，host forwarder 保留配置 gating 和日志。
176. Chat/Responses/Codex transform 对 reasoning model 判定、reasoning effort 解析和 billing header strip 的调用已改为直接引用 `proxy-core::request_body`，`transform` 模块不再作为这些 core helper 的二次 re-export 面。
177. Forwarder 对媒体降级日志中的 unsupported image marker 已直接引用 `proxy-core::request_media`，`media_sanitizer` 不再作为该 core 常量的二次 re-export 面。
178. `proxy::session` 与 `proxy::usage` 上剩余的 core 类型 re-export 已删除；response handler、session adapter 和 usage 调用方直接引用 `proxy-core` 类型。
179. Thinking budget/signature rectifier host wrapper 的 result/snapshot 类型别名已删除；wrapper 返回 core result 类型，只保留 `RectifierConfig` 到 core config 的投影。
180. Claude takeover service 对 `[1M]` 模型后缀的字符串剥离已直接引用 `proxy-core::model_mapping`；host `model_mapper` facade 已删除，不再为该纯字符串 helper 提供二次出口。
181. Channel dry-run adapter 已删除重复的 source kind 字符串映射，统一使用 `ProxyChannelSourceKind::as_str()` 作为 legacy/manual channel source label 源头。
182. Channel dry-run 中 circuit breaker open 的候选过滤已拆为“host 读取 breaker 状态 + core 按 channel id 执行 route response mutation”；`circuit_open` rejected reason 的 shape 由 `proxy-core::route_resolve::reject_unavailable_channel_ids` 维护。
183. Channel DAO 的 `ProxyChannelRecord` row 解析已统一到 `map_proxy_channel_row`，`list_proxy_channels_for_app`、全局列表和单 channel 查询不再维护重复字段映射。
184. `/proxy/v1/apps` 与 `/proxy/v1/apps/{app}/routes/current` 的管理 API response 组装已改为 core input factory：host 只传入 app/provider/runtime target 事实，`proxy-core::{AppSummaryInput, AppListResponse::from_app_inputs, CurrentRouteProviderSummaryInput, CurrentRouteResponse::from_inputs}` 负责 DTO 映射与外部 JSON shape。
185. `/proxy/v1/apps/{app}/channels/migration/preview` 与 `/materialize` 的管理 API response 组装已改为 core input factory：host 只传入 DB migration preview/materialize 事实，`proxy-core::{ChannelMigrationPreviewInput, ChannelMigrationPreviewResponse::from_input, ChannelMigrationMaterializeInput, ChannelMigrationMaterializeResponse::from_input}` 负责字段映射。
186. `/proxy/v1/groups` 的 source/group 输入包装已迁入 `proxy-core::RouteGroupSourceInput::from_route_source`；host 只传入 app、`ChannelRouteSource` 和 channel groups，不再手写 source label 或 `RouteGroupChannelInput` 包装。
187. Provider `settings_config.env` 到 `ModelMapping` 的字段投影已迁入 `proxy-core::ModelMapping::from_settings_config`；host `model_mapper` facade 已删除，forwarder 直接传入当前 provider settings 并调用 core 映射与日志 helper。
188. Claude/Kimi/DeepSeek/MiMo 兼容所需的 reasoning vendor hint 判定、OpenAI Chat `reasoning_content` 保留 gate、Anthropic tool-use history thinking block 规范化和占位符常量已迁入 `proxy-core::response_transform`；host Claude adapter 的生产路径直接传入 provider settings/api_format 并执行协议转换编排。
189. DeepSeek 官方 Anthropic endpoint 的 `thinking: disabled` 与 `output_config.effort`/`reasoning_effort` 冲突清理、endpoint 候选字段判定和官方 URL 常量已迁入 `proxy-core::response_transform`；host Claude adapter 的生产路径直接把 `Provider.settings_config` 传给 core helper。
190. Codex Responses -> Chat 中转的 reasoning profile 规范化、平台优先推断和模型厂商推断已迁入 `proxy-core::response_transform::{CodexChatReasoningProfile,infer_codex_chat_reasoning_profile,normalize_codex_chat_reasoning_profile}`，覆盖 DeepSeek/StepFun/Kimi/Moonshot/GLM/Qwen/MiniMax/MiMo 以及 OpenRouter/SiliconFlow 中转站形态；host Codex adapter 只负责从 `Provider` 提取 name/base_url/model 和显式 meta，再把 core profile 投影回兼容配置或请求侧 `CodexChatReasoningOptions`。
191. Codex OAuth / ChatGPT 反代的 Responses 请求契约已迁入 `proxy-core::response_transform::apply_codex_oauth_responses_request_contract`，包括 `store=false`、`reasoning.encrypted_content` include 去重、FAST mode `service_tier=priority`、删除 ChatGPT 后端不接受的 token/temperature/top_p 字段、补齐 instructions/tools/parallel_tool_calls 默认值以及强制 `stream=true`；调用点只负责判断当前目标是否是 Codex OAuth 并传入 core 契约函数。
192. OpenAI Responses 非流式响应到 Anthropic message 的 JSON 组装已迁入 `proxy-core::response_transform::openai_responses_to_anthropic_message`，包括 output_text/refusal 文本块、function_call 到 tool_use、Read 空 pages 清理、reasoning summary 到 thinking、status/incomplete 到 stop_reason 和 usage shape 映射；生产 handler/adapter 直接调用 core，host `transform_responses` 测试入口已删除，`ProxyError::TransformError` 映射只保留在调用点。
193. Anthropic Messages 到 OpenAI Responses 请求体的 JSON 组装已迁入 `proxy-core::response_transform::anthropic_to_openai_responses_request`，包括 system/instructions 归一化、billing header strip、messages 到 input 遍历、tool_use/tool_result 提升、图片 data URL、max_tokens/temperature/top_p/stream/tool_choice/tools/cache key 映射、reasoning effort 映射以及 Codex OAuth contract 应用；生产 Claude adapter 直接调用 core，host `transform_responses` wrapper 已删除，协议回归由 core 测试覆盖。
194. Anthropic Messages 到 OpenAI Chat Completions 请求体的 JSON 组装已迁入 `proxy-core::response_transform::anthropic_to_openai_chat_request`，包括 system message 合并、billing header strip、text/image/tool_use/tool_result/thinking/redacted_thinking 消息转换、o-series `max_completion_tokens`、reasoning_effort、tools/tool_choice 和可选 `reasoning_content` 兼容字段；生产 Claude adapter 直接调用 core，host `transform` wrapper 已删除，协议回归由 core 测试覆盖。
195. OpenAI Chat Completions 非流式响应到 Anthropic message 的 JSON 组装已迁入 `proxy-core::response_transform::openai_chat_to_anthropic_message`，包括 choices/message 校验、`reasoning_content` 到 thinking、文本/refusal content parts、tool_calls 和 legacy function_call 到 tool_use、finish_reason 到 stop_reason 以及 usage shape 映射；生产 handler/adapter 直接调用 core，host `transform::openai_to_anthropic` 测试入口已删除，`ProxyError::TransformError` 映射只保留在调用点。
196. 模型目录拉取的 URL 候选生成策略已迁入 `proxy-core::model_fetch::build_models_url_candidates`，包括 full URL 反推 `/v1/models`、版本段 `/vN` 处理、override 优先、已知 Anthropic-compatible 子路径剥离和顺序去重；host `services::model_fetch` 只负责 reqwest transport、日志、timeout 设置和 body 读取。
197. OpenAI-compatible `/models` 响应 DTO、`FetchedModel` 外部契约和响应解析/排序已迁入 `proxy-core::model_fetch::{FetchedModel,parse_models_response_bytes}`；命令层直接引用 core DTO，host service 不再维护模型目录协议结构或 DTO re-export。
198. Codex OAuth/ChatGPT 后端模型目录的 JSON shape 兼容解析已迁入 `proxy-core::model_fetch::parse_codex_oauth_models`，包括 `data`/`models`/`items`/顶层数组/`models` map、多字段模型 id 识别、默认 `Codex` owner、fallback key 和排序去重；host `services::codex_oauth_models` 只负责 reqwest transport、query/header 注入、timeout 设置和 body 读取。
199. GitHub Copilot live `/models` 的可选模型 DTO 与响应过滤解析已迁入 `proxy-core::copilot_model_map::{CopilotModel,parse_copilot_models_response_bytes}`；host `copilot_auth` 保留账号缓存、token、endpoint 解析和 HTTP 请求，命令层直接引用 core DTO。
200. Copilot OAuth/GHES 域名输入规范化已迁入 `proxy-core::copilot_model_map::normalize_github_domain`，包括协议剥离、path/query/fragment 剥离、小写化、端口保留和 userinfo/空值拒绝；host `copilot_auth` 在设备码和 token 轮询调用点直接把 core 字符串错误映射为 `CopilotAuthError::InvalidDomain`。
201. Copilot 多账号复合账号 ID 生成规则已迁入 `proxy-core::copilot_model_map::copilot_composite_account_id`，保留 github.com 数字 ID 的向后兼容语义，同时对 GHES 账号使用 `domain:user_id` 防止不同实例的用户 ID 冲突；host `copilot_auth` 只保留同名 wrapper 兼容既有调用点。
202. Copilot model map 的 host 兼容 facade 已删除；forwarder 直接调用 `proxy-core::{apply_copilot_model_normalization,resolve_copilot_model_against_ids}`，并在调用点保留原先的 normalization debug 日志和 live model resolve info 日志。
203. Codex client model catalog 的 raw JSON 摘要、provider settings 到 Codex catalog 模板投影、以及 catalog 文件反向简化解析已迁入 `proxy-core::model_fetch::{client_model_catalog_from_raw,build_codex_model_catalog_from_settings,simplify_codex_model_catalog}`；host `codex_config` 只保留 TOML 默认上下文窗口解析、模板加载、文件路径 stale guard 和实际读写。
204. provider settings 中的模型目录摘要策略已迁入 `proxy-core::model_fetch::provider_model_catalog_from_settings`，统一处理顶层 `model`、Anthropic/Gemini env 默认模型和 `modelCatalog.models` 的 `model/id/name` 字段；host `CcSwitchModelCatalogProvider::load_catalog` 只负责按 provider_id 从数据库读取 settings。
205. Codex Chat 上游模型选择策略已迁入 `proxy-core::request_body::{resolve_codex_provider_upstream_model,codex_provider_catalog_model_ids_from_settings,apply_codex_chat_upstream_model_policy}`，包括 settings model 优先、provider catalog 模型保留和 chat-completions provider 的 body model 改写；host Codex adapter 只保留 Provider/meta/TOML 提取与既有公开函数签名。
206. provider kind 的默认 endpoint 与 transform requirement 策略已迁入 `proxy-core::domain::ProviderKind::{default_endpoint,needs_transform}`，host `ProviderType` 仅作为适配器选择枚举并委托 core 处理稳定 provider kind 语义。
207. thinking signature rectifier 的 request body mutation 纯 facade 已删除；forwarder 直接调用 `proxy-core::{rectify_anthropic_request,normalize_thinking_type}`，host `thinking_rectifier` 只保留 `RectifierConfig` 到 core config 的投影和错误触发判断兼容入口。
208. thinking budget rectifier 的 request body mutation 纯 facade 已删除；forwarder 直接调用 `proxy-core::rectify_thinking_budget`，host `thinking_budget_rectifier` 只保留 `RectifierConfig` 到 core config 的投影和错误触发判断兼容入口。
209. media fallback 的图片检测与无条件 marker 替换纯 facade 已删除；forwarder 直接调用 `proxy-core::{contains_image_blocks,replace_image_blocks_with_marker}`，图片降级规则继续向 core 收敛。
210. `[1M]` 本地模型能力标记的上游剥离纯 facade 已删除；forwarder 直接调用 `proxy-core::{strip_one_m_suffix_for_upstream,strip_one_m_suffix_for_upstream_from_body}` 以及 core `ModelMapping` 投影、映射和日志 helper，host `model_mapper` facade 已删除。
211. session user_id 解析纯 facade 已删除；Copilot/Claude provider 会话缓存键直接调用 `proxy-core::session::parse_session_from_user_id`，host `session` 只保留 HeaderMap/UUID generator 到 core session extraction 的适配入口。
212. Copilot 多账号复合 ID 纯 facade 已删除；`copilot_auth` 的账号持久化、OAuth 完成和 legacy 迁移路径直接调用 `proxy-core::copilot_composite_account_id`，host 只保留 GitHub 域名错误映射、文件存储和 HTTP 认证流程。
213. 非流式响应 usage 归因的模型优先级已迁入 `proxy-core::usage::resolve_usage_response_model`；host `response_processor` 只负责 JSON 解析、日志任务调度和 Axum response 构造，不再内联 `usage.model -> body.model -> outbound -> request` 回退链。
214. Channel route 选中后的请求体 model override 规则已迁入 `proxy-core::request_body::apply_channel_route_model_override`；host `route_attempt` 只负责从 `ForwardAttempt` 提取 public/upstream model 并保留原 debug 日志，provider settings/meta 覆写仍留在宿主适配层。
215. `RouteSelection -> ChannelRouteCandidate` 的 core DTO 映射已迁入 `proxy-core::route_resolve::route_candidate_from_selection`；host `route_attempt` 不再手写 channel/provider/model/interface/priority 字段复制，只负责把 candidate 包装回当前 `ForwardAttempt`。
216. 模型目录 HTTP 拉取错误体截断策略已迁入 `proxy-core::model_fetch::truncate_model_fetch_error_body`；host `services::model_fetch` 不再持有 404/405 HTML body 截断长度规则或错误文案拼接。
217. Codex OAuth 模型目录 HTTP 错误体截断策略已迁入 `proxy-core::model_fetch::truncate_codex_oauth_models_error_body`，保留历史 `"..."` 后缀；host `services::codex_oauth_models` 不再持有失败响应截断和 JSON 解析入口。
218. `services::model_fetch::FetchedModel` host re-export 已删除；模型目录命令层直接引用 `proxy-core::FetchedModel`，host service 不再作为 core 模型目录 DTO 的二次出口。
219. `proxy::providers::copilot_auth::CopilotModel` host re-export 已删除；Copilot 命令层直接引用 `proxy-core::CopilotModel`，host `copilot_auth` 只保留认证、账号缓存、endpoint/cache 和 HTTP 拉取流程。
220. thinking signature/budget rectifier 的错误命中判断 wrapper 已删除；forwarder 直接调用 `proxy-core::{should_rectify_thinking_signature,should_rectify_thinking_budget}`，host `RectifierConfig` 仅负责投影 core 中立配置。
221. `proxy::media_sanitizer` host 模块已删除；forwarder 直接调用 `proxy-core::request_media` 的 text-only 图片替换和 unsupported-image 错误分类，原 host 回归样例迁入 core 测试。
222. `proxy::handler_context::extract_gemini_model_from_path` host wrapper 已删除；Gemini handler/context 直接引用 `proxy-core::extract_gemini_model_from_path`，路径解析测试保留在 core。
223. forwarder 内部的 `request_model_for_forward` / `interface_kind_for_forward` host wrapper 已删除；attempt 规划直接把 `AppType` 投影为 `proxy-core::AppKind` 后调用 core request-url helper。
224. `OptimizerConfig` 到 thinking optimizer/cache injector core config 的字段投影已收敛到 host 配置类型本身；`thinking_optimizer`/`cache_injector` host facade 已删除，forwarder 直接调用 core mutation 与 core report log helper。
225. Claude/Codex 非流式 handler 的 response JSON 转换已直接调用 `proxy-core::{openai_responses_to_anthropic_message,openai_chat_to_anthropic_message,chat_completion_to_response_with_context}`；host handler 只保留 `ProxyError::TransformError` 映射、日志、history 和 usage 编排。
226. `providers::adapter::auth_header_value` host facade 已删除；Claude/Gemini/Codex adapter 直接调用 `proxy-core::auth_header_value` 并在本地映射认证错误。
227. Claude provider adapter 的非流式 OpenAI/Responses 响应转换已直接调用 `proxy-core::{openai_chat_to_anthropic_message,openai_responses_to_anthropic_message}`；provider 响应转换 wrapper 仅保留测试路径。
228. Claude provider adapter 的 Anthropic->OpenAI Chat/Responses 请求转换已直接调用 `proxy-core::{anthropic_to_openai_chat_request,anthropic_to_openai_responses_request}`；OpenAI Chat/Responses provider transform wrappers 均已删除。
229. `providers::transform`、`providers::transform_responses` 和 `providers::transform_codex_chat` 模块均已删除；生产构建不再编译旧 provider facade，协议回归由 core 测试覆盖。
230. Codex Responses->Chat 上游请求转换已从 forwarder 直接调用 `proxy-core::responses_to_chat_completions_with_options`，模型能力判定也直接使用 core helper；`providers::transform_codex_chat` fixture 模块已删除，回归覆盖迁入 core 测试。
231. Gemini Native endpoint rewrite 的 request model 读取已在 forwarder 直接从请求体提取，并删除 `transform_gemini::extract_gemini_model` 纯 passthrough helper；Gemini transform 模块继续只保留实际协议转换与 shadow 状态维护。
232. Codex OAuth/ChatGPT 后端模型目录的请求契约已迁入 `proxy-core::model_fetch::build_codex_oauth_models_request`，包括 backend URL、`client_version` query、Bearer header、`originator` 和 `chatgpt-account-id` header；host `services::codex_oauth_models` 只负责按 core request plan 发送 HTTP。
233. OpenAI-compatible `/models` 单次候选请求的 Bearer auth header 契约已迁入 `proxy-core::model_fetch::build_openai_compatible_models_request`；host `services::model_fetch` 只负责按 core request plan 发送 HTTP。
234. Claude adapter 的 DeepSeek thinking-disabled 生产 wrapper 已删除；`normalize_anthropic_messages_for_provider` 直接调用 core mutation，provider-specific 回归样例保留在 host 测试中。
235. Claude adapter 的 Anthropic tool-thinking history 生产 wrapper 已删除；`normalize_anthropic_messages_for_provider` 直接调用 core gate/mutation，provider-specific 回归样例保留在 host 测试中。
236. Claude Desktop gateway `/v1/models` 的 response DTO 与 envelope 构造已迁入 `proxy-core::claude_desktop_gateway_auth`；host 只负责鉴权、provider 选择和 route 推断。
237. Claude takeover live 配置中的 `[1M]` 后缀检测已迁入 `proxy-core::model_mapping::has_one_m_suffix_for_upstream`；host `ProxyService` 只负责 role 字段写入。
238. Codex 客户端入口的规范 endpoint + 原始 query 拼接规则已迁入 `proxy-core::request_url::append_query_to_endpoint_path`；host handler 只提供 Axum URI 的 query。
239. Claude Desktop gateway 请求入口的 endpoint prefix 剥离规则已迁入 `proxy-core::request_url::strip_endpoint_prefix`；host handler 只提供原始 path/query 和可选前缀。
240. Claude/Gemini handler 的原始 endpoint path/query 构造已统一走 `proxy-core::request_url::append_query_to_endpoint_path`，并保留旧 handler 对空 query 的 `?` 语义。
241. Codex Responses->Chat endpoint rewrite 的 host passthrough wrapper 已删除；forwarder 生产路径和回归测试都直接消费 `proxy-core::request_url::rewrite_codex_responses_endpoint_to_chat`。
242. OpenAI-compatible `/models` 请求计划中的可选 User-Agent header 契约已迁入 `proxy-core::model_fetch::OpenAiCompatibleModelsRequest`；host `services::model_fetch` 只负责 reqwest 发送。
243. OpenAI-compatible 与 Codex OAuth 模型目录请求的 timeout，以及 OpenAI-compatible 候选端点的 404/405 回退判定已迁入 `proxy-core::model_fetch` request plan / retry policy；host 只负责按计划设置 reqwest timeout 并读取 response body。
244. 模型目录失败响应的 retry/fail 决策和 `HTTP {status}: {body}` 错误格式已迁入 `proxy-core::model_fetch::{openai_compatible_models_failure,codex_oauth_models_failure}`；host 只读取 response body。
245. OpenAI-compatible 模型目录拉取的 API key 空值校验已迁入 `proxy-core::model_fetch::validate_openai_compatible_models_api_key`；host 不再持有请求前校验文案。
246. OpenAI-compatible 模型目录拉取的候选端点编排已迁入 `proxy-core::model_fetch::fetch_openai_compatible_models_with_transport`，并通过 `OpenAiCompatibleModelsTransport` 端口隔离实际 HTTP；host `services::model_fetch` 只实现 reqwest transport、日志、timeout 设置和 body 读取。
247. Codex OAuth 模型目录拉取的 backend 请求编排、HTTP 失败映射、JSON parse 和模型抽取已迁入 `proxy-core::model_fetch::fetch_codex_oauth_models_with_transport`，并通过 `CodexOAuthModelsTransport` 端口隔离实际 HTTP；host `services::codex_oauth_models` 只实现 reqwest transport、query/header 注入、timeout 设置和 body 读取。
248. `/proxy/v1/apps/{app}/providers` 的 provider summary 输入投影与列表 response 组装已迁入 `proxy-core::ProviderListResponse::from_provider_specs`；host handler 只把宿主 `Provider` 适配为脱敏 `ProviderSpec` 并提供 current/failover/routeCandidate 事实，不再手写 category/sortIndex/icon/providerType 字段抽取。
249. `/proxy/v1/apps/{app}/routes/current` 的 configured provider summary 输入投影已迁入 `proxy-core::CurrentRouteProviderSummaryInput::from_provider_spec`；host handler 只负责读取当前 provider 并适配为脱敏 `ProviderSpec`。
250. `/proxy/v1/apps/{app}/routes/current` 的 active target response contract 已迁入 `proxy-core::CurrentRouteTarget`；host runtime 直接使用该 core DTO，外部字段仍覆盖 app/provider/channel/interface/publicModel/upstreamModel。
251. `/proxy/v1/groups` 的 route group source 输入投影已迁入 `proxy-core::RouteGroupSourceInput::from_channel_specs`；host handler 只把宿主 channel record 适配为中立 `ChannelSpec`，不再直接抽取 DB record 的 `groups` 字段。
252. `/proxy/v1/apps` 的 app summary 计数输入已迁入 `proxy-core::AppSummaryInput::from_specs`；host handler 只把 provider/channel DB 结果适配为 `ProviderSpec`/`ChannelSpec`，不再直接在 handler 里计算 provider/channel 数量。
253. Claude/Codex 转换后非流响应的 usage record 构造已迁入 `proxy-core::transformed_response_usage_record_with_request_id_fallback`；host handler 只负责注入 provider/app/runtime 事实、UUID fallback 和异步 `UsageSink` 写入。
254. 通用流式响应的 usage event parser/model extractor 到 `UsageRecord` 的组装已迁入 `proxy-core::streaming_response_usage_record_with_request_id_fallback`；host `response_processor` 只保留 SSE 异步收集、缺失 usage 诊断日志和 `UsageSink` 写入。
255. 通用非流式响应的 JSON body/model/usage parser 到 `UsageRecord` 的组装已迁入 `proxy-core::non_streaming_response_usage_record_with_request_id_fallback`；host `response_processor` 只负责整包读取、JSON parse 成败诊断和异步 `UsageSink` 写入。
256. `/proxy/v1/channels` POST、`/proxy/v1/channels/{channel_id}` GET/PATCH 的单条 channel record response payload 已迁入 `proxy-core::ChannelRecord` 并通过透明 `ChannelRecordResponse<T>` 返回；host 继续负责 DB CRUD，但外部 JSON 仍保持原始 channel record 形状。
257. Codex 兼容 `/v1/models` 的 raw catalog response contract 已迁入透明 `proxy-core::ClientModelCatalogResponse`；host handler 只负责调用 `ProxyEngine::client_model_catalog`，外部 JSON 仍保持 Codex 原始 catalog 形状。
258. `/proxy/v1/status` 与 Tauri `get_proxy_status` 的运行态 status response payload 已迁入 `proxy-core::ProxyRuntimeStatus`，HTTP 侧通过透明 `ProxyStatusResponse<T>` 返回；host 继续维护 runtime counter，但对外入口只做 host status 到 core DTO 的适配，外部 JSON 仍保持 snake_case status 字段和 active target 字段。
259. `/proxy/v1/channels/{channel_id}/models` 的 response payload 已迁入 `proxy-core::ChannelModelRecord`；host handler 只把 DB `ProxyChannelModelRecord` 通过 `ModelRoute` 适配为 core DTO，保留 public/upstream/capabilities/pricing/overrides 字段。
260. `/proxy/v1/channels`、`/proxy/v1/apps/{app}/channels` 和 migration preview 的 channel record payload 已迁入 `proxy-core::ChannelRecord`；host handler 只把 DB `ProxyChannelRecord` 逐字段适配为 core DTO，保留 sourceKind/sourceEndpointUrl/models/review 字段。
261. `/proxy/v1/events` 的 SSE data envelope 已迁入 `proxy-core::ProxyEventEnvelope`，connected/lagged 控制事件 payload 也由 core helper 生成；host `ProxyEventBus` 只负责广播、序号和 RFC3339 时间戳注入。
262. Tauri `get_proxy_takeover_status` 的接管状态 response DTO 已迁入 `proxy-core::ProxyTakeoverStatus`；host `ProxyService` 继续读取 CC Switch DB/live takeover 状态并构造该对外契约，调用方直接引用 core DTO。
263. Tauri `get_provider_health` 的 legacy provider 级健康状态 DTO 已迁入 `proxy-core::ProviderHealth`；host DAO 继续负责 provider_health 表查询、缺省 healthy 语义和阈值更新逻辑，避免与 channel health 聚合视图混用，调用方直接引用 core DTO。
264. Tauri `start_proxy`/`start_proxy_with_takeover` 的启动结果 DTO 已迁入 `proxy-core::ProxyServerInfo`；host `ProxyServer` 继续负责监听地址、端口和启动时间的运行态注入，调用方直接引用 core DTO。
265. Tauri `get_proxy_config`/`update_proxy_config` 的 legacy runtime config DTO 已迁入 `proxy-core::ProxyConfig`；host DAO 继续负责从 `proxy_config` 三行镜像读取/写入，core 固化默认端口、废弃 `request_timeout` 和新增 timeout 字段的 serde 兼容语义，调用方直接引用 core DTO。
266. Tauri `get_global_proxy_config`/`update_global_proxy_config` 与 `get_proxy_config_for_app`/`update_proxy_config_for_app` 的 management config DTO 已迁入 `proxy-core::GlobalProxyConfig`/`AppProxyConfig`；host DAO 继续负责三行镜像和单 app 行更新，`CircuitBreakerConfig`/默认值与 `AppProxyConfig` 投影已迁入 core，避免与 engine 端口用的 `ProxyGlobalConfig`/`ProxyAppConfig` 混用；全局与单 app 配置调用方已直接引用 core DTO。
267. Tauri settings 中与代理请求处理相关的 `RectifierConfig`、`OptimizerConfig`、`CopilotOptimizerConfig` 已迁入 `proxy-core`，并保留到 thinking/cache/rectifier core config 的投影 helper；host settings DAO 继续负责 settings 表 JSON 读写，`LogConfig` 仍留在 host 因其直接依赖应用日志初始化；三类 runtime 配置调用方已直接引用 core DTO。
268. host runtime `ActiveTarget` 重复结构与别名出口已删除，现直接使用 `proxy-core::CurrentRouteTarget`；`ProxyStatus` 仍作为 host 可变计数状态存在，但 active target/current route 的外部 contract 已使用同一 core DTO。
269. Codex Chat Completions SSE 转 Responses SSE 的 response envelope 与 `response.failed` event 构造已迁入 `proxy-core::response_transform::{codex_chat_stream_response,codex_chat_stream_failed_event}`；这些 builder 现由 `CodexChatToResponsesState` 消费，host wrapper 不再直接构造 response envelope。
270. Codex Chat Completions SSE 转 Responses SSE 的 `response.created`、`response.in_progress`、`response.completed` 生命周期 event envelope 构造已迁入 `proxy-core::response_transform::{codex_chat_stream_started_events,codex_chat_stream_completed_event}`；开始/完成时机与 incomplete_details 注入现由 `CodexChatToResponsesState` 统一处理。
271. Codex Chat Completions SSE 转 Responses SSE 的 reasoning/message item 与 output_text/reasoning_summary added/delta/done event contract 已迁入 `proxy-core::response_transform` 的 `codex_chat_stream_*` builder；item id 分配、buffer 累积、done 标记和输出顺序现由 `CodexChatToResponsesState` 承接。
272. Codex Chat Completions SSE 转 Responses SSE 的 function_call arguments 与 custom_tool_call input delta/done event contract 已迁入 `proxy-core::response_transform` 的 `codex_chat_stream_*` builder；参数分片累积、custom/tool_search 判断和完成时机现由 `CodexChatToResponsesState` 承接。
273. Codex Chat Completions SSE 转 Responses SSE 的纯状态机已迁入 `proxy-core::CodexChatToResponsesState`，覆盖 chunk 处理、inline `<think>` 边界、text/reasoning/tool item 推进、完成/失败 envelope 生成和 stream-end 截断判定所需状态；后续 async stream transport、SSE block 读取、JSON parse 与错误桥接也已归入 core wrapper，host `streaming_codex_chat` 兼容入口已删除。
274. Codex Responses -> Chat Completions 的跨请求工具调用历史缓存与 request enrich 规则已迁入 `proxy-core::CodexChatHistoryState`；host `CodexChatHistoryStore` 只保留 `tokio::RwLock`、`Arc` 生命周期和 streaming response pass-through 记录 wrapper，避免 core 引入异步锁依赖。
275. Gemini Native thought signature 与 tool turn replay 使用的 shadow session 状态缓存已迁入 `proxy-core::GeminiShadowStore`；host provider 层不再声明 `gemini_shadow` 模块，Gemini request/streaming 转换路径直接消费 core 类型。
276. Gemini CLI/OAuth 凭证解析已迁入 `proxy-core::parse_gemini_oauth_credentials` 与 `GeminiOAuthCredentials`；host `GeminiAdapter` 只负责从 provider 配置提取原始 key，并保留 `OAuthCredentials` 类型别名以缩小调用面变动。
277. Copilot OAuth 的 public GitHub/GHES client id 选择、设备码/token/user/usage/token-exchange URL 派生和 Copilot API base fallback 已迁入 `proxy-core::copilot_*` helpers；host `CopilotAuthManager` 只负责 HTTP 调用、存储和错误映射。
278. OpenAI Responses SSE -> Anthropic SSE 的纯状态机已迁入 `proxy-core::OpenAiResponsesToAnthropicSseState`，覆盖 message_start、text/refusal/reasoning/tool blocks、interleaved tool args、Read 参数清理和 completed 收尾；后续 async stream、UTF-8/SSE block 读取和 JSON parse wrapper 也已归入 core wrapper，host `streaming_responses` 兼容入口已删除。
279. Gemini Native tool-call 参数 schema hint 提取与 args rectifier 已迁入 `proxy-core::gemini_tool_args`；host `transform_gemini` 保留原 API 名称和日志 wrapper，streaming/non-streaming Gemini 转换共用同一 core 规则。
280. Gemini Native streaming 的 `content.parts` 分析、tool-call snapshot 合并、合成 tool id 前缀识别和 shadow assistant parts 构造已迁入 `proxy-core::gemini_stream`；host 只提供 UUID 生成闭包与 rectifier 日志回调。
281. Gemini Native streaming 的 Anthropic SSE payload contract builder 已迁入 `proxy-core::gemini_stream`，覆盖 message_start、text block start/delta/stop、tool_use block start/input_delta/stop、message_delta 和 message_stop；事件输出时机与 SSE 字节编码现在由 core transport wrapper 统一处理。
282. Gemini Native streaming 的 parsed-chunk 状态机已迁入 `proxy-core::GeminiToAnthropicSseState`，覆盖 responseId/model/usage 捕获、message_start 首发、累计文本 delta 推进、blocked prompt fallback、text block close、tool_use flush、message_delta/message_stop 收尾和 shadow record 准备；host 只传入 tool schema hints、shadow store 与 session context。
283. Gemini Native streaming 的 SSE data 行提取、`[DONE]` 判断和 Gemini chunk JSON parse 已迁入 `proxy-core::GeminiToAnthropicSseState::handle_sse_block`；host 不再持有 Gemini stream block/data 解析逻辑。
284. Gemini Native streaming 的 UTF-8 安全拼接与完整 SSE block draining 已迁入 `proxy-core::GeminiToAnthropicSseState::handle_bytes`；host 不再从 upstream bytes 直接驱动状态机。
285. Gemini Native streaming 的 SSE event byte 编码已迁入 `proxy-core::GeminiStreamSseEvent::to_sse_bytes` / `encode_gemini_stream_sse`；host 不再持有 Anthropic SSE 序列化格式。
286. Gemini Native streaming 的 shadow record 写入决策已迁入 `proxy-core::GeminiStreamFinalOutput::record_shadow`，包括缺失 store/provider/session 时不消费待写入 record 的语义；host 只传入可选 store 与 session context。
287. `/proxy/v1/apps/{app}/models` 管理路由的 path/query 到模型目录请求输入已迁入 `proxy-core::AppModelCatalogRequest` 与 `ProxyEngine::list_model_catalog_for_request`；host handler 只保留 Axum 提取、错误映射和 JSON 包装。
288. `/proxy/v1/channels` 与 `/proxy/v1/groups` 的可选 `appType` filter 规范化与空值校验已迁入 `proxy-core::ChannelListRequest`/`GroupListRequest`；host handler 只根据 core request 调用 DB/router 并包装响应。
289. `/proxy/v1/apps/{app}/channels` 的 app path 规范化、route-filter 判定以及 `RouteResolveRequest` 构造已迁入 `proxy-core::AppChannelManagementRequest`；host handler 只按 core request 分派 list 或 dry-run router 调用。
290. `/proxy/v1/channels/{channel_id}`、`/models` 子资源和 breaker reset 的 channel path 规范化已迁入 `proxy-core::ChannelPathRequest`；host handler 不再直接处理 channel id trim/空值语义。
291. `/proxy/v1/apps/{app}/providers`、current route 和 channel migration 管理路由的 app path 规范化已迁入 `proxy-core::ManagementAppPathRequest`；host 仍保留 `AppType` 适配和 DB/router 执行。
292. `/proxy/v1/route/resolve` 的 request body appType 校验已迁入 `proxy-core::RouteResolveManagementRequest`；host handler 只把校验后的 body 交给 provider router dry-run。
293. 流式响应 usage record 的 outbound/fallback model 归因决策已迁入 `proxy-core::streaming_response_usage_record_with_optional_outbound_model`；host `response_processor` 只传入可选 outbound model 与 parser/extractor 端口。
294. 旧 host `proxy::response_handler` 兼容模块已删除；其未使用的 `ResponseType`、`StreamHandler`、`NonStreamHandler` 能力已由 `proxy-core::sse`/`usage` 与当前 `response_processor` 承接。
295. usage 成本计算领域对象与公式已迁入 `proxy-core::cost`（`CostCalculator`、`ModelPricing`、`CostBreakdown`）；host `proxy::usage` 只保留数据库日志写入与配置读取桥接。
296. host `proxy::events::ProxyEventEnvelope` re-export 已删除；事件总线仍保留在 host，SSE adapter 直接消费 `proxy-core::ProxyEventEnvelope`。
297. host `transform_gemini` 的 Anthropic tool schema hint 类型 re-export 已删除；Gemini streaming 与转换路径直接消费 `proxy-core::AnthropicToolSchemaHints`。
298. OpenAI Chat Completions SSE -> Anthropic SSE 的状态机已迁入 `proxy-core::OpenAiChatToAnthropicSseState`，覆盖 Chat chunk 到 Anthropic SSE event 的协议转换；async byte transport wrapper 继续在后续步骤收敛。
299. route dry-run 的 channel/model input 合约与 spec 入口已迁入 `proxy-core::RouteResolveChannelInput::from_channel_spec` / `resolve_channel_route_from_specs`；host 侧 DB record 到 route input 的投影由 `proxy_core_adapter` 承接，以便保留存储态 status/source kind 语义。
300. 转换后流式响应 usage record 构造已迁入 `proxy-core::transformed_streaming_response_usage_record_with_request_id_fallback`，覆盖 Claude/OpenRouter 与 Codex Chat->Responses 转换流的 usage 解析、全 0 usage 跳过、response/outbound/request 模型归因和 request_id fallback；host `handlers` 只保留 SSE 事件收集、`UsageSink` 调用与日志。
301. OpenAI Chat Completions SSE -> Anthropic SSE 的 async byte transport wrapper 已迁入 `proxy-core::create_openai_chat_to_anthropic_sse_stream`，覆盖 UTF-8 安全拼接、SSE block/data 行解析、上游错误事件和 stream 结束 finalization；原 host `providers::streaming` 兼容 re-export 已删除，端到端协议回归测试归入 `proxy-core::openai_chat_stream`。
302. OpenAI Responses SSE -> Anthropic SSE 的 async byte transport wrapper 已迁入 `proxy-core::create_openai_responses_to_anthropic_sse_stream`，覆盖 UTF-8 安全拼接、Responses SSE event/data 聚合、JSON parse 兜底跳过和上游错误事件；原 host `providers::streaming_responses` 兼容 re-export 已删除，端到端协议回归测试归入 `proxy-core::openai_responses_stream`。
303. `handlers` 对 OpenAI Chat/Responses 流式转换 wrapper 的生产调用已改为直接引用 `proxy-core`；host `providers::streaming` 与 `providers::streaming_responses` 均已删除，不再作为 re-export 面参与编译。
304. Codex Chat Completions SSE -> Responses SSE 的 async transport wrapper 已迁入 `proxy-core::create_codex_chat_to_responses_sse_stream_with_context`，覆盖 UTF-8 安全拼接、SSE block/data 行解析、`[DONE]` finalize、上游 error event 桥接、stream error 和截断 fallback；原 host `providers::streaming_codex_chat` 兼容 re-export 已删除，端到端协议回归测试归入 `proxy-core::response_transform`。
305. `handlers` 对 Codex Chat Completions SSE -> Responses SSE wrapper 的生产调用已改为直接引用 `proxy-core`，host `providers::streaming_codex_chat` 已删除，不再作为 re-export 面参与编译。
306. Gemini Native streaming 的 async upstream transport wrapper 已迁入 `proxy-core::create_gemini_to_anthropic_sse_stream_with_callbacks`，覆盖 upstream byte stream 驱动、上游错误转 `io::Error`、结束 finalization、shadow persistence wrapper 调用和 rectifier 通知回调；原 host `providers::streaming_gemini` 兼容入口已删除，streaming 回归测试归入 `proxy-core::gemini_stream`，生产 handler 直接调用 core wrapper 并绑定 host 的 id 生成与日志回调。
307. OpenAI-compatible 与 Codex OAuth 模型目录的 reqwest HTTP transport 已从 `services::model_fetch` / `services::codex_oauth_models` 收敛到 `proxy::model_fetch_transport`，由同一个 proxy host adapter 执行 core request plan、共享响应体读取和错误字符串语义；`services` 层只保留命令兼容薄壳。
308. Claude Messages -> OpenAI Responses 转换的 `prompt_cache_key` 来源选择已迁入 `proxy-core::resolve_claude_responses_prompt_cache_key`，覆盖显式 provider key、普通 Claude session、Copilot metadata.user_id `_session_` 后缀与 metadata.session_id fallback；host `ClaudeAdapter` 只提供 provider 事实并消费 core 返回的 key/source。
309. Gemini provider settings 中 `GEMINI_API_KEY`/`apiKey`/`api_key` 与 `GOOGLE_GEMINI_BASE_URL`/`base_url`/`baseURL` 的投影规则已迁入 `proxy-core::gemini_auth`；host `GeminiAdapter` 只保留 `Provider` 到 core settings helper 的适配和 `ProxyError` 映射。
310. 全局 HTTP client 避免系统代理回指 CC Switch 自身监听端口的递归保护已迁入 `proxy-core::request_transport`；host `http_client` 只负责读取环境变量和当前监听端口，并把 proxy URL 判定交给 core policy。
311. Channel CRUD 管理接口的 channel-not-found 文案、record envelope 和 models envelope 已收敛到 `proxy-core::ChannelPathRequest` helper；host handler 只保留 DB CRUD 和 host record 到 core record 的适配。
312. `/proxy/v1/groups` 的 app scope 展开、route group source input 构造和最终 response envelope 调用已收敛到 `proxy-core::GroupListRequest` helper；host handler 只遍历 core 给定的 app scope 并查询 channel source。
313. `/proxy/v1/apps/{app}/channels` 的普通 list 与 dry-run route 两种 response envelope 构造已收敛到 `proxy-core::AppChannelManagementRequest` helper；host handler 只负责执行 route dry-run 或 app channel 查询。
314. `/proxy/v1/channels` 的全局 list envelope 与 DELETE response envelope 已收敛到 `proxy-core::ChannelListRequest` / `ChannelPathRequest` helper；host handler 只负责按 request 查询或删除 DB 并投影 channel record。
315. `/proxy/v1/apps/{app}/routes/current` 与 `/channels/migration/*` 的 app path response envelope 已收敛到 `proxy-core::ManagementAppPathRequest` helper；host handler 只负责读取 active target、DB preview/materialize 结果和 provider summary 输入。
316. `/proxy/v1/apps/{app}/providers` 的 provider list response envelope 已收敛到 `proxy-core::ManagementAppPathRequest` helper；host handler 只负责读取 provider/current/failover/router 候选数据并投影 `ProviderSpec`。
317. Codex `/v1/models` reachability response wrapper 已收敛到 `ProxyEngine::client_model_catalog_response`；host handler 只负责调用 engine 并返回 typed JSON。
318. `/proxy/v1/apps` 的 app list response envelope 已收敛到 `proxy-core::AppListRequest` helper；host handler 只负责读取各 app 的 config/provider/channel 并构造 `AppSummaryInput`。
319. `/health` 与 `/status` 的 runtime response envelope 已收敛到 `proxy-core::HealthCheckRequest` / `ProxyStatusRequest` helper；host handler 只负责提供当前时间和 runtime 状态快照。
320. `/proxy/v1/channels` POST 的 create response envelope 已收敛到 `proxy-core::ChannelCreateRequest` helper；host handler 只负责把原始 write DTO 交给 DB 并投影创建后的 channel record。
321. channel record/model record 的批量 host-to-core 投影 helper 已从 handler 移到 `proxy_core_adapter::{proxy_channel_records_to_core, proxy_channel_model_records_to_core}`；handler 不再持有 DB record collection 映射细节。
322. provider spec、channel spec、app summary input 与 current-route provider summary 的 host-to-core 投影组合已移到 `proxy_core_adapter` 命名 helper；handler 只负责取得 DB/router 数据并调用 adapter。
323. 单条 channel record 与 runtime status 的 host-to-core 投影也已收敛到 `proxy_core_adapter::{proxy_channel_record_to_core, proxy_runtime_status_to_core}`；handler 不再直接导入 `ToProxyCore*` 投影 trait。
324. route dry-run 使用的 channel route input + source kind 投影已收敛到 `proxy_core_adapter::proxy_channel_route_inputs_to_core`；`proxy::channel_routing` 只负责调用 core resolver 和错误适配，且保留 DB 原始 status 文本用于 rejected reason。
325. handler 中直接构造 `ProxyEngine::new(state.proxy_core_services.clone())` 的重复逻辑已收敛到 `ProxyState::proxy_engine`；HTTP handler 不再关心 core service 容器的克隆方式，后续可在 state/adapter 层统一调整 engine 生命周期。
326. route dry-run 的 circuit-open channel id 到 rejected:circuit_open response mutation 已收敛到 `proxy-core::reject_unavailable_channel_ids`；`ProviderRouter` 只负责查询当前候选的 circuit breaker 可用性并传回不可用 channel id 列表。
327. provider failover/current 选择的纯策略已迁入 `proxy-core::provider_selection`；`ProviderRouter` 只负责读取 DB/settings/circuit breaker 事实并把 `ProviderSelectionFailure` 映射回既有 `AppError` 与 FO 日志。
328. `CircuitBreakerConfig` DTO、默认值和 `AppProxyConfig` 到熔断器配置/失败阈值的投影已迁入 `proxy-core::circuit_breaker_config`；host `proxy::circuit_breaker` 只保留状态机实现，调用方直接引用 core 配置类型。
329. provider/channel circuit breaker key、app type 解析和 app scope prefix 规则已迁入 `proxy-core::circuit_breaker_key`；`ProviderRouter` 不再手写 `app:provider` / `channel:app:channel` 字符串契约。
330. response runtime policy 已在 `proxy-core::response_timeout` 中统一产出 failover-gated timeout 和 `max_retries`；host `RequestContext` 不再手写 failover 关闭时 retry 清零规则。
331. Codex proxy error code 字符串契约已迁入 `proxy-core::codex_error`；host `error_mapper` 只负责把 `ProxyError` 映射为 core-neutral `CodexProxyErrorKind`。
332. `ProxySession::from_request` 中的 client format、model 和 streaming flag 派生已迁入 `proxy-core::proxy_session_request_metadata`；host session 只补 UUID、时间和 provider 运行态字段。
333. `ProxyError` 到 HTTP status code 的状态契约已迁入 `proxy-core::proxy_error_http_status_code`；host `ProxyError::into_response` 和 `error_mapper` 共用同一个 host-neutral status kind 映射。
334. channel route candidate 到 provider settings/meta 的覆盖计划已迁入 `proxy-core::channel_provider_override_plan`；host `route_attempt` 只负责把 core plan 写入 `Provider.settings_config` 和 `Provider.meta`。
335. Codex Chat history 的 Responses SSE block 检查已迁入 `proxy-core::inspect_codex_chat_history_sse_block`；host `codex_chat_history` 只负责异步 stream 扫描、锁状态和按 core inspection 写入 history state。
336. Claude transform endpoint rewrite 的 body-derived input projection 已迁入 `proxy-core::claude_transform_endpoint_rewrite_input_from_body`；host `forwarder` 不再本地提取 Gemini 模型/stream 标志，只传入 endpoint、api format、Copilot 标志和 request body。
337. Bedrock pre-send optimizer 的 provider settings env flag 投影已迁入 `proxy-core::bedrock_env_flag_from_provider_settings`；host `forwarder` 只负责把 `Provider.settings_config` 递交给 core 并执行实际优化器 mutation。
338. request body 私有字段过滤 report 的日志文案规则已迁入 `proxy-core::request_body_filter_log_message`；host `forwarder` 只负责执行实际 debug 日志输出。
339. response processor 的响应头日志摘要格式化已迁入 `proxy-core::response_headers_log_summary`；host `response_processor` 只负责把摘要写入现有 debug 日志。
340. prompt cache trace 的日志消息生成已迁入 `proxy-core::prompt_cache_trace_log_message`；host `forwarder` 只负责 debug 开关判断、传入 app/provider 事实并输出日志。
341. Claude API format 的 provider meta/settings 投影已迁入 `proxy-core::resolve_claude_api_format_from_settings`；host `ClaudeAdapter` 只负责传入 `Provider.meta` 与 `settings_config`。
342. Claude provider kind 推断策略已迁入 `proxy-core::infer_claude_provider_kind`；host provider registry 只负责提供 api format、auth strategy、base URL、meta provider type 和 settings facts。
343. Claude base URL 的 Codex OAuth override 与 settings/env 字段投影已迁入 `proxy-core::extract_claude_base_url_from_settings`；host `ClaudeAdapter` 只负责把缺失映射为现有配置错误。
344. Claude API key 来源优先级与裁剪规则已迁入 `proxy-core::extract_claude_auth_key_from_settings`；host `ClaudeAdapter` 只负责按 key source 输出既有日志并映射到认证策略。
345. Claude 上游 URL 拼接、Codex OAuth `/responses` 固定路径和 `/v1/v1` 去重已迁入 `proxy-core::build_claude_upstream_url`；host `ClaudeAdapter` 不再手写 URL 字符串策略。
346. Codex 上游 URL 拼接、origin-only 自动补 `/v1` 和 `/v1/v1` 去重已迁入 `proxy-core::build_codex_upstream_url`；host `CodexAdapter` 不再手写 URL 字符串策略。
347. Gemini adapter 上游 URL 拼接和 `/v1beta/v1beta`、`/v1/v1` 去重已迁入 `proxy-core::build_gemini_upstream_url`；host `GeminiAdapter` 不再手写 URL 字符串策略。
348. Codex 官方客户端 User-Agent 前缀判定已迁入 `proxy-core::is_official_codex_client_user_agent`；host `CodexAdapter::is_official_client` 兼容委托已删除，调用方和测试直接引用 core helper。
349. Codex Bearer auth header 构造和非法 header value 校验已迁入 `proxy-core::build_codex_bearer_auth_headers`；host `CodexAdapter` 只负责把 core auth error 映射为 `ProxyError`。
350. Gemini API key/OAuth auth header 构造、`x-goog-api-client` 固定值和非法 header value 校验已迁入 `proxy-core::build_gemini_auth_headers`；host `GeminiAdapter` 只负责传入当前 auth strategy 事实并映射错误。
351. Claude 非 Copilot 静态 auth header 策略（Anthropic x-api-key、Bearer、Google、GoogleOAuth、CodexOAuth originator）已迁入 `proxy-core::build_claude_auth_headers`；host `ClaudeAdapter` 只保留 GitHub Copilot 动态 request id/header 分支。
352. GitHub Copilot auth header 列表、固定 fingerprint header 和 request id/header 绑定规则已迁入 `proxy-core::build_copilot_auth_headers`；host `ClaudeAdapter` 只负责生成 request id 并传入 Copilot 版本常量事实。
353. Claude Responses prompt cache 的 Copilot provider 判定已迁入 `proxy-core::is_copilot_prompt_cache_provider`；host `ClaudeAdapter` 只传入 provider meta/settings 事实并沿用既有 cache-key 解析。
354. Gemini OAuth key 形态判定（trim 后 `ya29.` 或 JSON 起始）已迁入 `proxy-core::is_gemini_oauth_key_shape`；host Claude/Gemini provider registry 不再本地手写 `starts_with` 规则。
355. Claude forward API format 的 Copilot vendor 分流策略已迁入 `proxy-core::resolve_claude_forward_api_format`；host `RequestForwarder` 只负责从 CopilotAuthState 拉取 live model vendor 事实。
356. Codex Responses→Chat 转发时 base URL 是否已是完整 `/chat/completions` endpoint 的判断已迁入 `proxy-core::is_codex_chat_full_endpoint_base`；host `RequestForwarder` 只负责按 core policy 选择 full URL query 追加路径。
357. 请求头大小写保真策略的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::should_preserve_exact_request_header_case` 并只传入 adapter/provider/Copilot/api_format 事实。
358. Bedrock pre-send optimizer env flag 的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::bedrock_env_flag_from_provider_settings` 并只传入 provider settings。
359. Claude transform endpoint rewrite 的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::claude_transform_endpoint_rewrite_input_from_body` 和 `proxy-core::rewrite_claude_transform_endpoint`，回归测试也只覆盖 core 路径。
360. 上游请求体准备的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::prepare_upstream_request_body_with_report`，仅保留 host debug 日志输出。
361. prompt cache trace 的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::prompt_cache_trace_log_message` 并在 host 调用点保留 debug gate/输出。
362. 托管账号 `PROXY_MANAGED` 上游泄漏保护的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::validate_managed_account_upstream_auth` 并只在调用点映射为 `ProxyError::AuthError`。
363. media reactive retry 的 unsupported-image 错误判断 host wrapper 已删除；`RequestForwarder` 在 `ProxyError::UpstreamError` 分支直接调用 `proxy-core::is_unsupported_image_error` 并把布尔事实传给 core retry policy。
364. thinking signature/budget rectifier 的 `ProxyError` 错误消息提取 host helper 已删除；`RequestForwarder` 在两个 rectifier gate 调用点局部投影上游错误 body，并继续把文本事实交给 core 判定。
365. Copilot GitHub 默认域名的 host wrapper 已删除；`copilot_auth` 直接使用 `proxy-core::COPILOT_PUBLIC_GITHUB_DOMAIN` 常量，并在 serde default 中直接引用 `proxy-core::default_copilot_github_domain`。
366. Codex 官方客户端 User-Agent 检测的 adapter 静态 wrapper 已删除；provider 测试与调用方直接使用 `proxy-core::is_official_codex_client_user_agent`。
367. Copilot OAuth/GHES 域名 normalize host wrapper 已删除；`copilot_auth` 在设备码和 token 轮询入口直接调用 `proxy-core::normalize_github_domain` 并只在调用点映射 host 错误。
368. `CircuitBreakerConfig` host re-export 已删除；commands、services、DAO、server 和 provider router 直接引用 `proxy-core::CircuitBreakerConfig`，`proxy::circuit_breaker` 仅保留运行态状态机。
369. `ActiveTarget` host alias 已删除；runtime current target map、`ProxyStatus.active_targets` 和相关测试直接使用 `proxy-core::CurrentRouteTarget`。
370. `ProviderHealth` host re-export 已删除；`get_provider_health` 命令和 DAO 直接使用 `proxy-core::ProviderHealth`，host 只保留 provider_health 表查询与缺省 healthy 语义。
371. `ProxyServerInfo` host re-export 已删除；server、service 和 Tauri command 直接使用 `proxy-core::ProxyServerInfo`，host 只注入监听地址、端口和启动时间。
372. `ProxyTakeoverStatus` host re-export 已删除；`get_proxy_takeover_status` command 和 `ProxyService` 直接使用 `proxy-core::ProxyTakeoverStatus`，host 只负责读取 takeover enabled 事实。
373. `GlobalProxyConfig` host re-export 已删除；global config command 和 DAO 直接使用 `proxy-core::GlobalProxyConfig`，host 只负责三行镜像读写。
374. `AppProxyConfig` host re-export 已删除；per-app config command、DAO、handler context 和 core host adapter 直接使用 `proxy-core::AppProxyConfig`，host 只保留单 app 行读写与 runtime 注入。
375. `RectifierConfig` host re-export 已删除；settings command/DAO、handler context、forwarder 和配置回归测试直接使用 `proxy-core::RectifierConfig`，host 只保留 settings JSON 读写和 runtime 传递。
376. `OptimizerConfig` host re-export 已删除；settings command/DAO、handler context 和 forwarder 直接使用 `proxy-core::OptimizerConfig`，host 只保留 settings JSON 读写和 runtime 传递。
377. `CopilotOptimizerConfig` host re-export 已删除；settings command/DAO、handler context 和 forwarder 直接使用 `proxy-core::CopilotOptimizerConfig`，host 只保留 settings JSON 读写和 runtime 传递。
378. `ProxyConfig` host re-export 已删除；server、service、command、DAO 和回归测试直接使用 `proxy-core::ProxyConfig`，host 只保留 legacy runtime config 读写与 runtime 应用。
379. Chat Completions SSE 与 OpenAI Responses SSE 的非流式聚合协议回归测试已迁入 `proxy-core::sse`；host `handlers` 测试只保留 Codex proxy error JSON 等 HTTP/error adapter 行为。
380. Claude 非流式响应解析已把 Codex OAuth Responses 强制 SSE 聚合与未标记 SSE 兜底合并到 `proxy-core::parse_upstream_json_or_unlabeled_sse`；host `handlers` 不再保留 Responses SSE 聚合包装器。
381. Gemini synthesized tool-call id 的外部前缀、识别规则和 id shape helper 已迁入 `proxy-core::gemini_stream`；host `transform_gemini` 只负责生成随机 suffix 并调用 core helper。
382. Gemini 非流式响应里 `functionCall.id` 缺失/空字符串时的补齐 mutation 已迁入 `proxy-core::ensure_gemini_function_call_ids`；host 只保留随机 id 生成器和 shadow store 写入。
383. Gemini `functionCall` parts 到 `GeminiToolCallMeta` 的投影已迁入 `proxy-core::extract_gemini_function_call_meta`；host `transform_gemini` 不再维护本地 metadata 提取器。
384. Anthropic `tool_result.content` 到 Gemini `functionResponse.response` 的归一化已迁入 `proxy-core::normalize_gemini_tool_result_response`；host `transform_gemini` 只负责解析 tool_use_id/name 并调用 core 形状规则。

因此，本分支目前已把主要转发入口（Claude Messages、Claude Desktop Messages、Codex Chat Completions、Codex Responses、Codex Responses Compact、Gemini Native）切到 `ProxyEngine`，并开始把管理查询类能力、Codex 客户端模型目录、Codex client raw catalog response contract、Codex client model catalog response helper、status response contract、runtime health/status response helper、Claude Desktop gateway 模型列表 envelope、legacy channel 投影构造、channel 写请求规范化、channel source label single-source、channel dry-run route input projection helper、channel dry-run circuit-open response mutation、channel dry-run circuit-open id helper、provider selection policy、circuit breaker config contract、circuit breaker key contract、response runtime policy、Codex proxy error code contract、proxy session request metadata、proxy error HTTP status contract、channel provider override plan、Codex Chat history SSE block inspection、Claude transform endpoint rewrite input projection、Bedrock provider env flag projection、request body filter report log message、response header log summary、prompt cache trace log message、Claude API format settings projection、Claude provider kind inference policy、Claude base URL settings projection、Claude auth key settings projection、Claude upstream URL builder、Codex upstream URL builder、Gemini upstream URL builder、Codex official client UA policy、Codex Bearer auth header helper、Gemini auth header helper、Claude static auth header helper、Copilot auth header builder、channel DAO row mapping single-source、channel record projection helper、provider/channel spec projection helper、channel record response contract、channel model response contract、channel create response helper、channel CRUD not-found/envelope helpers、channel list/delete response helper、handler ProxyEngine factory、管理 API input factory、app list response helper、app summary spec-count input factory、migration response input factory、provider summary input factory、provider list response helper、current route provider summary input factory、current route target response contract、app path current-route/migration response helper、route group channel-spec source factory、route group app scope/response helper、app channel list/route response helper、托管账号上游安全保护、请求头 transport 策略、请求 header strip policy、provider auth header value validation、provider auth header helper facade deletion、Anthropic request header policy、Copilot fingerprint header policy、Codex OAuth session header 构造、ordered request header assembly、upstream auth header finalization、Copilot endpoint selection、forward failure log policy、provider failure retry classification、rectifier retry failover classification、thinking budget/signature rectifier、thinking rectifier result alias deletion、Claude reasoning vendor transform gates、Claude tool-thinking host wrapper deletion、DeepSeek thinking-disabled compatibility、DeepSeek thinking-disabled host wrapper deletion、Codex Chat reasoning profile inference、Codex OAuth Responses request contract、OpenAI Responses to Anthropic message assembly、Anthropic Messages to OpenAI Responses request assembly、Anthropic Messages to OpenAI Chat request assembly、Claude Responses prompt cache-key source policy、Copilot prompt-cache provider detection、Gemini OAuth key shape policy、Claude forward API format vendor policy、模型目录候选 URL 策略、模型目录响应解析、OpenAI-compatible 模型目录 transport 端口、Codex OAuth 模型目录解析与 transport 端口、Copilot live 模型目录解析、Copilot OAuth/GHES 域名规范化、Copilot 多账号复合 ID、Copilot model map host facade deletion、media fallback gate policy、media image downgrade/content detection、media unsupported marker re-export deletion、session/usage core type re-export deletion、model mapper string helper re-export deletion、model mapping settings projection、host model mapper facade deletion、request body private-field filtering、usage parser configuration、Codex Chat response item assembly、Codex Chat to Responses identity/usage/non-stream response mapping、Codex Responses to Chat request envelope/input traversal/reasoning carryover/reasoning option application、Codex Responses content to Chat content mapping、Codex Responses instructions/system message normalization、Codex Chat tool_search/custom call item assembly、Codex Responses tool definition to Chat tool mapping、Codex Responses function_call to Chat tool_call mapping、Codex Responses tool_choice to Chat function selector mapping、Codex Responses context-aware tool name resolution、Codex Responses tool output to Chat tool message mapping、Codex Chat tool_calls output traversal、Codex Chat assistant message/reasoning output item mapping、Codex Chat tool_call item id prefix policy、Codex tool context indexing/discovery、Codex Chat tool_call spec dispatch、Codex streaming direct core builder usage、Codex Chat SSE helper policy、Codex streaming canonical/think helper direct core usage、Codex Chat history helper direct core usage、host canonical JSON facade deletion、host SSE facade deletion、host Gemini URL facade deletion、host body filter facade deletion、host log code facade deletion、host handler config facade deletion、host usage parser facade deletion、host response handler compatibility deletion、usage cost calculator core migration、host event envelope re-export deletion、provider Claude transform gate re-export deletion、host Copilot optimizer wrapper deletion、transform core helper re-export deletion、Bedrock optimizer gate/thinking/cache injection policy、session identity extraction、proxy log code contract、usage request-id/model fallback policy、success/error usage record construction、transformed response usage attribution、transformed response usage record construction、streaming response usage record construction、streaming usage fallback model policy、non-streaming response usage record construction、SSE aggregate fallback diagnostics、非流式 JSON/SSE 解析兜底策略、Codex Chat 上游错误体归一化、Codex proxy error body 分类、转换响应 header 重建策略、转换后 JSON/SSE neutral response 构造、host response transport/error adapter、response parse error adapter、Copilot optimizer session/deterministic ID/fallback/warmup/classification policy、Copilot warmup model body override、Copilot thinking-strip/orphan-sanitize/tool-result-merge mutation、Copilot optimizer production call site、upstream send transport policy、global proxy loopback recursion policy、proxy event payload contracts、上游请求体准备/发送策略、上游请求 transport policy、上游请求 URL/query helper、endpoint rewrite policy、route hint inference、Provider env 模型映射、Gemini provider settings projection、canonical JSON/tool argument 规范化、Gemini Native streaming parts/snapshot helper、Gemini Native streaming SSE payload contract、Gemini Native streaming parsed-chunk 状态机、Gemini Native streaming SSE data parse、Gemini Native streaming byte/block state、Gemini Native streaming SSE byte encoding、Gemini Native streaming shadow persistence wrapper、Gemini Native streaming async transport wrapper、model catalog reqwest host adapter、management app model catalog request factory、management channel/group list request factory、management app channel route-filter request factory、management channel path request factory、management app path request factory、management route resolve request factory 和请求日志写入收敛到 core 可复用接口。实际模型目录 HTTP client 仍由 host adapter 执行；下一阶段需要把剩余 response runtime 编排、剩余外部管理 API 继续收敛到独立代理模块边界内。

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
| 工具能力 | `thinking_optimizer.rs`, `cache_injector.rs` | 请求前/错误后优化日志与宿主配置投影；私有字段过滤和 media fallback 已进入 `proxy-core` |

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
          request_body.rs
          cache_injector.rs
          request_media.rs
          thinking_budget_rectifier.rs
          thinking_optimizer.rs
          thinking_rectifier.rs
        observability/
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

Channel 管理 API 的部分 contract 也已开始收敛到 core：`management_api` 固化 appType、route resolve appType 和 channel_id path 参数校验，`HealthCheckResponse` 表示 `/proxy/v1/health` 存活检查结果，`ProxyChannelWriteRequest`/`ProxyChannelPatchRequest` 表示 channel 创建与更新请求，`ProxyChannelModelWriteRequest`/`ProxyChannelModelsReplaceRequest` 表示 channel 模型替换请求，`AppModelListQuery`/`AppChannelListQuery` 表示 app 模型目录和 app channel 列表的 query alias/归一化契约，`ChannelListQuery`/`GroupListQuery` 表示全局 channel 列表和 route group 列表的 appType query 契约，`AppListResponse`/`AppSummary` 表示 app namespace 列表结果，`ProviderSummaryInput`/`ProviderListResponse::from_provider_inputs` 负责脱敏 provider 候选列表和 current/failover/routeCandidate 标记，`AppChannelResponse` 表示 app 维度 channel 列表和带过滤 dry-run 候选结果，`AppChannelListResponse::from_route_source` 与 `AppChannelRouteResponse::from_route_resolve` 负责把 route source 和 dry-run 结果包装成稳定外部 envelope，`CurrentRouteResponse<T>`/`CurrentRouteTarget`/`CurrentRouteProviderSummary` 表示当前路由状态，`ChannelMigrationPreviewResponse<T>`/`ChannelMigrationMaterializeResponse` 表示旧 provider/endpoint 到 channel 表的迁移结果，`RouteResolveRequest`/`RouteResolveResponse`/`ChannelRouteCandidate`/`ChannelRouteRejected`/`ChannelRouteSource` 表示 `/proxy/v1/route/resolve` dry-run API contract，`ChannelRouteSource::as_str` 固化外部 source label，`stable_channel_id` 固化 legacy/manual channel 幂等 ID 规则，`resolve_channel_route` 负责 dry-run 的过滤、接口兼容、淘汰原因和排序规则，`ChannelRecord`/`ChannelListResponse<T>` 表示全局和 app channel 列表结果，`ChannelHealthResetResponse` 表示 breaker reset 结果，`ChannelDeleteResponse` 表示 channel 删除结果，`ChannelModelRecord`/`ChannelModelsResponse<T>` 表示 channel model 列表/替换结果，`RouteGroupListResponse` 负责 `/proxy/v1/groups` 的默认组补齐、group 计数、appTypes 聚合和 sources 去重排序。host handler 仍负责 HTTP path/query 提取、DB CRUD、provider router 查询和 host record 类型，但外部请求/响应 shape 不再散落在 handler 或 DAO 的本地 DTO 定义中。

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
