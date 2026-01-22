import { useTranslation } from "react-i18next";
import { FormLabel } from "@/components/ui/form";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import EndpointSpeedTest from "./EndpointSpeedTest";
import { ApiKeySection, EndpointField } from "./shared";
import type { ProviderCategory } from "@/types";
import type { TemplateValueConfig } from "@/config/claudeProviderPresets";

interface EndpointCandidate {
  url: string;
}

interface ClaudeFormFieldsProps {
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

  // Template Values
  templateValueEntries: Array<[string, TemplateValueConfig]>;
  templateValues: Record<string, TemplateValueConfig>;
  templatePresetName: string;
  onTemplateValueChange: (key: string, value: string) => void;

  // Base URL
  shouldShowSpeedTest: boolean;
  baseUrl: string;
  onBaseUrlChange: (url: string) => void;
  isEndpointModalOpen: boolean;
  onEndpointModalToggle: (open: boolean) => void;
  onCustomEndpointsChange?: (endpoints: string[]) => void;
  autoSelect: boolean;
  onAutoSelectChange: (checked: boolean) => void;

  // Model Selector
  shouldShowModelSelector: boolean;
  claudeModel: string;
  reasoningModel: string;
  defaultHaikuModel: string;
  defaultSonnetModel: string;
  defaultOpusModel: string;
  onModelChange: (
    field:
      | "ANTHROPIC_MODEL"
      | "ANTHROPIC_REASONING_MODEL"
      | "ANTHROPIC_DEFAULT_HAIKU_MODEL"
      | "ANTHROPIC_DEFAULT_SONNET_MODEL"
      | "ANTHROPIC_DEFAULT_OPUS_MODEL",
    value: string,
  ) => void;

  // Speed Test Endpoints
  speedTestEndpoints: EndpointCandidate[];

  // OpenRouter Compat
  showOpenRouterCompatToggle: boolean;
  openRouterCompatEnabled: boolean;
  onOpenRouterCompatChange: (enabled: boolean) => void;

