import { t, errorMessage } from './i18n.js';
// Turns a file path into a three.js object. Native formats come pre-parsed
// from Rust (mesh://); the rest go through three.js loaders or OpenCascade.
import * as THREE from 'three';
import { rawUrl, meshUrl } from './api.js';
import { NATIVE_MESH, GLTF, CAD, GCODE, defaultZUp } from './formats.js';

export class LoadError extends Error {}

async function fetchBytes(url, signal) {
  const r = await fetch(url, { signal });
  if (!r.ok) throw new LoadError(errorMessage((await r.text().catch(() => '')) || `HTTP ${r.status}`));
  return r.arrayBuffer();
}

/** Directory URL for resolving relative resources (textures, .bin, .mtl). */
function resourceBase(path) {
  const i = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
  return rawUrl(path.slice(0, i)) + '/';
}

/** Material for one filament/material colour (0xRRGGBBAA, sRGB). */
function colorMaterial(base, rgba) {
  const m = base.clone();
  readableColor(m.color.setHex(rgba >>> 8));
  m.userData.filament = true;
  m.userData.fileHex = cssColor(rgba); // as written in the file, for the colour menu
  return m;
}

/** Keeps black filament readable, like slicers do. Mutates and returns `c`. */
export function readableColor(c) {
  if (0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b < 0.012) c.lerp(new THREE.Color(0x2c2c2c), 0.65);
  return c;
}

/** G-code ribbons only: near-white filament would wash out the layer lines. Mutates and returns `c`. */
export function softenWhite(c) {
  if (0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b > 0.8) c.lerp(new THREE.Color(0x9fa2a6), 0.6);
  return c;
}

const cssColor = (rgba) => `#${(rgba >>> 8).toString(16).padStart(6, '0')}`;

function decodeWire(buf, material) {
  const dv = new DataView(buf);
  if (buf.byteLength < 20 || dv.getUint32(0, true) !== 0x5644334d) throw new LoadError(t('geçersiz mesh verisi'));
  const flags = dv.getUint32(8, true);
  const vc = dv.getUint32(12, true);
  const ic = dv.getUint32(16, true);
  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(new Float32Array(buf, 20, vc * 3), 3));
  let off = 20 + vc * 12;
  if (flags & 1) {
    geom.setIndex(new THREE.BufferAttribute(new Uint32Array(buf, off, ic), 1));
    off += ic * 4;
  }
  let materials = null;
  let palette = null;
  if (flags & 4) {
    const n = dv.getUint32(off, true);
    off += 4;
    materials = [];
    palette = [];
    const byColor = new Map();
    for (let g = 0; g < n; g++, off += 12) {
      const start = dv.getUint32(off, true);
      const count = dv.getUint32(off + 4, true);
      const rgba = dv.getUint32(off + 8, true);
      let mi = byColor.get(rgba);
      if (mi === undefined) {
        mi = materials.length;
        byColor.set(rgba, mi);
        materials.push(rgba ? colorMaterial(material, rgba) : material);
        if (rgba) palette.push(cssColor(rgba));
      }
      geom.addGroup(start, count, mi);
    }
  }
  return { geom, zUp: !!(flags & 2), materials, palette };
}

let occtWorker = null;
let occtSeq = 0;
const occtPending = new Map();
function occt(format, buffer) {
  if (!occtWorker) {
    occtWorker = new Worker(new URL('./occt-worker.js', import.meta.url));
    occtWorker.onmessage = (e) => {
      const p = occtPending.get(e.data.id);
      if (!p) return;
      occtPending.delete(e.data.id);
      e.data.error ? p.reject(new LoadError(e.data.error)) : p.resolve(e.data.result);
    };
  }
  const id = ++occtSeq;
  return new Promise((resolve, reject) => {
    occtPending.set(id, { resolve, reject });
    occtWorker.postMessage({ id, format, buffer }, [buffer]);
  });
}

function countTriangles(obj) {
  let n = 0;
  obj.traverse((o) => {
    if (!o.isMesh || !o.geometry) return;
    const g = o.geometry;
    n += g.index ? g.index.count / 3 : (g.attributes.position?.count || 0) / 3;
  });
  return Math.round(n);
}

/**
 * @returns {Promise<{object: THREE.Object3D, zUp: boolean, ownMaterial: boolean, triangles: number, unit: string, lines?: boolean}>}
 */
