import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import "./App.css";

type Status = 
  | "checking"          // 正在检查安装状态
  | "preparing_engine"  // 正在下载/准备 Node 运行时
  | "downloading_n8n"   // 正在下载 n8n 核心包
  | "starting"          // 正在启动服务
  | "ready"             // 服务已就绪
  | "error";            // 发生错误

export default function App() {
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
      console.log("n8n 健康检查失败:", err);
      return false;
    }
  };

  useEffect(() => {
    let unlistenProgress: UnlistenFn | null = null;
    let checkTimer: number | null = null;
    let retryCount = 0;
    const MAX_RETRIES = 5;

    const init = async () => {
      try {
        // 1. 设置进度监听器
        unlistenProgress = await listen<{ progress: number }>("download-progress", (e) => {
          setProgress(Math.round(e.payload.progress));
        });

        // 2. 准备 Node 运行时
        setStatus("preparing_engine");
        setProgress(0);
        await invoke("setup_runtime");

        // 3. 检查 n8n 核心是否已安装
        const installed = await invoke<boolean>("is_installed");
        
        if (!installed) {
          setStatus("downloading_n8n");
          setProgress(0);
          await invoke("setup_n8n");
        }

        // 4. 启动 n8n 服务
        setStatus("starting");
        await invoke("launch_n8n");

        // 5. 轮询检测 n8n 健康状态（通过代理）
        checkTimer = window.setInterval(async () => {
          try {
            const isReady = await checkHealthViaProxy();
            
            if (isReady) {
              setStatus("ready");
              if (checkTimer) {
                clearInterval(checkTimer);
              }
            } else {
              retryCount++;
              if (retryCount >= MAX_RETRIES) {
                setErrorMsg("n8n 服务启动超时，请检查端口是否被占用");
                setStatus("error");
                if (checkTimer) {
                  clearInterval(checkTimer);
                }
              }
            }
          } catch (error) {
            console.log("健康检查轮询出错:", error);
            retryCount++;
          }
        }, 2000);

        // 设置总超时（60秒）
        setTimeout(() => {
          if (status !== "ready" && checkTimer) {
            setErrorMsg("启动超时，请检查网络连接或重启应用");
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
      if (checkTimer) clearInterval(checkTimer);
    };
  }, []);

  // 状态显示逻辑
  const renderStatusText = () => {
    switch (status) {
      case "checking": return "正在检查系统环境...";
      case "preparing_engine": return `正在准备 Node 引擎... ${progress}%`;
      case "downloading_n8n": return `正在下载 n8n 资源... ${progress}%`;
      case "starting": return "正在启动 n8n 服务...";
      case "error": return `启动失败: ${errorMsg}`;
      default: return "正在载入界面...";
    }
  };

  if (status === "ready") {
    // 直接重定向整个窗口到 n8n，避免跨域问题
    window.location.href = "http://localhost:5678";
    return (
      <div style={styles.container}>
        <div style={styles.card}>
          <h2 style={styles.title}>n8n Desktop</h2>
          <p style={styles.statusText}>正在跳转到 n8n...</p>
        </div>
      </div>
    );
  }

  return (
    <div style={styles.container}>
      <div style={styles.card}>
        <h2 style={styles.title}>n8n Desktop</h2>
        
        {status !== "error" ? (
          <>
            <div style={styles.progressContainer}>
              <div style={{ ...styles.progressBar, width: `${progress}%` }} />
            </div>
            <p style={styles.statusText}>{renderStatusText()}</p>
          </>
        ) : (
          <div style={styles.errorBox}>
            <p>{renderStatusText()}</p>
            <button 
              onClick={() => {
                setStatus("checking");
                setErrorMsg("");
                setProgress(0);
                window.location.reload();
              }} 
              style={styles.retryBtn}
            >
              重试
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

const styles: { [key: string]: React.CSSProperties } = {
  container: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100vh',
    backgroundColor: '#f4f7f9',
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif'
  },
  card: {
    width: '400px',
    padding: '40px',
    textAlign: 'center',
    backgroundColor: '#ffffff',
    borderRadius: '12px',
    boxShadow: '0 4px 20px rgba(0,0,0,0.08)'
  },
  title: {
    margin: '0 0 20px 0',
    color: '#333',
    fontSize: '24px'
  },
  progressContainer: {
    width: '100%',
    height: '8px',
    backgroundColor: '#eee',
    borderRadius: '4px',
    overflow: 'hidden',
    marginBottom: '15px'
  },
  progressBar: {
    height: '100%',
    backgroundColor: '#ff6d5a',
    transition: 'width 0.3s ease'
  },
  statusText: {
    fontSize: '14px',
    color: '#666',
    margin: 0
  },
  errorBox: {
    color: '#d93025',
    fontSize: '14px'
  },
  retryBtn: {
    marginTop: '15px',
    padding: '8px 20px',
    backgroundColor: '#ff6d5a',
    color: '#fff',
    border: 'none',
    borderRadius: '5px',
    cursor: 'pointer'
  }
};