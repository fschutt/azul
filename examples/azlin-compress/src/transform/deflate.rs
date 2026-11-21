//! Transform 2: the deflate streams inside PDF, ZIP and Office files, PNG images and gzip
//! files are replaced by their plain text and the few bytes that rebuild the exact stream
//! (azlin-preflate), so the segment codecs compress the plain text instead of meeting already
//! deflated bytes. JPEG images, in the plain text (a deflated picture in a DOCX) or between the
//! streams (a PDF's DCTDecode images), go through Lepton as in transform 1.
//!
//! Where the streams are found:
//! - ZIP (and DOCX, XLSX, PPTX, ODF, EPUB, JAR): every local file header with method 8;
//! - PDF: every `stream` keyword followed by a zlib header (FlateDecode);
//! - PNG: the IDAT chunks, joined (one zlib stream cut into chunks with CRCs);
//! - gzip: every member's deflate data.
//!
//! A stream is kept only when its plain text (after Lepton for pictures in it) compresses with
//! zstd level 3 to at least 2 % less than the stream plus its reconstruction data: a stream
//! that zstd cannot beat stays as it is.

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    time::Instant,
};

use azlin_preflate::{preflate_whole_deflate_stream, PreflateConfig};
use zeroize::Zeroizing;

use super::{
    jpeg::{self, MIN_EMBEDDED_JPEG},
    records::{verified, write_records, Allowed, Piece, MAX_DEFLATE_PLAIN, MAX_IDAT_CHUNKS},
    work::{Job, Work},
    DeflateStats, JpegStats,
};
use crate::{detect::Kind, options::Options};

/// Streams shorter than this stay as they are (the record's own bytes eat the gain).
const MIN_STREAM: usize = 256;
/// The deepest hash chain the analysis follows (the encoder's search depth it can model).
const MAX_CHAIN: u32 = 4096;
/// What the analysis needs beyond the plain text: hash chains and tables.
const ANALYSIS_OVERHEAD: u64 = 16 << 20;
/// Expected plain text per compressed byte, for the time projection when a header gives no
/// size (text-heavy documents inflate about 4 to 1).
const EXPANSION_GUESS: u64 = 4;

/// Where a deflate stream starts and what is known of it.
struct Candidate {
    /// The offset of the raw deflate data.
    start: usize,
    /// The plain text's length when a header says it (ZIP), else a guess, for the time budget.
    plain_guess: u64,
}

/// The analysis of one stream: its length, its reconstruction data and its plain text.
struct Analysis {
    stream_len: usize,
    reconstruction: Vec<u8>,
    plain: Zeroizing<Vec<u8>>,
}

fn analyze(data: &[u8], plain_limit: u64) -> Option<Analysis> {
    let config = PreflateConfig {
        max_chain_length: MAX_CHAIN,
        plain_text_limit: usize::try_from(plain_limit).unwrap_or(usize::MAX),
        // The whole record stream is decoded and compared before it is kept; preflate's own
        // check would rebuild every stream a second time.
        verify_compression: false,
    };
    // Vendored code parsing an untrusted file: a panic is a stream we leave as it is.
    let result = catch_unwind(AssertUnwindSafe(|| preflate_whole_deflate_stream(data, &config)));
    let (chunk, plain) = match result {
        Ok(Ok(found)) => found,
        _ => return None,
    };
    if chunk.compressed_size < MIN_STREAM || chunk.compressed_size > data.len() {
        return None;
    }
    Some(Analysis {
        stream_len: chunk.compressed_size,
        reconstruction: chunk.corrections,
        plain: Zeroizing::new(plain.text().to_vec()),
    })
}

fn zstd3_len(bytes: &[u8]) -> usize {
    zstd::bulk::compress(bytes, 3).map_or(bytes.len(), |c| c.len())
}

