import { invoke } from "@tauri-apps/api/core";
import type { AppId } from "./types";

export interface DeepLinkImportRequest {
  version: string;
  resource: string;
  app: AppId;
  name: string;
  homepage: string;
  endpoint: string;
  apiKey: string;
  model?: string;
  notes?: string;
}

export interface ConfigImportRequest {
  resource?: "config";
  app: AppId;
  data: string;
  format?: "json" | "toml";
}

export type DeepLinkEventRequest =
  | DeepLinkImportRequest
  | (ConfigImportRequest & { resource: "config" });

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

  /**
   * Import a full config payload (resource=config)
   */
  importConfig: async (request: ConfigImportRequest): Promise<string> => {
    return invoke("import_config_from_deeplink", { request });
  },
};
