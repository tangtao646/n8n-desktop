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

// 隧道状态机
type TunnelState = "UNAUTHORIZED" | "READY" | "STARTING" | "RUNNING";

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
  tunnel_mode?: "temporary" | "token";
  tunnel_token?: string;
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
  tunnelDomain: string; // Domain for Token mode
  tunnelMode: "temporary" | "token"; // 隧道模式
  tunnelToken: string; // Cloudflare Tunnel Token
  tunnelState: TunnelState; // 隧道状态机
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
    tunnelDomain: "",
    tunnelMode: "temporary",
    tunnelToken: "",
    tunnelState: "READY", // 初始状态为就绪（无需授权）
  });

  const [loading, setLoading] = useState<LoadingState>({
    tunnel: false,
    update: false,
    nodeUnblock: false,
    domainConfig: false,
  });

  const [authPollingInterval, setAuthPollingInterval] = useState<NodeJS.Timeout | null>(null);

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

  // 关联 Cloudflare 账号
  const associateCloudflareAccount = useCallback(async () => {
    console.log("[SidebarPanel] 用户点击关联 Cloudflare 账号");
    // 打开 Cloudflare 登录页面
    window.open("https://dash.cloudflare.com/profile/api-tokens", "_blank");
    
    // 启动轮询检查授权状态
    const interval = setInterval(async () => {
      try {
        const isAuthorized = await invoke<boolean>("check_auth_status");
        console.log("[SidebarPanel] 轮询检查授权状态:", isAuthorized);
        if (isAuthorized) {
          // 停止轮询
          clearInterval(interval);
          setAuthPollingInterval(null);
          // 更新状态为 READY
          updateAppState({ tunnelState: "READY" });
          console.log("[SidebarPanel] 授权成功，状态切换为 READY");
        }
      } catch (err) {
        console.error("[SidebarPanel] 轮询检查授权状态失败:", err);
      }
    }, 2000); // 2秒一次
    
    setAuthPollingInterval(interval);
  }, [updateAppState]);

  // 清理轮询定时器
  useEffect(() => {
    return () => {
      if (authPollingInterval) {
        clearInterval(authPollingInterval);
      }
    };
  }, [authPollingInterval]);

  // ========== 核心逻辑函数 ==========
  const loadAppInfo = useCallback(async () => {
    try {
      const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
      setCloudflaredInfo(versionInfo);

      // 加载隧道配置
      try {
        const config = await invoke<TunnelConfig>("get_tunnel_config");
        if (config.custom_domain) {
          updateAppState({ tunnelDomain: config.custom_domain });
        }
        if (config.tunnel_mode) {
          updateAppState({ tunnelMode: config.tunnel_mode });
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

    updateLoadingState({ tunnel: true });
    updateAppState({
      tunnelStatus: "connecting",
      tunnelState: "STARTING" // 切换到启动中状态
    });

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
          updateAppState({
            tunnelStatus: "offline",
            tunnelState: "READY" // 超时后回到就绪状态
          });
        }
      }, TUNNEL_START_TIMEOUT_MS);
    } catch (err) {
      updateAppState({
        tunnelStatus: "error",
        tunnelState: "READY" // 错误后回到就绪状态
      });
      updateLoadingState({ tunnel: false });
      
      handleError(err, "启动隧道失败");
    }
  }, [loading.tunnel, appState.tunnelStatus, updateLoadingState, updateAppState, handleError]);

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

      await invoke("apply_tunnel_config", {
        tunnelMode: appState.tunnelMode,
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

  const checkForUpdates = useCallback(async () => {
    if (loading.update) return;

    updateLoadingState({ update: true });

    try {
      await new Promise(resolve => setTimeout(resolve, UPDATE_CHECK_DELAY_MS));
      alert("✅ 检查更新完成\n\n当前已是最新版本");
    } catch (err) {
      handleError(err, "检查更新失败");
    } finally {
      updateLoadingState({ update: false });
    }
  }, [loading.update, updateLoadingState, handleError]);

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

  // ========== 隧道状态渲染函数 ==========
  const renderUnauthorizedState = () => (
    <div className="service-card">
      <div className="service-header">
        <div className="service-info">
          <h4 className="service-name">Cloudflare 隧道</h4>
          <span className="service-status text-red-600">
            未授权
          </span>
        </div>
      </div>

      <div className="tunnel-wizard-step">
        <div className="wizard-step-header">
          <div className="step-number">1</div>
          <div className="step-title">关联 Cloudflare 账号</div>
        </div>
        <div className="wizard-step-content">
          <p className="step-description">
            使用 Cloudflare 账号授权，以便创建和管理隧道。点击下方按钮前往 Cloudflare 仪表盘生成 API Token。
          </p>
          <button
            onClick={associateCloudflareAccount}
            className="wizard-primary-btn"
            disabled={!!authPollingInterval}
          >
            {authPollingInterval ? "等待授权中..." : "关联 Cloudflare 账号"}
          </button>
          {authPollingInterval && (
            <div className="auth-polling-info">
              <div className="spinner-small"></div>
              <span className="polling-text">正在检查授权状态...</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );

  const renderReadyState = () => {
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

        <div className="tunnel-wizard-step">
          <div className="wizard-step-header">
            <div className="step-number">2</div>
            <div className="step-title">配置隧道</div>
          </div>
          <div className="wizard-step-content">
            <p className="step-description">
              选择隧道模式并配置相关参数，然后点击"一键开启"启动隧道。
            </p>
            
            {renderCustomDomainSection()}
            
            <div className="wizard-actions">
              <button
                onClick={startTunnel}
                disabled={loading.tunnel}
                className="wizard-primary-btn"
              >
                {loading.tunnel ? "启动中..." : "一键开启隧道"}
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
          <h4 className="service-name">Cloudflare 隧道</h4>
          <span className="service-status text-yellow-600">
            启动中
          </span>
        </div>
      </div>

      <div className="tunnel-wizard-step">
        <div className="wizard-step-header">
          <div className="step-number">3</div>
          <div className="step-title">正在启动隧道</div>
        </div>
        <div className="wizard-step-content">
          <div className="loading-indicator">
            <div className="spinner-large"></div>
            <p className="loading-text">正在创建隧道并配置 DNS...</p>
            <p className="loading-subtext">这可能需要几秒钟时间</p>
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
            <h4 className="service-name">Cloudflare 隧道</h4>
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
            <div className="step-title">隧道运行中</div>
          </div>
          <div className="wizard-step-content">
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
    // 根据隧道状态机渲染不同的UI
    switch (appState.tunnelState) {
      case "UNAUTHORIZED":
        return renderUnauthorizedState();
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
    }
  };

  const renderCustomDomainSection = () => (
    <div className="custom-domain-section mt-4 pt-4 border-t border-gray-200">
      <h4 className="service-name text-sm mb-3">隧道配置</h4>

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
              {mode === "temporary" && "随机临时域名"}
              {mode === "token" && "固定自定义域名"}
            </button>
          ))}
        </div>
        <p className="text-xs text-gray-500 mt-2">
          {appState.tunnelMode === "temporary" && "每次启动随机生成临时公网地址"}
          {appState.tunnelMode === "token" && "使用 Cloudflare Tunnel Token 建立固定隧道"}
        </p>
      </div>

      {/* Token 模式输入区域 */}
      {appState.tunnelMode === "token" && (
        <>
          {/* Domain 输入 */}
          <div className="mb-3">
            <label className="block text-sm font-medium text-gray-700 mb-1">
              自定义域名
            </label>
            <input
              type="text"
              value={appState.tunnelDomain}
              onChange={(e) => updateAppState({ tunnelDomain: e.target.value })}
              placeholder="https://your-domain.com"
              className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              disabled={loading.domainConfig}
            />
            <p className="text-xs text-gray-500 mt-1">
              在 Cloudflare DNS 中配置 CNAME 记录指向你的隧道
            </p>
          </div>

          {/* Token 输入 */}
          <div className="mb-3">
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Tunnel Token
            </label>
            <textarea
              value={appState.tunnelToken}
              onChange={(e) => updateAppState({ tunnelToken: e.target.value })}
              placeholder="粘贴您的 Cloudflare Tunnel Token..."
              className="w-full px-3 py-2 border border-gray-300 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono"
              rows={3}
              disabled={loading.domainConfig}
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
              {loading.domainConfig ? "保存中..." : "保存配置"}
            </button>
          </div>
        </>
      )}
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
