use tauri::{AppHandle, Manager, Runtime, Window};
use std::process::Command;
use regex::Regex;
use which;
use serde::{Serialize, Deserialize};
use chrono;

use crate::services::downloader;

// --- 数据结构定义 ---

/// Cloudflared 缓存信息
#[derive(Clone, Serialize, Deserialize)]
pub struct CloudflaredCacheInfo {
    pub filename: String,
    pub downloaded_at: String,
    pub platform: String,
    pub version: String,
}

/// Cloudflared 版本信息
#[derive(Clone, Serialize)]
pub struct CloudflaredVersionInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub cached: bool,
    pub cache_age_days: Option<i64>,
}

// --- 内部辅助函数 ---

/// 获取平台字符串
fn get_platform_string() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        "unknown".to_string()
    }
}

/// 获取 cloudflared 二进制路径
/// 优先尝试从应用资源中加载，如果找不到则使用系统 PATH
pub fn get_cloudflared_path<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    // 首先尝试从应用资源中查找
    let resource_path = app.path()
        .resource_dir()
        .map_err(|e| format!("Failed to get resource dir: {}", e))?
        .join("cloudflared");
    
    if resource_path.exists() {
        return Ok(resource_path.to_string_lossy().to_string());
    }
    
    // 如果资源中不存在，尝试系统 PATH
    match which::which("cloudflared") {
        Ok(path) => Ok(path.to_string_lossy().to_string()),
        Err(_) => Err("cloudflared not found in PATH and not bundled with application".to_string()),
    }
}

/// 获取 cloudflared 二进制路径（支持自动下载）
/// 如果本地不存在，会自动下载对应平台的 cloudflared
pub async fn get_or_download_cloudflared<R: Runtime>(
    app: &AppHandle<R>, 
    window: Option<Window<R>>
) -> Result<String, String> {
    // 首先尝试从应用资源中查找
    let resource_path = app.path()
        .resource_dir()
        .map_err(|e| format!("Failed to get resource dir: {}", e))?
        .join("cloudflared");
    
    if resource_path.exists() {
        return Ok(resource_path.to_string_lossy().to_string());
    }
    
    // 如果资源中不存在，尝试系统 PATH
    match which::which("cloudflared") {
        Ok(path) => return Ok(path.to_string_lossy().to_string()),
        Err(_) => {
            // 系统PATH中也没有，需要自动下载
            println!("cloudflared not found, starting automatic download...");
        }
    }
    
    // 如果没有提供窗口，无法显示下载进度
    let window = match window {
        Some(w) => w,
        None => return Err("Window handle required for downloading cloudflared".to_string()),
    };
    
    // 检查缓存中是否有可用的cloudflared
    let app_data_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    
    let download_dir = app_data_dir.join("cloudflared");
    let cache_info_path = download_dir.join("cache_info.json");
    
    // 读取缓存信息
    let mut need_download = true;
    let mut cached_path = None;
    
    if download_dir.exists() && cache_info_path.exists() {
        if let Ok(cache_info_json) = std::fs::read_to_string(&cache_info_path) {
            if let Ok(cache_info) = serde_json::from_str::<CloudflaredCacheInfo>(&cache_info_json) {
                // 检查缓存是否过期（30天）
                let cache_age = chrono::Utc::now().signed_duration_since(
                    chrono::DateTime::parse_from_rfc3339(&cache_info.downloaded_at)
                        .unwrap_or_else(|_| chrono::Utc::now().into())
                );
                
                if cache_age.num_days() < 30 {
                    // 缓存有效，检查文件是否存在
                    let candidate_path = download_dir.join(&cache_info.filename);
                    if candidate_path.exists() {
                        println!("Using cached cloudflared from: {:?}", candidate_path);
                        cached_path = Some(candidate_path);
                        need_download = false;
                    }
                }
            }
        }
    }
    
    if !need_download {
        return Ok(cached_path.unwrap().to_string_lossy().to_string());
    }
    
    // 根据平台确定下载URL
    let (url, dest_filename) = if cfg!(target_os = "windows") {
        (
            "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe".to_string(),
            "cloudflared.exe".to_string()
        )
    } else if cfg!(target_os = "macos") {
        (
            "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-darwin-amd64.tgz".to_string(),
            "cloudflared".to_string()
        )
    } else if cfg!(target_os = "linux") {
        (
            "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64".to_string(),
            "cloudflared".to_string()
        )
    } else {
        return Err("Unsupported platform for cloudflared download".to_string());
    };
    
    // 使用代理下载（提高成功率）
    let proxy_prefix = "https://gh-proxy.com/";
    let proxy_url = format!("{}{}", proxy_prefix, url);
    
    // 确保下载目录存在
    if !download_dir.exists() {
        std::fs::create_dir_all(&download_dir)
            .map_err(|e| format!("Failed to create download directory: {}", e))?;
    }
    
    let dest_path = download_dir.join(&dest_filename);
    
    println!("Downloading cloudflared from: {}", proxy_url);
    println!("Destination: {:?}", dest_path);
    
    // 下载文件
    downloader::download_file(window.clone(), proxy_url, dest_path.clone(), "cloudflared".to_string()).await?;
    
    // 设置执行权限（Unix系统）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?
            .permissions();
        perms.set_mode(0o755); // rwxr-xr-x
        std::fs::set_permissions(&dest_path, perms)
            .map_err(|e| format!("Failed to set executable permissions: {}", e))?;
    }
    
    // 保存缓存信息
    let cache_info = CloudflaredCacheInfo {
        filename: dest_filename.clone(),
        downloaded_at: chrono::Utc::now().to_rfc3339(),
        platform: get_platform_string(),
        version: "latest".to_string(), // 可以尝试获取实际版本
    };
    
    let cache_info_json = serde_json::to_string_pretty(&cache_info)
        .map_err(|e| format!("Failed to serialize cache info: {}", e))?;
    
    std::fs::write(&cache_info_path, cache_info_json)
        .map_err(|e| format!("Failed to write cache info: {}", e))?;
    
    println!("cloudflared downloaded successfully to: {:?}", dest_path);
    
    Ok(dest_path.to_string_lossy().to_string())
}

