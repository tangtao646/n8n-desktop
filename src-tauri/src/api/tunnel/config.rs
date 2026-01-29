use tauri::{AppHandle, Manager, Runtime};

use super::models::TunnelConfig;
use super::state::{tunnel_config_lock, tunnel_url_lock};

/// 加载隧道配置
pub fn load_tunnel_config<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let config_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("tunnel_config.json");

    if !config_path.exists() {
        return Ok(());
    }

    let config_json = std::fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let config: TunnelConfig = serde_json::from_str(&config_json).map_err(|e| e.to_string())?;

    let mut config_guard = tunnel_config_lock();
    *config_guard = config;

    if let Some(last_url) = &config_guard.last_url {
        let mut url_guard = tunnel_url_lock();
        *url_guard = Some(last_url.clone());
    }

    Ok(())
}

/// 保存隧道配置
pub fn save_tunnel_config<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let config = tunnel_config_lock().clone();
    let config_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("tunnel_config.json");

    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let config_json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, config_json).map_err(|e| e.to_string())?;

    Ok(())
}

/// 更新最后使用的URL
pub fn update_last_url<R: Runtime>(app: &AppHandle<R>, url: &str) -> Result<(), String> {
    {
        let mut config_guard = tunnel_config_lock();
        config_guard.last_url = Some(url.to_string());
        config_guard.created_at = chrono::Local::now().to_rfc3339();
    }

    save_tunnel_config(app)
}
