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
            api::commands::is_installed,
            api::commands::setup_runtime,
            api::commands::setup_n8n,
            api::commands::launch_n8n,
            api::commands::proxy_health_check,
            api::commands::shutdown_n8n
        ]);

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