export async function loadModel(path, ext, material, signal, { plate, toolpath: sliced } = {}) {
  const base = resourceBase(path);
  let object;
  let zUp = defaultZUp(ext);
  let ownMaterial = false;
  let lines = false;
  let palette = null;
  let ribbons = null;
  let toolpath = false;

  if (NATIVE_MESH.has(ext)) {
    const url = plate ? `${meshUrl(path)}?plate=${plate}` : meshUrl(path);
    const wire = decodeWire(await fetchBytes(url, signal), material);
    if (sliced) for (const m of wire.materials || []) if (m.userData.filament) softenWhite(m.color);
    object = new THREE.Mesh(wire.geom, wire.materials || material);
    object.userData.own = true;
    object.userData.colorMaterials = wire.materials;
    palette = wire.palette;
    zUp = wire.zUp;
    ownMaterial = true;
    toolpath = !!sliced; // .gcode.3mf plates arrive as G-code ribbons
  } else if (GLTF.has(ext)) {
    const { GLTFLoader } = await import('three/addons/loaders/GLTFLoader.js');
    const buf = await fetchBytes(rawUrl(path), signal);
    const gltf = await new GLTFLoader().parseAsync(buf, base);
    object = gltf.scene;
  } else if (ext === 'fbx') {
    const { FBXLoader } = await import('three/addons/loaders/FBXLoader.js');
    object = new FBXLoader().parse(await fetchBytes(rawUrl(path), signal), base);
  } else if (ext === 'dae') {
    const { ColladaLoader } = await import('three/addons/loaders/ColladaLoader.js');
    const text = new TextDecoder().decode(await fetchBytes(rawUrl(path), signal));
    const res = new ColladaLoader().parse(text, base);
    if (!res?.scene) throw new LoadError(t('Collada sahnesi okunamadı'));
    object = res.scene;
    zUp = false; // ColladaLoader already converts Z_UP assets
  } else if (ext === '3ds') {
    const { TDSLoader } = await import('three/addons/loaders/TDSLoader.js');
    const l = new TDSLoader();
    l.setResourcePath(base);
    object = l.parse(await fetchBytes(rawUrl(path), signal), base);
  } else if (ext === 'amf') {
    const { AMFLoader } = await import('three/addons/loaders/AMFLoader.js');
    object = new AMFLoader().parse(await fetchBytes(rawUrl(path), signal));
  } else if (ext === 'wrl' || ext === 'vrml') {
    const { VRMLLoader } = await import('three/addons/loaders/VRMLLoader.js');
    const text = new TextDecoder().decode(await fetchBytes(rawUrl(path), signal));
    object = new VRMLLoader().parse(text, base);
  } else if (CAD.has(ext)) {
    const fmt = ext === 'iges' || ext === 'igs' ? 'iges' : ext === 'brep' ? 'brep' : 'step';
    const res = await occt(fmt, await fetchBytes(rawUrl(path), signal));
    if (!res?.success || !res.meshes?.length) throw new LoadError(t('CAD dosyası okunamadı'));
    object = new THREE.Group();
    for (const m of res.meshes) {
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.Float32BufferAttribute(m.attributes.position.array, 3));
      if (m.attributes.normal) g.setAttribute('normal', new THREE.Float32BufferAttribute(m.attributes.normal.array, 3));
      if (m.index) g.setIndex(m.index.array);
      const mat = material.clone();
      mat.flatShading = false;
      // CAD colours are often pure white; tone them down so shading stays visible.
      if (m.color) {
        mat.color.setRGB(m.color[0], m.color[1], m.color[2], THREE.SRGBColorSpace);
        mat.userData.fileHex = `#${mat.color.getHexString()}`;
        mat.color.multiplyScalar(0.62);
      }
      mat.userData.cadColor = !!m.color;
      object.add(new THREE.Mesh(g, mat));
    }
    ownMaterial = true;
  } else if (GCODE.has(ext) && (ribbons = await fetchBytes(meshUrl(path), signal).catch((e) => (e?.name === 'AbortError' ? Promise.reject(e) : null)))) {
    // Finished-print view: extrusion ribbons built in Rust, coloured per filament.
    const wire = decodeWire(ribbons, material);
    for (const m of wire.materials || []) if (m.userData.filament) softenWhite(m.color);
    object = new THREE.Mesh(wire.geom, wire.materials || material);
    object.userData.own = true;
    object.userData.colorMaterials = wire.materials;
    palette = wire.palette;
    zUp = wire.zUp;
    ownMaterial = true;
    toolpath = true;
  } else if (GCODE.has(ext)) {
    // Too big (or unreadable) for ribbons: thin toolpath lines.
    const { GCodeLoader } = await import('three/addons/loaders/GCodeLoader.js');
    const text = new TextDecoder().decode(await fetchBytes(rawUrl(path), signal));
    const l = new GCodeLoader();
    l.splitLayer = false;
    object = l.parse(text);
    // Keep only extrusion moves (material 'extruded'); travel moves would
    // clutter the view and stretch the bounding box to the printer origin.
    const extruded = new THREE.LineBasicMaterial({ color: material.color.clone().offsetHSL(0, 0, 0.05), transparent: true, opacity: 0.85 });
    for (const child of [...object.children]) {
      if (child.material?.name === 'path') {
        object.remove(child);
        child.geometry.dispose();
      } else if (child.material) child.material = extruded;
    }
    zUp = false; // GCodeLoader rotates into Y-up itself
    lines = true;
  } else {
    throw new LoadError(t('Bu biçim için 3B önizleme yok'));
  }

  if (signal?.aborted) throw new DOMException('aborted', 'AbortError');
  return { object, zUp, ownMaterial, lines, toolpath, palette, triangles: countTriangles(object), unit: GLTF.has(ext) || ext === 'fbx' ? '' : 'mm' };
}

export function disposeObject(obj, keepMaterial) {
  obj.traverse((o) => {
    o.geometry?.dispose?.();
    const mats = new Set([...(Array.isArray(o.material) ? o.material : o.material ? [o.material] : []), ...(o.userData.colorMaterials || [])]);
    for (const m of mats) {
      if (m === keepMaterial) continue;
      for (const v of Object.values(m)) if (v?.isTexture) v.dispose();
      m.dispose();
    }
  });
}
