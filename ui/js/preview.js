import { t, applyTranslations } from './i18n.js';
// Quick Look style preview: Space opens, arrows step through items, Esc/Space closes.
import { api, rawUrl, thumbUrl } from './api.js';
import { icon, bigIcon } from './icons.js';
import { el, esc, fmtDate, fmtDim, fmtInt, fmtSize } from './util.js';
import { BROWSER_IMAGE, typeLabel } from './formats.js';
import { Viewer3D } from './viewer3d.js';
import { toast } from './menu.js';

const TEXT_LIMIT = 256 * 1024;
const GCODE_TOOLPATH_LIMIT = 80 * 1024 * 1024;

// Quick picks in the colour menu: common PLA filament colours (approximate).
const FILAMENT_PRESETS = [
  ['#ffffff', 'Beyaz'], ['#161616', 'Siyah'], ['#8e9089', 'Gri'], ['#a6a9aa', 'Gümüş'],
  ['#c12e1f', 'Kırmızı'], ['#ff6a13', 'Turuncu'], ['#f4ee2a', 'Sarı'], ['#e4bd68', 'Altın'],
  ['#00ae42', 'Yeşil'], ['#0086d6', 'Camgöbeği'], ['#0a2989', 'Mavi'], ['#5e43b7', 'Mor'],
  ['#f55a74', 'Pembe'], ['#9d432c', 'Kahverengi'], ['#f7e6de', 'Bej'],
];

export class Preview {
  constructor({ step, rowSize, getSettings, saveViewer, onDelete }) {
    this.step = step;
    this.rowSize = rowSize;
    this.getSettings = getSettings;
    this.saveViewer = saveViewer;
    this.onDelete = onDelete;
    this.item = null;
    this.token = 0;
    this.abort = null;
    this.viewer = null;
    this.build();
  }

  get isOpen() {
    return !this.root.hidden;
  }

