# 代码改动总结

## 前端改动 - React/TypeScript

### 1. 扩展 AppState 类型定义
**文件**: `src/components/SidebarPanel.tsx`

```typescript
// 添加了两个新字段
type AppState = {
  // ... existing fields ...
  tunnelMode: "temporary" | "custom-domain" | "token";  // 隧道模式
  tunnelToken: string;  // Cloudflare Tunnel Token
};
```

### 2. 更新 TunnelConfig 接口
**文件**: `src/components/SidebarPanel.tsx`

```typescript
interface TunnelConfig {
  custom_domain?: string;
  use_custom_domain?: boolean;
  tunnel_mode?: "temporary" | "custom-domain" | "token";  // 新增
  tunnel_token?: string;  // 新增
  [key: string]: unknown;
}
```

### 3. 完善配置加载逻辑
**文件**: `src/components/SidebarPanel.tsx` - `loadAppInfo()`

```typescript
// 添加了读取新配置字段的逻辑
if (config.tunnel_mode) {
  updateAppState({ tunnelMode: config.tunnel_mode });
}
if (config.tunnel_token) {
  updateAppState({ tunnelToken: config.tunnel_token });
}
```

### 4. 改进配置保存函数
**文件**: `src/components/SidebarPanel.tsx` - `saveCustomDomainConfig()`

**变更**：
- 改名为支持多模式的配置应用
- 添加了模式特定的验证逻辑
- 调用新的 `apply_tunnel_config` 命令而不是 `apply_custom_domain_config`

**新验证逻辑**：
```typescript
// 自定义域名模式验证
if (appState.tunnelMode === "custom-domain") {
  if (!domainToSave) alert("请输入自定义域名");
  if (!domainToSave.includes("://")) alert("请输入完整的域名");
}

// Token 模式验证
if (appState.tunnelMode === "token") {
  if (!tokenToSave) alert("请输入 Token");
  if (tokenToSave.length < 50) alert("Token 格式不正确");
}
```

### 5. 完全重写 `renderCustomDomainSection()` 组件
**文件**: `src/components/SidebarPanel.tsx`

**新功能**：
- **隧道模式选择器**：三个按钮，用户可切换 "临时隧道" / "自定义域名" / "Token固定"
- **模式特定的输入框**：
  - 自定义域名模式：文本输入框
  - Token 模式：文本区域（支持多行）
  - 临时隧道模式：无输入框
- **动态提示文字**：根据选择的模式显示不同的说明
- **智能按钮显示**：仅在非临时隧道模式下显示"保存配置"按钮

**UI 代码示例**：
```typescript
// 模式选择按钮
{(["temporary", "custom-domain", "token"] as const).map((mode) => (
  <button
    key={mode}
    onClick={() => updateAppState({ tunnelMode: mode })}
    style={{...}}
  >
    {mode === "temporary" && "临时隧道"}
    {mode === "custom-domain" && "自定义域名"}
    {mode === "token" && "Token 固定"}
  </button>
))}

// 条件显示输入框
{appState.tunnelMode === "custom-domain" && (
  <div>
    <input type="text" placeholder="https://your-domain.com" />
  </div>
)}

{appState.tunnelMode === "token" && (
  <div>
    <textarea placeholder="粘贴您的 Cloudflare Tunnel Token..." rows={3} />
  </div>
)}
```

---

## 后端改动 - Rust

### 1. 扩展 TunnelConfig 结构体
**文件**: `src-tauri/src/api/tunnel.rs`

```rust
#[derive(Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub last_url: Option<String>,
    pub auto_start: bool,
    pub created_at: String,
    pub custom_domain: Option<String>,      // 保留向后兼容
    pub use_custom_domain: bool,            // 保留向后兼容
    pub tunnel_mode: String,                // NEW: "temporary" | "custom-domain" | "token"
    pub tunnel_token: Option<String>,       // NEW: Cloudflare Tunnel Token
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            // ... existing defaults ...
            tunnel_mode: "temporary".to_string(),
            tunnel_token: None,
        }
    }
}
```

### 2. 改进 `start_tunnel()` 函数
**文件**: `src-tauri/src/api/tunnel.rs`

**变更**：
- 从简单的 if-else 改为 match 语句支持三种模式
- 根据 `tunnel_mode` 决定 cloudflared 启动参数

```rust
let (tunnel_mode, custom_domain, tunnel_token) = {
    let cfg = TUNNEL_CONFIG.lock().unwrap();
    (cfg.tunnel_mode.clone(), cfg.custom_domain.clone(), cfg.tunnel_token.clone())
};

let mut child = match tunnel_mode.as_str() {
    "custom-domain" => {
        // 使用自定义域名（需要在 Cloudflare 中创建的隧道 ID）
        let domain = custom_domain.unwrap_or_else(|| "n8n-tunnel".into());
        Command::new(&cloudflared_path)
            .args(&["tunnel", "run", &domain, "--no-autoupdate"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?
    }
    "token" => {
        // 使用 Cloudflare Tunnel Token
        let token = tunnel_token.ok_or("Tunnel token not provided")?;
        Command::new(&cloudflared_path)
            .args(&["tunnel", "run", "--token", &token])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?
    }
    _ => {
        // 临时隧道（默认）
        Command::new(&cloudflared_path)
            .args(&[
                "tunnel",
                "--url",
                "http://localhost:5678",
                "--no-autoupdate",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?
    }
};
```

### 3. 新增 `apply_tunnel_config()` 函数
**文件**: `src-tauri/src/api/tunnel.rs`

