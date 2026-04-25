# Web Converter

Static browser UI for converting MusicXML into Scorify-backed Typst and previewing the rendered document.

Serve this directory or the repository root so the app can load `vendor/converter` and the bundled local Scorify copy in `typst/scorify`:

```powershell
cd web-converter
python -m http.server 8787
```

Open:

```text
http://localhost:8787/
```

The page uses CDN-hosted Preact and Typst.ts browser modules, the local converter WASM package in `vendor/converter`, and the local Scorify Typst package in `typst/scorify`. Generated Typst is normalized to import `"/scorify/lib.typ"` from the local copy mounted into the in-browser Typst filesystem.