  build() {
    const tool = (id, ic, title) => el('button', { class: 'pv-tool', 'data-tool': id, title: t(title), 'data-i18n-title': title, html: icon(ic) });
    this.root = el('div', { id: 'preview', class: 'preview', hidden: true });
    this.root.innerHTML = `
      <div class="pv-backdrop"></div>
      <div class="pv-panel" role="dialog" aria-modal="true">
        <header class="pv-head">
          <span class="pv-icon"></span>
          <div class="pv-title"><div class="pv-name"></div><div class="pv-sub"></div></div>
          <div class="pv-actions">
            <button class="btn pv-open" title="${t("Varsayılan uygulamada aç (Enter)")}" data-i18n-title="Varsayılan uygulamada aç (Enter)">${icon('external')}<span data-i18n="Aç">${t("Aç")}</span></button>
            <button class="icon-btn pv-reveal" title="${t("Klasörde göster")}" data-i18n-title="Klasörde göster">${icon('reveal')}</button>
            <button class="icon-btn pv-delete" title="${t("Sil (Delete)")}" data-i18n-title="Sil (Delete)">${icon('trash')}</button>
            <button class="icon-btn pv-close" title="${t("Kapat (Esc)")}" data-i18n-title="Kapat (Esc)">${icon('close')}</button>
          </div>
        </header>
        <div class="pv-body">
          <nav class="pv-plates" aria-label="${t("Plakalar")}" data-i18n-aria-label="Plakalar" hidden></nav>
          <div class="pv-stage"></div>
          <div class="pv-3d" hidden></div>
          <div class="pv-loading" hidden><div class="spinner"></div><span data-i18n="Yükleniyor…">${t("Yükleniyor…")}</span></div>
          <div class="pv-error" hidden></div>
          <button class="pv-nav prev" title="${t("Önceki (←)")}" data-i18n-title="Önceki (←)">${icon('chevronLeft')}</button>
          <button class="pv-nav next" title="${t("Sonraki (→)")}" data-i18n-title="Sonraki (→)">${icon('chevronRight')}</button>
          <div class="pv-tools" hidden></div>
          <img class="pv-inset" hidden alt="">
        </div>
        <footer class="pv-foot"></footer>
        <div class="pv-cpop" role="dialog" aria-label="${t("Renkler")}" data-i18n-aria-label="Renkler" hidden></div>
      </div>`;
    const tools = this.root.querySelector('.pv-tools');
    tools.append(
      tool('reset', 'focus', 'Görünümü sıfırla (R)'),
      tool('colors', 'palette', 'Filament renkleri (C)'),
      tool('wire', 'wire', 'Tel kafes (W)'),
      tool('smooth', 'sphere', 'Yumuşak gölgeleme (S)'),
      tool('grid', 'floor', 'Zemin ızgarası (G)'),
      tool('autoRotate', 'rotate', 'Otomatik döndür (A)'),
      el('button', { class: 'pv-tool pv-axis', 'data-tool': 'axis', 'data-i18n-title': 'Yukarı ekseni değiştir (U)', title: t('Yukarı ekseni değiştir (U)'), text: 'Z↑' })
    );
    document.body.append(this.root);
    const q = (s) => this.root.querySelector(s);
    this.$ = { panel: q('.pv-panel'), icon: q('.pv-icon'), name: q('.pv-name'), sub: q('.pv-sub'), stage: q('.pv-stage'), three: q('.pv-3d'), loading: q('.pv-loading'), error: q('.pv-error'), tools, foot: q('.pv-foot'), inset: q('.pv-inset'), axis: q('.pv-axis'), cpop: q('.pv-cpop'), body: q('.pv-body'), plates: q('.pv-plates') };
    this.colorRow = -1;
    this.plates = [];
    this.plate = null;
    this.$.plates.addEventListener('click', (e) => {
      const b = e.target.closest('[data-plate]');
      if (b) this.selectPlate(Number(b.dataset.plate));
    });
    this.$.foot.addEventListener('click', (e) => {
      if (e.target.closest('.pv-colors')) this.$.cpop.hidden ? this.openColors() : this.closeColors();
    });
    this.$.cpop.addEventListener('click', (e) => this.colorClick(e));
    this.$.cpop.addEventListener('input', (e) => e.target.type === 'color' && this.pickColor(e.target.value, false));
    this.$.cpop.addEventListener('change', (e) => e.target.type === 'color' && this.renderColors());
    document.addEventListener(
      'mousedown',
      (e) => {
        if (!this.$.cpop.hidden && !this.$.cpop.contains(e.target) && !e.target.closest('.pv-colors')) this.closeColors();
      },
      true
    );
    q('.pv-backdrop').addEventListener('mousedown', () => this.close());
    q('.pv-close').addEventListener('click', () => this.close());
    q('.pv-open').addEventListener('click', () => this.openExternal());
    q('.pv-reveal').addEventListener('click', () => this.item && api.reveal(this.item.path).catch((e) => toast(String(e), 'error')));
    q('.pv-delete').addEventListener('click', () => this.item && this.onDelete?.(this.item));
    q('.pv-nav.prev').addEventListener('click', () => this.go(-1));
    q('.pv-nav.next').addEventListener('click', () => this.go(1));
    tools.addEventListener('click', (e) => {
      const b = e.target.closest('.pv-tool');
      if (b) this.tool(b.dataset.tool);
    });
  }

  retranslate() {
    applyTranslations(this.root);
    if (!this.item) return;
    this.$.sub.textContent = this.item.rel ? `${typeLabel(this.item)} · ${this.item.rel}` : typeLabel(this.item);
    const hint = this.$.stage.querySelector('.pv-info-hint');
    if (hint) hint.textContent = t('Açmak için Enter veya çift tıklayın');
    const type = this.$.stage.querySelector('.pv-info-type');
    if (type) type.textContent = typeLabel(this.item);
    this.renderPlates();
    this.renderFoot(this.item, this.footInfo);
    if (!this.$.cpop.hidden) this.renderColors();
    if (!this.$.loading.hidden) this.setBusy(true, ...(this.busyState || []));
    if (this.errorState) this.setError(...this.errorState);
    const pre = this.$.stage.querySelector('.pv-text');
    if (pre && this.textPreview) pre.textContent = this.textPreview.text + (this.textPreview.truncated ? '\n\n' + t('… (ilk 256 KB gösteriliyor)') : '');
  }

