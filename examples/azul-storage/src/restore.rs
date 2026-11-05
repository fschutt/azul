//! One restore protocol for plain and encrypted drives (ARCH28 RF45 / AR5a-6; AZ-S2, AZ-S3,
//! AZ-S5, X-AR5a-3, X-AR5a-22): is what a drive serves a restore, a rollback or the ordinary
//! course of other devices' work, and what does a device do with its memory of the drive when it
//! is a restore. Pure: no file, no network, no clock.
//!
//! A restored drive (from its off-site copy, which lags, or one prefix as of a moment) serves
//! state OLDER than what a device last saw. A device that took that for the ordinary course would
//! resurrect deleted files and turn every edit since into a conflict; one that refused every
//! older state would never recover from a real restore. So each restore is announced by the token
//! server, with an epoch one above the drive's last ([`RestoreNotice`]); a device remembers the
//! newest epoch it accepted ([`Seen::epoch`]) and [`judge`] is the one place that decides, from
//! that memory, what the drive serves and the announcement, what the drive's state is:
//!
//! | what the drive serves vs the device's memory | the announcement                      | the standing            |
//! |----------------------------------------------|---------------------------------------|-------------------------|
//! | anything                                     | a newer epoch that covers the prefix  | [`Standing::Restored`]  |
//! | an own-counter drive ([`Clock::Own`])        | none that is new                      | [`Standing::InSync`]    |
//! | the same counter                             | none that is new                      | [`Standing::InSync`]    |
//! | a higher counter                             | none that is new                      | [`Standing::Behind`]    |
//! | a LOWER counter                              | none that is new                      | [`Standing::Rollback`]  |
//!
//! The announcement is looked at whatever the counter says (S3): another device that synced first
//! after the restore and committed on top of it makes the counters equal again. A drive synced
//! by name counts only this device's own commits ([`Clock::Own`], S5): its counter says nothing
//! about the drive, so only the epoch decides. The announcement is NEVER read from the bucket
//! (the storage is the adversary for a rollback): it comes from the token server's bundle only.
//!
//! After a restore the device's base is untrusted where the drive disagrees ([`rebase`]): a file
//! the restored drive lacks is sent again, a file it holds in another version than the base's is
//! kept as a conflict - never a download over an edit that may exist nowhere else (S2). A new or
//! reset pairing starts at the current epoch ([`baseline`]), so a stale announcement never
//! excuses a later unannounced rollback.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What kind of restore the token server announced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreKind {
    /// The server was rebuilt from its off-site copy (`azctl offsite restore`): the copy lags,
    /// so files committed after it are missing.
    #[default]
    Offsite,
    /// The drive's own restore of a prefix as of a moment (`point_in_time`): the user asked for
    /// the older state.
    PointInTime,
}

impl RestoreKind {
    /// The kind as the token server's answer names it (`offsite`, `point_in_time`); a word this
    /// version does not know is the lagging copy, the more careful of the two.
    #[must_use]
    pub fn from_wire(word: &str) -> RestoreKind {
        match word {
            "point_in_time" => RestoreKind::PointInTime,
            _ => RestoreKind::Offsite,
        }
    }

    /// The word of the token server's answer.
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            RestoreKind::Offsite => "offsite",
            RestoreKind::PointInTime => "point_in_time",
        }
    }
}

/// A restore of the drive (from its off-site copy, or of a folder as of a moment), announced by
/// the token server with an epoch one above the drive's last one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RestoreNotice {
    pub epoch: u64,
    /// The key prefix the restore covered; empty for the whole drive.
    #[serde(default)]
    pub prefix: String,
    /// Which restore it was.
    #[serde(default)]
    pub kind: RestoreKind,
    /// The moment the restored data is from (seconds since 1970): the copy's newest version for
    /// an off-site restore, the requested moment for a point-in-time one; `None` when unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<i64>,
}

impl RestoreNotice {
    /// Whether the restore covered the folder synced to `prefix`.
    #[must_use]
    pub fn covers(&self, prefix: &str) -> bool {
        self.prefix.is_empty()
            || prefix.starts_with(&self.prefix)
            || self.prefix.starts_with(prefix)
    }
}

/// What a device remembers of one drive between two looks at it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// The newest restore epoch the device accepted - or was born after ([`baseline`]).
    pub epoch: u64,
    /// The drive's counter at the last look: the index generation of a plain drive (shared by
    /// every device that commits), the manifest revision of a repository.
    pub clock: u64,
}

