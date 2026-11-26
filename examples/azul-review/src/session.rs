use azul::{
    json::Json,
    vec::{U8Vec, U8VecRef},
    zip::Zip,
};

use crate::{
    deletion::{self, Deleted, Deletion},
    model::{Finding, InkPoint, Note, NoteState, Semantic, Stroke, VoiceClip},
    ui, AppState,
};

/// The format number of the archive. It changes only when an older reader would misread a file:
/// the keys a version adds (`epoch` on a stroke, `notes`) are ignored by one that does not know them
/// and absent from a file written before them, which is why neither changed it.
const FORMAT: &str = "azreview/1";

fn bytes(v: &[u8]) -> U8Vec {
    v.first().map_or_else(U8Vec::create, |first| {
        U8Vec::copy_from_bytes(first, 0, v.len())
    })
}

/// A JSON string literal of `text`, escaped (quotes, backslashes, control characters; the rest as it is).
fn push_string(out: &mut String, text: &str) {
    use std::fmt::Write;
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A JSON number of `value`, rounded to 0.01 (a pen does not report more), as short as it reads back
/// the same (`120.5`, `0.55`, `3.0`); what is no number (NaN, infinity) is 0.
fn push_num(out: &mut String, value: f32) {
    use std::fmt::Write;
    let rounded = if value.is_finite() {
        (f64::from(value) * 100.0).round() / 100.0
    } else {
        0.0
    };
    if rounded.fract() == 0.0 {
        let _ = write!(out, "{rounded:.1}");
    } else {
        let _ = write!(out, "{rounded}");
    }
}

fn push_key(out: &mut String, key: &str) {
    push_string(out, key);
    out.push(':');
}

/// The samples of a stroke as a JSON list: where, how hard, how the pen was tilted and when (ms after the press).
fn push_points(out: &mut String, points: &[InkPoint]) {
    use std::fmt::Write;
    out.push('[');
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"x\":");
        push_num(out, p.x);
        out.push_str(",\"y\":");
        push_num(out, p.y);
        out.push_str(",\"pressure\":");
        push_num(out, p.pressure);
        out.push_str(",\"tilt_x\":");
        push_num(out, p.tilt_x);
        out.push_str(",\"tilt_y\":");
        push_num(out, p.tilt_y);
        let _ = write!(out, ",\"t\":{}", p.t_ms);
        out.push('}');
    }
    out.push(']');
}

fn push_stroke(out: &mut String, s: &Stroke) {
    use std::fmt::Write;
    let _ = write!(
        out,
        "{{\"id\":{},\"page\":{},\"epoch\":{},\"started\":{},\"semantic\":",
        s.id, s.page, s.epoch, s.started_unix_ms
    );
    push_string(out, s.semantic.label());
    out.push_str(",\"points\":");
    push_points(out, &s.points);
    out.push('}');
}

/// A deletion as the archive keeps it: the stroke that did it (its samples, with their times) and the words it took, each
/// with its line, its bytes and what it said.
fn push_deletion(out: &mut String, d: &Deletion) {
    use std::fmt::Write;
    let _ = write!(out, "{{\"id\":{},\"page\":{},\"started\":{},\"points\":", d.id, d.page, d.started_unix_ms);
    push_points(out, &d.points);
    out.push_str(",\"words\":[");
    for (i, w) in d.words.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"line\":{},\"start\":{},\"end\":{},\"text\":", w.line, w.start, w.end);
        push_string(out, &w.text);
        out.push('}');
    }
    out.push_str("]}");
}

/// A note as the archive keeps it: its anchor on the page, the line of the file that is under it
/// (for a reader that has no pages), the words and the language they were recognised in.
fn push_note(out: &mut String, n: &Note, first_line_of_page: usize) {
    use std::fmt::Write;
    let _ = write!(out, "{{\"id\":{},\"page\":{},\"x\":", n.id, n.page);
    push_num(out, n.x);
    out.push_str(",\"y\":");
    push_num(out, n.y);
    let _ = write!(out, ",\"line\":{},\"language\":", first_line_of_page + ui::line_of(n.y));
    push_string(out, &n.language);
    out.push_str(",\"text\":");
    push_string(out, &n.text);
    out.push('}');
}

