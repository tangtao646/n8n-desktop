//! n8n 安装管理模块
//!
//! 提供 n8n 核心包的下载、验证和安装功能。

use crate::services::downloader;
use reqwest;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime, Window};
use zip::ZipArchive;

use super::constants::*;
use super::error::{N8nCoreError, N8nResult};

/// n8n 安装管理器
pub struct N8nInstaller {
    platform: String,
    app_data_dir: PathBuf,
}

impl N8nInstaller {
    /// 创建新的安装管理器
    pub fn new<R: Runtime>(app: &AppHandle<R>) -> N8nResult<Self> {
        let platform = match env::consts::OS {
            "windows" => "windows",
            "macos" => "macos",
            "linux" => "linux",
            _ => "unknown",
        };

        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| N8nCoreError::Path(e.to_string()))?;

        Ok(Self {
            platform: platform.to_string(),
            app_data_dir,
        })
    }

    /// 检查是否已安装
    pub fn is_installed(&self) -> bool {
        let bin_path = self.app_data_dir.join("n8n-core/node_modules/n8n/bin/n8n");
        bin_path.exists()
    }

    /// 获取下载 URL
    pub fn download_url(&self) -> String {
        let file_name = format!("n8n-core-{}.zip", self.platform);
        format!("{}{}/{}", GH_PROXY_PREFIX, N8N_CORE_BASE_URL, file_name)
    }

    /// 获取目标文件路径
    pub fn zip_path(&self) -> PathBuf {
        let file_name = format!("n8n-core-{}.zip", self.platform);
        self.app_data_dir.join(file_name)
    }

    /// 获取解压目录
    pub fn extract_dir(&self) -> PathBuf {
        self.app_data_dir.join("n8n-core")
    }

    /// 执行安装
    pub async fn install<R: Runtime>(&self, window: Window<R>) -> N8nResult<()> {
        println!("开始处理 n8n 资源包: {}", self.platform);

        // 1. 获取远程 SHA256 哈希值
        println!("正在获取远程 SHA256 哈希值...");
        let remote_sha256_opt = fetch_latest_sha256(&self.platform).await?;

        let need_download = self.should_download(remote_sha256_opt)?;

        // 2. 如果需要下载，则下载文件
        if need_download {
            println!("开始下载资源包: {}", self.download_url());
            downloader::download_file(
                window.clone(),
                self.download_url(),
                self.zip_path(),
                "n8n-core".to_string(),
            )
            .await
            .map_err(N8nCoreError::Installation)?;
            println!("下载完成");
        }

        // 3. 清理旧的目录并解压
        self.clean_and_extract()?;

        println!("n8n-core 安装完成");
        Ok(())
    }

    /// 判断是否需要下载
    fn should_download(&self, remote_sha: Option<String>) -> N8nResult<bool> {
        let path = self.zip_path();

        // 场景 A：本地文件根本不存在 -> 直接下载
        if !path.exists() {
            println!("本地文件不存在，需要下载");
            return Ok(true);
        }

        // 场景 B：无法获取远程哈希 -> 信任本地现有文件
        let Some(remote_hash) = remote_sha else {
            println!("无法获取远程 SHA256，跳过验证直接使用本地文件");
            return Ok(false);
        };

        // 场景 C：本地存在且有远程哈希 -> 验证完整性
        println!("成功获取远程 SHA256: {}，正在验证完整性...", remote_hash);

        let local_hash = match calculate_file_sha256(&path) {
            Ok(h) => h,
            Err(e) => {
                println!("计算本地文件哈希失败: {}，准备重新下载", e);
                return Ok(true);
            }
        };

        if local_hash == remote_hash {
            println!("文件完整性验证通过，跳过下载");
            Ok(false)
        } else {
            println!(
                "文件哈希不匹配 (本地: {}, 远程: {})",
                local_hash, remote_hash
            );
            // 尝试删除损坏文件，但不应因为删除失败就让整个 setup 崩溃
            let _ = fs::remove_file(&path).map_err(|e| {
                eprintln!("警告：清理损坏文件失败: {}", e);
            });
            Ok(true)
        }
    }

    /// 清理旧的目录并解压
    fn clean_and_extract(&self) -> N8nResult<()> {
        let final_dir = self.extract_dir();

        // 清理旧的目录（如果存在），防止解压冲突
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir)?;
        }
        fs::create_dir_all(&final_dir)?;

        // 解压到最终目录
        println!("开始解压到: {:?}", final_dir);
        self.extract_zip_file(&self.zip_path(), &final_dir)?;
        println!("解压完成");

        Ok(())
    }

    /// 解压 ZIP 文件
    fn extract_zip_file(&self, archive_path: &Path, target_dir: &Path) -> N8nResult<()> {
        let file = fs::File::open(archive_path)?;
        let mut archive = ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = match file.enclosed_name() {
                Some(path) => target_dir.join(path),
                None => continue,
            };

            if (*file.name()).ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = fs::File::create(&outpath)?;
                io::copy(&mut file, &mut outfile)?;
            }
        }
        Ok(())
    }
}

/// 从 GitHub API 获取最新发布的 SHA256 哈希值
pub async fn fetch_latest_sha256(platform: &str) -> N8nResult<Option<String>> {
    let client = reqwest::Client::new();
    let file_name = format!("n8n-core-{}.zip", platform);

    // 发送 API 请求
    let response = client
        .get(GITHUB_API_URL)
        .header("User-Agent", GITHUB_USER_AGENT)
        .header("Accept", GITHUB_ACCEPT_HEADER)
        .send()
        .await?;

    // 检查响应状态
    if !response.status().is_success() {
        println!(
            "GitHub API 返回错误 {}，跳过 SHA256 验证",
            response.status()
        );
        return Ok(None);
    }

    // 解析响应
    let text = response.text().await?;
    let json: Value = serde_json::from_str(&text)?;

    // 查找对应的资产
    let assets = json["assets"]
        .as_array()
        .ok_or_else(|| N8nCoreError::Config("GitHub 响应中缺少 assets 字段".to_string()))?;

    for asset in assets {
        if asset["name"].as_str() == Some(&file_name) {
            let digest = asset["digest"]
                .as_str()
                .ok_or_else(|| N8nCoreError::Config("资产缺少 digest 字段".to_string()))?;

            // digest 格式: "sha256:xxxxxxxx..."
            match digest.strip_prefix("sha256:") {
                Some(sha256) => return Ok(Some(sha256.to_string())),
                None => {
                    println!("无效的 digest 格式: {}，跳过 SHA256 验证", digest);
                    return Ok(None);
                }
            }
        }
    }

    println!("未找到 {} 的发布资源，跳过 SHA256 验证", file_name);
    Ok(None)
}

/// 计算文件的 SHA256 哈希值
pub fn calculate_file_sha256(file_path: &Path) -> N8nResult<String> {
    use std::io::Read;

    let mut file = fs::File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// 验证文件哈希
pub fn verify_file_hash(file_path: &Path, expected_hash: &str) -> N8nResult<()> {
    let actual_hash = calculate_file_sha256(file_path)?;

    if actual_hash == expected_hash {
        Ok(())
    } else {
        Err(N8nCoreError::HashMismatch {
            expected: expected_hash.to_string(),
            actual: actual_hash,
        })
    }
}
