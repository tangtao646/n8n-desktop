use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};

use super::config::{load_tunnel_config, save_tunnel_config};
use super::models::{
    TunnelConfig, TunnelError, TunnelEvent, TunnelHealth, TunnelHealthStatus, TunnelMode,
};
use super::n8n_integration::restart_n8n_with_env;
use super::runner::{TunnelMonitor, TunnelRunner};
use super::state::{tunnel_config_lock, tunnel_running_lock, tunnel_url_lock};
use crate::i18n;
use crate::services::manager::PROCESS_MANAGER;

/// 启动隧道
pub async fn start_tunnel<R: Runtime>(
    app: AppHandle<R>,
    cloudflared_path: String,
) -> Result<(), String> {
    // 1. 环境准备
    TunnelRunner::cleanup_prev_processes();
    tokio::time::sleep(Duration::from_millis(500)).await;
    app.emit("tunnel-event", TunnelEvent::new("connecting"))
        .ok();

    // 2. 启动进程
    let cfg = tunnel_config_lock().clone();
    let runner = TunnelRunner::new(cloudflared_path, cfg.tunnel_mode.clone());
    let mut child = runner.spawn().map_err(|e| e.to_string())?;

    let stderr = child.stderr.take().ok_or(i18n::t("tunnel.cannot_capture_stderr"))?;
    *tunnel_running_lock() = true;

    // 3. 异步监听 (非阻塞)
    let monitor = TunnelMonitor {
        app,
        mode: cfg.tunnel_mode,
    };
    tokio::spawn(async move {
        monitor.watch(stderr).await;
    });

    Ok(())
}

/// 停止隧道
pub fn stop_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    println!("[Tunnel] 正在停止隧道...");

    #[cfg(unix)]
    let _ = Command::new("pkill").args(["-f", "cloudflared"]).output();
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "cloudflared.exe", "/T"])
        .output();

    *tunnel_url_lock() = None;
    *tunnel_running_lock() = false;

    println!("[Tunnel] 隧道已停止，清理缓存");
    app.emit("tunnel-event", TunnelEvent::new("offline")).ok();
    Ok(())
}

/// 获取隧道状态
pub fn get_tunnel_status<R: Runtime>(_app: AppHandle<R>) -> Result<TunnelEvent, String> {
    let url = tunnel_url_lock().clone();
    let running = *tunnel_running_lock();
    let status = if running {
        if url.is_some() {
            "online"
        } else {
            "connecting"
        }
    } else {
        "offline"
    };
    Ok(TunnelEvent {
        status: status.into(),
        url,
        progress: None,
        message: None,
    })
}

/// 复制隧道URL到剪贴板
pub fn copy_tunnel_url<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let url = tunnel_url_lock().clone().ok_or("No URL")?;
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(stdin) = c.stdin.as_mut() {
            stdin.write_all(url.as_bytes()).ok();
        }
    }
    #[cfg(target_os = "windows")]
    {
        let mut c = Command::new("clip")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(stdin) = c.stdin.as_mut() {
            stdin.write_all(url.as_bytes()).ok();
        }
    }
    app.emit("tunnel-event", TunnelEvent::with_url("online", url))
        .ok();
    Ok(())
}

/// 获取隧道配置
pub fn get_tunnel_config<R: Runtime>(app: AppHandle<R>) -> Result<TunnelConfig, String> {
    load_tunnel_config(&app)?;
    Ok(tunnel_config_lock().clone())
}

/// 更新隧道配置
pub fn update_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    auto_start: Option<bool>,
    last_url: Option<String>,
    custom_domain: Option<String>,
    use_custom_domain: Option<bool>,
) -> Result<(), String> {
    {
        let mut cfg = tunnel_config_lock();
        if let Some(v) = auto_start {
            cfg.auto_start = v;
        }
        if let Some(v) = last_url {
            cfg.last_url = Some(v);
        }
        if let Some(v) = custom_domain {
            cfg.custom_domain = Some(v);
        }
        if let Some(v) = use_custom_domain {
            cfg.use_custom_domain = v;
        }
    }
    save_tunnel_config(&app)
}

/// 检查隧道健康状态
pub fn check_tunnel_health<R: Runtime>(_app: AppHandle<R>) -> Result<TunnelHealth, String> {
    let url = tunnel_url_lock().clone();
    let running = *tunnel_running_lock();
    let (status, msg) = if running {
        if url.is_some() {
            (TunnelHealthStatus::Healthy, i18n::t("tunnel.health.healthy"))
        } else {
            (TunnelHealthStatus::Connecting, i18n::t("tunnel.health.connecting"))
        }
    } else {
        (TunnelHealthStatus::Stopped, i18n::t("tunnel.health.stopped"))
    };
    Ok(TunnelHealth {
        status,
        ping_ms: Some(100),
        last_check: chrono::Local::now().to_rfc3339(),
        message: msg.into(),
    })
}

/// 恢复隧道
pub fn recover_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    stop_tunnel(app.clone())?;
    Ok(())
}

/// 应用隧道配置
pub fn apply_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    tunnel_mode: TunnelMode,
    _custom_domain: Option<String>,
    _tunnel_token: Option<String>,
) -> Result<(), String> {
    // 验证输入
    match &tunnel_mode {
        TunnelMode::Token { token, domain } => {
            if token.trim().is_empty() {
                return Err(i18n::t("tunnel.token_mode.needs_token"));
            }
            if domain.trim().is_empty() {
                return Err(i18n::t("tunnel.token_mode.needs_domain"));
            }
        }
        TunnelMode::Temporary => {
            // 临时隧道模式无需验证
        }
    }

    // 更新配置
    {
        let mut cfg = tunnel_config_lock();
        cfg.tunnel_mode = tunnel_mode;
        // 对于 Token 模式，需要更新 custom_domain 字段
        if let TunnelMode::Token { domain, .. } = &cfg.tunnel_mode {
            cfg.custom_domain = Some(domain.clone());
        }
        // tunnel_token 字段现在在 TunnelMode::Token 中，不需要单独存储
    }

    save_tunnel_config(&app)?;

    // 如果隧道正在运行，需要重启
    if PROCESS_MANAGER
        .lock()
        .expect("PROCESS_MANAGER mutex poisoned")
        .has_child()
    {
        let url = tunnel_url_lock().clone().unwrap_or_default();
        restart_n8n_with_env(&app, &url);
        // 注意：restart_n8n_with_env 内部已经包含 emit_global_sync 调用
    }

    Ok(())
}

/// 获取隧道错误列表
pub fn get_tunnel_errors<R: Runtime>(_app: AppHandle<R>) -> Result<Vec<TunnelError>, String> {
    Ok(vec![])
}

/// 启动时加载隧道配置
pub fn load_tunnel_config_on_start<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    load_tunnel_config(&app)
}
