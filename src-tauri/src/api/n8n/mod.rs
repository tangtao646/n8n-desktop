//! n8n 核心功能模块
//!
//! 提供 n8n 安装、启动、配置和管理的核心功能。
//! 重构版本：解决原始代码中的架构问题、错误处理混乱、并发安全风险等。

// 导出子模块
pub mod constants;
pub mod error;
pub mod installer;
pub mod state;

// 重新导出常用类型和函数
pub use constants::*;
pub use error::{N8nCoreError, N8nResult};
pub use installer::{calculate_file_sha256, fetch_latest_sha256, verify_file_hash, N8nInstaller};
pub use state::{construct_n8n_envs, get_nodes_unlocked, set_nodes_unlocked, N8nHealthChecker};

use crate::services::{downloader, manager};
use std::fs;
use tauri::{AppHandle, Manager, Runtime, Window};

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

    let url = manager::get_node_url().map_err(N8nCoreError::Installation)?;

    // 下载逻辑内部应处理好解压
    downloader::download_file(window, url, runtime_dir, "runtime".to_string())
        .await
        .map_err(N8nCoreError::Installation)
}

/// 安装 n8n 核心包 (下载 + 解压，带 SHA256 验证)
pub async fn setup_n8n<R: Runtime>(window: Window<R>) -> N8nResult<()> {
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
        .map_err(N8nCoreError::Process)
}

/// 代理健康检查
pub async fn proxy_health_check() -> N8nResult<String> {
    N8nHealthChecker::check().await
}

/// 关闭 n8n 进程
pub fn shutdown_n8n() -> N8nResult<()> {
    use crate::services::manager::PROCESS_MANAGER;

    // 1. 使用 map_err 统一错误转换，减少缩进
    let mut manager = PROCESS_MANAGER
        .lock()
        .map_err(|_| N8nCoreError::Process("PROCESS_MANAGER 锁已被毒化 (Poisoned)".into()))?;

    // 2. 执行 kill
    manager.kill_child();

    println!("[n8n] 进程已请求关闭");
    Ok(())
}
