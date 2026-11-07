// Inline stroke icons (24x24 grid) and larger two-tone file-type artwork.

const P = {
  back: '<path d="M19 12H5"/><path d="m12 19-7-7 7-7"/>',
  forward: '<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>',
  up: '<path d="M12 19V5"/><path d="m5 12 7-7 7 7"/>',
  refresh: '<path d="M21 12a9 9 0 1 1-2.64-6.36L21 8"/><path d="M21 3v5h-5"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  star: '<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2L12 17.3l-5.6 2.9 1.1-6.2L3 9.6l6.2-.9z"/>',
  grid: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.5"/>',
  list: '<path d="M8 6h13M8 12h13M8 18h13"/><path d="M3.5 6h.01M3.5 12h.01M3.5 18h.01"/>',
  settings: '<path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1"/><circle cx="15" cy="6" r="2"/><circle cx="9" cy="12" r="2"/><circle cx="17" cy="18" r="2"/>',
  close: '<path d="M18 6 6 18M6 6l12 12"/>',
  folder: '<path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2.2h7.5A2.5 2.5 0 0 1 21 9.7v7.8a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5z"/>',
  file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
  fileText: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5M9 13h6M9 17h4"/>',
  cube: '<path d="M12 2.8 3.8 7.4v9.2l8.2 4.6 8.2-4.6V7.4z"/><path d="m3.8 7.4 8.2 4.6 8.2-4.6M12 12v9.2"/>',
  wire: '<path d="M12 2.8 3.8 7.4v9.2l8.2 4.6 8.2-4.6V7.4z"/><path d="m3.8 7.4 8.2 4.6 8.2-4.6M12 12v9.2M8 5.1l8.2 4.6M16 5.1 7.9 9.7"/>',
  archive: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5M10 5v2m2 0v2m-2 0v2m2 0v2"/><rect x="9" y="14" width="4" height="4" rx="1"/>',
  image: '<rect x="3" y="3" width="18" height="18" rx="2.5"/><circle cx="9" cy="9" r="1.8"/><path d="m21 15-4.6-4.6L6 21"/>',
  layers: '<path d="m12 3 9 4.6-9 4.6-9-4.6z"/><path d="m3 12.2 9 4.6 9-4.6M3 16.6l9 4.6 9-4.6"/>',
  video: '<rect x="2.5" y="5.5" width="14" height="13" rx="2"/><path d="m16.5 10.2 5-3v9.6l-5-3"/>',
  pdf: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/><path d="M8.5 16.5c2-1 4.5-4.5 4-6.5-.4-1.6-2-.6-1.5 1.5.6 2.5 3 4.5 4.5 4.5"/>',
  drive: '<rect x="2.5" y="13" width="19" height="7.5" rx="2"/><path d="M5 13 7.4 4.5h9.2L19 13M6.5 16.8h.01M10 16.8h.01"/>',
  usb: '<rect x="6" y="9" width="12" height="12.5" rx="2"/><path d="M8.5 9V2.5h7V9M10.5 5.2h.01M13.5 5.2h.01"/>',
  network: '<rect x="2.5" y="3" width="19" height="7" rx="1.5"/><rect x="2.5" y="14" width="19" height="7" rx="1.5"/><path d="M6.5 6.5h.01M6.5 17.5h.01"/>',
  home: '<path d="m3 10.2 9-7 9 7V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z"/>',
  desktop: '<rect x="2.5" y="3.5" width="19" height="13" rx="2"/><path d="M8 20.5h8M12 16.5v4"/>',
  documents: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5M9 13h6M9 17h6"/>',
  downloads: '<path d="M12 3.5v12"/><path d="m7 10.5 5 5 5-5"/><path d="M4.5 20.5h15"/>',
  pictures: '<rect x="3" y="3" width="18" height="18" rx="2.5"/><circle cx="9" cy="9" r="1.8"/><path d="m21 15-4.6-4.6L6 21"/>',
  computer: '<rect x="2.5" y="3.5" width="19" height="13" rx="2"/><path d="M8 20.5h8M12 16.5v4"/>',
  external: '<path d="M14 4h6v6"/><path d="M10 14 20 4"/><path d="M19 14v4.5a1.5 1.5 0 0 1-1.5 1.5h-12A1.5 1.5 0 0 1 4 18.5v-12A1.5 1.5 0 0 1 5.5 5H10"/>',
  reveal: '<path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2.2h7.5A2.5 2.5 0 0 1 21 9.7v7.8a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5z"/><path d="M12 11v5M9.5 13.5 12 11l2.5 2.5"/>',
  copy: '<rect x="8.5" y="8.5" width="12" height="12" rx="2"/><path d="M15.5 8.5V5a1.5 1.5 0 0 0-1.5-1.5H5A1.5 1.5 0 0 0 3.5 5v9A1.5 1.5 0 0 0 5 15.5h3.5"/>',
  scan: '<path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2.2h7.5A2.5 2.5 0 0 1 21 9.7v1.8"/><path d="M3 7.5v10A2.5 2.5 0 0 0 5.5 20H11"/><circle cx="17" cy="16.5" r="3.2"/><path d="m21.5 21-2.2-2.2"/>',
  sortAsc: '<path d="M4 6h8M4 12h6M4 18h4"/><path d="M17 20V4M13.5 7.5 17 4l3.5 3.5"/>',
  sortDesc: '<path d="M4 6h4M4 12h6M4 18h8"/><path d="M17 4v16M13.5 16.5 17 20l3.5-3.5"/>',
  eye: '<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  focus: '<circle cx="12" cy="12" r="3"/><path d="M3 8.5V5a2 2 0 0 1 2-2h3.5M15.5 3H19a2 2 0 0 1 2 2v3.5M21 15.5V19a2 2 0 0 1-2 2h-3.5M8.5 21H5a2 2 0 0 1-2-2v-3.5"/>',
  floor: '<path d="M3 3h18v18H3zM3 9h18M3 15h18M9 3v18M15 3v18"/>',
  rotate: '<path d="M16.5 7.8c2.9.9 5 2.5 5 4.2 0 2.8-4.3 5-9.5 5S2.5 14.8 2.5 12 6.8 7 12 7"/><path d="m13 4 3 3.3-3 3"/>',
  sphere: '<circle cx="12" cy="12" r="9"/><path d="M3 12c0 2.2 4 4 9 4s9-1.8 9-4"/><path d="M12 3c-2.2 2.5-3 5.5-3 9s.8 6.5 3 9"/>',
  axis: '<path d="M5 19 5 5M5 19h14"/><path d="m2.5 7.5 2.5-2.5 2.5 2.5M16.5 16.5l2.5 2.5-2.5 2.5"/><path d="m5 19 7-7"/>',
  chevronLeft: '<path d="m15 18-6-6 6-6"/>',
  chevronRight: '<path d="m9 18 6-6-6-6"/>',
  chevronDown: '<path d="m6 9 6 6 6-6"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M5.5 7l1 12.5a1.5 1.5 0 0 0 1.5 1.5h8a1.5 1.5 0 0 0 1.5-1.5l1-12.5M9 7V4.5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1V7"/>',
  arrowUpDown: '<path d="m7 15 5 5 5-5M7 9l5-5 5 5"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>',
  ruler: '<path d="M3 17 17 3l4 4L7 21z"/><path d="m7 13 2 2M10 10l2 2M13 7l2 2"/>',
  palette: '<path d="M12 3a9 9 0 1 0 0 18c1.1 0 1.8-.8 1.8-1.8 0-.5-.2-.9-.5-1.2-.3-.3-.5-.8-.5-1.3 0-1 .8-1.8 1.8-1.8H16a5 5 0 0 0 5-5C21 6 17 3 12 3z"/><circle cx="7.5" cy="11.5" r="1.3"/><circle cx="10.5" cy="7.3" r="1.3"/><circle cx="15.5" cy="7.8" r="1.3"/>',
  rename: '<path d="M4 20h4L19 9a2.1 2.1 0 0 0-3-3L5 17z"/><path d="m14.5 7.5 3 3"/>',
  winMin: '<path d="M6.5 12h11"/>',
  winMax: '<rect x="6.5" y="6.5" width="11" height="11" rx="1"/>',
  winRestore: '<rect x="6.5" y="8.5" width="9" height="9" rx="1"/><path d="M9 8.5V7.2c0-.4.3-.7.7-.7h7.1c.4 0 .7.3.7.7v7.1c0 .4-.3.7-.7.7h-1.3"/>',
  winClose: '<path d="m7 7 10 10M17 7 7 17"/>',
};

