use chrono;
use flate2::read::GzDecoder;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::copy;
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::Archive;
use tauri::{AppHandle, Manager, Runtime, Window};

use crate::services::downloader;

// --- 常量定义 ---

/// Cloudflared GitHub Releases 基础 URL
const CLOUDFLARED_BASE_URL: &str =
    "https://github.com/cloudflare/cloudflared/releases/latest/download";

/// GitHub 代理前缀（用于中国大陆访问加速）
const GH_PROXY_PREFIX: &str = "https://gh-proxy.com/";

/// Unix 可执行文件权限模式
const UNIX_EXECUTABLE_PERMISSIONS: u32 = 0o755; // rwxr-xr-x

/// 缓存信息文件名
const CACHE_INFO_FILENAME: &str = "cache_info.json";

/// 临时下载文件名（存档）
const TEMP_ARCHIVE_FILENAME: &str = "cloudflared_temp.tgz";

/// 临时下载文件名（二进制）
const TEMP_BINARY_FILENAME: &str = "cloudflared.tmp";

/// 版本号提取正则表达式（使用 Lazy 避免重复编译）
static VERSION_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"version (\d+\.\d+\.\d+)").expect("版本正则表达式编译失败"));

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

/// 平台特定的下载配置
struct PlatformDownloadConfig {
    remote_filename: String,
    is_archive: bool,
    final_binary_name: String,
}

// --- 内部辅助函数 ---

/// 获取当前平台标识字符串
fn get_platform_identifier() -> String {
    match env::consts::OS {
        "windows" => "windows".to_string(),
        "macos" => "macos".to_string(),
        "linux" => "linux".to_string(),
        _ => "unknown".to_string(),
    }
}

/// 获取当前系统架构标识
fn get_architecture_identifier() -> &'static str {
    match env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        _ => "unknown",
    }
}

/// 从 TAR.GZ 存档中提取 cloudflared 二进制文件
fn extract_binary_from_tar_gz(archive_path: &Path, destination_path: &Path) -> Result<(), String> {
    let tar_gz_file = File::open(archive_path)
        .map_err(|error| format!("无法打开压缩包 '{}': {}", archive_path.display(), error))?;

    let tar_decoder = GzDecoder::new(tar_gz_file);
    let mut archive = Archive::new(tar_decoder);

    for entry_result in archive
        .entries()
        .map_err(|error| format!("读取压缩包条目失败: {}", error))?
    {
        let mut entry = entry_result.map_err(|error| format!("处理压缩包条目失败: {}", error))?;

        let entry_path = entry
            .path()
            .map_err(|error| format!("获取条目路径失败: {}", error))?;

        // 匹配名为 "cloudflared" 的文件（忽略目录结构）
        if entry_path.file_name().and_then(|name| name.to_str()) == Some("cloudflared") {
            let mut output_file = File::create(destination_path).map_err(|error| {
                format!(
                    "创建目标文件 '{}' 失败: {}",
                    destination_path.display(),
                    error
                )
            })?;

            copy(&mut entry, &mut output_file).map_err(|error| {
                format!(
                    "提取文件到 '{}' 失败: {}",
                    destination_path.display(),
                    error
                )
            })?;

            return Ok(());
        }
    }

    Err(format!(
        "压缩包 '{}' 中未找到 cloudflared 二进制文件",
        archive_path.display()
    ))
}

/// 获取 cloudflared 二进制文件路径（多位置查找）
pub fn get_cloudflared_path<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    // 1. 首先检查资源目录
    if let Some(resource_path) = find_cloudflared_in_resource_dir(app)? {
        return Ok(resource_path);
    }

    // 2. 检查应用数据目录中的缓存
    if let Some(cache_path) = find_cloudflared_in_cache_dir(app)? {
        return Ok(cache_path);
    }

    // 3. 最后检查系统 PATH
    find_cloudflared_in_system_path()
}

/// 在资源目录中查找 cloudflared
fn find_cloudflared_in_resource_dir<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<String>, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("获取资源目录失败: {}", error))?;

    let cloudflared_dir = resource_dir.join("cloudflared");

    if !cloudflared_dir.exists() {
        return Ok(None);
    }

    let binary_name = match env::consts::OS {
        "windows" => "cloudflared.exe",
        _ => "cloudflared",
    };

    // 检查是否为直接的文件
    if cloudflared_dir.is_file() {
        return Ok(Some(cloudflared_dir.to_string_lossy().to_string()));
    }

    // 检查目录中的二进制文件
    let binary_path = cloudflared_dir.join(binary_name);
    if binary_path.exists() {
        return Ok(Some(binary_path.to_string_lossy().to_string()));
    }

    Ok(None)
}

