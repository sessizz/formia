// App shell: navigation, filtering, scan mode, toolbar and keyboard wiring.
import { api, bumpThumbGen } from './api.js';
import { $, fold, naturalCompare, debounce, isTypingTarget, fmtInt, fmtSize, esc } from './util.js';
import { icon, bigIcon } from './icons.js';
import { toItem, SCAN_EXTS } from './formats.js';
import { FileView } from './fileview.js';
import { Preview } from './preview.js';
import { Sidebar, baseName, driveName } from './sidebar.js';
import { showMenu, isMenuOpen, toast, confirmDialog } from './menu.js';
import { openSettings, mergeSettings } from './settings.js';

const THUMB_STEPS = [96, 120, 144, 168, 200, 240, 280, 320];

const S = {
  settings: null,
  places: null,
  path: undefined, // null = "Bu bilgisayar"
  listing: null,
  all: [],
  items: [],
  search: '',
  scan: null,
  back: [],
  fwd: [],
  navToken: 0,
};

let view, preview, sidebar;

const save = debounce(() => api.saveSettings(S.settings).catch((e) => console.warn('settings', e)), 400);

// ---------- helpers ----------

const sep = () => S.places?.sep || '\\';
const normPath = (p) => (p || '').replace(/[\\/]+$/, '').toLowerCase();

function parentOf(path) {
  const p = path.replace(/[\\/]+$/, '');
  const i = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'));
  if (i < 0) return null;
  const parent = p.slice(0, i);
  return /^[A-Za-z]:$/.test(parent) ? parent + '\\' : parent || '/';
}

function relDir(root, path) {
  let rest = path.slice(root.length).replace(/^[\\/]+/, '');
  const i = Math.max(rest.lastIndexOf('\\'), rest.lastIndexOf('/'));
  return i >= 0 ? rest.slice(0, i) : '';
}

function drivesAsItems() {
  return (S.places?.drives || []).map((d) => toItem({ name: driveName(d), path: d.path, isDir: true, size: 0, mtime: 0, archive: false, hidden: false }, { drive: d }));
}

const isFav = (path) => S.settings.favorites.some((f) => normPath(f.path) === normPath(path));

function toggleFavorite(path, name, archive = false) {
  if (path == null) return;
  const favs = S.settings.favorites;
  const i = favs.findIndex((f) => normPath(f.path) === normPath(path));
  if (i >= 0) {
    favs.splice(i, 1);
    toast('Favorilerden çıkarıldı');
  } else {
    favs.push({ path, name: name || baseName(path), archive });
    toast('Favorilere eklendi');
  }
  save();
  sidebar.render(S.places, favs);
  updateFavButton();
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    toast('Panoya kopyalandı');
  } catch {
    toast('Kopyalanamadı', 'error');
  }
}

// ---------- navigation ----------

async function navigate(path, { push = true, focusPath = null, keepScroll = false, keepSelection = false } = {}) {
  if (S.scan) stopScan(true, false);
  const token = ++S.navToken;
  setLoading(true);
  let listing;
  try {
    if (path == null) listing = { path: null, parent: null, inArchive: false, entries: null };
    else listing = await api.listDir(path, S.settings.showHidden);
  } catch (e) {
    if (token === S.navToken) {
      setLoading(false);
      toast(`Açılamadı: ${e}`, 'error', 4500);
    }
    return false;
  }
  if (token !== S.navToken) return false;
  setLoading(false);
  const changed = S.path === undefined || normPath(S.path) !== normPath(listing.path) || (S.path === null) !== (listing.path === null);
  if (push && changed && S.path !== undefined) {
    S.back.push(S.path);
    S.fwd = [];
  }
  S.path = listing.path;
  S.listing = listing;
  S.all = listing.entries ? listing.entries.map((e) => toItem(e)) : drivesAsItems();
  if (changed) {
    S.search = '';
    $('#search').value = '';
  }
  S.settings.lastPath = S.path;
  save();
  applyFilters({ focusPath, keepScroll, keepSelection });
  renderCrumbs();
  updateNav();
  sidebar.highlight(S.path);
  updateFavButton();
  $('#search').placeholder = S.path == null ? 'Sürücülerde ara' : `${baseName(S.path) || S.path} içinde ara`;
  api.setTitle(S.path == null ? 'Formia' : `${baseName(S.path) || S.path} — Formia`).catch(() => {});
  return true;
}

function goBack() {
  if (!S.back.length) return;
  const target = S.back.pop();
  S.fwd.push(S.path);
  navigate(target, { push: false, focusPath: S.path });
}

function goForward() {
  if (!S.fwd.length) return;
  const target = S.fwd.pop();
  S.back.push(S.path);
  navigate(target, { push: false });
}

function goUp() {
  if (S.scan) return stopScan();
  if (S.path == null) return;
  navigate(S.listing?.parent ?? null, { focusPath: S.path });
}

function refresh() {
  if (S.scan) return startScan();
  loadPlaces();
  navigate(S.path, { push: false, keepScroll: true, keepSelection: true });
}

