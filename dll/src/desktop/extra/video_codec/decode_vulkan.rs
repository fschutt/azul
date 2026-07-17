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
//! NV12 -> RGBA8 conversion happens here on the CPU, picking the YCbCr matrix
//! from each frame's signalled colour space / range.

use azul_core::video::VideoFrame;
use azul_css::U8Vec;
use gpu_video::{
    parameters::{
        ColorRange, ColorSpace, DecoderParameters, VideoAdapterDescriptor,
        VideoDeviceDescriptor, VideoInstanceDescriptor,
    },
    EncodedInputChunk, OutputFrame, RawFrameData, VideoInstance,
};

/// A hardware H.264 decoder backed by Vulkan Video. Feed Annex-B chunks via
/// [`decode`](Self::decode); drain trailing reordered frames with
/// [`flush`](Self::flush).
pub struct VulkanVideoDecoder {
    decoder: gpu_video::BytesDecoder,
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
        Some(Self { decoder })
    }

    /// Feed one Annex-B chunk (one or more NAL units). Returns any frames that
    /// became ready (decode is pipelined + B-frame-reordered, so a chunk may
    /// yield zero, one, or several frames), already converted to RGBA8.
    pub fn decode(&mut self, annexb: &[u8]) -> Vec<VideoFrame> {
        match self.decoder.decode(EncodedInputChunk {
            data: annexb,
            pts: None,
        }) {
            Ok(frames) => frames.into_iter().map(output_frame_to_rgba).collect(),
            Err(e) => {
                eprintln!("[video] decode error: {e}");
                Vec::new()
            }
        }
    }

    /// Drain frames still buffered for reordering at end-of-stream.
    pub fn flush(&mut self) -> Vec<VideoFrame> {
        match self.decoder.flush() {
            Ok(frames) => frames.into_iter().map(output_frame_to_rgba).collect(),
            Err(e) => {
                eprintln!("[video] flush error: {e}");
                Vec::new()
            }
        }
    }
}

/// Convert one decoded NV12 [`OutputFrame`] into an RGBA8 [`VideoFrame`].
fn output_frame_to_rgba(frame: OutputFrame<RawFrameData>) -> VideoFrame {
    let RawFrameData {
        frame: nv12,
        width,
        height,
    } = frame.data;
    let rgba = nv12_to_rgba(
        &nv12,
        width,
        height,
        frame.metadata.color_space,
        frame.metadata.color_range,
    );
    VideoFrame::new(width, height, U8Vec::from_vec(rgba))
}

/// Convert tightly-packed NV12 (Y plane `w*h`, then interleaved Cb/Cr at half
/// resolution) to tightly-packed RGBA8 (`w*h*4`). The YCbCr->RGB matrix is
/// selected from the stream-signalled colour space + range; `Unspecified`
/// defaults to BT.601 limited (correct for typical SD content).
fn nv12_to_rgba(
    nv12: &[u8],
    width: u32,
    height: u32,
    color_space: ColorSpace,
    color_range: ColorRange,
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let y_size = w * h;
    let uv_off = y_size;
    let mut out = vec![0u8; w * h * 4];
    // Need the full Y plane + the interleaved chroma plane (w * h/2 bytes).
    if nv12.len() < y_size + w * (h / 2) {
        eprintln!(
            "[video] short NV12 buffer ({} < {}) — emitting black frame",
            nv12.len(),
            y_size + w * (h / 2)
        );
        for px in out.chunks_exact_mut(4) {
            px[3] = 255;
        }
        return out;
    }

    let full = matches!(color_range, ColorRange::Full);
    let bt709 = matches!(color_space, ColorSpace::BT709);
    // (luma_scale, luma_bias, Cr->R, Cb->G, Cr->G, Cb->B). Limited range bakes
    // the 255/219 luma stretch into luma_scale; full range uses unity luma.
    let (ls, lb, crr, cbg, crg, cbb): (f32, f32, f32, f32, f32, f32) = match (bt709, full) {
        (false, false) => (1.164_383, 16.0, 1.596_027, -0.391_762, -0.812_968, 2.017_232), // BT.601 limited
        (true, false) => (1.164_383, 16.0, 1.792_741, -0.213_249, -0.532_909, 2.112_402), // BT.709 limited
        (false, true) => (1.0, 0.0, 1.402_000, -0.344_136, -0.714_136, 1.772_000),        // BT.601 full
        (true, true) => (1.0, 0.0, 1.574_800, -0.187_324, -0.468_124, 1.855_600),         // BT.709 full
    };

    for j in 0..h {
        let y_row = j * w;
        let uv_row = uv_off + (j / 2) * w;
        let out_row = y_row * 4;
        for i in 0..w {
            let y = nv12[y_row + i] as f32;
            let uv = (i / 2) * 2;
            let u = nv12[uv_row + uv] as f32 - 128.0;
            let v = nv12[uv_row + uv + 1] as f32 - 128.0;
            let c = (y - lb) * ls;
            let r = c + crr * v;
            let g = c + cbg * u + crg * v;
            let b = c + cbb * u;
            let o = out_row + i * 4;
            out[o] = r.clamp(0.0, 255.0) as u8;
            out[o + 1] = g.clamp(0.0, 255.0) as u8;
            out[o + 2] = b.clamp(0.0, 255.0) as u8;
            out[o + 3] = 255;
        }
    }
    out
}
