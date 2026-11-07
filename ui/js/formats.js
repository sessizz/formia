// File type knowledge shared by the browser, thumbnails and preview.
import { extOf } from './util.js';

export const NATIVE_MESH = new Set(['stl', 'obj', '3mf', 'ply', 'off']); // parsed in Rust (mesh://)
export const GLTF = new Set(['glb', 'gltf']);
export const THREE_MODELS = new Set(['fbx', 'dae', '3ds', 'amf', 'wrl', 'vrml']);
export const CAD = new Set(['step', 'stp', 'iges', 'igs', 'brep']);
export const GCODE = new Set(['gcode', 'gco', 'g']);
export const IMAGE = new Set(['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'tga', 'svg', 'avif']);
export const BROWSER_IMAGE = new Set(['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'svg', 'avif']);
export const TEXT = new Set(['txt', 'md', 'log', 'ini', 'cfg', 'conf', 'csv', 'scad', 'json', 'xml', 'yaml', 'yml', 'nc', 'ngc', 'py', 'js', 'html', 'css']);
export const VIDEO = new Set(['mp4', 'webm', 'm4v', 'mov']);
export const PDF = new Set(['pdf']);

export const MODEL_EXTS = new Set([...NATIVE_MESH, ...GLTF, ...THREE_MODELS, ...CAD]);
/** Extensions collected by the recursive archive scan. */
export const SCAN_EXTS = [...MODEL_EXTS];

/** Thumbnail produced by the Rust side (thumb:// protocol). */
const RUST_THUMB = new Set([...NATIVE_MESH, ...GLTF, ...GCODE, 'png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'tga']);
/** Thumbnail rendered in the web view with three.js / occt and cached by Rust. */
const CLIENT_THUMB = new Set([...THREE_MODELS, ...CAD]);

/** Up-axis convention when the file itself does not say. */
const Z_UP = new Set(['stl', '3mf', 'ply', 'off', 'amf', '3ds', ...CAD]);

export const TYPE_LABEL = {
  stl: 'STL modeli', obj: 'OBJ modeli', '3mf': '3MF projesi', ply: 'PLY modeli', off: 'OFF modeli',
  glb: 'glTF ikili', gltf: 'glTF', fbx: 'FBX sahnesi', dae: 'Collada', '3ds': '3DS modeli', amf: 'AMF modeli',
  wrl: 'VRML', vrml: 'VRML', step: 'STEP CAD', stp: 'STEP CAD', iges: 'IGES CAD', igs: 'IGES CAD', brep: 'BREP CAD',
  gcode: 'G-code', gco: 'G-code', g: 'G-code', zip: 'ZIP arşivi', pdf: 'PDF belgesi',
};

export function kindOf(entry) {
  if (entry.drive) return 'drive';
  if (entry.isDir) return 'dir';
  if (entry.archive) return 'archive';
  const ext = entry.ext ?? extOf(entry.name);
  if (MODEL_EXTS.has(ext)) return 'model';
  if (GCODE.has(ext)) return 'gcode';
  if (IMAGE.has(ext)) return 'image';
  if (PDF.has(ext)) return 'pdf';
  if (VIDEO.has(ext)) return 'video';
  if (TEXT.has(ext)) return 'text';
  return 'file';
}

export function thumbSource(ext) {
  if (RUST_THUMB.has(ext)) return 'rust';
  if (CLIENT_THUMB.has(ext)) return 'client';
  return null;
}

export const defaultZUp = (ext) => Z_UP.has(ext);

export function typeLabel(item) {
  if (item.kind === 'dir') return 'Klasör';
  if (item.kind === 'drive') return 'Sürücü';
  if (TYPE_LABEL[item.ext]) return TYPE_LABEL[item.ext];
  if (item.kind === 'image') return `${item.ext.toUpperCase()} görseli`;
  if (item.kind === 'video') return `${item.ext.toUpperCase()} videosu`;
  return item.ext ? `${item.ext.toUpperCase()} dosyası` : 'Dosya';
}

/** Normalizes a Rust entry into the UI item model. */
export function toItem(e, extra = {}) {
  const ext = e.isDir ? '' : extOf(e.name);
  const item = { ...e, ext, ...extra };
  item.kind = kindOf(item);
  item.thumb = item.kind === 'dir' || item.kind === 'archive' || item.kind === 'drive' ? null : thumbSource(ext);
  return item;
}
