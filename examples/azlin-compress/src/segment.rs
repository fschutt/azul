//! Segments: a file (or a transform's output) is cut into segments of a fixed plaintext size,
//! and each one is stored or compressed on its own. The first byte of a stored segment is its
//! codec byte, the same table AZL1 uses:
//!
//! | byte | codec |
//! |---|---|
//! | 0 | stored as it is |
//! | 1 | zstd, one frame, any level |
//! | 2 | brotli, one stream, any quality |
//! | 3 | reserved for JPEG XL; never written, refused by name |
//! | 4 | zstd with a shipped dictionary: the dictionary id (u16 LE), then one frame |
//!
//! How the codec is chosen, per file:
//! - a dense kind (video, HEIC, archives, ...) is stored without a trial;
//! - otherwise the first segment is compressed with zstd level 1 as a trial (early abort, as
//!   OpenZFS does): when that saves under 5 %, only every [`RETRY_EVERY`]th segment after it is
//!   tried again, and once one of those compresses, every segment is;
//! - when the trial compresses, the time it took says how slow a level the budget allows: the
//!   highest zstd level (and for text, brotli quality) whose projected time fits. Text tries both
//!   on the first segment, small text files also zstd with their kind's dictionary, and the
//!   smallest wins for the whole file;
//! - a segment is kept compressed only when that saves at least 5 % ([`worth_it`]);
//! - when the segments fall behind the budget, the level steps down for the rest.

use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::{
    detect::Kind,
    dict::{self, DictId},
    error::{damaged, Error},
    options::Options,
};

/// A segment stays compressed only when that saves at least this many percent.
pub const MIN_SAVING_PERCENT: u64 = 5;
/// After a trial that did not compress, every this many segments one is tried again.
pub const RETRY_EVERY: u32 = 8;
/// The zstd level of the first-segment trial (fast: incompressible data runs at GB/s).
pub const TRIAL_ZSTD_LEVEL: i32 = 1;
/// Files up to this size try their kind's dictionary (it helps most below a few KiB and
/// stops mattering past about a hundred).
pub const DICTIONARY_MAX_LEN: u64 = 128 * 1024;

/// The codec byte of a JPEG XL segment: kept free, never written or read.
pub const CODEC_JPEG_XL: u8 = 3;

/// How a segment's bytes are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Stored,
    Zstd,
    Brotli,
    /// zstd with a shipped dictionary.
    ZstdDict,
}

impl Codec {
    /// The codec byte.
    #[must_use]
    pub fn byte(self) -> u8 {
        match self {
            Codec::Stored => 0,
            Codec::Zstd => 1,
            Codec::Brotli => 2,
            Codec::ZstdDict => 4,
        }
    }

    /// The codec of a codec byte.
    pub fn from_byte(byte: u8) -> Result<Codec, Error> {
        match byte {
            0 => Ok(Codec::Stored),
            1 => Ok(Codec::Zstd),
            2 => Ok(Codec::Brotli),
            4 => Ok(Codec::ZstdDict),
            CODEC_JPEG_XL => Err(Error::Unsupported(String::from(
                "a JPEG XL segment (codec 3)",
            ))),
            other => Err(Error::Unsupported(format!("segment codec {other}"))),
        }
    }
}

/// Whether `compressed` bytes are worth keeping instead of `raw` ones: at least 5 % fewer.
#[must_use]
pub fn worth_it(compressed: usize, raw: usize) -> bool {
    raw > 0 && (compressed as u64) * 100 <= (raw as u64) * (100 - MIN_SAVING_PERCENT)
}

/// One encoded segment.
#[derive(Debug)]
pub enum Segment {
    /// Stored as it is: the caller writes the raw bytes after codec byte 0.
    Stored,
    /// The codec and its bytes (for [`Codec::ZstdDict`] the dictionary id first).
    Compressed {
        codec: Codec,
        data: Zeroizing<Vec<u8>>,
    },
}

impl Segment {
    #[must_use]
    pub fn codec(&self) -> Codec {
        match self {
            Segment::Stored => Codec::Stored,
            Segment::Compressed { codec, .. } => *codec,
        }
    }
}

/// zstd levels with their time relative to level 1 (in tenths), from the zstd 1.5 benchmarks
/// and our Silesia run. Only the projection uses them: the encoder steps down when the real
/// segments are slower.
const ZSTD_LADDER: &[(i32, u64)] = &[
    (1, 10),
    (3, 17),
    (6, 40),
    (9, 70),
    (12, 170),
    (15, 400),
    (17, 700),
    (19, 1500),
];
/// brotli qualities with their time relative to zstd level 1 (in tenths; the pure-Rust
/// encoder, a little slower than the C one).
const BROTLI_LADDER: &[(u32, u64)] = &[(1, 15), (5, 200), (7, 300), (9, 450), (11, 14_000)];