/// 在缓存目录中查找 cloudflared
fn find_cloudflared_in_cache_dir<R: Runtime>(app: &AppHandle<R>) -> Result<Option<String>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {}", error))?;

    let download_dir = app_data_dir.join("cloudflared");
    let cache_info_path = download_dir.join(CACHE_INFO_FILENAME);

    if !download_dir.exists() || !cache_info_path.exists() {
        return Ok(None);
    }

    let cache_info_json = std::fs::read_to_string(&cache_info_path)
        .map_err(|error| format!("读取缓存信息文件失败: {}", error))?;

    let cache_info: CloudflaredCacheInfo = serde_json::from_str(&cache_info_json)
        .map_err(|error| format!("解析缓存信息失败: {}", error))?;

    let candidate_path = download_dir.join(&cache_info.filename);
    if candidate_path.exists() {
        return Ok(Some(candidate_path.to_string_lossy().to_string()));
    }

    Ok(None)
}

/// 在系统 PATH 中查找 cloudflared
fn find_cloudflared_in_system_path() -> Result<String, String> {
    match which::which("cloudflared") {
        Ok(path) => Ok(path.to_string_lossy().to_string()),
        Err(_) => Err("系统中未找到 cloudflared 可执行文件".to_string()),
    }
}

/// 获取平台特定的下载配置
fn get_platform_download_config() -> Result<PlatformDownloadConfig, String> {
    let architecture = get_architecture_identifier();

    match env::consts::OS {
        "windows" => Ok(PlatformDownloadConfig {
            remote_filename: "cloudflared-windows-amd64.exe".to_string(),
            is_archive: false,
            final_binary_name: "cloudflared.exe".to_string(),
        }),
        "macos" => Ok(PlatformDownloadConfig {
            remote_filename: format!("cloudflared-darwin-{}.tgz", architecture),
            is_archive: true,
            final_binary_name: "cloudflared".to_string(),
        }),
        "linux" => Ok(PlatformDownloadConfig {
            remote_filename: format!("cloudflared-linux-{}", architecture),
            is_archive: false,
            final_binary_name: "cloudflared".to_string(),
        }),
        _ => Err("当前操作系统平台暂不支持".to_string()),
    }
}

/// 下载并安装 cloudflared
pub async fn get_or_download_cloudflared<R: Runtime>(
    app: &AppHandle<R>,
    window: Option<Window<R>>,
) -> Result<String, String> {
    // 首先尝试查找已安装的 cloudflared
    if let Ok(existing_path) = get_cloudflared_path(app) {
        return Ok(existing_path);
    }

    // 需要窗口句柄进行下载
    let window = window.ok_or("下载 cloudflared 需要窗口句柄".to_string())?;

    // 获取平台配置
    let config = get_platform_download_config()?;

    // 准备下载目录
    let download_dir = prepare_download_directory(app)?;

    // 构建下载 URL
    let download_url = format!("{}/{}", CLOUDFLARED_BASE_URL, config.remote_filename);
    let proxy_url = format!("{}{}", GH_PROXY_PREFIX, download_url);

    // 定义文件路径
    let final_binary_path = download_dir.join(&config.final_binary_name);
    let temp_download_path = download_dir.join(if config.is_archive {
        TEMP_ARCHIVE_FILENAME
    } else {
        TEMP_BINARY_FILENAME
    });

    // 执行下载
    download_cloudflared_binary(&window, &proxy_url, &temp_download_path).await?;

    // 处理下载的文件
    process_downloaded_file(&temp_download_path, &final_binary_path, config.is_archive)?;

    // 设置文件权限
    set_file_permissions(&final_binary_path)?;

    // 保存缓存信息
    save_cache_info(&download_dir, &config.final_binary_name)?;

    Ok(final_binary_path.to_string_lossy().to_string())
}

/// 准备下载目录
fn prepare_download_directory<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {}", error))?;

    let download_dir = app_data_dir.join("cloudflared");

    if !download_dir.exists() {
        std::fs::create_dir_all(&download_dir).map_err(|error| {
            format!("创建下载目录 '{}' 失败: {}", download_dir.display(), error)
        })?;
    }

    Ok(download_dir)
}

/// 下载 cloudflared 二进制文件
async fn download_cloudflared_binary<R: Runtime>(
    window: &Window<R>,
    url: &str,
    destination: &Path,
) -> Result<(), String> {
    downloader::download_file(
        window.clone(),
        url.to_string(),
        destination.to_path_buf(),
        "cloudflared".to_string(),
    )
    .await
    .map_err(|error| format!("下载 cloudflared 失败: {}", error))
}

