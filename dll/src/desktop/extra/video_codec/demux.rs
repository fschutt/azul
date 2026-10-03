//! MP4 -> H.264 Annex-B demuxer for the video decoder.
//!
//! gpu-video (Vulkan Video) decodes raw H.264 *elementary streams* (the Annex-B
//! byte-stream format) only — it does NOT parse MP4 containers. This module
//! pulls the H.264 track out of an MP4, converts its AVCC (4-byte-length-
//! prefixed NAL) samples into Annex-B (start-code-prefixed), and prepends the
//! SPS/PPS (from the `avcC` box) before each keyframe so a decoder can start on
//! any IDR. The output chunks feed straight into gpu-video's
//! `EncodedInputChunk { data, pts }`.
//!
//! Pure Rust (`mp4` crate), behind the `video-native` feature — unit-testable
//! on any machine, no GPU needed (which matters here: this box's NVK driver
//! exposes no Vulkan Video decode, so the gpu-video decode step is gated, but
//! the demux is fully verifiable).

use std::io::Cursor;

use mp4::{MediaType, Mp4Reader};

// AVCC -> Annex-B is the container module's (`container::append_avcc_as_annexb`):
// one rewrite for this demuxer and the VideoToolbox encoder's output.
use super::container::append_avcc_as_annexb;

/// 4-byte Annex-B start code, prefixed before every NAL unit.
const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// One demuxed access unit (one frame's worth of NALs), Annex-B framed.
#[derive(Debug, Clone)]
pub struct H264Chunk {
    /// Annex-B bytes: start-code-prefixed NALs, with SPS+PPS prepended on
    /// keyframes so a decoder can start mid-stream.
    pub annexb: Vec<u8>,
    /// Presentation timestamp in milliseconds.
    pub pts_ms: f64,
    /// Whether this access unit is a keyframe (IDR).
    pub is_keyframe: bool,
}

/// A fully-demuxed H.264 elementary stream plus the metadata a player needs.
#[derive(Debug, Clone)]
pub struct DemuxedH264 {
    /// Coded picture width in pixels.
    pub width: u32,
    /// Coded picture height in pixels.
    pub height: u32,
    /// Nominal frame rate (fps), best-effort from the track.
    pub fps: f32,
    /// Sequence parameter set (raw NAL bytes, no start code).
    pub sps: Vec<u8>,
    /// Picture parameter set (raw NAL bytes, no start code).
    pub pps: Vec<u8>,
    /// Access units in decode order.
    pub chunks: Vec<H264Chunk>,
}

/// Demux an in-memory MP4 into an Annex-B H.264 stream.
///
/// Returns an error if the bytes aren't a parseable MP4 or carry no H.264/AVC
/// video track. Assumes the standard 4-byte NAL length prefix
/// (`lengthSizeMinusOne == 3`), which every browser/ffmpeg-produced MP4 uses.
pub fn demux_mp4_h264(mp4_bytes: &[u8]) -> Result<DemuxedH264, String> {
    let size = mp4_bytes.len() as u64;
    let mut reader = Mp4Reader::read_header(Cursor::new(mp4_bytes), size)
        .map_err(|e| format!("mp4 header parse failed: {e}"))?;

    // Locate the H.264/AVC video track and pull its config (SPS/PPS/dims) before
    // we start the mutable sample reads.
    let mut found = None;
    for track in reader.tracks().values() {
        if track.media_type().ok() != Some(MediaType::H264) {
            continue;
        }
        let sps = match track.sequence_parameter_set() {
            Ok(s) => s.to_vec(),
            Err(_) => continue,
        };
        let pps = match track.picture_parameter_set() {
            Ok(p) => p.to_vec(),
            Err(_) => continue,
        };
        found = Some((
            track.track_id(),
            track.width() as u32,
            track.height() as u32,
            sps,
            pps,
            track.sample_count(),
            track.timescale(),
            track.frame_rate() as f32,
        ));
        break;
    }
    let (track_id, width, height, sps, pps, sample_count, timescale, fps) =
        found.ok_or_else(|| String::from("no H.264/AVC video track in MP4"))?;
    if sps.is_empty() || pps.is_empty() {
        return Err(String::from("H.264 track has no SPS/PPS in its avcC box"));
    }
    let timescale = timescale.max(1) as f64;

    let mut chunks = Vec::with_capacity(sample_count as usize);
    // mp4 sample ids are 1-based.
    for sid in 1..=sample_count {
        let sample = match reader.read_sample(track_id, sid) {
            Ok(Some(s)) => s,
            Ok(None) => continue,
            Err(e) => return Err(format!("read_sample {sid} failed: {e}")),
        };
        let is_keyframe = sample.is_sync;
        let mut annexb = Vec::with_capacity(sample.bytes.len() + 16);
        if is_keyframe {
            annexb.extend_from_slice(&START_CODE);
            annexb.extend_from_slice(&sps);
            annexb.extend_from_slice(&START_CODE);
            annexb.extend_from_slice(&pps);
        }
        append_avcc_as_annexb(&sample.bytes, &mut annexb);
        let pts_ms = presentation_ms(sample.start_time, sample.rendering_offset, timescale);
        chunks.push(H264Chunk {
            annexb,
            pts_ms,
            is_keyframe,
        });
    }

    Ok(DemuxedH264 {
        width,
        height,
        fps,
        sps,
        pps,
        chunks,
    })
}

