//! The MP4 container handles an app edits video with: an MP4's H.264 track
//! as Annex-B access units by index (seek to a keyframe, decode forward) and
//! an encoder's Annex-B access units back into an MP4.

use azul_core::video::{OptionVideoChunk, VideoChunk};
use azul_css::U8Vec;

use super::*;

/// A start code.
const SC: [u8; 4] = [0, 0, 0, 1];
/// A sequence parameter set (NAL type 7): High profile, level 3.1.
const SPS: [u8; 8] = [0x67, 0x64, 0x00, 0x1f, 0xac, 0xd9, 0x40, 0x50];
/// A picture parameter set (NAL type 8).
const PPS: [u8; 4] = [0x68, 0xeb, 0xe3, 0xcb];

/// An IDR slice (NAL type 5) whose payload says `n`.
fn idr(n: u8) -> Vec<u8> {
    vec![0x65, 0x88, 0x84, n, n, n]
}

/// A non-IDR slice (NAL type 1) whose payload says `n`.
fn slice(n: u8) -> Vec<u8> {
    vec![0x41, 0x9a, 0x02, n, n]
}

/// An access unit as an encoder hands it out: start-code-prefixed NALs, the
/// parameter sets in front of a keyframe.
fn access_unit(keyframe: bool, n: u8) -> Vec<u8> {
    let mut out = Vec::new();
    if keyframe {
        out.extend_from_slice(&SC);
        out.extend_from_slice(&SPS);
        out.extend_from_slice(&SC);
        out.extend_from_slice(&PPS);
        out.extend_from_slice(&SC);
        out.extend_from_slice(&idr(n));
    } else {
        // A three-byte start code: both lengths occur in the wild.
        out.extend_from_slice(&[0, 0, 1]);
        out.extend_from_slice(&slice(n));
    }
    out
}

fn chunk(pts_ms: f64, is_keyframe: bool) -> VideoChunk {
    VideoChunk {
        pts_ms,
        data: U8Vec::from_vec(vec![1, 2, 3]),
        is_keyframe,
    }
}

#[test]
fn an_annexb_access_unit_becomes_one_length_prefixed_sample_without_its_parameter_sets() {
    let mut au = access_unit(true, 7);
    // An SEI after the slice stays in the sample.
    au.extend_from_slice(&SC);
    au.extend_from_slice(&[0x06, 0x05, 0x01]);
    let sample = annexb_to_avcc(&au);
    assert!(sample.keyframe, "an IDR slice makes the sample a keyframe");
    assert_eq!(sample.sps.as_deref(), Some(&SPS[..]));
    assert_eq!(sample.pps.as_deref(), Some(&PPS[..]));
    let mut expected = Vec::new();
    expected.extend_from_slice(&(idr(7).len() as u32).to_be_bytes());
    expected.extend_from_slice(&idr(7));
    expected.extend_from_slice(&3u32.to_be_bytes());
    expected.extend_from_slice(&[0x06, 0x05, 0x01]);
    assert_eq!(
        sample.bytes, expected,
        "the parameter sets go to the avcC box, every other NAL is length-prefixed"
    );
}

#[test]
fn a_non_idr_access_unit_is_not_a_keyframe_and_carries_no_parameter_sets() {
    let sample = annexb_to_avcc(&access_unit(false, 3));
    assert!(!sample.keyframe);
    assert!(sample.sps.is_none() && sample.pps.is_none());
    let mut expected = (slice(3).len() as u32).to_be_bytes().to_vec();
    expected.extend_from_slice(&slice(3));
    assert_eq!(sample.bytes, expected);
}

#[test]
fn the_nal_splitter_takes_three_and_four_byte_start_codes() {
    let mut data = vec![0, 0, 0, 1, 0x67, 1, 2];
    data.extend_from_slice(&[0, 0, 1, 0x68, 3]);
    data.extend_from_slice(&[0, 0, 0, 1, 0x65, 4, 5, 6]);
    let nals = annexb_nals(&data);
    assert_eq!(nals, vec![&[0x67, 1, 2][..], &[0x68, 3][..], &[0x65, 4, 5, 6][..]]);
}

#[test]
fn the_frame_shown_at_a_time_is_the_last_one_whose_presentation_time_has_come() {
    let chunks = [
        chunk(0.0, true),
        chunk(40.0, false),
        chunk(80.0, false),
        chunk(120.0, false),
    ];
    assert_eq!(shown_at(&chunks, 0.0), 0);
    assert_eq!(shown_at(&chunks, 39.9), 0);
    assert_eq!(shown_at(&chunks, 40.0), 1);
    assert_eq!(shown_at(&chunks, 100.0), 2);
    assert_eq!(shown_at(&chunks, 10_000.0), 3, "past the end: the last frame");
    assert_eq!(shown_at(&chunks, -5.0), 0, "before the start: the first frame");
    assert_eq!(shown_at(&[], 5.0), 0, "an empty stream answers 0");
}

#[test]
fn with_b_frames_the_frame_shown_at_a_time_is_found_by_presentation_time_not_decode_order() {
    // Decode order I P B B: the P is shown after both Bs.
    let chunks = [
        chunk(0.0, true),
        chunk(120.0, false),
        chunk(40.0, false),
        chunk(80.0, false),
    ];
    assert_eq!(shown_at(&chunks, 50.0), 2, "the first B is shown at 40 ms");
    assert_eq!(shown_at(&chunks, 90.0), 3);
    assert_eq!(shown_at(&chunks, 130.0), 1, "the P is shown last");
}

