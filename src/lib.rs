use std::{
    collections::{BTreeMap, HashMap},
    io::{Cursor, Read},
    string::FromUtf8Error,
};

use roxmltree::{Document, Node, ParsingOptions};
use serde::Deserialize;
use thiserror::Error;

mod scorify_to_musicxml;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
pub struct ConversionOptions {
    pub include_import: bool,
    pub package: String,
    pub measures_per_line: Option<u32>,
    pub include_comment: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversionOptionsPatch {
    include_import: Option<bool>,
    package: Option<String>,
    measures_per_line: Option<u32>,
    include_comment: Option<bool>,
}

impl ConversionOptions {
    fn apply_patch(&self, patch: ConversionOptionsPatch) -> Self {
        Self {
            include_import: patch.include_import.unwrap_or(self.include_import),
            package: patch.package.unwrap_or_else(|| self.package.clone()),
            measures_per_line: patch.measures_per_line.or(self.measures_per_line),
            include_comment: patch.include_comment.unwrap_or(self.include_comment),
        }
    }
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            include_import: true,
            package: "@preview/scorify:0.3.0".to_string(),
            measures_per_line: None,
            include_comment: false,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("invalid MusicXML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("MusicXML file is not valid UTF-8: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("Scorify input is not valid UTF-8: {0}")]
    ScorifyUtf8(FromUtf8Error),
    #[error("failed to read compressed .mxl archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("failed to read compressed .mxl archive entry: {0}")]
    Io(#[from] std::io::Error),
    #[error("no MusicXML score file found in .mxl archive")]
    NoScoreInArchive,
    #[error("invalid conversion options JSON: {0}")]
    OptionsJson(#[from] serde_json::Error),
    #[error("MusicXML document does not contain a score-partwise root")]
    MissingScore,
    #[error("MusicXML score contains no parts")]
    NoParts,
    #[error("Scorify input does not contain a #score(...) call")]
    MissingScorifyScore,
    #[error("invalid Scorify input: {0}")]
    InvalidScorify(String),
}

pub fn convert_musicxml_to_scorify(
    xml: &str,
    options: &ConversionOptions,
) -> Result<String, ConvertError> {
    let score = parse_musicxml(xml)?;
    Ok(emit_typst(&score, options))
}

pub fn convert_musicxml_file_to_scorify(
    bytes: &[u8],
    filename: &str,
    options: &ConversionOptions,
) -> Result<String, ConvertError> {
    let xml = read_musicxml_bytes(bytes, filename)?;
    convert_musicxml_to_scorify(&xml, options)
}

pub fn convert_scorify_to_musicxml(source: &str) -> Result<String, ConvertError> {
    scorify_to_musicxml::convert_scorify_to_musicxml(source)
}

pub fn convert_scorify_file_to_musicxml(bytes: &[u8]) -> Result<String, ConvertError> {
    let source = String::from_utf8(bytes.to_vec()).map_err(ConvertError::ScorifyUtf8)?;
    convert_scorify_to_musicxml(&source)
}

pub fn read_musicxml_bytes(bytes: &[u8], filename: &str) -> Result<String, ConvertError> {
    if is_mxl_file(bytes, filename) {
        read_mxl(bytes)
    } else {
        String::from_utf8(bytes.to_vec()).map_err(ConvertError::Utf8)
    }
}

pub fn conversion_options_from_json(options_json: &str) -> Result<ConversionOptions, ConvertError> {
    if options_json.trim().is_empty() {
        return Ok(ConversionOptions::default());
    }

    let patch: ConversionOptionsPatch = serde_json::from_str(options_json)?;
    Ok(ConversionOptions::default().apply_patch(patch))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn convert_musicxml_to_scorify_wasm(xml: &str) -> Result<String, JsValue> {
    convert_musicxml_text_wasm(xml)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn convert_musicxml_text_wasm(xml: &str) -> Result<String, JsValue> {
    convert_musicxml_to_scorify(xml, &ConversionOptions::default()).map_err(js_error)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn convert_musicxml_text_with_options_wasm(
    xml: &str,
    options_json: &str,
) -> Result<String, JsValue> {
    let options = conversion_options_from_json(options_json).map_err(js_error)?;
    convert_musicxml_to_scorify(xml, &options).map_err(js_error)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn convert_musicxml_file_wasm(
    bytes: &[u8],
    filename: &str,
    options_json: &str,
) -> Result<String, JsValue> {
    let options = conversion_options_from_json(options_json).map_err(js_error)?;
    convert_musicxml_file_to_scorify(bytes, filename, &options).map_err(js_error)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn convert_scorify_to_musicxml_wasm(source: &str) -> Result<String, JsValue> {
    convert_scorify_to_musicxml(source).map_err(js_error)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn convert_scorify_text_wasm(source: &str) -> Result<String, JsValue> {
    convert_scorify_to_musicxml(source).map_err(js_error)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn convert_scorify_file_wasm(bytes: &[u8]) -> Result<String, JsValue> {
    convert_scorify_file_to_musicxml(bytes).map_err(js_error)
}

#[cfg(target_arch = "wasm32")]
fn js_error(error: ConvertError) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Score {
    title: Option<String>,
    composer: Option<String>,
    key: Option<String>,
    time: Option<String>,
    staves: Vec<Staff>,
}

#[derive(Debug, Clone)]
pub(crate) struct Staff {
    clef: String,
    music: String,
    instrument_name: Option<String>,
    instrument_name_cont: Option<String>,
    brace_start: bool,
    brace_end: bool,
    bracket_start: bool,
    bracket_end: bool,
}

#[derive(Debug, Clone)]
struct PartInfo {
    id: String,
    name: Option<String>,
    abbreviation: Option<String>,
}

#[derive(Debug, Clone)]
struct PartState {
    divisions: i32,
    key: Option<String>,
    time: Option<String>,
    staves: usize,
    clefs: BTreeMap<usize, String>,
}

impl Default for PartState {
    fn default() -> Self {
        let mut clefs = BTreeMap::new();
        clefs.insert(1, "treble".to_string());
        Self {
            divisions: 1,
            key: None,
            time: None,
            staves: 1,
            clefs,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MeasureAcc {
    voices: BTreeMap<String, Vec<TimedEvent>>,
    controls: Vec<TimedEvent>,
}

#[derive(Debug, Clone)]
struct MeasureResult {
    staves: BTreeMap<usize, MeasureAcc>,
    barline_before: Option<String>,
    barline_after: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TimedEvent {
    start: i32,
    duration: i32,
    token: Token,
}

#[derive(Debug, Clone)]
pub(crate) enum Token {
    Note(NoteToken),
    Rest,
    Clef(String),
    Time(String),
}

#[derive(Debug, Clone)]
pub(crate) struct NoteToken {
    clef: String,
    pitches: Vec<PitchToken>,
    duration_text: Option<String>,
    dots: usize,
    tie_start: bool,
    tie_stop: bool,
    slur_start: bool,
    slur_stop: bool,
    dynamic: Option<String>,
    chord_symbol: Option<String>,
    staff_text: Option<String>,
    lyrics: Vec<LyricToken>,
    articulations: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub(crate) struct PitchToken {
    step: char,
    alter: i32,
    accidental_text: Option<String>,
    octave: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct LyricToken {
    verse: Option<u32>,
    text: String,
}

#[derive(Debug, Clone, Default)]
struct PendingAttachment {
    dynamic: Option<String>,
    staff_text: Option<String>,
    chord_symbol: Option<String>,
}

fn parse_musicxml(xml: &str) -> Result<Score, ConvertError> {
    let doc = parse_musicxml_document(xml)?;
    let root = doc
        .descendants()
        .find(|node| node.has_tag_name("score-partwise"))
        .ok_or(ConvertError::MissingScore)?;

    let part_infos = collect_part_infos(root);
    let part_by_id: HashMap<&str, &PartInfo> = part_infos
        .iter()
        .map(|part| (part.id.as_str(), part))
        .collect();

    let mut score = Score {
        title: first_child_text(root, "movement-title")
            .or_else(|| first_descendant_text(root, "work-title")),
        composer: root
            .descendants()
            .find(|node| {
                node.has_tag_name("creator")
                    && node
                        .attribute("type")
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("composer"))
            })
            .and_then(node_text),
        ..Score::default()
    };

    let parts: Vec<_> = root
        .children()
        .filter(|node| node.is_element() && node.has_tag_name("part"))
        .collect();

    if parts.is_empty() {
        return Err(ConvertError::NoParts);
    }

    for part_node in parts {
        let part_id = part_node.attribute("id").unwrap_or("");
        let part_info = part_by_id.get(part_id).copied();
        let mut state = PartState::default();
        let part_staves = parse_part(part_node, &mut state);

        let is_grand_staff = part_staves.len() == 2
            && matches!(part_staves.first().map(|s| s.clef.as_str()), Some("treble"))
            && matches!(part_staves.get(1).map(|s| s.clef.as_str()), Some("bass"));

        for (index, mut staff) in part_staves.into_iter().enumerate() {
            if index == 0 {
                staff.instrument_name = part_info.and_then(|p| p.name.clone());
                staff.instrument_name_cont = part_info.and_then(|p| p.abbreviation.clone());
            }
            if is_grand_staff {
                staff.brace_start = index == 0;
                staff.brace_end = index == 1;
            } else if state.staves > 1 {
                staff.bracket_start = index == 0;
                staff.bracket_end = index + 1 == state.staves;
            }
            score.staves.push(staff);
        }

        if score.key.is_none() {
            score.key = state.key.clone();
        }
        if score.time.is_none() {
            score.time = state.time.clone();
        }
    }

    Ok(score)
}

fn parse_musicxml_document(xml: &str) -> Result<Document<'_>, roxmltree::Error> {
    Document::parse_with_options(
        xml.trim_start_matches('\u{feff}').trim_start(),
        ParsingOptions {
            allow_dtd: true,
            ..ParsingOptions::default()
        },
    )
}

fn is_mxl_file(bytes: &[u8], filename: &str) -> bool {
    filename
        .rsplit('.')
        .next()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mxl"))
        || bytes.starts_with(b"PK\x03\x04")
}

fn read_mxl(bytes: &[u8]) -> Result<String, ConvertError> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;

    let container_xml = match archive.by_name("META-INF/container.xml") {
        Ok(mut container) => {
            let mut container_xml = String::new();
            container.read_to_string(&mut container_xml)?;
            Some(container_xml)
        }
        Err(_) => None,
    };

    if let Some(container_xml) = container_xml {
        if let Ok(doc) = parse_musicxml_document(&container_xml) {
            if let Some(full_path) = doc
                .descendants()
                .find(|node| node.has_tag_name("rootfile"))
                .and_then(|node| node.attribute("full-path"))
            {
                let mut score = archive.by_name(full_path)?;
                let mut xml = String::new();
                score.read_to_string(&mut xml)?;
                return Ok(xml);
            }
        }
    }

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let lower = file.name().to_ascii_lowercase();
        if lower.ends_with(".musicxml") || lower.ends_with(".xml") {
            let mut xml = String::new();
            file.read_to_string(&mut xml)?;
            return Ok(xml);
        }
    }

    Err(ConvertError::NoScoreInArchive)
}

fn collect_part_infos(root: Node<'_, '_>) -> Vec<PartInfo> {
    root.children()
        .find(|node| node.has_tag_name("part-list"))
        .into_iter()
        .flat_map(|part_list| {
            part_list
                .children()
                .filter(|node| node.is_element() && node.has_tag_name("score-part"))
        })
        .map(|part| PartInfo {
            id: part.attribute("id").unwrap_or("").to_string(),
            name: first_child_text(part, "part-name"),
            abbreviation: first_child_text(part, "part-abbreviation"),
        })
        .collect()
}

fn parse_part(part_node: Node<'_, '_>, state: &mut PartState) -> Vec<Staff> {
    let mut staff_music: BTreeMap<usize, Vec<(String, String)>> = BTreeMap::new();

    for measure in part_node
        .children()
        .filter(|node| node.is_element() && node.has_tag_name("measure"))
    {
        let measure_result = parse_measure(measure, state);

        for staff_no in 1..=state.staves.max(1) {
            let acc = measure_result
                .staves
                .get(&staff_no)
                .cloned()
                .unwrap_or_else(|| MeasureAcc {
                    voices: BTreeMap::new(),
                    controls: Vec::new(),
                });
            let mut measure_text = emit_measure(&acc, state.divisions);
            if let Some(before) = &measure_result.barline_before {
                measure_text = if measure_text.is_empty() {
                    before.clone()
                } else {
                    format!("{before} {measure_text}")
                };
            }
            if !measure_text.is_empty() {
                staff_music
                    .entry(staff_no)
                    .or_default()
                    .push((measure_text, measure_result.barline_after.clone()));
            }
        }
    }

    for staff_no in 1..=state.staves.max(1) {
        staff_music.entry(staff_no).or_default();
    }

    staff_music
        .into_iter()
        .map(|(staff_no, measures)| {
            let music = measures.iter().enumerate().fold(
                String::new(),
                |mut acc, (idx, (measure, barline))| {
                    if idx > 0 {
                        acc.push(' ');
                    }
                    acc.push_str(measure);
                    let next_starts_with_left_repeat = measures
                        .get(idx + 1)
                        .is_some_and(|(next, _)| next.starts_with("|:"));
                    if (idx + 1 < measures.len()
                        && !(barline == "|" && next_starts_with_left_repeat))
                        || barline != "|"
                    {
                        acc.push(' ');
                        acc.push_str(barline);
                    }
                    acc
                },
            );

            Staff {
                clef: state
                    .clefs
                    .get(&staff_no)
                    .cloned()
                    .unwrap_or_else(|| "treble".to_string()),
                music,
                instrument_name: None,
                instrument_name_cont: None,
                brace_start: false,
                brace_end: false,
                bracket_start: false,
                bracket_end: false,
            }
        })
        .collect()
}

fn parse_measure(measure: Node<'_, '_>, state: &mut PartState) -> MeasureResult {
    let mut cursor = 0;
    let mut last_note_key: Option<(usize, String, i32)> = None;
    let mut pending = PendingAttachment::default();
    let mut accs: BTreeMap<usize, MeasureAcc> = BTreeMap::new();
    let mut barline_before = None;
    let mut barline_after = "|".to_string();

    for child in measure.children().filter(|node| node.is_element()) {
        match child.tag_name().name() {
            "attributes" => {
                parse_attributes(child, state, cursor, &mut accs);
            }
            "backup" => {
                cursor = (cursor - first_child_i32(child, "duration").unwrap_or(0)).max(0);
                last_note_key = None;
            }
            "forward" => {
                cursor += first_child_i32(child, "duration").unwrap_or(0);
                last_note_key = None;
            }
            "direction" => {
                collect_direction(child, &mut pending);
            }
            "harmony" => {
                pending.chord_symbol = harmony_text(child);
            }
            "note" => {
                let is_chord = child.children().any(|node| node.has_tag_name("chord"));
                let staff_no = first_child_usize(child, "staff")
                    .or_else(|| staff_from_voice(first_child_text(child, "voice").as_deref()))
                    .unwrap_or(1);
                let voice = first_child_text(child, "voice").unwrap_or_else(|| "1".to_string());
                let duration = first_child_i32(child, "duration").unwrap_or(0);
                let start = if is_chord {
                    last_note_key
                        .as_ref()
                        .filter(|(staff, last_voice, _)| *staff == staff_no && last_voice == &voice)
                        .map(|(_, _, start)| *start)
                        .unwrap_or(cursor)
                } else {
                    cursor
                };

                let token = parse_note_token(child, state, staff_no, pending.clone());
                pending = PendingAttachment::default();

                let acc = accs.entry(staff_no).or_insert_with(|| MeasureAcc {
                    voices: BTreeMap::new(),
                    controls: Vec::new(),
                });

                if let Some(Token::Note(note)) = token.as_ref() {
                    if is_chord {
                        if let Some(last) = acc
                            .voices
                            .get_mut(&voice)
                            .and_then(|events| events.last_mut())
                            .filter(|event| event.start == start)
                        {
                            if let Token::Note(existing) = &mut last.token {
                                existing.pitches.extend(note.pitches.clone());
                            }
                        }
                    } else {
                        acc.voices
                            .entry(voice.clone())
                            .or_default()
                            .push(TimedEvent {
                                start,
                                duration,
                                token: Token::Note(note.clone()),
                            });
                    }
                } else if let Some(token) = token {
                    acc.voices
                        .entry(voice.clone())
                        .or_default()
                        .push(TimedEvent {
                            start,
                            duration,
                            token,
                        });
                }

                if !is_chord {
                    cursor += duration;
                    last_note_key = Some((staff_no, voice, start));
                }
            }
            "barline" => {
                let barline = parse_barline(child);
                if child.attribute("location") == Some("left") {
                    barline_before = Some(barline);
                } else {
                    barline_after = barline;
                }
            }
            _ => {}
        }
    }

    MeasureResult {
        staves: accs,
        barline_before,
        barline_after,
    }
}

fn parse_attributes(
    attrs: Node<'_, '_>,
    state: &mut PartState,
    cursor: i32,
    accs: &mut BTreeMap<usize, MeasureAcc>,
) {
    if let Some(divisions) = first_child_i32(attrs, "divisions") {
        state.divisions = divisions.max(1);
    }
    if let Some(staves) = first_child_usize(attrs, "staves") {
        state.staves = state.staves.max(staves.max(1));
    }
    if let Some(key_node) = attrs.children().find(|node| node.has_tag_name("key")) {
        if let Some(key) = parse_key(key_node) {
            state.key = Some(key);
        }
    }
    if let Some(time_node) = attrs.children().find(|node| node.has_tag_name("time")) {
        if let Some(time) = parse_time(time_node) {
            let is_change = state
                .time
                .as_deref()
                .is_some_and(|previous| previous != time);
            state.time = Some(time.clone());
            if is_change {
                for staff_no in 1..=state.staves.max(1) {
                    accs.entry(staff_no)
                        .or_insert_with(|| MeasureAcc {
                            voices: BTreeMap::new(),
                            controls: Vec::new(),
                        })
                        .controls
                        .push(TimedEvent {
                            start: cursor,
                            duration: 0,
                            token: Token::Time(time.clone()),
                        });
                }
            }
        }
    }

    for clef_node in attrs.children().filter(|node| node.has_tag_name("clef")) {
        let staff_no = clef_node
            .attribute("number")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1);
        let clef = parse_clef(clef_node);
        let old = state.clefs.insert(staff_no, clef.clone());
        if old.as_deref().is_some_and(|previous| previous != clef) {
            accs.entry(staff_no)
                .or_insert_with(|| MeasureAcc {
                    voices: BTreeMap::new(),
                    controls: Vec::new(),
                })
                .controls
                .push(TimedEvent {
                    start: cursor,
                    duration: 0,
                    token: Token::Clef(clef),
                });
        }
    }
}

fn parse_note_token(
    note: Node<'_, '_>,
    state: &PartState,
    staff_no: usize,
    pending: PendingAttachment,
) -> Option<Token> {
    if note.children().any(|node| node.has_tag_name("rest")) {
        return Some(Token::Rest);
    }

    let pitch_node = note.children().find(|node| node.has_tag_name("pitch"))?;
    let step = first_child_text(pitch_node, "step")?
        .chars()
        .next()
        .unwrap_or('C')
        .to_ascii_lowercase();
    let alter = first_child_i32(pitch_node, "alter").unwrap_or(0);
    let octave = first_child_i32(pitch_node, "octave").unwrap_or_else(|| {
        clef_base_octave(
            state
                .clefs
                .get(&staff_no)
                .map(String::as_str)
                .unwrap_or("treble"),
        )
    });
    let accidental_text = first_child_text(note, "accidental");
    let clef = state
        .clefs
        .get(&staff_no)
        .cloned()
        .unwrap_or_else(|| "treble".to_string());

    let notations = note.children().find(|node| node.has_tag_name("notations"));
    let slur_start = notations.is_some_and(|notations| {
        notations.descendants().any(|node| {
            node.has_tag_name("slur")
                && node
                    .attribute("type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("start"))
        })
    });
    let slur_stop = notations.is_some_and(|notations| {
        notations.descendants().any(|node| {
            node.has_tag_name("slur")
                && node
                    .attribute("type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("stop"))
        })
    });

    let articulations = notations
        .into_iter()
        .flat_map(|notations| notations.descendants())
        .filter_map(|node| match node.tag_name().name() {
            "accent" => Some(">"),
            "staccato" => Some("*"),
            "tenuto" => Some("-"),
            "fermata" => Some("_"),
            _ => None,
        })
        .collect();

    let tie_start = note.children().any(|node| {
        node.has_tag_name("tie")
            && node
                .attribute("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("start"))
    });
    let tie_stop = note.children().any(|node| {
        node.has_tag_name("tie")
            && node
                .attribute("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("stop"))
    });

    let dots = note
        .children()
        .filter(|node| node.has_tag_name("dot"))
        .count();
    let lyrics = parse_lyrics(note);

    Some(Token::Note(NoteToken {
        clef,
        pitches: vec![PitchToken {
            step,
            alter,
            accidental_text,
            octave,
        }],
        duration_text: first_child_text(note, "type").and_then(|kind| type_to_duration(&kind)),
        dots,
        tie_start,
        tie_stop,
        slur_start,
        slur_stop,
        dynamic: pending.dynamic,
        chord_symbol: pending.chord_symbol,
        staff_text: pending.staff_text,
        lyrics,
        articulations,
    }))
}

fn collect_direction(direction: Node<'_, '_>, pending: &mut PendingAttachment) {
    for direction_type in direction
        .children()
        .filter(|node| node.has_tag_name("direction-type"))
    {
        if let Some(dynamic) = direction_type
            .descendants()
            .find(|node| {
                matches!(
                    node.tag_name().name(),
                    "pppppp"
                        | "ppppp"
                        | "pppp"
                        | "ppp"
                        | "pp"
                        | "p"
                        | "mp"
                        | "mf"
                        | "f"
                        | "ff"
                        | "fff"
                        | "ffff"
                        | "fp"
                        | "sf"
                        | "sfp"
                        | "sfz"
                        | "rfz"
                )
            })
            .map(|node| node.tag_name().name().to_string())
        {
            pending.dynamic = Some(dynamic);
        }

        if let Some(words) = direction_type
            .children()
            .find(|node| node.has_tag_name("words"))
            .and_then(node_text)
        {
            pending.staff_text = Some(words);
        }
    }
}

fn emit_measure(acc: &MeasureAcc, divisions: i32) -> String {
    let mut controls = acc.controls.clone();
    controls.sort_by_key(|event| event.start);

    if acc.voices.is_empty() {
        return controls
            .iter()
            .map(|event| token_to_string(event, divisions))
            .collect::<Vec<_>>()
            .join(" ");
    }

    let mut voice_entries: Vec<_> = acc.voices.iter().collect();
    voice_entries.sort_by_key(|(voice, _)| voice.parse::<u32>().unwrap_or(u32::MAX));

    if voice_entries.len() == 1 {
        let (_, events) = voice_entries[0];
        emit_voice(events, &controls, divisions)
    } else {
        let upper = emit_voice(voice_entries[0].1, &controls, divisions);
        let lower = emit_voice(voice_entries[1].1, &[], divisions);
        format!("v{{{};{}}}", upper.trim(), lower.trim())
    }
}

fn emit_voice(events: &[TimedEvent], controls: &[TimedEvent], divisions: i32) -> String {
    let mut pieces = Vec::new();
    let mut sorted = events.to_vec();
    sorted.sort_by_key(|event| event.start);

    let mut control_idx = 0;
    let mut cursor = 0;

    for event in sorted {
        while control_idx < controls.len() && controls[control_idx].start <= event.start {
            pieces.push(token_to_string(&controls[control_idx], divisions));
            control_idx += 1;
        }
        if event.start > cursor {
            pieces.extend(duration_to_tokens("s", event.start - cursor, divisions));
        }
        pieces.push(token_to_string(&event, divisions));
        cursor = cursor.max(event.start + event.duration);
    }

    while control_idx < controls.len() {
        pieces.push(token_to_string(&controls[control_idx], divisions));
        control_idx += 1;
    }

    pieces
        .into_iter()
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn token_to_string(event: &TimedEvent, divisions: i32) -> String {
    match &event.token {
        Token::Note(note) => note_to_string(note, event.duration, divisions),
        Token::Rest => duration_to_tokens("r", event.duration, divisions).join(" "),
        Token::Clef(clef) => clef.clone(),
        Token::Time(time) => time.clone(),
    }
}

fn note_to_string(note: &NoteToken, duration: i32, divisions: i32) -> String {
    let (duration_text, dots) = if let Some(duration_text) = &note.duration_text {
        (duration_text.clone(), ".".repeat(note.dots))
    } else {
        (
            duration_to_text(duration, divisions).unwrap_or_else(|| "4".to_string()),
            String::new(),
        )
    };
    let pitch_text = if note.pitches.len() == 1 {
        pitch_to_string(&note.pitches[0], &note.clef)
    } else {
        format!(
            "<{}>",
            note.pitches
                .iter()
                .map(|pitch| pitch_to_string(pitch, &note.clef))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };

    let mut token = format!("{pitch_text}{duration_text}{dots}");
    if note.slur_start {
        token.push('(');
    }
    for articulation in &note.articulations {
        token.push_str(articulation);
    }
    if note.tie_start {
        token.push('~');
    }
    if note.slur_stop {
        token.push(')');
    }
    if let Some(dynamic) = &note.dynamic {
        token.push_str("v[");
        token.push_str(&escape_inline(dynamic));
        token.push(']');
    }
    if let Some(text) = &note.staff_text {
        token.push_str("text[");
        token.push_str(&escape_inline(text));
        token.push(']');
    }
    if let Some(chord) = &note.chord_symbol {
        token.push('[');
        token.push_str(&escape_inline(chord));
        token.push(']');
    }
    for lyric in lyric_tokens_to_scorify(&note.lyrics) {
        token.push_str(&lyric);
    }

    token
}

fn pitch_to_string(pitch: &PitchToken, clef: &str) -> String {
    let mut out = String::new();
    out.push(pitch.step);
    out.push_str(accidental_to_scorify(
        pitch.alter,
        pitch.accidental_text.as_deref(),
    ));

    let base = clef_base_octave(clef);
    let delta = pitch.octave - base;
    if delta > 0 {
        out.push_str(&"'".repeat(delta as usize));
    } else if delta < 0 {
        out.push_str(&",".repeat((-delta) as usize));
    }
    out
}

fn accidental_to_scorify(alter: i32, accidental: Option<&str>) -> &'static str {
    match accidental.unwrap_or("").to_ascii_lowercase().as_str() {
        "sharp" => "#",
        "double-sharp" | "sharp-sharp" => "##",
        "flat" => "&",
        "flat-flat" | "double-flat" => "&&",
        "natural" => "=",
        _ => match alter {
            2 => "##",
            1 => "#",
            -1 => "&",
            -2 => "&&",
            _ => "",
        },
    }
}

fn duration_to_tokens(prefix: &str, mut ticks: i32, divisions: i32) -> Vec<String> {
    let mut out = Vec::new();
    while ticks > 0 {
        if let Some(duration) = duration_to_text(ticks, divisions) {
            out.push(format!("{prefix}{duration}"));
            break;
        }

        let mut emitted = false;
        for denom in [1, 2, 4, 8, 16, 32, 64, 128] {
            let unit_num = 4 * divisions;
            if unit_num % denom == 0 {
                let unit = unit_num / denom;
                if unit > 0 && unit <= ticks {
                    out.push(format!(
                        "{prefix}{}",
                        duration_name(denom).unwrap_or_else(|| denom.to_string())
                    ));
                    ticks -= unit;
                    emitted = true;
                    break;
                }
            }
        }
        if !emitted {
            out.push(format!("{prefix}4"));
            break;
        }
    }
    out
}

fn duration_to_text(ticks: i32, divisions: i32) -> Option<String> {
    if ticks <= 0 || divisions <= 0 {
        return None;
    }
    let whole_num = 4 * divisions;
    for denom in [1, 2, 4, 8, 16, 32, 64, 128] {
        for dots in 0..=3 {
            let multiplier_num = (1 << (dots + 1)) - 1;
            let multiplier_den = 1 << dots;
            if whole_num * multiplier_num == ticks * denom * multiplier_den {
                let mut text = duration_name(denom).unwrap_or_else(|| denom.to_string());
                text.push_str(&".".repeat(dots as usize));
                return Some(text);
            }
        }
    }
    None
}

fn duration_name(denom: i32) -> Option<String> {
    match denom {
        1 => Some("1".to_string()),
        2 => Some("2".to_string()),
        4 => Some("4".to_string()),
        8 => Some("8".to_string()),
        16 => Some("16".to_string()),
        32 => Some("32".to_string()),
        64 => Some("64".to_string()),
        128 => Some("128".to_string()),
        _ => None,
    }
}

fn type_to_duration(kind: &str) -> Option<String> {
    match kind.trim() {
        "maxima" => Some("maxima".to_string()),
        "long" | "longa" => Some("longa".to_string()),
        "breve" => Some("breve".to_string()),
        "whole" => Some("1".to_string()),
        "half" => Some("2".to_string()),
        "quarter" => Some("4".to_string()),
        "eighth" => Some("8".to_string()),
        "16th" => Some("16".to_string()),
        "32nd" => Some("32".to_string()),
        "64th" => Some("64".to_string()),
        "128th" => Some("128".to_string()),
        _ => None,
    }
}

fn parse_key(key_node: Node<'_, '_>) -> Option<String> {
    let fifths = first_child_i32(key_node, "fifths")?;
    let mode = first_child_text(key_node, "mode").unwrap_or_else(|| "major".to_string());
    let major = [
        (-7, "Cb"),
        (-6, "Gb"),
        (-5, "Db"),
        (-4, "Ab"),
        (-3, "Eb"),
        (-2, "Bb"),
        (-1, "F"),
        (0, "C"),
        (1, "G"),
        (2, "D"),
        (3, "A"),
        (4, "E"),
        (5, "B"),
        (6, "F#"),
        (7, "C#"),
    ];
    let minor = [
        (-7, "ab"),
        (-6, "eb"),
        (-5, "bb"),
        (-4, "f"),
        (-3, "c"),
        (-2, "g"),
        (-1, "d"),
        (0, "a"),
        (1, "e"),
        (2, "b"),
        (3, "f#"),
        (4, "c#"),
        (5, "g#"),
        (6, "d#"),
        (7, "a#"),
    ];
    let table = if mode.eq_ignore_ascii_case("minor") {
        minor.as_slice()
    } else {
        major.as_slice()
    };
    table
        .iter()
        .find(|(count, _)| *count == fifths)
        .map(|(_, key)| (*key).to_string())
}

fn parse_time(time_node: Node<'_, '_>) -> Option<String> {
    let symbol = time_node.attribute("symbol");
    if symbol.is_some_and(|value| value.eq_ignore_ascii_case("common")) {
        return Some("common".to_string());
    }
    if symbol.is_some_and(|value| value.eq_ignore_ascii_case("cut")) {
        return Some("cut".to_string());
    }
    let beats = first_child_text(time_node, "beats")?;
    let beat_type = first_child_text(time_node, "beat-type")?;
    Some(format!("{beats}/{beat_type}"))
}

fn parse_clef(clef_node: Node<'_, '_>) -> String {
    let sign = first_child_text(clef_node, "sign").unwrap_or_else(|| "G".to_string());
    let line = first_child_i32(clef_node, "line").unwrap_or(2);
    let octave_change = first_child_i32(clef_node, "clef-octave-change").unwrap_or(0);

    let base = match (sign.as_str(), line) {
        ("G", 2) => "treble",
        ("F", 4) => "bass",
        ("C", 3) => "alto",
        ("C", 4) => "tenor",
        ("percussion", _) | ("PERCUSSION", _) => "percussion",
        _ => "treble",
    };

    match (base, octave_change) {
        ("treble", 1) => "treble-8a".to_string(),
        ("treble", -1) => "treble-8b".to_string(),
        ("treble", 2) => "treble-15a".to_string(),
        ("treble", -2) => "treble-15b".to_string(),
        ("bass", 1) => "bass-8a".to_string(),
        ("bass", -1) => "bass-8b".to_string(),
        ("bass", 2) => "bass-15a".to_string(),
        ("bass", -2) => "bass-15b".to_string(),
        _ => base.to_string(),
    }
}

fn parse_barline(barline: Node<'_, '_>) -> String {
    if let Some(repeat) = barline.children().find(|node| node.has_tag_name("repeat")) {
        match repeat.attribute("direction") {
            Some("forward") => return "|:".to_string(),
            Some("backward") => return ":|".to_string(),
            _ => {}
        }
    }

    match first_child_text(barline, "bar-style").as_deref() {
        Some("light-heavy") => "|.".to_string(),
        Some("light-light") => "||".to_string(),
        Some("heavy-light") => "|:".to_string(),
        Some("heavy-heavy") => "||".to_string(),
        _ => "|".to_string(),
    }
}

fn harmony_text(harmony: Node<'_, '_>) -> Option<String> {
    let root = harmony.children().find(|node| node.has_tag_name("root"))?;
    let step = first_child_text(root, "root-step")?;
    let alter = first_child_i32(root, "root-alter").unwrap_or(0);
    let kind_node = harmony.children().find(|node| node.has_tag_name("kind"));
    let kind = kind_node.and_then(node_text).unwrap_or_default();
    let kind_text = kind_node.and_then(|node| node.attribute("text"));
    let bass = harmony.children().find(|node| node.has_tag_name("bass"));

    let mut text = step;
    text.push_str(match alter {
        1 => "#",
        -1 => "b",
        2 => "##",
        -2 => "bb",
        _ => "",
    });
    text.push_str(chord_kind_suffix(&kind, kind_text).as_str());

    if let Some(bass) = bass {
        if let Some(step) = first_child_text(bass, "bass-step") {
            text.push('/');
            text.push_str(&step);
            text.push_str(match first_child_i32(bass, "bass-alter").unwrap_or(0) {
                1 => "#",
                -1 => "b",
                2 => "##",
                -2 => "bb",
                _ => "",
            });
        }
    }

    Some(text)
}

fn chord_kind_suffix(kind: &str, text: Option<&str>) -> String {
    if let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) {
        return text.replace(' ', "");
    }

    match kind.trim() {
        "" | "major" | "none" => "",
        "minor" => "m",
        "augmented" => "aug",
        "diminished" => "dim",
        "dominant" => "7",
        "major-seventh" => "Maj7",
        "minor-seventh" => "m7",
        "diminished-seventh" => "dim7",
        "augmented-seventh" => "aug7",
        "half-diminished" => "m7b5",
        "major-minor" => "mMaj7",
        "major-sixth" => "6",
        "minor-sixth" => "m6",
        "dominant-ninth" => "9",
        "major-ninth" => "Maj9",
        "minor-ninth" => "m9",
        "dominant-11th" => "11",
        "major-11th" => "Maj11",
        "minor-11th" => "m11",
        "dominant-13th" => "13",
        "major-13th" => "Maj13",
        "minor-13th" => "m13",
        "suspended-second" => "sus2",
        "suspended-fourth" => "sus4",
        "power" => "5",
        "Neapolitan" => "N",
        "Italian" => "It+6",
        "French" => "Fr+6",
        "German" => "Ger+6",
        "Tristan" => "Tristan",
        "other" => "",
        other => other,
    }
    .to_string()
}

fn parse_lyric(lyric: Node<'_, '_>) -> Option<String> {
    let mut text = String::new();
    for child in lyric.children().filter(|node| node.is_element()) {
        match child.tag_name().name() {
            "syllabic" => {}
            "text" => text.push_str(&normalize_lyric_text(child.text().unwrap_or(""))),
            "extend" => text.push('_'),
            _ => {}
        }
    }

    if text.is_empty() {
        None
    } else {
        let syllabic = first_child_text(lyric, "syllabic");
        if syllabic.as_deref() == Some("begin") || syllabic.as_deref() == Some("middle") {
            text.push('-');
        }
        Some(text)
    }
}

fn normalize_lyric_text(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{00c2}' && chars.peek() == Some(&'\u{00a0}') {
            chars.next();
            out.push(' ');
        } else if matches!(ch, '\u{00a0}' | '\u{202f}' | '\u{2007}' | '\u{feff}') {
            out.push(' ');
        } else if ch.is_whitespace() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }

    out
}

fn parse_lyrics(note: Node<'_, '_>) -> Vec<LyricToken> {
    let mut lyrics = note
        .children()
        .filter(|node| node.has_tag_name("lyric"))
        .enumerate()
        .filter_map(|(index, lyric)| {
            let text = parse_lyric(lyric)?;
            let verse = lyric.attribute("number").and_then(|value| {
                value
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|number| *number > 0)
            });
            Some((verse.unwrap_or(u32::MAX), index, LyricToken { verse, text }))
        })
        .collect::<Vec<_>>();

    lyrics.sort_by_key(|(verse, index, _)| (*verse, *index));
    lyrics.into_iter().map(|(_, _, lyric)| lyric).collect()
}

fn lyric_tokens_to_scorify(lyrics: &[LyricToken]) -> Vec<String> {
    let mut out = Vec::new();
    let mut next_numbered_verse = 1;

    for lyric in lyrics {
        if let Some(verse) = lyric.verse {
            while next_numbered_verse < verse {
                out.push("l".to_string());
                next_numbered_verse += 1;
            }
            next_numbered_verse = verse.saturating_add(1);
        }

        let mut text = String::from("l[");
        text.push_str(&escape_inline(&lyric.text));
        text.push(']');
        out.push(text);
    }

    out
}

fn staff_from_voice(voice: Option<&str>) -> Option<usize> {
    let voice = voice?.parse::<usize>().ok()?;
    if voice >= 5 { Some(2) } else { Some(1) }
}

pub(crate) fn clef_base_octave(clef: &str) -> i32 {
    if clef.starts_with("bass") { 3 } else { 4 }
}

fn emit_typst(score: &Score, options: &ConversionOptions) -> String {
    let mut out = String::new();
    if options.include_comment {
        out.push_str("// Generated by musicxml-to-scorify.\n");
    }
    if options.include_import {
        out.push_str("#import \"");
        out.push_str(&escape_typst_string(&options.package));
        out.push_str("\": score\n\n");
    }

    out.push_str("#score(\n");
    if let Some(title) = &score.title {
        out.push_str("  title: \"");
        out.push_str(&escape_typst_string(title));
        out.push_str("\",\n");
    }
    if let Some(composer) = &score.composer {
        out.push_str("  composer: \"");
        out.push_str(&escape_typst_string(composer));
        out.push_str("\",\n");
    }
    out.push_str("  key: \"");
    out.push_str(&escape_typst_string(score.key.as_deref().unwrap_or("C")));
    out.push_str("\",\n");
    if let Some(time) = &score.time {
        out.push_str("  time: \"");
        out.push_str(&escape_typst_string(time));
        out.push_str("\",\n");
    }
    if let Some(measures_per_line) = options.measures_per_line {
        out.push_str("  measures-per-line: ");
        out.push_str(&measures_per_line.to_string());
        out.push_str(",\n");
    }

    out.push_str("  staves: (\n");
    for staff in &score.staves {
        out.push_str("    (\n");
        out.push_str("      clef: \"");
        out.push_str(&escape_typst_string(&staff.clef));
        out.push_str("\",\n");
        if let Some(name) = &staff.instrument_name {
            out.push_str("      instrument-name: \"");
            out.push_str(&escape_typst_string(name));
            out.push_str("\",\n");
        }
        if let Some(name) = &staff.instrument_name_cont {
            out.push_str("      instrument-name-cont: \"");
            out.push_str(&escape_typst_string(name));
            out.push_str("\",\n");
        }
        if staff.brace_start {
            out.push_str("      brace-start: true,\n");
        }
        if staff.brace_end {
            out.push_str("      brace-end: true,\n");
        }
        if staff.bracket_start {
            out.push_str("      bracket-start: true,\n");
        }
        if staff.bracket_end {
            out.push_str("      bracket-end: true,\n");
        }
        out.push_str("      music: \"");
        out.push_str(&escape_typst_string(&staff.music));
        out.push_str("\",\n");
        out.push_str("    ),\n");
    }
    out.push_str("  ),\n");
    out.push_str(")\n");
    out
}

fn first_child_text(node: Node<'_, '_>, tag: &str) -> Option<String> {
    node.children()
        .find(|child| child.is_element() && child.has_tag_name(tag))
        .and_then(node_text)
}

fn first_descendant_text(node: Node<'_, '_>, tag: &str) -> Option<String> {
    node.descendants()
        .find(|child| child.is_element() && child.has_tag_name(tag))
        .and_then(node_text)
}

fn first_child_i32(node: Node<'_, '_>, tag: &str) -> Option<i32> {
    first_child_text(node, tag)?.trim().parse().ok()
}

fn first_child_usize(node: Node<'_, '_>, tag: &str) -> Option<usize> {
    first_child_text(node, tag)?.trim().parse().ok()
}

fn node_text(node: Node<'_, '_>) -> Option<String> {
    let text = node.text()?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn escape_typst_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn escape_inline(value: &str) -> String {
    value.replace(']', "\\]")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_XML: &str = r#"
<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <work><work-title>Tiny Tune</work-title></work>
  <identification><creator type="composer">A. Composer</creator></identification>
  <part-list>
    <score-part id="P1"><part-name>Piano</part-name><part-abbreviation>Pno.</part-abbreviation></score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>2</divisions>
        <key><fifths>2</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <staves>2</staves>
        <clef number="1"><sign>G</sign><line>2</line></clef>
        <clef number="2"><sign>F</sign><line>4</line></clef>
      </attributes>
      <direction><direction-type><dynamics><mf/></dynamics></direction-type></direction>
      <note>
        <pitch><step>F</step><alter>1</alter><octave>4</octave></pitch>
        <duration>2</duration><voice>1</voice><type>quarter</type><staff>1</staff>
        <lyric><syllabic>single</syllabic><text>Joy</text></lyric>
      </note>
      <note>
        <chord/>
        <pitch><step>A</step><octave>4</octave></pitch>
        <duration>2</duration><voice>1</voice><type>quarter</type><staff>1</staff>
      </note>
      <note>
        <rest/><duration>2</duration><voice>1</voice><type>quarter</type><staff>1</staff>
      </note>
      <backup><duration>4</duration></backup>
      <note>
        <pitch><step>D</step><octave>3</octave></pitch>
        <duration>4</duration><voice>5</voice><type>half</type><staff>2</staff>
      </note>
    </measure>
  </part>
</score-partwise>
"#;

    #[test]
    fn converts_simple_grand_staff_score() {
        let result = convert_musicxml_to_scorify(
            SIMPLE_XML,
            &ConversionOptions {
                include_import: false,
                ..ConversionOptions::default()
            },
        )
        .unwrap();

        assert!(result.contains("title: \"Tiny Tune\""));
        assert!(result.contains("composer: \"A. Composer\""));
        assert!(result.contains("key: \"D\""));
        assert!(result.contains("time: \"4/4\""));
        assert!(result.contains("brace-start: true"));
        assert!(result.contains("brace-end: true"));
        assert!(
            result.contains("music: \"<f# a>4v[mf]l[Joy] r4\""),
            "{result}"
        );
        assert!(result.contains("music: \"d2\""), "{result}");
    }

    #[test]
    fn maps_minor_keys_and_octaves() {
        let xml = r#"
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Flute</part-name></score-part></part-list>
  <part id="P1"><measure number="1">
    <attributes><divisions>1</divisions><key><fifths>-3</fifths><mode>minor</mode></key><clef><sign>G</sign><line>2</line></clef></attributes>
    <note><pitch><step>C</step><octave>5</octave></pitch><duration>1</duration><type>quarter</type></note>
    <note><pitch><step>B</step><alter>-1</alter><octave>3</octave></pitch><duration>1</duration><type>quarter</type></note>
  </measure></part>
</score-partwise>
"#;
        let result = convert_musicxml_to_scorify(
            xml,
            &ConversionOptions {
                include_import: false,
                ..ConversionOptions::default()
            },
        )
        .unwrap();

        assert!(result.contains("key: \"c\""));
        assert!(result.contains("c'4"));
        assert!(result.contains("b&,4"));
    }

    #[test]
    fn accepts_musicxml_doctype_declarations() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Voice</part-name></score-part></part-list>
  <part id="P1"><measure number="1">
    <attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
    <note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note>
  </measure></part>
</score-partwise>
"#;

        let result = convert_musicxml_to_scorify(
            xml,
            &ConversionOptions {
                include_import: false,
                ..ConversionOptions::default()
            },
        )
        .unwrap();

        assert!(result.contains("time: \"4/4\""));
        assert!(result.contains("music: \"c4\""));
    }

    #[test]
    fn emits_multiple_numbered_lyric_verses_on_one_note() {
        let xml = r#"
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Voice</part-name></score-part></part-list>
  <part id="P1"><measure number="1">
    <attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
    <note>
      <pitch><step>C</step><octave>4</octave></pitch>
      <duration>1</duration>
      <type>quarter</type>
      <lyric number="2"><syllabic>single</syllabic><text>Second</text></lyric>
      <lyric number="1"><syllabic>begin</syllabic><text>First</text></lyric>
    </note>
    <note>
      <pitch><step>D</step><octave>4</octave></pitch>
      <duration>1</duration>
      <type>quarter</type>
      <lyric number="1"><syllabic>end</syllabic><text>verse</text></lyric>
      <lyric number="2"><syllabic>single</syllabic><text>line</text></lyric>
    </note>
  </measure></part>
</score-partwise>
"#;

        let result = convert_musicxml_to_scorify(
            xml,
            &ConversionOptions {
                include_import: false,
                ..ConversionOptions::default()
            },
        )
        .unwrap();

        assert!(
            result.contains("music: \"c4l[First-]l[Second] d4l[verse]l[line]\""),
            "{result}"
        );
    }

    #[test]
    fn emits_empty_lyric_placeholders_for_later_verse_only() {
        let xml = r#"
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Voice</part-name></score-part></part-list>
  <part id="P1"><measure number="1">
    <attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
    <note>
      <pitch><step>C</step><octave>4</octave></pitch>
      <duration>1</duration>
      <type>quarter</type>
      <lyric number="3"><syllabic>single</syllabic><text>You,</text></lyric>
    </note>
  </measure></part>
</score-partwise>
"#;

        let result = convert_musicxml_to_scorify(
            xml,
            &ConversionOptions {
                include_import: false,
                ..ConversionOptions::default()
            },
        )
        .unwrap();

        assert!(result.contains("music: \"c4lll[You,]\""), "{result}");
    }

    #[test]
    fn maps_musicxml_harmony_kinds_to_compact_chord_symbols() {
        let xml = r#"
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Lead Sheet</part-name></score-part></part-list>
  <part id="P1"><measure number="1">
    <attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
    <harmony print-frame="no"><root><root-step>F</root-step></root><kind text="6">major-sixth</kind></harmony>
    <note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note>
    <harmony><root><root-step>C</root-step></root><kind>major-seventh</kind></harmony>
    <note><pitch><step>D</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note>
    <harmony><root><root-step>C</root-step></root><kind>dominant</kind></harmony>
    <note><pitch><step>E</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note>
    <harmony><root><root-step>C</root-step></root><kind>minor-seventh</kind></harmony>
    <note><pitch><step>F</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note>
  </measure></part>
</score-partwise>
"#;

        let result = convert_musicxml_to_scorify(
            xml,
            &ConversionOptions {
                include_import: false,
                ..ConversionOptions::default()
            },
        )
        .unwrap();

        assert!(
            result.contains("music: \"c4[F6] d4[CMaj7] e4[C7] f4[Cm7]\""),
            "{result}"
        );
        assert!(!result.contains("major-sixth"), "{result}");
    }

    #[test]
    fn normalizes_non_breaking_spaces_in_lyrics() {
        let xml = r#"
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Voice</part-name></score-part></part-list>
  <part id="P1"><measure number="1">
    <attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
    <note>
      <pitch><step>C</step><octave>4</octave></pitch>
      <duration>1</duration>
      <type>quarter</type>
      <lyric><syllabic>begin</syllabic><text>1.&#160;Ev</text></lyric>
    </note>
    <note>
      <pitch><step>D</step><octave>4</octave></pitch>
      <duration>1</duration>
      <type>quarter</type>
      <lyric><syllabic>single</syllabic><text>2.&#194;&#160;Ev</text></lyric>
    </note>
  </measure></part>
</score-partwise>
"#;

        let result = convert_musicxml_to_scorify(
            xml,
            &ConversionOptions {
                include_import: false,
                ..ConversionOptions::default()
            },
        )
        .unwrap();

        assert!(
            result.contains("music: \"c4l[1. Ev-] d4l[2. Ev]\""),
            "{result}"
        );
        assert!(!result.contains('\u{00a0}'), "{result}");
        assert!(!result.contains('\u{00c2}'), "{result}");
    }

    #[test]
    fn converts_file_bytes_and_json_options() {
        let options = conversion_options_from_json(
            r#"{"includeImport":false,"includeComment":true,"measuresPerLine":2}"#,
        )
        .unwrap();
        let result =
            convert_musicxml_file_to_scorify(SIMPLE_XML.as_bytes(), "simple.musicxml", &options)
                .unwrap();

        assert!(result.starts_with("// Generated by musicxml-to-scorify."));
        assert!(!result.contains("#import"));
        assert!(result.contains("measures-per-line: 2"));
    }

    #[test]
    fn extracts_compressed_mxl_bytes() {
        use std::io::Write;

        let mut bytes = Vec::new();
        {
            let cursor = Cursor::new(&mut bytes);
            let mut archive = zip::ZipWriter::new(cursor);
            let options = zip::write::SimpleFileOptions::default();

            archive
                .start_file("META-INF/container.xml", options)
                .unwrap();
            archive
                .write_all(
                    br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="score.musicxml"/></rootfiles>
</container>"#,
                )
                .unwrap();
            archive.start_file("score.musicxml", options).unwrap();
            archive.write_all(SIMPLE_XML.as_bytes()).unwrap();
            archive.finish().unwrap();
        }

        let result = convert_musicxml_file_to_scorify(
            &bytes,
            "simple.mxl",
            &ConversionOptions {
                include_import: false,
                ..ConversionOptions::default()
            },
        )
        .unwrap();

        assert!(result.contains("title: \"Tiny Tune\""));
        assert!(result.contains("music: \"<f# a>4v[mf]l[Joy] r4\""));
    }

        #[test]
        fn converts_scorify_score_back_to_musicxml() {
                let typst = r#"
#import "@preview/scorify:0.3.0": score

#score(
    title: "Simple Scorify Sample",
    composer: "MusicXML Fixture",
    key: "C",
    time: "4/4",
    staves: (
        (
            clef: "treble",
            instrument-name: "Flute",
            instrument-name-cont: "Fl.",
            music: "c4v[mf]l[Hel-] d4l[lo] e4*[G7] r4 |: f2~ f2 :|",
        ),
    ),
)
"#;

                let result = convert_scorify_to_musicxml(typst).unwrap();

                assert!(result.contains("<work-title>Simple Scorify Sample</work-title>"), "{result}");
                assert!(result.contains("<creator type=\"composer\">MusicXML Fixture</creator>"), "{result}");
                assert!(result.contains("<part-name>Flute</part-name>"), "{result}");
                assert!(result.contains("<dynamics><mf/></dynamics>"), "{result}");
                assert!(result.contains("<kind text=\"7\">other</kind>"), "{result}");
                assert!(result.contains("<repeat direction=\"forward\"/>"), "{result}");
                assert!(result.contains("<tie type=\"start\"/>"), "{result}");
                assert!(result.contains("<tie type=\"stop\"/>"), "{result}");
        }

        #[test]
        fn round_trips_grand_staff_and_secondary_voice() {
                let xml = r#"
<score-partwise version="4.0">
    <work><work-title>Tiny Tune</work-title></work>
    <identification><creator type="composer">A. Composer</creator></identification>
    <part-list>
        <score-part id="P1"><part-name>Piano</part-name><part-abbreviation>Pno.</part-abbreviation></score-part>
    </part-list>
    <part id="P1">
        <measure number="1">
            <attributes>
                <divisions>2</divisions>
                <key><fifths>2</fifths></key>
                <time><beats>4</beats><beat-type>4</beat-type></time>
                <staves>2</staves>
                <clef number="1"><sign>G</sign><line>2</line></clef>
                <clef number="2"><sign>F</sign><line>4</line></clef>
            </attributes>
            <note>
                <pitch><step>C</step><octave>4</octave></pitch>
                <duration>2</duration><voice>1</voice><type>quarter</type><staff>1</staff>
            </note>
            <backup><duration>2</duration></backup>
            <note>
                <rest/><duration>2</duration><voice>2</voice><type>quarter</type><staff>1</staff>
            </note>
            <note>
                <pitch><step>D</step><octave>3</octave></pitch>
                <duration>4</duration><voice>1</voice><type>half</type><staff>2</staff>
            </note>
        </measure>
    </part>
</score-partwise>
"#;

                let typst = convert_musicxml_to_scorify(
                        xml,
                        &ConversionOptions {
                                include_import: false,
                                ..ConversionOptions::default()
                        },
                )
                .unwrap();
                let result = convert_scorify_to_musicxml(&typst).unwrap();

                assert!(result.contains("<staves>2</staves>"), "{result}");
                assert!(result.contains("<clef number=\"2\"><sign>F</sign><line>4</line></clef>"), "{result}");
                assert!(result.contains("<backup><duration>768</duration></backup>"), "{result}");
                assert!(result.contains("<voice>2</voice>"), "{result}");
                assert!(result.contains("<staff>2</staff>"), "{result}");
        }
}
