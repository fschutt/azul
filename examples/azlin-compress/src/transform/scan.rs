//! Finds JPEG images in a file by walking their marker structure: the file itself when it is a
//! JPEG (and the second picture phones append after it: a gain map, a depth map), and the
//! pictures that PDF (DCTDecode streams), ZIP and Office files (stored members) and camera RAW
//! files (previews) keep as plain JPEG bytes.
//!
//! The walk only has to find where an image ends; whether Lepton can take it is Lepton's call,
//! and every result is decoded and compared before it is kept. So a wrong guess here costs time,
//! never data. The walk is bounded: it never reads past the input, and the whole search stops
//! after a fixed amount of work, so a file full of fake markers cannot make it quadratic.

/// One JPEG image inside the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JpegSpan {
    /// The offset of its SOI marker.
    pub start: usize,
    /// The offset just past its EOI marker.
    pub end: usize,
    pub width: u16,
    pub height: u16,
    /// The bytes its DCT coefficients take in memory (MCU-padded, 2 bytes each): Lepton's
    /// working set grows with this, so the memory cap is checked against it.
    pub coefficient_bytes: u64,
}

impl JpegSpan {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// The search gives up after this many bytes walked per input byte (plus a fixed allowance).
const WORK_PER_BYTE: u64 = 8;
const WORK_FIXED: u64 = 1 << 20;

/// The JPEG images in `data` of at least `min_len` bytes, in order, not overlapping.
pub(crate) fn find_jpegs(data: &[u8], min_len: usize) -> Vec<JpegSpan> {
    let cap = (data.len() as u64)
        .saturating_mul(WORK_PER_BYTE)
        .saturating_add(WORK_FIXED);
    let mut work = 0u64;
    let mut found = Vec::new();
    let mut pos = 0;
    while let Some(start) = find_soi(data, pos) {
        match jpeg_at(data, start, &mut work, cap) {
            Some(span) if span.len() >= min_len => {
                pos = span.end;
                found.push(span);
            }
            _ => pos = start + 1,
        }
        if work > cap {
            break;
        }
    }
    found
}

/// The next `FF D8 FF` (an SOI marker and the start of the next marker) at or after `from`.
fn find_soi(data: &[u8], from: usize) -> Option<usize> {
    let rest = data.get(from..)?;
    rest.windows(3)
        .position(|w| w == [0xFF, 0xD8, 0xFF])
        .map(|i| from + i)
}

fn be16(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at + 2)?;
    Some(usize::from(u16::from_be_bytes([bytes[0], bytes[1]])))
}

/// The JPEG image whose SOI marker is at `start`, if its markers form a whole image Lepton may
/// take: one 8-bit Huffman frame (baseline, extended or progressive), at least one scan, an EOI.
/// `work` counts the bytes walked; past `cap` the walk gives up.
pub(crate) fn jpeg_at(data: &[u8], start: usize, work: &mut u64, cap: u64) -> Option<JpegSpan> {
    if data.get(start..start + 2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut pos = start + 2;
    let mut frame: Option<(u16, u16, u64)> = None;
    let mut scans = 0u32;
    loop {
        if *work > cap {
            return None;
        }
        if *data.get(pos)? != 0xFF {
            return None;
        }
        // Fill bytes: any number of 0xFF before the marker code.
        while *data.get(pos + 1)? == 0xFF {
            pos += 1;
        }
        let marker = *data.get(pos + 1)?;
        pos += 2;
        *work += 2;
        match marker {
            0xD9 => {
                let (width, height, coefficient_bytes) = frame?;
                if scans == 0 {
                    return None;
                }
                return Some(JpegSpan {
                    start,
                    end: pos,
                    width,
                    height,
                    coefficient_bytes,
                });
            }
            0xD0..=0xD7 | 0x01 => continue,
            0xD8 | 0x00 => return None,
            _ => {}
        }
        let len = be16(data, pos)?;
        if len < 2 {
            return None;
        }
        let segment = data.get(pos + 2..pos + len)?;
        match marker {
            0xC0..=0xC2 => {
                if frame.is_some() {
                    // A second frame: a hierarchical image, which Lepton does not take.
                    return None;
                }
                frame = Some(parse_frame(segment)?);
            }
            // Lossless, hierarchical and arithmetic-coded frames.
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return None,
            _ => {}
        }
        pos += len;
        *work += len as u64;
        if marker == 0xDA {
            frame?;
            scans += 1;
            // The entropy-coded data runs to the next marker that is neither a stuffed zero
            // nor a restart marker.
            loop {
                let rest = data.get(pos..)?;
                let skip = rest.iter().position(|&b| b == 0xFF)?;
                pos += skip;
                *work += skip as u64 + 1;
                if *work > cap {
                    return None;
                }
                match *data.get(pos + 1)? {
                    0x00 | 0xD0..=0xD7 => pos += 2,
                    0xFF => pos += 1,
                    _ => break,
                }
            }
        }
    }
}

/// Width, height and the coefficients' bytes of a SOF0/1/2 segment.
fn parse_frame(segment: &[u8]) -> Option<(u16, u16, u64)> {
    let precision = *segment.first()?;
    let height = u16::try_from(be16(segment, 1)?).ok()?;
    let width = u16::try_from(be16(segment, 3)?).ok()?;
    let components = usize::from(*segment.get(5)?);
    // 8-bit samples only; a height of 0 (set later by a DNL marker) is not taken either.
    if precision != 8 || height == 0 || width == 0 || !(1..=4).contains(&components) {
        return None;
    }
    let mut factors = Vec::with_capacity(components);
    for c in 0..components {
        let sampling = *segment.get(6 + c * 3 + 1)?;
        let (h, v) = (u64::from(sampling >> 4), u64::from(sampling & 0x0F));
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
            return None;
        }
        factors.push((h, v));
    }
    let h_max = factors.iter().map(|f| f.0).max()?;
    let v_max = factors.iter().map(|f| f.1).max()?;
    let mcus_x = u64::from(width).div_ceil(8 * h_max);
    let mcus_y = u64::from(height).div_ceil(8 * v_max);
    let blocks: u64 = factors.iter().map(|(h, v)| mcus_x * h * mcus_y * v).sum();
    Some((width, height, blocks * 64 * 2))
}
