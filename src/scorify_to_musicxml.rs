use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use crate::{
    ConvertError, LyricToken, MeasureAcc, NoteToken, PitchToken, Score, Staff, TimedEvent, Token,
    clef_base_octave,
};

const MUSICXML_DIVISIONS: i32 = 768;

pub(crate) fn convert_scorify_to_musicxml(source: &str) -> Result<String, ConvertError> {
    let score = parse_scorify_score(source)?;
    let parts = build_parts(&score)?;
    emit_musicxml(&score, &parts)
}

#[derive(Debug, Clone)]
enum TypstValue {
    Bool(bool),
    Number(i64),
    String(String),
    Tuple(Vec<TypstValue>),
    Object(BTreeMap<String, TypstValue>),
}

#[derive(Debug, Clone)]
struct ParsedStaffMeasure {
    acc: MeasureAcc,
    barline_before: Option<String>,
    barline_after: String,
    length: i32,
}

#[derive(Debug, Clone)]
struct PartSpec {
    id: String,
    name: Option<String>,
    abbreviation: Option<String>,
    staves: Vec<PartStaff>,
}

#[derive(Debug, Clone)]
struct PartStaff {
    staff_number: usize,
    clef: String,
    measures: Vec<ParsedStaffMeasure>,
}

fn parse_scorify_score(source: &str) -> Result<Score, ConvertError> {
    let score_offset = source
        .find("#score")
        .ok_or(ConvertError::MissingScorifyScore)?;
    let mut parser = TypstParser::new(&source[score_offset + "#score".len()..]);
    parser.skip_ws_comments();
    parser.expect('(')?;
    let fields = parser.parse_named_args(')')?;
    parser.expect(')')?;

    let staves_value = fields
        .get("staves")
        .ok_or_else(|| ConvertError::InvalidScorify("score is missing staves".to_string()))?;
    let _ = optional_number(&fields, "measures-per-line")?;
    let staves_tuple = match staves_value {
        TypstValue::Tuple(tuple) => tuple,
        _ => {
            return Err(ConvertError::InvalidScorify(
                "staves must be a tuple".to_string(),
            ))
        }
    };

    let mut staves = Vec::with_capacity(staves_tuple.len());
    for entry in staves_tuple {
        let object = match entry {
            TypstValue::Object(object) => object,
            _ => {
                return Err(ConvertError::InvalidScorify(
                    "each staff must be a tuple of named fields".to_string(),
                ))
            }
        };
        let clef = required_string(object, "clef")?;
        let music = required_string(object, "music")?;
        staves.push(Staff {
            clef,
            music,
            instrument_name: optional_string(object, "instrument-name")?,
            instrument_name_cont: optional_string(object, "instrument-name-cont")?,
            brace_start: optional_bool(object, "brace-start")?.unwrap_or(false),
            brace_end: optional_bool(object, "brace-end")?.unwrap_or(false),
            bracket_start: optional_bool(object, "bracket-start")?.unwrap_or(false),
            bracket_end: optional_bool(object, "bracket-end")?.unwrap_or(false),
        });
    }

    Ok(Score {
        title: optional_string(&fields, "title")?,
        composer: optional_string(&fields, "composer")?,
        key: optional_string(&fields, "key")?,
        time: optional_string(&fields, "time")?,
        staves,
    })
}

fn build_parts(score: &Score) -> Result<Vec<PartSpec>, ConvertError> {
    let mut parts = Vec::new();
    let mut index = 0;

    while index < score.staves.len() {
        let start = index;
        let mut end = index + 1;
        let staff = &score.staves[index];
        if staff.brace_start {
            while end < score.staves.len() && !score.staves[end - 1].brace_end {
                end += 1;
                if score.staves.get(end - 1).is_some_and(|candidate| candidate.brace_end) {
                    break;
                }
            }
            if end == start + 1 {
                end = (start + 2).min(score.staves.len());
            }
        } else if staff.bracket_start {
            while end < score.staves.len() && !score.staves[end - 1].bracket_end {
                end += 1;
                if score
                    .staves
                    .get(end - 1)
                    .is_some_and(|candidate| candidate.bracket_end)
                {
                    break;
                }
            }
            if end == start + 1 {
                end = (start + 2).min(score.staves.len());
            }
        }

        let grouped = &score.staves[start..end];
        let mut part_staves = Vec::with_capacity(grouped.len());
        let mut expected_measures = None;
        for (offset, grouped_staff) in grouped.iter().enumerate() {
            let measures = parse_staff_music(&grouped_staff.music, &grouped_staff.clef)?;
            match expected_measures {
                Some(expected) if expected != measures.len() => {
                    return Err(ConvertError::InvalidScorify(format!(
                        "staff group starting at staff {} has mismatched measure counts",
                        start + 1
                    )))
                }
                None => expected_measures = Some(measures.len()),
                _ => {}
            }
            part_staves.push(PartStaff {
                staff_number: offset + 1,
                clef: grouped_staff.clef.clone(),
                measures,
            });
        }

        parts.push(PartSpec {
            id: format!("P{}", parts.len() + 1),
            name: grouped.first().and_then(|staff| staff.instrument_name.clone()),
            abbreviation: grouped
                .first()
                .and_then(|staff| staff.instrument_name_cont.clone()),
            staves: part_staves,
        });
        index = end;
    }

    Ok(parts)
}

fn parse_staff_music(music: &str, default_clef: &str) -> Result<Vec<ParsedStaffMeasure>, ConvertError> {
    let tokens = split_top_level_tokens(music)?;
    let mut current = Vec::new();
    let mut measures = Vec::new();
    let mut pending_before = None;

    for token in tokens {
        if is_barline_token(&token) {
            if current.is_empty() {
                pending_before = Some(token);
                continue;
            }

            measures.push(parse_measure_tokens(
                &current,
                pending_before.take(),
                token,
                default_clef,
            )?);
            current.clear();
        } else {
            current.push(token);
        }
    }

    if !current.is_empty() || pending_before.is_some() {
        measures.push(parse_measure_tokens(
            &current,
            pending_before,
            "|".to_string(),
            default_clef,
        )?);
    }

    if measures.is_empty() {
        measures.push(ParsedStaffMeasure {
            acc: MeasureAcc {
                voices: BTreeMap::new(),
                controls: Vec::new(),
            },
            barline_before: None,
            barline_after: "|".to_string(),
            length: 0,
        });
    }

    Ok(measures)
}

