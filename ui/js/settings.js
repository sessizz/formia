import { t, setLanguage, resolveLanguage } from './i18n.js';
// Settings dialog.
import { api } from './api.js';
import { icon } from './icons.js';
import { el, fmtSize, fmtInt } from './util.js';
import { toast } from './menu.js';

export const DEFAULTS = {
  view: 'grid',
  thumbSize: 168,
  sortKey: 'name',
  sortDir: 1,
  showHidden: false,
  only3d: false,
  favorites: [],
  theme: 'dark',
  language: null,
  scanArchives: true,
  preferEmbedded: true,
  lastPath: null,
  sidebarWidth: 248,
  modelColor: '#a7b6c8',
  viewer: { wire: false, grid: true, smooth: false, autoRotate: false, colors: true },
};

export function mergeSettings(saved, languages) {
  const s = structuredClone(DEFAULTS);
  if (saved && typeof saved === 'object') {
    for (const k of Object.keys(DEFAULTS)) if (k in saved) s[k] = saved[k];
    s.viewer = { ...DEFAULTS.viewer, ...(saved.viewer || {}) };
    if (!Array.isArray(s.favorites)) s.favorites = [];
  }
  s.language = resolveLanguage(s.language, languages);
  return s;
}

export function openSettings(S, apply) {
  const dlg = el('dialog', { class: 'settings' });
  const row = (label, control, hint) =>
    el('label', { class: 'set-row' }, el('div', { class: 'set-text' }, el('div', { class: 'set-label', text: label }), hint ? el('div', { class: 'set-hint', text: hint }) : null), control);
  const check = (key, onChange) => {
    const c = el('input', { type: 'checkbox', class: 'switch' });
    c.checked = !!S[key];
    c.addEventListener('change', async () => {
      S[key] = c.checked;
      await onChange?.(c.checked);
      apply(key);
    });
    return c;
  };
  const theme = el('select', {}, el('option', { value: 'dark', text: t('Koyu') }), el('option', { value: 'light', text: t('Açık') }), el('option', { value: 'system', text: t('Sistem') }));
  theme.value = S.theme;
  theme.addEventListener('change', () => {
    S.theme = theme.value;
    apply('theme');
  });
  const language = el('select', { id: 'settings-language', 'aria-label': t('Dil') },
    el('option', { value: 'tr', text: 'Türkçe' }), el('option', { value: 'en', text: 'English' }));
  language.value = S.language;
  language.addEventListener('change', () => {
    S.language = language.value;
    setLanguage(S.language);
    apply('language');
    dlg.close();
    const next = openSettings(S, apply);
    next.querySelector('#settings-language').focus();
  });
  const color = el('input', { type: 'color', value: S.modelColor });
  color.addEventListener('input', () => {
    S.modelColor = color.value;
    apply('modelColor');
  });
  const cacheText = el('span', { class: 'set-hint', text: t('hesaplanıyor…') });
  const refreshCache = () =>
    api.cacheInfo().then((c) => (cacheText.textContent = t('{count} dosya · {size}', { count: fmtInt(c.files), size: fmtSize(c.bytes) }))).catch(() => (cacheText.textContent = '—'));
  const clearBtn = el('button', { class: 'btn', html: `${icon('trash')}<span>${t('Temizle')}</span>` });
  clearBtn.addEventListener('click', async (e) => {
    e.preventDefault();
    await api.cacheClear();
    toast(t('Küçük resim önbelleği temizlendi'));
    refreshCache();
    apply('cache');
  });

  dlg.append(
    el('header', { class: 'set-head' }, el('h2', { text: t('Ayarlar') }), el('button', { class: 'icon-btn', html: icon('close'), onclick: () => dlg.close() })),
    el(
      'div',
      { class: 'set-body' },
      row(t('Dil'), language),
      row(t('Tema'), theme),
      row(t('Gizli dosyaları göster'), check('showHidden'), t('Nokta ile başlayan ve gizli/sistem dosyaları')),
      row(t('Taramada zip arşivlerinin içine bak'), check('scanArchives'), t('“Alt klasörleri tara” zip içindeki modelleri de bulur')),
      row(t('3MF içindeki slicer görselini kullan'), check('preferEmbedded', (v) => api.setPreferEmbedded(v)), t('Kapalıysa 3MF küçük resmi modelden çizilir')),
      row(t('Önizleme model rengi'), color),
      row(t('Küçük resim önbelleği'), el('div', { class: 'set-inline' }, cacheText, clearBtn))
    ),
    el('footer', { class: 'set-foot' }, el('span', { text: t('Formia 1.0 ·Space: önizleme · Ctrl+Tekerlek: boyut · Backspace: üst klasör') }))
  );
  dlg.addEventListener('close', () => dlg.remove());
  dlg.addEventListener('mousedown', (e) => {
    if (e.target === dlg) dlg.close();
  });
  document.body.append(dlg);
  dlg.showModal();
  refreshCache();
  return dlg;
}
