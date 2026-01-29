//! n8n 状态管理和健康检查模块
//!
//! 提供全局状态管理、健康检查和环境变量构造功能。

use crate::api::tunnel::{tunnel_config_lock, tunnel_running_lock, tunnel_url_lock};
use crate::services::manager::PROCESS_MANAGER;
use reqwest;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tauri::{AppHandle, Manager, Runtime};

use super::constants::*;
use super::error::{N8nCoreError, N8nResult};

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
pub fn construct_n8n_envs() -> HashMap<String, String> {
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

// --- 状态访问函数 ---

/// 获取节点解禁状态
pub fn get_nodes_unlocked() -> N8nResult<bool> {
    Ok(N8N_STATE.nodes_unlocked())
}

/// 设置节点解禁状态并重启 n8n
pub async fn set_nodes_unlocked<R: Runtime>(app: AppHandle<R>, enabled: bool) -> N8nResult<()> {
    use crate::api::utils::emit_global_sync;
    use crate::services::manager;
    use std::fs;
    use tokio::time::Duration;

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
