//! n8n 核心常量定义模块
//!
//! 包含所有配置常量、URL 和超时设置。

use std::time::Duration;

/// 默认禁用的节点列表
pub const DEFAULT_BLOCKED_NODES: &str = r#"["n8n-nodes-base.executeCommand"]"#;
pub const DEFAULT_BLOCKED_NODES_NAMES: &str = "executeCommand";

/// GitHub API 相关常量
pub const GITHUB_API_URL: &str =
    "https://api.github.com/repos/tangtao646/n8n-core-builder/releases/latest";
pub const GITHUB_USER_AGENT: &str = "n8n-desktop";
pub const GITHUB_ACCEPT_HEADER: &str = "application/vnd.github.v3+json";

/// 代理下载前缀
pub const GH_PROXY_PREFIX: &str = "https://gh-proxy.com/";
pub const N8N_CORE_BASE_URL: &str =
    "https://github.com/tangtao646/n8n-core-builder/releases/latest/download";

/// 健康检查端点
pub const HEALTH_CHECK_ENDPOINTS: [&str; 4] = [
    "http://localhost:5678/healthz",
    "http://127.0.0.1:5678/healthz",
    "http://localhost:5678/",
    "http://127.0.0.1:5678/",
];

/// 健康检查配置
pub const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
pub const HEALTH_CHECK_RETRIES: usize = 3;
pub const HEALTH_CHECK_RETRY_DELAY: Duration = Duration::from_millis(500);
