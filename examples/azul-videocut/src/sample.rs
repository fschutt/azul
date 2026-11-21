//! The sample project (`--sample`): a short cut the E2E and a first look
//! need, with no media files of the user's.
//!
//! Two coloured clips (a blue and an orange sweep) cut together on V1 with a
//! cross dissolve, colour bars after them, and a title matte as a
//! picture-in-picture on V2. Where the machine has an H.264 encoder the two
//! clips are ENCODED to MP4 files in the project's `media/` folder through
//! the export path (the same `run_export`, so the sample also proves the
//! encode -> mux -> demux -> decode round trip), and the project plays them
//! from there; elsewhere they stay generated pictures.

use crate::{
    export::{run_export, ExportRange, ExportSettings, ExportShared, OutputFormat},
    model::{Edit, Effects, MediaItem, MediaSource, Pattern, Project, Transition, TransitionKind},
    render::Generated,
};

/// The sample sequence: 1280 x 720 at 25 fps.
pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 720;
pub const FPS: u32 = 25;
/// The encoded sample clips' size.
pub const CLIP_WIDTH: u32 = 640;
pub const CLIP_HEIGHT: u32 = 360;

/// A sample clip: its name, its pattern, its length in frames.
pub const CLIPS: [(&str, Pattern, i64); 2] = [
    ("blue-sweep.mp4", Pattern::Sweep { rgb: [40, 90, 200] }, 100),
    ("orange-sweep.mp4", Pattern::Sweep { rgb: [220, 120, 30] }, 100),
];

/// The sample project with generated media only.
#[must_use]
pub fn sample_project(id: String) -> Project {
    let mut p = Project::create(id, String::from("Sample cut"), WIDTH, HEIGHT, FPS);
    let blue = p.add_media(MediaItem::generated(CLIPS[0].0, CLIPS[0].1.clone(), CLIPS[0].2, CLIP_WIDTH, CLIP_HEIGHT));
    let orange = p.add_media(MediaItem::generated(CLIPS[1].0, CLIPS[1].1.clone(), CLIPS[1].2, CLIP_WIDTH, CLIP_HEIGHT));
    let bars = p.add_media(MediaItem::generated("Colour bars", Pattern::Bars, 250, WIDTH, HEIGHT));
    let title = p.add_media(MediaItem::generated(
        "Title matte",
        Pattern::Matte { rgb: [30, 30, 40] },
        250,
        WIDTH,
        HEIGHT,
    ));
    let a = p.clip_from_media(blue, 0, 75);
    let b = p.clip_from_media(orange, 10, 75);
    let c = p.clip_from_media(bars, 0, 50);
    let t = p.clip_from_media(title, 0, 60);
    let (idb, idt) = (b.id, t.id);
    let _ = p.edit(Edit::Overwrite { track: 0, at: 0, clip: a });
    let _ = p.edit(Edit::Overwrite { track: 0, at: 75, clip: b });
    let _ = p.edit(Edit::Overwrite { track: 0, at: 150, clip: c });
    let _ = p.edit(Edit::Overwrite { track: 1, at: 25, clip: t });
    let _ = p.edit(Edit::SetTransition {
        clip: idb,
        transition: Some(Transition { kind: TransitionKind::CrossDissolve, frames: 12 }),
    });
    let _ = p.edit(Edit::SetEffects {
        clip: idt,
        effects: Effects {
            scale: 0.3,
            x: 400.0,
            y: -220.0,
            opacity: 0.85,
            ..Effects::default()
        },
    });
    // A sample starts with a clean history.
    p.clear_history();
    p
}

/// Encodes the generated clip `pattern` (`frames` long) to an MP4 at the
/// sample clip size through the export path; `None` where this machine has
/// no H.264 encoder (the export answers with a Y4M then).
#[must_use]
pub fn encode_clip(pattern: &Pattern, frames: i64) -> Option<Vec<u8>> {
    let mut tiny = Project::create(String::from("sample-clip"), String::from("clip"), CLIP_WIDTH, CLIP_HEIGHT, FPS);
    let m = tiny.add_media(MediaItem::generated("clip", pattern.clone(), frames, CLIP_WIDTH, CLIP_HEIGHT));
    let clip = tiny.clip_from_media(m, 0, frames);
    tiny.edit(Edit::Overwrite { track: 0, at: 0, clip }).ok()?;
    let settings = ExportSettings {
        width: CLIP_WIDTH,
        height: CLIP_HEIGHT,
        bitrate_kbps: 2000,
        range: ExportRange::Sequence,
        name: String::from("clip"),
    };
    let shared = ExportShared::default();
    match run_export(&tiny, &settings, &mut Generated, &shared) {
        Ok((bytes, OutputFormat::Mp4)) => Some(bytes),
        _ => None,
    }
}

/// Points the sample's generated clip `name` at its encoded file `key`.
pub fn use_encoded(project: &mut Project, name: &str, key: String) {
    for m in &mut project.media {
        if m.name == name {
            m.source = MediaSource::Stored { key: key.clone() };
            m.codec = String::from("H.264");
            m.fps = f64::from(FPS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_is_two_clips_with_a_dissolve_then_bars_on_v1_and_a_picture_in_picture_on_v2() {
        let p = sample_project(String::from("s"));
        let v1 = &p.sequence.tracks[0].clips;
        assert_eq!(v1.len(), 3);
        assert_eq!((v1[0].start, v1[1].start, v1[2].start), (0, 75, 150));
        assert_eq!(v1[1].transition.map(|t| t.kind), Some(TransitionKind::CrossDissolve));
        let v2 = &p.sequence.tracks[1].clips;
        assert_eq!(v2.len(), 1);
        assert!((v2[0].effects.scale - 0.3).abs() < 1e-6);
        assert_eq!(p.sequence.end(), 200);
        assert!(!p.can_undo(), "a sample starts with a clean history");
        assert!(p.media.iter().all(MediaItem::is_generated));
    }
}
