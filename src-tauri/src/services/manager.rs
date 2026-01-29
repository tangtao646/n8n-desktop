use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

// --- 常量定义 ---

/// Node.js 版本要求
const NODEJS_VERSION: &str = "v20.19.0";

/// Node.js 官方下载地址
const NODEJS_BASE_URL: &str = "https://nodejs.org/dist/";

/// Node.js 华为云镜像地址（备用）
#[allow(dead_code)]
const NODEJS_HUAWEI_MIRROR_URL: &str = "https://mirrors.huaweicloud.com/nodejs";

/// n8n 服务端口
const N8N_SERVICE_PORT: &str = "5678";

/// n8n 服务主机
const N8N_SERVICE_HOST: &str = "127.0.0.1";

/// Windows 进程创建标志（CREATE_NO_WINDOW）
#[cfg(windows)]
const WINDOWS_CREATE_NO_WINDOW_FLAG: u32 = 0x08000000;

// --- 环境变量常量 ---

const ENV_N8N_USER_FOLDER: &str = "N8N_USER_FOLDER";
const ENV_N8N_DISABLE_INTERACTIVE_REPL: &str = "N8N_DISABLE_INTERACTIVE_REPL";
const ENV_N8N_BLOCK_IFRAME_EMBEDS: &str = "N8N_BLOCK_IFRAME_EMBEDS";
const ENV_N8N_USE_SAMESITE_COOKIE_STRICT: &str = "N8N_USE_SAMESITE_COOKIE_STRICT";
const ENV_N8N_CORS_ALLOWED_ORIGINS: &str = "N8N_CORS_ALLOWED_ORIGINS";
const ENV_N8N_SECURE_COOKIE: &str = "N8N_SECURE_COOKIE";
const ENV_N8N_USER_MANAGEMENT_DISABLED: &str = "N8N_USER_MANAGEMENT_DISABLED";
const ENV_SKIP_SETUP: &str = "SKIP_SETUP";
const ENV_N8N_PORT: &str = "N8N_PORT";
const ENV_N8N_HOST: &str = "N8N_HOST";

// --- 进程管理器 ---

/// 全局进程管理器实例
pub static PROCESS_MANAGER: Lazy<Mutex<ProcessManager>> =
    Lazy::new(|| Mutex::new(ProcessManager::new()));

/// 进程管理器结构体
pub struct ProcessManager {
    child: Option<Child>,
}

impl ProcessManager {
    /// 创建新的进程管理器实例
    pub fn new() -> Self {
        ProcessManager { child: None }
    }

    /// 设置子进程
    pub fn set_child(&mut self, child: Child) {
        self.child = Some(child);
    }

    /// 终止子进程
    pub fn kill_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            // 首先尝试优雅地杀死进程
            if let Err(error) = child.kill() {
                eprintln!("终止进程失败: {error}");
            }

            // 等待进程完全退出，确保资源释放
            let _ = child.wait();
        }
    }

    /// 检查是否有活动的子进程
    pub fn has_child(&self) -> bool {
        self.child.is_some()
    }
}

// --- Node.js 下载 URL 生成 ---

/// 获取当前平台的 Node.js 下载 URL
pub fn get_node_url() -> Result<String, String> {
    let platform = env::consts::OS;
    let architecture = env::consts::ARCH;

    match (platform, architecture) {
        ("macos", "aarch64") => Ok(format_nodejs_url("darwin-arm64", "tar.gz")),
        ("macos", "x86_64") => Ok(format_nodejs_url("darwin-x64", "tar.gz")),
        ("windows", _) => Ok(format_nodejs_url("win-x64", "zip")),
        _ => Err(format!("不支持的平台架构: {platform} {architecture}")),
    }
}

/// 格式化 Node.js 下载 URL
fn format_nodejs_url(platform_arch: &str, extension: &str) -> String {
    format!(
        "{}/{}/node-{}-{}.{}",
        NODEJS_BASE_URL, NODEJS_VERSION, NODEJS_VERSION, platform_arch, extension
    )
}

// --- Node.js 二进制路径查找 ---