export function icon(name, cls = '') {
  return `<svg class="ico ${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${P[name] || P.file}</svg>`;
}

// Large two-tone artwork used in place of thumbnails.
const BIG = {
  dir: '<path d="M8 19a5 5 0 0 1 5-5h11l6 7h21a5 5 0 0 1 5 5v23a5 5 0 0 1-5 5H13a5 5 0 0 1-5-5z" fill="color-mix(in srgb, var(--folder) 12%, transparent)" stroke="var(--folder)" stroke-width="2.6" stroke-linejoin="round"/>',
  archive: '<path d="M37 9H17a5 5 0 0 0-5 5v36a5 5 0 0 0 5 5h30a5 5 0 0 0 5-5V24z" fill="color-mix(in srgb, var(--archive) 12%, transparent)" stroke="var(--archive)" stroke-width="2.6" stroke-linejoin="round"/><path d="M37 9v11a4 4 0 0 0 4 4h11M27 12v5m4 0v5m-4 0v5m4 0v5" fill="none" stroke="var(--archive)" stroke-width="2.6" stroke-linejoin="round"/><rect x="25" y="35" width="8" height="10" rx="2" fill="none" stroke="var(--archive)" stroke-width="2.6"/><path d="M28 40h2" stroke="var(--archive)" stroke-width="2" stroke-linecap="round"/>',
  drive: '<rect x="8" y="30" width="48" height="20" rx="5" fill="var(--drive)"/><path d="M14 30 19 14h26l5 16z" fill="var(--drive-back)"/><circle cx="17" cy="40" r="2.2" fill="#4ade80"/><path d="M25 40h22" stroke="rgba(0,0,0,.3)" stroke-width="3" stroke-linecap="round"/>',
};

export function bigIcon(kind) {
  if (BIG[kind]) return `<svg class="big" viewBox="0 0 64 64" aria-hidden="true">${BIG[kind]}</svg>`;
  const map = { model: 'cube', gcode: 'layers', image: 'image', text: 'fileText', pdf: 'pdf', video: 'video', file: 'file' };
  return `<svg class="big stroke k-${kind}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${P[map[kind]] || P.file}</svg>`;
}

export function kindIcon(kind, cls = '') {
  const map = { dir: 'folder', archive: 'archive', model: 'cube', gcode: 'layers', image: 'image', text: 'fileText', pdf: 'pdf', video: 'video', drive: 'drive', file: 'file' };
  return icon(map[kind] || 'file', `k-${kind} ${cls}`);
}