// --- Tauri 命令 ---

/// 检查 cloudflared 版本
pub async fn check_cloudflared_version<R: Runtime>(app: AppHandle<R>) -> Result<CloudflaredVersionInfo, String> {
    let path = match get_cloudflared_path(&app) {
        Ok(path) => path,
        Err(_) => {
            return Ok(CloudflaredVersionInfo {
                installed: false,
                version: None,
                path: None,
                cached: false,
                cache_age_days: None,
            });
        }
    };
    
    // 尝试运行 cloudflared --version 获取版本
    let version_output = Command::new(&path)
        .arg("--version")
        .output()
        .map_err(|e| format!("Failed to get cloudflared version: {}", e))?;
    
    let version_string = String::from_utf8_lossy(&version_output.stdout).to_string();
    
    // 解析版本号（简化版本）
    let version = if version_string.contains("cloudflared") {
        // 尝试提取版本号，例如 "cloudflared version 2024.1.1 (built 2024-01-01)"
        let re = Regex::new(r"cloudflared version (\d+\.\d+\.\d+)").unwrap();
        re.captures(&version_string)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
    } else {
        None
    };
    
    // 检查是否有缓存
    let app_data_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    
    let cache_info_path = app_data_dir.join("cloudflared/cache_info.json");
    let mut cached = false;
    let mut cache_age_days = None;
    
    if cache_info_path.exists() {
        if let Ok(cache_info_json) = std::fs::read_to_string(&cache_info_path) {
            if let Ok(cache_info) = serde_json::from_str::<CloudflaredCacheInfo>(&cache_info_json) {
                cached = true;
                
                // 计算缓存年龄
                if let Ok(cache_time) = chrono::DateTime::parse_from_rfc3339(&cache_info.downloaded_at) {
                    let cache_age = chrono::Utc::now().signed_duration_since(cache_time);
                    cache_age_days = Some(cache_age.num_days());
                }
            }
        }
    }
    
    Ok(CloudflaredVersionInfo {
        installed: true,
        version,
        path: Some(path),
        cached,
        cache_age_days,
    })
}

/// 清理 cloudflared 缓存
pub async fn clear_cloudflared_cache<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let app_data_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    
    let download_dir = app_data_dir.join("cloudflared");
    
    if download_dir.exists() {
        std::fs::remove_dir_all(&download_dir)
            .map_err(|e| format!("Failed to remove cache directory: {}", e))?;
        println!("Cloudflared cache cleared");
    }
    
    Ok(())
}