//! 主命令模块 - 整合所有功能模块

// 导入 Tauri 相关依赖
use tauri::{AppHandle, Runtime, Window};

// 从父模块导入功能模块
use crate::api::{cloudflared, n8n, tunnel};
use crate::i18n;

// 重新导出类型定义，但不重新导出函数（避免宏冲突）
pub use cloudflared::{CloudflaredCacheInfo, CloudflaredVersionInfo};
pub use tunnel::{TunnelConfig, TunnelError, TunnelEvent, TunnelHealth, TunnelHealthStatus};

/// 向后兼容的包装函数 - 检查 n8n 是否已安装
#[tauri::command]
pub async fn is_installed<R: Runtime>(app: AppHandle<R>) -> bool {
    n8n::is_installed(app)
}

/// 向后兼容的包装函数 - 设置 Node 运行环境
#[tauri::command]
pub async fn setup_runtime<R: Runtime>(window: Window<R>) -> Result<(), String> {
    n8n::setup_runtime(window).await.map_err(|e| e.to_string())
}

/// 向后兼容的包装函数 - 安装 n8n 核心包
#[tauri::command]
pub async fn setup_n8n<R: tauri::Runtime>(window: tauri::Window<R>) -> Result<(), String> {
    n8n::setup_n8n(window).await.map_err(|e| e.to_string())
}

/// 向后兼容的包装函数 - 启动本地 n8n 进程
#[tauri::command]
pub async fn launch_n8n<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    n8n::launch_n8n(app).map_err(|e| e.to_string())
}

/// 向后兼容的包装函数 - 关闭 n8n 进程
#[tauri::command]
pub fn shutdown_n8n() -> Result<(), String> {
    n8n::shutdown_n8n().map_err(|e| e.to_string())
}

/// 向后兼容的包装函数 - 启动 Cloudflare Tunnel
#[tauri::command]
pub async fn start_tunnel<R: Runtime>(
    app: AppHandle<R>,
    cloudflared_path: String,
) -> Result<(), String> {
    tunnel::start_tunnel(app, cloudflared_path).await
}

/// 向后兼容的包装函数 - 停止 Cloudflare Tunnel
#[tauri::command]
pub async fn stop_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    tunnel::stop_tunnel(app)
}

/// 向后兼容的包装函数 - 获取隧道状态
#[tauri::command]
pub async fn get_tunnel_status<R: Runtime>(app: AppHandle<R>) -> Result<TunnelEvent, String> {
    tunnel::get_tunnel_status(app)
}

/// 向后兼容的包装函数 - 复制隧道URL到剪贴板
#[tauri::command]
pub async fn copy_tunnel_url<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    tunnel::copy_tunnel_url(app)
}

/// 向后兼容的包装函数 - 下载 cloudflared 二进制文件
#[tauri::command]
pub async fn download_cloudflared<R: Runtime>(
    app: AppHandle<R>,
    window: Window<R>,
) -> Result<(), String> {
    cloudflared::download_cloudflared(app, window).await
}

/// 向后兼容的包装函数 - 检查 cloudflared 版本
#[tauri::command]
pub async fn check_cloudflared_version<R: Runtime>(
    app: AppHandle<R>,
) -> Result<CloudflaredVersionInfo, String> {
    cloudflared::check_cloudflared_version(app)
}

/// 向后兼容的包装函数 - 获取隧道配置
#[tauri::command]
pub async fn get_tunnel_config<R: Runtime>(app: AppHandle<R>) -> Result<TunnelConfig, String> {
    tunnel::get_tunnel_config(app)
}

/// 向后兼容的包装函数 - 更新隧道配置
#[tauri::command]
pub async fn update_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    auto_start: Option<bool>,
    last_url: Option<String>,
    custom_domain: Option<String>,
    use_custom_domain: Option<bool>,
) -> Result<(), String> {
    tunnel::update_tunnel_config(app, auto_start, last_url, custom_domain, use_custom_domain)
}

/// 向后兼容的包装函数 - 应用启动时加载配置
#[tauri::command]
pub async fn load_tunnel_config_on_start<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    tunnel::load_tunnel_config_on_start(app)
}

