#!/usr/bin/env sh
set -eu

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen was not found. Install it with: cargo install wasm-bindgen-cli" >&2
  exit 1
fi

cargo build --release --target wasm32-unknown-unknown --lib

wasm-bindgen \
  --target web \
  --out-dir pkg \
  --out-name musicxml_to_scorify \
  target/wasm32-unknown-unknown/release/musicxml_to_scorify.wasm

echo "WASM package written to ./pkg"
