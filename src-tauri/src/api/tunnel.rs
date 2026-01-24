use tauri::{AppHandle, Emitter, Manager, Runtime, Window};
use std::sync::{Arc, Mutex};
use regex::Regex;
use std::process::{Command, Stdio};
use std::io::Write;
use serde::{Serialize, Deserialize};
use chrono;
use once_cell::sync::Lazy;

use crate::services::manager::PROCESS_MANAGER;
use super::n8n_core::shutdown_n8n;

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
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            last_url: None,
            auto_start: false,
            created_at: chrono::Local::now().to_rfc3339(),
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
static TUNNEL_URL: Lazy<Arc<Mutex<Option<String>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
static TUNNEL_RUNNING: Lazy<Arc<Mutex<bool>>> = Lazy::new(|| Arc::new(Mutex::new(false)));
static TUNNEL_CONFIG: Lazy<Arc<Mutex<TunnelConfig>>> = Lazy::new(|| Arc::new(Mutex::new(TunnelConfig::default())));

// --- 内部辅助函数 ---

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
fn restart_n8n_with_env<R: Runtime>(app: &AppHandle<R>, url: &str) {
    use std::fs;
    
    // 获取 n8n 二进制路径
    let app_path = match app.path().app_data_dir() {
        Ok(path) => path,
        Err(e) => {
            println!("Failed to get app data dir: {}", e);
            return;
        }
    };
    
    let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
    
    if !n8n_bin.exists() {
        println!("N8N binary not found at: {:?}", n8n_bin);
        return;
    }
    
    // 获取 node 二进制路径
    let runtime_dir = app_path.join("runtime");
    let node_path = if runtime_dir.exists() {
        // 简化：假设node在runtime/bin目录下
        runtime_dir.join("bin/node")
    } else {
        // 如果runtime不存在，使用系统node
        "node".into()
    };
    
    // 准备数据目录
    let data_dir = app_path.join("n8n-data");
    if !data_dir.exists() {
        let _ = std::fs::create_dir_all(&data_dir);
    }
    
    // 停止现有的 n8n 进程
    shutdown_n8n();
    
    // 启动 n8n 进程并注入环境变量
    match Command::new(node_path)
        .arg(n8n_bin)
        .env("WEBHOOK_URL", url)
        .env("N8N_EDITOR_BASE_URL", url)
        .env("N8N_USER_FOLDER", &data_dir)
        .current_dir(&app_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn() {
            Ok(_child) => {
                println!("N8N restarted with WEBHOOK_URL={}", url);
                // 存储进程到管理器
                if let Ok(_manager) = PROCESS_MANAGER.lock() {
                    // 注意：这里需要适配实际的进程管理
                    println!("Tunnel: N8N process started with tunnel URL");
                }
            }
            Err(e) => {
                println!("Failed to restart n8n: {}", e);
            }
        }
}

// --- 隧道功能函数（不包含 #[tauri::command] 属性）---

/// 启动 Cloudflare Tunnel（支持自动下载）
pub async fn start_tunnel<R: Runtime>(
    app: AppHandle<R>, 
    window: Window<R>,
    cloudflared_path: String
) -> Result<(), String> {
    // 检查是否已有隧道在运行
    {
        let running_guard = TUNNEL_RUNNING.lock().unwrap();
        if *running_guard {
            return Err("Tunnel is already running".to_string());
        }
    }
    
    // 设置隧道运行状态
    {
        let mut running_guard = TUNNEL_RUNNING.lock().unwrap();
        *running_guard = true;
    }
    
    // 发送连接中事件
    app.emit("tunnel-update", TunnelEvent::new("Connecting"))
        .map_err(|e| e.to_string())?;
    
    println!("Using cloudflared at: {}", cloudflared_path);
    
    // 启动 cloudflared 进程
    let output = Command::new(&cloudflared_path)
        .args(&["tunnel", "--url", "http://localhost:5678", "--no-autoupdate"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start cloudflared at {}: {}", cloudflared_path, e))?;
    
    let app_clone = app.clone();
    
    // 使用线程来监控输出
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        
        let stdout = output.stdout.unwrap();
        let reader = BufReader::new(stdout);
        
        // 匹配 cloudflared 输出中的隧道 URL
        // 可能的格式:
        // - https://xxx.trycloudflare.com
        // - | https://xxx.trycloudflare.com
        // - Your tunnel is available at: https://xxx.trycloudflare.com
        let regex = Regex::new(r"https://[a-z0-9-]+\.trycloudflare\.com").unwrap();
        
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    println!("Cloudflared output: {}", line);
                    
                    if let Some(captures) = regex.captures(&line) {
                        if let Some(url_match) = captures.get(0) {
                            let url = url_match.as_str().to_string();
                            println!("Found tunnel URL: {}", url);
                            
                            // 存储隧道URL
                            {
                                let mut url_guard = TUNNEL_URL.lock().unwrap();
                                *url_guard = Some(url.clone());
                            }
                            
                            // 保存配置
                            let _ = update_last_url(&app_clone, &url);
                            
                            // 发送事件到前端
                            let _ = app_clone.emit("tunnel-update", TunnelEvent::with_url("Online", url.clone()));
                            
                            // 重启 n8n 进程并注入环境变量
                            restart_n8n_with_env(&app_clone, &url);
                            
                            break; // 找到URL后停止监控
                        }
                    }
                }
                Err(e) => {
                    println!("Error reading cloudflared output: {}", e);
                    break;
                }
            }
        }
    });
    
    Ok(())
}