fn parse_measure_tokens(
    tokens: &[String],
    barline_before: Option<String>,
    barline_after: String,
    default_clef: &str,
) -> Result<ParsedStaffMeasure, ConvertError> {
    let mut acc = MeasureAcc {
        voices: BTreeMap::new(),
        controls: Vec::new(),
    };
    let mut length = 0;

    if tokens.len() == 1 && tokens[0].starts_with("v{") {
        let (upper, lower) = split_voice_token(&tokens[0])?;
        length = length.max(parse_voice_tokens(
            &split_top_level_tokens(&upper)?,
            "1",
            default_clef,
            &mut acc,
        )?);
        length = length.max(parse_voice_tokens(
            &split_top_level_tokens(&lower)?,
            "2",
            default_clef,
            &mut acc,
        )?);
    } else {
        length = parse_voice_tokens(tokens, "1", default_clef, &mut acc)?;
    }

    Ok(ParsedStaffMeasure {
        acc,
        barline_before,
        barline_after,
        length,
    })
}

fn parse_voice_tokens(
    tokens: &[String],
    voice: &str,
    default_clef: &str,
    acc: &mut MeasureAcc,
) -> Result<i32, ConvertError> {
    let mut cursor = 0;
    let mut clef = default_clef.to_string();

    for token in tokens {
        if is_clef_token(token) {
            clef = token.clone();
            acc.controls.push(TimedEvent {
                start: cursor,
                duration: 0,
                token: Token::Clef(token.clone()),
            });
            continue;
        }

        if is_time_token(token) {
            acc.controls.push(TimedEvent {
                start: cursor,
                duration: 0,
                token: Token::Time(token.clone()),
            });
            continue;
        }

        match parse_voice_event(token, &clef)? {
            VoiceEvent::Skip(duration) => {
                cursor += duration;
            }
            VoiceEvent::Rest(duration, duration_text, dots) => {
                acc.voices
                    .entry(voice.to_string())
                    .or_default()
                    .push(TimedEvent {
                        start: cursor,
                        duration,
                        token: Token::Rest,
                    });
                let _ = (duration_text, dots);
                cursor += duration;
            }
            VoiceEvent::Note { duration, note } => {
                acc.voices
                    .entry(voice.to_string())
                    .or_default()
                    .push(TimedEvent {
                        start: cursor,
                        duration,
                        token: Token::Note(note),
                    });
                cursor += duration;
            }
        }
    }

    Ok(cursor)
}

enum VoiceEvent {
    Note { duration: i32, note: NoteToken },
    Rest(i32, String, usize),
    Skip(i32),
}

fn parse_voice_event(token: &str, clef: &str) -> Result<VoiceEvent, ConvertError> {
    if let Some(rest) = token.strip_prefix('r') {
        let (duration_text, dots, suffix) = parse_duration(rest)?;
        if !suffix.is_empty() {
            return Err(ConvertError::InvalidScorify(format!(
                "unexpected suffix on rest token '{token}'"
            )));
        }
        let duration = duration_to_ticks(&duration_text, dots)?;
        return Ok(VoiceEvent::Rest(duration, duration_text, dots));
    }

    if let Some(rest) = token.strip_prefix('s') {
        let (duration_text, dots, suffix) = parse_duration(rest)?;
        if !suffix.is_empty() {
            return Err(ConvertError::InvalidScorify(format!(
                "unexpected suffix on skip token '{token}'"
            )));
        }
        let duration = duration_to_ticks(&duration_text, dots)?;
        return Ok(VoiceEvent::Skip(duration));
    }

    let (pitches, rest) = if let Some(after) = token.strip_prefix('<') {
        let end = after.find('>').ok_or_else(|| {
            ConvertError::InvalidScorify(format!("unterminated chord token '{token}'"))
        })?;
        let pitch_text = &after[..end];
        let mut pitches = Vec::new();
        for pitch in pitch_text.split_whitespace() {
            pitches.push(parse_pitch(pitch, clef)?);
        }
        (pitches, &after[end + 1..])
    } else {
        let pitch_len = pitch_token_len(token)?;
        (vec![parse_pitch(&token[..pitch_len], clef)?], &token[pitch_len..])
    };

    let (duration_text, dots, suffix) = parse_duration(rest)?;
    let duration = duration_to_ticks(&duration_text, dots)?;
    let mut suffix = suffix;

    let mut slur_start = false;
    if let Some(next) = suffix.strip_prefix('(') {
        slur_start = true;
        suffix = next;
    }

    let mut articulations = Vec::new();
    while let Some(ch) = suffix.chars().next() {
        let articulation = match ch {
            '>' => Some(">"),
            '*' => Some("*"),
            '-' => Some("-"),
            '_' => Some("_"),
            _ => None,
        };
        let Some(articulation) = articulation else {
            break;
        };
        articulations.push(articulation);
        suffix = &suffix[ch.len_utf8()..];
    }

    let mut tie_start = false;
    if let Some(next) = suffix.strip_prefix('~') {
        tie_start = true;
        suffix = next;
    }

    let mut slur_stop = false;
    if let Some(next) = suffix.strip_prefix(')') {
        slur_stop = true;
        suffix = next;
    }

    let mut dynamic = None;
    let mut staff_text = None;
    let mut chord_symbol = None;
    let mut lyric_segments = Vec::new();

    while !suffix.is_empty() {
        if let Some(next) = suffix.strip_prefix("v[") {
            let (value, rest) = parse_bracket_text(next)?;
            dynamic = Some(value);
            suffix = rest;
            continue;
        }

        if let Some(next) = suffix.strip_prefix("text[") {
            let (value, rest) = parse_bracket_text(next)?;
            staff_text = Some(value);
            suffix = rest;
            continue;
        }

        if let Some(next) = suffix.strip_prefix('[') {
            let (value, rest) = parse_bracket_text(next)?;
            chord_symbol = Some(value);
            suffix = rest;
            continue;
        }

        if let Some(next) = suffix.strip_prefix("l[") {
            let (value, rest) = parse_bracket_text(next)?;
            lyric_segments.push(Some(value));
            suffix = rest;
            continue;
        }

        if let Some(next) = suffix.strip_prefix('l') {
            lyric_segments.push(None);
            suffix = next;
            continue;
        }

        return Err(ConvertError::InvalidScorify(format!(
            "could not parse token suffix in '{token}'"
        )));
    }

    let mut lyrics = Vec::new();
    if lyric_segments.len() == 1 && lyric_segments[0].is_some() {
        lyrics.push(LyricToken {
            verse: None,
            text: lyric_segments[0].clone().unwrap_or_default(),
        });
    } else {
        for (index, segment) in lyric_segments.into_iter().enumerate() {
            if let Some(text) = segment {
                lyrics.push(LyricToken {
                    verse: Some(index as u32 + 1),
                    text,
                });
            }
        }
    }

    Ok(VoiceEvent::Note {
        duration,
        note: NoteToken {
            clef: clef.to_string(),
            pitches,
            duration_text: Some(duration_text),
            dots,
            tie_start,
            tie_stop: false,
            slur_start,
            slur_stop,
            dynamic,
            chord_symbol,
            staff_text,
            lyrics,
            articulations,
        },
    })
}

