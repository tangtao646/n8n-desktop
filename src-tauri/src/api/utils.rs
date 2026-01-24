use std::fs;
use std::path::Path;
use sha2::{Sha256, Digest};
use zip::ZipArchive;
use std::io::{self, Read, Write};

/// 计算文件的 SHA256 哈希值
pub fn calculate_file_sha256(file_path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(file_path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];
    
    loop {
        let bytes_read = file.read(&mut buffer).map_err(|e| format!("读取文件失败: {}", e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    
    Ok(format!("{:x}", hasher.finalize()))
}

/// 解压 ZIP 文件到目标目录
pub fn extract_zip_file(archive_path: &Path, target_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        if (*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).map_err(|e| e.to_string())?;
                }
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 解压 GZIP 文件到目标文件
pub fn extract_gzip_file(gz_path: &Path, target_path: &Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    
    let gz_file = fs::File::open(gz_path).map_err(|e| e.to_string())?;
    let mut decoder = GzDecoder::new(gz_file);
    let mut target_file = fs::File::create(target_path).map_err(|e| e.to_string())?;
    
    io::copy(&mut decoder, &mut target_file).map_err(|e| e.to_string())?;
    Ok(())
}

/// 解压 TAR.GZ 文件到目标目录
pub fn extract_tar_gz_file(tar_gz_path: &Path, target_dir: &Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use tar::Archive;
    
    let tar_gz_file = fs::File::open(tar_gz_path).map_err(|e| e.to_string())?;
    let decoder = GzDecoder::new(tar_gz_file);
    let mut archive = Archive::new(decoder);
    
    archive.unpack(target_dir).map_err(|e| e.to_string())?;
    Ok(())
}

/// 确保目录存在，如果不存在则创建
pub fn ensure_dir_exists(dir_path: &Path) -> Result<(), String> {
    if !dir_path.exists() {
        fs::create_dir_all(dir_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 删除目录及其所有内容
pub fn remove_dir_if_exists(dir_path: &Path) -> Result<(), String> {
    if dir_path.exists() {
        fs::remove_dir_all(dir_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 删除文件（如果存在）
pub fn remove_file_if_exists(file_path: &Path) -> Result<(), String> {
    if file_path.exists() {
        fs::remove_file(file_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

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

/// 格式化字节数为人类可读的字符串
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let base = 1024_f64;
    let exponent = (bytes as f64).log(base).floor() as u32;
    let exponent = exponent.min((UNITS.len() - 1) as u32);
    let value = bytes as f64 / base.powi(exponent as i32);
    
    format!("{:.2} {}", value, UNITS[exponent as usize])
}

/// 生成随机字符串
pub fn generate_random_string(length: usize) -> String {
    use rand::Rng;
    use rand::distributions::Alphanumeric;
    
    let rng = rand::thread_rng();
    rng.sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

/// 验证 URL 是否有效
pub fn is_valid_url(url: &str) -> bool {
    url::Url::parse(url).is_ok()
}

/// 从 URL 中提取域名
pub fn extract_domain_from_url(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
}

/// 从 Cloudflare Tunnel 输出中提取域名
pub fn extract_tunnel_domain_from_output(output: &str) -> Option<String> {
    use regex::Regex;
    
    // 匹配 trycloudflare.com 域名
    let re = Regex::new(r"https://([a-z0-9-]+\.trycloudflare\.com)").unwrap();
    re.captures(output)
        .and_then(|caps| caps.get(1))
        .map(|m| format!("https://{}", m.as_str()))
}