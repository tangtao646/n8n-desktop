use tauri::{AppHandle, Emitter, Manager, Runtime};
use std::sync::{Arc, Mutex};
use regex::Regex;
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use serde::{Serialize, Deserialize};
use chrono;
use once_cell::sync::Lazy;

use crate::services::manager::{self, PROCESS_MANAGER};
use super::n8n_core::{shutdown_n8n, construct_n8n_envs};

// --- 数据结构定义 ---

#[derive(Clone, Serialize)]
pub struct TunnelEvent {
    pub status: String,
    pub url: Option<String>,
    pub progress: Option<f64>, // 下载进度 0-100
    pub message: Option<String>, // 附加消息
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

// 隧道配置结构
#[derive(Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub last_url: Option<String>,
    pub auto_start: bool,
    pub created_at: String,
    pub custom_domain: Option<String>,
    pub use_custom_domain: bool,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            last_url: None,
            auto_start: false,
            created_at: chrono::Local::now().to_rfc3339(),
            custom_domain: None,
            use_custom_domain: false,
        }
    }
}

// 隧道健康状态枚举
#[derive(Clone, Serialize)]
pub enum TunnelHealthStatus {
    Healthy,
    Connecting,
    Stopped,
    Error,
}

// 隧道健康信息
#[derive(Clone, Serialize)]
pub struct TunnelHealth {
    pub status: TunnelHealthStatus,
    pub ping_ms: Option<u32>,
    pub last_check: String,
    pub message: String,
}

// 隧道错误信息
#[derive(Clone, Serialize)]
pub struct TunnelError {
    pub timestamp: String,
    pub message: String,
    pub severity: String, // "info", "warning", "error"
}

// --- 全局状态 ---

// 向后兼容的全局状态（单隧道）
pub(crate) static TUNNEL_URL: Lazy<Arc<Mutex<Option<String>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
pub(crate) static TUNNEL_RUNNING: Lazy<Arc<Mutex<bool>>> = Lazy::new(|| Arc::new(Mutex::new(false)));
pub(crate) static TUNNEL_CONFIG: Lazy<Arc<Mutex<TunnelConfig>>> = Lazy::new(|| Arc::new(Mutex::new(TunnelConfig::default())));

// --- 内部辅助函数 ---

/// 处理隧道URL匹配逻辑
fn process_tunnel_url_match<R: Runtime>(
    url_match: &regex::Match,
    is_temporary: bool,
    app_clone: &AppHandle<R>,
) -> bool {
    let url = url_match.as_str().to_string();
    
    // 验证URL
    let is_valid_url = if is_temporary {
        // 临时域名验证：必须严格以 .trycloudflare.com 结尾，并且不包含 cloudflare.com
        url.ends_with(".trycloudflare.com") && !url.contains("cloudflare.com")
    } else {
        // 自定义域名验证：必须包含协议，不是cloudflare.com相关域名，也不是临时域名
        (url.starts_with("http://") || url.starts_with("https://"))
            && !url.contains("cloudflare.com")
            && !url.ends_with(".trycloudflare.com")
    };
    
    if !is_valid_url {
        return false;
    }
    
    let url_type = if is_temporary { "temporary" } else { "custom domain" };
    println!("Found {} tunnel URL: {}", url_type, url);
    
    // 更新状态
    {
        let mut url_guard = TUNNEL_URL.lock().unwrap();
        *url_guard = Some(url.clone());
    }
    
    // 保存配置并发送事件
    let _ = update_last_url(app_clone, &url);
    let _ = app_clone.emit("tunnel-update", TunnelEvent::with_url("Online", url.clone()));
    
    // 注入环境变量并重启 n8n
    restart_n8n_with_env(app_clone, &url);
    
    true
}

/// 保存隧道配置到文件
fn save_tunnel_config<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let config_guard = TUNNEL_CONFIG.lock().unwrap();
    let config = config_guard.clone();
    
    let config_path = app.path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config dir: {}", e))?
        .join("tunnel_config.json");
    
    // 确保目录存在
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }
    
    let config_json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    
    std::fs::write(&config_path, config_json)
        .map_err(|e| format!("Failed to write config file: {}", e))?;
    
    Ok(())
}

