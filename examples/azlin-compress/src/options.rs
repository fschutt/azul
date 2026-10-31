//! How hard to try, and the caps every stage stays under.
//!
//! Two presets cover AzDrive's two moments: [`Options::upload`] (fast enough not to slow a transfer
//! down; still runs Lepton, which shrinks JPEGs by about a fifth and so makes the upload itself
//! shorter) and [`Options::idle`] (the recompression pass on an idle computer on mains power: the
//! highest levels the budget allows).

use std::time::Duration;

/// Plaintext bytes per segment, as AZL1 cuts files.
pub const DEFAULT_SEGMENT_SIZE: u32 = 1 << 20;
/// The smallest and the largest segment size a container may name (AZL1's bounds).
pub const MIN_SEGMENT_SIZE: u32 = 1 << 10;
pub const MAX_SEGMENT_SIZE: u32 = 1 << 24;

/// The largest JPEG image (in pixels) a Lepton record ever holds: the default decode limit,
/// so every container decodes with [`Limits::default`]. Lepton's working set at this size is
/// about 150 MB.
pub const MAX_JPEG_PIXELS: u64 = 26_000_000;

/// A time budget that grows with the input: `fixed` plus `per_mib` for every MiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub fixed: Duration,
    pub per_mib: Duration,
}

impl Budget {
    /// No time at all: the cheapest choice everywhere, no transform.
    pub const ZERO: Budget = Budget {
        fixed: Duration::ZERO,
        per_mib: Duration::ZERO,
    };
    /// More time than any file needs: the levels are the caps in [`Options`].
    pub const UNLIMITED: Budget = Budget {
        fixed: Duration::from_secs(1 << 40),
        per_mib: Duration::from_secs(1 << 20),
    };

    /// The time allowed for `len` bytes.
    #[must_use]
    pub fn for_len(&self, len: u64) -> Duration {
        let mib = u32::try_from(len.div_ceil(1 << 20)).unwrap_or(u32::MAX);
        self.fixed
            .saturating_add(self.per_mib.checked_mul(mib).unwrap_or(Duration::MAX))
    }
}

/// The caps of one encode or decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The memory a whole-file transform may need, in bytes (the input, its output, the
    /// verifying decode, Lepton's and the deflate analysis's working sets). A piece (an image, a
    /// deflate stream) whose estimate is over it stays as it is.
    pub max_memory: u64,
    /// Whole-file transforms run only on files up to this size.
    pub max_transform_input: u64,
    /// The largest JPEG (in pixels) Lepton is given: its working set grows with the pixels.
    /// The encoder never goes past [`MAX_JPEG_PIXELS`], whatever this says.
    pub max_jpeg_pixels: u64,
    /// The largest file a decoder rebuilds (a decompression bomb stops here).
    pub max_decoded_len: u64,
    /// The time the transform stage (encode and its verifying decode) may take.
    pub transform_time: Budget,
    /// The time the codec stage (zstd / brotli over the segments) may take; the levels are
    /// chosen to fit it.
    pub codec_time: Budget,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_memory: 150 << 20,
            max_transform_input: 64 << 20,
            max_jpeg_pixels: MAX_JPEG_PIXELS,
            max_decoded_len: 1 << 40,
            transform_time: Budget {
                fixed: Duration::from_secs(2),
                per_mib: Duration::from_millis(500),
            },
            codec_time: Budget {
                fixed: Duration::from_millis(50),
                per_mib: Duration::from_millis(40),
            },
        }
    }
}

/// How a file is encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Plaintext bytes per segment ([`MIN_SEGMENT_SIZE`] to [`MAX_SEGMENT_SIZE`]).
    pub segment_size: u32,
    /// The highest zstd level ever used. Levels above 19 need far more memory to decode.
    pub max_zstd_level: i32,
    /// The highest brotli quality ever used (11 is the slowest and smallest).
    pub max_brotli_quality: u32,
    /// Whether whole-file transforms (Lepton for JPEG images, deflate reconstruction for PDF,
    /// Office, ZIP, PNG and gzip files) may run.
    pub transforms: bool,
    /// Whether the shipped zstd dictionaries may be used for small files.
    pub dictionaries: bool,
    /// Threads Lepton may use; 1 starts none.
    pub threads: usize,
    /// The file's name extension ("jpg", "csv"), lower or upper case, without the dot. Only a
    /// tie-break: the content decides first.
    pub extension: Option<String>,
    pub limits: Limits,
}

impl Options {
    /// While a file is uploaded: zstd levels that keep up with a fast connection, Lepton on.
    #[must_use]
    pub fn upload() -> Options {
        Options {
            segment_size: DEFAULT_SEGMENT_SIZE,
            max_zstd_level: 19,
            max_brotli_quality: 11,
            transforms: true,
            dictionaries: true,
            threads: 4,
            extension: None,
            limits: Limits::default(),
        }
    }

    /// The recompression pass on an idle computer on mains power: the smallest output the
    /// budget allows (zstd 19, brotli 11 for text).
    #[must_use]
    pub fn idle() -> Options {
        Options {
            limits: Limits {
                transform_time: Budget {
                    fixed: Duration::from_secs(10),
                    per_mib: Duration::from_secs(2),
                },
                codec_time: Budget {
                    fixed: Duration::from_secs(1),
                    per_mib: Duration::from_secs(4),
                },
                ..Limits::default()
            },
            ..Options::upload()
        }
    }

    /// `self` with the file's extension as the detection tie-break.
    #[must_use]
    pub fn with_extension(mut self, extension: &str) -> Options {
        self.extension = Some(extension.to_ascii_lowercase());
        self
    }
}

impl Default for Options {
    fn default() -> Self {
        Options::upload()
    }
}
