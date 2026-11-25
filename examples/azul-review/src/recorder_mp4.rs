//! Fragmented MP4 (ISO BMFF) for the recorder's tracks: the muxer, the H.264 and Opus helpers it
//! needs, and a scanner that reads what a file holds and where its last whole fragment ends.
//!
//! WHY OUR OWN. A recording runs for hours and may be killed at any moment, so each track is
//! written as a sequence of SEGMENT files, each a fragmented MP4: `ftyp` + `moov` (the track's
//! description, no samples) first, then one `moof` + `mdat` pair per fragment of about a second or
//! two of media. After a crash the file is whole up to its last complete fragment; [`scan`] finds
//! that point and the recorder cuts the file there. azul's own `Mp4Muxer` writes one H.264 track
//! into a buffer in memory, only finishes a file when it is done and needs a build feature; this
//! one needs nothing but `std`.
//!
//! ONE TRACK PER FILE. The screen, the camera and the microphone are three kinds of files in three
//! folders, each with its own encoder, its own start time on the session clock and its own on and
//! off; a file holds one track, which keeps the muxer small and a failed track from taking the
//! others with it. The exporter (ffmpeg) puts them together.
//!
//! WHAT IS IN A FILE. H.264 (`avc1`, the parameter sets in `avcC`, samples as length-prefixed NAL
//! units: what azul's `VideoEncoder` hands out as Annex-B is rewritten by [`annexb_to_avcc`]) or
//! Opus (`Opus` + `dOps`, 48 kHz, the raw packets azul's `AudioEncoder` hands out). Times are
//! microseconds from the start of the segment (the caller's pts), kept in the track's own
//! timescale (90 kHz for video, 48 kHz for audio) as whole ticks; a sample lasts until the next
//! one's time (a screen that did not change for 20 s is one sample of 20 s). The last sample of a
//! fragment is held until its successor arrives, because its length is not known before.
//!
//! Plain `std`, no azul: unit-tested here, and every output checked against ffprobe, ffmpeg and
//! AVFoundation (`avmediainfo`) during development.

use std::{
    fmt,
    io::{Read, Seek, SeekFrom},
};

// ---------------------------------------------------------------------------
// Bytes
// ---------------------------------------------------------------------------

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put_zeros(out: &mut Vec<u8>, n: usize) {
    out.resize(out.len() + n, 0);
}

/// A box: its size, its four-letter name, its body. A box larger than 4 GB gets the 64-bit size.
fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 16);
    match u32::try_from(body.len() + 8) {
        Ok(size) => {
            put_u32(&mut out, size);
            out.extend_from_slice(kind);
        }
        Err(_) => {
            put_u32(&mut out, 1);
            out.extend_from_slice(kind);
            put_u64(&mut out, body.len() as u64 + 16);
        }
    }
    out.extend_from_slice(body);
    out
}

/// A box with a version byte and 24 flag bits before its body.
fn full_box(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(body.len() + 4);
    inner.push(version);
    inner.extend_from_slice(&flags.to_be_bytes()[1..]);
    inner.extend_from_slice(body);
    boxed(kind, &inner)
}

/// The unity transformation matrix of a movie and a track header.
fn identity_matrix(out: &mut Vec<u8>) {
    for v in [0x0001_0000_u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        put_u32(out, v);
    }
}

// ---------------------------------------------------------------------------
// H.264: Annex-B to the MP4 form, and the sequence parameter set
// ---------------------------------------------------------------------------

/// The NAL units of an Annex-B stream (three- or four-byte start codes), each without its start
/// code and without the zero bytes that pad it before the next one.
#[must_use]
pub fn annexb_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut starts: Vec<usize> = Vec::new(); // the index of the first byte AFTER each start code
    let mut codes: Vec<usize> = Vec::new(); // the index of the first byte OF each start code
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            codes.push(i);
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::with_capacity(starts.len());
    for (k, &start) in starts.iter().enumerate() {
        let mut end = codes.get(k + 1).copied().unwrap_or(data.len());
        while end > start && data[end - 1] == 0 {
            end -= 1;
        }
        if end > start {
            out.push(&data[start..end]);
        }
    }
    out
}

/// One Annex-B access unit as an MP4 sample.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AvccSample {
    /// Every NAL unit but the parameter sets and access-unit delimiters, each behind its length
    /// as four bytes (big-endian).
    pub bytes: Vec<u8>,
    /// The first sequence parameter set (NAL type 7) of the unit.
    pub sps: Option<Vec<u8>>,
    /// The first picture parameter set (NAL type 8) of the unit.
    pub pps: Option<Vec<u8>>,
    /// The unit holds an IDR slice (NAL type 5): a picture a decoder can start at.
    pub keyframe: bool,
}

/// An Annex-B access unit rewritten as an MP4 sample: the parameter sets go to the track's
/// description (returned beside the sample), delimiters are dropped, every other NAL is
/// length-prefixed. SEI and the slices stay, so a decoder reads what the encoder wrote.
#[must_use]
pub fn annexb_to_avcc(au: &[u8]) -> AvccSample {
    let mut out = AvccSample {
        bytes: Vec::with_capacity(au.len() + 16),
        ..AvccSample::default()
    };
    for nal in annexb_nals(au) {
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
            9 => {}
            kind => {
                out.keyframe |= kind == 5;
                let len = u32::try_from(nal.len()).unwrap_or(u32::MAX);
                out.bytes.extend_from_slice(&len.to_be_bytes());
                out.bytes.extend_from_slice(nal);
            }
        }
    }
    out
}

/// What the recorder needs to know of a sequence parameter set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sps {
    pub profile_idc: u8,
    /// The constraint flags and the reserved bits as one byte (`profile_compatibility`).
    pub compatibility: u8,
    pub level_idc: u8,
    /// 1 = 4:2:0, the only format the encoders here make.
    pub chroma_format_idc: u32,
    pub bit_depth_luma: u32,
    pub bit_depth_chroma: u32,
    /// The size of the picture after cropping, in pixels.
    pub width: u32,
    pub height: u32,
    /// The colour description of the stream, when the SPS carries one: primaries, transfer,
    /// matrix, and whether the range is full.
    pub colour: Option<Colour>,
}

