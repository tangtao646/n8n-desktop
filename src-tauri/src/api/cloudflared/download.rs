//! Cloudflared 下载逻辑

use std::path::Path;
use tauri::{AppHandle, Runtime, Window};

use crate::api::cloudflared::config::PlatformDownloadConfig;
use crate::api::cloudflared::error::{CloudflaredError, CloudflaredResult};
use crate::api::cloudflared::path_resolver::CloudflaredPathResolver;
use crate::services::downloader;

/// 下载管理器
#[derive(Debug, Clone)]
pub struct DownloadManager {
    path_resolver: CloudflaredPathResolver,
}

impl DownloadManager {
    /// 创建新的下载管理器
    pub fn new() -> Self {
        Self {
            path_resolver: CloudflaredPathResolver::new(),
        }
    }

    /// 下载并安装 cloudflared
    pub async fn download_and_install<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        window: Option<Window<R>>,
        use_proxy: bool,
    ) -> CloudflaredResult<String> {
        // 首先尝试查找已安装的 cloudflared
        if let Ok(existing_path) = self.path_resolver.get_cloudflared_path(app) {
            return Ok(existing_path);
        }

        // 需要窗口句柄进行下载
        let window = window.ok_or_else(|| {
            CloudflaredError::download("下载 cloudflared 需要窗口句柄".to_string())
        })?;

        // 获取平台配置
        let config = PlatformDownloadConfig::for_current_platform()
            .map_err(|error| CloudflaredError::unsupported_platform(&error))?;

        // 准备下载目录
        let download_dir = self.path_resolver.prepare_download_directory(app)?;

        // 构建下载 URL
        let download_url = config.download_url(use_proxy);

        // 定义文件路径
        let final_binary_path = download_dir.join(&config.final_binary_name);
        let temp_download_path = download_dir.join(if config.is_archive {
            crate::api::cloudflared::config::TEMP_ARCHIVE_FILENAME
        } else {
            crate::api::cloudflared::config::TEMP_BINARY_FILENAME
        });

        // 执行下载
        self.download_binary(&window, &download_url, &temp_download_path)
            .await?;

        // 处理下载的文件
        self.process_downloaded_file(&temp_download_path, &final_binary_path, config.is_archive)?;

        // 设置文件权限
        self.set_file_permissions(&final_binary_path)?;

        Ok(final_binary_path.to_string_lossy().to_string())
    }

    /// 下载 cloudflared 二进制文件
    async fn download_binary<R: Runtime>(
        &self,
        window: &Window<R>,
        url: &str,
        destination: &Path,
    ) -> CloudflaredResult<()> {
        downloader::download_file(
            window.clone(),
            url.to_string(),
            destination.to_path_buf(),
            "cloudflared".to_string(),
        )
        .await
        .map_err(|error| CloudflaredError::download(format!("下载 cloudflared 失败: {}", error)))
    }

    /// 处理下载的文件（解压或移动）
    fn process_downloaded_file(
        &self,
        temp_path: &Path,
        final_path: &Path,
        is_archive: bool,
    ) -> CloudflaredResult<()> {
        if is_archive {
            // 解压 TAR.GZ 存档
            self.extract_from_tar_gz(temp_path, final_path)?;

            // 清理临时文件
            std::fs::remove_file(temp_path).map_err(|error| {
                CloudflaredError::filesystem(format!(
                    "删除临时文件 '{}' 失败: {}",
                    temp_path.display(),
                    error
                ))
            })?;
        } else {
            // 移动二进制文件
            if final_path.exists() {
                std::fs::remove_file(final_path).map_err(|error| {
                    CloudflaredError::filesystem(format!(
                        "删除现有文件 '{}' 失败: {}",
                        final_path.display(),
                        error
                    ))
                })?;
            }

            std::fs::rename(temp_path, final_path).map_err(|error| {
                CloudflaredError::filesystem(format!(
                    "移动文件到 '{}' 失败: {}",
                    final_path.display(),
                    error
                ))
            })?;
        }

        Ok(())
    }

    /// 从 TAR.GZ 存档中提取 cloudflared 二进制文件
    fn extract_from_tar_gz(
        &self,
        archive_path: &Path,
        destination_path: &Path,
    ) -> CloudflaredResult<()> {
        use flate2::read::GzDecoder;
        use std::fs::File;
        use std::io::copy;
        use tar::Archive;

        let tar_gz_file = File::open(archive_path).map_err(|error| {
            CloudflaredError::filesystem(format!(
                "无法打开压缩包 '{}': {}",
                archive_path.display(),
                error
            ))
        })?;

        let tar_decoder = GzDecoder::new(tar_gz_file);
        let mut archive = Archive::new(tar_decoder);

        for entry_result in archive.entries().map_err(|error| {
            CloudflaredError::extraction(format!("读取压缩包条目失败: {}", error))
        })? {
            let mut entry = entry_result.map_err(|error| {
                CloudflaredError::extraction(format!("处理压缩包条目失败: {}", error))
            })?;

            let entry_path = entry.path().map_err(|error| {
                CloudflaredError::extraction(format!("获取条目路径失败: {}", error))
            })?;

            // 匹配名为 "cloudflared" 的文件（忽略目录结构）
            if entry_path.file_name().and_then(|name| name.to_str()) == Some("cloudflared") {
                let mut output_file = File::create(destination_path).map_err(|error| {
                    CloudflaredError::filesystem(format!(
                        "创建目标文件 '{}' 失败: {}",
                        destination_path.display(),
                        error
                    ))
                })?;

                copy(&mut entry, &mut output_file).map_err(|error| {
                    CloudflaredError::filesystem(format!(
                        "提取文件到 '{}' 失败: {}",
                        destination_path.display(),
                        error
                    ))
                })?;

                return Ok(());
            }
        }

        Err(CloudflaredError::extraction(format!(
            "压缩包 '{}' 中未找到 cloudflared 二进制文件",
            archive_path.display()
        )))
    }

    /// 设置文件权限（仅 Unix 系统）
    fn set_file_permissions(&self, binary_path: &Path) -> CloudflaredResult<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            // 设置可执行权限
            std::fs::set_permissions(
                binary_path,
                std::fs::Permissions::from_mode(
                    crate::api::cloudflared::config::UNIX_EXECUTABLE_PERMISSIONS,
                ),
            )
            .map_err(|error| {
                CloudflaredError::permission(format!("设置文件权限失败: {}", error))
            })?;

            // macOS 特定：移除隔离属性
            #[cfg(target_os = "macos")]
            self.remove_macos_quarantine_attribute(binary_path);
        }

        Ok(())
    }

    /// 移除 macOS 隔离属性
    #[cfg(target_os = "macos")]
    fn remove_macos_quarantine_attribute(&self, file_path: &Path) {
        use std::process::Command;

        let _ = Command::new("xattr")
            .arg("-d")
            .arg("com.apple.quarantine")
            .arg(file_path.to_str().unwrap_or(""))
            .output();
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}