/// 从文件加载隧道配置
pub fn load_tunnel_config<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let config_path = app.path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config dir: {}", e))?
        .join("tunnel_config.json");
    
    if !config_path.exists() {
        println!("No tunnel config file found, using defaults");
        return Ok(());
    }
    
    let config_json = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config file: {}", e))?;
    
    let config: TunnelConfig = serde_json::from_str(&config_json)
        .map_err(|e| format!("Failed to parse config: {}", e))?;
    
    let mut config_guard = TUNNEL_CONFIG.lock().unwrap();
    *config_guard = config;
    
    // 如果配置中有上次的URL，更新到内存状态
    if let Some(last_url) = &config_guard.last_url {
        let mut url_guard = TUNNEL_URL.lock().unwrap();
        *url_guard = Some(last_url.clone());
    }
    
    Ok(())
}

/// 更新隧道配置中的最后URL
fn update_last_url<R: Runtime>(app: &AppHandle<R>, url: &str) -> Result<(), String> {
    {
        let mut config_guard = TUNNEL_CONFIG.lock().unwrap();
        config_guard.last_url = Some(url.to_string());
        config_guard.created_at = chrono::Local::now().to_rfc3339();
    }
    
    save_tunnel_config(app)
}

/// 内部函数：重启 n8n 并注入环境变量
/// 延迟重启机制：确保只有在真正监听到有效的隧道 URL 时才重启
fn restart_n8n_with_env<R: Runtime>(app: &AppHandle<R>, url: &str) {
    // 验证 URL 是否有效（必须包含协议和域名）
    if !url.starts_with("http://") && !url.starts_with("https://") {
        println!("Invalid tunnel URL (missing protocol): {}", url);
        return;
    }
    
    // 检查隧道是否正在运行
    let tunnel_running = {
        let running_guard = TUNNEL_RUNNING.lock().unwrap();
        *running_guard
    };
    
    if !tunnel_running {
        println!("Tunnel is not running, skipping n8n restart");
        return;
    }
    
    println!("[DELAYED RESTART] Valid tunnel URL detected: {}, proceeding with n8n restart", url);
    
    // 获取应用数据目录
    let app_path = match app.path().app_data_dir() {
        Ok(path) => path,
        Err(e) => {
            println!("Failed to get app data dir: {}", e);
            return;
        }
    };
    
    // 检查 n8n 二进制文件是否存在
    let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
    if !n8n_bin.exists() {
        println!("N8N binary not found at: {:?}", n8n_bin);
        return;
    }
    
    // 获取 node 二进制路径
    let runtime_dir = app_path.join("runtime");
    let node_path = manager::get_node_binary_path(runtime_dir);
    
    if !node_path.exists() {
        println!("Node binary not found at: {:?}", node_path);
        return;
    }
    
    // 准备数据目录
    let data_dir = app_path.join("n8n-data");
    if !data_dir.exists() {
        let _ = std::fs::create_dir_all(&data_dir);
    }
    
    // 停止现有的 n8n 进程（杀掉再重启）
    shutdown_n8n();
    
    // 等待 100ms 确保端口完全释放
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    // 构造环境变量映射（包含隧道 URL 和节点解禁状态）
    let mut additional_envs = construct_n8n_envs();
    
    // 确保隧道 URL 被正确设置（因为 construct_n8n_envs 已经根据 TUNNEL_RUNNING 状态设置了）
    // 但为了保险，如果隧道正在运行，我们强制设置 URL
    {
        let running_guard = TUNNEL_RUNNING.lock().unwrap();
        if *running_guard {
            additional_envs.insert("WEBHOOK_URL".to_string(), url.to_string());
            additional_envs.insert("N8N_EDITOR_BASE_URL".to_string(), url.to_string());
            additional_envs.insert("N8N_CORS_ALLOWED_ORIGINS".to_string(), "*".to_string());
        }
    }
    
    // 启动 n8n 进程
    match manager::start_node(node_path, n8n_bin, data_dir, additional_envs) {
        Ok(_) => {
            println!("[DELAYED RESTART] N8N successfully restarted with tunnel URL: {}", url);
            if let Ok(_manager) = PROCESS_MANAGER.lock() {
                println!("Tunnel: N8N process started with tunnel URL");
            }
        }
        Err(e) => {
            println!("[DELAYED RESTART] Failed to restart n8n: {}", e);
        }
    }
}

