//! 国际化模块 — 中英文文本映射
//!
//! 提供运行时语言切换能力，将硬编码文本抽取为可翻译的键值对。
//! 通过 `set_language` command 由前端设置当前语言。

use std::sync::atomic::{AtomicU8, Ordering};

/// 语言枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Lang {
    En = 0,
    Zh = 1,
}

impl Lang {
    /// 从 u8 解码
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::En,
            _ => Self::Zh,
        }
    }
}

/// 全局语言标志（0=En, 1=Zh）
static CURRENT_LANG: AtomicU8 = AtomicU8::new(Lang::Zh as u8);

/// 设置当前语言
pub fn set_language(lang: Lang) {
    CURRENT_LANG.store(lang as u8, Ordering::SeqCst);
}

/// 获取当前语言
pub fn current_lang() -> Lang {
    Lang::from_u8(CURRENT_LANG.load(Ordering::SeqCst))
}

/// 根据当前全局语言返回对应文本
/// `key` 为映射表中的键，若未找到则返回键本身
pub fn t(key: &str) -> String {
    let lang = current_lang();
    match lang {
        Lang::En => en(key).unwrap_or(key).to_string(),
        Lang::Zh => zh(key).unwrap_or(key).to_string(),
    }
}

/// 英文映射
fn en(key: &str) -> Option<&'static str> {
    Some(match key {
        // ── Tunnel / 隧道 ──
        "tunnel.token_mode.needs_token" => "Token mode requires a Cloudflare Tunnel Token",
        "tunnel.token_mode.needs_domain" => "Token mode requires a custom domain",
        "tunnel.unknown_mode" => "Unknown tunnel mode",
        "tunnel.token_obj.needs_token_field" => "Token object must contain 'token' field",
        "tunnel.token_obj.needs_domain_field" => "Token object must contain 'domain' field",
        "tunnel.token_field.must_be_object" => "Token field must be an object",
        "tunnel.mode_obj.must_contain_token" => "Tunnel mode object must contain 'Token' or other valid field",
        "tunnel.mode.must_be_string_or_object" => "Tunnel mode must be a string or an object",
        "tunnel.cannot_capture_stderr" => "Cannot capture stderr stream",

        // ── Tunnel event / 隧道事件 ──
        "tunnel.event.connecting" => "Connecting",
        "tunnel.event.online" => "Online",
        "tunnel.event.offline" => "Offline",
        "tunnel.health.healthy" => "Healthy",
        "tunnel.health.connecting" => "Connecting",
        "tunnel.health.stopped" => "Stopped",

        // ── Cloudflared ──
        "cloudflared.download.needs_window" => "Download cloudflared requires a window handle",
        "cloudflared.download.failed" => "Download cloudflared failed",
        "cloudflared.extraction.binary_not_found" => "cloudflared binary not found in archive",
        "cloudflared.unsupported_platform" => "Unsupported platform",

        // ── Node.js / Runtime ──
        "node.not_found" => "Node.js not found. Please run setup_runtime first",
        "n8n.core_not_found" => "n8n core not found. Please run setup_n8n first",
        "n8n.binary_not_found" => "n8n binary not found",
        "runtime.unsupported_platform" => "Unsupported platform architecture",

        // ── n8n State / 状态 ──
        "n8n.state.process_manager_poisoned" => "Process manager lock poisoned",
        "n8n.state.no_check_run" => "No health check performed yet",
        "n8n.state.network_error" => "Network error",
        "n8n.state.http_status" => "HTTP status code",
        "n8n.state.healthy" => "healthy",
        "n8n.state.cannot_read_body" => "Cannot read response body",

        // ── Filesystem / 文件系统 ──
        "fs.cannot_create_dir" => "Cannot create directory",
        "fs.cannot_read_dir" => "Cannot read directory",
        "fs.cannot_delete_dir" => "Cannot delete directory",
        "fs.cannot_delete_file" => "Cannot delete file",
        "fs.cannot_write_file" => "Cannot write file",
        "fs.cannot_rename_file" => "Cannot rename/move file",
        "fs.cannot_create_parent_dir" => "Cannot create parent directory",
        "fs.cannot_read_cache" => "Cannot read cache file",

        // ── Download / 下载 ──
        "download.clean_dir_failed" => "Failed to clean directory",
        "download.create_dir_failed" => "Failed to create directory",
        "download.zip_invalid" => "Invalid ZIP format",
        "download.zip_extract_failed" => "ZIP extraction failed",
        "download.tar_extract_failed" => "TAR.GZ extraction failed",
        "download.stream_error" => "Download stream error",
        "download.http_error" => "HTTP request failed",
        "download.http_status_error" => "Download failed with HTTP status",
        "download.create_client_failed" => "Failed to create HTTP client",
        "download.read_dir_entry_failed" => "Failed to read directory entry",
        "download.permission_fix_failed" => "Permission fix failed",

        // ── N8n Process / 进程 ──
        "process.spawn_failed" => "Failed to spawn n8n process",
        "process.invalid_user_data_path" => "User data directory path contains invalid characters",

        // ── Cloudflared path ──
        "cloudflared.path.not_found_in_system" => "cloudflared executable not found in system PATH",
        "cloudflared.path.resource_dir_failed" => "Failed to get resource directory",
        "cloudflared.path.app_data_dir_failed" => "Failed to get app data directory",
        "cloudflared.path.read_cache_failed" => "Failed to read cache info file",
        "cloudflared.path.parse_cache_failed" => "Failed to parse cache info",

        // ── Cloudflared install / cache ──
        "cloudflared.version.exec_failed" => "Failed to execute cloudflared version check",
        "cloudflared.file_meta_failed" => "Failed to get file metadata",
        "cloudflared.copy_failed" => "Failed to copy file",
        "cloudflared.permission_set_failed" => "Failed to set permissions",
        "cloudflared.registry_failed" => "Failed to access registry",
        "cloudflared.cache.serialize_failed" => "Failed to serialize cache info",
        "cloudflared.cache.write_failed" => "Failed to write cache file",
        "cloudflared.cache.get_download_dir_failed" => "Failed to get download directory",

                // ── Filesystem / 文件系统 (带占位符) ──
        "fs.cannot_delete_temp_file" => "Cannot delete temporary file",
        "fs.cannot_delete_existing_file" => "Cannot delete existing file",
        "fs.cannot_move_to" => "Cannot move file to",
        "fs.cannot_open_archive" => "Cannot open archive",
        "fs.cannot_read_archive_entry" => "Cannot read archive entry",
        "fs.cannot_get_entry_path" => "Cannot get entry path",
        "fs.cannot_create_target_file" => "Cannot create target file",
        "fs.cannot_extract_to" => "Cannot extract to",
        "fs.cannot_create_download_dir" => "Cannot create download directory",
        "fs.cannot_get_metadata" => "Cannot get file metadata",
        "fs.cannot_open_registry" => "Cannot open registry",
        "fs.cannot_get_program_path" => "Cannot get program path",

        // ── Generic / 通用 ──
        "error.unknown" => "Unknown error",

        _ => return None,
    })
}

