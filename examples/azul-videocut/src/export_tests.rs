//! The export: which frames, at what size, keyframes, and the Y4M fallback
//! for a machine with no H.264 encoder.

use super::*;
use crate::model::{Edit, MediaItem, Pattern, Project};

#[test]
fn an_rgba_picture_converts_to_i420_in_the_bt601_video_range() {
    let white = vec![255u8; 4 * 4 * 2];
    let (y, u, v) = rgba_to_i420(&white, 4, 2);
    assert_eq!((y.len(), u.len(), v.len()), (8, 2, 2));
    assert!(y.iter().all(|s| *s == 235), "{y:?}");
    assert!(u.iter().chain(v.iter()).all(|s| (i32::from(*s) - 128).abs() <= 1));
    let mut black = vec![0u8; 4 * 4 * 2];
    for px in black.chunks_exact_mut(4) {
        px[3] = 255;
    }
    let (y, _, _) = rgba_to_i420(&black, 4, 2);
    assert!(y.iter().all(|s| *s == 16));
    let red: Vec<u8> = (0..8).flat_map(|_| [255u8, 0, 0, 255]).collect();
    let (_, _, v) = rgba_to_i420(&red, 4, 2);
    assert!(v.iter().all(|s| *s > 200), "red is high Cr: {v:?}");
}

#[test]
fn a_y4m_file_is_its_header_and_one_frame_record_per_frame() {
    assert_eq!(y4m_header(4, 2, 25), "YUV4MPEG2 W4 H2 F25:1 Ip A1:1 C420jpeg\n");
    let mut out = Vec::new();
    let picture = vec![128u8; 4 * 2 * 4];
    append_y4m_frame(&mut out, &picture, 4, 2);
    assert_eq!(&out[..6], b"FRAME\n");
    assert_eq!(out.len(), 6 + 8 + 2 + 2);
}

#[test]
fn the_export_covers_the_sequence_or_the_marked_range() {
    let mut p = Project::create("p".into(), "t".into(), 64, 36, 25);
    let m = p.add_media(MediaItem::generated("m", Pattern::Bars, 100, 64, 36));
    let c = p.clip_from_media(m, 0, 40);
    p.edit(Edit::Overwrite { track: 0, at: 10, clip: c }).expect("clip");
    assert_eq!(export_frames(&p, ExportRange::Sequence), 0..50);
    assert_eq!(export_frames(&p, ExportRange::Marked { from: 12, to: 20 }), 12..21);
    assert_eq!(export_frames(&p, ExportRange::Marked { from: 20, to: 12 }), 20..20, "nothing");
    assert_eq!(export_frames(&p, ExportRange::First(5)), 0..5);
}

#[test]
fn an_encoder_gets_an_even_size_and_a_keyframe_every_two_seconds() {
    assert_eq!(even_size(641, 361), (640, 360));
    assert_eq!(even_size(1, 1), (2, 2));
    let keys: Vec<i64> = (0..120).filter(|f| is_keyframe(*f, 25)).collect();
    assert_eq!(keys, vec![0, 50, 100]);
    assert_eq!(timestamp_us(25, 25), 1_000_000);
    assert_eq!(timestamp_us(1, 30), 33_333);
}

#[test]
fn the_export_file_name_ends_in_the_format_it_holds() {
    assert_eq!(output_name("Teaser cut", OutputFormat::Mp4), "Teaser cut.mp4");
    assert_eq!(output_name("a/b:c", OutputFormat::Y4m), "a-b-c.y4m");
    assert_eq!(output_name("", OutputFormat::Mp4), "export.mp4");
}
