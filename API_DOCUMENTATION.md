# Tauri 命令 API 文档

## 概述

本文档列出了与隧道配置相关的所有 Tauri 命令和数据结构。

---

## 新增命令

### `apply_tunnel_config`

应用新的隧道配置（支持三种模式）

#### 签名
```rust
#[tauri::command]
pub async fn apply_tunnel_config(
    app: AppHandle<R>,
    tunnel_mode: String,
    custom_domain: Option<String>,
    tunnel_token: Option<String>,
) -> Result<(), String>
```

#### 参数

| 参数 | 类型 | 说明 | 例子 |
|------|------|------|------|
| `tunnel_mode` | String | 隧道模式 | `"temporary"` / `"custom-domain"` / `"token"` |
| `custom_domain` | Option<String> | 自定义域名 | `"https://myapp.example.com"` |
| `tunnel_token` | Option<String> | Cloudflare Tunnel Token | `"eyJhIjo..."` |

#### 返回值

| 类型 | 说明 |
|------|------|
| `Result<(), String>` | 成功返回 `Ok(())`, 失败返回 `Err(error_message)` |

#### 错误情况

- 自定义域名模式下未提供域名：`"Custom domain cannot be empty"`
- Token 模式下未提供 Token：`"Invalid tunnel token"`
- Token 长度不足 50 个字符：`"Invalid tunnel token"`
- 未知的 tunnel_mode：`"Unknown tunnel mode: {mode}"`

#### 前端调用示例

```typescript
// TypeScript/React
import { invoke } from "@tauri-apps/api/core";

try {
  await invoke("apply_tunnel_config", {
    tunnel_mode: "custom-domain",
    custom_domain: "https://myapp.example.com",
    tunnel_token: null,
  });
  console.log("隧道配置已应用");
} catch (error) {
  console.error("配置失败:", error);
}
```

#### 后端调用示例

```rust
// 在其他 Rust 模块中调用
use crate::api::tunnel;

tunnel::apply_tunnel_config(
    app,
    "token",
    None,
    Some("eyJhIjo...".to_string()),
)
.await?;
```

---

## 保留命令（向后兼容）

### `apply_custom_domain_config`

旧版本的自定义域名配置命令（已弃用但保留）

#### 签名
```rust
#[tauri::command]
pub async fn apply_custom_domain_config(
    app: AppHandle<R>,
    custom_domain: Option<String>,
    use_custom_domain: bool,
) -> Result<(), String>
```

#### 说明

该命令已被 `apply_tunnel_config` 替代，但为了保持向后兼容性仍保留。

内部实现会自动转换为新的命令：
- 如果 `use_custom_domain` 为 true，转换为 `tunnel_mode: "custom-domain"`
- 如果 `use_custom_domain` 为 false，转换为 `tunnel_mode: "temporary"`

---

## 查询命令

### `get_tunnel_config`

获取当前隧道配置

#### 签名
```rust
#[tauri::command]
pub async fn get_tunnel_config() -> Result<TunnelConfig, String>
```

#### 返回结构

```typescript
interface TunnelConfig {
  last_url?: string;           // 最后使用的 URL
  auto_start?: boolean;        // 是否自动启动隧道
  created_at?: string;         // 创建时间
  custom_domain?: string;      // 自定义域名
  use_custom_domain?: boolean; // 是否使用自定义域名（旧字段）
  tunnel_mode?: string;        // 隧道模式（新）
  tunnel_token?: string;       // Token（新）
}
```

#### 前端调用示例

```typescript
import { invoke } from "@tauri-apps/api/core";

const config = await invoke<TunnelConfig>("get_tunnel_config");
console.log("当前隧道模式:", config.tunnel_mode);
console.log("最后使用的 URL:", config.last_url);
```

---

## 隧道控制命令

### `start_tunnel`

启动隧道连接

#### 签名
```rust
#[tauri::command]
pub async fn start_tunnel(
    app: AppHandle<R>,
    cloudflared_path: String,
) -> Result<(), String>
```

#### 说明

该命令会：
1. 清理之前残留的 cloudflared 进程
2. 根据 `tunnel_mode` 使用不同的 cloudflared 参数
3. 监听 stderr 输出以获取隧道 URL
4. 发出 `tunnel-event` 事件更新 UI

#### 前端调用示例

```typescript
await invoke("start_tunnel", {
  cloudflared_path: "cloudflared",
});
```

### `stop_tunnel`

停止隧道连接

#### 签名
```rust
#[tauri::command]
pub async fn stop_tunnel(app: AppHandle<R>) -> Result<(), String>
```

---

## 事件（Events）

### `tunnel-event`

隧道状态变化事件

#### 事件结构

```typescript
interface TunnelEvent {
  status: string;           // "connecting" | "online" | "offline" | "error"
  url?: string;            // 隧道 URL（当 status 为 "online" 时）
  progress?: number;       // 进度（0-100）
  message?: string;        // 额外消息
}
```

#### 事件状态说明

| 状态 | 说明 |
|------|------|
| `connecting` | 正在连接隧道 |
| `online` | 隧道已连接，获得 URL |
| `offline` | 隧道已关闭 |
| `error` | 隧道连接出错 |

#### 前端监听示例

```typescript
import { listen } from "@tauri-apps/api/event";

const unlisten = await listen<TunnelEvent>("tunnel-event", (event) => {
  console.log("隧道状态:", event.payload.status);
  if (event.payload.status === "online") {
    console.log("隧道 URL:", event.payload.url);
  }
});

// 不再需要监听时
unlisten();
```

---

## 内部函数（Rust）

### `apply_tunnel_config()`

核心配置应用函数

#### 定位
```
src-tauri/src/api/tunnel.rs
```

