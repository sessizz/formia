// Explicit UI localization. Never translate arbitrary DOM text or user file names.
import { catalog, errorSources } from './translations.js';
export { catalog };

export function systemLanguages() {
  return globalThis.navigator?.languages?.length ? navigator.languages : [globalThis.navigator?.language];
}

export function resolveLanguage(saved, languages = systemLanguages()) {
  if (saved === 'tr' || saved === 'en') return saved;
  const primary = Array.isArray(languages) ? languages[0] : null;
  return typeof primary === 'string' && /^tr(?:[-_]|$)/i.test(primary) ? 'tr' : 'en';
}

let language = resolveLanguage(null);
export const getLanguage = () => language;
export const getLocale = () => language === 'tr' ? 'tr-TR' : 'en-US';
export function setLanguage(value) {
  language = resolveLanguage(value);
  if (globalThis.document) document.documentElement.lang = language;
}

/** Substitute once so braces in file names/other values remain literal. */
export function t(source, values = {}) {
  const message = language === 'en' ? (catalog[source] ?? source) : source;
  return message.replace(/\{(\w+)\}/g, (match, key) => Object.hasOwn(values, key) ? (key === 'error' ? errorMessage(values[key]) : String(values[key])) : match);
}

const escapePattern = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const errorPatterns = errorSources.map(source => {
  const english = catalog[source];
  const pattern = (text) => new RegExp('^' + text.split('{detail}').map(escapePattern).join('([\\s\\S]*)') + '$');
  return { source, english, trPattern: pattern(source), enPattern: pattern(english) };
});

/** Localize only known application errors; OS details, paths and names stay intact. */
export function errorMessage(error) {
  const text = String(error?.message ?? error);
  for (const { source, english, trPattern, enPattern } of errorPatterns) {
    const match = trPattern.exec(text) || enPattern.exec(text);
    if (match) return (language === 'tr' ? source : english).replace('{detail}', () => match[1] ?? '');
  }
  return text;
}

/** Only elements explicitly marked as application UI are touched. */
export function applyTranslations(root = document) {
  for (const node of root.querySelectorAll('[data-i18n]')) node.textContent = t(node.dataset.i18n);
  for (const attr of ['title', 'placeholder', 'aria-label']) {
    for (const node of root.querySelectorAll(`[data-i18n-${attr}]`)) node.setAttribute(attr, t(node.getAttribute(`data-i18n-${attr}`)));
  }
}
