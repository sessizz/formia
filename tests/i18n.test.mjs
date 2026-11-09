import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { resolveLanguage, setLanguage, getLanguage, t, catalog, errorMessage } from '../ui/js/i18n.js';
import { fmtInt, fmtDim, fmtDate } from '../ui/js/util.js';

test('first run follows the primary system language, with English fallback', () => {
  for (const lang of ['tr', 'tr-TR', 'TR-tr', 'tr_CY']) assert.equal(resolveLanguage(null, [lang]), 'tr');
  for (const lang of ['en-US', 'de-DE', '', null]) assert.equal(resolveLanguage(null, [lang]), 'en');
  assert.equal(resolveLanguage(null, ['de-DE', 'tr-TR']), 'en');
  assert.equal(resolveLanguage(null, []), 'en');
  assert.equal(resolveLanguage(null, null), 'en');
});

test('explicit supported preference wins and invalid saved settings fall back', () => {
  assert.equal(resolveLanguage('en', ['tr-TR']), 'en');
  assert.equal(resolveLanguage('tr', ['en-US']), 'tr');
  assert.equal(resolveLanguage('bad', ['tr-TR']), 'tr');
});

test('switching and interpolation preserve user content', () => {
  setLanguage('en');
  assert.equal(getLanguage(), 'en');
  assert.equal(t('Ayarlar'), 'Settings');
  assert.equal(t('{name} içinde ara', { name: 'Ayarlar {other} <model>' }), 'Search in Ayarlar {other} <model>');
  setLanguage('tr');
  assert.equal(t('Ayarlar'), 'Ayarlar');
  assert.equal(t('{name} içinde ara', { name: 'archive' }), 'archive içinde ara');
});

test('every catalog translation preserves interpolation parameters', () => {
  const parameters = (s) => [...s.matchAll(/\{(\w+)\}/g)].map(m => m[1]).sort();
  for (const [source, english] of Object.entries(catalog)) {
    assert.ok(english.trim(), source);
    assert.deepEqual(parameters(source), parameters(english), source);
  }
});

test('numbers and dates follow the selected language', () => {
  setLanguage('en');
  assert.equal(fmtInt(1234), '1,234');
  assert.equal(fmtDim(12.5), '12.5');
  const enDate = fmtDate(Date.UTC(2026, 10, 9, 12));
  setLanguage('tr');
  assert.equal(fmtInt(1234), '1.234');
  assert.equal(fmtDim(12.5), '12,5');
  assert.notEqual(fmtDate(Date.UTC(2026, 10, 9, 12)), enDate);
});

test('known errors translate while paths, file names and unknown OS details remain intact', () => {
  setLanguage('en');
  assert.equal(errorMessage('Bu klasöre erişim izni yok'), 'You do not have permission to access this folder');
  assert.equal(errorMessage('Yol bulunamadı: C:\\Ayarlar\\{detail}.stl'), 'Path not found: C:\\Ayarlar\\{detail}.stl');
  assert.equal(errorMessage('“Ayarlar” adında bir öğe zaten var'), 'An item named “Ayarlar” already exists');
  assert.equal(errorMessage('OS error 123'), 'OS error 123');
  setLanguage('tr');
  assert.equal(errorMessage('Folder not found'), 'Klasör bulunamadı');
});

test('all explicit UI translation keys have an English entry', async () => {
  const jsRoot = new URL('../ui/js/', import.meta.url);
  const sources = await Promise.all((await readdir(jsRoot)).filter(name => name.endsWith('.js') && name !== 'translations.js').map(name => readFile(new URL(name, jsRoot), 'utf8')));
  sources.push(await readFile(new URL('../ui/index.html', import.meta.url), 'utf8'));
  for (const source of sources) {
    for (const match of source.matchAll(/\bt\(\s*(['"])([^'"\n]+)\1/g)) assert.ok(Object.hasOwn(catalog, match[2]), `Missing translation: ${match[2]}`);
    for (const match of source.matchAll(/data-i18n(?:-title|-placeholder|-aria-label)?="([^"$]+)"/g)) assert.ok(Object.hasOwn(catalog, match[1]), `Missing static translation: ${match[1]}`);
  }
});
