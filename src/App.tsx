import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useI18n } from "./i18n/context";
import { CloudflaredVersionInfo } from "./components/TunnelManager";
import SidebarPanel from "./components/SidebarPanel";
import "./App.css";

type Status =
  | "checking"          // 正在检查安装状态
  | "preparing_engine"  // 正在下载/准备 Node 运行时
  | "downloading_n8n"   // 正在下载 n8n 核心包
  | "extracting"        // 正在解压资源包
  | "preparing_tunnel"  // 正在准备 Cloudflare Tunnel
  | "starting"          // 正在启动服务
  | "ready"             // 服务已就绪
  | "error";            // 发生错误

export default function App() {
  const { t } = useI18n();
  const [status, setStatus] = useState<Status>("checking");
  const [progress, setProgress] = useState(0);
  const [errorMsg, setErrorMsg] = useState("");
  const [iframeLoaded, setIframeLoaded] = useState(false);
  const [iframeError, setIframeError] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    // 从 localStorage 读取保存的折叠状态，默认折叠为 true
    const saved = localStorage.getItem('sidebarCollapsed');
    // 如果 localStorage 中没有保存过，则默认折叠
    if (saved === null) {
      return true;
    }
    return saved === 'true';
  });

  // 保存折叠状态到 localStorage
  const handleToggleSidebar = () => {
    const newState = !sidebarCollapsed;
    setSidebarCollapsed(newState);
    localStorage.setItem('sidebarCollapsed', newState.toString());
  };

  // 通过 Tauri 代理检查 n8n 健康状态，添加超时和重试，增加对瞬态错误的容忍度
  const checkHealthViaProxy = async (): Promise<boolean> => {
    try {
      // 增加超时时间到8秒，给后端重试逻辑更多时间
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error("健康检查超时")), 8000);
      });

      // 关键修改：使用 Tauri 命令代替直接 fetch
      const resultPromise = invoke<string>("proxy_health_check");
      const result = await Promise.race([resultPromise, timeoutPromise]);

      // 根据后端返回结果判断是否健康
      // 后端返回格式: "healthy - 200 - {\"status\":\"ok\"}" 或类似
      if (typeof result === 'string') {
        const lowerResult = result.toLowerCase();
        // 放宽健康检查条件：只要包含 healthy, ready, ok, 200 或 201 都算健康
        if (lowerResult.includes("healthy") ||
          lowerResult.includes("ready") ||
          result.includes("200") ||
          result.includes("201") ||
          lowerResult.includes("ok")) {
          console.log("n8n health check passed:", result);
          return true;
        }
        // 记录非健康响应但不立即失败（让重试逻辑处理）
        console.log("n8n health check returned non-healthy response:", result);
      }
      return false;
    } catch (err) {
      // 对特定错误类型更宽容：502 Bad Gateway 可能是瞬态错误
      const errMsg = String(err);
      if (errMsg.includes("502") || errMsg.includes("Bad Gateway")) {
        console.log("n8n health check encountered transient 502 error, will retry:", err);
      } else {
        console.log("n8n health check failed:", err);
      }
      return false;
    }
  };

  useEffect(() => {
    let unlistenProgress: UnlistenFn | null = null;
    let unlistenExtractionStart: UnlistenFn | null = null;
    let checkTimer: number | null = null;
    let retryCount = 0;
    const MAX_RETRIES = 8; // 增加重试次数，给瞬态错误更多机会

    // 用于跟踪当前下载类型和防抖
    let currentDownloadType = "";
    let lastProgressUpdate = 0;
    const PROGRESS_DEBOUNCE_MS = 100;
    
    // 用于跟踪是否已经成功（防止重复设置 ready 状态）
    let hasSucceeded = false;

    const init = async () => {
      try {
        // 1. 设置进度监听器，根据下载类型过滤
        unlistenProgress = await listen<{ progress: number; download_type: string }>("download-progress", (e) => {
          const { progress, download_type } = e.payload;

          // 只处理当前活动下载类型的进度事件
          if (download_type !== currentDownloadType) {
            return;
          }

          // 防抖处理：避免频繁更新
          const now = Date.now();
          if (now - lastProgressUpdate < PROGRESS_DEBOUNCE_MS) {
            return;
          }

          // 确保进度不倒退（防止旧事件干扰）
          const roundedProgress = Math.round(progress);
          setProgress((prev) => {
            // 只允许进度增加或保持不变，防止跳回
            if (roundedProgress >= prev || roundedProgress === 100) {
              return roundedProgress;
            }
            // 如果新进度小于当前进度，可能是旧事件，忽略
            return prev;
          });

          lastProgressUpdate = now;
        });

        // 2. 设置解压开始监听器
        unlistenExtractionStart = await listen<{ download_type: string }>("extraction-start", (e) => {
          const { download_type } = e.payload;

          // 只处理当前活动下载类型的解压事件
          if (download_type !== currentDownloadType) {
            return;
          }

          // 更新状态为解压中
          if (download_type === "n8n-core") {
            setStatus("extracting");
          }
          // 对于 runtime 下载，保持 preparing_engine 状态但可以更新文本
          // 这里暂时不处理，因为 runtime 状态文本已经固定
        });

        // 2. 准备 Node 运行时
        setStatus("preparing_engine");
        setProgress(0);
        currentDownloadType = "runtime";
        await invoke("setup_runtime");
        currentDownloadType = ""; // 清除当前下载类型

        // 3. 检查并安装 n8n
        let installed = await invoke<boolean>("is_installed");

        if (!installed) {
          setStatus("downloading_n8n");
          setProgress(0);
          currentDownloadType = "n8n-core";
          await invoke("setup_n8n");
          currentDownloadType = ""; // 清除当前下载类型

          // 进度条保持 100%，状态可能已经是 extracting（由事件触发）
          // 如果解压很快，可能已经完成，状态还是 downloading_n8n
          // 确保进度显示 100%
          setProgress(100);

          // --- 关键修改点：等待文件系统稳定 ---
          // 下载完成后，状态设为 starting 前，先确认一下
          let checkInstalled = false;
          let attempts = 0;

          // 循环检查 5 次，每次间隔 1 秒，确保解压后的 bin 文件确实存在了
          while (!checkInstalled && attempts < 5) {
            checkInstalled = await invoke<boolean>("is_installed");
            if (!checkInstalled) {
              await new Promise(r => setTimeout(r, 1000));
              attempts++;
            }
          }

          if (!checkInstalled) {
            throw new Error("资源包已下载，但未能正确安装（验证失败）");
          }
        }

        // 4. 准备 Cloudflare Tunnel（检查并下载 cloudflared）
        setStatus("preparing_tunnel");
        setProgress(0);
        currentDownloadType = "cloudflared";

        try {
          // 检查 cloudflared 版本，如果不存在会自动下载
          const versionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");

          if (!versionInfo.installed) {
            // 触发下载
            await invoke("download_cloudflared");
            // 下载完成后再次检查
            const newVersionInfo = await invoke<CloudflaredVersionInfo>("check_cloudflared_version");
            if (!newVersionInfo.installed) {
              throw new Error("Failed to download cloudflared");
            }
          }
        } catch (err: any) {
          console.warn("Cloudflared preparation failed, tunnel feature may be unavailable:", err);
          // 不阻止应用启动，只是记录警告
        } finally {
          currentDownloadType = ""; // 清除当前下载类型
        }

        // 5. 启动 n8n 服务
        setStatus("starting");
        await invoke("launch_n8n");

        // 5. 轮询检测 n8n 健康状态（通过代理）
        checkTimer = window.setInterval(async () => {
          try {
            const isReady = await checkHealthViaProxy();

            if (isReady && !hasSucceeded) {
              hasSucceeded = true;
              // 健康检查通过，等待 2 秒确保 n8n UI 完全就绪
              if (checkTimer) {
                clearInterval(checkTimer);
                checkTimer = null;
              }
              setTimeout(() => {
                setStatus("ready");
              }, 2000);
            } else if (!isReady) {
              retryCount++;
              console.log(`Health check failed, retry ${retryCount}/${MAX_RETRIES}`);
              if (retryCount >= MAX_RETRIES) {
                setErrorMsg(t("errors.timeout"));
                setStatus("error");
                if (checkTimer) {
                  clearInterval(checkTimer);
                  checkTimer = null;
                }
              }
            }
          } catch (error) {
            console.log("Health check polling error:", error);
            retryCount++;
          }
        }, 2000);

        // 设置总超时（60秒）- 大幅增加时间以应对慢速启动和瞬态错误
        const totalTimeout = 60000;
        const timeoutId = window.setTimeout(() => {
          // 检查当前状态（通过闭包捕获的变量可能过时，所以直接检查 hasSucceeded）
          if (!hasSucceeded && checkTimer) {
            console.log("Startup timeout triggered, current status:", status);
            console.log("checkTimer is:", checkTimer);
            
            // 提供更详细的错误信息
            let detailedError = t("errors.startup_timeout");
            if (status === "preparing_engine" || status === "downloading_n8n" || status === "preparing_tunnel") {
              detailedError = "网络下载超时，请检查网络连接或尝试使用VPN";
            } else if (status === "starting") {
              detailedError = "n8n服务启动超时，请检查端口5678是否被占用或服务启动过慢";
            }
            setErrorMsg(detailedError);
            setStatus("error");
            if (checkTimer) {
              clearInterval(checkTimer);
              checkTimer = null;
            }
          }
        }, totalTimeout);

        // 清理总超时
        return () => {
          window.clearTimeout(timeoutId);
        };

      } catch (err: any) {
        console.error("Initialization failed:", err);
        setErrorMsg(err.toString());
        setStatus("error");
      }
    };

    init();

    // 清理函数
    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenExtractionStart) unlistenExtractionStart();
      if (checkTimer) {
        clearInterval(checkTimer);
        checkTimer = null;
      }
    };
  }, [t]);

  // 状态显示逻辑
  const renderStatusText = () => {
    switch (status) {
      case "checking": return t("status.checking");
      case "preparing_engine": return t("status.preparing_engine", { progress });
      case "downloading_n8n": return t("status.downloading_n8n", { progress });
      case "extracting": return t("status.extracting");
      case "preparing_tunnel": return t("status.preparing_tunnel", { progress });
      case "starting": return t("status.starting");
      case "error": return t("status.error", { error: errorMsg });
      default: return t("status.loading");
    }
  };


  if (status === "ready") {
    return (
      <div className="main-container">
        {/* 左侧设置面板 */}
        <SidebarPanel
          collapsed={sidebarCollapsed}
          onToggleSidebar={handleToggleSidebar}
        />

        {/* 右侧 n8n Web UI */}
        <div className={`main-content ${sidebarCollapsed ? 'expanded' : ''}`}>
          {!iframeLoaded && !iframeError && (
            <div className="iframe-loading">
              <div className="loading-spinner"></div>
              <p>正在加载 n8n 界面...</p>
            </div>
          )}

          {iframeError && (
            <div className="iframe-fallback">
              <div className="fallback-content">
                <h3>无法加载 n8n 界面</h3>
                <p>请确保 n8n 服务正在运行在 localhost:5678</p>
                <div className="fallback-actions">
                  <button
                    onClick={() => {
                      setIframeError(false);
                      setIframeLoaded(false);
                    }}
                    className="action-btn primary"
                  >
                    重试
                  </button>
                  <button
                    onClick={() => window.open("http://localhost:5678", "_blank")}
                    className="action-btn secondary"
                  >
                    在新窗口中打开
                  </button>
                </div>
              </div>
            </div>
          )}

          <iframe
            src="http://localhost:5678"
            className="webview-container"
            title="n8n Editor"
            // 放宽 sandbox 限制以支持更多功能
            sandbox="allow-same-origin allow-scripts allow-popups allow-forms allow-modals allow-top-navigation allow-downloads"
            // 允许更多权限
            allow="clipboard-read; clipboard-write; fullscreen; microphone; camera"
            // 添加 referrer 策略
            referrerPolicy="no-referrer-when-downgrade"
            // 允许跨域资源共享
            allowFullScreen
            // 加载状态处理
            onLoad={() => {
              setIframeLoaded(true);
              setIframeError(false);
            }}
            onError={() => {
              setIframeError(true);
              setIframeLoaded(false);
            }}
            style={{ display: iframeLoaded && !iframeError ? 'block' : 'none' }}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="n8n-container">
      <div className="n8n-card">
        <h2 className="n8n-title">{t("app.title")}</h2>

        {status !== "error" ? (
          <>
            <div className="n8n-progress-container">
              <div
                className="n8n-progress-bar"
                style={{ width: `${progress}%` }}
              />
            </div>
            <p className="n8n-status-text">{renderStatusText()}</p>
          </>
        ) : (
          <div className="n8n-error-box">
            <p>{renderStatusText()}</p>
            <button
              onClick={() => {
                setStatus("checking");
                setErrorMsg("");
                setProgress(0);
                window.location.reload();
              }}
              className="n8n-retry-btn"
            >
              {t("app.retry")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}