import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useI18n } from "./i18n/context";
import "./App.css";

type Status =
  | "checking"          // 正在检查安装状态
  | "preparing_engine"  // 正在下载/准备 Node 运行时
  | "downloading_n8n"   // 正在下载 n8n 核心包
  | "extracting"        // 正在解压资源包
  | "starting"          // 正在启动服务
  | "ready"             // 服务已就绪
  | "error";            // 发生错误

export default function App() {
  const { t } = useI18n();
  const [status, setStatus] = useState<Status>("checking");
  const [progress, setProgress] = useState(0);
  const [errorMsg, setErrorMsg] = useState("");

  // 通过 Tauri 代理检查 n8n 健康状态
  const checkHealthViaProxy = async (): Promise<boolean> => {
    try {
      // 关键修改：使用 Tauri 命令代替直接 fetch
      const result = await invoke<string>("proxy_health_check");

      // 根据后端返回结果判断是否健康
      // 假设后端返回 "healthy" 或 "ready" 表示正常
      if (typeof result === 'string' && (
        result.toLowerCase().includes("healthy") ||
        result.toLowerCase().includes("ready") ||
        result.includes("200")
      )) {
        return true;
      }
      return false;
    } catch (err) {
      console.log("n8n health check failed:", err);
      return false;
    }
  };

  useEffect(() => {
    let unlistenProgress: UnlistenFn | null = null;
    let unlistenExtractionStart: UnlistenFn | null = null;
    let checkTimer: number | null = null;
    let retryCount = 0;
    const MAX_RETRIES = 5;
    
    // 用于跟踪当前下载类型和防抖
    let currentDownloadType = "";
    let lastProgressUpdate = 0;
    const PROGRESS_DEBOUNCE_MS = 100;

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

        // 4. 启动 n8n 服务
        setStatus("starting");
        await invoke("launch_n8n");

        // 5. 轮询检测 n8n 健康状态（通过代理）
        checkTimer = window.setInterval(async () => {
          try {
            const isReady = await checkHealthViaProxy();

            if (isReady) {
              // 健康检查通过，等待 2 秒确保 n8n UI 完全就绪
              if (checkTimer) {
                clearInterval(checkTimer);
              }
              setTimeout(() => {
                setStatus("ready");
              }, 2000);
            } else {
              retryCount++;
              if (retryCount >= MAX_RETRIES) {
                setErrorMsg(t("errors.timeout"));
                setStatus("error");
                if (checkTimer) {
                  clearInterval(checkTimer);
                }
              }
            }
          } catch (error) {
            console.log("Health check polling error:", error);
            retryCount++;
          }
        }, 2000);

        // 设置总超时（60秒）
        setTimeout(() => {
          if (status !== "ready" && checkTimer) {
            setErrorMsg(t("errors.startup_timeout"));
            setStatus("error");
            clearInterval(checkTimer);
          }
        }, 60000);

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
      if (checkTimer) clearInterval(checkTimer);
    };
  }, [t]);

  // 状态显示逻辑
  const renderStatusText = () => {
    switch (status) {
      case "checking": return t("status.checking");
      case "preparing_engine": return t("status.preparing_engine", { progress });
      case "downloading_n8n": return t("status.downloading_n8n", { progress });
      case "extracting": return t("status.extracting");
      case "starting": return t("status.starting");
      case "error": return t("status.error", { error: errorMsg });
      default: return t("status.loading");
    }
  };

  if (status === "ready") {
    // 直接重定向整个窗口到 n8n，避免跨域问题
    window.location.href = "http://localhost:5678";
    return (
      <div className="n8n-container">
        <div className="n8n-card">
          <h2 className="n8n-title">{t("app.title")}</h2>
          <p className="n8n-status-text">{t("app.redirecting")}</p>
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