//! Cloudflared 缓存管理

use chrono::{DateTime, Utc};
use serde_json;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

use crate::api::cloudflared::config::CACHE_INFO_FILENAME;
use crate::api::cloudflared::error::{CloudflaredError, CloudflaredResult};
use crate::api::cloudflared::models::CloudflaredCacheInfo;
use crate::api::cloudflared::platform::PlatformDetector;

/// 缓存管理器
#[derive(Debug, Clone)]
pub struct CacheManager {
    platform_detector: PlatformDetector,
}

impl CacheManager {
    /// 创建新的缓存管理器
    pub fn new() -> Self {
        Self {
            platform_detector: PlatformDetector::new(),
        }
    }

    /// 保存缓存信息
    pub fn save_cache_info(&self, download_dir: &Path, binary_name: &str) -> CloudflaredResult<()> {
        let cache_info = CloudflaredCacheInfo {
            filename: binary_name.to_string(),
            downloaded_at: Utc::now().to_rfc3339(),
            platform: self.platform_detector.platform_identifier(),
            version: "latest".to_string(),
        };

        let info_json = serde_json::to_string_pretty(&cache_info).map_err(|error| {
            CloudflaredError::serialization(format!("序列化缓存信息失败: {}", error))
        })?;

        let cache_path = download_dir.join(CACHE_INFO_FILENAME);
        std::fs::write(&cache_path, info_json).map_err(|error| {
            CloudflaredError::filesystem(format!(
                "写入缓存文件 '{}' 失败: {}",
                cache_path.display(),
                error
            ))
        })?;

        Ok(())
    }

    /// 检查缓存信息
    pub fn check_cache_info<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> CloudflaredResult<(bool, Option<i64>)> {
        let app_data_dir = app.path().app_data_dir().map_err(|error| {
            CloudflaredError::filesystem(format!("获取应用数据目录失败: {}", error))
        })?;

        let cache_path = app_data_dir.join("cloudflared").join(CACHE_INFO_FILENAME);

        if !cache_path.exists() {
            return Ok((false, None));
        }

        let cache_info_json = std::fs::read_to_string(&cache_path).map_err(|error| {
            CloudflaredError::filesystem(format!("读取缓存文件失败: {}", error))
        })?;

        let cache_info: CloudflaredCacheInfo =
            serde_json::from_str(&cache_info_json).map_err(|error| {
                CloudflaredError::serialization(format!("解析缓存信息失败: {}", error))
            })?;

        let downloaded_at =
            DateTime::parse_from_rfc3339(&cache_info.downloaded_at).map_err(|error| {
                CloudflaredError::serialization(format!("解析下载时间失败: {}", error))
            })?;

        let cache_age_days = Utc::now().signed_duration_since(downloaded_at).num_days();

        Ok((true, Some(cache_age_days)))
    }

    /// 获取缓存文件路径
    pub fn get_cache_path<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> CloudflaredResult<Option<PathBuf>> {
        let app_data_dir = app.path().app_data_dir().map_err(|error| {
            CloudflaredError::filesystem(format!("获取应用数据目录失败: {}", error))
        })?;

        let cache_dir = app_data_dir.join("cloudflared");
        let cache_info_path = cache_dir.join(CACHE_INFO_FILENAME);

        if !cache_dir.exists() || !cache_info_path.exists() {
            return Ok(None);
        }

        let cache_info_json = std::fs::read_to_string(&cache_info_path).map_err(|error| {
            CloudflaredError::filesystem(format!("读取缓存信息文件失败: {}", error))
        })?;

        let cache_info: CloudflaredCacheInfo =
            serde_json::from_str(&cache_info_json).map_err(|error| {
                CloudflaredError::serialization(format!("解析缓存信息失败: {}", error))
            })?;

        let candidate_path = cache_dir.join(&cache_info.filename);
        if candidate_path.exists() {
            return Ok(Some(candidate_path));
        }

        Ok(None)
    }

    /// 清理过期缓存
    pub fn cleanup_expired_cache<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        max_age_days: i64,
    ) -> CloudflaredResult<bool> {
        let (cached, cache_age_days) = self.check_cache_info(app)?;

        if !cached {
            return Ok(false);
        }

        if let Some(age) = cache_age_days {
            if age > max_age_days {
                let app_data_dir = app.path().app_data_dir().map_err(|error| {
                    CloudflaredError::filesystem(format!("获取应用数据目录失败: {}", error))
                })?;

                let cache_dir = app_data_dir.join("cloudflared");

                // 删除缓存目录
                if cache_dir.exists() {
                    std::fs::remove_dir_all(&cache_dir).map_err(|error| {
                        CloudflaredError::filesystem(format!("删除缓存目录失败: {}", error))
                    })?;
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new()
    }
}
