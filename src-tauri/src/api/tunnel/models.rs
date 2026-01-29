use chrono;
use serde::{Deserialize, Serialize};

/// 隧道事件结构，用于前端通信
#[derive(Clone, Serialize)]
pub struct TunnelEvent {
    pub status: String,
    pub url: Option<String>,
    pub progress: Option<f64>,
    pub message: Option<String>,
}

/// 隧道模式枚举
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum TunnelMode {
    /// 临时隧道模式，每次启动生成随机URL
    Temporary,
    /// Token 模式，使用 Cloudflare Tunnel Token 和自定义域名
    Token { token: String, domain: String },
}

impl TunnelEvent {
    pub fn new(status: &str) -> Self {
        Self {
            status: status.to_string(),
            url: None,
            progress: None,
            message: None,
        }
    }

    pub fn with_url(status: &str, url: String) -> Self {
        Self {
            status: status.to_string(),
            url: Some(url),
            progress: None,
            message: None,
        }
    }

    pub fn with_progress(status: &str, progress: f64) -> Self {
        Self {
            status: status.to_string(),
            url: None,
            progress: Some(progress),
            message: None,
        }
    }

    pub fn with_message(status: &str, message: String) -> Self {
        Self {
            status: status.to_string(),
            url: None,
            progress: None,
            message: Some(message),
        }
    }
}

/// 隧道配置结构
#[derive(Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub last_url: Option<String>,
    pub auto_start: bool,
    pub created_at: String,
    pub custom_domain: Option<String>,
    pub use_custom_domain: bool,
    pub tunnel_mode: TunnelMode, // 使用枚举替代字符串
    pub tunnel_token: Option<String>,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            last_url: None,
            auto_start: false,
            created_at: chrono::Local::now().to_rfc3339(),
            custom_domain: None,
            use_custom_domain: false,
            tunnel_mode: TunnelMode::Temporary,
            tunnel_token: None,
        }
    }
}

/// 隧道健康状态枚举
#[derive(Clone, Serialize)]
pub enum TunnelHealthStatus {
    Healthy,
    Connecting,
    Stopped,
    Error,
}

/// 隧道健康检查结果
#[derive(Clone, Serialize)]
pub struct TunnelHealth {
    pub status: TunnelHealthStatus,
    pub ping_ms: Option<u32>,
    pub last_check: String,
    pub message: String,
}

/// 隧道错误信息
#[derive(Clone, Serialize)]
pub struct TunnelError {
    pub timestamp: String,
    pub message: String,
    pub severity: String,
}
