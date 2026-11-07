// Thumbnails for formats only the web view can read (FBX, Collada, 3DS, AMF,
// VRML, STEP/IGES). Rendered one at a time offscreen, then cached by Rust.
import * as THREE from 'three';
import { api, thumbUrl } from './api.js';
import { loadModel, disposeObject } from './loaders.js';

const SIZE = 384;
const AZ = THREE.MathUtils.degToRad(35);
const EL = THREE.MathUtils.degToRad(24);

let ctx = null;
function setup() {
  if (ctx) return ctx;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = SIZE;
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(1);
  renderer.setSize(SIZE, SIZE, false);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.setClearColor(0x000000, 0);
  const scene = new THREE.Scene();
  const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.01, 1000);
  scene.add(camera);
  const head = new THREE.DirectionalLight(0xffffff, 1.6);
  head.position.set(-0.6, 0.8, 1);
  head.target.position.set(0, 0, -1);
  camera.add(head, head.target);
  scene.add(new THREE.HemisphereLight(0xffffff, 0x30343c, 1.2));
  const material = new THREE.MeshStandardMaterial({ color: 0xb4bfcd, roughness: 0.55, metalness: 0.05, flatShading: true, side: THREE.DoubleSide });
  ctx = { canvas, renderer, scene, camera, material };
  return ctx;
}

async function render(item) {
  const { canvas, renderer, scene, camera, material } = setup();
  const res = await loadModel(item.path, item.ext, material);
  const pivot = new THREE.Group();
  pivot.add(res.object);
  if (res.zUp) pivot.rotation.x = -Math.PI / 2;
  scene.add(pivot);
  try {
    pivot.updateMatrixWorld(true);
    const box = new THREE.Box3().setFromObject(pivot);
    if (box.isEmpty()) throw new Error('boş model');
    const center = box.getCenter(new THREE.Vector3());
    const radius = box.getSize(new THREE.Vector3()).length() / 2 || 1;
    const dir = new THREE.Vector3(Math.sin(AZ) * Math.cos(EL), Math.sin(EL), Math.cos(AZ) * Math.cos(EL));
    camera.position.copy(center).addScaledVector(dir, radius * 4);
    camera.lookAt(center);
    camera.updateMatrixWorld(true);
    // Fit the orthographic frustum to the projected bounding box corners.
    const inv = camera.matrixWorldInverse;
    let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity;
    for (let i = 0; i < 8; i++) {
      const p = new THREE.Vector3(i & 1 ? box.max.x : box.min.x, i & 2 ? box.max.y : box.min.y, i & 4 ? box.max.z : box.min.z).applyMatrix4(inv);
      x0 = Math.min(x0, p.x); x1 = Math.max(x1, p.x); y0 = Math.min(y0, p.y); y1 = Math.max(y1, p.y);
    }
    const half = (Math.max(x1 - x0, y1 - y0) / 2) * 1.08;
    const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
    Object.assign(camera, { left: cx - half, right: cx + half, top: cy + half, bottom: cy - half, near: 0.001 * radius, far: radius * 10 });
    camera.updateProjectionMatrix();
    renderer.render(scene, camera);
    const blob = await new Promise((r) => canvas.toBlob(r, 'image/png'));
    const bytes = new Uint8Array(await blob.arrayBuffer());
    let bin = '';
    for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
    return btoa(bin);
  } finally {
    scene.remove(pivot);
    disposeObject(res.object, material);
  }
}

const waiting = new Map(); // path -> { item, cbs:Set }
const order = []; // LIFO of paths
let busy = false;

export function requestClientThumb(item, cb) {
  let w = waiting.get(item.path);
  if (!w) {
    w = { item, cbs: new Set() };
    waiting.set(item.path, w);
  }
  w.cbs.add(cb);
  const i = order.indexOf(item.path);
  if (i >= 0) order.splice(i, 1);
  order.push(item.path);
  pump();
}

export function cancelClientThumb(path, cb) {
  const w = waiting.get(path);
  if (!w) return;
  w.cbs.delete(cb);
}

async function pump() {
  if (busy) return;
  busy = true;
  try {
    while (order.length) {
      const path = order.pop();
      const w = waiting.get(path);
      if (!w) continue;
      if (!w.cbs.size) {
        waiting.delete(path);
        continue;
      }
      let url = null;
      try {
        const state = await api.thumbState(path);
        if (state === 'hit') url = thumbUrl(w.item);
        else if (state === 'miss') {
          try {
            await api.thumbStore(path, await render(w.item));
            url = thumbUrl(w.item);
          } catch (e) {
            console.warn('thumbnail failed', path, e);
            await api.thumbStore(path, null);
          }
        }
      } catch (e) {
        console.warn('thumb state failed', path, e);
      }
      waiting.delete(path);
      for (const cb of w.cbs) cb(url);
      await new Promise((r) => setTimeout(r, 0));
    }
  } finally {
    busy = false;
  }
}