  ensureViewer() {
    if (!this.viewer) {
      const s = this.getSettings();
      this.viewer = new Viewer3D(this.$.three, { color: s.modelColor });
      this.viewer.setTheme(document.documentElement.dataset.theme !== 'light');
      for (const k of ['wire', 'grid', 'smooth', 'autoRotate', 'colors']) this.viewer.set(k, !!s.viewer[k]);
    }
    return this.viewer;
  }

  applyTheme(dark) {
    this.viewer?.setTheme(dark);
  }

  applyColor(hex) {
    this.viewer?.setColor(hex);
    this.refreshColors();
  }

  /* ---------- build plates (multi-plate 3MF) ---------- */

  setPlates(plates) {
    this.plates = Array.isArray(plates) && plates.length > 1 ? plates : [];
    this.plate = this.plates.length ? (this.plates.find((p) => p.objects > 0) || this.plates[0]).id : null;
    this.$.plates.hidden = !this.plates.length;
    this.$.body.classList.toggle('has-plates', !!this.plates.length);
    this.renderPlates();
    this.viewer?.resize();
  }

  renderPlates() {
    if (!this.plates.length) return void (this.$.plates.innerHTML = '');
    const items = this.plates
      .map((p) => {
        const label = p.name || t('Plaka {id}', { id: p.id });
        const pic = p.thumb ? `<img src="${p.thumb}" alt="" draggable="false">` : `<span class="pl-none">${icon('cube')}</span>`;
        const cls = `pl-item${p.id === this.plate ? ' on' : ''}${p.objects ? '' : ' pl-blank'}`;
        const title = p.objects ? t('{name} · {count} nesne', { name: label, count: fmtInt(p.objects) }) : t('{name} · boş', { name: label });
        return `<button class="${cls}" data-plate="${p.id}" title="${esc(title)}"><span class="pl-pic">${pic}<span class="pl-num">${p.id}</span></span><span class="pl-name">${esc(label)}</span></button>`;
      })
      .join('');
    this.$.plates.innerHTML = `<div class="pl-head">${t('Plakalar')} <span>${this.plates.length}</span></div>${items}`;
    this.$.plates.querySelector('.pl-item.on')?.scrollIntoView({ block: 'nearest' });
  }

  /** Shows another plate of the open project; colour picks carry over. */
  async selectPlate(id) {
    if (!this.item || !this.viewer || id === this.plate) return;
    this.plate = id;
    this.renderPlates();
    const token = ++this.token;
    this.abort?.abort();
    this.abort = new AbortController();
    this.setError(null);
    this.setBusy(true, 'Plaka yükleniyor…');
    try {
      const info = await this.viewer.load(this.item, this.abort.signal, { plate: id, toolpath: this.toolpath });
      if (token !== this.token) return;
      this.setBusy(false);
      this.$.tools.querySelector('[data-tool=colors]').hidden = !info?.palette?.length;
      this.syncTools();
      this.renderFoot(this.item, info);
      if (!this.$.cpop.hidden) this.renderColors();
    } catch (e) {
      if (token !== this.token || e?.name === 'AbortError') return;
      this.setBusy(false);
      this.viewer.clear();
      this.closeColors();
      this.renderFoot(this.item, null);
      this.setError('Plaka açılamadı: {error}', { error: e?.message || e });
    }
  }

  /* ---------- colour menu (view only: the file and thumbnails are untouched) ---------- */

