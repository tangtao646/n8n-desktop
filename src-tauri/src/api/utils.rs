use std::fs;
use std::path::Path;
use sha2::{Sha256, Digest};
use zip::ZipArchive;
use std::io::{self, Read, Write};
use regex::Regex;
use url::Url;

// --- 常量定义 ---

/// 文件读取缓冲区大小（8KB）
const READ_BUFFER_SIZE: usize = 8192;

/// 字节格式化单位
const BYTE_UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

/// 字节计算基数
const BYTE_BASE: f64 = 1024.0;

/// Cloudflare Tunnel 域名正则表达式模式
const CLOUDFLARE_TUNNEL_PATTERN: &str = r"https://([a-z0-9-]+\.trycloudflare\.com)";

// --- 类型别名 ---

/// 统一错误类型
pub type UtilsResult<T> = Result<T, String>;

// --- 文件操作函数 ---

/// 计算文件的 SHA256 哈希值
pub fn calculate_file_sha256(file_path: &Path) -> UtilsResult<String> {
    let mut file = fs::File::open(file_path)
        .map_err(|e| format!("无法打开文件 '{}': {}", file_path.display(), e))?;
    
    let mut hasher = Sha256::new();
    let mut buffer = [0; READ_BUFFER_SIZE];
    
    loop {
        let bytes_read = file.read(&mut buffer)
            .map_err(|e| format!("读取文件 '{}' 失败: {}", file_path.display(), e))?;
        
        if bytes_read == 0 {
            break;
        }
        
        hasher.update(&buffer[..bytes_read]);
    }
    
    Ok(format!("{:x}", hasher.finalize()))
}

/// 解压 ZIP 文件到目标目录
pub fn extract_zip_file(archive_path: &Path, target_dir: &Path) -> UtilsResult<()> {
    let file = fs::File::open(archive_path)
        .map_err(|e| format!("无法打开 ZIP 文件 '{}': {}", archive_path.display(), e))?;
    
    let mut archive = ZipArchive::new(file)
        .map_err(|e| format!("ZIP 文件格式错误 '{}': {}", archive_path.display(), e))?;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index)
            .map_err(|e| format!("无法读取 ZIP 条目 {}: {}", index, e))?;
        
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        if (*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath)
                .map_err(|e| format!("无法创建目录 '{}': {}", outpath.display(), e))?;
        } else {
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("无法创建父目录 '{}': {}", parent.display(), e))?;
                }
            }
            
            let mut outfile = fs::File::create(&outpath)
                .map_err(|e| format!("无法创建文件 '{}': {}", outpath.display(), e))?;
            
            io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("无法写入文件 '{}': {}", outpath.display(), e))?;
        }
    }
    
    Ok(())
}

/// 解压 GZIP 文件到目标文件
pub fn extract_gzip_file(gz_path: &Path, target_path: &Path) -> UtilsResult<()> {
    use flate2::read::GzDecoder;
    
    let gz_file = fs::File::open(gz_path)
        .map_err(|e| format!("无法打开 GZIP 文件 '{}': {}", gz_path.display(), e))?;
    
    let mut decoder = GzDecoder::new(gz_file);
    let mut target_file = fs::File::create(target_path)
        .map_err(|e| format!("无法创建目标文件 '{}': {}", target_path.display(), e))?;
    
    io::copy(&mut decoder, &mut target_file)
        .map_err(|e| format!("无法解压 GZIP 文件 '{}': {}", gz_path.display(), e))?;
    
    Ok(())
}

/// 解压 TAR.GZ 文件到目标目录
pub fn extract_tar_gz_file(tar_gz_path: &Path, target_dir: &Path) -> UtilsResult<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;
    
    let tar_gz_file = fs::File::open(tar_gz_path)
        .map_err(|e| format!("无法打开 TAR.GZ 文件 '{}': {}", tar_gz_path.display(), e))?;
    
    let decoder = GzDecoder::new(tar_gz_file);
    let mut archive = Archive::new(decoder);
    
    archive.unpack(target_dir)
        .map_err(|e| format!("无法解压 TAR.GZ 文件 '{}': {}", tar_gz_path.display(), e))?;
    
    Ok(())
}

// --- 目录和文件管理函数 ---

/// 确保目录存在，如果不存在则创建
pub fn ensure_dir_exists(dir_path: &Path) -> UtilsResult<()> {
    if !dir_path.exists() {
        fs::create_dir_all(dir_path)
            .map_err(|e| format!("无法创建目录 '{}': {}", dir_path.display(), e))?;
    }
    Ok(())
}

/// 删除目录及其所有内容（如果存在）
pub fn remove_dir_if_exists(dir_path: &Path) -> UtilsResult<()> {
    if dir_path.exists() {
        fs::remove_dir_all(dir_path)
            .map_err(|e| format!("无法删除目录 '{}': {}", dir_path.display(), e))?;
    }
    Ok(())
}