/// When a sample is SHOWN, in milliseconds: its decode time `start_time` plus
/// its composition offset (`ctts`), over the track's `timescale`.
fn presentation_ms(start_time: u64, rendering_offset: i32, timescale: f64) -> f64 {
    (start_time as f64 + f64::from(rendering_offset)) * 1000.0 / timescale
}

#[cfg(test)]
mod demux_tests {
    use super::*;

    /// Hand-built AVCC buffer (two NALs: lengths 3 and 2) converts to two
    /// start-code-prefixed NALs, no SPS/PPS prepend at this layer.
    #[test]
    fn avcc_to_annexb_splits_length_prefixed_nals() {
        // [len=3][AA BB CC][len=2][DD EE]
        let avcc = [0, 0, 0, 3, 0xAA, 0xBB, 0xCC, 0, 0, 0, 2, 0xDD, 0xEE];
        let mut out = Vec::new();
        append_avcc_as_annexb(&avcc, &mut out);
        assert_eq!(
            out,
            vec![
                0, 0, 0, 1, 0xAA, 0xBB, 0xCC, // first NAL
                0, 0, 0, 1, 0xDD, 0xEE, // second NAL
            ]
        );
    }

    /// A stream with B-frames DECODES a frame before it SHOWS it: the `ctts`
    /// offset is what moves it to its place. Big Buck Bunny's 360p clip has
    /// 194 `ctts` entries; without them its frames play in decode order.
    #[test]
    fn a_presentation_time_includes_the_composition_offset() {
        // 15360 ticks per second (BBB's timescale), 512 ticks per frame.
        assert_eq!(presentation_ms(0, 1024, 15360.0), 1024.0 * 1000.0 / 15360.0);
        assert_eq!(presentation_ms(512, 1536, 15360.0), 2048.0 * 1000.0 / 15360.0);
        assert_eq!(presentation_ms(1024, 0, 15360.0), 1024.0 * 1000.0 / 15360.0);
        assert_eq!(presentation_ms(1536, -512, 15360.0), 1024.0 * 1000.0 / 15360.0);
    }

    /// A truncated length prefix (claims 9 bytes, only 2 present) stops cleanly.
    #[test]
    fn avcc_to_annexb_tolerates_truncated_tail() {
        let avcc = [0, 0, 0, 9, 0x11, 0x22];
        let mut out = Vec::new();
        append_avcc_as_annexb(&avcc, &mut out);
        assert!(out.is_empty(), "an unsatisfiable length must emit nothing");
    }

    /// End-to-end against a real H.264 MP4 (the big-buck-bunny sample the user
    /// pointed at). Soft-skips when the sample isn't present so CI without the
    /// asset still passes.
    #[test]
    fn demux_big_buck_bunny_480p() {
        let path = "/tmp/video-media-samples/big-buck-bunny-480p-30sec.mp4";
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => {
                eprintln!("[demux test] sample {path} absent — skipping");
                return;
            }
        };
        let d = demux_mp4_h264(&bytes).expect("demux must succeed on a valid H.264 MP4");

        assert_eq!(d.width, 854, "big-buck-bunny 480p is 854x480");
        assert_eq!(d.height, 480);
        assert!(d.fps > 20.0 && d.fps < 40.0, "≈30fps, got {}", d.fps);
        assert!(!d.sps.is_empty() && !d.pps.is_empty(), "SPS/PPS extracted");
        // SPS NAL header: forbidden_zero_bit 0 + nal_ref_idc + type 7 (SPS).
        assert_eq!(d.sps[0] & 0x1f, 7, "first SPS byte is a type-7 NAL");
        assert_eq!(d.pps[0] & 0x1f, 8, "first PPS byte is a type-8 NAL");

        assert!(
            d.chunks.len() > 100,
            "30s @30fps ≈ 900 frames, got {}",
            d.chunks.len()
        );
        let first = &d.chunks[0];
        assert!(first.is_keyframe, "first access unit must be an IDR");
        assert_eq!(&first.annexb[0..4], &START_CODE, "Annex-B framed");
        // Keyframe carries the prepended SPS (type 7) right after the start code.
        assert_eq!(first.annexb[4] & 0x1f, 7, "keyframe begins with SPS");
        let keyframes = d.chunks.iter().filter(|c| c.is_keyframe).count();
        assert!(keyframes >= 1, "at least one keyframe");
    }
}
