pub mod api;
pub mod i18n;
pub mod services;

use tauri::RunEvent;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
            api::commands::set_nodes_unlocked,
            api::commands::get_nodes_unlocked,
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
            api::commands::apply_tunnel_config,
            // cloudflared 管理
            api::commands::download_cloudflared,
            api::commands::check_cloudflared_version,
            // 侧边栏管理
            api::commands::toggle_sidebar,
            // 国际化
            api::commands::set_language,
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
        .run(|_app, event| handle_app_run_event(event));
}

fn handle_app_run_event(event: tauri::RunEvent) {
    if let RunEvent::ExitRequested { .. } = event {
        // 在应用退出前，直接调用 shutdown_n8n
        api::commands::shutdown_n8n();

        // 关闭隧道
        #[cfg(unix)]
        let _ = std::process::Command::new("pkill")
            .args(&["-f", "cloudflared"])
            .output();
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(&["/F", "/IM", "cloudflared.exe", "/T"])
            .output();

        println!("Application exiting: Cleaned up n8n and tunnel processes.");
    }
}
