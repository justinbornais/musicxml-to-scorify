# musicxml-to-scorify

Convert MusicXML files into Typst [Scorify](https://github.com/justinbornais/typst-sheet-music) `#score(...)` calls.

The converter is written in Rust. File handling lives in the CLI, while the conversion core accepts a `&str` and returns Typst text, which keeps the main path suitable for a future WASM wrapper.

## Usage

```powershell
cargo run -- samples/simple.musicxml
cargo run -- samples/grand-staff.musicxml --output out.typ
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
```

## Supported Conversion Surface

- Score metadata: title, composer, part names, abbreviations
- Score attributes: key signatures, time signatures, clefs, multi-staff parts
- Notes, rests, chords, dotted durations, ties, slurs, common articulations
- Two-voice measures via Scorify `v{upper;lower}`
- Lyrics, dynamics, staff text, and chord symbols
- Regular, double, final, forward-repeat, and backward-repeat barlines
- Plain `.musicxml`/`.xml` files and compressed `.mxl` archives

The Scorify output intentionally stays textual and editable. Unsupported MusicXML details are skipped rather than encoded as opaque comments.

## Library Entry Points

```rust
use musicxml_to_scorify::{convert_musicxml_to_scorify, ConversionOptions};

let typst = convert_musicxml_to_scorify(xml, &ConversionOptions::default())?;
```

For `wasm32` builds, the crate also exposes:

```rust
convert_musicxml_to_scorify_wasm(xml: &str) -> Result<String, JsValue>
```
