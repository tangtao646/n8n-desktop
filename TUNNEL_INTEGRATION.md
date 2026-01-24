# Cloudflare Tunnel 集成文档

## 概述
本功能在 n8n-desktop 应用中集成了 Cloudflare Tunnel，实现一键开启公网访问，并自动同步更新 n8n 的 WEBHOOK_URL 环境变量。

## 实现功能

### 1. Rust 后端实现
在 `src-tauri/src/api/commands.rs` 中添加了以下隧道管理命令：

#### 新增命令：
1. `start_tunnel` - 启动 Cloudflare Tunnel
2. `stop_tunnel` - 停止 Cloudflare Tunnel  
3. `get_tunnel_status` - 获取隧道状态

#### 核心特性：
- 使用正则表达式 `https://[a-z0-9-]+\.trycloudflare\.com` 提取动态域名
- 自动重启 n8n 进程并注入环境变量：
  - `WEBHOOK_URL`: Cloudflare 隧道域名
  - `N8N_EDITOR_BASE_URL`: Cloudflare 隧道域名
- 通过 Tauri Event 系统发送隧道状态更新到前端

### 2. 环境变量注入
当隧道启动并获取到域名后，系统会：
1. 停止现有的 n8n 进程
2. 使用新的环境变量重启 n8n 进程
3. 确保 n8n 生成的 Webhook URL 使用公网域名

### 3. 前端集成
隧道命令已集成到主应用程序的命令处理器中，前端可以通过以下方式调用：

```typescript
// 启动隧道
await invoke('start_tunnel');

// 停止隧道  
await invoke('stop_tunnel');

// 获取隧道状态
const status = await invoke<TunnelEvent>('get_tunnel_status');

// 监听隧道状态更新
import { listen } from '@tauri-apps/api/event';
listen('tunnel-update', (event) => {
  const tunnelEvent = event.payload as TunnelEvent;
  console.log('隧道状态:', tunnelEvent.status);
  console.log('隧道URL:', tunnelEvent.url);
});
```

### 4. 类型定义
```typescript
interface TunnelEvent {
  status: 'Connecting' | 'Online' | 'Offline' | 'Error';
  url?: string;
}
```

## 技术架构

### 进程管理
1. **Cloudflared 进程**: 通过 `Command::new("cloudflared")` 启动（需要 cloudflared 在系统 PATH 中）
2. **输出监控**: 使用线程监控 stdout，正则表达式 `https://[a-z0-9-]+\.trycloudflare\.com` 提取域名
3. **n8n 进程管理**: 通过现有的 `PROCESS_MANAGER` 管理 n8n 生命周期，隧道启动时自动重启 n8n 并注入环境变量

### 状态管理
使用全局静态变量管理隧道状态：
- `TUNNEL_URL`: 存储当前隧道域名
- `TUNNEL_RUNNING`: 隧道运行状态

### 错误处理
- 隧道已运行时的错误提示
- Cloudflared 未安装时的友好错误信息
- 网络断开时的自动重连（由 cloudflared 处理）

## 使用流程

### 场景 A: 配置 Google OAuth
1. 用户点击"开启外网访问"按钮
2. 应用启动 cloudflared 隧道
3. 系统提取 `xxx.trycloudflare.com` 域名
4. n8n 自动重启，回调地址从 `127.0.0.1` 变为 `https://xxx.trycloudflare.com`
5. 用户复制公网域名用于 OAuth 配置

### 场景 B: 关闭隧道
1. 用户关闭应用或点击"断开隧道"
2. cloudflared 进程被终止
3. 域名失效，确保隐私安全
4. n8n 继续在本地运行

## 配置要求

### 系统依赖
- **cloudflared**: 需要预先安装并添加到系统 PATH 环境变量
  - macOS: `brew install cloudflared`
  - Linux: 从 [Cloudflare 官网](https://developers.cloudflare.com/cloudflare-one/connections/connect-apps/install-and-setup/installation/) 下载并安装
  - Windows: 使用安装程序或手动添加到 PATH
- **验证安装**: 在终端运行 `cloudflared --version` 确认安装成功

### 端口要求
- n8n 必须在 `localhost:5678` 端口运行并监听
- 隧道启动前会通过 `proxy_health_check` 命令验证 n8n 服务状态
- 确保防火墙允许 cloudflared 访问本地端口

## 边缘情况处理

### 1. 隧道重连
- cloudflared 自动处理网络断开重连
- 如果域名改变，系统会重新提取并更新 n8n 环境变量

### 2. 端口占用
- 启动隧道前检查 `localhost:5678` 是否可访问
- 通过 `proxy_health_check` 命令验证 n8n 状态

### 3. 进程生命周期
- 应用退出时自动停止隧道
- 隧道进程与应用主进程绑定

## 测试验证

### 编译测试
```bash
cd src-tauri
cargo check    # 语法检查
cargo build    # 构建测试
```

### 功能测试步骤
1. 确保 cloudflared 已安装
2. 启动 n8n-desktop 应用
3. 调用 `start_tunnel` 命令
4. 验证隧道域名提取
5. 检查 n8n 环境变量更新
6. 测试 Webhook 地址生成

## 后续优化建议

1. **Sidecar 集成**: 将 cloudflared 二进制打包为应用资源
2. **UI 增强**: 添加隧道状态显示和复制按钮
3. **配置持久化**: 保存隧道配置和上次使用的域名
4. **多隧道支持**: 支持多个隧道实例
5. **错误恢复**: 更完善的错误处理和恢复机制

## 文件变更

### 新增/修改文件
1. `src-tauri/src/api/commands.rs` - 添加隧道命令实现
2. `src-tauri/Cargo.toml` - 添加 regex 依赖
3. `src-tauri/src/lib.rs` - 集成隧道命令到处理器

### 删除文件
1. `src-tauri/src/bin/tunnel.rs` - 独立的隧道二进制（已集成到主应用）

## 注意事项

1. **安全性**: 隧道开启期间，n8n 实例可通过公网访问
2. **性能**: 隧道会增加网络延迟
3. **依赖**: 需要稳定的互联网连接
4. **兼容性**: 依赖 cloudflared 的稳定性和 API 兼容性