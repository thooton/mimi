/* Presentation metadata for language codes the backend may publish. Course
   availability belongs to GET /courses; this table only supplies names,
   endonyms, and an intentionally chosen flag when we know the language. */

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
}

export const LANGUAGES: LanguageOption[] = [
  { code: 'es', name: 'Spanish', endonym: 'Español', region: 'es' },
  { code: 'fr', name: 'French', endonym: 'Français', region: 'fr' },
  { code: 'de', name: 'German', endonym: 'Deutsch', region: 'de' },
  { code: 'it', name: 'Italian', endonym: 'Italiano', region: 'it' },
  { code: 'pt', name: 'Portuguese', endonym: 'Português', region: 'br' },
  { code: 'ja', name: 'Japanese', endonym: '日本語', region: 'jp' },
  { code: 'ko', name: 'Korean', endonym: '한국어', region: 'kr' },
  { code: 'zh', name: 'Chinese', endonym: '中文', region: 'cn' },
  { code: 'ru', name: 'Russian', endonym: 'Русский', region: 'ru' },
  { code: 'ar', name: 'Arabic', endonym: 'العربية', region: 'sa' },
  { code: 'hi', name: 'Hindi', endonym: 'हिन्दी', region: 'in' },
  { code: 'nl', name: 'Dutch', endonym: 'Nederlands', region: 'nl' },
  { code: 'pl', name: 'Polish', endonym: 'Polski', region: 'pl' },
  { code: 'tr', name: 'Turkish', endonym: 'Türkçe', region: 'tr' },
  { code: 'sv', name: 'Swedish', endonym: 'Svenska', region: 'se' },
  { code: 'no', name: 'Norwegian', endonym: 'Norsk', region: 'no' },
  { code: 'da', name: 'Danish', endonym: 'Dansk', region: 'dk' },
  { code: 'fi', name: 'Finnish', endonym: 'Suomi', region: 'fi' },
  { code: 'el', name: 'Greek', endonym: 'Ελληνικά', region: 'gr' },
  { code: 'he', name: 'Hebrew', endonym: 'עברית', region: 'il' },
  { code: 'uk', name: 'Ukrainian', endonym: 'Українська', region: 'ua' },
  { code: 'cs', name: 'Czech', endonym: 'Čeština', region: 'cz' },
  { code: 'ro', name: 'Romanian', endonym: 'Română', region: 'ro' },
  { code: 'hu', name: 'Hungarian', endonym: 'Magyar', region: 'hu' },
  { code: 'vi', name: 'Vietnamese', endonym: 'Tiếng Việt', region: 'vn' },
  { code: 'th', name: 'Thai', endonym: 'ไทย', region: 'th' },
  { code: 'id', name: 'Indonesian', endonym: 'Bahasa Indonesia', region: 'id' },
  { code: 'sw', name: 'Swahili', endonym: 'Kiswahili', region: 'ke' },
  { code: 'ga', name: 'Irish', endonym: 'Gaeilge', region: 'ie' },
  { code: 'en', name: 'English', endonym: 'English', region: 'gb' },
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