fn emit_musicxml(score: &Score, parts: &[PartSpec]) -> Result<String, ConvertError> {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<score-partwise version=\"4.0\">\n");

    if let Some(title) = &score.title {
        writeln!(out, "  <work><work-title>{}</work-title></work>", xml_escape(title)).unwrap();
    }
    if let Some(composer) = &score.composer {
        writeln!(out, "  <identification><creator type=\"composer\">{}</creator></identification>", xml_escape(composer)).unwrap();
    }

    out.push_str("  <part-list>\n");
    for part in parts {
        writeln!(out, "    <score-part id=\"{}\">", part.id).unwrap();
        writeln!(out, "      <part-name>{}</part-name>", xml_escape(part.name.as_deref().unwrap_or("Part"))).unwrap();
        if let Some(abbreviation) = &part.abbreviation {
            writeln!(out, "      <part-abbreviation>{}</part-abbreviation>", xml_escape(abbreviation)).unwrap();
        }
        out.push_str("    </score-part>\n");
    }
    out.push_str("  </part-list>\n");

    for part in parts {
        writeln!(out, "  <part id=\"{}\">", part.id).unwrap();
        let measure_count = part
            .staves
            .first()
            .map(|staff| staff.measures.len())
            .unwrap_or(0);

        for measure_index in 0..measure_count {
            writeln!(out, "    <measure number=\"{}\">", measure_index + 1).unwrap();

            if let Some(barline) = part
                .staves
                .iter()
                .find_map(|staff| staff.measures[measure_index].barline_before.clone())
            {
                emit_barline(&mut out, 6, "left", &barline);
            }

            if measure_index == 0 {
                emit_initial_attributes(&mut out, part, score, measure_index)?;
            }

            for (staff_offset, staff) in part.staves.iter().enumerate() {
                let measure = &staff.measures[measure_index];
                let mut voice_numbers: Vec<_> = measure.acc.voices.keys().cloned().collect();
                voice_numbers.sort_by_key(|voice| voice.parse::<u32>().unwrap_or(u32::MAX));

                if voice_numbers.is_empty() {
                    emit_mid_measure_controls(&mut out, &measure.acc.controls, 0, None)?;
                }

                let mut active_ties = BTreeSet::new();
                for (voice_index, voice_number) in voice_numbers.iter().enumerate() {
                    if voice_index > 0 {
                        writeln!(out, "      <backup><duration>{}</duration></backup>", measure.length).unwrap();
                    }
                    let events = measure
                        .acc
                        .voices
                        .get(voice_number)
                        .ok_or_else(|| ConvertError::InvalidScorify("missing voice events".to_string()))?;
                    let controls: &[TimedEvent] = if voice_index == 0 {
                        &measure.acc.controls
                    } else {
                        &[]
                    };
                    emit_voice_xml(
                        &mut out,
                        events,
                        controls,
                        measure.length,
                        voice_index + 1,
                        if part.staves.len() > 1 { Some(staff_offset + 1) } else { None },
                        &mut active_ties,
                    )?;
                }
            }

            let right_barline = part
                .staves
                .iter()
                .find_map(|staff| Some(staff.measures[measure_index].barline_after.clone()))
                .unwrap_or_else(|| "|".to_string());
            if right_barline != "|" {
                emit_barline(&mut out, 6, "right", &right_barline);
            }
            out.push_str("    </measure>\n");
        }

        out.push_str("  </part>\n");
    }

    out.push_str("</score-partwise>\n");
    Ok(out)
}

fn emit_initial_attributes(
    out: &mut String,
    part: &PartSpec,
    score: &Score,
    measure_index: usize,
) -> Result<(), ConvertError> {
    out.push_str("      <attributes>");
    write!(out, "<divisions>{}</divisions>", MUSICXML_DIVISIONS).unwrap();

    if let Some(key) = &score.key {
        let (fifths, mode) = key_to_musicxml(key)?;
        write!(out, "<key><fifths>{}</fifths>", fifths).unwrap();
        if let Some(mode) = mode {
            write!(out, "<mode>{}</mode>", mode).unwrap();
        }
        out.push_str("</key>");
    }

    if let Some(time) = &score.time {
        out.push_str(&time_to_musicxml(time)?);
    }

    if part.staves.len() > 1 {
        write!(out, "<staves>{}</staves>", part.staves.len()).unwrap();
    }

    for staff in &part.staves {
        out.push_str(&clef_to_musicxml(
            &staff.clef,
            if part.staves.len() > 1 {
                Some(staff.staff_number)
            } else {
                None
            },
        )?);
    }

    out.push_str("</attributes>\n");

    for staff in &part.staves {
        emit_mid_measure_controls(out, &staff.measures[measure_index].acc.controls, 0, None)?;
    }
    Ok(())
}