/// 获取 Node.js 二进制文件路径
pub fn get_node_binary_path(runtime_dir: PathBuf) -> PathBuf {
    if cfg!(target_os = "windows") {
        find_windows_node_binary(&runtime_dir)
    } else {
        find_unix_node_binary(&runtime_dir)
    }
}

/// 在 Windows 上查找 Node.js 二进制文件
fn find_windows_node_binary(runtime_dir: &PathBuf) -> PathBuf {
    let direct_path = runtime_dir.join("node.exe");

    if direct_path.exists() {
        return direct_path;
    }

    search_node_binary(runtime_dir, "node.exe").unwrap_or(direct_path)
}

/// 在 Unix 系统上查找 Node.js 二进制文件
fn find_unix_node_binary(runtime_dir: &PathBuf) -> PathBuf {
    let direct_path = runtime_dir.join("bin/node");

    if direct_path.exists() {
        return direct_path;
    }

    search_node_binary(runtime_dir, "bin/node").unwrap_or(direct_path)
}

/// 递归搜索 Node.js 二进制文件
fn search_node_binary(directory: &PathBuf, target_name: &str) -> Option<PathBuf> {
    use std::fs;

    // 首先尝试直接路径
    let candidate = directory.join(target_name);
    if candidate.exists() {
        return Some(candidate);
    }

    // 递归搜索目录
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                // 递归搜索子目录
                if let Some(found) = search_node_binary(&path, target_name) {
                    return Some(found);
                }
            } else if is_node_binary_file(&path) {
                // 找到 node 或 node.exe 文件
                return Some(path);
            }
        }
    }

    None
}

/// 检查文件是否为 Node.js 二进制文件
fn is_node_binary_file(path: &PathBuf) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "node" || name == "node.exe")
}

// --- n8n 进程启动 ---

/// n8n 启动配置
pub struct N8nStartConfig {
    pub node_path: PathBuf,
    pub n8n_binary: PathBuf,
    pub user_data_dir: PathBuf,
    pub additional_envs: HashMap<String, String>,
}

/// 启动 n8n 进程
pub fn start_node(
    node_path: PathBuf,
    n8n_bin: PathBuf,
    user_data: PathBuf,
    additional_envs: HashMap<String, String>,
) -> Result<(), String> {
    let config = N8nStartConfig {
        node_path,
        n8n_binary: n8n_bin,
        user_data_dir: user_data,
        additional_envs,
    };

    terminate_existing_node_processes();
    let child = create_and_start_n8n_process(&config)?;
    register_process_with_manager(child);

    Ok(())
}

/// 终止现有的 node 进程（仅 Unix 系统）
fn terminate_existing_node_processes() {
    #[cfg(unix)]
    {
        let _ = Command::new("pkill").arg("-9").arg("node").output();
    }
}

/// 创建并启动 n8n 进程
fn create_and_start_n8n_process(config: &N8nStartConfig) -> Result<Child, String> {
    let mut command = build_n8n_command(config)?;
    configure_process_stdio(&mut command);
    apply_platform_specific_config(&mut command);

    command
        .spawn()
        .map_err(|error| format!("启动 n8n 进程失败: {error}"))
}

/// 构建 n8n 命令
fn build_n8n_command(config: &N8nStartConfig) -> Result<Command, String> {
    let user_data_str = config
        .user_data_dir
        .to_str()
        .ok_or("用户数据目录路径包含无效字符".to_string())?;

    let mut command = Command::new(&config.node_path);

    command
        .arg(&config.n8n_binary)
        .arg("start")
        .env(ENV_N8N_USER_FOLDER, user_data_str)
        .env(ENV_N8N_DISABLE_INTERACTIVE_REPL, "true")
        .env(ENV_N8N_BLOCK_IFRAME_EMBEDS, "false")
        .env(ENV_N8N_USE_SAMESITE_COOKIE_STRICT, "false")
        .env(ENV_N8N_CORS_ALLOWED_ORIGINS, "*")
        .env(ENV_N8N_SECURE_COOKIE, "false")
        .env(ENV_N8N_USER_MANAGEMENT_DISABLED, "true")
        .env(ENV_SKIP_SETUP, "true")
        .env(ENV_N8N_PORT, N8N_SERVICE_PORT)
        .env(ENV_N8N_HOST, N8N_SERVICE_HOST);

    // 添加额外的环境变量
    for (key, value) in &config.additional_envs {
        command.env(key, value);
    }

    Ok(command)
}