impl Seen {
    /// The memory of a pairing made now: the current epoch, nothing served yet.
    #[must_use]
    pub fn fresh(notice: Option<&RestoreNotice>) -> Seen {
        Seen {
            epoch: baseline(notice),
            clock: 0,
        }
    }
}

/// What the counter a drive serves counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    /// Every device that commits moves it (a plain drive's index generation, a repository's
    /// revision): older than the memory is a rollback.
    Shared,
    /// It counts this device's own commits only (a drive synced by name): it says nothing about
    /// the drive, so only the epoch decides.
    Own,
}

/// What the drive's state is, for a device that remembers [`Seen`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Standing {
    /// The drive is where the device last saw it.
    InSync,
    /// The drive moved on (other devices committed): the ordinary merge.
    Behind,
    /// The drive was restored and announced it: the device's memory of the drive is untrusted
    /// where the drive disagrees ([`rebase`]), and the epoch is accepted.
    Restored(RestoreNotice),
    /// The drive serves a state older than the device saw, and no announcement excuses it: a
    /// replayed or rolled-back state. Refused: nothing is changed anywhere.
    Rollback { seen: u64, served: u64 },
}

/// What the drive's state is. `served` is the counter the drive serves now; `notice` the newest
/// announcement the token server verified; `prefix` the folder's key prefix. Total: every input
/// has one answer (the table in the module's documentation).
#[must_use]
pub fn judge(
    seen: &Seen,
    served: u64,
    clock: Clock,
    notice: Option<&RestoreNotice>,
    prefix: &str,
) -> Standing {
    if let Some(notice) = notice.filter(|n| n.epoch > seen.epoch && n.covers(prefix)) {
        return Standing::Restored(notice.clone());
    }
    match clock {
        Clock::Own => Standing::InSync,
        Clock::Shared => match served.cmp(&seen.clock) {
            std::cmp::Ordering::Less => Standing::Rollback {
                seen: seen.clock,
                served,
            },
            std::cmp::Ordering::Equal => Standing::InSync,
            std::cmp::Ordering::Greater => Standing::Behind,
        },
    }
}

/// Whether the counter the drive serves is older than the memory's (never for an own-counter
/// drive, whose counter says nothing about the drive). A restore that went back in the counter
/// says so to the user; a restore announced after other devices committed on top of it does not.
#[must_use]
pub fn went_back(seen: &Seen, served: u64, clock: Clock) -> bool {
    clock == Clock::Shared && served < seen.clock
}

/// The epoch a new or reset pairing starts at: the current one, so an announcement that was made
/// before the pairing existed never excuses a rollback after it.
#[must_use]
pub fn baseline(notice: Option<&RestoreNotice>) -> u64 {
    notice.map_or(0, |n| n.epoch)
}

/// How a restored drive's state relates to one entry of a device's base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remote {
    /// The drive holds the file as the base has it - or the entry holds no bytes on this device
    /// (a new version of it on the drive is an ordinary download).
    Same,
    /// The drive holds the file in another version than the base's: this device's copy may be an
    /// edit that exists nowhere else.
    Other,
    /// The drive lacks the file and records no delete of it: the restore lost it.
    Lost,
    /// The drive records the file's delete: the ordinary rule for a file deleted elsewhere.
    Deleted,
}

/// A base after a restore, and what was dropped from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rebased<B> {
    /// The base the next merge works from: every entry the drive agrees with, or deleted.
    pub trusted: BTreeMap<String, B>,
    /// The keys the restore lost: with no base entry the merge finds a file only here, and sends
    /// it again.
    pub lost: Vec<String>,
    /// The keys the drive holds in another version: with no base entry the merge finds both
    /// sides different, and keeps both (a conflict copy), never a download over this device's
    /// copy.
    pub edited: Vec<String>,
}

/// The base after a restore (pure: `base` is not changed). `relation` says, per entry, how the
/// restored drive relates to it. Nothing is ever deleted by this: a dropped entry only makes the
/// next merge see the file as new, so the files are united by content, never overwritten.
#[must_use]
pub fn rebase<B: Clone>(
    base: &BTreeMap<String, B>,
    relation: &dyn Fn(&str, &B) -> Remote,
) -> Rebased<B> {
    let mut out = Rebased {
        trusted: BTreeMap::new(),
        lost: Vec::new(),
        edited: Vec::new(),
    };
    for (key, entry) in base {
        match relation(key, entry) {
            Remote::Same | Remote::Deleted => {
                out.trusted.insert(key.clone(), entry.clone());
            }
            Remote::Lost => out.lost.push(key.clone()),
            Remote::Other => out.edited.push(key.clone()),
        }
    }
    out
}
