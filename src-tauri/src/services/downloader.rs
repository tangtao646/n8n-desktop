use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Instant, Duration};
use tauri::{Emitter, Runtime, Window};
use futures_util::StreamExt;

// 引入 Unix 专属权限库
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, serde::Serialize)]
struct Progress {
    progress: f64,
    download_type: String,
}

#[derive(Clone, serde::Serialize)]
pub struct ExtractionStart {
    pub download_type: String,
}

pub async fn download_file<R: Runtime>(
    window: Window<R>,
    url: String,
    dest: PathBuf,
    download_type: String,
) -> Result<(), String> {
    // 1. 创建具备 User-Agent 的客户端
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .build()
        .map_err(|e| e.to_string())?;

    let res = client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("下载失败: HTTP {}", res.status()));
    }

    let total = res.content_length().unwrap_or(0);
    let mut downloaded = 0;
    let mut stream = res.bytes_stream();
    let mut buffer = Vec::new();

    let mut last_emit_time = Instant::now();
    let mut last_emit_progress = -1.0;

    // 3. 下载流处理
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buffer.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        
        if total > 0 {
            let progress = (downloaded as f64 / total as f64) * 100.0;
            if progress - last_emit_progress >= 0.5 || last_emit_time.elapsed() >= Duration::from_millis(150) {
                let _ = window.emit("download-progress", Progress {
                    progress,
                    download_type: download_type.clone(),
                });
                last_emit_progress = progress;
                last_emit_time = Instant::now();
            }
        }
    }

    // 4. 判断目标是文件还是目录
    let pure_url = url.split('?').next().unwrap_or(&url).to_lowercase();
    let is_archive = pure_url.ends_with(".tar.gz") || pure_url.ends_with(".tgz") || pure_url.ends_with(".zip");
    
    // 判断 dest 是文件还是目录：如果以存档扩展名结尾且看起来像文件名，则保存为文件
    let dest_is_file = dest.extension().is_some() && dest.parent().is_some();
    
    if is_archive && !dest_is_file {
        // dest 是目录：清理并准备目录，然后解压
        if dest.exists() {
            fs::remove_dir_all(&dest).ok();
        }
        fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

        // 发送解压开始事件
        let _ = window.emit("extraction-start", ExtractionStart {
            download_type: download_type.clone(),
        });

        // 根据后缀名解压
        if pure_url.ends_with(".tar.gz") || pure_url.ends_with(".tgz") {
            extract_tgz(&buffer, &dest)?;
        } else {
            extract_zip(&buffer, &dest)?;
        }

        // 处理解压后的“套娃”文件夹
        flatten_directory(&dest)?;
    } else {
        // 目标是文件：写入文件
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&dest, &buffer).map_err(|e| e.to_string())?;
    }

    // --- 新增：7. 权限修复与隔离属性移除 (仅限 Unix/macOS) ---
    // 仅当解压了存档时才执行权限修复
    if is_archive && !dest_is_file {
        #[cfg(unix)]
        {
            // 递归赋予可执行权限 (755)
            fix_recursive_permissions(&dest).map_err(|e| format!("权限修复失败: {}", e))?;
            
            // 如果是 macOS，移除 Quarantine 属性，防止系统拦截二进制文件执行
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("xattr")
                    .args(["-cr", dest.to_str().unwrap()])
                    .spawn();
            }
        }
    }

    // 8. 完成
    let _ = window.emit("download-progress", Progress {
        progress: 100.0,
        download_type: download_type.clone(),
    });
    Ok(())
}

/// 递归为目录下的所有文件赋予可执行权限 (仅 Unix)
#[cfg(unix)]
fn fix_recursive_permissions(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            fix_recursive_permissions(&entry.path())?;
        }
    } else {
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755); // rwxr-xr-x
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn extract_zip(buffer: &[u8], dest: &PathBuf) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(buffer))
        .map_err(|e| format!("Zip格式非法: {}", e))?;
    archive.extract(dest).map_err(|e| format!("Zip解压失败: {}", e))
}

fn extract_tgz(buffer: &[u8], dest: &PathBuf) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use tar::Archive;
    let tar_gz = GzDecoder::new(Cursor::new(buffer));
    let mut archive = Archive::new(tar_gz);
    archive.unpack(dest).map_err(|e| format!("Tar.gz解压失败: {}", e))
}

fn flatten_directory(dest: &PathBuf) -> Result<(), String> {
    let entries: Vec<_> = fs::read_dir(dest).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .collect();

    let dir_entries: Vec<_> = entries.iter()
        .filter(|e| {
            let path = e.path();
            let is_dir = path.is_dir();
            let file_name = path.file_name().and_then(|n| n.to_str());
            let is_hidden = file_name.map(|n| n.starts_with('.')).unwrap_or(false);
            is_dir && !is_hidden
        })
        .collect();

    if dir_entries.len() == 1 {
        let sub_dir = dir_entries[0].path();
        let sub_entries = fs::read_dir(&sub_dir).map_err(|e| e.to_string())?;
        
        for entry in sub_entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let from = entry.path();
            let to = dest.join(from.file_name().unwrap());
            fs::rename(from, to).map_err(|e| e.to_string())?;
        }
        fs::remove_dir(sub_dir).ok();
    }
    Ok(())
}