/// The deflate piece of the stream at `data[0..]`, or `None` (not a stream, no time, no
/// memory, or not worth it); counts what happened in `work`.
fn deflate_piece<'a>(data: &[u8], plain_guess: u64, work: &mut Work<'_>) -> Option<Piece<'a>> {
    work.deflate_stats.found += 1;
    if !work.has_time(Job::Deflate, plain_guess) {
        work.deflate_stats.over_budget += 1;
        return None;
    }
    // The plain text twice (the analysis and its verifying rebuild) and the tables.
    let left = work
        .options
        .limits
        .max_memory
        .saturating_sub(work.held)
        .saturating_sub(ANALYSIS_OVERHEAD);
    let plain_limit = MAX_DEFLATE_PLAIN.min(left / 2);
    if plain_limit < 64 * 1024 {
        work.deflate_stats.over_memory += 1;
        return None;
    }
    let started = Instant::now();
    let analysis = analyze(data, plain_limit);
    let Some(analysis) = analysis else {
        work.did(Job::Deflate, plain_guess.min(1 << 20), started.elapsed().as_secs_f64());
        work.deflate_stats.refused += 1;
        return None;
    };
    work.did(Job::Deflate, analysis.plain.len() as u64, started.elapsed().as_secs_f64());
    // Pictures in the plain text go through Lepton; the rest is judged by zstd level 3.
    let (jpeg_before, held_before) = (work.jpeg.clone(), work.held);
    let nested = jpeg::pieces(&analysis.plain, MIN_EMBEDDED_JPEG, work);
    let estimate: usize = analysis.reconstruction.len()
        + 13
        + nested
            .iter()
            .map(|piece| match piece {
                Piece::Literal(bytes) => zstd3_len(bytes),
                Piece::Lepton { data, .. } => data.len(),
                _ => 0,
            })
            .sum::<usize>();
    if estimate * 100 > analysis.stream_len * 98 {
        work.deflate_stats.not_worth += 1;
        // The pictures inside are dropped with it.
        work.jpeg = jpeg_before;
        work.held = held_before;
        return None;
    }
    let plain_records = write_records(&nested).ok()?;
    let stream_len = u32::try_from(analysis.stream_len).ok()?;
    work.held += (plain_records.len() + analysis.reconstruction.len()) as u64;
    work.deflate_stats.recompressed += 1;
    work.deflate_stats.bytes_in += analysis.stream_len as u64;
    work.deflate_stats.plain_bytes += analysis.plain.len() as u64;
    Some(Piece::Deflate {
        stream_len,
        reconstruction: analysis.reconstruction,
        plain_records,
        plain_len: analysis.plain.len() as u64,
    })
}

fn le16(data: &[u8], at: usize) -> Option<usize> {
    data.get(at..at + 2).map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
}

