//! H.264 bitstream helpers of the MediaCodec backend: pure Rust over byte slices, no codec, no
//! device, so a Linux runner tests them (`h264_test.rs`).
//!
//! What the backend needs from the bitstream and a codec's API does not give it:
//! - the NAL units of an Annex-B chunk with their start-code positions (a decoder is fed one
//!   access unit per input buffer, with its start codes), and where a chunk holding several
//!   pictures splits;
//! - the picture size of an SPS (cropping applied) and what its VUI says about colour and
//!   reordering, so a decoder is configured for the stream it is given and converts with the
//!   matrix the stream means;
//! - the parameter sets (SPS, PPS) of an encoder's output, which an encoder that hands them out in
//!   a separate CODEC_CONFIG buffer must repeat in front of every key frame.

/// The NAL unit types the backend tells apart.
pub(super) const NAL_SLICE: u8 = 1;
pub(super) const NAL_IDR: u8 = 5;
pub(super) const NAL_SEI: u8 = 6;
pub(super) const NAL_SPS: u8 = 7;
pub(super) const NAL_PPS: u8 = 8;
pub(super) const NAL_AUD: u8 = 9;

/// The four-byte start code written in front of every NAL unit this module builds.
pub(super) const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// One NAL unit of an Annex-B buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NalSpan {
    /// Where its start code begins (leading zero bytes included).
    pub start_code: usize,
    /// Where the NAL unit itself begins: its header byte.
    pub payload: usize,
    /// One past its last byte (the zero bytes before the next start code are not part of it).
    pub end: usize,
}

impl NalSpan {
    /// The `nal_unit_type` (the header's low five bits).
    pub(super) fn nal_type(&self, data: &[u8]) -> u8 {
        data.get(self.payload).map_or(0, |b| b & 0x1f)
    }

    /// The NAL unit without its start code.
    pub(super) fn bytes<'a>(&self, data: &'a [u8]) -> &'a [u8] {
        data.get(self.payload..self.end).unwrap_or(&[])
    }

    /// The NAL unit with its start code.
    pub(super) fn with_start_code<'a>(&self, data: &'a [u8]) -> &'a [u8] {
        data.get(self.start_code..self.end).unwrap_or(&[])
    }
}

