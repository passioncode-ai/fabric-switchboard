/**
 * Interface language. The window follows the system language (Russian when the first preferred
 * language is Russian, English otherwise); About → Language overrides it per viewer. The choice is
 * read once, when the window starts: changing it reloads the window, so text built at start-up —
 * the tour, labels in tables — is in one language.
 *
 * English is the source text and the key: `t('Refresh')`. A string missing from the Russian
 * dictionary shows in English rather than as a key. `{name}` placeholders take values from
 * `params`. `scripts/check-locale.mjs` (in the gate) fails when an English string the interface
 * uses has no Russian entry.
 */
import { RU } from './locales/ru';

export type Locale = 'en' | 'ru';
export type LocaleChoice = 'system' | Locale;
export const LOCALE_KEY = 'switchboard.locale';

/** The language a list of preferred languages (navigator.languages) asks for. */
export function detectLocale(languages: readonly string[] | undefined): Locale {
  const first = (languages ?? [])[0]?.toLowerCase() ?? '';
  return first === 'ru' || first.startsWith('ru-') ? 'ru' : 'en';
}

/** A stored choice; anything unknown is the system default. */
export function parseLocaleChoice(value: unknown): LocaleChoice {
  return value === 'en' || value === 'ru' ? value : 'system';
}

export function resolveLocale(choice: LocaleChoice, languages: readonly string[] | undefined): Locale {
  return choice === 'system' ? detectLocale(languages) : choice;
}

function storedChoice(): LocaleChoice {
  if (typeof window === 'undefined') return 'system';
  try { return parseLocaleChoice(window.localStorage.getItem(LOCALE_KEY)); } catch { return 'system'; }
}

let current: Locale = resolveLocale(storedChoice(), globalThis.navigator?.languages);

export function locale(): Locale { return current; }
/** For tests and the demo: switch without a reload. */
export function setLocale(next: Locale) { current = next; }

/** Saves the viewer's choice; the caller reloads the window. False when it could not be saved. */
export function saveLocaleChoice(choice: LocaleChoice): boolean {
  if (typeof window === 'undefined') return false;
  try {
    if (choice === 'system') window.localStorage.removeItem(LOCALE_KEY);
    else window.localStorage.setItem(LOCALE_KEY, choice);
    return true;
  } catch { return false; }
}

const fill = (text: string, params?: Record<string, string | number>) =>
  params ? text.replace(/\{(\w+)\}/g, (whole, name: string) => (name in params ? String(params[name]) : whole)) : text;

/** The interface text for an English source string, in the current language. */
export function t(source: string, params?: Record<string, string | number>): string {
  return fill(current === 'ru' ? RU[source] ?? source : source, params);
}

/**
 * A count with its noun. `forms` are English: one and other (`{n}` is the count). Russian takes
 * its three forms from the dictionary entry of `forms.other`, separated by `|`:
 * "{n} правило|{n} правила|{n} правил" — one (1, 21, 31…), few (2–4, 22–24…), many (the rest).
 */
export function plural(n: number, forms: { one: string; other: string }, params?: Record<string, string | number>): string {
  const values = { ...params, n };
  if (current !== 'ru') return fill(n === 1 ? forms.one : forms.other, values);
  const entry = RU[forms.other];
  if (!entry) return fill(n === 1 ? forms.one : forms.other, values);
  const [one, few = one, many = few] = entry.split('|');
  const mod10 = Math.abs(n) % 10;
  const mod100 = Math.abs(n) % 100;
  const form = mod10 === 1 && mod100 !== 11 ? one : mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14) ? few : many;
  return fill(form, values);
}

/** Dates and times: Russian in Russian, the system's own format otherwise (as before). */
export function dateLocale(): string | undefined { return current === 'ru' ? 'ru-RU' : undefined; }
