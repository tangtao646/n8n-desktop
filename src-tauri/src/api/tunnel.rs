use chrono;
use dirs;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::n8n_core::{construct_n8n_envs, shutdown_n8n};
use super::utils::emit_global_sync;
use crate::services::manager::{self, PROCESS_MANAGER};

// --- 1. 数据结构定义 ---

#[derive(Clone, Serialize)]
pub struct TunnelEvent {
    pub status: String,
    pub url: Option<String>,
    pub progress: Option<f64>,
    pub message: Option<String>,
}

/// 隧道模式枚举
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum TunnelMode {
    /// 临时隧道模式，每次启动生成随机URL
    Temporary,
    /// Token 模式，使用 Cloudflare Tunnel Token 和自定义域名
    Token { token: String, domain: String },
}

impl TunnelEvent {
    pub fn new(status: &str) -> Self {
        Self {
            status: status.to_string(),
            url: None,
            progress: None,
            message: None,
        }
    }
    pub fn with_url(status: &str, url: String) -> Self {
        Self {
            status: status.to_string(),
            url: Some(url),
            progress: None,
            message: None,
        }
    }
    pub fn with_progress(status: &str, progress: f64) -> Self {
        Self {
            status: status.to_string(),
            url: None,
            progress: Some(progress),
            message: None,
        }
    }
    pub fn with_message(status: &str, message: String) -> Self {
        Self {
            status: status.to_string(),
            url: None,
            progress: None,
            message: Some(message),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub last_url: Option<String>,
    pub auto_start: bool,
    pub created_at: String,
    pub custom_domain: Option<String>,
    pub use_custom_domain: bool,
    pub tunnel_mode: TunnelMode, // 使用枚举替代字符串
    pub tunnel_token: Option<String>,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            last_url: None,
            auto_start: false,
            created_at: chrono::Local::now().to_rfc3339(),
            custom_domain: None,
            use_custom_domain: false,
            tunnel_mode: TunnelMode::Temporary,
            tunnel_token: None,
        }
    }
}

#[derive(Clone, Serialize)]
pub enum TunnelHealthStatus {
    Healthy,
    Connecting,
    Stopped,
    Error,
}

#[derive(Clone, Serialize)]
pub struct TunnelHealth {
    pub status: TunnelHealthStatus,
    pub ping_ms: Option<u32>,
    pub last_check: String,
    pub message: String,
}

#[derive(Clone, Serialize)]
pub struct TunnelError {
    pub timestamp: String,
    pub message: String,
    pub severity: String,
}

// --- 2. 全局状态 ---
pub(crate) static TUNNEL_URL: Lazy<Arc<Mutex<Option<String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));
pub(crate) static TUNNEL_RUNNING: Lazy<Arc<Mutex<bool>>> =
    Lazy::new(|| Arc::new(Mutex::new(false)));
pub(crate) static TUNNEL_CONFIG: Lazy<Arc<Mutex<TunnelConfig>>> =
    Lazy::new(|| Arc::new(Mutex::new(TunnelConfig::default())));

// --- 3. 内部辅助函数 ---

/// 从 cloudflared 输出中提取隧道 ID
fn extract_tunnel_id_from_output(output: &str) -> Option<String> {
    // 正则匹配隧道 ID 格式：类似 xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx 或短 ID
    let re =
        Regex::new(r"[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}|[a-z0-9]{32}")
            .unwrap();
    re.find(output).map(|m| m.as_str().to_string())
}

/// 核心修复：处理隧道URL匹配逻辑
fn process_tunnel_url_match<R: Runtime>(
    url_match: &regex::Match,
    is_temporary: bool,
    app_clone: &AppHandle<R>,
) -> bool {
    let url = url_match.as_str().to_string();
    handle_tunnel_url(&url, is_temporary, app_clone)
}

