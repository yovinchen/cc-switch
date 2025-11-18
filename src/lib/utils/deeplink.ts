/**
 * Deep Link 工具函数
 * 用于配置导入的 Base64 编解码和 URL 生成
 */

const MAX_CONFIG_SIZE = 100 * 1024; // 100KB

/**
 * 将配置内容编码为 Base64
 * @param content 配置内容（JSON 或 TOML 字符串）
 * @returns Base64 编码后的字符串
 * @throws Error 如果编码失败或内容超过大小限制
 */
export function encodeConfig(content: string): string {
  // 检查配置大小
  if (content.length > MAX_CONFIG_SIZE) {
    throw new Error(`配置文件过大（最大 ${MAX_CONFIG_SIZE / 1024}KB）`);
  }

  try {
    // 使用浏览器原生 btoa 进行 Base64 编码
    // 先转换为 UTF-8 字节再编码，支持中文等非 ASCII 字符
    return btoa(
      encodeURIComponent(content).replace(/%([0-9A-F]{2})/g, (_, p1) => {
        return String.fromCharCode(parseInt(p1, 16));
      }),
    );
  } catch (error) {
    throw new Error(
      `Base64 编码失败: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

/**
 * 将 Base64 编码的配置内容解码
 * @param encoded Base64 编码的字符串
 * @returns 解码后的配置内容
 * @throws Error 如果解码失败
 */
export function decodeConfig(encoded: string): string {
  try {
    // 使用浏览器原生 atob 进行 Base64 解码
    const decoded = atob(encoded);
    // 将字节转换回 UTF-8 字符串
    return decodeURIComponent(
      decoded
        .split("")
        .map((c) => "%" + ("00" + c.charCodeAt(0).toString(16)).slice(-2))
        .join(""),
    );
  } catch (error) {
    throw new Error(
      `Base64 解码失败: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

/**
 * 生成配置导入深链接 URL
 * @param app 目标应用类型
 * @param configContent 配置内容（JSON 或 TOML 字符串）
 * @param format 配置格式（可选，自动检测）
 * @returns 完整的 ccswitch:// 深链接 URL
 * @throws Error 如果编码失败或内容超过大小限制
 */
export function generateConfigImportUrl(
  app: "claude" | "codex" | "gemini",
  configContent: string,
  format?: "json" | "toml",
): string {
  const encodedData = encodeConfig(configContent);

  const params = new URLSearchParams({
    resource: "config",
    app,
    data: encodedData,
  });

  if (format) {
    params.set("format", format);
  }

  return `ccswitch://v1/import?${params.toString()}`;
}

/**
 * 验证配置内容格式
 * @param content 配置内容
 * @param format 期望的格式
 * @returns 是否有效
 */
export function validateConfigFormat(
  content: string,
  format: "json" | "toml",
): boolean {
  try {
    if (format === "json") {
      JSON.parse(content);
      return true;
    } else if (format === "toml") {
      // TOML 验证需要后端支持，这里只做基本检查
      return content.trim().length > 0;
    }
    return false;
  } catch {
    return false;
  }
}
