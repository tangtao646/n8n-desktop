//! n8n 核心错误定义模块
//!
//! 提供统一的错误类型和结果别名。

use reqwest;
use serde_json;
use std::io;
use thiserror::Error;
use zip::result::ZipError;

/// n8n 核心功能错误类型
#[derive(Debug, Error)]
pub enum N8nCoreError {
    /// IO 操作失败
    #[error("IO 操作失败: {0}")]
    Io(#[from] io::Error),

    /// 网络请求失败
    #[error("网络请求失败: {0}")]
    Network(#[from] reqwest::Error),

    /// JSON 解析失败
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// ZIP 处理错误
    #[error("ZIP 处理错误: {0}")]
    Zip(#[from] ZipError),

    /// 文件哈希验证失败
    #[error("文件哈希验证失败: 期望 {expected}, 实际 {actual}")]
    HashMismatch { expected: String, actual: String },

    /// 安装失败
    #[error("安装失败: {0}")]
    Installation(String),

    /// 进程管理失败
    #[error("进程管理失败: {0}")]
    Process(String),

    /// 路径操作失败
    #[error("路径操作失败: {0}")]
    Path(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// 服务未响应
    #[error("服务未响应: {0}")]
    ServiceUnavailable(String),

    /// Tauri 相关错误
    #[error("Tauri 错误: {0}")]
    Tauri(String),
}

/// 统一 Result 类型
pub type N8nResult<T> = Result<T, N8nCoreError>;
