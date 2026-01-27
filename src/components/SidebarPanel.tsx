import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn, Event } from "@tauri-apps/api/event";

// ========== 常量定义 ==========
const CLOUDFLARED_DEFAULT_PATH = "cloudflared";
const TUNNEL_START_TIMEOUT_MS = 60000;
const TUNNEL_STOP_TIMEOUT_MS = 5000;
const UPDATE_CHECK_DELAY_MS = 1500;
const N8N_LOCAL_ADDRESS = "http://localhost:5678";
const DEFAULT_APP_VERSION = "1.0.2";

// ========== 类型定义 ==========
type TunnelStatus = "offline" | "connecting" | "online" | "error";
type N8nStatus = "running" | "stopped" | "starting";

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
  [key: string]: unknown;
}

interface SidebarPanelProps {
  collapsed?: boolean;
  onToggleSidebar?: () => void;
  onTunnelOnline?: () => void;
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
  customDomain: string;
  useCustomDomain: boolean;
};

// ========== 状态映射 ==========
const N8N_STATUS_MAP: Record<N8nStatus, { text: string; color: string }> = {
  running: { text: "运行中", color: "text-green-600" },
  stopped: { text: "已停止", color: "text-red-600" },
  starting: { text: "启动中", color: "text-yellow-600" },
};

const TUNNEL_STATUS_MAP: Record<TunnelStatus, { text: string; color: string }> = {
  offline: { text: "隧道已关闭", color: "text-gray-600" },
  connecting: { text: "隧道连接中...", color: "text-yellow-600" },
  online: { text: "隧道已连接", color: "text-green-600" },
  error: { text: "隧道错误", color: "text-red-600" },
};

