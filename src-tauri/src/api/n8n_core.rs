use crate::services::manager::PROCESS_MANAGER;
use crate::services::{downloader, manager};
use once_cell::sync::Lazy;
use reqwest;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime, Window};

use super::utils::emit_global_sync;

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

// --- 全局状态 ---

/// 节点解禁状态
static NODES_UNLOCKED: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

// --- 内部辅助函数 ---

/// 确定最终的隧道 URL（根据隧道模式和配置）
fn determine_tunnel_url(
    tunnel_mode: &crate::api::tunnel::TunnelMode,
    _custom_domain: Option<String>,
    tunnel_url: Option<String>,
) -> Option<String> {
    match tunnel_mode {
        crate::api::tunnel::TunnelMode::Token { domain, .. } => {
            // Token 模式：返回配置的自定义域名
            Some(format!("https://{}", domain))
        }
        crate::api::tunnel::TunnelMode::Temporary => {
            // 临时隧道模式：使用 cloudflared 生成的临时 URL
            tunnel_url
        }
    }
}

/// 构造 n8n 进程的环境变量映射
pub(crate) fn construct_n8n_envs() -> HashMap<String, String> {
    use crate::api::tunnel::{TUNNEL_CONFIG, TUNNEL_RUNNING, TUNNEL_URL};
    use std::collections::HashMap;

    let mut envs = HashMap::new();

    // 条件 A: 隧道开关 tunnel_enabled
    let tunnel_enabled = {
        let running_guard = TUNNEL_RUNNING.lock().unwrap();
        *running_guard
    };

    // 卫语句：如果隧道未启用，跳过隧道相关环境变量设置
    if tunnel_enabled {
        // 获取隧道配置和 URL
        let (tunnel_mode, custom_domain) = {
            let config_guard = TUNNEL_CONFIG.lock().unwrap();
            (
                config_guard.tunnel_mode.clone(),
                config_guard.custom_domain.clone(),
            )
        };

        let tunnel_url = {
            let url_guard = TUNNEL_URL.lock().unwrap();
            url_guard.clone()
        };

        // 使用辅助函数确定最终 URL
        if let Some(final_url) = determine_tunnel_url(&tunnel_mode, custom_domain, tunnel_url) {
            envs.insert("WEBHOOK_URL".to_string(), final_url.clone());
            envs.insert("N8N_WEBHOOK_URL".to_string(), final_url.clone());
            envs.insert("N8N_EDITOR_BASE_URL".to_string(), final_url);
            envs.insert("N8N_CORS_ALLOWED_ORIGINS".to_string(), "*".to_string());
        }
        // 如果没有有效的 URL，不设置环境变量（保持 envs 为空）
    }

    // 条件 B: 节点解禁开关 nodes_unlocked
    let nodes_unlocked = {
        let unlocked_guard = NODES_UNLOCKED.lock().unwrap();
        *unlocked_guard
    };

    if nodes_unlocked {
        // 启用节点解禁：设置空列表
        envs.insert("NODES_EXCLUDE".to_string(), "[]".to_string());
        envs.insert("N8N_BLOCK_NODES".to_string(), "".to_string());
    } else {
        // 禁用节点解禁：设置预设的禁用列表
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

/// 计算文件的 SHA256 哈希值
pub fn calculate_file_sha256(file_path: &Path) -> Result<String, String> {
    use std::io::Read;

    let mut file = fs::File::open(file_path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|e| format!("读取文件失败: {}", e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// 从 GitHub API 获取最新发布的 SHA256 哈希值
/// 如果 API 请求失败（如 403 限制），返回 None 表示跳过验证
pub async fn fetch_latest_sha256(platform: &str) -> Result<Option<String>, String> {
    let client = reqwest::Client::new();
    let file_name = format!("n8n-core-{}.zip", platform);

    // 发送 API 请求
    let response = client
        .get(GITHUB_API_URL)
        .header("User-Agent", GITHUB_USER_AGENT)
        .header("Accept", GITHUB_ACCEPT_HEADER)
        .send()
        .await
        .map_err(|e| {
            println!("GitHub API 请求失败，跳过 SHA256 验证: {}", e);
            e.to_string()
        })?;

    // 检查响应状态
    if !response.status().is_success() {
        println!(
            "GitHub API 返回错误 {}，跳过 SHA256 验证",
            response.status()
        );
        return Ok(None);
    }

    // 解析响应
    let text = response.text().await.map_err(|e| {
        println!("读取 GitHub 响应失败，跳过 SHA256 验证: {}", e);
        e.to_string()
    })?;

    let json: Value = serde_json::from_str(&text).map_err(|e| {
        println!("解析 GitHub JSON 失败，跳过 SHA256 验证: {}", e);
        e.to_string()
    })?;

    // 查找对应的资产
    let assets = json["assets"].as_array().ok_or_else(|| {
        println!("GitHub 响应中缺少 assets 字段，跳过 SHA256 验证");
        "缺少 assets 字段".to_string()
    })?;

    for asset in assets {
        if asset["name"].as_str() == Some(&file_name) {
            let digest = asset["digest"].as_str().ok_or_else(|| {
                println!("资产缺少 digest 字段，跳过 SHA256 验证");
                "缺少 digest 字段".to_string()
            })?;

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

// --- Tauri 命令 ---

/// 检查 n8n 是否已经安装在 AppData 目录
pub async fn is_installed<R: Runtime>(app: AppHandle<R>) -> bool {
    app.path()
        .app_data_dir()
        .map(|p| {
            // 注意：解压后路径通常是 n8n-core/node_modules/n8n/bin/n8n
            let bin_path = p.join("n8n-core/node_modules/n8n/bin/n8n");
            bin_path.exists()
        })
        .unwrap_or(false)
}

/// 全自动设置 Node 运行环境 (Runtime)
pub async fn setup_runtime<R: Runtime>(window: Window<R>) -> Result<(), String> {
    let app_handle = window.app_handle();
    let runtime_dir = app_handle
        .path()
        .app_data_dir()
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

/// 安装 n8n 核心包 (下载 + 解压，带 SHA256 验证)
pub async fn setup_n8n<R: tauri::Runtime>(window: tauri::Window<R>) -> Result<(), String> {
    let app_handle = window.app_handle();

    let platform = match env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        _ => "unknown",
    };
    let file_name = format!("n8n-core-{}.zip", platform);

    // 构建下载 URL
    let url = format!("{}{}/{}", GH_PROXY_PREFIX, N8N_CORE_BASE_URL, file_name);

    let app_data = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let zip_dest = app_data.join(&file_name); // 使用原始文件名，而不是临时文件名
    let final_dir = app_data.join("n8n-core");

    println!("开始处理 n8n 资源包: {}", file_name);

    // 1. 获取远程 SHA256 哈希值
    println!("正在获取远程 SHA256 哈希值...");
    let remote_sha256_opt = fetch_latest_sha256(platform).await?;

    let need_download = match remote_sha256_opt {
        Some(remote_sha256) => {
            println!("成功获取远程 SHA256: {}", remote_sha256);

            // 2. 检查本地文件是否存在且哈希匹配
            if zip_dest.exists() {
                println!("本地文件已存在，正在验证完整性...");
                match calculate_file_sha256(&zip_dest) {
                    Ok(local_sha256) => {
                        if local_sha256 == remote_sha256 {
                            println!("文件完整性验证通过，跳过下载");
                            false
                        } else {
                            println!(
                                "文件哈希不匹配 (本地: {}, 远程: {})，需要重新下载",
                                local_sha256, remote_sha256
                            );
                            // 删除损坏的文件
                            fs::remove_file(&zip_dest)
                                .map_err(|e| format!("删除损坏文件失败: {}", e))?;
                            true
                        }
                    }
                    Err(e) => {
                        println!("计算本地文件哈希失败: {}，需要重新下载", e);
                        true
                    }
                }
            } else {
                println!("本地文件不存在，需要下载");
                true
            }
        }
        None => {
            println!("无法获取远程 SHA256，跳过验证直接检查文件是否存在");
            // 无法获取远程哈希，只检查文件是否存在
            if zip_dest.exists() {
                println!("本地文件已存在，跳过下载（无法验证完整性）");
                false
            } else {
                println!("本地文件不存在，需要下载");
                true
            }
        }
    };

    // 3. 如果需要下载，则下载文件
    if need_download {
        println!("开始下载资源包: {}", url);
        downloader::download_file(
            window.clone(),
            url,
            zip_dest.clone(),
            "n8n-core".to_string(),
        )
        .await?;
        println!("下载完成");
    }

    // 4. 清理旧的目录（如果存在），防止解压冲突
    if final_dir.exists() {
        fs::remove_dir_all(&final_dir).map_err(|e| format!("清理旧目录失败: {}", e))?;
    }
    fs::create_dir_all(&final_dir).map_err(|e| e.to_string())?;

    // 5. 解压到最终目录
    println!("开始解压到: {:?}", final_dir);

    // 发送解压开始事件
    let _ = window.emit(
        "extraction-start",
        crate::services::downloader::ExtractionStart {
            download_type: "n8n-core".to_string(),
        },
    );

    // 内部函数：解压 ZIP 文件
    fn extract_zip_file(archive_path: &Path, target_dir: &Path) -> Result<(), String> {
        use std::io;
        use zip::ZipArchive;

        let file = fs::File::open(archive_path).map_err(|e| e.to_string())?;
        let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

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

    extract_zip_file(&zip_dest, &final_dir)?;
    println!("解压完成");

    // 6. 保留压缩包（不删除），以便下次验证
    println!("n8n-core 安装完成");

    Ok(())
}

/// 启动本地 n8n 进程
pub async fn launch_n8n<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let app_path = app.path().app_data_dir().map_err(|e| e.to_string())?;

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

    // 1. 创建环境变量容器
    let additional_envs = construct_n8n_envs();
    manager::start_node(node_path, n8n_bin, data_dir, additional_envs)
}

pub async fn proxy_health_check() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5)) // 增加超时时间到5秒
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    // 重试逻辑：最多重试3次，每次间隔500ms
    const MAX_RETRIES: usize = 3;
    let mut last_error = None;

    for retry in 0..MAX_RETRIES {
        for endpoint in HEALTH_CHECK_ENDPOINTS.iter() {
            match client.get(*endpoint).send().await {
                Ok(response) => {
                    let status = response.status();

                    // 处理瞬态错误：502, 503, 504 可以重试
                    if status.is_success() {
                        // 尝试读取响应体以获取更多信息
                        let body_text = response.text().await.unwrap_or_default();
                        return Ok(format!("healthy - {} - {}", status, body_text));
                    } else if retry < MAX_RETRIES - 1
                        && (status == 502 || status == 503 || status == 504)
                    {
                        // 瞬态错误，记录并继续重试
                        println!(
                            "健康检查遇到瞬态错误 {}，重试 {}/{}",
                            status,
                            retry + 1,
                            MAX_RETRIES
                        );
                        last_error = Some(format!("端点 {} 返回状态码: {}", endpoint, status));
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    last_error = Some(format!("端点 {} 返回状态码: {}", endpoint, status));
                }
                Err(e) => {
                    // 网络错误也可以重试
                    if retry < MAX_RETRIES - 1 {
                        println!(
                            "健康检查网络错误: {}，重试 {}/{}",
                            e,
                            retry + 1,
                            MAX_RETRIES
                        );
                        last_error = Some(format!("端点 {} 请求失败: {}", endpoint, e));
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    last_error = Some(format!("端点 {} 请求失败: {}", endpoint, e));
                }
            }
        }

        // 如果所有端点都失败了，等待一下再重试
        if retry < MAX_RETRIES - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    Err(format!(
        "n8n 服务未响应: {}",
        last_error.unwrap_or_else(|| "未知错误".to_string())
    ))
}

/// 设置节点解禁状态并重启 n8n
pub async fn set_nodes_unlocked<R: Runtime>(
    app: AppHandle<R>,
    enabled: bool,
) -> Result<(), String> {
    // 1. 更新全局状态
    {
        let mut unlocked = NODES_UNLOCKED.lock().unwrap();
        *unlocked = enabled;
    }
    println!("[DEBUG] 节点解禁状态已设置为: {}", enabled);

    // 2. 检查 n8n 是否正在运行
    let is_running = {
        let manager = PROCESS_MANAGER.lock().unwrap();
        manager.has_child()
    };

    println!("[DEBUG] n8n 运行状态: {}", is_running);

    if !is_running {
        println!("[DEBUG] n8n 未运行，无需重启");
        return Ok(());
    }

    // 3. 获取应用路径和二进制
    let app_path = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    println!("[DEBUG] 应用路径: {:?}", app_path);

    let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
    println!("[DEBUG] n8n 二进制路径: {:?}", n8n_bin);

    if !n8n_bin.exists() {
        println!("[DEBUG] n8n 二进制文件不存在");
        return Err("N8N binary not found".to_string());
    }

    let runtime_dir = app_path.join("runtime");
    let node_path = manager::get_node_binary_path(runtime_dir);
    println!("[DEBUG] node 二进制路径: {:?}", node_path);

    if !node_path.exists() {
        println!("[DEBUG] node 二进制文件不存在");
        return Err("Node binary not found".to_string());
    }

    let data_dir = app_path.join("n8n-data");
    if !data_dir.exists() {
        println!("[DEBUG] 创建数据目录: {:?}", data_dir);
        fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    }

    // 4. 构建新的环境变量
    let additional_envs = construct_n8n_envs();
    println!("[DEBUG] 构建的环境变量: {:?}", additional_envs);

    // 5. 物理重启：杀掉再重启
    println!("[DEBUG] 正在重启 n8n 以应用节点解禁设置...");

    // 5.1 杀掉现有进程
    println!("[DEBUG] 杀掉现有进程...");
    if let Ok(mut manager) = PROCESS_MANAGER.lock() {
        manager.kill_child();
    }

    // 5.2 等待 500ms 确保端口释放
    println!("[DEBUG] 等待 500ms 确保端口释放...");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 5.3 重新启动 n8n
    println!("[DEBUG] 重新启动 n8n...");
    match manager::start_node(node_path, n8n_bin, data_dir, additional_envs) {
        Ok(_) => {
            println!("[DEBUG] n8n 已重启，节点解禁设置已应用");

            // 广播全局同步事件，通知前端刷新 UI
            emit_global_sync(&app);
            Ok(())
        }
        Err(e) => {
            println!("[DEBUG] 重启 n8n 失败: {}", e);
            Err(e)
        }
    }
}

/// 获取节点解禁状态
pub async fn get_nodes_unlocked() -> Result<bool, String> {
    let unlocked = NODES_UNLOCKED.lock().unwrap();
    Ok(*unlocked)
}

/// 关闭 n8n 进程
pub fn shutdown_n8n() {
    if let Ok(mut manager) = PROCESS_MANAGER.lock() {
        manager.kill_child();
    }
}
