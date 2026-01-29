use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use super::models::TunnelConfig;

/// 全局隧道URL状态
pub(crate) static TUNNEL_URL: LazyLock<Arc<Mutex<Option<String>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

/// 全局隧道运行状态
pub(crate) static TUNNEL_RUNNING: LazyLock<Arc<Mutex<bool>>> =
    LazyLock::new(|| Arc::new(Mutex::new(false)));

/// 全局隧道配置状态
pub(crate) static TUNNEL_CONFIG: LazyLock<Arc<Mutex<TunnelConfig>>> =
    LazyLock::new(|| Arc::new(Mutex::new(TunnelConfig::default())));

/// 安全获取 TUNNEL_URL 的锁
pub fn tunnel_url_lock() -> MutexGuard<'static, Option<String>> {
    TUNNEL_URL.lock().expect("TUNNEL_URL mutex poisoned")
}

/// 安全获取 TUNNEL_RUNNING 的锁
pub fn tunnel_running_lock() -> MutexGuard<'static, bool> {
    TUNNEL_RUNNING
        .lock()
        .expect("TUNNEL_RUNNING mutex poisoned")
}

/// 安全获取 TUNNEL_CONFIG 的锁
pub fn tunnel_config_lock() -> MutexGuard<'static, TunnelConfig> {
    TUNNEL_CONFIG.lock().expect("TUNNEL_CONFIG mutex poisoned")
}
