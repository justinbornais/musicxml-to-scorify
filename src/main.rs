#[cfg(not(target_arch = "wasm32"))]
use std::{fs, path::PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use anyhow::{Context, Result};
#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
#[cfg(not(target_arch = "wasm32"))]
use musicxml_to_scorify::{ConversionOptions, convert_musicxml_file_to_scorify};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Parser)]
#[command(version, about = "Convert MusicXML into a Typst Scorify #score call")]
struct Cli {
    /// Input .musicxml/.xml file, or compressed .mxl file.
    input: PathBuf,

    /// Write output to a file instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

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

    let options = ConversionOptions {
        include_import: !cli.no_import,
        package: cli.package,
        measures_per_line: cli.measures_per_line,
        include_comment: cli.comment,
    };

    let filename = cli.input.to_string_lossy();
    let output = convert_musicxml_file_to_scorify(&bytes, &filename, &options)?;

    if let Some(path) = cli.output {
        fs::write(&path, output).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        print!("{output}");
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
