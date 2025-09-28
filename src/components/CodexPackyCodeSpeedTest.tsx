import React, { useState, useEffect, useCallback } from "react";
import { Zap, Loader2, Check, AlertCircle, Plus, Trash2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { AppType } from "../lib/tauri-api";

interface CodexPackyCodeSpeedTestProps {
  providerName: string;
  baseUrl: string;
  onUpdateBaseUrl: (url: string) => void;
  appType: AppType;
  onSpeedTestRequired?: () => Promise<void>;
}

interface EndpointTest {
  url: string;
  name: string;
  latency: number | null;
  isCustom?: boolean;
}

// 预设的 Codex 端点
const PRESET_CODEX_ENDPOINTS: EndpointTest[] = [
  {
    name: "默认节点",
    url: "https://codex-api.packycode.com/v1",
    latency: null,
  },
  {
    name: "香港CN2",
    url: "https://codex-api-hk-cn2.packycode.com/v1",
    latency: null,
  },
  {
    name: "香港CDN",
    url: "https://codex-api-hk-cdn.packycode.com/v1",
    latency: null,
  },
];

export const CodexPackyCodeSpeedTest: React.FC<
  CodexPackyCodeSpeedTestProps
> = ({
  providerName,
  baseUrl,
  onUpdateBaseUrl,
  appType,
  onSpeedTestRequired,
}) => {
  const [isTestingSpeed, setIsTestingSpeed] = useState(false);
  const [testResults, setTestResults] = useState<EndpointTest[]>([]);
  const [autoSelectBest, setAutoSelectBest] = useState(true);
  const [customUrl, setCustomUrl] = useState("");
  const [customEndpoints, setCustomEndpoints] = useState<EndpointTest[]>([]);
  const [hasTestedOnce, setHasTestedOnce] = useState(false);

  // 判断是否为 PackyCode 供应商
  const isPackyCodeProvider = providerName.toLowerCase().includes("packycode");

  // 如果不是 PackyCode 供应商，不显示测速组件
  if (!isPackyCodeProvider || appType !== "codex") {
    return null;
  }

  // 测试单个端点
  const testSingleEndpoint = async (
    endpoint: EndpointTest,
  ): Promise<EndpointTest> => {
    try {
      const result = await invoke<{ latency: number | null }>(
        "test_single_endpoint",
        {
          url: endpoint.url,
        },
      );
      return { ...endpoint, latency: result.latency };
    } catch (error) {
      console.error(`测试端点 ${endpoint.url} 失败:`, error);
      return { ...endpoint, latency: null };
    }
  };

  // 执行测速
  const performSpeedTest = useCallback(
    async (silent = false) => {
      if (!silent) setIsTestingSpeed(true);

      try {
        // 合并预设和自定义端点
        const allEndpoints = [...PRESET_CODEX_ENDPOINTS, ...customEndpoints];

        // 并行测试所有端点
        const results = await Promise.all(
          allEndpoints.map((endpoint) => testSingleEndpoint(endpoint)),
        );

        // 按延迟从低到高排序
        const sortedResults = results.sort((a, b) => {
          if (a.latency === null) return 1;
          if (b.latency === null) return -1;
          return a.latency - b.latency;
        });

        setTestResults(sortedResults);
        setHasTestedOnce(true);

        // 如果开启自动选择，选择延迟最低的端点
        if (autoSelectBest) {
          let bestEndpoint: EndpointTest | null = null;
          let minLatency = Infinity;

          for (const endpoint of results) {
            if (endpoint.latency !== null && endpoint.latency < minLatency) {
              minLatency = endpoint.latency;
              bestEndpoint = endpoint;
            }
          }

          if (bestEndpoint) {
            onUpdateBaseUrl(bestEndpoint.url);
          }
        }
      } catch (error) {
        console.error("测速失败:", error);
      } finally {
        if (!silent) setIsTestingSpeed(false);
      }
    },
    [customEndpoints, autoSelectBest, onUpdateBaseUrl],
  );

  // 添加自定义端点
  const addCustomEndpoint = () => {
    if (!customUrl.trim()) return;

    // 验证 URL 格式
    try {
      const url = new URL(customUrl);
      if (!url.protocol.startsWith("http")) {
        alert("请输入有效的 HTTP/HTTPS URL");
        return;
      }
    } catch {
      alert("请输入有效的 URL");
      return;
    }

    // 检查是否已存在
    const exists = [...PRESET_CODEX_ENDPOINTS, ...customEndpoints].some(
      (ep) => ep.url === customUrl,
    );

    if (exists) {
      alert("该地址已存在");
      return;
    }

    const newEndpoint: EndpointTest = {
      name: `自定义节点 ${customEndpoints.length + 1}`,
      url: customUrl,
      latency: null,
      isCustom: true,
    };

    setCustomEndpoints([...customEndpoints, newEndpoint]);
    setCustomUrl("");

    // 如果已经测速过，直接添加到结果中（未测试状态）
    if (hasTestedOnce) {
      // 已测速，直接添加未测试的端点到结果末尾
      setTestResults((prev) => [...prev, newEndpoint]);
    } else {
      // 未测速过，立即测试新添加的端点
      testSingleEndpoint(newEndpoint).then((result) => {
        setTestResults((prev) => {
          // 添加后重新排序
          const updated = [...prev, result];
          return updated.sort((a, b) => {
            if (a.latency === null) return 1;
            if (b.latency === null) return -1;
            return a.latency - b.latency;
          });
        });
      });
    }
  };

  // 删除自定义端点
  const removeCustomEndpoint = (url: string) => {
    setCustomEndpoints((prev) => prev.filter((ep) => ep.url !== url));
    setTestResults((prev) => prev.filter((ep) => ep.url !== url));
  };

  // 应用选定的端点
  const applyEndpoint = (url: string) => {
    onUpdateBaseUrl(url);
  };

  // 暴露测速功能供父组件调用（保存时自动测速）
  useEffect(() => {
    if (onSpeedTestRequired) {
      // 仅在未测速时执行自动测速
      window.codexSpeedTest = async () => {
        if (!hasTestedOnce) {
          await performSpeedTest();
        }
      };
    }
    return () => {
      if (window.codexSpeedTest) {
        delete window.codexSpeedTest;
      }
    };
  }, [hasTestedOnce, performSpeedTest]);

  return (
    <div className="mt-4 p-4 bg-gray-50 dark:bg-gray-800 rounded-lg">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">
          PackyCode 节点测速
        </h3>
        <button
          type="button"
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            performSpeedTest(false);
          }}
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
          id="auto-select-codex"
          checked={autoSelectBest}
          onChange={(e) => setAutoSelectBest(e.target.checked)}
          className="rounded border-gray-300 dark:border-gray-600"
        />
        <label
          htmlFor="auto-select-codex"
          className="text-sm text-gray-600 dark:text-gray-400"
        >
          自动选择延迟最低的节点
        </label>
      </div>

      {/* 自定义地址输入 */}
      <div className="mb-3">
        <div className="flex gap-2">
          <input
            type="url"
            value={customUrl}
            onChange={(e) => setCustomUrl(e.target.value)}
            onKeyPress={(e) => e.key === "Enter" && addCustomEndpoint()}
            placeholder="输入自定义 API 地址（如: https://your-api.com/v1）"
            className="flex-1 px-3 py-1.5 text-sm border border-gray-200 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-100 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500/20 dark:focus:ring-blue-400/20 focus:border-blue-500 dark:focus:border-blue-400"
          />
          <button
            type="button"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              addCustomEndpoint();
            }}
            className="px-3 py-1.5 bg-green-500 text-white rounded-md hover:bg-green-600 text-sm flex items-center gap-1"
          >
            <Plus className="w-4 h-4" />
            添加
          </button>
        </div>
      </div>

      {/* 测速结果 */}
      {(testResults.length > 0 || customEndpoints.length > 0) && (
        <div className="space-y-1">
          <h4 className="text-sm font-medium text-gray-600 dark:text-gray-400 mb-2">
            节点列表
          </h4>
          {/* 如果有测试结果，显示排序后的结果；否则显示原始列表 */}
          {(testResults.length > 0
            ? testResults
            : [...PRESET_CODEX_ENDPOINTS, ...customEndpoints]
          ).map((endpoint) => {
            const testResult = testResults.find((r) => r.url === endpoint.url);
            const isSelected = baseUrl === endpoint.url;
            const hasLatency =
              testResult?.latency !== null && testResult?.latency !== undefined;

            return (
              <div
                key={endpoint.url}
                className={`flex items-center justify-between p-2 rounded-md text-sm ${
                  isSelected
                    ? "bg-green-100 dark:bg-green-900/30 border border-green-300 dark:border-green-700"
                    : "bg-white dark:bg-gray-700 border border-gray-200 dark:border-gray-600"
                }`}
              >
                <div className="flex-1 flex items-center gap-2">
                  {isSelected && (
                    <Check className="w-4 h-4 text-green-600 dark:text-green-400" />
                  )}
                  <div className="flex-1">
                    <div className="font-medium">{endpoint.name}</div>
                    <div className="text-xs text-gray-500 dark:text-gray-400 break-all">
                      {endpoint.url}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-2 ml-2">
                  {testResult ? (
                    hasLatency ? (
                      <span
                        className={`font-mono text-xs ${
                          testResult.latency! < 100
                            ? "text-green-600 dark:text-green-400"
                            : testResult.latency! < 300
                              ? "text-yellow-600 dark:text-yellow-400"
                              : "text-red-600 dark:text-red-400"
                        }`}
                      >
                        {testResult.latency}ms
                      </span>
                    ) : (
                      <span className="text-gray-400 flex items-center gap-1 text-xs">
                        <AlertCircle className="w-3 h-3" />
                        超时
                      </span>
                    )
                  ) : (
                    <span className="text-gray-400 text-xs">未测试</span>
                  )}

                  {!autoSelectBest && hasLatency && (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        applyEndpoint(endpoint.url);
                      }}
                      disabled={isSelected}
                      className="px-2 py-0.5 text-xs bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {isSelected ? "已选择" : "选择"}
                    </button>
                  )}

                  {(endpoint as EndpointTest).isCustom && (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        removeCustomEndpoint(endpoint.url);
                      }}
                      className="p-1 text-red-500 hover:text-red-600"
                      title="删除自定义端点"
                    >
                      <Trash2 className="w-3 h-3" />
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {!hasTestedOnce && (
        <div className="mt-3 p-2 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-md">
          <p className="text-xs text-amber-700 dark:text-amber-300">
            💡 提示：保存时会自动测速并选择最佳节点
          </p>
        </div>
      )}
    </div>
  );
};

// 扩展 Window 接口以支持全局测速函数
declare global {
  interface Window {
    codexSpeedTest?: () => Promise<void>;
  }
}
