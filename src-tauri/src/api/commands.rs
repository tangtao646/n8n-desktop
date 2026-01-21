use tauri::{AppHandle, Emitter, Manager, Runtime, Window};
use crate::services::{downloader, manager};
use crate::services::manager::PROCESS_MANAGER;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 指令 1：检查 n8n 是否已经安装在 AppData 目录
#[tauri::command]
pub async fn is_installed<R: Runtime>(app: AppHandle<R>) -> bool {
    app.path().app_data_dir()
        .map(|p| {
            // 注意：解压后路径通常是 n8n-core/node_modules/n8n/bin/n8n
            let bin_path = p.join("n8n-core/node_modules/n8n/bin/n8n");
            bin_path.exists()
        })
        .unwrap_or(false)
}

/// 指令 2：全自动设置 Node 运行环境 (Runtime)
#[tauri::command]
pub async fn setup_runtime<R: Runtime>(window: Window<R>) -> Result<(), String> {
    let app_handle = window.app_handle();
    let runtime_dir = app_handle.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("runtime");

    // 如果运行时已存在且二进制文件可找到，跳过
    if manager::get_node_binary_path(runtime_dir.clone()).exists() {
        return Ok(());
    }

    let url = manager::get_node_url()?;
    
    // 下载逻辑内部应处理好解压
    downloader::download_file(window, url, runtime_dir, "runtime".to_string()).await
}

/// 指令 3：安装 n8n 核心包 (下载 + 解压)
#[tauri::command]
pub async fn setup_n8n<R: tauri::Runtime>(window: tauri::Window<R>) -> Result<(), String> {
    let app_handle = window.app_handle();
    
    let proxy_prefix = "https://gh-proxy.com/"; 
    let base_url = "https://github.com/tangtao646/n8n-core-builder/releases/latest/download";
    
    let platform = if cfg!(target_os = "windows") { "windows" } else { "macos" };
    let file_name = format!("n8n-core-{}.zip", platform);
    let url = format!("{}{}/{}", proxy_prefix, base_url, file_name);

    let app_data = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let zip_dest = app_data.join("n8n-core-temp.zip");
    let final_dir = app_data.join("n8n-core");

    println!("开始下载资源包: {}", url);

    // 1. 下载压缩包到临时位置
    let window_clone = window.clone();
    downloader::download_file(window, url, zip_dest.clone(), "n8n-core".to_string()).await?;
    
    // 2. 清理旧的目录（如果存在），防止解压冲突
    if final_dir.exists() {
        fs::remove_dir_all(&final_dir).map_err(|e| format!("清理旧目录失败: {}", e))?;
    }
    fs::create_dir_all(&final_dir).map_err(|e| e.to_string())?;

    // 3. 解压到最终目录
    println!("下载完成，开始解压到: {:?}", final_dir);
    
    // 发送解压开始事件
    let _ = window_clone.emit("extraction-start", crate::services::downloader::ExtractionStart {
        download_type: "n8n-core".to_string(),
    });
    
    extract_zip_file(&zip_dest, &final_dir)?;

    // 4. 清理临时压缩包
    let _ = fs::remove_file(zip_dest);

    Ok(())
}

/// 指令 4：启动本地 n8n 进程
#[tauri::command]
pub async fn launch_n8n<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let app_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?;
    
    let runtime_dir = app_path.join("runtime");
    let node_path = manager::get_node_binary_path(runtime_dir);

    if !node_path.exists() {
        return Err("NODE_NOT_FOUND: 请先执行 setup_runtime".to_string());
    }

    let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
    if !n8n_bin.exists() {
        return Err("N8N_CORE_NOT_FOUND: 请先执行 setup_n8n".to_string());
    }

    let data_dir = app_path.join("n8n-data");
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    }

    manager::start_node(node_path, n8n_bin, data_dir)
}

#[tauri::command]
pub async fn proxy_health_check() -> Result<String, String> {
    let client = reqwest::Client::new();
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
    if let Ok(mut manager) = PROCESS_MANAGER.lock() {
        manager.kill_child();
    }
}

// --- 内部辅助函数：解压 ZIP ---
fn extract_zip_file(archive_path: &Path, target_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        if (*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).map_err(|e| e.to_string())?;
                }
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}