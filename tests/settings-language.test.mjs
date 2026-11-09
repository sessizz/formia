import test from 'node:test';
import assert from 'node:assert/strict';
globalThis.window = { __TAURI__: {} };
const { mergeSettings } = await import('../ui/js/settings.js');

test('legacy settings follow system locale and preserve favorites and viewer preferences', () => {
  const saved = { favorites: [{ name: 'Ayarlar', path: '/models' }], viewer: { grid: false }, theme: 'light' };
  const settings = mergeSettings(saved, ['tr-TR']);
  assert.equal(settings.language, 'tr');
  assert.deepEqual(settings.favorites, saved.favorites);
  assert.equal(settings.viewer.grid, false);
  assert.equal(settings.viewer.wire, false);
  assert.equal(settings.theme, 'light');
  assert.equal(Object.hasOwn(saved, 'language'), false);
});

test('a manual choice survives saving and reopening on a different system language', () => {
  const initial = mergeSettings(null, ['tr-TR']);
  initial.language = 'en';
  const saved = JSON.parse(JSON.stringify(initial));
  assert.equal(mergeSettings(saved, ['tr-TR']).language, 'en');
  assert.equal(mergeSettings({ language: 'tr' }, ['en-US']).language, 'tr');
  assert.equal(mergeSettings({ language: 'unsupported' }, ['tr-TR']).language, 'tr');
});
