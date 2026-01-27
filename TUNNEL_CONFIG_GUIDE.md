# Cloudflare Tunnel 自定义域名支持 - 完善指南

## 概述

本次完善为 n8n-desktop 应用添加了完整的 Cloudflare Tunnel 自定义域名和 Token 固定隧道支持。支持三种隧道模式，用户可灵活选择。

## 三种隧道模式

### 1. 临时隧道（Temporary）- 默认模式
- **描述**：每次启动随机生成临时公网地址
- **特点**：无需配置，即插即用
- **URL 格式**：`https://xxx.trycloudflare.com`
- **使用场景**：快速测试、临时共享
- **cloudflared 命令**：
  ```bash
  cloudflared tunnel --url http://localhost:5678 --no-autoupdate
  ```

### 2. 自定义域名（Custom Domain）
- **描述**：使用自己的域名作为固定公网地址
- **前置条件**：
  1. 域名在 Cloudflare 注册或转入 Cloudflare
  2. 在 Cloudflare 中创建 Tunnel
  3. 在 Cloudflare DNS 中添加 CNAME 记录
- **配置步骤**：
  1. 在应用中选择"自定义域名"模式
  2. 输入你的 Tunnel ID（在 Cloudflare 中创建隧道时获得）
  3. 点击"保存配置"

- **Cloudflare 配置示例**：
  ```
  DNS 记录：
  Type: CNAME
  Name: myapp
  Content: xxx.cfargotunnel.com
  
  Tunnel 配置中添加：
  Hostname: myapp.example.com
  Service: http://localhost:5678
  ```

- **cloudflared 命令**：
  ```bash
  cloudflared tunnel run <TUNNEL_ID> --no-autoupdate
  ```

### 3. Token 固定隧道（Token Mode）
- **描述**：使用 Cloudflare Tunnel Token 建立与 Cloudflare 账户绑定的固定隧道
- **特点**：固定隧道 URL，不需要每次手动配置
- **前置条件**：
  1. 在 Cloudflare 中创建隧道
  2. 生成 Tunnel Connector Token
- **获取 Token 步骤**：
  1. 登录 Cloudflare 仪表盘
  2. 进入 Zero Trust > Tunnels
  3. 创建或选择隧道
  4. 点击"Connectors" 或 "Run"
  5. 复制完整的 Token

- **cloudflared 命令**：
  ```bash
  cloudflared tunnel run --token <YOUR_TOKEN>
  ```

## 前端 UI 改动

### SidebarPanel 组件

#### 1. 新增隧道模式选择器
```tsx
// 三个按钮，用户可切换模式
- 临时隧道
- 自定义域名
- Token 固定
```

#### 2. 模式特定的输入字段
- **自定义域名模式**：显示域名输入框
- **Token 模式**：显示 Token 输入框（文本框）
- **临时隧道**：无需输入，直接使用

#### 3. 配置保存流程
- 用户选择模式 → 输入相关信息 → 点击"保存配置" → 应用立即重启 n8n

## 后端实现

### Rust 数据结构更新

#### TunnelConfig 结构体
```rust
pub struct TunnelConfig {
    pub last_url: Option<String>,
    pub auto_start: bool,
    pub created_at: String,
    pub custom_domain: Option<String>,        // 保留向后兼容
    pub use_custom_domain: bool,              // 保留向后兼容
    pub tunnel_mode: String,                  // "temporary" | "custom-domain" | "token"
    pub tunnel_token: Option<String>,         // 新增：Tunnel Token
}
```

### 核心函数实现

#### 1. `apply_tunnel_config()` - 新的配置应用函数
```rust
// 位置：src-tauri/src/api/tunnel.rs
// 功能：
// - 验证输入参数
// - 更新 TUNNEL_CONFIG
// - 保存配置到文件
// - 如果隧道运行，则重启 n8n
```

#### 2. `start_tunnel()` - 改进的隧道启动
```rust
// 根据 tunnel_mode 决定 cloudflared 命令参数
match tunnel_mode {
    "custom-domain" => { ... }   // tunnel run <DOMAIN>
    "token" => { ... }            // tunnel run --token <TOKEN>
    _ => { ... }                  // tunnel --url http://localhost:5678
}
```

#### 3. `determine_tunnel_url()` - URL 确定逻辑
```rust
// 根据 tunnel_mode 返回最终 URL
fn determine_tunnel_url(
    tunnel_mode: &str,
    custom_domain: Option<String>,
    tunnel_url: Option<String>,
) -> Option<String>
```

