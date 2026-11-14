//! The device's rule "a head never goes backwards", with the one exception the metadata group can
//! announce (planning/architecture.md section 5.13 "A head never goes backwards. A device refuses a
//! space head older than the newest it has seen. The only way back is a restore (`restore_as_of`),
//! which the metadata group announces signed; devices heal from that announcement only", section 6.10
//! S15 "A space head older than the last one seen is refused, unless the metadata group's signed
//! restore announcement raised the restore epoch (V3)", section 6.10 F3 and V3, section 12 E4
//! "devices accept going back only through a restore signed by the map key"; build unit S16, lane
//! W1-S16a).
//!
//! It is the head-register form of `azul-storage/src/restore.rs` (its table is the specification):
//! [`judge`] is pure, total and the one place that decides what the space serves against what the
//! device remembers.
//!
//! | what the space serves against the device's memory | the announcement | the standing |
//! |---|---|---|
//! | anything (any version) | a verified restore, newer than the device's epoch, that covers the space | [`Standing::Restored`] |
//! | the same restore epoch, a higher version | none that applies | [`Standing::Behind`] |
//! | the same restore epoch, the same version, the same value | none that applies | [`Standing::InSync`] |
//! | the same restore epoch, a LOWER version | none that applies | [`Standing::Rollback`] |
//! | the same restore epoch, the same version, ANOTHER value | none that applies | [`Standing::Equivocation`] |
//! | a LOWER restore epoch (the lost timeline, whatever the version) | none that applies | [`Standing::Rollback`] |
//! | a HIGHER restore epoch | none that applies | [`Standing::UnannouncedEpoch`] |
//! | a first look (no version remembered): only the epoch decides | none that applies | [`Standing::Behind`], or `Rollback` below the announced epoch |
//!
//! THE FIVE ROWS OF THE SPECIFICATION, for heads (DRIFT X-DELTA-W1S16A-2): restored, same, higher,
//! lower, and the row for a counter that says nothing about the drive. A head's version is always the
//! register's shared counter, so a head has no "own counter" in the old sense (a drive synced by name
//! keeps `azul_storage::restore::Clock::Own`); the row is restated for a pairing that remembers no
//! version yet (`Seen::fresh`): the counter says nothing, so only the epoch decides.
//!
//! THE EPOCH of a head is the restore epoch of the register (`HeadRecord::restore_epoch`): the epoch of
//! the last restore that covered the space, or 0. A device remembers, per space, the newest epoch it
//! accepted. A head of a LOWER epoch than that is the lost timeline of a restore, whatever its version
//! (E4: "one older than a device's newest head"): it can be a version far above the device's, and only
//! the epoch shows that it is the old history coming back. A head of a HIGHER epoch is a restore the
//! device has not been told of: it waits for the announcement and does not follow the head.
//!
//! WHAT THE CALLER DOES with each standing: `InSync` nothing; `Behind` the ordinary merge; `Rollback`
//! refuses (changes nothing anywhere, and asks the leader once when the answer came from a replica: a
//! stale replica costs a retry, never a rollback, planning/architecture.md section 2.4); `Restored`
//! heals (the base is untrusted where the head disagrees, whatever the device holds beyond the
//! survivor is sent again, never deleted: V3); `Equivocation` and `UnannouncedEpoch` refuse, and raise
//! an alarm if they persist.
//!
//! ```compile_fail,E0308
//! // An unverified restore record cannot be passed to `judge`: it takes a `VerifiedRestore` (made
//! // only by `azlin_chain::verify_record`) and nothing else.
//! use azlin_chain::RestoreRecord;
//! use azlin_seams::SpaceId;
//! use azlin_sync::{judge, Seen, Served};
//! fn misuse(record: &RestoreRecord, seen: &Seen, served: &Served, space: SpaceId) {
//!     let _ = judge(seen, served, None, Some(record), space);
//! }
//! ```

use core::cmp::Ordering;

use azlin_chain::{ChainId, VerifiedRestore};
use azlin_seams::{mutant, Hash32, RestoreEpoch, SpaceId};

use crate::head::HeadRecord;
use crate::version::Version;

