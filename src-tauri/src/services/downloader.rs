use futures_util::StreamExt;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{Emitter, Runtime, Window};

// 引入 Unix 专属权限库
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

// --- 常量定义 ---

/// 用户代理字符串
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36";

/// 进度更新最小增量（百分比）
const PROGRESS_UPDATE_MIN_INCREMENT: f64 = 0.5;

/// 进度更新最小时间间隔（毫秒）
const PROGRESS_UPDATE_MIN_INTERVAL_MS: u64 = 150;

/// Unix 文件可执行权限模式
#[cfg(unix)]
const EXECUTABLE_PERMISSIONS_MODE: u32 = 0o755; // rwxr-xr-x

/// 存档文件扩展名
const ARCHIVE_EXTENSIONS: [&str; 3] = [".tar.gz", ".tgz", ".zip"];

// --- 数据结构 ---

#[derive(Clone, serde::Serialize)]
struct Progress {
    progress: f64,
    download_type: String,
}

#[derive(Clone, serde::Serialize)]
pub struct ExtractionStart {
    pub download_type: String,
}

/// 下载配置参数
struct DownloadConfig {
    url: String,
    destination: PathBuf,
    download_type: String,
    is_archive: bool,
    destination_is_file: bool,
}

// --- 主下载函数 ---
///
/// # Errors
///
pub async fn download_file<R: Runtime>(
    window: Window<R>,
    url: String,
    dest: PathBuf,
    download_type: String,
) -> Result<(), String> {
    let config = analyze_download_config(&url, &dest, download_type);

    process_downloaded_content(&window, &config).await?;
    finalize_download(&window, &config);

    Ok(())
}

// --- 辅助函数 ---

/// 分析下载配置
fn analyze_download_config(url: &str, dest: &Path, download_type: String) -> DownloadConfig {
    let pure_url = url.split('?').next().unwrap_or(url).to_lowercase();
    let is_archive = ARCHIVE_EXTENSIONS.iter().any(|ext| pure_url.ends_with(ext));
    let destination_is_file = dest.extension().is_some() && dest.parent().is_some();

    DownloadConfig {
        url: url.to_string(),
        destination: dest.to_path_buf(),
        download_type,
        is_archive,
        destination_is_file,
    }
}

/// 执行带进度显示的下载
async fn download_with_progress<R: Runtime>(
    window: &Window<R>,
    config: &DownloadConfig,
) -> Result<Vec<u8>, String> {
    let client = create_http_client()?;
    let response = fetch_http_response(&client, &config.url).await?;
    validate_http_response(&response)?;

    let total_size = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut downloaded = 0;

    let mut last_emit_time = Instant::now();
    let mut last_emit_progress = -1.0;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("下载流错误: {e}"))?;
        buffer.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            update_progress_if_needed(
                window,
                downloaded,
                total_size,
                &config.download_type,
                &mut last_emit_time,
                &mut last_emit_progress,
            );
        }
    }

    Ok(buffer)
}

/// 创建 HTTP 客户端
fn create_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

/// 获取 HTTP 响应
async fn fetch_http_response(
    client: &reqwest::Client,
    url: &str,
) -> Result<reqwest::Response, String> {
    client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP 请求失败 '{url}': {e}"))
}

/// 验证 HTTP 响应状态
fn validate_http_response(response: &reqwest::Response) -> Result<(), String> {
    if !response.status().is_success() {
        return Err(format!("下载失败: HTTP {}", response.status()));
    }
    Ok(())
}

/// 根据需要更新进度显示
fn update_progress_if_needed<R: Runtime>(
    window: &Window<R>,
    downloaded: u64,
    total: u64,
    download_type: &str,
    last_emit_time: &mut Instant,
    last_emit_progress: &mut f64,
) {
    let downloaded_u32 = u32::try_from(downloaded).unwrap_or(u32::MAX);
    let total_u32 = u32::try_from(total).unwrap_or(u32::MAX);
    let progress = (f64::from(downloaded_u32) / f64::from(total_u32)) * 100.0;
    let time_elapsed =
        last_emit_time.elapsed() >= Duration::from_millis(PROGRESS_UPDATE_MIN_INTERVAL_MS);
    let progress_increased = progress - *last_emit_progress >= PROGRESS_UPDATE_MIN_INCREMENT;

    if time_elapsed || progress_increased {
        let _ = window.emit(
            "download-progress",
            Progress {
                progress,
                download_type: download_type.to_string(),
            },
        );

        *last_emit_progress = progress;
        *last_emit_time = Instant::now();
    }
}

/// 处理下载的内容（解压或保存）
async fn process_downloaded_content<R: Runtime>(
    window: &Window<R>,
    config: &DownloadConfig,
) -> Result<(), String> {
    let buffer = download_with_progress(window, config).await?;

    if config.is_archive && !config.destination_is_file {
        handle_archive_download(window, config, &buffer)
    } else {
        handle_file_download(config, &buffer)
    }
}

/// 处理存档文件下载
fn handle_archive_download<R: Runtime>(
    window: &Window<R>,
    config: &DownloadConfig,
    buffer: &[u8],
) -> Result<(), String> {
    prepare_destination_directory(&config.destination)?;
    notify_extraction_start(window, &config.download_type);

    extract_archive(buffer, &config.destination)?;
    flatten_single_directory(&config.destination)?;
    fix_permissions_if_needed(&config.destination)?;

    Ok(())
}

