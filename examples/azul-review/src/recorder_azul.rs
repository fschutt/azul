//! azul's own encoders behind the recorder's codec traits (`recorder_codec.rs`).
//!
//! The picture goes to azul's `VideoEncoder` (VideoToolbox on a Mac, in hardware; Vulkan Video where
//! the GPU encodes H.264; nowhere else yet) and the voice to its `AudioEncoder` (Opus through
//! AudioToolbox, Apple only). Both run on threads of their own inside the engine, so handing a picture
//! over costs the window's thread a move of its bytes and nothing else, and a packet is a
//! `recv_packet` away. Where an engine does not open, the factory says why in words and the recorder
//! turns that track off in the manifest and goes on with the others.

use azul::{
    audio::{AudioConfig, AudioEncoder, AudioFrame},
    image::RawImageFormat,
    vec::{F32Vec, U8Vec},
    video::{VideoEncoder, VideoFrame},
};

use crate::recorder_codec::{in_background, AudioCodec, CodecFactory, PixelFormat, RawFrame, VideoCodec};

/// azul's `VideoEncoder` as a [`VideoCodec`].
pub struct AzulVideoCodec {
    encoder: VideoEncoder,
}

impl VideoCodec for AzulVideoCodec {
    fn submit(&mut self, frame: RawFrame, keyframe: bool) -> bool {
        let format = match frame.format {
            PixelFormat::Rgba8 => RawImageFormat::RGBA8,
            PixelFormat::Bgra8 => RawImageFormat::BGRA8,
            PixelFormat::Nv12 => RawImageFormat::NV12Rec709Video,
        };
        self.encoder.encode(
            VideoFrame {
                width: frame.width,
                height: frame.height,
                bytes: U8Vec::from_vec(frame.bytes),
                format,
            },
            keyframe,
        )
    }

    fn poll(&mut self) -> Option<Vec<u8>> {
        self.encoder.recv_packet().into_option().map(|p| p.as_slice().to_vec())
    }

    fn flush(&mut self) {
        self.encoder.flush();
    }

    fn name(&self) -> String {
        VideoEncoder::backend_name().as_str().to_string()
    }

    fn hardware(&self) -> bool {
        self.encoder.is_hardware()
    }
}

/// azul's `AudioEncoder` as an [`AudioCodec`].
pub struct AzulAudioCodec {
    encoder: AudioEncoder,
    sample_rate: u32,
    channels: u16,
}

impl AudioCodec for AzulAudioCodec {
    fn submit(&mut self, samples: &[f32]) -> bool {
        self.encoder.encode(AudioFrame {
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples: F32Vec::from(samples.to_vec()),
        })
    }

    fn poll(&mut self) -> Option<Vec<u8>> {
        self.encoder.recv_packet().into_option().map(|p| p.as_slice().to_vec())
    }

    fn name(&self) -> String {
        AudioEncoder::backend_name().as_str().to_string()
    }
}

/// The side of the picture the warm-up session is for, pixels (what the engine's own probe uses).
const WARM_UP_SIDE: u32 = 128;
/// Its bit rate.
const WARM_UP_KBPS: u32 = 400;

/// Opens azul's encoders.
#[derive(Debug, Clone, Copy, Default)]
pub struct AzulFactory;

impl CodecFactory for AzulFactory {
    fn video(&self, width: u32, height: u32, kbps: u32) -> Result<Box<dyn VideoCodec>, String> {
        let encoder = VideoEncoder::open(width, height, false, kbps);
        if encoder.is_open() {
            Ok(Box::new(AzulVideoCodec { encoder }))
        } else {
            Err(format!(
                "no H.264 encoder: the {} engine did not open a {width}x{height} session on this machine (the [video] lines of the log say why)",
                VideoEncoder::backend_name().as_str()
            ))
        }
    }

    fn warm_up(&self) {
        // A small session, opened and closed on a thread of its own: the engine's first VideoToolbox session of the process
        // (its probe, the framework, the hardware encoder) is 0.3 to 0.4 s, which the real open at the first picture
        // paid on the window's thread (a stack sample of the demo: 173 of 1536 samples of the main thread in
        // `AzVideoEncoder::open`). The encoder is made and dropped on that thread; nothing is handed over.
        in_background("recorder-warmup", || drop(VideoEncoder::open(WARM_UP_SIDE, WARM_UP_SIDE, false, WARM_UP_KBPS)));
    }

    fn audio(&self, sample_rate: u32, channels: u16, kbps: u32) -> Result<Box<dyn AudioCodec>, String> {
        let encoder = AudioEncoder::create(AudioConfig { sample_rate, channels }, kbps);
        if encoder.is_open() {
            Ok(Box::new(AzulAudioCodec { encoder, sample_rate, channels }))
        } else {
            Err(format!(
                "no Opus encoder: the {} engine is not available on this machine",
                AudioEncoder::backend_name().as_str()
            ))
        }
    }
}

#[cfg(test)]
#[path = "recorder_azul_test.rs"]
mod recorder_azul_test;
