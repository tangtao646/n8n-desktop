use std::fs;
use std::io;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Runtime};
use thiserror::Error;
use url::Url;
use zip::ZipArchive;

// --- 错误定义 ---

#[derive(Debug, Error)]
pub enum UtilsError {
    #[error("IO 操作失败: {0}")]
    Io(#[from] io::Error),

    #[error("ZIP 处理错误: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("系统时间倒流")]
    SystemTimeError,

    #[error("无效的 URL: {0}")]
    InvalidUrl(String),

    #[error("解压路径异常")]
    InvalidPath,

    #[error("Tauri 事件发射失败: {0}")]
    TauriError(String),
}

/// 统一 Result 类型
pub type UtilsResult<T> = Result<T, UtilsError>;

// --- 常量定义 ---

const BYTE_UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
const BYTE_BASE: f64 = 1024.0;
const CLOUDFLARE_TUNNEL_PATTERN: &str = r"https://([a-z0-9-]+\.trycloudflare\.com)";

/// 使用 2026 现代 Rust 标准的 LazyLock 预编译正则，避免重复开销
static RE_CLOUDFLARE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(CLOUDFLARE_TUNNEL_PATTERN).expect("Cloudflare Tunnel 正则表达式语法错误")
});

// --- 文件操作函数 ---
///
///  # Errors
///
/// 计算文件的 SHA256 哈希值
/// 使用 impl AsRef<Path> 使其兼容 &str, String, PathBuf
pub fn calculate_file_sha256(file_path: impl AsRef<Path>) -> UtilsResult<String> {
    let mut file = fs::File::open(file_path.as_ref())?;
    let mut hasher = Sha256::new();

    // 现代写法：io::copy 可以直接将 Read 对象拷贝到实现 Write 的 Hasher 中
    io::copy(&mut file, &mut hasher)?;

    Ok(format!("{:x}", hasher.finalize()))
}

/// 解压 ZIP 文件到目标目录
///
///  # Errors
///
/// 本函数在以下情况会返回 `UtilsError`:
/// * 无法打开指定的 ZIP 文件。
/// * ZIP 格式损坏或不支持。
/// * 目标目录没有写入权限。
/// * 磁盘空间不足导致写入失败。
pub fn extract_zip_file(
    archive_path: impl AsRef<Path>,
    target_dir: impl AsRef<Path>,
) -> UtilsResult<()> {
    let target_dir = target_dir.as_ref();
    let file = fs::File::open(archive_path.as_ref())?;
    let mut archive = ZipArchive::new(file)?;

    // 使用索引迭代器，结合 ? 自动传播错误
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }
    }
    Ok(())
}

/// 解压 GZIP 文件到目标文件
/// ///
///  # Errors
///
pub fn extract_gzip_file(
    gz_path: impl AsRef<Path>,
    target_path: impl AsRef<Path>,
) -> UtilsResult<()> {
    use flate2::read::GzDecoder;

    let gz_file = fs::File::open(gz_path.as_ref())?;
    let mut decoder = GzDecoder::new(gz_file);
    let mut target_file = fs::File::create(target_path.as_ref())?;

    io::copy(&mut decoder, &mut target_file)?;
    Ok(())
}

///  解压 TAR.GZ 文件到目标目录
/// # Errors
pub fn extract_tar_gz_file(
    tar_gz_path: impl AsRef<Path>,
    target_dir: impl AsRef<Path>,
) -> UtilsResult<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let tar_gz_file = fs::File::open(tar_gz_path.as_ref())?;
    let decoder = GzDecoder::new(tar_gz_file);
    let mut archive = Archive::new(decoder);

    archive.unpack(target_dir.as_ref())?;
    Ok(())
}

// --- 目录管理 (简化版) ---

pub fn ensure_dir_exists(dir_path: impl AsRef<Path>) -> UtilsResult<()> {
    let path = dir_path.as_ref();
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

pub fn remove_dir_if_exists(dir_path: impl AsRef<Path>) -> UtilsResult<()> {
    let path = dir_path.as_ref();
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub fn remove_file_if_exists(file_path: impl AsRef<Path>) -> UtilsResult<()> {
    let path = file_path.as_ref();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

// --- 平台识别 (保持 const) ---

pub const fn get_platform_identifier() -> &'static str {
    match () {
        () if cfg!(target_os = "windows") => "windows",
        () if cfg!(target_os = "macos") => "macos",
        () if cfg!(target_os = "linux") => "linux",
        () => "unknown",
    }
}

pub const fn get_arch_identifier() -> &'static str {
    match () {
        () if cfg!(target_arch = "x86_64") => "x64",
        () if cfg!(target_arch = "aarch64") => "arm64",
        () if cfg!(target_arch = "x86") => "x86",
        () => "unknown",
    }
}

// --- 格式化函数 ---

/// 使用函数式链式调用重构
pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }

    let exponent = (bytes as f64).log(BYTE_BASE).floor() as usize;
    let exponent = exponent.min(BYTE_UNITS.len() - 1);
    let value = bytes as f64 / BYTE_BASE.powi(exponent as i32);

    format!("{:.2} {}", value, BYTE_UNITS[exponent])
}

pub fn generate_random_string(length: usize) -> String {
    use rand::{distributions::Alphanumeric, Rng};

    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

// --- URL & Tunnel ---

pub fn is_valid_url(url: &str) -> bool {
    Url::parse(url).is_ok()
}

pub fn extract_domain_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(ToString::to_string))
}

pub fn extract_tunnel_domain_from_output(output: &str) -> Option<String> {
    RE_CLOUDFLARE
        .captures(output)
        .and_then(|caps| caps.get(1))
        .map(|m| format!("https://{}", m.as_str()))
}

// --- Tauri 全局同步 ---

pub fn emit_global_sync<R: Runtime>(app: &AppHandle<R>) -> UtilsResult<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| UtilsError::SystemTimeError)?
        .as_millis()
        .to_string();

    app.emit("app://sync-state", &timestamp)
        .map_err(|e| UtilsError::TauriError(e.to_string()))?;

    Ok(())
}