### 命令处理

#### 新增 Tauri 命令
```rust
#[tauri::command]
pub async fn apply_tunnel_config(
    app: AppHandle<R>,
    tunnel_mode: String,
    custom_domain: Option<String>,
    tunnel_token: Option<String>,
) -> Result<(), String>
```

#### 保留旧命令（向后兼容）
```rust
#[tauri::command]
pub async fn apply_custom_domain_config(
    app: AppHandle<R>,
    custom_domain: Option<String>,
    use_custom_domain: bool,
) -> Result<(), String>
// 自动转换为新的 apply_tunnel_config
```

## 配置文件格式

隧道配置保存在 `~/.n8n-desktop/tunnel-config.json`

### 示例：临时隧道
```json
{
  "last_url": "https://xxx.trycloudflare.com",
  "auto_start": false,
  "created_at": "2026-01-27T...",
  "custom_domain": null,
  "use_custom_domain": false,
  "tunnel_mode": "temporary",
  "tunnel_token": null
}
```

### 示例：自定义域名
```json
{
  "last_url": "https://myapp.example.com",
  "auto_start": false,
  "created_at": "2026-01-27T...",
  "custom_domain": "https://myapp.example.com",
  "use_custom_domain": true,
  "tunnel_mode": "custom-domain",
  "tunnel_token": null
}
```

### 示例：Token 模式
```json
{
  "last_url": null,
  "auto_start": false,
  "created_at": "2026-01-27T...",
  "custom_domain": null,
  "use_custom_domain": false,
  "tunnel_mode": "token",
  "tunnel_token": "eyJhIjoiYWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoifQ..."
}
```

## 环境变量

隧道启动时，n8n 获得以下环境变量：

```bash
# 所有模式都设置这些变量
WEBHOOK_URL=<最终公网地址>
N8N_EDITOR_BASE_URL=<最终公网地址>
N8N_CORS_ALLOWED_ORIGINS=*
```

例如：
- 临时隧道：`WEBHOOK_URL=https://abc123.trycloudflare.com`
- 自定义域名：`WEBHOOK_URL=https://myapp.example.com`
- Token 模式：`WEBHOOK_URL=<cloudflared 输出的 URL>`

## 向后兼容性

- ✅ 旧的 `apply_custom_domain_config` 命令仍然可用
- ✅ 旧的配置字段 `use_custom_domain` 和 `custom_domain` 保留
- ✅ 如果无法读取 `tunnel_mode` 字段，默认使用 `"temporary"`
- ✅ 自动迁移：旧配置会逐步升级到新格式

## 常见问题

### Q1：如何在自定义域名和 Token 之间切换？
A：在隧道配置界面，选择不同的模式按钮，输入相应信息，点击保存即可。应用会自动重启隧道。

### Q2：Token 模式和自定义域名有什么区别？
A：
- **自定义域名**：需要在 Cloudflare 中手动配置隧道和 DNS 记录
- **Token 模式**：Cloudflare 自动管理，只需提供 Token，URL 由 Cloudflare 分配

### Q3：能否同时使用多个隧道？
A：暂不支持，但可以快速切换模式。未来可以扩展为支持多隧道。

### Q4：Token 过期了怎么办？
A：在隧道配置界面更新新的 Token，点击保存即可。

## 测试建议

1. **测试临时隧道**：启动应用，检查是否能连接
2. **测试自定义域名**：配置自定义域名，验证 webhook URL 是否更新
3. **测试 Token 模式**：输入有效 Token，检查连接状态
4. **测试切换**：在三种模式间切换，确保正确重启 n8n
5. **测试配置持久化**：重启应用，检查配置是否保存

## 相关文件

| 文件 | 变更 |
|------|------|
| `src/components/SidebarPanel.tsx` | 新增隧道模式选择器和 UI |
| `src-tauri/src/api/tunnel.rs` | 新增 `apply_tunnel_config()`, 改进 `start_tunnel()` |
| `src-tauri/src/api/n8n_core.rs` | 更新 `determine_tunnel_url()` 函数 |
| `src-tauri/src/api/commands.rs` | 新增 `apply_tunnel_config` 命令 |
| `src/App.css` | 新增隧道模式选择器样式 |

## 下一步计划

- [ ] 支持多隧道管理
- [ ] 自动检测 Token 过期
- [ ] 添加隧道健康检查
- [ ] 支持自定义 cloudflared 配置文件
- [ ] 添加隧道日志查看界面