/// What a file's segments are compressed with, once the first segment chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Zstd(i32),
    Brotli(u32),
    ZstdDict(DictId, i32),
}

/// What the codec stage did, for the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodecStats {
    /// The choice of the first segment, after its trial (`None`: nothing compressed).
    pub first_choice: Option<Choice>,
    /// The choice for the last segment (lower than the first when the budget ran short).
    pub last_choice: Option<Choice>,
    pub stored: u64,
    pub compressed: u64,
    /// Compressed segments that did not decode back to their bytes and were stored instead
    /// (a codec bug; expected to stay 0).
    pub verify_failed: u64,
}

enum State {
    /// The first segment decides.
    Undecided,
    /// No segment is tried (a dense kind).
    Off,
    /// Every segment is compressed with the choice.
    On(Choice),
    /// The last try did not compress: the next one after [`RETRY_EVERY`] segments.
    Sparse { skipped: u32 },
}

/// One file's codec stage: give it the segments in order.
pub struct SegmentEncoder {
    kind: Kind,
    state: State,
    max_zstd_level: i32,
    max_brotli_quality: u32,
    dictionaries: bool,
    /// The whole input's length (the transform's output), when known: the budget and the
    /// dictionary rule need it. A streaming caller passes what it knows.
    total_len: Option<u64>,
    /// The time the whole codec stage may take.
    budget: Duration,
    started: Instant,
    /// Bytes encoded so far (all segments).
    done: u64,
    /// One zstd context per level in use (made on first use; the level changes rarely).
    zstd: Option<(i32, zstd::bulk::Compressor<'static>)>,
    stats: CodecStats,
}

impl SegmentEncoder {
    /// The encoder of a file of `kind`, `total_len` bytes long (when known), under `options`.
    #[must_use]
    pub fn new(kind: Kind, total_len: Option<u64>, options: &Options) -> SegmentEncoder {
        let budget = options
            .limits
            .codec_time
            .for_len(total_len.unwrap_or(u64::from(options.segment_size)));
        SegmentEncoder {
            kind,
            state: if kind.is_dense() { State::Off } else { State::Undecided },
            max_zstd_level: options.max_zstd_level.clamp(1, 22),
            max_brotli_quality: options.max_brotli_quality.min(11),
            dictionaries: options.dictionaries,
            total_len,
            budget,
            started: Instant::now(),
            done: 0,
            zstd: None,
            stats: CodecStats::default(),
        }
    }

    /// What the stage did so far.
    #[must_use]
    pub fn stats(&self) -> CodecStats {
        self.stats
    }

    /// How the next segment, `raw`, is stored. A compressed segment is decoded and compared
    /// with `raw` before it is returned; one that does not give `raw` back is stored instead.
    pub fn encode(&mut self, raw: &[u8]) -> Result<Segment, Error> {
        let segment = match self.encode_inner(raw)? {
            Segment::Compressed { codec, data } => {
                match decode_segment(codec.byte(), &data, raw.len()) {
                    Ok(plain) if plain[..] == raw[..] => Segment::Compressed { codec, data },
                    _ => {
                        self.stats.verify_failed += 1;
                        Segment::Stored
                    }
                }
            }
            Segment::Stored => Segment::Stored,
        };
        self.done += raw.len() as u64;
        match segment.codec() {
            Codec::Stored => self.stats.stored += 1,
            _ => self.stats.compressed += 1,
        }
        Ok(segment)
    }

    fn encode_inner(&mut self, raw: &[u8]) -> Result<Segment, Error> {
        if raw.is_empty() {
            return Ok(Segment::Stored);
        }
        match self.state {
            State::Off => Ok(Segment::Stored),
            State::Undecided => self.first(raw),
            State::Sparse { skipped } => {
                if skipped + 1 < RETRY_EVERY {
                    self.state = State::Sparse {
                        skipped: skipped + 1,
                    };
                    return Ok(Segment::Stored);
                }
                // This one is tried like a first segment: it compresses (every segment is
                // tried again, at the level its timing allows) or it waits again.
                self.first(raw)
            }
            State::On(choice) => {
                let segment = self.compress(choice, raw)?;
                self.keep_pace(choice);
                Ok(segment)
            }
        }
    }

