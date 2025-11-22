import { invoke } from "@tauri-apps/api/core";

export interface DeepLinkImportRequest {
  version: string;
  resource: string;
  app: "claude" | "codex" | "gemini";
  name: string;
  homepage: string;
  endpoint: string;
  apiKey: string;
  model?: string;
  notes?: string;
  // Claude 专用模型字段 (v3.7.1+)
  haikuModel?: string;
  sonnetModel?: string;
  opusModel?: string;
  // 配置文件导入字段 (v3.8+)
  config?: string; // Base64 编码的配置内容
  configFormat?: string; // json/toml
  configUrl?: string; // 远程配置 URL
}

export const deeplinkApi = {
  /**
   * Parse a deep link URL
   * @param url The ccswitch:// URL to parse
   * @returns Parsed deep link request
   */
  parseDeeplink: async (url: string): Promise<DeepLinkImportRequest> => {
    return invoke("parse_deeplink", { url });
  },

  /**
   * Import a provider from a deep link request
   * @param request The deep link import request
   * @returns The ID of the imported provider
   */
  importFromDeeplink: async (
    request: DeepLinkImportRequest,
  ): Promise<string> => {
    return invoke("import_from_deeplink", { request });
  },
};