  colorChip(slots) {
    const dots = slots
      .slice(0, 8)
      .map((s) => `<i style="background:${s.current || s.original}"></i>`)
      .join('');
    const label = slots.length > 1 ? t('{count} renk', { count: fmtInt(slots.length) }) : t('Renk');
    const open = this.$.cpop.hidden ? '' : ' open';
    const changed = slots.some((s) => s.current) ? ' changed' : '';
    return `<button class="chip-info swatches pv-colors${open}${changed}" title="${t("Renkleri değiştir (yalnızca önizleme)")}" data-i18n-title="Renkleri değiştir (yalnızca önizleme)">${dots}<span>${label}</span>${icon('chevronDown')}</button>`;
  }

  /** Re-renders the footer chip and, when open, the menu after the model's colours changed. */
  refreshColors() {
    const b = this.$.foot.querySelector('.pv-colors');
    if (b && this.viewer) b.outerHTML = this.colorChip(this.viewer.colorSlots());
    if (!this.$.cpop.hidden) this.renderColors();
  }

  openColors() {
    if (!this.viewer || this.$.three.hidden) return;
    // Filament colours switched off with C: bring them back, they are what the menu edits.
    if (this.viewer.info?.palette?.length && !this.viewer.opts.colors) this.tool('colors');
    const slots = this.viewer.colorSlots();
    if (!slots.length) return;
    this.colorRow = slots.length === 1 ? 0 : -1;
    this.$.cpop.hidden = false;
    this.renderColors();
    this.$.foot.querySelector('.pv-colors')?.classList.add('open');
  }

  closeColors() {
    if (this.$.cpop.hidden) return;
    this.$.cpop.hidden = true;
    this.colorRow = -1;
    this.$.foot.querySelector('.pv-colors')?.classList.remove('open');
  }

  renderColors() {
    const slots = (this.slots = this.viewer?.colorSlots() || []);
    if (!slots.length) return this.closeColors();
    if (this.colorRow >= slots.length) this.colorRow = -1;
    const rows = slots
      .map((s, i) => {
        const cur = s.current;
        const row = `<div class="cp-row${i === this.colorRow ? ' on' : ''}">
          <span class="cp-sw"><i style="background:${s.original}"></i>${s.original.toUpperCase()}</span>
          <span class="cp-arrow">→</span>
          <button class="cp-sw cp-new${cur ? ' set' : ''}" data-act="row" data-i="${i}" title="${t("Yeni renk seç")}" data-i18n-title="Yeni renk seç"><i style="background:${cur || s.original}"></i>${cur ? cur.toUpperCase() : t('aynı')}</button>
          <button class="icon-btn cp-reset" data-act="reset" data-i="${i}" title="${t("Orijinale dön")}" data-i18n-title="Orijinale dön"${cur ? '' : ' disabled'}>${icon('refresh')}</button>
        </div>`;
        if (i !== this.colorRow) return row;
        const sel = (cur || '').toLowerCase();
        const dots = FILAMENT_PRESETS.map(([hex, name]) => `<button class="cp-dot${sel === hex ? ' sel' : ''}" data-act="preset" data-hex="${hex}" title="${t(name)}" style="background:${hex}"></button>`).join('');
        return `${row}<div class="cp-grid">${dots}<label class="cp-dot cp-custom" title="${t("Özel renk…")}" data-i18n-title="Özel renk…"><input type="color" value="${cur || s.original}"></label></div>`;
      })
      .join('');
    const any = slots.some((s) => s.current);
    this.$.cpop.innerHTML = `
      <div class="cp-head"><b>${slots.length > 1 ? t('Filament renkleri') : t('Model rengi')}</b><small data-i18n="yalnızca önizleme">${t("yalnızca önizleme")}</small></div>
      <div class="cp-cols"><span data-i18n="Orijinal">${t("Orijinal")}</span><span></span><span data-i18n="Yeni">${t("Yeni")}</span><span></span></div>
      <div class="cp-list">${rows}</div>
      <div class="cp-foot"><span data-i18n="Esc ile kapanır">${t("Esc ile kapanır")}</span><button class="btn cp-all" data-act="all"${any ? '' : ' disabled'} data-i18n="Hepsini sıfırla">${t("Hepsini sıfırla")}</button></div>`;
    this.positionColors();
  }