/// The NAL units of an Annex-B buffer (three- or four-byte start codes, any run of zero bytes in
/// front of one). Bytes before the first start code belong to no NAL unit. An empty NAL unit (two
/// start codes in a row) is skipped.
pub(super) fn nal_spans(data: &[u8]) -> Vec<NalSpan> {
    // (where the delimiter starts, where the payload starts)
    let mut starts: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            let mut delimiter = i;
            while delimiter > 0 && data[delimiter - 1] == 0 {
                delimiter -= 1;
            }
            starts.push((delimiter, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut spans = Vec::with_capacity(starts.len());
    for (k, &(start_code, payload)) in starts.iter().enumerate() {
        let end = starts.get(k + 1).map_or(data.len(), |next| next.0);
        if payload < end {
            spans.push(NalSpan {
                start_code,
                payload,
                end,
            });
        }
    }
    spans
}

/// Whether `nal_type` is a VCL type (a slice of a picture).
const fn is_vcl(nal_type: u8) -> bool {
    matches!(nal_type, 1..=5)
}

/// Whether the slice at `span` is the first of its picture: `first_mb_in_slice` is `ue(v)` and
/// reads 0 exactly when its first bit is 1.
fn first_slice_of_picture(data: &[u8], span: &NalSpan) -> bool {
    data.get(span.payload + 1).is_some_and(|b| b & 0x80 != 0)
}

/// Splits a chunk into access units, each with its start codes. A codec takes one access unit per
/// input buffer; a chunk that holds several (a demuxer's whole GOP, two frames an encoder's thread
/// put into one packet) must not reach it as one. A new unit begins at an access unit delimiter,
/// SPS, PPS or SEI that follows a slice, and at a slice that is the first of its picture.
pub(super) fn split_access_units(data: &[u8]) -> Vec<&[u8]> {
    let mut units: Vec<&[u8]> = Vec::new();
    let mut unit_start: Option<usize> = None;
    let mut has_slice = false;
    for span in nal_spans(data) {
        let nal_type = span.nal_type(data);
        let begins_unit = has_slice
            && (matches!(nal_type, NAL_SEI..=NAL_AUD)
                || (14..=18).contains(&nal_type)
                || (is_vcl(nal_type) && first_slice_of_picture(data, &span)));
        if begins_unit {
            if let Some(start) = unit_start {
                units.push(&data[start..span.start_code]);
            }
            unit_start = Some(span.start_code);
            has_slice = false;
        }
        if unit_start.is_none() {
            unit_start = Some(span.start_code);
        }
        if is_vcl(nal_type) {
            has_slice = true;
        }
    }
    if let Some(start) = unit_start {
        if start < data.len() {
            units.push(&data[start..]);
        }
    }
    units
}

/// Whether `data` holds a NAL unit of `nal_type`.
pub(super) fn contains_nal(data: &[u8], nal_type: u8) -> bool {
    nal_spans(data)
        .iter()
        .any(|span| span.nal_type(data) == nal_type)
}

/// Whether `data` holds an IDR slice: a key frame.
pub(super) fn contains_idr(data: &[u8]) -> bool {
    contains_nal(data, NAL_IDR)
}

/// Whether the first NAL unit of `data` is an SPS.
pub(super) fn starts_with_sps(data: &[u8]) -> bool {
    nal_spans(data)
        .first()
        .is_some_and(|span| span.nal_type(data) == NAL_SPS)
}

/// The SPS and PPS NAL units of `data` as one Annex-B buffer with four-byte start codes, in the
/// order they stand: what an encoder's CODEC_CONFIG buffer holds. Empty when `data` has neither.
pub(super) fn parameter_sets(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for span in nal_spans(data) {
        if matches!(span.nal_type(data), NAL_SPS | NAL_PPS) {
            out.extend_from_slice(&START_CODE);
            out.extend_from_slice(span.bytes(data));
        }
    }
    out
}

/// `config` (parameter sets) in front of `frame`, unless the frame begins with its own SPS.
pub(super) fn with_parameter_sets(config: &[u8], frame: &[u8]) -> Vec<u8> {
    if config.is_empty() || starts_with_sps(frame) {
        return frame.to_vec();
    }
    let mut out = Vec::with_capacity(config.len() + frame.len());
    out.extend_from_slice(config);
    out.extend_from_slice(frame);
    out
}

// ---------------------------------------------------------------------------
// SPS
// ---------------------------------------------------------------------------

/// The colour description of an SPS's VUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Colour {
    /// `video_full_range_flag`: Y, Cb and Cr use 0..255 and not the video range.
    pub full_range: bool,
    /// `matrix_coefficients`: 1 BT.709, 5 and 6 BT.601, 2 not specified (the VUI said nothing:
    /// `2` is also what this holds when only the range was given).
    pub matrix_coefficients: u8,
}

/// What a decoder and an encoder's caller need of an SPS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SpsInfo {
    pub profile_idc: u8,
    pub level_idc: u8,
    /// The picture width in pixels, cropping applied.
    pub width: u32,
    /// The picture height in pixels, cropping applied.
    pub height: u32,
    /// `max_num_reorder_frames` of the VUI's bitstream restriction, when it has one.
    pub max_num_reorder_frames: Option<u32>,
    /// The VUI's colour description, when it has one.
    pub colour: Option<Colour>,
}

/// An RBSP's bits, most significant first.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Bits { data, pos: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.pos / 8)?;
        let bit = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Some(u32::from(bit))
    }

    /// `n` bits (at most 32) as a number.
    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut value = 0u64;
        for _ in 0..n {
            value = (value << 1) | u64::from(self.bit()?);
        }
        u32::try_from(value).ok()
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        let end = self.pos.checked_add(n)?;
        if end > self.data.len() * 8 {
            return None;
        }
        self.pos = end;
        Some(())
    }

    /// An unsigned Exp-Golomb code, `ue(v)`.
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0u32;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 32 {
                return None;
            }
        }
        if zeros == 0 {
            return Some(0);
        }
        let rest = u64::from(self.bits(zeros)?);
        u32::try_from((1u64 << zeros) - 1 + rest).ok()
    }

    /// A signed Exp-Golomb code, `se(v)`.
    fn se(&mut self) -> Option<i32> {
        let k = i64::from(self.ue()?);
        let value = if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) };
        i32::try_from(value).ok()
    }

    fn flag(&mut self) -> Option<bool> {
        self.bit().map(|b| b == 1)
    }
}

