use tauri::{AppHandle, Manager, Runtime, Window};
use crate::services::{downloader, manager};
use crate::services::manager::PROCESS_MANAGER;

/// 指令 1：检查 n8n 是否已经安装在 AppData 目录
#[tauri::command]
pub async fn is_installed<R: Runtime>(app: AppHandle<R>) -> bool {
    app.path().app_data_dir()
        .map(|p| p.join("n8n-core/node_modules/n8n/bin/n8n").exists())
        .unwrap_or(false)
}



/// 指令 2：全自动设置 Node 运行环境 (Runtime)
/// 前端可以调用此指令并显示“正在准备运行环境...”
#[tauri::command]
pub async fn setup_runtime<R: Runtime>(window: Window<R>) -> Result<(), String> {
    let app_handle = window.app_handle();
    let runtime_dir = app_handle.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("runtime");

    // 如果运行时已存在，直接返回
    if runtime_dir.exists() {
        return Ok(());
    }

    // 根据当前操作系统获取 Node 下载链接 (调用 manager 里的逻辑)
    let url = manager::get_node_url()?;
    
    // 下载并解压到 runtime 目录
    downloader::download_n8n(window, url, runtime_dir).await
}

/// 指令 3：安装 n8n 核心包
// 在 src-tauri/src/api/commands.rs 中修改 setup_n8n 调用逻辑
#[tauri::command]
pub async fn setup_n8n<R: tauri::Runtime>(window: tauri::Window<R>) -> Result<(), String> {
    let app_handle = window.app_handle();
    
    // --- 修改点：使用加速代理 ---
    // gh-proxy.com 是国内常用的 GitHub 加速站
    
    let proxy_prefix = "https://gh-proxy.com/"; 
    let base_url = "https://github.com/tangtao646/n8n-core-builder/releases/latest/download";
    
    let platform = if cfg!(target_os = "windows") { "windows" } else { "macos" };
    let file_name = format!("n8n-core-{}.zip", platform);
    
    // 拼接成：https://gh-proxy.com/https://github.com/...
    let url = format!("{}{}/{}", proxy_prefix, base_url, file_name);

    let dest = app_handle.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("n8n-core");

    println!("正在通过加速通道下载: {}", url);

    crate::services::downloader::download_n8n(window, url, dest).await
}

/// 指令 4：启动本地 n8n 进程
#[tauri::command]
pub async fn launch_n8n<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let app_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?;
    
    // 1. 确定 Node 执行文件路径
    let runtime_dir = app_path.join("runtime");
    // 注意：如果是 tar.gz 解压出来的目录通常会有多层级，如 node-v20.../bin/node
    // 这里调用 manager 里的辅助函数定位真正的二进制文件
    let node_path = manager::get_node_binary_path(runtime_dir);

    if !node_path.exists() {
        return Err("NODE_NOT_FOUND: 请先执行 setup_runtime".to_string());
    }

    // 2. 确定 n8n 启动脚本路径
    let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
    if !n8n_bin.exists() {
        return Err("N8N_CORE_NOT_FOUND: 请先执行 setup_n8n".to_string());
    }

    // 3. 确定用户数据存储路径
    let data_dir = app_path.join("n8n-data");
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    }

    // 4. 调用底层 manager 启动进程
    manager::start_node(node_path, n8n_bin, data_dir)
}



#[tauri::command]
pub async fn proxy_health_check() -> Result<String, String> {
    let client = reqwest::Client::new();
    
    // 尝试不同的端口和地址组合
    let endpoints = [
        "http://localhost:5678/healthz",
        "http://127.0.0.1:5678/healthz",
    ];
    
    for endpoint in endpoints.iter() {
        match client.get(*endpoint).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    return Ok(format!("healthy - {}", response.status()));
                }
            }
            Err(_) => continue,
        }
    }
    
    Err("n8n 服务未响应".to_string())
}

/// 指令 5：关闭 n8n 进程
#[tauri::command]
pub fn shutdown_n8n() {
    PROCESS_MANAGER.lock().unwrap().kill_child();
}