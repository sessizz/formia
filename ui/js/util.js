// Small shared helpers: formatting, text folding, DOM shortcuts.
import { getLocale } from './i18n.js';

export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

export function el(tag, attrs = {}, ...children) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === 'class') n.className = v;
    else if (k === 'html') n.innerHTML = v;
    else if (k === 'text') n.textContent = v;
    else if (k.startsWith('on')) n.addEventListener(k.slice(2), v);
    else if (k === 'dataset') Object.assign(n.dataset, v);
    else n.setAttribute(k, v === true ? '' : v);
  }
  for (const c of children.flat()) {
    if (c == null || c === false) continue;
    n.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return n;
}

const HTML_ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
export const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => HTML_ESC[c]);

const formatters = new Map();
function formats() {
  const locale = getLocale();
  if (!formatters.has(locale)) formatters.set(locale, {
    nf1: new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }),
    nf0: new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }),
    nf2: new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 2 }),
    date: new Intl.DateTimeFormat(locale, { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' }),
    collator: new Intl.Collator(locale, { numeric: true, sensitivity: 'base' }),
  });
  return formatters.get(locale);
}
export const fmtInt = (n) => formats().nf0.format(n);
export const fmtDim = (n) => formats().nf2.format(n);

export function fmtSize(bytes) {
  if (bytes == null) return '';
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? formats().nf0.format(v) : formats().nf1.format(v)} ${units[i]}`;
}

export const fmtDate = (ms) => (ms ? formats().date.format(new Date(ms)) : '');

/** Case/diacritic-insensitive form for searching (handles Turkish I/ı/İ). */
export function fold(s) {
  return String(s)
    .toLocaleLowerCase('en')
    .replace(/ı/g, 'i')
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '');
}

export const naturalCompare = (a, b) => formats().collator.compare(a, b);

export function extOf(name) {
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(i + 1).toLowerCase() : '';
}

export function debounce(fn, ms) {
  let t;
  return (...a) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
}

export function throttleFrame(fn) {
  let queued = false;
  return () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      fn();
    });
  };
}

export function isTypingTarget(t) {
  return t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable);
}

export function clamp(v, lo, hi) {
  return Math.min(hi, Math.max(lo, v));
}
