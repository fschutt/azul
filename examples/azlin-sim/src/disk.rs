//! The simulated disk: an append log of typed records with a durability cursor and the faults of a
//! real disk (planning/architecture.md section 1.4 rule 2: torn and short writes, a flush that
//! lies, sector errors, flipped bits, vanishing disks; section 6.10 V1 "saved means fsynced").
//!
//! `append` writes (volatile); `fsync` moves the durability cursor. A CRASH keeps the synced
//! prefix plus a random-length prefix of the unsynced records (the operating system flushed some),
//! the last of which is torn (present but corrupt) with the torn-write probability. A LYING fsync
//! returns `Ok` and does not move the cursor: the lie shows only after a crash. A bit flip marks
//! one record corrupt; `vanish` removes everything. Records are typed values, not bytes: a
//! corruption is a flag the reader meets as [`DiskError::Corrupt`], which is what the checksum of a
//! real write-ahead log would say. Every outcome depends only on the generator it is given.

use azlin_seams::sim::Rng;

use crate::config::{Ppm, MAX_DISK_RECORDS};
use crate::draw::below;

/// Why a disk operation failed.
///
/// Implements planning/architecture.md section 1.4 rule 2 (sector errors, vanishing disks) and
/// rule 4 (a limit is a typed refusal).
///
/// Caller: `SimDisk`, `leader`, `device`, tests/s1_02.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskError {
    /// A read failed (a sector error); a retry can succeed.
    Read,
    /// The record at this index does not match its checksum.
    Corrupt {
        /// The index of the first corrupt record.
        index: usize,
    },
    /// The disk is gone: nothing can be read or written.
    Gone,
    /// The disk holds as many records as it may.
    Full,
}

error_text!(DiskError {
    DiskError::Read => "the read failed",
    DiskError::Corrupt { index } => "record {index} is corrupt",
    DiskError::Gone => "the disk is gone",
    DiskError::Full => "the disk is full",
});

/// What an `fsync` did.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a flush that lies).
///
/// Caller: `SimDisk::fsync`, `leader`, `device`, tests/s1_02.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Synced {
    /// Everything written so far is durable.
    Durable,
    /// The flush said `Ok` and moved nothing.
    Lied,
}

/// What a crash did to the disk.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a crash at any point, torn writes).
///
/// Caller: `SimDisk::crash`, `world::World`, `invariants`, tests/s1_02.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CrashReport {
    /// Records that are on the disk after the crash.
    pub kept: usize,
    /// Records that were written and are gone.
    pub lost: usize,
    /// Of those, records a lying fsync had reported durable.
    pub lost_by_lie: usize,
    /// The last kept record is torn.
    pub torn: bool,
}

/// What a disk did, in counters.
///
/// Implements planning/architecture.md section 1.4 rule 2 (every fault is counted).
///
/// Caller: `SimDisk::stats`, `Coverage`, tests/s1_02.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiskStats {
    /// Records appended.
    pub appends: u64,
    /// Flushes asked for.
    pub fsyncs: u64,
    /// Flushes that lied.
    pub lies: u64,
    /// Crashes.
    pub crashes: u64,
    /// Crashes that tore a record.
    pub torn: u64,
    /// Records lost in crashes.
    pub lost: u64,
    /// Records lost that a lying fsync had reported durable.
    pub lost_by_lie: u64,
    /// Bits flipped.
    pub flips: u64,
    /// Reads that failed.
    pub read_errors: u64,
    /// Disks that vanished.
    pub vanishes: u64,
}

/// The valid prefix of a log: the records before the first corrupt one, and what cut it short.
///
/// Implements planning/architecture.md section 5.9 ("any copy can restore it") and glue rule G2
/// (restart replays the valid prefix and truncates at the first corrupt record).
///
/// Caller: `SimDisk::valid_prefix`, `leader`, `device`.
#[derive(Clone, Debug)]
pub struct Prefix<R> {
    /// The records before the first corrupt one.
    pub records: Vec<R>,
    /// Why the log ended before its last record, if it did.
    pub cut: Option<DiskError>,
}

#[derive(Clone, Debug)]
struct Slot<R> {
    rec: R,
    corrupt: bool,
}