function openItem(item) {
  if (!item) return;
  if (item.kind === 'dir' || item.kind === 'archive' || item.kind === 'drive') navigate(item.path);
  else api.open(item.path).catch((e) => toast(`Açılamadı: ${e}`, 'error'));
}

async function loadPlaces() {
  try {
    S.places = await api.places();
  } catch (e) {
    S.places = S.places || { quick: [], drives: [], sep: '\\' };
  }
  sidebar.render(S.places, S.settings.favorites);
  if (S.path === null) {
    S.all = drivesAsItems();
    applyFilters({ keepSelection: true, keepScroll: true });
  }
}

// ---------- filtering / sorting ----------

const RANK = { drive: 0, dir: 1, archive: 2 };
const SHOW_3D = new Set(['dir', 'archive', 'drive', 'model', 'gcode']);

function sortItems(list) {
  const { sortKey: key, sortDir: dir } = S.settings;
  const scan = !!S.scan;
  const cmp = {
    name: (a, b) => naturalCompare(a.name, b.name),
    mtime: (a, b) => a.mtime - b.mtime,
    size: (a, b) => a.size - b.size,
    type: (a, b) => naturalCompare(a.ext, b.ext),
    loc: (a, b) => naturalCompare(a.rel || '', b.rel || ''),
  }[key] || ((a, b) => naturalCompare(a.name, b.name));
  return list.slice().sort((a, b) => {
    if (a.kind === 'drive' && b.kind === 'drive') return naturalCompare(a.path, b.path);
    if (!scan) {
      const ra = RANK[a.kind] ?? 3;
      const rb = RANK[b.kind] ?? 3;
      if (ra !== rb) return ra - rb;
    }
    return dir * cmp(a, b) || naturalCompare(a.name, b.name);
  });
}

function applyFilters(opts = {}) {
  const src = S.scan ? S.scan.items : S.all;
  const terms = fold(S.search.trim()).split(/\s+/).filter(Boolean);
  let list = src;
  if (S.settings.only3d && !S.scan) list = list.filter((i) => SHOW_3D.has(i.kind));
  if (terms.length) {
    list = list.filter((i) => {
      const n = (i._f ??= fold(i.name + (i.rel ? ' ' + i.rel : '')));
      return terms.every((t) => n.includes(t));
    });
  }
  S.items = sortItems(list);
  view.setShowLocation(!!S.scan);
  $('#list-header').classList.toggle('with-location', !!S.scan);
  view.setItems(S.items, opts);
  updateEmpty();
  updateStatus();
}

const applySearch = debounce(() => applyFilters({ keepSelection: true }), 90);

// ---------- recursive scan ----------

function startScan() {
  if (S.path == null) {
    toast('Taramak için önce bir klasör açın');
    return;
  }
  if (S.scan && !S.scan.done) api.scanCancel(S.scan.id).catch(() => {});
  const root = S.path;
  const scan = { id: null, root, items: [], dirs: 0, files: 0, done: false, error: null, t0: performance.now() };
  S.scan = scan;
  updateScanUi();
  let pending = false;
  const flush = () => {
    if (pending) return;
    pending = true;
    setTimeout(() => {
      pending = false;
      if (S.scan === scan) applyFilters({ keepScroll: true, keepSelection: true });
    }, 350);
  };
  applyFilters();
  api
    .scanStart(root, SCAN_EXTS, S.settings.showHidden, S.settings.scanArchives, (ev) => {
      if (S.scan !== scan) return;
      if (ev.type === 'batch') {
        for (const e of ev.entries) scan.items.push(toItem(e, { rel: relDir(root, e.path) }));
        scan.dirs = ev.dirs;
        scan.files = ev.files;
        flush();
        updateStatus();
      } else if (ev.type === 'done') {
        scan.done = true;
        scan.dirs = ev.dirs;
        scan.files = ev.files;
        scan.error = ev.error;
        scan.t1 = performance.now();
        if (ev.error) toast(`Tarama hatası: ${ev.error}`, 'error');
        applyFilters({ keepScroll: true, keepSelection: true });
        updateScanUi();
      }
    })
    .then((id) => {
      scan.id = id;
      if (S.scan !== scan) api.scanCancel(id).catch(() => {});
    })
    .catch((e) => toast(`Tarama başlatılamadı: ${e}`, 'error'));
}

function stopScan(exit = true, rerender = true) {
  const scan = S.scan;
  if (!scan) return;
  if (!scan.done && scan.id != null) api.scanCancel(scan.id).catch(() => {});
  if (exit) {
    S.scan = null;
    updateScanUi();
    if (rerender) applyFilters();
  }
}

// ---------- UI updates ----------

function setLoading(on) {
  $('#loading-bar').hidden = !on;
}

function updateNav() {
  $('#btn-back').disabled = !S.back.length;
  $('#btn-fwd').disabled = !S.fwd.length;
  $('#btn-up').disabled = S.path == null && !S.scan;
}