/// 处理下载的文件（解压或移动）
fn process_downloaded_file(
    temp_path: &Path,
    final_path: &Path,
    is_archive: bool,
) -> Result<(), String> {
    if is_archive {
        // 解压 TAR.GZ 存档
        extract_binary_from_tar_gz(temp_path, final_path)?;

        // 清理临时文件
        std::fs::remove_file(temp_path)
            .map_err(|error| format!("删除临时文件 '{}' 失败: {}", temp_path.display(), error))?;
    } else {
        // 移动二进制文件
        if final_path.exists() {
            std::fs::remove_file(final_path).map_err(|error| {
                format!("删除现有文件 '{}' 失败: {}", final_path.display(), error)
            })?;
        }

        std::fs::rename(temp_path, final_path)
            .map_err(|error| format!("移动文件到 '{}' 失败: {}", final_path.display(), error))?;
    }

    Ok(())
}

/// 设置文件权限（仅 Unix 系统）
fn set_file_permissions(binary_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        // 设置可执行权限
        std::fs::set_permissions(
            binary_path,
            std::fs::Permissions::from_mode(UNIX_EXECUTABLE_PERMISSIONS),
        )
        .map_err(|error| format!("设置文件权限失败: {}", error))?;

        // macOS 特定：移除隔离属性
        #[cfg(target_os = "macos")]
        remove_macos_quarantine_attribute(binary_path);
    }

    Ok(())
}

/// 移除 macOS 隔离属性
#[cfg(target_os = "macos")]
fn remove_macos_quarantine_attribute(file_path: &Path) {
    let _ = Command::new("xattr")
        .arg("-d")
        .arg("com.apple.quarantine")
        .arg(file_path.to_str().unwrap_or(""))
        .output();
}

/// 保存缓存信息
fn save_cache_info(download_dir: &Path, binary_name: &str) -> Result<(), String> {
    let cache_info = CloudflaredCacheInfo {
        filename: binary_name.to_string(),
        downloaded_at: chrono::Utc::now().to_rfc3339(),
        platform: get_platform_identifier(),
        version: "latest".to_string(),
    };

    let info_json = serde_json::to_string_pretty(&cache_info)
        .map_err(|error| format!("序列化缓存信息失败: {}", error))?;

    let cache_path = download_dir.join(CACHE_INFO_FILENAME);
    std::fs::write(&cache_path, info_json)
        .map_err(|error| format!("写入缓存文件 '{}' 失败: {}", cache_path.display(), error))?;

    Ok(())
}

// --- Tauri 命令函数 ---

/// 检查 cloudflared 版本信息
pub async fn check_cloudflared_version<R: Runtime>(
    app: AppHandle<R>,
) -> Result<CloudflaredVersionInfo, String> {
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

    // 获取版本号
    let version = extract_cloudflared_version(&path)?;

    // 检查缓存信息
    let (cached, cache_age_days) = check_cache_info(&app)?;

    Ok(CloudflaredVersionInfo {
        installed: true,
        version,
        path: Some(path),
        cached,
        cache_age_days,
    })
}

/// 提取 cloudflared 版本号
fn extract_cloudflared_version(binary_path: &str) -> Result<Option<String>, String> {
    let output = Command::new(binary_path)
        .arg("--version")
        .output()
        .map_err(|error| format!("执行 cloudflared 版本检查失败: {}", error))?;

    let version_output = String::from_utf8_lossy(&output.stdout);

    let version = VERSION_REGEX
        .captures(&version_output)
        .and_then(|captures| captures.get(1))
        .map(|matched| matched.as_str().to_string());

    Ok(version)
}

/// 检查缓存信息
fn check_cache_info<R: Runtime>(app: &AppHandle<R>) -> Result<(bool, Option<i64>), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {}", error))?;

    let cache_path = app_data_dir.join("cloudflared").join(CACHE_INFO_FILENAME);

    if !cache_path.exists() {
        return Ok((false, None));
    }

    let cache_info_json = std::fs::read_to_string(&cache_path)
        .map_err(|error| format!("读取缓存文件失败: {}", error))?;

    let cache_info: CloudflaredCacheInfo = serde_json::from_str(&cache_info_json)
        .map_err(|error| format!("解析缓存信息失败: {}", error))?;

    let downloaded_at = chrono::DateTime::parse_from_rfc3339(&cache_info.downloaded_at)
        .map_err(|error| format!("解析下载时间失败: {}", error))?;

    let cache_age_days = chrono::Utc::now()
        .signed_duration_since(downloaded_at)
        .num_days();

    Ok((true, Some(cache_age_days)))
}

/// 下载 cloudflared（Tauri 命令包装）
pub async fn download_cloudflared<R: Runtime>(
    app: AppHandle<R>,
    window: Window<R>,
) -> Result<(), String> {
    get_or_download_cloudflared(&app, Some(window))
        .await
        .map(|_| ())
}