/// The colour tags of a stream (ISO/IEC 23091-2 numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    pub primaries: u8,
    pub transfer: u8,
    pub matrix: u8,
    pub full_range: bool,
}

/// The bits of a NAL unit's payload with the emulation-prevention bytes taken out.
struct Bits {
    data: Vec<u8>,
    pos: usize,
}

impl Bits {
    /// `nal` is a whole NAL unit (the header byte included).
    fn new(nal: &[u8]) -> Self {
        let mut data = Vec::with_capacity(nal.len());
        let mut zeros = 0;
        for &b in nal.iter().skip(1) {
            if zeros >= 2 && b == 3 {
                zeros = 0;
                continue;
            }
            zeros = if b == 0 { zeros + 1 } else { 0 };
            data.push(b);
        }
        Self { data, pos: 0 }
    }

    fn bit(&mut self) -> Result<u32, String> {
        let byte = *self.data.get(self.pos / 8).ok_or("the parameter set ends early")?;
        let bit = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Ok(u32::from(bit))
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0_u32;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }

    /// An unsigned Exp-Golomb code.
    fn ue(&mut self) -> Result<u32, String> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return Err(String::from("an Exp-Golomb code is too long"));
            }
        }
        if zeros == 0 {
            return Ok(0);
        }
        Ok((1_u32 << zeros) - 1 + self.bits(zeros)?)
    }

    /// A signed Exp-Golomb code.
    fn se(&mut self) -> Result<i64, String> {
        let k = i64::from(self.ue()?);
        Ok(if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) })
    }

    fn scaling_list(&mut self, size: usize) -> Result<(), String> {
        let (mut last, mut next) = (8_i64, 8_i64);
        for _ in 0..size {
            if next != 0 {
                let delta = self.se()?;
                next = (last + delta + 256).rem_euclid(256);
            }
            last = if next == 0 { last } else { next };
        }
        Ok(())
    }
}

