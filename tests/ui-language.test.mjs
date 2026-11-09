// Browser plugin not available: use isolated Playwright with a mocked Tauri bridge.
// Optional: FORMIA_PLAYWRIGHT points to playwright-core's index.mjs; FORMIA_BROWSER to Chromium.
// No real app settings, file operations or running windows are touched.
import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';
import os from 'node:os';

const enabled = !!process.env.FORMIA_PLAYWRIGHT;
test('full UI: first-run locale, live switching, persistence, previews and scan state', { skip: !enabled }, async () => {
  const { chromium } = await import(pathToFileURL(process.env.FORMIA_PLAYWRIGHT).href);
  const ui = path.resolve(fileURLToPath(new URL('../ui/', import.meta.url)));
  const server = http.createServer(async (req, res) => {
    const name = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
    const target = path.resolve(ui, '.' + (name === '/' ? '/index.html' : name));
    if (!target.startsWith(ui + path.sep) && target !== path.join(ui, 'index.html')) { res.writeHead(403).end(); return; }
    try {
      const mime = { '.js': 'text/javascript', '.html': 'text/html', '.css': 'text/css', '.png': 'image/png' }[path.extname(target)] || 'application/octet-stream';
      const bytes = await readFile(target);
      res.writeHead(200, { 'Content-Type': mime }); res.end(bytes);
    } catch { res.writeHead(404).end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let browser;
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.FORMIA_BROWSER });
    for (const [locale, initialLanguage] of [['tr-TR', 'tr'], ['en-US', 'en'], ['de-DE', 'en']]) {
      const context = await browser.newContext({ locale, viewport: { width: 1280, height: 900 } });
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', error => errors.push(String(error)));
      await page.addInitScript(() => {
        window.testCalls = [];
        const file = { name: 'Ayarlar.txt', path: '/models/Ayarlar.txt', size: 1234, mtime: 1700000000000, isDir: false, archive: false };
        const folder = { ...file, name: 'Klasör', path: '/models/Klasör', isDir: true };
        window.__TAURI__ = {
          core: {
            Channel: class { constructor() { window.scanChannel = this; } },
            convertFileSrc: () => 'data:text/plain,Test%20Ayarlar',
            invoke: async (cmd, args) => {
              window.testCalls.push(cmd);
              if (cmd === 'get_settings') return JSON.parse(localStorage.getItem('test-settings') || 'null');
              if (cmd === 'save_settings') { localStorage.setItem('test-settings', JSON.stringify(args.value)); return; }
              if (cmd === 'get_places') return { os: 'windows', sep: '/', quick: [{ id: 'home', name: 'Ana klasör', path: '/models' }], drives: [] };
              if (cmd === 'list_dir') return { path: args.path, parent: null, inArchive: false, entries: [file, folder] };
              if (cmd === 'cache_info') return { files: 1234, bytes: 12345 };
              if (cmd === 'scan_start') {
                window.scanChannel.onmessage({ type: 'batch', entries: [file], dirs: 2, files: 3 });
                return 1;
              }
              return null;
            },
          },
          window: { getCurrentWindow: () => ({ setTitle: async () => {}, isMaximized: async () => false, onResized: async () => {}, minimize: async () => {}, toggleMaximize: async () => {}, close: async () => {} }) },
        };
      });
      const url = `http://127.0.0.1:${server.address().port}/`;
      await page.goto(url);
      await page.waitForFunction(() => document.querySelector('#status-left').textContent.length > 0, null, { timeout: 5000 }).catch(async error => {
        throw new Error(`${error}\nPage errors: ${errors.join('\n')}\n${await page.locator('body').innerText()}`);
      });
      assert.equal(await page.title(), 'Formia');
      assert.equal(await page.locator('html').getAttribute('lang'), initialLanguage);
      assert.equal(await page.locator('#btn-settings').getAttribute('title'), initialLanguage === 'tr' ? 'Ayarlar' : 'Settings');
      assert.equal(await page.locator('.brand-sub').textContent(), initialLanguage === 'tr' ? '3D dosya gezgini' : '3D file browser');
      assert.ok(await page.locator('.name', { hasText: 'Ayarlar.txt' }).count());
      await page.locator('#btn-settings').click();
      const target = initialLanguage === 'tr' ? 'en' : 'tr';
      await page.locator('#settings-language').selectOption(target);
      await page.waitForFunction(lang => document.documentElement.lang === lang, target);
      assert.equal(await page.locator('dialog[open] h2').textContent(), target === 'tr' ? 'Ayarlar' : 'Settings');
      assert.equal(await page.locator('#btn-settings').getAttribute('title'), target === 'tr' ? 'Ayarlar' : 'Settings');
      await page.screenshot({ path: path.join(os.tmpdir(), `formia-settings-${target}.png`) });
      await page.locator('dialog[open]').evaluate(dialog => dialog.close());
      await page.waitForFunction(lang => JSON.parse(localStorage.getItem('test-settings')).language === lang, target);
      await page.reload();
      await page.waitForFunction(() => document.querySelector('#status-left').textContent.length > 0);
      assert.equal(await page.locator('html').getAttribute('lang'), target);
      // An open preview must keep its file and content when the language changes.
      await page.locator('.name', { hasText: 'Ayarlar.txt' }).click();
      await page.locator('#view').press('Space');
      await page.waitForFunction(() => document.querySelector('.pv-text')?.textContent.includes('Test Ayarlar'));
      // Preview covers the toolbar; dispatch its existing handler to stress-test live preview refresh.
      await page.locator('#btn-settings').evaluate(button => button.click());
      await page.locator('#settings-language').selectOption(initialLanguage);
      await page.locator('dialog[open]').evaluate(dialog => dialog.close());
      assert.equal(await page.locator('.pv-name').textContent(), 'Ayarlar.txt');
      assert.equal(await page.locator('.pv-text').textContent(), 'Test Ayarlar');
      assert.equal(await page.locator('button.pv-open span').textContent(), initialLanguage === 'tr' ? 'Aç' : 'Open');
      assert.equal(await page.locator('[data-tool=reset]').getAttribute('title'), initialLanguage === 'tr' ? 'Görünümü sıfırla (R)' : 'Reset view (R)');
      await page.locator('.pv-close').click();
      await page.locator('#chip-scan').click();
      await page.waitForFunction(() => document.querySelector('#chip-scan').classList.contains('busy'));
      await page.locator('#btn-settings').click();
      await page.locator('#settings-language').selectOption(target);
      await page.locator('dialog[open]').evaluate(dialog => dialog.close());
      assert.equal(await page.locator('#chip-scan span').first().textContent(), target === 'tr' ? 'Taranıyor…' : 'Scanning…');
      assert.equal(await page.evaluate(() => window.testCalls.includes('scan_cancel')), false);
      assert.equal(await page.evaluate(() => window.testCalls.includes('delete_paths')), false);
      // Exercise plate/color metadata and in-flight error/loading labels without a real model or GPU.
      await page.evaluate(async () => {
        const { Preview } = await import('/js/preview.js');
        const { setLanguage } = await import('/js/i18n.js');
        const { mergeSettings } = await import('/js/settings.js');
        const pv = new Preview({ step: () => {}, rowSize: () => 1, getSettings: () => mergeSettings(null), saveViewer: () => {} });
        pv.item = { name: 'Ayarlar', path: '/Ayarlar', kind: 'model', size: 1234, mtime: 1700000000000 };
        pv.viewer = { colorSlots: () => [{ original: '#ffffff', current: null }, { original: '#000000', current: null }], resize: () => {} };
        pv.setPlates([{ id: 1, name: 'Ayarlar', objects: 1 }, { id: 2, name: '', objects: 0 }]);
        pv.renderFoot(pv.item, { x: 12.5, y: 10, z: 10, triangles: 1234, unit: 'mm' });
        pv.colorRow = 0;
        pv.$.cpop.hidden = false;
        pv.setError('Önizleme açılamadı: {error}', { error: 'Yol bulunamadı: /Ayarlar' });
        pv.setBusy(true, 'CAD dosyası üçgenlere dönüştürülüyor…');
        for (const language of ['en', 'tr']) {
          setLanguage(language);
          pv.retranslate();
          const expected = language === 'en' ? ['Plate 2', 'Filament colors', 'Tessellating CAD file…', 'Could not open preview: Path not found: /Ayarlar', 'White'] : ['Plaka 2', 'Filament renkleri', 'CAD dosyası üçgenlere dönüştürülüyor…', 'Önizleme açılamadı: Yol bulunamadı: /Ayarlar', 'Beyaz'];
          const got = [pv.root.querySelectorAll('.pl-name')[1].textContent, pv.root.querySelector('.cp-head b').textContent, pv.$.loading.querySelector('span').textContent, pv.$.error.querySelector('span').textContent, pv.root.querySelector('[data-hex="#ffffff"]').title];
          if (JSON.stringify(got) !== JSON.stringify(expected)) throw new Error(JSON.stringify({ got, expected }));
          if (pv.root.querySelector('.pl-name').textContent !== 'Ayarlar') throw new Error('User plate name changed');
        }
        pv.root.remove();
      });
      await page.screenshot({ path: path.join(os.tmpdir(), `formia-language-${locale}.png`) });
      assert.deepEqual(errors, []);
      await context.close();
    }
  } finally {
    await browser?.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
});
