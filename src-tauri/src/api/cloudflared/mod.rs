//! Cloudflared 管理模块
//!
//! 提供 cloudflared 二进制文件的下载、安装、版本检查和路径管理功能。

pub mod cache;
pub mod config;
pub mod download;
pub mod error;
pub mod install;
pub mod models;
pub mod path_resolver;
pub mod platform;

// 重新导出常用类型
pub use config::*;
pub use error::CloudflaredError;
pub use models::{CloudflaredCacheInfo, CloudflaredVersionInfo};
pub use path_resolver::CloudflaredPathResolver;
pub use platform::PlatformDetector;

use tauri::{AppHandle, Runtime, Window};

/// Cloudflared 管理器
///
/// 提供 cloudflared 相关操作的统一接口
pub struct CloudflaredManager {
    path_resolver: CloudflaredPathResolver,
    platform_detector: PlatformDetector,
}

impl CloudflaredManager {
    /// 创建新的 Cloudflared 管理器
    pub fn new() -> Self {
        Self {
            path_resolver: CloudflaredPathResolver::new(),
            platform_detector: PlatformDetector::new(),
        }
    }

    /// 获取平台检测器
    pub const fn platform_detector(&self) -> &PlatformDetector {
        &self.platform_detector
    }

    /// 获取路径解析器
    pub const fn path_resolver(&self) -> &CloudflaredPathResolver {
        &self.path_resolver
    }
}

impl Default for CloudflaredManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 下载 cloudflared（Tauri 命令包装）
pub async fn download_cloudflared<R: Runtime>(
    app: AppHandle<R>,
    window: Window<R>,
) -> Result<(), String> {
    let download_manager = download::DownloadManager::new();
    download_manager
        .download_and_install(&app, Some(window), true)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 检查 cloudflared 版本信息
pub fn check_cloudflared_version<R: Runtime>(
    app: AppHandle<R>,
) -> Result<CloudflaredVersionInfo, String> {
    let path_resolver = CloudflaredPathResolver::new();
    let cache_manager = cache::CacheManager::new();

    // 第一个 match 无法用 unwrap_or_default，因为我们需要在错误时返回一个特定的值
    let path = match path_resolver.get_cloudflared_path(&app) {
        Ok(path) => path,
        Err(_) => {
            return Ok(CloudflaredVersionInfo::not_installed());
        }
    };

    // 获取版本号（如果失败，版本号为 None）
    let version = install::InstallManager::extract_version(&path).unwrap_or_default();

    // 检查缓存信息（如果失败，视为无缓存）
    let (cached, cache_age_days) = cache_manager.check_cache_info(&app).unwrap_or_default();

    Ok(CloudflaredVersionInfo::installed(
        version,
        path,
        cached,
        cache_age_days,
    ))
}