fn le32(data: &[u8], at: usize) -> Option<u64> {
    data.get(at..at + 4)
        .map(|b| u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
}

fn is_zlib_header(data: &[u8]) -> bool {
    match data {
        [cmf, flg, ..] => {
            cmf & 0x0F == 8 && cmf >> 4 <= 7 && flg & 0x20 == 0 && (u16::from(*cmf) << 8 | u16::from(*flg)) % 31 == 0
        }
        _ => false,
    }
}

/// The deflate streams of ZIP local headers (method 8) and PDF FlateDecode streams, in order.
/// Each pattern is searched once over the input (one pass each, whatever the file holds).
fn container_candidates(input: &[u8]) -> Vec<Candidate> {
    let ends = all_positions(input, b"endstream");
    let mut found: Vec<Candidate> = all_positions(input, b"PK\x03\x04")
        .into_iter()
        .filter_map(|pos| zip_entry(input, pos))
        .chain(
            all_positions(input, b"stream")
                .into_iter()
                .filter_map(|pos| pdf_stream(input, pos, &ends)),
        )
        .collect();
    found.sort_by_key(|c| c.start);
    found
}

/// Every position of `pattern` in `data`.
fn all_positions(data: &[u8], pattern: &[u8]) -> Vec<usize> {
    data.windows(pattern.len())
        .enumerate()
        .filter(|(_, w)| *w == pattern)
        .map(|(i, _)| i)
        .collect()
}

/// The deflate data of the ZIP local file header at `pos`.
fn zip_entry(input: &[u8], pos: usize) -> Option<Candidate> {
    let flags = le16(input, pos + 6)?;
    let method = le16(input, pos + 8)?;
    if method != 8 || flags & 1 != 0 {
        return None; // not deflated, or encrypted
    }
    let plain_size = le32(input, pos + 22)?;
    let name_len = le16(input, pos + 26)?;
    let extra_len = le16(input, pos + 28)?;
    let start = pos + 30 + name_len + extra_len;
    (start < input.len()).then(|| Candidate {
        start,
        plain_guess: if plain_size > 0 {
            plain_size
        } else {
            (input.len() - start) as u64 * EXPANSION_GUESS
        },
    })
}

/// The deflate data of a PDF stream whose `stream` keyword is at `pos` (a FlateDecode stream
/// starts with a zlib header after the keyword's end of line). `ends` are the positions of
/// every `endstream` keyword, in order: the next one bounds the stream for the time budget.
fn pdf_stream(input: &[u8], pos: usize, ends: &[usize]) -> Option<Candidate> {
    if pos >= 3 && &input[pos - 3..pos] == b"end" {
        return None;
    }
    let after = pos + 6;
    let data = if input[after..].starts_with(b"\r\n") {
        after + 2
    } else if input[after..].starts_with(b"\n") || input[after..].starts_with(b"\r") {
        after + 1
    } else {
        return None;
    };
    if !is_zlib_header(input.get(data..)?) {
        return None;
    }
    let end = ends.get(ends.partition_point(|&e| e < data)).copied().unwrap_or(input.len());
    Some(Candidate {
        start: data + 2,
        plain_guess: (end - data) as u64 * EXPANSION_GUESS,
    })
}

/// The deflate data of the gzip member that starts `input` (its header parsed: the optional
/// extra field, name, comment and header CRC). Where it ends is only known once the deflate
/// data is parsed.
fn gzip_member(input: &[u8]) -> Option<Candidate> {
    if !input.starts_with(b"\x1F\x8B\x08") {
        return None;
    }
    let flags = *input.get(3)?;
    let mut at = 10;
    if flags & 0x04 != 0 {
        at += 2 + le16(input, at)?;
    }
    for bit in [0x08, 0x10] {
        if flags & bit != 0 {
            at += input.get(at..)?.iter().position(|&b| b == 0)? + 1;
        }
    }
    if flags & 0x02 != 0 {
        at += 2;
    }
    if at >= input.len() {
        return None;
    }
    // The size in the last trailer is the plain text's (of a one-member file), for the budget.
    let size = le32(input, input.len().saturating_sub(4)).unwrap_or(0);
    Some(Candidate {
        start: at,
        plain_guess: size.max(((input.len() - at) as u64) * EXPANSION_GUESS),
    })
}

/// The pieces of a container: deflate streams at the candidates, Lepton for the pictures
/// between them, literals for the rest.
fn container_pieces<'a>(input: &'a [u8], candidates: Vec<Candidate>, work: &mut Work<'_>) -> Vec<Piece<'a>> {
    let mut pieces = Vec::new();
    let mut at = 0;
    for candidate in candidates {
        if candidate.start < at || candidate.start >= input.len() {
            continue;
        }
        let Some(piece) = deflate_piece(&input[candidate.start..], candidate.plain_guess, work) else {
            continue;
        };
        if candidate.start > at {
            pieces.extend(jpeg::pieces(&input[at..candidate.start], MIN_EMBEDDED_JPEG, work));
        }
        at = candidate.start + piece.file_len() as usize;
        pieces.push(piece);
    }
    if at < input.len() {
        pieces.extend(jpeg::pieces(&input[at..], MIN_EMBEDDED_JPEG, work));
    }
    pieces
}

/// A gzip file's pieces: one deflate stream per member.
fn gzip_pieces<'a>(input: &'a [u8], work: &mut Work<'_>) -> Vec<Piece<'a>> {
    let mut pieces = Vec::new();
    let mut at = 0;
    loop {
        let Some(candidate) = gzip_member(&input[at..]) else { break };
        let start = at + candidate.start;
        let Some(piece) = deflate_piece(&input[start..], candidate.plain_guess, work) else { break };
        pieces.push(Piece::Literal(&input[at..start]));
        let end = start + piece.file_len() as usize;
        pieces.push(piece);
        // The trailer (CRC-32 and size) closes the member.
        at = (end + 8).min(input.len());
        pieces.push(Piece::Literal(&input[end..at]));
        if at >= input.len() {
            break;
        }
    }
    if at < input.len() {
        pieces.push(Piece::Literal(&input[at..]));
    }
    pieces.retain(|p| p.file_len() > 0);
    pieces
}

/// A PNG's pieces: the first run of IDAT chunks as one record, literals around it.
fn png_pieces<'a>(input: &'a [u8], work: &mut Work<'_>) -> Vec<Piece<'a>> {
    let all = vec![Piece::Literal(input)];
    let mut pos = 8;
    let mut first = None;
    let mut lens = Vec::new();
    let mut joined = Zeroizing::new(Vec::new());
    let mut end = pos;
    while pos + 12 <= input.len() {
        let len = u32::from_be_bytes([input[pos], input[pos + 1], input[pos + 2], input[pos + 3]]);
        let data_end = pos + 8 + len as usize;
        if data_end + 4 > input.len() {
            break;
        }
        let is_idat = &input[pos + 4..pos + 8] == b"IDAT";
        if is_idat {
            first.get_or_insert(pos);
            lens.push(len);
            joined.extend_from_slice(&input[pos + 8..data_end]);
            end = data_end + 4;
        } else if first.is_some() {
            break; // the IDAT run is over
        }
        if lens.len() >= MAX_IDAT_CHUNKS {
            return all;
        }
        pos = data_end + 4;
    }
    let Some(first) = first else { return all };
    if !is_zlib_header(&joined) || joined.len() as u64 > MAX_DEFLATE_PLAIN {
        return all;
    }
    let guess = joined.len() as u64 * EXPANSION_GUESS;
    let Some(deflate) = deflate_piece(&joined[2..], guess, work) else { return all };
    let (stream_len, plain_len) = match &deflate {
        Piece::Deflate { stream_len, plain_len, .. } => (u64::from(*stream_len), *plain_len),
        _ => (0, 0),
    };
    let deflate_end = 2 + deflate.file_len() as usize;
    let inner = vec![
        Piece::Literal(&joined[..2]),
        deflate,
        Piece::Literal(&joined[deflate_end..]),
    ];
    let Ok(joined_records) = write_records(&inner) else { return all };
    vec![
        Piece::Literal(&input[..first]),
        Piece::PngIdat {
            chunk_lens: lens,
            joined_records,
            file_len: (end - first) as u64,
            stream_len,
            plain_len,
        },
        Piece::Literal(&input[end..]),
    ]
}

/// Transform 2 of `input` (a file of `kind`): its records, verified, and what was done.
pub(crate) fn forward(
    input: &[u8],
    kind: Kind,
    options: &Options,
) -> Result<(Zeroizing<Vec<u8>>, JpegStats, DeflateStats), String> {
    let mut work = Work::new(input.len() as u64, options);
    let pieces = match kind {
        Kind::Png => png_pieces(input, &mut work),
        Kind::Gzip => gzip_pieces(input, &mut work),
        _ => container_pieces(input, container_candidates(input), &mut work),
    };
    let (mut jpeg, mut deflate) = (work.jpeg.clone(), work.deflate_stats.clone());
    let summary = |deflate: &DeflateStats, jpeg: &JpegStats| {
        format!(
            "nothing transformed ({} deflate streams: {} over the time budget, {} over the memory \
             cap, {} not analysable, {} not worth it; {} JPEG images, {} recompressed)",
            deflate.found,
            deflate.over_budget,
            deflate.over_memory,
            deflate.refused,
            deflate.not_worth,
            jpeg.found,
            jpeg.recompressed
        )
    };
    if !pieces.iter().any(Piece::is_transform) {
        return Err(summary(&deflate, &jpeg));
    }
    let threads = options.threads.max(1);
    let result = verified(input, pieces, Allowed::Containers, &options.limits, threads, |piece| {
        match piece {
            Piece::Lepton { span, data } => {
                jpeg.recompressed -= 1;
                jpeg.refused += 1;
                jpeg.bytes_in -= span.len() as u64;
                jpeg.bytes_out -= data.len() as u64;
            }
            Piece::Deflate { stream_len, plain_len, .. } => {
                deflate.recompressed -= 1;
                deflate.refused += 1;
                deflate.bytes_in -= u64::from(*stream_len);
                deflate.plain_bytes -= plain_len;
            }
            Piece::PngIdat { stream_len, plain_len, .. } => {
                deflate.recompressed -= 1;
                deflate.refused += 1;
                deflate.bytes_in -= stream_len;
                deflate.plain_bytes -= plain_len;
            }
            Piece::Literal(_) => {}
        }
    });
    match result {
        Ok((stream, _)) => Ok((stream, jpeg, deflate)),
        Err(_) => Err(summary(&deflate, &jpeg)),
    }
}