/// The RBSP of a NAL unit: its payload after the header byte, with the emulation prevention bytes
/// (`00 00 03`) taken out.
fn rbsp(nal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal.len());
    let mut zeros = 0u32;
    for &byte in nal.iter().skip(1) {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
    out
}

/// A scaling list of an SPS (spec 7.3.2.1.1.1): only its length matters here.
fn skip_scaling_list(bits: &mut Bits<'_>, size: usize) -> Option<()> {
    let mut last = 8i32;
    let mut next = 8i32;
    for _ in 0..size {
        if next != 0 {
            let delta = bits.se()?;
            next = (last + delta + 256).rem_euclid(256);
        }
        if next != 0 {
            last = next;
        }
    }
    Some(())
}

/// `hrd_parameters()` of a VUI (spec E.1.2): skipped.
fn skip_hrd(bits: &mut Bits<'_>) -> Option<()> {
    let cpb_cnt = bits.ue()?.checked_add(1)?;
    if cpb_cnt > 32 {
        return None;
    }
    bits.skip(8)?; // bit_rate_scale, cpb_size_scale
    for _ in 0..cpb_cnt {
        bits.ue()?;
        bits.ue()?;
        bits.skip(1)?;
    }
    bits.skip(20)?; // the four length fields
    Some(())
}

/// The VUI of an SPS (spec E.1.1): what the backend reads of it.
fn parse_vui(bits: &mut Bits<'_>) -> Option<(Option<Colour>, Option<u32>)> {
    let mut colour = None;
    if bits.flag()? {
        // aspect_ratio_info_present_flag
        if bits.bits(8)? == 255 {
            bits.skip(32)?;
        }
    }
    if bits.flag()? {
        bits.skip(1)?; // overscan_appropriate_flag
    }
    if bits.flag()? {
        // video_signal_type_present_flag
        bits.skip(3)?; // video_format
        let full_range = bits.flag()?;
        let matrix = if bits.flag()? {
            bits.skip(16)?; // colour_primaries, transfer_characteristics
            u8::try_from(bits.bits(8)?).ok()?
        } else {
            2
        };
        colour = Some(Colour {
            full_range,
            matrix_coefficients: matrix,
        });
    }
    if bits.flag()? {
        bits.ue()?;
        bits.ue()?;
    }
    if bits.flag()? {
        bits.skip(65)?; // num_units_in_tick, time_scale, fixed_frame_rate_flag
    }
    let nal_hrd = bits.flag()?;
    if nal_hrd {
        skip_hrd(bits)?;
    }
    let vcl_hrd = bits.flag()?;
    if vcl_hrd {
        skip_hrd(bits)?;
    }
    if nal_hrd || vcl_hrd {
        bits.skip(1)?; // low_delay_hrd_flag
    }
    bits.skip(1)?; // pic_struct_present_flag
    let mut reorder = None;
    if bits.flag()? {
        // bitstream_restriction_flag
        bits.skip(1)?;
        for _ in 0..4 {
            bits.ue()?;
        }
        reorder = Some(bits.ue()?);
    }
    Some((colour, reorder))
}

/// The profiles whose SPS carries the chroma format and the scaling matrices (spec 7.3.2.1.1).
const fn has_chroma_info(profile_idc: u8) -> bool {
    matches!(
        profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    )
}

