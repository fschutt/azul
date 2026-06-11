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
        if let OptionVideoFrame::Some(frame) =
            decoder.decode(U8Vec::from_vec(chunk.annexb.clone()))
        {
            frames.push(frame);
        }
        access_units_fed += 1;
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

    /// End-to-end demux + feed on the real big-buck-bunny sample: the stream
    /// geometry is reported and every access unit is pushed to the decoder. Frame
    /// production itself is hardware-gated (no Vulkan Video decode on this box),
    /// so `frames` may be empty here — but the pipeline ran end to end, and the
    /// same call yields frames on a capable GPU. Soft-skips without the asset.
    #[test]
    fn pipeline_demuxes_and_feeds_the_whole_stream() {
        let path = "/tmp/video-media-samples/big-buck-bunny-480p-30sec.mp4";
        if !std::path::Path::new(path).exists() {
            eprintln!("[pipeline test] sample absent — skipping");
            return;
        }
        let d = decode_mp4_h264_file(path).expect("pipeline runs on a valid H.264 MP4");
        assert_eq!(d.width, 854);
        assert_eq!(d.height, 480);
        assert!(d.fps > 20.0 && d.fps < 40.0);
        assert!(
            d.access_units_fed > 100,
            "the whole ~30s stream was fed: {} AUs",
            d.access_units_fed
        );
        // frames.len() is 0 without a HW decoder and == access_units_fed with one;
        // either way it can never exceed what we fed.
        assert!(d.frames.len() <= d.access_units_fed);
        eprintln!(
            "[pipeline] {}x{} @{:.1}fps — fed {} AUs, produced {} frames",
            d.width,
            d.height,
            d.fps,
            d.access_units_fed,
            d.frames.len()
        );
    }
}
