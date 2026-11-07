// Left navigation: quick places, favorites, drives.
import { icon } from './icons.js';
import { esc, fmtSize } from './util.js';

const PLACE_ICON = { home: 'home', desktop: 'desktop', documents: 'documents', downloads: 'downloads', pictures: 'pictures' };

export function baseName(path) {
  const p = path.replace(/[\\/]+$/, '');
  const i = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'));
  return i >= 0 ? p.slice(i + 1) || p : p;
}

export function driveName(d) {
  // macOS / Linux mount points: just the volume name, as Finder shows it ("Macintosh HD", "USB").
  if (d.path.startsWith('/')) return d.label || baseName(d.path) || d.path;
  const letter = d.path.replace(/[\\/]+$/, '');
  if (d.label) return `${d.label} (${letter})`;
  const def = { removable: 'Çıkarılabilir Disk', network: 'Ağ Sürücüsü', cdrom: 'CD Sürücüsü', fixed: 'Yerel Disk' }[d.kind] || 'Disk';
  return `${def} (${letter})`;
}

export class Sidebar {
  constructor(root, { navigate, favoriteMenu, reorder }) {
    this.root = root;
    this.navigate = navigate;
    this.favoriteMenu = favoriteMenu;
    this.reorder = reorder;
    this.active = null;
    this.dragFrom = -1;
    root.addEventListener('click', (e) => {
      const a = e.target.closest('[data-path], [data-computer]');
      if (!a) return;
      this.navigate(a.dataset.computer != null ? null : a.dataset.path);
    });
    root.addEventListener('contextmenu', (e) => {
      const a = e.target.closest('.fav');
      if (!a) return;
      e.preventDefault();
      this.favoriteMenu(e, Number(a.dataset.fav));
    });
    root.addEventListener('dragstart', (e) => {
      const a = e.target.closest('.fav');
      if (!a) return;
      this.dragFrom = Number(a.dataset.fav);
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', a.dataset.path);
    });
    root.addEventListener('dragover', (e) => {
      const a = e.target.closest('.fav');
      if (!a || this.dragFrom < 0) return;
      e.preventDefault();
      for (const n of root.querySelectorAll('.fav.drop')) n.classList.remove('drop');
      a.classList.add('drop');
    });
    root.addEventListener('drop', (e) => {
      const a = e.target.closest('.fav');
      for (const n of root.querySelectorAll('.fav.drop')) n.classList.remove('drop');
      if (!a || this.dragFrom < 0) return;
      e.preventDefault();
      this.reorder(this.dragFrom, Number(a.dataset.fav));
      this.dragFrom = -1;
    });
    root.addEventListener('dragend', () => {
      this.dragFrom = -1;
      for (const n of root.querySelectorAll('.fav.drop')) n.classList.remove('drop');
    });
  }

  render(places, favorites) {
    const link = (path, label, ic, extra = '', attrs = '') =>
      `<a class="nav-item${extra}" data-path="${esc(path)}" title="${esc(path)}" ${attrs}>${icon(ic)}<span class="nav-label">${esc(label)}</span></a>`;
    const quick = (places?.quick || []).map((p) => link(p.path, p.name, PLACE_ICON[p.id] || 'folder')).join('');
    const favs = favorites.length
      ? favorites.map((f, i) => link(f.path, f.name || baseName(f.path), f.archive ? 'archive' : 'star', ' fav', `data-fav="${i}" draggable="true"`)).join('')
      : `<div class="nav-empty">Klasöre sağ tıklayıp <b>Favorilere ekle</b>’yi seçin ya da ☆ düğmesine basın.</div>`;
    const drives = (places?.drives || [])
      .map((d) => {
        const ic = d.kind === 'removable' ? 'usb' : d.kind === 'network' ? 'network' : 'drive';
        const used = d.total ? 1 - d.free / d.total : 0;
        const bar = d.total ? `<span class="nav-usage${used > 0.9 ? ' full' : ''}"><i style="width:${(used * 100).toFixed(1)}%"></i></span><span class="nav-free">${fmtSize(d.free)} boş</span>` : '';
        return `<a class="nav-item drive" data-path="${esc(d.path)}" title="${esc(d.path)}">${icon(ic)}<span class="nav-label">${esc(driveName(d))}${bar}</span></a>`;
      })
      .join('');
    this.root.innerHTML = `
      <div class="nav-section"><div class="nav-title">Hızlı erişim</div>${quick}</div>
      <div class="nav-section"><div class="nav-title">Favoriler</div>${favs}</div>
      <div class="nav-section"><div class="nav-title"><a class="nav-title-link" data-computer>Bu bilgisayar</a></div>${drives}</div>`;
    this.highlight(this.active);
  }

  highlight(path) {
    this.active = path;
    const norm = (p) => (p || '').replace(/[\\/]+$/, '').toLowerCase();
    const target = norm(path);
    for (const a of this.root.querySelectorAll('.nav-item')) a.classList.toggle('active', path != null && norm(a.dataset.path) === target);
    this.root.querySelector('[data-computer]')?.classList.toggle('active', path == null);
  }
}