    /// The trial: zstd level 1 says whether the data compresses at all and how fast; then the
    /// best choice the budget allows is made and used for this segment.
    fn first(&mut self, raw: &[u8]) -> Result<Segment, Error> {
        let trial_started = Instant::now();
        let trial = Zeroizing::new(self.zstd_compress(TRIAL_ZSTD_LEVEL, raw)?);
        let trial_time = trial_started.elapsed();
        if !worth_it(trial.len(), raw.len()) {
            self.state = State::Sparse { skipped: 0 };
            return Ok(Segment::Stored);
        }
        // The time the rest of the input would take at level 1, from this segment's pace.
        let total = self.total_len.unwrap_or(self.done + raw.len() as u64).max(1);
        let rest = total.saturating_sub(self.done).max(raw.len() as u64);
        let level1_time = trial_time.as_secs_f64() * rest as f64 / raw.len() as f64;
        let left = self.budget.saturating_sub(self.started.elapsed()).as_secs_f64();
        let zstd_level = ZSTD_LADDER
            .iter()
            .filter(|(level, cost)| {
                *level <= self.max_zstd_level && level1_time * (*cost as f64) / 10.0 <= left
            })
            .map(|(level, _)| *level)
            .next_back()
            .unwrap_or(TRIAL_ZSTD_LEVEL);
        let mut best = if zstd_level == TRIAL_ZSTD_LEVEL {
            (Choice::Zstd(TRIAL_ZSTD_LEVEL), trial)
        } else {
            let choice = Choice::Zstd(zstd_level);
            (choice, self.compress_with(choice, raw)?)
        };
        if self.kind.is_text() {
            let quality = BROTLI_LADDER
                .iter()
                .filter(|(quality, cost)| {
                    *quality <= self.max_brotli_quality
                        && level1_time * (*cost as f64) / 10.0 <= left
                })
                .map(|(quality, _)| *quality)
                .next_back();
            if let Some(quality) = quality {
                let choice = Choice::Brotli(quality);
                let data = self.compress_with(choice, raw)?;
                if data.len() < best.1.len() {
                    best = (choice, data);
                }
            }
            let small = total <= DICTIONARY_MAX_LEN;
            if let (true, true, Some(id)) = (self.dictionaries, small, dict::for_kind(self.kind)) {
                let choice = Choice::ZstdDict(id, zstd_level);
                let data = self.compress_with(choice, raw)?;
                if data.len() < best.1.len() {
                    best = (choice, data);
                }
            }
        }
        let (choice, data) = best;
        self.state = State::On(choice);
        if self.stats.first_choice.is_none() {
            self.stats.first_choice = Some(choice);
        }
        self.stats.last_choice = Some(choice);
        Ok(if worth_it(data.len(), raw.len()) {
            Segment::Compressed {
                codec: codec_of(choice),
                data,
            }
        } else {
            Segment::Stored
        })
    }

    /// `raw` with `choice`, kept when it saves 5 %.
    fn compress(&mut self, choice: Choice, raw: &[u8]) -> Result<Segment, Error> {
        let data = self.compress_with(choice, raw)?;
        Ok(if worth_it(data.len(), raw.len()) {
            Segment::Compressed {
                codec: codec_of(choice),
                data,
            }
        } else {
            Segment::Stored
        })
    }

    fn compress_with(&mut self, choice: Choice, raw: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
        match choice {
            Choice::Zstd(level) => Ok(Zeroizing::new(self.zstd_compress(level, raw)?)),
            Choice::Brotli(quality) => brotli_compress(raw, quality),
            Choice::ZstdDict(id, level) => {
                let dictionary = dict::by_id(id)
                    .ok_or_else(|| Error::Codec(format!("no dictionary {id}")))?;
                let mut compressor = zstd::bulk::Compressor::with_dictionary(level, dictionary.bytes)
                    .map_err(|e| Error::Codec(format!("zstd: {e}")))?;
                let frame = Zeroizing::new(
                    compressor
                        .compress(raw)
                        .map_err(|e| Error::Codec(format!("zstd: {e}")))?,
                );
                let mut data = Zeroizing::new(Vec::with_capacity(frame.len() + 2));
                data.extend_from_slice(&id.to_le_bytes());
                data.extend_from_slice(&frame);
                Ok(data)
            }
        }
    }

    fn zstd_compress(&mut self, level: i32, raw: &[u8]) -> Result<Vec<u8>, Error> {
        let reuse = matches!(&self.zstd, Some((current, _)) if *current == level);
        if !reuse {
            let compressor = zstd::bulk::Compressor::new(level)
                .map_err(|e| Error::Codec(format!("zstd: {e}")))?;
            self.zstd = Some((level, compressor));
        }
        let Some((_, compressor)) = self.zstd.as_mut() else {
            return Err(Error::Codec(String::from("zstd: no context")));
        };
        compressor
            .compress(raw)
            .map_err(|e| Error::Codec(format!("zstd: {e}")))
    }

