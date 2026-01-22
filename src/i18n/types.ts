export type Language = 'en' | 'zh';

export interface Translations {
  app: {
    title: string;
    redirecting: string;
    retry: string;
  };
  status: {
    checking: string;
    preparing_engine: string;
    downloading_n8n: string;
    extracting: string;
    starting: string;
    error: string;
    loading: string;
  };
  errors: {
    timeout: string;
    startup_timeout: string;
  };
}