/// 处理普通文件下载
fn handle_file_download(config: &DownloadConfig, buffer: &[u8]) -> Result<(), String> {
    ensure_parent_directory_exists(&config.destination)?;
    write_file_content(&config.destination, buffer)
}

/// 准备目标目录
fn prepare_destination_directory(dest: &Path) -> Result<(), String> {
    if dest.exists() {
        fs::remove_dir_all(dest)
            .map_err(|e| format!("清理目录 '{}' 失败: {}", dest.display(), e))?;
    }

    fs::create_dir_all(dest).map_err(|e| format!("创建目录 '{}' 失败: {}", dest.display(), e))
}

/// 通知解压开始
fn notify_extraction_start<R: Runtime>(window: &Window<R>, download_type: &str) {
    let _ = window.emit(
        "extraction-start",
        ExtractionStart {
            download_type: download_type.to_string(),
        },
    );
}

/// 确保父目录存在
fn ensure_parent_directory_exists(file_path: &Path) -> Result<(), String> {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建父目录 '{}' 失败: {}", parent.display(), e))?;
    }
    Ok(())
}

/// 写入文件内容
fn write_file_content(file_path: &Path, content: &[u8]) -> Result<(), String> {
    fs::write(file_path, content)
        .map_err(|e| format!("写入文件 '{}' 失败: {}", file_path.display(), e))
}

/// 解压存档文件
fn extract_archive(buffer: &[u8], dest: &Path) -> Result<(), String> {
    if is_tar_gz_archive(buffer) {
        extract_tar_gz(buffer, dest)
    } else {
        extract_zip(buffer, dest)
    }
}

/// 检查是否为 tar.gz 格式
fn is_tar_gz_archive(buffer: &[u8]) -> bool {
    buffer.starts_with(&[0x1f, 0x8b]) // GZIP 魔数
}

/// 解压 ZIP 文件
fn extract_zip(buffer: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(buffer)).map_err(|e| format!("ZIP 格式非法: {e}"))?;

    archive
        .extract(dest)
        .map_err(|e| format!("ZIP 解压失败: {e}"))
}

/// 解压 TAR.GZ 文件
fn extract_tar_gz(buffer: &[u8], dest: &Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let tar_gz = GzDecoder::new(Cursor::new(buffer));
    let mut archive = Archive::new(tar_gz);

    archive
        .unpack(dest)
        .map_err(|e| format!("TAR.GZ 解压失败: {e}"))
}

/// 展平单层目录结构
fn flatten_single_directory(dest: &Path) -> Result<(), String> {
    let entries: Vec<_> = fs::read_dir(dest)
        .map_err(|e| format!("读取目录 '{}' 失败: {}", dest.display(), e))?
        .filter_map(Result::ok)
        .collect();

    let directories: Vec<_> = entries
        .iter()
        .filter(|entry| {
            let path = entry.path();
            let is_dir = path.is_dir();
            let is_hidden = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'));
            is_dir && !is_hidden
        })
        .collect();

    // 如果只有一个非隐藏目录，展平它
    if directories.len() == 1 {
        let sub_dir = &directories[0].path();
        flatten_directory_contents(sub_dir, dest)?;
        fs::remove_dir(sub_dir)
            .map_err(|e| format!("删除目录 '{}' 失败: {}", sub_dir.display(), e))?;
    }

    Ok(())
}

/// 展平目录内容
fn flatten_directory_contents(source_dir: &Path, target_dir: &Path) -> Result<(), String> {
    let entries = fs::read_dir(source_dir)
        .map_err(|e| format!("读取目录 '{}' 失败: {}", source_dir.display(), e))?;

    for entry_result in entries {
        let entry = entry_result.map_err(|e| format!("读取目录条目失败: {}", e))?;

        let from = entry.path();
        let to = target_dir.join(entry.file_name());

        fs::rename(&from, &to).map_err(|e| {
            format!(
                "移动文件 '{}' 到 '{}' 失败: {}",
                from.display(),
                to.display(),
                e
            )
        })?;
    }

    Ok(())
}

/// 修复权限（仅 Unix 系统）
fn fix_permissions_if_needed(dest: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        fix_recursive_permissions(dest).map_err(|e| format!("权限修复失败: {}", e))?;

        #[cfg(target_os = "macos")]
        remove_macos_quarantine_attribute(dest);
    }

    Ok(())
}

/// 递归修复权限（仅 Unix）
#[cfg(unix)]
fn fix_recursive_permissions(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            fix_recursive_permissions(&entry.path())?;
        }
    } else {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(EXECUTABLE_PERMISSIONS_MODE);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

/// 移除 macOS 隔离属性
#[cfg(target_os = "macos")]
fn remove_macos_quarantine_attribute(path: &Path) {
    let _ = path.to_str().and_then(|path_str| {
        std::process::Command::new("xattr")
            .args(["-cr", path_str])
            .spawn()
            .ok()
    });
}

/// 完成下载
fn finalize_download<R: Runtime>(window: &Window<R>, config: &DownloadConfig) {
    let _ = window.emit(
        "download-progress",
        Progress {
            progress: 100.0,
            download_type: config.download_type.clone(),
        },
    );
}
