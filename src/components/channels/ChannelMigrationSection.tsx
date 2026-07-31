import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Loader2, DatabaseZap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { extractErrorMessage } from "@/utils/errorUtils";
import { channelApi } from "@/lib/api/channels";
import { useMaterializeMigration } from "@/lib/query/channels";
import type { ChannelMigrationPreviewResponse } from "@/types/channel";

const APP_TYPES = ["claude", "codex", "gemini"] as const;

export function ChannelMigrationSection() {
  const { t } = useTranslation();
  const [appType, setAppType] = useState<string>("claude");
  const [preview, setPreview] =
    useState<ChannelMigrationPreviewResponse | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const materialize = useMaterializeMigration();

  const handlePreview = async () => {
    setPreviewing(true);
    try {
      const result = await channelApi.previewMigration(appType);
      setPreview(result);
    } catch (err) {
      toast.error(extractErrorMessage(err) || t("channel.toast.error"));
    } finally {
      setPreviewing(false);
    }
  };

  const handleMaterialize = async () => {
    setConfirmOpen(false);
    try {
      const result = await materialize.mutateAsync(appType);
      toast.success(
        t("channel.migration.materializeSuccess", {
          channels: result.insertedChannels,
          models: result.insertedModels,
        }),
      );
      setPreview(null);
    } catch (err) {
      toast.error(extractErrorMessage(err) || t("channel.toast.error"));
    }
  };

  return (
    <div className="border-t border-border/50 pt-5 space-y-3">
      <div className="flex items-center gap-2">
        <DatabaseZap className="h-4 w-4 text-emerald-500" />
        <h4 className="text-sm font-semibold">
          {t("channel.migration.title")}
        </h4>
      </div>
      <p className="text-xs text-muted-foreground">
        {t("channel.migration.description")}
      </p>

      <div className="flex items-center gap-2 flex-wrap">
        <Select value={appType} onValueChange={setAppType}>
          <SelectTrigger className="w-32">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {APP_TYPES.map((app) => (
              <SelectItem key={app} value={app}>
                {app}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          variant="outline"
          size="sm"
          onClick={() => void handlePreview()}
          disabled={previewing}
        >
          {previewing && <Loader2 className="h-4 w-4 animate-spin" />}
          {t("channel.migration.preview")}
        </Button>
        <Button
          size="sm"
          onClick={() => setConfirmOpen(true)}
          disabled={!preview || preview.channels.length === 0}
        >
          {t("channel.migration.materialize")}
        </Button>
      </div>

      {preview &&
        (preview.channels.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            {t("channel.migration.empty")}
          </p>
        ) : (
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-xs">
              <Badge variant="secondary">
                {t("channel.migration.previewSummary", {
                  count: preview.channels.length,
                  duplicate: preview.duplicateCount,
                  review: preview.needsReviewCount,
                })}
              </Badge>
            </div>
            <ul className="text-xs text-muted-foreground space-y-1 max-h-40 overflow-y-auto">
              {preview.channels.map((ch) => (
                <li key={ch.id} className="flex items-center gap-2">
                  <span className="font-medium text-foreground">{ch.name}</span>
                  <span className="truncate">{ch.baseUrl}</span>
                  {ch.needsReview && (
                    <Badge variant="outline" className="text-amber-600">
                      {t("channel.needsReview")}
                    </Badge>
                  )}
                </li>
              ))}
            </ul>
          </div>
        ))}

      <ConfirmDialog
        isOpen={confirmOpen}
        variant="info"
        title={t("channel.migration.materialize")}
        message={t("channel.migration.description")}
        confirmText={t("channel.migration.materialize")}
        onConfirm={() => void handleMaterialize()}
        onCancel={() => setConfirmOpen(false)}
      />
    </div>
  );
}
