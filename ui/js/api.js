// Thin wrappers around the Tauri bridge.
import { errorMessage } from './i18n.js';
const T = window.__TAURI__;

export const invoke = (cmd, args) => T.core.invoke(cmd, args).catch((error) => { throw errorMessage(error); });

const src = (path, proto) => T.core.convertFileSrc(path, proto);

// Bumped when thumbnails change for reasons the URL cannot see (cache cleared,
// embedded-image preference toggled) so the web view does not reuse old images.
let thumbGen = 0;
export const bumpThumbGen = () => ++thumbGen;
export const thumbUrl = (item) => `${src(item.path, 'thumb')}?v=${item.mtime || 0}-${item.size || 0}-${thumbGen}`;
export const rawUrl = (path) => src(path, 'raw');
export const meshUrl = (path) => src(path, 'mesh');

export const api = {
  places: () => invoke('get_places'),
  listDir: (path, showHidden) => invoke('list_dir', { path, showHidden }),
  open: (path) => invoke('open_path', { path }),
  reveal: (path) => invoke('reveal_path', { path }),
  realPath: (path) => invoke('real_path', { path }),
  startDrag: (paths) => invoke('start_drag', { paths }),
  deletePaths: (paths) => invoke('delete_paths', { paths }),
  renamePath: (path, newName) => invoke('rename_path', { path, newName }),
  thumbState: (path) => invoke('thumb_state', { path }),
  thumbStore: (path, pngBase64) => invoke('thumb_store', { path, pngBase64 }),
  cacheInfo: () => invoke('cache_info'),
  cacheClear: () => invoke('cache_clear'),
  setPreferEmbedded: (value) => invoke('set_prefer_embedded', { value }),
  /** Plates of a multi-plate 3MF: [{id, name, objects, thumb}] (empty for a single plate). */
  plates: (path) => invoke('plate_list', { path }),
  getSettings: () => invoke('get_settings'),
  saveSettings: (value) => invoke('save_settings', { value }),
  scanCancel: (id) => invoke('scan_cancel', { id }),
  scanStart(root, exts, showHidden, includeArchives, onEvent) {
    const ch = new T.core.Channel();
    ch.onmessage = onEvent;
    return invoke('scan_start', { root, exts, showHidden, includeArchives, onEvent: ch });
  },
  setTitle: (title) => T.window.getCurrentWindow().setTitle(title),
  win: () => T.window.getCurrentWindow(),
};