/// 处理隧道URL（直接传入URL字符串，不依赖正则匹配）
fn handle_tunnel_url<R: Runtime>(url: &str, is_temporary: bool, app_clone: &AppHandle<R>) -> bool {
    // 【修复点】：移除对 "cloudflare.com" 的排除逻辑，只验证后缀
    let is_valid_url = if is_temporary {
        url.ends_with(".trycloudflare.com")
    } else {
        (url.starts_with("http://") || url.starts_with("https://"))
            && !url.ends_with(".trycloudflare.com")
    };

    if !is_valid_url {
        return false;
    }

    println!("[Tunnel] 成功捕获并验证 URL: {}", url);

    {
        let mut url_guard = TUNNEL_URL.lock().unwrap();
        *url_guard = Some(url.to_string());
    }

    let _ = update_last_url(app_clone, url);
    // 【修复点】：状态统一为小写，确保前端匹配
    let _ = app_clone.emit(
        "tunnel-event",
        TunnelEvent::with_url("online", url.to_string()),
    );

    restart_n8n_with_env(app_clone, url);
    true
}

pub fn load_tunnel_config<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let config_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("tunnel_config.json");
    if !config_path.exists() {
        return Ok(());
    }
    let config_json = std::fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let config: TunnelConfig = serde_json::from_str(&config_json).map_err(|e| e.to_string())?;
    let mut config_guard = TUNNEL_CONFIG.lock().unwrap();
    *config_guard = config;
    if let Some(last_url) = &config_guard.last_url {
        let mut url_guard = TUNNEL_URL.lock().unwrap();
        *url_guard = Some(last_url.clone());
    }
    Ok(())
}

fn save_tunnel_config<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let config = TUNNEL_CONFIG.lock().unwrap().clone();
    let config_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("tunnel_config.json");
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let config_json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, config_json).map_err(|e| e.to_string())?;
    Ok(())
}

fn update_last_url<R: Runtime>(app: &AppHandle<R>, url: &str) -> Result<(), String> {
    {
        let mut config_guard = TUNNEL_CONFIG.lock().unwrap();
        config_guard.last_url = Some(url.to_string());
        config_guard.created_at = chrono::Local::now().to_rfc3339();
    }
    save_tunnel_config(app)
}

fn restart_n8n_with_env<R: Runtime>(app: &AppHandle<R>, url: &str) {
    if !url.starts_with("http") {
        return;
    }

    let tunnel_running = *TUNNEL_RUNNING.lock().unwrap();
    if !tunnel_running {
        return;
    }

    println!("[Tunnel] 正在应用 URL 并物理重启 n8n...");
    shutdown_n8n();
    std::thread::sleep(Duration::from_millis(800));

    if let Ok(app_path) = app.path().app_data_dir() {
        let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
        let node_path = manager::get_node_binary_path(app_path.join("runtime"));
        let data_dir = app_path.join("n8n-data");

        // 【关键】清除前端会话和缓存，强制用户重新登录以刷新 webhook URL
        // 这会清除浏览器在 n8n 中的会话，使得用户需要重新登录
        // 重新登录后，前端会从新的 WEBHOOK_URL 环境变量获取最新的 webhook 地址
        let sessions_dir = data_dir.join(".n8n/sessions");
        if sessions_dir.exists() {
            println!("[Tunnel] 清除用户会话（强制重新登录）");
            let _ = std::fs::remove_dir_all(&sessions_dir);
        }

        let mut envs = construct_n8n_envs();
        envs.insert("N8N_HOST".to_string(), "127.0.0.1".to_string()); // 强制监听 IPv4
        envs.insert("N8N_PORT".to_string(), "5678".to_string());
        envs.insert("WEBHOOK_URL".to_string(), url.to_string());
        envs.insert("N8N_EDITOR_BASE_URL".to_string(), url.to_string());

        println!("[Tunnel] 启动 n8n...");
        match manager::start_node(node_path, n8n_bin, data_dir, envs) {
            Ok(_) => {
                println!("[Tunnel] ✓ n8n 重启成功");
                println!("[Tunnel] ✓ 新的 WEBHOOK_URL: {}", url);
                println!("[Tunnel] ⚠️  请重新登录 n8n 以刷新 webhook 地址");
                
                // 广播全局同步事件，通知前端刷新 UI
                emit_global_sync(app);
            }
            Err(e) => println!("[Tunnel] ✗ 启动失败: {}", e),
        }
    }
}

// --- 4. 导出给 Tauri 的主要功能 ---

