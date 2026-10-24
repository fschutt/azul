//! Real hardware H.264 decode via Vulkan Video (the `gpu-video` crate).
//!
//! Linux + Windows only (gated in `Cargo.toml` + at the `mod` site). Uses the
//! raw-Vulkan `BytesDecoder` path — `default-features = false` on `gpu-video`
//! drops the heavy `wgpu` dependency, so we get a self-contained decoder that
//! takes Annex-B H.264 chunks and hands back **NV12** frames in CPU memory.
//!
//! The decode itself runs on the GPU's video-decode queue (`VK_KHR_video_decode_h264`);
//! gpu-video copies the decoded picture out of the DPB into a host-visible buffer.
//! That "decode on the GPU, copy the result back to the CPU" is exactly the
//! portable CPU-mode frame source (`VideoFrame` is tightly-packed RGBA8), and the
//! same decoder is the basis for the zero-copy GPU/YUV-texture path later.
//!
//! Frames are handed out as the NV12 they are when the app asked for NV12
//! (`VideoDecoder::set_output_format`: a YUV tile converts on the GPU, the
//! CPU rasterizer only the rows it paints), else converted to RGBA8 through
//! the one YCbCr table (`azul_core::resources::nv12_to_rgba`), picking the
//! matrix from each frame's signalled colour space / range.

use azul_core::{
    resources::{nv12_to_rgba, RawImageFormat},
    video::VideoFrame,
};
use azul_css::U8Vec;
use gpu_video::{
    parameters::{
        ColorRange, ColorSpace, DecoderParameters, VideoAdapterDescriptor, VideoDeviceDescriptor,
        VideoInstanceDescriptor,
    },
    EncodedInputChunk, OutputFrame, RawFrameData, VideoInstance,
};

/// A hardware H.264 decoder backed by Vulkan Video. Feed Annex-B chunks via
/// [`decode`](Self::decode); drain trailing reordered frames with
/// [`flush`](Self::flush).
pub struct VulkanVideoDecoder {
    decoder: gpu_video::BytesDecoder,
    /// What frames are handed out as: NV12 as decoded, else RGBA8.
    output: RawImageFormat,
}

impl VulkanVideoDecoder {
    /// Try to open a decode-only Vulkan Video H.264 decoder. Returns `None` when
    /// Vulkan Video decode is unavailable (no Vulkan loader, no decode-capable
    /// adapter, driver/extension missing) so the caller can fall back gracefully.
    ///
    /// Note `supports_encoding: false`: many decode-capable GPUs (e.g. Maxwell /
    /// GTX 9xx) expose `VK_KHR_video_decode_h264` but **not** encode, so requiring
    /// both — the descriptor default — would reject them.
    pub fn open_h264() -> Option<Self> {
        let instance = match VideoInstance::new(&VideoInstanceDescriptor::default()) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("[video] Vulkan instance init failed: {e}");
                return None;
            }
        };
        let adapter = match instance.create_adapter(&VideoAdapterDescriptor {
            supports_decoding: true,
            supports_encoding: false,
        }) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[video] no Vulkan Video decode adapter: {e}");
                return None;
            }
        };
        // create_device clones the backend instance Arc into the device, so the
        // returned VideoDevice (held by BytesDecoder) keeps Vulkan alive on its own —
        // `instance`/`adapter` can drop after this returns.
        let device = match adapter.create_device(&VideoDeviceDescriptor::default()) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[video] Vulkan device create failed: {e}");
                return None;
            }
        };
        let decoder = match device.create_bytes_decoder_h264(DecoderParameters::default()) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[video] H.264 decoder create failed: {e}");
                return None;
            }
        };
        Some(Self {
            decoder,
            output: RawImageFormat::RGBA8,
        })
    }

    /// Hand frames out as NV12 (any NV12 variant: the frame carries the
    /// stream's own matrix and range) or, for anything else, RGBA8.
    pub fn set_output_format(&mut self, format: RawImageFormat) {
        self.output = format;
    }

    /// Feed one Annex-B chunk (one or more NAL units). Returns any frames that
    /// became ready (decode is pipelined + B-frame-reordered, so a chunk may
    /// yield zero, one, or several frames), already converted to RGBA8.
    pub fn decode(&mut self, annexb: &[u8]) -> Vec<VideoFrame> {
        match self.decoder.decode(EncodedInputChunk {
            data: annexb,
            pts: None,
        }) {
            Ok(frames) => {
                let nv12 = self.output.is_nv12();
                frames
                    .into_iter()
                    .map(|f| output_frame(f, nv12))
                    .collect()
            }
            Err(e) => {
                eprintln!("[video] decode error: {e}");
                Vec::new()
            }
        }
    }

    /// Drain frames still buffered for reordering at end-of-stream.
    pub fn flush(&mut self) -> Vec<VideoFrame> {
        match self.decoder.flush() {
            Ok(frames) => {
                let nv12 = self.output.is_nv12();
                frames
                    .into_iter()
                    .map(|f| output_frame(f, nv12))
                    .collect()
            }
            Err(e) => {
                eprintln!("[video] flush error: {e}");
                Vec::new()
            }
        }
    }
}

/// One decoded NV12 [`OutputFrame`] as a [`VideoFrame`]: the NV12 itself
/// (tagged with the stream's matrix and range) when `nv12`, else RGBA8. An
/// unspecified colour space is BT.601, correct for typical SD content.
fn output_frame(frame: OutputFrame<RawFrameData>, nv12: bool) -> VideoFrame {
    let RawFrameData {
        frame: mut planes,
        width,
        height,
    } = frame.data;
    let format = RawImageFormat::nv12(
        matches!(frame.metadata.color_space, ColorSpace::BT709),
        matches!(frame.metadata.color_range, ColorRange::Full),
    );
    if nv12 {
        // Exactly both planes (a decoder may pad the buffer).
        if let Some(total) =
            azul_core::resources::Nv12Layout::new(width as usize, height as usize)
                .checked_total_len()
        {
            planes.truncate(total);
        }
        return VideoFrame::with_format(width, height, U8Vec::from_vec(planes), format);
    }
    let rgba = nv12_to_rgba(&planes, width as usize, height as usize, format).unwrap_or_else(|| {
        eprintln!(
            "[video] short NV12 buffer ({} bytes for {}x{}) — emitting a black frame",
            planes.len(),
            width,
            height
        );
        let mut black = vec![0u8; width as usize * height as usize * 4];
        for px in black.chunks_exact_mut(4) {
            px[3] = 255;
        }
        black
    });
    VideoFrame::new(width, height, U8Vec::from_vec(rgba))
}