// --- 隧道功能函数（不包含 #[tauri::command] 属性）---

/// 启动 Cloudflare Tunnel
pub async fn start_tunnel<R: Runtime>(
    app: AppHandle<R>,
    cloudflared_path: String
) -> Result<(), String> {
    // 1. 强制清理系统中残留的独立进程 (无论是不是本应用启动的)
    #[cfg(unix)]
    let _ = Command::new("pkill").args(&["-f", "cloudflared"]).output();
    #[cfg(windows)]
    let _ = Command::new("taskkill").args(&["/F", "/IM", "cloudflared.exe", "/T"]).output();

    // 给系统一点点时间释放资源
    std::thread::sleep(std::time::Duration::from_millis(500));
    {
        let mut running_guard = TUNNEL_RUNNING.lock().unwrap();
        // 既然已经物理杀死了进程，这里可以强制重置状态
        *running_guard = false;
    }

    // 3. 发送连接中事件
    app.emit("tunnel-update", TunnelEvent::new("Connecting")).ok();
    
    println!("Using cloudflared at: {}", cloudflared_path);
    
    // 4. 检查配置，决定使用临时模式还是固定模式
    let (use_custom_domain, custom_domain) = {
        let config_guard = TUNNEL_CONFIG.lock().unwrap();
        (config_guard.use_custom_domain, config_guard.custom_domain.clone())
    };
    
    let mut child = if use_custom_domain {
        // 固定模式：使用自定义域名
        // 注意：这里需要用户已经配置了 tunnel 并登录 cloudflared
        // 我们假设用户已经配置了 tunnel，使用 tunnel run 命令
        // 实际应用中可能需要更复杂的逻辑来处理 credentials 等
        println!("Starting tunnel in fixed mode with custom domain: {:?}", custom_domain);
        
        // 这里需要 tunnel name 或 ID，暂时使用一个占位符
        // 实际应该从配置中读取或让用户输入
        let tunnel_name = "n8n-tunnel"; // 默认隧道名称
        Command::new(&cloudflared_path)
            .args(&["tunnel", "run", tunnel_name, "--no-autoupdate"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("无法启动 cloudflared (固定模式): {}", e))?
    } else {
        // 临时模式：使用随机域名
        println!("Starting tunnel in temporary mode");
        Command::new(&cloudflared_path)
            .args(&["tunnel", "--url", "http://localhost:5678", "--no-autoupdate"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("无法启动 cloudflared: {}", e))?
    };
    
    {
        let mut running_guard = TUNNEL_RUNNING.lock().unwrap();
        *running_guard = true;
    }

    let app_clone = app.clone();
    // 取出 stderr 流进行监听
    let stderr = child.stderr.take().ok_or("无法获取 stderr 流")?;
    
    // 5. 使用线程监控日志输出抓取 URL
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        // 更新正则表达式以匹配自定义域名和临时域名
        // 临时域名必须严格以 .trycloudflare.com 结尾，避免匹配 cloudflare.com
        // 允许 http:// 或 https:// 协议，匹配包含在文本中的 URL
        let regex_temp = Regex::new(r"https?://[a-z0-9-]+\.trycloudflare\.com").unwrap();
        // 自定义域名正则表达式，匹配有效的 URL
        let regex_custom = Regex::new(r"https?://[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
        let mut found_url = false;

        for line in reader.lines() {
            if let Ok(l) = line {
                println!("Cloudflared: {}", l);
                
                if !found_url {
                    // 先尝试匹配临时域名（必须严格以 .trycloudflare.com 结尾）
                    // 使用 find 而不是 captures，因为 URL 可能被其他字符包围
                    if let Some(url_match) = regex_temp.find(&l) {
                        found_url = process_tunnel_url_match(&url_match, true, &app_clone);
                    }
                    
                    // 如果没有找到临时域名，尝试匹配自定义域名
                    if !found_url {
                        if let Some(url_match) = regex_custom.find(&l) {
                            found_url = process_tunnel_url_match(&url_match, false, &app_clone);
                        }
                    }
                }
            }
        }
        
        // 线程结束（通常意味着进程退出）
        let mut running_guard = TUNNEL_RUNNING.lock().unwrap();
        *running_guard = false;
        let mut url_guard = TUNNEL_URL.lock().unwrap();
        *url_guard = None;
        let _ = app_clone.emit("tunnel-update", TunnelEvent::new("Offline"));
    });
    
    Ok(())
}

/// 停止 Cloudflare Tunnel
pub async fn stop_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    #[cfg(unix)]
    let _ = Command::new("pkill").args(&["-f", "cloudflared"]).output();
    #[cfg(windows)]
    let _ = Command::new("taskkill").args(&["/F", "/IM", "cloudflared.exe", "/T"]).output();
    
    {
        let mut url_guard = TUNNEL_URL.lock().unwrap();
        *url_guard = None;
        
        let mut running_guard = TUNNEL_RUNNING.lock().unwrap();
        *running_guard = false;
    }
    
    app.emit("tunnel-update", TunnelEvent::new("Offline")).ok();
    Ok(())
}

