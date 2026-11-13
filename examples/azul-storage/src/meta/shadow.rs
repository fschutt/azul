//! What the reclaimer's shadow rounds of a drive recorded (feature `encryption`; ARCH29 C8,
//! X-DELTA-AZDRIVE-SYNC-6): the evidence [`super::reclaim::go`] asks before a run deletes anything.
//!
//! The reclaimer ships in shadow mode: a round reads the drive's history, says which data objects
//! it would delete, and deletes nothing. The line it printed was counts on the stdout of a GUI app,
//! and its acceptance ("zero would-delete of live objects") compared the analysis with itself, so no
//! round could ever show that the analysis was wrong. Now every round writes one [`Round`] into a
//! [`Ledger`] that lives in the bucket beside the repository (`.azlin/meta/shadow`, sealed with the
//! drive key like everything there): when it ran, on which device, at which revision, the digest of
//! its candidates, and whether it was *clean* - it ran to its end, fitted the budget, and an
//! independent listing of the head (the app's own listing, not the reclaimer's walk) named nothing
//! that its live set lacked. A round that was not clean ends the streak the switch to deleting
//! waits for.
//!
//! The ledger is shared by the drive's devices: each appends its round to the one object under a
//! compare-and-swap, and the next round of any device is due a day after the last of any device
//! ([`EVERY_SECS`]): one round per drive per day, not one per device per maintenance round, which
//! read the whole history of the drive every six hours on every computer.
//!
//! It is advisory: a participant who can write the drive's objects could do worse than falsify it,
//! and a ledger that does not open starts again (fewer clean rounds, never more).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    bucket::{Bucket, Version},
    keys,
    reclaim::GoEvidence,
    seal::Sealer,
    wal::{open_json, seal_json},
    MetaError,
};

/// How long after the last round of any device the next one is due (seconds).
pub const EVERY_SECS: u64 = 86_400;

/// Rounds the ledger keeps (the newest).
pub const KEEP_ROUNDS: usize = 64;

/// The version of what a round means by "clean": the analysis that finds the candidates, the window
/// it judges the versions by (the restore dialog's), the independent listing of the head and the
/// bucket's clock that vouches for the stamp. A round carries the version of the analysis that made
/// it ([`Round::rules`]) and only rounds of THIS version count towards the switch to deleting
/// ([`Ledger::evidence`]): seven clean rounds of an older definition of "clean" are no evidence for
/// the next one (X-DELTA-SYNC4-7). Bump it with any change of `analyse`, of `restorable` or of the
/// window, of the head's listing or of the clock check of a round: the ledgers of the builds before
/// it then count for nothing, and the streak starts again (a round that does not count only holds
/// the switch back).
pub const ANALYSIS_RULES: u32 = 1;

/// Attempts at the compare-and-swap of the ledger before the round is left unrecorded.
const WRITE_ATTEMPTS: u32 = 3;

/// One shadow round, as the ledger remembers it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Round {
    /// When it ran, by the clock of the device that ran it (seconds since 1970).
    pub at: u64,
    /// The device that ran it.
    pub device: String,
    /// The manifest revision it read.
    pub revision: u64,
    /// The digest of its candidates ([`super::reclaim::ReclaimReport::digest`]).
    #[serde(default)]
    pub digest: String,
    /// What it would have deleted: objects and their bytes.
    #[serde(default)]
    pub candidates: u64,
    #[serde(default)]
    pub candidate_bytes: u64,
    /// The data objects the head and the window name.
    #[serde(default)]
    pub live: u64,
    /// The version of the analysis that made it ([`ANALYSIS_RULES`]); 0 for a round a build wrote
    /// before rounds were versioned: such a round never counts.
    #[serde(default)]
    pub rules: u32,
    /// Whether it ran to its end, fitted the budget and the independent listing agreed.
    pub clean: bool,
    /// Why it was not clean (empty when it was).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unclean: String,
}

