import React, { useState } from "react";
import { Zap, Loader2, Check, AlertCircle } from "lucide-react";
import {
  testPackyCodeEndpoints,
  type PackyCodeService,
  type PackyCodeEndpoint,
} from "../lib/packycode-api";
import { Provider } from "../types";

interface PackyCodeSpeedTestProps {
  provider: Provider;
  onUpdateProvider: (provider: Provider) => Promise<void>;
}

export const PackyCodeSpeedTest: React.FC<PackyCodeSpeedTestProps> = ({
  provider,
  onUpdateProvider,
}) => {
  const [isTestingSpeed, setIsTestingSpeed] = useState(false);
  const [testResults, setTestResults] = useState<PackyCodeService[]>([]);
  const [autoSelectBest, setAutoSelectBest] = useState(true);

  // 判断是否为 PackyCode 供应商
  const isPackyCodeProvider = provider.name.toLowerCase().includes("packycode");

  // 获取对应的服务类型
  const getServiceType = (): string => {
    const name = provider.name.toLowerCase();
    if (name.includes("滴滴")) return "滴滴车";
    if (name.includes("公交")) return "公交车";
    if (name.includes("codex")) return "Codex";
    return "";
  };

  // 执行测速
  const performSpeedTest = async () => {
    setIsTestingSpeed(true);
    setTestResults([]);

    try {
      const results = await testPackyCodeEndpoints();
      setTestResults(results);

      // 如果开启自动选择，选择延迟最低的端点
      if (autoSelectBest) {
        const serviceType = getServiceType();
        const service = results.find((s) => s.name === serviceType);

        if (service) {
          // 找到延迟最低的端点
          let bestEndpoint: PackyCodeEndpoint | null = null;
          let minLatency = Infinity;

          for (const endpoint of service.endpoints) {
            if (
              endpoint.latency !== null &&
              endpoint.latency !== undefined &&
              endpoint.latency < minLatency
            ) {
              minLatency = endpoint.latency;
              bestEndpoint = endpoint;
            }
          }

          if (bestEndpoint) {
            await applyEndpoint(bestEndpoint.url);
          }
        }
      }
    } catch (error) {
      console.error("测速失败:", error);
    } finally {
      setIsTestingSpeed(false);
    }
  };

  // 应用选定的端点
  const applyEndpoint = async (url: string) => {
    const updatedProvider = {
      ...provider,
      settingsConfig: {
        ...provider.settingsConfig,
        env: {
          ...(provider.settingsConfig as any).env,
          ANTHROPIC_BASE_URL: url,
        },
      },
    };

    await onUpdateProvider(updatedProvider);
  };

  // 获取当前配置的 URL
  const getCurrentUrl = (): string => {
    return (provider.settingsConfig as any)?.env?.ANTHROPIC_BASE_URL || "";
  };

  // 如果不是 PackyCode 供应商，不显示测速按钮
  if (!isPackyCodeProvider) {
    return null;
  }

  return (
    <div className="mt-4 p-4 bg-gray-50 dark:bg-gray-800 rounded-lg">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">
          PackyCode 节点测速
        </h3>
        <button
          onClick={performSpeedTest}
          disabled={isTestingSpeed}
          className="flex items-center gap-2 px-3 py-1.5 bg-blue-500 text-white rounded-md hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed text-sm"
        >
          {isTestingSpeed ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              测速中...
            </>
          ) : (
            <>
              <Zap className="w-4 h-4" />
              开始测速
            </>
          )}
        </button>
      </div>

      <div className="flex items-center gap-2 mb-3">
        <input
          type="checkbox"
          id="auto-select"
          checked={autoSelectBest}
          onChange={(e) => setAutoSelectBest(e.target.checked)}
          className="rounded border-gray-300 dark:border-gray-600"
        />
        <label
          htmlFor="auto-select"
          className="text-sm text-gray-600 dark:text-gray-400"
        >
          自动选择延迟最低的节点
        </label>
      </div>

      {/* 测速结果 */}
      {testResults.length > 0 && (
        <div className="space-y-3">
          {testResults
            .filter((service) => service.name === getServiceType())
            .map((service) => (
              <div key={service.name} className="space-y-2">
                <h4 className="text-sm font-medium text-gray-600 dark:text-gray-400">
                  {service.name}
                </h4>
                <div className="space-y-1">
                  {service.endpoints.map((endpoint) => {
                    const isSelected = getCurrentUrl() === endpoint.url;
                    const hasLatency =
                      endpoint.latency !== null &&
                      endpoint.latency !== undefined;

                    return (
                      <div
                        key={endpoint.url}
                        className={`flex items-center justify-between p-2 rounded-md text-sm ${
                          isSelected
                            ? "bg-green-100 dark:bg-green-900/30 border border-green-300 dark:border-green-700"
                            : "bg-white dark:bg-gray-700 border border-gray-200 dark:border-gray-600"
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          {isSelected && (
                            <Check className="w-4 h-4 text-green-600 dark:text-green-400" />
                          )}
                          <span className="font-medium">{endpoint.name}</span>
                        </div>
                        <div className="flex items-center gap-3">
                          {hasLatency ? (
                            <span
                              className={`font-mono ${
                                endpoint.latency! < 100
                                  ? "text-green-600 dark:text-green-400"
                                  : endpoint.latency! < 300
                                    ? "text-yellow-600 dark:text-yellow-400"
                                    : "text-red-600 dark:text-red-400"
                              }`}
                            >
                              {endpoint.latency}ms
                            </span>
                          ) : (
                            <span className="text-gray-400 flex items-center gap-1">
                              <AlertCircle className="w-3 h-3" />
                              超时
                            </span>
                          )}
                          {!autoSelectBest && hasLatency && (
                            <button
                              onClick={() => applyEndpoint(endpoint.url)}
                              disabled={isSelected}
                              className="px-2 py-0.5 text-xs bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                              {isSelected ? "已选择" : "选择"}
                            </button>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
        </div>
      )}
    </div>
  );
};
