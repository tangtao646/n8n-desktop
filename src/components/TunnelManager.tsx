import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn, Event } from "@tauri-apps/api/event";
import { useAutoSync } from "../hooks/useAutoSync";
import { useI18n } from "../i18n/context";

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
// 这些映射现在将在组件内部使用翻译

// ========== 主组件 ==========
export default function TunnelManager({ onStatusChange, className = "" }: TunnelManagerProps) {
  const { t } = useI18n();

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
    const statusMap: Record<TunnelStatus, { text: string; color: string }> = {
      offline: { text: t("ui.disabled"), color: "text-gray-600" },
      connecting: { text: t("ui.starting"), color: "text-yellow-600" },
      online: { text: t("ui.enabled"), color: "text-green-600" },
      error: { text: t("messages.tunnel_start_failed"), color: "text-red-600" },
    };
    return statusMap[status] || { text: t("ui.disabled"), color: "text-gray-600" };
  }, [t]);

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
        error: t("messages.operation_failed_retry"),
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
          error: t("ui.starting"),
        }));
      }

      await invoke("start_tunnel", { cloudflaredPath });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      console.error("Failed to start tunnel:", err);
      setTunnelState(prev => ({
        ...prev,
        error: `${t("messages.tunnel_start_failed")}: ${errorMessage}`,
        status: "error",
        isLoading: false,
      }));
    }
  }, [tunnelState.isLoading, t]);

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
        error: `${t("messages.tunnel_stop_failed")}: ${errorMessage}`,
        isLoading: false,
      }));
    }
  }, [tunnelState.isLoading, t]);

  const copyTunnelUrl = useCallback(async () => {
    if (!tunnelState.url) return;

    try {
      await invoke("copy_tunnel_url");
      alert(t("messages.copy_url_success"));
    } catch (err) {
      console.error("Failed to copy URL:", err);
      setTunnelState(prev => ({
        ...prev,
        error: t("messages.copy_url_failed"),
      }));
    }
  }, [tunnelState.url, t]);

  const updateConfig = useCallback(async (updates: Partial<TunnelConfig>) => {
    try {
      await invoke("update_tunnel_config", updates);
      await loadTunnelState();
    } catch (err) {
      console.error("Failed to update config:", err);
      setTunnelState(prev => ({
        ...prev,
        error: t("messages.update_config_failed"),
      }));
    }
  }, [loadTunnelState, t]);

  const clearCloudflaredCache = useCallback(async () => {
    try {
      await invoke("clear_cloudflared_cache");
      await loadTunnelState();
      alert(t("messages.cache_cleared"));
    } catch (err) {
      console.error("Failed to clear cache:", err);
      setTunnelState(prev => ({
        ...prev,
        error: t("messages.clear_cache_failed"),
      }));
    }
  }, [loadTunnelState, t]);

  const applyTunnelConfig = useCallback(async () => {
    if (isSavingConfig) return;

    setIsSavingConfig(true);
    try {
      await invoke("apply_tunnel_config", {
        tunnelMode: config.tunnel_mode || "temporary",
        customDomain: config.custom_domain || null,
        tunnelToken: config.tunnel_token || null,
      });
      alert(t("messages.config_saved"));
    } catch (err) {
      console.error("Failed to apply tunnel config:", err);
      setTunnelState(prev => ({
        ...prev,
        error: t("messages.save_config_failed"),
      }));
    } finally {
      setIsSavingConfig(false);
    }
  }, [config, isSavingConfig, t]);

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

  // ========== 自动同步 Hook ==========
  // 监听 app://sync-state 事件，自动重新加载隧道状态
  useAutoSync(() => {
    console.log('[TunnelManager] 收到同步事件，重新加载隧道状态');
    loadTunnelState();
  });

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
          error: t("messages.operation_failed_retry"),
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
                  <span className="ml-2 text-sm text-gray-600">{t("ui.starting")}</span>
                </div>
              )}
            </div>
            {tunnelState.url && (
              <div className="mt-1">
                <span className="text-sm text-gray-600">{t("ui.public_address")}: </span>
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
                {tunnelState.isLoading ? t("ui.starting") : t("buttons.start_tunnel")}
              </button>
            ) : (
              <button
                onClick={stopTunnel}
                disabled={tunnelState.isLoading}
                className="px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700 disabled:opacity-50"
              >
                {tunnelState.isLoading ? t("ui.saving") : t("buttons.stop_tunnel")}
              </button>
            )}

            {tunnelState.url && (
              <button
                onClick={copyTunnelUrl}
                className="px-4 py-2 bg-gray-200 text-gray-800 rounded hover:bg-gray-300"
              >
                {t("buttons.copy")}
              </button>
            )}

            {isError && !tunnelState.isLoading && (
              <button
                onClick={startTunnel}
                className="px-4 py-2 bg-yellow-600 text-white rounded hover:bg-yellow-700"
              >
                {t("buttons.retry")}
              </button>
            )}
          </div>
        </div>

        {tunnelState.error && (
          <div className={`mt-2 p-2 rounded text-sm ${isError ? 'bg-red-50 text-red-700' :
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
                <p className="text-gray-600">{t("ui.risk_warning")}</p>
                <ul className="list-disc pl-5 mt-1 space-y-1">
                  <li>{t("error_reasons.unstable_network")}</li>
                  <li>{t("error_reasons.cloudflare_service_unavailable")}</li>
                  <li>{t("error_reasons.firewall_proxy_blocking")}</li>
                  <li>{t("error_reasons.try_retry_button")}</li>
                </ul>
              </div>
            )}
          </div>
        )}

        {cloudflaredInfo && (
          <div className="mt-3 text-sm text-gray-600">
            <div className="flex items-center">
              <span className="font-medium">{t("ui.cloudflared")}: </span>
              <span className="ml-1">
                {cloudflaredInfo.installed
                  ? `${t("ui.installed")} ${cloudflaredInfo.version || t("ui.current_version")}`
                  : t("ui.not_installed_click_to_download")}
              </span>
              {cloudflaredInfo.cached && (
                <span className="ml-2 text-xs bg-gray-100 px-2 py-1 rounded">
                  {t("ui.cache")} ({cloudflaredInfo.cache_age_days || 0}天前)
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
        <h4 className="font-medium mb-3">{t("ui.tunnel_configuration")}</h4>

        <div className="space-y-4">
          {/* 隧道模式选择 */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">{t("ui.tunnel_mode")}</label>
            <div className="flex gap-2">
              {(["temporary", "token"] as const).map((mode) => (
                <button
                  key={mode}
                  onClick={() => setConfig(prev => ({ ...prev, tunnel_mode: mode }))}
                  className={`flex-1 px-3 py-2 text-sm rounded-md transition-colors ${config.tunnel_mode === mode
                    ? "bg-blue-600 text-white"
                    : "bg-gray-100 text-gray-700 hover:bg-gray-200"
                    }`}
                >
                  {mode === "temporary" ? t("ui.random_temporary_domain") : t("ui.fixed_custom_domain")}
                </button>
              ))}
            </div>
            <p className="text-xs text-gray-500 mt-1">
              {config.tunnel_mode === "temporary"
                ? t("ui.random_temporary_domain_desc")
                : t("ui.fixed_custom_domain_desc")}
            </p>
          </div>

          {/* Token 模式配置 */}
          {config.tunnel_mode === "token" && (
            <>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  {t("ui.custom_domain")}
                </label>
                <input
                  type="text"
                  value={config.custom_domain || ""}
                  onChange={(e) => setConfig(prev => ({ ...prev, custom_domain: e.target.value }))}
                  placeholder="https://your-domain.com"
                  className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
                <p className="text-xs text-gray-500 mt-1">
                  {t("ui.configure_cname_in_cloudflare_dns")}
                </p>
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  {t("ui.tunnel_token")}
                </label>
                <textarea
                  value={config.tunnel_token || ""}
                  onChange={(e) => setConfig(prev => ({ ...prev, tunnel_token: e.target.value }))}
                  placeholder={t("ui.paste_tunnel_token")}
                  className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono"
                  rows={3}
                />
                <p className="text-xs text-gray-500 mt-1">
                  {t("ui.associate_cloudflare_account")}
                  <a
                    href="https://dash.cloudflare.com/profile/api-tokens"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-blue-600 hover:text-blue-800 ml-1"
                  >
                    ({t("ui.get_token")})
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
              {t("ui.auto_start_tunnel_on_app_launch")}
            </label>
          </div>

          {config.last_url && (
            <div className="text-sm">
              <div className="font-medium">{t("ui.last_tunnel_address")}:</div>
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
              {isSavingConfig ? t("ui.saving") : t("buttons.save_tunnel_config")}
            </button>

            <button
              onClick={clearCloudflaredCache}
              className="px-3 py-1 text-sm bg-gray-200 text-gray-800 rounded hover:bg-gray-300"
            >
              {t("ui.clear_cloudflared_cache")}
            </button>
            <p className="text-xs text-gray-500 mt-1">
              {t("ui.clear_cloudflared_cache_desc")}
            </p>
          </div>
        </div>
      </div>
    );
  };

  const renderInstructions = () => (
    <div className="mt-4 text-sm text-gray-600">
      <p className="font-medium">{t("ui.instructions")}:</p>
      <ul className="list-disc pl-5 mt-1 space-y-1">
        <li>{t("instructions.random_temporary_domain")}</li>
        <li>{t("instructions.fixed_custom_domain")}</li>
        <li>{t("instructions.start_tunnel_for_public_address")}</li>
        <li>{t("instructions.tunnel_closes_address_invalid")}</li>
        <li>{t("instructions.copy_address_for_oauth_webhook")}</li>
      </ul>
    </div>
  );

  // ========== 主渲染 ==========
  return (
    <div className={`space-y-4 ${className}`}>
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold">{t("ui.tunnel")}</h3>
        <button
          onClick={() => setShowConfig(!showConfig)}
          className="text-sm text-gray-500 hover:text-gray-700"
        >
          {showConfig ? t("app.hide_tunnel") : t("app.show_tunnel")}
        </button>
      </div>

      {renderStatusCard()}
      {renderConfigPanel()}
      {renderInstructions()}
    </div>
  );
}