fn push_finding(out: &mut String, f: &Finding) {
    use std::fmt::Write;
    out.push_str("{\"semantic\":");
    push_string(out, f.semantic.label());
    out.push_str(",\"file\":");
    push_string(out, &f.file);
    let _ = write!(
        out,
        ",\"first_line\":{},\"last_line\":{},\"stroke_count\":{}",
        f.first_line, f.last_line, f.stroke_count
    );
    if let Some(v) = &f.voice_note {
        out.push_str(",\"voice_note\":");
        push_string(out, v);
    }
    out.push('}');
}

fn push_clip(out: &mut String, index: usize, c: &VoiceClip) {
    use std::fmt::Write;
    out.push_str("{\"audio\":");
    push_string(out, &wav_name(index));
    let _ = write!(out, ",\"sample_rate\":{},\"samples\":{},\"stroke_ids\":[", c.sample_rate, c.samples.len());
    for (i, id) in c.stroke_ids.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{id}");
    }
    out.push_str("]}");
}

/// `items` as a JSON list, an item to a line (a stroke is a long line, but a diff of two sessions
/// still shows which strokes differ).
fn push_list<T>(out: &mut String, items: impl Iterator<Item = T>, mut push: impl FnMut(&mut String, T)) {
    out.push('[');
    for (i, item) in items.enumerate() {
        out.push_str(if i > 0 { ",\n  " } else { "\n  " });
        push(out, item);
    }
    out.push_str("\n]");
}

/// The text of `session.json`: the strokes, the findings, the notes, the clips. Written as text, not
/// built as azul `Json` values: every number of every point crossed the C boundary as a value of its
/// own, and a session of ten thousand points took a frame to build every time the pen lifted.
fn session_text(s: &AppState, clips: &[&VoiceClip]) -> String {
    let mut out = String::with_capacity(256 + s.strokes.iter().map(|st| st.points.len()).sum::<usize>() * 72);
    out.push_str("{\n");
    push_key(&mut out, "format");
    push_string(&mut out, FORMAT);
    out.push_str(",\n");
    push_key(&mut out, "root");
    push_string(&mut out, &s.root.display().to_string());
    out.push_str(",\n");
    push_key(&mut out, "file");
    push_string(&mut out, s.file().map_or("", |f| f.display.as_str()));
    out.push_str(",\n");
    push_key(&mut out, "strokes");
    push_list(&mut out, s.strokes.iter(), push_stroke);
    out.push_str(",\n");
    push_key(&mut out, "slashes");
    push_list(&mut out, s.slashes.iter(), push_deletion);
    out.push_str(",\n");
    push_key(&mut out, "findings");
    push_list(&mut out, s.findings.iter(), push_finding);
    out.push_str(",\n");
    push_key(&mut out, "notes");
    let first_line = |page: usize| s.file().map_or(1, |f| f.page(page).0);
    push_list(&mut out, s.notes.iter(), |o, n| push_note(o, n, first_line(n.page)));
    out.push_str(",\n");
    push_key(&mut out, "clips");
    push_list(&mut out, clips.iter().enumerate(), |o, (i, c)| push_clip(o, i, c));
    out.push_str("\n}\n");
    out
}

fn wav_name(index: usize) -> String {
    format!("clip-{index}.wav")
}

/// A mono voice clip as a 16-bit PCM WAV (azul's one WAV writer).
fn wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    azul::widgets::Waveform::encode_wav(azul::vec::F32Vec::from(samples.to_vec()), sample_rate, 1)
        .as_ref()
        .to_vec()
}

/// How hard the archive is deflated: lightly. It is built on the window's thread every time the pen
/// lifts, and its size does not matter on a local disk.
const ZIP_LEVEL: u8 = 1;

