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

/// An encoder that works as azul's do: it takes a frame at once, and a frame's packet comes out
/// after `lag` more frames went in; the last `lag` stay inside until `flush` (a codec thread's
/// queue, a hardware codec's pipeline). A packet is three bytes, all the frame's number.
struct HoldingEncoder {
    lag: usize,
    held: std::collections::VecDeque<Vec<u8>>,
    ready: std::collections::VecDeque<Vec<u8>>,
    /// The stamp and the key flag of every frame taken.
    taken: Vec<(u64, bool)>,
    refuse_at: Option<usize>,
    flushed: bool,
}

impl HoldingEncoder {
    fn new(lag: usize) -> Self {
        HoldingEncoder {
            lag,
            held: std::collections::VecDeque::new(),
            ready: std::collections::VecDeque::new(),
            taken: Vec::new(),
            refuse_at: None,
            flushed: false,
        }
    }
}

impl PacketEncoder for HoldingEncoder {
    fn encode_at(&mut self, _frame: VideoFrame, timestamp_us: u64, key: bool) -> bool {
        let number = self.taken.len();
        if self.refuse_at == Some(number) {
            return false;
        }
        self.taken.push((timestamp_us, key));
        self.held.push_back(vec![number as u8; 3]);
        while self.held.len() > self.lag {
            if let Some(packet) = self.held.pop_front() {
                self.ready.push_back(packet);
            }
        }
        true
    }

    fn recv_packet(&mut self) -> Option<Vec<u8>> {
        self.ready.pop_front()
    }

    fn flush(&mut self) {
        self.flushed = true;
        self.ready.extend(self.held.drain(..));
    }
}

fn clip(frames: usize) -> impl Iterator<Item = Result<VideoFrame, String>> {
    (0..frames).map(|_| {
        Ok(VideoFrame {
            width: 2,
            height: 2,
            bytes: U8Vec::from_vec(vec![0; 16]),
            format: RawImageFormat::RGBA8,
        })
    })
}

#[test]
fn the_last_frames_of_an_encoder_that_holds_them_back_are_in_the_file() {
    for lag in [0, 1, 3, 8, 20] {
        let mut encoder = HoldingEncoder::new(lag);
        let mut file = Vec::new();
        let mut progress = Vec::new();
        encode_frames(
            &mut encoder,
            clip(10),
            25,
            |packet| {
                file.push(packet);
                Ok(())
            },
            |done| progress.push(done),
        )
        .expect("an export");
        let every_packet: Vec<Vec<u8>> = (0..10u8).map(|n| vec![n; 3]).collect();
        assert_eq!(file, every_packet, "lag {lag}: every packet of every frame, in the order of the frames");
        assert_eq!(progress, (1..=10).collect::<Vec<i64>>(), "lag {lag}");
        assert!(encoder.flushed, "lag {lag}: the encoder was flushed before the end");
    }
}

#[test]
fn every_frame_is_stamped_with_its_own_time_and_a_key_frame_comes_every_two_seconds() {
    let mut encoder = HoldingEncoder::new(2);
    encode_frames(&mut encoder, clip(120), 25, |_| Ok(()), |_| {}).expect("an export");
    assert_eq!(encoder.taken.len(), 120);
    assert_eq!(encoder.taken[0], (0, true));
    assert_eq!(encoder.taken[1], (40_000, false));
    assert_eq!(encoder.taken[25], (1_000_000, false));
    let keys: Vec<usize> = encoder.taken.iter().enumerate().filter(|(_, t)| t.1).map(|(i, _)| i).collect();
    assert_eq!(keys, vec![0, 50, 100]);
}

#[test]
fn an_encoder_that_refuses_a_frame_ends_the_export_with_that_error() {
    let mut encoder = HoldingEncoder::new(1);
    encoder.refuse_at = Some(4);
    let mut file = Vec::new();
    let result = encode_frames(
        &mut encoder,
        clip(10),
        25,
        |packet| {
            file.push(packet);
            Ok(())
        },
        |_| {},
    );
    assert_eq!(result, Err(String::from("the encoder refused a frame")));
    assert_eq!(file.len(), 3, "the packets of the frames before it were taken");
}

#[test]
fn a_frame_that_cannot_be_rendered_or_a_cancel_ends_the_export_at_once() {
    let mut encoder = HoldingEncoder::new(2);
    let frames = clip(3).chain(std::iter::once(Err(String::from("cancelled")))).chain(clip(3));
    let result = encode_frames(&mut encoder, frames, 25, |_| Ok(()), |_| {});
    assert_eq!(result, Err(String::from("cancelled")));
    assert_eq!(encoder.taken.len(), 3, "nothing after it was encoded");
    assert!(!encoder.flushed, "an abandoned export does not wait for its encoder");
}

#[test]
fn a_packet_the_file_refuses_ends_the_export_with_its_reason_also_after_the_last_frame() {
    // Refused while frames still go in.
    let mut encoder = HoldingEncoder::new(0);
    let result = encode_frames(&mut encoder, clip(5), 25, |_| Err(String::from("the muxer is full")), |_| {});
    assert_eq!(result, Err(String::from("the muxer is full")));
    // Refused for a packet that only the final flush brings out.
    let mut encoder = HoldingEncoder::new(8);
    let mut seen = 0;
    let result = encode_frames(
        &mut encoder,
        clip(5),
        25,
        |_| {
            seen += 1;
            if seen == 2 {
                Err(String::from("the muxer is full"))
            } else {
                Ok(())
            }
        },
        |_| {},
    );
    assert_eq!(result, Err(String::from("the muxer is full")));
    assert_eq!(seen, 2);
}

#[test]
fn an_empty_clip_gives_no_packet_and_succeeds() {
    let mut encoder = HoldingEncoder::new(3);
    let mut seen = 0;
    encode_frames(&mut encoder, clip(0), 25, |_| {
        seen += 1;
        Ok(())
    }, |_| {})
    .expect("an empty export");
    assert_eq!(seen, 0);
}