/// Parses an SPS NAL unit (with its header byte, without a start code). `None` for anything that
/// is not a well-formed SPS of a picture of at least 1x1 and at most 16384x16384 pixels (a
/// bitstream error must not become an allocation).
pub(super) fn parse_sps(nal: &[u8]) -> Option<SpsInfo> {
    if nal.first().map(|b| b & 0x1f) != Some(NAL_SPS) {
        return None;
    }
    let data = rbsp(nal);
    let mut bits = Bits::new(&data);
    let profile_idc = u8::try_from(bits.bits(8)?).ok()?;
    bits.skip(8)?; // constraint flags and reserved bits
    let level_idc = u8::try_from(bits.bits(8)?).ok()?;
    bits.ue()?; // seq_parameter_set_id
    let mut chroma_format_idc = 1u32;
    let mut separate_colour_planes = false;
    if has_chroma_info(profile_idc) {
        chroma_format_idc = bits.ue()?;
        if chroma_format_idc > 3 {
            return None;
        }
        if chroma_format_idc == 3 {
            separate_colour_planes = bits.flag()?;
        }
        bits.ue()?; // bit_depth_luma_minus8
        bits.ue()?; // bit_depth_chroma_minus8
        bits.skip(1)?; // qpprime_y_zero_transform_bypass_flag
        if bits.flag()? {
            let lists = if chroma_format_idc == 3 { 12 } else { 8 };
            for index in 0..lists {
                if bits.flag()? {
                    skip_scaling_list(&mut bits, if index < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    bits.ue()?; // log2_max_frame_num_minus4
    match bits.ue()? {
        0 => {
            bits.ue()?; // log2_max_pic_order_cnt_lsb_minus4
        }
        1 => {
            bits.skip(1)?;
            bits.se()?;
            bits.se()?;
            let cycle = bits.ue()?;
            if cycle > 255 {
                return None;
            }
            for _ in 0..cycle {
                bits.se()?;
            }
        }
        _ => {}
    }
    bits.ue()?; // max_num_ref_frames
    bits.skip(1)?; // gaps_in_frame_num_value_allowed_flag
    let width_in_mbs = bits.ue()?.checked_add(1)?;
    let height_in_map_units = bits.ue()?.checked_add(1)?;
    let frame_mbs_only = bits.flag()?;
    if !frame_mbs_only {
        bits.skip(1)?; // mb_adaptive_frame_field_flag
    }
    bits.skip(1)?; // direct_8x8_inference_flag
    let (mut crop_left, mut crop_right, mut crop_top, mut crop_bottom) = (0u32, 0u32, 0u32, 0u32);
    if bits.flag()? {
        crop_left = bits.ue()?;
        crop_right = bits.ue()?;
        crop_top = bits.ue()?;
        crop_bottom = bits.ue()?;
    }
    let chroma_array_type = if separate_colour_planes {
        0
    } else {
        chroma_format_idc
    };
    let frame_factor = if frame_mbs_only { 1 } else { 2 };
    let (unit_x, unit_y): (u32, u32) = match chroma_array_type {
        0 => (1, frame_factor),
        1 => (2, 2 * frame_factor),
        2 => (2, frame_factor),
        _ => (1, frame_factor),
    };
    let coded_width = width_in_mbs.checked_mul(16)?;
    let coded_height = height_in_map_units.checked_mul(16)?.checked_mul(frame_factor)?;
    let width = coded_width.checked_sub(unit_x.checked_mul(crop_left.checked_add(crop_right)?)?)?;
    let height =
        coded_height.checked_sub(unit_y.checked_mul(crop_top.checked_add(crop_bottom)?)?)?;
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return None;
    }
    let (mut colour, mut max_num_reorder_frames) = (None, None);
    // A truncated VUI keeps what was read before it: the size is what a decoder needs.
    if bits.flag().unwrap_or(false) {
        if let Some((vui_colour, vui_reorder)) = parse_vui(&mut bits) {
            colour = vui_colour;
            max_num_reorder_frames = vui_reorder;
        }
    }
    Some(SpsInfo {
        profile_idc,
        level_idc,
        width,
        height,
        max_num_reorder_frames,
        colour,
    })
}

/// The first SPS of `data` (an Annex-B buffer), parsed.
pub(super) fn first_sps(data: &[u8]) -> Option<SpsInfo> {
    nal_spans(data)
        .iter()
        .filter(|span| span.nal_type(data) == NAL_SPS)
        .find_map(|span| parse_sps(span.bytes(data)))
}

#[cfg(test)]
#[path = "h264_test.rs"]
mod h264_test;