// ========== 主组件 ==========
export default function SidebarPanel({ collapsed = false, onToggleSidebar, onTunnelOnline, className = "" }: SidebarPanelProps) {
  // ========== 状态定义 ==========
  const [appState, setAppState] = useState<AppState>({
    tunnelStatus: "offline",
    tunnelUrl: "",
    n8nStatus: "running",
    nodeUnblockEnabled: false,
    customDomain: "",
    useCustomDomain: false,
  });

  const [loading, setLoading] = useState<LoadingState>({
    tunnel: false,
    update: false,
    nodeUnblock: false,
    domainConfig: false,
  });

  const [cloudflaredInfo, setCloudflaredInfo] = useState<CloudflaredVersionInfo | null>(null);
  const [appVersion] = useState<string>(DEFAULT_APP_VERSION);

  // ========== 工具函数 ==========
  const getN8nStatusDisplay = useCallback((status: N8nStatus) => {
    return N8N_STATUS_MAP[status] || { text: "未知", color: "text-gray-600" };
  }, []);

  const getTunnelStatusDisplay = useCallback((status: TunnelStatus) => {
    return TUNNEL_STATUS_MAP[status] || { text: "未知状态", color: "text-gray-600" };
  }, []);

  const updateLoadingState = useCallback((updates: Partial<LoadingState>) => {
    setLoading(prev => ({ ...prev, ...updates }));
  }, []);

  const updateAppState = useCallback((updates: Partial<AppState>) => {
    setAppState(prev => ({ ...prev, ...updates }));
  }, []);

  // ========== 核心逻辑函数 ==========
  const loadAppInfo = useCallback(async () => {
    try {
      const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
      setCloudflaredInfo(versionInfo);

      // 加载隧道配置
      try {
        const config = await invoke<TunnelConfig>("get_tunnel_config");
        if (config.custom_domain) {
          updateAppState({ customDomain: config.custom_domain });
        }
        if (config.use_custom_domain !== undefined) {
          updateAppState({ useCustomDomain: config.use_custom_domain });
        }
      } catch (err) {
        console.error("Failed to load tunnel config:", err);
      }
    } catch (err) {
      console.error("Failed to load app info:", err);
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

    updateLoadingState({ tunnel: true });
    updateAppState({ tunnelStatus: "connecting" });

    try {
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
          updateAppState({ tunnelStatus: "offline" });
        }
      }, TUNNEL_START_TIMEOUT_MS);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      console.error("Failed to start tunnel:", err);
      updateAppState({ tunnelStatus: "error" });
      updateLoadingState({ tunnel: false });

      alert(`启动隧道失败: ${errorMessage}\n\n请确保:\n1. 网络连接正常\n2. 可以访问 GitHub\n3. 或者手动安装 cloudflared`);
    }
  }, [loading.tunnel, appState.tunnelStatus, updateLoadingState, updateAppState]);

  const stopTunnel = useCallback(async () => {
    if (loading.tunnel) return;

    updateLoadingState({ tunnel: true });

    try {
      await invoke("stop_tunnel");

      setTimeout(() => {
        updateLoadingState({ tunnel: false });
      }, TUNNEL_STOP_TIMEOUT_MS);
    } catch (err) {
      console.error("Failed to stop tunnel:", err);
      updateLoadingState({ tunnel: false });
    }
  }, [loading.tunnel, updateLoadingState]);

  const saveCustomDomainConfig = useCallback(async () => {
    if (loading.domainConfig) return;

    updateLoadingState({ domainConfig: true });

    try {
      const domainToSave = appState.customDomain.trim();
      if (appState.useCustomDomain && domainToSave && !domainToSave.includes("://")) {
        alert("请输入完整的域名（包含 http:// 或 https://）");
        updateLoadingState({ domainConfig: false });
        return;
      }

      await invoke("apply_custom_domain_config", {
        customDomain: domainToSave || null,
        useCustomDomain: appState.useCustomDomain,
      });

      alert("域名配置已保存并应用");
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      console.error("Failed to save custom domain config:", err);
      alert(`保存域名配置失败: ${errorMessage}`);
    } finally {
      updateLoadingState({ domainConfig: false });
    }
  }, [loading.domainConfig, appState.customDomain, appState.useCustomDomain, updateLoadingState]);

  const checkForUpdates = useCallback(async () => {
    if (loading.update) return;

    updateLoadingState({ update: true });

    try {
      await new Promise(resolve => setTimeout(resolve, UPDATE_CHECK_DELAY_MS));
      alert("当前已是最新版本");
    } catch (err) {
      console.error("Failed to check updates:", err);
      alert("检查更新失败，请检查网络连接");
    } finally {
      updateLoadingState({ update: false });
    }
  }, [loading.update, updateLoadingState]);

  const toggleNodeUnblock = useCallback(async (enabled: boolean) => {
    try {
      updateAppState({ nodeUnblockEnabled: enabled });
      updateLoadingState({ nodeUnblock: true });

      await invoke("set_nodes_unlocked", { enabled });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      console.error("Failed to set node unblock:", err);
      updateAppState({ nodeUnblockEnabled: !enabled });
      alert(`设置节点解禁状态失败: ${errorMessage}`);
    } finally {
      updateLoadingState({ nodeUnblock: false });
    }
  }, [updateAppState, updateLoadingState]);

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

    if (tunnelStatus === "online" || tunnelStatus === "offline" || tunnelStatus === "error") {
      updateLoadingState({ tunnel: false });
    }

    updateAppState(updates);
  }, [appState.tunnelUrl, updateAppState, updateLoadingState]);

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
      <span className="text-lg mt-[-1px] text-red-600">⚠️</span>
      <div className="flex flex-col gap-1">
        <strong className="text-sm font-bold text-gray-900">
          风险提示
        </strong>
        <p className="text-xs leading-relaxed m-0 text-red-600">
          解除禁用节点可能存在风险。启用此功能可能会允许执行潜在不安全的节点操作（如执行系统命令等），请谨慎使用。
        </p>
      </div>
    </div>
  );

  const renderServiceStatusCard = () => {
    const { text: n8nText, color: n8nColor } = getN8nStatusDisplay(appState.n8nStatus);

    return (
      <div className="service-card">

        <div className="service-details">
          {/* n8n 服务状态 */}
          <div className="service-item">
            <div className="service-item-header">
              <span className="item-label">n8n 服务</span>
              <span className={`service-status ${n8nColor}`}>
                {n8nText}
              </span>
            </div>
            <div className="service-address">
              <span className="address-label">本地地址:</span>
              <span className="address-value">{N8N_LOCAL_ADDRESS}</span>
            </div>
          </div>

          {/* 分隔线 */}
          <div style={{ margin: '12px 0', borderTop: '1px solid #e5e7eb' }} />

          {/* 节点解禁 */}
          <div className="service-item">
            <div className="service-item-header">
              <span className="item-label">节点解禁</span>
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <span className="service-status text-yellow-600">
                  {appState.nodeUnblockEnabled ? "已启用" : "已禁用"}
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
            <button
              onClick={() => {
                console.log("[SidebarPanel] 用户点击刷新 n8n UI");
                onTunnelOnline?.();
              }}
              style={{
                marginTop: '8px',
                padding: '6px 12px',
                backgroundColor: '#4CAF50',
                color: 'white',
                border: 'none',
                borderRadius: '4px',
                cursor: 'pointer',
                fontSize: '12px',
                width: '100%',
                fontFamily: 'inherit'
              }}
            >
              刷新 n8n UI
            </button>
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
  };

  const renderCustomDomainSection = () => (
    <div className="custom-domain-section mt-4 pt-4 border-t border-gray-200">
      <div className="service-header">
        <div className="service-info">
          <h4 className="service-name text-sm">自定义域名</h4>
          <span className="service-status text-blue-600 text-sm">
            {appState.useCustomDomain ? "已启用" : "已禁用"}
          </span>
        </div>
        <div className="service-switch">
          <label className="switch">
            <input
              type="checkbox"
              checked={appState.useCustomDomain}
              onChange={(e) => updateAppState({ useCustomDomain: e.target.checked })}
              disabled={loading.domainConfig}
            />
            <span className="slider"></span>
          </label>
        </div>
      </div>

      {appState.useCustomDomain && (
        <div className="mt-3">
          <div className="mb-2">
            <label className="block text-sm font-medium text-gray-700 mb-1">
              自定义域名
            </label>
            <input
              type="text"
              value={appState.customDomain}
              onChange={(e) => updateAppState({ customDomain: e.target.value })}
              placeholder="https://your-domain.com"
              className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              disabled={loading.domainConfig}
            />
            <p className="text-xs text-gray-500 mt-1">
              请输入完整的域名，包含 http:// 或 https://
            </p>
          </div>
        </div>
      )}

      <div className="mt-4">
        <button
          onClick={saveCustomDomainConfig}
          disabled={loading.domainConfig || (appState.useCustomDomain && !appState.customDomain.trim())}
          className="w-full px-4 py-2 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {loading.domainConfig ? (
            <>
              <svg className="spinner inline mr-2" width="16" height="16" viewBox="0 0 24 24">
                <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
              </svg>
              保存域名配置
            </>
          ) : "保存域名配置"}
        </button>
        <p className="text-xs text-gray-500 mt-2">
          保存后会自动重启 n8n 以应用新的域名配置
        </p>
      </div>
    </div>
  );

  const renderAppInfoSection = () => (
    <div className="app-info-section">
      <h3 className="section-title">应用信息</h3>

      <div className="app-info-card">
        <div className="version-info">
          <div className="version-label">当前版本</div>
          <div className="version-value">v{appVersion}</div>
        </div>

        <button
          onClick={checkForUpdates}
          disabled={loading.update}
          className="check-update-btn"
        >
          {loading.update ? (
            <>
              <svg className="spinner" width="16" height="16" viewBox="0 0 24 24">
                <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
              </svg>
              检查中...
            </>
          ) : "检查更新"}
        </button>
      </div>

      <div className="app-links">
        <a href="#" className="app-link" onClick={(e) => { e.preventDefault(); alert("文档功能开发中"); }}>
          查看文档
        </a>
        <a href="#" className="app-link" onClick={(e) => { e.preventDefault(); alert("问题反馈功能开发中"); }}>
          报告问题
        </a>
        <a href="#" className="app-link" onClick={(e) => { e.preventDefault(); alert("日志目录功能开发中"); }}>
          打开日志目录
        </a>
      </div>
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
          <h2 className="text-lg font-bold">n8n Desktop</h2>
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
          <h3 className="section-title">服务配置</h3>
          {renderServiceStatusCard()}
          {renderTunnelCard()}
        </div>

        {/* 应用设置 & 关于区域 */}
        {renderAppInfoSection()}
      </div>

      {/* 底部状态栏 */}
      {renderFooter()}
    </div>
  );
}
