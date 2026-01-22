import { useTranslation } from "react-i18next";
import { FormLabel } from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Info } from "lucide-react";
import EndpointSpeedTest from "./EndpointSpeedTest";
import { ApiKeySection, EndpointField } from "./shared";
import type { ProviderCategory } from "@/types";

interface EndpointCandidate {
  url: string;
}

interface GeminiFormFieldsProps {
  providerId?: string;
  // API Key
  shouldShowApiKey: boolean;
  apiKey: string;
  onApiKeyChange: (key: string) => void;
  category?: ProviderCategory;
  shouldShowApiKeyLink: boolean;
  websiteUrl: string;
  isPartner?: boolean;
  partnerPromotionKey?: string;

  // Base URL
  shouldShowSpeedTest: boolean;
  baseUrl: string;
  onBaseUrlChange: (url: string) => void;
  isEndpointModalOpen: boolean;
  onEndpointModalToggle: (open: boolean) => void;
  onCustomEndpointsChange: (endpoints: string[]) => void;
  autoSelect: boolean;
  onAutoSelectChange: (checked: boolean) => void;

  // Model
  shouldShowModelField: boolean;
  model: string;
  onModelChange: (value: string) => void;

  // Speed Test Endpoints
  speedTestEndpoints: EndpointCandidate[];

  // Protocol Conversion Mode
  showProtocolConversionMode?: boolean;
  protocolConversionEnabled?: boolean;
  onProtocolConversionChange?: (enabled: boolean) => void;

  // Converter Selection
  showConverterSelector?: boolean;
  converterType?: string;
  onConverterTypeChange?: (type: string) => void;

  // Multi-Protocol Conversion
  sourceFormat?: string;
  targetFormat?: string;
  onSourceFormatChange?: (format: string) => void;
  onTargetFormatChange?: (format: string) => void;
}