function updateFavButton() {
  const b = $('#btn-fav');
  const on = S.path != null && isFav(S.path);
  b.classList.toggle('on', on);
  b.disabled = S.path == null;
  b.title = on ? 'Favorilerden çıkar (Ctrl+D)' : 'Favorilere ekle (Ctrl+D)';
}

function updateScanUi() {
  const chip = $('#chip-scan');
  const running = S.scan && !S.scan.done;
  chip.classList.toggle('on', !!S.scan);
  chip.innerHTML = `${icon('scan')}<span>${S.scan ? (running ? 'Taranıyor…' : 'Tarama sonucu') : 'Alt klasörleri tara'}</span>${S.scan ? `<span class="chip-x" title="Taramadan çık (Esc)">${icon('close')}</span>` : ''}`;
  chip.classList.toggle('busy', !!running);
  $('#chip-3d').disabled = !!S.scan;
  updateNav();
}

function updateChips() {
  const c = $('#chip-3d');
  c.classList.toggle('on', S.settings.only3d);
  c.innerHTML = `${icon('cube')}<span>Sadece 3D</span>`;
}

function updateViewButtons() {
  $('#view-grid').classList.toggle('on', S.settings.view === 'grid');
  $('#view-list').classList.toggle('on', S.settings.view === 'list');
  $('#list-header').hidden = S.settings.view !== 'list';
  const idx = Math.max(0, THUMB_STEPS.indexOf(S.settings.thumbSize));
  $('#thumb-size').value = idx;
  $('#thumb-size').disabled = S.settings.view !== 'grid';
  $('#sort-key').value = S.settings.sortKey;
  $('#sort-dir').innerHTML = icon(S.settings.sortDir > 0 ? 'sortAsc' : 'sortDesc');
  for (const b of document.querySelectorAll('#list-header [data-sort]')) {
    b.classList.toggle('sorted', b.dataset.sort === S.settings.sortKey);
    b.dataset.dir = S.settings.sortDir > 0 ? 'asc' : 'desc';
  }
}

function updateEmpty() {
  const e = $('#empty');
  let msg = '';
  if (!S.items.length) {
    if (S.scan) msg = S.scan.done ? (S.search ? 'Aramayla eşleşen model yok' : 'Bu klasörün altında 3D model bulunamadı') : 'Modeller aranıyor…';
    else if (S.search) msg = `“${esc(S.search)}” ile eşleşen öğe yok`;
    else if (S.settings.only3d && S.all.length) msg = 'Bu klasörde 3D dosya yok<small>“Sadece 3D” filtresini kapatarak diğer dosyaları görebilirsiniz</small>';
    else if (S.path === null) msg = 'Sürücü bulunamadı';
    else msg = 'Bu klasör boş';
  }
  e.hidden = !msg;
  e.innerHTML = msg ? `${icon(S.scan && !S.scan.done ? 'scan' : 'folder')}<div>${msg}</div>` : '';
}

