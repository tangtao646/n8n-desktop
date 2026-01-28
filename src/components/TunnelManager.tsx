import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn, Event } from "@tauri-apps/api/event";

// ========== 常量定义 ==========
const CLOUDFLARED_DEFAULT_PATH = "cloudflared";
const AUTO_START_DELAY_MS = 1000;

// ========== 类型定义 ==========
export type TunnelStatus = "offline" | "connecting" | "online" | "error";

interface TunnelEventPayload {
  status: string;
  url?: string;
  progress?: number;
  message?: string;
}

interface TunnelConfig {
  last_url?: string;
  auto_start: boolean;
  created_at: string;
  custom_domain?: string;
  use_custom_domain?: boolean;
  tunnel_mode?: "temporary" | "token";
  tunnel_token?: string;
}

export interface CloudflaredVersionInfo {
  installed: boolean;
  version?: string;
  path?: string;
  cached: boolean;
  cache_age_days?: number;
}

interface TunnelManagerProps {
  onStatusChange?: (status: TunnelStatus, url?: string) => void;
  className?: string;
}

type TunnelState = {
  status: TunnelStatus;
  url: string;
  isLoading: boolean;
  error: string;
};

// ========== 状态映射 ==========
const STATUS_DISPLAY_MAP: Record<TunnelStatus, { text: string; color: string }> = {
  offline: { text: "隧道已关闭", color: "text-gray-600" },
  connecting: { text: "隧道连接中...", color: "text-yellow-600" },
  online: { text: "隧道已连接", color: "text-green-600" },
  error: { text: "隧道错误", color: "text-red-600" },
};

