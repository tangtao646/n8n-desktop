import { Language, Translations } from './types';
import { en } from './en';
import { zh } from './zh';

export const translations: Record<Language, Translations> = {
  en,
  zh,
};

export { type Language, type Translations };
export { en, zh };