# Cloudflared 二进制文件配置指南

## 概述
n8n-desktop 应用需要 cloudflared 二进制文件来实现隧道功能。以下是配置方法：

## 方法一：自动下载（推荐）
在应用启动时自动下载对应平台的 cloudflared 二进制文件。

## 方法二：手动放置
将对应平台的 cloudflared 二进制文件放置在此目录：

### macOS
1. 从 https://github.com/cloudflare/cloudflared/releases 下载 `cloudflared-darwin-amd64`
2. 重命名为 `cloudflared`
3. 放入 `src-tauri/resources/cloudflared` 目录
4. 确保文件有执行权限：`chmod +x cloudflared`

### Windows
1. 从 https://github.com/cloudflare/cloudflared/releases 下载 `cloudflared-windows-amd64.exe`
2. 重命名为 `cloudflared.exe`
3. 放入 `src-tauri/resources/cloudflared.exe` 目录

### Linux
1. 从 https://github.com/cloudflare/cloudflared/releases 下载 `cloudflared-linux-amd64`
2. 重命名为 `cloudflared`
3. 放入 `src-tauri/resources/cloudflared` 目录
4. 确保文件有执行权限：`chmod +x cloudflared`

## 文件结构
```
src-tauri/resources/
├── cloudflared          # macOS/Linux 二进制文件
├── cloudflared.exe      # Windows 二进制文件
└── README.md           # 本文件
```

## 验证
应用启动时会检查资源目录中的 cloudflared 二进制文件。如果找到，将使用该文件；否则会尝试系统 PATH 中的 cloudflared。

## 构建说明
在构建应用时，这些资源文件会被打包到最终的应用包中，用户无需单独安装 cloudflared。