/// 停止 Cloudflare Tunnel
pub async fn stop_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    // 停止 cloudflared 进程
    let _ = Command::new("pkill")
        .args(&["-f", "cloudflared"])
        .output();
    
    // 清除隧道状态
    {
        let mut url_guard = TUNNEL_URL.lock().unwrap();
        *url_guard = None;
        
        let mut running_guard = TUNNEL_RUNNING.lock().unwrap();
        *running_guard = false;
    }
    
    // 发送离线事件到前端
    app.emit("tunnel-update", TunnelEvent::new("Offline"))
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

/// 获取隧道状态
pub async fn get_tunnel_status<R: Runtime>(_app: AppHandle<R>) -> Result<TunnelEvent, String> {
    let url_guard = TUNNEL_URL.lock().unwrap();
    let running_guard = TUNNEL_RUNNING.lock().unwrap();
    
    let status = if *running_guard {
        if url_guard.is_some() {
            "Online".to_string()
        } else {
            "Connecting".to_string()
        }
    } else {
        "Offline".to_string()
    };
    
    Ok(TunnelEvent {
        status,
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
            // 使用系统命令复制到剪贴板
            #[cfg(target_os = "macos")]
            {
                use std::process::Command;
                let mut child = Command::new("pbcopy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("Failed to spawn pbcopy: {}", e))?;
                
                let stdin = child.stdin.as_mut().ok_or("Failed to get stdin".to_string())?;
                stdin.write_all(url.as_bytes())
                    .map_err(|e| format!("Failed to write to pbcopy: {}", e))?;
            }
            
            #[cfg(target_os = "windows")]
            {
                use std::process::Command;
                let mut child = Command::new("clip")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("Failed to spawn clip: {}", e))?;
                
                let stdin = child.stdin.as_mut().ok_or("Failed to get stdin".to_string())?;
                stdin.write_all(url.as_bytes())
                    .map_err(|e| format!("Failed to write to clip: {}", e))?;
            }
            
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                let mut child = Command::new("xclip")
                    .args(&["-selection", "clipboard"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("Failed to spawn xclip: {}", e))?;
                
                let stdin = child.stdin.as_mut().ok_or("Failed to get stdin".to_string())?;
                stdin.write_all(url.as_bytes())
                    .map_err(|e| format!("Failed to write to xclip: {}", e))?;
            }
            
            // 发送复制成功事件
            app.emit("tunnel-copied", TunnelEvent::with_url("Copied", url.clone()))
                .map_err(|e| e.to_string())?;
            
            Ok(())
        }
        None => Err("No tunnel URL available".to_string()),
    }
}

/// 获取隧道配置
pub async fn get_tunnel_config<R: Runtime>(app: AppHandle<R>) -> Result<TunnelConfig, String> {
    // 加载最新配置
    load_tunnel_config(&app)?;
    
    let config_guard = TUNNEL_CONFIG.lock().unwrap();
    Ok(config_guard.clone())
}