/// A simulated disk of records of type `R`.
///
/// Implements planning/architecture.md section 1.4 rule 2 (disk faults) and section 6.10 V1.
///
/// Caller: `leader::Leader`, `device::Device`, tests/s1_02.
#[derive(Clone, Debug)]
pub struct SimDisk<R: Clone> {
    slots: Vec<Slot<R>>,
    synced: usize,
    lied_upto: usize,
    gone: bool,
    lie_left: u32,
    tear_next: bool,
    read_err_next: bool,
    stats: DiskStats,
    limit: usize,
}

impl<R: Clone> Default for SimDisk<R> {
    fn default() -> SimDisk<R> {
        SimDisk::new()
    }
}

impl<R: Clone> SimDisk<R> {
    /// An empty disk with the bound [`MAX_DISK_RECORDS`].
    pub fn new() -> SimDisk<R> {
        SimDisk::with_limit(MAX_DISK_RECORDS)
    }

    /// An empty disk that holds at most `limit` records.
    pub fn with_limit(limit: usize) -> SimDisk<R> {
        SimDisk {
            slots: Vec::new(),
            synced: 0,
            lied_upto: 0,
            gone: false,
            lie_left: 0,
            tear_next: false,
            read_err_next: false,
            stats: DiskStats::default(),
            limit,
        }
    }

    /// How many records the disk holds (written, durable or not).
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the disk holds no record.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// How many records are durable (before the cursor).
    pub fn synced_len(&self) -> usize {
        self.synced
    }

    /// The bound.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Whether the disk has vanished.
    pub fn is_gone(&self) -> bool {
        self.gone
    }

    /// The counters.
    pub fn stats(&self) -> DiskStats {
        self.stats
    }

    /// Writes a record (volatile until the next honest fsync).
    pub fn append(&mut self, rec: R) -> Result<(), DiskError> {
        if self.gone {
            return Err(DiskError::Gone);
        }
        if self.slots.len() >= self.limit {
            return Err(DiskError::Full);
        }
        self.slots.push(Slot {
            rec,
            corrupt: false,
        });
        self.stats.appends = self.stats.appends.saturating_add(1);
        Ok(())
    }

    /// Flushes: everything written becomes durable, unless the flush lies (`lies`, or a lie armed
    /// by [`SimDisk::arm_lie`]): then it says `Ok` and the cursor stays.
    pub fn fsync(&mut self, lies: bool) -> Result<Synced, DiskError> {
        if self.gone {
            return Err(DiskError::Gone);
        }
        self.stats.fsyncs = self.stats.fsyncs.saturating_add(1);
        let armed = self.lie_left > 0;
        if armed {
            self.lie_left = self.lie_left.saturating_sub(1);
        }
        if lies || armed {
            self.stats.lies = self.stats.lies.saturating_add(1);
            self.lied_upto = self.slots.len();
            return Ok(Synced::Lied);
        }
        self.synced = self.slots.len();
        self.lied_upto = self.synced;
        Ok(Synced::Durable)
    }

    /// Replaces the whole content with `records` as one atomic step (a file written beside the old
    /// one and renamed over it after its fsync): durable at once, or, when the flush lies, not at
    /// all (the old content stays and the lie is counted).
    pub fn replace(&mut self, records: Vec<R>, lies: bool) -> Result<Synced, DiskError> {
        if self.gone {
            return Err(DiskError::Gone);
        }
        if records.len() > self.limit {
            return Err(DiskError::Full);
        }
        self.stats.fsyncs = self.stats.fsyncs.saturating_add(1);
        let armed = self.lie_left > 0;
        if armed {
            self.lie_left = self.lie_left.saturating_sub(1);
        }
        if lies || armed {
            self.stats.lies = self.stats.lies.saturating_add(1);
            return Ok(Synced::Lied);
        }
        self.slots = records
            .into_iter()
            .map(|rec| Slot {
                rec,
                corrupt: false,
            })
            .collect();
        self.synced = self.slots.len();
        self.lied_upto = self.synced;
        Ok(Synced::Durable)
    }

    /// Reads every record in order; the first corrupt record, a failed read and a vanished disk
    /// are typed errors. An armed read error fails exactly one read.
    pub fn read_all(&mut self) -> Result<Vec<R>, DiskError> {
        if self.gone {
            return Err(DiskError::Gone);
        }
        if self.read_err_next {
            self.read_err_next = false;
            self.stats.read_errors = self.stats.read_errors.saturating_add(1);
            return Err(DiskError::Read);
        }
        if let Some(index) = self.slots.iter().position(|slot| slot.corrupt) {
            return Err(DiskError::Corrupt { index });
        }
        Ok(self.slots.iter().map(|slot| slot.rec.clone()).collect())
    }

