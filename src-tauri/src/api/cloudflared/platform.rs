//! 平台检测和平台特定逻辑

use std::env;

/// 平台检测器
#[derive(Debug, Clone)]
pub struct PlatformDetector;

impl PlatformDetector {
    /// 创建新的平台检测器
    pub fn new() -> Self {
        Self
    }

    /// 获取当前操作系统
    pub fn os(&self) -> &'static str {
        env::consts::OS
    }

    /// 获取当前系统架构
    pub fn arch(&self) -> &'static str {
        env::consts::ARCH
    }

    /// 检查是否为 Windows 系统
    pub fn is_windows(&self) -> bool {
        self.os() == "windows"
    }

    /// 检查是否为 macOS 系统
    pub fn is_macos(&self) -> bool {
        self.os() == "macos"
    }

    /// 检查是否为 Linux 系统
    pub fn is_linux(&self) -> bool {
        self.os() == "linux"
    }

    /// 获取平台标识字符串
    pub fn platform_identifier(&self) -> String {
        match self.os() {
            "windows" => "windows".to_string(),
            "macos" => "macos".to_string(),
            "linux" => "linux".to_string(),
            _ => "unknown".to_string(),
        }
    }

    /// 获取架构标识
    pub fn architecture_identifier(&self) -> &'static str {
        match self.arch() {
            "aarch64" => "arm64",
            "x86_64" => "amd64",
            _ => "unknown",
        }
    }

    /// 获取二进制文件扩展名
    pub fn binary_extension(&self) -> &'static str {
        if self.is_windows() {
            ".exe"
        } else {
            ""
        }
    }

    /// 获取 cloudflared 二进制文件名
    pub fn cloudflared_binary_name(&self) -> String {
        if self.is_windows() {
            "cloudflared.exe".to_string()
        } else {
            "cloudflared".to_string()
        }
    }
}

impl Default for PlatformDetector {
    fn default() -> Self {
        Self::new()
    }
}
