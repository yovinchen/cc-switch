// 中转站（channel）相关类型定义
//
// 这些类型镜像 Rust proxy-core 的 camelCase 契约（见 crates/proxy-core/src/ports.rs），
// 通过 Tauri 命令桥接（src-tauri/src/commands/proxy_channel.rs）读写，字段与 HTTP
// 管理 API `/proxy/v1/*` 完全一致。

/** channel 支持的上游接口协议族 */
export type InterfaceKind =
  | "anthropic_messages"
  | "openai_chat_completions"
  | "openai_responses"
  | "gemini_native"
  | "gemini_openai_compatible"
  | string;

/** channel 状态 */
export type ChannelStatus = "enabled" | "disabled" | "manual_disabled" | string;

/** channel 来源：物化 channel 表 vs legacy provider 投影 */
export type ChannelRouteSource = "materialized_channels" | "legacy_projection";

/** 模型路由：对外模型名 -> 上游模型名的映射 */
export interface ChannelModelRecord {
  channelId: string;
  publicModel: string;
  upstreamModel: string;
  capabilities: unknown;
  pricingModel: string | null;
  requestOverrides: unknown;
  responseOverrides: unknown;
}

/** channel 记录（列表/详情返回） */
export interface ChannelRecord {
  id: string;
  providerId: string;
  appType: string;
  name: string;
  status: ChannelStatus;
  baseUrl: string;
  interfaceKind: InterfaceKind;
  authProfileRef: string | null;
  groups: string[];
  priority: number;
  weight: number;
  retryPolicy: unknown;
  healthPolicy: unknown;
  headerOverrides: unknown;
  paramOverrides: unknown;
  statusCodeMapping: unknown;
  tags: string[];
  metadata: unknown;
  sourceKind: string;
  sourceEndpointUrl: string | null;
  models: ChannelModelRecord[];
  needsReview: boolean;
  reviewReasons: string[];
}

/** channel key 记录（永远不含明文 key，只有 key_ref） */
export interface ChannelKeyRecord {
  channelId: string;
  keyRef: string;
  status: string;
  priority: number;
  weight: number;
  lastFailureAt: number | null;
}

/** 路由候选（route dry-run / app channels 路由模式） */
export interface ChannelRouteCandidate {
  channelId: string;
  providerId: string;
  channelName: string;
  baseUrl: string;
  interfaceKind: string;
  publicModel: string | null;
  upstreamModel: string | null;
  routeGroup: string;
  priority: number;
  weight: number;
  sourceKind: string;
}

/** 被淘汰的候选及原因 */
export interface ChannelRouteRejected {
  channelId: string;
  providerId: string;
  channelName: string;
  reasons: string[];
}

/** 当前生效的路由目标 */
export interface CurrentRouteTarget {
  appType: string;
  providerName: string;
  providerId: string;
  channelId: string | null;
  channelName: string | null;
  interfaceKind: string | null;
  publicModel: string | null;
  upstreamModel: string | null;
  pricingModel: string | null;
}

/** channel 连通性测试结果 */
export interface ChannelTestResponse {
  channelId: string;
  providerId: string;
  appType: string;
  channelName: string;
  baseUrl: string;
  interfaceKind: string;
  model: string | null;
  modelAvailable: boolean | null;
  success: boolean;
  status: string;
  message: string;
  latencyMs: number | null;
  httpStatus: number | null;
  testedAt: number;
  retryCount: number;
  failureReason: string | null;
}

// ---- 请求体 DTO（写入类命令） -----------------------------------------

export interface ChannelModelWriteRequest {
  publicModel: string;
  upstreamModel: string;
  capabilities?: unknown;
  pricingModel?: string | null;
  requestOverrides?: unknown;
  responseOverrides?: unknown;
}

/** 创建 channel 请求体 */
export interface ChannelWriteRequest {
  id?: string | null;
  providerId: string;
  appType: string;
  name: string;
  status?: ChannelStatus;
  baseUrl: string;
  interfaceKind: InterfaceKind;
  authProfileRef?: string | null;
  groups?: string[];
  priority?: number;
  weight?: number;
  retryPolicy?: unknown;
  healthPolicy?: unknown;
  headerOverrides?: unknown;
  paramOverrides?: unknown;
  statusCodeMapping?: unknown;
  tags?: string[];
  metadata?: unknown;
  models?: ChannelModelWriteRequest[];
}

