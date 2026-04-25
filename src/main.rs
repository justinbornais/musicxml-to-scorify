#[cfg(not(target_arch = "wasm32"))]
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

#[cfg(not(target_arch = "wasm32"))]
use anyhow::{Context, Result, anyhow};
#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
#[cfg(not(target_arch = "wasm32"))]
use musicxml_to_scorify::{ConversionOptions, convert_musicxml_to_scorify};

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
    let xml = read_musicxml_file(&cli.input)?;

    let options = ConversionOptions {
        include_import: !cli.no_import,
        package: cli.package,
        measures_per_line: cli.measures_per_line,
        include_comment: cli.comment,
    };

    let output = convert_musicxml_to_scorify(&xml, &options)?;

    if let Some(path) = cli.output {
        fs::write(&path, output).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        print!("{output}");
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn read_musicxml_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let is_mxl = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mxl"));

    if is_mxl {
        read_mxl(&bytes)
    } else {
        String::from_utf8(bytes).context("MusicXML file is not valid UTF-8")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_mxl(bytes: &[u8]) -> Result<String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).context("failed to open .mxl archive")?;

    let container_xml = match archive.by_name("META-INF/container.xml") {
        Ok(mut container) => {
            let mut container_xml = String::new();
            container
                .read_to_string(&mut container_xml)
                .context("failed to read META-INF/container.xml")?;
            Some(container_xml)
        }
        Err(_) => None,
    };

    if let Some(container_xml) = container_xml {
        if let Ok(doc) = roxmltree::Document::parse_with_options(
            container_xml.trim_start_matches('\u{feff}').trim_start(),
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..roxmltree::ParsingOptions::default()
            },
        ) {
            if let Some(full_path) = doc
                .descendants()
                .find(|node| node.has_tag_name("rootfile"))
                .and_then(|node| node.attribute("full-path"))
            {
                let mut score = archive.by_name(full_path).with_context(|| {
                    format!("container points to missing score file {full_path:?}")
                })?;
                let mut xml = String::new();
                score
                    .read_to_string(&mut xml)
                    .with_context(|| format!("failed to read score file {full_path:?}"))?;
                return Ok(xml);
            }
        }
    }

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".musicxml") || lower.ends_with(".xml") {
            let mut xml = String::new();
            file.read_to_string(&mut xml)
                .with_context(|| format!("failed to read score file {name:?}"))?;
            return Ok(xml);
        }
    }

    Err(anyhow!("no MusicXML score file found in .mxl archive"))
}