/// Reads a sequence parameter set (a whole NAL unit, type 7).
///
/// # Errors
/// What is wrong with it: it is not an SPS, it ends early, or a field is out of range.
pub fn parse_sps(nal: &[u8]) -> Result<Sps, String> {
    if nal.len() < 4 || nal[0] & 0x1f != 7 {
        return Err(String::from("not a sequence parameter set"));
    }
    let (profile_idc, compatibility, level_idc) = (nal[1], nal[2], nal[3]);
    let mut r = Bits::new(&nal[3..]); // after the three bytes of profile, constraints, level
    let _sps_id = r.ue()?;
    let (mut chroma_format_idc, mut luma, mut chroma, mut separate_planes) = (1_u32, 8_u32, 8_u32, false);
    if matches!(profile_idc, 100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135) {
        chroma_format_idc = r.ue()?;
        if chroma_format_idc > 3 {
            return Err(format!("chroma_format_idc {chroma_format_idc} is out of range"));
        }
        if chroma_format_idc == 3 {
            separate_planes = r.bit()? == 1;
        }
        luma = 8 + r.ue()?;
        chroma = 8 + r.ue()?;
        if luma > 14 || chroma > 14 {
            return Err(String::from("a bit depth is out of range"));
        }
        let _bypass = r.bit()?;
        if r.bit()? == 1 {
            let lists = if chroma_format_idc == 3 { 12 } else { 8 };
            for i in 0..lists {
                if r.bit()? == 1 {
                    r.scaling_list(if i < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    let _log2_max_frame_num = r.ue()?;
    match r.ue()? {
        0 => {
            let _log2_max_poc_lsb = r.ue()?;
        }
        1 => {
            let _always_zero = r.bit()?;
            let _offset_non_ref = r.se()?;
            let _offset_top_bottom = r.se()?;
            let cycle = r.ue()?;
            if cycle > 255 {
                return Err(String::from("num_ref_frames_in_pic_order_cnt_cycle is out of range"));
            }
            for _ in 0..cycle {
                let _offset = r.se()?;
            }
        }
        _ => {}
    }
    let _max_ref_frames = r.ue()?;
    let _gaps = r.bit()?;
    let width_mbs = r.ue()?.saturating_add(1);
    let height_map_units = r.ue()?.saturating_add(1);
    let frame_mbs_only = r.bit()?;
    if frame_mbs_only == 0 {
        let _mbaff = r.bit()?;
    }
    let _direct_8x8 = r.bit()?;
    let (mut crop_l, mut crop_r, mut crop_t, mut crop_b) = (0_u32, 0_u32, 0_u32, 0_u32);
    if r.bit()? == 1 {
        crop_l = r.ue()?;
        crop_r = r.ue()?;
        crop_t = r.ue()?;
        crop_b = r.ue()?;
    }
    let chroma_array_type = if separate_planes { 0 } else { chroma_format_idc };
    let (unit_x, unit_y): (u32, u32) = {
        let (sub_w, sub_h) = match chroma_array_type {
            1 => (2, 2),
            2 => (2, 1),
            _ => (1, 1),
        };
        let field = 2 - frame_mbs_only;
        if chroma_array_type == 0 {
            (1, field)
        } else {
            (sub_w, sub_h * field)
        }
    };
    let width = width_mbs
        .checked_mul(16)
        .and_then(|w| w.checked_sub(unit_x.checked_mul(crop_l.saturating_add(crop_r))?))
        .filter(|w| *w > 0 && *w <= 16_384)
        .ok_or("the picture width is out of range")?;
    let height = height_map_units
        .checked_mul(16 * (2 - frame_mbs_only))
        .and_then(|h| h.checked_sub(unit_y.checked_mul(crop_t.saturating_add(crop_b))?))
        .filter(|h| *h > 0 && *h <= 16_384)
        .ok_or("the picture height is out of range")?;

    // The VUI, as far as the colour description.
    let mut colour = None;
    if r.bit()? == 1 {
        if r.bit()? == 1 && r.bits(8)? == 255 {
            let _sar = r.bits(32)?;
        }
        if r.bit()? == 1 {
            let _overscan_appropriate = r.bit()?;
        }
        if r.bit()? == 1 {
            let _video_format = r.bits(3)?;
            let full_range = r.bit()? == 1;
            if r.bit()? == 1 {
                let (primaries, transfer, matrix) = (r.bits(8)?, r.bits(8)?, r.bits(8)?);
                colour = Some(Colour {
                    primaries: u8::try_from(primaries).unwrap_or(2),
                    transfer: u8::try_from(transfer).unwrap_or(2),
                    matrix: u8::try_from(matrix).unwrap_or(2),
                    full_range,
                });
            }
        }
    }
    Ok(Sps {
        profile_idc,
        compatibility,
        level_idc,
        chroma_format_idc,
        bit_depth_luma: luma,
        bit_depth_chroma: chroma,
        width,
        height,
        colour,
    })
}

// ---------------------------------------------------------------------------
// Opus
// ---------------------------------------------------------------------------

/// The number of samples (at 48 kHz) an Opus packet holds, read from its table-of-contents byte
/// (RFC 6716, section 3.1); `None` for a packet that cannot be one. azul's encoder makes packets
/// of 20 ms (960), but the muxer reads the truth rather than assuming it.
#[must_use]
pub fn opus_packet_samples(packet: &[u8]) -> Option<u32> {
    let toc = *packet.first()?;
    let config = toc >> 3;
    let frame = match config {
        0..=11 => [480, 960, 1920, 2880][usize::from(config % 4)],
        12 | 14 => 480,
        13 | 15 => 960,
        _ => [120, 240, 480, 960][usize::from(config % 4)],
    };
    let frames = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => u32::from(*packet.get(1)? & 0x3f),
    };
    let samples = frame * frames;
    // One packet never holds more than 120 ms.
    (frames > 0 && samples <= 5_760).then_some(samples)
}

// ---------------------------------------------------------------------------
// The track a file holds
// ---------------------------------------------------------------------------

/// The H.264 track's description: the parameter sets and the size they give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoConfig {
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
    pub info: Sps,
}

impl VideoConfig {
    /// The track's description from the first keyframe of the stream (an encoder puts the
    /// parameter sets in front of every keyframe).
    ///
    /// # Errors
    /// The unit is no keyframe with its parameter sets, or they cannot be read.
    pub fn from_keyframe(annexb_au: &[u8]) -> Result<Self, String> {
        let sample = annexb_to_avcc(annexb_au);
        let (Some(sps), Some(pps)) = (sample.sps, sample.pps) else {
            return Err(String::from(
                "the first picture of a file must carry its sequence and picture parameter sets",
            ));
        };
        let info = parse_sps(&sps)?;
        Ok(Self { sps, pps, info })
    }
}

/// The Opus track's description (the `dOps` box).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpusConfig {
    pub channels: u8,
    /// The rate of the audio before it was encoded, Hz (information only: Opus plays at 48 kHz).
    pub input_rate: u32,
    /// Samples a decoder drops at the start: the encoder's lookahead.
    pub pre_skip: u16,
}

impl OpusConfig {
    /// Mono at 48 kHz with libopus's usual 312 samples of lookahead.
    #[must_use]
    pub const fn mono_48k() -> Self {
        Self {
            channels: 1,
            input_rate: 48_000,
            pre_skip: 312,
        }
    }
}

/// What a file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackConfig {
    Video(VideoConfig),
    Opus(OpusConfig),
}

impl TrackConfig {
    /// Ticks a second in the track's timescale.
    #[must_use]
    pub const fn timescale(&self) -> u32 {
        match self {
            Self::Video(_) => VIDEO_TIMESCALE,
            Self::Opus(_) => OPUS_TIMESCALE,
        }
    }
}

/// The video track's timescale: 90 kHz, the MPEG clock, whole ticks for every usual frame rate.
pub const VIDEO_TIMESCALE: u32 = 90_000;
/// The audio track's timescale: Opus's own 48 kHz.
pub const OPUS_TIMESCALE: u32 = 48_000;

/// Seconds between 1904-01-01 (the MP4 epoch) and 1970-01-01.
const MP4_EPOCH_OFFSET_S: u64 = 2_082_844_800;

/// Microseconds as ticks of `timescale`, rounded to the nearest.
fn us_to_ticks(us: u64, timescale: u32) -> u64 {
    let t = u128::from(us) * u128::from(timescale);
    u64::try_from((t + 500_000) / 1_000_000).unwrap_or(u64::MAX)
}

/// Ticks of `timescale` as microseconds, rounded to the nearest.
fn ticks_to_us(ticks: u64, timescale: u32) -> u64 {
    let t = u128::from(ticks) * 1_000_000;
    let scale = u128::from(timescale.max(1));
    u64::try_from((t + scale / 2) / scale).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// The head of the file: ftyp + moov
// ---------------------------------------------------------------------------

fn ftyp() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"iso5");
    put_u32(&mut body, 512);
    for brand in [b"iso5", b"iso6", b"mp41"] {
        body.extend_from_slice(brand);
    }
    boxed(b"ftyp", &body)
}

