import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn, Event } from "@tauri-apps/api/event";
import { useAutoSync, generateTimestampedUrl } from "../hooks/useAutoSync";
import { useI18n } from "../i18n/context";

// ========== 常量定义 ==========
const CLOUDFLARED_DEFAULT_PATH = "cloudflared";
const TUNNEL_START_TIMEOUT_MS = 60000;
const TUNNEL_STOP_TIMEOUT_MS = 5000;
const N8N_LOCAL_ADDRESS = "http://localhost:5678";
const DEFAULT_APP_VERSION = "1.0.2";

// ========== 类型定义 ==========
type TunnelStatus = "offline" | "connecting" | "online" | "error";
type N8nStatus = "running" | "stopped" | "starting";

// 隧道状态机
type TunnelState = "READY" | "STARTING" | "RUNNING";

interface TunnelEventPayload {
  status: string;
  url?: string;
  progress?: number;
  message?: string;
}

interface CloudflaredVersionInfo {
  installed: boolean;
  version?: string;
  path?: string;
  cached: boolean;
  cache_age_days?: number;
}

interface TunnelConfig {
  custom_domain?: string;
  use_custom_domain?: boolean;
  tunnel_mode?: any; // 支持 Temporary 字符串或 Token 对象
  tunnel_token?: string;
  [key: string]: unknown;
}

interface SidebarPanelProps {
  collapsed?: boolean;
  onToggleSidebar?: () => void;
  className?: string;
}

type LoadingState = {
  tunnel: boolean;
  update: boolean;
  nodeUnblock: boolean;
  domainConfig: boolean;
};

type AppState = {
  tunnelStatus: TunnelStatus;
  tunnelUrl: string;
  n8nStatus: N8nStatus;
  nodeUnblockEnabled: boolean;
  tunnelDomain: string; // Domain for Token mode
  tunnelMode: "temporary" | "token"; // 隧道模式
  tunnelToken: string; // Cloudflare Tunnel Token
  tunnelState: TunnelState; // 隧道状态机
};

// ========== 状态映射 ==========
// 这些映射现在将在组件内部使用翻译