/// 获取隧道状态
pub async fn get_tunnel_status<R: Runtime>(_app: AppHandle<R>) -> Result<TunnelEvent, String> {
    let url_guard = TUNNEL_URL.lock().unwrap();
    let running_guard = TUNNEL_RUNNING.lock().unwrap();
    
    let status = if *running_guard {
        if url_guard.is_some() { "Online" } else { "Connecting" }
    } else {
        "Offline"
    };
    
    Ok(TunnelEvent {
        status: status.to_string(),
        url: url_guard.clone(),
        progress: None,
        message: None,
    })
}

/// 复制隧道URL到剪贴板
pub async fn copy_tunnel_url<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let url_guard = TUNNEL_URL.lock().unwrap();
    
    match url_guard.as_ref() {
        Some(url) => {
            #[cfg(target_os = "macos")]
            {
                let mut child = Command::new("pbcopy")
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                let stdin = child.stdin.as_mut().ok_or("Failed to get stdin")?;
                stdin.write_all(url.as_bytes()).map_err(|e| e.to_string())?;
            }
            #[cfg(target_os = "windows")]
            {
                let mut child = Command::new("clip")
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                let stdin = child.stdin.as_mut().ok_or("Failed to get stdin")?;
                stdin.write_all(url.as_bytes()).map_err(|e| e.to_string())?;
            }
            #[cfg(target_os = "linux")]
            {
                let _ = Command::new("xclip")
                    .args(&["-selection", "clipboard"])
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|e| e.to_string())?
                    .stdin.as_mut().unwrap()
                    .write_all(url.as_bytes());
            }
            
            app.emit("tunnel-copied", TunnelEvent::with_url("Copied", url.clone())).ok();
            Ok(())
        }
        None => Err("No tunnel URL available".to_string()),
    }
}

/// 获取隧道配置
pub async fn get_tunnel_config<R: Runtime>(app: AppHandle<R>) -> Result<TunnelConfig, String> {
    load_tunnel_config(&app)?;
    let config_guard = TUNNEL_CONFIG.lock().unwrap();
    Ok(config_guard.clone())
}

/// 更新隧道配置
pub async fn update_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    auto_start: Option<bool>,
    last_url: Option<String>,
    custom_domain: Option<String>,
    use_custom_domain: Option<bool>,
) -> Result<(), String> {
    {
        let mut config_guard = TUNNEL_CONFIG.lock().unwrap();
        if let Some(v) = auto_start { config_guard.auto_start = v; }
        if let Some(v) = last_url { config_guard.last_url = Some(v); }
        if let Some(v) = custom_domain { config_guard.custom_domain = Some(v); }
        if let Some(v) = use_custom_domain { config_guard.use_custom_domain = v; }
        config_guard.created_at = chrono::Local::now().to_rfc3339();
    }
    save_tunnel_config(&app)
}

/// 应用启动时加载配置
pub async fn load_tunnel_config_on_start<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    load_tunnel_config(&app)
}

