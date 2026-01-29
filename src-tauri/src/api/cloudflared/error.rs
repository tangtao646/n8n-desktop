//! Cloudflared 错误类型定义

use thiserror::Error;

/// Cloudflared 操作错误
#[derive(Error, Debug)]
pub enum CloudflaredError {
    /// 平台不支持错误
    #[error("当前操作系统平台暂不支持: {0}")]
    UnsupportedPlatform(String),

    /// 文件系统错误
    #[error("文件系统操作失败: {0}")]
    FileSystem(String),

    /// 网络错误
    #[error("网络操作失败: {0}")]
    Network(String),

    /// 下载错误
    #[error("下载失败: {0}")]
    Download(String),

    /// 解压错误
    #[error("解压失败: {0}")]
    Extraction(String),

    /// 版本检查错误
    #[error("版本检查失败: {0}")]
    VersionCheck(String),

    /// 路径查找错误
    #[error("路径查找失败: {0}")]
    PathNotFound(String),

    /// 缓存错误
    #[error("缓存操作失败: {0}")]
    Cache(String),

    /// 权限错误
    #[error("权限设置失败: {0}")]
    Permission(String),

    /// 命令执行错误
    #[error("命令执行失败: {0}")]
    CommandExecution(String),

    /// 序列化/反序列化错误
    #[error("数据序列化失败: {0}")]
    Serialization(String),

    /// 其他错误
    #[error("未知错误: {0}")]
    Other(String),
}

impl CloudflaredError {
    /// 创建平台不支持错误
    pub fn unsupported_platform(os: &str) -> Self {
        Self::UnsupportedPlatform(format!("操作系统: {}", os))
    }

    /// 创建文件系统错误
    pub fn filesystem<S: Into<String>>(msg: S) -> Self {
        Self::FileSystem(msg.into())
    }

    /// 创建下载错误
    pub fn download<S: Into<String>>(msg: S) -> Self {
        Self::Download(msg.into())
    }

    /// 创建路径查找错误
    pub fn path_not_found<S: Into<String>>(msg: S) -> Self {
        Self::PathNotFound(msg.into())
    }

    /// 创建序列化错误
    pub fn serialization<S: Into<String>>(msg: S) -> Self {
        Self::Serialization(msg.into())
    }

    /// 创建缓存错误
    pub fn cache<S: Into<String>>(msg: S) -> Self {
        Self::Cache(msg.into())
    }

    /// 创建解压错误
    pub fn extraction<S: Into<String>>(msg: S) -> Self {
        Self::Extraction(msg.into())
    }

    /// 创建命令执行错误
    pub fn command_execution<S: Into<String>>(msg: S) -> Self {
        Self::CommandExecution(msg.into())
    }

    /// 创建权限错误
    pub fn permission<S: Into<String>>(msg: S) -> Self {
        Self::Permission(msg.into())
    }
}

/// 结果类型别名
pub type CloudflaredResult<T> = Result<T, CloudflaredError>;