fn emit_voice_xml(
    out: &mut String,
    events: &[TimedEvent],
    controls: &[TimedEvent],
    measure_length: i32,
    voice_number: usize,
    staff_number: Option<usize>,
    active_ties: &mut BTreeSet<String>,
) -> Result<(), ConvertError> {
    let mut cursor = 0;
    let mut control_index = 0;

    let mut sorted_events = events.to_vec();
    sorted_events.sort_by_key(|event| event.start);

    let mut sorted_controls = controls.to_vec();
    sorted_controls.sort_by_key(|event| event.start);

    for event in &sorted_events {
        while control_index < sorted_controls.len() && sorted_controls[control_index].start <= event.start {
            if sorted_controls[control_index].start > cursor {
                writeln!(out, "      <forward><duration>{}</duration></forward>", sorted_controls[control_index].start - cursor).unwrap();
                cursor = sorted_controls[control_index].start;
            }
            emit_control(out, &sorted_controls[control_index], staff_number)?;
            control_index += 1;
        }

        if event.start > cursor {
            writeln!(out, "      <forward><duration>{}</duration></forward>", event.start - cursor).unwrap();
            cursor = event.start;
        }

        emit_event_xml(out, event, voice_number, staff_number, active_ties)?;
        cursor = cursor.max(event.start + event.duration);
    }

    while control_index < sorted_controls.len() {
        if sorted_controls[control_index].start > cursor {
            writeln!(out, "      <forward><duration>{}</duration></forward>", sorted_controls[control_index].start - cursor).unwrap();
            cursor = sorted_controls[control_index].start;
        }
        emit_control(out, &sorted_controls[control_index], staff_number)?;
        control_index += 1;
    }

    if cursor < measure_length {
        writeln!(out, "      <forward><duration>{}</duration></forward>", measure_length - cursor).unwrap();
    }

    Ok(())
}

fn emit_event_xml(
    out: &mut String,
    event: &TimedEvent,
    voice_number: usize,
    staff_number: Option<usize>,
    active_ties: &mut BTreeSet<String>,
) -> Result<(), ConvertError> {
    match &event.token {
        Token::Rest => emit_rest_xml(out, event.duration, voice_number, staff_number),
        Token::Note(note) => emit_note_xml(out, note, event.duration, voice_number, staff_number, active_ties),
        Token::Clef(_) | Token::Time(_) => Ok(()),
    }
}

fn emit_rest_xml(
    out: &mut String,
    duration: i32,
    voice_number: usize,
    staff_number: Option<usize>,
) -> Result<(), ConvertError> {
    let (type_name, dots) = duration_and_type(duration, None, 0)?;
    out.push_str("      <note><rest/>");
    write!(out, "<duration>{}</duration>", duration).unwrap();
    write!(out, "<voice>{}</voice>", voice_number).unwrap();
    write!(out, "<type>{}</type>", type_name).unwrap();
    for _ in 0..dots {
        out.push_str("<dot/>");
    }
    if let Some(staff_number) = staff_number {
        write!(out, "<staff>{}</staff>", staff_number).unwrap();
    }
    out.push_str("</note>\n");
    Ok(())
}

fn emit_note_xml(
    out: &mut String,
    note: &NoteToken,
    duration: i32,
    voice_number: usize,
    staff_number: Option<usize>,
    active_ties: &mut BTreeSet<String>,
) -> Result<(), ConvertError> {
    if let Some(dynamic) = &note.dynamic {
        writeln!(out, "      <direction><direction-type><dynamics><{0}/></dynamics></direction-type></direction>", xml_name(dynamic)).unwrap();
    }
    if let Some(text) = &note.staff_text {
        writeln!(out, "      <direction><direction-type><words>{}</words></direction-type></direction>", xml_escape(text)).unwrap();
    }
    if let Some(chord_symbol) = &note.chord_symbol {
        writeln!(out, "      {}</harmony>", harmony_to_musicxml(chord_symbol)?).unwrap();
    }

    let (type_name, dots) = duration_and_type(duration, note.duration_text.as_deref(), note.dots)?;
    let signature = note_signature(note);
    let tie_stop = note.tie_stop || active_ties.remove(&signature);
    if note.tie_start {
        active_ties.insert(signature);
    }

    for (index, pitch) in note.pitches.iter().enumerate() {
        out.push_str("      <note>");
        if index > 0 {
            out.push_str("<chord/>");
        }
        out.push_str(&pitch_to_musicxml(pitch));
        write!(out, "<duration>{}</duration>", duration).unwrap();
        if note.tie_start {
            out.push_str("<tie type=\"start\"/>");
        }
        if tie_stop {
            out.push_str("<tie type=\"stop\"/>");
        }
        write!(out, "<voice>{}</voice>", voice_number).unwrap();
        if let Some(accidental) = &pitch.accidental_text {
            write!(out, "<accidental>{}</accidental>", xml_escape(accidental)).unwrap();
        }
        write!(out, "<type>{}</type>", type_name).unwrap();
        for _ in 0..dots {
            out.push_str("<dot/>");
        }
        if let Some(staff_number) = staff_number {
            write!(out, "<staff>{}</staff>", staff_number).unwrap();
        }

        if !note.lyrics.is_empty() && index == 0 {
            for lyric in &note.lyrics {
                out.push_str(&lyric_to_musicxml(lyric));
            }
        }

        let notation = note_notations(note, tie_stop);
        if !notation.is_empty() {
            write!(out, "<notations>{}</notations>", notation).unwrap();
        }
        out.push_str("</note>\n");
    }

    Ok(())
}

fn emit_control(out: &mut String, event: &TimedEvent, staff_number: Option<usize>) -> Result<(), ConvertError> {
    match &event.token {
        Token::Clef(clef) => {
            writeln!(out, "      <attributes>{}</attributes>", clef_to_musicxml(clef, staff_number)?).unwrap();
            Ok(())
        }
        Token::Time(time) => {
            writeln!(out, "      <attributes>{}</attributes>", time_to_musicxml(time)?).unwrap();
            Ok(())
        }
        Token::Note(_) | Token::Rest => Ok(()),
    }
}

fn emit_mid_measure_controls(
    out: &mut String,
    controls: &[TimedEvent],
    at: i32,
    staff_number: Option<usize>,
) -> Result<(), ConvertError> {
    for control in controls.iter().filter(|control| control.start == at) {
        emit_control(out, control, staff_number)?;
    }
    Ok(())
}

