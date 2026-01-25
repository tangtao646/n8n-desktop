import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

// 隧道状态类型
export type TunnelStatus = "offline" | "connecting" | "online" | "error";

// 隧道事件类型
interface TunnelEvent {
  status: string;
  url?: string;
  progress?: number;
  message?: string;
}

// 隧道配置类型
interface TunnelConfig {
  last_url?: string;
  auto_start: boolean;
  created_at: string;
}

// Cloudflared 版本信息
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

export default function TunnelManager({ onStatusChange, className = "" }: TunnelManagerProps) {
  const [tunnelStatus, setTunnelStatus] = useState<TunnelStatus>("offline");
  const [tunnelUrl, setTunnelUrl] = useState<string>("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string>("");
  const [cloudflaredInfo, setCloudflaredInfo] = useState<CloudflaredVersionInfo | null>(null);
  const [config, setConfig] = useState<TunnelConfig>({ auto_start: false, created_at: "" });
  const [showConfig, setShowConfig] = useState(false);

  // 加载隧道状态和配置
  const loadTunnelState = async () => {
    try {
      // 获取隧道状态
      const status = await invoke<TunnelEvent>("get_tunnel_status");
      if (status.url) {
        setTunnelUrl(status.url);
        setTunnelStatus(status.status.toLowerCase() as TunnelStatus);
      } else {
        setTunnelStatus(status.status.toLowerCase() as TunnelStatus);
      }

      // 获取隧道配置
      const tunnelConfig = await invoke<TunnelConfig>("get_tunnel_config");
      setConfig(tunnelConfig);

      // 获取 cloudflared 版本信息
      const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
      setCloudflaredInfo(versionInfo);

      // 通知父组件状态变化
      if (onStatusChange) {
        onStatusChange(status.status.toLowerCase() as TunnelStatus, status.url);
      }
    } catch (err) {
      console.error("Failed to load tunnel state:", err);
      setError("无法加载隧道状态");
    }
  };

  // 启动隧道
  const startTunnel = async () => {
    if (isLoading) return;
    
    setIsLoading(true);
    setError("");
    
    try {
      // 首先检查 cloudflared 是否可用
      const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
      
      if (!versionInfo.installed) {
        // 如果没有安装 cloudflared，需要先下载
        setTunnelStatus("connecting");
        setError("正在下载 cloudflared...");
        
        // 这里需要实现 cloudflared 下载逻辑
        // 暂时使用默认路径
        const cloudflaredPath = "cloudflared";
        await invoke("start_tunnel", { cloudflaredPath });
      } else {
        // 使用现有的 cloudflared
        const cloudflaredPath = versionInfo.path || "cloudflared";
        await invoke("start_tunnel", { cloudflaredPath });
      }
      
      // 状态更新将通过事件监听器处理
    } catch (err: any) {
      console.error("Failed to start tunnel:", err);
      setError(`启动隧道失败: ${err.message || err}`);
      setTunnelStatus("error");
      setIsLoading(false);
    }
  };

  // 停止隧道
  const stopTunnel = async () => {
    if (isLoading) return;
    
    setIsLoading(true);
    setError("");
    
    try {
      await invoke("stop_tunnel");
      // 状态更新将通过事件监听器处理
    } catch (err: any) {
      console.error("Failed to stop tunnel:", err);
      setError(`停止隧道失败: ${err.message || err}`);
      setIsLoading(false);
    }
  };

  // 复制隧道 URL 到剪贴板
  const copyTunnelUrl = async () => {
    if (!tunnelUrl) return;
    
    try {
      await invoke("copy_tunnel_url");
      // 可以在这里添加复制成功的提示
      alert("隧道 URL 已复制到剪贴板");
    } catch (err) {
      console.error("Failed to copy URL:", err);
      setError("复制 URL 失败");
    }
  };

  // 更新隧道配置
  const updateConfig = async (updates: Partial<TunnelConfig>) => {
    try {
      await invoke("update_tunnel_config", updates);
      await loadTunnelState(); // 重新加载配置
    } catch (err) {
      console.error("Failed to update config:", err);
      setError("更新配置失败");
    }
  };

  // 清理 cloudflared 缓存
  const clearCloudflaredCache = async () => {
    try {
      await invoke("clear_cloudflared_cache");
      await loadTunnelState(); // 重新加载 cloudflared 信息
      alert("cloudflared 缓存已清理");
    } catch (err) {
      console.error("Failed to clear cache:", err);
      setError("清理缓存失败");
    }
  };

  // 初始化：加载状态和设置事件监听
  useEffect(() => {
    let unlistenTunnelUpdate: UnlistenFn | null = null;
    let unlistenTunnelCopied: UnlistenFn | null = null;

    const setupListeners = async () => {
      try {
        // 监听隧道状态更新事件
        unlistenTunnelUpdate = await listen<TunnelEvent>("tunnel-update", (event) => {
          const { status, url } = event.payload;
          
          setTunnelStatus(status.toLowerCase() as TunnelStatus);
          if (url) {
            setTunnelUrl(url);
          }
          
          // 如果隧道在线，停止加载状态
          if (status.toLowerCase() === "online") {
            setIsLoading(false);
            setError("");
          }
          
          // 如果隧道连接失败
          if (status.toLowerCase() === "error" || status.toLowerCase() === "offline") {
            setIsLoading(false);
          }
          
          // 通知父组件
          if (onStatusChange) {
            onStatusChange(status.toLowerCase() as TunnelStatus, url);
          }
        });

        // 监听复制成功事件
        unlistenTunnelCopied = await listen<TunnelEvent>("tunnel-copied", () => {
          // 可以在这里显示复制成功的通知
          console.log("Tunnel URL copied successfully");
        });

        // 初始加载状态
        await loadTunnelState();

        // 如果配置了自动启动且隧道未运行，自动启动隧道
        if (config.auto_start && tunnelStatus === "offline" && config.last_url) {
          setTimeout(() => {
            startTunnel();
          }, 1000);
        }
      } catch (err) {
        console.error("Failed to setup tunnel listeners:", err);
        setError("无法设置隧道监听器");
      }
    };

    setupListeners();

    // 清理函数
    return () => {
      if (unlistenTunnelUpdate) unlistenTunnelUpdate();
      if (unlistenTunnelCopied) unlistenTunnelCopied();
    };
  }, [config.auto_start]);

  // 状态显示文本
  const getStatusText = () => {
    switch (tunnelStatus) {
      case "offline": return "隧道已关闭";
      case "connecting": return "隧道连接中...";
      case "online": return "隧道已连接";
      case "error": return "隧道错误";
      default: return "未知状态";
    }
  };

  // 状态颜色
  const getStatusColor = () => {
    switch (tunnelStatus) {
      case "online": return "text-green-600";
      case "connecting": return "text-yellow-600";
      case "error": return "text-red-600";
      default: return "text-gray-600";
    }
  };

  return (
    <div className={`tunnel-manager ${className}`}>
      <div className="tunnel-header">
        <h3 className="text-lg font-semibold">Cloudflare 隧道</h3>
        <button
          onClick={() => setShowConfig(!showConfig)}
          className="text-sm text-gray-500 hover:text-gray-700"
        >
          {showConfig ? "隐藏配置" : "显示配置"}
        </button>
      </div>

      {/* 隧道状态卡片 */}
      <div className="tunnel-card">
        <div className="tunnel-status">
          <div className="flex items-center justify-between">
            <div>
              <span className={`font-medium ${getStatusColor()}`}>
                {getStatusText()}
              </span>
              {tunnelUrl && (
                <div className="mt-1">
                  <span className="text-sm text-gray-600">公网地址: </span>
                  <a
                    href={tunnelUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-sm text-blue-600 hover:text-blue-800 break-all"
                  >
                    {tunnelUrl}
                  </a>
                </div>
              )}
            </div>
            
            <div className="flex space-x-2">
              {tunnelStatus === "offline" || tunnelStatus === "error" ? (
                <button
                  onClick={startTunnel}
                  disabled={isLoading}
                  className="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 disabled:opacity-50"
                >
                  {isLoading ? "启动中..." : "启动隧道"}
                </button>
              ) : (
                <button
                  onClick={stopTunnel}
                  disabled={isLoading}
                  className="px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700 disabled:opacity-50"
                >
                  {isLoading ? "停止中..." : "停止隧道"}
                </button>
              )}
              
              {tunnelUrl && (
                <button
                  onClick={copyTunnelUrl}
                  className="px-4 py-2 bg-gray-200 text-gray-800 rounded hover:bg-gray-300"
                >
                  复制
                </button>
              )}
            </div>
          </div>
        </div>

        {/* 错误显示 */}
        {error && (
          <div className="mt-2 p-2 bg-red-50 text-red-700 rounded text-sm">
            {error}
          </div>
        )}

        {/* cloudflared 信息 */}
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

      {/* 配置面板 */}
      {showConfig && (
        <div className="tunnel-config mt-4 p-4 bg-gray-50 rounded">
          <h4 className="font-medium mb-3">隧道配置</h4>
          
          <div className="space-y-3">
            {/* 自动启动开关 */}
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

            {/* 上次使用的 URL */}
            {config.last_url && (
              <div className="text-sm">
                <div className="font-medium">上次隧道地址:</div>
                <div className="text-gray-600 break-all">{config.last_url}</div>
              </div>
            )}

            {/* 缓存管理 */}
            <div className="pt-2 border-t">
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
      )}

      {/* 使用说明 */}
      <div className="mt-4 text-sm text-gray-600">
        <p className="font-medium">使用说明:</p>
        <ul className="list-disc pl-5 mt-1 space-y-1">
          <li>启动隧道后，将获得一个临时的公网地址</li>
          <li>该地址可用于 OAuth 回调、Webhook 等外部服务访问</li>
          <li>隧道关闭后地址失效，确保隐私安全</li>
          <li>复制地址用于 Google OAuth、GitHub Webhook 等配置</li>
        </ul>
      </div>
    </div>
  );
}