/// The record of a hold on the deleting: the sealed content of the object [`keys::RECLAIM_HOLD`]. Its PRESENCE is the
/// switch; this says who set it and why, for whoever looks (the content is never read to decide).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteHold {
    /// When it was set, by the clock of the device that set it (seconds since 1970).
    pub at: u64,
    /// The device that set it.
    pub by: String,
    /// Why, in the words of whoever set it.
    #[serde(default)]
    pub why: String,
}

/// The shadow rounds of a drive, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub rounds: Vec<Round>,
}

/// What a run knows about itself that the ledger cannot say ([`GoEvidence`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Footing {
    pub drive_devices: u32,
    pub rederived_live_equal: Option<bool>,
    pub window_rederived: Option<bool>,
    pub clock_agrees: Option<bool>,
    pub byte_budget_ok: bool,
    pub floor_known: bool,
    pub restore_source: bool,
}

/// What a round that was clean by the analysis says until the bucket's clock has vouched for its
/// stamp ([`Ledger::confirm`]): it is written NOT clean first, so a stop or a lost write between the
/// two leaves a round that does not count - never one that does (X-DELTA-SYNC2-2: the stamp of a round
/// is the writing device's clock, and one past-dated clean round stretched `days`).
pub const UNVERIFIED: &str = "the bucket's clock has not vouched for this round's time";

impl Ledger {
    /// Whether a round is due at `now`: none was run in the last [`EVERY_SECS`]. A round stamped in
    /// the future (a device whose clock was ahead) is a stale one, not a fresh one.
    #[must_use]
    pub fn due(&self, now: u64) -> bool {
        match self.rounds.iter().map(|round| round.at).max() {
            None => true,
            Some(last) if last > now => true,
            Some(last) => now - last >= EVERY_SECS,
        }
    }

    /// Adds `round` and forgets the oldest ones beyond [`KEEP_ROUNDS`].
    pub fn record(&mut self, round: Round) {
        self.rounds.push(round);
        let extra = self.rounds.len().saturating_sub(KEEP_ROUNDS);
        self.rounds.drain(..extra);
    }

    /// What the record shows at `now`, with what the run knows of its own footing: the clean rounds
    /// in a row at its end, the days they span and the devices that ran them. A stamp in the future
    /// counts as `now`, so a device with its clock ahead does not make days.
    #[must_use]
    pub fn evidence(&self, now: u64, footing: &Footing) -> GoEvidence {
        let mut clean_rounds = 0u32;
        let mut stamps: Vec<u64> = Vec::new();
        let mut devices: BTreeSet<&str> = BTreeSet::new();
        for round in self.rounds.iter().rev() {
            // A round of another version of the analysis ends the streak as one that was not clean.
            if !round.clean || round.rules != ANALYSIS_RULES {
                break;
            }
            clean_rounds = clean_rounds.saturating_add(1);
            stamps.push(round.at.min(now));
            devices.insert(round.device.as_str());
        }
        let days = match (stamps.iter().min(), stamps.iter().max()) {
            (Some(first), Some(last)) => u32::try_from((last - first) / 86_400).unwrap_or(u32::MAX),
            _ => 0,
        };
        GoEvidence {
            clean_rounds,
            days,
            devices: u32::try_from(devices.len()).unwrap_or(u32::MAX),
            drive_devices: footing.drive_devices.max(1),
            rederived_live_equal: footing.rederived_live_equal,
            window_rederived: footing.window_rederived,
            clock_agrees: footing.clock_agrees,
            byte_budget_ok: footing.byte_budget_ok,
            floor_known: footing.floor_known,
            restore_source: footing.restore_source,
        }
    }

