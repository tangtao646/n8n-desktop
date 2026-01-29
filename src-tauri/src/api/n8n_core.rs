//! n8n 核心功能模块
//!
//! 提供 n8n 安装、启动、配置和管理的核心功能。
//! 重构版本：解决原始代码中的架构问题、错误处理混乱、并发安全风险等。

use crate::services::manager::PROCESS_MANAGER;
use crate::services::{downloader, manager};
use reqwest;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime, Window};
use thiserror::Error;

use super::utils::emit_global_sync;

// --- 错误定义 ---

/// n8n 核心功能错误类型
#[derive(Debug, Error)]
pub enum N8nCoreError {
    /// IO 操作失败
    #[error("IO 操作失败: {0}")]
    Io(#[from] std::io::Error),

    /// 网络请求失败
    #[error("网络请求失败: {0}")]
    Network(#[from] reqwest::Error),

    /// JSON 解析失败
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// ZIP 处理错误
    #[error("ZIP 处理错误: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// 文件哈希验证失败
    #[error("文件哈希验证失败: 期望 {expected}, 实际 {actual}")]
    HashMismatch { expected: String, actual: String },

    /// 安装失败
    #[error("安装失败: {0}")]
    Installation(String),

    /// 进程管理失败
    #[error("进程管理失败: {0}")]
    Process(String),

    /// 路径操作失败
    #[error("路径操作失败: {0}")]
    Path(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// 服务未响应
    #[error("服务未响应: {0}")]
    ServiceUnavailable(String),

    /// Tauri 相关错误
    #[error("Tauri 错误: {0}")]
    Tauri(String),
}

/// 统一 Result 类型
pub type N8nResult<T> = Result<T, N8nCoreError>;

// --- 常量定义 ---

/// 默认禁用的节点列表
const DEFAULT_BLOCKED_NODES: &str = r#"["n8n-nodes-base.executeCommand"]"#;
const DEFAULT_BLOCKED_NODES_NAMES: &str = "executeCommand";

/// GitHub API 相关常量
const GITHUB_API_URL: &str =
    "https://api.github.com/repos/tangtao646/n8n-core-builder/releases/latest";
const GITHUB_USER_AGENT: &str = "n8n-desktop";
const GITHUB_ACCEPT_HEADER: &str = "application/vnd.github.v3+json";

/// 代理下载前缀
const GH_PROXY_PREFIX: &str = "https://gh-proxy.com/";
const N8N_CORE_BASE_URL: &str =
    "https://github.com/tangtao646/n8n-core-builder/releases/latest/download";

/// 健康检查端点
const HEALTH_CHECK_ENDPOINTS: [&str; 4] = [
    "http://localhost:5678/healthz",
    "http://127.0.0.1:5678/healthz",
    "http://localhost:5678/",
    "http://127.0.0.1:5678/",
];

/// 健康检查配置
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const HEALTH_CHECK_RETRIES: usize = 3;
const HEALTH_CHECK_RETRY_DELAY: Duration = Duration::from_millis(500);

// --- 状态管理 ---

/// n8n 核心状态管理器
#[derive(Clone)]
pub struct N8nState {
    /// 节点解禁状态
    nodes_unlocked: Arc<std::sync::atomic::AtomicBool>,
}

impl N8nState {
    /// 创建新的状态实例
    pub fn new() -> Self {
        Self {
            nodes_unlocked: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 获取节点解禁状态
    pub fn nodes_unlocked(&self) -> bool {
        self.nodes_unlocked
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 设置节点解禁状态
    pub fn set_nodes_unlocked(&self, enabled: bool) {
        self.nodes_unlocked
            .store(enabled, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 全局状态实例
static N8N_STATE: LazyLock<N8nState> = LazyLock::new(N8nState::new);

// --- 环境变量构造器 ---

/// n8n 环境变量配置器
pub struct N8nEnvBuilder {
    tunnel_enabled: bool,
    tunnel_mode: Option<crate::api::tunnel::TunnelMode>,
    tunnel_url: Option<String>,
    custom_domain: Option<String>,
    nodes_unlocked: bool,
}

impl N8nEnvBuilder {
    /// 创建新的环境变量构建器
    pub fn new() -> Self {
        Self {
            tunnel_enabled: false,
            tunnel_mode: None,
            tunnel_url: None,
            custom_domain: None,
            nodes_unlocked: false,
        }
    }

    /// 启用隧道
    pub fn with_tunnel(
        mut self,
        tunnel_mode: crate::api::tunnel::TunnelMode,
        tunnel_url: Option<String>,
        custom_domain: Option<String>,
    ) -> Self {
        self.tunnel_enabled = true;
        self.tunnel_mode = Some(tunnel_mode);
        self.tunnel_url = tunnel_url;
        self.custom_domain = custom_domain;
        self
    }

    /// 设置节点解禁状态
    pub fn with_nodes_unlocked(mut self, unlocked: bool) -> Self {
        self.nodes_unlocked = unlocked;
        self
    }

    /// 构建环境变量映射
    pub fn build(self) -> HashMap<String, String> {
        let mut envs = HashMap::new();

        // 隧道相关环境变量
        if self.tunnel_enabled {
            if let Some(final_url) = self.determine_tunnel_url() {
                envs.insert("WEBHOOK_URL".to_string(), final_url.clone());
                envs.insert("N8N_WEBHOOK_URL".to_string(), final_url.clone());
                envs.insert("N8N_EDITOR_BASE_URL".to_string(), final_url);
                envs.insert("N8N_CORS_ALLOWED_ORIGINS".to_string(), "*".to_string());
            }
        }

        // 节点解禁相关环境变量
        if self.nodes_unlocked {
            envs.insert("NODES_EXCLUDE".to_string(), "[]".to_string());
            envs.insert("N8N_BLOCK_NODES".to_string(), "".to_string());
        } else {
            envs.insert(
                "NODES_EXCLUDE".to_string(),
                DEFAULT_BLOCKED_NODES.to_string(),
            );
            envs.insert(
                "N8N_BLOCK_NODES".to_string(),
                DEFAULT_BLOCKED_NODES_NAMES.to_string(),
            );
        }

        envs
    }

    /// 确定最终的隧道 URL
    fn determine_tunnel_url(&self) -> Option<String> {
        match self.tunnel_mode.as_ref()? {
            crate::api::tunnel::TunnelMode::Token { domain, .. } => {
                // Token 模式：返回配置的自定义域名
                Some(format!("https://{domain}"))
            }
            crate::api::tunnel::TunnelMode::Temporary => {
                // 临时隧道模式：使用 cloudflared 生成的临时 URL
                self.tunnel_url.clone()
            }
        }
    }
}

/// 构造 n8n 进程的环境变量映射
pub(crate) fn construct_n8n_envs() -> HashMap<String, String> {
    use crate::api::tunnel::{tunnel_config_lock, tunnel_running_lock, tunnel_url_lock};

    let tunnel_enabled = *tunnel_running_lock();
    let nodes_unlocked = N8N_STATE.nodes_unlocked();

    let mut builder = N8nEnvBuilder::new().with_nodes_unlocked(nodes_unlocked);

    if tunnel_enabled {
        let (tunnel_mode, custom_domain) = {
            let config_guard = tunnel_config_lock();
            (
                config_guard.tunnel_mode.clone(),
                config_guard.custom_domain.clone(),
            )
        };

        let tunnel_url = tunnel_url_lock().clone();
        builder = builder.with_tunnel(tunnel_mode, tunnel_url, custom_domain);
    }

    builder.build()
}

// --- 文件操作 ---

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

// --- GitHub API 交互 ---

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

// --- 安装管理 ---

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
            .map_err(|e| N8nCoreError::Installation(e))?;
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
        use std::io;
        use zip::ZipArchive;

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

// --- 健康检查 ---

/// n8n 健康检查器
pub struct N8nHealthChecker;

impl N8nHealthChecker {
    /// 执行健康检查
    pub async fn check() -> N8nResult<String> {
        let client = reqwest::Client::builder()
            .timeout(HEALTH_CHECK_TIMEOUT)
            .build()?;

        let mut last_error_msg = String::from("未启动检查");

        for retry in 0..HEALTH_CHECK_RETRIES {
            // 每一轮重试，依次尝试所有端点
            for endpoint in HEALTH_CHECK_ENDPOINTS {
                match Self::attempt_ping(&client, endpoint).await {
                    Ok(msg) => return Ok(msg), // 任意一个成功，立即返回
                    Err(e) => {
                        last_error_msg = format!("端点 {}: {}", endpoint, e);
                        // 这里不 sleep，立即尝试下一个端点（Failover 逻辑）
                    }
                }
            }

            // 一轮尝试（所有端点）全部失败后，才进行重试等待
            if retry < HEALTH_CHECK_RETRIES - 1 {
                println!(
                    "本轮健康检查全灭，等待重试 ({}/{})",
                    retry + 1,
                    HEALTH_CHECK_RETRIES
                );
                tokio::time::sleep(HEALTH_CHECK_RETRY_DELAY).await;
            }
        }

        Err(N8nCoreError::ServiceUnavailable(last_error_msg))
    }

    /// 将单个请求的逻辑提取出来，消除嵌套
    async fn attempt_ping(client: &reqwest::Client, url: &str) -> Result<String, String> {
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("网络错误: {e}"))?;

        let status = response.status();

        if status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "无法读取Body".into());
            return Ok(format!("healthy - {status} - {body}"));
        }

        // 可以在这里细化错误分类，哪些是瞬态的，哪些是永久的
        Err(format!("HTTP 状态码: {status}"))
    }
}

// --- Tauri 命令接口 ---

/// 检查 n8n 是否已经安装在 AppData 目录
pub fn is_installed<R: Runtime>(app: AppHandle<R>) -> bool {
    app.path()
        .app_data_dir()
        .map(|p| {
            let bin_path = p.join("n8n-core/node_modules/n8n/bin/n8n");
            bin_path.exists()
        })
        .unwrap_or(false)
}

/// 全自动设置 Node 运行环境 (Runtime)
pub async fn setup_runtime<R: Runtime>(window: Window<R>) -> N8nResult<()> {
    let app_handle = window.app_handle();
    let runtime_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| N8nCoreError::Path(e.to_string()))?
        .join("runtime");

    // 如果运行时已存在且二进制文件可找到，跳过
    if manager::get_node_binary_path(runtime_dir.clone()).exists() {
        return Ok(());
    }

    let url = manager::get_node_url().map_err(|e| N8nCoreError::Installation(e))?;

    // 下载逻辑内部应处理好解压
    downloader::download_file(window, url, runtime_dir, "runtime".to_string())
        .await
        .map_err(|e| N8nCoreError::Installation(e))
}

/// 安装 n8n 核心包 (下载 + 解压，带 SHA256 验证)
pub async fn setup_n8n<R: tauri::Runtime>(window: tauri::Window<R>) -> N8nResult<()> {
    let installer = N8nInstaller::new(&window.app_handle())?;
    installer.install(window).await
}

/// 启动本地 n8n 进程
pub fn launch_n8n<R: Runtime>(app: AppHandle<R>) -> N8nResult<()> {
    let app_path = app
        .path()
        .app_data_dir()
        .map_err(|e| N8nCoreError::Path(e.to_string()))?;

    let runtime_dir = app_path.join("runtime");
    let node_path = manager::get_node_binary_path(runtime_dir);

    if !node_path.exists() {
        return Err(N8nCoreError::Installation(
            "NODE_NOT_FOUND: 请先执行 setup_runtime".to_string(),
        ));
    }

    let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
    if !n8n_bin.exists() {
        return Err(N8nCoreError::Installation(
            "N8N_CORE_NOT_FOUND: 请先执行 setup_n8n".to_string(),
        ));
    }

    let data_dir = app_path.join("n8n-data");
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)?;
    }

    // 创建环境变量容器
    let additional_envs = construct_n8n_envs();

    manager::start_node(node_path, n8n_bin, data_dir, additional_envs)
        .map_err(|e| N8nCoreError::Process(e))
}

/// 代理健康检查
pub async fn proxy_health_check() -> N8nResult<String> {
    N8nHealthChecker::check().await
}

/// 设置节点解禁状态并重启 n8n
pub async fn set_nodes_unlocked<R: Runtime>(app: AppHandle<R>, enabled: bool) -> N8nResult<()> {
    // 1. 更新全局状态
    N8N_STATE.set_nodes_unlocked(enabled);
    println!("[DEBUG] 节点解禁状态已设置为: {enabled}");

    // 2. 检查 n8n 是否正在运行
    let is_running = {
        let manager = PROCESS_MANAGER
            .lock()
            .map_err(|_| N8nCoreError::Process("PROCESS_MANAGER mutex poisoned".to_string()))?;
        manager.has_child()
    };

    println!("[DEBUG] n8n 运行状态: {is_running}");

    if !is_running {
        println!("[DEBUG] n8n 未运行，无需重启");
        return Ok(());
    }

    // 3. 获取应用路径和二进制
    let app_path = app
        .path()
        .app_data_dir()
        .map_err(|e| N8nCoreError::Path(format!("Failed to get app data dir: {e}")))?;

    println!("[DEBUG] 应用路径: {}", app_path.display());

    let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
    println!("[DEBUG] n8n 二进制路径: {}", n8n_bin.display());

    if !n8n_bin.exists() {
        println!("[DEBUG] n8n 二进制文件不存在");
        return Err(N8nCoreError::Installation(
            "N8N binary not found".to_string(),
        ));
    }

    let runtime_dir = app_path.join("runtime");
    let node_path = manager::get_node_binary_path(runtime_dir);
    println!("[DEBUG] node 二进制路径: {}", node_path.display());

    if !node_path.exists() {
        println!("[DEBUG] node 二进制文件不存在");
        return Err(N8nCoreError::Installation(
            "Node binary not found".to_string(),
        ));
    }

    let data_dir = app_path.join("n8n-data");
    if !data_dir.exists() {
        println!("[DEBUG] 创建数据目录: {}", data_dir.display());
        fs::create_dir_all(&data_dir)?;
    }

    // 4. 构建新的环境变量
    let additional_envs = construct_n8n_envs();
    println!("[DEBUG] 构建的环境变量: {additional_envs:?}");

    // 5. 物理重启：杀掉再重启
    println!("[DEBUG] 正在重启 n8n 以应用节点解禁设置...");

    // 5.1 杀掉现有进程
    println!("[DEBUG] 杀掉现有进程...");
    if let Ok(mut manager) = PROCESS_MANAGER.lock() {
        manager.kill_child();
    }

    // 5.2 等待 500ms 确保端口释放
    println!("[DEBUG] 等待 500ms 确保端口释放...");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 5.3 重新启动 n8n
    println!("[DEBUG] 重新启动 n8n...");
    match manager::start_node(node_path, n8n_bin, data_dir, additional_envs) {
        Ok(()) => {
            println!("[DEBUG] n8n 已重启，节点解禁设置已应用");

            // 广播全局同步事件，通知前端刷新 UI
            emit_global_sync(&app).map_err(|e| N8nCoreError::Tauri(e.to_string()))?;
            Ok(())
        }
        Err(e) => {
            println!("[DEBUG] 重启 n8n 失败: {e}");
            Err(N8nCoreError::Process(e))
        }
    }
}

/// 获取节点解禁状态
pub fn get_nodes_unlocked() -> N8nResult<bool> {
    Ok(N8N_STATE.nodes_unlocked())
}

/// 关闭 n8n 进程
pub fn shutdown_n8n() -> N8nResult<()> {
    // 1. 使用 map_err 统一错误转换，减少缩进
    let mut manager = PROCESS_MANAGER
        .lock()
        .map_err(|_| N8nCoreError::Process("PROCESS_MANAGER 锁已被毒化 (Poisoned)".into()))?;

    // 2. 执行 kill
    manager.kill_child();

    println!("[n8n] 进程已请求关闭");
    Ok(())
}
