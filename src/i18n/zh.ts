import { Translations } from './types';

export const zh: Translations = {
  app: {
    title: 'n8n 桌面版',
    redirecting: '正在跳转到 n8n...',
    retry: '重试',
  },
  status: {
    checking: '正在检查系统环境...',
    preparing_engine: '正在准备 Node 引擎... {{progress}}%',
    downloading_n8n: '正在下载 n8n 资源... {{progress}}%',
    extracting: '正在解压资源包...',
    starting: '正在启动 n8n 服务...',
    error: '启动失败: {{error}}',
    loading: '正在载入界面...',
  },
  errors: {
    timeout: 'n8n 服务启动超时，请检查端口是否被占用',
    startup_timeout: '启动超时，请检查网络连接或重启应用',
  },
};