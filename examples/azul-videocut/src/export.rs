//! The export: the sequence rendered frame by frame on a worker, encoded to
//! H.264 by azul's `VideoEncoder` (VideoToolbox in hardware on macOS) with
//! every frame stamped with its own time (`encode_at`), and muxed into an
//! MP4 by azul's `Mp4Muxer`. A machine with no H.264 encoder (Linux and
//! Windows today) gets an uncompressed Y4M file instead (ffmpeg, VLC and mpv
//! play it), and a build whose muxer does not open gets the raw H.264
//! elementary stream (`.h264`).
//!
//! The pure parts (the frames, the sizes, the stamps, the colour
//! conversion, the Y4M records) are unit-tested; [`run_export`] is the
//! worker body.

use std::{
    ops::Range,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use azul::{
    image::{RawImage, RawImageFormat},
    vec::{U8Vec, U8VecRef},
    video::{Mp4Muxer, VideoEncoder, VideoFrame},
};

use crate::{
    model::{Frame, Project},
    render::{compose, FrameSource},
};

/// Which frames to export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportRange {
    /// The whole sequence.
    Sequence,
    /// From frame `from` to frame `to`, both included (the program
    /// monitor's in and out marks).
    Marked { from: Frame, to: Frame },
    /// The first `n` frames (the E2E's "a few frames").
    First(Frame),
}

/// The file an export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// H.264 in an MP4.
    Mp4,
    /// The raw H.264 elementary stream (the muxer did not open).
    H264,
    /// Uncompressed YUV 4:2:0 (no H.264 encoder on this machine).
    Y4m,
}

impl OutputFormat {
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Mp4 => "mp4",
            OutputFormat::H264 => "h264",
            OutputFormat::Y4m => "y4m",
        }
    }
}

/// The frames `range` covers in `project`.
#[must_use]
pub fn export_frames(project: &Project, range: ExportRange) -> Range<Frame> {
    match range {
        ExportRange::Sequence => 0..project.sequence.end(),
        ExportRange::Marked { from, to } => {
            if to < from {
                from..from
            } else {
                from..to + 1
            }
        }
        ExportRange::First(n) => 0..n.max(0),
    }
}

/// `width` x `height` made even (4:2:0 and H.264 want even sizes).
#[must_use]
pub fn even_size(width: u32, height: u32) -> (u32, u32) {
    ((width / 2 * 2).max(2), (height / 2 * 2).max(2))
}

/// A keyframe every two seconds of the export (frame `i` of it).
#[must_use]
pub fn is_keyframe(i: i64, fps: u32) -> bool {
    i % (2 * i64::from(fps.max(1))) == 0
}

/// Frame `i`'s presentation time in microseconds at `fps`.
#[must_use]
#[allow(clippy::cast_sign_loss)]
pub fn timestamp_us(i: i64, fps: u32) -> u64 {
    (i.max(0) as u64) * 1_000_000 / u64::from(fps.max(1))
}

/// The export's file name: `name` without path separators, with the
/// format's extension.
#[must_use]
pub fn output_name(name: &str, format: OutputFormat) -> String {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '-' } else { c })
        .collect();
    let base = if clean.is_empty() { "export" } else { clean.as_str() };
    format!("{base}.{}", format.extension())
}

/// RGBA8 to planar I420, BT.601 video range (Y 16..235, Cb / Cr 16..240),
/// chroma averaged over each 2 x 2 block: azul's one RGB -> YCbCr conversion
/// (`RawImage::rgba_to_nv12` to NV12 Rec.601 video range) with its
/// interleaved Cb,Cr pairs split into the two chroma planes. A picture
/// shorter than its size is black.
#[must_use]
pub fn rgba_to_i420(rgba: &[u8], width: u32, height: u32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (w, h) = (width as usize, height as usize);
    let luma = w * h;
    let chroma = w.div_ceil(2) * h.div_ceil(2);
    let nv12 = RawImage::rgba_to_nv12(
        U8VecRef::from(rgba),
        width,
        height,
        RawImageFormat::RGBA8,
        RawImageFormat::NV12Rec601Video,
    )
    .into_option();
    let Some(nv12) = nv12 else {
        return (vec![16; luma], vec![128; chroma], vec![128; chroma]);
    };
    let nv12 = nv12.as_slice();
    let (y, uv) = nv12.split_at(luma.min(nv12.len()));
    let u = uv.iter().step_by(2).copied().collect();
    let v = uv.iter().skip(1).step_by(2).copied().collect();
    (y.to_vec(), u, v)
}

/// The Y4M stream header for `width` x `height` at `fps`.
#[must_use]
pub fn y4m_header(width: u32, height: u32, fps: u32) -> String {
    format!("YUV4MPEG2 W{width} H{height} F{fps}:1 Ip A1:1 C420jpeg\n")
}

