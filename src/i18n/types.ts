export type Language = 'en' | 'zh';

export interface Translations {
  app: {
    title: string;
    redirecting: string;
    retry: string;
    ready: string;
    open_editor: string;
    show_tunnel: string;
    hide_tunnel: string;
    tunnel_active: string;
    open_via_tunnel: string;
  };
  status: {
    checking: string;
    preparing_engine: string;
    downloading_n8n: string;
    extracting: string;
    preparing_tunnel: string;
    starting: string;
    error: string;
    loading: string;
  };
  errors: {
    timeout: string;
    startup_timeout: string;
  };
}