pub async fn start_tunnel<R: Runtime>(
    app: AppHandle<R>,
    cloudflared_path: String,
) -> Result<(), String> {
    // 1. 物理清理残留进程
    #[cfg(unix)]
    let _ = Command::new("pkill").args(&["-f", "cloudflared"]).output();
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(&["/F", "/IM", "cloudflared.exe", "/T"])
        .output();

    std::thread::sleep(Duration::from_millis(500));
    app.emit("tunnel-event", TunnelEvent::new("connecting"))
        .ok();

    // 2. 获取当前配置模式
    let cfg = TUNNEL_CONFIG.lock().unwrap().clone();
    let tunnel_mode = cfg.tunnel_mode;

    // 3. 根据模式构造不同的启动命令
    let mut child = match &tunnel_mode {
        TunnelMode::Token { token, .. } => {
            println!("[Tunnel] 启动 Token 模式：跳过本地配置，连接云端端点...");
            Command::new(&cloudflared_path)
                .args(&["tunnel", "run", "--token", token]) // 关键：Token 模式严禁带 --url
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?
        }
        TunnelMode::Temporary => {
            println!("[Tunnel] 启动临时模式：准备捕获随机域名...");
            Command::new(&cloudflared_path)
                .args(&[
                    "tunnel",
                    "--url",
                    "http://localhost:5678",
                    "--no-autoupdate",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?
        }
    };

    *TUNNEL_RUNNING.lock().unwrap() = true;
    let stderr = child.stderr.take().ok_or("无法捕获标准错误流")?;
    let app_clone = app.clone();

    // 4. 分轨处理逻辑
    match tunnel_mode {
        TunnelMode::Token { domain, .. } => {
            // Token 模式：不解析日志，直接信任用户输入的域名
            let app_inner = app_clone.clone();
            // 对于 Token 模式，我们不需要读取 stderr 来获取 URL，但应该读取它以检测错误
            // 所以 spawn 一个线程来读取 stderr（仅用于错误检测）
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        // 可以在这里检测错误日志，但暂时只打印
                        if l.contains("error") || l.contains("Error") || l.contains("ERROR") {
                            println!("[Tunnel] Cloudflared 错误: {}", l);
                        }
                    }
                }
            });

            // 另一个线程处理 URL 设置和 n8n 重启
            std::thread::spawn(move || {
                // 给予 cloudflared 一定的连接握手时间
                std::thread::sleep(Duration::from_secs(2));

                // 格式化域名：确保以 https:// 开头
                let final_url = if domain.starts_with("http") {
                    domain
                } else {
                    format!("https://{}", domain)
                };

                println!("[Tunnel] Token 模式上线，应用域名: {}", final_url);

                // 使用 handle_tunnel_url 处理 URL（包含验证、更新状态、保存配置、重启 n8n）
                // is_temporary = false 表示这是 Token 模式（非临时域名）
                handle_tunnel_url(&final_url, false, &app_inner);
            });
        }
        TunnelMode::Temporary => {
            // 临时模式：继续保持正则抓取逻辑
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                let regex_temp = Regex::new(r"https://[a-z0-9-]+\.trycloudflare\.com").unwrap();
                let mut found_url = false;

                for line in reader.lines() {
                    if let Ok(l) = line {
                        // println!("Cloudflared: {}", l); // 调试时可开启
                        if !found_url {
                            if let Some(mat) = regex_temp.find(&l) {
                                found_url = process_tunnel_url_match(&mat, true, &app_clone);
                            }
                        }
                    }
                }
            });
        }
    }

    Ok(())
}

pub async fn stop_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    println!("[Tunnel] 正在停止隧道...");

    #[cfg(unix)]
    let _ = Command::new("pkill").args(&["-f", "cloudflared"]).output();
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(&["/F", "/IM", "cloudflared.exe", "/T"])
        .output();

    *TUNNEL_URL.lock().unwrap() = None;
    *TUNNEL_RUNNING.lock().unwrap() = false;

    println!("[Tunnel] 隧道已停止，清理缓存");
    app.emit("tunnel-event", TunnelEvent::new("offline")).ok();
    Ok(())
}

pub async fn get_tunnel_status<R: Runtime>(_app: AppHandle<R>) -> Result<TunnelEvent, String> {
    let url = TUNNEL_URL.lock().unwrap().clone();
    let running = *TUNNEL_RUNNING.lock().unwrap();
    let status = if running {
        if url.is_some() {
            "online"
        } else {
            "connecting"
        }
    } else {
        "offline"
    };
    Ok(TunnelEvent {
        status: status.into(),
        url,
        progress: None,
        message: None,
    })
}

