#!/usr/bin/env bash
# macOS derlemesi: Formia.app + .dmg
# Gerekenler: Xcode Command Line Tools (xcode-select --install), Rust (rustup), Node.js (npx için).
set -euo pipefail
cd "$(dirname "$0")/src-tauri"

# src-tauri/.cargo/config.toml Windows'taki yerel klasörü gösterir; Mac'te ev dizinine yönlendir.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/model-gezgini-target}"
unset TAURI_CONFIG || true

# Apple Silicon + Intel için tek paket istenirse: ./build-mac.sh --target universal-apple-darwin
# (önce: rustup target add aarch64-apple-darwin x86_64-apple-darwin)
npx --yes @tauri-apps/cli@2 build --bundles app,dmg "$@"

echo
echo "Çıktılar:"
find "$CARGO_TARGET_DIR" -path '*release/bundle*' \( -name '*.dmg' -o -name '*.app' \) -maxdepth 6 -print