/// The `avcC` box. A High profile carries the chroma format and bit depths after the parameter sets.
fn avcc_box(video: &VideoConfig) -> Vec<u8> {
    let mut b = vec![1, video.info.profile_idc, video.info.compatibility, video.info.level_idc, 0xff, 0xe1];
    put_u16(&mut b, u16::try_from(video.sps.len()).unwrap_or(u16::MAX));
    b.extend_from_slice(&video.sps);
    b.push(1);
    put_u16(&mut b, u16::try_from(video.pps.len()).unwrap_or(u16::MAX));
    b.extend_from_slice(&video.pps);
    if matches!(video.info.profile_idc, 100 | 110 | 122 | 144) {
        b.push(0xfc | u8::try_from(video.info.chroma_format_idc & 3).unwrap_or(1));
        b.push(0xf8 | u8::try_from(video.info.bit_depth_luma.saturating_sub(8) & 7).unwrap_or(0));
        b.push(0xf8 | u8::try_from(video.info.bit_depth_chroma.saturating_sub(8) & 7).unwrap_or(0));
        b.push(0);
    }
    boxed(b"avcC", &b)
}

fn visual_sample_entry(video: &VideoConfig) -> Vec<u8> {
    let mut b = Vec::new();
    put_zeros(&mut b, 6);
    put_u16(&mut b, 1); // data_reference_index
    put_zeros(&mut b, 16); // pre_defined, reserved
    put_u16(&mut b, u16::try_from(video.info.width).unwrap_or(u16::MAX));
    put_u16(&mut b, u16::try_from(video.info.height).unwrap_or(u16::MAX));
    put_u32(&mut b, 0x0048_0000); // 72 dpi
    put_u32(&mut b, 0x0048_0000);
    put_u32(&mut b, 0);
    put_u16(&mut b, 1); // frame_count
    put_zeros(&mut b, 32); // compressorname
    put_u16(&mut b, 0x0018); // depth
    put_u16(&mut b, 0xffff); // pre_defined
    b.extend_from_slice(&avcc_box(video));
    if let Some(c) = video.info.colour {
        let mut colr = Vec::new();
        colr.extend_from_slice(b"nclx");
        put_u16(&mut colr, u16::from(c.primaries));
        put_u16(&mut colr, u16::from(c.transfer));
        put_u16(&mut colr, u16::from(c.matrix));
        colr.push(if c.full_range { 0x80 } else { 0 });
        b.extend_from_slice(&boxed(b"colr", &colr));
    }
    boxed(b"avc1", &b)
}

fn opus_sample_entry(opus: &OpusConfig) -> Vec<u8> {
    let mut b = Vec::new();
    put_zeros(&mut b, 6);
    put_u16(&mut b, 1); // data_reference_index
    put_zeros(&mut b, 8); // version, revision, vendor
    put_u16(&mut b, u16::from(opus.channels));
    put_u16(&mut b, 16); // samplesize
    put_u32(&mut b, 0); // pre_defined, reserved
    put_u32(&mut b, OPUS_TIMESCALE << 16);
    let mut ops = vec![0, opus.channels];
    put_u16(&mut ops, opus.pre_skip);
    put_u32(&mut ops, opus.input_rate);
    put_u16(&mut ops, 0); // output gain
    ops.push(0); // channel mapping family 0: mono or stereo
    b.extend_from_slice(&boxed(b"dOps", &ops));
    boxed(b"Opus", &b)
}

fn empty_table(kind: &[u8; 4]) -> Vec<u8> {
    full_box(kind, 0, 0, &[0, 0, 0, 0])
}

/// The `moov` of a fragmented file: the one track, with empty sample tables, and `mvex` to say
/// that fragments follow.
fn moov(track: &TrackConfig, created_unix_s: u64) -> Vec<u8> {
    let created = u32::try_from(created_unix_s + MP4_EPOCH_OFFSET_S).unwrap_or(u32::MAX);
    let timescale = track.timescale();
    let is_video = matches!(track, TrackConfig::Video(_));

    let mut mvhd = Vec::new();
    put_u32(&mut mvhd, created);
    put_u32(&mut mvhd, created);
    put_u32(&mut mvhd, 1_000);
    put_u32(&mut mvhd, 0); // duration: unknown while recording
    put_u32(&mut mvhd, 0x0001_0000); // rate
    put_u16(&mut mvhd, 0x0100); // volume
    put_zeros(&mut mvhd, 10);
    identity_matrix(&mut mvhd);
    put_zeros(&mut mvhd, 24);
    put_u32(&mut mvhd, 2); // next_track_ID

    let mut tkhd = Vec::new();
    put_u32(&mut tkhd, created);
    put_u32(&mut tkhd, created);
    put_u32(&mut tkhd, 1); // track_ID
    put_u32(&mut tkhd, 0);
    put_u32(&mut tkhd, 0); // duration
    put_zeros(&mut tkhd, 8);
    put_u16(&mut tkhd, 0); // layer
    put_u16(&mut tkhd, 0); // alternate_group
    put_u16(&mut tkhd, if is_video { 0 } else { 0x0100 });
    put_u16(&mut tkhd, 0);
    identity_matrix(&mut tkhd);
    match track {
        TrackConfig::Video(v) => {
            put_u32(&mut tkhd, v.info.width << 16);
            put_u32(&mut tkhd, v.info.height << 16);
        }
        TrackConfig::Opus(_) => put_zeros(&mut tkhd, 8),
    }

    let mut mdhd = Vec::new();
    put_u32(&mut mdhd, created);
    put_u32(&mut mdhd, created);
    put_u32(&mut mdhd, timescale);
    put_u32(&mut mdhd, 0);
    put_u16(&mut mdhd, 0x55c4); // language: und
    put_u16(&mut mdhd, 0);

    let mut hdlr = Vec::new();
    put_u32(&mut hdlr, 0);
    hdlr.extend_from_slice(if is_video { b"vide" } else { b"soun" });
    put_zeros(&mut hdlr, 12);
    hdlr.extend_from_slice(if is_video { b"AzReview video\0" } else { b"AzReview audio\0" });

    let mut dref = vec![0, 0, 0, 1];
    dref.extend_from_slice(&full_box(b"url ", 0, 1, &[]));
    let dinf = boxed(b"dinf", &full_box(b"dref", 0, 0, &dref));

    let entry = match track {
        TrackConfig::Video(v) => visual_sample_entry(v),
        TrackConfig::Opus(o) => opus_sample_entry(o),
    };
    let mut stsd = vec![0, 0, 0, 1];
    stsd.extend_from_slice(&entry);
    let mut stbl = full_box(b"stsd", 0, 0, &stsd);
    stbl.extend_from_slice(&empty_table(b"stts"));
    stbl.extend_from_slice(&empty_table(b"stsc"));
    stbl.extend_from_slice(&full_box(b"stsz", 0, 0, &[0; 8]));
    stbl.extend_from_slice(&empty_table(b"stco"));

    let mut minf = if is_video {
        full_box(b"vmhd", 0, 1, &[0; 8])
    } else {
        full_box(b"smhd", 0, 0, &[0; 4])
    };
    minf.extend_from_slice(&dinf);
    minf.extend_from_slice(&boxed(b"stbl", &stbl));

    let mut mdia = full_box(b"mdhd", 0, 0, &mdhd);
    mdia.extend_from_slice(&full_box(b"hdlr", 0, 0, &hdlr));
    mdia.extend_from_slice(&boxed(b"minf", &minf));

    let mut trak = full_box(b"tkhd", 0, 3, &tkhd);
    trak.extend_from_slice(&boxed(b"mdia", &mdia));

    let mut trex = Vec::new();
    put_u32(&mut trex, 1);
    put_u32(&mut trex, 1); // default_sample_description_index
    put_zeros(&mut trex, 12);

    let mut body = full_box(b"mvhd", 0, 0, &mvhd);
    body.extend_from_slice(&boxed(b"trak", &trak));
    body.extend_from_slice(&boxed(b"mvex", &full_box(b"trex", 0, 0, &trex)));
    boxed(b"moov", &body)
}