/// The session as an archive: its file name ([`archive_name`]) and its
/// bytes - every voice clip as a WAV and `session.json` (the strokes, the
/// findings, the notes, the clips). No disk here: the caller writes it into the data
/// tree on a Thread (`crate::save_session`).
#[must_use]
pub fn archive(s: &AppState) -> (String, Vec<u8>) {
    let mut zip = Zip::create();

    let clips: Vec<&VoiceClip> = s.clips.iter().chain(s.recording.iter()).collect();
    for (i, c) in clips.iter().enumerate() {
        zip.add_file(wav_name(i), bytes(&wav(&c.samples, c.sample_rate)));
    }
    zip.add_file("session.json", bytes(session_text(s, &clips).as_bytes()));

    (
        archive_name(s.file().map(|f| f.display.as_str())),
        zip.to_bytes_with_level(ZIP_LEVEL).as_slice().to_vec(),
    )
}

/// The archive's file name for the file under review (`src/lib.rs` ->
/// `src_lib.rs.azreview.zip`; no file: `session.azreview.zip`).
#[must_use]
pub fn archive_name(display: Option<&str>) -> String {
    let stem = display
        .filter(|d| !d.is_empty())
        .map_or_else(|| "session".to_string(), |d| d.replace(['/', '\\'], "_"));
    format!("{stem}.azreview.zip")
}

/// What an archive holds, as the app takes it back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loaded {
    /// The file under review, as the archive names it.
    pub file: String,
    pub strokes: Vec<Stroke>,
    pub notes: Vec<Note>,
    /// The words the quick slashes took.
    pub slashes: Vec<Deletion>,
}

impl Loaded {
    /// The id the next stroke takes (after the archive's own strokes and slashes: they are one series).
    #[must_use]
    pub fn next_stroke_id(&self) -> u64 {
        self.strokes
            .iter()
            .map(|s| s.id)
            .chain(self.slashes.iter().map(|d| d.id))
            .max()
            .map_or(1, |id| id + 1)
    }

    /// The id the next note takes (after the archive's own).
    #[must_use]
    pub fn next_note_id(&self) -> u64 {
        self.notes.iter().map(|n| n.id).max().map_or(1, |id| id + 1)
    }
}

/// The value under `key` of the object `json`.
fn key(json: &Json, key: &str) -> Option<Json> {
    json.get_key(key).into_option()
}

/// A number under `key` (an integer counts: `1` and `1.0` are the same number to a reader).
fn num(json: &Json, name: &str) -> Option<f64> {
    let v = key(json, name)?;
    v.as_float()
        .into_option()
        .or_else(|| v.as_int().into_option().map(|i| i as f64))
}

/// A whole number under `key`, not below 0.
fn count(json: &Json, name: &str) -> Option<u64> {
    let v = key(json, name)?;
    let i = v
        .as_int()
        .into_option()
        .or_else(|| v.as_float().into_option().map(|f| f as i64))?;
    u64::try_from(i).ok()
}

fn text_of(json: &Json, name: &str) -> Option<String> {
    key(json, name)?
        .as_string()
        .into_option()
        .map(|s| s.as_str().to_string())
}

/// The items of the list under `key` (none when it is missing or no list).
fn items(json: &Json, name: &str) -> Vec<Json> {
    key(json, name)
        .and_then(|list| list.to_array().into_option())
        .map(|list| list.as_slice().to_vec())
        .unwrap_or_default()
}

fn point_from(json: &Json) -> Option<InkPoint> {
    Some(InkPoint {
        x: num(json, "x")? as f32,
        y: num(json, "y")? as f32,
        pressure: num(json, "pressure").map_or(0.55, |p| p as f32),
        tilt_x: num(json, "tilt_x").map_or(0.0, |t| t as f32),
        tilt_y: num(json, "tilt_y").map_or(0.0, |t| t as f32),
        t_ms: count(json, "t").map_or(0, |t| u32::try_from(t).unwrap_or(u32::MAX)),
    })
}

fn stroke_from(json: &Json) -> Option<Stroke> {
    Some(Stroke {
        page: usize::try_from(count(json, "page")?).ok()?,
        semantic: Semantic::from_label(&text_of(json, "semantic")?)?,
        points: items(json, "points").iter().filter_map(point_from).collect(),
        id: count(json, "id")?,
        epoch: count(json, "epoch").unwrap_or(0),
        started_unix_ms: count(json, "started").unwrap_or(0),
    })
}