/// What a device remembers of one space's head between two looks: the newest version it accepted (none
/// before the first look) and the newest restore epoch it accepted, or was born after (`baseline`).
/// The hash of the value it saw at that version is remembered next to it and passed to [`judge`] on
/// its own.
///
/// Implements planning/architecture.md section 5.13 (a device refuses a head older than the newest it
/// has seen) and section 6.10 S15.
///
/// Caller: `judge`, `Seen::after`; SYNC-CLIENT stores it per (device, space) pairing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The newest version accepted; `None` before the first look.
    pub version: Option<Version>,
    /// The newest restore epoch accepted, or the one the pairing was born after.
    pub restore_epoch: RestoreEpoch,
}

/// What the space serves now, as the device reads it from a `head_get` answer: the version, the
/// restore epoch and the hash of the value (`HeadValue::hash`).
///
/// Implements planning/architecture.md section 5.13 (`head_get`) and section 6.10 S15.
///
/// Caller: `judge`, `Seen::after`; SYNC-CLIENT builds it from a `HeadRecord` (`Served::of`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Served {
    /// The version served.
    pub version: Version,
    /// The restore epoch of the head served.
    pub restore_epoch: RestoreEpoch,
    /// The hash of the value served.
    pub value_hash: Hash32,
}

impl Served {
    /// What a `head_get` answer serves.
    pub fn of(record: &HeadRecord) -> Served {
        Served {
            version: record.version,
            restore_epoch: record.restore_epoch,
            value_hash: record.value.hash(),
        }
    }
}

/// What the head the space serves is, for a device that remembers [`Seen`].
///
/// Implements planning/architecture.md section 5.13 (a head never goes backwards), section 6.10 S15
/// and V3 (a restore announced is healed from, an unannounced one is refused) and section 12 E4.
///
/// Caller: `judge`; SYNC-CLIENT reacts to each (see the module).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Standing {
    /// The head is where the device last saw it.
    InSync,
    /// The head moved on (other devices committed): the ordinary merge.
    Behind,
    /// The head is older than the device saw, or of the lost timeline of a restore, and no
    /// announcement excuses it: a replayed or rolled-back head. Refused: nothing is changed anywhere.
    Rollback,
    /// The space was restored and the metadata group announced it: the device's memory of the head is
    /// untrusted where the head disagrees, and `epoch` is accepted.
    Restored {
        /// The restore epoch of the announcement.
        epoch: RestoreEpoch,
    },
    /// The same version of the same restore epoch with another value than the one the device saw: the
    /// server serves two histories. Refused and alarmed.
    Equivocation,
    /// The head carries a restore epoch above the device's, and no announcement that applies came with
    /// it: refused until the announcement arrives.
    UnannouncedEpoch,
}

/// The restore epoch a new or reset pairing starts at: that of the newest verified restore announcement
/// that covers `space`, or 0 when there is none, so that an announcement that was made before the
/// pairing existed never excuses a rollback after it. An announcement that does not cover the space
/// counts for nothing.
///
/// Implements planning/architecture.md section 6.10 V3 and `azul-storage/src/restore.rs` (`baseline`:
/// a new or reset pairing starts at the current epoch); DRIFT X-DELTA-W1S16A-10 (the brief's
/// `baseline(notice)` has no space: a restore record lists many spaces).
///
/// Caller: `Seen::fresh`; SYNC-CLIENT when it pairs a space on a device.
pub fn baseline(notice: Option<&VerifiedRestore>, space: SpaceId) -> RestoreEpoch {
    match notice {
        Some(notice) if notice.covers(&ChainId::space(space)) => notice.epoch(),
        Some(_) | None => RestoreEpoch::ZERO,
    }
}

impl Seen {
    /// The memory of a pairing made now: no version yet, and the epoch of [`baseline`].
    pub fn fresh(notice: Option<&VerifiedRestore>, space: SpaceId) -> Seen {
        Seen {
            version: None,
            restore_epoch: baseline(notice, space),
        }
    }