// ---------------------------------------------------------------------------
// The fragment writer
// ---------------------------------------------------------------------------

/// Why a sample was not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MuxError {
    /// The file was finished already.
    Finished,
    /// A sample without a byte in it.
    EmptySample,
    /// A sample larger than [`Limits::max_sample_bytes`].
    SampleTooLarge { bytes: usize, limit: usize },
}

impl fmt::Display for MuxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Finished => f.write_str("the file is finished already"),
            Self::EmptySample => f.write_str("a sample without data"),
            Self::SampleTooLarge { bytes, limit } => {
                write!(f, "a sample of {bytes} bytes is over the limit of {limit}")
            }
        }
    }
}

impl std::error::Error for MuxError {}

/// When a fragment closes, and how big anything may get.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// A fragment closes at the next sync sample once it spans this long.
    pub min_fragment_us: u64,
    /// A fragment closes at the next sample, sync or not, once it spans this long.
    pub max_fragment_us: u64,
    /// ... or once it holds this many bytes.
    pub max_fragment_bytes: usize,
    /// ... or this many samples.
    pub max_fragment_samples: usize,
    /// A sample over this is refused.
    pub max_sample_bytes: usize,
}

impl Default for Limits {
    /// Fragments of one to two seconds that start at a keyframe; at most 4 s, 32 MB or 20 000
    /// samples; a sample may be 24 MB (a keyframe of 4K screen content is a few MB).
    fn default() -> Self {
        Self {
            min_fragment_us: 1_000_000,
            max_fragment_us: 4_000_000,
            max_fragment_bytes: 32 * 1024 * 1024,
            max_fragment_samples: 20_000,
            max_sample_bytes: 24 * 1024 * 1024,
        }
    }
}

/// What the writer has done so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MuxStats {
    /// Samples taken.
    pub samples: u64,
    /// Fragments written.
    pub fragments: u32,
    /// Bytes handed out so far (the file's length).
    pub bytes: u64,
    /// Samples whose time was moved up by a tick because it did not rise.
    pub bumped: u64,
}

struct Pending {
    ticks: u64,
    data: Vec<u8>,
    sync: bool,
}

/// The writer of one fragmented MP4 file. It hands out bytes; the caller appends them to the file
/// (and syncs it after each fragment).
pub struct FragmentWriter {
    track: TrackConfig,
    timescale: u32,
    limits: Limits,
    pending: Vec<Pending>,
    pending_bytes: usize,
    last_ticks: Option<u64>,
    last_duration: u64,
    sequence: u32,
    index: Vec<(u64, u64)>,
    stats: MuxStats,
    finished: bool,
}

impl FragmentWriter {
    /// A writer for `track`, and the first bytes of the file (`ftyp` and `moov`).
    /// `created_unix_s` is the wall-clock time the file's header says.
    #[must_use]
    pub fn new(track: TrackConfig, created_unix_s: u64, limits: Limits) -> (Self, Vec<u8>) {
        let mut head = ftyp();
        head.extend_from_slice(&moov(&track, created_unix_s));
        let timescale = track.timescale();
        let stats = MuxStats {
            bytes: head.len() as u64,
            ..MuxStats::default()
        };
        (
            Self {
                track,
                timescale,
                limits,
                pending: Vec::new(),
                pending_bytes: 0,
                last_ticks: None,
                last_duration: 1,
                sequence: 0,
                index: Vec::new(),
                stats,
                finished: false,
            },
            head,
        )
    }

    /// What the writer has done.
    #[must_use]
    pub const fn stats(&self) -> MuxStats {
        self.stats
    }

    /// The track the file holds.
    #[must_use]
    pub const fn track(&self) -> &TrackConfig {
        &self.track
    }

