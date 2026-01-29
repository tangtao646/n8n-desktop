//! Cloudflared 数据结构定义

use serde::{Deserialize, Serialize};

/// Cloudflared 缓存信息
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CloudflaredCacheInfo {
    /// 文件名
    pub filename: String,
    /// 下载时间（RFC3339 格式）
    pub downloaded_at: String,
    /// 平台标识
    pub platform: String,
    /// 版本号
    pub version: String,
}

/// Cloudflared 版本信息
#[derive(Clone, Serialize, Debug)]
pub struct CloudflaredVersionInfo {
    /// 是否已安装
    pub installed: bool,
    /// 版本号（如果已安装）
    pub version: Option<String>,
    /// 安装路径（如果已安装）
    pub path: Option<String>,
    /// 是否有缓存
    pub cached: bool,
    /// 缓存天数（如果有缓存）
    pub cache_age_days: Option<i64>,
}

impl CloudflaredVersionInfo {
    /// 创建未安装的版本信息
    pub fn not_installed() -> Self {
        Self {
            installed: false,
            version: None,
            path: None,
            cached: false,
            cache_age_days: None,
        }
    }

    /// 创建已安装的版本信息
    pub fn installed(
        version: Option<String>,
        path: String,
        cached: bool,
        cache_age_days: Option<i64>,
    ) -> Self {
        Self {
            installed: true,
            version,
            path: Some(path),
            cached,
            cache_age_days,
        }
    }
}