fn word_from(json: &Json) -> Option<Deleted> {
    Some(Deleted {
        line: usize::try_from(count(json, "line")?).ok()?,
        start: usize::try_from(count(json, "start")?).ok()?,
        end: usize::try_from(count(json, "end")?).ok()?,
        text: text_of(json, "text").unwrap_or_default(),
    })
}

/// A deletion needs its id and its page; the words and samples that cannot be read are left out.
fn deletion_from(json: &Json) -> Option<Deletion> {
    Some(Deletion {
        id: count(json, "id")?,
        page: usize::try_from(count(json, "page")?).ok()?,
        started_unix_ms: count(json, "started").unwrap_or(0),
        points: items(json, "points")
            .iter()
            .filter_map(point_from)
            .take(deletion::MAX_POINTS)
            .collect(),
        words: items(json, "words").iter().filter_map(word_from).collect(),
    })
}

/// A note comes back finished: whatever listened when the archive was written is gone.
fn note_from(json: &Json) -> Option<Note> {
    Some(Note {
        id: count(json, "id")?,
        page: usize::try_from(count(json, "page")?).ok()?,
        x: num(json, "x")? as f32,
        y: num(json, "y")? as f32,
        text: text_of(json, "text").unwrap_or_default(),
        language: text_of(json, "language").unwrap_or_default(),
        state: NoteState::Done,
    })
}

/// Reads an archive [`archive`] wrote, or an older one; the keys a later version adds are ignored,
/// a stroke or a note that cannot be read is left out and the rest kept.
///
/// # Errors
/// A sentence for the user when the bytes are no archive of this app, or of a format this build does
/// not read.
pub fn load(archive: &[u8]) -> Result<Loaded, String> {
    let zip = Zip::from_bytes(bytes(archive));
    if !zip.is_valid() {
        return Err("this is not a session archive (it is no zip file)".to_string());
    }
    if !zip.contains("session.json") {
        return Err("the session archive has no session.json".to_string());
    }
    let raw = zip.get_file("session.json");
    let json = Json::parse_bytes(U8VecRef::from(raw.as_slice()))
        .into_result()
        .map_err(|_| "session.json in the archive is not valid JSON".to_string())?;
    if let Some(format) = text_of(&json, "format").filter(|f| f != FORMAT) {
        return Err(format!(
            "the session archive is of format {format}, which this version of AzReview does not read"
        ));
    }
    Ok(Loaded {
        file: text_of(&json, "file").unwrap_or_default(),
        strokes: items(&json, "strokes").iter().filter_map(stroke_from).collect(),
        notes: items(&json, "notes").iter().filter_map(note_from).collect(),
        slashes: items(&json, "slashes")
            .iter()
            .filter_map(deletion_from)
            .take(deletion::MAX_DELETIONS)
            .collect(),
    })
}

#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_archive_is_named_after_the_file_under_review() {
        assert_eq!(archive_name(Some("src/lib.rs")), "src_lib.rs.azreview.zip");
        assert_eq!(archive_name(Some("a\\b.rs")), "a_b.rs.azreview.zip");
        assert_eq!(archive_name(None), "session.azreview.zip");
        assert_eq!(archive_name(Some("")), "session.azreview.zip");
    }

    #[test]
    fn a_wav_header_declares_the_sample_count_it_actually_carries() {
        let w = wav(&[0.0, 0.5, -0.5, 1.0], 48_000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert_eq!(&w[36..40], b"data");
        let declared = u32::from_le_bytes([w[40], w[41], w[42], w[43]]) as usize;
        assert_eq!(declared, 8, "4 samples x 2 bytes");
        assert_eq!(w.len(), 44 + declared);
    }

    #[test]
    fn a_peak_above_full_scale_clips_instead_of_wrapping() {
        let w = wav(&[2.0, -2.0], 48_000);
        let a = i16::from_le_bytes([w[44], w[45]]);
        let b = i16::from_le_bytes([w[46], w[47]]);
        assert!(a > 32_000, "positive peak wrapped: {a}");
        assert!(b < -32_000, "negative peak wrapped: {b}");
    }
}