fn note_notations(note: &NoteToken, tie_stop: bool) -> String {
    let mut out = String::new();
    if note.slur_start {
        out.push_str("<slur type=\"start\"/>");
    }
    if note.slur_stop {
        out.push_str("<slur type=\"stop\"/>");
    }
    if note.tie_start {
        out.push_str("<tied type=\"start\"/>");
    }
    if tie_stop {
        out.push_str("<tied type=\"stop\"/>");
    }
    if !note.articulations.is_empty() {
        out.push_str("<articulations>");
        for articulation in &note.articulations {
            out.push_str(match *articulation {
                ">" => "<accent/>",
                "*" => "<staccato/>",
                "-" => "<tenuto/>",
                "_" => "<fermata/>",
                _ => "",
            });
        }
        out.push_str("</articulations>");
    }
    out
}

fn pitch_to_musicxml(pitch: &PitchToken) -> String {
    let mut out = String::new();
    out.push_str("<pitch>");
    write!(out, "<step>{}</step>", pitch.step.to_ascii_uppercase()).unwrap();
    if pitch.alter != 0 {
        write!(out, "<alter>{}</alter>", pitch.alter).unwrap();
    }
    write!(out, "<octave>{}</octave>", pitch.octave).unwrap();
    out.push_str("</pitch>");
    out
}

fn lyric_to_musicxml(lyric: &LyricToken) -> String {
    let mut out = String::new();
    out.push_str("<lyric");
    if let Some(verse) = lyric.verse {
        write!(out, " number=\"{}\"", verse).unwrap();
    }
    out.push('>');

    let (syllabic, text) = lyric_text_to_musicxml(&lyric.text);
    if let Some(syllabic) = syllabic {
        write!(out, "<syllabic>{}</syllabic>", syllabic).unwrap();
    }
    write!(out, "<text>{}</text>", xml_escape(text)).unwrap();
    if lyric.text.ends_with('_') {
        out.push_str("<extend/>");
    }
    out.push_str("</lyric>");
    out
}

fn lyric_text_to_musicxml(text: &str) -> (Option<&'static str>, &str) {
    if let Some(stripped) = text.strip_suffix('-') {
        (Some("begin"), stripped)
    } else if text.ends_with('_') {
        (Some("single"), text.trim_end_matches('_'))
    } else {
        (Some("single"), text)
    }
}

fn note_signature(note: &NoteToken) -> String {
    note.pitches
        .iter()
        .map(|pitch| format!("{}:{}:{}", pitch.step, pitch.alter, pitch.octave))
        .collect::<Vec<_>>()
        .join("|")
}

fn harmony_to_musicxml(chord: &str) -> Result<String, ConvertError> {
    let (root, suffix, bass) = parse_chord_symbol(chord)?;
    let mut out = String::from("<harmony><root>");
    write!(out, "<root-step>{}</root-step>", root.0).unwrap();
    if root.1 != 0 {
        write!(out, "<root-alter>{}</root-alter>", root.1).unwrap();
    }
    out.push_str("</root>");
    if suffix.is_empty() {
        out.push_str("<kind>major</kind>");
    } else {
        write!(out, "<kind text=\"{}\">other</kind>", xml_escape(&suffix)).unwrap();
    }
    if let Some((step, alter)) = bass {
        out.push_str("<bass>");
        write!(out, "<bass-step>{}</bass-step>", step).unwrap();
        if alter != 0 {
            write!(out, "<bass-alter>{}</bass-alter>", alter).unwrap();
        }
        out.push_str("</bass>");
    }
    Ok(out)
}

fn parse_chord_symbol(
    chord: &str,
) -> Result<((char, i32), String, Option<(char, i32)>), ConvertError> {
    let (main, bass) = chord.split_once('/').map_or((chord, None), |(head, tail)| (head, Some(tail)));
    let (root_step, root_alter, suffix) = parse_chord_root(main)?;
    let bass = bass.map(parse_chord_root).transpose()?.map(|(step, alter, _)| (step, alter));
    Ok(((root_step, root_alter), suffix, bass))
}

fn parse_chord_root(value: &str) -> Result<(char, i32, String), ConvertError> {
    let mut chars = value.chars();
    let step = chars
        .next()
        .filter(|ch| ch.is_ascii_alphabetic())
        .ok_or_else(|| ConvertError::InvalidScorify(format!("invalid chord symbol '{value}'")))?
        .to_ascii_uppercase();
    let rest = chars.as_str();
    let (alter, suffix) = if let Some(rest) = rest.strip_prefix("##") {
        (2, rest)
    } else if let Some(rest) = rest.strip_prefix("bb") {
        (-2, rest)
    } else if let Some(rest) = rest.strip_prefix('#') {
        (1, rest)
    } else if let Some(rest) = rest.strip_prefix('b') {
        (-1, rest)
    } else {
        (0, rest)
    };
    Ok((step, alter, suffix.to_string()))
}

fn split_voice_token(token: &str) -> Result<(String, String), ConvertError> {
    let inner = token
        .strip_prefix("v{")
        .and_then(|value| value.strip_suffix('}'))
        .ok_or_else(|| ConvertError::InvalidScorify(format!("invalid multi-voice token '{token}'")))?;
    let mut depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut angle_depth = 0usize;
    let mut escape = false;

    for (idx, ch) in inner.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' => escape = true,
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '<' => angle_depth += 1,
            '>' => angle_depth = angle_depth.saturating_sub(1),
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ';' if depth == 0 && bracket_depth == 0 && angle_depth == 0 => {
                return Ok((inner[..idx].to_string(), inner[idx + 1..].to_string()))
            }
            _ => {}
        }
    }

    Err(ConvertError::InvalidScorify(format!(
        "multi-voice token is missing ';' in '{token}'"
    )))
}