/// 删除文件（如果存在）
pub fn remove_file_if_exists(file_path: &Path) -> UtilsResult<()> {
    if file_path.exists() {
        fs::remove_file(file_path)
            .map_err(|e| format!("无法删除文件 '{}': {}", file_path.display(), e))?;
    }
    Ok(())
}

// --- 平台和架构识别函数 ---

/// 获取当前平台标识符
pub fn get_platform_identifier() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

/// 获取当前架构标识符
pub fn get_arch_identifier() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "unknown"
    }
}

/// 获取完整的平台-架构标识符
pub fn get_platform_arch_identifier() -> String {
    format!("{}-{}", get_platform_identifier(), get_arch_identifier())
}

// --- 格式化函数 ---

/// 格式化字节数为人类可读的字符串
pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let exponent = (bytes as f64).log(BYTE_BASE).floor() as u32;
    let exponent = exponent.min((BYTE_UNITS.len() - 1) as u32);
    let value = bytes as f64 / BYTE_BASE.powi(exponent as i32);
    
    format!("{:.2} {}", value, BYTE_UNITS[exponent as usize])
}

/// 生成指定长度的随机字符串
pub fn generate_random_string(length: usize) -> String {
    use rand::Rng;
    use rand::distributions::Alphanumeric;
    
    let rng = rand::thread_rng();
    rng.sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

// --- URL 处理函数 ---

/// 验证 URL 是否有效
pub fn is_valid_url(url: &str) -> bool {
    Url::parse(url).is_ok()
}

/// 从 URL 中提取域名
pub fn extract_domain_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_string()))
}

/// 从 Cloudflare Tunnel 输出中提取域名
pub fn extract_tunnel_domain_from_output(output: &str) -> Option<String> {
    let re = Regex::new(CLOUDFLARE_TUNNEL_PATTERN)
        .expect("Cloudflare Tunnel 正则表达式编译失败");
    
    re.captures(output)
        .and_then(|caps| caps.get(1))
        .map(|matched| format!("https://{}", matched.as_str()))
}

// --- 测试模块 ---
#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs::File;
    use std::io::Write;
    
    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }
    
    #[test]
    fn test_platform_identifier() {
        let platform = get_platform_identifier();
        assert!(!platform.is_empty());
    }
    
    #[test]
    fn test_arch_identifier() {
        let arch = get_arch_identifier();
        assert!(!arch.is_empty());
    }
    
    #[test]
    fn test_is_valid_url() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://localhost:8080"));
        assert!(!is_valid_url("not-a-url"));
    }
    
    #[test]
    fn test_extract_domain_from_url() {
        assert_eq!(
            extract_domain_from_url("https://example.com/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_domain_from_url("http://localhost:8080"),
            Some("localhost".to_string())
        );
        assert!(extract_domain_from_url("not-a-url").is_none());
    }
    
    #[test]
    fn test_extract_tunnel_domain_from_output() {
        let output = "Tunnel established at https://abc123.trycloudflare.com";
        assert_eq!(
            extract_tunnel_domain_from_output(output),
            Some("https://abc123.trycloudflare.com".to_string())
        );
        
        let invalid_output = "No tunnel here";
        assert!(extract_tunnel_domain_from_output(invalid_output).is_none());
    }
    
    #[test]
    fn test_generate_random_string() {
        let str1 = generate_random_string(10);
        let str2 = generate_random_string(10);
        
        assert_eq!(str1.len(), 10);
        assert_eq!(str2.len(), 10);
        assert_ne!(str1, str2); // 随机字符串应该不同
    }
    
    #[test]
    fn test_ensure_and_remove_dir() {
        let temp_dir = temp_dir().join("test_utils_dir");
        
        // 测试创建目录
        assert!(ensure_dir_exists(&temp_dir).is_ok());
        assert!(temp_dir.exists());
        
        // 测试删除目录
        assert!(remove_dir_if_exists(&temp_dir).is_ok());
        assert!(!temp_dir.exists());
        
        // 测试删除不存在的目录（应该成功）
        assert!(remove_dir_if_exists(&temp_dir).is_ok());
    }
    
    #[test]
    fn test_remove_file_if_exists() {
        let temp_file = temp_dir().join("test_utils_file.txt");
        
        // 创建测试文件
        File::create(&temp_file).unwrap().write_all(b"test").unwrap();
        assert!(temp_file.exists());
        
        // 测试删除文件
        assert!(remove_file_if_exists(&temp_file).is_ok());
        assert!(!temp_file.exists());
        
        // 测试删除不存在的文件（应该成功）
        assert!(remove_file_if_exists(&temp_file).is_ok());
    }
}