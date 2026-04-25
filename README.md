# musicxml-to-scorify

Convert MusicXML files into Typst [Scorify](https://github.com/justinbornais/typst-sheet-music) `#score(...)` calls, and convert Scorify scores back into MusicXML.

The converter is written in Rust. The conversion core is shared by the CLI and the WASM build, so browser callers can convert pasted MusicXML text or uploaded `.musicxml`/`.xml`/`.mxl` files into Typst code, or turn Scorify input back into MusicXML.

## Usage

```powershell
cargo run -- samples/simple.musicxml
cargo run -- samples/grand-staff.musicxml --output out.typ
cargo run -- samples/Hear, O Lord.typ --from scorify --output out.musicxml
```

By default the output includes:

```typ
#import "@preview/scorify:0.3.0": score
```

Useful options:

```powershell
cargo run -- score.musicxml --no-import
cargo run -- score.musicxml --measures-per-line 4
cargo run -- score.mxl --output score.typ
cargo run -- score.typ --from scorify --output score.musicxml
```

`--from` accepts `auto`, `musicxml`, and `scorify`. `auto` is the default and detects common file extensions plus `#score(` or `<score-partwise` content.

## Supported Conversion Surface

- Score metadata: title, composer, part names, abbreviations
- Score attributes: key signatures, time signatures, clefs, multi-staff parts
- Notes, rests, chords, dotted durations, ties, slurs, common articulations
- Two-voice measures via Scorify `v{upper;lower}`
- Lyrics, dynamics, staff text, and chord symbols
- Regular, double, final, forward-repeat, and backward-repeat barlines
- Plain `.musicxml`/`.xml` files and compressed `.mxl` archives

The reverse Scorify parser targets the same score properties and music tokens that this project emits in the MusicXML -> Scorify direction, so round-tripping preserves the supported subset instead of attempting to parse arbitrary Typst programs.

The Scorify output intentionally stays textual and editable. Unsupported MusicXML details are skipped rather than encoded as opaque comments.

## Library Entry Points

```rust
use musicxml_to_scorify::{
    convert_musicxml_file_to_scorify,
    convert_musicxml_to_scorify,
  convert_scorify_file_to_musicxml,
  convert_scorify_to_musicxml,
    ConversionOptions,
};

let typst = convert_musicxml_to_scorify(xml, &ConversionOptions::default())?;
let typst_from_file = convert_musicxml_file_to_scorify(
    bytes,
    "score.mxl",
    &ConversionOptions::default(),
)?;
  let musicxml = convert_scorify_to_musicxml(typst_source)?;
  let musicxml_from_file = convert_scorify_file_to_musicxml(typst_bytes)?;
```

## WASM Build

Install the target and `wasm-bindgen` CLI once:

```powershell
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
```

Build the browser package:

```powershell
.\scripts\build-wasm.ps1
```

On macOS/Linux:

```sh
sh scripts/build-wasm.sh
```

This writes a browser-loadable package to `pkg/`, including `musicxml_to_scorify.js` and `musicxml_to_scorify_bg.wasm`.

The generated module exposes:

```js
convert_musicxml_text_wasm(xml)
convert_musicxml_text_with_options_wasm(xml, optionsJson)
convert_musicxml_file_wasm(bytes, filename, optionsJson)
convert_scorify_text_wasm(source)
convert_scorify_file_wasm(bytes)
```

`optionsJson` uses camelCase keys:

```json
{
  "includeImport": true,
  "package": "@preview/scorify:0.3.0",
  "measuresPerLine": 4,
  "includeComment": false
}
```

Browser example:

```js
import init, {
  convert_musicxml_file_wasm,
  convert_musicxml_text_with_options_wasm,
  convert_scorify_file_wasm,
  convert_scorify_text_wasm,
} from "./pkg/musicxml_to_scorify.js";

await init();

const typstFromText = convert_musicxml_text_with_options_wasm(
  musicXmlString,
  JSON.stringify({ includeImport: false }),
);

const bytes = new Uint8Array(await file.arrayBuffer());
const typstFromFile = convert_musicxml_file_wasm(
  bytes,
  file.name,
  JSON.stringify({ includeImport: true }),
);

const musicXmlFromText = convert_scorify_text_wasm(scorifySource);
const musicXmlFromFile = convert_scorify_file_wasm(bytes);
```

See `examples/browser.html` for a tiny file-upload example.

## Web Converter

The static browser app lives in `web-converter/`. It uses the generated converter WASM package, bundles the local Scorify Typst package at `web-converter/typst/scorify`, and renders Scorify documents through Typst.ts. The UI supports both MusicXML -> Scorify and Scorify -> MusicXML, and keeps the preview focused on the Scorify side of the conversion.

Serve the app directory:

```powershell
cd web-converter
python -m http.server 8787
```

Then open:

```text
http://localhost:8787/
```
