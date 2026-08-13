/* Every language the app offers to teach.

   Only courses marked `available` are shown to users. The remaining language
   metadata is retained for API values and future implementation, without
   advertising courses that do not exist yet. */

export interface LanguageOption {
  /** ISO 639-1, and what the backend means by `target_lang` */
  code: string;
  /** what an English-speaking user calls it */
  name: string;
  /** what it calls itself — the picker shows both, the way every language
      selector worth using does */
  endonym: string;
  /**
   * ISO 3166-1 alpha-2, naming the flag in /public/flags.
   *
   * Deliberately not derived from `code`: a language is not a country, and
   * the two only line up by luck. Spanish is spoken in twenty countries and
   * we fly Spain's flag; Portuguese gets Brazil's because that is the
   * bigger half; Arabic has no country at all and borrows Saudi Arabia's.
   * These are choices, so they are written down as choices.
   */
  region: string;
  /** a course exists on the backend and can actually be learnt */
  available: boolean;
}

export const LANGUAGES: LanguageOption[] = [
  { code: 'es', name: 'Spanish', endonym: 'Español', region: 'es', available: true },
  { code: 'fr', name: 'French', endonym: 'Français', region: 'fr', available: false },
  { code: 'de', name: 'German', endonym: 'Deutsch', region: 'de', available: false },
  { code: 'it', name: 'Italian', endonym: 'Italiano', region: 'it', available: false },
  { code: 'pt', name: 'Portuguese', endonym: 'Português', region: 'br', available: false },
  { code: 'ja', name: 'Japanese', endonym: '日本語', region: 'jp', available: false },
  { code: 'ko', name: 'Korean', endonym: '한국어', region: 'kr', available: false },
  { code: 'zh', name: 'Chinese', endonym: '中文', region: 'cn', available: false },
  { code: 'ru', name: 'Russian', endonym: 'Русский', region: 'ru', available: false },
  { code: 'ar', name: 'Arabic', endonym: 'العربية', region: 'sa', available: false },
  { code: 'hi', name: 'Hindi', endonym: 'हिन्दी', region: 'in', available: false },
  { code: 'nl', name: 'Dutch', endonym: 'Nederlands', region: 'nl', available: false },
  { code: 'pl', name: 'Polish', endonym: 'Polski', region: 'pl', available: false },
  { code: 'tr', name: 'Turkish', endonym: 'Türkçe', region: 'tr', available: false },
  { code: 'sv', name: 'Swedish', endonym: 'Svenska', region: 'se', available: false },
  { code: 'no', name: 'Norwegian', endonym: 'Norsk', region: 'no', available: false },
  { code: 'da', name: 'Danish', endonym: 'Dansk', region: 'dk', available: false },
  { code: 'fi', name: 'Finnish', endonym: 'Suomi', region: 'fi', available: false },
  { code: 'el', name: 'Greek', endonym: 'Ελληνικά', region: 'gr', available: false },
  { code: 'he', name: 'Hebrew', endonym: 'עברית', region: 'il', available: false },
  { code: 'uk', name: 'Ukrainian', endonym: 'Українська', region: 'ua', available: false },
  { code: 'cs', name: 'Czech', endonym: 'Čeština', region: 'cz', available: false },
  { code: 'ro', name: 'Romanian', endonym: 'Română', region: 'ro', available: false },
  { code: 'hu', name: 'Hungarian', endonym: 'Magyar', region: 'hu', available: false },
  { code: 'vi', name: 'Vietnamese', endonym: 'Tiếng Việt', region: 'vn', available: false },
  { code: 'th', name: 'Thai', endonym: 'ไทย', region: 'th', available: false },
  { code: 'id', name: 'Indonesian', endonym: 'Bahasa Indonesia', region: 'id', available: false },
  { code: 'sw', name: 'Swahili', endonym: 'Kiswahili', region: 'ke', available: false },
  { code: 'ga', name: 'Irish', endonym: 'Gaeilge', region: 'ie', available: false },
  { code: 'en', name: 'English', endonym: 'English', region: 'gb', available: false },
];

const BY_CODE = new Map(LANGUAGES.map((l) => [l.code, l]));

export function languageByCode(code: string | null | undefined): LanguageOption | undefined {
  return code ? BY_CODE.get(code) : undefined;
}

/** the display name for a language code, falling back to the code itself —
    the backend is free to name a language we haven't listed */
export function languageName(code: string): string {
  return BY_CODE.get(code)?.name ?? code.toUpperCase();
}

/** The courses that currently exist. All course selectors render this list. */
export const AVAILABLE = LANGUAGES.filter((l) => l.available);