/** 更新 channel 请求体（PATCH 语义，全部可选） */
export interface ChannelPatchRequest {
  name?: string;
  status?: ChannelStatus;
  baseUrl?: string;
  interfaceKind?: InterfaceKind;
  authProfileRef?: string | null;
  groups?: string[];
  priority?: number;
  weight?: number;
  retryPolicy?: unknown;
  healthPolicy?: unknown;
  headerOverrides?: unknown;
  paramOverrides?: unknown;
  statusCodeMapping?: unknown;
  tags?: string[];
  metadata?: unknown;
}

export interface ChannelModelsReplaceRequest {
  models: ChannelModelWriteRequest[];
}

export interface ChannelKeyWriteRequest {
  keyValue: string;
  status?: string;
  priority?: number;
  weight?: number;
}

export interface ChannelKeyPatchRequest {
  keyValue?: string;
  status?: string;
  priority?: number;
  weight?: number;
}

export interface ChannelTestRequest {
  model?: string | null;
  interfaceKind?: string | null;
}

export interface RouteResolveRequest {
  appType: string;
  requestedModel?: string | null;
  interfaceKind?: string | null;
  routeGroup?: string | null;
}

// ---- 响应包装 DTO ------------------------------------------------------

export interface ChannelListResponse {
  channels: ChannelRecord[];
}

export interface ChannelRecordResponse {
  channel: ChannelRecord;
}

export interface ChannelModelsResponse {
  models: ChannelModelRecord[];
}

export interface ChannelKeysResponse {
  keys: ChannelKeyRecord[];
}

export interface ChannelKeyRecordResponse {
  key: ChannelKeyRecord;
}

export interface ChannelDeleteResponse {
  channelId: string;
  deleted: boolean;
}

export interface ChannelKeyDeleteResponse {
  channelId: string;
  keyRef: string;
  deleted: boolean;
}

export interface ChannelHealthResetResponse {
  channelId: string;
  appType: string;
  reset: boolean;
}

export interface CircuitBreakerStats {
  state: string;
  consecutiveFailures?: number;
  [key: string]: unknown;
}

export interface ChannelBreakerStatsResponse {
  channelId: string;
  appType: string;
  stats: CircuitBreakerStats | null;
}

export interface ChannelMigrationPreviewResponse {
  appType: string;
  channels: ChannelRecord[];
  duplicateCount: number;
  needsReviewCount: number;
}

export interface ChannelMigrationMaterializeResponse {
  appType: string;
  previewedChannels: number;
  insertedChannels: number;
  insertedModels: number;
  insertedHealthRows: number;
  duplicateCount: number;
  needsReviewCount: number;
}

export interface RouteResolveResponse {
  appType: string;
  requestedModel: string | null;
  interfaceKind: string | null;
  routeGroup: string;
  source: ChannelRouteSource;
  candidates: ChannelRouteCandidate[];
  rejected: ChannelRouteRejected[];
}

export interface RouteGroupSummary {
  name: string;
  appTypes: string[];
  channelCount: number;
}

export interface RouteGroupListResponse {
  appType: string | null;
  sources: string[];
  groups: RouteGroupSummary[];
}

export interface CurrentRouteResponse {
  appType: string;
  target: CurrentRouteTarget | null;
  [key: string]: unknown;
}

/**
 * GET app channels 的返回是 untagged 联合：
 * - 列表模式（无路由过滤）：{ appType, source, channels }
 * - 路由 dry-run 模式（带 requestedModel + interfaceKind）：额外带 requestedModel / interfaceKind / routeGroup / rejected
 */
export interface AppChannelResponse {
  appType: string;
  source: string;
  channels: ChannelRecord[] | ChannelRouteCandidate[];
  requestedModel?: string | null;
  interfaceKind?: string | null;
  routeGroup?: string;
  rejected?: ChannelRouteRejected[];
}