#### 签名
```rust
pub async fn apply_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    tunnel_mode: &str,
    custom_domain: Option<String>,
    tunnel_token: Option<String>,
) -> Result<(), String>
```

#### 处理逻辑

1. **验证输入**
   - 自定义域名模式：检查 domain 不为空
   - Token 模式：检查 token 长度 >= 50
   - 临时隧道模式：无特殊验证

2. **更新配置**
   - 更新 `TUNNEL_CONFIG` 全局状态
   - 保存到 `tunnel-config.json`

3. **应用配置**
   - 如果隧道正在运行，触发 n8n 重启
   - 调用 `restart_n8n_with_env()` 应用新的环境变量

#### 调用示例

```rust
tunnel::apply_tunnel_config(
    app.clone(),
    "token",
    None,
    Some("eyJhIjo...".to_string()),
)
.await?;
```

### `start_tunnel()` 内部参数选择

#### 临时隧道
```rust
Command::new(&cloudflared_path)
    .args(&[
        "tunnel",
        "--url",
        "http://localhost:5678",
        "--no-autoupdate",
    ])
```

#### 自定义域名
```rust
Command::new(&cloudflared_path)
    .args(&["tunnel", "run", &domain, "--no-autoupdate"])
```

#### Token 模式
```rust
Command::new(&cloudflared_path)
    .args(&["tunnel", "run", "--token", &token])
```

### `determine_tunnel_url()`

确定最终使用的 URL

#### 定位
```
src-tauri/src/api/n8n_core.rs
```

#### 签名
```rust
fn determine_tunnel_url(
    tunnel_mode: &str,
    custom_domain: Option<String>,
    tunnel_url: Option<String>,
) -> Option<String>
```

#### 逻辑

```
match tunnel_mode {
    "custom-domain" => return custom_domain
    "token" => return tunnel_url (from cloudflared)
    _ => return tunnel_url (from cloudflared, temporary)
}
```

---

## 配置文件格式

### 文件位置
```
macOS/Linux: ~/.n8n-desktop/tunnel-config.json
Windows: %APPDATA%\.n8n-desktop\tunnel-config.json
```

### JSON 结构

```json
{
  "last_url": "https://abc123.trycloudflare.com",
  "auto_start": false,
  "created_at": "2026-01-27T10:30:00+00:00",
  "custom_domain": "https://myapp.example.com",
  "use_custom_domain": true,
  "tunnel_mode": "custom-domain",
  "tunnel_token": null
}
```

### Rust 结构体

```rust
#[derive(Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub last_url: Option<String>,
    pub auto_start: bool,
    pub created_at: String,
    pub custom_domain: Option<String>,
    pub use_custom_domain: bool,
    pub tunnel_mode: String,
    pub tunnel_token: Option<String>,
}
```

---

## 环境变量

### 设置的环境变量

当隧道连接成功时，n8n 进程会收到以下环境变量：

```bash
# 所有模式都会设置这些
WEBHOOK_URL=<最终公网地址>
N8N_EDITOR_BASE_URL=<最终公网地址>
N8N_CORS_ALLOWED_ORIGINS=*
```

### 例子

**临时隧道**：
```bash
WEBHOOK_URL=https://abc123xyz.trycloudflare.com
N8N_EDITOR_BASE_URL=https://abc123xyz.trycloudflare.com
```

**自定义域名**：
```bash
WEBHOOK_URL=https://myapp.example.com
N8N_EDITOR_BASE_URL=https://myapp.example.com
```

**Token 模式**：
```bash
WEBHOOK_URL=https://def456uvw.cfargotunnel.com
N8N_EDITOR_BASE_URL=https://def456uvw.cfargotunnel.com
```

---

## 错误处理

### 常见错误

| 错误消息 | 原因 | 解决方案 |
|---------|------|---------|
| "Custom domain cannot be empty" | 自定义域名模式下未提供域名 | 输入有效的域名 |
| "Invalid tunnel token" | Token 不存在或过短 | 提供完整的 Token（1000+ 字符） |
| "Unknown tunnel mode" | 传入的 tunnel_mode 无效 | 使用 "temporary" / "custom-domain" / "token" |
| "Tunnel token not provided" | Token 模式下但未配置 Token | 设置 tunnel_token 字段 |

### 前端错误处理示例

```typescript
try {
  await invoke("apply_tunnel_config", {
    tunnel_mode: "token",
    custom_domain: null,
    tunnel_token: tokenValue,
  });
} catch (error) {
  const errorMsg = error instanceof Error ? error.message : String(error);
  
  if (errorMsg.includes("Invalid tunnel token")) {
    alert("Token 格式不正确，请确保复制完整的 Token");
  } else if (errorMsg.includes("tunnel mode")) {
    alert("隧道模式不正确");
  } else {
    alert(`配置失败: ${errorMsg}`);
  }
}
```

---

## 性能考量

### 隧道启动时间

| 模式 | 启动时间 | 说明 |
|------|---------|------|
| 临时隧道 | 2-3 秒 | cloudflared 生成随机 URL |
| 自定义域名 | 3-5 秒 | 需要验证 Cloudflare 隧道配置 |
| Token 模式 | 2-4 秒 | cloudflared 使用预配置的 Token |

### 资源占用

- cloudflared 进程内存占用：约 20-30 MB
- 隧道连接不会显著增加 CPU 使用率
- 网络流量：取决于 n8n 数据量，隧道本身开销很小

---

## 版本兼容性

### 当前支持

- ✅ Tauri 2.x
- ✅ Rust 1.56+
- ✅ Node.js 16+ (frontend)

### 向后兼容性

- ✅ 旧的 `apply_custom_domain_config` 命令仍可用
- ✅ 旧的 TunnelConfig 字段保留
- ✅ 旧配置文件自动升级

