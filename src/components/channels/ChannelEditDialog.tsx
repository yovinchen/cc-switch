import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Plus, Trash2, Loader2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { extractErrorMessage } from "@/utils/errorUtils";
import {
  useCreateChannel,
  useReplaceChannelModels,
  useUpdateChannel,
} from "@/lib/query/channels";
import type {
  ChannelModelWriteRequest,
  ChannelRecord,
  InterfaceKind,
} from "@/types/channel";

const INTERFACE_KINDS: InterfaceKind[] = [
  "anthropic_messages",
  "openai_chat_completions",
  "openai_responses",
  "gemini_native",
  "gemini_openai_compatible",
];

const APP_TYPES = ["claude", "codex", "gemini"] as const;

interface ModelRow {
  publicModel: string;
  upstreamModel: string;
  pricingModel: string;
}

interface Props {
  mode: "create" | "edit";
  channel?: ChannelRecord;
  providerOptions: { id: string; appType: string }[];
  onClose: () => void;
}

export function ChannelEditDialog({
  mode,
  channel,
  providerOptions,
  onClose,
}: Props) {
  const { t } = useTranslation();
  const createChannel = useCreateChannel();
  const updateChannel = useUpdateChannel();
  const replaceModels = useReplaceChannelModels();

  const [name, setName] = useState(channel?.name ?? "");
  const [providerId, setProviderId] = useState(
    channel?.providerId ?? providerOptions[0]?.id ?? "",
  );
  const [appType, setAppType] = useState(channel?.appType ?? "claude");
  const [baseUrl, setBaseUrl] = useState(channel?.baseUrl ?? "");
  const [interfaceKind, setInterfaceKind] = useState<InterfaceKind>(
    channel?.interfaceKind ?? "anthropic_messages",
  );
  const [groups, setGroups] = useState(
    (channel?.groups ?? ["default"]).join(","),
  );
  const [priority, setPriority] = useState(String(channel?.priority ?? 0));
  const [weight, setWeight] = useState(String(channel?.weight ?? 100));
  const [status, setStatus] = useState(channel?.status ?? "enabled");
  const [models, setModels] = useState<ModelRow[]>(
    (channel?.models ?? []).map((m) => ({
      publicModel: m.publicModel,
      upstreamModel: m.upstreamModel,
      pricingModel: m.pricingModel ?? "",
    })),
  );
  const [saving, setSaving] = useState(false);

  const parseGroups = () =>
    groups
      .split(",")
      .map((g) => g.trim())
      .filter(Boolean);

  const buildModels = (): ChannelModelWriteRequest[] =>
    models
      .filter((m) => m.publicModel.trim() && m.upstreamModel.trim())
      .map((m) => ({
        publicModel: m.publicModel.trim(),
        upstreamModel: m.upstreamModel.trim(),
        pricingModel: m.pricingModel.trim() || null,
      }));

  const validate = (): string | null => {
    if (!name.trim())
      return t("channel.form.required", { field: t("channel.form.name") });
    if (!baseUrl.trim())
      return t("channel.form.required", { field: t("channel.form.baseUrl") });
    if (mode === "create" && !providerId.trim())
      return t("channel.form.required", { field: t("channel.form.provider") });
    return null;
  };

  const handleSave = async () => {
    const error = validate();
    if (error) {
      toast.error(error);
      return;
    }
    setSaving(true);
    try {
      if (mode === "create") {
        await createChannel.mutateAsync({
          providerId: providerId.trim(),
          appType,
          name: name.trim(),
          status,
          baseUrl: baseUrl.trim(),
          interfaceKind,
          groups: parseGroups(),
          priority: Number(priority) || 0,
          weight: Number(weight) || 100,
          models: buildModels(),
        });
        toast.success(t("channel.toast.created"));
      } else if (channel) {
        await updateChannel.mutateAsync({
          channelId: channel.id,
          request: {
            name: name.trim(),
            status,
            baseUrl: baseUrl.trim(),
            interfaceKind,
            groups: parseGroups(),
            priority: Number(priority) || 0,
            weight: Number(weight) || 100,
          },
        });
        // 模型映射通过独立接口替换。
        await replaceModels.mutateAsync({
          channelId: channel.id,
          request: { models: buildModels() },
        });
        toast.success(t("channel.toast.updated"));
      }
      onClose();
    } catch (err) {
      toast.error(extractErrorMessage(err) || t("channel.toast.error"));
    } finally {
      setSaving(false);
    }
  };

  const updateModel = (index: number, patch: Partial<ModelRow>) => {
    setModels((prev) =>
      prev.map((m, i) => (i === index ? { ...m, ...patch } : m)),
    );
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-2xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {mode === "create"
              ? t("channel.form.createTitle")
              : t("channel.form.editTitle")}
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>{t("channel.form.name")}</Label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t("channel.form.namePlaceholder")}
              />
            </div>
            <div className="space-y-1.5">
              <Label>{t("channel.form.status")}</Label>
              <Select value={status} onValueChange={setStatus}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="enabled">
                    {t("channel.status.enabled")}
                  </SelectItem>
                  <SelectItem value="manual_disabled">
                    {t("channel.status.manualDisabled")}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          {mode === "create" && (
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-1.5">
                <Label>{t("channel.form.provider")}</Label>
                {providerOptions.length > 0 ? (
                  <Select value={providerId} onValueChange={setProviderId}>
                    <SelectTrigger>
                      <SelectValue
                        placeholder={t("channel.form.providerPlaceholder")}
                      />
                    </SelectTrigger>
                    <SelectContent>
                      {providerOptions.map((p) => (
                        <SelectItem key={p.id} value={p.id}>
                          {p.id}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <Input
                    value={providerId}
                    onChange={(e) => setProviderId(e.target.value)}
                    placeholder={t("channel.form.providerPlaceholder")}
                  />
                )}
              </div>
              <div className="space-y-1.5">
                <Label>{t("channel.form.appType")}</Label>
                <Select value={appType} onValueChange={setAppType}>
                  <SelectTrigger>
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
              </div>
            </div>
          )}

          <div className="space-y-1.5">
            <Label>{t("channel.form.baseUrl")}</Label>
            <Input
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder={t("channel.form.baseUrlPlaceholder")}
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>{t("channel.form.interfaceKind")}</Label>
              <Select
                value={interfaceKind}
                onValueChange={(v) => setInterfaceKind(v as InterfaceKind)}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {INTERFACE_KINDS.map((kind) => (
                    <SelectItem key={kind} value={kind}>
                      {kind}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label>{t("channel.form.groups")}</Label>
              <Input
                value={groups}
                onChange={(e) => setGroups(e.target.value)}
                placeholder={t("channel.form.groupsPlaceholder")}
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>{t("channel.form.priority")}</Label>
              <Input
                type="number"
                value={priority}
                onChange={(e) => setPriority(e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {t("channel.form.priorityHint")}
              </p>
            </div>
            <div className="space-y-1.5">
              <Label>{t("channel.form.weight")}</Label>
              <Input
                type="number"
                value={weight}
                onChange={(e) => setWeight(e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {t("channel.form.weightHint")}
              </p>
            </div>
          </div>

          {/* Model mapping rows */}
          <div className="space-y-2 border-t border-border/50 pt-4">
            <div className="flex items-center justify-between">
              <Label>{t("channel.models.title")}</Label>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  setModels((prev) => [
                    ...prev,
                    { publicModel: "", upstreamModel: "", pricingModel: "" },
                  ])
                }
              >
                <Plus className="h-4 w-4" />
                {t("channel.models.addRow")}
              </Button>
            </div>
            {models.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                {t("channel.models.empty")}
              </p>
            ) : (
              <div className="space-y-2">
                {models.map((row, index) => (
                  <div key={index} className="flex items-center gap-2">
                    <Input
                      className="flex-1"
                      value={row.publicModel}
                      onChange={(e) =>
                        updateModel(index, { publicModel: e.target.value })
                      }
                      placeholder={t("channel.models.publicModel")}
                    />
                    <Input
                      className="flex-1"
                      value={row.upstreamModel}
                      onChange={(e) =>
                        updateModel(index, { upstreamModel: e.target.value })
                      }
                      placeholder={t("channel.models.upstreamModel")}
                    />
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      onClick={() =>
                        setModels((prev) => prev.filter((_, i) => i !== index))
                      }
                    >
                      <Trash2 className="h-4 w-4 text-destructive" />
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={saving}>
            {t("channel.form.cancel")}
          </Button>
          <Button onClick={() => void handleSave()} disabled={saving}>
            {saving && <Loader2 className="h-4 w-4 animate-spin" />}
            {t("channel.form.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
