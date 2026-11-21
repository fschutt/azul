//! The JPEG images in a run of bytes as records: a Lepton record for every image Lepton
//! shrinks by at least 5 %, literals around them. Transform 1 runs it over a whole JPEG or RAW
//! file; transform 2 over a container's literal parts and over the plain text of its deflate
//! streams (a picture inside a DOCX is a deflated JPEG).

use std::time::Instant;

use zeroize::Zeroizing;

use super::{
    lepton,
    records::{verified, Allowed, Piece},
    scan::find_jpegs,
    work::{Job, Work},
    JpegStats,
};
use crate::{
    detect::Kind,
    options::{Options, MAX_JPEG_PIXELS},
    segment::worth_it,
};

/// The smallest JPEG file the transform tries, and the smallest picture it looks for inside
/// another file (thumbnails and icons save a few hundred bytes, not worth a Lepton run).
pub(crate) const MIN_JPEG_FILE: usize = 2 * 1024;
pub(crate) const MIN_EMBEDDED_JPEG: usize = 16 * 1024;
/// What Lepton needs beyond the coefficients: its tables, per-thread state, buffers.
const LEPTON_OVERHEAD: u64 = 24 << 20;

/// The records of `data`: its JPEG images of at least `min_len` bytes through Lepton (where
/// time, memory and Lepton allow), the rest literal. Not yet verified.
pub(crate) fn pieces<'a>(data: &'a [u8], min_len: usize, work: &mut Work<'_>) -> Vec<Piece<'a>> {
    let spans = find_jpegs(data, min_len);
    let mut pieces = Vec::with_capacity(spans.len() * 2 + 1);
    let mut at = 0;
    for span in spans {
        work.jpeg.found += 1;
        if span.start > at {
            pieces.push(Piece::Literal(&data[at..span.start]));
        }
        at = span.end;
        let jpeg = &data[span.start..span.end];
        if !work.has_time(Job::Lepton, span.len() as u64) {
            work.jpeg.over_budget += 1;
            pieces.push(Piece::Literal(jpeg));
            continue;
        }
        // Its coefficients, its Lepton output and its verifying decode.
        let need = span.coefficient_bytes + 2 * span.len() as u64 + LEPTON_OVERHEAD;
        let max_pixels = work.options.limits.max_jpeg_pixels.min(MAX_JPEG_PIXELS);
        if span.pixels() > max_pixels || !work.has_memory(need) {
            work.jpeg.over_memory += 1;
            pieces.push(Piece::Literal(jpeg));
            continue;
        }
        let started = Instant::now();
        let encoded = lepton::encode(jpeg, &span, work.options.threads.max(1));
        work.did(Job::Lepton, span.len() as u64, started.elapsed().as_secs_f64());
        match encoded {
            Ok(lepton) if worth_it(lepton.len(), jpeg.len()) => {
                work.held += lepton.len() as u64;
                work.jpeg.recompressed += 1;
                work.jpeg.bytes_in += span.len() as u64;
                work.jpeg.bytes_out += lepton.len() as u64;
                pieces.push(Piece::Lepton { span, data: lepton });
            }
            _ => {
                work.jpeg.refused += 1;
                pieces.push(Piece::Literal(jpeg));
            }
        }
    }
    if at < data.len() {
        pieces.push(Piece::Literal(&data[at..]));
    }
    pieces
}

/// Transform 1 of `input` (a file of `kind`): its records, verified, and what was done.
pub(crate) fn forward(
    input: &[u8],
    kind: Kind,
    options: &Options,
) -> Result<(Zeroizing<Vec<u8>>, JpegStats), String> {
    let mut work = Work::new(input.len() as u64, options);
    let min_len = if kind == Kind::Jpeg {
        MIN_JPEG_FILE
    } else {
        MIN_EMBEDDED_JPEG
    };
    let pieces = pieces(input, min_len, &mut work);
    if work.jpeg.found == 0 {
        return Err(String::from("no JPEG image found"));
    }
    let mut stats = work.jpeg.clone();
    let summary = |stats: &JpegStats| {
        format!(
            "no JPEG image recompressed ({} found: {} over the time budget, {} over the memory \
             cap, {} refused by Lepton)",
            stats.found, stats.over_budget, stats.over_memory, stats.refused
        )
    };
    if !pieces.iter().any(Piece::is_transform) {
        return Err(summary(&stats));
    }
    let threads = options.threads.max(1);
    let result = verified(input, pieces, Allowed::Jpeg, &options.limits, threads, |piece| {
        if let Piece::Lepton { span, data } = piece {
            stats.recompressed -= 1;
            stats.refused += 1;
            stats.bytes_in -= span.len() as u64;
            stats.bytes_out -= data.len() as u64;
        }
    });
    match result {
        Ok((stream, _)) => Ok((stream, stats)),
        Err(_) => Err(summary(&stats)),
    }
}
