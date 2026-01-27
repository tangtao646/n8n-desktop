# Cloudflare Tunnel 三种模式功能验证

## ✅ 现状检查

### 前端实现
- ✅ UI 组件完整 - 支持三种隧道模式切换
- ✅ 模式选择按钮 - 临时隧道 / 自定义域名 / Token 固定
- ✅ 配置输入字段 - 自定义域名输入 / Token 输入框
- ✅ 调用后端命令 - `invoke("apply_tunnel_config", {...})`
- ✅ TypeScript 编译 - ✓ 成功

### 后端实现
- ✅ tunnel.rs - 支持三种模式的 start_tunnel 逻辑
- ✅ tunnel.rs - apply_tunnel_config 函数完整
- ✅ n8n_core.rs - determine_tunnel_url 正确处理三种模式
- ✅ commands.rs - apply_tunnel_config 命令定义
- ✅ lib.rs - **✅ 已注册** apply_tunnel_config 命令
- ✅ Rust 编译 - ✓ 成功（仅有代码风格警告）

## 三种模式详细说明

### 1. 临时隧道（Temporary）
```
模式: "temporary"
参数: 无需额外参数
行为: 每次启动时随机生成新的 trycloudflare.com 域名
适用场景: 开发测试、临时公网访问
```

**工作流程:**
1. cloudflared 启动时使用 `--url http://localhost:5678` 参数
2. Cloudflare 自动分配临时域名 (如: `https://xxx.trycloudflare.com`)
3. 域名通过 stderr 解析并提取
4. 设置 `WEBHOOK_URL` 环境变量
5. n8n webhook 节点自动使用这个 URL

### 2. 自定义域名（Custom Domain）
```
模式: "custom-domain"
参数: customDomain = "https://your-domain.com"
行为: 使用已在 Cloudflare 注册的固定域名
适用场景: 生产环境、需要固定域名
```

**工作流程:**
1. 用户在 Cloudflare 仪表盘创建 Tunnel
2. 获取 Tunnel ID (如: `my-tunnel`)
3. 在 Cloudflare DNS 中添加 CNAME 记录:
   - 类型: CNAME
   - 名称: `your-domain.com`
   - 内容: `my-tunnel.cfargotunnel.com`
4. 本地配置中保存 `tunnel_mode = "custom-domain"`
5. 启动 cloudflared: `cloudflared tunnel run my-tunnel`
6. 设置 `WEBHOOK_URL = "https://your-domain.com"`

### 3. Token 固定隧道（Token Mode）
```
模式: "token"
参数: tunnelToken = "<长Token字符串>"
行为: 使用 Cloudflare Tunnel 的 Token 连接固定隧道
适用场景: 跨机器持久隧道、CI/CD 自动化
```

**工作流程:**
1. 用户在 Cloudflare 仪表盘创建 Tunnel
2. 获取 Tunnel Token (长字符串，通常 > 100 字符)
3. 本地配置中保存 Token 和 `tunnel_mode = "token"`
4. 启动 cloudflared: `cloudflared tunnel run --token <TOKEN>`
5. cloudflared 使用 Token 连接到 Cloudflare 的特定隧道
6. 隧道恢复其预配置的公网 URL
7. 设置 `WEBHOOK_URL` 环境变量

## 关键实现细节

### 配置保存流程
```
前端 input → invoke("apply_tunnel_config")
  ↓
commands.rs: apply_tunnel_config()
  ↓
tunnel.rs: apply_tunnel_config()
  ├─ 验证输入 (custom-domain 和 token 需要非空)
  ├─ 更新 TUNNEL_CONFIG 全局变量
  ├─ 保存到 config.json (~/.n8n-desktop/tunnel_config.json)
  └─ 如果 n8n 正在运行，重启 n8n 进程
```