  positionColors() {
    const b = this.$.foot.querySelector('.pv-colors');
    if (!b) return this.closeColors();
    const pop = this.$.cpop;
    const panel = this.$.panel.getBoundingClientRect();
    const r = b.getBoundingClientRect();
    const left = Math.max(8, Math.min(r.left - panel.left, panel.width - pop.offsetWidth - 8));
    pop.style.left = `${left}px`;
    pop.style.bottom = `${panel.bottom - r.top + 8}px`;
    pop.style.setProperty('--arrow-x', `${r.left - panel.left - left + Math.min(r.width / 2, 40)}px`);
  }

  colorClick(e) {
    const b = e.target.closest('[data-act]');
    if (!b || !this.viewer) return;
    const i = Number(b.dataset.i);
    switch (b.dataset.act) {
      case 'row':
        this.colorRow = this.colorRow === i ? -1 : i;
        return this.renderColors();
      case 'reset':
        this.viewer.setSlotColor(this.slots[i], null);
        return this.refreshColors();
      case 'preset':
        return this.pickColor(b.dataset.hex, true);
      case 'all':
        for (const s of this.slots) this.viewer.setSlotColor(s, null);
        return this.refreshColors();
    }
  }

  /** Applies a colour to the active row. `rerender` is false while the native picker is open. */
  pickColor(hex, rerender) {
    const slot = this.slots?.[this.colorRow];
    if (!slot || !this.viewer) return;
    this.viewer.setSlotColor(slot, hex);
    if (rerender) return this.refreshColors();
    // Live update without rebuilding the menu, which would close the system colour picker.
    const b = this.$.foot.querySelector('.pv-colors');
    if (b) b.outerHTML = this.colorChip(this.viewer.colorSlots());
    const row = this.$.cpop.querySelectorAll('.cp-row')[this.colorRow];
    const nw = row?.querySelector('.cp-new');
    if (nw) {
      nw.classList.add('set');
      nw.innerHTML = `<i style="background:${hex}"></i>${hex.toUpperCase()}`;
    }
    row?.querySelector('.cp-reset')?.removeAttribute('disabled');
    this.$.cpop.querySelector('.cp-all')?.removeAttribute('disabled');
    for (const d of this.$.cpop.querySelectorAll('.cp-dot.sel')) d.classList.remove('sel');
  }

  open(item) {
    if (!item) return;
    this.root.hidden = false;
    document.body.classList.add('pv-open');
    requestAnimationFrame(() => this.root.classList.add('in'));
    this.show(item);
  }

  close() {
    if (!this.isOpen) return;
    this.closeColors();
    this.token++;
    this.abort?.abort();
    this.root.classList.remove('in');
    this.root.hidden = true;
    document.body.classList.remove('pv-open');
    this.viewer?.stop();
    this.viewer?.clear();
    this.$.stage.replaceChildren();
    this.item = null;
  }

  go(delta) {
    const next = this.step(delta);
    if (next && next !== this.item) this.show(next);
  }

  openExternal() {
    if (this.item) api.open(this.item.path).catch((e) => toast(t('Açılamadı: {error}', { error: e }), 'error'));
  }

  syncTools() {
    const s = this.getSettings().viewer;
    for (const b of this.$.tools.querySelectorAll('.pv-tool')) {
      const t = b.dataset.tool;
      if (t in s) b.classList.toggle('on', !!s[t]);
    }
    if (this.viewer) this.$.axis.textContent = this.viewer.zUp ? 'Z↑' : 'Y↑';
  }

  tool(t) {
    if (!this.viewer || this.$.three.hidden) return;
    const s = this.getSettings().viewer;
    if (t === 'reset') this.viewer.fit();
    else if (t === 'axis') {
      const info = this.viewer.toggleUpAxis();
      this.renderFoot(this.item, info);
    } else {
      s[t] = !s[t];
      this.viewer.set(t, s[t]);
      this.saveViewer();
      if (t === 'colors') this.refreshColors();
    }
    this.syncTools();
  }