function updateStatus() {
  const left = $('#status-left');
  const right = $('#status-right');
  const sel = view.selected();
  let text = `${fmtInt(S.items.length)} öğe`;
  if (S.items.length !== (S.scan ? S.scan.items.length : S.all.length)) text += ` (toplam ${fmtInt(S.scan ? S.scan.items.length : S.all.length)})`;
  if (sel.length) {
    const bytes = sel.reduce((a, i) => a + (i.kind === 'dir' ? 0 : i.size || 0), 0);
    text += ` · ${fmtInt(sel.length)} seçili${bytes ? ` (${fmtSize(bytes)})` : ''}`;
  }
  left.textContent = text;
  if (S.scan) {
    const s = S.scan;
    const secs = (((s.t1 ?? performance.now()) - s.t0) / 1000).toLocaleString('tr-TR', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
    right.innerHTML = `${s.done ? '' : '<span class="spinner small"></span>'}${fmtInt(s.dirs)} klasör · ${fmtInt(s.files)} dosya tarandı · ${fmtInt(s.items.length)} model · ${secs} sn`;
  } else {
    right.textContent = S.listing?.inArchive ? 'Zip arşivi içinde' : '';
  }
}

function renderCrumbs() {
  const host = $('#crumbs');
  const parts = [{ label: 'Bu bilgisayar', path: null, ic: 'computer' }];
  const p = S.path;
  if (p != null) {
    if (sep() === '\\') {
      let segs;
      let acc;
      if (p.startsWith('\\\\')) {
        segs = p.slice(2).split('\\').filter(Boolean);
        acc = `\\\\${segs.shift() || ''}\\${segs.shift() || ''}`;
        parts.push({ label: acc, path: acc });
      } else {
        segs = p.split('\\').filter(Boolean);
        const drive = segs.shift();
        acc = drive + '\\';
        const d = S.places?.drives?.find((x) => normPath(x.path) === normPath(acc));
        parts.push({ label: d ? driveName(d) : drive, path: acc });
      }
      for (const s of segs) {
        acc = acc.endsWith('\\') ? acc + s : `${acc}\\${s}`;
        parts.push({ label: s, path: acc, zip: /\.zip$/i.test(s) });
      }
    } else {
      let acc = '';
      parts.push({ label: '/', path: '/' });
      for (const s of p.split('/').filter(Boolean)) {
        acc += '/' + s;
        parts.push({ label: s, path: acc, zip: /\.zip$/i.test(s) });
      }
    }
  }
  host.innerHTML = parts
    .map((c, i) => {
      const last = i === parts.length - 1;
      const ic = c.ic ? icon(c.ic) : c.zip ? icon('archive') : '';
      return `${i ? `<span class="crumb-sep">${icon('chevronRight')}</span>` : ''}<button class="crumb${last ? ' current' : ''}" data-i="${i}">${ic}<span>${esc(c.label)}</span></button>`;
    })
    .join('');
  host._parts = parts;
  host.scrollLeft = host.scrollWidth;
  host.classList.toggle('clipped', host.scrollLeft > 0);
}

function editAddress() {
  const input = $('#address-input');
  $('#crumbs').hidden = true;
  input.hidden = false;
  input.value = S.path ?? '';
  input.focus();
  input.select();
}

function endEditAddress() {
  $('#address-input').hidden = true;
  $('#crumbs').hidden = false;
}

function applyTheme() {
  const t = S.settings.theme === 'system' ? (matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark') : S.settings.theme;
  document.documentElement.dataset.theme = t;
  preview?.applyTheme(t !== 'light');
}

function setThumbSize(px) {
  S.settings.thumbSize = px;
  view.setThumbSize(px);
  updateViewButtons();
  save();
}

function zoom(dir) {
  if (S.settings.view !== 'grid') {
    if (dir > 0) setView('grid');
    return;
  }
  const i = THUMB_STEPS.indexOf(S.settings.thumbSize);
  const next = THUMB_STEPS[Math.max(0, Math.min(THUMB_STEPS.length - 1, (i < 0 ? 3 : i) + dir))];
  if (dir < 0 && i === 0) return setView('list');
  setThumbSize(next);
}

function setView(v) {
  S.settings.view = v;
  view.setView(v);
  updateViewButtons();
  save();
}

function setSort(key) {
  if (S.settings.sortKey === key) S.settings.sortDir *= -1;
  else {
    S.settings.sortKey = key;
    S.settings.sortDir = key === 'mtime' || key === 'size' ? -1 : 1;
  }
  updateViewButtons();
  applyFilters({ keepSelection: true, keepScroll: false });
  save();
}

// ---------- context menus ----------

function itemMenu(e, item) {
  const sel = view.selected();
  const multi = sel.length > 1;
  const entries = [];
  if (item) {
    const container = item.kind === 'dir' || item.kind === 'archive' || item.kind === 'drive';
    if (container) entries.push({ label: 'Aç', icon: 'folder', hint: 'Enter', action: () => openItem(item) });
    else entries.push({ label: 'Önizle', icon: 'eye', hint: 'Space', action: () => preview.open(item) });
    entries.push({ label: container ? 'Sistem gezgininde aç' : 'Varsayılan uygulamayla aç', icon: 'external', hint: container ? '' : 'Enter', action: () => api.open(item.path).catch((er) => toast(`Açılamadı: ${er}`, 'error')) });
    if (item.kind !== 'drive') entries.push({ label: 'Klasörde göster', icon: 'reveal', action: () => api.reveal(item.path).catch((er) => toast(String(er), 'error')) });
    if (S.scan) entries.push({ label: 'Bulunduğu klasöre git', icon: 'folder', action: () => navigate(parentOf(item.path), { focusPath: item.path }) });
    entries.push('-');
    if (container) entries.push({ label: isFav(item.path) ? 'Favorilerden çıkar' : 'Favorilere ekle', icon: 'star', action: () => toggleFavorite(item.path, item.kind === 'drive' ? item.name : null, item.kind === 'archive') });
    entries.push({ label: multi ? `${sel.length} yolu kopyala` : 'Yolu kopyala', icon: 'copy', action: () => copyText(multi ? sel.map((s) => s.path).join('\r\n') : item.path) });
    entries.push({ label: 'Adı kopyala', icon: 'copy', action: () => copyText(item.name) });
    if (item.kind !== 'drive') {
      entries.push('-');
      if (!multi) entries.push({ label: 'Yeniden adlandır', icon: 'rename', hint: 'F2', action: renameFocused });
      entries.push({ label: multi ? `${fmtInt(sel.length)} öğeyi sil` : 'Sil', icon: 'trash', hint: 'Del', danger: true, action: () => deleteItems(multi ? sel : [item]) });
    }
  } else {
    entries.push({ label: 'Yenile', icon: 'refresh', hint: 'F5', action: refresh });
    entries.push('-');
    entries.push({ label: 'Izgara görünümü', icon: 'grid', checked: S.settings.view === 'grid', action: () => setView('grid') });
    entries.push({ label: 'Liste görünümü', icon: 'list', checked: S.settings.view === 'list', action: () => setView('list') });
    entries.push({ label: 'Sadece 3D', icon: 'cube', checked: S.settings.only3d, disabled: !!S.scan, action: toggleOnly3d });
    if (S.path != null) {
      entries.push({ label: S.scan ? 'Taramadan çık' : 'Alt klasörleri tara', icon: 'scan', action: () => (S.scan ? stopScan() : startScan()) });
      entries.push('-');
      entries.push({ label: isFav(S.path) ? 'Bu klasörü favorilerden çıkar' : 'Bu klasörü favorilere ekle', icon: 'star', action: () => toggleFavorite(S.path, null, !!S.listing?.inArchive) });
      entries.push({ label: 'Sistem gezgininde aç', icon: 'external', action: () => api.open(S.path).catch((er) => toast(String(er), 'error')) });
      entries.push({ label: 'Yolu kopyala', icon: 'copy', action: () => copyText(S.path) });
    }
  }
  showMenu(e.clientX, e.clientY, entries);
}

function favoriteMenu(e, index) {
  const f = S.settings.favorites[index];
  if (!f) return;
  const move = (d) => {
    const favs = S.settings.favorites;
    const j = index + d;
    if (j < 0 || j >= favs.length) return;
    [favs[index], favs[j]] = [favs[j], favs[index]];
    save();
    sidebar.render(S.places, favs);
  };
  showMenu(e.clientX, e.clientY, [
    { label: 'Aç', icon: 'folder', action: () => navigate(f.path) },
    { label: 'Sistem gezgininde aç', icon: 'external', action: () => api.open(f.path).catch((er) => toast(String(er), 'error')) },
    '-',
    { label: 'Yukarı taşı', icon: 'up', disabled: index === 0, action: () => move(-1) },
    { label: 'Aşağı taşı', icon: 'chevronDown', disabled: index === S.settings.favorites.length - 1, action: () => move(1) },
    { label: 'Yeniden adlandır', icon: 'fileText', action: () => renameFavorite(index) },
    '-',
    { label: 'Favorilerden çıkar', icon: 'trash', action: () => toggleFavorite(f.path) },
  ]);
}

function renameFavorite(index) {
  const f = S.settings.favorites[index];
  const a = document.querySelector(`.fav[data-fav="${index}"] .nav-label`);
  if (!a) return;
  const input = document.createElement('input');
  input.className = 'fav-rename';
  input.value = f.name || baseName(f.path);
  a.replaceChildren(input);
  input.focus();
  input.select();
  const done = (ok) => {
    if (ok && input.value.trim()) f.name = input.value.trim();
    save();
    sidebar.render(S.places, S.settings.favorites);
  };
  input.addEventListener('keydown', (e) => {
    e.stopPropagation();
    if (e.key === 'Enter') done(true);
    if (e.key === 'Escape') done(false);
  });
  input.addEventListener('blur', () => done(true));
  input.addEventListener('click', (e) => e.stopPropagation());
}

function toggleOnly3d() {
  S.settings.only3d = !S.settings.only3d;
  updateChips();
  applyFilters({ keepSelection: true });
  save();
}

// ---------- delete / rename ----------

const trashName = () => (S.places?.os === 'macos' ? 'Çöp Kutusu' : 'Geri Dönüşüm Kutusu');

function deleteBody(items) {
  const wrap = document.createElement('div');
  const folders = items.filter((i) => i.kind === 'dir').length;
  const what = items.length === 1 ? `“${items[0].name}”` : `Bu ${fmtInt(items.length)} öğe`;
  wrap.innerHTML = `<p>${esc(what)} ${esc(trashName())}’na taşınsın mı?${folders ? ` <span class="muted">${folders === 1 && items.length === 1 ? 'Klasörün' : 'Klasörlerin'} içindekiler de taşınır.</span>` : ''}</p>`;
  const list = document.createElement('ul');
  list.className = 'confirm-list';
  for (const it of items.slice(0, 6)) {
    const li = document.createElement('li');
    li.className = `k-${it.kind}`;
    li.innerHTML = `<span class="confirm-ico">${bigIcon(it.kind)}</span><span class="confirm-name">${esc(it.name)}</span><span class="muted">${it.kind === 'dir' ? 'Klasör' : fmtSize(it.size)}</span>`;
    list.append(li);
  }
  if (items.length > 6) list.insertAdjacentHTML('beforeend', `<li class="muted">… ve ${fmtInt(items.length - 6)} öğe daha</li>`);
  wrap.append(list);
  return wrap;
}

/** Asks, moves the items to the Recycle Bin and selects the item that took their place. */
async function deleteItems(items) {
  items = items.filter((i) => i && i.kind !== 'drive');
  if (!items.length) return;
  const ok = await confirmDialog({ title: items.length === 1 ? 'Sil' : `${fmtInt(items.length)} öğeyi sil`, body: deleteBody(items), confirmLabel: 'Sil', danger: true });
  if (!ok) return;
  const anchor = Math.max(0, Math.min(...items.map((i) => S.items.indexOf(i)).filter((i) => i >= 0)));
  let res;
  try {
    res = await api.deletePaths(items.map((i) => i.path));
  } catch (e) {
    toast(`Silinemedi: ${e}`, 'error', 5000);
    return;
  }
  const gone = new Set(res.deleted.map(normPath));
  if (gone.size) {
    const keep = (i) => !gone.has(normPath(i.path));
    S.all = S.all.filter(keep);
    if (S.scan) S.scan.items = S.scan.items.filter(keep);
    applyFilters({ keepScroll: true });
    const next = Math.min(anchor, S.items.length - 1);
    if (next >= 0) {
      view.selectOnly(next);
      view.ensureVisible(next);
    }
    toast(gone.size === 1 ? `${trashName()}’na taşındı` : `${fmtInt(gone.size)} öğe ${trashName()}’na taşındı`);
  }
  if (res.failed.length) toast(`${fmtInt(res.failed.length)} öğe silinemedi: ${res.failed[0].error}`, 'error', 6000);
  if (preview.isOpen) {
    const it = view.focused();
    if (it) preview.show(it);
    else preview.close();
  }
}

function renameFocused() {
  const i = view.focus >= 0 ? view.focus : S.items.indexOf(view.selected()[0]);
  const item = S.items[i];
  if (!item || item.kind === 'drive') return;
  view.selectOnly(i);
  view.beginRename(i, (value) => renameItem(item, value));
}

async function renameItem(item, value) {
  const name = (value ?? '').trim();
  if (!name || name === item.name) return;
  let newPath;
  try {
    newPath = await api.renamePath(item.path, name);
  } catch (e) {
    toast(`Yeniden adlandırılamadı: ${e}`, 'error', 5000);
    return;
  }
  // Favorites pointing at (or inside) a renamed folder follow it.
  const old = normPath(item.path);
  for (const f of S.settings.favorites) {
    const p = normPath(f.path);
    if (p === old || p.startsWith(old + '\\') || p.startsWith(old + '/')) {
      if (p === old && f.name === item.name) f.name = name;
      f.path = newPath + f.path.slice(item.path.length);
    }
  }
  save();
  sidebar.render(S.places, S.settings.favorites);
  const raw = { name, path: newPath, isDir: item.isDir, size: item.size, mtime: item.mtime, archive: !item.isDir && /\.zip$/i.test(name), hidden: item.hidden };
  const updated = toItem(raw, item.rel != null ? { rel: item.rel } : {});
  const swap = (list) => list.map((i) => (i === item || normPath(i.path) === old ? updated : i));
  S.all = swap(S.all);
  if (S.scan) S.scan.items = swap(S.scan.items);
  applyFilters({ keepScroll: true, focusPath: newPath });
}

// ---------- drag out (to slicers, Explorer, …) ----------

/** After a mousedown on an item, starts a native file drag once the pointer moves a few pixels. */
function beginDragWatch(down, item) {
  if (item.kind === 'drive') return;
  const x0 = down.clientX;
  const y0 = down.clientY;
  const move = (e) => {
    if (!(e.buttons & 1)) return stop();
    if (Math.hypot(e.clientX - x0, e.clientY - y0) < 6) return;
    stop();
    const sel = view.selected();
    const paths = sel.some((s) => s.path === item.path) ? sel.filter((s) => s.kind !== 'drive').map((s) => s.path) : [item.path];
    api.startDrag(paths).catch((er) => toast(`Sürükleme başlatılamadı: ${er}`, 'error'));
  };
  const stop = () => {
    window.removeEventListener('mousemove', move);
    window.removeEventListener('mouseup', stop);
  };
  window.addEventListener('mousemove', move);
  window.addEventListener('mouseup', stop);
}

// ---------- wiring ----------

function bindToolbar() {
  const set = (id, ic) => ($(id).innerHTML = icon(ic));
  set('#btn-back', 'back');
  set('#btn-fwd', 'forward');
  set('#btn-up', 'up');
  set('#btn-refresh', 'refresh');
  set('#btn-fav', 'star');
  set('#view-grid', 'grid');
  set('#view-list', 'list');
  set('#btn-settings', 'settings');
  $('.search-ico').innerHTML = icon('search');

  $('#btn-back').onclick = goBack;
  $('#btn-fwd').onclick = goForward;
  $('#btn-up').onclick = goUp;
  $('#btn-refresh').onclick = refresh;
  $('#btn-fav').onclick = (e) => {
    e.stopPropagation();
    toggleFavorite(S.path, null, !!S.listing?.inArchive);
  };
  $('#crumbs').addEventListener('click', (e) => {
    const b = e.target.closest('.crumb');
    if (!b) return;
    e.stopPropagation();
    const c = $('#crumbs')._parts[Number(b.dataset.i)];
    if (normPath(c.path) !== normPath(S.path) || (c.path === null) !== (S.path === null) || S.scan) navigate(c.path, { focusPath: null });
  });
  $('#address').addEventListener('click', (e) => {
    if (e.target.closest('.crumb, .fav-btn, input')) return;
    editAddress();
  });
  const input = $('#address-input');
  input.addEventListener('keydown', async (e) => {
    if (e.key === 'Enter') {
      const v = input.value.trim();
      endEditAddress();
      if (!v) return navigate(null);
      if (!(await navigate(v))) {
        const parent = parentOf(v);
        if (parent) navigate(parent, { focusPath: v });
      }
      view.root.focus();
    } else if (e.key === 'Escape') {
      endEditAddress();
      view.root.focus();
    }
  });
  input.addEventListener('blur', endEditAddress);

  const search = $('#search');
  search.addEventListener('input', () => {
    S.search = search.value;
    applySearch();
  });
  search.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      if (search.value) {
        search.value = '';
        S.search = '';
        applyFilters({ keepSelection: true });
      } else view.root.focus();
      e.stopPropagation();
    } else if (e.key === 'Enter' || e.key === 'ArrowDown') {
      e.preventDefault();
      view.root.focus();
      view.moveTo(0, false);
    }
  });

  $('#chip-3d').onclick = toggleOnly3d;
  $('#chip-scan').onclick = (e) => {
    if (e.target.closest('.chip-x') || S.scan) stopScan();
    else startScan();
  };
  $('#sort-key').onchange = (e) => setSort(e.target.value);
  $('#sort-dir').onclick = () => setSort(S.settings.sortKey);
  $('#view-grid').onclick = () => setView('grid');
  $('#view-list').onclick = () => setView('list');
  $('#thumb-size').oninput = (e) => setThumbSize(THUMB_STEPS[Number(e.target.value)]);
  $('#btn-settings').onclick = () =>
    openSettings(S.settings, (key) => {
      if (key === 'theme') applyTheme();
      if (key === 'modelColor') preview.applyColor(S.settings.modelColor);
      if (key === 'showHidden') refresh();
      if (key === 'cache' || key === 'preferEmbedded') {
        bumpThumbGen();
        view.clearNodes();
        view.relayout();
      }
      save();
    });
  for (const b of document.querySelectorAll('#list-header [data-sort]')) b.onclick = () => setSort(b.dataset.sort);

  // Sidebar width
  const resizer = $('#sidebar-resizer');
  resizer.addEventListener('pointerdown', (e) => {
    resizer.setPointerCapture(e.pointerId);
    const move = (ev) => {
      S.settings.sidebarWidth = Math.max(180, Math.min(420, ev.clientX));
      document.documentElement.style.setProperty('--sidebar-w', `${S.settings.sidebarWidth}px`);
    };
    const up = () => {
      resizer.removeEventListener('pointermove', move);
      resizer.removeEventListener('pointerup', up);
      save();
    };
    resizer.addEventListener('pointermove', move);
    resizer.addEventListener('pointerup', up);
  });
}