    /// The records before the first corrupt one and what cut the log short (the restart rule G2).
    /// A failed read or a vanished disk gives no record and the error.
    pub fn valid_prefix(&mut self) -> Prefix<R> {
        match self.read_all() {
            Ok(records) => Prefix { records, cut: None },
            Err(DiskError::Corrupt { index }) => Prefix {
                records: self
                    .slots
                    .iter()
                    .take(index)
                    .map(|slot| slot.rec.clone())
                    .collect(),
                cut: Some(DiskError::Corrupt { index }),
            },
            Err(error) => Prefix {
                records: Vec::new(),
                cut: Some(error),
            },
        }
    }

    /// The record at `index` as it is now, without any fault (the node's own apply loop reads the
    /// records it wrote; a record that is gone after a crash is `None`).
    pub fn record_at(&self, index: usize) -> Option<R> {
        self.slots.get(index).map(|slot| slot.rec.clone())
    }

    /// The records as they are now, without any fault, for the checkers of the harness (never for
    /// the nodes: a node reads through [`SimDisk::read_all`]).
    pub fn peek(&self) -> Vec<R> {
        self.slots.iter().map(|slot| slot.rec.clone()).collect()
    }

    /// A crash: the synced prefix stays, a random-length prefix of the unsynced records stays too
    /// (the operating system flushed some), and the last of those is torn when `tear_rate` says so
    /// or a tear is armed. The cursor then stands at the end (what is there is on the platter).
    pub fn crash(&mut self, rng: &mut Rng, tear_rate: Ppm) -> CrashReport {
        self.stats.crashes = self.stats.crashes.saturating_add(1);
        let tear_draw = tear_rate.hit(rng);
        let flushed = below(
            rng,
            (self.slots.len().saturating_sub(self.synced) as u64).saturating_add(1),
        );
        let tear = tear_draw || self.tear_next;
        self.tear_next = false;
        let before = self.slots.len();
        let keep = self.synced.saturating_add(flushed as usize).min(before);
        let lost_by_lie = self
            .lied_upto
            .min(before)
            .saturating_sub(keep.max(self.synced));
        self.slots.truncate(keep);
        let mut torn = false;
        if keep > self.synced && tear {
            if let Some(last) = self.slots.last_mut() {
                last.corrupt = true;
                torn = true;
            }
        }
        let lost = before.saturating_sub(keep);
        self.synced = self.slots.len();
        self.lied_upto = self.synced;
        self.stats.lost = self.stats.lost.saturating_add(lost as u64);
        self.stats.lost_by_lie = self.stats.lost_by_lie.saturating_add(lost_by_lie as u64);
        if torn {
            self.stats.torn = self.stats.torn.saturating_add(1);
        }
        CrashReport {
            kept: self.slots.len(),
            lost,
            lost_by_lie,
            torn,
        }
    }

    /// The next `count` flushes lie.
    pub fn arm_lie(&mut self, count: u32) {
        self.lie_left = self.lie_left.saturating_add(count);
    }

    /// The next crash tears the last record it keeps.
    pub fn arm_tear(&mut self) {
        self.tear_next = true;
    }

    /// The next read fails.
    pub fn arm_read_error(&mut self) {
        self.read_err_next = true;
    }

    /// One record (chosen from `rng`) turns corrupt; nothing on an empty disk.
    pub fn flip(&mut self, rng: &mut Rng) {
        let count = self.slots.len() as u64;
        let index = below(rng, count) as usize;
        if let Some(slot) = self.slots.get_mut(index) {
            slot.corrupt = true;
            self.stats.flips = self.stats.flips.saturating_add(1);
        }
    }

    /// Everything vanishes: the disk is gone until it is reset.
    pub fn vanish(&mut self) {
        self.slots.clear();
        self.synced = 0;
        self.lied_upto = 0;
        self.gone = true;
        self.stats.vanishes = self.stats.vanishes.saturating_add(1);
    }

    /// A new, empty disk in the place of this one (a restart with the disk wiped); the counters stay.
    pub fn reset(&mut self) {
        self.slots.clear();
        self.synced = 0;
        self.lied_upto = 0;
        self.gone = false;
        self.lie_left = 0;
        self.tear_next = false;
        self.read_err_next = false;
    }
}
