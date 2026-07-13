//! Cloudflared 配置管理
//!
//! 包含常量定义、平台配置和下载配置。

use crate::i18n;
use std::env;

/// Cloudflared GitHub Releases 基础 URL
pub const CLOUDFLARED_BASE_URL: &str =
    "https://github.com/cloudflare/cloudflared/releases/latest/download";

/// GitHub 代理前缀（用于中国大陆访问加速）
pub const GH_PROXY_PREFIX: &str = "https://gh-proxy.com/";

/// Unix 可执行文件权限模式
pub const UNIX_EXECUTABLE_PERMISSIONS: u32 = 0o755; // rwxr-xr-x

/// 缓存信息文件名
pub const CACHE_INFO_FILENAME: &str = "cache_info.json";

/// 临时下载文件名（存档）
pub const TEMP_ARCHIVE_FILENAME: &str = "cloudflared_temp.tgz";

/// 临时下载文件名（二进制）
pub const TEMP_BINARY_FILENAME: &str = "cloudflared.tmp";

/// 平台特定的下载配置
#[derive(Debug, Clone)]
pub struct PlatformDownloadConfig {
    /// 远程文件名
    pub remote_filename: String,
    /// 是否为存档文件
    pub is_archive: bool,
    /// 最终二进制文件名
    pub final_binary_name: String,
}

impl PlatformDownloadConfig {
    /// 获取当前平台的下载配置
    pub fn for_current_platform() -> Result<Self, String> {
        let architecture = get_architecture_identifier();

        match env::consts::OS {
            "windows" => Ok(Self {
                remote_filename: "cloudflared-windows-amd64.exe".to_string(),
                is_archive: false,
                final_binary_name: "cloudflared.exe".to_string(),
            }),
            "macos" => Ok(Self {
                remote_filename: format!("cloudflared-darwin-{}.tgz", architecture),
                is_archive: true,
                final_binary_name: "cloudflared".to_string(),
            }),
            "linux" => Ok(Self {
                remote_filename: format!("cloudflared-linux-{}", architecture),
                is_archive: false,
                final_binary_name: "cloudflared".to_string(),
            }),
            os => Err(format!("{}: {}", i18n::t("cloudflared.unsupported_platform"), os)),
        }
    }

    /// 获取下载 URL
    pub fn download_url(&self, use_proxy: bool) -> String {
        let base_url = if use_proxy {
            format!("{}{}", GH_PROXY_PREFIX, CLOUDFLARED_BASE_URL)
        } else {
            CLOUDFLARED_BASE_URL.to_string()
        };
        format!("{}/{}", base_url, self.remote_filename)
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

/// 获取当前平台标识字符串
pub fn get_platform_identifier() -> String {
    match env::consts::OS {
        "windows" => "windows".to_string(),
        "macos" => "macos".to_string(),
        "linux" => "linux".to_string(),
        _ => "unknown".to_string(),
    }
}
