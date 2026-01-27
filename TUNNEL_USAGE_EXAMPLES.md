# 隧道配置使用示例

## 场景 1：快速测试（临时隧道）

### 步骤
1. 打开 n8n Desktop 应用
2. 在左侧边栏找到"隧道配置"部分
3. 确保选中 **"临时隧道"** 按钮（默认选中）
4. 点击隧道开关打开隧道
5. 等待连接建立，获得类似 `https://abc123.trycloudflare.com` 的临时 URL

### 效果
- 每次启动都会获得新的 URL
- 无需任何配置，即插即用
- 适合快速测试和临时共享

---

## 场景 2：使用自定义域名（固定 URL）

### 前置条件
1. 拥有自己的域名（例如 `example.com`）
2. 域名已在 Cloudflare 中注册或转入

### Cloudflare 侧配置

#### Step 1：创建隧道
1. 登录 [Cloudflare 仪表盘](https://dash.cloudflare.com)
2. 进入 **Zero Trust** > **Tunnels**（或搜索 "Tunnels"）
3. 点击 **"Create a tunnel"**
4. 给隧道命名，例如 `n8n-app`
5. 在"Connectors"选项卡中找到隧道 ID（例如 `123e4567-e89b-12d3-a456-426614174000`）或在隧道名称显示的地方

#### Step 2：配置公网路由
1. 在隧道详情页，进入 **"Public Hostnames"** 标签
2. 点击 **"Add a public hostname"**
3. 填写配置：
   - **Subdomain**: `n8n` （或任何你想要的子域名）
   - **Domain**: `example.com`
   - **Type**: `HTTPS`
   - **URL**: `http://localhost:5678`
4. 点击 **"Save"**

#### Step 3：获取隧道 ID
在 Tunnels 列表中可以看到你创建的隧道名称，点击进去可以看到完整的隧道 ID。

### n8n Desktop 侧配置

1. 打开应用，找到"隧道配置"部分
2. 点击 **"自定义域名"** 按钮
3. 在输入框中输入你的隧道 ID 或域名：
   - 如果使用隧道 ID：`n8n-app` 或完整 UUID
   - 如果使用完整域名：`n8n.example.com` 或 `https://n8n.example.com`
4. 点击 **"保存配置"**
5. 应用会自动重启 n8n 和隧道

### 结果
- n8n 现在可通过 `https://n8n.example.com` 访问
- 所有 webhook URL 使用 `https://n8n.example.com` 作为基础 URL
- 下次重启仍使用相同的 URL

### 示例 webhook URL
- 工作流 webhook：`https://n8n.example.com/webhook/abc123def`
- 二进制数据 webhook：`https://n8n.example.com/webhook-test/xyz789`

---

## 场景 3：使用 Token 固定隧道

### 前置条件
1. 在 Cloudflare 中已创建隧道
2. 生成了 Tunnel Connector Token

### 获取 Tunnel Token

#### 方法 A：从 Cloudflare CLI 命令复制
1. 登录 Cloudflare 仪表盘 > Tunnels
2. 选择你的隧道
3. 在 **"Connectors"** 标签中找到 Linux 的连接命令
4. 命令格式如下：
   ```bash
   cloudflared tunnel run --token eyJhIjoiYWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXp...
   ```
5. 复制 `--token` 后面的整个字符串（通常是很长的 Base64 编码）

#### 方法 B：从 Dashboard 复制
1. Tunnels > 选择隧道
2. 点击 **"Configure"**
3. 在 **"Connector Token"** 部分找到完整 Token
4. 点击复制按钮或手动选中复制

### n8n Desktop 侧配置

1. 打开应用，找到"隧道配置"部分
2. 点击 **"Token 固定"** 按钮
3. 在大文本框中粘贴完整的 Tunnel Token：
   ```
   eyJhIjoiYWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXpaYWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXpaYWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXpaYWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo...
   ```
4. 点击 **"保存配置"**
5. 应用会自动重启 n8n 和隧道

### 结果
- n8n 通过 Token 与 Cloudflare 建立固定隧道
- Cloudflare 分配固定的公网 URL（在 Dashboard 中可见）
- 每次重启使用相同的 URL
- 比临时隧道更稳定，比自定义域名更简单

### 常见的 Token 位置
```bash
# Cloudflare CLI 命令示例
cloudflared tunnel run --token eyJhIjoiNGI5Y2ExODctZDUwYS00MWY3LWJkZDUtMjI1ODc5MDk4NTg0IiwiYiBdiddd...
```

---

## 场景 4：在隧道模式间切换

### 从临时隧道切换到自定义域名
1. 选中 **"自定义域名"** 按钮
2. 输入你的隧道 ID 或域名
3. 点击 **"保存配置"**
4. 应用自动重启，开始使用新的固定 URL

### 从自定义域名切换到 Token
1. 选中 **"Token 固定"** 按钮
2. 粘贴 Cloudflare Tunnel Token
3. 点击 **"保存配置"**
4. 应用自动重启

### 从 Token 回到临时隧道
1. 选中 **"临时隧道"** 按钮
2. 直接关闭隧道开关，再打开
3. 获得新的临时 URL

---

## 故障排查

### 问题 1：自定义域名模式连接失败

**症状**：隧道显示"已连接"，但 n8n 无法通过域名访问

**解决方案**：
1. 检查 Cloudflare DNS 记录是否正确配置
   ```
   DNS 记录应该是：
   Type: CNAME
   Name: n8n
   Content: xxx.cfargotunnel.com
   ```
2. 检查隧道的 Public Hostname 配置是否正确
3. 在 Cloudflare Dashboard 中验证隧道状态是否为"Connected"
4. 尝试重启应用（完全关闭然后重新打开）

### 问题 2：Token 模式"Token 格式不正确"错误

**症状**：保存配置时显示"Token 格式似乎不正确"

**解决方案**：
1. 确保复制的是完整的 Token（应该很长，通常 1000+ 字符）
2. 不要在 Token 中间或开头/结尾包含额外的空格
3. 重新从 Cloudflare Dashboard 复制 Token
4. 确保 Token 未过期（如果隧道已删除，Token 会失效）

### 问题 3：切换模式后 n8n 没有重启

**症状**：修改了隧道模式和配置，但 n8n 还在使用旧的 URL

**解决方案**：
1. 等待 5-10 秒，应用后台可能还在重启
2. 如果仍未更新，手动关闭隧道开关，再打开
3. 检查应用日志（如果有）查看重启错误

### 问题 4：临时隧道无法获得 URL

**症状**：打开隧道 5 秒后仍显示"连接中..."

**解决方案**：
1. 确保 cloudflared 已安装（应用会自动下载）
2. 检查网络连接
3. 确保端口 5678 没有被其他应用占用：
   ```bash
   lsof -i :5678  # macOS/Linux
   netstat -ano | findstr :5678  # Windows
   ```
4. 尝试重启应用和 n8n

---

## 最佳实践

### 生产环境
- ✅ 使用 **自定义域名** 或 **Token 模式**（固定 URL）
- ✅ 配置完整的 SSL/TLS 证书（Cloudflare 会自动处理）
- ✅ 启用 Cloudflare 的 DDoS 防护
- ❌ 避免使用临时隧道（URL 每次都变化）

### 开发环境
- ✅ 使用 **临时隧道**（快速、无需配置）
- ✅ 使用 **Token 模式**（稳定）
- ❌ 频繁切换模式可能导致 webhook 失效

### 测试环境
- ✅ 使用 **自定义域名**（模拟生产环境）
- ✅ 使用测试域名的子域名

---

## 配置保存位置

所有隧道配置保存在以下位置：
- **macOS**: `~/.n8n-desktop/tunnel-config.json`
- **Linux**: `~/.n8n-desktop/tunnel-config.json`
- **Windows**: `%APPDATA%\.n8n-desktop\tunnel-config.json`

### 配置文件示例（Token 模式）
```json
{
  "last_url": "https://abc123.cfargotunnel.com",
  "auto_start": false,
  "created_at": "2026-01-27T10:30:00+00:00",
  "custom_domain": null,
  "use_custom_domain": false,
  "tunnel_mode": "token",
  "tunnel_token": "eyJhIjoiNGI5Y2ExODctZDUwYS00MWY3LWJkZDUtMjI1ODc5MDk4NTg0IiwiYiI6..."
}
```

---

## FAQ

### Q: 能否同时使用多个隧道？
A: 当前不支持，但可以快速切换。未来版本可能支持多隧道。

### Q: Token 过期多久需要更新？
A: Cloudflare Token 通常不会过期，但如果隧道被删除，Token 会失效。此时需要重新创建隧道并获取新 Token。

### Q: 隧道 URL 会暴露我的本地 IP 吗？
A: 不会。所有流量都通过 Cloudflare 代理，你的本地 IP 地址对外完全隐藏。

### Q: 能否限制隧道只允许特定的 IP 访问？
A: 可以，这是 Cloudflare 的功能，在 Dashboard 中配置。

### Q: 临时隧道和固定隧道有速度区别吗？
A: 没有显著区别，都通过 Cloudflare 全球 CDN。

### Q: 如何查看隧道的访问日志？
A: 在 Cloudflare Dashboard > Tunnels > 选择隧道 > Analytics 查看。