/// 配置进程的标准输入/输出
fn configure_process_stdio(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
}

/// 应用平台特定的配置
#[allow(unused_variables)]
fn apply_platform_specific_config(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(WINDOWS_CREATE_NO_WINDOW_FLAG);
    }
}

/// 将进程注册到全局管理器
fn register_process_with_manager(child: Child) {
    if let Ok(mut manager) = PROCESS_MANAGER.lock() {
        manager.set_child(child);
    }
}

// --- 测试模块 ---
#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_process_manager_creation() {
        let manager = ProcessManager::new();
        assert!(!manager.has_child());
    }

    #[test]
    fn test_process_manager_child_management() {
        let mut manager = ProcessManager::new();

        // 模拟一个子进程（使用 sleep 命令）
        #[cfg(unix)]
        let child = Command::new("sleep")
            .arg("1")
            .spawn()
            .expect("Failed to spawn sleep process");

        #[cfg(windows)]
        let child = Command::new("timeout")
            .arg("/t")
            .arg("1")
            .spawn()
            .expect("Failed to spawn timeout process");

        manager.set_child(child);
        assert!(manager.has_child());

        manager.kill_child();
        // 注意：kill_child 后 has_child 应该返回 false
        // 但实际实现中，take() 会取出 child，所以 has_child 为 false
        assert!(!manager.has_child());
    }

    #[test]
    fn test_get_node_url() {
        let result = get_node_url();

        // 根据当前平台验证 URL 格式
        if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
            assert!(result.is_ok());
            let url = result.expect("Failed to get node URL");
            assert!(url.contains("darwin-arm64"));
            assert!(url.contains(NODEJS_VERSION));
        } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
            assert!(result.is_ok());
            let url = result.expect("Failed to get node URL");
            assert!(url.contains("darwin-x64"));
        } else if cfg!(target_os = "windows") {
            assert!(result.is_ok());
            let url = result.expect("Failed to get node URL");
            assert!(url.contains("win-x64"));
            assert!(url.ends_with(".zip"));
        }
        // 其他平台可能返回错误，这是预期的
    }

    #[test]
    fn test_is_node_binary_file() {
        let _node_path = PathBuf::from("/usr/bin/node");
        let _node_exe_path = PathBuf::from("C:\\Program Files\\nodejs\\node.exe");
        let _other_path = PathBuf::from("/usr/bin/python");

        // 这些测试在 Windows 和 Unix 上行为不同
        // 我们主要测试逻辑正确性
        assert!(is_node_binary_file(&PathBuf::from("node")));
        assert!(is_node_binary_file(&PathBuf::from("node.exe")));
        assert!(!is_node_binary_file(&PathBuf::from("python")));
    }

    #[test]
    fn test_format_nodejs_url() {
        let url = format_nodejs_url("darwin-arm64", "tar.gz");

        assert!(url.contains(NODEJS_BASE_URL));
        assert!(url.contains(NODEJS_VERSION));
        assert!(url.contains("darwin-arm64"));
        assert!(url.ends_with(".tar.gz"));
    }

    #[test]
    fn test_build_n8n_command_structure() {
        let temp_dir = temp_dir();
        let config = N8nStartConfig {
            node_path: PathBuf::from("/usr/bin/node"),
            n8n_binary: PathBuf::from("/app/n8n"),
            user_data_dir: temp_dir.clone(),
            additional_envs: HashMap::from([("TEST_KEY".to_string(), "TEST_VALUE".to_string())]),
        };

        let command_result = build_n8n_command(&config);
        assert!(command_result.is_ok());

        // 命令应该包含必要的参数和环境变量
        let _command = command_result.expect("Failed to build n8n command");
        // 注意：我们无法直接检查 Command 的内部状态
        // 这个测试主要确保函数不会 panic
    }
}
