// Interactive model viewer used by the Quick Look preview.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';
import { mergeVertices } from 'three/addons/utils/BufferGeometryUtils.js';
import { loadModel, disposeObject, readableColor, softenWhite } from './loaders.js';

const AZ = THREE.MathUtils.degToRad(35);
const EL = THREE.MathUtils.degToRad(24);

function niceStep(v) {
  const p = Math.pow(10, Math.floor(Math.log10(v)));
  const n = v / p;
  return (n < 2 ? 1 : n < 5 ? 2 : 5) * p;
}

function shadowTexture() {
  const c = document.createElement('canvas');
  c.width = c.height = 128;
  const g = c.getContext('2d');
  const grd = g.createRadialGradient(64, 64, 0, 64, 64, 64);
  grd.addColorStop(0, 'rgba(0,0,0,0.55)');
  grd.addColorStop(0.55, 'rgba(0,0,0,0.18)');
  grd.addColorStop(1, 'rgba(0,0,0,0)');
  g.fillStyle = grd;
  g.fillRect(0, 0, 128, 128);
  const t = new THREE.CanvasTexture(c);
  t.colorSpace = THREE.SRGBColorSpace;
  return t;
}

export class Viewer3D {
  constructor(host, { color = '#a7b6c8' } = {}) {
    this.host = host;
    const r = (this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'high-performance' }));
    r.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    r.outputColorSpace = THREE.SRGBColorSpace;
    r.toneMapping = THREE.NeutralToneMapping ?? THREE.ACESFilmicToneMapping;
    r.setClearColor(0x000000, 0);
    r.domElement.className = 'viewer-canvas';
    host.append(r.domElement);

    this.scene = new THREE.Scene();
    const pmrem = new THREE.PMREMGenerator(r);
    this.scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
    this.scene.environmentIntensity = 0.45;

    this.camera = new THREE.PerspectiveCamera(32, 1, 0.01, 1e7);
    this.scene.add(this.camera);
    const head = new THREE.DirectionalLight(0xffffff, 1.5);
    head.position.set(-0.6, 0.8, 1);
    head.target.position.set(0, 0, -1);
    this.camera.add(head, head.target);
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x2a2e36, 1.1));
    const key = new THREE.DirectionalLight(0xffffff, 0.7);
    key.position.set(2, 3, 1.5);
    this.scene.add(key);

    this.controls = new OrbitControls(this.camera, r.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.12;
    this.controls.screenSpacePanning = true;
    this.controls.autoRotateSpeed = 1.6;
    this.controls.addEventListener('change', () => (this.dirty = true));

    this.pivot = new THREE.Group();
    this.scene.add(this.pivot);
    this.extras = new THREE.Group();
    this.scene.add(this.extras);

    this.material = new THREE.MeshStandardMaterial({ color: new THREE.Color(color), roughness: 0.55, metalness: 0.06, flatShading: true, side: THREE.DoubleSide });
    this.baseHex = `#${this.material.color.getHexString()}`;
    // Colour-menu overrides (view only), keyed by original colour so they carry over
    // to every plate of a project; the preview clears them when the file changes.
    this.overrides = new Map();
    this.origColors = new WeakMap();
    this.shadowTex = shadowTexture();
    this.opts = { wire: false, grid: true, smooth: false, autoRotate: false, colors: true };
    this.gridColors = [0x3a3f4a, 0x2a2e36];
    this.current = null;
    this.zUp = false;
    this.info = null;
    this.running = false;
    this.dirty = true;

    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(host);
    this.resize();
  }

  setTheme(dark) {
    this.gridColors = dark ? [0x434957, 0x2b2f38] : [0xb9c0cb, 0xd9dde3];
    this.buildExtras();
  }

  setColor(hex) {
    this.overrides.delete(this.baseHex);
    this.material.color.set(hex);
    this.baseHex = `#${this.material.color.getHexString()}`;
    this.origColors.delete(this.material);
    this.dirty = true;
  }

  /**
   * Colour groups of the current model for the colour menu, one per distinct
   * original colour: filament colours, CAD/glTF material colours, or the single
   * preview colour. Textured materials and G-code lines are left out.
   * @returns {{original: string, current: string|null, materials: THREE.Material[]}[]}
   */
  colorSlots() {
    if (!this.current || this.current.lines) return [];
    const slots = new Map();
    this.current.object.traverse((o) => {
      if (!o.isMesh) return;
      for (const m of Array.isArray(o.material) ? o.material : [o.material]) {
        if (!m?.color || m.map) continue;
        const orig = this.origColors.get(m) || m.color;
        const hex = (m === this.material ? this.baseHex : m.userData.fileHex || `#${orig.getHexString()}`).toLowerCase();
        let s = slots.get(hex);
        if (!s) slots.set(hex, (s = { original: hex, current: null, materials: [] }));
        if (!s.materials.includes(m)) s.materials.push(m);
      }
    });
    for (const s of slots.values()) s.current = this.overrides.get(s.original) || null;
    return [...slots.values()];
  }

  /** Recolours one slot from colorSlots() for viewing; `null` restores the original. */
  setSlotColor(slot, hex) {
    if (hex) this.overrides.set(slot.original, hex.toLowerCase());
    else this.overrides.delete(slot.original);
    this.paintSlot(slot, hex);
  }

  paintSlot(slot, hex) {
    for (const m of slot.materials) {
      if (!this.origColors.has(m)) this.origColors.set(m, m.color.clone());
      const picked = hex && readableColor(new THREE.Color(hex));
      if (picked && this.current?.toolpath) softenWhite(picked);
      m.color.copy(picked || this.origColors.get(m));
    }
    this.dirty = true;
  }

  /** Forgets every colour override (a different file was opened). */
  resetColors() {
    this.overrides.clear();
    this.restoreBaseColor();
    for (const s of this.colorSlots()) this.paintSlot(s, null);
  }

  /** Undoes a colour-menu override of the shared preview material (other materials die with the model). */
  restoreBaseColor() {
    const orig = this.origColors.get(this.material);
    if (orig) this.material.color.copy(orig);
    this.origColors.delete(this.material);
  }

  resize() {
    const w = this.host.clientWidth || 1;
    const h = this.host.clientHeight || 1;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.dirty = true;
  }

  start() {
    if (this.running) return;
    this.running = true;
    const tick = () => {
      if (!this.running) return;
      requestAnimationFrame(tick);
      const moved = this.controls.update();
      if (moved || this.dirty || this.controls.autoRotate) {
        this.renderer.render(this.scene, this.camera);
        this.dirty = false;
      }
    };
    tick();
  }

  stop() {
    this.running = false;
  }

  clear() {
    if (this.current) {
      this.pivot.remove(this.current.object);
      disposeObject(this.current.object, this.material);
      this.current = null;
    }
    this.restoreBaseColor();
    this.extras.clear();
    this.info = null;
    this.dirty = true;
  }

  /** `opts.plate` picks one build plate of a multi-plate 3MF. */
  async load(item, signal, opts) {
    this.clear();
    const res = await loadModel(item.path, item.ext, this.material, signal, opts);
    this.current = res;
    this.zUp = res.zUp;
    // Thousands of thin ribbon facets saturate under the model lighting: expose
    // G-code prints a little darker so the layer lines read, as in slicer previews.
    this.renderer.toneMappingExposure = res.toolpath ? 0.7 : 1;
    this.pivot.add(res.object);
    this.applyMaterialOptions();
    // Colour-menu picks made on another plate of the same project.
    for (const s of this.colorSlots()) if (s.current) this.paintSlot(s, s.current);
    this.layout();
    this.fit();
    return this.info;
  }

  /** Puts the model on the floor, centred, honouring the up axis. */
  layout() {
    if (!this.current) return;
    this.pivot.rotation.set(this.zUp ? -Math.PI / 2 : 0, 0, 0);
    this.pivot.position.set(0, 0, 0);
    this.pivot.updateMatrixWorld(true);
    const box = new THREE.Box3().setFromObject(this.pivot);
    if (box.isEmpty()) throw new Error('Model boş görünüyor');
    const c = box.getCenter(new THREE.Vector3());
    this.pivot.position.set(-c.x, -box.min.y, -c.z);
    this.pivot.updateMatrixWorld(true);
    this.box = new THREE.Box3().setFromObject(this.pivot);
    const s = this.box.getSize(new THREE.Vector3());
    this.info = { x: s.x, y: s.z, z: s.y, triangles: this.current.triangles, unit: this.current.unit, zUp: this.zUp, lines: this.current.lines, palette: this.current.palette };
    this.buildExtras();
    this.dirty = true;
  }

  buildExtras() {
    this.extras.clear();
    if (!this.box) return;
    const s = this.box.getSize(new THREE.Vector3());
    const span = Math.max(s.x, s.z, s.y * 0.6, 1e-6);
    const shadow = new THREE.Mesh(
      new THREE.PlaneGeometry(1, 1),
      new THREE.MeshBasicMaterial({ map: this.shadowTex, transparent: true, depthWrite: false, opacity: 0.8 })
    );
    shadow.rotation.x = -Math.PI / 2;
    shadow.scale.set(s.x * 1.5 + span * 0.1, s.z * 1.5 + span * 0.1, 1);
    shadow.position.y = span * 0.0005;
    shadow.renderOrder = -1;
    this.extras.add(shadow);
    if (this.opts.grid) {
      const cell = niceStep(span / 8);
      const size = Math.ceil((span * 2.2) / cell) * cell;
      const div = Math.min(400, Math.round(size / cell));
      const grid = new THREE.GridHelper(size, div, this.gridColors[0], this.gridColors[1]);
      grid.material.transparent = true;
      grid.material.opacity = 0.7;
      grid.material.depthWrite = false;
      this.extras.add(grid);
    }
    this.dirty = true;
  }

  fit() {
    if (!this.box) return;
    const sphere = this.box.getBoundingSphere(new THREE.Sphere());
    const r = Math.max(sphere.radius, 1e-6);
    const dir = new THREE.Vector3(Math.sin(AZ) * Math.cos(EL), Math.sin(EL), Math.cos(AZ) * Math.cos(EL));
    // Smallest distance at which every bounding-box corner fits the frustum.
    const right = new THREE.Vector3().crossVectors(new THREE.Vector3(0, 1, 0), dir).normalize();
    const up = new THREE.Vector3().crossVectors(dir, right);
    const tanV = Math.tan(THREE.MathUtils.degToRad(this.camera.fov) / 2) * 0.84;
    const tanH = tanV * this.camera.aspect;
    let dist = 0;
    const q = new THREE.Vector3();
    for (let i = 0; i < 8; i++) {
      q.set(i & 1 ? this.box.max.x : this.box.min.x, i & 2 ? this.box.max.y : this.box.min.y, i & 4 ? this.box.max.z : this.box.min.z).sub(sphere.center);
      const depth = q.dot(dir);
      dist = Math.max(dist, depth + Math.abs(q.dot(right)) / tanH, depth + Math.abs(q.dot(up)) / tanV);
    }
    dist = Math.max(dist, r * 0.5);
    this.camera.near = dist / 500;
    this.camera.far = dist * 200;
    this.camera.updateProjectionMatrix();
    this.camera.position.copy(sphere.center).addScaledVector(dir, dist);
    this.controls.target.copy(sphere.center);
    this.controls.minDistance = r * 0.05;
    this.controls.maxDistance = dist * 20;
    this.controls.update();
    this.dirty = true;
  }

  toggleUpAxis() {
    this.zUp = !this.zUp;
    this.layout();
    this.fit();
    return this.info;
  }

  set(opt, value) {
    this.opts[opt] = value;
    if (opt === 'grid') this.buildExtras();
    if (opt === 'autoRotate') this.controls.autoRotate = value;
    if (opt === 'wire' || opt === 'smooth' || opt === 'colors') this.applyMaterialOptions();
    this.dirty = true;
  }

  applyMaterialOptions() {
    if (!this.current) return;
    const { wire, smooth, colors } = this.opts;
    this.current.object.traverse((o) => {
      if (!o.isMesh) return;
      if (o.userData.own) {
        // Filament colours (one material per geometry group) or the uniform preview colour.
        o.material = colors && o.userData.colorMaterials ? o.userData.colorMaterials : this.material;
      }
      if (o.userData.own && smooth && !o.geometry.attributes.normal) {
        // Triangle soups (STL) need shared vertices before normals can be smooth.
        if (!o.geometry.index && o.geometry.attributes.position.count < 9e6) {
          const merged = mergeVertices(o.geometry, 1e-5);
          o.geometry.dispose();
          o.geometry = merged;
        }
        o.geometry.computeVertexNormals();
      }
      for (const m of Array.isArray(o.material) ? o.material : [o.material]) {
        m.wireframe = wire;
        if (m === this.material || m.userData.filament) {
          m.flatShading = !smooth;
        }
        m.needsUpdate = true;
      }
    });
    this.dirty = true;
  }

  dispose() {
    this.stop();
    this.clear();
    this.ro.disconnect();
    this.controls.dispose();
    this.renderer.dispose();
  }
}