```rust
pub async fn apply_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    tunnel_mode: &str,
    custom_domain: Option<String>,
    tunnel_token: Option<String>,
) -> Result<(), String> {
    // 验证输入
    match tunnel_mode {
        "custom-domain" => {
            if custom_domain.is_none() || custom_domain.as_ref()
                .map(|d| d.trim().is_empty()).unwrap_or(true) {
                return Err("Custom domain cannot be empty".into());
            }
        }
        "token" => {
            if tunnel_token.is_none() || tunnel_token.as_ref()
                .map(|t| t.trim().len() < 50).unwrap_or(true) {
                return Err("Invalid tunnel token".into());
            }
        }
        "temporary" => { /* 无需验证 */ }
        _ => return Err(format!("Unknown tunnel mode: {}", tunnel_mode)),
    }

    // 更新配置
    {
        let mut cfg = TUNNEL_CONFIG.lock().unwrap();
        cfg.tunnel_mode = tunnel_mode.to_string();
        if let Some(domain) = custom_domain {
            cfg.custom_domain = Some(domain);
        }
        if let Some(token) = tunnel_token {
            cfg.tunnel_token = Some(token);
        }
    }

    save_tunnel_config(&app)?;

    // 如果隧道正在运行，重启 n8n
    if PROCESS_MANAGER.lock().unwrap().has_child() {
        let url = TUNNEL_URL.lock().unwrap().clone().unwrap_or_default();
        restart_n8n_with_env(&app, &url);
    }

    Ok(())
}
```

### 4. 改进 `apply_custom_domain_config()` 以支持向后兼容
**文件**: `src-tauri/src/api/tunnel.rs`

```rust
// 保留旧函数，自动转换为新的 tunnel_mode 调用
pub async fn apply_custom_domain_config<R: Runtime>(
    app: AppHandle<R>,
    custom_domain: Option<String>,
    use_custom_domain: bool,
) -> Result<(), String> {
    let tunnel_mode = if use_custom_domain { "custom-domain" } else { "temporary" };
    apply_tunnel_config(app, tunnel_mode, custom_domain, None).await
}
```

### 5. 改进 `determine_tunnel_url()` 函数
**文件**: `src-tauri/src/api/n8n_core.rs`

**旧逻辑**（基于 boolean）：
```rust
fn determine_tunnel_url(
    use_custom_domain: bool,
    custom_domain: Option<String>,
    tunnel_url: Option<String>,
) -> Option<String> { ... }
```

**新逻辑**（基于 tunnel_mode）：
```rust
fn determine_tunnel_url(
    tunnel_mode: &str,
    custom_domain: Option<String>,
    tunnel_url: Option<String>,
) -> Option<String> {
    match tunnel_mode {
        "custom-domain" => {
            // 返回配置的自定义域名
            custom_domain.filter(|d| !d.trim().is_empty())
        }
        "token" => {
            // 使用 cloudflared 生成的 URL
            tunnel_url
        }
        _ => {
            // 临时隧道：使用 cloudflared 生成的临时 URL
            tunnel_url
        }
    }
}
```

### 6. 更新 `construct_n8n_envs()` 函数调用
**文件**: `src-tauri/src/api/n8n_core.rs`

```rust
// 旧代码
let (use_custom_domain, custom_domain) = { ... };
if let Some(final_url) = determine_tunnel_url(use_custom_domain, custom_domain, tunnel_url) { ... }

// 新代码
let (tunnel_mode, custom_domain) = { ... };
if let Some(final_url) = determine_tunnel_url(&tunnel_mode, custom_domain, tunnel_url) { ... }
```

### 7. 新增 Tauri 命令处理
**文件**: `src-tauri/src/api/commands.rs`

```rust
/// 应用新的隧道配置（支持三种模式）
#[tauri::command]
pub async fn apply_tunnel_config<R: Runtime>(
    app: AppHandle<R>,
    tunnel_mode: String,
    custom_domain: Option<String>,
    tunnel_token: Option<String>,
) -> Result<(), String> {
    tunnel::apply_tunnel_config(
        app,
        &tunnel_mode,
        custom_domain,
        tunnel_token,
    )
    .await
}

/// 应用自定义域名配置并重启 n8n（保留以向后兼容）
#[tauri::command]
pub async fn apply_custom_domain_config<R: Runtime>(
    app: AppHandle<R>,
    custom_domain: Option<String>,
    use_custom_domain: bool,
) -> Result<(), String> {
    tunnel::apply_custom_domain_config(app, custom_domain, use_custom_domain).await
}
```

---

## CSS 改动

### 新增隧道模式选择器样式
**文件**: `src/App.css`

```css
.service-item {
  padding: 8px 0;
}

.service-item-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}

.item-label {
  font-size: 13px;
  font-weight: 600;
  color: #334155;
}
```

---

## 测试清单

- [x] React 编译通过（Vite 打包成功）
- [x] Rust 编译通过（cargo check 无错误）
- [ ] 临时隧道模式启动
- [ ] 自定义域名模式配置和启动
- [ ] Token 模式配置和启动
- [ ] 隧道模式切换正常重启
- [ ] 配置持久化（重启应用后配置保存）
- [ ] 向后兼容性（旧配置正确加载）
- [ ] 验证逻辑（错误提示正确显示）

---

## 配置文件迁移

### 自动处理流程
1. 应用启动时读取 `tunnel-config.json`
2. 如果缺少 `tunnel_mode` 字段，自动设置为 `"temporary"`
3. 保存时使用新格式，保留旧字段以向后兼容

### 手动迁移示例

**旧格式**：
```json
{
  "use_custom_domain": true,
  "custom_domain": "https://myapp.example.com"
}
```

**新格式**（自动升级）：
```json
{
  "use_custom_domain": true,
  "custom_domain": "https://myapp.example.com",
  "tunnel_mode": "custom-domain",
  "tunnel_token": null
}
```

