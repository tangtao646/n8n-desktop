import { Translations } from './types';

export const en: Translations = {
  app: {
    title: 'n8n Desktop',
    redirecting: 'Redirecting to n8n...',
    retry: 'Retry',
  },
  status: {
    checking: 'Checking system environment...',
    preparing_engine: 'Preparing Node engine... {{progress}}%',
    downloading_n8n: 'Downloading n8n resources... {{progress}}%',
    extracting: 'Extracting resource package...',
    starting: 'Starting n8n service...',
    error: 'Startup failed: {{error}}',
    loading: 'Loading interface...',
  },
  errors: {
    timeout: 'n8n service startup timeout, please check if the port is occupied',
    startup_timeout: 'Startup timeout, please check network connection or restart the application',
  },
};