import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, Eye, EyeOff } from "lucide-react";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import type { ConfigImportRequest } from "@/lib/api/deeplink";

interface ConfigImportDialogProps {
  open: boolean;
  request: ConfigImportRequest | null;
  isSubmitting?: boolean;
  onClose: () => void;
  onConfirm: (request: ConfigImportRequest) => void | Promise<void>;
}

const SENSITIVE_KEY_PATTERN =
  /(\"?((?:api[_-]?key)|(?:auth[_-]?token)|(?:ANTHROPIC_AUTH_TOKEN)|(?:ANTHROPIC_API_KEY)|(?:GEMINI_API_KEY)|(?:GOOGLE_GEMINI_API_KEY)|(?:OPENAI_API_KEY)|(?:ACCESS_TOKEN)|(?:secret))\"?\s*[:=]\s*)(\"([^\"]*)\"|'([^']*)'|[^\s,}]+)/gi;

const decodeBase64Content = (data: string) => {
  const binary = atob(data);
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  return new TextDecoder().decode(bytes);
};

const encodeBase64Content = (content: string) => {
  const bytes = new TextEncoder().encode(content);
  const binary = Array.from(bytes, (byte) => String.fromCharCode(byte)).join(
    "",
  );
  return btoa(binary);
};

const maskValue = (value: string) => {
  if (!value) return "****";
  const trimmed = value.trim();
  if (trimmed.length <= 8) {
    return `${trimmed[0] ?? "*"}***${trimmed.slice(-1)}`;
  }
  return `${trimmed.slice(0, 4)}***${trimmed.slice(-4)}`;
};

const maskSensitiveContent = (content: string) => {
  return content.replace(
    SENSITIVE_KEY_PATTERN,
    (_match, prefix: string, _key: string, rawValue: string) => {
      const unquoted = rawValue.replace(/^['"]|['"]$/g, "");
      const masked = maskValue(unquoted);
      const startQuote =
        rawValue.startsWith('"') || rawValue.startsWith("'") ? rawValue[0] : "";
      const endQuote =
        rawValue.endsWith('"') || rawValue.endsWith("'")
          ? rawValue[rawValue.length - 1]
          : "";

      return `${prefix}${startQuote}${masked}${endQuote}`;
    },
  );
};

export function ConfigImportDialog({
  open,
  request,
  isSubmitting,
  onClose,
  onConfirm,
}: ConfigImportDialogProps) {
  const { t } = useTranslation();
  const [content, setContent] = useState("");
  const [decodeError, setDecodeError] = useState<string | null>(null);
  const [showRaw, setShowRaw] = useState(false);

  useEffect(() => {
    if (!request) {
      setContent("");
      setDecodeError(null);
      setShowRaw(false);
      return;
    }

    try {
      const decoded = decodeBase64Content(request.data);
      setContent(decoded);
      setDecodeError(null);
      setShowRaw(false);
    } catch (error) {
      const message =
        error instanceof Error
          ? error.message
          : t("deeplink.config.decodeErrorFallback");
      setDecodeError(message);
      setContent("");
    }
  }, [request, t]);

  const maskedPreview = useMemo(() => maskSensitiveContent(content), [content]);
  const formatLabel = request?.format
    ? request.format.toUpperCase()
    : t("deeplink.config.formatAuto");
  const displayContent = showRaw ? content : maskedPreview;
  const canImport = Boolean(request && !decodeError && content.trim().length);

  const handleConfirm = () => {
    if (!request || !canImport) return;

    onConfirm({
      ...request,
      resource: "config",
      data: encodeBase64Content(content),
    });
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => (!next ? onClose() : undefined)}
    >
      <DialogContent className="max-w-4xl sm:max-w-5xl">
        <DialogHeader>
          <DialogTitle>{t("deeplink.config.title")}</DialogTitle>
          <DialogDescription>
            {t("deeplink.config.description")}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <Alert variant="warning">
            <AlertTriangle className="h-5 w-5" aria-hidden="true" />
            <div>
              <AlertTitle>{t("deeplink.config.securityTitle")}</AlertTitle>
              <AlertDescription>
                {t("deeplink.config.securityBody")}
              </AlertDescription>
            </div>
          </Alert>

          <div className="grid gap-3 rounded-lg border border-border-default bg-background p-4 text-sm sm:grid-cols-2">
            <div className="flex flex-col gap-1">
              <span className="text-muted-foreground">
                {t("deeplink.config.app")}
              </span>
              <span className="font-medium capitalize">{request?.app}</span>
            </div>
            <div className="flex flex-col gap-1">
              <span className="text-muted-foreground">
                {t("deeplink.config.format")}
              </span>
              <span className="font-medium">{formatLabel}</span>
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between gap-3">
              <div className="text-sm text-muted-foreground">
                {t("deeplink.config.maskHint")}
              </div>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => setShowRaw((prev) => !prev)}
                aria-pressed={showRaw}
              >
                {showRaw ? (
                  <EyeOff className="h-4 w-4" aria-hidden="true" />
                ) : (
                  <Eye className="h-4 w-4" aria-hidden="true" />
                )}
                {showRaw
                  ? t("deeplink.config.hideRaw")
                  : t("deeplink.config.showRaw")}
              </Button>
            </div>

            <Textarea
              value={displayContent}
              onChange={(event) => {
                if (!showRaw || decodeError) return;
                setContent(event.target.value);
              }}
              readOnly={!showRaw || Boolean(decodeError)}
              className="min-h-[280px] font-mono"
              spellCheck={false}
              aria-label={t("deeplink.config.ariaEditor")}
              placeholder={t("deeplink.config.placeholder")}
            />

            {decodeError ? (
              <p className="text-sm text-destructive" role="status">
                {t("deeplink.config.decodeError", { message: decodeError })}
              </p>
            ) : (
              <p className="text-xs text-muted-foreground">
                {t("deeplink.config.editTip")}
              </p>
            )}
          </div>
        </div>

        <DialogFooter className="gap-2 sm:justify-end">
          <Button variant="outline" onClick={onClose} disabled={isSubmitting}>
            {t("common.cancel")}
          </Button>
          <Button
            onClick={handleConfirm}
            disabled={!canImport || isSubmitting}
            aria-disabled={!canImport || isSubmitting}
          >
            {isSubmitting
              ? t("deeplink.config.importing")
              : t("deeplink.config.import")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
