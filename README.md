# Web Converter

Static browser UI for converting MusicXML into Scorify-backed Typst, converting Scorify back into MusicXML, and previewing the Scorify side of the score through Typst.

Serve this directory or the repository root so the app can load `vendor/converter` and the bundled local Scorify copy in `typst/scorify`:

```powershell
cd web-converter
python -m http.server 8787
```

Open:

```text
http://localhost:8787/
```

The page uses CDN-hosted Preact and Typst.ts browser modules, the local converter WASM package in `vendor/converter`, and the local Scorify Typst package in `typst/scorify`. Generated or loaded Scorify is normalized to import `"/scorify/lib.typ"` from the local copy mounted into the in-browser Typst filesystem.

Direction support:

- `MusicXML -> Scorify`: upload, paste, or fetch `.musicxml`, `.xml`, or `.mxl`; adjust `measures-per-line`; preview the generated Typst.
- `Scorify -> MusicXML`: upload, paste, or fetch Scorify `.typ` text; convert it back to MusicXML while previewing the Scorify source.
