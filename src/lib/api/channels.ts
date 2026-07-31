import { invoke } from "@tauri-apps/api/core";
import type {
  AppChannelResponse,
  ChannelBreakerStatsResponse,
  ChannelDeleteResponse,
  ChannelHealthResetResponse,
  ChannelKeyDeleteResponse,
  ChannelKeyPatchRequest,
  ChannelKeyRecordResponse,
  ChannelKeysResponse,
  ChannelKeyWriteRequest,
  ChannelListResponse,
  ChannelMigrationMaterializeResponse,
  ChannelMigrationPreviewResponse,
  ChannelModelsReplaceRequest,
  ChannelModelsResponse,
  ChannelPatchRequest,
  ChannelRecordResponse,
  ChannelTestRequest,
  ChannelTestResponse,
  ChannelWriteRequest,
  CurrentRouteResponse,
  RouteGroupListResponse,
  RouteResolveRequest,
  RouteResolveResponse,
} from "@/types/channel";

/**
 * 中转站（channel）管理 API。
 *
 * 通过 Tauri 命令桥接到 `ProxyEngine`，等价于 HTTP `/proxy/v1/*` 管理接口。
 * channel 数据是数据库持久化的，即使本地代理服务未启动也可读写。
 */
export const channelApi = {
  // ========== Channel CRUD ==========

  async listChannels(appType?: string): Promise<ChannelListResponse> {
    return invoke("list_proxy_channels", { appType: appType ?? null });
  },

  async createChannel(
    request: ChannelWriteRequest,
  ): Promise<ChannelRecordResponse> {
    return invoke("create_proxy_channel", { request });
  },

  async getChannel(channelId: string): Promise<ChannelRecordResponse> {
    return invoke("get_proxy_channel", { channelId });
  },

  async updateChannel(
    channelId: string,
    request: ChannelPatchRequest,
  ): Promise<ChannelRecordResponse> {
    return invoke("update_proxy_channel", { channelId, request });
  },

  async deleteChannel(channelId: string): Promise<ChannelDeleteResponse> {
    return invoke("delete_proxy_channel", { channelId });
  },

  // ========== Channel models ==========

  async listChannelModels(channelId: string): Promise<ChannelModelsResponse> {
    return invoke("list_proxy_channel_models", { channelId });
  },

  async replaceChannelModels(
    channelId: string,
    request: ChannelModelsReplaceRequest,
  ): Promise<ChannelModelsResponse> {
    return invoke("replace_proxy_channel_models", { channelId, request });
  },

  // ========== Channel keys ==========

  async listChannelKeys(channelId: string): Promise<ChannelKeysResponse> {
    return invoke("list_proxy_channel_keys", { channelId });
  },

  async upsertChannelKey(
    channelId: string,
    keyRef: string,
    request: ChannelKeyWriteRequest,
  ): Promise<ChannelKeyRecordResponse> {
    return invoke("upsert_proxy_channel_key", { channelId, keyRef, request });
  },

  async updateChannelKey(
    channelId: string,
    keyRef: string,
    request: ChannelKeyPatchRequest,
  ): Promise<ChannelKeyRecordResponse> {
    return invoke("update_proxy_channel_key", { channelId, keyRef, request });
  },

  async deleteChannelKey(
    channelId: string,
    keyRef: string,
  ): Promise<ChannelKeyDeleteResponse> {
    return invoke("delete_proxy_channel_key", { channelId, keyRef });
  },

  // ========== Channel test ==========

  async testChannel(
    channelId: string,
    request: ChannelTestRequest = {},
  ): Promise<ChannelTestResponse> {
    return invoke("test_proxy_channel", { channelId, request });
  },

  // ========== App-scoped views ==========

  async listAppChannels(
    appType: string,
    options: {
      requestedModel?: string;
      interfaceKind?: string;
      routeGroup?: string;
    } = {},
  ): Promise<AppChannelResponse> {
    return invoke("list_proxy_app_channels", {
      appType,
      requestedModel: options.requestedModel ?? null,
      interfaceKind: options.interfaceKind ?? null,
      routeGroup: options.routeGroup ?? null,
    });
  },

  async getCurrentRoute(appType: string): Promise<CurrentRouteResponse> {
    return invoke("get_current_proxy_route", { appType });
  },

  async listRouteGroups(appType?: string): Promise<RouteGroupListResponse> {
    return invoke("list_proxy_route_groups", { appType: appType ?? null });
  },

  // ========== Legacy migration ==========

  async previewMigration(
    appType: string,
  ): Promise<ChannelMigrationPreviewResponse> {
    return invoke("preview_proxy_channel_migration", { appType });
  },

  async materializeMigration(
    appType: string,
  ): Promise<ChannelMigrationMaterializeResponse> {
    return invoke("materialize_proxy_channel_migration", { appType });
  },

  // ========== Route dry-run & breakers ==========

  async resolveRoute(
    request: RouteResolveRequest,
  ): Promise<RouteResolveResponse> {
    return invoke("resolve_proxy_route", { request });
  },

  async getChannelBreakerStats(
    channelId: string,
  ): Promise<ChannelBreakerStatsResponse> {
    return invoke("get_proxy_channel_breaker_stats", { channelId });
  },

  async resetChannelBreaker(
    channelId: string,
  ): Promise<ChannelHealthResetResponse> {
    return invoke("reset_proxy_channel_breaker", { channelId });
  },
};
