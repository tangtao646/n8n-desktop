use tauri::{AppHandle, Manager, Runtime, Window};
use std::process::Command;
use std::fs::File;
use std::io::{self, copy};
use std::path::{Path, PathBuf};
use regex::Regex;
use serde::{Serialize, Deserialize};
use chrono;
use flate2::read::GzDecoder;
use tar::Archive;

use crate::services::downloader;

// --- 公共常量提取 ---
const CLOUDFLARED_BASE_URL: &str = "https://github.com/cloudflare/cloudflared/releases/latest/download";
const GH_PROXY_PREFIX: &str = "https://gh-proxy.com/";

// --- 数据结构定义 ---

#[derive(Clone, Serialize, Deserialize)]
pub struct CloudflaredCacheInfo {
    pub filename: String,
    pub downloaded_at: String,
    pub platform: String,
    pub version: String,
}

#[derive(Clone, Serialize)]
pub struct CloudflaredVersionInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub cached: bool,
    pub cache_age_days: Option<i64>,
}

// --- 内部辅助函数 ---

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

/// 【核心改进】精准解压函数：
/// 遍历 tgz，只提取名为 "cloudflared" 的二进制文件，忽略任何内部文件夹结构
fn extract_bin_from_tgz(archive_path: &Path, dest_path: &Path) -> Result<(), String> {
    let tar_gz = File::open(archive_path).map_err(|e| format!("无法打开压缩包: {}", e))?;
    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);
    
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;

        // 无论压缩包内是否有文件夹包裹（如 ./bin/cloudflared），只匹配文件名
        if path.file_name().and_then(|s| s.to_str()) == Some("cloudflared") {
            let mut out_file = File::create(dest_path).map_err(|e| e.to_string())?;
            copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err("压缩包内未找到 cloudflared 二进制文件".to_string())
}

pub fn get_cloudflared_path<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let resource_path = app.path()
        .resource_dir()
        .map_err(|e| format!("获取资源目录失败: {}", e))?
        .join("cloudflared");

    if resource_path.exists() {
        if resource_path.is_file() {
            return Ok(resource_path.to_string_lossy().to_string());
        } else if resource_path.is_dir() {
            let exe_name = if cfg!(target_os = "windows") { "cloudflared.exe" } else { "cloudflared" };
            let exe_path = resource_path.join(exe_name);
            if exe_path.exists() {
                return Ok(exe_path.to_string_lossy().to_string());
            }
        }
    }

    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let download_dir = app_data_dir.join("cloudflared");
    let cache_info_path = download_dir.join("cache_info.json");

    if download_dir.exists() && cache_info_path.exists() {
        if let Ok(cache_info_json) = std::fs::read_to_string(&cache_info_path) {
            if let Ok(cache_info) = serde_json::from_str::<CloudflaredCacheInfo>(&cache_info_json) {
                let candidate_path = download_dir.join(&cache_info.filename);
                if candidate_path.exists() {
                    return Ok(candidate_path.to_string_lossy().to_string());
                }
            }
        }
    }

    match which::which("cloudflared") {
        Ok(path) => Ok(path.to_string_lossy().to_string()),
        Err(_) => Err("未找到 cloudflared".to_string()),
    }
}