  /** Keyboard handling while open. Returns true when handled. */
  handleKey(e) {
    const k = e.key;
    if (k === 'Escape' && !this.$.cpop.hidden) return this.closeColors(), true;
    if (k === 'Escape' || k === ' ') return this.close(), true;
    // Same movement as in the file grid: left/right = previous/next, up/down = row above/below.
    if (k === 'ArrowLeft') return this.go(-1), true;
    if (k === 'ArrowRight') return this.go(1), true;
    if (k === 'ArrowUp') return this.go(-this.rowSize()), true;
    if (k === 'ArrowDown') return this.go(this.rowSize()), true;
    if (k === 'Enter') return this.openExternal(), true;
    // Cmd+Backspace is the Finder delete shortcut on macOS (same as the file grid in main.js).
    if (k === 'Delete' || (e.metaKey && k === 'Backspace')) return this.item && this.onDelete?.(this.item), true;
    const map = { r: 'reset', w: 'wire', s: 'smooth', g: 'grid', a: 'autoRotate', u: 'axis', c: 'colors' };
    const t = map[k.toLowerCase()];
    if (t && !e.ctrlKey && !e.altKey) return this.tool(t), true;
    return false;
  }

  setBusy(on, source = 'Yükleniyor…', values = {}) {
    this.busyState = on ? [source, values] : null;
    this.$.loading.hidden = !on;
    this.$.loading.querySelector('span').textContent = t(source, values);
  }

  setError(msg, values = {}) {
    this.errorState = msg ? [msg, values] : null;
    this.$.error.hidden = !msg;
    this.$.error.innerHTML = msg ? `${icon('info')}<span>${esc(t(msg, values))}</span>` : '';
  }

  async show(item) {
    const token = ++this.token;
    this.closeColors();
    this.viewer?.resetColors(); // colour picks belong to one file (shared by its plates)
    this.setPlates([]);
    this.abort?.abort();
    this.abort = new AbortController();
    const signal = this.abort.signal;
    this.item = item;
    this.textPreview = null;
    this.$.icon.innerHTML = bigIcon(item.kind);
    this.$.name.textContent = item.name;
    this.$.sub.textContent = item.rel ? `${typeLabel(item)} · ${item.rel}` : typeLabel(item);
    this.$.stage.replaceChildren();
    this.$.three.hidden = true;
    this.$.tools.hidden = true;
    this.$.tools.querySelector('[data-tool=colors]').hidden = true;
    this.$.inset.hidden = true;
    this.setError(null);
    this.setBusy(false);
    this.renderFoot(item, null);
    this.viewer?.stop();
    this.viewer?.clear();

    const kind = item.kind;
    try {
      if (kind === 'model' || (kind === 'gcode' && item.size <= GCODE_TOOLPATH_LIMIT)) {
        const v = this.ensureViewer();
        this.$.three.hidden = false;
        this.$.tools.hidden = false;
        v.resize();
        v.start();
        const cad = ['step', 'stp', 'iges', 'igs', 'brep'].includes(item.ext);
        this.setBusy(true, cad ? 'CAD dosyası üçgenlere dönüştürülüyor…' : item.size > 50e6 ? 'Büyük dosya yükleniyor ({size})…' : 'Yükleniyor…', { size: fmtSize(item.size) });
        if (kind === 'gcode') this.showInset(item);
        // Multi-plate Bambu/Orca project: list the plates and show one at a time.
        const plates = item.ext === '3mf' ? await api.plates(item.path).catch(() => []) : [];
        if (token !== this.token) return;
        // Sliced .gcode.3mf: plates come from G-code and are drawn as a finished print.
        this.toolpath = Array.isArray(plates) && plates.some((p) => p.toolpath);
        this.setPlates(plates);
        const info = await v.load(item, signal, { plate: this.plate || undefined, toolpath: this.toolpath });
        if (token !== this.token) return;
        this.setBusy(false);
        this.$.tools.querySelector('[data-tool=colors]').hidden = !info?.palette?.length;
        this.syncTools();
        this.renderFoot(item, info);
      } else if (kind === 'gcode' || (kind === 'image' && !BROWSER_IMAGE.has(item.ext))) {
        this.$.stage.append(this.imageEl(thumbUrl(item)));
      } else if (kind === 'image') {
        this.$.stage.append(this.imageEl(rawUrl(item.path)));
      } else if (kind === 'video') {
        this.$.stage.append(el('video', { class: 'pv-video', src: rawUrl(item.path), controls: true, autoplay: true }));
      } else if (kind === 'pdf') {
        this.$.stage.append(el('embed', { class: 'pv-pdf', src: rawUrl(item.path), type: 'application/pdf' }));
      } else if (kind === 'text') {
        this.setBusy(true);
        const r = await fetch(rawUrl(item.path), { headers: { Range: `bytes=0-${TEXT_LIMIT - 1}` }, signal });
        const text = await r.text();
        if (token !== this.token) return;
        this.setBusy(false);
        const pre = el('pre', { class: 'pv-text' });
        this.textPreview = { text, truncated: item.size > TEXT_LIMIT };
        pre.textContent = text + (item.size > TEXT_LIMIT ? '\n\n' + t('… (ilk 256 KB gösteriliyor)') : '');
        this.$.stage.append(pre);
      } else {
        this.$.stage.append(this.infoCard(item));
      }
    } catch (e) {
      if (token !== this.token || e?.name === 'AbortError') return;
      this.setBusy(false);
      this.$.three.hidden = true;
      this.$.tools.hidden = true;
      this.viewer?.stop();
      this.$.stage.append(this.infoCard(item));
      this.setError('Önizleme açılamadı: {error}', { error: e?.message || e });
    }
  }

