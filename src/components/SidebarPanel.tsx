import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

interface SidebarPanelProps {
  collapsed?: boolean;
  onToggleSidebar?: () => void;
  className?: string;
}

type TunnelStatus = "offline" | "connecting" | "online" | "error";

interface CloudflaredVersionInfo {
  installed: boolean;
  version?: string;
  path?: string;
  cached: boolean;
  cache_age_days?: number;
}

export default function SidebarPanel({ collapsed = false, onToggleSidebar, className = "" }: SidebarPanelProps) {
  const [tunnelStatus, setTunnelStatus] = useState<TunnelStatus>("offline");
  const [tunnelUrl, setTunnelUrl] = useState<string>("");
  const [cloudflaredInfo, setCloudflaredInfo] = useState<CloudflaredVersionInfo | null>(null);
  const [appVersion, setAppVersion] = useState<string>("1.0.0");
  const [n8nStatus, setN8nStatus] = useState<"running" | "stopped" | "starting">("running");
  const [isTunnelLoading, setIsTunnelLoading] = useState(false);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [nodeUnblockEnabled, setNodeUnblockEnabled] = useState(false);
  const [isNodeUnblockLoading, setIsNodeUnblockLoading] = useState(false);

  // 加载应用信息
  const loadAppInfo = async () => {
    try {
      // 获取 cloudflared 信息
      const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
      setCloudflaredInfo(versionInfo);

      // 获取应用版本（这里可以调用后端命令获取）
      // 暂时使用固定值
      setAppVersion("1.0.2");
    } catch (err) {
      console.error("Failed to load app info:", err);
    }
  };

  // 检查 n8n 状态
  const checkN8nStatus = async () => {
    try {
      // 这里可以调用后端命令检查 n8n 状态
      // 暂时假设 n8n 在运行
      setN8nStatus("running");
    } catch (err) {
      console.error("Failed to check n8n status:", err);
      setN8nStatus("stopped");
    }
  };

  // 启动隧道
  const startTunnel = async () => {
    if (isTunnelLoading) return;

    setIsTunnelLoading(true);
    setTunnelStatus("connecting");

    try {
      // 首先检查 cloudflared 是否可用
      const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
      console.log("Cloudflared version info:", versionInfo);

      let cloudflaredPath = "cloudflared";
      if (versionInfo.installed && versionInfo.path) {
        cloudflaredPath = versionInfo.path;
        console.log(`Using existing cloudflared from: "${cloudflaredPath}"`);
      } else {
        // 如果没有安装 cloudflared，需要先下载
        console.log("cloudflared not found, starting automatic download...");

        try {
          // 下载 cloudflared
          await invoke("download_cloudflared");
          console.log("cloudflared download completed");

          // 重新检查版本
          const newVersionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
          console.log("After download, cloudflared info:", newVersionInfo);

          if (!newVersionInfo.installed) {
            throw new Error("下载 cloudflared 失败，请检查网络连接或手动安装 cloudflared");
          }

          if (newVersionInfo.path) {
            cloudflaredPath = newVersionInfo.path;
            console.log(`Using downloaded cloudflared from: "${cloudflaredPath}"`);
          }
        } catch (downloadErr: any) {
          console.error("Cloudflared download failed:", downloadErr);
          throw new Error(`cloudflared 下载失败: ${downloadErr.message || downloadErr}. 请手动安装 cloudflared 或检查网络连接。`);
        }
      }

      // 启动隧道
      console.log("Starting tunnel with cloudflared path:", cloudflaredPath);
      await invoke("start_tunnel", { cloudflaredPath: cloudflaredPath });
      console.log("Tunnel start command sent successfully");

      // 状态更新将通过事件监听器处理
      // 设置超时，防止无限加载
      setTimeout(() => {
        if (tunnelStatus === "connecting") {
          console.log("Tunnel start timeout after 30 seconds");
          setIsTunnelLoading(false);
          setTunnelStatus("offline");
        }
      }, 30000); // 30秒超时

    } catch (err: any) {
      console.error("Failed to start tunnel:", err);
      setTunnelStatus("error");
      setIsTunnelLoading(false);
      // 显示用户友好的错误信息
      alert(`启动隧道失败: ${err.message || err}\n\n请确保:\n1. 网络连接正常\n2. 可以访问 GitHub\n3. 或者手动安装 cloudflared`);
    }
  };

  // 停止隧道
  const stopTunnel = async () => {
    if (isTunnelLoading) return;

    setIsTunnelLoading(true);
    try {
      await invoke("stop_tunnel");
      // 状态更新将通过事件监听器处理
      // 设置超时，防止无限加载
      setTimeout(() => {
        setIsTunnelLoading(false);
      }, 5000); // 5秒超时
    } catch (err: any) {
      console.error("Failed to stop tunnel:", err);
      setIsTunnelLoading(false);
    }
  };

  // 检查更新
  const checkForUpdates = async () => {
    if (isCheckingUpdate) return;

    setIsCheckingUpdate(true);
    try {
      // 这里可以调用后端命令检查更新
      // 暂时模拟检查过程
      await new Promise(resolve => setTimeout(resolve, 1500));
      alert("当前已是最新版本");
    } catch (err) {
      console.error("Failed to check updates:", err);
      alert("检查更新失败，请检查网络连接");
    } finally {
      setIsCheckingUpdate(false);
    }
  };

  // 切换节点解禁开关
  const toggleNodeUnblock = async (enabled: boolean) => {
    try {

      // 乐观更新：立即更新UI
      setNodeUnblockEnabled(enabled);
      setIsNodeUnblockLoading(true);

      console.log(`[DEBUG] 调用后端命令 set_nodes_unlocked(${enabled})`);
      // 调用后端命令来实际启用/禁用节点解禁
      await invoke("set_nodes_unlocked", { enabled });
      console.log(`[DEBUG] 节点解禁已${enabled ? '启用' : '禁用'}`);
    } catch (err: any) {
      console.error("[DEBUG] Failed to set node unblock:", err);
      // 回滚状态
      setNodeUnblockEnabled(!enabled);
      alert(`设置节点解禁状态失败: ${err.message || err}`);
    } finally {
      console.log("[DEBUG] 清除加载状态");
      setIsNodeUnblockLoading(false);
    }
  };


  // 初始化
  useEffect(() => {
    let unlistenTunnelUpdate: UnlistenFn | null = null;

    const setupListeners = async () => {
      try {
        // 监听隧道状态更新
        unlistenTunnelUpdate = await listen("tunnel-update", (event: any) => {
          const { status, url } = event.payload;
          const tunnelStatus = status.toLowerCase() as TunnelStatus;
          setTunnelStatus(tunnelStatus);
          if (url) {
            setTunnelUrl(url);
          }

          // 更新加载状态
          if (tunnelStatus === "online" || tunnelStatus === "offline" || tunnelStatus === "error") {
            setIsTunnelLoading(false);
          }
        });

        // 加载应用信息
        await loadAppInfo();
        await checkN8nStatus();

        // 加载节点解禁状态
        try {
          const unlocked = await invoke<boolean>("get_nodes_unlocked");
          setNodeUnblockEnabled(unlocked);
        } catch (err) {
          console.error("Failed to load node unblock status:", err);
        }
      } catch (err) {
        console.error("Failed to setup sidebar listeners:", err);
      }
    };

    setupListeners();

    return () => {
      if (unlistenTunnelUpdate) unlistenTunnelUpdate();
    };
  }, []);

  // n8n 状态显示文本
  const getN8nStatusText = () => {
    switch (n8nStatus) {
      case "running": return "运行中";
      case "stopped": return "已停止";
      case "starting": return "启动中";
      default: return "未知";
    }
  };

  // n8n 状态颜色
  const getN8nStatusColor = () => {
    switch (n8nStatus) {
      case "running": return "text-green-600";
      case "stopped": return "text-red-600";
      case "starting": return "text-yellow-600";
      default: return "text-gray-600";
    }
  };

  // 隧道状态显示文本
  const getTunnelStatusText = () => {
    switch (tunnelStatus) {
      case "offline": return "隧道已关闭";
      case "connecting": return "隧道连接中...";
      case "online": return "隧道已连接";
      case "error": return "隧道错误";
      default: return "未知状态";
    }
  };

  // 隧道状态颜色
  const getTunnelStatusColor = () => {
    switch (tunnelStatus) {
      case "online": return "text-green-600";
      case "connecting": return "text-yellow-600";
      case "error": return "text-red-600";
      default: return "text-gray-600";
    }
  };

  // 处理侧边栏切换
  const handleToggleSidebar = async () => {
    if (onToggleSidebar) {
      onToggleSidebar();
    }
    // 调用后端命令同步布局
    try {
      await invoke("toggle_sidebar");
    } catch (err) {
      console.error("Failed to toggle sidebar via backend:", err);
    }
  };

  // 如果侧边栏折叠，只显示切换按钮
  if (collapsed) {
    return (
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
          <h3 className="section-title">服务控制</h3>

          {/* n8n 服务状态 */}
          <div className="service-card">
            <div className="service-header">
              <div className="service-info">
                <h4 className="service-name">n8n 服务</h4>
                <span className={`service-status ${getN8nStatusColor()}`}>
                  {getN8nStatusText()}
                </span>
              </div>

            </div>
            <div className="service-details">
              <div className="service-address">
                <span className="address-label">本地地址:</span>
                <span className="address-value">http://localhost:5678</span>
              </div>
            </div>
          </div>

          {/* 节点解禁控制 */}
          <div className="service-card">
            <div className="service-header">
              <div className="service-info">
                <h4 className="service-name">节点解禁</h4>
                <span className="service-status text-yellow-600">
                  {nodeUnblockEnabled ? "已启用" : "已禁用"}
                </span>
              </div>
              <div className="service-switch">
                <label className="switch">
                  <input
                    type="checkbox"
                    checked={nodeUnblockEnabled}
                    onChange={(e) => {

                      const newValue = e.target.checked; // 计算新值
                      toggleNodeUnblock(newValue);
                    }}
                    disabled={isNodeUnblockLoading}
                  />
                  <span className="slider"></span>
                </label>
              </div>
            </div>

            <div className="service-details">
              <div
                className="mt-3 p-3 rounded-lg border flex items-start gap-2"
                style={{
                  backgroundColor: '#fff7ed', // 极浅的橘黄色背景，警告感更好
                  borderColor: '#ffedd5',     // 浅橘色边框
                  boxShadow: '0 1px 2px 0 rgba(0, 0, 0, 0.05)'
                }}
              >
                {/* 这里的图标单独控制颜色 */}
                <span style={{ fontSize: '16px', marginTop: '-1px' }}>⚠️</span>

                <div className="flex flex-col gap-1">
                  <strong
                    style={{
                      color: '#9a3412', // 深橘红色
                      fontSize: '13px',
                      fontWeight: '700'
                    }}
                  >
                    风险提示
                  </strong>
                  <p
                    style={{
                      color: '#c2410c', // 标准警告橘红
                      fontSize: '12px',
                      lineHeight: '1.5',
                      margin: 0
                    }}
                  >
                    解除禁用节点可能存在风险。启用此功能可能会允许执行潜在不安全的节点操作（如执行系统命令等），请谨慎使用。
                  </p>
                </div>
              </div>
            </div>
          </div>

          {/* 隧道控制 */}
          <div className="service-card">
            <div className="service-header">
              <div className="service-info">
                <h4 className="service-name">Cloudflare 隧道</h4>
                <span className={`service-status ${getTunnelStatusColor()}`}>
                  {getTunnelStatusText()}
                </span>
              </div>
              <div className="service-switch">
                <label className="switch">
                  <input
                    type="checkbox"
                    checked={tunnelStatus === "online" || tunnelStatus === "connecting"}
                    onChange={(e) => {
                      if (e.target.checked) {
                        startTunnel();
                      } else {
                        stopTunnel();
                      }
                    }}
                    disabled={isTunnelLoading}
                  />
                  <span className="slider"></span>
                </label>
              </div>
            </div>

            {/* 隧道URL显示 */}
            {tunnelStatus === "online" && tunnelUrl && (
              <div className="tunnel-url-section">
                <div className="tunnel-url-label">公网地址:</div>
                <div className="tunnel-url-value">
                  <a
                    href={tunnelUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="tunnel-link"
                  >
                    {tunnelUrl.replace('https://', '')}
                  </a>
                </div>
              </div>
            )}

            {/* cloudflared 信息 */}
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

        {/* 应用设置 & 关于区域 */}
        <div className="app-info-section">
          <h3 className="section-title">应用信息</h3>

          <div className="app-info-card">
            <div className="version-info">
              <div className="version-label">当前版本</div>
              <div className="version-value">v{appVersion}</div>
            </div>

            <button
              onClick={checkForUpdates}
              disabled={isCheckingUpdate}
              className="check-update-btn"
            >
              {isCheckingUpdate ? (
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
      </div>

      {/* 底部状态栏 */}
      <div className="sidebar-footer-minimal">
        <div className="footer-status">
          <div className={`status-dot ${n8nStatus === "running" ? "running" : "stopped"}`} />
          <span className="status-text">n8n {getN8nStatusText()}</span>
        </div>
      </div>
    </div>
  );
}