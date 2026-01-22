import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react';
import { Language, translations } from './index';

interface I18nContextType {
  language: Language;
  setLanguage: (lang: Language) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextType | undefined>(undefined);

// Helper function to get translation with parameter substitution
const getTranslation = (obj: any, keyPath: string, params?: Record<string, string | number>): string => {
  const keys = keyPath.split('.');
  let value: any = obj;
  
  for (const key of keys) {
    if (value && typeof value === 'object' && key in value) {
      value = value[key];
    } else {
      return keyPath; // Fallback to key path if not found
    }
  }
  
  if (typeof value !== 'string') {
    return keyPath;
  }
  
  // Replace parameters like {{param}}
  if (params) {
    return value.replace(/\{\{(\w+)\}\}/g, (match: string, paramName: string) => {
      return params[paramName]?.toString() || match;
    });
  }
  
  return value;
};

interface I18nProviderProps {
  children: ReactNode;
  defaultLanguage?: Language;
}

export const I18nProvider: React.FC<I18nProviderProps> = ({ 
  children, 
  defaultLanguage = 'en' 
}) => {
  // Try to get language from localStorage or browser preference
  const getInitialLanguage = (): Language => {
    const saved = localStorage.getItem('n8n-desktop-language') as Language;
    if (saved && (saved === 'en' || saved === 'zh')) {
      return saved;
    }
    
    // Detect browser language
    const browserLang = navigator.language.toLowerCase();
    if (browserLang.startsWith('zh')) {
      return 'zh';
    }
    
    return defaultLanguage;
  };

  const [language, setLanguageState] = useState<Language>(getInitialLanguage);

  const setLanguage = (lang: Language) => {
    setLanguageState(lang);
    localStorage.setItem('n8n-desktop-language', lang);
  };

  const t = (key: string, params?: Record<string, string | number>): string => {
    return getTranslation(translations[language], key, params);
  };

  useEffect(() => {
    // Update document language attribute
    document.documentElement.lang = language;
    // Both English and Chinese are LTR languages
    document.documentElement.dir = 'ltr';
  }, [language]);

  return (
    <I18nContext.Provider value={{ language, setLanguage, t }}>
      {children}
    </I18nContext.Provider>
  );
};

export const useI18n = (): I18nContextType => {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error('useI18n must be used within an I18nProvider');
  }
  return context;
};