// ========== 主组件 ==========
export default function TunnelManager({ onStatusChange, className = "" }: TunnelManagerProps) {
  // ========== 状态定义 ==========
  const [tunnelState, setTunnelState] = useState<TunnelState>({
    status: "offline",
    url: "",
    isLoading: false,
    error: "",
  });
  const [cloudflaredInfo, setCloudflaredInfo] = useState<CloudflaredVersionInfo | null>(null);
  const [config, setConfig] = useState<TunnelConfig>({
    auto_start: false,
    created_at: "",
    tunnel_mode: "temporary",
    tunnel_token: "",
    custom_domain: ""
  });
  const [showConfig, setShowConfig] = useState(false);
  const [isSavingConfig, setIsSavingConfig] = useState(false);

  // ========== 工具函数 ==========
  const getStatusDisplay = useCallback((status: TunnelStatus) => {
    return STATUS_DISPLAY_MAP[status] || { text: "未知状态", color: "text-gray-600" };
  }, []);

  const notifyParent = useCallback((status: TunnelStatus, url?: string) => {
    onStatusChange?.(status, url);
  }, [onStatusChange]);

  // ========== 核心逻辑函数 ==========
  const loadTunnelState = useCallback(async () => {
    try {
      const [status, tunnelConfig, versionInfo] = await Promise.all([
        invoke<TunnelEventPayload>("get_tunnel_status"),
        invoke<TunnelConfig>("get_tunnel_config"),
        invoke<CloudflaredVersionInfo>("check_cloudflared_version"),
      ]);

      setTunnelState(prev => ({
        ...prev,
        status: status.status.toLowerCase() as TunnelStatus,
        url: status.url || prev.url,
      }));
      setConfig(tunnelConfig);
      setCloudflaredInfo(versionInfo);

      notifyParent(status.status.toLowerCase() as TunnelStatus, status.url);
    } catch (err) {
      console.error("Failed to load tunnel state:", err);
      setTunnelState(prev => ({
        ...prev,
        error: "无法加载隧道状态",
      }));
    }
  }, [notifyParent]);

  const startTunnel = useCallback(async () => {
    if (tunnelState.isLoading) return;

    setTunnelState(prev => ({
      ...prev,
      isLoading: true,
      error: "",
      status: "connecting"
    }));

    try {
      const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
      const cloudflaredPath = versionInfo.installed
        ? versionInfo.path || CLOUDFLARED_DEFAULT_PATH
        : CLOUDFLARED_DEFAULT_PATH;

      if (!versionInfo.installed) {
        setTunnelState(prev => ({
          ...prev,
          error: "正在下载 cloudflared...",
        }));
      }

      await invoke("start_tunnel", { cloudflaredPath });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      console.error("Failed to start tunnel:", err);
      setTunnelState(prev => ({
        ...prev,
        error: `启动隧道失败: ${errorMessage}`,
        status: "error",
        isLoading: false,
      }));
    }
  }, [tunnelState.isLoading]);

  const stopTunnel = useCallback(async () => {
    if (tunnelState.isLoading) return;

    setTunnelState(prev => ({ ...prev, isLoading: true, error: "" }));

    try {
      await invoke("stop_tunnel");
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      console.error("Failed to stop tunnel:", err);
      setTunnelState(prev => ({
        ...prev,
        error: `停止隧道失败: ${errorMessage}`,
        isLoading: false,
      }));
    }
  }, [tunnelState.isLoading]);

  const copyTunnelUrl = useCallback(async () => {
    if (!tunnelState.url) return;

    try {
      await invoke("copy_tunnel_url");
      alert("隧道 URL 已复制到剪贴板");
    } catch (err) {
      console.error("Failed to copy URL:", err);
      setTunnelState(prev => ({
        ...prev,
        error: "复制 URL 失败",
      }));
    }
  }, [tunnelState.url]);

  const updateConfig = useCallback(async (updates: Partial<TunnelConfig>) => {
    try {
      await invoke("update_tunnel_config", updates);
      await loadTunnelState();
    } catch (err) {
      console.error("Failed to update config:", err);
      setTunnelState(prev => ({
        ...prev,
        error: "更新配置失败",
      }));
    }
  }, [loadTunnelState]);

  const clearCloudflaredCache = useCallback(async () => {
    try {
      await invoke("clear_cloudflared_cache");
      await loadTunnelState();
      alert("cloudflared 缓存已清理");
    } catch (err) {
      console.error("Failed to clear cache:", err);
      setTunnelState(prev => ({
        ...prev,
        error: "清理缓存失败",
      }));
    }
  }, [loadTunnelState]);

  const applyTunnelConfig = useCallback(async () => {
    if (isSavingConfig) return;
    
    setIsSavingConfig(true);
    try {
      await invoke("apply_tunnel_config", {
        tunnelMode: config.tunnel_mode || "temporary",
        customDomain: config.custom_domain || null,
        tunnelToken: config.tunnel_token || null,
      });
      alert("隧道配置已保存");
    } catch (err) {
      console.error("Failed to apply tunnel config:", err);
      setTunnelState(prev => ({
        ...prev,
        error: "保存配置失败",
      }));
    } finally {
      setIsSavingConfig(false);
    }
  }, [config, isSavingConfig]);

  // ========== 事件处理函数 ==========
  const handleTunnelUpdate = useCallback((event: Event<TunnelEventPayload>) => {
    const { status, url, message } = event.payload;
    const newStatus = status.toLowerCase() as TunnelStatus;

    setTunnelState(prev => {
      const updates: Partial<TunnelState> = {
        status: newStatus,
        url: url || prev.url,
      };

      // 处理错误消息
      if (message && (newStatus === "error" || newStatus === "connecting")) {
        updates.error = message;
      } else if (newStatus === "online") {
        updates.isLoading = false;
        updates.error = "";
      } else if (newStatus === "error" || newStatus === "offline") {
        updates.isLoading = false;
        // 保留现有的错误消息，除非有新的消息
        if (!message) {
          updates.error = prev.error;
        }
      }

      // 如果是连接中状态，更新错误消息但不停止加载
      if (newStatus === "connecting" && message) {
        updates.error = message;
        updates.isLoading = true;
      }

      return { ...prev, ...updates };
    });

    notifyParent(newStatus, url);
  }, [notifyParent]);

  // ========== 副作用 ==========
  useEffect(() => {
    let unlistenTunnelUpdate: UnlistenFn | null = null;
    let unlistenTunnelCopied: UnlistenFn | null = null;

    const setupListeners = async () => {
      try {
        unlistenTunnelUpdate = await listen<TunnelEventPayload>("tunnel-update", handleTunnelUpdate);
        unlistenTunnelCopied = await listen<TunnelEventPayload>("tunnel-copied", () => {
          console.log("Tunnel URL copied successfully");
        });

        await loadTunnelState();

        if (config.auto_start && tunnelState.status === "offline" && config.last_url) {
          setTimeout(() => {
            startTunnel();
          }, AUTO_START_DELAY_MS);
        }
      } catch (err) {
        console.error("Failed to setup tunnel listeners:", err);
        setTunnelState(prev => ({
          ...prev,
          error: "无法设置隧道监听器",
        }));
      }
    };

    setupListeners();

    return () => {
      unlistenTunnelUpdate?.();
      unlistenTunnelCopied?.();
    };
  }, [config.auto_start, loadTunnelState, startTunnel, tunnelState.status, config.last_url, handleTunnelUpdate]);

  // ========== 渲染函数 ==========
  const renderStatusCard = () => {
    const { text, color } = getStatusDisplay(tunnelState.status);
    const isStopped = tunnelState.status === "offline" || tunnelState.status === "error";
    const isConnecting = tunnelState.status === "connecting";
    const isError = tunnelState.status === "error";

    return (
      <div className="bg-gray-50 border border-gray-200 rounded-lg p-4 mb-4">
        <div className="flex items-center justify-between">
          <div>
            <div className="flex items-center">
              <span className={`font-medium ${color}`}>
                {text}
              </span>
              {isConnecting && tunnelState.isLoading && (
                <div className="ml-3 flex items-center">
                  <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-blue-600"></div>
                  <span className="ml-2 text-sm text-gray-600">正在连接...</span>
                </div>
              )}
            </div>
            {tunnelState.url && (
              <div className="mt-1">
                <span className="text-sm text-gray-600">公网地址: </span>
                <a
                  href={tunnelState.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-sm text-blue-600 hover:text-blue-800 break-all"
                >
                  {tunnelState.url}
                </a>
              </div>
            )}
          </div>
          
          <div className="flex space-x-2">
            {isStopped ? (
              <button
                onClick={startTunnel}
                disabled={tunnelState.isLoading}
                className="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 disabled:opacity-50"
              >
                {tunnelState.isLoading ? "启动中..." : "启动隧道"}
              </button>
            ) : (
              <button
                onClick={stopTunnel}
                disabled={tunnelState.isLoading}
                className="px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700 disabled:opacity-50"
              >
                {tunnelState.isLoading ? "停止中..." : "停止隧道"}
              </button>
            )}
            
            {tunnelState.url && (
              <button
                onClick={copyTunnelUrl}
                className="px-4 py-2 bg-gray-200 text-gray-800 rounded hover:bg-gray-300"
              >
                复制
              </button>
            )}

            {isError && !tunnelState.isLoading && (
              <button
                onClick={startTunnel}
                className="px-4 py-2 bg-yellow-600 text-white rounded hover:bg-yellow-700"
              >
                重试
              </button>
            )}
          </div>
        </div>

        {tunnelState.error && (
          <div className={`mt-2 p-2 rounded text-sm ${
            isError ? 'bg-red-50 text-red-700' :
            isConnecting ? 'bg-yellow-50 text-yellow-700' :
            'bg-gray-50 text-gray-700'
          }`}>
            <div className="flex items-start">
              <span className="flex-shrink-0">
                {isError ? '❌' : isConnecting ? '⚠️' : 'ℹ️'}
              </span>
              <span className="ml-2">{tunnelState.error}</span>
            </div>
            {isError && (
              <div className="mt-2 text-xs">
                <p className="text-gray-600">可能的原因:</p>
                <ul className="list-disc pl-5 mt-1 space-y-1">
                  <li>网络连接不稳定</li>
                  <li>Cloudflare 服务暂时不可用</li>
                  <li>防火墙或代理设置阻止连接</li>
                  <li>尝试点击"重试"按钮重新连接</li>
                </ul>
              </div>
            )}
          </div>
        )}

        {cloudflaredInfo && (
          <div className="mt-3 text-sm text-gray-600">
            <div className="flex items-center">
              <span className="font-medium">cloudflared: </span>
              <span className="ml-1">
                {cloudflaredInfo.installed
                  ? `已安装 ${cloudflaredInfo.version || "未知版本"}`
                  : "未安装"}
              </span>
              {cloudflaredInfo.cached && (
                <span className="ml-2 text-xs bg-gray-100 px-2 py-1 rounded">
                  缓存 ({cloudflaredInfo.cache_age_days || 0}天前)
                </span>
              )}
            </div>
          </div>
        )}
      </div>
    );
  };

  const renderConfigPanel = () => {
    if (!showConfig) return null;

    return (
      <div className="mt-4 p-4 bg-gray-50 rounded-lg border border-gray-200">
        <h4 className="font-medium mb-3">隧道配置</h4>
        
        <div className="space-y-4">
          {/* 隧道模式选择 */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">隧道模式</label>
            <div className="flex gap-2">
              {(["temporary", "token"] as const).map((mode) => (
                <button
                  key={mode}
                  onClick={() => setConfig(prev => ({ ...prev, tunnel_mode: mode }))}
                  className={`flex-1 px-3 py-2 text-sm rounded-md transition-colors ${
                    config.tunnel_mode === mode
                      ? "bg-blue-600 text-white"
                      : "bg-gray-100 text-gray-700 hover:bg-gray-200"
                  }`}
                >
                  {mode === "temporary" ? "随机临时域名" : "固定自定义域名"}
                </button>
              ))}
            </div>
            <p className="text-xs text-gray-500 mt-1">
              {config.tunnel_mode === "temporary"
                ? "每次启动随机生成临时公网地址"
                : "使用 Cloudflare Tunnel Token 建立固定隧道"}
            </p>
          </div>

          {/* Token 模式配置 */}
          {config.tunnel_mode === "token" && (
            <>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  自定义域名
                </label>
                <input
                  type="text"
                  value={config.custom_domain || ""}
                  onChange={(e) => setConfig(prev => ({ ...prev, custom_domain: e.target.value }))}
                  placeholder="https://your-domain.com"
                  className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
                <p className="text-xs text-gray-500 mt-1">
                  在 Cloudflare DNS 中配置 CNAME 记录指向你的隧道
                </p>
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Tunnel Token
                </label>
                <textarea
                  value={config.tunnel_token || ""}
                  onChange={(e) => setConfig(prev => ({ ...prev, tunnel_token: e.target.value }))}
                  placeholder="粘贴您的 Cloudflare Tunnel Token..."
                  className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono"
                  rows={3}
                />
                <p className="text-xs text-gray-500 mt-1">
                  从 Cloudflare 仪表盘复制 Tunnel 的连接 Token
                  <a
                    href="https://dash.cloudflare.com/profile/api-tokens"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-blue-600 hover:text-blue-800 ml-1"
                  >
                    (获取 Token)
                  </a>
                </p>
              </div>
            </>
          )}

          {/* 自动启动配置 */}
          <div className="flex items-center">
            <input
              type="checkbox"
              id="auto-start"
              checked={config.auto_start}
              onChange={(e) => updateConfig({ auto_start: e.target.checked })}
              className="mr-2"
            />
            <label htmlFor="auto-start" className="text-sm">
              应用启动时自动连接隧道
            </label>
          </div>

          {config.last_url && (
            <div className="text-sm">
              <div className="font-medium">上次隧道地址:</div>
              <div className="text-gray-600 break-all">{config.last_url}</div>
            </div>
          )}

          {/* 配置操作按钮 */}
          <div className="pt-2 border-t space-y-3">
            <button
              onClick={applyTunnelConfig}
              disabled={isSavingConfig}
              className="w-full px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed text-sm"
            >
              {isSavingConfig ? "保存中..." : "保存隧道配置"}
            </button>

            <button
              onClick={clearCloudflaredCache}
              className="px-3 py-1 text-sm bg-gray-200 text-gray-800 rounded hover:bg-gray-300"
            >
              清理 cloudflared 缓存
            </button>
            <p className="text-xs text-gray-500 mt-1">
              清理下载的 cloudflared 二进制文件缓存
            </p>
          </div>
        </div>
      </div>
    );
  };

  const renderInstructions = () => (
    <div className="mt-4 text-sm text-gray-600">
      <p className="font-medium">使用说明:</p>
      <ul className="list-disc pl-5 mt-1 space-y-1">
        <li>选择"随机临时域名"模式：每次启动获得随机临时地址</li>
        <li>选择"固定自定义域名"模式：配置 Token 和域名获得固定地址</li>
        <li>启动隧道后，将获得公网地址用于 OAuth 回调、Webhook 等</li>
        <li>隧道关闭后地址失效，确保隐私安全</li>
        <li>复制地址用于 Google OAuth、GitHub Webhook 等配置</li>
      </ul>
    </div>
  );

  // ========== 主渲染 ==========
  return (
    <div className={`space-y-4 ${className}`}>
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold">Cloudflare 隧道</h3>
        <button
          onClick={() => setShowConfig(!showConfig)}
          className="text-sm text-gray-500 hover:text-gray-700"
        >
          {showConfig ? "隐藏配置" : "显示配置"}
        </button>
      </div>

      {renderStatusCard()}
      {renderConfigPanel()}
      {renderInstructions()}
    </div>
  );
}
