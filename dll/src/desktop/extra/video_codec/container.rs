//! MP4 container I/O for apps: [`Mp4Demuxer`] and [`Mp4Muxer`].
//!
//! A video EDITOR seeks: to show the frame at a time it starts a decoder at
//! the keyframe at or before that frame and decodes forward. That needs an
//! MP4's H.264 track as access units BY INDEX - the [`Mp4Demuxer`] (the
//! eager `decode_mp4_h264` decodes a whole clip into memory instead). An
//! EXPORT encodes frames with a `VideoEncoder` and needs its Annex-B access
//! units in a file - the [`Mp4Muxer`].
//!
//! The wire format is Annex-B on both sides (start-code-prefixed NAL units,
//! the parameter sets in front of every keyframe), exactly what
//! `VideoEncoder::recv_packet` hands out and `VideoDecoder::decode` takes;
//! MP4 stores AVCC (length-prefixed NALs) with the parameter sets in the
//! `avcC` box, and the rewrite both ways lives here, once
//! ([`annexb_nals`], [`append_avcc_as_annexb`], [`annexb_to_avcc`]).
//!
//! The handles are C-ABI handles like `VideoDecoder`: always present, the
//! `mp4` crate behind them is the `video-native` feature's. Without it the
//! demuxer reports why it did not open and the muxer does not open.
//!
//! Seeking assumes a decoder that hands frames out in DECODE order, one per
//! access unit (VideoToolbox's synchronous decode): to show access unit `i`,
//! feed `keyframe_before(i)..=i` and keep the last frame. [`shown_at`] finds
//! `i` by presentation time, so B-frame streams map times to the right
//! access unit.
//!
//! Key types: [`Mp4Demuxer`], [`Mp4Muxer`], `azul_core::video::VideoChunk`.

use core::ffi::c_void;

use azul_core::video::{OptionVideoChunk, VideoChunk};
use azul_css::{AzString, U8Vec};

/// The track timescale the muxer writes (ticks per second): 90 kHz, the
/// MPEG clock, so the common frame rates are whole ticks per frame.
#[cfg_attr(not(feature = "video-native"), allow(dead_code))]
const MUX_TIMESCALE: u32 = 90_000;

// ---------------------------------------------------------------------------
// Annex-B <-> AVCC
// ---------------------------------------------------------------------------

/// The NAL units of an Annex-B stream (3- or 4-byte start codes), each
/// without its start code.
pub(crate) fn annexb_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut start: Option<usize> = None;
    while i + 3 <= data.len() {
        let (is_sc, sc_len) = if data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                (true, 3)
            } else if i + 4 <= data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                (true, 4)
            } else {
                (false, 0)
            }
        } else {
            (false, 0)
        };
        if is_sc {
            if let Some(s) = start {
                if i > s {
                    out.push(&data[s..i]);
                }
            }
            i += sc_len;
            start = Some(i);
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        if s < data.len() {
            out.push(&data[s..]);
        }
    }
    out
}

/// Rewrite one AVCC sample (a run of `[u32 big-endian length][NAL bytes]`)
/// into Annex-B by replacing each length prefix with a start code, appended
/// to `out`. A malformed tail (a length that runs past the buffer) stops the
/// walk rather than panicking.
pub(crate) fn append_avcc_as_annexb(avcc: &[u8], out: &mut Vec<u8>) {
    let mut i = 0usize;
    while i + 4 <= avcc.len() {
        let len = u32::from_be_bytes([avcc[i], avcc[i + 1], avcc[i + 2], avcc[i + 3]]) as usize;
        i += 4;
        if len == 0 || i + len > avcc.len() {
            break;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&avcc[i..i + len]);
        i += len;
    }
}

/// One Annex-B access unit rewritten for an MP4 sample.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AvccSample {
    /// The sample: every NAL but the parameter sets and access-unit
    /// delimiters, each behind its 4-byte big-endian length.
    pub bytes: Vec<u8>,
    /// The first sequence parameter set (NAL type 7) the unit carried.
    pub sps: Option<Vec<u8>>,
    /// The first picture parameter set (NAL type 8) the unit carried.
    pub pps: Option<Vec<u8>>,
    /// The unit holds an IDR slice (NAL type 5).
    pub keyframe: bool,
}

