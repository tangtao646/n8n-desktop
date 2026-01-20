#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 调用库中定义的 run 函数
fn main() {
    // 这里的 app 是 Cargo.toml 中定义的 package name
    // 如果你的项目名叫 n8n-desktop，底层库名就是 n8n_desktop
    n8n_desktop_lib::run(); 
}