pub async fn get_or_download_cloudflared<R: Runtime>(
    app: &AppHandle<R>, 
    window: Option<Window<R>>
) -> Result<String, String> {
    if let Ok(path) = get_cloudflared_path(app) {
        return Ok(path);
    }

    let window = window.ok_or("下载 cloudflared 需要窗口句柄".to_string())?;
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let download_dir = app_data_dir.join("cloudflared");
    
    if !download_dir.exists() {
        std::fs::create_dir_all(&download_dir).map_err(|e| e.to_string())?;
    }

    // --- 识别架构与平台 ---
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    
    let (remote_filename, is_archive) = if cfg!(target_os = "windows") {
        ("cloudflared-windows-amd64.exe".to_string(), false)
    } else if cfg!(target_os = "macos") {
        // macOS 需要根据架构选 tgz
        (format!("cloudflared-darwin-{}.tgz", arch), true)
    } else if cfg!(target_os = "linux") {
        // Linux GitHub Release 是裸奔的二进制，没有 .exe 也没有 .tgz
        (format!("cloudflared-linux-{}", arch), false)
    } else {
        return Err("暂不支持此操作系统平台".to_string());
    };

    let download_url = format!("{}/{}", CLOUDFLARED_BASE_URL, remote_filename);
    let proxy_url = format!("{}{}", GH_PROXY_PREFIX, download_url);

    // 统一定义最终二进制文件名
    let final_bin_name = if cfg!(target_os = "windows") { "cloudflared.exe" } else { "cloudflared" };
    let final_bin_path = download_dir.join(final_bin_name);
    
    // 定义下载时的临时路径
    let temp_download_path = download_dir.join(if is_archive { "cloudflared_temp.tgz" } else { "cloudflared.tmp" });

    // --- 执行下载 ---
    downloader::download_file(
        window.clone(), 
        proxy_url, 
        temp_download_path.clone(), 
        "cloudflared".to_string()
    ).await?;

    // --- 处理解压或移动 ---
    if is_archive {
        // 如果是 macOS 的 tgz，执行精准解压
        extract_bin_from_tgz(&temp_download_path, &final_bin_path)?;
        let _ = std::fs::remove_file(&temp_download_path); 
    } else {
        // 如果是 Windows 或 Linux 的裸二进制文件，直接覆盖移动
        if final_bin_path.exists() {
            std::fs::remove_file(&final_bin_path).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&temp_download_path, &final_bin_path).map_err(|e| e.to_string())?;
    }

    // --- 设置权限与 macOS 隔离位清理 ---
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            &final_bin_path, 
            std::fs::Permissions::from_mode(0o755)
        ).map_err(|e| format!("设置权限失败: {}", e))?;

        #[cfg(target_os = "macos")]
        {
            // 清除 macOS 的“隔离属性”，防止用户运行报错“无法验证开发者”
            let _ = Command::new("xattr")
                .arg("-d")
                .arg("com.apple.quarantine")
                .arg(&final_bin_path)
                .output();
        }
    }

    // --- 写入缓存元数据 ---
    let cache_info = CloudflaredCacheInfo {
        filename: final_bin_name.to_string(),
        downloaded_at: chrono::Utc::now().to_rfc3339(),
        platform: get_platform_string(),
        version: "latest".to_string(),
    };
    
    let info_json = serde_json::to_string(&cache_info).map_err(|e| e.to_string())?;
    std::fs::write(download_dir.join("cache_info.json"), info_json).map_err(|e| e.to_string())?;

    Ok(final_bin_path.to_string_lossy().to_string())
}

// --- Tauri 指令 ---

pub async fn check_cloudflared_version<R: Runtime>(app: AppHandle<R>) -> Result<CloudflaredVersionInfo, String> {
    let path = match get_cloudflared_path(&app) {
        Ok(p) => p,
        Err(_) => return Ok(CloudflaredVersionInfo { 
            installed: false, version: None, path: None, cached: false, cache_age_days: None 
        }),
    };

    let output = Command::new(&path).arg("--version").output().map_err(|e| e.to_string())?;
    let version_string = String::from_utf8_lossy(&output.stdout);
    
    let re = Regex::new(r"version (\d+\.\d+\.\d+)").unwrap();
    let version = re.captures(&version_string)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let app_data_dir = app.path().app_data_dir().unwrap();
    let cache_path = app_data_dir.join("cloudflared/cache_info.json");
    let mut cache_age = None;

    if let Ok(info_str) = std::fs::read_to_string(&cache_path) {
        if let Ok(info) = serde_json::from_str::<CloudflaredCacheInfo>(&info_str) {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&info.downloaded_at) {
                cache_age = Some(chrono::Utc::now().signed_duration_since(dt).num_days());
            }
        }
    }

    Ok(CloudflaredVersionInfo {
        installed: true,
        version,
        path: Some(path),
        cached: cache_path.exists(),
        cache_age_days: cache_age,
    })
}

pub async fn download_cloudflared<R: Runtime>(app: AppHandle<R>, window: Window<R>) -> Result<(), String> {
    get_or_download_cloudflared(&app, Some(window)).await.map(|_| ())
}