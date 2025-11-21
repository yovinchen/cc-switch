import { useMemo } from "react";
import {
  AlertCircle,
  CheckCircle2,
  FolderOpen,
  Loader2,
  Save,
  XCircle,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { useTranslation } from "react-i18next";
import type { ImportStatus } from "@/hooks/useImportExport";

interface ImportExportSectionProps {
  status: ImportStatus;
  selectedFile: string;
  errorMessage: string | null;
  backupId: string | null;
  isImporting: boolean;
  onSelectFile: () => Promise<void>;
  onImport: () => Promise<void>;
  onExport: () => Promise<void>;
  onClear: () => void;
}

export function ImportExportSection({
  status,
  selectedFile,
  errorMessage,
  backupId,
  isImporting,
  onSelectFile,
  onImport,
  onExport,
  onClear,
}: ImportExportSectionProps) {
  const { t } = useTranslation();

  const selectedFileName = useMemo(() => {
    if (!selectedFile) return "";
    const segments = selectedFile.split(/[\\/]/);
    return segments[segments.length - 1] || selectedFile;
  }, [selectedFile]);

  return (
    <section className="space-y-4">
      <header className="space-y-2">
        <h3 className="text-base font-semibold text-foreground">
          {t("settings.importExport")}
        </h3>
        <p className="text-sm text-muted-foreground">
          {t("settings.importExportHint")}
        </p>
      </header>

      <div className="space-y-4 rounded-xl glass-card p-6 border border-white/10">
        {/* Export Section */}
        <div className="space-y-3">
          <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
            <Save className="h-4 w-4" />
            <span>导出配置</span>
          </div>
          <Button
            type="button"
            className="w-full bg-primary/90 hover:bg-primary text-primary-foreground shadow-lg shadow-primary/20"
            onClick={onExport}
          >
            <Save className="mr-2 h-4 w-4" />
            {t("settings.exportConfig")}
          </Button>
        </div>

        {/* Divider */}
        <div className="border-t border-white/10" />

        {/* Import Section */}
        <div className="space-y-3">
          <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
            <FolderOpen className="h-4 w-4" />
            <span>导入配置</span>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="outline"
              className="flex-1 min-w-[180px] hover:bg-black/5 dark:hover:bg-white/5 border-white/10"
              onClick={onSelectFile}
            >
              <FolderOpen className="mr-2 h-4 w-4" />
              {t("settings.selectConfigFile")}
            </Button>
            <Button
              type="button"
              disabled={!selectedFile || isImporting}
              className="bg-primary hover:bg-primary/90"
              onClick={onImport}
            >
              {isImporting ? (
                <span className="inline-flex items-center gap-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t("settings.importing")}
                </span>
              ) : (
                <>
                  <CheckCircle2 className="mr-2 h-4 w-4" />
                  {t("settings.import")}
                </>
              )}
            </Button>
            {selectedFile ? (
              <Button
                type="button"
                variant="ghost"
                onClick={onClear}
                className="hover:bg-black/5 dark:hover:bg-white/5"
              >
                <XCircle className="mr-2 h-4 w-4" />
                {t("common.clear")}
              </Button>
            ) : null}
          </div>

          {selectedFile ? (
            <div className="glass rounded-lg border border-white/10 p-3">
              <p className="text-xs font-mono text-foreground/80 truncate">
                📄 {selectedFileName}
              </p>
            </div>
          ) : (
            <div className="glass rounded-lg border border-white/10 p-3">
              <p className="text-xs text-muted-foreground italic">
                {t("settings.noFileSelected")}
              </p>
            </div>
          )}
        </div>

        <ImportStatusMessage
          status={status}
          errorMessage={errorMessage}
          backupId={backupId}
        />
      </div>
    </section>
  );
}

interface ImportStatusMessageProps {
  status: ImportStatus;
  errorMessage: string | null;
  backupId: string | null;
}

function ImportStatusMessage({
  status,
  errorMessage,
  backupId,
}: ImportStatusMessageProps) {
  const { t } = useTranslation();

  if (status === "idle") {
    return null;
  }

  const baseClass =
    "flex items-start gap-3 rounded-xl border p-4 text-sm leading-relaxed backdrop-blur-sm";

  if (status === "importing") {
    return (
      <div
        className={`${baseClass} border-blue-500/30 bg-blue-500/10 text-blue-600 dark:text-blue-400`}
      >
        <Loader2 className="mt-0.5 h-5 w-5 flex-shrink-0 animate-spin" />
        <div>
          <p className="font-semibold">{t("settings.importing")}</p>
          <p className="text-blue-600/80 dark:text-blue-400/80">
            {t("common.loading")}
          </p>
        </div>
      </div>
    );
  }

  if (status === "success") {
    return (
      <div
        className={`${baseClass} border-green-500/30 bg-green-500/10 text-green-700 dark:text-green-400`}
      >
        <CheckCircle2 className="mt-0.5 h-5 w-5 flex-shrink-0" />
        <div className="space-y-1.5">
          <p className="font-semibold">{t("settings.importSuccess")}</p>
          {backupId ? (
            <p className="text-xs text-green-600/80 dark:text-green-400/80">
              {t("settings.backupId")}: {backupId}
            </p>
          ) : null}
          <p className="text-green-600/80 dark:text-green-400/80">
            {t("settings.autoReload")}
          </p>
        </div>
      </div>
    );
  }

  if (status === "partial-success") {
    return (
      <div
        className={`${baseClass} border-yellow-500/30 bg-yellow-500/10 text-yellow-700 dark:text-yellow-400`}
      >
        <AlertCircle className="mt-0.5 h-5 w-5 flex-shrink-0" />
        <div className="space-y-1.5">
          <p className="font-semibold">{t("settings.importPartialSuccess")}</p>
          <p className="text-yellow-600/80 dark:text-yellow-400/80">
            {t("settings.importPartialHint")}
          </p>
        </div>
      </div>
    );
  }

  const message = errorMessage || t("settings.importFailed");

  return (
    <div
      className={`${baseClass} border-red-500/30 bg-red-500/10 text-red-600 dark:text-red-400`}
    >
      <AlertCircle className="mt-0.5 h-5 w-5 flex-shrink-0" />
      <div className="space-y-1.5">
        <p className="font-semibold">{t("settings.importFailed")}</p>
        <p className="text-red-600/80 dark:text-red-400/80">{message}</p>
      </div>
    </div>
  );
}
