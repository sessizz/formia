# Third-party notices

Formia itself is MIT-licensed (see `LICENSE`). It ships the following libraries
unmodified in `ui/vendor/`, each under its own license:

| Library | Version | License | Files |
| --- | --- | --- | --- |
| [three.js](https://github.com/mrdoob/three.js) | r187 | MIT | `ui/vendor/three/` (license: `ui/vendor/three/LICENSE`) |
| [occt-import-js](https://github.com/kovacsv/occt-import-js) | | LGPL-2.1 | `ui/vendor/occt/occt-import-js.js`, `.wasm` (license: `ui/vendor/occt/license.occt-import-js.txt`) |
| [Open CASCADE Technology](https://dev.opencascade.org/) (inside occt-import-js) | | LGPL-2.1 with the OCCT exception | license: `ui/vendor/occt/license.occt.txt` |

occt-import-js / Open CASCADE are used as a separate, replaceable WebAssembly
module, loaded at run time by `ui/js/occt-worker.js`; you may swap in your own
build of it. Rust crates used by the backend are listed in `src-tauri/Cargo.toml`
and `src-tauri/Cargo.lock` with their own (permissive) licenses.
