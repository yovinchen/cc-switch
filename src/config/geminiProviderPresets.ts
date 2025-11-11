import type { ProviderCategory } from "@/types";

export interface GeminiProviderPreset {
  name: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: object;
  baseURL?: string;
  model?: string;
  description?: string;
  category?: ProviderCategory;
  isPartner?: boolean;
  partnerPromotionKey?: string;
  endpointCandidates?: string[];
}

export const geminiProviderPresets: GeminiProviderPreset[] = [
  {
    name: 'PackyCode',
    websiteUrl: 'https://www.packyapi.com',
    apiKeyUrl: 'https://www.packyapi.com/register?aff=cc-switch',
    settingsConfig: {
      env: {
        GOOGLE_GEMINI_BASE_URL: 'https://www.packyapi.com',
        GEMINI_MODEL: 'gemini-2.5-pro',
      },
    },
    baseURL: 'https://www.packyapi.com',
    model: 'gemini-2.5-pro',
    description: 'PackyCode',
    category: 'third_party',
    isPartner: true,
    partnerPromotionKey: 'packycode',
    endpointCandidates: ['https://api-slb.packyapi.com', 'https://www.packyapi.com'],
  },
  {
    name: '自定义',
    websiteUrl: '',
    settingsConfig: {
      env: {
        GOOGLE_GEMINI_BASE_URL: '',
        GEMINI_MODEL: 'gemini-2.5-pro',
      },
    },
    baseURL: '',
    model: 'gemini-2.5-pro',
    description: '自定义 Gemini API 端点',
    category: 'custom',
  },
];

export function getGeminiPresetByName(name: string): GeminiProviderPreset | undefined {
  return geminiProviderPresets.find((preset) => preset.name === name);
}

export function getGeminiPresetByUrl(url: string): GeminiProviderPreset | undefined {
  if (!url) return undefined;
  return geminiProviderPresets.find((preset) =>
    preset.baseURL && url.toLowerCase().includes(preset.baseURL.toLowerCase())
  );
}