  showInset(item) {
    const img = this.$.inset;
    img.onload = () => (img.hidden = false);
    img.onerror = () => (img.hidden = true);
    img.src = thumbUrl(item);
  }

  imageEl(src) {
    const img = el('img', { class: 'pv-image', alt: '', draggable: 'false' });
    img.onerror = () => {
      img.remove();
      this.$.stage.append(this.infoCard(this.item));
      this.setError('Görsel açılamadı');
    };
    img.src = src;
    return img;
  }

  infoCard(item) {
    return el(
      'div',
      { class: 'pv-info' },
      el('div', { class: 'pv-info-icon', html: bigIcon(item.kind) }),
      el('div', { class: 'pv-info-name', text: item.name }),
      el('div', { class: 'pv-info-type', text: typeLabel(item) }),
      item.kind === 'dir' || item.kind === 'archive' || item.kind === 'drive' ? el('div', { class: 'pv-info-hint', text: t('Açmak için Enter veya çift tıklayın') }) : null
    );
  }

  renderFoot(item, info) {
    this.footInfo = info;
    const chips = [];
    if (info) {
      const u = info.unit ? ` ${info.unit}` : '';
      chips.push(`<span class="chip-info" title="${t("Genişlik × Derinlik × Yükseklik")}" data-i18n-title="Genişlik × Derinlik × Yükseklik">${icon('ruler')}${fmtDim(info.x)} × ${fmtDim(info.y)} × ${fmtDim(info.z)}${u}</span>`);
      if (info.triangles) chips.push(`<span class="chip-info">${icon('cube')}${t('{count} üçgen', { count: fmtInt(info.triangles) })}</span>`);
      const slots = this.viewer?.colorSlots() || [];
      if (slots.length) chips.push(this.colorChip(slots));
    }
    if (item.kind !== 'dir' && item.kind !== 'drive') chips.push(`<span class="chip-info">${fmtSize(item.size)}</span>`);
    if (item.mtime) chips.push(`<span class="chip-info">${fmtDate(item.mtime)}</span>`);
    chips.push(`<span class="pv-path" title="${esc(item.path)}"><bdi>${esc(item.path)}</bdi></span>`);
    this.$.foot.innerHTML = chips.join('');
  }
}
