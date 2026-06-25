# 代理模块抽离与独立中转接口设计

> 分支：`refactor-proxy-channel-migration-module`
> 基线：`origin/main`
> 日期：2026-06-24
> 状态：已进入分阶段实现

## 当前落地状态

截至 2026-06-24，本分支已经完成以下迁移切片：

1. 新增 host-neutral `proxy_core` 领域模型和 `ProxyServices` 端口，主工程通过 `cc-switch-proxy-core` path crate 引用。
2. 新增 `proxy_channels`、`proxy_channel_models`、`proxy_channel_health` 等 channel 存储，并提供 legacy provider/endpoints 到 channel 的兼容投影。
3. 路由规划已进入 `ProxyEngine`：`plan_route` 支持 legacy projection，`plan_materialized_route` 只使用显式 materialized channel。
4. 现有 live `RequestForwarder` 的 materialized channel 尝试已改为经 `ProxyEngine` 规划，再映射回 host `ForwardAttempt` 执行，保持旧转发链路不回归。
5. `ProxyEngine::handle` 已拥有编排骨架：请求计数、route selected 事件、`ForwardPipeline` 端口调用、`UsageSink` 调用。
6. `CcSwitchEventSink` 已桥接到现有 `ProxyEventBus`，核心事件可以进入 `/proxy/v1/events` 的 SSE 流。
7. `UsageSink` 已升级为完整 `UsageRecord` 并可通过 `CcSwitchUsageSink` 写入现有 `proxy_request_logs`；response pipeline 与 handler 转换路径的成功 usage、forward error 日志都已改为走 `UsageSink`，协议入口的 core error 映射与 forward error usage 记录也已收敛到 `proxy_core_adapter::record_forward_core_error_usage`，app-specific 响应转换仍在 host 层。
8. 核心响应体已从请求 `ProxyBody` 拆出为 `ProxyResponseBody`，可以表达 empty/json/bytes/stream，避免把 axum/hyper 类型带入 core crate。
9. `RequestForwarder` 的重试循环已从 attempts 构建中拆出，新增 preplanned attempts 入口，并且预规划入口不再强制持有 `CcSwitchProxyServices`，避免 host runtime adapter 产生自引用。
10. `CcSwitchForwardPipeline` 已迁为 adapter-owned optional-runtime wrapper，`CcSwitchProxyServices<R>` 也已迁为 adapter-owned generic `ProxyServices` 容器，并接入运行中 `ProxyServer` 的共享 router/status/event/history/failover 运行态；`HostForwardRuntime for CcSwitchProxyRuntime` 仍留在 host 装配 DB、router、Tauri handle 等不可移植资源，把 `ProxyEngine::handle` 的 `RoutePlan` 映射为 host `ForwardAttempt` 并复用现有 HTTP 转发链。后续重点转为迁移 response pipeline、模型列表接口和外部管理 API。
11. Codex `/v1/chat/completions` handler 已改为构造 neutral `ProxyRequest` 并进入 `ProxyEngine::handle`；返回的 `ProxyResult` 暂时桥接回旧 `process_response`，保留现有 usage 解析、流式处理和响应构造。
12. Gemini handler 已改为进入 `ProxyEngine::handle`；模型名继续从 URI 提取，无模型的 `/models` 类端点不会把 `unknown` 写入 route filter，避免误过滤 channel。
13. Codex `/v1/responses` 与 `/v1/responses/compact` handler 已进入 `ProxyEngine::handle`；chat-to-responses 转换仍在 host 层执行，等待后续 response pipeline 迁移。
14. Claude 与 Claude Desktop `/v1/messages` handler 已进入 `ProxyEngine::handle`；核心 `ProxyResult` 会带回 `claudeApiFormat` 等宿主 metadata，host 侧继续复用现有格式转换、SSE/非流式响应处理和用量解析。
15. `ProxyEngine::list_models` 已提供按 app/group/interface 过滤的可路由模型视图，复用 channel source 的 legacy projection；`ProxyEngine::list_model_catalog` 负责生成 `/proxy/v1/apps/{app}/models` 的管理 API response envelope，返回模型对应的 provider/channel/interface 路由信息。
16. Codex 兼容 `/v1/models` 已从 handler 直读配置迁到 `ModelCatalogProvider::load_client_catalog` 与 `ProxyEngine::client_model_catalog`；CC Switch host adapter 保留 `model_catalog_json` stale guard 和 raw catalog 返回语义。
17. `/proxy/v1/channels/{channel_id}/breakers/reset` 已从 handler 直连 DB/router 改为 `ProxyEngine::reset_channel_health`；adapter-owned `CcSwitchChannelHealthStore` 负责同时清内存 circuit breaker 和持久化健康状态。
18. response pipeline 中的 hop-by-hop 响应头清理和重建 body 后实体头清理已迁入 `proxy-core::response_headers`，host `response_processor` 与特殊响应转换分支复用 core helper。
19. response pipeline 的 body 诊断摘要、content header 诊断后缀、SSE 聚合兜底失败诊断消息和未标记 SSE body 嗅探已迁入 `proxy-core::response_diagnostics`，host 只负责把诊断文本包装成现有 `ProxyError`。
20. SSE field 解析、SSE block 分帧和跨 chunk UTF-8 拼接已迁入 `proxy-core::sse`；host 调用方已直接引用 core helper，`proxy::sse` 兼容模块已删除，现有 Claude/OpenAI/Responses/Gemini/Codex 流式转换路径继续复用同一实现。
21. 非流式兜底使用的 Chat Completions SSE 与 OpenAI Responses SSE 聚合器已迁入 `proxy-core::sse`；host handler 只保留 `ProxyError` 映射和缺失 chat completion id 时的 UUID 生成适配。
22. 非流式响应体的 `content-encoding` 提取和 gzip/x-gzip/deflate/br 解压算法已迁入 `proxy-core::response_body`；host `response_processor` 只保留 body 读取超时和 transport 适配。
23. SSE data 行扫描、跨 chunk UTF-8 缓冲、`[DONE]` 判定和可选 JSON parse 已迁入 `proxy-core::sse::SseEventScanner`；host `response_processor` 只负责 stream transport 拆包、collector 创建和 Axum response 适配。
24. SSE usage 事件缓存、首个被收集事件计时和 finish-once 防重入已迁入 `proxy-core::sse::SseUsageAccumulator`；host `SseUsageCollector` 只保留异步互斥、usage 事件预过滤、parser/model extractor 回调和 `UsageSink` 落库适配。
25. Claude/OpenAI/Codex/Gemini 的 SSE usage 事件预过滤函数已迁入 `proxy-core::sse`；协议 parser 配置表也已由 `proxy-core::usage_config` 统一维护，host handler 直接消费 core 配置。
26. `TokenUsage` 与 Claude/OpenAI/Codex/Gemini 的 usage JSON 解析器已迁入 `proxy-core::usage`；host 调用方已直接引用 core 类型/常量，`proxy::usage::parser` 兼容模块已删除，usage request_id 的 message_id/session 前缀与 fallback 决策由 core 维护，随机 UUID 生成器注入由 `proxy_core_adapter` 承接。
27. Claude/OpenAI/Codex/Gemini 的流式 usage model extractor 已迁入 `proxy-core::usage`；host `handler_config` 兼容模块已删除，host handler/response processor 直接消费 core parser 配置。
28. `TokenUsage` 到 `UsageTokens` 的映射、success/error usage record 的 neutral record 构造、request/outbound/response model 归因规则已迁入 `proxy-core::usage`；host `usage_sink_bridge` 已删除，UUID 生成器注入和 provider meta 到 `ProviderKind` 的适配由 `proxy_core_adapter` 承接。
29. `UsageRecord` 到 `TokenUsage` 的 host 回填转换、pricing model override/request/response 选择规则已迁入 `proxy-core::usage`；adapter-owned `CcSwitchUsageSink` 负责读取 DB 计价配置、查询定价、执行 Decimal 成本计算并写入 `UsageLogger`。
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
43. `/proxy/v1/apps/{app}/channels/migration/preview` 与 `/materialize` 的 response envelope 和 preview channel payload 已迁入 `proxy-core::ChannelMigrationPreviewResponse<ChannelRecord>`/`ChannelMigrationMaterializeResponse`；旧 provider/endpoint 到 channel 的兼容迁移规划已由 `proxy-core::build_legacy_channel_migration_plan` 负责，host adapter 只装配本地 provider facts，DAO 只负责 DB 读写。
44. `/proxy/v1/route/resolve` 的 request/response/candidate/rejected/source API contract 已迁入 `proxy-core::{RouteResolveRequest, RouteResolveResponse, ChannelRouteCandidate, ChannelRouteRejected, ChannelRouteSource}`；host 不再保留 `channel_routing` 桥接模块，DB channel record 到 route input 的投影由 adapter 承接。
45. `/proxy/v1/route/resolve` 的 dry-run 过滤、淘汰原因生成、兼容接口匹配、候选排序算法和 circuit-open 候选转 rejected 的 response mutation 已迁入 `proxy-core::route_resolve`；`ProviderRouter` 只暴露 `RouteResolveChannelInput` 与 source，source kind 字符串使用 DAO enum 的唯一出口。
46. `/proxy/v1/channels` POST/PATCH 与 `/proxy/v1/channels/{channel_id}/models` PUT 的 request DTO 已迁入 `proxy-core::{ProxyChannelWriteRequest, ProxyChannelPatchRequest, ProxyChannelModelWriteRequest, ProxyChannelModelsReplaceRequest}`；host DB DAO 继续负责持久化、规范化和 provider/app 校验。
47. `/proxy/v1/health` 的 response contract 已迁入 `proxy-core::HealthCheckResponse`；host 只负责注入当前 RFC3339 时间并返回 typed JSON。
48. `/proxy/v1/apps/{app}/models` 与 `/proxy/v1/apps/{app}/channels` 的 query DTO 和别名归一化已迁入 `proxy-core::{AppModelListQuery, AppChannelListQuery}`；host handler 只负责 axum query 提取和调用 core/adapter。
49. `ChannelRouteSource` 的外部 source label 和 `/proxy/v1/apps/{app}/channels` list/route response 组装已迁入 `proxy-core::{ChannelRouteSource::as_str, AppChannelListResponse::from_route_source, AppChannelRouteResponse::from_route_resolve}`；host 不再手写 source 字符串映射。
50. 管理 API 鉴权策略已迁入 `proxy-core::management_auth`；token-source 决策已由 `proxy_core_adapter::management_auth_decision_from_proxy_config` 统一读取 `ProxyConfig` 与 `CC_SWITCH_PROXY_MANAGEMENT_TOKEN` fallback，host middleware 只负责读取 config guard、校验 header 并把 core 鉴权错误映射为现有 `ProxyError::AuthError`。
51. `/proxy/v1/apps/{app}/providers` 的 provider summary 打标和 response 组装已迁入 `proxy-core::{ProviderSummaryInput, ProviderListResponse::from_provider_inputs}`；host 只负责查询 provider/current/failover/routeCandidate 输入集合。
52. legacy/manual channel 的幂等 ID 生成规则已迁入 `proxy-core::channel_identity::stable_channel_id`；DB DAO 只负责调用 core 函数并写入 schema。
53. legacy channel 投影的 priority、interface kind、模型路由推断、endpoint 排序和 normalized base URL 去重规则已迁入 `proxy-core::legacy_projection`；host adapter 负责把宿主 `Provider`/TOML/env/meta 字段适配成 core migration input。
54. legacy channel 投影的默认字段、review 标记、metadata、auth profile 引用和 model channel id 绑定已迁入 `proxy-core::LegacyChannelProjection`；host adapter 负责映射回当前 `ProxyChannelRecord` 形状，DB DAO 只负责读取 legacy facts 和物化落库。
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
70. Copilot 上游判定和动态 endpoint 替换条件已迁入 `proxy-core::request_url::{is_github_copilot_upstream, should_resolve_copilot_dynamic_endpoint, resolved_copilot_dynamic_base_url}`；host forwarder 只消费 `proxy::managed_account_auth` 返回的账号 endpoint 事实并把运行时事实交给 core 决策。
71. provider 可重试失败、单 provider 失败、全部 provider 失败的日志 code/message 策略、上游错误摘要规则和 forward failure message 选择策略已迁入 `proxy-core::forward_failure`；`ProxyError` 到中立 `ForwardFailureKind` 的 host-only 事实投影已收敛到 `proxy_core_adapter`，host forwarder 只消费分类结果并执行实际日志输出。
72. provider failover eligibility 与健康度污染边界已迁入 `proxy-core::forward_failure::categorize_forward_failure`；400/405/406/413/414/415/422/501 等客户端请求错误不再由 host forwarder 本地判定，`ProxyError -> ForwardFailureKind` 也不再由 `error_mapper` 持有。
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
86. 非流式响应 JSON 解析失败后的错标 SSE 嗅探、Chat/Responses 聚合选择和解析/聚合失败诊断消息已迁入 `proxy-core::response_parse`；host `handlers` 只负责按协议传入 body/header/聚合事实，解析、fallback event 分发、解析失败日志和 core 错误到现有 `ProxyError` 的映射由 `proxy::error_mapper::{parse_claude_transform_upstream_json_or_unlabeled_sse,parse_codex_chat_upstream_json_or_unlabeled_sse}` 承接。解析失败日志的 Claude/Codex 前缀、body lossy 投影和未标记 SSE fallback context 选择已收敛到 adapter/error mapper，handler 不再直接维护该日志格式或协议 context。
87. Codex Chat 上游错误体的 JSON/文本解析、非 JSON 预览截断和 Responses 风格 error envelope 归一化已迁入 `proxy-core::codex_error::normalize_codex_chat_error_body`；host handler 只负责读取上游错误 body，非 JSON warning 与 neutral error response 构造由 `proxy_core_adapter::codex_chat_error_proxy_response` 承接，Axum response bridge 由 `proxy::response_adapter::codex_chat_error_response_to_axum_response` 承接。
88. 转换后 JSON 响应的实体/hop-by-hop/header content-type 重建策略，以及转换后 SSE 响应的固定 `text/event-stream`/`no-cache` 头，已迁入 `proxy-core::response_headers`；host 只负责把 core header 集合写入 Axum response builder。
89. 转换后 JSON 响应的 header 重建、body 序列化和 neutral `ProxyCoreResponse` 构造已迁入 `proxy-core::response_build::rebuilt_json_proxy_response`；host `handlers` 不再直接调用 core builder 或选择 Claude/Codex build context，而是通过 `proxy::response_adapter` 的协议专用 JSON helper 完成 build-error 映射和 Axum bridge。
90. 转换后 SSE 响应的固定 header、OK status 和 stream body neutral `ProxyCoreResponse` 构造已迁入 `proxy-core::response_build::transformed_sse_proxy_response`；host `handlers` 只保留 transform stream 生成、usage/timeout 包装，并通过 `proxy::response_adapter` 的协议专用 SSE helper 桥接 Axum。
91. `ProxyCoreResponse -> Axum Response` 与 `ProxyCoreResponse -> hyper_client::ProxyResponse` 的宿主 transport 桥接已从 `handlers` 抽到 `proxy::response_adapter`；handler 只保留协议入口编排和错误语义映射。Axum response builder 失败时的 Claude/Codex/通用 tag 诊断上下文已收敛到 adapter-owned `AxumResponseBuildErrorContext`，转换后 JSON/SSE 的 core response builder 调用也已收敛到 response adapter，生产 handler/response processor 不再散落 build-error 文案。
92. 转换后非流式响应的 usage 提取、全 0 usage 跳过和 response/outbound/request model 归因 fallback 已迁入 `proxy-core::usage::transformed_response_usage`；host `handlers` 只负责把 core 归因结果交给现有 `log_usage`。
93. `ProxyCoreError -> ProxyError` 的宿主错误适配已从 `handlers` 抽到 `proxy::error_mapper::proxy_core_error_to_proxy_error`；handler 不再承载 core/host 错误类型映射表。转换后 JSON/Codex 错误响应构造失败的日志上下文与 core-error 映射已进一步收敛到 `proxy::error_mapper::{response_build_error_to_proxy_error,codex_responses_error_body_build_error_to_proxy_error,codex_proxy_error_body_build_error_to_proxy_error}`，Claude/Codex 响应转换失败的日志上下文与 `TransformError` 包装已进一步收敛到 `proxy::error_mapper::{claude_response_transform_error_to_proxy_error,codex_chat_to_responses_transform_error_to_proxy_error}`。
94. response body parse/aggregation 专用错误适配已从 `handlers` 抽到 `proxy::error_mapper::response_body_parse_error_to_proxy_error`；core `Upstream` 诊断继续映射为 host `TransformError`，避免被通用 core error 映射当作转发失败。
95. Codex 代理错误响应的 `ProxyError -> Responses error body` 宿主分类已从 `handlers` 抽到 `proxy::error_mapper::codex_proxy_error_json`，JSON body 的 neutral response 构造已迁入 `proxy-core::response_build::json_proxy_response`；handler 只传入 provider/model/endpoint/error facts，neutral response 构造与 Axum bridge 由 `proxy::response_adapter::codex_proxy_error_to_axum_response` 承接。
96. `/proxy/v1/channels` 与 `/proxy/v1/groups` 的 query DTO 已迁入 `proxy-core::{ChannelListQuery, GroupListQuery}`；host handler 只负责 Axum query 提取、appType 校验和 DB/router 查询。
97. 管理 API 的 appType、route resolve appType 和 channel_id path 参数校验已迁入 `proxy-core::management_api`；host handler 只负责调用 core 校验并通过 `management_api_error_to_proxy_error` 保留旧 InvalidRequest 文案。
98. `/proxy/v1/channels` POST 与 `/proxy/v1/channels/{channel_id}` GET/PATCH 已迁入 `proxy-core::ChannelRecord` 与透明 `ChannelRecordResponse<T>`；外部 JSON shape 不变，handler 不再暴露 DB channel record。
99. 管理 API 鉴权错误的 `ManagementAuthError -> ProxyError` 宿主适配已从 `handlers` 抽到 `proxy::error_mapper::management_auth_error_to_proxy_error`；handler 只保留 header 提取和 core bearer 校验调用。
100. 管理 API bearer `Authorization` header 解析已迁入 `proxy-core::management_auth::validate_management_bearer_header`；host middleware 不再手写 header parse，只把 Axum/header map 交给 core 并映射错误。
101. Claude Desktop gateway 的 bearer header/token 校验和错误文案已迁入 `proxy-core::claude_desktop_gateway_auth`；host handler 只负责从 `claude_desktop_config` 读取/生成 gateway token 并映射 core auth 错误。
102. Claude 非流响应转换中未标记 SSE body 的 api_format -> 聚合策略映射已迁入 `proxy-core::response_transform::claude_transform_unlabeled_sse_aggregation`；当前由 `proxy_core_adapter::provider_claude_transform_streaming_decision` 统一携带给 JSON/SSE parse helper，handler 不再直接选择聚合策略。
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
192. OpenAI Responses 非流式响应到 Anthropic message 的 JSON 组装已迁入 `proxy-core::response_transform::openai_responses_to_anthropic_message`，包括 output_text/refusal 文本块、function_call 到 tool_use、Read 空 pages 清理、reasoning summary 到 thinking、status/incomplete 到 stop_reason 和 usage shape 映射；生产 handler/adapter 直接调用 core，host `transform_responses` 测试入口已删除，`ProxyError::TransformError` 包装通过 `proxy::error_mapper::claude_response_transform_error_to_proxy_error` 统一完成。
193. Anthropic Messages 到 OpenAI Responses 请求体的 JSON 组装已迁入 `proxy-core::response_transform::anthropic_to_openai_responses_request`，包括 system/instructions 归一化、billing header strip、messages 到 input 遍历、tool_use/tool_result 提升、图片 data URL、max_tokens/temperature/top_p/stream/tool_choice/tools/cache key 映射、reasoning effort 映射以及 Codex OAuth contract 应用；生产 Claude adapter 直接调用 core，host `transform_responses` wrapper 已删除，协议回归由 core 测试覆盖。
194. Anthropic Messages 到 OpenAI Chat Completions 请求体的 JSON 组装已迁入 `proxy-core::response_transform::anthropic_to_openai_chat_request`，包括 system message 合并、billing header strip、text/image/tool_use/tool_result/thinking/redacted_thinking 消息转换、o-series `max_completion_tokens`、reasoning_effort、tools/tool_choice 和可选 `reasoning_content` 兼容字段；生产 Claude adapter 直接调用 core，host `transform` wrapper 已删除，协议回归由 core 测试覆盖。
195. OpenAI Chat Completions 非流式响应到 Anthropic message 的 JSON 组装已迁入 `proxy-core::response_transform::openai_chat_to_anthropic_message`，包括 choices/message 校验、`reasoning_content` 到 thinking、文本/refusal content parts、tool_calls 和 legacy function_call 到 tool_use、finish_reason 到 stop_reason 以及 usage shape 映射；生产 handler/adapter 直接调用 core，host `transform::openai_to_anthropic` 测试入口已删除，`ProxyError::TransformError` 包装通过 `proxy::error_mapper::claude_response_transform_error_to_proxy_error` 统一完成。
196. 模型目录拉取的 URL 候选生成策略已迁入 `proxy-core::model_fetch::build_models_url_candidates`，包括 full URL 反推 `/v1/models`、版本段 `/vN` 处理、override 优先、已知 Anthropic-compatible 子路径剥离和顺序去重；host `services::model_fetch` 只负责 reqwest transport、日志、timeout 设置和 body 读取。
197. OpenAI-compatible `/models` 响应 DTO、`FetchedModel` 外部契约和响应解析/排序已迁入 `proxy-core::model_fetch::{FetchedModel,parse_models_response_bytes}`；命令层直接引用 core DTO，host service 不再维护模型目录协议结构或 DTO re-export。
198. Codex OAuth/ChatGPT 后端模型目录的 JSON shape 兼容解析已迁入 `proxy-core::model_fetch::parse_codex_oauth_models`，包括 `data`/`models`/`items`/顶层数组/`models` map、多字段模型 id 识别、默认 `Codex` owner、fallback key 和排序去重；host `services::codex_oauth_models` 只负责 reqwest transport、query/header 注入、timeout 设置和 body 读取。
199. GitHub Copilot live `/models` 的可选模型 DTO 与响应过滤解析已迁入 `proxy-core::copilot_model_map::{CopilotModel,parse_copilot_models_response_bytes}`；host `copilot_auth` 保留账号缓存、token、endpoint 解析和 HTTP 请求，命令层直接引用 core DTO。
200. Copilot OAuth/GHES 域名输入规范化已迁入 `proxy-core::copilot_model_map::normalize_github_domain`，包括协议剥离、path/query/fragment 剥离、小写化、端口保留和 userinfo/空值拒绝；host `copilot_auth` 在设备码和 token 轮询调用点直接把 core 字符串错误映射为 `CopilotAuthError::InvalidDomain`。
201. Copilot 多账号复合账号 ID 生成规则已迁入 `proxy-core::copilot_model_map::copilot_composite_account_id`，保留 github.com 数字 ID 的向后兼容语义，同时对 GHES 账号使用 `domain:user_id` 防止不同实例的用户 ID 冲突；host `copilot_auth` 只保留同名 wrapper 兼容既有调用点。
202. Copilot model map 的 host 兼容 facade 已删除；forwarder 直接调用 `proxy-core::{apply_copilot_model_normalization,resolve_copilot_model_against_ids}`，并在调用点保留原先的 normalization debug 日志和 live model resolve info 日志。
203. Codex client model catalog 的 raw JSON 摘要、provider settings 到 Codex catalog 模板投影、以及 catalog 文件反向简化解析已迁入 `proxy-core::model_fetch::{client_model_catalog_from_raw,build_codex_model_catalog_from_settings,simplify_codex_model_catalog}`；host `codex_config` 只保留 TOML 默认上下文窗口解析、模板加载、文件路径 stale guard 和实际读写。
204. provider settings 中的模型目录摘要策略已迁入 `proxy-core::model_fetch::provider_model_catalog_from_settings`，统一处理顶层 `model`、Anthropic/Gemini env 默认模型和 `modelCatalog.models` 的 `model/id/name` 字段；adapter-owned `CcSwitchModelCatalogProvider::load_catalog` 负责按 provider_id 从数据库读取 settings。
205. Codex Chat 上游模型选择策略已迁入 `proxy-core::request_body::{resolve_codex_provider_upstream_model,codex_provider_catalog_model_ids_from_settings,apply_codex_chat_upstream_model_policy}`，包括 settings model 优先、provider catalog 模型保留和 chat-completions provider 的 body model 改写；host Codex adapter 只保留 Provider/meta/TOML 提取与既有公开函数签名。
206. provider kind 的默认 endpoint 与 transform requirement 策略已迁入 `proxy-core::domain::ProviderKind::{default_endpoint,needs_transform}`，host provider adapter 直接消费 core provider kind 语义。
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
225. Claude/Codex response 转换已收敛到 adapter/core helper：Claude handler 通过 `proxy_core_adapter::{provider_claude_transform_streaming_decision,provider_claude_transform_response_for_api_format,provider_claude_transform_sse_for_api_format}` 分发流式/非流聚合路由和 OpenAI Responses、OpenAI Chat、Gemini Native 非流/流式响应转换，Codex handler 通过 `proxy_core_adapter::{codex_chat_transform_streaming_decision,transform_codex_chat_response_with_history,transform_codex_chat_sse_with_history}` 统一执行 Chat->Responses 流式/聚合路由、非流/流式转换与 history 记录；host handler 只保留 usage 与 transport 编排，转换失败日志与 `ProxyError::TransformError` 包装由 `error_mapper` 协议专用 helper 承接。
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
276. Gemini CLI/OAuth 凭证解析已迁入 `proxy-core::parse_gemini_oauth_credentials` 与 `GeminiOAuthCredentials`；host `GeminiAdapter` 只负责从 provider 配置提取原始 key。
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
300. 转换后流式响应 usage record 构造已迁入 `proxy-core::transformed_streaming_response_usage_record_with_request_id_fallback`，覆盖 Claude/OpenRouter 与 Codex Chat->Responses 转换流的 usage 解析、全 0 usage 跳过、response/outbound/request 模型归因和 request_id fallback；host `handlers` 只调用 adapter 的协议专用 usage collector helper，不再直接选择 transformed usage format 或 stream event filter。
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
324. route dry-run 使用的 DB channel record 到 `RouteResolveChannelInput` 投影已收敛到 `proxy_core_adapter::proxy_channel_record_to_route_resolve_channel_input`；router channel source 直接返回 core route input，保留 DB 原始 status/source kind 文本用于 rejected reason。
325. handler 中直接构造 `ProxyEngine::new(state.proxy_core_services.clone())` 的重复逻辑已收敛到 `ProxyState::proxy_engine`；HTTP handler 不再关心 core service 容器的克隆方式，后续可在 state/adapter 层统一调整 engine 生命周期。
326. route dry-run 的 circuit-open channel id 到 rejected:circuit_open response mutation 已收敛到 `proxy-core::reject_unavailable_channel_ids`；`ProviderRouter` 只负责查询当前候选的 circuit breaker 可用性并传回不可用 channel id 列表。
327. provider failover/current 选择的纯策略、auto failover 启用 plan、failover 队列到 provider circuit lookup/candidate 的投影已迁入 `proxy-core::provider_selection`；`ProviderRouter`/Tauri command 只负责读取 DB/settings/circuit breaker 事实并把 core 决策映射回既有 `AppError`、String 错误与 FO 日志。
328. `CircuitBreakerConfig` DTO、默认值和 `AppProxyConfig` 到熔断器配置/失败阈值的投影已迁入 `proxy-core::circuit_breaker_config`；host `proxy::circuit_breaker` 只保留状态机实现，调用方直接引用 core 配置类型。
329. provider/channel circuit breaker key、app type 解析和 app scope prefix 规则已迁入 `proxy-core::circuit_breaker_key`；`ProviderRouter` 不再手写 `app:provider` / `channel:app:channel` 字符串契约。
330. response runtime policy 已在 `proxy-core::response_timeout` 中统一产出 failover-gated timeout 和 `max_retries`；host `RequestContext` 不再手写 failover 关闭时 retry 清零规则。
331. Codex proxy error code 字符串契约已迁入 `proxy-core::codex_error`；`ProxyError` 到 Codex proxy error facts/kind 的宿主投影已收敛到 `proxy_core_adapter`，host `error_mapper` 只保留兼容 wrapper。
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
355. Claude forward API format 的 Copilot vendor 分流策略已迁入 `proxy-core::resolve_claude_forward_api_format`；host `RequestForwarder` 只消费 `proxy::managed_account_auth` 返回的 live model vendor 事实。
356. Codex Responses→Chat 转发时 base URL 是否已是完整 `/chat/completions` endpoint 的判断已迁入 `proxy-core::is_codex_chat_full_endpoint_base`；host `RequestForwarder` 只负责按 core policy 选择 full URL query 追加路径。
357. 请求头大小写保真策略的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::should_preserve_exact_request_header_case` 并只传入 adapter/provider/Copilot/api_format 事实。
358. Bedrock pre-send optimizer env flag 的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::bedrock_env_flag_from_provider_settings` 并只传入 provider settings。
359. Claude transform endpoint rewrite 的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::claude_transform_endpoint_rewrite_input_from_body` 和 `proxy-core::rewrite_claude_transform_endpoint`，回归测试也只覆盖 core 路径。
360. 上游请求体准备的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::prepare_upstream_request_body_with_report`，仅保留 host debug 日志输出。
361. prompt cache trace 的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::prompt_cache_trace_log_message` 并在 host 调用点保留 debug gate/输出。
362. 托管账号 `PROXY_MANAGED` 上游泄漏保护的 host wrapper 已删除；`RequestForwarder` 直接调用 `proxy-core::validate_managed_account_upstream_auth` 并只在调用点映射为 `ProxyError::AuthError`。
363. media reactive retry 的 unsupported-image 错误判断 host wrapper 已删除；`RequestForwarder` 在 `ProxyError::UpstreamError` 分支直接调用 `proxy-core::is_unsupported_image_error` 并把布尔事实传给 core retry policy。
364. thinking signature/budget rectifier 的错误消息选择策略已迁入 `proxy-core::forward_failure::forwarder_rectifier_error_message`；host adapter 只把 `ProxyError::UpstreamError` 的可选 body 或其它错误文本投影为 core input，`RequestForwarder` 继续只消费 request source 的 rectifier plan。
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
385. Anthropic 请求标量参数到 Gemini `generationConfig` 的字段映射已迁入 `proxy-core::build_gemini_generation_config`；host `transform_gemini` 不再维护本地 generation config builder。
386. Anthropic 顶层/system message 到 Gemini `systemInstruction` 的文本收集与 envelope 构造已迁入 `proxy-core::build_gemini_system_instruction`；host 只把 core 错误文本映射成既有 `ProxyError::TransformError`。
387. Anthropic `tool_choice` 到 Gemini `toolConfig.functionCallingConfig` 的映射已迁入 `proxy-core::map_gemini_tool_choice_to_config`；host 只把 core 错误文本映射成既有 transform error。
388. Gemini shadow assistant content 重放时的 parts 提取和 synthesized/empty `functionCall.id` 剥离已迁入 `proxy-core::gemini_shadow_replay_parts`；host 只负责选择匹配 shadow turn 并调用 core replay 清洗。
389. Gemini assistant `tool_use` 到 shadow turn 的匹配策略已迁入 `proxy-core::find_matching_gemini_shadow_turn`；id 优先、名称/归一化名称 fallback 的规则不再由 host `transform_gemini` 维护。
390. Gemini shadow replay 所需的 tool name map、thought signature map 以及 replay parts 中 functionCall name 合并规则已迁入 `proxy-core::{build_gemini_shadow_tool_name_map, build_gemini_shadow_thought_signature_map, merge_gemini_shadow_tool_names, merge_gemini_shadow_thought_signatures, merge_gemini_function_call_names_from_parts}`；host 只持有当前请求的可变 map 状态。
391. 当前请求 assistant `tool_use` id/name 预扫描和不覆盖既有 shadow name 的 map seed 规则已迁入 `proxy-core::merge_gemini_assistant_tool_use_names`；host 只遍历消息并调用 core helper。
392. Anthropic message content block 到 Gemini Native parts 的映射、inlineData 校验、tool_use/functionCall、tool_result/functionResponse、synthesized id 隔离和 same-content fallback name 解析已迁入 `proxy-core::anthropic_message_content_to_gemini_parts`；host 只负责 shadow turn 选择与 `TransformError` 映射。
393. Anthropic message list 到 Gemini `contents` 的 role 映射、system message 跳过、shadow turn 截断对齐、id/name 优先匹配、shadow 使用去重和 replay fallback 编排已迁入 `proxy-core::anthropic_messages_to_gemini_contents`；host Gemini request transform 只负责读取 shadow session、构造顶层 envelope 和映射 core 错误。
394. Anthropic request 到 Gemini Native 顶层 request envelope 的组装已迁入 `proxy-core::anthropic_request_to_gemini_request`，覆盖 systemInstruction、contents、generationConfig、tools/functionDeclarations 和 toolConfig；host `anthropic_to_gemini_with_shadow` 只负责从 shadow store 读取 turns 并映射 core 错误。
395. Gemini Native 非流式 response 到 Anthropic message 的纯转换已迁入 `proxy-core::gemini_response_to_anthropic_message`，覆盖 blocked prompt refusal、parts rectification、missing functionCall id 合成、tool_use/text content 组装、usage/stop_reason 映射以及 shadow record 生成；host 只负责 rectifier 日志、shadow store 写入和 `TransformError` 映射。
396. host `transform_gemini::rectify_tool_call_parts` facade 已删除；非流式 response rectifier 现在由 core 产出 `rectified_tool_names`，host 仅消费这些名称写入既有日志。
397. Gemini request transform 的 shadow session 读取与 request envelope 调用 wrapper 已迁入 `proxy-core::anthropic_request_to_gemini_request_with_shadow`；host `anthropic_to_gemini_with_shadow` 只转发 store/provider/session 并映射 core 错误。
398. Gemini Native 非流式 response 转换后的 shadow store 写入 wrapper 已迁入 `proxy-core::gemini_response_to_anthropic_message_with_shadow`；host 只传入 store/provider/session、记录 rectifier 日志并映射 core 错误。
399. host `transform_gemini::{extract_anthropic_tool_schema_hints, rectify_tool_call_args}` passthrough facade 已删除；handler 和测试直接引用 `proxy-core` 的 tool schema hint/args rectifier contract。
400. Claude/Gemini 生产 request/response 调用点已从 host `transform_gemini` wrapper 改为直接调用 `proxy-core::{anthropic_request_to_gemini_request_with_shadow, gemini_response_to_anthropic_message_with_shadow, gemini_response_to_anthropic_message}`；host wrapper 仅保留测试/兼容入口，UUID suffix 生成和 rectifier 日志仍由 Tauri host 负责。
401. `providers::transform_gemini` host facade 已删除；Gemini request/response 协议规则只在 `proxy-core::{gemini_request, gemini_response, gemini_stream}` 维护，Tauri host 仅在生产调用点注入 UUID suffix、shadow store、日志和 `ProxyError` 映射。
402. `proxy::health` 空占位模块已删除；Provider 健康事实继续由现有 circuit breaker、health DAO/query 和 runtime status 路径表达，避免保留无行为 host surface。
403. `proxy::channel_routing` 桥接模块已删除；management dry-run route resolution 已收敛到 `proxy_core_adapter::management_route_response_from_router_source`，adapter 负责向 router 读取 core route channel input、调用 `proxy-core::resolve_channel_route` 并应用 circuit-open rejection；`ProviderRouter` 只提供 route channel inputs/source 和 live circuit availability 查询。
404. `proxy::session::ProxySession` 未使用 host 会话类型和重复解析测试已删除；session metadata、ClientFormat 和 session-id 解析规则由 `proxy-core::session` 维护，host `proxy::session` 仅保留 UUID 生成器注入适配。
405. 模型目录 HTTP transport 已从 `proxy::model_fetch_transport` 移到 `services::model_fetch_transport`；`proxy-core` 继续负责 OpenAI-compatible/Codex OAuth catalog 的请求计划和响应解析，host reqwest 执行层不再扩大代理转发模块表面积。
406. `providers::models::{anthropic, openai}` 未使用 DTO 模块已删除；Anthropic/OpenAI/Codex/Gemini 协议 request/response shape 继续由 `proxy-core` 的转换与端口类型维护，host provider 层不再保留死的协议模型副本。
407. Forwarder 的转换后请求体选择（Codex Responses→Chat、Claude protocol transform body、provider transform、passthrough）和映射后初始 outbound model 归因已收敛到 `ForwarderRequestSource::transform_request_body`；host forwarder 只保留 Codex history enrichment 与 Claude session/shadow 状态调用。
408. Codex Chat history enrichment 的启用 gate 已收敛到 `ForwarderProtocolStateSource::enrich_codex_chat_request` 输入；host forwarder 不再本地包裹 `codex_responses_to_chat` 条件，只传入状态源需要的 enabled 事实和可变 body。
409. 上游响应的 success/error 判定、成功响应 first-byte/body-read 准备和非成功响应 `ProxyError::UpstreamError` 投影已收敛到 `ForwarderResponseSource::finalize_upstream_response`；host forwarder 不再直接读取 `response.status().is_success()`。
410. 出站 body 定稿后的最终 outbound model 归因已收敛到 `ForwarderRequestSource::prepare_upstream_body` 输出；host forwarder 不再本地执行 prepared body model 覆盖初始 mapped model 的回退链。
411. 普通 forward failure 的重试决策、retryable provider 日志和 terminal failure 日志输入已进一步收敛到 `ForwarderRuntimeStateSource::{forward_failure_decision,terminal_forward_failure_log_for_error}`；host forwarder 不再本地串联 `ForwardFailureKind`、retryable 分类和日志 helper，只消费 source 的决策结果并执行 permit/健康状态副作用。
412. forwarder 主循环的 max attempts 上限判定和 warning 文案已收敛到 `ForwarderAttemptRuntimeSource::attempt_limit_reached`；host forwarder 不再本地展开 retry policy 的次数比较和日志文本，只在 source 返回 limit 时停止尝试。
413. legacy 单 provider 场景跳过 circuit breaker 的判定已收敛到 `ForwarderAttemptRuntimeSource::should_bypass_circuit_breaker`；host forwarder 不再本地检查 attempts 数量或 channel 形态，只把 source 返回的 bypass 事实传入 allow 端口。
414. thinking/media rectifier retry failure 的 provider/client 归因已进一步收敛为 `ForwarderRuntimeStateSource::rectifier_retry_failure_decision` 结构化决策；host forwarder 不再消费布尔 failover gate，只根据 source 返回的 provider failure 或 client failure 执行既有 permit、健康状态和错误返回流程。
415. terminal/no-available forward failure 的 runtime status 记录已收敛到 `ForwarderRuntimeStateSource::{record_no_available_provider_status,record_terminal_failure_status}`；host forwarder 不再接收中间状态文案，只在终态分支触发 source 写入状态。
416. ordinary forward failure 的 retryable error message 与日志投影已并入 `ForwarderRuntimeStateSource::forward_failure_decision`，non-retryable 分支改为直接调用 `record_forward_error_status`；host forwarder 不再对普通 forward 错误执行 `to_string()` 来填充熔断记录或 runtime status。
417. 托管账号运行态读取端口已迁入 `proxy-core::managed_account_auth::ManagedAccountRuntimeSource`；core 负责按 GitHub Copilot/Codex OAuth runtime plan 调用宿主 source 并组装 `ManagedAccountAuthResolution`，host adapter 只负责把 CC Switch Provider 的 account binding 投影为 account id，并实现 Tauri/Copilot/Codex OAuth 的默认 source。
418. Copilot 托管账号的动态 API endpoint 与 live model id 解析编排已迁入 `proxy-core::managed_account_auth`，host adapter 只负责把 Provider account binding 投影为 account id，并保留 base URL 日志与请求体 model 写回副作用。
419. Copilot live model vendor 的 runtime-source gate 已迁入 `proxy-core::managed_account_auth::resolve_copilot_model_vendor_with_runtime_source`；host adapter 只从请求体读取模型名并把 vendor 事实交给 Claude API format 规则。
420. rectifier client-side failure 的 runtime status 记录已收敛到 `ForwarderRuntimeStateSource::record_forward_error_status`；signature/budget rectifier 客户端失败分支不再直接对 `ProxyError` 执行 `to_string()` 或处理中间状态文案，只把错误交给 runtime source 写入状态。
421. success status 后的 failover switch target 已收敛到 `ForwarderRuntimeStateSource::record_success_status` 返回的 `ForwarderFailoverSwitchTarget`；host forwarder 不再把成功状态的 bool 决策重新投影为 provider id/name，只调度 source 返回的 target。
422. forwarder 四条成功返回分支已收敛到 `complete_successful_attempt` 单入口；`forward()` 返回结构化 `ForwarderUpstreamSuccess`，成功副作用、active target 更新、status/switch 调度和 `ForwardResult` 投影不再在主成功/media/signature/budget retry 分支重复展开。
423. per-attempt current provider 状态写入已收敛到 `ForwarderRuntimeStateSource::record_current_provider(&Provider)`；host forwarder 不再拆 `provider.id/name` 写 runtime status，只传递当前 provider 事实。
424. provider failure 名称投影已收敛到 `ForwarderRuntimeStateSource::{forward_failure_decision,record_provider_failure,record_provider_rectifier_retry_failure}` 接收 `&Provider`；host forwarder 不再拆 `provider.name` 构造重试日志或 runtime failure status。
425. 客户端侧 rectifier retry failure 与 ordinary non-retryable failure 的 runtime status 记录已统一走 `ForwarderRuntimeStateSource::record_forward_error_status`；对应决策枚举不再携带只用于状态写入的 `error_message` 字段，host forwarder 不再保留 message-only failure status helper。
426. ordinary/terminal forward failure 的 warning log 行格式已收敛到 `ForwarderRuntimeStateSource::{forward_failure_decision,terminal_forward_failure_log_line_for_error}`；host forwarder 不再拼接 `[app] [code] message`，只消费 source 返回的最终日志行并执行日志副作用。
427. media/signature/budget rectifier retry 的成功/失败日志行和 provider failure label 已收敛到 `ForwarderRuntimeStateSource::{rectifier_retry_success_log_line,rectifier_retry_failure_log_line,rectifier_retry_failure_label}`；host forwarder 只选择 `ForwarderRectifierRetryKind` 并执行日志/收尾副作用。
428. provider-scoped rectifier retry failure 状态记录已改为 `ForwarderRuntimeStateSource::record_provider_rectifier_retry_failure(&Provider, ForwarderRectifierRetryKind, &ProxyError)`；host forwarder 不再取回 label 或错误字符串再回传 source，label 与错误消息投影完全保留在 runtime source 内部。
429. forwarder max-attempt warning 行格式已收敛到 `ForwarderAttemptRuntimeSource::attempt_limit_reached(app, attempted, max)` 返回的 `ForwarderAttemptLimitReached::log_line`；host forwarder 不再拼接 app 前缀或保留中间 message 投影。
430. 成功后的 failover switch 调度已改为 `FailoverSwitchScheduler::schedule_switch(app, ForwarderFailoverSwitchTarget)`；host forwarder 不再拆 target 的 provider id/name 或自行把 app 投影为 owned string，调度输入转换留在 scheduler 边界。
431. attempt started/succeeded/failed phase 选择已收敛到 `ForwarderRuntimeStateSource::{emit_attempt_started,emit_attempt_succeeded,emit_attempt_failed}`；host forwarder 不再导入/传递 `AttemptEventPhase`，只触发语义化事件方法。
432. request-started、attempt 事件和 active route target 的 host forwarder 薄 wrapper 已删除；`RequestForwarder` 直接调用 `ForwarderRuntimeStateSource` 语义方法，channel route target 状态写入与 `route_selected` 事件回归测试下移到 source 层。
433. request-started runtime status 的 timestamp 生成已收敛到 `ForwarderRuntimeStateSource::record_request_started_now()`；host forwarder 不再直接调用 `chrono::Utc` 或传递中间时间字符串。
434. request lifecycle id 生成已收敛到 `ForwarderRuntimeStateSource::next_request_id()`；host forwarder 不再直接调用 `uuid::Uuid::new_v4()`，只消费 runtime source 提供的 request id。
435. active connection RAII guard 类型已从 `proxy::forwarder` 移到 `proxy_core_adapter::ActiveConnectionGuard`；forwarder/handler/response processor 继续传递同一 guard，但运行态连接计数生命周期类型归属 adapter 边界。
436. `CcSwitchProxyRuntime` 显式持有 `ProxyEventBus` 并供 `CcSwitchProxyServices` event sink 装配使用；`ForwarderRuntimeStateSource` 不再暴露 event bus 读出口，测试观测也改由 fixture 自持 event handle。
437. `CcSwitchProxyRuntime` 显式持有 current route target map 并供 provider/app summary source 装配使用；`ForwarderRuntimeStateSource` 不再暴露 `current_providers()` 读出口，只保留 active route target 写入语义方法。
438. `ForwarderAuthSource` 现在持有 managed-account runtime source 并负责托管账号 auth 解析；`RequestForwarder` 在上游 auth headers 阶段不再把 `ManagedAccountRuntimeSource` 作为 input 字段转手传递。
439. request-side 托管账号运行态决策已收进 `ForwarderRequestSource`：Copilot live model 覆写、Copilot dynamic base URL 和 Claude API format runtime resolution 不再由 `RequestForwarder` 直连 `ManagedAccountRuntimeSource`。
440. response finalization 的 response、streaming mode 与 timeout facts 已收敛为 `ForwarderResponseFinalizationInput`；`RequestForwarder` 不再以散参形式把响应读取/首包预读策略转手传给 `ForwarderResponseSource`。
441. host forward bridge 不再拆 `ForwarderRuntimeConfig` 的 timeout/retry/rectifier/optimizer 字段来构造 `RequestForwarder`；runtime config 作为整体进入 forwarder，由 forwarder 构造器在边界内完成 options 与三类 optimizer/rectifier config 投影。
442. channel response status mapping 的 response 与 selected channel facts 已收敛为 `ForwarderChannelResponseStatusInput`；`RequestForwarder` 不再以散参形式把 statusCodeMapping 所需事实转手传给 `ForwarderResponseSource`。
443. `ForwarderResponseSource` trait 不再暴露 `upstream_error_body` / `upstream_error_response` 内部 helper；非成功上游响应的 status/body 投影仍由默认 source 内部完成，并只通过 `finalize_upstream_response` 对 forwarder 暴露。
444. channel authProfileRef 到 provider/channel-key/ignore 的 attempt action 判定已迁入 `proxy-core::channel_auth_profile_action` 与 `ChannelAuthProfileAction`；provider 存在性、缺失 provider warning 和 channel-key 应用计划继续收敛到 `proxy-core::channel_auth_profile_provider_application` / `ChannelAuthProfileProviderApplication`，host adapter 不再维护本地 enum 或 resolution wrapper，只负责 provider/key 查询和 provider settings mutation。
445. `ForwarderResponseSource` trait 不再暴露 `prepare_success_response` 成功 readiness helper；非流式 body buffering 与流式首包 replay 仍由默认 source 内部执行，`RequestForwarder` 与外部替换 source 只面对 `finalize_upstream_response`。
446. `ForwarderRequestSource` trait 不再暴露 `request_body_model` 内部 JSON model probe；最终 body model label 与 outbound model attribution 仍由默认 source 内部计算，并通过 `transform_request_body` / `prepare_upstream_body` 的结构化结果对 forwarder 暴露。
447. `ForwarderRequestSource` trait 不再暴露 `transform_provider_request_body` provider-adapter branch helper；通用 provider transform 包装仍由默认 source 内部执行，外部替换 source 只面对 `transform_request_body` 的完整转换选择结果。
448. `ForwarderRequestSource` trait 不再暴露 `convert_codex_responses_to_chat_body` Codex bridge body helper；Responses 到 Chat Completions 的 body 转换仍由默认 source 内部执行，外部替换 source 只面对 `transform_request_body` 的完整转换选择结果。
449. `ForwarderRequestSource` trait 不再暴露 `optimize_copilot_request` Copilot optimizer sequencing helper；Copilot 分类、孤立 tool_result 清理、tool_result 合并、thinking strip 与 warmup 模型降级仍由默认 source 内部执行，外部替换 source 只面对 `prepare_copilot_request_optimization` 的完整优化 gate 结果。
450. `ForwarderRequestSource` trait 不再暴露 `apply_media_prevention` media replacement helper；预防式图片替换策略、text-only provider/model 判定与替换日志仍由默认 source 内部执行，外部替换 source 只面对 `apply_app_media_prevention` 与 `apply_claude_body_policies` 的行为级入口。
451. `ForwarderRuntimeStateSource` trait 不再暴露测试用 `status()` / `events()` 读 handle；默认 runtime source 仍持有 status、current route target map 与 event bus，但 forwarder 测试改由本地 fixture 保存观测 handle，外部替换 runtime source 只实现语义化状态/事件写入端口。
452. `ForwarderAuthSource` trait 不再暴露 `prepare_copilot_auth_optimization` direct helper；Copilot auth override 的 session id、deterministic request id 与 interaction id sequencing 仍由默认 auth source 内部执行，外部替换 auth source 只面对 `prepare_optional_copilot_auth_optimization` 的行为级入口。
453. `ForwarderAttemptRuntimeSource` trait 不再暴露 `should_bypass_circuit_breaker` legacy helper；默认 attempt runtime source 仍根据完整 attempts 列表执行单 provider 兼容 bypass，外部替换 source 只面对 `allow(ForwarderAttemptAllowInput)` 的行为级放行入口。
454. `ForwarderAttemptRuntimeSource` trait 不再暴露 `attempt_limit_reached` max-attempt helper；默认 attempt runtime source 在 `allow` 决策内先执行尝试上限判定，再进入 circuit breaker permit，外部替换 source 只返回结构化 `ForwarderAttemptAllowDecision`。
455. `ForwarderRuntimeStateSource` trait 不再暴露 `rectifier_retry_success_log_line` / `rectifier_retry_failure_log_line` 字符串 helper；默认 runtime state source 仍保留日志行投影并通过 `log_rectifier_retry_success` / `log_rectifier_retry_failure` 执行日志副作用。
456. `ForwarderRuntimeStateSource` trait 不再暴露 `terminal_forward_failure_log_line_for_error` 字符串 helper；默认 runtime state source 仍保留 terminal failure 日志行投影并通过 `log_terminal_forward_failure` 执行日志副作用。
457. `ForwarderFailureDecision::Retryable` 不再携带 `log_line` 字符串；默认 runtime state source 仍保留 retryable forward failure 日志行投影并通过 `log_retryable_forward_failure` 执行日志副作用。
458. `ForwarderAttemptAllowDecision::Stop` 不再携带 `ForwarderAttemptLimitReached` 日志 payload；默认 attempt runtime source 仍保留 max-attempt 日志行投影并在 `allow` 决策内执行日志副作用。
459. `ForwarderRuntimeStateSource::forward_failure_decision` 不再接收 app/provider/attempt retry log context；普通 forward failure 的可重试分类只依赖 `ProxyError`，provider/attempt facts 只进入 `log_retryable_forward_failure` 日志行为入口。
460. `ForwarderRequestSource` trait 不再暴露 `codex_responses_to_chat_enabled` gate helper；Codex Responses→Chat gate 事实并入 `ForwarderTransformPlan`，URL planning、protocol enrichment 与 body transform 共用同一个 plan 输出。
461. `ForwarderUpstreamUrlInput` 不再暴露拆开的 `codex_responses_to_chat` / `use_claude_transform` / `claude_api_format` URL facts；URL planning 直接消费 `ForwarderTransformPlan`，host forwarder 不再转手拆解 transform plan 字段。
462. `ForwarderRequestPreparationInput` 不再暴露拆开的 `needs_transform` / `codex_responses_to_chat` transport-policy facts；上游 body 定稿阶段直接消费 `ForwarderTransformPlan`，默认 source 内部再投影给 transport policy resolver。
463. `ForwarderRequestSource` 的 adapter 相关输入不再暴露拆开的 `adapter_name` / `is_claude_adapter` facts；Claude format/policy、transform plan、media retry、request parts 与 upstream log 均消费 `ForwarderAdapterContext`，host forwarder 不再充当 adapter fact bus。
464. protocol preparation 的 Claude transform 与 Codex chat enrichment 决策已收敛为 `ForwarderProtocolPreparation`；`RequestForwarder` 不再直接读取 `ForwarderTransformPlan` 的 `codex_responses_to_chat` / `use_claude_transform` / `claude_api_format_for_transform` 字段。
465. media/rectifier 相关 request-source 输入不再暴露拆开的 `rectifier_enabled` / `request_media_fallback` / `request_media_heuristic` 开关；Claude body policy、app media prevention、media retry 均消费完整 `RectifierConfig`。
466. request parts 构造与 upstream request 日志不再接收拆开的 `filtered_body` / `body_model_label` / `force_identity_encoding` facts；两者直接消费 `ForwarderPreparedRequest`，host forwarder 只保留发送/响应阶段仍需的 streaming 与 outbound model 事实。
467. `proxy::types::ApiFormat` 未使用预留枚举已删除；Claude/OpenAI/Gemini format 判断统一沿用 `proxy-core` 的 provider kind、client format 和 response transform contract。
468. `LogConfig` 已从 `proxy::types` 移到 `settings::LogConfig`；日志设置不再扩大代理运行态类型模块，proxy host types 只保留代理状态/备份等运行态数据。
469. `RectifierConfig` 的默认值、serde 和 core 检测投影测试已从 host `proxy::types` 迁入 `proxy-core::ports`；host proxy types 不再承担 core 配置契约测试。
470. `LiveBackup` 已从 `proxy::types` 移到 `database::LiveBackup`；Live 接管备份记录归属数据库持久化边界，proxy host types 只保留运行态 status。
471. host `ProxyStatus` 已收敛为 `proxy-core::ProxyRuntimeStatus`，并删除 host status 到 core status 的字段拷贝适配器；运行态 status 结构只在 core 维护。
472. `proxy::types` 兼容壳模块已删除；调用点不再依赖代理 host 本地 DTO 模块。
473. `proxy::session` host UUID adapter 已迁入 `proxy_core_adapter::extract_proxy_session_id`；session 提取策略继续由 `proxy-core::session` 维护，proxy host 模块不再保留 session 壳文件。
474. provider auth key/token 遮蔽算法已迁入 `proxy-core::mask_secret`；日志安全展示规则由 core 维护。
475. 全局代理 URL 日志遮蔽规则已迁入 `proxy-core::mask_url_for_log`；外部中转集成可复用同一 URL 脱敏规则。
476. `proxy-core::ProviderKind` 已补齐 Display 与 known-only FromStr 契约；host provider adapter 不再复制字符串别名规则。
477. `CircuitState`、`AllowResult` 与 `CircuitBreakerStats` 已迁入 `proxy-core::circuit_breaker_config`；host `proxy::circuit_breaker` 保留 Tokio 运行态实现。
478. channel 转发命中的运行态快照已迁入 `proxy-core::ResolvedChannelAttempt`，host `ForwardAttempt` 只持有本地 `Provider` 与 core channel attempt，不再维护重复的 channel attempt DTO。
479. host `ProviderType` 重复枚举已删除；provider 模块仅保留从 desktop `Provider`/`AppType` 投影到 core provider kind 的适配函数。
480. provider adapter 的认证 DTO 与策略枚举已迁入 `proxy-core::{ProviderAuthInfo, ProviderAuthStrategy}`。
481. host `providers::auth` 兼容壳已删除；provider adapter 不再保留本地认证 DTO 模块。
482. host `proxy::http_client::mask_url` 兼容 wrapper 已删除；全局代理 command 与 HTTP client 日志现在直接调用 `proxy-core::mask_url_for_log`。
483. host 顶层 `proxy::ProxyStatus` 兼容 alias 已删除；server、forwarder、services 与测试直接引用 `proxy-core::ProxyRuntimeStatus`。
484. host `providers::ProviderType` 兼容 alias 已删除；Claude/Gemini adapter、forwarder 和 provider tests 直接引用 `proxy-core::ProviderKind`。
485. `/proxy/v1/status` 的运行态快照读取已迁入 `proxy-core::RuntimeStatusSource` 与 `ProxyEngine::proxy_status_response`；host `handlers` 不再直接读取 `state.status` 或组装 status response envelope，只负责 HTTP JSON 适配。
486. Tauri/service 侧 `ProxyServer::get_status` 也已改为调用 `ProxyEngine::runtime_status`，HTTP 与桌面 command 共用同一个 runtime status source，避免继续维护两条状态快照读取路径。
485. host `proxy::circuit_breaker` 与 `proxy` 顶层的熔断 DTO re-export 已删除；provider router、commands 和测试直接引用 `proxy-core::{CircuitState, AllowResult, CircuitBreakerStats}`。
486. host `providers::{AuthInfo, AuthStrategy}` 兼容 alias 已删除；provider adapter trait、Claude/Codex/Gemini adapter 与 forwarder 直接引用 `proxy-core::{ProviderAuthInfo, ProviderAuthStrategy}`。
487. host `providers::gemini::OAuthCredentials` 类型别名已删除；Gemini adapter 的 OAuth 解析直接返回 `proxy-core::GeminiOAuthCredentials`。
488. `proxy_core_host` 内联的 `ForwardError/ProxyError -> ProxyCoreError` 映射表已迁入 `proxy::error_mapper`；core engine host adapter 的错误桥接集中到单一模块。
489. provider adapter、Claude/Codex/Gemini adapter、provider kind 推断和 forwarder 中残留的 `ProviderAuthInfo as AuthInfo` / `ProviderAuthStrategy as AuthStrategy` 本地别名已删除；认证 DTO 调用点直接使用 core 类型名。
490. proxy server、forwarder、response processor 测试、proxy service 与 `proxy_core_host` 中残留的 `ProxyRuntimeStatus as ProxyStatus` 本地别名已删除；运行态状态调用点直接使用 core 类型名。
491. `proxy` 顶层的 `CircuitBreaker`、`ProxyError` 与 `ProviderRouter` 兼容 re-export 已删除；代理模块内部调用点直接引用真实子模块路径。
492. `ProxyError -> ForwardFailureKind` 的宿主错误适配已从 `forwarder` 抽到 `proxy::error_mapper::forward_failure_kind_from_proxy_error`；forwarder 不再维护 provider failover 分类前置映射表。
493. reqwest 发送错误到 `ProxyError` 的宿主适配已从 `forwarder` 迁入 `proxy::error_mapper::reqwest_send_error_to_proxy_error`；forwarder 只负责 transport 调用，不再内联错误分类规则。
494. Bedrock pre-send thinking/cache 优化串联规则已迁入 `proxy-core::apply_bedrock_pre_send_optimizers`，core 返回结构化报告，host forwarder 只负责 provider gate、clone 隔离与日志输出。
495. `ProxyBody` 到 JSON 请求体的解释规则已迁入 `proxy-core::ProxyBody::into_json`；host forward pipeline 不再维护本地 body parse helper。
496. Gemini tool-call ID 的随机 UUID 宿主适配已集中到 `proxy_core_adapter::synthesize_gemini_tool_call_id_with_uuid`；handlers 与 Claude provider 不再各自维护重复 wrapper，core 仍只保留确定性 suffix 生成规则。
497. channel provider override plan 的 settings JSON 应用规则已迁入 `proxy-core::apply_channel_provider_settings_overrides`；host route attempt 只负责把 core plan 应用到宿主 `Provider` 并保留 `ProviderMeta` 更新。
498. `ChannelQuery` 对 `ChannelSpec` 的 provider/model/group/status 过滤规则已迁入 `proxy-core::channel_matches_query`；adapter-owned channel source 不再维护本地筛选 helper。
499. `AppProxyConfig` 管理 API raw envelope 的 `currentProviderId` 注入规则已迁入 `proxy-core::app_proxy_config_raw`；adapter-owned config source 负责读取当前 provider 事实并传入 core helper。
500. `ProxyCoreEventType` 外部事件名和 `ProxyCoreEvent` payload 注入规则已迁入 `proxy-core::{event_name, into_event_payload}`；adapter-owned `CcSwitchEventSink` 负责把 core 事件转发给现有 `ProxyEventBus`，host services 只装配可选事件总线。
501. `/proxy/v1/channels/{channel_id}/test` 管理 API 已落地；adapter-owned reachability probe 负责读取 channel/provider 并执行现有 stream-check，core 负责 `ProxyChannelTestRequest`/`ChannelTestResponse` 对外契约。
502. channel test 的 requested model/interface 预检、模型可用性判定和探测结果 response 组装已迁入 `proxy-core::{plan_channel_test, ChannelTestContext, ChannelReachabilityResult}`；host handler 只保留 DB/StreamCheck 适配。
503. 非流式响应体解码后的日志 level 与消息投影已迁入 `proxy-core::ResponseBodyDecodeStatus::log_event`；host `response_processor` 只负责输出 core 返回的事件。
504. SSE passthrough 事件的 debug 日志消息投影已迁入 `proxy-core::SsePassthroughEvent::log_message`；host 流式透传只保留 usage 收集和 stream transport 编排。
505. `ProxyCoreResponse`/`ProxyResponseBody` 的 JSON body 序列化与 transport-ready body 规范化已迁入 core；host response adapter 只把 `Empty`/`Bytes`/`Stream` 接到具体 Axum/`ProxyResponse` transport。
506. response usage 缺失/非 JSON body 的诊断日志投影已迁入 `proxy-core::{StreamingResponseUsageRecord, NonStreamingResponseUsageRecord}`；host `response_processor` 只负责输出 core 返回的日志消息并调度 `UsageSink`。
507. channel test reachability 的外部状态字符串 contract 已迁入 `proxy-core::ChannelReachabilityStatus` 与 `ChannelReachabilityResult::from_input`；host handler 只把本地 `HealthStatus` 适配为 core 状态枚举。
508. `/proxy/v1/events` 的 SSE `id`/`event`/`data` 字段投影已迁入 `proxy-core::ProxyEventSseSpec`；host handler 只把 neutral spec 接到 Axum `Event` 并保留广播与 keepalive transport。
509. Claude/Codex 转换后流式 response 缺 usage 的诊断文案已迁入 `proxy-core::TransformedResponseUsageFormat::missing_streaming_usage_log_message`；host handler 只负责输出 core 文案并调度 transformed usage 落库。
510. 非流式上游 JSON parse 失败后未标记 SSE fallback 的诊断日志投影已迁入 `proxy-core::UpstreamJsonBodySource::unlabeled_sse_fallback_log_event`，host 日志 level/message 分发已收敛到 `proxy_core_adapter::log_unlabeled_sse_fallback_event`；Claude/Codex handler 只传入协议上下文。
511. Codex Chat 上游错误体归一化后的非 JSON body 诊断文案已迁入 `proxy-core::CodexChatErrorNormalization::non_json_body_log_message`，host warn 输出和 Responses 错误体 neutral response 构造已收敛到 `proxy_core_adapter::codex_chat_error_proxy_response`；handler 只负责读取 body 并桥接 Axum response。
512. channel test 的 `StreamCheckResult` 到 `ChannelReachabilityResult` 适配已迁入 `proxy_core_adapter::stream_check_result_to_channel_reachability`；host handler 只负责执行 DB 查询、StreamCheck 探测和输出 core response。
513. `/proxy/v1/events` 的 `ProxyEventEnvelope` 到 Axum `Event` transport 适配已迁入 `proxy::response_adapter::proxy_event_envelope_to_axum_sse_event`；host handler 只负责订阅事件流和 keepalive 编排。
514. `ProxyResult` 回填 host `RequestContext` 的 outbound model、selected provider hydration 和 Claude api_format fallback 已迁入 `RequestContext::{apply_proxy_result,claude_api_format_for_proxy_result}`；各协议 handler 只调用 context 方法。
515. Claude Desktop gateway 的宿主 token 读取、core bearer 校验和 `ProxyError` 映射已迁入 `proxy::auth_adapter::validate_claude_desktop_gateway_auth`；handler 只负责传入请求 headers。
516. forward error 的失败请求 usage record 构造与 `UsageSink` 异步调度已从 `proxy::usage_sink_bridge::record_forward_error_usage` 继续上移到 `proxy_core_adapter::record_forward_error_usage`；协议 handler 的 `ProxyCoreError -> ProxyError` 映射与 forward error usage 记录已继续收敛到 `proxy_core_adapter::record_forward_core_error_usage`。
517. Claude/Codex 转换响应的非流式 usage 落库调度与流式 `SseUsageCollector` 构造已从 `proxy::usage_sink_bridge` 继续上移到 `proxy_core_adapter`；handler 不再直接拼装 transformed usage record 或调用 `UsageSink`。
518. `/proxy/v1/groups` 的 channel source facts 到 `RouteGroupListResponse` 投影已迁入 `proxy-core::GroupListRequest::response_from_channel_sources`；host handler 只负责按 app 查询 route source 和 channel specs。
519. `/proxy/v1/apps/{app}/providers` 的 provider/current/failover/route-candidate facts 已聚合为 `proxy-core::ProviderListSource`；host handler 只负责读取 DB/router facts 并交给 core 生成 provider list response。
520. `/proxy/v1/apps/{app}/channels` 的 route-aware 分支计划与 list source 投影已迁入 `proxy-core::{AppChannelManagementPlan, AppChannelListSource}`；host handler 只负责执行 dry-run route resolve 或 channel list 查询。
521. `/proxy/v1/channels/{channel_id}/test` 的 probe provider/base URL 输入投影已迁入 `proxy-core::ChannelTestProbeRequest`；host handler 只负责按 probe request 查 provider 并执行 `StreamCheckService`。
522. `/proxy/v1/apps/{app}/routes/current` 的 active target/configured provider facts 已聚合为 `proxy-core::CurrentRouteSource`；host handler 只负责读取运行态 current target 和 DB configured provider。
523. `/proxy/v1/apps/{app}/channels/migration/{preview,materialize}` 的 DB result facts 已聚合为 `proxy-core::{ChannelMigrationPreviewSource, ChannelMigrationMaterializeSource}`；host handler 只负责执行 legacy migration DB 调用。
524. `CcSwitchForwarderAttemptRuntimeSource` 已删除私有 `should_bypass_circuit_breaker` / `attempt_limit_reached` 二次 helper；`allow` 直接调用 core/adapter 策略函数并返回结构化决策，避免默认 source 实现继续扩散内部投影层。
524. `/proxy/v1/channels` 的 app filter 分支计划与 channel list facts 已迁入 `proxy-core::{ChannelListPlan, ChannelListSource}`；host handler 只负责执行 per-app/all-channel DB 查询。
525. `/proxy/v1/apps` 的 app summary facts 已聚合为 `proxy-core::AppListSource`；host handler 只负责遍历 app 并读取 config/provider/channel facts。
526. `/proxy/v1/channels/{channel_id}` 与 `/proxy/v1/channels/{channel_id}/models` 的 record/models/delete facts 已聚合为 `proxy-core::{ChannelRecordSource, ChannelModelsSource, ChannelDeleteSource}`；host handler 只负责执行 channel/model DB 操作。
527. `POST /proxy/v1/channels` 的 created channel record fact 已聚合为 `proxy-core::ChannelCreateSource`；host handler 只负责执行 channel create DB 操作和 record adapter。
528. health/status 管理端点的 timestamp/runtime status facts 已聚合为 `proxy-core::{HealthCheckSource, ProxyStatusSource}`；host handler 只负责读取 clock 和 runtime state。
529. `POST /proxy/v1/route/resolve` 的 dry-run route response 已通过 `proxy-core::RouteResolveManagementRequest::response_from_resolution` 输出；host handler 只负责调用 provider router dry-run。
530. `POST /proxy/v1/channels/{channel_id}/breakers/reset` 的 breaker reset response 已聚合为 `proxy-core::ChannelHealthResetSource`；host handler 只负责调用 proxy engine reset。
531. Claude Desktop `ResolvedModelRoute` 到 `ClaudeDesktopModelListResponse` 的 host-to-core 投影已迁入 `proxy_core_adapter::claude_desktop_model_routes_to_core_response`；Claude Desktop config 模块只负责读取 provider route facts。
532. Codex settings/model catalog text 到 core catalog response 的 host-to-core 投影已迁入 `proxy_core_adapter::{codex_model_catalog_from_settings,simplify_codex_model_catalog}`；Codex config 模块只负责 TOML、模板和文件路径处理。
533. Claude takeover 的上游 `[1M]` suffix 到客户端 role model/display name 的 host-to-core 投影已迁入 `proxy_core_adapter::{claude_takeover_client_model_for_upstream,claude_takeover_default_display_name}`；`ProxyService` 只负责 live config 字段写入和 auth 占位策略。
534. provider settings/client raw catalog 到 `ModelCatalog` 的 host-to-core 投影已迁入 `proxy_core_adapter::{provider_model_catalog_from_settings,client_model_catalog_from_raw}`；adapter-owned model catalog provider 负责 DB/raw 文件读取，`proxy_core_host` 只装配 provider。
535. `RoutePlan` provider id 提取与 host `ForwardResult` selected route 回填的 host-to-core 投影已迁入 `proxy_core_adapter::{route_plan_provider_ids,route_selection_for_forward_result}`；`proxy_core_host` 只负责 host provider 查询和 response transport 适配。
536. core `UsageRecord` 到 host `RequestLog` 的 usage/cost/request-id/missing-pricing 投影已迁入 `proxy_core_adapter::{usage_record_pricing_model,usage_record_to_request_log}`；adapter-owned `CcSwitchUsageSink` 负责 pricing DB 查询、warning 输出和日志落库。
537. route group 与 inbound/outbound interface 兼容性 predicate 已迁入 `proxy_core_adapter::{route_group_matches,route_interfaces_compatible}`；runtime route planning 继续由 core route resolver 生成候选与排序。
538. `RouteSelection` 的 provider/channel/model route/inbound/outbound interface 字段装配已迁入 `proxy_core_adapter::route_selection_from_parts`；host route resolver 不再手写 core selection 结构体。
539. route attempt 的 `RouteSelection -> ChannelRouteCandidate -> ResolvedChannelAttempt` 投影、channel provider override 和 request body model override 已迁入 `proxy_core_adapter`；`route_attempt` 模块只负责 host `ForwardAttempt` 组装和 attempt 顺序。
540. Codex Responses handler 的 tool context 提取与 Chat error body normalization 已迁入 `proxy_core_adapter::{codex_tool_context_from_request,codex_chat_error_proxy_response}`；handler 只负责 Axum transport、`RequestContext` 和 transform 调度。
541. provider settings 与 Claude Desktop route 到 request body model mapping 的投影已收敛到 `proxy_core_adapter::apply_forward_request_model_mapping_from_provider`；`RequestForwarder` 只负责 transport 准备和可选 debug 日志输出。
542. Codex Responses -> Chat endpoint rewrite 与 Gemini Native URL build/resolve 投影已迁入 `proxy_core_adapter::{rewrite_codex_responses_endpoint_to_chat,resolve_gemini_native_url}`；forwarder 的 Codex app gate 与 Responses->Chat provider predicate 已收进 request source transform plan；`RequestForwarder` 保留 provider adapter URL fallback、full-url query 拼接和 transport 分支。
543. Claude API format 是否需要 transform 的 predicate 已迁入 `proxy_core_adapter::claude_api_format_needs_transform`；`RequestForwarder` 只负责 provider adapter fallback 和 transform 分支调度。
544. 上游请求 `anthropic-beta` 组装、ordered request headers 构建与 method-aware body serialization 已迁入 `proxy_core_adapter::{anthropic_beta_header_value,build_upstream_request_headers,serialize_upstream_request_body}`；`RequestForwarder` 只负责收集 upstream host、auth/session headers 和 managed-account 校验。
545. `ProxyCoreError::Unavailable` 分类已迁入 `proxy_core_adapter::proxy_core_error_is_unavailable`；`RequestForwarder` 只保留 materialized route 不可用时降级为空 attempts 的行为。
546. 上游请求 transport/streaming/SOCKS 发送策略已迁入 `proxy_core_adapter::{resolve_upstream_request_transport_policy,resolve_upstream_send_policy,is_socks_proxy_url}`；`RequestForwarder` 只保留 reqwest/raw-hyper 执行、proxy URL 读取与 response priming。
547. Codex 官方客户端 User-Agent 判定已迁入 `proxy_core_adapter::is_official_codex_client_user_agent`；Codex provider 测试不再直连 core policy helper。
548. Claude provider 的 API format transform predicate、OpenAI stream usage 注入、Anthropic tool-thinking history normalize 与 DeepSeek thinking-disabled effort 清理已迁入 `proxy_core_adapter`；Claude provider 只负责 provider settings、API format dispatch 和 transform 编排。
549. Copilot GitHub domain normalize、GHES 判定、默认 public domain 与复合 account id 策略已迁入 `proxy_core_adapter`；Copilot auth 模块只负责 token/OAuth 流程、账号存储与 endpoint/model 缓存。
550. Copilot OAuth/API URL 构造、Copilot API base fallback、模型列表响应解析与 `CopilotModel` 类型入口已迁入 `proxy_core_adapter`；Copilot auth 模块只负责 HTTP 调用、token/OAuth 状态和 endpoint/model 缓存。
551. Claude Desktop gateway bearer token 校验与 auth error 类型入口已迁入 `proxy_core_adapter`；host auth adapter 只负责读取/创建 gateway token 并映射为 `ProxyError`。
552. global proxy URL masking、显式代理 URL parse/scheme 校验、系统代理 env key 与 loopback 自环检测 helper 已迁入 `proxy_core_adapter`；global proxy command 和 host HTTP client 只负责 DB 状态、reqwest client 生命周期、reqwest proxy 应用和环境变量读取。
553. 模型拉取 command/service 边界使用的 `FetchedModel` DTO 入口已迁入 `proxy_core_adapter`；model fetch transport 继续只负责 reqwest 执行并复用 core request planning/response parsing ports。
554. host `ProxyError` 到 HTTP status 的 `ProxyErrorStatusKind` 与状态码解析入口已迁入 `proxy_core_adapter`；host error 模块只负责 `ProxyError` 枚举、Axum response body 和 host/core error bridge。
555. settings command/DAO 使用的 rectifier、optimizer 与 Copilot optimizer 配置 DTO 入口已迁入 `proxy_core_adapter`；host settings DAO 继续负责 settings key、JSON 持久化与 `AppError` 映射。
556. proxy management command/service/DAO 使用的 proxy config、runtime status、server info、takeover status、provider health 与 circuit breaker DTO 入口已迁入 `proxy_core_adapter`；host 继续负责代理服务生命周期、DB 行映射和运行时热更新。
557. proxy channel DAO 使用的 legacy channel projection、channel identity、request validation、normalization helper 与 channel write DTO 入口已迁入 `proxy_core_adapter`；host DAO 继续负责 SQLite row mapping、source kind 与 `AppError` 映射。
558. model fetch transport 使用的 OpenAI-compatible/Codex OAuth request plan、transport trait、HTTP response 与 core planning/response parsing wrapper 已迁入 `proxy_core_adapter`；host transport 继续只负责 shared reqwest client 执行与 response body 读取。
559. provider router/circuit breaker 使用的熔断 DTO、熔断 key helper、provider selection、route resolve 与 unavailable-channel filter 入口已迁入 `proxy_core_adapter`；host router 继续只负责 DB/provider facts、熔断器实例生命周期和健康状态写回。
560. proxy event bus/response SSE bridge 使用的 event envelope、connected/lagged event 常量、payload builder 与 SSE spec 投影入口已迁入 `proxy_core_adapter`；host event bus 继续只负责 broadcast runtime state 与 sequence 分配。
561. response adapter 使用的 core response、transport response 与 transport body DTO 入口已迁入 `proxy_core_adapter`；host response adapter 继续只负责映射到内部 `ProxyResponse` 与 Axum response/SSE event。
562. route attempt 使用的 route candidate、resolved channel attempt、route plan 与 route selection DTO 入口已迁入 `proxy_core_adapter`；host `ForwardAttempt` 继续只负责 provider override、provider-shaped attempt 编排和 model override log。
563. usage stats/logger/session usage 使用的 cost calculator、pricing、token usage、cost breakdown 与 session request id prefix 入口已迁入 `proxy_core_adapter`；host 继续负责 DB 查询、日志文件解析、dedup 与 request log 落库。
564. handler context 使用的 app config DTO、proxy result/services trait、response runtime policy DTO、Gemini path model 提取、Claude metadata API-format 提取与 session id 提取入口已迁入 `proxy_core_adapter`；host `RequestContext` 只保留 route result 后的 DB provider 回填与 response/usage 生命周期事实。
565. proxy server 使用的 runtime config/status/info、current route target、Gemini shadow store、ProxyEngine、route resolve request 与 server log code 入口已迁入 `proxy_core_adapter`；host server 继续负责 Axum/Hyper/Tauri 状态、监听生命周期和管理路由装配。
566. provider adapters 使用的 provider auth info 与 auth strategy DTO 入口已迁入 `proxy_core_adapter`；host provider adapters 继续负责从 desktop Provider 配置提取凭证、构建上游 URL 与 header。
567. provider kind、Claude provider kind inference 与 Gemini OAuth key shape detection 入口已迁入 `proxy_core_adapter`；host provider 模块继续负责把 desktop Provider 存储形态投影为 core provider kind。
568. Codex Chat history 使用的 SSE UTF-8 append/block split helper、SSE inspection DTO 与 history state DTO 入口已迁入 `proxy_core_adapter`；host provider 模块继续负责 tokio lock、stream wrapping 与跨请求 history store 生命周期。
569. Gemini provider 使用的 settings key/base-url 提取、OAuth credentials parse、upstream URL builder 与 auth header builder 入口已迁入 `proxy_core_adapter`；host Gemini adapter 继续负责 desktop Provider 配置读取与 `ProxyError` 映射。
570. Codex provider 使用的 upstream URL builder、Bearer auth header builder、Responses-to-Chat endpoint 判定、upstream model policy、catalog model IDs 与 reasoning profile/options 入口已迁入 `proxy_core_adapter`；host Codex adapter 继续负责 Provider/TOML 配置解析和 `ProxyError` 映射。
571. Claude provider 使用的 API-format resolution、auth-key/base-url 提取、upstream URL builder、static/Copilot auth header builder 与 prompt-cache key helper 入口已迁入 `proxy_core_adapter`；Claude 大请求/响应 transform 仍保留为下一组独立迁移切片。
572. Claude provider 使用的 Anthropic/OpenAI/Gemini request/response transform contract 入口已迁入 `proxy_core_adapter`；`src-tauri/src/proxy/providers` 已不再直接 import `proxy_core`，host provider adapters 继续负责 `Provider` 解析、runtime wrapping、logging 与 `ProxyError` 映射。
573. error mapper 使用的 ProxyCore error/result/response、forward failure kind、management auth error 与 Codex proxy error body/response helper 入口已迁入 `proxy_core_adapter`；host error mapper 继续负责 `ProxyError`/`ForwardError` 与 core error category 的双向桥接。
574. route attempt 测试使用的 provider/channel/route DTO alias 已迁入 `proxy_core_adapter`；`route_attempt.rs` 生产与测试路径均不再直接 import `proxy_core`，host 继续负责 provider cloning、AppType-specific override 与 ForwardAttempt 编排。
575. usage sink bridge 使用的 usage record helper、stream event filter、transformed usage format、provider usage facts、`UsageRecord` 与 `ProxyServices` 入口已迁入 `proxy_core_adapter`；host 继续负责 `RequestContext` 生命周期事实、usage logging 开关与异步落库调度。
576. response processor 使用的 response body decode、passthrough response builder、response header log summary、SSE scanner/usage accumulator、timeout phase、usage parser config、provider usage facts 与 streaming/non-streaming usage record helper 入口已迁入 `proxy_core_adapter`；host 继续负责 Axum response 转换、connection guard 生命周期与 `ProxyState` 调度。
577. Claude Desktop config 使用的 model-list response contract 与测试 proxy config 类型入口已迁入 `proxy_core_adapter`；host 继续负责桌面 profile 文件写入、route ID 管理与 provider model projection。
578. handlers 使用的管理 DTO、parser config、SSE transform helper、management auth decision、model catalog response、Codex tool context 与 channel/app response contract 入口已迁入 `proxy_core_adapter`；host handlers 继续负责 Axum extractor/response、数据库访问、provider routing 与 `ProxyError` 映射。
579. proxy core host service glue 使用的 core service traits、runtime config contract、health reset DTO、event/usage DTO、route policy/request、channel query 与测试 DTO 入口已迁入 `proxy_core_adapter`；host 继续负责 `Database`、`ProviderRouter`、`ProxyEventBus`、Tauri runtime 与 forward pipeline delegation。
580. forwarder 使用的 transport helper、optimizer/rectifier policy、retry classification、event payload builder、media fallback guard、upstream header/body policy、managed-account auth check 与 forwarder 测试 helper 入口已迁入 `proxy_core_adapter`；host forwarder 继续负责 provider selection、reqwest request execution、active connection accounting 与 retry orchestration。
581. `proxy-core` 已新增分组式 `api` 集成面（engine/ports/routing/transport/transforms/management/auth/model_catalog/events/usage/session/prelude），保留 legacy flat exports 兼容现有调用；下一步 host adapter 与外部集成应优先面向 `proxy_core::api` 固化公开契约。
582. `proxy_core_adapter` 的首批 engine/config/transport/usage/auth/routing/management/ports/domain alias 已改为经 `proxy_core::api` 解析，证明分组公开 API 可以承载 host 集成；legacy root exports 仍保留给未迁移入口兼容。
583. `proxy_core_adapter` 已继续把 model catalog、runtime ports、events、transforms、transport、routing、management 与 error contract 的类型别名和首批 service/port use-group 切到 `proxy_core::api`；剩余 root 入口主要集中在协议 helper 函数和少量内部兼容常量。
584. `proxy_core_adapter` 的协议 helper、management DTO、event helper、model catalog helper、transport policy 与测试 helper import 已改为经 `proxy_core::api` 分组导入；剩余 legacy root 依赖主要是 adapter wrapper 函数体中的直接 `crate::proxy_core::*` 调用。
585. `proxy_core_adapter` 的首批 wrapper 函数体已改为调用 `proxy_core::api`：覆盖 secret/global proxy helper、Copilot model catalog、model fetch transport、proxy event/SSE helper、Codex/Gemini/Claude provider helper 与 Anthropic/OpenAI/Gemini transform helper；剩余直接 root 调用集中在 route/channel、error mapper、model mapping、transport/response 和 usage wrapper 区段。
586. `proxy_core_adapter` 的 route/channel/circuit wrapper 区段已改为调用 `proxy_core::api`，覆盖 provider selection、route resolve、circuit key/config、legacy channel projection、channel id 和 channel write validation helper；剩余直接 root 调用继续集中在 model catalog、error mapper、transport/response、usage 与 session wrapper 区段。
587. `proxy_core_adapter` 的 model catalog、route selection、Codex error、channel override、transform guard 与 model mapping wrapper 已改为调用 `proxy_core::api`；`api::routing` 同步暴露 route selection helper，剩余直接 root 调用继续集中在 transport/response、usage、session、log code 与少量测试 helper 区段。
588. `proxy_core_adapter` 的 transport/response wrapper 区段已改为调用 `proxy_core::api`，覆盖 endpoint rewrite、Gemini URL、请求 header/body、upstream transport policy、response header/body、passthrough response、Codex UA 与 SSE test helper；剩余直接 root 调用继续集中在 usage、session、log code 与少量测试 helper 区段。
589. `proxy_core_adapter` 的 usage wrapper 区段已改为调用 `proxy_core::api`，覆盖 success/error usage record、transformed usage record、stream/non-stream usage record、pricing model/request-id fallback、token projection 与 Claude 1M suffix helper；剩余直接 root 调用继续集中在 session、log code 与少量测试 helper 区段。
590. `proxy_core_adapter` 的 session、log code 与测试 helper root 调用已全部改为经 `proxy_core::api` 分组解析；adapter 内已无非 `api::` 的直接 `crate::proxy_core::*` 调用，后续可以开始收缩 legacy flat exports 与继续稳定 host-only 端口边界。
591. `proxy-core` crate root 的 legacy flat `pub use *` 已移除，crate 内部改为从 owning module 显式 import；当前集成面以 `proxy_core::api` 分组为准，后续继续稳定 host-only port 边界并补 runtime smoke 验证。
592. runtime route planning 的 channel 过滤、模型匹配、候选排序和 attempt plan 构造已下沉到 `proxy-core::build_route_plan`，adapter-owned `CcSwitchRouteResolver` 只保留 `RouteRequest`/management dry-run 委托；host 只装配 resolver。
593. 上游 URL authority 到 Host header replacement 值的解析已下沉到 `proxy-core::upstream_host_header_from_url`；host forwarder 不再直接解析 `http::Uri`，只通过 adapter 调用 core helper 并继续把结果传入 header builder。
594. `ProxyErrorStatusKind` 到 `ProxyCoreError`/`ForwardFailureKind` 的分类规则和 forward failure message 选择策略已下沉到 `proxy-core`；host `error_mapper`/adapter 只负责把 `ProxyError` 投影为 kind/raw-message/display-message/body 事实，并继续保留 reqwest 与 Tauri-facing response 适配。
595. `/proxy/v1/channels` HTTP CRUD smoke 已扩展覆盖每个 channel 独立的 `authProfileRef`、base URL、interface、模型映射、weight、priority、health policy、header/param override、status mapping、tags 与 metadata；同一测试通过 `/proxy/v1/route/resolve` 验证候选 channel 继续携带独立 weight/priority 与模型映射。
596. `proxy-core` 已新增独立 boundary integration test，自动扫描 crate `Cargo.toml` 与 `src/`，防止重新引入 `tauri`、SQLite client 或 `crate::database/settings/services` 等宿主依赖；`cargo test --manifest-path src-tauri/crates/proxy-core/Cargo.toml --target-dir /private/tmp/cc-switch-proxy-core-target --offline` 已验证 core crate 可独立测试。
597. channel `paramOverrides` 已在最终上游 URL 构建后生效，同名 query 参数会被 channel 配置覆盖，scalar 值会做 query component encoding；channel `headerOverrides` 已接入上游请求 header 构建，并明确禁止覆盖 Host、认证 header 与 hop/tracing 类剥离 header。runtime `ForwardAttempt` 现在从完整 `RouteSelection` 保留 header/param overrides，管理 route candidate response 继续保持轻量且不暴露 override 内容。
598. `ProxyServer::start` 级 runtime smoke 已覆盖真实本机随机端口启动、`/proxy/v1/health` 与 `/proxy/v1/status` HTTP 请求和 `stop()` 关闭路径；该测试使用 no-proxy reqwest client，补齐仅构建 Axum router 之外的版本化管理 API 运行时验证。
599. 管理 API dry-run route 与 runtime `ProxyEngine::plan_materialized_route` 已新增同源排序测试：同一组 materialized channels 下，`proxy_core_adapter::management_route_response_from_router_source` 和 engine materialized plan 对 priority/weight 排序、模型匹配和首选 channel 选择保持一致，防止调试接口与真实转发路径漂移。
600. usage/request log 已保留 materialized channel 归因：core 新增 `UsageRouteContext` 与 route-context 投影 helper，host `RequestContext` 在 `ProxyEngine` 选路后保存 channel 上下文，普通透传、转换响应与错误 usage 均写入 `proxy_request_logs.channel_id/channel_name/route_group`；数据库 schema 已升级到 v13 并为旧 v12 明细表补列。
601. runtime resolved channel contract 已保留 `authProfileRef`：`ResolvedChannelAttempt` 从完整 `RouteSelection` 继承 channel auth profile，host `ForwardAttempt` 不再丢失每个中转地址独立认证 profile 的选择事实；candidate 兼容路径保持 `None`，后续可在 auth resolver 切片中把该 profile 解析为具体 header/key。
602. `provider:{app}:{providerId}` auth profile 已接入 host runtime：materialized channel 仍使用自身 provider/channel 的 base URL、interface、模型映射、熔断与健康统计，但认证提取和托管账号 token 选择会读取被引用 provider 的配置；跨 app 或缺失 provider 引用仍按兼容路径 fallback，不误覆盖认证来源。
603. `proxy_channel_keys` 已作为 schema v14 预留并接入 host DAO/runtime：channel 可通过 `authProfileRef = "channel-key:<keyRef>"` 选择同一 channel 下启用的 key，运行时会生成仅认证用 provider 副本并保持 route provider、base URL、模型映射、健康和 usage 归因不变；管理 channel record 仍不输出 `keyValue`，避免普通查询泄漏密钥。
604. `/proxy/v1/channels/{channel_id}/keys` 的 GET/PUT/PATCH/DELETE 管理接口已接入 `proxy-core::ChannelKeyRecord`、`ChannelKeysResponse`、`ChannelKeyDeleteResponse` 与 key path helper：外部集成方可以按 channel 独立创建、列出、轮换、禁用或移除 key，`keyValue` 仅作为写入字段进入 DAO/runtime，所有公开响应只返回 keyRef/status/priority/weight/lastFailureAt/deleted 等脱敏元数据。
605. 显式 `channel-key:<keyRef>` auth profile 已改为 fail-closed：如果 key 缺失或被禁用，runtime 返回认证错误而不是静默回退到 provider 原始凭据，避免“地址声明了独立 key 但实际走错 key”的中转隔离风险。
606. `authProfileRef` 的显式 profile 形状校验已迁入 `proxy-core::channel_request`，channel 写入和 patch 会在入库前拒绝空 `channel-key:`、空 provider ref 或未知 profile 前缀，避免运行期把 malformed channel key profile 回退成 provider 凭据。
607. host crate 新增 `proxy_core_boundary` 集成测试，固定除 `src/lib.rs` re-export 与 `src/proxy_core_adapter.rs` 外不得直接引用 `crate::proxy_core::` 或 `cc_switch_proxy_core::`，确保后续 Tauri host 继续通过 adapter 集中接入 core。
608. 模型目录服务层删除 `services::model_fetch` 与 `services::codex_oauth_models` 纯转发 facade，Tauri commands 直接调用 `model_fetch_transport` 这个 host reqwest adapter；URL 规划、请求契约、失败映射与响应解析继续由 `proxy-core::model_fetch` 维护。
609. stream check 的延迟状态判定与 timeout-like retry 判定已迁入 `proxy-core` 的 channel reachability contract；host `StreamCheckService` 保留 reqwest 探测、provider base URL 提取和现有 DTO/DAO 兼容映射。
610. `RoutePlan` 的 selection 迭代规则已收敛到 `proxy-core::route_plan_selections`：多候选使用 `selections`，空列表时回退 primary `selection`；host `route_attempt` 只负责把 core selection 映射为 `ForwardAttempt`。
611. Claude Desktop gateway 的 token 读取与 bearer 校验已继续下沉到 `proxy-core::ClaudeDesktopGatewayAuthSource` 与 `ProxyEngine::validate_claude_desktop_gateway_auth`；host `proxy::auth_adapter` 不再直接读取 DB 或调用 `claude_desktop_config`，只负责把 core auth error 映射到 HTTP 错误。
611. `ProxyServer::start` 级 runtime smoke 已扩展到 `/proxy/v1/route/resolve`：真实本机端口启动后创建 materialized channel，再通过 HTTP dry-run 验证 source、candidate channel id 与 upstream model，覆盖 router 之外的 runtime 管理 API 路径。
612. host/core 边界测试继续收紧 `proxy_core_adapter`：adapter 中直接访问 `crate::proxy_core::` 时必须走 `proxy_core::api` 分组集成面，防止后续迁移重新依赖 core 内部文件布局。
613. `ProxyServer::start` 级 runtime smoke 已继续扩展到 `/proxy/v1/groups?appType=claude`：真实本机端口创建 channel 后通过 HTTP 验证 group source、默认组、channelCount 与 appTypes 聚合，覆盖 route group 对外查询接口不只停留在 router unit test。
614. `ProxyServer::start` 级 runtime smoke 已覆盖 `/proxy/v1/apps` 与 `/proxy/v1/apps/{app}/providers`：真实本机端口验证 app providerCount、provider current/routeCandidate 标记与 settings/key 脱敏，确保管理 API 的 provider 汇总外部 contract 经过 runtime listener 固化。
615. `ProxyServer::start` 级 runtime smoke 已覆盖 `/proxy/v1/apps/{app}/channels` 的列表与 route-aware 过滤分支：真实本机端口验证 materialized channel source、channel id、模型映射、interface/routeGroup 回显与 rejected 空列表，固定 app 维度 channel 查询接口。
616. `ProxyServer::start` 级 runtime smoke 已覆盖 `/proxy/v1/apps/{app}/models`：真实本机端口基于 materialized channel 验证 routeGroup/interfaceKind 回显、public/upstream 模型映射与 channelName，固定外部集成方读取可路由模型目录的 HTTP contract。
617. `ProxyServer::start` 级 runtime smoke 已覆盖 `/proxy/v1/apps/{app}/routes/current`：真实本机端口分别验证 configured-provider-only 与 active channel target 两种 envelope，并继续确认 provider secret 不会从 current route 查询泄漏。
618. `ProxyServer::start` 级 runtime smoke 已覆盖 `/proxy/v1/apps/{app}/channels/migration/preview` 与 `/materialize`：真实本机端口验证旧 provider 投影、密钥不泄漏、物化计数、物化后 channel 查询可见，以及二次 materialize 插入计数为 0 的幂等行为。
619. 熔断器的 Open 超时恢复判定与失败后是否打开的纯策略已迁入 `proxy-core::circuit_breaker_config`；host `proxy::circuit_breaker` 继续保留 Tokio/Atomic/Instant 运行态和日志，但不再手写失败阈值、错误率和 HalfOpen 失败开闸规则。
620. 熔断器 HalfOpen 成功恢复阈值与探测名额放行规则已继续迁入 `proxy-core::circuit_breaker_config`；host 仍负责原子计数增减和 permit 回退，但成功恢复与 permit 结果由 core 策略函数决定。
621. `ProxyServer::start` 级 runtime smoke 已覆盖 `/proxy/v1/channels/{channel_id}/breakers/reset`：真实本机端口在物化 channel 后制造熔断、验证 dry-run route 被拦截，再通过 HTTP reset 恢复 route candidate。
622. `ProviderSpec` 的 metadata/accountRef 投影规则已迁入 `proxy-core::domain`：host adapter 只采集 `Provider/ProviderMeta` 的非密钥事实并传给 core DTO，provider summary/current route 等对外响应继续由 core 负责脱敏字段选择。
623. host adapter 的 channel/model JSON 默认化已复用 `proxy-core::channel_request` 的 object/array 默认规则，删除本地重复 helper，确保 `ChannelSpec`、`ModelRoute` 与 channel 写请求使用同一 JSON 形状收敛策略。
624. `ProxyServer::start` 级 runtime smoke 已覆盖 `/proxy/v1/apps/{app}/channels` 的 route filter rejected 分支：真实本机端口验证 missing model 查询返回空 channels、保留 rejected channel/reason，并继续回显 materialized channel source 与 route group。
625. provider settings 到请求 body 的模型映射聚合已迁入 `proxy-core::model_mapping`：core 统一负责 settings 解析、body model 替换和 `[ModelMapper]` 日志消息生成，host adapter 只保留薄包装调用。
626. Claude takeover 的客户端 1M 模型标记和默认显示名策略已迁入 `proxy-core::model_mapping`：core 统一处理上游 `[1m]` 后缀检测、客户端 `[1M]` 附加和显示名后缀剥离，host adapter 不再持有本地 marker 常量。
627. `ProxyServer::start` 级 runtime smoke 已扩展 `/proxy/v1/groups?appType=claude` 多组聚合：真实本机端口创建 default/beta 两个 materialized channel，并验证 group channelCount、appTypes 与 source 去重。
628. `ChannelSpec/ModelRoute` 的 record fact 到 core spec 构造规则已迁入 `proxy-core::domain`：host adapter 只组装中立 input DTO，core 统一负责 app/status/interface 解析、auth profile 包装和 JSON object/array 默认化。
629. 管理 API `ChannelRecord/ChannelModelRecord` 的公开 response 投影已迁入 `proxy-core::ports`：host adapter 只传入 storage fact input，core 负责生成外部 JSON contract 使用的 record DTO。
630. 管理 API `ChannelKeyRecord` 的公开 response 投影已迁入 `proxy-core::ports`：host adapter 不暴露 `key_value`，只把 channel/key/status/weight/failure fact 传给 core 生成对外 key DTO。
631. route resolve 的 channel/model record fact 到 `RouteResolveChannelInput` 投影已迁入 `proxy-core::route_resolve`：host adapter 只传入中立 record input，core 统一生成路由候选输入结构。
632. `/proxy/v1/groups` 的 channel groups fact 投影已迁入 `proxy-core::management_api`：host adapter 不再为了 group 聚合构造完整 `ChannelSpec`，只传入每个 channel 的 groups fact。
633. `/proxy/v1/apps` 的 app summary 投影已收窄为 provider/channel count fact：host adapter 不再为了 `providerCount/channelCount` 构造完整 `ProviderSpec` 与 `ChannelSpec`。
634. `/proxy/v1/apps/{app}/providers` 的 provider list 曾收窄为 `ProviderSummaryInput` 展示字段；后续已在 604 升级为经 `ProviderSpec` 投影，统一复用 core metadata 规则。
635. `/proxy/v1/apps/{app}/routes/current` 的 configured provider summary 曾收窄为 `CurrentRouteProviderSummaryInput::new(id,name,category)`；后续已在 605 升级为经 `ProviderSpec` 投影，统一复用 core metadata 规则。
636. `ProxyServer::start` 级 runtime smoke 已补充 provider list/current route 的 category 与 settings 脱敏断言：真实本机端口确认摘要接口只暴露展示 fact，不回传 provider settings/secret。
637. `authProfileRef` 的 `provider:<app>:<providerId>` / `channel-key:<keyRef>` 格式解析已迁入 `proxy-core::domain`：channel 写请求校验与 host auth-profile 应用共用同一 parser，host 只负责 DB/provider/key 查找和密钥注入。
638. legacy channel migration preview/materialize 的 response source 已可由 `ChannelMigrationPreviewInput` / `ChannelMigrationMaterializeInput` 构造：handler 不再逐字段拼 source，只把 DB 结果投影为 core input。
639. channel reachability 的健康状态枚举已复用 `proxy-core::ChannelReachabilityStatus`：stream check service 不再定义宿主侧重复 `HealthStatus`，adapter 也不再做状态枚举转换。
640. stream check 的配置 DTO/默认值/camelCase 序列化契约已迁入 `proxy-core::StreamCheckConfig`：host 只 re-export 同名配置并继续负责 reqwest 探测与 provider 覆盖合并。
641. stream check 的结果 DTO/历史字段兼容契约已迁入 `proxy-core::StreamCheckResult`：host service 不再定义重复 response struct，commands/DAO/handler 通过同名 re-export 保持现有调用路径。
642. stream check result 到 channel reachability result 的投影已迁入 `proxy-core::channel_reachability_result_from_stream_check_result`：adapter 只保留边界包装，不再逐字段手写转换。
643. OpenCode/OpenClaw/Hermes 的 stream-check base URL 解析策略已迁入 `proxy-core::domain`：core 统一识别 `options.baseURL`、`baseUrl`、`base_url` 与 OpenCode npm 默认端点，并维护缺失 base URL 的本地化错误规格；host 只负责 app 分支和错误类型包装。
644. stream check 日志 status 落库值已改为复用 `ChannelReachabilityStatus::as_str()`：DAO 不再通过 enum debug 字符串手写 lower-case contract。
645. `AppType -> AppKind` 转换已委托给 `proxy-core::AppKind::from(&str)`：host adapter 不再重复维护 Claude/Codex/Gemini 与累加模式应用的 app-kind match。
646. channel health 的 success/failure 状态推进规则已迁入 `proxy-core::channel_health_update_from_input`：DAO 只读取当前失败数并写入 core 计算出的 healthy/degraded/unhealthy、时间戳和 disabled reason。
647. channel health 的初始/缺省 unknown 状态已收敛到 `proxy-core::CHANNEL_HEALTH_UNKNOWN_STATUS`：DAO 创建 health row 与缺省读取不再硬编码字符串。
648. channel key 写入路径已改为传递 `ProxyChannelKeyWriteRequest` 整体请求并复用 `proxy-core::validate_proxy_channel_key_write_request_fields`：handler 不再把 keyValue/status/priority/weight 拆成 host 散参数。
649. channel model replace 路径已改为传递 `ProxyChannelModelsReplaceRequest` 整体请求并新增 `proxy-core::validate_proxy_channel_models_replace_request_fields`：handler 不再提前拆出 models Vec，DAO 只在落库前调用 core 请求校验。
650. channel patch 请求字段校验已新增 `proxy-core::validate_proxy_channel_patch_request_fields` 并接入 DAO update 路径：缺失 channel 仍返回 not-found，存在 channel 的 name/status/baseUrl/interfaceKind/authProfileRef 校验统一由 core 执行。
651. channel patch 请求字段归一化已新增 `proxy-core::normalize_proxy_channel_patch_request_fields`：DAO update 只记录 authProfileRef 字段是否出现以保留清空语义，其余 trim、baseUrl 去尾斜杠、groups 去重排序和 JSON object/array 容器默认均由 core 返回的 normalized patch 提供。
652. channel create/model replace 写请求归一化已新增 `proxy-core::normalize_proxy_channel_write_request_fields`、`normalize_proxy_channel_model_write_request_fields` 与 `normalize_proxy_channel_models_replace_request_fields`：DAO create/replace 不再手写 name/status/model trim、baseUrl 去尾斜杠、groups 默认或 JSON 容器默认，只保留 AppType/provider 校验与 SQLite 持久化。
653. channel key write/patch 请求归一化已新增 `proxy-core::normalize_proxy_channel_key_write_request_fields` 与 `normalize_proxy_channel_key_patch_request_fields`：DAO key upsert/update 不再手写 keyValue/status trim，只保留 keyRef path 校验、channel/key 存在性和 SQLite 写入。
654. provider health success/failure 状态推进规则已新增 `proxy-core::provider_health_update_from_input`：provider DAO 只读取当前失败数并写入 core 计算出的健康布尔值、失败计数、时间字段和 last_error，保持 SQLite 查询与 UPSERT 仍属于 host adapter。
655. usage 计费配置校验已新增 `proxy-core::validate_cost_multiplier_value` 与 `normalize_pricing_source`：DAO 和 provider service 继续使用原 host wrapper 与本地化错误映射，但倍率解析、负数拒绝、计费来源 trim/白名单规则由 core 统一提供。
656. app 级 proxy_config 默认值策略已新增 `proxy-core::app_proxy_config_defaults_for_app`：DAO 的缺省读取、单行 ensure 和三行 init 不再手写 claude/codex/gemini 的重试、超时和熔断 seed 值，只保留 SQL upsert/insert。
657. proxy takeover/hot-switch 期间阻断 official provider 的业务规则已新增 `proxy-core::should_block_proxy_switch_to_provider_category`：Tauri command 与 provider service 不再各自手写 `category == "official"` 判定，host 只保留查询 provider、执行切换和错误文案。
658. global proxy_config 缺省值已收敛到 `proxy-core::GlobalProxyConfig::default()`：DAO 在全局配置行缺失时不再手写 listen address、port、logging 与 enabled 默认值，只负责初始化行和返回 core 默认 DTO。
659. 熔断恢复后的 failover switchback 判定已新增 `proxy-core::restored_provider_switchback_decision`：`reset_circuit_breaker` 仍负责健康状态重置、队列读取和实际切换，但队列 sort_index 提取以及“接管中 + 自动故障转移 + 服务运行 + 恢复 provider 优先级更高”的业务规则由 core 统一判定。
660. channel route 的 resolved attempt model override 已新增 `proxy-core::apply_resolved_channel_model_override` 与 `ChannelRouteModelOverride`：host `route_attempt` 不再读取 body/public/upstream model 组合，只把 core 返回的 channel/model 变更上下文写入日志。
661. 响应 SSE content-type 识别已新增 `proxy-core::response_headers_indicate_sse`：host `ProxyResponse::is_sse` 不再手写 `text/event-stream` 字符串判定，只负责把响应 headers 传入 core helper。
662. `ProxyServer::start` 级 runtime smoke 已覆盖 `/proxy/v1/channels/{channel_id}/test`：真实本机端口创建 materialized channel 后，通过本机临时 upstream 验证 channel test 的 modelAvailable、reachability success、HTTP status、retryCount 和无 failureReason 的外部 JSON contract。
663. proxy 默认 listen address/port 已新增 `proxy-core::{DEFAULT_PROXY_LISTEN_ADDRESS, DEFAULT_PROXY_LISTEN_PORT}`：`ProxyConfig`、`GlobalProxyConfig` 和 host 全局 HTTP client 的递归代理检测 fallback 不再各自硬编码 `127.0.0.1:15721`。
664. provider list 管理 API 已改为经 `ProviderSpec` 投影并复用 `ProviderListSource::from_provider_specs`：host 删除 provider summary input 专用 facade，只负责把 CC Switch provider 转为 core provider spec。
665. current route 管理 API 的 configured provider summary 已改为经 `CurrentRouteProviderSummaryInput::from_provider_spec`：host 删除 current-route provider summary 专用 facade，配置 provider 的 category 继续来自 core metadata 投影。
666. legacy channel migration 的 provider settings 投影已从 DAO 移到 host adapter `legacy_provider_projection_input`：DAO 不再解析 Codex TOML、env/modelCatalog 或 Claude Desktop model routes，只把 provider fact 投影交给 adapter 后调用 core legacy projection。
667. legacy channel projection 到 `ProxyChannelRecord` / `ProxyChannelModelRecord` 的 host 映射已移入 `proxy_core_adapter::proxy_channel_record_from_legacy_projection`：DAO 不再逐字段展开 core legacy projection，也不再手写 preview 去重，只负责读取 legacy provider 事实和物化落库。
668. forward result 到 `ProxyResult` 的 selected route、metadata 和 outbound model 投影已移入 host adapter `proxy_result_from_forward_parts`：`proxy_core_host` 只保留 `ProxyResponse`/connection guard 到 core response 的 runtime 桥接，结果 contract 由 adapter 统一生成。
669. `CcSwitchChannelSource` 的 channel record 到 `ChannelSpec` 投影与 `ChannelQuery` 过滤已收敛到 host adapter `proxy_channel_records_to_core_specs_for_query`：adapter-owned source 负责选择 legacy/materialized 数据来源、读取 DB/router 并映射 host 错误，`proxy_core_host` 只装配 source。
670. `CcSwitchProviderSource` 的 DB provider 到 `ProviderSpec` 投影已收敛到 host adapter `proxy_provider_to_core_spec` / `proxy_providers_to_core_specs`：adapter-owned source 负责 provider list/get/current 查询与 active-route runtime map 读取，metadata 脱敏和 provider kind 推断继续由 adapter/core 投影规则生成。
671. route policy 的 failover queue provider id 到 `RoutePolicy` raw contract 投影已新增 `proxy-core::route_policy_from_failover_provider_ids` 并经 host adapter `route_policy_from_failover_queue` 接入：adapter-owned `CcSwitchRoutePolicySource` 负责读取 DB 队列和错误映射，host 只装配 source。
672. `CcSwitchConfigSource` 的 global/app/runtime config DTO 投影已新增 `proxy-core` helpers 并由 adapter-owned source wrapper 接入：config source 负责读取 DB/settings，`ProxyGlobalConfig`、`ProxyAppConfig`、optimizer specs 与 `ProxyRuntimeConfig` 的 raw contract 由 core 统一生成。
673. `CcSwitchAuthProvider` 的 `AuthProfileRef` 到 `AuthInfo` 默认投影已新增 `proxy-core::auth_info_from_profile_ref` 并由 adapter-owned auth provider 包装：`proxy_core_host` 不再保留 auth provider 实现，也不再手写 headers/accountRef/source metadata envelope。
674. channel health reset fact 已新增 `proxy-core::channel_health_reset_from_parts` 并经 host adapter 接入：`CcSwitchChannelHealthStore` 在 reset 后只传 channel_id/app_type，`ChannelHealthReset` 的 app-kind 投影由 core 统一生成。
675. client model catalog 的空目录默认 raw contract 已新增 `proxy-core::client_model_catalog_from_optional_raw` 并经 host adapter 接入：adapter-owned `CcSwitchModelCatalogProvider` 负责读取 Codex 本地 catalog raw，非 Codex 默认空模型列表由 core 统一生成。
676. channel health attempt 的默认 failure threshold 已收敛到 `proxy-core::DEFAULT_CHANNEL_HEALTH_FAILURE_THRESHOLD` 并经 host adapter re-export：`CcSwitchChannelHealthStore` 不再维护独立阈值常量，只负责把 attempt fact 写入 DB。
677. usage request log 字段、成本计算和缺失定价告警文案已新增 `proxy-core::usage_request_log_projection` 并经 host adapter 映射到 `RequestLog`：adapter-owned `CcSwitchUsageSink` 负责读取计费配置/定价、提供 request_id fallback、执行 host 落库和输出 core 生成的 warning。
678. channel authProfileRef 的 provider/channel-key resolution 和缺失 provider warning 文案已新增 `proxy-core::channel_auth_profile_resolution` / `channel_auth_profile_missing_provider_warning` 并经 host adapter 接入：`apply_channel_auth_profile_providers` 不再解析 auth profile 字符串，只保留 provider lookup、channel key DB 读取和 auth provider materialization。
679. route plan provider id 与 host configured provider ids 的匹配结果已新增 `proxy-core::route_plan_provider_match` 并经 host adapter 接入：`host_providers_for_plan` 不再手写去重 provider id 的匹配/空匹配判定，只负责按 core 返回的 matched ids clone host provider。
680. forward runtime 的 timeout/retry 投影已统一经 host adapter `response_runtime_policy_from_app_proxy_config` 复用 `proxy-core::resolve_response_runtime_policy`：`CcSwitchProxyRuntime::forward` 和 `ProxyHandlerContext` 不再各自展开 auto_failover_enabled、max_retries 与三类 timeout 字段。
681. channel-key auth provider 的 settings JSON 注入策略已新增 `proxy-core::settings_config_with_channel_auth_key` 并经 host adapter 接入：host 不再按 AppType/ClaudeAuthKeySource 手写 env/apiKey patch，只负责 clone provider 并写入 core 返回的 settings_config。
682. channel-key auth profile 缺失/禁用 key 的错误文案已新增 `proxy-core::channel_auth_profile_missing_key_error_message` 并经 host adapter 接入：host 只负责把 core 文案包装为 `ProxyCoreError::Auth`，不再维护独立 runtime 文案。
683. forward runtime 当前 provider 来源优先级已新增 `proxy-core::current_provider_id_from_sources` 并经 host adapter 接入：host 只负责读取 settings 与 DB 两个来源，settings 优先、DB 兜底、缺失时空串的选择策略由 core 统一锁定。
684. forward pipeline 缺 runtime、route plan 无匹配 host provider、route plan provider 未配置三类固定错误文案已新增 core helpers 并经 host adapter 接入：host 只负责选择 `ProxyCoreError` variant，不再维护这些 runtime 文案常量。
685. unsupported app kind 的 parse 错误文案已新增 `proxy-core::unsupported_app_kind_error_message` 并经 host adapter 接入：host `parse_app_type` 只负责 `AppKind -> AppType` 转换和 `ProxyCoreError::Config` 包装。
686. host adapter 的 context/error 拼接格式已新增 `proxy-core::error_message_with_context` 并经 adapter 接入：`proxy_core_host` 的 `app_error` / `usage_error` 只负责选择 `Config` 或 `Internal` variant，不再维护 `{context}: {error}` 文案格式。
687. channel authProfileRef 缺失 provider warning 的 optional fallback 已收敛到 `proxy-core::channel_auth_profile_missing_provider_warning`：host 不再对 `auth_profile_ref` 手写 `unwrap_or_default()`，只传递原始 optional ref。
688. current provider 来源优先级的 `Option<String>` 形态已新增 `proxy-core::current_provider_id_option_from_sources`：adapter-owned `CcSwitchConfigSource::load_app` 负责读取 settings 当前 provider 事实，`ProxyAppConfig.raw.currentProviderId` 的 option contract 由 core helper 统一生成；forward runtime 的 string helper 继续复用同一来源选择规则。
689. Codex client model catalog raw JSON 解析和空 models fallback 已新增 `proxy-core::client_model_catalog_raw_from_text` / `empty_client_model_catalog_raw`：host 只负责解析 Codex 配置路径与读取文件文本，raw catalog contract 由 core 统一生成。
690. legacy provider settings JSON 的 config/env/modelCatalog 形状投影已新增 `proxy-core::legacy_provider_config_text_from_settings`、`legacy_provider_env_from_settings` 与 `legacy_provider_codex_catalog_models_from_settings`：host adapter 不再手写 env string-map 和 Codex catalog model id 展开，只负责把 Provider/meta 事实拼入 legacy projection input。
691. host `ProxyResponse` 到 `ProxyCoreResponse` 的 buffered/streamed bridge 已移入 `proxy_core_adapter::proxy_response_to_core_response`：`proxy_core_host` 不再维护响应体/stream guard 转换细节，只负责把 forward result 交给 adapter 投影。
692. route plan 到 host Provider 列表的匹配过滤已移入 `proxy_core_adapter::host_providers_for_plan`：`proxy_core_host` 不再展开 `RoutePlanProviderMatch`，只负责读取 DB provider map 并接收 adapter 返回的执行 provider 集合。
693. host `ForwardResult` 到 `ProxyResult` 的 selected route、metadata、outbound model 和 response bridge 组合已移入 `proxy_core_adapter::forward_result_to_proxy_result`：`proxy_core_host` 不再拆 host forward result，只把 runtime 结果交给 adapter。
694. `AppKind` 到 host `AppType` 的 required/optional 解析桥接已新增 `proxy_core_adapter::app_type_from_proxy_core_app` 与 `app_type_option_from_proxy_core_app`：`proxy_core_host` 不再直接调用 `AppType::from_str` 或包装 unsupported app kind 错误。
695. channel-key auth profile 的 missing-key 错误包装与 provider settings patch 已移入 `proxy_core_adapter::channel_key_auth_error` / `provider_with_channel_auth_key`：`proxy_core_host` 只保留 channel key DB 查询和 attempts 变更，不再 clone provider 或注入 auth key settings。
696. host `AppError` 到 `ProxyCoreError::Config/Internal` 的上下文包装已移入 `proxy_core_adapter::app_error` / `usage_error`：`proxy_core_host` 不再直接调用 `error_message_with_context` 或选择 core error variant。
697. current-provider 的 DB fallback 查询判定已新增 `proxy-core::current_provider_db_fallback_required` 并经 adapter 接入：forward runtime 不再手写 `settings_current_provider_id.is_none()`，空 settings 值仍按已存在来源处理而不回退 DB。
698. forward pipeline 缺少 runtime 时的 Unsupported 错误包装已移入 `proxy_core_adapter::forwarding_runtime_unavailable_error`：`proxy_core_host` 不再直接选择 `ProxyCoreError::Unsupported` 或复制 runtime-required 文案。
699. channel authProfileRef 到 attempt 处理动作的判定已进一步从 adapter 移入 `proxy-core::channel_auth_profile_action`；本轮又把 provider 存在性检查、缺失 provider warning 分支和 channel-key 应用计划收敛到 `proxy-core::channel_auth_profile_provider_application`：`proxy_core_adapter` 只 re-export/消费 core plan，host runtime 继续只保留 provider/key 查询与 attempt 写入。
700. route plan 无匹配 host provider attempts 时的 Unavailable 错误包装已移入 `proxy_core_adapter::route_plan_no_matching_host_providers_error`：forward runtime 不再直接选择 `ProxyCoreError::Unavailable` 或复制固定错误文案。
701. provider config auth profile 的 metadata source label 已移入 `proxy_core_adapter::auth_info_from_cc_switch_provider_config`：`proxy_core_host` 不再硬编码 `cc_switch_provider_config` 字符串。
702. Codex client model catalog 的 active config 路径解析、文件读取与 stale guard fallback 已移入 `proxy_core_adapter::codex_client_model_catalog_raw_from_active_config`：`proxy_core_host` 的 `ModelCatalogProvider` 只保留 app 分派和 catalog envelope 组装。
703. host Provider 到 provider model catalog 的 settings 投影已移入 `proxy_core_adapter::provider_model_catalog_from_provider`：`proxy_core_host` 不再直接读取 `Provider.settings_config` 构造 catalog。
704. app config source 的当前 provider settings 读取与 `ProxyAppConfig` parts 组装已移入 `proxy_core_adapter::current_provider_id_from_settings_for_app` / `proxy_app_config_from_config_source_parts`：adapter-owned `CcSwitchConfigSource` 保留 DB 配置和优化器配置读取，`proxy_core_host` 只装配 source。
705. runtime config source 的 host 默认 privacy-filter flag 包装已移入 `proxy_core_adapter::proxy_runtime_config_from_config_source`：`proxy_core_host` 不再直接传入固定 `false` 构造 runtime config。
706. ProviderSource 的列表/单条 provider 到 `ProviderSpec` 投影已移入 `proxy_core_adapter::provider_specs_from_source` / `provider_spec_from_source`：`proxy_core_host` 不再直接解析 app kind 或调用 provider spec conversion。
707. ChannelSource 的列表/单条 channel 到 `ChannelSpec` 投影已移入 `proxy_core_adapter::channel_specs_from_source` / `channel_spec_from_source`：`proxy_core_host` 不再直接调用 channel record conversion。
708. ChannelHealthStore 的 attempt 写库参数投影已移入 `proxy_core_adapter::channel_health_attempt_db_update`，adapter-owned `CcSwitchChannelHealthStore` 负责调用 DB 写入；`proxy_core_host` 只装配该 store。
709. `ProxyCoreEvent` 到 host event bus name/payload 的投影已移入 `proxy_core_adapter::proxy_core_event_to_bus_message`：`proxy_core_host` 的 event sink 不再直接调用 `event_name()` 或 `into_event_payload()`。
710. RoutePolicySource 的 failover queue 到 optional `RoutePolicy` source 投影、DB queue 查询与错误映射已由 adapter-owned `CcSwitchRoutePolicySource` 包装；`proxy_core_host` 不再保留该桥接实现。
711. ChannelHealthStore reset 的 app lookup 结果校验、router reset 调用与 reset fact 投影已由 adapter-owned `CcSwitchChannelHealthStore` 包装；`proxy_core_host` 不再保留该桥接实现。
712. client model catalog 的 app 分派、Codex active catalog raw 读取与非 Codex 空目录默认值已移入 `proxy_core_adapter::client_model_catalog_from_source`：`proxy_core_host` 的 `ModelCatalogProvider` 不再维护客户端 catalog source 分支。
713. UsageSink 的 provider/app 计费配置 lookup 输入已移入 `proxy_core_adapter::usage_pricing_config_lookup_from_record`：adapter-owned `CcSwitchUsageSink` 负责调用 usage logger 读取配置、定价和落库，`proxy_core_host` 只装配 sink。
714. `ProxyCoreEvent` 的 event bus 投影与分发闭包入口已移入 `proxy_core_adapter::emit_proxy_core_event`：`proxy_core_host` 的 event sink 只提供实际 bus emit 副作用。
715. forward runtime 的 `RoutePlan` + host providers 到 `ForwardAttempt` 列表构造入口已移入 `proxy_core_adapter::forward_attempts_from_plan`：`proxy_core_host` 不再直接依赖 `proxy::route_attempt::forward_attempts_from_route_plan`。
716. forward runtime 的 auth profile provider/channel-key 应用循环已移入 `proxy_core_adapter::apply_channel_auth_profile_providers_from_source`：`proxy_core_host` 只提供 channel key DB 读取闭包。
717. forward runtime 的 current-provider settings/DB fallback 组合已移入 `proxy_core_adapter::forward_current_provider_id_from_source` / `current_provider_id_from_settings_for_app_type`：`proxy_core_host` 只提供 DB fallback 读取闭包。
718. forward runtime 的 required attempts 空结果错误判断已移入 `proxy_core_adapter::required_forward_attempts_from_plan`：`proxy_core_host` 不再直接选择 `route_plan_no_matching_host_providers_error`。
719. `ProxyRequest` 到 host forward runtime 输入的 app/body/method/header/session 投影已移入 `proxy_core_adapter::forward_runtime_request_from_proxy_request`：`proxy_core_host` 不再直接拆 `ProxyRequest`、调用 `ProxyBody::into_json` 或抽取 session id。
720. `AppProxyConfig` 到 `RequestForwarder` timeout/retry 选项的投影已移入 `proxy_core_adapter::forwarder_runtime_options_from_app_proxy_config`：`proxy_core_host` 不再直接展开 `ResponseRuntimePolicy.timeout`。
721. forward runtime 的 `AppProxyConfig`、rectifier、optimizer 与 Copilot optimizer 组合已收敛为 `proxy_core_adapter::forwarder_runtime_config_from_sources`：`proxy_core_host` 仍负责读取 DB 配置，但不再把 forwarder runtime config 作为散落局部变量维护。
722. ConfigSource 的 app 配置投影已新增 `proxy_core_adapter::proxy_app_config_from_config_source` wrapper，app summary 投影已新增 `app_summary_config_from_config_source` wrapper：`proxy_core_host` 只传入 DB 读取到的 app/optimizer 配置，settings current-provider 读取、`ProxyAppConfig` 组合与 `AppSummaryConfig` 组装由 adapter 统一处理。
723. channel-key auth profile 的 DB key record 到 runtime key value 投影已移入 `proxy_core_adapter::channel_key_value_from_record`：host auth-profile closure 只负责查询 enabled key 和错误映射，不再直接拆 DAO record 字段。
724. management handlers 的 provider summary/current route provider 投影已改为调用 `proxy_core_adapter::proxy_provider_to_core_spec` / `proxy_providers_to_core_specs`：handler 不再直接依赖 provider projection trait，只保留 HTTP path、DB 查询和 response envelope 调用。
725. channel/group 管理列表的 response source 组装已新增 `proxy_core_adapter::channel_list_source_from_records`、`app_channel_list_source_from_records` 与 `group_list_channel_source_from_records`：handler 仍负责 DB/router 查询，但不再手写 channel record 到 list/group source 的组合。
726. channel create/get/update 管理路由的 record response source 组装已新增 `proxy_core_adapter::channel_create_source_from_record` 与 `channel_record_source_from_record`：handler 不再直接调用 `ChannelCreateSource`/`ChannelRecordSource` 或手写 `ProxyChannelRecord -> ChannelRecord` 映射。
727. channel keys/models 管理路由的 source 组装已新增 `proxy_core_adapter::channel_keys_source_from_records`、`channel_key_record_source_from_record` 与 `channel_models_source_from_records`：handler 不再直接调用 key/model source DTO 或手写 key/model record 映射。
728. channel migration preview/materialize 管理路由的 source 组装已新增 `proxy_core_adapter::channel_migration_preview_source_from_result` 与 `channel_migration_materialize_source_from_result`：handler 不再拆 migration DAO result 或手写 preview channel/count input。
729. channel test 管理路由的 preflight 规划已进入 `ProxyEngine::channel_test_response`：handler 不再读取 channel record、provider 或 stream-check 配置，也不再直接执行 reachability probe。
730. channel delete/key delete 管理路由的 delete source 组装已新增 `proxy_core_adapter::channel_delete_source_from_deleted` / `channel_key_delete_source_from_deleted`：handler 不再直接调用 delete source DTO。
731. channel health reset 管理路由的 reset source 组装已新增 `proxy_core_adapter::channel_health_reset_source_from_response`：handler 不再直接调用 `ChannelHealthResetSource`。
732. health/status/app list 基础管理路由已继续收敛：health/status 直接调用 core request response helper，app list 走 `ProxyEngine::app_list_response`，handler 不再直接调用这些 response source DTO。
733. provider list/current route 管理路由的 source 组装已新增 `proxy_core_adapter::provider_list_source_from_providers` 与 `current_route_source_from_provider`：handler 不再直接调用 provider/current route source DTO 或 ProviderSpec summary 投影。
734. host 生产路径的 `ProxyEngine` 构造入口先前收敛到 `proxy_core_adapter::proxy_engine_from_services`，使 `ProxyState` 和 forwarder 的 materialized route planning 不再散落直连 `ProxyEngine::new`；后续 1059 已删除该一跳 facade，由 adapter 所有权边界内的 `ProxyState::proxy_engine` 直接构造 engine。
735. `proxy_core_boundary` 已新增生产代码 `ProxyEngine::new` 禁用扫描：除 `proxy_core_adapter.rs` 外，host 生产源码必须通过 adapter 构造 core engine，测试模块中的直接实例化仍可用于验证 services。
736. `RequestForwarder` 已删除旧的 self-planning 兼容入口：`RequestContext::create_forwarder`、`RequestForwarder::new`、`forward_with_retry` 和 `build_forward_attempts` 不再存在，forwarder 只接收 `ProxyEngine`/host pipeline 预规划的 attempts。
737. `RequestContext` 已删除旧 forwarder planning 遗留字段：不再保存 providers 链、currentProviderId、session_client_provided 或 rectifier/optimizer/copilot optimizer 配置，只保留响应处理和 usage/error 仍需要的请求事实。
738. `proxy_core_boundary` 已新增 forwarder self-planning 禁用扫描：生产代码不得重新引入 `RequestForwarder::new`、`forward_with_retry`、`build_forward_attempts` 或 `RequestContext::create_forwarder`，确保 forwarder 只消费 `ProxyEngine`/`ForwardPipeline` 已规划 attempts。
739. `RequestContext` 的 app config raw 投影已收敛到 `proxy_core_adapter::app_proxy_config_from_proxy_app_config`：context 不再直接展开 core `ProxyAppConfig.raw` 的 serde 形状，只保留配置读取、response timeout 调用和后续 provider/usage 所需事实。
740. `RequestContext` 不再保存完整 `AppProxyConfig`：构造时只把 app config 投影为 `ResponseRuntimePolicy`，context 字段只保留 response processor 仍需要的 timeout/retry 策略。
741. `ProxyResult` 回填 `RequestContext` 的 outbound model、usage route context 与 selected provider hydration 已收敛到 `proxy_core_adapter::request_context_route_update_from_proxy_result`；`UsageSink` 的 missing pricing warning emission 已收敛到 `proxy_core_adapter::log_usage_request_projection_warnings`；context/host sink 只保留 host DB provider 查找、定价查询和落库字段赋值。
742. `RequestContext::new` 不再调用 `ProviderRouter` selection API 或提前保存 provider：provider 只在 `ProxyEngine` 成功返回 route result 后由 `apply_proxy_result` 回填；转发入口 handler 的 `ProxyRequest` app/interface/body/context bridge 已收敛到 `proxy_core_adapter::json_proxy_request_from_input`，forward error usage 与 Codex error envelope 在选路失败时使用 app/tag fallback。
743. `proxy_core_boundary` 已新增 `RequestContext` provider preselect 禁用扫描：`handler_context.rs` 生产代码不得重新调用 `provider_router`、`.select_providers(` 或 `.select_provider_ids(`，防止请求 context 重新承担 route planning。
744. channel reachability probe 已新增 `ChannelReachabilityProbe` core 端口，probe request 的 app 解析、provider 缺失、DB provider/config 读取、`StreamCheckService` 副作用和 probe 错误包装均由 adapter-owned `CcSwitchChannelReachabilityProbe` 包装；`proxy_core_host` 只装配 probe，core 负责 channel test 编排、preflight failure 和最终 response envelope。
745. app namespace catalog 已收敛到 `ProxyConfigSource::list_apps` 端口，CC Switch 桌面宿主的当前 app catalog 事实由 `proxy_core_adapter::cc_switch_app_kinds` 提供；route candidate provider selection 的 host router 结果已经直接是 provider id 列表，`proxy_core_adapter::route_candidate_provider_ids_from_selection_result` 只负责错误兼容包装；Claude Desktop model route 的 provider id selection 结果与 Provider 回查/错误包装已收敛到 `proxy_core_adapter::claude_desktop_provider_from_selection_result`，完整 Provider 只在 adapter-owned model catalog provider 需要模型路由时通过 DB 加载；ProviderRouter 的 core route error / provider selection failure 到 `AppError` 映射已收敛到 `proxy_core_adapter::{app_error_from_proxy_core_error,app_error_from_provider_selection_failure}`，current provider 的 settings/DB fallback 来源组合已收敛到 `proxy_core_adapter::current_provider_id_from_router_sources`，materialized channel 空表时是否加载 legacy projection 以及最终 records/source 返回已收敛到 `proxy_core_adapter::channel_route_records_from_sources`，`proxy_core_boundary` 已钉住 `proxy_core_host.rs` 生产代码不得直接构造 `ProxyCoreError` variant：`/proxy/v1/apps` 与 `/proxy/v1/groups` 的 handler 不再把 `AppType::all()` 直接传给 engine，`CcSwitchProviderSource` 和 `CcSwitchModelCatalogProvider` 已迁为 adapter-owned。
746. forwarder 上游 URL/effective endpoint 规划已收敛到 `proxy_core_adapter::forward_upstream_url_plan`：Codex Responses->Chat endpoint rewrite、Claude transform endpoint rewrite、Gemini Native URL、full endpoint query 透传和 channel param override 合并不再散落在 `RequestForwarder` 热路径中。
747. 托管账号动态认证 token 刷新已先从 `RequestForwarder` 热路径抽到 `proxy::managed_account_auth::resolve_managed_account_auth`：forwarder 不再直接依赖 `CodexOAuthState`、`CodexOAuthManager` 或 `CopilotAuthManager` 的刷新细节，只消费 materialized `ProviderAuthInfo`、Codex account id 和 session header gate。
748. Copilot 托管账号运行态读取已继续收敛到 `proxy::managed_account_auth`：动态 API endpoint、live `/models` 列表和 model vendor 查询不再让 `RequestForwarder` 直接依赖 `CopilotAuthState`，forwarder 只消费 endpoint/model/vendor 事实。
749. forwarder 成功路径的运行态统计和 failover UI 切换调度已合并为 `record_success_status_and_maybe_switch` / `schedule_failover_switch`：四条成功返回分支不再各自复制 success counter、success rate 和 `FailoverSwitchManager::try_switch` 调度细节，为后续把 failover side effect 升级成 host 端口保留单入口。
750. `FailoverSwitchManager` 已新增 `spawn_try_switch` 调度入口：`RequestForwarder` 不再直接调用 `try_switch` 或展开后台任务闭包，failover UI/托盘切换副作用继续留在 host manager 内部。
751. channel `statusCodeMapping` 已进入运行时响应路径：`ResolvedChannelAttempt` 保留选中地址的 mapping，`proxy-core::mapped_channel_response_status` 负责纯规则解析，forwarder 在 success/error 判定前按 channel 映射 HTTP status，非法或非数值 target 保持忽略以兼容现有管理 API 可存储的语义标签。
752. forwarder 成功状态更新策略已迁入 `proxy-core::record_forward_success_status`：success counter、last_error 清理、success_rate 计算、failover_count 增量和“是否需要 host 调度当前 provider 切换”的决策由 core 纯函数返回，host forwarder 只负责状态锁和 `FailoverSwitchManager` 副作用。
753. active route target DTO 构造已迁入 `proxy-core::current_route_target_from_input`：host adapter 只把 `ForwardAttempt` 或 provider-only 事实投影为 core input，forwarder/server 不再手写 `CurrentRouteTarget` 的 provider/channel/model 字段拷贝。
754. forwarder 失败状态更新策略已迁入 `proxy-core::record_forward_failure_status`：failed counter、last_error 和 success_rate 计算不再散落在 rectifier/client-error/terminal failure 分支，host forwarder 只负责状态锁和错误分支控制流。
755. runtime status 的 active target 注入与按 appType 排序规则已迁入 `proxy-core::apply_proxy_runtime_active_targets`：server 只负责读取 host runtime map，状态响应里的排序口径由 core 维护。
756. request started 与 active connection 的 runtime status 计数策略已迁入 `proxy-core::{record_forward_request_started_status,record_active_connection_acquired_status,record_active_connection_released_status}`：forwarder 保留 RAII guard 和时间戳注入，计数加减与饱和规则由 core 维护。
757. server lifecycle 的 runtime status 更新策略已迁入 `proxy-core::{record_proxy_server_started_status,record_proxy_server_stopped_status,apply_proxy_runtime_uptime}`：server 继续负责 socket 监听、Instant 计时和事件发送，running/address/port/uptime 字段变更由 core 维护。
758. server lifecycle 与 proxy events connected/lagged 事件名、payload contract 已经通过 `proxy_core_adapter::{server_started_event_message,server_stopped_event_message,proxy_events_connected_message,proxy_events_lagged_message}` 投影为 event bus message，底层 event 常量和 payload builder 不再作为 adapter 公共入口：server/event bus 只负责监听事实、sequence/timestamp 和现有事件总线 emit。
759. `ProxyServerInfo` 构造已迁入 `proxy-core::proxy_server_info_from_parts`：server start 和 service 已运行返回路径都只提供 address/port/started_at 事实，不再手写 core DTO 字段。
760. `ProxyTakeoverStatus` 构造已迁入 `proxy-core::proxy_takeover_status_from_parts`：service 继续负责读取各 app 接管事实，DTO 字段 shape 与序列化 contract 由 core 统一维护。
761. provider-switched 事件名、source 常量与 payload contract 已经通过 `proxy_core_adapter::{provider_switched_failover_event_message,provider_switched_failover_enabled_event_message}` 投影为 Tauri event message：failover manager 与 command 只负责切换副作用和 emit 时机。
762. proxy-official-warning 事件名与 payload contract 已经通过 `proxy_core_adapter::proxy_official_warning_event_message` 投影为 Tauri event message：service 继续负责官方供应商风险判断和 Tauri emit，前端 warning payload shape 由 adapter/core 维护。
763. 未运行代理的 runtime status 默认 DTO 已迁入 `proxy-core::proxy_runtime_status_stopped`：service 只负责判定是否存在 server，stopped 状态字段 shape 由 core 维护。
764. `ProxyError` HTTP JSON body contract 已迁入 `proxy-core::{proxy_error_response_body,upstream_proxy_error_response_body}`：host 仍负责错误枚举和 HTTP status 映射，上游 JSON 透传/文本包装/proxy_error envelope 由 core 统一维护。
765. forwarder 的 route_selected 事件名与 `ForwardAttempt` 到 bus message 的投影已切到 `proxy_core_adapter::route_selected_event_message_from_forward_attempt`：forwarder 只负责 active target 写入和 emit 时机，不再手写事件名或 route-selected payload 组合。
766. request_started 事件名与 payload message 已迁入 `proxy_core_adapter::request_started_event_message`：forwarder 继续负责请求开始时机，不再分别引用事件名常量和 payload builder。
767. `ForwardAttempt` 到 attempt event bus message 的 host 投影已迁入 `proxy_core_adapter::attempt_event_message_from_forward_attempt`：forwarder 只负责 emit 时机和 attempt phase，不再手写 provider/channel 字段拆箱或 attempt 事件名/payload 组合。
768. handler 请求体 `stream` 标志解析已迁入 `proxy-core::request_body_stream_flag`：Claude/Codex/Gemini handler 继续负责读取 body 与使用场景，stream 布尔 contract 由 core 统一维护并被 transport streaming 判定复用。
769. handler 到 core `ProxyRequest` 的 observed request context 装配已迁入 `proxy-core::ProxyRequest::with_observed_request_context`：handler 继续负责 endpoint/model 来源，`requested_model`/headers/extensions 写入由 core domain builder 维护。
770. handler 请求体 JSON 解析契约已迁入 `proxy-core::{parse_json_request_body,parse_json_request_body_or_null}`：host 继续负责 axum body collection，strict JSON 与 Gemini 空 body -> `Null` 语义及 parse error 前缀由 core 统一维护。
771. handler 请求体读取错误文案已迁入 `proxy-core::request_body_read_error_message`：axum body collection 仍由 host 执行，但 read failure 的 client-visible message contract 由 core 维护。
772. 上游请求体序列化错误文案已迁入 `proxy-core::request_body_serialize_error_message`：forwarder 继续调用 core 序列化 helper，serialize failure 的 client-visible message contract 也由 core 维护。
773. raw hyper 上游 URL parse 错误文案已迁入 `proxy-core::invalid_upstream_url_error_message`：forwarder 仍执行 `http::Uri` 解析和发送分支，invalid URL 的 client-visible message contract 由 core URL policy 维护。
774. channel response status mapping 非法目标状态文案已迁入 `proxy-core::invalid_mapped_channel_response_status_message`：forwarder 仍执行 `StatusCode::from_u16` 适配，mapping failure 的 client-visible message contract 由 core transport policy 维护。
775. 非流式响应体读取超时文案已接入 `proxy-core::non_streaming_body_timeout_message`：forwarder 仍负责等待 body/failover retry，timeout 的 client-visible message contract 由 core response timeout policy 维护。
776. 流式响应首包等待/读取错误文案已迁入 `proxy-core::{streaming_header_timeout_message,streaming_body_first_chunk_timeout_message,streaming_body_ended_before_first_chunk_message,streaming_body_first_chunk_read_error_message}`：forwarder 仍负责 reqwest header wait 与 SSE 首包 priming，流式响应等待失败 contract 由 core response timeout policy 维护。
777. `RequestContext` 的请求模型推断已接入 `proxy-core::request_model_for_forward`：context 只传入 app/body 或 Gemini path 事实，普通 body `model` trim/empty fallback 与 Gemini path 提取规则由 core request URL policy 维护。
778. `RequestContext` 的 selected-provider lifecycle 错误与 `unselected:<app>` fallback id 已接入 `proxy-core::{selected_provider_missing_from_source_message,selected_provider_not_applied_message,unselected_provider_fallback_id}`：context 仍负责 DB provider hydration，但错误/fallback contract 不再手写在 host。
779. `RequestContext` 的 Codex 错误体 provider display name fallback 已接入 `proxy-core::selected_provider_display_name_for_error`：context 只提供 selected provider name 与 tag fallback 事实，“选中 provider 用 name，否则用 app tag” 的错误展示 contract 由 core 维护。
780. usage 记录路径里 selected provider 未回填时的跳过日志已接入 `proxy-core::usage_selected_provider_missing_log_message` 与 `UsageSelectedProviderMissingPhase`：host 只传入 tag 和 usage 阶段，流式透传、转换响应、转换流式三类日志文案由 core 统一维护。
781. usage 写入失败 warning 文案已接入 `proxy-core::usage_record_failure_warning_message` 与 `UsageRecordFailureLogContext`：host 只传入 forward-error 或普通 usage 写入失败上下文，`[USG-001]` 与失败请求日志前缀不再散落在 bridge/processor。
782. usage 写入前的 debug 日志格式已接入 `proxy-core::usage_record_debug_log_message`：host `record_usage_internal` 只负责调用 usage sink，日志字段选择、response/outbound model fallback 与 session fallback 由 core 维护。
783. usage logging 开关的 config 读取 fallback 策略已接入 `proxy-core::usage_logging_enabled_from_config_flag`：host 只负责从配置锁读取 `enable_logging`，读取失败时默认开启的兼容策略由 core 维护，`response_processor` 流式与非流式路径复用同一 host helper。
784. usage 路径的 host `Provider` 到 core `ProviderKind` 投影已移入 `proxy_core_adapter::provider_kind_from_provider`：`usage_sink_bridge` 已删除，`response_processor` 也不再反向依赖 bridge 获取 core provider kind。
785. Claude transform handler 的 Codex OAuth provider 判定已改为 `proxy_core_adapter::provider_is_codex_oauth`：handler 只消费 provider fact 的布尔投影，`provider.meta.provider_type == "codex_oauth"` 字符串判断不再留在协议处理分支。
786. forwarder 发送策略里的 Codex OAuth provider 判定已改为 `proxy_core_adapter::provider_is_codex_oauth`：exact header casing 判断继续由 core request-header policy 执行，但 forwarder 不再直接调用 host `Provider::is_codex_oauth()`。
787. forwarder 的 GitHub Copilot upstream 判定已改为 `proxy_core_adapter::provider_is_github_copilot_upstream`：provider_type 与 base URL 的组合识别仍复用 core request URL policy，但 forwarder 不再直接拆 `provider.meta.provider_type`。
788. forwarder 的 full-url provider metadata 判定已改为 `proxy_core_adapter::provider_is_full_url`：Copilot dynamic endpoint 与 upstream URL planning 仍消费布尔事实，但 forwarder 不再直接拆 `provider.meta.is_full_url`。
789. forwarder 的 Provider 级自定义 User-Agent 投影已改为 `proxy_core_adapter::provider_custom_user_agent_header`：forwarder 不再直接读取 `provider.meta.custom_user_agent_header()`，Copilot 指纹 UA 不可覆盖与非法 UA 静默忽略语义集中在 adapter。
790. forwarder 的 Bedrock provider env flag 判定已改为 `proxy_core_adapter::provider_bedrock_env_flag`：Bedrock pre-send optimizer gate 继续复用 core transform policy，但 forwarder 不再直接传入 `provider.settings_config`。
791. forwarder 的 provider settings 模型映射已改为 `proxy_core_adapter::apply_provider_model_mapping_from_provider`，text-only media 预防投影已改为 `proxy-core::request_media::apply_forwarder_media_prevention_from_facts`：forwarder 只传入 `Provider` 与运行期开关，settings schema 读取继续集中在 adapter/core policy 边界。
792. forwarder 的 Anthropic thinking rectifier provider 判定已改为 `proxy_core_adapter::provider_uses_anthropic_rectifiers`：forwarder 不再直接调用 provider kind 推断并 matches `ProviderKind`，只消费是否可运行 rectifier 的布尔事实。
793. stream_check Copilot 动态 endpoint 预解析的 provider 判定已改为 `proxy_core_adapter::{provider_is_github_copilot_stream_check_target,provider_is_full_url,provider_github_copilot_managed_account_id}`：命令层只负责 OAuth manager endpoint 副作用，provider meta/settings 投影集中在 adapter。
794. stream_check service 的 OpenCode/OpenClaw/Hermes base URL 与自定义 User-Agent provider 投影已改为 `proxy_core_adapter` 的 Provider-aware helpers：服务层继续负责 reachability 探测与错误包装，settings/meta schema 读取集中在 adapter。
795. managed account auth 的 Copilot/Codex OAuth 账号 ID 投影已改为 `proxy_core_adapter::{provider_github_copilot_managed_account_id,provider_codex_oauth_managed_account_id}`：认证模块只负责调用 OAuth manager，`ProviderMeta.authBinding`/旧字段兼容逻辑集中在 adapter。
796. provider usage 查询命令里的 usage script 与 Copilot account 投影已改为 `proxy_core_adapter::{provider_usage_script,provider_github_copilot_managed_account_id}`：命令层保留模板分支和外部查询副作用，不再直接穿透 `Provider.meta.usage_script` 或 managed-account 绑定字段。
797. Claude Desktop provider import 的 Claude env、Claude-safe 模型检查与 1M 默认支持 provider 投影已改为 `proxy_core_adapter::{provider_claude_env_settings,provider_claude_models_are_claude_safe,provider_claude_desktop_routes_support_1m_by_default}`：`commands/provider` 继续负责 route suggestion merge/import flow，settings/meta schema 读取集中在 adapter。
798. saved usage script 查询服务的 usage script 读取已改为 `proxy_core_adapter::provider_usage_script`，custom endpoints 服务的列表排序、URL 归一化与 last-used mutation 已改为 `proxy_core_adapter::{provider_custom_endpoint_list,normalize_custom_endpoint_url,custom_endpoint_url_key,mark_custom_endpoint_last_used}`：`services/provider` 继续负责执行脚本、credential fallback、DB 读写与结果格式化，不再直接穿透 provider meta 投影细节。
799. stream check 服务的 provider-level testConfig 读取已改为 `proxy_core_adapter::provider_stream_check_test_config`：服务层继续负责与全局 `StreamCheckConfig` 合并，`Provider.meta.test_config` 的启用过滤集中在 adapter。
800. Claude provider adapter 的 Codex OAuth 判定已改为 `proxy_core_adapter::provider_is_codex_oauth`：Claude transform 与 base URL 提取继续消费布尔事实，不再直接调用 `Provider::is_codex_oauth()` 或本地拆 `Provider.meta.provider_type`。
801. proxy service 的接管策略 provider 判定已改为 `proxy_core_adapter::{provider_is_github_copilot,provider_is_codex_oauth}`：接管写配置仍负责占位符策略和模型字段合并，不再直接调用 `Provider::is_github_copilot()`/`Provider::is_codex_oauth()`。
802. proxy service 的 managed-account 接管聚合判定已改为 `proxy_core_adapter::provider_uses_managed_account_auth`：服务层继续负责接管策略分支，不再直接调用 `Provider::uses_managed_account_auth()`。
803. Codex history migration 的 Codex OAuth provider 过滤已改为 `proxy_core_adapter::provider_is_codex_oauth`：历史归桶与 provider template 迁移继续负责 state/config 改写，不再直接调用 `Provider::is_codex_oauth()`。
804. Gemini provider adapter 的 API key 与 base URL 单字段 getter 已删除；Gemini adapter 继续通过 auth strategy/base URL 对外入口工作，adapter 内部直接调用 core settings extractor 读取 `Provider.settings_config`。
805. Codex provider adapter 的 API key 与 base URL 提取已改为 `proxy_core_adapter::{provider_codex_api_key,provider_codex_base_url}`：Codex adapter 继续负责 auth header 与 upstream URL 构造，不再在 `extract_key`/`extract_base_url` 中直接穿透 `Provider.settings_config`。
806. Codex provider adapter 的 chat-completions 判定、upstream model 与 catalog model ids 已改为 `proxy_core_adapter::{provider_codex_uses_chat_completions,provider_codex_upstream_model,provider_codex_catalog_model_ids}`：Codex adapter 保留 request body 改写时机，settings/meta/TOML 投影集中在 adapter。
807. Codex Chat reasoning 的 provider meta override、base URL/config TOML 与 upstream model fallback 投影已改为 `proxy_core_adapter::provider_codex_chat_reasoning_profile`：Codex adapter 只从请求体传入 client model，并继续把 profile 转成请求参数。
808. Claude provider adapter 的 API format、auth key 与 base URL provider 投影已改为 `proxy_core_adapter::{provider_claude_api_format,provider_claude_auth_key,provider_claude_base_url}`：Claude adapter 保留鉴权策略选择、auth header 构造与 upstream URL 构造，不再在这些入口直接穿透 `Provider.settings_config`/`meta`。
809. Claude provider kind 的 API format、Google OAuth key shape、meta provider type、base URL/settings 组合推断已改为 `proxy_core_adapter::provider_claude_kind`：Claude adapter 只按 provider kind 选择运行时鉴权策略，不再自行拼装推断输入。
810. Claude transform 的 Responses prompt cache provider facts、OpenAI Chat 显式 prompt cache key 与 Codex fast-mode provider fact 已改为 `proxy_core_adapter::{provider_claude_responses_prompt_cache_key,provider_claude_prompt_cache_key,provider_codex_fast_mode_enabled}`：Claude adapter 继续负责请求体转换与日志输出，不再直接读取这些 `Provider.meta/settings_config` 字段。
811. Claude transform 的 OpenAI Chat reasoning_content 保留判定已改为 `proxy_core_adapter::provider_should_preserve_reasoning_content_for_openai_chat`：Claude adapter 继续负责转换时机，只把 provider 与请求体交给 adapter 解析 settings/model facts。
812. Claude normalize pipeline 的 Anthropic tool-thinking history gate 与 DeepSeek thinking-disabled effort 清理已改为 `proxy_core_adapter::{provider_should_normalize_anthropic_tool_thinking_history,provider_normalize_deepseek_thinking_disabled_strip_effort}`：Claude adapter 只保留 normalize 调用顺序，不再直接传入 `Provider.settings_config`。
813. `provider_kind_from_app_type_and_config` 的真实实现已收敛到 `proxy_core_adapter`，并新增 `provider_gemini_kind`：`proxy/providers` 只保留兼容 facade，`proxy_core_adapter` 不再反向依赖 provider adapter 模块来构造 `ProviderSpec` 或 rectifier 判定。
814. provider terminal 启动环境变量投影已改为 `proxy_core_adapter::provider_launch_env_vars_for_app`：命令层只负责加载 provider 与启动终端，Claude/Codex/Gemini 的 settings schema、base URL env key 与 API key/env 转换规则集中在 adapter。
815. Claude takeover 模型字段快照已改为 `proxy_core_adapter::{provider_claude_takeover_model_fields,claude_takeover_model_fields_from_settings}`：proxy service 继续负责 live config 写入、token 占位和 managed-account 分支，Claude 模型 env/schema、1M 标记与显示名 fallback 规则集中在 adapter。
816. takeover placeholder 检测已改为 `proxy_core_adapter::{live_config_has_proxy_placeholder_for_app,provider_settings_have_proxy_placeholder_for_app}`：proxy service 继续决定 `PROXY_MANAGED` 写入和恢复流程，但 Claude/Codex/Gemini live/settings 中的占位符 schema 判定集中在 adapter。
817. Claude Desktop proxy provider 的 credential shape 检测已改为 `proxy_core_adapter::provider_claude_desktop_proxy_has_base_url_and_key`：`claude_desktop_config` 继续负责本地化错误和模式验证，base URL/API key/settings/meta schema 读取集中在 adapter，typed OAuth provider 的无静态 key 放行语义保持精确。
818. Claude Desktop 的 MiMo Anthropic thinking history normalize gate 已改为 `proxy_core_adapter::provider_should_normalize_mimo_anthropic_thinking_history`：`claude_desktop_config` 继续负责请求体改写，provider API format、MiMo 模型/endpoint 判定集中在 adapter。
819. Claude Desktop direct/proxy provider 的配置兼容性 validation facts 已改为 `proxy_core_adapter::{provider_claude_desktop_direct_validation_issue,provider_claude_desktop_proxy_config_validation_issue}`：`claude_desktop_config` 继续负责本地化错误、route 校验与直连凭证提取，settings/meta schema 判定集中在 adapter。
820. Codex switch-away backfill 的 DB-stored `modelCatalog` 原始值读取已内联到 provider live backfill source：写回策略仍在 host 边界，provider settings 内部字段不再通过 adapter-local 单字段 getter 暴露。
821. OpenClaw live 写入 fallback 的 raw provider shape 检测已改为 `proxy_core_adapter::provider_openclaw_has_live_provider_fields`：provider live 继续负责 typed parse/raw write 策略，`baseUrl`/`api`/`models` 字段存在性规则集中在 core/domain 与 adapter。
822. Codex live 默认导入的 provider category 推断已改为 `proxy_core_adapter::provider_codex_imported_live_category`：provider live 继续负责导入与 DB 写入，`auth` 登录材料、`OPENAI_API_KEY` 与 TOML experimental bearer token 的官方/自定义分类规则集中在 adapter。
823. legacy config sync 的 Codex live settings 结构校验与 `auth`/`config` 提取已改为 `proxy_core_adapter::provider_codex_live_settings_parts`：`services/config` 继续负责旧配置同步和中文错误消息，provider settings shape 读取集中在 adapter。
824. ProviderService Codex provider validation 的 settings/auth/config shape 判定已改为 `proxy_core_adapter::provider_codex_validation_parts`：ProviderService 继续负责本地化错误和 TOML 语法校验，provider settings 字段读取集中在 adapter。
825. provider live snapshot 的 Codex `auth`/`config` 提取已改为 `proxy_core_adapter::provider_codex_live_snapshot_parts`：live 写入继续负责文件落盘和错误文案，snapshot 路径保留旧的 auth 存在性语义，字段读取集中在 adapter。
826. provider live snapshot 的 OpenCode provider fragment 投影已改为 `proxy_core_adapter::provider_opencode_live_provider_fragment`：live 写入继续负责 typed/raw 写入和日志，整份 opencode config 到单 provider fragment 的兼容提取集中在 adapter。
827. OpenCode live 写入 fallback 的 raw provider shape 检测已改为 `proxy_core_adapter::opencode_live_provider_fragment_has_provider_fields`：provider live 继续负责 raw write 策略，`npm`/`options` 字段存在性规则集中在 core/domain 与 adapter。
828. Gemini live 写入的 `config` object/null/absent/invalid 投影已改为 `proxy_core_adapter::provider_gemini_live_config_object`：`write_gemini_live` 继续负责 settings.json 合并与本地化错误，provider settings 字段读取集中在 adapter。
829. Codex common-config 路径的 settings `config` TOML 文本读取已改为 `proxy_core_adapter::codex_config_text_from_settings`：ProviderService/provider live 继续负责 TOML merge/remove/export 与错误文案，Codex settings schema 字段读取集中在 adapter。
830. Gemini common-config 路径的 settings `env` 对象读取已改为 `proxy_core_adapter::gemini_env_map_from_settings`：ProviderService/provider live 继续负责 JSON subset/merge/remove/export 与 credential 排除，Gemini settings schema 字段读取集中在 adapter。
831. ProviderService Codex credential 提取的 auth 对象与 auth/config API key 解析已改为 `proxy_core_adapter::{codex_auth_object_value_from_settings,codex_api_key_from_auth_and_config}`：服务层继续负责本地化错误与 base_url regex 兼容解析，Codex settings 字段读取与 API key 来源规则集中在 adapter。
832. ProviderService Claude credential 提取的 env/api key/base URL 投影已改为 `proxy_core_adapter::claude_env_credentials_from_settings`：服务层继续负责本地化错误并保留旧 helper 的原始字符串返回语义，Claude settings 字段读取集中在 adapter。
833. Gemini live import/read 路径的 `env_to_json` envelope 提取已改为 `proxy_core_adapter::gemini_env_value_from_env_json`：provider live 继续负责文件读取、缺失错误与最终 `{env, config}` 组装，Gemini `.env` JSON envelope 字段读取集中在 adapter。
834. legacy ConfigService Codex live backfill 的 restored `auth`/`config` 提取已改为 `proxy_core_adapter::codex_restored_live_settings_parts`：ConfigService 继续负责 live 同步、token restore 与写回目标 provider，restored settings 字段读取集中在 adapter。
835. ProviderService 的 Claude/OpenCode/OpenClaw/Hermes settings object shape 判定已改为 `proxy_core_adapter::provider_settings_config_is_object`：ProviderService 继续负责各 app 的本地化错误 key/message，provider settings 顶层 shape 读取集中在 adapter。
836. Gemini live 写入的 provider settings 到 `.env` map 投影与 strict validation 入口已改为 `proxy_core_adapter::{provider_gemini_env_map,validate_provider_gemini_settings_strict}`：provider live 继续负责 auth mode 分支、原子写 `.env`、settings.json 合并与安全标记写入，Gemini settings 转换/校验入口集中在 adapter。
837. ProviderService Gemini credential 提取的 settings 到 `.env` map 投影已改为复用 `proxy_core_adapter::provider_gemini_env_map`：服务层继续负责缺失 API key 本地化错误与 base URL 默认值，Gemini env schema 转换集中在 adapter。
838. ProviderService OpenCode/OpenClaw/Hermes credential 提取已改为 `proxy_core_adapter::{provider_opencode_credential_parts,provider_openclaw_credential_parts}`：服务层继续负责缺失 options/API key 的本地化错误与空 base URL fallback，OpenCode options 与 OpenClaw/Hermes 顶层 credential 字段读取集中在 adapter。
839. ProviderService OpenCode/OpenClaw common-config provider-specific 字段剥离已改为 `proxy_core_adapter::{opencode_common_config_value_from_settings,openclaw_common_config_value_from_settings}`：服务层继续负责 snippet 序列化与空对象输出，OpenCode `options.apiKey/baseURL` 与 OpenClaw `apiKey/baseUrl` 字段规则集中在 adapter。
840. ProxyService live token 回填的 app-specific `env`/`auth` token 提取、占位符过滤与 provider settings 修补已改为 `proxy_core_adapter::provider_settings_with_live_token_sync`：服务层继续负责当前 provider 查询、异常日志和 DB 写回，Claude/Codex/Gemini live/settings schema 投影集中在 adapter。
841. ProxyService Codex takeover 的 config.toml proxy base URL/wire_api/model 投影与 provider `modelCatalog` 注入已改为 `proxy_core_adapter::{codex_takeover_toml_config_for_provider,attach_codex_model_catalog_from_provider}`：服务层继续负责读取/写入 live 文件、备份策略和 OAuth auth 保留分支，Codex provider settings/schema 读取集中在 adapter。
842. ProxyService Gemini takeover 的 `GOOGLE_GEMINI_BASE_URL` 与 `GEMINI_API_KEY` env 注入已改为 `proxy_core_adapter::apply_gemini_takeover_env_fields`：服务层继续负责 live 文件读取/写入与 strict/best-effort 控制流，Gemini takeover env schema 规则集中在 adapter。
843. ProxyService Codex takeover 的 `auth.OPENAI_API_KEY` 占位符注入已改为 `proxy_core_adapter::{apply_codex_takeover_auth_placeholder_if_present,ensure_codex_takeover_auth_placeholder}`：服务层继续区分 live takeover 的“已有 auth 才写”和 provider sync 的“必要时创建 auth”两种控制流，Codex auth schema 写入规则集中在 adapter。
844. ProxyService Codex/Gemini takeover cleanup 的占位符移除已改为 `proxy_core_adapter::{remove_codex_takeover_auth_placeholder_if_present,remove_gemini_takeover_env_fields_if_present}`：服务层继续负责 live 文件读写、Codex TOML 清理和本地 URL 判定，Codex/Gemini takeover cleanup schema 规则集中在 adapter。
845. ProxyService Claude takeover cleanup 的 env 占位符与本地 base URL 移除已改为 `proxy_core_adapter::remove_claude_takeover_env_fields_if_present`：服务层继续负责 Claude live 文件读写和本地 URL 判定，Claude takeover token/base URL cleanup schema 规则集中在 adapter。
846. ProxyService live takeover 是否匹配当前代理地址的 app-specific schema 判定已改为 `proxy_core_adapter::live_takeover_config_matches_proxy_for_app`：服务层继续负责构造当前 Claude/Gemini 与 Codex 代理 URL、读取 live 文件，Claude/Gemini env base URL、Codex TOML active provider base_url、尾斜杠兼容与占位符组合判定集中在 adapter。
847. ProxyService Codex provider-derived backup 的 MCP server 合并与 OAuth auth 保留投影已改为 `proxy_core_adapter::{preserve_codex_mcp_servers_from_existing_config,preserve_codex_oauth_auth_in_backup_if_present}`：服务层继续负责读取现有 backup、settings 开关和本地化错误映射，Codex `config` TOML、`mcp_servers` 冲突策略、OAuth auth shape 与 provider token 写回规则集中在 adapter。
848. ProxyService Codex preserve-auth live 写入路径的占位符 auth 判定与 config-only live config 文本构造已改为 `proxy_core_adapter::codex_preserved_auth_live_config_text_if_proxy_placeholder`：服务层继续负责 preserve 开关、provider/category 分支和文件落盘，`auth.OPENAI_API_KEY` 占位符识别、可选 modelCatalog 投影与 provider token 写回 config TOML 的规则集中在 adapter。
849. ProxyService Codex verbatim live 写入的 auth/config/modelCatalog 分流已改为 `proxy_core_adapter::{codex_live_write_projection,CodexLiveWriteProjection}`：服务层继续负责 auth.json/config.toml 路径与文件落盘，snapshot backup 与 provider-rebuilt backup 的 config 投影、空 auth 删除、auth/config/no-op 写入分支集中在 adapter。
850. ProxyService Claude takeover env 字段写入已改为 `proxy_core_adapter::{apply_claude_takeover_fields_with_policy_and_models,ClaudeTakeoverAuthPolicy}`：服务层继续负责 provider 类型判定、模型字段来源选择、live 文件读写和占位符值传递，Claude env normalization、base URL 注入、模型覆盖字段清理、token placeholder 策略集中在 adapter。
851. ProxyService Codex takeover cleanup 的 config.toml base URL 与 bearer placeholder 清理已改为 `proxy_core_adapter::remove_codex_takeover_config_placeholders_if_present`：服务层继续负责 live 文件读写、auth placeholder 清理调用和本地代理 URL 判定注入，Codex TOML provider base_url 与 `experimental_bearer_token` 清理规则集中在 adapter。
852. ProxyService Gemini provider-derived backup 的 `.env` envelope 投影已改为 `proxy_core_adapter::gemini_live_backup_from_effective_settings`：服务层继续负责 common config 组装、序列化和 DB 保存，Gemini takeover 仅恢复 `.env`、不把 settings.json `config/mcpServers` 写入 live backup 的规则集中在 adapter。
853. ProxyService live backup snapshot 的代理占位符跳过判定已改为 `proxy_core_adapter::live_backup_snapshot_from_live_config`：服务层继续负责读取 live 文件、日志、序列化和 DB 保存，Claude/Codex/Gemini app-specific proxy placeholder 检测与 clean snapshot 投影集中在 adapter。
854. ProxyService restore fallback 的 live backup 可恢复判定已复用 `proxy_core_adapter::live_backup_snapshot_from_live_config`：服务层继续负责读取 backup row、JSON 解析、落盘与 SSOT fallback，备份值是否包含代理占位符以及 clean restore value 选择集中在 adapter。
855. ProxyService live takeover 检测入口已直接复用 `proxy_core_adapter::live_config_has_proxy_placeholder_for_app`：服务层继续负责读取 Claude/Codex/Gemini live 文件与兜底入口编排，Claude env、Codex auth/config、Gemini env 的占位符识别规则集中在 adapter。
856. ProxyService takeover cleanup 的本地代理 URL 判定已改为 `proxy_core_adapter::is_local_proxy_url`：服务层继续负责读取/写回 live 文件与调用 app-specific cleanup，`http://127.0.0.1`、`localhost`、`0.0.0.0`、IPv6 loopback/unspecified 前缀分类规则集中在 adapter。
857. ProxyService Codex takeover TOML 写入路径已删除 `apply_codex_proxy_toml_config_for_provider` 本地 facade，生产调用与回归测试直接使用 `proxy_core_adapter::codex_takeover_toml_config_for_provider`：服务层继续负责 live 文件读取、provider 查找、catalog attachment 与写回，base_url/wire_api/model 恢复策略集中在 adapter。
858. ProxyService Codex takeover 模型目录 attachment 已删除 `attach_codex_model_catalog_from_provider` 本地 facade，生产调用直接使用 `proxy_core_adapter::attach_codex_model_catalog_from_provider`：服务层继续负责决定何时把 provider catalog 投影到 live config，catalog pointer/inline catalog 写入规则集中在 adapter。
859. ProxyService Claude takeover provider 策略已改为 `proxy_core_adapter::{apply_claude_takeover_fields_for_provider,apply_claude_takeover_fields_with_policy}`：服务层继续负责读取 live、构建 effective provider、选择有无 provider 的接管路径与写回，受管账号 token 策略、Copilot/Codex 例外、模型字段来源选择和 Claude env 字段投影集中在 adapter。
860. ProxyService live takeover 代理地址构造已改为 `proxy_core_adapter::proxy_live_urls_from_listen_parts`：服务层继续负责读取 proxy_config、使用运行中 server 端口覆盖静态端口并保留端口 0 错误文案，`0.0.0.0`/`::` 到 loopback 的客户端地址转换、IPv6 bracket 和 Codex `/v1` base URL 生成规则集中在 adapter。
861. ProxyService hot-switch 的 official provider 阻断已改为 `proxy_core_adapter::should_block_proxy_switch_to_provider_category`：服务层继续负责 provider 读取、错误文案和实际切换流程，proxy takeover active + category=official 的阻断策略集中在 adapter，并与 commands/provider service 入口共用同一规则。
862. ProxyService 的 test-only `update_toml_base_url` facade 与重复服务层测试已删除：Codex TOML 单字段更新规则继续由 `codex_config::update_codex_toml_field` 的底层单测覆盖，服务层只保留 takeover TOML 组合策略和 live 写入流程相关回归。
863. ProxyService Codex backup projection 错误文案映射已改为 `proxy_core_adapter::codex_backup_projection_error_message`：服务层继续负责读取现有备份、调用 MCP/OAuth backup projection 和拼接上层上下文，`CodexBackupProjectionIssue` 到稳定中文错误文案的映射集中在 adapter。
864. ProxyService 的 `write_codex_live` 纯转发 facade 已删除：需要原样写 Codex live 的恢复、cleanup 和测试路径直接调用 `write_codex_live_verbatim`，provider-aware 写入与 takeover 写入继续保留独立入口。
865. ProxyService live Token 回写 provider settings 的字段投影已改为 `proxy_core_adapter::sync_provider_settings_with_live_token`：服务层继续负责读取当前 provider、记录 app/provider 上下文日志和数据库持久化，Claude/Codex/Gemini live token 提取、占位符跳过、settings_config 形态校验与写回变更判定集中在 adapter。
866. ProxyService Codex provider live 写入参数拆解已改为 `proxy_core_adapter::codex_provider_live_write_parts`：服务层继续负责调用 `codex_config` 落盘与保留不同业务上下文错误文案，provider category、auth 引用、config.toml 文本提取与缺 auth 分支集中在 adapter。
867. ProxyService Codex preserve-auth live 写入的开关 gate 已改为 `proxy_core_adapter::codex_preserved_auth_live_config_text_for_policy`：服务层继续负责读取全局设置和落盘，preserve 开关关闭时的 no-op、占位符 auth 判定、可选 modelCatalog 投影与 config-only live 文本构造集中在 adapter。
868. ProxyService Codex takeover live 字段投影已改为 `proxy_core_adapter::{apply_codex_takeover_fields_for_provider,CodexTakeoverAuthPolicy}`：服务层继续负责读取 live、查当前 provider 和写回，已有 live 接管只改已有 auth、provider 同步时确保 auth 占位符、config.toml base_url/wire_api/model 更新与 modelCatalog attachment 集中在 adapter。
869. Host Codex live snapshot/sync 写入参数中的 provider category 投影已并入 `proxy_core_adapter::{provider_codex_live_settings_parts,provider_codex_live_snapshot_parts}`：`ConfigService` 与 `provider/live` 继续负责错误映射、backfill 与落盘，category/auth/config_text 的 provider 事实抽取集中在 adapter。
870. Host Codex provider backfill 的 token 恢复与统一会话 bucket 剥离策略已改为 `proxy_core_adapter::provider_codex_backfill_parts`：`ConfigService` 与 `provider/live` 继续负责实际 backfill mutation、日志和写回，基于 provider category/settings_config 的 restore token 与 strip unified-session 判定集中在 adapter。
871. ProxyService Codex live backup 的统一会话 bucket 注入已改为 `proxy_core_adapter::apply_codex_unified_session_bucket_for_provider`：服务层继续负责构造 effective settings、读取现有备份和错误上下文，provider category 到 official-only unified-session 注入的参数投影集中在 adapter。
872. ProxyService takeover 启用后的官方供应商风险告警判定已改为 `proxy_core_adapter::should_emit_proxy_official_warning_for_provider`：服务层继续负责读取当前 provider 与 Tauri 事件发送，provider category 到 official warning 的策略判定集中在 adapter。
873. ProviderService 当前 Codex 官方供应商 live 重应用 gate 已改为 `proxy_core_adapter::should_reapply_codex_official_live_for_provider`：服务层继续负责 current provider 解析、backup/live 所有权判断与写回，统一会话变更只作用于 official category 的判定集中在 adapter。
874. commands/ProxyService/ProviderService 的 proxy takeover official provider 阻断调用已改为 `proxy_core_adapter::should_block_proxy_switch_to_provider`：调用方继续负责 takeover 活跃状态、provider 读取和错误文案，Provider 到 category 的投影集中在 adapter 并复用同一阻断规则。
875. Host Codex switch-away backfill 的 restore/strip mutation 已改为 `proxy_core_adapter::{restore_codex_settings_for_provider_backfill,strip_codex_unified_session_bucket_for_provider_backfill}`：`ConfigService` 与 `provider/live` 继续负责上下文错误处理、日志和写回，provider category/settings_config 到 token restore 与 unified-session strip 的实际 mutation 集中在 adapter。
876. Provider live sync/save/switch 的 proxy-owned live 判定已改为 `proxy_core_adapter::{proxy_live_config_owned_by_takeover,proxy_switch_should_hot_switch}`：服务层继续负责读取 backup、检测 live 占位符和执行写回，backup/live placeholder 到“由代理接管拥有”以及 takeover active 到 hot-switch 的布尔策略集中在 adapter。
877. ProxyService hot-switch 的 backup 更新触发判定已复用 `proxy_core_adapter::proxy_live_config_owned_by_takeover`：服务层继续负责读取 backup、检测 live 占位符、更新 DB/settings 与执行 live/backup 写回，hot-switch 中 backup row/live placeholder 到“需要同步备份”的策略集中在 adapter。
878. app 启动异常恢复与退出清理的 takeover 残留恢复判定已复用 `proxy_core_adapter::proxy_live_config_owned_by_takeover`：入口层继续负责读取全局 backup、检测 live 占位符和调用恢复流程，backup/live placeholder 到“需要恢复”的布尔策略集中在 adapter。
879. ProxyService Codex preserve official auth 的全局开关读取已改为 `proxy_core_adapter::{preserve_codex_oauth_auth_in_backup_for_configured_policy,codex_preserved_auth_live_config_text_for_configured_policy}`：服务层继续负责已有备份读取、错误上下文和 live 文件写入，preserve-auth 设置到 backup/live config 投影的策略入口集中在 adapter。
880. ProxyService 单 app takeover 已标记状态的可复用/重建前恢复判定已改为 `proxy_core_adapter::{proxy_takeover_marked_state_is_reusable,proxy_takeover_should_restore_existing_backup_before_retakeover}`：服务层继续负责读取 backup、检测 live 是否指向当前代理、恢复旧备份和重新接管，backup/live match 到幂等返回或恢复旧 backup 的布尔策略集中在 adapter。
881. ProxyService hot-switch 的 Codex backup-only live 刷新判定已改为 `proxy_core_adapter::proxy_hot_switch_should_refresh_codex_live_from_backup`：服务层继续负责构造 effective settings、拆解 live 写入参数和落盘，app 类型、backup row 与 live takeover 状态到“是否从 backup 刷新 Codex live”的布尔策略集中在 adapter。
882. ProxyService hot-switch 的 Codex takeover-owned live 同步判定已改为 `proxy_core_adapter::proxy_hot_switch_should_sync_codex_live_while_proxy_active`：服务层继续负责按 provider 写回 Codex live，app 类型与 live takeover 状态到“是否代理活跃时同步 Codex live”的布尔策略集中在 adapter。
883. ProxyService hot-switch 的 Claude proxy-owned live 同步判定已改为 `proxy_core_adapter::proxy_hot_switch_should_sync_claude_live_while_proxy_active`：服务层继续负责按 provider 写回 Claude live，app 类型与 proxy-owned live 状态到“是否代理活跃时同步 Claude live”的布尔策略集中在 adapter。
884. Host Claude live 写入前的内部字段清洗已改为 `proxy_core_adapter::sanitize_claude_settings_for_live`：`ConfigService`、`ProviderService` 与 `ProxyService` 继续负责 live 文件读写和 provider 状态回填，`api_format`/`apiFormat`/OpenRouter compatibility 字段不写入 Claude live 的投影规则集中在 adapter。
885. Provider common-config 的 JSON 子集匹配和数组移除算法已改为 `proxy_core_adapter::{json_value_is_subset,json_array_contains_subset,json_remove_array_items}`：`provider/live` 继续负责 common-config snippet 解析、merge/remove 编排和 AppError 映射，JSON subset 语义集中在 adapter。
886. Provider common-config 的 TOML 子集匹配和删除算法已改为 `proxy_core_adapter::{toml_item_is_subset,toml_value_is_subset,toml_array_contains_subset,toml_remove_array_items,remove_toml_table_like}`：`provider/live` 继续负责 Codex config.toml 解析、merge 编排和错误映射，TOML subset/remove 语义集中在 adapter。
887. Provider common-config 的 JSON deep merge/remove 编排已改为 `proxy_core_adapter::{json_deep_merge,json_deep_remove}`：`provider/live` 继续负责 Claude/Gemini snippet 解析和 AppError 映射，JSON 对象递归合并、数组差集删除与空对象清理语义集中在 adapter。
888. Provider common-config 的命中判定已改为 `proxy_core_adapter::{contains_common_config_snippet,provider_uses_common_config}`：`provider/live` 继续负责 DB snippet 读取、apply/remove 编排和错误日志，App 类型、provider meta 开关与 legacy snippet 到“是否使用公共配置”的策略集中在 adapter。
889. Provider credential 提取已改为 `proxy_core_adapter::{provider_credential_values,ProviderCredentialIssue}`：`ProviderService::extract_credentials` 继续保留 Claude Desktop gateway 解析和 AppError 本地化映射，Claude/Codex/Gemini/OpenCode/OpenClaw/Hermes 的 credential 来源解析与缺字段分类集中在 adapter。
890. Provider common-config snippet 生成已改为 `proxy_core_adapter::{common_config_snippet_from_settings,CommonConfigSnippetIssue}`：`ProviderService` 继续负责 current provider 查询和 AppError 文案映射，Claude/Codex/Gemini/OpenCode/OpenClaw/Hermes 的 snippet 清洗、序列化、TOML parse 分类集中在 adapter。
891. Claude provider 模型字段规范化已改为 `proxy_core_adapter::normalize_claude_models_in_value`：`provider/mod` 继续负责 add/update 时机，`provider/live` 继续负责 live import 时机，旧 `ANTHROPIC_SMALL_FAST_MODEL` 到 `ANTHROPIC_DEFAULT_*` 的回填与清理策略集中在 adapter。
892. Provider settings 基础校验已改为 `proxy_core_adapter::{provider_settings_validation_parts,ProviderSettingsValidationIssue}`：`ProviderService` 继续负责 Claude Desktop/Gemini 专用校验、Codex TOML 语义校验和 AppError 本地化映射，Claude/Codex/OpenCode/OpenClaw/Hermes 的 settings object/auth/config 形态分类集中在 adapter。
893. Provider common-config 的 TOML deep merge 算法已改为 `proxy_core_adapter::merge_toml_table_like`：`provider/live` 继续负责 snippet 解析、apply/remove 编排和 AppError 映射，TOML 表递归合并语义集中在 adapter 并与 subset/remove helper 同区维护。
894. Provider common-config 的 settings apply/remove 编排已改为 `proxy_core_adapter::{apply_common_config_to_settings,remove_common_config_from_settings,CommonConfigSettingsMutationIssue}`：`provider/live` 继续负责 DB snippet 读取、日志和 AppError 文案映射，Claude/Codex/Gemini settings mutation 及 unsupported app no-op 策略集中在 adapter。
895. Provider common-config 存储前规范化已改为 `proxy_core_adapter::{provider_common_config_storage_normalization_requires_snippet,normalize_provider_common_config_for_storage}`：`provider/live` 继续负责 DB snippet 读取、日志和 AppError 映射，显式 meta 开关到“保存前是否剥离公共配置”以及具体 settings 变换集中在 adapter，避免 legacy subset 检测误影响新增/更新存储路径。
896. Provider switch-away backfill 的 Codex settings 恢复、统一会话 bucket 清理和 DB `modelCatalog` 优先保留已改为 `proxy_core_adapter::{restore_live_settings_for_provider_backfill,ProviderBackfillSettingsWarning}`：`provider/live` 继续负责 DB snippet 读取和 warning 日志，Codex-only backfill projection 及诊断事件集中在 adapter。
897. Provider switch-away backfill 的 common-config snippet 剥离编排已改为 `proxy_core_adapter::strip_common_config_from_live_settings_for_backfill`：`provider/live` 继续负责 DB snippet 读取与 warning 日志，provider meta/legacy snippet 命中、剥离失败降级为原 live settings、以及后续 Codex backfill projection 的串联集中在 adapter。
898. Provider live 写入前的 effective settings 构造已改为 `proxy_core_adapter::{build_effective_settings_with_common_config,ProviderEffectiveSettingsWarning}`：`provider/live` 继续负责 DB snippet 读取、Claude Desktop/live 文件写入和 warning 日志，provider meta/legacy snippet 命中、公共配置 apply 与失败保留原 settings 的策略集中在 adapter。
899. OpenCode additive provider live 写入计划已改为 `proxy_core_adapter::{provider_opencode_live_write_plan,OpenCodeLiveWriteConfig}`：`provider/live` 继续负责 `opencode_config` typed/raw 写入与日志，完整 config fragment 提取、typed parse、raw fallback 与 invalid 分类集中在 adapter。
900. OpenClaw additive provider live 写入计划已改为 `proxy_core_adapter::{provider_openclaw_live_write_plan,OpenClawLiveWriteConfig}`：`provider/live` 继续负责 `openclaw_config` typed/raw 写入与日志，typed parse、raw fallback 与 invalid 分类集中在 adapter。
901. OpenCode live provider 导入投影已改为 `proxy_core_adapter::{provider_from_opencode_live_config,OpenCodeLiveImportIssue}`：`provider/live` 继续负责读取 live typed providers、重复 ID 过滤、DB 保存和日志，OpenCode typed config 到 `Provider`/`settings_config`/`live_config_managed` 的投影集中在 adapter。
902. OpenClaw live provider 导入投影已改为 `proxy_core_adapter::{provider_from_openclaw_live_config,OpenClawLiveImportIssue}`：`provider/live` 继续负责读取 live typed providers、重复 ID 过滤、DB 保存和日志，空 ID/无模型校验、首模型名 display name 推导、typed config 到 `Provider`/`settings_config`/`live_config_managed` 的投影集中在 adapter。
903. Hermes live provider 导入投影已改为 `proxy_core_adapter::{provider_from_hermes_live_config,HermesLiveImportIssue}`：`provider/live` 继续负责读取 live providers、重复名称过滤、DB 保存和日志，空名称校验、Hermes config 到 `Provider`/`settings_config`/`live_config_managed` 的投影集中在 adapter。
904. 通用 default live 配置导入投影已改为 `proxy_core_adapter::provider_from_default_live_settings`：`provider/live` 继续负责 takeover 检测、live 文件读取、DB 保存和 current provider 写入，`settings_config` 到 default `Provider`、Codex official/custom category 推导和非 Codex custom category 归类集中在 adapter。
905. Gemini live settings 写入合并规则已改为 `proxy_core_adapter::gemini_live_settings_to_write`：`provider/live` 继续负责 settings 文件存在性判断、读取错误策略、env/settings 写文件和 auth security 写入，provider `config` 覆盖 existing settings、缺省/null config 保留 existing settings 的纯数据规则集中在 adapter。
906. OpenCode/OpenClaw additive provider live 写入动作投影已改为 `proxy_core_adapter::{provider_opencode_live_write_projection,provider_openclaw_live_write_projection,OpenCodeLiveWriteAction,OpenClawLiveWriteAction}`：`provider/live` 继续负责 typed/raw config 写入和日志，typed/raw/reject 动作分类、invalid live config 错误消息和 OpenCode full-config fragment warning 标记集中在 adapter。
907. Gemini live settings 读取/导入形状已改为 `proxy_core_adapter::gemini_live_settings_from_env_json_and_config`：`provider/live` 继续负责 `.env` 与 `settings.json` 文件存在性判断和读取，Gemini env JSON + settings config 到 `{env,config}` provider settings 形状的投影集中在 adapter。
908. Codex live settings 的 model catalog 复原投影已改为 `proxy_core_adapter::codex_live_settings_with_model_catalog`：`provider/live` 继续负责读取 `auth.json`/`config.toml` 和 catalog projection 文件，backfill 继续负责 provider/live 数据来源选择，`modelCatalog` 附加到 live settings 的纯数据规则集中在 adapter。
909. Common config mutation issue 的错误文本已改为 `proxy_core_adapter::common_config_settings_mutation_issue_message`：`provider/live` 继续负责把迁移模块 issue 包装为 `AppError`，Claude/Codex/Gemini common config apply/remove 的具体错误消息集中在 adapter。
910. Provider common config snippet 与 settings validation issue 的错误契约已改为 `proxy_core_adapter::{common_config_snippet_issue_message,provider_settings_validation_issue_spec,LocalizedErrorSpec}`：`ProviderService` 继续负责 `AppError` 包装和本地化错误类型落地，snippet serialization/TOML parse 文本、provider settings validation key/中英文消息集中在 adapter。
911. Provider credential issue 的错误契约已改为 `proxy_core_adapter::provider_credential_issue_spec`：`ProviderService` 继续负责 Claude Desktop gateway 特例和 `AppError` 包装，Claude/Codex/Gemini/OpenCode/OpenClaw credential validation 的错误 key 与中英文消息集中在 adapter。
912. Default live import 的 settings 归一化已改为 `proxy_core_adapter::provider_default_live_import_settings`：`provider/live` 继续负责 live 文件读取、takeover gate、DB 保存和 current provider 写入，Claude 旧模型字段归一化等 default-import 前 settings shape 规则集中在 adapter。
913. Default live import 的跳过策略已改为 `proxy_core_adapter::{should_skip_manual_default_live_import,should_skip_startup_default_live_import}`：`provider/live` 继续负责查询 DB 是否已有 provider/非官方 seed provider，additive app 不走通用导入、手动导入允许官方 seed 共存、启动导入遇到任意 provider 即跳过的策略集中在 adapter。
914. Provider live 同步范围策略已改为 `proxy_core_adapter::{provider_live_sync_scope,ProviderLiveSyncScope}`：`provider/live` 继续负责 DB 读取、current provider 解析、takeover-aware live/backup 写入、MCP/Skill sync，additive app 同步全部 provider、switch app 只同步当前 provider 的策略集中在 adapter。
915. Provider live 同步包含策略已改为 `proxy_core_adapter::provider_should_sync_to_live`：`provider/live` 继续负责遍历 DB provider 与写 live 文件，`liveConfigManaged=false` 的 DB-only provider 跳过同步、未知/managed provider 参与同步的判定集中在 adapter。
916. Provider live config 存在性检查的错误容忍策略已改为 `proxy_core_adapter::{provider_live_config_presence_error_policy,ProviderLiveConfigPresenceErrorPolicy}`：`ProviderService` 继续负责调用 live config 解析/查询，DB-only provider 遇到 live 解析错误按缺失处理、未知/managed provider 严格传播错误的策略集中在 adapter。
917. Provider current-provider scope 已改为 `proxy_core_adapter::provider_app_has_current_provider`：`ProviderService::current` 继续负责读取 settings/DB 的有效 current provider，additive app 不暴露 current provider 概念、switch app 使用 current provider 的策略集中在 adapter。
918. Provider key rename 策略已改为 `proxy_core_adapter::{provider_key_change_policy_issue,provider_key_change_policy_issue_message,ProviderKeyChangePolicyIssue}`：`ProviderService::update` 继续负责原 provider/目标 provider DB 查询、live config 存在性检查和落库，只有 additive app 支持改 key、OpenCode OMO/OMO Slim provider 禁止改 key 以及对应错误文案集中在 adapter。
919. Additive provider 新增时的 live 写入策略已改为 `proxy_core_adapter::{provider_additive_live_write_action,ProviderAdditiveLiveWriteAction}`：`ProviderService::add` 继续负责 DB 保存与 live 写入副作用，OpenCode OMO/OMO Slim 新增不自动启用、用户未选择写 live 时跳过、其他 additive provider 写入 live 的策略集中在 adapter。
920. Provider switch 分流策略已改为 `proxy_core_adapter::{provider_switch_dispatch,ProviderSwitchDispatch}`：`ProviderService::switch` 继续负责 provider 查询、proxy takeover lock/hot-switch 和 normal switch 副作用，OpenCode OMO/OMO Slim 与 Claude Desktop 直接走 normal switch、其他 provider 进入 takeover-aware 分支的策略集中在 adapter。
921. Provider normal switch 的状态推进策略已改为 `proxy_core_adapter::{provider_switch_backfill_source_id,provider_switch_should_mark_live_config_managed}`：`ProviderService::switch_normal` 继续负责 live config 读取、common config strip、DB 保存、live 回滚和 MCP sync，exclusive app 切换到不同 current provider 时才 backfill、DB-only additive provider 写入 live 后才标记为 managed 的判定集中在 adapter。
922. OpenCode OMO/OMO Slim normal switch 互斥策略已改为 `proxy_core_adapter::{provider_omo_switch_pair,ProviderOmoSwitchPair,ProviderOmoVariant}`：`ProviderService::switch_normal` 继续负责 current OMO DB 标记、OMO config 写入和旧 variant config 删除，`omo` 启用 standard 并禁用 slim、`omo-slim` 启用 slim 并禁用 standard、非 OpenCode/非 OMO provider 不进入互斥分支的策略集中在 adapter。
923. OpenCode OMO/OMO Slim category 解析策略已改为 `proxy_core_adapter::provider_omo_variant_for_category`：`ProviderService::update/delete/remove_from_live_config/switch_normal` 继续负责 DB current 判定、provider 保存、live config 写删和回滚，只有 OpenCode 的 `omo`/`omo-slim` category 会映射到 OMO variant、其他 app/category 走普通 additive provider 分支的判定集中在 adapter。
924. Provider legacy common-config 迁移跳过策略已改为 `proxy_core_adapter::{provider_supports_legacy_common_config_migration,should_skip_provider_legacy_common_config_migration}`：`ProviderService::migrate_legacy_common_config_usage*` 继续负责 DB snippet/provider 查询、common config 使用检测、settings mutation 和 DB 保存，additive app 不参与 legacy common-config 迁移、空 snippet 直接跳过的判定集中在 adapter。
925. Provider 单 app live sync 分流已复用 `proxy_core_adapter::{provider_live_sync_scope,ProviderLiveSyncScope}`：`ProviderService::sync_current_provider_for_app` 继续负责 current provider 查询、takeover backup/live 判定和 live 写入，additive app 直接同步全部 provider、switch app 进入 current-provider/takeover-aware 路径的分流与 `provider/live` 的全量同步策略保持同一 adapter 来源。
926. Provider settings 保存前旧模型字段归一化已改为 `proxy_core_adapter::normalize_provider_settings_for_storage`：`ProviderService::add/update` 继续负责 provider validation、common config storage normalization 和 DB/live 写入，Claude 旧 `ANTHROPIC_MODEL`/`ANTHROPIC_SMALL_FAST_MODEL` 归一化到默认模型字段、非 Claude app 不改 settings 的判定和 mutation 集中在 adapter，并与 default live import 复用同一 helper。
927. Provider 新增时的 `live_config_managed` 初始 marker 策略已改为 `proxy_core_adapter::provider_initial_live_config_managed_marker`：`ProviderService::add` 继续负责 provider meta mutation、DB 保存和 live 写入，只有 additive app 根据用户是否选择写入 live 设置初始 managed marker、switch app 不写该 marker 的判定集中在 adapter。
928. Provider switch 的 takeover lock 适用范围已改为 `proxy_core_adapter::provider_switch_requires_takeover_lock`：`ProviderService::switch` 继续负责实际 lock 获取、takeover 状态读取和 hot-switch/live 写入，Claude/Codex/Gemini 需要与 takeover toggle 串行化、Claude Desktop 与 additive app 不进入该 lock 的判定集中在 adapter。
929. Provider 在 takeover-owned live 状态下的同步目标已改为 `proxy_core_adapter::{provider_takeover_live_sync_target,ProviderTakeoverLiveSyncTarget}`：`ProviderService::update/sync_current_provider_for_app` 继续负责 backup/live ownership 检测、实际 live 写入和 backup 更新，Claude Desktop 在 takeover-owned 状态下仍写 live config、其他 app 更新 live backup 的目标选择集中在 adapter。
930. Provider 更新当前 Claude 且 proxy 正在运行时的 live refresh 判定已复用 `proxy_core_adapter::proxy_hot_switch_should_sync_claude_live_while_proxy_active`：`ProviderService::update` 继续负责 proxy running 查询与 `sync_claude_live_from_provider_while_proxy_active` 副作用，只有 Claude 且 live 已归 takeover 所有时才允许刷新 proxy-safe live 配置的 app/type 判定集中在 adapter。
931. Provider live 删除目标映射已改为 `proxy_core_adapter::{provider_live_removal_target,ProviderLiveRemovalTarget}`：`ProviderService::delete/remove_from_live_config/switch_normal` 继续负责 live 存在性检查、OMO current 处理、DB 删除/保存和失败回滚，OpenCode/OpenClaw/Hermes 分别映射到对应 live 删除后端、非 additive live config app 不映射删除目标的判定集中在 adapter；`remove_from_live_config` 只保留 OpenCode OMO 特例，其余 app 删除后端选择复用该 adapter 映射。
932. Provider additive update 分流已改为 `proxy_core_adapter::{provider_additive_update_route,ProviderAdditiveUpdateRoute}`：`ProviderService::update` 继续负责 OMO current 查询、OMO 文件写入/回滚、live config 存在性检查和 DB 保存，OpenCode OMO/OMO Slim 更新走 OMO variant 路径、普通 additive provider 走 live presence 路径、非 additive app 不进入该分支的判定集中在 adapter。
933. Provider 非 additive 删除的 current provider 使用判定已改为 `proxy_core_adapter::provider_delete_is_current_provider`：`ProviderService::delete` 继续负责读取 local settings 与 DB current provider、返回用户错误和执行 DB 删除，任一来源指向目标 provider 时阻止删除的判定集中在 adapter。
934. handler 请求体 JSON 解析与 `stream` 标志组合已收敛到 `proxy_core_adapter::{parse_json_proxy_request_body,parse_json_proxy_request_body_or_null}`：Axum body collection 仍在 host 层执行，但 strict JSON、Gemini empty-body null fallback 与 stream 判定不再散落在各协议 handler。
935. handler 的 Axum request body collection 与读取错误映射已收敛到 `proxy::response_adapter::collect_axum_request_body`：handler 不再直接调用 `BodyExt::collect` 或 `request_body_read_error_message`，HTTP transport 适配与 core 错误文案桥接保持单入口。
936. `RequestContext` 的 Claude API format provider fact 已改为 `proxy_core_adapter::provider_claude_api_format`：context 继续负责把 route metadata 和选中 provider 组合成响应格式判断，但不再直接依赖 `proxy::providers` adapter。
937. handler 的 Claude transform gate 与 Codex Responses→Chat gate 已改为 `proxy_core_adapter::{provider_needs_claude_transform,provider_should_convert_codex_responses_to_chat}`：handler 只负责选择响应处理分支，不再创建 provider adapter 或直接调用 Codex provider helper 做 provider/endpoint 判定。
938. upstream transport request 不再暴露拆开的 `ordered_headers` / `body` / `preserve_exact_header_case` request-parts facts；`ForwarderUpstreamTransportRequest` 直接携带 `ForwarderUpstreamRequestParts`，host forwarder 不再拆 request-source 产物后转手给 transport source。
939. 边界测试已对齐 source-owned runtime 结构：`RequestForwarder` 不再持有 managed-account runtime source，`ForwarderAuthSource` / `ForwarderRequestSource` 持有 managed-account runtime；`ActiveConnectionGuard` 的 lifecycle 校验也改为验证 adapter-owned guard 与 `ForwarderRuntimeStateSource`。
940. 托管账号 provider binding 到 account id 的选择规则已迁入 `proxy-core::managed_account_auth::{ManagedAccountBindingInput,managed_account_id_for_auth_provider}`；host `ProviderMeta` 不再持有解析方法，只负责把 `authBinding` / legacy `githubAccountId` 投影为 core-neutral 输入。
941. 托管账号 auth resolution 已改为消费 `proxy-core::managed_account_auth::resolve_managed_account_auth_for_binding_with_runtime_source`；host adapter 不再先拆 GitHub Copilot/Codex OAuth 两个 account id，只把 `ProviderMeta` binding 与 legacy GitHub account fact 交给 core。
942. Copilot dynamic endpoint、live model id 和 model vendor 三条 runtime lookup 已改为消费 `proxy-core::managed_account_auth::*_for_binding_with_runtime_source`；host adapter 不再为这些请求先计算 GitHub account id，只保留日志、base URL 写回和 body model 写回副作用。
943. Provider 是否使用托管账号认证的判定已改为消费 `proxy-core::managed_account_auth::provider_kind_uses_managed_account_auth`；host adapter 只负责从 `ProviderMeta` 与 `ANTHROPIC_BASE_URL` 提取输入，不再维护 GitHub Copilot/Codex OAuth/ChatGPT Codex endpoint 的第二份判断策略。
944. Provider 的 GitHub Copilot / Codex OAuth 分类 helper 已改为消费 `proxy-core::managed_account_auth::{provider_kind_is_github_copilot,provider_kind_is_codex_oauth}`；host adapter 继续负责 Provider 输入投影，但 provider 类型和 legacy base URL 命中的布尔规则不再留在 host。
945. GitHub Copilot upstream transport 判定已改为复用 `ProviderKind::from` 的 provider type alias 解析；`proxy-core::request_url::is_github_copilot_upstream` 不再只识别精确 `github_copilot` 字符串，host 继续只传递 meta provider type 与 base URL。
946. Claude transform gate 的 GitHub Copilot / Codex OAuth 判定已改为复用 `ProviderKind::needs_transform()`；host adapter 不再手写 provider kind match，只保留 api format fallback 判定。
947. Anthropic rectifier gate 的 Claude / ClaudeAuth 判定已改为复用 `ProviderKind::uses_anthropic_rectifiers()`；host adapter 不再维护 provider kind match，只负责 app/provider 输入投影。
948. Claude provider 的 GitHub Copilot / Codex OAuth runtime auth placeholder 选择已改为消费 `proxy-core::managed_account_auth::managed_provider_auth_info_for_provider_kind`；host adapter 不再手写 managed provider kind 到 `ProviderAuthStrategy` 的映射，只保留普通 Claude/Gemini/OpenRouter credential 提取。
949. Gemini provider 的 `ProviderKind` 到 `ProviderAuthStrategy` 映射已改为消费 `proxy-core::provider_auth::gemini_auth_strategy_for_provider_kind`；host adapter 继续负责 OAuth key shape 识别与 access token 解析，不再维护 GeminiCli/GoogleOAuth 的策略表。
950. Claude Anthropic auth key source 到 `ProviderAuthStrategy` 的映射已改为消费 `proxy-core::provider_auth::claude_anthropic_auth_strategy_for_key_source`；host adapter 不再维护 `ANTHROPIC_AUTH_TOKEN`/`ANTHROPIC_API_KEY` 到 ClaudeAuth/Anthropic strategy 的第二份表。
951. Claude provider kind 的静态 auth strategy 分支已改为消费 `proxy-core::provider_auth::claude_static_auth_strategy_for_provider_kind`；host adapter 不再维护 Gemini/OpenRouter/ClaudeAuth 到 Google/Bearer/ClaudeAuth strategy 的第二份表，GeminiCli 仍留在 host 解析 OAuth access token。
952. Claude static auth header kind 选择已改为消费 `proxy-core::request_headers::claude_auth_header_kind_for_provider_strategy`；host adapter 不再维护 `ProviderAuthStrategy` 到 `ClaudeAuthHeaderKind` 的第二份表，只保留 GitHub Copilot 需要 request id 的动态 header 构造。
953. Gemini provider auth header 构造已改为消费 `proxy-core::request_headers::build_gemini_provider_auth_headers`；host adapter 不再判断 `ProviderAuthStrategy::GoogleOAuth` 来选择 OAuth/API-key header，只传入已解析的 `ProviderAuthInfo`。
954. Codex API key 到 `ProviderAuthInfo::Bearer` 的包装已改为消费 `proxy-core::provider_auth::codex_auth_info_from_api_key`；host adapter 继续保留 Codex live/config/env 兼容读取，但不再直接写入 Codex Bearer strategy。
955. Codex provider auth header 构造已改为消费 `proxy-core::request_headers::build_codex_provider_auth_headers`；host adapter 不再直接读取 `ProviderAuthInfo.api_key` 拼接 Bearer header，只保留 String error adapter。
956. Gemini API key/OAuth credentials 到 `ProviderAuthInfo` 的包装已改为消费 `proxy-core::provider_auth::gemini_auth_info_from_api_key`；host adapter 继续负责 settings key 提取与 OAuth JSON 解析，但不再直接写入 Google/GoogleOAuth auth info 构造分支。
957. Claude provider auth header 分派已改为消费 `proxy-core::request_headers::build_claude_provider_auth_headers`；host adapter 只生成 Copilot request id 并传入编辑器/集成常量，不再维护静态 Claude header 与 GitHub Copilot header 的分支。
958. Claude static/GeminiCli auth info 构造已改为消费 `proxy-core::provider_auth::{claude_static_auth_info_from_key,claude_gemini_cli_auth_info_from_api_key}`；host adapter 继续负责 provider settings 提取、OAuth JSON 解析和带 provider id 的 warning 日志，但不再直接写入 Claude/GeminiCli `ProviderAuthInfo` 构造分支。
959. 托管账号 runtime token 到 `ProviderAuthInfo` 的包装已改为消费 `proxy-core::managed_account_auth::ManagedAccountAuthRuntime::provider_auth_info`；`proxy::managed_account_auth` 继续负责 Tauri state token 读取和错误日志，但不再直接维护 Copilot/Codex OAuth strategy 映射。
960. `AuthProvider` 返回的显式 header 到 `http::HeaderName/HeaderValue` 的校验和转换已改为消费 `proxy-core::request_headers::build_auth_provider_headers`；`ForwarderAuthSource` 不再维护 AuthProvider header name/value 的第二份错误 contract。
961. Codex OAuth session header 的发送 gate 已改为消费 `proxy-core::request_headers::build_codex_oauth_session_headers_for_forwarder`；`ForwarderAuthSource` 只传入 managed-auth session-header 决策、客户端 session 是否存在和 session id，不再维护二次发送条件。
962. Copilot auth override 的 prepared facts 到 `CopilotAuthHeaderOverrides` 的转换已改为消费 `proxy-core::request_headers::build_copilot_auth_header_overrides_for_forwarder`，subagent log gate 消费 `should_log_copilot_subagent_auth_override`；`ForwarderAuthSource` 不再维护 request-classification initiator 写入条件和 subagent header override 判定。
963. 默认 `AuthProvider` 的 route-context metadata envelope 已改为消费 `proxy-core::ports::auth_info_from_route_context`；`CcSwitchAuthProvider` 只提供 CC Switch source label，不再手写 `source/app/providerId/channelId` 元数据结构。
964. 默认 auth channel 的 app 到 interface fallback 已改为消费 `proxy-core::domain::default_auth_interface_for_app_kind`；`ForwarderAuthSource` 只做 `AppType -> AppKind` 投影，不再维护 Claude/Gemini/Codex/custom app 的默认 interface 表。
965. `ForwarderAuthSource` 的 AuthProvider channel context 投影已改为消费 `proxy-core::domain::auth_channel_spec_from_attempt`；host 只传 app/provider 基本事实与可选 resolved channel，不再手写 fallback provider channel、materialized channel model route 和 override 默认字段。
966. `ForwarderAuthSource` 的 AuthProvider request context 投影已改为消费 `proxy-core::domain::auth_provider_proxy_request_from_context`；host 只传 method/endpoint/body/header/channel facts，不再手写 requested model 提取、`ProxyBody` envelope 和 observed request context。
967. Copilot auth optimization 的 prepared facts 与 header override facts 投影已改为消费 `proxy-core::request_optimizer::prepare_copilot_auth_optimization_for_forwarder` / `PreparedCopilotAuthOptimization::as_header_override_facts`；`ForwarderAuthSource` 只保留 UUID fallback 生成器，不再手写 session/request/interaction id 组合和 facts 结构映射。
968. Copilot auth optimization 的 optional preparation gate 已改为消费 `proxy-core::request_optimizer::prepare_optional_copilot_auth_optimization_for_forwarder`；`ForwarderAuthSource` trait surface 的 maybe-input 直接使用 core input 类型，host 不再维护 classification optional gate 和 config flag projection。
969. 上游 auth headers 的 finalization 已改为消费 `proxy-core::request_optimizer::finalize_forwarder_auth_headers` 与 core `ForwarderAuthHeaders` contract；`ForwarderAuthSource` 只提供 base auth headers、Codex session/account facts 和可选 Copilot prepared facts，不再手写 Codex session header、Copilot override、upstream auth header 和 subagent log gate 的组合。
970. `AuthProvider` 显式 headers 与 provider-adapter fallback 的选择语义已改为消费 `proxy-core::request_headers::resolve_auth_provider_headers` / `AuthProviderHeaderResolution`；`ForwarderAuthSource` 不再把 `build_auth_provider_headers` 的 `Option` 形状当作 host-local 协议解释。
971. forwarder transform plan 到 protocol preparation 的投影已改为消费 `proxy-core::request_transport::forwarder_protocol_preparation_from_transform_plan` 与 core `ForwarderTransformPlan` / `ForwarderProtocolPreparation` contract；`ForwarderRequestSource` 只负责收集 provider/adapter 事实并产出 transform plan，不再手写 Claude transform 与 Codex chat enrichment 的互斥投影。
972. forwarder transform plan 的 facts 到 plan 投影已改为消费 `proxy-core::request_transport::forwarder_transform_plan_from_facts`；`ForwarderRequestSource` 只负责查询 Codex bridge gate、provider transform gate、adapter 是否 Claude 与 Claude API format facts，不再手写 transform plan 的 Claude/provider 分支组合。
973. `ForwarderAuthSource` 的 channel/request context 不再经由 adapter-local `forwarder_auth_channel_spec` / `forwarder_auth_proxy_request` wrapper 转手；默认 source 直接消费 `proxy-core::domain::auth_channel_spec_from_attempt` 与 `auth_provider_proxy_request_from_context`，host 只保留 `ForwardAttempt`/HTTP facts 到 core 参数的投影。
974. forwarder 请求体 model probe 已改为消费 `proxy-core::request_body::forwarder_request_body_model`；`ForwarderRequestSource` 不再维护 adapter-local JSON `model` 字段读取 helper，transform body 与 prepared body 的 outbound/logging model attribution 共用 core 规则。
975. media retry 的 gate 与图片替换计划已改为消费 `proxy-core::request_media::forwarder_media_retry_plan_from_facts`；`ForwarderRequestSource` 只负责把 `ProxyError` 分类成 unsupported-image fact 并补 provider/app 日志上下文，不再手写 retry gate、body image detection 和 marker replacement 组合。
976. 预防式 media fallback 的开关解析与 text-only provider 图片替换已改为消费 `proxy-core::request_media::apply_forwarder_media_prevention_from_facts`；`ForwarderRequestSource` 只保留 Codex app gate 与 provider/model 日志上下文，不再维护 provider settings 到 media replacement 的 host wrapper。
977. `CcSwitchForwarderResponseSource` 已删除私有 `upstream_error_body` / `upstream_error_response` 二次 helper；非成功上游响应在 `finalize_upstream_response` 内直接投影为 `ProxyError::UpstreamError`，外部替换 source 只面对完整响应 finalization 入口。
978. `CcSwitchForwarderRequestSource` 已删除私有 `apply_media_prevention` 方法；Claude body policy 与 Codex app media prevention 共用 adapter-level `apply_forwarder_media_prevention_with_log`，默认 request source 不再额外扩散媒体替换 helper 层。
979. `CcSwitchForwarderRuntimeStateSource` 已删除私有 rectifier/forward failure 日志行 helper；runtime state source 只通过 `log_*` 行为方法执行日志副作用，日志行投影改由 adapter-level helper 承接并由边界测试固定。
980. `RequestForwarder` 生产路径不再直接 import 或传递 `ForwarderAdapterHandle`；provider adapter trait 被包进 `ForwarderAdapterContext`，forwarder 只消费 context/facts，底层 `ProviderAdapter` 调用继续集中在 `proxy_core_adapter`。
981. `ForwarderRequestSource` trait 已删除 `adapter_facts` 转手方法；adapter facts 由 `ForwarderAdapterContext` 直接携带，request source surface 不再暴露纯 DTO 读取 helper。
982. `ForwarderAuthSource` 的 provider adapter fallback auth info/header 构造已改为通过 `ForwarderAdapterContext` 调用；auth source 不再直接调用 provider adapter，context 内部也已删除 `forwarder_provider_auth_info` / `forwarder_provider_auth_headers` 一跳 wrapper。
983. `ForwarderRequestSource::provider_url_facts` 已改为委托 `ForwarderAdapterContext` 读取 provider base URL/full-url/Copilot URL facts；request source 不再直接调用 provider adapter base URL helper。
984. `ForwarderRequestSource` 的 provider transform gate 与 transform request action 已改为委托 `ForwarderAdapterContext`；request source 不再直接调用 provider adapter transform helper。
985. `ForwarderRequestSource::plan_upstream_url` 的 provider URL 拼接回调已改为委托 `ForwarderAdapterContext`；request source 不再解包底层 `ProviderAdapter` 执行 upstream URL assembly。
986. `ForwarderRequestSource` trait 已删除 `provider_url_facts` 转手方法；provider base URL/full-url/Copilot URL facts 由 `RequestForwarder` 直接从 `ForwarderAdapterContext` 读取，外部替换 request source 不再需要实现纯 provider URL fact helper。
987. `ForwarderClaudeBodyPolicyInput` 已从携带 `ForwarderAdapterFacts` 改为携带 `ForwarderAdapterContext`；Claude body policy 的 adapter gate 由 request source 内部读取 context facts，`RequestForwarder` 少一处 adapter facts 转手。
988. `ForwarderTransformPlanInput` 已删除冗余 `ForwarderAdapterFacts` 字段；transform planning 直接从 `ForwarderAdapterContext` 读取 Claude adapter fact，`RequestForwarder` 不再为 transform plan 额外转手 adapter facts。
989. `ForwarderClaudeApiFormatInput` 已从携带 `ForwarderAdapterFacts` 改为携带 `ForwarderAdapterContext`；Claude API format runtime resolution 的 Claude-adapter gate 由 request source 内部读取 context facts。
990. `ForwarderMediaRetryPlanInput` 已从携带 `ForwarderAdapterFacts` 改为携带 `ForwarderAdapterContext`；media retry 的 adapter 名称判定由 request source 内部从 context facts 读取，测试也不再手工构造 adapter facts。
991. `ForwarderUpstreamRequestLogInput` 与 `ForwarderRequestPartsInput` 已从携带 `ForwarderAdapterFacts` 改为携带 `ForwarderAdapterContext`；上游请求日志 tag、Anthropic header gate 与 header-case policy 都在 request source 内部读取 context facts。
992. `ForwarderFailureDecision::Retryable` 已删除格式化错误消息 payload；ordinary forward failure 的 attempt failure、attempt-failed event 与 provider failure 状态写入都改为把 `ProxyError` 交给对应 source 内部投影消息，`RequestForwarder` 不再转手 retryable 错误字符串。
993. `ForwarderRectifierRetryFailureDecision::ProviderFailure` 已删除格式化错误消息 payload；rectifier retry provider failure 的 attempt failure 和 provider-scoped rectifier failure 状态写入都改为把 `ProxyError` 交给 runtime/attempt source 内部投影消息。
994. `record_forward_provider_failure_runtime_source`、`record_forward_provider_rectifier_retry_failure_runtime_source` 与 `record_forward_attempt_failure_runtime_source` 已从裸 `provider_name`/`rectifier_label`/`error_message` 参数收束为 `Provider`、`ForwarderRectifierRetryKind`、`ForwardAttempt` 与 `ProxyError` facts；默认 source 只转交结构化事实，错误展示字符串继续留在 runtime-source helper 内部生成。

因此，本分支目前已把主要转发入口（Claude Messages、Claude Desktop Messages、Codex Chat Completions、Codex Responses、Codex Responses Compact、Gemini Native）切到 `ProxyEngine`，并开始把管理查询类能力、Codex 客户端模型目录、Codex client raw catalog response contract、Codex client model catalog response helper、status response contract、runtime health/status response helper、Claude Desktop gateway 模型列表 envelope、legacy channel 投影构造、channel 写请求规范化、channel source label single-source、channel dry-run route input projection helper、channel dry-run circuit-open response mutation、channel dry-run circuit-open id helper、provider selection policy、circuit breaker config contract、circuit breaker key contract、response runtime policy、Codex proxy error code contract、proxy session request metadata、proxy error HTTP status contract、channel provider override plan、Codex Chat history SSE block inspection、Claude transform endpoint rewrite input projection、Bedrock provider env flag projection、request body filter report log message、response header log summary、prompt cache trace log message、Claude API format settings projection、Claude provider kind inference policy、Claude base URL settings projection、Claude auth key settings projection、Claude upstream URL builder、Codex upstream URL builder、Gemini upstream URL builder、Codex official client UA policy、Codex Bearer auth header helper、Gemini auth header helper、Claude static auth header helper、Copilot auth header builder、channel DAO row mapping single-source、channel record projection helper、provider/channel spec projection helper、channel record response contract、channel model response contract、channel create response helper、channel CRUD not-found/envelope helpers、channel list/delete response helper、handler ProxyEngine factory、管理 API input factory、app list response helper、app summary spec-count input factory、migration response input factory、provider summary input factory、provider list response helper、provider list ProviderSpec 投影、current route provider summary input factory、current route ProviderSpec 投影、current route target response contract、app path current-route/migration response helper、route group channel-spec source factory、route group app scope/response helper、app channel list/route response helper、托管账号上游安全保护、请求头 transport 策略、请求 header strip policy、provider auth header value validation、provider auth header helper facade deletion、Anthropic request header policy、Copilot fingerprint header policy、Codex OAuth session header 构造、ordered request header assembly、upstream auth header finalization、Copilot endpoint selection、forward failure log policy、provider failure retry classification、rectifier retry failover classification、thinking budget/signature rectifier、thinking rectifier result alias deletion、Claude reasoning vendor transform gates、Claude tool-thinking host wrapper deletion、DeepSeek thinking-disabled compatibility、DeepSeek thinking-disabled host wrapper deletion、Codex Chat reasoning profile inference、Codex OAuth Responses request contract、OpenAI Responses to Anthropic message assembly、Anthropic Messages to OpenAI Responses request assembly、Claude Responses prompt cache-key source policy、Copilot prompt-cache provider detection、Gemini OAuth key shape policy、Claude forward API format vendor policy、模型目录候选 URL 策略、模型目录响应解析、OpenAI-compatible 模型目录 transport 端口、Codex OAuth 模型目录解析与 transport 端口、Copilot live 模型目录解析、Copilot OAuth/GHES 域名规范化、Copilot 多账号复合 ID、Copilot model map host facade deletion、media fallback gate policy、media image downgrade/content detection、media unsupported marker re-export deletion、session/usage core type re-export deletion、model mapper string helper re-export deletion、model mapping settings projection、host model mapper facade deletion、request body private-field filtering、usage parser configuration、Codex Chat response item assembly、Codex Chat to Responses identity/usage/non-stream response mapping、Codex Responses to Chat request envelope/input traversal/reasoning carryover/reasoning option application、Codex Responses content to Chat content mapping、Codex Responses instructions/system message normalization、Codex Chat tool_search/custom call item assembly、Codex Responses tool definition to Chat tool mapping、Codex Responses function_call to Chat tool_call mapping、Codex Responses tool_choice to Chat function selector mapping、Codex Responses context-aware tool name resolution、Codex Responses tool output to Chat tool message mapping、Codex Chat tool_calls output traversal、Codex Chat assistant message/reasoning output item mapping、Codex Chat tool_call item id prefix policy、Codex tool context indexing/discovery、Codex Chat tool_call spec dispatch、Codex streaming direct core builder usage、Codex Chat SSE helper policy、Codex streaming canonical/think helper direct core usage、Codex Chat history helper direct core usage、host canonical JSON facade deletion、host SSE facade deletion、host Gemini URL facade deletion、host body filter facade deletion、host log code facade deletion、host handler config facade deletion、host usage parser facade deletion、host response handler compatibility deletion、usage cost calculator core migration、usage request log projection、channel authProfileRef resolution、route plan provider matching、forward runtime timeout/retry projection、channel-key settings auth patch、host event envelope re-export deletion、provider Claude transform gate re-export deletion、host Copilot optimizer wrapper deletion、transform core helper re-export deletion、Bedrock optimizer gate/thinking/cache injection policy、session identity extraction、proxy log code contract、usage request-id/model fallback policy、success/error usage record construction、transformed response usage attribution、transformed response usage record construction、streaming response usage record construction、streaming usage fallback model policy、non-streaming response usage record construction、SSE aggregate fallback diagnostics、非流式 JSON/SSE 解析兜底策略、Codex Chat 上游错误体归一化、Codex proxy error body 分类、转换响应 header 重建策略、转换后 JSON/SSE neutral response 构造、host response transport/error adapter、response parse error adapter、Copilot optimizer session/deterministic ID/fallback/warmup/classification policy、Copilot warmup model body override、Copilot thinking-strip/orphan-sanitize/tool-result-merge mutation、Copilot optimizer production call site、upstream send transport policy、global proxy loopback recursion policy、proxy event payload contracts、上游请求体准备/发送策略、上游请求 transport policy、上游请求 URL/query helper、endpoint rewrite policy、route hint inference、Provider env 模型映射、Gemini provider settings projection、canonical JSON/tool argument 规范化、Gemini Native streaming parts/snapshot helper、Gemini Native streaming SSE payload contract、Gemini Native streaming parsed-chunk 状态机、Gemini Native streaming SSE data parse、Gemini Native streaming byte/block state、Gemini Native streaming SSE byte encoding、Gemini Native streaming shadow persistence wrapper、Gemini Native streaming async transport wrapper、model catalog reqwest host adapter、management app model catalog request factory、management channel/group list request factory、management app channel route-filter request factory、management channel path request factory、management app path request factory、management route resolve request factory、provider/current route runtime smoke 脱敏边界验证、legacy provider settings host adapter 投影、legacy channel record host adapter 投影、authProfileRef 格式解析、migration response source input 构造、channel reachability 状态枚举、stream check 配置契约、stream check 结果契约、stream check reachability 投影、stream-check base URL 解析策略、stream check 日志状态 contract、app kind 解析策略、channel health 状态推进规则、channel health unknown 状态 contract、channel key 写请求校验/传递口径、channel model replace 请求校验/传递口径、channel patch 请求字段校验、channel patch 请求归一化、channel create/model replace 写请求归一化、channel key write/patch 请求归一化、provider health 阈值状态推进规则、usage 计费配置校验、app proxy_config 默认值策略、official provider 热切换阻断规则、global proxy_config 默认 DTO、failover 恢复切回判定、resolved channel model override log context、response SSE content-type 判定、channel test runtime smoke、proxy 默认监听端口/地址常量、provider list ProviderSpec 投影 和 current route ProviderSpec 投影、legacy provider settings host adapter 投影 和 legacy channel record host adapter 投影 收敛到 core 可复用接口/验证面。实际模型目录 HTTP client、数据库、Tauri runtime、ProviderRouter 与 reqwest 执行仍由 host adapter 执行；下一阶段需要基于已固化的 `proxy_core::api` 集成面继续稳定 host-only 端口边界，并补充完整 Tauri runtime smoke 验证。

本轮继续把 channel-key 缺失/禁用 key 的 runtime 错误文案收敛为 core helper，避免 host 在 auth profile 迁移路径中维护第二份错误 contract。

本轮还把 client model catalog 的 app source 选择收敛到 adapter，后续再把 ModelCatalogProvider 端口实现本身迁为 adapter-owned。

本轮继续把 `/proxy/v1/apps/{app}/models` 的模型目录 envelope 收敛到 `proxy-core::management_api`：新增 `AppModelCatalogSource` 与 `AppModelCatalogRequest::response(_from_source)`，`ProxyEngine::list_model_catalog_for_request` 只负责读取 route-visible models facts，app/group/interface JSON envelope 由 management contract 统一生成，并经 `api::prelude` 暴露给外部中转集成。

forwarder 的 Claude 请求阶段 normalization 与 transform 一跳 wrapper `forwarder_claude_normalize_anthropic_messages` / `forwarder_claude_transform_request_for_api_format` 已删除；request/protocol source 直接复用 provider 级 Claude helper。

forwarder adapter facts 的 Claude 名称判定不再保留 `provider_adapter_name_is_claude` 单行 helper；`ForwarderAdapterFacts::from_adapter` 在唯一语义归属点直接从 adapter name 投影 `is_claude_adapter`。

channel-key auth profile 的 test-only DB convenience helper `apply_channel_auth_profile_providers_from_db` 与 borrowed runtime source 已删除；host 回归测试和 forward runtime 均显式消费 `ChannelKeyRuntimeSource` 注入契约。

Codex forwarder media-prevention 的 app gate 已下沉到 `proxy-core::request_media::should_apply_forwarder_media_prevention_for_app`；adapter 只把 `AppType` 投影成 core `AppKind` 并执行 request source 副作用。

本轮继续把 Gemini live settings 的 env/config 组装与 env-only backup JSON contract 收敛到 `proxy-core::ports::{gemini_live_settings_from_env_json_and_config,gemini_live_backup_from_effective_settings}`；host adapter 只 re-export core helper 供 live write/backup 流程使用。

本轮继续把 Gemini live provider `config` 对象选择与 settings.json 顶层 merge 写入 contract 收敛到 `proxy-core::ports::{gemini_live_config_object_from_settings,gemini_live_settings_to_write}`；host adapter 只负责从 `Provider.settings_config` 投影输入。

本轮继续删除 proxy event/server event payload 的 adapter 私有 passthrough helper；`proxy_core_adapter` 的消息构造直接消费 `proxy-core::events::{build_proxy_events_connected_payload,build_proxy_events_lagged_payload,build_server_started_event_payload,build_server_stopped_event_payload}`，payload shape 继续由 core 单测覆盖。

本轮继续把 Codex provider `config` 文本读取、config TOML `wire_api`/`model` 投影与 active provider `base_url` 匹配收敛到 `proxy-core::ports::{codex_config_text_from_settings,codex_wire_api_from_config_toml,codex_model_from_config_toml,codex_config_has_base_url_matching}`；host adapter 不再维护 TOML value 解析、live takeover base_url 匹配或 legacy projection duplicate extractor。
forwarder 的 Claude/ClaudeAuth rectifier gate 一跳 wrapper `forwarder_uses_anthropic_rectifiers` 已删除；request source 直接复用 provider 级 rectifier 判定，`forwarder.rs` 仍只通过 source 获取 rectifier gate。
Codex Responses→Chat 上游模型覆写与 reasoning options 解析已由 forwarder request source 直接复用 provider 级 adapter API；此前的 `forwarder_apply_codex_chat_upstream_model` / `forwarder_codex_chat_reasoning_options` 一跳 wrapper 已删除。
forwarder 的 Codex OAuth header-casing fact 一跳 wrapper `forwarder_is_codex_oauth_provider` 已删除；request source 直接复用 provider 级 Codex OAuth 判定，`forwarder.rs` 仍只通过 source 获取 header policy。
forwarder 的 Bedrock pre-send optimizer provider env fact 一跳 wrapper `forwarder_bedrock_env_flag` 已删除；request source 直接复用 provider 级 env 投影 helper，`forwarder.rs` 仍只通过 source 执行 optimizer gate。
forwarder 的 custom User-Agent header provider fact 一跳 wrapper `forwarder_custom_user_agent_header` 已删除；request source 直接复用 provider 级 UA 投影 helper，`forwarder.rs` 仍只通过 source 获取 upstream header parts。
forwarder provider URL facts 内部的 full URL 与 GitHub Copilot upstream 一跳 wrapper 已删除；adapter context 直接复用 `provider_is_full_url` / `provider_is_github_copilot_upstream`，`forwarder.rs` 仍只消费注入后的 URL facts。
本轮继续把 forwarder 的 media prevention text-only provider 图片替换 fact 收敛到 `proxy-core::request_media::apply_forwarder_media_prevention_from_facts`；media 预防式降级不再直接消费 provider 级模型能力投影 helper。
forwarder provider adapter transform gate/request 的一跳 wrapper `forwarder_provider_transform_required` / `forwarder_provider_transform_request` 已删除；adapter context 内部直接调用 trait，`forwarder.rs` 仍只通过 request source 执行 transform 策略。
本轮继续把 usage sink 的计费配置 lookup 输入收敛到 adapter，host 不再直接拆 `UsageRecord` 的 app/provider 字段。
本轮还把 core event 到 host event bus 的投影+分发入口收敛到 adapter，后续再把 event sink 端口实现本身收敛为 adapter-owned source wrapper。
本轮继续把 `CcSwitchForwardPipeline` 本身迁为 adapter-owned optional runtime wrapper，runtime 缺失判断和 host forward runtime 调度都在 adapter wrapper 内完成；host services 只持有 `CcSwitchForwardPipeline<CcSwitchProxyRuntime>` 并保留 `HostForwardRuntime for CcSwitchProxyRuntime` 作为 DB/router/Tauri 资源装配点。
本轮继续把 `CcSwitchProxyServices` 迁为 adapter-owned generic `ProxyServices` 容器；`proxy_core_host` 只保留 `CcSwitchProxyServices = CcSwitchProxyServices<CcSwitchProxyRuntime>` type alias 和 `ProxyServiceRuntimeResources` 实现，用于暴露 DB、ProviderRouter、current providers 与 event bus。
本轮继续把 response processor 非流式 usage 的 provider 缺失判定和 usage record 输入组装收敛到 adapter wrapper，response processor 只负责读取响应体、记录日志和触发 `UsageSink`。
本轮继续把 response processor 流式 usage 的 provider facts 选择、缺失 warning 和 usage record 输入组装收敛到 adapter wrapper，response processor 只保留 usage collector 创建入口和响应 transport 编排。
本轮继续把 response processor 的 SSE usage collector、finish guard、SSE passthrough scanner 和流式 first-byte/idle timeout loop 迁入 adapter-owned `create_logged_passthrough_stream`，host response processor/handlers 只传入 stream、tag、collector、timeout config 与 active-connection guard。
本轮继续把 response processor 的 route/channel usage 归因合并收敛到 adapter wrapper，streaming/non-streaming 输出的 `UsageRecord` 已在进入 `UsageSink` 前带好 route context。
本轮继续把 response processor 的 `UsageSink` 调用、usage debug 日志、落库失败 warning 和默认落库 task 调度收敛到 adapter wrapper，response processor 不再维护本地 `tokio::spawn` usage 落库 helper。
本轮继续把 usage sink bridge 的 `UsageSink` 调用、落库失败 warning 和 failure-context-aware task 调度收敛到 adapter wrapper，bridge 只保留 transformed/forward-error usage 的入口编排。
本轮继续把 usage sink bridge 的 forward-error/transformed response usage provider 选择、缺失 warning、record 组装和 route context 合并收敛到 adapter wrapper，bridge 只保留 logging 开关读取和 SSE collector 生命周期入口。
本轮继续把 response processor 的 raw response body 接收日志、content-encoding decode 调用和 decode status 日志投影收敛到 adapter-owned helper；host `read_decoded_body` 只保留 `ProxyResponse` body 读取、timeout/`ProxyError` 映射和 transport 返回值拆包。
本轮继续把 response processor 的 streaming response header receive log 与 content-encoding warning 收敛到 adapter-owned helper；host `handle_streaming` 只保留 `ProxyResponse` status/header/stream 拆包、collector 创建和 passthrough response 适配。
本轮继续把 response processor 的非流式 decoded body content debug 日志收敛到 adapter-owned helper；host `handle_non_streaming` 只传入 body bytes 与 tag，不再负责日志字符串投影。
本轮继续把 response processor 的 streaming usage collector 创建、provider facts 选择、missing-usage 诊断日志和 usage record 组装收敛到 adapter-owned helper；host `handle_streaming` 只传入 request/route/parser facts 与 services。
本轮继续把 response processor 的非流式 usage JSON parse、usage log event 输出、disabled logging 诊断和 usage record task 调度收敛到 adapter-owned helper；host `handle_non_streaming` 只传入 body/request/route/parser facts 与 services，并保留 `ProxyError::ConfigError` 映射。
本轮继续把 response processor 的 `ProxyConfig.enable_logging` 读取和读取失败 fallback 策略收敛到 adapter-owned helper；host 只把 config lock 作为事实传入。
本轮继续把 usage sink bridge 的 `ProxyConfig.enable_logging` 读取和读取失败 fallback 策略收敛到 adapter-owned helper；bridge 只把 config lock 作为事实传入。
本轮继续把 usage sink bridge 的 forward-error usage、转换后非流 usage 和转换后流式 usage collector 组装/调度收敛到 adapter-owned helpers；bridge 只保留 `ProxyError` 到 status/message 的 host 映射和 ctx/state facts 传递。
本轮继续把 forward runtime 的 route plan attempt 构造入口收敛到 adapter；auth profile 的 DB key 注入 wrapper 也已收敛到 adapter，host forward runtime 只调用统一 helper。
本轮还把 forward runtime 的 auth profile action 应用循环收敛到 adapter，host 不再维护 channel-key DB lookup 闭包。
本轮继续把 forward runtime 的 current-provider 来源组合收敛到 adapter，host forward runtime 不再读取 settings 或手写 DB fallback 闭包。
本轮还把 forward runtime 的 required attempts 空结果错误判断收敛到 adapter，host 只接收可执行 attempts 或 core error。
本轮还把 `ProxyRequest` 到 host forward 输入的投影收敛到 adapter，host forward runtime 不再直接处理 app kind 解析、body JSON 转换和 session id 抽取。
本轮继续把 `AppProxyConfig` 到 forwarder timeout/retry 参数的投影收敛到 adapter，host 不再直接展开 response runtime policy。
本轮还把 forwarder runtime 的 app config 与 rectifier/optimizer 配置 DB 读取及组合入口收敛到 adapter，host forward runtime 只保留 `RequestForwarder` 运行态装配。
本轮继续把 forward runtime 的 provider DB 读取、plan provider 过滤、required attempts 构造与 auth profile DB 注入统一收敛到 adapter wrapper，host forward runtime 只接收最终 attempts。
本轮继续把 `RequestForwarder::new_preplanned` 构造、预规划 attempts 执行和 forward error 到 core error 的映射收敛到 adapter runtime launcher，host forward runtime 只传入资源包和已解析 runtime facts。
本轮继续把 `ProxyRequest` 解析、forwarder runtime config/current-provider/attempt source 读取和 preplanned launcher 串联为单一 adapter forward runtime 入口，host forward runtime 只保留资源包委托。
本轮继续把 `CcSwitchConfigSource` 的 global/app/summary/runtime 配置 DB 读取和 core DTO 投影收敛到 adapter-owned source wrapper，host services 只装配 source。
本轮继续把 `CcSwitchProviderSource` 的 provider list/get/current-provider DB 读取和 `ProviderSpec` 投影收敛到 adapter-owned source wrapper。
本轮继续把 `CcSwitchChannelSource` 的 channel spec list/get 读取与 `ChannelSpec` 投影收敛到 adapter-owned source wrapper。
本轮继续把 `CcSwitchChannelSource` 的 channel record create/get/update/delete DB 操作与 `ChannelRecord` 投影收敛到 adapter-owned source wrapper。
本轮继续把 `CcSwitchChannelSource` 的 channel key/model 子资源 DB 操作与 `ChannelKeyRecord`/`ChannelModelRecord` 投影收敛到 adapter-owned source wrapper。
本轮补齐 `proxy-core::api::prelude` 的外部接入烟测所需 channel 构造 contract；独立 integration test 只经 public prelude 构造 `ProxyEngine`、实现 `ProxyServices` 并调用 `handle`，锁住外部 host 直接集成中转模块的最小可用路径。
本轮加固 `proxy_core_host.rs` 兼容壳边界：文件在 `mod tests` 之前只允许 `#[cfg(test)]` 保护的 import/re-export，防止旧 host 模块重新承载生产 services/runtime 装配。
本轮删除 adapter 内部 `proxy_engine_from_services` 一跳构造 facade；`ProxyState::proxy_engine` 在 adapter 所有权边界内直接调用 `ProxyEngine::new`，保留“生产 host 只能经 adapter 构造 engine”的边界测试。
本轮继续删除 `ProxyState` 对 `AppHandle` 和 `FailoverSwitchManager` 的重复保留字段；这些 host 资源只由 `ManagedAccountRuntimeSource` 与 `FailoverSwitchScheduler` 注入 source 持有，`ProxyState` 只保留 HTTP server 实际读写的 runtime state surface。
本轮继续删除 `ToProxyCore*` 单 impl DTO trait facade；Provider/channel/model 的 host record 到 core DTO 投影统一改为 adapter 直接函数，避免为独立中转模块暴露不必要的扩展 trait 表面。
本轮继续把 `ForwardAttempt::from_provider` 收成 test-only 构造器；生产 `ForwardAttempt` 只能从 `ProxyEngine` route selection 转成 channel-aware attempt，避免 provider-only fallback 构造路径回流。
本轮继续删除 `ForwardError` 中未被 core error bridge 使用的 host `Provider` payload，并移除 `RequestForwarder` 预规划生产入口上的过时 dead-code allowance；forwarder 错误 surface 只保留 neutral `ProxyError` 分类事实。
本轮继续把 `CcSwitchProxyServices::new` / `with_event_bus`、`DefaultRuntimeStatusSource` 与 `CcSwitchForwardPipeline::without_runtime` 收成 test-only；生产服务容器只暴露 runtime-backed `with_runtime` 构造路径。
本轮继续删除 `proxy/error_mapper.rs` 中 `map_proxy_error_to_status` 与 `get_error_message` 两个纯策略 facade；错误状态码和展示文案测试直接调用 adapter 暴露的 core contract，error mapper 只保留桥接与响应构造职责。
本轮继续把 `CcSwitchChannelSource` 的 route/materialized channel record list 读取与 `ChannelRecord` 投影收敛到 adapter-owned source wrapper。
本轮继续把 `CcSwitchChannelSource` 的 legacy channel migration preview/materialize DB 操作与 response input 投影收敛到 adapter-owned source wrapper，host services 只装配 channel source。
本轮继续把 `CcSwitchRoutePolicySource` 的 failover queue DB 读取与 `RoutePolicy` 投影迁入 adapter-owned source，host services 只装配 source。
本轮继续把 channel health attempt DB 更新、reset app lookup、breaker reset 与 `ChannelHealthReset` 投影迁入 adapter-owned `CcSwitchChannelHealthStore`，host services 只装配 store。
本轮继续把 `CcSwitchChannelReachabilityProbe` 的 probe request app/provider 投影、provider/config DB 读取、stream-check 调用与 reachability 结果投影收敛到 adapter-owned source wrapper，host services 只装配 probe。
本轮继续把 `CcSwitchModelCatalogProvider` 的 provider catalog DB 读取、client catalog source 选择、Claude Desktop provider route 选择与 model route 投影收敛到 adapter-owned source wrapper，host services 只装配 provider。
本轮继续把 `CcSwitchUsageSink` 的 usage pricing lookup、pricing model 解析、request log 投影、缺价告警与 usage log 写入收敛到 adapter-owned source wrapper，host services 只装配 sink。
本轮继续把 `CcSwitchProviderSource` 的 route candidate provider router selection 与 provider-id 投影收敛到 adapter-owned source wrapper。
本轮继续把 `CcSwitchRouteResolver` 的 core route plan 委托、management dry-run router 调用与错误映射迁入 adapter-owned resolver，host services 只装配 resolver。
本轮继续把 `CcSwitchProviderSource` 的 active route runtime map lookup 收敛到 adapter-owned source wrapper，`proxy_core_host` 不再直接读取 `current_providers`。
本轮继续把 `ProxyServer` 的 circuit breaker runtime config 更新与 provider breaker reset 副作用收敛到 adapter runtime wrapper，server 生产方法不再直接调用 ProviderRouter circuit runtime API。
本轮继续把 `ProxyServer` 的 started/stopped runtime status mutation、uptime/active target status projection 与 active target map 更新收敛到 adapter runtime wrapper，server 生产方法只保留监听生命周期和 transport 编排。
本轮继续把 `ProxyServer` 的 server started/stopped lifecycle event 分发收敛到 adapter runtime wrapper，server 生产方法只传入监听事实，不再直接构造 event bus message 或 emit payload。
本轮继续把 `ProxyServer` 启动后同步全局 HTTP client 递归保护监听端口的副作用收敛到 adapter runtime wrapper，server 生产方法不再直接调用 `proxy::http_client::set_proxy_port`。
本轮继续把 `RequestForwarder` 的请求运行态 status 更新、active target map 写入和 request/attempt/route event 分发收敛到 adapter runtime wrapper，forwarder 生产方法只保留请求生命周期编排与上游发送。
本轮继续把 `RequestForwarder` 的 attempt allow、provider/channel 成功失败健康记录和 neutral half-open permit 释放收敛到 adapter runtime wrapper，forwarder 不再直接分叉调用 ProviderRouter 的 provider/channel runtime API。
本轮还把 ConfigSource 的 app 配置 wrapper 收敛到 adapter，host 不再显式读取 settings current-provider 后再拼 `ProxyAppConfig`。
本轮继续把 channel-key DB record 到 runtime key value 的字段投影收敛到 adapter，host auth-profile 路径不再直接查询或投影 channel-key。
本轮也把 management handler 中 provider 到 core spec 的直接 trait 调用收敛为 adapter helper，provider list/current route handler 不再暴露投影细节。
本轮继续把 channel/group 管理列表的 response source 组装收敛到 adapter，list/group handlers 不再直接拼 record-to-source 投影。
本轮还把 channel create/get/update 的 record response source 组装收敛到 adapter，CRUD handler 不再直接暴露 channel record 投影细节。
本轮继续把 channel keys/models 管理路由的 source 组装收敛到 adapter，key/model handlers 不再直接暴露 DAO record 到 core DTO 的投影细节。
本轮还把 channel migration preview/materialize 的 DAO result 到 response source 组装收敛到 adapter，migration handlers 不再拆 preview/materialize result 字段。
本轮继续把旧 provider 主地址和 `provider_endpoints` 到 channel 的兼容迁移规划收敛到 `proxy-core::build_legacy_channel_migration_plan`，DAO 不再手写 endpoint 排序、normalized base URL 去重、priority/interface/model route 推导。
本轮继续把 channel test 的 record-to-plan preflight 投影收敛到 adapter，test handler 只保留 DB/provider/stream-check 副作用。
本轮继续把 channel delete/key delete 的 source 组装收敛到 adapter，delete handlers 不再直接暴露 delete response source DTO。
本轮继续把 channel health reset 的 source 组装收敛到 adapter，reset handler 只保留 path 解析和 engine reset 调用。
本轮继续把 health/status/app list 的基础 source 组装收敛到 adapter，基础管理 handlers 不再直接暴露 core source DTO。
本轮继续把 provider list/current route 的 source 组装收敛到 adapter，provider handlers 不再直接暴露 ProviderSpec/summary/source DTO 投影细节。
本轮补齐 `proxy-core::api::prelude` 的 channel management 契约出口：外部中转集成只依赖 prelude 即可拿到 channel 写入、模型/key 子资源、route dry-run、list/status/test 等对外 DTO，避免直接耦合 core 内部 management/ports 模块布局。
本轮继续补齐 `proxy-core::api::prelude` 的 model catalog 契约出口：外部中转集成可直接复用 `ModelCatalog`、`ClientModelCatalogResponse`、`FetchedModel` 与 route-visible `RoutableModelList`，模型目录接口不再要求调用方知道 model_catalog/domain/ports 的内部拆分。
本轮继续补齐 `proxy-core::api::prelude` 的 host service 契约出口：外部中转宿主可只依赖 prelude 实现 `ProxyServices` 全套端口，包括 config/provider/channel/route/health/reachability/auth/model catalog/usage/event/forward pipeline，不再需要直接引用 ports/domain 中的零散返回类型或 futures `BoxFuture`。
本轮继续把 host 生产路径的 `ProxyEngine` 构造入口收敛到 adapter，server/forwarder 不再直接 new core engine。
本轮同时补充 `proxy_core_boundary` 护栏，防止生产路径重新绕过 adapter 直接构造 `ProxyEngine`。
本轮继续删除 forwarder 旧 self-planning 兼容路径，route planning 只保留在 `ProxyEngine`/host pipeline 侧，`RequestForwarder` 只执行预规划 attempts。
本轮继续瘦身 `RequestContext`，删除旧 forwarder planning 遗留的 provider chain/current-provider/config 字段和解析。
本轮补充 forwarder self-planning 边界测试，防止生产路径重新绕过 `ProxyEngine`/`ForwardPipeline` 构建 attempts。
本轮继续把 `RequestContext` 中 core app config raw 到 host app config 的投影收敛到 adapter，避免 handler context 直接维护 serde 细节。
本轮进一步瘦身 `RequestContext`，只保存 response runtime policy，不再把完整 app config DTO 带入请求生命周期。
本轮继续把 `ProxyResult` 到 `RequestContext` route/provider update 的投影收敛到 adapter，context 不再直接调用 route attempt 映射 helper。
本轮继续移除 `RequestContext::new` 的重复 provider 预选，provider 改为 route result 后回填；选路失败时错误 usage 使用 `unselected:<app>` fallback，Codex 错误体使用 app tag fallback。
本轮继续把 `RequestContext::new` 的 host `AppType` 到 core `AppKind` 投影收敛到 adapter helper，context 只负责加载 app config 和请求生命周期事实。
本轮继续把 `RequestContext::apply_proxy_result` 的 selected-route provider lookup、缺失文案和 route update 组装收敛到 adapter source wrapper，context 只保留 DB 查询闭包和字段回填。
本轮补充 `RequestContext` provider preselect 边界测试，防止后续把 provider router 选路重新塞回 handler context。
本轮也把 forward runtime 当前 provider 来源优先级收敛为 core helper，host 不再手写 settings/DB fallback 选择策略。
本轮进一步把 forward pipeline 和 route plan host-provider 匹配失败的固定错误文案收敛为 core helper，host 只保留错误类型包装。
本轮还把 unsupported app kind parse 错误文案迁入 core，减少 host 中零散的 Config 文案拼接。
本轮继续把 host adapter 的 context/error 通用拼接格式迁入 core errors API，host 只保留错误类别映射。
本轮继续把 `ProxyError` 到 Codex 错误响应 envelope 的 context/code/status 组装收敛到 adapter wrapper，error mapper 只保留 host error facts 归类。
本轮继续把 Claude/Codex/Gemini provider adapter 的必填 base_url 提取和缺失错误文案收敛到 adapter helper，provider adapter 只保留现有 `ProxyError` 映射。
本轮继续把 Gemini provider adapter 的 auth strategy/auth info 构造收敛到 adapter helper，Gemini adapter 不再直接解析 OAuth key 或构造 `ProviderAuthInfo`。
本轮继续把 Codex provider adapter 的 Bearer auth info 构造收敛到 adapter helper，Codex adapter 不再保留私有 API key 提取和 `ProviderAuthInfo` 包装逻辑。
本轮继续把 Claude provider adapter 的 auth key source 日志、占位 token、Gemini OAuth 降级和 `ProviderAuthInfo` 构造收敛到 adapter helper，Claude adapter 的 `extract_auth` 只保留 helper 调用。
本轮继续把 Codex/Gemini provider adapter 的 auth header 构造和 `AuthError` 文案包装收敛到 adapter helper，两个简单 provider adapter 的 `get_auth_headers` 只保留 helper 调用。
本轮继续把 Claude provider adapter 的 auth strategy 到 header kind 映射、Copilot 指纹 header 构造和 `AuthError` 文案包装收敛到 adapter helper，Claude adapter 的 `get_auth_headers` 只保留 helper 调用。
本轮继续把 Claude/Codex/Gemini provider adapter 的 upstream URL 构造收敛到 provider-scoped adapter helper，三个 provider adapter 的 `build_url` 只保留统一 helper 调用。
本轮继续把 Claude provider adapter 的 transform 判定策略收敛到 adapter helper，`needs_transform` 不再直接维护 Copilot/CodexOAuth 特例或 `api_format` 判定。
本轮也把 channel authProfileRef 缺失 provider warning 的 optional fallback 收敛到 core，host 不再维护空 ref 兜底逻辑。
本轮继续把 current provider 来源优先级的 option/string 两种 contract 统一到 core，config source 和 forward runtime 复用同一选择规则。
本轮还把 Codex client model catalog raw JSON 解析失败 fallback 收敛到 core，adapter-owned model catalog provider 只保留路径选择与文件读取。
本轮继续把 legacy provider settings JSON 的 config/env/modelCatalog 形状投影收敛到 core，host adapter 只保留 Provider/meta 到 projection input 的装配。
本轮还把 host `ProxyResponse` 到 core response 的 runtime bridge 收敛到 adapter，host forward result 投影不再维护响应体转换分支。
本轮继续把 route plan 的 host provider 匹配过滤收敛到 adapter，host forward runtime 不再直接展开 `RoutePlanProviderMatch`。
本轮进一步把 host `ForwardResult` 到 core `ProxyResult` 的整体投影收敛到 adapter，forward runtime 不再拆 response/provider/channel 结果字段。
本轮也把 `AppKind` 到 host `AppType` 的解析/错误包装收敛到 adapter，host config/provider/runtime source 不再直接执行 app type parse。
本轮继续把 channel-key auth profile 的错误包装和 provider settings patch 收敛到 adapter，host auth-profile 应用逻辑只保留 DB key 查询。
本轮还把 host `AppError` 到 core error 的 Config/Internal 包装收敛到 adapter，host source/sink 只保留错误发生位置。
本轮继续把 current-provider 的 DB fallback 查询判定收敛到 core，forward runtime 只负责按 core 决策读取 DB 当前 provider。
本轮也把 forward pipeline 缺 runtime 的 Unsupported 错误包装收敛到 adapter，host forward pipeline 只负责 runtime 存取。
本轮继续把 channel authProfileRef 到 provider/channel-key/ignore 的动作判定收敛到 adapter，host auth-profile loop 只保留持久化查询和 attempt 变更。
本轮也把 route plan 无匹配 host provider attempts 的 Unavailable 错误包装收敛到 adapter，forward runtime 不再直接构造 core error variant。
本轮继续把 provider config auth profile 的 metadata source label 收敛到 adapter，后续再把 AuthProvider 端口实现本身收敛为 adapter-owned source wrapper。
本轮还把 Codex client model catalog 的 active config 读取与 stale guard fallback 收敛到 adapter，`proxy_core_host` 不再维护 catalog 文件解析 helper。
本轮继续把 host Provider 到 provider model catalog 的 settings 投影收敛到 adapter，`proxy_core_host` 不再直接拆 provider 字段。
本轮也把 app config source 的当前 provider settings 查询和 `ProxyAppConfig` parts 组装收敛到 adapter-owned source wrapper。
本轮继续把 runtime config source 的默认 privacy-filter flag 包装收敛到 adapter，`proxy_core_host` 不再维护该固定参数。
本轮还把 ProviderSource 的 provider 列表/单条投影收敛到 adapter，host ProviderSource 只保留 DB provider 查询。
本轮继续把 ChannelSource 的 channel 列表/单条投影收敛到 adapter，host ChannelSource 只保留 DB/router channel 查询。
本轮也把 ChannelHealthStore 的 attempt 写库参数投影和 DB 写入调用收敛到 adapter-owned health store，host services 只装配 store。
本轮继续把 `ProxyCoreEvent` 到 host event bus 的 name/payload 投影收敛到 adapter。
本轮继续把 `CcSwitchEventSink` 的 `ProxyEventBus` 分发副作用和 optional event bus 判断收敛到 adapter-owned source wrapper，host services 只负责注入事件总线。
本轮还把 RoutePolicySource 的 failover queue 到 optional route policy 包装和 DB 查询收敛到 adapter-owned source，host services 只装配 source。
本轮继续把 ChannelHealthStore reset 的 app lookup 校验、router reset 和 reset fact 投影收敛到 adapter-owned health store，host services 不再保留 reset 桥接实现。
本轮继续把 response processor 的 provider/app usage facts 投影收敛到 adapter，response processor 不再直接调用 provider kind 或 app kind 投影 helper。
本轮也把 usage sink bridge 的 forward error 与 transformed usage provider facts 投影收敛到 adapter，bridge 不再直接拼 provider kind、app kind 或 transformed usage record。
本轮继续把 Axum response builder 失败上下文投影收敛到 adapter，`handlers` 与 `response_processor` 不再直接维护 Claude/Codex/通用 tag 的 build-error 文案。
本轮继续把上游响应解析/聚合失败日志的协议前缀与 body lossy 投影收敛到 adapter，`handlers` 不再直接调用 `String::from_utf8_lossy` 拼日志。
本轮继续把未标记 SSE fallback 诊断 event 的 debug/warn 分发收敛到 adapter，`handlers` 不再直接匹配 `UnlabeledSseFallbackLogLevel`。
本轮继续把非流式上游 JSON/错标 SSE 解析、解析失败日志、fallback event 分发和 response body parse error 映射收敛到 `error_mapper` helper，`handlers` 不再直接调用 core parse helper 或 parse-error adapter。
本轮继续把 Claude/Codex 非流式上游响应解析失败文案、`UpstreamResponseParseFailureLogContext` 和 `UnlabeledSseFallbackLogContext` 选择收敛到 `error_mapper` 协议专用 helper，`handlers` 不再直接维护 parse/fallback context。
本轮继续把转换后 JSON/SSE 的 core response builder 调用、构造失败映射和 Axum bridge 收敛到 `response_adapter` helper，Claude/Codex transform handler 不再直接调用 `rebuilt_json_proxy_response` 或 `transformed_sse_proxy_response`。
本轮继续把 Claude/Codex transform 正常响应的 JSON/SSE build context 选择收敛到 `response_adapter` 协议专用 helper，handler 不再直接引用 `CoreResponseBuildFailureContext::{ClaudeJson,CodexResponses}` 或对应 `AxumResponseBuildErrorContext`。
本轮继续把 Claude 非流式响应转换的 api_format 分发收敛到 `proxy_core_adapter::provider_claude_transform_response_for_api_format`，handler 不再直接调用 OpenAI/Gemini 响应转换函数或维护 Gemini rectifier 日志循环。
本轮继续把 Claude 流式响应转换的 api_format 分发收敛到 `proxy_core_adapter::provider_claude_transform_sse_for_api_format`，handler 不再直接选择 OpenAI Chat/Responses/Gemini stream converter 或维护 Gemini rectifier 日志回调。
本轮继续把 Claude transform 的 streaming/聚合决策收敛到 `proxy_core_adapter::provider_claude_transform_streaming_decision`，handler 不再直接判断 Codex OAuth、上游 SSE header、Responses 聚合例外或 api_format -> 错标 SSE 聚合策略。
本轮继续把 Codex Chat 非流式 Chat->Responses 转换与 history 记录收敛到 `proxy_core_adapter::transform_codex_chat_response_with_history`，handler 不再直接调用 `chat_completion_to_response_with_context` 或非流 history record helper。
本轮继续把 Codex Chat 流式 Chat->Responses SSE 转换与 history 记录收敛到 `proxy_core_adapter::transform_codex_chat_sse_with_history`，handler 不再手动串联 Chat SSE 转换 helper 和流式 history record helper。
本轮继续把 Codex Chat->Responses transform 的 streaming/错标 SSE 聚合决策收敛到 `proxy_core_adapter::codex_chat_transform_streaming_decision`，handler 不再直接判断上游 SSE header 或指定 ChatCompletions 聚合策略。
本轮继续把 Claude/Codex transformed usage 的 format 与 stream event filter 选择收敛到 `proxy_core_adapter` 的协议专用 helper，handler 不再直接引用 `TransformedResponseUsageFormat`、`claude_stream_usage_event_filter` 或 `codex_stream_usage_event_filter`。
本轮继续把 Codex Chat 错误体归一化后的非 JSON warning 输出与 Responses 错误体 neutral response 构造收敛到 adapter，`handlers` 不再直接调用 `normalize_codex_chat_error_body` 或 `non_json_body_log_message`。
本轮继续把转换后 JSON/Codex 错误响应构造失败的日志上下文和 `ProxyCoreError -> ProxyError` 映射收敛到 `error_mapper` 协议专用 helper，`handlers` 不再直接选择 `CoreResponseBuildFailureContext::CodexResponsesError`/`CodexProxyError` 或调用通用 `response_build_error_to_proxy_error`。
本轮继续把 Codex Chat 上游错误响应与转发层 Codex proxy error 的 neutral response 构造、build-error 映射和 Axum bridge 收敛到 `response_adapter` 协议专用 helper，`handlers` 不再直接调用 `codex_chat_error_proxy_response`、`codex_proxy_error_response` 或选择 Codex 错误响应的 `AxumResponseBuildErrorContext`。
本轮继续把 Claude/Codex 响应转换失败的日志上下文和 `TransformError` 包装收敛到 `error_mapper` 协议专用 helper，`handlers` 不再直接选择 `ResponseTransformFailureContext` 或调用通用 `response_transform_error_to_proxy_error`。
本轮继续把协议入口的 `ProxyCoreError -> ProxyError` 映射与 forward error usage 记录收敛到 `proxy_core_adapter::record_forward_core_error_usage`，Claude/Codex/Gemini handler 不再直接串联 `proxy_core_error_to_proxy_error` 与 `record_forward_error_usage`。
本轮继续把 ConfigSource 的 app summary DTO 组装收敛到 adapter，`proxy_core_host` 不再直接构造 `AppSummaryConfig`。
本轮也把管理 API token-source 决策收敛到 adapter，handler middleware 不再直接读取 `CC_SWITCH_PROXY_MANAGEMENT_TOKEN` 或调用 core 决策函数。
本轮继续把 ProviderRouter 的 channel route input/source fallback 决策收敛到 adapter，router 只消费 adapter 提供的 core route input 与 source，不再暴露 route record 命名。
本轮继续把 ProviderRouter 的当前供应商选择结果组装收敛到 adapter，router 只消费 adapter 返回的当前 provider id，不再加载完整 Provider 实体或直接构造 `ProviderSelectionInput::current`。
本轮继续把 ProviderRouter 的 failover 候选选择结果组装收敛到 adapter，router 只负责读取 failover lookup facts、已配置 provider id 列表和 circuit breaker 可用性，不再直接构造 `ProviderSelectionInput` 或调用 core `select_provider_ids`。
本轮继续把 ProviderRouter 的 auto-failover 配置读取结果决策收敛到 adapter，router 只负责读取 proxy_config，读取失败时的日志和默认禁用故障转移策略由 adapter 维护。
本轮继续把 ProviderRouter 的 circuit breaker config 与 failure threshold 配置读取结果 fallback 收敛到 adapter，router 只负责读取 proxy_config 并管理 breaker 实例生命周期。
本轮继续把 ProviderRouter 的 dry-run route circuit availability 到 rejected:circuit_open response mutation 收敛到 adapter，router 只负责按候选 circuit key 查询 breaker 可用性。
本轮继续把 ProviderRouter 的 failover lookup availability 到 selection candidate 的投影收敛到 adapter，router 只负责读取 failover lookup/provider id facts 并查询 breaker 可用性。
本轮继续把 ProviderRouter 的 failover queue/provider id facts 到 provider circuit lookup 的投影收敛到 adapter，router 不再展开 queue provider_id 或 provider map keys。
本轮继续把 management dry-run route resolution 收敛到 adapter：`ProviderRouter` 不再持有 `RouteResolveRequest`/`RouteResolveResponse`、不再直接调用 `resolve_channel_route` 或 response mutation helper，只暴露 channel route facts 和 candidate circuit availability 查询。
本轮继续把固定 app catalog 从 proxy-core 默认端口移出，`ProxyConfigSource::list_apps` 改为宿主必填能力，CC Switch 的 `AppType::all()` 只保留在 adapter-owned config source。
本轮继续把 RoutePolicy raw 中 `failoverProviderIds` 的读取 contract 收敛到 domain/routing helper，`ProxyEngine` 不再直接读取 raw JSON 字段。
本轮继续把 forward runtime 的 auth profile DB key 注入 helper 收敛到 adapter，`proxy_core_host` 不再维护本地 wrapper 或直接查询 channel-key。
本轮继续把 forward runtime 的 current-provider settings/DB fallback 读取入口收敛到 adapter，`proxy_core_host` 只消费最终 provider id 字符串。
本轮继续把 Claude 非流式响应转换的 OpenAI Chat/OpenAI Responses/Gemini Native 分支识别与转换调度收敛到 adapter，Claude provider 只保留 trait 边界和错误映射。
本轮继续把 Claude 请求转换的 api_format 分支、prompt cache 选择、stream usage 注入和 Gemini request wrapper 调度收敛到 adapter，provider 公开函数只保留兼容入口和错误映射。
本轮继续把 Claude Anthropic 消息规范化策略收敛到 adapter，provider 公开函数不再直接判断 api_format、tool-thinking history gate 或 DeepSeek thinking-disabled effort 清理。
本轮继续把 forwarder 对 Claude api_format、消息规范化和请求转换的调用改为直接消费 adapter helper，不再经由 provider 兼容函数绕回代理模块。
本轮继续删除 provider 模块对 Claude api_format、消息规范化和请求转换兼容函数的 re-export，Claude provider 对外只保留 `ClaudeAdapter`。
本轮继续把 forwarder 对 Codex Responses->Chat 判定、上游模型覆写和 reasoning options 的调用改为直接消费 adapter helper，并删除 provider 模块对应 re-export。
forwarder 的 Codex app gate 与 Responses->Chat provider predicate 一跳 wrapper `forwarder_should_convert_codex_responses_to_chat` 已删除；request source transform plan 直接组合 app gate 与 provider predicate。
本轮继续把 global proxy 的显式代理 URL parse、scheme allowlist 和错误消息投影收敛到 `proxy_core_adapter::{validate_explicit_proxy_url,invalid_explicit_proxy_url_message}`，host HTTP client 只负责 reqwest proxy 构造和 client builder。
本轮继续把 provider custom endpoints 的列表排序、URL key 归一化、空 URL 新增校验和 last-used mutation 收敛到 `proxy_core_adapter`，endpoint service 不再直接穿透 `Provider.meta.custom_endpoints`。
本轮继续把 Codex proxy error facts/kind 的 `ProxyError` 投影收敛到 `proxy_core_adapter`，`error_mapper` 不再直接引用 `CodexProxyErrorKind` 或组装 `CodexProxyHostErrorFacts`。
本轮继续把 forward failure 的 `ProxyError -> ForwardFailureKind` 投影收敛到 `proxy_core_adapter`，并把 raw message 与 display message 的选择策略下移到 `proxy-core::forward_failure_message_from_proxy_status`；`error_mapper` 不再直接引用 `ForwardFailureKind` 或调用 core forward-failure 分类入口。
本轮继续把 forwarder 的 Claude Desktop route 模型映射收敛到 `proxy_core_adapter::apply_forward_request_model_mapping_from_provider`，forwarder 不再直接调用 `claude_desktop_config`。
本轮曾把 forwarder 的 Claude provider adapter 名称判定收敛到 `proxy_core_adapter::provider_adapter_name_is_claude`，forwarder 不再手写 `adapter.name() == "Claude"`；后续 `ForwarderAdapterFacts::from_adapter` 成为唯一归属点后该单行 helper 已删除。
forwarder 的 Claude 默认 api_format 一跳 wrapper `forwarder_claude_api_format` 已删除；Copilot vendor 分流入口继续保留在 `proxy_core_adapter::resolve_forwarder_claude_api_format`，forwarder 仍不直接读取 provider Claude api format。
本轮继续把 forwarder 的 Claude api_format transform gate 收敛到 `proxy_core_adapter::forwarder_claude_transform_required`，forwarder 不再直接调用 core transform gate。
本轮继续把 Copilot fingerprint header 常量提升到 adapter，`proxy_core_adapter` 不再反向引用 `providers::copilot_auth` 常量。
本轮继续把 `codex_chat_history` 从 `proxy::providers` 移到 `proxy` 模块根，provider 目录只保留 provider adapter 和账号认证相关实现。
本轮继续把 `ProviderRouterSource` 拆成 router 端的 provider/channel/config/health 四个 focused port；host adapter 侧拆出对应 DB-backed source/store，并把 `ProviderRouter::new(Arc<Database>)` 迁到 `proxy_core_adapter::provider_router_from_database` factory，生产代码不再直连 router 的 DB 构造入口。
本轮继续把 `ProviderRouter` 的 route channel 输入从 router-local `ProviderRouterChannelRecord` 切到 core `RouteResolveChannelInput`；management channel specs/records 仍由 host adapter 直接从 DB source 读取完整记录，避免为了管理 API 把 DAO record 暴露给 router，也避免 router 维护自己的中转 DTO。
forwarder provider adapter base URL 的一跳 wrapper `forwarder_provider_base_url` 已删除；adapter context 和 stream check fallback 在各自 host 边界内直接调用 trait object，`forwarder.rs` 仍只消费 URL facts。
forwarder provider adapter auth info/header 的一跳 wrapper `forwarder_provider_auth_info` / `forwarder_provider_auth_headers` 已删除；adapter context 内部直接调用 trait object，`forwarder.rs` 仍通过 auth source 获取 fallback auth。
forwarder provider adapter upstream URL 的一跳 wrapper `forwarder_provider_upstream_url` 已删除；adapter context 内部直接调用 trait object，`forwarder.rs` 仍通过 request source 获取 URL plan。
forwarder provider adapter name 的一跳 wrapper `forwarder_provider_adapter_name` 已删除；`ForwarderAdapterFacts` 在 adapter context 内直接从 trait object 读取 name，`forwarder.rs` 仍只消费 facts。
forwarder provider adapter registry 的一跳 wrapper `forwarder_provider_adapter_for_app` 已删除；adapter context factory 和 stream check fallback 在 adapter 边界内直接调用 provider registry，`forwarder.rs` 仍不直接调用 provider 模块。
本轮继续把 forwarder 暴露在函数签名里的 provider adapter trait 收敛为 `proxy_core_adapter::ForwarderAdapterHandle`，forwarder 不再直接导入 `providers::ProviderAdapter`。
本轮继续把模型列表命令层的 `FetchedModel` DTO 入口收敛到 `proxy_core_adapter::FetchedModel`，`model_fetch_transport` 不再作为命令层 DTO re-export，只保留 core model catalog transport port 的 reqwest 执行实现。
本轮继续把模型列表命令层的自定义 User-Agent 解析入口收敛到 `proxy_core_adapter::model_fetch_custom_user_agent_header`，命令不再直接调用 provider 模块的 schema helper。
本轮继续把 stream_check 服务的标准 provider adapter base URL 提取收敛到 `proxy_core_adapter::stream_check_provider_base_url`，服务层不再直接导入 provider adapter registry、trait 或具体 Claude adapter，只保留 reachability HTTP 探测执行。
本轮继续把 `FailoverSwitchManager` 的 proxy_config enabled 读取收敛到 `proxy_core_adapter::failover_switch_app_enabled_from_db`：manager 只保留 Tauri emit、托盘刷新和 `hot_switch_provider` 宿主副作用，配置读取失败时跳过切换的策略集中在 adapter。
本轮继续收窄 `switch_proxy_provider` 命令边界：命令层不再直接读取 provider 或重复官方供应商拦截策略，只把 app/provider 交给 `ProxyService::switch_proxy_target`，由 service 热切换路径统一执行 provider 存在性校验和接管模式防线。
本轮继续把 `reset_circuit_breaker` 恢复后切回判断的 source 读取收敛到 `proxy_core_adapter::reset_circuit_breaker_switchback_target_from_db`：命令层保留健康状态重置、运行时熔断器重置和 `FailoverSwitchManager` 的 Tauri 副作用，proxy_config、当前 provider、故障转移队列和 provider name 投影由 adapter 负责。
本轮继续把 `set_auto_failover_enabled` 的开关 plan source 读取收敛到 `proxy_core_adapter::auto_failover_toggle_plan_from_db`：命令层保留队列写入、目标切换、proxy_config 写回、事件和托盘刷新，proxy_config、故障转移队列、当前 provider 和 core plan 输入组装由 adapter 负责。
本轮继续把批量 stream check 的 `proxy_targets_only` 过滤 source 读取收敛到 `proxy_core_adapter::stream_check_proxy_target_ids_from_db`：命令层不再直接组合当前 provider 与故障转移队列，只保留 provider 遍历、Copilot endpoint override、可达性探测和日志保存。
本轮继续把 `ProxyService::get_takeover_status` 的 per-app proxy_config enabled 读取和 DTO 投影收敛到 `proxy_core_adapter::proxy_takeover_status_from_db`：service 不再直接读取 Claude/Codex/Gemini 三个配置行或手动构造 `ProxyTakeoverStatus`，读取失败按 false 的兼容语义由 adapter 维护。
本轮继续把 Live 接管支持的 app catalog 收敛到 `proxy_core_adapter::live_takeover_app_types`：`ProxyService` 的全量恢复和残留接管检测不再手写 Claude/Codex/Gemini 列表，只保留具体 live 配置读写、恢复和检测副作用。
本轮继续把接管开启后的官方供应商 warning source 和事件投影收敛到 `proxy_core_adapter::proxy_official_warning_event_from_current_provider_db`：`ProxyService::set_takeover_for_app` 不再直接读取当前 provider、加载 Provider 实体或拼 warning payload，只保留 Tauri `AppHandle` emit。
本轮继续把 Live 接管流程里的当前 provider source 读取收敛到 `proxy_core_adapter::{current_provider_for_app_from_db,require_current_provider_for_app_from_db}`：`ProxyService` 保留本地 helper 名称和 live 文件副作用，但不再直接读取 settings 当前 provider 或 provider 表实体。
本轮继续把 Live token 同步的当前 provider source 和支持 app label 收敛到 `proxy_core_adapter::{live_token_sync_provider_from_db,live_token_sync_app_label}`：`ProxyService::sync_live_config_to_provider` 不再为 Claude/Codex/Gemini 重复读取 settings/provider 表，只保留 token 投影、provider settings 写回和日志。
本轮继续把 `ProxyService::set_takeover_for_app` 的 proxy_config enabled 状态读取和 read-modify-write 收敛到 `proxy_core_adapter::{proxy_app_enabled_from_db,set_proxy_app_enabled_in_db}`：service 只保留接管/恢复流程编排，enabled 持久化错误文案和状态投影由 adapter 维护。
本轮继续把手动 `stop_with_restore` 的批量 enabled 清理收敛到 `proxy_core_adapter::clear_live_takeover_enabled_flags_in_db`：service 不再手写 Claude/Codex/Gemini app catalog 或配置表循环，读取失败忽略、写入失败 warning 的 best-effort 语义由 adapter 维护；`stop_with_restore_keep_state` 继续保留 enabled 状态以支持下次启动恢复。
本轮继续把简单 Live 备份恢复的备份读取和 JSON 解析错误投影收敛到 `proxy_core_adapter::live_backup_config_for_simple_restore_from_db`：`ProxyService::restore_live_config_for_app_inner` 只负责 Claude/Codex/Gemini live 文件写回与日志，读备份失败静默跳过、备份 JSON 损坏时报错的旧语义由 adapter 锁定。
本轮继续把 keep-state 关闭流程里的 legacy `live_takeover_active` 标志清理收敛到 `proxy_core_adapter::clear_legacy_live_takeover_active_flag_in_db`：`ProxyService::stop_with_restore_keep_state` 不再直接读写全局 proxy_config，同时继续保留 per-app enabled 状态用于下次启动自动恢复。
本轮继续把 `set_takeover_for_app` 的 Live 备份存在性 source 读取收敛到 `proxy_core_adapter::live_takeover_backup_exists_from_db`：service 只消费是否存在备份的事实，读取失败按无备份继续重建接管的兼容策略和 warning 文案由 adapter 维护。
本轮继续把 `set_takeover_for_app` 的 per-app Live 备份删除收敛到 `proxy_core_adapter::{delete_live_backup_best_effort_in_db,delete_live_backup_in_db}`：同步/写入失败回滚继续静默清理，关闭接管时删除失败继续作为错误返回，两种语义在 adapter 中显式分离。
本轮继续把 `set_takeover_for_app` 的 legacy 接管 active flag 兼容写入和 any-enabled 读取收敛到 `proxy_core_adapter::{set_legacy_live_takeover_active_best_effort_in_db,live_takeover_any_enabled_from_db}`：service 不再直接调用废弃 DAO 兼容方法或维护“检查接管状态失败”错误投影。
本轮继续把 `set_takeover_for_app` 关闭接管后的 provider health 清理收敛到 `proxy_core_adapter::clear_provider_health_for_app_in_db`：service 不再直接调用 health DAO，清理失败继续按原中文错误返回。
本轮继续把 `start_with_takeover` 的全量 Live 备份清理收敛到 `proxy_core_adapter::{cleanup_all_live_backups_best_effort_in_db,delete_all_live_backups_best_effort_in_db}`：启动前失败继续输出清理 warning，恢复成功后的清理继续静默 best-effort。
本轮继续把 `start_with_takeover` 的 legacy 接管 active flag 写入收敛到 `proxy_core_adapter::{set_legacy_live_takeover_active_in_db,set_legacy_live_takeover_active_best_effort_in_db}`：首次写入失败仍中止启动，恢复成功后的回滚写入仍为 best-effort。
本轮继续把 Live token 回填后的 provider settings 持久化收敛到 `proxy_core_adapter::update_live_token_sync_provider_settings_in_db`：`ProxyService::sync_live_config_to_provider` 只负责判断 token 投影是否产生变更，DB 写回失败继续 warning-only，不阻断接管。
本轮继续把 `ProxyService::{start,stop}` 的全局 `proxy_enabled` 持久化收敛到 `proxy_core_adapter::{enable_global_proxy_in_db,disable_global_proxy_best_effort_in_db}`：启动时启用失败仍返回错误，停止时禁用失败继续只输出 warning。
本轮继续把 `ProxyService` 的 `proxy_config` 读取、动态端口持久化和 `update_config` 保存规则收敛到 `proxy_core_adapter::{proxy_config_from_db,persist_ephemeral_listen_port_if_needed_in_db,update_proxy_config_preserving_live_takeover_active_in_db}`：service 只消费配置事实并根据 previous/new config 判断是否重启，`live_takeover_active` 保留和动态端口写回错误投影由 adapter 维护。
本轮继续把 `ProxyService` 中构建有效 provider settings 所需的 common config snippet 读取收敛到 `proxy_core_adapter::provider_effective_settings_with_common_config_from_db`：service 不再通过 provider live 模块穿透读取 DB snippet，只保留接管字段注入、备份合并和 live 写入编排；实际 `write_live_with_common_config` 文件写入边界留作后续独立切片。
本轮继续把 `stop_with_restore` / `stop_with_restore_keep_state` 的 legacy active flag 清理、全量 Live 备份删除和全量 provider health reset 收敛到 `proxy_core_adapter::{clear_legacy_live_takeover_active_flag_strict_in_db,delete_all_live_backups_in_db,clear_all_provider_health_in_db}`：service 只保留停止 server、恢复 live 文件和 enabled 状态策略编排，cleanup 错误文案由 adapter 维护。
本轮继续把 `recover_from_crash` 的 legacy active flag 清理和全量 Live 备份删除复用到 `proxy_core_adapter::{clear_legacy_live_takeover_active_flag_strict_in_db,delete_all_live_backups_in_db}`：异常恢复路径只保留 live 文件恢复编排，cleanup 失败仍按旧语义中断返回。
本轮继续把 `backup_live_configs` / `backup_live_config_strict` 的 Live 备份 JSON 序列化和 `save_live_backup` 错误投影收敛到 `proxy_core_adapter::save_live_backup_value_in_db`：service 只负责读取 live 文件、判断占位符接管状态并决定是否保存备份，备份持久化细节由 adapter 维护。
本轮继续把 `restore_live_config_for_app_with_fallback_inner` 的 Live 备份读取与 JSON 解析错误投影收敛到 `proxy_core_adapter::live_backup_value_for_restore_from_db`：service 保留“备份是代理占位符则跳过并走 SSOT/清理兜底”的恢复策略，DB backup source 细节由 adapter 维护。
本轮继续把 `restore_live_from_ssot_for_app` 的当前供应商读取、provider 列表读取和代理占位符供应商保护收敛到 `proxy_core_adapter::ssot_live_restore_provider_from_db`：service 只负责把 adapter 返回的可恢复 provider 写回 live 文件，`write_live_with_common_config` 文件写入边界仍留作后续切片。
本轮继续把 `update_live_backup_from_provider_inner` 中用于保留 Codex MCP/OAuth 的现有 Live 备份读取与 JSON 解析错误投影收敛到 `proxy_core_adapter::existing_live_backup_value_for_update_from_db`：service 保留备份内容合并和统一会话路由注入，existing backup source 细节由 adapter 维护。
本轮继续把 `update_live_backup_from_provider_inner` 的 provider-derived Live 备份序列化和 `save_live_backup` 更新错误投影收敛到 `proxy_core_adapter::save_provider_live_backup_from_effective_settings_in_db`：Claude/Codex 保留完整 effective settings，Gemini 继续只保存 env 备份，service 只负责构造和合并 effective settings。
本轮继续把 `hot_switch_provider_inner` 的 provider 读取、官方供应商拦截、当前目标判断、Live 备份存在性读取和 current provider 双写收敛到 `proxy_core_adapter::{proxy_hot_switch_target_state_from_db,persist_hot_switch_current_provider_sources}`：service 只消费目标状态并保留 live 备份刷新、live 文件同步和运行时 active target 更新编排。
本轮继续把 `ProxyService::{start,update_config}` 中的 `ProxyServer::new` 构造入口收敛到 `proxy_core_adapter::proxy_server_from_runtime_config`：service 仍持有运行中 server 并负责生命周期锁、启动、停止和重启编排，但不再直接绑定 runtime server 构造签名。
本轮继续把 `restore_live_from_ssot_for_app` 的 SSOT provider live 写入调用和错误投影收敛到 `proxy_core_adapter::write_ssot_live_restore_provider_with_common_config`：service 只保留恢复分支编排，具体 provider common-config live writer 调用留在 adapter 边界内。
本轮继续把 `ProxyServer::new` 的运行态装配收敛到 `proxy_core_adapter::proxy_state_from_runtime_sources`：server 只保留 listener/router/shutdown 生命周期编排，ProviderRouter、事件总线、failover manager、shadow/history store 与 `CcSwitchProxyServices` 装配集中在 adapter 边界内。
本轮继续把 `ProxyService::write_claude_live` 的 Claude live settings 清洗入口改为直接调用 `proxy_core_adapter::sanitize_claude_settings_for_live`：service 不再绕行 `services::provider` façade，Live 写入仍保留原有 host 文件落盘职责。
本轮继续把 `CcSwitchProxyRuntime` 到 `ForwarderRuntimeHostResources` 的资源包构造收敛到 `proxy_core_adapter::forwarder_runtime_host_resources_from_runtime`：host forward runtime 只把 runtime 交给 adapter，转发链所需 router/status/event/history/failover/AppHandle 克隆集中在 adapter 边界内。
本轮继续把 `HostForwardRuntime for CcSwitchProxyRuntime` 的 forwarding bridge 调用收敛到 `proxy_core_adapter::forward_proxy_request_with_cc_switch_runtime`：host trait impl 只转交 `self/request/plan`，adapter 负责取 DB、构造 runtime resources 并进入现有 forwarding bridge。
本轮继续把 `ProxyServiceRuntimeResources` 与 `HostForwardRuntime` for `CcSwitchProxyRuntime` 两个 runtime trait impl 移入 `proxy_core_adapter`：`proxy_core_host` 只声明 runtime 资源字段和 `CcSwitchProxyServices` 类型别名，adapter 负责把 host runtime 投影成 core service/forwarding 能力。
本轮继续把 `CcSwitchProxyRuntime` 数据结构和具体 `CcSwitchProxyServices<CcSwitchProxyRuntime>` alias 移入 `proxy_core_adapter`：`proxy_core_host` 仅保留旧模块路径的兼容 re-export，生产版 host 文件不再定义 runtime 数据形状。
本轮继续把 `ProxyServer` 对 `CcSwitchProxyServices` 的生产 import 切到 `proxy_core_adapter::CcSwitchProxyRuntimeServices`：运行中 server 的 core services 类型不再依赖 `proxy_core_host` 兼容路径，`proxy_core_host` 的旧 alias 仅保留给测试兼容。
本轮继续把 `src/lib.rs` 中的 `proxy_core_host` 模块声明限制为 `#[cfg(test)]`：生产 crate 编译图不再包含该兼容模块，相关历史边界测试仍可通过 test-only module 读取旧测试辅助路径。
本轮继续把 `ProxyState` 运行态数据结构与 `proxy_engine` helper 移入 `proxy_core_adapter`：`proxy/server.rs` 只保留旧路径 re-export 和 HTTP listener/router 生命周期，运行态 state shape 由 adapter 维护。
本轮继续把 `ProxyService` 持有的运行中 server 类型路径收敛到 `proxy_core_adapter::CcSwitchProxyServer`：service 既不直接构造 `ProxyServer`，也不直接导入 `proxy/server.rs` 的具体类型，adapter 继续作为 server runtime 入口。
本轮继续把生产 `ProxyServer` 构造入口改为 `from_runtime_state(config, state)`：`proxy_core_adapter::proxy_server_from_runtime_config` 负责从 `Database/AppHandle` 装配 `ProxyState`，旧 `ProxyServer::new(config, db, app_handle)` 仅保留为 server 单元测试兼容入口。
本轮继续把 handler、response processor、auth adapter 和 request context 的 `ProxyState` import 直接切到 `proxy_core_adapter`，并移除 `proxy/server.rs` 对 `ProxyState` 的旧路径 re-export：HTTP server 不再作为运行态 state 类型的兼容出口。
本轮继续把 forwarder 对托管账号运行时的 Copilot endpoint/model vendor/live model list 以及 token resolution 入口收敛到 `proxy_core_adapter` thin wrappers：forwarder 不再直接依赖 `proxy::managed_account_auth`，后续可在 adapter 内继续把 Tauri runtime 读取替换为外部宿主可注入的 AuthProvider 端口。
本轮继续把 `ManagedAccountAuthResolution` DTO 迁入 `proxy_core_adapter`：`proxy::managed_account_auth` 只负责读取 Copilot/Codex OAuth 运行时并返回 adapter-owned resolution，结果字段归属不再留在 host auth 模块。
本轮继续把托管账号的 `managed_account_auth_plan` 调用、provider account-id 投影和 resolution 组装迁入 `proxy_core_adapter`：`proxy::managed_account_auth` 的公开入口降为接收 account id / runtime 的 Tauri 运行态读取函数，便于后续替换为宿主可注入 AuthProvider。
本轮继续在 `proxy_core_adapter` 内引入 `CcSwitchManagedAccountRuntimeSource`：managed-auth plan/resolution 组装不再直接调用 host auth runtime 函数，而是通过 adapter-owned source 包装 Copilot/Codex OAuth 运行态读取，后续可把该 source 替换为外部宿主实现。
本轮继续把 `CcSwitchManagedAccountRuntimeSource` 提升为 `ManagedAccountRuntimeSource` trait 实现：adapter 的 managed-auth plan/resolution 只依赖运行态 source trait，CC Switch 的 Tauri/Copilot/Codex OAuth 读取保留在默认 source 实现内，为后续外部宿主替换 source 留出稳定接点。
本轮继续把 `ManagedAccountRuntimeSource` 接入 `CcSwitchProxyRuntime`、`ForwarderRuntimeHostResources` 和 `RequestForwarder`：forwarder 对 Copilot 动态 endpoint、live models、model vendor 和 managed token resolution 的读取都走 runtime 注入 source，不再通过 `app_handle` wrapper 临时构造运行态读取入口。
本轮继续收窄 `ManagedAccountRuntimeSource` 的 forwarder 生产接口：provider account-id 投影、managed auth plan/resolution、Copilot dynamic endpoint、live models 与 model vendor 查询都改为 source 的 provider-aware 方法，`RequestForwarder` 不再调用 `*_from_runtime_source` helper。
本轮继续收窄 managed-auth 的测试入口：`proxy_core_adapter` 不再保留 test-only `AppHandle` convenience wrapper，单测通过 `ManagedAccountRuntimeSource` 直接验证 plan/resolution 行为，避免后续测试重新依赖 Tauri runtime 入口。
本轮继续补强 managed-auth 外部 source contract：adapter 单测使用 fake `ManagedAccountRuntimeSource` 覆盖 provider `authBinding` 到 Copilot/Codex account id 的投影、Codex OAuth account id 回传和 session header gate，证明外部宿主可替换 token runtime 而不复刻 Tauri 状态读取。
本轮继续把 Copilot dynamic endpoint 到上游 `base_url` 的选择、日志和写回收敛到 `ManagedAccountRuntimeSource::apply_copilot_dynamic_base_url_for_provider`：`RequestForwarder` 不再直接读取 runtime endpoint、调用 dynamic base URL 决策 helper 或维护 base URL mutation 细节，只触发 source 行为。
本轮继续把 Claude adapter gate、Copilot model vendor 读取与 Claude API format 决策收敛到 `ManagedAccountRuntimeSource::resolve_claude_api_format_for_adapter`：`RequestForwarder` 不再直接读取 Copilot vendor、判断是否需要解析 Claude API format 或调用 Claude format 决策 helper，只消费 source 返回的可选 API format。
本轮继续把 Claude body policy 的 adapter/API-format gate 收敛到 `ForwarderRequestSource::apply_claude_body_policies`：`RequestForwarder` 不再先行判断是否应用 Claude body policy，只把 adapter fact、可选 API format 与 rectifier 配置交给 request source。
本轮继续把上游请求体 `model` 字段投影收敛到 `ForwarderRequestSource::request_body_model`：`RequestForwarder` 不再直接从 JSON body 读取 model，只消费 request source 返回的 outbound/logging model 事实。
本轮继续把 Codex media prevention 的 app gate 收敛到 `ForwarderRequestSource::apply_app_media_prevention`：`RequestForwarder` 不再判断是否为 Codex app 后才调用 media prevention，只把 app/provider/body 和 rectifier 配置交给 request source。
本轮继续把 Copilot live model 列表读取、model ID fallback 解析、Copilot gate 和请求体 `model` 写回收敛到 `ManagedAccountRuntimeSource::apply_copilot_live_model_for_adapter`：`RequestForwarder` 不再直接读取 live models、判断是否需要 live model resolution、调用模型 ID 匹配 helper 或维护 JSON 写回细节，只触发 source 行为。
本轮继续把 failover 切换调度封装为 `FailoverSwitchScheduler` runtime source：`RequestForwarder` 不再持有 `FailoverSwitchManager` 或 `AppHandle`，只在成功记录后调用注入 scheduler；CC Switch 默认 scheduler 仍在 adapter 内把调度投影到现有 manager、托盘/UI 与 Live 切换副作用。
本轮继续把 forwarder 的 `ProxyRuntimeStatus`、active route target map 和 `ProxyEventBus` 合并为 `ForwarderRuntimeStateSource`：`RequestForwarder` 不再直持三份运行态状态资源，状态计数、当前目标和事件发射仍通过 adapter helper 执行，后续外部宿主可以替换 runtime state source 或把事件桥接到自己的监控接口。
本轮继续把 active connection RAII guard 的 acquire/release 生命周期接入 `ForwarderRuntimeStateSource`：guard 不再直持 `ProxyRuntimeStatus`，连接计数增减的异步释放也通过 runtime source 方法执行，避免流式响应生命周期把 status lock 类型泄漏到 forwarder。
本轮继续把 request-started 事件发射与 request-started 状态写入接入 `ForwarderRuntimeStateSource`：`RequestForwarder` 只保留请求生命周期顺序编排，不再直接拆出 `ProxyEventBus`/`ProxyRuntimeStatus` 调用 request lifecycle helper。
本轮继续把 provider/channel attempt started/succeeded/failed 事件发射接入 `ForwarderRuntimeStateSource`：`RequestForwarder` 只决定 attempt 阶段与时机，不再直接拆出 `ProxyEventBus` 调用 attempt event helper。
本轮继续把 active route target 写入与 route-selected 事件发射接入 `ForwarderRuntimeStateSource`：`RequestForwarder` 不再同时拆出 `current_providers` 与 `ProxyEventBus` 调用 active target helper，后续外部宿主可替换当前路由目标存储/事件桥接。
本轮继续把 forward success/failure 状态写入接入 `ForwarderRuntimeStateSource`：`RequestForwarder` 不再直接拆出 `ProxyRuntimeStatus` 调用 success/failure status helper，成功后是否触发 failover switch 仍由 source 返回布尔结果交给 forwarder 调度。
本轮继续把 current provider、provider failure、rectifier retry failure 状态写入与 rectifier retry failure failover 分类接入 `ForwarderRuntimeStateSource`：`RequestForwarder` 不再直接拆出 `ProxyRuntimeStatus` 或调用 retry-failure policy helper，运行态状态观测也不再作为生产路径依赖。
本轮继续收窄 `ForwarderRuntimeStateSource` trait surface：测试用 `status()` / `events()` 读 handle 不再属于 trait contract，forwarder 单测通过 fixture 自持 status/event bus 观测运行态结果，默认 source 只保留 concrete 层的 status 观测 helper 供 adapter 单测验证状态写入。
本轮继续把普通 forward failure 的 `ProxyError -> ForwardFailureKind` 投影、可重试分类和 provider/terminal 失败日志策略接入 `ForwarderRuntimeStateSource`：`RequestForwarder` 不再直接调用 forward failure helper，只消费 source 返回的失败事实、重试决策和日志记录内容。
本轮继续把 Gemini shadow session store 与 Codex Chat history store 合并为 `ForwarderProtocolStateSource`：`RequestForwarder` 不再直持协议会话状态，Claude/Gemini transform replay 和 Codex Responses->Chat history enrich 仍消费同一批 store，外部宿主可在 adapter 边界替换协议会话状态实现。
本轮继续收窄 `ForwarderProtocolStateSource` 的生产接口：`RequestForwarder` 不再通过 source getter 拿到 `GeminiShadowStore`/`CodexChatHistoryStore`，而是调用 source 暴露的 Codex Chat request enrich 与 Claude request transform 行为，协议状态存储类型继续留在 adapter 内。
本轮继续把 Codex Chat history enrich 的恢复计数日志收敛到 `ForwarderProtocolStateSource::enrich_codex_chat_request`：`RequestForwarder` 不再读取 enrich 返回数量或维护协议状态日志文本，只等待 protocol source 完成请求体补全。
本轮继续把 Claude protocol transform 的默认 API format 与 client-provided session gate 收敛到 `ForwarderProtocolStateSource::transform_claude_request`：`RequestForwarder` 不再默认填充 `anthropic` 或判断是否透传 session id，只把 transform plan 的可选 format 与 session facts 交给 protocol source。
本轮继续把 forwarder 的 provider/channel attempt runtime 包装为 `ForwarderAttemptRuntimeSource`：`RequestForwarder` 不再直持 `ProviderRouter`，attempt 放行、成功/失败健康记录和 neutral permit 释放都通过注入 source 进入 adapter helper，后续可把该 source 替换为独立中转模块的 routing/circuit runtime。
本轮继续把 forwarder 的上游发送执行包装为 `ForwarderTransportSource`：`RequestForwarder` 不再直接读取全局代理 URL、展开 reqwest/raw-hyper 发送分支或映射 reqwest 错误，CC Switch 默认 source 仍复用现有 pooled reqwest、raw hyper、SOCKS/HTTP proxy 和 header-case 策略，后续外部宿主可替换 transport 执行层。
本轮继续把 forwarder 的响应读取与成功就绪判定包装为 `ForwarderResponseSource`：`RequestForwarder` 不再直接读取 response body、执行非流式 body timeout、流式首包 timeout/replay 或错误响应 body 文本提取，默认 source 保持“记录 provider 成功前先确认响应可读”的既有 failover 语义。
本轮继续把 channel response status mapping 收敛到 `ForwarderResponseSource`：`RequestForwarder` 不再直接调用响应状态映射 helper，响应 source 负责根据当前 channel 的 statusCodeMapping 改写上游状态码并保留原有 debug 语义。
本轮继续把非成功上游响应的 status/body 到 `ProxyError::UpstreamError` 投影收敛到 `ForwarderResponseSource::finalize_upstream_response`：`RequestForwarder` 不再直接读取 status code、读取错误 body 或构造 upstream error，默认 source 内部 helper 不作为 trait surface 暴露。
本轮继续把成功响应 readiness helper 从 `ForwarderResponseSource` trait surface 收进默认 source 内部：`prepare_success_response` 仍负责非流式 body buffering 和流式首包 replay，但只作为 `CcSwitchForwarderResponseSource` 内部 helper，`RequestForwarder` 的测试入口也改为通过 `finalize_upstream_response` 覆盖同一语义。
本轮还把 channel authProfileRef 的 provider/channel-key/ignore action 判定从 adapter 上移到 `proxy-core::domain`：core 现在同时持有 auth profile 解析、missing provider warning 和 attempt action contract，host adapter 只执行 provider/key 查询、key materialization 和 attempt 写入副作用。
本轮继续把 forwarder 的上游请求 body/headers/transport-policy 组装包装为 `ForwarderRequestSource`：`RequestForwarder` 不再直接调用请求体过滤、prompt cache trace、stream/identity 策略、ordered headers、body 序列化和 managed-account 上游占位 auth 校验 helper，默认 source 仍保持现有上游请求语义，后续外部宿主可替换请求组装层。
本轮继续把 finalized upstream body 的 model/outbound logging label 收敛到 `ForwarderPreparedRequest`：`RequestForwarder` 不再在请求体定稿后再次投影 `filtered_body.model`，只消费 request source 随 prepared body 返回的最终模型事实。
本轮继续把 `request_body_model` 从 `ForwarderRequestSource` trait surface 收进默认 source 内部：外部替换 source 不再需要实现独立 JSON model probe，只需通过 transformed/prepared request 级结果返回模型归因事实。
本轮继续把 `transform_provider_request_body` 从 `ForwarderRequestSource` trait surface 收进默认 source 内部：provider adapter transform branch 仍由默认 source 包装，但外部替换 source 只需实现完整的 `transform_request_body` 行为级入口。
本轮继续把 `convert_codex_responses_to_chat_body` 从 `ForwarderRequestSource` trait surface 收进默认 source 内部：Codex Responses 到 Chat Completions body 转换仍由默认 source 包装，但外部替换 source 只需实现完整的 `transform_request_body` 行为级入口。
本轮继续把 `optimize_copilot_request` 从 `ForwarderRequestSource` trait surface 收进默认 source 内部：Copilot 分类、body cleanup、tool_result merge、thinking strip 与 warmup override sequencing 仍由默认 source 包装，但外部替换 source 只需实现 `prepare_copilot_request_optimization` 行为级入口。
本轮继续把 `apply_media_prevention` 从 `ForwarderRequestSource` trait surface 收进默认 source 内部：media prevention policy resolution、text-only provider/model replacement 与替换日志仍由默认 source 包装，但外部替换 source 只需实现 `apply_app_media_prevention` / `apply_claude_body_policies` 行为级入口。
本轮继续把上游请求 URL/model 与 debug body 日志收敛到 `ForwarderRequestSource::log_upstream_request`：`RequestForwarder` 不再拼接请求日志文本或直接序列化 finalized body，只把 adapter tag、URL、model label 和 body 交给 request source。
本轮继续收窄 `ForwarderRequestSource` 的请求 header 策略边界：custom User-Agent provider fact 与 exact header-case 保留策略也由 request source 计算并随上游请求 parts 返回，`RequestForwarder` 不再直接消费这些请求组装 helper。
本轮继续把每个 provider attempt 的 Bedrock pre-send optimizer 收敛到 `ForwarderRequestSource`：Bedrock env flag 判断、thinking/cache 优化 mutation 与对应日志投影都在 request source 内完成，`RequestForwarder` 只消费按 provider 独立克隆后的 attempt body，避免 failover 场景优化字段泄漏。
本轮继续把 provider/adapter request facts 收敛到 `ForwarderRequestSource`：provider adapter registry 选择进入 `adapter_for_app`，provider base URL 提取、full-url 标记和 GitHub Copilot upstream 判定合并为 `ForwarderProviderUrlFacts`，adapter name 与 Claude adapter 判定合并为 `ForwarderAdapterFacts`，`RequestForwarder` 不再直接消费这些地址/adapter 事实 helper，后续地址级中转配置可在 request source 边界扩展 base URL/interface/model 组合。
本轮继续把 provider/channel/Copilot 上游请求体模型预处理收敛到 `ForwarderRequestSource`：provider model mapping、thinking type normalization、channel upstream model override、Copilot model id normalization 和非 Copilot `[1M]` suffix 剥离都由 request source 完成；`RequestForwarder` 只保留异步 Copilot live model resolution、URL planning 与后续 transform 编排。
本轮继续把 Codex Responses→Chat 上游请求体转换收敛到 `ForwarderRequestSource`：Codex Chat 上游模型覆写、reasoning options 投影、OpenAI o-series/max token 分流、Responses body 到 Chat Completions body 的纯转换，以及 app/provider/endpoint gate 都由 request source 完成；`RequestForwarder` 只保留 protocol source 的 history enrich 并消费 request source 的布尔结果。
本轮继续把 generic provider adapter 的同步 request transform 收敛到 `ForwarderRequestSource`：非 Claude adapter 的 `transform_request` 调度由 request source 统一包装，`RequestForwarder` 只保留 Claude protocol-state transform 与通用 transform 分支选择。
本轮继续把 transform planning 收敛到 `ForwarderRequestSource`：Claude API format fallback、Claude transform gate、通用 provider transform gate 与 URL/body transform 所需格式投影合并为 `ForwarderTransformPlan`，`RequestForwarder` 只保留异步 runtime API-format resolution 和协议状态转换调用。
本轮继续让 body transform 执行分支消费 `ForwarderTransformPlan::use_claude_transform`：`RequestForwarder` 不再在 transform 执行处重新判断是否为 Claude adapter，只按 request source 产出的 transform plan 选择 Claude protocol-state transform 或通用 provider transform。
本轮继续让 body transform 执行分支消费 `ForwarderTransformPlan::use_provider_transform`：`RequestForwarder` 不再用总体 `needs_transform` 反推通用 provider transform 路径，Claude/provider 两类 transform 执行 gate 都由 request source 的 plan 输出。
本轮继续把 forwarder 的上游 URL planning 收敛到 `ForwarderRequestSource`：Codex Responses→Chat endpoint rewrite、Claude transform endpoint rewrite、Gemini Native URL、full URL query passthrough 与 channel param override 合并仍复用 adapter/core URL helper，但 `RequestForwarder` 不再直接调用 URL planning helper 或 adapter URL builder。
本轮继续把 Claude API-format 请求体策略收敛到 `ForwarderRequestSource`：Claude/Anthropic 消息历史 normalization 与同阶段 media prevention 由 request source 统一执行，`RequestForwarder` 不再直接调用 Claude normalization helper，只保留 API format runtime resolution 与 transform 分支编排。
本轮继续把 media fallback 的预防式图片替换与反应式 retry body 计划收敛到 `ForwarderRequestSource`：`RequestForwarder` 不再直接调用 media fallback gate、unsupported-image 错误识别、图片块检测或 marker 替换 helper，只消费 request source 产出的替换结果和 retry body。
本轮继续把 thinking signature/budget rectifier 的错误触发判断、Anthropic app/provider gate 与请求体整流收敛到 `ForwarderRequestSource`：`RequestForwarder` 不再直接抽取 rectifier 错误文本、调用 signature/budget 判定或 app/provider gate helper，只根据 request source 返回的 rectifier plan 编排同 provider 重试与失败归因。
本轮继续把 forwarder 的上游 auth header 准备包装为 `ForwarderAuthSource`：`RequestForwarder` 不再直接提取 provider auth、解析 managed-account runtime token、构造 Codex OAuth session headers、注入 Copilot optimizer auth overrides 或调用 upstream auth finalization helper，默认 source 保持现有鉴权语义，后续外部宿主可替换鉴权头组装层。
本轮继续把 Copilot auth override 的可选准备、request classification/deterministic request id gate、deterministic request id 与 interaction id 计算收敛到 `ForwarderAuthSource`：`RequestForwarder` 不再直接调用 Copilot session/request/interaction helper，也不再读取 optimizer auth override 配置字段，只把可选分类事实、配置、原始 body、上游 body 和 headers 交给 auth source。
本轮继续把 `prepare_copilot_auth_optimization` 从 `ForwarderAuthSource` trait surface 收进默认 source 内部：direct Copilot auth override sequencing 仍由默认 auth source 包装，但外部替换 source 只需实现 `prepare_optional_copilot_auth_optimization` 行为级入口。
本轮继续把 core-facing `AuthProvider` 接入 `ForwarderAuthSource` 热路径：默认 auth source 在构造上游认证头前先用 app/provider/channel/request 上下文调用 `AuthProvider::resolve_auth`，若返回显式 `AuthInfo.headers` 则直接作为 base auth headers；CC Switch 默认 `CcSwitchAuthProvider` 仍返回空 headers，因此现有 provider adapter、managed-account token 刷新、Codex session header 和 Copilot auth override fallback 语义不变。
本轮继续把 managed-account token 解析后的结果契约 `ManagedAccountAuthResolution` 迁入 `proxy-core::auth`：runtime token、Codex account id 和是否发送 Codex session headers 的事实不再是 adapter 私有 DTO，外部宿主实现可复用同一 resolution contract。
本轮继续把 Copilot optimizer 的启用 gate、请求体分类与变形收敛到 `ForwarderRequestSource`：`RequestForwarder` 不再直接判断 Copilot optimizer 是否运行，也不再直接调用 Copilot 分类、孤立 tool_result 清理、tool_result 合并、thinking block 剥离或 warmup 模型降级 helper，只消费 request source 返回的优化后 body 与可选分类事实。
本轮继续收窄 `ForwarderAttemptRuntimeSource` 的放行接口：legacy 单 provider circuit-breaker bypass 判定从 trait surface 收进默认 source 内部，`RequestForwarder` 只通过 `ForwarderAttemptAllowInput` 传当前 attempt、app 和完整 attempts 事实，外部中转实现不再需要复刻 CC Switch 的历史兼容 helper。
本轮继续把 max-attempt 上限判定并入 `ForwarderAttemptRuntimeSource::allow` 的结构化决策：默认 source 在占用 half-open permit 前先返回 `Stop(ForwarderAttemptLimitReached)`，`RequestForwarder` 只处理 stop/skip/allowed 三种结果，外部中转实现不再需要单独暴露 retry-policy helper。
本轮继续收窄 `ForwarderRuntimeStateSource` 的日志接口：media/signature/budget rectifier retry 的成功/失败日志行拼接从 trait surface 收进默认 source 内部，`RequestForwarder` 只触发 `log_rectifier_retry_success` / `log_rectifier_retry_failure` 行为方法，外部中转实现不再需要返回 CC Switch 格式化日志字符串。
本轮继续把 terminal forward failure warning 的日志行拼接从 `ForwarderRuntimeStateSource` trait surface 收进默认 source 内部：`RequestForwarder` 只触发 `log_terminal_forward_failure`，外部中转实现不再需要返回 `[FWD-002]` 格式化日志字符串。
本轮继续把 retryable forward failure warning 的日志行拼接和错误消息 payload 从 `ForwarderFailureDecision::Retryable` 返回值中移出：`RequestForwarder` 只消费 retryable 控制流并触发 `log_retryable_forward_failure`，外部中转实现不再需要返回 `[FWD-001]` 格式化日志字符串或格式化错误消息。
本轮继续把 max-attempt warning 的日志行 payload 从 `ForwarderAttemptAllowDecision::Stop` 中移出：默认 attempt runtime source 在 `allow` 内部记录上限 warning，`RequestForwarder` 只按 `Stop` 中断循环，外部中转实现不再需要返回 max-attempt 格式化日志字符串。
本轮继续把 forwarder 请求体 transform 执行优先级收敛到 `proxy-core::request_transport::forwarder_request_body_transform_action_from_plan`：Codex Responses→Chat、Claude protocol transformed body、generic provider transform 与 passthrough 的分支选择由 core 纯规则维护，`ForwarderRequestSource` 只负责执行对应宿主 adapter/protocol 转换。
本轮继续把 max-attempt 停止原因文案收敛到 `proxy-core::forward_failure::build_forward_attempt_limit_reached_log`：默认 attempt runtime source 只负责加 app 前缀和写日志，不再维护尝试上限的宿主侧中文 payload。
本轮继续把 media/signature/budget rectifier retry 的 kind、成功/失败文案、provider failure label 和 rectifier 错误消息选择策略收敛到 `proxy-core::forward_failure`：adapter 只保留 app 前缀兼容壳和 `ProxyError` 到 core input 的 host 映射，不再维护三类 retry payload 或上游 body/fallback 文本选择规则。
本轮继续把 terminal/no-available forward failure 的 runtime status 文案收敛到 `proxy-core::forward_failure::{forwarder_no_available_provider_status_message,forwarder_terminal_failure_status_message}`：runtime state source 只负责写入状态，不再维护终态错误中文 payload。
本轮继续把普通 forward failure 的 app/code 日志行格式与错误消息选择收敛到 `proxy-core::forward_failure::{forwarder_failure_log_line,forward_failure_message_from_proxy_status}`：adapter 只保留调用兼容壳和 host `ProxyError` 事实投影，不再维护 `[app] [FWD-*]` 拼接规则或 message-kind 分支。
本轮继续把上游 URL planning 的 endpoint rewrite、full-endpoint/query 透传和 channel param override 策略收敛到 `proxy-core::request_url::forward_upstream_url_plan`：`ForwarderRequestSource` 只投影 base URL、endpoint、Claude/Codex/Gemini transform facts 与 host adapter URL builder，不再维护本地 plan 策略。
本轮继续把 all-providers-circuit-open 的 FO-004 warning 日志行收敛到 `proxy-core::forward_failure::forwarder_all_providers_circuit_open_log_line`：provider selection failure adapter 只负责在 host 边界发出 warning，不再维护 `[FO-004] 所有供应商均已熔断` payload。
本轮继续把 no-providers-configured 的 FO-005 warning 日志行收敛到 `proxy-core::forward_failure::forwarder_no_providers_configured_log_line`：provider selection failure adapter 保留 AppError 映射与日志副作用，不再维护 `[FO-005] 未配置供应商` payload。
本轮继续把 failover switch 配置读取失败的 FO-002 warning 日志行收敛到 `proxy-core::provider_selection::failover_config_read_error_log_line`：adapter 仍负责读取 config、默认返回 false 和发 warning，不再维护 `[FO-002] 无法读取...跳过切换` payload。
本轮继续把 ProviderRouter 读取 auto-failover 配置失败时默认禁用的决策收敛到 `proxy-core::provider_selection::provider_router_auto_failover_enabled_decision`：adapter 只把 `AppProxyConfig.auto_failover_enabled` 投影为 bool、按 core 返回的 log line 发 error，不再维护读取失败 fallback 策略。
本轮继续把接管状态的 enabled/缺失配置矩阵收敛到 `proxy-core::ports::proxy_takeover_status_from_enabled_options`：adapter 只把 DB config 读取结果投影成 `Option<bool>`，不再维护 Claude/Codex/Gemini 缺失时默认 false 和 OpenCode/OpenClaw 默认 false 的状态组装规则。
本轮继续把 proxy live URL 生成规则收敛到 `proxy-core::ports::proxy_live_urls_from_listen_parts`：adapter 不再维护 `0.0.0.0`/`::` 转回环地址、IPv6 URL bracket 和端口 0 不写回的规则，只在 host 启动流程中消费 core 返回的 origin/base URL。
本轮继续把临时监听端口写回规则收敛到 `proxy-core::ports::proxy_config_with_ephemeral_listen_port`：adapter 只负责持久化 DB，端口 0 才写回实际 bind 端口、固定端口不写回的策略由 core 维护。
本轮继续把更新 proxy config 时保留 legacy `live_takeover_active` 的规则收敛到 `proxy-core::ports::proxy_config_preserving_live_takeover_active`：adapter 只负责读取 previous config、持久化 new config 和返回前后配置，不再直接维护 legacy flag 复制规则。
本轮继续把 legacy `live_takeover_active` flag 的配置更新规则收敛到 `proxy-core::ports::proxy_config_with_live_takeover_active`：best-effort 清理路径仍由 adapter 负责 DB 读写，但不再直接改 `ProxyConfig.live_takeover_active` 字段。
本轮继续把 app 级 proxy config 的 enabled flag 更新规则收敛到 `proxy-core::ports::app_proxy_config_with_enabled`：adapter 只负责读取/写回 app config 和错误文案，不再维护 `AppProxyConfig.enabled` 字段更新 helper。
本轮继续把 Live 接管支持的 switch-mode app catalog 收敛到 `proxy-core::ports::live_takeover_app_kinds`：core 统一维护 Claude/Codex/Gemini 清单与顺序，adapter 仅映射为宿主 `AppType` 并保留 DB/live 文件副作用。
本轮继续把 Provider switch 的 normal/takeover-aware 分流和 takeover lock 适用范围收敛到 `proxy-core::ports::{provider_switch_dispatch_for_app,provider_switch_requires_takeover_lock}`：core 维护 OpenCode OMO/Claude Desktop 直通 normal、Claude/Codex/Gemini 需要 lock 的纯策略，adapter 只投影 `AppType` 与 provider category。
本轮继续把 Provider takeover-owned live 同步目标和 additive app 删除后端映射收敛到 `proxy-core::ports::{provider_takeover_live_sync_target_for_app,provider_live_removal_target_for_app}`：core 维护 Claude Desktop 写 live config、其他 app 写 backup，以及 OpenCode/OpenClaw/Hermes 删除目标，adapter 继续只负责宿主 enum 投影。
本轮继续把 Provider live sync scope、current-provider 可用性、删除当前 provider 判定、switch backfill source 与 live_config_managed 标记判定收敛到 `proxy-core::ports`：core 统一维护 additive app 与 switch app 的纯策略，adapter 只负责把宿主 `AppType` 转成 `AppKind`。
本轮继续把 Provider 新增/更新路径里的 additive live write、key rename、legacy common-config migration、初始 live_config_managed marker、OpenCode OMO/OMO Slim switch/update 分流收敛到 `proxy-core::ports`：core 维护 app/category 纯策略和错误文案，adapter 继续只投影宿主 provider category。
本轮继续把 Live token sync 支持 app label 收敛到 `proxy-core::ports::live_token_sync_app_label`：core 维护 Claude/Codex/Gemini label 矩阵，adapter 只把宿主 `AppType` 映射为 `AppKind`，DB/current provider 查询仍留在 host。
本轮继续把默认 live import 跳过策略、provider 是否同步 live 和 DB-only provider live-config presence 错误策略收敛到 `proxy-core::ports`：core 消费 `AppKind` 与 `live_config_managed` 事实，adapter 继续负责 provider meta 投影和实际 DB/live 读写。
本轮继续把 provider credential issue 的本地化错误规格收敛到 `proxy-core::ports`：core 维护稳定的 key/中英文文案映射，adapter 继续负责从宿主 provider settings 提取凭据事实并把 spec 转成 `AppError`。
本轮继续把 Claude default model 归一化收敛到 `proxy-core::ports::normalize_claude_models_in_value`：core 维护旧 `ANTHROPIC_MODEL`/`ANTHROPIC_SMALL_FAST_MODEL` 到 `ANTHROPIC_DEFAULT_*` 的纯 JSON 变换，adapter 仅按 `AppType::Claude` 调用。
本轮继续把 Claude/Gemini live proxy placeholder probe 和本地 proxy URL 判定收敛到 `proxy-core::ports`：core 维护 env token key 与 loopback URL 纯规则，Codex TOML experimental bearer token 检测暂留 adapter 以复用宿主 `codex_config` parser。
本轮继续把 provider launch env 投影收敛到 `proxy-core::ports::launch_env_vars_from_provider_settings`：core 维护 Claude/Codex/Gemini 从 provider settings JSON 到启动环境变量的纯映射，adapter 仅负责传入宿主 `Provider.settings_config` 与 `AppType` 映射。
本轮继续把 live token 同步到 provider settings 的 JSON 变换收敛到 `proxy-core::ports::provider_settings_with_live_token_sync`：core 维护 Claude/Codex/Gemini token 提取、placeholder no-op、空 settings 初始化和非法 settings 报错规则，adapter 仅负责把结果写回宿主 `Provider`。
本轮继续把 takeover placeholder/env 的纯 JSON mutation 收敛到 `proxy-core::ports`：core 维护 Claude env 清理、Codex auth placeholder 增删与 Gemini env 写入/清理，adapter 继续保留 Codex TOML config placeholder 清理和 live 文件副作用。
本轮继续把 live takeover 的 URL/base-url 纯匹配规则收敛到 `proxy-core::ports::{proxy_urls_match,live_env_base_url_matches}`：core 维护 trim 与尾斜杠兼容，adapter 继续组合 Codex TOML base_url 解析。
本轮继续把 Claude live settings 清理和 JSON common-config subset/merge/remove 辅助收敛到 `proxy-core::ports`：core 维护 host-only 字段剥离、数组一次性匹配删除、深合并与深删除规则，adapter/common-config 路径只复用 core helper。
本轮继续把 provider 默认 live import 与存储前 settings 归一化收敛到 `proxy-core::ports::{provider_default_live_import_settings,normalize_provider_settings_for_storage}`：core 维护 Claude 旧模型字段兼容和非 Claude no-op 规则，adapter 只负责 `AppType` 到 `AppKind` 投影。
本轮继续把非 Codex provider 凭据 JSON 形状收敛到 `proxy-core::ports`：core 维护 Claude env、Gemini env map、OpenCode options 与 OpenClaw/Hermes apiKey/baseUrl 的纯提取规则，adapter 继续保留 Codex TOML/API key 解析和宿主 `Provider` 投影。
本轮继续收窄 provider policy adapter façade：legacy common-config migration skip 直接委托 core `should_skip_provider_legacy_common_config_migration`，credential issue spec 与 key-change issue message 改为 core helper 直接 re-export，adapter 只保留需要宿主 enum/record 投影的入口。
本轮继续把 Claude takeover 模型字段派生与 env 写入策略收敛到 `proxy-core::ports`：core 维护角色别名、`[1M]` 标记、显示名 fallback、模型覆盖字段清理和 token placeholder 策略；adapter 继续保留 `Provider` 到 managed-account/Copilot auth policy 的宿主投影。
本轮继续把 OpenCode/OpenClaw common-config 片段里的 provider 凭据剥离规则收敛到 `proxy-core::ports`：core 维护 `options.apiKey`/`options.baseURL` 与 `apiKey`/`baseUrl` 的纯 JSON 删除策略，adapter 继续保留 snippet 序列化、Codex TOML 和宿主 `AppType` 分发。
本轮继续把非 Codex common-config snippet 生成与分发入口收敛到 `proxy-core::ports`：core 维护 Claude/Gemini/OpenCode/OpenClaw 的 provider 字段剥离、Claude Desktop/Hermes 空片段、空片段 `{}` fallback 和 JSON pretty serialization；adapter 继续保留 Codex TOML 清理和宿主 `Provider` 投影。
本轮继续把 Claude/Gemini common-config 的 contains/apply/remove 运行时 JSON 规则收敛到 `proxy-core::ports`：core 维护 JSON snippet 解析、Claude 深合并/深删除、Gemini env 合并/删除和错误文案；adapter 继续保留 Codex TOML merge/remove 与 `AppType` 分发。
本轮继续把 provider common-config 启用判定收敛到 `proxy-core::ports`：core 维护显式 `common_config_enabled` 优先级、snippet 非空 gate 和存储归一化是否需要 snippet 的纯策略；adapter 只负责从 `ProviderMeta`、宿主 settings 和 legacy contains 检测投影输入事实。
本轮继续把 proxy takeover 的 retakeover backup 决策和 official provider category 策略收敛到 `proxy-core::ports`：core 维护 backup 可复用/需恢复、官方供应商 warning 与 Codex official live 重应用判定；adapter 只负责从 `Provider.category` 投影 category 字符串。
本轮继续把 OpenClaw live write 的 typed/raw/reject 决策收敛到 `proxy-core::ports`：core 维护 typed parse 成功、raw fallback 和 reject 文案策略；adapter 继续负责把 `Provider.settings_config` 反序列化为宿主 `OpenClawProviderConfig`，service 继续负责实际写入 live config。
本轮继续把 OpenCode live provider fragment 提取和 live write typed/raw/reject 决策收敛到 `proxy-core::ports`：core 维护 full config 中 `provider.{id}` 片段选择、raw fallback 和 reject 文案策略；adapter 继续负责宿主 `OpenCodeProviderConfig` 反序列化，service 继续负责实际写入 live config。
本轮继续把非 Codex provider credential value 组装收敛到 `proxy-core::ports`：core 维护 Claude/Gemini/OpenCode/OpenClaw/Hermes 的缺字段分类、Gemini 默认 base URL 和 additive app 空 base URL fallback；adapter 继续保留 Codex auth/config.toml 合并、base_url regex 兼容解析和宿主 `Provider` 投影。
本轮继续把 additive app 的 stream-check base URL 分发收敛到 `proxy-core::domain` 与 `proxy_core_adapter::stream_check_provider_base_url`：core 维护 OpenCode npm fallback、OpenClaw `baseUrl`、Hermes `base_url` 纯解析和缺失 base URL 错误规格，service 只调用统一入口并保留 reachability 探测职责。
本轮继续把 Gemini provider settings 基础结构校验收敛到 `proxy-core::ports::validate_gemini_settings_basic`：core 维护 `env`/`config` 字段形状和本地化错误规格，`gemini_config` 与 `ProviderService` 只通过 adapter 复用同一校验入口。
本轮继续把 Gemini settings 的 env map 与 JSON settings 双向投影收敛到 `proxy-core::ports`：core 维护纯 HashMap/JSON 转换和非字符串值过滤，`gemini_config` 仅保留兼容函数名并委托 adapter。
本轮继续把 Gemini settings 严格切换校验收敛到 `proxy-core::ports::validate_gemini_settings_strict`：core 维护 OAuth 空 env 放行、非空 env 必须含 `GEMINI_API_KEY` 和本地化错误规格，host 只保留 AppError 映射。
本轮继续把 Gemini provider 鉴权类型判定收敛到 `proxy-core::ports::detect_gemini_auth_type`：core 维护 partner key 优先级、Google official 名称匹配与 PackyCode 关键词规则，host 只负责从 `Provider` 投影输入并执行 settings 文件写入。
本轮继续把 Gemini `.env` 解析和序列化收敛到 `proxy-core::ports`：core 维护宽松解析、严格行号错误分类和稳定排序输出，adapter 负责把结构化 parse issue 映射回宿主 `AppError`。
本轮继续把 usage script 凭据覆盖策略收敛到 `proxy-core::ports::usage_script_credentials_from_parts`：core 维护脚本显式非空值优先、空值回退 provider 凭据和 `baseUrl` 去尾斜杠规则，host 继续负责按 app/provider settings 提取 fallback 与执行脚本。
本轮继续把 Claude takeover 的 provider facts 决策收敛到 `proxy-core::ports::apply_claude_takeover_fields_for_provider_facts`：core 维护 managed-account/Copilot auth policy、provider-vs-live 模型字段来源和 env 写入，adapter 只投影宿主 provider facts。
本轮继续把 Codex takeover TOML 字段计划收敛到 `proxy-core::ports::codex_takeover_toml_config_patch`：core 维护接管时强制 local proxy `base_url`、`wire_api = responses` 和可选 upstream model 写回规则，adapter 继续负责 `toml_edit` 语法保留写入。
本轮继续把 live takeover 占位符检测的 app 分派收敛到 `proxy-core::ports::live_config_has_proxy_placeholder_for_app`：core 维护 Claude/Codex/Gemini 统一检测语义，adapter 只投影 Codex config.toml bearer-token 是否命中的宿主解析事实。
本轮继续把 live takeover 是否匹配当前代理地址的 app 分派收敛到 `proxy-core::ports::live_takeover_config_matches_proxy_for_app`：core 维护 Claude/Gemini env base URL 与 Codex config facts 的组合规则，adapter 只投影 Codex config.toml base_url 是否匹配当前 `/v1` 代理地址。
本轮继续把 live backup snapshot 的占位符跳过策略收敛到 `proxy-core::ports::live_backup_snapshot_from_live_config`：core 维护“接管中的 live config 不作为备份来源”的规则，adapter 只投影 Codex config.toml bearer-token 是否命中的宿主解析事实。
本轮继续把 takeover hot-switch 的 live 持有、Codex backup refresh、Codex/Claude live sync gate 收敛到 `proxy-core::ports`：core 维护纯布尔策略，adapter 只把 `AppType` 投影为 `AppKind`。
本轮继续把 channel-key 覆盖 provider settings 的 app 选择入口从字符串推进到 `proxy-core::auth::settings_config_with_channel_auth_key_for_app`：core 接收 `AppKind` 维护 Claude/Gemini/Codex/自定义 app 的 key 注入字段规则，adapter 只负责克隆宿主 `Provider` 并投影 `AppType`。
本轮继续把 `ProxyError` 的用户展示文案策略收敛到 `proxy-core::errors::proxy_error_display_message_from_status`：core 维护上游错误、超时、转发失败、不可用 provider、数据库和转换错误的稳定中文 contract，adapter 只把 host `ProxyError` 投影为状态 kind、原始 message、上游 body 与 fallback display message，`error_mapper` 保留兼容 wrapper。
本轮继续把显式上游代理 URL 的解析失败文案和 scheme allowlist 收敛到 `proxy-core::transport::{validate_explicit_proxy_url,invalid_explicit_proxy_url_message}`：core 维护 `http/https/socks5/socks5h` 支持矩阵、URL 脱敏和错误文本，host `http_client` 仍负责实际构造 `reqwest::Proxy`。
本轮继续把非流式上游响应 JSON/SSE 解析失败的日志上下文与 body lossy 投影收敛到 `proxy-core::response_parse::upstream_response_parse_failure_log_message`：core 维护 Claude/Codex 前缀、Chat 专用文案和 body 展示策略，adapter 只负责把日志级别分发给宿主 logger。
本轮继续把 Axum/transport response 构造失败的上下文文案收敛到 `proxy-core::response_build::ProxyResponseBuildErrorContext`：core 维护 tag、Claude、Codex SSE/JSON/错误响应的稳定日志文本，host response adapter 只保留旧 `AxumResponseBuildErrorContext` type alias 并执行 Axum bridge。

当前原则：核心 crate 可以新增端口和领域字段，但不得引入 `tauri`、`Database`、settings、commands、services 等宿主依赖；现有 runtime 行为必须继续通过 targeted tests 证明不回归。
977. Claude Desktop gateway 的 provider 可用性、1M 默认能力、API format 支持范围、managed OAuth 直连禁用和 proxy 模式 Base URL/API Key 校验规则已迁入 `proxy-core::claude_desktop_gateway_auth`；host adapter 只把 `Provider.settings_config` 与 `Provider.meta` 投影为 `ClaudeDesktopProviderValidationInput`。
978. MiMo Anthropic thinking history normalization gate 已迁入 `proxy-core::response_transform::should_normalize_mimo_anthropic_thinking_history`；host adapter 只投影 provider settings/meta 与 upstream model。
979. 自定义 endpoint URL key 归一化与空 URL 本地化 issue spec 已迁入 `proxy-core::management_api`；host adapter 只 re-export URL key helper 并把 core issue spec 映射为 `AppError::localized`。
980. Codex provider credential value 的 base_url 解析与 `CodexBaseUrlMissing/Invalid` 分类已迁入 `proxy-core::ports::provider_codex_credential_values_from_parts`；host adapter 只提取 auth/config 文本并投影为 `CodexCredentialParts`。
981. Codex provider base URL 从 `settings_config.base_url/baseURL/config` 的提取与尾斜杠归一化已迁入 `proxy-core::ports::codex_base_url_from_settings`；host adapter 只传入 provider settings。
982. required provider base URL 的缺失错误文案与 `Option<String> -> Result<String, String>` 包装已迁入 `proxy-core::ports::required_provider_base_url`；host adapter 只提供 provider label 与各 provider base URL。
983. 管理 API bearer 鉴权的 runtime config 读取与校验已下沉到 `proxy-core::ManagementAuthSource` 与 `ProxyEngine::validate_management_auth`；host middleware 不再直接读取 `state.config` 或组装 bearer decision，热更新后的 `ProxyState.config` 通过 runtime source 继续生效。
984. 管理 API bearer 鉴权的 adapter 测试 facade 已删除；测试直接调用 `proxy-core::management_auth` 决策函数，避免 `proxy_core_adapter` 继续作为纯策略重导出层。
985. forwarding runtime 缺失与 route plan provider mismatch 的 `ProxyCoreError` 构造已迁入 `proxy-core::routing`；host adapter 不再手写这些 core error variant，只负责转发 core constructor。
986. Claude Desktop model-list provider selection 的失败/无可用 provider 错误 contract 已迁入 `proxy-core::claude_desktop_gateway_auth`；adapter 只负责把 router/provider loader 结果交给 core error constructor。
987. channel test/reachability probe 的 provider-not-found 与 probe failure `ProxyCoreError` 构造已迁入 `proxy-core::ports`；adapter 只负责查询宿主 provider 与执行 stream check。
988. unsupported `AppKind -> AppType` 的 config error 构造已迁入 `proxy-core::domain`；adapter 不再保留 app-kind 错误文案 wrapper。
989. channel auth profile 缺失/禁用 key 的 auth error 构造已迁入 `proxy-core::provider_auth`；adapter 只负责把 channel/key ref 交给 core contract。
990. Claude Desktop gateway token 加载失败的 auth error 构造已迁入 `proxy-core::claude_desktop_gateway_auth`；adapter 只负责调用宿主 token store 并把错误交给 core constructor。
991. channel test probe 的 app type 解析失败 `InvalidRequest` 构造已迁入 `proxy-core::ports`；adapter 只负责执行宿主 `AppType` parse 并传递错误。
992. AppError 桥接使用的 generic config/internal/invalid-request `ProxyCoreError` 构造已迁入 `proxy-core::error`；adapter 保留宿主错误类型分派但不再直接 new core error variant。
993. client model catalog raw 到 `ModelCatalog` 的纯投影 facade 已从 adapter 移除；host 测试和 adapter 内部直接调用 `proxy-core::model_fetch` 的 stable API，adapter 仅保留 Codex active config 读取这类宿主 I/O。
994. Codex 默认模型上下文窗口的函数 facade 已从 adapter 移除；生产 host 通过 adapter re-export 的 core 常量读取默认值，adapter 不再保留额外函数包装。
995. materialized channel 数量到 route source 的一行 adapter helper 已移除；adapter 在唯一 source 选择点直接调用 `proxy-core::management` source 决策并保留 legacy fallback I/O。
996. Gemini env 的解析、序列化、JSON/Map 转换纯函数 facade 已从 adapter 移除；生产 host 仍经 adapter re-export 接入 `proxy-core::ports`，adapter 不再保留额外函数包装。
997. common-config snippet 错误文案和 OpenCode/OpenClaw common-config value 测试 helper 的纯函数 facade 已从 adapter 移除，改为直接 re-export core ports。
998. common-config settings mutation 错误文案的纯函数 facade 已从 adapter 移除，生产 host 仍经 adapter re-export 接入 core ports。
999. forwarder 终态状态文案、failure log line 和 rectifier retry label 的纯函数 facade 已从 adapter 移除；adapter 只保留 host `ProxyError` 到 core failure facts 的投影函数。
1000. runtime active targets 写入的同签名纯函数 facade 已从 adapter 移除，`ProxyRuntimeStatus` 的 active target 排序规则直接由 core port re-export 提供。
1001. Codex settings 中 `auth` object 与 config text 读取的纯 accessor facade 已从 adapter 移除；adapter 继续 re-export core ports，Provider 级 API key/base URL 投影仍留在宿主边界。
1002. takeover/hot-switch 的纯 bool 状态策略 facade 已从 adapter 移除；带 `AppType` 的 hot-switch helper 仍留在 adapter 负责 `AppKind` 投影。
1003. Codex restored live settings parts、provider validation issue spec、live-config presence policy、delete-current-provider 判断的值级 facade 已从 adapter 移除，改为 re-export core ports。
1004. local proxy URL 判断与 Codex takeover auth placeholder 的同签名值级 facade 已从 adapter 移除，改为 re-export core ports；带配置文本和 provider 投影的 takeover helper 保持在 adapter。
1005. Claude/Gemini takeover env 字段应用与清理的同签名值级 facade 已从 adapter 移除，改为 re-export core ports；按 app/provider 组合 live takeover 的 helper 仍留在 adapter。
1006. Claude takeover policy 写入 helper 的同签名 facade 已从 adapter 移除，改为 re-export core ports；provider facts 组装仍留在 adapter。
1007. Channel key runtime lookup 已从 DAO enabled-key convenience selector 改为 adapter-owned source：adapter 读取 raw key record，投影为 core runtime candidate 并调用 core selection policy，DAO 的 enabled selector 降为测试便捷入口。
1008. `proxy_core_host` 测试中的 client model catalog raw 投影直连 core 已改为经 `proxy_core_adapter` re-export 接入，host/core 边界测试重新覆盖该入口。
1009. Usage script credential fallback helper 的一行 adapter facade 已删除，改为 re-export `proxy-core` port，provider usage service 继续经 adapter 入口消费同一凭据解析策略。
1010. `src-tauri` 已声明 Cargo workspace 并纳入 `crates/proxy-core`，独立代理模块测试复用主 `Cargo.lock` 与 workspace target，避免 path crate 单独测试生成游离构建产物。
1011. `proxy-core` 纳入 workspace 后进入主 crate `cargo clippy --all-targets` 门禁，已清理 core 内部等价 clippy warning，保证迁移后的独立模块与宿主共享静态检查口径。
1012. Forwarder attempt runtime 的最大尝试次数日志和 legacy 单 provider circuit-breaker bypass 决策已迁入 `proxy-core::forward_failure`，host adapter 只投影 `ForwardAttempt` 数量/channel 事实并继续执行 router permit。
1013. 自定义 User-Agent 的 trim、空值忽略和 `HeaderValue` 校验规则已迁入 `proxy-core::request_headers::parse_custom_user_agent`，provider/model-fetch/stream-check/forwarder 路径继续经 `proxy_core_adapter` 共享同一解析入口。
1014. Stream check 的 provider override 配置合并与 probe result 到 `StreamCheckResult` 的成功/失败/degraded envelope 构造已迁入 `proxy-core::ports`，host `StreamCheckService` 只保留 reqwest 探测、timestamp 注入和 provider override 投影。
1015. Stream check 批量命令捕获单 provider 异常后的 failed `StreamCheckResult` envelope 已迁入 `proxy-core::ports::stream_check_failed_result`，host command 只负责并发/循环调度、错误文本和 timestamp 注入。
1016. Stream check retry loop 的终端兜底 failed envelope 已迁入 `proxy-core::ports::stream_check_failed_result_with_retry_count`，host service 仍负责重试循环和 `retry_count` 事实注入。
1017. Host `ProxyError` 到 core `ProxyErrorStatusKind` 的状态事实投影已收敛到 `proxy_core_adapter::proxy_error_status_kind`，`proxy/error.rs` 不再维护 host-to-core 映射，`error_mapper` 也统一经 adapter 获取 status fact。
1018. Copilot `/copilot_internal/user` usage/quota DTO、usage JSON parse 和 `endpoints.api` 到默认 Copilot API base 的 fallback policy 已迁入 `proxy-core::copilot_model_map`，host `copilot_auth` 只保留 HTTP 调用、token/account 读取和 endpoint cache 写入。
1019. Copilot OAuth device polling 的错误码分类与 token 提前刷新 buffer policy 已迁入 `proxy-core::managed_account_auth`；host `copilot_auth` 只保留 HTTP 轮询、账号持久化和 core classification 到 `CopilotAuthError` 的映射。
1020. Codex OAuth device polling 的 HTTP status 分类、device code 默认过期/interval 安全余量、access token 过期时间计算和 token 提前刷新 buffer policy 已迁入 `proxy-core::managed_account_auth`；host `codex_oauth_auth` 只保留 reqwest 调用、token cache、refresh token 持久化和 status classification 到 `CodexOAuthError` 的映射。
1021. Codex OAuth device/code/refresh 请求 contract 已迁入 `proxy-core::managed_account_auth`，包括 OpenAI OAuth endpoint、client id、device JSON body、authorization/refresh form、verification URL 和固定错误文案；host `codex_oauth_auth` 只按 core request plan 执行 reqwest 并映射错误类型。
1022. 托管账号统一命令的 `auth_provider` 白名单与 unsupported provider 错误文案已迁入 `proxy-core::managed_account_auth::ensure_managed_auth_provider`；Tauri `commands/auth.rs` 只消费 core validation 结果并分派到 Copilot/Codex OAuth manager。
1023. 托管账号统一命令的 account/status/device-code response DTO 与默认账号标记规则已迁入 `proxy-core::managed_account_auth`；Tauri `commands/auth.rs` 只把 Copilot/Codex OAuth manager 返回的账号事实投影给 core response helper。
1024. Copilot/Codex OAuth 托管账号的默认账号 fallback 与对外账号列表排序策略已迁入 `proxy-core::managed_account_auth`；host manager 只把本地账号存储投影为 core candidate/sort key，继续负责读写磁盘、token refresh 和 manager 状态锁。
1025. Codex OAuth `id_token/access_token` claims 到 `account_id/email` 的提取优先级已迁入 `proxy-core::managed_account_auth`；host `codex_oauth_auth` 只保留 JWT payload 的 base64url decode 与 token HTTP/持久化流程。
1026. 旧 Copilot/Codex OAuth Tauri 命令复用的 account/device-code/status DTO 已迁入 `proxy-core::managed_account_auth`；host `copilot_auth`/`codex_oauth_auth` 只保留本地账号存储到 core DTO 的投影。
1027. 旧 Copilot/Codex OAuth status 的 `authenticated` 与兼容 `username` 组装规则已迁入 `proxy-core::managed_account_auth`；host manager 只提供账号列表、默认账号、迁移错误和默认 token 过期事实。
1028. channel-key runtime lookup 已删除 adapter-local `channel_key_value_from_runtime_candidate` 一行 facade；CC Switch runtime source 直接消费 core-selected `ChannelKeyRuntimeCandidate` 并投影 key value，减少中转宿主复制无意义 wrapper 的机会。
1029. channel-auth missing key 错误不再经由 adapter-local `channel_key_auth_error` 别名；host runtime source 直接使用 `proxy-core::provider_auth::channel_auth_profile_missing_key_error`，避免中转宿主复制纯错误包装函数。
1030. channel reachability probe 删除 `channel_test_app_type_from_probe_request` / `channel_test_provider_from_probe_source` 两个 adapter-local wrapper；DB-backed probe source 直接 parse host app type 并调用 core error helper，把中转宿主需要复制的 surface 缩到实际 IO 边界。
1031. channel health reset 删除 `channel_health_reset_from_plan` 一行 DTO wrapper；host health source 保留 DB lookup/router reset plan，但直接调用 `proxy-core::ports::channel_health_reset_from_parts` 构造对外响应。
1032. provider-router config source 删除 `circuit_breaker_config_from_router_config_result`、`circuit_failure_threshold_from_router_config_result` 和 `proxy_takeover_status_from_config_results` 三个 Result-fallback facade；adapter source 只负责把 host `AppError` 投影为缺省事实，core helper 继续负责 circuit/takeover DTO 策略。
1033. config source 删除 `app_summary_config_from_config_source` 与 `proxy_runtime_config_from_config_source` 两个 DTO wrapper；DB-backed source 直接调用 `AppSummaryConfig::new` 和 `proxy_runtime_config_from_proxy_config`，让 adapter 只保留实际 DB 读取与错误映射。
1034. model catalog provider source 删除 `provider_model_catalog_from_provider` wrapper；DB source 直接把 host `Provider.settings_config` 投影给 `proxy-core::model_fetch::provider_model_catalog_from_settings`，避免为单字段投影保留额外 adapter API。
1035. client model catalog source 删除 `client_model_catalog_raw_from_source` wrapper；adapter 仍按 core `ClientModelCatalogSource` 选择是否读取 Codex active catalog，但不再为一次 match 暴露额外 host API。
1036. Codex takeover model catalog 注入删除 `attach_codex_model_catalog_from_provider` 单调用点 mutation wrapper；takeover field applicator 直接投影 provider `modelCatalog`，值级合并继续复用 `codex_live_settings_with_model_catalog`。
1037. provider model catalog 原始值删除 `provider_model_catalog_raw_value` 单字段 getter；backfill/takeover source 在已持有 host `Provider` 的位置直接读取 `settings_config["modelCatalog"]`，避免为中转宿主暴露一层无状态 getter。
1038. Codex responses-to-chat forwarder source 删除 `forwarder_apply_codex_chat_upstream_model` / `forwarder_codex_chat_reasoning_options` 两个一跳 wrapper；request source 直接调用 provider 级 adapter API，避免在中转 forwarder 内部复制额外命名层。
1039. Gemini provider API key/base URL 删除 `provider_gemini_api_key` / `provider_gemini_base_url` 单字段 getter；保留 auth/base-url 对外入口，但 adapter 内部直接使用 core settings extractor 读取 `Provider.settings_config`。
1040. Claude forwarder api_format 删除 `forwarder_claude_api_format` 一跳 wrapper；request source 和 `resolve_forwarder_claude_api_format` 直接复用 provider 级 adapter API，Copilot vendor 分流策略保持不变。
1041. forwarder provider URL facts 删除 `forwarder_is_full_url_provider` / `forwarder_is_github_copilot_upstream` 两个一跳 wrapper；adapter context 在已持有 provider/base_url 的位置直接调用 provider 级事实投影。
1042. forwarder provider adapter facts 删除 `forwarder_provider_adapter_name` 一跳 wrapper；adapter context 构造 facts 时直接读取 trait object name，外部 forwarder 仍只拿到聚合后的 adapter facts。
1043. forwarder provider transform 删除 `forwarder_provider_transform_required` / `forwarder_provider_transform_request` 两个一跳 wrapper；adapter context 直接调用 trait object 的 transform gate/request，外部 forwarder 仍经 request source。
1044. forwarder provider base URL 删除 `forwarder_provider_base_url` 一跳 wrapper；adapter context 和 stream check fallback 直接调用 trait object 的 `extract_base_url`，forwarder 仍经 URL facts。
1045. forwarder provider auth fallback 删除 `forwarder_provider_auth_info` / `forwarder_provider_auth_headers` 两个一跳 wrapper；adapter context 直接调用 trait object 的 auth/header 方法，forwarder 仍经 auth source。
1046. forwarder provider upstream URL 删除 `forwarder_provider_upstream_url` 一跳 wrapper；adapter context 直接调用 trait object 的 `build_url`，forwarder 仍经 request source 获取最终 URL plan。
1047. forwarder provider adapter registry 删除 `forwarder_provider_adapter_for_app` 一跳 wrapper；adapter context factory 和 stream check fallback 直接调用 provider registry，forwarder 仍只消费 context/source 入口。
1048. forwarder provider facts 删除 `forwarder_is_codex_oauth_provider` / `forwarder_bedrock_env_flag` 两个一跳 wrapper；request source 直接调用 provider 级 fact helper，forwarder 仍只消费 source 组装结果。
1049. forwarder custom User-Agent 删除 `forwarder_custom_user_agent_header` 一跳 wrapper；request source 直接调用 provider 级 UA helper，并用 request-parts 单测锁住最终 `user-agent` header。
1050. forwarder Anthropic rectifier gate 删除 `forwarder_uses_anthropic_rectifiers` 一跳 wrapper；request source 直接调用 provider 级 rectifier gate helper，forwarder 仍只消费 source 判定结果。
1051. forwarder Codex Responses→Chat gate 删除 `forwarder_should_convert_codex_responses_to_chat` 一跳 wrapper；request source transform plan 直接组合 Codex app gate 与 provider 级 endpoint predicate。
1052. forwarder Claude normalize 删除 `forwarder_claude_normalize_anthropic_messages` 一跳 wrapper；request source 直接调用 provider 级 normalize helper，body policy 行为由 request-source 单测覆盖。
1053. forwarder Claude request transform 删除 `forwarder_claude_transform_request_for_api_format` 一跳 wrapper；protocol state source 直接调用 provider 级 transform helper，session id 与 Gemini shadow 传递逻辑保持在 source 内。
1054. forwarder adapter facts 删除 `provider_adapter_name_is_claude` 单行 helper；`ForwarderAdapterFacts::from_adapter` 在 adapter context 边界内直接投影 Claude adapter fact，boundary forbidden marker 防止 helper 复活。
1055. channel-key auth profile 删除 cfg(test) DB convenience helper 与 borrowed runtime source；host 测试改为构造 `CcSwitchChannelKeyRuntimeSource` 后调用 `apply_channel_auth_profile_providers_from_source`，boundary 反向禁止 DB helper 回流。
1056. Codex media-prevention app gate 新增 core helper `should_apply_forwarder_media_prevention_for_app`；request source 只投影 `AppType -> AppKind` 后调用 core policy，boundary 禁止 adapter-local `AppType::Codex` gate 回流。
1057. `proxy-core::api::prelude` 补齐外部 channel 构造 helper 与 `ChannelAttemptPlan` 导出；新增 `tests/public_prelude.rs` 作为 crate 外集成烟测，只通过 public prelude 构造 services/engine 并调用 `ProxyEngine::handle`，验证独立中转模块最小集成路径。
1058. `proxy_core_host.rs` 新增兼容壳边界测试：`mod tests` 前只允许 `#[cfg(test)]` import/re-export，生产 services/runtime 装配继续归属 `proxy_core_adapter`，旧 host 模块保持 test-only。
1059. 删除 `proxy_engine_from_services` 一跳构造 helper；`ProxyState::proxy_engine` 直接在 adapter 边界内调用 `ProxyEngine::new`，新增 boundary marker 防止无语义 constructor facade 回流。
1060. 删除 `ProxyState` 对 `app_handle` / `failover_manager` 的重复 host 资源保留字段；`ManagedAccountRuntimeSource` 与 `FailoverSwitchScheduler` 仍在 adapter runtime 装配阶段持有所需 clone，新增 boundary marker 防止注入型 host 资源重新回流到 server state shape。
1061. 删除 `ToProxyCoreProviderSpec`、`ToProxyCoreChannelSpec`、`ToProxyCoreModelRoute`、`ToProxyCoreChannelModelRecord`、`ToProxyCoreChannelRecord` 五个单 impl DTO trait facade；转换入口保留为 adapter 直接函数，新增 boundary marker 防止 DTO 投影重新绕回扩展 trait。
1062. `ForwardAttempt::from_provider` 改为 `#[cfg(test)]`；生产 attempt 构造只保留 `from_core_selection` 路径，新增 boundary marker 防止 provider-only `ForwardAttempt` fallback 构造器重新进入生产 surface。
1063. 删除 `ForwardError.provider` host payload 与 forwarder 预规划入口上的过时 `#[allow(dead_code)]`：`forward_error_to_core_error` 只消费 `ProxyError` 分类，新增 boundary marker 防止 host `Provider` 实体重新挂回 forward error surface。
1064. `CcSwitchProxyServices` 的 no-runtime test fixture 构造链改为 `#[cfg(test)]`：`new`、`with_event_bus`、`DefaultRuntimeStatusSource` 和 `CcSwitchForwardPipeline::without_runtime` 不再属于生产 surface，生产服务容器只通过 `with_runtime` 装配可用 host runtime。
1065. 删除 `proxy/error_mapper.rs` 的 `map_proxy_error_to_status` 与 `get_error_message` dead-code facade；状态码和展示文案行为继续由 `proxy_core_adapter` 暴露的 core contract 测试覆盖，新增 boundary marker 防止纯错误策略 wrapper 回流。

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
| `crate::commands::{CodexOAuthState, CopilotAuthState}` | `proxy::managed_account_auth` 托管账号 token/runtime 读取 | 当前仍是 CC Switch host 适配层能力，后续应替换为外部宿主可注入的 `AuthProvider` |
| `crate::services::usage_stats` | `UsageLogger` 定价查询 | 用量记录无法替换为外部 sink |
| `crate::claude_desktop_config`, `crate::codex_config` | model list、gateway auth | 协议入口混入桌面配置文件细节 |

### 当前请求链路

```text
HTTP request
  -> server.rs route
  -> handlers.rs parse body / endpoint / query
  -> ProxyEngine::handle / management API helpers
       -> ProxyServices ports
       -> RouteResolver / ChannelSource / ProviderSource
       -> ForwardPipeline with RoutePlan
  -> RequestForwarder::forward_with_preplanned_attempts
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
  CcSwitchChannelHealthStore
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
    proxy-core/
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

当前 `RequestContext::new` 已通过 `CcSwitchConfigSource` 读取 app config，provider 选择由 `ProxyEngine`/route pipeline 负责，context 不再提前预选 provider。

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

当前 `ProviderRouter::select_provider_ids` 应继续拆成三部分：

- `ProviderSource` 只负责读取供应商/账号元数据。
- `ChannelSource` 负责读取可路由 channel，包括现有 provider 主 URL、`provider_endpoints` 投影出来的兼容 channel，以及未来新增的独立 channel 表。
- `RouteResolver` 负责按 app、接口、模型、group、优先级、权重、熔断、限流和 retry 策略生成尝试计划。

当前分支已先把 `ProviderRouter` 的 DB source 读取和健康持久化收进 `proxy_core_adapter`：router 生产代码不再直接调用 provider/channel/config/health 表的读写 API，也不再暴露 `Arc<Database>` 构造入口；provider selection 生产 API 只返回 provider id，完整 Provider record 的加载留在 host adapter。router 通过 `ProviderRouterProviderSource`、`ProviderRouterChannelSource`、`ProviderRouterConfigSource` 和 `ProviderRouterHealthStore` 四个可注入端口取得 provider failover id sources、current provider id source、channel route source、router config 和 health persistence 入口。后续真正拆 crate 时，应把这些 router 端口进一步映射到 core-facing `ProviderSource`/`ChannelSource`/`HealthStore` trait 实现，而不是让 core 持有 CC Switch `Database`。

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

当前迁移已先把 auto failover 开关启用 plan、空队列自动加入当前 provider 的决策、pending switch key 和 provider-switched source 常量抽到 core。CC Switch command/manager 继续负责队列 DB 写入、`switch_proxy_target`/`hot_switch_provider`、托盘菜单和 Tauri emit。

### 认证接口

当前 `forwarder.rs` 已不再直接依赖 Codex OAuth/Copilot token 刷新 manager，也不再直接读取 Copilot 动态 endpoint、live model list 或 model vendor 状态；这一步先用 `proxy::managed_account_auth` 把 refresh/read state 副作用收口。独立模块最终应把“获取/刷新可用 token”和“读取账号运行态能力”抽象成宿主可替换端口。

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

本轮已把 `proxy-core` 端口签名从单独的 `auth_profile` 参数推进到 `app + provider + channel + request` 上下文；`CcSwitchAuthProvider` 会把 app/provider/channel 事实放入 `AuthInfo.metadata`，外部中转实现因此可以按 channel 选择不同 key、账号或 token runtime。生产 header 组装也已让 `ForwarderAuthSource` 先调用 core `AuthProvider`：如果外部实现返回显式 `AuthInfo.headers`，默认 source 会直接使用这些 header；如果返回空 headers，则继续走 CC Switch 现有 provider adapter 与 managed-account fallback。channel-key 注入已改为在生成 `ForwardAttempt` 后通过 `ChannelKeyRuntimeSource` 处理；DB-backed 回归测试证明相同 `key_ref` 在不同 channel 下会按 `channel_id` 各自取 key，不会全局串用。

provider adapter 仍负责 CC Switch 默认 fallback 的 provider settings 到 header 转换，但 token 刷新和宿主账号状态读取不在 provider adapter 内完成。channel 可以指向同一个 provider 的不同 key/auth profile，必须避免把一个 channel 的 key 泄漏到另一个 channel。下一步是把 managed-account token 刷新、channel-key 轮询/随机和失败回退继续收敛到宿主可替换端口，直到 forwarder 不再需要理解 CC Switch 的 provider settings 鉴权细节。

本轮继续把 channel-key runtime candidate 的启用状态和确定性选择规则收进 `proxy-core::ports`：运行时候选保留 `key_value` 供认证注入，选择规则按 enabled 状态、`priority DESC`、`weight DESC`、`key_ref ASC` 收敛；DAO 只读取指定 `channel_id + key_ref` 候选并交给 adapter 投影到 core runtime candidate。后续把单 key 扩展为多 key、轮询 key、随机 key 或失败回退时，应复用同一候选选择入口，而不是让 DAO/forwarder 重新理解 key 状态和排序策略。

本轮还把 channel-key lookup 的闭包形态推进为 `proxy-core::ports::ChannelKeyRuntimeSource`：`apply_channel_auth_profile_providers_from_source` 只消费 core source contract，CC Switch 默认实现 `CcSwitchChannelKeyRuntimeSource` 负责 DB 查询、enabled candidate 选择和 key value 投影。后续外部中转宿主可以替换这个 source 来接 Vault/KMS/轮询 key 池或账号 runtime，而不需要改 auth profile 应用循环。

随后又把 `ChannelKeyRuntimeSource` 接入 `ProxyServices` 容器：core service catalog 现在显式包含 channel-key runtime source，CC Switch services 持有 DB-backed source 实现。外部中转宿主在组装独立 proxy module 时可以和 `AuthProvider`、`ProviderSource`、`ChannelSource` 一样注入自己的 key runtime，而不是依赖 CC Switch adapter 的辅助函数。

本轮继续让 `CcSwitchForwardPipeline` 持有并传递 `ChannelKeyRuntimeSource`：host forward runtime 生成 `ForwardAttempt` 时走 source-injected helper，DB convenience helper 已删除。这样后续把 forward pipeline 抽到独立中转宿主时，可以直接替换 channel key runtime，而不用把 CC Switch 的数据库读取路径带过去。

本轮继续把 managed-account token runtime 的日志/错误文案 contract 收进 `proxy-core::managed_account_auth`：core 统一生成 Copilot/Codex OAuth 的无 AppHandle、指定/默认账号取 token、成功和失败文本；`src/proxy/managed_account_auth.rs` 只保留 Tauri state 读取和 token 获取调用。这让外部中转宿主可以复用相同 runtime 反馈 contract，而不复制 CC Switch 桌面 host 的中文文案分支。

本轮继续把 Codex live/settings 的 JSON 形状契约收进 `proxy-core::ports`：auth 对象提取、live write parts、restore parts、live settings parts、snapshot parts 和 provider validation parts 都由 core 基于 `settings + category` 生成；`proxy_core_adapter` 只负责把 CC Switch `Provider` 拆成中立输入。这样后续中转迁移可以让不同地址绑定不同认证、接口和模型信息，同时不让宿主 adapter 重新承载 Codex 配置形状规则。

随后又把 provider settings validation 的 app 分派和 localized issue spec 收进 `proxy-core::ports`：core 基于 `AppKind + settings` 决定 Claude/OpenCode/OpenClaw/Hermes 是否要求 JSON object，并复用 Codex validation parts 输出 Codex config 文本；host 仍只负责执行 ClaudeDesktop/Gemini 这类宿主专属校验和实际 TOML 语义校验。

本轮再把默认 Live 导入时的 provider category 决策收进 `proxy-core::ports`：core 根据 Codex auth 登录材料、provider key 事实和 app kind 统一产出 `official/custom`，adapter 仅负责提取 CC Switch/Codex 专属 config.toml bearer-token 事实并创建本地 `Provider`。这让外部中转宿主可以复用相同 category 策略，同时避免 core 复制 Codex reserved model provider id 解析细节。

本轮继续把 Codex provider backfill 的 restore/strip 决策收进 `proxy-core::ports`：core 基于 provider category 和 auth 事实决定是否回填 provider bearer token、是否剥离统一会话注入；adapter 仍只负责调用宿主 `codex_config` 执行实际 token/TOML mutation。OAuth 登录材料判定也统一走 core，避免外部中转宿主在迁移时复制 CC Switch adapter 里的 Codex 认证分支。

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

旧 provider 主地址和 `provider_endpoints` 到 channel 的兼容迁移规划也已进入 core：`build_legacy_channel_migration_plan` 负责 primary/endpoint channel 投影、endpoint 稳定排序、normalized base URL 去重、priority/interface/model route 推导和 review 计数。CC Switch adapter 只把本地 `Provider`、`custom_endpoints`、当前 provider 事实装配成 core input；DAO 只负责读取 legacy provider 事实和写入 `proxy_channels`/models/health。

materialized channel 优先、空表才 fallback 到 legacy projection 的 source 选择规则已由 `channel_route_source_for_materialized_count` 固化，并经 `CcSwitchChannelSource::list_channel_records` 包装为 core-facing `ChannelSource` 入口；`ProviderRouter` 只接收 adapter 投影后的 `RouteResolveChannelInput` 路由字段，不再直接读取 materialized records、legacy preview、完整 `ProxyChannelRecord` DAO 形状或 router-local channel DTO。management channel specs/records 与 router dry-run route 输入现在复用同一个 core `ChannelRecord` 投影来源。

dry-run route 的 circuit-open 识别也继续收敛：`route_candidate_channel_circuit_keys` 负责把 `RouteResolveResponse` 的候选投影成 channel circuit lookup facts，`proxy_core_adapter::management_route_response_from_router_source` 负责调用 core resolver、向 `ProviderRouter` 查询已有 breaker 可用性并把 availability facts 交给 `apply_route_candidate_circuit_availability`，由 adapter/core 生成 rejected:circuit_open response mutation。

provider failover 的 circuit lookup 也继续收敛：`provider_failover_circuit_lookups` 负责保留 failover queue 顺序、标记 missing provider 并生成已配置 provider 的 circuit key；`proxy_core_adapter::provider_failover_sources_from_router_provider_source` 通过 core-facing `ProviderSource::list_providers` 读取 provider id facts，failover queue 已经通过 `RoutePolicySource::load_policy` 的 `RoutePolicy` raw contract 提取 failover provider ids 并投影为 lookup facts，`ProviderRouter` 只查询 live breaker 可用性，随后把 lookup availability facts 交给 `select_failover_provider_ids_from_router_lookup_availability` 投影为 selection candidates 并执行 core selection 策略，最终仍只返回 provider id 列表。

`ProviderRouter` 的配置源、provider 读取、健康持久化和 management dry-run route resolution 也已进一步收口：auto failover gate、circuit breaker config 和 failure threshold 读取已由 `CcSwitchProviderRouterConfigSource` 通过 core `ProxyConfigSource::load_app` 投影；provider id/current provider id 读取已由 `CcSwitchProviderRouterProviderSource` 通过 core `ProviderSource` 投影，failover queue ids 读取通过 core `RoutePolicySource` 投影，router provider source trait 改为 async 以消费 core source；channel health 写入由 router 组装 core `ChannelAttemptResult` fact，并经异步 health store 端口复用 `channel_health_attempt_db_update` 投影写入，仍可携带 router 传入的 app-specific failure threshold，provider health 写入已落到 core `ProviderAttemptResult` / `ProviderHealthStore` attempt 端口，状态推进仍由 DB 层调用 core `provider_health_update_from_input` 完成；channel health reset 由 router 在清 live circuit 后组装 app-scoped `ChannelHealthReset` fact，再交给 adapter helper 写入宿主持久层，管理端只有 channel id 的 reset 路径仍由 `CcSwitchChannelHealthStore` 查询 app 并复用 router reset；management route response 由 adapter 组合 core resolver 与 router circuit availability；DB-backed router 构造统一由 `provider_router_from_database` adapter factory 完成。router channel source 已改为持有 `CcSwitchChannelSource` 并通过 core `ChannelSource::list_channel_records` 生成 route 输入；live circuit breaker map、permit/half-open 状态机、breaker availability 查询和热更新已由 router 内的 `ProviderRoutingCircuitRuntime` 承接；`ProviderRouter` 保留现有公开方法与 `AppError` 兼容，但自身不再直接持有 breaker map。

auto failover 开关启用的计划也已收敛：`plan_auto_failover_toggle` 负责“接管未开启则拒绝”、“队列非空则切 P1”、“队列为空则自动加入当前 provider 并切换”的纯决策；Tauri command 只读取 config/queue/current provider、执行 DB 队列写入、调用 proxy service 切换、写回 config 并 emit core 事件 contract。

熔断 reset 后的恢复切回也已进一步收敛：`restored_provider_switchback_decision` 接收 failover queue position facts，返回是否切回以及日志所需的 restored/current sort_index；`reset_circuit_breaker` 只负责读取 DB 队列、查询 provider 名称并调用 `FailoverSwitchManager` 执行宿主副作用。

管理 API 查询类入口已基本收敛到 `ProxyEngine`：`list_proxy_providers`、`list_proxy_channels` 的 route-aware 分支、`list_proxy_groups` 和 `test_proxy_channel` 都只保留 HTTP path/query/body 提取与错误映射；固定 app catalog 已从 `proxy-core` 的端口默认实现移出，改由 adapter-owned `CcSwitchConfigSource` 显式提供；RoutePolicy 的 failover provider id 读取也已由 domain/routing helper 维护，engine 不再直接读 raw JSON 字段；managed-auth 的单测入口也已切到 runtime source surface；下一步应继续减少 host runtime 对 Tauri runtime smoke 覆盖和外部集成契约的隐性依赖。

CC Switch 桌面宿主通过 adapter-owned `CcSwitchModelCatalogProvider::load_client_catalog` 实现 Codex `model_catalog_json` 文件读取和 stale guard；外部宿主可以返回自己的模型目录。后续如需让 Codex `/v1/models` 完全使用 route-visible 目录，应在 core 内生成 Codex 兼容 raw catalog，而不是让 handler 重新拼装。

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

当前已迁移到 core 的 response pipeline 子能力包括响应头清理、响应体诊断、非流式 body 解压、timeout 选择、Claude transform 路由策略、Codex 代理错误 envelope 和 SSE 文本/聚合工具：`strip_hop_by_hop_response_headers` 移除 hop-by-hop 头和 `Connection` 点名扩展头，`strip_entity_headers_for_rebuilt_body` 移除重建 body 后失真的实体头，`prepare_rebuilt_json_response_headers` 负责转换后 JSON body 的实体/hop-by-hop/content-type 头重建，`json_proxy_response`/`rebuilt_json_proxy_response` 负责 JSON 响应的 header 重建、body 序列化与 `ProxyCoreResponse` 构造，`transformed_sse_response_headers` 负责转换后 SSE 固定响应头，`transformed_sse_proxy_response` 负责转换后 SSE 响应的固定 header、OK status 和 stream body `ProxyCoreResponse` 构造，`body_looks_like_sse`/`body_diagnostics_suffix`/`body_snippet` 负责未标记 SSE body 嗅探与错误现场摘要，`get_content_encoding`/`decompress_body`/`decode_response_body` 负责 gzip/x-gzip/deflate/br 的 content-encoding 判定与解压、未知编码/失败解码原样透传和成功解码后的 header 一致性处理，`resolve_response_timeout_config` 负责 failover-gated 非流式 body timeout 与流式 first-byte/idle timeout 选择规则，`should_use_claude_transform_streaming`/`should_aggregate_codex_oauth_responses_sse` 提供 Claude transform 的 streaming 与非流 SSE 聚合 core 策略，`codex_proxy_error_json`/`codex_upstream_error_to_response_error`/`normalize_codex_chat_error_body` 负责 Codex 转发层代理错误和 Chat 上游错误响应的 Responses 风格 envelope、上游错误体归一化、非 JSON 预览截断和 413 上游体积限制提示，`strip_sse_field`/`take_sse_block`/`append_utf8_safe` 负责 SSE 字段提取、事件分帧和跨 chunk UTF-8 拼接，`SseEventScanner` 负责流式 data 行扫描、`[DONE]` 判定和可选 JSON parse，`SseUsageAccumulator` 负责 usage 事件缓存、首个被收集事件计时和 finish-once 防重入，`claude_stream_usage_event_filter`/`openai_stream_usage_event_filter`/`codex_stream_usage_event_filter`/`gemini_stream_usage_event_filter` 负责热路径 usage 事件预过滤，`TokenUsage` 及其 Claude/OpenAI/Codex/Gemini usage JSON parser 与 stream model extractor 负责协议用量解析和模型归因，`usage_tokens_from_token_usage`/`normalize_usage_models`/`normalize_error_usage_models`/`token_usage_from_usage_record`/`resolve_usage_record_pricing_models`/`transformed_response_usage` 负责 `UsageRecord` 的 token bucket 映射、模型归因、pricing model 选择和转换后非流响应 usage 归因规则，`StreamingResponseUsageRecord::missing_usage_log_message` 与 `NonStreamingResponseUsageRecord::log_event` 负责 usage 缺失/非 JSON body 的诊断消息投影，`chat_sse_to_response_value`/`responses_sse_to_response_value` 负责错标 SSE 非流式兜底聚合。body 读取、日志落地和 app-specific 响应转换仍在 host 层，`proxy::response_adapter` 集中承接 Axum/旧 `hyper_client::ProxyResponse` transport 桥接，`proxy::error_mapper` 集中承接 core/host error 映射、response parse 专用错误适配、response build error 映射、response transform error 映射和 Codex proxy error body 分类，`proxy_core_adapter::AxumResponseBuildErrorContext` 集中承接 Axum response builder 失败上下文投影，`proxy_core_adapter::upstream_response_parse_failure_log_message` 集中承接解析失败日志投影，`proxy_core_adapter::log_unlabeled_sse_fallback_event` 集中承接 fallback event 日志分发，`proxy_core_adapter::codex_chat_error_proxy_response` 集中承接 Codex Chat error normalization warning 与 neutral error response 构造，`proxy_core_adapter::provider_claude_transform_streaming_decision` 集中承接 Claude transform 的 streaming/聚合运行时决策和 Codex OAuth Responses SSE 非流聚合例外，`proxy_core_adapter::codex_chat_transform_streaming_decision` 集中承接 Codex Chat->Responses transform 的 streaming/错标 SSE 聚合运行时决策，`proxy_core_adapter::provider_claude_transform_sse_for_api_format` 集中承接 Claude 流式 OpenAI Chat/Responses/Gemini Native 响应转换分发与 Gemini rectifier 日志回调，`proxy_core_adapter::transform_codex_chat_response_with_history` 集中承接 Codex Chat 非流式 Chat->Responses 转换和 history 记录，`proxy_core_adapter::transform_codex_chat_sse_with_history` 集中承接 Codex Chat 流式 Chat->Responses SSE 转换和 history 记录；后续应按纯逻辑先行、transport adapter 后置的顺序继续抽离。

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
| `provider_router.rs` | `engine/routing.rs` | 当前已通过 router 端 provider/channel/config/health 四个 focused port 注入 source/store，channel route 输入已切到 core `RouteResolveChannelInput`，config/provider/channel/route-policy source adapter 已分别通过 `CcSwitchConfigSource` / `ProviderSource` / `CcSwitchChannelSource` / `RoutePolicySource` 对齐 core `ProxyConfigSource` / `ProviderSource` / `ChannelSource` / `RoutePolicySource`，provider health 写入已对齐 core `ProviderHealthStore`/`ProviderAttemptResult`，channel health 写入已改为异步传递 core `ChannelAttemptResult` 且保留 router failure threshold，channel reset 已改为传递 app-scoped `ChannelHealthReset` fact，DB-backed 构造统一在 host adapter factory；live breaker map 已由 `ProviderRoutingCircuitRuntime` 承接，下一步评估是否可在不引入 router/store 环的前提下完全实现 core `ChannelHealthStore` |
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

1. `CcSwitchConfigSource` 作为 adapter-owned 端口包装现有 DB/settings 读取。
2. `CcSwitchProviderSource` 作为 adapter-owned 端口包装 provider/current provider 读取。
3. `CcSwitchChannelSource` 作为 adapter-owned 端口包装 provider 主 URL、`provider_endpoints` 和未来 channel 表读取。
4. `CcSwitchRoutePolicySource` 包装 failover queue、group 和优先级/权重策略；`CcSwitchRouteResolver` 包装 core route plan 与 management dry-run route resolution。
5. `CcSwitchChannelHealthStore` 包装 channel health 写入；兼容期可同时写 provider health 聚合。
6. `CcSwitchUsageSink` 作为 adapter-owned 端口包装 `UsageLogger`；写入必须使用完整 `UsageRecord`，不能用简化 hint 直接写账单。
7. `CcSwitchEventSink` 作为 adapter-owned 端口包装 `ProxyEventBus`，再由宿主决定是否转发到 Tauri/UI/托盘。
8. `CcSwitchAuthProvider` 作为 adapter-owned 端口包装 provider config auth profile 到 `AuthInfo` 的默认投影；Codex/Copilot OAuth token 和 channel-key 注入继续由 forward runtime adapter 渐进收敛。

验收：

- `RequestContext::new` 不再直接读 DB/settings，而是走接口；也不再提前调用 provider router 预选 provider。
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
cargo test --manifest-path src-tauri/crates/proxy-core/Cargo.toml
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
| OAuth token 刷新被移出 forwarder 后时序改变 | Copilot/Codex OAuth 请求失败 | `managed_account_auth` 覆盖非托管透传、缺少 AppHandle 的错误路径和 Copilot runtime 无 AppHandle skip；`ManagedAccountRuntimeSource` contract 已覆盖 provider account 绑定、Codex account 回传和 session header gate；后续 `AuthProvider` 继续补 token 缓存、刷新、失败回退测试 |
| Live 接管逻辑误入核心 | 独立模块仍不可复用 | import 防线和 code review checklist 强制拦截 |
| 一次性移动 4.5 万行导致冲突大 | 难 review、难回滚 | 按端口、engine、transport、crate 分阶段小提交 |
| channel 与 provider 健康边界混淆 | 一个地址失败误伤同 provider 其他地址 | 熔断、健康、auto-ban 全部以 `channel_id` 为主键，provider 只做聚合展示 |
| 模型映射歧义 | 请求被发往不支持的模型或计价错误 | `RouteResolver` 输出淘汰原因；`/proxy/v1/route/resolve` 支持 dry-run；usage 记录 public/upstream/pricing model |
| channel key 串用 | 多地址/多账号时认证泄漏 | `AuthProvider::resolve_auth` 已接收 app/provider/channel/request 上下文；DB-backed 测试确保相同 `key_ref` 会按 `channel_id` 分别解析，后续迁移 header 组装时必须保留该隔离 |
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

- adapter-owned `CcSwitchProviderSource` 与现有 DB provider 表兼容。
- adapter-owned `CcSwitchChannelSource` 可以从 provider 主 URL、`provider_endpoints` 和新 channel 表生成一致候选。
- 旧配置迁移 dry-run 可以输出新增、重复、需人工确认的 channel。
- adapter-owned `CcSwitchUsageSink` 写入 `proxy_request_logs` 字段完整，`proxy_core_host` 只装配 sink。
- adapter-owned `CcSwitchEventSink` 可以驱动托盘和前端事件，`proxy_core_host` 只装配事件总线。
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

- `proxy-core` 可以独立 `cargo test`。
- `proxy-core` 不依赖 Tauri、SQLite Database、CC Switch settings/config/service modules。
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
