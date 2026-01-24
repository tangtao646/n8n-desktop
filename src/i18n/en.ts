import { Translations } from './types';

export const en: Translations = {
  app: {
    title: 'n8n Desktop',
    redirecting: 'Redirecting to n8n...',
    retry: 'Retry',
    ready: 'n8n is ready!',
    open_editor: 'Open n8n Editor',
    show_tunnel: 'Show Tunnel Manager',
    hide_tunnel: 'Hide Tunnel Manager',
    tunnel_active: 'Tunnel Active:',
    open_via_tunnel: 'Open via Tunnel',
  },
  status: {
    checking: 'Checking system environment...',
    preparing_engine: 'Preparing Node engine... {{progress}}%',
    downloading_n8n: 'Downloading n8n resources... {{progress}}%',
    extracting: 'Extracting resource package...',
    preparing_tunnel: 'Preparing Cloudflare Tunnel... {{progress}}%',
    starting: 'Starting n8n service...',
    error: 'Startup failed: {{error}}',
    loading: 'Loading interface...',
  },
  errors: {
    timeout: 'n8n service startup timeout, please check if the port is occupied',
    startup_timeout: 'Startup timeout, please check network connection or restart the application',
  },
};