// ========== 主组件 ==========
export default function SidebarPanel({ collapsed = false, onToggleSidebar, className = "" }: SidebarPanelProps) {
  const { t } = useI18n();

  // ========== 状态定义 ==========
  const [appState, setAppState] = useState<AppState>({
    tunnelStatus: "offline",
    tunnelUrl: "",
    n8nStatus: "running",
    nodeUnblockEnabled: false,
    tunnelDomain: "",
    tunnelMode: "temporary",
    tunnelToken: "",
    tunnelState: "READY",
  });

  // 从后端返回的 tunnelMode 中提取 token 和 domain（Token 模式）
  const extractTokenModeConfig = (tunnelMode: any) => {
    if (tunnelMode && typeof tunnelMode === "object" && tunnelMode.Token) {
      return {
        token: tunnelMode.Token.token || "",
        domain: tunnelMode.Token.domain || "",
      };
    }
    return null;
  };

  const [loading, setLoading] = useState<LoadingState>({
    tunnel: false,
    update: false,
    nodeUnblock: false,
    domainConfig: false,
  });

  // const [authPollingInterval, setAuthPollingInterval] = useState<NodeJS.Timeout | null>(null);

  const [cloudflaredInfo, setCloudflaredInfo] = useState<CloudflaredVersionInfo | null>(null);
  const [appVersion] = useState<string>(DEFAULT_APP_VERSION);

  // ========== 工具函数 ==========
  const getN8nStatusDisplay = useCallback((status: N8nStatus) => {
    const statusMap: Record<N8nStatus, { text: string; color: string }> = {
      running: { text: t("ui.enabled"), color: "text-green-600" },
      stopped: { text: t("ui.disabled"), color: "text-red-600" },
      starting: { text: t("ui.starting"), color: "text-yellow-600" },
    };
    return statusMap[status] || { text: t("ui.disabled"), color: "text-gray-600" };
  }, [t]);

  const getTunnelStatusDisplay = useCallback((status: TunnelStatus) => {
    const statusMap: Record<TunnelStatus, { text: string; color: string }> = {
      offline: { text: t("ui.disabled"), color: "text-gray-600" },
      connecting: { text: t("ui.starting"), color: "text-yellow-600" },
      online: { text: t("ui.enabled"), color: "text-green-600" },
      error: { text: t("messages.tunnel_start_failed"), color: "text-red-600" },
    };
    return statusMap[status] || { text: t("ui.disabled"), color: "text-gray-600" };
  }, [t]);

  // 检查当前隧道模式配置是否有效
  const isTunnelConfigValid = useCallback(() => {
    if (appState.tunnelMode === "temporary") {
      // Temporary 模式无需额外配置
      return true;
    } else if (appState.tunnelMode === "token") {
      // Token 模式需要检查 domain 和 token 都不为空
      const tokenTrimmed = appState.tunnelToken.trim();
      const domainTrimmed = appState.tunnelDomain.trim();

      // 检查基本的非空条件
      if (!tokenTrimmed || !domainTrimmed) {
        return false;
      }

      // 检查 token 长度（Cloudflare Token 通常很长）
      if (tokenTrimmed.length < 50) {
        return false;
      }

      // 检查 domain 格式（必须包含 :// ）
      if (!domainTrimmed.includes("://")) {
        return false;
      }

      return true;
    }
    return false;
  }, [appState.tunnelMode, appState.tunnelToken, appState.tunnelDomain]);

  const updateLoadingState = useCallback((updates: Partial<LoadingState>) => {
    setLoading(prev => ({ ...prev, ...updates }));
  }, []);

  const updateAppState = useCallback((updates: Partial<AppState>) => {
    setAppState(prev => ({ ...prev, ...updates }));
  }, []);

  // ========== 错误处理函数 ==========
  const handleError = useCallback((error: unknown, context: string) => {
    console.error(`[SidebarPanel] ${context}:`, error);

    let errorMessage = "发生未知错误";
    let userMessage = "操作失败，请稍后重试";

    if (error instanceof Error) {
      errorMessage = error.message;

      // 根据错误类型提供更友好的中文提示
      if (errorMessage.includes("cloudflared")) {
        userMessage = "cloudflared 工具出现问题。请检查网络连接或手动安装 cloudflared。";
      } else if (errorMessage.includes("network") || errorMessage.includes("连接")) {
        userMessage = "网络连接失败。请检查您的网络设置并重试。";
      } else if (errorMessage.includes("timeout") || errorMessage.includes("超时")) {
        userMessage = "操作超时。请检查网络连接并重试。";
      } else if (errorMessage.includes("permission") || errorMessage.includes("权限")) {
        userMessage = "权限不足。请检查文件系统权限或使用管理员权限运行。";
      } else if (errorMessage.includes("download") || errorMessage.includes("下载")) {
        userMessage = "下载失败。请检查网络连接或手动下载所需文件。";
      } else if (errorMessage.includes("auth") || errorMessage.includes("授权")) {
        userMessage = "授权失败。请检查 Cloudflare 账号设置并确保已正确配置 API Token。";
      }
    }

    // 显示错误提示给用户
    alert(`❌ ${context}\n\n${userMessage}\n\n错误详情: ${errorMessage}`);

    return userMessage;
  }, []);



  // 清理轮询定时器
  // useEffect(() => {
  //   return () => {
  //     if (authPollingInterval) {
  //       clearInterval(authPollingInterval);
  //     }
  //   };
  // }, [authPollingInterval]);

  // ========== 核心逻辑函数 ==========
  const loadAppInfo = useCallback(async () => {
    try {
      const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
      setCloudflaredInfo(versionInfo);

      // 加载隧道配置
      try {
        const config = await invoke<TunnelConfig>("get_tunnel_config");
        console.log("[SidebarPanel] Loaded tunnel config:", config);

        // 处理 Token 模式：从 tunnelMode 中提取 token 和 domain
        if (config.tunnel_mode) {
          const tokenModeConfig = extractTokenModeConfig(config.tunnel_mode);
          if (tokenModeConfig) {
            console.log("[SidebarPanel] Extracted Token mode config:", tokenModeConfig);
            updateAppState({
              tunnelMode: "token",
              tunnelToken: tokenModeConfig.token,
              tunnelDomain: tokenModeConfig.domain,
            });
          } else if (config.tunnel_mode === "temporary" || config.tunnel_mode === "Temporary") {
            updateAppState({ tunnelMode: "temporary" });
          }
        }

        // 兼容旧格式的 custom_domain 字段
        if (config.custom_domain && !config.tunnel_mode) {
          updateAppState({ tunnelDomain: config.custom_domain });
        }
        if (config.tunnel_token) {
          updateAppState({ tunnelToken: config.tunnel_token });
        }
      } catch (err) {
        console.error("Failed to load tunnel config:", err);
        // 不显示错误提示，因为可能是首次运行没有配置
      }
    } catch (err) {
      console.error("Failed to load app info:", err);
      // 不显示错误提示，避免干扰用户体验
    }
  }, [updateAppState]);

  const checkN8nStatus = useCallback(async () => {
    try {
      // 这里可以调用后端命令检查 n8n 状态
      updateAppState({ n8nStatus: "running" });
    } catch (err) {
      console.error("Failed to check n8n status:", err);
      updateAppState({ n8nStatus: "stopped" });
    }
  }, [updateAppState]);

  const startTunnel = useCallback(async () => {
    if (loading.tunnel) return;

    // 再次检查配置有效性（防止竞态条件）
    if (!isTunnelConfigValid()) {
      alert("❌ 启动隧道失败\n\n请确保隧道配置完整且有效");
      return;
    }

    updateLoadingState({ tunnel: true });
    updateAppState({
      tunnelStatus: "connecting",
      tunnelState: "STARTING"
    });

    try {
      // 首先，确保当前的隧道模式配置已保存
      const tunnelModeToSave = appState.tunnelMode === "token"
        ? { Token: { token: appState.tunnelToken.trim(), domain: appState.tunnelDomain.trim() } }
        : "Temporary";

      console.log("[SidebarPanel] 在启动前应用隧道配置:", tunnelModeToSave);

      await invoke("apply_tunnel_config", {
        tunnelMode: tunnelModeToSave,
        customDomain: appState.tunnelDomain.trim() || null,
        tunnelToken: appState.tunnelToken.trim() || null,
      });

      const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
      let cloudflaredPath = CLOUDFLARED_DEFAULT_PATH;

      if (versionInfo.installed && versionInfo.path) {
        cloudflaredPath = versionInfo.path;
      } else {
        try {
          await invoke("download_cloudflared");
          const newVersionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");

          if (!newVersionInfo.installed) {
            throw new Error("下载 cloudflared 失败，请检查网络连接或手动安装 cloudflared");
          }

          if (newVersionInfo.path) {
            cloudflaredPath = newVersionInfo.path;
          }
        } catch (downloadErr) {
          const errorMessage = downloadErr instanceof Error ? downloadErr.message : String(downloadErr);
          throw new Error(`cloudflared 下载失败: ${errorMessage}. 请手动安装 cloudflared 或检查网络连接。`);
        }
      }

      await invoke("start_tunnel", { cloudflaredPath });

      // 设置超时
      setTimeout(() => {
        if (appState.tunnelStatus === "connecting") {
          updateLoadingState({ tunnel: false });
          updateAppState({
            tunnelStatus: "offline",
            tunnelState: "READY"
          });
        }
      }, TUNNEL_START_TIMEOUT_MS);
    } catch (err) {
      updateAppState({
        tunnelStatus: "error",
        tunnelState: "READY"
      });
      updateLoadingState({ tunnel: false });

      handleError(err, "启动隧道失败");
    }
  }, [loading.tunnel, appState.tunnelStatus, appState.tunnelMode, appState.tunnelToken, appState.tunnelDomain, isTunnelConfigValid, updateLoadingState, updateAppState, handleError]);

  const stopTunnel = useCallback(async () => {
    if (loading.tunnel) return;

    updateLoadingState({ tunnel: true });

    try {
      await invoke("stop_tunnel");

      // 停止隧道后，状态应该回到 READY
      updateAppState({
        tunnelStatus: "offline",
        tunnelState: "READY"
      });

      setTimeout(() => {
        updateLoadingState({ tunnel: false });
      }, TUNNEL_STOP_TIMEOUT_MS);
    } catch (err) {
      updateLoadingState({ tunnel: false });
      handleError(err, "停止隧道失败");
    }
  }, [loading.tunnel, updateLoadingState, updateAppState, handleError]);

  const saveCustomDomainConfig = useCallback(async () => {
    if (loading.domainConfig) return;

    updateLoadingState({ domainConfig: true });

    try {
      const domainToSave = appState.tunnelDomain.trim();
      const tokenToSave = appState.tunnelToken.trim();

      // 验证输入
      if (appState.tunnelMode === "token") {
        if (!tokenToSave) {
          alert("❌ 配置验证失败\n\n请输入 Cloudflare Tunnel Token");
          updateLoadingState({ domainConfig: false });
          return;
        }
        // 简单的 Token 格式验证（Cloudflare Token 通常很长）
        if (tokenToSave.length < 50) {
          alert("❌ 配置验证失败\n\nToken 格式似乎不正确，请确保复制完整的 Token\n\nCloudflare Tunnel Token 通常长度超过 50 个字符");
          updateLoadingState({ domainConfig: false });
          return;
        }
        if (!domainToSave) {
          alert("❌ 配置验证失败\n\n请输入自定义域名");
          updateLoadingState({ domainConfig: false });
          return;
        }
        if (!domainToSave.includes("://")) {
          alert("❌ 配置验证失败\n\n请输入完整的域名（包含 http:// 或 https://）\n\n例如: https://your-domain.com");
          updateLoadingState({ domainConfig: false });
          return;
        }
      }

      // 构建 tunnelMode 对象
      const tunnelModeToSave = appState.tunnelMode === "token"
        ? { Token: { token: tokenToSave, domain: domainToSave } }
        : "Temporary";

      await invoke("apply_tunnel_config", {
        tunnelMode: tunnelModeToSave,
        customDomain: domainToSave || null,
        tunnelToken: tokenToSave || null,
      });

      alert("✅ 配置保存成功\n\n隧道配置已保存并应用");
    } catch (err) {
      handleError(err, "保存隧道配置失败");
    } finally {
      updateLoadingState({ domainConfig: false });
    }
  }, [loading.domainConfig, appState.tunnelDomain, appState.tunnelMode, appState.tunnelToken, updateLoadingState, handleError]);



  const toggleNodeUnblock = useCallback(async (enabled: boolean) => {
    try {
      updateAppState({ nodeUnblockEnabled: enabled });
      updateLoadingState({ nodeUnblock: true });

      await invoke("set_nodes_unlocked", { enabled });
    } catch (err) {
      updateAppState({ nodeUnblockEnabled: !enabled });
      handleError(err, "设置节点解禁状态失败");
    } finally {
      updateLoadingState({ nodeUnblock: false });
    }
  }, [updateAppState, updateLoadingState, handleError]);

  const handleToggleSidebar = useCallback(async () => {
    onToggleSidebar?.();

    try {
      await invoke("toggle_sidebar");
    } catch (err) {
      console.error("Failed to toggle sidebar via backend:", err);
    }
  }, [onToggleSidebar]);

  // ========== 事件处理函数 ==========
  const handleTunnelUpdate = useCallback((event: Event<TunnelEventPayload>) => {
    const { status, url } = event.payload;
    const tunnelStatus = status.toLowerCase() as TunnelStatus;

    const updates: Partial<AppState> = {
      tunnelStatus,
      tunnelUrl: url || appState.tunnelUrl,
    };

    // 根据隧道状态更新隧道状态机
    if (tunnelStatus === "online") {
      updates.tunnelState = "RUNNING";
    } else if (tunnelStatus === "offline" || tunnelStatus === "error") {
      // 只有当当前状态是 STARTING 或 RUNNING 时才切换回 READY
      if (appState.tunnelState === "STARTING" || appState.tunnelState === "RUNNING") {
        updates.tunnelState = "READY";
      }
    }

    if (tunnelStatus === "online" || tunnelStatus === "offline" || tunnelStatus === "error") {
      updateLoadingState({ tunnel: false });
    }

    updateAppState(updates);
  }, [appState.tunnelUrl, appState.tunnelState, updateAppState, updateLoadingState]);

  // ========== 自动同步 Hook ==========
  // 监听 app://sync-state 事件，自动刷新 n8n 状态和强制刷新 iframe/WebView
  useAutoSync(() => {
    console.log('[SidebarPanel] 收到同步事件，刷新 n8n 状态');

    // 1. 重新获取 n8n 服务状态
    checkN8nStatus();

    // 2. 强制刷新关联的 iframe 或 WebView 页面
    // 通过修改 URL 添加随机查询参数来绕过缓存
    const n8nUrl = N8N_LOCAL_ADDRESS;
    const timestampedUrl = generateTimestampedUrl(n8nUrl);
    console.log('[SidebarPanel] 生成带时间戳的 URL:', timestampedUrl);

    // 在实际应用中，这里可以更新 iframe 的 src 或触发页面刷新
    // 例如：iframeRef.current.src = timestampedUrl;
    // 由于这是一个示例，我们只记录日志
  });

  // ========== 副作用 ==========
  useEffect(() => {
    let unlistenTunnelUpdate: UnlistenFn | null = null;

    const setupListeners = async () => {
      try {
        unlistenTunnelUpdate = await listen<TunnelEventPayload>("tunnel-event", handleTunnelUpdate);

        await loadAppInfo();
        await checkN8nStatus();

        try {
          const unlocked = await invoke<boolean>("get_nodes_unlocked");
          updateAppState({ nodeUnblockEnabled: unlocked });
        } catch (err) {
          console.error("Failed to load node unblock status:", err);
        }
      } catch (err) {
        console.error("Failed to setup sidebar listeners:", err);
      }
    };

    setupListeners();

    return () => {
      unlistenTunnelUpdate?.();
    };
  }, [loadAppInfo, checkN8nStatus, handleTunnelUpdate, updateAppState]);

  // ========== 渲染函数 ==========
  const renderCollapsedSidebar = () => (
    <div className={`sidebar-panel collapsed ${className}`}>
      <div className="sidebar-header-collapsed">
        <button
          onClick={handleToggleSidebar}
          className="sidebar-toggle-btn-modern"
          title="展开侧边栏"
        >
          <svg className="panel-left-open-icon" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect>
            <line x1="9" y1="3" x2="9" y2="21"></line>
          </svg>
        </button>
      </div>
    </div>
  );

  const renderWarningBox = () => (
    <div className="mt-3 p-3 rounded-lg border border-gray-200 bg-gray-50 flex items-start gap-2 shadow-sm">
      <div className="flex flex-col gap-1">

        <p className="text-xs leading-relaxed m-0 text-red-600">
          {t("ui.risk_warning")}
        </p>
      </div>
    </div>
  );

  // ========== 隧道状态渲染函数 ==========
  const renderReadyState = () => {
    const { text, color } = getTunnelStatusDisplay(appState.tunnelStatus);
    const isTunnelActive = appState.tunnelStatus === "online" || appState.tunnelStatus === "connecting";

    return (
      <div className="service-card">
        <div className="service-header">
          <div className="service-info">
            <h4 className="service-name">{t("ui.tunnel")}</h4>
            <span className={`service-status ${color}`}>
              {text}
            </span>
          </div>
          <div className="service-switch">
            <label className="switch">
              <input
                type="checkbox"
                checked={isTunnelActive}
                onChange={(e) => e.target.checked ? startTunnel() : stopTunnel()}
                disabled={loading.tunnel}
              />
              <span className="slider"></span>
            </label>
          </div>
        </div>

        <div className="tunnel-wizard-step">
          <div className="wizard-step-header">
            <div className="step-number">2</div>
            <div className="step-title">{t("ui.configure_tunnel")}</div>
          </div>
          <div className="wizard-step-content">

            {renderCustomDomainSection()}

            <div className="wizard-actions">
              <button
                onClick={startTunnel}
                disabled={loading.tunnel || !isTunnelConfigValid()}
                className="wizard-primary-btn"
                title={!isTunnelConfigValid() && appState.tunnelMode === "token" ? t("messages.config_validation_failed") : ""}
              >
                {loading.tunnel ? t("ui.starting") : t("ui.start_tunnel_with_one_click")}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  };

  const renderStartingState = () => (
    <div className="service-card">
      <div className="service-header">
        <div className="service-info">
          <h4 className="service-name">{t("ui.tunnel")}</h4>
          <span className="service-status text-yellow-600">
            {t("ui.starting")}
          </span>
        </div>
      </div>

      <div className="tunnel-wizard-step">
        <div className="wizard-step-header">
          <div className="step-number">3</div>
          <div className="step-title">{t("ui.starting")}</div>
        </div>
        <div className="wizard-step-content">
          <div className="loading-indicator">
            <div className="spinner-large"></div>
            <p className="loading-text">{t("ui.creating_tunnel_configuring_dns")}</p>
            <p className="loading-subtext">{t("ui.may_take_few_seconds")}</p>
          </div>
        </div>
      </div>
    </div>
  );

  const renderRunningState = () => {
    const { text, color } = getTunnelStatusDisplay(appState.tunnelStatus);

    return (
      <div className="service-card">
        <div className="service-header">
          <div className="service-info">
            <h4 className="service-name">{t("ui.tunnel")}</h4>
            <span className={`service-status ${color}`}>
              {text}
            </span>
          </div>
          <div className="service-switch">
            <label className="switch">
              <input
                type="checkbox"
                checked={true}
                onChange={() => stopTunnel()}
                disabled={loading.tunnel}
              />
              <span className="slider"></span>
            </label>
          </div>
        </div>

        <div className="tunnel-wizard-step">
          <div className="wizard-step-header">
            <div className="step-number">4</div>
            <div className="step-title">{t("ui.tunnel_running")}</div>
          </div>
          <div className="wizard-step-content">
            {appState.tunnelStatus === "online" && appState.tunnelUrl && (
              <div className="tunnel-url-section">
                <div className="tunnel-url-label">{t("ui.public_address")}</div>
                <div className="tunnel-url-value">
                  <a
                    href={appState.tunnelUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="tunnel-link"
                  >
                    {appState.tunnelUrl.replace('https://', '')}
                  </a>
                </div>
                {/* 刷新按钮已移除，由自动同步机制处理 */}
              </div>
            )}

            {cloudflaredInfo && (
              <div className="cloudflared-info">
                <span className="info-label">{t("ui.cloudflared")}</span>
                <span className={`info-value ${!cloudflaredInfo.installed ? 'not-installed' : ''}`}>
                  {cloudflaredInfo.installed
                    ? `${t("ui.installed")} ${cloudflaredInfo.version || t("ui.disabled")}`
                    : t("ui.not_installed_click_to_download")}
                </span>
              </div>
            )}
          </div>
        </div>
      </div>
    );
  };

  const renderServiceStatusCard = () => {
    const { text: n8nText, color: n8nColor } = getN8nStatusDisplay(appState.n8nStatus);

    return (
      <div className="service-card">

        <div className="service-details">
          {/* n8n 服务状态 */}
          <div className="service-item">
            <div className="service-item-header">
              <span className="item-label">{t("ui.n8n_service")}</span>
              <span className={`service-status ${n8nColor}`}>
                {n8nText}
              </span>
            </div>
            <div className="service-address">
              <span className="address-label">{t("ui.local_address")}</span>
              <span className="address-value">{N8N_LOCAL_ADDRESS}</span>
            </div>
          </div>

          {/* 分隔线 */}
          <div style={{ margin: '12px 0', borderTop: '1px solid #e5e7eb' }} />

          {/* 节点解禁 */}
          <div className="service-item">
            <div className="service-item-header">
              <span className="item-label">{t("ui.node_unblock")}</span>
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <span className="service-status text-yellow-600">
                  {appState.nodeUnblockEnabled ? t("ui.enabled") : t("ui.disabled")}
                </span>
                <label className="switch">
                  <input
                    type="checkbox"
                    checked={appState.nodeUnblockEnabled}
                    onChange={(e) => toggleNodeUnblock(e.target.checked)}
                    disabled={loading.nodeUnblock}
                  />
                  <span className="slider"></span>
                </label>
              </div>
            </div>
            {renderWarningBox()}
          </div>
        </div>
      </div>
    );
  };

  const renderTunnelCard = () => {
    // 根据隧道状态机渲染不同的UI
    switch (appState.tunnelState) {

      case "READY":
        return renderReadyState();
      case "STARTING":
        return renderStartingState();
      case "RUNNING":
        return renderRunningState();
      default:
        // 回退到原始UI
        const { text, color } = getTunnelStatusDisplay(appState.tunnelStatus);
        const isTunnelActive = appState.tunnelStatus === "online" || appState.tunnelStatus === "connecting";

        return (
          <div className="service-card">
            <div className="service-header">
              <div className="service-info">
                <h4 className="service-name">Cloudflare 隧道</h4>
                <span className={`service-status ${color}`}>
                  {text}
                </span>
              </div>
              <div className="service-switch">
                <label className="switch">
                  <input
                    type="checkbox"
                    checked={isTunnelActive}
                    onChange={(e) => e.target.checked ? startTunnel() : stopTunnel()}
                    disabled={loading.tunnel}
                  />
                  <span className="slider"></span>
                </label>
              </div>
            </div>

            {appState.tunnelStatus === "online" && appState.tunnelUrl && (
              <div className="tunnel-url-section">
                <div className="tunnel-url-label">公网地址:</div>
                <div className="tunnel-url-value">
                  <a
                    href={appState.tunnelUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="tunnel-link"
                  >
                    {appState.tunnelUrl.replace('https://', '')}
                  </a>
                </div>
                {/* 刷新按钮已移除，由自动同步机制处理 */}
              </div>
            )}

            {cloudflaredInfo && (
              <div className="cloudflared-info">
                <span className="info-label">cloudflared:</span>
                <span className={`info-value ${!cloudflaredInfo.installed ? 'not-installed' : ''}`}>
                  {cloudflaredInfo.installed
                    ? `已安装 ${cloudflaredInfo.version || "未知版本"}`
                    : "未安装 (点击隧道开关将自动下载)"}
                </span>
              </div>
            )}

            {renderCustomDomainSection()}
          </div>
        );
    }
  };

  const renderCustomDomainSection = () => (
    <div className="custom-domain-section mt-4 pt-4 border-t border-gray-200">

      {/* 隧道模式选择 */}
      <div className="tunnel-mode-selector mb-4">
        <div className="flex gap-2">
          {(["temporary", "token"] as const).map((mode) => (
            <button
              key={mode}
              onClick={() => updateAppState({ tunnelMode: mode })}
              style={{
                flex: 1,
                padding: "8px 12px",
                fontSize: "12px",
                border: "1px solid #cbd5e1",
                borderRadius: "4px",
                backgroundColor: appState.tunnelMode === mode ? "#3b82f6" : "#f1f5f9",
                color: appState.tunnelMode === mode ? "white" : "#334155",
                cursor: "pointer",
                fontWeight: "500",
                transition: "all 0.2s",
              }}
            >
              {mode === "temporary" && t("ui.random_temporary_domain")}
              {mode === "token" && t("ui.fixed_custom_domain")}
            </button>
          ))}
        </div>
        <p className="text-xs text-gray-500 mt-2">
          {appState.tunnelMode === "temporary" && t("ui.random_temporary_domain_desc")}
          {appState.tunnelMode === "token" && t("ui.fixed_custom_domain_desc")}
        </p>
      </div>

      {/* Token 模式输入区域 */}
      {appState.tunnelMode === "token" && (
        <>
          {/* Domain 输入 */}
          <div className="mb-3">
            <label className="block text-sm font-medium text-gray-700 mb-1">
              {t("ui.custom_domain")}
            </label>

            <input
              type="text"
              value={appState.tunnelDomain}
              onChange={(e) => updateAppState({ tunnelDomain: e.target.value })}
              placeholder="https://your-domain.com"
              className="w-full px-3 py-2 border border-gray-300 rounded-md text-xs focus:outline-none focus:ring-2 focus:ring-blue-500 "
              disabled={loading.domainConfig}
            />


          </div>

          {/* Token 输入 */}
          <div className="mb-3">
            <label className="block text-sm font-medium text-gray-700 mb-2">
              {t("ui.tunnel_token")}
            </label>
            <textarea
              value={appState.tunnelToken}
              onChange={(e) => updateAppState({ tunnelToken: e.target.value })}
              placeholder={t("ui.paste_tunnel_token")}
              className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono"
              rows={3}
              disabled={loading.domainConfig}
            />

          </div>

          {/* 保存按钮 */}
          <div>
            <button
              onClick={saveCustomDomainConfig}
              disabled={loading.domainConfig}
              style={{
                width: "100%",
                padding: "8px 12px",
                backgroundColor: loading.domainConfig ? "#cbd5e1" : "#3b82f6",
                color: "white",
                border: "none",
                borderRadius: "4px",
                cursor: loading.domainConfig ? "not-allowed" : "pointer",
                fontSize: "12px",
                fontWeight: "500",
                transition: "background-color 0.2s",
              }}
            >
              {loading.domainConfig ? t("ui.saving") : t("ui.save_config")}
            </button>
          </div>
        </>
      )}
    </div>
  );


  const renderFooter = () => (
    <div className="sidebar-footer-minimal">
      <div className="footer-status">
        <div className={`status-dot ${appState.n8nStatus === "running" ? "running" : "stopped"}`} />
        <span className="status-text">n8n {getN8nStatusDisplay(appState.n8nStatus).text}</span>
      </div>
    </div>
  );

  // ========== 主渲染逻辑 ==========
  if (collapsed) {
    return renderCollapsedSidebar();
  }

  return (
    <div className={`sidebar-panel ${className}`}>
      {/* 顶部标题栏 */}
      <div className="sidebar-header">
        <div className="sidebar-title">
          <h2 className="text-lg font-bold">{t("app.title")}</h2>
          <span className="text-xs text-gray-500">v{appVersion}</span>
        </div>
        <button
          onClick={handleToggleSidebar}
          className="sidebar-toggle-btn-modern"
          title="折叠侧边栏"
        >
          <svg className="panel-left-close-icon" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect>
            <line x1="9" y1="3" x2="9" y2="21"></line>
          </svg>
        </button>
      </div>

      {/* 主内容区域 - 垂直单页布局 */}
      <div className="sidebar-content-single">
        {/* 服务控制区域 */}
        <div className="service-control-section">
          <h3 className="section-title">{t("ui.service_configuration")}</h3>
          {renderServiceStatusCard()}
          {renderTunnelCard()}
        </div>

        {/* 应用设置 & 关于区域 */}
        {/* {renderAppInfoSection()} */}
      </div>

      {/* 底部状态栏 */}
      {renderFooter()}
    </div>
  );
}
