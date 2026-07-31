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
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { extractErrorMessage } from "@/utils/errorUtils";
import {
  useChannelKeys,
  useDeleteChannelKey,
  useUpsertChannelKey,
} from "@/lib/query/channels";
import type { ChannelRecord } from "@/types/channel";

interface Props {
  channel: ChannelRecord;
  onClose: () => void;
}

export function ChannelKeysDialog({ channel, onClose }: Props) {
  const { t } = useTranslation();
  const { data, isLoading } = useChannelKeys(channel.id);
  const upsertKey = useUpsertChannelKey();
  const deleteKey = useDeleteChannelKey();

  const [keyRef, setKeyRef] = useState("");
  const [keyValue, setKeyValue] = useState("");
  const [priority, setPriority] = useState("0");
  const [weight, setWeight] = useState("100");
  const [saving, setSaving] = useState(false);

  const keys = data?.keys ?? [];

  const handleAdd = async () => {
    if (!keyRef.trim()) {
      toast.error(t("channel.keys.keyRefRequired"));
      return;
    }
    if (!keyValue.trim()) {
      toast.error(t("channel.keys.keyValueRequired"));
      return;
    }
    setSaving(true);
    try {
      await upsertKey.mutateAsync({
        channelId: channel.id,
        keyRef: keyRef.trim(),
        request: {
          keyValue: keyValue.trim(),
          status: "enabled",
          priority: Number(priority) || 0,
          weight: Number(weight) || 100,
        },
      });
      toast.success(t("channel.keys.added"));
      setKeyRef("");
      setKeyValue("");
      setPriority("0");
      setWeight("100");
    } catch (err) {
      toast.error(extractErrorMessage(err) || t("channel.toast.error"));
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (ref: string) => {
    try {
      await deleteKey.mutateAsync({ channelId: channel.id, keyRef: ref });
      toast.success(t("channel.keys.deleted"));
    } catch (err) {
      toast.error(extractErrorMessage(err) || t("channel.toast.error"));
    }
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {t("channel.keys.title", { name: channel.name })}
          </DialogTitle>
        </DialogHeader>

        <p className="text-xs text-muted-foreground">
          {t("channel.keys.description")}
        </p>

        <div className="space-y-2 rounded-md border border-border/50 p-3">
          <div className="grid grid-cols-2 gap-2">
            <div className="space-y-1.5">
              <Label>{t("channel.keys.keyRef")}</Label>
              <Input
                value={keyRef}
                onChange={(e) => setKeyRef(e.target.value)}
                placeholder={t("channel.keys.keyRefPlaceholder")}
              />
            </div>
            <div className="space-y-1.5">
              <Label>{t("channel.keys.keyValue")}</Label>
              <Input
                type="password"
                value={keyValue}
                onChange={(e) => setKeyValue(e.target.value)}
                placeholder={t("channel.keys.keyValuePlaceholder")}
              />
            </div>
          </div>
          <div className="flex items-end gap-2">
            <div className="w-24 space-y-1.5">
              <Label>{t("channel.form.priority")}</Label>
              <Input
                type="number"
                value={priority}
                onChange={(e) => setPriority(e.target.value)}
              />
            </div>
            <div className="w-24 space-y-1.5">
              <Label>{t("channel.form.weight")}</Label>
              <Input
                type="number"
                value={weight}
                onChange={(e) => setWeight(e.target.value)}
              />
            </div>
            <Button
              className="ml-auto"
              onClick={() => void handleAdd()}
              disabled={saving}
            >
              {saving ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Plus className="h-4 w-4" />
              )}
              {t("channel.keys.add")}
            </Button>
          </div>
        </div>

        <div className="max-h-64 overflow-y-auto rounded-md border border-border/50">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("channel.keys.keyRef")}</TableHead>
                <TableHead>{t("channel.form.priority")}</TableHead>
                <TableHead>{t("channel.form.weight")}</TableHead>
                <TableHead className="w-12" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell colSpan={4} className="text-center">
                    <Loader2 className="mx-auto h-4 w-4 animate-spin" />
                  </TableCell>
                </TableRow>
              ) : keys.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={4}
                    className="text-center text-xs text-muted-foreground"
                  >
                    {t("channel.keys.empty")}
                  </TableCell>
                </TableRow>
              ) : (
                keys.map((key) => (
                  <TableRow key={key.keyRef}>
                    <TableCell className="font-mono text-xs">
                      {key.keyRef}
                    </TableCell>
                    <TableCell>{key.priority}</TableCell>
                    <TableCell>{key.weight}</TableCell>
                    <TableCell>
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => void handleDelete(key.keyRef)}
                      >
                        <Trash2 className="h-4 w-4 text-destructive" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("channel.form.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
