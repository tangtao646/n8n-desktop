//! Cloudflared 安装和解压逻辑

use std::path::Path;
use std::process::Command;

use crate::api::cloudflared::error::{CloudflaredError, CloudflaredResult};

/// 安装管理器
#[derive(Debug, Clone)]
pub struct InstallManager;

impl InstallManager {
    /// 创建新的安装管理器
    pub fn new() -> Self {
        Self
    }

    /// 提取 cloudflared 版本号
    pub fn extract_version(binary_path: &str) -> CloudflaredResult<Option<String>> {
        let output = Command::new(binary_path)
            .arg("--version")
            .output()
            .map_err(|error| {
                CloudflaredError::command_execution(format!(
                    "执行 cloudflared 版本检查失败: {}",
                    error
                ))
            })?;

        let version_output = String::from_utf8_lossy(&output.stdout);

        // 使用正则表达式提取版本号
        use once_cell::sync::Lazy;
        use regex::Regex;

        static VERSION_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"version (\d+\.\d+\.\d+)").expect("版本正则表达式编译失败"));

        let version = VERSION_REGEX
            .captures(&version_output)
            .and_then(|captures| captures.get(1))
            .map(|matched| matched.as_str().to_string());

        Ok(version)
    }

    /// 验证 cloudflared 二进制文件
    pub fn validate_binary(binary_path: &Path) -> CloudflaredResult<bool> {
        if !binary_path.exists() {
            return Ok(false);
        }

        // 检查文件是否可执行
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(binary_path).map_err(|error| {
                CloudflaredError::filesystem(format!("获取文件元数据失败: {}", error))
            })?;

            if metadata.permissions().mode() & 0o111 == 0 {
                return Ok(false);
            }
        }

        // 尝试执行版本检查
        match Self::extract_version(&binary_path.to_string_lossy()) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false), // 能执行但没有版本号
            Err(_) => Ok(false),   // 执行失败
        }
    }

    /// 安装 cloudflared 到系统路径（需要管理员权限）
    #[cfg(unix)]
    pub fn install_to_system_path(binary_path: &Path) -> CloudflaredResult<()> {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let system_path = Path::new("/usr/local/bin/cloudflared");

        if system_path.exists() {
            fs::remove_file(system_path).map_err(|error| {
                CloudflaredError::filesystem(format!("删除现有文件失败: {}", error))
            })?;
        }

        fs::copy(binary_path, system_path)
            .map_err(|error| CloudflaredError::filesystem(format!("复制文件失败: {}", error)))?;

        // 设置权限
        fs::set_permissions(
            system_path,
            fs::Permissions::from_mode(
                crate::api::cloudflared::config::UNIX_EXECUTABLE_PERMISSIONS,
            ),
        )
        .map_err(|error| CloudflaredError::permission(format!("设置权限失败: {}", error)))?;

        Ok(())
    }

    /// 安装 cloudflared 到系统路径（Windows）
    #[cfg(windows)]
    pub fn install_to_system_path(binary_path: &Path) -> CloudflaredResult<()> {
        use std::fs;
        use winreg::enums::*;
        use winreg::RegKey;

        let system_path = Path::new("C:\\Program Files\\cloudflared\\cloudflared.exe");

        // 创建目录
        if let Some(parent) = system_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CloudflaredError::filesystem(format!("创建目录失败: {}", error))
            })?;
        }

        // 复制文件
        fs::copy(binary_path, system_path)
            .map_err(|error| CloudflaredError::filesystem(format!("复制文件失败: {}", error)))?;

        // 添加到 PATH（需要管理员权限）
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let environment = hkcu
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .map_err(|error| CloudflaredError::filesystem(format!("打开注册表失败: {}", error)))?;

        let current_path: String = environment
            .get_value("Path")
            .unwrap_or_else(|_| "".to_string());

        let program_files_path = system_path
            .parent()
            .and_then(|p| p.to_str())
            .ok_or_else(|| CloudflaredError::filesystem("无法获取程序路径".to_string()))?;

        if !current_path.contains(program_files_path) {
            let new_path = if current_path.is_empty() {
                program_files_path.to_string()
            } else {
                format!("{};{}", current_path, program_files_path)
            };

            environment.set_value("Path", &new_path).map_err(|error| {
                CloudflaredError::filesystem(format!("设置注册表失败: {}", error))
            })?;
        }

        Ok(())
    }
}

impl Default for InstallManager {
    fn default() -> Self {
        Self::new()
    }
}