/// 中文映射
fn zh(key: &str) -> Option<&'static str> {
    Some(match key {
        // ── Tunnel / 隧道 ──
        "tunnel.token_mode.needs_token" => "Token 模式需要提供 Cloudflare Tunnel Token",
        "tunnel.token_mode.needs_domain" => "Token 模式需要提供自定义域名",
        "tunnel.unknown_mode" => "未知的隧道模式",
        "tunnel.token_obj.needs_token_field" => "Token 对象必须包含 'token' 字段",
        "tunnel.token_obj.needs_domain_field" => "Token 对象必须包含 'domain' 字段",
        "tunnel.token_field.must_be_object" => "Token 字段必须是一个对象",
        "tunnel.mode_obj.must_contain_token" => "隧道模式对象必须包含 'Token' 或其他有效字段",
        "tunnel.mode.must_be_string_or_object" => "隧道模式必须是字符串或对象",
        "tunnel.cannot_capture_stderr" => "无法捕获标准错误流",

        // ── Tunnel event / 隧道事件 ──
        "tunnel.event.connecting" => "连接中",
        "tunnel.event.online" => "已上线",
        "tunnel.event.offline" => "已离线",
        "tunnel.health.healthy" => "健康",
        "tunnel.health.connecting" => "连接中",
        "tunnel.health.stopped" => "已停止",

        // ── Cloudflared ──
        "cloudflared.download.needs_window" => "下载 cloudflared 需要窗口句柄",
        "cloudflared.download.failed" => "下载 cloudflared 失败",
        "cloudflared.extraction.binary_not_found" => "压缩包中未找到 cloudflared 二进制文件",
        "cloudflared.unsupported_platform" => "当前操作系统平台暂不支持",

        // ── Node.js / Runtime ──
        "node.not_found" => "Node.js 未找到，请先执行 setup_runtime",
        "n8n.core_not_found" => "n8n 核心未找到，请先执行 setup_n8n",
        "n8n.binary_not_found" => "n8n 二进制文件未找到",
        "runtime.unsupported_platform" => "不支持的平台架构",

        // ── n8n State / 状态 ──
        "n8n.state.process_manager_poisoned" => "PROCESS_MANAGER 锁已被毒化 (Poisoned)",
        "n8n.state.no_check_run" => "未启动检查",
        "n8n.state.network_error" => "网络错误",
        "n8n.state.http_status" => "HTTP 状态码",
        "n8n.state.healthy" => "健康",
        "n8n.state.cannot_read_body" => "无法读取Body",

        // ── Filesystem / 文件系统 ──
        "fs.cannot_create_dir" => "创建目录失败",
        "fs.cannot_read_dir" => "读取目录失败",
        "fs.cannot_delete_dir" => "删除目录失败",
        "fs.cannot_delete_file" => "删除文件失败",
        "fs.cannot_write_file" => "写入文件失败",
        "fs.cannot_rename_file" => "移动文件失败",
        "fs.cannot_create_parent_dir" => "创建父目录失败",
        "fs.cannot_read_cache" => "读取缓存文件失败",

        // ── Download / 下载 ──
        "download.clean_dir_failed" => "清理目录失败",
        "download.create_dir_failed" => "创建目录失败",
        "download.zip_invalid" => "ZIP 格式非法",
        "download.zip_extract_failed" => "ZIP 解压失败",
        "download.tar_extract_failed" => "TAR.GZ 解压失败",
        "download.stream_error" => "下载流错误",
        "download.http_error" => "HTTP 请求失败",
        "download.http_status_error" => "下载失败: HTTP",
        "download.create_client_failed" => "创建 HTTP 客户端失败",
        "download.read_dir_entry_failed" => "读取目录条目失败",
        "download.permission_fix_failed" => "权限修复失败",

        // ── N8n Process / 进程 ──
        "process.spawn_failed" => "启动 n8n 进程失败",
        "process.invalid_user_data_path" => "用户数据目录路径包含无效字符",

        // ── Cloudflared path ──
        "cloudflared.path.not_found_in_system" => "系统中未找到 cloudflared 可执行文件",
        "cloudflared.path.resource_dir_failed" => "获取资源目录失败",
        "cloudflared.path.app_data_dir_failed" => "获取应用数据目录失败",
        "cloudflared.path.read_cache_failed" => "读取缓存信息文件失败",
        "cloudflared.path.parse_cache_failed" => "解析缓存信息失败",

        // ── Cloudflared install / cache ──
        "cloudflared.version.exec_failed" => "执行 cloudflared 版本检查失败",
        "cloudflared.file_meta_failed" => "获取文件元数据失败",
        "cloudflared.copy_failed" => "复制文件失败",
        "cloudflared.permission_set_failed" => "设置权限失败",
        "cloudflared.registry_failed" => "打开注册表失败",
        "cloudflared.cache.serialize_failed" => "序列化缓存信息失败",
        "cloudflared.cache.write_failed" => "写入缓存文件失败",
        "cloudflared.cache.get_download_dir_failed" => "获取下载目录失败",

        // ── Filesystem / 文件系统 (带占位符) ──
        "fs.cannot_delete_temp_file" => "删除临时文件失败",
        "fs.cannot_delete_existing_file" => "删除现有文件失败",
        "fs.cannot_move_to" => "移动文件到失败",
        "fs.cannot_open_archive" => "无法打开压缩包",
        "fs.cannot_read_archive_entry" => "读取压缩包条目失败",
        "fs.cannot_get_entry_path" => "获取条目路径失败",
        "fs.cannot_create_target_file" => "创建目标文件失败",
        "fs.cannot_extract_to" => "提取文件到失败",
        "fs.cannot_create_download_dir" => "创建下载目录失败",
        "fs.cannot_get_metadata" => "获取文件元数据失败",
        "fs.cannot_open_registry" => "打开注册表失败",
        "fs.cannot_get_program_path" => "获取程序路径失败",

        // ── Generic / 通用 ──
        "error.unknown" => "未知错误",

        _ => return None,
    })
}
