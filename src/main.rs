#[cfg(not(target_arch = "wasm32"))]
use std::{fs, path::PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use anyhow::{Context, Result};
#[cfg(not(target_arch = "wasm32"))]
use clap::{Parser, ValueEnum};
#[cfg(not(target_arch = "wasm32"))]
use musicxml_to_scorify::{
    ConversionOptions, convert_musicxml_file_to_scorify, convert_scorify_file_to_musicxml,
};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, ValueEnum)]
enum InputFormat {
    Auto,
    Musicxml,
    Scorify,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Parser)]
#[command(version, about = "Convert MusicXML to Scorify or Scorify back to MusicXML")]
struct Cli {
    /// Input .musicxml/.xml file, or compressed .mxl file.
    input: PathBuf,

    /// Write output to a file instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Input format. Auto-detects by extension and content by default.
    #[arg(long, value_enum, default_value_t = InputFormat::Auto)]
    from: InputFormat,

    /// Emit only the #score(...) call, without an import line.
    #[arg(long)]
    no_import: bool,

    /// Typst package import target.
    #[arg(long, default_value = "@preview/scorify:0.3.0")]
    package: String,

    /// Force Scorify's measures-per-line parameter.
    #[arg(long)]
    measures_per_line: Option<u32>,

    /// Include a small conversion comment at the top of the output.
    #[arg(long)]
    comment: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    let cli = Cli::parse();
    let bytes =
        fs::read(&cli.input).with_context(|| format!("failed to read {}", cli.input.display()))?;

    let format = detect_input_format(&cli.input, &bytes, cli.from)?;
    let output = match format {
        InputFormat::Musicxml => {
            let options = ConversionOptions {
                include_import: !cli.no_import,
                package: cli.package,
                measures_per_line: cli.measures_per_line,
                include_comment: cli.comment,
            };
            let filename = cli.input.to_string_lossy();
            convert_musicxml_file_to_scorify(&bytes, &filename, &options)?
        }
        InputFormat::Scorify => convert_scorify_file_to_musicxml(&bytes)?,
        InputFormat::Auto => unreachable!(),
    };

    if let Some(path) = cli.output {
        fs::write(&path, output).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        print!("{output}");
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_input_format(path: &PathBuf, bytes: &[u8], requested: InputFormat) -> Result<InputFormat> {
    if !matches!(requested, InputFormat::Auto) {
        return Ok(requested);
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());

    if matches!(extension.as_deref(), Some("musicxml" | "xml" | "mxl")) {
        return Ok(InputFormat::Musicxml);
    }

    if matches!(extension.as_deref(), Some("typ" | "scorify" | "txt")) {
        return Ok(InputFormat::Scorify);
    }

    if bytes.starts_with(b"PK\x03\x04") {
        return Ok(InputFormat::Musicxml);
    }

    let text = String::from_utf8_lossy(bytes);
    if text.contains("<score-partwise") {
        return Ok(InputFormat::Musicxml);
    }
    if text.contains("#score(") {
        return Ok(InputFormat::Scorify);
    }

    anyhow::bail!(
        "could not determine input format for {}; use --from musicxml or --from scorify",
        path.display()
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}
