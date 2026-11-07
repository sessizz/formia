// Runs OpenCascade (occt-import-js, WebAssembly) off the UI thread for STEP/IGES/BREP.
const base = new URL('../vendor/occt/', self.location.href).href;
importScripts(base + 'occt-import-js.js');

/* global occtimportjs */
const ready = occtimportjs({ locateFile: (name) => base + name });

self.onmessage = async (e) => {
  const { id, format, buffer } = e.data;
  try {
    const occt = await ready;
    const data = new Uint8Array(buffer);
    const params = { linearUnit: 'millimeter', linearDeflectionType: 'bounding_box_ratio', linearDeflection: 0.001, angularDeflection: 0.5 };
    const result = format === 'iges' ? occt.ReadIgesFile(data, params) : format === 'brep' ? occt.ReadBrepFile(data, params) : occt.ReadStepFile(data, params);
    self.postMessage({ id, result });
  } catch (err) {
    self.postMessage({ id, error: String(err?.message || err) });
  }
};