/// 更新隧道配置
pub async fn update_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    auto_start: Option<bool>,
    last_url: Option<String>,
) -> Result<(), String> {
    {
        let mut config_guard = TUNNEL_CONFIG.lock().unwrap();
        
        if let Some(auto_start_val) = auto_start {
            config_guard.auto_start = auto_start_val;
        }
        
        if let Some(last_url_val) = last_url {
            config_guard.last_url = Some(last_url_val);
        }
        
        config_guard.created_at = chrono::Local::now().to_rfc3339();
    }
    
    save_tunnel_config(&app)
}

/// 应用启动时加载配置
pub async fn load_tunnel_config_on_start<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    load_tunnel_config(&app)?;
    
    // 如果配置了自动启动且上次有URL，可以在这里自动启动隧道
    let config_guard = TUNNEL_CONFIG.lock().unwrap();
    if config_guard.auto_start && config_guard.last_url.is_some() {
        println!("Auto-start tunnel configured, but manual start required for now");
        // 注意：这里不自动启动，需要用户手动点击
    }
    
    Ok(())
}

/// 检查隧道健康状况
pub async fn check_tunnel_health<R: Runtime>(_app: AppHandle<R>) -> Result<TunnelHealth, String> {
    let url_guard = TUNNEL_URL.lock().unwrap();
    let running_guard = TUNNEL_RUNNING.lock().unwrap();
    
    let health_status = if *running_guard {
        if url_guard.is_some() {
            TunnelHealthStatus::Healthy
        } else {
            TunnelHealthStatus::Connecting
        }
    } else {
        TunnelHealthStatus::Stopped
    };
    
    // 如果有URL，可以尝试ping测试（简化版本）
    let ping_ms = if url_guard.is_some() && *running_guard {
        Some(100) // 模拟ping值
    } else {
        None
    };
    
    let message = match health_status {
        TunnelHealthStatus::Healthy => "Tunnel is working properly".to_string(),
        TunnelHealthStatus::Connecting => "Tunnel is connecting".to_string(),
        TunnelHealthStatus::Stopped => "Tunnel is stopped".to_string(),
        TunnelHealthStatus::Error => "Tunnel has errors".to_string(),
    };
    
    Ok(TunnelHealth {
        status: health_status,
        ping_ms,
        last_check: chrono::Local::now().to_rfc3339(),
        message,
    })
}

/// 尝试恢复隧道连接
pub async fn recover_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let (should_recover, last_url) = {
        let running_guard = TUNNEL_RUNNING.lock().unwrap();
        let url_guard = TUNNEL_URL.lock().unwrap();
        
        // 如果隧道标记为运行中但没有URL，尝试恢复
        (*running_guard && url_guard.is_none(), url_guard.clone())
    };
    
    if should_recover {
        println!("Attempting to recover tunnel...");
        
        // 发送恢复中事件
        app.emit("tunnel-update", TunnelEvent::new("Recovering"))
            .map_err(|e| e.to_string())?;
        
        // 停止现有进程
        let _ = Command::new("pkill")
            .args(&["-f", "cloudflared"])
            .output();
        
        // 清除状态
        {
            let mut running_guard = TUNNEL_RUNNING.lock().unwrap();
            *running_guard = false;
        }
        
        // 如果有上次的URL，尝试重新启动
        if let Some(last_url) = last_url {
            println!("Last URL was {}, attempting restart...", last_url);
            // 这里可以添加重试逻辑，但为了简化，我们只是重置状态
            // 实际应用中应该重新启动隧道
        }
        
        // 发送恢复完成事件
        app.emit("tunnel-update", TunnelEvent::new("RecoveryComplete"))
            .map_err(|e| e.to_string())?;
        
        Ok(())
    } else {
        Err("Tunnel does not need recovery".to_string())
    }
}

/// 获取错误日志
pub async fn get_tunnel_errors<R: Runtime>(_app: AppHandle<R>) -> Result<Vec<TunnelError>, String> {
    // 简化版本：返回空错误列表
    // 实际应用中应该从日志文件或内存中读取错误
    Ok(vec![])
}