  // Chat Completions Mode (Protocol Conversion)
  showChatCompletionsMode?: boolean;
  chatCompletionsModeEnabled?: boolean;
  onChatCompletionsModeChange?: (enabled: boolean) => void;

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

export function ClaudeFormFields({
  providerId,
  shouldShowApiKey,
  apiKey,
  onApiKeyChange,
  category,
  shouldShowApiKeyLink,
  websiteUrl,
  isPartner,
  partnerPromotionKey,
  templateValueEntries,
  templateValues,
  templatePresetName,
  onTemplateValueChange,
  shouldShowSpeedTest,
  baseUrl,
  onBaseUrlChange,
  isEndpointModalOpen,
  onEndpointModalToggle,
  onCustomEndpointsChange,
  autoSelect,
  onAutoSelectChange,
  shouldShowModelSelector,
  claudeModel,
  reasoningModel,
  defaultHaikuModel,
  defaultSonnetModel,
  defaultOpusModel,
  onModelChange,
  speedTestEndpoints,
  showOpenRouterCompatToggle,
  openRouterCompatEnabled,
  onOpenRouterCompatChange,
  showChatCompletionsMode,
  chatCompletionsModeEnabled,
  onChatCompletionsModeChange,
  showConverterSelector,
  converterType,
  onConverterTypeChange,
  sourceFormat,
  targetFormat,
  onSourceFormatChange,
  onTargetFormatChange,
}: ClaudeFormFieldsProps) {
  const { t } = useTranslation();

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
      {/* API Key 输入框 */}
      {shouldShowApiKey && (
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

      {/* 模板变量输入 */}
      {templateValueEntries.length > 0 && (
        <div className="space-y-3">
          <FormLabel>
            {t("providerForm.parameterConfig", {
              name: templatePresetName,
              defaultValue: `${templatePresetName} 参数配置`,
            })}
          </FormLabel>
          <div className="space-y-4">
            {templateValueEntries.map(([key, config]) => (
              <div key={key} className="space-y-2">
                <FormLabel htmlFor={`template-${key}`}>
                  {config.label}
                </FormLabel>
                <Input
                  id={`template-${key}`}
                  type="text"
                  required
                  value={
                    templateValues[key]?.editorValue ??
                    config.editorValue ??
                    config.defaultValue ??
                    ""
                  }
                  onChange={(e) => onTemplateValueChange(key, e.target.value)}
                  placeholder={config.placeholder || config.label}
                  autoComplete="off"
                />
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Base URL 输入框 */}
      {shouldShowSpeedTest && (
        <EndpointField
          id="baseUrl"
          label={t("providerForm.apiEndpoint")}
          value={baseUrl}
          onChange={onBaseUrlChange}
          placeholder={t("providerForm.apiEndpointPlaceholder")}
          hint={t("providerForm.apiHint")}
          onManageClick={() => onEndpointModalToggle(true)}
        />
      )}

      {/* 端点测速弹窗 */}
      {shouldShowSpeedTest && isEndpointModalOpen && (
        <EndpointSpeedTest
          appId="claude"
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

      {showOpenRouterCompatToggle && (
        <div className="flex items-center justify-between rounded-lg border border-white/10 bg-background/60 p-4">
          <div className="space-y-1">
            <FormLabel>
              {t("providerForm.openrouterCompatMode", {
                defaultValue: "OpenRouter 兼容模式",
              })}
            </FormLabel>
            <p className="text-xs text-muted-foreground">
              {t("providerForm.openrouterCompatModeHint", {
                defaultValue:
                  "使用 OpenAI Chat Completions 接口并转换为 Anthropic SSE。",
              })}
            </p>
          </div>
          <Switch
            checked={openRouterCompatEnabled}
            onCheckedChange={onOpenRouterCompatChange}
          />
        </div>
      )}

      {/* 模型选择器 - 放在协议转换配置之前，确保始终可见 */}
      {shouldShowModelSelector && (
        <div className="space-y-3">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* 主模型 */}
            <div className="space-y-2">
              <FormLabel htmlFor="claudeModel">
                {t("providerForm.anthropicModel", { defaultValue: "主模型" })}
              </FormLabel>
              <Input
                id="claudeModel"
                type="text"
                value={claudeModel ?? ""}
                onChange={(e) =>
                  onModelChange("ANTHROPIC_MODEL", e.target.value)
                }
                placeholder={t("providerForm.modelPlaceholder", {
                  defaultValue: "",
                })}
                autoComplete="off"
              />
            </div>

            {/* 推理模型 */}
            <div className="space-y-2">
              <FormLabel htmlFor="reasoningModel">
                {t("providerForm.anthropicReasoningModel")}
              </FormLabel>
              <Input
                id="reasoningModel"
                type="text"
                value={reasoningModel ?? ""}
                onChange={(e) =>
                  onModelChange("ANTHROPIC_REASONING_MODEL", e.target.value)
                }
                autoComplete="off"
              />
            </div>

            {/* 默认 Haiku */}
            <div className="space-y-2">
              <FormLabel htmlFor="claudeDefaultHaikuModel">
                {t("providerForm.anthropicDefaultHaikuModel", {
                  defaultValue: "Haiku 默认模型",
                })}
              </FormLabel>
              <Input
                id="claudeDefaultHaikuModel"
                type="text"
                value={defaultHaikuModel ?? ""}
                onChange={(e) =>
                  onModelChange("ANTHROPIC_DEFAULT_HAIKU_MODEL", e.target.value)
                }
                placeholder={t("providerForm.haikuModelPlaceholder", {
                  defaultValue: "",
                })}
                autoComplete="off"
              />
            </div>

            {/* 默认 Sonnet */}
            <div className="space-y-2">
              <FormLabel htmlFor="claudeDefaultSonnetModel">
                {t("providerForm.anthropicDefaultSonnetModel", {
                  defaultValue: "Sonnet 默认模型",
                })}
              </FormLabel>
              <Input
                id="claudeDefaultSonnetModel"
                type="text"
                value={defaultSonnetModel ?? ""}
                onChange={(e) =>
                  onModelChange(
                    "ANTHROPIC_DEFAULT_SONNET_MODEL",
                    e.target.value,
                  )
                }
                placeholder={t("providerForm.modelPlaceholder", {
                  defaultValue: "",
                })}
                autoComplete="off"
              />
            </div>

            {/* 默认 Opus */}
            <div className="space-y-2">
              <FormLabel htmlFor="claudeDefaultOpusModel">
                {t("providerForm.anthropicDefaultOpusModel", {
                  defaultValue: "Opus 默认模型",
                })}
              </FormLabel>
              <Input
                id="claudeDefaultOpusModel"
                type="text"
                value={defaultOpusModel ?? ""}
                onChange={(e) =>
                  onModelChange("ANTHROPIC_DEFAULT_OPUS_MODEL", e.target.value)
                }
                placeholder={t("providerForm.modelPlaceholder", {
                  defaultValue: "",
                })}
                autoComplete="off"
              />
            </div>
          </div>
          <p className="text-xs text-muted-foreground">
            {t("providerForm.modelHelper", {
              defaultValue:
                "可选：指定默认使用的 Claude 模型，留空则使用系统默认。",
            })}
          </p>
        </div>
      )}

      {/* Chat Completions 协议转换模式 */}
      {showChatCompletionsMode && (
        <div className="flex items-center justify-between rounded-lg border border-white/10 bg-background/60 p-4">
          <div className="space-y-1">
            <FormLabel>
              {t("providerForm.chatCompletionsMode", {
                defaultValue: "协议转换模式",
              })}
            </FormLabel>
            <p className="text-xs text-muted-foreground">
              {t("providerForm.chatCompletionsModeHint", {
                defaultValue:
                  "将 Anthropic 格式请求转换为 OpenAI Chat Completions 格式，适用于 OpenAI 兼容的中转服务。",
              })}
            </p>
          </div>
          <Switch
            checked={chatCompletionsModeEnabled}
            onCheckedChange={onChatCompletionsModeChange}
          />
        </div>
      )}

      {/* 转换器选择（仅在启用协议转换时显示） */}
      {showConverterSelector && chatCompletionsModeEnabled && (
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
        chatCompletionsModeEnabled &&
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
                <FormLabel htmlFor="sourceFormat">
                  {t("providerForm.sourceFormat", {
                    defaultValue: "源格式（客户端）",
                  })}
                </FormLabel>
                <Select
                  value={sourceFormat || "anthropic"}
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
                <FormLabel htmlFor="targetFormat">
                  {t("providerForm.targetFormat", {
                    defaultValue: "目标格式（上游 API）",
                  })}
                </FormLabel>
                <Select
                  value={targetFormat || "openai"}
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