fn split_top_level_tokens(source: &str) -> Result<Vec<String>, ConvertError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut bracket_depth = 0usize;
    let mut angle_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut escape = false;

    for ch in source.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' => {
                current.push(ch);
                escape = true;
            }
            '[' => {
                bracket_depth += 1;
                current.push(ch);
            }
            ']' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                current.push(ch);
            }
            '<' => {
                angle_depth += 1;
                current.push(ch);
            }
            '>' => {
                angle_depth = angle_depth.saturating_sub(1);
                current.push(ch);
            }
            '{' => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(ch);
            }
            ch if ch.is_whitespace() && bracket_depth == 0 && angle_depth == 0 && brace_depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if bracket_depth != 0 || angle_depth != 0 || brace_depth != 0 {
        return Err(ConvertError::InvalidScorify(
            "unterminated grouping in music string".to_string(),
        ));
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

fn is_barline_token(token: &str) -> bool {
    matches!(token, "|" | "||" | "|." | "|:" | ":|")
}

fn is_clef_token(token: &str) -> bool {
    matches!(
        token,
        "treble"
            | "bass"
            | "alto"
            | "tenor"
            | "percussion"
            | "treble-8a"
            | "treble-8b"
            | "treble-15a"
            | "treble-15b"
            | "bass-8a"
            | "bass-8b"
            | "bass-15a"
            | "bass-15b"
    )
}

fn is_time_token(token: &str) -> bool {
    token == "common"
        || token == "cut"
        || token.split_once('/').is_some_and(|(beats, beat_type)| {
            beats.chars().all(|ch| ch.is_ascii_digit())
                && beat_type.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn pitch_token_len(token: &str) -> Result<usize, ConvertError> {
    let mut chars = token.char_indices();
    let Some((_, step)) = chars.next() else {
        return Err(ConvertError::InvalidScorify("empty note token".to_string()));
    };
    if !step.is_ascii_alphabetic() {
        return Err(ConvertError::InvalidScorify(format!("invalid note token '{token}'")));
    }

    let mut end = step.len_utf8();
    let rest = &token[end..];
    if let Some(rest) = rest.strip_prefix("##") {
        end = token.len() - rest.len();
    } else if let Some(rest) = rest.strip_prefix("&&") {
        end = token.len() - rest.len();
    } else if let Some(rest) = rest.strip_prefix('#') {
        end = token.len() - rest.len();
    } else if let Some(rest) = rest.strip_prefix('&') {
        end = token.len() - rest.len();
    } else if let Some(rest) = rest.strip_prefix('=') {
        end = token.len() - rest.len();
    }

    for (index, ch) in token[end..].char_indices() {
        if matches!(ch, '\'' | ',') {
            end += index + ch.len_utf8();
        } else {
            break;
        }
    }

    Ok(end)
}

fn parse_pitch(token: &str, clef: &str) -> Result<PitchToken, ConvertError> {
    let mut chars = token.chars();
    let step = chars
        .next()
        .filter(|ch| ch.is_ascii_alphabetic())
        .ok_or_else(|| ConvertError::InvalidScorify(format!("invalid note pitch '{token}'")))?
        .to_ascii_lowercase();
    let rest = chars.as_str();
    let (alter, accidental_text, rest) = if let Some(rest) = rest.strip_prefix("##") {
        (2, Some("double-sharp".to_string()), rest)
    } else if let Some(rest) = rest.strip_prefix("&&") {
        (-2, Some("double-flat".to_string()), rest)
    } else if let Some(rest) = rest.strip_prefix('#') {
        (1, Some("sharp".to_string()), rest)
    } else if let Some(rest) = rest.strip_prefix('&') {
        (-1, Some("flat".to_string()), rest)
    } else if let Some(rest) = rest.strip_prefix('=') {
        (0, Some("natural".to_string()), rest)
    } else {
        (0, None, rest)
    };

    let mut octave = clef_base_octave(clef);
    for ch in rest.chars() {
        match ch {
            '\'' => octave += 1,
            ',' => octave -= 1,
            _ => {
                return Err(ConvertError::InvalidScorify(format!(
                    "invalid pitch octave suffix in '{token}'"
                )))
            }
        }
    }

    Ok(PitchToken {
        step,
        alter,
        accidental_text,
        octave,
    })
}

fn parse_duration(input: &str) -> Result<(String, usize, &str), ConvertError> {
    let mut rest = input;
    let duration_text = if let Some(next) = rest.strip_prefix("maxima") {
        rest = next;
        "maxima".to_string()
    } else if let Some(next) = rest.strip_prefix("longa") {
        rest = next;
        "longa".to_string()
    } else if let Some(next) = rest.strip_prefix("breve") {
        rest = next;
        "breve".to_string()
    } else {
        let digits = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() {
            return Err(ConvertError::InvalidScorify(format!(
                "missing duration in token fragment '{input}'"
            )));
        }
        rest = &rest[digits.len()..];
        digits
    };

    let dots = rest.chars().take_while(|ch| *ch == '.').count();
    rest = &rest[dots..];
    Ok((duration_text, dots, rest))
}

fn parse_bracket_text(input: &str) -> Result<(String, &str), ConvertError> {
    let mut value = String::new();
    let mut escape = false;
    for (idx, ch) in input.char_indices() {
        if escape {
            value.push(ch);
            escape = false;
            continue;
        }
        match ch {
            '\\' => escape = true,
            ']' => return Ok((value, &input[idx + 1..])),
            _ => value.push(ch),
        }
    }
    Err(ConvertError::InvalidScorify(
        "unterminated bracketed attachment".to_string(),
    ))
}

fn duration_to_ticks(duration_text: &str, dots: usize) -> Result<i32, ConvertError> {
    let whole = 4 * MUSICXML_DIVISIONS;
    let base = match duration_text {
        "maxima" => whole * 8,
        "longa" => whole * 4,
        "breve" => whole * 2,
        other => {
            let denom = other.parse::<i32>().map_err(|_| {
                ConvertError::InvalidScorify(format!("unsupported duration '{duration_text}'"))
            })?;
            if denom <= 0 || whole % denom != 0 {
                return Err(ConvertError::InvalidScorify(format!(
                    "unsupported duration '{duration_text}'"
                )));
            }
            whole / denom
        }
    };

    let numerator = (1 << (dots + 1)) - 1;
    let denominator = 1 << dots;
    Ok(base * numerator as i32 / denominator as i32)
}

fn duration_and_type(
    duration: i32,
    duration_text: Option<&str>,
    dots: usize,
) -> Result<(String, usize), ConvertError> {
    if let Some(duration_text) = duration_text {
        return Ok((duration_text_to_type(duration_text).to_string(), dots));
    }

    let whole = 4 * MUSICXML_DIVISIONS;
    for (text, type_name) in [
        ("maxima", "maxima"),
        ("longa", "long"),
        ("breve", "breve"),
        ("1", "whole"),
        ("2", "half"),
        ("4", "quarter"),
        ("8", "eighth"),
        ("16", "16th"),
        ("32", "32nd"),
        ("64", "64th"),
        ("128", "128th"),
    ] {
        let base = match text {
            "maxima" => whole * 8,
            "longa" => whole * 4,
            "breve" => whole * 2,
            other => whole / other.parse::<i32>().unwrap_or(1),
        };
        for extra_dots in 0..=3usize {
            let numerator = ((1 << (extra_dots + 1)) - 1) as i32;
            let denominator = (1 << extra_dots) as i32;
            if base * numerator / denominator == duration {
                return Ok((type_name.to_string(), extra_dots));
            }
        }
    }

    Err(ConvertError::InvalidScorify(format!(
        "could not map duration value {duration} to a MusicXML note type"
    )))
}

fn duration_text_to_type(value: &str) -> &'static str {
    match value {
        "maxima" => "maxima",
        "longa" => "long",
        "breve" => "breve",
        "1" => "whole",
        "2" => "half",
        "4" => "quarter",
        "8" => "eighth",
        "16" => "16th",
        "32" => "32nd",
        "64" => "64th",
        "128" => "128th",
        _ => "quarter",
    }
}

fn key_to_musicxml(key: &str) -> Result<(i32, Option<&'static str>), ConvertError> {
    const MAJOR: [(&str, i32); 15] = [
        ("Cb", -7),
        ("Gb", -6),
        ("Db", -5),
        ("Ab", -4),
        ("Eb", -3),
        ("Bb", -2),
        ("F", -1),
        ("C", 0),
        ("G", 1),
        ("D", 2),
        ("A", 3),
        ("E", 4),
        ("B", 5),
        ("F#", 6),
        ("C#", 7),
    ];
    const MINOR: [(&str, i32); 15] = [
        ("ab", -7),
        ("eb", -6),
        ("bb", -5),
        ("f", -4),
        ("c", -3),
        ("g", -2),
        ("d", -1),
        ("a", 0),
        ("e", 1),
        ("b", 2),
        ("f#", 3),
        ("c#", 4),
        ("g#", 5),
        ("d#", 6),
        ("a#", 7),
    ];

    if let Some((_, fifths)) = MAJOR.iter().find(|(candidate, _)| *candidate == key) {
        return Ok((*fifths, None));
    }
    if let Some((_, fifths)) = MINOR.iter().find(|(candidate, _)| *candidate == key) {
        return Ok((*fifths, Some("minor")));
    }

    Err(ConvertError::InvalidScorify(format!("unsupported key '{key}'")))
}

fn time_to_musicxml(time: &str) -> Result<String, ConvertError> {
    if time == "common" {
        return Ok("<time symbol=\"common\"><beats>4</beats><beat-type>4</beat-type></time>".to_string());
    }
    if time == "cut" {
        return Ok("<time symbol=\"cut\"><beats>2</beats><beat-type>2</beat-type></time>".to_string());
    }
    let (beats, beat_type) = time.split_once('/').ok_or_else(|| {
        ConvertError::InvalidScorify(format!("unsupported time signature '{time}'"))
    })?;
    Ok(format!(
        "<time><beats>{}</beats><beat-type>{}</beat-type></time>",
        xml_escape(beats),
        xml_escape(beat_type)
    ))
}

fn clef_to_musicxml(clef: &str, number: Option<usize>) -> Result<String, ConvertError> {
    let mut out = String::from("<clef");
    if let Some(number) = number {
        write!(out, " number=\"{}\"", number).unwrap();
    }
    out.push('>');
    match clef {
        "treble" => out.push_str("<sign>G</sign><line>2</line>"),
        "bass" => out.push_str("<sign>F</sign><line>4</line>"),
        "alto" => out.push_str("<sign>C</sign><line>3</line>"),
        "tenor" => out.push_str("<sign>C</sign><line>4</line>"),
        "percussion" => out.push_str("<sign>percussion</sign><line>2</line>"),
        "treble-8a" => out.push_str("<sign>G</sign><line>2</line><clef-octave-change>1</clef-octave-change>"),
        "treble-8b" => out.push_str("<sign>G</sign><line>2</line><clef-octave-change>-1</clef-octave-change>"),
        "treble-15a" => out.push_str("<sign>G</sign><line>2</line><clef-octave-change>2</clef-octave-change>"),
        "treble-15b" => out.push_str("<sign>G</sign><line>2</line><clef-octave-change>-2</clef-octave-change>"),
        "bass-8a" => out.push_str("<sign>F</sign><line>4</line><clef-octave-change>1</clef-octave-change>"),
        "bass-8b" => out.push_str("<sign>F</sign><line>4</line><clef-octave-change>-1</clef-octave-change>"),
        "bass-15a" => out.push_str("<sign>F</sign><line>4</line><clef-octave-change>2</clef-octave-change>"),
        "bass-15b" => out.push_str("<sign>F</sign><line>4</line><clef-octave-change>-2</clef-octave-change>"),
        _ => {
            return Err(ConvertError::InvalidScorify(format!(
                "unsupported clef '{clef}'"
            )))
        }
    }
    out.push_str("</clef>");
    Ok(out)
}

fn emit_barline(out: &mut String, indent: usize, location: &str, barline: &str) {
    let prefix = " ".repeat(indent);
    match barline {
        "|:" => {
            writeln!(out, "{prefix}<barline location=\"{location}\"><bar-style>heavy-light</bar-style><repeat direction=\"forward\"/></barline>").unwrap();
        }
        ":|" => {
            writeln!(out, "{prefix}<barline location=\"{location}\"><bar-style>light-heavy</bar-style><repeat direction=\"backward\"/></barline>").unwrap();
        }
        "||" => {
            writeln!(out, "{prefix}<barline location=\"{location}\"><bar-style>light-light</bar-style></barline>").unwrap();
        }
        "|." => {
            writeln!(out, "{prefix}<barline location=\"{location}\"><bar-style>light-heavy</bar-style></barline>").unwrap();
        }
        _ => {}
    }
}

fn required_string(values: &BTreeMap<String, TypstValue>, key: &str) -> Result<String, ConvertError> {
    optional_string(values, key)?.ok_or_else(|| {
        ConvertError::InvalidScorify(format!("missing required score field '{key}'"))
    })
}

fn optional_string(values: &BTreeMap<String, TypstValue>, key: &str) -> Result<Option<String>, ConvertError> {
    match values.get(key) {
        Some(TypstValue::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(ConvertError::InvalidScorify(format!(
            "field '{key}' must be a string"
        ))),
        None => Ok(None),
    }
}

fn optional_bool(values: &BTreeMap<String, TypstValue>, key: &str) -> Result<Option<bool>, ConvertError> {
    match values.get(key) {
        Some(TypstValue::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(ConvertError::InvalidScorify(format!(
            "field '{key}' must be a boolean"
        ))),
        None => Ok(None),
    }
}

fn optional_number(values: &BTreeMap<String, TypstValue>, key: &str) -> Result<Option<i64>, ConvertError> {
    match values.get(key) {
        Some(TypstValue::Number(value)) => Ok(Some(*value)),
        Some(_) => Err(ConvertError::InvalidScorify(format!(
            "field '{key}' must be a number"
        ))),
        None => Ok(None),
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn xml_name(value: &str) -> &str {
    match value {
        "pppppp" | "ppppp" | "pppp" | "ppp" | "pp" | "p" | "mp" | "mf" | "f" | "ff"
        | "fff" | "ffff" | "fp" | "sf" | "sfp" | "sfz" | "rfz" => value,
        _ => "mf",
    }
}

struct TypstParser<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> TypstParser<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, position: 0 }
    }

    fn skip_ws_comments(&mut self) {
        loop {
            let rest = &self.source[self.position..];
            if rest.starts_with("//") {
                if let Some(offset) = rest.find('\n') {
                    self.position += offset + 1;
                } else {
                    self.position = self.source.len();
                }
                continue;
            }

            let next = rest.chars().next();
            if next.is_some_and(char::is_whitespace) {
                self.position += next.unwrap_or_default().len_utf8();
                continue;
            }
            break;
        }
    }

    fn expect(&mut self, ch: char) -> Result<(), ConvertError> {
        self.skip_ws_comments();
        let rest = &self.source[self.position..];
        if rest.starts_with(ch) {
            self.position += ch.len_utf8();
            Ok(())
        } else {
            Err(ConvertError::InvalidScorify(format!(
                "expected '{ch}' near '{}'",
                preview(rest)
            )))
        }
    }

    fn parse_named_args(&mut self, terminator: char) -> Result<BTreeMap<String, TypstValue>, ConvertError> {
        let mut out = BTreeMap::new();
        loop {
            self.skip_ws_comments();
            if self.source[self.position..].starts_with(terminator) {
                break;
            }
            let key = self.parse_identifier()?;
            self.skip_ws_comments();
            self.expect(':')?;
            let value = self.parse_value()?;
            out.insert(key, value);
            self.skip_ws_comments();
            if self.source[self.position..].starts_with(',') {
                self.position += 1;
            }
        }
        Ok(out)
    }

    fn parse_value(&mut self) -> Result<TypstValue, ConvertError> {
        self.skip_ws_comments();
        let rest = &self.source[self.position..];
        if rest.starts_with('"') {
            return self.parse_string().map(TypstValue::String);
        }
        if rest.starts_with('(') {
            return self.parse_tuple_like();
        }
        if rest.starts_with("true") {
            self.position += 4;
            return Ok(TypstValue::Bool(true));
        }
        if rest.starts_with("false") {
            self.position += 5;
            return Ok(TypstValue::Bool(false));
        }

        let number = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if !number.is_empty() {
            self.position += number.len();
            return Ok(TypstValue::Number(number.parse::<i64>().map_err(|error| {
                ConvertError::InvalidScorify(format!("invalid number '{number}': {error}"))
            })?));
        }

        Err(ConvertError::InvalidScorify(format!(
            "unsupported Typst value near '{}'",
            preview(rest)
        )))
    }

    fn parse_tuple_like(&mut self) -> Result<TypstValue, ConvertError> {
        self.expect('(')?;
        self.skip_ws_comments();
        if self.source[self.position..].starts_with(')') {
            self.position += 1;
            return Ok(TypstValue::Tuple(Vec::new()));
        }

        if self.looks_like_named_arg() {
            let object = self.parse_named_args(')')?;
            self.expect(')')?;
            return Ok(TypstValue::Object(object));
        }

        let mut values = Vec::new();
        loop {
            values.push(self.parse_value()?);
            self.skip_ws_comments();
            if self.source[self.position..].starts_with(',') {
                self.position += 1;
                self.skip_ws_comments();
                if self.source[self.position..].starts_with(')') {
                    break;
                }
                continue;
            }
            break;
        }
        self.expect(')')?;
        Ok(TypstValue::Tuple(values))
    }

    fn looks_like_named_arg(&self) -> bool {
        let mut offset = 0;
        let rest = &self.source[self.position..];
        let identifier = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
            .collect::<String>();
        if identifier.is_empty() {
            return false;
        }
        offset += identifier.len();
        let rest = &rest[offset..];
        let whitespace = rest.chars().take_while(|ch| ch.is_whitespace()).collect::<String>();
        offset += whitespace.len();
        self.source[self.position + offset..].starts_with(':')
    }

    fn parse_identifier(&mut self) -> Result<String, ConvertError> {
        self.skip_ws_comments();
        let rest = &self.source[self.position..];
        let identifier = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
            .collect::<String>();
        if identifier.is_empty() {
            return Err(ConvertError::InvalidScorify(format!(
                "expected identifier near '{}'",
                preview(rest)
            )));
        }
        self.position += identifier.len();
        Ok(identifier)
    }

    fn parse_string(&mut self) -> Result<String, ConvertError> {
        self.expect('"')?;
        let mut out = String::new();
        let mut escape = false;
        while self.position < self.source.len() {
            let ch = self.source[self.position..]
                .chars()
                .next()
                .ok_or_else(|| ConvertError::InvalidScorify("unterminated string".to_string()))?;
            self.position += ch.len_utf8();
            if escape {
                out.push(match ch {
                    'n' => '\n',
                    '"' => '"',
                    '\\' => '\\',
                    other => other,
                });
                escape = false;
                continue;
            }
            match ch {
                '\\' => escape = true,
                '"' => return Ok(out),
                other => out.push(other),
            }
        }
        Err(ConvertError::InvalidScorify("unterminated string".to_string()))
    }
}

fn preview(value: &str) -> String {
    value.chars().take(32).collect()
}