/// One Y4M frame record of an RGBA8 picture.
pub fn append_y4m_frame(out: &mut Vec<u8>, rgba: &[u8], width: u32, height: u32) {
    let (y, u, v) = rgba_to_i420(rgba, width, height);
    out.extend_from_slice(b"FRAME\n");
    out.extend_from_slice(&y);
    out.extend_from_slice(&u);
    out.extend_from_slice(&v);
}

/// What an export is asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportSettings {
    pub width: u32,
    pub height: u32,
    pub bitrate_kbps: u32,
    pub range: ExportRange,
    /// The file's name without extension.
    pub name: String,
}

/// How far an export got; the UI reads it from a timer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportProgress {
    pub done: i64,
    pub total: i64,
    pub finished: bool,
    pub error: Option<String>,
    /// The Drive key or path the file went to.
    pub output: Option<String>,
    /// "VideoToolbox H.264 + MP4", "Y4M (no H.264 encoder)", ...
    pub how: String,
}

impl ExportProgress {
    /// 0.0 ..= 100.0.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn percent(&self) -> f32 {
        if self.total <= 0 {
            return if self.finished { 100.0 } else { 0.0 };
        }
        (self.done as f32 / self.total as f32 * 100.0).clamp(0.0, 100.0)
    }
}

/// The worker's side of an export: its progress and the cancel switch.
#[derive(Debug, Default)]
pub struct ExportShared {
    pub progress: Mutex<ExportProgress>,
    pub cancel: AtomicBool,
}

impl ExportShared {
    fn set(&self, f: impl FnOnce(&mut ExportProgress)) {
        if let Ok(mut p) = self.progress.lock() {
            f(&mut p);
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Renders, encodes and muxes the export; the file's bytes and format.
/// Runs on a worker: it blocks for the whole export.
pub fn run_export(
    project: &Project,
    settings: &ExportSettings,
    source: &mut dyn FrameSource,
    shared: &ExportShared,
) -> Result<(Vec<u8>, OutputFormat), String> {
    let frames = export_frames(project, settings.range);
    let total = frames.end - frames.start;
    if total <= 0 {
        return Err(String::from("there are no frames to export"));
    }
    let (w, h) = even_size(settings.width, settings.height);
    let fps = project.sequence.fps;
    shared.set(|p| {
        p.total = total;
        p.done = 0;
    });
    let mut encoder = VideoEncoder::open(w, h, false, settings.bitrate_kbps.max(100));
    if !encoder.is_open() {
        shared.set(|p| p.how = String::from("Y4M, uncompressed (no H.264 encoder on this machine)"));
        let mut out = y4m_header(w, h, fps).into_bytes();
        for (i, f) in frames.enumerate() {
            if shared.cancelled() {
                return Err(String::from("cancelled"));
            }
            let picture = compose(project, f, w, h, source);
            append_y4m_frame(&mut out, &picture.rgba, w, h);
            #[allow(clippy::cast_possible_wrap)]
            let done = i as i64 + 1;
            shared.set(|p| p.done = done);
        }
        return Ok((out, OutputFormat::Y4m));
    }
    let mut muxer = Mp4Muxer::create(w, h, fps as f32);
    let muxed = muxer.is_open();
    shared.set(|p| {
        p.how = if muxed {
            format!("H.264 ({}) in MP4", VideoEncoder::backend_name().as_str())
        } else {
            format!("raw H.264 ({}), no MP4 muxer in this build", VideoEncoder::backend_name().as_str())
        };
    });
    let mut raw: Vec<u8> = Vec::new();
    let take = |encoder: &mut VideoEncoder, muxer: &mut Mp4Muxer, raw: &mut Vec<u8>| -> Result<(), String> {
        while let Some(packet) = encoder.recv_packet().into_option() {
            if muxed {
                if !muxer.write_annexb(packet) {
                    return Err(format!("the MP4 muxer refused a packet: {}", muxer.last_error().as_str()));
                }
            } else {
                raw.extend_from_slice(packet.as_slice());
            }
        }
        Ok(())
    };
    for (i, f) in frames.enumerate() {
        if shared.cancelled() {
            return Err(String::from("cancelled"));
        }
        let picture = compose(project, f, w, h, source);
        #[allow(clippy::cast_possible_wrap)]
        let index = i as i64;
        let frame = VideoFrame {
            width: w,
            height: h,
            bytes: U8Vec::from_vec(picture.rgba),
            format: RawImageFormat::RGBA8,
        };
        if !encoder.encode_at(frame, timestamp_us(index, fps), is_keyframe(index, fps)) {
            return Err(String::from("the encoder refused a frame"));
        }
        take(&mut encoder, &mut muxer, &mut raw)?;
        shared.set(|p| p.done = index + 1);
    }
    take(&mut encoder, &mut muxer, &mut raw)?;
    if muxed {
        let file = muxer.finish();
        if file.as_slice().is_empty() {
            return Err(format!("the MP4 muxer wrote nothing: {}", muxer.last_error().as_str()));
        }
        Ok((file.as_slice().to_vec(), OutputFormat::Mp4))
    } else {
        Ok((raw, OutputFormat::H264))
    }
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
