import { t } from './i18n.js';
// Virtualized grid/list of files with thumbnails, selection and keyboard navigation.
import { thumbUrl } from './api.js';
import { bigIcon, kindIcon } from './icons.js';
import { esc, fmtDate, fmtSize, clamp, throttleFrame } from './util.js';
import { typeLabel } from './formats.js';
import { requestClientThumb, cancelClientThumb } from './clientthumbs.js';

const failed = new Set();
const PAD = 14;
const GAP = 8;
const LIST_ROW = 30;

export class FileView {
  constructor(root, handlers) {
    this.root = root;
    this.h = handlers; // { open, preview, context, selection }
    this.sizer = root.querySelector('.sizer');
    this.items = [];
    this.nodes = new Map();
    this.sel = new Set();
    this.focus = -1;
    this.anchor = -1;
    this.view = 'grid';
    this.thumb = 168;
    this.showLocation = false;
    this.typeBuf = '';
    this.typeTimer = 0;
    this.metrics = { cols: 1, colW: 100, rowH: 100, gapY: GAP };
    this.scheduleRender = throttleFrame(() => this.render());

    root.addEventListener('scroll', this.scheduleRender, { passive: true });
    new ResizeObserver(() => this.relayout()).observe(root);
    root.addEventListener('mousedown', (e) => this.onMouseDown(e));
    root.addEventListener('dblclick', (e) => {
      const i = this.indexFromEvent(e);
      if (i >= 0) this.h.open(this.items[i]);
    });
    root.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      const i = this.indexFromEvent(e);
      if (i >= 0 && !this.sel.has(this.items[i].path)) this.selectOnly(i);
      if (i < 0) this.clearSelection();
      this.h.context(e, i >= 0 ? this.items[i] : null);
    });
    root.addEventListener(
      'wheel',
      (e) => {
        if (!e.ctrlKey) return;
        e.preventDefault();
        this.h.zoom?.(e.deltaY < 0 ? 1 : -1);
      },
      { passive: false }
    );
  }

  setItems(items, { keepSelection = false, keepScroll = false, focusPath = null } = {}) {
    this.items = items;
    if (!keepSelection) this.sel.clear();
    else {
      const paths = new Set(items.map((i) => i.path));
      for (const p of [...this.sel]) if (!paths.has(p)) this.sel.delete(p);
    }
    const fp = focusPath ?? (keepSelection && this.focus >= 0 ? this.focusedPath : null);
    this.focus = fp ? items.findIndex((i) => i.path === fp) : -1;
    this._fp = this.focus >= 0 ? fp : null;
    if (this.focus >= 0 && focusPath) this.sel.add(fp);
    this.anchor = this.focus;
    this.clearNodes();
    if (!keepScroll) this.root.scrollTop = 0;
    this.relayout();
    if (this.focus >= 0) this.ensureVisible(this.focus);
    this.h.selection?.();
  }

  get focusedPath() {
    return this._fp;
  }

  setView(view) {
    if (view === this.view) return;
    this.view = view;
    this.root.classList.toggle('as-list', view === 'list');
    this.root.classList.toggle('as-grid', view === 'grid');
    this.clearNodes();
    this.relayout();
    if (this.focus >= 0) this.ensureVisible(this.focus);
  }

  setThumbSize(px) {
    this.thumb = px;
    this.root.style.setProperty('--thumb', `${px}px`);
    if (this.view === 'grid') {
      const anchor = this.firstVisibleIndex();
      this.clearNodes();
      this.relayout();
      if (anchor > 0) this.ensureVisible(anchor, true);
    }
  }

  setShowLocation(v) {
    this.showLocation = v;
    this.root.classList.toggle('with-location', v);
  }

  clearNodes() {
    for (const [, n] of this.nodes) this.releaseNode(n);
    this.nodes.clear();
  }

  releaseNode(n) {
    if (n._cancel) n._cancel();
    n.remove();
  }

  relayout() {
    const W = this.root.clientWidth;
    if (this.view === 'grid') {
      const minW = this.thumb + 20;
      const cols = Math.max(1, Math.floor((W - 2 * PAD + GAP) / (minW + GAP)));
      const colW = Math.floor((W - 2 * PAD - (cols - 1) * GAP) / cols);
      const rowH = this.thumb + 20 + (this.showLocation ? 66 : 50);
      this.metrics = { cols, colW, rowH, gapY: GAP };
    } else {
      this.metrics = { cols: 1, colW: W - 12, rowH: LIST_ROW, gapY: 0 };
    }
    const { cols, rowH, gapY } = this.metrics;
    const rows = Math.ceil(this.items.length / cols);
    const pad = this.view === 'grid' ? PAD : 4;
    this.sizer.style.height = `${rows ? pad * 2 + rows * rowH + (rows - 1) * gapY : 0}px`;
    for (const [i, n] of this.nodes) this.place(n, i);
    this.render();
  }

  place(n, i) {
    const { cols, colW, rowH, gapY } = this.metrics;
    const pad = this.view === 'grid' ? PAD : 4;
    const r = Math.floor(i / cols);
    const c = i % cols;
    const x = this.view === 'grid' ? PAD + c * (colW + GAP) : 6;
    const y = pad + r * (rowH + gapY);
    n.style.transform = `translate(${x}px, ${y}px)`;
    n.style.width = `${colW}px`;
    n.style.height = `${rowH}px`;
  }

  firstVisibleIndex() {
    const { cols, rowH, gapY } = this.metrics;
    return Math.max(0, Math.floor(this.root.scrollTop / (rowH + gapY)) * cols);
  }

  render() {
    const { cols, rowH, gapY } = this.metrics;
    const top = this.root.scrollTop;
    const h = this.root.clientHeight;
    const step = rowH + gapY;
    const r0 = Math.max(0, Math.floor(top / step) - 2);
    const r1 = Math.ceil((top + h) / step) + 2;
    const i0 = r0 * cols;
    const i1 = Math.min(this.items.length, (r1 + 1) * cols);
    for (const [i, n] of this.nodes) {
      if (i < i0 || i >= i1) {
        this.releaseNode(n);
        this.nodes.delete(i);
      }
    }
    const frag = document.createDocumentFragment();
    for (let i = i0; i < i1; i++) {
      if (this.nodes.has(i)) continue;
      const n = this.view === 'grid' ? this.makeCard(i) : this.makeRow(i);
      this.place(n, i);
      this.nodes.set(i, n);
      frag.append(n);
    }
    this.root.append(frag);
  }

  makeCard(i) {
    const it = this.items[i];
    const n = document.createElement('div');
    n.className = `card k-${it.kind}${this.sel.has(it.path) ? ' selected' : ''}${i === this.focus ? ' focused' : ''}${it.hidden ? ' hidden-file' : ''}`;
    n.dataset.i = i;
    const badge = it.ext && it.kind !== 'dir' ? `<span class="badge b-${it.kind}">${esc(it.ext.toUpperCase())}</span>` : '';
    n.innerHTML = `<div class="thumb">${bigIcon(it.kind === 'drive' ? 'drive' : it.kind)}${badge}</div><div class="name" title="${esc(it.name)}">${esc(it.name)}</div><div class="meta">${esc(this.metaText(it))}</div>${
      this.showLocation ? `<div class="loc" title="${esc(it.rel || '')}"><bdi>${esc(it.rel || t('(bu klasör)'))}</bdi></div>` : ''
    }`;
    if (it.kind === 'drive' && it.drive.total) {
      const used = 1 - it.drive.free / it.drive.total;
      n.querySelector('.meta').insertAdjacentHTML('beforebegin', `<div class="usage${used > 0.9 ? ' full' : ''}"><i style="width:${(used * 100).toFixed(1)}%"></i></div>`);
    }
    this.attachThumb(n, n.querySelector('.thumb'), it);
    return n;
  }

  makeRow(i) {
    const it = this.items[i];
    const n = document.createElement('div');
    n.className = `row k-${it.kind}${this.sel.has(it.path) ? ' selected' : ''}${i === this.focus ? ' focused' : ''}${it.hidden ? ' hidden-file' : ''}`;
    n.dataset.i = i;
    n.innerHTML = `<div class="c-name"><span class="mini">${kindIcon(it.kind)}</span><span class="label" title="${esc(it.name)}">${esc(it.name)}</span></div>${
      this.showLocation ? `<div class="c-loc" title="${esc(it.rel || '')}"><bdi>${esc(it.rel || t('(bu klasör)'))}</bdi></div>` : ''
    }<div class="c-date">${esc(fmtDate(it.mtime))}</div><div class="c-type">${esc(typeLabel(it))}</div><div class="c-size">${it.kind === 'dir' ? '' : esc(it.kind === 'drive' ? fmtSize(it.drive.free) + t(' boş') : fmtSize(it.size))}</div>`;
    this.attachThumb(n, n.querySelector('.mini'), it);
    return n;
  }

  metaText(it) {
    if (it.kind === 'drive') return it.drive.total ? t('{free} boş / {total}', { free: fmtSize(it.drive.free), total: fmtSize(it.drive.total) }) : it.path;
    if (it.kind === 'dir') return t('Klasör');
    return fmtSize(it.size);
  }

  attachThumb(node, box, it) {
    if (!it.thumb || failed.has(it.path)) return;
    const show = (url) => {
      if (!url) {
        failed.add(it.path);
        return;
      }
      const img = new Image();
      img.decoding = 'async';
      img.draggable = false;
      img.alt = '';
      img.onload = () => {
        box.classList.add('has-img');
        img.classList.add('in');
      };
      img.onerror = () => {
        failed.add(it.path);
        img.remove();
      };
      img.src = url;
      box.prepend(img);
    };
    if (it.thumb === 'rust') show(thumbUrl(it));
    else {
      const cb = (url) => show(url);
      requestClientThumb(it, cb);
      node._cancel = () => cancelClientThumb(it.path, cb);
    }
  }

  indexFromEvent(e) {
    const n = e.target.closest?.('.card, .row');
    return n && this.root.contains(n) ? Number(n.dataset.i) : -1;
  }

  onMouseDown(e) {
    if (e.button !== 0 && e.button !== 2) return;
    const i = this.indexFromEvent(e);
    this.root.focus({ preventScroll: true });
    if (i < 0) {
      if (e.button === 0 && (e.target === this.root || e.target === this.sizer)) this.clearSelection();
      return;
    }
    if (e.button === 2) return;
    if (e.shiftKey && this.anchor >= 0) this.selectRange(this.anchor, i, e.ctrlKey);
    else if (e.ctrlKey || e.metaKey) this.toggle(i);
    else this.selectOnly(i);
    this.h.dragStart?.(e, this.items[i]);
  }

  selected() {
    return this.items.filter((it) => this.sel.has(it.path));
  }

  focused() {
    return this.items[this.focus] ?? this.selected()[0] ?? null;
  }

  setFocus(i) {
    const old = this.nodes.get(this.focus);
    old?.classList.remove('focused');
    this.focus = i;
    this._fp = this.items[i]?.path ?? null;
    this.nodes.get(i)?.classList.add('focused');
  }

  refreshSelection() {
    for (const [i, n] of this.nodes) n.classList.toggle('selected', this.sel.has(this.items[i].path));
    this.h.selection?.();
  }

  selectOnly(i) {
    this.sel.clear();
    if (i >= 0) this.sel.add(this.items[i].path);
    this.anchor = i;
    this.setFocus(i);
    this.refreshSelection();
  }

  toggle(i) {
    const p = this.items[i].path;
    this.sel.has(p) ? this.sel.delete(p) : this.sel.add(p);
    this.anchor = i;
    this.setFocus(i);
    this.refreshSelection();
  }

  selectRange(a, b, additive) {
    if (!additive) this.sel.clear();
    const [lo, hi] = a < b ? [a, b] : [b, a];
    for (let k = lo; k <= hi; k++) this.sel.add(this.items[k].path);
    this.setFocus(b);
    this.refreshSelection();
  }

  selectAll() {
    for (const it of this.items) this.sel.add(it.path);
    this.refreshSelection();
  }

  clearSelection() {
    this.sel.clear();
    this.anchor = -1;
    this.refreshSelection();
  }

  ensureVisible(i, toTop = false) {
    const { cols, rowH, gapY } = this.metrics;
    const pad = this.view === 'grid' ? PAD : 4;
    const y = pad + Math.floor(i / cols) * (rowH + gapY);
    const top = this.root.scrollTop;
    const h = this.root.clientHeight;
    if (toTop) this.root.scrollTop = y - pad;
    else if (y < top) this.root.scrollTop = y - pad;
    else if (y + rowH > top + h) this.root.scrollTop = y + rowH - h + pad;
    this.render();
  }

  moveTo(i, extend) {
    if (!this.items.length) return;
    if (this.focus >= 0 && Math.abs(i - this.focus) === this.metrics.cols && this.metrics.cols > 1) i = this.stepTarget(this.focus, i - this.focus);
    i = clamp(i, 0, this.items.length - 1);
    if (extend && this.anchor >= 0) this.selectRange(this.anchor, i, false);
    else this.selectOnly(i);
    this.ensureVisible(i);
  }

  /** Returns true when the key was handled. */
  handleKey(e) {
    const n = this.items.length;
    if (!n) return false;
    const { cols } = this.metrics;
    const cur = this.focus < 0 ? -1 : this.focus;
    const perPage = Math.max(1, Math.floor(this.root.clientHeight / (this.metrics.rowH + this.metrics.gapY))) * cols;
    const grid = this.view === 'grid';
    switch (e.key) {
      case 'ArrowRight':
        if (!grid) return false;
        this.moveTo(cur + 1, e.shiftKey);
        return true;
      case 'ArrowLeft':
        if (!grid) return false;
        this.moveTo(cur < 0 ? 0 : cur - 1, e.shiftKey);
        return true;
      case 'ArrowDown':
        this.moveTo(cur < 0 ? 0 : cur + cols, e.shiftKey);
        return true;
      case 'ArrowUp':
        this.moveTo(cur < 0 ? 0 : cur - cols, e.shiftKey);
        return true;
      case 'Home':
        this.moveTo(0, e.shiftKey);
        return true;
      case 'End':
        this.moveTo(n - 1, e.shiftKey);
        return true;
      case 'PageDown':
        this.moveTo(cur + perPage, e.shiftKey);
        return true;
      case 'PageUp':
        this.moveTo(cur - perPage, e.shiftKey);
        return true;
    }
    if ((e.key === 'a' || e.key === 'A') && (e.ctrlKey || e.metaKey)) {
      this.selectAll();
      return true;
    }
    if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey && e.key !== ' ') {
      clearTimeout(this.typeTimer);
      this.typeBuf += e.key.toLocaleLowerCase('tr');
      this.typeTimer = setTimeout(() => (this.typeBuf = ''), 900);
      const start = this.typeBuf.length === 1 ? cur + 1 : Math.max(cur, 0);
      for (let k = 0; k < n; k++) {
        const j = (start + k) % n;
        if (this.items[j].name.toLocaleLowerCase('tr').startsWith(this.typeBuf)) {
          this.moveTo(j, false);
          break;
        }
      }
      return true;
    }
    return false;
  }

  /**
   * Target index for moving `delta` items. Row moves (|delta| > 1) that would
   * leave the grid stay put, except that moving down from a row above the last
   * one lands on the last item, like Explorer.
   */
  stepTarget(cur, delta) {
    const n = this.items.length;
    const t = cur + delta;
    if (t >= 0 && t < n) return t;
    if (Math.abs(delta) <= 1) return clamp(t, 0, n - 1);
    const cols = this.metrics.cols;
    if (t >= n && Math.floor(cur / cols) < Math.floor((n - 1) / cols)) return n - 1;
    return cur;
  }

  /** Moves the selection (used by the preview) and returns the new item. */
  step(delta) {
    if (!this.items.length) return null;
    const i = this.stepTarget(this.focus < 0 ? 0 : this.focus, delta);
    this.selectOnly(i);
    this.ensureVisible(i);
    return this.items[i];
  }

  /** Inline rename of item `index`; calls `done(newName | null)` once. */
  beginRename(index, done) {
    const it = this.items[index];
    if (!it) return;
    this.ensureVisible(index);
    const n = this.nodes.get(index);
    const label = n?.querySelector(this.view === 'grid' ? '.name' : '.label');
    if (!label) return;
    const input = document.createElement('input');
    input.className = 'rename-input';
    input.value = it.name;
    input.spellcheck = false;
    label.classList.add('renaming');
    label.replaceChildren(input);
    input.focus();
    const dot = it.kind === 'dir' ? -1 : it.name.lastIndexOf('.');
    input.setSelectionRange(0, dot > 0 ? dot : it.name.length);
    let finished = false;
    const finish = (value) => {
      if (finished) return;
      finished = true;
      label.classList.remove('renaming');
      label.textContent = it.name;
      this.root.focus({ preventScroll: true });
      done(value);
    };
    input.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (e.key === 'Enter') {
        e.preventDefault();
        finish(input.value);
      } else if (e.key === 'Escape') {
        e.preventDefault();
        finish(null);
      }
    });
    input.addEventListener('blur', () => finish(input.value));
    for (const ev of ['mousedown', 'dblclick', 'contextmenu']) input.addEventListener(ev, (e) => e.stopPropagation());
  }
}