    /// Takes a sample whose time is `pts_us` microseconds from the start of the file's timeline,
    /// and returns the bytes of a fragment when this sample closed one (usually nothing).
    /// `data` is the sample in the file's own form: for video what [`annexb_to_avcc`] returns,
    /// for Opus the packet. A time that does not rise past the last one is moved up one tick.
    ///
    /// # Errors
    /// The sample is empty or too large, or the file is finished.
    pub fn push(&mut self, pts_us: u64, data: Vec<u8>, sync: bool) -> Result<Vec<u8>, MuxError> {
        self.push_ticks(us_to_ticks(pts_us, self.timescale), data, sync)
    }

    /// [`Self::push`] with the time in ticks of the track's timescale (the audio's sample count,
    /// which has no rounding to do).
    ///
    /// # Errors
    /// As [`Self::push`].
    pub fn push_ticks(&mut self, ticks: u64, data: Vec<u8>, sync: bool) -> Result<Vec<u8>, MuxError> {
        if self.finished {
            return Err(MuxError::Finished);
        }
        if data.is_empty() {
            return Err(MuxError::EmptySample);
        }
        if data.len() > self.limits.max_sample_bytes {
            return Err(MuxError::SampleTooLarge {
                bytes: data.len(),
                limit: self.limits.max_sample_bytes,
            });
        }
        let mut ticks = ticks;
        if let Some(last) = self.last_ticks {
            if ticks <= last {
                ticks = last + 1;
                self.stats.bumped += 1;
            }
        }
        let mut out = Vec::new();
        if self.closes_before(ticks, sync, data.len()) {
            out = self.close_fragment(Some(ticks), 0);
        }
        self.pending_bytes += data.len();
        self.pending.push(Pending { ticks, data, sync });
        self.last_ticks = Some(ticks);
        self.stats.samples += 1;
        Ok(out)
    }

    fn closes_before(&self, next_ticks: u64, next_sync: bool, next_bytes: usize) -> bool {
        let Some(first) = self.pending.first() else {
            return false;
        };
        let span_us = ticks_to_us(next_ticks.saturating_sub(first.ticks), self.timescale);
        (next_sync && span_us >= self.limits.min_fragment_us)
            || span_us >= self.limits.max_fragment_us
            || self.pending_bytes + next_bytes > self.limits.max_fragment_bytes
            || self.pending.len() >= self.limits.max_fragment_samples
    }

    /// Writes the open fragment: `next_ticks` is where the sample after it begins (it gives the
    /// last sample of this fragment its length); without one the last sample lasts `tail` ticks.
    fn close_fragment(&mut self, next_ticks: Option<u64>, tail: u64) -> Vec<u8> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let samples = std::mem::take(&mut self.pending);
        self.pending_bytes = 0;
        let n = samples.len();
        let mut durations: Vec<u64> = Vec::with_capacity(n);
        for i in 0..n {
            let end = match samples.get(i + 1) {
                Some(next) => next.ticks,
                None => next_ticks.unwrap_or_else(|| samples[i].ticks + tail.max(1)),
            };
            durations.push(end.saturating_sub(samples[i].ticks).max(1));
        }
        self.last_duration = durations.last().copied().unwrap_or(1);
        let base = samples[0].ticks;
        let video = matches!(self.track, TrackConfig::Video(_));
        let flags_of = |sync: bool| -> u32 {
            match (video, sync) {
                (_, true) => 0x0200_0000,
                (true, false) => 0x0101_0000,
                (false, false) => 0x0200_0000,
            }
        };

        self.sequence += 1;
        let payload_len: usize = samples.iter().map(|s| s.data.len()).sum();

        let mfhd = full_box(b"mfhd", 0, 0, &self.sequence.to_be_bytes());
        let mut tfhd = Vec::new();
        put_u32(&mut tfhd, 1);
        let tfhd = full_box(b"tfhd", 0, 0x0002_0000, &tfhd); // default-base-is-moof
        let mut tfdt = Vec::new();
        put_u64(&mut tfdt, base);
        let tfdt = full_box(b"tfdt", 1, 0, &tfdt);
        // trun: data offset, then a duration, a size and flags for every sample.
        let build_trun = |data_offset: u32| -> Vec<u8> {
            let mut t = Vec::with_capacity(8 + n * 12);
            put_u32(&mut t, u32::try_from(n).unwrap_or(u32::MAX));
            put_u32(&mut t, data_offset);
            for (s, d) in samples.iter().zip(&durations) {
                put_u32(&mut t, u32::try_from(*d).unwrap_or(u32::MAX));
                put_u32(&mut t, u32::try_from(s.data.len()).unwrap_or(u32::MAX));
                put_u32(&mut t, flags_of(s.sync));
            }
            full_box(b"trun", 0, 0x0000_0701, &t)
        };
        let moof_size = |trun: &[u8]| 8 + mfhd.len() + 8 + tfhd.len() + tfdt.len() + trun.len();
        let first_pass = build_trun(0);
        let data_offset = u32::try_from(moof_size(&first_pass) + 8).unwrap_or(u32::MAX);
        let trun = build_trun(data_offset);
        let mut traf_body = tfhd.clone();
        traf_body.extend_from_slice(&tfdt);
        traf_body.extend_from_slice(&trun);
        let mut moof_body = mfhd.clone();
        moof_body.extend_from_slice(&boxed(b"traf", &traf_body));
        let moof = boxed(b"moof", &moof_body);

