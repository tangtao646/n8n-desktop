import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import TunnelManager, { TunnelStatus, CloudflaredVersionInfo } from "./TunnelManager";

interface SidebarPanelProps {
  collapsed?: boolean;
  onToggleSidebar?: () => void;
  className?: string;
}

export default function SidebarPanel({ collapsed = false, onToggleSidebar, className = "" }: SidebarPanelProps) {
  const [tunnelStatus, setTunnelStatus] = useState<TunnelStatus>("offline");
  const [tunnelUrl, setTunnelUrl] = useState<string>("");
  const [cloudflaredInfo, setCloudflaredInfo] = useState<CloudflaredVersionInfo | null>(null);
  const [appVersion, setAppVersion] = useState<string>("1.0.0");
  const [n8nStatus, setN8nStatus] = useState<"running" | "stopped" | "starting">("running");
  const [activeTab, setActiveTab] = useState<"tunnel" | "settings" | "about">("tunnel");

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

  // 重启 n8n 服务
  const restartN8n = async () => {
    try {
      setN8nStatus("starting");
      // 先关闭 n8n
      invoke("shutdown_n8n");
      // 等待一下再启动
      setTimeout(async () => {
        await invoke("launch_n8n");
        setN8nStatus("running");
      }, 1000);
    } catch (err) {
      console.error("Failed to restart n8n:", err);
      setN8nStatus("stopped");
    }
  };

  // 隧道状态变化处理
  const handleTunnelStatusChange = (status: TunnelStatus, url?: string) => {
    setTunnelStatus(status);
    if (url) {
      setTunnelUrl(url);
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
          setTunnelStatus(status.toLowerCase() as TunnelStatus);
          if (url) {
            setTunnelUrl(url);
          }
        });

        // 加载应用信息
        await loadAppInfo();
        await checkN8nStatus();
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

  return (
    <div className={`sidebar-panel ${collapsed ? 'collapsed' : ''} ${className}`}>
      {/* 顶部标题栏 - 折叠时只显示切换按钮 */}
      <div className="sidebar-header">
        {!collapsed && (
          <div className="sidebar-title">
            <h2 className="text-lg font-bold">n8n Desktop</h2>
            <span className="text-xs text-gray-500">v{appVersion}</span>
          </div>
        )}
        <button
          onClick={handleToggleSidebar}
          className="sidebar-toggle-btn"
          title={collapsed ? "展开侧边栏" : "折叠侧边栏"}
        >
          {collapsed ? "▶" : "◀"}
        </button>
      </div>

      {/* 标签导航 - 折叠时隐藏，展开时显示 */}
      {!collapsed && (
        <div className="sidebar-tabs">
          <button
            className={`sidebar-tab ${activeTab === "tunnel" ? "active" : ""}`}
            onClick={() => setActiveTab("tunnel")}
          >
            <span className="tab-icon">🌐</span>
            <span className="tab-text">隧道</span>
          </button>
          <button
            className={`sidebar-tab ${activeTab === "settings" ? "active" : ""}`}
            onClick={() => setActiveTab("settings")}
          >
            <span className="tab-icon">⚙️</span>
            <span className="tab-text">设置</span>
          </button>
          <button
            className={`sidebar-tab ${activeTab === "about" ? "active" : ""}`}
            onClick={() => setActiveTab("about")}
          >
            <span className="tab-icon">ℹ️</span>
            <span className="tab-text">关于</span>
          </button>
        </div>
      )}

      {/* 内容区域 - 折叠时隐藏 */}
      {!collapsed && (
        <div className="sidebar-content">
          {activeTab === "tunnel" && (
            <div className="tunnel-tab">
              <TunnelManager
                onStatusChange={handleTunnelStatusChange}
                className="compact"
              />
              
              {/* n8n 状态卡片 */}
              <div className="n8n-status-card">
                <div className="n8n-status-header">
                  <h3 className="text-md font-semibold">n8n 服务</h3>
                  <span className={`text-sm font-medium ${getN8nStatusColor()}`}>
                    {getN8nStatusText()}
                  </span>
                </div>
                
                <div className="n8n-status-info">
                  <div className="status-item">
                    <span className="label">本地地址:</span>
                    <a
                      href="http://localhost:5678"
                      target="_blank"
                      rel="noopener noreferrer"
                      className="value link"
                    >
                      http://localhost:5678
                    </a>
                  </div>
                  
                  {tunnelStatus === "online" && tunnelUrl && (
                    <div className="status-item">
                      <span className="label">公网地址:</span>
                      <a
                        href={tunnelUrl}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="value link"
                      >
                        {tunnelUrl.replace('https://', '')}
                      </a>
                    </div>
                  )}
                </div>
                
                <div className="n8n-actions">
                  <button
                    onClick={restartN8n}
                    disabled={n8nStatus === "starting"}
                    className="action-btn primary"
                  >
                    {n8nStatus === "starting" ? "重启中..." : "重启服务"}
                  </button>
                  <button
                    onClick={() => window.open("http://localhost:5678", "_blank")}
                    className="action-btn secondary"
                  >
                    新窗口打开
                  </button>
                </div>
              </div>
            </div>
          )}

          {activeTab === "settings" && (
            <div className="settings-tab">
              <h3 className="text-md font-semibold mb-4">应用设置</h3>
              
              <div className="settings-section">
                <h4 className="text-sm font-medium mb-2">启动设置</h4>
                <div className="setting-item">
                  <label className="setting-label">
                    <input type="checkbox" className="mr-2" />
                    启动时自动检查更新
                  </label>
                </div>
                <div className="setting-item">
                  <label className="setting-label">
                    <input type="checkbox" className="mr-2" />
                    最小化到系统托盘
                  </label>
                </div>
              </div>

              <div className="settings-section">
                <h4 className="text-sm font-medium mb-2">隧道设置</h4>
                <div className="setting-item">
                  <label className="setting-label">
                    <input type="checkbox" className="mr-2" />
                    应用启动时自动连接隧道
                  </label>
                </div>
                <div className="setting-item">
                  <label className="setting-label">
                    <input type="checkbox" className="mr-2" />
                    隧道断开时显示通知
                  </label>
                </div>
              </div>

              <div className="settings-section">
                <h4 className="text-sm font-medium mb-2">高级设置</h4>
                <div className="setting-item">
                  <span className="setting-label">日志级别:</span>
                  <select className="setting-input">
                    <option>info</option>
                    <option>debug</option>
                    <option>warn</option>
                    <option>error</option>
                  </select>
                </div>
                <button className="action-btn secondary mt-2">
                  打开日志目录
                </button>
              </div>
            </div>
          )}

          {activeTab === "about" && (
            <div className="about-tab">
              <div className="about-header">
                <div className="app-icon">n8n</div>
                <h3 className="text-lg font-bold">n8n Desktop</h3>
                <p className="text-sm text-gray-600">版本 {appVersion}</p>
              </div>

              <div className="about-info">
                <p className="text-sm mb-4">
                  n8n Desktop 是一个本地运行的 n8n 工作流自动化平台桌面客户端。
                </p>
                
                <div className="info-section">
                  <h4 className="text-sm font-medium mb-2">系统信息</h4>
                  <div className="info-item">
                    <span className="label">Cloudflared:</span>
                    <span className="value">
                      {cloudflaredInfo?.installed
                        ? `已安装 ${cloudflaredInfo.version || "未知版本"}`
                        : "未安装"}
                    </span>
                  </div>
                  <div className="info-item">
                    <span className="label">n8n 版本:</span>
                    <span className="value">1.0.0</span>
                  </div>
                  <div className="info-item">
                    <span className="label">运行时间:</span>
                    <span className="value">--</span>
                  </div>
                </div>

                <div className="about-actions">
                  <button className="action-btn secondary">
                    检查更新
                  </button>
                  <button className="action-btn secondary">
                    查看文档
                  </button>
                  <button className="action-btn secondary">
                    报告问题
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {/* 底部状态栏 - 折叠时隐藏 */}
      {!collapsed && (
        <div className="sidebar-footer">
          <div className="footer-status">
            <div className={`status-dot ${n8nStatus === "running" ? "running" : "stopped"}`} />
            <span className="status-text">n8n {getN8nStatusText()}</span>
          </div>
          <div className="footer-actions">
            <button className="footer-btn" title="退出应用">
              ⏻
            </button>
          </div>
        </div>
      )}
    </div>
  );
}