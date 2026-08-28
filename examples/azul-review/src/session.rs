//! Session persistence: one ZIP holding a JSON model plus the raw audio.
//!
//! # Everything here goes through the Azul API
//!
//! No `zip`, `serde` or `hound` crate is reachable from this file. The archive
//! is `azul::zip::Zip` (`AzZip_*` over the C ABI) and the model is
//! `azul::json::Json` (`AzJson_*`), so this file is a demonstration that the
//! toolkit can write a real session format, not a Rust program borrowing Rust
//! libraries. A C or Python port of AzReview would call the same entry points.
//!
//! # Why a ZIP and not a directory
//!
//! A review is ink plus speech. The ink is small and structured, the audio is
//! large and opaque, and the two are useless apart - a clip that has lost its
//! stroke ids is just noise. One file that cannot be half-copied is the cheapest
//! way to keep them together.
//!
//! # Why the layout is flat
//!
//! Audio lands as `clip-0.wav`, `clip-1.wav`, ... beside `session.json`, with
//! no nesting. `session.json` names each clip, so the directory structure would
//! carry no information the JSON does not already hold - and a flat archive is
//! one `unzip` away from being usable by anything.
//!
//! # Why the analysis is not here
//!
//! This is collection only. What the marks MEAN is a question for a later tool
//! reading these archives in bulk; deciding it now, per session, would bake
//! today's guess into the recorded data and make the corpus unusable when the
//! guess changes.

use azul::json::{Json, JsonKeyValue};
use azul::vec::{JsonKeyValueVec, JsonVec, U8Vec};
use azul::zip::Zip;

use crate::model::{Finding, Stroke, VoiceClip};
use crate::AppState;

// --------------------------------------------------------------------------
// Small bridges into the C-ABI vector types.
//
// The FFI vectors have no `From<Vec<T>>` - they are built from a pointer and a
// length, which is what a non-Rust caller has too. `copy_from_array` CLONES
// each element, so the source vector is still ours to drop.
// --------------------------------------------------------------------------

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

/// Rounded to 2 decimals. Ink is sampled in logical pixels off a ~1000 Hz
/// digitizer; the extra digits are noise that would triple the file size.
fn kv_num(key: &str, value: f32) -> JsonKeyValue {
    JsonKeyValue::create(key, Json::float(f64::from((value * 100.0).round() / 100.0)))
}

// --------------------------------------------------------------------------
// Model -> JSON
// --------------------------------------------------------------------------

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
        kv_str("semantic", s.semantic.label()),
        JsonKeyValue::create("points", arr(points)),
    ])
}

/// Findings are DERIVED, and they are written anyway.
///
/// Redundant with the strokes by construction - which is the point. Whatever
/// derives findings will change; keeping the derivation this session actually
/// used makes it possible to tell a later change from a change in the ink.
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
        // The binding, and the only part that cannot be reconstructed later:
        // which marks were on the page while this was being said.
        JsonKeyValue::create("stroke_ids", arr(ids)),
    ])
}

fn wav_name(index: usize) -> String {
    format!("clip-{index}.wav")
}

// --------------------------------------------------------------------------
// Audio
// --------------------------------------------------------------------------

/// 16-bit mono PCM WAV.
///
/// Written by hand rather than pulled from a crate: it is 44 bytes of header,
/// and a dependency here would be exactly the Rust-only shortcut this app is
/// meant to avoid. WAV over anything smaller because the archive is an input to
/// tools that do not exist yet - every one of them can read WAV.
fn wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    let mut tag = |s: &str| out.extend_from_slice(s.as_bytes());
    tag("RIFF");
    out.extend_from_slice(
        &u32::try_from(36 + data_len)
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&u32::try_from(data_len).unwrap_or(u32::MAX).to_le_bytes());
    for s in samples {
        // Clamp before scaling: a mic peak above 1.0 would wrap to a loud click
        // rather than clipping, and a click in the middle of a spoken remark is
        // worse than the lost headroom.
        let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

// --------------------------------------------------------------------------
// Write
// --------------------------------------------------------------------------

/// Build the whole archive in memory and write it.
///
/// Whole-archive rewrite on every save, not an append: a review is at most a
/// few MB, and a partially-appended ZIP after a crash is unreadable - which
/// would defeat the reason autosave exists.
///
/// Written to a sibling temp path and renamed, for the same reason. Autosave
/// runs on every pen-up, so it is running most of the time the app is; a crash
/// during the write must not be able to destroy the previous good archive.
pub fn save(s: &AppState) -> bool {
    let dir = crate::scratch_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }

    let mut zip = Zip::new();

    let clips: Vec<&VoiceClip> = s.clips.iter().chain(s.recording.iter()).collect();
    for (i, c) in clips.iter().enumerate() {
        zip.add_file(wav_name(i), bytes(&wav(&c.samples, c.sample_rate)));
    }

    let model = obj(vec![
        kv_str("format", "azreview/1"),
        kv_str("root", &s.root.display().to_string()),
        kv_str("file", s.file().map_or("", |f| f.display.as_str())),
        JsonKeyValue::create("strokes", arr(s.strokes.iter().map(stroke_json).collect())),
        JsonKeyValue::create(
            "findings",
            arr(s.findings.iter().map(finding_json).collect()),
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

    let final_path = archive_path(s);
    let temp_path = final_path.with_extension("zip.part");
    if !zip.to_file(temp_path.to_string_lossy().as_ref()) {
        return false;
    }
    std::fs::rename(&temp_path, &final_path).is_ok()
}

/// One archive per reviewed file, named after it.
///
/// Per-file rather than per-session because ink is already per-file: switching
/// files parks the sheet, and an archive that spanned files would have to be
/// rewritten in full every time one of them changed.
fn archive_path(s: &AppState) -> std::path::PathBuf {
    let stem = s.file().map_or_else(
        || "session".to_string(),
        |f| f.display.replace(['/', '\\'], "_"),
    );
    crate::scratch_dir().join(format!("{stem}.azreview.zip"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Wrapping would turn a loud syllable into a click - louder than the
        // signal it replaced, and in the middle of the spoken rationale.
        let w = wav(&[2.0, -2.0], 48_000);
        let a = i16::from_le_bytes([w[44], w[45]]);
        let b = i16::from_le_bytes([w[46], w[47]]);
        assert!(a > 32_000, "positive peak wrapped: {a}");
        assert!(b < -32_000, "negative peak wrapped: {b}");
    }
}
