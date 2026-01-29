use regex::Regex;
use tauri::{AppHandle, Emitter, Runtime};

use super::config::update_last_url;
use super::models::TunnelEvent;
use super::n8n_integration::restart_n8n_with_env;
use super::state::tunnel_url_lock;

/// 从 cloudflared 输出中提取隧道 ID
#[allow(dead_code)]
pub fn extract_tunnel_id_from_output(output: &str) -> Option<String> {
    // 正则匹配隧道 ID 格式：类似 xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx 或短 ID
    let re =
        Regex::new(r"[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}|[a-z0-9]{32}")
            .expect("Failed to compile tunnel ID regex");
    re.find(output).map(|m| m.as_str().to_string())
}

/// 核心修复：处理隧道URL匹配逻辑
pub fn process_tunnel_url_match<R: Runtime>(
    url_match: &regex::Match,
    is_temporary: bool,
    app_clone: &AppHandle<R>,
) -> bool {
    let url = url_match.as_str().to_string();
    handle_tunnel_url(&url, is_temporary, app_clone)
}

/// 处理隧道URL（直接传入URL字符串，不依赖正则匹配）
pub fn handle_tunnel_url<R: Runtime>(
    url: &str,
    is_temporary: bool,
    app_clone: &AppHandle<R>,
) -> bool {
    // 【修复点】：移除对 "cloudflare.com" 的排除逻辑，只验证后缀
    let is_valid_url = if is_temporary {
        url.ends_with(".trycloudflare.com")
    } else {
        (url.starts_with("http://") || url.starts_with("https://"))
            && !url.ends_with(".trycloudflare.com")
    };

    if !is_valid_url {
        return false;
    }

    println!("[Tunnel] 成功捕获并验证 URL: {url}");

    {
        let mut url_guard = tunnel_url_lock();
        *url_guard = Some(url.to_string());
    }

    let _ = update_last_url(app_clone, url);
    // 【修复点】：状态统一为小写，确保前端匹配
    let _ = app_clone.emit(
        "tunnel-event",
        TunnelEvent::with_url("online", url.to_string()),
    );

    restart_n8n_with_env(app_clone, url);
    true
}
