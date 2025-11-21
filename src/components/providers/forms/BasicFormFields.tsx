import { useTranslation } from "react-i18next";
import { useState } from "react";
import {
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { ArrowLeft } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { ProviderIcon } from "@/components/ProviderIcon";
import { IconPicker } from "@/components/IconPicker";
import { getIconMetadata } from "@/icons/extracted/metadata";
import type { UseFormReturn } from "react-hook-form";
import type { ProviderFormData } from "@/lib/schemas/provider";

interface BasicFormFieldsProps {
  form: UseFormReturn<ProviderFormData>;
}

export function BasicFormFields({ form }: BasicFormFieldsProps) {
  const { t } = useTranslation();
  const [iconDialogOpen, setIconDialogOpen] = useState(false);

  const currentIcon = form.watch("icon");
  const currentIconColor = form.watch("iconColor");
  const providerName = form.watch("name") || "Provider";
  const effectiveIconColor =
    currentIconColor ||
    (currentIcon ? getIconMetadata(currentIcon)?.defaultColor : undefined);

  const handleIconSelect = (icon: string) => {
    const meta = getIconMetadata(icon);
    form.setValue("icon", icon);
    form.setValue("iconColor", meta?.defaultColor ?? "");
  };

  return (
    <>
      <FormField
        control={form.control}
        name="name"
        render={({ field }) => (
          <FormItem>
            <FormLabel>{t("provider.name")}</FormLabel>
            <FormControl>
              <Input {...field} placeholder={t("provider.namePlaceholder")} />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />

      <FormField
        control={form.control}
        name="websiteUrl"
        render={({ field }) => (
          <FormItem>
            <FormLabel>{t("provider.websiteUrl")}</FormLabel>
            <FormControl>
              <Input {...field} placeholder="https://" />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />

      <FormField
        control={form.control}
        name="notes"
        render={({ field }) => (
          <FormItem>
            <FormLabel>{t("provider.notes")}</FormLabel>
            <FormControl>
              <Input {...field} placeholder={t("provider.notesPlaceholder")} />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />

      {/* 图标配置 */}
      <div className="space-y-3">
        <FormLabel>
          {t("providerIcon.label", { defaultValue: "图标" })}
        </FormLabel>

        {/* 图标预览 */}
        <div className="flex items-center gap-4 p-4 rounded-lg bg-muted">
          <ProviderIcon
            icon={currentIcon}
            name={providerName}
            color={effectiveIconColor}
            size={48}
          />
          <div className="flex-1">
            <p className="font-medium">{providerName}</p>
            <p className="text-sm text-muted-foreground">
              {currentIcon ||
                t("providerIcon.noIcon", { defaultValue: "未选择图标" })}
            </p>
          </div>
          <Dialog open={iconDialogOpen} onOpenChange={setIconDialogOpen}>
            <DialogTrigger asChild>
              <Button type="button" variant="outline">
                {t("providerIcon.selectIcon", { defaultValue: "选择图标" })}
              </Button>
            </DialogTrigger>
            <DialogContent
              variant="fullscreen"
              zIndex="top"
              overlayClassName="bg-[hsl(var(--background))] backdrop-blur-0"
              className="p-0 sm:rounded-none"
            >
              <div className="flex h-full flex-col">
                <div className="flex-shrink-0 px-6 py-4 flex items-center gap-4 border-b border-border-default bg-muted/40">
                  <DialogClose asChild>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <ArrowLeft className="h-5 w-5" />
                    </Button>
                  </DialogClose>
                  <div className="space-y-1">
                    <p className="text-lg font-semibold leading-tight">
                      {t("providerIcon.selectIcon", {
                        defaultValue: "选择图标",
                      })}
                    </p>
                    <p className="text-sm text-muted-foreground">
                      {t("providerIcon.selectDescription", {
                        defaultValue: "为供应商选择一个图标",
                      })}
                    </p>
                  </div>
                </div>
                <div className="flex-1 overflow-y-auto px-6 py-6">
                  <div className="space-y-6 max-w-5xl mx-auto w-full">
                    {/* 图标选择器 */}
                    <IconPicker
                      value={currentIcon}
                      onValueChange={handleIconSelect}
                      color={effectiveIconColor}
                    />
                    <div className="flex justify-end gap-2">
                      <DialogClose asChild>
                        <Button type="button" variant="outline">
                          {t("common.done", { defaultValue: "完成" })}
                        </Button>
                      </DialogClose>
                    </div>
                  </div>
                </div>
              </div>
            </DialogContent>
          </Dialog>
        </div>
      </div>
    </>
  );
}
