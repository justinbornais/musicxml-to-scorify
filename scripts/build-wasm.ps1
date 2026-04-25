$ErrorActionPreference = "Stop"

if (-not (Get-Command wasm-bindgen -ErrorAction SilentlyContinue)) {
    Write-Error "wasm-bindgen was not found. Install it with: cargo install wasm-bindgen-cli"
}

cargo build --release --target wasm32-unknown-unknown --lib

wasm-bindgen `
    --target web `
    --out-dir pkg `
    --out-name musicxml_to_scorify `
    target/wasm32-unknown-unknown/release/musicxml_to_scorify.wasm

Write-Host "WASM package written to ./pkg"
