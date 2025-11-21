import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { DeepLinkImportRequest, deeplinkApi } from "@/lib/api/deeplink";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";

interface DeeplinkError {
  url: string;
  error: string;
}

export function DeepLinkImportDialog() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [request, setRequest] = useState<DeepLinkImportRequest | null>(null);
  const [isImporting, setIsImporting] = useState(false);
  const [isOpen, setIsOpen] = useState(false);

  useEffect(() => {
    // Listen for deep link import events
    const unlistenImport = listen<DeepLinkImportRequest>(
      "deeplink-import",
      (event) => {
        console.log("Deep link import event received:", event.payload);
        setRequest(event.payload);
        setIsOpen(true);
      },
    );

    // Listen for deep link error events
    const unlistenError = listen<DeeplinkError>("deeplink-error", (event) => {
      console.error("Deep link error:", event.payload);
      toast.error(t("deeplink.parseError"), {
        description: event.payload.error,
      });
    });

    return () => {
      unlistenImport.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, [t]);

  const handleImport = async () => {
    if (!request) return;

    setIsImporting(true);

    try {
      await deeplinkApi.importFromDeeplink(request);

      // Invalidate provider queries to refresh the list
      await queryClient.invalidateQueries({
        queryKey: ["providers", request.app],
      });

      toast.success(t("deeplink.importSuccess"), {
        description: t("deeplink.importSuccessDescription", {
          name: request.name,
        }),
      });

      setIsOpen(false);
      setRequest(null);
    } catch (error) {
      console.error("Failed to import provider from deep link:", error);
      toast.error(t("deeplink.importError"), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsImporting(false);
    }
  };

  const handleCancel = () => {
    setIsOpen(false);
    setRequest(null);
  };

  if (!request) return null;

  // Mask API key for display (show first 4 chars + ***)
  const maskedApiKey =
    request.apiKey.length > 4
      ? `${request.apiKey.substring(0, 4)}${"*".repeat(20)}`
      : "****";

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen} modal={true}>
      <DialogContent className="sm:max-w-[650px] z-[9999]">
        {/* 标题显式左对齐，避免默认居中样式影响 */}
        <DialogHeader className="text-left sm:text-left">
          <DialogTitle>{t("deeplink.confirmImport")}</DialogTitle>
          <DialogDescription>
            {t("deeplink.confirmImportDescription")}
          </DialogDescription>
        </DialogHeader>

        {/* 使用两列布局压缩内容 */}
        <div className="space-y-4 px-4 py-3">
          {/* 第一行：应用类型 + 供应商名称 */}
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1">
              <div className="font-medium text-xs text-muted-foreground uppercase">
                {t("deeplink.app")}
              </div>
              <div className="text-sm font-medium capitalize">
                {request.app}
              </div>
            </div>
            <div className="space-y-1">
              <div className="font-medium text-xs text-muted-foreground uppercase">
                {t("deeplink.providerName")}
              </div>
              <div className="text-sm font-medium truncate" title={request.name}>
                {request.name}
              </div>
            </div>
          </div>

          {/* 第二行：官网 + 端点 */}
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1">
              <div className="font-medium text-xs text-muted-foreground uppercase">
                {t("deeplink.homepage")}
              </div>
              <div className="text-xs break-all text-blue-600 dark:text-blue-400 line-clamp-2" title={request.homepage}>
                {request.homepage}
              </div>
            </div>
            <div className="space-y-1">
              <div className="font-medium text-xs text-muted-foreground uppercase">
                {t("deeplink.endpoint")}
              </div>
              <div className="text-xs break-all line-clamp-2" title={request.endpoint}>
                {request.endpoint}
              </div>
            </div>
          </div>

          {/* 第三行：API Key */}
          <div className="space-y-1">
            <div className="font-medium text-xs text-muted-foreground uppercase">
              {t("deeplink.apiKey")}
            </div>
            <div className="text-sm font-mono text-muted-foreground">
              {maskedApiKey}
            </div>
          </div>

          {/* 第四行：默认模型（如果有） */}
          {request.model && (
            <div className="space-y-1">
              <div className="font-medium text-xs text-muted-foreground uppercase">
                {t("deeplink.model")}
              </div>
              <div className="text-sm font-mono">{request.model}</div>
            </div>
          )}

          {/* Claude 专用模型字段（紧凑布局） */}
          {request.app === "claude" && (request.haikuModel || request.sonnetModel || request.opusModel) && (
            <div className="rounded-lg bg-blue-50 dark:bg-blue-900/20 p-3 space-y-2">
              <div className="font-medium text-xs text-blue-900 dark:text-blue-100 uppercase">
                {t("deeplink.claudeModels", "Claude 模型配置")}
              </div>

              <div className="grid grid-cols-3 gap-2 text-xs">
                {request.haikuModel && (
                  <div>
                    <span className="text-muted-foreground">Haiku:</span>
                    <div className="font-mono truncate" title={request.haikuModel}>
                      {request.haikuModel}
                    </div>
                  </div>
                )}

                {request.sonnetModel && (
                  <div>
                    <span className="text-muted-foreground">Sonnet:</span>
                    <div className="font-mono truncate" title={request.sonnetModel}>
                      {request.sonnetModel}
                    </div>
                  </div>
                )}

                {request.opusModel && (
                  <div>
                    <span className="text-muted-foreground">Opus:</span>
                    <div className="font-mono truncate" title={request.opusModel}>
                      {request.opusModel}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* 备注（如果有） */}
          {request.notes && (
            <div className="space-y-1">
              <div className="font-medium text-xs text-muted-foreground uppercase">
                {t("deeplink.notes")}
              </div>
              <div className="text-sm text-muted-foreground line-clamp-2" title={request.notes}>
                {request.notes}
              </div>
            </div>
          )}

          {/* 警告提示（紧凑版） */}
          <div className="rounded-lg bg-yellow-50 dark:bg-yellow-900/20 p-2 text-xs text-yellow-800 dark:text-yellow-200">
            {t("deeplink.warning")}
          </div>
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={handleCancel}
            disabled={isImporting}
          >
            {t("common.cancel")}
          </Button>
          <Button onClick={handleImport} disabled={isImporting}>
            {isImporting ? t("deeplink.importing") : t("deeplink.import")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
