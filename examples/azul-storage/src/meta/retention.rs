//! How long a drive keeps what it no longer shows (ARCH28 RF44 / AR5a-5; X-AR5a-21).
//!
//! Retention had four owners that disagreed - the UI's restore window (14 days), the packs' grace
//! (30 days), the sync engine's trash (30 days) and the index's own garbage grace (a day) - so a
//! file the user could see in the history had lost its bytes, or the other way round. The two
//! spans a user is promised are here; the window of the UI, the grace of a retired pack and the
//! reclaimer read them from this one place.
//!
//! The index's own garbage grace ([`super::wal::Maintenance::grace`]) is something else: how long
//! an object of the repository itself, no manifest names, waits for a slow reader. It is not a
//! promise to the user.

use std::time::Duration;

/// The spans a drive keeps what it released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retention {
    /// How long a deleted file stays in the trash, and how far back a drive whose node keeps the
    /// replaced objects itself (a plain drive) can be restored.
    pub trash: Duration,
    /// How long an encrypted drive keeps the versions of its files: an object the drive's history
    /// no longer names in its head stays in the bucket until its last commit is this old. Neither
    /// span counts toward the quota (owner's decision of 2026-10-24).
    pub versions: Duration,
}

/// What the drives keep: the trash 14 days, the versions 90 days (the owner's decision (a)).
pub const RETENTION: Retention = Retention {
    trash: Duration::from_secs(14 * 86_400),
    versions: Duration::from_secs(90 * 86_400),
};

impl Retention {
    /// The trash span in whole days.
    #[must_use]
    pub const fn trash_days(self) -> u64 {
        self.trash.as_secs() / 86_400
    }

    /// The versions span in whole days.
    #[must_use]
    pub const fn versions_days(self) -> u64 {
        self.versions.as_secs() / 86_400
    }
}
