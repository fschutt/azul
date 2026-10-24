//! What a forward transform may still spend: the time budget (projected from the pace of the
//! images and streams done so far, since neither Lepton nor the deflate analysis can be
//! stopped midway) and the memory cap (the input, everything transformed so far, and the next
//! piece's working set).

use std::time::Instant;

use super::{DeflateStats, JpegStats};
use crate::options::Options;

/// Lepton's encode pace before any image was timed, per thread (an M2 core does about
/// 4.8 MB/s; a phone core about half). Its verifying decode takes about as long again.
const LEPTON_BYTES_PER_SEC: f64 = 2.0e6;
/// The deflate analysis's pace before any stream was timed, in plain-text bytes (1 to 3 MB/s
/// on an M2 core). Its verifying rebuild is faster; counted at half again.
const DEFLATE_BYTES_PER_SEC: f64 = 1.0e6;

/// What the transform does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Job {
    Lepton,
    Deflate,
}

pub(crate) struct Work<'o> {
    pub options: &'o Options,
    started: Instant,
    budget_secs: f64,
    /// Time spent and bytes done per job, for the pace.
    lepton: (f64, u64),
    deflate: (f64, u64),
    /// The input's length plus the bytes held by the pieces made so far.
    pub held: u64,
    pub jpeg: JpegStats,
    pub deflate_stats: DeflateStats,
}

impl<'o> Work<'o> {
    pub fn new(input_len: u64, options: &'o Options) -> Work<'o> {
        Work {
            options,
            started: Instant::now(),
            budget_secs: options.limits.transform_time.for_len(input_len).as_secs_f64(),
            lepton: (0.0, 0),
            deflate: (0.0, 0),
            held: input_len,
            jpeg: JpegStats::default(),
            deflate_stats: DeflateStats::default(),
        }
    }

    fn threads(&self) -> f64 {
        // More threads help Lepton less than linearly: it splits an image into bands.
        1.0 + 0.75 * (self.options.threads.clamp(1, 8) as f64 - 1.0)
    }

    /// Whether `bytes` more of `job` fit the time left: the time spent, the verify still owed
    /// for the work done (about its own time again), and this piece's work and verify at the
    /// pace so far.
    pub fn has_time(&self, job: Job, bytes: u64) -> bool {
        let (default_pace, done, verify_factor) = match job {
            Job::Lepton => (LEPTON_BYTES_PER_SEC * self.threads(), self.lepton, 2.0),
            Job::Deflate => (DEFLATE_BYTES_PER_SEC, self.deflate, 1.5),
        };
        let pace = if done.1 > 0 && done.0 > 0.0 {
            done.1 as f64 / done.0
        } else {
            default_pace
        };
        let owed = self.lepton.0 + self.deflate.0 * 0.5;
        let projected = verify_factor * bytes as f64 / pace;
        self.started.elapsed().as_secs_f64() + owed + projected <= self.budget_secs
    }

    /// Whether a piece whose working set is `bytes` fits the memory cap now.
    pub fn has_memory(&self, bytes: u64) -> bool {
        self.held.saturating_add(bytes) <= self.options.limits.max_memory
    }

    /// Records `bytes` of `job` done in `secs`.
    pub fn did(&mut self, job: Job, bytes: u64, secs: f64) {
        let done = match job {
            Job::Lepton => &mut self.lepton,
            Job::Deflate => &mut self.deflate,
        };
        done.0 += secs;
        done.1 += bytes;
    }
}
