pub mod api;
pub mod services;
use std::env;

use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env::set_var("NODES_EXCLUDE", "[]");
    env::set_var("N8N_BLOCK_NODES", "");//解除节点禁用
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            // n8n 核心功能
            api::commands::is_installed,
            api::commands::setup_runtime,
            api::commands::setup_n8n,
            api::commands::launch_n8n,
            api::commands::shutdown_n8n,
            api::commands::proxy_health_check,
            // 隧道功能
            api::commands::start_tunnel,
            api::commands::stop_tunnel,
            api::commands::get_tunnel_status,
            api::commands::copy_tunnel_url,
            api::commands::get_tunnel_config,
            api::commands::update_tunnel_config,
            api::commands::load_tunnel_config_on_start,
            api::commands::check_tunnel_health,
            api::commands::recover_tunnel,
            api::commands::get_tunnel_errors,
            // cloudflared 管理
            api::commands::download_cloudflared,
            api::commands::check_cloudflared_version,
            api::commands::clear_cloudflared_cache,
            // 侧边栏管理
            api::commands::toggle_sidebar,
        ])
        .on_window_event(|_window, event| {
            match event {
                tauri::WindowEvent::Resized(_) => {
                    // 窗口大小改变时，可以在这里触发布局更新
                    // 由于我们是单Webview，可以通过事件通知前端
                    // 这里暂时只记录日志
                    println!("Window resized, potential layout update needed");
                }
                _ => {}
            }
        });

    builder
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                // 在应用退出前，直接调用 shutdown_n8n
                // 它现在是同步的，所以能保证在退出前完成
                api::commands::shutdown_n8n();
            }
        });
}