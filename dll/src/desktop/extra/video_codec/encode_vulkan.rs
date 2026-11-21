//! Hardware H.264 **encode** via Vulkan Video (the `gpu-video` crate): the
//! Linux / Windows engine behind `VideoEncoder`, the counterpart of
//! `decode_vulkan.rs` - so a call off Apple sends H.264, not JPEG.
//!
//! Linux (glibc) + Windows on x86_64, behind `video-native` (`az_gpu_video`,
//! one definition in `build.rs`). Uses the raw-Vulkan `BytesEncoderH264`
//! path: frames go in as tightly packed NV12 in CPU memory, Annex-B access
//! units (SPS / PPS in front of every IDR) come out. Many decode-capable GPUs
//! cannot encode (NVIDIA Maxwell / GTX 9xx); `encode_engine` asks the driver
//! for `VK_KHR_video_encode_h264` before a handle opens, and `open` fails
//! cleanly where the device still refuses.
//!
//! - An NV12 frame goes in as it is; an RGBA8 / BGRA8 frame is converted
//!   once (`rgba_to_nv12`, Rec.709 video range - the colour tags say so).
//! - The colour tags (matrix, range) follow the frames, as VideoToolbox's
//!   do: a frame in another NV12 format gets an encoder made for it, whose
//!   first frame is a keyframe.
//! - Low-latency preset, variable bit rate at the asked rate (twice that at
//!   peaks, a one second buffer), an IDR every two seconds.
//! - `set_bitrate` makes a new encoder at the new rate (gpu-video has no
//!   live rate change): the next frame is a keyframe. The app's rate
//!   controller changes the rate at most every half second, and only by
//!   steps (`AzMeet`'s `rate.rs`).

use std::num::NonZeroU32;

use azul_core::{
    resources::{rgba_to_nv12, Nv12Layout, RawImageFormat},
    video::VideoFrame,
};
use gpu_video::{
    parameters::{
        ColorRange, ColorSpace, EncoderParametersH264, RateControl, VideoAdapterDescriptor,
        VideoDeviceDescriptor, VideoInstanceDescriptor, VideoParameters,
    },
    BytesEncoderH264, InputFrame, RawFrameData, VideoDevice, VideoInstance,
};

/// Frames a second the rate control plans for (a camera's; screen shares
/// send fewer, which only leaves bits unused).
const TARGET_FPS: u32 = 30;
/// An IDR at least this often (frames): two seconds, so a receiver that
/// lost a packet waits at most that long without asking.
const IDR_PERIOD: u32 = 60;

/// The NV12 format an encoder session is made for, for a frame in `format`:
/// an NV12 frame's own; RGB frames are converted to Rec.709 video range.
fn session_format(format: RawImageFormat) -> RawImageFormat {
    if format.is_nv12() {
        format
    } else {
        RawImageFormat::NV12Rec709Video
    }
}

/// A hardware H.264 encoder on a Vulkan Video device.
pub struct VulkanVideoEncoder {
    device: VideoDevice,
    encoder: BytesEncoderH264,
    width: u32,
    height: u32,
    bitrate_kbps: u32,
    /// The NV12 format the session encodes (its colour tags).
    format: RawImageFormat,
}

impl VulkanVideoEncoder {
    /// A `width` x `height` H.264 encoder at `bitrate_kbps`, or why none
    /// opens on this machine (no Vulkan loader, no encode-capable device, a
    /// size or profile the device refuses).
    pub fn open(width: u32, height: u32, bitrate_kbps: u32) -> Result<Self, String> {
        let instance = VideoInstance::new(&VideoInstanceDescriptor::default())
            .map_err(|e| format!("the Vulkan instance did not start: {e}"))?;
        let adapter = instance
            .create_adapter(&VideoAdapterDescriptor {
                supports_decoding: false,
                supports_encoding: true,
            })
            .map_err(|e| format!("no Vulkan Video device here encodes: {e}"))?;
        // The device keeps the backend instance alive on its own (see
        // `decode_vulkan.rs`): `instance` and `adapter` may drop.
        let device = adapter
            .create_device(&VideoDeviceDescriptor::default())
            .map_err(|e| format!("the Vulkan Video device did not open: {e}"))?;
        let format = RawImageFormat::NV12Rec709Video;
        let encoder = Self::make(&device, width, height, bitrate_kbps, format)?;
        eprintln!("[video] Vulkan Video H.264 encoder open: {width}x{height} @{bitrate_kbps}kbps");
        Ok(VulkanVideoEncoder {
            device,
            encoder,
            width,
            height,
            bitrate_kbps,
            format,
        })
    }

