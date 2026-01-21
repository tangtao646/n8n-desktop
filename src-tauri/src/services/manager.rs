use once_cell::sync::Lazy;
use std::env;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// 1. 定义一个全局的进程管理器
pub static PROCESS_MANAGER: Lazy<Mutex<ProcessManager>> =
    Lazy::new(|| Mutex::new(ProcessManager::new()));

/// 2. 定义 ProcessManager 结构体
pub struct ProcessManager {
    child: Option<Child>,
}

impl ProcessManager {
    pub fn new() -> Self {
        ProcessManager { child: None }
    }

    pub fn set_child(&mut self, child: Child) {
        self.child = Some(child);
    }

    pub fn kill_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            // 尝试优雅地杀死进程，如果失败则强制杀死
            if let Err(e) = child.kill() {
                eprintln!("Failed to kill process: {}", e);
            }
        }
    }
}

pub fn get_node_url() -> Result<String, String> {
    // n8n requires Node.js >=20.19 <= 24.x
    // Using Node.js 20.19.0 which is the minimum supported version
    let version = "v20.19.0";
    let base_url = "https://mirrors.huaweicloud.com/nodejs";
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => Ok(format!(
            "{}/{}/node-{}-darwin-arm64.tar.gz",
            base_url, version, version
        )),
        ("macos", "x86_64") => Ok(format!(
            "{}/{}/node-{}-darwin-x64.tar.gz",
            base_url, version, version
        )),
        ("windows", _) => Ok(format!(
            "{}/{}/node-{}-win-x64.zip",
            base_url, version, version
        )),
        _ => Err(format!(
            "Unsupported platform: {} {}",
            env::consts::OS,
            env::consts::ARCH
        )),
    }
}

pub fn get_node_binary_path(runtime_dir: PathBuf) -> PathBuf {
    if cfg!(target_os = "windows") {
        // 对于 Windows，先尝试直接路径，然后搜索
        let direct_path = runtime_dir.join("node.exe");
        if direct_path.exists() {
            return direct_path;
        }
        // 搜索嵌套目录中的 node.exe
        search_node_binary(&runtime_dir, "node.exe").unwrap_or(direct_path)
    } else {
        // MacOS/Linux: 先尝试直接路径 bin/node
        let direct_path = runtime_dir.join("bin/node");
        if direct_path.exists() {
            return direct_path;
        }
        // 搜索嵌套目录中的 bin/node
        search_node_binary(&runtime_dir, "bin/node").unwrap_or(direct_path)
    }
}

/// 递归搜索 node 二进制文件
fn search_node_binary(dir: &PathBuf, target: &str) -> Option<PathBuf> {
    use std::fs;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // 递归搜索子目录
                if let Some(found) = search_node_binary(&path, target) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|n| n.to_str()) == Some("node")
                || path.file_name().and_then(|n| n.to_str()) == Some("node.exe")
            {
                // 找到 node 或 node.exe 文件
                return Some(path);
            }
        }
    }

    // 如果没找到，尝试拼接目标路径
    let candidate = dir.join(target);
    if candidate.exists() {
        return Some(candidate);
    }

    None
}

pub fn start_node(node_path: PathBuf, n8n_bin: PathBuf, user_data: PathBuf) -> Result<(), String> {
    #[cfg(unix)]
    let _ = Command::new("pkill").arg("-9").arg("node").output();

    let mut cmd = Command::new(node_path);
    cmd.arg(n8n_bin)
        .arg("start")
        .env("N8N_USER_FOLDER", user_data.to_str().unwrap())
        // 关键环境变量：禁用交互模式
        .env("N8N_DISABLE_INTERACTIVE_REPL", "true")
        .env("N8N_BLOCK_IFRAME_EMBEDS", "false")
        .env("N8N_USE_SAMESITE_COOKIE_STRICT", "false")
        .env("N8N_CORS_ALLOWED_ORIGINS", "*")
        .env("N8N_SECURE_COOKIE", "false")
        .env("N8N_USER_MANAGEMENT_DISABLED", "true")
        .env("SKIP_SETUP", "true")
        .env("N8N_PORT", "5678")
        .env("N8N_HOST", "127.0.0.1")
        // 核心修正：提供一个空的 stdin 防止 setRawMode 报错
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    let child = cmd.spawn().map_err(|e| format!("进程启动失败: {}", e))?;
    let mut manager = PROCESS_MANAGER.lock().unwrap();
    manager.set_child(child);

    Ok(())
}
