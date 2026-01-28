import { useEffect } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

/**
 * 全局 UI 自动同步 Hook
 * 监听 app://sync-state 事件，触发时执行回调函数
 * 
 * @param onSync 同步事件触发时的回调函数
 */
export function useAutoSync(onSync: () => void) {
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    const setupListener = async () => {
      try {
        // 监听 app://sync-state 事件
        unlisten = await listen<string>('app://sync-state', (event) => {
          console.log('[useAutoSync] 收到同步事件，payload:', event.payload);
          onSync();
        });
        console.log('[useAutoSync] 监听器已注册');
      } catch (error) {
        console.error('[useAutoSync] 注册监听器失败:', error);
      }
    };

    setupListener();

    // 清理函数：组件卸载时取消监听
    return () => {
      if (unlisten) {
        unlisten();
        console.log('[useAutoSync] 监听器已清理');
      }
    };
  }, [onSync]);
}

/**
 * 生成带时间戳的 URL，用于强制刷新 iframe 或 WebView
 *
 * @param baseUrl 基础 URL
 * @returns 带时间戳参数的 URL
 */
export function generateTimestampedUrl(baseUrl: string): string {
  const timestamp = Date.now();
  const separator = baseUrl.includes('?') ? '&' : '?';
  return `${baseUrl}${separator}t=${timestamp}`;
}

/**
 * Hook 版本：生成带时间戳的 URL，用于强制刷新 iframe 或 WebView
 *
 * @param baseUrl 基础 URL
 * @returns 带时间戳参数的 URL
 */
export function useTimestampedUrl(baseUrl: string): string {
  return generateTimestampedUrl(baseUrl);
}