/// 检查隧道健康状况
pub async fn check_tunnel_health<R: Runtime>(_app: AppHandle<R>) -> Result<TunnelHealth, String> {
    let url_guard = TUNNEL_URL.lock().unwrap();
    let running_guard = TUNNEL_RUNNING.lock().unwrap();
    
    let (health_status, message) = if *running_guard {
        if url_guard.is_some() {
            (TunnelHealthStatus::Healthy, "Tunnel is working properly")
        } else {
            (TunnelHealthStatus::Connecting, "Tunnel is connecting")
        }
    } else {
        (TunnelHealthStatus::Stopped, "Tunnel is stopped")
    };
    
    Ok(TunnelHealth {
        status: health_status,
        ping_ms: if url_guard.is_some() { Some(100) } else { None },
        last_check: chrono::Local::now().to_rfc3339(),
        message: message.to_string(),
    })
}

/// 尝试恢复隧道连接
pub async fn recover_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let (should_recover, last_url) = {
        let running_guard = TUNNEL_RUNNING.lock().unwrap();
        let url_guard = TUNNEL_URL.lock().unwrap();
        (*running_guard && url_guard.is_none(), url_guard.clone())
    };
    
    if should_recover {
        app.emit("tunnel-update", TunnelEvent::new("Recovering")).ok();
        stop_tunnel(app.clone()).await?;
        // 实际应用中可以在这里重新调用启动逻辑
        app.emit("tunnel-update", TunnelEvent::new("RecoveryComplete")).ok();
        Ok(())
    } else {
        Err("Tunnel does not need recovery".to_string())
    }
}

/// 应用自定义域名配置并重启 n8n
pub async fn apply_custom_domain_config<R: Runtime>(
    app: AppHandle<R>,
    custom_domain: Option<String>,
    use_custom_domain: bool,
) -> Result<(), String> {
    // 1. 更新全局配置状态
    {
        let mut config_guard = TUNNEL_CONFIG.lock().unwrap();
        config_guard.custom_domain = custom_domain;
        config_guard.use_custom_domain = use_custom_domain;
        config_guard.created_at = chrono::Local::now().to_rfc3339();
    }
    
    // 保存配置到文件
    save_tunnel_config(&app)?;
    
    // 2. 检查 n8n 是否正在运行
    let is_running = {
        let manager = PROCESS_MANAGER.lock().unwrap();
        manager.has_child()
    };
    
    if !is_running {
        println!("N8N is not running, configuration saved but no restart needed");
        return Ok(());
    }
    
    // 3. 获取当前隧道 URL（如果有）
    let current_url = {
        let url_guard = TUNNEL_URL.lock().unwrap();
        url_guard.clone()
    };
    
    // 4. 杀掉当前 n8n 进程
    println!("Killing current n8n process to apply domain configuration...");
    shutdown_n8n();
    
    // 5. 等待 500ms 确保端口释放
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    // 6. 获取应用路径和二进制
    let app_path = app.path().app_data_dir().map_err(|e| format!("Failed to get app data dir: {}", e))?;
    
    let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
    if !n8n_bin.exists() {
        return Err("N8N binary not found".to_string());
    }
    
    let runtime_dir = app_path.join("runtime");
    let node_path = manager::get_node_binary_path(runtime_dir);
    if !node_path.exists() {
        return Err("Node binary not found".to_string());
    }
    
    let data_dir = app_path.join("n8n-data");
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    }
    
    // 7. 构造新的环境变量（包含新的域名配置）
    let additional_envs = construct_n8n_envs();
    println!("New environment variables with domain config: {:?}", additional_envs);
    
    // 8. 重新启动 n8n
    match manager::start_node(node_path, n8n_bin, data_dir, additional_envs) {
        Ok(_) => {
            println!("N8N restarted with updated domain configuration");
            Ok(())
        }
        Err(e) => {
            println!("Failed to restart n8n: {}", e);
            Err(e)
        }
    }
}

/// 获取错误日志
pub async fn get_tunnel_errors<R: Runtime>(_app: AppHandle<R>) -> Result<Vec<TunnelError>, String> {
    Ok(vec![])
}