    /// The ledger of the drive in `bucket`, and the version it was read at (what [`Ledger::write`]
    /// replaces). A drive with none has an empty one and no version. One that does not open - the
    /// wrong key, damage, a format that is not this one - is an empty ledger at the version it has:
    /// the next write replaces it.
    ///
    /// # Errors
    ///
    /// The bucket's, when it cannot be asked.
    pub fn read<B: Bucket + ?Sized, S: Sealer + ?Sized>(
        bucket: &B,
        sealer: &S,
    ) -> Result<(Ledger, Option<Version>), MetaError> {
        match bucket.read(keys::SHADOW)? {
            None => Ok((Ledger::default(), None)),
            Some((bytes, version)) => {
                // collapse-ok: a ledger that does not open starts again, and an empty one shows
                // fewer clean rounds, never more: the failure only holds the switch back.
                let ledger =
                    open_json::<Ledger, S>(sealer, keys::SHADOW, &bytes).unwrap_or_default();
                Ok((ledger, Some(version)))
            }
        }
    }

    /// Writes the ledger over the version `known` it was read at (or creates it when `known` is
    /// none). `Ok(false)`: another device wrote first, and nothing was changed.
    ///
    /// # Errors
    ///
    /// The bucket's.
    pub fn write<B: Bucket + ?Sized, S: Sealer + ?Sized>(
        &self,
        bucket: &B,
        sealer: &S,
        known: Option<&str>,
    ) -> Result<bool, MetaError> {
        let sealed = seal_json(sealer, keys::SHADOW, self)?;
        let written = match known {
            Some(known) => bucket.replace(keys::SHADOW, &sealed, known),
            None => bucket.create(keys::SHADOW, &sealed),
        };
        match written {
            Ok(_) => Ok(true),
            Err(MetaError::Conflict { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Adds `round` to the ledger of the drive in `bucket` under the compare-and-swap, reading it
    /// again when another device was first (up to [`WRITE_ATTEMPTS`] times): the ledger as written,
    /// or `None` when the round could not be recorded - which only means the streak does not grow.
    ///
    /// # Errors
    ///
    /// The bucket's.
    pub fn append<B: Bucket + ?Sized, S: Sealer + ?Sized>(
        bucket: &B,
        sealer: &S,
        round: &Round,
    ) -> Result<Option<Ledger>, MetaError> {
        for _ in 0..WRITE_ATTEMPTS {
            let (mut ledger, version) = Ledger::read(bucket, sealer)?;
            ledger.record(round.clone());
            if ledger.write(bucket, sealer, version.as_deref())? {
                return Ok(Some(ledger));
            }
        }
        Ok(None)
    }

    /// Makes the round `written` - appended by [`Ledger::append`] as not clean, with
    /// [`UNVERIFIED`] for its reason - the clean round it was: the bucket's clock vouched for its
    /// stamp. It is found by what it says (device, time, revision, digest), read again when another
    /// device was first (up to [`WRITE_ATTEMPTS`] times); a round that is not on the ledger any more
    /// (more than [`KEEP_ROUNDS`] later) or was changed stays as it is. The ledger as written, or
    /// `None` when nothing was changed - which only means the round does not count.
    ///
    /// # Errors
    ///
    /// The bucket's.
    pub fn confirm<B: Bucket + ?Sized, S: Sealer + ?Sized>(
        bucket: &B,
        sealer: &S,
        written: &Round,
    ) -> Result<Option<Ledger>, MetaError> {
        for _ in 0..WRITE_ATTEMPTS {
            let (mut ledger, version) = Ledger::read(bucket, sealer)?;
            let Some(round) = ledger.rounds.iter_mut().rev().find(|round| {
                !round.clean
                    && round.unclean == UNVERIFIED
                    && round.rules == written.rules
                    && round.device == written.device
                    && round.at == written.at
                    && round.revision == written.revision
                    && round.digest == written.digest
            }) else {
                return Ok(None);
            };
            round.clean = true;
            round.unclean.clear();
            if ledger.write(bucket, sealer, version.as_deref())? {
                return Ok(Some(ledger));
            }
        }
        Ok(None)
    }
}