/// Rewrite an Annex-B access unit as an MP4 sample: the parameter sets go
/// to the `avcC` box (returned beside the sample), access-unit delimiters
/// are dropped, every other NAL is length-prefixed.
pub(crate) fn annexb_to_avcc(data: &[u8]) -> AvccSample {
    let mut out = AvccSample {
        bytes: Vec::with_capacity(data.len() + 16),
        ..AvccSample::default()
    };
    for nal in annexb_nals(data) {
        if nal.is_empty() {
            continue;
        }
        match nal[0] & 0x1f {
            7 => {
                if out.sps.is_none() {
                    out.sps = Some(nal.to_vec());
                }
            }
            8 => {
                if out.pps.is_none() {
                    out.pps = Some(nal.to_vec());
                }
            }
            // Access-unit delimiter: MP4 samples do not carry it.
            9 => {}
            kind => {
                out.keyframe |= kind == 5;
                #[allow(clippy::cast_possible_truncation)]
                out.bytes
                    .extend_from_slice(&(nal.len() as u32).to_be_bytes());
                out.bytes.extend_from_slice(nal);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Seeking
// ---------------------------------------------------------------------------

/// The index (decode order) of the access unit SHOWN at `time_ms`: the one
/// with the latest presentation time at or before `time_ms`; before the
/// first presentation time, the first one shown. `0` for no units.
pub(crate) fn shown_at(chunks: &[VideoChunk], time_ms: f64) -> usize {
    let mut best: Option<(usize, f64)> = None;
    let mut first: Option<(usize, f64)> = None;
    for (i, c) in chunks.iter().enumerate() {
        if first.map_or(true, |(_, p)| c.pts_ms < p) {
            first = Some((i, c.pts_ms));
        }
        if c.pts_ms <= time_ms && best.map_or(true, |(_, p)| c.pts_ms >= p) {
            best = Some((i, c.pts_ms));
        }
    }
    best.or(first).map_or(0, |(i, _)| i)
}

/// The keyframe a decoder must start at to reach access unit `index`: the
/// last keyframe at or before it (decode order); `0` when there is none.
/// An index past the end counts as the last unit.
pub(crate) fn keyframe_at_or_before(chunks: &[VideoChunk], index: usize) -> usize {
    if chunks.is_empty() {
        return 0;
    }
    let index = index.min(chunks.len() - 1);
    (0..=index).rev().find(|&i| chunks[i].is_keyframe).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Demuxer
// ---------------------------------------------------------------------------

/// What an [`Mp4Demuxer`] holds: the stream's geometry and its access units
/// in decode order, or why it did not open.
struct DemuxerInner {
    width: u32,
    height: u32,
    fps: f32,
    duration_ms: f64,
    chunks: Vec<VideoChunk>,
    /// Empty when the file opened.
    error: String,
}

impl DemuxerInner {
    fn failed(error: String) -> Self {
        Self {
            width: 0,
            height: 0,
            fps: 0.0,
            duration_ms: 0.0,
            chunks: Vec::new(),
            error,
        }
    }

    fn open(bytes: &[u8]) -> Self {
        #[cfg(feature = "video-native")]
        {
            match super::demux::demux_mp4_h264(bytes) {
                Ok(d) => Self::from_demuxed(d),
                Err(e) => Self::failed(e),
            }
        }
        #[cfg(not(feature = "video-native"))]
        {
            let _ = bytes;
            Self::failed(String::from(
                "this build has no `video-native` feature: MP4 demux is compiled out",
            ))
        }
    }

    #[cfg(feature = "video-native")]
    fn from_demuxed(d: super::demux::DemuxedH264) -> Self {
        let chunks: Vec<VideoChunk> = d
            .chunks
            .into_iter()
            .map(|c| VideoChunk {
                pts_ms: c.pts_ms,
                data: U8Vec::from_vec(c.annexb),
                is_keyframe: c.is_keyframe,
            })
            .collect();
        let (fps, duration_ms) = timing_of(&chunks, d.fps);
        Self {
            width: d.width,
            height: d.height,
            fps,
            duration_ms,
            chunks,
            error: String::new(),
        }
    }
}

/// The frame rate and the length of a stream from its presentation times:
/// the rate is the units over the span between the first and the last one
/// shown (the container's own rate, `fallback_fps`, when there is no span),
/// the length runs to the end of the last frame.
#[cfg_attr(not(feature = "video-native"), allow(dead_code))]
fn timing_of(chunks: &[VideoChunk], fallback_fps: f32) -> (f32, f64) {
    let first = chunks.iter().map(|c| c.pts_ms).fold(f64::INFINITY, f64::min);
    let last = chunks.iter().map(|c| c.pts_ms).fold(f64::NEG_INFINITY, f64::max);
    let span = last - first;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let fps = if chunks.len() >= 2 && span > 0.0 {
        ((chunks.len() - 1) as f64 * 1000.0 / span) as f32
    } else if fallback_fps > 0.0 {
        fallback_fps
    } else {
        30.0
    };
    let duration_ms = if chunks.is_empty() {
        0.0
    } else {
        last + 1000.0 / f64::from(fps)
    };
    (fps, duration_ms)
}

/// An MP4 file's H.264 track as Annex-B access units, by index.
///
/// `create` parses the whole file at once (the bytes stay in memory); the
/// handle then answers the stream's size, rate and length, hands out any
/// access unit ([`chunk`](Self::chunk), decode order) and plans a seek:
/// [`frame_at`](Self::frame_at) is the unit shown at a time,
/// [`keyframe_before`](Self::keyframe_before) the keyframe a decoder starts
/// at to reach it. A C-ABI handle like `VideoDecoder`.
#[repr(C)]
pub struct Mp4Demuxer {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for Mp4Demuxer {
    fn clone(&self) -> Self {
        Mp4Demuxer {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for Mp4Demuxer {
    fn default() -> Self {
        Mp4Demuxer {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Mp4Demuxer {
    /// Parse the MP4 file in `bytes` and take its H.264 track. On a file
    /// that is not an MP4, has no H.264 track or no parameter sets (or a
    /// build without `video-native`), the handle does not open and
    /// [`error`](Self::error) says why.
    pub fn create(bytes: U8Vec) -> Mp4Demuxer {
        let inner = Box::new(DemuxerInner::open(bytes.as_ref()));
        Mp4Demuxer {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    fn inner(&self) -> Option<&DemuxerInner> {
        unsafe { (self.ptr as *const DemuxerInner).as_ref() }
    }

    /// Whether the file was demuxed.
    pub fn is_open(&self) -> bool {
        self.inner().is_some_and(|i| i.error.is_empty())
    }

    /// Why the file did not open; empty when it did.
    pub fn open_error(&self) -> AzString {
        AzString::from(
            self.inner()
                .map_or_else(|| String::from("no file"), |i| i.error.clone()),
        )
    }

    /// The coded width in pixels (0 when not open).
    pub fn width(&self) -> u32 {
        self.inner().map_or(0, |i| i.width)
    }

    /// The coded height in pixels (0 when not open).
    pub fn height(&self) -> u32 {
        self.inner().map_or(0, |i| i.height)
    }

    /// Frames per second, from the presentation times.
    pub fn fps(&self) -> f32 {
        self.inner().map_or(0.0, |i| i.fps)
    }

    /// The stream's length in milliseconds: to the end of the last frame.
    pub fn duration_ms(&self) -> f64 {
        self.inner().map_or(0.0, |i| i.duration_ms)
    }

    /// How many access units the track has.
    pub fn chunk_count(&self) -> usize {
        self.inner().map_or(0, |i| i.chunks.len())
    }

    /// Access unit `index` in decode order (a copy of its bytes), or `None`
    /// past the end.
    pub fn chunk(&self, index: usize) -> OptionVideoChunk {
        self.inner()
            .and_then(|i| i.chunks.get(index).cloned())
            .into()
    }

    /// The index of the access unit shown at `time_ms`: the latest one
    /// whose presentation time has come (the first one before the start,
    /// the last one past the end).
    pub fn frame_at(&self, time_ms: f64) -> usize {
        self.inner().map_or(0, |i| shown_at(&i.chunks, time_ms))
    }

    /// The keyframe at or before access unit `index`: where a decoder
    /// starts to show `index` (feed `keyframe_before(index)..=index`, keep
    /// the last frame).
    pub fn keyframe_before(&self, index: usize) -> usize {
        self.inner()
            .map_or(0, |i| keyframe_at_or_before(&i.chunks, index))
    }

    /// Release the demuxer and its bytes. (Drop does this too.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut DemuxerInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for Mp4Demuxer {
    fn drop(&mut self) {
        self.drop_inner();
    }
}

// ---------------------------------------------------------------------------
// Muxer
// ---------------------------------------------------------------------------

/// What an [`Mp4Muxer`] holds while it writes.
struct MuxerInner {
    width: u32,
    height: u32,
    fps: f32,
    samples: u64,
    /// The last failure; empty while all is well.
    error: String,
    finished: bool,
    /// Started at the first keyframe (its parameter sets make the `avcC`).
    #[cfg(feature = "video-native")]
    writer: Option<mp4::Mp4Writer<std::io::Cursor<Vec<u8>>>>,
}

impl MuxerInner {
    /// Ticks of [`MUX_TIMESCALE`] per frame.
    #[cfg_attr(not(feature = "video-native"), allow(dead_code))]
    fn sample_duration(&self) -> u32 {
        let fps = if self.fps > 0.0 { self.fps } else { 30.0 };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ticks = (f64::from(MUX_TIMESCALE) / f64::from(fps)).round() as u32;
        ticks.max(1)
    }

    fn write(&mut self, au: &[u8]) -> bool {
        if self.finished {
            self.error = String::from("the file is already finished");
            return false;
        }
        let sample = annexb_to_avcc(au);
        #[cfg(feature = "video-native")]
        {
            if self.writer.is_none() {
                let (keyframe, sps, pps) = (sample.keyframe, sample.sps, sample.pps);
                match (keyframe, sps, pps) {
                    (true, Some(sps), Some(pps)) => match start_writer(self.width, self.height, sps, pps) {
                        Ok(writer) => self.writer = Some(writer),
                        Err(e) => {
                            self.error = e;
                            return false;
                        }
                    },
                    _ => {
                        self.error = String::from(
                            "the stream must start with a keyframe that carries its SPS and PPS",
                        );
                        return false;
                    }
                }
            }
            if sample.bytes.is_empty() {
                self.error = String::from("an access unit without a picture");
                return false;
            }
            let duration = self.sample_duration();
            let mp4_sample = mp4::Mp4Sample {
                start_time: self.samples * u64::from(duration),
                duration,
                rendering_offset: 0,
                is_sync: sample.keyframe,
                bytes: mp4::Bytes::from(sample.bytes),
            };
            let Some(writer) = self.writer.as_mut() else {
                return false;
            };
            match writer.write_sample(1, &mp4_sample) {
                Ok(()) => {
                    self.samples += 1;
                    true
                }
                Err(e) => {
                    self.error = format!("writing sample {} failed: {e}", self.samples);
                    false
                }
            }
        }
        #[cfg(not(feature = "video-native"))]
        {
            let _ = sample;
            self.error = String::from(
                "this build has no `video-native` feature: MP4 mux is compiled out",
            );
            false
        }
    }

    fn finish(&mut self) -> Vec<u8> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;
        #[cfg(feature = "video-native")]
        {
            let Some(mut writer) = self.writer.take() else {
                if self.error.is_empty() {
                    self.error = String::from("no access unit was written");
                }
                return Vec::new();
            };
            if let Err(e) = writer.write_end() {
                self.error = format!("finishing the file failed: {e}");
                return Vec::new();
            }
            writer.into_writer().into_inner()
        }
        #[cfg(not(feature = "video-native"))]
        {
            if self.error.is_empty() {
                self.error = String::from(
                    "this build has no `video-native` feature: MP4 mux is compiled out",
                );
            }
            Vec::new()
        }
    }
}

/// An MP4 writer in memory with one H.264 track of `width` x `height`
/// whose `avcC` holds `sps` / `pps`.
#[cfg(feature = "video-native")]
fn start_writer(
    width: u32,
    height: u32,
    sps: Vec<u8>,
    pps: Vec<u8>,
) -> Result<mp4::Mp4Writer<std::io::Cursor<Vec<u8>>>, String> {
    if sps.len() < 4 {
        return Err(String::from("the sequence parameter set is too short"));
    }
    let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
        return Err(format!("{width}x{height} is too large for an MP4 track"));
    };
    let brand = |b: &[u8; 4]| mp4::FourCC { value: *b };
    let config = mp4::Mp4Config {
        major_brand: brand(b"isom"),
        minor_version: 512,
        compatible_brands: vec![brand(b"isom"), brand(b"iso2"), brand(b"avc1"), brand(b"mp41")],
        timescale: 1000,
    };
    let mut writer = mp4::Mp4Writer::write_start(std::io::Cursor::new(Vec::new()), &config)
        .map_err(|e| format!("starting the file failed: {e}"))?;
    writer
        .add_track(&mp4::TrackConfig {
            track_type: mp4::TrackType::Video,
            timescale: MUX_TIMESCALE,
            language: String::from("und"),
            media_conf: mp4::MediaConfig::AvcConfig(mp4::AvcConfig {
                width,
                height,
                seq_param_set: sps,
                pic_param_set: pps,
            }),
        })
        .map_err(|e| format!("adding the video track failed: {e}"))?;
    Ok(writer)
}

/// Writes an encoder's H.264 access units into an MP4 file, in memory.
///
/// Feed it what `VideoEncoder::recv_packet` hands out, one access unit per
/// [`write_annexb`](Self::write_annexb), in order, at a constant `fps`; the
/// first one must be a keyframe with its parameter sets (an encoder's first
/// packet is). [`finish`](Self::finish) hands the file's bytes back. A C-ABI
/// handle like `VideoEncoder`.
#[repr(C)]
pub struct Mp4Muxer {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for Mp4Muxer {
    fn clone(&self) -> Self {
        Mp4Muxer {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for Mp4Muxer {
    fn default() -> Self {
        Mp4Muxer {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Mp4Muxer {
    /// A muxer for an H.264 track of `width` x `height` at `fps` frames a
    /// second. Does not open in a build without `video-native`.
    pub fn create(width: u32, height: u32, fps: f32) -> Mp4Muxer {
        if !cfg!(feature = "video-native") {
            return Mp4Muxer::default();
        }
        let inner = Box::new(MuxerInner {
            width,
            height,
            fps,
            samples: 0,
            error: String::new(),
            finished: false,
            #[cfg(feature = "video-native")]
            writer: None,
        });
        Mp4Muxer {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    fn inner(&self) -> Option<&MuxerInner> {
        unsafe { (self.ptr as *const MuxerInner).as_ref() }
    }

    fn inner_mut(&mut self) -> Option<&mut MuxerInner> {
        unsafe { (self.ptr as *mut MuxerInner).as_mut() }
    }

    /// Whether the muxer opened.
    pub fn is_open(&self) -> bool {
        self.inner().is_some()
    }

    /// Append one access unit (Annex-B, one picture). `false` when it was
    /// not written: no keyframe with parameter sets started the stream, the
    /// unit holds no picture, or the file is finished ([`error`](Self::error)
    /// says which).
    pub fn write_annexb(&mut self, data: U8Vec) -> bool {
        match self.inner_mut() {
            Some(inner) => inner.write(data.as_ref()),
            None => false,
        }
    }

    /// Access units written so far.
    pub fn samples_written(&self) -> u64 {
        self.inner().map_or(0, |i| i.samples)
    }

    /// Close the file and hand its bytes back (empty when nothing could be
    /// written - [`error`](Self::error) says why). A second call is empty.
    pub fn finish(&mut self) -> U8Vec {
        U8Vec::from_vec(self.inner_mut().map(MuxerInner::finish).unwrap_or_default())
    }

    /// The last failure; empty while all is well.
    pub fn last_error(&self) -> AzString {
        AzString::from(self.inner().map_or_else(
            || {
                String::from(
                    "no muxer: this build has no `video-native` feature (MP4 mux is compiled out)",
                )
            },
            |i| i.error.clone(),
        ))
    }

    /// Release the muxer. (Drop does this too.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut MuxerInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for Mp4Muxer {
    fn drop(&mut self) {
        self.drop_inner();
    }
}

#[cfg(test)]
#[path = "container_tests.rs"]
mod container_tests;