### 启动隧道流程
```
前端 toggle → invoke("start_tunnel", cloudflared_path)
  ↓
tunnel.rs: start_tunnel()
  ├─ 清理旧的 cloudflared 进程
  ├─ 根据 tunnel_mode 启动不同的 cloudflared 命令
  │  ├─ temporary: cloudflared tunnel --url http://localhost:5678
  │  ├─ custom-domain: cloudflared tunnel run <domain-id>
  │  └─ token: cloudflared tunnel run --token <TOKEN>
  ├─ 监听 stderr 获取公网 URL
  └─ 更新 TUNNEL_URL 并通知前端
```

### 环境变量设置流程
```
隧道 URL 获取后 → restart_n8n_with_env()
  ↓
n8n_core.rs: construct_n8n_envs()
  ├─ 读取 TUNNEL_CONFIG (tunnel_mode, custom_domain, tunnel_token)
  ├─ 读取 TUNNEL_URL (cloudflared 输出的 URL)
  ├─ determine_tunnel_url() 判断使用哪个 URL
  │  ├─ custom-domain: 使用配置的 customDomain
  │  ├─ token: 使用 cloudflared 输出的 URL
  │  └─ temporary: 使用 cloudflared 输出的 URL
  └─ 设置 WEBHOOK_URL, N8N_EDITOR_BASE_URL, N8N_CORS_ALLOWED_ORIGINS
```

## 验证检查列表

### 编译状态
- [x] Rust 库编译成功
- [x] React 前端编译成功
- [x] apply_tunnel_config 命令已注册

### 功能检查
- [ ] **开发中测试** - 运行 `cargo tauri dev` 测试
- [ ] 临时隧道模式 - 启动后能否获得 trycloudflare.com 地址
- [ ] 自定义域名模式 - 输入域名并启动隧道
- [ ] Token 模式 - 输入 Token 并启动隧道

## 可能的问题与解决

### 问题1: Token 模式启动失败
**症状:** "Invalid tunnel token" 错误

**检查:**
1. Token 长度是否 >= 50 字符
2. Token 是否完整复制（没有换行或空格）
3. Token 是否来自正确的 Tunnel

**解决:**
```bash
# 从 Cloudflare 仪表盘重新复制完整 Token
# 在命令行测试:
cloudflared tunnel run --token <YOUR_TOKEN>
```

### 问题2: 自定义域名无法连接
**症状:** 域名无法访问

**检查:**
1. 域名是否在 Cloudflare 注册
2. CNAME 记录是否正确配置
3. Tunnel ID 是否匹配

**解决:**
```bash
# 测试 cloudflared 命令
cloudflared tunnel run my-tunnel-id

# 查看 Cloudflare 仪表盘中隧道的 DNS 配置
```

### 问题3: webhook URL 未更新
**症状:** 切换模式后 webhook 节点中的 URL 没有变化

**原因:** n8n 前端缓存 webhook URL

**解决:**
1. 在新 URL 加载后点击"刷新 n8n UI"按钮
2. 或者重新登录 n8n 账户

## 测试命令

### 完整的开发构建
```bash
cd /Users/tangtao/Projects/n8n-desktop

# 前端
npm run build

# 后端
cd src-tauri
cargo build --lib

# 运行开发版本
cargo tauri dev
```

### 测试隧道功能
```javascript
// 在浏览器控制台测试
// 临时隧道
await window.__TAURI__.core.invoke('apply_tunnel_config', {
  tunnel_mode: 'temporary',
  custom_domain: null,
  tunnel_token: null
})

// 自定义域名
await window.__TAURI__.core.invoke('apply_tunnel_config', {
  tunnel_mode: 'custom-domain',
  custom_domain: 'https://my-app.example.com',
  tunnel_token: null
})

// Token 模式
await window.__TAURI__.core.invoke('apply_tunnel_config', {
  tunnel_mode: 'token',
  custom_domain: null,
  tunnel_token: 'eyJhIjoiZTEx...'  // 完整 Token
})
```

## 总结

✅ **当前可用状态:** 
- 所有代码已编译成功
- 前后端实现完整
- 三种隧道模式都支持
- 缺少的唯一东西是 apply_tunnel_config 命令的注册 **已修复** ✓

🚀 **下一步:**
运行 `cargo tauri dev` 进行实际测试验证三种模式的功能