        let mut out = Vec::with_capacity(moof.len() + payload_len + 16);
        let moof_offset = self.stats.bytes;
        out.extend_from_slice(&moof);
        let mut mdat = Vec::with_capacity(16);
        if let Ok(size) = u32::try_from(payload_len + 8) {
            put_u32(&mut mdat, size);
            mdat.extend_from_slice(b"mdat");
        } else {
            put_u32(&mut mdat, 1);
            mdat.extend_from_slice(b"mdat");
            put_u64(&mut mdat, payload_len as u64 + 16);
        }
        out.extend_from_slice(&mdat);
        for s in &samples {
            out.extend_from_slice(&s.data);
        }
        if samples[0].sync {
            self.index.push((base, moof_offset));
        }
        self.stats.fragments += 1;
        self.stats.bytes += out.len() as u64;
        out
    }

    /// Closes the file: the last fragment (its last sample lasts until `end_us`, or as long as
    /// the sample before it when `end_us` is not past it) and the index (`mfra`) that lets a
    /// player seek. Returns the bytes to append.
    pub fn finish(&mut self, end_us: u64) -> Vec<u8> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;
        let last = self.pending.last().map_or(0, |p| p.ticks);
        let end = us_to_ticks(end_us, self.timescale);
        let tail = if end > last {
            end - last
        } else if self.pending.len() >= 2 {
            last - self.pending[self.pending.len() - 2].ticks
        } else {
            self.last_duration
        };
        let mut out = self.close_fragment(None, tail);
        let fragment_len = out.len() as u64;
        if !self.index.is_empty() {
            let mut tfra = Vec::new();
            put_u32(&mut tfra, 1); // track_ID
            put_u32(&mut tfra, 0); // one byte each for the traf, trun and sample numbers
            put_u32(&mut tfra, u32::try_from(self.index.len()).unwrap_or(u32::MAX));
            for (time, offset) in &self.index {
                put_u64(&mut tfra, *time);
                put_u64(&mut tfra, *offset);
                tfra.extend_from_slice(&[1, 1, 1]);
            }
            let mut mfra = full_box(b"tfra", 1, 0, &tfra);
            let mfro_size = u32::try_from(mfra.len() + 8 + 16).unwrap_or(u32::MAX);
            mfra.extend_from_slice(&full_box(b"mfro", 0, 0, &mfro_size.to_be_bytes()));
            out.extend_from_slice(&boxed(b"mfra", &mfra));
        }
        // The fragment was counted when it was written; the index was not.
        self.stats.bytes += out.len() as u64 - fragment_len;
        out
    }
}

// ---------------------------------------------------------------------------
// The scanner
// ---------------------------------------------------------------------------

/// What kind of media a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
}

/// What a scan found in a fragmented MP4.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scan {
    /// The file's length.
    pub file_len: u64,
    /// Where the last whole fragment (or the head, before any fragment) ends: cut the file here
    /// to drop an unfinished tail.
    pub whole_end: u64,
    /// There are bytes after `whole_end`.
    pub truncated: bool,
    /// The file has its `moov`.
    pub has_moov: bool,
    /// Whole fragments.
    pub fragments: u32,
    /// Samples in the whole fragments.
    pub samples: u64,
    /// The track's ticks a second.
    pub timescale: u32,
    /// The time of the first sample, in ticks.
    pub first_ticks: u64,
    /// The end of the last sample of the last whole fragment, in ticks.
    pub end_ticks: u64,
    pub kind: Option<TrackKind>,
    /// The sample entry's name: `avc1` or `Opus`.
    pub codec: Option<[u8; 4]>,
    /// The size of the picture (video).
    pub width: u32,
    pub height: u32,
    /// An index (`mfra`) closes the file: it was finished, not cut off.
    pub finished: bool,
}

impl Scan {
    /// The length of the media from the first sample to the end of the last, in microseconds.
    #[must_use]
    pub fn duration_us(&self) -> u64 {
        ticks_to_us(self.end_ticks.saturating_sub(self.first_ticks), self.timescale)
    }

    /// The time of the first sample, in microseconds from the start of the file's timeline.
    #[must_use]
    pub fn first_us(&self) -> u64 {
        ticks_to_us(self.first_ticks, self.timescale)
    }

    /// Whether the file holds anything that plays.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.fragments == 0
    }
}

/// Why a file could not be scanned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    /// The file does not start with an MP4 `ftyp` box.
    NotMp4,
    /// Reading failed.
    Io(String),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotMp4 => f.write_str("not an MP4 file (no ftyp box at the start)"),
            Self::Io(e) => write!(f, "reading failed: {e}"),
        }
    }
}

impl std::error::Error for ScanError {}

/// The most a `moov` or a `moof` may be for a scan to read it.
const MAX_HEAD_BOX: u64 = 16 * 1024 * 1024;

fn be32(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn be64(d: &[u8], at: usize) -> Option<u64> {
    d.get(at..at + 8)
        .map(|b| u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
}

/// The children of a box body: `(name, body)` in order, stopping at the first that does not fit.
fn children(body: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= body.len() {
        let Some(size) = be32(body, at) else { break };
        let kind = [body[at + 4], body[at + 5], body[at + 6], body[at + 7]];
        let (header, size) = if size == 1 {
            match be64(body, at + 8) {
                Some(big) => (16, usize::try_from(big).unwrap_or(usize::MAX)),
                None => break,
            }
        } else if size == 0 {
            (8, body.len() - at)
        } else {
            (8, size as usize)
        };
        if size < header || at + size > body.len() {
            break;
        }
        out.push((kind, &body[at + header..at + size]));
        at += size;
    }
    out
}

fn child<'a>(body: &'a [u8], name: &[u8; 4]) -> Option<&'a [u8]> {
    children(body).into_iter().find(|(k, _)| k == name).map(|(_, b)| b)
}

/// What a `moov` says about its first track.
fn read_moov(moov: &[u8], scan: &mut Scan) {
    let Some(trak) = child(moov, b"trak") else { return };
    let Some(mdia) = child(trak, b"mdia") else { return };
    if let Some(mdhd) = child(mdia, b"mdhd") {
        scan.timescale = if mdhd.first() == Some(&1) {
            be32(mdhd, 20).unwrap_or(0)
        } else {
            be32(mdhd, 12).unwrap_or(0)
        };
    }
    if let Some(hdlr) = child(mdia, b"hdlr") {
        scan.kind = match hdlr.get(8..12) {
            Some(b"vide") => Some(TrackKind::Video),
            Some(b"soun") => Some(TrackKind::Audio),
            _ => None,
        };
    }
    let entry = child(mdia, b"minf")
        .and_then(|m| child(m, b"stbl"))
        .and_then(|s| child(s, b"stsd"))
        .and_then(|s| s.get(8..));
    if let Some(entry) = entry {
        if let Some((kind, body)) = children(entry).into_iter().next() {
            scan.codec = Some(kind);
            if scan.kind == Some(TrackKind::Video) && body.len() >= 28 {
                scan.width = u32::from(u16::from_be_bytes([body[24], body[25]]));
                scan.height = u32::from(u16::from_be_bytes([body[26], body[27]]));
            }
        }
    }
}