const IS_MAC = /Mac/i.test(navigator.platform || navigator.userAgent);

/** Frameless window on Windows: own minimise / maximise / close buttons and drag areas. */
function bindWindowFrame() {
  document.documentElement.classList.add(IS_MAC ? 'frame-mac' : 'frame-custom');
  if (IS_MAC) return; // macOS keeps its traffic lights over the transparent title bar
  const win = api.win();
  const box = $('#win-controls');
  box.hidden = false;
  $('#wc-min').innerHTML = icon('winMin');
  $('#wc-close').innerHTML = icon('winClose');
  const syncMax = async () => {
    const max = await win.isMaximized().catch(() => false);
    $('#wc-max').innerHTML = icon(max ? 'winRestore' : 'winMax');
    $('#wc-max').title = max ? 'Önceki boyutuna getir' : 'Ekranı kapla';
    document.documentElement.classList.toggle('win-maximized', max);
  };
  $('#wc-min').onclick = () => win.minimize();
  $('#wc-max').onclick = () => win.toggleMaximize();
  $('#wc-close').onclick = () => win.close();
  win.onResized(syncMax).catch(() => {});
  syncMax();
}

function bindKeys() {
  document.addEventListener('keydown', (e) => {
    // Before everything else: the frameless window has no system menu to do this.
    if (e.altKey && e.key === 'F4') return e.preventDefault(), api.win().close();
    if (document.querySelector('dialog[open]') || isMenuOpen()) return;
    if (preview.isOpen) {
      if (preview.handleKey(e)) e.preventDefault();
      return;
    }
    const k = e.key;
    const lower = k.length === 1 ? k.toLowerCase() : k;
    const ctrl = e.ctrlKey || e.metaKey;
    if ((ctrl && lower === 'l') || (k === 'F4' && !e.altKey) || (e.altKey && lower === 'd')) return e.preventDefault(), editAddress();
    if (ctrl && e.shiftKey && lower === 'f') return e.preventDefault(), S.scan ? stopScan() : startScan();
    if (ctrl && lower === 'f') return e.preventDefault(), $('#search').focus(), $('#search').select();
    if (k === 'F5' || (ctrl && lower === 'r')) return e.preventDefault(), refresh();
    if (e.altKey && k === 'ArrowLeft') return e.preventDefault(), goBack();
    if (e.altKey && k === 'ArrowRight') return e.preventDefault(), goForward();
    if (e.altKey && k === 'ArrowUp') return e.preventDefault(), goUp();
    if (ctrl && lower === 'd') return e.preventDefault(), toggleFavorite(S.path, null, !!S.listing?.inArchive);
    if (ctrl && (k === '+' || k === '=')) return e.preventDefault(), zoom(1);
    if (ctrl && k === '-') return e.preventDefault(), zoom(-1);
    if (ctrl && k === '1') return e.preventDefault(), setView('grid');
    if (ctrl && k === '2') return e.preventDefault(), setView('list');
    if (isTypingTarget(e.target)) return;
    // Finder-style Cmd+↑ / Cmd+↓ (Alt+↑ / Alt+↓ on Windows): parent folder / open the selected item.
    if (e.metaKey && k === 'ArrowUp') return e.preventDefault(), goUp();
    if ((e.metaKey || e.altKey) && k === 'ArrowDown') return e.preventDefault(), openItem(view.focused());
    if (k === ' ') {
      e.preventDefault();
      const it = view.focused();
      if (it) preview.open(it);
    } else if (k === 'Enter') {
      e.preventDefault();
      const sel = view.selected();
      if (sel.length > 1 && sel.every((s) => s.kind !== 'dir' && s.kind !== 'archive' && s.kind !== 'drive')) sel.slice(0, 20).forEach(openItem);
      else openItem(view.focused());
    } else if (k === 'Delete' || (e.metaKey && k === 'Backspace')) {
      e.preventDefault();
      const sel = view.selected();
      deleteItems(sel.length ? sel : [view.focused()]);
    } else if (k === 'F2') {
      e.preventDefault();
      renameFocused();
    } else if (k === 'Backspace') {
      e.preventDefault();
      goUp();
    } else if (k === 'Escape') {
      if (S.scan) stopScan();
      else if (S.search) {
        $('#search').value = '';
        S.search = '';
        applyFilters({ keepSelection: true });
      } else view.clearSelection();
    } else if (k === 'ContextMenu' || (e.shiftKey && k === 'F10')) {
      e.preventDefault();
      const it = view.focused();
      const n = document.querySelector('.card.focused, .row.focused');
      const r = n?.getBoundingClientRect() ?? view.root.getBoundingClientRect();
      itemMenu({ clientX: r.left + 20, clientY: r.top + 20 }, it);
    } else if (view.handleKey(e)) e.preventDefault();
  });
  window.addEventListener('mouseup', (e) => {
    if (e.button === 3) goBack();
    if (e.button === 4) goForward();
  });
}