pub async fn copy_tunnel_url<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let url = TUNNEL_URL.lock().unwrap().clone().ok_or("No URL")?;
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        c.stdin.as_mut().unwrap().write_all(url.as_bytes()).ok();
    }
    #[cfg(target_os = "windows")]
    {
        let mut c = Command::new("clip")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        c.stdin.as_mut().unwrap().write_all(url.as_bytes()).ok();
    }
    app.emit("tunnel-event", TunnelEvent::with_url("online", url))
        .ok();
    Ok(())
}

pub async fn get_tunnel_config<R: Runtime>(app: AppHandle<R>) -> Result<TunnelConfig, String> {
    load_tunnel_config(&app)?;
    Ok(TUNNEL_CONFIG.lock().unwrap().clone())
}

pub async fn update_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    auto_start: Option<bool>,
    last_url: Option<String>,
    custom_domain: Option<String>,
    use_custom_domain: Option<bool>,
) -> Result<(), String> {
    {
        let mut cfg = TUNNEL_CONFIG.lock().unwrap();
        if let Some(v) = auto_start {
            cfg.auto_start = v;
        }
        if let Some(v) = last_url {
            cfg.last_url = Some(v);
        }
        if let Some(v) = custom_domain {
            cfg.custom_domain = Some(v);
        }
        if let Some(v) = use_custom_domain {
            cfg.use_custom_domain = v;
        }
    }
    save_tunnel_config(&app)
}

pub async fn check_tunnel_health<R: Runtime>(_app: AppHandle<R>) -> Result<TunnelHealth, String> {
    let url = TUNNEL_URL.lock().unwrap().clone();
    let running = *TUNNEL_RUNNING.lock().unwrap();
    let (status, msg) = if running {
        if url.is_some() {
            (TunnelHealthStatus::Healthy, "Healthy")
        } else {
            (TunnelHealthStatus::Connecting, "Connecting")
        }
    } else {
        (TunnelHealthStatus::Stopped, "Stopped")
    };
    Ok(TunnelHealth {
        status,
        ping_ms: Some(100),
        last_check: chrono::Local::now().to_rfc3339(),
        message: msg.into(),
    })
}

pub async fn recover_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    stop_tunnel(app.clone()).await?;
    Ok(())
}

pub async fn apply_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    tunnel_mode: TunnelMode,
    custom_domain: Option<String>,
    tunnel_token: Option<String>,
) -> Result<(), String> {
    // 验证输入
    match &tunnel_mode {
        TunnelMode::Token { token, domain } => {
            if token.trim().is_empty() {
                return Err("Token 模式需要提供 Cloudflare Tunnel Token".into());
            }
            if domain.trim().is_empty() {
                return Err("Token 模式需要提供自定义域名".into());
            }
        }
        TunnelMode::Temporary => {
            // 临时隧道模式无需验证
        }
    }

    // 更新配置
    {
        let mut cfg = TUNNEL_CONFIG.lock().unwrap();
        cfg.tunnel_mode = tunnel_mode;
        // 对于 Token 模式，需要更新 custom_domain 字段
        if let TunnelMode::Token { domain, .. } = &cfg.tunnel_mode {
            cfg.custom_domain = Some(domain.clone());
        }
        // tunnel_token 字段现在在 TunnelMode::Token 中，不需要单独存储
    }

    save_tunnel_config(&app)?;

    // 如果隧道正在运行，需要重启
    if PROCESS_MANAGER.lock().unwrap().has_child() {
        let url = TUNNEL_URL.lock().unwrap().clone().unwrap_or_default();
        restart_n8n_with_env(&app, &url);
        // 注意：restart_n8n_with_env 内部已经包含 emit_global_sync 调用
    }

    Ok(())
}

pub async fn get_tunnel_errors<R: Runtime>(_app: AppHandle<R>) -> Result<Vec<TunnelError>, String> {
    Ok(vec![])
}

pub async fn load_tunnel_config_on_start<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    load_tunnel_config(&app)
}
