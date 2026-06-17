//! File -> frames pipeline: demux an MP4 and feed it through a [`VideoDecoder`].
//!
//! Ties the two halves of the decode path together so an app or the video widget
//! can just say "decode this file": [`demux`](super::demux) extracts the H.264
//! Annex-B access units, and each is handed to the platform [`VideoDecoder`]
//! (gpu-video / VideoToolbox / MediaCodec). The decode step is the only part
//! gated on hardware — on a box with no Vulkan Video decode the demux + feed
//! still run and report the stream geometry, which is what's verifiable here;
//! the same call yields real frames on a capable GPU.
//!
//! Behind `video-native` (needs the demuxer). The widget worker
//! (`azul_layout::widgets::video`) is the streaming counterpart; this is the
//! batch/eager form used for tests and simple "load a clip" cases.

use azul_core::video::{OptionVideoFrame, VideoFrame};
use azul_css::U8Vec;

use super::demux::demux_mp4_h264;
use super::VideoDecoder;

/// A decoded clip: stream geometry plus whatever frames the backend produced.
#[derive(Debug)]
pub struct DecodedVideo {
    /// Coded width in pixels (from the H.264 SPS / avcC).
    pub width: u32,
    /// Coded height in pixels.
    pub height: u32,
    /// Nominal frame rate.
    pub fps: f32,
    /// Decoded RGBA frames, in order. Empty when no hardware decoder is present
    /// (the demux + feed still ran — see module docs).
    pub frames: Vec<VideoFrame>,
    /// Access units fed to the decoder (== demuxed chunk count). Lets a caller /
    /// test confirm the whole stream was pushed even when `frames` is empty.
    pub access_units_fed: usize,
}

/// Demux + decode an MP4 file at `path`.
pub fn decode_mp4_h264_file(path: &str) -> Result<DecodedVideo, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    decode_mp4_h264_bytes(&bytes)
}

/// Demux + decode an in-memory MP4.
pub fn decode_mp4_h264_bytes(mp4: &[u8]) -> Result<DecodedVideo, String> {
    let demuxed = demux_mp4_h264(mp4)?;
    let decoder = VideoDecoder::open(false /* h264 */);

    let mut frames = Vec::new();
    let mut access_units_fed = 0usize;
    for chunk in &demuxed.chunks {
        // A chunk can yield 0..N frames (pipelining + B-frame reorder); drain all.
        let mut f = decoder.decode(U8Vec::from_vec(chunk.annexb.clone()));
        while let OptionVideoFrame::Some(frame) = f {
            frames.push(frame);
            f = decoder.next_frame();
        }
        access_units_fed += 1;
    }
    // End of stream: flush frames held back for reordering.
    let mut f = decoder.flush();
    while let OptionVideoFrame::Some(frame) = f {
        frames.push(frame);
        f = decoder.next_frame();
    }

    Ok(DecodedVideo {
        width: demuxed.width,
        height: demuxed.height,
        fps: demuxed.fps,
        frames,
        access_units_fed,
    })
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;

    /// End-to-end demux + decode on the real big-buck-bunny sample (360p H.264,
    /// fetched by the harness to `/tmp/video-media-samples`). Asserts the stream
    /// geometry and — when frames are produced — that each is a tightly-packed
    /// RGBA8 frame at the coded size. Frame *production* is hardware-gated (Vulkan
    /// Video decode), so on a box without it `frames` is empty (demux + feed still
    /// ran). Set `AZ_REQUIRE_HW_DECODE=1` on a decode-capable runner to make the
    /// test fail loudly if decode stops producing frames. Soft-skips without the
    /// asset.
    #[test]
    fn pipeline_demuxes_and_decodes_the_whole_stream() {
        let path = "/tmp/video-media-samples/big-buck-bunny-360p.mp4";
        if !std::path::Path::new(path).exists() {
            eprintln!("[pipeline test] sample absent — skipping");
            return;
        }
        let d = decode_mp4_h264_file(path).expect("pipeline runs on a valid H.264 MP4");
        assert!(
            d.width >= 320 && d.height >= 180,
            "real geometry expected, got {}x{}",
            d.width,
            d.height
        );
        assert!(d.fps > 10.0, "plausible fps, got {}", d.fps);
        assert!(
            d.access_units_fed > 50,
            "the whole stream was fed: {} AUs",
            d.access_units_fed
        );
        eprintln!(
            "[pipeline] {}x{} @{:.1}fps — fed {} AUs, produced {} frames",
            d.width,
            d.height,
            d.fps,
            d.access_units_fed,
            d.frames.len()
        );
        // Whenever frames were decoded, each must be a full RGBA8 frame at the
        // coded resolution (catches stride / conversion / geometry bugs).
        for fr in &d.frames {
            assert_eq!(fr.width, d.width, "frame width matches stream");
            assert_eq!(fr.height, d.height, "frame height matches stream");
            assert_eq!(
                fr.bytes.len(),
                (d.width as usize) * (d.height as usize) * 4,
                "frame is tightly-packed RGBA8"
            );
        }
        if std::env::var("AZ_REQUIRE_HW_DECODE").is_ok() {
            assert!(
                !d.frames.is_empty(),
                "AZ_REQUIRE_HW_DECODE set but no frames were decoded"
            );
            assert!(
                (d.frames.len() as f32) > (d.access_units_fed as f32) * 0.5,
                "expected most AUs to decode to frames: {} frames / {} AUs",
                d.frames.len(),
                d.access_units_fed
            );
        }
    }
}
