import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  Loader2,
  Plus,
  RefreshCw,
  Pencil,
  Trash2,
  Activity,
  KeyRound,
  Zap,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { extractErrorMessage } from "@/utils/errorUtils";
import {
  useChannels,
  useDeleteChannel,
  useResetChannelBreaker,
  useTestChannel,
} from "@/lib/query/channels";
import type { ChannelRecord } from "@/types/channel";
import { ChannelEditDialog } from "./ChannelEditDialog";
import { ChannelKeysDialog } from "./ChannelKeysDialog";
import { ChannelMigrationSection } from "./ChannelMigrationSection";
import { RouteResolvePreview } from "./RouteResolvePreview";

const APP_TYPES = ["claude", "codex", "gemini"] as const;

export function ChannelManagerPanel() {
  const { t } = useTranslation();
  const [appFilter, setAppFilter] = useState<string>("all");
  const appType = appFilter === "all" ? undefined : appFilter;

  const { data, isLoading, isError, refetch, isFetching } =
    useChannels(appType);
  const deleteChannel = useDeleteChannel();
  const resetBreaker = useResetChannelBreaker();
  const testChannel = useTestChannel();

  const [editTarget, setEditTarget] = useState<ChannelRecord | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [keysTarget, setKeysTarget] = useState<ChannelRecord | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ChannelRecord | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);

  const channels = useMemo(() => data?.channels ?? [], [data]);

  // 从现有 channel 提取供应商选项，供手动新增表单复用（无需额外桥接 provider 命令）。
  const providerOptions = useMemo(() => {
    const map = new Map<string, { id: string; appType: string }>();
    for (const ch of channels) {
      if (!map.has(ch.providerId)) {
        map.set(ch.providerId, { id: ch.providerId, appType: ch.appType });
      }
    }
    return Array.from(map.values());
  }, [channels]);

  const handleDelete = async () => {
    if (!deleteTarget) return;
    try {
      await deleteChannel.mutateAsync(deleteTarget.id);
      toast.success(t("channel.toast.deleted"));
    } catch (error) {
      toast.error(extractErrorMessage(error) || t("channel.toast.error"));
    } finally {
      setDeleteTarget(null);
    }
  };

  const handleTest = async (channel: ChannelRecord) => {
    setTestingId(channel.id);
    try {
      const result = await testChannel.mutateAsync({ channelId: channel.id });
      if (result.success) {
        toast.success(
          `${t("channel.test.success")}${
            result.latencyMs != null
              ? ` · ${t("channel.test.latency", { ms: result.latencyMs })}`
              : ""
          }`,
        );
      } else {
        toast.error(
          `${t("channel.test.failed")}${
            result.failureReason ? ` · ${result.failureReason}` : ""
          }`,
        );
      }
    } catch (error) {
      toast.error(extractErrorMessage(error) || t("channel.toast.error"));
    } finally {
      setTestingId(null);
    }
  };

  const handleResetBreaker = async (channel: ChannelRecord) => {
    try {
      await resetBreaker.mutateAsync(channel.id);
      toast.success(t("channel.toast.breakerReset"));
    } catch (error) {
      toast.error(extractErrorMessage(error) || t("channel.toast.error"));
    }
  };

  const statusLabel = (status: string) => {
    if (status === "enabled") return t("channel.status.enabled");
    if (status === "manual_disabled") return t("channel.status.manualDisabled");
    return t("channel.status.disabled");
  };

  return (
    <div className="space-y-6">
      <div>
        <p className="text-sm text-muted-foreground">
          {t("channel.description")}
        </p>
      </div>

      {/* Toolbar */}
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <Select value={appFilter} onValueChange={setAppFilter}>
          <SelectTrigger className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t("channel.filter.allApps")}</SelectItem>
            {APP_TYPES.map((app) => (
              <SelectItem key={app} value={app}>
                {app}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => void refetch()}
            disabled={isFetching}
          >
            <RefreshCw
              className={`h-4 w-4 ${isFetching ? "animate-spin" : ""}`}
            />
            {t("channel.actions.refresh")}
          </Button>
          <Button size="sm" onClick={() => setCreateOpen(true)}>
            <Plus className="h-4 w-4" />
            {t("channel.actions.add")}
          </Button>
        </div>
      </div>

      {/* List */}
      {isLoading ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground py-8 justify-center">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t("channel.loading")}
        </div>
      ) : isError ? (
        <div className="p-4 rounded-lg bg-destructive/10 border border-destructive/20 text-sm text-destructive">
          {t("channel.loadError")}
        </div>
      ) : channels.length === 0 ? (
        <div className="p-4 rounded-lg bg-muted/40 border border-border/50 text-sm text-muted-foreground">
          {t("channel.empty")}
        </div>
      ) : (
        <div className="rounded-lg border border-border/50 overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("channel.columns.name")}</TableHead>
                <TableHead>{t("channel.columns.baseUrl")}</TableHead>
                <TableHead>{t("channel.columns.interface")}</TableHead>
                <TableHead className="text-center">
                  {t("channel.columns.models")}
                </TableHead>
                <TableHead className="text-center">
                  {t("channel.columns.priority")}
                </TableHead>
                <TableHead className="text-center">
                  {t("channel.columns.weight")}
                </TableHead>
                <TableHead>{t("channel.columns.status")}</TableHead>
                <TableHead className="text-right">
                  {t("channel.columns.actions")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {channels.map((channel) => (
                <TableRow key={channel.id}>
                  <TableCell className="font-medium">
                    <div className="flex items-center gap-2">
                      {channel.name}
                      {channel.needsReview && (
                        <Badge variant="outline" className="text-amber-600">
                          {t("channel.needsReview")}
                        </Badge>
                      )}
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {channel.sourceKind === "legacy_projection"
                        ? t("channel.sourceLegacy")
                        : t("channel.sourceMaterialized")}
                    </div>
                  </TableCell>
                  <TableCell
                    className="max-w-[220px] truncate text-xs"
                    title={channel.baseUrl}
                  >
                    {channel.baseUrl}
                  </TableCell>
                  <TableCell className="text-xs">
                    {channel.interfaceKind}
                  </TableCell>
                  <TableCell className="text-center">
                    {channel.models.length}
                  </TableCell>
                  <TableCell className="text-center">
                    {channel.priority}
                  </TableCell>
                  <TableCell className="text-center">
                    {channel.weight}
                  </TableCell>
                  <TableCell>
                    <Badge
                      variant={
                        channel.status === "enabled" ? "default" : "secondary"
                      }
                    >
                      {statusLabel(channel.status)}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center justify-end gap-1">
                      <Button
                        variant="ghost"
                        size="icon"
                        title={t("channel.actions.test")}
                        onClick={() => void handleTest(channel)}
                        disabled={testingId === channel.id}
                      >
                        {testingId === channel.id ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Activity className="h-4 w-4" />
                        )}
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        title={t("channel.actions.manageKeys")}
                        onClick={() => setKeysTarget(channel)}
                      >
                        <KeyRound className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        title={t("channel.actions.resetBreaker")}
                        onClick={() => void handleResetBreaker(channel)}
                      >
                        <Zap className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        title={t("channel.actions.edit")}
                        onClick={() => setEditTarget(channel)}
                      >
                        <Pencil className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        title={t("channel.actions.delete")}
                        onClick={() => setDeleteTarget(channel)}
                      >
                        <Trash2 className="h-4 w-4 text-destructive" />
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      {/* Legacy migration */}
      <ChannelMigrationSection />

      {/* Route dry-run */}
      <RouteResolvePreview />

      {/* Dialogs */}
      {createOpen && (
        <ChannelEditDialog
          mode="create"
          providerOptions={providerOptions}
          onClose={() => setCreateOpen(false)}
        />
      )}
      {editTarget && (
        <ChannelEditDialog
          mode="edit"
          channel={editTarget}
          providerOptions={providerOptions}
          onClose={() => setEditTarget(null)}
        />
      )}
      {keysTarget && (
        <ChannelKeysDialog
          channel={keysTarget}
          onClose={() => setKeysTarget(null)}
        />
      )}
      <ConfirmDialog
        isOpen={!!deleteTarget}
        variant="destructive"
        title={t("channel.actions.delete")}
        message={t("channel.deleteConfirm", { name: deleteTarget?.name ?? "" })}
        confirmText={t("channel.actions.delete")}
        onConfirm={() => void handleDelete()}
        onCancel={() => setDeleteTarget(null)}
      />
    </div>
  );
}