#[test]
fn a_seek_starts_at_the_keyframe_at_or_before_the_frame() {
    let chunks = [
        chunk(0.0, true),
        chunk(40.0, false),
        chunk(80.0, false),
        chunk(120.0, true),
        chunk(160.0, false),
        chunk(200.0, false),
    ];
    assert_eq!(keyframe_at_or_before(&chunks, 0), 0);
    assert_eq!(keyframe_at_or_before(&chunks, 2), 0);
    assert_eq!(keyframe_at_or_before(&chunks, 3), 3, "a keyframe starts at itself");
    assert_eq!(keyframe_at_or_before(&chunks, 5), 3);
    assert_eq!(keyframe_at_or_before(&chunks, 99), 3, "an index past the end is clamped");
    let no_keyframe = [chunk(0.0, false), chunk(40.0, false)];
    assert_eq!(keyframe_at_or_before(&no_keyframe, 1), 0, "no keyframe: from the start");
}

#[test]
fn a_bad_file_opens_no_demuxer_and_says_why_and_a_build_without_mp4_has_no_muxer() {
    let demuxer = Mp4Demuxer::create(U8Vec::from_vec(b"not an mp4 file at all".to_vec()));
    assert!(!demuxer.is_open());
    assert!(!demuxer.error().as_str().is_empty(), "the reason is kept");
    assert_eq!(demuxer.chunk_count(), 0);
    assert!(matches!(demuxer.chunk(0), OptionVideoChunk::None));
    assert_eq!(demuxer.width(), 0);
    if !cfg!(feature = "video-native") {
        let muxer = Mp4Muxer::create(320, 180, 25.0);
        assert!(!muxer.is_open(), "no mp4 crate, no muxer");
        assert!(!muxer.error().as_str().is_empty());
    }
}

#[cfg(feature = "video-native")]
#[test]
fn a_muxer_that_never_saw_a_keyframe_writes_no_file_and_says_why() {
    let mut muxer = Mp4Muxer::create(320, 180, 25.0);
    assert!(muxer.is_open(), "a muxer opens before its first access unit");
    assert!(
        !muxer.write_annexb(U8Vec::from_vec(access_unit(false, 1))),
        "a stream cannot start without its parameter sets"
    );
    let bytes = muxer.finish();
    assert!(bytes.as_ref().is_empty());
    assert!(!muxer.error().as_str().is_empty());
}

#[cfg(feature = "video-native")]
#[test]
fn an_encoded_stream_muxed_to_mp4_demuxes_to_the_same_access_units() {
    let mut muxer = Mp4Muxer::create(320, 180, 25.0);
    for i in 0..6u8 {
        // A keyframe every third frame.
        assert!(muxer.write_annexb(U8Vec::from_vec(access_unit(i % 3 == 0, i))));
    }
    assert_eq!(muxer.samples_written(), 6);
    let bytes = muxer.finish();
    let file = bytes.as_ref();
    assert!(file.len() > 100, "an MP4 came out ({} bytes)", file.len());
    assert_eq!(&file[4..8], b"ftyp", "it starts with the file type box");
    assert!(muxer.error().as_str().is_empty(), "{}", muxer.error().as_str());

    let demuxer = Mp4Demuxer::create(U8Vec::from_vec(file.to_vec()));
    assert!(demuxer.is_open(), "{}", demuxer.error().as_str());
    assert_eq!((demuxer.width(), demuxer.height()), (320, 180));
    assert_eq!(demuxer.chunk_count(), 6);
    assert!((demuxer.fps() - 25.0).abs() < 0.01, "fps {}", demuxer.fps());
    assert!((demuxer.duration_ms() - 240.0).abs() < 0.5, "duration {}", demuxer.duration_ms());
    for i in 0..6usize {
        let OptionVideoChunk::Some(c) = demuxer.chunk(i) else {
            panic!("chunk {i} is missing");
        };
        assert_eq!(c.is_keyframe, i % 3 == 0, "chunk {i}'s keyframe flag");
        assert!((c.pts_ms - 40.0 * i as f64).abs() < 0.01, "chunk {i} at {}", c.pts_ms);
        // The demuxer hands every keyframe out with its parameter sets in
        // front, so a decoder can start there: the same Annex-B the encoder
        // produced.
        let nals: Vec<Vec<u8>> = annexb_nals(c.data.as_ref()).into_iter().map(<[u8]>::to_vec).collect();
        if i % 3 == 0 {
            assert_eq!(nals, vec![SPS.to_vec(), PPS.to_vec(), idr(i as u8)]);
        } else {
            assert_eq!(nals, vec![slice(i as u8)]);
        }
    }
    assert!(matches!(demuxer.chunk(6), OptionVideoChunk::None));
    // The seek helpers on the handle.
    assert_eq!(demuxer.frame_at(130.0), 3);
    assert_eq!(demuxer.keyframe_before(5), 3);
    assert_eq!(demuxer.keyframe_before(2), 0);
}