export function GeminiFormFields({
  providerId,
  shouldShowApiKey,
  apiKey,
  onApiKeyChange,
  category,
  shouldShowApiKeyLink,
  websiteUrl,
  isPartner,
  partnerPromotionKey,
  shouldShowSpeedTest,
  baseUrl,
  onBaseUrlChange,
  isEndpointModalOpen,
  onEndpointModalToggle,
  onCustomEndpointsChange,
  autoSelect,
  onAutoSelectChange,
  shouldShowModelField,
  model,
  onModelChange,
  speedTestEndpoints,
  showProtocolConversionMode,
  protocolConversionEnabled,
  onProtocolConversionChange,
  showConverterSelector,
  converterType,
  onConverterTypeChange,
  sourceFormat,
  targetFormat,
  onSourceFormatChange,
  onTargetFormatChange,
}: GeminiFormFieldsProps) {
  const { t } = useTranslation();

  // 检测是否为 Google 官方（使用 OAuth）
  const isGoogleOfficial =
    partnerPromotionKey?.toLowerCase() === "google-official";

  // 支持的协议格式列表
  const protocolFormats = [
    { value: "anthropic", label: "Anthropic (Claude)" },
    { value: "openai", label: "OpenAI Chat Completions" },
    { value: "openai_responses", label: "OpenAI Responses API" },
    { value: "gemini", label: "Google Gemini" },
    { value: "cohere", label: "Cohere" },
    { value: "deepseek", label: "DeepSeek" },
    { value: "mistral", label: "Mistral" },
    { value: "groq", label: "Groq" },
    { value: "xai", label: "xAI (Grok)" },
    { value: "ollama", label: "Ollama" },
    { value: "openrouter", label: "OpenRouter" },
  ];

  return (
    <>
      {/* Google OAuth 提示 */}
      {isGoogleOfficial && (
        <div className="rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-950">
          <div className="flex gap-3">
            <Info className="h-5 w-5 flex-shrink-0 text-blue-600 dark:text-blue-400" />
            <div className="space-y-1">
              <p className="text-sm font-medium text-blue-900 dark:text-blue-100">
                {t("provider.form.gemini.oauthTitle", {
                  defaultValue: "OAuth 认证模式",
                })}
              </p>
              <p className="text-sm text-blue-700 dark:text-blue-300">
                {t("provider.form.gemini.oauthHint", {
                  defaultValue:
                    "Google 官方使用 OAuth 个人认证，无需填写 API Key。首次使用时会自动打开浏览器进行登录。",
                })}
              </p>
            </div>
          </div>
        </div>
      )}

      {/* API Key 输入框 */}
      {shouldShowApiKey && !isGoogleOfficial && (
        <ApiKeySection
          value={apiKey}
          onChange={onApiKeyChange}
          category={category}
          shouldShowLink={shouldShowApiKeyLink}
          websiteUrl={websiteUrl}
          isPartner={isPartner}
          partnerPromotionKey={partnerPromotionKey}
        />
      )}

      {/* Base URL 输入框（统一使用与 Codex 相同的样式与交互） */}
      {shouldShowSpeedTest && (
        <EndpointField
          id="baseUrl"
          label={t("providerForm.apiEndpoint", { defaultValue: "API 端点" })}
          value={baseUrl}
          onChange={onBaseUrlChange}
          placeholder={t("providerForm.apiEndpointPlaceholder", {
            defaultValue: "https://your-api-endpoint.com/",
          })}
          onManageClick={() => onEndpointModalToggle(true)}
        />
      )}

      {/* Model 输入框 */}
      {shouldShowModelField && (
        <div>
          <FormLabel htmlFor="gemini-model">
            {t("provider.form.gemini.model", { defaultValue: "模型" })}
          </FormLabel>
          <Input
            id="gemini-model"
            value={model ?? ""}
            onChange={(e) => onModelChange(e.target.value)}
            placeholder="gemini-3-pro-preview"
          />
        </div>
      )}

      {/* 端点测速弹窗 */}
      {shouldShowSpeedTest && isEndpointModalOpen && (
        <EndpointSpeedTest
          appId="gemini"
          providerId={providerId}
          value={baseUrl}
          onChange={onBaseUrlChange}
          initialEndpoints={speedTestEndpoints}
          visible={isEndpointModalOpen}
          onClose={() => onEndpointModalToggle(false)}
          autoSelect={autoSelect}
          onAutoSelectChange={onAutoSelectChange}
          onCustomEndpointsChange={onCustomEndpointsChange}
        />
      )}

      {/* 协议转换模式 */}
      {showProtocolConversionMode && (
        <div className="flex items-center justify-between rounded-lg border border-white/10 bg-background/60 p-4">
          <div className="space-y-1">
            <FormLabel>
              {t("providerForm.protocolConversionMode", {
                defaultValue: "协议转换模式",
              })}
            </FormLabel>
            <p className="text-xs text-muted-foreground">
              {t("providerForm.protocolConversionModeHint", {
                defaultValue:
                  "启用后可将其他协议格式的请求转换为 Gemini 格式，或将 Gemini 格式转换为其他格式。",
              })}
            </p>
          </div>
          <Switch
            checked={protocolConversionEnabled}
            onCheckedChange={onProtocolConversionChange}
          />
        </div>
      )}

      {/* 转换器选择（仅在启用协议转换时显示） */}
      {showConverterSelector && protocolConversionEnabled && (
        <div className="space-y-2 rounded-lg border border-white/10 bg-background/60 p-4">
          <FormLabel>
            {t("providerForm.converterType", {
              defaultValue: "转换器",
            })}
          </FormLabel>
          <Select
            value={converterType || "legacy"}
            onValueChange={onConverterTypeChange}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="legacy">
                {t("providerForm.converterLegacy", {
                  defaultValue: "Legacy (稳定版)",
                })}
              </SelectItem>
              <SelectItem value="rig">
                {t("providerForm.converterRig", {
                  defaultValue: "Rig (统一格式)",
                })}
              </SelectItem>
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground">
            {converterType === "rig"
              ? t("providerForm.converterRigHint", {
                  defaultValue:
                    "使用基于 Rig 设计的统一消息格式转换器，支持更丰富的消息类型。",
                })
              : t("providerForm.converterLegacyHint", {
                  defaultValue: "使用经过验证的稳定版转换器。",
                })}
          </p>
        </div>
      )}

      {/* 多协议转换配置（仅在启用协议转换且使用 Rig 转换器时显示） */}
      {showConverterSelector &&
        protocolConversionEnabled &&
        converterType === "rig" && (
          <div className="space-y-4 rounded-lg border border-white/10 bg-background/60 p-4">
            <FormLabel>
              {t("providerForm.protocolConversion", {
                defaultValue: "协议转换配置",
              })}
            </FormLabel>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {/* 源协议格式 */}
              <div className="space-y-2">
                <FormLabel htmlFor="geminiSourceFormat">
                  {t("providerForm.sourceFormat", {
                    defaultValue: "源格式（客户端）",
                  })}
                </FormLabel>
                <Select
                  value={sourceFormat || "gemini"}
                  onValueChange={onSourceFormatChange}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {protocolFormats.map((format) => (
                      <SelectItem key={format.value} value={format.value}>
                        {format.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* 目标协议格式 */}
              <div className="space-y-2">
                <FormLabel htmlFor="geminiTargetFormat">
                  {t("providerForm.targetFormat", {
                    defaultValue: "目标格式（上游 API）",
                  })}
                </FormLabel>
                <Select
                  value={targetFormat || "gemini"}
                  onValueChange={onTargetFormatChange}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {protocolFormats.map((format) => (
                      <SelectItem key={format.value} value={format.value}>
                        {format.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>
            <p className="text-xs text-muted-foreground">
              {t("providerForm.protocolConversionHint", {
                defaultValue:
                  "选择客户端发送的请求格式和上游 API 期望的格式，系统将自动进行转换。",
              })}
            </p>
            {sourceFormat === targetFormat && (
              <p className="text-xs text-amber-500">
                {t("providerForm.sameFormatWarning", {
                  defaultValue:
                    "源格式和目标格式相同，将不进行转换（透传模式）。",
                })}
              </p>
            )}
          </div>
        )}
    </>
  );
}
