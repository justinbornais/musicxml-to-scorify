# Web Converter

Static browser UI for converting MusicXML into Scorify-backed Typst and previewing the rendered document.

Serve the repository root so the app can load both `web-converter/vendor/converter` and `typst/scorify`:

```powershell
python -m http.server 8787
```

Open:

```text
http://localhost:8787/web-converter/
```

The page uses CDN-hosted Preact and Typst.ts browser modules, the local converter WASM package in `vendor/converter`, and the local Scorify Typst package in `../typst/scorify`.