    /// Steps the level down one rung when the segments so far took more than their share of
    /// the budget (with a quarter of slack: one slow segment is noise).
    fn keep_pace(&mut self, choice: Choice) {
        let Some(total) = self.total_len else {
            return;
        };
        if total == 0 {
            return;
        }
        let share = self.budget.as_secs_f64() * (self.done as f64 / total as f64);
        if self.started.elapsed().as_secs_f64() <= share * 1.25 {
            return;
        }
        let lower = match choice {
            Choice::Zstd(level) => step_down_zstd(level).map(Choice::Zstd),
            Choice::ZstdDict(id, level) => step_down_zstd(level).map(|l| Choice::ZstdDict(id, l)),
            Choice::Brotli(quality) => BROTLI_LADDER
                .iter()
                .map(|(q, _)| *q)
                .filter(|q| *q < quality)
                .next_back()
                .map(Choice::Brotli)
                .or(Some(Choice::Zstd(TRIAL_ZSTD_LEVEL))),
        };
        if let Some(lower) = lower {
            self.state = State::On(lower);
            self.stats.last_choice = Some(lower);
        }
    }
}

fn step_down_zstd(level: i32) -> Option<i32> {
    ZSTD_LADDER
        .iter()
        .map(|(l, _)| *l)
        .filter(|l| *l < level)
        .next_back()
}

fn codec_of(choice: Choice) -> Codec {
    match choice {
        Choice::Zstd(_) => Codec::Zstd,
        Choice::Brotli(_) => Codec::Brotli,
        Choice::ZstdDict(..) => Codec::ZstdDict,
    }
}

/// The smallest brotli window (log2 of its bytes) that holds `len` bytes, 16 to 24: a reader
/// never needs more memory than the segment.
#[must_use]
pub fn brotli_window_bits(len: usize) -> u32 {
    let mut bits = 16;
    while bits < 24 && (1usize << bits) < len {
        bits += 1;
    }
    bits
}

fn brotli_compress(raw: &[u8], quality: u32) -> Result<Zeroizing<Vec<u8>>, Error> {
    use std::io::Write;
    let mut out = Zeroizing::new(Vec::with_capacity(raw.len() / 2));
    let mut writer =
        brotli::CompressorWriter::new(&mut *out, 64 * 1024, quality, brotli_window_bits(raw.len()));
    writer
        .write_all(raw)
        .map_err(|e| Error::Codec(format!("brotli: {e}")))?;
    // `into_inner` finishes the stream (into `out`, which needs no I/O to fail).
    let _ = writer.into_inner();
    Ok(out)
}

/// The plaintext of a segment from its codec byte and its stored bytes; it must be exactly
/// `expected` bytes (decoding stops one byte past: a segment cannot inflate past its size).
pub fn decode_segment(
    codec_byte: u8,
    data: &[u8],
    expected: usize,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    let plain = match Codec::from_byte(codec_byte)? {
        Codec::Stored => Zeroizing::new(data.to_vec()),
        Codec::Zstd => Zeroizing::new(
            zstd::bulk::decompress(data, expected)
                .map_err(|_| damaged("a zstd segment does not decompress"))?,
        ),
        Codec::Brotli => {
            use std::io::Read;
            let mut plain = Zeroizing::new(Vec::with_capacity(expected));
            brotli::Decompressor::new(data, 64 * 1024)
                .take(expected as u64 + 1)
                .read_to_end(&mut plain)
                .map_err(|_| damaged("a brotli segment does not decompress"))?;
            plain
        }
        Codec::ZstdDict => {
            let (id, frame) = match data {
                [lo, hi, frame @ ..] => (u16::from_le_bytes([*lo, *hi]), frame),
                _ => return Err(damaged("a dictionary segment without its dictionary id")),
            };
            let dictionary = dict::by_id(id)
                .ok_or_else(|| Error::Unsupported(format!("dictionary {id}")))?;
            let mut decompressor = zstd::bulk::Decompressor::with_dictionary(dictionary.bytes)
                .map_err(|e| Error::Codec(format!("zstd: {e}")))?;
            Zeroizing::new(
                decompressor
                    .decompress(frame, expected)
                    .map_err(|_| damaged("a zstd dictionary segment does not decompress"))?,
            )
        }
    };
    if plain.len() != expected {
        return Err(damaged(
            "a segment decodes to another length than the container says",
        ));
    }
    Ok(plain)
}
