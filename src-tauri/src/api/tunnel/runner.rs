use super::models::TunnelMode;
use super::utils::{handle_tunnel_url, process_tunnel_url_match};
use regex::Regex;
use std::io::{self, BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tauri::{AppHandle, Runtime};

/// 隧道进程运行器
pub struct TunnelRunner {
    cloudflared_path: String,
    tunnel_mode: TunnelMode,
}

impl TunnelRunner {
    /// 创建新的运行器实例
    pub const fn new(cloudflared_path: String, tunnel_mode: TunnelMode) -> Self {
        Self {
            cloudflared_path,
            tunnel_mode,
        }
    }

    /// 清理之前的 cloudflared 进程
    pub fn cleanup_prev_processes() {
        #[cfg(unix)]
        let _ = Command::new("pkill").args(["-f", "cloudflared"]).output();
        #[cfg(windows)]
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "cloudflared.exe", "/T"])
            .output();
    }

    /// 启动 cloudflared 进程
    pub fn spawn(&self) -> io::Result<Child> {
        let (args, message) = match &self.tunnel_mode {
            TunnelMode::Token { token, .. } => (
                vec!["tunnel", "run", "--token", token],
                "[Tunnel] 启动 Token 模式：跳过本地配置，连接云端端点...",
            ),
            TunnelMode::Temporary => (
                vec![
                    "tunnel",
                    "--url",
                    "http://localhost:5678",
                    "--no-autoupdate",
                ],
                "[Tunnel] 启动临时模式：准备捕获随机域名...",
            ),
        };

        println!("{}", message);

        Command::new(&self.cloudflared_path)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }
}

/// 隧道输出监视器
pub struct TunnelMonitor<R: Runtime> {
    pub app: AppHandle<R>,
    pub mode: TunnelMode,
}

impl<R: Runtime> TunnelMonitor<R> {
    /// 异步监视 stderr 输出
    pub async fn watch(&self, stderr: impl io::Read + Send + 'static) {
        match &self.mode {
            TunnelMode::Token { domain, .. } => {
                self.watch_token_mode(stderr, domain.clone()).await;
            }
            TunnelMode::Temporary => {
                self.watch_temporary_mode(stderr).await;
            }
        }
    }

    /// 监视 Token 模式输出
    async fn watch_token_mode(&self, stderr: impl io::Read + Send + 'static, domain: String) {
        let app = self.app.clone();

        // 使用 tokio::spawn 在后台运行阻塞的 I/O 操作
        tokio::task::spawn_blocking(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(l) = line {
                    // 检测错误日志
                    if l.contains("error") || l.contains("Error") || l.contains("ERROR") {
                        println!("[Tunnel] Cloudflared 错误: {}", l);
                    }
                }
            }
        });

        // 给予 cloudflared 一定的连接握手时间
        tokio::time::sleep(Duration::from_secs(2)).await;

        // 格式化域名：确保以 https:// 开头
        let final_url = if domain.starts_with("http") {
            domain
        } else {
            format!("https://{}", domain)
        };

        println!("[Tunnel] Token 模式上线，应用域名: {}", final_url);

        // 使用 handle_tunnel_url 处理 URL（包含验证、更新状态、保存配置、重启 n8n）
        // is_temporary = false 表示这是 Token 模式（非临时域名）
        handle_tunnel_url(&final_url, false, &app);
    }

    /// 监视临时模式输出
    async fn watch_temporary_mode(&self, stderr: impl io::Read + Send + 'static) {
        let app = self.app.clone();

        tokio::task::spawn_blocking(move || {
            let reader = BufReader::new(stderr);
            let regex_temp = Regex::new(r"https://[a-z0-9-]+\.trycloudflare\.com")
                .expect("Failed to compile temporary tunnel URL regex");
            let mut found_url = false;

            for line in reader.lines() {
                if let Ok(l) = line {
                    // println!("Cloudflared: {}", l); // 调试时可开启
                    if !found_url {
                        if let Some(mat) = regex_temp.find(&l) {
                            found_url = process_tunnel_url_match(&mat, true, &app);
                        }
                    }
                }
            }
        });
    }
}