    /// The memory after `standing` was decided for `served`: `InSync` and `Behind` take the head (the
    /// version never goes below what is remembered), `Restored` takes the head and the announced epoch
    /// (the one way the version goes down, and only when the epoch is above the remembered one), and a
    /// refusal changes nothing. The epoch never goes down.
    pub fn after(&self, standing: &Standing, served: &Served) -> Seen {
        match *standing {
            Standing::InSync | Standing::Behind => {
                let version = match self.version {
                    Some(held) if held > served.version && !mutant!("sync_seen_regresses") => held,
                    Some(_) | None => served.version,
                };
                let restore_epoch = if served.restore_epoch > self.restore_epoch {
                    served.restore_epoch
                } else {
                    self.restore_epoch
                };
                Seen {
                    version: Some(version),
                    restore_epoch,
                }
            }
            Standing::Restored { epoch } if epoch > self.restore_epoch => Seen {
                version: Some(served.version),
                restore_epoch: epoch,
            },
            Standing::Restored { .. }
            | Standing::Rollback
            | Standing::Equivocation
            | Standing::UnannouncedEpoch => *self,
        }
    }
}

/// What the head the space serves is. `served` is what `head_get` answered now; `seen_value_hash` the
/// hash of the value the device saw at `seen.version` (it remembers it with the version); `notice` the
/// newest restore announcement the device holds that was VERIFIED (a `VerifiedRestore` can be made
/// only by `azlin_chain::verify_record`, so an unverified record cannot be passed); `space` the space.
/// Total: every input has one answer (the table in the module's documentation).
///
/// An announcement excuses nothing unless it covers `space`, is above the epoch the device accepted,
/// and the head served carries exactly its epoch (a head below it is the lost timeline, a head above
/// it a restore not announced yet).
///
/// Implements planning/architecture.md section 5.13 (a head never goes backwards), section 6.10 S15,
/// V3 and F3 (a replayed old head is refused; an announced restore is healed from), and section 12 E4.
///
/// Caller: SYNC-CLIENT (azcloud-kit's sync engine, in place of `azul_storage::restore::judge` for
/// native spaces); S47c's E4 stories.
pub fn judge(
    seen: &Seen,
    served: &Served,
    seen_value_hash: Option<Hash32>,
    notice: Option<&VerifiedRestore>,
    space: SpaceId,
) -> Standing {
    // 1. An announcement that applies: verified (by its type), covering this space, newer than the
    //    epoch the device accepted. A replayed old record or one for another space is no excuse.
    if let Some(notice) = notice {
        let covers =
            notice.covers(&ChainId::space(space)) || mutant!("sync_judge_notice_coverage_ignored");
        let newer =
            notice.epoch() > seen.restore_epoch || mutant!("sync_judge_notice_epoch_ignored");
        if covers && newer {
            return match served.restore_epoch.cmp(&notice.epoch()) {
                Ordering::Equal => Standing::Restored {
                    epoch: notice.epoch(),
                },
                Ordering::Less => {
                    if mutant!("sync_judge_lost_timeline_excused") {
                        Standing::Restored {
                            epoch: notice.epoch(),
                        }
                    } else {
                        Standing::Rollback
                    }
                }
                Ordering::Greater => Standing::UnannouncedEpoch,
            };
        }
    }
    // 2. The restore epochs: the head of a lower one is the lost timeline, of a higher one a restore
    //    nobody announced. A first look (no version remembered) has nothing to heal, so it takes a
    //    higher epoch as it is (DRIFT X-DELTA-W1S16A-15).
    match served.restore_epoch.cmp(&seen.restore_epoch) {
        Ordering::Greater => {
            if seen.version.is_none() {
                return Standing::Behind;
            }
            if !mutant!("sync_judge_unannounced_epoch_accepted") {
                return Standing::UnannouncedEpoch;
            }
        }
        Ordering::Less => {
            if !mutant!("sync_judge_lost_timeline_accepted") {
                return Standing::Rollback;
            }
        }
        Ordering::Equal => {}
    }
    // 3. The same epoch: the counter and the value.
    let Some(seen_version) = seen.version else {
        return Standing::Behind;
    };
    match served.version.cmp(&seen_version) {
        Ordering::Greater => Standing::Behind,
        Ordering::Less => {
            if mutant!("sync_judge_rollback_as_behind") {
                Standing::Behind
            } else {
                Standing::Rollback
            }
        }
        Ordering::Equal => match seen_value_hash {
            Some(hash) if hash != served.value_hash => {
                if mutant!("sync_judge_equivocation_unchecked") {
                    Standing::InSync
                } else {
                    Standing::Equivocation
                }
            }
            Some(_) => Standing::InSync,
            // The device does not remember the value it saw: it cannot say the head is the one it
            // has, so it reads the head again (an ordinary merge) rather than call it in sync.
            None => Standing::Behind,
        },
    }
}
