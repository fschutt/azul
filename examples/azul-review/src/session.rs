use azul::{
    json::{Json, JsonKeyValue},
    vec::{JsonKeyValueVec, JsonVec, U8Vec, U8VecRef},
    zip::Zip,
};

use crate::{
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

fn obj(entries: Vec<JsonKeyValue>) -> Json {
    let vec = entries
        .first()
        .map_or_else(JsonKeyValueVec::create, |first| {
            JsonKeyValueVec::copy_from_array(first, entries.len())
        });
    Json::object(vec)
}

fn arr(items: Vec<Json>) -> Json {
    let vec = items.first().map_or_else(JsonVec::create, |first| {
        JsonVec::copy_from_array(first, items.len())
    });
    Json::array(vec)
}

fn kv_str(key: &str, value: &str) -> JsonKeyValue {
    JsonKeyValue::create(key, Json::string(value))
}

fn kv_int(key: &str, value: usize) -> JsonKeyValue {
    JsonKeyValue::create(key, Json::int(value as i64))
}

fn kv_num(key: &str, value: f32) -> JsonKeyValue {
    JsonKeyValue::create(key, Json::float(f64::from((value * 100.0).round() / 100.0)))
}

fn stroke_json(s: &Stroke) -> Json {
    let points = s
        .points
        .iter()
        .map(|p| {
            obj(vec![
                kv_num("x", p.x),
                kv_num("y", p.y),
                kv_num("pressure", p.pressure),
                kv_num("tilt_x", p.tilt_x),
                kv_num("tilt_y", p.tilt_y),
            ])
        })
        .collect::<Vec<_>>();
    obj(vec![
        JsonKeyValue::create("id", Json::int(s.id as i64)),
        kv_int("page", s.page),
        JsonKeyValue::create("epoch", Json::int(s.epoch as i64)),
        kv_str("semantic", s.semantic.label()),
        JsonKeyValue::create("points", arr(points)),
    ])
}

/// A note as the archive keeps it: its anchor on the page, the line of the file that is under it
/// (for a reader that has no pages), the words and the language they were recognised in.
fn note_json(n: &Note, first_line_of_page: usize) -> Json {
    obj(vec![
        JsonKeyValue::create("id", Json::int(n.id as i64)),
        kv_int("page", n.page),
        kv_num("x", n.x),
        kv_num("y", n.y),
        kv_int("line", first_line_of_page + ui::line_of(n.y)),
        kv_str("language", &n.language),
        kv_str("text", &n.text),
    ])
}

fn finding_json(f: &Finding) -> Json {
    let mut e = vec![
        kv_str("semantic", f.semantic.label()),
        kv_str("file", &f.file),
        kv_int("first_line", f.first_line),
        kv_int("last_line", f.last_line),
        kv_int("stroke_count", f.stroke_count),
    ];
    if let Some(v) = &f.voice_note {
        e.push(kv_str("voice_note", v));
    }
    obj(e)
}

fn clip_json(index: usize, c: &VoiceClip) -> Json {
    let ids = c
        .stroke_ids
        .iter()
        .map(|id| Json::int(*id as i64))
        .collect::<Vec<_>>();
    obj(vec![
        kv_str("audio", &wav_name(index)),
        JsonKeyValue::create("sample_rate", Json::int(i64::from(c.sample_rate))),
        kv_int("samples", c.samples.len()),
        JsonKeyValue::create("stroke_ids", arr(ids)),
    ])
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

/// The session as an archive: its file name ([`archive_name`]) and its
/// bytes - every voice clip as a WAV and `session.json` (the strokes, the
/// findings, the clips). No disk here: the caller writes it into the data
/// tree on a Thread (`crate::save_session`).
#[must_use]
pub fn archive(s: &AppState) -> (String, Vec<u8>) {
    let mut zip = Zip::create();

    let clips: Vec<&VoiceClip> = s.clips.iter().chain(s.recording.iter()).collect();
    for (i, c) in clips.iter().enumerate() {
        zip.add_file(wav_name(i), bytes(&wav(&c.samples, c.sample_rate)));
    }

    let model = obj(vec![
        kv_str("format", FORMAT),
        kv_str("root", &s.root.display().to_string()),
        kv_str("file", s.file().map_or("", |f| f.display.as_str())),
        JsonKeyValue::create("strokes", arr(s.strokes.iter().map(stroke_json).collect())),
        JsonKeyValue::create(
            "findings",
            arr(s.findings.iter().map(finding_json).collect()),
        ),
        JsonKeyValue::create(
            "notes",
            arr(s
                .notes
                .iter()
                .map(|n| note_json(n, s.file().map_or(1, |f| f.page(n.page).0)))
                .collect()),
        ),
        JsonKeyValue::create(
            "clips",
            arr(clips
                .iter()
                .enumerate()
                .map(|(i, c)| clip_json(i, c))
                .collect()),
        ),
    ]);
    zip.add_file(
        "session.json",
        bytes(model.to_string_pretty().as_str().as_bytes()),
    );

    (
        archive_name(s.file().map(|f| f.display.as_str())),
        zip.to_bytes().as_slice().to_vec(),
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
}

impl Loaded {
    /// The id the next stroke takes (after the archive's own).
    #[must_use]
    pub fn next_stroke_id(&self) -> u64 {
        self.strokes.iter().map(|s| s.id).max().map_or(1, |id| id + 1)
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
    })
}

fn stroke_from(json: &Json) -> Option<Stroke> {
    Some(Stroke {
        page: usize::try_from(count(json, "page")?).ok()?,
        semantic: Semantic::from_label(&text_of(json, "semantic")?)?,
        points: items(json, "points").iter().filter_map(point_from).collect(),
        id: count(json, "id")?,
        epoch: count(json, "epoch").unwrap_or(0),
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