    /// An encoder session on `device`: low latency, VBR at `kbps`, the
    /// colour tags of `format`.
    fn make(
        device: &VideoDevice,
        width: u32,
        height: u32,
        kbps: u32,
        format: RawImageFormat,
    ) -> Result<BytesEncoderH264, String> {
        let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            return Err(format!("a {width}x{height} frame has no pixels"));
        };
        let bits = u64::from(kbps.max(64)) * 1000;
        let mut output = device
            .encoder_output_parameters_h264_low_latency(RateControl::VariableBitrate {
                average_bitrate: bits,
                max_bitrate: bits * 2,
                virtual_buffer_size: std::time::Duration::from_secs(1),
            })
            .map_err(|e| format!("this device does not encode H.264: {e}"))?;
        output.idr_period = NonZeroU32::new(IDR_PERIOD);
        output.inline_stream_params = Some(true);
        output.color_space = Some(if format.is_rec709() {
            ColorSpace::BT709
        } else {
            ColorSpace::BT601Ntsc
        });
        output.color_range = Some(if format.is_full_range() {
            ColorRange::Full
        } else {
            ColorRange::Limited
        });
        device
            .create_bytes_encoder_h264(EncoderParametersH264 {
                input_parameters: VideoParameters {
                    width: w,
                    height: h,
                    target_framerate: TARGET_FPS.into(),
                },
                output_parameters: output,
            })
            .map_err(|e| format!("the H.264 encoder did not open ({width}x{height}): {e}"))
    }

    /// The rate the encoder spends (kbit/s).
    pub fn bitrate_kbps(&self) -> u32 {
        self.bitrate_kbps
    }

    /// Spend `kbps` from the next frame on: a new session at that rate (its
    /// first frame a keyframe). The old session stays when the device
    /// refuses the new one.
    pub fn set_bitrate(&mut self, kbps: u32) {
        match Self::make(&self.device, self.width, self.height, kbps, self.format) {
            Ok(encoder) => {
                self.encoder = encoder;
                self.bitrate_kbps = kbps;
            }
            Err(why) => eprintln!("[video] Vulkan Video encoder kept its rate: {why}"),
        }
    }

    /// Encode one frame (NV12, BGRA8 or RGBA8, at the session's size) into
    /// Annex-B, stamped `micros` when given. Empty for a frame of another
    /// size, a short frame, or a failed encode.
    pub fn encode(
        &mut self,
        frame: &VideoFrame,
        force_keyframe: bool,
        micros: Option<i64>,
    ) -> Vec<u8> {
        if (frame.width, frame.height) != (self.width, self.height) {
            return Vec::new();
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let wanted = session_format(frame.format);
        if wanted != self.format {
            // Frames in another matrix or range: a session tagged for them.
            match Self::make(
                &self.device,
                self.width,
                self.height,
                self.bitrate_kbps,
                wanted,
            ) {
                Ok(encoder) => {
                    self.encoder = encoder;
                    self.format = wanted;
                }
                Err(why) => {
                    eprintln!("[video] Vulkan Video encoder: {why}");
                    return Vec::new();
                }
            }
        }
        let Some(total) = Nv12Layout::new(w, h).checked_total_len() else {
            return Vec::new();
        };
        let nv12 = if frame.format.is_nv12() {
            match frame.bytes.as_ref().get(..total) {
                Some(planes) => planes.to_vec(),
                None => return Vec::new(),
            }
        } else {
            match rgba_to_nv12(frame.bytes.as_ref(), w, h, frame.format, self.format) {
                Some(planes) => planes,
                None => return Vec::new(),
            }
        };
        let input = InputFrame {
            data: RawFrameData {
                frame: nv12,
                width: self.width,
                height: self.height,
            },
            pts: micros.and_then(|m| u64::try_from(m).ok()),
        };
        match self.encoder.encode(&input, force_keyframe) {
            Ok(chunk) => chunk.data,
            Err(e) => {
                eprintln!("[video] Vulkan Video encode failed: {e}");
                Vec::new()
            }
        }
    }
}
