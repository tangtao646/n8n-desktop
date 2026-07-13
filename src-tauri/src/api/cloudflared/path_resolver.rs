//! Cloudflared 路径查找逻辑

use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

use crate::api::cloudflared::config::CACHE_INFO_FILENAME;
use crate::api::cloudflared::error::{CloudflaredError, CloudflaredResult};
use crate::api::cloudflared::platform::PlatformDetector;
use crate::i18n;

/// Cloudflared 路径解析器
#[derive(Debug, Clone)]
pub struct CloudflaredPathResolver {
    platform_detector: PlatformDetector,
}

impl CloudflaredPathResolver {
    /// 创建新的路径解析器
    pub fn new() -> Self {
        Self {
            platform_detector: PlatformDetector::new(),
        }
    }

    /// 获取 cloudflared 二进制文件路径（多位置查找）
    pub fn get_cloudflared_path<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> CloudflaredResult<String> {
        // 1. 首先检查资源目录
        if let Some(resource_path) = self.find_in_resource_dir(app)? {
            return Ok(resource_path);
        }

        // 2. 检查应用数据目录中的缓存
        if let Some(cache_path) = self.find_in_cache_dir(app)? {
            return Ok(cache_path);
        }

        // 3. 最后检查系统 PATH
        self.find_in_system_path()
    }

    /// 在资源目录中查找 cloudflared
    fn find_in_resource_dir<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> CloudflaredResult<Option<String>> {
        let resource_dir = app.path().resource_dir().map_err(|error| {
            CloudflaredError::filesystem(format!("获取资源目录失败: {}", error))
        })?;

        let cloudflared_dir = resource_dir.join("cloudflared");

        if !cloudflared_dir.exists() {
            return Ok(None);
        }

        let binary_name = self.platform_detector.cloudflared_binary_name();

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
    fn find_in_cache_dir<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> CloudflaredResult<Option<String>> {
        let app_data_dir = app.path().app_data_dir().map_err(|error| {
            CloudflaredError::filesystem(format!("获取应用数据目录失败: {}", error))
        })?;

        let download_dir = app_data_dir.join("cloudflared");

        // 如果下载目录不存在，直接返回 None
        if !download_dir.exists() {
            return Ok(None);
        }

        let cache_info_path = download_dir.join(CACHE_INFO_FILENAME);

        // 首先检查是否有缓存信息文件
        if cache_info_path.exists() {
            let cache_info_json = std::fs::read_to_string(&cache_info_path).map_err(|error| {
                CloudflaredError::filesystem(format!("读取缓存信息文件失败: {}", error))
            })?;

            let cache_info: crate::api::cloudflared::models::CloudflaredCacheInfo =
                serde_json::from_str(&cache_info_json).map_err(|error| {
                    CloudflaredError::serialization(format!("解析缓存信息失败: {}", error))
                })?;

            let candidate_path = download_dir.join(&cache_info.filename);
            if candidate_path.exists() {
                return Ok(Some(candidate_path.to_string_lossy().to_string()));
            }
        }

        // 如果没有缓存信息文件，直接查找 cloudflared 二进制文件
        // 这是为了兼容旧版本或手动安装的情况
        let binary_name = self.platform_detector.cloudflared_binary_name();
        let binary_path = download_dir.join(&binary_name);

        if binary_path.exists() {
            // 如果找到二进制文件，尝试创建缓存信息以便下次使用
            let _ = self.create_cache_info_if_missing(&download_dir, &binary_name);
            return Ok(Some(binary_path.to_string_lossy().to_string()));
        }

        // 也检查目录中是否有名为 "cloudflared" 的文件（无扩展名）
        let fallback_path = download_dir.join("cloudflared");
        if fallback_path.exists() {
            // 如果找到二进制文件，尝试创建缓存信息以便下次使用
            let _ = self.create_cache_info_if_missing(&download_dir, "cloudflared");
            return Ok(Some(fallback_path.to_string_lossy().to_string()));
        }

        Ok(None)
    }

    /// 在系统 PATH 中查找 cloudflared
    fn find_in_system_path(&self) -> CloudflaredResult<String> {
        which::which("cloudflared")
            .map(|path| path.to_string_lossy().to_string())
            .map_err(|_| CloudflaredError::path_not_found(i18n::t("cloudflared.path.not_found_in_system")))
    }

    /// 如果缓存信息缺失，尝试创建它
    fn create_cache_info_if_missing(
        &self,
        download_dir: &Path,
        binary_name: &str,
    ) -> Result<(), ()> {
        use crate::api::cloudflared::cache::CacheManager;
        use crate::api::cloudflared::config::CACHE_INFO_FILENAME;

        let cache_info_path = download_dir.join(CACHE_INFO_FILENAME);

        // 如果缓存信息文件已经存在，不需要创建
        if cache_info_path.exists() {
            return Ok(());
        }

        // 尝试创建缓存信息
        let cache_manager = CacheManager::new();
        if let Err(_) = cache_manager.save_cache_info(download_dir, binary_name) {
            // 如果创建失败，静默失败（不返回错误）
            return Err(());
        }

        Ok(())
    }

    /// 准备下载目录
    pub fn prepare_download_directory<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> CloudflaredResult<PathBuf> {
        let app_data_dir = app.path().app_data_dir().map_err(|error| {
            CloudflaredError::filesystem(format!("获取应用数据目录失败: {}", error))
        })?;

        let download_dir = app_data_dir.join("cloudflared");

        if !download_dir.exists() {
            std::fs::create_dir_all(&download_dir).map_err(|error| {
                CloudflaredError::filesystem(format!(
                    "创建下载目录 '{}' 失败: {}",
                    download_dir.display(),
                    error
                ))
            })?;
        }

        Ok(download_dir)
    }
}

impl Default for CloudflaredPathResolver {
    fn default() -> Self {
        Self::new()
    }
}
