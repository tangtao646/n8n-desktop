use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

use super::state::tunnel_running_lock;
use crate::api::n8n::{self, shutdown_n8n};
use crate::api::utils::emit_global_sync;
use crate::services::manager;

/// 使用新的隧道URL重启n8n
pub fn restart_n8n_with_env<R: Runtime>(app: &AppHandle<R>, url: &str) {
    if !url.starts_with("http") {
        return;
    }

    let tunnel_running = *tunnel_running_lock();
    if !tunnel_running {
        return;
    }

    println!("[Tunnel] 正在应用 URL 并物理重启 n8n...");
    shutdown_n8n();
    std::thread::sleep(Duration::from_millis(800));

    if let Ok(app_path) = app.path().app_data_dir() {
        let n8n_bin = app_path.join("n8n-core/node_modules/n8n/bin/n8n");
        let node_path = manager::get_node_binary_path(app_path.join("runtime"));
        let data_dir = app_path.join("n8n-data");

        // 【关键】清除前端会话和缓存，强制用户重新登录以刷新 webhook URL
        // 这会清除浏览器在 n8n 中的会话，使得用户需要重新登录
        // 重新登录后，前端会从新的 WEBHOOK_URL 环境变量获取最新的 webhook 地址
        let sessions_dir = data_dir.join(".n8n/sessions");
        if sessions_dir.exists() {
            println!("[Tunnel] 清除用户会话（强制重新登录）");
            let _ = std::fs::remove_dir_all(&sessions_dir);
        }

        let mut envs = n8n::construct_n8n_envs();
        envs.insert("N8N_HOST".to_string(), "127.0.0.1".to_string()); // 强制监听 IPv4
        envs.insert("N8N_PORT".to_string(), "5678".to_string());
        envs.insert("WEBHOOK_URL".to_string(), url.to_string());
        envs.insert("N8N_EDITOR_BASE_URL".to_string(), url.to_string());

        println!("[Tunnel] 启动 n8n...");
        match manager::start_node(node_path, n8n_bin, data_dir, envs) {
            Ok(()) => {
                println!("[Tunnel] ✓ n8n 重启成功");
                println!("[Tunnel] ✓ 新的 WEBHOOK_URL: {url}");
                println!("[Tunnel] ⚠️  请重新登录 n8n 以刷新 webhook 地址");

                // 广播全局同步事件，通知前端刷新 UI
                emit_global_sync(app);
            }
            Err(e) => println!("[Tunnel] ✗ 启动失败: {e}"),
        }
    }
}