async function init() {
  S.settings = mergeSettings(await api.getSettings().catch(() => null));
  document.documentElement.style.setProperty('--sidebar-w', `${S.settings.sidebarWidth}px`);
  applyTheme();
  matchMedia('(prefers-color-scheme: light)').addEventListener('change', applyTheme);
  api.setPreferEmbedded(S.settings.preferEmbedded).catch(() => {});

  view = new FileView($('#view'), {
    open: openItem,
    preview: (it) => preview.open(it),
    context: itemMenu,
    selection: updateStatus,
    zoom,
    dragStart: beginDragWatch,
  });
  view.setView(S.settings.view);
  view.setThumbSize(S.settings.thumbSize);
  preview = new Preview({
    step: (d) => view.step(d),
    rowSize: () => view.metrics.cols,
    getSettings: () => S.settings,
    saveViewer: save,
    onDelete: (item) => deleteItems([item]),
  });
  applyTheme();
  sidebar = new Sidebar($('#places'), {
    navigate: (p) => navigate(p),
    favoriteMenu,
    reorder: (from, to) => {
      const favs = S.settings.favorites;
      const [f] = favs.splice(from, 1);
      favs.splice(to, 0, f);
      save();
      sidebar.render(S.places, favs);
    },
  });
  bindToolbar();
  bindWindowFrame();
  bindKeys();
  updateChips();
  updateScanUi();
  updateViewButtons();
  await loadPlaces();

  let lastFocus = 0;
  window.addEventListener('focus', () => {
    if (Date.now() - lastFocus < 4000) return;
    lastFocus = Date.now();
    loadPlaces();
  });

  const home = S.places?.quick?.find((p) => p.id === 'home')?.path ?? null;
  const start = S.settings.lastPath ?? home;
  if (!(await navigate(start, { push: false }))) {
    if (!(await navigate(home, { push: false }))) await navigate(null, { push: false });
  }
  view.root.focus();
}

init().catch((e) => {
  console.error(e);
  document.body.insertAdjacentHTML('beforeend', `<pre class="fatal">Başlatma hatası: ${esc(e?.stack || e)}</pre>`);
});
