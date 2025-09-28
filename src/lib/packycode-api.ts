import { invoke } from "@tauri-apps/api/core";

export interface PackyCodeEndpoint {
  name: string;
  url: string;
  latency?: number | null;
}

export interface PackyCodeService {
  name: string;
  endpoints: PackyCodeEndpoint[];
}

// 测试所有 PackyCode 端点
export async function testPackyCodeEndpoints(): Promise<PackyCodeService[]> {
  try {
    const result = await invoke<PackyCodeService[]>("test_packycode_endpoints");
    return result;
  } catch (error) {
    console.error("测试 PackyCode 端点失败:", error);
    throw error;
  }
}

// 为指定服务选择最佳端点
export async function selectBestPackyCodeEndpoint(
  serviceName: string,
): Promise<PackyCodeEndpoint> {
  try {
    const result = await invoke<PackyCodeEndpoint>(
      "select_best_packycode_endpoint",
      { serviceName },
    );
    return result;
  } catch (error) {
    console.error("选择最佳端点失败:", error);
    throw error;
  }
}