/// 向后兼容的包装函数 - 检查隧道健康状况
#[tauri::command]
pub async fn check_tunnel_health<R: Runtime>(app: AppHandle<R>) -> Result<TunnelHealth, String> {
    tunnel::check_tunnel_health(app)
}

/// 向后兼容的包装函数 - 尝试恢复隧道连接
#[tauri::command]
pub async fn recover_tunnel<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    tunnel::recover_tunnel(app)
}

/// 向后兼容的包装函数 - 获取错误日志
#[tauri::command]
pub async fn get_tunnel_errors<R: Runtime>(app: AppHandle<R>) -> Result<Vec<TunnelError>, String> {
    tunnel::get_tunnel_errors(app)
}

/// 向后兼容的包装函数 - 代理健康检查
#[tauri::command]
pub async fn proxy_health_check() -> Result<String, String> {
    n8n::proxy_health_check().await.map_err(|e| e.to_string())
}

/// 设置节点解禁状态
#[tauri::command]
pub async fn set_nodes_unlocked<R: Runtime>(
    app: AppHandle<R>,
    enabled: bool,
) -> Result<(), String> {
    n8n::set_nodes_unlocked(app, enabled)
        .await
        .map_err(|e| e.to_string())
}

/// 获取节点解禁状态
#[tauri::command]
pub async fn get_nodes_unlocked() -> Result<bool, String> {
    n8n::get_nodes_unlocked().map_err(|e| e.to_string())
}

/// 应用新的隧道配置（支持两种模式）
#[tauri::command]
pub async fn apply_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    tunnel_mode: serde_json::Value, // 改为 serde_json::Value 以支持灵活的输入格式
    custom_domain: Option<String>,
    tunnel_token: Option<String>,
) -> Result<(), String> {
    // 将 serde_json::Value 转换为 TunnelMode 枚举
    let mode = match tunnel_mode {
        // 处理字符串格式："Temporary"
        serde_json::Value::String(s) => match s.as_str() {
            "Temporary" | "temporary" => tunnel::TunnelMode::Temporary,
            "Token" | "token" => {
                let token = tunnel_token.ok_or(i18n::t("tunnel.token_mode.needs_token"))?;
                let domain = custom_domain
                    .clone()
                    .ok_or(i18n::t("tunnel.token_mode.needs_domain"))?;
                tunnel::TunnelMode::Token { token, domain }
            }
            _ => return Err(format!("{}: {s}", i18n::t("tunnel.unknown_mode"))),
        },
        // 处理对象格式：{ Token: { token, domain } }
        serde_json::Value::Object(obj) => {
            if let Some(token_obj) = obj.get("Token") {
                if let Some(token_data) = token_obj.as_object() {
                    let token = token_data
                        .get("token")
                        .and_then(|v| v.as_str())
                        .ok_or(i18n::t("tunnel.token_obj.needs_token_field"))?
                        .to_string();
                    let domain = token_data
                        .get("domain")
                        .and_then(|v| v.as_str())
                        .ok_or(i18n::t("tunnel.token_obj.needs_domain_field"))?
                        .to_string();
                    tunnel::TunnelMode::Token { token, domain }
                } else {
                    return Err(i18n::t("tunnel.token_field.must_be_object"));
                }
            } else {
                return Err(i18n::t("tunnel.mode_obj.must_contain_token"));
            }
        }
        _ => return Err(i18n::t("tunnel.mode.must_be_string_or_object")),
    };

    tunnel::apply_tunnel_config(
        app,
        mode,
        custom_domain,
        None, // tunnel_token 现在包含在 TunnelMode::Token 中
    )
}

/// 切换侧边栏状态
#[tauri::command]
pub async fn toggle_sidebar<R: Runtime>(_window: Window<R>) -> Result<bool, String> {
    // 这里可以添加实际的布局调整逻辑
    // 目前先返回一个简单的状态切换
    Ok(true)
}

/// 设置应用语言（前端调用）
#[tauri::command]
pub fn set_language(lang: String) {
    let l = match lang.as_str() {
        "en" | "EN" | "En" => i18n::Lang::En,
        _ => i18n::Lang::Zh,
    };
    i18n::set_language(l);
}
