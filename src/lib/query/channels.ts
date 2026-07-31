import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { channelApi } from "@/lib/api/channels";
import type {
  ChannelKeyPatchRequest,
  ChannelKeyWriteRequest,
  ChannelModelsReplaceRequest,
  ChannelPatchRequest,
  ChannelTestRequest,
  ChannelWriteRequest,
  RouteResolveRequest,
} from "@/types/channel";

// ========== Query keys ==========

const channelKeys = {
  all: ["channels"] as const,
  list: (appType?: string) => ["channels", "list", appType ?? "all"] as const,
  detail: (channelId: string) => ["channels", "detail", channelId] as const,
  models: (channelId: string) => ["channels", "models", channelId] as const,
  keys: (channelId: string) => ["channels", "keys", channelId] as const,
  breaker: (channelId: string) => ["channels", "breaker", channelId] as const,
  appChannels: (appType: string) => ["channels", "app", appType] as const,
  currentRoute: (appType: string) =>
    ["channels", "currentRoute", appType] as const,
  groups: (appType?: string) =>
    ["channels", "groups", appType ?? "all"] as const,
  migrationPreview: (appType: string) =>
    ["channels", "migrationPreview", appType] as const,
};

// ========== Channel list / detail ==========

export function useChannels(appType?: string) {
  return useQuery({
    queryKey: channelKeys.list(appType),
    queryFn: () => channelApi.listChannels(appType),
  });
}

export function useChannel(channelId: string | undefined) {
  return useQuery({
    queryKey: channelKeys.detail(channelId ?? ""),
    queryFn: () => channelApi.getChannel(channelId as string),
    enabled: !!channelId,
  });
}

// ========== Channel mutations ==========

export function useCreateChannel() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: ChannelWriteRequest) =>
      channelApi.createChannel(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: channelKeys.all });
    },
  });
}

export function useUpdateChannel() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      channelId,
      request,
    }: {
      channelId: string;
      request: ChannelPatchRequest;
    }) => channelApi.updateChannel(channelId, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: channelKeys.all });
    },
  });
}

export function useDeleteChannel() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (channelId: string) => channelApi.deleteChannel(channelId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: channelKeys.all });
    },
  });
}

// ========== Channel models ==========

export function useChannelModels(channelId: string | undefined) {
  return useQuery({
    queryKey: channelKeys.models(channelId ?? ""),
    queryFn: () => channelApi.listChannelModels(channelId as string),
    enabled: !!channelId,
  });
}

export function useReplaceChannelModels() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      channelId,
      request,
    }: {
      channelId: string;
      request: ChannelModelsReplaceRequest;
    }) => channelApi.replaceChannelModels(channelId, request),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: channelKeys.models(variables.channelId),
      });
      queryClient.invalidateQueries({ queryKey: channelKeys.all });
    },
  });
}

// ========== Channel keys ==========

export function useChannelKeys(channelId: string | undefined) {
  return useQuery({
    queryKey: channelKeys.keys(channelId ?? ""),
    queryFn: () => channelApi.listChannelKeys(channelId as string),
    enabled: !!channelId,
  });
}

export function useUpsertChannelKey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      channelId,
      keyRef,
      request,
    }: {
      channelId: string;
      keyRef: string;
      request: ChannelKeyWriteRequest;
    }) => channelApi.upsertChannelKey(channelId, keyRef, request),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: channelKeys.keys(variables.channelId),
      });
    },
  });
}

export function useUpdateChannelKey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      channelId,
      keyRef,
      request,
    }: {
      channelId: string;
      keyRef: string;
      request: ChannelKeyPatchRequest;
    }) => channelApi.updateChannelKey(channelId, keyRef, request),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: channelKeys.keys(variables.channelId),
      });
    },
  });
}

export function useDeleteChannelKey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      channelId,
      keyRef,
    }: {
      channelId: string;
      keyRef: string;
    }) => channelApi.deleteChannelKey(channelId, keyRef),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: channelKeys.keys(variables.channelId),
      });
    },
  });
}

// ========== Channel test ==========

export function useTestChannel() {
  return useMutation({
    mutationFn: ({
      channelId,
      request,
    }: {
      channelId: string;
      request?: ChannelTestRequest;
    }) => channelApi.testChannel(channelId, request ?? {}),
  });
}

// ========== App-scoped views ==========

export function useCurrentRoute(appType: string) {
  return useQuery({
    queryKey: channelKeys.currentRoute(appType),
    queryFn: () => channelApi.getCurrentRoute(appType),
    enabled: !!appType,
  });
}

export function useRouteGroups(appType?: string) {
  return useQuery({
    queryKey: channelKeys.groups(appType),
    queryFn: () => channelApi.listRouteGroups(appType),
  });
}

// ========== Legacy migration ==========

export function useMigrationPreview(appType: string, enabled = true) {
  return useQuery({
    queryKey: channelKeys.migrationPreview(appType),
    queryFn: () => channelApi.previewMigration(appType),
    enabled: !!appType && enabled,
  });
}

export function useMaterializeMigration() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (appType: string) => channelApi.materializeMigration(appType),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: channelKeys.all });
    },
  });
}

// ========== Route dry-run & breakers ==========

export function useResolveRoute() {
  return useMutation({
    mutationFn: (request: RouteResolveRequest) =>
      channelApi.resolveRoute(request),
  });
}

export function useChannelBreakerStats(channelId: string | undefined) {
  return useQuery({
    queryKey: channelKeys.breaker(channelId ?? ""),
    queryFn: () => channelApi.getChannelBreakerStats(channelId as string),
    enabled: !!channelId,
  });
}

export function useResetChannelBreaker() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (channelId: string) =>
      channelApi.resetChannelBreaker(channelId),
    onSuccess: (_data, channelId) => {
      queryClient.invalidateQueries({
        queryKey: channelKeys.breaker(channelId),
      });
    },
  });
}