/// What one `moof` holds: the time of its first sample, its sample count and its length in ticks.
fn read_moof(moof: &[u8]) -> Option<(u64, u64, u64)> {
    let traf = child(moof, b"traf")?;
    let tfhd = child(traf, b"tfhd")?;
    let tfhd_flags = be32(tfhd, 0)? & 0x00ff_ffff;
    let mut at = 8;
    if tfhd_flags & 0x01 != 0 {
        at += 8;
    }
    if tfhd_flags & 0x02 != 0 {
        at += 4;
    }
    let default_duration = if tfhd_flags & 0x08 != 0 { be32(tfhd, at).map(u64::from) } else { None };
    let tfdt = child(traf, b"tfdt")?;
    let base = if tfdt.first() == Some(&1) { be64(tfdt, 4)? } else { u64::from(be32(tfdt, 4)?) };
    let trun = child(traf, b"trun")?;
    let flags = be32(trun, 0)? & 0x00ff_ffff;
    let count = u64::from(be32(trun, 4)?);
    let mut at = 8;
    if flags & 0x1 != 0 {
        at += 4;
    }
    if flags & 0x4 != 0 {
        at += 4;
    }
    let per_sample = [0x100, 0x200, 0x400, 0x800].iter().filter(|f| flags & **f != 0).count() * 4;
    let mut total = 0_u64;
    for i in 0..usize::try_from(count).ok()? {
        let p = at + i * per_sample;
        let duration = if flags & 0x100 != 0 {
            u64::from(be32(trun, p)?)
        } else {
            default_duration.unwrap_or(0)
        };
        total += duration;
    }
    Some((base, count, total))
}

/// Reads a fragmented MP4: what it holds, and where its last whole fragment ends. Only the boxes'
/// headers, the `moov` and the `moof`s are read, never the media, so a segment of hours is scanned
/// in milliseconds.
///
/// # Errors
/// The file is not an MP4, or reading failed.
pub fn scan<R: Read + Seek>(reader: &mut R) -> Result<Scan, ScanError> {
    let io = |e: std::io::Error| ScanError::Io(e.to_string());
    let len = reader.seek(SeekFrom::End(0)).map_err(io)?;
    let mut scan = Scan {
        file_len: len,
        ..Scan::default()
    };
    let mut at = 0_u64;
    let mut commit = 0_u64;
    let mut first_box = true;
    // The last `moof` read: where it starts, the time of its first sample, its sample count and
    // its length in ticks. It counts once its `mdat` is whole.
    let mut open_moof: Option<(u64, u64, u64, u64)> = None;
    let mut first_fragment = true;
    while at + 8 <= len {
        reader.seek(SeekFrom::Start(at)).map_err(io)?;
        let mut header = [0_u8; 16];
        let want = usize::try_from((len - at).min(16)).unwrap_or(16);
        reader.read_exact(&mut header[..want]).map_err(io)?;
        let size32 = u64::from(u32::from_be_bytes([header[0], header[1], header[2], header[3]]));
        let kind = [header[4], header[5], header[6], header[7]];
        if first_box && &kind != b"ftyp" {
            return Err(ScanError::NotMp4);
        }
        first_box = false;
        let (head_len, size) = match size32 {
            1 if want >= 16 => (
                16,
                u64::from_be_bytes([
                    header[8], header[9], header[10], header[11], header[12], header[13], header[14], header[15],
                ]),
            ),
            0 => (8, len - at),
            n => (8, n),
        };
        if size < head_len || at + size > len {
            break; // a box cut off by the end of the file
        }
        match &kind {
            b"moov" if size <= MAX_HEAD_BOX => {
                let mut body = vec![0_u8; usize::try_from(size - head_len).unwrap_or(0)];
                reader.seek(SeekFrom::Start(at + head_len)).map_err(io)?;
                reader.read_exact(&mut body).map_err(io)?;
                read_moov(&body, &mut scan);
                scan.has_moov = true;
                commit = at + size;
            }
            b"moof" if size <= MAX_HEAD_BOX => {
                let mut body = vec![0_u8; usize::try_from(size - head_len).unwrap_or(0)];
                reader.seek(SeekFrom::Start(at + head_len)).map_err(io)?;
                reader.read_exact(&mut body).map_err(io)?;
                open_moof = read_moof(&body).map(|(base, count, total)| (at, base, count, total));
            }
            b"mdat" => {
                // A fragment is whole once its media is there.
                if let Some((_, base, count, total)) = open_moof.take() {
                    if first_fragment {
                        scan.first_ticks = base;
                        first_fragment = false;
                    }
                    scan.end_ticks = base + total;
                    scan.samples += count;
                    scan.fragments += 1;
                    commit = at + size;
                }
            }
            b"mfra" => {
                scan.finished = true;
                commit = at + size;
            }
            _ => {
                if open_moof.is_none() {
                    commit = at + size;
                }
            }
        }
        at += size;
    }
    scan.whole_end = if scan.has_moov { commit } else { 0 };
    scan.truncated = len > scan.whole_end;
    if !scan.has_moov {
        scan.fragments = 0;
    }
    Ok(scan)
}

#[cfg(test)]
#[path = "recorder_mp4_test.rs"]
mod recorder_mp4_test;
